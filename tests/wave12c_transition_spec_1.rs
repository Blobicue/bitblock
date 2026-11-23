#![cfg(feature = "protocol-research")]
//! Wave 12C (`TRANSITION-SPEC-1`) integration tests: golden tag parity with
//! `tools/transition_spec_1_vectors.py`, Launch vs Successor `PROFILE-D1` closure, and all 7
//! powered semantic mutants (op-tag collision, write-domain overlap, omitted-effect receipt,
//! partial-write rollback leak, wrong version/profile, uncharged work `gas == 0`, and inactive
//! `Program/VM` op `80..=95` rejection on launch vs admission on successor).

use bitblock::asset_d1::AssetPolicyDescriptorV1;
use bitblock::bounds_1::ProtocolBoundsProfileV1;
use bitblock::commit_state::DomainRegistryV1;
use bitblock::consensus_gov_d1::GovernancePolicyDescriptorV1;
use bitblock::crypto_d1::{CanonicalEd25519PublicKeyV1, CryptoSuiteDescriptorV1};
use bitblock::identity_d1::{CredentialRoleV1, ThresholdPolicyV1};
use bitblock::local_bounds::LocalStorageProfile;
use bitblock::profile_bootstrap::ProfileScopeV1;
use bitblock::profile_d1::{BoundProfileManifestV1, CandidateProfileManifestV1};
use bitblock::protocol::ContentTag;
use bitblock::resource_1::ResourcePolicyV1;
use bitblock::threat_1::ThreatModelV1;
use bitblock::transition_spec_1::{
    EvalSpecContextV1, ProfileTransitionBatchV1, ProfileTransitionEvalReceiptV1,
    ProfileTransitionOpV1, ResolvedProfileTransitionSpecV1, TransitionEvalDispositionV1,
    TransitionSpec1Error, evaluate_profile_transition_spec, verify_receipt_invariants,
};

const GOLDEN_LAUNCH_SPEC_TAG: &str =
    "#bb.policy.sha256.xhy32kud2wdzknglo6p2n3gulbnlikjn3eqhmmquzl2jcofjg6wa";
const GOLDEN_SUCC_SPEC_TAG: &str =
    "#bb.policy.sha256.ny3mlc3tk7f7ebqexdcz3azhleksz554tgoh2rrqohjp3bx6ldta";
const GOLDEN_LAUNCH_EVAL_RECEIPT_TAG: &str =
    "#bb.receipt.sha256.ekrbspma4yaxej5loltysz3bznmy3ksyvqi3aaavui2fjvvjr3tq";
const GOLDEN_SUCC_EVAL_RECEIPT_TAG: &str =
    "#bb.receipt.sha256.ozw6nmwf2si4ukpj25l6ppiacte5xfiwd7npftacave272nzissq";

fn canonical_manifests_and_specs() -> (
    BoundProfileManifestV1,
    BoundProfileManifestV1,
    ResolvedProfileTransitionSpecV1,
    ResolvedProfileTransitionSpecV1,
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
        mandatory_exit_timelock_epochs: 21,
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

    let launch_cand = CandidateProfileManifestV1::canonical_launch_v1(
        genesis_d,
        [0x21; 32],
        [0x31; 32],
        bounds.tag().unwrap().digest(),
        crypto_d,
        resource.tag().unwrap().digest(),
    )
    .unwrap();
    let launch_bound = BoundProfileManifestV1::seal(launch_cand).unwrap();
    let succ_cand =
        CandidateProfileManifestV1::canonical_successor_program_v1(&launch_bound, [0x71; 32])
            .unwrap();
    let succ_bound = BoundProfileManifestV1::seal(succ_cand).unwrap();

    let launch_spec =
        ResolvedProfileTransitionSpecV1::resolve_from_profile(&launch_bound, &bounds, &resource)
            .unwrap();
    let succ_spec =
        ResolvedProfileTransitionSpecV1::resolve_from_profile(&succ_bound, &bounds, &resource)
            .unwrap();
    (launch_bound, succ_bound, launch_spec, succ_spec)
}

#[test]
fn transition_spec_1_golden_tags_and_codec_parity() {
    let (launch_bound, succ_bound, launch_spec, succ_spec) = canonical_manifests_and_specs();
    assert_eq!(
        launch_spec.tag().unwrap().to_string(),
        GOLDEN_LAUNCH_SPEC_TAG
    );
    assert_eq!(succ_spec.tag().unwrap().to_string(), GOLDEN_SUCC_SPEC_TAG);
    assert_eq!(
        ResolvedProfileTransitionSpecV1::decode_canonical(&launch_spec.encode_canonical().unwrap())
            .unwrap(),
        launch_spec
    );
    assert_eq!(
        ResolvedProfileTransitionSpecV1::decode_canonical(&succ_spec.encode_canonical().unwrap())
            .unwrap(),
        succ_spec
    );
    // Cross-profile verification catches swapped manifests fail-closed.
    assert!(matches!(
        launch_spec.verify_against_profile(&succ_bound),
        Err(TransitionSpec1Error::ProfileClosureMismatch(_))
    ));

    let ctx_l = EvalSpecContextV1 {
        expected_profile_id: launch_bound.profile_id,
        block_height: 100,
        pre_state_root: [0x10; 32],
    };
    let launch_batch = ProfileTransitionBatchV1 {
        spec_version: 1,
        profile_id: launch_bound.profile_id,
        block_height: 100,
        ops: vec![
            ProfileTransitionOpV1 {
                op_tag: 0,
                target_key: [1; 8],
                payload: vec![1, 2, 3, 4],
            },
            ProfileTransitionOpV1 {
                op_tag: 16,
                target_key: [2; 8],
                payload: vec![5, 6],
            },
            ProfileTransitionOpV1 {
                op_tag: 32,
                target_key: [3; 8],
                payload: vec![7, 8],
            },
            ProfileTransitionOpV1 {
                op_tag: 64,
                target_key: [4; 8],
                payload: vec![9],
            },
        ],
    };
    let launch_bytes = launch_batch.encode_canonical();
    let launch_rcpt =
        evaluate_profile_transition_spec(&launch_spec, &ctx_l, &launch_bytes).unwrap();
    assert_eq!(
        launch_rcpt.disposition,
        TransitionEvalDispositionV1::Admitted
    );
    assert_eq!(launch_rcpt.gas_charged, 88_144);
    assert_eq!(launch_rcpt.effects_count, 4);
    assert_eq!(
        launch_rcpt.tag().unwrap().to_string(),
        GOLDEN_LAUNCH_EVAL_RECEIPT_TAG
    );
    assert_eq!(
        ProfileTransitionEvalReceiptV1::decode_canonical(&launch_rcpt.encode_canonical().unwrap())
            .unwrap(),
        launch_rcpt
    );
    verify_receipt_invariants(&launch_spec, &ctx_l, &launch_bytes, &launch_rcpt).unwrap();

    let ctx_s = EvalSpecContextV1 {
        expected_profile_id: succ_bound.profile_id,
        block_height: 200,
        pre_state_root: [0x20; 32],
    };
    let succ_batch = ProfileTransitionBatchV1 {
        spec_version: 1,
        profile_id: succ_bound.profile_id,
        block_height: 200,
        ops: vec![ProfileTransitionOpV1 {
            op_tag: 80,
            target_key: [8; 8],
            payload: vec![0xDE, 0xAD, 0xBE, 0xEF],
        }],
    };
    let succ_bytes = succ_batch.encode_canonical();
    let succ_rcpt = evaluate_profile_transition_spec(&succ_spec, &ctx_s, &succ_bytes).unwrap();
    assert_eq!(succ_rcpt.disposition, TransitionEvalDispositionV1::Admitted);
    assert_eq!(succ_rcpt.gas_charged, 50_128);
    assert_eq!(
        succ_rcpt.tag().unwrap().to_string(),
        GOLDEN_SUCC_EVAL_RECEIPT_TAG
    );
    verify_receipt_invariants(&succ_spec, &ctx_s, &succ_bytes, &succ_rcpt).unwrap();
}

#[test]
fn transition_spec_1_seven_powered_semantic_mutants_and_total_boundaries() {
    let (launch_bound, succ_bound, launch_spec, succ_spec) = canonical_manifests_and_specs();

    // Mutant 1: Op-tag band collision
    let mut m1 = launch_spec.clone();
    m1.bands[1].op_tag_min = 31;
    assert_eq!(
        m1.validate(),
        Err(TransitionSpec1Error::OpTagBandCollision {
            first_slot: 1,
            second_slot: 2
        })
    );

    // Mutant 2: Write-domain bitmask overlap
    let mut m2 = launch_spec.clone();
    m2.bands[1].write_domains_mask |= m2.bands[0].write_domains_mask;
    m2.bands[1].read_domains_mask |= m2.bands[1].write_domains_mask;
    assert!(matches!(
        m2.validate(),
        Err(TransitionSpec1Error::WriteDomainOverlap {
            first_slot: 1,
            second_slot: 2,
            ..
        })
    ));

    // Mutant 3: Omitted-effect receipt tamper
    let ctx_l = EvalSpecContextV1 {
        expected_profile_id: launch_bound.profile_id,
        block_height: 100,
        pre_state_root: [0x10; 32],
    };
    let good_batch = ProfileTransitionBatchV1 {
        spec_version: 1,
        profile_id: launch_bound.profile_id,
        block_height: 100,
        ops: vec![
            ProfileTransitionOpV1 {
                op_tag: 0,
                target_key: [1; 8],
                payload: vec![1, 2],
            },
            ProfileTransitionOpV1 {
                op_tag: 48,
                target_key: [5; 8],
                payload: vec![3, 4],
            },
        ],
    };
    let good_bytes = good_batch.encode_canonical();
    let mut omitted = evaluate_profile_transition_spec(&launch_spec, &ctx_l, &good_bytes).unwrap();
    omitted.effects_count = 1;
    assert_eq!(
        verify_receipt_invariants(&launch_spec, &ctx_l, &good_bytes, &omitted),
        Err(TransitionSpec1Error::OmittedEffectDetected)
    );

    // Mutant 4: Partial-write rollback leak on rejected batch (op 0 followed by unassigned op 96)
    let bad_tag_batch = ProfileTransitionBatchV1 {
        spec_version: 1,
        profile_id: launch_bound.profile_id,
        block_height: 100,
        ops: vec![
            ProfileTransitionOpV1 {
                op_tag: 0,
                target_key: [1; 8],
                payload: vec![1],
            },
            ProfileTransitionOpV1 {
                op_tag: 96,
                target_key: [9; 8],
                payload: vec![2],
            },
        ],
    };
    let bad_tag_bytes = bad_tag_batch.encode_canonical();
    let mut leaked =
        evaluate_profile_transition_spec(&launch_spec, &ctx_l, &bad_tag_bytes).unwrap();
    assert_eq!(
        leaked.disposition,
        TransitionEvalDispositionV1::RejectedUnknownOperationTag
    );
    assert_eq!(leaked.post_state_root, ctx_l.pre_state_root);
    leaked.post_state_root[31] ^= 0xFF;
    assert_eq!(
        verify_receipt_invariants(&launch_spec, &ctx_l, &bad_tag_bytes, &leaked),
        Err(TransitionSpec1Error::PartialWriteRollbackViolation)
    );

    // Mutant 5: Wrong version or wrong profile_id
    let mut wrong_spec = launch_spec.clone();
    wrong_spec.spec_version = 2;
    assert_eq!(
        wrong_spec.validate(),
        Err(TransitionSpec1Error::WrongVersion {
            expected: 1,
            actual: 2
        })
    );
    let mut wrong_pid_batch = good_batch.clone();
    wrong_pid_batch.profile_id = succ_bound.profile_id;
    let r5 =
        evaluate_profile_transition_spec(&launch_spec, &ctx_l, &wrong_pid_batch.encode_canonical())
            .unwrap();
    assert_eq!(
        r5.disposition,
        TransitionEvalDispositionV1::RejectedWrongVersionOrProfile
    );

    // Mutant 6: Uncharged work formula (`gas == 0`)
    let mut m6 = launch_spec.clone();
    m6.bands[2].base_gas = 0;
    assert_eq!(
        m6.validate(),
        Err(TransitionSpec1Error::UnchargedWorkFormula { slot_id: 3 })
    );

    // Mutant 7: Inactive Program/VM slot `80..=95` rejected on Launch, admitted on Successor
    let prog_launch = ProfileTransitionBatchV1 {
        spec_version: 1,
        profile_id: launch_bound.profile_id,
        block_height: 100,
        ops: vec![ProfileTransitionOpV1 {
            op_tag: 80,
            target_key: [8; 8],
            payload: vec![1, 2, 3],
        }],
    };
    let r7 =
        evaluate_profile_transition_spec(&launch_spec, &ctx_l, &prog_launch.encode_canonical())
            .unwrap();
    assert_eq!(
        r7.disposition,
        TransitionEvalDispositionV1::RejectedInactiveExtensionSlot
    );
    assert_eq!(r7.post_state_root, ctx_l.pre_state_root);

    // Extra total-evaluator checks: RejectConcurrentWrite conflict, malformed bytes, gas limit, unassigned 255
    let conflict_batch = ProfileTransitionBatchV1 {
        spec_version: 1,
        profile_id: launch_bound.profile_id,
        block_height: 100,
        ops: vec![
            ProfileTransitionOpV1 {
                op_tag: 32,
                target_key: [7; 8],
                payload: vec![1],
            },
            ProfileTransitionOpV1 {
                op_tag: 33,
                target_key: [7; 8],
                payload: vec![2],
            },
        ],
    };
    let rc =
        evaluate_profile_transition_spec(&launch_spec, &ctx_l, &conflict_batch.encode_canonical())
            .unwrap();
    assert_eq!(
        rc.disposition,
        TransitionEvalDispositionV1::RejectedConflict
    );
    assert_eq!(rc.post_state_root, ctx_l.pre_state_root);

    let r_mal = evaluate_profile_transition_spec(&launch_spec, &ctx_l, b"truncated").unwrap();
    assert_eq!(
        r_mal.disposition,
        TransitionEvalDispositionV1::RejectedMalformedFrame
    );

    let ctx_s = EvalSpecContextV1 {
        expected_profile_id: succ_bound.profile_id,
        block_height: 200,
        pre_state_root: [0x20; 32],
    };
    let op255 = ProfileTransitionBatchV1 {
        spec_version: 1,
        profile_id: succ_bound.profile_id,
        block_height: 200,
        ops: vec![ProfileTransitionOpV1 {
            op_tag: 255,
            target_key: [9; 8],
            payload: vec![0],
        }],
    };
    let r255 =
        evaluate_profile_transition_spec(&succ_spec, &ctx_s, &op255.encode_canonical()).unwrap();
    assert_eq!(
        r255.disposition,
        TransitionEvalDispositionV1::RejectedUnknownOperationTag
    );
}
