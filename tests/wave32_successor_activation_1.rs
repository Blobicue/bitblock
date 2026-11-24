#![cfg(feature = "protocol-research")]

use std::path::Path;

use bitblock::consensus_1::{
    AcceptedConsensusNativeParentAuthorityCertificateV1, ParentAuthorityPurposeV1,
    ParentAuthorityStatusV1,
};
use bitblock::release_1::ReleaseParentPurposeCodeV1;
use bitblock::successor_activation_1::{
    ActivationSignerSignatureV1, AuthorizedMigrationSystemTransitionV1, DedicatedMigrationBodyV1,
    EXPLICIT_SUCCESSOR_ARTIFACT_COUNT, INTERNAL_DEP_REF_TAG_V1, SUCCESSOR_ACTIVATION_1_AUTH_SCHEMA,
    SUCCESSOR_ACTIVATION_1_BUNDLE_SCHEMA, SUCCESSOR_ACTIVATION_1_CARRIER_SCHEMA,
    SUCCESSOR_ACTIVATION_1_SIGN_DOC_SCHEMA, SUCCESSOR_EMBEDDED_CARRIER_MODE_V1,
    SUCCESSOR_EXTERNAL_ROOT_COUNT, SuccessorActivation1Error, SuccessorActivationBundleV1,
    SuccessorActivationCarrierV1, SuccessorPreflightCapsV1,
    SupplementarySuccessorActivationArtifactV1, build_canonical_devnet_successor_activation,
    preflight_successor_activation_bundle_bytes, verify_successor_activation_carrier,
};

fn recompute_carrier_avail(carrier: &mut SuccessorActivationCarrierV1, old_profile_id: [u8; 32]) {
    if let SuccessorActivationCarrierV1::EmbeddedSuccessorActivationV1 {
        bundle,
        successor_activation_bundle_id,
        authorization,
        activation_authorization_id,
        dedicated_migration_body,
        availability_commitment,
    } = carrier
    {
        if let Ok(b_id) = bundle.tag().map(|t| t.digest()) {
            *successor_activation_bundle_id = b_id;
        }
        if let Ok(a_id) = authorization.tag().map(|t| t.digest()) {
            *activation_authorization_id = a_id;
        }
        if let (Ok(bb), Ok(ab), Ok(db)) = (
            bundle.encode_canonical(),
            authorization.encode_canonical(),
            dedicated_migration_body.encode_canonical(),
        ) {
            *availability_commitment =
                SuccessorActivationCarrierV1::compute_availability_commitment(
                    old_profile_id,
                    &bb,
                    &ab,
                    &db,
                );
        }
    }
}

#[test]
fn wave32_successor_activation_1_end_to_end_s3_and_s0() {
    let s3 = build_canonical_devnet_successor_activation(true).expect("s3 build");
    let s0 = build_canonical_devnet_successor_activation(false).expect("s0 build");

    let out_s3 = verify_successor_activation_carrier(
        &s3.carrier,
        &s3.base_context,
        &s3.release_parent_cert,
        &s3.activation_parent_cert,
        SuccessorPreflightCapsV1::V1_DEFAULT,
    )
    .expect("s3 verify");
    assert_eq!(out_s3.preflight.internal_artifact_count_u32, 10);
    assert_eq!(
        out_s3.preflight.supplementary_internal_artifact_count_u32,
        3
    );
    assert_eq!(out_s3.preflight.internal_dependency_edge_count_u32, 23);
    assert_eq!(
        out_s3.release_stage.consumer_context.purpose,
        ParentAuthorityPurposeV1::ReleaseAuthority
    );
    assert_eq!(
        out_s3.activation_stage.consumer_context.purpose,
        ParentAuthorityPurposeV1::ActivationAuthority
    );
    assert_ne!(
        out_s3.release_stage.release_cert_id,
        out_s3.activation_stage.activation_cert_id
    );
    assert_eq!(
        out_s3
            .activation_stage
            .consumer_context
            .consumer_object_digest,
        out_s3.activation_stage.successor_activation_bundle_id
    );
    assert_eq!(
        out_s3.activation_height,
        s3.upgrade_outcome.migration_spec.activation_height
    );

    let out_s0 = verify_successor_activation_carrier(
        &s0.carrier,
        &s0.base_context,
        &s0.release_parent_cert,
        &s0.activation_parent_cert,
        SuccessorPreflightCapsV1::V1_DEFAULT,
    )
    .expect("s0 verify");
    assert_eq!(out_s0.preflight.internal_artifact_count_u32, 7);
    assert_eq!(
        out_s0.preflight.supplementary_internal_artifact_count_u32,
        0
    );
    assert_eq!(out_s0.preflight.internal_dependency_edge_count_u32, 15);
    assert_eq!(EXPLICIT_SUCCESSOR_ARTIFACT_COUNT, 7);
    assert_eq!(SUCCESSOR_EXTERNAL_ROOT_COUNT, 0);
    assert_eq!(
        SUCCESSOR_ACTIVATION_1_BUNDLE_SCHEMA,
        b"bitblock/successor-activation-bundle/v1"
    );
    assert_eq!(
        SUCCESSOR_ACTIVATION_1_SIGN_DOC_SCHEMA,
        b"bitblock/successor-activation-sign-doc/v1"
    );
    assert_eq!(
        SUCCESSOR_ACTIVATION_1_AUTH_SCHEMA,
        b"bitblock/successor-activation-authorization/v1"
    );
    assert_eq!(
        SUCCESSOR_ACTIVATION_1_CARRIER_SCHEMA,
        b"bitblock/successor-activation-1/carrier/v1"
    );
}

#[test]
fn wave32_successor_activation_1_sixteen_fail_closed_adversarial_mutants() {
    let s3 = build_canonical_devnet_successor_activation(true).expect("s3 build");
    let raw_bundle = s3.bundle.encode_canonical().expect("raw bundle");
    let caps = SuccessorPreflightCapsV1::V1_DEFAULT;
    let old_pid = s3.base_context.old_profile_id;

    // 1. Off-by-one internal_artifact_count_u32 rejected in zero-alloc preflight.
    let mut bad = raw_bundle.clone();
    let off_internal = 2 + 32 * 7;
    bad[off_internal + 3] ^= 0x01;
    assert!(matches!(
        preflight_successor_activation_bundle_bytes(&bad, caps),
        Err(SuccessorActivation1Error::EquationViolation(_))
    ));

    // 2. Off-by-one internal_dependency_edge_count_u32 rejected in preflight.
    let mut bad = raw_bundle.clone();
    let off_edges = off_internal + 8;
    bad[off_edges + 3] ^= 0x01;
    assert!(matches!(
        preflight_successor_activation_bundle_bytes(&bad, caps),
        Err(SuccessorActivation1Error::EquationViolation(_))
    ));

    // 3. u64::MAX explicit preimage byte overflow rejected in preflight.
    let mut bad = raw_bundle.clone();
    let off_expl_len0 = off_internal + 20;
    bad[off_expl_len0..off_expl_len0 + 8].copy_from_slice(&u64::MAX.to_be_bytes());
    assert!(matches!(
        preflight_successor_activation_bundle_bytes(&bad, caps),
        Err(SuccessorActivation1Error::PreflightRejected(_))
    ));

    // 4. Locator carrier mode (tag != 1) rejected before allocation.
    let mut bad = raw_bundle.clone();
    let off_mode0 = off_expl_len0 + 7 * 8;
    assert_eq!(bad[off_mode0], SUCCESSOR_EMBEDDED_CARRIER_MODE_V1);
    bad[off_mode0] = 2;
    assert!(matches!(
        preflight_successor_activation_bundle_bytes(&bad, caps),
        Err(SuccessorActivation1Error::PreflightRejected(_))
    ));

    // 5. External dependency ref tag (tag = 2) injected into D_s rejected in preflight.
    let mut bad = raw_bundle.clone();
    let mut cursor = off_mode0;
    for &l in &s3.bundle.explicit_internal_preimage_lengths_u64 {
        cursor += 1 + 8 + (l as usize);
    }
    cursor += 4; // n_nodes
    let mut patched_dep_tag = false;
    for _ in 0..s3.bundle.internal_dependency_nodes.len() {
        cursor += 4 + 32;
        let nd = u16::from_be_bytes([bad[cursor], bad[cursor + 1]]) as usize;
        cursor += 2;
        if nd > 0 && !patched_dep_tag {
            assert_eq!(bad[cursor], INTERNAL_DEP_REF_TAG_V1);
            bad[cursor] = 2;
            patched_dep_tag = true;
            break;
        }
        cursor += nd * 33;
    }
    assert!(patched_dep_tag);
    assert!(matches!(
        preflight_successor_activation_bundle_bytes(&bad, caps),
        Err(SuccessorActivation1Error::DagViolation(_))
    ));

    // 6. Trailing bytes after bundle rejected in preflight.
    let mut bad = raw_bundle.clone();
    bad.push(0xEE);
    assert!(matches!(
        preflight_successor_activation_bundle_bytes(&bad, caps),
        Err(SuccessorActivation1Error::PreflightRejected(_))
    ));

    // 7. Duplicated prior genesis bytes injected into supplementary S_s rejected.
    let mut bad_carrier = s3.carrier.clone();
    if let SuccessorActivationCarrierV1::EmbeddedSuccessorActivationV1 { bundle, .. } =
        &mut bad_carrier
    {
        let dup_bytes = s3
            .base_context
            .genesis_package
            .encode_canonical()
            .expect("g");
        let dup_id = s3.base_context.genesis_package.genesis_id;
        bundle
            .supplementary_internal_artifacts
            .push(SupplementarySuccessorActivationArtifactV1 {
                typed_artifact_schema_id: b"bitblock/genesis-d1/genesis-package/v1".to_vec(),
                typed_artifact_id: dup_id,
                canonical_artifact_preimage_length_u64: dup_bytes.len() as u64,
                canonical_artifact_preimage_bytes: dup_bytes.clone(),
            });
        bundle
            .supplementary_internal_artifacts
            .sort_by_key(|x| x.typed_artifact_id);
        bundle.supplementary_internal_artifact_count_u32 += 1;
        bundle.internal_artifact_count_u32 += 1;
        bundle.internal_canonical_preimage_bytes_u64 += dup_bytes.len() as u64;
        bundle.internal_dependency_nodes.push(
            bitblock::successor_activation_1::SuccessorActivationInternalDependencyNodeV1 {
                topological_rank_u32: 0,
                typed_internal_artifact_id: dup_id,
                internal_dependency_refs: vec![],
            },
        );
        bundle
            .internal_dependency_nodes
            .sort_by_key(|x| x.typed_internal_artifact_id);
    }
    recompute_carrier_avail(&mut bad_carrier, old_pid);
    assert!(matches!(
        verify_successor_activation_carrier(
            &bad_carrier,
            &s3.base_context,
            &s3.release_parent_cert,
            &s3.activation_parent_cert,
            caps
        ),
        Err(SuccessorActivation1Error::DagViolation(_))
    ));

    // 8. Purpose swap: supply release_parent_cert as activation_parent_cert -> rejected.
    assert!(matches!(
        verify_successor_activation_carrier(
            &s3.carrier,
            &s3.base_context,
            &s3.release_parent_cert,
            &s3.release_parent_cert,
            caps
        ),
        Err(SuccessorActivation1Error::AuthorityStageRejected(_))
    ));

    // 9. Purpose swap in reverse: supply activation_parent_cert as release_parent_cert -> rejected.
    assert!(matches!(
        verify_successor_activation_carrier(
            &s3.carrier,
            &s3.base_context,
            &s3.activation_parent_cert,
            &s3.activation_parent_cert,
            caps
        ),
        Err(SuccessorActivation1Error::AuthorityStageRejected(_))
    ));

    // 10. Replay-archive substitution (`from_replay_archive = true`) in Stage 2 or Stage 3 -> rejected.
    let mut replay_act = s3.activation_parent_cert.clone();
    replay_act.from_replay_archive = true;
    assert!(matches!(
        verify_successor_activation_carrier(
            &s3.carrier,
            &s3.base_context,
            &s3.release_parent_cert,
            &replay_act,
            caps
        ),
        Err(SuccessorActivation1Error::AuthorityStageRejected(_))
    ));

    // 11. Partial occurrence view (omitting one pre-cutoff occurrence receipt) -> rejected.
    let mut partial_act = s3.activation_parent_cert.clone();
    partial_act.occurrence_receipts.pop();
    assert!(matches!(
        verify_successor_activation_carrier(
            &s3.carrier,
            &s3.base_context,
            &s3.release_parent_cert,
            &partial_act,
            caps
        ),
        Err(SuccessorActivation1Error::AuthorityStageRejected(_))
    ));

    // 12. Stage 1 core mismatch between release and activation eligibilities -> rejected.
    let mut bad_base = s3.base_context.clone();
    bad_base
        .verified_activation_parent_eligibility
        .unique_accepted_proposal_core_block_id[0] ^= 0xFF;
    assert!(matches!(
        verify_successor_activation_carrier(
            &s3.carrier,
            &bad_base,
            &s3.release_parent_cert,
            &s3.activation_parent_cert,
            caps
        ),
        Err(SuccessorActivation1Error::AuthorityStageRejected(_))
    ));

    // 13. Tentative status on Stage 1 eligibility -> rejected.
    let mut tent_base = s3.base_context.clone();
    tent_base.verified_release_parent_eligibility.status = ParentAuthorityStatusV1::Tentative;
    assert!(matches!(
        verify_successor_activation_carrier(
            &s3.carrier,
            &tent_base,
            &s3.release_parent_cert,
            &s3.activation_parent_cert,
            caps
        ),
        Err(SuccessorActivation1Error::AuthorityStageRejected(_))
    ));

    // 14. Sub-threshold activation authorization (2-of-5) -> rejected.
    let mut sub_auth = s3.carrier.clone();
    if let SuccessorActivationCarrierV1::EmbeddedSuccessorActivationV1 { authorization, .. } =
        &mut sub_auth
    {
        authorization.signatures.pop();
    }
    recompute_carrier_avail(&mut sub_auth, old_pid);
    assert!(matches!(
        verify_successor_activation_carrier(
            &sub_auth,
            &s3.base_context,
            &s3.release_parent_cert,
            &s3.activation_parent_cert,
            caps
        ),
        Err(SuccessorActivation1Error::InvalidActivationAuthorization(_))
    ));

    // 15. Non-empty transactions inside DedicatedMigrationBodyV1 -> rejected.
    let mut bad_body = s3.carrier.clone();
    if let SuccessorActivationCarrierV1::EmbeddedSuccessorActivationV1 {
        dedicated_migration_body,
        ..
    } = &mut bad_body
    {
        dedicated_migration_body.signed_transaction_count = 1;
    }
    recompute_carrier_avail(&mut bad_body, old_pid);
    assert!(matches!(
        verify_successor_activation_carrier(
            &bad_body,
            &s3.base_context,
            &s3.release_parent_cert,
            &s3.activation_parent_cert,
            caps
        ),
        Err(SuccessorActivation1Error::DedicatedMigrationBodyViolation(
            _
        ))
    ));

    // 16. Tampered availability commitment & NoSuccessorActivationV1 rejected.
    let mut bad_avail = s3.carrier.clone();
    if let SuccessorActivationCarrierV1::EmbeddedSuccessorActivationV1 {
        availability_commitment,
        ..
    } = &mut bad_avail
    {
        availability_commitment[0] ^= 0x01;
    }
    assert!(matches!(
        verify_successor_activation_carrier(
            &bad_avail,
            &s3.base_context,
            &s3.release_parent_cert,
            &s3.activation_parent_cert,
            caps
        ),
        Err(SuccessorActivation1Error::DedicatedMigrationBodyViolation(
            _
        ))
    ));
    assert!(matches!(
        verify_successor_activation_carrier(
            &SuccessorActivationCarrierV1::NoSuccessorActivationV1,
            &s3.base_context,
            &s3.release_parent_cert,
            &s3.activation_parent_cert,
            caps
        ),
        Err(SuccessorActivation1Error::DedicatedMigrationBodyViolation(
            _
        ))
    ));

    // Silence unused imports while pinning types in integration scope.
    let _: fn(
        &AcceptedConsensusNativeParentAuthorityCertificateV1,
    ) -> Result<_, SuccessorActivation1Error> =
        bitblock::successor_activation_1::HistoricalAcceptedParentAuthorityUseRefV1::from_live_cert;
    let _ = (
        ReleaseParentPurposeCodeV1::SuccessorReleaseAuthority,
        std::mem::size_of::<ActivationSignerSignatureV1>(),
        std::mem::size_of::<AuthorizedMigrationSystemTransitionV1>(),
        std::mem::size_of::<DedicatedMigrationBodyV1>(),
        std::mem::size_of::<SuccessorActivationBundleV1>(),
    );
}

#[test]
fn wave32_successor_activation_lean_correspondence() {
    let lean_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("proof/BitblockProof/SuccessorActivationRefinement.lean");
    let src = std::fs::read_to_string(&lean_path).expect("read SuccessorActivationRefinement.lean");
    for required in [
        "crates/research/src/successor_activation_1.rs",
        "theorem succ_dep_nodes_eq_seven_plus_supp",
        "theorem succ_external_roots_strictly_zero",
        "theorem succ_canonical_s3_counts",
        "theorem succ_minimal_s0_counts",
        "theorem succ_dag_no_self_loop",
        "theorem succ_canonical_s3_dag_monotone",
        "theorem succ_minimal_s0_dag_monotone",
        "theorem activation_stage_cannot_feed_into_bundle",
        "theorem two_purpose_certs_strictly_distinct",
        "theorem single_purpose_cert_cannot_satisfy_both",
        "theorem replay_cert_rejected_in_stage2",
        "theorem replay_cert_rejected_in_stage3",
        "theorem ordinary_body_rejected_for_migration",
        "theorem nonempty_tx_rejected_for_migration",
        "theorem neg_external_root_injected_unsat",
        "theorem neg_duplicated_prior_bytes_unsat",
        "theorem neg_swapped_purposes_unsat",
    ] {
        assert!(
            src.contains(required),
            "missing Lean theorem/citation {required} in SuccessorActivationRefinement.lean"
        );
    }
    let s3 = build_canonical_devnet_successor_activation(true).expect("s3");
    let s0 = build_canonical_devnet_successor_activation(false).expect("s0");
    assert_eq!(s3.bundle.internal_artifact_count_u32, 10);
    assert_eq!(s3.bundle.internal_dependency_edge_count_u32, 23);
    assert_eq!(s0.bundle.internal_artifact_count_u32, 7);
    assert_eq!(s0.bundle.internal_dependency_edge_count_u32, 15);
}
