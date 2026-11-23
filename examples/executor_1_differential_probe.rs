//! Release differential & latency probe for Wave 16 (`EXECUTOR-1`, `PS-065`).

use std::collections::BTreeSet;
use std::time::Instant;

use bitblock::asset_d1::{AssetPolicyDescriptorV1, ValidatorEconPolicyV1};
use bitblock::bounds_1::ProtocolBoundsProfileV1;
use bitblock::commit_state::DomainRegistryV1;
use bitblock::consensus_gov_d1::{ConsensusPolicyDescriptorV1, GovernancePolicyDescriptorV1};
use bitblock::crypto_d1::{
    CanonicalEd25519PublicKeyV1, HashSuiteIdV1, SignatureSuiteIdV1, SigningDomainTagV1,
    sign_preimage_deterministic,
};
use bitblock::executor_1::{
    ExecutorBlockBatchV1, ExecutorForkContextV1, ExecutorGenesisConfigV1, ExecutorOpV1,
    PrimaryLaunchExecutorV1, verify_executor_receipt_against_spec,
};
use bitblock::governance_1::GovernanceContextV1;
use bitblock::history::world_tag;
use bitblock::identity_1::{ActorAuthStateV1, IdentityContextV1};
use bitblock::identity_d1::{
    ActorIdV1, AuthorizationDescriptorV1, CredentialRoleV1, NetworkEditOpV1, ThresholdPolicyV1,
    TransactionBodyV1,
};
use bitblock::local_bounds::LocalStorageProfile;
use bitblock::profile_bootstrap::ProfileScopeV1;
use bitblock::resource_1::ResourcePolicyV1;
use bitblock::threat_1::ThreatModelV1;
use bitblock::transition_spec::{BASE_STATE_DOMAINS_MASK, BaseTransitionSpecD1, ConflictPolicy};
use bitblock::transition_spec_1::{
    BandPredicateGateV1, DOMAIN_MASK_ASSET_RESOURCE, DOMAIN_MASK_AUTH_NONCE,
    DOMAIN_MASK_GOV_UPGRADE, DOMAIN_MASK_PROGRAM_VM, DOMAIN_MASK_VALIDATOR_SCHEDULE,
    ResolvedProfileTransitionSpecV1, TRANSITION_SPEC_1_VERSION, TransitionBandSlotV1,
};
use bitblock::validator_schedule_1::ValidatorKeyPairV1;
use bitblock::world::{Coord, World};

fn pk(seed: u8) -> CanonicalEd25519PublicKeyV1 {
    CanonicalEd25519PublicKeyV1::from_secret_seed(&[seed; 32]).unwrap()
}

fn tpol(role: CredentialRoleV1, seed: u8) -> ThresholdPolicyV1 {
    ThresholdPolicyV1 {
        role,
        threshold: 1,
        keys: vec![pk(seed)],
    }
}

fn build_probe_executor() -> (
    PrimaryLaunchExecutorV1,
    IdentityContextV1,
    ActorIdV1,
    ActorIdV1,
    [ActorIdV1; 4],
    [u8; 32],
    [u8; 32],
) {
    let econ_pol = ValidatorEconPolicyV1::v1_pos_default();
    let cons_pol = ConsensusPolicyDescriptorV1::v1_bft_pos_default();
    let gov_pol = GovernancePolicyDescriptorV1 {
        schema_version: 1,
        upgrade_quorum_numerator: 2,
        upgrade_quorum_denominator: 3,
        mandatory_exit_timelock_epochs: econ_pol.unbonding_delay_epochs,
        max_emergency_pause_epochs: 3,
        automatic_safe_halt_on_proven_fault: true,
        emergency_council_pause_only: true,
        forbid_history_rewrite: true,
        emergency_council: tpol(CredentialRoleV1::EmergencyPauseSigner, 91),
    };
    let preds = "asset-active economic-charging stake-consensus validator-economics"
        .split_ascii_whitespace()
        .map(str::to_string)
        .collect();
    let scope =
        ProfileScopeV1::from_wave1_contracts(&DomainRegistryV1::local_v1(), &gov_pol, preds)
            .unwrap();
    let threat = ThreatModelV1::v1_conservative_consumer(&scope).unwrap();
    let bounds =
        ProtocolBoundsProfileV1::v1_consumer_default(&threat, &LocalStorageProfile::DEFAULT)
            .unwrap();
    let asset_pol = AssetPolicyDescriptorV1::v1_dual_domain_default();
    let resource = ResourcePolicyV1::v1_default(&bounds, &asset_pol).unwrap();
    let genesis_id = [0xAA; 32];
    let profile_id = [0xBB; 32];
    let launch_manifest_id = [0x22; 32];
    let bounds_digest = bounds.tag().unwrap().digest();
    let resource_digest = resource.tag().unwrap().digest();
    let spec = ResolvedProfileTransitionSpecV1 {
        spec_version: TRANSITION_SPEC_1_VERSION,
        profile_id,
        base_contract_digest: BaseTransitionSpecD1::canonical_base_only()
            .tag()
            .unwrap()
            .digest(),
        bounds_profile_digest: bounds_digest,
        resource_policy_digest: resource_digest,
        max_batch_ops: bounds.max_block_ops,
        max_batch_gas: resource.max_block_gas,
        base_op_gas: u64::from(resource.gas_per_occupancy_op),
        base_domains_mask: BASE_STATE_DOMAINS_MASK,
        program_slot_active: false,
        bands: vec![
            TransitionBandSlotV1 {
                slot_id: 1,
                op_tag_min: 16,
                op_tag_max: 31,
                disposition_tag_min: 16,
                disposition_tag_max: 31,
                active: true,
                conflict_policy: ConflictPolicy::DeterministicOrder,
                read_domains_mask: BASE_STATE_DOMAINS_MASK | DOMAIN_MASK_AUTH_NONCE,
                write_domains_mask: DOMAIN_MASK_AUTH_NONCE,
                base_gas: u64::from(resource.gas_per_tx_base),
                per_byte_gas: u64::from(resource.gas_per_tx_byte),
                predicate_gate: BandPredicateGateV1::AlwaysRequired,
            },
            TransitionBandSlotV1 {
                slot_id: 2,
                op_tag_min: 32,
                op_tag_max: 47,
                disposition_tag_min: 32,
                disposition_tag_max: 47,
                active: true,
                conflict_policy: ConflictPolicy::RejectConcurrentWrite,
                read_domains_mask: BASE_STATE_DOMAINS_MASK
                    | DOMAIN_MASK_AUTH_NONCE
                    | DOMAIN_MASK_ASSET_RESOURCE,
                write_domains_mask: DOMAIN_MASK_ASSET_RESOURCE,
                base_gas: 25_000,
                per_byte_gas: u64::from(resource.gas_per_tx_byte),
                predicate_gate: BandPredicateGateV1::AssetActive,
            },
            TransitionBandSlotV1 {
                slot_id: 3,
                op_tag_min: 48,
                op_tag_max: 63,
                disposition_tag_min: 48,
                disposition_tag_max: 63,
                active: true,
                conflict_policy: ConflictPolicy::DeterministicOrder,
                read_domains_mask: BASE_STATE_DOMAINS_MASK
                    | DOMAIN_MASK_AUTH_NONCE
                    | DOMAIN_MASK_ASSET_RESOURCE
                    | DOMAIN_MASK_VALIDATOR_SCHEDULE,
                write_domains_mask: DOMAIN_MASK_VALIDATOR_SCHEDULE,
                base_gas: u64::from(resource.gas_per_ed25519_sig),
                per_byte_gas: u64::from(resource.gas_per_tx_byte),
                predicate_gate: BandPredicateGateV1::AlwaysRequired,
            },
            TransitionBandSlotV1 {
                slot_id: 4,
                op_tag_min: 64,
                op_tag_max: 79,
                disposition_tag_min: 64,
                disposition_tag_max: 79,
                active: true,
                conflict_policy: ConflictPolicy::RejectConcurrentWrite,
                read_domains_mask: BASE_STATE_DOMAINS_MASK
                    | DOMAIN_MASK_AUTH_NONCE
                    | DOMAIN_MASK_ASSET_RESOURCE
                    | DOMAIN_MASK_VALIDATOR_SCHEDULE
                    | DOMAIN_MASK_GOV_UPGRADE,
                write_domains_mask: DOMAIN_MASK_GOV_UPGRADE,
                base_gas: 40_000,
                per_byte_gas: u64::from(resource.gas_per_tx_byte),
                predicate_gate: BandPredicateGateV1::AlwaysRequired,
            },
            TransitionBandSlotV1 {
                slot_id: 5,
                op_tag_min: 80,
                op_tag_max: 95,
                disposition_tag_min: 80,
                disposition_tag_max: 95,
                active: false,
                conflict_policy: ConflictPolicy::RejectConcurrentWrite,
                read_domains_mask: BASE_STATE_DOMAINS_MASK
                    | DOMAIN_MASK_AUTH_NONCE
                    | DOMAIN_MASK_ASSET_RESOURCE
                    | DOMAIN_MASK_VALIDATOR_SCHEDULE
                    | DOMAIN_MASK_GOV_UPGRADE
                    | DOMAIN_MASK_PROGRAM_VM,
                write_domains_mask: DOMAIN_MASK_PROGRAM_VM,
                base_gas: 50_000,
                per_byte_gas: u64::from(resource.gas_per_proof_byte),
                predicate_gate: BandPredicateGateV1::ProgramActive,
            },
        ],
    };
    let spec_digest = spec.tag().unwrap().digest();
    let fork_context = ExecutorForkContextV1 {
        chain_id: 77,
        world_id: 1,
        genesis_id,
        profile_id,
        transition_spec_digest: spec_digest,
    };
    let desc_a = AuthorizationDescriptorV1 {
        descriptor_version: 0,
        action_policy: tpol(CredentialRoleV1::ActorAction, 11),
        admin_policy: tpol(CredentialRoleV1::ActorAdmin, 12),
        delegations: vec![],
        revoked: false,
    };
    let desc_b = AuthorizationDescriptorV1 {
        descriptor_version: 0,
        action_policy: tpol(CredentialRoleV1::ActorAction, 21),
        admin_policy: tpol(CredentialRoleV1::ActorAdmin, 22),
        delegations: vec![],
        revoked: false,
    };
    let actor_a = ActorIdV1::derive(77, "exec-a", &desc_a).unwrap();
    let actor_b = ActorIdV1::derive(77, "exec-b", &desc_b).unwrap();
    let vals = [
        ActorIdV1::from_bytes([1; 32]),
        ActorIdV1::from_bytes([2; 32]),
        ActorIdV1::from_bytes([3; 32]),
        ActorIdV1::from_bytes([4; 32]),
    ];
    let auth_ctx = IdentityContextV1 {
        protocol_version: 1,
        chain_id: 77,
        genesis_id,
        launch_manifest_id,
        profile_id,
        world_tag: world_tag(&World::new()),
        signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
        hash_suite: HashSuiteIdV1::Sha256V1,
    };
    let gov_ctx = GovernanceContextV1 {
        protocol_version: 1,
        chain_id: 77,
        genesis_id,
        launch_manifest_id,
        profile_id,
    };
    let st_a = ActorAuthStateV1 {
        state_version: 1,
        actor_id: actor_a,
        descriptor: desc_a,
        next_nonce: 0,
        recovery_guardians: tpol(CredentialRoleV1::ActorAdmin, 13),
        recovery_delay_heights: 100,
        pending_recovery: None,
    };
    let st_b = ActorAuthStateV1 {
        state_version: 1,
        actor_id: actor_b,
        descriptor: desc_b,
        next_nonce: 0,
        recovery_guardians: tpol(CredentialRoleV1::ActorAdmin, 23),
        recovery_delay_heights: 100,
        pending_recovery: None,
    };
    let mut forbidden = BTreeSet::new();
    for s in [11, 12, 13, 21, 22, 23] {
        forbidden.insert(pk(s));
    }
    let cfg = ExecutorGenesisConfigV1 {
        fork_context,
        profile_scope_digest: scope.tag().unwrap().digest(),
        spec,
        bounds,
        resource,
        auth_context: auth_ctx,
        initial_actors: vec![st_a, st_b],
        asset_policy: asset_pol,
        initial_balances: vec![(actor_a, 5_000_000), (actor_b, 5_000_000)],
        gov_context: gov_ctx,
        gov_policy: gov_pol,
        initial_params: vec![(1, 50_000_000)],
        econ_policy: econ_pol,
        initial_stakes: vals.iter().map(|&v| (v, 100_000_000_000)).collect(),
        consensus_policy: cons_pol,
        initial_validator_keys: (0..4)
            .map(|i| {
                (
                    vals[i],
                    ValidatorKeyPairV1 {
                        vote_key: pk(31 + i as u8),
                        proposal_key: pk(41 + i as u8),
                    },
                )
            })
            .collect(),
        forbidden_actor_keys: forbidden,
    };
    (
        PrimaryLaunchExecutorV1::from_genesis(cfg).unwrap(),
        auth_ctx,
        actor_a,
        actor_b,
        vals,
        spec_digest,
        bounds_digest,
    )
}

fn main() {
    let (base_exec, auth_ctx, actor_a, actor_b, vals, spec_digest, bounds_digest) =
        build_probe_executor();
    let tx = TransactionBodyV1 {
        actor_id: actor_a,
        nonce: 0,
        valid_after: 1,
        valid_until: 50,
        expected_state_root: Some(auth_ctx.world_tag.digest()),
        max_fee: 1_000,
        operations: vec![NetworkEditOpV1::On(Coord::new(2, 0, 0))],
    };
    let pre = auth_ctx.build_preimage(
        SigningDomainTagV1::ActorTransaction,
        tx.tag().unwrap().digest(),
    );
    let sig = sign_preimage_deterministic(&[11; 32], &pre).unwrap();
    let batch1 = ExecutorBlockBatchV1 {
        batch_version: 1,
        batch_height: 1,
        epoch: 0,
        fork_context: base_exec.fork_context(),
        base_fee_per_gas: 2,
        ops: vec![
            ExecutorOpV1::base_place_voxel(Coord::new(2, 0, 0)),
            ExecutorOpV1::auth_commit_tx(
                actor_a,
                actor_a,
                0,
                1,
                50,
                1_000,
                Coord::new(2, 0, 0),
                *pk(11).as_bytes(),
                *sig.as_bytes(),
            ),
            ExecutorOpV1::asset_transfer(actor_a, actor_b, 250_000),
            ExecutorOpV1::asset_mint_deposit(actor_a, Coord::new(2, 0, 0), [0x55; 32]),
            ExecutorOpV1::econ_bond_stake(vals[0], 10_000_000_000),
            ExecutorOpV1::schedule_epoch(0),
            ExecutorOpV1 {
                op_tag: 80,
                payload: vec![1, 2, 3],
            },
            ExecutorOpV1 {
                op_tag: 200,
                payload: vec![0xFF],
            },
        ],
    };
    let batch2 = ExecutorBlockBatchV1 {
        batch_version: 1,
        batch_height: 2,
        epoch: 1,
        fork_context: base_exec.fork_context(),
        base_fee_per_gas: 3,
        ops: vec![
            ExecutorOpV1::base_delete_voxel(Coord::new(0, 0, 0)),
            ExecutorOpV1::asset_transfer(actor_a, actor_b, 999_999_999),
            ExecutorOpV1::gov_safe_halt([0xEE; 32], 1, 3_000_000, 2_097_152),
            ExecutorOpV1::schedule_epoch(1),
        ],
    };

    let mut samples_b1 = Vec::with_capacity(50);
    let mut samples_b2 = Vec::with_capacity(50);
    let mut samples_rebuild = Vec::with_capacity(50);
    let mut r1_tag = String::new();
    let mut r2_tag = String::new();
    let mut r1_gas = 0u64;
    let mut r1_fee = 0u128;

    for _ in 0..50 {
        let mut ex = base_exec.clone();
        let t0 = Instant::now();
        let r1 = ex.execute_batch(&batch1).unwrap();
        samples_b1.push(t0.elapsed().as_secs_f64() * 1e6);
        let t1 = Instant::now();
        let r2 = ex.execute_batch(&batch2).unwrap();
        samples_b2.push(t1.elapsed().as_secs_f64() * 1e6);
        let t2 = Instant::now();
        ex.verify_all_domain_rebuild_parity().unwrap();
        samples_rebuild.push(t2.elapsed().as_secs_f64() * 1e6);
        verify_executor_receipt_against_spec(&batch1, ex.spec(), &r1).unwrap();
        verify_executor_receipt_against_spec(&batch2, ex.spec(), &r2).unwrap();
        r1_tag = r1.tag().unwrap().to_string();
        r2_tag = r2.tag().unwrap().to_string();
        r1_gas = r1.total_gas_charged;
        r1_fee = r1.total_fee_burned;
    }
    samples_b1.sort_by(f64::total_cmp);
    samples_b2.sort_by(f64::total_cmp);
    samples_rebuild.sort_by(f64::total_cmp);

    let hex = |b: &[u8; 32]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
    let mut ex_once = base_exec.clone();
    let r1_once = ex_once.execute_batch(&batch1).unwrap();
    let pre_r: Vec<_> = r1_once
        .pre_snapshot
        .active_root_array()
        .iter()
        .map(|x| hex(x)[..12].to_string())
        .collect();
    let post_r: Vec<_> = r1_once
        .post_snapshot
        .active_root_array()
        .iter()
        .map(|x| hex(x)[..12].to_string())
        .collect();
    println!("PRE_ROOTS={pre_r:?}");
    println!("POST_ROOTS={post_r:?}");
    println!("SPEC_DIGEST_HEX={}", hex(&spec_digest));
    println!("BOUNDS_DIGEST_HEX={}", hex(&bounds_digest));
    println!("BATCH1_TAG={}", batch1.tag().unwrap());
    println!("RCPT1_TAG={r1_tag}");
    println!("RCPT2_TAG={r2_tag}");
    println!("BATCH1_GAS={r1_gas}");
    println!("BATCH1_FEE={r1_fee}");
    println!("B1_P50_US={:.2}", samples_b1[25]);
    println!("B2_P50_US={:.2}", samples_b2[25]);
    println!("REBUILD_10DOM_P50_US={:.2}", samples_rebuild[25]);
}
