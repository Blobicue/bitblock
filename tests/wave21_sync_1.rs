#![cfg(feature = "protocol-research")]
//! Wave 21 (`SYNC-1@base`, `PS-034`) integration tests:
//! 1. Strict 4-role separation (`FullVerifier`, `Archive`, `LightClient`, `NonAuthorityRpc`),
//!    real `127.0.0.1` loopback TCP multi-peer bootstrap, `STORAGE-1` disk crash-restart,
//!    bounded `<= 12 KiB` LightClient inclusion witness verification, and golden parity.
//! 2. Complete `7/7` fault-injection matrix (`F1` eclipse 7/8 & 8/8, `F2` partial-data/withheld-body,
//!    `F3` corrupt-snapshot chunk, `F4` slow-peer/backpressure DoS, `F5` crash-restart recovery,
//!    `F6` `>= 28`-epoch retention pruning + archive refusal, `F7` fresh-node bootstrap & opaque
//!    authority blob non-authority invariant preserving `PS-095:SYNC-1@replay-authority=LOCKED`).

use std::fs::OpenOptions;
use std::io::Write;
use std::net::TcpListener;
use std::thread;

use bitblock::auth_commit_1::{DomainIdV1, DomainPatriciaTrieV1};
use bitblock::identity_d1::ActorIdV1;
use bitblock::journal::AnchorPolicy;
use bitblock::multi_1::{
    CanonicalMultiActorFixtureV1, MULTI_1_VERSION, MultiActorOpKindV1, MultiActorStreamBundleV1,
    SignedActorTxEnvelopeV1,
};
use bitblock::sequencer_1::{DEFAULT_SEQUENCER_SEED, DurableSequencerStoreV1, SequencerEngineV1};
use bitblock::sync_1::{
    ArchiveNodeV1, FullVerifierNodeV1, LightClientNodeV1, NonAuthorityRpcNodeV1,
    OpaqueAuthorityBlobV1, OpaqueBlobKindV1, PeerIdV1, PeerTableV1, SimulatedPeerBehaviorV1,
    Sync1Error, SyncClusterHarnessV1, SyncNodeRoleV1, SyncRecoveryReceiptV1, sync_net,
};
use bitblock::world::{BEDROCK_COORD, Coord};

#[test]
fn wave21_sync_1_four_roles_loopback_tcp_bootstrap_disk_restart_and_golden_parity() {
    let fix = CanonicalMultiActorFixtureV1::build().unwrap();
    let mut engine = SequencerEngineV1::from_fixture(&fix, DEFAULT_SEQUENCER_SEED).unwrap();
    let bounds = engine.bounds();
    let bootstrap = fix.bundle.bootstrap_policy;

    let wr1 = engine.current_world_root().unwrap();
    let b1 = fix.canonical_mixed_race_bundle(1, wr1).unwrap();
    let pub1 = engine.seal_bundle_and_publish(&b1).unwrap();

    // Populate Role 2 (Archive) with publication + bounded opaque authority blobs
    let mut archive = ArchiveNodeV1::new(bootstrap);
    archive.store_publication(pub1.clone());
    let cert_blob = OpaqueAuthorityBlobV1::new(
        OpaqueBlobKindV1::OpaqueCertificate,
        b"opaque-cert-carrier-v1".to_vec(),
    )
    .unwrap();
    let occ_blob = OpaqueAuthorityBlobV1::new(
        OpaqueBlobKindV1::OpaqueOccurrence,
        b"opaque-occ-carrier-v1".to_vec(),
    )
    .unwrap();
    archive.store_opaque_blob(cert_blob.clone()).unwrap();
    archive.store_opaque_blob(occ_blob.clone()).unwrap();

    // Real 127.0.0.1 loopback TCP multi-peer bootstrap from Archive to fresh FullVerifier
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let srv_archive = archive.clone();
    let srv = thread::spawn(move || {
        sync_net::serve_archive_bootstrap_once(&listener, &srv_archive, 1, 1_024, &bounds)
    });
    let fetched = sync_net::fetch_archive_bootstrap_loopback(addr, &bounds).unwrap();
    srv.join().unwrap().unwrap();
    assert_eq!(fetched.publications, vec![pub1.clone()]);
    assert_eq!(
        fetched.manifest.tag().unwrap().to_string(),
        "#bb.receipt.sha256.m267jyqvgn3t7iqja3cu4jnb3qxnlpqjxr3lrlh4dq77nnura6qa"
    );
    assert_eq!(fetched.opaque_blobs.len(), 2);
    for b in &fetched.opaque_blobs {
        assert!(
            !b.interpreted_as_live_authority(),
            "opaque blobs must never be admitted as live authority in SYNC-1@base"
        );
    }

    // Boot fresh Role 1 (FullVerifier) from loopback TCP bundle
    let mut fv = FullVerifierNodeV1::from_fixture(&fix, engine.sequencer_pubkey()).unwrap();
    for p in &fetched.publications {
        fv.apply_publication(p).unwrap();
    }
    for b in fetched.opaque_blobs {
        fv.retain_opaque_blob(b).unwrap();
    }
    assert_eq!(fv.verified_height(), 1);

    // Role 3 (LightClient): verifies header + <= 12 KiB ReceiptMerkleProof + DomainPatriciaProof
    let mut light = LightClientNodeV1::new(engine.sequencer_pubkey(), bounds);
    light.track_signed_header(&pub1.header).unwrap();
    assert_eq!(light.stored_body_count(), 0);
    assert_eq!(light.tip_height(), 1);
    let proof0 = pub1.build_receipt_proof(0).unwrap();
    light
        .verify_receipt_witness(1, &proof0, &pub1.fold_receipt.step_outcomes[0])
        .unwrap();
    let mut trie = DomainPatriciaTrieV1::empty(DomainIdV1::NativeBalance);
    trie.upsert(
        *fix.actor_a.as_bytes(),
        99_999_000_000u128.to_be_bytes().to_vec(),
    );
    let pat_proof = trie.prove(fix.actor_a.as_bytes()).unwrap();
    light
        .verify_patricia_witness(&pat_proof, trie.root(), &99_999_000_000u128.to_be_bytes())
        .unwrap();

    // Role 4 (NonAuthorityRpc): read-only, rate-limited (<= 64 req/s, <= 4 MiB inflight)
    let mut rpc = NonAuthorityRpcNodeV1::new(bounds, bootstrap, vec![pub1.header]);
    assert!(!rpc.can_mutate_ledger());
    assert!(!rpc.can_admit_live_authority());
    let caller = PeerIdV1([99u8; 32]);
    rpc.register_client(caller);
    assert_eq!(rpc.query_header(caller, 1, 128).unwrap(), Some(pub1.header));
    // Exceed 4 MiB inflight -> throttled fail-closed!
    assert!(matches!(
        rpc.query_header(caller, 1, u64::from(bounds.max_peer_inflight_bytes) + 1),
        Err(Sync1Error::PeerBackpressureExceeded { .. })
    ));

    // F5: STORAGE-1 disk crash-restart recovery for Archive/FullVerifier
    let tmp = std::env::temp_dir().join(format!(
        "bitblock_wave21_sync1_{}_{}.journal",
        std::process::id(),
        pub1.header.header_digest().unwrap()[0]
    ));
    let _ = std::fs::remove_file(&tmp);
    let anchor = {
        let mut store = DurableSequencerStoreV1::create_new(&tmp).unwrap();
        let (_, h_rcpt) = store.append_publication_synced(&pub1).unwrap();
        h_rcpt.record_tag
    };
    {
        let mut f = OpenOptions::new().append(true).open(&tmp).unwrap();
        f.write_all(b"BBSTRREC\x00\x01\xca\xfe").unwrap();
        f.sync_all().unwrap();
    }
    let (_, rec_pubs, discard) = DurableSequencerStoreV1::open_and_recover(
        &tmp,
        AnchorPolicy::RequireAncestor(anchor),
        &engine.sequencer_pubkey(),
        &bounds,
    )
    .unwrap();
    let _ = std::fs::remove_file(&tmp);
    assert!(discard.is_some());
    assert_eq!(rec_pubs, vec![pub1]);
}

#[test]
fn wave21_sync_1_seven_fault_classes_eclipse_pruning_and_non_authority_guards() {
    let fix = CanonicalMultiActorFixtureV1::build().unwrap();
    let mut engine = SequencerEngineV1::from_fixture(&fix, DEFAULT_SEQUENCER_SEED).unwrap();
    let bounds = engine.bounds();
    let bootstrap = fix.bundle.bootstrap_policy;

    // Build 3 batches spanning epoch 0 -> epoch 30 to exercise >= 28-epoch retention pruning (F6)
    let wr1 = engine.current_world_root().unwrap();
    let b1 = fix.canonical_mixed_race_bundle(1, wr1).unwrap();
    let pub1 = engine.seal_bundle_and_publish(&b1).unwrap();

    let wr2 = engine.current_world_root().unwrap();
    let zero_a = ActorIdV1::from_bytes([0u8; 32]);
    let tx2 = SignedActorTxEnvelopeV1 {
        envelope_version: MULTI_1_VERSION,
        chain_id: fix.fork_context.chain_id,
        principal_actor_id: fix.actor_b,
        signer_actor_id: fix.actor_b,
        nonce: 0,
        priority_tip: 100,
        expected_world_root: wr2,
        op_kind: MultiActorOpKindV1::PlaceVoxel,
        coord: Coord::new(8, 0, 0),
        target_actor_id: zero_a,
        amount: 0,
        signer_pubkey: [0u8; 32],
        signature: [0u8; 64],
    }
    .sign_with_seed(
        &CanonicalMultiActorFixtureV1::ACTOR_B_SEED,
        &fix.fork_context,
    )
    .unwrap();
    let pub2 = engine
        .seal_bundle_and_publish(&MultiActorStreamBundleV1 {
            bundle_version: MULTI_1_VERSION,
            batch_height: 2,
            epoch: 10,
            fork_context: fix.fork_context,
            base_fee_per_gas: 2,
            envelopes: vec![tx2],
        })
        .unwrap();

    let wr3 = engine.current_world_root().unwrap();
    let tx3 = SignedActorTxEnvelopeV1 {
        envelope_version: MULTI_1_VERSION,
        chain_id: fix.fork_context.chain_id,
        principal_actor_id: fix.actor_b,
        signer_actor_id: fix.actor_b,
        nonce: 1,
        priority_tip: 100,
        expected_world_root: wr3,
        op_kind: MultiActorOpKindV1::TransferNative,
        coord: BEDROCK_COORD,
        target_actor_id: fix.actor_a,
        amount: 2_000,
        signer_pubkey: [0u8; 32],
        signature: [0u8; 64],
    }
    .sign_with_seed(
        &CanonicalMultiActorFixtureV1::ACTOR_B_SEED,
        &fix.fork_context,
    )
    .unwrap();
    let pub3 = engine
        .seal_bundle_and_publish(&MultiActorStreamBundleV1 {
            bundle_version: MULTI_1_VERSION,
            batch_height: 3,
            epoch: 30,
            fork_context: fix.fork_context,
            base_fee_per_gas: 2,
            envelopes: vec![tx3],
        })
        .unwrap();

    // Seed diversity gate: < 8 seed peers rejected fail-closed
    let mut small_pt = PeerTableV1::new(bounds, bootstrap);
    for i in 1..=7u8 {
        small_pt.register_peer(PeerIdV1([i; 32]), SyncNodeRoleV1::Archive);
    }
    assert!(matches!(
        small_pt.verify_seed_diversity(),
        Err(Sync1Error::InsufficientPeerDiversity {
            observed: 7,
            required: 8
        })
    ));

    let mut archive = ArchiveNodeV1::new(bootstrap);
    archive.store_publication(pub1.clone());
    archive.store_publication(pub2);
    archive.store_publication(pub3);
    archive
        .store_opaque_blob(
            OpaqueAuthorityBlobV1::new(
                OpaqueBlobKindV1::OpaqueCertificate,
                b"opaque-cert-carrier-v1".to_vec(),
            )
            .unwrap(),
        )
        .unwrap();

    // F1 (7/8 eclipse) + F2 (withheld body) + F3 (corrupt chunk) + F4 (slow-peer DoS) + F7 (fresh bootstrap)
    let mixed_8 = [
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
    let mut cluster =
        SyncClusterHarnessV1::new(bounds, bootstrap, archive.clone(), 3, 1_024, &mixed_8).unwrap();
    let (mut fv, rcpt) = cluster
        .sync_fresh_full_verifier(&fix, engine.sequencer_pubkey())
        .unwrap();
    assert_eq!(rcpt.peers_discovered, 8);
    assert_eq!(rcpt.peers_banned, 7);
    assert_eq!(rcpt.honest_peers_used, 1);
    assert_eq!(rcpt.synced_height, 3);
    assert!(!rcpt.live_authority_admitted);

    // F1 (8/8 total eclipse): fails closed with zero corrupted state
    let total_eclipse_8 = [
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
            SimulatedPeerBehaviorV1::StallTimeout,
        ),
    ];
    let mut eclipse_cluster = SyncClusterHarnessV1::new(
        bounds,
        bootstrap,
        archive.clone(),
        3,
        1_024,
        &total_eclipse_8,
    )
    .unwrap();
    assert!(matches!(
        eclipse_cluster.sync_fresh_full_verifier(&fix, engine.sequencer_pubkey()),
        Err(Sync1Error::TotalEclipseNoHonestPeers)
    ));

    // F6: >= 28-epoch retention pruning on FullVerifier (epoch 0 < 30 - 28 = 2 -> height 1 pruned,
    // epoch 10 & 30 retained!), while Archive structurally refuses pruning!
    assert_eq!(fv.prune_below_retention_window(30).unwrap(), 1);
    assert_eq!(fv.pruned_through_height(), 1);
    assert_eq!(fv.retained_count(), 2);
    assert!(matches!(
        fv.attempt_force_prune_height(2),
        Err(Sync1Error::UnsafePruneForbidden(_))
    ));
    assert!(matches!(
        fv.attempt_force_prune_height(3),
        Err(Sync1Error::UnsafePruneForbidden(_))
    ));
    assert_eq!(
        archive.attempt_prune(1),
        Err(Sync1Error::ArchivePruningForbidden)
    );

    // Non-authority guard: live_authority_admitted = true is rejected on encode & decode!
    let mut bad_rcpt = rcpt;
    bad_rcpt.live_authority_admitted = true;
    assert_eq!(
        bad_rcpt.encode_canonical(),
        Err(Sync1Error::LiveAuthorityAdmissionForbiddenInBasePhase)
    );
    let mut bad_raw = rcpt.encode_canonical().unwrap();
    bad_raw[SyncRecoveryReceiptV1::WIRE_LEN - 1] = 1;
    assert_eq!(
        SyncRecoveryReceiptV1::decode_canonical(&bad_raw),
        Err(Sync1Error::LiveAuthorityAdmissionForbiddenInBasePhase)
    );
}
