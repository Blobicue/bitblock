//! `EXECUTOR-1`: primary launch-profile transition executor composing `COMMIT-1` and Waves 13–15
//! authenticated state producers (`AUTH-COMMIT-1`, `ASSET-1`, `GOVERNANCE-1`, `ECONOMIC-STATE-1`,
//! and `VALIDATOR-SCHEDULE-1`) behind `TRANSITION-SPEC-1`, `BOUNDS-1`, and `RESOURCE-1` (`PS-065`).
//!
//! Designed specifically for true cross-implementation differential verification against the
//! clean-room pure-Python independent executor in `tools/executor_1_vectors.py` (zero shared
//! authority-path code):
//! - **Exact launch-profile closure:** enforces `ResolvedProfileTransitionSpecV1`'s disjoint
//!   16-tag bands (`0..=15` Base, `16..=31` Auth/Nonce, `32..=47` Asset/Resource, `48..=63`
//!   Validator/Schedule, `64..=79` Gov/Upgrade), rejects the INACTIVE `Program/VM` band
//!   (`80..=95`) fail-closed with zero mutation, rejects unassigned tags (`96..=255`), and refuses
//!   unresolved predicates or mismatched fork contexts.
//! - **Atomic step staging & rollback:** every operation executes against a staged clone of the
//!   affected domain state(s); any validation, authorization, conservation, bedrock, Safe-Halt, or
//!   bounds failure leaves every domain root byte-identical to its pre-step snapshot.
//! - **Root-composer domain-registry output:** emits [`ExecutorDomainSnapshotV1`] carrying all 10
//!   active launch domain roots (`World`, `ActiveToken`, `Episode`, `Authorization`, `Nonce`,
//!   `NativeBalance`, `EpisodeOwnership`, `GovernanceState`, `EconomicStake`, `ValidatorSchedule`)
//!   plus explicit `program_state_active = false` for downstream `EXECUTION-ROOT-1` and
//!   `GENESIS-ROOT-PARITY-1`.
//! - **Powered semantic mutant verification:** [`verify_executor_receipt_against_spec`] rejects
//!   omitted step effects, partial-write rollback leaks, uncharged admitted gas, cross-fork
//!   replay, inactive-extension reachability, and mismatched domain snapshots.
//!
//! An independent executor detects implementation divergence under the pinned specification; two
//! wrappers around the same reducer are not independent, and execution alone is not consensus,
//! data availability, or networking.

use std::collections::{BTreeSet, HashSet};
use std::fmt;

use sha2::{Digest, Sha256};

use crate::asset_1::{AssetDispositionV1, AssetLedgerStateV1, AssetOperationV1};
use crate::asset_d1::{AssetPolicyDescriptorV1, ValidatorEconPolicyV1};
use crate::auth_commit_1::AuthCommitStateV1;
use crate::bounds_1::{ProtocolBoundsProfileV1, SafeHaltEvidenceV1};
use crate::commit_state::{
    DomainRegistryV1, LocalAuthenticatedSidecar, ProfileRegistryV1, verify_independent_projection,
};
use crate::consensus_gov_d1::{ConsensusPolicyDescriptorV1, GovernancePolicyDescriptorV1};
use crate::coordinate_episode::OccupancyEpisodeLedger;
use crate::crypto_d1::{CanonicalEd25519PublicKeyV1, SigningDomainTagV1};
use crate::economic_state_1::{EconomicDispositionV1, EconomicStateV1};
use crate::governance_1::{
    GovernanceContextV1, GovernanceDispositionV1, GovernanceLifecycleCodeV1, GovernanceStateV1,
};
use crate::history::{History, NoOpReason, TransitionEffect};
use crate::history_store::HistoryReplayLimits;
use crate::identity_1::{ActorAuthStateV1, IdentityContextV1};
use crate::identity_d1::{ActorIdV1, NetworkEditOpV1, TransactionBodyV1};
use crate::local_bounds::LocalStorageProfile;
use crate::program::{Edit, Operation};
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::resource_1::ResourcePolicyV1;
use crate::transition_spec_1::ResolvedProfileTransitionSpecV1;
use crate::validator_schedule_1::{ValidatorKeyPairV1, ValidatorScheduleStateV1};
use crate::world::{Coord, World, is_bedrock};

/// Canonical schema string for an [`ExecutorBlockBatchV1`] input carrier (`EXECUTOR-1`).
pub const EXECUTOR_1_BATCH_SCHEMA: &[u8] = b"bitblock/executor-1/block-batch/v1";
/// Canonical schema string for an [`ExecutorBlockReceiptV1`] execution receipt (`EXECUTOR-1`).
pub const EXECUTOR_1_RECEIPT_SCHEMA: &[u8] = b"bitblock/executor-1/execution-receipt/v1";

/// Canonical version identifier for `EXECUTOR-1` batches, snapshots, and receipts.
pub const EXECUTOR_1_VERSION: u16 = 1;

/// Base band (`0..=15`) operation tag: place one non-bedrock voxel.
pub const OP_BASE_PLACE_VOXEL: u8 = 1;
/// Base band (`0..=15`) operation tag: delete one non-bedrock voxel.
pub const OP_BASE_DELETE_VOXEL: u8 = 2;
/// Auth/Nonce band (`16..=31`) operation tag: commit one signed `IDENTITY-1` transaction.
pub const OP_AUTH_COMMIT_TX: u8 = 16;
/// Asset/Resource band (`32..=47`) operation tag: fungible native transfer.
pub const OP_ASSET_TRANSFER: u8 = 32;
/// Asset/Resource band (`32..=47`) operation tag: occupancy-episode mint & refundable deposit lock.
pub const OP_ASSET_MINT_DEPOSIT: u8 = 33;
/// Asset/Resource band (`32..=47`) operation tag: occupancy-episode burn & deposit refund.
pub const OP_ASSET_BURN_REFUND: u8 = 34;
/// Validator/Schedule band (`48..=63`) operation tag: bond additional validator stake.
pub const OP_ECON_BOND_STAKE: u8 = 48;
/// Validator/Schedule band (`48..=63`) operation tag: 100% slash-to-burn equivocation fault.
pub const OP_ECON_SLASH_EQUIVOCATION: u8 = 49;
/// Validator/Schedule band (`48..=63`) operation tag: derive & commit delayed `source_epoch + 2` schedule.
pub const OP_SCHEDULE_EPOCH: u8 = 50;
/// Gov/Upgrade band (`64..=79`) operation tag: advance governance epoch.
pub const OP_GOV_ADVANCE_EPOCH: u8 = 64;
/// Gov/Upgrade band (`64..=79`) operation tag: automatic deterministic Safe-Halt from `BOUNDS-1` evidence.
pub const OP_GOV_SAFE_HALT: u8 = 65;

const STEP_EFFECT_PREFIX: &[u8] = b"bb.exec1.step.v1";
const OUTCOMES_DIGEST_PREFIX: &[u8] = b"bb.exec1.outcomes.v1";

/// Fail-closed validation, codec, preflight, or mutant-verification error for `EXECUTOR-1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Executor1Error {
    /// Wire buffer had invalid version, length, discriminant, or trailing bytes.
    InvalidWire(&'static str),
    /// Launch executor refused a misconfigured or unresolved profile specification.
    InvalidLaunchConfig(&'static str),
    /// Batch exceeded `BOUNDS-1` / `TRANSITION-SPEC-1` op or byte limits before execution.
    PreflightOverBounds(&'static str),
    /// Batch fork context disagreed with the executor's bound chain/genesis/profile/spec context.
    ForkContextMismatch(&'static str),
    /// Batch height or epoch was non-monotonic relative to executor state.
    NonMonotonicBatch(&'static str),
    /// Upstream state producer returned an internal invariant or rebuild-parity error.
    DomainInvariantFailure(String),
    /// Powered semantic mutant verification caught a receipt invariant breach.
    SemanticMutantDetected(&'static str),
}

impl fmt::Display for Executor1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Executor1Error {}

/// Fork-binding execution context preventing cross-chain, cross-genesis, or cross-profile replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ExecutorForkContextV1 {
    /// Chain identifier (`> 0`).
    pub chain_id: u64,
    /// Single `i64³` world identifier (`> 0`).
    pub world_id: u64,
    /// Bound `GENESIS-D1` genesis identifier.
    pub genesis_id: [u8; 32],
    /// Bound `PROFILE-D1` profile identifier.
    pub profile_id: [u8; 32],
    /// SHA-256 digest of the exact governing [`ResolvedProfileTransitionSpecV1`] policy tag.
    pub transition_spec_digest: [u8; 32],
}

impl ExecutorForkContextV1 {
    /// Fixed canonical big-endian wire length (`112` bytes).
    pub const WIRE_LEN: usize = 112;

    /// Encodes this fork context into 112 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> [u8; Self::WIRE_LEN] {
        let mut buf = Vec::with_capacity(Self::WIRE_LEN);
        buf.extend_from_slice(&self.chain_id.to_be_bytes());
        buf.extend_from_slice(&self.world_id.to_be_bytes());
        buf.extend_from_slice(&self.genesis_id);
        buf.extend_from_slice(&self.profile_id);
        buf.extend_from_slice(&self.transition_spec_digest);
        buf.try_into().expect("fixed ExecutorForkContextV1 length")
    }

    /// Strictly decodes 112 canonical big-endian wire bytes into an [`ExecutorForkContextV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Executor1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(Executor1Error::InvalidWire(
                "wrong ExecutorForkContextV1 len",
            ));
        }
        let chain_id = read_u64_be(bytes, 0)?;
        let world_id = read_u64_be(bytes, 8)?;
        if chain_id == 0 || world_id == 0 {
            return Err(Executor1Error::InvalidWire("zero chain_id or world_id"));
        }
        Ok(Self {
            chain_id,
            world_id,
            genesis_id: read_arr32(bytes, 16)?,
            profile_id: read_arr32(bytes, 48)?,
            transition_spec_digest: read_arr32(bytes, 80)?,
        })
    }
}

/// Complete 10-domain active root snapshot consumed by `EXECUTION-ROOT-1` and `GENESIS-ROOT-PARITY-1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ExecutorDomainSnapshotV1 {
    /// Snapshot version (`1`).
    pub snapshot_version: u16,
    /// Block batch height at which these roots were captured.
    pub batch_height: u64,
    /// Active protocol epoch at which these roots were captured.
    pub epoch: u64,
    /// `COMMIT-1` `WorldRootV1::root` (192-bit voxel occupancy Patricia root).
    pub world_root: [u8; 32],
    /// `COMMIT-1` `ActiveTokenRootV1::root` (192-bit active mint provenance Patricia root).
    pub active_token_root: [u8; 32],
    /// `COMMIT-1` `EpisodeRootV1::root` (256-bit occupancy episode Patricia root).
    pub episode_root: [u8; 32],
    /// `AUTH-COMMIT-1` Domain 1 `authorization_root`.
    pub auth_root: [u8; 32],
    /// `AUTH-COMMIT-1` Domain 2 `nonce_root`.
    pub nonce_root: [u8; 32],
    /// `ASSET-1` Domain 3 `native_balance_root`.
    pub native_balance_root: [u8; 32],
    /// `ASSET-1` Domain 4 `episode_ownership_root`.
    pub episode_ownership_root: [u8; 32],
    /// `GOVERNANCE-1` Domain 5 `governance_trie_root`.
    pub governance_root: [u8; 32],
    /// `ECONOMIC-STATE-1` Domain 6 `stake_trie_root`.
    pub economic_stake_root: [u8; 32],
    /// `VALIDATOR-SCHEDULE-1` Domain 7 `schedule_trie_root`.
    pub validator_schedule_root: [u8; 32],
    /// `false` on launch profile (inactive sentinel slot); `true` only after `PROGRAM-STATE-1`.
    pub program_state_active: bool,
}

impl ExecutorDomainSnapshotV1 {
    /// Fixed canonical big-endian wire length (`339` bytes).
    pub const WIRE_LEN: usize = 339;

    /// Encodes all 10 domain roots and metadata into 339 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], Executor1Error> {
        if self.snapshot_version != EXECUTOR_1_VERSION {
            return Err(Executor1Error::InvalidWire("bad snapshot_version"));
        }
        let mut buf = Vec::with_capacity(Self::WIRE_LEN);
        buf.extend_from_slice(&self.snapshot_version.to_be_bytes());
        buf.extend_from_slice(&self.batch_height.to_be_bytes());
        buf.extend_from_slice(&self.epoch.to_be_bytes());
        for root in self.active_root_array() {
            buf.extend_from_slice(&root);
        }
        buf.push(u8::from(self.program_state_active));
        Ok(buf.try_into().expect("fixed ExecutorDomainSnapshotV1 len"))
    }

    /// Strictly decodes 339 canonical big-endian wire bytes into an [`ExecutorDomainSnapshotV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Executor1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(Executor1Error::InvalidWire("wrong snapshot len"));
        }
        let snapshot_version = read_u16_be(bytes, 0)?;
        if snapshot_version != EXECUTOR_1_VERSION {
            return Err(Executor1Error::InvalidWire("bad snapshot_version"));
        }
        let program_state_active = match bytes[338] {
            0 => false,
            1 => true,
            _ => {
                return Err(Executor1Error::InvalidWire(
                    "non-boolean program_state_active",
                ));
            }
        };
        Ok(Self {
            snapshot_version,
            batch_height: read_u64_be(bytes, 2)?,
            epoch: read_u64_be(bytes, 10)?,
            world_root: read_arr32(bytes, 18)?,
            active_token_root: read_arr32(bytes, 50)?,
            episode_root: read_arr32(bytes, 82)?,
            auth_root: read_arr32(bytes, 114)?,
            nonce_root: read_arr32(bytes, 146)?,
            native_balance_root: read_arr32(bytes, 178)?,
            episode_ownership_root: read_arr32(bytes, 210)?,
            governance_root: read_arr32(bytes, 242)?,
            economic_stake_root: read_arr32(bytes, 274)?,
            validator_schedule_root: read_arr32(bytes, 306)?,
            program_state_active,
        })
    }

    /// Returns all 10 active launch domain root digests in canonical domain order.
    pub const fn active_root_array(&self) -> [[u8; 32]; 10] {
        [
            self.world_root,
            self.active_token_root,
            self.episode_root,
            self.auth_root,
            self.nonce_root,
            self.native_balance_root,
            self.episode_ownership_root,
            self.governance_root,
            self.economic_stake_root,
            self.validator_schedule_root,
        ]
    }

    /// Computes SHA-256 over the canonical 339-byte snapshot encoding.
    pub fn canonical_sha256(&self) -> Result<[u8; 32], Executor1Error> {
        let bytes = self.encode_canonical()?;
        Ok(Sha256::digest(bytes).into())
    }

    /// Returns `true` iff all 10 domain roots and `program_state_active` match `other`.
    pub fn roots_equal(&self, other: &Self) -> bool {
        self.active_root_array() == other.active_root_array()
            && self.program_state_active == other.program_state_active
    }
}

/// Deterministic per-step disposition emitted by `EXECUTOR-1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ExecutorDispositionV1 {
    /// Operation validated, metered, and atomically committed across its declared write domains.
    Admitted = 0,
    /// Operation tag targeted an INACTIVE extension band (`80..=95` on Launch) — atomic rollback.
    RejectedInactiveExtensionBand = 1,
    /// Operation tag fell in unassigned `96..=255` or had no handler in its band — atomic rollback.
    RejectedUnknownOperationTag = 2,
    /// Operation payload had invalid byte length or malformed fields — atomic rollback.
    RejectedMalformedPayload = 3,
    /// Base occupancy edit targeted immutable bedrock `(0,0,0)` — atomic rollback.
    RejectedBedrockImmutable = 4,
    /// Base occupancy edit was a no-op (already on/off) — atomic rollback.
    RejectedNoOpOrConflict = 5,
    /// `IDENTITY-1` / `AUTH-COMMIT-1` signature, nonce, or policy check failed — atomic rollback.
    RejectedAuthorizationFailed = 6,
    /// `ASSET-1` conservation, balance, ownership, or bedrock check failed — atomic rollback.
    RejectedAssetInvariant = 7,
    /// `ECONOMIC-STATE-1` or `VALIDATOR-SCHEDULE-1` stake, delay, or Safe-Halt check failed.
    RejectedValidatorOrSchedule = 8,
    /// `GOVERNANCE-1` epoch, fault evidence, or Safe-Halt check failed — atomic rollback.
    RejectedGovernanceFailed = 9,
    /// Cumulative batch gas exceeded `spec.max_batch_gas` — atomic rollback.
    RejectedGasLimitExceeded = 10,
}

impl ExecutorDispositionV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte disposition discriminant.
    pub const fn from_code(code: u8) -> Result<Self, Executor1Error> {
        match code {
            0 => Ok(Self::Admitted),
            1 => Ok(Self::RejectedInactiveExtensionBand),
            2 => Ok(Self::RejectedUnknownOperationTag),
            3 => Ok(Self::RejectedMalformedPayload),
            4 => Ok(Self::RejectedBedrockImmutable),
            5 => Ok(Self::RejectedNoOpOrConflict),
            6 => Ok(Self::RejectedAuthorizationFailed),
            7 => Ok(Self::RejectedAssetInvariant),
            8 => Ok(Self::RejectedValidatorOrSchedule),
            9 => Ok(Self::RejectedGovernanceFailed),
            10 => Ok(Self::RejectedGasLimitExceeded),
            _ => Err(Executor1Error::InvalidWire("unknown ExecutorDispositionV1")),
        }
    }
}

/// One framed, tagged operation inside an [`ExecutorBlockBatchV1`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExecutorOpV1 {
    /// Disjoint 16-tag band operation identifier (`0..=255`).
    pub op_tag: u8,
    /// Canonical big-endian operation payload bytes.
    pub payload: Vec<u8>,
}

impl ExecutorOpV1 {
    /// Builds a Band 0 (`OP_BASE_PLACE_VOXEL`) operation placing one non-bedrock voxel.
    pub fn base_place_voxel(coord: Coord) -> Self {
        Self {
            op_tag: OP_BASE_PLACE_VOXEL,
            payload: encode_coord_be(coord).to_vec(),
        }
    }

    /// Builds a Band 0 (`OP_BASE_DELETE_VOXEL`) operation deleting one non-bedrock voxel.
    pub fn base_delete_voxel(coord: Coord) -> Self {
        Self {
            op_tag: OP_BASE_DELETE_VOXEL,
            payload: encode_coord_be(coord).to_vec(),
        }
    }

    /// Builds a Band 1 (`OP_AUTH_COMMIT_TX`) operation committing a signed `IDENTITY-1` tx.
    #[allow(clippy::too_many_arguments)]
    pub fn auth_commit_tx(
        actor_id: ActorIdV1,
        signer_actor_id: ActorIdV1,
        nonce: u64,
        valid_after: u64,
        valid_until: u64,
        max_fee: u128,
        coord: Coord,
        pubkey: [u8; 32],
        sig: [u8; 64],
    ) -> Self {
        let mut p = Vec::with_capacity(224);
        p.extend_from_slice(actor_id.as_bytes());
        p.extend_from_slice(signer_actor_id.as_bytes());
        p.extend_from_slice(&nonce.to_be_bytes());
        p.extend_from_slice(&valid_after.to_be_bytes());
        p.extend_from_slice(&valid_until.to_be_bytes());
        p.extend_from_slice(&max_fee.to_be_bytes());
        p.extend_from_slice(&encode_coord_be(coord));
        p.extend_from_slice(&pubkey);
        p.extend_from_slice(&sig);
        Self {
            op_tag: OP_AUTH_COMMIT_TX,
            payload: p,
        }
    }

    /// Builds a Band 2 (`OP_ASSET_TRANSFER`) operation transferring native liquid units.
    pub fn asset_transfer(from: ActorIdV1, to: ActorIdV1, amount: u128) -> Self {
        let mut p = Vec::with_capacity(80);
        p.extend_from_slice(from.as_bytes());
        p.extend_from_slice(to.as_bytes());
        p.extend_from_slice(&amount.to_be_bytes());
        Self {
            op_tag: OP_ASSET_TRANSFER,
            payload: p,
        }
    }

    /// Builds a Band 2 (`OP_ASSET_MINT_DEPOSIT`) operation locking an occupancy deposit.
    pub fn asset_mint_deposit(actor: ActorIdV1, coord: Coord, mint_episode_id: [u8; 32]) -> Self {
        let mut p = Vec::with_capacity(88);
        p.extend_from_slice(actor.as_bytes());
        p.extend_from_slice(&encode_coord_be(coord));
        p.extend_from_slice(&mint_episode_id);
        Self {
            op_tag: OP_ASSET_MINT_DEPOSIT,
            payload: p,
        }
    }

    /// Builds a Band 2 (`OP_ASSET_BURN_REFUND`) operation refunding an occupancy deposit.
    pub fn asset_burn_refund(actor: ActorIdV1, coord: Coord) -> Self {
        let mut p = Vec::with_capacity(56);
        p.extend_from_slice(actor.as_bytes());
        p.extend_from_slice(&encode_coord_be(coord));
        Self {
            op_tag: OP_ASSET_BURN_REFUND,
            payload: p,
        }
    }

    /// Builds a Band 3 (`OP_ECON_BOND_STAKE`) operation bonding validator stake.
    pub fn econ_bond_stake(validator_id: ActorIdV1, amount: u128) -> Self {
        let mut p = Vec::with_capacity(48);
        p.extend_from_slice(validator_id.as_bytes());
        p.extend_from_slice(&amount.to_be_bytes());
        Self {
            op_tag: OP_ECON_BOND_STAKE,
            payload: p,
        }
    }

    /// Builds a Band 3 (`OP_ECON_SLASH_EQUIVOCATION`) operation applying 100% slash-to-burn.
    pub fn econ_slash_equivocation(validator_id: ActorIdV1, evidence_digest: [u8; 32]) -> Self {
        let mut p = Vec::with_capacity(64);
        p.extend_from_slice(validator_id.as_bytes());
        p.extend_from_slice(&evidence_digest);
        Self {
            op_tag: OP_ECON_SLASH_EQUIVOCATION,
            payload: p,
        }
    }

    /// Builds a Band 3 (`OP_SCHEDULE_EPOCH`) operation deriving the delayed `source_epoch + 2` schedule.
    pub fn schedule_epoch(source_epoch: u64) -> Self {
        Self {
            op_tag: OP_SCHEDULE_EPOCH,
            payload: source_epoch.to_be_bytes().to_vec(),
        }
    }

    /// Builds a Band 4 (`OP_GOV_ADVANCE_EPOCH`) operation advancing governance epoch.
    pub fn gov_advance_epoch(next_epoch: u64) -> Self {
        Self {
            op_tag: OP_GOV_ADVANCE_EPOCH,
            payload: next_epoch.to_be_bytes().to_vec(),
        }
    }

    /// Builds a Band 4 (`OP_GOV_SAFE_HALT`) operation triggering automatic deterministic Safe-Halt.
    pub fn gov_safe_halt(
        artifact_digest: [u8; 32],
        violation_code: u8,
        observed_value: u64,
        committed_limit: u64,
    ) -> Self {
        let mut p = Vec::with_capacity(49);
        p.extend_from_slice(&artifact_digest);
        p.push(violation_code);
        p.extend_from_slice(&observed_value.to_be_bytes());
        p.extend_from_slice(&committed_limit.to_be_bytes());
        Self {
            op_tag: OP_GOV_SAFE_HALT,
            payload: p,
        }
    }
}

/// Canonical serialized block batch input consumed identically by Rust and clean-room Python executors.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExecutorBlockBatchV1 {
    /// Batch schema version (`1`).
    pub batch_version: u16,
    /// Monotonic execution batch height (`>= 1`).
    pub batch_height: u64,
    /// Active protocol epoch for this batch.
    pub epoch: u64,
    /// Governing fork context (`chain_id`, `world_id`, `genesis_id`, `profile_id`, `spec_digest`).
    pub fork_context: ExecutorForkContextV1,
    /// EIP-1559 base fee burned per charged gas unit (`RESOURCE-1`).
    pub base_fee_per_gas: u64,
    /// Ordered sequence of framed band operations.
    pub ops: Vec<ExecutorOpV1>,
}

impl ExecutorBlockBatchV1 {
    /// Encodes this batch into canonical big-endian wire bytes (`bitblock/executor-1/block-batch/v1`).
    pub fn encode_canonical(&self) -> Result<Vec<u8>, Executor1Error> {
        if self.batch_version != EXECUTOR_1_VERSION {
            return Err(Executor1Error::InvalidWire("bad batch_version"));
        }
        let op_count = u32::try_from(self.ops.len())
            .map_err(|_| Executor1Error::InvalidWire("op count overflow"))?;
        let mut out = Vec::with_capacity(142 + self.ops.len() * 64);
        out.extend_from_slice(&self.batch_version.to_be_bytes());
        out.extend_from_slice(&self.batch_height.to_be_bytes());
        out.extend_from_slice(&self.epoch.to_be_bytes());
        out.extend_from_slice(&self.fork_context.encode_canonical());
        out.extend_from_slice(&self.base_fee_per_gas.to_be_bytes());
        out.extend_from_slice(&op_count.to_be_bytes());
        for op in &self.ops {
            let plen = u16::try_from(op.payload.len())
                .map_err(|_| Executor1Error::InvalidWire("op payload > u16::MAX"))?;
            out.push(op.op_tag);
            out.extend_from_slice(&plen.to_be_bytes());
            out.extend_from_slice(&op.payload);
        }
        Ok(out)
    }

    /// Strictly decodes canonical big-endian wire bytes into an [`ExecutorBlockBatchV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Executor1Error> {
        if bytes.len() < 142 {
            return Err(Executor1Error::InvalidWire(
                "truncated ExecutorBlockBatchV1",
            ));
        }
        let batch_version = read_u16_be(bytes, 0)?;
        if batch_version != EXECUTOR_1_VERSION {
            return Err(Executor1Error::InvalidWire("bad batch_version"));
        }
        let batch_height = read_u64_be(bytes, 2)?;
        let epoch = read_u64_be(bytes, 10)?;
        let fork_context = ExecutorForkContextV1::decode_canonical(&bytes[18..130])?;
        let base_fee_per_gas = read_u64_be(bytes, 130)?;
        let op_count = read_u32_be(bytes, 138)? as usize;
        let mut cur = 142usize;
        let mut ops = Vec::with_capacity(op_count.min(1024));
        for _ in 0..op_count {
            if bytes.len().saturating_sub(cur) < 3 {
                return Err(Executor1Error::InvalidWire("truncated op header"));
            }
            let op_tag = bytes[cur];
            let plen = read_u16_be(bytes, cur + 1)? as usize;
            cur += 3;
            if bytes.len().saturating_sub(cur) < plen {
                return Err(Executor1Error::InvalidWire("truncated op payload"));
            }
            ops.push(ExecutorOpV1 {
                op_tag,
                payload: bytes[cur..cur + plen].to_vec(),
            });
            cur += plen;
        }
        if cur != bytes.len() {
            return Err(Executor1Error::InvalidWire("trailing bytes in batch"));
        }
        Ok(Self {
            batch_version,
            batch_height,
            epoch,
            fork_context,
            base_fee_per_gas,
            ops,
        })
    }

    /// Computes the canonical `Receipt` `ContentTag` for this block batch.
    pub fn tag(&self) -> Result<ContentTag, Executor1Error> {
        let bytes = self.encode_canonical()?;
        hash_canonical(
            TagKind::Receipt,
            EXECUTOR_1_BATCH_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .map_err(|_| Executor1Error::InvalidWire("hash_canonical failed"))
    }
}

/// Deterministic ordered outcome emitted for one step inside [`ExecutorBlockReceiptV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ExecutorStepOutcomeV1 {
    /// Zero-based step ordinal within the batch.
    pub op_index: u32,
    /// Dispatched operation tag (`0..=255`).
    pub op_tag: u8,
    /// Owning `TRANSITION-SPEC-1` band slot (`0` = base, `1..=5` = extension, `0xFFFF` = unknown).
    pub band_slot_id: u16,
    /// Step disposition (`Admitted` or typed atomic-rollback rejection).
    pub disposition: ExecutorDispositionV1,
    /// Deterministic gas charged for this step (`0` on rejected inactive/unknown/over-gas steps).
    pub gas_charged: u64,
    /// Base fee burned (`gas_charged * base_fee_per_gas`).
    pub fee_burned: u128,
    /// Exclusive `TRANSITION-SPEC-1` write-domain bitmask mutated (`0` on any rejection).
    pub write_domains_mask: u32,
    /// SHA-256 commitment binding step fields and post-step snapshot digest.
    pub effect_digest: [u8; 32],
}

impl ExecutorStepOutcomeV1 {
    /// Fixed canonical big-endian wire length (`68` bytes).
    pub const WIRE_LEN: usize = 68;

    /// Encodes this step outcome into 68 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> [u8; Self::WIRE_LEN] {
        let mut buf = Vec::with_capacity(Self::WIRE_LEN);
        buf.extend_from_slice(&self.op_index.to_be_bytes());
        buf.push(self.op_tag);
        buf.extend_from_slice(&self.band_slot_id.to_be_bytes());
        buf.push(self.disposition.code());
        buf.extend_from_slice(&self.gas_charged.to_be_bytes());
        buf.extend_from_slice(&self.fee_burned.to_be_bytes());
        buf.extend_from_slice(&self.write_domains_mask.to_be_bytes());
        buf.extend_from_slice(&self.effect_digest);
        buf.try_into().expect("fixed ExecutorStepOutcomeV1 len")
    }

    /// Strictly decodes 68 canonical big-endian wire bytes into an [`ExecutorStepOutcomeV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Executor1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(Executor1Error::InvalidWire("wrong step outcome len"));
        }
        Ok(Self {
            op_index: read_u32_be(bytes, 0)?,
            op_tag: bytes[4],
            band_slot_id: read_u16_be(bytes, 5)?,
            disposition: ExecutorDispositionV1::from_code(bytes[7])?,
            gas_charged: read_u64_be(bytes, 8)?,
            fee_burned: read_u128_be(bytes, 16)?,
            write_domains_mask: read_u32_be(bytes, 32)?,
            effect_digest: read_arr32(bytes, 36)?,
        })
    }
}

/// Complete deterministic block execution receipt (`bitblock/executor-1/execution-receipt/v1`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExecutorBlockReceiptV1 {
    /// Receipt schema version (`1`).
    pub receipt_version: u16,
    /// Digest of the executed [`ExecutorBlockBatchV1`] `ContentTag`.
    pub batch_digest: [u8; 32],
    /// Fork context under which the batch executed.
    pub fork_context: ExecutorForkContextV1,
    /// Pre-batch 10-domain root snapshot.
    pub pre_snapshot: ExecutorDomainSnapshotV1,
    /// Post-batch 10-domain root snapshot (consumed by `EXECUTION-ROOT-1`).
    pub post_snapshot: ExecutorDomainSnapshotV1,
    /// Total admitted steps in this batch.
    pub admitted_count: u32,
    /// Total rejected steps in this batch (all atomically rolled back).
    pub rejected_count: u32,
    /// Cumulative deterministic gas charged across the batch.
    pub total_gas_charged: u64,
    /// Cumulative EIP-1559 base fee burned across the batch.
    pub total_fee_burned: u128,
    /// Union of write-domain bitmasks touched by admitted steps.
    pub cumulative_write_mask: u32,
    /// Deterministic SHA-256 digest over ordered step effect digests.
    pub outcomes_digest: [u8; 32],
    /// Complete ordered per-step outcome lane.
    pub step_outcomes: Vec<ExecutorStepOutcomeV1>,
}

impl ExecutorBlockReceiptV1 {
    /// Encodes this execution receipt into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, Executor1Error> {
        if self.receipt_version != EXECUTOR_1_VERSION {
            return Err(Executor1Error::InvalidWire("bad receipt_version"));
        }
        let step_count = u32::try_from(self.step_outcomes.len())
            .map_err(|_| Executor1Error::InvalidWire("step count overflow"))?;
        let mut out =
            Vec::with_capacity(896 + self.step_outcomes.len() * ExecutorStepOutcomeV1::WIRE_LEN);
        out.extend_from_slice(&self.receipt_version.to_be_bytes());
        out.extend_from_slice(&self.batch_digest);
        out.extend_from_slice(&self.fork_context.encode_canonical());
        out.extend_from_slice(&self.pre_snapshot.encode_canonical()?);
        out.extend_from_slice(&self.post_snapshot.encode_canonical()?);
        out.extend_from_slice(&self.admitted_count.to_be_bytes());
        out.extend_from_slice(&self.rejected_count.to_be_bytes());
        out.extend_from_slice(&self.total_gas_charged.to_be_bytes());
        out.extend_from_slice(&self.total_fee_burned.to_be_bytes());
        out.extend_from_slice(&self.cumulative_write_mask.to_be_bytes());
        out.extend_from_slice(&self.outcomes_digest);
        out.extend_from_slice(&step_count.to_be_bytes());
        for s in &self.step_outcomes {
            out.extend_from_slice(&s.encode_canonical());
        }
        Ok(out)
    }

    /// Strictly decodes canonical big-endian wire bytes into an [`ExecutorBlockReceiptV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Executor1Error> {
        const FIXED_HEAD: usize = 2 + 32 + 112 + 339 + 339 + 4 + 4 + 8 + 16 + 4 + 32 + 4;
        if bytes.len() < FIXED_HEAD {
            return Err(Executor1Error::InvalidWire(
                "truncated ExecutorBlockReceiptV1",
            ));
        }
        let receipt_version = read_u16_be(bytes, 0)?;
        if receipt_version != EXECUTOR_1_VERSION {
            return Err(Executor1Error::InvalidWire("bad receipt_version"));
        }
        let batch_digest = read_arr32(bytes, 2)?;
        let fork_context = ExecutorForkContextV1::decode_canonical(&bytes[34..146])?;
        let pre_snapshot = ExecutorDomainSnapshotV1::decode_canonical(&bytes[146..485])?;
        let post_snapshot = ExecutorDomainSnapshotV1::decode_canonical(&bytes[485..824])?;
        let admitted_count = read_u32_be(bytes, 824)?;
        let rejected_count = read_u32_be(bytes, 828)?;
        let total_gas_charged = read_u64_be(bytes, 832)?;
        let total_fee_burned = read_u128_be(bytes, 840)?;
        let cumulative_write_mask = read_u32_be(bytes, 856)?;
        let outcomes_digest = read_arr32(bytes, 860)?;
        let step_count = read_u32_be(bytes, 892)? as usize;
        let expected_len = FIXED_HEAD
            .checked_add(
                step_count
                    .checked_mul(ExecutorStepOutcomeV1::WIRE_LEN)
                    .ok_or(Executor1Error::InvalidWire("step len overflow"))?,
            )
            .ok_or(Executor1Error::InvalidWire("receipt len overflow"))?;
        if bytes.len() != expected_len {
            return Err(Executor1Error::InvalidWire("receipt length mismatch"));
        }
        let mut step_outcomes = Vec::with_capacity(step_count);
        let mut cur = FIXED_HEAD;
        for _ in 0..step_count {
            step_outcomes.push(ExecutorStepOutcomeV1::decode_canonical(
                &bytes[cur..cur + ExecutorStepOutcomeV1::WIRE_LEN],
            )?);
            cur += ExecutorStepOutcomeV1::WIRE_LEN;
        }
        Ok(Self {
            receipt_version,
            batch_digest,
            fork_context,
            pre_snapshot,
            post_snapshot,
            admitted_count,
            rejected_count,
            total_gas_charged,
            total_fee_burned,
            cumulative_write_mask,
            outcomes_digest,
            step_outcomes,
        })
    }

    /// Computes the canonical `Receipt` `ContentTag` for this block execution receipt.
    pub fn tag(&self) -> Result<ContentTag, Executor1Error> {
        let bytes = self.encode_canonical()?;
        hash_canonical(
            TagKind::Receipt,
            EXECUTOR_1_RECEIPT_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .map_err(|_| Executor1Error::InvalidWire("hash_canonical failed"))
    }
}

/// Genesis initialization bundle for [`PrimaryLaunchExecutorV1`].
#[derive(Clone, Debug)]
pub struct ExecutorGenesisConfigV1 {
    /// Governing fork context (`chain_id`, `world_id`, `genesis_id`, `profile_id`, `spec_digest`).
    pub fork_context: ExecutorForkContextV1,
    /// `PROFILE-SCOPE-D0` pre-ID scope digest bound into `COMMIT-1`.
    pub profile_scope_digest: [u8; 32],
    /// Resolved launch `TRANSITION-SPEC-1` specification (`program_slot_active` must be `false`).
    pub spec: ResolvedProfileTransitionSpecV1,
    /// Governing `BOUNDS-1` consumer-validator protocol bounds.
    pub bounds: ProtocolBoundsProfileV1,
    /// Governing `RESOURCE-1` fee policy.
    pub resource: ResourcePolicyV1,
    /// Governing `IDENTITY-1` context.
    pub auth_context: IdentityContextV1,
    /// Initial genesis actor auth states.
    pub initial_actors: Vec<ActorAuthStateV1>,
    /// Governing `ASSET-D1` policy.
    pub asset_policy: AssetPolicyDescriptorV1,
    /// Initial fungible liquid balances (`(actor_id, balance)`).
    pub initial_balances: Vec<(ActorIdV1, u128)>,
    /// Governing `GOVERNANCE-1` context.
    pub gov_context: GovernanceContextV1,
    /// Governing `GOVERNANCE-D1` policy.
    pub gov_policy: GovernancePolicyDescriptorV1,
    /// Initial governance parameter key/value pairs.
    pub initial_params: Vec<(u32, u64)>,
    /// Governing `VALIDATOR-ECON-D1` policy.
    pub econ_policy: ValidatorEconPolicyV1,
    /// Initial bonded validator stakes (`(validator_id, bonded_stake)`).
    pub initial_stakes: Vec<(ActorIdV1, u128)>,
    /// Governing `CONSENSUS-D1` policy.
    pub consensus_policy: ConsensusPolicyDescriptorV1,
    /// Initial separated vote & proposal keys per genesis validator.
    pub initial_validator_keys: Vec<(ActorIdV1, ValidatorKeyPairV1)>,
    /// Actor public keys forbidden from colliding with validator keys.
    pub forbidden_actor_keys: BTreeSet<CanonicalEd25519PublicKeyV1>,
}

/// Primary Rust launch-profile transition executor composing `COMMIT-1` and Waves 13–15 producers.
#[derive(Clone, Debug)]
pub struct PrimaryLaunchExecutorV1 {
    fork_context: ExecutorForkContextV1,
    spec: ResolvedProfileTransitionSpecV1,
    bounds: ProtocolBoundsProfileV1,
    resource: ResourcePolicyV1,
    bounds_profile_digest: [u8; 32],
    batch_height: u64,
    epoch: u64,
    initial_world: World,
    world: World,
    history: History,
    episodes: OccupancyEpisodeLedger,
    commit_sidecar: LocalAuthenticatedSidecar,
    auth: AuthCommitStateV1,
    asset: AssetLedgerStateV1,
    gov: GovernanceStateV1,
    econ: EconomicStateV1,
    schedule: ValidatorScheduleStateV1,
    forbidden_actor_keys: BTreeSet<CanonicalEd25519PublicKeyV1>,
}

impl PrimaryLaunchExecutorV1 {
    /// Initializes a deterministic launch-profile executor from `cfg`, refusing unresolved
    /// predicates, active `Program/VM` slots, or mismatched fork/spec digests.
    pub fn from_genesis(cfg: ExecutorGenesisConfigV1) -> Result<Self, Executor1Error> {
        cfg.spec
            .validate()
            .map_err(|_| Executor1Error::InvalidLaunchConfig("invalid TRANSITION-SPEC-1"))?;
        if cfg.spec.program_slot_active {
            return Err(Executor1Error::InvalidLaunchConfig(
                "launch executor forbids active Program/VM band",
            ));
        }
        let spec_digest = cfg
            .spec
            .tag()
            .map_err(|_| Executor1Error::InvalidLaunchConfig("cannot hash spec"))?
            .digest();
        if spec_digest != cfg.fork_context.transition_spec_digest
            || cfg.spec.profile_id != cfg.fork_context.profile_id
        {
            return Err(Executor1Error::InvalidLaunchConfig(
                "fork_context disagrees with ResolvedProfileTransitionSpecV1",
            ));
        }
        let bounds_profile_digest = cfg
            .bounds
            .tag()
            .map_err(|_| Executor1Error::InvalidLaunchConfig("invalid BOUNDS-1"))?
            .digest();
        if bounds_profile_digest != cfg.spec.bounds_profile_digest {
            return Err(Executor1Error::InvalidLaunchConfig(
                "BOUNDS-1 digest disagrees with transition spec",
            ));
        }
        let resource_digest = cfg
            .resource
            .tag()
            .map_err(|_| Executor1Error::InvalidLaunchConfig("invalid RESOURCE-1"))?
            .digest();
        if resource_digest != cfg.spec.resource_policy_digest {
            return Err(Executor1Error::InvalidLaunchConfig(
                "RESOURCE-1 digest disagrees with transition spec",
            ));
        }
        let initial_world = World::new();
        let history = History::new(&initial_world);
        let episodes = OccupancyEpisodeLedger::new(history.genesis());
        let commit_sidecar = LocalAuthenticatedSidecar::from_genesis(
            &initial_world,
            &history,
            &episodes,
            &LocalStorageProfile::DEFAULT,
            cfg.profile_scope_digest,
        )
        .map_err(|e| Executor1Error::DomainInvariantFailure(e.to_string()))?;
        let auth = AuthCommitStateV1::from_genesis(cfg.auth_context, cfg.initial_actors)
            .map_err(|e| Executor1Error::DomainInvariantFailure(e.to_string()))?;
        let asset = AssetLedgerStateV1::from_genesis(
            cfg.asset_policy,
            cfg.fork_context.profile_id,
            &cfg.initial_balances,
        )
        .map_err(|e| Executor1Error::DomainInvariantFailure(e.to_string()))?;
        let gov =
            GovernanceStateV1::from_genesis(cfg.gov_context, cfg.gov_policy, &cfg.initial_params)
                .map_err(|e| Executor1Error::DomainInvariantFailure(e.to_string()))?;
        let econ = EconomicStateV1::from_genesis(
            cfg.econ_policy,
            cfg.fork_context.profile_id,
            &cfg.initial_stakes,
        )
        .map_err(|e| Executor1Error::DomainInvariantFailure(e.to_string()))?;
        let schedule = ValidatorScheduleStateV1::from_genesis(
            cfg.consensus_policy,
            cfg.econ_policy,
            cfg.fork_context.profile_id,
            &econ,
            &gov,
            &cfg.initial_validator_keys,
            &cfg.forbidden_actor_keys,
        )
        .map_err(|e| Executor1Error::DomainInvariantFailure(e.to_string()))?;
        Ok(Self {
            fork_context: cfg.fork_context,
            spec: cfg.spec,
            bounds: cfg.bounds,
            resource: cfg.resource,
            bounds_profile_digest,
            batch_height: 0,
            epoch: 0,
            world: initial_world.clone(),
            initial_world,
            history,
            episodes,
            commit_sidecar,
            auth,
            asset,
            gov,
            econ,
            schedule,
            forbidden_actor_keys: cfg.forbidden_actor_keys,
        })
    }

    /// Returns the bound fork context.
    pub const fn fork_context(&self) -> ExecutorForkContextV1 {
        self.fork_context
    }

    /// Returns the resolved `TRANSITION-SPEC-1` governing this executor.
    pub const fn spec(&self) -> &ResolvedProfileTransitionSpecV1 {
        &self.spec
    }

    /// Returns the current 10-domain authenticated root snapshot for `EXECUTION-ROOT-1`.
    pub fn snapshot(&self) -> Result<ExecutorDomainSnapshotV1, Executor1Error> {
        let auth_roots = self.auth.roots();
        let asset_roots = self
            .asset
            .roots()
            .map_err(|e| Executor1Error::DomainInvariantFailure(e.to_string()))?;
        let gov_root = self.gov.root_commitment();
        let econ_root = self
            .econ
            .root_commitment()
            .map_err(|e| Executor1Error::DomainInvariantFailure(e.to_string()))?;
        let sched_root = self
            .schedule
            .root_commitment()
            .map_err(|e| Executor1Error::DomainInvariantFailure(e.to_string()))?;
        Ok(ExecutorDomainSnapshotV1 {
            snapshot_version: EXECUTOR_1_VERSION,
            batch_height: self.batch_height,
            epoch: self.epoch,
            world_root: self.commit_sidecar.world_root().root,
            active_token_root: self.commit_sidecar.active_token_root().root,
            episode_root: self.commit_sidecar.episode_root().root,
            auth_root: auth_roots.authorization_root,
            nonce_root: auth_roots.nonce_root,
            native_balance_root: asset_roots.native_balance_root,
            episode_ownership_root: asset_roots.episode_ownership_root,
            governance_root: gov_root.governance_trie_root,
            economic_stake_root: econ_root.economic_trie_root,
            validator_schedule_root: sched_root.schedule_trie_root,
            program_state_active: self.spec.program_slot_active,
        })
    }

    /// Verifies cold rebuild parity and conservation across every active launch domain.
    pub fn verify_all_domain_rebuild_parity(&self) -> Result<(), Executor1Error> {
        let empty_pruned = HashSet::new();
        verify_independent_projection(
            &self.initial_world,
            &self.history,
            &empty_pruned,
            self.commit_sidecar.current_set(),
            &ProfileRegistryV1::local_v1(),
            &DomainRegistryV1::local_v1(),
            HistoryReplayLimits::DEFAULT,
        )
        .map_err(|e| Executor1Error::DomainInvariantFailure(e.to_string()))?;
        self.auth
            .verify_rebuild_parity()
            .map_err(|e| Executor1Error::DomainInvariantFailure(e.to_string()))?;
        self.asset
            .verify_rebuild_parity()
            .map_err(|e| Executor1Error::DomainInvariantFailure(e.to_string()))?;
        self.asset
            .verify_conservation()
            .map_err(|e| Executor1Error::DomainInvariantFailure(e.to_string()))?;
        self.gov
            .verify_rebuild_parity()
            .map_err(|e| Executor1Error::DomainInvariantFailure(e.to_string()))?;
        self.econ
            .verify_rebuild_parity()
            .map_err(|e| Executor1Error::DomainInvariantFailure(e.to_string()))?;
        self.econ
            .verify_conservation()
            .map_err(|e| Executor1Error::DomainInvariantFailure(e.to_string()))?;
        self.schedule
            .verify_rebuild_parity()
            .map_err(|e| Executor1Error::DomainInvariantFailure(e.to_string()))?;
        Ok(())
    }

    /// Executes canonical serialized [`ExecutorBlockBatchV1`] wire bytes atomically per step.
    pub fn execute_batch_bytes(
        &mut self,
        batch_bytes: &[u8],
    ) -> Result<ExecutorBlockReceiptV1, Executor1Error> {
        if batch_bytes.len() > self.bounds.max_block_bytes as usize {
            return Err(Executor1Error::PreflightOverBounds(
                "batch_bytes exceeds BOUNDS-1 max_block_bytes",
            ));
        }
        let batch = ExecutorBlockBatchV1::decode_canonical(batch_bytes)?;
        self.execute_batch(&batch)
    }

    /// Executes an [`ExecutorBlockBatchV1`], committing valid steps atomically and rolling back
    /// rejected/no-op steps with zero partial mutation.
    pub fn execute_batch(
        &mut self,
        batch: &ExecutorBlockBatchV1,
    ) -> Result<ExecutorBlockReceiptV1, Executor1Error> {
        if batch.fork_context != self.fork_context {
            return Err(Executor1Error::ForkContextMismatch(
                "batch fork_context does not match executor binding",
            ));
        }
        if batch.batch_height != self.batch_height + 1 {
            return Err(Executor1Error::NonMonotonicBatch(
                "batch_height must equal current_height + 1",
            ));
        }
        if batch.epoch < self.epoch {
            return Err(Executor1Error::NonMonotonicBatch(
                "batch epoch must be non-decreasing",
            ));
        }
        let encoded_batch = batch.encode_canonical()?;
        if encoded_batch.len() > self.bounds.max_block_bytes as usize {
            return Err(Executor1Error::PreflightOverBounds(
                "encoded batch exceeds BOUNDS-1 max_block_bytes",
            ));
        }
        if batch.ops.len() > self.spec.max_batch_ops as usize
            || batch.ops.len() > self.bounds.max_block_ops as usize
        {
            return Err(Executor1Error::PreflightOverBounds(
                "batch ops exceed max_batch_ops",
            ));
        }
        let batch_digest = batch.tag()?.digest();
        let pre_snapshot = self.snapshot()?;
        let mut staged = self.clone();
        staged.batch_height = batch.batch_height;
        staged.epoch = batch.epoch;

        let mut step_outcomes = Vec::with_capacity(batch.ops.len());
        let mut admitted_count = 0u32;
        let mut rejected_count = 0u32;
        let mut total_gas_charged = 0u64;
        let mut total_fee_burned = 0u128;
        let mut cumulative_write_mask = 0u32;

        for (idx, op) in batch.ops.iter().enumerate() {
            let op_index = idx as u32;
            let (band_slot_id, is_active, write_mask, step_gas) =
                staged.classify_and_meter(op.op_tag, op.payload.len())?;

            let (disposition, gas_charged, fee_burned, mutated_mask) = if band_slot_id == u16::MAX {
                (ExecutorDispositionV1::RejectedUnknownOperationTag, 0, 0, 0)
            } else if !is_active {
                (
                    ExecutorDispositionV1::RejectedInactiveExtensionBand,
                    0,
                    0,
                    0,
                )
            } else if total_gas_charged
                .checked_add(step_gas)
                .is_none_or(|g| g > staged.spec.max_batch_gas)
            {
                (ExecutorDispositionV1::RejectedGasLimitExceeded, 0, 0, 0)
            } else {
                let mut step_stage = staged.clone();
                let disp =
                    step_stage.dispatch_active_op(op, op_index, batch.batch_height, batch.epoch);
                let fee = u128::from(step_gas) * u128::from(batch.base_fee_per_gas);
                if disp == ExecutorDispositionV1::Admitted {
                    staged = step_stage;
                    (disp, step_gas, fee, write_mask)
                } else {
                    (disp, step_gas, fee, 0)
                }
            };

            if disposition == ExecutorDispositionV1::Admitted {
                admitted_count += 1;
                cumulative_write_mask |= mutated_mask;
            } else {
                rejected_count += 1;
            }
            total_gas_charged = total_gas_charged
                .checked_add(gas_charged)
                .ok_or(Executor1Error::PreflightOverBounds("gas sum overflow"))?;
            total_fee_burned = total_fee_burned
                .checked_add(fee_burned)
                .ok_or(Executor1Error::PreflightOverBounds("fee sum overflow"))?;

            let step_snap_sha = staged.snapshot()?.canonical_sha256()?;
            let effect_digest = compute_step_effect_digest(
                op_index,
                op.op_tag,
                band_slot_id,
                disposition,
                gas_charged,
                fee_burned,
                mutated_mask,
                step_snap_sha,
            );
            step_outcomes.push(ExecutorStepOutcomeV1 {
                op_index,
                op_tag: op.op_tag,
                band_slot_id,
                disposition,
                gas_charged,
                fee_burned,
                write_domains_mask: mutated_mask,
                effect_digest,
            });
        }

        let post_snapshot = staged.snapshot()?;
        let outcomes_digest = compute_outcomes_digest(&step_outcomes);
        *self = staged;

        Ok(ExecutorBlockReceiptV1 {
            receipt_version: EXECUTOR_1_VERSION,
            batch_digest,
            fork_context: self.fork_context,
            pre_snapshot,
            post_snapshot,
            admitted_count,
            rejected_count,
            total_gas_charged,
            total_fee_burned,
            cumulative_write_mask,
            outcomes_digest,
            step_outcomes,
        })
    }

    fn classify_and_meter(
        &self,
        op_tag: u8,
        payload_len: usize,
    ) -> Result<(u16, bool, u32, u64), Executor1Error> {
        let plen = payload_len as u64;
        if op_tag <= 15 {
            let per_byte = u64::from(self.resource.gas_per_tx_byte);
            let gas = self
                .spec
                .base_op_gas
                .checked_add(
                    plen.checked_mul(per_byte)
                        .ok_or(Executor1Error::PreflightOverBounds("gas mul overflow"))?,
                )
                .ok_or(Executor1Error::PreflightOverBounds("gas add overflow"))?;
            return Ok((0, true, self.spec.base_domains_mask, gas));
        }
        for band in &self.spec.bands {
            if op_tag >= band.op_tag_min && op_tag <= band.op_tag_max {
                let gas = band
                    .base_gas
                    .checked_add(
                        plen.checked_mul(band.per_byte_gas)
                            .ok_or(Executor1Error::PreflightOverBounds("gas mul overflow"))?,
                    )
                    .ok_or(Executor1Error::PreflightOverBounds("gas add overflow"))?;
                return Ok((band.slot_id, band.active, band.write_domains_mask, gas));
            }
        }
        Ok((u16::MAX, false, 0, 0))
    }

    fn dispatch_active_op(
        &mut self,
        op: &ExecutorOpV1,
        op_index: u32,
        batch_height: u64,
        epoch: u64,
    ) -> ExecutorDispositionV1 {
        match op.op_tag {
            OP_BASE_PLACE_VOXEL => {
                self.apply_base_edit(&op.payload, Operation::On, batch_height, op_index)
            }
            OP_BASE_DELETE_VOXEL => {
                self.apply_base_edit(&op.payload, Operation::Off, batch_height, op_index)
            }
            OP_AUTH_COMMIT_TX => self.apply_auth_tx(&op.payload, batch_height),
            OP_ASSET_TRANSFER => self.apply_asset_transfer(&op.payload, batch_height),
            OP_ASSET_MINT_DEPOSIT => self.apply_asset_mint(&op.payload, batch_height),
            OP_ASSET_BURN_REFUND => self.apply_asset_burn(&op.payload, batch_height),
            OP_ECON_BOND_STAKE => self.apply_econ_bond(&op.payload, epoch),
            OP_ECON_SLASH_EQUIVOCATION => self.apply_econ_slash(&op.payload, epoch),
            OP_SCHEDULE_EPOCH => self.apply_schedule_epoch(&op.payload),
            OP_GOV_ADVANCE_EPOCH => self.apply_gov_advance(&op.payload),
            OP_GOV_SAFE_HALT => self.apply_gov_safe_halt(&op.payload, epoch),
            _ => ExecutorDispositionV1::RejectedUnknownOperationTag,
        }
    }

    fn apply_base_edit(
        &mut self,
        payload: &[u8],
        operation: Operation,
        batch_height: u64,
        op_index: u32,
    ) -> ExecutorDispositionV1 {
        let Ok(coord) = decode_coord_be(payload) else {
            return ExecutorDispositionV1::RejectedMalformedPayload;
        };
        if is_bedrock(coord) {
            return ExecutorDispositionV1::RejectedBedrockImmutable;
        }
        let tick = batch_height
            .saturating_mul(1024)
            .saturating_add(u64::from(op_index));
        let mut tx = self.history.begin_tick(tick);
        let edit = Edit { operation, coord };
        tx.apply_edit(&self.history, &mut self.world, edit);
        let Some(first_effect) = tx.effects().first().copied() else {
            return ExecutorDispositionV1::RejectedNoOpOrConflict;
        };
        if let TransitionEffect::NoOp { reason, .. } = first_effect {
            return match reason {
                NoOpReason::BedrockProtected => ExecutorDispositionV1::RejectedBedrockImmutable,
                NoOpReason::AlreadyOn | NoOpReason::AlreadyOff => {
                    ExecutorDispositionV1::RejectedNoOpOrConflict
                }
            };
        }
        let Some(receipt) = self.history.commit_tick(tx, &self.world).cloned() else {
            return ExecutorDispositionV1::RejectedNoOpOrConflict;
        };
        if self.episodes.apply_receipt(&receipt).is_err() {
            return ExecutorDispositionV1::RejectedNoOpOrConflict;
        }
        if self
            .commit_sidecar
            .advance_for_receipt(
                &self.initial_world,
                &self.world,
                &self.history,
                &self.episodes,
                &receipt,
            )
            .is_err()
        {
            return ExecutorDispositionV1::RejectedNoOpOrConflict;
        }
        ExecutorDispositionV1::Admitted
    }

    fn apply_auth_tx(&mut self, payload: &[u8], batch_height: u64) -> ExecutorDispositionV1 {
        if payload.len() != 224 {
            return ExecutorDispositionV1::RejectedMalformedPayload;
        }
        let Ok(actor_raw) = read_arr32(payload, 0) else {
            return ExecutorDispositionV1::RejectedMalformedPayload;
        };
        let Ok(signer_raw) = read_arr32(payload, 32) else {
            return ExecutorDispositionV1::RejectedMalformedPayload;
        };
        let Ok(nonce) = read_u64_be(payload, 64) else {
            return ExecutorDispositionV1::RejectedMalformedPayload;
        };
        let Ok(valid_after) = read_u64_be(payload, 72) else {
            return ExecutorDispositionV1::RejectedMalformedPayload;
        };
        let Ok(valid_until) = read_u64_be(payload, 80) else {
            return ExecutorDispositionV1::RejectedMalformedPayload;
        };
        let Ok(max_fee) = read_u128_be(payload, 88) else {
            return ExecutorDispositionV1::RejectedMalformedPayload;
        };
        let Ok(coord) = decode_coord_be(&payload[104..128]) else {
            return ExecutorDispositionV1::RejectedMalformedPayload;
        };
        let Ok(pk) = read_arr32(payload, 128) else {
            return ExecutorDispositionV1::RejectedMalformedPayload;
        };
        let mut sig = [0u8; 64];
        sig.copy_from_slice(&payload[160..224]);
        let ctx = self.auth.registry().context();
        let tx = TransactionBodyV1 {
            actor_id: ActorIdV1::from_bytes(actor_raw),
            nonce,
            valid_after,
            valid_until,
            expected_state_root: Some(ctx.world_tag.digest()),
            max_fee,
            operations: vec![NetworkEditOpV1::On(coord)],
        };
        let Ok(tx_tag) = tx.tag() else {
            return ExecutorDispositionV1::RejectedMalformedPayload;
        };
        let pre = ctx.build_preimage(SigningDomainTagV1::ActorTransaction, tx_tag.digest());
        match self.auth.commit_transaction(
            &tx,
            ActorIdV1::from_bytes(signer_raw),
            &pre,
            &[(pk, sig)],
            batch_height,
        ) {
            Ok(_) => ExecutorDispositionV1::Admitted,
            Err(_) => ExecutorDispositionV1::RejectedAuthorizationFailed,
        }
    }

    fn apply_asset_transfer(&mut self, payload: &[u8], batch_height: u64) -> ExecutorDispositionV1 {
        if payload.len() != 80 {
            return ExecutorDispositionV1::RejectedMalformedPayload;
        }
        let (Ok(f), Ok(t), Ok(amt)) = (
            read_arr32(payload, 0),
            read_arr32(payload, 32),
            read_u128_be(payload, 64),
        ) else {
            return ExecutorDispositionV1::RejectedMalformedPayload;
        };
        let op = AssetOperationV1::TransferNative {
            from: ActorIdV1::from_bytes(f),
            to: ActorIdV1::from_bytes(t),
            amount: amt,
        };
        match self.asset.apply_local_batch(&[op], batch_height) {
            Ok(r) if r.disposition == AssetDispositionV1::Admitted => {
                ExecutorDispositionV1::Admitted
            }
            _ => ExecutorDispositionV1::RejectedAssetInvariant,
        }
    }

    fn apply_asset_mint(&mut self, payload: &[u8], batch_height: u64) -> ExecutorDispositionV1 {
        if payload.len() != 88 {
            return ExecutorDispositionV1::RejectedMalformedPayload;
        }
        let (Ok(a), Ok(c), Ok(eid)) = (
            read_arr32(payload, 0),
            decode_coord_be(&payload[32..56]),
            read_arr32(payload, 56),
        ) else {
            return ExecutorDispositionV1::RejectedMalformedPayload;
        };
        let op = AssetOperationV1::MintEpisode {
            actor: ActorIdV1::from_bytes(a),
            coord: c,
            mint_episode_id: eid,
        };
        match self.asset.apply_local_batch(&[op], batch_height) {
            Ok(r) if r.disposition == AssetDispositionV1::Admitted => {
                ExecutorDispositionV1::Admitted
            }
            _ => ExecutorDispositionV1::RejectedAssetInvariant,
        }
    }

    fn apply_asset_burn(&mut self, payload: &[u8], batch_height: u64) -> ExecutorDispositionV1 {
        if payload.len() != 56 {
            return ExecutorDispositionV1::RejectedMalformedPayload;
        }
        let (Ok(a), Ok(c)) = (read_arr32(payload, 0), decode_coord_be(&payload[32..56])) else {
            return ExecutorDispositionV1::RejectedMalformedPayload;
        };
        let op = AssetOperationV1::BurnEpisode {
            actor: ActorIdV1::from_bytes(a),
            coord: c,
        };
        match self.asset.apply_local_batch(&[op], batch_height) {
            Ok(r) if r.disposition == AssetDispositionV1::Admitted => {
                ExecutorDispositionV1::Admitted
            }
            _ => ExecutorDispositionV1::RejectedAssetInvariant,
        }
    }

    fn apply_econ_bond(&mut self, payload: &[u8], epoch: u64) -> ExecutorDispositionV1 {
        if payload.len() != 48 {
            return ExecutorDispositionV1::RejectedMalformedPayload;
        }
        let (Ok(v), Ok(amt)) = (read_arr32(payload, 0), read_u128_be(payload, 32)) else {
            return ExecutorDispositionV1::RejectedMalformedPayload;
        };
        match self.econ.bond_stake(ActorIdV1::from_bytes(v), amt, epoch) {
            Ok(r) if r.disposition == EconomicDispositionV1::Admitted => {
                ExecutorDispositionV1::Admitted
            }
            _ => ExecutorDispositionV1::RejectedValidatorOrSchedule,
        }
    }

    fn apply_econ_slash(&mut self, payload: &[u8], epoch: u64) -> ExecutorDispositionV1 {
        if payload.len() != 64 {
            return ExecutorDispositionV1::RejectedMalformedPayload;
        }
        let (Ok(v), Ok(ev)) = (read_arr32(payload, 0), read_arr32(payload, 32)) else {
            return ExecutorDispositionV1::RejectedMalformedPayload;
        };
        match self
            .econ
            .apply_equivocation_slash(ActorIdV1::from_bytes(v), ev, epoch)
        {
            Ok(r) if r.disposition == EconomicDispositionV1::Admitted => {
                ExecutorDispositionV1::Admitted
            }
            _ => ExecutorDispositionV1::RejectedValidatorOrSchedule,
        }
    }

    fn apply_schedule_epoch(&mut self, payload: &[u8]) -> ExecutorDispositionV1 {
        if payload.len() != 8 {
            return ExecutorDispositionV1::RejectedMalformedPayload;
        }
        let Ok(src_epoch) = read_u64_be(payload, 0) else {
            return ExecutorDispositionV1::RejectedMalformedPayload;
        };
        if self.gov.root_commitment().lifecycle == GovernanceLifecycleCodeV1::SafeHalted {
            return ExecutorDispositionV1::RejectedValidatorOrSchedule;
        }
        match self.schedule.schedule_epoch_from_sources(
            &self.econ,
            &self.gov,
            src_epoch,
            &self.forbidden_actor_keys,
        ) {
            Ok(_) => ExecutorDispositionV1::Admitted,
            Err(_) => ExecutorDispositionV1::RejectedValidatorOrSchedule,
        }
    }

    fn apply_gov_advance(&mut self, payload: &[u8]) -> ExecutorDispositionV1 {
        if payload.len() != 8 {
            return ExecutorDispositionV1::RejectedMalformedPayload;
        }
        let Ok(next_epoch) = read_u64_be(payload, 0) else {
            return ExecutorDispositionV1::RejectedMalformedPayload;
        };
        match self.gov.advance_epoch(next_epoch) {
            Ok(()) => ExecutorDispositionV1::Admitted,
            Err(_) => ExecutorDispositionV1::RejectedGovernanceFailed,
        }
    }

    fn apply_gov_safe_halt(&mut self, payload: &[u8], epoch: u64) -> ExecutorDispositionV1 {
        if payload.len() != 49 {
            return ExecutorDispositionV1::RejectedMalformedPayload;
        }
        let (Ok(art), Ok(obs), Ok(lim)) = (
            read_arr32(payload, 0),
            read_u64_be(payload, 33),
            read_u64_be(payload, 41),
        ) else {
            return ExecutorDispositionV1::RejectedMalformedPayload;
        };
        let ev = SafeHaltEvidenceV1 {
            schema_version: 1,
            bounds_profile_digest: self.bounds_profile_digest,
            artifact_digest: art,
            violation_code: payload[32],
            observed_value: obs,
            committed_limit: lim,
        };
        match self.gov.trigger_safe_halt_from_bounds_evidence(&ev, epoch) {
            Ok(r) if r.disposition == GovernanceDispositionV1::Admitted => {
                ExecutorDispositionV1::Admitted
            }
            _ => ExecutorDispositionV1::RejectedGovernanceFailed,
        }
    }
}

/// Powered semantic-mutant verifier enforcing all `EXECUTOR-1` receipt invariants against `batch`
/// and `spec`.
pub fn verify_executor_receipt_against_spec(
    batch: &ExecutorBlockBatchV1,
    spec: &ResolvedProfileTransitionSpecV1,
    receipt: &ExecutorBlockReceiptV1,
) -> Result<(), Executor1Error> {
    if receipt.receipt_version != EXECUTOR_1_VERSION {
        return Err(Executor1Error::SemanticMutantDetected(
            "bad receipt_version",
        ));
    }
    if receipt.batch_digest != batch.tag()?.digest() {
        return Err(Executor1Error::SemanticMutantDetected(
            "batch_digest mismatch",
        ));
    }
    if receipt.fork_context != batch.fork_context
        || receipt.fork_context.profile_id != spec.profile_id
    {
        return Err(Executor1Error::SemanticMutantDetected(
            "fork_context mismatch",
        ));
    }
    if receipt.step_outcomes.len() != batch.ops.len() {
        return Err(Executor1Error::SemanticMutantDetected(
            "omitted or extra step outcome",
        ));
    }
    if receipt.post_snapshot.program_state_active && !spec.program_slot_active {
        return Err(Executor1Error::SemanticMutantDetected(
            "inactive Program/VM activated",
        ));
    }
    let mut admitted = 0u32;
    let mut rejected = 0u32;
    let mut gas_sum = 0u64;
    let mut fee_sum = 0u128;
    let mut write_union = 0u32;
    for (i, (op, out)) in batch.ops.iter().zip(&receipt.step_outcomes).enumerate() {
        if out.op_index != i as u32 || out.op_tag != op.op_tag {
            return Err(Executor1Error::SemanticMutantDetected(
                "step index/tag mismatch",
            ));
        }
        if !spec.program_slot_active
            && (80..=95).contains(&op.op_tag)
            && out.disposition != ExecutorDispositionV1::RejectedInactiveExtensionBand
        {
            return Err(Executor1Error::SemanticMutantDetected(
                "inactive band reached",
            ));
        }
        if out.disposition == ExecutorDispositionV1::Admitted {
            if out.gas_charged == 0 {
                return Err(Executor1Error::SemanticMutantDetected(
                    "uncharged admitted work",
                ));
            }
            admitted += 1;
            write_union |= out.write_domains_mask;
        } else {
            if out.write_domains_mask != 0 {
                return Err(Executor1Error::SemanticMutantDetected(
                    "rejected step wrote domain",
                ));
            }
            rejected += 1;
        }
        if out.fee_burned != u128::from(out.gas_charged) * u128::from(batch.base_fee_per_gas) {
            return Err(Executor1Error::SemanticMutantDetected(
                "fee arithmetic mismatch",
            ));
        }
        gas_sum = gas_sum
            .checked_add(out.gas_charged)
            .ok_or(Executor1Error::SemanticMutantDetected("gas overflow"))?;
        fee_sum = fee_sum
            .checked_add(out.fee_burned)
            .ok_or(Executor1Error::SemanticMutantDetected("fee overflow"))?;
    }
    if admitted != receipt.admitted_count
        || rejected != receipt.rejected_count
        || gas_sum != receipt.total_gas_charged
        || fee_sum != receipt.total_fee_burned
        || write_union != receipt.cumulative_write_mask
    {
        return Err(Executor1Error::SemanticMutantDetected(
            "aggregate totals mismatch",
        ));
    }
    if admitted == 0 && !receipt.pre_snapshot.roots_equal(&receipt.post_snapshot) {
        return Err(Executor1Error::SemanticMutantDetected(
            "all-rejected batch leaked partial root mutation",
        ));
    }
    if compute_outcomes_digest(&receipt.step_outcomes) != receipt.outcomes_digest {
        return Err(Executor1Error::SemanticMutantDetected(
            "outcomes_digest tampered",
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn compute_step_effect_digest(
    op_index: u32,
    op_tag: u8,
    band_slot_id: u16,
    disposition: ExecutorDispositionV1,
    gas_charged: u64,
    fee_burned: u128,
    write_domains_mask: u32,
    post_snapshot_sha: [u8; 32],
) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(STEP_EFFECT_PREFIX);
    h.update(op_index.to_be_bytes());
    h.update([op_tag]);
    h.update(band_slot_id.to_be_bytes());
    h.update([disposition.code()]);
    h.update(gas_charged.to_be_bytes());
    h.update(fee_burned.to_be_bytes());
    h.update(write_domains_mask.to_be_bytes());
    h.update(post_snapshot_sha);
    h.finalize().into()
}

fn compute_outcomes_digest(outcomes: &[ExecutorStepOutcomeV1]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(OUTCOMES_DIGEST_PREFIX);
    h.update((outcomes.len() as u32).to_be_bytes());
    for o in outcomes {
        h.update(o.effect_digest);
    }
    h.finalize().into()
}

fn encode_coord_be(c: Coord) -> [u8; 24] {
    let mut out = Vec::with_capacity(24);
    out.extend_from_slice(&c.x.to_be_bytes());
    out.extend_from_slice(&c.y.to_be_bytes());
    out.extend_from_slice(&c.z.to_be_bytes());
    out.try_into().expect("24-byte coord")
}

fn decode_coord_be(bytes: &[u8]) -> Result<Coord, Executor1Error> {
    if bytes.len() != 24 {
        return Err(Executor1Error::InvalidWire(
            "coord payload must be 24 bytes",
        ));
    }
    let x = i64::from_be_bytes(bytes[0..8].try_into().unwrap());
    let y = i64::from_be_bytes(bytes[8..16].try_into().unwrap());
    let z = i64::from_be_bytes(bytes[16..24].try_into().unwrap());
    Ok(Coord::new(x, y, z))
}

fn read_u16_be(bytes: &[u8], off: usize) -> Result<u16, Executor1Error> {
    let sl: [u8; 2] = bytes
        .get(off..off + 2)
        .ok_or(Executor1Error::InvalidWire("truncated u16"))?
        .try_into()
        .unwrap();
    Ok(u16::from_be_bytes(sl))
}

fn read_u32_be(bytes: &[u8], off: usize) -> Result<u32, Executor1Error> {
    let sl: [u8; 4] = bytes
        .get(off..off + 4)
        .ok_or(Executor1Error::InvalidWire("truncated u32"))?
        .try_into()
        .unwrap();
    Ok(u32::from_be_bytes(sl))
}

fn read_u64_be(bytes: &[u8], off: usize) -> Result<u64, Executor1Error> {
    let sl: [u8; 8] = bytes
        .get(off..off + 8)
        .ok_or(Executor1Error::InvalidWire("truncated u64"))?
        .try_into()
        .unwrap();
    Ok(u64::from_be_bytes(sl))
}

fn read_u128_be(bytes: &[u8], off: usize) -> Result<u128, Executor1Error> {
    let sl: [u8; 16] = bytes
        .get(off..off + 16)
        .ok_or(Executor1Error::InvalidWire("truncated u128"))?
        .try_into()
        .unwrap();
    Ok(u128::from_be_bytes(sl))
}

fn read_arr32(bytes: &[u8], off: usize) -> Result<[u8; 32], Executor1Error> {
    let sl: [u8; 32] = bytes
        .get(off..off + 32)
        .ok_or(Executor1Error::InvalidWire("truncated [u8; 32]"))?
        .try_into()
        .unwrap();
    Ok(sl)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commit_state::DomainRegistryV1;
    use crate::crypto_d1::{HashSuiteIdV1, SignatureSuiteIdV1, sign_preimage_deterministic};
    use crate::history::world_tag;
    use crate::identity_d1::{AuthorizationDescriptorV1, CredentialRoleV1, ThresholdPolicyV1};
    use crate::profile_bootstrap::ProfileScopeV1;
    use crate::threat_1::ThreatModelV1;
    use crate::transition_spec::{BASE_STATE_DOMAINS_MASK, BaseTransitionSpecD1, ConflictPolicy};
    use crate::transition_spec_1::{
        BandPredicateGateV1, DOMAIN_MASK_ASSET_RESOURCE, DOMAIN_MASK_AUTH_NONCE,
        DOMAIN_MASK_GOV_UPGRADE, DOMAIN_MASK_PROGRAM_VM, DOMAIN_MASK_VALIDATOR_SCHEDULE,
        TRANSITION_SPEC_1_VERSION, TransitionBandSlotV1,
    };

    fn k(seed: u8) -> CanonicalEd25519PublicKeyV1 {
        CanonicalEd25519PublicKeyV1::from_secret_seed(&[seed; 32]).unwrap()
    }

    fn tpol(role: CredentialRoleV1, seed: u8) -> ThresholdPolicyV1 {
        ThresholdPolicyV1 {
            role,
            threshold: 1,
            keys: vec![k(seed)],
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn make_slot(
        id: u16,
        lo: u8,
        hi: u8,
        active: bool,
        cp: ConflictPolicy,
        rmask: u32,
        wmask: u32,
        bgas: u64,
        pbgas: u64,
        gate: BandPredicateGateV1,
    ) -> TransitionBandSlotV1 {
        TransitionBandSlotV1 {
            slot_id: id,
            op_tag_min: lo,
            op_tag_max: hi,
            disposition_tag_min: lo,
            disposition_tag_max: hi,
            active,
            conflict_policy: cp,
            read_domains_mask: rmask,
            write_domains_mask: wmask,
            base_gas: bgas,
            per_byte_gas: pbgas,
            predicate_gate: gate,
        }
    }

    fn fixture_executor() -> (
        PrimaryLaunchExecutorV1,
        ActorIdV1,
        ActorIdV1,
        [ActorIdV1; 4],
    ) {
        let econ_pol = ValidatorEconPolicyV1::v1_pos_default();
        let cons_pol = ConsensusPolicyDescriptorV1::v1_bft_pos_default();
        let gov_pol = GovernancePolicyDescriptorV1 {
            schema_version: 1,
            upgrade_quorum_numerator: 2,
            upgrade_quorum_denominator: 3,
            mandatory_exit_timelock_epochs: econ_pol.unbonding_delay_epochs,
            max_emergency_pause_epochs: 3,
            automatic_safe_halt_on_proven_fault: true,
            emergency_council_pause_only: true,
            forbid_history_rewrite: true,
            emergency_council: tpol(CredentialRoleV1::EmergencyPauseSigner, 91),
        };
        let preds = "asset-active economic-charging stake-consensus validator-economics"
            .split_ascii_whitespace()
            .map(str::to_string)
            .collect();
        let scope =
            ProfileScopeV1::from_wave1_contracts(&DomainRegistryV1::local_v1(), &gov_pol, preds)
                .unwrap();
        let threat = ThreatModelV1::v1_conservative_consumer(&scope).unwrap();
        let bounds =
            ProtocolBoundsProfileV1::v1_consumer_default(&threat, &LocalStorageProfile::DEFAULT)
                .unwrap();
        let asset_pol = AssetPolicyDescriptorV1::v1_dual_domain_default();
        let resource = ResourcePolicyV1::v1_default(&bounds, &asset_pol).unwrap();
        let genesis_id = [0xAA; 32];
        let profile_id = [0xBB; 32];
        let pbg = u64::from(resource.gas_per_tx_byte);
        let b0 = BASE_STATE_DOMAINS_MASK;
        let b1 = b0 | DOMAIN_MASK_AUTH_NONCE;
        let b2 = b1 | DOMAIN_MASK_ASSET_RESOURCE;
        let b3 = b2 | DOMAIN_MASK_VALIDATOR_SCHEDULE;
        let b4 = b3 | DOMAIN_MASK_GOV_UPGRADE;
        let b5 = b4 | DOMAIN_MASK_PROGRAM_VM;
        let spec = ResolvedProfileTransitionSpecV1 {
            spec_version: TRANSITION_SPEC_1_VERSION,
            profile_id,
            base_contract_digest: BaseTransitionSpecD1::canonical_base_only()
                .tag()
                .unwrap()
                .digest(),
            bounds_profile_digest: bounds.tag().unwrap().digest(),
            resource_policy_digest: resource.tag().unwrap().digest(),
            max_batch_ops: bounds.max_block_ops,
            max_batch_gas: resource.max_block_gas,
            base_op_gas: u64::from(resource.gas_per_occupancy_op),
            base_domains_mask: b0,
            program_slot_active: false,
            bands: vec![
                make_slot(
                    1,
                    16,
                    31,
                    true,
                    ConflictPolicy::DeterministicOrder,
                    b1,
                    DOMAIN_MASK_AUTH_NONCE,
                    u64::from(resource.gas_per_tx_base),
                    pbg,
                    BandPredicateGateV1::AlwaysRequired,
                ),
                make_slot(
                    2,
                    32,
                    47,
                    true,
                    ConflictPolicy::RejectConcurrentWrite,
                    b2,
                    DOMAIN_MASK_ASSET_RESOURCE,
                    25_000,
                    pbg,
                    BandPredicateGateV1::AssetActive,
                ),
                make_slot(
                    3,
                    48,
                    63,
                    true,
                    ConflictPolicy::DeterministicOrder,
                    b3,
                    DOMAIN_MASK_VALIDATOR_SCHEDULE,
                    u64::from(resource.gas_per_ed25519_sig),
                    pbg,
                    BandPredicateGateV1::AlwaysRequired,
                ),
                make_slot(
                    4,
                    64,
                    79,
                    true,
                    ConflictPolicy::RejectConcurrentWrite,
                    b4,
                    DOMAIN_MASK_GOV_UPGRADE,
                    40_000,
                    pbg,
                    BandPredicateGateV1::AlwaysRequired,
                ),
                make_slot(
                    5,
                    80,
                    95,
                    false,
                    ConflictPolicy::RejectConcurrentWrite,
                    b5,
                    DOMAIN_MASK_PROGRAM_VM,
                    50_000,
                    u64::from(resource.gas_per_proof_byte),
                    BandPredicateGateV1::ProgramActive,
                ),
            ],
        };
        let fork_context = ExecutorForkContextV1 {
            chain_id: 77,
            world_id: 1,
            genesis_id,
            profile_id,
            transition_spec_digest: spec.tag().unwrap().digest(),
        };
        let desc_a = AuthorizationDescriptorV1 {
            descriptor_version: 0,
            action_policy: tpol(CredentialRoleV1::ActorAction, 11),
            admin_policy: tpol(CredentialRoleV1::ActorAdmin, 12),
            delegations: vec![],
            revoked: false,
        };
        let desc_b = AuthorizationDescriptorV1 {
            descriptor_version: 0,
            action_policy: tpol(CredentialRoleV1::ActorAction, 21),
            admin_policy: tpol(CredentialRoleV1::ActorAdmin, 22),
            delegations: vec![],
            revoked: false,
        };
        let actor_a = ActorIdV1::derive(77, "exec-a", &desc_a).unwrap();
        let actor_b = ActorIdV1::derive(77, "exec-b", &desc_b).unwrap();
        let vals = [
            ActorIdV1::from_bytes([1; 32]),
            ActorIdV1::from_bytes([2; 32]),
            ActorIdV1::from_bytes([3; 32]),
            ActorIdV1::from_bytes([4; 32]),
        ];
        let auth_ctx = IdentityContextV1 {
            protocol_version: 1,
            chain_id: 77,
            genesis_id,
            launch_manifest_id: [0x22; 32],
            profile_id,
            world_tag: world_tag(&World::new()),
            signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
            hash_suite: HashSuiteIdV1::Sha256V1,
        };
        let gov_ctx = GovernanceContextV1 {
            protocol_version: 1,
            chain_id: 77,
            genesis_id,
            launch_manifest_id: [0x22; 32],
            profile_id,
        };
        let st_a = ActorAuthStateV1 {
            state_version: 1,
            actor_id: actor_a,
            descriptor: desc_a,
            next_nonce: 0,
            recovery_guardians: tpol(CredentialRoleV1::ActorAdmin, 13),
            recovery_delay_heights: 100,
            pending_recovery: None,
        };
        let st_b = ActorAuthStateV1 {
            state_version: 1,
            actor_id: actor_b,
            descriptor: desc_b,
            next_nonce: 0,
            recovery_guardians: tpol(CredentialRoleV1::ActorAdmin, 23),
            recovery_delay_heights: 100,
            pending_recovery: None,
        };
        let forbidden: BTreeSet<_> = [11, 12, 13, 21, 22, 23].into_iter().map(k).collect();
        let cfg = ExecutorGenesisConfigV1 {
            fork_context,
            profile_scope_digest: scope.tag().unwrap().digest(),
            spec,
            bounds,
            resource,
            auth_context: auth_ctx,
            initial_actors: vec![st_a, st_b],
            asset_policy: asset_pol,
            initial_balances: vec![(actor_a, 5_000_000), (actor_b, 5_000_000)],
            gov_context: gov_ctx,
            gov_policy: gov_pol,
            initial_params: vec![(1, 50_000_000)],
            econ_policy: econ_pol,
            initial_stakes: vals.iter().map(|&v| (v, 100_000_000_000)).collect(),
            consensus_policy: cons_pol,
            initial_validator_keys: (0..4)
                .map(|i| {
                    (
                        vals[i],
                        ValidatorKeyPairV1 {
                            vote_key: k(31 + i as u8),
                            proposal_key: k(41 + i as u8),
                        },
                    )
                })
                .collect(),
            forbidden_actor_keys: forbidden,
        };
        (
            PrimaryLaunchExecutorV1::from_genesis(cfg).unwrap(),
            actor_a,
            actor_b,
            vals,
        )
    }

    #[test]
    fn executor_1_codec_execution_rollback_and_mutant_invariants() {
        let (mut exec, actor_a, actor_b, vals) = fixture_executor();
        exec.verify_all_domain_rebuild_parity().unwrap();
        let tx = TransactionBodyV1 {
            actor_id: actor_a,
            nonce: 0,
            valid_after: 1,
            valid_until: 50,
            expected_state_root: Some(world_tag(&World::new()).digest()),
            max_fee: 1_000,
            operations: vec![NetworkEditOpV1::On(Coord::new(2, 0, 0))],
        };
        let pre = exec.auth.registry().context().build_preimage(
            SigningDomainTagV1::ActorTransaction,
            tx.tag().unwrap().digest(),
        );
        let sig = sign_preimage_deterministic(&[11; 32], &pre).unwrap();
        let batch = ExecutorBlockBatchV1 {
            batch_version: 1,
            batch_height: 1,
            epoch: 0,
            fork_context: exec.fork_context(),
            base_fee_per_gas: 2,
            ops: vec![
                ExecutorOpV1::base_place_voxel(Coord::new(2, 0, 0)),
                ExecutorOpV1::auth_commit_tx(
                    actor_a,
                    actor_a,
                    0,
                    1,
                    50,
                    1_000,
                    Coord::new(2, 0, 0),
                    *k(11).as_bytes(),
                    *sig.as_bytes(),
                ),
                ExecutorOpV1::asset_transfer(actor_a, actor_b, 250_000),
                ExecutorOpV1::econ_bond_stake(vals[0], 10_000_000_000),
                ExecutorOpV1::schedule_epoch(0),
                ExecutorOpV1 {
                    op_tag: 80,
                    payload: vec![1, 2, 3],
                },
            ],
        };
        let encoded = batch.encode_canonical().unwrap();
        assert_eq!(
            ExecutorBlockBatchV1::decode_canonical(&encoded).unwrap(),
            batch
        );
        let rcpt = exec.execute_batch_bytes(&encoded).unwrap();
        let rcpt_bytes = rcpt.encode_canonical().unwrap();
        assert_eq!(
            ExecutorBlockReceiptV1::decode_canonical(&rcpt_bytes).unwrap(),
            rcpt
        );
        verify_executor_receipt_against_spec(&batch, exec.spec(), &rcpt).unwrap();
        assert_eq!(rcpt.admitted_count, 5);
        assert_eq!(rcpt.rejected_count, 1);
        assert_eq!(
            rcpt.step_outcomes[5].disposition,
            ExecutorDispositionV1::RejectedInactiveExtensionBand
        );
        exec.verify_all_domain_rebuild_parity().unwrap();
    }
}
