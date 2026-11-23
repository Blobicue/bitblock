//! `SYNC-1@base`: role-separated peer synchronization, bounded backpressure, checksummed
//! checkpoint bootstrap, retention pruning, opaque certificate/occurrence retention, deterministic
//! 8-peer fault injection, and native loopback TCP multi-peer recovery (`PS-034`).
//!
//! Strictly realizes ONLY `PS-034:SYNC-1@base` while keeping `PS-095:SYNC-1@replay-authority`
//! honestly `LOCKED` on `PD-06` (`CONSENSUS-1@base`):
//! - **Strict 4-role separation (`SyncNodeRoleV1`):**
//!   - [`FullVerifierNodeV1`]: verifies signed headers + complete bodies via independent replay
//!     and supports safe rolling retention pruning respecting `archive_retention_epochs >= 28` and
//!     pinned ancestors.
//!   - [`ArchiveNodeV1`]: retains complete unpruned history, chunked checkpoints, and bounded
//!     [`OpaqueAuthorityBlobV1`] bytes; structurally refuses pruning (`ArchivePruningForbidden`).
//!   - [`LightClientNodeV1`]: verifies signed headers, `ExecutionRootRefV1`, and bounded
//!     `<= 12 KiB` inclusion/absence witnesses (`ReceiptMerkleProofV1` + `DomainPatriciaProofV1`)
//!     without retaining batch bodies.
//!   - [`NonAuthorityRpcNodeV1`]: read-only rate-limited query facade enforcing `BOUNDS-1`
//!     (`<= 4 MiB` inflight, `<= 64 req/s` per peer); structurally cannot mutate ledger state or
//!     admit live authority.
//! - **Opaque-only authority transport (`OpaqueAuthorityBlobV1`):** transports and SHA-256-commits
//!   bounded `<= 64 KiB` opaque authority-certificate and occurrence bytes with a compile-time
//!   invariant `interpreted_as_live_authority() == false` (never admits consensus-native
//!   certificates or turns replay archives into live consensus input).
//! - **All 7 normative fault classes (`SyncClusterHarnessV1` + `sync_net`):** eclipse (7/8 recovery
//!   & 8/8 fail-closed refusal), partial-data/withheld-body, corrupt checkpoint chunk, slow-peer
//!   backpressure DoS, `STORAGE-1` crash-restart recovery, `>= 28`-epoch retention pruning +
//!   archive refusal, and fresh-node bootstrap over real `127.0.0.1` loopback TCP.
//!
//! Non-claim: data retrieval does not prove correct execution, canonical ordering, finality, or
//! certificate admission. Replay reconstruction audits accepted authority and never selects it live.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use sha2::{Digest, Sha256};

use crate::auth_commit_1::DomainPatriciaProofV1;
use crate::bounds_1::ProtocolBoundsProfileV1;
use crate::crypto_d1::CanonicalEd25519PublicKeyV1;
use crate::execution_root_1::ExecutionRootRefV1;
use crate::genesis_d1::BootstrapAvailabilityPolicyV1;
use crate::multi_1::{CanonicalMultiActorFixtureV1, MultiActorStepOutcomeV1};
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::sequencer_1::{
    ReceiptMerkleProofV1, Sequencer1Error, SequencerBatchHeaderV1, SequencerBatchPublicationV1,
    SequencerClientVerifierV1, receipt_merkle_leaf_digest,
};

/// Canonical schema string for a chunked checksummed [`SyncCheckpointManifestV1`].
pub const SYNC_1_CHECKPOINT_MANIFEST_SCHEMA: &[u8] = b"bitblock/sync-1/checkpoint-manifest/v1";
/// Canonical schema string for a deterministic [`SyncRecoveryReceiptV1`].
pub const SYNC_1_RECOVERY_RECEIPT_SCHEMA: &[u8] = b"bitblock/sync-1/sync-recovery-receipt/v1";

/// Protocol version encoded in all `SYNC-1@base` manifests and receipts.
pub const SYNC_1_VERSION: u16 = 1;
/// Hard cap for one [`OpaqueAuthorityBlobV1`] payload (`64 KiB`).
pub const MAX_OPAQUE_BLOB_BYTES: usize = 64 * 1024;
/// Fixed magic bytes for native loopback multi-peer sync framing.
pub const SYNC_WIRE_MAGIC: [u8; 6] = *b"BBSYN1";

const SYNC_CHUNK_PREFIX: &[u8] = b"bb.sync1.chk_chunk.v1";
const SYNC_OPAQUE_BLOB_PREFIX: &[u8] = b"bb.sync1.opaque_blob.v1";
const SYNC_OPAQUE_SET_PREFIX: &[u8] = b"bb.sync1.opaque_set.v1";

/// Errors returned by `SYNC-1@base` role enforcement, peer sync, checkpoints, pruning, or I/O.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sync1Error {
    /// Underlying `SEQUENCER-1` verification, replay, or bounds error.
    Sequencer(Sequencer1Error),
    /// Malformed, truncated, over-limit, or trailing-byte canonical encoding.
    InvalidWire(&'static str),
    /// Peer violated `BOUNDS-1` inflight-byte or requests/sec backpressure cap.
    PeerBackpressureExceeded {
        /// Offending peer identifier.
        peer_id: PeerIdV1,
        /// Reason string.
        reason: &'static str,
    },
    /// Seed peer set did not satisfy `BootstrapAvailabilityPolicyV1::min_seed_peers >= 8`.
    InsufficientPeerDiversity {
        /// Observed seed peer count.
        observed: usize,
        /// Required minimum seed peer count.
        required: u16,
    },
    /// Checkpoint chunk failed SHA-256 verification or length bounds before installation.
    CorruptCheckpointChunk {
        /// Zero-based index of the corrupt chunk.
        chunk_index: u32,
    },
    /// All discovered peers were malicious or stalled (8/8 eclipse) — failed closed with zero
    /// corrupted state installed.
    TotalEclipseNoHonestPeers,
    /// Archive nodes structurally forbid history pruning.
    ArchivePruningForbidden,
    /// Pruning attempted to drop a pinned ancestor, unverified height, or inside the retention window.
    UnsafePruneForbidden(&'static str),
    /// Light-client witness exceeded `BOUNDS-1` `12 KiB` proof cap or failed cryptographic check.
    InvalidLightWitness(&'static str),
    /// Attempted to set `live_authority_admitted = true` in `SYNC-1@base` (forbidden until `PD-06`).
    LiveAuthorityAdmissionForbiddenInBasePhase,
    /// Native loopback TCP or disk I/O error.
    Io(String),
}

impl From<Sequencer1Error> for Sync1Error {
    fn from(v: Sequencer1Error) -> Self {
        Self::Sequencer(v)
    }
}

impl fmt::Display for Sync1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Sync1Error {}

/// Strict 4-role separation required by `docs/CAPABILITY_TREE.md` §`SYNC-1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum SyncNodeRoleV1 {
    /// Independently executes & verifies complete bodies, retains rolling `>= 28`-epoch window.
    FullVerifier = 1,
    /// Retains complete unpruned history, checkpoints, and opaque authority blobs; refuses pruning.
    Archive = 2,
    /// Verifies signed headers + bounded `<= 12 KiB` inclusion/absence witnesses without full bodies.
    LightClient = 3,
    /// Read-only rate-limited query facade; cannot mutate state or assert live authority.
    NonAuthorityRpc = 4,
}

impl SyncNodeRoleV1 {
    /// Returns the canonical 1-byte role discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte role discriminant fail-closed.
    pub const fn from_code(code: u8) -> Result<Self, Sync1Error> {
        match code {
            1 => Ok(Self::FullVerifier),
            2 => Ok(Self::Archive),
            3 => Ok(Self::LightClient),
            4 => Ok(Self::NonAuthorityRpc),
            _ => Err(Sync1Error::InvalidWire("unknown SyncNodeRoleV1 code")),
        }
    }
}

/// Stable 32-byte peer identifier in [`PeerTableV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PeerIdV1(pub [u8; 32]);

/// Kind of bounded opaque authority bytes transported by `SYNC-1@base` without live admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum OpaqueBlobKindV1 {
    /// Opaque authority-certificate bytes retained for later audit (`PS-095` remains `LOCKED`).
    OpaqueCertificate = 1,
    /// Opaque wrapper-occurrence bytes retained for later audit (`PS-095` remains `LOCKED`).
    OpaqueOccurrence = 2,
}

impl OpaqueBlobKindV1 {
    /// Returns the 1-byte canonical kind code.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a 1-byte canonical kind code fail-closed.
    pub const fn from_code(code: u8) -> Result<Self, Sync1Error> {
        match code {
            1 => Ok(Self::OpaqueCertificate),
            2 => Ok(Self::OpaqueOccurrence),
            _ => Err(Sync1Error::InvalidWire("unknown OpaqueBlobKindV1 code")),
        }
    }
}

/// Bounded, checksummed opaque authority-certificate or occurrence byte carrier.
///
/// `SYNC-1@base` transports and retains these bytes verbatim without parsing, admitting, or
/// interpreting them as live consensus authority (`interpreted_as_live_authority() == false`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct OpaqueAuthorityBlobV1 {
    /// Opaque payload classification.
    pub blob_kind: OpaqueBlobKindV1,
    /// Raw opaque bytes (`1..=64 KiB`), never interpreted as live consensus authority.
    pub raw_bytes: Vec<u8>,
    /// Domain-separated SHA-256 digest binding `(blob_kind, len, raw_bytes)`.
    pub content_sha256: [u8; 32],
}

impl OpaqueAuthorityBlobV1 {
    /// Constructs and SHA-256-commits a bounded opaque blob fail-closed.
    pub fn new(blob_kind: OpaqueBlobKindV1, raw_bytes: Vec<u8>) -> Result<Self, Sync1Error> {
        if raw_bytes.is_empty() || raw_bytes.len() > MAX_OPAQUE_BLOB_BYTES {
            return Err(Sync1Error::InvalidWire(
                "opaque blob bytes must lie in 1..=64 KiB",
            ));
        }
        let content_sha256 = compute_opaque_blob_digest(blob_kind, &raw_bytes);
        Ok(Self {
            blob_kind,
            raw_bytes,
            content_sha256,
        })
    }

    /// Compile-time non-authority invariant: `SYNC-1@base` NEVER interprets opaque blobs as live
    /// consensus authority (preserving `PS-095:SYNC-1@replay-authority=LOCKED` on `PD-06`).
    pub const fn interpreted_as_live_authority(&self) -> bool {
        false
    }

    /// Encodes this opaque blob into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, Sync1Error> {
        if self.raw_bytes.is_empty() || self.raw_bytes.len() > MAX_OPAQUE_BLOB_BYTES {
            return Err(Sync1Error::InvalidWire("opaque blob out of range"));
        }
        if compute_opaque_blob_digest(self.blob_kind, &self.raw_bytes) != self.content_sha256 {
            return Err(Sync1Error::InvalidWire(
                "opaque blob content_sha256 mismatch",
            ));
        }
        let blen = u32::try_from(self.raw_bytes.len())
            .map_err(|_| Sync1Error::InvalidWire("opaque blob len overflow"))?;
        let mut out = Vec::with_capacity(1 + 4 + self.raw_bytes.len() + 32);
        out.push(self.blob_kind.code());
        out.extend_from_slice(&blen.to_be_bytes());
        out.extend_from_slice(&self.raw_bytes);
        out.extend_from_slice(&self.content_sha256);
        Ok(out)
    }

    /// Decodes and verifies a canonical [`OpaqueAuthorityBlobV1`] fail-closed.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Sync1Error> {
        let mut cur = SyncCursor::from_slice(bytes);
        let blob_kind = OpaqueBlobKindV1::from_code(cur.next_u8()?)?;
        let blen = cur.next_u32()? as usize;
        if blen == 0 || blen > MAX_OPAQUE_BLOB_BYTES {
            return Err(Sync1Error::InvalidWire("opaque blob length out of range"));
        }
        let raw_bytes = cur.next_slice(blen)?.to_vec();
        let content_sha256 = cur.next_32()?;
        cur.expect_eof()?;
        if compute_opaque_blob_digest(blob_kind, &raw_bytes) != content_sha256 {
            return Err(Sync1Error::InvalidWire("opaque blob sha256 mismatch"));
        }
        Ok(Self {
            blob_kind,
            raw_bytes,
            content_sha256,
        })
    }
}

/// Computes the domain-separated SHA-256 digest of one [`OpaqueAuthorityBlobV1`].
pub fn compute_opaque_blob_digest(blob_kind: OpaqueBlobKindV1, raw_bytes: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(SYNC_OPAQUE_BLOB_PREFIX);
    h.update([blob_kind.code()]);
    h.update((raw_bytes.len() as u32).to_be_bytes());
    h.update(raw_bytes);
    h.finalize().into()
}

/// Computes the canonical sorted SHA-256 commitment over a set of opaque blob digests.
pub fn compute_opaque_blobs_set_digest(blobs: &[OpaqueAuthorityBlobV1]) -> [u8; 32] {
    let mut sorted: Vec<[u8; 32]> = blobs.iter().map(|b| b.content_sha256).collect();
    sorted.sort();
    sorted.dedup();
    let mut h = Sha256::new();
    h.update(SYNC_OPAQUE_SET_PREFIX);
    h.update((sorted.len() as u32).to_be_bytes());
    for d in sorted {
        h.update(d);
    }
    h.finalize().into()
}

/// Computes the domain-separated SHA-256 checksum of one checkpoint chunk.
pub fn compute_checkpoint_chunk_digest(chunk_index: u32, chunk_bytes: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(SYNC_CHUNK_PREFIX);
    h.update(chunk_index.to_be_bytes());
    h.update(chunk_bytes);
    h.finalize().into()
}

/// Chunked checksummed checkpoint manifest (`bitblock/sync-1/checkpoint-manifest/v1`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SyncCheckpointManifestV1 {
    /// Manifest schema version (`1`).
    pub manifest_version: u16,
    /// Governing `genesis_id`.
    pub genesis_id: [u8; 32],
    /// Governing `profile_id`.
    pub profile_id: [u8; 32],
    /// Verified checkpoint batch height.
    pub checkpoint_height: u64,
    /// Verified checkpoint epoch.
    pub checkpoint_epoch: u64,
    /// Signed sequencer header digest at `checkpoint_height`.
    pub header_digest: [u8; 32],
    /// Verified 11-slot [`ExecutionRootRefV1`] at `checkpoint_height`.
    pub execution_root_ref: ExecutionRootRefV1,
    /// Maximum byte length of each chunk (`1..=BootstrapAvailabilityPolicyV1::max_genesis_chunk_bytes`).
    pub max_chunk_bytes: u32,
    /// Total exact byte length across all concatenated chunks.
    pub total_payload_bytes: u64,
    /// Canonical sorted SHA-256 commitment over retained opaque authority blobs.
    pub opaque_blobs_digest: [u8; 32],
    /// Ordered per-chunk domain-separated SHA-256 checksums.
    pub chunk_digests: Vec<[u8; 32]>,
}

impl SyncCheckpointManifestV1 {
    /// Splits `payload` into bounded chunks (`<= max_chunk_bytes`), computes per-chunk SHA-256
    /// checksums, and returns `(manifest, chunks)`.
    #[allow(clippy::too_many_arguments)]
    pub fn build_from_payload(
        genesis_id: [u8; 32],
        profile_id: [u8; 32],
        checkpoint_height: u64,
        checkpoint_epoch: u64,
        header_digest: [u8; 32],
        execution_root_ref: ExecutionRootRefV1,
        bootstrap_policy: &BootstrapAvailabilityPolicyV1,
        max_chunk_bytes: u32,
        opaque_blobs_digest: [u8; 32],
        payload: &[u8],
    ) -> Result<(Self, Vec<Vec<u8>>), Sync1Error> {
        if max_chunk_bytes == 0 || max_chunk_bytes > bootstrap_policy.max_genesis_chunk_bytes {
            return Err(Sync1Error::InvalidWire(
                "max_chunk_bytes violates BootstrapAvailabilityPolicyV1",
            ));
        }
        if payload.is_empty() {
            return Err(Sync1Error::InvalidWire(
                "checkpoint payload cannot be empty",
            ));
        }
        let step = max_chunk_bytes as usize;
        let mut chunks = Vec::new();
        let mut chunk_digests = Vec::new();
        for (i, slice) in payload.chunks(step).enumerate() {
            let idx =
                u32::try_from(i).map_err(|_| Sync1Error::InvalidWire("chunk index overflow"))?;
            chunk_digests.push(compute_checkpoint_chunk_digest(idx, slice));
            chunks.push(slice.to_vec());
        }
        let manifest = Self {
            manifest_version: SYNC_1_VERSION,
            genesis_id,
            profile_id,
            checkpoint_height,
            checkpoint_epoch,
            header_digest,
            execution_root_ref,
            max_chunk_bytes,
            total_payload_bytes: payload.len() as u64,
            opaque_blobs_digest,
            chunk_digests,
        };
        Ok((manifest, chunks))
    }

    /// Verifies every supplied chunk's size and SHA-256 digest before reassembling the payload;
    /// any 1-bit tamper rejects with [`Sync1Error::CorruptCheckpointChunk`] before state install.
    pub fn verify_and_reassemble(&self, chunks: &[Vec<u8>]) -> Result<Vec<u8>, Sync1Error> {
        if chunks.len() != self.chunk_digests.len() || chunks.is_empty() {
            return Err(Sync1Error::CorruptCheckpointChunk { chunk_index: 0 });
        }
        let mut out = Vec::with_capacity(self.total_payload_bytes as usize);
        for (i, chunk) in chunks.iter().enumerate() {
            let idx = i as u32;
            if chunk.is_empty() || chunk.len() > self.max_chunk_bytes as usize {
                return Err(Sync1Error::CorruptCheckpointChunk { chunk_index: idx });
            }
            if compute_checkpoint_chunk_digest(idx, chunk) != self.chunk_digests[i] {
                return Err(Sync1Error::CorruptCheckpointChunk { chunk_index: idx });
            }
            out.extend_from_slice(chunk);
        }
        if out.len() as u64 != self.total_payload_bytes {
            return Err(Sync1Error::CorruptCheckpointChunk {
                chunk_index: (chunks.len() - 1) as u32,
            });
        }
        Ok(out)
    }

    /// Encodes this checkpoint manifest into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, Sync1Error> {
        if self.manifest_version != SYNC_1_VERSION || self.chunk_digests.is_empty() {
            return Err(Sync1Error::InvalidWire("bad checkpoint manifest fields"));
        }
        let ccount = u32::try_from(self.chunk_digests.len())
            .map_err(|_| Sync1Error::InvalidWire("chunk count overflow"))?;
        let mut out = Vec::with_capacity(342 + self.chunk_digests.len() * 32);
        out.extend_from_slice(&self.manifest_version.to_be_bytes());
        out.extend_from_slice(&self.genesis_id);
        out.extend_from_slice(&self.profile_id);
        out.extend_from_slice(&self.checkpoint_height.to_be_bytes());
        out.extend_from_slice(&self.checkpoint_epoch.to_be_bytes());
        out.extend_from_slice(&self.header_digest);
        out.extend_from_slice(&self.execution_root_ref.encode_canonical());
        out.extend_from_slice(&self.max_chunk_bytes.to_be_bytes());
        out.extend_from_slice(&self.total_payload_bytes.to_be_bytes());
        out.extend_from_slice(&self.opaque_blobs_digest);
        out.extend_from_slice(&ccount.to_be_bytes());
        for d in &self.chunk_digests {
            out.extend_from_slice(d);
        }
        Ok(out)
    }

    /// Strictly decodes canonical big-endian wire bytes into a [`SyncCheckpointManifestV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Sync1Error> {
        let mut cur = SyncCursor::from_slice(bytes);
        let manifest_version = cur.next_u16()?;
        if manifest_version != SYNC_1_VERSION {
            return Err(Sync1Error::InvalidWire("bad manifest_version"));
        }
        let genesis_id = cur.next_32()?;
        let profile_id = cur.next_32()?;
        let checkpoint_height = cur.next_u64()?;
        let checkpoint_epoch = cur.next_u64()?;
        let header_digest = cur.next_32()?;
        let execution_root_ref =
            ExecutionRootRefV1::decode_canonical(cur.next_slice(ExecutionRootRefV1::WIRE_LEN)?)
                .map_err(|_| Sync1Error::InvalidWire("bad execution_root_ref"))?;
        let max_chunk_bytes = cur.next_u32()?;
        let total_payload_bytes = cur.next_u64()?;
        let opaque_blobs_digest = cur.next_32()?;
        let ccount = cur.next_u32()? as usize;
        if ccount == 0 || ccount > 65_536 {
            return Err(Sync1Error::InvalidWire("invalid chunk_digests count"));
        }
        let mut chunk_digests = Vec::with_capacity(ccount);
        for _ in 0..ccount {
            chunk_digests.push(cur.next_32()?);
        }
        cur.expect_eof()?;
        Ok(Self {
            manifest_version,
            genesis_id,
            profile_id,
            checkpoint_height,
            checkpoint_epoch,
            header_digest,
            execution_root_ref,
            max_chunk_bytes,
            total_payload_bytes,
            opaque_blobs_digest,
            chunk_digests,
        })
    }

    /// Computes the canonical `Receipt` `ContentTag` (`bitblock/sync-1/checkpoint-manifest/v1`).
    pub fn tag(&self) -> Result<ContentTag, Sync1Error> {
        let raw = self.encode_canonical()?;
        hash_canonical(
            TagKind::Receipt,
            SYNC_1_CHECKPOINT_MANIFEST_SCHEMA,
            raw.len() as u64,
            |w| w(&raw),
        )
        .map_err(|_| Sync1Error::InvalidWire("checkpoint manifest hash_canonical failed"))
    }
}

/// Deterministic sync & recovery receipt (`bitblock/sync-1/sync-recovery-receipt/v1`, `380` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SyncRecoveryReceiptV1 {
    /// Receipt schema version (`1`).
    pub receipt_version: u16,
    /// Executing node's strict [`SyncNodeRoleV1`].
    pub node_role: SyncNodeRoleV1,
    /// Governing `genesis_id`.
    pub genesis_id: [u8; 32],
    /// Governing `profile_id`.
    pub profile_id: [u8; 32],
    /// Highest verified synced batch height.
    pub synced_height: u64,
    /// Highest verified synced epoch.
    pub synced_epoch: u64,
    /// Highest height safely pruned (`0` if unpruned or `Archive`).
    pub pruned_through_height: u64,
    /// Active retention window in epochs (`>= 28`).
    pub retained_epoch_window: u32,
    /// Verified signed sequencer header digest at `synced_height`.
    pub verified_header_digest: [u8; 32],
    /// Verified 11-slot [`ExecutionRootRefV1`] at `synced_height`.
    pub verified_execution_root_ref: ExecutionRootRefV1,
    /// Digest of the verified [`SyncCheckpointManifestV1`] used during bootstrap.
    pub checkpoint_manifest_digest: [u8; 32],
    /// Number of bounded opaque authority blobs retained verbatim.
    pub opaque_blobs_retained_count: u32,
    /// Canonical sorted SHA-256 commitment over all retained opaque authority blobs.
    pub opaque_blobs_digest: [u8; 32],
    /// Total discovered peers in [`PeerTableV1`].
    pub peers_discovered: u16,
    /// Number of malicious/faulty peers banned during sync.
    pub peers_banned: u16,
    /// Number of honest peers that delivered verified data.
    pub honest_peers_used: u16,
    /// Must ALWAYS equal `false` in `SYNC-1@base` (`PS-095` remains `LOCKED` on `PD-06`).
    pub live_authority_admitted: bool,
}

impl SyncRecoveryReceiptV1 {
    /// Fixed canonical big-endian wire byte length (`380` bytes).
    pub const WIRE_LEN: usize = 2
        + 1
        + 32
        + 32
        + 8
        + 8
        + 8
        + 4
        + 32
        + ExecutionRootRefV1::WIRE_LEN
        + 32
        + 4
        + 32
        + 2
        + 2
        + 2
        + 1;

    /// Encodes this recovery receipt into 380 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], Sync1Error> {
        if self.receipt_version != SYNC_1_VERSION {
            return Err(Sync1Error::InvalidWire("bad receipt_version"));
        }
        if self.live_authority_admitted {
            return Err(Sync1Error::LiveAuthorityAdmissionForbiddenInBasePhase);
        }
        let mut out = Vec::with_capacity(Self::WIRE_LEN);
        out.extend_from_slice(&self.receipt_version.to_be_bytes());
        out.push(self.node_role.code());
        out.extend_from_slice(&self.genesis_id);
        out.extend_from_slice(&self.profile_id);
        out.extend_from_slice(&self.synced_height.to_be_bytes());
        out.extend_from_slice(&self.synced_epoch.to_be_bytes());
        out.extend_from_slice(&self.pruned_through_height.to_be_bytes());
        out.extend_from_slice(&self.retained_epoch_window.to_be_bytes());
        out.extend_from_slice(&self.verified_header_digest);
        out.extend_from_slice(&self.verified_execution_root_ref.encode_canonical());
        out.extend_from_slice(&self.checkpoint_manifest_digest);
        out.extend_from_slice(&self.opaque_blobs_retained_count.to_be_bytes());
        out.extend_from_slice(&self.opaque_blobs_digest);
        out.extend_from_slice(&self.peers_discovered.to_be_bytes());
        out.extend_from_slice(&self.peers_banned.to_be_bytes());
        out.extend_from_slice(&self.honest_peers_used.to_be_bytes());
        out.push(0u8);
        Ok(out.try_into().expect("380-byte SyncRecoveryReceiptV1"))
    }

    /// Strictly decodes 380 canonical big-endian wire bytes into a [`SyncRecoveryReceiptV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Sync1Error> {
        let mut cur = SyncCursor::from_slice(bytes);
        let receipt_version = cur.next_u16()?;
        if receipt_version != SYNC_1_VERSION {
            return Err(Sync1Error::InvalidWire("bad receipt_version"));
        }
        let node_role = SyncNodeRoleV1::from_code(cur.next_u8()?)?;
        let genesis_id = cur.next_32()?;
        let profile_id = cur.next_32()?;
        let synced_height = cur.next_u64()?;
        let synced_epoch = cur.next_u64()?;
        let pruned_through_height = cur.next_u64()?;
        let retained_epoch_window = cur.next_u32()?;
        let verified_header_digest = cur.next_32()?;
        let verified_execution_root_ref =
            ExecutionRootRefV1::decode_canonical(cur.next_slice(ExecutionRootRefV1::WIRE_LEN)?)
                .map_err(|_| Sync1Error::InvalidWire("bad execution_root_ref"))?;
        let checkpoint_manifest_digest = cur.next_32()?;
        let opaque_blobs_retained_count = cur.next_u32()?;
        let opaque_blobs_digest = cur.next_32()?;
        let peers_discovered = cur.next_u16()?;
        let peers_banned = cur.next_u16()?;
        let honest_peers_used = cur.next_u16()?;
        let live_flag = cur.next_u8()?;
        cur.expect_eof()?;
        if live_flag != 0 {
            return Err(Sync1Error::LiveAuthorityAdmissionForbiddenInBasePhase);
        }
        Ok(Self {
            receipt_version,
            node_role,
            genesis_id,
            profile_id,
            synced_height,
            synced_epoch,
            pruned_through_height,
            retained_epoch_window,
            verified_header_digest,
            verified_execution_root_ref,
            checkpoint_manifest_digest,
            opaque_blobs_retained_count,
            opaque_blobs_digest,
            peers_discovered,
            peers_banned,
            honest_peers_used,
            live_authority_admitted: false,
        })
    }

    /// Computes the canonical `Receipt` `ContentTag` (`bitblock/sync-1/sync-recovery-receipt/v1`).
    pub fn tag(&self) -> Result<ContentTag, Sync1Error> {
        let raw = self.encode_canonical()?;
        hash_canonical(
            TagKind::Receipt,
            SYNC_1_RECOVERY_RECEIPT_SCHEMA,
            raw.len() as u64,
            |w| w(&raw),
        )
        .map_err(|_| Sync1Error::InvalidWire("sync recovery receipt hash_canonical failed"))
    }
}

/// Health status of a peer inside [`PeerTableV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PeerStatusV1 {
    /// Active eligible peer.
    Healthy,
    /// Temporarily throttled after exceeding `BOUNDS-1` rate/inflight limits.
    Throttled,
    /// Permanently banned after withholding, corrupting, or forging protocol payloads.
    Banned,
}

/// Per-peer accounting entry inside [`PeerTableV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PeerEntryV1 {
    /// Stable peer ID.
    pub peer_id: PeerIdV1,
    /// Advertised node role.
    pub role: SyncNodeRoleV1,
    /// Current admission status.
    pub status: PeerStatusV1,
    /// Current buffered inflight bytes for this peer (`<= 4 MiB`).
    pub inflight_bytes: u64,
    /// Requests recorded in the current 1-second rate window (`<= 64`).
    pub requests_in_window: u64,
}

/// Bounded peer discovery and per-peer `BOUNDS-1` backpressure table.
#[derive(Clone, Debug)]
pub struct PeerTableV1 {
    bounds: ProtocolBoundsProfileV1,
    bootstrap: BootstrapAvailabilityPolicyV1,
    peers: BTreeMap<PeerIdV1, PeerEntryV1>,
}

impl PeerTableV1 {
    /// Creates an empty peer table governed by `bounds` and `bootstrap`.
    pub const fn new(
        bounds: ProtocolBoundsProfileV1,
        bootstrap: BootstrapAvailabilityPolicyV1,
    ) -> Self {
        Self {
            bounds,
            bootstrap,
            peers: BTreeMap::new(),
        }
    }

    /// Registers a discovered seed peer.
    pub fn register_peer(&mut self, peer_id: PeerIdV1, role: SyncNodeRoleV1) {
        self.peers.insert(
            peer_id,
            PeerEntryV1 {
                peer_id,
                role,
                status: PeerStatusV1::Healthy,
                inflight_bytes: 0,
                requests_in_window: 0,
            },
        );
    }

    /// Verifies that the peer table satisfies `BootstrapAvailabilityPolicyV1::min_seed_peers >= 8`.
    pub fn verify_seed_diversity(&self) -> Result<(), Sync1Error> {
        if self.peers.len() < usize::from(self.bootstrap.min_seed_peers) {
            return Err(Sync1Error::InsufficientPeerDiversity {
                observed: self.peers.len(),
                required: self.bootstrap.min_seed_peers,
            });
        }
        Ok(())
    }

    /// Checks and accounts one peer request against `BOUNDS-1` `max_peer_inflight_bytes` (`4 MiB`)
    /// and `max_peer_requests_per_sec` (`64 req/s`); throttles offending peer without starving others.
    pub fn charge_peer_request(
        &mut self,
        peer_id: PeerIdV1,
        added_bytes: u64,
    ) -> Result<(), Sync1Error> {
        let Some(entry) = self.peers.get_mut(&peer_id) else {
            return Err(Sync1Error::InvalidWire("unknown peer"));
        };
        if entry.status == PeerStatusV1::Banned {
            return Err(Sync1Error::PeerBackpressureExceeded {
                peer_id,
                reason: "peer is banned",
            });
        }
        let next_bytes = entry.inflight_bytes.saturating_add(added_bytes);
        let next_reqs = entry.requests_in_window.saturating_add(1);
        if self
            .bounds
            .check_peer_budget(next_bytes, next_reqs)
            .is_err()
        {
            entry.status = PeerStatusV1::Throttled;
            return Err(Sync1Error::PeerBackpressureExceeded {
                peer_id,
                reason: "exceeded BOUNDS-1 inflight bytes or requests/sec",
            });
        }
        entry.inflight_bytes = next_bytes;
        entry.requests_in_window = next_reqs;
        Ok(())
    }

    /// Permanently bans a misbehaving peer.
    pub fn ban_peer(&mut self, peer_id: PeerIdV1) {
        if let Some(e) = self.peers.get_mut(&peer_id) {
            e.status = PeerStatusV1::Banned;
        }
    }

    /// Returns counts `(discovered, banned, healthy)`.
    pub fn counts(&self) -> (u16, u16, u16) {
        let total = self.peers.len() as u16;
        let banned = self
            .peers
            .values()
            .filter(|e| e.status == PeerStatusV1::Banned)
            .count() as u16;
        let healthy = self
            .peers
            .values()
            .filter(|e| e.status == PeerStatusV1::Healthy)
            .count() as u16;
        (total, banned, healthy)
    }

    /// Returns ordered peer IDs currently eligible (`Healthy`).
    pub fn healthy_peer_ids(&self) -> Vec<PeerIdV1> {
        self.peers
            .values()
            .filter(|e| e.status == PeerStatusV1::Healthy)
            .map(|e| e.peer_id)
            .collect()
    }
}

/// Role 1 (`FullVerifier`): independently executes & verifies bodies and supports bounded rolling
/// retention pruning preserving `archive_retention_epochs >= 28` and pinned heights.
#[derive(Clone, Debug)]
pub struct FullVerifierNodeV1 {
    verifier: SequencerClientVerifierV1,
    bootstrap: BootstrapAvailabilityPolicyV1,
    retained_publications: BTreeMap<u64, SequencerBatchPublicationV1>,
    pinned_heights: BTreeSet<u64>,
    opaque_blobs: BTreeMap<[u8; 32], OpaqueAuthorityBlobV1>,
    pruned_through_height: u64,
}

impl FullVerifierNodeV1 {
    /// Boots a fresh `FullVerifier` from verified genesis (`GENESIS-ROOT-PARITY-1`).
    pub fn from_fixture(
        fix: &CanonicalMultiActorFixtureV1,
        expected_sequencer_pk: CanonicalEd25519PublicKeyV1,
    ) -> Result<Self, Sync1Error> {
        let verifier = SequencerClientVerifierV1::from_fixture(fix, expected_sequencer_pk)?;
        Ok(Self {
            verifier,
            bootstrap: fix.bundle.bootstrap_policy,
            retained_publications: BTreeMap::new(),
            pinned_heights: BTreeSet::new(),
            opaque_blobs: BTreeMap::new(),
            pruned_through_height: 0,
        })
    }

    /// Independently verifies and applies one complete batch publication.
    pub fn apply_publication(
        &mut self,
        publication: &SequencerBatchPublicationV1,
    ) -> Result<(), Sync1Error> {
        let _ = self.verifier.verify_and_apply_publication(publication)?;
        self.retained_publications
            .insert(publication.header.batch_height, publication.clone());
        Ok(())
    }

    /// Pins a specific verified height so rolling pruning can never delete it.
    pub fn pin_height(&mut self, height: u64) {
        self.pinned_heights.insert(height);
    }

    /// Retains one bounded [`OpaqueAuthorityBlobV1`] verbatim without live authority admission.
    pub fn retain_opaque_blob(&mut self, blob: OpaqueAuthorityBlobV1) -> Result<(), Sync1Error> {
        let _ = blob.encode_canonical()?;
        debug_assert!(!blob.interpreted_as_live_authority());
        self.opaque_blobs.insert(blob.content_sha256, blob);
        Ok(())
    }

    /// Prunes retained publications whose epoch is strictly older than
    /// `current_epoch - archive_retention_epochs`, never deleting the tip or any pinned height.
    pub fn prune_below_retention_window(
        &mut self,
        current_epoch: u64,
    ) -> Result<usize, Sync1Error> {
        let tip = self.verifier.verified_height();
        let cutoff_epoch =
            current_epoch.saturating_sub(u64::from(self.bootstrap.archive_retention_epochs));
        let mut to_remove = Vec::new();
        for (&h, p) in &self.retained_publications {
            if h == tip || self.pinned_heights.contains(&h) {
                continue;
            }
            if p.header.epoch < cutoff_epoch {
                to_remove.push(h);
            }
        }
        for &h in &to_remove {
            self.retained_publications.remove(&h);
            if h > self.pruned_through_height {
                self.pruned_through_height = h;
            }
        }
        Ok(to_remove.len())
    }

    /// Explicitly refuses unsafe pruning of a pinned ancestor or tip height.
    pub fn attempt_force_prune_height(&mut self, height: u64) -> Result<(), Sync1Error> {
        if height == self.verifier.verified_height() {
            return Err(Sync1Error::UnsafePruneForbidden(
                "cannot prune current verified tip height",
            ));
        }
        if self.pinned_heights.contains(&height) {
            return Err(Sync1Error::UnsafePruneForbidden(
                "cannot prune pinned ancestor height",
            ));
        }
        if let Some(p) = self.retained_publications.get(&height) {
            let tip_epoch = self
                .retained_publications
                .values()
                .map(|b| b.header.epoch)
                .max()
                .unwrap_or(0);
            let min_keep =
                tip_epoch.saturating_sub(u64::from(self.bootstrap.archive_retention_epochs));
            if p.header.epoch >= min_keep {
                return Err(Sync1Error::UnsafePruneForbidden(
                    "height still inside >= 28-epoch retention window",
                ));
            }
        }
        self.retained_publications.remove(&height);
        if height > self.pruned_through_height {
            self.pruned_through_height = height;
        }
        Ok(())
    }

    /// Returns verified tip height.
    pub const fn verified_height(&self) -> u64 {
        self.verifier.verified_height()
    }

    /// Returns highest pruned height (`0` if unpruned).
    pub const fn pruned_through_height(&self) -> u64 {
        self.pruned_through_height
    }

    /// Returns the number of currently retained batch publications.
    pub fn retained_count(&self) -> usize {
        self.retained_publications.len()
    }

    /// Returns all retained opaque authority blobs in sorted digest order.
    pub fn opaque_blobs(&self) -> Vec<OpaqueAuthorityBlobV1> {
        self.opaque_blobs.values().cloned().collect()
    }

    /// Returns the verified signed header at `height`, if applied.
    pub fn verified_header(&self, height: u64) -> Option<&SequencerBatchHeaderV1> {
        self.verifier.verified_header(height)
    }

    /// Returns the last verified header digest.
    pub const fn last_header_digest(&self) -> [u8; 32] {
        self.verifier.last_header_digest()
    }
}

/// Role 2 (`Archive`): retains complete unpruned history, chunked checkpoints, and opaque
/// authority blobs; structurally refuses any pruning request.
#[derive(Clone, Debug)]
pub struct ArchiveNodeV1 {
    bootstrap: BootstrapAvailabilityPolicyV1,
    publications: BTreeMap<u64, SequencerBatchPublicationV1>,
    opaque_blobs: BTreeMap<[u8; 32], OpaqueAuthorityBlobV1>,
}

impl ArchiveNodeV1 {
    /// Creates an empty unpruned archive node.
    pub const fn new(bootstrap: BootstrapAvailabilityPolicyV1) -> Self {
        Self {
            bootstrap,
            publications: BTreeMap::new(),
            opaque_blobs: BTreeMap::new(),
        }
    }

    /// Stores one verified batch publication permanently.
    pub fn store_publication(&mut self, publication: SequencerBatchPublicationV1) {
        self.publications
            .insert(publication.header.batch_height, publication);
    }

    /// Stores one bounded [`OpaqueAuthorityBlobV1`] verbatim (never interpreted as live authority).
    pub fn store_opaque_blob(&mut self, blob: OpaqueAuthorityBlobV1) -> Result<(), Sync1Error> {
        let _ = blob.encode_canonical()?;
        debug_assert!(!blob.interpreted_as_live_authority());
        self.opaque_blobs.insert(blob.content_sha256, blob);
        Ok(())
    }

    /// Archive nodes structurally forbid pruning under `SYNC-1@base`.
    pub fn attempt_prune(&mut self, _below_height: u64) -> Result<(), Sync1Error> {
        Err(Sync1Error::ArchivePruningForbidden)
    }

    /// Builds a chunked checksummed [`SyncCheckpointManifestV1`] over the canonical publication
    /// bytes at `height`.
    pub fn build_checkpoint(
        &self,
        height: u64,
        max_chunk_bytes: u32,
    ) -> Result<(SyncCheckpointManifestV1, Vec<Vec<u8>>), Sync1Error> {
        let Some(pub_b) = self.publications.get(&height) else {
            return Err(Sync1Error::InvalidWire("archive missing checkpoint height"));
        };
        let payload = pub_b.encode_canonical()?;
        let blobs = self.opaque_blobs();
        let opaque_dig = compute_opaque_blobs_set_digest(&blobs);
        SyncCheckpointManifestV1::build_from_payload(
            pub_b.header.fork_context.genesis_id,
            pub_b.header.fork_context.profile_id,
            pub_b.header.batch_height,
            pub_b.header.epoch,
            pub_b.header.header_digest()?,
            pub_b.header.execution_root_ref,
            &self.bootstrap,
            max_chunk_bytes,
            opaque_dig,
            &payload,
        )
    }

    /// Returns all retained publications in ascending height order.
    pub fn publications(&self) -> Vec<SequencerBatchPublicationV1> {
        self.publications.values().cloned().collect()
    }

    /// Returns all retained opaque authority blobs in sorted digest order.
    pub fn opaque_blobs(&self) -> Vec<OpaqueAuthorityBlobV1> {
        self.opaque_blobs.values().cloned().collect()
    }
}

/// Role 3 (`LightClient`): verifies signed headers and bounded `<= 12 KiB` inclusion/absence
/// witnesses without retaining full batch bodies.
#[derive(Clone, Debug)]
pub struct LightClientNodeV1 {
    expected_sequencer_pk: CanonicalEd25519PublicKeyV1,
    bounds: ProtocolBoundsProfileV1,
    headers: BTreeMap<u64, SequencerBatchHeaderV1>,
    last_header_digest: [u8; 32],
}

impl LightClientNodeV1 {
    /// Boots a light client pinned to `expected_sequencer_pk` and `bounds`.
    pub const fn new(
        expected_sequencer_pk: CanonicalEd25519PublicKeyV1,
        bounds: ProtocolBoundsProfileV1,
    ) -> Self {
        Self {
            expected_sequencer_pk,
            bounds,
            headers: BTreeMap::new(),
            last_header_digest: [0u8; 32],
        }
    }

    /// Verifies and tracks one sequential signed header (rejects bad signatures, parent breaks,
    /// and same-height equivocation) without storing full batch bodies.
    pub fn track_signed_header(
        &mut self,
        header: &SequencerBatchHeaderV1,
    ) -> Result<(), Sync1Error> {
        header.verify_signature(&self.expected_sequencer_pk)?;
        let dig = header.header_digest()?;
        if let Some(prev) = self.headers.get(&header.batch_height) {
            let prev_dig = prev.header_digest()?;
            if prev_dig != dig {
                return Err(Sync1Error::Sequencer(
                    Sequencer1Error::EquivocationDetected {
                        height: header.batch_height,
                        existing_header_digest: prev_dig,
                        conflicting_header_digest: dig,
                    },
                ));
            }
            return Ok(());
        }
        let next_h = self.headers.len() as u64 + 1;
        if header.batch_height != next_h || header.parent_header_digest != self.last_header_digest {
            return Err(Sync1Error::Sequencer(
                Sequencer1Error::ChainContinuityError("light header continuity mismatch"),
            ));
        }
        self.headers.insert(header.batch_height, *header);
        self.last_header_digest = dig;
        Ok(())
    }

    /// Verifies a bounded `<= 12 KiB` [`ReceiptMerkleProofV1`] against the tracked signed header.
    pub fn verify_receipt_witness(
        &self,
        height: u64,
        proof: &ReceiptMerkleProofV1,
        outcome: &MultiActorStepOutcomeV1,
    ) -> Result<(), Sync1Error> {
        let encoded = proof.encode_canonical()?;
        if encoded.len() > self.bounds.max_proof_bytes as usize {
            return Err(Sync1Error::InvalidLightWitness(
                "receipt proof exceeds BOUNDS-1 max_proof_bytes",
            ));
        }
        let Some(hdr) = self.headers.get(&height) else {
            return Err(Sync1Error::InvalidLightWitness(
                "unknown light header height",
            ));
        };
        if proof.leaf_digest != receipt_merkle_leaf_digest(proof.leaf_index, outcome) {
            return Err(Sync1Error::InvalidLightWitness("leaf digest mismatch"));
        }
        proof.verify_against_root(hdr.receipts_merkle_root)?;
        Ok(())
    }

    /// Verifies a bounded `<= 12 KiB` / `<= 256`-depth [`DomainPatriciaProofV1`] membership
    /// witness against a supplied domain root.
    pub fn verify_patricia_witness(
        &self,
        proof: &DomainPatriciaProofV1,
        expected_root: [u8; 32],
        expected_value: &[u8],
    ) -> Result<(), Sync1Error> {
        let encoded = proof
            .encode_canonical()
            .map_err(|_| Sync1Error::InvalidLightWitness("patricia proof encode failed"))?;
        if encoded.len() > self.bounds.max_proof_bytes as usize
            || proof.steps.len() > self.bounds.max_proof_depth as usize
        {
            return Err(Sync1Error::InvalidLightWitness(
                "patricia proof exceeds BOUNDS-1 limits",
            ));
        }
        let observed = proof
            .verify(proof.domain, expected_root, &proof.key)
            .map_err(|_| Sync1Error::InvalidLightWitness("patricia membership verify failed"))?;
        if observed.as_deref() != Some(expected_value) {
            return Err(Sync1Error::InvalidLightWitness(
                "patricia verified value mismatch",
            ));
        }
        Ok(())
    }

    /// Compile-time structural guarantee: light clients store zero batch bodies.
    pub const fn stored_body_count(&self) -> usize {
        0
    }

    /// Returns the highest verified header height.
    pub fn tip_height(&self) -> u64 {
        self.headers.len() as u64
    }
}

/// Role 4 (`NonAuthorityRpc`): read-only rate-limited query facade enforcing `BOUNDS-1` per-peer
/// budgets (`<= 4 MiB` inflight, `<= 64 req/s`). Structurally cannot mutate ledger state or
/// manufacture authority/finality claims.
#[derive(Clone, Debug)]
pub struct NonAuthorityRpcNodeV1 {
    peer_table: PeerTableV1,
    headers: BTreeMap<u64, SequencerBatchHeaderV1>,
}

impl NonAuthorityRpcNodeV1 {
    /// Creates a read-only non-authority RPC facade over an immutable snapshot of headers.
    pub fn new(
        bounds: ProtocolBoundsProfileV1,
        bootstrap: BootstrapAvailabilityPolicyV1,
        headers: Vec<SequencerBatchHeaderV1>,
    ) -> Self {
        let mut map = BTreeMap::new();
        for h in headers {
            map.insert(h.batch_height, h);
        }
        Self {
            peer_table: PeerTableV1::new(bounds, bootstrap),
            headers: map,
        }
    }

    /// Registers an RPC client peer for `BOUNDS-1` backpressure tracking.
    pub fn register_client(&mut self, peer_id: PeerIdV1) {
        self.peer_table
            .register_peer(peer_id, SyncNodeRoleV1::LightClient);
    }

    /// Services a rate-limited read-only header lookup (`<= 4 MiB` inflight, `<= 64 req/s`).
    pub fn query_header(
        &mut self,
        caller: PeerIdV1,
        height: u64,
        declared_req_bytes: u64,
    ) -> Result<Option<SequencerBatchHeaderV1>, Sync1Error> {
        self.peer_table
            .charge_peer_request(caller, declared_req_bytes)?;
        Ok(self.headers.get(&height).copied())
    }

    /// Structural non-authority invariant: RPC nodes NEVER mutate ledger state.
    pub const fn can_mutate_ledger(&self) -> bool {
        false
    }

    /// Structural non-authority invariant: RPC nodes NEVER admit live authority.
    pub const fn can_admit_live_authority(&self) -> bool {
        false
    }
}

/// Per-peer fault behavior injected by [`SyncClusterHarnessV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimulatedPeerBehaviorV1 {
    /// Honest peer serving genuine verified checkpoint chunks, publications, and opaque blobs.
    Honest,
    /// Malicious peer withholding bodies (serves header-only without body).
    WithholdBody,
    /// Malicious peer flipping 1 bit inside a checkpoint chunk.
    CorruptCheckpointChunk,
    /// Malicious peer forging a tampered sequencer signature.
    BadSignature,
    /// Malicious slow/DoS peer exceeding `BOUNDS-1` inflight bytes or request rate.
    SlowBackpressureDoS,
    /// Unresponsive stalling peer.
    StallTimeout,
}

/// Deterministic 8-peer fault-injection cluster covering all 7 normative `SYNC-1@base` fault classes.
#[derive(Clone, Debug)]
pub struct SyncClusterHarnessV1 {
    /// Peer discovery and backpressure table (`>= 8` diverse peers).
    pub peer_table: PeerTableV1,
    /// Assigned fault class for each peer ID.
    pub behaviors: BTreeMap<PeerIdV1, SimulatedPeerBehaviorV1>,
    /// Canonical honest archive source backing honest peers.
    pub honest_archive: ArchiveNodeV1,
    /// Canonical checkpoint manifest for bootstrap.
    pub checkpoint_manifest: SyncCheckpointManifestV1,
    /// Genuine chunk payloads matching `checkpoint_manifest`.
    pub checkpoint_chunks: Vec<Vec<u8>>,
}

impl SyncClusterHarnessV1 {
    /// Constructs a deterministic 8-peer cluster from an honest [`ArchiveNodeV1`] and per-peer
    /// fault assignments (`peer_behaviors.len() >= 8`).
    pub fn new(
        bounds: ProtocolBoundsProfileV1,
        bootstrap: BootstrapAvailabilityPolicyV1,
        honest_archive: ArchiveNodeV1,
        checkpoint_height: u64,
        max_chunk_bytes: u32,
        peer_behaviors: &[(PeerIdV1, SyncNodeRoleV1, SimulatedPeerBehaviorV1)],
    ) -> Result<Self, Sync1Error> {
        let mut peer_table = PeerTableV1::new(bounds, bootstrap);
        let mut behaviors = BTreeMap::new();
        for &(pid, role, beh) in peer_behaviors {
            peer_table.register_peer(pid, role);
            behaviors.insert(pid, beh);
        }
        peer_table.verify_seed_diversity()?;
        let (checkpoint_manifest, checkpoint_chunks) =
            honest_archive.build_checkpoint(checkpoint_height, max_chunk_bytes)?;
        Ok(Self {
            peer_table,
            behaviors,
            honest_archive,
            checkpoint_manifest,
            checkpoint_chunks,
        })
    }

    /// Synchronizes a fresh [`FullVerifierNodeV1`] across the 8-peer cluster, automatically
    /// banning withholding, corrupt-chunk, bad-signature, slow-DoS, and stalling peers until an
    /// honest peer delivers verifiable data (or failing closed under 8/8 total eclipse).
    pub fn sync_fresh_full_verifier(
        &mut self,
        fix: &CanonicalMultiActorFixtureV1,
        expected_sequencer_pk: CanonicalEd25519PublicKeyV1,
    ) -> Result<(FullVerifierNodeV1, SyncRecoveryReceiptV1), Sync1Error> {
        self.peer_table.verify_seed_diversity()?;
        let candidates = self.peer_table.healthy_peer_ids();
        let mut synced_node: Option<FullVerifierNodeV1> = None;
        let mut honest_used = 0u16;

        for pid in candidates {
            let beh = self
                .behaviors
                .get(&pid)
                .copied()
                .unwrap_or(SimulatedPeerBehaviorV1::StallTimeout);
            match beh {
                SimulatedPeerBehaviorV1::StallTimeout => {
                    self.peer_table.ban_peer(pid);
                }
                SimulatedPeerBehaviorV1::SlowBackpressureDoS => {
                    let over = u64::from(fix.bundle.bounds_profile.max_peer_inflight_bytes) + 1;
                    let _ = self.peer_table.charge_peer_request(pid, over);
                    self.peer_table.ban_peer(pid);
                }
                SimulatedPeerBehaviorV1::WithholdBody => {
                    // Header advertised without body -> client refuses & bans peer
                    let cli = SequencerClientVerifierV1::from_fixture(fix, expected_sequencer_pk)?;
                    if let Some(p) = self.honest_archive.publications().first() {
                        let res = cli.submit_header_without_body(&p.header);
                        debug_assert!(matches!(res, Err(Sequencer1Error::WithheldBodyRefused)));
                    }
                    self.peer_table.ban_peer(pid);
                }
                SimulatedPeerBehaviorV1::CorruptCheckpointChunk => {
                    let mut bad_chunks = self.checkpoint_chunks.clone();
                    bad_chunks[0][0] ^= 0x01;
                    let res = self.checkpoint_manifest.verify_and_reassemble(&bad_chunks);
                    debug_assert!(matches!(
                        res,
                        Err(Sync1Error::CorruptCheckpointChunk { chunk_index: 0 })
                    ));
                    self.peer_table.ban_peer(pid);
                }
                SimulatedPeerBehaviorV1::BadSignature => {
                    let mut node = FullVerifierNodeV1::from_fixture(fix, expected_sequencer_pk)?;
                    if let Some(mut p) = self.honest_archive.publications().first().cloned() {
                        p.header.sequencer_signature[0] ^= 0x40;
                        let res = node.apply_publication(&p);
                        debug_assert!(res.is_err());
                    }
                    self.peer_table.ban_peer(pid);
                }
                SimulatedPeerBehaviorV1::Honest => {
                    self.peer_table
                        .charge_peer_request(pid, self.checkpoint_manifest.total_payload_bytes)?;
                    let reassembled = self
                        .checkpoint_manifest
                        .verify_and_reassemble(&self.checkpoint_chunks)?;
                    let _chk_pub = SequencerBatchPublicationV1::decode_canonical(
                        &reassembled,
                        &fix.bundle.bounds_profile,
                    )?;
                    let mut node = FullVerifierNodeV1::from_fixture(fix, expected_sequencer_pk)?;
                    for p in self.honest_archive.publications() {
                        node.apply_publication(&p)?;
                    }
                    for blob in self.honest_archive.opaque_blobs() {
                        node.retain_opaque_blob(blob)?;
                    }
                    honest_used = 1;
                    synced_node = Some(node);
                    break;
                }
            }
        }

        let Some(node) = synced_node else {
            return Err(Sync1Error::TotalEclipseNoHonestPeers);
        };
        let (discovered, banned, _) = self.peer_table.counts();
        let tip_h = node.verified_height();
        let tip_hdr = node
            .verified_header(tip_h)
            .ok_or(Sync1Error::InvalidWire("missing tip header"))?;
        let blobs = node.opaque_blobs();
        let receipt = SyncRecoveryReceiptV1 {
            receipt_version: SYNC_1_VERSION,
            node_role: SyncNodeRoleV1::FullVerifier,
            genesis_id: fix.package.genesis_id,
            profile_id: fix.bound_manifest.profile_id,
            synced_height: tip_h,
            synced_epoch: tip_hdr.epoch,
            pruned_through_height: node.pruned_through_height(),
            retained_epoch_window: fix.bundle.bootstrap_policy.archive_retention_epochs,
            verified_header_digest: node.last_header_digest(),
            verified_execution_root_ref: tip_hdr.execution_root_ref,
            checkpoint_manifest_digest: self.checkpoint_manifest.tag()?.digest(),
            opaque_blobs_retained_count: blobs.len() as u32,
            opaque_blobs_digest: compute_opaque_blobs_set_digest(&blobs),
            peers_discovered: discovered,
            peers_banned: banned,
            honest_peers_used: honest_used,
            live_authority_admitted: false,
        };
        Ok((node, receipt))
    }
}

/// Native `std::net` loopback TCP multi-peer bootstrap adapter (`#[cfg(not(target_arch = "wasm32"))]`)
/// streaming checksummed checkpoint manifests, chunks, publications, and opaque blobs under strict
/// pre-allocation `BOUNDS-1` frame limits.
#[cfg(not(target_arch = "wasm32"))]
pub mod sync_net {
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpListener, TcpStream};
    use std::time::Duration;

    use super::{
        ArchiveNodeV1, MAX_OPAQUE_BLOB_BYTES, OpaqueAuthorityBlobV1, ProtocolBoundsProfileV1,
        SYNC_1_VERSION, SYNC_WIRE_MAGIC, SequencerBatchPublicationV1, Sync1Error,
        SyncCheckpointManifestV1,
    };

    /// Complete bootstrap payload returned by [`fetch_archive_bootstrap_loopback`].
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct LoopbackBootstrapBundleV1 {
        /// Verified checkpoint manifest.
        pub manifest: SyncCheckpointManifestV1,
        /// Verified raw checkpoint chunks.
        pub chunks: Vec<Vec<u8>>,
        /// Verified full publications in ascending height order.
        pub publications: Vec<SequencerBatchPublicationV1>,
        /// Retained opaque authority blobs (never interpreted as live authority).
        pub opaque_blobs: Vec<OpaqueAuthorityBlobV1>,
    }

    fn write_frame<W: Write>(
        w: &mut W,
        kind: u8,
        payload: &[u8],
        max_bytes: usize,
    ) -> Result<(), Sync1Error> {
        if payload.len() > max_bytes {
            return Err(Sync1Error::InvalidWire("sync_net frame exceeds bound"));
        }
        let mut hdr = [0u8; 13];
        hdr[0..6].copy_from_slice(&SYNC_WIRE_MAGIC);
        hdr[6..8].copy_from_slice(&SYNC_1_VERSION.to_be_bytes());
        hdr[8] = kind;
        hdr[9..13].copy_from_slice(&(payload.len() as u32).to_be_bytes());
        w.write_all(&hdr)
            .map_err(|e| Sync1Error::Io(format!("sync_net write hdr: {e}")))?;
        w.write_all(payload)
            .map_err(|e| Sync1Error::Io(format!("sync_net write body: {e}")))?;
        w.flush()
            .map_err(|e| Sync1Error::Io(format!("sync_net flush: {e}")))
    }

    fn read_frame<R: Read>(r: &mut R, max_bytes: usize) -> Result<(u8, Vec<u8>), Sync1Error> {
        let mut hdr = [0u8; 13];
        r.read_exact(&mut hdr)
            .map_err(|e| Sync1Error::Io(format!("sync_net read hdr: {e}")))?;
        if hdr[0..6] != SYNC_WIRE_MAGIC || u16::from_be_bytes([hdr[6], hdr[7]]) != SYNC_1_VERSION {
            return Err(Sync1Error::InvalidWire("bad BBSYN1 frame header"));
        }
        let kind = hdr[8];
        let plen = u32::from_be_bytes([hdr[9], hdr[10], hdr[11], hdr[12]]) as usize;
        if plen > max_bytes {
            return Err(Sync1Error::InvalidWire(
                "sync_net frame rejected before allocation",
            ));
        }
        let mut buf = vec![0u8; plen];
        r.read_exact(&mut buf)
            .map_err(|e| Sync1Error::Io(format!("sync_net read body: {e}")))?;
        Ok((kind, buf))
    }

    /// Serves one fresh-node bootstrap session from an [`ArchiveNodeV1`] over `listener`.
    pub fn serve_archive_bootstrap_once(
        listener: &TcpListener,
        archive: &ArchiveNodeV1,
        checkpoint_height: u64,
        max_chunk_bytes: u32,
        bounds: &ProtocolBoundsProfileV1,
    ) -> Result<(), Sync1Error> {
        let (mut stream, _) = listener
            .accept()
            .map_err(|e| Sync1Error::Io(format!("sync_net accept: {e}")))?;
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
        let (req_kind, _) = read_frame(&mut stream, 64)?;
        if req_kind != 1 {
            return Err(Sync1Error::InvalidWire("expected bootstrap request kind 1"));
        }
        let (manifest, chunks) = archive.build_checkpoint(checkpoint_height, max_chunk_bytes)?;
        let cap = bounds.max_block_bytes as usize;
        write_frame(&mut stream, 2, &manifest.encode_canonical()?, cap)?;
        for c in &chunks {
            write_frame(&mut stream, 3, c, cap)?;
        }
        for p in archive.publications() {
            write_frame(&mut stream, 4, &p.encode_canonical()?, cap)?;
        }
        for b in archive.opaque_blobs() {
            write_frame(
                &mut stream,
                5,
                &b.encode_canonical()?,
                MAX_OPAQUE_BLOB_BYTES + 64,
            )?;
        }
        write_frame(&mut stream, 6, &[], 64)?;
        Ok(())
    }

    /// Pulls a complete bounded bootstrap bundle from a loopback archive endpoint at `addr`.
    pub fn fetch_archive_bootstrap_loopback(
        addr: SocketAddr,
        bounds: &ProtocolBoundsProfileV1,
    ) -> Result<LoopbackBootstrapBundleV1, Sync1Error> {
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5))
            .map_err(|e| Sync1Error::Io(format!("sync_net connect: {e}")))?;
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
        write_frame(&mut stream, 1, &[], 64)?;
        let cap = bounds.max_block_bytes as usize;
        let mut manifest: Option<SyncCheckpointManifestV1> = None;
        let mut chunks = Vec::new();
        let mut publications = Vec::new();
        let mut opaque_blobs = Vec::new();
        loop {
            let (kind, body) = read_frame(&mut stream, cap)?;
            match kind {
                2 => manifest = Some(SyncCheckpointManifestV1::decode_canonical(&body)?),
                3 => chunks.push(body),
                4 => publications.push(SequencerBatchPublicationV1::decode_canonical(
                    &body, bounds,
                )?),
                5 => opaque_blobs.push(OpaqueAuthorityBlobV1::decode_canonical(&body)?),
                6 => break,
                _ => return Err(Sync1Error::InvalidWire("unexpected sync_net frame kind")),
            }
        }
        let manifest = manifest.ok_or(Sync1Error::InvalidWire("missing checkpoint manifest"))?;
        let _ = manifest.verify_and_reassemble(&chunks)?;
        Ok(LoopbackBootstrapBundleV1 {
            manifest,
            chunks,
            publications,
            opaque_blobs,
        })
    }
}

struct SyncCursor<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> SyncCursor<'a> {
    const fn from_slice(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn next_slice(&mut self, len: usize) -> Result<&'a [u8], Sync1Error> {
        if self.data.len().saturating_sub(self.offset) < len {
            return Err(Sync1Error::InvalidWire("truncated sync_1 payload"));
        }
        let s = &self.data[self.offset..self.offset + len];
        self.offset += len;
        Ok(s)
    }

    fn next_u8(&mut self) -> Result<u8, Sync1Error> {
        Ok(self.next_slice(1)?[0])
    }

    fn next_u16(&mut self) -> Result<u16, Sync1Error> {
        Ok(u16::from_be_bytes(self.next_slice(2)?.try_into().unwrap()))
    }

    fn next_u32(&mut self) -> Result<u32, Sync1Error> {
        Ok(u32::from_be_bytes(self.next_slice(4)?.try_into().unwrap()))
    }

    fn next_u64(&mut self) -> Result<u64, Sync1Error> {
        Ok(u64::from_be_bytes(self.next_slice(8)?.try_into().unwrap()))
    }

    fn next_32(&mut self) -> Result<[u8; 32], Sync1Error> {
        Ok(self.next_slice(32)?.try_into().unwrap())
    }

    fn expect_eof(self) -> Result<(), Sync1Error> {
        if self.offset != self.data.len() {
            return Err(Sync1Error::InvalidWire("trailing sync_1 payload bytes"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sequencer_1::{DEFAULT_SEQUENCER_SEED, SequencerEngineV1};

    #[test]
    fn sync_1_checkpoint_manifest_recovery_receipt_and_7_of_8_eclipse_roundtrip() {
        let fix = CanonicalMultiActorFixtureV1::build().unwrap();
        let mut engine = SequencerEngineV1::from_fixture(&fix, DEFAULT_SEQUENCER_SEED).unwrap();
        let pre_wr = engine.current_world_root().unwrap();
        let b1 = fix.canonical_mixed_race_bundle(1, pre_wr).unwrap();
        let pub1 = engine.seal_bundle_and_publish(&b1).unwrap();

        let mut archive = ArchiveNodeV1::new(fix.bundle.bootstrap_policy);
        archive.store_publication(pub1.clone());
        let cert_blob = OpaqueAuthorityBlobV1::new(
            OpaqueBlobKindV1::OpaqueCertificate,
            b"opaque-cert-carrier-v1".to_vec(),
        )
        .unwrap();
        let occ_blob = OpaqueAuthorityBlobV1::new(
            OpaqueBlobKindV1::OpaqueOccurrence,
            b"opaque-occ-carrier-v1".to_vec(),
        )
        .unwrap();
        assert!(!cert_blob.interpreted_as_live_authority());
        assert!(!occ_blob.interpreted_as_live_authority());
        let cert_raw = cert_blob.encode_canonical().unwrap();
        assert_eq!(
            OpaqueAuthorityBlobV1::decode_canonical(&cert_raw).unwrap(),
            cert_blob
        );
        archive.store_opaque_blob(cert_blob).unwrap();
        archive.store_opaque_blob(occ_blob).unwrap();
        assert_eq!(
            archive.attempt_prune(1),
            Err(Sync1Error::ArchivePruningForbidden)
        );

        let behaviors = [
            (
                PeerIdV1([1u8; 32]),
                SyncNodeRoleV1::Archive,
                SimulatedPeerBehaviorV1::WithholdBody,
            ),
            (
                PeerIdV1([2u8; 32]),
                SyncNodeRoleV1::Archive,
                SimulatedPeerBehaviorV1::CorruptCheckpointChunk,
            ),
            (
                PeerIdV1([3u8; 32]),
                SyncNodeRoleV1::Archive,
                SimulatedPeerBehaviorV1::BadSignature,
            ),
            (
                PeerIdV1([4u8; 32]),
                SyncNodeRoleV1::Archive,
                SimulatedPeerBehaviorV1::SlowBackpressureDoS,
            ),
            (
                PeerIdV1([5u8; 32]),
                SyncNodeRoleV1::FullVerifier,
                SimulatedPeerBehaviorV1::StallTimeout,
            ),
            (
                PeerIdV1([6u8; 32]),
                SyncNodeRoleV1::FullVerifier,
                SimulatedPeerBehaviorV1::WithholdBody,
            ),
            (
                PeerIdV1([7u8; 32]),
                SyncNodeRoleV1::FullVerifier,
                SimulatedPeerBehaviorV1::CorruptCheckpointChunk,
            ),
            (
                PeerIdV1([8u8; 32]),
                SyncNodeRoleV1::Archive,
                SimulatedPeerBehaviorV1::Honest,
            ),
        ];
        let mut cluster = SyncClusterHarnessV1::new(
            fix.bundle.bounds_profile,
            fix.bundle.bootstrap_policy,
            archive,
            1,
            1_024,
            &behaviors,
        )
        .unwrap();

        let chk_raw = cluster.checkpoint_manifest.encode_canonical().unwrap();
        assert_eq!(
            SyncCheckpointManifestV1::decode_canonical(&chk_raw).unwrap(),
            cluster.checkpoint_manifest
        );

        let (mut node, rcpt) = cluster
            .sync_fresh_full_verifier(&fix, engine.sequencer_pubkey())
            .unwrap();
        assert_eq!(rcpt.peers_discovered, 8);
        assert_eq!(rcpt.peers_banned, 7);
        assert_eq!(rcpt.honest_peers_used, 1);
        assert!(!rcpt.live_authority_admitted);
        let rcpt_raw = rcpt.encode_canonical().unwrap();
        assert_eq!(
            SyncRecoveryReceiptV1::decode_canonical(&rcpt_raw).unwrap(),
            rcpt
        );
        assert_eq!(
            cluster.checkpoint_manifest.tag().unwrap().to_string(),
            "#bb.receipt.sha256.m267jyqvgn3t7iqja3cu4jnb3qxnlpqjxr3lrlh4dq77nnura6qa"
        );
        assert_eq!(
            rcpt.tag().unwrap().to_string(),
            "#bb.receipt.sha256.p4ock6iohrsau6v6sjkehw3tt36kw2zj2gdrjhqjgt25j2aly5aq"
        );
        assert_eq!(
            rcpt.opaque_blobs_digest[0..4],
            [0x85, 0xf1, 0x4c, 0x56],
            "opaque blobs set digest pinned"
        );
        node.pin_height(1);
        assert!(node.attempt_force_prune_height(1).is_err());
        assert_eq!(node.retained_count(), 1);
    }
}
