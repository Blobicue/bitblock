//! Release benchmark probe for Wave 21 (`SYNC-1@base`, `PS-034`).
//! Runs the 8-peer 7/8-eclipse recovery, LightClient witness verification, and checksummed
//! checkpoint reassembly benchmarks, emitting golden tags and 50-run p50 latencies.

use std::time::Instant;

use bitblock::multi_1::CanonicalMultiActorFixtureV1;
use bitblock::sequencer_1::{DEFAULT_SEQUENCER_SEED, SequencerEngineV1};
use bitblock::sync_1::{
    ArchiveNodeV1, LightClientNodeV1, OpaqueAuthorityBlobV1, OpaqueBlobKindV1, PeerIdV1,
    SimulatedPeerBehaviorV1, SyncClusterHarnessV1, SyncNodeRoleV1, compute_opaque_blobs_set_digest,
};

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
    let mut engine = SequencerEngineV1::from_fixture(&fix, DEFAULT_SEQUENCER_SEED).unwrap();
    let bounds = engine.bounds();
    let bootstrap = fix.bundle.bootstrap_policy;
    let pre_wr = engine.current_world_root().unwrap();
    let b1 = fix.canonical_mixed_race_bundle(1, pre_wr).unwrap();
    let pub1 = engine.seal_bundle_and_publish(&b1).unwrap();

    let mut archive = ArchiveNodeV1::new(bootstrap);
    archive.store_publication(pub1.clone());
    archive
        .store_opaque_blob(
            OpaqueAuthorityBlobV1::new(
                OpaqueBlobKindV1::OpaqueCertificate,
                b"opaque-cert-carrier-v1".to_vec(),
            )
            .unwrap(),
        )
        .unwrap();
    archive
        .store_opaque_blob(
            OpaqueAuthorityBlobV1::new(
                OpaqueBlobKindV1::OpaqueOccurrence,
                b"opaque-occ-carrier-v1".to_vec(),
            )
            .unwrap(),
        )
        .unwrap();

    let behaviors = [
        (
            PeerIdV1([1u8; 32]),
            SyncNodeRoleV1::Archive,
            SimulatedPeerBehaviorV1::WithholdBody,
        ),
        (
            PeerIdV1([2u8; 32]),
            SyncNodeRoleV1::Archive,
            SimulatedPeerBehaviorV1::CorruptCheckpointChunk,
        ),
        (
            PeerIdV1([3u8; 32]),
            SyncNodeRoleV1::Archive,
            SimulatedPeerBehaviorV1::BadSignature,
        ),
        (
            PeerIdV1([4u8; 32]),
            SyncNodeRoleV1::Archive,
            SimulatedPeerBehaviorV1::SlowBackpressureDoS,
        ),
        (
            PeerIdV1([5u8; 32]),
            SyncNodeRoleV1::FullVerifier,
            SimulatedPeerBehaviorV1::StallTimeout,
        ),
        (
            PeerIdV1([6u8; 32]),
            SyncNodeRoleV1::FullVerifier,
            SimulatedPeerBehaviorV1::WithholdBody,
        ),
        (
            PeerIdV1([7u8; 32]),
            SyncNodeRoleV1::FullVerifier,
            SimulatedPeerBehaviorV1::CorruptCheckpointChunk,
        ),
        (
            PeerIdV1([8u8; 32]),
            SyncNodeRoleV1::Archive,
            SimulatedPeerBehaviorV1::Honest,
        ),
    ];
    let base_cluster =
        SyncClusterHarnessV1::new(bounds, bootstrap, archive.clone(), 1, 1_024, &behaviors)
            .unwrap();
    let proof0 = pub1.build_receipt_proof(0).unwrap();

    let mut full_times = Vec::with_capacity(50);
    let mut light_times = Vec::with_capacity(50);
    let mut chk_times = Vec::with_capacity(50);
    let mut sample_rcpt = None;

    for _ in 0..50 {
        let mut c = base_cluster.clone();
        let t0 = Instant::now();
        let (_node, rcpt) = c
            .sync_fresh_full_verifier(&fix, engine.sequencer_pubkey())
            .unwrap();
        full_times.push(t0.elapsed().as_secs_f64() * 1e6);

        let mut lc = LightClientNodeV1::new(engine.sequencer_pubkey(), bounds);
        let t1 = Instant::now();
        lc.track_signed_header(&pub1.header).unwrap();
        lc.verify_receipt_witness(1, &proof0, &pub1.fold_receipt.step_outcomes[0])
            .unwrap();
        light_times.push(t1.elapsed().as_secs_f64() * 1e6);

        let t2 = Instant::now();
        let _ = base_cluster
            .checkpoint_manifest
            .verify_and_reassemble(&base_cluster.checkpoint_chunks)
            .unwrap();
        chk_times.push(t2.elapsed().as_secs_f64() * 1e6);

        sample_rcpt = Some(rcpt);
    }

    let opq_dig = compute_opaque_blobs_set_digest(&archive.opaque_blobs());
    println!(
        "CHK_TAG={}",
        base_cluster.checkpoint_manifest.tag().unwrap()
    );
    println!("REC_TAG={}", sample_rcpt.unwrap().tag().unwrap());
    println!("OPQ_SHA={}", hex_lower(&opq_dig));
    println!("FULL_P50_US={:.2}", median(&mut full_times));
    println!("LIGHT_P50_US={:.2}", median(&mut light_times));
    println!("CHK_P50_US={:.2}", median(&mut chk_times));
}
