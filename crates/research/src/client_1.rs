//! `CLIENT-1`: proof-bound 6-state provisional and finalized client state tracker (`PS-057`).
//!
//! Realizes every normative invariant in `docs/CAPABILITY_TREE.md` §`CLIENT-1` behind
//! `--features protocol-research` without touching any frozen surface:
//! - **Strict 6-state transaction lifecycle (`ClientTxStatusV1`):** `Submitted`, `Admitted`,
//!   `IncludedProvisional` (`PrepareQC`-only), `Finalized` (strict `> 2/3` `CommitQC`), `Rejected`,
//!   and `Replaced` — never presenting a sequencer/mempool acknowledgement or `PrepareQC` candidate
//!   as finalized truth (`ProvisionalNotFinalized`).
//! - **Proof-bound header, receipt, and state verification:** independently verifies
//!   [`ConsensusCommitCertificateV1`] (`Proposal` + `PrepareQC` + `CommitQC`) against immutable
//!   `VALIDATOR-SCHEDULE-1` epoch schedules (including `epoch 0 -> epoch 2` validator-set changes),
//!   `O(log n)` [`ReceiptMerkleProofV1`] against `receipts_merkle_root`, and bounded `<= 12 KiB`
//!   [`DomainPatriciaProofV1`] witnesses against the committed 11-slot [`ExecutionRootRefV1`]
//!   (rejecting wrong-root proofs and withheld bodies fail-closed).
//! - **Atomic pre-finality fork/reorg & nonce-replacement rollback:** when a higher-tip tx replaces
//!   a pending tx at the same `(actor, nonce)` or a competing finalized branch displaces a
//!   `PrepareQC`-only candidate, all provisional balance deltas, minted `MintIdV1` episodes, and
//!   occupancy ownership projections roll back atomically — proving **zero phantom ownership** and
//!   **zero phantom supply**.
//! - **Light-client trust inputs & expiry:** enforces weak-subjectivity checkpoint freshness
//!   (`<= 28` epochs, rejecting stale checkpoints with `StaleWeakSubjectivityCheckpoint`) and clean
//!   expiry + resubmission past `valid_until_height`.
//!
//! Non-claim: a locally displayed "confirmed" label is never stronger than the exact consensus
//! evidence and assumptions the client independently verified. Independent-process behavior remains
//! for `TESTNET-1`.

use std::collections::BTreeMap;
use std::fmt;

use sha2::{Digest, Sha256};

use crate::asset_1::EpisodeOwnershipValueV1;
use crate::auth_commit_1::{AuthCommit1Error, DomainIdV1, DomainPatriciaProofV1};
use crate::bounds_1::ProtocolBoundsProfileV1;
use crate::consensus_1::{
    Consensus1Error, ConsensusClusterHarnessV1, ConsensusCommitCertificateV1, ConsensusProposalV1,
    ConsensusVotePhaseV1, QuorumCertificateV1,
};
use crate::execution_root_1::ExecutionRootRefV1;
use crate::executor_1::ExecutorForkContextV1;
use crate::identity_d1::ActorIdV1;
use crate::multi_1::{
    Multi1Error, MultiActorDispositionV1, MultiActorFoldReceiptV1, MultiActorStepOutcomeV1,
    MultiActorStreamBundleV1, SignedActorTxEnvelopeV1,
};
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::sequencer_1::{
    ReceiptMerkleProofV1, Sequencer1Error, compute_batch_body_digest, compute_receipts_merkle_root,
    receipt_merkle_leaf_digest,
};
use crate::validator_schedule_1::EpochValidatorScheduleV1;
use crate::world::Coord;

/// Canonical schema string for a [`ClientVerifiedStateViewV1`] (`CLIENT-1`).
pub const CLIENT_1_STATE_VIEW_SCHEMA: &[u8] = b"bitblock/client-1/verified-state-view/v1";
/// Canonical schema string for a [`ClientTxLifecycleReceiptV1`] (`CLIENT-1`).
pub const CLIENT_1_TX_LIFECYCLE_SCHEMA: &[u8] = b"bitblock/client-1/tx-lifecycle-receipt/v1";

/// Protocol version encoded in all `CLIENT-1` state views and lifecycle receipts.
pub const CLIENT_1_VERSION: u16 = 1;
/// Maximum allowed weak-subjectivity checkpoint age in epochs (`CONSENSUS-D1` / `THREAT-1`).
pub const MAX_WEAK_SUBJECTIVITY_AGE_EPOCHS: u64 = 28;

const CLI_STATE_SUMMARY_PREFIX: &[u8] = b"bb.cli1.state_summary.v1";

/// Fail-closed verification, trust-anchor, proof, reorg, or codec error for `CLIENT-1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Client1Error {
    /// Truncated, non-canonical, invalid discriminant, or trailing-byte wire encoding.
    InvalidWire(&'static str),
    /// Weak-subjectivity checkpoint exceeded `max_checkpoint_age_epochs` (`> 28` epochs stale).
    StaleWeakSubjectivityCheckpoint {
        /// Checkpoint anchor epoch.
        checkpoint_epoch: u64,
        /// Observed target epoch.
        target_epoch: u64,
        /// Maximum permitted epoch span.
        max_age_epochs: u64,
    },
    /// Attempted to treat a `PrepareQC`-only provisional candidate as finalized truth.
    ProvisionalNotFinalized(&'static str),
    /// Referenced block body or fold receipt was withheld (refused fail-closed).
    MissingBodyRefused,
    /// `DomainPatriciaProofV1` or `ReceiptMerkleProofV1` disagreed with committed header root or
    /// exceeded `BOUNDS-1` proof cap.
    WrongRootStateProof(&'static str),
    /// Duplicate tx or lower-tip replacement attempt at an occupied `(principal, nonce)`.
    DuplicateOrLowerTipReplacement(&'static str),
    /// Submitted transaction already expired (`valid_until_height <= finalized_height`).
    TransactionExpired,
    /// Height, parent block ID, fork context, or schedule epoch disagreed with trust anchor.
    ChainContinuityViolation(&'static str),
    /// Unknown transaction digest queried for lifecycle receipt.
    UnknownTransaction([u8; 32]),
    /// Underlying `CONSENSUS-1` verification error.
    Consensus(Box<Consensus1Error>),
    /// Underlying `SEQUENCER-1` receipt-proof error.
    Sequencer(Sequencer1Error),
    /// Underlying `AUTH-COMMIT-1` Patricia proof error.
    AuthCommit(AuthCommit1Error),
    /// Underlying `MULTI-1` codec error.
    Multi(Multi1Error),
}

impl From<Consensus1Error> for Client1Error {
    fn from(v: Consensus1Error) -> Self {
        Self::Consensus(Box::new(v))
    }
}
impl From<Sequencer1Error> for Client1Error {
    fn from(v: Sequencer1Error) -> Self {
        Self::Sequencer(v)
    }
}
impl From<AuthCommit1Error> for Client1Error {
    fn from(v: AuthCommit1Error) -> Self {
        Self::AuthCommit(v)
    }
}
impl From<Multi1Error> for Client1Error {
    fn from(v: Multi1Error) -> Self {
        Self::Multi(v)
    }
}
impl fmt::Display for Client1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Client1Error {}

struct CliReader<'a> {
    buf: &'a [u8],
    idx: usize,
}

impl<'a> CliReader<'a> {
    fn open(buf: &'a [u8]) -> Self {
        Self { buf, idx: 0 }
    }
    fn slice(&mut self, n: usize) -> Result<&'a [u8], Client1Error> {
        let to = self
            .idx
            .checked_add(n)
            .ok_or(Client1Error::InvalidWire("cli reader overflow"))?;
        if to > self.buf.len() {
            return Err(Client1Error::InvalidWire("truncated client_1 payload"));
        }
        let s = &self.buf[self.idx..to];
        self.idx = to;
        Ok(s)
    }
    fn byte(&mut self) -> Result<u8, Client1Error> {
        Ok(self.slice(1)?[0])
    }
    fn u16_be(&mut self) -> Result<u16, Client1Error> {
        let s = self.slice(2)?;
        Ok(u16::from_be_bytes([s[0], s[1]]))
    }
    fn u32_be(&mut self) -> Result<u32, Client1Error> {
        let s = self.slice(4)?;
        Ok(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn u64_be(&mut self) -> Result<u64, Client1Error> {
        Ok(u64::from_be_bytes(self.slice(8)?.try_into().expect("8")))
    }
    fn u128_be(&mut self) -> Result<u128, Client1Error> {
        Ok(u128::from_be_bytes(self.slice(16)?.try_into().expect("16")))
    }
    fn h32(&mut self) -> Result<[u8; 32], Client1Error> {
        Ok(self.slice(32)?.try_into().expect("32"))
    }
    fn bool_b(&mut self) -> Result<bool, Client1Error> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Client1Error::InvalidWire("non-canonical bool")),
        }
    }
    fn finish(&self) -> Result<(), Client1Error> {
        if self.idx != self.buf.len() {
            return Err(Client1Error::InvalidWire("trailing client_1 bytes"));
        }
        Ok(())
    }
}

/// Exact 6-state client transaction lifecycle required by `docs/CAPABILITY_TREE.md` §`CLIENT-1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ClientTxStatusV1 {
    /// Locally signed and broadcast; zero mempool, inclusion, or finality guarantee.
    Submitted = 1,
    /// Acknowledged into candidate admission queue; zero inclusion or finality guarantee.
    Admitted = 2,
    /// Included in a `PrepareQC`-only candidate block; reversible upon pre-finality fork/reorg.
    IncludedProvisional = 3,
    /// Finalized under verified strict `> 2/3` `CommitQC` + `ReceiptMerkleProofV1`.
    Finalized = 4,
    /// Rejected by execution disposition, displaced on reorg, or expired past `valid_until_height`.
    Rejected = 5,
    /// Atomically superseded by a strictly higher-tip competing tx at the same `(actor, nonce)`.
    Replaced = 6,
}

impl ClientTxStatusV1 {
    /// Returns the canonical 1-byte lifecycle discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte lifecycle discriminant fail-closed.
    pub const fn from_code(c: u8) -> Result<Self, Client1Error> {
        match c {
            1 => Ok(Self::Submitted),
            2 => Ok(Self::Admitted),
            3 => Ok(Self::IncludedProvisional),
            4 => Ok(Self::Finalized),
            5 => Ok(Self::Rejected),
            6 => Ok(Self::Replaced),
            _ => Err(Client1Error::InvalidWire("unknown ClientTxStatusV1")),
        }
    }
}

/// Explicit root verification strength exposed to wallets and observers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ClientRootStatusV1 {
    /// Genesis anchor before any consensus block is finalized.
    GenesisAnchor = 1,
    /// Provisional `PrepareQC`-only candidate (reversible; NEVER finalized truth).
    ProvisionalCandidate = 2,
    /// Finalized under verified strict `> 2/3` `CommitQC` and immutable validator schedule.
    FinalizedCommitQc = 3,
}

impl ClientRootStatusV1 {
    /// Returns the canonical 1-byte root-status discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte root-status discriminant fail-closed.
    pub const fn from_code(c: u8) -> Result<Self, Client1Error> {
        match c {
            1 => Ok(Self::GenesisAnchor),
            2 => Ok(Self::ProvisionalCandidate),
            3 => Ok(Self::FinalizedCommitQc),
            _ => Err(Client1Error::InvalidWire("unknown ClientRootStatusV1")),
        }
    }
}

/// Light-client trust inputs pinning genesis, profile, weak-subjectivity checkpoint age, and
/// immutable epoch validator schedules.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientTrustAnchorV1 {
    /// Pinned launch `genesis_id`.
    pub genesis_id: [u8; 32],
    /// Pinned active `profile_id`.
    pub profile_id: [u8; 32],
    /// Weak-subjectivity trusted checkpoint epoch.
    pub weak_subjectivity_checkpoint_epoch: u64,
    /// Hard maximum allowed epoch distance (`28` epochs).
    pub max_checkpoint_age_epochs: u64,
    /// Immutable committed `VALIDATOR-SCHEDULE-1` epoch schedules keyed by `effective_epoch`.
    pub schedules: BTreeMap<u64, EpochValidatorScheduleV1>,
}

impl ClientTrustAnchorV1 {
    /// Constructs a trust anchor pinned to `genesis_id`, `profile_id`, initial checkpoint epoch,
    /// and initial validator schedules.
    pub fn new(
        genesis_id: [u8; 32],
        profile_id: [u8; 32],
        weak_subjectivity_checkpoint_epoch: u64,
        max_checkpoint_age_epochs: u64,
        schedules: BTreeMap<u64, EpochValidatorScheduleV1>,
    ) -> Result<Self, Client1Error> {
        if max_checkpoint_age_epochs == 0
            || max_checkpoint_age_epochs > MAX_WEAK_SUBJECTIVITY_AGE_EPOCHS
        {
            return Err(Client1Error::InvalidWire(
                "max_checkpoint_age_epochs must lie in 1..=28",
            ));
        }
        for s in schedules.values() {
            s.validate()
                .map_err(|_| Client1Error::ChainContinuityViolation("invalid schedule"))?;
        }
        Ok(Self {
            genesis_id,
            profile_id,
            weak_subjectivity_checkpoint_epoch,
            max_checkpoint_age_epochs,
            schedules,
        })
    }

    /// Rejects any target epoch outside the weak-subjectivity trust horizon fail-closed.
    pub fn check_checkpoint_freshness(&self, target_epoch: u64) -> Result<(), Client1Error> {
        let max_allowed = self
            .weak_subjectivity_checkpoint_epoch
            .saturating_add(self.max_checkpoint_age_epochs);
        if target_epoch > max_allowed {
            return Err(Client1Error::StaleWeakSubjectivityCheckpoint {
                checkpoint_epoch: self.weak_subjectivity_checkpoint_epoch,
                target_epoch,
                max_age_epochs: self.max_checkpoint_age_epochs,
            });
        }
        Ok(())
    }

    /// Registers a verified validator-set transition schedule for a future epoch without
    /// overwriting any immutable historical schedule.
    pub fn register_epoch_schedule(
        &mut self,
        schedule: EpochValidatorScheduleV1,
    ) -> Result<(), Client1Error> {
        self.check_checkpoint_freshness(schedule.effective_epoch)?;
        schedule
            .validate()
            .map_err(|_| Client1Error::ChainContinuityViolation("invalid epoch schedule"))?;
        if self.schedules.contains_key(&schedule.effective_epoch) {
            return Err(Client1Error::ChainContinuityViolation(
                "cannot overwrite existing immutable schedule",
            ));
        }
        self.schedules.insert(schedule.effective_epoch, schedule);
        Ok(())
    }

    /// Looks up the active epoch schedule after checking weak-subjectivity checkpoint freshness.
    pub fn active_schedule(&self, epoch: u64) -> Result<&EpochValidatorScheduleV1, Client1Error> {
        self.check_checkpoint_freshness(epoch)?;
        self.schedules
            .get(&epoch)
            .ok_or(Client1Error::ChainContinuityViolation(
                "missing validator schedule for target epoch",
            ))
    }
}

/// Per-transaction tracked lifecycle record inside [`ClientStateTrackerV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ClientTxRecordV1 {
    /// Canonical envelope digest (`SignedActorTxEnvelopeV1::envelope_digest`).
    pub tx_digest: [u8; 32],
    /// Principal actor identity.
    pub principal_actor_id: ActorIdV1,
    /// Principal sequential nonce.
    pub nonce: u64,
    /// Priority tip (governs nonce-replacement precedence).
    pub priority_tip: u128,
    /// Expiry height after which an unfinalized tx transitions cleanly to `Rejected`.
    pub valid_until_height: u64,
    /// Strongest independently verified 6-state lifecycle status.
    pub status: ClientTxStatusV1,
    /// Strongest root status backing this transaction's current state.
    pub root_status: ClientRootStatusV1,
    /// Height where included/finalized (`0` if not yet included).
    pub observed_height: u64,
    /// Block ID where included/finalized (`[0u8; 32]` if not yet included).
    pub observed_block_id: [u8; 32],
    /// Replacement tx digest if `status == Replaced` (`[0u8; 32]` otherwise).
    pub replaced_by_tx_digest: [u8; 32],
}

/// Reversible `PrepareQC`-only candidate branch held strictly separate from finalized state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientProvisionalBranchV1 {
    /// Candidate height (`finalized_height + 1`).
    pub height: u64,
    /// Candidate epoch.
    pub epoch: u64,
    /// Candidate proposal `block_id`.
    pub block_id: [u8; 32],
    /// Candidate 11-slot execution root reference (provisional only!).
    pub execution_root_ref: ExecutionRootRefV1,
    /// Provisional projected liquid balances (atomically discarded on reorg!).
    pub provisional_balances: BTreeMap<ActorIdV1, u128>,
    /// Provisional projected occupancy episodes (atomically discarded on reorg!).
    pub provisional_episodes: BTreeMap<Coord, EpisodeOwnershipValueV1>,
    /// Transaction digests provisionally included in this candidate block.
    pub included_tx_digests: Vec<[u8; 32]>,
}

/// Canonical verified client state summary (`bitblock/client-1/verified-state-view/v1`, `450` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ClientVerifiedStateViewV1 {
    /// Schema version (`1`).
    pub view_version: u16,
    /// Pinned launch `genesis_id`.
    pub genesis_id: [u8; 32],
    /// Pinned active `profile_id`.
    pub profile_id: [u8; 32],
    /// Strongest verified root status at tip (`GenesisAnchor` or `FinalizedCommitQc`).
    pub finalized_root_status: ClientRootStatusV1,
    /// Verified finalized block height.
    pub finalized_height: u64,
    /// Verified finalized epoch.
    pub finalized_epoch: u64,
    /// Verified finalized `block_id` (`[0u8; 32]` at genesis).
    pub finalized_block_id: [u8; 32],
    /// Verified finalized 11-slot [`ExecutionRootRefV1`] (`178` bytes).
    pub finalized_execution_root_ref: ExecutionRootRefV1,
    /// Verified finalized `receipts_merkle_root`.
    pub finalized_receipts_root: [u8; 32],
    /// Weak-subjectivity checkpoint anchor epoch.
    pub weak_subjectivity_checkpoint_epoch: u64,
    /// Exact sum of finalized liquid balances (never includes unfinalized provisional deltas!).
    pub finalized_total_liquid_balance: u128,
    /// Exact sum of finalized locked occupancy deposits (zero phantom supply!).
    pub finalized_total_locked_deposit: u128,
    /// Exact count of finalized occupied episodes (zero phantom ownership!).
    pub finalized_episode_count: u32,
    /// Whether an active reversible `PrepareQC`-only provisional branch is currently tracked.
    pub has_provisional_branch: bool,
    /// Provisional candidate `block_id` (`[0u8; 32]` when none or rolled back).
    pub provisional_block_id: [u8; 32],
    /// Total tracked transactions across all 6 states.
    pub tracked_tx_count: u32,
    /// Count of `Finalized` transactions.
    pub finalized_tx_count: u32,
    /// Count of `Rejected` transactions (includes reorg-displaced & expired).
    pub rejected_tx_count: u32,
    /// Count of `Replaced` transactions (nonce-superseded).
    pub replaced_tx_count: u32,
    /// Domain-separated commitment over sorted finalized balances + episodes.
    pub finalized_state_summary_digest: [u8; 32],
}

impl ClientVerifiedStateViewV1 {
    /// Fixed canonical big-endian wire length (`450` bytes).
    pub const WIRE_LEN: usize =
        2 + 32 * 6 + 1 + 8 * 3 + ExecutionRootRefV1::WIRE_LEN + 16 * 2 + 4 * 5 + 1;

    /// Encodes this verified client state view into 450 canonical big-endian bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], Client1Error> {
        if self.view_version != CLIENT_1_VERSION {
            return Err(Client1Error::InvalidWire("bad client state view version"));
        }
        let mut out = Vec::with_capacity(Self::WIRE_LEN);
        out.extend_from_slice(&self.view_version.to_be_bytes());
        out.extend_from_slice(&self.genesis_id);
        out.extend_from_slice(&self.profile_id);
        out.push(self.finalized_root_status.code());
        out.extend_from_slice(&self.finalized_height.to_be_bytes());
        out.extend_from_slice(&self.finalized_epoch.to_be_bytes());
        out.extend_from_slice(&self.finalized_block_id);
        out.extend_from_slice(&self.finalized_execution_root_ref.encode_canonical());
        out.extend_from_slice(&self.finalized_receipts_root);
        out.extend_from_slice(&self.weak_subjectivity_checkpoint_epoch.to_be_bytes());
        out.extend_from_slice(&self.finalized_total_liquid_balance.to_be_bytes());
        out.extend_from_slice(&self.finalized_total_locked_deposit.to_be_bytes());
        out.extend_from_slice(&self.finalized_episode_count.to_be_bytes());
        out.push(u8::from(self.has_provisional_branch));
        out.extend_from_slice(&self.provisional_block_id);
        out.extend_from_slice(&self.tracked_tx_count.to_be_bytes());
        out.extend_from_slice(&self.finalized_tx_count.to_be_bytes());
        out.extend_from_slice(&self.rejected_tx_count.to_be_bytes());
        out.extend_from_slice(&self.replaced_tx_count.to_be_bytes());
        out.extend_from_slice(&self.finalized_state_summary_digest);
        Ok(out.try_into().expect("450"))
    }

    /// Decodes 450 canonical big-endian bytes fail-closed.
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, Client1Error> {
        let mut r = CliReader::open(raw);
        let view_version = r.u16_be()?;
        if view_version != CLIENT_1_VERSION {
            return Err(Client1Error::InvalidWire("bad client state view version"));
        }
        let genesis_id = r.h32()?;
        let profile_id = r.h32()?;
        let finalized_root_status = ClientRootStatusV1::from_code(r.byte()?)?;
        let finalized_height = r.u64_be()?;
        let finalized_epoch = r.u64_be()?;
        let finalized_block_id = r.h32()?;
        let finalized_execution_root_ref =
            ExecutionRootRefV1::decode_canonical(r.slice(ExecutionRootRefV1::WIRE_LEN)?)
                .map_err(|_| Client1Error::InvalidWire("bad execution_root_ref"))?;
        let finalized_receipts_root = r.h32()?;
        let weak_subjectivity_checkpoint_epoch = r.u64_be()?;
        let finalized_total_liquid_balance = r.u128_be()?;
        let finalized_total_locked_deposit = r.u128_be()?;
        let finalized_episode_count = r.u32_be()?;
        let has_provisional_branch = r.bool_b()?;
        let provisional_block_id = r.h32()?;
        let tracked_tx_count = r.u32_be()?;
        let finalized_tx_count = r.u32_be()?;
        let rejected_tx_count = r.u32_be()?;
        let replaced_tx_count = r.u32_be()?;
        let finalized_state_summary_digest = r.h32()?;
        r.finish()?;
        Ok(Self {
            view_version,
            genesis_id,
            profile_id,
            finalized_root_status,
            finalized_height,
            finalized_epoch,
            finalized_block_id,
            finalized_execution_root_ref,
            finalized_receipts_root,
            weak_subjectivity_checkpoint_epoch,
            finalized_total_liquid_balance,
            finalized_total_locked_deposit,
            finalized_episode_count,
            has_provisional_branch,
            provisional_block_id,
            tracked_tx_count,
            finalized_tx_count,
            rejected_tx_count,
            replaced_tx_count,
            finalized_state_summary_digest,
        })
    }

    /// Computes the canonical `Receipt` `ContentTag` (`bitblock/client-1/verified-state-view/v1`).
    pub fn tag(&self) -> Result<ContentTag, Client1Error> {
        let raw = self.encode_canonical()?;
        hash_canonical(
            TagKind::Receipt,
            CLIENT_1_STATE_VIEW_SCHEMA,
            raw.len() as u64,
            |w| w(&raw),
        )
        .map_err(|_| Client1Error::InvalidWire("state view hash_canonical failed"))
    }
}

/// Canonical proof-bound client transaction lifecycle receipt
/// (`bitblock/client-1/tx-lifecycle-receipt/v1`, `392` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ClientTxLifecycleReceiptV1 {
    /// Receipt schema version (`1`).
    pub receipt_version: u16,
    /// Pinned launch `genesis_id`.
    pub genesis_id: [u8; 32],
    /// Pinned active `profile_id`.
    pub profile_id: [u8; 32],
    /// Tracked transaction digest.
    pub tx_digest: [u8; 32],
    /// Principal actor identity.
    pub principal_actor_id: ActorIdV1,
    /// Principal nonce.
    pub nonce: u64,
    /// Strongest verified 6-state status (`Submitted..Replaced`).
    pub status: ClientTxStatusV1,
    /// Strongest verified root status backing `status`.
    pub root_status: ClientRootStatusV1,
    /// Observed block height (`0` if unincluded).
    pub observed_height: u64,
    /// Observed `block_id` (`[0u8; 32]` if unincluded).
    pub observed_block_id: [u8; 32],
    /// Observed 11-slot [`ExecutionRootRefV1`] (`178` bytes).
    pub observed_execution_root_ref: ExecutionRootRefV1,
    /// Replacement transaction digest if `status == Replaced` (`[0u8; 32]` otherwise).
    pub replaced_by_tx_digest: [u8; 32],
    /// Zero phantom-ownership guarantee (must be `false`).
    pub phantom_ownership_retained: bool,
    /// Zero phantom-supply guarantee (must be `false`).
    pub phantom_supply_retained: bool,
}

impl ClientTxLifecycleReceiptV1 {
    /// Fixed canonical big-endian wire length (`392` bytes).
    pub const WIRE_LEN: usize =
        2 + 32 * 4 + 8 + 1 + 1 + 8 + 32 + ExecutionRootRefV1::WIRE_LEN + 32 + 1 + 1;

    /// Encodes this lifecycle receipt into 392 canonical big-endian bytes, enforcing zero phantom
    /// ownership and zero phantom supply fail-closed.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], Client1Error> {
        if self.receipt_version != CLIENT_1_VERSION {
            return Err(Client1Error::InvalidWire(
                "bad tx lifecycle receipt version",
            ));
        }
        if self.phantom_ownership_retained || self.phantom_supply_retained {
            return Err(Client1Error::InvalidWire(
                "phantom ownership or supply cannot be retained in CLIENT-1",
            ));
        }
        if self.status == ClientTxStatusV1::Finalized
            && self.root_status != ClientRootStatusV1::FinalizedCommitQc
        {
            return Err(Client1Error::ProvisionalNotFinalized(
                "Finalized status strictly requires FinalizedCommitQc root_status",
            ));
        }
        let mut out = Vec::with_capacity(Self::WIRE_LEN);
        out.extend_from_slice(&self.receipt_version.to_be_bytes());
        out.extend_from_slice(&self.genesis_id);
        out.extend_from_slice(&self.profile_id);
        out.extend_from_slice(&self.tx_digest);
        out.extend_from_slice(self.principal_actor_id.as_bytes());
        out.extend_from_slice(&self.nonce.to_be_bytes());
        out.push(self.status.code());
        out.push(self.root_status.code());
        out.extend_from_slice(&self.observed_height.to_be_bytes());
        out.extend_from_slice(&self.observed_block_id);
        out.extend_from_slice(&self.observed_execution_root_ref.encode_canonical());
        out.extend_from_slice(&self.replaced_by_tx_digest);
        out.push(u8::from(self.phantom_ownership_retained));
        out.push(u8::from(self.phantom_supply_retained));
        Ok(out.try_into().expect("392"))
    }

    /// Decodes 392 canonical big-endian bytes fail-closed.
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, Client1Error> {
        let mut r = CliReader::open(raw);
        let receipt_version = r.u16_be()?;
        if receipt_version != CLIENT_1_VERSION {
            return Err(Client1Error::InvalidWire(
                "bad tx lifecycle receipt version",
            ));
        }
        let genesis_id = r.h32()?;
        let profile_id = r.h32()?;
        let tx_digest = r.h32()?;
        let principal_actor_id = ActorIdV1::from_bytes(r.h32()?);
        let nonce = r.u64_be()?;
        let status = ClientTxStatusV1::from_code(r.byte()?)?;
        let root_status = ClientRootStatusV1::from_code(r.byte()?)?;
        let observed_height = r.u64_be()?;
        let observed_block_id = r.h32()?;
        let observed_execution_root_ref =
            ExecutionRootRefV1::decode_canonical(r.slice(ExecutionRootRefV1::WIRE_LEN)?)
                .map_err(|_| Client1Error::InvalidWire("bad observed_execution_root_ref"))?;
        let replaced_by_tx_digest = r.h32()?;
        let phantom_ownership_retained = r.bool_b()?;
        let phantom_supply_retained = r.bool_b()?;
        r.finish()?;
        let rec = Self {
            receipt_version,
            genesis_id,
            profile_id,
            tx_digest,
            principal_actor_id,
            nonce,
            status,
            root_status,
            observed_height,
            observed_block_id,
            observed_execution_root_ref,
            replaced_by_tx_digest,
            phantom_ownership_retained,
            phantom_supply_retained,
        };
        let _ = rec.encode_canonical()?;
        Ok(rec)
    }

    /// Computes the canonical `Receipt` `ContentTag` (`bitblock/client-1/tx-lifecycle-receipt/v1`).
    pub fn tag(&self) -> Result<ContentTag, Client1Error> {
        let raw = self.encode_canonical()?;
        hash_canonical(
            TagKind::Receipt,
            CLIENT_1_TX_LIFECYCLE_SCHEMA,
            raw.len() as u64,
            |w| w(&raw),
        )
        .map_err(|_| Client1Error::InvalidWire("tx lifecycle hash_canonical failed"))
    }
}

/// Computes the deterministic commitment over sorted finalized balances and occupancy episodes.
pub fn compute_finalized_state_summary_digest(
    balances: &BTreeMap<ActorIdV1, u128>,
    episodes: &BTreeMap<Coord, EpisodeOwnershipValueV1>,
) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(CLI_STATE_SUMMARY_PREFIX);
    h.update((balances.len() as u32).to_be_bytes());
    for (aid, bal) in balances {
        h.update(aid.as_bytes());
        h.update(bal.to_be_bytes());
    }
    h.update((episodes.len() as u32).to_be_bytes());
    for (coord, ep) in episodes {
        h.update(coord.x.to_be_bytes());
        h.update(coord.y.to_be_bytes());
        h.update(coord.z.to_be_bytes());
        h.update(ep.encode_canonical());
    }
    h.finalize().into()
}

/// Proof-bound client state tracker cleanly separating `Submitted`/`Admitted`,
/// reversible `IncludedProvisional` (`PrepareQC`-only), and `Finalized` (`CommitQC`) truth.
#[derive(Clone, Debug)]
pub struct ClientStateTrackerV1 {
    trust_anchor: ClientTrustAnchorV1,
    fork_context: ExecutorForkContextV1,
    bounds: ProtocolBoundsProfileV1,
    finalized_root_status: ClientRootStatusV1,
    finalized_height: u64,
    finalized_epoch: u64,
    finalized_block_id: [u8; 32],
    finalized_execution_root_ref: ExecutionRootRefV1,
    finalized_receipts_root: [u8; 32],
    finalized_balances: BTreeMap<ActorIdV1, u128>,
    finalized_episodes: BTreeMap<Coord, EpisodeOwnershipValueV1>,
    provisional_branch: Option<ClientProvisionalBranchV1>,
    txs: BTreeMap<[u8; 32], ClientTxRecordV1>,
    active_by_actor_nonce: BTreeMap<(ActorIdV1, u64), [u8; 32]>,
}

impl ClientStateTrackerV1 {
    /// Initializes a proof-bound client tracker at genesis from a [`ConsensusClusterHarnessV1`].
    pub fn from_cluster(cluster: &ConsensusClusterHarnessV1) -> Result<Self, Client1Error> {
        let fork = cluster.fixture.fork_context;
        let mut scheds = BTreeMap::new();
        for ep in 0..cluster
            .fixture
            .bundle
            .validator_econ_policy
            .activation_delay_epochs
        {
            if let Some(s) = cluster.schedule_state.schedule_at(ep) {
                scheds.insert(ep, s.clone());
            }
        }
        let trust_anchor = ClientTrustAnchorV1::new(
            fork.genesis_id,
            fork.profile_id,
            0,
            MAX_WEAK_SUBJECTIVITY_AGE_EPOCHS,
            scheds,
        )?;
        let init_comp = cluster.nodes[0].harness().current_composition()?;
        let mut finalized_balances = BTreeMap::new();
        for a in &cluster.fixture.package.state.initial_actors {
            finalized_balances.insert(a.actor_id, a.initial_liquid_balance);
        }
        Ok(Self {
            trust_anchor,
            fork_context: fork,
            bounds: cluster.fixture.bundle.bounds_profile,
            finalized_root_status: ClientRootStatusV1::GenesisAnchor,
            finalized_height: 0,
            finalized_epoch: 0,
            finalized_block_id: [0u8; 32],
            finalized_execution_root_ref: init_comp.execution_root_ref,
            finalized_receipts_root: [0u8; 32],
            finalized_balances,
            finalized_episodes: BTreeMap::new(),
            provisional_branch: None,
            txs: BTreeMap::new(),
            active_by_actor_nonce: BTreeMap::new(),
        })
    }

    /// Returns a reference to the client's light-client trust anchor.
    pub fn trust_anchor(&self) -> &ClientTrustAnchorV1 {
        &self.trust_anchor
    }

    /// Registers an epoch validator-set transition schedule in the trust anchor.
    pub fn register_epoch_schedule(
        &mut self,
        schedule: EpochValidatorScheduleV1,
    ) -> Result<(), Client1Error> {
        self.trust_anchor.register_epoch_schedule(schedule)
    }

    /// Returns the verified finalized liquid balance for `actor` (never provisional!).
    pub fn finalized_balance_of(&self, actor: &ActorIdV1) -> u128 {
        self.finalized_balances.get(actor).copied().unwrap_or(0)
    }

    /// Returns the verified finalized occupancy episode at `coord`, if any (never provisional!).
    pub fn finalized_episode_at(&self, coord: Coord) -> Option<EpisodeOwnershipValueV1> {
        self.finalized_episodes.get(&coord).copied()
    }

    /// Returns the provisional balance projection if a reversible branch exists, else finalized.
    pub fn provisional_or_finalized_balance_of(&self, actor: &ActorIdV1) -> u128 {
        if let Some(pb) = &self.provisional_branch {
            pb.provisional_balances.get(actor).copied().unwrap_or(0)
        } else {
            self.finalized_balance_of(actor)
        }
    }

    /// Returns the active reversible provisional branch, if any.
    pub fn provisional_branch(&self) -> Option<&ClientProvisionalBranchV1> {
        self.provisional_branch.as_ref()
    }

    /// Returns the tracked record for `tx_digest`, if known.
    pub fn tx_record(&self, tx_digest: &[u8; 32]) -> Option<&ClientTxRecordV1> {
        self.txs.get(tx_digest)
    }

    /// Submits a signed transaction (`Submitted`) with expiry height `valid_until_height`,
    /// handling strict higher-tip nonce replacement (`Replaced`) and rejecting duplicates or
    /// lower-tip replacements fail-closed.
    pub fn submit_tx(
        &mut self,
        env: &SignedActorTxEnvelopeV1,
        valid_until_height: u64,
    ) -> Result<[u8; 32], Client1Error> {
        if valid_until_height <= self.finalized_height {
            return Err(Client1Error::TransactionExpired);
        }
        let digest = env.envelope_digest()?;
        if self.txs.contains_key(&digest) {
            return Err(Client1Error::DuplicateOrLowerTipReplacement(
                "duplicate transaction digest already tracked",
            ));
        }
        let key = (env.principal_actor_id, env.nonce);
        if let Some(&prev_digest) = self.active_by_actor_nonce.get(&key)
            && let Some(prev) = self.txs.get_mut(&prev_digest)
        {
            match prev.status {
                ClientTxStatusV1::Finalized => {
                    return Err(Client1Error::DuplicateOrLowerTipReplacement(
                        "cannot replace already Finalized nonce",
                    ));
                }
                ClientTxStatusV1::Submitted
                | ClientTxStatusV1::Admitted
                | ClientTxStatusV1::IncludedProvisional => {
                    if env.priority_tip <= prev.priority_tip {
                        return Err(Client1Error::DuplicateOrLowerTipReplacement(
                            "replacement tx requires strictly higher priority_tip",
                        ));
                    }
                    prev.status = ClientTxStatusV1::Replaced;
                    prev.replaced_by_tx_digest = digest;
                }
                ClientTxStatusV1::Rejected | ClientTxStatusV1::Replaced => {}
            }
        }
        let rec = ClientTxRecordV1 {
            tx_digest: digest,
            principal_actor_id: env.principal_actor_id,
            nonce: env.nonce,
            priority_tip: env.priority_tip,
            valid_until_height,
            status: ClientTxStatusV1::Submitted,
            root_status: self.finalized_root_status,
            observed_height: 0,
            observed_block_id: [0u8; 32],
            replaced_by_tx_digest: [0u8; 32],
        };
        self.txs.insert(digest, rec);
        self.active_by_actor_nonce.insert(key, digest);
        Ok(digest)
    }

    /// Advances a locally `Submitted` tx to `Admitted` (explicitly NOT included or finalized!).
    pub fn mark_admitted(&mut self, tx_digest: [u8; 32]) -> Result<(), Client1Error> {
        let rec = self
            .txs
            .get_mut(&tx_digest)
            .ok_or(Client1Error::UnknownTransaction(tx_digest))?;
        if rec.status != ClientTxStatusV1::Submitted {
            return Err(Client1Error::InvalidWire(
                "only Submitted tx can transition to Admitted",
            ));
        }
        rec.status = ClientTxStatusV1::Admitted;
        Ok(())
    }

    /// Observes a reversible `PrepareQC`-only candidate block (`IncludedProvisional`), refusing
    /// missing bodies, stale weak-subjectivity checkpoints, or invalid QCs fail-closed.
    #[allow(clippy::too_many_arguments)]
    pub fn observe_provisional_candidate(
        &mut self,
        proposal: &ConsensusProposalV1,
        prepare_qc: &QuorumCertificateV1,
        body: Option<&MultiActorStreamBundleV1>,
        receipt: Option<&MultiActorFoldReceiptV1>,
        provisional_balances: BTreeMap<ActorIdV1, u128>,
        provisional_episodes: BTreeMap<Coord, EpisodeOwnershipValueV1>,
    ) -> Result<(), Client1Error> {
        let (Some(bundle), Some(fold_rcpt)) = (body, receipt) else {
            return Err(Client1Error::MissingBodyRefused);
        };
        if proposal.fork_context != self.fork_context
            || proposal.height != self.finalized_height + 1
            || proposal.parent_block_id != self.finalized_block_id
        {
            return Err(Client1Error::ChainContinuityViolation(
                "provisional candidate height, parent, or fork mismatch",
            ));
        }
        let schedule = self.trust_anchor.active_schedule(proposal.epoch)?.clone();
        proposal.verify_against_schedule(&schedule)?;
        if prepare_qc.phase != ConsensusVotePhaseV1::Prepare {
            return Err(Client1Error::InvalidWire("expected PrepareQC"));
        }
        prepare_qc.verify(&self.fork_context, &schedule)?;
        let bid = proposal.block_id();
        if prepare_qc.block_id != bid || prepare_qc.height != proposal.height {
            return Err(Client1Error::InvalidWire(
                "PrepareQC disagrees with candidate proposal",
            ));
        }
        let bdig =
            compute_batch_body_digest(&bundle.encode_canonical()?, &fold_rcpt.encode_canonical()?);
        if bdig != proposal.body_digest {
            return Err(Client1Error::WrongRootStateProof(
                "provisional body_digest mismatch",
            ));
        }
        let mut included = Vec::new();
        for step in &fold_rcpt.step_outcomes {
            if let Some(rec) = self.txs.get_mut(&step.tx_digest)
                && matches!(
                    rec.status,
                    ClientTxStatusV1::Submitted | ClientTxStatusV1::Admitted
                )
            {
                rec.status = ClientTxStatusV1::IncludedProvisional;
                rec.root_status = ClientRootStatusV1::ProvisionalCandidate;
                rec.observed_height = proposal.height;
                rec.observed_block_id = bid;
                included.push(step.tx_digest);
            }
        }
        self.provisional_branch = Some(ClientProvisionalBranchV1 {
            height: proposal.height,
            epoch: proposal.epoch,
            block_id: bid,
            execution_root_ref: proposal.execution_root_ref,
            provisional_balances,
            provisional_episodes,
            included_tx_digests: included,
        });
        Ok(())
    }

    /// Explicitly refuses attempting to claim finality from a `PrepareQC`-only candidate.
    pub fn require_commit_qc_not_prepare_only(
        &self,
        qc: &QuorumCertificateV1,
    ) -> Result<(), Client1Error> {
        if qc.phase != ConsensusVotePhaseV1::Commit {
            return Err(Client1Error::ProvisionalNotFinalized(
                "PrepareQC cannot upgrade state to Finalized; CommitQC required",
            ));
        }
        Ok(())
    }

    /// Applies a verified finalized [`ConsensusCommitCertificateV1`] (`CommitQC`) with complete
    /// body & receipt Merkle proof verification, atomically rolling back any displaced
    /// `PrepareQC`-only branch (zero phantom ownership / zero phantom supply!) and expiring stale
    /// unfinalized transactions.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_finalized_commit(
        &mut self,
        cert: &ConsensusCommitCertificateV1,
        body: Option<&MultiActorStreamBundleV1>,
        receipt: Option<&MultiActorFoldReceiptV1>,
        finalized_balances: BTreeMap<ActorIdV1, u128>,
        finalized_episodes: BTreeMap<Coord, EpisodeOwnershipValueV1>,
        verify_receipt_indices: &[u32],
    ) -> Result<bool, Client1Error> {
        let (Some(bundle), Some(fold_rcpt)) = (body, receipt) else {
            return Err(Client1Error::MissingBodyRefused);
        };
        if cert.proposal.fork_context != self.fork_context
            || cert.proposal.height != self.finalized_height + 1
            || cert.proposal.parent_block_id != self.finalized_block_id
        {
            return Err(Client1Error::ChainContinuityViolation(
                "finalized cert height, parent, or fork mismatch",
            ));
        }
        let schedule = self
            .trust_anchor
            .active_schedule(cert.proposal.epoch)?
            .clone();
        self.require_commit_qc_not_prepare_only(&cert.commit_qc)?;
        cert.verify(&schedule)?;

        let bdig =
            compute_batch_body_digest(&bundle.encode_canonical()?, &fold_rcpt.encode_canonical()?);
        let rroot = compute_receipts_merkle_root(&fold_rcpt.step_outcomes);
        if bdig != cert.proposal.body_digest || rroot != cert.proposal.receipts_merkle_root {
            return Err(Client1Error::WrongRootStateProof(
                "finalized body or receipts_merkle_root mismatch",
            ));
        }
        for &idx in verify_receipt_indices {
            let proof = ReceiptMerkleProofV1::build(&fold_rcpt.step_outcomes, idx)?;
            proof.verify_against_root(cert.proposal.receipts_merkle_root)?;
        }

        let winning_bid = cert.proposal.block_id();
        let mut rolled_back_reorg = false;

        // Pre-finality fork/reorg rollback: if provisional branch had a DIFFERENT block_id,
        // discard ALL provisional balances/episodes and mark displaced provisional txs Rejected!
        if let Some(pb) = self.provisional_branch.take()
            && pb.block_id != winning_bid
        {
            rolled_back_reorg = true;
            for td in pb.included_tx_digests {
                if let Some(rec) = self.txs.get_mut(&td)
                    && rec.status == ClientTxStatusV1::IncludedProvisional
                {
                    rec.status = ClientTxStatusV1::Rejected;
                    rec.root_status = ClientRootStatusV1::FinalizedCommitQc;
                    rec.observed_height = cert.proposal.height;
                    rec.observed_block_id = winning_bid;
                    // Release (actor, nonce) slot if it still pointed to this reorged-out tx
                    let k = (rec.principal_actor_id, rec.nonce);
                    if self.active_by_actor_nonce.get(&k) == Some(&td) {
                        self.active_by_actor_nonce.remove(&k);
                    }
                }
            }
        }

        // Update finalized tip and committed balances/episodes strictly from verified finalized truth
        self.finalized_root_status = ClientRootStatusV1::FinalizedCommitQc;
        self.finalized_height = cert.proposal.height;
        self.finalized_epoch = cert.proposal.epoch;
        self.finalized_block_id = winning_bid;
        self.finalized_execution_root_ref = cert.proposal.execution_root_ref;
        self.finalized_receipts_root = cert.proposal.receipts_merkle_root;
        self.finalized_balances = finalized_balances;
        self.finalized_episodes = finalized_episodes;

        // Transition every included transaction according to its verified execution disposition
        for step in &fold_rcpt.step_outcomes {
            if let Some(rec) = self.txs.get_mut(&step.tx_digest) {
                rec.root_status = ClientRootStatusV1::FinalizedCommitQc;
                rec.observed_height = cert.proposal.height;
                rec.observed_block_id = winning_bid;
                rec.status = if step.disposition == MultiActorDispositionV1::AdmittedExecuted {
                    ClientTxStatusV1::Finalized
                } else {
                    ClientTxStatusV1::Rejected
                };
            }
        }

        // Expire any still-unfinalized tx past valid_until_height cleanly to Rejected
        let cur_h = self.finalized_height;
        for rec in self.txs.values_mut() {
            if matches!(
                rec.status,
                ClientTxStatusV1::Submitted | ClientTxStatusV1::Admitted
            ) && rec.valid_until_height <= cur_h
            {
                rec.status = ClientTxStatusV1::Rejected;
                rec.root_status = ClientRootStatusV1::FinalizedCommitQc;
                rec.observed_height = cur_h;
                rec.observed_block_id = winning_bid;
                let k = (rec.principal_actor_id, rec.nonce);
                if self.active_by_actor_nonce.get(&k) == Some(&rec.tx_digest) {
                    self.active_by_actor_nonce.remove(&k);
                }
            }
        }

        Ok(rolled_back_reorg)
    }

    /// Independently verifies a [`ReceiptMerkleProofV1`] against the finalized `receipts_merkle_root`.
    pub fn verify_finalized_receipt_proof(
        &self,
        outcome: &MultiActorStepOutcomeV1,
        proof: &ReceiptMerkleProofV1,
    ) -> Result<(), Client1Error> {
        let enc = proof.encode_canonical()?;
        if enc.len() > self.bounds.max_proof_bytes as usize {
            return Err(Client1Error::WrongRootStateProof(
                "receipt proof exceeds BOUNDS-1 max_proof_bytes",
            ));
        }
        let leaf = receipt_merkle_leaf_digest(proof.leaf_index, outcome);
        if leaf != proof.leaf_digest {
            return Err(Client1Error::WrongRootStateProof(
                "receipt leaf digest disagrees with step outcome",
            ));
        }
        proof
            .verify_against_root(self.finalized_receipts_root)
            .map_err(|_| {
                Client1Error::WrongRootStateProof("receipt proof failed finalized merkle root")
            })
    }

    /// Independently verifies a bounded `<= 12 KiB` [`DomainPatriciaProofV1`] against a committed
    /// domain root (rejects wrong-root proofs and oversized proofs fail-closed).
    pub fn verify_finalized_state_proof(
        &self,
        domain: DomainIdV1,
        expected_domain_root: [u8; 32],
        key: &[u8; 32],
        proof: &DomainPatriciaProofV1,
    ) -> Result<Option<Vec<u8>>, Client1Error> {
        let enc = proof.encode_canonical()?;
        if enc.len() > self.bounds.max_proof_bytes as usize {
            return Err(Client1Error::WrongRootStateProof(
                "state proof exceeds BOUNDS-1 max_proof_bytes",
            ));
        }
        proof
            .verify(domain, expected_domain_root, key)
            .map_err(|_| {
                Client1Error::WrongRootStateProof("DomainPatriciaProofV1 failed committed root")
            })
    }

    /// Emits a canonical [`ClientVerifiedStateViewV1`] summarizing finalized truth and provisional
    /// status without ever leaking provisional balances or episodes into finalized totals.
    pub fn verified_state_view(&self) -> ClientVerifiedStateViewV1 {
        let finalized_total_liquid_balance: u128 = self.finalized_balances.values().copied().sum();
        let finalized_total_locked_deposit: u128 = self
            .finalized_episodes
            .values()
            .map(|e| e.locked_deposit)
            .sum();
        let (mut fin_c, mut rej_c, mut rep_c) = (0u32, 0u32, 0u32);
        for rec in self.txs.values() {
            match rec.status {
                ClientTxStatusV1::Finalized => fin_c += 1,
                ClientTxStatusV1::Rejected => rej_c += 1,
                ClientTxStatusV1::Replaced => rep_c += 1,
                _ => {}
            }
        }
        let (has_provisional_branch, provisional_block_id) = match &self.provisional_branch {
            Some(pb) => (true, pb.block_id),
            None => (false, [0u8; 32]),
        };
        let finalized_state_summary_digest = compute_finalized_state_summary_digest(
            &self.finalized_balances,
            &self.finalized_episodes,
        );
        ClientVerifiedStateViewV1 {
            view_version: CLIENT_1_VERSION,
            genesis_id: self.trust_anchor.genesis_id,
            profile_id: self.trust_anchor.profile_id,
            finalized_root_status: self.finalized_root_status,
            finalized_height: self.finalized_height,
            finalized_epoch: self.finalized_epoch,
            finalized_block_id: self.finalized_block_id,
            finalized_execution_root_ref: self.finalized_execution_root_ref,
            finalized_receipts_root: self.finalized_receipts_root,
            weak_subjectivity_checkpoint_epoch: self
                .trust_anchor
                .weak_subjectivity_checkpoint_epoch,
            finalized_total_liquid_balance,
            finalized_total_locked_deposit,
            finalized_episode_count: self.finalized_episodes.len() as u32,
            has_provisional_branch,
            provisional_block_id,
            tracked_tx_count: self.txs.len() as u32,
            finalized_tx_count: fin_c,
            rejected_tx_count: rej_c,
            replaced_tx_count: rep_c,
            finalized_state_summary_digest,
        }
    }

    /// Emits a canonical [`ClientTxLifecycleReceiptV1`] for `tx_digest`.
    pub fn tx_lifecycle_receipt(
        &self,
        tx_digest: [u8; 32],
    ) -> Result<ClientTxLifecycleReceiptV1, Client1Error> {
        let rec = self
            .txs
            .get(&tx_digest)
            .ok_or(Client1Error::UnknownTransaction(tx_digest))?;
        let obs_ref = if rec.root_status == ClientRootStatusV1::ProvisionalCandidate {
            self.provisional_branch
                .as_ref()
                .map(|pb| pb.execution_root_ref)
                .unwrap_or(self.finalized_execution_root_ref)
        } else {
            self.finalized_execution_root_ref
        };
        Ok(ClientTxLifecycleReceiptV1 {
            receipt_version: CLIENT_1_VERSION,
            genesis_id: self.trust_anchor.genesis_id,
            profile_id: self.trust_anchor.profile_id,
            tx_digest: rec.tx_digest,
            principal_actor_id: rec.principal_actor_id,
            nonce: rec.nonce,
            status: rec.status,
            root_status: rec.root_status,
            observed_height: rec.observed_height,
            observed_block_id: rec.observed_block_id,
            observed_execution_root_ref: obs_ref,
            replaced_by_tx_digest: rec.replaced_by_tx_digest,
            phantom_ownership_retained: false,
            phantom_supply_retained: false,
        })
    }
}

/// Extracts finalized liquid balances and occupancy episodes from a [`ConsensusClusterHarnessV1`]
/// node after a finalized block so tests and probes build verified client snapshots cleanly.
pub fn snapshot_cluster_finalized_assets(
    cluster: &ConsensusClusterHarnessV1,
    coords: &[Coord],
) -> (
    BTreeMap<ActorIdV1, u128>,
    BTreeMap<Coord, EpisodeOwnershipValueV1>,
) {
    let h = cluster.nodes[0].harness();
    let mut bals = BTreeMap::new();
    for a in &cluster.fixture.package.state.initial_actors {
        bals.insert(a.actor_id, h.balance_of(&a.actor_id));
    }
    let dep = cluster
        .fixture
        .bundle
        .asset_policy
        .deposit_per_occupied_block;
    let mut eps = BTreeMap::new();
    for &c in coords {
        if let Some(mid) = h.episode_mint_at(c) {
            eps.insert(
                c,
                EpisodeOwnershipValueV1 {
                    owner: cluster.fixture.actor_a,
                    mint_episode_id: mid,
                    locked_deposit: dep,
                },
            );
        }
    }
    (bals, eps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_1_six_state_lifecycle_and_golden_tags() {
        let mut cluster = ConsensusClusterHarnessV1::bootstrap_four_validators().unwrap();
        let mut client = ClientStateTrackerV1::from_cluster(&cluster).unwrap();
        let pre_wr = cluster.nodes[0].harness().snapshot().unwrap().world_root;
        let bundle = cluster
            .fixture
            .canonical_mixed_race_bundle(1, pre_wr)
            .unwrap();
        // Submit Actor A's winning MintEpisode tx (index 1 in bundle.envelopes)
        let tx_a = &bundle.envelopes[1];
        let td_a = client.submit_tx(tx_a, 10).unwrap();
        assert_eq!(
            client.tx_record(&td_a).unwrap().status,
            ClientTxStatusV1::Submitted
        );
        client.mark_admitted(td_a).unwrap();
        assert_eq!(
            client.tx_record(&td_a).unwrap().status,
            ClientTxStatusV1::Admitted
        );

        let cert = cluster
            .run_round_with_subset(&bundle, 0, &[0, 1, 2, 3])
            .unwrap();
        let rcpt = cluster.nodes[0].finalized_receipt_at(1).unwrap().clone();
        let coords = [Coord::new(3, 0, 0), Coord::new(4, 0, 0)];
        let (bals, eps) = snapshot_cluster_finalized_assets(&cluster, &coords);

        let rolled_back = client
            .apply_finalized_commit(&cert, Some(&bundle), Some(&rcpt), bals, eps, &[0, 1])
            .unwrap();
        assert!(!rolled_back);
        assert_eq!(
            client.tx_record(&td_a).unwrap().status,
            ClientTxStatusV1::Finalized
        );

        let view = client.verified_state_view();
        let lrcpt = client.tx_lifecycle_receipt(td_a).unwrap();
        assert_eq!(
            ClientVerifiedStateViewV1::decode_canonical(&view.encode_canonical().unwrap()).unwrap(),
            view
        );
        assert_eq!(
            ClientTxLifecycleReceiptV1::decode_canonical(&lrcpt.encode_canonical().unwrap())
                .unwrap(),
            lrcpt
        );
        let view_tag = view.tag().unwrap().to_string();
        let lrcpt_tag = lrcpt.tag().unwrap().to_string();
        assert_eq!(
            view_tag,
            "#bb.receipt.sha256.532kser2hxodz65nosyv557zpcqdnlhly2orxsyvnmzu45xr5l5q"
        );
        assert_eq!(
            lrcpt_tag,
            "#bb.receipt.sha256.vqjs76elssntzogtqd4ir6xtydoj7ctzmi7odas7pjq3562ayjvq"
        );
    }
}
