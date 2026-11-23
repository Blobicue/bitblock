#![cfg(feature = "protocol-research")]
//! Wave 18 (`GENESIS-ROOT-PARITY-1`, `PS-031`) integration tests: 4-way preimage-verified
//! logical-genesis to typed-root parity (`Cold rebuild == Incremental == EXECUTOR-1 == Pure Python`)
//! and complete 8-control adversarial mutation gate matrix.

use bitblock::execution_root_1::{
    ExecutionDomainEntryV1, ExecutionDomainRegistryV1, ExecutionDomainTagV1, ExecutionLayerV1,
    ExecutionRoot1Error, RootSlotCommitmentV1, compose_from_slot_commitments,
};
use bitblock::genesis_root_parity_1::{
    CanonicalDevnetGenesisFixtureV1, GenesisParityInputV1, GenesisPreimageBundleV1,
    GenesisRootParity1Error, GenesisRootParityRecordV1, verify_bedrock_only_world,
    verify_genesis_root_parity, verify_parity_record_against_composition,
};
use bitblock::profile_d1::{BoundProfileManifestV1, CandidateProfileManifestV1};
use bitblock::world::{Coord, World};

#[test]
fn wave18_genesis_root_parity_1_four_way_rebuild_and_golden_parity() {
    let fix = CanonicalDevnetGenesisFixtureV1::build().unwrap();
    let raw_pkg = fix.package.encode_canonical().unwrap();
    let raw_bundle = fix.bundle.encode_canonical().unwrap();

    let input = GenesisParityInputV1 {
        raw_genesis_package: &raw_pkg,
        raw_preimage_bundle: &raw_bundle,
        profile_scope: &fix.profile_scope,
        commit_domain_registry: &fix.commit_domain_registry,
        bound_manifest: &fix.bound_manifest,
        transition_spec: &fix.transition_spec,
        exec_registry: &fix.exec_registry,
    };
    let out = verify_genesis_root_parity(&input).unwrap();

    // Round-trip codecs
    let enc_rec = out.record.encode_canonical().unwrap();
    assert_eq!(
        GenesisRootParityRecordV1::decode_canonical(&enc_rec).unwrap(),
        out.record
    );
    assert_eq!(
        GenesisPreimageBundleV1::decode_canonical(&raw_bundle).unwrap(),
        fix.bundle
    );
    assert_eq!(
        out.record.independent_reconstruction_artifact_digests.len(),
        3
    );
    assert_eq!(out.composition.slots.len(), 11);
    assert!(!out.snapshot.program_state_active);

    // Corroborated 4-way golden tags matching clean-room Python Path 4 (tools/genesis_root_parity_1_vectors.py)
    assert_eq!(
        fix.package.tag().unwrap().to_string(),
        "#bb.receipt.sha256.3xkh3lfjjgfxg52qhgoqw5zl34zr4d5zax5d6apqufpyiyrpysoa"
    );
    assert_eq!(
        fix.bundle.tag().unwrap().to_string(),
        "#bb.policy.sha256.6mcfpltkrno445wrrzhsecwowfll6m5ekyhakh6aw5n2vcdirpsa"
    );
    assert_eq!(
        out.composition.tag().unwrap().to_string(),
        "#bb.receipt.sha256.uihe33whofr5752ptw2dxfq3tuvtee7ssqrukxxldfolnjsvay2q"
    );
    assert_eq!(
        out.record.tag().unwrap().to_string(),
        "#bb.receipt.sha256.xkaeumdnpaz77nlkk3oolxx2surjpvay2ycpksqckx4oddbm67fq"
    );
}

#[test]
fn wave18_genesis_root_parity_1_eight_adversarial_mutation_gates() {
    let fix = CanonicalDevnetGenesisFixtureV1::build().unwrap();
    let raw_pkg = fix.package.encode_canonical().unwrap();
    let raw_bundle = fix.bundle.encode_canonical().unwrap();
    let base_in = GenesisParityInputV1 {
        raw_genesis_package: &raw_pkg,
        raw_preimage_bundle: &raw_bundle,
        profile_scope: &fix.profile_scope,
        commit_domain_registry: &fix.commit_domain_registry,
        bound_manifest: &fix.bound_manifest,
        transition_spec: &fix.transition_spec,
        exec_registry: &fix.exec_registry,
    };
    let good = verify_genesis_root_parity(&base_in).unwrap();

    // G1: 1-bit preimage tamper (altered bootstrap policy preimage)
    let mut bad_bundle = fix.bundle.clone();
    bad_bundle.bootstrap_policy.min_seed_peers += 1;
    let raw_bad_b = bad_bundle.encode_canonical().unwrap();
    assert!(matches!(
        verify_genesis_root_parity(&GenesisParityInputV1 {
            raw_preimage_bundle: &raw_bad_b,
            ..base_in
        }),
        Err(GenesisRootParity1Error::PreimageDigestMismatch(_))
    ));

    // G2: Omitted actor descriptor preimage
    let mut omitted = fix.bundle.clone();
    omitted.actor_descriptors.pop();
    let raw_omit = omitted.encode_canonical().unwrap();
    assert!(matches!(
        verify_genesis_root_parity(&GenesisParityInputV1 {
            raw_preimage_bundle: &raw_omit,
            ..base_in
        }),
        Err(GenesisRootParity1Error::InvalidBundleCardinalityOrOrder(_))
    ));

    // G3: Duplicated actor descriptor preimage
    let mut dup = fix.bundle.clone();
    let first = dup.actor_descriptors[0].clone();
    dup.actor_descriptors.push(first);
    assert!(dup.validate().is_err());

    // G4: Cross-domain root substitution (swap Auth <-> Nonce in composition)
    let mut swapped_comp = good.composition.clone();
    let auth_slot = swapped_comp.slots[6];
    let nonce_slot = swapped_comp.slots[7];
    if let (
        RootSlotCommitmentV1::ActiveRoot {
            root_bytes: auth_r, ..
        },
        RootSlotCommitmentV1::ActiveRoot {
            root_bytes: nonce_r,
            ..
        },
    ) = (auth_slot, nonce_slot)
    {
        swapped_comp.slots[6] = RootSlotCommitmentV1::ActiveRoot {
            domain_tag: ExecutionDomainTagV1::Authorization,
            layer: ExecutionLayerV1::ProtocolLayer,
            root_schema_digest: ExecutionDomainTagV1::Authorization.canonical_root_schema_digest(),
            root_bytes: nonce_r,
        };
        swapped_comp.slots[7] = RootSlotCommitmentV1::ActiveRoot {
            domain_tag: ExecutionDomainTagV1::Nonce,
            layer: ExecutionLayerV1::ProtocolLayer,
            root_schema_digest: ExecutionDomainTagV1::Nonce.canonical_root_schema_digest(),
            root_bytes: auth_r,
        };
    }
    let recomposed_swap = compose_from_slot_commitments(
        &swapped_comp.slots,
        0,
        0,
        &fix.exec_registry,
        good.composition.preimage_context,
        fix.bound_manifest.profile_id,
    )
    .unwrap();
    assert!(
        verify_parity_record_against_composition(
            &good.record,
            &recomposed_swap,
            &fix.exec_registry,
            &fix.bound_manifest
        )
        .is_err()
    );

    // G5: Stale / CE-43 successor profile substituted for initial launch profile
    let succ_cand =
        CandidateProfileManifestV1::canonical_successor_program_v1(&fix.bound_manifest, [0x5E; 32])
            .unwrap();
    let succ_bound = BoundProfileManifestV1::seal(succ_cand).unwrap();
    assert!(matches!(
        verify_genesis_root_parity(&GenesisParityInputV1 {
            bound_manifest: &succ_bound,
            ..base_in
        }),
        Err(GenesisRootParity1Error::StaleOrNonInitialProfileRejected(_))
    ));

    // G6: Synthetic-zero substitute for inactive ProgramState sentinel (slot 6 marked Active with [0;32])
    let mut bad_reg = ExecutionDomainRegistryV1::canonical_launch_v1();
    bad_reg.entries[5] = ExecutionDomainEntryV1::Active {
        domain_tag: ExecutionDomainTagV1::ProgramState,
        layer: ExecutionLayerV1::ApplicationLayer,
        root_schema_digest: ExecutionDomainTagV1::ProgramState.canonical_root_schema_digest(),
    };
    assert!(matches!(
        verify_genesis_root_parity(&GenesisParityInputV1 {
            exec_registry: &bad_reg,
            ..base_in
        }),
        Err(GenesisRootParity1Error::ExecutionRoot(
            ExecutionRoot1Error::MixedManifestRejected(_)
        ))
    ));

    // G7: Self-derived-ID mutation (tamper genesis_id inside raw GenesisPackageV1)
    let mut tampered_pkg = raw_pkg.clone();
    let last = tampered_pkg.len() - 1;
    tampered_pkg[last] ^= 0x01;
    assert!(matches!(
        verify_genesis_root_parity(&GenesisParityInputV1 {
            raw_genesis_package: &tampered_pkg,
            ..base_in
        }),
        Err(GenesisRootParity1Error::GenesisPackage(_))
    ));

    // G8: Non-bedrock world B_0 mutation (extra voxel outside (0,0,0))
    let mut bad_world = World::new();
    bad_world.set_on(Coord::new(1, 0, 0));
    assert!(matches!(
        verify_bedrock_only_world(&bad_world),
        Err(GenesisRootParity1Error::NonBedrockWorldRejected(_))
    ));
}
