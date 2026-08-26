//! Typed local persistence for canonical display-capture artifacts.
//!
//! Each physical storage record contains one complete [`DisplayCaptureArtifact`]. The storage
//! record parent orders physical appends; the embedded [`crate::display::DisplayReceiptV2`]
//! parent independently orders display observations. Opening or recovering a journal replays and
//! checks both chains before exposing any writable state.
//!
//! A successful append proves only that the exact typed candidate passed before I/O,
//! `File::sync_all` returned success, and the appended carrier bytes were reread through the
//! retained handle and matched that candidate before its semantic head was published. It does not prove sudden-
//! power survival, latest-prefix/rollback resistance, external custody, scanout, or photons.

use std::fmt;
use std::path::Path;

use crate::display::{DisplayCapture, DisplayCaptureFacts, DisplayProtocolError, DisplayReceiptV2};
use crate::display_artifact::{
    CapturedPixelForm, DISPLAY_CAPTURE_ARTIFACT_SCHEMA, DisplayArtifactError,
    DisplayCaptureArtifact,
};
use crate::frame::FrameRef;
use crate::frame_v2::FrameReceiptV2;
use crate::journal::{
    AnchorPolicy, AppendSyncReceipt, CreateReceipt, Journal, JournalError, OpenJournal,
    RecoveryJournal, RecoveryReport, ScanReport, TailDiscardReceipt,
};
use crate::presentation_output::{
    PRESENTATION_OUTPUT_ARTIFACT_SCHEMA, PresentationOutputArtifact,
    PresentationOutputArtifactError, RouteStability,
};
use crate::protocol::{ContentTag, TagKind};
use crate::storage::{StorageError, StorageLimits, StorageRecord};

/// One semantically verified artifact and the hash-addressed references it carries.
///
/// `artifact`, `display_receipt`, `frame_receipt`, `expected_frame`, and an available captured
/// frame have canonical bytes embedded in the artifact payload. `policy`, `tick_outcome`,
/// `frame_input`, and an optional frame-receipt parent are references only; this journal does not
/// claim their transitive bytes are present. `display_parent`, when present, is guaranteed to be
/// the preceding inventory entry's embedded display receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisplayArtifactInventoryEntry {
    pub storage_record: ContentTag,
    pub storage_parent: Option<ContentTag>,
    /// Exact object named by the enclosing storage record. For legacy records this equals
    /// `artifact`; for presentation-output records it is the outer bundle tag.
    pub stored_object: ContentTag,
    pub artifact: ContentTag,
    pub timing_artifact: Option<ContentTag>,
    pub topology_before: Option<ContentTag>,
    pub topology_after: Option<ContentTag>,
    pub route_stability: Option<RouteStability>,
    pub display_receipt: ContentTag,
    pub display_parent: Option<ContentTag>,
    pub frame_receipt: ContentTag,
    pub frame_receipt_parent: Option<ContentTag>,
    pub tick_outcome: ContentTag,
    pub frame_input: ContentTag,
    pub expected_frame: ContentTag,
    pub captured_frame: Option<ContentTag>,
    pub policy: ContentTag,
    pub captured_form: CapturedPixelForm,
    pub logical_payload_bytes: u64,
    pub physical_record_bytes: u64,
    pub start_offset: u64,
    pub end_offset: u64,
}

/// Exact inventory reconstructed from the currently admitted carrier prefix.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplayedDisplayArtifacts {
    entries: Vec<DisplayArtifactInventoryEntry>,
    logical_payload_bytes: u64,
    physical_record_bytes: u64,
}

impl ReplayedDisplayArtifacts {
    pub fn entries(&self) -> &[DisplayArtifactInventoryEntry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn artifact_head(&self) -> Option<ContentTag> {
        self.entries.last().map(|entry| entry.artifact)
    }

    pub fn stored_object_head(&self) -> Option<ContentTag> {
        self.entries.last().map(|entry| entry.stored_object)
    }

    pub fn display_receipt_head(&self) -> Option<ContentTag> {
        self.entries.last().map(|entry| entry.display_receipt)
    }

    pub fn storage_head(&self) -> Option<ContentTag> {
        self.entries.last().map(|entry| entry.storage_record)
    }

    pub const fn logical_payload_bytes(&self) -> u64 {
        self.logical_payload_bytes
    }

    pub const fn physical_record_bytes(&self) -> u64 {
        self.physical_record_bytes
    }
}

/// A writable display-artifact journal whose entire physical prefix passed typed replay.
pub struct DisplayArtifactJournal {
    carrier: Journal,
    state: ReplayedDisplayArtifacts,
    limits: StorageLimits,
    semantic_poisoned: bool,
}

/// A non-writable physical recovery state whose retained prefix also passed typed replay.
pub struct DisplayArtifactRecoveryJournal {
    carrier: RecoveryJournal,
    state: ReplayedDisplayArtifacts,
    limits: StorageLimits,
}

/// Typed outcome of opening an existing display-artifact journal.
pub enum OpenDisplayArtifactJournal {
    Ready {
        journal: Box<DisplayArtifactJournal>,
        carrier_report: ScanReport,
    },
    NeedsRecovery {
        journal: Box<DisplayArtifactRecoveryJournal>,
    },
}

/// Evidence returned after one artifact was synchronized, reread, decoded, and published.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayArtifactAppendReceipt {
    pub carrier_append: AppendSyncReceipt,
    pub artifact: ContentTag,
    pub display_receipt: ContentTag,
    pub expected_frame: ContentTag,
    pub captured_frame: Option<ContentTag>,
    pub captured_form: CapturedPixelForm,
    pub logical_payload_bytes: u64,
    pub artifact_count: usize,
}

/// Evidence returned after one presentation-output bundle was synchronized, reread, and
/// published through the same unified display-receipt chain as legacy display artifacts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresentationOutputAppendReceipt {
    pub carrier_append: AppendSyncReceipt,
    pub presentation_output: ContentTag,
    pub artifact: ContentTag,
    pub timing_artifact: ContentTag,
    pub topology_before: ContentTag,
    pub topology_after: ContentTag,
    pub route_stability: RouteStability,
    pub display_receipt: ContentTag,
    pub expected_frame: ContentTag,
    pub captured_frame: Option<ContentTag>,
    pub captured_form: CapturedPixelForm,
    pub logical_payload_bytes: u64,
    pub artifact_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DisplayArtifactSemanticError {
    StorageParentMismatch {
        record_index: usize,
        expected: Option<ContentTag>,
        actual: Option<ContentTag>,
    },
    WrongKind {
        record_index: usize,
        actual: TagKind,
    },
    WrongSchema {
        record_index: usize,
        actual: Vec<u8>,
    },
    ArtifactCodec {
        record_index: usize,
        source: DisplayArtifactError,
    },
    PresentationOutputCodec {
        record_index: usize,
        source: PresentationOutputArtifactError,
    },
    ArtifactObjectTagMismatch {
        record_index: usize,
        expected: ContentTag,
        actual: ContentTag,
    },
    PresentationOutputObjectTagMismatch {
        record_index: usize,
        expected: ContentTag,
        actual: ContentTag,
    },
    DisplayParentMismatch {
        record_index: usize,
        expected: Option<ContentTag>,
        actual: Option<ContentTag>,
    },
    LengthOverflow {
        field: &'static str,
    },
    AllocationFailed {
        requested_entries: usize,
    },
}

impl fmt::Display for DisplayArtifactSemanticError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StorageParentMismatch {
                record_index,
                expected,
                actual,
            } => write!(
                formatter,
                "display carrier record {record_index} parent is {actual:?}, expected {expected:?}"
            ),
            Self::WrongKind {
                record_index,
                actual,
            } => write!(
                formatter,
                "display carrier record {record_index} must contain an exact supported display object, received {}",
                actual.name()
            ),
            Self::WrongSchema {
                record_index,
                actual,
            } => write!(
                formatter,
                "display carrier record {record_index} schema {:?} is not the exact schema for its display-object kind (legacy schema {:?})",
                String::from_utf8_lossy(actual),
                String::from_utf8_lossy(DISPLAY_CAPTURE_ARTIFACT_SCHEMA)
            ),
            Self::ArtifactCodec {
                record_index,
                source,
            } => write!(
                formatter,
                "display carrier record {record_index} is not a canonical artifact: {source}"
            ),
            Self::PresentationOutputCodec {
                record_index,
                source,
            } => write!(
                formatter,
                "display carrier record {record_index} is not a canonical presentation-output artifact: {source}"
            ),
            Self::ArtifactObjectTagMismatch {
                record_index,
                expected,
                actual,
            } => write!(
                formatter,
                "display carrier record {record_index} artifact is {expected}, carrier names {actual}"
            ),
            Self::PresentationOutputObjectTagMismatch {
                record_index,
                expected,
                actual,
            } => write!(
                formatter,
                "display carrier record {record_index} presentation output is {expected}, carrier names {actual}"
            ),
            Self::DisplayParentMismatch {
                record_index,
                expected,
                actual,
            } => write!(
                formatter,
                "display artifact {record_index} receipt parent is {actual:?}, expected {expected:?}"
            ),
            Self::LengthOverflow { field } => {
                write!(formatter, "display-artifact inventory {field} overflowed")
            }
            Self::AllocationFailed { requested_entries } => write!(
                formatter,
                "could not reserve {requested_entries} display-artifact inventory entries"
            ),
        }
    }
}

impl std::error::Error for DisplayArtifactSemanticError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ArtifactCodec { source, .. } => Some(source),
            Self::PresentationOutputCodec { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum DisplayArtifactStoreError {
    Journal(JournalError),
    StoragePreflight(StorageError),
    DisplayProtocol(DisplayProtocolError),
    Artifact(DisplayArtifactError),
    PresentationOutput(PresentationOutputArtifactError),
    Semantic(DisplayArtifactSemanticError),
    Poisoned,
    InventoryAllocationFailed,
    PostAppendInvariant {
        field: &'static str,
    },
    PersistedButUnpublished {
        record_tag: ContentTag,
        start_offset: u64,
        end_offset: u64,
        field: &'static str,
    },
}

impl fmt::Display for DisplayArtifactStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Journal(source) => write!(formatter, "display journal carrier failed: {source}"),
            Self::StoragePreflight(source) => {
                write!(formatter, "display journal preflight failed: {source}")
            }
            Self::DisplayProtocol(source) => {
                write!(formatter, "next display receipt was refused: {source}")
            }
            Self::Artifact(source) => write!(formatter, "display artifact was refused: {source}"),
            Self::PresentationOutput(source) => {
                write!(
                    formatter,
                    "presentation-output artifact was refused: {source}"
                )
            }
            Self::Semantic(source) => write!(formatter, "display journal replay failed: {source}"),
            Self::Poisoned => formatter
                .write_str("display journal is poisoned; close and reopen through typed replay"),
            Self::InventoryAllocationFailed => formatter.write_str(
                "could not reserve one display-artifact inventory entry before persistence",
            ),
            Self::PostAppendInvariant { field } => write!(
                formatter,
                "synchronized display artifact violated post-append invariant {field}; close and reopen through typed replay"
            ),
            Self::PersistedButUnpublished {
                record_tag,
                start_offset,
                end_offset,
                field,
            } => write!(
                formatter,
                "synchronized record {record_tag} at [{start_offset}, {end_offset}) violated post-append invariant {field}; the typed handle requires anchored reopen"
            ),
        }
    }
}

impl std::error::Error for DisplayArtifactStoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Journal(source) => Some(source),
            Self::StoragePreflight(source) => Some(source),
            Self::DisplayProtocol(source) => Some(source),
            Self::Artifact(source) => Some(source),
            Self::PresentationOutput(source) => Some(source),
            Self::Semantic(source) => Some(source),
            Self::Poisoned
            | Self::InventoryAllocationFailed
            | Self::PostAppendInvariant { .. }
            | Self::PersistedButUnpublished { .. } => None,
        }
    }
}

impl From<JournalError> for DisplayArtifactStoreError {
    fn from(source: JournalError) -> Self {
        Self::Journal(source)
    }
}

impl From<DisplayArtifactError> for DisplayArtifactStoreError {
    fn from(source: DisplayArtifactError) -> Self {
        Self::Artifact(source)
    }
}

impl From<PresentationOutputArtifactError> for DisplayArtifactStoreError {
    fn from(source: PresentationOutputArtifactError) -> Self {
        Self::PresentationOutput(source)
    }
}

impl From<DisplayProtocolError> for DisplayArtifactStoreError {
    fn from(source: DisplayProtocolError) -> Self {
        Self::DisplayProtocol(source)
    }
}

impl From<DisplayArtifactSemanticError> for DisplayArtifactStoreError {
    fn from(source: DisplayArtifactSemanticError) -> Self {
        Self::Semantic(source)
    }
}

impl DisplayArtifactJournal {
    /// Creates and synchronizes a new empty carrier. The first later artifact must have no display
    /// parent. Existing paths are never opened or replaced.
    pub fn create_new<P: AsRef<Path>>(
        path: P,
        limits: StorageLimits,
    ) -> Result<(Self, CreateReceipt), DisplayArtifactStoreError> {
        let (carrier, receipt) = Journal::create_new(path, limits)?;
        Ok((
            Self {
                carrier,
                state: ReplayedDisplayArtifacts::default(),
                limits,
                semantic_poisoned: false,
            },
            receipt,
        ))
    }

    /// Opens an existing carrier only after typed replay of every physically admitted record.
    pub fn open_existing<P: AsRef<Path>>(
        path: P,
        limits: StorageLimits,
        anchor: AnchorPolicy,
    ) -> Result<OpenDisplayArtifactJournal, DisplayArtifactStoreError> {
        match Journal::open_existing(path, limits, anchor)? {
            OpenJournal::Ready {
                journal: carrier,
                report: carrier_report,
            } => {
                let state = replay_display_artifact_records(carrier.records())?;
                if state.physical_record_bytes() != carrier.valid_bytes() {
                    return Err(DisplayArtifactStoreError::PostAppendInvariant {
                        field: "replayed physical length",
                    });
                }
                Ok(OpenDisplayArtifactJournal::Ready {
                    journal: Box::new(Self {
                        carrier,
                        state,
                        limits,
                        semantic_poisoned: false,
                    }),
                    carrier_report,
                })
            }
            OpenJournal::NeedsRecovery { journal: carrier } => {
                let state = replay_display_artifact_records(carrier.verified_prefix_records())?;
                if state.physical_record_bytes() != carrier.report().valid_prefix_bytes {
                    return Err(DisplayArtifactStoreError::PostAppendInvariant {
                        field: "recovery-prefix physical length",
                    });
                }
                Ok(OpenDisplayArtifactJournal::NeedsRecovery {
                    journal: Box::new(DisplayArtifactRecoveryJournal {
                        carrier: *carrier,
                        state,
                        limits,
                    }),
                })
            }
        }
    }

    pub const fn state(&self) -> &ReplayedDisplayArtifacts {
        &self.state
    }

    pub fn carrier_head(&self) -> Option<ContentTag> {
        self.carrier.head()
    }

    pub const fn carrier_valid_bytes(&self) -> u64 {
        self.carrier.valid_bytes()
    }

    pub const fn is_poisoned(&self) -> bool {
        self.carrier.is_poisoned() || self.semantic_poisoned
    }

    /// Builds the next receipt against the exact display head recovered by this journal.
    ///
    /// This is nonpublishing. [`Self::append_artifact`] still checks the parent again, so a stale
    /// prepared receipt cannot advance the journal.
    pub fn prepare_next_receipt(
        &self,
        expected_receipt: FrameReceiptV2,
        expected_width: u32,
        expected_height: u32,
        facts: DisplayCaptureFacts,
    ) -> Result<DisplayReceiptV2, DisplayArtifactStoreError> {
        if self.is_poisoned() {
            return Err(DisplayArtifactStoreError::Poisoned);
        }
        DisplayReceiptV2::prepare(
            self.state.display_receipt_head(),
            expected_receipt,
            expected_width,
            expected_height,
            facts,
        )
        .map_err(Into::into)
    }

    /// Converts one transient capture into a complete canonical artifact against this journal's
    /// currently published semantic head. This does not perform I/O or publish the display head,
    /// so a caller can retain the exact owned artifact across a refused append without resampling
    /// the display.
    pub fn prepare_capture_artifact(
        &self,
        expected_receipt: FrameReceiptV2,
        expected: FrameRef<'_>,
        capture: &DisplayCapture,
    ) -> Result<DisplayCaptureArtifact, DisplayArtifactStoreError> {
        let receipt = self.prepare_next_receipt(
            expected_receipt,
            expected.width(),
            expected.height(),
            capture.facts,
        )?;
        let captured = capture
            .pixels
            .as_deref()
            .map(|pixels| FrameRef::new(expected.width(), expected.height(), pixels))
            .transpose()
            .map_err(DisplayArtifactError::FrameShape)?;
        DisplayCaptureArtifact::new(receipt, expected, captured).map_err(Into::into)
    }

    /// Converts and appends one transient capture through the typed journal gate. Call
    /// [`Self::prepare_capture_artifact`] followed by [`Self::append_artifact`] instead when the
    /// exact candidate must survive a refused append.
    pub fn append_capture(
        &mut self,
        expected_receipt: FrameReceiptV2,
        expected: FrameRef<'_>,
        capture: &DisplayCapture,
    ) -> Result<DisplayArtifactAppendReceipt, DisplayArtifactStoreError> {
        let artifact = self.prepare_capture_artifact(expected_receipt, expected, capture)?;
        self.append_artifact(&artifact)
    }

    /// Fully types and reserves one candidate before I/O, then synchronizes and rereads its exact
    /// record before advancing the semantic inventory. No fallible allocation or decode occurs
    /// after carrier success.
    pub fn append_artifact(
        &mut self,
        artifact: &DisplayCaptureArtifact,
    ) -> Result<DisplayArtifactAppendReceipt, DisplayArtifactStoreError> {
        if self.is_poisoned() {
            return Err(DisplayArtifactStoreError::Poisoned);
        }
        artifact.verify()?;
        let record_index = self.state.len();
        let display_receipt = artifact.receipt();
        let expected_parent = self.state.display_receipt_head();
        if display_receipt.parent() != expected_parent {
            return Err(DisplayArtifactSemanticError::DisplayParentMismatch {
                record_index,
                expected: expected_parent,
                actual: display_receipt.parent(),
            }
            .into());
        }
        let payload = artifact.encode_canonical()?;
        self.state
            .entries
            .try_reserve(1)
            .map_err(|_| DisplayArtifactStoreError::InventoryAllocationFailed)?;

        let expected_artifact = artifact.tag();
        let candidate = StorageRecord::from_tagged_payload_with_limits(
            self.carrier.head(),
            expected_artifact,
            DISPLAY_CAPTURE_ARTIFACT_SCHEMA,
            &payload,
            self.limits,
        )
        .map_err(DisplayArtifactStoreError::StoragePreflight)?;
        let entry = inventory_entry(
            &candidate,
            record_index,
            self.state.storage_head(),
            expected_parent,
            self.state.physical_record_bytes,
        )?;
        let next_logical_payload_bytes = self
            .state
            .logical_payload_bytes
            .checked_add(entry.logical_payload_bytes)
            .ok_or(DisplayArtifactSemanticError::LengthOverflow {
                field: "logical payload bytes",
            })?;
        drop(candidate);

        let carrier_append = self.carrier.append_synced(
            TagKind::DisplayArtifact,
            DISPLAY_CAPTURE_ARTIFACT_SCHEMA,
            &payload,
        )?;
        self.semantic_poisoned = true;

        if carrier_append.object_tag != expected_artifact {
            return Err(persisted_but_unpublished(
                &carrier_append,
                "artifact object tag",
            ));
        }
        let Some(record) = self.carrier.records().last() else {
            return Err(persisted_but_unpublished(
                &carrier_append,
                "published carrier record",
            ));
        };
        if entry.storage_record != carrier_append.record_tag {
            return Err(persisted_but_unpublished(
                &carrier_append,
                "storage-record tag",
            ));
        }
        if entry.start_offset != carrier_append.start_offset
            || entry.end_offset != carrier_append.end_offset
            || entry.physical_record_bytes != carrier_append.encoded_bytes
        {
            return Err(persisted_but_unpublished(
                &carrier_append,
                "carrier byte range",
            ));
        }
        if entry.artifact != expected_artifact
            || entry.display_receipt != display_receipt.tag()
            || entry.expected_frame != display_receipt.expected_frame()
            || entry.captured_form != artifact.captured_form()
        {
            return Err(persisted_but_unpublished(
                &carrier_append,
                "preflight artifact facts",
            ));
        }
        if record.record_tag() != entry.storage_record
            || record.parent() != entry.storage_parent
            || record.object_tag() != entry.artifact
            || record.object_schema() != DISPLAY_CAPTURE_ARTIFACT_SCHEMA
            || record.payload() != payload
            || self.carrier.valid_bytes() != entry.end_offset
        {
            return Err(persisted_but_unpublished(
                &carrier_append,
                "published carrier candidate equality",
            ));
        }

        self.state.logical_payload_bytes = next_logical_payload_bytes;
        self.state.physical_record_bytes = entry.end_offset;
        self.state.entries.push(entry);
        self.semantic_poisoned = false;

        Ok(DisplayArtifactAppendReceipt {
            carrier_append,
            artifact: entry.artifact,
            display_receipt: entry.display_receipt,
            expected_frame: entry.expected_frame,
            captured_frame: entry.captured_frame,
            captured_form: entry.captured_form,
            logical_payload_bytes: entry.logical_payload_bytes,
            artifact_count: self.state.len(),
        })
    }

    /// Appends one exact presentation-output bundle while advancing the same embedded
    /// display-receipt chain used by [`Self::append_artifact`]. All nested decoding, storage
    /// preflight, aggregate arithmetic, and inventory reservation complete before I/O.
    pub fn append_presentation_output(
        &mut self,
        presentation: &PresentationOutputArtifact,
    ) -> Result<PresentationOutputAppendReceipt, DisplayArtifactStoreError> {
        if self.is_poisoned() {
            return Err(DisplayArtifactStoreError::Poisoned);
        }
        presentation.verify()?;
        let record_index = self.state.len();
        let display = presentation.display();
        let display_receipt = display.receipt();
        let expected_parent = self.state.display_receipt_head();
        if display_receipt.parent() != expected_parent {
            return Err(DisplayArtifactSemanticError::DisplayParentMismatch {
                record_index,
                expected: expected_parent,
                actual: display_receipt.parent(),
            }
            .into());
        }

        let payload = presentation.encode_canonical()?;
        self.state
            .entries
            .try_reserve(1)
            .map_err(|_| DisplayArtifactStoreError::InventoryAllocationFailed)?;

        let expected_presentation = presentation.tag();
        let expected_artifact = display.tag();
        let expected_timing = presentation.timing().tag();
        let expected_topology_before = presentation.pre_topology().tag();
        let expected_topology_after = presentation.post_topology().tag();
        let expected_stability = presentation.route_stability();
        let candidate = StorageRecord::from_tagged_payload_with_limits(
            self.carrier.head(),
            expected_presentation,
            PRESENTATION_OUTPUT_ARTIFACT_SCHEMA,
            &payload,
            self.limits,
        )
        .map_err(DisplayArtifactStoreError::StoragePreflight)?;
        let entry = inventory_entry(
            &candidate,
            record_index,
            self.state.storage_head(),
            expected_parent,
            self.state.physical_record_bytes,
        )?;
        let next_logical_payload_bytes = self
            .state
            .logical_payload_bytes
            .checked_add(entry.logical_payload_bytes)
            .ok_or(DisplayArtifactSemanticError::LengthOverflow {
                field: "logical payload bytes",
            })?;
        drop(candidate);

        let carrier_append = self.carrier.append_synced(
            TagKind::PresentationOutputArtifact,
            PRESENTATION_OUTPUT_ARTIFACT_SCHEMA,
            &payload,
        )?;
        self.semantic_poisoned = true;

        if carrier_append.object_tag != expected_presentation {
            return Err(persisted_but_unpublished(
                &carrier_append,
                "presentation-output object tag",
            ));
        }
        let Some(record) = self.carrier.records().last() else {
            return Err(persisted_but_unpublished(
                &carrier_append,
                "published carrier record",
            ));
        };
        if entry.storage_record != carrier_append.record_tag {
            return Err(persisted_but_unpublished(
                &carrier_append,
                "storage-record tag",
            ));
        }
        if entry.start_offset != carrier_append.start_offset
            || entry.end_offset != carrier_append.end_offset
            || entry.physical_record_bytes != carrier_append.encoded_bytes
        {
            return Err(persisted_but_unpublished(
                &carrier_append,
                "carrier byte range",
            ));
        }
        if entry.stored_object != expected_presentation
            || entry.artifact != expected_artifact
            || entry.timing_artifact != Some(expected_timing)
            || entry.topology_before != Some(expected_topology_before)
            || entry.topology_after != Some(expected_topology_after)
            || entry.route_stability != Some(expected_stability)
            || entry.display_receipt != display_receipt.tag()
            || entry.expected_frame != display_receipt.expected_frame()
            || entry.captured_form != display.captured_form()
        {
            return Err(persisted_but_unpublished(
                &carrier_append,
                "preflight presentation-output facts",
            ));
        }
        if record.record_tag() != entry.storage_record
            || record.parent() != entry.storage_parent
            || record.object_tag() != entry.stored_object
            || record.object_schema() != PRESENTATION_OUTPUT_ARTIFACT_SCHEMA
            || record.payload() != payload
            || self.carrier.valid_bytes() != entry.end_offset
        {
            return Err(persisted_but_unpublished(
                &carrier_append,
                "published carrier candidate equality",
            ));
        }

        self.state.logical_payload_bytes = next_logical_payload_bytes;
        self.state.physical_record_bytes = entry.end_offset;
        self.state.entries.push(entry);
        self.semantic_poisoned = false;

        Ok(PresentationOutputAppendReceipt {
            carrier_append,
            presentation_output: entry.stored_object,
            artifact: entry.artifact,
            timing_artifact: entry
                .timing_artifact
                .expect("presentation-output inventory retains timing tag"),
            topology_before: entry
                .topology_before
                .expect("presentation-output inventory retains pre topology tag"),
            topology_after: entry
                .topology_after
                .expect("presentation-output inventory retains post topology tag"),
            route_stability: entry
                .route_stability
                .expect("presentation-output inventory retains route stability"),
            display_receipt: entry.display_receipt,
            expected_frame: entry.expected_frame,
            captured_frame: entry.captured_frame,
            captured_form: entry.captured_form,
            logical_payload_bytes: entry.logical_payload_bytes,
            artifact_count: self.state.len(),
        })
    }
}

fn persisted_but_unpublished(
    receipt: &AppendSyncReceipt,
    field: &'static str,
) -> DisplayArtifactStoreError {
    DisplayArtifactStoreError::PersistedButUnpublished {
        record_tag: receipt.record_tag,
        start_offset: receipt.start_offset,
        end_offset: receipt.end_offset,
        field,
    }
}

impl DisplayArtifactRecoveryJournal {
    pub const fn report(&self) -> &RecoveryReport {
        self.carrier.report()
    }

    pub const fn state(&self) -> &ReplayedDisplayArtifacts {
        &self.state
    }

    pub fn export_rejected_tail(&mut self) -> Result<Vec<u8>, DisplayArtifactStoreError> {
        self.carrier.export_rejected_tail().map_err(Into::into)
    }

    /// Discards only a physically eligible suffix. The generic carrier re-verifies that the exact
    /// retained bytes equal the prefix already typed-replayed into `state`, so no fallible typed
    /// work occurs after truncation succeeds.
    pub fn discard_unverified_tail(
        self,
    ) -> Result<(DisplayArtifactJournal, TailDiscardReceipt), DisplayArtifactStoreError> {
        let Self {
            carrier,
            state,
            limits,
        } = self;
        let (carrier, receipt) = carrier.discard_unverified_tail()?;
        debug_assert_eq!(state.physical_record_bytes(), carrier.valid_bytes());
        debug_assert_eq!(state.storage_head(), carrier.head());
        Ok((
            DisplayArtifactJournal {
                carrier,
                state,
                limits,
                semantic_poisoned: false,
            },
            receipt,
        ))
    }
}

/// Reconstructs the exact typed inventory for one already admitted storage-record prefix.
pub fn replay_display_artifact_records(
    records: &[StorageRecord],
) -> Result<ReplayedDisplayArtifacts, DisplayArtifactSemanticError> {
    let mut entries = Vec::new();
    entries.try_reserve_exact(records.len()).map_err(|_| {
        DisplayArtifactSemanticError::AllocationFailed {
            requested_entries: records.len(),
        }
    })?;
    let mut expected_storage_parent = None;
    let mut expected_display_parent = None;
    let mut logical_payload_bytes = 0_u64;
    let mut physical_record_bytes = 0_u64;

    for (record_index, record) in records.iter().enumerate() {
        let entry = inventory_entry(
            record,
            record_index,
            expected_storage_parent,
            expected_display_parent,
            physical_record_bytes,
        )?;
        logical_payload_bytes = logical_payload_bytes
            .checked_add(entry.logical_payload_bytes)
            .ok_or(DisplayArtifactSemanticError::LengthOverflow {
                field: "logical payload bytes",
            })?;
        physical_record_bytes = entry.end_offset;
        expected_storage_parent = Some(entry.storage_record);
        expected_display_parent = Some(entry.display_receipt);
        entries.push(entry);
    }

    Ok(ReplayedDisplayArtifacts {
        entries,
        logical_payload_bytes,
        physical_record_bytes,
    })
}

fn inventory_entry(
    record: &StorageRecord,
    record_index: usize,
    expected_storage_parent: Option<ContentTag>,
    expected_display_parent: Option<ContentTag>,
    start_offset: u64,
) -> Result<DisplayArtifactInventoryEntry, DisplayArtifactSemanticError> {
    if record.parent() != expected_storage_parent {
        return Err(DisplayArtifactSemanticError::StorageParentMismatch {
            record_index,
            expected: expected_storage_parent,
            actual: record.parent(),
        });
    }
    let (
        stored_object,
        artifact_tag,
        timing_artifact,
        topology_before,
        topology_after,
        route_stability,
        display_receipt,
        captured_form,
    ) = match record.object_tag().kind() {
        TagKind::DisplayArtifact => {
            if record.object_schema() != DISPLAY_CAPTURE_ARTIFACT_SCHEMA {
                return Err(DisplayArtifactSemanticError::WrongSchema {
                    record_index,
                    actual: record.object_schema().to_vec(),
                });
            }
            let artifact =
                DisplayCaptureArtifact::decode_canonical(record.payload()).map_err(|source| {
                    DisplayArtifactSemanticError::ArtifactCodec {
                        record_index,
                        source,
                    }
                })?;
            if artifact.tag() != record.object_tag() {
                return Err(DisplayArtifactSemanticError::ArtifactObjectTagMismatch {
                    record_index,
                    expected: artifact.tag(),
                    actual: record.object_tag(),
                });
            }
            (
                artifact.tag(),
                artifact.tag(),
                None,
                None,
                None,
                None,
                artifact.receipt(),
                artifact.captured_form(),
            )
        }
        TagKind::PresentationOutputArtifact => {
            if record.object_schema() != PRESENTATION_OUTPUT_ARTIFACT_SCHEMA {
                return Err(DisplayArtifactSemanticError::WrongSchema {
                    record_index,
                    actual: record.object_schema().to_vec(),
                });
            }
            let presentation = PresentationOutputArtifact::decode_canonical(record.payload())
                .map_err(
                    |source| DisplayArtifactSemanticError::PresentationOutputCodec {
                        record_index,
                        source,
                    },
                )?;
            if presentation.tag() != record.object_tag() {
                return Err(
                    DisplayArtifactSemanticError::PresentationOutputObjectTagMismatch {
                        record_index,
                        expected: presentation.tag(),
                        actual: record.object_tag(),
                    },
                );
            }
            (
                presentation.tag(),
                presentation.display().tag(),
                Some(presentation.timing().tag()),
                Some(presentation.pre_topology().tag()),
                Some(presentation.post_topology().tag()),
                Some(presentation.route_stability()),
                presentation.display().receipt(),
                presentation.display().captured_form(),
            )
        }
        actual => {
            return Err(DisplayArtifactSemanticError::WrongKind {
                record_index,
                actual,
            });
        }
    };
    if display_receipt.parent() != expected_display_parent {
        return Err(DisplayArtifactSemanticError::DisplayParentMismatch {
            record_index,
            expected: expected_display_parent,
            actual: display_receipt.parent(),
        });
    }
    let frame_receipt = display_receipt.expected_receipt();
    let logical_payload_bytes = u64::try_from(record.payload().len()).map_err(|_| {
        DisplayArtifactSemanticError::LengthOverflow {
            field: "logical payload bytes",
        }
    })?;
    let physical_record_bytes = u64::try_from(record.encoded_len()).map_err(|_| {
        DisplayArtifactSemanticError::LengthOverflow {
            field: "physical record bytes",
        }
    })?;
    let end_offset = start_offset.checked_add(physical_record_bytes).ok_or(
        DisplayArtifactSemanticError::LengthOverflow {
            field: "physical end offset",
        },
    )?;

    Ok(DisplayArtifactInventoryEntry {
        storage_record: record.record_tag(),
        storage_parent: record.parent(),
        stored_object,
        artifact: artifact_tag,
        timing_artifact,
        topology_before,
        topology_after,
        route_stability,
        display_receipt: display_receipt.tag(),
        display_parent: display_receipt.parent(),
        frame_receipt: frame_receipt.tag(),
        frame_receipt_parent: frame_receipt.parent(),
        tick_outcome: frame_receipt.outcome(),
        frame_input: frame_receipt.input(),
        expected_frame: frame_receipt.frame(),
        captured_frame: display_receipt
            .facts()
            .captured
            .map(|observation| observation.tag),
        policy: display_receipt.policy(),
        captured_form,
        logical_payload_bytes,
        physical_record_bytes,
        start_offset,
        end_offset,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, OpenOptions};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use crate::display::{
        CaptureRect, CaptureStage, DisplayCapture, DisplayCaptureFacts, DisplayLedgerV2,
        DisplayOutcome,
    };
    use crate::frame::FrameRef;
    use crate::frame_v2::{FrameReceiptV2, FrameStream};
    use crate::journal::{AppendedBytesEvidence, JournalError, RejectionClass, SyncEvidence};
    use crate::output_topology::{AcquisitionAttempt, AcquisitionFacts, OutputTopologyObservation};
    use crate::presentation::{
        CompositionTimingEnvelope, DwmFlushCall, DwmTimingCall, GdiFlushCall, PresenterEvidence,
        QpcBracket, QpcFrequency, QpcRead,
    };
    use crate::protocol::hash_canonical;
    use crate::storage::{
        MAX_STORAGE_RECORD_BYTES, MAX_STORAGE_RECORDS, MAX_STORAGE_SCHEMA_BYTES,
        MAX_STORAGE_STREAM_BYTES, StorageError, StorageRecord,
    };

    const EXPECTED_PIXELS: [u32; 2] = [0x0011_2233, 0x0044_5566];
    static PATH_SERIAL: AtomicU64 = AtomicU64::new(0);

    struct TempJournalPath(PathBuf);

    impl TempJournalPath {
        fn new(label: &str) -> Self {
            let serial = PATH_SERIAL.fetch_add(1, Ordering::Relaxed);
            Self(std::env::temp_dir().join(format!(
                "bitblock-display-journal-{label}-{}-{serial}.bbj",
                std::process::id()
            )))
        }
    }

    impl Drop for TempJournalPath {
        fn drop(&mut self) {
            match fs::remove_file(&self.0) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => panic!("could not remove display-journal test file: {error}"),
            }
        }
    }

    fn tag(kind: TagKind, label: &[u8]) -> ContentTag {
        hash_canonical(
            kind,
            b"bitblock/display-artifact-test/v1",
            label.len() as u64,
            |sink| sink(label),
        )
        .unwrap()
    }

    fn frame(pixels: &[u32]) -> FrameRef<'_> {
        FrameRef::new(2, 1, pixels).unwrap()
    }

    fn complete_facts(expected: FrameRef<'_>, captured: FrameRef<'_>) -> DisplayCaptureFacts {
        let rect = CaptureRect {
            left: -20,
            top: 30,
            right: -18,
            bottom: 31,
        };
        let mut mismatch_count = 0_u64;
        let mut first_mismatch = None;
        for (index, (&wanted, &seen)) in expected.pixels().iter().zip(captured.pixels()).enumerate()
        {
            if wanted != seen {
                mismatch_count += 1;
                first_mismatch.get_or_insert(index as u64);
            }
        }
        DisplayCaptureFacts {
            outcome: if mismatch_count == 0 {
                DisplayOutcome::Match
            } else {
                DisplayOutcome::Mismatch
            },
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
            mismatch_count,
            first_mismatch,
        }
    }

    fn artifact_pair() -> (DisplayCaptureArtifact, DisplayCaptureArtifact) {
        let expected = frame(&EXPECTED_PIXELS);
        let mut display = DisplayLedgerV2::default();
        let first_frame_receipt = FrameReceiptV2::new(
            None,
            FrameStream::Composite,
            tag(TagKind::TickOutcome, b"outcome"),
            tag(TagKind::FrameInput, b"input"),
            expected.content_tag(),
        )
        .unwrap();
        let first_receipt = *display
            .append(
                first_frame_receipt,
                2,
                1,
                complete_facts(expected, expected),
            )
            .unwrap();
        let first = DisplayCaptureArtifact::new(first_receipt, expected, Some(expected)).unwrap();

        let second_frame_receipt = FrameReceiptV2::new(
            Some(first_frame_receipt.tag()),
            FrameStream::Composite,
            tag(TagKind::TickOutcome, b"outcome-2"),
            tag(TagKind::FrameInput, b"input-2"),
            expected.content_tag(),
        )
        .unwrap();
        let second_captured_pixels = [0x0011_2233, 0x0044_5567];
        let second_captured = frame(&second_captured_pixels);
        let second_receipt = *display
            .append(
                second_frame_receipt,
                2,
                1,
                complete_facts(expected, second_captured),
            )
            .unwrap();
        let second =
            DisplayCaptureArtifact::new(second_receipt, expected, Some(second_captured)).unwrap();
        (first, second)
    }

    fn standalone_artifact() -> DisplayCaptureArtifact {
        artifact_pair().0
    }

    fn absent_artifact() -> DisplayCaptureArtifact {
        let expected = frame(&EXPECTED_PIXELS);
        let frame_receipt = FrameReceiptV2::new(
            None,
            FrameStream::Composite,
            tag(TagKind::TickOutcome, b"absent-outcome"),
            tag(TagKind::FrameInput, b"absent-input"),
            expected.content_tag(),
        )
        .unwrap();
        let mut display = DisplayLedgerV2::default();
        let receipt = *display
            .append(
                frame_receipt,
                2,
                1,
                DisplayCaptureFacts::unavailable(CaptureStage::FlushBefore, 5),
            )
            .unwrap();
        DisplayCaptureArtifact::new(receipt, expected, None).unwrap()
    }

    fn qpc(value: i64) -> QpcRead {
        QpcRead {
            succeeded: true,
            value,
            error_code: 0,
        }
    }

    fn qpc_bracket(before: i64, after: i64) -> QpcBracket {
        QpcBracket {
            before: qpc(before),
            after: qpc(after),
        }
    }

    fn timing_for(artifact: &DisplayCaptureArtifact) -> CompositionTimingEnvelope {
        CompositionTimingEnvelope::new(
            artifact.receipt().expected_receipt(),
            PresenterEvidence {
                policy: tag(TagKind::Policy, b"presenter-policy"),
                requested_scanlines: 1,
                returned_scanlines: 1,
                call_error_code: 0,
                release_result: 1,
                release_error_code: 0,
            },
            QpcFrequency {
                succeeded: true,
                counts_per_second: 10_000_000,
                error_code: 0,
            },
            DwmTimingCall {
                bracket: qpc_bracket(10, 11),
                hresult: -1,
                snapshot: None,
            },
            qpc_bracket(12, 13),
            GdiFlushCall {
                bracket: qpc_bracket(14, 15),
                succeeded: true,
            },
            DwmFlushCall {
                bracket: qpc_bracket(16, 17),
                hresult: 0,
            },
            DwmTimingCall {
                bracket: qpc_bracket(18, 19),
                hresult: -1,
                snapshot: None,
            },
        )
        .unwrap()
    }

    fn unavailable_topology(profile: u32) -> OutputTopologyObservation {
        OutputTopologyObservation::new_unavailable(
            AcquisitionFacts {
                profile_code_raw: profile,
                request_flags_raw: 0x52,
                attempts: vec![AcquisitionAttempt {
                    operation_code_raw: 1,
                    profile_code_raw: profile,
                    request_flags_raw: 0x52,
                    retry_ordinal: 0,
                    outcome_code_raw: 1,
                    error_code: 1168,
                    reported_path_capacity: 0,
                    reported_mode_capacity: 0,
                }],
            },
            1,
            1168,
        )
        .unwrap()
    }

    fn presentation_output(artifact: DisplayCaptureArtifact) -> PresentationOutputArtifact {
        let timing = timing_for(&artifact);
        PresentationOutputArtifact::new(
            artifact,
            timing,
            unavailable_topology(1),
            unavailable_topology(2),
        )
        .unwrap()
    }

    #[test]
    fn transient_match_mismatch_and_unavailable_captures_append_as_one_typed_chain() {
        let path = TempJournalPath::new("live-adapter");
        let expected = frame(&EXPECTED_PIXELS);
        let (mut journal, _) =
            DisplayArtifactJournal::create_new(&path.0, StorageLimits::DEFAULT).unwrap();

        let first_frame_receipt = FrameReceiptV2::new(
            None,
            FrameStream::Composite,
            tag(TagKind::TickOutcome, b"live-outcome-1"),
            tag(TagKind::FrameInput, b"live-input-1"),
            expected.content_tag(),
        )
        .unwrap();
        let matched = DisplayCapture {
            facts: complete_facts(expected, expected),
            pixels: Some(EXPECTED_PIXELS.to_vec()),
        };
        let first = journal
            .append_capture(first_frame_receipt, expected, &matched)
            .unwrap();
        assert_eq!(first.captured_form, CapturedPixelForm::ReuseExpected);
        assert_eq!(first.artifact_count, 1);

        let mismatch_pixels = [0x0011_2232, 0x0044_5566];
        let mismatch_frame = frame(&mismatch_pixels);
        let second_frame_receipt = FrameReceiptV2::new(
            Some(first_frame_receipt.tag()),
            FrameStream::Composite,
            tag(TagKind::TickOutcome, b"live-outcome-2"),
            tag(TagKind::FrameInput, b"live-input-2"),
            expected.content_tag(),
        )
        .unwrap();
        let mismatched = DisplayCapture {
            facts: complete_facts(expected, mismatch_frame),
            pixels: Some(mismatch_pixels.to_vec()),
        };
        let second = journal
            .append_capture(second_frame_receipt, expected, &mismatched)
            .unwrap();
        assert_eq!(second.captured_form, CapturedPixelForm::Inline);
        assert_eq!(second.artifact_count, 2);

        let third_frame_receipt = FrameReceiptV2::new(
            Some(second_frame_receipt.tag()),
            FrameStream::Composite,
            tag(TagKind::TickOutcome, b"live-outcome-3"),
            tag(TagKind::FrameInput, b"live-input-3"),
            expected.content_tag(),
        )
        .unwrap();
        let unavailable = DisplayCapture::unavailable(CaptureStage::FlushBefore, 5);
        let third = journal
            .append_capture(third_frame_receipt, expected, &unavailable)
            .unwrap();
        assert_eq!(third.captured_form, CapturedPixelForm::Absent);
        assert_eq!(third.artifact_count, 3);

        let fourth_frame_receipt = FrameReceiptV2::new(
            Some(third_frame_receipt.tag()),
            FrameStream::Composite,
            tag(TagKind::TickOutcome, b"live-outcome-4"),
            tag(TagKind::FrameInput, b"live-input-4"),
            expected.content_tag(),
        )
        .unwrap();
        let post_read_pixels = [0x0011_2233, 0x0044_5567];
        let post_read_frame = frame(&post_read_pixels);
        let mut post_read_facts = complete_facts(expected, post_read_frame);
        post_read_facts.outcome = DisplayOutcome::Unavailable;
        post_read_facts.stage = CaptureStage::GeometryAfter;
        post_read_facts.error_code = 87;
        post_read_facts.mismatch_count = 0;
        post_read_facts.first_mismatch = None;
        let post_read_unavailable = DisplayCapture {
            facts: post_read_facts,
            pixels: Some(post_read_pixels.to_vec()),
        };
        let fourth = journal
            .append_capture(fourth_frame_receipt, expected, &post_read_unavailable)
            .unwrap();
        assert_eq!(fourth.captured_form, CapturedPixelForm::Inline);
        assert_eq!(fourth.artifact_count, 4);

        let entries = journal.state().entries();
        assert_eq!(entries[0].display_parent, None);
        assert_eq!(entries[1].display_parent, Some(entries[0].display_receipt));
        assert_eq!(entries[2].display_parent, Some(entries[1].display_receipt));
        assert_eq!(entries[3].display_parent, Some(entries[2].display_receipt));
        assert_eq!(entries[0].frame_receipt, first_frame_receipt.tag());
        assert_eq!(entries[1].frame_receipt, second_frame_receipt.tag());
        assert_eq!(entries[2].frame_receipt, third_frame_receipt.tag());
        assert_eq!(entries[3].frame_receipt, fourth_frame_receipt.tag());
        assert_eq!(
            entries[3].captured_frame,
            post_read_facts.captured.map(|frame| frame.tag)
        );
        assert_eq!(journal.state().artifact_head(), Some(fourth.artifact));
    }

    #[test]
    fn empty_journal_is_a_reopenable_typed_inventory() {
        let path = TempJournalPath::new("empty");
        let (journal, created) =
            DisplayArtifactJournal::create_new(&path.0, StorageLimits::DEFAULT).unwrap();
        assert_eq!(created.file_bytes, 0);
        assert_eq!(created.sync_evidence, SyncEvidence::OsSyncAllReturnedOk);
        assert!(journal.state().is_empty());
        assert_eq!(journal.carrier_head(), None);
        drop(journal);

        let OpenDisplayArtifactJournal::Ready {
            journal,
            carrier_report,
        } = DisplayArtifactJournal::open_existing(
            &path.0,
            StorageLimits::DEFAULT,
            AnchorPolicy::UnanchoredLocal,
        )
        .unwrap()
        else {
            panic!("empty typed journal required recovery")
        };
        assert!(journal.state().is_empty());
        assert_eq!(carrier_report.valid_record_count, 0);
        assert_eq!(carrier_report.valid_prefix_bytes, 0);
    }

    #[test]
    fn synced_artifacts_reopen_with_distinct_physical_and_display_parent_chains() {
        let path = TempJournalPath::new("reopen");
        let (first, second) = artifact_pair();
        assert_eq!(
            first.tag().to_string(),
            "#bb.display-artifact.sha256.fbd5hyvucb3zycqdqletiyrasrflufmnkjuo4zkdkd6iddprjs6q"
        );
        let (mut journal, _) =
            DisplayArtifactJournal::create_new(&path.0, StorageLimits::DEFAULT).unwrap();
        let first_append = journal.append_artifact(&first).unwrap();
        let second_append = journal.append_artifact(&second).unwrap();

        assert_eq!(first_append.carrier_append.parent, None);
        assert_eq!(first.receipt().parent(), None);
        assert_eq!(
            second_append.carrier_append.parent,
            Some(first_append.carrier_append.record_tag)
        );
        assert_eq!(second.receipt().parent(), Some(first.receipt().tag()));
        assert_ne!(
            second_append.carrier_append.parent,
            second.receipt().parent()
        );
        assert_ne!(second.receipt().parent(), Some(first.tag()));
        assert_eq!(
            second_append.carrier_append.sync_evidence,
            SyncEvidence::OsSyncAllReturnedOk
        );
        assert_eq!(
            second_append.carrier_append.appended_bytes_evidence,
            AppendedBytesEvidence::OsVisibleBytesRereadEqual
        );

        let expected_state = journal.state().clone();
        let expected_carrier_head = journal.carrier_head();
        let expected_bytes = journal.carrier_valid_bytes();
        let required = second_append.carrier_append.record_tag;
        drop(journal);

        let OpenDisplayArtifactJournal::Ready {
            mut journal,
            carrier_report,
        } = DisplayArtifactJournal::open_existing(
            &path.0,
            StorageLimits::DEFAULT,
            AnchorPolicy::RequireAncestor(required),
        )
        .unwrap()
        else {
            panic!("synchronized typed journal required recovery")
        };
        assert_eq!(journal.state(), &expected_state);
        assert_eq!(journal.carrier_head(), expected_carrier_head);
        assert_eq!(journal.carrier_valid_bytes(), expected_bytes);
        assert_eq!(journal.state().artifact_head(), Some(second.tag()));
        assert_eq!(
            journal.state().display_receipt_head(),
            Some(second.receipt().tag())
        );
        assert_eq!(carrier_report.valid_record_count, 2);
        assert!(carrier_report.required_ancestor_seen);

        let entries = journal.state().entries();
        assert_eq!(entries[0].logical_payload_bytes, 457);
        assert_eq!(entries[0].start_offset, 0);
        assert_eq!(entries[0].end_offset, entries[0].physical_record_bytes);
        assert_eq!(entries[1].start_offset, entries[0].end_offset);
        assert_eq!(entries[1].end_offset, expected_bytes);
        assert_eq!(entries[1].storage_parent, Some(entries[0].storage_record));
        assert_eq!(entries[1].display_parent, Some(entries[0].display_receipt));
        assert_eq!(
            entries[1].frame_receipt_parent,
            Some(entries[0].frame_receipt)
        );
        assert_eq!(entries[1].captured_form, CapturedPixelForm::Inline);
        for ((entry, artifact), append) in entries
            .iter()
            .zip([&first, &second])
            .zip([&first_append, &second_append])
        {
            let display_receipt = artifact.receipt();
            let frame_receipt = display_receipt.expected_receipt();
            assert_eq!(entry.storage_record, append.carrier_append.record_tag);
            assert_eq!(entry.storage_parent, append.carrier_append.parent);
            assert_eq!(entry.artifact, artifact.tag());
            assert_eq!(entry.display_receipt, display_receipt.tag());
            assert_eq!(entry.display_parent, display_receipt.parent());
            assert_eq!(entry.frame_receipt, frame_receipt.tag());
            assert_eq!(entry.frame_receipt_parent, frame_receipt.parent());
            assert_eq!(entry.tick_outcome, frame_receipt.outcome());
            assert_eq!(entry.frame_input, frame_receipt.input());
            assert_eq!(entry.expected_frame, frame_receipt.frame());
            assert_eq!(
                entry.captured_frame,
                display_receipt
                    .facts()
                    .captured
                    .map(|observation| observation.tag)
            );
            assert_eq!(entry.policy, display_receipt.policy());
            assert_eq!(entry.captured_form, artifact.captured_form());
            assert_eq!(entry.logical_payload_bytes, artifact.encoded_len() as u64);
            assert_eq!(
                entry.physical_record_bytes,
                append.carrier_append.encoded_bytes
            );
            assert_eq!(entry.start_offset, append.carrier_append.start_offset);
            assert_eq!(entry.end_offset, append.carrier_append.end_offset);
        }

        let expected = frame(&EXPECTED_PIXELS);
        let third_frame_receipt = FrameReceiptV2::new(
            Some(second.receipt().expected_receipt().tag()),
            FrameStream::Composite,
            tag(TagKind::TickOutcome, b"outcome-3"),
            tag(TagKind::FrameInput, b"input-3"),
            expected.content_tag(),
        )
        .unwrap();
        let third_receipt = journal
            .prepare_next_receipt(
                third_frame_receipt,
                2,
                1,
                complete_facts(expected, expected),
            )
            .unwrap();
        assert_eq!(third_receipt.parent(), Some(second.receipt().tag()));
        let third = DisplayCaptureArtifact::new(third_receipt, expected, Some(expected)).unwrap();
        journal.append_artifact(&third).unwrap();
        assert_eq!(journal.state().artifact_head(), Some(third.tag()));
        assert_eq!(journal.state().len(), 3);
    }

    #[test]
    fn absent_capture_inventory_keeps_reference_only_and_embedded_roles_distinct() {
        let path = TempJournalPath::new("absent-inventory");
        let artifact = absent_artifact();
        let (mut journal, _) =
            DisplayArtifactJournal::create_new(&path.0, StorageLimits::DEFAULT).unwrap();
        journal.append_artifact(&artifact).unwrap();

        let [entry] = journal.state().entries() else {
            panic!("one append must publish one inventory entry")
        };
        let display_receipt = artifact.receipt();
        let frame_receipt = display_receipt.expected_receipt();
        assert_eq!(entry.artifact, artifact.tag());
        assert_eq!(entry.display_receipt, display_receipt.tag());
        assert_eq!(entry.frame_receipt, frame_receipt.tag());
        assert_eq!(entry.expected_frame, frame_receipt.frame());
        assert_eq!(entry.tick_outcome, frame_receipt.outcome());
        assert_eq!(entry.frame_input, frame_receipt.input());
        assert_eq!(entry.policy, display_receipt.policy());
        assert_eq!(entry.captured_frame, None);
        assert_eq!(entry.captured_form, CapturedPixelForm::Absent);
    }

    #[test]
    fn public_replay_indexes_kind_schema_codec_and_storage_parent_failures() {
        let (first, second) = artifact_pair();
        let first_payload = first.encode_canonical().unwrap();
        let second_payload = second.encode_canonical().unwrap();

        let wrong_kind = StorageRecord::new(
            None,
            TagKind::World,
            DISPLAY_CAPTURE_ARTIFACT_SCHEMA,
            &first_payload,
        )
        .unwrap();
        assert!(matches!(
            replay_display_artifact_records(&[wrong_kind]),
            Err(DisplayArtifactSemanticError::WrongKind {
                record_index: 0,
                actual: TagKind::World,
            })
        ));

        let wrong_schema = StorageRecord::new(
            None,
            TagKind::DisplayArtifact,
            b"bitblock/wrong-display-artifact/v1",
            &first_payload,
        )
        .unwrap();
        assert!(matches!(
            replay_display_artifact_records(&[wrong_schema]),
            Err(DisplayArtifactSemanticError::WrongSchema {
                record_index: 0,
                ..
            })
        ));

        let bad_codec = StorageRecord::new(
            None,
            TagKind::DisplayArtifact,
            DISPLAY_CAPTURE_ARTIFACT_SCHEMA,
            b"not-an-artifact",
        )
        .unwrap();
        assert!(matches!(
            replay_display_artifact_records(&[bad_codec]),
            Err(DisplayArtifactSemanticError::ArtifactCodec {
                record_index: 0,
                ..
            })
        ));

        let first_record = StorageRecord::new(
            None,
            TagKind::DisplayArtifact,
            DISPLAY_CAPTURE_ARTIFACT_SCHEMA,
            &first_payload,
        )
        .unwrap();
        let second_record = StorageRecord::new(
            None,
            TagKind::DisplayArtifact,
            DISPLAY_CAPTURE_ARTIFACT_SCHEMA,
            &second_payload,
        )
        .unwrap();
        assert!(matches!(
            replay_display_artifact_records(&[first_record, second_record]),
            Err(DisplayArtifactSemanticError::StorageParentMismatch {
                record_index: 1,
                expected: Some(_),
                actual: None,
            })
        ));
    }

    #[test]
    fn semantic_parent_refusal_happens_before_any_physical_append() {
        let path = TempJournalPath::new("parent-preflight");
        let (first, _) = artifact_pair();
        let wrong_parent = standalone_artifact();
        let (mut journal, _) =
            DisplayArtifactJournal::create_new(&path.0, StorageLimits::DEFAULT).unwrap();
        journal.append_artifact(&first).unwrap();
        let old_state = journal.state().clone();
        let old_head = journal.carrier_head();
        let old_bytes = journal.carrier_valid_bytes();

        assert!(matches!(
            journal.append_artifact(&wrong_parent),
            Err(DisplayArtifactStoreError::Semantic(
                DisplayArtifactSemanticError::DisplayParentMismatch {
                    record_index: 1,
                    expected: Some(expected),
                    actual: None,
                }
            )) if expected == first.receipt().tag()
        ));
        assert_eq!(journal.state(), &old_state);
        assert_eq!(journal.carrier_head(), old_head);
        assert_eq!(journal.carrier_valid_bytes(), old_bytes);
        assert!(!journal.is_poisoned());
    }

    #[test]
    fn carrier_valid_outer_hashes_do_not_repair_a_false_display_parent() {
        let path = TempJournalPath::new("semantic-parent");
        let first = standalone_artifact();
        let false_successor = standalone_artifact();
        let first_payload = first.encode_canonical().unwrap();
        let second_payload = false_successor.encode_canonical().unwrap();
        let (mut carrier, _) = Journal::create_new(&path.0, StorageLimits::DEFAULT).unwrap();
        carrier
            .append_synced(
                TagKind::DisplayArtifact,
                DISPLAY_CAPTURE_ARTIFACT_SCHEMA,
                &first_payload,
            )
            .unwrap();
        carrier
            .append_synced(
                TagKind::DisplayArtifact,
                DISPLAY_CAPTURE_ARTIFACT_SCHEMA,
                &second_payload,
            )
            .unwrap();
        drop(carrier);
        let before = fs::read(&path.0).unwrap();

        let error = match DisplayArtifactJournal::open_existing(
            &path.0,
            StorageLimits::DEFAULT,
            AnchorPolicy::UnanchoredLocal,
        ) {
            Err(error) => error,
            Ok(_) => panic!("carrier-valid false semantic parent was admitted"),
        };
        assert!(matches!(
            error,
            DisplayArtifactStoreError::Semantic(
                DisplayArtifactSemanticError::DisplayParentMismatch {
                    record_index: 1,
                    expected: Some(_),
                    actual: None,
                }
            )
        ));
        assert_eq!(fs::read(&path.0).unwrap(), before);
    }

    #[test]
    fn semantic_false_recovery_prefix_exposes_no_typed_discard_handle() {
        let path = TempJournalPath::new("semantic-recovery-prefix");
        let first = standalone_artifact();
        let false_successor = standalone_artifact();
        let first_payload = first.encode_canonical().unwrap();
        let second_payload = false_successor.encode_canonical().unwrap();
        let (mut carrier, _) = Journal::create_new(&path.0, StorageLimits::DEFAULT).unwrap();
        carrier
            .append_synced(
                TagKind::DisplayArtifact,
                DISPLAY_CAPTURE_ARTIFACT_SCHEMA,
                &first_payload,
            )
            .unwrap();
        carrier
            .append_synced(
                TagKind::DisplayArtifact,
                DISPLAY_CAPTURE_ARTIFACT_SCHEMA,
                &second_payload,
            )
            .unwrap();
        drop(carrier);
        let mut before = fs::read(&path.0).unwrap();
        before.push(0xff);
        fs::write(&path.0, &before).unwrap();

        let error = match DisplayArtifactJournal::open_existing(
            &path.0,
            StorageLimits::DEFAULT,
            AnchorPolicy::UnanchoredLocal,
        ) {
            Err(error) => error,
            Ok(_) => panic!("semantically false recovery prefix exposed a typed handle"),
        };
        assert!(matches!(
            error,
            DisplayArtifactStoreError::Semantic(
                DisplayArtifactSemanticError::DisplayParentMismatch {
                    record_index: 1,
                    expected: Some(_),
                    actual: None,
                }
            )
        ));
        assert_eq!(fs::read(&path.0).unwrap(), before);
    }

    #[test]
    fn corrupt_physical_suffix_is_preserved_then_explicitly_discarded_and_replayed() {
        let path = TempJournalPath::new("recovery");
        let (first, second) = artifact_pair();
        let (mut journal, _) =
            DisplayArtifactJournal::create_new(&path.0, StorageLimits::DEFAULT).unwrap();
        let first_append = journal.append_artifact(&first).unwrap();
        let second_append = journal.append_artifact(&second).unwrap();
        let second_start = second_append.carrier_append.start_offset as usize;
        drop(journal);

        let mut physical = fs::read(&path.0).unwrap();
        *physical.last_mut().unwrap() ^= 1;
        fs::write(&path.0, &physical).unwrap();
        let expected_tail = physical[second_start..].to_vec();

        let OpenDisplayArtifactJournal::NeedsRecovery { journal } =
            DisplayArtifactJournal::open_existing(
                &path.0,
                StorageLimits::DEFAULT,
                AnchorPolicy::RequireAncestor(first_append.carrier_append.record_tag),
            )
            .unwrap()
        else {
            panic!("corrupt physical suffix was exposed as ready")
        };
        let mut journal = *journal;
        assert_eq!(journal.state().len(), 1);
        assert_eq!(journal.state().artifact_head(), Some(first.tag()));
        assert_eq!(
            journal.report().rejection_class,
            RejectionClass::StructuralOrAuthentication
        );
        assert_eq!(journal.export_rejected_tail().unwrap(), expected_tail);

        let (mut journal, discard) = journal.discard_unverified_tail().unwrap();
        assert_eq!(discard.retained_prefix_bytes, second_start as u64);
        assert_eq!(discard.discarded_bytes, expected_tail.len() as u64);
        assert_eq!(journal.state().len(), 1);
        journal.append_artifact(&second).unwrap();
        assert_eq!(journal.state().artifact_head(), Some(second.tag()));
    }

    #[test]
    fn payload_limit_refusal_is_policy_not_poison_or_mutation() {
        let path = TempJournalPath::new("payload-limit");
        let exact_path = TempJournalPath::new("payload-limit-exact");
        let artifact = standalone_artifact();
        let max_payload = artifact.encoded_len() - 1;
        let limits = StorageLimits::new(
            MAX_STORAGE_RECORD_BYTES,
            MAX_STORAGE_SCHEMA_BYTES,
            max_payload,
            MAX_STORAGE_RECORDS,
            MAX_STORAGE_STREAM_BYTES,
        )
        .unwrap();
        let (mut journal, _) = DisplayArtifactJournal::create_new(&path.0, limits).unwrap();

        assert!(matches!(
            journal.append_artifact(&artifact),
            Err(DisplayArtifactStoreError::StoragePreflight(
                StorageError::PayloadTooLarge { actual: 457, max }
            )) if max == max_payload
        ));
        assert!(journal.state().is_empty());
        assert_eq!(journal.carrier_head(), None);
        assert_eq!(journal.carrier_valid_bytes(), 0);
        assert!(!journal.is_poisoned());
        assert_eq!(fs::metadata(&path.0).unwrap().len(), 0);

        let exact_limits = StorageLimits::new(
            MAX_STORAGE_RECORD_BYTES,
            MAX_STORAGE_SCHEMA_BYTES,
            artifact.encoded_len(),
            MAX_STORAGE_RECORDS,
            MAX_STORAGE_STREAM_BYTES,
        )
        .unwrap();
        let (mut exact, _) =
            DisplayArtifactJournal::create_new(&exact_path.0, exact_limits).unwrap();
        let appended = exact.append_artifact(&artifact).unwrap();
        assert_eq!(
            appended.logical_payload_bytes,
            artifact.encoded_len() as u64
        );
        assert_eq!(exact.state().artifact_head(), Some(artifact.tag()));
    }

    #[test]
    fn exact_boundary_truncation_is_only_detected_with_an_external_storage_anchor() {
        let path = TempJournalPath::new("anchor");
        let (first, second) = artifact_pair();
        let (mut journal, _) =
            DisplayArtifactJournal::create_new(&path.0, StorageLimits::DEFAULT).unwrap();
        let first_append = journal.append_artifact(&first).unwrap();
        let second_append = journal.append_artifact(&second).unwrap();
        drop(journal);

        OpenOptions::new()
            .write(true)
            .open(&path.0)
            .unwrap()
            .set_len(first_append.carrier_append.end_offset)
            .unwrap();

        let OpenDisplayArtifactJournal::Ready { journal, .. } =
            DisplayArtifactJournal::open_existing(
                &path.0,
                StorageLimits::DEFAULT,
                AnchorPolicy::UnanchoredLocal,
            )
            .unwrap()
        else {
            panic!("canonical boundary truncation is locally corrupt")
        };
        assert_eq!(journal.state().artifact_head(), Some(first.tag()));
        drop(journal);

        assert!(matches!(
            DisplayArtifactJournal::open_existing(
                &path.0,
                StorageLimits::DEFAULT,
                AnchorPolicy::RequireAncestor(second_append.carrier_append.record_tag),
            ),
            Err(DisplayArtifactStoreError::Journal(
                JournalError::RequiredAncestorMissing { required, .. }
            )) if required == second_append.carrier_append.record_tag
        ));
    }

    #[test]
    fn legacy_and_presentation_outputs_share_one_exact_mixed_replay_chain() {
        for (ordinal, (wrap_first, wrap_second)) in
            [(false, false), (false, true), (true, false), (true, true)]
                .into_iter()
                .enumerate()
        {
            let path = TempJournalPath::new(&format!("mixed-{ordinal}"));
            let (first, second) = artifact_pair();
            let first_bundle = presentation_output(first.clone());
            let second_bundle = presentation_output(second.clone());
            let expected_first_stored = if wrap_first {
                first_bundle.tag()
            } else {
                first.tag()
            };
            let expected_second_stored = if wrap_second {
                second_bundle.tag()
            } else {
                second.tag()
            };

            let (mut journal, _) =
                DisplayArtifactJournal::create_new(&path.0, StorageLimits::DEFAULT).unwrap();
            if wrap_first {
                journal.append_presentation_output(&first_bundle).unwrap();
            } else {
                journal.append_artifact(&first).unwrap();
            }
            if wrap_second {
                journal.append_presentation_output(&second_bundle).unwrap();
            } else {
                journal.append_artifact(&second).unwrap();
            }
            let storage_head = journal.state().storage_head().unwrap();
            drop(journal);

            let OpenDisplayArtifactJournal::Ready { journal, .. } =
                DisplayArtifactJournal::open_existing(
                    &path.0,
                    StorageLimits::DEFAULT,
                    AnchorPolicy::RequireAncestor(storage_head),
                )
                .unwrap()
            else {
                panic!("an exact mixed prefix unexpectedly required recovery")
            };
            let entries = journal.state().entries();
            assert_eq!(entries.len(), 2);
            assert_eq!(entries[0].stored_object, expected_first_stored);
            assert_eq!(entries[1].stored_object, expected_second_stored);
            assert_eq!(entries[0].artifact, first.tag());
            assert_eq!(entries[1].artifact, second.tag());
            assert_eq!(entries[1].storage_parent, Some(entries[0].storage_record));
            assert_eq!(entries[1].display_parent, Some(entries[0].display_receipt));
            assert_eq!(entries[0].timing_artifact.is_some(), wrap_first);
            assert_eq!(entries[1].timing_artifact.is_some(), wrap_second);
            assert_eq!(entries[0].topology_before.is_some(), wrap_first);
            assert_eq!(entries[1].topology_after.is_some(), wrap_second);
            assert_eq!(
                entries[0].route_stability,
                wrap_first.then_some(RouteStability::Unavailable)
            );
            assert_eq!(
                entries[1].route_stability,
                wrap_second.then_some(RouteStability::Unavailable)
            );
            assert_eq!(journal.state().artifact_head(), Some(second.tag()));
            assert_eq!(
                journal.state().stored_object_head(),
                Some(expected_second_stored)
            );
        }
    }

    #[test]
    fn presentation_output_payload_limit_refuses_before_io_without_poison() {
        let path = TempJournalPath::new("presentation-payload-limit");
        let artifact = standalone_artifact();
        let presentation = presentation_output(artifact);
        let max_payload = presentation.encoded_len() - 1;
        let limits = StorageLimits::new(
            MAX_STORAGE_RECORD_BYTES,
            MAX_STORAGE_SCHEMA_BYTES,
            max_payload,
            MAX_STORAGE_RECORDS,
            MAX_STORAGE_STREAM_BYTES,
        )
        .unwrap();
        let (mut journal, _) = DisplayArtifactJournal::create_new(&path.0, limits).unwrap();

        assert!(matches!(
            journal.append_presentation_output(&presentation),
            Err(DisplayArtifactStoreError::StoragePreflight(
                StorageError::PayloadTooLarge { actual, max }
            )) if actual == presentation.encoded_len() && max == max_payload
        ));
        assert!(journal.state().is_empty());
        assert_eq!(journal.carrier_head(), None);
        assert_eq!(journal.carrier_valid_bytes(), 0);
        assert!(!journal.is_poisoned());
        assert_eq!(fs::metadata(&path.0).unwrap().len(), 0);
    }
}
