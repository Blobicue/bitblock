//! Content-addressed local coordinate history.
//!
//! This is an unsigned, in-memory append chain. It provides deterministic replay, exact
//! mint/burn provenance, and tamper-evident tags under the hash assumption. It does not provide
//! persistence, rollback resistance, ownership, consensus, cryptocurrency scarcity, or a Layer 1.

use std::collections::HashMap;
use std::fmt;

#[cfg(test)]
use crate::program::Operation;
use crate::program::{EDIT_CANONICAL_LEN, Edit, EditCodecError};
use crate::protocol::{ContentTag, TagKind, hash_canonical};
#[cfg(test)]
use crate::world::BEDROCK_COORD;
use crate::world::{Coord, World, is_bedrock};

const WORLD_SCHEMA: &[u8] = b"bitblock/world/sorted-i64-be/v1";
/// Canonical schema identifier for a coordinate-history genesis receipt.
pub const GENESIS_SCHEMA: &[u8] = b"bitblock/history/genesis/v1";
const EVENT_SCHEMA: &[u8] = b"bitblock/history/event/v1";
/// Canonical schema identifier for a committed coordinate-history edit receipt.
pub const RECEIPT_SCHEMA: &[u8] = b"bitblock/history/receipt/v1";

const RECEIPT_FIXED_BYTES: usize = 32 + 8 + 8 + 32 + 32 + 2;

/// Reason an attempted coordinate-history edit left voxel occupancy unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoOpReason {
    BedrockProtected,
    AlreadyOn,
    AlreadyOff,
}

/// Deterministic provenance effect recorded when applying one edit to coordinate history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionEffect {
    Mint {
        coord: Coord,
        event: ContentTag,
    },
    Burn {
        coord: Coord,
        event: ContentTag,
        consumed_mint: ContentTag,
    },
    NoOp {
        coord: Coord,
        event: ContentTag,
        reason: NoOpReason,
    },
}

impl TransitionEffect {
    /// Returns the content-addressed event tag chained by this coordinate transition effect.
    pub const fn event(self) -> ContentTag {
        match self {
            Self::Mint { event, .. } | Self::Burn { event, .. } | Self::NoOp { event, .. } => event,
        }
    }
}

/// Committed coordinate-history receipt binding a tick's applied edits, effects, and post-world tag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    tag: ContentTag,
    parent: ContentTag,
    tick: u64,
    edits: Vec<Edit>,
    effects: Vec<TransitionEffect>,
    post_world: ContentTag,
}

impl Receipt {
    /// Returns the canonical content tag identifying this committed history receipt.
    pub const fn tag(&self) -> ContentTag {
        self.tag
    }

    /// Returns the content tag of the preceding history receipt or genesis record.
    pub const fn parent(&self) -> ContentTag {
        self.parent
    }

    /// Returns the simulation tick ordinal at which this history receipt was committed.
    pub const fn tick(&self) -> u64 {
        self.tick
    }

    /// Returns the ordered slice of coordinate edits committed in this history receipt.
    pub fn edits(&self) -> &[Edit] {
        &self.edits
    }

    /// Returns the ordered mint, burn, and no-op provenance effects produced by this receipt's edits.
    pub fn effects(&self) -> &[TransitionEffect] {
        &self.effects
    }

    /// Returns the canonical world content tag immediately after applying this receipt's edits.
    pub const fn post_world(&self) -> ContentTag {
        self.post_world
    }

    /// Returns the terminal event content tag chained by the last transition effect in this receipt.
    pub fn final_event(&self) -> ContentTag {
        self.effects
            .last()
            .expect("a committed receipt has at least one effect")
            .event()
    }

    /// Complete canonical payload hashed by [`Receipt::tag`].
    pub fn canonical_payload(&self) -> HistoryReceiptPayload {
        HistoryReceiptPayload {
            parent: self.parent,
            tick: self.tick,
            edits: self.edits.clone(),
            post_world: self.post_world,
            final_event: self.final_event(),
        }
    }
}

/// Decoded canonical fields of one coordinate-history receipt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryReceiptPayload {
    parent: ContentTag,
    tick: u64,
    edits: Vec<Edit>,
    post_world: ContentTag,
    final_event: ContentTag,
}

impl HistoryReceiptPayload {
    /// Validates tag kinds and edit count to construct a canonical history receipt payload.
    pub fn new(
        parent: ContentTag,
        tick: u64,
        edits: Vec<Edit>,
        post_world: ContentTag,
        final_event: ContentTag,
    ) -> Result<Self, HistoryReceiptCodecError> {
        if parent.kind() != TagKind::Receipt {
            return Err(HistoryReceiptCodecError::ParentKind {
                actual: parent.kind(),
            });
        }
        if edits.is_empty() {
            return Err(HistoryReceiptCodecError::EmptyEdits);
        }
        if post_world.kind() != TagKind::World {
            return Err(HistoryReceiptCodecError::PostWorldKind {
                actual: post_world.kind(),
            });
        }
        if final_event.kind() != TagKind::Event {
            return Err(HistoryReceiptCodecError::FinalEventKind {
                actual: final_event.kind(),
            });
        }
        canonical_receipt_payload_len(edits.len())?;
        Ok(Self {
            parent,
            tick,
            edits,
            post_world,
            final_event,
        })
    }

    /// Returns the parent receipt content tag committed by this history receipt payload.
    pub const fn parent(&self) -> ContentTag {
        self.parent
    }

    /// Returns the simulation tick ordinal encoded in this history receipt payload.
    pub const fn tick(&self) -> u64 {
        self.tick
    }

    /// Returns the ordered slice of coordinate edits encoded in this history receipt payload.
    pub fn edits(&self) -> &[Edit] {
        &self.edits
    }

    /// Returns the post-tick world content tag committed by this history receipt payload.
    pub const fn post_world(&self) -> ContentTag {
        self.post_world
    }

    /// Returns the final chained event content tag committed by this history receipt payload.
    pub const fn final_event(&self) -> ContentTag {
        self.final_event
    }

    /// Returns the exact canonical byte length of this history receipt payload.
    pub fn encoded_len(&self) -> usize {
        canonical_receipt_payload_len(self.edits.len())
            .expect("an admitted receipt payload has a representable length")
    }

    /// Encodes this history receipt payload into its canonical big-endian byte representation.
    pub fn encode_canonical(&self) -> Vec<u8> {
        encode_receipt_fields(
            self.parent,
            self.tick,
            &self.edits,
            self.post_world,
            self.final_event,
        )
    }

    /// Validates the byte length header and extracts the declared edit count from a canonical receipt.
    #[cfg(any(test, feature = "protocol-research"))]
    pub fn declared_edit_count(bytes: &[u8]) -> Result<usize, HistoryReceiptCodecError> {
        if bytes.len() < RECEIPT_FIXED_BYTES {
            return Err(HistoryReceiptCodecError::WrongLength {
                expected: RECEIPT_FIXED_BYTES,
                actual: bytes.len(),
            });
        }
        let edit_count_u64 =
            u64::from_be_bytes(bytes[40..48].try_into().expect("fixed count field"));
        let edit_count = usize::try_from(edit_count_u64)
            .map_err(|_| HistoryReceiptCodecError::LengthOverflow)?;
        if edit_count == 0 {
            return Err(HistoryReceiptCodecError::EmptyEdits);
        }
        let expected = canonical_receipt_payload_len(edit_count)?;
        if bytes.len() != expected {
            return Err(HistoryReceiptCodecError::WrongLength {
                expected,
                actual: bytes.len(),
            });
        }
        Ok(edit_count)
    }

    /// Decodes and validates a canonical history receipt payload from raw bytes.
    #[cfg(any(test, feature = "protocol-research"))]
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, HistoryReceiptCodecError> {
        let edit_count = Self::declared_edit_count(bytes)?;
        let tick = u64::from_be_bytes(bytes[32..40].try_into().expect("fixed tick field"));

        let mut edits = Vec::with_capacity(edit_count);
        let mut cursor = 48;
        for index in 0..edit_count {
            let end = cursor + EDIT_CANONICAL_LEN;
            let edit = Edit::decode_canonical(&bytes[cursor..end])
                .map_err(|source| HistoryReceiptCodecError::Edit { index, source })?;
            edits.push(edit);
            cursor = end;
        }
        let post_world_end = cursor + 32;
        let final_event_end = post_world_end + 32;
        let flags_end = final_event_end + 2;
        if bytes[final_event_end..flags_end] != [0, 0] {
            return Err(HistoryReceiptCodecError::ReservedFlags {
                actual: bytes[final_event_end..flags_end]
                    .try_into()
                    .expect("flags field has fixed length"),
            });
        }

        let payload = Self {
            parent: ContentTag::from_digest(
                TagKind::Receipt,
                bytes[..32]
                    .try_into()
                    .expect("parent digest has fixed length"),
            ),
            tick,
            edits,
            post_world: ContentTag::from_digest(
                TagKind::World,
                bytes[cursor..post_world_end]
                    .try_into()
                    .expect("world digest has fixed length"),
            ),
            final_event: ContentTag::from_digest(
                TagKind::Event,
                bytes[post_world_end..final_event_end]
                    .try_into()
                    .expect("event digest has fixed length"),
            ),
        };
        if payload.encode_canonical() != bytes {
            return Err(HistoryReceiptCodecError::NonCanonicalEncoding);
        }
        Ok(payload)
    }

    /// Computes the `TagKind::Receipt` content tag over this payload under `RECEIPT_SCHEMA`.
    pub fn content_tag(&self) -> ContentTag {
        let encoded = self.encode_canonical();
        hash_canonical(
            TagKind::Receipt,
            RECEIPT_SCHEMA,
            encoded.len() as u64,
            |sink| sink(&encoded),
        )
        .expect("canonical history receipt emits its declared length")
    }
}

/// Validation or canonical decoding failure for a `HistoryReceiptPayload`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryReceiptCodecError {
    ParentKind {
        actual: TagKind,
    },
    PostWorldKind {
        actual: TagKind,
    },
    FinalEventKind {
        actual: TagKind,
    },
    EmptyEdits,
    LengthOverflow,
    WrongLength {
        expected: usize,
        actual: usize,
    },
    Edit {
        index: usize,
        source: EditCodecError,
    },
    ReservedFlags {
        actual: [u8; 2],
    },
    NonCanonicalEncoding,
}

impl fmt::Display for HistoryReceiptCodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::ParentKind { actual } => write!(
                formatter,
                "history receipt parent must be receipt, received {}",
                actual.name()
            ),
            Self::PostWorldKind { actual } => write!(
                formatter,
                "history receipt post-world must be world, received {}",
                actual.name()
            ),
            Self::FinalEventKind { actual } => write!(
                formatter,
                "history receipt final event must be event, received {}",
                actual.name()
            ),
            Self::EmptyEdits => formatter.write_str("history receipt cannot contain zero edits"),
            Self::LengthOverflow => {
                formatter.write_str("history receipt length is not representable")
            }
            Self::WrongLength { expected, actual } => write!(
                formatter,
                "history receipt requires {expected} bytes, found {actual}"
            ),
            Self::Edit { index, source } => {
                write!(
                    formatter,
                    "history receipt edit {index} is invalid: {source}"
                )
            }
            Self::ReservedFlags { actual } => {
                write!(
                    formatter,
                    "history receipt reserved flags are {actual:02x?}"
                )
            }
            Self::NonCanonicalEncoding => {
                formatter.write_str("history receipt does not re-encode canonically")
            }
        }
    }
}

impl std::error::Error for HistoryReceiptCodecError {}

/// Exact canonical payload length for a receipt containing `edit_count` edits.
pub fn canonical_receipt_payload_len(edit_count: usize) -> Result<usize, HistoryReceiptCodecError> {
    edit_count
        .checked_mul(EDIT_CANONICAL_LEN)
        .and_then(|edit_bytes| RECEIPT_FIXED_BYTES.checked_add(edit_bytes))
        .ok_or(HistoryReceiptCodecError::LengthOverflow)
}

fn encode_receipt_fields(
    parent: ContentTag,
    tick: u64,
    edits: &[Edit],
    post_world: ContentTag,
    final_event: ContentTag,
) -> Vec<u8> {
    let encoded_len = canonical_receipt_payload_len(edits.len())
        .expect("an in-memory edit vector has a representable canonical receipt length");
    let mut encoded = Vec::with_capacity(encoded_len);
    encoded.extend_from_slice(&parent.digest());
    encoded.extend_from_slice(&tick.to_be_bytes());
    encoded.extend_from_slice(&(edits.len() as u64).to_be_bytes());
    for &edit in edits {
        encoded.extend_from_slice(&edit.encode_canonical());
    }
    encoded.extend_from_slice(&post_world.digest());
    encoded.extend_from_slice(&final_event.digest());
    encoded.extend_from_slice(&[0, 0]);
    debug_assert_eq!(encoded.len(), encoded_len);
    encoded
}

/// In-memory content-addressed coordinate history chain tracking receipts and active mint provenance.
#[derive(Clone, Debug)]
pub struct History {
    genesis: ContentTag,
    genesis_world: ContentTag,
    head: ContentTag,
    receipts: Vec<Receipt>,
    active_mints: HashMap<Coord, ContentTag>,
}

impl History {
    /// Initializes a coordinate history chain anchored at the genesis tag of `initial_world`.
    pub fn new(initial_world: &World) -> Self {
        let genesis_world = world_tag(initial_world);
        let genesis = genesis_tag(genesis_world);
        let active_mints = initial_world
            .iter_on()
            .filter(|&coord| !is_bedrock(coord))
            .map(|coord| (coord, genesis))
            .collect();
        Self {
            genesis,
            genesis_world,
            head: genesis,
            receipts: Vec::new(),
            active_mints,
        }
    }

    /// Returns the genesis receipt content tag at the root of this coordinate history chain.
    pub const fn genesis(&self) -> ContentTag {
        self.genesis
    }

    /// Returns the canonical world content tag of the initial genesis world.
    pub const fn genesis_world(&self) -> ContentTag {
        self.genesis_world
    }

    /// Exact canonical payload stored for this history's genesis record.
    pub const fn genesis_payload(&self) -> [u8; 32] {
        self.genesis_world.digest()
    }

    /// Returns the latest committed receipt content tag (or genesis tag if no edits were committed).
    pub const fn head(&self) -> ContentTag {
        self.head
    }

    /// Canonical tag of the world after the most recently committed edit receipt.
    ///
    /// This is the same tag that a fresh [`world_tag`] scan would produce, without sorting and
    /// hashing every occupied coordinate again for camera-only renders.
    pub fn current_world(&self) -> ContentTag {
        self.receipts
            .last()
            .map_or(self.genesis_world, Receipt::post_world)
    }

    /// Returns the chronological slice of committed edit receipts in this coordinate history.
    pub fn receipts(&self) -> &[Receipt] {
        &self.receipts
    }

    /// Looks up the active mint provenance tag for an occupied mutable coordinate.
    pub fn active_mint(&self, coord: Coord) -> Option<ContentTag> {
        self.active_mints.get(&coord).copied()
    }

    /// Returns the count of currently occupied mutable coordinates with active mint provenance.
    pub fn circulating_supply(&self) -> usize {
        self.active_mints.len()
    }

    /// Opens a staged coordinate-history transaction for `tick` parented at the current head.
    pub fn begin_tick(&self, tick: u64) -> TickTransaction {
        TickTransaction {
            parent: self.head,
            tick,
            event_head: self.head,
            edits: Vec::new(),
            effects: Vec::new(),
            provenance_updates: HashMap::new(),
            changes: 0,
        }
    }

    /// Builds the exact receipt that this transaction would commit without publishing it.
    ///
    /// `world` must already reflect every edit in `transaction`, just as it must for
    /// [`History::commit_tick`]. This is intended for append-before-publish persistence: persist
    /// and verify the candidate first, then consume the same transaction with `commit_tick`.
    pub fn preview_tick(&self, transaction: &TickTransaction, world: &World) -> Option<Receipt> {
        let (tag, post_world) = self.receipt_identity(transaction, world)?;
        Some(Receipt {
            tag,
            parent: transaction.parent,
            tick: transaction.tick,
            edits: transaction.edits.clone(),
            effects: transaction.effects.clone(),
            post_world,
        })
    }

    /// Commits a non-empty `TickTransaction` against the post-tick `world` and advances the history head.
    pub fn commit_tick(&mut self, transaction: TickTransaction, world: &World) -> Option<&Receipt> {
        let prepared = self.prepare_tick(transaction, world)?;
        Some(self.commit_prepared_tick(prepared, world))
    }

    /// Seals one nonpublished candidate so persistence can name it before live publication.
    /// Stage a tick without committing it, so a persistent store can admit the
    /// bytes first and commit only if that succeeded.
    pub fn prepare_tick(
        &self,
        transaction: TickTransaction,
        world: &World,
    ) -> Option<PreparedTick> {
        let (tag, post_world) = self.receipt_identity(&transaction, world)?;
        Some(PreparedTick {
            receipt: Receipt {
                tag,
                parent: transaction.parent,
                tick: transaction.tick,
                edits: transaction.edits,
                effects: transaction.effects,
                post_world,
            },
            provenance_updates: transaction.provenance_updates,
            #[cfg(any(test, feature = "protocol-research"))]
            changes: transaction.changes,
        })
    }

    /// Publishes a candidate previously sealed by [`History::prepare_tick`] without rehashing it.
    /// Commit what [`Self::prepare_tick`] staged.
    pub fn commit_prepared_tick(&mut self, prepared: PreparedTick, world: &World) -> &Receipt {
        assert_eq!(
            prepared.receipt.parent, self.head,
            "history changed after a tick was prepared"
        );
        if let Some(previous) = self.receipts.last() {
            assert!(
                prepared.receipt.tick > previous.tick,
                "prepared receipt-bearing tick must advance strictly"
            );
        }
        for (coord, mint) in prepared.provenance_updates {
            match mint {
                Some(mint) => {
                    self.active_mints.insert(coord, mint);
                }
                None => {
                    self.active_mints.remove(&coord);
                }
            }
        }
        assert_eq!(
            self.active_mints.len(),
            world.len().saturating_sub(1),
            "active mint provenance must equal mutable occupied supply"
        );

        self.head = prepared.receipt.tag;
        self.receipts.push(prepared.receipt);
        self.receipts
            .last()
            .expect("a prepared receipt was just published")
    }

    fn receipt_identity(
        &self,
        transaction: &TickTransaction,
        world: &World,
    ) -> Option<(ContentTag, ContentTag)> {
        assert_eq!(
            transaction.parent, self.head,
            "history changed while a tick transaction was open"
        );
        if transaction.edits.is_empty() {
            return None;
        }
        if let Some(previous) = self.receipts.last() {
            assert!(
                transaction.tick > previous.tick,
                "receipt-bearing tick must advance strictly"
            );
        }

        let post_world = world_tag(world);
        let tag = receipt_tag(
            transaction.parent,
            transaction.tick,
            &transaction.edits,
            post_world,
            transaction.event_head,
        );
        Some((tag, post_world))
    }

    /// Rebuilds the complete chain from `initial_world` and checks all cached effects and tags.
    pub fn verify(&self, initial_world: &World) -> Result<World, HistoryError> {
        let mut world = initial_world.clone();
        let mut rebuilt = Self::new(&world);
        if rebuilt.genesis != self.genesis || rebuilt.genesis_world != self.genesis_world {
            return Err(HistoryError::GenesisMismatch);
        }

        for (index, expected) in self.receipts.iter().enumerate() {
            if expected.parent != rebuilt.head
                || expected.edits.is_empty()
                || rebuilt
                    .receipts
                    .last()
                    .is_some_and(|previous| expected.tick <= previous.tick)
            {
                return Err(HistoryError::ReceiptMismatch { index });
            }
            let mut transaction = rebuilt.begin_tick(expected.tick);
            for &edit in &expected.edits {
                transaction.apply_edit(&rebuilt, &mut world, edit);
            }
            if transaction.effects != expected.effects {
                return Err(HistoryError::EffectMismatch { index });
            }
            rebuilt.commit_tick(transaction, &world);
            let actual = rebuilt
                .receipts
                .last()
                .expect("a nonempty expected receipt commits one rebuilt receipt");
            if actual != expected {
                return Err(HistoryError::ReceiptMismatch { index });
            }
        }

        if rebuilt.head != self.head {
            return Err(HistoryError::HeadMismatch);
        }
        if rebuilt.active_mints != self.active_mints {
            return Err(HistoryError::ProvenanceMismatch);
        }
        Ok(world)
    }
}

/// A tick staged by [`History::prepare_tick`] and not yet committed.
///
/// Its fields stay private: the point of the type is that a caller can inspect
/// what a tick *would* commit, admit those bytes to a store, and only then call
/// [`History::commit_prepared_tick`]. It cannot be constructed or altered from
/// outside.
pub struct PreparedTick {
    receipt: Receipt,
    provenance_updates: HashMap<Coord, Option<ContentTag>>,
    #[cfg(any(test, feature = "protocol-research"))]
    changes: usize,
}

#[cfg(any(test, feature = "protocol-research"))]
impl PreparedTick {
    /// The receipt this tick would commit.
    pub const fn receipt(&self) -> &Receipt {
        &self.receipt
    }

    /// How many coordinates the tick would change.
    pub const fn changes(&self) -> usize {
        self.changes
    }
}

/// Open per-tick coordinate edit transaction accumulating event chain tags and provenance updates.
pub struct TickTransaction {
    parent: ContentTag,
    tick: u64,
    event_head: ContentTag,
    edits: Vec<Edit>,
    effects: Vec<TransitionEffect>,
    provenance_updates: HashMap<Coord, Option<ContentTag>>,
    changes: usize,
}

impl TickTransaction {
    /// Applies `edit` to `world`, chains its event tag, and records the resulting mint, burn, or no-op effect.
    pub fn apply_edit(&mut self, history: &History, world: &mut World, edit: Edit) -> bool {
        let before = world.is_on(edit.coord);
        let event = event_tag(self.event_head, self.tick, edit);
        let changed = edit.apply(world);
        let after = world.is_on(edit.coord);

        let effect = if is_bedrock(edit.coord) {
            debug_assert!(before && after && !changed);
            TransitionEffect::NoOp {
                coord: edit.coord,
                event,
                reason: NoOpReason::BedrockProtected,
            }
        } else if !before && after {
            let old = self.current_mint(history, edit.coord);
            debug_assert!(old.is_none());
            self.provenance_updates.insert(edit.coord, Some(event));
            TransitionEffect::Mint {
                coord: edit.coord,
                event,
            }
        } else if before && !after {
            let consumed_mint = self
                .current_mint(history, edit.coord)
                .expect("an occupied mutable coordinate must have active mint provenance");
            self.provenance_updates.insert(edit.coord, None);
            TransitionEffect::Burn {
                coord: edit.coord,
                event,
                consumed_mint,
            }
        } else {
            debug_assert_eq!(before, after);
            debug_assert!(!changed);
            TransitionEffect::NoOp {
                coord: edit.coord,
                event,
                reason: if before {
                    NoOpReason::AlreadyOn
                } else {
                    NoOpReason::AlreadyOff
                },
            }
        };

        self.event_head = event;
        self.edits.push(edit);
        self.effects.push(effect);
        self.changes += usize::from(changed);
        changed
    }

    /// Returns the ordered coordinate edits applied so far within this tick transaction.
    pub fn edits(&self) -> &[Edit] {
        &self.edits
    }

    /// Returns the ordered transition effects recorded so far within this tick transaction.
    pub fn effects(&self) -> &[TransitionEffect] {
        &self.effects
    }

    /// Returns the number of applied edits that changed voxel occupancy during this tick transaction.
    pub const fn changes(&self) -> usize {
        self.changes
    }

    fn current_mint(&self, history: &History, coord: Coord) -> Option<ContentTag> {
        match self.provenance_updates.get(&coord) {
            Some(mint) => *mint,
            None => history.active_mint(coord),
        }
    }
}

/// Verification failure detected while replaying a coordinate `History` chain from genesis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryError {
    GenesisMismatch,
    EffectMismatch { index: usize },
    ReceiptMismatch { index: usize },
    HeadMismatch,
    ProvenanceMismatch,
}

impl fmt::Display for HistoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::GenesisMismatch => {
                formatter.write_str("history genesis does not match initial world")
            }
            Self::EffectMismatch { index } => {
                write!(formatter, "receipt {index} cached effects do not replay")
            }
            Self::ReceiptMismatch { index } => {
                write!(formatter, "receipt {index} content tag does not replay")
            }
            Self::HeadMismatch => formatter.write_str("history head does not match replayed head"),
            Self::ProvenanceMismatch => {
                formatter.write_str("active mint provenance does not match replay")
            }
        }
    }
}

impl std::error::Error for HistoryError {}

/// Computes the canonical `TagKind::World` content tag over all sorted occupied coordinates in `world`.
pub fn world_tag(world: &World) -> ContentTag {
    let mut coords = Vec::with_capacity(world.len());
    coords.extend(world.iter_on());
    coords.sort_unstable();
    let payload_len = 2 + WORLD_SCHEMA.len() + 8 + coords.len() * 24;
    hash_canonical(TagKind::World, WORLD_SCHEMA, payload_len as u64, |sink| {
        let schema_len = u16::try_from(WORLD_SCHEMA.len()).expect("world schema exceeds u16");
        sink(&schema_len.to_be_bytes());
        sink(WORLD_SCHEMA);
        sink(&(coords.len() as u64).to_be_bytes());
        for coord in coords {
            let mut encoded = [0_u8; 24];
            encoded[..8].copy_from_slice(&coord.x.to_be_bytes());
            encoded[8..16].copy_from_slice(&coord.y.to_be_bytes());
            encoded[16..].copy_from_slice(&coord.z.to_be_bytes());
            sink(&encoded);
        }
    })
    .expect("canonical world length must match its encoder")
}

#[cfg(test)]
fn world_tag_before_preallocation(world: &World) -> ContentTag {
    let mut coords: Vec<_> = world.iter_on().collect();
    coords.sort_unstable();
    let payload_len = 2 + WORLD_SCHEMA.len() + 8 + coords.len() * 24;
    hash_canonical(TagKind::World, WORLD_SCHEMA, payload_len as u64, |sink| {
        let schema_len = u16::try_from(WORLD_SCHEMA.len()).expect("world schema exceeds u16");
        sink(&schema_len.to_be_bytes());
        sink(WORLD_SCHEMA);
        sink(&(coords.len() as u64).to_be_bytes());
        for coord in coords {
            let mut encoded = [0_u8; 24];
            encoded[..8].copy_from_slice(&coord.x.to_be_bytes());
            encoded[8..16].copy_from_slice(&coord.y.to_be_bytes());
            encoded[16..].copy_from_slice(&coord.z.to_be_bytes());
            sink(&encoded);
        }
    })
    .expect("canonical world length must match its encoder")
}

fn genesis_tag(genesis_world: ContentTag) -> ContentTag {
    debug_assert_eq!(genesis_world.kind(), TagKind::World);
    let payload = genesis_world.digest();
    hash_canonical(
        TagKind::Receipt,
        GENESIS_SCHEMA,
        payload.len() as u64,
        |sink| sink(&payload),
    )
    .expect("genesis encoding has a fixed length")
}

fn event_tag(parent: ContentTag, tick: u64, edit: Edit) -> ContentTag {
    hash_canonical(TagKind::Event, EVENT_SCHEMA, 65, |sink| {
        sink(&parent.digest());
        sink(&tick.to_be_bytes());
        encode_edit(edit, sink);
    })
    .expect("canonical event encoding has a fixed length")
}

fn receipt_tag(
    parent: ContentTag,
    tick: u64,
    edits: &[Edit],
    post_world: ContentTag,
    final_event: ContentTag,
) -> ContentTag {
    let encoded = encode_receipt_fields(parent, tick, edits, post_world, final_event);
    hash_canonical(
        TagKind::Receipt,
        RECEIPT_SCHEMA,
        encoded.len() as u64,
        |sink| sink(&encoded),
    )
    .expect("canonical receipt length must match its encoder")
}

fn encode_edit(edit: Edit, sink: &mut dyn FnMut(&[u8])) {
    sink(&edit.encode_canonical());
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: Coord = Coord::new(1, -2, 3);

    fn apply_tick(history: &mut History, world: &mut World, tick: u64, edits: &[Edit]) {
        let mut transaction = history.begin_tick(tick);
        for &edit in edits {
            transaction.apply_edit(history, world, edit);
        }
        history.commit_tick(transaction, world);
    }

    fn parse_tag(text: &str) -> ContentTag {
        ContentTag::parse_full(text).expect("known-answer tag must parse canonically")
    }

    fn apply_one_receipt(initial: &World, tick: u64, edits: &[Edit]) -> (World, History) {
        let mut world = initial.clone();
        let mut history = History::new(initial);
        apply_tick(&mut history, &mut world, tick, edits);
        assert_eq!(history.receipts().len(), 1);
        (world, history)
    }

    fn effect_name(effect: TransitionEffect) -> &'static str {
        match effect {
            TransitionEffect::Mint { .. } => "mint",
            TransitionEffect::Burn { .. } => "burn",
            TransitionEffect::NoOp {
                reason: NoOpReason::BedrockProtected,
                ..
            } => "bedrock-protected",
            TransitionEffect::NoOp {
                reason: NoOpReason::AlreadyOn,
                ..
            } => "already-on",
            TransitionEffect::NoOp {
                reason: NoOpReason::AlreadyOff,
                ..
            } => "already-off",
        }
    }

    #[test]
    fn world_tag_is_independent_of_sparse_hashmap_insertion_order() {
        let points = [P, Coord::new(-16, 17, 9), Coord::new(33, 0, -8)];
        let mut forward = World::new();
        let mut reverse = World::new();
        for &point in &points {
            forward.set_on(point);
        }
        for &point in points.iter().rev() {
            reverse.set_on(point);
        }
        assert_eq!(world_tag(&forward), world_tag(&reverse));
    }

    #[test]
    fn preallocated_world_tag_matches_the_prior_collection_path() {
        let points = [
            P,
            Coord::new(-16, 17, 9),
            Coord::new(33, 0, -8),
            Coord::new(i64::MIN, 0, i64::MAX),
            Coord::new(i64::MAX, -1, i64::MIN),
        ];
        let mut forward = World::new();
        let mut reverse = World::new();
        for &point in &points {
            forward.set_on(point);
        }
        for &point in points.iter().rev() {
            reverse.set_on(point);
        }
        assert!(forward.set_off(P));
        assert!(reverse.set_off(P));

        for world in [&World::new(), &forward, &reverse] {
            assert_eq!(world_tag(world), world_tag_before_preallocation(world));
        }
        assert_eq!(world_tag(&forward), world_tag(&reverse));
    }

    #[test]
    fn mint_burn_noop_and_remint_have_distinct_permanent_occurrences() {
        let initial = World::new();
        let mut world = initial.clone();
        let mut history = History::new(&world);
        apply_tick(
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

        let receipt = &history.receipts()[0];
        assert_eq!(receipt.effects().len(), 6);
        let first_mint = match receipt.effects()[0] {
            TransitionEffect::Mint { event, .. } => event,
            other => panic!("expected mint, got {other:?}"),
        };
        assert!(matches!(
            receipt.effects()[1],
            TransitionEffect::NoOp {
                reason: NoOpReason::AlreadyOn,
                ..
            }
        ));
        assert!(matches!(
            receipt.effects()[2],
            TransitionEffect::Burn {
                consumed_mint,
                ..
            } if consumed_mint == first_mint
        ));
        assert!(matches!(
            receipt.effects()[3],
            TransitionEffect::NoOp {
                reason: NoOpReason::AlreadyOff,
                ..
            }
        ));
        let remint = match receipt.effects()[4] {
            TransitionEffect::Mint { event, .. } => event,
            other => panic!("expected remint, got {other:?}"),
        };
        assert_ne!(first_mint, remint);
        assert!(matches!(
            receipt.effects()[5],
            TransitionEffect::NoOp {
                reason: NoOpReason::BedrockProtected,
                ..
            }
        ));
        assert_eq!(history.active_mint(P), Some(remint));
        assert_eq!(history.circulating_supply(), 1);
        assert_eq!(world.len(), 2);
        assert!(history.verify(&initial).is_ok());
    }

    #[test]
    fn empty_ticks_do_not_grow_coordinate_history() {
        let world = World::new();
        let mut history = History::new(&world);
        let transaction = history.begin_tick(9);
        assert!(history.commit_tick(transaction, &world).is_none());
        assert!(history.receipts().is_empty());
        assert_eq!(history.head(), history.genesis());
    }

    #[test]
    fn receipt_parent_chain_and_replay_reconstruct_the_world() {
        let initial = World::new();
        let mut world = initial.clone();
        let mut history = History::new(&world);
        assert_eq!(history.current_world(), world_tag(&world));
        apply_tick(&mut history, &mut world, 2, &[Edit::new(Operation::On, P)]);
        assert_eq!(history.current_world(), world_tag(&world));
        apply_tick(
            &mut history,
            &mut world,
            8,
            &[Edit::new(Operation::On, Coord::new(7, 8, 9))],
        );

        assert_eq!(history.receipts()[0].parent(), history.genesis());
        assert_eq!(history.receipts()[1].parent(), history.receipts()[0].tag());
        assert_eq!(history.head(), history.receipts()[1].tag());
        assert_eq!(history.current_world(), world_tag(&world));
        let replayed = history.verify(&initial).unwrap();
        assert_eq!(world_tag(&replayed), world_tag(&world));
    }

    #[test]
    fn genesis_materialization_exposes_the_exact_schema_payload_and_tag() {
        let mut world = World::new();
        world.set_on(P);
        let history = History::new(&world);

        assert_eq!(history.genesis_payload(), history.genesis_world().digest());
        let payload = history.genesis_payload();
        let reconstructed = hash_canonical(
            TagKind::Receipt,
            GENESIS_SCHEMA,
            payload.len() as u64,
            |sink| sink(&payload),
        )
        .unwrap();
        assert_eq!(reconstructed, history.genesis());
    }

    #[test]
    fn receipt_codec_round_trips_and_names_the_live_candidate() {
        let initial = World::new();
        let mut world = initial.clone();
        let history = History::new(&initial);
        let mut transaction = history.begin_tick(7);
        transaction.apply_edit(&history, &mut world, Edit::new(Operation::On, P));
        transaction.apply_edit(
            &history,
            &mut world,
            Edit::new(Operation::Toggle, Coord::new(-8, 9, i64::MAX)),
        );

        let candidate = history.preview_tick(&transaction, &world).unwrap();
        let payload = candidate.canonical_payload();
        let encoded = payload.encode_canonical();
        assert_eq!(
            payload.encoded_len(),
            RECEIPT_FIXED_BYTES + 2 * EDIT_CANONICAL_LEN
        );
        assert_eq!(
            HistoryReceiptPayload::decode_canonical(&encoded),
            Ok(payload.clone())
        );
        assert_eq!(payload.content_tag(), candidate.tag());
        assert_eq!(payload.parent(), candidate.parent());
        assert_eq!(payload.tick(), candidate.tick());
        assert_eq!(payload.edits(), candidate.edits());
        assert_eq!(payload.post_world(), candidate.post_world());
        assert_eq!(payload.final_event(), candidate.final_event());
    }

    #[test]
    fn receipt_codec_rejects_every_truncation_and_an_overlong_payload() {
        let initial = World::new();
        let mut world = initial.clone();
        let history = History::new(&initial);
        let mut transaction = history.begin_tick(1);
        transaction.apply_edit(&history, &mut world, Edit::new(Operation::On, P));
        let encoded = history
            .preview_tick(&transaction, &world)
            .unwrap()
            .canonical_payload()
            .encode_canonical();

        for length in 0..encoded.len() {
            assert!(
                HistoryReceiptPayload::decode_canonical(&encoded[..length]).is_err(),
                "proper prefix of length {length} was accepted"
            );
        }
        let mut overlong = encoded;
        overlong.push(0);
        assert!(matches!(
            HistoryReceiptPayload::decode_canonical(&overlong),
            Err(HistoryReceiptCodecError::WrongLength { .. })
        ));
    }

    #[test]
    fn receipt_codec_rejects_zero_or_inconsistent_edit_counts() {
        let initial = World::new();
        let mut world = initial.clone();
        let history = History::new(&initial);
        let mut transaction = history.begin_tick(1);
        transaction.apply_edit(&history, &mut world, Edit::new(Operation::On, P));
        let encoded = history
            .preview_tick(&transaction, &world)
            .unwrap()
            .canonical_payload()
            .encode_canonical();

        let mut zero_count = encoded.clone();
        zero_count[40..48].fill(0);
        assert_eq!(
            HistoryReceiptPayload::decode_canonical(&zero_count),
            Err(HistoryReceiptCodecError::EmptyEdits)
        );

        let mut inconsistent_count = encoded;
        inconsistent_count[40..48].copy_from_slice(&2_u64.to_be_bytes());
        assert_eq!(
            HistoryReceiptPayload::decode_canonical(&inconsistent_count),
            Err(HistoryReceiptCodecError::WrongLength {
                expected: RECEIPT_FIXED_BYTES + 2 * EDIT_CANONICAL_LEN,
                actual: RECEIPT_FIXED_BYTES + EDIT_CANONICAL_LEN,
            })
        );
    }

    #[test]
    fn receipt_codec_rejects_an_invalid_edit_and_nonzero_reserved_flags() {
        let initial = World::new();
        let mut world = initial.clone();
        let history = History::new(&initial);
        let mut transaction = history.begin_tick(1);
        transaction.apply_edit(&history, &mut world, Edit::new(Operation::On, P));
        let encoded = history
            .preview_tick(&transaction, &world)
            .unwrap()
            .canonical_payload()
            .encode_canonical();

        let mut invalid_edit = encoded.clone();
        invalid_edit[48] = u8::MAX;
        assert_eq!(
            HistoryReceiptPayload::decode_canonical(&invalid_edit),
            Err(HistoryReceiptCodecError::Edit {
                index: 0,
                source: EditCodecError::UnknownOperation(u8::MAX),
            })
        );

        for flag_index in encoded.len() - 2..encoded.len() {
            let mut nonzero_flags = encoded.clone();
            nonzero_flags[flag_index] = 1;
            assert!(matches!(
                HistoryReceiptPayload::decode_canonical(&nonzero_flags),
                Err(HistoryReceiptCodecError::ReservedFlags { .. })
            ));
        }
    }

    #[test]
    fn preview_is_nonpublishing_and_commit_publishes_the_identical_receipt() {
        let initial = World::new();
        let mut world = initial.clone();
        let mut history = History::new(&initial);
        let initial_head = history.head();
        let mut transaction = history.begin_tick(11);
        transaction.apply_edit(&history, &mut world, Edit::new(Operation::On, P));

        let candidate = history.preview_tick(&transaction, &world).unwrap();
        assert_eq!(history.head(), initial_head);
        assert!(history.receipts().is_empty());
        assert_eq!(history.circulating_supply(), 0);
        assert_eq!(history.current_world(), history.genesis_world());
        assert_eq!(
            history.preview_tick(&transaction, &world),
            Some(candidate.clone())
        );

        let committed = history.commit_tick(transaction, &world).unwrap();
        assert_eq!(committed, &candidate);
        assert_eq!(history.head(), candidate.tag());
        assert_eq!(history.circulating_supply(), 1);
    }

    #[test]
    #[should_panic(expected = "history changed while a tick transaction was open")]
    fn preview_preserves_the_stale_parent_invariant() {
        let initial = World::new();
        let mut world = initial.clone();
        let mut history = History::new(&initial);
        let stale = history.begin_tick(2);
        apply_tick(&mut history, &mut world, 1, &[Edit::new(Operation::On, P)]);
        let _ = history.preview_tick(&stale, &world);
    }

    #[test]
    #[should_panic(expected = "receipt-bearing tick must advance strictly")]
    fn preview_preserves_the_strict_tick_invariant() {
        let initial = World::new();
        let mut world = initial.clone();
        let mut history = History::new(&initial);
        apply_tick(&mut history, &mut world, 3, &[Edit::new(Operation::On, P)]);
        let mut transaction = history.begin_tick(3);
        transaction.apply_edit(
            &history,
            &mut world,
            Edit::new(Operation::On, Coord::new(4, 5, 6)),
        );
        let _ = history.preview_tick(&transaction, &world);
    }

    #[test]
    fn independent_scalar_batch_vectors_match_live_history() {
        // `tools/semantic_batch_vectors.py` reconstructs these values without importing Rust.
        let initial_off = World::new();
        let (cold_world, cold_history) = apply_one_receipt(
            &initial_off,
            4,
            &[
                Edit::new(Operation::On, P),
                Edit::new(Operation::On, P),
                Edit::new(Operation::Off, P),
                Edit::new(Operation::Off, P),
                Edit::new(Operation::Toggle, P),
            ],
        );
        let cold = &cold_history.receipts()[0];
        assert_eq!(
            cold.tag(),
            parse_tag("#bb.receipt.sha256.psal6njm4ndyap7wclfv55gnlxwwou4eq2rvfotazrxzragceusa")
        );
        assert_eq!(
            cold.effects()[0].event(),
            parse_tag("#bb.event.sha256.znebuddob7z4u2vwyqe7bwhclvyeqtgya7252fm5qq7hewauh2kq")
        );
        assert_eq!(
            cold.effects()[4].event(),
            parse_tag("#bb.event.sha256.6kcbq6sgg7zr5nymes5da5ksm4kt2y6fpytmfwlbs3xmn4zdsx5q")
        );
        assert_eq!(
            cold.effects()
                .iter()
                .copied()
                .map(effect_name)
                .collect::<Vec<_>>(),
            ["mint", "already-on", "burn", "already-off", "mint"]
        );
        assert!(matches!(
            cold.effects()[2],
            TransitionEffect::Burn { consumed_mint, .. } if consumed_mint == cold.effects()[0].event()
        ));
        assert!(cold_world.is_on(P));
        assert_eq!(cold_history.active_mint(P), Some(cold.effects()[4].event()));
        assert!(cold_history.verify(&initial_off).is_ok());

        let (reuse_world, reuse_history) = apply_one_receipt(
            &initial_off,
            7,
            &[
                Edit::new(Operation::On, P),
                Edit::new(Operation::Off, P),
                Edit::new(Operation::On, P),
                Edit::new(Operation::Off, P),
            ],
        );
        let reuse = &reuse_history.receipts()[0];
        assert_eq!(
            reuse.tag(),
            parse_tag("#bb.receipt.sha256.fakwjpoid3eqaztgfahmlepmfawgi7yffed345eovjysn2izuf5a")
        );
        assert_eq!(
            reuse.effects()[0].event(),
            parse_tag("#bb.event.sha256.h7rmaq3zvlig2btqavazt37nhrk7el2pwulowcvhzmzbm245i6ua")
        );
        assert_eq!(
            reuse.effects()[2].event(),
            parse_tag("#bb.event.sha256.zr3ehfiz5xxheosxvxydb27m4vpfibfbpjkg7cicuob6tsf46cja")
        );
        assert_eq!(
            reuse
                .effects()
                .iter()
                .copied()
                .map(effect_name)
                .collect::<Vec<_>>(),
            ["mint", "burn", "mint", "burn"]
        );
        assert!(matches!(
            reuse.effects()[1],
            TransitionEffect::Burn { consumed_mint, .. } if consumed_mint == reuse.effects()[0].event()
        ));
        assert!(matches!(
            reuse.effects()[3],
            TransitionEffect::Burn { consumed_mint, .. } if consumed_mint == reuse.effects()[2].event()
        ));
        assert!(!reuse_world.is_on(P));
        assert_eq!(reuse_history.active_mint(P), None);
        assert!(reuse_history.verify(&initial_off).is_ok());

        let mut initial_on = World::new();
        initial_on.set_on(P);
        let initial_mint = History::new(&initial_on).genesis();
        let (warm_world, warm_history) =
            apply_one_receipt(&initial_on, 9, &[Edit::new(Operation::Off, P)]);
        let warm = &warm_history.receipts()[0];
        assert_eq!(
            warm.tag(),
            parse_tag("#bb.receipt.sha256.v4rixvqu53nkaraowd4z26s2r55fv4rnz6qnq2dbtjaccxiljffq")
        );
        assert_eq!(
            warm.effects()[0].event(),
            parse_tag("#bb.event.sha256.gibrybl5w6zs3bq73t3e3qg6qjrhifcf2ce6xhnxyttsnukxvwma")
        );
        assert!(matches!(
            warm.effects()[0],
            TransitionEffect::Burn { consumed_mint, .. } if consumed_mint == initial_mint
        ));
        assert!(!warm_world.is_on(P));
        assert_eq!(warm_history.active_mint(P), None);
        assert!(warm_history.verify(&initial_on).is_ok());

        let (noop_world, noop_history) = apply_one_receipt(
            &initial_off,
            11,
            &[Edit::new(Operation::Off, P), Edit::new(Operation::On, P)],
        );
        let noop = &noop_history.receipts()[0];
        assert_eq!(
            noop.tag(),
            parse_tag("#bb.receipt.sha256.kdqtkvuuljhq4xjmkft44bk46rd4rqnty2h6sbozjwg2o4nnjgmq")
        );
        assert_eq!(
            noop.effects()
                .iter()
                .copied()
                .map(effect_name)
                .collect::<Vec<_>>(),
            ["already-off", "mint"]
        );
        assert_ne!(
            noop.effects()[1].event(),
            event_tag(noop.parent(), 11, Edit::new(Operation::On, P))
        );
        assert!(noop_world.is_on(P));
        assert_eq!(noop_history.active_mint(P), Some(noop.effects()[1].event()));
        assert!(noop_history.verify(&initial_off).is_ok());

        let (toggle_world, toggle_history) = apply_one_receipt(
            &initial_off,
            13,
            &[
                Edit::new(Operation::Toggle, P),
                Edit::new(Operation::Toggle, P),
            ],
        );
        let toggle = &toggle_history.receipts()[0];
        assert_eq!(
            toggle.tag(),
            parse_tag("#bb.receipt.sha256.li2iopqyi652bk4hdt75ad356z5iqryugxmxkefhfroxvhtpetja")
        );
        assert_eq!(
            toggle
                .effects()
                .iter()
                .copied()
                .map(effect_name)
                .collect::<Vec<_>>(),
            ["mint", "burn"]
        );
        assert!(!toggle_world.is_on(P));
        assert_eq!(toggle_history.active_mint(P), None);
        assert!(toggle_history.verify(&initial_off).is_ok());

        let (bedrock_world, bedrock_history) = apply_one_receipt(
            &initial_off,
            17,
            &[
                Edit::new(Operation::On, BEDROCK_COORD),
                Edit::new(Operation::Off, BEDROCK_COORD),
                Edit::new(Operation::Toggle, BEDROCK_COORD),
            ],
        );
        let bedrock = &bedrock_history.receipts()[0];
        assert_eq!(
            bedrock.tag(),
            parse_tag("#bb.receipt.sha256.amsihpcd2ydi5243jdzuoa2g3a5tbh726k5zru6dfmyargeul6ja")
        );
        assert_eq!(
            bedrock
                .effects()
                .iter()
                .copied()
                .map(effect_name)
                .collect::<Vec<_>>(),
            [
                "bedrock-protected",
                "bedrock-protected",
                "bedrock-protected"
            ]
        );
        assert!(bedrock_world.is_on(BEDROCK_COORD));
        assert_eq!(bedrock_history.circulating_supply(), 0);
        assert!(bedrock_history.verify(&initial_off).is_ok());

        let edge = Coord::new(i64::MIN, 0, i64::MAX);
        let (edge_world, edge_history) =
            apply_one_receipt(&initial_off, 23, &[Edit::new(Operation::On, edge)]);
        let edge_receipt = &edge_history.receipts()[0];
        assert_eq!(
            edge_receipt.tag(),
            parse_tag("#bb.receipt.sha256.dzihycw2ucnfjaulmme6pqlgv3n4u65gn4bqi36xvzkmxymzn22q")
        );
        assert_eq!(effect_name(edge_receipt.effects()[0]), "mint");
        assert!(edge_world.is_on(edge));
        assert_eq!(
            edge_history.active_mint(edge),
            Some(edge_receipt.effects()[0].event())
        );
        assert!(edge_history.verify(&initial_off).is_ok());
    }
}
