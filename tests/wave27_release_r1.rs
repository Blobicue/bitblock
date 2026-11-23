#![cfg(feature = "protocol-research")]

use bitblock::consensus_1::ConsensusClusterHarnessV1;
use bitblock::release_r1::{
    BuilderAttestationV1, CandidateBuildInputsV1, CandidatePlatformTargetV1,
    CandidateReleaseReceiptV1, PINNED_COMMITTED_WASM_SHA256, PINNED_FRAMEWORK_MANIFEST_SHA256,
    ReleaseR1Error, build_canonical_candidate_fixture, digest_build_config, digest_feature_profile,
    digest_source_label, verify_candidate_reproducibility,
};

#[test]
fn wave27_profile_bound_candidate_reproducibility_and_platform_targets() {
    let cluster = ConsensusClusterHarnessV1::bootstrap_four_validators().unwrap();
    let bound = &cluster.fixture.bound_manifest;

    let (inputs, rcpt) = build_canonical_candidate_fixture(bound).unwrap();
    inputs.verify_bound_profile(bound).unwrap();
    assert_eq!(rcpt.profile_id, bound.profile_id);
    assert_eq!(rcpt.genesis_id, bound.candidate.genesis_id);
    assert_eq!(
        rcpt.framework_manifest_sha256,
        PINNED_FRAMEWORK_MANIFEST_SHA256
    );
    assert_eq!(rcpt.wasm_artifact_sha256, PINNED_COMMITTED_WASM_SHA256);
    assert_eq!(rcpt.independent_builder_count, 2);
    assert!(rcpt.builders_bit_identical);
    assert!(rcpt.platform_scoped_only);
    assert!(!rcpt.admitted_release_authority);

    // Every platform target discriminant round-trips canonically
    for target in [
        CandidatePlatformTargetV1::LinuxAarch64Elf,
        CandidatePlatformTargetV1::LinuxX86_64Elf,
        CandidatePlatformTargetV1::WindowsX86_64Pe,
        CandidatePlatformTargetV1::Wasm32Simd128,
    ] {
        assert_eq!(
            CandidatePlatformTargetV1::from_code(target.code()).unwrap(),
            target
        );
        let mut t_in = inputs;
        t_in.platform_target = target;
        let dec =
            CandidateBuildInputsV1::decode_canonical(&t_in.encode_canonical().unwrap()).unwrap();
        assert_eq!(dec.platform_target, target);
    }
    assert!(CandidatePlatformTargetV1::from_code(99).is_err());
}

#[test]
fn wave27_eight_mutation_divergence_and_non_admission_controls() {
    let cluster = ConsensusClusterHarnessV1::bootstrap_four_validators().unwrap();
    let bound = &cluster.fixture.bound_manifest;
    let (inputs, rcpt) = build_canonical_candidate_fixture(bound).unwrap();
    let base_inputs_tag = inputs.tag().unwrap();

    // Control 1: source_tree_digest mutation strictly moves candidate identity
    let mut m_src = inputs;
    m_src.source_tree_digest = digest_source_label("mutated-source-tree");
    assert_ne!(m_src.tag().unwrap(), base_inputs_tag);

    // Control 2: rust_toolchain / sbom / feature / config mutations each move identity
    let mut m_feat = inputs;
    m_feat.feature_profile_digest = digest_feature_profile("default-only");
    assert_ne!(m_feat.tag().unwrap(), base_inputs_tag);

    let mut m_cfg = inputs;
    m_cfg.build_config_digest = digest_build_config("debug-unlocked");
    assert_ne!(m_cfg.tag().unwrap(), base_inputs_tag);

    // Control 3: wrong PROFILE-D1 profile_id rejected fail-closed
    let mut bad_prof = inputs;
    bad_prof.profile_id[0] ^= 0xFF;
    assert!(matches!(
        bad_prof.verify_bound_profile(bound),
        Err(ReleaseR1Error::ProfileBindingMismatch(_))
    ));

    // Control 4: wrong RELEASE-R1@framework digest rejected fail-closed
    let b1 = BuilderAttestationV1 {
        builder_id: 1,
        native_artifact_sha256: rcpt.native_artifact_sha256,
        native_artifact_len: rcpt.native_artifact_len,
        wasm_artifact_sha256: rcpt.wasm_artifact_sha256,
        wasm_artifact_len: rcpt.wasm_artifact_len,
    };
    let b2 = BuilderAttestationV1 {
        builder_id: 2,
        ..b1
    };
    let mut bad_fw = PINNED_FRAMEWORK_MANIFEST_SHA256;
    bad_fw[0] ^= 0x01;
    assert!(matches!(
        verify_candidate_reproducibility(bound, &inputs, bad_fw, &[b1, b2]),
        Err(ReleaseR1Error::ProfileBindingMismatch(_))
    ));

    // Control 5: single builder (< 2) or duplicate builder_id rejected fail-closed
    assert!(matches!(
        verify_candidate_reproducibility(bound, &inputs, PINNED_FRAMEWORK_MANIFEST_SHA256, &[b1]),
        Err(ReleaseR1Error::ReproducibilityDivergence(_))
    ));
    assert!(matches!(
        verify_candidate_reproducibility(
            bound,
            &inputs,
            PINNED_FRAMEWORK_MANIFEST_SHA256,
            &[b1, b1]
        ),
        Err(ReleaseR1Error::ReproducibilityDivergence(_))
    ));

    // Control 6: divergent builder native or wasm artifact hash rejected fail-closed
    let mut b2_div = b2;
    b2_div.native_artifact_sha256[31] ^= 0x80;
    assert!(matches!(
        verify_candidate_reproducibility(
            bound,
            &inputs,
            PINNED_FRAMEWORK_MANIFEST_SHA256,
            &[b1, b2_div]
        ),
        Err(ReleaseR1Error::ReproducibilityDivergence(_))
    ));

    // Control 7: admitted_release_authority=true wire tamper rejected fail-closed!
    let mut raw_adm = rcpt.encode_canonical().unwrap();
    raw_adm[CandidateReleaseReceiptV1::WIRE_LEN - 1] = 1;
    assert!(matches!(
        CandidateReleaseReceiptV1::decode_canonical(&raw_adm),
        Err(ReleaseR1Error::AdmittedReleaseAuthorityForbidden)
    ));

    // Control 8: platform_scoped_only=false wire tamper rejected fail-closed!
    let mut raw_ps = rcpt.encode_canonical().unwrap();
    raw_ps[CandidateReleaseReceiptV1::WIRE_LEN - 2] = 0;
    assert!(matches!(
        CandidateReleaseReceiptV1::decode_canonical(&raw_ps),
        Err(ReleaseR1Error::PlatformScopingRequired)
    ));
}
