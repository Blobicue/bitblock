//! Reproducible `COMMIT-R1` comparative assay probe.
//!
//! Benchmarks Candidate A (`MerklePatriciaTrie`), Candidate B (`SparseMerkleTree`),
//! and reference `SparseOctreeV1` across the five `SCALE-1` workloads plus two
//! 1,000-key R4/R5 scale worlds (`Clustered1000` and `Dispersed1000`).
//!
//! Reports input-sort time separately from tree rebuild (per `AUTHENTICATED_STATE_PLAN.md` R0),
//! raw multi-sample p50/max timings, node counts, single incremental insert/delete work,
//! bounded point-proof verification under `ProofPolicyLimits`, clustered and dispersed
//! multiproof wire sizes, and live sibling-fork + delete/reinsert restoration parity.
//!
//! ```text
//! cargo run --release --features protocol-research --example commit_r1_assay_probe
//! cargo run --release --features protocol-research --example commit_r1_assay_probe -- --json
//! ```

use std::env;
use std::hint::black_box;
use std::time::Instant;

use bitblock::commit_r1::{
    CoordinateKeyV1, KEY_BITS, MerklePatriciaTrie, ProofPolicyLimits, SparseMerkleTree,
};
use bitblock::octree_commit::SparseOctreeV1;
use bitblock::scale_assay::{ALL_SCALE_WORKLOADS, ScaleWorkloadId, build_workload_world};
use bitblock::world::{Coord, World};

const REPEATS: usize = 7;

struct WorkloadRow {
    name: &'static str,
    keys: usize,
    chunks: usize,
    sort_p50_us: f64,
    patricia_rebuild_p50_us: f64,
    patricia_rebuild_max_us: f64,
    smt_rebuild_p50_us: f64,
    smt_rebuild_max_us: f64,
    octree_rebuild_p50_us: f64,
    patricia_total_nodes: usize,
    patricia_ext_nodes: usize,
    patricia_branch_nodes: usize,
    smt_non_default_branches: usize,
    patricia_present_proof_bytes: usize,
    smt_present_proof_bytes: usize,
    patricia_absent_proof_bytes: usize,
    smt_absent_proof_bytes: usize,
    patricia_verify_p50_us: f64,
    smt_verify_p50_us: f64,
    patricia_insert_hashes: usize,
    smt_insert_hashes: usize,
    patricia_insert_p50_us: f64,
    smt_insert_p50_us: f64,
    patricia_clustered_mp_bytes: usize,
    smt_clustered_mp_bytes: usize,
    patricia_dispersed_mp_bytes: usize,
    smt_dispersed_mp_bytes: usize,
    patricia_root_tag: String,
    smt_root_tag: String,
    octree_root_tag: String,
}

fn nearest_rank_p50_max(mut samples: Vec<f64>) -> (f64, f64) {
    samples.sort_by(f64::total_cmp);
    let n = samples.len();
    let p50 = samples[(n.div_ceil(2)).saturating_sub(1)];
    let max = samples[n - 1];
    (p50, max)
}

fn measure_us<R>(repeats: usize, mut f: impl FnMut() -> R) -> (f64, f64) {
    black_box(f());
    let mut samples = Vec::with_capacity(repeats);
    for _ in 0..repeats {
        let t0 = Instant::now();
        black_box(f());
        samples.push(t0.elapsed().as_nanos() as f64 / 1_000.0);
    }
    nearest_rank_p50_max(samples)
}

fn build_clustered_1000() -> World {
    let mut world = World::new();
    for x in 0..10_i64 {
        for y in 0..10_i64 {
            for z in 0..10_i64 {
                world.set_on(Coord::new(x, y, z));
            }
        }
    }
    world
}

fn build_dispersed_1000() -> World {
    let mut world = World::new();
    // Deterministic xorshift64 sequence spanning positive/negative signed chunks and far offsets
    let mut state: u64 = 0x517c_c1b7_2722_0a95;
    let mut next_coord = || -> i64 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let signed = (state & 0x000f_ffff) as i64 - 524_288;
        signed * 17
    };
    while world.len() < 1_000 {
        let c = Coord::new(next_coord(), next_coord(), next_coord());
        world.set_on(c);
    }
    world
}

fn assay_world(name: &'static str, world: &World) -> WorkloadRow {
    let raw_coords: Vec<Coord> = world.iter_on().collect();
    let (sort_p50_us, _) = measure_us(REPEATS, || {
        let mut keys: Vec<CoordinateKeyV1> = raw_coords
            .iter()
            .copied()
            .map(CoordinateKeyV1::from_coord)
            .collect();
        keys.sort_unstable();
        keys.dedup();
        keys
    });

    let mut sorted_keys: Vec<CoordinateKeyV1> = raw_coords
        .iter()
        .copied()
        .map(CoordinateKeyV1::from_coord)
        .collect();
    sorted_keys.sort_unstable();
    sorted_keys.dedup();

    let (patricia_rebuild_p50_us, patricia_rebuild_max_us) = measure_us(REPEATS, || {
        MerklePatriciaTrie::from_sorted_keys(sorted_keys.clone())
    });
    let (smt_rebuild_p50_us, smt_rebuild_max_us) = measure_us(REPEATS, || {
        SparseMerkleTree::from_sorted_keys(sorted_keys.clone())
    });
    let (octree_rebuild_p50_us, _) = measure_us(REPEATS, || SparseOctreeV1::from_world(world));

    let patricia = MerklePatriciaTrie::from_sorted_keys(sorted_keys.clone());
    let smt = SparseMerkleTree::from_sorted_keys(sorted_keys.clone());
    let octree = SparseOctreeV1::from_world(world);

    let (patricia_total_nodes, patricia_ext_nodes, patricia_branch_nodes) = patricia.node_counts();
    let smt_non_default_branches = smt.non_default_branch_count();

    let present_key = sorted_keys[sorted_keys.len() / 2];
    let mut absent_coord = Coord::new(999_999, -888_888, 777_777);
    while world.is_on(absent_coord) {
        absent_coord.x = absent_coord.x.wrapping_add(1);
    }
    let absent_key = CoordinateKeyV1::from_coord(absent_coord);

    let p_present_bytes = patricia.prove(&present_key).to_canonical_bytes();
    let s_present_bytes = smt.prove(&present_key).to_canonical_bytes();
    let p_absent_bytes = patricia.prove(&absent_key).to_canonical_bytes();
    let s_absent_bytes = smt.prove(&absent_key).to_canonical_bytes();

    // Verify bounded proofs under default R1 proof policies
    assert_eq!(
        MerklePatriciaTrie::verify_proof_bytes_with_limits(
            patricia.root(),
            &present_key,
            &p_present_bytes,
            ProofPolicyLimits::PATRICIA_R1_DEFAULT,
        ),
        Some(true)
    );
    assert_eq!(
        SparseMerkleTree::verify_proof_bytes_with_limits(
            smt.root(),
            &present_key,
            &s_present_bytes,
            ProofPolicyLimits::SMT_R1_DEFAULT,
        ),
        Some(true)
    );
    assert_eq!(
        MerklePatriciaTrie::verify_proof_bytes_with_limits(
            patricia.root(),
            &absent_key,
            &p_absent_bytes,
            ProofPolicyLimits::PATRICIA_R1_DEFAULT,
        ),
        Some(false)
    );
    assert_eq!(
        SparseMerkleTree::verify_proof_bytes_with_limits(
            smt.root(),
            &absent_key,
            &s_absent_bytes,
            ProofPolicyLimits::SMT_R1_DEFAULT,
        ),
        Some(false)
    );

    let (patricia_verify_p50_us, _) = measure_us(REPEATS * 8, || {
        MerklePatriciaTrie::verify_proof_bytes_with_limits(
            patricia.root(),
            &present_key,
            &p_present_bytes,
            ProofPolicyLimits::PATRICIA_R1_DEFAULT,
        )
    });
    let (smt_verify_p50_us, _) = measure_us(REPEATS * 8, || {
        SparseMerkleTree::verify_proof_bytes_with_limits(
            smt.root(),
            &present_key,
            &s_present_bytes,
            ProofPolicyLimits::SMT_R1_DEFAULT,
        )
    });

    // Incremental single insert/delete + live sibling fork + restoration parity
    let mut p_probe = patricia.clone();
    let (inserted, patricia_insert_hashes) = p_probe.insert_counted(absent_key);
    assert!(inserted);
    assert!(p_probe.delete(&absent_key));
    assert_eq!(p_probe.root(), patricia.root());
    assert_eq!(p_probe.root_node(), patricia.root_node());

    let mut s_probe = smt.clone();
    assert!(s_probe.insert(absent_key));
    assert!(s_probe.delete(&absent_key));
    assert_eq!(s_probe.root(), smt.root());

    let (patricia_insert_p50_us, _) = measure_us(REPEATS * 4, || {
        let mut fork = patricia.clone();
        fork.insert(absent_key);
        fork.root()
    });
    let (smt_insert_p50_us, _) = measure_us(REPEATS * 4, || {
        let mut fork = smt.clone();
        fork.insert(absent_key);
        fork.root()
    });

    // Clustered (first 8 sorted keys) and dispersed (evenly strided 8 keys) multiproof wire bytes
    let clustered_keys: Vec<CoordinateKeyV1> = sorted_keys.iter().copied().take(8).collect();
    let step = (sorted_keys.len() / 8).max(1);
    let dispersed_keys: Vec<CoordinateKeyV1> =
        sorted_keys.iter().step_by(step).copied().take(8).collect();

    WorkloadRow {
        name,
        keys: sorted_keys.len(),
        chunks: world.allocated_chunk_count(),
        sort_p50_us,
        patricia_rebuild_p50_us,
        patricia_rebuild_max_us,
        smt_rebuild_p50_us,
        smt_rebuild_max_us,
        octree_rebuild_p50_us,
        patricia_total_nodes,
        patricia_ext_nodes,
        patricia_branch_nodes,
        smt_non_default_branches,
        patricia_present_proof_bytes: p_present_bytes.len(),
        smt_present_proof_bytes: s_present_bytes.len(),
        patricia_absent_proof_bytes: p_absent_bytes.len(),
        smt_absent_proof_bytes: s_absent_bytes.len(),
        patricia_verify_p50_us,
        smt_verify_p50_us,
        patricia_insert_hashes,
        smt_insert_hashes: KEY_BITS + 1,
        patricia_insert_p50_us,
        smt_insert_p50_us,
        patricia_clustered_mp_bytes: patricia.multiproof_wire_bytes(&clustered_keys),
        smt_clustered_mp_bytes: smt.multiproof_wire_bytes(&clustered_keys),
        patricia_dispersed_mp_bytes: patricia.multiproof_wire_bytes(&dispersed_keys),
        smt_dispersed_mp_bytes: smt.multiproof_wire_bytes(&dispersed_keys),
        patricia_root_tag: patricia.root_tag().to_string(),
        smt_root_tag: smt.root_tag().to_string(),
        octree_root_tag: octree.root_tag().to_string(),
    }
}

fn workload_name(id: ScaleWorkloadId) -> &'static str {
    match id {
        ScaleWorkloadId::SingleOrigin => "SingleOrigin",
        ScaleWorkloadId::DenseCube16 => "DenseCube16",
        ScaleWorkloadId::HollowShell16 => "HollowShell16",
        ScaleWorkloadId::NegativeChunkCluster => "NegativeChunkCluster",
        ScaleWorkloadId::SparseFarExtrema => "SparseFarExtrema",
    }
}

fn main() {
    let json_mode = env::args().any(|a| a == "--json");

    let mut rows = Vec::new();
    for id in ALL_SCALE_WORKLOADS {
        let world = build_workload_world(id);
        rows.push(assay_world(workload_name(id), &world));
    }
    rows.push(assay_world("Clustered1000", &build_clustered_1000()));
    rows.push(assay_world("Dispersed1000", &build_dispersed_1000()));

    if json_mode {
        println!("{{");
        println!("  \"repeats\": {REPEATS},");
        println!("  \"workloads\": [");
        for (i, r) in rows.iter().enumerate() {
            let comma = if i + 1 == rows.len() { "" } else { "," };
            println!("    {{");
            println!("      \"name\": \"{}\",", r.name);
            println!("      \"keys\": {},", r.keys);
            println!("      \"chunks\": {},", r.chunks);
            println!("      \"input_sort_p50_us\": {:.2},", r.sort_p50_us);
            println!(
                "      \"patricia_rebuild_p50_us\": {:.2},",
                r.patricia_rebuild_p50_us
            );
            println!(
                "      \"patricia_rebuild_max_us\": {:.2},",
                r.patricia_rebuild_max_us
            );
            println!("      \"smt_rebuild_p50_us\": {:.2},", r.smt_rebuild_p50_us);
            println!("      \"smt_rebuild_max_us\": {:.2},", r.smt_rebuild_max_us);
            println!(
                "      \"octree_rebuild_p50_us\": {:.2},",
                r.octree_rebuild_p50_us
            );
            println!(
                "      \"patricia_total_nodes\": {},",
                r.patricia_total_nodes
            );
            println!("      \"patricia_ext_nodes\": {},", r.patricia_ext_nodes);
            println!(
                "      \"patricia_branch_nodes\": {},",
                r.patricia_branch_nodes
            );
            println!(
                "      \"smt_non_default_branches\": {},",
                r.smt_non_default_branches
            );
            println!(
                "      \"patricia_present_proof_bytes\": {},",
                r.patricia_present_proof_bytes
            );
            println!(
                "      \"smt_present_proof_bytes\": {},",
                r.smt_present_proof_bytes
            );
            println!(
                "      \"patricia_absent_proof_bytes\": {},",
                r.patricia_absent_proof_bytes
            );
            println!(
                "      \"smt_absent_proof_bytes\": {},",
                r.smt_absent_proof_bytes
            );
            println!(
                "      \"patricia_verify_p50_us\": {:.2},",
                r.patricia_verify_p50_us
            );
            println!("      \"smt_verify_p50_us\": {:.2},", r.smt_verify_p50_us);
            println!(
                "      \"patricia_insert_hashes\": {},",
                r.patricia_insert_hashes
            );
            println!("      \"smt_insert_hashes\": {},", r.smt_insert_hashes);
            println!(
                "      \"patricia_insert_p50_us\": {:.2},",
                r.patricia_insert_p50_us
            );
            println!("      \"smt_insert_p50_us\": {:.2},", r.smt_insert_p50_us);
            println!(
                "      \"patricia_clustered_mp_bytes\": {},",
                r.patricia_clustered_mp_bytes
            );
            println!(
                "      \"smt_clustered_mp_bytes\": {},",
                r.smt_clustered_mp_bytes
            );
            println!(
                "      \"patricia_dispersed_mp_bytes\": {},",
                r.patricia_dispersed_mp_bytes
            );
            println!(
                "      \"smt_dispersed_mp_bytes\": {},",
                r.smt_dispersed_mp_bytes
            );
            println!("      \"patricia_root_tag\": \"{}\",", r.patricia_root_tag);
            println!("      \"smt_root_tag\": \"{}\",", r.smt_root_tag);
            println!("      \"octree_root_tag\": \"{}\"", r.octree_root_tag);
            println!("    }}{comma}");
        }
        println!("  ]");
        println!("}}");
        return;
    }

    println!(
        "{:<20} {:>5} {:>6} {:>9} {:>9} {:>7} {:>7} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6}",
        "workload",
        "keys",
        "chunks",
        "mpt_us",
        "smt_us",
        "mpt_nd",
        "smt_br",
        "mpt_pB",
        "smt_pB",
        "mpt_v",
        "smt_v",
        "mpt_iH",
        "smt_iH"
    );
    for r in &rows {
        println!(
            "{:<20} {:>5} {:>6} {:>9.1} {:>9.1} {:>7} {:>7} {:>6} {:>6} {:>6.1} {:>6.1} {:>6} {:>6}",
            r.name,
            r.keys,
            r.chunks,
            r.patricia_rebuild_p50_us,
            r.smt_rebuild_p50_us,
            r.patricia_total_nodes,
            r.smt_non_default_branches,
            r.patricia_present_proof_bytes,
            r.smt_present_proof_bytes,
            r.patricia_verify_p50_us,
            r.smt_verify_p50_us,
            r.patricia_insert_hashes,
            r.smt_insert_hashes
        );
    }
}
