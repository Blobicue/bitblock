//! Reproducible `COST-1` measurement probe for the implemented `COMMIT-1` authenticated
//! local-state profile (`WorldRootV1`, `ActiveTokenRootV1`, `EpisodeRootV1`,
//! `CommitmentSetV1`, `MemoryCommitmentStore`, `DurableAuthenticatedLedger`, GC, and recovery).
//!
//! Reports deterministic counters (node counts, canonical wire bytes, proof bytes, GC
//! reclaimed nodes) alongside 7-repeat p50/max timings, and executes a powered subject
//! control verifying that tightening `ProofPolicyDescriptorV1::max_proof_bytes` by 1 byte
//! below the observed proof length rejects fail-closed.
//!
//! ```text
//! cargo run --release --features protocol-research --example commit_1_cost_probe
//! cargo run --release --features protocol-research --example commit_1_cost_probe -- --json
//! ```

use std::collections::HashSet;
use std::env;
use std::fs;
use std::hint::black_box;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use bitblock::commit_r1::CoordinateKeyV1;
use bitblock::commit_state::{
    CommitmentOperationalLimitsDescriptorV1, CommitmentProfileDescriptorV1, CrashPoint,
    DomainRegistryV1, DurableAuthenticatedLedger, ExpectedProofContextV1, GcSweepOutcome,
    LocalAuthenticatedSidecar, LookupResult, MemoryCommitmentStore, ON_V1_VALUE_BYTES,
    ProfileRegistryV1, ProofPolicyDescriptorV1, TypedPatriciaTrie, verify_independent_projection,
    verify_lookup,
};
use bitblock::coordinate_episode::OccupancyEpisodeLedger;
use bitblock::history::History;
use bitblock::history_store::HistoryReplayLimits;
use bitblock::journal::AnchorPolicy;
use bitblock::local_bounds::LocalStorageProfile;
use bitblock::program::{Edit, Operation};
use bitblock::transition_spec::BaseTransitionSpecD1;
use bitblock::world::{Coord, World};

const REPEATS: usize = 7;

fn p50_max_us<R>(repeats: usize, mut f: impl FnMut() -> R) -> (f64, f64) {
    black_box(f());
    let mut samples = Vec::with_capacity(repeats);
    for _ in 0..repeats {
        let t0 = Instant::now();
        black_box(f());
        samples.push(t0.elapsed().as_nanos() as f64 / 1_000.0);
    }
    samples.sort_by(f64::total_cmp);
    let n = samples.len();
    (samples[(n.div_ceil(2)).saturating_sub(1)], samples[n - 1])
}

fn main() {
    let json_mode = env::args().any(|a| a == "--json");
    let profile = LocalStorageProfile::DEFAULT;
    let profiles = ProfileRegistryV1::local_v1();
    let domain_registry = DomainRegistryV1::local_v1();
    let op_limits = CommitmentOperationalLimitsDescriptorV1::LOCAL_DEFAULT;
    let scope_digest = BaseTransitionSpecD1::canonical_base_only()
        .tag()
        .expect("base transition spec tag")
        .digest();

    // 1. In-memory 64-tick mint/burn/remint trajectory on LocalAuthenticatedSidecar + MemoryCommitmentStore
    let initial = World::new();
    let mut world = initial.clone();
    let mut history = History::new(&initial);
    let mut episodes = OccupancyEpisodeLedger::from_history(&initial, &history).unwrap();
    let mut sidecar = LocalAuthenticatedSidecar::from_genesis(
        &initial,
        &history,
        &episodes,
        &profile,
        scope_digest,
    )
    .unwrap();
    let mut mem_store = MemoryCommitmentStore::new();
    let genesis_nodes = sidecar.collect_all_node_records();
    let genesis_set_id = mem_store
        .publish_with_crash_point(
            None,
            sidecar.current_set(),
            &genesis_nodes,
            CrashPoint::None,
        )
        .unwrap();

    let mut advance_samples_us = Vec::new();
    let mut prev_set_id = genesis_set_id;
    let mut intermediate_ids = Vec::new();
    let mut last_event_id = None;
    for tick in 1..=64_u64 {
        let mut tx = history.begin_tick(tick);
        let c = Coord::new((tick as i64) * 3, -(tick as i64), (tick as i64) % 17);
        assert!(tx.apply_edit(&history, &mut world, Edit::new(Operation::On, c)));
        if tick > 16 && tick % 4 == 0 {
            let burn_c = Coord::new(
                ((tick - 16) as i64) * 3,
                -((tick - 16) as i64),
                ((tick - 16) as i64) % 17,
            );
            assert!(tx.apply_edit(&history, &mut world, Edit::new(Operation::Off, burn_c)));
        }
        let receipt = history.commit_tick(tx, &world).unwrap().clone();
        episodes.apply_receipt(&receipt).unwrap();
        for ev in receipt.effects() {
            if let bitblock::history::TransitionEffect::Mint { event, .. } = *ev {
                last_event_id = Some(event.digest());
            }
        }
        let t0 = Instant::now();
        let next_set = sidecar
            .advance_for_receipt(&initial, &world, &history, &episodes, &receipt)
            .unwrap();
        advance_samples_us.push(t0.elapsed().as_nanos() as f64 / 1_000.0);

        let nodes = sidecar.collect_all_node_records();
        prev_set_id = mem_store
            .publish_with_crash_point(Some(prev_set_id), &next_set, &nodes, CrashPoint::None)
            .unwrap();
        if tick < 64 {
            intermediate_ids.push(prev_set_id);
        }
    }
    advance_samples_us.sort_by(f64::total_cmp);
    let advance_p50_us =
        advance_samples_us[(advance_samples_us.len().div_ceil(2)).saturating_sub(1)];
    let advance_max_us = *advance_samples_us.last().unwrap();

    let world_nodes_64 = sidecar.world_trie().collect_node_records().len();
    let active_nodes_64 = sidecar.active_token_trie().collect_node_records().len();
    let episode_nodes_64 = sidecar.episode_trie().collect_node_records().len();
    let commitment_set_bytes = sidecar.current_set().encode_canonical().len();
    let mem_nodes_before_gc = mem_store.node_count();

    // 2. Write-barrier GC sweep reclaiming unpinned intermediate nodes while keeping genesis + head pinned
    for id in &intermediate_ids {
        assert!(mem_store.unpin_permanent(id).unwrap());
    }
    let gc_mark = mem_store.begin_gc_mark().unwrap();
    let (gc_reclaimed_nodes, gc_remaining_nodes) = match mem_store.finish_gc_sweep(&gc_mark) {
        GcSweepOutcome::Completed {
            reclaimed_nodes,
            remaining_nodes,
        } => (reclaimed_nodes, remaining_nodes),
        other => panic!("expected clean GC completion, got {other:?}"),
    };
    assert!(gc_reclaimed_nodes > 0);

    // 3. Point proofs and verification across all 3 domains + powered 1-byte-tighter cap control
    let prof_w = CommitmentProfileDescriptorV1::world_v1();
    let prof_a = CommitmentProfileDescriptorV1::active_token_v1();
    let prof_e = CommitmentProfileDescriptorV1::episode_v1();
    let q_coord = Coord::new(63 * 3, -63, 63 % 17);
    let q_ckey = CoordinateKeyV1::from_coord(q_coord);
    let q_ev = last_event_id.expect("at least one event committed");

    let w_proof = sidecar
        .world_trie()
        .prove(q_ckey.as_bytes())
        .unwrap()
        .to_canonical_bytes();
    let a_proof = sidecar
        .active_token_trie()
        .prove(q_ckey.as_bytes())
        .unwrap()
        .to_canonical_bytes();
    let e_proof = sidecar
        .episode_trie()
        .prove(&q_ev)
        .unwrap()
        .to_canonical_bytes();

    let ctx_w = ExpectedProofContextV1::for_profile(&prof_w, sidecar.world_trie().root());
    let ctx_a = ExpectedProofContextV1::for_profile(&prof_a, sidecar.active_token_trie().root());
    let ctx_e = ExpectedProofContextV1::for_profile(&prof_e, sidecar.episode_trie().root());

    let (verify_world_p50_us, verify_world_max_us) = p50_max_us(REPEATS * 8, || {
        assert_eq!(
            verify_lookup(&profiles, &ctx_w, q_ckey.as_bytes(), &w_proof, None).unwrap(),
            LookupResult::Present(ON_V1_VALUE_BYTES.to_vec())
        );
    });
    let (verify_active_p50_us, _) = p50_max_us(REPEATS * 8, || {
        assert!(matches!(
            verify_lookup(&profiles, &ctx_a, q_ckey.as_bytes(), &a_proof, None).unwrap(),
            LookupResult::Present(_)
        ));
    });
    let (verify_episode_p50_us, _) = p50_max_us(REPEATS * 8, || {
        assert!(matches!(
            verify_lookup(&profiles, &ctx_e, &q_ev, &e_proof, None).unwrap(),
            LookupResult::Present(_)
        ));
    });

    // Powered subject control: tightening max_proof_bytes by 1 byte below observed proof length MUST reject
    let mut tighter = ProofPolicyDescriptorV1::WORLD_DEFAULT;
    tighter.max_proof_bytes = (w_proof.len() - 1) as u32;
    let powered_one_byte_tighter_rejected = verify_lookup(
        &profiles,
        &ctx_w,
        q_ckey.as_bytes(),
        &w_proof,
        Some(tighter),
    )
    .is_err();
    assert!(powered_one_byte_tighter_rejected);

    // 4. Independent full projection rebuild verification (64 ticks)
    let no_pruned = HashSet::new();
    let (independent_proj_p50_us, independent_proj_max_us) = p50_max_us(REPEATS, || {
        verify_independent_projection(
            &initial,
            &history,
            &no_pruned,
            sidecar.current_set(),
            &profiles,
            &domain_registry,
            HistoryReplayLimits::default(),
        )
        .unwrap();
    });

    // 5. Large-world cold build: 4,096-key DenseCube16 WorldRootV1 (COMMIT-1 TypedPatriciaTrie)
    let mut dense_entries = Vec::with_capacity(4096);
    for x in 0..16_i64 {
        for y in 0..16_i64 {
            for z in 0..16_i64 {
                dense_entries.push((
                    CoordinateKeyV1::from_coord(Coord::new(x, y, z))
                        .as_bytes()
                        .to_vec(),
                    ON_V1_VALUE_BYTES.to_vec(),
                ));
            }
        }
    }
    let (dense4096_build_p50_us, dense4096_build_max_us) = p50_max_us(REPEATS, || {
        TypedPatriciaTrie::from_entries(prof_w, &op_limits, dense_entries.clone()).unwrap()
    });
    let dense4096_trie =
        TypedPatriciaTrie::from_entries(prof_w, &op_limits, dense_entries).unwrap();
    let dense4096_nodes = dense4096_trie.collect_node_records().len();

    // 6. Durable filesystem commit + full restart recovery (16 ticks on DurableAuthenticatedLedger)
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base_dir = env::temp_dir().join(format!(
        "bitblock_commit1_cost_{}_{}",
        std::process::id(),
        stamp
    ));
    fs::create_dir_all(&base_dir).unwrap();
    let journal_path = base_dir.join("ledger.bbj");
    let store_dir = base_dir.join("commit_store");

    let (mut durable, _, _) = DurableAuthenticatedLedger::create_new(
        &journal_path,
        &store_dir,
        &profile,
        scope_digest,
        &initial,
    )
    .unwrap();
    let mut durable_tick_us = Vec::new();
    for tick in 1..=16_u64 {
        let c = Coord::new(tick as i64, 0, 0);
        let t0 = Instant::now();
        durable
            .append_tick(tick, &[Edit::new(Operation::On, c)])
            .unwrap();
        durable_tick_us.push(t0.elapsed().as_nanos() as f64 / 1_000.0);
    }
    let durable_persisted_nodes = durable.store().persisted_node_count().unwrap();
    drop(durable);

    let (durable_recover_p50_us, durable_recover_max_us) = p50_max_us(REPEATS, || {
        let (reopened, _, report) = DurableAuthenticatedLedger::open_and_recover(
            &journal_path,
            &store_dir,
            &profile,
            scope_digest,
            AnchorPolicy::UnanchoredLocal,
            &initial,
        )
        .unwrap();
        assert_eq!(reopened.ledger().history().receipts().len(), 16);
        report
    });
    let _ = fs::remove_dir_all(&base_dir);

    durable_tick_us.sort_by(f64::total_cmp);
    let durable_tick_p50_us =
        durable_tick_us[(durable_tick_us.len().div_ceil(2)).saturating_sub(1)];
    let durable_tick_max_us = *durable_tick_us.last().unwrap();

    if json_mode {
        println!("{{");
        println!("  \"repeats\": {REPEATS},");
        println!("  \"trajectory_64_ticks\": {{");
        println!("    \"advance_p50_us\": {advance_p50_us:.2},");
        println!("    \"advance_max_us\": {advance_max_us:.2},");
        println!("    \"world_nodes\": {world_nodes_64},");
        println!("    \"active_token_nodes\": {active_nodes_64},");
        println!("    \"episode_nodes\": {episode_nodes_64},");
        println!("    \"commitment_set_canonical_bytes\": {commitment_set_bytes},");
        println!("    \"mem_nodes_before_gc\": {mem_nodes_before_gc},");
        println!("    \"gc_reclaimed_nodes\": {gc_reclaimed_nodes},");
        println!("    \"gc_remaining_nodes\": {gc_remaining_nodes},");
        println!("    \"world_proof_bytes\": {},", w_proof.len());
        println!("    \"active_token_proof_bytes\": {},", a_proof.len());
        println!("    \"episode_proof_bytes\": {},", e_proof.len());
        println!("    \"verify_world_p50_us\": {verify_world_p50_us:.2},");
        println!("    \"verify_world_max_us\": {verify_world_max_us:.2},");
        println!("    \"verify_active_token_p50_us\": {verify_active_p50_us:.2},");
        println!("    \"verify_episode_p50_us\": {verify_episode_p50_us:.2},");
        println!(
            "    \"powered_one_byte_tighter_cap_rejected\": {powered_one_byte_tighter_rejected},"
        );
        println!("    \"independent_projection_p50_us\": {independent_proj_p50_us:.2},");
        println!("    \"independent_projection_max_us\": {independent_proj_max_us:.2}");
        println!("  }},");
        println!("  \"dense_cube_4096_world_root\": {{");
        println!("    \"occupied_keys\": 4096,");
        println!("    \"unique_cas_nodes\": {dense4096_nodes},");
        println!("    \"cold_build_p50_us\": {dense4096_build_p50_us:.2},");
        println!("    \"cold_build_max_us\": {dense4096_build_max_us:.2}");
        println!("  }},");
        println!("  \"durable_16_ticks\": {{");
        println!("    \"append_tick_fsync_p50_us\": {durable_tick_p50_us:.2},");
        println!("    \"append_tick_fsync_max_us\": {durable_tick_max_us:.2},");
        println!("    \"persisted_node_files\": {durable_persisted_nodes},");
        println!("    \"open_and_recover_p50_us\": {durable_recover_p50_us:.2},");
        println!("    \"open_and_recover_max_us\": {durable_recover_max_us:.2}");
        println!("  }}");
        println!("}}");
        return;
    }

    println!("COMMIT-1 COST-1 probe (repeats={REPEATS}):");
    println!(
        "  64-tick sidecar advance:    p50={advance_p50_us:.1} us, max={advance_max_us:.1} us"
    );
    println!(
        "  64-tick nodes (W/A/E):      {world_nodes_64} / {active_nodes_64} / {episode_nodes_64}, CommitmentSetV1={commitment_set_bytes} B"
    );
    println!(
        "  Memory store GC:            before={mem_nodes_before_gc}, reclaimed={gc_reclaimed_nodes}, remaining={gc_remaining_nodes}"
    );
    println!(
        "  Proof bytes (W/A/E):        {} / {} / {} B (1-byte-tighter control rejected={powered_one_byte_tighter_rejected})",
        w_proof.len(),
        a_proof.len(),
        e_proof.len()
    );
    println!(
        "  Verify p50 (W/A/E):         {verify_world_p50_us:.2} / {verify_active_p50_us:.2} / {verify_episode_p50_us:.2} us"
    );
    println!(
        "  Independent projection:     p50={independent_proj_p50_us:.1} us, max={independent_proj_max_us:.1} us"
    );
    println!(
        "  Dense4096 cold build:       unique_cas_nodes={dense4096_nodes}, p50={dense4096_build_p50_us:.1} us, max={dense4096_build_max_us:.1} us"
    );
    println!(
        "  Durable 16-tick fsync:      p50={durable_tick_p50_us:.1} us, max={durable_tick_max_us:.1} us, node_files={durable_persisted_nodes}"
    );
    println!(
        "  Durable open_and_recover:   p50={durable_recover_p50_us:.1} us, max={durable_recover_max_us:.1} us"
    );
}
