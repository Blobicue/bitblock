//! `AUTH-COMMIT-1`: separately typed authenticated authorization (`AuthorizationRootV1`) and
//! sequential nonce (`NonceRootV1`) commitments over a reusable 256-bit domain-separated
//! Candidate A binary Merkle Patricia Trie (`PS-029`).
//!
//! Leaves `COMMIT-1` (`commit_state.rs`) and Lean 4 `REFINE-1@commitment` untouched while
//! exposing [`DomainPatriciaTrieV1`] and [`DomainPatriciaProofV1`] for all Layer 1 state
//! producers (`AUTH-COMMIT-1`, `ASSET-1`, `GOVERNANCE-1`, `ECONOMIC-STATE-1`, and
//! `VALIDATOR-SCHEDULE-1`):
//! - **Strict domain separation ([`DomainIdV1`]):** every empty, leaf, extension, and branch node
//!   binds its 1-byte `DomainIdV1` inside SHA-256, making cross-domain proof verification across
//!   `Authorization`, `Nonce`, `Asset`, `Governance`, `Economic`, `Schedule`, or `COMMIT-1`
//!   occupancy/episode roots impossible.
//! - **Canonical path-copy normalization:** incremental `upsert` and `delete` produce the exact
//!   same trie topology and root hash as cold [`DomainPatriciaTrieV1::from_entries`] full rebuild.
//! - **Bounded inclusion & non-membership proofs:** every [`DomainPatriciaProofV1`] enforces
//!   `BOUNDS-1`'s `<= 256` depth and `<= 12 KiB` encoded proof ceiling.
//! - **Atomic staged commits ([`AuthCommitStateV1`]):** transaction nonce advances, admin
//!   rotations, revocations, and timelocked recoveries stage updates across both tries and roll
//!   back completely on any authorization or validation failure.
//!
//! An authenticated authorization map does not prove the key holder is one unique human or that a
//! submitted action was included, executed, or finalized by consensus.

use std::collections::BTreeMap;
use std::fmt;

use sha2::{Digest, Sha256};

use crate::bounds_1::{DEFAULT_MAX_PROOF_BYTES, DEFAULT_MAX_PROOF_DEPTH};
use crate::crypto_d1::SigningPreimageV1;
use crate::identity_1::{
    ActorAuthStateV1, Identity1Error, IdentityAuthReceiptV1, IdentityContextV1,
    IdentityDispositionV1, IdentityRegistryV1,
};
use crate::identity_d1::{ActorIdV1, AuthorizationDescriptorV1, TransactionBodyV1};
use crate::protocol::{ContentTag, TagKind, hash_canonical};

/// Canonical schema string for an [`AuthCommitRootsV1`] paired root commitment (`AUTH-COMMIT-1`).
pub const AUTH_COMMIT_1_ROOTS_SCHEMA: &[u8] = b"bitblock/auth-commit-1/auth-nonce-roots/v1";
/// Canonical schema string for a [`DomainPatriciaProofV1`] point witness (`AUTH-COMMIT-1`).
pub const AUTH_COMMIT_1_PROOF_SCHEMA: &[u8] = b"bitblock/auth-commit-1/domain-patricia-proof/v1";

/// Fixed key bit width (`256` bits = `32` bytes) for [`DomainPatriciaTrieV1`].
pub const DOMAIN_KEY_BITS: usize = 256;
/// Fixed key byte length (`32` bytes) for [`DomainPatriciaTrieV1`].
pub const DOMAIN_KEY_BYTES: usize = 32;

const PATRICIA_EMPTY_PREFIX: &[u8] = b"bb.patricia.empty.v1";
const PATRICIA_LEAF_PREFIX: &[u8] = b"bb.patricia.leaf.v1";
const PATRICIA_EXT_PREFIX: &[u8] = b"bb.patricia.ext.v1";
const PATRICIA_BRANCH_PREFIX: &[u8] = b"bb.patricia.branch.v1";

/// Fail-closed validation, codec, proof, or atomic-commit error for `AUTH-COMMIT-1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthCommit1Error {
    /// Underlying `IDENTITY-1` configuration or codec error.
    Identity(Identity1Error),
    /// Wire buffer had invalid length, version, bit padding, discriminant, or trailing bytes.
    InvalidWire(&'static str),
    /// Duplicate key was supplied during cold trie rebuild.
    DuplicateKey,
    /// Proof exceeded `BOUNDS-1` depth (`256`) or byte ceiling (`12 KiB`).
    ProofOverBounds(&'static str),
    /// Proof domain tag did not match the expected [`DomainIdV1`].
    CrossDomainProofRejected {
        /// Domain requested by verifier.
        expected: DomainIdV1,
        /// Domain embedded inside the submitted proof.
        actual: DomainIdV1,
    },
    /// Proof path, terminal invariant, or recomputed root hash failed verification.
    ProofVerificationFailed(&'static str),
    /// Incremental trie root disagreed with cold full-rebuild parity check.
    RebuildParityMismatch,
    /// Staged atomic authorization commit failed and rolled back cleanly.
    AtomicCommitRejected(IdentityDispositionV1),
}

impl From<Identity1Error> for AuthCommit1Error {
    fn from(value: Identity1Error) -> Self {
        Self::Identity(value)
    }
}

impl fmt::Display for AuthCommit1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for AuthCommit1Error {}

/// Domain separator isolating every Layer 1 authenticated Patricia state tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum DomainIdV1 {
    /// Committed [`ActorAuthStateV1`] descriptors and recovery state (`AUTH-COMMIT-1`).
    Authorization = 1,
    /// Committed sequential `u64` principal nonces (`AUTH-COMMIT-1`).
    Nonce = 2,
    /// Committed fungible native balances (`ASSET-1`).
    NativeBalance = 3,
    /// Committed non-fungible occupancy-episode ownership records (`ASSET-1`).
    EpisodeOwnership = 4,
    /// Committed governance proposals and parameter state (`GOVERNANCE-1`).
    GovernanceState = 5,
    /// Committed validator bonded stake, unbonding, and slash records (`ECONOMIC-STATE-1`).
    EconomicStake = 6,
    /// Committed delayed-activation epoch validator schedules (`VALIDATOR-SCHEDULE-1`).
    ValidatorSchedule = 7,
}

impl DomainIdV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte domain discriminant.
    pub const fn from_code(code: u8) -> Result<Self, AuthCommit1Error> {
        match code {
            1 => Ok(Self::Authorization),
            2 => Ok(Self::Nonce),
            3 => Ok(Self::NativeBalance),
            4 => Ok(Self::EpisodeOwnership),
            5 => Ok(Self::GovernanceState),
            6 => Ok(Self::EconomicStake),
            7 => Ok(Self::ValidatorSchedule),
            _ => Err(AuthCommit1Error::InvalidWire("unknown DomainIdV1")),
        }
    }
}

fn key_to_bits(key: &[u8; DOMAIN_KEY_BYTES]) -> [u8; DOMAIN_KEY_BITS] {
    let mut out = [0u8; DOMAIN_KEY_BITS];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = (key[i >> 3] >> (7 - (i & 7))) & 1;
    }
    out
}

fn encode_bitpath(bits: &[u8], out: &mut Vec<u8>) {
    out.extend_from_slice(&(bits.len() as u16).to_be_bytes());
    let byte_len = bits.len().div_ceil(8);
    let start = out.len();
    out.resize(start + byte_len, 0u8);
    for (i, &b) in bits.iter().enumerate() {
        if b != 0 {
            out[start + (i >> 3)] |= 1u8 << (7 - (i & 7));
        }
    }
}

fn decode_bitpath(bytes: &[u8], cur: &mut usize) -> Result<Vec<u8>, AuthCommit1Error> {
    if bytes.len().saturating_sub(*cur) < 2 {
        return Err(AuthCommit1Error::InvalidWire("truncated bitpath len"));
    }
    let bit_len = u16::from_be_bytes([bytes[*cur], bytes[*cur + 1]]) as usize;
    *cur += 2;
    if bit_len > DOMAIN_KEY_BITS {
        return Err(AuthCommit1Error::InvalidWire("bitpath exceeds 256 bits"));
    }
    let byte_len = bit_len.div_ceil(8);
    if bytes.len().saturating_sub(*cur) < byte_len {
        return Err(AuthCommit1Error::InvalidWire("truncated bitpath bytes"));
    }
    let slice = &bytes[*cur..*cur + byte_len];
    *cur += byte_len;
    let unused_bits = byte_len * 8 - bit_len;
    if unused_bits > 0 && (slice[byte_len - 1] & ((1u8 << unused_bits) - 1)) != 0 {
        return Err(AuthCommit1Error::InvalidWire("non-zero bitpath padding"));
    }
    let mut bits = Vec::with_capacity(bit_len);
    for i in 0..bit_len {
        bits.push((slice[i >> 3] >> (7 - (i & 7))) & 1);
    }
    Ok(bits)
}

fn sha256_bytes(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

/// Computes the domain-separated empty-root digest for `domain`.
pub fn empty_domain_root(domain: DomainIdV1) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(PATRICIA_EMPTY_PREFIX);
    h.update([domain.code()]);
    h.finalize().into()
}

fn hash_leaf(domain: DomainIdV1, suffix: &[u8], value_digest: [u8; 32]) -> [u8; 32] {
    let mut bp = Vec::with_capacity(2 + suffix.len().div_ceil(8));
    encode_bitpath(suffix, &mut bp);
    let mut h = Sha256::new();
    h.update(PATRICIA_LEAF_PREFIX);
    h.update([domain.code()]);
    h.update(&bp);
    h.update(value_digest);
    h.finalize().into()
}

fn hash_ext(domain: DomainIdV1, prefix: &[u8], child_hash: [u8; 32]) -> [u8; 32] {
    let mut bp = Vec::with_capacity(2 + prefix.len().div_ceil(8));
    encode_bitpath(prefix, &mut bp);
    let mut h = Sha256::new();
    h.update(PATRICIA_EXT_PREFIX);
    h.update([domain.code()]);
    h.update(&bp);
    h.update(child_hash);
    h.finalize().into()
}

fn hash_branch(domain: DomainIdV1, left_hash: [u8; 32], right_hash: [u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(PATRICIA_BRANCH_PREFIX);
    h.update([domain.code()]);
    h.update(left_hash);
    h.update(right_hash);
    h.finalize().into()
}

fn lcp_len(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count()
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TrieNode {
    Empty,
    Leaf {
        suffix: Vec<u8>,
        value: Vec<u8>,
        value_digest: [u8; 32],
        hash: [u8; 32],
    },
    Extension {
        prefix: Vec<u8>,
        child: Box<TrieNode>,
        hash: [u8; 32],
    },
    Branch {
        left: Box<TrieNode>,
        right: Box<TrieNode>,
        hash: [u8; 32],
    },
}

impl TrieNode {
    fn hash(&self, domain: DomainIdV1) -> [u8; 32] {
        match self {
            Self::Empty => empty_domain_root(domain),
            Self::Leaf { hash, .. } | Self::Extension { hash, .. } | Self::Branch { hash, .. } => {
                *hash
            }
        }
    }

    fn leaf(domain: DomainIdV1, suffix: Vec<u8>, value: Vec<u8>) -> Self {
        let value_digest = sha256_bytes(&value);
        let hash = hash_leaf(domain, &suffix, value_digest);
        Self::Leaf {
            suffix,
            value,
            value_digest,
            hash,
        }
    }

    fn ext(domain: DomainIdV1, prefix: Vec<u8>, child: Self) -> Self {
        debug_assert!(!prefix.is_empty());
        let hash = hash_ext(domain, &prefix, child.hash(domain));
        Self::Extension {
            prefix,
            child: Box::new(child),
            hash,
        }
    }

    fn branch(domain: DomainIdV1, left: Self, right: Self) -> Self {
        let hash = hash_branch(domain, left.hash(domain), right.hash(domain));
        Self::Branch {
            left: Box::new(left),
            right: Box::new(right),
            hash,
        }
    }
}

fn build_subtree(
    domain: DomainIdV1,
    items: &[([u8; DOMAIN_KEY_BITS], &[u8])],
    depth: usize,
) -> TrieNode {
    match items.len() {
        0 => TrieNode::Empty,
        1 => TrieNode::leaf(domain, items[0].0[depth..].to_vec(), items[0].1.to_vec()),
        _ => {
            let first = &items[0].0[depth..];
            let last = &items[items.len() - 1].0[depth..];
            let common = lcp_len(first, last);
            let split_depth = depth + common;
            let mid = items.partition_point(|(bits, _)| bits[split_depth] == 0);
            let left = build_subtree(domain, &items[..mid], split_depth + 1);
            let right = build_subtree(domain, &items[mid..], split_depth + 1);
            let br = TrieNode::branch(domain, left, right);
            if common > 0 {
                TrieNode::ext(domain, first[..common].to_vec(), br)
            } else {
                br
            }
        }
    }
}

fn upsert_node(
    domain: DomainIdV1,
    node: TrieNode,
    key_bits: &[u8; DOMAIN_KEY_BITS],
    depth: usize,
    value: Vec<u8>,
) -> TrieNode {
    match node {
        TrieNode::Empty => TrieNode::leaf(domain, key_bits[depth..].to_vec(), value),
        TrieNode::Leaf {
            suffix,
            value: old_val,
            ..
        } => {
            let rem = &key_bits[depth..];
            if suffix.as_slice() == rem {
                return TrieNode::leaf(domain, suffix, value);
            }
            let common = lcp_len(&suffix, rem);
            let old_leaf = TrieNode::leaf(domain, suffix[common + 1..].to_vec(), old_val);
            let new_leaf = TrieNode::leaf(domain, rem[common + 1..].to_vec(), value);
            let br = if suffix[common] == 0 {
                TrieNode::branch(domain, old_leaf, new_leaf)
            } else {
                TrieNode::branch(domain, new_leaf, old_leaf)
            };
            if common > 0 {
                TrieNode::ext(domain, suffix[..common].to_vec(), br)
            } else {
                br
            }
        }
        TrieNode::Extension { prefix, child, .. } => {
            let rem = &key_bits[depth..];
            let common = lcp_len(&prefix, rem);
            if common == prefix.len() {
                let updated = upsert_node(domain, *child, key_bits, depth + common, value);
                return TrieNode::ext(domain, prefix, updated);
            }
            let old_branch_side = if common + 1 < prefix.len() {
                TrieNode::ext(domain, prefix[common + 1..].to_vec(), *child)
            } else {
                *child
            };
            let new_leaf = TrieNode::leaf(domain, rem[common + 1..].to_vec(), value);
            let br = if prefix[common] == 0 {
                TrieNode::branch(domain, old_branch_side, new_leaf)
            } else {
                TrieNode::branch(domain, new_leaf, old_branch_side)
            };
            if common > 0 {
                TrieNode::ext(domain, prefix[..common].to_vec(), br)
            } else {
                br
            }
        }
        TrieNode::Branch { left, right, .. } => {
            if key_bits[depth] == 0 {
                let new_left = upsert_node(domain, *left, key_bits, depth + 1, value);
                TrieNode::branch(domain, new_left, *right)
            } else {
                let new_right = upsert_node(domain, *right, key_bits, depth + 1, value);
                TrieNode::branch(domain, *left, new_right)
            }
        }
    }
}

fn prepend_edge(domain: DomainIdV1, mut edge: Vec<u8>, node: TrieNode) -> TrieNode {
    match node {
        TrieNode::Empty => TrieNode::Empty,
        TrieNode::Leaf { suffix, value, .. } => {
            edge.extend_from_slice(&suffix);
            TrieNode::leaf(domain, edge, value)
        }
        TrieNode::Extension { prefix, child, .. } => {
            edge.extend_from_slice(&prefix);
            TrieNode::ext(domain, edge, *child)
        }
        branch @ TrieNode::Branch { .. } => TrieNode::ext(domain, edge, branch),
    }
}

fn delete_node(
    domain: DomainIdV1,
    node: TrieNode,
    key_bits: &[u8; DOMAIN_KEY_BITS],
    depth: usize,
) -> (TrieNode, bool) {
    match node {
        TrieNode::Empty => (TrieNode::Empty, false),
        TrieNode::Leaf {
            suffix,
            value,
            value_digest,
            hash,
        } => {
            if suffix.as_slice() == &key_bits[depth..] {
                (TrieNode::Empty, true)
            } else {
                (
                    TrieNode::Leaf {
                        suffix,
                        value,
                        value_digest,
                        hash,
                    },
                    false,
                )
            }
        }
        TrieNode::Extension {
            prefix,
            child,
            hash,
        } => {
            let rem = &key_bits[depth..];
            if !rem.starts_with(&prefix) {
                return (
                    TrieNode::Extension {
                        prefix,
                        child,
                        hash,
                    },
                    false,
                );
            }
            let (new_child, removed) = delete_node(domain, *child, key_bits, depth + prefix.len());
            if !removed {
                return (
                    TrieNode::Extension {
                        prefix,
                        child: Box::new(new_child),
                        hash,
                    },
                    false,
                );
            }
            (prepend_edge(domain, prefix, new_child), true)
        }
        TrieNode::Branch { left, right, hash } => {
            if key_bits[depth] == 0 {
                let (new_left, removed) = delete_node(domain, *left, key_bits, depth + 1);
                if !removed {
                    return (
                        TrieNode::Branch {
                            left: Box::new(new_left),
                            right,
                            hash,
                        },
                        false,
                    );
                }
                if matches!(new_left, TrieNode::Empty) {
                    (prepend_edge(domain, vec![1], *right), true)
                } else {
                    (TrieNode::branch(domain, new_left, *right), true)
                }
            } else {
                let (new_right, removed) = delete_node(domain, *right, key_bits, depth + 1);
                if !removed {
                    return (
                        TrieNode::Branch {
                            left,
                            right: Box::new(new_right),
                            hash,
                        },
                        false,
                    );
                }
                if matches!(new_right, TrieNode::Empty) {
                    (prepend_edge(domain, vec![0], *left), true)
                } else {
                    (TrieNode::branch(domain, *left, new_right), true)
                }
            }
        }
    }
}

/// One traversal step along a [`DomainPatriciaProofV1`] witness path.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DomainProofStepV1 {
    /// Binary branch step carrying the traversed edge bit (`0` or `1`) and sibling hash.
    Branch {
        /// Bit taken along the queried key (`0` = left, `1` = right).
        direction_bit: u8,
        /// 32-byte digest of the untraversed sibling child.
        sibling_hash: [u8; 32],
    },
    /// Shared compressed bit-prefix step matching the queried key.
    Extension {
        /// Non-empty matched bit prefix (`1..=256` bits).
        prefix_bits: Vec<u8>,
    },
}

/// Terminal witness node concluding a [`DomainPatriciaProofV1`] inclusion or absence proof.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DomainProofTerminalV1 {
    /// Root trie is empty (proves non-membership).
    Empty,
    /// Reached exact matching key leaf with its canonical value bytes (proves membership).
    LeafMatch {
        /// Remaining suffix bits completing the 256-bit key.
        suffix_bits: Vec<u8>,
        /// Canonical value bytes stored at `key`.
        value: Vec<u8>,
    },
    /// Reached another key's leaf whose suffix differs from `key` (proves non-membership).
    LeafMismatch {
        /// Neighbor leaf's remaining suffix bits (`!= remaining key bits`).
        suffix_bits: Vec<u8>,
        /// SHA-256 digest of the neighbor leaf's value (avoids leaking neighbor value bytes).
        value_digest: [u8; 32],
    },
    /// Encountered an extension whose prefix diverges from `key` (proves non-membership).
    DivergentExtension {
        /// Non-empty extension prefix that disagrees with the queried key.
        prefix_bits: Vec<u8>,
        /// 32-byte digest of the extension's child node.
        child_hash: [u8; 32],
    },
}

/// Domain-separated 256-bit binary Patricia inclusion or non-membership proof (`AUTH-COMMIT-1`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DomainPatriciaProofV1 {
    /// Proof schema version (`1`).
    pub proof_version: u16,
    /// Domain separator binding this proof to exactly one state tree.
    pub domain: DomainIdV1,
    /// 32-byte queried key.
    pub key: [u8; DOMAIN_KEY_BYTES],
    /// Root-to-terminal traversal steps (`<= 256`).
    pub steps: Vec<DomainProofStepV1>,
    /// Terminal witness node.
    pub terminal: DomainProofTerminalV1,
}

impl DomainPatriciaProofV1 {
    /// Encodes this proof into canonical big-endian wire bytes, enforcing `BOUNDS-1` `12 KiB` cap.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, AuthCommit1Error> {
        if self.proof_version != 1 {
            return Err(AuthCommit1Error::InvalidWire("proof_version must equal 1"));
        }
        if self.steps.len() > usize::from(DEFAULT_MAX_PROOF_DEPTH) {
            return Err(AuthCommit1Error::ProofOverBounds("proof depth exceeds 256"));
        }
        let mut out = Vec::with_capacity(128);
        out.extend_from_slice(&self.proof_version.to_be_bytes());
        out.push(self.domain.code());
        out.extend_from_slice(&self.key);
        out.extend_from_slice(&(self.steps.len() as u16).to_be_bytes());
        for step in &self.steps {
            match step {
                DomainProofStepV1::Branch {
                    direction_bit,
                    sibling_hash,
                } => {
                    if *direction_bit > 1 {
                        return Err(AuthCommit1Error::InvalidWire("invalid branch bit"));
                    }
                    out.push(1);
                    out.push(*direction_bit);
                    out.extend_from_slice(sibling_hash);
                }
                DomainProofStepV1::Extension { prefix_bits } => {
                    if prefix_bits.is_empty() || prefix_bits.len() > DOMAIN_KEY_BITS {
                        return Err(AuthCommit1Error::InvalidWire("invalid ext prefix len"));
                    }
                    out.push(2);
                    encode_bitpath(prefix_bits, &mut out);
                }
            }
        }
        match &self.terminal {
            DomainProofTerminalV1::Empty => out.push(0),
            DomainProofTerminalV1::LeafMatch { suffix_bits, value } => {
                out.push(1);
                encode_bitpath(suffix_bits, &mut out);
                out.extend_from_slice(&(value.len() as u32).to_be_bytes());
                out.extend_from_slice(value);
            }
            DomainProofTerminalV1::LeafMismatch {
                suffix_bits,
                value_digest,
            } => {
                out.push(2);
                encode_bitpath(suffix_bits, &mut out);
                out.extend_from_slice(value_digest);
            }
            DomainProofTerminalV1::DivergentExtension {
                prefix_bits,
                child_hash,
            } => {
                if prefix_bits.is_empty() {
                    return Err(AuthCommit1Error::InvalidWire("empty divergent ext"));
                }
                out.push(3);
                encode_bitpath(prefix_bits, &mut out);
                out.extend_from_slice(child_hash);
            }
        }
        if out.len() > DEFAULT_MAX_PROOF_BYTES as usize {
            return Err(AuthCommit1Error::ProofOverBounds(
                "encoded proof exceeds BOUNDS-1 12 KiB cap",
            ));
        }
        Ok(out)
    }

    /// Decodes and bounds-checks a canonical [`DomainPatriciaProofV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, AuthCommit1Error> {
        if bytes.len() > DEFAULT_MAX_PROOF_BYTES as usize {
            return Err(AuthCommit1Error::ProofOverBounds(
                "proof exceeds BOUNDS-1 12 KiB cap",
            ));
        }
        if bytes.len() < 38 {
            return Err(AuthCommit1Error::InvalidWire(
                "truncated DomainPatriciaProofV1",
            ));
        }
        let proof_version = u16::from_be_bytes([bytes[0], bytes[1]]);
        if proof_version != 1 {
            return Err(AuthCommit1Error::InvalidWire("proof_version must equal 1"));
        }
        let domain = DomainIdV1::from_code(bytes[2])?;
        let key: [u8; 32] = bytes[3..35].try_into().expect("32");
        let step_count = u16::from_be_bytes([bytes[35], bytes[36]]) as usize;
        if step_count > usize::from(DEFAULT_MAX_PROOF_DEPTH) {
            return Err(AuthCommit1Error::ProofOverBounds("proof depth exceeds 256"));
        }
        let mut cur = 37usize;
        let mut steps = Vec::with_capacity(step_count);
        for _ in 0..step_count {
            if bytes.len().saturating_sub(cur) < 1 {
                return Err(AuthCommit1Error::InvalidWire("truncated step tag"));
            }
            let tag = bytes[cur];
            cur += 1;
            match tag {
                1 => {
                    if bytes.len().saturating_sub(cur) < 33 {
                        return Err(AuthCommit1Error::InvalidWire("truncated branch step"));
                    }
                    let direction_bit = bytes[cur];
                    if direction_bit > 1 {
                        return Err(AuthCommit1Error::InvalidWire("invalid branch bit"));
                    }
                    let sibling_hash: [u8; 32] = bytes[cur + 1..cur + 33].try_into().expect("32");
                    cur += 33;
                    steps.push(DomainProofStepV1::Branch {
                        direction_bit,
                        sibling_hash,
                    });
                }
                2 => {
                    let prefix_bits = decode_bitpath(bytes, &mut cur)?;
                    if prefix_bits.is_empty() {
                        return Err(AuthCommit1Error::InvalidWire("empty ext step"));
                    }
                    steps.push(DomainProofStepV1::Extension { prefix_bits });
                }
                _ => return Err(AuthCommit1Error::InvalidWire("unknown proof step tag")),
            }
        }
        if bytes.len().saturating_sub(cur) < 1 {
            return Err(AuthCommit1Error::InvalidWire("missing terminal tag"));
        }
        let term_tag = bytes[cur];
        cur += 1;
        let terminal = match term_tag {
            0 => DomainProofTerminalV1::Empty,
            1 => {
                let suffix_bits = decode_bitpath(bytes, &mut cur)?;
                if bytes.len().saturating_sub(cur) < 4 {
                    return Err(AuthCommit1Error::InvalidWire("truncated leaf value len"));
                }
                let vlen = u32::from_be_bytes(bytes[cur..cur + 4].try_into().expect("4")) as usize;
                cur += 4;
                if bytes.len().saturating_sub(cur) < vlen {
                    return Err(AuthCommit1Error::InvalidWire("truncated leaf value"));
                }
                let value = bytes[cur..cur + vlen].to_vec();
                cur += vlen;
                DomainProofTerminalV1::LeafMatch { suffix_bits, value }
            }
            2 => {
                let suffix_bits = decode_bitpath(bytes, &mut cur)?;
                if bytes.len().saturating_sub(cur) < 32 {
                    return Err(AuthCommit1Error::InvalidWire("truncated mismatch digest"));
                }
                let value_digest: [u8; 32] = bytes[cur..cur + 32].try_into().expect("32");
                cur += 32;
                DomainProofTerminalV1::LeafMismatch {
                    suffix_bits,
                    value_digest,
                }
            }
            3 => {
                let prefix_bits = decode_bitpath(bytes, &mut cur)?;
                if prefix_bits.is_empty() || bytes.len().saturating_sub(cur) < 32 {
                    return Err(AuthCommit1Error::InvalidWire("truncated divergent ext"));
                }
                let child_hash: [u8; 32] = bytes[cur..cur + 32].try_into().expect("32");
                cur += 32;
                DomainProofTerminalV1::DivergentExtension {
                    prefix_bits,
                    child_hash,
                }
            }
            _ => return Err(AuthCommit1Error::InvalidWire("unknown terminal tag")),
        };
        if cur != bytes.len() {
            return Err(AuthCommit1Error::InvalidWire("trailing proof bytes"));
        }
        Ok(Self {
            proof_version,
            domain,
            key,
            steps,
            terminal,
        })
    }

    /// Computes the canonical `Receipt` `ContentTag` for this domain Patricia proof.
    pub fn tag(&self) -> Result<ContentTag, AuthCommit1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            AUTH_COMMIT_1_PROOF_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and bounded length"))
    }

    /// Verifies this proof against `(expected_domain, expected_root, expected_key)`, returning
    /// `Ok(Some(value))` for inclusion or `Ok(None)` for proven non-membership.
    pub fn verify(
        &self,
        expected_domain: DomainIdV1,
        expected_root: [u8; 32],
        expected_key: &[u8; DOMAIN_KEY_BYTES],
    ) -> Result<Option<Vec<u8>>, AuthCommit1Error> {
        let _ = self.encode_canonical()?;
        if self.domain != expected_domain {
            return Err(AuthCommit1Error::CrossDomainProofRejected {
                expected: expected_domain,
                actual: self.domain,
            });
        }
        if &self.key != expected_key {
            return Err(AuthCommit1Error::ProofVerificationFailed(
                "proof key mismatch",
            ));
        }
        let key_bits = key_to_bits(expected_key);
        let mut depth = 0usize;
        for step in &self.steps {
            match step {
                DomainProofStepV1::Branch { direction_bit, .. } => {
                    if depth >= DOMAIN_KEY_BITS || key_bits[depth] != *direction_bit {
                        return Err(AuthCommit1Error::ProofVerificationFailed(
                            "branch step disagrees with key bit",
                        ));
                    }
                    depth += 1;
                }
                DomainProofStepV1::Extension { prefix_bits } => {
                    if depth + prefix_bits.len() > DOMAIN_KEY_BITS
                        || &key_bits[depth..depth + prefix_bits.len()] != prefix_bits.as_slice()
                    {
                        return Err(AuthCommit1Error::ProofVerificationFailed(
                            "extension step disagrees with key bits",
                        ));
                    }
                    depth += prefix_bits.len();
                }
            }
        }
        let (mut cur_hash, outcome) = match &self.terminal {
            DomainProofTerminalV1::Empty => {
                if !self.steps.is_empty() {
                    return Err(AuthCommit1Error::ProofVerificationFailed(
                        "Empty terminal requires zero steps",
                    ));
                }
                (empty_domain_root(self.domain), None)
            }
            DomainProofTerminalV1::LeafMatch { suffix_bits, value } => {
                if depth + suffix_bits.len() != DOMAIN_KEY_BITS
                    || &key_bits[depth..] != suffix_bits.as_slice()
                {
                    return Err(AuthCommit1Error::ProofVerificationFailed(
                        "LeafMatch suffix disagrees with queried key",
                    ));
                }
                (
                    hash_leaf(self.domain, suffix_bits, sha256_bytes(value)),
                    Some(value.clone()),
                )
            }
            DomainProofTerminalV1::LeafMismatch {
                suffix_bits,
                value_digest,
            } => {
                if depth + suffix_bits.len() != DOMAIN_KEY_BITS
                    || &key_bits[depth..] == suffix_bits.as_slice()
                {
                    return Err(AuthCommit1Error::ProofVerificationFailed(
                        "LeafMismatch must differ from queried key",
                    ));
                }
                (hash_leaf(self.domain, suffix_bits, *value_digest), None)
            }
            DomainProofTerminalV1::DivergentExtension {
                prefix_bits,
                child_hash,
            } => {
                if prefix_bits.is_empty()
                    || depth + prefix_bits.len() > DOMAIN_KEY_BITS
                    || &key_bits[depth..depth + prefix_bits.len()] == prefix_bits.as_slice()
                {
                    return Err(AuthCommit1Error::ProofVerificationFailed(
                        "DivergentExtension must disagree with queried key",
                    ));
                }
                (hash_ext(self.domain, prefix_bits, *child_hash), None)
            }
        };

        for step in self.steps.iter().rev() {
            cur_hash = match step {
                DomainProofStepV1::Branch {
                    direction_bit,
                    sibling_hash,
                } => {
                    if *direction_bit == 0 {
                        hash_branch(self.domain, cur_hash, *sibling_hash)
                    } else {
                        hash_branch(self.domain, *sibling_hash, cur_hash)
                    }
                }
                DomainProofStepV1::Extension { prefix_bits } => {
                    hash_ext(self.domain, prefix_bits, cur_hash)
                }
            };
        }
        if cur_hash != expected_root {
            return Err(AuthCommit1Error::ProofVerificationFailed(
                "recomputed root disagrees with expected_root",
            ));
        }
        Ok(outcome)
    }
}

/// Reusable 256-bit domain-separated Candidate A binary Merkle Patricia Trie shared across
/// Waves 13–15 state producers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DomainPatriciaTrieV1 {
    domain: DomainIdV1,
    root: TrieNode,
    entries: BTreeMap<[u8; DOMAIN_KEY_BYTES], Vec<u8>>,
}

impl DomainPatriciaTrieV1 {
    /// Creates an empty trie bound exclusively to `domain`.
    pub const fn empty(domain: DomainIdV1) -> Self {
        Self {
            domain,
            root: TrieNode::Empty,
            entries: BTreeMap::new(),
        }
    }

    /// Cold-rebuilds a canonical trie from unsorted `(key, value)` pairs, rejecting duplicates.
    pub fn from_entries(
        domain: DomainIdV1,
        mut pairs: Vec<([u8; DOMAIN_KEY_BYTES], Vec<u8>)>,
    ) -> Result<Self, AuthCommit1Error> {
        pairs.sort_unstable_by_key(|a| a.0);
        for w in pairs.windows(2) {
            if w[0].0 == w[1].0 {
                return Err(AuthCommit1Error::DuplicateKey);
            }
        }
        let bit_items: Vec<([u8; DOMAIN_KEY_BITS], &[u8])> = pairs
            .iter()
            .map(|(k, v)| (key_to_bits(k), v.as_slice()))
            .collect();
        let root = build_subtree(domain, &bit_items, 0);
        let entries = pairs.into_iter().collect();
        Ok(Self {
            domain,
            root,
            entries,
        })
    }

    /// Returns the domain identifier bound to this trie.
    pub const fn domain(&self) -> DomainIdV1 {
        self.domain
    }

    /// Returns the 32-byte canonical domain-separated root digest.
    pub fn root(&self) -> [u8; 32] {
        self.root.hash(self.domain)
    }

    /// Returns the number of committed entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if no entries are committed.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Looks up the canonical value bytes at `key`.
    pub fn get(&self, key: &[u8; DOMAIN_KEY_BYTES]) -> Option<&[u8]> {
        self.entries.get(key).map(Vec::as_slice)
    }

    /// Iterates over all committed `(key, value)` entries in canonical ascending key order.
    pub fn iter(&self) -> impl Iterator<Item = (&[u8; DOMAIN_KEY_BYTES], &[u8])> {
        self.entries.iter().map(|(k, v)| (k, v.as_slice()))
    }

    /// Incrementally inserts or updates `(key, value)` via canonical path-copy normalization.
    pub fn upsert(&mut self, key: [u8; DOMAIN_KEY_BYTES], value: Vec<u8>) {
        let bits = key_to_bits(&key);
        let old_root = std::mem::replace(&mut self.root, TrieNode::Empty);
        self.root = upsert_node(self.domain, old_root, &bits, 0, value.clone());
        self.entries.insert(key, value);
    }

    /// Incrementally deletes `key` via canonical path-copy normalization, returning `true` if found.
    pub fn delete(&mut self, key: &[u8; DOMAIN_KEY_BYTES]) -> bool {
        let bits = key_to_bits(key);
        let old_root = std::mem::replace(&mut self.root, TrieNode::Empty);
        let (new_root, removed) = delete_node(self.domain, old_root, &bits, 0);
        self.root = new_root;
        if removed {
            self.entries.remove(key);
        }
        removed
    }

    /// Verifies that the incremental root equals a fresh cold [`Self::from_entries`] rebuild.
    pub fn verify_rebuild_parity(&self) -> Result<(), AuthCommit1Error> {
        let pairs: Vec<_> = self.entries.iter().map(|(k, v)| (*k, v.clone())).collect();
        let rebuilt = Self::from_entries(self.domain, pairs)?;
        if rebuilt.root() != self.root() {
            return Err(AuthCommit1Error::RebuildParityMismatch);
        }
        Ok(())
    }

    /// Generates a bounded canonical inclusion or non-membership proof for `key`.
    pub fn prove(
        &self,
        key: &[u8; DOMAIN_KEY_BYTES],
    ) -> Result<DomainPatriciaProofV1, AuthCommit1Error> {
        let key_bits = key_to_bits(key);
        let mut steps = Vec::new();
        let mut node = &self.root;
        let mut depth = 0usize;
        let terminal = loop {
            match node {
                TrieNode::Empty => break DomainProofTerminalV1::Empty,
                TrieNode::Leaf {
                    suffix,
                    value,
                    value_digest,
                    ..
                } => {
                    if suffix.as_slice() == &key_bits[depth..] {
                        break DomainProofTerminalV1::LeafMatch {
                            suffix_bits: suffix.clone(),
                            value: value.clone(),
                        };
                    }
                    break DomainProofTerminalV1::LeafMismatch {
                        suffix_bits: suffix.clone(),
                        value_digest: *value_digest,
                    };
                }
                TrieNode::Extension { prefix, child, .. } => {
                    if key_bits[depth..].starts_with(prefix) {
                        steps.push(DomainProofStepV1::Extension {
                            prefix_bits: prefix.clone(),
                        });
                        depth += prefix.len();
                        node = child;
                    } else {
                        break DomainProofTerminalV1::DivergentExtension {
                            prefix_bits: prefix.clone(),
                            child_hash: child.hash(self.domain),
                        };
                    }
                }
                TrieNode::Branch { left, right, .. } => {
                    let dir = key_bits[depth];
                    if dir == 0 {
                        steps.push(DomainProofStepV1::Branch {
                            direction_bit: 0,
                            sibling_hash: right.hash(self.domain),
                        });
                        node = left;
                    } else {
                        steps.push(DomainProofStepV1::Branch {
                            direction_bit: 1,
                            sibling_hash: left.hash(self.domain),
                        });
                        node = right;
                    }
                    depth += 1;
                }
            }
        };
        let proof = DomainPatriciaProofV1 {
            proof_version: 1,
            domain: self.domain,
            key: *key,
            steps,
            terminal,
        };
        let _ = proof.encode_canonical()?;
        Ok(proof)
    }
}

/// Paired immutable commitment to `AuthorizationRootV1` and `NonceRootV1` (`AUTH-COMMIT-1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AuthCommitRootsV1 {
    /// Commitment schema version (`1`).
    pub roots_version: u16,
    /// Bound `PROFILE-D1` profile identifier.
    pub profile_id: [u8; 32],
    /// Committed ledger height.
    pub block_height: u64,
    /// Number of committed actors in both synchronized trees.
    pub actor_count: u32,
    /// 32-byte `AuthorizationRootV1` (`DomainIdV1::Authorization`).
    pub authorization_root: [u8; 32],
    /// 32-byte `NonceRootV1` (`DomainIdV1::Nonce`).
    pub nonce_root: [u8; 32],
}

impl AuthCommitRootsV1 {
    /// Fixed canonical big-endian wire length (`110` bytes).
    pub const WIRE_LEN: usize = 2 + 32 + 8 + 4 + 32 + 32;

    /// Encodes the paired roots into fixed 110-byte canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], AuthCommit1Error> {
        if self.roots_version != 1 {
            return Err(AuthCommit1Error::InvalidWire("roots_version must equal 1"));
        }
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.roots_version.to_be_bytes());
        out[2..34].copy_from_slice(&self.profile_id);
        out[34..42].copy_from_slice(&self.block_height.to_be_bytes());
        out[42..46].copy_from_slice(&self.actor_count.to_be_bytes());
        out[46..78].copy_from_slice(&self.authorization_root);
        out[78..110].copy_from_slice(&self.nonce_root);
        Ok(out)
    }

    /// Decodes and validates a 110-byte canonical [`AuthCommitRootsV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, AuthCommit1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(AuthCommit1Error::InvalidWire(
                "AuthCommitRootsV1 must be 110 bytes",
            ));
        }
        let roots_version = u16::from_be_bytes([bytes[0], bytes[1]]);
        if roots_version != 1 {
            return Err(AuthCommit1Error::InvalidWire("roots_version must equal 1"));
        }
        Ok(Self {
            roots_version,
            profile_id: bytes[2..34].try_into().expect("32"),
            block_height: u64::from_be_bytes(bytes[34..42].try_into().expect("8")),
            actor_count: u32::from_be_bytes(bytes[42..46].try_into().expect("4")),
            authorization_root: bytes[46..78].try_into().expect("32"),
            nonce_root: bytes[78..110].try_into().expect("32"),
        })
    }

    /// Computes the canonical `Receipt` `ContentTag` for these paired authorization/nonce roots.
    pub fn tag(&self) -> Result<ContentTag, AuthCommit1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            AUTH_COMMIT_1_ROOTS_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

/// Synchronized `IDENTITY-1` runtime registry plus separately typed authenticated
/// `AuthorizationRootV1` and `NonceRootV1` Patricia tries (`AUTH-COMMIT-1`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthCommitStateV1 {
    registry: IdentityRegistryV1,
    auth_trie: DomainPatriciaTrieV1,
    nonce_trie: DomainPatriciaTrieV1,
    block_height: u64,
}

impl AuthCommitStateV1 {
    /// Initializes an authenticated genesis authorization/nonce state from `context` and `actors`.
    pub fn from_genesis(
        context: IdentityContextV1,
        actors: Vec<ActorAuthStateV1>,
    ) -> Result<Self, AuthCommit1Error> {
        let mut registry = IdentityRegistryV1::new(context);
        let mut auth_pairs = Vec::with_capacity(actors.len());
        let mut nonce_pairs = Vec::with_capacity(actors.len());
        for state in actors {
            let key = *state.actor_id.as_bytes();
            let auth_bytes = state.encode_canonical()?;
            let nonce_bytes = state.next_nonce.to_be_bytes().to_vec();
            registry.register_actor(state)?;
            auth_pairs.push((key, auth_bytes));
            nonce_pairs.push((key, nonce_bytes));
        }
        let auth_trie = DomainPatriciaTrieV1::from_entries(DomainIdV1::Authorization, auth_pairs)?;
        let nonce_trie = DomainPatriciaTrieV1::from_entries(DomainIdV1::Nonce, nonce_pairs)?;
        Ok(Self {
            registry,
            auth_trie,
            nonce_trie,
            block_height: 0,
        })
    }

    /// Borrows the underlying [`IdentityRegistryV1`].
    pub const fn registry(&self) -> &IdentityRegistryV1 {
        &self.registry
    }

    /// Borrows the authenticated authorization trie (`DomainIdV1::Authorization`).
    pub const fn auth_trie(&self) -> &DomainPatriciaTrieV1 {
        &self.auth_trie
    }

    /// Borrows the authenticated nonce trie (`DomainIdV1::Nonce`).
    pub const fn nonce_trie(&self) -> &DomainPatriciaTrieV1 {
        &self.nonce_trie
    }

    /// Returns the paired immutable [`AuthCommitRootsV1`] snapshot at current height.
    pub fn roots(&self) -> AuthCommitRootsV1 {
        AuthCommitRootsV1 {
            roots_version: 1,
            profile_id: self.registry.context().profile_id,
            block_height: self.block_height,
            actor_count: self.auth_trie.len() as u32,
            authorization_root: self.auth_trie.root(),
            nonce_root: self.nonce_trie.root(),
        }
    }

    /// Generates an inclusion or non-membership proof for `actor_id` in `AuthorizationRootV1`.
    pub fn prove_authorization(
        &self,
        actor_id: &ActorIdV1,
    ) -> Result<DomainPatriciaProofV1, AuthCommit1Error> {
        self.auth_trie.prove(actor_id.as_bytes())
    }

    /// Generates an inclusion or non-membership proof for `actor_id` in `NonceRootV1`.
    pub fn prove_nonce(
        &self,
        actor_id: &ActorIdV1,
    ) -> Result<DomainPatriciaProofV1, AuthCommit1Error> {
        self.nonce_trie.prove(actor_id.as_bytes())
    }

    /// Verifies cold rebuild parity across both `AuthorizationRootV1` and `NonceRootV1`.
    pub fn verify_rebuild_parity(&self) -> Result<(), AuthCommit1Error> {
        self.auth_trie.verify_rebuild_parity()?;
        self.nonce_trie.verify_rebuild_parity()?;
        Ok(())
    }

    fn sync_actor(
        &mut self,
        actor_id: &ActorIdV1,
        block_height: u64,
    ) -> Result<(), AuthCommit1Error> {
        let state =
            self.registry
                .actor_state(actor_id)
                .ok_or(AuthCommit1Error::AtomicCommitRejected(
                    IdentityDispositionV1::RejectedUnknownActor,
                ))?;
        let auth_bytes = state.encode_canonical()?;
        let nonce_bytes = state.next_nonce.to_be_bytes().to_vec();
        self.auth_trie.upsert(*actor_id.as_bytes(), auth_bytes);
        self.nonce_trie.upsert(*actor_id.as_bytes(), nonce_bytes);
        self.block_height = block_height;
        Ok(())
    }

    /// Atomically evaluates and commits an `IDENTITY-1` transaction, updating both roots only
    /// on [`IdentityDispositionV1::Admitted`] and rolling back cleanly on any rejection.
    pub fn commit_transaction(
        &mut self,
        tx: &TransactionBodyV1,
        signer_actor_id: ActorIdV1,
        preimage: &SigningPreimageV1,
        raw_signatures: &[([u8; 32], [u8; 64])],
        block_height: u64,
    ) -> Result<IdentityAuthReceiptV1, AuthCommit1Error> {
        let mut staged = self.clone();
        let rcpt = staged.registry.authorize_transaction(
            tx,
            signer_actor_id,
            preimage,
            raw_signatures,
            block_height,
        );
        if rcpt.disposition != IdentityDispositionV1::Admitted {
            return Err(AuthCommit1Error::AtomicCommitRejected(rcpt.disposition));
        }
        staged.sync_actor(&tx.actor_id, block_height)?;
        *self = staged;
        Ok(rcpt)
    }

    /// Atomically commits an admin-signed key/delegation rotation or permanent revocation.
    pub fn commit_admin_rotation(
        &mut self,
        actor_id: ActorIdV1,
        next_descriptor: AuthorizationDescriptorV1,
        preimage: &SigningPreimageV1,
        raw_admin_signatures: &[([u8; 32], [u8; 64])],
        block_height: u64,
    ) -> Result<IdentityAuthReceiptV1, AuthCommit1Error> {
        let mut staged = self.clone();
        let rcpt = staged.registry.apply_admin_rotation(
            actor_id,
            next_descriptor.action_policy,
            next_descriptor.admin_policy,
            next_descriptor.delegations,
            next_descriptor.revoked,
            preimage,
            raw_admin_signatures,
            block_height,
        );
        if rcpt.disposition != IdentityDispositionV1::Admitted {
            return Err(AuthCommit1Error::AtomicCommitRejected(rcpt.disposition));
        }
        staged.sync_actor(&actor_id, block_height)?;
        *self = staged;
        Ok(rcpt)
    }

    /// Atomically commits step 1 of timelocked recovery (`InitiateRecovery`).
    pub fn commit_initiate_recovery(
        &mut self,
        actor_id: ActorIdV1,
        proposed_descriptor: AuthorizationDescriptorV1,
        preimage: &SigningPreimageV1,
        raw_guardian_signatures: &[([u8; 32], [u8; 64])],
        block_height: u64,
    ) -> Result<IdentityAuthReceiptV1, AuthCommit1Error> {
        let mut staged = self.clone();
        let rcpt = staged.registry.initiate_recovery(
            actor_id,
            proposed_descriptor,
            preimage,
            raw_guardian_signatures,
            block_height,
        );
        if rcpt.disposition != IdentityDispositionV1::Admitted {
            return Err(AuthCommit1Error::AtomicCommitRejected(rcpt.disposition));
        }
        staged.sync_actor(&actor_id, block_height)?;
        *self = staged;
        Ok(rcpt)
    }

    /// Atomically commits step 2b of timelocked recovery (`FinalizeRecovery`).
    pub fn commit_finalize_recovery(
        &mut self,
        actor_id: ActorIdV1,
        block_height: u64,
    ) -> Result<IdentityAuthReceiptV1, AuthCommit1Error> {
        let mut staged = self.clone();
        let rcpt = staged.registry.finalize_recovery(actor_id, block_height);
        if rcpt.disposition != IdentityDispositionV1::Admitted {
            return Err(AuthCommit1Error::AtomicCommitRejected(rcpt.disposition));
        }
        staged.sync_actor(&actor_id, block_height)?;
        *self = staged;
        Ok(rcpt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_patricia_incremental_upsert_delete_matches_cold_rebuild_and_proofs() {
        let mut trie = DomainPatriciaTrieV1::empty(DomainIdV1::Nonce);
        let keys: Vec<[u8; 32]> = (0u8..24).map(|i| sha256_bytes(&[b'k', i])).collect();
        for (i, &k) in keys.iter().enumerate() {
            trie.upsert(k, (i as u64).to_be_bytes().to_vec());
            trie.verify_rebuild_parity().unwrap();
        }
        let absent = sha256_bytes(b"missing-key");
        let p_inc = trie.prove(&keys[7]).unwrap();
        assert_eq!(
            p_inc
                .verify(DomainIdV1::Nonce, trie.root(), &keys[7])
                .unwrap(),
            Some(7u64.to_be_bytes().to_vec())
        );
        let p_abs = trie.prove(&absent).unwrap();
        assert_eq!(
            p_abs
                .verify(DomainIdV1::Nonce, trie.root(), &absent)
                .unwrap(),
            None
        );
        // Cross-domain proof fails closed.
        assert!(matches!(
            p_inc.verify(DomainIdV1::Authorization, trie.root(), &keys[7]),
            Err(AuthCommit1Error::CrossDomainProofRejected { .. })
        ));
        // Deletions normalize identically to cold rebuild.
        for &k in &keys[0..12] {
            assert!(trie.delete(&k));
            trie.verify_rebuild_parity().unwrap();
        }
        let enc = p_inc.encode_canonical().unwrap();
        assert_eq!(
            DomainPatriciaProofV1::decode_canonical(&enc).unwrap(),
            p_inc
        );
    }
}
