//! Release DAG & pre-ID genesis verification probe for `GENESIS-D1` (`PS-078`).

use bitblock::asset_d1::{AssetPolicyDescriptorV1, ValidatorEconPolicyV1};
use bitblock::bounds_1::ProtocolBoundsProfileV1;
use bitblock::commit_state::DomainRegistryV1;
use bitblock::consensus_gov_d1::GovernancePolicyDescriptorV1;
use bitblock::crypto_d1::CanonicalEd25519PublicKeyV1;
use bitblock::genesis_d1::{
    BootstrapAvailabilityPolicyV1, CANONICAL_LAUNCH_PREDICATES_V1, DEVNET_V1_IDENTITY_NAMESPACE,
    GenesisActorEntryV1, GenesisPackageV1, GenesisValidatorEntryV1, LaunchGenesisStateV1,
};
use bitblock::identity_d1::{ActorIdV1, CredentialRoleV1, ThresholdPolicyV1};
use bitblock::local_bounds::LocalStorageProfile;
use bitblock::profile_bootstrap::ProfileScopeV1;
use bitblock::resource_1::ResourcePolicyV1;
use bitblock::threat_1::ThreatModelV1;

fn main() {
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
    let scope = ProfileScopeV1::from_wave1_contracts(
        &dom,
        &gov,
        CANONICAL_LAUNCH_PREDICATES_V1
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
    )
    .unwrap();
    let threat = ThreatModelV1::v1_conservative_consumer(&scope).unwrap();
    let bounds =
        ProtocolBoundsProfileV1::v1_consumer_default(&threat, &LocalStorageProfile::DEFAULT)
            .unwrap();
    let asset = AssetPolicyDescriptorV1::v1_dual_domain_default();
    let econ = ValidatorEconPolicyV1::v1_pos_default();
    let resource = ResourcePolicyV1::v1_default(&bounds, &asset).unwrap();
    let bootstrap = BootstrapAvailabilityPolicyV1::v1_default();

    let actors = vec![
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
    let validators = (1u8..=4)
        .map(|i| GenesisValidatorEntryV1 {
            validator_id: [i; 32],
            vote_pubkey: CanonicalEd25519PublicKeyV1::from_secret_seed(&[100 + i; 32]).unwrap(),
            proposal_pubkey: CanonicalEd25519PublicKeyV1::from_secret_seed(&[200 + i; 32]).unwrap(),
            bonded_stake: 200_000_000_000,
        })
        .collect::<Vec<_>>();

    let state = LaunchGenesisStateV1::build_launch_v1(
        &scope,
        &dom,
        &resource,
        &asset,
        &econ,
        &gov,
        &bounds,
        &bootstrap,
        1,
        DEVNET_V1_IDENTITY_NAMESPACE.to_string(),
        actors,
        validators,
    )
    .unwrap();
    let pkg = GenesisPackageV1::seal(state).unwrap();

    let total_stake: u128 = pkg
        .state
        .initial_validators
        .iter()
        .map(|v| v.bonded_stake)
        .sum();
    let total_liquid: u128 = pkg
        .state
        .initial_actors
        .iter()
        .map(|a| a.initial_liquid_balance)
        .sum();
    println!("{{");
    println!(
        "  \"bootstrap_policy_tag\": \"{}\",",
        bootstrap.tag().unwrap()
    );
    println!("  \"launch_state_tag\": \"{}\",", pkg.state.tag().unwrap());
    println!("  \"genesis_package_tag\": \"{}\",", pkg.tag().unwrap());
    println!(
        "  \"identity_namespace\": \"{}\",",
        pkg.state.identity_namespace
    );
    println!("  \"active_domains\": 6,");
    println!("  \"inactive_domains\": 2,");
    println!(
        "  \"validator_count\": {},",
        pkg.state.initial_validators.len()
    );
    println!("  \"total_liquid\": {total_liquid},");
    println!("  \"total_bonded_stake\": {total_stake},");
    println!(
        "  \"initial_native_supply\": {},",
        pkg.state.initial_native_supply
    );
    println!("  \"validator_shares_bps\": [2500, 2500, 2500, 2500],");
    println!("  \"max_single_validator_bps\": 3333");
    println!("}}");
}
