//! `SEQUENCER-1`: accountable centralized multiplayer engine with `BOUNDS-1`-preflighted wire
//! framing, RFC 8032 Ed25519-signed batch headers, `PROTOCOL-ID-1` proposal-core binding,
//! `O(log n)` receipt Merkle inclusion proofs, atomic body-before-root publication over frozen
//! `STORAGE-1` (`StorageChain` + native exclusive-locked `Journal`), independent client replay
//! verification, equivocation detection, and native `std::net` loopback TCP resynchronization
//! (`PS-033`).
//!
//! Realizes every normative gate in `docs/CAPABILITY_TREE.md` §`SEQUENCER-1` without modifying
//! `crates/core/**`, `storage.rs`, `journal.rs`, `commit_state.rs`, or `Cargo.lock`:
//! - **Bounded wire framing (`SequencerWireFrameV1`):** fixed 13-byte header (`BBSEQ1`) checked
//!   against `ProtocolBoundsProfileV1` (`max_tx_bytes = 16 KiB`, `max_block_bytes = 2 MiB`,
//!   `max_peer_inflight_bytes = 4 MiB`) **before any payload allocation or decode**.
//! - **Signed accountable batch headers (`SequencerBatchHeaderV1`):** binds height, epoch,
//!   parent header digest, fork context, body digest, receipt Merkle root, `PROTOCOL-ID-1`
//!   `CanonicalAcceptedProposalCoreV1` digest, 11-slot `ExecutionRootRefV1`, and strict RFC 8032
//!   Ed25519 signature (`SigningDomainTagV1::BlockProposal`).
//! - **Atomic body-before-root publication (`SequencerLogV1` & `DurableSequencerStoreV1`):**
//!   writes and syncs the complete `SequencerBatchPublicationV1` body+receipt `StorageRecord`
//!   BEFORE appending the parent-linked `SequencerBatchHeaderV1` `StorageRecord`, preventing
//!   orphan root publication and recovering cleanly from torn tails via `Journal`.
//! - **Independent replaying clients (`SequencerClientVerifierV1`):** refuse withheld-body
//!   headers, detect same-height equivocation fail-closed, replay every batch body through
//!   `MultiActorHarnessV1` / `PrimaryLaunchExecutorV1` + `EXECUTION-ROOT-1`, and prove exact
//!   `ExecutionRootRefV1` & `ReceiptMerkleProofV1` agreement.
//!
//! Non-claim: the sequencer can still censor, reorder, halt, equivocate, lose data, or present
//! different histories. Accountability is not decentralization.

use std::collections::BTreeMap;
use std::fmt;

use sha2::{Digest, Sha256};

use crate::bounds_1::{BlockEnvelopePreflightHeaderV1, ProtocolBoundsProfileV1};
use crate::crypto_d1::{
    CanonicalEd25519PublicKeyV1, CanonicalEd25519SignatureV1, HashSuiteIdV1, SignatureSuiteIdV1,
    SigningDomainTagV1, SigningPreimageV1, sign_preimage_deterministic, verify_preimage_strict,
};
use crate::execution_root_1::ExecutionRootRefV1;
use crate::executor_1::ExecutorForkContextV1;
use crate::multi_1::{
    CanonicalMultiActorFixtureV1, MULTI_1_VERSION, Multi1Error, MultiActorFoldReceiptV1,
    MultiActorFoldResultV1, MultiActorHarnessV1, MultiActorStepOutcomeV1, MultiActorStreamBundleV1,
    SignedActorTxEnvelopeV1,
};
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::protocol_id_1::{
    CanonicalAcceptedProposalCoreV1, ProtocolId1Error, profile_id_hash_parent_lookup,
};
use crate::storage::{StorageChain, StorageError, StorageLimits, StorageRecord};

/// Canonical schema string for a signed [`SequencerBatchHeaderV1`].
pub const SEQUENCER_1_BATCH_HEADER_SCHEMA: &[u8] = b"bitblock/sequencer-1/signed-batch-header/v1";
/// Canonical schema string for an atomic [`SequencerBatchPublicationV1`] bundle.
pub const SEQUENCER_1_PUBLICATION_BUNDLE_SCHEMA: &[u8] =
    b"bitblock/sequencer-1/batch-publication/v1";

/// Protocol version encoded in all `SEQUENCER-1` headers, publications, and wire frames.
pub const SEQUENCER_1_VERSION: u16 = 1;
/// Fixed 6-byte magic header for [`SequencerWireFrameV1`].
pub const SEQ_WIRE_MAGIC: [u8; 6] = *b"BBSEQ1";
/// Fixed byte length of a [`SequencerWireFrameV1`] preflight header (`13` bytes).
pub const SEQ_WIRE_HEADER_LEN: usize = 6 + 2 + 1 + 4;
/// Default deterministic Ed25519 secret seed used by canonical devnet sequencer fixtures.
pub const DEFAULT_SEQUENCER_SEED: [u8; 32] = [77u8; 32];

const SEQ_HDR_PAYLOAD_PREFIX: &[u8] = b"bb.seq1.hdr_payload.v1";
const SEQ_HDR_DIGEST_PREFIX: &[u8] = b"bb.seq1.hdr_digest.v1";
const SEQ_BODY_DIGEST_PREFIX: &[u8] = b"bb.seq1.body_digest.v1";
const SEQ_PARENT_LOOKUP_PREFIX: &[u8] = b"bb.seq1.parent_lookup.v1";
const SEQ_RCPT_LEAF_PREFIX: &[u8] = b"bb.seq1.rcpt_leaf.v1";
const SEQ_RCPT_NODE_PREFIX: &[u8] = b"bb.seq1.rcpt_node.v1";
const SEQ_RCPT_EMPTY_PREFIX: &[u8] = b"bb.seq1.rcpt_empty.v1";

/// Errors returned by `SEQUENCER-1` framing, publication, verification, storage, or transport.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sequencer1Error {
    /// Underlying `MULTI-1` execution or fixture failure.
    Multi(Multi1Error),
    /// Underlying `PROTOCOL-ID-1` proposal-core validation failure.
    ProtocolId(ProtocolId1Error),
    /// Underlying `STORAGE-1` record/chain failure.
    Storage(StorageError),
    /// Pre-allocation `BOUNDS-1` wire or batch budget violation.
    BoundsExceeded(&'static str),
    /// Malformed, truncated, unknown-magic, or trailing-byte canonical encoding.
    InvalidWire(&'static str),
    /// Invalid or mismatched RFC 8032 sequencer signature or public key.
    InvalidSequencerSignature,
    /// Header advertised without its complete body & receipt bundle (withheld-body refusal).
    WithheldBodyRefused,
    /// Two conflicting signed headers were observed for the exact same batch height.
    EquivocationDetected {
        /// Conflicting height.
        height: u64,
        /// Previously accepted header digest.
        existing_header_digest: [u8; 32],
        /// Conflicting newly observed header digest.
        conflicting_header_digest: [u8; 32],
    },
    /// Batch height, parent link, or fork context disagreed with client state.
    ChainContinuityError(&'static str),
    /// Independent client replay disagreed with the published fold receipt or execution root.
    ReplayMismatch(&'static str),
    /// Receipt Merkle inclusion proof failed verification against the signed header root.
    InvalidReceiptProof(&'static str),
    /// Native disk journal or loopback TCP I/O error.
    Io(String),
}

impl From<Multi1Error> for Sequencer1Error {
    fn from(v: Multi1Error) -> Self {
        Self::Multi(v)
    }
}

impl From<ProtocolId1Error> for Sequencer1Error {
    fn from(v: ProtocolId1Error) -> Self {
        Self::ProtocolId(v)
    }
}

impl From<StorageError> for Sequencer1Error {
    fn from(v: StorageError) -> Self {
        Self::Storage(v)
    }
}

impl fmt::Display for Sequencer1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Sequencer1Error {}

/// Canonical message-kind discriminant for [`SequencerWireFrameV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SequencerWireMsgKindV1 {
    /// Remote client submitting one 299-byte [`SignedActorTxEnvelopeV1`].
    SubmitEnvelope = 1,
    /// Remote client requesting all committed batches starting at an inclusive height (`8` bytes).
    RequestBatchRange = 2,
    /// Sequencer transmitting one atomic [`SequencerBatchPublicationV1`].
    BatchPublication = 3,
    /// Remote client requesting a [`ReceiptMerkleProofV1`] for `(height, leaf_index)` (`12` bytes).
    RequestReceiptProof = 4,
    /// Sequencer transmitting one encoded [`ReceiptMerkleProofV1`].
    ReceiptProofResponse = 5,
    /// Bounded client acknowledgement carrying verified tip height and header digest (`40` bytes).
    ClientAck = 6,
}

impl SequencerWireMsgKindV1 {
    /// Returns the 1-byte canonical wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a 1-byte canonical wire discriminant fail-closed.
    pub const fn from_code(code: u8) -> Result<Self, Sequencer1Error> {
        match code {
            1 => Ok(Self::SubmitEnvelope),
            2 => Ok(Self::RequestBatchRange),
            3 => Ok(Self::BatchPublication),
            4 => Ok(Self::RequestReceiptProof),
            5 => Ok(Self::ReceiptProofResponse),
            6 => Ok(Self::ClientAck),
            _ => Err(Sequencer1Error::InvalidWire("unknown wire msg kind")),
        }
    }
}

/// Bounded streaming wire frame checked strictly against `BOUNDS-1` before payload allocation.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SequencerWireFrameV1 {
    /// Wire protocol version (`1`).
    pub version: u16,
    /// Message kind tag.
    pub msg_kind: SequencerWireMsgKindV1,
    /// Bounded canonical payload bytes.
    pub payload: Vec<u8>,
}

impl SequencerWireFrameV1 {
    /// Zero-allocation preflight check on a raw 13-byte frame header before allocating payload RAM.
    pub fn preflight_header(
        header: &[u8; SEQ_WIRE_HEADER_LEN],
        bounds: &ProtocolBoundsProfileV1,
    ) -> Result<(SequencerWireMsgKindV1, usize), Sequencer1Error> {
        if header[0..6] != SEQ_WIRE_MAGIC {
            return Err(Sequencer1Error::InvalidWire("bad BBSEQ1 wire magic"));
        }
        let ver = u16::from_be_bytes([header[6], header[7]]);
        if ver != SEQUENCER_1_VERSION {
            return Err(Sequencer1Error::InvalidWire("unsupported wire version"));
        }
        let kind = SequencerWireMsgKindV1::from_code(header[8])?;
        let payload_len =
            u32::from_be_bytes([header[9], header[10], header[11], header[12]]) as usize;
        let max_payload = match kind {
            SequencerWireMsgKindV1::SubmitEnvelope => bounds.max_tx_bytes as usize,
            SequencerWireMsgKindV1::RequestBatchRange => 8,
            SequencerWireMsgKindV1::RequestReceiptProof => 12,
            SequencerWireMsgKindV1::ClientAck => 40,
            SequencerWireMsgKindV1::ReceiptProofResponse => bounds.max_proof_bytes as usize,
            SequencerWireMsgKindV1::BatchPublication => bounds.max_block_bytes as usize,
        };
        if payload_len > max_payload {
            return Err(Sequencer1Error::BoundsExceeded(
                "wire frame payload exceeds BOUNDS-1 limit",
            ));
        }
        let Some(total_len) = (SEQ_WIRE_HEADER_LEN as u64).checked_add(payload_len as u64) else {
            return Err(Sequencer1Error::BoundsExceeded("wire frame size overflow"));
        };
        if total_len > u64::from(bounds.max_peer_inflight_bytes) {
            return Err(Sequencer1Error::BoundsExceeded(
                "wire frame exceeds max_peer_inflight_bytes",
            ));
        }
        Ok((kind, payload_len))
    }

    /// Encodes a bounded frame after checking `BOUNDS-1` limits.
    pub fn encode_canonical(
        &self,
        bounds: &ProtocolBoundsProfileV1,
    ) -> Result<Vec<u8>, Sequencer1Error> {
        if self.version != SEQUENCER_1_VERSION {
            return Err(Sequencer1Error::InvalidWire("bad wire frame version"));
        }
        let plen = u32::try_from(self.payload.len())
            .map_err(|_| Sequencer1Error::BoundsExceeded("payload len exceeds u32"))?;
        let mut hdr = [0u8; SEQ_WIRE_HEADER_LEN];
        hdr[0..6].copy_from_slice(&SEQ_WIRE_MAGIC);
        hdr[6..8].copy_from_slice(&self.version.to_be_bytes());
        hdr[8] = self.msg_kind.code();
        hdr[9..13].copy_from_slice(&plen.to_be_bytes());
        let _ = Self::preflight_header(&hdr, bounds)?;
        let mut out = Vec::with_capacity(SEQ_WIRE_HEADER_LEN + self.payload.len());
        out.extend_from_slice(&hdr);
        out.extend_from_slice(&self.payload);
        Ok(out)
    }

    /// Decodes one bounded wire frame fail-closed (preflights header before reading payload and
    /// rejects any truncation or trailing bytes).
    pub fn decode_canonical(
        bytes: &[u8],
        bounds: &ProtocolBoundsProfileV1,
    ) -> Result<Self, Sequencer1Error> {
        if bytes.len() < SEQ_WIRE_HEADER_LEN {
            return Err(Sequencer1Error::InvalidWire("truncated wire frame header"));
        }
        let hdr: [u8; SEQ_WIRE_HEADER_LEN] = bytes[0..SEQ_WIRE_HEADER_LEN]
            .try_into()
            .expect("13-byte header");
        let (msg_kind, payload_len) = Self::preflight_header(&hdr, bounds)?;
        let expected_len = SEQ_WIRE_HEADER_LEN
            .checked_add(payload_len)
            .ok_or(Sequencer1Error::BoundsExceeded("frame length overflow"))?;
        if bytes.len() != expected_len {
            return Err(Sequencer1Error::InvalidWire(
                "wire frame truncated or has trailing bytes",
            ));
        }
        Ok(Self {
            version: SEQUENCER_1_VERSION,
            msg_kind,
            payload: bytes[SEQ_WIRE_HEADER_LEN..expected_len].to_vec(),
        })
    }

    /// Encodes a canonical 8-byte `RequestBatchRange` start-height payload.
    pub fn encode_range_request(from_height: u64) -> [u8; 8] {
        from_height.to_be_bytes()
    }

    /// Decodes a canonical 8-byte `RequestBatchRange` start-height payload.
    pub fn decode_range_request(payload: &[u8]) -> Result<u64, Sequencer1Error> {
        let arr: [u8; 8] = payload
            .try_into()
            .map_err(|_| Sequencer1Error::InvalidWire("bad range request len"))?;
        Ok(u64::from_be_bytes(arr))
    }
}

/// Computes the domain-separated SHA-256 leaf digest for one ordered [`MultiActorStepOutcomeV1`].
pub fn receipt_merkle_leaf_digest(leaf_index: u32, outcome: &MultiActorStepOutcomeV1) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(SEQ_RCPT_LEAF_PREFIX);
    h.update(leaf_index.to_be_bytes());
    h.update(outcome.encode_canonical());
    h.finalize().into()
}

fn hash_merkle_pair(left: [u8; 32], right: [u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(SEQ_RCPT_NODE_PREFIX);
    h.update(left);
    h.update(right);
    h.finalize().into()
}

/// Computes the domain-separated binary SHA-256 Merkle root over `outcomes`.
pub fn compute_receipts_merkle_root(outcomes: &[MultiActorStepOutcomeV1]) -> [u8; 32] {
    if outcomes.is_empty() {
        let mut h = Sha256::new();
        h.update(SEQ_RCPT_EMPTY_PREFIX);
        return h.finalize().into();
    }
    let mut level: Vec<[u8; 32]> = outcomes
        .iter()
        .enumerate()
        .map(|(i, o)| receipt_merkle_leaf_digest(i as u32, o))
        .collect();
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut idx = 0usize;
        while idx < level.len() {
            let left = level[idx];
            let right = if idx + 1 < level.len() {
                level[idx + 1]
            } else {
                left
            };
            next.push(hash_merkle_pair(left, right));
            idx += 2;
        }
        level = next;
    }
    level[0]
}

/// `O(log n)` inclusion proof binding one [`MultiActorStepOutcomeV1`] to
/// `SequencerBatchHeaderV1::receipts_merkle_root`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ReceiptMerkleProofV1 {
    /// Zero-based leaf position in canonical outcome order.
    pub leaf_index: u32,
    /// Total number of leaves in the batch outcome lane (`>= 1`).
    pub total_leaves: u32,
    /// Domain-separated SHA-256 leaf digest (`receipt_merkle_leaf_digest`).
    pub leaf_digest: [u8; 32],
    /// Bottom-up sibling digests along the binary Merkle path.
    pub siblings: Vec<[u8; 32]>,
}

impl ReceiptMerkleProofV1 {
    /// Builds a canonical inclusion proof for `leaf_index` in `outcomes`.
    pub fn build(
        outcomes: &[MultiActorStepOutcomeV1],
        leaf_index: u32,
    ) -> Result<Self, Sequencer1Error> {
        let total = u32::try_from(outcomes.len())
            .map_err(|_| Sequencer1Error::BoundsExceeded("outcome count overflow"))?;
        if total == 0 || leaf_index >= total {
            return Err(Sequencer1Error::InvalidReceiptProof(
                "leaf_index out of range",
            ));
        }
        let mut level: Vec<[u8; 32]> = outcomes
            .iter()
            .enumerate()
            .map(|(i, o)| receipt_merkle_leaf_digest(i as u32, o))
            .collect();
        let leaf_digest = level[leaf_index as usize];
        let mut idx = leaf_index as usize;
        let mut siblings = Vec::new();
        while level.len() > 1 {
            let sib_idx = if idx.is_multiple_of(2) {
                if idx + 1 < level.len() { idx + 1 } else { idx }
            } else {
                idx - 1
            };
            siblings.push(level[sib_idx]);
            let mut next = Vec::with_capacity(level.len().div_ceil(2));
            let mut k = 0usize;
            while k < level.len() {
                let left = level[k];
                let right = if k + 1 < level.len() {
                    level[k + 1]
                } else {
                    left
                };
                next.push(hash_merkle_pair(left, right));
                k += 2;
            }
            level = next;
            idx /= 2;
        }
        Ok(Self {
            leaf_index,
            total_leaves: total,
            leaf_digest,
            siblings,
        })
    }

    /// Recomputes the Merkle root from this proof and verifies exact equality with `expected_root`.
    pub fn verify_against_root(&self, expected_root: [u8; 32]) -> Result<(), Sequencer1Error> {
        if self.total_leaves == 0 || self.leaf_index >= self.total_leaves {
            return Err(Sequencer1Error::InvalidReceiptProof(
                "invalid leaf_index or total_leaves",
            ));
        }
        if self.siblings.len() > 32 {
            return Err(Sequencer1Error::InvalidReceiptProof(
                "proof depth exceeds 32",
            ));
        }
        let mut expected_depth = 0usize;
        let mut width = self.total_leaves as usize;
        while width > 1 {
            expected_depth += 1;
            width = width.div_ceil(2);
        }
        if self.siblings.len() != expected_depth {
            return Err(Sequencer1Error::InvalidReceiptProof(
                "sibling count disagrees with total_leaves",
            ));
        }
        let mut cur = self.leaf_digest;
        let mut idx = self.leaf_index as usize;
        for &sib in &self.siblings {
            cur = if idx.is_multiple_of(2) {
                hash_merkle_pair(cur, sib)
            } else {
                hash_merkle_pair(sib, cur)
            };
            idx /= 2;
        }
        if cur != expected_root {
            return Err(Sequencer1Error::InvalidReceiptProof(
                "recomputed receipts_merkle_root mismatch",
            ));
        }
        Ok(())
    }

    /// Encodes this Merkle proof into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, Sequencer1Error> {
        let sc = u16::try_from(self.siblings.len())
            .map_err(|_| Sequencer1Error::BoundsExceeded("sibling count overflow"))?;
        let mut out = Vec::with_capacity(42 + self.siblings.len() * 32);
        out.extend_from_slice(&self.leaf_index.to_be_bytes());
        out.extend_from_slice(&self.total_leaves.to_be_bytes());
        out.extend_from_slice(&self.leaf_digest);
        out.extend_from_slice(&sc.to_be_bytes());
        for s in &self.siblings {
            out.extend_from_slice(s);
        }
        Ok(out)
    }

    /// Decodes a canonical [`ReceiptMerkleProofV1`] fail-closed.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Sequencer1Error> {
        let mut r = SeqReader::new(bytes);
        let leaf_index = r.u32_be()?;
        let total_leaves = r.u32_be()?;
        let leaf_digest = r.arr32()?;
        let sc = r.u16_be()? as usize;
        if sc > 32 {
            return Err(Sequencer1Error::InvalidWire("excessive merkle siblings"));
        }
        let mut siblings = Vec::with_capacity(sc);
        for _ in 0..sc {
            siblings.push(r.arr32()?);
        }
        r.finish()?;
        Ok(Self {
            leaf_index,
            total_leaves,
            leaf_digest,
            siblings,
        })
    }
}

/// Computes the canonical `batch_body_digest` committing both raw stream bundle and fold receipt.
pub fn compute_batch_body_digest(bundle_bytes: &[u8], receipt_bytes: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(SEQ_BODY_DIGEST_PREFIX);
    h.update((bundle_bytes.len() as u32).to_be_bytes());
    h.update(bundle_bytes);
    h.update((receipt_bytes.len() as u32).to_be_bytes());
    h.update(receipt_bytes);
    h.finalize().into()
}

/// Builds the exact non-circular `PROTOCOL-ID-1` [`CanonicalAcceptedProposalCoreV1`] bound inside
/// [`SequencerBatchHeaderV1::proposal_core_digest`].
#[allow(clippy::too_many_arguments)]
pub fn build_sequencer_proposal_core(
    genesis_id: [u8; 32],
    profile_id: [u8; 32],
    parent_header_digest: [u8; 32],
    batch_height: u64,
    epoch: u64,
    receipts_merkle_root: [u8; 32],
    execution_root_ref: &ExecutionRootRefV1,
    batch_body_digest: [u8; 32],
) -> Result<CanonicalAcceptedProposalCoreV1, Sequencer1Error> {
    let mut lookup_pre = Vec::with_capacity(SEQ_PARENT_LOOKUP_PREFIX.len() + 64);
    lookup_pre.extend_from_slice(SEQ_PARENT_LOOKUP_PREFIX);
    lookup_pre.extend_from_slice(&genesis_id);
    lookup_pre.extend_from_slice(&parent_header_digest);
    let accepted_parent_lookup_id = profile_id_hash_parent_lookup(&lookup_pre);

    let mut auth_ref = Vec::with_capacity(64);
    auth_ref.extend_from_slice(&genesis_id);
    auth_ref.extend_from_slice(&profile_id);

    let mut hdr_bytes = Vec::with_capacity(8 + 8 + 32 + ExecutionRootRefV1::WIRE_LEN);
    hdr_bytes.extend_from_slice(&batch_height.to_be_bytes());
    hdr_bytes.extend_from_slice(&epoch.to_be_bytes());
    hdr_bytes.extend_from_slice(&receipts_merkle_root);
    hdr_bytes.extend_from_slice(&execution_root_ref.encode_canonical());

    let core = CanonicalAcceptedProposalCoreV1 {
        accepted_proposal_core_version: 1,
        accepted_parent_lookup_id,
        authority_ref_kind: 1,
        canonical_authority_ref_bytes: auth_ref,
        canonical_proposal_header_bytes: hdr_bytes,
        canonical_availability_payload_bytes: batch_body_digest.to_vec(),
    };
    core.validate()?;
    Ok(core)
}

/// Signed accountable batch header (`bitblock/sequencer-1/signed-batch-header/v1`, `532` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SequencerBatchHeaderV1 {
    /// Header schema version (`1`).
    pub header_version: u16,
    /// Strictly sequential batch height (`>= 1`).
    pub batch_height: u64,
    /// Active protocol epoch.
    pub epoch: u64,
    /// Previous committed header digest (`[0u8; 32]` at height `1`).
    pub parent_header_digest: [u8; 32],
    /// Governing execution fork context (`112` bytes).
    pub fork_context: ExecutorForkContextV1,
    /// SHA-256 commitment over the complete stream bundle and fold receipt bytes.
    pub batch_body_digest: [u8; 32],
    /// Binary SHA-256 Merkle root over `fold_receipt.step_outcomes`.
    pub receipts_merkle_root: [u8; 32],
    /// Digest of the signature-free `PROTOCOL-ID-1` [`CanonicalAcceptedProposalCoreV1`].
    pub proposal_core_digest: [u8; 32],
    /// Post-batch 11-slot composed [`ExecutionRootRefV1`] (`178` bytes).
    pub execution_root_ref: ExecutionRootRefV1,
    /// Canonical 32-byte Ed25519 public key of the centralized sequencer.
    pub sequencer_pubkey: [u8; 32],
    /// Strict RFC 8032 64-byte Ed25519 signature over `signing_preimage()`.
    pub sequencer_signature: [u8; 64],
}

impl SequencerBatchHeaderV1 {
    /// Fixed canonical big-endian wire byte length (`532` bytes).
    pub const WIRE_LEN: usize = 2
        + 8
        + 8
        + 32
        + ExecutorForkContextV1::WIRE_LEN
        + 32
        + 32
        + 32
        + ExecutionRootRefV1::WIRE_LEN
        + 32
        + 64;

    /// Computes the signature-free SHA-256 payload digest bound inside [`SigningPreimageV1`].
    pub fn unsigned_payload_digest(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(SEQ_HDR_PAYLOAD_PREFIX);
        h.update(self.header_version.to_be_bytes());
        h.update(self.batch_height.to_be_bytes());
        h.update(self.epoch.to_be_bytes());
        h.update(self.parent_header_digest);
        h.update(self.fork_context.encode_canonical());
        h.update(self.batch_body_digest);
        h.update(self.receipts_merkle_root);
        h.update(self.proposal_core_digest);
        h.update(self.execution_root_ref.encode_canonical());
        h.finalize().into()
    }

    /// Builds the domain-separated [`SigningPreimageV1`] (`SigningDomainTagV1::BlockProposal`).
    pub fn signing_preimage(&self) -> SigningPreimageV1 {
        SigningPreimageV1 {
            protocol_version: 1,
            chain_id: self.fork_context.chain_id,
            genesis_id: self.fork_context.genesis_id,
            launch_manifest_id: self.fork_context.genesis_id,
            profile_id: self.fork_context.profile_id,
            domain_tag: SigningDomainTagV1::BlockProposal,
            signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
            hash_suite: HashSuiteIdV1::Sha256V1,
            payload_digest: self.unsigned_payload_digest(),
        }
    }

    /// Signs this header deterministically with `sequencer_seed` under RFC 8032.
    pub fn sign_with_seed(mut self, sequencer_seed: &[u8; 32]) -> Result<Self, Sequencer1Error> {
        let pk = CanonicalEd25519PublicKeyV1::from_secret_seed(sequencer_seed)
            .map_err(|_| Sequencer1Error::InvalidSequencerSignature)?;
        let sig = sign_preimage_deterministic(sequencer_seed, &self.signing_preimage())
            .map_err(|_| Sequencer1Error::InvalidSequencerSignature)?;
        self.sequencer_pubkey = *pk.as_bytes();
        self.sequencer_signature = *sig.as_bytes();
        Ok(self)
    }

    /// Verifies the header signature strictly against `expected_sequencer_pk`.
    pub fn verify_signature(
        &self,
        expected_sequencer_pk: &CanonicalEd25519PublicKeyV1,
    ) -> Result<(), Sequencer1Error> {
        if self.sequencer_pubkey != *expected_sequencer_pk.as_bytes() {
            return Err(Sequencer1Error::InvalidSequencerSignature);
        }
        let sig = CanonicalEd25519SignatureV1::from_bytes(self.sequencer_signature)
            .map_err(|_| Sequencer1Error::InvalidSequencerSignature)?;
        verify_preimage_strict(expected_sequencer_pk, &self.signing_preimage(), &sig)
            .map_err(|_| Sequencer1Error::InvalidSequencerSignature)
    }

    /// Encodes this header into 532 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], Sequencer1Error> {
        if self.header_version != SEQUENCER_1_VERSION || self.batch_height == 0 {
            return Err(Sequencer1Error::InvalidWire("bad header version or height"));
        }
        let mut out = Vec::with_capacity(Self::WIRE_LEN);
        out.extend_from_slice(&self.header_version.to_be_bytes());
        out.extend_from_slice(&self.batch_height.to_be_bytes());
        out.extend_from_slice(&self.epoch.to_be_bytes());
        out.extend_from_slice(&self.parent_header_digest);
        out.extend_from_slice(&self.fork_context.encode_canonical());
        out.extend_from_slice(&self.batch_body_digest);
        out.extend_from_slice(&self.receipts_merkle_root);
        out.extend_from_slice(&self.proposal_core_digest);
        out.extend_from_slice(&self.execution_root_ref.encode_canonical());
        out.extend_from_slice(&self.sequencer_pubkey);
        out.extend_from_slice(&self.sequencer_signature);
        Ok(out.try_into().expect("532-byte SequencerBatchHeaderV1"))
    }

    /// Strictly decodes 532 canonical big-endian wire bytes into a [`SequencerBatchHeaderV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Sequencer1Error> {
        let mut r = SeqReader::new(bytes);
        let header_version = r.u16_be()?;
        let batch_height = r.u64_be()?;
        if header_version != SEQUENCER_1_VERSION || batch_height == 0 {
            return Err(Sequencer1Error::InvalidWire("bad header version or height"));
        }
        let epoch = r.u64_be()?;
        let parent_header_digest = r.arr32()?;
        let fork_context =
            ExecutorForkContextV1::decode_canonical(r.take(ExecutorForkContextV1::WIRE_LEN)?)
                .map_err(|_| Sequencer1Error::InvalidWire("bad fork_context"))?;
        let batch_body_digest = r.arr32()?;
        let receipts_merkle_root = r.arr32()?;
        let proposal_core_digest = r.arr32()?;
        let execution_root_ref =
            ExecutionRootRefV1::decode_canonical(r.take(ExecutionRootRefV1::WIRE_LEN)?)
                .map_err(|_| Sequencer1Error::InvalidWire("bad execution_root_ref"))?;
        let sequencer_pubkey = r.arr32()?;
        let sequencer_signature = r.arr64()?;
        r.finish()?;
        Ok(Self {
            header_version,
            batch_height,
            epoch,
            parent_header_digest,
            fork_context,
            batch_body_digest,
            receipts_merkle_root,
            proposal_core_digest,
            execution_root_ref,
            sequencer_pubkey,
            sequencer_signature,
        })
    }

    /// Computes the canonical 32-byte header link digest used by child headers and equivocation maps.
    pub fn header_digest(&self) -> Result<[u8; 32], Sequencer1Error> {
        let raw = self.encode_canonical()?;
        let mut h = Sha256::new();
        h.update(SEQ_HDR_DIGEST_PREFIX);
        h.update(raw);
        Ok(h.finalize().into())
    }

    /// Computes the canonical `Receipt` `ContentTag` (`bitblock/sequencer-1/signed-batch-header/v1`).
    pub fn tag(&self) -> Result<ContentTag, Sequencer1Error> {
        let raw = self.encode_canonical()?;
        hash_canonical(
            TagKind::Receipt,
            SEQUENCER_1_BATCH_HEADER_SCHEMA,
            raw.len() as u64,
            |w| w(&raw),
        )
        .map_err(|_| Sequencer1Error::InvalidWire("header hash_canonical failed"))
    }
}

/// Complete atomic batch publication bundle (`bitblock/sequencer-1/batch-publication/v1`)
/// pairing the signed header with its full signed transaction stream and fold receipt.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SequencerBatchPublicationV1 {
    /// Publication schema version (`1`).
    pub pub_version: u16,
    /// Signed batch header.
    pub header: SequencerBatchHeaderV1,
    /// Complete raw arrival-order signed transaction bundle.
    pub stream_bundle: MultiActorStreamBundleV1,
    /// Complete arrival-order-invariant multi-actor fold receipt.
    pub fold_receipt: MultiActorFoldReceiptV1,
}

impl SequencerBatchPublicationV1 {
    /// Encodes this publication bundle into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, Sequencer1Error> {
        if self.pub_version != SEQUENCER_1_VERSION {
            return Err(Sequencer1Error::InvalidWire("bad pub_version"));
        }
        let hdr_bytes = self.header.encode_canonical()?;
        let bundle_bytes = self.stream_bundle.encode_canonical()?;
        let rcpt_bytes = self.fold_receipt.encode_canonical()?;
        let blen = u32::try_from(bundle_bytes.len())
            .map_err(|_| Sequencer1Error::BoundsExceeded("bundle bytes exceed u32"))?;
        let rlen = u32::try_from(rcpt_bytes.len())
            .map_err(|_| Sequencer1Error::BoundsExceeded("receipt bytes exceed u32"))?;
        let mut out = Vec::with_capacity(
            2 + SequencerBatchHeaderV1::WIRE_LEN + 8 + bundle_bytes.len() + rcpt_bytes.len(),
        );
        out.extend_from_slice(&self.pub_version.to_be_bytes());
        out.extend_from_slice(&hdr_bytes);
        out.extend_from_slice(&blen.to_be_bytes());
        out.extend_from_slice(&bundle_bytes);
        out.extend_from_slice(&rlen.to_be_bytes());
        out.extend_from_slice(&rcpt_bytes);
        Ok(out)
    }

    /// Decodes a canonical [`SequencerBatchPublicationV1`] under `bounds` fail-closed.
    pub fn decode_canonical(
        bytes: &[u8],
        bounds: &ProtocolBoundsProfileV1,
    ) -> Result<Self, Sequencer1Error> {
        if bytes.len() > bounds.max_block_bytes as usize {
            return Err(Sequencer1Error::BoundsExceeded(
                "publication bytes exceed max_block_bytes",
            ));
        }
        let mut r = SeqReader::new(bytes);
        let pub_version = r.u16_be()?;
        if pub_version != SEQUENCER_1_VERSION {
            return Err(Sequencer1Error::InvalidWire("bad pub_version"));
        }
        let header =
            SequencerBatchHeaderV1::decode_canonical(r.take(SequencerBatchHeaderV1::WIRE_LEN)?)?;
        let blen = r.u32_be()? as usize;
        let bundle_slice = r.take(blen)?;
        let rlen = r.u32_be()? as usize;
        let rcpt_slice = r.take(rlen)?;
        r.finish()?;
        let stream_bundle = MultiActorStreamBundleV1::decode_canonical(bundle_slice)?;
        let fold_receipt = MultiActorFoldReceiptV1::decode_canonical(rcpt_slice)?;
        Ok(Self {
            pub_version,
            header,
            stream_bundle,
            fold_receipt,
        })
    }

    /// Computes the canonical `Receipt` `ContentTag` (`bitblock/sequencer-1/batch-publication/v1`).
    pub fn tag(&self) -> Result<ContentTag, Sequencer1Error> {
        let raw = self.encode_canonical()?;
        hash_canonical(
            TagKind::Receipt,
            SEQUENCER_1_PUBLICATION_BUNDLE_SCHEMA,
            raw.len() as u64,
            |w| w(&raw),
        )
        .map_err(|_| Sequencer1Error::InvalidWire("publication hash_canonical failed"))
    }

    /// Verifies `BOUNDS-1` preflight, sequencer Ed25519 signature, body digest, receipt Merkle
    /// root, `PROTOCOL-ID-1` proposal core binding, and header/body metadata alignment.
    pub fn verify_internal_consistency(
        &self,
        expected_sequencer_pk: &CanonicalEd25519PublicKeyV1,
        bounds: &ProtocolBoundsProfileV1,
    ) -> Result<(), Sequencer1Error> {
        let encoded = self.encode_canonical()?;
        let tx_count = self.stream_bundle.envelopes.len() as u64;
        let preflight = BlockEnvelopePreflightHeaderV1 {
            block_bytes: encoded.len() as u64,
            tx_count,
            max_tx_bytes_observed: SignedActorTxEnvelopeV1::WIRE_LEN as u64,
            signature_count: tx_count.saturating_add(1),
            op_count: tx_count.saturating_mul(2),
            state_writes: tx_count.saturating_mul(2),
            max_proof_bytes_observed: 1024,
            max_proof_depth_observed: 32,
            estimated_hashes: tx_count.saturating_mul(64).saturating_add(64),
            estimated_alloc_bytes: (encoded.len() as u64).saturating_mul(4),
        };
        bounds
            .preflight_block_envelope(&preflight)
            .map_err(|_| Sequencer1Error::BoundsExceeded("batch violated BOUNDS-1 preflight"))?;

        self.header.verify_signature(expected_sequencer_pk)?;

        if self.stream_bundle.batch_height != self.header.batch_height
            || self.fold_receipt.batch_height != self.header.batch_height
            || self.stream_bundle.epoch != self.header.epoch
            || self.fold_receipt.epoch != self.header.epoch
            || self.stream_bundle.fork_context != self.header.fork_context
            || self.fold_receipt.fork_context != self.header.fork_context
        {
            return Err(Sequencer1Error::ChainContinuityError(
                "header/bundle/receipt height, epoch, or fork context mismatch",
            ));
        }

        let b_bytes = self.stream_bundle.encode_canonical()?;
        let r_bytes = self.fold_receipt.encode_canonical()?;
        if compute_batch_body_digest(&b_bytes, &r_bytes) != self.header.batch_body_digest {
            return Err(Sequencer1Error::ReplayMismatch(
                "batch_body_digest disagrees with stream_bundle + fold_receipt",
            ));
        }
        if compute_receipts_merkle_root(&self.fold_receipt.step_outcomes)
            != self.header.receipts_merkle_root
        {
            return Err(Sequencer1Error::ReplayMismatch(
                "receipts_merkle_root disagrees with fold_receipt.step_outcomes",
            ));
        }
        if self.fold_receipt.post_execution_root_ref != self.header.execution_root_ref {
            return Err(Sequencer1Error::ReplayMismatch(
                "header execution_root_ref disagrees with fold_receipt",
            ));
        }

        let core = build_sequencer_proposal_core(
            self.header.fork_context.genesis_id,
            self.header.fork_context.profile_id,
            self.header.parent_header_digest,
            self.header.batch_height,
            self.header.epoch,
            self.header.receipts_merkle_root,
            &self.header.execution_root_ref,
            self.header.batch_body_digest,
        )?;
        if core.tag()?.digest() != self.header.proposal_core_digest {
            return Err(Sequencer1Error::ReplayMismatch(
                "proposal_core_digest disagrees with PROTOCOL-ID-1 core",
            ));
        }
        Ok(())
    }

    /// Generates an inclusion proof for canonical outcome index `leaf_index` in this batch.
    pub fn build_receipt_proof(
        &self,
        leaf_index: u32,
    ) -> Result<ReceiptMerkleProofV1, Sequencer1Error> {
        ReceiptMerkleProofV1::build(&self.fold_receipt.step_outcomes, leaf_index)
    }
}

/// Portable (`wasm32`-clean) append-only `STORAGE-1` publication log enforcing atomic
/// body-before-root parent-linked record ordering (`StorageRecord[body] -> StorageRecord[header]`).
#[derive(Clone, Debug)]
pub struct SequencerLogV1 {
    chain: StorageChain,
    publications: Vec<SequencerBatchPublicationV1>,
}

impl SequencerLogV1 {
    /// Creates an empty portable publication log with `StorageLimits::DEFAULT`.
    pub fn new() -> Self {
        Self {
            chain: StorageChain::with_limits(StorageLimits::DEFAULT),
            publications: Vec::new(),
        }
    }

    /// Appends one complete publication atomically: body+receipt record FIRST, then parent-linked
    /// signed header record SECOND (preventing orphan header publication).
    pub fn append_publication_body_before_root(
        &mut self,
        publication: &SequencerBatchPublicationV1,
    ) -> Result<(ContentTag, ContentTag), Sequencer1Error> {
        let pub_bytes = publication.encode_canonical()?;
        let hdr_bytes = publication.header.encode_canonical()?;
        let mut staged = self.chain.clone();
        let body_rec_tag = staged
            .append(
                TagKind::Receipt,
                SEQUENCER_1_PUBLICATION_BUNDLE_SCHEMA,
                &pub_bytes,
            )?
            .record_tag();
        let hdr_rec_tag = staged
            .append(
                TagKind::Receipt,
                SEQUENCER_1_BATCH_HEADER_SCHEMA,
                &hdr_bytes,
            )?
            .record_tag();
        self.chain = staged;
        self.publications.push(publication.clone());
        Ok((body_rec_tag, hdr_rec_tag))
    }

    /// Verifies and reconstructs a [`SequencerLogV1`] from ordered `StorageRecord` entries,
    /// refusing any orphan header record lacking its immediately preceding body record.
    pub fn verify_from_storage_records(
        records: &[StorageRecord],
        expected_sequencer_pk: &CanonicalEd25519PublicKeyV1,
        bounds: &ProtocolBoundsProfileV1,
    ) -> Result<Vec<SequencerBatchPublicationV1>, Sequencer1Error> {
        let mut out = Vec::new();
        let mut idx = 0usize;
        let mut expected_parent = [0u8; 32];
        let mut expected_height = 1u64;
        while idx < records.len() {
            let body_rec = &records[idx];
            if body_rec.object_schema() == SEQUENCER_1_BATCH_HEADER_SCHEMA {
                return Err(Sequencer1Error::WithheldBodyRefused);
            }
            if body_rec.object_schema() != SEQUENCER_1_PUBLICATION_BUNDLE_SCHEMA {
                return Err(Sequencer1Error::InvalidWire(
                    "unexpected storage record schema in sequencer log",
                ));
            }
            if idx + 1 >= records.len() {
                // Uncommitted trailing body without synced header record is safely ignored
                // (atomic body-before-root crash boundary).
                break;
            }
            let hdr_rec = &records[idx + 1];
            if hdr_rec.object_schema() != SEQUENCER_1_BATCH_HEADER_SCHEMA
                || hdr_rec.parent() != Some(body_rec.record_tag())
            {
                return Err(Sequencer1Error::ChainContinuityError(
                    "header record not directly parent-linked to its body record",
                ));
            }
            let pub_bundle =
                SequencerBatchPublicationV1::decode_canonical(body_rec.payload(), bounds)?;
            let hdr = SequencerBatchHeaderV1::decode_canonical(hdr_rec.payload())?;
            if hdr != pub_bundle.header {
                return Err(Sequencer1Error::ReplayMismatch(
                    "header record payload disagrees with paired body publication header",
                ));
            }
            if hdr.batch_height != expected_height || hdr.parent_header_digest != expected_parent {
                return Err(Sequencer1Error::ChainContinuityError(
                    "non-sequential batch height or broken parent_header_digest",
                ));
            }
            pub_bundle.verify_internal_consistency(expected_sequencer_pk, bounds)?;
            expected_parent = hdr.header_digest()?;
            expected_height += 1;
            out.push(pub_bundle);
            idx += 2;
        }
        Ok(out)
    }

    /// Returns all committed batch publications in ascending height order.
    pub fn publications(&self) -> &[SequencerBatchPublicationV1] {
        &self.publications
    }

    /// Returns the underlying frozen `STORAGE-1` [`StorageChain`].
    pub const fn storage_chain(&self) -> &StorageChain {
        &self.chain
    }
}

impl Default for SequencerLogV1 {
    fn default() -> Self {
        Self::new()
    }
}

/// Accountable centralized multiplayer sequencer engine wrapping `MULTI-1` + `STORAGE-1`.
#[derive(Clone, Debug)]
pub struct SequencerEngineV1 {
    sequencer_seed: [u8; 32],
    sequencer_pk: CanonicalEd25519PublicKeyV1,
    bounds: ProtocolBoundsProfileV1,
    harness: MultiActorHarnessV1,
    fork_context: ExecutorForkContextV1,
    mempool: Vec<SignedActorTxEnvelopeV1>,
    seen_env_digests: BTreeMap<[u8; 32], ()>,
    last_header_digest: [u8; 32],
    log: SequencerLogV1,
}

impl SequencerEngineV1 {
    /// Initializes a sequencer engine from a `GENESIS-ROOT-PARITY-1`-verified fixture.
    pub fn from_fixture(
        fix: &CanonicalMultiActorFixtureV1,
        sequencer_seed: [u8; 32],
    ) -> Result<Self, Sequencer1Error> {
        let sequencer_pk = CanonicalEd25519PublicKeyV1::from_secret_seed(&sequencer_seed)
            .map_err(|_| Sequencer1Error::InvalidSequencerSignature)?;
        let harness = fix.into_harness()?;
        Ok(Self {
            sequencer_seed,
            sequencer_pk,
            bounds: fix.bundle.bounds_profile,
            harness,
            fork_context: fix.fork_context,
            mempool: Vec::new(),
            seen_env_digests: BTreeMap::new(),
            last_header_digest: [0u8; 32],
            log: SequencerLogV1::new(),
        })
    }

    /// Returns the sequencer's canonical Ed25519 verifying key.
    pub const fn sequencer_pubkey(&self) -> CanonicalEd25519PublicKeyV1 {
        self.sequencer_pk
    }

    /// Returns the active committed `BOUNDS-1` profile.
    pub const fn bounds(&self) -> ProtocolBoundsProfileV1 {
        self.bounds
    }

    /// Returns the current world root snapshot from the underlying harness.
    pub fn current_world_root(&self) -> Result<[u8; 32], Sequencer1Error> {
        Ok(self.harness.snapshot()?.world_root)
    }

    /// Admits one bounded `SubmitEnvelope` wire frame into the pending mempool, rejecting
    /// duplicate submissions and over-limit batches fail-closed.
    pub fn admit_wire_envelope(
        &mut self,
        frame: &SequencerWireFrameV1,
    ) -> Result<bool, Sequencer1Error> {
        if frame.msg_kind != SequencerWireMsgKindV1::SubmitEnvelope {
            return Err(Sequencer1Error::InvalidWire(
                "expected SubmitEnvelope frame",
            ));
        }
        if frame.payload.len() > self.bounds.max_tx_bytes as usize {
            return Err(Sequencer1Error::BoundsExceeded(
                "envelope exceeds max_tx_bytes",
            ));
        }
        if self.mempool.len() >= self.bounds.max_block_txs as usize {
            return Err(Sequencer1Error::BoundsExceeded(
                "mempool reached max_block_txs",
            ));
        }
        let env = SignedActorTxEnvelopeV1::decode_canonical(&frame.payload)?;
        let dig = env.envelope_digest()?;
        if self.seen_env_digests.contains_key(&dig) {
            return Ok(false);
        }
        self.seen_env_digests.insert(dig, ());
        self.mempool.push(env);
        Ok(true)
    }

    /// Seals all currently queued mempool envelopes into the next canonical batch, signs the
    /// header, and publishes body-before-root into [`SequencerLogV1`].
    pub fn seal_mempool_and_publish(
        &mut self,
        epoch: u64,
        base_fee_per_gas: u64,
    ) -> Result<SequencerBatchPublicationV1, Sequencer1Error> {
        let envs = std::mem::take(&mut self.mempool);
        let bundle = MultiActorStreamBundleV1 {
            bundle_version: MULTI_1_VERSION,
            batch_height: self.harness.batch_height() + 1,
            epoch,
            fork_context: self.fork_context,
            base_fee_per_gas,
            envelopes: envs,
        };
        self.seal_bundle_and_publish(&bundle)
    }

    /// Folds an explicit [`MultiActorStreamBundleV1`], computes its receipt Merkle root and
    /// `PROTOCOL-ID-1` proposal core, signs [`SequencerBatchHeaderV1`], and appends
    /// body-before-root to [`SequencerLogV1`].
    pub fn seal_bundle_and_publish(
        &mut self,
        bundle: &MultiActorStreamBundleV1,
    ) -> Result<SequencerBatchPublicationV1, Sequencer1Error> {
        let fold = self.harness.fold_stream_bundle(bundle)?;
        let bundle_bytes = bundle.encode_canonical()?;
        let rcpt_bytes = fold.fold_receipt.encode_canonical()?;
        let batch_body_digest = compute_batch_body_digest(&bundle_bytes, &rcpt_bytes);
        let receipts_merkle_root = compute_receipts_merkle_root(&fold.fold_receipt.step_outcomes);
        let core = build_sequencer_proposal_core(
            self.fork_context.genesis_id,
            self.fork_context.profile_id,
            self.last_header_digest,
            bundle.batch_height,
            bundle.epoch,
            receipts_merkle_root,
            &fold.post_composition.execution_root_ref,
            batch_body_digest,
        )?;
        let header = SequencerBatchHeaderV1 {
            header_version: SEQUENCER_1_VERSION,
            batch_height: bundle.batch_height,
            epoch: bundle.epoch,
            parent_header_digest: self.last_header_digest,
            fork_context: self.fork_context,
            batch_body_digest,
            receipts_merkle_root,
            proposal_core_digest: core.tag()?.digest(),
            execution_root_ref: fold.post_composition.execution_root_ref,
            sequencer_pubkey: [0u8; 32],
            sequencer_signature: [0u8; 64],
        }
        .sign_with_seed(&self.sequencer_seed)?;

        let publication = SequencerBatchPublicationV1 {
            pub_version: SEQUENCER_1_VERSION,
            header,
            stream_bundle: bundle.clone(),
            fold_receipt: fold.fold_receipt,
        };
        publication.verify_internal_consistency(&self.sequencer_pk, &self.bounds)?;
        let _ = self.log.append_publication_body_before_root(&publication)?;
        self.last_header_digest = header.header_digest()?;
        Ok(publication)
    }

    /// Returns all committed batch publications in ascending height order.
    pub fn publications(&self) -> &[SequencerBatchPublicationV1] {
        self.log.publications()
    }

    /// Returns the underlying portable [`SequencerLogV1`].
    pub const fn log(&self) -> &SequencerLogV1 {
        &self.log
    }
}

/// Independently replaying remote client that refuses withheld bodies, detects sequencer
/// equivocation, verifies `ReceiptMerkleProofV1` inclusion, and reproduces every `ExecutionRootRefV1`.
#[derive(Clone, Debug)]
pub struct SequencerClientVerifierV1 {
    expected_sequencer_pk: CanonicalEd25519PublicKeyV1,
    bounds: ProtocolBoundsProfileV1,
    harness: MultiActorHarnessV1,
    seen_headers: BTreeMap<u64, SequencerBatchHeaderV1>,
    last_header_digest: [u8; 32],
}

impl SequencerClientVerifierV1 {
    /// Boots a fresh independent verifier from a `GENESIS-ROOT-PARITY-1`-verified fixture.
    pub fn from_fixture(
        fix: &CanonicalMultiActorFixtureV1,
        expected_sequencer_pk: CanonicalEd25519PublicKeyV1,
    ) -> Result<Self, Sequencer1Error> {
        let harness = fix.into_harness()?;
        Ok(Self {
            expected_sequencer_pk,
            bounds: fix.bundle.bounds_profile,
            harness,
            seen_headers: BTreeMap::new(),
            last_header_digest: [0u8; 32],
        })
    }

    /// Refuses any header presented without its complete body & receipt bundle, while also
    /// detecting same-height sequencer equivocation immediately.
    pub fn submit_header_without_body(
        &self,
        header: &SequencerBatchHeaderV1,
    ) -> Result<(), Sequencer1Error> {
        header.verify_signature(&self.expected_sequencer_pk)?;
        let cand_dig = header.header_digest()?;
        if let Some(prev) = self.seen_headers.get(&header.batch_height) {
            let prev_dig = prev.header_digest()?;
            if prev_dig != cand_dig {
                return Err(Sequencer1Error::EquivocationDetected {
                    height: header.batch_height,
                    existing_header_digest: prev_dig,
                    conflicting_header_digest: cand_dig,
                });
            }
        }
        Err(Sequencer1Error::WithheldBodyRefused)
    }

    /// Verifies signature, bounds, chain continuity, equivocation absence, body/Merkle/core
    /// bindings, and replays the full batch independently through `MultiActorHarnessV1`,
    /// committing state atomically only when `ExecutionRootRefV1` matches bit-for-bit.
    pub fn verify_and_apply_publication(
        &mut self,
        publication: &SequencerBatchPublicationV1,
    ) -> Result<MultiActorFoldResultV1, Sequencer1Error> {
        publication
            .header
            .verify_signature(&self.expected_sequencer_pk)?;
        let hdr_dig = publication.header.header_digest()?;
        if let Some(prev) = self.seen_headers.get(&publication.header.batch_height) {
            let prev_dig = prev.header_digest()?;
            if prev_dig != hdr_dig {
                return Err(Sequencer1Error::EquivocationDetected {
                    height: publication.header.batch_height,
                    existing_header_digest: prev_dig,
                    conflicting_header_digest: hdr_dig,
                });
            }
            return Err(Sequencer1Error::ChainContinuityError(
                "duplicate already-verified batch height",
            ));
        }
        let expected_h = self.harness.batch_height() + 1;
        if publication.header.batch_height != expected_h {
            return Err(Sequencer1Error::ChainContinuityError(
                "unexpected batch height during client sync",
            ));
        }
        if publication.header.parent_header_digest != self.last_header_digest {
            return Err(Sequencer1Error::ChainContinuityError(
                "broken parent_header_digest chain link",
            ));
        }
        publication.verify_internal_consistency(&self.expected_sequencer_pk, &self.bounds)?;

        let mut staged = self.harness.clone();
        let replayed = staged.fold_stream_bundle(&publication.stream_bundle)?;
        if replayed.fold_receipt != publication.fold_receipt {
            return Err(Sequencer1Error::ReplayMismatch(
                "independent replay fold_receipt disagreed with sequencer publication",
            ));
        }
        if replayed.post_composition.execution_root_ref != publication.header.execution_root_ref {
            return Err(Sequencer1Error::ReplayMismatch(
                "independent replay ExecutionRootRefV1 disagreed with signed header",
            ));
        }

        self.harness = staged;
        self.seen_headers
            .insert(publication.header.batch_height, publication.header);
        self.last_header_digest = hdr_dig;
        Ok(replayed)
    }

    /// Verifies a [`ReceiptMerkleProofV1`] for `outcome` against the client's committed signed
    /// header at `batch_height`.
    pub fn verify_receipt_inclusion(
        &self,
        batch_height: u64,
        proof: &ReceiptMerkleProofV1,
        outcome: &MultiActorStepOutcomeV1,
    ) -> Result<(), Sequencer1Error> {
        let Some(hdr) = self.seen_headers.get(&batch_height) else {
            return Err(Sequencer1Error::ChainContinuityError(
                "batch_height not yet verified by client",
            ));
        };
        let expected_leaf = receipt_merkle_leaf_digest(proof.leaf_index, outcome);
        if proof.leaf_digest != expected_leaf {
            return Err(Sequencer1Error::InvalidReceiptProof(
                "proof leaf_digest disagrees with outcome",
            ));
        }
        proof.verify_against_root(hdr.receipts_merkle_root)
    }

    /// Returns the client's current verified batch height.
    pub const fn verified_height(&self) -> u64 {
        self.harness.batch_height()
    }

    /// Returns the client's current verified header digest (`[0u8; 32]` at genesis).
    pub const fn last_header_digest(&self) -> [u8; 32] {
        self.last_header_digest
    }

    /// Returns the verified signed header at `height`, if applied.
    pub fn verified_header(&self, height: u64) -> Option<&SequencerBatchHeaderV1> {
        self.seen_headers.get(&height)
    }
}

/// Native filesystem `STORAGE-1` durable sequencer store wrapping frozen [`crate::journal::Journal`]
/// with `sync_all` body-before-root persistence and explicit torn-tail crash recovery.
#[cfg(not(target_arch = "wasm32"))]
pub struct DurableSequencerStoreV1 {
    journal: crate::journal::Journal,
}

#[cfg(not(target_arch = "wasm32"))]
impl DurableSequencerStoreV1 {
    /// Creates a new exclusive-locked durable `STORAGE-1` journal at `path`.
    pub fn create_new<P: AsRef<std::path::Path>>(path: P) -> Result<Self, Sequencer1Error> {
        let (journal, _rcpt) = crate::journal::Journal::create_new(path, StorageLimits::DEFAULT)
            .map_err(|e| Sequencer1Error::Io(format!("journal create_new: {e:?}")))?;
        Ok(Self { journal })
    }

    /// Opens an existing durable journal, explicitly recovering any structural torn tail via
    /// `RecoveryJournal::discard_unverified_tail` and verifying every committed body-before-root
    /// publication pair against `expected_sequencer_pk` and `bounds`.
    pub fn open_and_recover<P: AsRef<std::path::Path>>(
        path: P,
        anchor: crate::journal::AnchorPolicy,
        expected_sequencer_pk: &CanonicalEd25519PublicKeyV1,
        bounds: &ProtocolBoundsProfileV1,
    ) -> Result<
        (
            Self,
            Vec<SequencerBatchPublicationV1>,
            Option<crate::journal::TailDiscardReceipt>,
        ),
        Sequencer1Error,
    > {
        let opened = crate::journal::Journal::open_existing(path, StorageLimits::DEFAULT, anchor)
            .map_err(|e| Sequencer1Error::Io(format!("journal open_existing: {e:?}")))?;
        let (journal, discard_rcpt) = match opened {
            crate::journal::OpenJournal::Ready { journal, .. } => (journal, None),
            crate::journal::OpenJournal::NeedsRecovery { mut journal } => {
                let _tail = journal
                    .export_rejected_tail()
                    .map_err(|e| Sequencer1Error::Io(format!("export_rejected_tail: {e:?}")))?;
                let (recovered, rcpt) = journal
                    .discard_unverified_tail()
                    .map_err(|e| Sequencer1Error::Io(format!("discard_unverified_tail: {e:?}")))?;
                (recovered, Some(rcpt))
            }
        };
        let pubs = SequencerLogV1::verify_from_storage_records(
            journal.records(),
            expected_sequencer_pk,
            bounds,
        )?;
        Ok((Self { journal }, pubs, discard_rcpt))
    }

    /// Durably appends and `sync_all`-verifies the complete body+receipt record FIRST, then the
    /// parent-linked signed header record SECOND.
    pub fn append_publication_synced(
        &mut self,
        publication: &SequencerBatchPublicationV1,
    ) -> Result<
        (
            crate::journal::AppendSyncReceipt,
            crate::journal::AppendSyncReceipt,
        ),
        Sequencer1Error,
    > {
        let pub_bytes = publication.encode_canonical()?;
        let hdr_bytes = publication.header.encode_canonical()?;
        let body_rcpt = self
            .journal
            .append_synced(
                TagKind::Receipt,
                SEQUENCER_1_PUBLICATION_BUNDLE_SCHEMA,
                &pub_bytes,
            )
            .map_err(|e| Sequencer1Error::Io(format!("append body synced: {e:?}")))?;
        let hdr_rcpt = self
            .journal
            .append_synced(
                TagKind::Receipt,
                SEQUENCER_1_BATCH_HEADER_SCHEMA,
                &hdr_bytes,
            )
            .map_err(|e| Sequencer1Error::Io(format!("append header synced: {e:?}")))?;
        Ok((body_rcpt, hdr_rcpt))
    }

    /// Returns the verified OS-visible byte length of the durable journal.
    pub const fn valid_bytes(&self) -> u64 {
        self.journal.valid_bytes()
    }

    /// Returns the latest committed storage record tag, if any.
    pub fn head(&self) -> Option<ContentTag> {
        self.journal.head()
    }
}

/// Native blocking `std::net` loopback TCP adapter enforcing `BOUNDS-1` streaming frame preflight.
#[cfg(not(target_arch = "wasm32"))]
pub mod net {
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpListener, TcpStream};
    use std::time::Duration;

    use super::{
        ProtocolBoundsProfileV1, SEQ_WIRE_HEADER_LEN, SEQUENCER_1_VERSION, Sequencer1Error,
        SequencerBatchPublicationV1, SequencerWireFrameV1, SequencerWireMsgKindV1,
    };

    /// Writes one `BOUNDS-1`-checked [`SequencerWireFrameV1`] to `writer`.
    pub fn write_bounded_frame<W: Write>(
        writer: &mut W,
        frame: &SequencerWireFrameV1,
        bounds: &ProtocolBoundsProfileV1,
    ) -> Result<(), Sequencer1Error> {
        let encoded = frame.encode_canonical(bounds)?;
        writer
            .write_all(&encoded)
            .map_err(|e| Sequencer1Error::Io(format!("tcp write frame: {e}")))?;
        writer
            .flush()
            .map_err(|e| Sequencer1Error::Io(format!("tcp flush frame: {e}")))
    }

    /// Reads one [`SequencerWireFrameV1`] from `reader`, preflighting the 13-byte header against
    /// `bounds` **before** allocating any payload buffer.
    pub fn read_bounded_frame<R: Read>(
        reader: &mut R,
        bounds: &ProtocolBoundsProfileV1,
    ) -> Result<SequencerWireFrameV1, Sequencer1Error> {
        let mut hdr = [0u8; SEQ_WIRE_HEADER_LEN];
        reader
            .read_exact(&mut hdr)
            .map_err(|e| Sequencer1Error::Io(format!("tcp read header: {e}")))?;
        let (msg_kind, payload_len) = SequencerWireFrameV1::preflight_header(&hdr, bounds)?;
        let mut payload = vec![0u8; payload_len];
        reader
            .read_exact(&mut payload)
            .map_err(|e| Sequencer1Error::Io(format!("tcp read payload: {e}")))?;
        Ok(SequencerWireFrameV1 {
            version: SEQUENCER_1_VERSION,
            msg_kind,
            payload,
        })
    }

    /// Serves one loopback client `RequestBatchRange` session from `publications` over `listener`.
    pub fn serve_batch_range_once(
        listener: &TcpListener,
        publications: &[SequencerBatchPublicationV1],
        bounds: &ProtocolBoundsProfileV1,
    ) -> Result<usize, Sequencer1Error> {
        let (mut stream, _) = listener
            .accept()
            .map_err(|e| Sequencer1Error::Io(format!("tcp accept: {e}")))?;
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
        let req = read_bounded_frame(&mut stream, bounds)?;
        if req.msg_kind != SequencerWireMsgKindV1::RequestBatchRange {
            return Err(Sequencer1Error::InvalidWire(
                "expected RequestBatchRange frame",
            ));
        }
        let from_height = SequencerWireFrameV1::decode_range_request(&req.payload)?;
        let mut sent = 0usize;
        for pub_bundle in publications {
            if pub_bundle.header.batch_height >= from_height {
                let frame = SequencerWireFrameV1 {
                    version: SEQUENCER_1_VERSION,
                    msg_kind: SequencerWireMsgKindV1::BatchPublication,
                    payload: pub_bundle.encode_canonical()?,
                };
                write_bounded_frame(&mut stream, &frame, bounds)?;
                sent += 1;
            }
        }
        // End-of-stream marker: ClientAck frame carrying count/tip
        let mut eof = [0u8; 40];
        eof[0..8].copy_from_slice(&(sent as u64).to_be_bytes());
        let done = SequencerWireFrameV1 {
            version: SEQUENCER_1_VERSION,
            msg_kind: SequencerWireMsgKindV1::ClientAck,
            payload: eof.to_vec(),
        };
        write_bounded_frame(&mut stream, &done, bounds)?;
        Ok(sent)
    }

    /// Connects to a loopback sequencer endpoint at `addr` and pulls all bounded publications
    /// starting at `from_height`.
    pub fn fetch_batch_range_loopback(
        addr: SocketAddr,
        from_height: u64,
        bounds: &ProtocolBoundsProfileV1,
    ) -> Result<Vec<SequencerBatchPublicationV1>, Sequencer1Error> {
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5))
            .map_err(|e| Sequencer1Error::Io(format!("tcp connect: {e}")))?;
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
        let req = SequencerWireFrameV1 {
            version: SEQUENCER_1_VERSION,
            msg_kind: SequencerWireMsgKindV1::RequestBatchRange,
            payload: SequencerWireFrameV1::encode_range_request(from_height).to_vec(),
        };
        write_bounded_frame(&mut stream, &req, bounds)?;
        let mut out = Vec::new();
        loop {
            let frame = read_bounded_frame(&mut stream, bounds)?;
            match frame.msg_kind {
                SequencerWireMsgKindV1::BatchPublication => {
                    out.push(SequencerBatchPublicationV1::decode_canonical(
                        &frame.payload,
                        bounds,
                    )?);
                }
                SequencerWireMsgKindV1::ClientAck => {
                    if frame.payload.len() != 40 {
                        return Err(Sequencer1Error::InvalidWire("bad eof ClientAck len"));
                    }
                    let expected = u64::from_be_bytes(frame.payload[0..8].try_into().unwrap());
                    if expected != out.len() as u64 {
                        return Err(Sequencer1Error::InvalidWire(
                            "batch range count disagreed with EOF marker",
                        ));
                    }
                    break;
                }
                _ => {
                    return Err(Sequencer1Error::InvalidWire(
                        "unexpected frame during batch range sync",
                    ));
                }
            }
        }
        Ok(out)
    }
}

struct SeqReader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> SeqReader<'a> {
    const fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], Sequencer1Error> {
        if self.buf.len().saturating_sub(self.pos) < n {
            return Err(Sequencer1Error::InvalidWire("truncated sequencer buffer"));
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn u16_be(&mut self) -> Result<u16, Sequencer1Error> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn u32_be(&mut self) -> Result<u32, Sequencer1Error> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn u64_be(&mut self) -> Result<u64, Sequencer1Error> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }

    fn arr32(&mut self) -> Result<[u8; 32], Sequencer1Error> {
        Ok(self.take(32)?.try_into().unwrap())
    }

    fn arr64(&mut self) -> Result<[u8; 64], Sequencer1Error> {
        Ok(self.take(64)?.try_into().unwrap())
    }

    fn finish(self) -> Result<(), Sequencer1Error> {
        if self.pos != self.buf.len() {
            return Err(Sequencer1Error::InvalidWire("trailing sequencer bytes"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequencer_1_canonical_header_publication_merkle_and_wire_roundtrip() {
        let fix = CanonicalMultiActorFixtureV1::build().unwrap();
        let mut engine = SequencerEngineV1::from_fixture(&fix, DEFAULT_SEQUENCER_SEED).unwrap();
        let pre_wr = engine.current_world_root().unwrap();
        let bundle = fix.canonical_mixed_race_bundle(1, pre_wr).unwrap();
        let pub_b = engine.seal_bundle_and_publish(&bundle).unwrap();

        let hdr_raw = pub_b.header.encode_canonical().unwrap();
        assert_eq!(
            SequencerBatchHeaderV1::decode_canonical(&hdr_raw).unwrap(),
            pub_b.header
        );
        let pub_raw = pub_b.encode_canonical().unwrap();
        assert_eq!(
            SequencerBatchPublicationV1::decode_canonical(&pub_raw, &engine.bounds()).unwrap(),
            pub_b
        );

        let proof0 = pub_b.build_receipt_proof(0).unwrap();
        let proof_raw = proof0.encode_canonical().unwrap();
        assert_eq!(
            ReceiptMerkleProofV1::decode_canonical(&proof_raw).unwrap(),
            proof0
        );

        let frame = SequencerWireFrameV1 {
            version: SEQUENCER_1_VERSION,
            msg_kind: SequencerWireMsgKindV1::BatchPublication,
            payload: pub_raw,
        };
        let f_raw = frame.encode_canonical(&engine.bounds()).unwrap();
        assert_eq!(
            SequencerWireFrameV1::decode_canonical(&f_raw, &engine.bounds()).unwrap(),
            frame
        );
        assert_eq!(
            SequencerWireFrameV1::encode_range_request(1),
            [0, 0, 0, 0, 0, 0, 0, 1]
        );

        let mut client =
            SequencerClientVerifierV1::from_fixture(&fix, engine.sequencer_pubkey()).unwrap();
        assert_eq!(
            client.submit_header_without_body(&pub_b.header),
            Err(Sequencer1Error::WithheldBodyRefused)
        );
        let replayed = client.verify_and_apply_publication(&pub_b).unwrap();
        assert_eq!(client.verified_header(1), Some(&pub_b.header));
        assert_eq!(
            replayed.post_composition.execution_root_ref,
            pub_b.header.execution_root_ref
        );
        client
            .verify_receipt_inclusion(1, &proof0, &pub_b.fold_receipt.step_outcomes[0])
            .unwrap();
        assert_eq!(
            pub_b.header.tag().unwrap().to_string(),
            "#bb.receipt.sha256.zq4zzmtlddpcvphk24t3koybltetooeb5qo3b6yvjhbncrgdsyza"
        );
        assert_eq!(
            pub_b.tag().unwrap().to_string(),
            "#bb.receipt.sha256.htcbximoqnut4khf6do3lev5r4znvuvd2ppjgaeicgjiml254eia"
        );
        let p_sha: [u8; 32] = Sha256::digest(&proof_raw).into();
        let f_sha: [u8; 32] = Sha256::digest(&f_raw).into();
        assert_eq!(
            p_sha[0..4],
            [0x2a, 0x1f, 0xbc, 0x6c],
            "proof0 sha256 prefix pinned"
        );
        assert_eq!(
            f_sha[0..4],
            [0x7d, 0x73, 0x76, 0x7e],
            "wire frame sha256 prefix pinned"
        );
        let verified_from_storage = SequencerLogV1::verify_from_storage_records(
            engine.log().storage_chain().records(),
            &engine.sequencer_pubkey(),
            &engine.bounds(),
        )
        .unwrap();
        assert_eq!(verified_from_storage, vec![pub_b.clone()]);
    }
}
