//! Typed persistent admission for coordinate history.
//!
//! The generic journal proves only canonical storage-envelope structure and content tags. This
//! layer additionally requires the first object to bind the caller-supplied initial world and
//! replays every later receipt through Bitblock's exact edit, event, mint, burn, no-op, bedrock,
//! world-tag, and receipt-tag semantics before exposing writable state.
//!
//! This remains a local unsigned history. It does not prove that the supplied initial world is
//! authoritative, that the file is the latest valid prefix, or that any party agreed to it.

use std::fmt;
use std::path::Path;

use crate::history::{
    GENESIS_SCHEMA, History, HistoryError, HistoryReceiptCodecError, HistoryReceiptPayload,
    RECEIPT_SCHEMA, Receipt, canonical_receipt_payload_len,
};
use crate::journal::{
    AnchorPolicy, AppendSyncReceipt, CreateReceipt, Journal, JournalError, OpenJournal,
    RecoveryJournal, RecoveryReport, ScanReport, TailDiscardReceipt,
};
use crate::program::Edit;
use crate::protocol::{ContentTag, TagKind};
use crate::storage::{StorageChain, StorageError, StorageLimits, StorageRecord};
use crate::world::{World, is_bedrock};

/// World and provenance state derived from a complete semantic replay.
#[derive(Clone, Debug)]
pub struct ReplayedHistory {
    world: World,
    history: History,
    usage: HistoryReplayUsage,
}

impl ReplayedHistory {
    /// Returns the final `World` reconstructed by semantic replay.
    pub const fn world(&self) -> &World {
        &self.world
    }

    /// Returns the provenance `History` reconstructed by semantic replay.
    pub const fn history(&self) -> &History {
        &self.history
    }

    /// Consumes the replay result and returns the reconstructed `World` and `History`.
    pub fn into_parts(self) -> (World, History) {
        (self.world, self.history)
    }

    /// Returns the cumulative semantic replay work metrics.
    pub const fn usage(&self) -> HistoryReplayUsage {
        self.usage
    }
}

/// Local work and memory policy for semantic replay.
///
/// These are admission limits, not coordinate-history semantics. A file refused only by this
/// policy can be retried under an explicitly larger policy; it is never classified as corrupt or
/// eligible for tail deletion merely because a local budget was reached.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HistoryReplayLimits {
    max_receipts: usize,
    max_edits_per_receipt: usize,
    max_total_edits: usize,
    max_occupied_bits: usize,
    max_allocated_chunks: usize,
    max_world_tag_coordinate_visits: usize,
}

impl HistoryReplayLimits {
    /// Default local work and memory limits for semantic coordinate-history replay.
    pub const DEFAULT: Self = Self {
        max_receipts: 16_384,
        max_edits_per_receipt: 65_536,
        max_total_edits: 262_144,
        max_occupied_bits: 65_536,
        max_allocated_chunks: 16_384,
        max_world_tag_coordinate_visits: 4_194_304,
    };

    /// Constructs custom local semantic-replay admission limits.
    pub const fn new(
        max_receipts: usize,
        max_edits_per_receipt: usize,
        max_total_edits: usize,
        max_occupied_bits: usize,
        max_allocated_chunks: usize,
        max_world_tag_coordinate_visits: usize,
    ) -> Self {
        Self {
            max_receipts,
            max_edits_per_receipt,
            max_total_edits,
            max_occupied_bits,
            max_allocated_chunks,
            max_world_tag_coordinate_visits,
        }
    }

    /// Returns the maximum number of post-genesis receipts admitted during replay.
    pub const fn max_receipts(self) -> usize {
        self.max_receipts
    }

    /// Returns the maximum number of edits allowed within a single receipt.
    pub const fn max_edits_per_receipt(self) -> usize {
        self.max_edits_per_receipt
    }

    /// Returns the maximum cumulative number of edits allowed across all replayed receipts.
    pub const fn max_total_edits(self) -> usize {
        self.max_total_edits
    }

    /// Returns the maximum number of simultaneously occupied coordinates allowed.
    pub const fn max_occupied_bits(self) -> usize {
        self.max_occupied_bits
    }

    /// Returns the maximum number of allocated world chunks allowed during replay.
    pub const fn max_allocated_chunks(self) -> usize {
        self.max_allocated_chunks
    }

    /// Returns the maximum cumulative coordinate visits allowed when hashing world tags.
    pub const fn max_world_tag_coordinate_visits(self) -> usize {
        self.max_world_tag_coordinate_visits
    }
}

impl Default for HistoryReplayLimits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Exact semantic work accumulated while deriving a history state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HistoryReplayUsage {
    pub receipt_count: usize,
    pub total_edits: usize,
    pub peak_occupied_bits: usize,
    pub peak_allocated_chunks: usize,
    pub world_tag_coordinate_visits: usize,
}

/// A borrowed in-memory history admitted by bounded semantic replay.
///
/// The token retains the replay-derived final world and exact usage so downstream policy code
/// does not need to trust cached effects merely because it received a `History` reference.
#[derive(Clone, Debug)]
pub struct VerifiedInMemoryHistory<'a> {
    history: &'a History,
    final_world: World,
    usage: HistoryReplayUsage,
}

impl<'a> VerifiedInMemoryHistory<'a> {
    /// Returns the borrowed `History` that passed bounded semantic replay.
    pub const fn history(&self) -> &'a History {
        self.history
    }

    /// Returns the final `World` state reconstructed during verification.
    pub const fn final_world(&self) -> &World {
        &self.final_world
    }

    /// Returns the semantic replay work accumulated during verification.
    pub const fn usage(&self) -> HistoryReplayUsage {
        self.usage
    }
}

/// Failure modes when verifying an in-memory `History` under replay limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InMemoryHistoryVerificationError {
    Semantic(HistoryError),
    Limit(HistoryReplayLimitError),
}

impl fmt::Display for InMemoryHistoryVerificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Semantic(source) => write!(formatter, "in-memory history failed: {source}"),
            Self::Limit(source) => write!(formatter, "in-memory history policy refused: {source}"),
        }
    }
}

impl std::error::Error for InMemoryHistoryVerificationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Semantic(source) => Some(source),
            Self::Limit(source) => Some(source),
        }
    }
}

impl From<HistoryError> for InMemoryHistoryVerificationError {
    fn from(source: HistoryError) -> Self {
        Self::Semantic(source)
    }
}

impl From<HistoryReplayLimitError> for InMemoryHistoryVerificationError {
    fn from(source: HistoryReplayLimitError) -> Self {
        Self::Limit(source)
    }
}

/// A semantically admitted, writable coordinate-history journal.
pub struct HistoryJournal {
    carrier: Journal,
    state: ReplayedHistory,
    limits: StorageLimits,
    replay_limits: HistoryReplayLimits,
    semantic_poisoned: bool,
}

/// A non-writable physical recovery state whose retained prefix also passed semantic replay.
pub struct HistoryRecoveryJournal {
    carrier: RecoveryJournal,
    state: ReplayedHistory,
    initial_world: World,
    limits: StorageLimits,
    replay_limits: HistoryReplayLimits,
}

/// Typed outcome of opening an existing physical journal.
pub enum OpenHistoryJournal {
    Ready {
        journal: Box<HistoryJournal>,
        carrier_report: ScanReport,
    },
    NeedsRecovery {
        journal: Box<HistoryRecoveryJournal>,
    },
}

/// Evidence returned only after the new file and its semantic genesis were synchronized.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryCreateReceipt {
    pub file_creation: CreateReceipt,
    pub genesis_append: AppendSyncReceipt,
    pub genesis: ContentTag,
    pub initial_world: ContentTag,
}

/// Evidence returned after one receipt was synchronized, reread, and then published in memory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryAppendReceipt {
    pub carrier_append: AppendSyncReceipt,
    pub receipt: ContentTag,
    pub post_world: ContentTag,
    pub tick: u64,
    pub edit_count: usize,
    pub state_changes: usize,
}

/// Semantic validation failure while replaying coordinate-history storage records.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistorySemanticError {
    MissingGenesis,
    StorageParentMismatch {
        record_index: usize,
        expected: Option<ContentTag>,
        actual: Option<ContentTag>,
    },
    GenesisKind {
        actual: TagKind,
    },
    GenesisSchema {
        actual: Vec<u8>,
    },
    GenesisPayloadLength {
        actual: usize,
    },
    GenesisWorldMismatch {
        expected: [u8; 32],
        actual: [u8; 32],
    },
    GenesisObjectTagMismatch {
        expected: ContentTag,
        actual: ContentTag,
    },
    ReceiptKind {
        record_index: usize,
        actual: TagKind,
    },
    ReceiptSchema {
        record_index: usize,
        actual: Vec<u8>,
    },
    UnexpectedGenesis {
        record_index: usize,
    },
    ReceiptCodec {
        record_index: usize,
        source: HistoryReceiptCodecError,
    },
    ReceiptObjectTagMismatch {
        record_index: usize,
        expected: ContentTag,
        actual: ContentTag,
    },
    ReceiptParentMismatch {
        record_index: usize,
        expected: ContentTag,
        actual: ContentTag,
    },
    ReceiptTickNotStrict {
        record_index: usize,
        previous: u64,
        actual: u64,
    },
    ReceiptPostWorldMismatch {
        record_index: usize,
        expected: ContentTag,
        actual: ContentTag,
    },
    ReceiptFinalEventMismatch {
        record_index: usize,
        expected: ContentTag,
        actual: ContentTag,
    },
    ReceiptTagMismatch {
        record_index: usize,
        expected: ContentTag,
        actual: ContentTag,
    },
}

impl fmt::Display for HistorySemanticError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingGenesis => formatter.write_str("coordinate journal has no genesis record"),
            Self::StorageParentMismatch {
                record_index,
                expected,
                actual,
            } => write!(
                formatter,
                "coordinate carrier record {record_index} parent is {actual:?}, expected {expected:?}"
            ),
            Self::GenesisKind { actual } => write!(
                formatter,
                "coordinate genesis must be receipt, received {}",
                actual.name()
            ),
            Self::GenesisSchema { actual } => write!(
                formatter,
                "coordinate genesis schema is {:?}, expected {:?}",
                String::from_utf8_lossy(actual),
                String::from_utf8_lossy(GENESIS_SCHEMA)
            ),
            Self::GenesisPayloadLength { actual } => {
                write!(
                    formatter,
                    "coordinate genesis payload has {actual} bytes, expected 32"
                )
            }
            Self::GenesisWorldMismatch { expected, actual } => write!(
                formatter,
                "coordinate genesis world digest differs: expected {expected:02x?}, found {actual:02x?}"
            ),
            Self::GenesisObjectTagMismatch { expected, actual } => write!(
                formatter,
                "coordinate genesis object tag differs: expected {expected}, found {actual}"
            ),
            Self::ReceiptKind {
                record_index,
                actual,
            } => write!(
                formatter,
                "coordinate record {record_index} must be receipt, received {}",
                actual.name()
            ),
            Self::ReceiptSchema {
                record_index,
                actual,
            } => write!(
                formatter,
                "coordinate record {record_index} schema is {:?}, expected {:?}",
                String::from_utf8_lossy(actual),
                String::from_utf8_lossy(RECEIPT_SCHEMA)
            ),
            Self::UnexpectedGenesis { record_index } => write!(
                formatter,
                "coordinate record {record_index} repeats the genesis schema"
            ),
            Self::ReceiptCodec {
                record_index,
                source,
            } => write!(
                formatter,
                "coordinate record {record_index} is not a receipt: {source}"
            ),
            Self::ReceiptObjectTagMismatch {
                record_index,
                expected,
                actual,
            } => write!(
                formatter,
                "coordinate record {record_index} payload names {expected}, carrier names {actual}"
            ),
            Self::ReceiptParentMismatch {
                record_index,
                expected,
                actual,
            } => write!(
                formatter,
                "coordinate record {record_index} semantic parent is {actual}, expected {expected}"
            ),
            Self::ReceiptTickNotStrict {
                record_index,
                previous,
                actual,
            } => write!(
                formatter,
                "coordinate record {record_index} tick {actual} does not advance after {previous}"
            ),
            Self::ReceiptPostWorldMismatch {
                record_index,
                expected,
                actual,
            } => write!(
                formatter,
                "coordinate record {record_index} post-world is {actual}, replay derived {expected}"
            ),
            Self::ReceiptFinalEventMismatch {
                record_index,
                expected,
                actual,
            } => write!(
                formatter,
                "coordinate record {record_index} final event is {actual}, replay derived {expected}"
            ),
            Self::ReceiptTagMismatch {
                record_index,
                expected,
                actual,
            } => write!(
                formatter,
                "coordinate record {record_index} receipt tag is {actual}, replay derived {expected}"
            ),
        }
    }
}

impl std::error::Error for HistorySemanticError {}

/// Refusal when semantic history replay exceeds a configured local resource bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryReplayLimitError {
    ReceiptCount {
        actual: usize,
        max: usize,
    },
    EditsPerReceipt {
        record_index: usize,
        actual: usize,
        max: usize,
    },
    TotalEdits {
        record_index: usize,
        actual: usize,
        max: usize,
    },
    OccupiedBits {
        record_index: usize,
        actual: usize,
        max: usize,
    },
    AllocatedChunks {
        record_index: usize,
        actual: usize,
        max: usize,
    },
    WorldTagCoordinateVisits {
        record_index: usize,
        actual: usize,
        max: usize,
    },
    ArithmeticOverflow {
        field: &'static str,
    },
}

impl fmt::Display for HistoryReplayLimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::ReceiptCount { actual, max } => {
                write!(
                    formatter,
                    "history has {actual} receipts, local limit is {max}"
                )
            }
            Self::EditsPerReceipt {
                record_index,
                actual,
                max,
            } => write!(
                formatter,
                "history record {record_index} has {actual} edits, local per-receipt limit is {max}"
            ),
            Self::TotalEdits {
                record_index,
                actual,
                max,
            } => write!(
                formatter,
                "history reaches {actual} edits at record {record_index}, local total limit is {max}"
            ),
            Self::OccupiedBits {
                record_index,
                actual,
                max,
            } => write!(
                formatter,
                "history reaches {actual} occupied bits at record {record_index}, local limit is {max}"
            ),
            Self::AllocatedChunks {
                record_index,
                actual,
                max,
            } => write!(
                formatter,
                "history reaches {actual} allocated chunks at record {record_index}, local limit is {max}"
            ),
            Self::WorldTagCoordinateVisits {
                record_index,
                actual,
                max,
            } => write!(
                formatter,
                "history reaches {actual} cumulative world-tag coordinate visits at record {record_index}, local limit is {max}"
            ),
            Self::ArithmeticOverflow { field } => {
                write!(formatter, "history replay {field} arithmetic overflowed")
            }
        }
    }
}

impl std::error::Error for HistoryReplayLimitError {}

/// Combined semantic or resource-limit failure during coordinate-history replay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryReplayError {
    Semantic(HistorySemanticError),
    Limit(HistoryReplayLimitError),
}

impl fmt::Display for HistoryReplayError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Semantic(source) => write!(formatter, "history semantics failed: {source}"),
            Self::Limit(source) => write!(formatter, "history replay policy refused: {source}"),
        }
    }
}

impl std::error::Error for HistoryReplayError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Semantic(source) => Some(source),
            Self::Limit(source) => Some(source),
        }
    }
}

impl From<HistorySemanticError> for HistoryReplayError {
    fn from(source: HistorySemanticError) -> Self {
        Self::Semantic(source)
    }
}

impl From<HistoryReplayLimitError> for HistoryReplayError {
    fn from(source: HistoryReplayLimitError) -> Self {
        Self::Limit(source)
    }
}

/// Error returned by persistent coordinate-history journal operations.
#[derive(Debug)]
pub enum HistoryStoreError {
    Journal(JournalError),
    GenesisInitializationFailed {
        source: JournalError,
    },
    StoragePreflight(StorageError),
    Semantic(HistorySemanticError),
    ReplayLimit(HistoryReplayLimitError),
    ReceiptLength(HistoryReceiptCodecError),
    ReceiptPayloadTooLarge {
        actual: usize,
        max: usize,
    },
    AppendObjectTagMismatch {
        expected: ContentTag,
        actual: ContentTag,
    },
    NonIncreasingAppendTick {
        previous: u64,
        actual: u64,
    },
    LocalBounds(crate::local_bounds::BoundsError),
    Poisoned,
}

impl fmt::Display for HistoryStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Journal(source) => {
                write!(formatter, "coordinate journal carrier failed: {source}")
            }
            Self::GenesisInitializationFailed { source } => write!(
                formatter,
                "coordinate genesis initialization failed after path creation: {source}; the path is preserved for explicit generic inspection"
            ),
            Self::StoragePreflight(source) => {
                write!(formatter, "coordinate journal preflight failed: {source}")
            }
            Self::Semantic(source) => {
                write!(formatter, "coordinate history replay failed: {source}")
            }
            Self::ReplayLimit(source) => {
                write!(
                    formatter,
                    "coordinate history replay policy refused: {source}"
                )
            }
            Self::ReceiptLength(source) => {
                write!(formatter, "coordinate receipt length failed: {source}")
            }
            Self::ReceiptPayloadTooLarge { actual, max } => write!(
                formatter,
                "coordinate receipt payload has {actual} bytes, local limit is {max}"
            ),
            Self::AppendObjectTagMismatch { expected, actual } => write!(
                formatter,
                "synchronized carrier object is {actual}, expected semantic receipt {expected}"
            ),
            Self::NonIncreasingAppendTick { previous, actual } => write!(
                formatter,
                "coordinate receipt tick {actual} does not advance after {previous}"
            ),
            Self::LocalBounds(source) => {
                write!(formatter, "local storage bounds rejected: {source}")
            }
            Self::Poisoned => formatter.write_str(
                "coordinate journal is poisoned; close and reopen through semantic replay",
            ),
        }
    }
}

impl std::error::Error for HistoryStoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Journal(source) => Some(source),
            Self::GenesisInitializationFailed { source } => Some(source),
            Self::StoragePreflight(source) => Some(source),
            Self::Semantic(source) => Some(source),
            Self::ReplayLimit(source) => Some(source),
            Self::ReceiptLength(source) => Some(source),
            Self::LocalBounds(source) => Some(source),
            _ => None,
        }
    }
}

impl From<crate::local_bounds::BoundsError> for HistoryStoreError {
    fn from(source: crate::local_bounds::BoundsError) -> Self {
        Self::LocalBounds(source)
    }
}

impl From<JournalError> for HistoryStoreError {
    fn from(source: JournalError) -> Self {
        Self::Journal(source)
    }
}

impl From<HistorySemanticError> for HistoryStoreError {
    fn from(source: HistorySemanticError) -> Self {
        Self::Semantic(source)
    }
}

impl From<HistoryReplayError> for HistoryStoreError {
    fn from(source: HistoryReplayError) -> Self {
        match source {
            HistoryReplayError::Semantic(source) => Self::Semantic(source),
            HistoryReplayError::Limit(source) => Self::ReplayLimit(source),
        }
    }
}

impl From<HistoryReplayLimitError> for HistoryStoreError {
    fn from(source: HistoryReplayLimitError) -> Self {
        Self::ReplayLimit(source)
    }
}

fn initial_replay_usage(
    initial_world: &World,
    limits: HistoryReplayLimits,
) -> Result<HistoryReplayUsage, HistoryReplayLimitError> {
    if initial_world.len() > limits.max_occupied_bits {
        return Err(HistoryReplayLimitError::OccupiedBits {
            record_index: 0,
            actual: initial_world.len(),
            max: limits.max_occupied_bits,
        });
    }
    if initial_world.allocated_chunk_count() > limits.max_allocated_chunks {
        return Err(HistoryReplayLimitError::AllocatedChunks {
            record_index: 0,
            actual: initial_world.allocated_chunk_count(),
            max: limits.max_allocated_chunks,
        });
    }
    if initial_world.len() > limits.max_world_tag_coordinate_visits {
        return Err(HistoryReplayLimitError::WorldTagCoordinateVisits {
            record_index: 0,
            actual: initial_world.len(),
            max: limits.max_world_tag_coordinate_visits,
        });
    }
    Ok(HistoryReplayUsage {
        receipt_count: 0,
        total_edits: 0,
        peak_occupied_bits: initial_world.len(),
        peak_allocated_chunks: initial_world.allocated_chunk_count(),
        world_tag_coordinate_visits: initial_world.len(),
    })
}

/// Replays one already materialized `History` under the same bounded world-work policy used by
/// the typed journal and returns a token only after all cached effects, tags, and provenance agree.
pub fn verify_in_memory_history_with_limits<'a>(
    initial_world: &World,
    claimed: &'a History,
    limits: HistoryReplayLimits,
) -> Result<VerifiedInMemoryHistory<'a>, InMemoryHistoryVerificationError> {
    if claimed.receipts().len() > limits.max_receipts {
        return Err(HistoryReplayLimitError::ReceiptCount {
            actual: claimed.receipts().len(),
            max: limits.max_receipts,
        }
        .into());
    }
    let mut usage = initial_replay_usage(initial_world, limits)?;
    let mut world = initial_world.clone();
    let mut rebuilt = History::new(&world);
    if rebuilt.genesis() != claimed.genesis() || rebuilt.genesis_world() != claimed.genesis_world()
    {
        return Err(HistoryError::GenesisMismatch.into());
    }

    for (receipt_index, expected) in claimed.receipts().iter().enumerate() {
        let record_index =
            receipt_index
                .checked_add(1)
                .ok_or(HistoryReplayLimitError::ArithmeticOverflow {
                    field: "record index",
                })?;
        let edit_count = expected.edits().len();
        if edit_count > limits.max_edits_per_receipt {
            return Err(HistoryReplayLimitError::EditsPerReceipt {
                record_index,
                actual: edit_count,
                max: limits.max_edits_per_receipt,
            }
            .into());
        }
        let next_total_edits = usage.total_edits.checked_add(edit_count).ok_or(
            HistoryReplayLimitError::ArithmeticOverflow {
                field: "total edit count",
            },
        )?;
        if next_total_edits > limits.max_total_edits {
            return Err(HistoryReplayLimitError::TotalEdits {
                record_index,
                actual: next_total_edits,
                max: limits.max_total_edits,
            }
            .into());
        }
        if expected.parent() != rebuilt.head()
            || expected.edits().is_empty()
            || rebuilt
                .receipts()
                .last()
                .is_some_and(|previous| expected.tick() <= previous.tick())
        {
            return Err(HistoryError::ReceiptMismatch {
                index: receipt_index,
            }
            .into());
        }

        let mut transaction = rebuilt.begin_tick(expected.tick());
        for &edit in expected.edits() {
            transaction.apply_edit(&rebuilt, &mut world, edit);
            if world.len() > limits.max_occupied_bits {
                return Err(HistoryReplayLimitError::OccupiedBits {
                    record_index,
                    actual: world.len(),
                    max: limits.max_occupied_bits,
                }
                .into());
            }
            if world.allocated_chunk_count() > limits.max_allocated_chunks {
                return Err(HistoryReplayLimitError::AllocatedChunks {
                    record_index,
                    actual: world.allocated_chunk_count(),
                    max: limits.max_allocated_chunks,
                }
                .into());
            }
            usage.peak_occupied_bits = usage.peak_occupied_bits.max(world.len());
            usage.peak_allocated_chunks = usage
                .peak_allocated_chunks
                .max(world.allocated_chunk_count());
        }
        if transaction.effects() != expected.effects() {
            return Err(HistoryError::EffectMismatch {
                index: receipt_index,
            }
            .into());
        }
        let next_world_tag_visits = usage
            .world_tag_coordinate_visits
            .checked_add(world.len())
            .ok_or(HistoryReplayLimitError::ArithmeticOverflow {
                field: "world-tag coordinate visit count",
            })?;
        if next_world_tag_visits > limits.max_world_tag_coordinate_visits {
            return Err(HistoryReplayLimitError::WorldTagCoordinateVisits {
                record_index,
                actual: next_world_tag_visits,
                max: limits.max_world_tag_coordinate_visits,
            }
            .into());
        }

        rebuilt.commit_tick(transaction, &world);
        let actual = rebuilt
            .receipts()
            .last()
            .expect("one nonempty transaction committed one rebuilt receipt");
        if actual != expected {
            return Err(HistoryError::ReceiptMismatch {
                index: receipt_index,
            }
            .into());
        }
        usage.receipt_count = usage.receipt_count.checked_add(1).ok_or(
            HistoryReplayLimitError::ArithmeticOverflow {
                field: "verified receipt count",
            },
        )?;
        usage.total_edits = next_total_edits;
        usage.world_tag_coordinate_visits = next_world_tag_visits;
    }

    if rebuilt.head() != claimed.head() {
        return Err(HistoryError::HeadMismatch.into());
    }
    if rebuilt.circulating_supply() != claimed.circulating_supply()
        || world
            .iter_on()
            .filter(|&coord| !is_bedrock(coord))
            .any(|coord| rebuilt.active_mint(coord) != claimed.active_mint(coord))
    {
        return Err(HistoryError::ProvenanceMismatch.into());
    }

    Ok(VerifiedInMemoryHistory {
        history: claimed,
        final_world: world,
        usage,
    })
}

/// Replays canonical carrier records under coordinate-history semantics.
///
/// The caller supplies the initial world. The first record commits its world tag but does not
/// materialize that world, so this function cannot discover or authorize an initial world alone.
pub fn replay_history_records(
    initial_world: &World,
    records: &[StorageRecord],
) -> Result<ReplayedHistory, HistoryReplayError> {
    replay_history_records_with_limits(initial_world, records, HistoryReplayLimits::DEFAULT)
}

/// Replays canonical storage records into a `ReplayedHistory` under explicit replay limits.
pub fn replay_history_records_with_limits(
    initial_world: &World,
    records: &[StorageRecord],
    limits: HistoryReplayLimits,
) -> Result<ReplayedHistory, HistoryReplayError> {
    if records.is_empty() {
        return Err(HistorySemanticError::MissingGenesis.into());
    }
    let receipt_count = records.len() - 1;
    if receipt_count > limits.max_receipts {
        return Err(HistoryReplayLimitError::ReceiptCount {
            actual: receipt_count,
            max: limits.max_receipts,
        }
        .into());
    }
    let mut usage = initial_replay_usage(initial_world, limits)?;

    let mut expected_storage_parent = None;
    for (record_index, record) in records.iter().enumerate() {
        if record.parent() != expected_storage_parent {
            return Err(HistorySemanticError::StorageParentMismatch {
                record_index,
                expected: expected_storage_parent,
                actual: record.parent(),
            }
            .into());
        }
        expected_storage_parent = Some(record.record_tag());
    }

    let mut world = initial_world.clone();
    let mut history = History::new(&world);
    let genesis = &records[0];
    verify_genesis(&history, genesis)?;

    for (record_index, record) in records.iter().enumerate().skip(1) {
        if record.object_tag().kind() != TagKind::Receipt {
            return Err(HistorySemanticError::ReceiptKind {
                record_index,
                actual: record.object_tag().kind(),
            }
            .into());
        }
        if record.object_schema() == GENESIS_SCHEMA {
            return Err(HistorySemanticError::UnexpectedGenesis { record_index }.into());
        }
        if record.object_schema() != RECEIPT_SCHEMA {
            return Err(HistorySemanticError::ReceiptSchema {
                record_index,
                actual: record.object_schema().to_vec(),
            }
            .into());
        }
        let edit_count =
            HistoryReceiptPayload::declared_edit_count(record.payload()).map_err(|source| {
                HistorySemanticError::ReceiptCodec {
                    record_index,
                    source,
                }
            })?;
        if edit_count > limits.max_edits_per_receipt {
            return Err(HistoryReplayLimitError::EditsPerReceipt {
                record_index,
                actual: edit_count,
                max: limits.max_edits_per_receipt,
            }
            .into());
        }
        let next_total_edits = usage.total_edits.checked_add(edit_count).ok_or(
            HistoryReplayLimitError::ArithmeticOverflow {
                field: "total edit count",
            },
        )?;
        if next_total_edits > limits.max_total_edits {
            return Err(HistoryReplayLimitError::TotalEdits {
                record_index,
                actual: next_total_edits,
                max: limits.max_total_edits,
            }
            .into());
        }
        let claimed =
            HistoryReceiptPayload::decode_canonical(record.payload()).map_err(|source| {
                HistorySemanticError::ReceiptCodec {
                    record_index,
                    source,
                }
            })?;
        let claimed_tag = claimed.content_tag();
        if claimed_tag != record.object_tag() {
            return Err(HistorySemanticError::ReceiptObjectTagMismatch {
                record_index,
                expected: claimed_tag,
                actual: record.object_tag(),
            }
            .into());
        }
        if claimed.parent() != history.head() {
            return Err(HistorySemanticError::ReceiptParentMismatch {
                record_index,
                expected: history.head(),
                actual: claimed.parent(),
            }
            .into());
        }
        if let Some(previous) = history.receipts().last()
            && claimed.tick() <= previous.tick()
        {
            return Err(HistorySemanticError::ReceiptTickNotStrict {
                record_index,
                previous: previous.tick(),
                actual: claimed.tick(),
            }
            .into());
        }

        let mut transaction = history.begin_tick(claimed.tick());
        for &edit in claimed.edits() {
            transaction.apply_edit(&history, &mut world, edit);
            if world.len() > limits.max_occupied_bits {
                return Err(HistoryReplayLimitError::OccupiedBits {
                    record_index,
                    actual: world.len(),
                    max: limits.max_occupied_bits,
                }
                .into());
            }
            if world.allocated_chunk_count() > limits.max_allocated_chunks {
                return Err(HistoryReplayLimitError::AllocatedChunks {
                    record_index,
                    actual: world.allocated_chunk_count(),
                    max: limits.max_allocated_chunks,
                }
                .into());
            }
            usage.peak_occupied_bits = usage.peak_occupied_bits.max(world.len());
            usage.peak_allocated_chunks = usage
                .peak_allocated_chunks
                .max(world.allocated_chunk_count());
        }
        let next_world_tag_visits = usage
            .world_tag_coordinate_visits
            .checked_add(world.len())
            .ok_or(HistoryReplayLimitError::ArithmeticOverflow {
                field: "world-tag coordinate visit count",
            })?;
        if next_world_tag_visits > limits.max_world_tag_coordinate_visits {
            return Err(HistoryReplayLimitError::WorldTagCoordinateVisits {
                record_index,
                actual: next_world_tag_visits,
                max: limits.max_world_tag_coordinate_visits,
            }
            .into());
        }
        let prepared = history
            .prepare_tick(transaction, &world)
            .expect("a decoded history receipt contains at least one edit");
        verify_derived_receipt(record_index, &claimed, record, prepared.receipt())?;
        history.commit_prepared_tick(prepared, &world);
        usage.receipt_count += 1;
        usage.total_edits = next_total_edits;
        usage.world_tag_coordinate_visits = next_world_tag_visits;
    }

    Ok(ReplayedHistory {
        world,
        history,
        usage,
    })
}

fn verify_genesis(history: &History, record: &StorageRecord) -> Result<(), HistorySemanticError> {
    if record.object_tag().kind() != TagKind::Receipt {
        return Err(HistorySemanticError::GenesisKind {
            actual: record.object_tag().kind(),
        });
    }
    if record.object_schema() != GENESIS_SCHEMA {
        return Err(HistorySemanticError::GenesisSchema {
            actual: record.object_schema().to_vec(),
        });
    }
    let actual: [u8; 32] =
        record
            .payload()
            .try_into()
            .map_err(|_| HistorySemanticError::GenesisPayloadLength {
                actual: record.payload().len(),
            })?;
    let expected = history.genesis_payload();
    if actual != expected {
        return Err(HistorySemanticError::GenesisWorldMismatch { expected, actual });
    }
    if record.object_tag() != history.genesis() {
        return Err(HistorySemanticError::GenesisObjectTagMismatch {
            expected: history.genesis(),
            actual: record.object_tag(),
        });
    }
    Ok(())
}

fn verify_derived_receipt(
    record_index: usize,
    claimed: &HistoryReceiptPayload,
    record: &StorageRecord,
    derived: &Receipt,
) -> Result<(), HistorySemanticError> {
    if claimed.post_world() != derived.post_world() {
        return Err(HistorySemanticError::ReceiptPostWorldMismatch {
            record_index,
            expected: derived.post_world(),
            actual: claimed.post_world(),
        });
    }
    if claimed.final_event() != derived.final_event() {
        return Err(HistorySemanticError::ReceiptFinalEventMismatch {
            record_index,
            expected: derived.final_event(),
            actual: claimed.final_event(),
        });
    }
    if derived.tag() != record.object_tag() {
        return Err(HistorySemanticError::ReceiptTagMismatch {
            record_index,
            expected: derived.tag(),
            actual: record.object_tag(),
        });
    }
    Ok(())
}

impl HistoryJournal {
    /// Creates a new journal and synchronizes its semantic genesis before exposing writable state.
    pub fn create_new<P: AsRef<Path>>(
        path: P,
        limits: StorageLimits,
        initial_world: &World,
    ) -> Result<(Self, HistoryCreateReceipt), HistoryStoreError> {
        Self::create_new_with_replay_limits(
            path,
            limits,
            HistoryReplayLimits::DEFAULT,
            initial_world,
        )
    }

    /// Creates a new coordinate-history journal with explicit storage and semantic replay limits.
    pub fn create_new_with_replay_limits<P: AsRef<Path>>(
        path: P,
        limits: StorageLimits,
        replay_limits: HistoryReplayLimits,
        initial_world: &World,
    ) -> Result<(Self, HistoryCreateReceipt), HistoryStoreError> {
        initial_replay_usage(initial_world, replay_limits)
            .map_err(HistoryStoreError::ReplayLimit)?;
        let initial_history = History::new(initial_world);
        let payload = initial_history.genesis_payload();
        let mut preflight = StorageChain::with_limits(limits);
        preflight
            .append(TagKind::Receipt, GENESIS_SCHEMA, &payload)
            .map_err(HistoryStoreError::StoragePreflight)?;
        let (mut carrier, file_creation) = Journal::create_new(path, limits)?;
        let genesis_append = carrier
            .append_synced(TagKind::Receipt, GENESIS_SCHEMA, &payload)
            .map_err(|source| HistoryStoreError::GenesisInitializationFailed { source })?;
        if genesis_append.object_tag != initial_history.genesis() {
            return Err(HistoryStoreError::AppendObjectTagMismatch {
                expected: initial_history.genesis(),
                actual: genesis_append.object_tag,
            });
        }
        let state =
            replay_history_records_with_limits(initial_world, carrier.records(), replay_limits)?;
        let receipt = HistoryCreateReceipt {
            file_creation,
            genesis_append,
            genesis: state.history.genesis(),
            initial_world: state.history.genesis_world(),
        };
        Ok((
            Self {
                carrier,
                state,
                limits,
                replay_limits,
                semantic_poisoned: false,
            },
            receipt,
        ))
    }

    /// Creates a new persistent history journal using limits from a LocalStorageProfile.
    pub fn create_new_with_profile<P: AsRef<Path>>(
        path: P,
        profile: &crate::local_bounds::LocalStorageProfile,
        initial_world: &World,
    ) -> Result<(Self, HistoryCreateReceipt), HistoryStoreError> {
        let storage_limits = profile.to_storage_limits()?;
        let replay_limits = profile.to_history_replay_limits()?;
        Self::create_new_with_replay_limits(path, storage_limits, replay_limits, initial_world)
    }

    /// Opens an existing journal using limits from a LocalStorageProfile.
    pub fn open_existing_with_profile<P: AsRef<Path>>(
        path: P,
        profile: &crate::local_bounds::LocalStorageProfile,
        anchor: AnchorPolicy,
        initial_world: &World,
    ) -> Result<OpenHistoryJournal, HistoryStoreError> {
        let storage_limits = profile.to_storage_limits()?;
        let replay_limits = profile.to_history_replay_limits()?;
        Self::open_existing_with_replay_limits(
            path,
            storage_limits,
            replay_limits,
            anchor,
            initial_world,
        )
    }

    /// Opens an existing journal only after semantically replaying its admitted physical prefix.
    pub fn open_existing<P: AsRef<Path>>(
        path: P,
        limits: StorageLimits,
        anchor: AnchorPolicy,
        initial_world: &World,
    ) -> Result<OpenHistoryJournal, HistoryStoreError> {
        Self::open_existing_with_replay_limits(
            path,
            limits,
            HistoryReplayLimits::DEFAULT,
            anchor,
            initial_world,
        )
    }

    /// Opens an existing coordinate-history journal under explicit storage and replay limits.
    pub fn open_existing_with_replay_limits<P: AsRef<Path>>(
        path: P,
        limits: StorageLimits,
        replay_limits: HistoryReplayLimits,
        anchor: AnchorPolicy,
        initial_world: &World,
    ) -> Result<OpenHistoryJournal, HistoryStoreError> {
        match Journal::open_existing(path, limits, anchor)? {
            OpenJournal::Ready {
                journal: carrier,
                report: carrier_report,
            } => {
                let state = replay_history_records_with_limits(
                    initial_world,
                    carrier.records(),
                    replay_limits,
                )?;
                Ok(OpenHistoryJournal::Ready {
                    journal: Box::new(Self {
                        carrier,
                        state,
                        limits,
                        replay_limits,
                        semantic_poisoned: false,
                    }),
                    carrier_report,
                })
            }
            OpenJournal::NeedsRecovery { journal: carrier } => {
                let state = replay_history_records_with_limits(
                    initial_world,
                    carrier.verified_prefix_records(),
                    replay_limits,
                )?;
                Ok(OpenHistoryJournal::NeedsRecovery {
                    journal: Box::new(HistoryRecoveryJournal {
                        carrier: *carrier,
                        state,
                        initial_world: initial_world.clone(),
                        limits,
                        replay_limits,
                    }),
                })
            }
        }
    }

    /// Returns the current committed `World` state.
    pub const fn world(&self) -> &World {
        &self.state.world
    }

    /// Returns the current committed coordinate `History`.
    pub const fn history(&self) -> &History {
        &self.state.history
    }

    /// Returns the cumulative semantic replay usage for the admitted journal state.
    pub const fn replay_usage(&self) -> HistoryReplayUsage {
        self.state.usage
    }

    /// Returns the semantic replay limits enforced by this journal.
    pub const fn replay_limits(&self) -> HistoryReplayLimits {
        self.replay_limits
    }

    /// Returns the `ContentTag` of the latest storage record in the underlying carrier.
    pub fn carrier_head(&self) -> Option<ContentTag> {
        self.carrier.head()
    }

    /// Returns the verified byte length of the underlying physical journal.
    pub const fn carrier_valid_bytes(&self) -> u64 {
        self.carrier.valid_bytes()
    }

    /// Returns `true` if the carrier or semantic state is poisoned and requires reopening.
    pub const fn carrier_is_poisoned(&self) -> bool {
        self.carrier.is_poisoned() || self.semantic_poisoned
    }

    /// Applies edits to a private candidate world, persists their receipt, then publishes state.
    pub fn append_tick(
        &mut self,
        tick: u64,
        edits: &[Edit],
    ) -> Result<Option<HistoryAppendReceipt>, HistoryStoreError> {
        if self.carrier.is_poisoned() || self.semantic_poisoned {
            return Err(HistoryStoreError::Poisoned);
        }
        if edits.is_empty() {
            return Ok(None);
        }
        if let Some(previous) = self.state.history.receipts().last()
            && tick <= previous.tick()
        {
            return Err(HistoryStoreError::NonIncreasingAppendTick {
                previous: previous.tick(),
                actual: tick,
            });
        }
        let next_receipt_count = self.state.usage.receipt_count.checked_add(1).ok_or(
            HistoryReplayLimitError::ArithmeticOverflow {
                field: "receipt count",
            },
        )?;
        if next_receipt_count > self.replay_limits.max_receipts {
            return Err(HistoryStoreError::ReplayLimit(
                HistoryReplayLimitError::ReceiptCount {
                    actual: next_receipt_count,
                    max: self.replay_limits.max_receipts,
                },
            ));
        }
        if edits.len() > self.replay_limits.max_edits_per_receipt {
            return Err(HistoryStoreError::ReplayLimit(
                HistoryReplayLimitError::EditsPerReceipt {
                    record_index: next_receipt_count,
                    actual: edits.len(),
                    max: self.replay_limits.max_edits_per_receipt,
                },
            ));
        }
        let next_total_edits = self
            .state
            .usage
            .total_edits
            .checked_add(edits.len())
            .ok_or(HistoryReplayLimitError::ArithmeticOverflow {
                field: "total edit count",
            })?;
        if next_total_edits > self.replay_limits.max_total_edits {
            return Err(HistoryStoreError::ReplayLimit(
                HistoryReplayLimitError::TotalEdits {
                    record_index: next_receipt_count,
                    actual: next_total_edits,
                    max: self.replay_limits.max_total_edits,
                },
            ));
        }
        let payload_len =
            canonical_receipt_payload_len(edits.len()).map_err(HistoryStoreError::ReceiptLength)?;
        if payload_len > self.limits.max_payload_bytes() {
            return Err(HistoryStoreError::ReceiptPayloadTooLarge {
                actual: payload_len,
                max: self.limits.max_payload_bytes(),
            });
        }
        let mut candidate_world = self.state.world.clone();
        let mut candidate_peak_occupied = self.state.usage.peak_occupied_bits;
        let mut candidate_peak_chunks = self.state.usage.peak_allocated_chunks;
        let mut transaction = self.state.history.begin_tick(tick);
        for &edit in edits {
            transaction.apply_edit(&self.state.history, &mut candidate_world, edit);
            if candidate_world.len() > self.replay_limits.max_occupied_bits {
                return Err(HistoryStoreError::ReplayLimit(
                    HistoryReplayLimitError::OccupiedBits {
                        record_index: next_receipt_count,
                        actual: candidate_world.len(),
                        max: self.replay_limits.max_occupied_bits,
                    },
                ));
            }
            if candidate_world.allocated_chunk_count() > self.replay_limits.max_allocated_chunks {
                return Err(HistoryStoreError::ReplayLimit(
                    HistoryReplayLimitError::AllocatedChunks {
                        record_index: next_receipt_count,
                        actual: candidate_world.allocated_chunk_count(),
                        max: self.replay_limits.max_allocated_chunks,
                    },
                ));
            }
            candidate_peak_occupied = candidate_peak_occupied.max(candidate_world.len());
            candidate_peak_chunks =
                candidate_peak_chunks.max(candidate_world.allocated_chunk_count());
        }
        let next_world_tag_visits = self
            .state
            .usage
            .world_tag_coordinate_visits
            .checked_add(candidate_world.len())
            .ok_or(HistoryReplayLimitError::ArithmeticOverflow {
                field: "world-tag coordinate visit count",
            })?;
        if next_world_tag_visits > self.replay_limits.max_world_tag_coordinate_visits {
            return Err(HistoryStoreError::ReplayLimit(
                HistoryReplayLimitError::WorldTagCoordinateVisits {
                    record_index: next_receipt_count,
                    actual: next_world_tag_visits,
                    max: self.replay_limits.max_world_tag_coordinate_visits,
                },
            ));
        }
        let prepared = self
            .state
            .history
            .prepare_tick(transaction, &candidate_world)
            .expect("a nonempty edit slice produces one receipt candidate");
        let candidate_tag = prepared.receipt().tag();
        let candidate_post_world = prepared.receipt().post_world();
        let state_changes = prepared.changes();
        let payload = prepared.receipt().canonical_payload().encode_canonical();
        debug_assert_eq!(payload.len(), payload_len);
        let carrier_append =
            self.carrier
                .append_synced(TagKind::Receipt, RECEIPT_SCHEMA, &payload)?;
        self.semantic_poisoned = true;
        if carrier_append.object_tag != candidate_tag {
            return Err(HistoryStoreError::AppendObjectTagMismatch {
                expected: candidate_tag,
                actual: carrier_append.object_tag,
            });
        }

        self.state
            .history
            .commit_prepared_tick(prepared, &candidate_world);
        self.state.world = candidate_world;
        self.state.usage.receipt_count = next_receipt_count;
        self.state.usage.total_edits = next_total_edits;
        self.state.usage.peak_occupied_bits = candidate_peak_occupied;
        self.state.usage.peak_allocated_chunks = candidate_peak_chunks;
        self.state.usage.world_tag_coordinate_visits = next_world_tag_visits;
        self.semantic_poisoned = false;

        Ok(Some(HistoryAppendReceipt {
            carrier_append,
            receipt: candidate_tag,
            post_world: candidate_post_world,
            tick,
            edit_count: edits.len(),
            state_changes,
        }))
    }
}

impl HistoryRecoveryJournal {
    /// Returns the physical recovery report describing the verified prefix and rejected tail.
    pub const fn report(&self) -> &RecoveryReport {
        self.carrier.report()
    }

    /// Returns the `World` state reconstructed from the verified physical prefix.
    pub const fn world(&self) -> &World {
        &self.state.world
    }

    /// Returns the coordinate `History` reconstructed from the verified physical prefix.
    pub const fn history(&self) -> &History {
        &self.state.history
    }

    /// Reads and returns the unverified trailing bytes from the underlying recovery journal.
    pub fn export_rejected_tail(&mut self) -> Result<Vec<u8>, HistoryStoreError> {
        self.carrier.export_rejected_tail().map_err(Into::into)
    }

    /// Discards only a physically eligible suffix, then replays the retained semantic prefix.
    pub fn discard_unverified_tail(
        self,
    ) -> Result<(HistoryJournal, TailDiscardReceipt), HistoryStoreError> {
        let Self {
            carrier,
            state: _,
            initial_world,
            limits,
            replay_limits,
        } = self;
        let (carrier, receipt) = carrier.discard_unverified_tail()?;
        let state =
            replay_history_records_with_limits(&initial_world, carrier.records(), replay_limits)?;
        Ok((
            HistoryJournal {
                carrier,
                state,
                limits,
                replay_limits,
                semantic_poisoned: false,
            },
            receipt,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::{NoOpReason, TransitionEffect, world_tag};
    use crate::journal::{ByteScan, inspect_bytes};
    use crate::program::Operation;
    use crate::storage::StorageChain;
    use crate::world::{BEDROCK_COORD, Coord};

    const INITIAL_BIT: Coord = Coord::new(-8, 3, 5);
    const P: Coord = Coord::new(1, -2, 3);
    const Q: Coord = Coord::new(17, 18, -19);

    type Object = (TagKind, Vec<u8>, Vec<u8>);

    fn append_receipt(
        chain: &mut StorageChain,
        history: &mut History,
        world: &mut World,
        tick: u64,
        edits: &[Edit],
    ) {
        let mut transaction = history.begin_tick(tick);
        for &edit in edits {
            transaction.apply_edit(history, world, edit);
        }
        let candidate = history.preview_tick(&transaction, world).unwrap();
        let payload = candidate.canonical_payload().encode_canonical();
        let stored = chain
            .append(TagKind::Receipt, RECEIPT_SCHEMA, &payload)
            .unwrap();
        assert_eq!(stored.object_tag(), candidate.tag());
        assert_eq!(history.commit_tick(transaction, world), Some(&candidate));
    }

    fn matched_chain() -> (World, StorageChain, History, World) {
        let mut initial = World::new();
        assert!(initial.set_on(INITIAL_BIT));
        let mut world = initial.clone();
        let mut history = History::new(&world);
        let mut chain = StorageChain::new();
        chain
            .append(TagKind::Receipt, GENESIS_SCHEMA, &history.genesis_payload())
            .unwrap();
        append_receipt(
            &mut chain,
            &mut history,
            &mut world,
            4,
            &[
                Edit::new(Operation::On, P),
                Edit::new(Operation::On, P),
                Edit::new(Operation::Off, P),
                Edit::new(Operation::Off, P),
                Edit::new(Operation::Toggle, P),
                Edit::new(Operation::Off, BEDROCK_COORD),
            ],
        );
        append_receipt(
            &mut chain,
            &mut history,
            &mut world,
            9,
            &[Edit::new(Operation::On, Q), Edit::new(Operation::Toggle, P)],
        );
        (initial, chain, history, world)
    }

    fn objects(chain: &StorageChain) -> Vec<Object> {
        chain
            .records()
            .iter()
            .map(|record| {
                (
                    record.object_tag().kind(),
                    record.object_schema().to_vec(),
                    record.payload().to_vec(),
                )
            })
            .collect()
    }

    fn rebuild(objects: &[Object]) -> StorageChain {
        let mut rebuilt = StorageChain::new();
        for (kind, schema, payload) in objects {
            rebuilt.append(*kind, schema, payload).unwrap();
        }
        rebuilt
    }

    fn assert_repaired_carrier_rejected(
        label: &str,
        initial: &World,
        objects: Vec<Object>,
        expected: fn(&HistorySemanticError) -> bool,
    ) {
        let rebuilt = rebuild(&objects);
        let encoded = rebuilt.encode();
        assert_eq!(
            StorageChain::decode(&encoded).unwrap(),
            rebuilt,
            "{label}: rebuilt carrier is not canonical"
        );
        assert!(matches!(
            inspect_bytes(
                &encoded,
                StorageLimits::DEFAULT,
                AnchorPolicy::UnanchoredLocal
            )
            .unwrap(),
            ByteScan::Clean(_)
        ));
        let error = match replay_history_records(initial, rebuilt.records()).unwrap_err() {
            HistoryReplayError::Semantic(error) => error,
            HistoryReplayError::Limit(error) => {
                panic!("{label}: mutation reached replay policy instead of semantics: {error}")
            }
        };
        assert!(expected(&error), "{label}: unexpected error {error:?}");
    }

    fn receipt_offsets(payload: &[u8]) -> (usize, usize, usize) {
        let count =
            usize::try_from(u64::from_be_bytes(payload[40..48].try_into().unwrap())).unwrap();
        let post_world = 48 + count * 25;
        let final_event = post_world + 32;
        let flags = final_event + 32;
        assert_eq!(flags + 2, payload.len());
        (post_world, final_event, flags)
    }

    #[test]
    fn bounded_in_memory_verification_matches_persistent_usage_and_refuses_each_limit() {
        let (initial, chain, history, expected_world) = matched_chain();
        let exact = HistoryReplayLimits::new(2, 6, 8, 4, 4, 8);
        let verified = verify_in_memory_history_with_limits(&initial, &history, exact).unwrap();
        let persisted =
            replay_history_records_with_limits(&initial, chain.records(), exact).unwrap();

        assert_eq!(verified.usage(), persisted.usage());
        assert_eq!(
            verified.usage(),
            HistoryReplayUsage {
                receipt_count: 2,
                total_edits: 8,
                peak_occupied_bits: 4,
                peak_allocated_chunks: 4,
                world_tag_coordinate_visits: 8,
            }
        );
        assert_eq!(verified.history().head(), history.head());
        assert_eq!(
            world_tag(verified.final_world()),
            world_tag(&expected_world)
        );

        let cases = [
            HistoryReplayLimits::new(1, 6, 8, 4, 4, 8),
            HistoryReplayLimits::new(2, 5, 8, 4, 4, 8),
            HistoryReplayLimits::new(2, 6, 7, 4, 4, 8),
            HistoryReplayLimits::new(2, 6, 8, 3, 4, 8),
            HistoryReplayLimits::new(2, 6, 8, 4, 3, 8),
            HistoryReplayLimits::new(2, 6, 8, 4, 4, 7),
        ];
        for (index, limits) in cases.into_iter().enumerate() {
            let error = verify_in_memory_history_with_limits(&initial, &history, limits)
                .expect_err("one-less limit must refuse");
            let expected_variant = match index {
                0 => matches!(
                    error,
                    InMemoryHistoryVerificationError::Limit(
                        HistoryReplayLimitError::ReceiptCount { .. }
                    )
                ),
                1 => matches!(
                    error,
                    InMemoryHistoryVerificationError::Limit(
                        HistoryReplayLimitError::EditsPerReceipt { .. }
                    )
                ),
                2 => matches!(
                    error,
                    InMemoryHistoryVerificationError::Limit(
                        HistoryReplayLimitError::TotalEdits { .. }
                    )
                ),
                3 => matches!(
                    error,
                    InMemoryHistoryVerificationError::Limit(
                        HistoryReplayLimitError::OccupiedBits { .. }
                    )
                ),
                4 => matches!(
                    error,
                    InMemoryHistoryVerificationError::Limit(
                        HistoryReplayLimitError::AllocatedChunks { .. }
                    )
                ),
                5 => matches!(
                    error,
                    InMemoryHistoryVerificationError::Limit(
                        HistoryReplayLimitError::WorldTagCoordinateVisits { .. }
                    )
                ),
                _ => unreachable!(),
            };
            assert!(expected_variant, "case {index} returned {error:?}");
        }

        assert!(verify_in_memory_history_with_limits(&initial, &history, exact).is_ok());

        let empty_world = World::new();
        let empty_history = History::new(&empty_world);
        let empty = verify_in_memory_history_with_limits(
            &empty_world,
            &empty_history,
            HistoryReplayLimits::new(0, 0, 0, 1, 1, 1),
        )
        .unwrap();
        assert_eq!(
            empty.usage(),
            HistoryReplayUsage {
                receipt_count: 0,
                total_edits: 0,
                peak_occupied_bits: 1,
                peak_allocated_chunks: 1,
                world_tag_coordinate_visits: 1,
            }
        );
    }

    #[test]
    fn semantic_replay_reconstructs_world_effects_and_provenance() {
        let (initial, chain, expected_history, expected_world) = matched_chain();
        let replayed = replay_history_records(&initial, chain.records()).unwrap();

        assert_eq!(world_tag(replayed.world()), world_tag(&expected_world));
        assert_eq!(replayed.history().head(), expected_history.head());
        assert_eq!(replayed.history().receipts(), expected_history.receipts());
        assert_eq!(replayed.history().circulating_supply(), 2);
        assert_eq!(
            replayed.history().active_mint(INITIAL_BIT),
            Some(replayed.history().genesis())
        );
        assert_eq!(replayed.history().active_mint(P), None);
        assert!(replayed.history().active_mint(Q).is_some());
        assert_eq!(
            replayed.usage(),
            HistoryReplayUsage {
                receipt_count: 2,
                total_edits: 8,
                peak_occupied_bits: 4,
                peak_allocated_chunks: 4,
                world_tag_coordinate_visits: 8,
            }
        );
        let mut actual_coords = replayed.world().iter_on().collect::<Vec<_>>();
        actual_coords.sort_unstable();
        let mut expected_coords = vec![BEDROCK_COORD, INITIAL_BIT, Q];
        expected_coords.sort_unstable();
        assert_eq!(actual_coords, expected_coords);

        let first = &replayed.history().receipts()[0];
        let first_mint = first.effects()[0].event();
        let remint = first.effects()[4].event();
        assert!(matches!(
            first.effects()[2],
            TransitionEffect::Burn { consumed_mint, .. } if consumed_mint == first_mint
        ));
        assert!(matches!(
            first.effects()[1],
            TransitionEffect::NoOp {
                reason: NoOpReason::AlreadyOn,
                ..
            }
        ));
        assert!(matches!(
            first.effects()[3],
            TransitionEffect::NoOp {
                reason: NoOpReason::AlreadyOff,
                ..
            }
        ));
        assert!(matches!(
            first.effects()[5],
            TransitionEffect::NoOp {
                reason: NoOpReason::BedrockProtected,
                ..
            }
        ));
        let second = &replayed.history().receipts()[1];
        let q_mint = second.effects()[0].event();
        assert!(matches!(
            second.effects()[1],
            TransitionEffect::Burn { consumed_mint, .. } if consumed_mint == remint
        ));
        assert_eq!(replayed.history().active_mint(Q), Some(q_mint));

        let records = chain.records();
        let first_claim = HistoryReceiptPayload::decode_canonical(records[1].payload()).unwrap();
        assert_eq!(records[1].parent(), Some(records[0].record_tag()));
        assert_eq!(first_claim.parent(), records[0].object_tag());
        assert_ne!(records[0].record_tag(), records[0].object_tag());
    }

    #[test]
    fn repaired_carrier_hashes_cannot_substitute_for_coordinate_semantics() {
        let (initial, chain, _, _) = matched_chain();
        let original = objects(&chain);

        let mut case = original.clone();
        case[0].0 = TagKind::World;
        assert_repaired_carrier_rejected("genesis kind", &initial, case, |error| {
            matches!(error, HistorySemanticError::GenesisKind { .. })
        });

        let mut case = original.clone();
        case[0].1 = b"bitblock/history/not-genesis/v1".to_vec();
        assert_repaired_carrier_rejected("genesis schema", &initial, case, |error| {
            matches!(error, HistorySemanticError::GenesisSchema { .. })
        });

        let mut case = original.clone();
        case[0].2[0] ^= 1;
        assert_repaired_carrier_rejected("genesis world", &initial, case, |error| {
            matches!(error, HistorySemanticError::GenesisWorldMismatch { .. })
        });

        let mut case = original.clone();
        case.insert(1, original[0].clone());
        assert_repaired_carrier_rejected("duplicate genesis", &initial, case, |error| {
            matches!(error, HistorySemanticError::UnexpectedGenesis { .. })
        });

        let mut case = original.clone();
        case[1].0 = TagKind::World;
        assert_repaired_carrier_rejected("receipt kind", &initial, case, |error| {
            matches!(error, HistorySemanticError::ReceiptKind { .. })
        });

        let mut case = original.clone();
        case[1].1 = b"bitblock/history/not-receipt/v1".to_vec();
        assert_repaired_carrier_rejected("receipt schema", &initial, case, |error| {
            matches!(error, HistorySemanticError::ReceiptSchema { .. })
        });

        let mut case = original.clone();
        case[1].2[0] ^= 1;
        assert_repaired_carrier_rejected("semantic parent", &initial, case, |error| {
            matches!(error, HistorySemanticError::ReceiptParentMismatch { .. })
        });

        let mut case = original.clone();
        let first_tick = case[1].2[32..40].to_vec();
        case[2].2[32..40].copy_from_slice(&first_tick);
        assert_repaired_carrier_rejected("non-increasing tick", &initial, case, |error| {
            matches!(error, HistorySemanticError::ReceiptTickNotStrict { .. })
        });

        let mut case = original.clone();
        case[1].2[40..48].copy_from_slice(&0_u64.to_be_bytes());
        assert_repaired_carrier_rejected("zero edit count", &initial, case, |error| {
            matches!(
                error,
                HistorySemanticError::ReceiptCodec {
                    source: HistoryReceiptCodecError::EmptyEdits,
                    ..
                }
            )
        });

        let mut case = original.clone();
        case[1].2[40..48].copy_from_slice(&7_u64.to_be_bytes());
        assert_repaired_carrier_rejected("inconsistent edit count", &initial, case, |error| {
            matches!(
                error,
                HistorySemanticError::ReceiptCodec {
                    source: HistoryReceiptCodecError::WrongLength { .. },
                    ..
                }
            )
        });

        let mut case = original.clone();
        case[1].2[48] = u8::MAX;
        assert_repaired_carrier_rejected("invalid edit operation", &initial, case, |error| {
            matches!(
                error,
                HistorySemanticError::ReceiptCodec {
                    source: HistoryReceiptCodecError::Edit { .. },
                    ..
                }
            )
        });

        let mut case = original.clone();
        case[1].2[49] ^= 1;
        assert_repaired_carrier_rejected("changed valid edit", &initial, case, |error| {
            matches!(error, HistorySemanticError::ReceiptPostWorldMismatch { .. })
        });

        let mut case = original.clone();
        let (post_world, _, _) = receipt_offsets(&case[1].2);
        case[1].2[post_world] ^= 1;
        assert_repaired_carrier_rejected("claimed post-world", &initial, case, |error| {
            matches!(error, HistorySemanticError::ReceiptPostWorldMismatch { .. })
        });

        let mut case = original.clone();
        let (_, final_event, _) = receipt_offsets(&case[1].2);
        case[1].2[final_event] ^= 1;
        assert_repaired_carrier_rejected("claimed final event", &initial, case, |error| {
            matches!(
                error,
                HistorySemanticError::ReceiptFinalEventMismatch { .. }
            )
        });

        for flag in 0..2 {
            let mut case = original.clone();
            let (_, _, flags) = receipt_offsets(&case[1].2);
            case[1].2[flags + flag] = 1;
            assert_repaired_carrier_rejected("reserved flag", &initial, case, |error| {
                matches!(
                    error,
                    HistorySemanticError::ReceiptCodec {
                        source: HistoryReceiptCodecError::ReservedFlags { .. },
                        ..
                    }
                )
            });
        }

        let mut case = original.clone();
        case.swap(1, 2);
        assert_repaired_carrier_rejected("reordered receipts", &initial, case, |error| {
            matches!(error, HistorySemanticError::ReceiptParentMismatch { .. })
        });

        let mut case = original.clone();
        case.push(original[2].clone());
        assert_repaired_carrier_rejected("duplicated receipt", &initial, case, |error| {
            matches!(error, HistorySemanticError::ReceiptParentMismatch { .. })
        });
    }

    #[test]
    fn caller_supplied_initial_world_is_part_of_the_replay_claim() {
        let (_, chain, _, _) = matched_chain();
        let wrong = World::new();
        assert!(matches!(
            replay_history_records(&wrong, chain.records()),
            Err(HistoryReplayError::Semantic(
                HistorySemanticError::GenesisWorldMismatch { .. }
            ))
        ));
    }

    #[test]
    fn public_replay_refuses_a_broken_physical_parent_slice() {
        let (initial, chain, _, _) = matched_chain();
        let mut broken = chain.records().to_vec();
        broken.swap(1, 2);
        assert!(matches!(
            replay_history_records(&initial, &broken),
            Err(HistoryReplayError::Semantic(
                HistorySemanticError::StorageParentMismatch {
                    record_index: 1,
                    ..
                }
            ))
        ));
    }

    #[test]
    fn replay_policy_bounds_decode_state_and_cumulative_world_tag_work() {
        let (initial, chain, _, _) = matched_chain();
        let per_receipt = HistoryReplayLimits::new(2, 5, 100, 100, 100, 100);
        assert!(matches!(
            replay_history_records_with_limits(&initial, chain.records(), per_receipt),
            Err(HistoryReplayError::Limit(
                HistoryReplayLimitError::EditsPerReceipt {
                    record_index: 1,
                    actual: 6,
                    max: 5
                }
            ))
        ));

        let world_work = HistoryReplayLimits::new(2, 10, 10, 10, 10, 7);
        assert!(matches!(
            replay_history_records_with_limits(&initial, chain.records(), world_work),
            Err(HistoryReplayError::Limit(
                HistoryReplayLimitError::WorldTagCoordinateVisits {
                    record_index: 2,
                    actual: 8,
                    max: 7
                }
            ))
        ));

        let mut malformed = objects(&chain);
        malformed[1].2[40..48].copy_from_slice(&u64::MAX.to_be_bytes());
        let malformed = rebuild(&malformed);
        let zero_policy = HistoryReplayLimits::new(2, 0, 0, 10, 10, 10);
        assert!(matches!(
            replay_history_records_with_limits(&initial, malformed.records(), zero_policy),
            Err(HistoryReplayError::Semantic(
                HistorySemanticError::ReceiptCodec {
                    source: HistoryReceiptCodecError::LengthOverflow,
                    ..
                }
            ))
        ));

        let initial = World::new();
        let mut world = initial.clone();
        let mut history = History::new(&world);
        let mut transient = StorageChain::new();
        transient
            .append(TagKind::Receipt, GENESIS_SCHEMA, &history.genesis_payload())
            .unwrap();
        append_receipt(
            &mut transient,
            &mut history,
            &mut world,
            1,
            &[
                Edit::new(Operation::On, P),
                Edit::new(Operation::On, Q),
                Edit::new(Operation::Off, P),
                Edit::new(Operation::Off, Q),
            ],
        );
        let peak = HistoryReplayLimits::new(1, 4, 4, 2, 10, 10);
        assert!(matches!(
            replay_history_records_with_limits(&initial, transient.records(), peak),
            Err(HistoryReplayError::Limit(
                HistoryReplayLimitError::OccupiedBits {
                    record_index: 1,
                    actual: 3,
                    max: 2
                }
            ))
        ));

        let chunks = HistoryReplayLimits::new(1, 4, 4, 10, 1, 10);
        assert!(matches!(
            replay_history_records_with_limits(&initial, transient.records(), chunks),
            Err(HistoryReplayError::Limit(
                HistoryReplayLimitError::AllocatedChunks {
                    record_index: 1,
                    actual: 2,
                    max: 1
                }
            ))
        ));
    }

    #[test]
    fn exact_boundary_rollback_and_alternate_valid_branch_remain_locally_admissible() {
        let (initial, full, _, _) = matched_chain();
        let mut shorter_objects = objects(&full);
        shorter_objects.pop();
        let shorter = rebuild(&shorter_objects);
        let shorter_state = replay_history_records(&initial, shorter.records()).unwrap();
        assert_ne!(
            shorter_state.history().head(),
            replay_history_records(&initial, full.records())
                .unwrap()
                .history()
                .head()
        );

        let mut alternate_world = initial.clone();
        let mut alternate_history = History::new(&alternate_world);
        let mut alternate = StorageChain::new();
        alternate
            .append(
                TagKind::Receipt,
                GENESIS_SCHEMA,
                &alternate_history.genesis_payload(),
            )
            .unwrap();
        append_receipt(
            &mut alternate,
            &mut alternate_history,
            &mut alternate_world,
            5,
            &[Edit::new(Operation::On, Coord::new(99, 0, 0))],
        );
        assert!(replay_history_records(&initial, alternate.records()).is_ok());
        assert_ne!(alternate_history.head(), shorter_state.history().head());
    }

    mod file_backed {
        use super::*;
        use std::fs::OpenOptions;
        use std::io::{Read, Seek, SeekFrom, Write};
        use std::sync::atomic::{AtomicU64, Ordering};

        static NEXT_PATH: AtomicU64 = AtomicU64::new(0);

        struct TempHistoryPath(std::path::PathBuf);

        impl TempHistoryPath {
            fn new(label: &str) -> Self {
                let serial = NEXT_PATH.fetch_add(1, Ordering::Relaxed);
                Self(std::env::temp_dir().join(format!(
                    "bitblock-history-{label}-{}-{serial}.bbj",
                    std::process::id()
                )))
            }
        }

        impl Drop for TempHistoryPath {
            fn drop(&mut self) {
                match std::fs::remove_file(&self.0) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => panic!("could not remove history test file: {error}"),
                }
            }
        }

        #[test]
        fn synced_history_reopens_to_the_exact_world_head_and_provenance() {
            let path = TempHistoryPath::new("reopen");
            let initial = World::new();
            let (mut journal, created) =
                HistoryJournal::create_new(&path.0, StorageLimits::DEFAULT, &initial).unwrap();
            assert_eq!(created.genesis, journal.history().genesis());
            assert_eq!(created.initial_world, world_tag(&initial));
            assert_eq!(created.genesis_append.object_tag, created.genesis);

            let first = journal
                .append_tick(
                    2,
                    &[Edit::new(Operation::On, P), Edit::new(Operation::On, P)],
                )
                .unwrap()
                .unwrap();
            let second = journal
                .append_tick(
                    8,
                    &[Edit::new(Operation::Off, P), Edit::new(Operation::On, Q)],
                )
                .unwrap()
                .unwrap();
            assert_eq!(first.state_changes, 1);
            assert_eq!(second.state_changes, 2);
            let expected_history = journal.history().head();
            let expected_world = world_tag(journal.world());
            let expected_storage = journal.carrier_head();
            let expected_bytes = journal.carrier_valid_bytes();
            let expected_mint = journal.history().active_mint(Q);
            drop(journal);

            let OpenHistoryJournal::Ready {
                journal,
                carrier_report,
            } = HistoryJournal::open_existing(
                &path.0,
                StorageLimits::DEFAULT,
                AnchorPolicy::UnanchoredLocal,
                &initial,
            )
            .unwrap()
            else {
                panic!("synced typed journal required recovery")
            };
            assert_eq!(journal.history().head(), expected_history);
            assert_eq!(world_tag(journal.world()), expected_world);
            assert_eq!(journal.carrier_head(), expected_storage);
            assert_eq!(journal.carrier_valid_bytes(), expected_bytes);
            assert_eq!(carrier_report.valid_record_count, 3);
            assert_eq!(journal.history().active_mint(Q), expected_mint);
            assert_eq!(journal.history().active_mint(P), None);
        }

        #[test]
        fn profile_backed_history_journal_creates_appends_and_reopens() {
            let path = TempHistoryPath::new("profile_journal");
            let initial = World::new();
            let profile = crate::local_bounds::LocalStorageProfile::DEFAULT;
            let (mut journal, created) =
                HistoryJournal::create_new_with_profile(&path.0, &profile, &initial).unwrap();
            assert_eq!(created.genesis, journal.history().genesis());

            journal
                .append_tick(1, &[Edit::new(Operation::On, P)])
                .unwrap()
                .unwrap();
            let expected_head = journal.history().head();
            drop(journal);

            let OpenHistoryJournal::Ready { journal, .. } =
                HistoryJournal::open_existing_with_profile(
                    &path.0,
                    &profile,
                    AnchorPolicy::UnanchoredLocal,
                    &initial,
                )
                .unwrap()
            else {
                panic!("profile backed journal failed to open");
            };
            assert_eq!(journal.history().head(), expected_head);
        }

        #[test]
        fn deterministic_genesis_preflight_failure_does_not_create_a_file() {
            let path = TempHistoryPath::new("preflight");
            let default = StorageLimits::DEFAULT;
            let zero_records = StorageLimits::new(
                default.max_record_bytes(),
                default.max_schema_bytes(),
                default.max_payload_bytes(),
                0,
                default.max_stream_bytes(),
            )
            .unwrap();
            assert!(matches!(
                HistoryJournal::create_new(&path.0, zero_records, &World::new()),
                Err(HistoryStoreError::StoragePreflight(
                    StorageError::RecordCountLimit { max: 0 }
                ))
            ));
            assert!(!path.0.exists());
        }

        #[test]
        fn failed_receipt_append_leaves_all_live_semantic_heads_unpublished() {
            let path = TempHistoryPath::new("append-failure");
            let default = StorageLimits::DEFAULT;
            let genesis_only = StorageLimits::new(
                default.max_record_bytes(),
                default.max_schema_bytes(),
                default.max_payload_bytes(),
                1,
                default.max_stream_bytes(),
            )
            .unwrap();
            let initial = World::new();
            let (mut journal, _) =
                HistoryJournal::create_new(&path.0, genesis_only, &initial).unwrap();
            let history_head = journal.history().head();
            let world_head = world_tag(journal.world());
            let carrier_head = journal.carrier_head();
            let valid_bytes = journal.carrier_valid_bytes();
            let supply = journal.history().circulating_supply();

            assert!(matches!(
                journal.append_tick(1, &[Edit::new(Operation::On, P)]),
                Err(HistoryStoreError::Journal(JournalError::Storage(
                    StorageError::RecordCountLimit { max: 1 }
                )))
            ));
            assert_eq!(journal.history().head(), history_head);
            assert_eq!(world_tag(journal.world()), world_head);
            assert_eq!(journal.carrier_head(), carrier_head);
            assert_eq!(journal.carrier_valid_bytes(), valid_bytes);
            assert_eq!(journal.history().circulating_supply(), supply);
            assert!(!journal.world().is_on(P));
            assert!(!journal.carrier_is_poisoned());
        }

        #[test]
        fn non_increasing_live_tick_is_rejected_without_panicking_or_mutating() {
            let path = TempHistoryPath::new("tick-order");
            let initial = World::new();
            let (mut journal, _) =
                HistoryJournal::create_new(&path.0, StorageLimits::DEFAULT, &initial).unwrap();
            journal
                .append_tick(7, &[Edit::new(Operation::On, P)])
                .unwrap();
            let history_head = journal.history().head();
            let world_head = world_tag(journal.world());
            let carrier_head = journal.carrier_head();

            assert!(matches!(
                journal.append_tick(7, &[Edit::new(Operation::On, Q)]),
                Err(HistoryStoreError::NonIncreasingAppendTick {
                    previous: 7,
                    actual: 7
                })
            ));
            assert_eq!(journal.history().head(), history_head);
            assert_eq!(world_tag(journal.world()), world_head);
            assert_eq!(journal.carrier_head(), carrier_head);
            assert!(!journal.world().is_on(Q));
        }

        #[test]
        fn live_transient_replay_usage_matches_reopened_reconstruction() {
            let path = TempHistoryPath::new("usage-parity");
            let initial = World::new();
            let (mut journal, _) =
                HistoryJournal::create_new(&path.0, StorageLimits::DEFAULT, &initial).unwrap();
            journal
                .append_tick(
                    1,
                    &[
                        Edit::new(Operation::On, P),
                        Edit::new(Operation::On, Q),
                        Edit::new(Operation::Off, P),
                        Edit::new(Operation::Off, Q),
                    ],
                )
                .unwrap();
            let expected = journal.replay_usage();
            assert_eq!(expected.peak_occupied_bits, 3);
            assert_eq!(expected.peak_allocated_chunks, 3);
            assert_eq!(journal.world().len(), 1);
            drop(journal);

            let OpenHistoryJournal::Ready { journal, .. } = HistoryJournal::open_existing(
                &path.0,
                StorageLimits::DEFAULT,
                AnchorPolicy::UnanchoredLocal,
                &initial,
            )
            .unwrap() else {
                panic!("complete transient receipt required recovery")
            };
            assert_eq!(journal.replay_usage(), expected);
            assert_eq!(journal.world().len(), 1);
        }

        #[test]
        fn live_replay_policy_failure_precedes_carrier_or_semantic_publication() {
            let path = TempHistoryPath::new("live-policy");
            let initial = World::new();
            let replay_limits = HistoryReplayLimits::new(4, 4, 4, 1, 1, 16);
            let (mut journal, _) = HistoryJournal::create_new_with_replay_limits(
                &path.0,
                StorageLimits::DEFAULT,
                replay_limits,
                &initial,
            )
            .unwrap();
            let history_head = journal.history().head();
            let carrier_head = journal.carrier_head();
            let valid_bytes = journal.carrier_valid_bytes();

            assert!(matches!(
                journal.append_tick(1, &[Edit::new(Operation::On, P)]),
                Err(HistoryStoreError::ReplayLimit(
                    HistoryReplayLimitError::OccupiedBits {
                        record_index: 1,
                        actual: 2,
                        max: 1
                    }
                ))
            ));
            assert_eq!(journal.history().head(), history_head);
            assert_eq!(journal.carrier_head(), carrier_head);
            assert_eq!(journal.carrier_valid_bytes(), valid_bytes);
            assert!(!journal.world().is_on(P));
            assert!(!journal.carrier_is_poisoned());
        }

        #[test]
        fn persisted_but_unpublished_candidate_is_recovered_only_by_replay() {
            let path = TempHistoryPath::new("persisted-unpublished");
            let initial = World::new();
            let (mut journal, _) =
                HistoryJournal::create_new(&path.0, StorageLimits::DEFAULT, &initial).unwrap();
            let old_history = journal.history().head();
            let old_world = world_tag(journal.world());
            let mut candidate_world = journal.state.world.clone();
            let mut transaction = journal.state.history.begin_tick(3);
            transaction.apply_edit(
                &journal.state.history,
                &mut candidate_world,
                Edit::new(Operation::On, P),
            );
            let prepared = journal
                .state
                .history
                .prepare_tick(transaction, &candidate_world)
                .unwrap();
            let expected_receipt = prepared.receipt().tag();
            let expected_world = prepared.receipt().post_world();
            let payload = prepared.receipt().canonical_payload().encode_canonical();
            let appended = journal
                .carrier
                .append_synced(TagKind::Receipt, RECEIPT_SCHEMA, &payload)
                .unwrap();
            assert_eq!(appended.object_tag, expected_receipt);
            drop(prepared);

            assert_eq!(journal.history().head(), old_history);
            assert_eq!(world_tag(journal.world()), old_world);
            assert_eq!(journal.carrier_head(), Some(appended.record_tag));
            drop(journal);

            let OpenHistoryJournal::Ready { journal, .. } = HistoryJournal::open_existing(
                &path.0,
                StorageLimits::DEFAULT,
                AnchorPolicy::UnanchoredLocal,
                &initial,
            )
            .unwrap() else {
                panic!("complete persisted candidate required recovery")
            };
            assert_eq!(journal.history().head(), expected_receipt);
            assert_eq!(world_tag(journal.world()), expected_world);
            assert!(journal.world().is_on(P));
        }

        #[test]
        fn missing_or_torn_genesis_exposes_no_typed_recovery_and_preserves_bytes() {
            let empty_path = TempHistoryPath::new("empty-genesis");
            let (carrier, _) = Journal::create_new(&empty_path.0, StorageLimits::DEFAULT).unwrap();
            drop(carrier);
            let empty_bytes = std::fs::read(&empty_path.0).unwrap();
            assert!(empty_bytes.is_empty());
            assert!(matches!(
                HistoryJournal::open_existing(
                    &empty_path.0,
                    StorageLimits::DEFAULT,
                    AnchorPolicy::UnanchoredLocal,
                    &World::new()
                ),
                Err(HistoryStoreError::Semantic(
                    HistorySemanticError::MissingGenesis
                ))
            ));
            assert_eq!(std::fs::read(&empty_path.0).unwrap(), empty_bytes);

            let torn_path = TempHistoryPath::new("torn-genesis");
            let initial = World::new();
            let (journal, created) =
                HistoryJournal::create_new(&torn_path.0, StorageLimits::DEFAULT, &initial).unwrap();
            drop(journal);
            let full = std::fs::read(&torn_path.0).unwrap();
            let cut = usize::try_from(created.genesis_append.end_offset / 2).unwrap();
            let file = OpenOptions::new().write(true).open(&torn_path.0).unwrap();
            file.set_len(cut as u64).unwrap();
            file.sync_all().unwrap();
            drop(file);
            let torn = full[..cut].to_vec();
            assert!(matches!(
                HistoryJournal::open_existing(
                    &torn_path.0,
                    StorageLimits::DEFAULT,
                    AnchorPolicy::UnanchoredLocal,
                    &initial
                ),
                Err(HistoryStoreError::Semantic(
                    HistorySemanticError::MissingGenesis
                ))
            ));
            assert_eq!(std::fs::read(&torn_path.0).unwrap(), torn);
        }

        #[test]
        fn typed_boundary_rollback_is_detected_only_relative_to_supplied_anchor() {
            let path = TempHistoryPath::new("typed-anchor");
            let initial = World::new();
            let (mut journal, _) =
                HistoryJournal::create_new(&path.0, StorageLimits::DEFAULT, &initial).unwrap();
            let first = journal
                .append_tick(1, &[Edit::new(Operation::On, P)])
                .unwrap()
                .unwrap();
            let second = journal
                .append_tick(2, &[Edit::new(Operation::On, Q)])
                .unwrap()
                .unwrap();
            drop(journal);

            let file = OpenOptions::new().write(true).open(&path.0).unwrap();
            file.set_len(first.carrier_append.end_offset).unwrap();
            file.sync_all().unwrap();
            drop(file);

            let OpenHistoryJournal::Ready { journal, .. } = HistoryJournal::open_existing(
                &path.0,
                StorageLimits::DEFAULT,
                AnchorPolicy::UnanchoredLocal,
                &initial,
            )
            .unwrap() else {
                panic!("exact-boundary rollback is locally canonical")
            };
            assert!(journal.world().is_on(P));
            assert!(!journal.world().is_on(Q));
            drop(journal);

            assert!(matches!(
                HistoryJournal::open_existing(
                    &path.0,
                    StorageLimits::DEFAULT,
                    AnchorPolicy::RequireAncestor(second.carrier_append.record_tag),
                    &initial,
                ),
                Err(HistoryStoreError::Journal(
                    JournalError::RequiredAncestorMissing { .. }
                ))
            ));
        }

        #[test]
        fn physical_tail_recovery_requires_a_semantically_valid_retained_prefix() {
            let path = TempHistoryPath::new("tail-recovery");
            let initial = World::new();
            let (mut journal, _) =
                HistoryJournal::create_new(&path.0, StorageLimits::DEFAULT, &initial).unwrap();
            let first = journal
                .append_tick(2, &[Edit::new(Operation::On, P)])
                .unwrap()
                .unwrap();
            let second = journal
                .append_tick(5, &[Edit::new(Operation::On, Q)])
                .unwrap()
                .unwrap();
            let original_bytes = second.carrier_append.end_offset;
            drop(journal);

            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path.0)
                .unwrap();
            file.seek(SeekFrom::Start(original_bytes - 1)).unwrap();
            let mut byte = [0_u8; 1];
            file.read_exact(&mut byte).unwrap();
            byte[0] ^= 1;
            file.seek(SeekFrom::Start(original_bytes - 1)).unwrap();
            file.write_all(&byte).unwrap();
            file.sync_all().unwrap();
            drop(file);

            let OpenHistoryJournal::NeedsRecovery { journal } = HistoryJournal::open_existing(
                &path.0,
                StorageLimits::DEFAULT,
                AnchorPolicy::UnanchoredLocal,
                &initial,
            )
            .unwrap() else {
                panic!("corrupt physical suffix was admitted")
            };
            let mut journal = *journal;
            assert_eq!(
                journal.report().valid_prefix_bytes,
                first.carrier_append.end_offset
            );
            assert!(journal.world().is_on(P));
            assert!(!journal.world().is_on(Q));
            assert_eq!(journal.history().receipts().len(), 1);
            assert!(!journal.export_rejected_tail().unwrap().is_empty());

            let (mut journal, discard) = journal.discard_unverified_tail().unwrap();
            assert_eq!(
                discard.retained_prefix_bytes,
                first.carrier_append.end_offset
            );
            assert!(journal.world().is_on(P));
            assert!(!journal.world().is_on(Q));
            journal
                .append_tick(6, &[Edit::new(Operation::On, Q)])
                .unwrap();
            assert!(journal.world().is_on(Q));
        }

        #[test]
        fn carrier_valid_semantic_failure_is_never_exposed_as_writable_or_repairable() {
            let path = TempHistoryPath::new("semantic-prefix");
            let (initial, chain, _, _) = matched_chain();
            let mut objects = objects(&chain);
            let (post_world, _, _) = receipt_offsets(&objects[1].2);
            objects[1].2[post_world] ^= 1;
            let invalid = rebuild(&objects);
            let mut physical = invalid.encode();
            physical.push(0xff);
            std::fs::write(&path.0, &physical).unwrap();

            let error = match HistoryJournal::open_existing(
                &path.0,
                StorageLimits::DEFAULT,
                AnchorPolicy::UnanchoredLocal,
                &initial,
            ) {
                Err(error) => error,
                Ok(_) => panic!("semantically false prefix was exposed"),
            };
            assert!(matches!(
                error,
                HistoryStoreError::Semantic(HistorySemanticError::ReceiptPostWorldMismatch { .. })
            ));
            assert_eq!(std::fs::read(&path.0).unwrap(), physical);
        }
    }
}
