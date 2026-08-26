//! Exclusive-writer filesystem journal for canonical storage records.
//!
//! The journal is deliberately narrower than a database. On Windows it retains one read/write
//! handle opened with sharing disabled, appends one fully encoded [`StorageRecord`] at the
//! verified end of file, calls [`std::fs::File::sync_all`], and only then publishes the new local
//! head. Recovery admits the longest canonical parent-linked prefix and exposes the entire
//! rejected suffix. It never truncates, skips, or resynchronizes automatically.
//!
//! A successful sync is evidence that the operating-system call returned success. It is not an
//! atomic-append, power-loss, rollback-resistance, custody, availability, or consensus proof.

use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;

use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::storage::{StorageChain, StorageError, StorageLimits, StorageRecord, StorageScanner};

/// Schema for a rejected physical suffix. Its payload binds both offsets before the exact bytes.
pub const JOURNAL_REJECTED_TAIL_SCHEMA: &[u8] = b"bitblock/journal/rejected-tail/v1";

/// Optional externally retained evidence used while opening a journal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AnchorPolicy {
    /// Admit any locally canonical chain. Valid boundary truncation remains locally invisible.
    #[default]
    UnanchoredLocal,
    /// Require this storage-record tag to occur in the admitted prefix.
    RequireAncestor(ContentTag),
}

impl AnchorPolicy {
    fn validate(self) -> Result<(), JournalError> {
        if let Self::RequireAncestor(required) = self
            && required.kind() != TagKind::StorageRecord
        {
            return Err(JournalError::RequiredAncestorKind {
                actual: required.kind(),
            });
        }
        Ok(())
    }

    fn required(self) -> Option<ContentTag> {
        match self {
            Self::UnanchoredLocal => None,
            Self::RequireAncestor(required) => Some(required),
        }
    }
}

/// Exact report for a journal whose entire byte stream was admitted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanReport {
    pub file_bytes: u64,
    pub valid_prefix_bytes: u64,
    pub valid_record_count: usize,
    pub valid_head: Option<ContentTag>,
    pub required_ancestor_seen: bool,
}

/// Exact report for the first refused record and every unsearched byte after it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryReport {
    pub file_bytes: u64,
    pub valid_prefix_bytes: u64,
    pub rejected_tail_bytes: u64,
    pub valid_record_count: usize,
    pub valid_head: Option<ContentTag>,
    pub required_ancestor_seen: bool,
    pub rejection_class: RejectionClass,
    pub first_error: StorageError,
    pub rejected_tail_tag: ContentTag,
}

/// Why scanning stopped, separated from the exact decoder error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RejectionClass {
    /// Bytes violate this protocol's structure, parent chain, or authentication checks.
    StructuralOrAuthentication,
    /// Bytes may be canonical under wider protocol-permitted local resource limits.
    PolicyLimit,
    /// Bytes may belong to a protocol version or object kind this build does not implement.
    UnsupportedProtocol,
}

/// Read-only result of scanning supplied bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ByteScan {
    Clean(ScanReport),
    NeedsRecovery(RecoveryReport),
}

/// Result of opening an existing file while retaining its exclusive handle.
pub enum OpenJournal {
    Ready {
        journal: Journal,
        report: ScanReport,
    },
    NeedsRecovery {
        journal: Box<RecoveryJournal>,
    },
}

/// A writable journal whose OS-visible bytes passed the declared scan and append checks.
pub struct Journal {
    file: File,
    chain: StorageChain,
    valid_bytes: u64,
    poisoned: bool,
}

/// A non-writable journal retaining an invalid suffix and the exclusive file handle.
pub struct RecoveryJournal {
    file: File,
    chain: StorageChain,
    report: RecoveryReport,
    anchor: AnchorPolicy,
}

/// What a successful append actually observed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncEvidence {
    OsSyncAllReturnedOk,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppendedBytesEvidence {
    OsVisibleBytesRereadEqual,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateReceipt {
    pub file_bytes: u64,
    pub sync_evidence: SyncEvidence,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppendSyncReceipt {
    pub parent: Option<ContentTag>,
    pub object_tag: ContentTag,
    pub record_tag: ContentTag,
    pub start_offset: u64,
    pub end_offset: u64,
    pub encoded_bytes: u64,
    pub sync_evidence: SyncEvidence,
    pub appended_bytes_evidence: AppendedBytesEvidence,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TailDiscardReceipt {
    pub prior_file_bytes: u64,
    pub retained_prefix_bytes: u64,
    pub discarded_bytes: u64,
    pub retained_head: Option<ContentTag>,
    pub rejected_tail_tag: ContentTag,
    pub first_error: StorageError,
    pub sync_evidence: SyncEvidence,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppendPhase {
    Write,
    SyncAll,
    VerifyEnd,
    VerifyContent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UncertainAppend {
    pub phase: AppendPhase,
    pub candidate_record_tag: ContentTag,
    pub start_offset: u64,
    pub expected_end_offset: u64,
    pub accepted_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TailDiscardPhase {
    SetLength,
    SyncAll,
    VerifyPrefix,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JournalIoPhase {
    CreateNew,
    OpenExisting,
    Metadata,
    Read,
    Seek,
    SyncNewFile,
}

#[derive(Debug)]
pub enum JournalError {
    Storage(StorageError),
    Io {
        phase: JournalIoPhase,
        source: io::Error,
    },
    UnsupportedExclusivePlatform,
    NotRegularFile,
    FileTooLarge {
        actual: u64,
        max: u64,
    },
    AllocationFailed {
        requested: usize,
    },
    OffsetOverflow,
    RequiredAncestorKind {
        actual: TagKind,
    },
    RequiredAncestorMissing {
        required: ContentTag,
        observed_head: Option<ContentTag>,
    },
    FileLengthChanged {
        expected: u64,
        actual: u64,
    },
    AppendCommitUncertain {
        outcome: UncertainAppend,
        source: io::Error,
    },
    AppendPublicationFailed {
        candidate_record_tag: ContentTag,
        source: StorageError,
    },
    TailDiscardUncertain {
        valid_prefix_bytes: u64,
        rejected_tail_tag: ContentTag,
        phase: TailDiscardPhase,
        source: io::Error,
    },
    TailDiscardVerificationFailed,
    TailDiscardForbidden {
        rejection_class: RejectionClass,
    },
    RejectedTailChanged {
        expected: ContentTag,
        actual: ContentTag,
    },
    Poisoned,
}

impl Journal {
    /// Creates a new empty journal. An existing path is never opened or replaced.
    pub fn create_new<P: AsRef<Path>>(
        path: P,
        limits: StorageLimits,
    ) -> Result<(Self, CreateReceipt), JournalError> {
        let mut file = open_exclusive(path.as_ref(), true)?;
        validate_regular_file(&file)?;
        file.sync_all().map_err(|source| JournalError::Io {
            phase: JournalIoPhase::SyncNewFile,
            source,
        })?;
        file.seek(SeekFrom::Start(0))
            .map_err(|source| JournalError::Io {
                phase: JournalIoPhase::Seek,
                source,
            })?;
        Ok((
            Self {
                file,
                chain: StorageChain::with_limits(limits),
                valid_bytes: 0,
                poisoned: false,
            },
            CreateReceipt {
                file_bytes: 0,
                sync_evidence: SyncEvidence::OsSyncAllReturnedOk,
            },
        ))
    }

    /// Opens an existing path only. Missing paths are never created.
    pub fn open_existing<P: AsRef<Path>>(
        path: P,
        limits: StorageLimits,
        anchor: AnchorPolicy,
    ) -> Result<OpenJournal, JournalError> {
        anchor.validate()?;
        let mut file = open_exclusive(path.as_ref(), false)?;
        validate_regular_file(&file)?;
        let bytes = read_bounded_file(&mut file, limits)?;
        let inspected = inspect(&bytes, limits, anchor)?;

        match inspected.scan {
            ByteScan::Clean(report) => {
                require_anchor_if_clean(anchor, &report)?;
                let valid_bytes = report.valid_prefix_bytes;
                Ok(OpenJournal::Ready {
                    journal: Self {
                        file,
                        chain: inspected.chain,
                        valid_bytes,
                        poisoned: false,
                    },
                    report,
                })
            }
            ByteScan::NeedsRecovery(report) => Ok(OpenJournal::NeedsRecovery {
                journal: Box::new(RecoveryJournal {
                    file,
                    chain: inspected.chain,
                    report,
                    anchor,
                }),
            }),
        }
    }

    pub fn head(&self) -> Option<ContentTag> {
        self.chain.head()
    }

    pub fn records(&self) -> &[StorageRecord] {
        self.chain.records()
    }

    pub const fn valid_bytes(&self) -> u64 {
        self.valid_bytes
    }

    pub const fn is_poisoned(&self) -> bool {
        self.poisoned
    }

    /// Appends one record, synchronizes content and metadata, then publishes its head.
    pub fn append_synced(
        &mut self,
        object_kind: TagKind,
        object_schema: &[u8],
        payload: &[u8],
    ) -> Result<AppendSyncReceipt, JournalError> {
        append_synced_with_io(
            &mut self.file,
            &mut self.chain,
            &mut self.valid_bytes,
            &mut self.poisoned,
            object_kind,
            object_schema,
            payload,
        )
    }
}

trait JournalIo {
    fn metadata_len(&self) -> io::Result<u64>;
    fn seek_start(&mut self, offset: u64) -> io::Result<()>;
    fn write_some(&mut self, bytes: &[u8]) -> io::Result<usize>;
    fn sync_everything(&mut self) -> io::Result<()>;
    fn read_exact_bytes(&mut self, bytes: &mut [u8]) -> io::Result<()>;
    fn set_length(&mut self, length: u64) -> io::Result<()>;
}

impl JournalIo for File {
    fn metadata_len(&self) -> io::Result<u64> {
        self.metadata().map(|metadata| metadata.len())
    }

    fn seek_start(&mut self, offset: u64) -> io::Result<()> {
        self.seek(SeekFrom::Start(offset)).map(|_| ())
    }

    fn write_some(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.write(bytes)
    }

    fn sync_everything(&mut self) -> io::Result<()> {
        self.sync_all()
    }

    fn read_exact_bytes(&mut self, bytes: &mut [u8]) -> io::Result<()> {
        self.read_exact(bytes)
    }

    fn set_length(&mut self, length: u64) -> io::Result<()> {
        self.set_len(length)
    }
}

#[allow(clippy::too_many_arguments)]
fn append_synced_with_io<I: JournalIo>(
    io: &mut I,
    chain: &mut StorageChain,
    valid_bytes: &mut u64,
    poisoned: &mut bool,
    object_kind: TagKind,
    object_schema: &[u8],
    payload: &[u8],
) -> Result<AppendSyncReceipt, JournalError> {
    if *poisoned {
        return Err(JournalError::Poisoned);
    }

    let parent = chain.head();
    let candidate = StorageRecord::new_with_limits(
        parent,
        object_kind,
        object_schema,
        payload,
        chain.limits(),
    )?;
    preflight_chain_append(chain, &candidate)?;
    let encoded = candidate.encode();
    let mut reread = Vec::new();
    reread
        .try_reserve_exact(encoded.len())
        .map_err(|_| JournalError::AllocationFailed {
            requested: encoded.len(),
        })?;
    reread.resize(encoded.len(), 0);
    let encoded_bytes = u64::try_from(encoded.len()).map_err(|_| JournalError::OffsetOverflow)?;
    let expected_end = valid_bytes
        .checked_add(encoded_bytes)
        .ok_or(JournalError::OffsetOverflow)?;

    let observed = match io.metadata_len() {
        Ok(length) => length,
        Err(source) => {
            *poisoned = true;
            return Err(JournalError::Io {
                phase: JournalIoPhase::Metadata,
                source,
            });
        }
    };
    if observed != *valid_bytes {
        *poisoned = true;
        return Err(JournalError::FileLengthChanged {
            expected: *valid_bytes,
            actual: observed,
        });
    }
    if let Err(source) = io.seek_start(*valid_bytes) {
        *poisoned = true;
        return Err(JournalError::Io {
            phase: JournalIoPhase::Seek,
            source,
        });
    }

    let mut accepted = 0_usize;
    while accepted < encoded.len() {
        match io.write_some(&encoded[accepted..]) {
            Ok(0) => {
                let source =
                    io::Error::new(io::ErrorKind::WriteZero, "journal write returned zero");
                *poisoned = true;
                return Err(uncertain_append(
                    AppendPhase::Write,
                    candidate.record_tag(),
                    *valid_bytes,
                    expected_end,
                    accepted,
                    source,
                ));
            }
            Ok(count) if count <= encoded.len() - accepted => accepted += count,
            Ok(count) => {
                *poisoned = true;
                return Err(uncertain_append(
                    AppendPhase::Write,
                    candidate.record_tag(),
                    *valid_bytes,
                    expected_end,
                    accepted,
                    io::Error::other(format!("journal writer accepted impossible count {count}")),
                ));
            }
            Err(source) if source.kind() == io::ErrorKind::Interrupted => continue,
            Err(source) => {
                *poisoned = true;
                return Err(uncertain_append(
                    AppendPhase::Write,
                    candidate.record_tag(),
                    *valid_bytes,
                    expected_end,
                    accepted,
                    source,
                ));
            }
        }
    }

    if let Err(source) = io.sync_everything() {
        *poisoned = true;
        return Err(uncertain_append(
            AppendPhase::SyncAll,
            candidate.record_tag(),
            *valid_bytes,
            expected_end,
            accepted,
            source,
        ));
    }

    let observed_end = match io.metadata_len() {
        Ok(length) => length,
        Err(source) => {
            *poisoned = true;
            return Err(uncertain_append(
                AppendPhase::VerifyEnd,
                candidate.record_tag(),
                *valid_bytes,
                expected_end,
                accepted,
                source,
            ));
        }
    };
    if observed_end != expected_end {
        *poisoned = true;
        return Err(uncertain_append(
            AppendPhase::VerifyEnd,
            candidate.record_tag(),
            *valid_bytes,
            expected_end,
            accepted,
            io::Error::other(format!(
                "expected journal end {expected_end}, observed {observed_end}"
            )),
        ));
    }

    if let Err(source) = io.seek_start(*valid_bytes) {
        *poisoned = true;
        return Err(uncertain_append(
            AppendPhase::VerifyContent,
            candidate.record_tag(),
            *valid_bytes,
            expected_end,
            accepted,
            source,
        ));
    }
    if let Err(source) = io.read_exact_bytes(&mut reread) {
        *poisoned = true;
        return Err(uncertain_append(
            AppendPhase::VerifyContent,
            candidate.record_tag(),
            *valid_bytes,
            expected_end,
            accepted,
            source,
        ));
    }
    if reread != encoded {
        *poisoned = true;
        return Err(uncertain_append(
            AppendPhase::VerifyContent,
            candidate.record_tag(),
            *valid_bytes,
            expected_end,
            accepted,
            io::Error::other("OS-visible appended bytes differ from the candidate record"),
        ));
    }

    let receipt = AppendSyncReceipt {
        parent,
        object_tag: candidate.object_tag(),
        record_tag: candidate.record_tag(),
        start_offset: *valid_bytes,
        end_offset: expected_end,
        encoded_bytes,
        sync_evidence: SyncEvidence::OsSyncAllReturnedOk,
        appended_bytes_evidence: AppendedBytesEvidence::OsVisibleBytesRereadEqual,
    };
    if let Err(source) = chain.admit(candidate) {
        *poisoned = true;
        return Err(JournalError::AppendPublicationFailed {
            candidate_record_tag: receipt.record_tag,
            source,
        });
    }
    *valid_bytes = expected_end;
    Ok(receipt)
}

impl RecoveryJournal {
    pub const fn report(&self) -> &RecoveryReport {
        &self.report
    }

    /// Returns exactly the canonical records admitted before the refused suffix.
    ///
    /// This is a read-only view of the opening scan's physical prefix. Bytes in the rejected
    /// suffix are never decoded into or exposed through this slice.
    pub fn verified_prefix_records(&self) -> &[StorageRecord] {
        self.chain.records()
    }

    /// Reads the exact refused suffix while retaining the exclusive handle and rechecks its tag.
    pub fn export_rejected_tail(&mut self) -> Result<Vec<u8>, JournalError> {
        let observed = file_len(&self.file)?;
        if observed != self.report.file_bytes {
            return Err(JournalError::FileLengthChanged {
                expected: self.report.file_bytes,
                actual: observed,
            });
        }
        let requested = usize::try_from(self.report.rejected_tail_bytes)
            .map_err(|_| JournalError::OffsetOverflow)?;
        let mut tail = Vec::new();
        tail.try_reserve_exact(requested)
            .map_err(|_| JournalError::AllocationFailed { requested })?;
        tail.resize(requested, 0);
        self.file
            .seek(SeekFrom::Start(self.report.valid_prefix_bytes))
            .map_err(|source| JournalError::Io {
                phase: JournalIoPhase::Seek,
                source,
            })?;
        self.file
            .read_exact(&mut tail)
            .map_err(|source| JournalError::Io {
                phase: JournalIoPhase::Read,
                source,
            })?;
        let actual = rejected_tail_tag(
            self.report.valid_prefix_bytes,
            self.report.file_bytes,
            &tail,
        )?;
        if actual != self.report.rejected_tail_tag {
            return Err(JournalError::RejectedTailChanged {
                expected: self.report.rejected_tail_tag,
                actual,
            });
        }
        Ok(tail)
    }

    /// Explicitly discards the refused suffix. No open or append path calls this automatically.
    pub fn discard_unverified_tail(
        mut self,
    ) -> Result<(Journal, TailDiscardReceipt), JournalError> {
        let (chain, valid_bytes, receipt) =
            discard_tail_with_io(&mut self.file, &self.chain, &self.report, self.anchor)?;
        Ok((
            Journal {
                file: self.file,
                chain,
                valid_bytes,
                poisoned: false,
            },
            receipt,
        ))
    }
}

fn discard_tail_with_io<I: JournalIo>(
    io: &mut I,
    original_chain: &StorageChain,
    report: &RecoveryReport,
    anchor: AnchorPolicy,
) -> Result<(StorageChain, u64, TailDiscardReceipt), JournalError> {
    if report.rejection_class != RejectionClass::StructuralOrAuthentication {
        return Err(JournalError::TailDiscardForbidden {
            rejection_class: report.rejection_class,
        });
    }
    if let Some(required) = anchor.required()
        && !report.required_ancestor_seen
    {
        return Err(JournalError::RequiredAncestorMissing {
            required,
            observed_head: report.valid_head,
        });
    }

    let requested =
        usize::try_from(report.valid_prefix_bytes).map_err(|_| JournalError::OffsetOverflow)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(requested)
        .map_err(|_| JournalError::AllocationFailed { requested })?;
    bytes.resize(requested, 0);

    let observed = io.metadata_len().map_err(|source| JournalError::Io {
        phase: JournalIoPhase::Metadata,
        source,
    })?;
    if observed != report.file_bytes {
        return Err(JournalError::FileLengthChanged {
            expected: report.file_bytes,
            actual: observed,
        });
    }
    if let Err(source) = io.set_length(report.valid_prefix_bytes) {
        return Err(JournalError::TailDiscardUncertain {
            valid_prefix_bytes: report.valid_prefix_bytes,
            rejected_tail_tag: report.rejected_tail_tag,
            phase: TailDiscardPhase::SetLength,
            source,
        });
    }
    if let Err(source) = io.sync_everything() {
        return Err(JournalError::TailDiscardUncertain {
            valid_prefix_bytes: report.valid_prefix_bytes,
            rejected_tail_tag: report.rejected_tail_tag,
            phase: TailDiscardPhase::SyncAll,
            source,
        });
    }

    let observed = io
        .metadata_len()
        .map_err(|source| JournalError::TailDiscardUncertain {
            valid_prefix_bytes: report.valid_prefix_bytes,
            rejected_tail_tag: report.rejected_tail_tag,
            phase: TailDiscardPhase::VerifyPrefix,
            source,
        })?;
    if observed != report.valid_prefix_bytes {
        return Err(JournalError::TailDiscardUncertain {
            valid_prefix_bytes: report.valid_prefix_bytes,
            rejected_tail_tag: report.rejected_tail_tag,
            phase: TailDiscardPhase::VerifyPrefix,
            source: io::Error::other(format!(
                "expected retained prefix {}, observed {observed}",
                report.valid_prefix_bytes
            )),
        });
    }
    io.seek_start(0)
        .map_err(|source| JournalError::TailDiscardUncertain {
            valid_prefix_bytes: report.valid_prefix_bytes,
            rejected_tail_tag: report.rejected_tail_tag,
            phase: TailDiscardPhase::VerifyPrefix,
            source,
        })?;
    io.read_exact_bytes(&mut bytes)
        .map_err(|source| JournalError::TailDiscardUncertain {
            valid_prefix_bytes: report.valid_prefix_bytes,
            rejected_tail_tag: report.rejected_tail_tag,
            phase: TailDiscardPhase::VerifyPrefix,
            source,
        })?;

    let inspected = inspect(&bytes, original_chain.limits(), anchor)?;
    let ByteScan::Clean(clean) = inspected.scan else {
        return Err(JournalError::TailDiscardVerificationFailed);
    };
    if clean.valid_prefix_bytes != report.valid_prefix_bytes
        || clean.valid_record_count != report.valid_record_count
        || clean.valid_head != report.valid_head
        || inspected.chain != *original_chain
    {
        return Err(JournalError::TailDiscardVerificationFailed);
    }

    let receipt = TailDiscardReceipt {
        prior_file_bytes: report.file_bytes,
        retained_prefix_bytes: report.valid_prefix_bytes,
        discarded_bytes: report.rejected_tail_bytes,
        retained_head: report.valid_head,
        rejected_tail_tag: report.rejected_tail_tag,
        first_error: report.first_error.clone(),
        sync_evidence: SyncEvidence::OsSyncAllReturnedOk,
    };
    Ok((inspected.chain, clean.valid_prefix_bytes, receipt))
}

/// Scans already available bytes with the exact same admission logic as filesystem opening.
pub fn inspect_bytes(
    bytes: &[u8],
    limits: StorageLimits,
    anchor: AnchorPolicy,
) -> Result<ByteScan, JournalError> {
    anchor.validate()?;
    let actual = u64::try_from(bytes.len()).map_err(|_| JournalError::OffsetOverflow)?;
    let max = u64::try_from(limits.max_stream_bytes()).unwrap_or(u64::MAX);
    if actual > max {
        return Err(JournalError::FileTooLarge { actual, max });
    }
    let inspected = inspect(bytes, limits, anchor)?;
    if let ByteScan::Clean(report) = &inspected.scan {
        require_anchor_if_clean(anchor, report)?;
    }
    Ok(inspected.scan)
}

struct Inspection {
    chain: StorageChain,
    scan: ByteScan,
}

fn inspect(
    bytes: &[u8],
    limits: StorageLimits,
    anchor: AnchorPolicy,
) -> Result<Inspection, JournalError> {
    let required = anchor.required();
    let mut required_seen = false;
    let mut first_error = None;
    let mut scanner = StorageScanner::with_limits(bytes, None, limits);
    for result in scanner.by_ref() {
        match result {
            Ok(record) => {
                if Some(record.record_tag()) == required {
                    required_seen = true;
                }
            }
            Err(error) => {
                first_error = Some(error);
                break;
            }
        }
    }

    let valid_prefix = scanner.offset();
    let chain = StorageChain::decode_with_limits(&bytes[..valid_prefix], limits)?;
    let valid_prefix_u64 = u64::try_from(valid_prefix).map_err(|_| JournalError::OffsetOverflow)?;
    let file_bytes = u64::try_from(bytes.len()).map_err(|_| JournalError::OffsetOverflow)?;
    let common = ScanReport {
        file_bytes,
        valid_prefix_bytes: valid_prefix_u64,
        valid_record_count: chain.records().len(),
        valid_head: chain.head(),
        required_ancestor_seen: required.is_none() || required_seen,
    };

    let scan = if let Some(first_error) = first_error {
        let tail = &bytes[valid_prefix..];
        let rejected_tail_bytes =
            u64::try_from(tail.len()).map_err(|_| JournalError::OffsetOverflow)?;
        let rejected_tail_tag = rejected_tail_tag(valid_prefix_u64, file_bytes, tail)?;
        ByteScan::NeedsRecovery(RecoveryReport {
            file_bytes,
            valid_prefix_bytes: valid_prefix_u64,
            rejected_tail_bytes,
            valid_record_count: common.valid_record_count,
            valid_head: common.valid_head,
            required_ancestor_seen: common.required_ancestor_seen,
            rejection_class: classify_rejection(&first_error),
            first_error,
            rejected_tail_tag,
        })
    } else {
        ByteScan::Clean(common)
    };
    Ok(Inspection { chain, scan })
}

fn require_anchor_if_clean(anchor: AnchorPolicy, report: &ScanReport) -> Result<(), JournalError> {
    if let Some(required) = anchor.required()
        && !report.required_ancestor_seen
    {
        return Err(JournalError::RequiredAncestorMissing {
            required,
            observed_head: report.valid_head,
        });
    }
    Ok(())
}

fn classify_rejection(error: &StorageError) -> RejectionClass {
    match error {
        StorageError::LimitsExceedProtocolCaps
        | StorageError::SchemaTooLarge { .. }
        | StorageError::PayloadTooLarge { .. }
        | StorageError::RecordTooLarge { .. }
        | StorageError::StreamTooLarge { .. }
        | StorageError::RecordCountLimit { .. } => RejectionClass::PolicyLimit,
        StorageError::UnsupportedVersion { .. } | StorageError::UnknownObjectKind => {
            RejectionClass::UnsupportedProtocol
        }
        _ => RejectionClass::StructuralOrAuthentication,
    }
}

fn rejected_tail_tag(
    valid_prefix_bytes: u64,
    file_bytes: u64,
    tail: &[u8],
) -> Result<ContentTag, JournalError> {
    let tail_len = u64::try_from(tail.len()).map_err(|_| JournalError::OffsetOverflow)?;
    let payload_len = 16_u64
        .checked_add(tail_len)
        .ok_or(JournalError::OffsetOverflow)?;
    hash_canonical(
        TagKind::JournalRejectedTail,
        JOURNAL_REJECTED_TAIL_SCHEMA,
        payload_len,
        |sink| {
            sink(&valid_prefix_bytes.to_be_bytes());
            sink(&file_bytes.to_be_bytes());
            sink(tail)
        },
    )
    .map_err(StorageError::from)
    .map_err(JournalError::from)
}

fn uncertain_append(
    phase: AppendPhase,
    candidate_record_tag: ContentTag,
    start_offset: u64,
    expected_end_offset: u64,
    accepted_bytes: usize,
    source: io::Error,
) -> JournalError {
    JournalError::AppendCommitUncertain {
        outcome: UncertainAppend {
            phase,
            candidate_record_tag,
            start_offset,
            expected_end_offset,
            accepted_bytes: u64::try_from(accepted_bytes).unwrap_or(u64::MAX),
        },
        source,
    }
}

fn preflight_chain_append(
    chain: &StorageChain,
    candidate: &StorageRecord,
) -> Result<(), StorageError> {
    let limits = chain.limits();
    if chain.records().len() >= limits.max_records() {
        return Err(StorageError::RecordCountLimit {
            max: limits.max_records(),
        });
    }
    let next_bytes = chain
        .encoded_bytes()
        .checked_add(candidate.encoded_len())
        .ok_or(StorageError::LengthOverflow)?;
    if next_bytes > limits.max_stream_bytes() {
        return Err(StorageError::StreamTooLarge {
            actual: next_bytes,
            max: limits.max_stream_bytes(),
        });
    }
    Ok(())
}

fn validate_regular_file(file: &File) -> Result<(), JournalError> {
    let metadata = file.metadata().map_err(|source| JournalError::Io {
        phase: JournalIoPhase::Metadata,
        source,
    })?;
    if !metadata.is_file() {
        return Err(JournalError::NotRegularFile);
    }
    Ok(())
}

fn file_len(file: &File) -> Result<u64, JournalError> {
    file.metadata()
        .map(|metadata| metadata.len())
        .map_err(|source| JournalError::Io {
            phase: JournalIoPhase::Metadata,
            source,
        })
}

fn read_bounded_file(file: &mut File, limits: StorageLimits) -> Result<Vec<u8>, JournalError> {
    let measured = file_len(file)?;
    let max = u64::try_from(limits.max_stream_bytes()).unwrap_or(u64::MAX);
    if measured > max {
        return Err(JournalError::FileTooLarge {
            actual: measured,
            max,
        });
    }
    let requested = usize::try_from(measured).map_err(|_| JournalError::OffsetOverflow)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(requested)
        .map_err(|_| JournalError::AllocationFailed { requested })?;
    bytes.resize(requested, 0);
    file.seek(SeekFrom::Start(0))
        .map_err(|source| JournalError::Io {
            phase: JournalIoPhase::Seek,
            source,
        })?;
    file.read_exact(&mut bytes)
        .map_err(|source| JournalError::Io {
            phase: JournalIoPhase::Read,
            source,
        })?;
    let actual = file_len(file)?;
    if actual != measured {
        return Err(JournalError::FileLengthChanged {
            expected: measured,
            actual,
        });
    }
    Ok(bytes)
}

#[cfg(windows)]
fn open_exclusive(path: &Path, create_new: bool) -> Result<File, JournalError> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).share_mode(0);
    let phase = if create_new {
        options.create_new(true);
        JournalIoPhase::CreateNew
    } else {
        JournalIoPhase::OpenExisting
    };
    options
        .open(path)
        .map_err(|source| JournalError::Io { phase, source })
}

#[cfg(not(windows))]
fn open_exclusive(_path: &Path, _create_new: bool) -> Result<File, JournalError> {
    Err(JournalError::UnsupportedExclusivePlatform)
}

impl fmt::Display for JournalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => write!(formatter, "journal storage error: {error}"),
            Self::Io { phase, source } => {
                write!(formatter, "journal {phase:?} I/O failed: {source}")
            }
            Self::UnsupportedExclusivePlatform => {
                formatter.write_str("exclusive journal custody is implemented only on Windows")
            }
            Self::NotRegularFile => formatter.write_str("journal handle is not a regular file"),
            Self::FileTooLarge { actual, max } => {
                write!(formatter, "journal has {actual} bytes; read cap is {max}")
            }
            Self::AllocationFailed { requested } => {
                write!(formatter, "could not allocate {requested} journal bytes")
            }
            Self::OffsetOverflow => formatter.write_str("journal offset is not representable"),
            Self::RequiredAncestorKind { actual } => write!(
                formatter,
                "required ancestor must be storage-record, received {}",
                actual.name()
            ),
            Self::RequiredAncestorMissing {
                required,
                observed_head,
            } => write!(
                formatter,
                "required ancestor {required} is absent; observed head is {observed_head:?}"
            ),
            Self::FileLengthChanged { expected, actual } => write!(
                formatter,
                "journal length changed: expected {expected}, observed {actual}"
            ),
            Self::AppendCommitUncertain { outcome, source } => write!(
                formatter,
                "journal append is uncertain at {:?} after {} accepted bytes: {source}",
                outcome.phase, outcome.accepted_bytes
            ),
            Self::AppendPublicationFailed {
                candidate_record_tag,
                source,
            } => write!(
                formatter,
                "synced journal record {candidate_record_tag} could not publish locally: {source}"
            ),
            Self::TailDiscardUncertain { phase, source, .. } => {
                write!(
                    formatter,
                    "journal tail discard is uncertain at {phase:?}: {source}"
                )
            }
            Self::TailDiscardVerificationFailed => {
                formatter.write_str("discarded journal prefix did not reverify exactly")
            }
            Self::TailDiscardForbidden { rejection_class } => write!(
                formatter,
                "journal tail classified {rejection_class:?} cannot be discarded by this API"
            ),
            Self::RejectedTailChanged { expected, actual } => write!(
                formatter,
                "rejected journal tail changed: expected {expected}, observed {actual}"
            ),
            Self::Poisoned => {
                formatter.write_str("journal handle is poisoned; close and reopen it")
            }
        }
    }
}

impl std::error::Error for JournalError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            Self::Io { source, .. }
            | Self::AppendCommitUncertain { source, .. }
            | Self::TailDiscardUncertain { source, .. } => Some(source),
            Self::AppendPublicationFailed { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<StorageError> for JournalError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    const OBJECT_SCHEMA: &[u8] = b"bitblock/journal-test-object/v1";

    struct FaultIo {
        bytes: Vec<u8>,
        cursor: usize,
        appended: usize,
        max_write: usize,
        fail_write_at: Option<usize>,
        fail_sync: bool,
        fail_set_length: bool,
        corrupt_on_sync: bool,
        fail_read: bool,
        metadata_calls: Cell<usize>,
        fail_metadata_call: Option<usize>,
        wrong_length_call: Option<usize>,
    }

    impl FaultIo {
        fn new(bytes: Vec<u8>) -> Self {
            Self {
                bytes,
                cursor: 0,
                appended: 0,
                max_write: usize::MAX,
                fail_write_at: None,
                fail_sync: false,
                fail_set_length: false,
                corrupt_on_sync: false,
                fail_read: false,
                metadata_calls: Cell::new(0),
                fail_metadata_call: None,
                wrong_length_call: None,
            }
        }
    }

    impl JournalIo for FaultIo {
        fn metadata_len(&self) -> io::Result<u64> {
            let call = self.metadata_calls.get() + 1;
            self.metadata_calls.set(call);
            if self.fail_metadata_call == Some(call) {
                return Err(io::Error::other("powered metadata failure"));
            }
            let mut length = self.bytes.len() as u64;
            if self.wrong_length_call == Some(call) {
                length = length.saturating_add(1);
            }
            Ok(length)
        }

        fn seek_start(&mut self, offset: u64) -> io::Result<()> {
            self.cursor =
                usize::try_from(offset).map_err(|_| io::Error::other("powered seek overflow"))?;
            Ok(())
        }

        fn write_some(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.fail_write_at == Some(self.appended) {
                return Err(io::Error::other("powered write failure"));
            }
            let before_failure = self
                .fail_write_at
                .map_or(usize::MAX, |cut| cut.saturating_sub(self.appended));
            let count = bytes.len().min(self.max_write).min(before_failure);
            if count == 0 {
                return Err(io::Error::other("powered zero write"));
            }
            let end = self
                .cursor
                .checked_add(count)
                .ok_or_else(|| io::Error::other("powered write overflow"))?;
            if end > self.bytes.len() {
                self.bytes.resize(end, 0);
            }
            self.bytes[self.cursor..end].copy_from_slice(&bytes[..count]);
            self.cursor = end;
            self.appended += count;
            Ok(count)
        }

        fn sync_everything(&mut self) -> io::Result<()> {
            if self.fail_sync {
                return Err(io::Error::other("powered sync failure"));
            }
            if self.corrupt_on_sync {
                *self.bytes.last_mut().expect("append produced bytes") ^= 1;
            }
            Ok(())
        }

        fn read_exact_bytes(&mut self, bytes: &mut [u8]) -> io::Result<()> {
            if self.fail_read {
                return Err(io::Error::other("powered reread failure"));
            }
            let end = self
                .cursor
                .checked_add(bytes.len())
                .ok_or_else(|| io::Error::other("powered read overflow"))?;
            if end > self.bytes.len() {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "powered short reread",
                ));
            }
            bytes.copy_from_slice(&self.bytes[self.cursor..end]);
            self.cursor = end;
            Ok(())
        }

        fn set_length(&mut self, length: u64) -> io::Result<()> {
            if self.fail_set_length {
                return Err(io::Error::other("powered set-length failure"));
            }
            let length = usize::try_from(length)
                .map_err(|_| io::Error::other("powered truncate overflow"))?;
            self.bytes.resize(length, 0);
            if self.cursor > length {
                self.cursor = length;
            }
            Ok(())
        }
    }

    fn chain(payloads: &[&[u8]]) -> StorageChain {
        let mut chain = StorageChain::new();
        for payload in payloads {
            chain
                .append(TagKind::World, OBJECT_SCHEMA, payload)
                .unwrap();
        }
        chain
    }

    fn clean(scan: ByteScan) -> ScanReport {
        let ByteScan::Clean(report) = scan else {
            panic!("expected a clean scan")
        };
        report
    }

    fn recovery(scan: ByteScan) -> RecoveryReport {
        let ByteScan::NeedsRecovery(report) = scan else {
            panic!("expected recovery")
        };
        report
    }

    #[test]
    fn every_append_cut_has_one_exact_verified_prefix() {
        let base = chain(&[b"base-zero", b"base-one"]);
        let base_bytes = base.encode();
        let candidate =
            StorageRecord::new(base.head(), TagKind::World, OBJECT_SCHEMA, b"candidate").unwrap();
        let candidate_bytes = candidate.encode();

        for cut in 0..=candidate_bytes.len() {
            let mut physical = base_bytes.clone();
            physical.extend_from_slice(&candidate_bytes[..cut]);
            let scan = inspect_bytes(
                &physical,
                StorageLimits::DEFAULT,
                AnchorPolicy::UnanchoredLocal,
            )
            .unwrap();
            if cut == 0 {
                let report = clean(scan);
                assert_eq!(report.valid_prefix_bytes, base_bytes.len() as u64);
                assert_eq!(report.valid_head, base.head());
            } else if cut == candidate_bytes.len() {
                let report = clean(scan);
                assert_eq!(report.valid_prefix_bytes, physical.len() as u64);
                assert_eq!(report.valid_head, Some(candidate.record_tag()));
            } else {
                let report = recovery(scan);
                assert_eq!(report.valid_prefix_bytes, base_bytes.len() as u64);
                assert_eq!(report.valid_head, base.head());
                assert_eq!(report.rejected_tail_bytes, cut as u64);
                assert_eq!(
                    report.rejection_class,
                    RejectionClass::StructuralOrAuthentication
                );
                assert_eq!(report.first_error, StorageError::Truncated);
            }
        }
    }

    #[test]
    fn recovery_handle_exposes_exactly_the_scanner_admitted_prefix() {
        let base = chain(&[b"base-zero", b"base-one"]);
        let base_bytes = base.encode();
        let rejected =
            StorageRecord::new(base.head(), TagKind::World, OBJECT_SCHEMA, b"rejected").unwrap();
        let rejected_bytes = rejected.encode();
        let cut = rejected_bytes.len() / 2;
        let mut physical = base_bytes.clone();
        physical.extend_from_slice(&rejected_bytes[..cut]);

        let inspected = inspect(
            &physical,
            StorageLimits::DEFAULT,
            AnchorPolicy::UnanchoredLocal,
        )
        .unwrap();
        let ByteScan::NeedsRecovery(report) = inspected.scan else {
            panic!("partial record was admitted")
        };
        let journal = RecoveryJournal {
            file: File::open(std::env::current_exe().unwrap()).unwrap(),
            chain: inspected.chain,
            report,
            anchor: AnchorPolicy::UnanchoredLocal,
        };

        let exposed = journal.verified_prefix_records();
        assert_eq!(exposed, base.records());
        assert_eq!(exposed.len(), journal.report().valid_record_count);
        assert_eq!(
            exposed
                .iter()
                .map(StorageRecord::encoded_len)
                .sum::<usize>() as u64,
            journal.report().valid_prefix_bytes
        );
        assert_eq!(
            exposed
                .iter()
                .flat_map(StorageRecord::encode)
                .collect::<Vec<_>>(),
            physical[..journal.report().valid_prefix_bytes as usize]
        );
        assert!(
            exposed
                .iter()
                .all(|record| record.record_tag() != rejected.record_tag())
        );
        assert_eq!(
            journal.report().rejected_tail_bytes,
            (physical.len() - base_bytes.len()) as u64
        );
    }

    #[test]
    fn every_write_cut_and_postwrite_failure_keeps_head_unpublished_and_poisoned() {
        let base = chain(&[b"base"]);
        let base_bytes = base.encode();
        let base_head = base.head();
        let base_len = base_bytes.len() as u64;
        let candidate =
            StorageRecord::new(base_head, TagKind::World, OBJECT_SCHEMA, b"candidate").unwrap();
        let candidate_len = candidate.encoded_len();

        for cut in 0..candidate_len {
            let mut io = FaultIo::new(base_bytes.clone());
            io.fail_write_at = Some(cut);
            let mut current = base.clone();
            let mut valid_bytes = base_len;
            let mut poisoned = false;
            let result = append_synced_with_io(
                &mut io,
                &mut current,
                &mut valid_bytes,
                &mut poisoned,
                TagKind::World,
                OBJECT_SCHEMA,
                b"candidate",
            );
            let Err(JournalError::AppendCommitUncertain { outcome, .. }) = result else {
                panic!("write cut {cut} did not produce an uncertain append")
            };
            assert_eq!(outcome.phase, AppendPhase::Write);
            assert_eq!(outcome.accepted_bytes, cut as u64);
            assert_eq!(current.head(), base_head);
            assert_eq!(valid_bytes, base_len);
            assert!(poisoned);
            assert_eq!(io.bytes.len(), base_bytes.len() + cut);
            assert!(matches!(
                append_synced_with_io(
                    &mut io,
                    &mut current,
                    &mut valid_bytes,
                    &mut poisoned,
                    TagKind::World,
                    OBJECT_SCHEMA,
                    b"retry",
                ),
                Err(JournalError::Poisoned)
            ));
        }

        for expected_phase in [
            AppendPhase::SyncAll,
            AppendPhase::VerifyEnd,
            AppendPhase::VerifyContent,
        ] {
            let mut io = FaultIo::new(base_bytes.clone());
            match expected_phase {
                AppendPhase::SyncAll => io.fail_sync = true,
                AppendPhase::VerifyEnd => io.fail_metadata_call = Some(2),
                AppendPhase::VerifyContent => io.fail_read = true,
                AppendPhase::Write => unreachable!(),
            }
            let mut current = base.clone();
            let mut valid_bytes = base_len;
            let mut poisoned = false;
            let result = append_synced_with_io(
                &mut io,
                &mut current,
                &mut valid_bytes,
                &mut poisoned,
                TagKind::World,
                OBJECT_SCHEMA,
                b"candidate",
            );
            let Err(JournalError::AppendCommitUncertain { outcome, .. }) = result else {
                panic!("{expected_phase:?} failure published an append")
            };
            assert_eq!(outcome.phase, expected_phase);
            assert_eq!(outcome.accepted_bytes, candidate_len as u64);
            assert_eq!(current.head(), base_head);
            assert_eq!(valid_bytes, base_len);
            assert!(poisoned);
        }
    }

    #[test]
    fn short_writes_complete_but_same_length_content_substitution_is_refused() {
        let base = chain(&[b"base"]);
        let base_bytes = base.encode();
        let base_len = base_bytes.len() as u64;

        let mut short_io = FaultIo::new(base_bytes.clone());
        short_io.max_write = 1;
        let mut short_chain = base.clone();
        let mut short_valid = base_len;
        let mut short_poisoned = false;
        let receipt = append_synced_with_io(
            &mut short_io,
            &mut short_chain,
            &mut short_valid,
            &mut short_poisoned,
            TagKind::World,
            OBJECT_SCHEMA,
            b"candidate",
        )
        .unwrap();
        assert_eq!(short_chain.head(), Some(receipt.record_tag));
        assert_eq!(short_valid, receipt.end_offset);
        assert!(!short_poisoned);
        assert_eq!(short_io.bytes, short_chain.encode());

        let mut corrupt_io = FaultIo::new(base_bytes);
        corrupt_io.corrupt_on_sync = true;
        let mut corrupt_chain = base.clone();
        let mut corrupt_valid = base_len;
        let mut corrupt_poisoned = false;
        let result = append_synced_with_io(
            &mut corrupt_io,
            &mut corrupt_chain,
            &mut corrupt_valid,
            &mut corrupt_poisoned,
            TagKind::World,
            OBJECT_SCHEMA,
            b"candidate",
        );
        assert!(matches!(
            result,
            Err(JournalError::AppendCommitUncertain {
                outcome: UncertainAppend {
                    phase: AppendPhase::VerifyContent,
                    ..
                },
                ..
            })
        ));
        assert_eq!(corrupt_chain, base);
        assert_eq!(corrupt_valid, base_len);
        assert!(corrupt_poisoned);
    }

    #[test]
    fn every_tail_discard_failpoint_refuses_a_writable_result() {
        let base = chain(&[b"base"]);
        let base_bytes = base.encode();
        let mut physical = base_bytes.clone();
        physical.extend_from_slice(b"partial-record");
        let report = recovery(
            inspect_bytes(
                &physical,
                StorageLimits::DEFAULT,
                AnchorPolicy::UnanchoredLocal,
            )
            .unwrap(),
        );

        let mut set_len_failure = FaultIo::new(physical.clone());
        set_len_failure.fail_set_length = true;
        assert!(matches!(
            discard_tail_with_io(
                &mut set_len_failure,
                &base,
                &report,
                AnchorPolicy::UnanchoredLocal,
            ),
            Err(JournalError::TailDiscardUncertain {
                phase: TailDiscardPhase::SetLength,
                ..
            })
        ));
        assert_eq!(set_len_failure.bytes, physical);

        let mut sync_failure = FaultIo::new(physical.clone());
        sync_failure.fail_sync = true;
        assert!(matches!(
            discard_tail_with_io(
                &mut sync_failure,
                &base,
                &report,
                AnchorPolicy::UnanchoredLocal,
            ),
            Err(JournalError::TailDiscardUncertain {
                phase: TailDiscardPhase::SyncAll,
                ..
            })
        ));
        assert_eq!(sync_failure.bytes, base_bytes);

        let mut metadata_failure = FaultIo::new(physical.clone());
        metadata_failure.fail_metadata_call = Some(2);
        assert!(matches!(
            discard_tail_with_io(
                &mut metadata_failure,
                &base,
                &report,
                AnchorPolicy::UnanchoredLocal,
            ),
            Err(JournalError::TailDiscardUncertain {
                phase: TailDiscardPhase::VerifyPrefix,
                ..
            })
        ));

        let mut read_failure = FaultIo::new(physical.clone());
        read_failure.fail_read = true;
        assert!(matches!(
            discard_tail_with_io(
                &mut read_failure,
                &base,
                &report,
                AnchorPolicy::UnanchoredLocal,
            ),
            Err(JournalError::TailDiscardUncertain {
                phase: TailDiscardPhase::VerifyPrefix,
                ..
            })
        ));

        let mut substituted_prefix = FaultIo::new(physical);
        substituted_prefix.corrupt_on_sync = true;
        assert!(matches!(
            discard_tail_with_io(
                &mut substituted_prefix,
                &base,
                &report,
                AnchorPolicy::UnanchoredLocal,
            ),
            Err(JournalError::TailDiscardVerificationFailed)
        ));
    }

    #[test]
    fn torn_append_never_resynchronizes_to_later_valid_magic() {
        let base = chain(&[b"base"]);
        let base_bytes = base.encode();
        let candidate =
            StorageRecord::new(base.head(), TagKind::World, OBJECT_SCHEMA, b"candidate").unwrap();
        let candidate_bytes = candidate.encode();
        let later = StorageRecord::new(
            Some(candidate.record_tag()),
            TagKind::World,
            OBJECT_SCHEMA,
            b"later-valid-record",
        )
        .unwrap()
        .encode();

        for cut in 1..candidate_bytes.len() {
            let mut physical = base_bytes.clone();
            physical.extend_from_slice(&candidate_bytes[..cut]);
            physical.extend_from_slice(&later);
            let report = recovery(
                inspect_bytes(
                    &physical,
                    StorageLimits::DEFAULT,
                    AnchorPolicy::UnanchoredLocal,
                )
                .unwrap(),
            );
            assert_eq!(report.valid_prefix_bytes, base_bytes.len() as u64);
            assert_eq!(report.valid_head, base.head());
            assert_eq!(report.rejected_tail_bytes, (cut + later.len()) as u64);
        }
    }

    #[test]
    fn valid_boundary_rollback_is_local_clean_eof_but_anchor_detects_it() {
        let full = chain(&[b"zero", b"one", b"two"]);
        let required = full.records()[2].record_tag();
        let shortened = chain(&[b"zero", b"one"]);
        let shortened_bytes = shortened.encode();

        let local = clean(
            inspect_bytes(
                &shortened_bytes,
                StorageLimits::DEFAULT,
                AnchorPolicy::UnanchoredLocal,
            )
            .unwrap(),
        );
        assert_eq!(local.valid_head, shortened.head());

        assert!(matches!(
            inspect_bytes(
                &shortened_bytes,
                StorageLimits::DEFAULT,
                AnchorPolicy::RequireAncestor(required),
            ),
            Err(JournalError::RequiredAncestorMissing {
                required: missing,
                ..
            }) if missing == required
        ));
    }

    #[test]
    fn valid_alternate_chain_is_local_clean_but_foreign_anchor_detects_it() {
        let expected = chain(&[b"expected-genesis", b"expected-child"]);
        let alternate = chain(&[b"alternate-genesis", b"alternate-child"]);
        let alternate_bytes = alternate.encode();
        assert_eq!(
            clean(
                inspect_bytes(
                    &alternate_bytes,
                    StorageLimits::DEFAULT,
                    AnchorPolicy::UnanchoredLocal,
                )
                .unwrap()
            )
            .valid_head,
            alternate.head()
        );
        assert!(matches!(
            inspect_bytes(
                &alternate_bytes,
                StorageLimits::DEFAULT,
                AnchorPolicy::RequireAncestor(expected.head().unwrap()),
            ),
            Err(JournalError::RequiredAncestorMissing { .. })
        ));
    }

    #[test]
    fn rejected_tail_tag_binds_offsets_and_exact_suffix() {
        let base = chain(&[b"base"]);
        let mut physical = base.encode();
        physical.extend_from_slice(b"partial");
        let original = recovery(
            inspect_bytes(
                &physical,
                StorageLimits::DEFAULT,
                AnchorPolicy::UnanchoredLocal,
            )
            .unwrap(),
        );

        let mut changed = physical.clone();
        *changed.last_mut().unwrap() ^= 1;
        let changed = recovery(
            inspect_bytes(
                &changed,
                StorageLimits::DEFAULT,
                AnchorPolicy::UnanchoredLocal,
            )
            .unwrap(),
        );
        assert_ne!(original.rejected_tail_tag, changed.rejected_tail_tag);
        assert_eq!(
            original.rejected_tail_tag.kind(),
            TagKind::JournalRejectedTail
        );
    }

    #[test]
    fn oversized_byte_inspection_refuses_before_scanning_or_tail_hashing() {
        let default = StorageLimits::DEFAULT;
        let limits = StorageLimits::new(
            default.max_record_bytes(),
            default.max_schema_bytes(),
            default.max_payload_bytes(),
            default.max_records(),
            8,
        )
        .unwrap();
        assert!(matches!(
            inspect_bytes(&[0_u8; 9], limits, AnchorPolicy::UnanchoredLocal),
            Err(JournalError::FileTooLarge { actual: 9, max: 8 })
        ));
    }

    #[test]
    fn policy_limits_and_unsupported_protocol_are_not_corruption() {
        let source = chain(&[b"first", b"second"]);
        let default = StorageLimits::DEFAULT;
        let one_record = StorageLimits::new(
            default.max_record_bytes(),
            default.max_schema_bytes(),
            default.max_payload_bytes(),
            1,
            default.max_stream_bytes(),
        )
        .unwrap();
        let policy = recovery(
            inspect_bytes(&source.encode(), one_record, AnchorPolicy::UnanchoredLocal).unwrap(),
        );
        assert_eq!(policy.rejection_class, RejectionClass::PolicyLimit);
        assert_eq!(
            policy.first_error,
            StorageError::RecordCountLimit { max: 1 }
        );

        let mut unsupported = source.records()[0].encode();
        unsupported[9] = 2;
        let unsupported = recovery(
            inspect_bytes(
                &unsupported,
                StorageLimits::DEFAULT,
                AnchorPolicy::UnanchoredLocal,
            )
            .unwrap(),
        );
        assert_eq!(
            unsupported.rejection_class,
            RejectionClass::UnsupportedProtocol
        );
        assert_eq!(
            unsupported.first_error,
            StorageError::UnsupportedVersion { actual: 2 }
        );
    }

    #[cfg(windows)]
    mod windows {
        use super::*;
        use std::sync::atomic::{AtomicU64, Ordering};

        static NEXT_PATH: AtomicU64 = AtomicU64::new(0);

        struct TempJournalPath(std::path::PathBuf);

        impl TempJournalPath {
            fn new(label: &str) -> Self {
                let serial = NEXT_PATH.fetch_add(1, Ordering::Relaxed);
                Self(std::env::temp_dir().join(format!(
                    "bitblock-journal-{label}-{}-{serial}.bbj",
                    std::process::id()
                )))
            }
        }

        impl Drop for TempJournalPath {
            fn drop(&mut self) {
                match std::fs::remove_file(&self.0) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => panic!("could not remove journal test file: {error}"),
                }
            }
        }

        #[test]
        fn create_open_and_exclusive_share_contracts_are_distinct() {
            let path = TempJournalPath::new("open-contract");
            assert!(matches!(
                Journal::open_existing(
                    &path.0,
                    StorageLimits::DEFAULT,
                    AnchorPolicy::UnanchoredLocal
                ),
                Err(JournalError::Io {
                    phase: JournalIoPhase::OpenExisting,
                    ..
                })
            ));
            assert!(!path.0.exists());

            let (journal, receipt) = Journal::create_new(&path.0, StorageLimits::DEFAULT).unwrap();
            assert_eq!(receipt.file_bytes, 0);
            assert!(Journal::create_new(&path.0, StorageLimits::DEFAULT).is_err());
            assert!(OpenOptions::new().read(true).open(&path.0).is_err());
            drop(journal);

            assert!(OpenOptions::new().read(true).open(&path.0).is_ok());
            assert!(matches!(
                Journal::open_existing(
                    &path.0,
                    StorageLimits::DEFAULT,
                    AnchorPolicy::UnanchoredLocal
                )
                .unwrap(),
                OpenJournal::Ready { .. }
            ));
        }

        #[test]
        fn synced_append_reopens_with_the_published_head() {
            let path = TempJournalPath::new("append-reopen");
            let (mut journal, _) = Journal::create_new(&path.0, StorageLimits::DEFAULT).unwrap();
            let first = journal
                .append_synced(TagKind::World, OBJECT_SCHEMA, b"first")
                .unwrap();
            let second = journal
                .append_synced(TagKind::World, OBJECT_SCHEMA, b"second")
                .unwrap();
            assert_eq!(second.parent, Some(first.record_tag));
            assert_eq!(journal.head(), Some(second.record_tag));
            assert_eq!(journal.valid_bytes(), second.end_offset);
            drop(journal);

            let OpenJournal::Ready { journal, report } = Journal::open_existing(
                &path.0,
                StorageLimits::DEFAULT,
                AnchorPolicy::RequireAncestor(second.record_tag),
            )
            .unwrap() else {
                panic!("synced journal required recovery")
            };
            assert_eq!(report.valid_head, Some(second.record_tag));
            assert!(report.required_ancestor_seen);
            assert_eq!(journal.records().len(), 2);
        }

        #[test]
        fn corrupt_suffix_is_preserved_until_explicit_discard_then_reverified() {
            let path = TempJournalPath::new("explicit-discard");
            let (mut journal, _) = Journal::create_new(&path.0, StorageLimits::DEFAULT).unwrap();
            let first = journal
                .append_synced(TagKind::World, OBJECT_SCHEMA, b"first")
                .unwrap();
            let second = journal
                .append_synced(TagKind::World, OBJECT_SCHEMA, b"second")
                .unwrap();
            let original_len = second.end_offset;
            drop(journal);

            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path.0)
                .unwrap();
            file.seek(SeekFrom::Start(original_len - 1)).unwrap();
            let mut byte = [0_u8; 1];
            file.read_exact(&mut byte).unwrap();
            byte[0] ^= 1;
            file.seek(SeekFrom::Start(original_len - 1)).unwrap();
            file.write_all(&byte).unwrap();
            file.sync_all().unwrap();
            drop(file);
            let expected_tail =
                std::fs::read(&path.0).unwrap()[first.end_offset as usize..].to_vec();

            let OpenJournal::NeedsRecovery { journal } = Journal::open_existing(
                &path.0,
                StorageLimits::DEFAULT,
                AnchorPolicy::RequireAncestor(first.record_tag),
            )
            .unwrap() else {
                panic!("corrupt suffix was admitted")
            };
            let mut journal = *journal;
            let report = journal.report().clone();
            assert_eq!(std::fs::metadata(&path.0).unwrap().len(), original_len);
            assert_eq!(report.valid_head, Some(first.record_tag));
            assert_eq!(report.valid_prefix_bytes, first.end_offset);
            assert_eq!(report.rejected_tail_bytes, second.encoded_bytes);
            assert_eq!(journal.export_rejected_tail().unwrap(), expected_tail);

            let (journal, discard) = journal.discard_unverified_tail().unwrap();
            assert_eq!(discard.discarded_bytes, second.encoded_bytes);
            assert_eq!(journal.head(), Some(first.record_tag));
            assert_eq!(std::fs::metadata(&path.0).unwrap().len(), first.end_offset);
        }

        #[test]
        fn tighter_limits_report_but_cannot_discard_canonical_records() {
            let path = TempJournalPath::new("policy-tail");
            let (mut journal, _) = Journal::create_new(&path.0, StorageLimits::DEFAULT).unwrap();
            journal
                .append_synced(TagKind::World, OBJECT_SCHEMA, b"first")
                .unwrap();
            journal
                .append_synced(TagKind::World, OBJECT_SCHEMA, b"second")
                .unwrap();
            let expected_tail = journal.records()[1].encode();
            let expected_length = journal.valid_bytes();
            drop(journal);
            let expected_file = std::fs::read(&path.0).unwrap();

            let default = StorageLimits::DEFAULT;
            let one_record = StorageLimits::new(
                default.max_record_bytes(),
                default.max_schema_bytes(),
                default.max_payload_bytes(),
                1,
                default.max_stream_bytes(),
            )
            .unwrap();
            let OpenJournal::NeedsRecovery { journal } =
                Journal::open_existing(&path.0, one_record, AnchorPolicy::UnanchoredLocal).unwrap()
            else {
                panic!("record-count cap was ignored")
            };
            let mut journal = *journal;
            assert_eq!(
                journal.report().rejection_class,
                RejectionClass::PolicyLimit
            );
            assert_eq!(journal.export_rejected_tail().unwrap(), expected_tail);
            assert!(matches!(
                journal.discard_unverified_tail(),
                Err(JournalError::TailDiscardForbidden {
                    rejection_class: RejectionClass::PolicyLimit
                })
            ));
            assert_eq!(std::fs::metadata(&path.0).unwrap().len(), expected_length);
            assert_eq!(std::fs::read(&path.0).unwrap(), expected_file);
        }
    }
}
