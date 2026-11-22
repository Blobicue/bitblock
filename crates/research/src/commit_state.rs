//! `COMMIT-1`: authenticated local state over separately typed binary Patricia maps.
//!
//! Instantiates the Candidate A fixed-key binary Merkle Patricia Trie selected by
//! `COMMIT-R1` across the three local state domains specified in
//! `docs/AUTHENTICATED_STATE_PLAN.md`:
//!
//! - [`WorldRootV1`]: `CoordinateKeyV1` (192 bits) -> `OnV1` (`[0x01]`), including origin bedrock;
//! - [`ActiveTokenRootV1`]: `CoordinateKeyV1` (192 bits) -> [`MintIdV1`] (32-byte event digest),
//!   covering live mutable occupancy episodes only (origin bedrock strictly excluded);
//! - [`EpisodeRootV1`]: [`EventIdV1`] (256 bits) -> [`EpisodeRecordV1`] (canonical active/closed
//!   episode record), covering all active and retained closed episodes under the local retention policy.
//!
//! All three roots are bound with the committed ledger head inside one immutable
//! [`CommitmentSetV1`] under a [`DomainRegistryV1`] and immutable [`ProfileRegistryV1`], published
//! through a staged-node -> reachability-closure -> provisional-pin -> manifest -> recoverable-head
//! CAS -> pin-promotion pipeline with pin-epoch write-barrier GC.
//!
//! This module is a separately typed sidecar: it never mutates or reinterprets legacy
//! `world_tag` (`TagKind::World`), nor does it claim consensus finality, data availability,
//! or economic ownership.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt;
use std::sync::Arc;

use bitblock_core::history::{History, Receipt, TransitionEffect};
use bitblock_core::protocol::{ContentTag, TagKind, hash_canonical};
use bitblock_core::world::{Coord, World, is_bedrock};
use sha2::{Digest, Sha256};

use crate::commit_r1::{CoordinateKeyV1, KEY_BITS as COORD_KEY_BITS, KEY_BYTES as COORD_KEY_BYTES};
use crate::coordinate_episode::{
    CoordinateEpisode, CoordinateEpisodeError, OccupancyEpisodeLedger,
};
use crate::history_store::{HistoryReplayLimits, verify_in_memory_history_with_limits};
use crate::local_bounds::LocalStorageProfile;

#[cfg(not(target_arch = "wasm32"))]
use crate::coordinate_episode::{
    DurableLedgerError, DurableOccupancyLedger, OpenDurableOccupancyLedger,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::history_store::{HistoryAppendReceipt, HistoryCreateReceipt};
#[cfg(not(target_arch = "wasm32"))]
use crate::journal::{AnchorPolicy, ScanReport};
#[cfg(not(target_arch = "wasm32"))]
use bitblock_core::program::Edit;

/// Canonical schema for `ProofPolicyDescriptorV1` descriptors.
pub const COMMIT_1_PROOF_POLICY_SCHEMA: &[u8] = b"bitblock/commit-1/proof-policy/v1";
/// Canonical schema for `CommitmentOperationalLimitsDescriptorV1` descriptors.
pub const COMMIT_1_OP_LIMITS_SCHEMA: &[u8] = b"bitblock/commit-1/operational-limits/v1";
/// Canonical schema for `CommitmentProfileDescriptorV1` descriptors.
pub const COMMIT_1_PROFILE_SCHEMA: &[u8] = b"bitblock/commit-1/commitment-profile/v1";
/// Canonical schema for `DomainRegistryV1` domain activation manifests.
pub const COMMIT_1_DOMAIN_REGISTRY_SCHEMA: &[u8] = b"bitblock/commit-1/domain-registry/v1";
/// Canonical schema and tree domain for `WorldRootV1` authenticated world occupancy roots.
pub const COMMIT_1_WORLD_ROOT_SCHEMA: &[u8] = b"bitblock/commit-1/world-root/v1";
/// Canonical schema and tree domain for `ActiveTokenRootV1` live mutable episode roots.
pub const COMMIT_1_ACTIVE_TOKEN_ROOT_SCHEMA: &[u8] = b"bitblock/commit-1/active-token-root/v1";
/// Canonical schema and tree domain for `EpisodeRootV1` retained episode provenance roots.
pub const COMMIT_1_EPISODE_ROOT_SCHEMA: &[u8] = b"bitblock/commit-1/episode-root/v1";
/// Canonical schema for `TypedPatriciaProofV1` context-bound point witnesses.
pub const COMMIT_1_POINT_PROOF_SCHEMA: &[u8] = b"bitblock/commit-1/patricia-proof/v1";
/// Canonical schema for `CommitmentSetV1` immutable multi-root commitment manifests.
pub const COMMIT_1_COMMITMENT_SET_SCHEMA: &[u8] = b"bitblock/commit-1/commitment-set/v1";

/// Bit width (`256`) of an `EventIdV1` key.
pub const EVENT_KEY_BITS: usize = 256;
/// Byte length (`32`) of an `EventIdV1` key or `MintIdV1` value.
pub const EVENT_KEY_BYTES: usize = 32;
/// Canonical value bytes for `OnV1` (`[0x01]`).
pub const ON_V1_VALUE_BYTES: &[u8] = &[0x01];
/// Canonical schema version for all `COMMIT-1` v1 descriptors and sets.
pub const COMMIT_1_SCHEMA_VERSION: u16 = 1;
/// Canonical node codec version for `COMMIT-1` binary Patricia nodes.
pub const COMMIT_1_NODE_CODEC_VERSION: u16 = 1;
/// Canonical root codec version for `COMMIT-1` typed roots.
pub const COMMIT_1_ROOT_CODEC_VERSION: u16 = 1;

const HASH_SUITE_BYTES: &[u8] = b"sha256";
const NODE_TYPE_EMPTY: u8 = 0;
const NODE_TYPE_LEAF: u8 = 1;
const NODE_TYPE_EXTENSION: u8 = 2;
const NODE_TYPE_BRANCH: u8 = 3;

const PROOF_STEP_BRANCH: u8 = 1;
const PROOF_STEP_EXTENSION: u8 = 2;
const PROOF_TERM_EMPTY: u8 = 0;
const PROOF_TERM_LEAF: u8 = 1;
const PROOF_TERM_DIVERGENT_EXT: u8 = 2;

const EPISODE_ACTIVE_MARKER: u8 = 0;
const EPISODE_CLOSED_MARKER: u8 = 1;
const EPISODE_ACTIVE_WIRE_LEN: usize = 24 + 32 + 1;
const EPISODE_CLOSED_WIRE_LEN: usize = 24 + 32 + 1 + 32;

const SUBJECT_TAG_LOCAL_TRANSITION: u8 = 1;
const PREDECESSOR_TAG_GENESIS: u8 = 0;
const PREDECESSOR_TAG_ORDINARY: u8 = 1;
const REGISTRY_ENTRY_ACTIVE: u8 = 1;
const REGISTRY_ENTRY_INACTIVE: u8 = 0;

const PIN_RECORD_VERSION: u16 = 1;
const HEAD_RECORD_VERSION: u16 = 1;

/// Logical state domain tag in [`DomainRegistryV1`] and [`RequiredDomainV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum DomainTagV1 {
    /// Binary occupied-coordinate set (`WorldRootV1`).
    WorldOccupancy = 1,
    /// Live mutable coordinate-to-mint map (`ActiveTokenRootV1`).
    ActiveToken = 2,
    /// Retained active and closed episode provenance map (`EpisodeRootV1`).
    Episode = 3,
    /// Reserved future actor authorization domain (inactive in local `COMMIT-1`).
    Authorization = 4,
    /// Reserved future actor nonce domain (inactive in local `COMMIT-1`).
    Nonce = 5,
    /// Reserved future asset-state domain (inactive in local `COMMIT-1`).
    AssetState = 6,
}

impl DomainTagV1 {
    /// Returns the canonical 1-byte wire code.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte wire code.
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::WorldOccupancy),
            2 => Some(Self::ActiveToken),
            3 => Some(Self::Episode),
            4 => Some(Self::Authorization),
            5 => Some(Self::Nonce),
            6 => Some(Self::AssetState),
            _ => None,
        }
    }
}

/// Composition layer tag for a domain entry in [`DomainRegistryV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum CompositionLayerV1 {
    /// Base local authenticated state layer (`COMMIT-1`).
    LocalState = 1,
    /// Network identity and authorization layer (inactive locally).
    NetworkAuthority = 2,
    /// Optional profile extension layer (inactive locally).
    OptionalExtension = 3,
}

impl CompositionLayerV1 {
    /// Returns the canonical 1-byte wire code.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte wire code.
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::LocalState),
            2 => Some(Self::NetworkAuthority),
            3 => Some(Self::OptionalExtension),
            _ => None,
        }
    }
}

/// Typed root wrapper family identifier for activated `COMMIT-1` maps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum RootTypeV1 {
    /// `WorldRootV1` (`CoordinateKeyV1` -> `OnV1`).
    WorldRoot = 1,
    /// `ActiveTokenRootV1` (`CoordinateKeyV1` -> `MintIdV1`).
    ActiveTokenRoot = 2,
    /// `EpisodeRootV1` (`EventIdV1` -> `EpisodeRecordV1`).
    EpisodeRoot = 3,
}

impl RootTypeV1 {
    /// Returns the canonical 1-byte wire code.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte wire code.
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::WorldRoot),
            2 => Some(Self::ActiveTokenRoot),
            3 => Some(Self::EpisodeRoot),
            _ => None,
        }
    }

    /// Returns the canonical tree-domain / root schema byte string for this root type.
    pub const fn schema_bytes(self) -> &'static [u8] {
        match self {
            Self::WorldRoot => COMMIT_1_WORLD_ROOT_SCHEMA,
            Self::ActiveTokenRoot => COMMIT_1_ACTIVE_TOKEN_ROOT_SCHEMA,
            Self::EpisodeRoot => COMMIT_1_EPISODE_ROOT_SCHEMA,
        }
    }

    /// Returns the outer `TagKind` used when tagging this root wrapper.
    pub const fn tag_kind(self) -> TagKind {
        match self {
            Self::WorldRoot => TagKind::World,
            Self::ActiveTokenRoot | Self::EpisodeRoot => TagKind::Receipt,
        }
    }

    /// Returns the expected logical [`DomainTagV1`] bound to this root type.
    pub const fn domain_tag(self) -> DomainTagV1 {
        match self {
            Self::WorldRoot => DomainTagV1::WorldOccupancy,
            Self::ActiveTokenRoot => DomainTagV1::ActiveToken,
            Self::EpisodeRoot => DomainTagV1::Episode,
        }
    }
}

/// Fixed-width key codec identifier bound by [`CommitmentProfileDescriptorV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum KeyCodecIdV1 {
    /// Bijective 192-bit `CoordinateKeyV1` (`24` bytes).
    CoordinateKeyV1 = 1,
    /// 256-bit `EventIdV1` event content-tag digest (`32` bytes).
    EventIdV1 = 2,
}

impl KeyCodecIdV1 {
    /// Returns the canonical 1-byte wire code.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte wire code.
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::CoordinateKeyV1),
            2 => Some(Self::EventIdV1),
            _ => None,
        }
    }

    /// Returns the exact bit width of keys encoded under this codec.
    pub const fn bit_width(self) -> u16 {
        match self {
            Self::CoordinateKeyV1 => COORD_KEY_BITS as u16,
            Self::EventIdV1 => EVENT_KEY_BITS as u16,
        }
    }

    /// Returns the exact byte length of keys encoded under this codec.
    pub const fn byte_len(self) -> usize {
        match self {
            Self::CoordinateKeyV1 => COORD_KEY_BYTES,
            Self::EventIdV1 => EVENT_KEY_BYTES,
        }
    }
}

/// Typed leaf value codec identifier bound by [`CommitmentProfileDescriptorV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ValueCodecIdV1 {
    /// Single-byte presence marker `OnV1` (`[0x01]`).
    OnV1 = 1,
    /// 32-byte `MintIdV1` event digest.
    MintIdV1 = 2,
    /// 57-byte (active) or 89-byte (closed) `EpisodeRecordV1`.
    EpisodeRecordV1 = 3,
}

impl ValueCodecIdV1 {
    /// Returns the canonical 1-byte wire code.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte wire code.
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::OnV1),
            2 => Some(Self::MintIdV1),
            3 => Some(Self::EpisodeRecordV1),
            _ => None,
        }
    }
}

/// Authenticated tree algorithm identifier (`BinaryMerklePatriciaV1` selected by `COMMIT-R1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum TreeAlgorithmIdV1 {
    /// Candidate A fixed-key binary Merkle Patricia Trie.
    BinaryMerklePatriciaV1 = 1,
}

impl TreeAlgorithmIdV1 {
    /// Returns the canonical 1-byte wire code.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte wire code.
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::BinaryMerklePatriciaV1),
            _ => None,
        }
    }
}

/// Cryptographic hash suite identifier for `COMMIT-1` node hashing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum HashSuiteIdV1 {
    /// SHA-256 (32-byte digests).
    Sha256V1 = 1,
}

impl HashSuiteIdV1 {
    /// Returns the canonical 1-byte wire code.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte wire code.
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::Sha256V1),
            _ => None,
        }
    }
}

/// Empty-root rule identifier for `COMMIT-1` profiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum EmptyRootRuleIdV1 {
    /// `NodeHash(MptEmpty, depth=0, [])` with zero-step empty lookup proof.
    MptEmptyDepthZeroV1 = 1,
}

impl EmptyRootRuleIdV1 {
    /// Returns the canonical 1-byte wire code.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte wire code.
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::MptEmptyDepthZeroV1),
            _ => None,
        }
    }
}

/// Typed sentinel identifier for inactive domains in [`DomainRegistryV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum InactiveSentinelIdV1 {
    /// Domain is explicitly unactivated in the local `COMMIT-1` profile.
    UnactivatedLocalProfileV1 = 1,
}

impl InactiveSentinelIdV1 {
    /// Returns the canonical 1-byte wire code.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte wire code.
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::UnactivatedLocalProfileV1),
            _ => None,
        }
    }
}

/// Fail-closed error returned by `COMMIT-1` codec, proof, invariant, publication, or recovery checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommitStateError {
    /// Descriptor, record, or carrier version is unsupported or mismatched.
    WrongVersion { expected: u16, actual: u16 },
    /// Wire encoding is truncated, overlong, or violates canonical framing.
    MalformedEncoding { reason: &'static str },
    /// A numeric policy or operational limit is zero, overflowing, or exceeded.
    LimitExceeded { field: &'static str },
    /// Machine-local cap attempted to widen an admitted proof policy.
    LocalCapWidensPolicy { field: &'static str },
    /// Profile, policy, or domain registry entry is unknown or internally inconsistent.
    RegistryMismatch { reason: &'static str },
    /// Supplied key byte length or value bytes violate the domain's codec contract.
    InvalidKeyOrValue { reason: &'static str },
    /// Duplicate logical key supplied to a full-rebuild constructor.
    DuplicateKey,
    /// Proof bytes or reconstructed witness failed canonical or root verification.
    InvalidProof { reason: &'static str },
    /// Verified lookup result did not match caller's expected claim.
    ClaimMismatch,
    /// Commitment set, predecessor, or cross-map projection invariant failed.
    InvariantViolation { reason: &'static str },
    /// Content-addressed node missing, corrupted, or failed hash verification on read.
    CorruptOrMissingNode { expected_hash: [u8; 32] },
    /// Head compare-and-swap failed because the persisted head differed from expected prior set.
    HeadCasConflict {
        expected: Option<[u8; 32]>,
        actual: Option<[u8; 32]>,
    },
    /// Deterministic simulated crash triggered at a test-requested [`CrashPoint`].
    SimulatedCrash(CrashPoint),
    /// Filesystem or lock I/O error during durable store operation.
    IoFailure { detail: String },
}

impl fmt::Display for CommitStateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongVersion { expected, actual } => {
                write!(f, "wrong version {actual} (expected {expected})")
            }
            Self::MalformedEncoding { reason } => write!(f, "malformed encoding: {reason}"),
            Self::LimitExceeded { field } => write!(f, "limit exceeded or invalid: {field}"),
            Self::LocalCapWidensPolicy { field } => {
                write!(f, "local cap widens admitted proof policy field {field}")
            }
            Self::RegistryMismatch { reason } => write!(f, "registry mismatch: {reason}"),
            Self::InvalidKeyOrValue { reason } => write!(f, "invalid key or value: {reason}"),
            Self::DuplicateKey => write!(f, "duplicate logical key in full rebuild"),
            Self::InvalidProof { reason } => write!(f, "invalid proof: {reason}"),
            Self::ClaimMismatch => write!(f, "verified lookup disagrees with claimed value"),
            Self::InvariantViolation { reason } => write!(f, "invariant violation: {reason}"),
            Self::CorruptOrMissingNode { expected_hash } => {
                write!(
                    f,
                    "corrupt or missing node {:02x}{:02x}..",
                    expected_hash[0], expected_hash[1]
                )
            }
            Self::HeadCasConflict { expected, actual } => {
                write!(
                    f,
                    "head CAS conflict (expected present={}, actual present={})",
                    expected.is_some(),
                    actual.is_some()
                )
            }
            Self::SimulatedCrash(point) => write!(f, "simulated crash at {point:?}"),
            Self::IoFailure { detail } => write!(f, "durable commitment store I/O error: {detail}"),
        }
    }
}

impl std::error::Error for CommitStateError {}

impl From<CoordinateEpisodeError> for CommitStateError {
    fn from(err: CoordinateEpisodeError) -> Self {
        Self::IoFailure {
            detail: format!("episode error: {err}"),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<DurableLedgerError> for CommitStateError {
    fn from(err: DurableLedgerError) -> Self {
        Self::IoFailure {
            detail: format!("durable ledger error: {err}"),
        }
    }
}

// =========================================================================
// BIG-ENDIAN WIRE READER HELPER
// =========================================================================

struct WireReader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> WireReader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn take(&mut self, n: usize, reason: &'static str) -> Result<&'a [u8], CommitStateError> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or(CommitStateError::MalformedEncoding { reason })?;
        if end > self.bytes.len() {
            return Err(CommitStateError::MalformedEncoding { reason });
        }
        let slice = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn u8(&mut self, reason: &'static str) -> Result<u8, CommitStateError> {
        Ok(self.take(1, reason)?[0])
    }

    fn u16(&mut self, reason: &'static str) -> Result<u16, CommitStateError> {
        let b = self.take(2, reason)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    fn u32(&mut self, reason: &'static str) -> Result<u32, CommitStateError> {
        let b = self.take(4, reason)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64(&mut self, reason: &'static str) -> Result<u64, CommitStateError> {
        let b = self.take(8, reason)?;
        Ok(u64::from_be_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    fn i64(&mut self, reason: &'static str) -> Result<i64, CommitStateError> {
        Ok(self.u64(reason)? as i64)
    }

    fn bytes32(&mut self, reason: &'static str) -> Result<[u8; 32], CommitStateError> {
        let b = self.take(32, reason)?;
        let mut out = [0u8; 32];
        out.copy_from_slice(b);
        Ok(out)
    }

    fn finish(&self) -> Result<(), CommitStateError> {
        if self.pos != self.bytes.len() {
            return Err(CommitStateError::MalformedEncoding {
                reason: "trailing bytes after canonical payload",
            });
        }
        Ok(())
    }
}

// =========================================================================
// DESCRIPTORS & IMMUTABLE PROFILE / DOMAIN REGISTRIES
// =========================================================================

/// Fixed-width numeric verifier policy descriptor (`COMMIT_1_PROOF_POLICY_SCHEMA`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProofPolicyDescriptorV1 {
    /// Descriptor schema version (must be `1`).
    pub schema_version: u16,
    /// Maximum total wire proof byte length admitted before decoding.
    pub max_proof_bytes: u32,
    /// Maximum total witness records (`steps.len() + 1` for non-empty, `0` for empty root).
    pub max_proof_records: u16,
    /// Maximum reconstructed key bit depth (`192` or `256`).
    pub max_reconstructed_depth: u16,
    /// Maximum `node_hash` invocations allowed during proof verification.
    pub max_hash_invocations: u16,
    /// Maximum total preimage bytes hashed across all `node_hash` calls in one proof.
    pub max_hashed_preimage_bytes: u32,
    /// Maximum encoded payload byte length of a single witness node.
    pub max_single_encoded_node_bytes: u32,
    /// Maximum summed encoded payload byte length across all witness nodes in one proof.
    pub max_total_encoded_node_bytes: u32,
}

impl ProofPolicyDescriptorV1 {
    /// Canonical byte length of an encoded `ProofPolicyDescriptorV1` (`24` bytes).
    pub const WIRE_LEN: usize = 2 + 4 + 2 + 2 + 2 + 4 + 4 + 4;

    /// Admitted local proof policy for `WorldRootV1` (192-bit coord -> 1-byte `OnV1`).
    pub const WORLD_DEFAULT: Self = Self {
        schema_version: COMMIT_1_SCHEMA_VERSION,
        max_proof_bytes: 8 * 1024,
        max_proof_records: (COORD_KEY_BITS + 1) as u16,
        max_reconstructed_depth: COORD_KEY_BITS as u16,
        max_hash_invocations: (COORD_KEY_BITS + 1) as u16,
        max_hashed_preimage_bytes: 32 * 1024,
        max_single_encoded_node_bytes: 128,
        max_total_encoded_node_bytes: 16 * 1024,
    };

    /// Admitted local proof policy for `ActiveTokenRootV1` (192-bit coord -> 32-byte `MintIdV1`).
    pub const ACTIVE_TOKEN_DEFAULT: Self = Self {
        schema_version: COMMIT_1_SCHEMA_VERSION,
        max_proof_bytes: 8 * 1024,
        max_proof_records: (COORD_KEY_BITS + 1) as u16,
        max_reconstructed_depth: COORD_KEY_BITS as u16,
        max_hash_invocations: (COORD_KEY_BITS + 1) as u16,
        max_hashed_preimage_bytes: 32 * 1024,
        max_single_encoded_node_bytes: 160,
        max_total_encoded_node_bytes: 16 * 1024,
    };

    /// Admitted local proof policy for `EpisodeRootV1` (256-bit event -> 57/89-byte `EpisodeRecordV1`).
    pub const EPISODE_DEFAULT: Self = Self {
        schema_version: COMMIT_1_SCHEMA_VERSION,
        max_proof_bytes: 12 * 1024,
        max_proof_records: (EVENT_KEY_BITS + 1) as u16,
        max_reconstructed_depth: EVENT_KEY_BITS as u16,
        max_hash_invocations: (EVENT_KEY_BITS + 1) as u16,
        max_hashed_preimage_bytes: 48 * 1024,
        max_single_encoded_node_bytes: 256,
        max_total_encoded_node_bytes: 24 * 1024,
    };

    /// Validates that every policy field is non-zero and structurally coherent.
    pub fn validate(&self) -> Result<(), CommitStateError> {
        if self.schema_version != COMMIT_1_SCHEMA_VERSION {
            return Err(CommitStateError::WrongVersion {
                expected: COMMIT_1_SCHEMA_VERSION,
                actual: self.schema_version,
            });
        }
        if self.max_proof_bytes == 0 {
            return Err(CommitStateError::LimitExceeded {
                field: "max_proof_bytes",
            });
        }
        if self.max_proof_records == 0 {
            return Err(CommitStateError::LimitExceeded {
                field: "max_proof_records",
            });
        }
        if self.max_reconstructed_depth == 0 {
            return Err(CommitStateError::LimitExceeded {
                field: "max_reconstructed_depth",
            });
        }
        if self.max_hash_invocations == 0 {
            return Err(CommitStateError::LimitExceeded {
                field: "max_hash_invocations",
            });
        }
        if self.max_hashed_preimage_bytes == 0 {
            return Err(CommitStateError::LimitExceeded {
                field: "max_hashed_preimage_bytes",
            });
        }
        if self.max_single_encoded_node_bytes == 0
            || self.max_single_encoded_node_bytes > self.max_total_encoded_node_bytes
        {
            return Err(CommitStateError::LimitExceeded {
                field: "max_single_encoded_node_bytes",
            });
        }
        Ok(())
    }

    /// Applies an optional machine-local numeric cap; rejects any attempt to widen a field.
    pub fn tighten(&self, local_cap: Self) -> Result<Self, CommitStateError> {
        local_cap.validate()?;
        if local_cap.max_proof_bytes > self.max_proof_bytes {
            return Err(CommitStateError::LocalCapWidensPolicy {
                field: "max_proof_bytes",
            });
        }
        if local_cap.max_proof_records > self.max_proof_records {
            return Err(CommitStateError::LocalCapWidensPolicy {
                field: "max_proof_records",
            });
        }
        if local_cap.max_reconstructed_depth > self.max_reconstructed_depth {
            return Err(CommitStateError::LocalCapWidensPolicy {
                field: "max_reconstructed_depth",
            });
        }
        if local_cap.max_hash_invocations > self.max_hash_invocations {
            return Err(CommitStateError::LocalCapWidensPolicy {
                field: "max_hash_invocations",
            });
        }
        if local_cap.max_hashed_preimage_bytes > self.max_hashed_preimage_bytes {
            return Err(CommitStateError::LocalCapWidensPolicy {
                field: "max_hashed_preimage_bytes",
            });
        }
        if local_cap.max_single_encoded_node_bytes > self.max_single_encoded_node_bytes {
            return Err(CommitStateError::LocalCapWidensPolicy {
                field: "max_single_encoded_node_bytes",
            });
        }
        if local_cap.max_total_encoded_node_bytes > self.max_total_encoded_node_bytes {
            return Err(CommitStateError::LocalCapWidensPolicy {
                field: "max_total_encoded_node_bytes",
            });
        }
        Ok(local_cap)
    }

    /// Encodes this proof policy descriptor into exact big-endian bytes.
    pub fn encode_canonical(&self) -> [u8; Self::WIRE_LEN] {
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.schema_version.to_be_bytes());
        out[2..6].copy_from_slice(&self.max_proof_bytes.to_be_bytes());
        out[6..8].copy_from_slice(&self.max_proof_records.to_be_bytes());
        out[8..10].copy_from_slice(&self.max_reconstructed_depth.to_be_bytes());
        out[10..12].copy_from_slice(&self.max_hash_invocations.to_be_bytes());
        out[12..16].copy_from_slice(&self.max_hashed_preimage_bytes.to_be_bytes());
        out[16..20].copy_from_slice(&self.max_single_encoded_node_bytes.to_be_bytes());
        out[20..24].copy_from_slice(&self.max_total_encoded_node_bytes.to_be_bytes());
        out
    }

    /// Decodes and validates a canonical `ProofPolicyDescriptorV1`.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, CommitStateError> {
        let mut r = WireReader::new(bytes);
        let desc = Self {
            schema_version: r.u16("proof policy schema_version")?,
            max_proof_bytes: r.u32("proof policy max_proof_bytes")?,
            max_proof_records: r.u16("proof policy max_proof_records")?,
            max_reconstructed_depth: r.u16("proof policy max_reconstructed_depth")?,
            max_hash_invocations: r.u16("proof policy max_hash_invocations")?,
            max_hashed_preimage_bytes: r.u32("proof policy max_hashed_preimage_bytes")?,
            max_single_encoded_node_bytes: r.u32("proof policy max_single_encoded_node_bytes")?,
            max_total_encoded_node_bytes: r.u32("proof policy max_total_encoded_node_bytes")?,
        };
        r.finish()?;
        desc.validate()?;
        Ok(desc)
    }

    /// Returns the canonical `Policy` `ContentTag` for this descriptor.
    pub fn tag(&self) -> ContentTag {
        let bytes = self.encode_canonical();
        hash_canonical(
            TagKind::Policy,
            COMMIT_1_PROOF_POLICY_SCHEMA,
            bytes.len() as u64,
            |sink| sink(&bytes),
        )
        .expect("proof policy tag is canonical")
    }

    /// Returns the 32-byte `ProofPolicyIdV1` digest.
    pub fn id(&self) -> [u8; 32] {
        self.tag().digest()
    }
}

/// Fixed-width numeric operational-limits descriptor (`COMMIT_1_OP_LIMITS_SCHEMA`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommitmentOperationalLimitsDescriptorV1 {
    /// Descriptor schema version (must be `1`).
    pub schema_version: u16,
    /// Maximum keys admitted in one full rebuild.
    pub max_full_build_keys: u64,
    /// Maximum updates in one batch transition.
    pub max_batch_updates: u32,
    /// Maximum encoded byte length of one stored node record.
    pub max_node_store_record_bytes: u32,
    /// Maximum nodes staged in a single publication.
    pub max_staged_nodes: u64,
    /// Maximum total bytes staged in a single publication.
    pub max_staged_bytes: u64,
    /// Maximum concurrently pinned permanent roots.
    pub max_pinned_roots: u32,
    /// Maximum concurrently retained provisional pin records.
    pub max_provisional_roots: u32,
    /// Maximum records scanned during recovery.
    pub max_recovery_scan_records: u64,
    /// Maximum nodes marked in one bounded GC mark pass.
    pub max_gc_mark_nodes: u64,
    /// Maximum nodes reclaimed in one bounded GC sweep pass.
    pub max_gc_sweep_nodes: u64,
    /// Maximum deterministic GC work units per cycle.
    pub max_gc_work_units: u64,
}

impl CommitmentOperationalLimitsDescriptorV1 {
    /// Canonical byte length of an encoded `CommitmentOperationalLimitsDescriptorV1` (`74` bytes).
    pub const WIRE_LEN: usize = 2 + 8 + 4 + 4 + 8 + 8 + 4 + 4 + 8 + 8 + 8 + 8;

    /// Admitted default operational limits for local `COMMIT-1` commitment stores.
    pub const LOCAL_DEFAULT: Self = Self {
        schema_version: COMMIT_1_SCHEMA_VERSION,
        max_full_build_keys: 262_144,
        max_batch_updates: 65_536,
        max_node_store_record_bytes: 512,
        max_staged_nodes: 524_288,
        max_staged_bytes: 64 * 1024 * 1024,
        max_pinned_roots: 4_096,
        max_provisional_roots: 64,
        max_recovery_scan_records: 524_288,
        max_gc_mark_nodes: 524_288,
        max_gc_sweep_nodes: 65_536,
        max_gc_work_units: 1_048_576,
    };

    /// Validates that every operational limit is non-zero and valid.
    pub fn validate(&self) -> Result<(), CommitStateError> {
        if self.schema_version != COMMIT_1_SCHEMA_VERSION {
            return Err(CommitStateError::WrongVersion {
                expected: COMMIT_1_SCHEMA_VERSION,
                actual: self.schema_version,
            });
        }
        let non_zero = [
            (self.max_full_build_keys, "max_full_build_keys"),
            (self.max_batch_updates as u64, "max_batch_updates"),
            (
                self.max_node_store_record_bytes as u64,
                "max_node_store_record_bytes",
            ),
            (self.max_staged_nodes, "max_staged_nodes"),
            (self.max_staged_bytes, "max_staged_bytes"),
            (self.max_pinned_roots as u64, "max_pinned_roots"),
            (self.max_provisional_roots as u64, "max_provisional_roots"),
            (self.max_recovery_scan_records, "max_recovery_scan_records"),
            (self.max_gc_mark_nodes, "max_gc_mark_nodes"),
            (self.max_gc_sweep_nodes, "max_gc_sweep_nodes"),
            (self.max_gc_work_units, "max_gc_work_units"),
        ];
        for (value, field) in non_zero {
            if value == 0 {
                return Err(CommitStateError::LimitExceeded { field });
            }
        }
        Ok(())
    }

    /// Encodes this operational-limits descriptor into 74 exact big-endian bytes.
    pub fn encode_canonical(&self) -> [u8; Self::WIRE_LEN] {
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.schema_version.to_be_bytes());
        out[2..10].copy_from_slice(&self.max_full_build_keys.to_be_bytes());
        out[10..14].copy_from_slice(&self.max_batch_updates.to_be_bytes());
        out[14..18].copy_from_slice(&self.max_node_store_record_bytes.to_be_bytes());
        out[18..26].copy_from_slice(&self.max_staged_nodes.to_be_bytes());
        out[26..34].copy_from_slice(&self.max_staged_bytes.to_be_bytes());
        out[34..38].copy_from_slice(&self.max_pinned_roots.to_be_bytes());
        out[38..42].copy_from_slice(&self.max_provisional_roots.to_be_bytes());
        out[42..50].copy_from_slice(&self.max_recovery_scan_records.to_be_bytes());
        out[50..58].copy_from_slice(&self.max_gc_mark_nodes.to_be_bytes());
        out[58..66].copy_from_slice(&self.max_gc_sweep_nodes.to_be_bytes());
        out[66..74].copy_from_slice(&self.max_gc_work_units.to_be_bytes());
        out
    }

    /// Decodes and validates a canonical `CommitmentOperationalLimitsDescriptorV1`.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, CommitStateError> {
        let mut r = WireReader::new(bytes);
        let desc = Self {
            schema_version: r.u16("op limits schema_version")?,
            max_full_build_keys: r.u64("op limits max_full_build_keys")?,
            max_batch_updates: r.u32("op limits max_batch_updates")?,
            max_node_store_record_bytes: r.u32("op limits max_node_store_record_bytes")?,
            max_staged_nodes: r.u64("op limits max_staged_nodes")?,
            max_staged_bytes: r.u64("op limits max_staged_bytes")?,
            max_pinned_roots: r.u32("op limits max_pinned_roots")?,
            max_provisional_roots: r.u32("op limits max_provisional_roots")?,
            max_recovery_scan_records: r.u64("op limits max_recovery_scan_records")?,
            max_gc_mark_nodes: r.u64("op limits max_gc_mark_nodes")?,
            max_gc_sweep_nodes: r.u64("op limits max_gc_sweep_nodes")?,
            max_gc_work_units: r.u64("op limits max_gc_work_units")?,
        };
        r.finish()?;
        desc.validate()?;
        Ok(desc)
    }

    /// Returns the canonical `Policy` `ContentTag` for this operational-limits descriptor.
    pub fn tag(&self) -> ContentTag {
        let bytes = self.encode_canonical();
        hash_canonical(
            TagKind::Policy,
            COMMIT_1_OP_LIMITS_SCHEMA,
            bytes.len() as u64,
            |sink| sink(&bytes),
        )
        .expect("operational limits tag is canonical")
    }

    /// Returns the 32-byte `CommitmentOperationalLimitsIdV1` digest.
    pub fn id(&self) -> [u8; 32] {
        self.tag().digest()
    }
}

/// Non-self-referential descriptor fixing one authenticated map domain (`COMMIT_1_PROFILE_SCHEMA`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommitmentProfileDescriptorV1 {
    /// Descriptor schema version (must be `1`).
    pub schema_version: u16,
    /// Authenticated tree algorithm (`BinaryMerklePatriciaV1`).
    pub tree_algorithm: TreeAlgorithmIdV1,
    /// Typed root wrapper family.
    pub root_type: RootTypeV1,
    /// Logical state domain tag.
    pub domain_tag: DomainTagV1,
    /// Fixed-width key codec.
    pub key_codec: KeyCodecIdV1,
    /// Exact key bit width (`192` or `256`).
    pub key_bit_width: u16,
    /// Typed value codec.
    pub value_codec: ValueCodecIdV1,
    /// Hash suite (`Sha256V1`).
    pub hash_suite: HashSuiteIdV1,
    /// Digest length in bytes (`32`).
    pub digest_byte_length: u8,
    /// Node codec version (`1`).
    pub node_codec_version: u16,
    /// Root codec version (`1`).
    pub root_codec_version: u16,
    /// Empty-root derivation rule.
    pub empty_root_rule: EmptyRootRuleIdV1,
    /// Derived `ProofPolicyIdV1` governing proof verification for this profile.
    pub proof_policy_id: [u8; 32],
    /// Derived `CommitmentOperationalLimitsIdV1` governing store/GC work for this profile.
    pub operational_limits_id: [u8; 32],
}

impl CommitmentProfileDescriptorV1 {
    /// Returns the canonical `WorldRootV1` profile descriptor.
    pub fn world_v1() -> Self {
        Self {
            schema_version: COMMIT_1_SCHEMA_VERSION,
            tree_algorithm: TreeAlgorithmIdV1::BinaryMerklePatriciaV1,
            root_type: RootTypeV1::WorldRoot,
            domain_tag: DomainTagV1::WorldOccupancy,
            key_codec: KeyCodecIdV1::CoordinateKeyV1,
            key_bit_width: COORD_KEY_BITS as u16,
            value_codec: ValueCodecIdV1::OnV1,
            hash_suite: HashSuiteIdV1::Sha256V1,
            digest_byte_length: 32,
            node_codec_version: COMMIT_1_NODE_CODEC_VERSION,
            root_codec_version: COMMIT_1_ROOT_CODEC_VERSION,
            empty_root_rule: EmptyRootRuleIdV1::MptEmptyDepthZeroV1,
            proof_policy_id: ProofPolicyDescriptorV1::WORLD_DEFAULT.id(),
            operational_limits_id: CommitmentOperationalLimitsDescriptorV1::LOCAL_DEFAULT.id(),
        }
    }

    /// Returns the canonical `ActiveTokenRootV1` profile descriptor.
    pub fn active_token_v1() -> Self {
        Self {
            schema_version: COMMIT_1_SCHEMA_VERSION,
            tree_algorithm: TreeAlgorithmIdV1::BinaryMerklePatriciaV1,
            root_type: RootTypeV1::ActiveTokenRoot,
            domain_tag: DomainTagV1::ActiveToken,
            key_codec: KeyCodecIdV1::CoordinateKeyV1,
            key_bit_width: COORD_KEY_BITS as u16,
            value_codec: ValueCodecIdV1::MintIdV1,
            hash_suite: HashSuiteIdV1::Sha256V1,
            digest_byte_length: 32,
            node_codec_version: COMMIT_1_NODE_CODEC_VERSION,
            root_codec_version: COMMIT_1_ROOT_CODEC_VERSION,
            empty_root_rule: EmptyRootRuleIdV1::MptEmptyDepthZeroV1,
            proof_policy_id: ProofPolicyDescriptorV1::ACTIVE_TOKEN_DEFAULT.id(),
            operational_limits_id: CommitmentOperationalLimitsDescriptorV1::LOCAL_DEFAULT.id(),
        }
    }

    /// Returns the canonical `EpisodeRootV1` profile descriptor.
    pub fn episode_v1() -> Self {
        Self {
            schema_version: COMMIT_1_SCHEMA_VERSION,
            tree_algorithm: TreeAlgorithmIdV1::BinaryMerklePatriciaV1,
            root_type: RootTypeV1::EpisodeRoot,
            domain_tag: DomainTagV1::Episode,
            key_codec: KeyCodecIdV1::EventIdV1,
            key_bit_width: EVENT_KEY_BITS as u16,
            value_codec: ValueCodecIdV1::EpisodeRecordV1,
            hash_suite: HashSuiteIdV1::Sha256V1,
            digest_byte_length: 32,
            node_codec_version: COMMIT_1_NODE_CODEC_VERSION,
            root_codec_version: COMMIT_1_ROOT_CODEC_VERSION,
            empty_root_rule: EmptyRootRuleIdV1::MptEmptyDepthZeroV1,
            proof_policy_id: ProofPolicyDescriptorV1::EPISODE_DEFAULT.id(),
            operational_limits_id: CommitmentOperationalLimitsDescriptorV1::LOCAL_DEFAULT.id(),
        }
    }

    /// Returns the canonical tree-domain schema bytes bound into every node hash.
    pub const fn tree_domain(&self) -> &'static [u8] {
        self.root_type.schema_bytes()
    }

    /// Validates internal coherence of this profile descriptor.
    pub fn validate(&self) -> Result<(), CommitStateError> {
        if self.schema_version != COMMIT_1_SCHEMA_VERSION {
            return Err(CommitStateError::WrongVersion {
                expected: COMMIT_1_SCHEMA_VERSION,
                actual: self.schema_version,
            });
        }
        if self.node_codec_version != COMMIT_1_NODE_CODEC_VERSION {
            return Err(CommitStateError::WrongVersion {
                expected: COMMIT_1_NODE_CODEC_VERSION,
                actual: self.node_codec_version,
            });
        }
        if self.root_codec_version != COMMIT_1_ROOT_CODEC_VERSION {
            return Err(CommitStateError::WrongVersion {
                expected: COMMIT_1_ROOT_CODEC_VERSION,
                actual: self.root_codec_version,
            });
        }
        if self.digest_byte_length != 32 {
            return Err(CommitStateError::RegistryMismatch {
                reason: "digest_byte_length must be 32 for sha256",
            });
        }
        if self.key_bit_width != self.key_codec.bit_width() {
            return Err(CommitStateError::RegistryMismatch {
                reason: "key_bit_width disagrees with key_codec",
            });
        }
        if self.domain_tag != self.root_type.domain_tag() {
            return Err(CommitStateError::RegistryMismatch {
                reason: "domain_tag disagrees with root_type",
            });
        }
        let expected_pair = match self.root_type {
            RootTypeV1::WorldRoot => (KeyCodecIdV1::CoordinateKeyV1, ValueCodecIdV1::OnV1),
            RootTypeV1::ActiveTokenRoot => {
                (KeyCodecIdV1::CoordinateKeyV1, ValueCodecIdV1::MintIdV1)
            }
            RootTypeV1::EpisodeRoot => (KeyCodecIdV1::EventIdV1, ValueCodecIdV1::EpisodeRecordV1),
        };
        if (self.key_codec, self.value_codec) != expected_pair {
            return Err(CommitStateError::RegistryMismatch {
                reason: "key_codec/value_codec disagrees with root_type",
            });
        }
        Ok(())
    }

    /// Encodes this profile descriptor into canonical big-endian bytes.
    pub fn encode_canonical(&self) -> Vec<u8> {
        let domain = self.tree_domain();
        let mut out = Vec::with_capacity(82 + domain.len());
        out.extend_from_slice(&self.schema_version.to_be_bytes());
        out.push(self.tree_algorithm.code());
        out.push(self.root_type.code());
        out.push(self.domain_tag.code());
        out.extend_from_slice(&(domain.len() as u16).to_be_bytes());
        out.extend_from_slice(domain);
        out.push(self.key_codec.code());
        out.extend_from_slice(&self.key_bit_width.to_be_bytes());
        out.push(self.value_codec.code());
        out.push(self.hash_suite.code());
        out.push(self.digest_byte_length);
        out.extend_from_slice(&self.node_codec_version.to_be_bytes());
        out.extend_from_slice(&self.root_codec_version.to_be_bytes());
        out.push(self.empty_root_rule.code());
        out.extend_from_slice(&self.proof_policy_id);
        out.extend_from_slice(&self.operational_limits_id);
        out
    }

    /// Decodes and validates a canonical `CommitmentProfileDescriptorV1`.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, CommitStateError> {
        let mut r = WireReader::new(bytes);
        let schema_version = r.u16("profile schema_version")?;
        let tree_algorithm = TreeAlgorithmIdV1::from_code(r.u8("profile tree_algorithm")?).ok_or(
            CommitStateError::MalformedEncoding {
                reason: "unknown tree_algorithm",
            },
        )?;
        let root_type = RootTypeV1::from_code(r.u8("profile root_type")?).ok_or(
            CommitStateError::MalformedEncoding {
                reason: "unknown root_type",
            },
        )?;
        let domain_tag = DomainTagV1::from_code(r.u8("profile domain_tag")?).ok_or(
            CommitStateError::MalformedEncoding {
                reason: "unknown domain_tag",
            },
        )?;
        let domain_len = r.u16("profile tree_domain len")? as usize;
        let domain_bytes = r.take(domain_len, "profile tree_domain bytes")?;
        if domain_bytes != root_type.schema_bytes() {
            return Err(CommitStateError::RegistryMismatch {
                reason: "tree_domain bytes disagree with root_type",
            });
        }
        let key_codec = KeyCodecIdV1::from_code(r.u8("profile key_codec")?).ok_or(
            CommitStateError::MalformedEncoding {
                reason: "unknown key_codec",
            },
        )?;
        let key_bit_width = r.u16("profile key_bit_width")?;
        let value_codec = ValueCodecIdV1::from_code(r.u8("profile value_codec")?).ok_or(
            CommitStateError::MalformedEncoding {
                reason: "unknown value_codec",
            },
        )?;
        let hash_suite = HashSuiteIdV1::from_code(r.u8("profile hash_suite")?).ok_or(
            CommitStateError::MalformedEncoding {
                reason: "unknown hash_suite",
            },
        )?;
        let digest_byte_length = r.u8("profile digest_byte_length")?;
        let node_codec_version = r.u16("profile node_codec_version")?;
        let root_codec_version = r.u16("profile root_codec_version")?;
        let empty_root_rule = EmptyRootRuleIdV1::from_code(r.u8("profile empty_root_rule")?)
            .ok_or(CommitStateError::MalformedEncoding {
                reason: "unknown empty_root_rule",
            })?;
        let proof_policy_id = r.bytes32("profile proof_policy_id")?;
        let operational_limits_id = r.bytes32("profile operational_limits_id")?;
        r.finish()?;
        let desc = Self {
            schema_version,
            tree_algorithm,
            root_type,
            domain_tag,
            key_codec,
            key_bit_width,
            value_codec,
            hash_suite,
            digest_byte_length,
            node_codec_version,
            root_codec_version,
            empty_root_rule,
            proof_policy_id,
            operational_limits_id,
        };
        desc.validate()?;
        Ok(desc)
    }

    /// Returns the canonical `Policy` `ContentTag` for this commitment profile.
    pub fn tag(&self) -> ContentTag {
        let bytes = self.encode_canonical();
        hash_canonical(
            TagKind::Policy,
            COMMIT_1_PROFILE_SCHEMA,
            bytes.len() as u64,
            |sink| sink(&bytes),
        )
        .expect("commitment profile tag is canonical")
    }

    /// Returns the 32-byte `CommitmentProfileIdV1` digest.
    pub fn id(&self) -> [u8; 32] {
        self.tag().digest()
    }
}

/// Active-domain identity projected from [`DomainRegistryV1`] into [`CommitmentSetV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RequiredDomainV1 {
    /// Logical state domain tag.
    pub domain_tag: DomainTagV1,
    /// Composition layer owning this domain.
    pub composition_layer: CompositionLayerV1,
    /// Typed root wrapper family.
    pub root_type: RootTypeV1,
    /// Resolved `CommitmentProfileIdV1` digest.
    pub commitment_profile_id: [u8; 32],
}

impl RequiredDomainV1 {
    /// Encodes this required-domain record into canonical big-endian bytes.
    pub fn encode_canonical(&self, out: &mut Vec<u8>) {
        let schema = self.root_type.schema_bytes();
        out.push(self.domain_tag.code());
        out.push(self.composition_layer.code());
        out.push(self.root_type.code());
        out.extend_from_slice(&(schema.len() as u16).to_be_bytes());
        out.extend_from_slice(schema);
        out.extend_from_slice(&self.commitment_profile_id);
    }

    fn decode_from(r: &mut WireReader<'_>) -> Result<Self, CommitStateError> {
        let domain_tag = DomainTagV1::from_code(r.u8("required domain_tag")?).ok_or(
            CommitStateError::MalformedEncoding {
                reason: "unknown required domain_tag",
            },
        )?;
        let composition_layer = CompositionLayerV1::from_code(r.u8("required composition_layer")?)
            .ok_or(CommitStateError::MalformedEncoding {
                reason: "unknown required composition_layer",
            })?;
        let root_type = RootTypeV1::from_code(r.u8("required root_type")?).ok_or(
            CommitStateError::MalformedEncoding {
                reason: "unknown required root_type",
            },
        )?;
        let schema_len = r.u16("required root_schema len")? as usize;
        let schema = r.take(schema_len, "required root_schema bytes")?;
        if schema != root_type.schema_bytes() {
            return Err(CommitStateError::RegistryMismatch {
                reason: "required-domain schema disagrees with root_type",
            });
        }
        let commitment_profile_id = r.bytes32("required commitment_profile_id")?;
        if domain_tag != root_type.domain_tag() {
            return Err(CommitStateError::RegistryMismatch {
                reason: "required-domain tag disagrees with root_type",
            });
        }
        Ok(Self {
            domain_tag,
            composition_layer,
            root_type,
            commitment_profile_id,
        })
    }
}

/// One active or explicitly inactive entry in [`DomainRegistryV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DomainRegistryEntryV1 {
    /// Active domain carrying its complete [`RequiredDomainV1`] identity.
    Active(RequiredDomainV1),
    /// Explicitly inactive domain carrying a typed sentinel (never an empty root).
    Inactive {
        /// Logical state domain tag.
        domain_tag: DomainTagV1,
        /// Composition layer owning this reserved domain.
        composition_layer: CompositionLayerV1,
        /// Typed inactive sentinel identifier.
        sentinel: InactiveSentinelIdV1,
    },
}

impl DomainRegistryEntryV1 {
    /// Returns the `DomainTagV1` key of this registry entry.
    pub const fn domain_tag(&self) -> DomainTagV1 {
        match *self {
            Self::Active(req) => req.domain_tag,
            Self::Inactive { domain_tag, .. } => domain_tag,
        }
    }
}

/// Pre-ID canonically ordered domain activation registry (`COMMIT_1_DOMAIN_REGISTRY_SCHEMA`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DomainRegistryV1 {
    entries: Vec<DomainRegistryEntryV1>,
}

impl DomainRegistryV1 {
    /// Constructs and validates a `DomainRegistryV1`, requiring strictly ascending unique `domain_tag`s.
    pub fn new(entries: Vec<DomainRegistryEntryV1>) -> Result<Self, CommitStateError> {
        if entries.is_empty() {
            return Err(CommitStateError::RegistryMismatch {
                reason: "domain registry must not be empty",
            });
        }
        for window in entries.windows(2) {
            if window[0].domain_tag() >= window[1].domain_tag() {
                return Err(CommitStateError::RegistryMismatch {
                    reason: "domain registry entries must be strictly ascending and unique",
                });
            }
        }
        Ok(Self { entries })
    }

    /// Returns the canonical local `COMMIT-1` registry activating World, ActiveToken, and Episode
    /// while recording Authorization, Nonce, and AssetState as explicit inactive sentinels.
    pub fn local_v1() -> Self {
        Self::new(vec![
            DomainRegistryEntryV1::Active(RequiredDomainV1 {
                domain_tag: DomainTagV1::WorldOccupancy,
                composition_layer: CompositionLayerV1::LocalState,
                root_type: RootTypeV1::WorldRoot,
                commitment_profile_id: CommitmentProfileDescriptorV1::world_v1().id(),
            }),
            DomainRegistryEntryV1::Active(RequiredDomainV1 {
                domain_tag: DomainTagV1::ActiveToken,
                composition_layer: CompositionLayerV1::LocalState,
                root_type: RootTypeV1::ActiveTokenRoot,
                commitment_profile_id: CommitmentProfileDescriptorV1::active_token_v1().id(),
            }),
            DomainRegistryEntryV1::Active(RequiredDomainV1 {
                domain_tag: DomainTagV1::Episode,
                composition_layer: CompositionLayerV1::LocalState,
                root_type: RootTypeV1::EpisodeRoot,
                commitment_profile_id: CommitmentProfileDescriptorV1::episode_v1().id(),
            }),
            DomainRegistryEntryV1::Inactive {
                domain_tag: DomainTagV1::Authorization,
                composition_layer: CompositionLayerV1::NetworkAuthority,
                sentinel: InactiveSentinelIdV1::UnactivatedLocalProfileV1,
            },
            DomainRegistryEntryV1::Inactive {
                domain_tag: DomainTagV1::Nonce,
                composition_layer: CompositionLayerV1::NetworkAuthority,
                sentinel: InactiveSentinelIdV1::UnactivatedLocalProfileV1,
            },
            DomainRegistryEntryV1::Inactive {
                domain_tag: DomainTagV1::AssetState,
                composition_layer: CompositionLayerV1::OptionalExtension,
                sentinel: InactiveSentinelIdV1::UnactivatedLocalProfileV1,
            },
        ])
        .expect("local_v1 domain registry is canonical")
    }

    /// Returns all entries in canonical ascending `domain_tag` order.
    pub fn entries(&self) -> &[DomainRegistryEntryV1] {
        &self.entries
    }

    /// Projects the canonically sorted list of active [`RequiredDomainV1`] slots.
    pub fn required_domains(&self) -> Vec<RequiredDomainV1> {
        self.entries
            .iter()
            .filter_map(|e| match *e {
                DomainRegistryEntryV1::Active(req) => Some(req),
                DomainRegistryEntryV1::Inactive { .. } => None,
            })
            .collect()
    }

    /// Returns `true` if `domain` is active in this registry.
    pub fn is_active(&self, domain: DomainTagV1) -> bool {
        self.entries
            .iter()
            .any(|e| matches!(e, DomainRegistryEntryV1::Active(r) if r.domain_tag == domain))
    }

    /// Validates every active entry against `profiles`.
    pub fn validate_against(&self, profiles: &ProfileRegistryV1) -> Result<(), CommitStateError> {
        for entry in &self.entries {
            if let DomainRegistryEntryV1::Active(req) = *entry {
                let profile = profiles.resolve_profile(&req.commitment_profile_id)?;
                if profile.domain_tag != req.domain_tag || profile.root_type != req.root_type {
                    return Err(CommitStateError::RegistryMismatch {
                        reason: "active registry entry disagrees with resolved commitment profile",
                    });
                }
            }
        }
        Ok(())
    }

    /// Encodes this domain registry into canonical big-endian bytes.
    pub fn encode_canonical(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&COMMIT_1_SCHEMA_VERSION.to_be_bytes());
        out.extend_from_slice(&(self.entries.len() as u16).to_be_bytes());
        for entry in &self.entries {
            match *entry {
                DomainRegistryEntryV1::Active(req) => {
                    out.push(REGISTRY_ENTRY_ACTIVE);
                    req.encode_canonical(&mut out);
                }
                DomainRegistryEntryV1::Inactive {
                    domain_tag,
                    composition_layer,
                    sentinel,
                } => {
                    out.push(REGISTRY_ENTRY_INACTIVE);
                    out.push(domain_tag.code());
                    out.push(composition_layer.code());
                    out.push(sentinel.code());
                }
            }
        }
        out
    }

    /// Decodes and validates a canonical `DomainRegistryV1`.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, CommitStateError> {
        let mut r = WireReader::new(bytes);
        let ver = r.u16("domain registry version")?;
        if ver != COMMIT_1_SCHEMA_VERSION {
            return Err(CommitStateError::WrongVersion {
                expected: COMMIT_1_SCHEMA_VERSION,
                actual: ver,
            });
        }
        let count = r.u16("domain registry count")? as usize;
        let mut entries = Vec::with_capacity(count.min(16));
        for _ in 0..count {
            match r.u8("domain registry variant")? {
                REGISTRY_ENTRY_ACTIVE => {
                    entries.push(DomainRegistryEntryV1::Active(
                        RequiredDomainV1::decode_from(&mut r)?,
                    ));
                }
                REGISTRY_ENTRY_INACTIVE => {
                    let domain_tag = DomainTagV1::from_code(r.u8("inactive domain_tag")?).ok_or(
                        CommitStateError::MalformedEncoding {
                            reason: "unknown inactive domain_tag",
                        },
                    )?;
                    let composition_layer = CompositionLayerV1::from_code(
                        r.u8("inactive composition_layer")?,
                    )
                    .ok_or(CommitStateError::MalformedEncoding {
                        reason: "unknown inactive composition_layer",
                    })?;
                    let sentinel = InactiveSentinelIdV1::from_code(r.u8("inactive sentinel")?)
                        .ok_or(CommitStateError::MalformedEncoding {
                            reason: "unknown inactive sentinel",
                        })?;
                    entries.push(DomainRegistryEntryV1::Inactive {
                        domain_tag,
                        composition_layer,
                        sentinel,
                    });
                }
                _ => {
                    return Err(CommitStateError::MalformedEncoding {
                        reason: "unknown domain registry variant tag",
                    });
                }
            }
        }
        r.finish()?;
        Self::new(entries)
    }

    /// Returns the canonical `Policy` `ContentTag` for this domain registry.
    pub fn tag(&self) -> ContentTag {
        let bytes = self.encode_canonical();
        hash_canonical(
            TagKind::Policy,
            COMMIT_1_DOMAIN_REGISTRY_SCHEMA,
            bytes.len() as u64,
            |sink| sink(&bytes),
        )
        .expect("domain registry tag is canonical")
    }

    /// Returns the 32-byte `DomainRegistryDigestV1`.
    pub fn digest(&self) -> [u8; 32] {
        self.tag().digest()
    }
}

/// Immutable registry resolving admitted `COMMIT-1` proof policies, limits, and profiles by ID.
#[derive(Clone, Debug)]
pub struct ProfileRegistryV1 {
    proof_policies: BTreeMap<[u8; 32], ProofPolicyDescriptorV1>,
    op_limits: BTreeMap<[u8; 32], CommitmentOperationalLimitsDescriptorV1>,
    profiles: BTreeMap<[u8; 32], CommitmentProfileDescriptorV1>,
}

impl ProfileRegistryV1 {
    /// Constructs the immutable local `COMMIT-1` registry admitting World, ActiveToken, and Episode.
    pub fn local_v1() -> Self {
        let mut proof_policies = BTreeMap::new();
        for pp in [
            ProofPolicyDescriptorV1::WORLD_DEFAULT,
            ProofPolicyDescriptorV1::ACTIVE_TOKEN_DEFAULT,
            ProofPolicyDescriptorV1::EPISODE_DEFAULT,
        ] {
            proof_policies.insert(pp.id(), pp);
        }
        let mut op_limits = BTreeMap::new();
        let limits = CommitmentOperationalLimitsDescriptorV1::LOCAL_DEFAULT;
        op_limits.insert(limits.id(), limits);

        let mut profiles = BTreeMap::new();
        for prof in [
            CommitmentProfileDescriptorV1::world_v1(),
            CommitmentProfileDescriptorV1::active_token_v1(),
            CommitmentProfileDescriptorV1::episode_v1(),
        ] {
            profiles.insert(prof.id(), prof);
        }
        Self {
            proof_policies,
            op_limits,
            profiles,
        }
    }

    /// Resolves a `CommitmentProfileDescriptorV1` by its derived `CommitmentProfileIdV1`.
    pub fn resolve_profile(
        &self,
        id: &[u8; 32],
    ) -> Result<&CommitmentProfileDescriptorV1, CommitStateError> {
        let profile = self
            .profiles
            .get(id)
            .ok_or(CommitStateError::RegistryMismatch {
                reason: "unknown commitment_profile_id",
            })?;
        if &profile.id() != id {
            return Err(CommitStateError::RegistryMismatch {
                reason: "commitment_profile_id recomputation mismatch",
            });
        }
        Ok(profile)
    }

    /// Resolves a `ProofPolicyDescriptorV1` by its derived `ProofPolicyIdV1`.
    pub fn resolve_proof_policy(
        &self,
        id: &[u8; 32],
    ) -> Result<&ProofPolicyDescriptorV1, CommitStateError> {
        let policy = self
            .proof_policies
            .get(id)
            .ok_or(CommitStateError::RegistryMismatch {
                reason: "unknown proof_policy_id",
            })?;
        if &policy.id() != id {
            return Err(CommitStateError::RegistryMismatch {
                reason: "proof_policy_id recomputation mismatch",
            });
        }
        Ok(policy)
    }

    /// Resolves a `CommitmentOperationalLimitsDescriptorV1` by its derived ID.
    pub fn resolve_op_limits(
        &self,
        id: &[u8; 32],
    ) -> Result<&CommitmentOperationalLimitsDescriptorV1, CommitStateError> {
        let limits = self
            .op_limits
            .get(id)
            .ok_or(CommitStateError::RegistryMismatch {
                reason: "unknown operational_limits_id",
            })?;
        if &limits.id() != id {
            return Err(CommitStateError::RegistryMismatch {
                reason: "operational_limits_id recomputation mismatch",
            });
        }
        Ok(limits)
    }
}

// =========================================================================
// TYPED KEYS, VALUES, AND ROOTS
// =========================================================================

/// Fixed 256-bit event identifier keying [`EpisodeRootV1`] (raw SHA-256 digest of a `TagKind::Event`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventIdV1([u8; EVENT_KEY_BYTES]);

impl EventIdV1 {
    /// Constructs an `EventIdV1` from an `Event`-kind `ContentTag`, rejecting all other kinds.
    pub fn from_event_tag(tag: ContentTag) -> Result<Self, CommitStateError> {
        if tag.kind() != TagKind::Event {
            return Err(CommitStateError::InvalidKeyOrValue {
                reason: "EventIdV1 requires TagKind::Event",
            });
        }
        Ok(Self(tag.digest()))
    }

    /// Wraps raw 32-byte digest bytes as an `EventIdV1`.
    pub const fn from_bytes(bytes: [u8; EVENT_KEY_BYTES]) -> Self {
        Self(bytes)
    }

    /// Borrows the 32-byte big-endian key bytes.
    pub const fn as_bytes(&self) -> &[u8; EVENT_KEY_BYTES] {
        &self.0
    }
}

/// Fixed 32-byte live mint identifier stored as the value of [`ActiveTokenRootV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MintIdV1([u8; EVENT_KEY_BYTES]);

impl MintIdV1 {
    /// Constructs a `MintIdV1` from an `Event`-kind `ContentTag`, rejecting all other kinds.
    pub fn from_event_tag(tag: ContentTag) -> Result<Self, CommitStateError> {
        if tag.kind() != TagKind::Event {
            return Err(CommitStateError::InvalidKeyOrValue {
                reason: "MintIdV1 requires TagKind::Event",
            });
        }
        Ok(Self(tag.digest()))
    }

    /// Wraps raw 32-byte digest bytes as a `MintIdV1`.
    pub const fn from_bytes(bytes: [u8; EVENT_KEY_BYTES]) -> Self {
        Self(bytes)
    }

    /// Borrows the 32-byte canonical value bytes.
    pub const fn as_bytes(&self) -> &[u8; EVENT_KEY_BYTES] {
        &self.0
    }
}

/// Canonical episode provenance record stored as the value of [`EpisodeRootV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EpisodeRecordV1 {
    /// Mutable non-bedrock coordinate where this episode occurred.
    pub coord: Coord,
    /// Mint event identifier opening this episode (must match the leaf's `EventIdV1` key).
    pub mint: MintIdV1,
    /// Consuming burn event identifier if closed, or `None` if active.
    pub consuming_burn: Option<EventIdV1>,
}

impl EpisodeRecordV1 {
    /// Projects a canonical `EpisodeRecordV1` from a [`CoordinateEpisode`].
    pub fn from_episode(ep: &CoordinateEpisode) -> Result<Self, CommitStateError> {
        let mint = MintIdV1::from_event_tag(ep.mint())?;
        let consuming_burn = match ep.consuming_burn() {
            None => None,
            Some(b) => Some(EventIdV1::from_event_tag(b)?),
        };
        let rec = Self {
            coord: ep.coord(),
            mint,
            consuming_burn,
        };
        rec.validate()?;
        Ok(rec)
    }

    /// Validates that `coord` is not bedrock and `consuming_burn` is distinct from `mint`.
    pub fn validate(&self) -> Result<(), CommitStateError> {
        if is_bedrock(self.coord) {
            return Err(CommitStateError::InvalidKeyOrValue {
                reason: "bedrock coordinate forbidden in EpisodeRecordV1",
            });
        }
        if let Some(burn) = self.consuming_burn
            && burn.0 == self.mint.0
        {
            return Err(CommitStateError::InvalidKeyOrValue {
                reason: "consuming_burn must not equal mint in EpisodeRecordV1",
            });
        }
        Ok(())
    }

    /// Encodes this episode record into 57 bytes (active) or 89 bytes (closed).
    pub fn encode_canonical(&self) -> Vec<u8> {
        let cap = if self.consuming_burn.is_some() {
            EPISODE_CLOSED_WIRE_LEN
        } else {
            EPISODE_ACTIVE_WIRE_LEN
        };
        let mut out = Vec::with_capacity(cap);
        out.extend_from_slice(&self.coord.x.to_be_bytes());
        out.extend_from_slice(&self.coord.y.to_be_bytes());
        out.extend_from_slice(&self.coord.z.to_be_bytes());
        out.extend_from_slice(self.mint.as_bytes());
        match self.consuming_burn {
            None => out.push(EPISODE_ACTIVE_MARKER),
            Some(burn) => {
                out.push(EPISODE_CLOSED_MARKER);
                out.extend_from_slice(burn.as_bytes());
            }
        }
        out
    }

    /// Decodes and validates a canonical `EpisodeRecordV1`.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, CommitStateError> {
        let mut r = WireReader::new(bytes);
        let coord = Coord::new(
            r.i64("episode coord x")?,
            r.i64("episode coord y")?,
            r.i64("episode coord z")?,
        );
        let mint = MintIdV1::from_bytes(r.bytes32("episode mint")?);
        let consuming_burn = match r.u8("episode state marker")? {
            EPISODE_ACTIVE_MARKER => None,
            EPISODE_CLOSED_MARKER => Some(EventIdV1::from_bytes(r.bytes32("episode burn")?)),
            _ => {
                return Err(CommitStateError::InvalidKeyOrValue {
                    reason: "unknown EpisodeRecordV1 state marker",
                });
            }
        };
        r.finish()?;
        let rec = Self {
            coord,
            mint,
            consuming_burn,
        };
        rec.validate()?;
        Ok(rec)
    }
}

/// Validates that `key` and `value` obey the canonical codec rules of `profile`.
pub fn validate_domain_kv(
    profile: &CommitmentProfileDescriptorV1,
    key: &[u8],
    value: &[u8],
) -> Result<(), CommitStateError> {
    if key.len() != profile.key_codec.byte_len() {
        return Err(CommitStateError::InvalidKeyOrValue {
            reason: "key byte length disagrees with profile key_codec",
        });
    }
    match profile.value_codec {
        ValueCodecIdV1::OnV1 => {
            if value != ON_V1_VALUE_BYTES {
                return Err(CommitStateError::InvalidKeyOrValue {
                    reason: "OnV1 value must be exact byte [0x01]",
                });
            }
        }
        ValueCodecIdV1::MintIdV1 => {
            if value.len() != EVENT_KEY_BYTES {
                return Err(CommitStateError::InvalidKeyOrValue {
                    reason: "MintIdV1 value must be exact 32 bytes",
                });
            }
            let coord =
                CoordinateKeyV1::from_bytes(key.try_into().expect("24-byte key")).to_coord();
            if is_bedrock(coord) {
                return Err(CommitStateError::InvalidKeyOrValue {
                    reason: "bedrock coordinate forbidden in ActiveTokenRootV1",
                });
            }
        }
        ValueCodecIdV1::EpisodeRecordV1 => {
            let rec = EpisodeRecordV1::decode_canonical(value)?;
            if rec.mint.as_bytes() != key {
                return Err(CommitStateError::InvalidKeyOrValue {
                    reason: "EpisodeRecordV1 mint field disagrees with EventIdV1 key",
                });
            }
        }
    }
    Ok(())
}

/// Separately typed `WorldRootV1` wrapper (never aliases legacy `world_tag`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WorldRootV1 {
    /// 32-byte canonical root digest.
    pub root: [u8; 32],
    /// Derived `CommitmentProfileIdV1` under which `root` was computed.
    pub profile_id: [u8; 32],
}

impl WorldRootV1 {
    /// Computes the canonical `World` `ContentTag` under `COMMIT_1_WORLD_ROOT_SCHEMA`.
    pub fn tag(&self) -> ContentTag {
        hash_canonical(TagKind::World, COMMIT_1_WORLD_ROOT_SCHEMA, 64, |sink| {
            sink(&self.profile_id);
            sink(&self.root);
        })
        .expect("WorldRootV1 tag is canonical")
    }
}

/// Separately typed `ActiveTokenRootV1` wrapper.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ActiveTokenRootV1 {
    /// 32-byte canonical root digest.
    pub root: [u8; 32],
    /// Derived `CommitmentProfileIdV1` under which `root` was computed.
    pub profile_id: [u8; 32],
}

impl ActiveTokenRootV1 {
    /// Computes the canonical `Receipt` `ContentTag` under `COMMIT_1_ACTIVE_TOKEN_ROOT_SCHEMA`.
    pub fn tag(&self) -> ContentTag {
        hash_canonical(
            TagKind::Receipt,
            COMMIT_1_ACTIVE_TOKEN_ROOT_SCHEMA,
            64,
            |sink| {
                sink(&self.profile_id);
                sink(&self.root);
            },
        )
        .expect("ActiveTokenRootV1 tag is canonical")
    }
}

/// Separately typed `EpisodeRootV1` wrapper.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EpisodeRootV1 {
    /// 32-byte canonical root digest.
    pub root: [u8; 32],
    /// Derived `CommitmentProfileIdV1` under which `root` was computed.
    pub profile_id: [u8; 32],
}

impl EpisodeRootV1 {
    /// Computes the canonical `Receipt` `ContentTag` under `COMMIT_1_EPISODE_ROOT_SCHEMA`.
    pub fn tag(&self) -> ContentTag {
        hash_canonical(TagKind::Receipt, COMMIT_1_EPISODE_ROOT_SCHEMA, 64, |sink| {
            sink(&self.profile_id);
            sink(&self.root);
        })
        .expect("EpisodeRootV1 tag is canonical")
    }
}

// =========================================================================
// GENERIC FIXED-WIDTH BINARY MERKLE PATRICIA TRIE (CANDIDATE A)
// =========================================================================

fn key_bit(key: &[u8], depth: usize) -> u8 {
    (key[depth / 8] >> (7 - (depth % 8))) & 1
}

fn common_prefix_from(a: &[u8], b: &[u8], start_depth: usize, key_bits: usize) -> usize {
    let mut n = 0usize;
    while start_depth + n < key_bits && key_bit(a, start_depth + n) == key_bit(b, start_depth + n) {
        n += 1;
    }
    n
}

fn pack_bit_slice(bits: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; bits.len().div_ceil(8)];
    for (idx, &bit) in bits.iter().enumerate() {
        out[idx / 8] |= (bit & 1) << (7 - (idx % 8));
    }
    out
}

fn pack_key_suffix(key: &[u8], start_depth: usize, key_bits: usize) -> Vec<u8> {
    let suffix_len = key_bits - start_depth;
    let mut out = vec![0u8; suffix_len.div_ceil(8)];
    for idx in 0..suffix_len {
        let bit = key_bit(key, start_depth + idx);
        out[idx / 8] |= bit << (7 - (idx % 8));
    }
    out
}

fn unpack_bit_slice_strict(packed: &[u8], bit_len: usize) -> Result<Vec<u8>, CommitStateError> {
    if packed.len() != bit_len.div_ceil(8) {
        return Err(CommitStateError::MalformedEncoding {
            reason: "packed bit slice length mismatch",
        });
    }
    let rem = bit_len % 8;
    if rem != 0 {
        let mask = (1u8 << (8 - rem)) - 1;
        if (packed[packed.len() - 1] & mask) != 0 {
            return Err(CommitStateError::MalformedEncoding {
                reason: "non-zero unused padding bits in packed path",
            });
        }
    }
    let mut bits = Vec::with_capacity(bit_len);
    for idx in 0..bit_len {
        bits.push((packed[idx / 8] >> (7 - (idx % 8))) & 1);
    }
    Ok(bits)
}

/// Computes the canonical `COMMIT-1` `NodeHash` binding profile ID, tree domain, suite, version,
/// node type, 16-bit consumed depth, and length-framed payload.
///
/// Returns `(digest, total_preimage_bytes)` so bounded verifiers can charge hashed bytes exactly.
pub fn compute_typed_node_hash(
    profile_id: &[u8; 32],
    tree_domain: &[u8],
    node_codec_version: u16,
    node_type: u8,
    consumed_depth: u16,
    payload: &[u8],
) -> ([u8; 32], usize) {
    let mut hasher = Sha256::new();
    hasher.update(profile_id);
    hasher.update((tree_domain.len() as u16).to_be_bytes());
    hasher.update(tree_domain);
    hasher.update((HASH_SUITE_BYTES.len() as u16).to_be_bytes());
    hasher.update(HASH_SUITE_BYTES);
    hasher.update(node_codec_version.to_be_bytes());
    hasher.update([node_type]);
    hasher.update(consumed_depth.to_be_bytes());
    hasher.update((payload.len() as u32).to_be_bytes());
    hasher.update(payload);
    let preimage_len =
        32 + 2 + tree_domain.len() + 2 + HASH_SUITE_BYTES.len() + 2 + 1 + 2 + 4 + payload.len();
    (hasher.finalize().into(), preimage_len)
}

fn encode_leaf_payload(depth: usize, key: &[u8], value: &[u8], key_bits: usize) -> Vec<u8> {
    let suffix_len = key_bits - depth;
    let packed = pack_key_suffix(key, depth, key_bits);
    let mut payload = Vec::with_capacity(2 + packed.len() + 4 + value.len());
    payload.extend_from_slice(&(suffix_len as u16).to_be_bytes());
    payload.extend_from_slice(&packed);
    payload.extend_from_slice(&(value.len() as u32).to_be_bytes());
    payload.extend_from_slice(value);
    payload
}

fn encode_extension_payload(shared_bits: &[u8], child_hash: &[u8; 32]) -> Vec<u8> {
    let packed = pack_bit_slice(shared_bits);
    let mut payload = Vec::with_capacity(2 + packed.len() + 32);
    payload.extend_from_slice(&(shared_bits.len() as u16).to_be_bytes());
    payload.extend_from_slice(&packed);
    payload.extend_from_slice(child_hash);
    payload
}

fn encode_branch_payload(left_hash: &[u8; 32], right_hash: &[u8; 32]) -> [u8; 64] {
    let mut payload = [0u8; 64];
    payload[0..32].copy_from_slice(left_hash);
    payload[32..64].copy_from_slice(right_hash);
    payload
}

/// Encodes a canonical self-describing node store record `node_type || depth_u16 || payload_len_u32 || payload`.
fn encode_node_store_record(node_type: u8, depth: u16, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + 2 + 4 + payload.len());
    out.push(node_type);
    out.extend_from_slice(&depth.to_be_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

/// Decoded child references discovered when verifying a persisted node record during closure or GC.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodedNodeChildren {
    /// Leaf node (no child hashes).
    Leaf,
    /// Extension node referencing one non-empty branch child hash.
    Extension([u8; 32]),
    /// Branch node referencing two non-empty child hashes.
    Branch([u8; 32], [u8; 32]),
}

/// Decodes a stored node record canonically, checks structural invariants and recomputes `NodeHash`.
///
/// Fails closed on truncation, trailing bytes, invalid node type/depth, non-zero padding bits,
/// empty hash below branch/extension, or digest mismatch.
pub fn decode_and_verify_node_record(
    profile: &CommitmentProfileDescriptorV1,
    expected_hash: &[u8; 32],
    record_bytes: &[u8],
) -> Result<DecodedNodeChildren, CommitStateError> {
    let mut r = WireReader::new(record_bytes);
    let node_type = r.u8("node record type")?;
    let depth = r.u16("node record depth")?;
    let payload_len = r.u32("node record payload_len")? as usize;
    let payload = r.take(payload_len, "node record payload")?;
    r.finish()?;

    let profile_id = profile.id();
    let empty_hash = typed_empty_root(profile);
    let key_bits = profile.key_bit_width as usize;

    let children = match node_type {
        NODE_TYPE_LEAF => {
            if depth as usize > key_bits {
                return Err(CommitStateError::CorruptOrMissingNode {
                    expected_hash: *expected_hash,
                });
            }
            let mut pr = WireReader::new(payload);
            let suffix_bits = pr.u16("leaf suffix_bits")? as usize;
            if suffix_bits != key_bits - depth as usize {
                return Err(CommitStateError::CorruptOrMissingNode {
                    expected_hash: *expected_hash,
                });
            }
            let packed = pr.take(suffix_bits.div_ceil(8), "leaf suffix packed")?;
            let _ = unpack_bit_slice_strict(packed, suffix_bits).map_err(|_| {
                CommitStateError::CorruptOrMissingNode {
                    expected_hash: *expected_hash,
                }
            })?;
            let val_len = pr.u32("leaf value_len")? as usize;
            let _val = pr.take(val_len, "leaf value bytes")?;
            pr.finish()
                .map_err(|_| CommitStateError::CorruptOrMissingNode {
                    expected_hash: *expected_hash,
                })?;
            DecodedNodeChildren::Leaf
        }
        NODE_TYPE_EXTENSION => {
            let mut pr = WireReader::new(payload);
            let path_bits = pr.u16("ext path_bits")? as usize;
            if path_bits == 0 || (depth as usize) + path_bits >= key_bits {
                return Err(CommitStateError::CorruptOrMissingNode {
                    expected_hash: *expected_hash,
                });
            }
            let packed = pr.take(path_bits.div_ceil(8), "ext path packed")?;
            let _ = unpack_bit_slice_strict(packed, path_bits).map_err(|_| {
                CommitStateError::CorruptOrMissingNode {
                    expected_hash: *expected_hash,
                }
            })?;
            let child_hash = pr.bytes32("ext child_hash")?;
            pr.finish()
                .map_err(|_| CommitStateError::CorruptOrMissingNode {
                    expected_hash: *expected_hash,
                })?;
            if child_hash == empty_hash {
                return Err(CommitStateError::CorruptOrMissingNode {
                    expected_hash: *expected_hash,
                });
            }
            DecodedNodeChildren::Extension(child_hash)
        }
        NODE_TYPE_BRANCH => {
            if depth as usize >= key_bits || payload.len() != 64 {
                return Err(CommitStateError::CorruptOrMissingNode {
                    expected_hash: *expected_hash,
                });
            }
            let mut left = [0u8; 32];
            let mut right = [0u8; 32];
            left.copy_from_slice(&payload[0..32]);
            right.copy_from_slice(&payload[32..64]);
            if left == empty_hash || right == empty_hash {
                return Err(CommitStateError::CorruptOrMissingNode {
                    expected_hash: *expected_hash,
                });
            }
            DecodedNodeChildren::Branch(left, right)
        }
        _ => {
            return Err(CommitStateError::CorruptOrMissingNode {
                expected_hash: *expected_hash,
            });
        }
    };

    let (recomputed, _) = compute_typed_node_hash(
        &profile_id,
        profile.tree_domain(),
        profile.node_codec_version,
        node_type,
        depth,
        payload,
    );
    if &recomputed != expected_hash {
        return Err(CommitStateError::CorruptOrMissingNode {
            expected_hash: *expected_hash,
        });
    }
    Ok(children)
}

/// Returns the canonical empty-map root digest for `profile`.
pub fn typed_empty_root(profile: &CommitmentProfileDescriptorV1) -> [u8; 32] {
    let profile_id = profile.id();
    compute_typed_node_hash(
        &profile_id,
        profile.tree_domain(),
        profile.node_codec_version,
        NODE_TYPE_EMPTY,
        0,
        &[],
    )
    .0
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TypedNode {
    Empty,
    Leaf {
        depth: usize,
        key: Vec<u8>,
        value: Vec<u8>,
        hash: [u8; 32],
    },
    Extension {
        depth: usize,
        shared_bits: Vec<u8>,
        child: Arc<TypedNode>,
        hash: [u8; 32],
    },
    Branch {
        depth: usize,
        left: Arc<TypedNode>,
        right: Arc<TypedNode>,
        hash: [u8; 32],
    },
}

#[derive(Clone, Copy, Debug)]
struct HashCtx {
    profile_id: [u8; 32],
    tree_domain: &'static [u8],
    node_codec_version: u16,
    key_bits: usize,
    empty_hash: [u8; 32],
}

impl HashCtx {
    fn from_profile(profile: &CommitmentProfileDescriptorV1) -> Self {
        let profile_id = profile.id();
        let empty_hash = typed_empty_root(profile);
        Self {
            profile_id,
            tree_domain: profile.tree_domain(),
            node_codec_version: profile.node_codec_version,
            key_bits: profile.key_bit_width as usize,
            empty_hash,
        }
    }

    fn make_leaf(&self, depth: usize, key: Vec<u8>, value: Vec<u8>) -> TypedNode {
        let payload = encode_leaf_payload(depth, &key, &value, self.key_bits);
        let (hash, _) = compute_typed_node_hash(
            &self.profile_id,
            self.tree_domain,
            self.node_codec_version,
            NODE_TYPE_LEAF,
            depth as u16,
            &payload,
        );
        TypedNode::Leaf {
            depth,
            key,
            value,
            hash,
        }
    }

    fn make_ext(&self, depth: usize, shared_bits: Vec<u8>, child: Arc<TypedNode>) -> TypedNode {
        let ch = child.hash(self.empty_hash);
        let payload = encode_extension_payload(&shared_bits, &ch);
        let (hash, _) = compute_typed_node_hash(
            &self.profile_id,
            self.tree_domain,
            self.node_codec_version,
            NODE_TYPE_EXTENSION,
            depth as u16,
            &payload,
        );
        TypedNode::Extension {
            depth,
            shared_bits,
            child,
            hash,
        }
    }

    fn make_branch(&self, depth: usize, left: Arc<TypedNode>, right: Arc<TypedNode>) -> TypedNode {
        let lh = left.hash(self.empty_hash);
        let rh = right.hash(self.empty_hash);
        let payload = encode_branch_payload(&lh, &rh);
        let (hash, _) = compute_typed_node_hash(
            &self.profile_id,
            self.tree_domain,
            self.node_codec_version,
            NODE_TYPE_BRANCH,
            depth as u16,
            &payload,
        );
        TypedNode::Branch {
            depth,
            left,
            right,
            hash,
        }
    }
}

impl TypedNode {
    fn hash(&self, empty_hash: [u8; 32]) -> [u8; 32] {
        match self {
            Self::Empty => empty_hash,
            Self::Leaf { hash, .. } | Self::Extension { hash, .. } | Self::Branch { hash, .. } => {
                *hash
            }
        }
    }

    fn collect_records(&self, ctx: &HashCtx, out: &mut BTreeMap<[u8; 32], Vec<u8>>) {
        match self {
            Self::Empty => {}
            Self::Leaf {
                depth,
                key,
                value,
                hash,
            } => {
                let payload = encode_leaf_payload(*depth, key, value, ctx.key_bits);
                out.insert(
                    *hash,
                    encode_node_store_record(NODE_TYPE_LEAF, *depth as u16, &payload),
                );
            }
            Self::Extension {
                depth,
                shared_bits,
                child,
                hash,
            } => {
                let ch = child.hash(ctx.empty_hash);
                let payload = encode_extension_payload(shared_bits, &ch);
                out.insert(
                    *hash,
                    encode_node_store_record(NODE_TYPE_EXTENSION, *depth as u16, &payload),
                );
                child.collect_records(ctx, out);
            }
            Self::Branch {
                depth,
                left,
                right,
                hash,
            } => {
                let lh = left.hash(ctx.empty_hash);
                let rh = right.hash(ctx.empty_hash);
                let payload = encode_branch_payload(&lh, &rh);
                out.insert(
                    *hash,
                    encode_node_store_record(NODE_TYPE_BRANCH, *depth as u16, &payload),
                );
                left.collect_records(ctx, out);
                right.collect_records(ctx, out);
            }
        }
    }
}

fn build_typed_subtree(entries: &[(Vec<u8>, Vec<u8>)], depth: usize, ctx: &HashCtx) -> TypedNode {
    if entries.is_empty() {
        return TypedNode::Empty;
    }
    if entries.len() == 1 {
        let (k, v) = &entries[0];
        return ctx.make_leaf(depth, k.clone(), v.clone());
    }
    let first = &entries[0].0;
    let last = &entries[entries.len() - 1].0;
    let shared = common_prefix_from(first, last, depth, ctx.key_bits);
    let branch_depth = depth + shared;
    let split = entries.partition_point(|(k, _)| key_bit(k, branch_depth) == 0);
    let left = Arc::new(build_typed_subtree(
        &entries[..split],
        branch_depth + 1,
        ctx,
    ));
    let right = Arc::new(build_typed_subtree(
        &entries[split..],
        branch_depth + 1,
        ctx,
    ));
    let branch = ctx.make_branch(branch_depth, left, right);
    if shared == 0 {
        branch
    } else {
        let shared_bits: Vec<u8> = (depth..branch_depth).map(|d| key_bit(first, d)).collect();
        ctx.make_ext(depth, shared_bits, Arc::new(branch))
    }
}

fn upsert_typed_node(
    node: &TypedNode,
    key: Vec<u8>,
    value: Vec<u8>,
    depth: usize,
    ctx: &HashCtx,
) -> (TypedNode, bool) {
    match node {
        TypedNode::Empty => (ctx.make_leaf(0, key, value), true),
        TypedNode::Leaf {
            depth: d,
            key: existing_k,
            value: existing_v,
            ..
        } => {
            debug_assert_eq!(*d, depth);
            if existing_k == &key {
                if existing_v == &value {
                    return (node.clone(), false);
                }
                return (ctx.make_leaf(*d, key, value), true);
            }
            let shared = common_prefix_from(existing_k, &key, *d, ctx.key_bits);
            let split_d = *d + shared;
            let old_leaf =
                Arc::new(ctx.make_leaf(split_d + 1, existing_k.clone(), existing_v.clone()));
            let new_leaf = Arc::new(ctx.make_leaf(split_d + 1, key.clone(), value));
            let (left, right) = if key_bit(existing_k, split_d) == 0 {
                (old_leaf, new_leaf)
            } else {
                (new_leaf, old_leaf)
            };
            let branch = ctx.make_branch(split_d, left, right);
            if shared == 0 {
                (branch, true)
            } else {
                let bits: Vec<u8> = (*d..split_d).map(|bit_d| key_bit(&key, bit_d)).collect();
                (ctx.make_ext(*d, bits, Arc::new(branch)), true)
            }
        }
        TypedNode::Extension {
            depth: d,
            shared_bits,
            child,
            ..
        } => {
            debug_assert_eq!(*d, depth);
            let mut matched = 0usize;
            while matched < shared_bits.len() && key_bit(&key, *d + matched) == shared_bits[matched]
            {
                matched += 1;
            }
            if matched == shared_bits.len() {
                let (new_child, changed) =
                    upsert_typed_node(child, key, value, *d + shared_bits.len(), ctx);
                if !changed {
                    return (node.clone(), false);
                }
                return (
                    ctx.make_ext(*d, shared_bits.clone(), Arc::new(new_child)),
                    true,
                );
            }
            let split_d = *d + matched;
            let rem = shared_bits.len() - matched - 1;
            let existing_sub = if rem == 0 {
                Arc::clone(child)
            } else {
                Arc::new(ctx.make_ext(
                    split_d + 1,
                    shared_bits[matched + 1..].to_vec(),
                    Arc::clone(child),
                ))
            };
            let new_leaf = Arc::new(ctx.make_leaf(split_d + 1, key, value));
            let (left, right) = if shared_bits[matched] == 0 {
                (existing_sub, new_leaf)
            } else {
                (new_leaf, existing_sub)
            };
            let branch = ctx.make_branch(split_d, left, right);
            if matched == 0 {
                (branch, true)
            } else {
                (
                    ctx.make_ext(*d, shared_bits[..matched].to_vec(), Arc::new(branch)),
                    true,
                )
            }
        }
        TypedNode::Branch {
            depth: d,
            left,
            right,
            ..
        } => {
            debug_assert_eq!(*d, depth);
            if key_bit(&key, *d) == 0 {
                let (nl, changed) = upsert_typed_node(left, key, value, *d + 1, ctx);
                if !changed {
                    return (node.clone(), false);
                }
                (ctx.make_branch(*d, Arc::new(nl), Arc::clone(right)), true)
            } else {
                let (nr, changed) = upsert_typed_node(right, key, value, *d + 1, ctx);
                if !changed {
                    return (node.clone(), false);
                }
                (ctx.make_branch(*d, Arc::clone(left), Arc::new(nr)), true)
            }
        }
    }
}

fn collapse_survivor(
    survivor: &Arc<TypedNode>,
    depth: usize,
    dir_bit: u8,
    ctx: &HashCtx,
) -> TypedNode {
    match survivor.as_ref() {
        TypedNode::Leaf { key, value, .. } => ctx.make_leaf(depth, key.clone(), value.clone()),
        TypedNode::Extension {
            shared_bits, child, ..
        } => {
            let mut merged = Vec::with_capacity(1 + shared_bits.len());
            merged.push(dir_bit);
            merged.extend_from_slice(shared_bits);
            ctx.make_ext(depth, merged, Arc::clone(child))
        }
        TypedNode::Branch { .. } => ctx.make_ext(depth, vec![dir_bit], Arc::clone(survivor)),
        TypedNode::Empty => unreachable!("branch survivor is never empty"),
    }
}

fn delete_typed_node(
    node: &TypedNode,
    key: &[u8],
    depth: usize,
    ctx: &HashCtx,
) -> (TypedNode, bool) {
    match node {
        TypedNode::Empty => (TypedNode::Empty, false),
        TypedNode::Leaf {
            depth: d,
            key: existing_k,
            ..
        } => {
            debug_assert_eq!(*d, depth);
            if existing_k.as_slice() == key {
                (TypedNode::Empty, true)
            } else {
                (node.clone(), false)
            }
        }
        TypedNode::Branch {
            depth: d,
            left,
            right,
            ..
        } => {
            debug_assert_eq!(*d, depth);
            if key_bit(key, *d) == 0 {
                let (nl, removed) = delete_typed_node(left, key, *d + 1, ctx);
                if !removed {
                    return (node.clone(), false);
                }
                if matches!(nl, TypedNode::Empty) {
                    return (collapse_survivor(right, *d, 1, ctx), true);
                }
                (ctx.make_branch(*d, Arc::new(nl), Arc::clone(right)), true)
            } else {
                let (nr, removed) = delete_typed_node(right, key, *d + 1, ctx);
                if !removed {
                    return (node.clone(), false);
                }
                if matches!(nr, TypedNode::Empty) {
                    return (collapse_survivor(left, *d, 0, ctx), true);
                }
                (ctx.make_branch(*d, Arc::clone(left), Arc::new(nr)), true)
            }
        }
        TypedNode::Extension {
            depth: d,
            shared_bits,
            child,
            ..
        } => {
            debug_assert_eq!(*d, depth);
            if shared_bits
                .iter()
                .enumerate()
                .any(|(i, &b)| key_bit(key, *d + i) != b)
            {
                return (node.clone(), false);
            }
            let (nc, removed) = delete_typed_node(child, key, *d + shared_bits.len(), ctx);
            if !removed {
                return (node.clone(), false);
            }
            match nc {
                TypedNode::Leaf {
                    key: sk, value: sv, ..
                } => (ctx.make_leaf(*d, sk, sv), true),
                TypedNode::Extension {
                    shared_bits: cb,
                    child: gc,
                    ..
                } => {
                    let mut merged = Vec::with_capacity(shared_bits.len() + cb.len());
                    merged.extend_from_slice(shared_bits);
                    merged.extend_from_slice(&cb);
                    (ctx.make_ext(*d, merged, gc), true)
                }
                other => (ctx.make_ext(*d, shared_bits.clone(), Arc::new(other)), true),
            }
        }
    }
}

/// Generic fixed-width binary Merkle Patricia Trie bound to one [`CommitmentProfileDescriptorV1`].
#[derive(Clone, Debug)]
pub struct TypedPatriciaTrie {
    profile: CommitmentProfileDescriptorV1,
    ctx: HashCtx,
    root: TypedNode,
    entries: BTreeMap<Vec<u8>, Vec<u8>>,
}

impl TypedPatriciaTrie {
    /// Creates an empty authenticated map under `profile`.
    pub fn empty(profile: CommitmentProfileDescriptorV1) -> Result<Self, CommitStateError> {
        profile.validate()?;
        let ctx = HashCtx::from_profile(&profile);
        Ok(Self {
            profile,
            ctx,
            root: TypedNode::Empty,
            entries: BTreeMap::new(),
        })
    }

    /// Cold-rebuilds an authenticated map from unsorted `(key, value)` pairs, rejecting duplicate
    /// keys, invalid codecs, or counts exceeding `op_limits.max_full_build_keys`.
    pub fn from_entries(
        profile: CommitmentProfileDescriptorV1,
        op_limits: &CommitmentOperationalLimitsDescriptorV1,
        mut raw_entries: Vec<(Vec<u8>, Vec<u8>)>,
    ) -> Result<Self, CommitStateError> {
        profile.validate()?;
        op_limits.validate()?;
        if (raw_entries.len() as u64) > op_limits.max_full_build_keys {
            return Err(CommitStateError::LimitExceeded {
                field: "max_full_build_keys",
            });
        }
        for (k, v) in &raw_entries {
            validate_domain_kv(&profile, k, v)?;
        }
        raw_entries.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        for w in raw_entries.windows(2) {
            if w[0].0 == w[1].0 {
                return Err(CommitStateError::DuplicateKey);
            }
        }
        let ctx = HashCtx::from_profile(&profile);
        let root = build_typed_subtree(&raw_entries, 0, &ctx);
        let entries = raw_entries.into_iter().collect();
        Ok(Self {
            profile,
            ctx,
            root,
            entries,
        })
    }

    /// Returns the commitment profile bound to this trie.
    pub const fn profile(&self) -> &CommitmentProfileDescriptorV1 {
        &self.profile
    }

    /// Returns the 32-byte canonical root digest of this trie.
    pub fn root(&self) -> [u8; 32] {
        self.root.hash(self.ctx.empty_hash)
    }

    /// Returns the number of logical key/value entries in this trie.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if this trie contains zero entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Looks up a canonical key's stored value bytes.
    pub fn get(&self, key: &[u8]) -> Option<&[u8]> {
        self.entries.get(key).map(Vec::as_slice)
    }

    /// Incrementally inserts or updates `(key, value)` using canonical path-copy normalization.
    pub fn upsert(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<bool, CommitStateError> {
        validate_domain_kv(&self.profile, &key, &value)?;
        let (new_root, changed) =
            upsert_typed_node(&self.root, key.clone(), value.clone(), 0, &self.ctx);
        if changed {
            self.root = new_root;
            self.entries.insert(key, value);
        }
        Ok(changed)
    }

    /// Incrementally deletes `key` using canonical branch-collapse and extension-merge rules.
    pub fn delete(&mut self, key: &[u8]) -> Result<bool, CommitStateError> {
        if key.len() != self.profile.key_codec.byte_len() {
            return Err(CommitStateError::InvalidKeyOrValue {
                reason: "delete key byte length disagrees with profile key_codec",
            });
        }
        let (new_root, removed) = delete_typed_node(&self.root, key, 0, &self.ctx);
        if removed {
            self.root = new_root;
            self.entries.remove(key);
        }
        Ok(removed)
    }

    /// Collects all reachable non-empty canonical node records keyed by their 32-byte `NodeHash`.
    pub fn collect_node_records(&self) -> BTreeMap<[u8; 32], Vec<u8>> {
        let mut out = BTreeMap::new();
        self.root.collect_records(&self.ctx, &mut out);
        out
    }

    /// Generates a canonical inclusion or non-inclusion witness for `query_key`.
    pub fn prove(&self, query_key: &[u8]) -> Result<TypedPatriciaProofV1, CommitStateError> {
        if query_key.len() != self.profile.key_codec.byte_len() {
            return Err(CommitStateError::InvalidKeyOrValue {
                reason: "prove key byte length disagrees with profile key_codec",
            });
        }
        let mut steps = Vec::new();
        let mut cur = &self.root;
        loop {
            match cur {
                TypedNode::Empty => {
                    return Ok(TypedPatriciaProofV1 {
                        steps,
                        terminal: TypedProofTerminal::EmptyRoot,
                    });
                }
                TypedNode::Leaf {
                    depth, key, value, ..
                } => {
                    return Ok(TypedPatriciaProofV1 {
                        steps,
                        terminal: TypedProofTerminal::Leaf {
                            depth: *depth as u16,
                            key: key.clone(),
                            value: value.clone(),
                        },
                    });
                }
                TypedNode::Extension {
                    depth,
                    shared_bits,
                    child,
                    ..
                } => {
                    let diverges = shared_bits
                        .iter()
                        .enumerate()
                        .any(|(i, &b)| key_bit(query_key, *depth + i) != b);
                    if diverges {
                        return Ok(TypedPatriciaProofV1 {
                            steps,
                            terminal: TypedProofTerminal::DivergentExtension {
                                depth: *depth as u16,
                                shared_bits: shared_bits.clone(),
                                child_hash: child.hash(self.ctx.empty_hash),
                            },
                        });
                    }
                    steps.push(TypedProofStep::Extension {
                        depth: *depth as u16,
                        shared_bits: shared_bits.clone(),
                    });
                    cur = child;
                }
                TypedNode::Branch {
                    depth, left, right, ..
                } => {
                    let dir = key_bit(query_key, *depth);
                    if dir == 0 {
                        steps.push(TypedProofStep::Branch {
                            depth: *depth as u16,
                            direction: 0,
                            sibling_hash: right.hash(self.ctx.empty_hash),
                        });
                        cur = left;
                    } else {
                        steps.push(TypedProofStep::Branch {
                            depth: *depth as u16,
                            direction: 1,
                            sibling_hash: left.hash(self.ctx.empty_hash),
                        });
                        cur = right;
                    }
                }
            }
        }
    }
}

// =========================================================================
// CONTEXT-BOUND POINT PROOFS & BOUNDED VERIFIER
// =========================================================================

/// Non-terminal step along a [`TypedPatriciaProofV1`] witness path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypedProofStep {
    /// Binary branch step carrying direction (`0` or `1`) and the unchosen sibling hash.
    Branch {
        /// Consumed bit depth before this branch.
        depth: u16,
        /// Chosen branch direction (`0` = left, `1` = right).
        direction: u8,
        /// 32-byte digest of the unchosen non-empty sibling subtree.
        sibling_hash: [u8; 32],
    },
    /// Path-compressed extension step traversed by `query_key`.
    Extension {
        /// Consumed bit depth before this extension.
        depth: u16,
        /// Non-empty sequence of matched path bits (`0` or `1`).
        shared_bits: Vec<u8>,
    },
}

/// Terminal node concluding a [`TypedPatriciaProofV1`] witness path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypedProofTerminal {
    /// Whole map is empty (valid only when `steps` is empty and expected root equals `MptEmpty`).
    EmptyRoot,
    /// Terminal leaf proving presence (if `key == query_key`) or absence (if `key != query_key`).
    Leaf {
        /// Consumed bit depth before this leaf (`0..=key_bits`).
        depth: u16,
        /// Complete fixed-width canonical key bytes of the stored leaf.
        key: Vec<u8>,
        /// Canonical typed value bytes stored at this leaf.
        value: Vec<u8>,
    },
    /// Terminal extension whose shared bits diverge from `query_key`, proving absence.
    DivergentExtension {
        /// Consumed bit depth before this extension.
        depth: u16,
        /// Non-empty sequence of path bits (`0` or `1`) diverging from `query_key`.
        shared_bits: Vec<u8>,
        /// 32-byte digest of the non-empty branch child below this extension.
        child_hash: [u8; 32],
    },
}

/// Canonical context-bound point proof (`COMMIT_1_POINT_PROOF_SCHEMA`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypedPatriciaProofV1 {
    /// Ordered traversal steps from depth 0 down to the terminal node.
    pub steps: Vec<TypedProofStep>,
    /// Terminal witness node.
    pub terminal: TypedProofTerminal,
}

impl TypedPatriciaProofV1 {
    /// Encodes this point proof into canonical big-endian wire bytes.
    pub fn to_canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&COMMIT_1_SCHEMA_VERSION.to_be_bytes());
        out.extend_from_slice(&(self.steps.len() as u16).to_be_bytes());
        for step in &self.steps {
            match step {
                TypedProofStep::Branch {
                    depth,
                    direction,
                    sibling_hash,
                } => {
                    out.push(PROOF_STEP_BRANCH);
                    out.extend_from_slice(&depth.to_be_bytes());
                    out.push(*direction);
                    out.extend_from_slice(sibling_hash);
                }
                TypedProofStep::Extension { depth, shared_bits } => {
                    out.push(PROOF_STEP_EXTENSION);
                    out.extend_from_slice(&depth.to_be_bytes());
                    out.extend_from_slice(&(shared_bits.len() as u16).to_be_bytes());
                    out.extend_from_slice(&pack_bit_slice(shared_bits));
                }
            }
        }
        match &self.terminal {
            TypedProofTerminal::EmptyRoot => {
                out.push(PROOF_TERM_EMPTY);
            }
            TypedProofTerminal::Leaf { depth, key, value } => {
                out.push(PROOF_TERM_LEAF);
                out.extend_from_slice(&depth.to_be_bytes());
                out.extend_from_slice(&(key.len() as u16).to_be_bytes());
                out.extend_from_slice(key);
                out.extend_from_slice(&(value.len() as u32).to_be_bytes());
                out.extend_from_slice(value);
            }
            TypedProofTerminal::DivergentExtension {
                depth,
                shared_bits,
                child_hash,
            } => {
                out.push(PROOF_TERM_DIVERGENT_EXT);
                out.extend_from_slice(&depth.to_be_bytes());
                out.extend_from_slice(&(shared_bits.len() as u16).to_be_bytes());
                out.extend_from_slice(&pack_bit_slice(shared_bits));
                out.extend_from_slice(child_hash);
            }
        }
        out
    }

    /// Decodes a canonical `TypedPatriciaProofV1` enforcing `policy` bounds before allocating.
    pub fn from_canonical_bytes_bounded(
        bytes: &[u8],
        key_bytes: usize,
        key_bits: usize,
        policy: &ProofPolicyDescriptorV1,
    ) -> Result<Self, CommitStateError> {
        if bytes.len() > policy.max_proof_bytes as usize {
            return Err(CommitStateError::LimitExceeded {
                field: "max_proof_bytes",
            });
        }
        let mut r = WireReader::new(bytes);
        let ver = r.u16("proof schema_version")?;
        if ver != COMMIT_1_SCHEMA_VERSION {
            return Err(CommitStateError::WrongVersion {
                expected: COMMIT_1_SCHEMA_VERSION,
                actual: ver,
            });
        }
        let step_count = r.u16("proof step_count")? as usize;
        if step_count > key_bits || step_count > policy.max_proof_records as usize {
            return Err(CommitStateError::LimitExceeded {
                field: "max_proof_records",
            });
        }
        let mut steps = Vec::with_capacity(step_count);
        for _ in 0..step_count {
            match r.u8("proof step tag")? {
                PROOF_STEP_BRANCH => {
                    let depth = r.u16("branch step depth")?;
                    let direction = r.u8("branch step direction")?;
                    let sibling_hash = r.bytes32("branch step sibling_hash")?;
                    if depth as usize >= key_bits || direction > 1 {
                        return Err(CommitStateError::InvalidProof {
                            reason: "branch step depth or direction out of range",
                        });
                    }
                    steps.push(TypedProofStep::Branch {
                        depth,
                        direction,
                        sibling_hash,
                    });
                }
                PROOF_STEP_EXTENSION => {
                    let depth = r.u16("ext step depth")?;
                    let bit_len = r.u16("ext step bit_len")? as usize;
                    if bit_len == 0 || (depth as usize) + bit_len >= key_bits {
                        return Err(CommitStateError::InvalidProof {
                            reason: "extension step depth/length out of range",
                        });
                    }
                    let packed = r.take(bit_len.div_ceil(8), "ext step packed bits")?;
                    let shared_bits = unpack_bit_slice_strict(packed, bit_len)?;
                    steps.push(TypedProofStep::Extension { depth, shared_bits });
                }
                _ => {
                    return Err(CommitStateError::MalformedEncoding {
                        reason: "unknown proof step tag",
                    });
                }
            }
        }
        let terminal = match r.u8("proof terminal tag")? {
            PROOF_TERM_EMPTY => TypedProofTerminal::EmptyRoot,
            PROOF_TERM_LEAF => {
                let depth = r.u16("leaf terminal depth")?;
                let klen = r.u16("leaf terminal key_len")? as usize;
                if depth as usize > key_bits || klen != key_bytes {
                    return Err(CommitStateError::InvalidProof {
                        reason: "leaf terminal depth or key_len invalid",
                    });
                }
                let key = r.take(klen, "leaf terminal key")?.to_vec();
                let vlen = r.u32("leaf terminal value_len")? as usize;
                if vlen > policy.max_single_encoded_node_bytes as usize {
                    return Err(CommitStateError::LimitExceeded {
                        field: "max_single_encoded_node_bytes",
                    });
                }
                let value = r.take(vlen, "leaf terminal value")?.to_vec();
                TypedProofTerminal::Leaf { depth, key, value }
            }
            PROOF_TERM_DIVERGENT_EXT => {
                let depth = r.u16("div ext depth")?;
                let bit_len = r.u16("div ext bit_len")? as usize;
                if bit_len == 0 || (depth as usize) + bit_len >= key_bits {
                    return Err(CommitStateError::InvalidProof {
                        reason: "divergent extension depth/length out of range",
                    });
                }
                let packed = r.take(bit_len.div_ceil(8), "div ext packed bits")?;
                let shared_bits = unpack_bit_slice_strict(packed, bit_len)?;
                let child_hash = r.bytes32("div ext child_hash")?;
                TypedProofTerminal::DivergentExtension {
                    depth,
                    shared_bits,
                    child_hash,
                }
            }
            _ => {
                return Err(CommitStateError::MalformedEncoding {
                    reason: "unknown proof terminal tag",
                });
            }
        };
        r.finish()?;
        let record_count = match terminal {
            TypedProofTerminal::EmptyRoot => step_count,
            _ => step_count + 1,
        };
        if record_count > policy.max_proof_records as usize {
            return Err(CommitStateError::LimitExceeded {
                field: "max_proof_records",
            });
        }
        Ok(Self { steps, terminal })
    }

    /// Computes the canonical `Receipt` `ContentTag` under `COMMIT_1_POINT_PROOF_SCHEMA`.
    pub fn proof_tag(&self) -> ContentTag {
        let bytes = self.to_canonical_bytes();
        hash_canonical(
            TagKind::Receipt,
            COMMIT_1_POINT_PROOF_SCHEMA,
            bytes.len() as u64,
            |sink| sink(&bytes),
        )
        .expect("typed Patricia proof tag is canonical")
    }
}

/// Caller-supplied expected verification context (`ExpectedProofContextV1`).
///
/// Proof bytes never choose the profile, domain, root type, root digest, codecs, or policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpectedProofContextV1 {
    /// Expected `CommitmentProfileIdV1`.
    pub commitment_profile_id: [u8; 32],
    /// Expected logical domain tag.
    pub domain_tag: DomainTagV1,
    /// Expected typed root family.
    pub root_type: RootTypeV1,
    /// Expected 32-byte root digest selected by caller.
    pub expected_root: [u8; 32],
    /// Expected key codec.
    pub key_codec: KeyCodecIdV1,
    /// Expected value codec.
    pub value_codec: ValueCodecIdV1,
    /// Expected `ProofPolicyIdV1`.
    pub proof_policy_id: [u8; 32],
}

impl ExpectedProofContextV1 {
    /// Builds an expected verification context directly from a profile descriptor and root digest.
    pub fn for_profile(profile: &CommitmentProfileDescriptorV1, expected_root: [u8; 32]) -> Self {
        Self {
            commitment_profile_id: profile.id(),
            domain_tag: profile.domain_tag,
            root_type: profile.root_type,
            expected_root,
            key_codec: profile.key_codec,
            value_codec: profile.value_codec,
            proof_policy_id: profile.proof_policy_id,
        }
    }
}

/// Verified outcome of [`verify_lookup`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LookupResult {
    /// Key is proved absent under the caller-supplied root.
    Absent,
    /// Key is proved present with canonical typed value bytes.
    Present(Vec<u8>),
}

/// Verifies a canonical byte-encoded point witness against `expected_ctx` under `registry` and
/// optional machine-local `local_cap` (which may only tighten, never widen, the admitted policy).
pub fn verify_lookup(
    registry: &ProfileRegistryV1,
    expected_ctx: &ExpectedProofContextV1,
    query_key: &[u8],
    proof_bytes: &[u8],
    local_cap: Option<ProofPolicyDescriptorV1>,
) -> Result<LookupResult, CommitStateError> {
    let profile = registry.resolve_profile(&expected_ctx.commitment_profile_id)?;
    if profile.domain_tag != expected_ctx.domain_tag
        || profile.root_type != expected_ctx.root_type
        || profile.key_codec != expected_ctx.key_codec
        || profile.value_codec != expected_ctx.value_codec
        || profile.proof_policy_id != expected_ctx.proof_policy_id
    {
        return Err(CommitStateError::RegistryMismatch {
            reason: "ExpectedProofContextV1 disagrees with resolved profile",
        });
    }
    let admitted_policy = registry.resolve_proof_policy(&profile.proof_policy_id)?;
    let policy = match local_cap {
        Some(cap) => admitted_policy.tighten(cap)?,
        None => *admitted_policy,
    };
    let key_bits = profile.key_bit_width as usize;
    let key_bytes = profile.key_codec.byte_len();
    if query_key.len() != key_bytes {
        return Err(CommitStateError::InvalidKeyOrValue {
            reason: "query_key byte length disagrees with expected key_codec",
        });
    }
    if key_bits > policy.max_reconstructed_depth as usize {
        return Err(CommitStateError::LimitExceeded {
            field: "max_reconstructed_depth",
        });
    }

    let proof = TypedPatriciaProofV1::from_canonical_bytes_bounded(
        proof_bytes,
        key_bytes,
        key_bits,
        &policy,
    )?;
    let empty_hash = typed_empty_root(profile);

    let mut hash_calls = 0usize;
    let mut hashed_bytes = 0usize;
    let mut total_encoded_node_bytes = 0usize;
    let mut charge_node =
        |payload_len: usize, preimage_len: usize| -> Result<(), CommitStateError> {
            if payload_len > policy.max_single_encoded_node_bytes as usize {
                return Err(CommitStateError::LimitExceeded {
                    field: "max_single_encoded_node_bytes",
                });
            }
            total_encoded_node_bytes = total_encoded_node_bytes.checked_add(payload_len).ok_or(
                CommitStateError::LimitExceeded {
                    field: "max_total_encoded_node_bytes",
                },
            )?;
            if total_encoded_node_bytes > policy.max_total_encoded_node_bytes as usize {
                return Err(CommitStateError::LimitExceeded {
                    field: "max_total_encoded_node_bytes",
                });
            }
            hash_calls += 1;
            if hash_calls > policy.max_hash_invocations as usize {
                return Err(CommitStateError::LimitExceeded {
                    field: "max_hash_invocations",
                });
            }
            hashed_bytes =
                hashed_bytes
                    .checked_add(preimage_len)
                    .ok_or(CommitStateError::LimitExceeded {
                        field: "max_hashed_preimage_bytes",
                    })?;
            if hashed_bytes > policy.max_hashed_preimage_bytes as usize {
                return Err(CommitStateError::LimitExceeded {
                    field: "max_hashed_preimage_bytes",
                });
            }
            Ok(())
        };

    let (mut cur_hash, outcome, mut cur_depth, mut prev_ext) = match &proof.terminal {
        TypedProofTerminal::EmptyRoot => {
            if !proof.steps.is_empty() {
                return Err(CommitStateError::InvalidProof {
                    reason: "non-empty steps with EmptyRoot terminal",
                });
            }
            (empty_hash, LookupResult::Absent, 0usize, false)
        }
        TypedProofTerminal::Leaf { depth, key, value } => {
            let d = *depth as usize;
            if d == 0 && !proof.steps.is_empty() {
                return Err(CommitStateError::InvalidProof {
                    reason: "depth-0 leaf cannot have ancestor steps",
                });
            }
            if d > 0 && !matches!(proof.steps.last(), Some(TypedProofStep::Branch { .. })) {
                return Err(CommitStateError::InvalidProof {
                    reason: "non-root leaf must sit directly under a branch",
                });
            }
            if common_prefix_from(key, query_key, 0, key_bits) < d {
                return Err(CommitStateError::InvalidProof {
                    reason: "leaf key disagrees with query prefix above leaf depth",
                });
            }
            validate_domain_kv(profile, key, value)?;
            let payload = encode_leaf_payload(d, key, value, key_bits);
            let (h, preimage_len) = compute_typed_node_hash(
                &expected_ctx.commitment_profile_id,
                profile.tree_domain(),
                profile.node_codec_version,
                NODE_TYPE_LEAF,
                *depth,
                &payload,
            );
            charge_node(payload.len(), preimage_len)?;
            let outcome = if key.as_slice() == query_key {
                LookupResult::Present(value.clone())
            } else {
                LookupResult::Absent
            };
            (h, outcome, d, false)
        }
        TypedProofTerminal::DivergentExtension {
            depth,
            shared_bits,
            child_hash,
        } => {
            let d = *depth as usize;
            if *child_hash == empty_hash {
                return Err(CommitStateError::InvalidProof {
                    reason: "divergent extension child_hash must not be empty",
                });
            }
            let diverges = shared_bits
                .iter()
                .enumerate()
                .any(|(i, &b)| key_bit(query_key, d + i) != b);
            if !diverges {
                return Err(CommitStateError::InvalidProof {
                    reason: "divergent extension does not diverge from query_key",
                });
            }
            let payload = encode_extension_payload(shared_bits, child_hash);
            let (h, preimage_len) = compute_typed_node_hash(
                &expected_ctx.commitment_profile_id,
                profile.tree_domain(),
                profile.node_codec_version,
                NODE_TYPE_EXTENSION,
                *depth,
                &payload,
            );
            charge_node(payload.len(), preimage_len)?;
            (h, LookupResult::Absent, d, true)
        }
    };

    for step in proof.steps.iter().rev() {
        match step {
            TypedProofStep::Extension { depth, shared_bits } => {
                if prev_ext {
                    return Err(CommitStateError::InvalidProof {
                        reason: "adjacent extensions are non-canonical",
                    });
                }
                let d = *depth as usize;
                if d + shared_bits.len() != cur_depth {
                    return Err(CommitStateError::InvalidProof {
                        reason: "extension step child depth mismatch",
                    });
                }
                if shared_bits
                    .iter()
                    .enumerate()
                    .any(|(i, &b)| key_bit(query_key, d + i) != b)
                {
                    return Err(CommitStateError::InvalidProof {
                        reason: "traversed extension step disagrees with query_key",
                    });
                }
                let payload = encode_extension_payload(shared_bits, &cur_hash);
                let (h, preimage_len) = compute_typed_node_hash(
                    &expected_ctx.commitment_profile_id,
                    profile.tree_domain(),
                    profile.node_codec_version,
                    NODE_TYPE_EXTENSION,
                    *depth,
                    &payload,
                );
                charge_node(payload.len(), preimage_len)?;
                cur_hash = h;
                cur_depth = d;
                prev_ext = true;
            }
            TypedProofStep::Branch {
                depth,
                direction,
                sibling_hash,
            } => {
                let d = *depth as usize;
                if d + 1 != cur_depth
                    || *sibling_hash == empty_hash
                    || key_bit(query_key, d) != *direction
                {
                    return Err(CommitStateError::InvalidProof {
                        reason: "branch step depth, direction, or non-empty sibling violated",
                    });
                }
                let payload = if *direction == 0 {
                    encode_branch_payload(&cur_hash, sibling_hash)
                } else {
                    encode_branch_payload(sibling_hash, &cur_hash)
                };
                let (h, preimage_len) = compute_typed_node_hash(
                    &expected_ctx.commitment_profile_id,
                    profile.tree_domain(),
                    profile.node_codec_version,
                    NODE_TYPE_BRANCH,
                    *depth,
                    &payload,
                );
                charge_node(payload.len(), preimage_len)?;
                cur_hash = h;
                cur_depth = d;
                prev_ext = false;
            }
        }
    }

    if cur_depth != 0 || cur_hash != expected_ctx.expected_root {
        return Err(CommitStateError::InvalidProof {
            reason: "reconstructed root or depth disagrees with expected context",
        });
    }
    Ok(outcome)
}

/// Verifies that `proof_bytes` establishes caller's `expected` absence/presence claim under `expected_ctx`.
pub fn verify_claim(
    registry: &ProfileRegistryV1,
    expected_ctx: &ExpectedProofContextV1,
    query_key: &[u8],
    expected: &LookupResult,
    proof_bytes: &[u8],
    local_cap: Option<ProofPolicyDescriptorV1>,
) -> Result<(), CommitStateError> {
    let observed = verify_lookup(registry, expected_ctx, query_key, proof_bytes, local_cap)?;
    if &observed != expected {
        return Err(CommitStateError::ClaimMismatch);
    }
    Ok(())
}

// =========================================================================
// COMMITMENT SET V1 MANIFEST
// =========================================================================

/// One active root entry inside [`CommitmentSetV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RootEntryV1 {
    /// Active required-domain identity projected from `DomainRegistryV1`.
    pub required_domain: RequiredDomainV1,
    /// Exact 32-byte canonical root digest for this domain.
    pub root_bytes: [u8; 32],
}

/// Tagged subject binding a [`CommitmentSetV1`] to a local storage profile and sequence number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitmentSubjectV1 {
    /// Local single-machine authenticated transition sequence (`COMMIT-1`).
    LocalTransitionV1 {
        /// Digest of the active `LocalStorageProfile`.
        local_storage_profile_id: [u8; 32],
        /// Digest of the active `BaseTransitionSpecD1` or local scope policy.
        scope_digest: [u8; 32],
        /// Digest of the active `DomainRegistryV1`.
        domain_registry_digest: [u8; 32],
        /// Monotonic local transition counter (`0` at genesis).
        ledger_sequence: u64,
    },
}

/// Tagged predecessor link inside [`CommitmentSetV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitmentSetPredecessorV1 {
    /// Genesis commitment set (legal only when `ledger_sequence == 0`).
    GenesisCommitmentSetV1,
    /// Ordinary successor naming the exact prior `CommitmentSetIdV1` (`ledger_sequence == prior + 1`).
    OrdinaryCommitmentSuccessorV1 {
        /// 32-byte `CommitmentSetIdV1` of the immediate predecessor set.
        prior_commitment_set_id: [u8; 32],
    },
}

/// Immutable multi-root manifest binding one ledger head and all activated typed roots (`COMMIT_1_COMMITMENT_SET_SCHEMA`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitmentSetV1 {
    /// Manifest schema version (must be `1`).
    pub schema_version: u16,
    /// Tagged subject naming local storage profile, scope, domain registry, and sequence.
    pub subject: CommitmentSubjectV1,
    /// Exact 32-byte digest of the committed ledger head (`History::head().digest()`).
    pub ledger_head_id: [u8; 32],
    /// Canonically sorted active required-domain set projected from `DomainRegistryV1`.
    pub required_domain_set: Vec<RequiredDomainV1>,
    /// One `RootEntryV1` per required domain in matching ascending order.
    pub root_entries: Vec<RootEntryV1>,
    /// Predecessor relation (genesis at sequence 0 or ordinary successor at `prior + 1`).
    pub predecessor: CommitmentSetPredecessorV1,
}

impl CommitmentSetV1 {
    /// Validates manifest consistency against `profiles`, `domain_registry`, and optional `prior`.
    pub fn validate(
        &self,
        profiles: &ProfileRegistryV1,
        domain_registry: &DomainRegistryV1,
        prior: Option<&CommitmentSetV1>,
    ) -> Result<(), CommitStateError> {
        if self.schema_version != COMMIT_1_SCHEMA_VERSION {
            return Err(CommitStateError::WrongVersion {
                expected: COMMIT_1_SCHEMA_VERSION,
                actual: self.schema_version,
            });
        }
        domain_registry.validate_against(profiles)?;
        let CommitmentSubjectV1::LocalTransitionV1 {
            domain_registry_digest,
            ledger_sequence,
            ..
        } = self.subject;
        if domain_registry_digest != domain_registry.digest() {
            return Err(CommitStateError::InvariantViolation {
                reason: "CommitmentSetV1 domain_registry_digest mismatch",
            });
        }
        let expected_required = domain_registry.required_domains();
        if self.required_domain_set != expected_required {
            return Err(CommitStateError::InvariantViolation {
                reason: "CommitmentSetV1 required_domain_set disagrees with DomainRegistryV1",
            });
        }
        if self.root_entries.len() != self.required_domain_set.len() {
            return Err(CommitStateError::InvariantViolation {
                reason: "CommitmentSetV1 root_entries count disagrees with required_domain_set",
            });
        }
        for (req, entry) in self
            .required_domain_set
            .iter()
            .zip(self.root_entries.iter())
        {
            if &entry.required_domain != req {
                return Err(CommitStateError::InvariantViolation {
                    reason: "CommitmentSetV1 root_entry domain/profile order mismatch",
                });
            }
            let _ = profiles.resolve_profile(&req.commitment_profile_id)?;
        }
        match (self.predecessor, ledger_sequence, prior) {
            (CommitmentSetPredecessorV1::GenesisCommitmentSetV1, 0, None) => {}
            (
                CommitmentSetPredecessorV1::OrdinaryCommitmentSuccessorV1 {
                    prior_commitment_set_id,
                },
                seq,
                Some(p),
            ) if seq > 0 => {
                let CommitmentSubjectV1::LocalTransitionV1 {
                    local_storage_profile_id: p_lsp,
                    scope_digest: p_scope,
                    domain_registry_digest: p_reg,
                    ledger_sequence: p_seq,
                } = p.subject;
                let CommitmentSubjectV1::LocalTransitionV1 {
                    local_storage_profile_id: c_lsp,
                    scope_digest: c_scope,
                    domain_registry_digest: c_reg,
                    ledger_sequence: c_seq,
                } = self.subject;
                if p_lsp != c_lsp || p_scope != c_scope || p_reg != c_reg {
                    return Err(CommitStateError::InvariantViolation {
                        reason: "ordinary successor crossed local subject boundary",
                    });
                }
                if p_seq.checked_add(1) != Some(c_seq) {
                    return Err(CommitStateError::InvariantViolation {
                        reason: "ordinary successor ledger_sequence must increment by exactly 1",
                    });
                }
                if prior_commitment_set_id != p.id() {
                    return Err(CommitStateError::InvariantViolation {
                        reason: "ordinary successor prior_commitment_set_id mismatch",
                    });
                }
            }
            (CommitmentSetPredecessorV1::OrdinaryCommitmentSuccessorV1 { .. }, seq, None)
                if seq > 0 => {}
            _ => {
                return Err(CommitStateError::InvariantViolation {
                    reason: "invalid CommitmentSetV1 predecessor/sequence relation",
                });
            }
        }
        Ok(())
    }

    /// Looks up the 32-byte root digest for an active `domain`, returning `None` if absent.
    pub fn root_for(&self, domain: DomainTagV1) -> Option<[u8; 32]> {
        self.root_entries
            .iter()
            .find(|e| e.required_domain.domain_tag == domain)
            .map(|e| e.root_bytes)
    }

    /// Encodes this commitment set into canonical big-endian bytes.
    pub fn encode_canonical(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&self.schema_version.to_be_bytes());
        match self.subject {
            CommitmentSubjectV1::LocalTransitionV1 {
                local_storage_profile_id,
                scope_digest,
                domain_registry_digest,
                ledger_sequence,
            } => {
                out.push(SUBJECT_TAG_LOCAL_TRANSITION);
                out.extend_from_slice(&local_storage_profile_id);
                out.extend_from_slice(&scope_digest);
                out.extend_from_slice(&domain_registry_digest);
                out.extend_from_slice(&ledger_sequence.to_be_bytes());
            }
        }
        out.extend_from_slice(&self.ledger_head_id);
        out.extend_from_slice(&(self.required_domain_set.len() as u16).to_be_bytes());
        for req in &self.required_domain_set {
            req.encode_canonical(&mut out);
        }
        out.extend_from_slice(&(self.root_entries.len() as u16).to_be_bytes());
        for entry in &self.root_entries {
            entry.required_domain.encode_canonical(&mut out);
            out.extend_from_slice(&entry.root_bytes);
        }
        match self.predecessor {
            CommitmentSetPredecessorV1::GenesisCommitmentSetV1 => {
                out.push(PREDECESSOR_TAG_GENESIS);
            }
            CommitmentSetPredecessorV1::OrdinaryCommitmentSuccessorV1 {
                prior_commitment_set_id,
            } => {
                out.push(PREDECESSOR_TAG_ORDINARY);
                out.extend_from_slice(&prior_commitment_set_id);
            }
        }
        out
    }

    /// Decodes a canonical `CommitmentSetV1` from wire bytes.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, CommitStateError> {
        let mut r = WireReader::new(bytes);
        let schema_version = r.u16("commitment set schema_version")?;
        if schema_version != COMMIT_1_SCHEMA_VERSION {
            return Err(CommitStateError::WrongVersion {
                expected: COMMIT_1_SCHEMA_VERSION,
                actual: schema_version,
            });
        }
        let subject = match r.u8("commitment set subject tag")? {
            SUBJECT_TAG_LOCAL_TRANSITION => CommitmentSubjectV1::LocalTransitionV1 {
                local_storage_profile_id: r.bytes32("subject local_storage_profile_id")?,
                scope_digest: r.bytes32("subject scope_digest")?,
                domain_registry_digest: r.bytes32("subject domain_registry_digest")?,
                ledger_sequence: r.u64("subject ledger_sequence")?,
            },
            _ => {
                return Err(CommitStateError::MalformedEncoding {
                    reason: "unsupported CommitmentSubjectV1 tag",
                });
            }
        };
        let ledger_head_id = r.bytes32("commitment set ledger_head_id")?;
        let req_count = r.u16("commitment set required_domain_set len")? as usize;
        let mut required_domain_set = Vec::with_capacity(req_count.min(16));
        for _ in 0..req_count {
            required_domain_set.push(RequiredDomainV1::decode_from(&mut r)?);
        }
        let entry_count = r.u16("commitment set root_entries len")? as usize;
        let mut root_entries = Vec::with_capacity(entry_count.min(16));
        for _ in 0..entry_count {
            let required_domain = RequiredDomainV1::decode_from(&mut r)?;
            let root_bytes = r.bytes32("root entry root_bytes")?;
            root_entries.push(RootEntryV1 {
                required_domain,
                root_bytes,
            });
        }
        let predecessor = match r.u8("commitment set predecessor tag")? {
            PREDECESSOR_TAG_GENESIS => CommitmentSetPredecessorV1::GenesisCommitmentSetV1,
            PREDECESSOR_TAG_ORDINARY => CommitmentSetPredecessorV1::OrdinaryCommitmentSuccessorV1 {
                prior_commitment_set_id: r.bytes32("predecessor prior_commitment_set_id")?,
            },
            _ => {
                return Err(CommitStateError::MalformedEncoding {
                    reason: "unsupported CommitmentSetPredecessorV1 tag",
                });
            }
        };
        r.finish()?;
        Ok(Self {
            schema_version,
            subject,
            ledger_head_id,
            required_domain_set,
            root_entries,
            predecessor,
        })
    }

    /// Computes the canonical `Receipt` `ContentTag` under `COMMIT_1_COMMITMENT_SET_SCHEMA`.
    pub fn tag(&self) -> ContentTag {
        let bytes = self.encode_canonical();
        hash_canonical(
            TagKind::Receipt,
            COMMIT_1_COMMITMENT_SET_SCHEMA,
            bytes.len() as u64,
            |sink| sink(&bytes),
        )
        .expect("CommitmentSetV1 tag is canonical")
    }

    /// Returns the 32-byte `CommitmentSetIdV1` digest.
    pub fn id(&self) -> [u8; 32] {
        self.tag().digest()
    }
}

// =========================================================================
// INDEPENDENT PROJECTION REBUILD & INCREMENTAL SIDECAR STATE
// =========================================================================

/// Three-domain authenticated roots rebuilt independently from a verified ledger snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectedRootsV1 {
    /// Authenticated world occupancy root (`WorldRootV1`).
    pub world_root: WorldRootV1,
    /// Authenticated live mutable episode map root (`ActiveTokenRootV1`).
    pub active_token_root: ActiveTokenRootV1,
    /// Authenticated retained episode provenance root (`EpisodeRootV1`).
    pub episode_root: EpisodeRootV1,
}

/// Independently projects and cold-rebuilds `WorldRootV1`, `ActiveTokenRootV1`, and `EpisodeRootV1`
/// from `(world, episodes)` and verifies cross-map domain equalities against `initial_world`.
pub fn rebuild_projected_tries(
    initial_world: &World,
    current_world: &World,
    episodes: &OccupancyEpisodeLedger,
    op_limits: &CommitmentOperationalLimitsDescriptorV1,
) -> Result<(TypedPatriciaTrie, TypedPatriciaTrie, TypedPatriciaTrie), CommitStateError> {
    if !episodes.verify_world_bijection(initial_world, current_world) {
        return Err(CommitStateError::InvariantViolation {
            reason: "ledger failed world bijection before projection rebuild",
        });
    }
    let world_entries: Vec<(Vec<u8>, Vec<u8>)> = current_world
        .iter_on()
        .map(|c| {
            (
                CoordinateKeyV1::from_coord(c).as_bytes().to_vec(),
                ON_V1_VALUE_BYTES.to_vec(),
            )
        })
        .collect();
    let world_trie = TypedPatriciaTrie::from_entries(
        CommitmentProfileDescriptorV1::world_v1(),
        op_limits,
        world_entries,
    )?;

    let mut active_entries = Vec::with_capacity(episodes.active_count());
    let mut episode_entries = Vec::with_capacity(episodes.active_count() + episodes.closed_count());
    for (&coord, ep) in episodes.active_episodes() {
        let mint = MintIdV1::from_event_tag(ep.mint())?;
        active_entries.push((
            CoordinateKeyV1::from_coord(coord).as_bytes().to_vec(),
            mint.as_bytes().to_vec(),
        ));
        let rec = EpisodeRecordV1::from_episode(ep)?;
        episode_entries.push((mint.as_bytes().to_vec(), rec.encode_canonical()));
    }
    for ep in episodes.closed_episodes().values() {
        let rec = EpisodeRecordV1::from_episode(ep)?;
        episode_entries.push((rec.mint.as_bytes().to_vec(), rec.encode_canonical()));
    }

    let active_trie = TypedPatriciaTrie::from_entries(
        CommitmentProfileDescriptorV1::active_token_v1(),
        op_limits,
        active_entries,
    )?;
    let episode_trie = TypedPatriciaTrie::from_entries(
        CommitmentProfileDescriptorV1::episode_v1(),
        op_limits,
        episode_entries,
    )?;
    Ok((world_trie, active_trie, episode_trie))
}

/// Replays `history` independently from `initial_world`, applies any explicitly pruned closed mint
/// exclusions, cold-rebuilds all three roots, and asserts exact match with `commitment_set`.
pub fn verify_independent_projection(
    initial_world: &World,
    history: &History,
    pruned_closed_mints: &HashSet<ContentTag>,
    commitment_set: &CommitmentSetV1,
    profiles: &ProfileRegistryV1,
    domain_registry: &DomainRegistryV1,
    replay_limits: HistoryReplayLimits,
) -> Result<ProjectedRootsV1, CommitStateError> {
    commitment_set.validate(profiles, domain_registry, None)?;
    let verified = verify_in_memory_history_with_limits(initial_world, history, replay_limits)
        .map_err(|e| CommitStateError::InvariantViolation {
            reason: match e {
                crate::history_store::InMemoryHistoryVerificationError::Semantic(_) => {
                    "independent history replay semantic verification failed"
                }
                crate::history_store::InMemoryHistoryVerificationError::Limit(_) => {
                    "independent history replay exceeded limits"
                }
            },
        })?;
    if verified.history().head().digest() != commitment_set.ledger_head_id {
        return Err(CommitStateError::InvariantViolation {
            reason: "commitment_set ledger_head_id disagrees with verified history head",
        });
    }
    let mut episodes = OccupancyEpisodeLedger::from_history_with_limits(
        initial_world,
        verified.history(),
        replay_limits,
    )?;
    if !pruned_closed_mints.is_empty() {
        let mut retained_pins = HashSet::new();
        for &mint in episodes.closed_episodes().keys() {
            if !pruned_closed_mints.contains(&mint) {
                retained_pins.insert(mint);
            }
        }
        let max_prune = pruned_closed_mints.len();
        episodes.prune_closed_provenance(max_prune, &retained_pins);
    }
    let op_limits = CommitmentOperationalLimitsDescriptorV1::LOCAL_DEFAULT;
    let (wt, at, et) =
        rebuild_projected_tries(initial_world, verified.final_world(), &episodes, &op_limits)?;

    let projected = ProjectedRootsV1 {
        world_root: WorldRootV1 {
            root: wt.root(),
            profile_id: wt.profile().id(),
        },
        active_token_root: ActiveTokenRootV1 {
            root: at.root(),
            profile_id: at.profile().id(),
        },
        episode_root: EpisodeRootV1 {
            root: et.root(),
            profile_id: et.profile().id(),
        },
    };
    if commitment_set.root_for(DomainTagV1::WorldOccupancy) != Some(projected.world_root.root)
        || commitment_set.root_for(DomainTagV1::ActiveToken)
            != Some(projected.active_token_root.root)
        || commitment_set.root_for(DomainTagV1::Episode) != Some(projected.episode_root.root)
    {
        return Err(CommitStateError::InvariantViolation {
            reason: "independent projection roots disagree with CommitmentSetV1",
        });
    }
    Ok(projected)
}

/// Incremental three-domain sidecar maintaining `WorldRootV1`, `ActiveTokenRootV1`, and `EpisodeRootV1`.
#[derive(Clone, Debug)]
pub struct LocalAuthenticatedSidecar {
    world_trie: TypedPatriciaTrie,
    active_token_trie: TypedPatriciaTrie,
    episode_trie: TypedPatriciaTrie,
    local_storage_profile_id: [u8; 32],
    scope_digest: [u8; 32],
    domain_registry: DomainRegistryV1,
    profiles: ProfileRegistryV1,
    op_limits: CommitmentOperationalLimitsDescriptorV1,
    current_set: CommitmentSetV1,
}

impl LocalAuthenticatedSidecar {
    /// Initializes a genesis sidecar (`ledger_sequence = 0`) from `(initial_world, history, episodes)`.
    pub fn from_genesis(
        initial_world: &World,
        history: &History,
        episodes: &OccupancyEpisodeLedger,
        storage_profile: &LocalStorageProfile,
        scope_digest: [u8; 32],
    ) -> Result<Self, CommitStateError> {
        let op_limits = CommitmentOperationalLimitsDescriptorV1::LOCAL_DEFAULT;
        let (world_trie, active_token_trie, episode_trie) =
            rebuild_projected_tries(initial_world, initial_world, episodes, &op_limits)?;
        let domain_registry = DomainRegistryV1::local_v1();
        let profiles = ProfileRegistryV1::local_v1();
        let local_storage_profile_id = storage_profile.tag().digest();
        let current_set = build_commitment_set(
            local_storage_profile_id,
            scope_digest,
            &domain_registry,
            0,
            history.head().digest(),
            world_trie.root(),
            active_token_trie.root(),
            episode_trie.root(),
            CommitmentSetPredecessorV1::GenesisCommitmentSetV1,
        );
        current_set.validate(&profiles, &domain_registry, None)?;
        Ok(Self {
            world_trie,
            active_token_trie,
            episode_trie,
            local_storage_profile_id,
            scope_digest,
            domain_registry,
            profiles,
            op_limits,
            current_set,
        })
    }

    /// Incrementally advances all three tries for one committed `receipt`, verifies exact parity
    /// with a fresh full rebuild, and constructs the successor [`CommitmentSetV1`].
    pub fn advance_for_receipt(
        &mut self,
        initial_world: &World,
        current_world: &World,
        history: &History,
        episodes: &OccupancyEpisodeLedger,
        receipt: &Receipt,
    ) -> Result<CommitmentSetV1, CommitStateError> {
        let mut next_world = self.world_trie.clone();
        let mut next_active = self.active_token_trie.clone();
        let mut next_episode = self.episode_trie.clone();

        for effect in receipt.effects() {
            match *effect {
                TransitionEffect::Mint { coord, event } => {
                    let ckey = CoordinateKeyV1::from_coord(coord).as_bytes().to_vec();
                    let mint = MintIdV1::from_event_tag(event)?;
                    next_world.upsert(ckey.clone(), ON_V1_VALUE_BYTES.to_vec())?;
                    next_active.upsert(ckey, mint.as_bytes().to_vec())?;
                    let rec = EpisodeRecordV1 {
                        coord,
                        mint,
                        consuming_burn: None,
                    };
                    next_episode.upsert(mint.as_bytes().to_vec(), rec.encode_canonical())?;
                }
                TransitionEffect::Burn {
                    coord,
                    event,
                    consumed_mint,
                } => {
                    let ckey = CoordinateKeyV1::from_coord(coord).as_bytes().to_vec();
                    next_world.delete(&ckey)?;
                    if consumed_mint != history.genesis() {
                        next_active.delete(&ckey)?;
                        let mint = MintIdV1::from_event_tag(consumed_mint)?;
                        let burn = EventIdV1::from_event_tag(event)?;
                        let rec = EpisodeRecordV1 {
                            coord,
                            mint,
                            consuming_burn: Some(burn),
                        };
                        next_episode.upsert(mint.as_bytes().to_vec(), rec.encode_canonical())?;
                    }
                }
                TransitionEffect::NoOp { .. } => {}
            }
        }

        // Verify incremental == cold full rebuild on every transition.
        let (rebuilt_w, rebuilt_a, rebuilt_e) =
            rebuild_projected_tries(initial_world, current_world, episodes, &self.op_limits)?;
        if next_world.root() != rebuilt_w.root()
            || next_active.root() != rebuilt_a.root()
            || next_episode.root() != rebuilt_e.root()
        {
            return Err(CommitStateError::InvariantViolation {
                reason: "incremental trie update diverged from cold full rebuild",
            });
        }

        let CommitmentSubjectV1::LocalTransitionV1 {
            ledger_sequence, ..
        } = self.current_set.subject;
        let next_seq = ledger_sequence
            .checked_add(1)
            .ok_or(CommitStateError::LimitExceeded {
                field: "ledger_sequence",
            })?;
        let next_set = build_commitment_set(
            self.local_storage_profile_id,
            self.scope_digest,
            &self.domain_registry,
            next_seq,
            history.head().digest(),
            next_world.root(),
            next_active.root(),
            next_episode.root(),
            CommitmentSetPredecessorV1::OrdinaryCommitmentSuccessorV1 {
                prior_commitment_set_id: self.current_set.id(),
            },
        );
        next_set.validate(
            &self.profiles,
            &self.domain_registry,
            Some(&self.current_set),
        )?;

        self.world_trie = next_world;
        self.active_token_trie = next_active;
        self.episode_trie = next_episode;
        self.current_set = next_set.clone();
        Ok(next_set)
    }

    /// Applies an authorized logical episode-pruning transition removing closed `pruned_mints`
    /// from `EpisodeRootV1` while proving `WorldRootV1` and `ActiveTokenRootV1` stay identical.
    pub fn advance_for_logical_episode_prune(
        &mut self,
        initial_world: &World,
        current_world: &World,
        history: &History,
        episodes: &OccupancyEpisodeLedger,
        pruned_mints: &[ContentTag],
    ) -> Result<CommitmentSetV1, CommitStateError> {
        let mut next_episode = self.episode_trie.clone();
        for &mint_tag in pruned_mints {
            let key = EventIdV1::from_event_tag(mint_tag)?;
            next_episode.delete(key.as_bytes())?;
        }
        let (rebuilt_w, rebuilt_a, rebuilt_e) =
            rebuild_projected_tries(initial_world, current_world, episodes, &self.op_limits)?;
        if self.world_trie.root() != rebuilt_w.root()
            || self.active_token_trie.root() != rebuilt_a.root()
            || next_episode.root() != rebuilt_e.root()
        {
            return Err(CommitStateError::InvariantViolation {
                reason: "logical episode prune diverged from full projection rebuild",
            });
        }
        let CommitmentSubjectV1::LocalTransitionV1 {
            ledger_sequence, ..
        } = self.current_set.subject;
        let next_seq = ledger_sequence
            .checked_add(1)
            .ok_or(CommitStateError::LimitExceeded {
                field: "ledger_sequence",
            })?;
        let next_set = build_commitment_set(
            self.local_storage_profile_id,
            self.scope_digest,
            &self.domain_registry,
            next_seq,
            history.head().digest(),
            self.world_trie.root(),
            self.active_token_trie.root(),
            next_episode.root(),
            CommitmentSetPredecessorV1::OrdinaryCommitmentSuccessorV1 {
                prior_commitment_set_id: self.current_set.id(),
            },
        );
        next_set.validate(
            &self.profiles,
            &self.domain_registry,
            Some(&self.current_set),
        )?;
        self.episode_trie = next_episode;
        self.current_set = next_set.clone();
        Ok(next_set)
    }

    /// Borrows the current `WorldRootV1` trie.
    pub const fn world_trie(&self) -> &TypedPatriciaTrie {
        &self.world_trie
    }

    /// Borrows the current `ActiveTokenRootV1` trie.
    pub const fn active_token_trie(&self) -> &TypedPatriciaTrie {
        &self.active_token_trie
    }

    /// Borrows the current `EpisodeRootV1` trie.
    pub const fn episode_trie(&self) -> &TypedPatriciaTrie {
        &self.episode_trie
    }

    /// Borrows the current immutable [`CommitmentSetV1`].
    pub const fn current_set(&self) -> &CommitmentSetV1 {
        &self.current_set
    }

    /// Returns the typed `WorldRootV1` wrapper.
    pub fn world_root(&self) -> WorldRootV1 {
        WorldRootV1 {
            root: self.world_trie.root(),
            profile_id: self.world_trie.profile().id(),
        }
    }

    /// Returns the typed `ActiveTokenRootV1` wrapper.
    pub fn active_token_root(&self) -> ActiveTokenRootV1 {
        ActiveTokenRootV1 {
            root: self.active_token_trie.root(),
            profile_id: self.active_token_trie.profile().id(),
        }
    }

    /// Returns the typed `EpisodeRootV1` wrapper.
    pub fn episode_root(&self) -> EpisodeRootV1 {
        EpisodeRootV1 {
            root: self.episode_trie.root(),
            profile_id: self.episode_trie.profile().id(),
        }
    }

    /// Collects all content-addressed node records across all three active domains for staging.
    pub fn collect_all_node_records(&self) -> BTreeMap<NodeStoreKey, Vec<u8>> {
        let mut out = BTreeMap::new();
        for trie in [
            &self.world_trie,
            &self.active_token_trie,
            &self.episode_trie,
        ] {
            let pid = trie.profile().id();
            let domain = trie.profile().domain_tag;
            for (hash, bytes) in trie.collect_node_records() {
                out.insert(
                    NodeStoreKey {
                        commitment_profile_id: pid,
                        domain_tag: domain,
                        node_hash: hash,
                    },
                    bytes,
                );
            }
        }
        out
    }
}

#[allow(clippy::too_many_arguments)]
fn build_commitment_set(
    local_storage_profile_id: [u8; 32],
    scope_digest: [u8; 32],
    domain_registry: &DomainRegistryV1,
    ledger_sequence: u64,
    ledger_head_id: [u8; 32],
    world_root: [u8; 32],
    active_token_root: [u8; 32],
    episode_root: [u8; 32],
    predecessor: CommitmentSetPredecessorV1,
) -> CommitmentSetV1 {
    let required = domain_registry.required_domains();
    let root_entries = required
        .iter()
        .map(|&req| {
            let root_bytes = match req.domain_tag {
                DomainTagV1::WorldOccupancy => world_root,
                DomainTagV1::ActiveToken => active_token_root,
                DomainTagV1::Episode => episode_root,
                _ => unreachable!("only local domains are active in local_v1"),
            };
            RootEntryV1 {
                required_domain: req,
                root_bytes,
            }
        })
        .collect();
    CommitmentSetV1 {
        schema_version: COMMIT_1_SCHEMA_VERSION,
        subject: CommitmentSubjectV1::LocalTransitionV1 {
            local_storage_profile_id,
            scope_digest,
            domain_registry_digest: domain_registry.digest(),
            ledger_sequence,
        },
        ledger_head_id,
        required_domain_set: required,
        root_entries,
        predecessor,
    }
}

// =========================================================================
// CONTENT-ADDRESSED NODE STORE, PROVISIONAL PINS, GC WRITE BARRIER, CRASH RECOVERY
// =========================================================================

/// Composite storage key `(commitment_profile_id, domain_tag, node_hash)` preventing cross-domain aliasing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeStoreKey {
    /// Derived `CommitmentProfileIdV1`.
    pub commitment_profile_id: [u8; 32],
    /// Logical state domain tag.
    pub domain_tag: DomainTagV1,
    /// 32-byte `NodeHash` digest of the canonical node record.
    pub node_hash: [u8; 32],
}

/// Deterministic crash-injection point inside the staged publication pipeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrashPoint {
    /// Complete publication normally without injecting a crash.
    None,
    /// Crash immediately after staging nodes and verifying reachability, before writing provisional pin.
    AfterNodeStaging,
    /// Crash immediately after installing and rereading the provisional pin, before writing manifest.
    AfterProvisionalPin,
    /// Crash immediately after writing and rereading the manifest, before head compare-and-swap.
    AfterManifestWrite,
    /// Crash immediately after head CAS succeeds, before promoting provisional pin to permanent.
    AfterHeadCas,
    /// Crash immediately after pin promotion, before releasing superseded provisional pin.
    AfterPinPromotion,
}

/// Checksummed provisional-pin record protecting all candidate roots across publication and GC.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvisionalPinRecordV1 {
    /// Monotonic publication attempt identifier.
    pub attempt_id: u64,
    /// Pin epoch assigned when this provisional pin was installed.
    pub pin_epoch: u64,
    /// Expected prior `CommitmentSetIdV1` (`None` for genesis).
    pub expected_prior_set_id: Option<[u8; 32]>,
    /// Candidate `CommitmentSetIdV1` being published.
    pub candidate_set_id: [u8; 32],
    /// Canonically sorted root node keys protected by this pin.
    pub root_keys: Vec<NodeStoreKey>,
}

impl ProvisionalPinRecordV1 {
    /// Encodes this provisional pin with a trailing 32-byte SHA-256 checksum.
    pub fn encode_checksummed(&self) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(&PIN_RECORD_VERSION.to_be_bytes());
        body.extend_from_slice(&self.attempt_id.to_be_bytes());
        body.extend_from_slice(&self.pin_epoch.to_be_bytes());
        match self.expected_prior_set_id {
            None => body.push(0),
            Some(prior) => {
                body.push(1);
                body.extend_from_slice(&prior);
            }
        }
        body.extend_from_slice(&self.candidate_set_id);
        body.extend_from_slice(&(self.root_keys.len() as u16).to_be_bytes());
        for rk in &self.root_keys {
            body.extend_from_slice(&rk.commitment_profile_id);
            body.push(rk.domain_tag.code());
            body.extend_from_slice(&rk.node_hash);
        }
        let checksum: [u8; 32] = Sha256::digest(&body).into();
        body.extend_from_slice(&checksum);
        body
    }

    /// Verifies the trailing SHA-256 checksum and decodes a `ProvisionalPinRecordV1`.
    pub fn decode_checksummed(bytes: &[u8]) -> Result<Self, CommitStateError> {
        if bytes.len() < 32 {
            return Err(CommitStateError::MalformedEncoding {
                reason: "provisional pin shorter than checksum",
            });
        }
        let (body, sum) = bytes.split_at(bytes.len() - 32);
        let expected: [u8; 32] = Sha256::digest(body).into();
        if sum != expected {
            return Err(CommitStateError::MalformedEncoding {
                reason: "provisional pin checksum mismatch",
            });
        }
        let mut r = WireReader::new(body);
        let ver = r.u16("pin version")?;
        if ver != PIN_RECORD_VERSION {
            return Err(CommitStateError::WrongVersion {
                expected: PIN_RECORD_VERSION,
                actual: ver,
            });
        }
        let attempt_id = r.u64("pin attempt_id")?;
        let pin_epoch = r.u64("pin pin_epoch")?;
        let expected_prior_set_id = match r.u8("pin prior flag")? {
            0 => None,
            1 => Some(r.bytes32("pin expected_prior_set_id")?),
            _ => {
                return Err(CommitStateError::MalformedEncoding {
                    reason: "invalid pin prior flag",
                });
            }
        };
        let candidate_set_id = r.bytes32("pin candidate_set_id")?;
        let count = r.u16("pin root_keys count")? as usize;
        let mut root_keys = Vec::with_capacity(count.min(16));
        for _ in 0..count {
            let pid = r.bytes32("pin root profile_id")?;
            let domain_tag = DomainTagV1::from_code(r.u8("pin root domain_tag")?).ok_or(
                CommitStateError::MalformedEncoding {
                    reason: "unknown pin domain_tag",
                },
            )?;
            let node_hash = r.bytes32("pin root node_hash")?;
            root_keys.push(NodeStoreKey {
                commitment_profile_id: pid,
                domain_tag,
                node_hash,
            });
        }
        r.finish()?;
        Ok(Self {
            attempt_id,
            pin_epoch,
            expected_prior_set_id,
            candidate_set_id,
            root_keys,
        })
    }
}

/// Checksummed recoverable head pointer record (`head.ptr`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeadPointerRecordV1 {
    /// Current authoritative `CommitmentSetIdV1`.
    pub head_set_id: [u8; 32],
    /// Monotonic pin epoch observed at head publication.
    pub pin_epoch: u64,
}

impl HeadPointerRecordV1 {
    /// Encodes this head pointer with a trailing 32-byte SHA-256 checksum.
    pub fn encode_checksummed(&self) -> Vec<u8> {
        let mut body = Vec::with_capacity(2 + 32 + 8 + 32);
        body.extend_from_slice(&HEAD_RECORD_VERSION.to_be_bytes());
        body.extend_from_slice(&self.head_set_id);
        body.extend_from_slice(&self.pin_epoch.to_be_bytes());
        let sum: [u8; 32] = Sha256::digest(&body).into();
        body.extend_from_slice(&sum);
        body
    }

    /// Verifies the trailing SHA-256 checksum and decodes a `HeadPointerRecordV1`.
    pub fn decode_checksummed(bytes: &[u8]) -> Result<Self, CommitStateError> {
        if bytes.len() < 32 {
            return Err(CommitStateError::MalformedEncoding {
                reason: "head pointer shorter than checksum",
            });
        }
        let (body, sum) = bytes.split_at(bytes.len() - 32);
        let expected: [u8; 32] = Sha256::digest(body).into();
        if sum != expected {
            return Err(CommitStateError::MalformedEncoding {
                reason: "head pointer checksum mismatch",
            });
        }
        let mut r = WireReader::new(body);
        let ver = r.u16("head version")?;
        if ver != HEAD_RECORD_VERSION {
            return Err(CommitStateError::WrongVersion {
                expected: HEAD_RECORD_VERSION,
                actual: ver,
            });
        }
        let head_set_id = r.bytes32("head_set_id")?;
        let pin_epoch = r.u64("head pin_epoch")?;
        r.finish()?;
        Ok(Self {
            head_set_id,
            pin_epoch,
        })
    }
}

/// Snapshot captured at the start of a concurrent GC mark pass.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GcMarkSnapshot {
    /// Exact pin epoch observed when marking began and ended.
    pub pin_epoch: u64,
    /// Complete set of reachable `NodeStoreKey`s traced from all pinned roots.
    pub reachable: BTreeSet<NodeStoreKey>,
    /// Deterministic work units consumed during mark.
    pub work_units: u64,
}

/// Outcome of a bounded GC sweep pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GcSweepOutcome {
    /// Pin epoch moved during or after mark; sweep aborted safely with zero nodes reclaimed.
    AbortedEpochChanged {
        /// Epoch captured at mark time.
        mark_epoch: u64,
        /// Current store pin epoch.
        current_epoch: u64,
    },
    /// Sweep completed under stable epoch and reclaimed `reclaimed_nodes` unreferenced orphans.
    Completed {
        /// Number of orphan nodes deleted.
        reclaimed_nodes: usize,
        /// Number of remaining live nodes in store.
        remaining_nodes: usize,
    },
}

/// Report emitted by commitment-store crash recovery.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreRecoveryReport {
    /// Recovered authoritative head `CommitmentSetIdV1`, if any head was ever published.
    pub recovered_head: Option<[u8; 32]>,
    /// Provisional pins promoted because they matched the recovered head.
    pub promoted_provisional_pins: usize,
    /// Unreferenced provisional pins safely released after proving non-reference.
    pub released_provisional_pins: usize,
    /// Total reachable nodes verified across the recovered head closure.
    pub verified_reachable_nodes: usize,
}

fn root_keys_for_set(set: &CommitmentSetV1) -> Vec<NodeStoreKey> {
    let mut keys: Vec<NodeStoreKey> = set
        .root_entries
        .iter()
        .map(|e| NodeStoreKey {
            commitment_profile_id: e.required_domain.commitment_profile_id,
            domain_tag: e.required_domain.domain_tag,
            node_hash: e.root_bytes,
        })
        .collect();
    keys.sort();
    keys.dedup();
    keys
}

fn verify_closure_from_lookup<F>(
    profiles: &ProfileRegistryV1,
    op_limits: &CommitmentOperationalLimitsDescriptorV1,
    roots: &[NodeStoreKey],
    mut read_node: F,
) -> Result<(BTreeSet<NodeStoreKey>, u64), CommitStateError>
where
    F: FnMut(&NodeStoreKey) -> Result<Vec<u8>, CommitStateError>,
{
    let mut visited = BTreeSet::new();
    let mut stack: Vec<NodeStoreKey> = roots.to_vec();
    let mut work = 0u64;
    while let Some(key) = stack.pop() {
        let profile = profiles.resolve_profile(&key.commitment_profile_id)?;
        if key.node_hash == typed_empty_root(profile) {
            continue;
        }
        if !visited.insert(key) {
            continue;
        }
        work = work.saturating_add(1);
        if (visited.len() as u64) > op_limits.max_gc_mark_nodes
            || work > op_limits.max_gc_work_units
        {
            return Err(CommitStateError::LimitExceeded {
                field: "max_gc_mark_nodes",
            });
        }
        let bytes = read_node(&key)?;
        if bytes.len() > op_limits.max_node_store_record_bytes as usize {
            return Err(CommitStateError::LimitExceeded {
                field: "max_node_store_record_bytes",
            });
        }
        match decode_and_verify_node_record(profile, &key.node_hash, &bytes)? {
            DecodedNodeChildren::Leaf => {}
            DecodedNodeChildren::Extension(ch) => {
                stack.push(NodeStoreKey {
                    commitment_profile_id: key.commitment_profile_id,
                    domain_tag: key.domain_tag,
                    node_hash: ch,
                });
            }
            DecodedNodeChildren::Branch(lh, rh) => {
                stack.push(NodeStoreKey {
                    commitment_profile_id: key.commitment_profile_id,
                    domain_tag: key.domain_tag,
                    node_hash: lh,
                });
                stack.push(NodeStoreKey {
                    commitment_profile_id: key.commitment_profile_id,
                    domain_tag: key.domain_tag,
                    node_hash: rh,
                });
            }
        }
    }
    Ok((visited, work))
}

/// Portable in-memory content-addressed commitment store implementing the exact staged publication,
/// pin-epoch write barrier, mark/sweep GC, and crash-recovery protocol (works on native and `wasm32`).
#[derive(Clone, Debug)]
pub struct MemoryCommitmentStore {
    profiles: ProfileRegistryV1,
    domain_registry: DomainRegistryV1,
    op_limits: CommitmentOperationalLimitsDescriptorV1,
    nodes: BTreeMap<NodeStoreKey, Vec<u8>>,
    manifests: BTreeMap<[u8; 32], Vec<u8>>,
    provisional_pins: BTreeMap<u64, Vec<u8>>,
    permanent_pins: BTreeSet<[u8; 32]>,
    head: Option<Vec<u8>>,
    pin_epoch: u64,
    next_attempt_id: u64,
}

impl MemoryCommitmentStore {
    /// Creates an empty portable in-memory commitment store for local `COMMIT-1`.
    pub fn new() -> Self {
        Self {
            profiles: ProfileRegistryV1::local_v1(),
            domain_registry: DomainRegistryV1::local_v1(),
            op_limits: CommitmentOperationalLimitsDescriptorV1::LOCAL_DEFAULT,
            nodes: BTreeMap::new(),
            manifests: BTreeMap::new(),
            provisional_pins: BTreeMap::new(),
            permanent_pins: BTreeSet::new(),
            head: None,
            pin_epoch: 0,
            next_attempt_id: 1,
        }
    }

    /// Returns the current pin epoch.
    pub const fn pin_epoch(&self) -> u64 {
        self.pin_epoch
    }

    /// Returns the number of stored node bodies.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Returns the current head `CommitmentSetIdV1`, if any.
    pub fn head_set_id(&self) -> Result<Option<[u8; 32]>, CommitStateError> {
        match &self.head {
            None => Ok(None),
            Some(bytes) => Ok(Some(
                HeadPointerRecordV1::decode_checksummed(bytes)?.head_set_id,
            )),
        }
    }

    /// Reads and verifies a stored `CommitmentSetV1` by ID.
    pub fn read_manifest(&self, set_id: &[u8; 32]) -> Result<CommitmentSetV1, CommitStateError> {
        let bytes = self
            .manifests
            .get(set_id)
            .ok_or(CommitStateError::InvariantViolation {
                reason: "missing CommitmentSetV1 manifest",
            })?;
        let set = CommitmentSetV1::decode_canonical(bytes)?;
        if &set.id() != set_id {
            return Err(CommitStateError::InvariantViolation {
                reason: "stored CommitmentSetV1 ID mismatch",
            });
        }
        Ok(set)
    }

    /// Explicitly pins an already-stored `CommitmentSetIdV1` (e.g. genesis or archival checkpoint)
    /// through the write barrier (incrementing `pin_epoch`).
    pub fn pin_permanent(&mut self, set_id: [u8; 32]) -> Result<(), CommitStateError> {
        let _ = self.read_manifest(&set_id)?;
        if self.permanent_pins.len() >= self.op_limits.max_pinned_roots as usize
            && !self.permanent_pins.contains(&set_id)
        {
            return Err(CommitStateError::LimitExceeded {
                field: "max_pinned_roots",
            });
        }
        self.permanent_pins.insert(set_id);
        self.pin_epoch = self.pin_epoch.saturating_add(1);
        Ok(())
    }

    /// Unpins a previously pinned set (refusing to unpin the active head) via the write barrier.
    pub fn unpin_permanent(&mut self, set_id: &[u8; 32]) -> Result<bool, CommitStateError> {
        if self.head_set_id()? == Some(*set_id) {
            return Err(CommitStateError::InvariantViolation {
                reason: "cannot unpin active head CommitmentSetV1",
            });
        }
        let removed = self.permanent_pins.remove(set_id);
        if removed {
            self.pin_epoch = self.pin_epoch.saturating_add(1);
        }
        Ok(removed)
    }

    /// Publishes one candidate `CommitmentSetV1` and its node records using the exact staged
    /// publication protocol, optionally crashing at `crash_point`.
    pub fn publish_with_crash_point(
        &mut self,
        expected_prior: Option<[u8; 32]>,
        set: &CommitmentSetV1,
        staged_nodes: &BTreeMap<NodeStoreKey, Vec<u8>>,
        crash_point: CrashPoint,
    ) -> Result<[u8; 32], CommitStateError> {
        if (staged_nodes.len() as u64) > self.op_limits.max_staged_nodes {
            return Err(CommitStateError::LimitExceeded {
                field: "max_staged_nodes",
            });
        }
        let mut total_bytes = 0u64;
        for bytes in staged_nodes.values() {
            if bytes.len() > self.op_limits.max_node_store_record_bytes as usize {
                return Err(CommitStateError::LimitExceeded {
                    field: "max_node_store_record_bytes",
                });
            }
            total_bytes = total_bytes.checked_add(bytes.len() as u64).ok_or(
                CommitStateError::LimitExceeded {
                    field: "max_staged_bytes",
                },
            )?;
            if total_bytes > self.op_limits.max_staged_bytes {
                return Err(CommitStateError::LimitExceeded {
                    field: "max_staged_bytes",
                });
            }
        }
        if self.provisional_pins.len() >= self.op_limits.max_provisional_roots as usize {
            return Err(CommitStateError::LimitExceeded {
                field: "max_provisional_roots",
            });
        }
        let prior_manifest = match expected_prior {
            None => None,
            Some(pid) => Some(self.read_manifest(&pid)?),
        };
        set.validate(
            &self.profiles,
            &self.domain_registry,
            prior_manifest.as_ref(),
        )?;
        let candidate_id = set.id();

        // 1. Stage content-addressed nodes (reusing identical existing nodes, verifying every record).
        for (key, bytes) in staged_nodes {
            let profile = self.profiles.resolve_profile(&key.commitment_profile_id)?;
            let _ = decode_and_verify_node_record(profile, &key.node_hash, bytes)?;
            if let Some(existing) = self.nodes.get(key) {
                if existing != bytes {
                    return Err(CommitStateError::CorruptOrMissingNode {
                        expected_hash: key.node_hash,
                    });
                }
            } else {
                self.nodes.insert(*key, bytes.clone());
            }
        }
        // 2. Verify complete reachable closure of all 3 roots from the store.
        let roots = root_keys_for_set(set);
        let _ = verify_closure_from_lookup(&self.profiles, &self.op_limits, &roots, |k| {
            self.nodes
                .get(k)
                .cloned()
                .ok_or(CommitStateError::CorruptOrMissingNode {
                    expected_hash: k.node_hash,
                })
        })?;
        if crash_point == CrashPoint::AfterNodeStaging {
            return Err(CommitStateError::SimulatedCrash(crash_point));
        }

        // 3. Install checksummed provisional pin and increment pin epoch (write barrier).
        let attempt_id = self.next_attempt_id;
        self.next_attempt_id += 1;
        self.pin_epoch = self.pin_epoch.saturating_add(1);
        let pin = ProvisionalPinRecordV1 {
            attempt_id,
            pin_epoch: self.pin_epoch,
            expected_prior_set_id: expected_prior,
            candidate_set_id: candidate_id,
            root_keys: roots,
        };
        let pin_bytes = pin.encode_checksummed();
        let reread_pin = ProvisionalPinRecordV1::decode_checksummed(&pin_bytes)?;
        if reread_pin != pin {
            return Err(CommitStateError::InvariantViolation {
                reason: "provisional pin reread mismatch",
            });
        }
        self.provisional_pins.insert(attempt_id, pin_bytes);
        if crash_point == CrashPoint::AfterProvisionalPin {
            return Err(CommitStateError::SimulatedCrash(crash_point));
        }

        // 4. Write and reread immutable CommitmentSetV1 manifest.
        let manifest_bytes = set.encode_canonical();
        let reread_set = CommitmentSetV1::decode_canonical(&manifest_bytes)?;
        if reread_set.id() != candidate_id {
            return Err(CommitStateError::InvariantViolation {
                reason: "manifest reread ID mismatch",
            });
        }
        self.manifests.insert(candidate_id, manifest_bytes);
        if crash_point == CrashPoint::AfterManifestWrite {
            return Err(CommitStateError::SimulatedCrash(crash_point));
        }

        // 5. Compare-and-swap recoverable head pointer.
        let current_head = self.head_set_id()?;
        if current_head != expected_prior {
            return Err(CommitStateError::HeadCasConflict {
                expected: expected_prior,
                actual: current_head,
            });
        }
        let next_head = HeadPointerRecordV1 {
            head_set_id: candidate_id,
            pin_epoch: self.pin_epoch,
        };
        self.head = Some(next_head.encode_checksummed());
        if crash_point == CrashPoint::AfterHeadCas {
            return Err(CommitStateError::SimulatedCrash(crash_point));
        }

        // 6. Reread head, promote pin, and release provisional pin.
        if self.head_set_id()? != Some(candidate_id) {
            return Err(CommitStateError::InvariantViolation {
                reason: "head reread mismatch after CAS",
            });
        }
        self.permanent_pins.insert(candidate_id);
        self.pin_epoch = self.pin_epoch.saturating_add(1);
        if crash_point == CrashPoint::AfterPinPromotion {
            return Err(CommitStateError::SimulatedCrash(crash_point));
        }
        self.provisional_pins.remove(&attempt_id);
        Ok(candidate_id)
    }

    /// Corrupts one stored node's bytes in memory for negative-control testing.
    pub fn corrupt_node_for_test(&mut self, key: &NodeStoreKey) -> bool {
        if let Some(bytes) = self.nodes.get_mut(key)
            && !bytes.is_empty()
        {
            bytes[0] ^= 0xFF;
            return true;
        }
        false
    }

    /// Runs a bounded GC mark pass recording `pin_epoch` and tracing all permanently and
    /// provisionally pinned roots.
    pub fn begin_gc_mark(&self) -> Result<GcMarkSnapshot, CommitStateError> {
        let epoch = self.pin_epoch;
        let mut pin_roots = Vec::new();
        for set_id in &self.permanent_pins {
            let set = self.read_manifest(set_id)?;
            pin_roots.extend(root_keys_for_set(&set));
        }
        if let Some(hid) = self.head_set_id()? {
            let set = self.read_manifest(&hid)?;
            pin_roots.extend(root_keys_for_set(&set));
        }
        for raw_pin in self.provisional_pins.values() {
            let pin = ProvisionalPinRecordV1::decode_checksummed(raw_pin)?;
            pin_roots.extend(pin.root_keys);
        }
        pin_roots.sort();
        pin_roots.dedup();

        let (reachable, work_units) =
            verify_closure_from_lookup(&self.profiles, &self.op_limits, &pin_roots, |k| {
                self.nodes
                    .get(k)
                    .cloned()
                    .ok_or(CommitStateError::CorruptOrMissingNode {
                        expected_hash: k.node_hash,
                    })
            })?;
        Ok(GcMarkSnapshot {
            pin_epoch: epoch,
            reachable,
            work_units,
        })
    }

    /// Completes a bounded GC sweep only if `pin_epoch` is unchanged since `mark`.
    pub fn finish_gc_sweep(&mut self, mark: &GcMarkSnapshot) -> GcSweepOutcome {
        if self.pin_epoch != mark.pin_epoch {
            return GcSweepOutcome::AbortedEpochChanged {
                mark_epoch: mark.pin_epoch,
                current_epoch: self.pin_epoch,
            };
        }
        let orphans: Vec<NodeStoreKey> = self
            .nodes
            .keys()
            .filter(|k| !mark.reachable.contains(k))
            .take(self.op_limits.max_gc_sweep_nodes as usize)
            .copied()
            .collect();
        let reclaimed = orphans.len();
        for k in orphans {
            self.nodes.remove(&k);
        }
        GcSweepOutcome::Completed {
            reclaimed_nodes: reclaimed,
            remaining_nodes: self.nodes.len(),
        }
    }

    /// Recovers store state after a crash: verifies head manifest & complete reachable closure,
    /// promotes head-referenced provisional pins, and releases unreferenced provisional pins.
    pub fn recover(&mut self) -> Result<StoreRecoveryReport, CommitStateError> {
        let head_id = self.head_set_id()?;
        let mut verified_nodes = 0usize;
        if let Some(hid) = head_id {
            let set = self.read_manifest(&hid)?;
            set.validate(&self.profiles, &self.domain_registry, None)?;
            let roots = root_keys_for_set(&set);
            let (visited, _) =
                verify_closure_from_lookup(&self.profiles, &self.op_limits, &roots, |k| {
                    self.nodes
                        .get(k)
                        .cloned()
                        .ok_or(CommitStateError::CorruptOrMissingNode {
                            expected_hash: k.node_hash,
                        })
                })?;
            verified_nodes = visited.len();
        }
        let mut promoted = 0usize;
        let mut released = 0usize;
        let attempts: Vec<u64> = self.provisional_pins.keys().copied().collect();
        for attempt_id in attempts {
            let raw = self.provisional_pins[&attempt_id].clone();
            let pin = ProvisionalPinRecordV1::decode_checksummed(&raw)?;
            if Some(pin.candidate_set_id) == head_id {
                if self.permanent_pins.insert(pin.candidate_set_id) {
                    promoted += 1;
                }
                self.provisional_pins.remove(&attempt_id);
                self.pin_epoch = self.pin_epoch.saturating_add(1);
            } else if !self.permanent_pins.contains(&pin.candidate_set_id) {
                self.provisional_pins.remove(&attempt_id);
                released += 1;
                self.pin_epoch = self.pin_epoch.saturating_add(1);
            }
        }
        Ok(StoreRecoveryReport {
            recovered_head: head_id,
            promoted_provisional_pins: promoted,
            released_provisional_pins: released,
            verified_reachable_nodes: verified_nodes,
        })
    }
}

impl Default for MemoryCommitmentStore {
    fn default() -> Self {
        Self::new()
    }
}

// =========================================================================
// DURABLE FILESYSTEM COMMITMENT STORE & AUTHENTICATED LEDGER (NON-WASM)
// =========================================================================

#[cfg(not(target_arch = "wasm32"))]
mod durable_fs {
    use super::*;
    use std::fs::{self, File, OpenOptions};
    use std::io::Write;
    use std::path::{Path, PathBuf};

    #[cfg(windows)]
    use std::os::windows::fs::OpenOptionsExt;

    fn hex_encode(bytes: &[u8]) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut s = String::with_capacity(bytes.len() * 2);
        for &b in bytes {
            s.push(HEX[(b >> 4) as usize] as char);
            s.push(HEX[(b & 0x0f) as usize] as char);
        }
        s
    }

    fn map_io(ctx: &str, err: std::io::Error) -> CommitStateError {
        CommitStateError::IoFailure {
            detail: format!("{ctx}: {err}"),
        }
    }

    fn sync_dir_best_effort(dir: &Path) -> Result<(), CommitStateError> {
        #[cfg(unix)]
        {
            let d = File::open(dir).map_err(|e| map_io("open dir for sync", e))?;
            d.sync_all().map_err(|e| map_io("sync dir", e))?;
        }
        #[cfg(not(unix))]
        {
            let _ = dir;
        }
        Ok(())
    }

    fn write_atomic_synced(path: &Path, data: &[u8]) -> Result<(), CommitStateError> {
        let parent = path.parent().ok_or_else(|| CommitStateError::IoFailure {
            detail: "atomic write path lacks parent".to_string(),
        })?;
        let fname = path.file_name().and_then(|n| n.to_str()).ok_or_else(|| {
            CommitStateError::IoFailure {
                detail: "atomic write path lacks utf8 filename".to_string(),
            }
        })?;
        let tmp_path = parent.join(format!(".{fname}.tmp"));
        {
            let mut file = OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&tmp_path)
                .map_err(|e| map_io("create temp file", e))?;
            file.write_all(data)
                .map_err(|e| map_io("write temp file", e))?;
            file.sync_all().map_err(|e| map_io("sync temp file", e))?;
        }
        fs::rename(&tmp_path, path).map_err(|e| map_io("rename temp file", e))?;
        sync_dir_best_effort(parent)?;
        Ok(())
    }

    fn acquire_exclusive_lock(lock_path: &Path) -> Result<File, CommitStateError> {
        #[cfg(windows)]
        {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .share_mode(0)
                .open(lock_path)
                .map_err(|e| map_io("acquire windows exclusive store lock", e))?;
            Ok(file)
        }
        #[cfg(unix)]
        {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(lock_path)
                .map_err(|e| map_io("open unix store lock", e))?;
            use std::os::unix::io::AsRawFd;
            unsafe extern "C" {
                fn flock(fd: i32, operation: i32) -> i32;
            }
            const LOCK_EX: i32 = 2;
            const LOCK_NB: i32 = 4;
            let rc = unsafe { flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) };
            if rc != 0 {
                return Err(CommitStateError::IoFailure {
                    detail: "store lock already held by another writer".to_string(),
                });
            }
            Ok(file)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(lock_path)
                .map_err(|e| map_io("open store lock", e))?;
            Ok(file)
        }
    }

    /// Crash-safe filesystem commitment store implementing staged node writes, checksummed
    /// provisional pins, immutable manifests, atomic head CAS, exclusive writer locking, and
    /// write-barrier mark/sweep GC.
    pub struct DurableCommitmentStore {
        root_dir: PathBuf,
        _lock_file: File,
        profiles: ProfileRegistryV1,
        domain_registry: DomainRegistryV1,
        op_limits: CommitmentOperationalLimitsDescriptorV1,
        permanent_pins: BTreeSet<[u8; 32]>,
        pin_epoch: u64,
        next_attempt_id: u64,
    }

    impl DurableCommitmentStore {
        /// Opens or creates a durable commitment store at `dir`, acquires its exclusive lock, and
        /// executes crash recovery before returning.
        pub fn open_and_recover<P: AsRef<Path>>(
            dir: P,
        ) -> Result<(Self, StoreRecoveryReport), CommitStateError> {
            let root_dir = dir.as_ref().to_path_buf();
            fs::create_dir_all(root_dir.join("nodes"))
                .map_err(|e| map_io("create nodes dir", e))?;
            fs::create_dir_all(root_dir.join("manifests"))
                .map_err(|e| map_io("create manifests dir", e))?;
            fs::create_dir_all(root_dir.join("pins")).map_err(|e| map_io("create pins dir", e))?;
            let lock_file = acquire_exclusive_lock(&root_dir.join("store.lock"))?;

            let mut store = Self {
                root_dir,
                _lock_file: lock_file,
                profiles: ProfileRegistryV1::local_v1(),
                domain_registry: DomainRegistryV1::local_v1(),
                op_limits: CommitmentOperationalLimitsDescriptorV1::LOCAL_DEFAULT,
                permanent_pins: BTreeSet::new(),
                pin_epoch: 0,
                next_attempt_id: 1,
            };
            let report = store.recover_internal()?;
            Ok((store, report))
        }

        fn node_path(&self, key: &NodeStoreKey) -> PathBuf {
            self.root_dir.join("nodes").join(format!(
                "{}_{:02x}_{}.node",
                hex_encode(&key.commitment_profile_id),
                key.domain_tag.code(),
                hex_encode(&key.node_hash)
            ))
        }

        fn manifest_path(&self, set_id: &[u8; 32]) -> PathBuf {
            self.root_dir
                .join("manifests")
                .join(format!("{}.cset", hex_encode(set_id)))
        }

        fn provisional_pin_path(&self, attempt_id: u64) -> PathBuf {
            self.root_dir
                .join("pins")
                .join(format!("prov_{attempt_id:020}.pin"))
        }

        fn permanent_pin_path(&self, set_id: &[u8; 32]) -> PathBuf {
            self.root_dir
                .join("pins")
                .join(format!("perm_{}.pin", hex_encode(set_id)))
        }

        fn head_path(&self) -> PathBuf {
            self.root_dir.join("head.ptr")
        }

        /// Returns the current recovered or published head `CommitmentSetIdV1`.
        pub fn head_set_id(&self) -> Result<Option<[u8; 32]>, CommitStateError> {
            let hp = self.head_path();
            if !hp.exists() {
                return Ok(None);
            }
            let bytes = fs::read(&hp).map_err(|e| map_io("read head.ptr", e))?;
            Ok(Some(
                HeadPointerRecordV1::decode_checksummed(&bytes)?.head_set_id,
            ))
        }

        /// Returns the current pin epoch.
        pub const fn pin_epoch(&self) -> u64 {
            self.pin_epoch
        }

        /// Reads and verifies a persisted `CommitmentSetV1` manifest by ID.
        pub fn read_manifest(
            &self,
            set_id: &[u8; 32],
        ) -> Result<CommitmentSetV1, CommitStateError> {
            let bytes = fs::read(self.manifest_path(set_id)).map_err(|_| {
                CommitStateError::InvariantViolation {
                    reason: "missing durable CommitmentSetV1 manifest",
                }
            })?;
            let set = CommitmentSetV1::decode_canonical(&bytes)?;
            if &set.id() != set_id {
                return Err(CommitStateError::InvariantViolation {
                    reason: "durable CommitmentSetV1 ID mismatch",
                });
            }
            Ok(set)
        }

        /// Reads and canonically verifies one persisted node record by `NodeStoreKey`.
        pub fn read_verified_node(&self, key: &NodeStoreKey) -> Result<Vec<u8>, CommitStateError> {
            let profile = self.profiles.resolve_profile(&key.commitment_profile_id)?;
            let bytes = fs::read(self.node_path(key)).map_err(|_| {
                CommitStateError::CorruptOrMissingNode {
                    expected_hash: key.node_hash,
                }
            })?;
            let _ = decode_and_verify_node_record(profile, &key.node_hash, &bytes)?;
            Ok(bytes)
        }

        /// Returns the filesystem path of one stored node record (for external fault/corruption tests).
        pub fn node_file_path(&self, key: &NodeStoreKey) -> PathBuf {
            self.node_path(key)
        }

        /// Counts persisted `.node` files in the node directory.
        pub fn persisted_node_count(&self) -> Result<usize, CommitStateError> {
            let mut count = 0usize;
            for entry in fs::read_dir(self.root_dir.join("nodes"))
                .map_err(|e| map_io("read nodes dir", e))?
            {
                let entry = entry.map_err(|e| map_io("read nodes entry", e))?;
                if entry.path().extension().and_then(|s| s.to_str()) == Some("node") {
                    count += 1;
                }
            }
            Ok(count)
        }

        fn write_permanent_pin(&mut self, set_id: [u8; 32]) -> Result<(), CommitStateError> {
            let mut body = Vec::with_capacity(64);
            body.extend_from_slice(&set_id);
            let sum: [u8; 32] = Sha256::digest(&body).into();
            body.extend_from_slice(&sum);
            write_atomic_synced(&self.permanent_pin_path(&set_id), &body)?;
            self.permanent_pins.insert(set_id);
            self.pin_epoch = self.pin_epoch.saturating_add(1);
            Ok(())
        }

        /// Publishes a candidate `CommitmentSetV1` and its nodes durably with optional crash injection.
        pub fn publish_with_crash_point(
            &mut self,
            expected_prior: Option<[u8; 32]>,
            set: &CommitmentSetV1,
            staged_nodes: &BTreeMap<NodeStoreKey, Vec<u8>>,
            crash_point: CrashPoint,
        ) -> Result<[u8; 32], CommitStateError> {
            if (staged_nodes.len() as u64) > self.op_limits.max_staged_nodes {
                return Err(CommitStateError::LimitExceeded {
                    field: "max_staged_nodes",
                });
            }
            let prior_manifest = match expected_prior {
                None => None,
                Some(pid) => Some(self.read_manifest(&pid)?),
            };
            set.validate(
                &self.profiles,
                &self.domain_registry,
                prior_manifest.as_ref(),
            )?;
            let candidate_id = set.id();

            // 1. Stage content-addressed nodes durably and verify any reused existing node.
            for (key, bytes) in staged_nodes {
                let profile = self.profiles.resolve_profile(&key.commitment_profile_id)?;
                let _ = decode_and_verify_node_record(profile, &key.node_hash, bytes)?;
                let np = self.node_path(key);
                if np.exists() {
                    let existing = fs::read(&np).map_err(|e| map_io("read existing node", e))?;
                    if existing != *bytes {
                        return Err(CommitStateError::CorruptOrMissingNode {
                            expected_hash: key.node_hash,
                        });
                    }
                } else {
                    write_atomic_synced(&np, bytes)?;
                }
            }
            // 2. Verify complete reachable closure by reading back from disk.
            let roots = root_keys_for_set(set);
            let _ = verify_closure_from_lookup(&self.profiles, &self.op_limits, &roots, |k| {
                fs::read(self.node_path(k)).map_err(|_| CommitStateError::CorruptOrMissingNode {
                    expected_hash: k.node_hash,
                })
            })?;
            if crash_point == CrashPoint::AfterNodeStaging {
                return Err(CommitStateError::SimulatedCrash(crash_point));
            }

            // 3. Durably write & reread checksummed provisional pin and increment pin epoch.
            let attempt_id = self.next_attempt_id;
            self.next_attempt_id += 1;
            self.pin_epoch = self.pin_epoch.saturating_add(1);
            let pin = ProvisionalPinRecordV1 {
                attempt_id,
                pin_epoch: self.pin_epoch,
                expected_prior_set_id: expected_prior,
                candidate_set_id: candidate_id,
                root_keys: roots,
            };
            let pin_path = self.provisional_pin_path(attempt_id);
            write_atomic_synced(&pin_path, &pin.encode_checksummed())?;
            let reread_pin = ProvisionalPinRecordV1::decode_checksummed(
                &fs::read(&pin_path).map_err(|e| map_io("reread provisional pin", e))?,
            )?;
            if reread_pin != pin {
                return Err(CommitStateError::InvariantViolation {
                    reason: "durable provisional pin reread mismatch",
                });
            }
            if crash_point == CrashPoint::AfterProvisionalPin {
                return Err(CommitStateError::SimulatedCrash(crash_point));
            }

            // 4. Durably write & reread immutable manifest.
            let mp = self.manifest_path(&candidate_id);
            write_atomic_synced(&mp, &set.encode_canonical())?;
            let reread_set = CommitmentSetV1::decode_canonical(
                &fs::read(&mp).map_err(|e| map_io("reread manifest", e))?,
            )?;
            if reread_set.id() != candidate_id {
                return Err(CommitStateError::InvariantViolation {
                    reason: "durable manifest reread ID mismatch",
                });
            }
            if crash_point == CrashPoint::AfterManifestWrite {
                return Err(CommitStateError::SimulatedCrash(crash_point));
            }

            // 5. Compare-and-swap checksummed head pointer.
            let current_head = self.head_set_id()?;
            if current_head != expected_prior {
                return Err(CommitStateError::HeadCasConflict {
                    expected: expected_prior,
                    actual: current_head,
                });
            }
            let head_rec = HeadPointerRecordV1 {
                head_set_id: candidate_id,
                pin_epoch: self.pin_epoch,
            };
            write_atomic_synced(&self.head_path(), &head_rec.encode_checksummed())?;
            if crash_point == CrashPoint::AfterHeadCas {
                return Err(CommitStateError::SimulatedCrash(crash_point));
            }

            // 6. Reread head, promote pin, and release provisional pin.
            if self.head_set_id()? != Some(candidate_id) {
                return Err(CommitStateError::InvariantViolation {
                    reason: "durable head reread mismatch after CAS",
                });
            }
            self.write_permanent_pin(candidate_id)?;
            if crash_point == CrashPoint::AfterPinPromotion {
                return Err(CommitStateError::SimulatedCrash(crash_point));
            }
            let _ = fs::remove_file(&pin_path);
            sync_dir_best_effort(&self.root_dir.join("pins"))?;
            Ok(candidate_id)
        }

        fn recover_internal(&mut self) -> Result<StoreRecoveryReport, CommitStateError> {
            let head_id = self.head_set_id()?;
            let mut verified_nodes = 0usize;
            if let Some(hid) = head_id {
                let set = self.read_manifest(&hid)?;
                set.validate(&self.profiles, &self.domain_registry, None)?;
                let roots = root_keys_for_set(&set);
                let (visited, _) =
                    verify_closure_from_lookup(&self.profiles, &self.op_limits, &roots, |k| {
                        fs::read(self.node_path(k)).map_err(|_| {
                            CommitStateError::CorruptOrMissingNode {
                                expected_hash: k.node_hash,
                            }
                        })
                    })?;
                verified_nodes = visited.len();
            }

            let mut max_attempt = 0u64;
            let mut max_epoch = 0u64;
            if let Some(bytes) = self
                .head_path()
                .exists()
                .then(|| fs::read(self.head_path()))
                .transpose()
                .map_err(|e| map_io("read head epoch", e))?
            {
                let hr = HeadPointerRecordV1::decode_checksummed(&bytes)?;
                max_epoch = max_epoch.max(hr.pin_epoch);
            }

            let pins_dir = self.root_dir.join("pins");
            let mut prov_files = Vec::new();
            for entry in fs::read_dir(&pins_dir).map_err(|e| map_io("read pins dir", e))? {
                let entry = entry.map_err(|e| map_io("read pins entry", e))?;
                let path = entry.path();
                let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                    continue;
                };
                if name.starts_with("perm_") && name.ends_with(".pin") {
                    let data = fs::read(&path).map_err(|e| map_io("read perm pin", e))?;
                    if data.len() != 64 {
                        return Err(CommitStateError::MalformedEncoding {
                            reason: "corrupt permanent pin file",
                        });
                    }
                    let expected_sum: [u8; 32] = Sha256::digest(&data[0..32]).into();
                    if data[32..64] != expected_sum {
                        return Err(CommitStateError::MalformedEncoding {
                            reason: "permanent pin checksum mismatch",
                        });
                    }
                    let mut id = [0u8; 32];
                    id.copy_from_slice(&data[0..32]);
                    self.permanent_pins.insert(id);
                } else if name.starts_with("prov_") && name.ends_with(".pin") {
                    prov_files.push(path);
                }
            }

            let mut promoted = 0usize;
            let mut released = 0usize;
            for ppath in prov_files {
                let data = fs::read(&ppath).map_err(|e| map_io("read prov pin", e))?;
                let pin = ProvisionalPinRecordV1::decode_checksummed(&data)?;
                max_attempt = max_attempt.max(pin.attempt_id);
                max_epoch = max_epoch.max(pin.pin_epoch);
                if Some(pin.candidate_set_id) == head_id {
                    if !self.permanent_pins.contains(&pin.candidate_set_id) {
                        self.write_permanent_pin(pin.candidate_set_id)?;
                        promoted += 1;
                    }
                    fs::remove_file(&ppath).map_err(|e| map_io("remove promoted prov pin", e))?;
                } else if !self.permanent_pins.contains(&pin.candidate_set_id) {
                    fs::remove_file(&ppath).map_err(|e| map_io("release orphan prov pin", e))?;
                    released += 1;
                }
            }
            self.pin_epoch = self.pin_epoch.max(max_epoch).saturating_add(1);
            self.next_attempt_id = max_attempt.saturating_add(1);
            sync_dir_best_effort(&pins_dir)?;
            Ok(StoreRecoveryReport {
                recovered_head: head_id,
                promoted_provisional_pins: promoted,
                released_provisional_pins: released,
                verified_reachable_nodes: verified_nodes,
            })
        }

        /// Runs a bounded mark pass tracing all permanent pins, active head, and provisional pins.
        pub fn begin_gc_mark(&self) -> Result<GcMarkSnapshot, CommitStateError> {
            let epoch = self.pin_epoch;
            let mut roots = Vec::new();
            for set_id in &self.permanent_pins {
                let set = self.read_manifest(set_id)?;
                roots.extend(root_keys_for_set(&set));
            }
            if let Some(hid) = self.head_set_id()? {
                let set = self.read_manifest(&hid)?;
                roots.extend(root_keys_for_set(&set));
            }
            roots.sort();
            roots.dedup();
            let (reachable, work_units) =
                verify_closure_from_lookup(&self.profiles, &self.op_limits, &roots, |k| {
                    fs::read(self.node_path(k)).map_err(|_| {
                        CommitStateError::CorruptOrMissingNode {
                            expected_hash: k.node_hash,
                        }
                    })
                })?;
            Ok(GcMarkSnapshot {
                pin_epoch: epoch,
                reachable,
                work_units,
            })
        }

        /// Completes a bounded sweep on disk only if `pin_epoch` is unchanged since `mark`.
        pub fn finish_gc_sweep(
            &mut self,
            mark: &GcMarkSnapshot,
        ) -> Result<GcSweepOutcome, CommitStateError> {
            if self.pin_epoch != mark.pin_epoch {
                return Ok(GcSweepOutcome::AbortedEpochChanged {
                    mark_epoch: mark.pin_epoch,
                    current_epoch: self.pin_epoch,
                });
            }
            let reachable_names: HashSet<String> = mark
                .reachable
                .iter()
                .map(|k| {
                    format!(
                        "{}_{:02x}_{}.node",
                        hex_encode(&k.commitment_profile_id),
                        k.domain_tag.code(),
                        hex_encode(&k.node_hash)
                    )
                })
                .collect();
            let mut reclaimed = 0usize;
            let mut remaining = 0usize;
            let nodes_dir = self.root_dir.join("nodes");
            for entry in fs::read_dir(&nodes_dir).map_err(|e| map_io("read nodes dir", e))? {
                let entry = entry.map_err(|e| map_io("read nodes entry", e))?;
                let path = entry.path();
                let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                    continue;
                };
                if !name.ends_with(".node") {
                    continue;
                }
                if !reachable_names.contains(name)
                    && (reclaimed as u64) < self.op_limits.max_gc_sweep_nodes
                {
                    fs::remove_file(&path).map_err(|e| map_io("sweep orphan node", e))?;
                    reclaimed += 1;
                } else {
                    remaining += 1;
                }
            }
            sync_dir_best_effort(&nodes_dir)?;
            Ok(GcSweepOutcome::Completed {
                reclaimed_nodes: reclaimed,
                remaining_nodes: remaining,
            })
        }
    }

    /// Crash-safe authenticated local ledger pairing `DurableOccupancyLedger` with
    /// `DurableCommitmentStore` and `LocalAuthenticatedSidecar`.
    pub struct DurableAuthenticatedLedger {
        ledger: DurableOccupancyLedger,
        store: DurableCommitmentStore,
        sidecar: LocalAuthenticatedSidecar,
        initial_world: World,
        storage_profile: LocalStorageProfile,
    }

    impl DurableAuthenticatedLedger {
        /// Creates a new durable ledger journal and companion commitment store at genesis.
        pub fn create_new<P: AsRef<Path>, Q: AsRef<Path>>(
            journal_path: P,
            store_dir: Q,
            profile: &LocalStorageProfile,
            scope_digest: [u8; 32],
            initial_world: &World,
        ) -> Result<(Self, HistoryCreateReceipt, [u8; 32]), CommitStateError> {
            let (ledger, create_receipt) =
                DurableOccupancyLedger::create_new(journal_path, profile, initial_world)?;
            let (mut store, _) = DurableCommitmentStore::open_and_recover(store_dir)?;
            let sidecar = LocalAuthenticatedSidecar::from_genesis(
                initial_world,
                ledger.history(),
                ledger.episodes(),
                profile,
                scope_digest,
            )?;
            let nodes = sidecar.collect_all_node_records();
            let genesis_set_id = store.publish_with_crash_point(
                None,
                sidecar.current_set(),
                &nodes,
                CrashPoint::None,
            )?;
            Ok((
                Self {
                    ledger,
                    store,
                    sidecar,
                    initial_world: initial_world.clone(),
                    storage_profile: *profile,
                },
                create_receipt,
                genesis_set_id,
            ))
        }

        /// Opens and recovers a durable authenticated ledger, verifying that the recovered
        /// `CommitmentSetV1` head matches the verified ledger state (or deterministically catching
        /// up a single un-CAS'd committed ledger head without ever exposing a mixed tuple).
        pub fn open_and_recover<P: AsRef<Path>, Q: AsRef<Path>>(
            journal_path: P,
            store_dir: Q,
            profile: &LocalStorageProfile,
            scope_digest: [u8; 32],
            anchor: AnchorPolicy,
            initial_world: &World,
        ) -> Result<(Self, ScanReport, StoreRecoveryReport), CommitStateError> {
            let (ledger, scan_report) = match DurableOccupancyLedger::open_existing(
                journal_path,
                profile,
                anchor,
                initial_world,
            )? {
                OpenDurableOccupancyLedger::Ready {
                    ledger,
                    carrier_report,
                } => (*ledger, carrier_report),
                OpenDurableOccupancyLedger::NeedsRecovery { .. } => {
                    return Err(CommitStateError::InvariantViolation {
                        reason: "underlying ledger journal requires explicit tail recovery",
                    });
                }
            };
            let (mut store, store_report) = DurableCommitmentStore::open_and_recover(store_dir)?;
            let Some(head_id) = store_report.recovered_head else {
                return Err(CommitStateError::InvariantViolation {
                    reason: "commitment store has no committed genesis head",
                });
            };
            let head_set = store.read_manifest(&head_id)?;
            let expected_head = ledger.history().head().digest();

            // Rebuild exact sidecar from genesis and replay every committed ledger receipt.
            let mut sidecar = LocalAuthenticatedSidecar::from_genesis(
                initial_world,
                &History::new(initial_world),
                &OccupancyEpisodeLedger::from_history(initial_world, &History::new(initial_world))?,
                profile,
                scope_digest,
            )?;
            let mut replay_world = initial_world.clone();
            let mut replay_history = History::new(initial_world);
            let mut replay_episodes =
                OccupancyEpisodeLedger::from_history(initial_world, &replay_history)?;

            for receipt in ledger.history().receipts() {
                let mut tx = replay_history.begin_tick(receipt.tick());
                for &edit in receipt.edits() {
                    tx.apply_edit(&replay_history, &mut replay_world, edit);
                }
                let committed = replay_history
                    .commit_tick(tx, &replay_world)
                    .expect("verified journal receipt replays cleanly")
                    .clone();
                replay_episodes.apply_receipt(&committed)?;
                let next_set = sidecar.advance_for_receipt(
                    initial_world,
                    &replay_world,
                    &replay_history,
                    &replay_episodes,
                    &committed,
                )?;
                // If the crash interrupted publication before head CAS, publish the missing step now.
                if head_set.ledger_head_id != expected_head
                    && store.head_set_id()? != Some(next_set.id())
                {
                    let prior = store.head_set_id()?;
                    let is_after_head = prior == Some(head_id)
                        || matches!(
                            next_set.predecessor,
                            CommitmentSetPredecessorV1::OrdinaryCommitmentSuccessorV1 {
                                prior_commitment_set_id
                            } if Some(prior_commitment_set_id) == prior
                        );
                    if is_after_head {
                        let nodes = sidecar.collect_all_node_records();
                        store.publish_with_crash_point(
                            prior,
                            &next_set,
                            &nodes,
                            CrashPoint::None,
                        )?;
                    }
                }
            }

            let final_head_id =
                store
                    .head_set_id()?
                    .ok_or(CommitStateError::InvariantViolation {
                        reason: "missing head after recovery",
                    })?;
            let final_set = store.read_manifest(&final_head_id)?;
            if final_set != *sidecar.current_set() {
                return Err(CommitStateError::InvariantViolation {
                    reason: "recovered CommitmentSetV1 disagrees with verified ledger replay",
                });
            }
            let _ = verify_independent_projection(
                initial_world,
                ledger.history(),
                &HashSet::new(),
                &final_set,
                &ProfileRegistryV1::local_v1(),
                &DomainRegistryV1::local_v1(),
                profile.to_history_replay_limits().map_err(|_| {
                    CommitStateError::LimitExceeded {
                        field: "history_replay_limits",
                    }
                })?,
            )?;
            Ok((
                Self {
                    ledger,
                    store,
                    sidecar,
                    initial_world: initial_world.clone(),
                    storage_profile: *profile,
                },
                scan_report,
                store_report,
            ))
        }

        /// Commits one tick to the durable ledger and publishes the matching `CommitmentSetV1`,
        /// optionally crashing at `crash_point`.
        pub fn append_tick_with_crash_point(
            &mut self,
            tick: u64,
            edits: &[Edit],
            crash_point: CrashPoint,
        ) -> Result<Option<(HistoryAppendReceipt, [u8; 32])>, CommitStateError> {
            let Some(append_receipt) = self.ledger.append_tick(tick, edits)? else {
                return Ok(None);
            };
            let committed = self
                .ledger
                .history()
                .receipts()
                .last()
                .expect("committed tick appended one receipt");
            let prior_id = Some(self.sidecar.current_set().id());
            let next_set = self.sidecar.advance_for_receipt(
                &self.initial_world,
                self.ledger.world(),
                self.ledger.history(),
                self.ledger.episodes(),
                committed,
            )?;
            let nodes = self.sidecar.collect_all_node_records();
            let set_id =
                self.store
                    .publish_with_crash_point(prior_id, &next_set, &nodes, crash_point)?;
            Ok(Some((append_receipt, set_id)))
        }

        /// Commits one tick and publishes the complete `CommitmentSetV1` without crash injection.
        pub fn append_tick(
            &mut self,
            tick: u64,
            edits: &[Edit],
        ) -> Result<Option<(HistoryAppendReceipt, [u8; 32])>, CommitStateError> {
            self.append_tick_with_crash_point(tick, edits, CrashPoint::None)
        }

        /// Borrows the current `LocalAuthenticatedSidecar`.
        pub const fn sidecar(&self) -> &LocalAuthenticatedSidecar {
            &self.sidecar
        }

        /// Borrows the underlying `DurableCommitmentStore`.
        pub const fn store(&self) -> &DurableCommitmentStore {
            &self.store
        }

        /// Borrows the underlying `DurableOccupancyLedger`.
        pub const fn ledger(&self) -> &DurableOccupancyLedger {
            &self.ledger
        }

        /// Returns the active `LocalStorageProfile`.
        pub const fn storage_profile(&self) -> &LocalStorageProfile {
            &self.storage_profile
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use durable_fs::{DurableAuthenticatedLedger, DurableCommitmentStore};

#[cfg(test)]
mod tests {
    use super::*;
    use bitblock_core::program::{Edit, Operation};

    #[test]
    fn descriptors_and_registries_round_trip_and_pin_tags() {
        let pp_w = ProofPolicyDescriptorV1::WORLD_DEFAULT;
        let pp_a = ProofPolicyDescriptorV1::ACTIVE_TOKEN_DEFAULT;
        let pp_e = ProofPolicyDescriptorV1::EPISODE_DEFAULT;
        for pp in [pp_w, pp_a, pp_e] {
            let enc = pp.encode_canonical();
            assert_eq!(ProofPolicyDescriptorV1::decode_canonical(&enc).unwrap(), pp);
        }
        // Widening any local cap must fail closed.
        let mut wider = pp_w;
        wider.max_proof_bytes += 1;
        assert!(matches!(
            pp_w.tighten(wider),
            Err(CommitStateError::LocalCapWidensPolicy {
                field: "max_proof_bytes"
            })
        ));
        let mut tighter = pp_w;
        tighter.max_proof_bytes -= 1;
        assert_eq!(pp_w.tighten(tighter).unwrap(), tighter);

        let limits = CommitmentOperationalLimitsDescriptorV1::LOCAL_DEFAULT;
        let lbytes = limits.encode_canonical();
        assert_eq!(
            CommitmentOperationalLimitsDescriptorV1::decode_canonical(&lbytes).unwrap(),
            limits
        );

        let prof_w = CommitmentProfileDescriptorV1::world_v1();
        let prof_a = CommitmentProfileDescriptorV1::active_token_v1();
        let prof_e = CommitmentProfileDescriptorV1::episode_v1();
        for p in [prof_w, prof_a, prof_e] {
            let enc = p.encode_canonical();
            assert_eq!(
                CommitmentProfileDescriptorV1::decode_canonical(&enc).unwrap(),
                p
            );
        }
        assert_ne!(prof_w.id(), prof_a.id());
        assert_ne!(prof_a.id(), prof_e.id());

        let reg = DomainRegistryV1::local_v1();
        let reg_bytes = reg.encode_canonical();
        assert_eq!(DomainRegistryV1::decode_canonical(&reg_bytes).unwrap(), reg);
        assert!(reg.is_active(DomainTagV1::WorldOccupancy));
        assert!(reg.is_active(DomainTagV1::ActiveToken));
        assert!(reg.is_active(DomainTagV1::Episode));
        assert!(!reg.is_active(DomainTagV1::Authorization));
        assert!(!reg.is_active(DomainTagV1::Nonce));
        assert!(!reg.is_active(DomainTagV1::AssetState));
        reg.validate_against(&ProfileRegistryV1::local_v1())
            .unwrap();
    }

    #[test]
    fn incremental_matches_full_rebuild_and_world_root_restores_on_delete_replace() {
        let initial_world = World::new();
        let mut world = initial_world.clone();
        let mut history = History::new(&initial_world);
        let mut episodes = OccupancyEpisodeLedger::from_history(&initial_world, &history).unwrap();
        let profile = LocalStorageProfile::DEFAULT;
        let scope_digest = crate::transition_spec::BaseTransitionSpecD1::canonical_base_only()
            .tag()
            .unwrap()
            .digest();
        let mut sidecar = LocalAuthenticatedSidecar::from_genesis(
            &initial_world,
            &history,
            &episodes,
            &profile,
            scope_digest,
        )
        .unwrap();
        let w0 = sidecar.world_root();
        let a0 = sidecar.active_token_root();
        let e0 = sidecar.episode_root();

        let c = Coord::new(7, -3, 19);
        // Tick 1: place c
        let mut tx1 = history.begin_tick(1);
        tx1.apply_edit(&history, &mut world, Edit::new(Operation::On, c));
        let r1 = history.commit_tick(tx1, &world).unwrap().clone();
        episodes.apply_receipt(&r1).unwrap();
        let set1 = sidecar
            .advance_for_receipt(&initial_world, &world, &history, &episodes, &r1)
            .unwrap();
        let w1 = sidecar.world_root();
        let a1 = sidecar.active_token_root();
        let e1 = sidecar.episode_root();
        assert_ne!(w0.root, w1.root);
        assert_ne!(a0.root, a1.root);
        assert_ne!(e0.root, e1.root);

        // Tick 2: delete c -> world and active_token restore to W0/A0, episode keeps closed record!
        let mut tx2 = history.begin_tick(2);
        tx2.apply_edit(&history, &mut world, Edit::new(Operation::Off, c));
        let r2 = history.commit_tick(tx2, &world).unwrap().clone();
        episodes.apply_receipt(&r2).unwrap();
        let set2 = sidecar
            .advance_for_receipt(&initial_world, &world, &history, &episodes, &r2)
            .unwrap();
        assert_eq!(sidecar.world_root().root, w0.root);
        assert_eq!(sidecar.active_token_root().root, a0.root);
        assert_ne!(sidecar.episode_root().root, e0.root);
        assert_ne!(sidecar.episode_root().root, e1.root);
        assert_ne!(set2.id(), set1.id());

        // Tick 3: re-place c -> world restores W1, but active_token has fresh MintIdV1 != A1!
        let mut tx3 = history.begin_tick(3);
        tx3.apply_edit(&history, &mut world, Edit::new(Operation::On, c));
        let r3 = history.commit_tick(tx3, &world).unwrap().clone();
        episodes.apply_receipt(&r3).unwrap();
        let set3 = sidecar
            .advance_for_receipt(&initial_world, &world, &history, &episodes, &r3)
            .unwrap();
        assert_eq!(sidecar.world_root().root, w1.root);
        assert_ne!(sidecar.active_token_root().root, a1.root);
        assert_ne!(sidecar.episode_root().root, e1.root);

        // Verify independent projection from scratch agrees on every root and ledger_head_id.
        let projected = verify_independent_projection(
            &initial_world,
            &history,
            &HashSet::new(),
            &set3,
            &ProfileRegistryV1::local_v1(),
            &DomainRegistryV1::local_v1(),
            HistoryReplayLimits::DEFAULT,
        )
        .unwrap();
        assert_eq!(projected.world_root, sidecar.world_root());
        assert_eq!(projected.active_token_root, sidecar.active_token_root());
        assert_eq!(projected.episode_root, sidecar.episode_root());
    }

    #[test]
    fn golden_parity_matches_independent_python_vectors_and_pins_all_big_endian_sites() {
        fn to_hex(bytes: &[u8]) -> String {
            bytes.iter().map(|b| format!("{b:02x}")).collect()
        }

        // 1. Descriptors, profiles, and domain registry
        assert_eq!(
            ProofPolicyDescriptorV1::WORLD_DEFAULT.tag().to_string(),
            "#bb.policy.sha256.vkvqipvpzty76ylzflzkwdyuwq3u2ikjwumbtu52e45u5a6jbo3q"
        );
        assert_eq!(
            ProofPolicyDescriptorV1::ACTIVE_TOKEN_DEFAULT
                .tag()
                .to_string(),
            "#bb.policy.sha256.3rof4k6xgp5dfl5mc3fj63cuetrjetcxrucd6qhhvry3xvertw5q"
        );
        assert_eq!(
            ProofPolicyDescriptorV1::EPISODE_DEFAULT.tag().to_string(),
            "#bb.policy.sha256.qdewuvwl266u2dkfpowshnv3n3ihd52i2j366rivbph5ef4zh5ea"
        );
        assert_eq!(
            CommitmentOperationalLimitsDescriptorV1::LOCAL_DEFAULT
                .tag()
                .to_string(),
            "#bb.policy.sha256.u72lxb77ydfzak3eamjl4v53km4yb7nfkobb5dtmmzvoqicxsaua"
        );

        let prof_w = CommitmentProfileDescriptorV1::world_v1();
        let prof_a = CommitmentProfileDescriptorV1::active_token_v1();
        let prof_e = CommitmentProfileDescriptorV1::episode_v1();
        assert_eq!(
            prof_w.tag().to_string(),
            "#bb.policy.sha256.pta7j7uqpw2rtx2wmez3zobxicou5updtavrhc6kxvewbaexta6a"
        );
        assert_eq!(
            prof_a.tag().to_string(),
            "#bb.policy.sha256.m42tb7fel3fz6ktrir6huv6xq3uy67egs74xd22n2vlfsim4dbka"
        );
        assert_eq!(
            prof_e.tag().to_string(),
            "#bb.policy.sha256.3qk2vnttiwowmjhklrckm2p2dqwv6eiqozwex2wqnvk4k2uh7x2q"
        );

        let reg = DomainRegistryV1::local_v1();
        assert_eq!(
            reg.tag().to_string(),
            "#bb.policy.sha256.g7mygkuhz6daeuzqztg5bvftvhl7kutqmrvx7hsj53s22fzhpzlq"
        );

        // 2. Empty and populated typed roots matching tools/commit_state_vectors.py bit-for-bit
        let limits = CommitmentOperationalLimitsDescriptorV1::LOCAL_DEFAULT;
        assert_eq!(
            to_hex(&typed_empty_root(&prof_w)),
            "04a2eb261391b21b279448163dcac2a74245d7bde867a782c4a16cb7da8e60e6"
        );
        assert_eq!(
            to_hex(&typed_empty_root(&prof_a)),
            "09c7fc26a85988495c3999385dadda83b7de9f61551287b8cf299f71ff7459b2"
        );
        assert_eq!(
            to_hex(&typed_empty_root(&prof_e)),
            "55de32c3131e8013f2008a3d247bad53d867cfa006bb6eaa27ee9fdbe569afd7"
        );

        let w_bedrock = TypedPatriciaTrie::from_entries(
            prof_w,
            &limits,
            vec![(
                CoordinateKeyV1::from_coord(Coord::new(0, 0, 0))
                    .as_bytes()
                    .to_vec(),
                ON_V1_VALUE_BYTES.to_vec(),
            )],
        )
        .unwrap();
        let wr_bedrock = WorldRootV1 {
            root: w_bedrock.root(),
            profile_id: prof_w.id(),
        };
        assert_eq!(
            wr_bedrock.tag().to_string(),
            "#bb.world.sha256.tlnd3xyl3cp3aa6wsli3jd2uwx35srbomu4uxoarmlpgpffmxdyq"
        );

        let w_three = TypedPatriciaTrie::from_entries(
            prof_w,
            &limits,
            vec![
                (
                    CoordinateKeyV1::from_coord(Coord::new(0, 0, 0))
                        .as_bytes()
                        .to_vec(),
                    ON_V1_VALUE_BYTES.to_vec(),
                ),
                (
                    CoordinateKeyV1::from_coord(Coord::new(7, -3, 19))
                        .as_bytes()
                        .to_vec(),
                    ON_V1_VALUE_BYTES.to_vec(),
                ),
                (
                    CoordinateKeyV1::from_coord(Coord::new(i64::MIN, 0, i64::MAX))
                        .as_bytes()
                        .to_vec(),
                    ON_V1_VALUE_BYTES.to_vec(),
                ),
            ],
        )
        .unwrap();
        let wr_three = WorldRootV1 {
            root: w_three.root(),
            profile_id: prof_w.id(),
        };
        assert_eq!(
            wr_three.tag().to_string(),
            "#bb.world.sha256.3zkllbtffqgaeh7reojch6q7awq3juce4sdzsbrzoyxc65auihza"
        );

        let mint1: [u8; 32] = Sha256::digest(b"commit-1-vector-mint-1").into();
        let mint2: [u8; 32] = Sha256::digest(b"commit-1-vector-mint-2").into();
        let burn1: [u8; 32] = Sha256::digest(b"commit-1-vector-burn-1").into();
        let a_two = TypedPatriciaTrie::from_entries(
            prof_a,
            &limits,
            vec![
                (
                    CoordinateKeyV1::from_coord(Coord::new(7, -3, 19))
                        .as_bytes()
                        .to_vec(),
                    mint1.to_vec(),
                ),
                (
                    CoordinateKeyV1::from_coord(Coord::new(-16, 15, 0))
                        .as_bytes()
                        .to_vec(),
                    mint2.to_vec(),
                ),
            ],
        )
        .unwrap();
        let ar_two = ActiveTokenRootV1 {
            root: a_two.root(),
            profile_id: prof_a.id(),
        };
        assert_eq!(
            ar_two.tag().to_string(),
            "#bb.receipt.sha256.6uqfwbjqfpfllv3wsoxlgy5adzqhc2ltwlfkbnb347p5wygr56eq"
        );

        let mut twin_a = [0xAAu8; 32];
        twin_a[31] = 0x00;
        let mut twin_b = [0xAAu8; 32];
        twin_b[31] = 0x01;
        let e_three = TypedPatriciaTrie::from_entries(
            prof_e,
            &limits,
            vec![
                (
                    mint1.to_vec(),
                    EpisodeRecordV1 {
                        coord: Coord::new(7, -3, 19),
                        mint: MintIdV1::from_bytes(mint1),
                        consuming_burn: Some(EventIdV1::from_bytes(burn1)),
                    }
                    .encode_canonical(),
                ),
                (
                    twin_a.to_vec(),
                    EpisodeRecordV1 {
                        coord: Coord::new(1, 2, 3),
                        mint: MintIdV1::from_bytes(twin_a),
                        consuming_burn: None,
                    }
                    .encode_canonical(),
                ),
                (
                    twin_b.to_vec(),
                    EpisodeRecordV1 {
                        coord: Coord::new(4, 5, 6),
                        mint: MintIdV1::from_bytes(twin_b),
                        consuming_burn: None,
                    }
                    .encode_canonical(),
                ),
            ],
        )
        .unwrap();
        let er_three = EpisodeRootV1 {
            root: e_three.root(),
            profile_id: prof_e.id(),
        };
        assert_eq!(
            er_three.tag().to_string(),
            "#bb.receipt.sha256.cmyepgr23n5evmmnakckviwf7j6xvqcgquv4lh7zmo6fnzbwq4da"
        );

        // 3. Point proofs (including divergent extension & depth-256 leaf)
        let p_world_hit = w_three
            .prove(CoordinateKeyV1::from_coord(Coord::new(7, -3, 19)).as_bytes())
            .unwrap();
        assert_eq!(
            p_world_hit.proof_tag().to_string(),
            "#bb.receipt.sha256.tkvgvorsld35rwag7apuxabdai2siirjuajm7ms4yo5kjfhqsfka"
        );
        let p_world_div = w_three
            .prove(CoordinateKeyV1::from_coord(Coord::new(100, 0, 0)).as_bytes())
            .unwrap();
        assert!(matches!(
            p_world_div.terminal,
            TypedProofTerminal::DivergentExtension { .. }
        ));
        let p_div_bytes = p_world_div.to_canonical_bytes();
        let profiles = ProfileRegistryV1::local_v1();
        assert_eq!(
            verify_lookup(
                &profiles,
                &ExpectedProofContextV1::for_profile(&prof_w, w_three.root()),
                CoordinateKeyV1::from_coord(Coord::new(100, 0, 0)).as_bytes(),
                &p_div_bytes,
                None,
            )
            .unwrap(),
            LookupResult::Absent
        );
        // Pin exact big-endian bytes of divergent extension header (`ver=1, steps=1, branch(36B), term=2, depth=1, bitlen=60`)
        assert_eq!(&p_div_bytes[0..4], &[0x00, 0x01, 0x00, 0x01]);
        assert_eq!(p_div_bytes[40], PROOF_TERM_DIVERGENT_EXT);
        assert_eq!(u16::from_be_bytes([p_div_bytes[41], p_div_bytes[42]]), 1);
        assert_eq!(u16::from_be_bytes([p_div_bytes[43], p_div_bytes[44]]), 60);

        let p_ep_d256 = e_three.prove(&twin_b).unwrap();
        assert_eq!(
            p_ep_d256.proof_tag().to_string(),
            "#bb.receipt.sha256.f4uts2kyf6oart7zfdzqme4x3blrmwgf2x6bvrtoscoqgoenet7a"
        );

        // 4. CommitmentSetV1 genesis + successor tags
        let syn_lsp: [u8; 32] = Sha256::digest(b"commit-1-vector-lsp").into();
        let syn_scope: [u8; 32] = Sha256::digest(b"commit-1-vector-scope").into();
        let syn_head0: [u8; 32] = Sha256::digest(b"commit-1-vector-head-0").into();
        let syn_head1: [u8; 32] = Sha256::digest(b"commit-1-vector-head-1").into();
        let cset0 = build_commitment_set(
            syn_lsp,
            syn_scope,
            &reg,
            0,
            syn_head0,
            w_bedrock.root(),
            typed_empty_root(&prof_a),
            typed_empty_root(&prof_e),
            CommitmentSetPredecessorV1::GenesisCommitmentSetV1,
        );
        assert_eq!(
            cset0.tag().to_string(),
            "#bb.receipt.sha256.mrlbnlwnwjezmgrj5nqpngbcvng2oge4tvs2kvuzfi4beumci2cq"
        );
        let cset1 = build_commitment_set(
            syn_lsp,
            syn_scope,
            &reg,
            1,
            syn_head1,
            w_three.root(),
            a_two.root(),
            e_three.root(),
            CommitmentSetPredecessorV1::OrdinaryCommitmentSuccessorV1 {
                prior_commitment_set_id: cset0.id(),
            },
        );
        assert_eq!(
            cset1.tag().to_string(),
            "#bb.receipt.sha256.ss22ue2amqyrmaytjmxgvzhzou35fysojcbqlgys22b25wuhar7q"
        );

        // 5. Pin big-endian node store records, provisional pin, and head pointer wire headers
        let records = w_three.collect_node_records();
        let root_rec = &records[&w_three.root()];
        assert_eq!(root_rec[0], NODE_TYPE_BRANCH);
        assert_eq!(&root_rec[1..3], &[0x00, 0x00]); // depth=0 big-endian
        assert_eq!(&root_rec[3..7], &[0x00, 0x00, 0x00, 0x40]); // len=64 big-endian
        for (h, rec) in &records {
            let _ = decode_and_verify_node_record(&prof_w, h, rec).unwrap();
        }
        // Check a leaf record's depth & payload len big-endian encoding directly.
        let leaf_rec = records
            .values()
            .find(|r| r[0] == NODE_TYPE_LEAF)
            .expect("leaf record present");
        let leaf_depth = u16::from_be_bytes([leaf_rec[1], leaf_rec[2]]);
        assert!(leaf_depth > 0);
        assert_eq!(
            u32::from_be_bytes([leaf_rec[3], leaf_rec[4], leaf_rec[5], leaf_rec[6]]) as usize,
            leaf_rec.len() - 7
        );

        let pin = ProvisionalPinRecordV1 {
            attempt_id: 0x0102_0304_0506_0708,
            pin_epoch: 0x1112_1314_1516_1718,
            expected_prior_set_id: Some(cset0.id()),
            candidate_set_id: cset1.id(),
            root_keys: root_keys_for_set(&cset1),
        };
        let pin_bytes = pin.encode_checksummed();
        assert_eq!(&pin_bytes[0..2], &[0x00, 0x01]);
        assert_eq!(&pin_bytes[2..10], &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(
            &pin_bytes[10..18],
            &[0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18]
        );
        assert_eq!(
            &pin_bytes[18 + 1 + 32 + 32..18 + 1 + 32 + 32 + 2],
            &[0x00, 0x03]
        );
        assert_eq!(
            ProvisionalPinRecordV1::decode_checksummed(&pin_bytes).unwrap(),
            pin
        );

        let head_ptr = HeadPointerRecordV1 {
            head_set_id: cset1.id(),
            pin_epoch: 0x2122_2324_2526_2728,
        };
        let head_bytes = head_ptr.encode_checksummed();
        assert_eq!(&head_bytes[0..2], &[0x00, 0x01]);
        assert_eq!(
            &head_bytes[34..42],
            &[0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28]
        );
        assert_eq!(
            HeadPointerRecordV1::decode_checksummed(&head_bytes).unwrap(),
            head_ptr
        );
    }
}
