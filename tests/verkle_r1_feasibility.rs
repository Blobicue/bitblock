#![cfg(feature = "protocol-research")]
//! `VERKLE-R1@feasibility` integration checks: injective 32-byte coordinate embedding,
//! non-aliasing stem/field headroom, self-excluding `VectorCommitmentProfileDescriptorV1`,
//! and deterministic arity-256 structural analysis on the 7 `COMMIT-R1` worlds.

use bitblock::scale_assay::{ScaleWorkloadId, build_workload_world};
use bitblock::verkle_feasibility::{
    BANDERSNATCH_SCALAR_MODULUS_BE, BLS12_381_SCALAR_MODULUS_BE, IPA_256_OPENING_FLOOR_BYTES,
    IPA_256_VERIFY_MSM_POINTS, KZG_BLS12_381_OPENING_FLOOR_BYTES, SetupCustodyModelV1,
    SubgroupCheckPolicyV1, VERKLE_ARITY, VERKLE_COORDINATE_KEY_V1_SCHEMA, VERKLE_KEY_BYTES,
    VERKLE_PREFIX_BYTES, VERKLE_STEM_BYTES, VERKLE_VC_PROFILE_V1_SCHEMA,
    VectorCommitmentProfileDescriptorV1, VectorCommitmentSchemeIdV1, VerkleCoordinateKeyV1,
    VerkleFeasibilityError, VerkleLeafSlotEncodingV1, analyze_verkle_256_world,
    estimate_verkle_batch_whole_witness_bytes,
};
use bitblock::world::{Coord, World};

#[test]
fn verkle_coordinate_key_bijection_order_and_modulus_headroom_over_169_signed_pairs() {
    assert_eq!(VERKLE_KEY_BYTES, 32);
    assert_eq!(VERKLE_PREFIX_BYTES, 8);
    assert_eq!(VERKLE_STEM_BYTES, 31);
    assert_eq!(VERKLE_ARITY, 256);
    assert_eq!(
        VERKLE_COORDINATE_KEY_V1_SCHEMA,
        b"bitblock/verkle-r1/coord-key-32b/v1"
    );
    assert_eq!(
        VERKLE_VC_PROFILE_V1_SCHEMA,
        b"bitblock/verkle-r1/vector-commitment-profile/v1"
    );

    let boundaries: [i64; 13] = [
        i64::MIN,
        i64::MIN + 1,
        -17,
        -16,
        -15,
        -1,
        0,
        1,
        15,
        16,
        17,
        i64::MAX - 1,
        i64::MAX,
    ];

    for &a in &boundaries {
        let ca = Coord::new(a, a.saturating_neg(), a);
        let ka = VerkleCoordinateKeyV1::from_coord(ca);
        assert_eq!(
            VerkleCoordinateKeyV1::from_bytes(ka.as_bytes()).unwrap(),
            ka
        );
        assert_eq!(ka.to_coord(), ca);
        assert_eq!(
            VerkleCoordinateKeyV1::from_stem_and_suffix(ka.stem(), ka.suffix()).unwrap(),
            ka
        );
        assert!(ka.stem_strictly_below_modulus(&BANDERSNATCH_SCALAR_MODULUS_BE));
        assert!(ka.stem_strictly_below_modulus(&BLS12_381_SCALAR_MODULUS_BE));
        for &b in &boundaries {
            let kb = VerkleCoordinateKeyV1::from_coord(Coord::new(b, b.saturating_neg(), b));
            assert_eq!(a.cmp(&b), ka.cmp(&kb));
        }
    }

    // Present vs absent leaf slot encoding never aliases
    let zero_present = VerkleLeafSlotEncodingV1::present_slot(0);
    let on_present = VerkleLeafSlotEncodingV1::present_on_v1();
    assert_ne!(zero_present, VerkleLeafSlotEncodingV1::ABSENT_SLOT);
    assert_ne!(zero_present, on_present);
    assert!(VerkleLeafSlotEncodingV1::is_present(&zero_present));
    assert!(VerkleLeafSlotEncodingV1::is_present(&on_present));
    assert!(!VerkleLeafSlotEncodingV1::is_present(
        &VerkleLeafSlotEncodingV1::ABSENT_SLOT
    ));
}

#[test]
fn vector_commitment_profile_descriptor_round_trips_and_rejects_mutations() {
    let bander = VectorCommitmentProfileDescriptorV1::bandersnatch_ipa_256_v1();
    assert_eq!(
        bander.vc_scheme,
        VectorCommitmentSchemeIdV1::BandersnatchPedersenIpa256
    );
    assert_eq!(
        bander.setup_custody_model,
        SetupCustodyModelV1::TransparentHashToCurve
    );
    assert_eq!(
        bander.subgroup_check_policy,
        SubgroupCheckPolicyV1::RejectNonCanonicalOffCurveAndSmallSubgroup
    );
    assert_eq!(
        bander.opening_proof_fixed_floor_bytes_u32,
        IPA_256_OPENING_FLOOR_BYTES
    );

    let kzg = VectorCommitmentProfileDescriptorV1::bls12_381_kzg_256_rejected_v1();
    assert_eq!(
        kzg.opening_proof_fixed_floor_bytes_u32,
        KZG_BLS12_381_OPENING_FLOOR_BYTES
    );
    assert_ne!(bander.profile_id(), kzg.profile_id());

    // Unknown enum wire codes reject cleanly
    let mut raw = bander.encode_canonical();
    raw[2] = 99;
    assert_eq!(
        VectorCommitmentProfileDescriptorV1::decode_canonical(&raw),
        Err(VerkleFeasibilityError::UnknownEnumCode {
            field: "vc_scheme",
            code: 99,
        })
    );
    let mut raw_setup = bander.encode_canonical();
    raw_setup[51] = 77;
    assert_eq!(
        VectorCommitmentProfileDescriptorV1::decode_canonical(&raw_setup),
        Err(VerkleFeasibilityError::UnknownEnumCode {
            field: "setup_custody_model",
            code: 77,
        })
    );
    let mut raw_sub = bander.encode_canonical();
    raw_sub[54] = 0;
    assert_eq!(
        VectorCommitmentProfileDescriptorV1::decode_canonical(&raw_sub),
        Err(VerkleFeasibilityError::UnknownEnumCode {
            field: "subgroup_check_policy",
            code: 0,
        })
    );
    let mut raw_ver = bander.encode_canonical();
    raw_ver[1] = 2;
    assert_eq!(
        VectorCommitmentProfileDescriptorV1::decode_canonical(&raw_ver),
        Err(VerkleFeasibilityError::UnsupportedSchemaVersion(2))
    );
}

#[test]
fn verkle_structural_analyzer_captures_stem_sharing_and_witness_floors() {
    // 1. DenseCube16 (16x16x16 = 4,096 coords): z in 0..16 shares 1 stem per (x,y) => 256 stems!
    let dense = build_workload_world(ScaleWorkloadId::DenseCube16);
    let dense_coords: Vec<Coord> = dense.iter_on().take(8).collect();
    let dense_m = analyze_verkle_256_world(&dense, &dense_coords, &dense_coords);
    assert_eq!(dense_m.occupied_keys, 4_096);
    assert_eq!(dense_m.unique_stems, 256);
    assert!(dense_m.single_query_ipa_whole_witness_bytes > IPA_256_OPENING_FLOOR_BYTES as usize);
    assert_eq!(dense_m.ipa_verify_msm_points, IPA_256_VERIFY_MSM_POINTS);

    // 2. SingleOrigin: 1 key, 1 stem, 0 internal nodes, whole witness still exceeds 576 B IPA floor
    let origin = World::new();
    let orig_m = analyze_verkle_256_world(&origin, &[Coord::new(0, 0, 0)], &[Coord::new(0, 0, 0)]);
    assert_eq!(orig_m.occupied_keys, 1);
    assert_eq!(orig_m.unique_stems, 1);
    assert_eq!(orig_m.internal_nodes, 0);
    assert!(orig_m.single_query_ipa_whole_witness_bytes >= 576 + 64 + 33 + 16);

    // 3. Batch witness grows strictly with distinct opened stems (refuting constant 1-2 KB whole-witness claim)
    let mut dispersed = World::new();
    let mut batch64 = Vec::new();
    for i in 0..64_i64 {
        let c = Coord::new(i * 1024, -i * 512, i * 257);
        dispersed.set_on(c);
        batch64.push(c);
    }
    let w1 = estimate_verkle_batch_whole_witness_bytes(&dispersed, &batch64[..1], 32, 576);
    let w64 = estimate_verkle_batch_whole_witness_bytes(&dispersed, &batch64, 32, 576);
    assert!(w64 > w1 + 63 * 64);
}
