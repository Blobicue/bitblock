//! Release benchmark & audit probe for Wave 31 (`UPGRADE-1@base`, `PS-083`).
//!
//! Run on merged `origin/main` with:
//! `cargo run --release --features protocol-research --example upgrade_1_migration_probe`

use std::time::Instant;

use bitblock::upgrade_1::{
    MigrationFaultHookV1, build_canonical_devnet_upgrade_1, execute_bounded_profile_migration,
    verify_upgrade_1_evidence_manifest,
};

fn median_us(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(|a, b| a.total_cmp(b));
    samples[samples.len() / 2]
}

fn main() {
    let out = build_canonical_devnet_upgrade_1().expect("canonical UPGRADE-1");
    let fx = &out.launch_fixture;
    let succ = &out.successor_candidate;
    let total_supply = out.replay_receipt.conserved_total_native_supply;
    let total_stake = out.replay_receipt.conserved_bonded_stake;

    // 1. Full deterministic bounded migration latency (median of 300 runs).
    let mut mig_samples = Vec::with_capacity(300);
    for _ in 0..300 {
        let mut snap = out.old_snapshot;
        let t0 = Instant::now();
        let res = execute_bounded_profile_migration(
            &out.migration_spec,
            &out.occurrence_handoff,
            &fx.bound_manifest,
            &succ.successor_bound_manifest,
            &fx.exec_registry,
            &fx.bundle.initial_gov_params,
            out.launch_parity_record.initial_execution_root_ref,
            &mut snap,
            total_supply,
            total_stake,
            21,
            true,
            MigrationFaultHookV1::None,
        )
        .expect("migrate");
        std::hint::black_box(res);
        mig_samples.push(t0.elapsed().as_secs_f64() * 1e6);
    }

    // 2. Evidence manifest verification latency (median of 500 runs).
    let mut ver_samples = Vec::with_capacity(500);
    for _ in 0..500 {
        let t0 = Instant::now();
        let b = verify_upgrade_1_evidence_manifest(
            &out.migration_spec,
            &out.occurrence_handoff,
            &out.replay_receipt,
            &out.evidence_manifest,
        )
        .expect("verify");
        std::hint::black_box(b);
        ver_samples.push(t0.elapsed().as_secs_f64() * 1e6);
    }

    // 3. Atomic mid-migration interruption rollback latency (median of 300 runs).
    let mut rb_samples = Vec::with_capacity(300);
    for _ in 0..300 {
        let mut snap = out.old_snapshot;
        let t0 = Instant::now();
        let err = execute_bounded_profile_migration(
            &out.migration_spec,
            &out.occurrence_handoff,
            &fx.bound_manifest,
            &succ.successor_bound_manifest,
            &fx.exec_registry,
            &fx.bundle.initial_gov_params,
            out.launch_parity_record.initial_execution_root_ref,
            &mut snap,
            total_supply,
            total_stake,
            21,
            true,
            MigrationFaultHookV1::InterruptMidApply,
        )
        .unwrap_err();
        std::hint::black_box(err);
        rb_samples.push(t0.elapsed().as_secs_f64() * 1e6);
    }

    println!("UPG1_MIGRATE_US={:.2}", median_us(mig_samples));
    println!("UPG1_VERIFY_US={:.2}", median_us(ver_samples));
    println!("UPG1_ROLLBACK_US={:.2}", median_us(rb_samples));
    println!("UPG1_SPEC_TAG={}", out.migration_spec.tag().unwrap());
    println!("UPG1_HANDOFF_TAG={}", out.occurrence_handoff.tag().unwrap());
    println!("UPG1_REPLAY_TAG={}", out.replay_receipt.tag().unwrap());
    println!("UPG1_MANIFEST_TAG={}", out.evidence_manifest.tag().unwrap());
}
