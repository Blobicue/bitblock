//! COMMIT-R1: side-by-side authenticated-map research for Bitblock.
//!
//! Compares Candidate A (fixed-key binary Merkle Patricia Trie) and Candidate B
//! (192-level Sparse Merkle Tree) over [`CoordinateKeyV1`] as specified in
//! `docs/AUTHENTICATED_STATE_PLAN.md`.
//!
//! This is research and assay code for the capability node `COMMIT-R1`. It does not
//! replace `world_tag`, authenticate a live block, or mutate the runtime `World`.

use std::fmt;
use std::sync::OnceLock;

use bitblock_core::protocol::{ContentTag, TagKind, hash_canonical};
use bitblock_core::world::{Coord, World};
use sha2::{Digest, Sha256};

pub const COMMIT_R1_WORLD_PATRICIA_SCHEMA: &[u8] = b"bitblock/commit-r1/world-patricia/v1";
pub const COMMIT_R1_WORLD_SMT_SCHEMA: &[u8] = b"bitblock/commit-r1/world-smt/v1";

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
    pub const MAX: Self = Self([0xFF; KEY_BYTES]);

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
        debug_assert!(depth < KEY_BITS);
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

/// 192-level Sparse Merkle Tree over 192-bit CoordinateKeyV1.
#[derive(Clone)]
pub struct SparseMerkleTree {
    root: [u8; 32],
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
}

impl SparseMerkleTree {
    /// Constructs an SMT from an arbitrary set of world coordinates.
    pub fn from_world(world: &World) -> Self {
        let mut keys: Vec<CoordinateKeyV1> =
            world.iter_on().map(CoordinateKeyV1::from_coord).collect();
        keys.sort_unstable();
        keys.dedup();
        Self::from_sorted_keys(keys)
    }

    /// Constructs an SMT from pre-sorted unique CoordinateKeyV1 values.
    pub fn from_sorted_keys(keys: Vec<CoordinateKeyV1>) -> Self {
        if keys.is_empty() {
            return Self {
                root: smt_empty_root(),
                occupied: Vec::new(),
            };
        }
        let root = build_smt_subtree(&keys, 0);
        Self {
            root,
            occupied: keys,
        }
    }

    pub fn root(&self) -> [u8; 32] {
        self.root
    }

    pub fn root_tag(&self) -> ContentTag {
        hash_canonical(TagKind::World, COMMIT_R1_WORLD_SMT_SCHEMA, 32, |sink| {
            sink(&self.root);
        })
        .expect("SMT root tag encoding is canonical")
    }

    pub fn is_occupied(&self, key: &CoordinateKeyV1) -> bool {
        self.occupied.binary_search(key).is_ok()
    }

    pub fn insert(&mut self, key: CoordinateKeyV1) -> bool {
        match self.occupied.binary_search(&key) {
            Ok(_) => false,
            Err(pos) => {
                self.occupied.insert(pos, key);
                self.root = build_smt_subtree(&self.occupied, 0);
                true
            }
        }
    }

    pub fn delete(&mut self, key: &CoordinateKeyV1) -> bool {
        match self.occupied.binary_search(key) {
            Ok(pos) => {
                self.occupied.remove(pos);
                if self.occupied.is_empty() {
                    self.root = smt_empty_root();
                } else {
                    self.root = build_smt_subtree(&self.occupied, 0);
                }
                true
            }
            Err(_) => false,
        }
    }

    /// Generates a compact point witness (membership or non-membership) for a query key.
    pub fn prove(&self, key: &CoordinateKeyV1) -> SmtPointProof {
        let is_present = self.is_occupied(key);
        let mut bitmap = [0u8; KEY_BYTES];
        let mut non_default_siblings = Vec::new();

        let mut current_slice = &self.occupied[..];
        for d in 0..KEY_BITS {
            let key_bit = key.bit_at(d);
            // Split current_slice into left (bit 0) and right (bit 1)
            let split = current_slice.partition_point(|k| k.bit_at(d) == 0);
            let (left, right) = current_slice.split_at(split);

            let (path_slice, sibling_slice) = if key_bit == 0 {
                (left, right)
            } else {
                (right, left)
            };

            current_slice = path_slice;

            if !sibling_slice.is_empty() {
                // Sibling is non-default!
                bitmap[d / 8] |= 1 << (7 - (d % 8));
                let sibling_hash = build_smt_subtree(sibling_slice, d + 1);
                non_default_siblings.push(sibling_hash);
            }
        }

        SmtPointProof {
            bitmap,
            non_default_siblings,
            is_present,
        }
    }

    /// Verifies a point proof against an expected root.
    pub fn verify_proof(
        expected_root: [u8; 32],
        key: &CoordinateKeyV1,
        proof: &SmtPointProof,
    ) -> bool {
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
                proof.non_default_siblings[sibling_read_idx]
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
}

fn build_smt_subtree(keys: &[CoordinateKeyV1], depth: usize) -> [u8; 32] {
    if keys.is_empty() {
        return smt_default_hashes()[depth];
    }
    if depth == KEY_BITS {
        return smt_present_leaf_hash();
    }

    let split = keys.partition_point(|k| k.bit_at(depth) == 0);
    let (left, right) = keys.split_at(split);

    let left_hash = if left.is_empty() {
        smt_default_hashes()[depth + 1]
    } else {
        build_smt_subtree(left, depth + 1)
    };

    let right_hash = if right.is_empty() {
        smt_default_hashes()[depth + 1]
    } else {
        build_smt_subtree(right, depth + 1)
    };

    let mut payload = [0u8; 64];
    payload[0..32].copy_from_slice(&left_hash);
    payload[32..64].copy_from_slice(&right_hash);
    node_hash(SMT_BRANCH_TYPE, depth as u8, &payload)
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
                // Pack suffix bits
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
                let mut current_byte = 0u8;
                let mut bit_in_byte = 0;
                for &b in shared_bits {
                    current_byte |= b << (7 - bit_in_byte);
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

impl PatriciaPointProof {
    pub fn encoded_len(&self) -> usize {
        let mut len = 1; // terminal tag
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
                PatriciaProofStep::Branch { .. } => len += 1 + 1 + 32,
                PatriciaProofStep::Extension { shared_bits, .. } => {
                    len += 1 + 2 + shared_bits.len().div_ceil(8)
                }
            }
        }
        len
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

    pub fn insert(&mut self, key: CoordinateKeyV1) -> bool {
        match self.occupied.binary_search(&key) {
            Ok(_) => false,
            Err(pos) => {
                self.occupied.insert(pos, key);
                self.root_node = build_patricia_subtree(&self.occupied, 0);
                true
            }
        }
    }

    pub fn delete(&mut self, key: &CoordinateKeyV1) -> bool {
        match self.occupied.binary_search(key) {
            Ok(pos) => {
                self.occupied.remove(pos);
                if self.occupied.is_empty() {
                    self.root_node = PatriciaNode::Empty;
                } else {
                    self.root_node = build_patricia_subtree(&self.occupied, 0);
                }
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
                    // Check if key matches shared_bits
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

    /// Verifies a Patricia point proof against an expected root.
    /// Returns Some(true) if key is proved present, Some(false) if proved absent,
    /// or None if the proof is malformed / invalid.
    pub fn verify_proof(
        expected_root: [u8; 32],
        key: &CoordinateKeyV1,
        proof: &PatriciaPointProof,
    ) -> Option<bool> {
        let (mut current_hash, is_present, mut current_depth) = match &proof.terminal {
            PatriciaTerminal::EmptyRoot => {
                if !proof.steps.is_empty() {
                    return None;
                }
                (node_hash(MPT_EMPTY_TYPE, 0, &[]), false, 0)
            }
            PatriciaTerminal::Leaf { depth, key: lk } => {
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
                (h, present, *depth)
            }
            PatriciaTerminal::DivergentExtension {
                depth,
                shared_bits,
                child_hash,
            } => {
                let mut payload = Vec::with_capacity(2 + shared_bits.len().div_ceil(8) + 32);
                payload.extend_from_slice(&(shared_bits.len() as u16).to_be_bytes());
                let mut current_byte = 0u8;
                let mut bit_in_byte = 0;
                for &b in shared_bits {
                    current_byte |= b << (7 - bit_in_byte);
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
                payload.extend_from_slice(child_hash);
                let h = node_hash(MPT_EXTENSION_TYPE, *depth as u8, &payload);
                (h, false, *depth)
            }
        };

        // Reconstruct steps from leaf back to root
        for step in proof.steps.iter().rev() {
            match step {
                PatriciaProofStep::Extension { depth, shared_bits } => {
                    if *depth + shared_bits.len() != current_depth {
                        return None;
                    }
                    let mut payload = Vec::with_capacity(2 + shared_bits.len().div_ceil(8) + 32);
                    payload.extend_from_slice(&(shared_bits.len() as u16).to_be_bytes());
                    let mut current_byte = 0u8;
                    let mut bit_in_byte = 0;
                    for &b in shared_bits {
                        current_byte |= b << (7 - bit_in_byte);
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
                    payload.extend_from_slice(&current_hash);
                    current_hash = node_hash(MPT_EXTENSION_TYPE, *depth as u8, &payload);
                    current_depth = *depth;
                }
                PatriciaProofStep::Branch {
                    depth,
                    direction,
                    sibling_hash,
                } => {
                    if *depth + 1 != current_depth {
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
                }
            }
        }

        if current_depth != 0 || current_hash != expected_root {
            return None;
        }

        Some(is_present)
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

#[derive(Clone, Debug)]
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
}

/// Runs a side-by-side assay comparing Candidate A and Candidate B for a given world.
pub fn run_commit_assay(world: &World) -> CommitAssayReport {
    let t0 = std::time::Instant::now();
    let patricia = MerklePatriciaTrie::from_world(world);
    let patricia_rebuild_micros = t0.elapsed().as_micros();

    let t1 = std::time::Instant::now();
    let smt = SparseMerkleTree::from_world(world);
    let smt_rebuild_micros = t1.elapsed().as_micros();

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
    fn rebuild_parity_under_insertions_and_deletions() {
        let mut world = World::new();
        let mut patricia = MerklePatriciaTrie::from_world(&world);
        let mut smt = SparseMerkleTree::from_world(&world);

        let test_coords = [
            Coord::new(10, 20, 30),
            Coord::new(-5, -15, 25),
            Coord::new(100, 0, -50),
            Coord::new(0, 1, 0),
            Coord::new(-1, 0, 0),
        ];

        for &c in &test_coords {
            world.set_on(c);
            let k = CoordinateKeyV1::from_coord(c);
            assert!(patricia.insert(k));
            assert!(smt.insert(k));

            // Fresh rebuild must equal incremental root
            let p_fresh = MerklePatriciaTrie::from_world(&world);
            let s_fresh = SparseMerkleTree::from_world(&world);

            assert_eq!(patricia.root(), p_fresh.root(), "patricia insert parity");
            assert_eq!(smt.root(), s_fresh.root(), "smt insert parity");
        }

        // Delete coordinates one by one
        for &c in &test_coords {
            world.set_off(c);
            let k = CoordinateKeyV1::from_coord(c);
            assert!(patricia.delete(&k));
            assert!(smt.delete(&k));

            let p_fresh = MerklePatriciaTrie::from_world(&world);
            let s_fresh = SparseMerkleTree::from_world(&world);

            assert_eq!(patricia.root(), p_fresh.root(), "patricia delete parity");
            assert_eq!(smt.root(), s_fresh.root(), "smt delete parity");
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
    fn point_proof_verification_and_negative_mutations() {
        let mut world = World::new();
        world.set_on(Coord::new(1, 2, 3));
        world.set_on(Coord::new(4, 5, 6));

        let patricia = MerklePatriciaTrie::from_world(&world);
        let smt = SparseMerkleTree::from_world(&world);

        let present_key = CoordinateKeyV1::from_coord(Coord::new(1, 2, 3));
        let absent_key = CoordinateKeyV1::from_coord(Coord::new(7, 8, 9));

        // Patricia present
        let p_proof = patricia.prove(&present_key);
        assert_eq!(
            MerklePatriciaTrie::verify_proof(patricia.root(), &present_key, &p_proof),
            Some(true)
        );
        // Patricia claiming absent for present key must fail or return Some(true)
        assert_ne!(
            MerklePatriciaTrie::verify_proof(patricia.root(), &present_key, &p_proof),
            Some(false)
        );

        // Patricia absent
        let p_absent_proof = patricia.prove(&absent_key);
        assert_eq!(
            MerklePatriciaTrie::verify_proof(patricia.root(), &absent_key, &p_absent_proof),
            Some(false)
        );

        // SMT present
        let s_proof = smt.prove(&present_key);
        assert!(SparseMerkleTree::verify_proof(
            smt.root(),
            &present_key,
            &s_proof
        ));

        // SMT absent
        let s_absent_proof = smt.prove(&absent_key);
        assert!(SparseMerkleTree::verify_proof(
            smt.root(),
            &absent_key,
            &s_absent_proof
        ));

        // SMT falsified presence claim: flip is_present bit
        let mut fake_smt_proof = s_absent_proof.clone();
        fake_smt_proof.is_present = true;
        assert!(
            !SparseMerkleTree::verify_proof(smt.root(), &absent_key, &fake_smt_proof),
            "flipped is_present must fail SMT verification"
        );

        // SMT corrupted sibling hash
        if !fake_smt_proof.non_default_siblings.is_empty() {
            let mut bad_sibling = s_proof.clone();
            bad_sibling.non_default_siblings[0][0] ^= 1;
            assert!(
                !SparseMerkleTree::verify_proof(smt.root(), &present_key, &bad_sibling),
                "corrupted sibling hash must fail SMT verification"
            );
        }
    }

    #[test]
    fn side_by_side_assay_executes_and_measures() {
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
    }
}
