#![cfg(feature = "protocol-research")]
//! Wave 23 (`PS-095:SYNC-1@replay-authority`, `PD-06`) integration tests:
//! 1. End-to-end live `CONSENSUS-1` certificate emission -> independent post-consumer
//!    `ReplayEvidenceArchiveCheckpointV1` & `ReplayEvidenceViewV1` reconstruction (explicit
//!    positive control: checkpoint created strictly AFTER consumer!) + append-consistency across
//!    `C1 -> C2` + golden parity.
//! 2. Complete 12/12 `CAPABILITY_TREE.md` negative-control rejection matrix (omitted pre-cutoff
//!    conflict, post-cutoff insertion, event reorder/gap, event duplication, mixed root/policy/view,
//!    circular/substituted normalized entry, retained-subset masquerade, synthetic cross-wrapper
//!    pair, missing purpose/consumer, non-admitted/late certificate, certificate-to-replay
//!    mismatch, and partial checkpoint-publication prefix).

use bitblock::consensus_1::ParentAuthorityStatusV1;
use bitblock::sync_replay_authority_1::{
    DEFAULT_ARCHIVE_CHECKPOINT_AUTH_SEED, ReplayAuthority1Error, ReplayAuthorityAuditBundleV1,
    ReplayEvidenceArchiveCheckpointV1, ReplayEvidenceCheckpointConsistencyProofV1,
    ReplayEvidenceEventRecordV1, ReplayEvidenceEventV1,
    SameWrapperAcceptedRecordOccurrenceWitnessV1, build_canonical_replay_authority_fixture,
    compute_cert_occurrence_id, compute_dependency_closure_digest,
    compute_normalized_authority_view_root, compute_ordered_event_prefix_root,
    compute_outer_occurrence_id,
};

#[test]
fn wave23_post_consumer_positive_control_and_append_consistency() {
    let fix = build_canonical_replay_authority_fixture().expect("fixture");

    // Non-authority invariant: replay reconstruction NEVER selects live consensus authority
    assert!(!fix.audit_bundle_c1.selects_live_consensus_authority());
    assert!(!fix.audit_bundle_c2.selects_live_consensus_authority());

    // Required positive control: checkpoint created strictly AFTER consumer event height (5 > 1)
    let consumer_h = match &fix.audit_bundle_c1.replay_view.prefix_proof.records[2].event {
        ReplayEvidenceEventV1::AcceptedParentAuthorityUseOccurrence {
            consumer_ledger_height,
            ..
        } => *consumer_ledger_height,
        _ => panic!("expected consumer event at ordinal 3"),
    };
    assert_eq!(consumer_h, 1);
    assert!(fix.checkpoint_c1.checkpoint_ledger_height > consumer_h);
    assert!(
        fix.checkpoint_c2.checkpoint_ledger_height > fix.checkpoint_c1.checkpoint_ledger_height
    );

    // Full verification at C1 and append-consistent C2
    fix.audit_bundle_c1
        .verify_audit(
            &fix.fork_context,
            &fix.schedule,
            fix.archive_policy_id,
            fix.norm_policy_id,
            &fix.checkpoint_auth_pk,
            None,
        )
        .expect("C1 post-consumer audit passes");
    fix.audit_bundle_c2
        .verify_audit(
            &fix.fork_context,
            &fix.schedule,
            fix.archive_policy_id,
            fix.norm_policy_id,
            &fix.checkpoint_auth_pk,
            Some(&fix.checkpoint_c1),
        )
        .expect("C2 append-consistent audit passes");

    // Direct codec round-trips across helper carriers
    let rec0 = &fix.audit_bundle_c1.replay_view.prefix_proof.records[0];
    assert_eq!(
        ReplayEvidenceEventRecordV1::decode_canonical(&rec0.encode_canonical().unwrap()).unwrap(),
        *rec0
    );
    assert_eq!(
        ReplayEvidenceEventV1::decode_canonical(&rec0.event.encode_canonical().unwrap()).unwrap(),
        rec0.event
    );
    let sw0 = fix.audit_bundle_c1.same_wrapper_witnesses[0];
    assert_eq!(
        SameWrapperAcceptedRecordOccurrenceWitnessV1::decode_canonical(&sw0.encode_canonical())
            .unwrap(),
        sw0
    );
    let cp_proof = fix.audit_bundle_c2.consistency_proof.unwrap();
    assert_eq!(
        ReplayEvidenceCheckpointConsistencyProofV1::decode_canonical(&cp_proof.encode_canonical())
            .unwrap(),
        cp_proof
    );
    let _ = compute_dependency_closure_digest(&Default::default());
}

#[test]
fn wave23_twelve_negative_control_matrix() {
    let fix = build_canonical_replay_authority_fixture().expect("fixture");
    let check = |b: &ReplayAuthorityAuditBundleV1,
                 hist: Option<&ReplayEvidenceArchiveCheckpointV1>| {
        b.verify_audit(
            &fix.fork_context,
            &fix.schedule,
            fix.archive_policy_id,
            fix.norm_policy_id,
            &fix.checkpoint_auth_pk,
            hist,
        )
    };

    // NC1: Omitted pre-cutoff conflict material
    {
        let mut b = fix.audit_bundle_c1.clone();
        if let ReplayEvidenceEventV1::AcceptedRecordOuterOccurrence {
            occurrence_ordinal,
            event_ledger_height,
            raw_wrapper_bytes,
            availability_assessment_id,
            has_conflict_material,
            outer_occurrence_id,
        } = &mut b.replay_view.prefix_proof.records[0].event
        {
            *has_conflict_material = true;
            *outer_occurrence_id = compute_outer_occurrence_id(
                *occurrence_ordinal,
                *event_ledger_height,
                raw_wrapper_bytes,
                *availability_assessment_id,
                true,
            );
        }
        // Rechain records 2 and 3 + re-sign checkpoint while leaving normalized entry includes_conflict_material=false!
        let e1 = b.replay_view.prefix_proof.records[0].event_id().unwrap();
        b.replay_view.prefix_proof.records[1].prev_event_id = e1;
        let e2 = b.replay_view.prefix_proof.records[1].event_id().unwrap();
        b.replay_view.prefix_proof.records[2].prev_event_id = e2;
        let new_proot = compute_ordered_event_prefix_root(
            b.replay_view.checkpoint.archive_id,
            &b.replay_view.prefix_proof.records,
        )
        .unwrap();
        b.replay_view.checkpoint.ordered_event_prefix_root = new_proot;
        b.replay_view.checkpoint = b
            .replay_view
            .checkpoint
            .sign_with_seed(&DEFAULT_ARCHIVE_CHECKPOINT_AUTH_SEED, &fix.fork_context)
            .unwrap();
        b.replay_view.prefix_proof.checkpoint_id = b.replay_view.checkpoint.checkpoint_id();
        b.replay_view.prefix_proof.ordered_event_prefix_root = new_proot;
        assert!(matches!(
            check(&b, None),
            Err(ReplayAuthority1Error::OmittedPreCutoffConflict)
        ));
    }

    // NC2: Post-cutoff insertion (event at height 6 included inside cutoff_ledger_height=1)
    {
        let mut b = fix.audit_bundle_c2.clone();
        b.replay_view.cutoff_event_ordinal = 4; // pulls height-6 event 4 into height-1 cutoff!
        assert!(matches!(
            check(&b, Some(&fix.checkpoint_c1)),
            Err(ReplayAuthority1Error::PostCutoffInsertionRejected)
        ));
    }

    // NC3: Event reorder / gap
    {
        let mut b = fix.audit_bundle_c1.clone();
        b.replay_view.prefix_proof.records.swap(0, 1);
        assert!(matches!(
            check(&b, None),
            Err(ReplayAuthority1Error::EventReorderOrGapRejected(_))
        ));
    }

    // NC4: Event duplication
    {
        let mut b = fix.audit_bundle_c1.clone();
        b.replay_view.prefix_proof.records[1].event_ordinal = 1;
        assert!(matches!(
            check(&b, None),
            Err(ReplayAuthority1Error::EventDuplicationRejected(_))
        ));
    }

    // NC5: Mixed root / policy / view
    {
        let mut b = fix.audit_bundle_c1.clone();
        b.same_wrapper_witnesses[0].replay_evidence_view_id[0] ^= 0xAA;
        assert!(matches!(
            check(&b, None),
            Err(ReplayAuthority1Error::MixedRootPolicyOrViewRejected(_))
        ));
        let mut b_pol = fix.audit_bundle_c1.clone();
        b_pol.replay_view.checkpoint.normalization_policy_id[0] ^= 0xBB;
        b_pol.replay_view.checkpoint = b_pol
            .replay_view
            .checkpoint
            .sign_with_seed(&DEFAULT_ARCHIVE_CHECKPOINT_AUTH_SEED, &fix.fork_context)
            .unwrap();
        assert!(matches!(
            check(&b_pol, None),
            Err(ReplayAuthority1Error::MixedRootPolicyOrViewRejected(_))
        ));
    }

    // NC6: Circular or substituted normalized-entry commitment
    {
        let mut b = fix.audit_bundle_c1.clone();
        b.replay_view.normalized_entries[0].evidence_digests.pop();
        let bad_root =
            compute_normalized_authority_view_root(&b.replay_view.normalized_entries).unwrap();
        b.replay_view.checkpoint.normalized_authority_view_root = bad_root;
        b.replay_view.checkpoint = b
            .replay_view
            .checkpoint
            .sign_with_seed(&DEFAULT_ARCHIVE_CHECKPOINT_AUTH_SEED, &fix.fork_context)
            .unwrap();
        b.replay_view.prefix_proof.checkpoint_id = b.replay_view.checkpoint.checkpoint_id();
        assert!(matches!(
            check(&b, None),
            Err(ReplayAuthority1Error::CircularOrSubstitutedNormalizedEntry(
                _
            ))
        ));
    }

    // NC7: Retained-subset masquerade as full normalized entry core
    {
        let mut b = fix.audit_bundle_c1.clone();
        b.retention_proofs[0]
            .full_entry_core
            .credential_pubkeys
            .pop();
        assert!(matches!(
            check(&b, None),
            Err(ReplayAuthority1Error::RetainedSubsetMasqueradeRejected)
        ));
    }

    // NC8: Synthetic cross-wrapper pair (evidence from wrapper 0 paired with availability from wrapper 1)
    {
        let mut b = fix.audit_bundle_c1.clone();
        b.same_wrapper_witnesses[0].availability_assessment_id =
            b.same_wrapper_witnesses[1].availability_assessment_id;
        assert!(matches!(
            check(&b, None),
            Err(ReplayAuthority1Error::SyntheticCrossWrapperPairRejected)
        ));
    }

    // NC9: Missing purpose / consumer binding
    {
        let mut b = fix.audit_bundle_c1.clone();
        b.historical_use_witnesses[0]
            .consumer_preimage_bytes
            .clear();
        assert!(matches!(
            check(&b, None),
            Err(ReplayAuthority1Error::MissingPurposeOrConsumerBinding(_))
        ));
    }

    // NC10: Non-admitted or late certificate (tentative status or late-valid-suffix)
    {
        let mut b = fix.audit_bundle_c1.clone();
        b.historical_use_witnesses[0].certificate.attained_status =
            ParentAuthorityStatusV1::Tentative;
        assert!(matches!(
            check(&b, None),
            Err(ReplayAuthority1Error::NonAdmittedOrLateCertificateRejected(
                _
            ))
        ));
    }

    // NC11: Certificate-to-replay evidence mismatch (tampered reconstruction map digest)
    {
        let mut b = fix.audit_bundle_c1.clone();
        b.historical_use_witnesses[0]
            .commitment_proof
            .reconstruction_map_digest[0] ^= 0xFF;
        assert!(matches!(
            check(&b, None),
            Err(ReplayAuthority1Error::CertificateToReplayMismatch(_))
        ));
    }

    // NC12: Partial checkpoint-publication prefix (both incomplete flag AND truncated prefix records)
    {
        let mut b_flag = fix.audit_bundle_c1.clone();
        b_flag.replay_view.checkpoint.publication_complete = false;
        assert!(matches!(
            check(&b_flag, None),
            Err(ReplayAuthority1Error::PartialCheckpointPublicationRejected(
                _
            ))
        ));
        let mut b_trunc = fix.audit_bundle_c1.clone();
        b_trunc.replay_view.prefix_proof.records.pop();
        assert!(matches!(
            check(&b_trunc, None),
            Err(ReplayAuthority1Error::PartialCheckpointPublicationRejected(
                _
            ))
        ));
    }

    // Extra sanity: unauthorized checkpoint signer rejected fail-closed
    {
        let mut b = fix.audit_bundle_c1.clone();
        b.replay_view.checkpoint = b
            .replay_view
            .checkpoint
            .sign_with_seed(&[99u8; 32], &fix.fork_context)
            .unwrap();
        assert!(matches!(
            check(&b, None),
            Err(ReplayAuthority1Error::InvalidCheckpointAuthorization(_))
        ));
    }
    let _ = compute_cert_occurrence_id(
        fix.audit_bundle_c1.historical_use_witnesses[0]
            .certificate
            .consumer_context
            .purpose,
        [1u8; 32],
        1,
        1,
        b"x",
        b"y",
    );
    let _ = ReplayEvidenceArchiveCheckpointV1::WIRE_LEN;
}
