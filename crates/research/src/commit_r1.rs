//! COMMIT-R1: side-by-side authenticated-map research for Bitblock.
//!
//! Compares Candidate A (fixed-key binary Merkle Patricia Trie) and Candidate B
//! (192-level Sparse Merkle Tree) over [`CoordinateKeyV1`] as specified in
//! `docs/AUTHENTICATED_STATE_PLAN.md`.
//!
//! This is research and assay code for the capability node `COMMIT-R1`. It does not
//! replace `world_tag`, authenticate a live block, or mutate the runtime `World`.

use std::collections::BTreeSet;
use std::fmt;
use std::sync::OnceLock;

use bitblock_core::protocol::{ContentTag, TagKind, hash_canonical};
use bitblock_core::world::{Coord, World};
use sha2::{Digest, Sha256};

pub const COMMIT_R1_WORLD_PATRICIA_SCHEMA: &[u8] = b"bitblock/commit-r1/world-patricia/v1";
pub const COMMIT_R1_WORLD_SMT_SCHEMA: &[u8] = b"bitblock/commit-r1/world-smt/v1";
pub const COMMIT_R1_PATRICIA_PROOF_SCHEMA: &[u8] = b"bitblock/commit-r1/patricia-proof/v1";
pub const COMMIT_R1_SMT_PROOF_SCHEMA: &[u8] = b"bitblock/commit-r1/smt-proof/v1";
pub const COMMIT_R1_ASSAY_REPORT_SCHEMA: &[u8] = b"bitblock/commit-r1/assay-report/v1";

pub const KEY_BITS: usize = 192;
pub const KEY_BYTES: usize = 24;

const TREE_DOMAIN: &[u8] = b"bitblock/world-root/v1";
const PROFILE_ID: &[u8] = b"bitblock/commit-profile/r1";
const HASH_SUITE: &[u8] = b"sha256";
const CODEC_VERSION: u16 = 1;

/// Bijective 192-bit coordinate key mapping signed (x, y, z) into unsigned big-endian bytes.
///
/// Order-preserving: flips the sign bit on each 64-bit axis so that unsigned byte comparison
/// matches signed lexicographical ordering (x, then y, then z), including signed extrema.
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CoordinateKeyV1([u8; KEY_BYTES]);

impl CoordinateKeyV1 {
    pub const MIN: Self = Self([0x00; KEY_BYTES]);
    pub const MAX: Self = Self([0xff; KEY_BYTES]);

    /// Encodes signed (x, y, z) into a 192-bit CoordinateKeyV1.
    pub const fn from_coord(coord: Coord) -> Self {
        let x = (coord.x as u64) ^ (1 << 63);
        let y = (coord.y as u64) ^ (1 << 63);
        let z = (coord.z as u64) ^ (1 << 63);
        let xb = x.to_be_bytes();
        let yb = y.to_be_bytes();
        let zb = z.to_be_bytes();
        let mut bytes = [0u8; KEY_BYTES];
        let mut i = 0;
        while i < 8 {
            bytes[i] = xb[i];
            bytes[i + 8] = yb[i];
            bytes[i + 16] = zb[i];
            i += 1;
        }
        Self(bytes)
    }

    /// Decodes a 192-bit CoordinateKeyV1 back into signed (x, y, z).
    pub const fn to_coord(self) -> Coord {
        let mut xb = [0u8; 8];
        let mut yb = [0u8; 8];
        let mut zb = [0u8; 8];
        let mut i = 0;
        while i < 8 {
            xb[i] = self.0[i];
            yb[i] = self.0[i + 8];
            zb[i] = self.0[i + 16];
            i += 1;
        }
        let x = (u64::from_be_bytes(xb) ^ (1 << 63)) as i64;
        let y = (u64::from_be_bytes(yb) ^ (1 << 63)) as i64;
        let z = (u64::from_be_bytes(zb) ^ (1 << 63)) as i64;
        Coord::new(x, y, z)
    }

    pub const fn from_bytes(bytes: [u8; KEY_BYTES]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; KEY_BYTES] {
        &self.0
    }

    /// Reads the bit at decision depth `depth in 0..192` (MSB first).
    pub const fn bit_at(&self, depth: usize) -> u8 {
        if depth >= KEY_BITS {
            return 0;
        }
        (self.0[depth / 8] >> (7 - (depth % 8))) & 1
    }

    /// Returns the number of common leading bits between self and other, starting from `start_depth`.
    pub fn common_prefix_len(&self, other: &Self, start_depth: usize) -> usize {
        let mut match_len = 0;
        for d in start_depth..KEY_BITS {
            if self.bit_at(d) == other.bit_at(d) {
                match_len += 1;
            } else {
                break;
            }
        }
        match_len
    }
}

impl fmt::Debug for CoordinateKeyV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let c = self.to_coord();
        write!(f, "CoordinateKeyV1(({}, {}, {}))", c.x, c.y, c.z)
    }
}

/// The profile-fixed NodeHash framing specified in AUTHENTICATED_STATE_PLAN.md:
///
/// H(profile_id || tree_domain || hash_suite || version || node_type || depth || payload_len || payload)
pub fn node_hash(node_type: u8, depth: u8, payload: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update((PROFILE_ID.len() as u16).to_be_bytes());
    hasher.update(PROFILE_ID);
    hasher.update((TREE_DOMAIN.len() as u16).to_be_bytes());
    hasher.update(TREE_DOMAIN);
    hasher.update((HASH_SUITE.len() as u16).to_be_bytes());
    hasher.update(HASH_SUITE);
    hasher.update(CODEC_VERSION.to_be_bytes());
    hasher.update([node_type, depth]);
    hasher.update((payload.len() as u32).to_be_bytes());
    hasher.update(payload);
    hasher.finalize().into()
}

fn pack_bits_msb(bits: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bits.len().div_ceil(8));
    let mut current_byte = 0u8;
    let mut bit_in_byte = 0;
    for &b in bits {
        current_byte |= (b & 1) << (7 - bit_in_byte);
        bit_in_byte += 1;
        if bit_in_byte == 8 {
            out.push(current_byte);
            current_byte = 0;
            bit_in_byte = 0;
        }
    }
    if bit_in_byte > 0 {
        out.push(current_byte);
    }
    out
}

fn unpack_bits_msb_canonical(packed: &[u8], bit_len: usize) -> Option<Vec<u8>> {
    if packed.len() != bit_len.div_ceil(8) {
        return None;
    }
    let rem = bit_len % 8;
    if rem != 0 {
        let unused_mask = (1u8 << (8 - rem)) - 1;
        if (packed[packed.len() - 1] & unused_mask) != 0 {
            return None;
        }
    }
    let mut bits = Vec::with_capacity(bit_len);
    for i in 0..bit_len {
        let b = (packed[i / 8] >> (7 - (i % 8))) & 1;
        bits.push(b);
    }
    Some(bits)
}

// =========================================================================
// CANDIDATE B: 192-LEVEL SPARSE MERKLE TREE (SMT)
// =========================================================================

const SMT_EMPTY_LEAF_TYPE: u8 = 0;
const SMT_PRESENT_LEAF_TYPE: u8 = 1;
const SMT_BRANCH_TYPE: u8 = 2;

const PRESENT_VALUE_ON: &[u8] = &[1];

/// Precomputed default empty-subtree hashes for depths 192 down to 0.
/// D[192] = NodeHash(SmtEmptyLeaf, 192, [])
/// D[d] = NodeHash(SmtBranch, d, D[d+1] || D[d+1])
static SMT_DEFAULTS: OnceLock<[[u8; 32]; KEY_BITS + 1]> = OnceLock::new();

pub fn smt_default_hashes() -> &'static [[u8; 32]; KEY_BITS + 1] {
    SMT_DEFAULTS.get_or_init(|| {
        let mut d = [[0u8; 32]; KEY_BITS + 1];
        d[KEY_BITS] = node_hash(SMT_EMPTY_LEAF_TYPE, KEY_BITS as u8, &[]);
        let mut depth = KEY_BITS;
        while depth > 0 {
            depth -= 1;
            let mut branch_payload = [0u8; 64];
            branch_payload[0..32].copy_from_slice(&d[depth + 1]);
            branch_payload[32..64].copy_from_slice(&d[depth + 1]);
            d[depth] = node_hash(SMT_BRANCH_TYPE, depth as u8, &branch_payload);
        }
        d
    })
}

pub fn smt_empty_root() -> [u8; 32] {
    smt_default_hashes()[0]
}

pub fn smt_present_leaf_hash() -> [u8; 32] {
    node_hash(SMT_PRESENT_LEAF_TYPE, KEY_BITS as u8, PRESENT_VALUE_ON)
}

/// Sparse node representation caching branch digests for true O(192) path-copy updates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SmtNode {
    Empty {
        depth: usize,
    },
    PresentLeaf,
    Branch {
        depth: usize,
        left: Box<SmtNode>,
        right: Box<SmtNode>,
        hash: [u8; 32],
    },
}

impl SmtNode {
    pub fn hash(&self) -> [u8; 32] {
        match self {
            Self::Empty { depth } => smt_default_hashes()[*depth],
            Self::PresentLeaf => smt_present_leaf_hash(),
            Self::Branch { hash, .. } => *hash,
        }
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty { .. })
    }

    pub fn non_default_branch_count(&self) -> usize {
        match self {
            Self::Empty { .. } | Self::PresentLeaf => 0,
            Self::Branch { left, right, .. } => {
                1 + left.non_default_branch_count() + right.non_default_branch_count()
            }
        }
    }
}

fn make_smt_branch(depth: usize, left: SmtNode, right: SmtNode) -> SmtNode {
    if left.is_empty() && right.is_empty() {
        return SmtNode::Empty { depth };
    }
    let mut payload = [0u8; 64];
    payload[0..32].copy_from_slice(&left.hash());
    payload[32..64].copy_from_slice(&right.hash());
    let hash = node_hash(SMT_BRANCH_TYPE, depth as u8, &payload);
    SmtNode::Branch {
        depth,
        left: Box::new(left),
        right: Box::new(right),
        hash,
    }
}

fn insert_smt_node(node: &SmtNode, key: &CoordinateKeyV1, depth: usize) -> (SmtNode, bool) {
    if depth == KEY_BITS {
        if matches!(node, SmtNode::PresentLeaf) {
            return (SmtNode::PresentLeaf, false);
        }
        return (SmtNode::PresentLeaf, true);
    }

    let bit = key.bit_at(depth);
    match node {
        SmtNode::Empty { .. } => {
            let (child, _) = insert_smt_node(&SmtNode::Empty { depth: depth + 1 }, key, depth + 1);
            let empty_sib = SmtNode::Empty { depth: depth + 1 };
            let branch = if bit == 0 {
                make_smt_branch(depth, child, empty_sib)
            } else {
                make_smt_branch(depth, empty_sib, child)
            };
            (branch, true)
        }
        SmtNode::Branch { left, right, .. } => {
            if bit == 0 {
                let (new_left, inserted) = insert_smt_node(left, key, depth + 1);
                if !inserted {
                    return (node.clone(), false);
                }
                (make_smt_branch(depth, new_left, (**right).clone()), true)
            } else {
                let (new_right, inserted) = insert_smt_node(right, key, depth + 1);
                if !inserted {
                    return (node.clone(), false);
                }
                (make_smt_branch(depth, (**left).clone(), new_right), true)
            }
        }
        SmtNode::PresentLeaf => unreachable!("PresentLeaf only occurs at depth 192"),
    }
}

fn delete_smt_node(node: &SmtNode, key: &CoordinateKeyV1, depth: usize) -> (SmtNode, bool) {
    if depth == KEY_BITS {
        if matches!(node, SmtNode::PresentLeaf) {
            return (SmtNode::Empty { depth: KEY_BITS }, true);
        }
        return (node.clone(), false);
    }

    match node {
        SmtNode::Empty { .. } => (node.clone(), false),
        SmtNode::Branch { left, right, .. } => {
            let bit = key.bit_at(depth);
            if bit == 0 {
                let (new_left, deleted) = delete_smt_node(left, key, depth + 1);
                if !deleted {
                    return (node.clone(), false);
                }
                (make_smt_branch(depth, new_left, (**right).clone()), true)
            } else {
                let (new_right, deleted) = delete_smt_node(right, key, depth + 1);
                if !deleted {
                    return (node.clone(), false);
                }
                (make_smt_branch(depth, (**left).clone(), new_right), true)
            }
        }
        SmtNode::PresentLeaf => unreachable!("PresentLeaf only occurs at depth 192"),
    }
}

fn build_smt_node_subtree(keys: &[CoordinateKeyV1], depth: usize) -> SmtNode {
    if keys.is_empty() {
        return SmtNode::Empty { depth };
    }
    if depth == KEY_BITS {
        return SmtNode::PresentLeaf;
    }

    let split = keys.partition_point(|k| k.bit_at(depth) == 0);
    let (left_keys, right_keys) = keys.split_at(split);

    let left_node = build_smt_node_subtree(left_keys, depth + 1);
    let right_node = build_smt_node_subtree(right_keys, depth + 1);
    make_smt_branch(depth, left_node, right_node)
}

/// 192-level Sparse Merkle Tree over 192-bit CoordinateKeyV1.
#[derive(Clone)]
pub struct SparseMerkleTree {
    root_node: SmtNode,
    occupied: Vec<CoordinateKeyV1>,
}

/// Compact membership / non-membership witness for the Sparse Merkle Tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SmtPointProof {
    /// 192-bit bitmap (24 bytes) where bit `d` (MSB first) is 1 if sibling at child depth d+1
    /// is non-default, and 0 if it equals the default constant D[d+1].
    pub bitmap: [u8; KEY_BYTES],
    /// Carried non-default sibling hashes in increasing decision-depth order (0..191).
    pub non_default_siblings: Vec<[u8; 32]>,
    /// Whether the leaf at depth 192 is claimed present (true) or absent (false).
    pub is_present: bool,
}

impl SmtPointProof {
    pub fn encoded_len(&self) -> usize {
        KEY_BYTES + 1 + 2 + self.non_default_siblings.len() * 32
    }

    /// Serializes the SMT point proof into its canonical wire format:
    /// `bitmap[24] || is_present_u8 || sibling_count_u16_be || siblings[sibling_count * 32]`.
    pub fn to_canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.encoded_len());
        out.extend_from_slice(&self.bitmap);
        out.push(u8::from(self.is_present));
        out.extend_from_slice(&(self.non_default_siblings.len() as u16).to_be_bytes());
        for sib in &self.non_default_siblings {
            out.extend_from_slice(sib);
        }
        out
    }

    /// Decodes an SMT point proof from canonical wire bytes, rejecting non-boolean presence
    /// flags, bitmap popcount mismatches, or trailing/truncated bytes.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < KEY_BYTES + 3 {
            return None;
        }
        let mut bitmap = [0u8; KEY_BYTES];
        bitmap.copy_from_slice(&bytes[0..KEY_BYTES]);
        let present_byte = bytes[KEY_BYTES];
        if present_byte > 1 {
            return None;
        }
        let is_present = present_byte == 1;
        let count = u16::from_be_bytes([bytes[KEY_BYTES + 1], bytes[KEY_BYTES + 2]]) as usize;
        let popcount: usize = bitmap.iter().map(|b| b.count_ones() as usize).sum();
        if count != popcount {
            return None;
        }
        let expected_len = KEY_BYTES + 3 + count * 32;
        if bytes.len() != expected_len {
            return None;
        }
        let mut non_default_siblings = Vec::with_capacity(count);
        for i in 0..count {
            let offset = KEY_BYTES + 3 + i * 32;
            let mut sib = [0u8; 32];
            sib.copy_from_slice(&bytes[offset..offset + 32]);
            non_default_siblings.push(sib);
        }
        Some(Self {
            bitmap,
            non_default_siblings,
            is_present,
        })
    }

    pub fn proof_tag(&self) -> ContentTag {
        let bytes = self.to_canonical_bytes();
        hash_canonical(
            TagKind::Receipt,
            COMMIT_R1_SMT_PROOF_SCHEMA,
            bytes.len() as u64,
            |sink| {
                sink(&bytes);
            },
        )
        .expect("SMT proof tag encoding is canonical")
    }

    /// Returns the decision depths (0..191) that have non-default siblings.
    pub fn active_decision_depths(&self) -> Vec<usize> {
        let mut depths = Vec::with_capacity(self.non_default_siblings.len());
        for d in 0..KEY_BITS {
            if ((self.bitmap[d / 8] >> (7 - (d % 8))) & 1) == 1 {
                depths.push(d);
            }
        }
        depths
    }
}

impl SparseMerkleTree {
    /// Constructs an SMT from an arbitrary set of world coordinates via cold full rebuild.
    pub fn from_world(world: &World) -> Self {
        let mut keys: Vec<CoordinateKeyV1> =
            world.iter_on().map(CoordinateKeyV1::from_coord).collect();
        keys.sort_unstable();
        keys.dedup();
        Self::from_sorted_keys(keys)
    }

    /// Constructs an SMT from pre-sorted unique CoordinateKeyV1 values via cold full rebuild.
    pub fn from_sorted_keys(keys: Vec<CoordinateKeyV1>) -> Self {
        if keys.is_empty() {
            return Self {
                root_node: SmtNode::Empty { depth: 0 },
                occupied: Vec::new(),
            };
        }
        let root_node = build_smt_node_subtree(&keys, 0);
        Self {
            root_node,
            occupied: keys,
        }
    }

    pub fn root(&self) -> [u8; 32] {
        self.root_node.hash()
    }

    pub fn root_node(&self) -> &SmtNode {
        &self.root_node
    }

    pub fn non_default_branch_count(&self) -> usize {
        self.root_node.non_default_branch_count()
    }

    pub fn root_tag(&self) -> ContentTag {
        let root = self.root();
        hash_canonical(TagKind::World, COMMIT_R1_WORLD_SMT_SCHEMA, 32, |sink| {
            sink(&root);
        })
        .expect("SMT root tag encoding is canonical")
    }

    pub fn is_occupied(&self, key: &CoordinateKeyV1) -> bool {
        self.occupied.binary_search(key).is_ok()
    }

    /// Incrementally inserts `key` by updating only the 192-bit path in `self.root_node`.
    pub fn insert(&mut self, key: CoordinateKeyV1) -> bool {
        match self.occupied.binary_search(&key) {
            Ok(_) => false,
            Err(pos) => {
                self.occupied.insert(pos, key);
                let (new_root, inserted) = insert_smt_node(&self.root_node, &key, 0);
                debug_assert!(inserted);
                self.root_node = new_root;
                true
            }
        }
    }

    /// Incrementally deletes `key` by updating and collapsing only the 192-bit path in `self.root_node`.
    pub fn delete(&mut self, key: &CoordinateKeyV1) -> bool {
        match self.occupied.binary_search(key) {
            Ok(pos) => {
                self.occupied.remove(pos);
                let (new_root, deleted) = delete_smt_node(&self.root_node, key, 0);
                debug_assert!(deleted);
                self.root_node = new_root;
                true
            }
            Err(_) => false,
        }
    }

    /// Generates a compact point witness (membership or non-membership) in O(192) using cached node hashes.
    pub fn prove(&self, key: &CoordinateKeyV1) -> SmtPointProof {
        let mut bitmap = [0u8; KEY_BYTES];
        let mut non_default_siblings = Vec::new();
        let mut current = &self.root_node;

        for d in 0..KEY_BITS {
            match current {
                SmtNode::Empty { .. } => break,
                SmtNode::Branch { left, right, .. } => {
                    let key_bit = key.bit_at(d);
                    let (path_child, sibling_child) = if key_bit == 0 {
                        (left.as_ref(), right.as_ref())
                    } else {
                        (right.as_ref(), left.as_ref())
                    };
                    if !sibling_child.is_empty() {
                        bitmap[d / 8] |= 1 << (7 - (d % 8));
                        non_default_siblings.push(sibling_child.hash());
                    }
                    current = path_child;
                }
                SmtNode::PresentLeaf => unreachable!("PresentLeaf only at depth 192"),
            }
        }

        let is_present = matches!(current, SmtNode::PresentLeaf);
        SmtPointProof {
            bitmap,
            non_default_siblings,
            is_present,
        }
    }

    /// Verifies a point proof against an expected root.
    /// Rejects non-canonical proofs that explicitly carry a default sibling hash when the bitmap bit is 1.
    pub fn verify_proof(
        expected_root: [u8; 32],
        key: &CoordinateKeyV1,
        proof: &SmtPointProof,
    ) -> bool {
        let popcount: usize = proof.bitmap.iter().map(|b| b.count_ones() as usize).sum();
        if popcount != proof.non_default_siblings.len() {
            return false;
        }

        let defaults = smt_default_hashes();
        let mut current = if proof.is_present {
            smt_present_leaf_hash()
        } else {
            defaults[KEY_BITS]
        };

        // Reconstruct from depth 191 up to 0
        let mut sibling_read_idx = proof.non_default_siblings.len();
        let mut d = KEY_BITS;
        while d > 0 {
            d -= 1;
            let bit = (proof.bitmap[d / 8] >> (7 - (d % 8))) & 1;
            let sibling = if bit == 1 {
                if sibling_read_idx == 0 {
                    return false;
                }
                sibling_read_idx -= 1;
                let s = proof.non_default_siblings[sibling_read_idx];
                // Explicit default sibling when bitmap bit is 1 is non-canonical and must reject!
                if s == defaults[d + 1] {
                    return false;
                }
                s
            } else {
                defaults[d + 1]
            };

            let key_bit = key.bit_at(d);
            let mut payload = [0u8; 64];
            if key_bit == 0 {
                payload[0..32].copy_from_slice(&current);
                payload[32..64].copy_from_slice(&sibling);
            } else {
                payload[0..32].copy_from_slice(&sibling);
                payload[32..64].copy_from_slice(&current);
            }
            current = node_hash(SMT_BRANCH_TYPE, d as u8, &payload);
        }

        if sibling_read_idx != 0 {
            return false;
        }

        current == expected_root
    }

    /// Computes the canonical deduplicated multiproof wire byte size for a batch of query keys.
    pub fn multiproof_wire_bytes(&self, query_keys: &[CoordinateKeyV1]) -> usize {
        if query_keys.is_empty() {
            return 0;
        }
        let mut sorted: Vec<CoordinateKeyV1> = query_keys.to_vec();
        sorted.sort_unstable();
        sorted.dedup();

        // Collect all traversed `(depth, prefix_key)` nodes on the query paths, and all required external siblings.
        let mut required_siblings: BTreeSet<(usize, [u8; 32])> = BTreeSet::new();
        for k in &sorted {
            let mut current = &self.root_node;
            for d in 0..KEY_BITS {
                match current {
                    SmtNode::Empty { .. } => break,
                    SmtNode::Branch { left, right, .. } => {
                        let bit = k.bit_at(d);
                        let (path_child, sib_child) = if bit == 0 {
                            (left.as_ref(), right.as_ref())
                        } else {
                            (right.as_ref(), left.as_ref())
                        };
                        // Sibling is only external if no other query key traverses into sib_child at depth d
                        let sib_traversed = sorted.iter().any(|other| {
                            other.common_prefix_len(k, 0) >= d && other.bit_at(d) != bit
                        });
                        if !sib_traversed && !sib_child.is_empty() {
                            required_siblings.insert((d + 1, sib_child.hash()));
                        }
                        current = path_child;
                    }
                    SmtNode::PresentLeaf => break,
                }
            }
        }
        // Header (2-byte key count + 1-byte presence bitmap per key + 24-byte shared bitmap header) + unique 32-byte sibling hashes
        2 + sorted.len().div_ceil(8) + KEY_BYTES + required_siblings.len() * 32
    }
}

// =========================================================================
// CANDIDATE A: FIXED-KEY BINARY MERKLE PATRICIA TRIE (MPT)
// =========================================================================

const MPT_EMPTY_TYPE: u8 = 0;
const MPT_LEAF_TYPE: u8 = 1;
const MPT_EXTENSION_TYPE: u8 = 2;
const MPT_BRANCH_TYPE: u8 = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatriciaNode {
    Empty,
    Leaf {
        depth: usize,
        key: CoordinateKeyV1,
    },
    Extension {
        depth: usize,
        shared_bits: Vec<u8>, // 0 or 1 per bit
        child: Box<PatriciaNode>,
    },
    Branch {
        depth: usize,
        left: Box<PatriciaNode>,
        right: Box<PatriciaNode>,
    },
}

impl PatriciaNode {
    pub fn hash(&self) -> [u8; 32] {
        match self {
            Self::Empty => node_hash(MPT_EMPTY_TYPE, 0, &[]),
            Self::Leaf { depth, key } => {
                let suffix_len = KEY_BITS - depth;
                let mut payload = Vec::with_capacity(2 + suffix_len.div_ceil(8) + 1);
                payload.extend_from_slice(&(suffix_len as u16).to_be_bytes());
                let mut current_byte = 0u8;
                let mut bit_in_byte = 0;
                for d in *depth..KEY_BITS {
                    current_byte |= key.bit_at(d) << (7 - bit_in_byte);
                    bit_in_byte += 1;
                    if bit_in_byte == 8 {
                        payload.push(current_byte);
                        current_byte = 0;
                        bit_in_byte = 0;
                    }
                }
                if bit_in_byte > 0 {
                    payload.push(current_byte);
                }
                payload.extend_from_slice(PRESENT_VALUE_ON);
                node_hash(MPT_LEAF_TYPE, *depth as u8, &payload)
            }
            Self::Extension {
                depth,
                shared_bits,
                child,
            } => {
                let mut payload = Vec::with_capacity(2 + shared_bits.len().div_ceil(8) + 32);
                payload.extend_from_slice(&(shared_bits.len() as u16).to_be_bytes());
                payload.extend_from_slice(&pack_bits_msb(shared_bits));
                let child_hash = child.hash();
                payload.extend_from_slice(&child_hash);
                node_hash(MPT_EXTENSION_TYPE, *depth as u8, &payload)
            }
            Self::Branch { depth, left, right } => {
                let mut payload = [0u8; 64];
                payload[0..32].copy_from_slice(&left.hash());
                payload[32..64].copy_from_slice(&right.hash());
                node_hash(MPT_BRANCH_TYPE, *depth as u8, &payload)
            }
        }
    }

    /// Returns `(total_nodes, extension_nodes, branch_nodes)` in this subtree.
    pub fn node_counts(&self) -> (usize, usize, usize) {
        match self {
            Self::Empty => (0, 0, 0),
            Self::Leaf { .. } => (1, 0, 0),
            Self::Extension { child, .. } => {
                let (t, e, b) = child.node_counts();
                (1 + t, 1 + e, b)
            }
            Self::Branch { left, right, .. } => {
                let (lt, le, lb) = left.node_counts();
                let (rt, re, rb) = right.node_counts();
                (1 + lt + rt, le + re, 1 + lb + rb)
            }
        }
    }
}

/// True incremental path-copy insertion into a canonical [`PatriciaNode`].
/// Returns `(updated_node, was_inserted, rehashed_nodes_along_path)`.
fn insert_patricia_node(
    node: &PatriciaNode,
    key: CoordinateKeyV1,
    depth: usize,
) -> (PatriciaNode, bool, usize) {
    match node {
        PatriciaNode::Empty => (PatriciaNode::Leaf { depth: 0, key }, true, 1),
        PatriciaNode::Leaf {
            depth: d,
            key: existing_key,
        } => {
            debug_assert_eq!(*d, depth);
            if *existing_key == key {
                return (node.clone(), false, 0);
            }
            let common = existing_key.common_prefix_len(&key, *d);
            let split_depth = *d + common;
            let existing_leaf = PatriciaNode::Leaf {
                depth: split_depth + 1,
                key: *existing_key,
            };
            let new_leaf = PatriciaNode::Leaf {
                depth: split_depth + 1,
                key,
            };
            let (left, right) = if existing_key.bit_at(split_depth) == 0 {
                (Box::new(existing_leaf), Box::new(new_leaf))
            } else {
                (Box::new(new_leaf), Box::new(existing_leaf))
            };
            let branch = PatriciaNode::Branch {
                depth: split_depth,
                left,
                right,
            };
            if common == 0 {
                // 2 new leaves + 1 branch = 3 node hashes
                (branch, true, 3)
            } else {
                let mut shared_bits = Vec::with_capacity(common);
                for bit_d in *d..split_depth {
                    shared_bits.push(key.bit_at(bit_d));
                }
                (
                    PatriciaNode::Extension {
                        depth: *d,
                        shared_bits,
                        child: Box::new(branch),
                    },
                    true,
                    4,
                )
            }
        }
        PatriciaNode::Extension {
            depth: d,
            shared_bits,
            child,
        } => {
            debug_assert_eq!(*d, depth);
            let mut matched = 0;
            while matched < shared_bits.len() && key.bit_at(*d + matched) == shared_bits[matched] {
                matched += 1;
            }
            if matched == shared_bits.len() {
                let (new_child, inserted, child_hashes) =
                    insert_patricia_node(child, key, *d + shared_bits.len());
                if !inserted {
                    return (node.clone(), false, 0);
                }
                return (
                    PatriciaNode::Extension {
                        depth: *d,
                        shared_bits: shared_bits.clone(),
                        child: Box::new(new_child),
                    },
                    true,
                    child_hashes + 1,
                );
            }

            // Diverges inside `shared_bits` at `split_depth = *d + matched`
            let split_depth = *d + matched;
            let rem_len = shared_bits.len() - matched - 1;
            let (existing_sub, extra_hash) = if rem_len == 0 {
                ((**child).clone(), 0)
            } else {
                (
                    PatriciaNode::Extension {
                        depth: split_depth + 1,
                        shared_bits: shared_bits[matched + 1..].to_vec(),
                        child: child.clone(),
                    },
                    1,
                )
            };
            let new_leaf = PatriciaNode::Leaf {
                depth: split_depth + 1,
                key,
            };
            let (left, right) = if shared_bits[matched] == 0 {
                (Box::new(existing_sub), Box::new(new_leaf))
            } else {
                (Box::new(new_leaf), Box::new(existing_sub))
            };
            let branch = PatriciaNode::Branch {
                depth: split_depth,
                left,
                right,
            };
            if matched == 0 {
                (branch, true, 2 + extra_hash)
            } else {
                (
                    PatriciaNode::Extension {
                        depth: *d,
                        shared_bits: shared_bits[..matched].to_vec(),
                        child: Box::new(branch),
                    },
                    true,
                    3 + extra_hash,
                )
            }
        }
        PatriciaNode::Branch {
            depth: d,
            left,
            right,
        } => {
            debug_assert_eq!(*d, depth);
            if key.bit_at(*d) == 0 {
                let (new_left, inserted, h) = insert_patricia_node(left, key, *d + 1);
                if !inserted {
                    return (node.clone(), false, 0);
                }
                (
                    PatriciaNode::Branch {
                        depth: *d,
                        left: Box::new(new_left),
                        right: right.clone(),
                    },
                    true,
                    h + 1,
                )
            } else {
                let (new_right, inserted, h) = insert_patricia_node(right, key, *d + 1);
                if !inserted {
                    return (node.clone(), false, 0);
                }
                (
                    PatriciaNode::Branch {
                        depth: *d,
                        left: left.clone(),
                        right: Box::new(new_right),
                    },
                    true,
                    h + 1,
                )
            }
        }
    }
}

/// Normalizes the surviving child of a deleted `PatriciaNode::Branch` at depth `d`
/// by prepending the survivor's direction bit (`0` or `1`).
fn prepend_bit_to_patricia_survivor(survivor: PatriciaNode, d: usize, bit: u8) -> PatriciaNode {
    match survivor {
        PatriciaNode::Leaf { key, .. } => PatriciaNode::Leaf { depth: d, key },
        PatriciaNode::Extension {
            shared_bits, child, ..
        } => {
            let mut new_bits = Vec::with_capacity(1 + shared_bits.len());
            new_bits.push(bit);
            new_bits.extend_from_slice(&shared_bits);
            PatriciaNode::Extension {
                depth: d,
                shared_bits: new_bits,
                child,
            }
        }
        branch @ PatriciaNode::Branch { .. } => PatriciaNode::Extension {
            depth: d,
            shared_bits: vec![bit],
            child: Box::new(branch),
        },
        PatriciaNode::Empty => unreachable!("branch survivor is never empty"),
    }
}

/// True incremental path-copy deletion from a canonical [`PatriciaNode`].
fn delete_patricia_node(
    node: &PatriciaNode,
    key: &CoordinateKeyV1,
    depth: usize,
) -> (PatriciaNode, bool) {
    match node {
        PatriciaNode::Empty => (PatriciaNode::Empty, false),
        PatriciaNode::Leaf {
            depth: d,
            key: existing_key,
        } => {
            debug_assert_eq!(*d, depth);
            if existing_key == key {
                (PatriciaNode::Empty, true)
            } else {
                (node.clone(), false)
            }
        }
        PatriciaNode::Branch {
            depth: d,
            left,
            right,
        } => {
            debug_assert_eq!(*d, depth);
            if key.bit_at(*d) == 0 {
                let (new_left, deleted) = delete_patricia_node(left, key, *d + 1);
                if !deleted {
                    return (node.clone(), false);
                }
                if matches!(new_left, PatriciaNode::Empty) {
                    return (
                        prepend_bit_to_patricia_survivor((**right).clone(), *d, 1),
                        true,
                    );
                }
                (
                    PatriciaNode::Branch {
                        depth: *d,
                        left: Box::new(new_left),
                        right: right.clone(),
                    },
                    true,
                )
            } else {
                let (new_right, deleted) = delete_patricia_node(right, key, *d + 1);
                if !deleted {
                    return (node.clone(), false);
                }
                if matches!(new_right, PatriciaNode::Empty) {
                    return (
                        prepend_bit_to_patricia_survivor((**left).clone(), *d, 0),
                        true,
                    );
                }
                (
                    PatriciaNode::Branch {
                        depth: *d,
                        left: left.clone(),
                        right: Box::new(new_right),
                    },
                    true,
                )
            }
        }
        PatriciaNode::Extension {
            depth: d,
            shared_bits,
            child,
        } => {
            debug_assert_eq!(*d, depth);
            for (i, &b) in shared_bits.iter().enumerate() {
                if key.bit_at(*d + i) != b {
                    return (node.clone(), false);
                }
            }
            let (new_child, deleted) = delete_patricia_node(child, key, *d + shared_bits.len());
            if !deleted {
                return (node.clone(), false);
            }
            match new_child {
                PatriciaNode::Leaf { key: sk, .. } => {
                    (PatriciaNode::Leaf { depth: *d, key: sk }, true)
                }
                PatriciaNode::Extension {
                    shared_bits: child_bits,
                    child: grandchild,
                    ..
                } => {
                    let mut merged = Vec::with_capacity(shared_bits.len() + child_bits.len());
                    merged.extend_from_slice(shared_bits);
                    merged.extend_from_slice(&child_bits);
                    (
                        PatriciaNode::Extension {
                            depth: *d,
                            shared_bits: merged,
                            child: grandchild,
                        },
                        true,
                    )
                }
                other_branch => (
                    PatriciaNode::Extension {
                        depth: *d,
                        shared_bits: shared_bits.clone(),
                        child: Box::new(other_branch),
                    },
                    true,
                ),
            }
        }
    }
}

/// Fixed-key binary Merkle Patricia Trie over 192-bit CoordinateKeyV1.
#[derive(Clone)]
pub struct MerklePatriciaTrie {
    root_node: PatriciaNode,
    occupied: Vec<CoordinateKeyV1>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatriciaProofStep {
    Branch {
        depth: usize,
        direction: u8,
        sibling_hash: [u8; 32],
    },
    Extension {
        depth: usize,
        shared_bits: Vec<u8>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatriciaTerminal {
    EmptyRoot,
    Leaf {
        depth: usize,
        key: CoordinateKeyV1,
    },
    DivergentExtension {
        depth: usize,
        shared_bits: Vec<u8>,
        child_hash: [u8; 32],
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatriciaPointProof {
    pub steps: Vec<PatriciaProofStep>,
    pub terminal: PatriciaTerminal,
}

const STEP_TAG_BRANCH: u8 = 1;
const STEP_TAG_EXTENSION: u8 = 2;
const TERM_TAG_EMPTY: u8 = 0;
const TERM_TAG_LEAF: u8 = 1;
const TERM_TAG_DIVERGENT_EXT: u8 = 2;

impl PatriciaPointProof {
    pub fn encoded_len(&self) -> usize {
        let mut len = 2 + 1; // steps_count_u16 + terminal tag
        match &self.terminal {
            PatriciaTerminal::EmptyRoot => {}
            PatriciaTerminal::Leaf { depth: _, key: _ } => {
                len += 1 + KEY_BYTES;
            }
            PatriciaTerminal::DivergentExtension {
                depth: _,
                shared_bits,
                child_hash: _,
            } => {
                len += 1 + 2 + shared_bits.len().div_ceil(8) + 32;
            }
        }
        for step in &self.steps {
            match step {
                PatriciaProofStep::Branch { .. } => len += 1 + 1 + 1 + 32,
                PatriciaProofStep::Extension { shared_bits, .. } => {
                    len += 1 + 1 + 2 + shared_bits.len().div_ceil(8)
                }
            }
        }
        len
    }

    /// Serializes the Patricia point proof into canonical wire bytes.
    pub fn to_canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.encoded_len());
        out.extend_from_slice(&(self.steps.len() as u16).to_be_bytes());
        for step in &self.steps {
            match step {
                PatriciaProofStep::Branch {
                    depth,
                    direction,
                    sibling_hash,
                } => {
                    out.push(STEP_TAG_BRANCH);
                    out.push(*depth as u8);
                    out.push(*direction);
                    out.extend_from_slice(sibling_hash);
                }
                PatriciaProofStep::Extension { depth, shared_bits } => {
                    out.push(STEP_TAG_EXTENSION);
                    out.push(*depth as u8);
                    out.extend_from_slice(&(shared_bits.len() as u16).to_be_bytes());
                    out.extend_from_slice(&pack_bits_msb(shared_bits));
                }
            }
        }
        match &self.terminal {
            PatriciaTerminal::EmptyRoot => {
                out.push(TERM_TAG_EMPTY);
            }
            PatriciaTerminal::Leaf { depth, key } => {
                out.push(TERM_TAG_LEAF);
                out.push(*depth as u8);
                out.extend_from_slice(key.as_bytes());
            }
            PatriciaTerminal::DivergentExtension {
                depth,
                shared_bits,
                child_hash,
            } => {
                out.push(TERM_TAG_DIVERGENT_EXT);
                out.push(*depth as u8);
                out.extend_from_slice(&(shared_bits.len() as u16).to_be_bytes());
                out.extend_from_slice(&pack_bits_msb(shared_bits));
                out.extend_from_slice(child_hash);
            }
        }
        out
    }

    /// Decodes a Patricia point proof from canonical wire bytes, rejecting malformed tags,
    /// nonzero unused padding bits, out-of-range depths, or trailing bytes.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 3 {
            return None;
        }
        let step_count = u16::from_be_bytes([bytes[0], bytes[1]]) as usize;
        if step_count > KEY_BITS {
            return None;
        }
        let mut cursor = 2usize;
        let mut steps = Vec::with_capacity(step_count);
        for _ in 0..step_count {
            if cursor >= bytes.len() {
                return None;
            }
            let tag = bytes[cursor];
            cursor += 1;
            match tag {
                STEP_TAG_BRANCH => {
                    if cursor + 1 + 1 + 32 > bytes.len() {
                        return None;
                    }
                    let depth = bytes[cursor] as usize;
                    let direction = bytes[cursor + 1];
                    if depth >= KEY_BITS || direction > 1 {
                        return None;
                    }
                    let mut sibling_hash = [0u8; 32];
                    sibling_hash.copy_from_slice(&bytes[cursor + 2..cursor + 34]);
                    cursor += 34;
                    steps.push(PatriciaProofStep::Branch {
                        depth,
                        direction,
                        sibling_hash,
                    });
                }
                STEP_TAG_EXTENSION => {
                    if cursor + 3 > bytes.len() {
                        return None;
                    }
                    let depth = bytes[cursor] as usize;
                    let bit_len =
                        u16::from_be_bytes([bytes[cursor + 1], bytes[cursor + 2]]) as usize;
                    cursor += 3;
                    if bit_len == 0 || depth + bit_len >= KEY_BITS {
                        return None;
                    }
                    let packed_len = bit_len.div_ceil(8);
                    if cursor + packed_len > bytes.len() {
                        return None;
                    }
                    let shared_bits =
                        unpack_bits_msb_canonical(&bytes[cursor..cursor + packed_len], bit_len)?;
                    cursor += packed_len;
                    steps.push(PatriciaProofStep::Extension { depth, shared_bits });
                }
                _ => return None,
            }
        }

        if cursor >= bytes.len() {
            return None;
        }
        let term_tag = bytes[cursor];
        cursor += 1;
        let terminal = match term_tag {
            TERM_TAG_EMPTY => PatriciaTerminal::EmptyRoot,
            TERM_TAG_LEAF => {
                if cursor + 1 + KEY_BYTES != bytes.len() {
                    return None;
                }
                let depth = bytes[cursor] as usize;
                if depth > KEY_BITS {
                    return None;
                }
                let mut kb = [0u8; KEY_BYTES];
                kb.copy_from_slice(&bytes[cursor + 1..cursor + 1 + KEY_BYTES]);
                cursor += 1 + KEY_BYTES;
                PatriciaTerminal::Leaf {
                    depth,
                    key: CoordinateKeyV1::from_bytes(kb),
                }
            }
            TERM_TAG_DIVERGENT_EXT => {
                if cursor + 3 > bytes.len() {
                    return None;
                }
                let depth = bytes[cursor] as usize;
                let bit_len = u16::from_be_bytes([bytes[cursor + 1], bytes[cursor + 2]]) as usize;
                cursor += 3;
                if bit_len == 0 || depth + bit_len >= KEY_BITS {
                    return None;
                }
                let packed_len = bit_len.div_ceil(8);
                if cursor + packed_len + 32 != bytes.len() {
                    return None;
                }
                let shared_bits =
                    unpack_bits_msb_canonical(&bytes[cursor..cursor + packed_len], bit_len)?;
                cursor += packed_len;
                let mut child_hash = [0u8; 32];
                child_hash.copy_from_slice(&bytes[cursor..cursor + 32]);
                cursor += 32;
                PatriciaTerminal::DivergentExtension {
                    depth,
                    shared_bits,
                    child_hash,
                }
            }
            _ => return None,
        };

        if cursor != bytes.len() {
            return None;
        }

        Some(Self { steps, terminal })
    }

    pub fn proof_tag(&self) -> ContentTag {
        let bytes = self.to_canonical_bytes();
        hash_canonical(
            TagKind::Receipt,
            COMMIT_R1_PATRICIA_PROOF_SCHEMA,
            bytes.len() as u64,
            |sink| {
                sink(&bytes);
            },
        )
        .expect("Patricia proof tag encoding is canonical")
    }
}

impl MerklePatriciaTrie {
    pub fn from_world(world: &World) -> Self {
        let mut keys: Vec<CoordinateKeyV1> =
            world.iter_on().map(CoordinateKeyV1::from_coord).collect();
        keys.sort_unstable();
        keys.dedup();
        Self::from_sorted_keys(keys)
    }

    pub fn from_sorted_keys(keys: Vec<CoordinateKeyV1>) -> Self {
        if keys.is_empty() {
            return Self {
                root_node: PatriciaNode::Empty,
                occupied: Vec::new(),
            };
        }
        let root_node = build_patricia_subtree(&keys, 0);
        Self {
            root_node,
            occupied: keys,
        }
    }

    pub fn root(&self) -> [u8; 32] {
        self.root_node.hash()
    }

    pub fn root_node(&self) -> &PatriciaNode {
        &self.root_node
    }

    pub fn node_counts(&self) -> (usize, usize, usize) {
        self.root_node.node_counts()
    }

    pub fn root_tag(&self) -> ContentTag {
        let r = self.root();
        hash_canonical(
            TagKind::World,
            COMMIT_R1_WORLD_PATRICIA_SCHEMA,
            32,
            |sink| {
                sink(&r);
            },
        )
        .expect("Patricia root tag encoding is canonical")
    }

    pub fn is_occupied(&self, key: &CoordinateKeyV1) -> bool {
        self.occupied.binary_search(key).is_ok()
    }

    /// Incrementally inserts `key` into `self.root_node` via canonical path-copy normalization.
    pub fn insert(&mut self, key: CoordinateKeyV1) -> bool {
        match self.occupied.binary_search(&key) {
            Ok(_) => false,
            Err(pos) => {
                self.occupied.insert(pos, key);
                let (new_root, inserted, _) = insert_patricia_node(&self.root_node, key, 0);
                debug_assert!(inserted);
                self.root_node = new_root;
                true
            }
        }
    }

    /// Incrementally inserts `key` and returns `(inserted, rehashed_nodes_count)`.
    pub fn insert_counted(&mut self, key: CoordinateKeyV1) -> (bool, usize) {
        match self.occupied.binary_search(&key) {
            Ok(_) => (false, 0),
            Err(pos) => {
                self.occupied.insert(pos, key);
                let (new_root, inserted, hashes) = insert_patricia_node(&self.root_node, key, 0);
                debug_assert!(inserted);
                self.root_node = new_root;
                (true, hashes)
            }
        }
    }

    /// Incrementally deletes `key` from `self.root_node` via canonical branch-collapse & extension-merge normalization.
    pub fn delete(&mut self, key: &CoordinateKeyV1) -> bool {
        match self.occupied.binary_search(key) {
            Ok(pos) => {
                self.occupied.remove(pos);
                let (new_root, deleted) = delete_patricia_node(&self.root_node, key, 0);
                debug_assert!(deleted);
                self.root_node = new_root;
                true
            }
            Err(_) => false,
        }
    }

    /// Generates a point proof (membership or non-membership) for a query key.
    pub fn prove(&self, key: &CoordinateKeyV1) -> PatriciaPointProof {
        let mut steps = Vec::new();
        let mut current = &self.root_node;

        loop {
            match current {
                PatriciaNode::Empty => {
                    return PatriciaPointProof {
                        steps,
                        terminal: PatriciaTerminal::EmptyRoot,
                    };
                }
                PatriciaNode::Leaf { depth, key: lk } => {
                    return PatriciaPointProof {
                        steps,
                        terminal: PatriciaTerminal::Leaf {
                            depth: *depth,
                            key: *lk,
                        },
                    };
                }
                PatriciaNode::Extension {
                    depth,
                    shared_bits,
                    child,
                } => {
                    let mut matches = true;
                    for (i, &b) in shared_bits.iter().enumerate() {
                        if key.bit_at(depth + i) != b {
                            matches = false;
                            break;
                        }
                    }
                    if !matches {
                        return PatriciaPointProof {
                            steps,
                            terminal: PatriciaTerminal::DivergentExtension {
                                depth: *depth,
                                shared_bits: shared_bits.clone(),
                                child_hash: child.hash(),
                            },
                        };
                    }
                    steps.push(PatriciaProofStep::Extension {
                        depth: *depth,
                        shared_bits: shared_bits.clone(),
                    });
                    current = child;
                }
                PatriciaNode::Branch { depth, left, right } => {
                    let dir = key.bit_at(*depth);
                    if dir == 0 {
                        steps.push(PatriciaProofStep::Branch {
                            depth: *depth,
                            direction: 0,
                            sibling_hash: right.hash(),
                        });
                        current = left;
                    } else {
                        steps.push(PatriciaProofStep::Branch {
                            depth: *depth,
                            direction: 1,
                            sibling_hash: left.hash(),
                        });
                        current = right;
                    }
                }
            }
        }
    }

    /// Verifies a Patricia point proof against an expected root and query key.
    /// Returns `Some(true)` if key is proved present, `Some(false)` if proved absent,
    /// or `None` if the proof is malformed, non-canonical, or does not follow `key`'s bit path.
    pub fn verify_proof(
        expected_root: [u8; 32],
        key: &CoordinateKeyV1,
        proof: &PatriciaPointProof,
    ) -> Option<bool> {
        let empty_node_hash = node_hash(MPT_EMPTY_TYPE, 0, &[]);

        let (mut current_hash, is_present, mut current_depth, mut prev_was_extension) =
            match &proof.terminal {
                PatriciaTerminal::EmptyRoot => {
                    if !proof.steps.is_empty() {
                        return None;
                    }
                    (empty_node_hash, false, 0, false)
                }
                PatriciaTerminal::Leaf { depth, key: lk } => {
                    if *depth > KEY_BITS {
                        return None;
                    }
                    if *depth == 0 && !proof.steps.is_empty() {
                        return None;
                    }
                    // A canonical Leaf at depth > 0 must sit directly under a Branch (never under an Extension).
                    if *depth > 0
                        && !matches!(proof.steps.last(), Some(PatriciaProofStep::Branch { .. }))
                    {
                        return None;
                    }
                    // Soundness check: the leaf key `lk` must match the query `key` on all consumed prefix bits `0..*depth`.
                    if lk.common_prefix_len(key, 0) < *depth {
                        return None;
                    }
                    let suffix_len = KEY_BITS - depth;
                    let mut payload = Vec::with_capacity(2 + suffix_len.div_ceil(8) + 1);
                    payload.extend_from_slice(&(suffix_len as u16).to_be_bytes());
                    let mut current_byte = 0u8;
                    let mut bit_in_byte = 0;
                    for d in *depth..KEY_BITS {
                        current_byte |= lk.bit_at(d) << (7 - bit_in_byte);
                        bit_in_byte += 1;
                        if bit_in_byte == 8 {
                            payload.push(current_byte);
                            current_byte = 0;
                            bit_in_byte = 0;
                        }
                    }
                    if bit_in_byte > 0 {
                        payload.push(current_byte);
                    }
                    payload.extend_from_slice(PRESENT_VALUE_ON);
                    let h = node_hash(MPT_LEAF_TYPE, *depth as u8, &payload);
                    let present = lk == key;
                    (h, present, *depth, false)
                }
                PatriciaTerminal::DivergentExtension {
                    depth,
                    shared_bits,
                    child_hash,
                } => {
                    if shared_bits.is_empty()
                        || *depth + shared_bits.len() >= KEY_BITS
                        || *child_hash == empty_node_hash
                    {
                        return None;
                    }
                    // Soundness check: `key` MUST actually diverge from `shared_bits` within `0..shared_bits.len()`.
                    let mut diverges = false;
                    for (i, &b) in shared_bits.iter().enumerate() {
                        if b > 1 {
                            return None;
                        }
                        if key.bit_at(*depth + i) != b {
                            diverges = true;
                        }
                    }
                    if !diverges {
                        return None;
                    }
                    let mut payload = Vec::with_capacity(2 + shared_bits.len().div_ceil(8) + 32);
                    payload.extend_from_slice(&(shared_bits.len() as u16).to_be_bytes());
                    payload.extend_from_slice(&pack_bits_msb(shared_bits));
                    payload.extend_from_slice(child_hash);
                    let h = node_hash(MPT_EXTENSION_TYPE, *depth as u8, &payload);
                    (h, false, *depth, true)
                }
            };

        // Reconstruct steps from terminal back to root
        for step in proof.steps.iter().rev() {
            match step {
                PatriciaProofStep::Extension { depth, shared_bits } => {
                    // Two adjacent extensions are non-canonical
                    if prev_was_extension {
                        return None;
                    }
                    if shared_bits.is_empty() || *depth + shared_bits.len() != current_depth {
                        return None;
                    }
                    // Soundness check: `key` must match every bit of a traversed Extension
                    for (i, &b) in shared_bits.iter().enumerate() {
                        if b > 1 || key.bit_at(*depth + i) != b {
                            return None;
                        }
                    }
                    let mut payload = Vec::with_capacity(2 + shared_bits.len().div_ceil(8) + 32);
                    payload.extend_from_slice(&(shared_bits.len() as u16).to_be_bytes());
                    payload.extend_from_slice(&pack_bits_msb(shared_bits));
                    payload.extend_from_slice(&current_hash);
                    current_hash = node_hash(MPT_EXTENSION_TYPE, *depth as u8, &payload);
                    current_depth = *depth;
                    prev_was_extension = true;
                }
                PatriciaProofStep::Branch {
                    depth,
                    direction,
                    sibling_hash,
                } => {
                    if *depth + 1 != current_depth
                        || *direction > 1
                        || *sibling_hash == empty_node_hash
                    {
                        return None;
                    }
                    // Soundness check: branch direction must equal `key.bit_at(*depth)`
                    if key.bit_at(*depth) != *direction {
                        return None;
                    }
                    let mut payload = [0u8; 64];
                    if *direction == 0 {
                        payload[0..32].copy_from_slice(&current_hash);
                        payload[32..64].copy_from_slice(sibling_hash);
                    } else {
                        payload[0..32].copy_from_slice(sibling_hash);
                        payload[32..64].copy_from_slice(&current_hash);
                    }
                    current_hash = node_hash(MPT_BRANCH_TYPE, *depth as u8, &payload);
                    current_depth = *depth;
                    prev_was_extension = false;
                }
            }
        }

        if current_depth != 0 || current_hash != expected_root {
            return None;
        }

        Some(is_present)
    }

    /// Computes the canonical deduplicated multiproof wire byte size for a batch of query keys.
    pub fn multiproof_wire_bytes(&self, query_keys: &[CoordinateKeyV1]) -> usize {
        if query_keys.is_empty() {
            return 0;
        }
        let mut sorted: Vec<CoordinateKeyV1> = query_keys.to_vec();
        sorted.sort_unstable();
        sorted.dedup();

        let mut unique_ext_bytes = 0usize;
        let mut seen_extensions: BTreeSet<(usize, Vec<u8>)> = BTreeSet::new();
        let mut external_siblings: BTreeSet<(usize, u8, [u8; 32])> = BTreeSet::new();
        let mut terminal_leaves: BTreeSet<(usize, CoordinateKeyV1)> = BTreeSet::new();

        for k in &sorted {
            let proof = self.prove(k);
            for step in &proof.steps {
                match step {
                    PatriciaProofStep::Extension { depth, shared_bits } => {
                        if seen_extensions.insert((*depth, shared_bits.clone())) {
                            unique_ext_bytes += 1 + 1 + 2 + shared_bits.len().div_ceil(8);
                        }
                    }
                    PatriciaProofStep::Branch {
                        depth,
                        direction,
                        sibling_hash,
                    } => {
                        let sib_traversed = sorted.iter().any(|other| {
                            other.common_prefix_len(k, 0) >= *depth
                                && other.bit_at(*depth) != *direction
                        });
                        if !sib_traversed {
                            external_siblings.insert((*depth, *direction, *sibling_hash));
                        }
                    }
                }
            }
            match proof.terminal {
                PatriciaTerminal::EmptyRoot => {}
                PatriciaTerminal::Leaf { depth, key: lk } => {
                    terminal_leaves.insert((depth, lk));
                }
                PatriciaTerminal::DivergentExtension {
                    depth,
                    shared_bits,
                    child_hash,
                } => {
                    if seen_extensions.insert((depth, shared_bits.clone())) {
                        unique_ext_bytes += 1 + 1 + 2 + shared_bits.len().div_ceil(8);
                    }
                    external_siblings.insert((depth, 2, child_hash));
                }
            }
        }

        2 + unique_ext_bytes
            + external_siblings.len() * (1 + 1 + 32)
            + terminal_leaves.len() * (1 + 1 + KEY_BYTES)
    }
}

fn build_patricia_subtree(keys: &[CoordinateKeyV1], depth: usize) -> PatriciaNode {
    if keys.is_empty() {
        return PatriciaNode::Empty;
    }
    if keys.len() == 1 {
        return PatriciaNode::Leaf {
            depth,
            key: keys[0],
        };
    }

    // Find longest common prefix among all keys starting from `depth`
    let first = &keys[0];
    let last = &keys[keys.len() - 1];
    let common = first.common_prefix_len(last, depth);

    if common > 0 {
        let mut shared_bits = Vec::with_capacity(common);
        for d in depth..depth + common {
            shared_bits.push(first.bit_at(d));
        }
        let child = build_patricia_subtree(keys, depth + common);
        PatriciaNode::Extension {
            depth,
            shared_bits,
            child: Box::new(child),
        }
    } else {
        // Branch at depth
        let split = keys.partition_point(|k| k.bit_at(depth) == 0);
        let left = build_patricia_subtree(&keys[..split], depth + 1);
        let right = build_patricia_subtree(&keys[split..], depth + 1);
        PatriciaNode::Branch {
            depth,
            left: Box::new(left),
            right: Box::new(right),
        }
    }
}

// =========================================================================
// ASSAY & BENCHMARK SUITE
// =========================================================================

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitAssayReport {
    pub key_count: usize,
    pub patricia_root: [u8; 32],
    pub smt_root: [u8; 32],
    pub patricia_rebuild_micros: u128,
    pub smt_rebuild_micros: u128,
    pub patricia_point_proof_bytes: usize,
    pub smt_point_proof_bytes: usize,
    pub patricia_non_member_proof_bytes: usize,
    pub smt_non_member_proof_bytes: usize,
    pub patricia_node_count: usize,
    pub patricia_extension_count: usize,
    pub patricia_branch_count: usize,
    pub smt_non_default_branch_count: usize,
    pub patricia_single_insert_hashes: usize,
    pub smt_single_insert_hashes: usize,
    pub patricia_clustered_multiproof_bytes: usize,
    pub smt_clustered_multiproof_bytes: usize,
}

impl CommitAssayReport {
    /// Encodes the deterministic assay fields (excluding wall-clock `micros`) into canonical bytes.
    pub fn to_canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(64 + 12 * 4);
        out.extend_from_slice(&self.patricia_root);
        out.extend_from_slice(&self.smt_root);
        for val in [
            self.key_count,
            self.patricia_point_proof_bytes,
            self.smt_point_proof_bytes,
            self.patricia_non_member_proof_bytes,
            self.smt_non_member_proof_bytes,
            self.patricia_node_count,
            self.patricia_extension_count,
            self.patricia_branch_count,
            self.smt_non_default_branch_count,
            self.patricia_single_insert_hashes,
            self.smt_single_insert_hashes,
            self.patricia_clustered_multiproof_bytes,
            self.smt_clustered_multiproof_bytes,
        ] {
            out.extend_from_slice(&(val as u32).to_be_bytes());
        }
        out
    }

    pub fn assay_tag(&self) -> ContentTag {
        let bytes = self.to_canonical_bytes();
        hash_canonical(
            TagKind::Receipt,
            COMMIT_R1_ASSAY_REPORT_SCHEMA,
            bytes.len() as u64,
            |sink| {
                sink(&bytes);
            },
        )
        .expect("CommitAssayReport canonical tag succeeds")
    }
}

/// Runs a side-by-side assay comparing Candidate A (Binary Patricia) and Candidate B (192-level SMT)
/// for a given world. Safe on both native and `wasm32-unknown-unknown` targets.
pub fn run_commit_assay(world: &World) -> CommitAssayReport {
    #[cfg(not(target_arch = "wasm32"))]
    let t0 = std::time::Instant::now();
    let patricia = MerklePatriciaTrie::from_world(world);
    #[cfg(not(target_arch = "wasm32"))]
    let patricia_rebuild_micros = t0.elapsed().as_micros();
    #[cfg(target_arch = "wasm32")]
    let patricia_rebuild_micros = 0u128;

    #[cfg(not(target_arch = "wasm32"))]
    let t1 = std::time::Instant::now();
    let smt = SparseMerkleTree::from_world(world);
    #[cfg(not(target_arch = "wasm32"))]
    let smt_rebuild_micros = t1.elapsed().as_micros();
    #[cfg(target_arch = "wasm32")]
    let smt_rebuild_micros = 0u128;

    // Pick first occupied coordinate for membership proof
    let first_coord = world.iter_on().next().unwrap_or(Coord::new(0, 0, 0));
    let member_key = CoordinateKeyV1::from_coord(first_coord);

    let patricia_proof = patricia.prove(&member_key);
    let smt_proof = smt.prove(&member_key);

    assert_eq!(
        MerklePatriciaTrie::verify_proof(patricia.root(), &member_key, &patricia_proof),
        Some(true)
    );
    assert!(SparseMerkleTree::verify_proof(
        smt.root(),
        &member_key,
        &smt_proof
    ));

    // Absent coordinate for non-membership proof
    let non_member_coord = Coord::new(999_999, -888_888, 777_777);
    let non_member_key = CoordinateKeyV1::from_coord(non_member_coord);

    let patricia_absent = patricia.prove(&non_member_key);
    let smt_absent = smt.prove(&non_member_key);

    assert_eq!(
        MerklePatriciaTrie::verify_proof(patricia.root(), &non_member_key, &patricia_absent),
        Some(false)
    );
    assert!(SparseMerkleTree::verify_proof(
        smt.root(),
        &non_member_key,
        &smt_absent
    ));

    let (patricia_node_count, patricia_extension_count, patricia_branch_count) =
        patricia.node_counts();
    let smt_non_default_branch_count = smt.non_default_branch_count();

    // Measure incremental single-insert hash invocations on a clone
    let mut patricia_probe = patricia.clone();
    let (_, patricia_single_insert_hashes) = patricia_probe.insert_counted(non_member_key);
    let smt_single_insert_hashes = KEY_BITS + 1; // 1 leaf + 192 branch levels

    // Clustered multiproof over up to 8 occupied coordinates
    let clustered_keys: Vec<CoordinateKeyV1> = world
        .iter_on()
        .take(8)
        .map(CoordinateKeyV1::from_coord)
        .collect();
    let patricia_clustered_multiproof_bytes = patricia.multiproof_wire_bytes(&clustered_keys);
    let smt_clustered_multiproof_bytes = smt.multiproof_wire_bytes(&clustered_keys);

    CommitAssayReport {
        key_count: world.iter_on().count(),
        patricia_root: patricia.root(),
        smt_root: smt.root(),
        patricia_rebuild_micros,
        smt_rebuild_micros,
        patricia_point_proof_bytes: patricia_proof.encoded_len(),
        smt_point_proof_bytes: smt_proof.encoded_len(),
        patricia_non_member_proof_bytes: patricia_absent.encoded_len(),
        smt_non_member_proof_bytes: smt_absent.encoded_len(),
        patricia_node_count,
        patricia_extension_count,
        patricia_branch_count,
        smt_non_default_branch_count,
        patricia_single_insert_hashes,
        smt_single_insert_hashes,
        patricia_clustered_multiproof_bytes,
        smt_clustered_multiproof_bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinate_key_order_and_extrema_match_specification() {
        assert_eq!(
            CoordinateKeyV1::from_coord(Coord::new(i64::MIN, 0, 0)).0[0..8],
            [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
        assert_eq!(
            CoordinateKeyV1::from_coord(Coord::new(-1, 0, 0)).0[0..8],
            [0x7f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]
        );
        assert_eq!(
            CoordinateKeyV1::from_coord(Coord::new(0, 0, 0)).0[0..8],
            [0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
        assert_eq!(
            CoordinateKeyV1::from_coord(Coord::new(1, 0, 0)).0[0..8],
            [0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01]
        );
        assert_eq!(
            CoordinateKeyV1::from_coord(Coord::new(i64::MAX, 0, 0)).0[0..8],
            [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]
        );

        let coords = [
            Coord::new(i64::MIN, -100, 0),
            Coord::new(-5, 2, -1),
            Coord::new(-1, -1, -1),
            Coord::new(0, 0, 0),
            Coord::new(0, 0, 1),
            Coord::new(0, 1, 0),
            Coord::new(1, 0, 0),
            Coord::new(i64::MAX, 100, 0),
        ];
        for i in 0..coords.len() - 1 {
            let k1 = CoordinateKeyV1::from_coord(coords[i]);
            let k2 = CoordinateKeyV1::from_coord(coords[i + 1]);
            assert!(k1 < k2, "coordinate key order must match Coord Ord");
            assert_eq!(k1.to_coord(), coords[i]);
        }
    }

    #[test]
    fn smt_and_patricia_empty_and_bedrock_roots_are_deterministic() {
        let empty_patricia = MerklePatriciaTrie::from_sorted_keys(Vec::new());
        let empty_smt = SparseMerkleTree::from_sorted_keys(Vec::new());

        assert_eq!(empty_patricia.root(), node_hash(MPT_EMPTY_TYPE, 0, &[]));
        assert_eq!(empty_smt.root(), smt_empty_root());

        let world = World::new(); // exactly 1 block at (0, 0, 0)
        let patricia = MerklePatriciaTrie::from_world(&world);
        let smt = SparseMerkleTree::from_world(&world);

        const EXPECTED_EMPTY_SMT: &str =
            "1e43d15c5ca2ab68222b35b035b98cfa9f4b92bd952553e24c9922102445685e";
        const EXPECTED_EMPTY_PATRICIA: &str =
            "ce7ffb25b9babd0735a4803e66480adfab6e6876c20c684969776dd39fa472dd";
        const EXPECTED_BEDROCK_SMT_TAG: &str =
            "#bb.world.sha256.s62smydfcpkdz2fmdnmmiwn3k5o2uurmtqhhymvtxl5wk73cim3a";
        const EXPECTED_BEDROCK_PATRICIA_TAG: &str =
            "#bb.world.sha256.szkkbgkgckrmeembx2lifcleyel2zydi2i4e6lsdi6c3kr25uwcq";

        let mut hex_empty_smt = String::new();
        for b in empty_smt.root() {
            hex_empty_smt.push_str(&format!("{b:02x}"));
        }
        assert_eq!(hex_empty_smt, EXPECTED_EMPTY_SMT);

        let mut hex_empty_patricia = String::new();
        for b in empty_patricia.root() {
            hex_empty_patricia.push_str(&format!("{b:02x}"));
        }
        assert_eq!(hex_empty_patricia, EXPECTED_EMPTY_PATRICIA);

        assert_eq!(smt.root_tag().to_string(), EXPECTED_BEDROCK_SMT_TAG);
        assert_eq!(
            patricia.root_tag().to_string(),
            EXPECTED_BEDROCK_PATRICIA_TAG
        );

        assert_ne!(patricia.root(), empty_patricia.root());
        assert_ne!(smt.root(), empty_smt.root());
        assert_ne!(patricia.root(), smt.root());
    }

    #[test]
    fn rebuild_parity_under_incremental_path_copy_insertions_and_deletions() {
        let mut world = World::new();
        let mut patricia = MerklePatriciaTrie::from_world(&world);
        let mut smt = SparseMerkleTree::from_world(&world);

        let test_coords = [
            Coord::new(10, 20, 30),
            Coord::new(-5, -15, 25),
            Coord::new(100, 0, -50),
            Coord::new(0, 1, 0),
            Coord::new(0, 0, 1),  // shares 191 prefix bits with bedrock (0,0,0)!
            Coord::new(-1, 0, 0), // diverges at bit 0 from bedrock!
        ];

        for &c in &test_coords {
            world.set_on(c);
            let k = CoordinateKeyV1::from_coord(c);
            assert!(patricia.insert(k));
            assert!(smt.insert(k));

            // Cold full rebuild must equal incremental path-copy tree structure and root!
            let p_fresh = MerklePatriciaTrie::from_world(&world);
            let s_fresh = SparseMerkleTree::from_world(&world);

            assert_eq!(
                patricia.root_node(),
                p_fresh.root_node(),
                "patricia node structure parity"
            );
            assert_eq!(
                patricia.root(),
                p_fresh.root(),
                "patricia insert root parity"
            );
            assert_eq!(smt.root(), s_fresh.root(), "smt insert root parity");
        }

        // Delete coordinates in reverse/interleaved order and verify branch collapse & extension merging
        for &c in test_coords.iter().rev() {
            world.set_off(c);
            let k = CoordinateKeyV1::from_coord(c);
            assert!(patricia.delete(&k));
            assert!(smt.delete(&k));

            let p_fresh = MerklePatriciaTrie::from_world(&world);
            let s_fresh = SparseMerkleTree::from_world(&world);

            assert_eq!(
                patricia.root_node(),
                p_fresh.root_node(),
                "patricia delete structure parity"
            );
            assert_eq!(
                patricia.root(),
                p_fresh.root(),
                "patricia delete root parity"
            );
            assert_eq!(smt.root(), s_fresh.root(), "smt delete root parity");
        }

        // Back to bedrock only
        let initial_world = World::new();
        assert_eq!(
            patricia.root(),
            MerklePatriciaTrie::from_world(&initial_world).root()
        );
        assert_eq!(
            smt.root(),
            SparseMerkleTree::from_world(&initial_world).root()
        );
    }

    #[test]
    fn point_proof_verification_wire_codecs_and_soundness_controls() {
        let mut world = World::new();
        world.set_on(Coord::new(1, 2, 3));
        world.set_on(Coord::new(4, 5, 6));

        let patricia = MerklePatriciaTrie::from_world(&world);
        let smt = SparseMerkleTree::from_world(&world);

        let present_key = CoordinateKeyV1::from_coord(Coord::new(1, 2, 3));
        let other_present_key = CoordinateKeyV1::from_coord(Coord::new(4, 5, 6));
        let absent_key = CoordinateKeyV1::from_coord(Coord::new(7, 8, 9));

        // 1. Patricia present + canonical wire roundtrip + literal tag pin
        let p_proof = patricia.prove(&present_key);
        let p_bytes = p_proof.to_canonical_bytes();
        assert_eq!(p_bytes.len(), p_proof.encoded_len());
        assert_eq!(
            p_proof.proof_tag().to_string(),
            "#bb.receipt.sha256.rjm2d4ykhamhdxkbc3gv2pmeqdgjeiop7gfvq3cf4py4rejonnqq"
        );
        let p_decoded = PatriciaPointProof::from_canonical_bytes(&p_bytes)
            .expect("canonical patricia proof decodes");
        assert_eq!(p_decoded, p_proof);
        assert_eq!(
            MerklePatriciaTrie::verify_proof(patricia.root(), &present_key, &p_decoded),
            Some(true)
        );

        // 2. SOUNDNESS CONTROL A: Cross-key proof spoofing must reject (`None`), NEVER return `Some(false)`!
        // Previously, passing `p_proof` (a valid proof for `present_key`) when querying `other_present_key`
        // walked `present_key`'s branch, saw `lk != other_present_key`, and falsely returned `Some(false)`!
        assert_eq!(
            MerklePatriciaTrie::verify_proof(patricia.root(), &other_present_key, &p_proof),
            None,
            "cross-key proof substitution must be rejected as invalid (None), not accepted as an absence proof"
        );

        // 3. SOUNDNESS CONTROL B: Spoofed DivergentExtension on a matching prefix must reject (`None`)!
        if let Some(PatriciaProofStep::Extension { depth, shared_bits }) = p_proof.steps.first() {
            let spoofed_div = PatriciaPointProof {
                steps: Vec::new(),
                terminal: PatriciaTerminal::DivergentExtension {
                    depth: *depth,
                    shared_bits: shared_bits.clone(),
                    child_hash: [0x11; 32],
                },
            };
            assert_eq!(
                MerklePatriciaTrie::verify_proof(patricia.root(), &present_key, &spoofed_div),
                None,
                "DivergentExtension whose bits actually match query key must reject"
            );
        }

        // 4. Patricia absent + canonical wire roundtrip
        let p_absent_proof = patricia.prove(&absent_key);
        let p_abs_bytes = p_absent_proof.to_canonical_bytes();
        assert_eq!(
            PatriciaPointProof::from_canonical_bytes(&p_abs_bytes).as_ref(),
            Some(&p_absent_proof)
        );
        assert_eq!(
            MerklePatriciaTrie::verify_proof(patricia.root(), &absent_key, &p_absent_proof),
            Some(false)
        );

        // 5. SMT present + canonical wire roundtrip + literal tag pin
        let s_proof = smt.prove(&present_key);
        assert_eq!(
            s_proof.active_decision_depths().len(),
            s_proof.non_default_siblings.len()
        );
        let s_bytes = s_proof.to_canonical_bytes();
        assert_eq!(s_bytes.len(), s_proof.encoded_len());
        assert_eq!(
            s_proof.proof_tag().to_string(),
            "#bb.receipt.sha256.ke5w46p75ibf4owqccyjxqaryc6dpq3u5lglb7a56upvck4tinia"
        );
        let s_decoded =
            SmtPointProof::from_canonical_bytes(&s_bytes).expect("canonical smt proof decodes");
        assert_eq!(s_decoded, s_proof);
        assert!(SparseMerkleTree::verify_proof(
            smt.root(),
            &present_key,
            &s_decoded
        ));

        // 6. SOUNDNESS CONTROL C: Explicit default sibling in SMT when bitmap bit is 1 must reject!
        let mut malleable_smt = s_proof.clone();
        // Find first 0 bit in bitmap and set it to 1 with explicit default hash D[d+1]
        for d in 0..KEY_BITS {
            let bit = (malleable_smt.bitmap[d / 8] >> (7 - (d % 8))) & 1;
            if bit == 0 {
                malleable_smt.bitmap[d / 8] |= 1 << (7 - (d % 8));
                // Count how many 1 bits precede d to insert at the right position
                let mut insert_idx = 0;
                for prev_d in 0..d {
                    if ((s_proof.bitmap[prev_d / 8] >> (7 - (prev_d % 8))) & 1) == 1 {
                        insert_idx += 1;
                    }
                }
                malleable_smt
                    .non_default_siblings
                    .insert(insert_idx, smt_default_hashes()[d + 1]);
                break;
            }
        }
        assert!(
            !SparseMerkleTree::verify_proof(smt.root(), &present_key, &malleable_smt),
            "SMT proof carrying an explicit default sibling hash must reject as non-canonical"
        );

        // 7. SMT absent
        let s_absent_proof = smt.prove(&absent_key);
        assert!(SparseMerkleTree::verify_proof(
            smt.root(),
            &absent_key,
            &s_absent_proof
        ));

        // 8. SMT falsified presence claim
        let mut fake_smt_proof = s_absent_proof.clone();
        fake_smt_proof.is_present = true;
        assert!(
            !SparseMerkleTree::verify_proof(smt.root(), &absent_key, &fake_smt_proof),
            "flipped is_present must fail SMT verification"
        );
    }

    #[test]
    fn side_by_side_assay_executes_and_pins_canonical_tags() {
        let mut world = World::new();
        for i in 0..10 {
            world.set_on(Coord::new(i, i * 2, i * 3));
        }

        let report = run_commit_assay(&world);
        assert_eq!(report.key_count, 10);
        assert_ne!(report.patricia_root, [0u8; 32]);
        assert_ne!(report.smt_root, [0u8; 32]);
        assert!(report.patricia_point_proof_bytes > 0);
        assert!(report.smt_point_proof_bytes > 0);
        assert!(report.patricia_node_count > 0);
        assert!(report.smt_non_default_branch_count > 0);
        assert!(report.patricia_single_insert_hashes < report.smt_single_insert_hashes);
        assert!(report.patricia_clustered_multiproof_bytes > 0);
        assert!(report.smt_clustered_multiproof_bytes > 0);
        assert!(
            report
                .assay_tag()
                .to_string()
                .starts_with("#bb.receipt.sha256.")
        );
    }

    #[test]
    fn test_coordinate_key_bit_at_out_of_bounds_safety() {
        let key = CoordinateKeyV1::from_coord(Coord::new(123, -456, 789));
        assert_eq!(key.bit_at(KEY_BITS), 0);
        assert_eq!(key.bit_at(KEY_BITS + 10), 0);
        assert_eq!(key.bit_at(usize::MAX), 0);
    }
}
