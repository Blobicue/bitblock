//! Tag-only local issuance-provenance episodes derived from verified coordinate history.
//!
//! A mutable off-to-on transition is identified by its existing `#bb.event...` mint tag. A later
//! on-to-off transition may name that exact tag as its consumed mint. This is a local replay
//! projection: it has no balances, owners, transfers, signatures, custody, consensus, stake,
//! elapsed-time, cryptocurrency, or Layer-1 claim.

use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::history::{History, Receipt, TransitionEffect, world_tag};
use crate::history_store::{
    HistoryReplayLimits, InMemoryHistoryVerificationError, verify_in_memory_history_with_limits,
};
use crate::protocol::{ContentTag, TagError, TagKind, hash_canonical};
use crate::world::{Coord, World};

pub const COORDINATE_EPISODE_EVIDENCE_SCHEMA: &[u8] = b"bitblock/coordinate-episode-evidence/v1";
pub const COORDINATE_EPISODE_DERIVATION_POLICY_SCHEMA: &[u8] =
    b"bitblock/coordinate-episode-derivation-policy/v1";
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
    pub const fn coord(self) -> Coord {
        self.coord
    }

    pub const fn mint(self) -> ContentTag {
        self.mint
    }

    pub const fn consuming_burn(self) -> Option<ContentTag> {
        self.consuming_burn
    }

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

    pub const fn tag(&self) -> ContentTag {
        self.tag
    }

    pub const fn policy(&self) -> ContentTag {
        self.policy
    }

    pub const fn history_genesis(&self) -> ContentTag {
        self.history_genesis
    }

    pub const fn history_head(&self) -> ContentTag {
        self.history_head
    }

    pub const fn final_world(&self) -> ContentTag {
        self.final_world
    }

    pub fn episodes(&self) -> &[CoordinateEpisode] {
        &self.episodes
    }

    pub fn encoded_len(&self) -> Result<usize, CoordinateEpisodeError> {
        encoded_len(&self.episodes)
    }

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
/// consuming burns, recording fresh re-mints, and excluding bedrock coordinates (which originate at genesis).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OccupancyEpisodeLedger {
    history_genesis: ContentTag,
    history_head: ContentTag,
    active_by_coord: HashMap<Coord, CoordinateEpisode>,
    active_by_mint: HashMap<ContentTag, Coord>,
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
            total_mints: 0,
            total_burns: 0,
        }
    }

    /// Reconstructs the ledger state by replaying a verified History.
    pub fn from_history(
        initial_world: &World,
        history: &History,
    ) -> Result<Self, CoordinateEpisodeError> {
        let mut ledger = Self::new(history.genesis());
        for receipt in history.receipts() {
            ledger.apply_receipt(receipt)?;
        }
        if !ledger.verify_consistency(history, initial_world) {
            return Err(CoordinateEpisodeError::DerivationMismatch);
        }
        Ok(ledger)
    }

    /// Applies one verified history receipt to advance the active episode map.
    pub fn apply_receipt(&mut self, receipt: &Receipt) -> Result<(), CoordinateEpisodeError> {
        for effect in receipt.effects() {
            self.apply_effect(effect)?;
        }
        self.history_head = receipt.tag();
        Ok(())
    }

    /// Applies one transition effect.
    pub fn apply_effect(
        &mut self,
        effect: &TransitionEffect,
    ) -> Result<(), CoordinateEpisodeError> {
        match *effect {
            TransitionEffect::Mint { coord, event } => {
                if self.active_by_mint.contains_key(&event) {
                    return Err(CoordinateEpisodeError::DuplicateMintEvent { event });
                }
                if self.active_by_coord.contains_key(&coord) {
                    return Err(CoordinateEpisodeError::DuplicateMintEvent { event });
                }
                let episode = CoordinateEpisode {
                    coord,
                    mint: event,
                    consuming_burn: None,
                };
                self.active_by_coord.insert(coord, episode);
                self.active_by_mint.insert(event, coord);
                self.total_mints += 1;
            }
            TransitionEffect::Burn {
                coord,
                event,
                consumed_mint,
            } => {
                // Bedrock coordinates already on at genesis close no post-genesis episode
                if consumed_mint == self.history_genesis {
                    return Ok(());
                }
                let Some(mint_coord) = self.active_by_mint.remove(&consumed_mint) else {
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
                let Some(episode) = self.active_by_coord.remove(&coord) else {
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
                self.total_burns += 1;
            }
            TransitionEffect::NoOp { .. } => {}
        }
        Ok(())
    }

    pub fn get_active(&self, coord: Coord) -> Option<&CoordinateEpisode> {
        self.active_by_coord.get(&coord)
    }

    pub fn is_active_mint(&self, mint: ContentTag) -> bool {
        self.active_by_mint.contains_key(&mint)
    }

    pub fn active_episodes(&self) -> &HashMap<Coord, CoordinateEpisode> {
        &self.active_by_coord
    }

    pub fn active_count(&self) -> usize {
        self.active_by_coord.len()
    }

    pub fn total_mints(&self) -> usize {
        self.total_mints
    }

    pub fn total_burns(&self) -> usize {
        self.total_burns
    }

    pub fn history_genesis(&self) -> ContentTag {
        self.history_genesis
    }

    pub fn history_head(&self) -> ContentTag {
        self.history_head
    }

    /// Checks that the active episodes match the history's circulating supply and active mints.
    pub fn verify_consistency(&self, history: &History, _initial_world: &World) -> bool {
        if self.active_count() != history.circulating_supply() {
            return false;
        }
        for (&coord, episode) in &self.active_by_coord {
            if history.active_mint(coord) != Some(episode.mint) {
                return false;
            }
        }
        true
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
            "#bb.receipt.sha256.6feyyp2ttluxp572qxdyfdzmwkgp4i3ad72usqvm32p3lc6ozo5a"
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
}
