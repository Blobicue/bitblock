//! Wave 8A CI integration tests for `GENESIS-D1` (`PS-078`).
#![cfg(feature = "protocol-research")]

use sha2::{Digest, Sha256};

use bitblock::asset_d1::{AssetPolicyDescriptorV1, ValidatorEconPolicyV1};
use bitblock::bounds_1::ProtocolBoundsProfileV1;
use bitblock::commit_state::DomainRegistryV1;
use bitblock::consensus_gov_d1::GovernancePolicyDescriptorV1;
use bitblock::crypto_d1::CanonicalEd25519PublicKeyV1;
use bitblock::genesis_d1::{
    BootstrapAvailabilityPolicyV1, CANONICAL_LAUNCH_PREDICATES_V1, DEVNET_V1_IDENTITY_NAMESPACE,
    GenesisActorEntryV1, GenesisD1Error, GenesisDomainSlotV1, GenesisPackageV1,
    GenesisValidatorEntryV1, LaunchGenesisStateV1,
};
use bitblock::history::world_tag;
use bitblock::identity_d1::{ActorIdV1, CredentialRoleV1, ThresholdPolicyV1};
use bitblock::local_bounds::LocalStorageProfile;
use bitblock::profile_bootstrap::ProfileScopeV1;
use bitblock::resource_1::ResourcePolicyV1;
use bitblock::threat_1::ThreatModelV1;
use bitblock::world::World;

const EXPECTED_BOOTSTRAP_TAG: &str =
    "#bb.policy.sha256.c6rrz6ronxsgc7unwdubse4epp7xobqqu4jkzgnghkpel3ozydta";
const EXPECTED_VECTOR_LAUNCH_STATE_TAG: &str =
    "#bb.policy.sha256.yzeh72ixpeuu4jyvt3uakqtznxkhmdqbeigcmwl4jlhrsp2dk6va";
const EXPECTED_VECTOR_PACKAGE_TAG: &str =
    "#bb.receipt.sha256.adu7bin3lwr47yecbamuvl6zjiy5qxes46dtvwkm3zfwuzmm3rsq";
const EXPECTED_FULL_DEVNET_STATE_TAG: &str =
    "#bb.policy.sha256.jowjx6wyiayipiaypxgmjc2xgynt335d7mrx3qfx2e6pxegyshgq";
const EXPECTED_FULL_DEVNET_PACKAGE_TAG: &str =
    "#bb.receipt.sha256.opfwpshgxba5nu5y4jvuorzlgigbvklier5dymchaauouhvivvgq";

fn sha256_bytes(msg: &[u8]) -> [u8; 32] {
    Sha256::digest(msg).into()
}

#[test]
fn wave8a_genesis_d1_golden_parity_and_adversarial_controls() {
    // 1. Exact bit-for-bit parity with tools/genesis_d1_vectors.py.
    let boot = BootstrapAvailabilityPolicyV1::v1_default();
    assert_eq!(boot.tag().unwrap().to_string(), EXPECTED_BOOTSTRAP_TAG);

    let dom = DomainRegistryV1::local_v1();
    let mut ckeys = [31u8, 32, 33]
        .into_iter()
        .map(|s| CanonicalEd25519PublicKeyV1::from_secret_seed(&[s; 32]).unwrap())
        .collect::<Vec<_>>();
    ckeys.sort();
    let gov = GovernancePolicyDescriptorV1 {
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
            keys: ckeys,
        },
    };
    let scope_4 = ProfileScopeV1::from_wave1_contracts(
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
    let threat_4 = ThreatModelV1::v1_conservative_consumer(&scope_4).unwrap();
    let bounds_4 =
        ProtocolBoundsProfileV1::v1_consumer_default(&threat_4, &LocalStorageProfile::DEFAULT)
            .unwrap();
    let asset = AssetPolicyDescriptorV1::v1_dual_domain_default();
    let econ = ValidatorEconPolicyV1::v1_pos_default();
    let res_4 = ResourcePolicyV1::v1_default(&bounds_4, &asset).unwrap();

    let vec_actors = vec![
        GenesisActorEntryV1 {
            actor_id: ActorIdV1::from_bytes([0x10; 32]),
            descriptor_digest: sha256_bytes(b"devnet-actor-1"),
            initial_nonce: 0,
            initial_liquid_balance: 100_000_000_000,
        },
        GenesisActorEntryV1 {
            actor_id: ActorIdV1::from_bytes([0x20; 32]),
            descriptor_digest: sha256_bytes(b"devnet-actor-2"),
            initial_nonce: 0,
            initial_liquid_balance: 100_000_000_000,
        },
    ];
    let validators: Vec<GenesisValidatorEntryV1> = (1u8..=4)
        .map(|i| GenesisValidatorEntryV1 {
            validator_id: [i; 32],
            vote_pubkey: CanonicalEd25519PublicKeyV1::from_secret_seed(&[100 + i; 32]).unwrap(),
            proposal_pubkey: CanonicalEd25519PublicKeyV1::from_secret_seed(&[200 + i; 32]).unwrap(),
            bonded_stake: 200_000_000_000,
        })
        .collect();

    let vec_state = LaunchGenesisStateV1 {
        genesis_schema_version: 1,
        initial_profile_scope_digest: [0xA1; 32],
        initial_domain_registry_digest: [0xA2; 32],
        chain_id: 1,
        world_id: 1,
        identity_namespace: DEVNET_V1_IDENTITY_NAMESPACE.to_string(),
        initial_world_digest: world_tag(&World::new()).digest(),
        initial_active_token_count: 0,
        initial_episode_count: 0,
        initial_locked_deposits: 0,
        resource_policy_digest: res_4.tag().unwrap().digest(),
        asset_policy_digest: asset.tag().unwrap().digest(),
        validator_econ_digest: econ.tag().unwrap().digest(),
        governance_policy_digest: [0xA6; 32],
        validated_limit_vector_digest: bounds_4.tag().unwrap().digest(),
        bootstrap_availability_digest: boot.tag().unwrap().digest(),
        launch_predicates: CANONICAL_LAUNCH_PREDICATES_V1
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        domain_entries: GenesisDomainSlotV1::canonical_launch_v1().to_vec(),
        initial_actors: vec_actors,
        initial_validators: validators.clone(),
        initial_native_supply: 1_000_000_000_000,
    };
    assert_eq!(
        vec_state.tag().unwrap().to_string(),
        EXPECTED_VECTOR_LAUNCH_STATE_TAG
    );
    let vec_pkg = GenesisPackageV1::seal(vec_state).unwrap();
    assert_eq!(
        vec_pkg.tag().unwrap().to_string(),
        EXPECTED_VECTOR_PACKAGE_TAG
    );

    // 2. Full end-to-end devnet launch package built from live Wave 1-7 contracts.
    let scope_7 = ProfileScopeV1::from_wave1_contracts(
        &dom,
        &gov,
        CANONICAL_LAUNCH_PREDICATES_V1
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
    )
    .unwrap();
    let threat_7 = ThreatModelV1::v1_conservative_consumer(&scope_7).unwrap();
    let bounds_7 =
        ProtocolBoundsProfileV1::v1_consumer_default(&threat_7, &LocalStorageProfile::DEFAULT)
            .unwrap();
    let res_7 = ResourcePolicyV1::v1_default(&bounds_7, &asset).unwrap();
    let full_actors = vec![
        GenesisActorEntryV1 {
            actor_id: ActorIdV1::from_bytes([0x10; 32]),
            descriptor_digest: [0x11; 32],
            initial_nonce: 0,
            initial_liquid_balance: 100_000_000_000,
        },
        GenesisActorEntryV1 {
            actor_id: ActorIdV1::from_bytes([0x20; 32]),
            descriptor_digest: [0x22; 32],
            initial_nonce: 0,
            initial_liquid_balance: 100_000_000_000,
        },
    ];
    let full_state = LaunchGenesisStateV1::build_launch_v1(
        &scope_7,
        &dom,
        &res_7,
        &asset,
        &econ,
        &gov,
        &bounds_7,
        &boot,
        1,
        DEVNET_V1_IDENTITY_NAMESPACE.to_string(),
        full_actors,
        validators,
    )
    .unwrap();
    assert_eq!(
        full_state.tag().unwrap().to_string(),
        EXPECTED_FULL_DEVNET_STATE_TAG
    );
    let full_pkg = GenesisPackageV1::seal(full_state.clone()).unwrap();
    assert_eq!(
        full_pkg.tag().unwrap().to_string(),
        EXPECTED_FULL_DEVNET_PACKAGE_TAG
    );
    let raw_pkg = full_pkg.encode_canonical().unwrap();
    assert_eq!(
        GenesisPackageV1::decode_canonical(&raw_pkg).unwrap(),
        full_pkg
    );

    // 3. Adversarial negative controls:
    // (a) Self-reference rejection
    let mut self_ref = full_pkg.clone();
    self_ref.state.initial_profile_scope_digest = full_pkg.genesis_id;
    assert_eq!(
        self_ref.verify(),
        Err(GenesisD1Error::SelfReferenceDetected)
    );

    // (b) Derived genesis_id mismatch
    let mut bad_id = full_pkg.clone();
    bad_id.genesis_id[0] ^= 1;
    assert_eq!(bad_id.verify(), Err(GenesisD1Error::GenesisIdMismatch));

    // (c) Forbidden activation of program-active or Verkle at genesis
    let mut bad_pred = full_state.clone();
    bad_pred
        .launch_predicates
        .push("program-active".to_string());
    assert!(bad_pred.validate(Some(&asset), Some(&econ)).is_err());

    let mut bad_dom = full_state.clone();
    bad_dom.domain_entries[6].active = true; // ProgramState forbidden at genesis
    assert!(bad_dom.validate(Some(&asset), Some(&econ)).is_err());

    // (d) > 1/3 validator stake concentration rejected
    let mut bad_stake = full_state.clone();
    bad_stake.initial_validators[0].bonded_stake = 500_000_000_000;
    bad_stake.initial_native_supply += 300_000_000_000;
    assert!(bad_stake.validate(Some(&asset), Some(&econ)).is_err());

    // (e) Unsorted actors / non-zero initial nonce / supply mismatch / duplicate vote=proposal key
    let mut bad_order = full_state.clone();
    bad_order.initial_actors.swap(0, 1);
    assert!(bad_order.validate(None, None).is_err());

    let mut bad_nonce = full_state.clone();
    bad_nonce.initial_actors[0].initial_nonce = 1;
    assert!(bad_nonce.validate(None, None).is_err());

    let mut bad_supply = full_state.clone();
    bad_supply.initial_native_supply += 1;
    assert!(bad_supply.validate(None, None).is_err());

    let mut bad_key = full_state;
    bad_key.initial_validators[0].proposal_pubkey = bad_key.initial_validators[0].vote_pubkey;
    assert!(bad_key.validate(None, None).is_err());

    // (f) Trailing wire bytes rejected
    let mut trailing = raw_pkg;
    trailing.push(0x00);
    assert!(GenesisPackageV1::decode_canonical(&trailing).is_err());
}
