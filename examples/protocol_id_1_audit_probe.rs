//! Release identifier-DAG & two-row collision audit probe for `PROTOCOL-ID-1` (`PS-027`).

use bitblock::crypto_d1::{
    CanonicalEd25519PublicKeyV1, HashSuiteIdV1, SignatureSuiteIdV1, SigningDomainTagV1,
    SigningPreimageV1, sign_preimage_deterministic,
};
use bitblock::protocol_id_1::{
    AcceptedProposalWrapperV1, AtomicAvailabilityIndexV1, CanonicalAcceptedProposalCoreV1,
    PROTOCOL_ID_1_CANONICAL_DAG, ProposalCredentialEntryV1, SameWrapperAvailabilityDependencyV1,
    TwoRowParentCollisionAuditV1, normalize_proposal_wrappers, profile_id_hash_parent_lookup,
    verify_canonical_protocol_id_dag,
};

fn main() {
    verify_canonical_protocol_id_dag().unwrap();
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
    let mut avail_idx = AtomicAvailabilityIndexV1::default();
    avail_idx.admit_wrapper(&merged).unwrap();

    let audit_r2 = TwoRowParentCollisionAuditV1::classify_with_hasher(
        lid,
        preimage,
        &core_a,
        preimage,
        &core_b,
        profile_id_hash_parent_lookup,
    )
    .unwrap();
    let audit_r1 = TwoRowParentCollisionAuditV1::classify_with_hasher(
        lid,
        preimage,
        &core_a,
        b"distinct-colliding-preimage",
        &core_b,
        |_| lid,
    )
    .unwrap();

    println!("{{");
    println!("  \"dag_edges\": {},", PROTOCOL_ID_1_CANONICAL_DAG.len());
    println!("  \"exact_core_tag\": \"{}\",", core_a.tag().unwrap());
    println!(
        "  \"normalized_wrapper_tag\": \"{}\",",
        merged.tag().unwrap()
    );
    println!("  \"audit_row2_tag\": \"{}\",", audit_r2.tag().unwrap());
    println!("  \"audit_row1_tag\": \"{}\",", audit_r1.tag().unwrap());
    println!(
        "  \"normalized_credentials\": {},",
        merged.credentials.len()
    );
    println!(
        "  \"normalized_evidence\": {},",
        merged.evidence_digests.len()
    );
    println!("  \"atomic_avail_deps\": {}", avail_idx.global_sizes.len());
    println!("}}");
}
