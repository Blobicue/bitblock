//! Release benchmark probe for Wave 20 (`SEQUENCER-1`, `PS-033`).
//! Emits golden tags, SHA-256 digests, and 50-run p50 timings for publication, independent
//! client replay, and receipt Merkle proof verification.

use std::time::Instant;

use bitblock::multi_1::CanonicalMultiActorFixtureV1;
use bitblock::sequencer_1::{
    DEFAULT_SEQUENCER_SEED, SEQUENCER_1_VERSION, SequencerClientVerifierV1, SequencerEngineV1,
    SequencerWireFrameV1, SequencerWireMsgKindV1,
};
use sha2::{Digest, Sha256};

fn median(vals: &mut [f64]) -> f64 {
    vals.sort_by(|a, b| a.total_cmp(b));
    vals[vals.len() / 2]
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(&mut s, "{b:02x}");
    }
    s
}

fn main() {
    let fix = CanonicalMultiActorFixtureV1::build().unwrap();
    let base_engine = SequencerEngineV1::from_fixture(&fix, DEFAULT_SEQUENCER_SEED).unwrap();
    let bounds = base_engine.bounds();
    let pre_wr = base_engine.current_world_root().unwrap();
    let bundle = fix.canonical_mixed_race_bundle(1, pre_wr).unwrap();

    let mut pub_times = Vec::with_capacity(50);
    let mut replay_times = Vec::with_capacity(50);
    let mut merkle_times = Vec::with_capacity(50);

    let mut sample_pub = None;
    for _ in 0..50 {
        let mut eng = base_engine.clone();
        let t0 = Instant::now();
        let p = eng.seal_bundle_and_publish(&bundle).unwrap();
        pub_times.push(t0.elapsed().as_secs_f64() * 1e6);

        let mut cli =
            SequencerClientVerifierV1::from_fixture(&fix, eng.sequencer_pubkey()).unwrap();
        let t1 = Instant::now();
        let _ = cli.verify_and_apply_publication(&p).unwrap();
        replay_times.push(t1.elapsed().as_secs_f64() * 1e6);

        let proof0 = p.build_receipt_proof(0).unwrap();
        let t2 = Instant::now();
        cli.verify_receipt_inclusion(1, &proof0, &p.fold_receipt.step_outcomes[0])
            .unwrap();
        merkle_times.push(t2.elapsed().as_secs_f64() * 1e6);

        sample_pub = Some(p);
    }

    let p = sample_pub.unwrap();
    let proof0_raw = p
        .build_receipt_proof(0)
        .unwrap()
        .encode_canonical()
        .unwrap();
    let frame_raw = SequencerWireFrameV1 {
        version: SEQUENCER_1_VERSION,
        msg_kind: SequencerWireMsgKindV1::BatchPublication,
        payload: p.encode_canonical().unwrap(),
    }
    .encode_canonical(&bounds)
    .unwrap();

    println!("HDR_TAG={}", p.header.tag().unwrap());
    println!("PUB_TAG={}", p.tag().unwrap());
    println!("PROOF0_SHA={}", hex_lower(&Sha256::digest(&proof0_raw)));
    println!("FRAME_SHA={}", hex_lower(&Sha256::digest(&frame_raw)));
    println!("PUB_P50_US={:.2}", median(&mut pub_times));
    println!("REPLAY_P50_US={:.2}", median(&mut replay_times));
    println!("MERKLE_P50_US={:.2}", median(&mut merkle_times));
}
