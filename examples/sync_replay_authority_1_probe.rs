//! Release benchmark probe for `SYNC-1@replay-authority` (`PD-06` / `PS-095`) producing
//! merged-head timing metrics for `sync_replay_authority_1_audit_chart.png`:
//! - Event-prefix completeness + normalized authority view reconstruction,
//! - `CONSENSUS-1` -> replay commitment-proof & append-consistency verification,
//! - Full post-consumer checkpoint audit bundle verification (`checkpoint_height 5 > consumer 1`).

use std::time::Instant;

use bitblock::sync_replay_authority_1::{
    build_canonical_replay_authority_fixture, compute_normalized_authority_view_root,
};

fn main() {
    let fix = build_canonical_replay_authority_fixture().expect("fixture");

    // 1. Event-prefix completeness + normalized view reconstruction
    let iters = 100u32;
    let t0 = Instant::now();
    for _ in 0..iters {
        fix.audit_bundle_c1
            .replay_view
            .prefix_proof
            .verify(&fix.checkpoint_c1)
            .expect("prefix");
        let r = compute_normalized_authority_view_root(
            &fix.audit_bundle_c1.replay_view.normalized_entries,
        )
        .expect("norm root");
        std::hint::black_box(r);
    }
    let view_us = t0.elapsed().as_secs_f64() * 1e6 / f64::from(iters);

    // 2. C1 -> C2 append-consistency + checkpoint authorization verification
    let t1 = Instant::now();
    for _ in 0..iters {
        fix.checkpoint_c2
            .verify_authorization(
                &fix.fork_context,
                fix.archive_policy_id,
                fix.norm_policy_id,
                &fix.checkpoint_auth_pk,
            )
            .expect("cp2 auth");
        fix.audit_bundle_c2
            .consistency_proof
            .as_ref()
            .expect("cproof")
            .verify(
                &fix.checkpoint_c1,
                &fix.checkpoint_c2,
                &fix.audit_bundle_c2.replay_view.prefix_proof.records,
            )
            .expect("consistency");
    }
    let commit_us = t1.elapsed().as_secs_f64() * 1e6 / f64::from(iters);

    // 3. Complete post-consumer audit bundle verification
    let t2 = Instant::now();
    for _ in 0..iters {
        fix.audit_bundle_c2
            .verify_audit(
                &fix.fork_context,
                &fix.schedule,
                fix.archive_policy_id,
                fix.norm_policy_id,
                &fix.checkpoint_auth_pk,
                Some(&fix.checkpoint_c1),
            )
            .expect("audit");
    }
    let audit_us = t2.elapsed().as_secs_f64() * 1e6 / f64::from(iters);

    println!("SRA_1_VIEW_US={view_us:.2}");
    println!("SRA_1_COMMIT_US={commit_us:.2}");
    println!("SRA_1_AUDIT_US={audit_us:.2}");
    println!("SRA_1_CP1_TAG={}", fix.checkpoint_c1.tag().expect("tag"));
    println!("SRA_1_B1_TAG={}", fix.audit_bundle_c1.tag().expect("tag"));
}
