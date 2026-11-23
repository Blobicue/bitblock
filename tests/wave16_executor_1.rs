#![cfg(feature = "protocol-research")]
//! Wave 16 (`EXECUTOR-1`, `PS-065`) integration tests: true dual-language differential parity
//! between Rust [`PrimaryLaunchExecutorV1`] and clean-room Python `tools/executor_1_vectors.py`,
//! 10-domain cold-rebuild parity, atomic failure/no-op rollback, inactive `Program/VM` wall,
//! fork-context isolation, and powered semantic mutant detection.

use std::collections::BTreeSet;

use bitblock::asset_d1::{AssetPolicyDescriptorV1, ValidatorEconPolicyV1};
use bitblock::bounds_1::ProtocolBoundsProfileV1;
use bitblock::commit_state::DomainRegistryV1;
use bitblock::consensus_gov_d1::{ConsensusPolicyDescriptorV1, GovernancePolicyDescriptorV1};
use bitblock::crypto_d1::{
    CanonicalEd25519PublicKeyV1, HashSuiteIdV1, SignatureSuiteIdV1, SigningDomainTagV1,
    sign_preimage_deterministic,
};
use bitblock::executor_1::{
    Executor1Error, ExecutorBlockBatchV1, ExecutorBlockReceiptV1, ExecutorDispositionV1,
    ExecutorForkContextV1, ExecutorGenesisConfigV1, ExecutorOpV1, PrimaryLaunchExecutorV1,
    verify_executor_receipt_against_spec,
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

const GOLDEN_BATCH1_TAG: &str =
    "#bb.receipt.sha256.fs5gowdymm4iv5rafhzuqk4uh5aoupwiqwxu7wydzthq56jql7pq";
const GOLDEN_RCPT1_TAG: &str =
    "#bb.receipt.sha256.d4ibxky7rkr5257wxkw6vt6nhladett7vrs5l2gkzgzlltazpwca";
const GOLDEN_RCPT2_TAG: &str =
    "#bb.receipt.sha256.lewpee4g4wfchckl3ibszv5nxdyagqlt4sjwx7ee66tfcx3kf2nq";

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

fn build_differential_executor() -> (
    PrimaryLaunchExecutorV1,
    IdentityContextV1,
    ActorIdV1,
    ActorIdV1,
    [ActorIdV1; 4],
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
    )
}

#[test]
fn wave16_executor_1_dual_language_differential_and_rebuild_parity() {
    let (mut exec, auth_ctx, actor_a, actor_b, vals) = build_differential_executor();
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
        fork_context: exec.fork_context(),
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
    let b1_bytes = batch1.encode_canonical().unwrap();
    let rcpt1 = exec.execute_batch_bytes(&b1_bytes).unwrap();
    verify_executor_receipt_against_spec(&batch1, exec.spec(), &rcpt1).unwrap();
    exec.verify_all_domain_rebuild_parity().unwrap();
    assert_eq!(rcpt1.admitted_count, 6);
    assert_eq!(rcpt1.rejected_count, 2);
    assert_eq!(
        rcpt1.step_outcomes[6].disposition,
        ExecutorDispositionV1::RejectedInactiveExtensionBand
    );
    assert_eq!(
        rcpt1.step_outcomes[7].disposition,
        ExecutorDispositionV1::RejectedUnknownOperationTag
    );

    let batch2 = ExecutorBlockBatchV1 {
        batch_version: 1,
        batch_height: 2,
        epoch: 1,
        fork_context: exec.fork_context(),
        base_fee_per_gas: 3,
        ops: vec![
            ExecutorOpV1::base_delete_voxel(Coord::new(0, 0, 0)),
            ExecutorOpV1::asset_transfer(actor_a, actor_b, 999_999_999),
            ExecutorOpV1::gov_safe_halt([0xEE; 32], 1, 3_000_000, 2_097_152),
            ExecutorOpV1::schedule_epoch(1),
        ],
    };
    let rcpt2 = exec.execute_batch(&batch2).unwrap();
    verify_executor_receipt_against_spec(&batch2, exec.spec(), &rcpt2).unwrap();
    exec.verify_all_domain_rebuild_parity().unwrap();
    assert_eq!(rcpt2.admitted_count, 1);
    assert_eq!(rcpt2.rejected_count, 3);
    assert_eq!(
        rcpt2.step_outcomes[0].disposition,
        ExecutorDispositionV1::RejectedBedrockImmutable
    );
    assert_eq!(
        rcpt2.step_outcomes[1].disposition,
        ExecutorDispositionV1::RejectedAssetInvariant
    );
    assert_eq!(
        rcpt2.step_outcomes[2].disposition,
        ExecutorDispositionV1::Admitted
    );
    assert_eq!(
        rcpt2.step_outcomes[3].disposition,
        ExecutorDispositionV1::RejectedValidatorOrSchedule
    );

    // Verify independent golden parity tags corroborated against clean-room Python executor
    assert_eq!(batch1.tag().unwrap().to_string(), GOLDEN_BATCH1_TAG);
    assert_eq!(rcpt1.tag().unwrap().to_string(), GOLDEN_RCPT1_TAG);
    assert_eq!(rcpt2.tag().unwrap().to_string(), GOLDEN_RCPT2_TAG);
    assert_eq!(
        ExecutorBlockReceiptV1::decode_canonical(&rcpt1.encode_canonical().unwrap()).unwrap(),
        rcpt1
    );
    assert_eq!(
        ExecutorBlockReceiptV1::decode_canonical(&rcpt2.encode_canonical().unwrap()).unwrap(),
        rcpt2
    );
}

#[test]
fn wave16_executor_1_adversarial_fork_rollback_and_powered_mutants() {
    let (mut exec, _, actor_a, actor_b, vals) = build_differential_executor();
    let base_snap = exec.snapshot().unwrap();

    // 1. Wrong fork context rejected fail-closed before execution
    let mut bad_ctx = exec.fork_context();
    bad_ctx.profile_id[0] ^= 0xFF;
    let bad_batch = ExecutorBlockBatchV1 {
        batch_version: 1,
        batch_height: 1,
        epoch: 0,
        fork_context: bad_ctx,
        base_fee_per_gas: 1,
        ops: vec![ExecutorOpV1::base_place_voxel(Coord::new(1, 0, 0))],
    };
    assert!(matches!(
        exec.execute_batch(&bad_batch),
        Err(Executor1Error::ForkContextMismatch(_))
    ));
    assert_eq!(exec.snapshot().unwrap(), base_snap);

    // 2. Non-monotonic batch height rejected fail-closed
    let skip_batch = ExecutorBlockBatchV1 {
        batch_version: 1,
        batch_height: 5,
        epoch: 0,
        fork_context: exec.fork_context(),
        base_fee_per_gas: 1,
        ops: vec![],
    };
    assert!(matches!(
        exec.execute_batch(&skip_batch),
        Err(Executor1Error::NonMonotonicBatch(_))
    ));

    // 3. All-rejected batch preserves all 10 domain roots identically
    let rej_batch = ExecutorBlockBatchV1 {
        batch_version: 1,
        batch_height: 1,
        epoch: 0,
        fork_context: exec.fork_context(),
        base_fee_per_gas: 2,
        ops: vec![
            ExecutorOpV1::base_place_voxel(Coord::new(0, 0, 0)),
            ExecutorOpV1::base_delete_voxel(Coord::new(9, 9, 9)),
            ExecutorOpV1 {
                op_tag: 85,
                payload: vec![0xAA],
            },
            ExecutorOpV1 {
                op_tag: 250,
                payload: vec![],
            },
            ExecutorOpV1::asset_transfer(actor_a, actor_b, 999_999_999),
            ExecutorOpV1::asset_burn_refund(actor_a, Coord::new(7, 7, 7)),
            ExecutorOpV1::econ_slash_equivocation(ActorIdV1::from_bytes([0x99; 32]), [1; 32]),
            ExecutorOpV1::gov_safe_halt([2; 32], 1, 10, 100),
        ],
    };
    let rej_rcpt = exec.execute_batch(&rej_batch).unwrap();
    assert_eq!(rej_rcpt.admitted_count, 0);
    assert_eq!(rej_rcpt.rejected_count, 8);
    assert!(rej_rcpt.pre_snapshot.roots_equal(&rej_rcpt.post_snapshot));
    verify_executor_receipt_against_spec(&rej_batch, exec.spec(), &rej_rcpt).unwrap();

    // 4. Powered semantic mutants (5 distinct mutant classes caught fail-closed)
    let mut m_omitted = rej_rcpt.clone();
    m_omitted.step_outcomes.pop();
    assert!(matches!(
        verify_executor_receipt_against_spec(&rej_batch, exec.spec(), &m_omitted),
        Err(Executor1Error::SemanticMutantDetected(_))
    ));

    let mut m_leak = rej_rcpt.clone();
    m_leak.post_snapshot.world_root[0] ^= 1;
    assert!(matches!(
        verify_executor_receipt_against_spec(&rej_batch, exec.spec(), &m_leak),
        Err(Executor1Error::SemanticMutantDetected(_))
    ));

    let mut m_inactive = rej_rcpt.clone();
    m_inactive.step_outcomes[2].disposition = ExecutorDispositionV1::Admitted;
    assert!(matches!(
        verify_executor_receipt_against_spec(&rej_batch, exec.spec(), &m_inactive),
        Err(Executor1Error::SemanticMutantDetected(_))
    ));

    let mut m_digest = rej_rcpt.clone();
    m_digest.outcomes_digest[0] ^= 1;
    assert!(matches!(
        verify_executor_receipt_against_spec(&rej_batch, exec.spec(), &m_digest),
        Err(Executor1Error::SemanticMutantDetected(_))
    ));

    // 5. Admitted slash + burn refund + gov epoch advance + uncharged-work mutant check
    let ok_batch = ExecutorBlockBatchV1 {
        batch_version: 1,
        batch_height: 2,
        epoch: 1,
        fork_context: exec.fork_context(),
        base_fee_per_gas: 2,
        ops: vec![
            ExecutorOpV1::base_place_voxel(Coord::new(5, 5, 5)),
            ExecutorOpV1::base_delete_voxel(Coord::new(5, 5, 5)),
            ExecutorOpV1::asset_mint_deposit(actor_a, Coord::new(5, 5, 5), [0x44; 32]),
            ExecutorOpV1::asset_burn_refund(actor_a, Coord::new(5, 5, 5)),
            ExecutorOpV1::econ_slash_equivocation(vals[3], [0x77; 32]),
            ExecutorOpV1::gov_advance_epoch(1),
        ],
    };
    let ok_rcpt = exec.execute_batch(&ok_batch).unwrap();
    assert_eq!(ok_rcpt.admitted_count, 6);
    assert_eq!(ok_rcpt.rejected_count, 0);
    exec.verify_all_domain_rebuild_parity().unwrap();
    let mut m_free_gas = ok_rcpt.clone();
    m_free_gas.step_outcomes[0].gas_charged = 0;
    assert!(matches!(
        verify_executor_receipt_against_spec(&ok_batch, exec.spec(), &m_free_gas),
        Err(Executor1Error::SemanticMutantDetected(_))
    ));
}
