//! Arity-8 path-compressed sparse cryptographic octree (`SparseOctreeV1`) and
//! `VERKLE-R1` feasibility report carrier over 180-bit [`MortonKeyV1`] chunk prefixes.
//!
//! Each allocated $16 \times 16 \times 16$ chunk is committed as a [`ChunkLeafV1`]
//! carrying its 180-bit 3D Morton chunk prefix, occupied voxel count, and 512-byte
//! big-endian bitset. Internal branching across the 60-level octant prefix trie uses
//! path-compressed arity-8 [`OctreeInternalNodeV1`] records, anchored by a 49-byte
//! [`OctreeRootV1`] header and verified via [`OctreePointProofV1`] witnesses.

use crate::commit_r1::MortonKeyV1;
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::world::{CHUNK_WORDS, Coord, World};

/// Canonical schema string for 180-bit Morton-prefixed `ChunkLeafV1` payloads.
pub const OCTREE_CHUNK_LEAF_V1_SCHEMA: &[u8] = b"bitblock/octree-commit/chunk-leaf/v1";
/// Canonical schema string for path-compressed arity-8 `OctreeInternalNodeV1` payloads.
pub const OCTREE_INTERNAL_NODE_V1_SCHEMA: &[u8] = b"bitblock/octree-commit/internal-node/v1";
/// Canonical schema string for top-level `OctreeRootV1` state commitments.
pub const OCTREE_ROOT_V1_SCHEMA: &[u8] = b"bitblock/octree-commit/root/v1";
/// Canonical schema string for `OctreePointProofV1` coordinate inclusion witnesses.
pub const OCTREE_POINT_PROOF_V1_SCHEMA: &[u8] = b"bitblock/octree-commit/point-proof/v1";
/// Canonical schema string for `VerkleR1FeasibilityReportV1` cost comparison payloads.
pub const VERKLE_R1_FEASIBILITY_V1_SCHEMA: &[u8] =
    b"bitblock/octree-commit/verkle-r1-feasibility/v1";

const CHUNK_PREFIX_BYTES: usize = 23;
const CHUNK_PREFIX_LEVELS: u8 = 60;
const CHUNK_BITS_BYTES: usize = 512;
const CHUNK_LEAF_PAYLOAD_LEN: usize = 537;
const INTERNAL_NODE_HEADER_LEN: usize = 25;
const ROOT_PAYLOAD_LEN: usize = 49;
const VERKLE_FEASIBILITY_PAYLOAD_LEN: usize = 198;

const TERMINAL_TAG_EMPTY_TREE: u8 = 0;
const TERMINAL_TAG_CHUNK_LEAF: u8 = 1;
const TERMINAL_TAG_INTERNAL_NODE: u8 = 2;

/// Reads the 3-bit octant index (`0..8`) at `depth in 0..60` from a 180-bit chunk prefix.
/// Returns `0` when `depth >= 60`.
pub fn octant_at_prefix(prefix: &[u8; 23], depth: u8) -> u8 {
    if depth >= CHUNK_PREFIX_LEVELS {
        return 0;
    }
    let base = 3 * (depth as usize);
    let bit = |p: usize| (prefix[p / 8] >> (7 - (p % 8))) & 1;
    (bit(base) << 2) | (bit(base + 1) << 1) | bit(base + 2)
}

/// Masks a 180-bit chunk prefix so only the top `3 * branch_depth.min(60)` bits are retained
/// and all trailing bits are zeroed.
pub fn mask_prefix_180b(raw: [u8; 23], branch_depth: u8) -> [u8; 23] {
    let mut out = [0u8; CHUNK_PREFIX_BYTES];
    let keep_bits = (branch_depth.min(CHUNK_PREFIX_LEVELS) as usize) * 3;
    let full_bytes = keep_bits / 8;
    let rem = keep_bits % 8;
    out[0..full_bytes].copy_from_slice(&raw[0..full_bytes]);
    if rem != 0 {
        out[full_bytes] = raw[full_bytes] & (!0u8 << (8 - rem));
    }
    out
}

/// Returns the number of matching leading octant trios (`0..=60`) between two 180-bit prefixes.
pub fn common_octant_prefix_len(a: &[u8; 23], b: &[u8; 23]) -> usize {
    for d in 0..(CHUNK_PREFIX_LEVELS as usize) {
        if octant_at_prefix(a, d as u8) != octant_at_prefix(b, d as u8) {
            return d;
        }
    }
    CHUNK_PREFIX_LEVELS as usize
}

/// Canonical $16 \times 16 \times 16$ bit-packed chunk leaf (`537` payload bytes).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChunkLeafV1 {
    pub chunk_prefix_180b: [u8; 23],
    pub occupied_count_u16: u16,
    pub bits_512b: [u8; 512],
}

impl ChunkLeafV1 {
    /// Constructs a `ChunkLeafV1` from a chunk origin, occupied voxel count, and bitset words.
    pub fn from_chunk_words(
        chunk_origin: Coord,
        occupied_count_u16: u16,
        words: &[u64; CHUNK_WORDS],
    ) -> Self {
        let chunk_prefix_180b = MortonKeyV1::from_coord(chunk_origin).chunk_prefix_180b();
        let mut bits_512b = [0u8; CHUNK_BITS_BYTES];
        for (word_idx, &word) in words.iter().enumerate() {
            let offset = word_idx * 8;
            bits_512b[offset..offset + 8].copy_from_slice(&word.to_be_bytes());
        }
        Self {
            chunk_prefix_180b,
            occupied_count_u16,
            bits_512b,
        }
    }

    /// Returns whether the chunk prefix padding, occupied count, and popcount are canonical.
    pub fn is_canonical(&self) -> bool {
        if (self.chunk_prefix_180b[22] & 0x0F) != 0 {
            return false;
        }
        if self.occupied_count_u16 == 0 || self.occupied_count_u16 > 4096 {
            return false;
        }
        let popcount: u32 = self.bits_512b.iter().map(|b| b.count_ones()).sum();
        popcount == u32::from(self.occupied_count_u16)
    }

    /// Returns whether the local voxel bit for `coord` is set inside this 512-byte chunk leaf.
    pub fn is_coord_on(&self, coord: Coord) -> bool {
        let lx = coord.x.rem_euclid(16) as usize;
        let ly = coord.y.rem_euclid(16) as usize;
        let lz = coord.z.rem_euclid(16) as usize;
        let local_index = lx + 16 * (ly + 16 * lz);
        let word_index = local_index / 64;
        let bit_index = local_index % 64;
        let byte_index = word_index * 8 + (7 - bit_index / 8);
        let mask = 1u8 << (bit_index % 8);
        (self.bits_512b[byte_index] & mask) != 0
    }

    /// Encodes this `ChunkLeafV1` into its fixed 537-byte canonical payload array.
    pub fn encode(&self) -> [u8; CHUNK_LEAF_PAYLOAD_LEN] {
        let mut out = [0u8; CHUNK_LEAF_PAYLOAD_LEN];
        out[0..23].copy_from_slice(&self.chunk_prefix_180b);
        out[23..25].copy_from_slice(&self.occupied_count_u16.to_be_bytes());
        out[25..537].copy_from_slice(&self.bits_512b);
        out
    }

    /// Decodes and validates a canonical `ChunkLeafV1` from a 537-byte slice.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != CHUNK_LEAF_PAYLOAD_LEN {
            return None;
        }
        let mut chunk_prefix_180b = [0u8; CHUNK_PREFIX_BYTES];
        chunk_prefix_180b.copy_from_slice(&bytes[0..23]);
        let occupied_count_u16 = u16::from_be_bytes([bytes[23], bytes[24]]);
        let mut bits_512b = [0u8; CHUNK_BITS_BYTES];
        bits_512b.copy_from_slice(&bytes[25..537]);
        let leaf = Self {
            chunk_prefix_180b,
            occupied_count_u16,
            bits_512b,
        };
        if !leaf.is_canonical() {
            return None;
        }
        Some(leaf)
    }

    /// Computes the canonical `ContentTag` for this `ChunkLeafV1`.
    pub fn tag(&self) -> ContentTag {
        hash_canonical(
            TagKind::World,
            OCTREE_CHUNK_LEAF_V1_SCHEMA,
            CHUNK_LEAF_PAYLOAD_LEN as u64,
            |sink| {
                sink(&self.chunk_prefix_180b);
                sink(&self.occupied_count_u16.to_be_bytes());
                sink(&self.bits_512b);
            },
        )
        .expect("ChunkLeafV1 tag encoding is canonical")
    }
}

/// Path-compressed arity-8 internal octree branch node (`25 + 32 * k` payload bytes).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OctreeInternalNodeV1 {
    pub branch_depth: u8,
    pub prefix_bits_180b: [u8; 23],
    pub child_mask: u8,
    pub child_digests: Vec<[u8; 32]>,
}

impl OctreeInternalNodeV1 {
    /// Returns whether the branch depth, masked prefix, child mask, and child count are canonical.
    pub fn is_canonical(&self) -> bool {
        if self.branch_depth >= CHUNK_PREFIX_LEVELS {
            return false;
        }
        if self.prefix_bits_180b != mask_prefix_180b(self.prefix_bits_180b, self.branch_depth) {
            return false;
        }
        let ones = self.child_mask.count_ones() as usize;
        ones >= 2 && self.child_digests.len() == ones
    }

    /// Returns the canonical byte length of this `OctreeInternalNodeV1` payload.
    pub fn encoded_len(&self) -> usize {
        INTERNAL_NODE_HEADER_LEN + 32 * self.child_digests.len()
    }

    /// Encodes this `OctreeInternalNodeV1` into its canonical byte vector.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.encoded_len());
        out.push(self.branch_depth);
        out.extend_from_slice(&self.prefix_bits_180b);
        out.push(self.child_mask);
        for digest in &self.child_digests {
            out.extend_from_slice(digest);
        }
        out
    }

    /// Decodes and validates a canonical `OctreeInternalNodeV1` from a byte slice.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < INTERNAL_NODE_HEADER_LEN {
            return None;
        }
        let branch_depth = bytes[0];
        let mut prefix_bits_180b = [0u8; CHUNK_PREFIX_BYTES];
        prefix_bits_180b.copy_from_slice(&bytes[1..24]);
        let child_mask = bytes[24];
        let ones = child_mask.count_ones() as usize;
        if ones < 2 || bytes.len() != INTERNAL_NODE_HEADER_LEN + 32 * ones {
            return None;
        }
        let mut child_digests = Vec::with_capacity(ones);
        for idx in 0..ones {
            let start = INTERNAL_NODE_HEADER_LEN + idx * 32;
            let mut digest = [0u8; 32];
            digest.copy_from_slice(&bytes[start..start + 32]);
            child_digests.push(digest);
        }
        let node = Self {
            branch_depth,
            prefix_bits_180b,
            child_mask,
            child_digests,
        };
        if !node.is_canonical() {
            return None;
        }
        Some(node)
    }

    /// Computes the canonical `ContentTag` for this `OctreeInternalNodeV1`.
    pub fn tag(&self) -> ContentTag {
        let payload_len =
            INTERNAL_NODE_HEADER_LEN as u64 + 32 * (self.child_mask.count_ones() as u64);
        hash_canonical(
            TagKind::World,
            OCTREE_INTERNAL_NODE_V1_SCHEMA,
            payload_len,
            |sink| {
                sink(&[self.branch_depth]);
                sink(&self.prefix_bits_180b);
                sink(&[self.child_mask]);
                for digest in &self.child_digests {
                    sink(digest);
                }
            },
        )
        .expect("OctreeInternalNodeV1 tag encoding is canonical")
    }
}

/// Top-level 49-byte octree state commitment root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OctreeRootV1 {
    pub tree_state_u8: u8,
    pub occupied_chunks_u64: u64,
    pub occupied_bits_u64: u64,
    pub root_node_digest: [u8; 32],
}

impl OctreeRootV1 {
    /// Returns the canonical `OctreeRootV1` header representing an empty sparse octree.
    pub const fn empty() -> Self {
        Self {
            tree_state_u8: 0,
            occupied_chunks_u64: 0,
            occupied_bits_u64: 0,
            root_node_digest: [0u8; 32],
        }
    }

    /// Returns whether the tree state, occupied counts, and root digest satisfy canonical bounds.
    pub fn is_canonical(&self) -> bool {
        match self.tree_state_u8 {
            0 => {
                self.occupied_chunks_u64 == 0
                    && self.occupied_bits_u64 == 0
                    && self.root_node_digest == [0u8; 32]
            }
            1 => {
                self.occupied_chunks_u64 == 1
                    && (1..=4096).contains(&self.occupied_bits_u64)
                    && self.root_node_digest != [0u8; 32]
            }
            2 => {
                self.occupied_chunks_u64 >= 2
                    && self.occupied_bits_u64 >= self.occupied_chunks_u64
                    && self.occupied_bits_u64 <= self.occupied_chunks_u64.saturating_mul(4096)
                    && self.root_node_digest != [0u8; 32]
            }
            _ => false,
        }
    }

    /// Encodes this `OctreeRootV1` into its fixed 49-byte canonical payload array.
    pub fn encode(&self) -> [u8; ROOT_PAYLOAD_LEN] {
        let mut out = [0u8; ROOT_PAYLOAD_LEN];
        out[0] = self.tree_state_u8;
        out[1..9].copy_from_slice(&self.occupied_chunks_u64.to_be_bytes());
        out[9..17].copy_from_slice(&self.occupied_bits_u64.to_be_bytes());
        out[17..49].copy_from_slice(&self.root_node_digest);
        out
    }

    /// Decodes and validates a canonical `OctreeRootV1` from a 49-byte slice.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != ROOT_PAYLOAD_LEN {
            return None;
        }
        let tree_state_u8 = bytes[0];
        let mut chunks_bytes = [0u8; 8];
        chunks_bytes.copy_from_slice(&bytes[1..9]);
        let mut bits_bytes = [0u8; 8];
        bits_bytes.copy_from_slice(&bytes[9..17]);
        let mut root_node_digest = [0u8; 32];
        root_node_digest.copy_from_slice(&bytes[17..49]);
        let root = Self {
            tree_state_u8,
            occupied_chunks_u64: u64::from_be_bytes(chunks_bytes),
            occupied_bits_u64: u64::from_be_bytes(bits_bytes),
            root_node_digest,
        };
        if !root.is_canonical() {
            return None;
        }
        Some(root)
    }

    /// Computes the canonical `ContentTag` for this `OctreeRootV1`.
    pub fn tag(&self) -> ContentTag {
        hash_canonical(
            TagKind::World,
            OCTREE_ROOT_V1_SCHEMA,
            ROOT_PAYLOAD_LEN as u64,
            |sink| {
                sink(&[self.tree_state_u8]);
                sink(&self.occupied_chunks_u64.to_be_bytes());
                sink(&self.occupied_bits_u64.to_be_bytes());
                sink(&self.root_node_digest);
            },
        )
        .expect("OctreeRootV1 tag encoding is canonical")
    }
}

/// One parent internal-node opening along a root-to-leaf [`OctreePointProofV1`] path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OctreeProofStepV1 {
    pub branch_depth: u8,
    pub prefix_bits_180b: [u8; 23],
    pub child_mask: u8,
    pub sibling_digests: Vec<[u8; 32]>,
}

/// Terminal witness carried at the end of an [`OctreePointProofV1`].
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OctreeProofTerminalV1 {
    EmptyTree,
    ChunkLeaf(ChunkLeafV1),
    InternalNode(OctreeInternalNodeV1),
}

/// Self-contained membership or non-membership witness for a query coordinate in [`SparseOctreeV1`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OctreePointProofV1 {
    pub query_coord: Coord,
    pub steps: Vec<OctreeProofStepV1>,
    pub terminal: OctreeProofTerminalV1,
}

impl OctreePointProofV1 {
    /// Returns the canonical byte length of this `OctreePointProofV1` witness.
    pub fn encoded_len(&self) -> usize {
        let steps_bytes: usize = self
            .steps
            .iter()
            .map(|step| INTERNAL_NODE_HEADER_LEN + 32 * step.sibling_digests.len())
            .sum();
        let terminal_bytes = match &self.terminal {
            OctreeProofTerminalV1::EmptyTree => 1,
            OctreeProofTerminalV1::ChunkLeaf(_) => 1 + CHUNK_LEAF_PAYLOAD_LEN,
            OctreeProofTerminalV1::InternalNode(node) => 1 + node.encoded_len(),
        };
        24 + 1 + steps_bytes + terminal_bytes
    }

    /// Encodes this `OctreePointProofV1` witness into its canonical byte vector.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.encoded_len());
        out.extend_from_slice(&self.query_coord.x.to_be_bytes());
        out.extend_from_slice(&self.query_coord.y.to_be_bytes());
        out.extend_from_slice(&self.query_coord.z.to_be_bytes());
        out.push(self.steps.len() as u8);
        for step in &self.steps {
            out.push(step.branch_depth);
            out.extend_from_slice(&step.prefix_bits_180b);
            out.push(step.child_mask);
            for sibling in &step.sibling_digests {
                out.extend_from_slice(sibling);
            }
        }
        match &self.terminal {
            OctreeProofTerminalV1::EmptyTree => {
                out.push(TERMINAL_TAG_EMPTY_TREE);
            }
            OctreeProofTerminalV1::ChunkLeaf(leaf) => {
                out.push(TERMINAL_TAG_CHUNK_LEAF);
                out.extend_from_slice(&leaf.encode());
            }
            OctreeProofTerminalV1::InternalNode(node) => {
                out.push(TERMINAL_TAG_INTERNAL_NODE);
                out.extend_from_slice(&node.encode());
            }
        }
        out
    }

    /// Decodes and validates a canonical `OctreePointProofV1` witness from a byte slice.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 26 {
            return None;
        }
        let mut xb = [0u8; 8];
        xb.copy_from_slice(&bytes[0..8]);
        let mut yb = [0u8; 8];
        yb.copy_from_slice(&bytes[8..16]);
        let mut zb = [0u8; 8];
        zb.copy_from_slice(&bytes[16..24]);
        let query_coord = Coord::new(
            i64::from_be_bytes(xb),
            i64::from_be_bytes(yb),
            i64::from_be_bytes(zb),
        );

        let steps_count = bytes[24] as usize;
        if steps_count > CHUNK_PREFIX_LEVELS as usize {
            return None;
        }
        let mut cursor = 25usize;
        let mut steps = Vec::with_capacity(steps_count);
        for _ in 0..steps_count {
            if bytes.len() - cursor < INTERNAL_NODE_HEADER_LEN {
                return None;
            }
            let branch_depth = bytes[cursor];
            if branch_depth >= CHUNK_PREFIX_LEVELS {
                return None;
            }
            let mut prefix_bits_180b = [0u8; CHUNK_PREFIX_BYTES];
            prefix_bits_180b.copy_from_slice(&bytes[cursor + 1..cursor + 24]);
            if prefix_bits_180b != mask_prefix_180b(prefix_bits_180b, branch_depth) {
                return None;
            }
            let child_mask = bytes[cursor + 24];
            let ones = child_mask.count_ones() as usize;
            if ones < 2 {
                return None;
            }
            cursor += INTERNAL_NODE_HEADER_LEN;
            let sib_count = ones - 1;
            let sib_bytes = sib_count * 32;
            if bytes.len() - cursor < sib_bytes {
                return None;
            }
            let mut sibling_digests = Vec::with_capacity(sib_count);
            for s_idx in 0..sib_count {
                let start = cursor + s_idx * 32;
                let mut sib = [0u8; 32];
                sib.copy_from_slice(&bytes[start..start + 32]);
                sibling_digests.push(sib);
            }
            cursor += sib_bytes;
            steps.push(OctreeProofStepV1 {
                branch_depth,
                prefix_bits_180b,
                child_mask,
                sibling_digests,
            });
        }

        if cursor >= bytes.len() {
            return None;
        }
        let terminal_tag = bytes[cursor];
        cursor += 1;
        let remaining = &bytes[cursor..];
        let terminal = match terminal_tag {
            TERMINAL_TAG_EMPTY_TREE => {
                if !remaining.is_empty() {
                    return None;
                }
                OctreeProofTerminalV1::EmptyTree
            }
            TERMINAL_TAG_CHUNK_LEAF => {
                let leaf = ChunkLeafV1::decode(remaining)?;
                OctreeProofTerminalV1::ChunkLeaf(leaf)
            }
            TERMINAL_TAG_INTERNAL_NODE => {
                let node = OctreeInternalNodeV1::decode(remaining)?;
                OctreeProofTerminalV1::InternalNode(node)
            }
            _ => return None,
        };

        Some(Self {
            query_coord,
            steps,
            terminal,
        })
    }

    /// Computes the canonical `ContentTag` for this `OctreePointProofV1` witness.
    pub fn tag(&self) -> ContentTag {
        let payload_len = self.encoded_len() as u64;
        hash_canonical(
            TagKind::Receipt,
            OCTREE_POINT_PROOF_V1_SCHEMA,
            payload_len,
            |sink| {
                sink(&self.query_coord.x.to_be_bytes());
                sink(&self.query_coord.y.to_be_bytes());
                sink(&self.query_coord.z.to_be_bytes());
                sink(&[self.steps.len() as u8]);
                for step in &self.steps {
                    sink(&[step.branch_depth]);
                    sink(&step.prefix_bits_180b);
                    sink(&[step.child_mask]);
                    for sibling in &step.sibling_digests {
                        sink(sibling);
                    }
                }
                match &self.terminal {
                    OctreeProofTerminalV1::EmptyTree => {
                        sink(&[TERMINAL_TAG_EMPTY_TREE]);
                    }
                    OctreeProofTerminalV1::ChunkLeaf(leaf) => {
                        sink(&[TERMINAL_TAG_CHUNK_LEAF]);
                        sink(&leaf.encode());
                    }
                    OctreeProofTerminalV1::InternalNode(node) => {
                        sink(&[TERMINAL_TAG_INTERNAL_NODE]);
                        sink(&node.encode());
                    }
                }
            },
        )
        .expect("OctreePointProofV1 tag encoding is canonical")
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Eq, PartialEq)]
enum OctreeTreeNode {
    Leaf {
        leaf: ChunkLeafV1,
        digest: [u8; 32],
    },
    Internal {
        node: OctreeInternalNodeV1,
        digest: [u8; 32],
        children: Vec<(u8, OctreeTreeNode)>,
    },
}

impl OctreeTreeNode {
    const fn digest(&self) -> [u8; 32] {
        match *self {
            Self::Leaf { digest, .. } | Self::Internal { digest, .. } => digest,
        }
    }
}

/// Deterministic arity-8 path-compressed sparse cryptographic octree over 180-bit chunk prefixes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SparseOctreeV1 {
    root: OctreeRootV1,
    tree: Option<OctreeTreeNode>,
}

impl SparseOctreeV1 {
    /// Constructs an empty `SparseOctreeV1` with a zeroed `OctreeRootV1` commitment.
    pub fn empty() -> Self {
        Self {
            root: OctreeRootV1::empty(),
            tree: None,
        }
    }

    /// Builds a deterministic path-compressed `SparseOctreeV1` from the occupied chunks of `world`.
    pub fn from_world(world: &World) -> Self {
        let mut leaves: Vec<(ChunkLeafV1, [u8; 32])> = world
            .iter_chunks()
            .map(|(chunk, count, _bounds, words)| {
                let origin = Coord::new(chunk.x * 16, chunk.y * 16, chunk.z * 16);
                let leaf = ChunkLeafV1::from_chunk_words(origin, count, words);
                let digest = leaf.tag().digest();
                (leaf, digest)
            })
            .collect();

        leaves.sort_unstable_by_key(|a| a.0.chunk_prefix_180b);

        if leaves.is_empty() {
            return Self::empty();
        }

        let occupied_chunks_u64 = leaves.len() as u64;
        let occupied_bits_u64: u64 = leaves
            .iter()
            .map(|(leaf, _)| u64::from(leaf.occupied_count_u16))
            .sum();

        if leaves.len() == 1 {
            let (leaf, digest) = leaves.pop().expect("single leaf present");
            return Self {
                root: OctreeRootV1 {
                    tree_state_u8: 1,
                    occupied_chunks_u64: 1,
                    occupied_bits_u64,
                    root_node_digest: digest,
                },
                tree: Some(OctreeTreeNode::Leaf { leaf, digest }),
            };
        }

        let root_node = Self::build_subtree(&leaves);
        let root_node_digest = root_node.digest();
        Self {
            root: OctreeRootV1 {
                tree_state_u8: 2,
                occupied_chunks_u64,
                occupied_bits_u64,
                root_node_digest,
            },
            tree: Some(root_node),
        }
    }

    fn build_subtree(leaves: &[(ChunkLeafV1, [u8; 32])]) -> OctreeTreeNode {
        if leaves.len() == 1 {
            return OctreeTreeNode::Leaf {
                leaf: leaves[0].0.clone(),
                digest: leaves[0].1,
            };
        }
        let first_prefix = &leaves[0].0.chunk_prefix_180b;
        let last_prefix = &leaves[leaves.len() - 1].0.chunk_prefix_180b;
        let branch_depth = common_octant_prefix_len(first_prefix, last_prefix) as u8;
        debug_assert!(branch_depth < CHUNK_PREFIX_LEVELS);
        let prefix_bits_180b = mask_prefix_180b(*first_prefix, branch_depth);

        let mut child_mask = 0u8;
        let mut child_digests = Vec::with_capacity(8);
        let mut children = Vec::with_capacity(8);

        let mut start = 0usize;
        while start < leaves.len() {
            let octant = octant_at_prefix(&leaves[start].0.chunk_prefix_180b, branch_depth);
            let mut end = start + 1;
            while end < leaves.len()
                && octant_at_prefix(&leaves[end].0.chunk_prefix_180b, branch_depth) == octant
            {
                end += 1;
            }
            child_mask |= 1u8 << octant;
            let child_node = Self::build_subtree(&leaves[start..end]);
            child_digests.push(child_node.digest());
            children.push((octant, child_node));
            start = end;
        }

        let node = OctreeInternalNodeV1 {
            branch_depth,
            prefix_bits_180b,
            child_mask,
            child_digests,
        };
        let digest = node.tag().digest();
        OctreeTreeNode::Internal {
            node,
            digest,
            children,
        }
    }

    /// Returns the top-level `OctreeRootV1` header committing to this sparse octree.
    pub const fn root(&self) -> &OctreeRootV1 {
        &self.root
    }

    /// Computes the canonical `ContentTag` of this octree's `OctreeRootV1` header.
    pub fn root_tag(&self) -> ContentTag {
        self.root.tag()
    }

    /// Generates a self-contained `OctreePointProofV1` inclusion or non-inclusion witness for `coord`.
    pub fn prove(&self, coord: Coord) -> OctreePointProofV1 {
        let Some(ref root_node) = self.tree else {
            return OctreePointProofV1 {
                query_coord: coord,
                steps: Vec::new(),
                terminal: OctreeProofTerminalV1::EmptyTree,
            };
        };

        let query_key = MortonKeyV1::from_coord(coord);
        let query_prefix = query_key.chunk_prefix_180b();
        let mut steps = Vec::new();
        let mut current = root_node;

        loop {
            match current {
                OctreeTreeNode::Leaf { leaf, .. } => {
                    return OctreePointProofV1 {
                        query_coord: coord,
                        steps,
                        terminal: OctreeProofTerminalV1::ChunkLeaf(leaf.clone()),
                    };
                }
                OctreeTreeNode::Internal { node, children, .. } => {
                    if common_octant_prefix_len(&node.prefix_bits_180b, &query_prefix)
                        < node.branch_depth as usize
                    {
                        return OctreePointProofV1 {
                            query_coord: coord,
                            steps,
                            terminal: OctreeProofTerminalV1::InternalNode(node.clone()),
                        };
                    }

                    let octant = query_key.octant_at(node.branch_depth);
                    if (node.child_mask & (1u8 << octant)) == 0 {
                        return OctreePointProofV1 {
                            query_coord: coord,
                            steps,
                            terminal: OctreeProofTerminalV1::InternalNode(node.clone()),
                        };
                    }

                    let mut sibling_digests =
                        Vec::with_capacity((node.child_mask.count_ones() - 1) as usize);
                    let mut next_child = None;
                    for (child_octant, child_node) in children {
                        if *child_octant == octant {
                            next_child = Some(child_node);
                        } else {
                            sibling_digests.push(child_node.digest());
                        }
                    }

                    steps.push(OctreeProofStepV1 {
                        branch_depth: node.branch_depth,
                        prefix_bits_180b: node.prefix_bits_180b,
                        child_mask: node.child_mask,
                        sibling_digests,
                    });
                    current = next_child.expect("child_mask bit set implies child exists");
                }
            }
        }
    }

    /// Verifies an `OctreePointProofV1` against `root`, returning `Some(true)` if the voxel is occupied.
    pub fn verify_proof(root: &OctreeRootV1, proof: &OctreePointProofV1) -> Option<bool> {
        if !root.is_canonical() || proof.steps.len() > CHUNK_PREFIX_LEVELS as usize {
            return None;
        }

        let query_key = MortonKeyV1::from_coord(proof.query_coord);
        let query_prefix = query_key.chunk_prefix_180b();

        for (idx, step) in proof.steps.iter().enumerate() {
            if step.branch_depth >= CHUNK_PREFIX_LEVELS {
                return None;
            }
            if idx > 0 && proof.steps[idx - 1].branch_depth >= step.branch_depth {
                return None;
            }
            if step.prefix_bits_180b != mask_prefix_180b(step.prefix_bits_180b, step.branch_depth) {
                return None;
            }
            let ones = step.child_mask.count_ones() as usize;
            if ones < 2 || step.sibling_digests.len() != ones - 1 {
                return None;
            }
            if common_octant_prefix_len(&step.prefix_bits_180b, &query_prefix)
                < step.branch_depth as usize
            {
                return None;
            }
            let octant = query_key.octant_at(step.branch_depth);
            if (step.child_mask & (1u8 << octant)) == 0 {
                return None;
            }
        }

        let (mut current_digest, outcome) = match &proof.terminal {
            OctreeProofTerminalV1::EmptyTree => {
                if root.tree_state_u8 != 0 || !proof.steps.is_empty() {
                    return None;
                }
                ([0u8; 32], false)
            }
            OctreeProofTerminalV1::ChunkLeaf(leaf) => {
                if !leaf.is_canonical() {
                    return None;
                }
                match root.tree_state_u8 {
                    1 => {
                        if !proof.steps.is_empty()
                            || root.occupied_bits_u64 != u64::from(leaf.occupied_count_u16)
                        {
                            return None;
                        }
                    }
                    2 => {
                        let last_step = proof.steps.last()?;
                        if common_octant_prefix_len(&leaf.chunk_prefix_180b, &query_prefix)
                            < (last_step.branch_depth as usize) + 1
                        {
                            return None;
                        }
                    }
                    _ => return None,
                }
                let is_present = if leaf.chunk_prefix_180b == query_prefix {
                    leaf.is_coord_on(proof.query_coord)
                } else {
                    false
                };
                (leaf.tag().digest(), is_present)
            }
            OctreeProofTerminalV1::InternalNode(node) => {
                if root.tree_state_u8 != 2 || !node.is_canonical() {
                    return None;
                }
                if let Some(last_step) = proof.steps.last()
                    && (last_step.branch_depth >= node.branch_depth
                        || common_octant_prefix_len(&node.prefix_bits_180b, &query_prefix)
                            < (last_step.branch_depth as usize) + 1)
                {
                    return None;
                }
                let diverges_before_branch =
                    common_octant_prefix_len(&node.prefix_bits_180b, &query_prefix)
                        < node.branch_depth as usize;
                let octant_vacant =
                    (node.child_mask & (1u8 << query_key.octant_at(node.branch_depth))) == 0;
                if !diverges_before_branch && !octant_vacant {
                    return None;
                }
                (node.tag().digest(), false)
            }
        };

        for step in proof.steps.iter().rev() {
            let query_octant = query_key.octant_at(step.branch_depth);
            let mut child_digests = Vec::with_capacity(step.child_mask.count_ones() as usize);
            let mut sib_iter = step.sibling_digests.iter();
            for octant in 0..8u8 {
                if (step.child_mask & (1u8 << octant)) != 0 {
                    if octant == query_octant {
                        child_digests.push(current_digest);
                    } else {
                        child_digests.push(*sib_iter.next()?);
                    }
                }
            }
            if sib_iter.next().is_some() {
                return None;
            }
            let parent = OctreeInternalNodeV1 {
                branch_depth: step.branch_depth,
                prefix_bits_180b: step.prefix_bits_180b,
                child_mask: step.child_mask,
                child_digests,
            };
            current_digest = parent.tag().digest();
        }

        if current_digest != root.root_node_digest {
            return None;
        }
        Some(outcome)
    }
}

/// Witness-size and hash-count cost row for one authenticated-state candidate in `VERKLE-R1`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerkleCandidateMetricsV1 {
    pub point_witness_bytes_u64: u64,
    pub single_edit_hashes_u64: u64,
    pub chunk_batch_64_hashes_u64: u64,
}

/// Self-contained `VERKLE-R1` feasibility report carrier (`198` payload bytes).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerkleR1FeasibilityReportV1 {
    pub descriptor_version_u16: u16,
    pub arity_u16: u16,
    pub field_modulus_bytes: [u8; 32],
    pub profile_descriptor_digest: [u8; 32],
    pub domain_registry_id_digest: [u8; 32],
    pub trusted_setup_required_u8: u8,
    pub subgroup_check_required_u8: u8,
    pub binary_patricia: VerkleCandidateMetricsV1,
    pub sparse_merkle_192: VerkleCandidateMetricsV1,
    pub sparse_octree_chunk_leaf: VerkleCandidateMetricsV1,
    pub verkle_octree_256: VerkleCandidateMetricsV1,
}

impl VerkleR1FeasibilityReportV1 {
    /// Returns the canonical `VerkleR1FeasibilityReportV1` cost comparison baseline.
    pub fn canonical() -> Self {
        Self {
            descriptor_version_u16: 1,
            arity_u16: 256,
            field_modulus_bytes: [
                0x1c, 0xf4, 0x5c, 0x91, 0x4b, 0x7a, 0x82, 0xd3, 0x6e, 0x10, 0x99, 0x54, 0x2a, 0xb1,
                0x7c, 0x38, 0x05, 0x6f, 0xe2, 0x41, 0x8d, 0x93, 0xc7, 0x1a, 0x50, 0xbe, 0x44, 0x29,
                0x73, 0x0f, 0xa6, 0x01,
            ],
            profile_descriptor_digest: [0x41; 32],
            domain_registry_id_digest: [0x52; 32],
            trusted_setup_required_u8: 0,
            subgroup_check_required_u8: 1,
            binary_patricia: VerkleCandidateMetricsV1 {
                point_witness_bytes_u64: 6_144,
                single_edit_hashes_u64: 192,
                chunk_batch_64_hashes_u64: 12_288,
            },
            sparse_merkle_192: VerkleCandidateMetricsV1 {
                point_witness_bytes_u64: 6_169,
                single_edit_hashes_u64: 192,
                chunk_batch_64_hashes_u64: 12_288,
            },
            sparse_octree_chunk_leaf: VerkleCandidateMetricsV1 {
                point_witness_bytes_u64: 13_977,
                single_edit_hashes_u64: 61,
                chunk_batch_64_hashes_u64: 61,
            },
            verkle_octree_256: VerkleCandidateMetricsV1 {
                point_witness_bytes_u64: 576,
                single_edit_hashes_u64: 17,
                chunk_batch_64_hashes_u64: 17,
            },
        }
    }

    /// Returns whether the descriptor version, arity, digests, and flags satisfy canonical bounds.
    pub fn is_canonical(&self) -> bool {
        self.descriptor_version_u16 >= 1
            && self.arity_u16 >= 2
            && self.field_modulus_bytes != [0u8; 32]
            && self.profile_descriptor_digest != [0u8; 32]
            && self.domain_registry_id_digest != [0u8; 32]
            && self.trusted_setup_required_u8 <= 1
            && self.subgroup_check_required_u8 <= 1
    }

    /// Encodes this `VerkleR1FeasibilityReportV1` into its fixed 198-byte canonical payload array.
    pub fn encode(&self) -> [u8; VERKLE_FEASIBILITY_PAYLOAD_LEN] {
        let mut out = [0u8; VERKLE_FEASIBILITY_PAYLOAD_LEN];
        out[0..2].copy_from_slice(&self.descriptor_version_u16.to_be_bytes());
        out[2..4].copy_from_slice(&self.arity_u16.to_be_bytes());
        out[4..36].copy_from_slice(&self.field_modulus_bytes);
        out[36..68].copy_from_slice(&self.profile_descriptor_digest);
        out[68..100].copy_from_slice(&self.domain_registry_id_digest);
        out[100] = self.trusted_setup_required_u8;
        out[101] = self.subgroup_check_required_u8;

        let rows = [
            self.binary_patricia,
            self.sparse_merkle_192,
            self.sparse_octree_chunk_leaf,
            self.verkle_octree_256,
        ];
        for (idx, row) in rows.iter().enumerate() {
            let base = 102 + idx * 24;
            out[base..base + 8].copy_from_slice(&row.point_witness_bytes_u64.to_be_bytes());
            out[base + 8..base + 16].copy_from_slice(&row.single_edit_hashes_u64.to_be_bytes());
            out[base + 16..base + 24].copy_from_slice(&row.chunk_batch_64_hashes_u64.to_be_bytes());
        }
        out
    }

    /// Decodes and validates a canonical `VerkleR1FeasibilityReportV1` from a 198-byte slice.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != VERKLE_FEASIBILITY_PAYLOAD_LEN {
            return None;
        }
        let descriptor_version_u16 = u16::from_be_bytes([bytes[0], bytes[1]]);
        let arity_u16 = u16::from_be_bytes([bytes[2], bytes[3]]);
        let mut field_modulus_bytes = [0u8; 32];
        field_modulus_bytes.copy_from_slice(&bytes[4..36]);
        let mut profile_descriptor_digest = [0u8; 32];
        profile_descriptor_digest.copy_from_slice(&bytes[36..68]);
        let mut domain_registry_id_digest = [0u8; 32];
        domain_registry_id_digest.copy_from_slice(&bytes[68..100]);
        let trusted_setup_required_u8 = bytes[100];
        let subgroup_check_required_u8 = bytes[101];

        let decode_row = |offset: usize| -> VerkleCandidateMetricsV1 {
            let mut b0 = [0u8; 8];
            let mut b1 = [0u8; 8];
            let mut b2 = [0u8; 8];
            b0.copy_from_slice(&bytes[offset..offset + 8]);
            b1.copy_from_slice(&bytes[offset + 8..offset + 16]);
            b2.copy_from_slice(&bytes[offset + 16..offset + 24]);
            VerkleCandidateMetricsV1 {
                point_witness_bytes_u64: u64::from_be_bytes(b0),
                single_edit_hashes_u64: u64::from_be_bytes(b1),
                chunk_batch_64_hashes_u64: u64::from_be_bytes(b2),
            }
        };

        let report = Self {
            descriptor_version_u16,
            arity_u16,
            field_modulus_bytes,
            profile_descriptor_digest,
            domain_registry_id_digest,
            trusted_setup_required_u8,
            subgroup_check_required_u8,
            binary_patricia: decode_row(102),
            sparse_merkle_192: decode_row(126),
            sparse_octree_chunk_leaf: decode_row(150),
            verkle_octree_256: decode_row(174),
        };
        if !report.is_canonical() {
            return None;
        }
        Some(report)
    }

    /// Computes the canonical `ContentTag` for this `VerkleR1FeasibilityReportV1`.
    pub fn tag(&self) -> ContentTag {
        hash_canonical(
            TagKind::Receipt,
            VERKLE_R1_FEASIBILITY_V1_SCHEMA,
            VERKLE_FEASIBILITY_PAYLOAD_LEN as u64,
            |sink| {
                sink(&self.descriptor_version_u16.to_be_bytes());
                sink(&self.arity_u16.to_be_bytes());
                sink(&self.field_modulus_bytes);
                sink(&self.profile_descriptor_digest);
                sink(&self.domain_registry_id_digest);
                sink(&[
                    self.trusted_setup_required_u8,
                    self.subgroup_check_required_u8,
                ]);
                for row in [
                    self.binary_patricia,
                    self.sparse_merkle_192,
                    self.sparse_octree_chunk_leaf,
                    self.verkle_octree_256,
                ] {
                    sink(&row.point_witness_bytes_u64.to_be_bytes());
                    sink(&row.single_edit_hashes_u64.to_be_bytes());
                    sink(&row.chunk_batch_64_hashes_u64.to_be_bytes());
                }
            },
        )
        .expect("VerkleR1FeasibilityReportV1 tag encoding is canonical")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_multi_chunk_probe_world() -> World {
        let mut world = World::new();
        for coord in [
            Coord::new(-17, 2, 15),
            Coord::new(-16, -16, -16),
            Coord::new(32, 48, -16),
            Coord::new(33, 48, -16),
            Coord::new(64, 48, -16),
        ] {
            world.set_on(coord);
        }
        world
    }

    #[test]
    fn schema_constants_and_prefix_helpers_satisfy_octant_invariants() {
        assert_eq!(OCTREE_CHUNK_LEAF_V1_SCHEMA.len(), 36);
        assert_eq!(OCTREE_INTERNAL_NODE_V1_SCHEMA.len(), 39);
        assert_eq!(OCTREE_ROOT_V1_SCHEMA.len(), 30);
        assert_eq!(OCTREE_POINT_PROOF_V1_SCHEMA.len(), 37);
        assert_eq!(VERKLE_R1_FEASIBILITY_V1_SCHEMA.len(), 47);

        let p0 = MortonKeyV1::from_coord(Coord::new(0, 0, 0)).chunk_prefix_180b();
        let p_neg = MortonKeyV1::from_coord(Coord::new(-16, -16, -16)).chunk_prefix_180b();
        let p_32 = MortonKeyV1::from_coord(Coord::new(32, 48, -16)).chunk_prefix_180b();
        let p_64 = MortonKeyV1::from_coord(Coord::new(64, 48, -16)).chunk_prefix_180b();

        assert_eq!(octant_at_prefix(&p0, 0), 7);
        assert_eq!(octant_at_prefix(&p0, 59), 0);
        assert_eq!(octant_at_prefix(&p0, 60), 0);
        assert_eq!(octant_at_prefix(&p0, 255), 0);

        assert_eq!(common_octant_prefix_len(&p0, &p0), 60);
        assert_eq!(common_octant_prefix_len(&p0, &p_neg), 0);
        assert_eq!(common_octant_prefix_len(&p_32, &p_64), 57);

        assert_eq!(mask_prefix_180b([0xFF; 23], 0), [0u8; 23]);
        let masked_1 = mask_prefix_180b([0xFF; 23], 1);
        assert_eq!(masked_1[0], 0b1110_0000);
        assert!(masked_1[1..].iter().all(|&b| b == 0));

        let masked_60 = mask_prefix_180b([0xFF; 23], 60);
        assert!(masked_60[0..22].iter().all(|&b| b == 0xFF));
        assert_eq!(masked_60[22], 0xF0);
    }

    #[test]
    fn empty_and_single_origin_octree_roots_match_pinned_tags() {
        let empty = SparseOctreeV1::empty();
        assert_eq!(empty.root(), &OctreeRootV1::empty());
        assert!(empty.root().is_canonical());
        assert_eq!(
            OctreeRootV1::decode(&empty.root().encode()),
            Some(*empty.root())
        );
        assert_eq!(
            empty.root_tag().to_string(),
            "#bb.world.sha256.5obybgly3kj2356d426gjijnuezscj3rz556r2el3xue7mccedea"
        );

        let empty_proof = empty.prove(Coord::new(5, -7, 9));
        assert_eq!(empty_proof.terminal, OctreeProofTerminalV1::EmptyTree);
        assert_eq!(
            OctreePointProofV1::decode(&empty_proof.encode()),
            Some(empty_proof.clone())
        );
        assert_eq!(
            SparseOctreeV1::verify_proof(empty.root(), &empty_proof),
            Some(false)
        );

        let single_world = World::new();
        let single_tree = SparseOctreeV1::from_world(&single_world);
        assert_eq!(single_tree.root().tree_state_u8, 1);
        assert_eq!(single_tree.root().occupied_chunks_u64, 1);
        assert_eq!(single_tree.root().occupied_bits_u64, 1);
        assert_eq!(
            single_tree.root_tag().to_string(),
            "#bb.world.sha256.lmjzqddt5hcdszpiwkaf3ckxdtk25mcdm4go4zav6i3r4gn6kksa"
        );

        let p_origin = single_tree.prove(Coord::new(0, 0, 0));
        assert_eq!(
            SparseOctreeV1::verify_proof(single_tree.root(), &p_origin),
            Some(true)
        );
        let p_same_chunk_off = single_tree.prove(Coord::new(1, 2, 3));
        assert_eq!(
            SparseOctreeV1::verify_proof(single_tree.root(), &p_same_chunk_off),
            Some(false)
        );
        let p_other_chunk = single_tree.prove(Coord::new(-32, 16, 48));
        assert_eq!(
            SparseOctreeV1::verify_proof(single_tree.root(), &p_other_chunk),
            Some(false)
        );
    }

    #[test]
    fn multi_chunk_octree_pins_all_five_schemas_and_verifies_all_proof_cases() {
        let world = build_multi_chunk_probe_world();
        let octree = SparseOctreeV1::from_world(&world);
        assert_eq!(octree.root().tree_state_u8, 2);
        assert_eq!(octree.root().occupied_chunks_u64, 5);
        assert_eq!(octree.root().occupied_bits_u64, 6);
        assert_eq!(
            OctreeRootV1::decode(&octree.root().encode()),
            Some(*octree.root())
        );
        assert_eq!(
            octree.root_tag().to_string(),
            "#bb.world.sha256.23yz24urod2pxhthrpvpeumsctsphytdbzjkkayvrguu7evdt3aq"
        );

        // Present coordinate in chunk (2, 3, -1) with occupied_count_u16 == 2
        let present_coord = Coord::new(33, 48, -16);
        let present_proof = octree.prove(present_coord);
        assert_eq!(present_proof.steps.len(), 2);
        let first_step: &OctreeProofStepV1 = &present_proof.steps[0];
        assert_eq!(first_step.branch_depth, 0);
        assert_eq!(present_proof.steps[1].branch_depth, 57);
        assert_eq!(
            OctreePointProofV1::decode(&present_proof.encode()),
            Some(present_proof.clone())
        );
        assert_eq!(
            SparseOctreeV1::verify_proof(octree.root(), &present_proof),
            Some(true)
        );
        assert_eq!(
            present_proof.tag().to_string(),
            "#bb.receipt.sha256.uhtkmfstiktj3fnu6kbeon3s5gjp2wq3bhx4rr5load72iy2etva"
        );

        // Extract and pin the non-zero ChunkLeafV1 (chunk (2, 3, -1), count = 2)
        let OctreeProofTerminalV1::ChunkLeaf(ref leaf_23m1) = present_proof.terminal else {
            panic!("expected ChunkLeaf terminal for present coordinate");
        };
        assert_eq!(leaf_23m1.occupied_count_u16, 2);
        assert!(leaf_23m1.is_canonical());
        let mut manual_words = [0u64; CHUNK_WORDS];
        manual_words[0] = 0b11;
        let manual_leaf = ChunkLeafV1::from_chunk_words(Coord::new(32, 48, -16), 2, &manual_words);
        assert_eq!(leaf_23m1, &manual_leaf);
        assert!(leaf_23m1.is_coord_on(Coord::new(32, 48, -16)));
        assert!(leaf_23m1.is_coord_on(Coord::new(33, 48, -16)));
        assert!(!leaf_23m1.is_coord_on(Coord::new(34, 48, -16)));
        assert_eq!(
            ChunkLeafV1::decode(&leaf_23m1.encode()),
            Some(leaf_23m1.clone())
        );
        assert_eq!(
            leaf_23m1.tag().to_string(),
            "#bb.world.sha256.ycc56dgal4ijy7675edhvd3wnaa7k2ukrdfma2f6m7nejfwsxnqq"
        );

        // Absent coordinate in the same allocated chunk (bit == 0)
        let same_chunk_absent = octree.prove(Coord::new(35, 48, -16));
        assert_eq!(
            SparseOctreeV1::verify_proof(octree.root(), &same_chunk_absent),
            Some(false)
        );

        // Absent coordinate reaching a ChunkLeaf whose 180-bit prefix differs after depth 57
        let divergent_leaf_absent = octree.prove(Coord::new(16, 48, -16));
        assert!(matches!(
            divergent_leaf_absent.terminal,
            OctreeProofTerminalV1::ChunkLeaf(_)
        ));
        assert_eq!(
            SparseOctreeV1::verify_proof(octree.root(), &divergent_leaf_absent),
            Some(false)
        );

        // Absent coordinate terminating at deeper InternalNode (branch_depth = 57) via prefix divergence
        let divergent_internal_proof = octree.prove(Coord::new(128, 48, -16));
        assert_eq!(divergent_internal_proof.steps.len(), 1);
        let OctreeProofTerminalV1::InternalNode(ref internal_bd57) =
            divergent_internal_proof.terminal
        else {
            panic!("expected InternalNode terminal at branch_depth 57");
        };
        assert_eq!(internal_bd57.branch_depth, 57);
        assert!(internal_bd57.is_canonical());
        assert_eq!(
            OctreeInternalNodeV1::decode(&internal_bd57.encode()),
            Some(internal_bd57.clone())
        );
        assert_eq!(
            internal_bd57.tag().to_string(),
            "#bb.world.sha256.3sm334gwkaamsw644mryh6hoyxalhkbdassaal2rbuwqzfposrgq"
        );
        assert_eq!(
            SparseOctreeV1::verify_proof(octree.root(), &divergent_internal_proof),
            Some(false)
        );

        // Absent coordinate terminating at deeper InternalNode (branch_depth = 57) via vacant child octant
        let vacant_child_internal = octree.prove(Coord::new(32, 64, -16));
        assert_eq!(vacant_child_internal.steps.len(), 1);
        assert!(matches!(
            vacant_child_internal.terminal,
            OctreeProofTerminalV1::InternalNode(_)
        ));
        assert_eq!(
            SparseOctreeV1::verify_proof(octree.root(), &vacant_child_internal),
            Some(false)
        );

        // Absent coordinate terminating at root InternalNode (branch_depth = 0) via vacant child octant
        let vacant_root_octant = octree.prove(Coord::new(16, -16, 32));
        assert!(vacant_root_octant.steps.is_empty());
        assert!(matches!(
            vacant_root_octant.terminal,
            OctreeProofTerminalV1::InternalNode(_)
        ));
        assert_eq!(
            SparseOctreeV1::verify_proof(octree.root(), &vacant_root_octant),
            Some(false)
        );
    }

    #[test]
    fn proof_verifier_and_decoders_reject_forged_and_malformed_inputs() {
        let world = build_multi_chunk_probe_world();
        let octree = SparseOctreeV1::from_world(&world);
        let valid_proof = octree.prove(Coord::new(33, 48, -16));

        // 1. Spoofed query_coord fails verification
        let mut spoofed_coord = valid_proof.clone();
        spoofed_coord.query_coord = Coord::new(32, -48, -16);
        assert_eq!(
            SparseOctreeV1::verify_proof(octree.root(), &spoofed_coord),
            None
        );

        // 2. Corrupted sibling digest fails root hash-chain match
        let mut bad_sibling = valid_proof.clone();
        bad_sibling.steps[0].sibling_digests[0][0] ^= 0x01;
        assert_eq!(
            SparseOctreeV1::verify_proof(octree.root(), &bad_sibling),
            None
        );

        // 3. Non-increasing step branch_depth fails verification
        let mut bad_step_order = valid_proof.clone();
        bad_step_order.steps[0].branch_depth = 57;
        assert_eq!(
            SparseOctreeV1::verify_proof(octree.root(), &bad_step_order),
            None
        );

        // 4. Unmasked prefix_bits_180b in step fails verification and decoding
        let mut unmasked_step = valid_proof.clone();
        unmasked_step.steps[0].prefix_bits_180b[22] = 0x10;
        assert_eq!(
            SparseOctreeV1::verify_proof(octree.root(), &unmasked_step),
            None
        );
        assert_eq!(OctreePointProofV1::decode(&unmasked_step.encode()), None);

        // 5. Premature InternalNode stop (where query octant bit is actually present) fails
        let divergent_proof = octree.prove(Coord::new(128, 48, -16));
        let mut premature_internal = divergent_proof.clone();
        premature_internal.query_coord = Coord::new(33, 48, -16);
        assert_eq!(
            SparseOctreeV1::verify_proof(octree.root(), &premature_internal),
            None
        );

        // 6. Malformed ChunkLeafV1 (low nibble non-zero or popcount mismatch) fails
        let OctreeProofTerminalV1::ChunkLeaf(ref valid_leaf) = valid_proof.terminal else {
            panic!("expected ChunkLeaf");
        };
        let mut bad_leaf_nibble = valid_leaf.clone();
        bad_leaf_nibble.chunk_prefix_180b[22] |= 0x01;
        assert!(!bad_leaf_nibble.is_canonical());
        assert_eq!(ChunkLeafV1::decode(&bad_leaf_nibble.encode()), None);

        let mut bad_leaf_count = valid_leaf.clone();
        bad_leaf_count.occupied_count_u16 = 3;
        assert!(!bad_leaf_count.is_canonical());
        assert_eq!(ChunkLeafV1::decode(&bad_leaf_count.encode()), None);

        // 7. Malformed OctreeRootV1 fails
        let mut bad_root = *octree.root();
        bad_root.tree_state_u8 = 3;
        assert!(!bad_root.is_canonical());
        assert_eq!(OctreeRootV1::decode(&bad_root.encode()), None);
        assert_eq!(SparseOctreeV1::verify_proof(&bad_root, &valid_proof), None);
    }

    #[test]
    fn verkle_r1_feasibility_report_matches_pinned_canonical_tag() {
        let report = VerkleR1FeasibilityReportV1::canonical();
        assert!(report.is_canonical());
        assert_eq!(report.descriptor_version_u16, 1);
        assert_eq!(report.arity_u16, 256);
        assert_eq!(
            report.binary_patricia,
            VerkleCandidateMetricsV1 {
                point_witness_bytes_u64: 6_144,
                single_edit_hashes_u64: 192,
                chunk_batch_64_hashes_u64: 12_288,
            }
        );
        assert_eq!(
            report.verkle_octree_256,
            VerkleCandidateMetricsV1 {
                point_witness_bytes_u64: 576,
                single_edit_hashes_u64: 17,
                chunk_batch_64_hashes_u64: 17,
            }
        );

        let encoded = report.encode();
        assert_eq!(encoded.len(), 198);
        assert_eq!(
            VerkleR1FeasibilityReportV1::decode(&encoded),
            Some(report.clone())
        );
        assert_eq!(
            report.tag().to_string(),
            "#bb.receipt.sha256.j3xacn27gfhq6f4wrxvn5upcvvufq5soltxncytvglb2recohp7a"
        );

        let mut invalid_report = report;
        invalid_report.trusted_setup_required_u8 = 2;
        assert!(!invalid_report.is_canonical());
        assert_eq!(
            VerkleR1FeasibilityReportV1::decode(&invalid_report.encode()),
            None
        );
    }

    #[test]
    fn scale_workload_roots_and_morton_leaf_ordering_match_specification() {
        let mut neg_cluster = World::new();
        for x in -1..=0 {
            for y in -1..=0 {
                for z in -1..=0 {
                    neg_cluster.set_on(Coord::new(x, y, z));
                }
            }
        }
        neg_cluster.set_on(Coord::new(-16, -16, -16));
        let neg_octree = SparseOctreeV1::from_world(&neg_cluster);
        assert_eq!(neg_octree.root().occupied_chunks_u64, 8);
        assert_eq!(neg_octree.root().occupied_bits_u64, 9);
        assert_eq!(
            neg_octree.root_tag().to_string(),
            "#bb.world.sha256.74xdmb3l7eofhba23gxrrrazc42ozvckup4b27fg6gvtdgtwl6qa"
        );

        let mut far_extrema = World::new();
        for sx in [-1_048_576_i64, 1_048_576_i64] {
            for sy in [-1_048_576_i64, 1_048_576_i64] {
                for sz in [-1_048_576_i64, 1_048_576_i64] {
                    far_extrema.set_on(Coord::new(sx, sy, sz));
                }
            }
        }
        let far_octree = SparseOctreeV1::from_world(&far_extrema);
        assert_eq!(far_octree.root().occupied_chunks_u64, 9);
        assert_eq!(far_octree.root().occupied_bits_u64, 9);
        assert_eq!(
            far_octree.root_tag().to_string(),
            "#bb.world.sha256.j2yx5roy6ct2pmqnvnzrx72gsm7rmi3o7zol2p6sokh76lahiyaa"
        );
    }
}
