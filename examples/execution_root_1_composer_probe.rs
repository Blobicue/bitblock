//! Release composer & verification benchmark probe for Wave 17A (`EXECUTION-ROOT-1`, `PS-030`).

use std::time::Instant;

use bitblock::asset_d1::{AssetPolicyDescriptorV1, ValidatorEconPolicyV1};
use bitblock::bounds_1::ProtocolBoundsProfileV1;
use bitblock::commit_state::DomainRegistryV1;
use bitblock::consensus_gov_d1::GovernancePolicyDescriptorV1;
use bitblock::crypto_d1::{CanonicalEd25519PublicKeyV1, CryptoSuiteDescriptorV1};
use bitblock::execution_root_1::{
    ExecutionDomainRegistryV1, ExecutionRootPreimageContextV1, compose_from_executor_snapshot,
    verify_header_and_manifest_correspondence,
};
use bitblock::executor_1::ExecutorDomainSnapshotV1;
use bitblock::identity_d1::{CredentialRoleV1, ThresholdPolicyV1};
use bitblock::local_bounds::LocalStorageProfile;
use bitblock::profile_bootstrap::ProfileScopeV1;
use bitblock::profile_d1::{BoundProfileManifestV1, CandidateProfileManifestV1};
use bitblock::protocol::ContentTag;
use bitblock::resource_1::ResourcePolicyV1;
use bitblock::threat_1::ThreatModelV1;

fn main() {
    let reg = ExecutionDomainRegistryV1::canonical_launch_v1();
    let snap0 = ExecutorDomainSnapshotV1 {
        snapshot_version: 1,
        batch_height: 0,
        epoch: 0,
        world_root: [0x11; 32],
        active_token_root: [0x22; 32],
        episode_root: [0x33; 32],
        auth_root: [0x44; 32],
        nonce_root: [0x55; 32],
        native_balance_root: [0x66; 32],
        episode_ownership_root: [0x77; 32],
        governance_root: [0x88; 32],
        economic_stake_root: [0x99; 32],
        validator_schedule_root: [0xAA; 32],
        program_state_active: false,
    };
    let pctx = ExecutionRootPreimageContextV1 {
        protocol_version: 1,
        chain_id: 77,
        world_id: 1,
        genesis_id: [0xB1; 32],
        profile_scope_digest: [0xC2; 32],
    };
    let comp1 = compose_from_executor_snapshot(&snap0, &reg, pctx, [0xD3; 32]).unwrap();
    let comp2 = compose_from_executor_snapshot(&snap0, &reg, pctx, [0xE4; 32]).unwrap();

    // Real BoundProfileManifestV1 for header verification timings
    let mut keys: Vec<_> = [31u8, 32, 33]
        .into_iter()
        .map(|s| CanonicalEd25519PublicKeyV1::from_secret_seed(&[s; 32]).unwrap())
        .collect();
    keys.sort();
    let gov = GovernancePolicyDescriptorV1 {
        schema_version: 1,
        upgrade_quorum_numerator: 2,
        upgrade_quorum_denominator: 3,
        mandatory_exit_timelock_epochs: ValidatorEconPolicyV1::v1_pos_default()
            .unbonding_delay_epochs,
        max_emergency_pause_epochs: 3,
        automatic_safe_halt_on_proven_fault: true,
        emergency_council_pause_only: true,
        forbid_history_rewrite: true,
        emergency_council: ThresholdPolicyV1 {
            role: CredentialRoleV1::EmergencyPauseSigner,
            threshold: 2,
            keys,
        },
    };
    let preds = "asset-active economic-charging stake-consensus validator-economics"
        .split_ascii_whitespace()
        .map(str::to_string)
        .collect();
    let scope =
        ProfileScopeV1::from_wave1_contracts(&DomainRegistryV1::local_v1(), &gov, preds).unwrap();
    let threat = ThreatModelV1::v1_conservative_consumer(&scope).unwrap();
    let bounds =
        ProtocolBoundsProfileV1::v1_consumer_default(&threat, &LocalStorageProfile::DEFAULT)
            .unwrap();
    let resource =
        ResourcePolicyV1::v1_default(&bounds, &AssetPolicyDescriptorV1::v1_dual_domain_default())
            .unwrap();
    let genesis_d = ContentTag::parse_full(
        "#bb.policy.sha256.jowjx6wyiayipiaypxgmjc2xgynt335d7mrx3qfx2e6pxegyshgq",
    )
    .unwrap()
    .digest();
    let scope_d = scope.tag().unwrap().digest();
    let launch_bound = BoundProfileManifestV1::seal(
        CandidateProfileManifestV1::canonical_launch_v1(
            genesis_d,
            scope_d,
            DomainRegistryV1::local_v1().tag().digest(),
            CryptoSuiteDescriptorV1::v1_ed25519_sha256()
                .tag()
                .unwrap()
                .digest(),
            bounds.tag().unwrap().digest(),
            resource.tag().unwrap().digest(),
        )
        .unwrap(),
    )
    .unwrap();
    let real_ctx = ExecutionRootPreimageContextV1 {
        protocol_version: 1,
        chain_id: 77,
        world_id: 1,
        genesis_id: genesis_d,
        profile_scope_digest: scope_d,
    };
    let mut snap1 = snap0;
    snap1.batch_height = 1;
    snap1.world_root[0] ^= 0x55;
    let real_comp =
        compose_from_executor_snapshot(&snap1, &reg, real_ctx, launch_bound.profile_id).unwrap();

    let mut t_gen = Vec::with_capacity(200);
    let mut t_post = Vec::with_capacity(200);
    let mut t_ver = Vec::with_capacity(200);
    for _ in 0..200 {
        let a = Instant::now();
        let _ = compose_from_executor_snapshot(&snap0, &reg, real_ctx, launch_bound.profile_id)
            .unwrap();
        t_gen.push(a.elapsed().as_secs_f64() * 1e6);

        let b = Instant::now();
        let _ = compose_from_executor_snapshot(&snap1, &reg, real_ctx, launch_bound.profile_id)
            .unwrap();
        t_post.push(b.elapsed().as_secs_f64() * 1e6);

        let c = Instant::now();
        verify_header_and_manifest_correspondence(
            &real_comp,
            &real_comp.execution_root_ref,
            &reg,
            &launch_bound,
        )
        .unwrap();
        t_ver.push(c.elapsed().as_secs_f64() * 1e6);
    }
    t_gen.sort_by(f64::total_cmp);
    t_post.sort_by(f64::total_cmp);
    t_ver.sort_by(f64::total_cmp);

    println!("REG_TAG={}", reg.tag().unwrap());
    println!("COMP1_TAG={}", comp1.tag().unwrap());
    println!("COMP2_TAG={}", comp2.tag().unwrap());
    println!("GEN_P50_US={:.2}", t_gen[100]);
    println!("POST_P50_US={:.2}", t_post[100]);
    println!("VERIFY_P50_US={:.2}", t_ver[100]);
}
