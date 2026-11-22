//! Reproducible `VERKLE-R1@feasibility` comparative structural and witness-crossover probe.
//!
//! Runs on the exact same 7 worlds benchmarked in `COMMIT-R1` (`SingleOrigin`, `DenseCube16`,
//! `HollowShell16`, `NegativeChunkCluster`, `SparseFarExtrema`, `Clustered1000`, `Dispersed1000`)
//! and compares:
//! - Live measured `COMMIT-1` `TypedPatriciaTrie` (`WorldRootV1`) single/batch proofs & verification,
//! - Exact deterministic Arity-256 Bandersnatch IPA Verkle structural & whole-witness bytes,
//! - Exact deterministic Arity-256 BLS12-381 KZG (rejected trusted-setup) whole-witness bytes,
//! - Crossover batch sizes (1, 8, 64, 256 dispersed keys on `Dispersed1000`).
//!
//! ```text
//! cargo run --release --features protocol-research --example verkle_r1_feasibility_probe
//! cargo run --release --features protocol-research --example verkle_r1_feasibility_probe -- --json
//! ```

use std::env;
use std::hint::black_box;
use std::time::Instant;

use bitblock::commit_r1::{CoordinateKeyV1, MerklePatriciaTrie};
use bitblock::commit_state::{
    CommitmentOperationalLimitsDescriptorV1, CommitmentProfileDescriptorV1, ExpectedProofContextV1,
    ON_V1_VALUE_BYTES, ProfileRegistryV1, TypedPatriciaTrie, verify_lookup,
};
use bitblock::scale_assay::{ALL_SCALE_WORKLOADS, ScaleWorkloadId, build_workload_world};
use bitblock::verkle_feasibility::{
    IPA_256_OPENING_FLOOR_BYTES, KZG_BLS12_381_OPENING_FLOOR_BYTES,
    VectorCommitmentProfileDescriptorV1, analyze_verkle_256_world,
    estimate_verkle_batch_whole_witness_bytes,
};
use bitblock::world::{Coord, World};

const REPEATS: usize = 7;

fn p50_us<R>(repeats: usize, mut f: impl FnMut() -> R) -> f64 {
    black_box(f());
    let mut samples = Vec::with_capacity(repeats);
    for _ in 0..repeats {
        let t0 = Instant::now();
        black_box(f());
        samples.push(t0.elapsed().as_nanos() as f64 / 1_000.0);
    }
    samples.sort_by(f64::total_cmp);
    samples[(samples.len().div_ceil(2)).saturating_sub(1)]
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
    let mut state: u64 = 0x517c_c1b7_2722_0a95;
    let mut next_coord = || -> i64 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let signed = (state & 0x000f_ffff) as i64 - 524_288;
        signed * 17
    };
    while world.len() < 1_000 {
        world.set_on(Coord::new(next_coord(), next_coord(), next_coord()));
    }
    world
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

struct ComparisonRow {
    name: &'static str,
    keys: usize,
    chunks: usize,
    patricia_logical_nodes: usize,
    commit1_unique_cas_nodes: usize,
    commit1_patricia_single_proof_bytes: usize,
    commit1_patricia_verify_p50_us: f64,
    patricia_clustered_8_mp_bytes: usize,
    patricia_dispersed_8_mp_bytes: usize,
    verkle_unique_stems: usize,
    verkle_internal_nodes: usize,
    verkle_max_internal_depth: usize,
    verkle_ipa_single_whole_bytes: usize,
    verkle_kzg_single_whole_bytes: usize,
    verkle_ipa_clustered_8_whole_bytes: usize,
    verkle_ipa_dispersed_8_whole_bytes: usize,
    verkle_single_edit_scalar_muls: usize,
    verkle_ipa_verify_msm_points: usize,
}

fn assay_world(name: &'static str, world: &World) -> ComparisonRow {
    let prof_w = CommitmentProfileDescriptorV1::world_v1();
    let limits = CommitmentOperationalLimitsDescriptorV1::LOCAL_DEFAULT;
    let profiles = ProfileRegistryV1::local_v1();

    let mut coords: Vec<Coord> = world.iter_on().collect();
    coords.sort_unstable();
    let entries: Vec<(Vec<u8>, Vec<u8>)> = coords
        .iter()
        .copied()
        .map(|c| {
            (
                CoordinateKeyV1::from_coord(c).as_bytes().to_vec(),
                ON_V1_VALUE_BYTES.to_vec(),
            )
        })
        .collect();
    let c1_trie = TypedPatriciaTrie::from_entries(prof_w, &limits, entries).unwrap();
    let c1_nodes = c1_trie.collect_node_records().len();

    let median_idx = coords.len() / 2;
    let median_key = CoordinateKeyV1::from_coord(coords[median_idx]);
    let c1_proof = c1_trie.prove(median_key.as_bytes()).unwrap();
    let c1_proof_bytes = c1_proof.to_canonical_bytes();
    let ctx = ExpectedProofContextV1::for_profile(&prof_w, c1_trie.root());
    let c1_verify_us = p50_us(REPEATS * 8, || {
        verify_lookup(
            &profiles,
            &ctx,
            median_key.as_bytes(),
            &c1_proof_bytes,
            None,
        )
        .unwrap()
    });

    // Reference Patricia multiproof sizes from COMMIT-R1
    let mut r1_keys: Vec<CoordinateKeyV1> = coords
        .iter()
        .copied()
        .map(CoordinateKeyV1::from_coord)
        .collect();
    r1_keys.sort_unstable();
    let r1_pat = MerklePatriciaTrie::from_sorted_keys(r1_keys.clone());
    let (r1_logical_nodes, _, _) = r1_pat.node_counts();
    let clustered_coords: Vec<Coord> = coords.iter().copied().take(8).collect();
    let step = (coords.len() / 8).max(1);
    let dispersed_coords: Vec<Coord> = coords.iter().step_by(step).copied().take(8).collect();
    let clustered_r1: Vec<CoordinateKeyV1> = clustered_coords
        .iter()
        .copied()
        .map(CoordinateKeyV1::from_coord)
        .collect();
    let dispersed_r1: Vec<CoordinateKeyV1> = dispersed_coords
        .iter()
        .copied()
        .map(CoordinateKeyV1::from_coord)
        .collect();

    let vm = analyze_verkle_256_world(world, &clustered_coords, &dispersed_coords);

    ComparisonRow {
        name,
        keys: coords.len(),
        chunks: world.allocated_chunk_count(),
        patricia_logical_nodes: r1_logical_nodes,
        commit1_unique_cas_nodes: c1_nodes,
        commit1_patricia_single_proof_bytes: c1_proof_bytes.len(),
        commit1_patricia_verify_p50_us: c1_verify_us,
        patricia_clustered_8_mp_bytes: r1_pat.multiproof_wire_bytes(&clustered_r1),
        patricia_dispersed_8_mp_bytes: r1_pat.multiproof_wire_bytes(&dispersed_r1),
        verkle_unique_stems: vm.unique_stems,
        verkle_internal_nodes: vm.internal_nodes,
        verkle_max_internal_depth: vm.max_internal_depth,
        verkle_ipa_single_whole_bytes: vm.single_query_ipa_whole_witness_bytes,
        verkle_kzg_single_whole_bytes: vm.single_query_kzg_whole_witness_bytes,
        verkle_ipa_clustered_8_whole_bytes: vm.clustered_batch_ipa_whole_witness_bytes,
        verkle_ipa_dispersed_8_whole_bytes: vm.dispersed_batch_ipa_whole_witness_bytes,
        verkle_single_edit_scalar_muls: vm.single_edit_pedersen_scalar_muls,
        verkle_ipa_verify_msm_points: vm.ipa_verify_msm_points,
    }
}

fn main() {
    let json_mode = env::args().any(|a| a == "--json");
    let bander = VectorCommitmentProfileDescriptorV1::bandersnatch_ipa_256_v1();
    let kzg = VectorCommitmentProfileDescriptorV1::bls12_381_kzg_256_rejected_v1();

    let mut rows = Vec::new();
    for id in ALL_SCALE_WORKLOADS {
        let world = build_workload_world(id);
        rows.push(assay_world(workload_name(id), &world));
    }
    rows.push(assay_world("Clustered1000", &build_clustered_1000()));
    let dispersed_1000 = build_dispersed_1000();
    rows.push(assay_world("Dispersed1000", &dispersed_1000));

    // Batch-size crossover sweep on Dispersed1000: 1, 8, 32, 64, 128, 256 dispersed keys
    let mut d_coords: Vec<Coord> = dispersed_1000.iter_on().collect();
    d_coords.sort_unstable();
    let d_keys: Vec<CoordinateKeyV1> = d_coords
        .iter()
        .copied()
        .map(CoordinateKeyV1::from_coord)
        .collect();
    let d_pat = MerklePatriciaTrie::from_sorted_keys(d_keys.clone());
    let mut crossover = Vec::new();
    for batch_k in [1usize, 8, 32, 64, 128, 256] {
        let stride = (d_coords.len() / batch_k).max(1);
        let q_coords: Vec<Coord> = d_coords
            .iter()
            .step_by(stride)
            .copied()
            .take(batch_k)
            .collect();
        let q_r1: Vec<CoordinateKeyV1> = q_coords
            .iter()
            .copied()
            .map(CoordinateKeyV1::from_coord)
            .collect();
        let pat_mp_bytes = d_pat.multiproof_wire_bytes(&q_r1);
        let ipa_bytes = estimate_verkle_batch_whole_witness_bytes(
            &dispersed_1000,
            &q_coords,
            32,
            IPA_256_OPENING_FLOOR_BYTES as usize,
        );
        let kzg_bytes = estimate_verkle_batch_whole_witness_bytes(
            &dispersed_1000,
            &q_coords,
            48,
            KZG_BLS12_381_OPENING_FLOOR_BYTES as usize,
        );
        crossover.push((q_coords.len(), pat_mp_bytes, ipa_bytes, kzg_bytes));
    }

    if json_mode {
        println!("{{");
        println!("  \"bandersnatch_ipa_profile_tag\": \"{}\",", bander.tag());
        println!(
            "  \"bls12_381_kzg_rejected_profile_tag\": \"{}\",",
            kzg.tag()
        );
        println!("  \"workloads\": [");
        for (i, r) in rows.iter().enumerate() {
            let comma = if i + 1 == rows.len() { "" } else { "," };
            println!("    {{");
            println!("      \"name\": \"{}\",", r.name);
            println!("      \"keys\": {},", r.keys);
            println!("      \"chunks\": {},", r.chunks);
            println!(
                "      \"patricia_logical_nodes\": {},",
                r.patricia_logical_nodes
            );
            println!(
                "      \"commit1_unique_cas_nodes\": {},",
                r.commit1_unique_cas_nodes
            );
            println!(
                "      \"commit1_patricia_single_proof_bytes\": {},",
                r.commit1_patricia_single_proof_bytes
            );
            println!(
                "      \"commit1_patricia_verify_p50_us\": {:.2},",
                r.commit1_patricia_verify_p50_us
            );
            println!(
                "      \"patricia_clustered_8_mp_bytes\": {},",
                r.patricia_clustered_8_mp_bytes
            );
            println!(
                "      \"patricia_dispersed_8_mp_bytes\": {},",
                r.patricia_dispersed_8_mp_bytes
            );
            println!("      \"verkle_unique_stems\": {},", r.verkle_unique_stems);
            println!(
                "      \"verkle_internal_nodes\": {},",
                r.verkle_internal_nodes
            );
            println!(
                "      \"verkle_max_internal_depth\": {},",
                r.verkle_max_internal_depth
            );
            println!(
                "      \"verkle_ipa_single_whole_bytes\": {},",
                r.verkle_ipa_single_whole_bytes
            );
            println!(
                "      \"verkle_kzg_single_whole_bytes\": {},",
                r.verkle_kzg_single_whole_bytes
            );
            println!(
                "      \"verkle_ipa_clustered_8_whole_bytes\": {},",
                r.verkle_ipa_clustered_8_whole_bytes
            );
            println!(
                "      \"verkle_ipa_dispersed_8_whole_bytes\": {},",
                r.verkle_ipa_dispersed_8_whole_bytes
            );
            println!(
                "      \"verkle_single_edit_scalar_muls\": {},",
                r.verkle_single_edit_scalar_muls
            );
            println!(
                "      \"verkle_ipa_verify_msm_points\": {}",
                r.verkle_ipa_verify_msm_points
            );
            println!("    }}{comma}");
        }
        println!("  ],");
        println!("  \"dispersed_1000_batch_crossover\": [");
        for (i, (k, pat_b, ipa_b, kzg_b)) in crossover.iter().enumerate() {
            let comma = if i + 1 == crossover.len() { "" } else { "," };
            println!(
                "    {{\"batch_keys\": {k}, \"patricia_multiproof_bytes\": {pat_b}, \"verkle_ipa_whole_witness_bytes\": {ipa_b}, \"verkle_kzg_whole_witness_bytes\": {kzg_b}}}{comma}"
            );
        }
        println!("  ]");
        println!("}}");
        return;
    }

    println!(
        "VERKLE-R1 feasibility profile (Bandersnatch IPA-256): {}",
        bander.tag()
    );
    println!(
        "VERKLE-R1 rejected KZG profile (BLS12-381 KZG-256):   {}",
        kzg.tag()
    );
    println!(
        "\n{:<20} {:>5} {:>5} {:>6} {:>7} {:>6} {:>7} {:>7} {:>7} {:>7}",
        "workload",
        "keys",
        "stems",
        "c1_pB",
        "c1_v_us",
        "ipa1B",
        "kzg1B",
        "pat8dB",
        "ipa8dB",
        "v_mul"
    );
    for r in &rows {
        println!(
            "{:<20} {:>5} {:>5} {:>6} {:>7.2} {:>6} {:>7} {:>7} {:>7} {:>7}",
            r.name,
            r.keys,
            r.verkle_unique_stems,
            r.commit1_patricia_single_proof_bytes,
            r.commit1_patricia_verify_p50_us,
            r.verkle_ipa_single_whole_bytes,
            r.verkle_kzg_single_whole_bytes,
            r.patricia_dispersed_8_mp_bytes,
            r.verkle_ipa_dispersed_8_whole_bytes,
            r.verkle_single_edit_scalar_muls
        );
    }
    println!("\nDispersed1000 batch crossover (bytes):");
    for (k, pat_b, ipa_b, kzg_b) in &crossover {
        println!(
            "  k={:<3}  Patricia_MP={:>6} B   Verkle_IPA_whole={:>6} B   Verkle_KZG_whole={:>6} B",
            k, pat_b, ipa_b, kzg_b
        );
    }
}
