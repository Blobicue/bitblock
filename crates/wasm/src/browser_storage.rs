//! Browser byte transport for research journals.
//!
//! Exclusive [`bitblock_research::journal::Journal`] file custody is Windows-only
//! (`UnsupportedExclusivePlatform` elsewhere). The durable carrier is still the
//! exact [`StorageChain`] byte stream that `append_synced` writes after
//! `sync_all` returns Ok, and [`inspect_bytes`] / typed replay are the same
//! admission path `open_existing` runs after reading those bytes.
//!
//! IndexedDB stores those carrier bytes. A successful readwrite transaction
//! commit is the durability analogue of `File::sync_all` returning Ok. It is
//! not an atomic-append, power-loss, rollback-resistance, or consensus proof.

#[cfg(test)]
use std::fs;

use bitblock_core::display::{
    CaptureRect, CaptureStage, DisplayCaptureFacts, DisplayLedgerV2, DisplayOutcome,
};
use bitblock_core::frame::FrameRef;
use bitblock_core::frame_v2::{FrameReceiptV2, FrameStream};
use bitblock_core::history::{world_tag, GENESIS_SCHEMA, History, RECEIPT_SCHEMA};
use bitblock_core::program::{Edit, Operation, EDIT_SCHEMA};
use bitblock_core::protocol::{hash_canonical, ContentTag, TagKind};
use bitblock_core::world::{Coord, World};
use bitblock_research::display_artifact::{
    DisplayCaptureArtifact, DISPLAY_CAPTURE_ARTIFACT_SCHEMA,
};
use bitblock_research::display_store::{
    inspect_display_artifact_bytes, replay_display_artifact_records,
    ReadOnlyDisplayArtifactInspection,
};
use bitblock_research::history_store::replay_history_records;
use bitblock_research::journal::{inspect_bytes, AnchorPolicy, ByteScan};
use bitblock_research::storage::{StorageChain, StorageLimits, StorageRecord};

const JOURNAL_OBJECT_SCHEMA: &[u8] = b"bitblock/browser-storage/journal-object/v1";

fn display_label(kind: TagKind, label: &[u8]) -> ContentTag {
    hash_canonical(
        kind,
        b"bitblock/display-artifact-test/v1",
        label.len() as u64,
        |sink| sink(label),
    )
    .expect("display-artifact test label")
}

fn sample_display_artifact() -> DisplayCaptureArtifact {
    const EXPECTED: [u32; 2] = [0x0011_2233, 0x0044_5566];
    let expected = FrameRef::new(2, 1, &EXPECTED).expect("expected frame");
    let captured = FrameRef::new(2, 1, &EXPECTED).expect("captured frame");
    let rect = CaptureRect {
        left: -20,
        top: 30,
        right: -18,
        bottom: 31,
    };
    let facts = DisplayCaptureFacts {
        outcome: DisplayOutcome::Match,
        stage: CaptureStage::Complete,
        error_code: 0,
        flush_before: true,
        compositor_flush: true,
        flush_after: true,
        rect_before: rect,
        rect_after: rect,
        dpi: 144,
        qpc_start: 100,
        qpc_end: 120,
        qpc_frequency: 10_000_000,
        captured: Some(captured.observe()),
        mismatch_count: 0,
        first_mismatch: None,
    };
    let frame_receipt = FrameReceiptV2::new(
        None,
        FrameStream::Composite,
        display_label(TagKind::TickOutcome, b"outcome"),
        display_label(TagKind::FrameInput, b"input"),
        expected.content_tag(),
    )
    .expect("display frame receipt");
    let receipt = *DisplayLedgerV2::default()
        .append(frame_receipt, 2, 1, facts)
        .expect("display ledger append");
    DisplayCaptureArtifact::new(receipt, expected, Some(captured)).expect("matched display artifact")
}

/// Canonical journal sample: two parent-linked storage records.
pub fn journal_sample_chain() -> StorageChain {
    let mut chain = StorageChain::new();
    let first = Edit::new(Operation::On, Coord::new(0, 0, 0));
    chain
        .append(TagKind::Edit, EDIT_SCHEMA, &first.encode_canonical())
        .expect("first journal record");
    chain
        .append(TagKind::World, JOURNAL_OBJECT_SCHEMA, b"second")
        .expect("second journal record");
    chain
}

pub fn journal_write_status() -> (String, Vec<u8>) {
    let chain = journal_sample_chain();
    let bytes = chain.encode();
    let head = chain.head().expect("sample has a head");
    let status = format!(
        "bitblock_browser_storage ok store=journal op=write records={} head={} bytes={}",
        chain.records().len(),
        head,
        bytes.len(),
    );
    (status, bytes)
}

pub fn journal_verify_status(bytes: &[u8]) -> String {
    match inspect_bytes(bytes, StorageLimits::DEFAULT, AnchorPolicy::UnanchoredLocal) {
        Ok(ByteScan::Clean(report)) => format!(
            "bitblock_browser_storage ok store=journal op=verify scan=clean records={} head={} bytes={} valid_prefix={}",
            report.valid_record_count,
            report
                .valid_head
                .map(|tag| tag.to_string())
                .unwrap_or_else(|| "none".to_owned()),
            report.file_bytes,
            report.valid_prefix_bytes,
        ),
        Ok(ByteScan::NeedsRecovery(report)) => format!(
            "bitblock_browser_storage ok store=journal op=verify scan=recovery records={} head={} bytes={} valid_prefix={} rejected={}",
            report.valid_record_count,
            report
                .valid_head
                .map(|tag| tag.to_string())
                .unwrap_or_else(|| "none".to_owned()),
            report.file_bytes,
            report.valid_prefix_bytes,
            report.rejected_tail_bytes,
        ),
        Err(err) => format!("bitblock_browser_storage err store=journal op=verify error={err}"),
    }
}

/// History sample: origin world genesis plus one On tick at (1,0,0).
pub fn history_sample_chain() -> (World, StorageChain, ContentTag, ContentTag, usize) {
    let initial = World::new_with_origin();
    let mut world = initial.clone();
    let mut history = History::new(&world);
    let mut chain = StorageChain::new();
    chain
        .append(TagKind::Receipt, GENESIS_SCHEMA, &history.genesis_payload())
        .expect("history genesis");
    let mut transaction = history.begin_tick(1);
    transaction.apply_edit(
        &history,
        &mut world,
        Edit::new(Operation::On, Coord::new(1, 0, 0)),
    );
    let candidate = history
        .preview_tick(&transaction, &world)
        .expect("history preview");
    let payload = candidate.canonical_payload().encode_canonical();
    let stored = chain
        .append(TagKind::Receipt, RECEIPT_SCHEMA, &payload)
        .expect("history receipt");
    assert_eq!(stored.object_tag(), candidate.tag());
    history
        .commit_tick(transaction, &world)
        .expect("history commit");
    let world_tag = world_tag(&world);
    let head = history.head();
    let on_bits = world.iter_on().count();
    (initial, chain, world_tag, head, on_bits)
}

pub fn history_write_status() -> (String, Vec<u8>) {
    let (initial, chain, world, head, on_bits) = history_sample_chain();
    let bytes = chain.encode();
    let replayed = replay_history_records(&initial, chain.records()).expect("history replay");
    assert_eq!(world_tag(replayed.world()), world);
    assert_eq!(replayed.history().head(), head);
    let status = format!(
        "bitblock_browser_storage ok store=history op=write records={} world={} head={} on_bits={} bytes={}",
        chain.records().len(),
        world,
        head,
        on_bits,
        bytes.len(),
    );
    (status, bytes)
}

pub fn history_verify_status(bytes: &[u8]) -> String {
    let initial = World::new_with_origin();
    match inspect_bytes(bytes, StorageLimits::DEFAULT, AnchorPolicy::UnanchoredLocal) {
        Ok(ByteScan::Clean(report)) => {
            let chain = match StorageChain::decode_with_limits(
                &bytes[..report.valid_prefix_bytes as usize],
                StorageLimits::DEFAULT,
            ) {
                Ok(chain) => chain,
                Err(err) => {
                    return format!(
                        "bitblock_browser_storage err store=history op=verify decode={err}"
                    );
                }
            };
            match replay_history_records(&initial, chain.records()) {
                Ok(replayed) => format!(
                    "bitblock_browser_storage ok store=history op=verify scan=clean records={} world={} head={} on_bits={} bytes={} valid_prefix={}",
                    report.valid_record_count,
                    world_tag(replayed.world()),
                    replayed.history().head(),
                    replayed.world().iter_on().count(),
                    report.file_bytes,
                    report.valid_prefix_bytes,
                ),
                Err(err) => format!(
                    "bitblock_browser_storage err store=history op=verify replay={err}"
                ),
            }
        }
        Ok(ByteScan::NeedsRecovery(report)) => format!(
            "bitblock_browser_storage ok store=history op=verify scan=recovery records={} bytes={} valid_prefix={} rejected={}",
            report.valid_record_count,
            report.file_bytes,
            report.valid_prefix_bytes,
            report.rejected_tail_bytes,
        ),
        Err(err) => format!("bitblock_browser_storage err store=history op=verify error={err}"),
    }
}

/// Display sample: one matched capture artifact as a typed storage record.
pub fn display_sample_chain() -> StorageChain {
    let artifact = sample_display_artifact();
    let payload = artifact.encode_canonical().expect("display payload");
    let mut chain = StorageChain::new();
    let record = StorageRecord::from_tagged_payload(
        None,
        artifact.tag(),
        DISPLAY_CAPTURE_ARTIFACT_SCHEMA,
        &payload,
    )
    .expect("display storage record");
    chain.admit(record).expect("admit display record");
    chain
}

pub fn display_write_status() -> (String, Vec<u8>) {
    let chain = display_sample_chain();
    let bytes = chain.encode();
    let replayed = replay_display_artifact_records(chain.records()).expect("display replay");
    let artifact = replayed
        .entries()
        .first()
        .expect("one display entry")
        .artifact;
    let status = format!(
        "bitblock_browser_storage ok store=display op=write records={} artifact={} storage_head={} bytes={}",
        chain.records().len(),
        artifact,
        chain.head().expect("display head"),
        bytes.len(),
    );
    (status, bytes)
}

pub fn display_verify_status(bytes: &[u8]) -> String {
    match inspect_display_artifact_bytes(bytes, StorageLimits::DEFAULT, AnchorPolicy::UnanchoredLocal)
    {
        Ok(ReadOnlyDisplayArtifactInspection::Clean { carrier, state }) => {
            let artifact = state
                .entries()
                .first()
                .map(|entry| entry.artifact.to_string())
                .unwrap_or_else(|| "none".to_owned());
            format!(
                "bitblock_browser_storage ok store=display op=verify scan=clean records={} artifact={} storage_head={} bytes={} valid_prefix={}",
                carrier.valid_record_count,
                artifact,
                carrier
                    .valid_head
                    .map(|tag| tag.to_string())
                    .unwrap_or_else(|| "none".to_owned()),
                carrier.file_bytes,
                carrier.valid_prefix_bytes,
            )
        }
        Ok(ReadOnlyDisplayArtifactInspection::NeedsRecovery { carrier, state }) => format!(
            "bitblock_browser_storage ok store=display op=verify scan=recovery records={} artifacts={} bytes={} valid_prefix={} rejected={}",
            carrier.valid_record_count,
            state.len(),
            carrier.file_bytes,
            carrier.valid_prefix_bytes,
            carrier.rejected_tail_bytes,
        ),
        Err(err) => format!("bitblock_browser_storage err store=display op=verify error={err}"),
    }
}

/// Write `bytes` through std::fs and read them back. Not Journal exclusive custody.
#[cfg(test)]
pub fn tempfile_roundtrip(label: &str, bytes: &[u8]) -> Vec<u8> {
    let path = std::env::temp_dir().join(format!(
        "bitblock-browser-storage-{label}-{}.bbj",
        std::process::id()
    ));
    fs::write(&path, bytes).expect("temp write");
    let read = fs::read(&path).expect("temp read");
    let _ = fs::remove_file(&path);
    read
}

#[cfg(test)]
pub fn assert_carrier_file_matches_bytes(label: &str, bytes: &[u8]) {
    let from_disk = tempfile_roundtrip(label, bytes);
    assert_eq!(from_disk, bytes, "{label}: tempfile bytes diverged");
    let memory = inspect_bytes(bytes, StorageLimits::DEFAULT, AnchorPolicy::UnanchoredLocal)
        .expect("memory inspect");
    let disk = inspect_bytes(&from_disk, StorageLimits::DEFAULT, AnchorPolicy::UnanchoredLocal)
        .expect("disk inspect");
    assert_eq!(memory, disk, "{label}: inspect_bytes diverged across tempfile");
}

#[cfg(test)]
pub fn corrupt_tail_case(clean: &[u8]) -> (Vec<u8>, u64) {
    let mut torn = clean.to_vec();
    torn.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef, 0x00]);
    let expected_prefix = clean.len() as u64;
    (torn, expected_prefix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal_bytes_match_tempfile_and_survive_reload_and_corrupt_tail() {
        let (write, bytes) = journal_write_status();
        assert!(write.starts_with("bitblock_browser_storage ok store=journal op=write "));
        assert_carrier_file_matches_bytes("journal", &bytes);
        let verify = journal_verify_status(&bytes);
        assert_eq!(
            verify,
            format!(
                "bitblock_browser_storage ok store=journal op=verify scan=clean records=2 head={} bytes={} valid_prefix={}",
                StorageChain::decode(&bytes).unwrap().head().unwrap(),
                bytes.len(),
                bytes.len(),
            )
        );
        let (torn, prefix) = corrupt_tail_case(&bytes);
        assert_carrier_file_matches_bytes("journal-torn", &torn);
        let recovery = journal_verify_status(&torn);
        assert!(recovery.contains("scan=recovery"));
        assert!(recovery.contains(&format!("valid_prefix={prefix}")));
        assert!(recovery.contains("rejected=5"));
        let truncated = &torn[..prefix as usize];
        assert_eq!(journal_verify_status(truncated), verify);
    }

    #[test]
    fn history_bytes_match_tempfile_and_reload_replays_same_world() {
        let (write, bytes) = history_write_status();
        assert!(write.starts_with("bitblock_browser_storage ok store=history op=write "));
        assert_carrier_file_matches_bytes("history", &bytes);
        let verify = history_verify_status(&bytes);
        assert!(verify.starts_with(
            "bitblock_browser_storage ok store=history op=verify scan=clean records=2 "
        ));
        assert!(verify.contains(&format!("bytes={}", bytes.len())));
        let (torn, prefix) = corrupt_tail_case(&bytes);
        let recovery = history_verify_status(&torn);
        assert!(recovery.contains("scan=recovery"));
        assert!(recovery.contains(&format!("valid_prefix={prefix}")));
        let truncated = &torn[..prefix as usize];
        assert_eq!(history_verify_status(truncated), verify);
    }

    #[test]
    fn display_bytes_match_tempfile_and_typed_reload() {
        let (write, bytes) = display_write_status();
        assert!(write.starts_with("bitblock_browser_storage ok store=display op=write "));
        assert_carrier_file_matches_bytes("display", &bytes);
        let verify = display_verify_status(&bytes);
        assert!(verify.starts_with(
            "bitblock_browser_storage ok store=display op=verify scan=clean records=1 "
        ));
        let (torn, prefix) = corrupt_tail_case(&bytes);
        let recovery = display_verify_status(&torn);
        assert!(recovery.contains("scan=recovery"));
        assert!(recovery.contains(&format!("valid_prefix={prefix}")));
        let truncated = &torn[..prefix as usize];
        assert_eq!(display_verify_status(truncated), verify);
    }

}
