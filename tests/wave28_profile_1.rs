#![cfg(feature = "protocol-research")]

use bitblock::genesis_root_parity_1::{
    CanonicalDevnetGenesisFixtureV1, GenesisParityInputV1, verify_genesis_root_parity,
};
use bitblock::profile_1::{
    MigrationEvidenceBindingV1, PROFILE_1_ADMISSION_MANIFEST_SCHEMA,
    PROFILE_1_EVIDENCE_CLOSURE_SCHEMA, PROFILE_1_VERSION, PredicateContextKindV1,
    PredicateEnvironmentV1, Profile1Error, ProfileAdmissionDispositionV1,
    ProfileAdmissionManifestV1, ProfileAdmissionRequestV1, ProfileClosureEvidenceSetV1,
    ProfileEvidenceNodeEntryV1, ProfileEvidenceStatusV1, ResolvedConditionalEdgeV1,
    RoleGraphEdgeV1, UNCONDITIONAL_PROFILE_1_PREREQS, VerklePrototypeBindingV1,
    admit_candidate_profile, build_canonical_devnet_profile_admission,
    canonical_admission_policy_id, canonical_devnet_launch_evidence_entries,
    canonical_launch_role_graph_edges, digest_capability_assumption,
    digest_capability_loss_condition, verify_role_aware_closure_acyclic,
};
use bitblock::profile_d1::{
    BoundProfileManifestV1, CandidateProfileManifestV1, ProfileDomainSlotV1, SubclaimDispositionV1,
    SubclaimEvidenceStatusV1,
};
use bitblock::transition_spec_1::ResolvedProfileTransitionSpecV1;

#[test]
fn wave28_initial_launch_profile_admission_and_real_evidence_closure() {
    let fx = CanonicalDevnetGenesisFixtureV1::build().unwrap();
    let raw_pkg = fx.package.encode_canonical().unwrap();
    let raw_bnd = fx.bundle.encode_canonical().unwrap();
    let parity = verify_genesis_root_parity(&GenesisParityInputV1 {
        raw_genesis_package: &raw_pkg,
        raw_preimage_bundle: &raw_bnd,
        profile_scope: &fx.profile_scope,
        commit_domain_registry: &fx.commit_domain_registry,
        bound_manifest: &fx.bound_manifest,
        transition_spec: &fx.transition_spec,
        exec_registry: &fx.exec_registry,
    })
    .unwrap();

    let (closure, manifest) = build_canonical_devnet_profile_admission(
        &fx.package,
        &fx.bound_manifest,
        &fx.transition_spec,
        &parity.record,
    )
    .unwrap();

    assert_eq!(closure.closure_version, PROFILE_1_VERSION);
    assert!(closure.initial_launch_upgrade_framework_scoped);
    assert_eq!(closure.evidence_entries.len(), 10);
    assert_eq!(closure.resolved_conditional_edges.len(), 5);
    assert_eq!(manifest.required_capability_ids.len(), 10);
    for req_cap in UNCONDITIONAL_PROFILE_1_PREREQS.split_ascii_whitespace() {
        assert!(
            manifest
                .required_capability_ids
                .iter()
                .any(|c| c == req_cap)
        );
    }
    assert_eq!(
        PROFILE_1_EVIDENCE_CLOSURE_SCHEMA,
        b"bitblock/profile-1/evidence-closure/v1"
    );
    assert_eq!(
        PROFILE_1_ADMISSION_MANIFEST_SCHEMA,
        b"bitblock/profile-admission-manifest/v1"
    );
    assert_eq!(
        manifest.admission_disposition,
        ProfileAdmissionDispositionV1::AdmittedBoundedV1
    );

    // Check enum round-trips and evidence helpers
    for st in [
        ProfileEvidenceStatusV1::PendingOrFailed,
        ProfileEvidenceStatusV1::PassedBounded,
        ProfileEvidenceStatusV1::PassedImplemented,
    ] {
        assert_eq!(ProfileEvidenceStatusV1::from_code(st.code()).unwrap(), st);
    }
    assert!(ProfileEvidenceStatusV1::PassedImplemented.is_passing());
    assert!(!ProfileEvidenceStatusV1::PendingOrFailed.is_passing());
    for ctx in [
        PredicateContextKindV1::LaunchProfile,
        PredicateContextKindV1::TargetProfile,
    ] {
        assert_eq!(PredicateContextKindV1::from_code(ctx.code()).unwrap(), ctx);
    }
    assert_eq!(
        ProfileAdmissionDispositionV1::from_code(ProfileAdmissionDispositionV1::Rejected.code())
            .unwrap(),
        ProfileAdmissionDispositionV1::Rejected
    );
    let ce0: &ResolvedConditionalEdgeV1 = &closure.resolved_conditional_edges[0];
    assert_eq!(ce0.edge_id, "CE-25");

    // Round-trip both canonical wire encodings
    let enc_c = closure.encode_canonical().unwrap();
    let enc_m = manifest.encode_canonical().unwrap();
    assert_eq!(
        ProfileClosureEvidenceSetV1::decode_canonical(&enc_c).unwrap(),
        closure
    );
    assert_eq!(
        ProfileAdmissionManifestV1::decode_canonical(&enc_m).unwrap(),
        manifest
    );
    assert_eq!(
        manifest.evidence_closure_digest,
        closure.tag().unwrap().digest()
    );
}

#[test]
fn wave28_ten_adversarial_closure_and_conditional_rejection_controls() {
    let fx = CanonicalDevnetGenesisFixtureV1::build().unwrap();
    let raw_pkg = fx.package.encode_canonical().unwrap();
    let raw_bnd = fx.bundle.encode_canonical().unwrap();
    let parity = verify_genesis_root_parity(&GenesisParityInputV1 {
        raw_genesis_package: &raw_pkg,
        raw_preimage_bundle: &raw_bnd,
        profile_scope: &fx.profile_scope,
        commit_domain_registry: &fx.commit_domain_registry,
        bound_manifest: &fx.bound_manifest,
        transition_spec: &fx.transition_spec,
        exec_registry: &fx.exec_registry,
    })
    .unwrap();

    let base_req = || ProfileAdmissionRequestV1 {
        genesis_package: &fx.package,
        bound_manifest: &fx.bound_manifest,
        transition_spec: &fx.transition_spec,
        parity_record: &parity.record,
        pred_env: PredicateEnvironmentV1 {
            launch_predicates: fx.package.state.launch_predicates.clone(),
            target_predicates: fx.bound_manifest.candidate.activated_predicates.clone(),
        },
        evidence_entries: canonical_devnet_launch_evidence_entries(),
        role_graph_edges: canonical_launch_role_graph_edges(),
        migration_binding: None,
        verkle_binding: None,
        admission_policy_id: canonical_admission_policy_id("bitblock-profile-1-launch-policy-v1"),
    };

    // Control 1: Missing unconditional prerequisite (`BOUNDS-1`) rejected fail-closed
    let mut r1 = base_req();
    r1.evidence_entries
        .retain(|e| e.capability_id != "BOUNDS-1");
    assert!(matches!(
        admit_candidate_profile(&r1),
        Err(Profile1Error::MissingPrerequisite(_))
    ));

    // Control 2: Wrong-version or Pending/Failed evidence rejected fail-closed
    let mut r2 = base_req();
    r2.evidence_entries[0].artifact_version = 2;
    assert!(matches!(
        admit_candidate_profile(&r2),
        Err(Profile1Error::StaleOrWrongVersionEvidence(_))
    ));
    let mut r2b = base_req();
    r2b.evidence_entries[0].evidence_status = ProfileEvidenceStatusV1::PendingOrFailed;
    assert!(matches!(
        admit_candidate_profile(&r2b),
        Err(Profile1Error::MissingPrerequisite(_))
    ));

    // Control 3: CE-26 (`program-active = true`) on SuccessorCandidate missing PROGRAM-CONSENSUS fails closed
    let succ_cand =
        CandidateProfileManifestV1::canonical_successor_program_v1(&fx.bound_manifest, [0x77; 32])
            .unwrap();
    let succ_bound = BoundProfileManifestV1::seal(succ_cand).unwrap();
    let succ_spec = ResolvedProfileTransitionSpecV1::resolve_from_profile(
        &succ_bound,
        &fx.bundle.bounds_profile,
        &fx.bundle.resource_policy,
    )
    .unwrap();
    let mut r3 = ProfileAdmissionRequestV1 {
        genesis_package: &fx.package,
        bound_manifest: &succ_bound,
        transition_spec: &succ_spec,
        parity_record: &parity.record,
        pred_env: PredicateEnvironmentV1 {
            launch_predicates: fx.package.state.launch_predicates.clone(),
            target_predicates: succ_bound.candidate.activated_predicates.clone(),
        },
        evidence_entries: canonical_devnet_launch_evidence_entries(),
        role_graph_edges: canonical_launch_role_graph_edges(),
        migration_binding: None,
        verkle_binding: None,
        admission_policy_id: canonical_admission_policy_id("bitblock-profile-1-launch-policy-v1"),
    };
    assert!(matches!(
        admit_candidate_profile(&r3),
        Err(Profile1Error::MissingPrerequisite(_))
    ));

    // Control 4: CE-29 (`SuccessorCandidate` + `migration-profile`) missing UPGRADE-1 fails closed
    // even once PROGRAM-CONSENSUS is supplied!
    r3.evidence_entries.push(ProfileEvidenceNodeEntryV1 {
        capability_id: "PROGRAM-CONSENSUS".to_string(),
        artifact_version: 1,
        commit_sha1: [0xAB; 20],
        receipt_tag_digest: [0xCD; 32],
        evidence_status: ProfileEvidenceStatusV1::PassedBounded,
        assumptions_digest: digest_capability_assumption("PROGRAM-CONSENSUS", "test"),
        loss_condition_digest: digest_capability_loss_condition("PROGRAM-CONSENSUS", "test"),
    });
    assert!(matches!(
        admit_candidate_profile(&r3),
        Err(Profile1Error::MissingPrerequisite(_))
    ));

    // Control 5: CE-29/CE-30/CE-31 inverted migration polarity (`old == new` or swapped) fails closed;
    // AND InitialLaunch with fabricated migration_binding fails closed!
    r3.migration_binding = Some(MigrationEvidenceBindingV1 {
        old_admitted_profile_id: succ_bound.profile_id, // inverted!
        new_candidate_profile_id: fx.bound_manifest.profile_id,
        upgrade_1_artifact_digest: [0x55; 32],
    });
    assert!(matches!(
        admit_candidate_profile(&r3),
        Err(Profile1Error::MigrationPolarityViolation(_))
    ));
    let mut r5b = base_req();
    r5b.migration_binding = Some(MigrationEvidenceBindingV1 {
        old_admitted_profile_id: [0x11; 32],
        new_candidate_profile_id: fx.bound_manifest.profile_id,
        upgrade_1_artifact_digest: [0x22; 32],
    });
    assert!(matches!(
        admit_candidate_profile(&r5b),
        Err(Profile1Error::MigrationPolarityViolation(_))
    ));

    // Control 6: CE-42 (`verkle-selected = true`) with mismatched CE-40 candidate_profile_id fails closed
    let mut vk_cand = fx.bound_manifest.candidate.clone();
    vk_cand
        .activated_predicates
        .push("verkle-selected".to_string());
    vk_cand.activated_predicates.sort();
    vk_cand.domain_slots = ProfileDomainSlotV1::canonical_slots(true, false, true).to_vec();
    for sc in &mut vk_cand.subclaims {
        if sc.subclaim_id == "G19.verkle" {
            sc.disposition = SubclaimDispositionV1::Required;
            sc.predicate_or_reason = "verkle-selected".to_string();
            sc.evidence_status = SubclaimEvidenceStatusV1::EvidencePending;
        }
    }
    vk_cand.lineage_kind = bitblock::profile_d1::ProfileLineageKindV1::SuccessorCandidate;
    vk_cand.predecessor_profile_id = Some(fx.bound_manifest.profile_id);
    vk_cand.current_scope_digest = [0x99; 32];
    let vk_bound = BoundProfileManifestV1::seal(vk_cand).unwrap();
    let vk_spec = ResolvedProfileTransitionSpecV1::resolve_from_profile(
        &vk_bound,
        &fx.bundle.bounds_profile,
        &fx.bundle.resource_policy,
    )
    .unwrap();
    let mut vk_ev = canonical_devnet_launch_evidence_entries();
    vk_ev.push(ProfileEvidenceNodeEntryV1 {
        capability_id: "UPGRADE-1".to_string(),
        artifact_version: 1,
        commit_sha1: [0x33; 20],
        receipt_tag_digest: [0x44; 32],
        evidence_status: ProfileEvidenceStatusV1::PassedBounded,
        assumptions_digest: digest_capability_assumption("UPGRADE-1", "test"),
        loss_condition_digest: digest_capability_loss_condition("UPGRADE-1", "test"),
    });
    vk_ev.push(ProfileEvidenceNodeEntryV1 {
        capability_id: "VERKLE-R1".to_string(),
        artifact_version: 1,
        commit_sha1: [0x66; 20],
        receipt_tag_digest: [0x77; 32],
        evidence_status: ProfileEvidenceStatusV1::PassedBounded,
        assumptions_digest: digest_capability_assumption("VERKLE-R1", "test"),
        loss_condition_digest: digest_capability_loss_condition("VERKLE-R1", "test"),
    });
    let r6 = ProfileAdmissionRequestV1 {
        genesis_package: &fx.package,
        bound_manifest: &vk_bound,
        transition_spec: &vk_spec,
        parity_record: &parity.record,
        pred_env: PredicateEnvironmentV1 {
            launch_predicates: fx.package.state.launch_predicates.clone(),
            target_predicates: vk_bound.candidate.activated_predicates.clone(),
        },
        evidence_entries: vk_ev,
        role_graph_edges: canonical_launch_role_graph_edges(),
        migration_binding: Some(MigrationEvidenceBindingV1 {
            old_admitted_profile_id: fx.bound_manifest.profile_id,
            new_candidate_profile_id: vk_bound.profile_id,
            upgrade_1_artifact_digest: [0x44; 32],
        }),
        verkle_binding: Some(VerklePrototypeBindingV1 {
            ce40_candidate_profile_id: [0xEE; 32], // wrong candidate ID!
            verkle_prototype_digest: [0x77; 32],
        }),
        admission_policy_id: canonical_admission_policy_id("bitblock-profile-1-launch-policy-v1"),
    };
    assert!(matches!(
        admit_candidate_profile(&r6),
        Err(Profile1Error::VerkleCandidateMismatch)
    ));

    // Control 7: Inactive-domain non-zero sentinel encoding rejected fail-closed
    let (mut bad_closure, manifest) = build_canonical_devnet_profile_admission(
        &fx.package,
        &fx.bound_manifest,
        &fx.transition_spec,
        &parity.record,
    )
    .unwrap();
    bad_closure.domain_slots[6].commitment_profile_id = 1; // inactive ProgramState with non-zero profile!
    assert!(matches!(
        bad_closure.validate(),
        Err(Profile1Error::InactiveDomainEncodingError(_))
    ));

    // Control 8: Role-aware dependency cycle rejected fail-closed
    let mut r8 = base_req();
    r8.role_graph_edges.push(RoleGraphEdgeV1 {
        from_node: "ASSET-1".to_string(),
        from_role: "implementation".to_string(),
        to_node: "PROFILE-1".to_string(),
        to_role: "new-admission".to_string(),
    });
    assert!(matches!(
        verify_role_aware_closure_acyclic(&r8.role_graph_edges),
        Err(Profile1Error::RoleGraphCycle)
    ));
    assert!(matches!(
        admit_candidate_profile(&r8),
        Err(Profile1Error::RoleGraphCycle)
    ));

    // Control 9: Predicate environment context isolation (`always` or missing entailment rejected)
    let bad_env = PredicateEnvironmentV1 {
        launch_predicates: vec!["economic-charging".to_string()], // missing entailed asset-active
        target_predicates: vec!["economic-charging".to_string()],
    };
    assert!(matches!(
        bad_env.validate(bitblock::profile_d1::ProfileLineageKindV1::InitialLaunch),
        Err(Profile1Error::PredicateClosureError(_))
    ));

    // Control 10: Non-passing admission_disposition wire tamper rejected fail-closed
    let mut raw_m = manifest.encode_canonical().unwrap();
    let last = raw_m.len() - 1;
    raw_m[last] = 0; // Rejected!
    assert!(matches!(
        ProfileAdmissionManifestV1::decode_canonical(&raw_m),
        Err(Profile1Error::AdmissionRejected(_))
    ));
}
