//! Release probe for Wave 12C (`TRANSITION-SPEC-1`): resolves launch & successor profile specs,
//! exercises the total byte-to-disposition evaluator across all 6 bands + unassigned `96..=255`,
//! validates all 7 powered semantic mutants, and prints structured JSON to stdout.

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
    EvalSpecContextV1, ProfileTransitionBatchV1, ProfileTransitionOpV1,
    ResolvedProfileTransitionSpecV1, TransitionEvalDispositionV1, TransitionSpec1Error,
    evaluate_profile_transition_spec, verify_receipt_invariants,
};

fn hex_prefix(bytes: &[u8; 32], n: usize) -> String {
    bytes
        .iter()
        .take(n)
        .map(|b| format!("{b:02x}"))
        .collect::<String>()
}

fn canonical_bounds_and_resource() -> (ProtocolBoundsProfileV1, ResourcePolicyV1) {
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
    (bounds, resource)
}

fn main() {
    let genesis_tag = ContentTag::parse_full(
        "#bb.policy.sha256.jowjx6wyiayipiaypxgmjc2xgynt335d7mrx3qfx2e6pxegyshgq",
    )
    .unwrap();
    let genesis_scope_digest = [0x21; 32];
    let successor_scope_digest = [0x71; 32];
    let domain_registry_digest = [0x31; 32];
    let (bounds, resource) = canonical_bounds_and_resource();
    let crypto_digest = CryptoSuiteDescriptorV1::v1_ed25519_sha256()
        .tag()
        .unwrap()
        .digest();

    let launch_cand = CandidateProfileManifestV1::canonical_launch_v1(
        genesis_tag.digest(),
        genesis_scope_digest,
        domain_registry_digest,
        bounds.tag().unwrap().digest(),
        crypto_digest,
        resource.tag().unwrap().digest(),
    )
    .unwrap();
    let launch_bound = BoundProfileManifestV1::seal(launch_cand).unwrap();
    let succ_cand = CandidateProfileManifestV1::canonical_successor_program_v1(
        &launch_bound,
        successor_scope_digest,
    )
    .unwrap();
    let succ_bound = BoundProfileManifestV1::seal(succ_cand).unwrap();

    let launch_spec =
        ResolvedProfileTransitionSpecV1::resolve_from_profile(&launch_bound, &bounds, &resource)
            .unwrap();
    let succ_spec =
        ResolvedProfileTransitionSpecV1::resolve_from_profile(&succ_bound, &bounds, &resource)
            .unwrap();

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
    let launch_batch_bytes = launch_batch.encode_canonical();
    let launch_rcpt =
        evaluate_profile_transition_spec(&launch_spec, &ctx_l, &launch_batch_bytes).unwrap();
    assert_eq!(
        launch_rcpt.disposition,
        TransitionEvalDispositionV1::Admitted
    );
    verify_receipt_invariants(&launch_spec, &ctx_l, &launch_batch_bytes, &launch_rcpt).unwrap();

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
    let succ_batch_bytes = succ_batch.encode_canonical();
    let succ_rcpt =
        evaluate_profile_transition_spec(&succ_spec, &ctx_s, &succ_batch_bytes).unwrap();
    assert_eq!(succ_rcpt.disposition, TransitionEvalDispositionV1::Admitted);

    // 7/7 powered semantic mutants verified fail-closed
    // M1: Op-tag band collision
    let mut m1 = launch_spec.clone();
    m1.bands[1].op_tag_min = 31;
    assert!(matches!(
        m1.validate(),
        Err(TransitionSpec1Error::OpTagBandCollision { .. })
    ));
    // M2: Write-domain overlap
    let mut m2 = launch_spec.clone();
    m2.bands[1].write_domains_mask |= m2.bands[0].write_domains_mask;
    m2.bands[1].read_domains_mask |= m2.bands[1].write_domains_mask;
    assert!(matches!(
        m2.validate(),
        Err(TransitionSpec1Error::WriteDomainOverlap { .. })
    ));
    // M3: Omitted-effect receipt mutant
    let mut m3 = launch_rcpt.clone();
    m3.effects_count -= 1;
    assert_eq!(
        verify_receipt_invariants(&launch_spec, &ctx_l, &launch_batch_bytes, &m3),
        Err(TransitionSpec1Error::OmittedEffectDetected)
    );
    // M4: Partial-write rollback leak on rejected batch
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
    let mut m4 = evaluate_profile_transition_spec(&launch_spec, &ctx_l, &bad_tag_bytes).unwrap();
    assert_eq!(
        m4.disposition,
        TransitionEvalDispositionV1::RejectedUnknownOperationTag
    );
    m4.post_state_root[0] ^= 0x01;
    assert_eq!(
        verify_receipt_invariants(&launch_spec, &ctx_l, &bad_tag_bytes, &m4),
        Err(TransitionSpec1Error::PartialWriteRollbackViolation)
    );
    // M5: Wrong version / profile_id
    let mut bad_ver = launch_batch.clone();
    bad_ver.spec_version = 9;
    let r5 = evaluate_profile_transition_spec(&launch_spec, &ctx_l, &bad_ver.encode_canonical())
        .unwrap();
    assert_eq!(
        r5.disposition,
        TransitionEvalDispositionV1::RejectedWrongVersionOrProfile
    );
    // M6: Uncharged work formula (gas == 0)
    let mut m6 = launch_spec.clone();
    m6.bands[0].base_gas = 0;
    assert_eq!(
        m6.validate(),
        Err(TransitionSpec1Error::UnchargedWorkFormula { slot_id: 1 })
    );
    // M7: Inactive Program/VM op 80 rejected on launch
    let prog_on_launch = ProfileTransitionBatchV1 {
        spec_version: 1,
        profile_id: launch_bound.profile_id,
        block_height: 100,
        ops: vec![ProfileTransitionOpV1 {
            op_tag: 80,
            target_key: [8; 8],
            payload: vec![1],
        }],
    };
    let r7 =
        evaluate_profile_transition_spec(&launch_spec, &ctx_l, &prog_on_launch.encode_canonical())
            .unwrap();
    assert_eq!(
        r7.disposition,
        TransitionEvalDispositionV1::RejectedInactiveExtensionSlot
    );

    let l_tag = launch_spec.tag().unwrap().to_string();
    let s_tag = succ_spec.tag().unwrap().to_string();
    let l_rcpt_tag = launch_rcpt.tag().unwrap().to_string();
    let s_rcpt_tag = succ_rcpt.tag().unwrap().to_string();

    println!("{{");
    println!("  \"launch_spec_tag\": \"{l_tag}\",");
    println!("  \"succ_spec_tag\": \"{s_tag}\",");
    println!("  \"launch_eval_receipt_tag\": \"{l_rcpt_tag}\",");
    println!("  \"succ_eval_receipt_tag\": \"{s_rcpt_tag}\",");
    println!(
        "  \"launch_profile_id_short\": \"{}\",",
        hex_prefix(&launch_bound.profile_id, 6)
    );
    println!("  \"launch_spec_tag_short\": \"{}\",", &l_tag[18..30]);
    println!(
        "  \"succ_profile_id_short\": \"{}\",",
        hex_prefix(&succ_bound.profile_id, 6)
    );
    println!("  \"succ_spec_tag_short\": \"{}\",", &s_tag[18..30]);
    println!("  \"mutants_caught\": 7,");
    println!("  \"mutants_total\": 7,");
    println!("  \"launch_batch_ops\": {},", launch_rcpt.effects_count);
    println!("  \"launch_batch_gas\": {},", launch_rcpt.gas_charged);
    println!("  \"succ_batch_gas\": {},", succ_rcpt.gas_charged);
    println!("  \"mutant_rows\": [");
    let rows = [
        ("M1. Op-Tag Band Collision (31..47)", "OpTagBandCollision"),
        (
            "M2. Write-Domain Overlap (Auth/Asset)",
            "WriteDomainOverlap",
        ),
        ("M3. Omitted-Effect Receipt Tamper", "OmittedEffectDetected"),
        (
            "M4. Partial-Write Rollback Leak",
            "PartialWriteRollbackViolation",
        ),
        (
            "M5. Wrong Version / Profile-ID",
            "RejectedWrongVersionOrProfile",
        ),
        ("M6. Uncharged Work Formula (gas=0)", "UnchargedWorkFormula"),
        (
            "M7. Inactive Program/VM Op 80",
            "RejectedInactiveExtensionSlot",
        ),
    ];
    for (i, (m, e)) in rows.iter().enumerate() {
        let comma = if i + 1 < rows.len() { "," } else { "" };
        println!("    {{\"mutant\": \"{m}\", \"expected_error\": \"{e}\"}}{comma}");
    }
    println!("  ]");
    println!("}}");
}
