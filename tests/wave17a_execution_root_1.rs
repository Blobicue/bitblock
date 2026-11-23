#![cfg(feature = "protocol-research")]
//! Wave 17A (`EXECUTION-ROOT-1`, `PS-030`) integration tests: exact L1 §5 two-layer 11-slot
//! root composition over `EXECUTOR-1` snapshots, golden Rust == Python tag parity, and all 10
//! `CAPABILITY_TREE.md` fail-closed adversarial gate controls.

use bitblock::asset_d1::{AssetPolicyDescriptorV1, ValidatorEconPolicyV1};
use bitblock::bounds_1::ProtocolBoundsProfileV1;
use bitblock::commit_state::DomainRegistryV1;
use bitblock::consensus_gov_d1::GovernancePolicyDescriptorV1;
use bitblock::crypto_d1::{CanonicalEd25519PublicKeyV1, CryptoSuiteDescriptorV1};
use bitblock::execution_root_1::{
    ExecutionDomainEntryV1, ExecutionDomainRegistryV1, ExecutionDomainTagV1, ExecutionLayerV1,
    ExecutionRoot1Error, ExecutionRootCompositionV1, ExecutionRootPreimageContextV1,
    InactiveDomainSentinelV1, RootSlotCommitmentV1, active_slot, compose_from_executor_snapshot,
    compose_from_slot_commitments, verify_header_and_manifest_correspondence,
};
use bitblock::executor_1::ExecutorDomainSnapshotV1;
use bitblock::identity_d1::{CredentialRoleV1, ThresholdPolicyV1};
use bitblock::local_bounds::LocalStorageProfile;
use bitblock::profile_bootstrap::ProfileScopeV1;
use bitblock::profile_d1::{BoundProfileManifestV1, CandidateProfileManifestV1};
use bitblock::protocol::ContentTag;
use bitblock::resource_1::ResourcePolicyV1;
use bitblock::threat_1::ThreatModelV1;

fn canonical_launch_and_successor_manifests() -> (
    BoundProfileManifestV1,
    BoundProfileManifestV1,
    [u8; 32],
    [u8; 32],
) {
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
    let crypto_d = CryptoSuiteDescriptorV1::v1_ed25519_sha256()
        .tag()
        .unwrap()
        .digest();
    let genesis_d = ContentTag::parse_full(
        "#bb.policy.sha256.jowjx6wyiayipiaypxgmjc2xgynt335d7mrx3qfx2e6pxegyshgq",
    )
    .unwrap()
    .digest();
    let scope_d = scope.tag().unwrap().digest();
    let dom_d = DomainRegistryV1::local_v1().tag().digest();
    let bounds_d = bounds.tag().unwrap().digest();
    let res_d = resource.tag().unwrap().digest();

    let launch_cand = CandidateProfileManifestV1::canonical_launch_v1(
        genesis_d, scope_d, dom_d, crypto_d, bounds_d, res_d,
    )
    .unwrap();
    let launch_bound = BoundProfileManifestV1::seal(launch_cand).unwrap();

    let succ_cand =
        CandidateProfileManifestV1::canonical_successor_program_v1(&launch_bound, [0x5E; 32])
            .unwrap();
    let succ_bound = BoundProfileManifestV1::seal(succ_cand).unwrap();
    (launch_bound, succ_bound, genesis_d, scope_d)
}

const GOLDEN_REG_TAG: &str =
    "#bb.policy.sha256.3wkwyds7dgkm5hd4vwj26lkokfi6xkeft6d7yl3hzddm3tq6i62q";
const GOLDEN_COMP1_TAG: &str =
    "#bb.receipt.sha256.klerr3wgoyagb24baztzztdolokhthmpyo4ohbx6bwxb6nzkd2mq";
const GOLDEN_COMP2_TAG: &str =
    "#bb.receipt.sha256.n36urqqdjll77eppiiprbnoifbgzruwzv6ztxkqyn22opx6zdwwq";

#[test]
fn wave17a_execution_root_1_golden_parity_and_header_manifest_verification() {
    let reg = ExecutionDomainRegistryV1::canonical_launch_v1();
    let snap = ExecutorDomainSnapshotV1 {
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
    let comp1 = compose_from_executor_snapshot(&snap, &reg, pctx, [0xD3; 32]).unwrap();
    let comp2 = compose_from_executor_snapshot(&snap, &reg, pctx, [0xE4; 32]).unwrap();
    assert_eq!(reg.tag().unwrap().to_string(), GOLDEN_REG_TAG);
    assert_eq!(comp1.tag().unwrap().to_string(), GOLDEN_COMP1_TAG);
    assert_eq!(comp2.tag().unwrap().to_string(), GOLDEN_COMP2_TAG);
    assert_eq!(comp1.application_root, comp2.application_root);
    assert_eq!(comp1.execution_state_root, comp2.execution_state_root);
    assert_ne!(comp1.execution_root_ref, comp2.execution_root_ref);

    // Header + manifest correspondence against real BoundProfileManifestV1
    let (launch_bound, succ_bound, genesis_d, scope_d) = canonical_launch_and_successor_manifests();
    let real_ctx = ExecutionRootPreimageContextV1 {
        protocol_version: 1,
        chain_id: 77,
        world_id: 1,
        genesis_id: genesis_d,
        profile_scope_digest: scope_d,
    };
    let launch_comp =
        compose_from_executor_snapshot(&snap, &reg, real_ctx, launch_bound.profile_id).unwrap();
    verify_header_and_manifest_correspondence(
        &launch_comp,
        &launch_comp.execution_root_ref,
        &reg,
        &launch_bound,
    )
    .unwrap();

    // Mixed-manifest publication: CE-43 successor manifest (program-active = true) rejected!
    assert!(matches!(
        verify_header_and_manifest_correspondence(
            &launch_comp,
            &launch_comp.execution_root_ref,
            &reg,
            &succ_bound,
        ),
        Err(ExecutionRoot1Error::MixedManifestRejected(_))
    ));
}

#[test]
fn wave17a_execution_root_1_ten_adversarial_gate_controls() {
    let reg = ExecutionDomainRegistryV1::canonical_launch_v1();
    let (launch_bound, succ_bound, genesis_d, scope_d) = canonical_launch_and_successor_manifests();
    let pctx = ExecutionRootPreimageContextV1 {
        protocol_version: 1,
        chain_id: 77,
        world_id: 1,
        genesis_id: genesis_d,
        profile_scope_digest: scope_d,
    };
    let snap = ExecutorDomainSnapshotV1 {
        snapshot_version: 1,
        batch_height: 4,
        epoch: 1,
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
    let base = compose_from_executor_snapshot(&snap, &reg, pctx, launch_bound.profile_id).unwrap();

    // Gate 1: Omitted required domain rejects
    let mut omitted = base.slots.clone();
    omitted.pop();
    assert!(matches!(
        compose_from_slot_commitments(&omitted, 4, 1, &reg, pctx, launch_bound.profile_id),
        Err(ExecutionRoot1Error::SlotCardinalityOrOrder(_))
    ));

    // Gate 2: Duplicate domain tag rejects
    let mut dup = base.slots.clone();
    dup[10] = dup[9];
    assert!(matches!(
        compose_from_slot_commitments(&dup, 4, 1, &reg, pctx, launch_bound.profile_id),
        Err(ExecutionRoot1Error::SlotCardinalityOrOrder(_))
    ));

    // Gate 3: Unknown domain tag wire decode rejects
    let mut bad_reg_wire = reg.encode_canonical().unwrap();
    bad_reg_wire[5] = 99;
    assert!(matches!(
        ExecutionDomainRegistryV1::decode_canonical(&bad_reg_wire),
        Err(ExecutionRoot1Error::InvalidRegistry(_))
    ));

    // Gate 4: Non-canonical slot ordering rejects
    let mut misordered = base.slots.clone();
    misordered.swap(6, 7);
    assert!(matches!(
        compose_from_slot_commitments(&misordered, 4, 1, &reg, pctx, launch_bound.profile_id),
        Err(ExecutionRoot1Error::SlotCardinalityOrOrder(_))
    ));

    // Gate 5: Synthetic zero substitute for inactive sentinel or active root rejects
    let mut zero_active = base.slots.clone();
    zero_active[0] = active_slot(ExecutionDomainTagV1::WorldOccupancy, [0u8; 32]);
    assert!(matches!(
        compose_from_slot_commitments(&zero_active, 4, 1, &reg, pctx, launch_bound.profile_id),
        Err(ExecutionRoot1Error::ActiveInactiveMismatch(_))
    ));
    let mut zero_sent_wire = base.slots[5].encode_canonical();
    zero_sent_wire[3] = 0; // sentinel discriminant 0 -> invalid
    assert!(matches!(
        RootSlotCommitmentV1::decode_canonical(&zero_sent_wire),
        Err(ExecutionRoot1Error::ActiveInactiveMismatch(_))
    ));

    // Gate 6: Active root supplied in inactive slot 6 rejects
    let mut active_in_inactive = base.slots.clone();
    active_in_inactive[5] = active_slot(ExecutionDomainTagV1::ProgramState, [0x5A; 32]);
    assert!(matches!(
        compose_from_slot_commitments(
            &active_in_inactive,
            4,
            1,
            &reg,
            pctx,
            launch_bound.profile_id
        ),
        Err(ExecutionRoot1Error::ActiveInactiveMismatch(_))
    ));

    // Gate 7: Inactive sentinel supplied in active slot rejects
    let mut sent_in_active = base.slots.clone();
    sent_in_active[0] = RootSlotCommitmentV1::InactiveSentinel {
        domain_tag: ExecutionDomainTagV1::WorldOccupancy,
        layer: ExecutionLayerV1::ApplicationLayer,
        sentinel: InactiveDomainSentinelV1::UnactivatedLaunchProgramV1,
        sentinel_schema_digest: InactiveDomainSentinelV1::UnactivatedLaunchProgramV1
            .schema_digest(),
    };
    assert!(matches!(
        compose_from_slot_commitments(&sent_in_active, 4, 1, &reg, pctx, launch_bound.profile_id),
        Err(ExecutionRoot1Error::ActiveInactiveMismatch(_))
    ));

    // Gate 8: Cross-domain root_schema_digest substitution rejects
    let mut swapped_schema = base.slots.clone();
    swapped_schema[6] = RootSlotCommitmentV1::ActiveRoot {
        domain_tag: ExecutionDomainTagV1::Authorization,
        layer: ExecutionLayerV1::ProtocolLayer,
        root_schema_digest: ExecutionDomainTagV1::Nonce.canonical_root_schema_digest(),
        root_bytes: [0x44; 32],
    };
    assert!(matches!(
        compose_from_slot_commitments(&swapped_schema, 4, 1, &reg, pctx, launch_bound.profile_id),
        Err(ExecutionRoot1Error::CrossDomainSubstitution(_))
    ));

    // Gate 9: Header / chain / genesis / world / version mutation rejects
    let mut bad_header = base.execution_root_ref;
    bad_header.chain_id = 78;
    assert!(matches!(
        verify_header_and_manifest_correspondence(&base, &bad_header, &reg, &launch_bound),
        Err(ExecutionRoot1Error::ContextOrHeaderMismatch(_))
    ));
    let mut bad_comp = base.clone();
    bad_comp.execution_state_root[0] ^= 0xFF;
    assert!(matches!(
        verify_header_and_manifest_correspondence(
            &bad_comp,
            &bad_comp.execution_root_ref,
            &reg,
            &launch_bound
        ),
        Err(ExecutionRoot1Error::ContextOrHeaderMismatch(_))
    ));

    // Gate 10: Mixed-manifest publication (successor manifest with launch registry) &
    // malformed registry marking mandatory domain inactive both reject!
    let mut bad_reg = reg.clone();
    bad_reg.entries[0] = ExecutionDomainEntryV1::Inactive {
        domain_tag: ExecutionDomainTagV1::WorldOccupancy,
        layer: ExecutionLayerV1::ApplicationLayer,
        sentinel: InactiveDomainSentinelV1::UnactivatedLaunchProgramV1,
        sentinel_schema_digest: InactiveDomainSentinelV1::UnactivatedLaunchProgramV1
            .schema_digest(),
    };
    assert!(matches!(
        bad_reg.validate(),
        Err(ExecutionRoot1Error::InvalidRegistry(_))
    ));
    let mut active_prog_snap = snap;
    active_prog_snap.program_state_active = true;
    assert!(matches!(
        compose_from_executor_snapshot(&active_prog_snap, &reg, pctx, succ_bound.profile_id),
        Err(ExecutionRoot1Error::MixedManifestRejected(_))
    ));
    // Verify roundtrip codec on final composition
    assert_eq!(
        ExecutionRootCompositionV1::decode_canonical(&base.encode_canonical().unwrap()).unwrap(),
        base
    );
}
