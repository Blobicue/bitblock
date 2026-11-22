//! `CODEC-1@local-v1` dual-language adversarial negative-control integration suite.
//!
//! Mirrors `tools/codec_parity_corpus.py` directly against the live Rust decoders in
//! `bitblock-core` (always compiled) and `bitblock-research` (compiled under
//! `--features protocol-research`). Verifies exhaustive prefix truncation rejection,
//! trailing-byte rejection, overlong/overflow preflight before allocation, noncanonical
//! encoding rejection, and cross-domain/cross-version `ContentTag` separation.

use bitblock::camera::Camera;
use bitblock::display::{CaptureStage, DisplayCaptureFacts, DisplayReceiptV2};
use bitblock::frame_v2::{FrameReceiptV2, FrameStream};
use bitblock::history::{History, HistoryReceiptPayload};
use bitblock::interaction::InteractionTape;
use bitblock::program::{Edit, Operation};
use bitblock::protocol::{ContentTag, TagKind, hash_canonical};
use bitblock::tick::{
    MAX_ACTIONS_PER_TICK, TickActionLedger, TickCameraState, TickDecodeLimits, TickInputFlags,
    TickInputState, TickOutcome,
};
use bitblock::world::{Coord, World};

fn assert_all_truncations_and_trailing_reject<T, E>(
    valid: &[u8],
    name: &str,
    mut decode: impl FnMut(&[u8]) -> Result<T, E>,
) {
    assert!(
        decode(valid).is_ok(),
        "{name}: canonical fixture must decode"
    );
    for cut in 0..valid.len() {
        assert!(
            decode(&valid[..cut]).is_err(),
            "{name}: truncated prefix of {cut}/{} bytes must fail closed",
            valid.len()
        );
    }
    let mut trail_zero = valid.to_vec();
    trail_zero.push(0);
    assert!(
        decode(&trail_zero).is_err(),
        "{name}: trailing 0x00 byte must fail closed"
    );
    let mut trail_ff = valid.to_vec();
    trail_ff.push(0xff);
    assert!(
        decode(&trail_ff).is_err(),
        "{name}: trailing 0xff byte must fail closed"
    );
}

#[test]
fn core_codecs_reject_every_truncation_trailing_and_invalid_discriminant() {
    // 1. ContentTag canonical text parsing & cross-domain/cross-version separation
    let edit = Edit::new(Operation::On, Coord::new(0, 0, 0));
    let edit_bytes = edit.encode_canonical();
    let edit_tag = hash_canonical(
        TagKind::Edit,
        b"bitblock/edit/op-u8-i64-be/v1",
        edit_bytes.len() as u64,
        |s| s(&edit_bytes),
    )
    .expect("edit tag");
    let tag_text = edit_tag.to_string();
    assert_eq!(ContentTag::parse_full(&tag_text), Ok(edit_tag));
    assert!(ContentTag::parse_full(&tag_text.to_uppercase()).is_err());
    assert!(ContentTag::parse_full(&format!("{tag_text}====")).is_err());
    assert!(ContentTag::parse_full(&tag_text[..tag_text.len() - 1]).is_err());
    assert!(ContentTag::parse_full(&format!("{tag_text} ")).is_err());
    assert!(ContentTag::parse_full(&tag_text.replace(".edit.", ".bogus.")).is_err());
    // Non-canonical final base32 character (256 % 5 = 1 bit; 'b' sets an unused trailing bit)
    let bad_b32 = format!("{}b", &tag_text[..tag_text.len() - 1]);
    assert!(ContentTag::parse_full(&bad_b32).is_err());

    // Cross-domain & cross-version separation on identical bytes
    let same = [0u8; 32];
    let fr_v1 = hash_canonical(
        TagKind::FrameReceipt,
        b"bitblock/frame-receipt/v1",
        32,
        |s| s(&same),
    )
    .expect("fr v1");
    let fr_v2 = hash_canonical(
        TagKind::FrameReceipt,
        b"bitblock/frame-receipt/v2",
        32,
        |s| s(&same),
    )
    .expect("fr v2");
    assert_ne!(fr_v1, fr_v2);

    // 2. Edit (25 bytes)
    assert_all_truncations_and_trailing_reject(&edit_bytes, "Edit", Edit::decode_canonical);
    for bad_op in [3u8, 4, 128, 255] {
        let mut bad = edit_bytes;
        bad[0] = bad_op;
        assert!(Edit::decode_canonical(&bad).is_err());
    }

    // 3. InteractionTape
    let tape =
        InteractionTape::parse("tick\nmove forward down\nlook 2.5 -1.25\nplace\nsidebar\nfit\n")
            .expect("valid tape");
    let tape_bytes = tape.encode_canonical();
    assert_all_truncations_and_trailing_reject(
        &tape_bytes,
        "InteractionTape",
        InteractionTape::decode_canonical,
    );

    // 4. HistoryReceiptPayload
    let mut world = World::new();
    let mut history = History::new(&world);
    let mut tx = history.begin_tick(1);
    tx.apply_edit(
        &history,
        &mut world,
        Edit::new(Operation::On, Coord::new(1, 0, 0)),
    );
    let receipt = history.commit_tick(tx, &world).expect("commit receipt");
    let receipt_bytes = receipt.canonical_payload().encode_canonical();
    assert_all_truncations_and_trailing_reject(
        &receipt_bytes,
        "HistoryReceiptPayload",
        HistoryReceiptPayload::decode_canonical,
    );

    // 5. TickOutcome v3 (469-byte fixed genesis-idle payload)
    let genesis_world = World::new();
    let genesis_history = History::new(&genesis_world);
    let world_tag = genesis_history.genesis_world();
    let history_tag = genesis_history.head();
    let input = TickInputState::new(
        TickInputFlags::new(
            false, false, false, false, false, false, false, false, false, true, false,
        ),
        0.0,
        0.0,
        0.0,
        64,
        64,
        64,
        64,
    )
    .expect("valid input");
    let outcome = TickOutcome::new(
        None,
        0,
        world_tag,
        history_tag,
        input,
        TickActionLedger::empty(),
        [],
        world_tag,
        history_tag,
        TickCameraState::capture(&Camera::for_observer_spawn()),
        None,
        None,
    )
    .expect("valid outcome");
    let outcome_bytes = outcome.encode_canonical();
    assert_all_truncations_and_trailing_reject(
        &outcome_bytes,
        "TickOutcomeV3",
        TickOutcome::decode_canonical,
    );

    // 6. FrameReceiptV2 (130 bytes)
    let fake_input = hash_canonical(TagKind::FrameInput, b"bitblock/frame-input/v1", 1, |s| {
        s(&[1])
    })
    .expect("frame input tag");
    let fake_frame = hash_canonical(
        TagKind::Frame,
        b"bitblock/frame/xrgb8888-row-major/v1",
        1,
        |s| s(&[2]),
    )
    .expect("frame tag");
    let fr2 = FrameReceiptV2::new(
        None,
        FrameStream::Game,
        outcome.tag(),
        fake_input,
        fake_frame,
    )
    .expect("valid FrameReceiptV2");
    let fr2_bytes = fr2.encode_canonical();
    assert_all_truncations_and_trailing_reject(
        &fr2_bytes,
        "FrameReceiptV2",
        FrameReceiptV2::decode_canonical,
    );

    // 7. DisplayReceiptV2 (378 bytes)
    let facts = DisplayCaptureFacts::unavailable(CaptureStage::WindowHandle, 5);
    let fr2_comp = FrameReceiptV2::new(
        None,
        FrameStream::Composite,
        outcome.tag(),
        fake_input,
        fake_frame,
    )
    .expect("valid composite FrameReceiptV2");
    let dr2 =
        DisplayReceiptV2::prepare(None, fr2_comp, 64, 64, facts).expect("valid DisplayReceiptV2");
    let dr2_bytes = dr2.encode_canonical();
    assert_all_truncations_and_trailing_reject(
        &dr2_bytes,
        "DisplayReceiptV2",
        DisplayReceiptV2::decode_canonical,
    );
}

#[test]
fn core_codecs_reject_overlong_and_overflow_headers_before_allocation() {
    // 1. InteractionTape claiming u32::MAX tick records in a 16-byte buffer
    let mut bad_tape = Vec::new();
    bad_tape.extend_from_slice(&u32::MAX.to_be_bytes());
    bad_tape.extend_from_slice(&[0u8; 12]);
    assert!(InteractionTape::decode_canonical(&bad_tape).is_err());

    // 2. HistoryReceiptPayload claiming u64::MAX edits in a small buffer
    let mut bad_receipt = vec![0u8; 120];
    // Edit count sits at offset 40 (32 parent + 8 tick)
    bad_receipt[40..48].copy_from_slice(&u64::MAX.to_be_bytes());
    assert!(HistoryReceiptPayload::decode_canonical(&bad_receipt).is_err());

    // 3. TickOutcome v3 claiming tight decode limit & over-limit action ledger
    let genesis_world = World::new();
    let genesis_history = History::new(&genesis_world);
    let world_tag = genesis_history.genesis_world();
    let history_tag = genesis_history.head();
    let input = TickInputState::new(
        TickInputFlags::new(
            false, false, false, false, false, false, false, false, false, true, false,
        ),
        0.0,
        0.0,
        0.0,
        64,
        64,
        64,
        64,
    )
    .expect("valid input");
    let outcome = TickOutcome::new(
        None,
        0,
        world_tag,
        history_tag,
        input,
        TickActionLedger::empty(),
        [],
        world_tag,
        history_tag,
        TickCameraState::capture(&Camera::for_observer_spawn()),
        None,
        None,
    )
    .expect("valid outcome");
    let valid = outcome.encode_canonical();

    // Tight decode limit below actual payload rejects before materialization
    let tight = TickDecodeLimits {
        max_attempts: 0,
        max_payload_bytes: valid.len() - 1,
    };
    assert!(TickOutcome::decode_canonical_with_limits(&valid, tight).is_err());

    // Mutate action ledger to exceed MAX_ACTIONS_PER_TICK
    let mut ledger = TickActionLedger::empty();
    for _ in 0..MAX_ACTIONS_PER_TICK {
        ledger.record(0).expect("within limit");
    }
    assert!(ledger.record(0).is_err());

    // Non-finite f64 in TickInputState rejects at construction
    assert!(
        TickInputState::new(
            TickInputFlags::new(
                false, false, false, false, false, false, false, false, false, true, false,
            ),
            f64::NAN,
            0.0,
            0.0,
            64,
            64,
            64,
            64,
        )
        .is_err()
    );
    assert!(
        TickInputState::new(
            TickInputFlags::new(
                false, false, false, false, false, false, false, false, false, true, false,
            ),
            f64::INFINITY,
            0.0,
            0.0,
            64,
            64,
            64,
            64,
        )
        .is_err()
    );
}

#[cfg(feature = "protocol-research")]
#[test]
fn research_codecs_reject_every_truncation_trailing_and_malformed_encoding() {
    use bitblock::commit_r1::{
        CoordinateKeyV1, MerklePatriciaTrie, MortonGapStreamV1, PatriciaPointProof,
        ProofPolicyLimits, SmtPointProof, SparseMerkleTree,
    };
    use bitblock::coordinate_episode::CoordinateEpisodeEvidence;
    use bitblock::local_bounds::LocalStorageProfile;
    use bitblock::octree_commit::{
        ChunkLeafV1, OctreeInternalNodeV1, OctreePointProofV1, OctreeRootV1, SparseOctreeV1,
        VerkleR1FeasibilityReportV1,
    };
    use bitblock::scale_assay::ScaleWorkloadReportV1;
    use bitblock::storage::{StorageLimits, StorageRecord};

    // 1. LocalStorageProfile (56 bytes)
    let bounds_bytes = LocalStorageProfile::DEFAULT.encode_canonical();
    assert_all_truncations_and_trailing_reject(
        &bounds_bytes,
        "LocalStorageProfile",
        LocalStorageProfile::decode_canonical,
    );

    // 2. StorageRecord envelope
    let edit_bytes = Edit::new(Operation::On, Coord::new(0, 0, 0)).encode_canonical();
    let rec = StorageRecord::new(
        None,
        TagKind::Edit,
        b"bitblock/edit/op-u8-i64-be/v1",
        &edit_bytes,
    )
    .expect("valid storage record");
    let rec_bytes = rec.encode();
    assert_all_truncations_and_trailing_reject(&rec_bytes, "StorageRecord", |b| {
        StorageRecord::decode_with_limits(b, None, StorageLimits::DEFAULT)
    });

    // 3. CoordinateEpisodeEvidence
    let mut world = World::new();
    let mut history = History::new(&world);
    let mut tx = history.begin_tick(1);
    tx.apply_edit(
        &history,
        &mut world,
        Edit::new(Operation::On, Coord::new(2, 3, 4)),
    );
    history.commit_tick(tx, &world).expect("commit mint");
    let ep_ev =
        CoordinateEpisodeEvidence::derive(&World::new(), &history).expect("episode evidence");
    let ep_bytes = ep_ev
        .try_encode_canonical()
        .expect("encode episode evidence");
    assert_all_truncations_and_trailing_reject(
        &ep_bytes,
        "CoordinateEpisodeEvidence",
        CoordinateEpisodeEvidence::decode_canonical,
    );

    // 4. MortonGapStreamV1
    let stream = MortonGapStreamV1::from_coords(&[
        Coord::new(-16, -16, -16),
        Coord::new(0, 0, 0),
        Coord::new(1, 2, 3),
    ])
    .expect("valid stream");
    let stream_bytes = stream.encode();
    assert_all_truncations_and_trailing_reject(
        &stream_bytes,
        "MortonGapStreamV1",
        MortonGapStreamV1::decode,
    );

    // 5. PatriciaPointProof & SmtPointProof
    let mpt = MerklePatriciaTrie::from_world(&world);
    let smt = SparseMerkleTree::from_world(&world);
    let q = CoordinateKeyV1::from_coord(Coord::new(2, 3, 4));
    let p_bytes = mpt.prove(&q).to_canonical_bytes();
    assert_all_truncations_and_trailing_reject(&p_bytes, "PatriciaPointProof", |b| {
        PatriciaPointProof::from_canonical_bytes(b).ok_or(())
    });
    assert_eq!(
        MerklePatriciaTrie::verify_proof_bytes_with_limits(
            mpt.root(),
            &q,
            &p_bytes,
            ProofPolicyLimits::PATRICIA_R1_DEFAULT,
        ),
        Some(true)
    );

    let s_bytes = smt.prove(&q).to_canonical_bytes();
    assert_all_truncations_and_trailing_reject(&s_bytes, "SmtPointProof", |b| {
        SmtPointProof::from_canonical_bytes(b).ok_or(())
    });
    assert_eq!(
        SparseMerkleTree::verify_proof_bytes_with_limits(
            smt.root(),
            &q,
            &s_bytes,
            ProofPolicyLimits::SMT_R1_DEFAULT,
        ),
        Some(true)
    );

    // 6. Octree ChunkLeafV1, OctreeInternalNodeV1, OctreeRootV1, OctreePointProofV1, VerkleR1FeasibilityReportV1
    let mut multi_chunk = World::new();
    multi_chunk.set_on(Coord::new(-1, -1, -1));
    multi_chunk.set_on(Coord::new(16, 16, 16));
    let oct = SparseOctreeV1::from_world(&multi_chunk);
    let root_bytes = oct.root().encode();
    assert_all_truncations_and_trailing_reject(&root_bytes, "OctreeRootV1", |b| {
        OctreeRootV1::decode(b).ok_or(())
    });
    let proof_bytes = oct.prove(Coord::new(-1, -1, -1)).encode();
    assert_all_truncations_and_trailing_reject(&proof_bytes, "OctreePointProofV1", |b| {
        OctreePointProofV1::decode(b).ok_or(())
    });
    let origin_world = World::new();
    let (_, occ, _, origin_words) = origin_world.iter_chunks().next().expect("origin chunk");
    let canon_leaf_bytes =
        ChunkLeafV1::from_chunk_words(Coord::new(0, 0, 0), occ, origin_words).encode();
    assert_eq!(canon_leaf_bytes.len(), 537);
    assert_all_truncations_and_trailing_reject(&canon_leaf_bytes, "ChunkLeafV1", |b| {
        ChunkLeafV1::decode(b).ok_or(())
    });
    let internal = OctreeInternalNodeV1 {
        branch_depth: 0,
        prefix_bits_180b: [0u8; 23],
        child_mask: 0b0000_0011,
        child_digests: vec![[0x11; 32], [0x22; 32]],
    };
    let internal_bytes = internal.encode();
    assert_all_truncations_and_trailing_reject(&internal_bytes, "OctreeInternalNodeV1", |b| {
        OctreeInternalNodeV1::decode(b).ok_or(())
    });
    let verkle_bytes = VerkleR1FeasibilityReportV1::canonical().encode();
    assert_all_truncations_and_trailing_reject(&verkle_bytes, "VerkleR1FeasibilityReportV1", |b| {
        VerkleR1FeasibilityReportV1::decode(b).ok_or(())
    });

    // 7. ScaleWorkloadReportV1 (929 bytes)
    let scale_bytes = ScaleWorkloadReportV1::canonical().encode();
    assert_all_truncations_and_trailing_reject(
        &scale_bytes,
        "ScaleWorkloadReportV1",
        ScaleWorkloadReportV1::decode,
    );
}

#[cfg(feature = "protocol-research")]
#[test]
fn research_codecs_reject_overlong_overflow_and_policy_limit_bypass() {
    use bitblock::commit_r1::{
        CoordinateKeyV1, MerklePatriciaTrie, MortonGapStreamV1, PatriciaPointProof,
        ProofPolicyLimits, SmtPointProof, SparseMerkleTree,
    };
    use bitblock::storage::{MAX_STORAGE_RECORD_BYTES, StorageLimits, StorageRecord};

    // 1. StorageRecord claiming MAX_STORAGE_RECORD_BYTES + 1 and u64::MAX in a tiny buffer
    for claimed in [(MAX_STORAGE_RECORD_BYTES as u64) + 1, u64::MAX] {
        let mut bad = Vec::new();
        bad.extend_from_slice(b"BBSTRREC");
        bad.extend_from_slice(&1u16.to_be_bytes());
        bad.extend_from_slice(&claimed.to_be_bytes());
        bad.extend_from_slice(&[0u8; 64]);
        assert!(StorageRecord::decode_with_limits(&bad, None, StorageLimits::DEFAULT).is_err());
    }

    // 2. MortonGapStreamV1 claiming u32::MAX keys in a 32-byte buffer
    let mut bad_morton = Vec::new();
    bad_morton.extend_from_slice(&u32::MAX.to_be_bytes());
    bad_morton.extend_from_slice(&[0u8; 28]);
    assert!(MortonGapStreamV1::decode(&bad_morton).is_err());

    // 3. PatriciaPointProof claiming >192 steps & u16::MAX steps
    assert!(PatriciaPointProof::from_canonical_bytes(&[0x00, 0xc1, 0x00]).is_none());
    assert!(PatriciaPointProof::from_canonical_bytes(&[0xff, 0xff, 0x00]).is_none());

    // 4. SmtPointProof claiming u16::MAX siblings in 27-byte buffer
    let mut bad_smt = vec![0u8; 27];
    bad_smt[25] = 0xff;
    bad_smt[26] = 0xff;
    assert!(SmtPointProof::from_canonical_bytes(&bad_smt).is_none());

    // 5. ProofPolicyLimits widening attempt fails closed; one-over-byte cap fails before decode
    let smt_pol = ProofPolicyLimits::SMT_R1_DEFAULT;
    assert!(
        smt_pol
            .tighten(ProofPolicyLimits {
                max_proof_bytes: smt_pol.max_proof_bytes + 1,
                ..smt_pol
            })
            .is_none()
    );
    let world = World::new();
    let mpt = MerklePatriciaTrie::from_world(&world);
    let smt = SparseMerkleTree::from_world(&world);
    let q = CoordinateKeyV1::from_coord(Coord::new(0, 0, 0));
    let p_bytes = mpt.prove(&q).to_canonical_bytes();
    let s_bytes = smt.prove(&q).to_canonical_bytes();
    assert_eq!(
        MerklePatriciaTrie::verify_proof_bytes_with_limits(
            mpt.root(),
            &q,
            &p_bytes,
            ProofPolicyLimits {
                max_proof_bytes: p_bytes.len() - 1,
                ..ProofPolicyLimits::PATRICIA_R1_DEFAULT
            },
        ),
        None
    );
    assert_eq!(
        SparseMerkleTree::verify_proof_bytes_with_limits(
            smt.root(),
            &q,
            &s_bytes,
            ProofPolicyLimits {
                max_proof_bytes: s_bytes.len() - 1,
                ..ProofPolicyLimits::SMT_R1_DEFAULT
            },
        ),
        None
    );
}
