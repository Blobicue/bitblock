//! `REFINE-1@world`: Rust-to-Lean `World` / `ChunkRefinement` / `OctreeRefinement` correspondence.
//!
//! Mirrors the named abstraction function `toAbstractWorld : ChunkWorld -> World` and the
//! algebraic definitions in `proof/BitblockProof/ChunkRefinement.lean` and
//! `proof/BitblockProof/OctreeRefinement.lean` against live `bitblock::world::World`.
//!
//! Explicitly bounded: verifies all 4,096 local chunk indices exhaustively, all `13^3 = 2,197`
//! signed chunk-boundary/extrema triples, base+step edit/eviction/bedrock/octree simulation
//! traces, and powered negative controls. Does not claim an exhaustive enumeration of all `2^192`
//! `i64^3` points or certify `rustc`/LLVM code generation.

use std::collections::BTreeSet;

use bitblock::world::{BEDROCK_COORD, CHUNK_SIDE, Coord, OCTREE_MAX_DEPTH, OctantCoord, World};

/// Reference Lean-mirror abstraction function `alpha(World) -> BTreeSet<Coord>`.
fn alpha_world(world: &World) -> BTreeSet<Coord> {
    world.iter_on().collect()
}

/// Mirror of Lean `BitblockProof.splitAxis` (`Int.ediv` / `Int.emod` by 16).
fn lean_split_axis(x: i64) -> (i64, usize) {
    (x.div_euclid(CHUNK_SIDE), x.rem_euclid(CHUNK_SIDE) as usize)
}

/// Mirror of Lean `BitblockProof.joinAxis`.
fn lean_join_axis(cx: i64, lx: usize) -> i64 {
    cx.checked_mul(CHUNK_SIDE)
        .expect("joinAxis chunk mul in i64")
        + (lx as i64)
}

/// Mirror of Lean `BitblockProof.localIndex` / `unflattenIndex`.
fn lean_local_index(lx: usize, ly: usize, lz: usize) -> usize {
    lx + 16 * (ly + 16 * lz)
}

fn lean_unflatten_index(idx: usize) -> (usize, usize, usize) {
    (idx % 16, (idx / 16) % 16, idx / 256)
}

#[test]
fn exhaustive_4096_local_index_bijection_and_chunk_word_layout_match_lean_model() {
    let mut seen = vec![false; 4096];
    let mut world = World::new();

    for lz in 0..16usize {
        for ly in 0..16usize {
            for lx in 0..16usize {
                let idx = lean_local_index(lx, ly, lz);
                assert!(idx < 4096);
                assert!(!seen[idx], "duplicate local index {idx}");
                seen[idx] = true;
                assert_eq!(lean_unflatten_index(idx), (lx, ly, lz));

                // Verify live Rust `World::chunk_of` agrees on both chunk (0,0,0) and chunk (-1,-1,-1)
                let pos_coord = Coord::new(lx as i64, ly as i64, lz as i64);
                let (pos_cc, pos_idx) = World::chunk_of(pos_coord);
                assert_eq!((pos_cc.x, pos_cc.y, pos_cc.z), (0, 0, 0));
                assert_eq!(pos_idx, idx);

                let neg_coord = Coord::new(-16 + lx as i64, -16 + ly as i64, -16 + lz as i64);
                let (neg_cc, neg_idx) = World::chunk_of(neg_coord);
                assert_eq!((neg_cc.x, neg_cc.y, neg_cc.z), (-1, -1, -1));
                assert_eq!(neg_idx, idx);

                world.set_on(neg_coord);
            }
        }
    }
    assert!(seen.iter().all(|&b| b));

    // Confirm all 64 u64 words of chunk (-1,-1,-1) are saturated (u64::MAX) and popcount == 4096
    let (neg_cc, _) = World::chunk_of(Coord::new(-1, -1, -1));
    let (count, words) = world
        .chunk_words(neg_cc)
        .expect("negative chunk must be allocated");
    assert_eq!(count, 4096);
    assert!(words.iter().all(|&w| w == u64::MAX));
}

#[test]
fn signed_boundary_and_extrema_chunk_splitting_matches_lean_with_truncated_div_control() {
    let boundary_values: [i64; 13] = [
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

    for &x in &boundary_values {
        let (cx, lx) = lean_split_axis(x);
        assert!(lx < 16);
        assert_eq!(lean_join_axis(cx, lx), x);
        for &y in &boundary_values {
            let (cy, ly) = lean_split_axis(y);
            for &z in &boundary_values {
                let (cz, lz) = lean_split_axis(z);
                let c = Coord::new(x, y, z);
                let (cc, idx) = World::chunk_of(c);
                assert_eq!((cc.x, cc.y, cc.z), (cx, cy, cz));
                assert_eq!(idx, lean_local_index(lx, ly, lz));
                assert_eq!(lean_unflatten_index(idx), (lx, ly, lz));
            }
        }
    }

    // Powered negative control mirroring Lean `truncDiv_breaks_octree_partitioning`:
    // Rust `/` truncates toward zero (-1 / 16 == 0) and disagrees with Euclidean `World::chunk_of` (-1).
    let (neg_one_chunk, neg_one_idx) = World::chunk_of(Coord::new(-1, 0, 0));
    assert_eq!(neg_one_chunk.x, -1);
    assert_eq!(neg_one_idx, 15);
    assert_ne!((-1_i64) / 16, neg_one_chunk.x);
}

#[test]
fn base_step_simulation_eviction_bedrock_and_octree_flush_refine_abstract_model() {
    // Base state: abstract world has exactly { (0,0,0) }
    let mut world = World::new();
    let mut model: BTreeSet<Coord> = BTreeSet::from([BEDROCK_COORD]);
    assert_eq!(alpha_world(&world), model);
    assert_eq!(world.allocated_chunk_count(), 1);

    let script = [
        (0u8, BEDROCK_COORD),                         // bedrock On -> no-op
        (1u8, BEDROCK_COORD),                         // bedrock Off -> protected!
        (2u8, BEDROCK_COORD),                         // bedrock Toggle -> protected!
        (0u8, Coord::new(-1, -1, -1)),                // negative chunk (-1,-1,-1)
        (0u8, Coord::new(-16, -16, -16)),             // same negative chunk (-1,-1,-1)
        (0u8, Coord::new(-17, 15, 16)),               // distinct chunk (-2, 0, 1)
        (0u8, Coord::new(1_048_576, -1_048_576, 42)), // far macro-octant
        (1u8, Coord::new(-1, -1, -1)),                // chunk (-1,-1,-1) still non-empty
        (1u8, Coord::new(-16, -16, -16)),             // chunk (-1,-1,-1) must now evict!
        (2u8, Coord::new(-17, 15, 16)),               // chunk (-2,0,1) must now evict!
        (0u8, Coord::new(i64::MIN, 0, i64::MAX)),     // signed extrema!
        (1u8, Coord::new(i64::MIN, 0, i64::MAX)),     // extrema chunk evicts!
    ];

    // Run both eager and deferred-octree worlds side by side and compare with abstract model
    let mut deferred = World::new();
    deferred.defer_octree();

    for &(op, coord) in &script {
        let rust_changed = match op {
            0 => world.set_on(coord),
            1 => world.set_off(coord),
            _ => world.toggle(coord),
        };
        let def_changed = match op {
            0 => deferred.set_on(coord),
            1 => deferred.set_off(coord),
            _ => deferred.toggle(coord),
        };
        assert_eq!(rust_changed, def_changed);

        let model_ret = if coord == BEDROCK_COORD {
            // `set_on`/`set_off` return `false` (unchanged); `toggle` returns the new state (`true`).
            op == 2
        } else {
            match op {
                0 => model.insert(coord),
                1 => model.remove(&coord),
                _ => {
                    if model.contains(&coord) {
                        model.remove(&coord);
                        false
                    } else {
                        model.insert(coord);
                        true
                    }
                }
            }
        };
        assert_eq!(rust_changed, model_ret);
        assert_eq!(alpha_world(&world), model);
        assert_eq!(world.len(), model.len());
        assert!(world.is_on(BEDROCK_COORD));

        // Verify `for_each_on_ordered` reproduces exact sorted BTreeSet order
        let mut ordered = Vec::new();
        world.for_each_on_ordered(|c| ordered.push(c));
        assert_eq!(ordered, model.iter().copied().collect::<Vec<_>>());

        // Verify empty non-origin chunks are evicted (every allocated chunk has count > 0)
        for (_, count, _, _) in world.iter_chunks() {
            assert!(count > 0, "empty chunk was not evicted");
        }
        let expected_chunks: BTreeSet<_> = model.iter().map(|&c| World::chunk_of(c).0).collect();
        assert_eq!(world.allocated_chunk_count(), expected_chunks.len());
    }

    // Flush deferred octree and verify exact root-octant & summary equality with eager `world`
    deferred.flush_octree();
    let eager_roots: BTreeSet<(OctantCoord, usize)> = world
        .root_octants()
        .map(|(oc, s)| (oc, s.occupied_bits))
        .collect();
    let flushed_roots: BTreeSet<(OctantCoord, usize)> = deferred
        .root_octants()
        .map(|(oc, s)| (oc, s.occupied_bits))
        .collect();
    assert_eq!(eager_roots, flushed_roots);
    assert_eq!(
        eager_roots.iter().map(|(_, bits)| *bits).sum::<usize>(),
        model.len()
    );

    // Verify conservative AABB enclosure for every occupied voxel in `model`
    for &c in &model {
        let (cc, _) = World::chunk_of(c);
        let root_oct = OctantCoord {
            depth: OCTREE_MAX_DEPTH,
            x: cc.x.div_euclid(1_i64 << OCTREE_MAX_DEPTH),
            y: cc.y.div_euclid(1_i64 << OCTREE_MAX_DEPTH),
            z: cc.z.div_euclid(1_i64 << OCTREE_MAX_DEPTH),
        };
        let summary = world
            .octant_summary(root_oct)
            .expect("every occupied voxel must be covered by an active depth-8 root octant");
        assert!(summary.occupied_bits > 0);
        assert!(c.x >= summary.bounds.min.x && c.x <= summary.bounds.max.x);
        assert!(c.y >= summary.bounds.min.y && c.y <= summary.bounds.max.y);
        assert!(c.z >= summary.bounds.min.z && c.z <= summary.bounds.max.z);
    }

    // Powered negative control: dropping bedrock from abstract model breaks alpha equality immediately
    let mut mutated_model = model;
    mutated_model.remove(&BEDROCK_COORD);
    assert_ne!(alpha_world(&world), mutated_model);
}
