//! Tag-only local issuance-provenance episodes derived from verified coordinate history.
//!
//! A mutable off-to-on transition is identified by its existing `#bb.event...` mint tag. A later
//! on-to-off transition may name that exact tag as its consumed mint. This is a local replay
//! projection: it has no balances, owners, transfers, signatures, custody, consensus, stake,
//! elapsed-time, cryptocurrency, or Layer-1 claim.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::Path;

use crate::history::{History, Receipt, TransitionEffect, world_tag};
use crate::history_store::{
    HistoryAppendReceipt, HistoryCreateReceipt, HistoryJournal, HistoryRecoveryJournal,
    HistoryReplayLimits, HistoryStoreError, InMemoryHistoryVerificationError, OpenHistoryJournal,
    verify_in_memory_history_with_limits,
};
use crate::journal::{
    AnchorPolicy, BackupReceipt, RecoveryReport, RestoreReceipt, ScanReport, TailDiscardReceipt,
};
use crate::local_bounds::LocalStorageProfile;
use crate::program::Edit;
use crate::protocol::{ContentTag, TagError, TagKind, hash_canonical};
use crate::world::{Coord, World, is_bedrock};

/// Canonical schema for `CoordinateEpisodeEvidence` receipt payloads.
pub const COORDINATE_EPISODE_EVIDENCE_SCHEMA: &[u8] = b"bitblock/coordinate-episode-evidence/v1";
/// Canonical schema for the tag-only coordinate-episode derivation policy.
pub const COORDINATE_EPISODE_DERIVATION_POLICY_SCHEMA: &[u8] =
    b"bitblock/coordinate-episode-derivation-policy/v1";
/// Hard upper bound on episodes admitted in a single `CoordinateEpisodeEvidence` payload.
pub const MAX_COORDINATE_EPISODES: usize = 262_144;

const HEADER_BYTES: usize = 32 + 32 + 32 + 32 + 8;
const EPISODE_FIXED_BYTES: usize = 24 + 32 + 1;
const CLOSED_EPISODE_BYTES: usize = EPISODE_FIXED_BYTES + 32;
const ACTIVE_MARKER: u8 = 0;
const CLOSED_MARKER: u8 = 1;

/// One actual mutable off-to-on occurrence, named by its mint-event tag rather than a numeric ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoordinateEpisode {
    coord: Coord,
    mint: ContentTag,
    consuming_burn: Option<ContentTag>,
}

impl CoordinateEpisode {
    /// Returns the world `Coord` where this mint-to-burn episode occurred.
    pub const fn coord(self) -> Coord {
        self.coord
    }

    /// Returns the `ContentTag` of the off-to-on mint event that opened this episode.
    pub const fn mint(self) -> ContentTag {
        self.mint
    }

    /// Returns the `ContentTag` of the on-to-off burn event that closed this episode, if any.
    pub const fn consuming_burn(self) -> Option<ContentTag> {
        self.consuming_burn
    }

    /// Returns `true` if this episode has not yet been closed by a burn event.
    pub const fn is_active(self) -> bool {
        self.consuming_burn.is_none()
    }

    fn encoded_len(self) -> usize {
        if self.consuming_burn.is_some() {
            CLOSED_EPISODE_BYTES
        } else {
            EPISODE_FIXED_BYTES
        }
    }

    fn encode(self, sink: &mut dyn FnMut(&[u8])) {
        sink(&self.coord.x.to_be_bytes());
        sink(&self.coord.y.to_be_bytes());
        sink(&self.coord.z.to_be_bytes());
        sink(&self.mint.digest());
        match self.consuming_burn {
            None => sink(&[ACTIVE_MARKER]),
            Some(burn) => {
                sink(&[CLOSED_MARKER]);
                sink(&burn.digest());
            }
        }
    }
}

/// A bounded, canonically encoded replay projection of mutable mint/burn episodes.
///
/// The outer tag is a receipt-kind content tag under a distinct schema. It is evidence of this
/// exact verified local history prefix only; no new asset, account, or chain identity is minted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoordinateEpisodeEvidence {
    tag: ContentTag,
    policy: ContentTag,
    history_genesis: ContentTag,
    history_head: ContentTag,
    final_world: ContentTag,
    episodes: Vec<CoordinateEpisode>,
}

impl CoordinateEpisodeEvidence {
    /// Derives episodes under the standard bounded semantic-replay policy.
    pub fn derive(
        initial_world: &World,
        history: &History,
    ) -> Result<Self, CoordinateEpisodeError> {
        Self::derive_with_limits(initial_world, history, HistoryReplayLimits::DEFAULT)
    }

    /// Derives episodes only after complete bounded replay has reconstructed effects and tags.
    pub fn derive_with_limits(
        initial_world: &World,
        claimed: &History,
        limits: HistoryReplayLimits,
    ) -> Result<Self, CoordinateEpisodeError> {
        let verified = verify_in_memory_history_with_limits(initial_world, claimed, limits)?;
        let history = verified.history();
        let final_world = world_tag(verified.final_world());
        if final_world != history.current_world() {
            return Err(CoordinateEpisodeError::VerifiedWorldTagMismatch {
                replayed: final_world,
                history: history.current_world(),
            });
        }

        let episodes = derive_episodes(history, MAX_COORDINATE_EPISODES)?;

        Self::from_fields(
            coordinate_episode_derivation_policy_tag(),
            history.genesis(),
            history.head(),
            final_world,
            episodes,
        )
    }

    /// Returns the receipt-kind `ContentTag` of this episode evidence payload.
    pub const fn tag(&self) -> ContentTag {
        self.tag
    }

    /// Returns the `ContentTag` of the derivation policy bound to this evidence.
    pub const fn policy(&self) -> ContentTag {
        self.policy
    }

    /// Returns the genesis `ContentTag` of the source coordinate history.
    pub const fn history_genesis(&self) -> ContentTag {
        self.history_genesis
    }

    /// Returns the head receipt `ContentTag` of the source coordinate history.
    pub const fn history_head(&self) -> ContentTag {
        self.history_head
    }

    /// Returns the `ContentTag` of the final world state after replaying the history.
    pub const fn final_world(&self) -> ContentTag {
        self.final_world
    }

    /// Returns the ordered slice of post-genesis `CoordinateEpisode` entries.
    pub fn episodes(&self) -> &[CoordinateEpisode] {
        &self.episodes
    }

    /// Computes the canonical serialized byte length of this evidence payload.
    pub fn encoded_len(&self) -> Result<usize, CoordinateEpisodeError> {
        encoded_len(&self.episodes)
    }

    /// Validates and encodes this evidence into its canonical byte representation.
    pub fn try_encode_canonical(&self) -> Result<Vec<u8>, CoordinateEpisodeError> {
        validate_fields(
            self.policy,
            self.history_genesis,
            self.history_head,
            self.final_world,
            &self.episodes,
        )?;
        let length = self.encoded_len()?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(length)
            .map_err(|_| CoordinateEpisodeError::AllocationFailed { requested: length })?;
        encode_fields(
            self.policy,
            self.history_genesis,
            self.history_head,
            self.final_world,
            &self.episodes,
            &mut |chunk| bytes.extend_from_slice(chunk),
        );
        debug_assert_eq!(bytes.len(), length);
        Ok(bytes)
    }

    /// Decodes and validates a `CoordinateEpisodeEvidence` from canonical bytes.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, CoordinateEpisodeError> {
        let mut reader = Reader::new(bytes);
        let policy = reader.tag(TagKind::Policy, "derivation policy")?;
        let history_genesis = reader.tag(TagKind::Receipt, "history genesis")?;
        let history_head = reader.tag(TagKind::Receipt, "history head")?;
        let final_world = reader.tag(TagKind::World, "final world")?;
        let count = reader.count()?;
        if count > MAX_COORDINATE_EPISODES {
            return Err(CoordinateEpisodeError::TooManyEpisodes {
                actual: count,
                maximum: MAX_COORDINATE_EPISODES,
            });
        }
        let minimum = count
            .checked_mul(EPISODE_FIXED_BYTES)
            .ok_or(CoordinateEpisodeError::LengthOverflow)?;
        reader.require("episode payload", minimum)?;
        let mut episodes = Vec::new();
        episodes
            .try_reserve_exact(count)
            .map_err(|_| CoordinateEpisodeError::AllocationFailed { requested: count })?;
        for index in 0..count {
            let coord = Coord::new(
                reader.i64("episode coordinate x")?,
                reader.i64("episode coordinate y")?,
                reader.i64("episode coordinate z")?,
            );
            let mint = reader.tag(TagKind::Event, "mint event")?;
            let consuming_burn = match reader.u8("episode state")? {
                ACTIVE_MARKER => None,
                CLOSED_MARKER => Some(reader.tag(TagKind::Event, "consuming burn event")?),
                marker => {
                    return Err(CoordinateEpisodeError::UnknownEpisodeState { index, marker });
                }
            };
            episodes.push(CoordinateEpisode {
                coord,
                mint,
                consuming_burn,
            });
        }
        reader.finish()?;

        let evidence =
            Self::from_fields(policy, history_genesis, history_head, final_world, episodes)?;
        if evidence.try_encode_canonical()?.as_slice() != bytes {
            return Err(CoordinateEpisodeError::NonCanonicalEncoding);
        }
        Ok(evidence)
    }

    /// Checks the tag and then re-derives the complete episode relation from a verified history.
    pub fn verify_against(
        &self,
        initial_world: &World,
        history: &History,
        limits: HistoryReplayLimits,
    ) -> Result<(), CoordinateEpisodeError> {
        self.verify()?;
        let expected = Self::derive_with_limits(initial_world, history, limits)?;
        if self != &expected {
            return Err(CoordinateEpisodeError::DerivationMismatch);
        }
        Ok(())
    }

    /// Verifies field invariants, event uniqueness, and the outer receipt `ContentTag`.
    pub fn verify(&self) -> Result<(), CoordinateEpisodeError> {
        validate_fields(
            self.policy,
            self.history_genesis,
            self.history_head,
            self.final_world,
            &self.episodes,
        )?;
        if self.tag.kind() != TagKind::Receipt {
            return Err(CoordinateEpisodeError::WrongEvidenceTagKind {
                actual: self.tag.kind(),
            });
        }
        if self.tag != self.recompute_tag()? {
            return Err(CoordinateEpisodeError::TagMismatch);
        }
        Ok(())
    }

    fn from_fields(
        policy: ContentTag,
        history_genesis: ContentTag,
        history_head: ContentTag,
        final_world: ContentTag,
        episodes: Vec<CoordinateEpisode>,
    ) -> Result<Self, CoordinateEpisodeError> {
        validate_fields(
            policy,
            history_genesis,
            history_head,
            final_world,
            &episodes,
        )?;
        let mut evidence = Self {
            tag: ContentTag::from_digest(TagKind::Receipt, [0; 32]),
            policy,
            history_genesis,
            history_head,
            final_world,
            episodes,
        };
        evidence.tag = evidence.recompute_tag()?;
        Ok(evidence)
    }

    fn recompute_tag(&self) -> Result<ContentTag, CoordinateEpisodeError> {
        let length = self.encoded_len()?;
        hash_canonical(
            TagKind::Receipt,
            COORDINATE_EPISODE_EVIDENCE_SCHEMA,
            length as u64,
            |sink| {
                encode_fields(
                    self.policy,
                    self.history_genesis,
                    self.history_head,
                    self.final_world,
                    &self.episodes,
                    sink,
                )
            },
        )
        .map_err(CoordinateEpisodeError::Hash)
    }
}

/// Static policy identity for this exact tag-only derivation rule.
pub fn coordinate_episode_derivation_policy_tag() -> ContentTag {
    hash_canonical(
        TagKind::Policy,
        COORDINATE_EPISODE_DERIVATION_POLICY_SCHEMA,
        0,
        |_| {},
    )
    .expect("coordinate-episode policy has an empty payload")
}

/// Durable local occupancy-episode ledger (LEDGER-1).
///
/// An authoritative local ledger maintaining the exact live episode per mutable occupied coordinate,
/// consuming burns with retained burn provenance, globally fresh episode identifiers (neither live
/// nor burned mint IDs may be reused), and excluding bedrock from the mutable occupancy-episode map.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OccupancyEpisodeLedger {
    history_genesis: ContentTag,
    history_head: ContentTag,
    active_by_coord: HashMap<Coord, CoordinateEpisode>,
    active_by_mint: HashMap<ContentTag, Coord>,
    closed_by_mint: HashMap<ContentTag, CoordinateEpisode>,
    seen_mints: HashSet<ContentTag>,
    seen_burns: HashSet<ContentTag>,
    total_mints: usize,
    total_burns: usize,
}

impl OccupancyEpisodeLedger {
    /// Constructs an empty occupancy ledger for a given history genesis.
    pub fn new(history_genesis: ContentTag) -> Self {
        Self {
            history_genesis,
            history_head: history_genesis,
            active_by_coord: HashMap::new(),
            active_by_mint: HashMap::new(),
            closed_by_mint: HashMap::new(),
            seen_mints: HashSet::new(),
            seen_burns: HashSet::new(),
            total_mints: 0,
            total_burns: 0,
        }
    }

    /// Reconstructs the ledger state by semantically replaying a verified History.
    pub fn from_history(
        initial_world: &World,
        history: &History,
    ) -> Result<Self, CoordinateEpisodeError> {
        Self::from_history_with_limits(initial_world, history, HistoryReplayLimits::DEFAULT)
    }

    /// Reconstructs the ledger state by semantically replaying a verified History under explicit limits.
    pub fn from_history_with_limits(
        initial_world: &World,
        history: &History,
        limits: HistoryReplayLimits,
    ) -> Result<Self, CoordinateEpisodeError> {
        let verified = verify_in_memory_history_with_limits(initial_world, history, limits)?;
        let verified_history = verified.history();
        let mut ledger = Self::new(verified_history.genesis());
        for receipt in verified_history.receipts() {
            ledger.apply_receipt(receipt)?;
        }
        if !ledger.verify_consistency(verified_history, initial_world)
            || !ledger.verify_world_bijection(initial_world, verified.final_world())
        {
            return Err(CoordinateEpisodeError::DerivationMismatch);
        }
        Ok(ledger)
    }

    /// Applies one verified history receipt atomically to advance the active episode map.
    pub fn apply_receipt(&mut self, receipt: &Receipt) -> Result<(), CoordinateEpisodeError> {
        if receipt.parent() != self.history_head {
            return Err(CoordinateEpisodeError::DerivationMismatch);
        }
        let mut staged = self.clone();
        for effect in receipt.effects() {
            staged.apply_effect(effect)?;
        }
        staged.history_head = receipt.tag();
        *self = staged;
        Ok(())
    }

    /// Applies one transition effect atomically without leaving partial state on error.
    pub fn apply_effect(
        &mut self,
        effect: &TransitionEffect,
    ) -> Result<(), CoordinateEpisodeError> {
        match *effect {
            TransitionEffect::Mint { coord, event } => {
                if is_bedrock(coord) {
                    return Err(CoordinateEpisodeError::BedrockCoordinateForbidden);
                }
                if self.seen_mints.contains(&event) {
                    return Err(CoordinateEpisodeError::DuplicateMintEvent { event });
                }
                if let Some(existing) = self.active_by_coord.get(&coord) {
                    return Err(CoordinateEpisodeError::ActiveCoordinateAlreadyOccupied {
                        coord,
                        existing_mint: existing.mint,
                    });
                }
                let episode = CoordinateEpisode {
                    coord,
                    mint: event,
                    consuming_burn: None,
                };
                self.active_by_coord.insert(coord, episode);
                self.active_by_mint.insert(event, coord);
                self.seen_mints.insert(event);
                self.total_mints += 1;
            }
            TransitionEffect::Burn {
                coord,
                event,
                consumed_mint,
            } => {
                if is_bedrock(coord) {
                    return Err(CoordinateEpisodeError::BedrockCoordinateForbidden);
                }
                // Mutable coordinates already on at genesis close no post-genesis episode.
                if consumed_mint == self.history_genesis {
                    return Ok(());
                }
                if self.seen_burns.contains(&event) {
                    return Err(CoordinateEpisodeError::DuplicateBurnEvent { event });
                }
                if self.closed_by_mint.contains_key(&consumed_mint)
                    || (self.seen_mints.contains(&consumed_mint)
                        && !self.active_by_mint.contains_key(&consumed_mint))
                {
                    return Err(CoordinateEpisodeError::BurnedEpisodeWasAlreadyClosed {
                        mint: consumed_mint,
                    });
                }
                let Some(&mint_coord) = self.active_by_mint.get(&consumed_mint) else {
                    return Err(CoordinateEpisodeError::BurnWithoutOpenMint {
                        burn: event,
                        consumed_mint,
                    });
                };
                if mint_coord != coord {
                    return Err(CoordinateEpisodeError::BurnProvenanceMismatch {
                        burn: event,
                        consumed_mint,
                        expected_coord: mint_coord,
                        actual_coord: coord,
                    });
                }
                let Some(episode) = self.active_by_coord.get(&coord) else {
                    return Err(CoordinateEpisodeError::BurnWithoutOpenMint {
                        burn: event,
                        consumed_mint,
                    });
                };
                if episode.mint != consumed_mint {
                    return Err(CoordinateEpisodeError::BurnProvenanceMismatch {
                        burn: event,
                        consumed_mint,
                        expected_coord: episode.coord,
                        actual_coord: coord,
                    });
                }
                self.active_by_mint.remove(&consumed_mint);
                self.active_by_coord.remove(&coord);
                self.closed_by_mint.insert(
                    consumed_mint,
                    CoordinateEpisode {
                        coord,
                        mint: consumed_mint,
                        consuming_burn: Some(event),
                    },
                );
                self.seen_burns.insert(event);
                self.total_burns += 1;
            }
            TransitionEffect::NoOp { .. } => {}
        }
        Ok(())
    }

    /// Returns the currently active `CoordinateEpisode` at `coord`, if any.
    pub fn get_active(&self, coord: Coord) -> Option<&CoordinateEpisode> {
        self.active_by_coord.get(&coord)
    }

    /// Returns the closed (burned) `CoordinateEpisode` and its burn provenance for `mint`, if retained.
    pub fn closed_episode(&self, mint: ContentTag) -> Option<&CoordinateEpisode> {
        self.closed_by_mint.get(&mint)
    }

    /// Returns the number of retained closed (burned) episodes with recorded burn provenance.
    pub fn closed_count(&self) -> usize {
        self.closed_by_mint.len()
    }

    /// Returns `true` if `mint` identifies an open episode in the ledger.
    pub fn is_active_mint(&self, mint: ContentTag) -> bool {
        self.active_by_mint.contains_key(&mint)
    }

    /// Returns `true` if `mint` has ever been minted (live or previously burned) in this ledger.
    pub fn has_seen_mint(&self, mint: ContentTag) -> bool {
        self.seen_mints.contains(&mint)
    }

    /// Returns the map of all currently open episodes indexed by `Coord`.
    pub fn active_episodes(&self) -> &HashMap<Coord, CoordinateEpisode> {
        &self.active_by_coord
    }

    /// Returns the map of all retained closed (burned) episodes indexed by mint `ContentTag`.
    pub fn closed_episodes(&self) -> &HashMap<ContentTag, CoordinateEpisode> {
        &self.closed_by_mint
    }

    /// Returns the number of currently active mutable coordinate episodes.
    pub fn active_count(&self) -> usize {
        self.active_by_coord.len()
    }

    /// Returns the cumulative count of post-genesis mint events applied to the ledger.
    pub fn total_mints(&self) -> usize {
        self.total_mints
    }

    /// Returns the cumulative count of post-genesis burn events applied to the ledger.
    pub fn total_burns(&self) -> usize {
        self.total_burns
    }

    /// Returns the genesis `ContentTag` bound to this ledger.
    pub fn history_genesis(&self) -> ContentTag {
        self.history_genesis
    }

    /// Returns the latest receipt `ContentTag` applied to this ledger.
    pub fn history_head(&self) -> ContentTag {
        self.history_head
    }

    /// Bounded local pruning of closed-episode metadata while keeping active episodes, pins, and ID freshness.
    pub fn prune_closed_provenance(
        &mut self,
        max_prune: usize,
        pinned_mints: &HashSet<ContentTag>,
    ) -> usize {
        if max_prune == 0 || self.closed_by_mint.is_empty() {
            return 0;
        }
        let mut removable: Vec<ContentTag> = self
            .closed_by_mint
            .keys()
            .copied()
            .filter(|mint| !pinned_mints.contains(mint))
            .collect();
        removable.sort_by_key(|tag| tag.digest());
        let to_remove = removable.len().min(max_prune);
        for mint in removable.into_iter().take(to_remove) {
            self.closed_by_mint.remove(&mint);
        }
        to_remove
    }

    /// Verifies exact bijection between post-genesis mutable occupied coordinates and active episodes.
    pub fn verify_world_bijection(&self, initial_world: &World, current_world: &World) -> bool {
        if self.active_by_coord.len() != self.active_by_mint.len() {
            return false;
        }
        for (&coord, episode) in &self.active_by_coord {
            if is_bedrock(coord)
                || !current_world.is_on(coord)
                || !episode.is_active()
                || self.active_by_mint.get(&episode.mint) != Some(&coord)
                || !self.seen_mints.contains(&episode.mint)
                || self.closed_by_mint.contains_key(&episode.mint)
            {
                return false;
            }
        }
        for (&mint, episode) in &self.closed_by_mint {
            if episode.mint != mint
                || episode.consuming_burn.is_none()
                || self.active_by_mint.contains_key(&mint)
                || !self.seen_mints.contains(&mint)
            {
                return false;
            }
        }
        for coord in current_world.iter_on() {
            if is_bedrock(coord) {
                continue;
            }
            let in_ledger = self.active_by_coord.contains_key(&coord);
            let was_genesis_on = initial_world.is_on(coord);
            if !in_ledger && !was_genesis_on {
                return false;
            }
        }
        true
    }

    /// Checks that the active episodes match the verified history's genesis, head, supply, and active mints.
    pub fn verify_consistency(&self, history: &History, initial_world: &World) -> bool {
        if self.history_genesis != history.genesis()
            || self.history_head != history.head()
            || self.active_by_coord.len() != self.active_by_mint.len()
        {
            return false;
        }
        let Ok(verified) = verify_in_memory_history_with_limits(
            initial_world,
            history,
            HistoryReplayLimits::DEFAULT,
        ) else {
            return false;
        };
        let verified_history = verified.history();
        if self.active_count() != verified_history.circulating_supply() {
            return false;
        }
        for (&coord, episode) in &self.active_by_coord {
            if self.active_by_mint.get(&episode.mint) != Some(&coord) {
                return false;
            }
            if verified_history.active_mint(coord) != Some(episode.mint) {
                return false;
            }
        }
        self.verify_world_bijection(initial_world, verified.final_world())
    }
}

/// Crash-safe durable local occupancy-episode ledger backed by `HistoryJournal` and `LocalStorageProfile`.
pub struct DurableOccupancyLedger {
    journal: HistoryJournal,
    ledger: OccupancyEpisodeLedger,
    initial_world: World,
}

/// Non-writable recovery handle for a durable occupancy ledger whose carrier has an unverified suffix.
pub struct DurableLedgerRecovery {
    recovery: HistoryRecoveryJournal,
    ledger: OccupancyEpisodeLedger,
    initial_world: World,
}

/// Outcome of opening an existing durable occupancy-episode ledger.
pub enum OpenDurableOccupancyLedger {
    Ready {
        ledger: Box<DurableOccupancyLedger>,
        carrier_report: ScanReport,
    },
    NeedsRecovery {
        recovery: Box<DurableLedgerRecovery>,
    },
}

/// Error returned by crash-safe durable occupancy-episode ledger operations.
#[derive(Debug)]
pub enum DurableLedgerError {
    Store(HistoryStoreError),
    Episode(CoordinateEpisodeError),
}

impl fmt::Display for DurableLedgerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store(err) => write!(f, "durable ledger storage error: {err}"),
            Self::Episode(err) => write!(f, "durable ledger episode error: {err}"),
        }
    }
}

impl std::error::Error for DurableLedgerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Store(err) => Some(err),
            Self::Episode(err) => Some(err),
        }
    }
}

impl From<HistoryStoreError> for DurableLedgerError {
    fn from(err: HistoryStoreError) -> Self {
        Self::Store(err)
    }
}

impl From<CoordinateEpisodeError> for DurableLedgerError {
    fn from(err: CoordinateEpisodeError) -> Self {
        Self::Episode(err)
    }
}

impl DurableOccupancyLedger {
    /// Creates a new crash-safe durable occupancy-episode ledger bounded by `profile`.
    pub fn create_new<P: AsRef<Path>>(
        path: P,
        profile: &LocalStorageProfile,
        initial_world: &World,
    ) -> Result<(Self, HistoryCreateReceipt), DurableLedgerError> {
        let (journal, receipt) =
            HistoryJournal::create_new_with_profile(path, profile, initial_world)?;
        let ledger = OccupancyEpisodeLedger::from_history_with_limits(
            initial_world,
            journal.history(),
            journal.replay_limits(),
        )?;
        Ok((
            Self {
                journal,
                ledger,
                initial_world: initial_world.clone(),
            },
            receipt,
        ))
    }

    /// Opens an existing durable occupancy-episode ledger and replays its verified committed prefix.
    pub fn open_existing<P: AsRef<Path>>(
        path: P,
        profile: &LocalStorageProfile,
        anchor: AnchorPolicy,
        initial_world: &World,
    ) -> Result<OpenDurableOccupancyLedger, DurableLedgerError> {
        match HistoryJournal::open_existing_with_profile(path, profile, anchor, initial_world)? {
            OpenHistoryJournal::Ready {
                journal,
                carrier_report,
            } => {
                let ledger = OccupancyEpisodeLedger::from_history_with_limits(
                    initial_world,
                    journal.history(),
                    journal.replay_limits(),
                )?;
                Ok(OpenDurableOccupancyLedger::Ready {
                    ledger: Box::new(Self {
                        journal: *journal,
                        ledger,
                        initial_world: initial_world.clone(),
                    }),
                    carrier_report,
                })
            }
            OpenHistoryJournal::NeedsRecovery { journal } => {
                let ledger =
                    OccupancyEpisodeLedger::from_history(initial_world, journal.history())?;
                Ok(OpenDurableOccupancyLedger::NeedsRecovery {
                    recovery: Box::new(DurableLedgerRecovery {
                        recovery: *journal,
                        ledger,
                        initial_world: initial_world.clone(),
                    }),
                })
            }
        }
    }

    /// Commits one tick durably to the carrier journal, then advances the live episode map atomically.
    pub fn append_tick(
        &mut self,
        tick: u64,
        edits: &[Edit],
    ) -> Result<Option<HistoryAppendReceipt>, DurableLedgerError> {
        let Some(receipt) = self.journal.append_tick(tick, edits)? else {
            return Ok(None);
        };
        let committed = self
            .journal
            .history()
            .receipts()
            .last()
            .expect("committed tick appends one history receipt");
        self.ledger.apply_receipt(committed)?;
        if !self
            .ledger
            .verify_world_bijection(&self.initial_world, self.journal.world())
        {
            return Err(DurableLedgerError::Episode(
                CoordinateEpisodeError::DerivationMismatch,
            ));
        }
        Ok(Some(receipt))
    }

    /// Writes a verified backup of the underlying journal bounded by `profile`.
    pub fn backup_to<P: AsRef<Path>>(
        &mut self,
        dest: P,
        profile: &LocalStorageProfile,
    ) -> Result<BackupReceipt, DurableLedgerError> {
        self.journal
            .backup_to_with_profile(dest, profile)
            .map_err(Into::into)
    }

    /// Verifies and restores a backup into a new durable occupancy-episode ledger.
    pub fn restore_from<P: AsRef<Path>, Q: AsRef<Path>>(
        backup_path: P,
        dest_path: Q,
        profile: &LocalStorageProfile,
        anchor: AnchorPolicy,
        initial_world: &World,
    ) -> Result<(Self, RestoreReceipt), DurableLedgerError> {
        let (journal, receipt) = HistoryJournal::restore_from_with_profile(
            backup_path,
            dest_path,
            profile,
            anchor,
            initial_world,
        )?;
        let ledger = OccupancyEpisodeLedger::from_history_with_limits(
            initial_world,
            journal.history(),
            journal.replay_limits(),
        )?;
        Ok((
            Self {
                journal,
                ledger,
                initial_world: initial_world.clone(),
            },
            receipt,
        ))
    }

    /// Returns the committed `World`.
    pub const fn world(&self) -> &World {
        self.journal.world()
    }

    /// Returns the committed coordinate `History`.
    pub const fn history(&self) -> &History {
        self.journal.history()
    }

    /// Returns the live `OccupancyEpisodeLedger`.
    pub const fn episodes(&self) -> &OccupancyEpisodeLedger {
        &self.ledger
    }

    /// Returns a mutable reference to prune retained closed-episode metadata while preserving freshness.
    pub fn prune_closed_provenance(
        &mut self,
        max_prune: usize,
        pinned_mints: &HashSet<ContentTag>,
    ) -> usize {
        self.ledger.prune_closed_provenance(max_prune, pinned_mints)
    }

    /// Returns the current carrier storage-record head tag.
    pub fn carrier_head(&self) -> Option<ContentTag> {
        self.journal.carrier_head()
    }

    /// Derives canonical `CoordinateEpisodeEvidence` from the committed history.
    pub fn export_evidence(&self) -> Result<CoordinateEpisodeEvidence, CoordinateEpisodeError> {
        CoordinateEpisodeEvidence::derive_with_limits(
            &self.initial_world,
            self.journal.history(),
            self.journal.replay_limits(),
        )
    }
}

impl DurableLedgerRecovery {
    /// Returns the physical recovery report for the unverified carrier suffix.
    pub const fn report(&self) -> &RecoveryReport {
        self.recovery.report()
    }

    /// Returns the `OccupancyEpisodeLedger` reconstructed from the verified committed prefix.
    pub const fn episodes(&self) -> &OccupancyEpisodeLedger {
        &self.ledger
    }

    /// Explicitly discards the unverified suffix and returns a writable `DurableOccupancyLedger`.
    pub fn discard_unverified_tail(
        self,
    ) -> Result<(DurableOccupancyLedger, TailDiscardReceipt), DurableLedgerError> {
        let (journal, receipt) = self.recovery.discard_unverified_tail()?;
        let ledger = OccupancyEpisodeLedger::from_history_with_limits(
            &self.initial_world,
            journal.history(),
            journal.replay_limits(),
        )?;
        Ok((
            DurableOccupancyLedger {
                journal,
                ledger,
                initial_world: self.initial_world,
            },
            receipt,
        ))
    }
}

/// Builds only the relation that is actually serialized. The caller's replay policy bounds source
/// work separately; this bound applies to emitted episodes, so arbitrarily many valid no-ops do
/// not consume episode capacity or force an equivalent allocation.
fn derive_episodes(
    history: &History,
    maximum: usize,
) -> Result<Vec<CoordinateEpisode>, CoordinateEpisodeError> {
    let mut episodes = Vec::new();
    let mut open_by_mint = HashMap::new();

    for receipt in history.receipts() {
        for effect in receipt.effects() {
            match *effect {
                TransitionEffect::Mint { coord, event } => {
                    if open_by_mint.contains_key(&event) {
                        return Err(CoordinateEpisodeError::DuplicateMintEvent { event });
                    }
                    let requested = episodes
                        .len()
                        .checked_add(1)
                        .ok_or(CoordinateEpisodeError::LengthOverflow)?;
                    if requested > maximum {
                        return Err(CoordinateEpisodeError::TooManyEpisodes {
                            actual: requested,
                            maximum,
                        });
                    }
                    episodes
                        .try_reserve(1)
                        .map_err(|_| CoordinateEpisodeError::AllocationFailed { requested })?;
                    open_by_mint
                        .try_reserve(1)
                        .map_err(|_| CoordinateEpisodeError::AllocationFailed { requested })?;
                    let index = episodes.len();
                    episodes.push(CoordinateEpisode {
                        coord,
                        mint: event,
                        consuming_burn: None,
                    });
                    open_by_mint.insert(event, index);
                }
                TransitionEffect::Burn {
                    coord,
                    event,
                    consumed_mint,
                } => {
                    // Mutable coordinates already on at genesis are deliberately not issued by
                    // this post-genesis relation. Their separate provenance is the genesis tag,
                    // so a burn of one closes no coordinate-episode here.
                    if consumed_mint == history.genesis() {
                        continue;
                    }
                    let Some(index) = open_by_mint.remove(&consumed_mint) else {
                        return Err(CoordinateEpisodeError::BurnWithoutOpenMint {
                            burn: event,
                            consumed_mint,
                        });
                    };
                    let episode = episodes.get_mut(index).expect(
                        "the open-mint map contains only indices of locally pushed episodes",
                    );
                    if episode.coord != coord || episode.mint != consumed_mint {
                        return Err(CoordinateEpisodeError::BurnProvenanceMismatch {
                            burn: event,
                            consumed_mint,
                            expected_coord: episode.coord,
                            actual_coord: coord,
                        });
                    }
                    if episode.consuming_burn.replace(event).is_some() {
                        return Err(CoordinateEpisodeError::BurnedEpisodeWasAlreadyClosed {
                            mint: consumed_mint,
                        });
                    }
                }
                TransitionEffect::NoOp { .. } => {}
            }
        }
    }

    for episode in &episodes {
        if episode.is_active() && history.active_mint(episode.coord) != Some(episode.mint) {
            return Err(CoordinateEpisodeError::ActiveProvenanceMismatch {
                coord: episode.coord,
                mint: episode.mint,
            });
        }
    }
    Ok(episodes)
}

fn encoded_len(episodes: &[CoordinateEpisode]) -> Result<usize, CoordinateEpisodeError> {
    if episodes.len() > MAX_COORDINATE_EPISODES {
        return Err(CoordinateEpisodeError::TooManyEpisodes {
            actual: episodes.len(),
            maximum: MAX_COORDINATE_EPISODES,
        });
    }
    episodes.iter().try_fold(HEADER_BYTES, |length, episode| {
        length
            .checked_add(episode.encoded_len())
            .ok_or(CoordinateEpisodeError::LengthOverflow)
    })
}

fn encode_fields(
    policy: ContentTag,
    history_genesis: ContentTag,
    history_head: ContentTag,
    final_world: ContentTag,
    episodes: &[CoordinateEpisode],
    sink: &mut dyn FnMut(&[u8]),
) {
    sink(&policy.digest());
    sink(&history_genesis.digest());
    sink(&history_head.digest());
    sink(&final_world.digest());
    sink(&(episodes.len() as u64).to_be_bytes());
    for episode in episodes {
        episode.encode(sink);
    }
}

fn validate_fields(
    policy: ContentTag,
    history_genesis: ContentTag,
    history_head: ContentTag,
    final_world: ContentTag,
    episodes: &[CoordinateEpisode],
) -> Result<(), CoordinateEpisodeError> {
    require_kind("derivation policy", policy, TagKind::Policy)?;
    if policy != coordinate_episode_derivation_policy_tag() {
        return Err(CoordinateEpisodeError::PolicyTagMismatch { actual: policy });
    }
    require_kind("history genesis", history_genesis, TagKind::Receipt)?;
    require_kind("history head", history_head, TagKind::Receipt)?;
    require_kind("final world", final_world, TagKind::World)?;
    if episodes.len() > MAX_COORDINATE_EPISODES {
        return Err(CoordinateEpisodeError::TooManyEpisodes {
            actual: episodes.len(),
            maximum: MAX_COORDINATE_EPISODES,
        });
    }

    let max_events = episodes
        .len()
        .checked_mul(2)
        .ok_or(CoordinateEpisodeError::LengthOverflow)?;
    let mut seen_events = HashSet::new();
    seen_events
        .try_reserve(max_events)
        .map_err(|_| CoordinateEpisodeError::AllocationFailed {
            requested: max_events,
        })?;
    for (index, episode) in episodes.iter().enumerate() {
        require_kind("mint event", episode.mint, TagKind::Event)?;
        if !seen_events.insert(episode.mint) {
            return Err(CoordinateEpisodeError::DuplicateEpisodeEvent {
                index,
                event: episode.mint,
            });
        }
        if let Some(burn) = episode.consuming_burn {
            require_kind("consuming burn event", burn, TagKind::Event)?;
            if !seen_events.insert(burn) {
                return Err(CoordinateEpisodeError::DuplicateEpisodeEvent { index, event: burn });
            }
        }
    }
    Ok(())
}

fn require_kind(
    field: &'static str,
    tag: ContentTag,
    expected: TagKind,
) -> Result<(), CoordinateEpisodeError> {
    if tag.kind() != expected {
        return Err(CoordinateEpisodeError::WrongFieldKind {
            field,
            expected,
            actual: tag.kind(),
        });
    }
    Ok(())
}

struct Reader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    fn require(&self, field: &'static str, needed: usize) -> Result<(), CoordinateEpisodeError> {
        let remaining = self.bytes.len().saturating_sub(self.cursor);
        if needed > remaining {
            return Err(CoordinateEpisodeError::UnexpectedEnd {
                field,
                needed,
                remaining,
            });
        }
        Ok(())
    }

    fn take(
        &mut self,
        field: &'static str,
        length: usize,
    ) -> Result<&'a [u8], CoordinateEpisodeError> {
        self.require(field, length)?;
        let end = self
            .cursor
            .checked_add(length)
            .ok_or(CoordinateEpisodeError::LengthOverflow)?;
        let value = &self.bytes[self.cursor..end];
        self.cursor = end;
        Ok(value)
    }

    fn u8(&mut self, field: &'static str) -> Result<u8, CoordinateEpisodeError> {
        Ok(self.take(field, 1)?[0])
    }

    fn i64(&mut self, field: &'static str) -> Result<i64, CoordinateEpisodeError> {
        Ok(i64::from_be_bytes(
            self.take(field, 8)?.try_into().expect("fixed i64 width"),
        ))
    }

    fn count(&mut self) -> Result<usize, CoordinateEpisodeError> {
        let value = u64::from_be_bytes(
            self.take("episode count", 8)?
                .try_into()
                .expect("fixed count width"),
        );
        usize::try_from(value).map_err(|_| CoordinateEpisodeError::CountOverflow { value })
    }

    fn tag(
        &mut self,
        kind: TagKind,
        field: &'static str,
    ) -> Result<ContentTag, CoordinateEpisodeError> {
        Ok(ContentTag::from_digest(
            kind,
            self.take(field, 32)?
                .try_into()
                .expect("fixed tag digest width"),
        ))
    }

    fn finish(self) -> Result<(), CoordinateEpisodeError> {
        if self.cursor != self.bytes.len() {
            return Err(CoordinateEpisodeError::TrailingBytes {
                actual: self.bytes.len() - self.cursor,
            });
        }
        Ok(())
    }
}

/// Error returned when deriving, encoding, decoding, or verifying coordinate episodes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoordinateEpisodeError {
    History(InMemoryHistoryVerificationError),
    VerifiedWorldTagMismatch {
        replayed: ContentTag,
        history: ContentTag,
    },
    TooManyEpisodes {
        actual: usize,
        maximum: usize,
    },
    AllocationFailed {
        requested: usize,
    },
    LengthOverflow,
    CountOverflow {
        value: u64,
    },
    UnexpectedEnd {
        field: &'static str,
        needed: usize,
        remaining: usize,
    },
    TrailingBytes {
        actual: usize,
    },
    UnknownEpisodeState {
        index: usize,
        marker: u8,
    },
    WrongFieldKind {
        field: &'static str,
        expected: TagKind,
        actual: TagKind,
    },
    PolicyTagMismatch {
        actual: ContentTag,
    },
    WrongEvidenceTagKind {
        actual: TagKind,
    },
    DuplicateEpisodeEvent {
        index: usize,
        event: ContentTag,
    },
    DuplicateMintEvent {
        event: ContentTag,
    },
    DuplicateBurnEvent {
        event: ContentTag,
    },
    ActiveCoordinateAlreadyOccupied {
        coord: Coord,
        existing_mint: ContentTag,
    },
    BedrockCoordinateForbidden,
    BurnWithoutOpenMint {
        burn: ContentTag,
        consumed_mint: ContentTag,
    },
    BurnProvenanceMismatch {
        burn: ContentTag,
        consumed_mint: ContentTag,
        expected_coord: Coord,
        actual_coord: Coord,
    },
    BurnedEpisodeWasAlreadyClosed {
        mint: ContentTag,
    },
    ActiveProvenanceMismatch {
        coord: Coord,
        mint: ContentTag,
    },
    TagMismatch,
    NonCanonicalEncoding,
    DerivationMismatch,
    Hash(TagError),
}

impl fmt::Display for CoordinateEpisodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::History(source) => write!(formatter, "history verification refused: {source}"),
            Self::VerifiedWorldTagMismatch { replayed, history } => write!(
                formatter,
                "verified final-world tag {replayed} disagrees with history tag {history}"
            ),
            Self::TooManyEpisodes { actual, maximum } => write!(
                formatter,
                "coordinate-episode count {actual} exceeds policy maximum {maximum}"
            ),
            Self::AllocationFailed { requested } => {
                write!(
                    formatter,
                    "could not allocate {requested} coordinate episodes"
                )
            }
            Self::LengthOverflow => {
                formatter.write_str("coordinate-episode payload length overflow")
            }
            Self::CountOverflow { value } => write!(
                formatter,
                "coordinate-episode count {value} is not representable on this host"
            ),
            Self::UnexpectedEnd {
                field,
                needed,
                remaining,
            } => write!(
                formatter,
                "coordinate-episode payload ended in {field}: needs {needed} bytes, has {remaining}"
            ),
            Self::TrailingBytes { actual } => {
                write!(
                    formatter,
                    "coordinate-episode payload has {actual} trailing bytes"
                )
            }
            Self::UnknownEpisodeState { index, marker } => write!(
                formatter,
                "coordinate episode at traversal position {index} has unknown state marker {marker}"
            ),
            Self::WrongFieldKind {
                field,
                expected,
                actual,
            } => write!(
                formatter,
                "coordinate-episode {field} must be {}, received {}",
                expected.name(),
                actual.name()
            ),
            Self::PolicyTagMismatch { actual } => write!(
                formatter,
                "coordinate-episode derivation policy {actual} is not the v1 policy"
            ),
            Self::WrongEvidenceTagKind { actual } => write!(
                formatter,
                "coordinate-episode evidence tag must be receipt, received {}",
                actual.name()
            ),
            Self::DuplicateEpisodeEvent { index, event } => write!(
                formatter,
                "coordinate episode at traversal position {index} reuses event {event}"
            ),
            Self::DuplicateMintEvent { event } => {
                write!(formatter, "verified history repeated mint event {event}")
            }
            Self::DuplicateBurnEvent { event } => {
                write!(formatter, "verified history repeated burn event {event}")
            }
            Self::ActiveCoordinateAlreadyOccupied {
                coord,
                existing_mint,
            } => write!(
                formatter,
                "coordinate ({}, {}, {}) already holds active episode {existing_mint}",
                coord.x, coord.y, coord.z
            ),
            Self::BedrockCoordinateForbidden => formatter.write_str(
                "bedrock coordinate (0, 0, 0) cannot enter the mutable occupancy-episode map",
            ),
            Self::BurnWithoutOpenMint {
                burn,
                consumed_mint,
            } => write!(
                formatter,
                "burn event {burn} consumes unopened mint event {consumed_mint}"
            ),
            Self::BurnProvenanceMismatch {
                burn,
                consumed_mint,
                expected_coord,
                actual_coord,
            } => write!(
                formatter,
                "burn event {burn} consumes {consumed_mint} at ({}, {}, {}) rather than ({}, {}, {})",
                actual_coord.x,
                actual_coord.y,
                actual_coord.z,
                expected_coord.x,
                expected_coord.y,
                expected_coord.z
            ),
            Self::BurnedEpisodeWasAlreadyClosed { mint } => {
                write!(formatter, "mint event {mint} was consumed more than once")
            }
            Self::ActiveProvenanceMismatch { coord, mint } => write!(
                formatter,
                "active mint {mint} is absent at ({}, {}, {})",
                coord.x, coord.y, coord.z
            ),
            Self::TagMismatch => formatter.write_str("coordinate-episode evidence tag mismatch"),
            Self::NonCanonicalEncoding => {
                formatter.write_str("coordinate-episode payload is not canonical")
            }
            Self::DerivationMismatch => formatter.write_str(
                "coordinate-episode evidence does not equal the verified history derivation",
            ),
            Self::Hash(source) => write!(formatter, "coordinate-episode hash failed: {source}"),
        }
    }
}

impl std::error::Error for CoordinateEpisodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::History(source) => Some(source),
            Self::Hash(source) => Some(source),
            _ => None,
        }
    }
}

impl From<InMemoryHistoryVerificationError> for CoordinateEpisodeError {
    fn from(source: InMemoryHistoryVerificationError) -> Self {
        Self::History(source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::program::{Edit, Operation};
    use crate::world::BEDROCK_COORD;

    const A: Coord = Coord::new(1, -2, 3);
    const B: Coord = Coord::new(-7, 4, 9);
    const GENESIS_MUTABLE: Coord = Coord::new(11, 12, 13);

    fn apply_tick(history: &mut History, world: &mut World, tick: u64, edits: &[Edit]) {
        let mut transaction = history.begin_tick(tick);
        for &edit in edits {
            transaction.apply_edit(history, world, edit);
        }
        history
            .commit_tick(transaction, world)
            .expect("nonempty test tick must commit");
    }

    fn episode_history() -> (World, History) {
        let initial = World::new_with_origin();
        let mut world = initial.clone();
        let mut history = History::new(&world);
        apply_tick(
            &mut history,
            &mut world,
            1,
            &[
                Edit::new(Operation::On, A),
                Edit::new(Operation::Off, A),
                Edit::new(Operation::On, A),
                Edit::new(Operation::Off, A),
                Edit::new(Operation::On, B),
            ],
        );
        (initial, history)
    }

    #[test]
    fn episodes_are_tag_identified_and_preserve_mint_burn_order() {
        let (initial, history) = episode_history();
        let evidence = CoordinateEpisodeEvidence::derive(&initial, &history).unwrap();

        assert_eq!(
            evidence.policy(),
            coordinate_episode_derivation_policy_tag()
        );
        assert_eq!(evidence.history_genesis(), history.genesis());
        assert_eq!(evidence.history_head(), history.head());
        assert_eq!(evidence.final_world(), history.current_world());
        assert_eq!(evidence.episodes().len(), 3);
        assert_eq!(evidence.episodes()[0].coord(), A);
        assert_eq!(evidence.episodes()[1].coord(), A);
        assert_eq!(evidence.episodes()[2].coord(), B);
        assert_ne!(evidence.episodes()[0].mint(), evidence.episodes()[1].mint());
        assert!(evidence.episodes()[0].consuming_burn().is_some());
        assert!(evidence.episodes()[1].consuming_burn().is_some());
        assert!(evidence.episodes()[2].is_active());
        assert_eq!(history.active_mint(B), Some(evidence.episodes()[2].mint()));
        assert_eq!(
            evidence.policy().to_string(),
            "#bb.policy.sha256.qjswilslbl4opnwl7npgkycn3lapzdlovmqphgj3cvlwugkr7rbq"
        );
        assert_eq!(
            evidence.tag().to_string(),
            "#bb.receipt.sha256.os4co4gqd6jed4ttwmbkjozviqpb6ly2xtczuuizobzcc7fcsplq"
        );
        assert_eq!(evidence.encoded_len(), Ok(371));
        evidence
            .verify_against(&initial, &history, HistoryReplayLimits::DEFAULT)
            .unwrap();
    }

    #[test]
    fn genesis_occupancy_is_not_rewritten_as_post_genesis_issuance() {
        let mut initial = World::new_with_origin();
        initial.set_on(GENESIS_MUTABLE);
        let mut world = initial.clone();
        let mut history = History::new(&world);
        apply_tick(
            &mut history,
            &mut world,
            1,
            &[
                Edit::new(Operation::Off, GENESIS_MUTABLE),
                Edit::new(Operation::On, GENESIS_MUTABLE),
            ],
        );

        let evidence = CoordinateEpisodeEvidence::derive(&initial, &history).unwrap();
        assert_eq!(evidence.episodes().len(), 1);
        assert_eq!(evidence.episodes()[0].coord(), GENESIS_MUTABLE);
        assert!(evidence.episodes()[0].is_active());
        assert_eq!(
            history.active_mint(GENESIS_MUTABLE),
            Some(evidence.episodes()[0].mint())
        );
    }

    #[test]
    fn noops_do_not_create_episodes_but_still_change_the_evidence_head() {
        let (initial, mut history) = episode_history();
        let before = CoordinateEpisodeEvidence::derive(&initial, &history).unwrap();
        let mut world = history.verify(&initial).unwrap();
        apply_tick(
            &mut history,
            &mut world,
            2,
            &[
                Edit::new(Operation::On, B),
                Edit::new(Operation::Off, Coord::new(20, 21, 22)),
                Edit::new(Operation::Off, BEDROCK_COORD),
            ],
        );
        let after = CoordinateEpisodeEvidence::derive(&initial, &history).unwrap();

        assert_eq!(before.episodes(), after.episodes());
        assert_ne!(before.history_head(), after.history_head());
        assert_ne!(before.tag(), after.tag());
    }

    #[test]
    fn codec_and_verified_derivation_reject_mutated_episode_fields() {
        let (initial, history) = episode_history();
        let evidence = CoordinateEpisodeEvidence::derive(&initial, &history).unwrap();
        let bytes = evidence.try_encode_canonical().unwrap();
        let decoded = CoordinateEpisodeEvidence::decode_canonical(&bytes).unwrap();
        assert_eq!(decoded, evidence);

        let mut wrong_policy = bytes.clone();
        wrong_policy[0] ^= 1;
        assert!(matches!(
            CoordinateEpisodeEvidence::decode_canonical(&wrong_policy),
            Err(CoordinateEpisodeError::PolicyTagMismatch { .. })
        ));

        let mut self_consistent_wrong_policy = evidence.clone();
        self_consistent_wrong_policy.policy = ContentTag::from_digest(TagKind::Policy, [0xa5; 32]);
        self_consistent_wrong_policy.tag = self_consistent_wrong_policy.recompute_tag().unwrap();
        assert!(matches!(
            self_consistent_wrong_policy.verify(),
            Err(CoordinateEpisodeError::PolicyTagMismatch { .. })
        ));

        let mut reordered = decoded.clone();
        reordered.episodes.swap(0, 1);
        reordered.tag = reordered.recompute_tag().unwrap();
        assert_eq!(
            reordered.verify_against(&initial, &history, HistoryReplayLimits::DEFAULT),
            Err(CoordinateEpisodeError::DerivationMismatch)
        );

        let mut wrong_burn = evidence.clone();
        wrong_burn.episodes[0].consuming_burn = None;
        wrong_burn.tag = wrong_burn.recompute_tag().unwrap();
        assert_eq!(
            wrong_burn.verify_against(&initial, &history, HistoryReplayLimits::DEFAULT),
            Err(CoordinateEpisodeError::DerivationMismatch)
        );

        let mut duplicate_event = bytes.clone();
        let mint_start = HEADER_BYTES + 24;
        let burn_start = mint_start + 32 + 1;
        let mint = duplicate_event[mint_start..mint_start + 32].to_vec();
        duplicate_event[burn_start..burn_start + 32].copy_from_slice(&mint);
        assert!(matches!(
            CoordinateEpisodeEvidence::decode_canonical(&duplicate_event),
            Err(CoordinateEpisodeError::DuplicateEpisodeEvent { index: 0, .. })
        ));

        let mut malformed = bytes;
        malformed[HEADER_BYTES + 24 + 32] = 9;
        assert!(matches!(
            CoordinateEpisodeEvidence::decode_canonical(&malformed),
            Err(CoordinateEpisodeError::UnknownEpisodeState { marker: 9, .. })
        ));
    }

    #[test]
    fn episode_limit_counts_emitted_mints_not_source_noops() {
        let initial = World::new_with_origin();
        let mut world = initial.clone();
        let mut history = History::new(&world);
        apply_tick(
            &mut history,
            &mut world,
            1,
            &[
                Edit::new(Operation::Off, Coord::new(20, 21, 22)),
                Edit::new(Operation::Off, Coord::new(23, 24, 25)),
            ],
        );
        assert!(derive_episodes(&history, 1).unwrap().is_empty());

        apply_tick(
            &mut history,
            &mut world,
            2,
            &[
                Edit::new(Operation::On, Coord::new(20, 21, 22)),
                Edit::new(Operation::On, Coord::new(23, 24, 25)),
            ],
        );
        assert_eq!(
            derive_episodes(&history, 1),
            Err(CoordinateEpisodeError::TooManyEpisodes {
                actual: 2,
                maximum: 1,
            })
        );
    }

    #[test]
    fn bounded_replay_is_required_before_an_episode_projection_is_returned() {
        let (initial, history) = episode_history();
        let exact = HistoryReplayLimits::new(1, 5, 5, 2, 2, 4);
        assert!(CoordinateEpisodeEvidence::derive_with_limits(&initial, &history, exact).is_ok());

        let one_less_edits = HistoryReplayLimits::new(1, 5, 4, 2, 2, 4);
        assert!(matches!(
            CoordinateEpisodeEvidence::derive_with_limits(&initial, &history, one_less_edits),
            Err(CoordinateEpisodeError::History(_))
        ));

        let mut wrong_initial = initial.clone();
        wrong_initial.set_on(Coord::new(99, 98, 97));
        assert!(matches!(
            CoordinateEpisodeEvidence::derive(&wrong_initial, &history),
            Err(CoordinateEpisodeError::History(_))
        ));
    }

    #[test]
    fn occupancy_ledger_tracks_mints_burns_and_re_mints() {
        let (initial, history) = episode_history();
        let evidence = CoordinateEpisodeEvidence::derive(&initial, &history).unwrap();
        let ledger = OccupancyEpisodeLedger::from_history(&initial, &history).unwrap();

        assert_eq!(ledger.total_mints(), 3);
        assert_eq!(ledger.total_burns(), 2);
        assert_eq!(ledger.active_count(), 1);
        assert_eq!(ledger.active_count(), history.circulating_supply());

        // Check active coordinate B has an active episode
        let b_episode = ledger.get_active(B).unwrap();
        assert_eq!(b_episode.coord(), B);
        assert!(b_episode.is_active());
        assert_eq!(b_episode.mint(), history.active_mint(B).unwrap());

        // A was burned and is currently off
        assert_eq!(ledger.get_active(A), None);
        assert!(!ledger.is_active_mint(evidence.episodes()[0].mint()));
        assert!(ledger.is_active_mint(b_episode.mint()));
        assert_eq!(ledger.active_episodes().len(), 1);
        assert!(ledger.verify_consistency(&history, &initial));
    }

    #[test]
    fn occupancy_ledger_burn_on_unminted_coordinate_fails() {
        let initial = World::new();
        let history = History::new(&initial);
        let mut ledger = OccupancyEpisodeLedger::new(history.genesis());

        let fake_burn = TransitionEffect::Burn {
            coord: Coord::new(10, 10, 10),
            event: ContentTag::from_digest(TagKind::Event, [1; 32]),
            consumed_mint: ContentTag::from_digest(TagKind::Event, [2; 32]),
        };
        assert!(matches!(
            ledger.apply_effect(&fake_burn),
            Err(CoordinateEpisodeError::BurnWithoutOpenMint { .. })
        ));
    }

    #[test]
    fn occupancy_ledger_preserves_state_on_burn_mismatch_and_rejects_wrong_parent() {
        let (initial, history) = episode_history();
        let mut ledger = OccupancyEpisodeLedger::from_history(&initial, &history).unwrap();
        let active_mint = ledger.get_active(B).unwrap().mint();

        let bad_burn = TransitionEffect::Burn {
            coord: A,
            event: ContentTag::from_digest(TagKind::Event, [9; 32]),
            consumed_mint: active_mint,
        };
        assert!(matches!(
            ledger.apply_effect(&bad_burn),
            Err(CoordinateEpisodeError::BurnProvenanceMismatch {
                expected_coord,
                actual_coord,
                ..
            }) if expected_coord == B && actual_coord == A
        ));
        // Both maps must remain completely intact after the refused burn.
        assert!(ledger.is_active_mint(active_mint));
        assert_eq!(ledger.get_active(B).unwrap().mint(), active_mint);
        assert!(ledger.verify_consistency(&history, &initial));

        // Re-applying an earlier receipt with the wrong parent fails closed.
        assert!(matches!(
            ledger.apply_receipt(&history.receipts()[0]),
            Err(CoordinateEpisodeError::DerivationMismatch)
        ));

        // Wrong initial_world fails semantic replay verification.
        let mut wrong_initial = World::new();
        wrong_initial.set_on(Coord::new(99, 99, 99));
        assert!(OccupancyEpisodeLedger::from_history(&wrong_initial, &history).is_err());
        assert!(!ledger.verify_consistency(&history, &wrong_initial));
    }

    #[test]
    fn ledger_enforces_fresh_ids_burn_provenance_bedrock_exclusion_and_pruning() {
        let (initial, history) = episode_history();
        let evidence = CoordinateEpisodeEvidence::derive(&initial, &history).unwrap();
        let mut ledger = OccupancyEpisodeLedger::from_history(&initial, &history).unwrap();

        let burned_mint_a = evidence.episodes()[0].mint();
        let burned_burn_a = evidence.episodes()[0].consuming_burn().unwrap();
        let burned_mint_b0 = evidence.episodes()[1].mint();
        let active_mint_b1 = evidence.episodes()[2].mint();

        // Burn provenance is retained for every consumed episode.
        assert_eq!(ledger.closed_count(), 2);
        assert_eq!(
            ledger
                .closed_episode(burned_mint_a)
                .unwrap()
                .consuming_burn(),
            Some(burned_burn_a)
        );
        assert!(ledger.has_seen_mint(burned_mint_a));
        assert!(ledger.has_seen_mint(active_mint_b1));

        // Reusing a burned episode ID fails closed (globally fresh IDs).
        let reuse_burned = TransitionEffect::Mint {
            coord: Coord::new(42, 42, 42),
            event: burned_mint_a,
        };
        assert!(matches!(
            ledger.apply_effect(&reuse_burned),
            Err(CoordinateEpisodeError::DuplicateMintEvent { event }) if event == burned_mint_a
        ));

        // Double-burning an already closed episode fails closed.
        let double_burn = TransitionEffect::Burn {
            coord: A,
            event: ContentTag::from_digest(TagKind::Event, [0x77; 32]),
            consumed_mint: burned_mint_a,
        };
        assert!(matches!(
            ledger.apply_effect(&double_burn),
            Err(CoordinateEpisodeError::BurnedEpisodeWasAlreadyClosed { mint }) if mint == burned_mint_a
        ));

        // Bedrock cannot be minted or burned in the mutable occupancy-episode map.
        let bedrock_mint = TransitionEffect::Mint {
            coord: BEDROCK_COORD,
            event: ContentTag::from_digest(TagKind::Event, [0x88; 32]),
        };
        assert_eq!(
            ledger.apply_effect(&bedrock_mint),
            Err(CoordinateEpisodeError::BedrockCoordinateForbidden)
        );

        // Pruning closed provenance respects pinned mints and NEVER clears ID freshness.
        let mut pinned = HashSet::new();
        pinned.insert(burned_mint_a);
        let pruned = ledger.prune_closed_provenance(4, &pinned);
        assert_eq!(pruned, 1);
        assert!(ledger.closed_episode(burned_mint_a).is_some());
        assert!(ledger.closed_episode(burned_mint_b0).is_none());
        assert!(ledger.has_seen_mint(burned_mint_b0));
        assert!(matches!(
            ledger.apply_effect(&TransitionEffect::Mint {
                coord: Coord::new(43, 43, 43),
                event: burned_mint_b0,
            }),
            Err(CoordinateEpisodeError::DuplicateMintEvent { event }) if event == burned_mint_b0
        ));
    }

    #[cfg(any(unix, windows))]
    mod fs_tests {
        use super::*;
        use std::path::PathBuf;
        use std::sync::atomic::{AtomicU64, Ordering};

        static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

        struct TempLedgerPath(PathBuf);

        impl TempLedgerPath {
            fn new(label: &str) -> Self {
                let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
                let path = std::env::temp_dir().join(format!(
                    "bitblock-ledger-{label}-{}-{id}.bbj",
                    std::process::id()
                ));
                let _ = std::fs::remove_file(&path);
                Self(path)
            }
        }

        impl Drop for TempLedgerPath {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }

        #[test]
        fn durable_ledger_persists_restarts_backups_restores_and_recovers_torn_tail() {
            let live_path = TempLedgerPath::new("live");
            let backup_path = TempLedgerPath::new("backup");
            let restored_path = TempLedgerPath::new("restored");
            let initial = World::new();
            let profile = LocalStorageProfile::DEFAULT;

            let (mut durable, _) =
                DurableOccupancyLedger::create_new(&live_path.0, &profile, &initial).unwrap();
            let tick1 = durable
                .append_tick(
                    1,
                    &[
                        Edit::new(Operation::On, A),
                        Edit::new(Operation::Off, BEDROCK_COORD),
                    ],
                )
                .unwrap()
                .unwrap();
            let mint_a1 = durable.episodes().get_active(A).unwrap().mint();
            let tick2 = durable
                .append_tick(
                    2,
                    &[Edit::new(Operation::Off, A), Edit::new(Operation::On, A)],
                )
                .unwrap()
                .unwrap();
            let mint_a2 = durable.episodes().get_active(A).unwrap().mint();
            assert_ne!(mint_a1, mint_a2);
            assert_eq!(durable.episodes().active_count(), 1);
            assert_eq!(durable.episodes().total_mints(), 2);
            assert_eq!(durable.episodes().total_burns(), 1);
            assert!(durable.episodes().closed_episode(mint_a1).is_some());
            assert_eq!(
                durable.export_evidence().unwrap().history_head(),
                tick2.receipt
            );
            let backup = durable.backup_to(&backup_path.0, &profile).unwrap();
            assert_eq!(backup.head, durable.carrier_head());
            let head_before_crash = durable.carrier_head();
            let history_head_before_crash = durable.history().head();
            drop(durable);

            // Simulate torn trailing write appended after committed tick 2 (valid declared length, truncated body).
            let mut bytes = std::fs::read(&live_path.0).unwrap();
            bytes.extend_from_slice(b"BBSTRREC\x00\x01\x00\x00\x00\x00\x00\x00\x00\x80torn");
            std::fs::write(&live_path.0, &bytes).unwrap();

            let OpenDurableOccupancyLedger::NeedsRecovery { recovery } =
                DurableOccupancyLedger::open_existing(
                    &live_path.0,
                    &profile,
                    AnchorPolicy::RequireAncestor(tick1.carrier_append.record_tag),
                    &initial,
                )
                .unwrap()
            else {
                panic!("torn tail was admitted as clean");
            };
            assert_eq!(recovery.report().valid_head, head_before_crash);
            assert_eq!(recovery.episodes().active_count(), 1);
            assert_eq!(recovery.episodes().get_active(A).unwrap().mint(), mint_a2);

            let (mut recovered, _discard) = recovery.discard_unverified_tail().unwrap();
            assert_eq!(recovered.history().head(), history_head_before_crash);
            assert_eq!(recovered.prune_closed_provenance(4, &HashSet::new()), 1);
            assert!(recovered.episodes().has_seen_mint(mint_a1));

            // Restore from backup and verify identical committed episode state.
            let (restored, restore_receipt) = DurableOccupancyLedger::restore_from(
                &backup_path.0,
                &restored_path.0,
                &profile,
                AnchorPolicy::RequireAncestor(tick1.carrier_append.record_tag),
                &initial,
            )
            .unwrap();
            assert_eq!(restore_receipt.head, head_before_crash);
            assert_eq!(restored.history().head(), history_head_before_crash);
            assert_eq!(restored.episodes().get_active(A).unwrap().mint(), mint_a2);
        }
    }
}
