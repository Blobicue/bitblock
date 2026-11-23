#![cfg(feature = "protocol-research")]
//! Wave 20 (`SEQUENCER-1`, `PS-033`) integration tests:
//! 1. Multi-batch durable `STORAGE-1` filesystem `Journal` publication with torn-tail crash
//!    injection + recovery + real `127.0.0.1` loopback TCP client resynchronization + independent
//!    executor replay parity + `ReceiptMerkleProofV1` inclusion + golden tag assertions.
//! 2. Complete `8/8` malicious-input & accountability gate matrix (over-limit `> 2 MiB` preflight
//!    rejection, malformed/truncated wire framing, duplicate submission deduplication, concurrent
//!    multi-actor race, withheld-body header refusal, tampered receipt Merkle proof refusal,
//!    tampered execution root refusal, and same-height sequencer equivocation detection).

use std::fs::OpenOptions;
use std::io::Write;
use std::net::TcpListener;
use std::thread;

use bitblock::identity_d1::ActorIdV1;
use bitblock::journal::AnchorPolicy;
use bitblock::multi_1::{
    CanonicalMultiActorFixtureV1, MULTI_1_VERSION, MultiActorOpKindV1, SignedActorTxEnvelopeV1,
};
use bitblock::sequencer_1::{
    DEFAULT_SEQUENCER_SEED, DurableSequencerStoreV1, SEQ_WIRE_HEADER_LEN, SEQ_WIRE_MAGIC,
    SEQUENCER_1_VERSION, Sequencer1Error, SequencerClientVerifierV1, SequencerEngineV1,
    SequencerWireFrameV1, SequencerWireMsgKindV1, net,
};
use bitblock::world::{BEDROCK_COORD, Coord};

#[test]
fn wave20_sequencer_1_durable_journal_recovery_loopback_tcp_and_replay_parity() {
    let fix = CanonicalMultiActorFixtureV1::build().unwrap();
    let mut engine = SequencerEngineV1::from_fixture(&fix, DEFAULT_SEQUENCER_SEED).unwrap();
    let bounds = engine.bounds();

    // Batch 1: canonical mixed race bundle
    let wr1 = engine.current_world_root().unwrap();
    let b1 = fix.canonical_mixed_race_bundle(1, wr1).unwrap();
    let pub1 = engine.seal_bundle_and_publish(&b1).unwrap();
    assert_eq!(
        pub1.header.tag().unwrap().to_string(),
        "#bb.receipt.sha256.zq4zzmtlddpcvphk24t3koybltetooeb5qo3b6yvjhbncrgdsyza"
    );
    assert_eq!(
        pub1.tag().unwrap().to_string(),
        "#bb.receipt.sha256.htcbximoqnut4khf6do3lev5r4znvuvd2ppjgaeicgjiml254eia"
    );

    // Batch 2: remote envelope submissions via bounded wire frames (including duplicate submission)
    let wr2 = engine.current_world_root().unwrap();
    let zero_a = ActorIdV1::from_bytes([0u8; 32]);
    let tx_place = SignedActorTxEnvelopeV1 {
        envelope_version: MULTI_1_VERSION,
        chain_id: fix.fork_context.chain_id,
        principal_actor_id: fix.actor_b,
        signer_actor_id: fix.actor_b,
        nonce: 0,
        priority_tip: 350,
        expected_world_root: wr2,
        op_kind: MultiActorOpKindV1::PlaceVoxel,
        coord: Coord::new(7, 0, 0),
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
    let submit_frame = SequencerWireFrameV1 {
        version: SEQUENCER_1_VERSION,
        msg_kind: SequencerWireMsgKindV1::SubmitEnvelope,
        payload: tx_place.encode_canonical().unwrap().to_vec(),
    };
    assert!(engine.admit_wire_envelope(&submit_frame).unwrap());
    assert!(
        !engine.admit_wire_envelope(&submit_frame).unwrap(),
        "duplicate envelope must be rejected cleanly"
    );
    let pub2 = engine.seal_mempool_and_publish(0, 2).unwrap();
    assert_eq!(pub2.header.batch_height, 2);
    assert_eq!(
        pub2.header.parent_header_digest,
        pub1.header.header_digest().unwrap()
    );

    // Durable STORAGE-1 filesystem Journal sync + simulated crash torn tail + recovery
    let tmp_path = std::env::temp_dir().join(format!(
        "bitblock_wave20_seq1_{}_{}.journal",
        std::process::id(),
        pub1.header.header_digest().unwrap()[0]
    ));
    let _ = std::fs::remove_file(&tmp_path);
    let anchor_tag = {
        let mut store = DurableSequencerStoreV1::create_new(&tmp_path).unwrap();
        let (_b1_rcpt, h1_rcpt) = store.append_publication_synced(&pub1).unwrap();
        let (_b2_rcpt, _h2_rcpt) = store.append_publication_synced(&pub2).unwrap();
        assert!(store.valid_bytes() > 0);
        h1_rcpt.record_tag
    };

    // Inject torn-tail garbage bytes simulating crash mid-write on batch 3
    {
        let mut f = OpenOptions::new().append(true).open(&tmp_path).unwrap();
        f.write_all(b"BBSTRREC\x00\x01\xde\xad\xbe\xef").unwrap();
        f.sync_all().unwrap();
    }

    let (_recovered_store, recovered_pubs, discard_rcpt) =
        DurableSequencerStoreV1::open_and_recover(
            &tmp_path,
            AnchorPolicy::RequireAncestor(anchor_tag),
            &engine.sequencer_pubkey(),
            &bounds,
        )
        .unwrap();
    let _ = std::fs::remove_file(&tmp_path);
    assert!(
        discard_rcpt.is_some(),
        "torn tail must trigger explicit TailDiscardReceipt"
    );
    assert_eq!(recovered_pubs.len(), 2);
    assert_eq!(recovered_pubs[0], pub1);
    assert_eq!(recovered_pubs[1], pub2);

    // Real 127.0.0.1 loopback TCP resynchronization to a fresh independent client
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server_pubs = recovered_pubs.clone();
    let server_handle =
        thread::spawn(move || net::serve_batch_range_once(&listener, &server_pubs, &bounds));

    let fetched = net::fetch_batch_range_loopback(addr, 1, &bounds).unwrap();
    assert_eq!(server_handle.join().unwrap().unwrap(), 2);
    assert_eq!(fetched, recovered_pubs);

    let mut client =
        SequencerClientVerifierV1::from_fixture(&fix, engine.sequencer_pubkey()).unwrap();
    for p in &fetched {
        let replayed = client.verify_and_apply_publication(p).unwrap();
        assert_eq!(
            replayed.post_composition.execution_root_ref,
            p.header.execution_root_ref
        );
    }
    assert_eq!(client.verified_height(), 2);

    // Verify receipt Merkle inclusion proofs for every single step outcome across both batches
    for p in &fetched {
        for (i, outcome) in p.fold_receipt.step_outcomes.iter().enumerate() {
            let proof = p.build_receipt_proof(i as u32).unwrap();
            client
                .verify_receipt_inclusion(p.header.batch_height, &proof, outcome)
                .unwrap();
        }
    }
}

#[test]
fn wave20_sequencer_1_eight_malicious_input_and_accountability_gates() {
    let fix = CanonicalMultiActorFixtureV1::build().unwrap();
    let mut engine = SequencerEngineV1::from_fixture(&fix, DEFAULT_SEQUENCER_SEED).unwrap();
    let bounds = engine.bounds();
    let wr1 = engine.current_world_root().unwrap();
    let b1 = fix.canonical_mixed_race_bundle(1, wr1).unwrap();
    let pub1 = engine.seal_bundle_and_publish(&b1).unwrap();

    // Gate 1: Oversized > 2 MiB wire frame rejected BEFORE allocation
    let mut oversized_hdr = [0u8; SEQ_WIRE_HEADER_LEN];
    oversized_hdr[0..6].copy_from_slice(&SEQ_WIRE_MAGIC);
    oversized_hdr[6..8].copy_from_slice(&SEQUENCER_1_VERSION.to_be_bytes());
    oversized_hdr[8] = SequencerWireMsgKindV1::BatchPublication.code();
    oversized_hdr[9..13].copy_from_slice(&(bounds.max_block_bytes + 1).to_be_bytes());
    assert!(matches!(
        SequencerWireFrameV1::preflight_header(&oversized_hdr, &bounds),
        Err(Sequencer1Error::BoundsExceeded(_))
    ));

    // Gate 2: Malformed / truncated / trailing-byte wire frame rejected fail-closed
    assert!(matches!(
        SequencerWireFrameV1::decode_canonical(b"BADMAG\x00\x01\x01\x00\x00\x00\x00", &bounds),
        Err(Sequencer1Error::InvalidWire(_))
    ));
    let mut trailing = SequencerWireFrameV1 {
        version: SEQUENCER_1_VERSION,
        msg_kind: SequencerWireMsgKindV1::RequestBatchRange,
        payload: SequencerWireFrameV1::encode_range_request(1).to_vec(),
    }
    .encode_canonical(&bounds)
    .unwrap();
    trailing.push(0xFF);
    assert!(matches!(
        SequencerWireFrameV1::decode_canonical(&trailing, &bounds),
        Err(Sequencer1Error::InvalidWire(_))
    ));

    // Gate 3: Bad sequencer signature rejected
    let mut bad_sig_pub = pub1.clone();
    bad_sig_pub.header.sequencer_signature[0] ^= 0x80;
    let mut client =
        SequencerClientVerifierV1::from_fixture(&fix, engine.sequencer_pubkey()).unwrap();
    assert_eq!(
        client.verify_and_apply_publication(&bad_sig_pub),
        Err(Sequencer1Error::InvalidSequencerSignature)
    );

    // Gate 4: Withheld-body header-only announcement refused
    assert_eq!(
        client.submit_header_without_body(&pub1.header),
        Err(Sequencer1Error::WithheldBodyRefused)
    );

    // Gate 5: Tampered execution root inside signed publication refused by independent replay
    let mut bad_root_pub = pub1.clone();
    bad_root_pub.header.execution_root_ref.execution_state_root[0] ^= 0x01;
    bad_root_pub.header = bad_root_pub
        .header
        .sign_with_seed(&DEFAULT_SEQUENCER_SEED)
        .unwrap();
    assert!(matches!(
        client.verify_and_apply_publication(&bad_root_pub),
        Err(Sequencer1Error::ReplayMismatch(_))
    ));

    // Apply genuine Batch 1 cleanly
    let _ = client.verify_and_apply_publication(&pub1).unwrap();

    // Gate 6: Tampered receipt Merkle proof rejected
    let mut bad_proof = pub1.build_receipt_proof(0).unwrap();
    bad_proof.siblings[0][0] ^= 0x01;
    assert!(matches!(
        client.verify_receipt_inclusion(1, &bad_proof, &pub1.fold_receipt.step_outcomes[0]),
        Err(Sequencer1Error::InvalidReceiptProof(_))
    ));

    // Gate 7: Sequencer equivocation — conflicting signed header at height 1 detected fail-closed!
    let mut alt_engine = SequencerEngineV1::from_fixture(&fix, DEFAULT_SEQUENCER_SEED).unwrap();
    let tx_alt = SignedActorTxEnvelopeV1 {
        envelope_version: MULTI_1_VERSION,
        chain_id: fix.fork_context.chain_id,
        principal_actor_id: fix.actor_c,
        signer_actor_id: fix.actor_c,
        nonce: 0,
        priority_tip: 10,
        expected_world_root: wr1,
        op_kind: MultiActorOpKindV1::TransferNative,
        coord: BEDROCK_COORD,
        target_actor_id: fix.actor_a,
        amount: 1_000,
        signer_pubkey: [0u8; 32],
        signature: [0u8; 64],
    }
    .sign_with_seed(
        &CanonicalMultiActorFixtureV1::ACTOR_C_SEED,
        &fix.fork_context,
    )
    .unwrap();
    let _ = alt_engine
        .admit_wire_envelope(&SequencerWireFrameV1 {
            version: SEQUENCER_1_VERSION,
            msg_kind: SequencerWireMsgKindV1::SubmitEnvelope,
            payload: tx_alt.encode_canonical().unwrap().to_vec(),
        })
        .unwrap();
    let equiv_pub1 = alt_engine.seal_mempool_and_publish(0, 2).unwrap();
    assert!(matches!(
        client.verify_and_apply_publication(&equiv_pub1),
        Err(Sequencer1Error::EquivocationDetected { height: 1, .. })
    ));

    // Gate 8: Broken parent_header_digest on height 2 rejected fail-closed
    let mut broken_h2 = pub1.clone();
    broken_h2.header.batch_height = 2;
    broken_h2.header.parent_header_digest = [0xAA; 32];
    broken_h2.header = broken_h2
        .header
        .sign_with_seed(&DEFAULT_SEQUENCER_SEED)
        .unwrap();
    assert!(matches!(
        client.verify_and_apply_publication(&broken_h2),
        Err(Sequencer1Error::ChainContinuityError(_))
    ));
}
