#![cfg(feature = "protocol-research")]

use bitblock::consensus_1::ConsensusClusterHarnessV1;
use bitblock::crypto_d1::{
    CanonicalEd25519PublicKeyV1, CanonicalEd25519SignatureV1, HashSuiteIdV1, SignatureSuiteIdV1,
    SigningDomainTagV1, SigningPreimageV1, sign_preimage_deterministic,
};
use bitblock::genesis_root_parity_1::{
    CanonicalDevnetGenesisFixtureV1, GenesisParityInputV1, verify_genesis_root_parity,
};
use bitblock::profile_1::build_canonical_devnet_profile_admission;
use bitblock::release_1::{
    AdmittedReleaseCoreV1, AdmittedReleaseRecordV1, RELEASE_1_AUTH_SIGN_DOC_SCHEMA,
    RELEASE_1_CORE_SCHEMA, RELEASE_1_RECORD_SCHEMA, RELEASE_1_SIGN_DOC_SCHEMA, RELEASE_1_VERSION,
    Release1Error, ReleaseAuthorityContextRefV1, ReleaseAuthorizationSignDocEnvelopeV1,
    ReleaseHistoricalAuthorityRefV1, ReleaseParentPurposeCodeV1, ReleaseSignDocV1,
    ReleaseSignerSignatureV1, SuccessorReleaseCertContextV1, TrustedReleaseAuthorityPolicyV1,
    VulnerabilityWithdrawalStatusV1, build_canonical_devnet_admitted_release,
    resolve_trusted_release_authority, verify_admitted_release_record,
};
use bitblock::release_r1::build_canonical_candidate_fixture;

#[test]
fn wave29_admitted_release_3_of_5_threshold_and_candidate_binding() {
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
    let (_closure, manifest) = build_canonical_devnet_profile_admission(
        &fx.package,
        &fx.bound_manifest,
        &fx.transition_spec,
        &parity.record,
    )
    .unwrap();
    let (cand_in, cand_rcpt) = build_canonical_candidate_fixture(&fx.bound_manifest).unwrap();

    let (policy, record) = build_canonical_devnet_admitted_release(
        &fx.package,
        &manifest,
        &parity.record,
        &cand_in,
        &cand_rcpt,
    )
    .unwrap();

    assert_eq!(record.schema_version, RELEASE_1_VERSION);
    assert_eq!(RELEASE_1_CORE_SCHEMA, b"bitblock/admitted-release-core/v1");
    assert_eq!(RELEASE_1_SIGN_DOC_SCHEMA, b"bitblock/release-sign-doc/v1");
    assert_eq!(
        RELEASE_1_AUTH_SIGN_DOC_SCHEMA,
        b"bitblock/release-authorization-sign-doc/v1"
    );
    assert_eq!(
        RELEASE_1_RECORD_SCHEMA,
        b"bitblock/admitted-release-record/v1"
    );

    // Round-trip all enums & wire formats
    for st in [
        VulnerabilityWithdrawalStatusV1::Active,
        VulnerabilityWithdrawalStatusV1::WithdrawnSecurityAdvisory,
        VulnerabilityWithdrawalStatusV1::Superseded,
    ] {
        assert_eq!(
            VulnerabilityWithdrawalStatusV1::from_code(st.code()).unwrap(),
            st
        );
    }
    for pc in [
        ReleaseParentPurposeCodeV1::GenesisLaunch,
        ReleaseParentPurposeCodeV1::EnvelopeParentAuthority,
        ReleaseParentPurposeCodeV1::SuccessorReleaseAuthority,
        ReleaseParentPurposeCodeV1::SuccessorActivationAuthority,
    ] {
        assert_eq!(
            ReleaseParentPurposeCodeV1::from_code(pc.code()).unwrap(),
            pc
        );
    }
    assert_eq!(
        AdmittedReleaseCoreV1::decode_canonical(&record.core.encode_canonical().unwrap()).unwrap(),
        record.core
    );
    assert_eq!(
        ReleaseSignDocV1::decode_canonical(
            &record.authorization.sign_doc.encode_canonical().unwrap()
        )
        .unwrap(),
        record.authorization.sign_doc
    );
    assert_eq!(
        AdmittedReleaseRecordV1::decode_canonical(&record.encode_canonical().unwrap()).unwrap(),
        record
    );
    let rec_id = verify_admitted_release_record(
        &record,
        &fx.package,
        &manifest,
        &parity.record,
        &cand_in,
        &cand_rcpt,
        &policy,
        None,
    )
    .unwrap();
    assert_eq!(rec_id, record.tag().unwrap().digest());
}

#[test]
fn wave29_ten_release_forgery_withdrawal_and_successor_authority_controls() {
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
    let (_closure, manifest) = build_canonical_devnet_profile_admission(
        &fx.package,
        &fx.bound_manifest,
        &fx.transition_spec,
        &parity.record,
    )
    .unwrap();
    let (cand_in, cand_rcpt) = build_canonical_candidate_fixture(&fx.bound_manifest).unwrap();
    let (policy, base_rec) = build_canonical_devnet_admitted_release(
        &fx.package,
        &manifest,
        &parity.record,
        &cand_in,
        &cand_rcpt,
    )
    .unwrap();

    // Control 1: Wrong RELEASE-R1 candidate artifact digest fails closed
    let mut bad_rcpt = cand_rcpt;
    bad_rcpt.native_artifact_sha256[0] ^= 0xFF;
    assert!(matches!(
        verify_admitted_release_record(
            &base_rec,
            &fx.package,
            &manifest,
            &parity.record,
            &cand_in,
            &bad_rcpt,
            &policy,
            None,
        ),
        Err(Release1Error::BindingMismatch(_))
    ));

    // Control 2: Wrong PROFILE-1 admission manifest / stale admission fails closed
    let mut stale_manifest = manifest.clone();
    stale_manifest.evidence_closure_digest[0] ^= 0x01;
    assert!(matches!(
        verify_admitted_release_record(
            &base_rec,
            &fx.package,
            &stale_manifest,
            &parity.record,
            &cand_in,
            &cand_rcpt,
            &policy,
            None,
        ),
        Err(Release1Error::BindingMismatch(_))
    ));

    // Control 3: Wrong platform target fails closed
    let mut wrong_plat = cand_in;
    wrong_plat.platform_target = bitblock::release_r1::CandidatePlatformTargetV1::WindowsX86_64Pe;
    assert!(matches!(
        verify_admitted_release_record(
            &base_rec,
            &fx.package,
            &manifest,
            &parity.record,
            &wrong_plat,
            &cand_rcpt,
            &policy,
            None,
        ),
        Err(Release1Error::BindingMismatch(_))
    ));

    // Control 4: < 3 signatures (2-of-5 shortfall) and duplicate signer replay fail closed
    let mut sub_quorum = base_rec.clone();
    sub_quorum.authorization.signatures.pop();
    assert!(matches!(
        verify_admitted_release_record(
            &sub_quorum,
            &fx.package,
            &manifest,
            &parity.record,
            &cand_in,
            &cand_rcpt,
            &policy,
            None,
        ),
        Err(Release1Error::InvalidReleaseAuthorization(_))
    ));
    let mut dup_signer = base_rec.clone();
    dup_signer.authorization.signatures[2] = dup_signer.authorization.signatures[0];
    assert!(matches!(
        verify_admitted_release_record(
            &dup_signer,
            &fx.package,
            &manifest,
            &parity.record,
            &cand_in,
            &cand_rcpt,
            &policy,
            None,
        ),
        Err(Release1Error::InvalidReleaseAuthorization(_))
    ));

    // Control 5: Unauthorized 6th signer key fails closed
    let rogue_seed = [99u8; 32];
    let rogue_pk = CanonicalEd25519PublicKeyV1::from_secret_seed(&rogue_seed).unwrap();
    let env = ReleaseAuthorizationSignDocEnvelopeV1 {
        sign_doc: base_rec.authorization.sign_doc,
    };
    let pre = env
        .signing_preimage(
            fx.package.state.chain_id,
            fx.package.genesis_id,
            base_rec.core.admitted_profile_id,
        )
        .unwrap();
    let rogue_sig = sign_preimage_deterministic(&rogue_seed, &pre).unwrap();
    let mut unauth = base_rec.clone();
    unauth.authorization.signatures[2] = ReleaseSignerSignatureV1 {
        signer_index: 2,
        public_key: rogue_pk,
        signature: rogue_sig,
    };
    assert!(matches!(
        verify_admitted_release_record(
            &unauth,
            &fx.package,
            &manifest,
            &parity.record,
            &cand_in,
            &cand_rcpt,
            &policy,
            None,
        ),
        Err(Release1Error::InvalidReleaseAuthorization(_))
    ));

    // Control 6: 1-bit signature mutation (both scalar S bit flip & wire R bit flip) fails closed
    let mut bit_mut = base_rec.clone();
    let mut sb = *bit_mut.authorization.signatures[0].signature.as_bytes();
    sb[32] ^= 0x01; // mutates scalar S while remaining < L
    bit_mut.authorization.signatures[0].signature =
        CanonicalEd25519SignatureV1::from_bytes(sb).unwrap();
    assert!(matches!(
        verify_admitted_release_record(
            &bit_mut,
            &fx.package,
            &manifest,
            &parity.record,
            &cand_in,
            &cand_rcpt,
            &policy,
            None,
        ),
        Err(Release1Error::InvalidReleaseAuthorization(_))
    ));
    let mut wire_mut = base_rec.encode_canonical().unwrap();
    let r_off = wire_mut.len() - 64;
    wire_mut[r_off] ^= 0x01;
    assert!(
        AdmittedReleaseRecordV1::decode_canonical(&wire_mut).is_err()
            || verify_admitted_release_record(
                &AdmittedReleaseRecordV1::decode_canonical(&wire_mut).unwrap(),
                &fx.package,
                &manifest,
                &parity.record,
                &cand_in,
                &cand_rcpt,
                &policy,
                None,
            )
            .is_err()
    );

    // Control 7: Cross-domain signature (signed under BlockProposal instead of ReleaseArtifact) fails closed
    let (_, key_pairs) = TrustedReleaseAuthorityPolicyV1::devnet_three_of_five();
    let wrong_dom_pre = SigningPreimageV1 {
        protocol_version: RELEASE_1_VERSION,
        chain_id: fx.package.state.chain_id,
        genesis_id: fx.package.genesis_id,
        launch_manifest_id: [0u8; 32],
        profile_id: base_rec.core.admitted_profile_id,
        domain_tag: SigningDomainTagV1::BlockProposal, // WRONG domain!
        signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
        hash_suite: HashSuiteIdV1::Sha256V1,
        payload_digest: env.tag().unwrap().digest(),
    };
    let mut wrong_dom = base_rec.clone();
    wrong_dom.authorization.signatures[0].signature =
        sign_preimage_deterministic(&key_pairs[0].1, &wrong_dom_pre).unwrap();
    assert!(matches!(
        verify_admitted_release_record(
            &wrong_dom,
            &fx.package,
            &manifest,
            &parity.record,
            &cand_in,
            &cand_rcpt,
            &policy,
            None,
        ),
        Err(Release1Error::InvalidReleaseAuthorization(_))
    ));

    // Control 8: WithdrawnSecurityAdvisory / Superseded releases blocked fail-closed from deployment
    let mut withdrawn = base_rec.clone();
    withdrawn.core.vulnerability_status =
        VulnerabilityWithdrawalStatusV1::WithdrawnSecurityAdvisory;
    assert!(matches!(
        verify_admitted_release_record(
            &withdrawn,
            &fx.package,
            &manifest,
            &parity.record,
            &cand_in,
            &cand_rcpt,
            &policy,
            None,
        ),
        Err(Release1Error::ReleaseNotDeployable(
            VulnerabilityWithdrawalStatusV1::WithdrawnSecurityAdvisory
        ))
    ));

    // Control 9: Core release_authority_policy_id disagrees with committed G_0 authority fails closed
    let mut bad_pol = policy.clone();
    bad_pol.threshold_m = 4; // different committed policy!
    assert!(matches!(
        verify_admitted_release_record(
            &base_rec,
            &fx.package,
            &manifest,
            &parity.record,
            &cand_in,
            &cand_rcpt,
            &bad_pol,
            None,
        ),
        Err(Release1Error::AuthorityPolicyMismatch(_))
    ));

    // Control 10: AcceptedParentReleaseAuthorityV1 purpose-swap & replay-archive substitution fail closed
    let mut cluster = ConsensusClusterHarnessV1::bootstrap_four_validators().unwrap();
    let pre_wr = cluster.nodes[0].harness().snapshot().unwrap().world_root;
    let b1 = cluster
        .fixture
        .canonical_mixed_race_bundle(1, pre_wr)
        .unwrap();
    let commit_cert = bitblock::consensus_1::consensus_net::run_loopback_four_validator_round(
        &mut cluster,
        &b1,
        0,
    )
    .unwrap();
    let (live_cert, _) = cluster
        .build_and_admit_canonical_live_parent_authority_cert(
            &commit_cert,
            bitblock::consensus_1::ParentAuthorityPurposeV1::ReleaseAuthority,
            1,
            1,
        )
        .unwrap();
    let succ_ctx_swap = ReleaseAuthorityContextRefV1::AcceptedParentReleaseAuthorityV1 {
        predecessor_accepted_parent_lookup_id: [0x22; 32],
        purpose_code: ReleaseParentPurposeCodeV1::SuccessorActivationAuthority, // SWAPPED!
        predecessor_governance_authority_root: [0x33; 32],
    };
    let succ_hist =
        ReleaseHistoricalAuthorityRefV1::AcceptedParentReleaseHistoricalAuthorityRefV1 {
            parent_cert_id: live_cert.tag().unwrap().digest(),
            evidence_commitment: live_cert.evidence_commitment_id,
            occurrence_universe_root: live_cert.occurrence_universe_root,
            height: 1,
            epoch: 0,
            purpose_code: ReleaseParentPurposeCodeV1::SuccessorReleaseAuthority,
        };
    assert!(matches!(
        resolve_trusted_release_authority(
            &fx.package,
            &policy,
            &succ_ctx_swap,
            &succ_hist,
            Some(&SuccessorReleaseCertContextV1 {
                live_parent_cert: &live_cert,
                from_replay_archive: false,
            }),
        ),
        Err(Release1Error::SuccessorAuthorityRejected(_))
    ));
    let succ_ctx_ok = ReleaseAuthorityContextRefV1::AcceptedParentReleaseAuthorityV1 {
        predecessor_accepted_parent_lookup_id: [0x22; 32],
        purpose_code: ReleaseParentPurposeCodeV1::SuccessorReleaseAuthority,
        predecessor_governance_authority_root: [0x33; 32],
    };
    // Replay-archive sourced cert MUST reject fail-closed!
    assert!(matches!(
        resolve_trusted_release_authority(
            &fx.package,
            &policy,
            &succ_ctx_ok,
            &succ_hist,
            Some(&SuccessorReleaseCertContextV1 {
                live_parent_cert: &live_cert,
                from_replay_archive: true,
            }),
        ),
        Err(Release1Error::SuccessorAuthorityRejected(_))
    ));
    // Missing live cert MUST reject fail-closed!
    assert!(matches!(
        resolve_trusted_release_authority(&fx.package, &policy, &succ_ctx_ok, &succ_hist, None),
        Err(Release1Error::SuccessorAuthorityRejected(_))
    ));
    // Exact valid live consensus-native cert resolves cleanly!
    assert_eq!(
        resolve_trusted_release_authority(
            &fx.package,
            &policy,
            &succ_ctx_ok,
            &succ_hist,
            Some(&SuccessorReleaseCertContextV1 {
                live_parent_cert: &live_cert,
                from_replay_archive: false,
            }),
        )
        .unwrap(),
        policy.policy_id().unwrap()
    );
}
