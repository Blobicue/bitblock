#![cfg(feature = "protocol-research")]
//! Wave 30 (`CHAIN-LAUNCH-1@base`, `PS-091`) end-to-end integration tests:
//! 1. Canonical `|S| = 3` (`10/11` artifacts, `27` edges) and minimal `|S| = 0` (`7/8` artifacts,
//!    `18` edges) launch-closure packages, streaming preflight, and detached `3-of-5` authorization;
//! 2. Complete `LAYER1_DESIGN_PAPER.md` lines 1140–1149 powered mutant corpus (18 fail-closed
//!    mutants spanning counts, lengths, checked sums, double-counting, slices, DAG ranks,
//!    reversed genesis edges, locator variants, forbidden direct edges, and forged authorization);
//! 3. Structural correspondence with `proof/BitblockProof/LaunchClosureRefinement.lean`.

use bitblock::chain_launch_1::{
    ChainLaunch1Error, ChainLaunchAuthorizationV1, ChainLaunchManifestV1,
    ExternalGenesisIncomingDependencyClassV1, ExternalGenesisOutgoingDependencyClassV1,
    LaunchClosureDependencyRefV1, LaunchPreflightCapsV1, LaunchSignerSignatureV1,
    TrustedLaunchAuthorityPolicyV1, build_canonical_devnet_chain_launch,
    preflight_launch_closure_bundle_bytes, verify_chain_launch_package,
};
use bitblock::crypto_d1::{
    CanonicalEd25519PublicKeyV1, CanonicalEd25519SignatureV1, SignatureSuiteIdV1,
    SigningDomainTagV1, sign_preimage_deterministic,
};
use bitblock::genesis_d1::GENESIS_D1_PACKAGE_SCHEMA;

#[test]
fn wave30_canonical_s3_and_minimal_s0_launch_closure_packages() {
    let s3 = build_canonical_devnet_chain_launch(true).expect("S=3 devnet launch");
    let pf3 = preflight_launch_closure_bundle_bytes(
        &s3.package.launch_closure_bundle_bytes,
        s3.package.genesis_package_bytes.len() as u64,
        LaunchPreflightCapsV1::V1_DEFAULT,
    )
    .expect("S=3 zero-alloc preflight");
    assert_eq!(pf3.internal_artifact_count_u32, 10);
    assert_eq!(pf3.external_root_count_u32, 1);
    assert_eq!(pf3.complete_artifact_count_u32, 11);
    assert_eq!(pf3.supplementary_artifact_count_u32, 3);
    assert_eq!(pf3.i2i_edges, 22);
    assert_eq!(pf3.i2e_edges, 3);
    assert_eq!(pf3.e2i_edges, 2);
    assert_eq!(pf3.complete_edges, 27);
    assert_eq!(
        pf3.internal_preimage_bytes + pf3.external_preimage_bytes,
        pf3.total_preimage_bytes
    );
    assert_eq!(pf3.external_root_rank, 2);

    let out3 = verify_chain_launch_package(
        &s3.package,
        &s3.release_policy,
        &s3.launch_policy,
        LaunchPreflightCapsV1::V1_DEFAULT,
    )
    .expect("S=3 verify");
    assert_eq!(
        out3.chain_launch_manifest_id,
        s3.package.chain_launch_manifest_id
    );

    let s0 = build_canonical_devnet_chain_launch(false).expect("S=0 devnet launch");
    let pf0 = preflight_launch_closure_bundle_bytes(
        &s0.package.launch_closure_bundle_bytes,
        s0.package.genesis_package_bytes.len() as u64,
        LaunchPreflightCapsV1::V1_DEFAULT,
    )
    .expect("S=0 zero-alloc preflight");
    assert_eq!(pf0.internal_artifact_count_u32, 7);
    assert_eq!(pf0.external_root_count_u32, 1);
    assert_eq!(pf0.complete_artifact_count_u32, 8);
    assert_eq!(pf0.supplementary_artifact_count_u32, 0);
    assert_eq!(pf0.i2i_edges, 13);
    assert_eq!(pf0.i2e_edges, 3);
    assert_eq!(pf0.e2i_edges, 2);
    assert_eq!(pf0.complete_edges, 18);
    assert_ne!(
        s3.package.chain_launch_manifest_id, s0.package.chain_launch_manifest_id,
        "different closure bundle must produce distinct launch trust anchor"
    );
}

#[test]
fn wave30_complete_powered_launch_carrier_mutant_corpus() {
    let base = build_canonical_devnet_chain_launch(true).expect("base S=3");
    let caps = LaunchPreflightCapsV1::V1_DEFAULT;
    let g_len = base.package.genesis_package_bytes.len() as u64;
    let raw_b = &base.package.launch_closure_bundle_bytes;

    // Helper: assert mutated raw bundle bytes reject during preflight BEFORE allocation/decode.
    let assert_preflight_rejects = |mut_bytes: &[u8], outer_len: u64, label: &str| {
        assert!(
            preflight_launch_closure_bundle_bytes(mut_bytes, outer_len, caps).is_err(),
            "mutant '{label}' unexpectedly passed preflight"
        );
    };

    // 1. Increment & decrement internal_artifact_count_u32 (offset 2..6).
    for delta in [1i32, -1i32] {
        let mut m = raw_b.clone();
        let v = (10i32 + delta) as u32;
        m[2..6].copy_from_slice(&v.to_be_bytes());
        assert_preflight_rejects(&m, g_len, "internal_count_off_by_one");
    }

    // 2. Inject second external root (external_root_count_u32 = 2, offset 6..10).
    {
        let mut m = raw_b.clone();
        m[6..10].copy_from_slice(&2u32.to_be_bytes());
        assert_preflight_rejects(&m, g_len, "second_external_root");
    }

    // 3. Off-by-one complete_artifact_count_u32 (offset 10..14) & supplementary_count (14..18).
    {
        let mut m = raw_b.clone();
        m[10..14].copy_from_slice(&12u32.to_be_bytes());
        assert_preflight_rejects(&m, g_len, "complete_count_off_by_one");
        let mut m2 = raw_b.clone();
        m2[14..18].copy_from_slice(&4u32.to_be_bytes());
        assert_preflight_rejects(&m2, g_len, "supplementary_count_off_by_one");
    }

    // 4. Independent mutation of i2i (18..22), i2e (22..26), e2i (26..30), and combined edge total (30..34).
    for (off, bad) in [(18usize, 23u32), (22, 4), (26, 1), (30, 28)] {
        let mut m = raw_b.clone();
        m[off..off + 4].copy_from_slice(&bad.to_be_bytes());
        assert_preflight_rejects(&m, g_len, "edge_count_mutant");
    }

    // 5. Checked-sum overflow (`internal_bytes = u64::MAX`, offset 34..42).
    {
        let mut m = raw_b.clone();
        m[34..42].copy_from_slice(&u64::MAX.to_be_bytes());
        assert_preflight_rejects(&m, g_len, "checked_sum_u64_max_overflow");
    }

    // 6. Double-count embedded DomainRegistryV1 bytes into total_canonical_preimage_bytes_u64 (50..58).
    {
        let mut m = raw_b.clone();
        let doubled = base.bundle.total_canonical_preimage_bytes_u64
            + base.bundle.domain_registry_bytes.len() as u64;
        m[50..58].copy_from_slice(&doubled.to_be_bytes());
        assert_preflight_rejects(&m, g_len, "double_counted_registry_bytes");
    }

    // 7. Truncated and trailing-byte slices.
    assert_preflight_rejects(&raw_b[..raw_b.len() - 1], g_len, "truncated_bundle");
    {
        let mut m = raw_b.clone();
        m.push(0x00);
        assert_preflight_rejects(&m, g_len, "trailing_bundle_byte");
    }

    // 8. Force external genesis node X to rank 0.
    let x_rank_off = 2 + 8 * 4 + 3 * 8 + 2 + GENESIS_D1_PACKAGE_SCHEMA.len();
    {
        let mut m = raw_b.clone();
        m[x_rank_off..x_rank_off + 4].copy_from_slice(&0u32.to_be_bytes());
        assert_preflight_rejects(&m, g_len, "rank_zero_genesis");
    }

    // 9. Differently typed external root schema ID.
    {
        let mut m = raw_b.clone();
        m[2 + 8 * 4 + 3 * 8 + 2] ^= 0x01;
        assert_preflight_rejects(&m, g_len, "wrong_external_root_schema");
    }

    // 10. Omit one outgoing ref from X (`outgoing_ref_count = 1`).
    let x_out_count_off = x_rank_off + 4 + 32 + 8;
    {
        let mut m = raw_b.clone();
        m[x_out_count_off..x_out_count_off + 2].copy_from_slice(&1u16.to_be_bytes());
        assert_preflight_rejects(&m, g_len, "omitted_external_genesis_outgoing_ref");
    }

    // 11. Replace embedded explicit preimage carrier mode `1` with locator-only variant `2`.
    let e0_mode_off = x_out_count_off + 2 + 2 * 33 + 7 * 8;
    assert_eq!(raw_b[e0_mode_off], 1);
    {
        let mut m = raw_b.clone();
        m[e0_mode_off] = 2;
        assert_preflight_rejects(&m, g_len, "locator_only_variant_rejected");
    }

    // 12. Outer genesis byte length mismatch.
    assert_preflight_rejects(raw_b, g_len + 1, "outer_genesis_len_mismatch");

    // Helper: re-encode mutated LaunchClosureBundleV1 into a package & assert verify rejects.
    let assert_bundle_mutant_rejects = |mut b: bitblock::chain_launch_1::LaunchClosureBundleV1,
                                        label: &str| {
        b.internal_to_internal_dependency_edge_count_u32 = b
            .internal_dependency_nodes
            .iter()
            .flat_map(|n| n.dependency_refs.iter())
            .filter(|r| matches!(r, LaunchClosureDependencyRefV1::InternalLaunchArtifactV1(_)))
            .count() as u32;
        b.internal_to_external_dependency_edge_count_u32 = b
            .internal_dependency_nodes
            .iter()
            .flat_map(|n| n.dependency_refs.iter())
            .filter(|r| {
                matches!(
                    r,
                    LaunchClosureDependencyRefV1::ExternalGenesisRootV1 { .. }
                )
            })
            .count() as u32;
        b.complete_dependency_edge_count_u32 = b.internal_to_internal_dependency_edge_count_u32
            + b.internal_to_external_dependency_edge_count_u32
            + b.external_to_internal_dependency_edge_count_u32;
        let b_bytes = b.encode_canonical().expect("re-encode mutant bundle");
        let b_id = b.tag().expect("mutant bundle tag").digest();
        let mut pkg = base.package.clone();
        pkg.launch_closure_bundle_bytes = b_bytes;
        pkg.launch_closure_bundle_id = b_id;
        pkg.manifest.launch_closure_bundle_id = b_id;
        pkg.chain_launch_manifest_id = pkg.manifest.tag().expect("m tag").digest();
        assert!(
            verify_chain_launch_package(&pkg, &base.release_policy, &base.launch_policy, caps)
                .is_err(),
            "DAG mutant '{label}' unexpectedly verified"
        );
    };

    // 13. Reverse ProfileScope -> ExternalGenesis edge (with artificially higher rank so only reversal fails).
    {
        let mut b = base.bundle.clone();
        let scope_id = b
            .external_genesis_node
            .outgoing_refs
            .iter()
            .find(|r| {
                r.outgoing_class == ExternalGenesisOutgoingDependencyClassV1::GenesisInitialScopeV1
            })
            .unwrap()
            .typed_internal_artifact_id;
        let n = b
            .internal_dependency_nodes
            .iter_mut()
            .find(|n| n.typed_artifact_id == scope_id)
            .unwrap();
        n.dependency_refs
            .push(LaunchClosureDependencyRefV1::ExternalGenesisRootV1 {
                genesis_id: b.external_genesis_node.genesis_id,
                edge_class:
                    ExternalGenesisIncomingDependencyClassV1::InitialCandidateGenesisIdentityV1,
            });
        assert_bundle_mutant_rejects(b, "reversed_scope_to_genesis_edge");
    }

    // 14. Reverse DomainRegistry -> ExternalGenesis edge.
    {
        let mut b = base.bundle.clone();
        let reg_id = b
            .external_genesis_node
            .outgoing_refs
            .iter()
            .find(|r| {
                r.outgoing_class
                    == ExternalGenesisOutgoingDependencyClassV1::GenesisInitialDomainRegistryV1
            })
            .unwrap()
            .typed_internal_artifact_id;
        let n = b
            .internal_dependency_nodes
            .iter_mut()
            .find(|n| n.typed_artifact_id == reg_id)
            .unwrap();
        n.dependency_refs
            .push(LaunchClosureDependencyRefV1::ExternalGenesisRootV1 {
                genesis_id: b.external_genesis_node.genesis_id,
                edge_class:
                    ExternalGenesisIncomingDependencyClassV1::InitialCandidateGenesisIdentityV1,
            });
        assert_bundle_mutant_rejects(b, "reversed_registry_to_genesis_edge");
    }

    // 15. Forbidden direct TransitionSpecAtProfileV1 -> ExternalGenesis & ProfileAdmission -> ExternalGenesis edges.
    for target_id in [
        base.package.manifest.initial_transition_spec_at_profile_id,
        base.package.manifest.initial_profile_admission_manifest_id,
    ] {
        let mut b = base.bundle.clone();
        let n = b
            .internal_dependency_nodes
            .iter_mut()
            .find(|n| n.typed_artifact_id == target_id)
            .unwrap();
        n.dependency_refs
            .push(LaunchClosureDependencyRefV1::ExternalGenesisRootV1 {
                genesis_id: b.external_genesis_node.genesis_id,
                edge_class: ExternalGenesisIncomingDependencyClassV1::GenesisStateDerivationV1,
            });
        assert_bundle_mutant_rejects(b, "forbidden_direct_spec_or_admission_genesis_edge");
    }

    // 16. Omit mandatory CandidateProfileV1 -> ExternalGenesis edge.
    {
        let mut b = base.bundle.clone();
        let n = b
            .internal_dependency_nodes
            .iter_mut()
            .find(|n| n.typed_artifact_id == base.package.manifest.initial_profile_id)
            .unwrap();
        n.dependency_refs.retain(|r| {
            !matches!(
                r,
                LaunchClosureDependencyRefV1::ExternalGenesisRootV1 { .. }
            )
        });
        assert_bundle_mutant_rejects(b, "omitted_candidate_to_genesis_edge");
    }

    // 17. Candidate-selected launch authority policy rejected (policy mismatch vs G_0).
    {
        let mut alt_pks: Vec<_> = [61u8, 62, 63, 64, 65]
            .into_iter()
            .map(|s| CanonicalEd25519PublicKeyV1::from_secret_seed(&[s; 32]).unwrap())
            .collect();
        alt_pks.sort_by_key(|pk| *pk.as_bytes());
        let alt_policy = TrustedLaunchAuthorityPolicyV1 {
            threshold_m: 3,
            authorized_pubkeys: [alt_pks[0], alt_pks[1], alt_pks[2], alt_pks[3], alt_pks[4]],
        };
        assert!(matches!(
            verify_chain_launch_package(&base.package, &base.release_policy, &alt_policy, caps),
            Err(ChainLaunch1Error::BindingMismatch(_))
        ));
    }

    // 18. Altered detached launch authorization: 2-of-5 shortfall, wrong signing domain, & 1-bit sig flip.
    {
        let mut p_short = base.package.clone();
        p_short.authorization.signatures.pop();
        assert!(matches!(
            verify_chain_launch_package(&p_short, &base.release_policy, &base.launch_policy, caps),
            Err(ChainLaunch1Error::InvalidLaunchAuthorization(_))
        ));

        let (_, kps) = TrustedLaunchAuthorityPolicyV1::devnet_three_of_five();
        let mut bad_preimage = ChainLaunchAuthorizationV1::signing_preimage(
            77,
            base.package.manifest.genesis_id,
            base.package.chain_launch_manifest_id,
            base.package.manifest.initial_profile_id,
            base.package.manifest.launch_authority_policy_id,
            SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
        );
        bad_preimage.domain_tag = SigningDomainTagV1::ReleaseArtifact;
        let mut p_dom = base.package.clone();
        p_dom.authorization.signatures[0] = LaunchSignerSignatureV1 {
            signer_index: 0,
            public_key: kps[0].0,
            signature: sign_preimage_deterministic(&kps[0].1, &bad_preimage).unwrap(),
        };
        assert!(matches!(
            verify_chain_launch_package(&p_dom, &base.release_policy, &base.launch_policy, caps),
            Err(ChainLaunch1Error::InvalidLaunchAuthorization(_))
        ));

        let mut p_flip = base.package.clone();
        let mut sb = *p_flip.authorization.signatures[0].signature.as_bytes();
        sb[32] ^= 0x01;
        p_flip.authorization.signatures[0].signature =
            CanonicalEd25519SignatureV1::from_bytes(sb).expect("scalar flip canonical");
        assert!(matches!(
            verify_chain_launch_package(&p_flip, &base.release_policy, &base.launch_policy, caps),
            Err(ChainLaunch1Error::InvalidLaunchAuthorization(_))
        ));
    }

    // 19. Manifest outer ID / self-reference tampering rejected.
    {
        let mut p_id = base.package.clone();
        p_id.chain_launch_manifest_id[0] ^= 0x80;
        assert!(matches!(
            verify_chain_launch_package(&p_id, &base.release_policy, &base.launch_policy, caps),
            Err(ChainLaunch1Error::BindingMismatch(_))
        ));
        let mut p_parent = base.package.clone();
        p_parent.manifest.genesis_parent_block_id =
            ChainLaunchManifestV1::derive_genesis_parent_block_id([0xAA; 32]);
        assert!(matches!(
            verify_chain_launch_package(&p_parent, &base.release_policy, &base.launch_policy, caps),
            Err(ChainLaunch1Error::BindingMismatch(_))
        ));
    }
}

#[test]
fn wave30_lean_launch_closure_refinement_correspondence() {
    // Mirrors proof/BitblockProof/LaunchClosureRefinement.lean equations and rank invariants
    // directly against the concrete Rust |S|=3 and |S|=0 launch-closure bundles.
    for (include_s, exp_s, exp_int, exp_comp, exp_edges) in
        [(true, 3u32, 10u32, 11u32, 27u32), (false, 0, 7, 8, 18)]
    {
        let built = build_canonical_devnet_chain_launch(include_s).expect("build");
        let b = &built.bundle;
        // Section 1 equations (`ValidLaunchClosureEquations`)
        assert_eq!(b.explicit_preimage_lengths_u64.len(), 7);
        assert_eq!(b.supplementary_artifact_count_u32, exp_s);
        assert_eq!(b.internal_artifact_count_u32, 7 + exp_s);
        assert_eq!(b.internal_artifact_count_u32, exp_int);
        assert_eq!(b.external_root_count_u32, 1);
        assert_eq!(b.complete_artifact_count_u32, 8 + exp_s);
        assert_eq!(b.complete_artifact_count_u32, exp_comp);
        assert_eq!(b.external_to_internal_dependency_edge_count_u32, 2);
        assert_eq!(
            b.complete_dependency_edge_count_u32,
            b.internal_to_internal_dependency_edge_count_u32
                + b.internal_to_external_dependency_edge_count_u32
                + 2
        );
        assert_eq!(b.complete_dependency_edge_count_u32, exp_edges);
        let sum_e: u64 = b.explicit_preimage_lengths_u64.iter().sum();
        let sum_s: u64 = b
            .supplementary_artifacts
            .iter()
            .map(|s| s.canonical_artifact_preimage_length_u64)
            .sum();
        assert_eq!(b.internal_canonical_preimage_bytes_u64, sum_e + sum_s);
        assert_eq!(
            b.total_canonical_preimage_bytes_u64,
            sum_e + sum_s + b.external_root_canonical_preimage_bytes_u64
        );

        // Sections 2–3 combined DAG strict rank monotonicity & external genesis rank >= 2
        let x_rank = b.external_genesis_node.external_root_topological_rank_u32;
        assert!(x_rank >= 2, "external_genesis_rank_ge_two");
        let rank_of = |id: [u8; 32]| -> u32 {
            b.internal_dependency_nodes
                .iter()
                .find(|n| n.typed_artifact_id == id)
                .unwrap()
                .topological_rank_u32
        };
        let scope_id = b
            .external_genesis_node
            .outgoing_refs
            .iter()
            .find(|r| {
                r.outgoing_class == ExternalGenesisOutgoingDependencyClassV1::GenesisInitialScopeV1
            })
            .unwrap()
            .typed_internal_artifact_id;
        let reg_id = b
            .external_genesis_node
            .outgoing_refs
            .iter()
            .find(|r| {
                r.outgoing_class
                    == ExternalGenesisOutgoingDependencyClassV1::GenesisInitialDomainRegistryV1
            })
            .unwrap()
            .typed_internal_artifact_id;
        assert_eq!(rank_of(reg_id), 0);
        assert_eq!(rank_of(scope_id), 1);
        assert_eq!(x_rank, 2);
        assert_eq!(rank_of(built.package.manifest.initial_profile_id), 3);
        assert_eq!(
            rank_of(built.package.manifest.initial_transition_spec_at_profile_id),
            4
        );
        assert_eq!(
            rank_of(built.package.manifest.initial_profile_admission_manifest_id),
            5
        );
        assert_eq!(
            rank_of(built.package.manifest.genesis_root_parity_record_id),
            6
        );
        assert_eq!(rank_of(built.package.manifest.initial_release_record_id), 7);
    }
}
