//! Wave 8C CI integration tests for `PROTOCOL-ID-1` (`PS-027`).
#![cfg(feature = "protocol-research")]

use bitblock::crypto_d1::{
    CanonicalEd25519PublicKeyV1, HashSuiteIdV1, SignatureSuiteIdV1, SigningDomainTagV1,
    SigningPreimageV1, sign_preimage_deterministic,
};
use bitblock::profile_bootstrap::{BootstrapError, verify_protocol_id_graph_acyclic};
use bitblock::protocol_id_1::{
    AcceptedProposalWrapperV1, AtomicAvailabilityIndexV1, CanonicalAcceptedProposalCoreV1,
    PROTOCOL_ID_1_CANONICAL_DAG, ParentCollisionRowV1, ProposalCredentialEntryV1, ProtocolId1Error,
    SameWrapperAvailabilityDependencyV1, TwoRowParentCollisionAuditV1, normalize_proposal_wrappers,
    profile_id_hash_parent_lookup, verify_canonical_protocol_id_dag,
};

const EXPECTED_EXACT_CORE_TAG: &str =
    "#bb.receipt.sha256.zaewefk4edydxkwnzxngps7h4szty4oohcrdtdpxwjrzsjxzi53q";
const EXPECTED_WRAPPER_TAG: &str =
    "#bb.receipt.sha256.aumbzbgscvkmaofwnpaqbujimukmiodgv2pbv7w26mexpr5qepxq";
const EXPECTED_AUDIT_ROW2_TAG: &str =
    "#bb.receipt.sha256.awmkppq2clfbrf7qouyfxdeuseo7kktzcrnriii3pm4tkckk6wha";
const EXPECTED_AUDIT_ROW1_TAG: &str =
    "#bb.receipt.sha256.k3k4i5i3xkghpncwnwssp4jklfbsvpnqfwwzynbmqwk6hbdotraq";

#[test]
fn wave8c_protocol_id_1_golden_parity_normalization_and_mutation_corpus() {
    // 1. Canonical 12-node identifier graph acyclicity + self-loop & multi-hop cycle controls.
    verify_canonical_protocol_id_dag().unwrap();
    let mut cyclic = PROTOCOL_ID_1_CANONICAL_DAG.to_vec();
    cyclic.push(("domain_registry_digest", "effect_id"));
    assert_eq!(
        verify_protocol_id_graph_acyclic(&cyclic),
        Err(BootstrapError::IdentifierCycleDetected)
    );
    assert_eq!(
        verify_protocol_id_graph_acyclic(&[("genesis_id", "genesis_id")]),
        Err(BootstrapError::IdentifierCycleDetected)
    );

    // 2. Exact 9-field core + wrapper normalization parity with tools/protocol_id_1_vectors.py.
    let preimage = b"bitblock.devnet.v1:genesis-parent-lookup";
    let lid = profile_id_hash_parent_lookup(preimage);
    let core_a = CanonicalAcceptedProposalCoreV1 {
        accepted_proposal_core_version: 1,
        accepted_parent_lookup_id: lid,
        authority_ref_kind: 1,
        canonical_authority_ref_bytes: b"launch-genesis-ref-v1".to_vec(),
        canonical_proposal_header_bytes: b"block-1-header-canonical".to_vec(),
        canonical_availability_payload_bytes: b"avail-blob-v1".to_vec(),
    };
    let core_b = CanonicalAcceptedProposalCoreV1 {
        canonical_proposal_header_bytes: b"block-1-header-equivocated".to_vec(),
        ..core_a.clone()
    };
    assert_eq!(core_a.tag().unwrap().to_string(), EXPECTED_EXACT_CORE_TAG);

    let core_a_d = core_a.tag().unwrap().digest();
    let sign_doc = SigningPreimageV1 {
        protocol_version: 1,
        chain_id: 1,
        genesis_id: [0x11; 32],
        launch_manifest_id: [0x22; 32],
        profile_id: [0x33; 32],
        domain_tag: SigningDomainTagV1::BlockProposal,
        signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
        hash_suite: HashSuiteIdV1::Sha256V1,
        payload_digest: core_a_d,
    };
    let mk_cred = |s: u8| ProposalCredentialEntryV1 {
        proposer_pubkey: CanonicalEd25519PublicKeyV1::from_secret_seed(&[s; 32]).unwrap(),
        signature: sign_preimage_deterministic(&[s; 32], &sign_doc).unwrap(),
    };
    let w1 = AcceptedProposalWrapperV1 {
        core: core_a.clone(),
        credentials: vec![mk_cred(41)],
        evidence_digests: vec![[0xAA; 32]],
        availability_dependencies: vec![SameWrapperAvailabilityDependencyV1 {
            dependency_id: [0xCC; 32],
            expected_bytes: 4096,
        }],
    };
    let w2 = AcceptedProposalWrapperV1 {
        core: core_a.clone(),
        credentials: vec![mk_cred(42)],
        evidence_digests: vec![[0xBB; 32]],
        availability_dependencies: vec![SameWrapperAvailabilityDependencyV1 {
            dependency_id: [0xCC; 32],
            expected_bytes: 4096,
        }],
    };
    let merged =
        normalize_proposal_wrappers(&w1, &w2, 1, [0x11; 32], [0x22; 32], [0x33; 32]).unwrap();
    assert_eq!(merged.tag().unwrap().to_string(), EXPECTED_WRAPPER_TAG);
    assert_eq!(merged.credentials.len(), 2);

    // Re-normalizing with duplicate proposer credential preserves weight (still 2, never 3).
    let deduped =
        normalize_proposal_wrappers(&merged, &w1, 1, [0x11; 32], [0x22; 32], [0x33; 32]).unwrap();
    assert_eq!(deduped, merged);

    // 3. Atomic same-wrapper availability insertion & conflict rollback.
    let mut avail_idx = AtomicAvailabilityIndexV1::default();
    avail_idx.admit_wrapper(&merged).unwrap();
    let snapshot = avail_idx.clone();
    let mut conflicting_w = w1.clone();
    conflicting_w.availability_dependencies = vec![
        SameWrapperAvailabilityDependencyV1 {
            dependency_id: [0xDD; 32],
            expected_bytes: 1024,
        },
        SameWrapperAvailabilityDependencyV1 {
            dependency_id: [0xCC; 32],
            expected_bytes: 9999, // conflicts with committed 4096
        },
    ];
    conflicting_w
        .availability_dependencies
        .sort_by_key(|d| d.dependency_id);
    assert!(matches!(
        avail_idx.admit_wrapper(&conflicting_w),
        Err(ProtocolId1Error::AvailabilityAtomicityViolation(_))
    ));
    assert_eq!(
        avail_idx, snapshot,
        "conflicting dependency must roll back entire wrapper atomically"
    );

    // 4. Two-row collision disposition (Row 2, Row 1, and InvalidLookupReconstruction non-Row-3).
    let audit_r2 = TwoRowParentCollisionAuditV1::classify_with_hasher(
        lid,
        preimage,
        &core_a,
        preimage,
        &core_b,
        profile_id_hash_parent_lookup,
    )
    .unwrap();
    assert_eq!(
        audit_r2.collision_row,
        ParentCollisionRowV1::ProfileOwnedAuthorityBindingCompromise
    );
    assert_eq!(audit_r2.tag().unwrap().to_string(), EXPECTED_AUDIT_ROW2_TAG);

    let audit_r1 = TwoRowParentCollisionAuditV1::classify_with_hasher(
        lid,
        preimage,
        &core_a,
        b"distinct-colliding-preimage",
        &core_b,
        |_| lid,
    )
    .unwrap();
    assert_eq!(
        audit_r1.collision_row,
        ParentCollisionRowV1::ProtocolIdentityHashCompromise
    );
    assert_eq!(audit_r1.tag().unwrap().to_string(), EXPECTED_AUDIT_ROW1_TAG);

    // Invalid lookup reconstruction rejects as InvalidLookupReconstruction (never a third row).
    assert_eq!(
        TwoRowParentCollisionAuditV1::classify_with_hasher(
            lid,
            b"wrong-preimage-does-not-hash-to-lid",
            &core_a,
            preimage,
            &core_b,
            profile_id_hash_parent_lookup,
        ),
        Err(ProtocolId1Error::InvalidLookupReconstruction)
    );
    // Identical preimage + identical unsigned core is not a collision (use normalization).
    assert_eq!(
        TwoRowParentCollisionAuditV1::classify_with_hasher(
            lid,
            preimage,
            &core_a,
            preimage,
            &core_a,
            profile_id_hash_parent_lookup,
        ),
        Err(ProtocolId1Error::NotACollisionUseWrapperNormalization)
    );

    // 5. Cross-domain / wrong-chain / wrong-version / field-omission / duplicate-field mutation controls.
    assert!(
        w1.verify_credentials(2, [0x11; 32], [0x22; 32], [0x33; 32])
            .is_err()
    );
    assert!(
        w1.verify_credentials(1, [0x99; 32], [0x22; 32], [0x33; 32])
            .is_err()
    );
    assert!(
        w1.verify_credentials(1, [0x11; 32], [0x22; 32], [0x99; 32])
            .is_err()
    );

    let mut wrong_ver = core_a.clone();
    wrong_ver.accepted_proposal_core_version = 2;
    assert!(wrong_ver.validate().is_err());

    let mut empty_hdr = core_a.clone();
    empty_hdr.canonical_proposal_header_bytes.clear();
    assert!(empty_hdr.validate().is_err());

    let mut dup_ev = w1.clone();
    dup_ev.evidence_digests = vec![[0xAA; 32], [0xAA; 32]];
    assert!(dup_ev.validate_structure().is_err());

    let mut wrong_parent = core_a;
    wrong_parent.accepted_parent_lookup_id[0] ^= 1;
    assert_ne!(
        wrong_parent.tag().unwrap().to_string(),
        EXPECTED_EXACT_CORE_TAG
    );
}
