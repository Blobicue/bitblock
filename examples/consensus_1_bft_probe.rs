//! Release benchmark probe for `CONSENSUS-1@base` (`PS-056`) producing merged-head timing metrics
//! for `consensus_1_bft_safety_chart.png`:
//! - 4-validator 2-phase HotStuff finality round (`Proposal -> PrepareQC -> CommitQC`),
//! - `ConsensusCommitCertificateV1` verification (`CommitQC` + `PrepareQC` + `Proposal`),
//! - Live `AcceptedConsensusNativeParentAuthorityCertificateV1` issuance & registry admission.

use std::time::Instant;

use bitblock::consensus_1::{ConsensusClusterHarnessV1, ParentAuthorityPurposeV1};

fn main() {
    let base_cluster = ConsensusClusterHarnessV1::bootstrap_four_validators().expect("bootstrap");
    let pre_wr = base_cluster.nodes[0]
        .harness()
        .snapshot()
        .expect("snap")
        .world_root;
    let bundle = base_cluster
        .fixture
        .canonical_mixed_race_bundle(1, pre_wr)
        .expect("bundle");

    // Warm up
    let mut warm = base_cluster.clone();
    let warm_cert = warm
        .run_round_with_subset(&bundle, 0, &[0, 1, 2, 3])
        .expect("warm round");
    let (warm_pa, _) = warm
        .build_and_admit_canonical_live_parent_authority_cert(
            &warm_cert,
            ParentAuthorityPurposeV1::ReleaseAuthority,
            1,
            2,
        )
        .expect("warm cert");

    // 1. 4-validator 2-phase finality round (Proposal + 4x Data-Before-Vote + PrepareQC + CommitQC)
    let round_iters = 20u32;
    let t0 = Instant::now();
    for _ in 0..round_iters {
        let mut c = base_cluster.clone();
        let cc = c
            .run_round_with_subset(&bundle, 0, &[0, 1, 2, 3])
            .expect("round");
        std::hint::black_box(cc);
    }
    let round_us = t0.elapsed().as_secs_f64() * 1e6 / f64::from(round_iters);

    // 2. ConsensusCommitCertificateV1 verification
    let sched0 = base_cluster
        .schedule_state
        .schedule_at(0)
        .expect("ep0")
        .clone();
    let qc_iters = 100u32;
    let t1 = Instant::now();
    for _ in 0..qc_iters {
        warm_cert.verify(&sched0).expect("verify");
    }
    let qc_us = t1.elapsed().as_secs_f64() * 1e6 / f64::from(qc_iters);

    // 3. Live AcceptedConsensusNativeParentAuthorityCertificateV1 verification + admission
    let cert_iters = 100u32;
    let t2 = Instant::now();
    for _ in 0..cert_iters {
        warm_pa
            .verify_live_admission(&base_cluster.fixture.fork_context, &sched0)
            .expect("verify cert");
    }
    let cert_us = t2.elapsed().as_secs_f64() * 1e6 / f64::from(cert_iters);

    println!("CONSENSUS_1_ROUND_US={round_us:.2}");
    println!("CONSENSUS_1_QC_VERIFY_US={qc_us:.2}");
    println!("CONSENSUS_1_CERT_ADMIT_US={cert_us:.2}");
    println!("CONSENSUS_1_COMMIT_TAG={}", warm_cert.tag().expect("tag"));
    println!(
        "CONSENSUS_1_PARENT_AUTH_TAG={}",
        warm_pa.tag().expect("tag")
    );
}
