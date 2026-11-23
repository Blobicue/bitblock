//! Wave 15C integration suite for `VALIDATOR-SCHEDULE-1` (`PS-055`):
//! verifies 2-epoch activation delay, immutable historical schedules & proofs, `CE-15` economic
//! coupling (jailed/low-stake exclusion), simultaneous join/exit/slash/key-rotation transitions,
//! strict `< 1/3` (`<= 3,333 bps`) concentration enforcement, Safe-Halt scheduling refusal,
//! retroactive rewrite blocking, actor/validator key-domain disjointness, and Domain 7 isolation.

#![cfg(feature = "protocol-research")]

use std::collections::BTreeSet;

use bitblock::asset_d1::ValidatorEconPolicyV1;
use bitblock::auth_commit_1::{AuthCommit1Error, DomainIdV1};
use bitblock::bounds_1::SafeHaltEvidenceV1;
use bitblock::consensus_gov_d1::{ConsensusPolicyDescriptorV1, GovernancePolicyDescriptorV1};
use bitblock::crypto_d1::CanonicalEd25519PublicKeyV1;
use bitblock::economic_state_1::EconomicStateV1;
use bitblock::governance_1::{GovernanceContextV1, GovernanceStateV1};
use bitblock::identity_d1::{ActorIdV1, CredentialRoleV1, ThresholdPolicyV1};
use bitblock::validator_schedule_1::{
    EpochValidatorScheduleV1, ValidatorKeyPairV1, ValidatorSchedule1Error,
    ValidatorScheduleStateV1, epoch_trie_key,
};

fn pk(seed: u8) -> CanonicalEd25519PublicKeyV1 {
    CanonicalEd25519PublicKeyV1::from_secret_seed(&[seed; 32]).unwrap()
}

fn setup() -> (
    EconomicStateV1,
    GovernanceStateV1,
    ValidatorScheduleStateV1,
    BTreeSet<CanonicalEd25519PublicKeyV1>,
) {
    let econ_policy = ValidatorEconPolicyV1::v1_pos_default();
    let cons_policy = ConsensusPolicyDescriptorV1::v1_bft_pos_default();
    let profile_id = [0x77; 32];
    let stake = 100_000_000_000u128;
    let init_stakes: Vec<_> = (1u8..=4)
        .map(|i| (ActorIdV1::from_bytes([i; 32]), stake))
        .collect();
    let econ = EconomicStateV1::from_genesis(econ_policy, profile_id, &init_stakes).unwrap();

    let mut c_keys = vec![pk(81), pk(82), pk(83)];
    c_keys.sort();
    let gov_policy = GovernancePolicyDescriptorV1 {
        schema_version: 1,
        upgrade_quorum_numerator: 2,
        upgrade_quorum_denominator: 3,
        mandatory_exit_timelock_epochs: 21,
        max_emergency_pause_epochs: 3,
        automatic_safe_halt_on_proven_fault: true,
        emergency_council_pause_only: true,
        forbid_history_rewrite: true,
        emergency_council: ThresholdPolicyV1 {
            role: CredentialRoleV1::EmergencyPauseSigner,
            threshold: 2,
            keys: c_keys,
        },
    };
    let gov_ctx = GovernanceContextV1 {
        protocol_version: 1,
        chain_id: 77,
        genesis_id: [0x11; 32],
        launch_manifest_id: [0x22; 32],
        profile_id,
    };
    let gov = GovernanceStateV1::from_genesis(gov_ctx, gov_policy, &[(1, 50_000_000)]).unwrap();
    let init_keys: Vec<_> = (1u8..=4)
        .map(|i| {
            (
                ActorIdV1::from_bytes([i; 32]),
                ValidatorKeyPairV1 {
                    vote_key: pk(10 + i),
                    proposal_key: pk(20 + i),
                },
            )
        })
        .collect();
    let mut forbidden = BTreeSet::new();
    forbidden.insert(pk(200)); // ordinary actor key

    let sched = ValidatorScheduleStateV1::from_genesis(
        cons_policy,
        econ_policy,
        profile_id,
        &econ,
        &gov,
        &init_keys,
        &forbidden,
    )
    .unwrap();
    (econ, gov, sched, forbidden)
}

#[test]
fn validator_schedule_1_delayed_activation_join_slash_rotation_and_historical_proofs() {
    let (mut econ, gov, mut sched, forbidden) = setup();
    let stake = 100_000_000_000u128;
    let v1 = ActorIdV1::from_bytes([1; 32]);
    let v2 = ActorIdV1::from_bytes([2; 32]);
    let v3 = ActorIdV1::from_bytes([3; 32]);
    let v5 = ActorIdV1::from_bytes([5; 32]);
    let v6 = ActorIdV1::from_bytes([6; 32]);

    // 1. At source_epoch=0: V5 & V6 join, V1 bonds +50B -> takes effect strictly at epoch 2!
    econ.bond_stake(v5, stake, 0).unwrap();
    econ.bond_stake(v6, stake, 0).unwrap();
    econ.bond_stake(v1, 50_000_000_000, 0).unwrap();
    sched
        .register_validator_keys(
            v5,
            ValidatorKeyPairV1 {
                vote_key: pk(15),
                proposal_key: pk(25),
            },
            &forbidden,
        )
        .unwrap();
    sched
        .register_validator_keys(
            v6,
            ValidatorKeyPairV1 {
                vote_key: pk(16),
                proposal_key: pk(26),
            },
            &forbidden,
        )
        .unwrap();
    let ep2 = sched
        .schedule_epoch_from_sources(&econ, &gov, 0, &forbidden)
        .unwrap();
    assert_eq!(ep2.effective_epoch, 2);
    assert_eq!(ep2.validators.len(), 6);
    assert_eq!(sched.schedule_at(0).unwrap().validators.len(), 4);
    assert_eq!(sched.schedule_at(1).unwrap().validators.len(), 4);

    // 2. At source_epoch=1: V3 equivocates & is jailed -> excluded at effective_epoch=3,
    //    and V1 (150B out of 550B = 2,727 bps) is deterministically policy-capped <= 2,500 bps!
    econ.apply_equivocation_slash(v3, [0xE1; 32], 1).unwrap();
    let ep3 = sched
        .schedule_epoch_from_sources(&econ, &gov, 1, &forbidden)
        .unwrap();
    assert_eq!(ep3.effective_epoch, 3);
    assert_eq!(ep3.validators.len(), 5);
    assert!(!ep3.validators.iter().any(|e| e.validator_id == v3));
    assert!(ep3.max_single_validator_bps <= 2_500);

    // 3. At source_epoch=2: V2 queues key rotation & V6 exits (unbonds 50B -> below min_self_stake)
    //    -> takes effect strictly at effective_epoch=4! Epoch 3 still has V2's old vote key!
    let eff_rot = sched
        .queue_key_rotation(
            v2,
            ValidatorKeyPairV1 {
                vote_key: pk(42),
                proposal_key: pk(52),
            },
            2,
            &forbidden,
        )
        .unwrap();
    assert_eq!(eff_rot, 4);
    econ.request_unbond(v6, 50_000_000_000, 2).unwrap();
    econ.request_unbond(v1, 30_000_000_000, 2).unwrap(); // V1: 120B (2,857 bps < 3,333)
    let ep4 = sched
        .schedule_epoch_from_sources(&econ, &gov, 2, &forbidden)
        .unwrap();
    assert_eq!(ep4.effective_epoch, 4);
    assert_eq!(ep4.validators.len(), 4);
    assert_eq!(
        ep4.validators
            .iter()
            .find(|e| e.validator_id == v2)
            .unwrap()
            .vote_key,
        pk(42)
    );
    assert_eq!(
        sched
            .schedule_at(3)
            .unwrap()
            .validators
            .iter()
            .find(|e| e.validator_id == v2)
            .unwrap()
            .vote_key,
        pk(12)
    );

    // 4. Historical epoch proofs across epochs 0..=4 & Domain 7 cross-domain isolation
    sched.advance_current_epoch(4).unwrap();
    let root = sched.root_commitment().unwrap();
    for ep in 0u64..=4 {
        let proof = sched.prove_epoch(ep).unwrap();
        let raw = proof
            .verify(
                DomainIdV1::ValidatorSchedule,
                root.schedule_trie_root,
                &epoch_trie_key(ep),
            )
            .unwrap()
            .expect("epoch present");
        let dec = EpochValidatorScheduleV1::decode_canonical(&raw).unwrap();
        assert_eq!(dec, *sched.schedule_at(ep).unwrap());
    }
    let p0 = sched.prove_epoch(0).unwrap();
    assert!(matches!(
        p0.verify(
            DomainIdV1::EconomicStake,
            root.schedule_trie_root,
            &epoch_trie_key(0),
        ),
        Err(AuthCommit1Error::CrossDomainProofRejected { .. })
    ));
    sched.verify_rebuild_parity().unwrap();
}

#[test]
fn validator_schedule_1_adversarial_rewrite_concentration_safehalt_and_key_guards() {
    let (mut econ, mut gov, mut sched, forbidden) = setup();
    let v1 = ActorIdV1::from_bytes([1; 32]);
    let v5 = ActorIdV1::from_bytes([5; 32]);

    // 1. Actor/validator key-domain collision rejected fail-closed
    assert!(matches!(
        sched.register_validator_keys(
            v5,
            ValidatorKeyPairV1 {
                vote_key: pk(200), // actor key!
                proposal_key: pk(61),
            },
            &forbidden,
        ),
        Err(ValidatorSchedule1Error::RejectedKeyDomainCollision(_))
    ));

    // 2. Retroactive schedule rewrite for already-committed epoch 2 rejected after scheduling epoch 2
    let _ = sched
        .schedule_epoch_from_sources(&econ, &gov, 0, &forbidden)
        .unwrap();
    assert!(matches!(
        sched.schedule_epoch_from_sources(&econ, &gov, 0, &forbidden),
        Err(ValidatorSchedule1Error::RejectedRetroactiveScheduleRewrite { effective_epoch: 2 })
    ));

    // 3. Non-contiguous jump (source_epoch=5 -> effective 7 skipping 3..6) rejected
    assert!(matches!(
        sched.schedule_epoch_from_sources(&econ, &gov, 5, &forbidden),
        Err(ValidatorSchedule1Error::RejectedNonContiguousEpoch { .. })
    ));

    // 4. BFT < 1/3 concentration breach: V1 bonds +300B (400B of 700B = 5,714 bps > 3,333) -> REJECT!
    econ.bond_stake(v1, 300_000_000_000, 1).unwrap();
    assert!(matches!(
        sched.schedule_epoch_from_sources(&econ, &gov, 1, &forbidden),
        Err(ValidatorSchedule1Error::RejectedConcentrationViolation { max_bps: 5714 })
    ));

    // 5. Safe-Halted governance MUST block scheduling any new epoch!
    let sh = SafeHaltEvidenceV1 {
        schema_version: 1,
        bounds_profile_digest: [0x77; 32],
        artifact_digest: [0x88; 32],
        violation_code: 3,
        observed_value: 20_000,
        committed_limit: 12_288,
    };
    gov.trigger_safe_halt_from_bounds_evidence(&sh, 1).unwrap();
    assert!(matches!(
        sched.schedule_epoch_from_sources(&econ, &gov, 1, &forbidden),
        Err(ValidatorSchedule1Error::RejectedStaleOrHaltedSource(_))
    ));
    sched.verify_rebuild_parity().unwrap();
}
