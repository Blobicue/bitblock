//! Wave 7 CI integration tests for `RESOURCE-1` (`PS-039`, `CE-08`, `CE-14`).
#![cfg(feature = "protocol-research")]

use std::collections::BTreeMap;

use bitblock::asset_d1::AssetPolicyDescriptorV1;
use bitblock::bounds_1::ProtocolBoundsProfileV1;
use bitblock::commit_state::DomainRegistryV1;
use bitblock::consensus_gov_d1::GovernancePolicyDescriptorV1;
use bitblock::crypto_d1::CanonicalEd25519PublicKeyV1;
use bitblock::identity_d1::{ActorIdV1, CredentialRoleV1, ThresholdPolicyV1};
use bitblock::local_bounds::LocalStorageProfile;
use bitblock::profile_bootstrap::ProfileScopeV1;
use bitblock::resource_1::{
    Resource1Error, ResourceAccountingLedgerV1, ResourcePolicyV1, TxResourceRequestV1,
    TxResourceWorkV1,
};
use bitblock::threat_1::ThreatModelV1;
use bitblock::world::{BEDROCK_COORD, Coord};

const EXPECTED_RESOURCE_POLICY_TAG: &str =
    "#bb.policy.sha256.6yvp73mvdkouoxk2xiolnj33lbxucxta5ikhjcwnvd267tzfrfyq";
const EXPECTED_SETTLEMENT_TAG: &str =
    "#bb.receipt.sha256.5n46g22vwa6oby2idk57kterg5sra3rx6y5xrf4hbjkaqgm6b7fa";

fn canonical_policy() -> ResourcePolicyV1 {
    let dom = DomainRegistryV1::local_v1();
    let mut ckeys = [31u8, 32, 33]
        .into_iter()
        .map(|s| CanonicalEd25519PublicKeyV1::from_secret_seed(&[s; 32]).unwrap())
        .collect::<Vec<_>>();
    ckeys.sort();
    let council = ThresholdPolicyV1 {
        role: CredentialRoleV1::EmergencyPauseSigner,
        threshold: 2,
        keys: ckeys,
    };
    let gov = GovernancePolicyDescriptorV1 {
        schema_version: 1,
        upgrade_quorum_numerator: 2,
        upgrade_quorum_denominator: 3,
        mandatory_exit_timelock_epochs: 21,
        max_emergency_pause_epochs: 3,
        automatic_safe_halt_on_proven_fault: true,
        emergency_council_pause_only: true,
        forbid_history_rewrite: true,
        emergency_council: council,
    };
    let scope = ProfileScopeV1::from_wave1_contracts(
        &dom,
        &gov,
        vec![
            "asset-active".to_string(),
            "economic-charging".to_string(),
            "stake-consensus".to_string(),
            "validator-economics".to_string(),
        ],
    )
    .unwrap();
    let threat = ThreatModelV1::v1_conservative_consumer(&scope).unwrap();
    let bounds =
        ProtocolBoundsProfileV1::v1_consumer_default(&threat, &LocalStorageProfile::DEFAULT)
            .unwrap();
    let asset = AssetPolicyDescriptorV1::v1_dual_domain_default();
    ResourcePolicyV1::v1_default(&bounds, &asset).unwrap()
}

#[test]
fn wave7_resource_1_golden_tags_and_adversarial_controls() {
    let policy = canonical_policy();
    assert_eq!(
        policy.tag().unwrap().to_string(),
        EXPECTED_RESOURCE_POLICY_TAG
    );

    // Golden block settlement matching tools/resource_1_vectors.py bit-for-bit.
    let alice = ActorIdV1::from_bytes([0x11; 32]);
    let bob = ActorIdV1::from_bytes([0x22; 32]);
    let proposer = ActorIdV1::from_bytes([0x99; 32]);
    let mut ledger = ResourceAccountingLedgerV1::new(
        BTreeMap::from([
            (alice, 500_000_000_000u128),
            (bob, 500_000_000_000u128),
            (proposer, 0u128),
        ]),
        100,
    )
    .unwrap();

    let s1 = ledger
        .settle_block(
            &policy,
            1,
            proposer,
            &[
                TxResourceRequestV1 {
                    payer: alice,
                    max_fee_per_gas: 115,
                    max_priority_tip_per_gas: 50, // capped to headroom = 15
                    work: TxResourceWorkV1 {
                        tx_bytes: 512,
                        ed25519_signatures: 2,
                        state_writes: 4,
                        proof_bytes: 512,
                        occupancy_ops: 4,
                    },
                    execution_succeeded: true,
                    minted_coords: vec![
                        Coord::new(1, 0, 0),
                        Coord::new(2, 0, 0),
                        Coord::new(3, 0, 0),
                        Coord::new(4, 0, 0),
                    ],
                    burned_coords: vec![],
                },
                TxResourceRequestV1 {
                    payer: bob,
                    max_fee_per_gas: 150,
                    max_priority_tip_per_gas: 10,
                    work: TxResourceWorkV1 {
                        tx_bytes: 256,
                        ed25519_signatures: 1,
                        state_writes: 2,
                        proof_bytes: 256,
                        occupancy_ops: 2,
                    },
                    execution_succeeded: false, // admitted failure: burned fee + tip still billed!
                    minted_coords: vec![Coord::new(9, 9, 9)],
                    burned_coords: vec![],
                },
            ],
        )
        .unwrap();
    assert_eq!(s1.tag().to_string(), EXPECTED_SETTLEMENT_TAG);
    assert_eq!(s1.net_locked_deposit_delta, 4_000_000);
    assert_eq!(ledger.locked_deposits.len(), 4);
    ledger.verify_conservation().unwrap();

    // Exact deposit refund on burn + conservation across block 2.
    let s2 = ledger
        .settle_block(
            &policy,
            2,
            proposer,
            &[TxResourceRequestV1 {
                payer: alice,
                max_fee_per_gas: 200,
                max_priority_tip_per_gas: 5,
                work: TxResourceWorkV1 {
                    tx_bytes: 128,
                    ed25519_signatures: 1,
                    state_writes: 2,
                    proof_bytes: 128,
                    occupancy_ops: 2,
                },
                execution_succeeded: true,
                minted_coords: vec![],
                burned_coords: vec![Coord::new(1, 0, 0), Coord::new(2, 0, 0)],
            }],
        )
        .unwrap();
    assert_eq!(s2.net_locked_deposit_delta, -2_000_000);
    assert_eq!(ledger.locked_deposits.len(), 2);
    ledger.verify_conservation().unwrap();

    // Negative controls: bedrock deposit forbidden, non-owner burn rejected, max_fee < base_fee rejected,
    // block gas overflow rejected, and state_rent / burn_base_fee tampering rejected.
    let snapshot = ledger.clone();
    let bedrock_err = ledger
        .settle_block(
            &policy,
            3,
            proposer,
            &[TxResourceRequestV1 {
                payer: alice,
                max_fee_per_gas: 200,
                max_priority_tip_per_gas: 1,
                work: TxResourceWorkV1 {
                    tx_bytes: 64,
                    ed25519_signatures: 1,
                    state_writes: 1,
                    proof_bytes: 0,
                    occupancy_ops: 1,
                },
                execution_succeeded: true,
                minted_coords: vec![BEDROCK_COORD],
                burned_coords: vec![],
            }],
        )
        .unwrap_err();
    assert_eq!(bedrock_err, Resource1Error::BedrockDepositForbidden);
    assert_eq!(
        ledger, snapshot,
        "failed settlement must leave ledger untouched"
    );

    let non_owner_err = ledger
        .settle_block(
            &policy,
            3,
            proposer,
            &[TxResourceRequestV1 {
                payer: bob,
                max_fee_per_gas: 200,
                max_priority_tip_per_gas: 1,
                work: TxResourceWorkV1 {
                    tx_bytes: 64,
                    ed25519_signatures: 1,
                    state_writes: 1,
                    proof_bytes: 0,
                    occupancy_ops: 1,
                },
                execution_succeeded: true,
                minted_coords: vec![],
                burned_coords: vec![Coord::new(3, 0, 0)],
            }],
        )
        .unwrap_err();
    assert!(matches!(
        non_owner_err,
        Resource1Error::DepositStateViolation(_)
    ));
    assert_eq!(ledger, snapshot);

    let low_fee_err = ledger
        .settle_block(
            &policy,
            3,
            proposer,
            &[TxResourceRequestV1 {
                payer: alice,
                max_fee_per_gas: 1,
                max_priority_tip_per_gas: 0,
                work: TxResourceWorkV1 {
                    tx_bytes: 64,
                    ed25519_signatures: 1,
                    state_writes: 0,
                    proof_bytes: 0,
                    occupancy_ops: 0,
                },
                execution_succeeded: true,
                minted_coords: vec![],
                burned_coords: vec![],
            }],
        )
        .unwrap_err();
    assert!(matches!(
        low_fee_err,
        Resource1Error::MaxFeeBelowBaseFee { .. }
    ));

    let mut bad_pol = policy;
    bad_pol.state_rent_enabled = true;
    assert!(bad_pol.validate(None, None).is_err());
    let mut bad_pol2 = policy;
    bad_pol2.burn_base_fee = false;
    assert!(bad_pol2.validate(None, None).is_err());
}
