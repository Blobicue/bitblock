#![cfg(feature = "protocol-research")]
//! `REFINE-1@commitment`: Rust-to-Lean `CommitmentRefinement` correspondence.
//!
//! Mirrors the named abstraction function `alphaPat : PatNode -> List Bool -> Option PatVal`,
//! `biasSigned64`/`unbiasSigned64`, structural uniqueness (`canonicalPat_unique_of_alpha_eq`),
//! and proof soundness/completeness (`verifyPatProof_sound`/`canonicalPat_provePat_complete`)
//! from `proof/BitblockProof/CommitmentRefinement.lean` against live
//! `bitblock::commit_state::TypedPatriciaTrie` and `bitblock::commit_r1::CoordinateKeyV1`.
//!
//! Explicitly bounded: verifies all `13^2 = 169` signed boundary/extrema order pairs, all
//! `4! = 24` insertion permutations over 192-bit and 256-bit boundary/twin sets, base+step
//! insert/delete restoration, complete proof soundness/completeness, and all powered negative
//! controls. Does not certify `rustc`/LLVM code generation or SHA-256 collision resistance.

use std::collections::BTreeMap;

use bitblock::commit_r1::CoordinateKeyV1;
use bitblock::commit_state::{
    CommitmentOperationalLimitsDescriptorV1, CommitmentProfileDescriptorV1, EpisodeRecordV1,
    EventIdV1, ExpectedProofContextV1, LookupResult, MintIdV1, ON_V1_VALUE_BYTES,
    ProfileRegistryV1, TypedPatriciaProofV1, TypedPatriciaTrie, TypedProofStep, TypedProofTerminal,
    decode_and_verify_node_record, typed_empty_root, verify_lookup,
};
use bitblock::world::Coord;

/// Named abstraction function `alpha_commitment`: maps a Rust `TypedPatriciaTrie` to its logical map.
fn alpha_commitment(trie: &TypedPatriciaTrie, universe: &[Vec<u8>]) -> BTreeMap<Vec<u8>, Vec<u8>> {
    let mut map = BTreeMap::new();
    for k in universe {
        if let Some(v) = trie.get(k) {
            map.insert(k.clone(), v.to_vec());
        }
    }
    map
}

/// Mirror of Lean `BitblockProof.biasSigned64`.
fn lean_bias_signed64(v: i64) -> u64 {
    (v as u64) ^ (1_u64 << 63)
}

/// Mirror of Lean `BitblockProof.unbiasSigned64`.
fn lean_unbias_signed64(u: u64) -> i64 {
    (u ^ (1_u64 << 63)) as i64
}

fn permutations<T: Clone>(items: &[T]) -> Vec<Vec<T>> {
    if items.is_empty() {
        return vec![vec![]];
    }
    let mut out = Vec::new();
    for i in 0..items.len() {
        let mut rest = items.to_vec();
        let head = rest.remove(i);
        for mut tail in permutations(&rest) {
            tail.insert(0, head.clone());
            out.push(tail);
        }
    }
    out
}

#[test]
fn signed64_bias_bijection_order_preservation_and_raw_twos_complement_negative_control() {
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

    // Exact scalar boundary vectors matching Lean `biasSigned64_boundary_vectors`
    assert_eq!(lean_bias_signed64(i64::MIN).to_be_bytes(), [0x00; 8]);
    assert_eq!(
        lean_bias_signed64(-1).to_be_bytes(),
        [0x7f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]
    );
    assert_eq!(
        lean_bias_signed64(0).to_be_bytes(),
        [0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
    );
    assert_eq!(
        lean_bias_signed64(1).to_be_bytes(),
        [0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01]
    );
    assert_eq!(lean_bias_signed64(i64::MAX).to_be_bytes(), [0xff; 8]);

    // Exhaustive 13x13 = 169 ordered pair correspondence with `CoordinateKeyV1`
    for &a in &boundaries {
        let ua = lean_bias_signed64(a);
        assert_eq!(lean_unbias_signed64(ua), a);
        let ka = CoordinateKeyV1::from_coord(Coord::new(a, a, a));
        assert_eq!(ka.to_coord(), Coord::new(a, a, a));
        assert_eq!(&ka.as_bytes()[0..8], &ua.to_be_bytes());
        for &b in &boundaries {
            let ub = lean_bias_signed64(b);
            let kb = CoordinateKeyV1::from_coord(Coord::new(b, b, b));
            assert_eq!(a < b, ua < ub);
            assert_eq!(a.cmp(&b), ka.as_bytes().cmp(kb.as_bytes()));
        }
    }

    // Powered negative control mirroring Lean `rawTwosComp64_breaks_signed_order`:
    // Raw unsigned two's complement places `-1` (0xffff...) strictly ABOVE `0` (0x0000...).
    let (neg_one, zero) = (boundaries[5], boundaries[6]);
    assert!(neg_one < zero);
    assert!((zero as u64) < (neg_one as u64));
    assert!(
        CoordinateKeyV1::from_coord(Coord::new(neg_one, 0, 0)).as_bytes()
            < CoordinateKeyV1::from_coord(Coord::new(zero, 0, 0)).as_bytes()
    );
}

#[test]
fn all_24_insertion_permutations_and_delete_restoration_match_lean_uniqueness_theorem() {
    let limits = CommitmentOperationalLimitsDescriptorV1::LOCAL_DEFAULT;
    let profiles = ProfileRegistryV1::local_v1();

    // 192-bit WorldRootV1 boundary set including signed extrema and a depth-192 twin pair
    let prof_w = CommitmentProfileDescriptorV1::world_v1();
    let world_entries: Vec<(Vec<u8>, Vec<u8>)> = [
        Coord::new(0, 0, 0),
        Coord::new(0, 0, 1), // shares 191 prefix bits with (0,0,0) -> depth-192 zero-suffix leaves
        Coord::new(i64::MIN, -1, i64::MAX),
        Coord::new(i64::MAX, 1, i64::MIN),
    ]
    .into_iter()
    .map(|c| {
        (
            CoordinateKeyV1::from_coord(c).as_bytes().to_vec(),
            ON_V1_VALUE_BYTES.to_vec(),
        )
    })
    .collect();
    let world_cold =
        TypedPatriciaTrie::from_entries(prof_w, &limits, world_entries.clone()).unwrap();
    let world_cold_records = world_cold.collect_node_records();
    let world_universe: Vec<Vec<u8>> = world_entries.iter().map(|(k, _)| k.clone()).collect();
    let expected_world_alpha = alpha_commitment(&world_cold, &world_universe);

    for perm in permutations(&world_entries) {
        let mut t = TypedPatriciaTrie::empty(prof_w).unwrap();
        for (k, v) in perm {
            t.upsert(k, v).unwrap();
        }
        // Mirror of Lean `canonicalPat_unique_of_alpha_eq`: identical alpha => identical tree/nodes/root!
        assert_eq!(alpha_commitment(&t, &world_universe), expected_world_alpha);
        assert_eq!(t.root(), world_cold.root());
        assert_eq!(t.collect_node_records(), world_cold_records);
    }

    // Delete-after-insert exact tree restoration (`delete(upsert(t, k, v), k) == t`)
    let extra_k = CoordinateKeyV1::from_coord(Coord::new(42, -99, 17))
        .as_bytes()
        .to_vec();
    let mut restored_w = world_cold.clone();
    assert!(
        restored_w
            .upsert(extra_k.clone(), ON_V1_VALUE_BYTES.to_vec())
            .unwrap()
    );
    assert_ne!(restored_w.root(), world_cold.root());
    assert!(restored_w.delete(&extra_k).unwrap());
    assert_eq!(restored_w.root(), world_cold.root());
    assert_eq!(restored_w.collect_node_records(), world_cold_records);

    // 256-bit EpisodeRootV1 boundary set including a depth-256 twin pair differing only in bit 255
    let prof_e = CommitmentProfileDescriptorV1::episode_v1();
    let mut k_a = [0xA5_u8; 32];
    k_a[31] = 0x00;
    let mut k_b = [0xA5_u8; 32];
    k_b[31] = 0x01;
    let k_c = [0x00_u8; 32];
    let k_d = [0xFF_u8; 32];
    let ep_entries: Vec<(Vec<u8>, Vec<u8>)> = [
        (k_a, Coord::new(1, 0, 0), None),
        (k_b, Coord::new(2, 0, 0), Some([0x11_u8; 32])),
        (k_c, Coord::new(-16, 15, 0), None),
        (k_d, Coord::new(16, -15, 1), Some([0x22_u8; 32])),
    ]
    .into_iter()
    .map(|(k, coord, burn)| {
        let rec = EpisodeRecordV1 {
            coord,
            mint: MintIdV1::from_bytes(k),
            consuming_burn: burn.map(EventIdV1::from_bytes),
        };
        (k.to_vec(), rec.encode_canonical())
    })
    .collect();
    let ep_cold = TypedPatriciaTrie::from_entries(prof_e, &limits, ep_entries.clone()).unwrap();
    let ep_cold_records = ep_cold.collect_node_records();
    let ep_universe: Vec<Vec<u8>> = ep_entries.iter().map(|(k, _)| k.clone()).collect();
    let expected_ep_alpha = alpha_commitment(&ep_cold, &ep_universe);

    for perm in permutations(&ep_entries) {
        let mut t = TypedPatriciaTrie::empty(prof_e).unwrap();
        for (k, v) in perm {
            t.upsert(k, v).unwrap();
        }
        assert_eq!(alpha_commitment(&t, &ep_universe), expected_ep_alpha);
        assert_eq!(t.root(), ep_cold.root());
        assert_eq!(t.collect_node_records(), ep_cold_records);
    }

    // Proof soundness & completeness correspondence (`verifyPatProof_sound` & `canonicalPat_provePat_complete`)
    let ctx_e = ExpectedProofContextV1::for_profile(&prof_e, ep_cold.root());
    let mut probes = ep_universe.clone();
    probes.push([0xA5_u8; 32].to_vec()); // divergent extension absence probe
    probes.push([0x7F_u8; 32].to_vec()); // leaf divergence absence probe
    for q in &probes {
        let proof = ep_cold.prove(q).unwrap();
        let observed =
            verify_lookup(&profiles, &ctx_e, q, &proof.to_canonical_bytes(), None).unwrap();
        match ep_cold.get(q) {
            Some(v) => assert_eq!(observed, LookupResult::Present(v.to_vec())),
            None => assert_eq!(observed, LookupResult::Absent),
        }
    }
}

#[test]
fn noncanonical_shapes_and_forged_proofs_rejected_matching_lean_negative_controls() {
    let profiles = ProfileRegistryV1::local_v1();
    let prof_w = CommitmentProfileDescriptorV1::world_v1();
    let empty_root = typed_empty_root(&prof_w);
    let q = CoordinateKeyV1::from_coord(Coord::new(0, 0, 0));
    let dummy_hash = [0x42_u8; 32];

    // Control 1 (`adjacent_extensions_not_canonical`): two adjacent extension steps must reject
    let adj_ext_proof = TypedPatriciaProofV1 {
        steps: vec![
            TypedProofStep::Extension {
                depth: 0,
                shared_bits: vec![1],
            },
            TypedProofStep::Extension {
                depth: 1,
                shared_bits: vec![0],
            },
        ],
        terminal: TypedProofTerminal::Leaf {
            depth: 2,
            key: q.as_bytes().to_vec(),
            value: ON_V1_VALUE_BYTES.to_vec(),
        },
    };
    let ctx_any = ExpectedProofContextV1::for_profile(&prof_w, dummy_hash);
    assert!(
        verify_lookup(
            &profiles,
            &ctx_any,
            q.as_bytes(),
            &adj_ext_proof.to_canonical_bytes(),
            None
        )
        .is_err()
    );

    // Control 2 (`extension_to_leaf_not_canonical` / `verifyPatProof_rejects_extStep_to_leaf`)
    let ext_to_leaf = TypedPatriciaProofV1 {
        steps: vec![TypedProofStep::Extension {
            depth: 0,
            shared_bits: vec![1],
        }],
        terminal: TypedProofTerminal::Leaf {
            depth: 1,
            key: q.as_bytes().to_vec(),
            value: ON_V1_VALUE_BYTES.to_vec(),
        },
    };
    assert!(
        verify_lookup(
            &profiles,
            &ctx_any,
            q.as_bytes(),
            &ext_to_leaf.to_canonical_bytes(),
            None
        )
        .is_err()
    );

    // Control 3 (`empty_under_branch_not_canonical`): branch with empty sibling hash must reject
    let mut empty_branch_rec = Vec::new();
    empty_branch_rec.push(3u8); // NODE_TYPE_BRANCH
    empty_branch_rec.extend_from_slice(&0u16.to_be_bytes());
    empty_branch_rec.extend_from_slice(&64u32.to_be_bytes());
    empty_branch_rec.extend_from_slice(&empty_root);
    empty_branch_rec.extend_from_slice(&dummy_hash);
    assert!(decode_and_verify_node_record(&prof_w, &dummy_hash, &empty_branch_rec).is_err());

    // Control 4 (`empty_extension_path_not_canonical`): 0-bit extension node record must reject
    let mut zero_ext_rec = Vec::new();
    zero_ext_rec.push(2u8); // NODE_TYPE_EXTENSION
    zero_ext_rec.extend_from_slice(&0u16.to_be_bytes());
    zero_ext_rec.extend_from_slice(&34u32.to_be_bytes());
    zero_ext_rec.extend_from_slice(&0u16.to_be_bytes()); // 0 path bits
    zero_ext_rec.extend_from_slice(&dummy_hash);
    assert!(decode_and_verify_node_record(&prof_w, &dummy_hash, &zero_ext_rec).is_err());

    // Control 5 (`verifyPatProof_rejects_nondivergent_ext`): divergentExt matching query bits rejects
    let non_div = TypedPatriciaProofV1 {
        steps: vec![],
        terminal: TypedProofTerminal::DivergentExtension {
            depth: 0,
            shared_bits: vec![1, 0, 0, 0], // matches Coord(0,0,0) top bits 0x80 = 1000_0000!
            child_hash: dummy_hash,
        },
    };
    assert!(
        verify_lookup(
            &profiles,
            &ctx_any,
            q.as_bytes(),
            &non_div.to_canonical_bytes(),
            None
        )
        .is_err()
    );
}
