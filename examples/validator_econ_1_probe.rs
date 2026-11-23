use std::time::Instant;

use bitblock::consensus_1::{ConsensusClusterHarnessV1, VoteEquivocationEvidenceV1};
use bitblock::identity_d1::ActorIdV1;
use bitblock::validator_econ_1::{
    ValidatorEconAdapterV1, run_canonical_validator_econ_adversarial_assay,
    sign_consensus_vote_for_sim,
};

fn p50_us(samples: &mut [f64]) -> f64 {
    samples.sort_by(|a, b| a.total_cmp(b));
    samples[samples.len() / 2]
}

fn main() {
    let mut cluster = ConsensusClusterHarnessV1::bootstrap_four_validators().unwrap();
    let pre_wr = cluster.nodes[0].harness().snapshot().unwrap().world_root;
    let bundle = cluster
        .fixture
        .canonical_mixed_race_bundle(1, pre_wr)
        .unwrap();
    let cert = cluster
        .run_round_with_subset(&bundle, 0, &[0, 1, 2, 3])
        .unwrap();
    let sched0 = cluster.nodes[0].schedule_at(0).unwrap().clone();
    let adapter = ValidatorEconAdapterV1::new(
        cluster.fixture.fork_context,
        cluster.fixture.bundle.validator_econ_policy,
    )
    .unwrap();

    // Benchmark 1: consensus-to-economic weight audit + epoch reward correspondence
    let mut rew_samples = Vec::with_capacity(21);
    let mut adapter_tag = String::new();
    for _ in 0..21 {
        let mut econ = cluster.economic_state.clone();
        let t0 = Instant::now();
        let _audit = adapter
            .audit_consensus_schedule_weights(&cert, &sched0, &econ)
            .unwrap();
        let (rew_rcpt, _) = adapter
            .apply_epoch_rewards_for_finalized_block(&cert, &sched0, &mut econ, 999_999_999, 1)
            .unwrap();
        rew_samples.push(t0.elapsed().as_secs_f64() * 1e6);
        adapter_tag = rew_rcpt.tag().unwrap().to_string();
    }

    // Benchmark 2: verified VoteEquivocationEvidenceV1 -> 50% active+unbonding slash correspondence
    let v1 = ActorIdV1::from_bytes([1u8; 32]);
    let vote_a = sign_consensus_vote_for_sim(
        &cluster.fixture.fork_context,
        &sched0,
        &cert,
        1,
        2,
        [0x11; 32],
    )
    .unwrap();
    let vote_b = sign_consensus_vote_for_sim(
        &cluster.fixture.fork_context,
        &sched0,
        &cert,
        1,
        2,
        [0x22; 32],
    )
    .unwrap();
    let ev = VoteEquivocationEvidenceV1 {
        voter_id: v1,
        first_vote: vote_a,
        second_vote: vote_b,
    };
    let mut slash_samples = Vec::with_capacity(21);
    for _ in 0..21 {
        let mut econ = cluster.economic_state.clone();
        econ.request_unbond(v1, 20_000_000_000, 1).unwrap();
        let t0 = Instant::now();
        let _ = adapter
            .apply_verified_equivocation_slash(&cert, &sched0, &ev, &mut econ, 1)
            .unwrap();
        slash_samples.push(t0.elapsed().as_secs_f64() * 1e6);
    }

    // Benchmark 3: complete 7-vector deterministic adversarial incentive assay
    let mut assay_samples = Vec::with_capacity(15);
    let mut assay_tag = String::new();
    for _ in 0..15 {
        let t0 = Instant::now();
        let (assay, _) = run_canonical_validator_econ_adversarial_assay(&cluster, &cert).unwrap();
        assay_samples.push(t0.elapsed().as_secs_f64() * 1e6);
        assay_tag = assay.tag().unwrap().to_string();
    }

    println!("VECON_REW_US={:.2}", p50_us(&mut rew_samples));
    println!("VECON_SLASH_US={:.2}", p50_us(&mut slash_samples));
    println!("VECON_ASSAY_US={:.2}", p50_us(&mut assay_samples));
    println!("VECON_ADAPTER_TAG={adapter_tag}");
    println!("VECON_ASSAY_TAG={assay_tag}");
}
