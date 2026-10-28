//! `SCALE-1` workload assay and comparative synthesis across authenticated-state
//! and spatial-query candidates (`bitblock/scale-assay/workload-report/v1`).
//!
//! Evaluates five deterministic reference worlds (`SingleOrigin`, `DenseCube16`,
//! `HollowShell16`, `NegativeChunkCluster`, and `SparseFarExtrema`) for chunk
//! allocation, macro-octant root count, bitwise surface-area versus buried-volume
//! face accounting, view-frustum chunk culling, slab-monotone raycast chunk pruning,
//! and cryptographic state-root digests (`MerklePatriciaTrie`, `SparseMerkleTree`,
//! and `SparseOctreeV1`).

use std::fmt;

use crate::camera::Camera;
use crate::commit_r1::{MerklePatriciaTrie, SparseMerkleTree};
use crate::octree_commit::SparseOctreeV1;
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::render::{bounds_aabb, ray_aabb_entry_distance};
use crate::world::{Coord, World};

/// Canonical schema string for v1 `SCALE-1` comparative workload assay reports.
pub const SCALE_WORKLOAD_REPORT_V1_SCHEMA: &[u8] = b"bitblock/scale-assay/workload-report/v1";

const WORKLOAD_COUNT: usize = 5;
const WORKLOAD_ENTRY_LEN: usize = 185;
const WORKLOAD_REPORT_PAYLOAD_LEN: usize = 4 + WORKLOAD_COUNT * WORKLOAD_ENTRY_LEN;
const ASSAY_VIEWPORT_WIDTH: u32 = 64;
const ASSAY_VIEWPORT_HEIGHT: u32 = 64;

/// Canonical identifier for one deterministic `SCALE-1` assay workload (`0..5`).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum ScaleWorkloadId {
    SingleOrigin = 0,
    DenseCube16 = 1,
    HollowShell16 = 2,
    NegativeChunkCluster = 3,
    SparseFarExtrema = 4,
}

/// Ordered array of all five canonical `ScaleWorkloadId` variants evaluated by `SCALE-1`.
pub const ALL_SCALE_WORKLOADS: [ScaleWorkloadId; 5] = [
    ScaleWorkloadId::SingleOrigin,
    ScaleWorkloadId::DenseCube16,
    ScaleWorkloadId::HollowShell16,
    ScaleWorkloadId::NegativeChunkCluster,
    ScaleWorkloadId::SparseFarExtrema,
];

/// Builds the deterministic reference [`World`] for one [`ScaleWorkloadId`].
pub fn build_workload_world(id: ScaleWorkloadId) -> World {
    let mut world = World::new();
    match id {
        ScaleWorkloadId::SingleOrigin => {}
        ScaleWorkloadId::DenseCube16 => {
            for x in 0..16_i64 {
                for y in 0..16_i64 {
                    for z in 0..16_i64 {
                        world.set_on(Coord::new(x, y, z));
                    }
                }
            }
        }
        ScaleWorkloadId::HollowShell16 => {
            for x in 0..16_i64 {
                for y in 0..16_i64 {
                    for z in 0..16_i64 {
                        if x == 0 || x == 15 || y == 0 || y == 15 || z == 0 || z == 15 {
                            world.set_on(Coord::new(x, y, z));
                        }
                    }
                }
            }
        }
        ScaleWorkloadId::NegativeChunkCluster => {
            for idx in 0..8_i64 {
                let dx = -((idx >> 2) & 1);
                let dy = -((idx >> 1) & 1);
                let dz = -(idx & 1);
                world.set_on(Coord::new(dx, dy, dz));
            }
            world.set_on(Coord::new(-16, -16, -16));
        }
        ScaleWorkloadId::SparseFarExtrema => {
            let span = 1_048_576_i64;
            for mask in 0..8_u8 {
                let sx = if (mask & 4) != 0 { span } else { -span };
                let sy = if (mask & 2) != 0 { span } else { -span };
                let sz = if (mask & 1) != 0 { span } else { -span };
                world.set_on(Coord::new(sx, sy, sz));
            }
        }
    }
    world
}

/// Validation failure when decoding or verifying a [`ScaleWorkloadReportV1`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScaleAssayError {
    InvalidPayloadLength {
        actual: usize,
    },
    InvalidWorkloadCount {
        count: u32,
    },
    InvalidWorkloadId {
        index: usize,
        actual: u8,
    },
    InvalidOccupancyBounds {
        index: usize,
        allocated_bits_u64: u64,
        allocated_chunks_u64: u64,
        active_root_octants_u64: u64,
        fully_buried_voxels_u64: u64,
    },
    FaceTotalMismatch {
        index: usize,
        allocated_bits_u64: u64,
        total_cube_faces_u64: u64,
    },
    SurfaceBuriedSumMismatch {
        index: usize,
        exposed_surface_faces_u64: u64,
        buried_interior_faces_u64: u64,
        total_cube_faces_u64: u64,
    },
    FrustumChunkSumMismatch {
        index: usize,
        frustum_visible_chunks_u64: u64,
        frustum_culled_chunks_u64: u64,
        allocated_chunks_u64: u64,
    },
    RaycastChunkSumMismatch {
        index: usize,
        raycast_tested_chunks_u64: u64,
        raycast_pruned_chunks_u64: u64,
        allocated_chunks_u64: u64,
    },
    ZeroCommitmentDigest {
        index: usize,
    },
}

impl fmt::Display for ScaleAssayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::InvalidPayloadLength { actual } => write!(
                f,
                "ScaleWorkloadReportV1 payload length {actual} does not equal expected {WORKLOAD_REPORT_PAYLOAD_LEN}"
            ),
            Self::InvalidWorkloadCount { count } => write!(
                f,
                "ScaleWorkloadReportV1 workload count {count} does not equal expected {WORKLOAD_COUNT}"
            ),
            Self::InvalidWorkloadId { index, actual } => write!(
                f,
                "ScaleWorkloadReportV1 entry {index} has workload_id_u8={actual} (expected {index})"
            ),
            Self::InvalidOccupancyBounds {
                index,
                allocated_bits_u64,
                allocated_chunks_u64,
                active_root_octants_u64,
                fully_buried_voxels_u64,
            } => write!(
                f,
                "ScaleWorkloadReportV1 entry {index} has invalid occupancy bounds (bits={allocated_bits_u64}, chunks={allocated_chunks_u64}, roots={active_root_octants_u64}, buried_voxels={fully_buried_voxels_u64})"
            ),
            Self::FaceTotalMismatch {
                index,
                allocated_bits_u64,
                total_cube_faces_u64,
            } => write!(
                f,
                "ScaleWorkloadReportV1 entry {index} total_cube_faces_u64={total_cube_faces_u64} != allocated_bits_u64 * 6 ({allocated_bits_u64} * 6)"
            ),
            Self::SurfaceBuriedSumMismatch {
                index,
                exposed_surface_faces_u64,
                buried_interior_faces_u64,
                total_cube_faces_u64,
            } => write!(
                f,
                "ScaleWorkloadReportV1 entry {index} exposed ({exposed_surface_faces_u64}) + buried ({buried_interior_faces_u64}) != total ({total_cube_faces_u64})"
            ),
            Self::FrustumChunkSumMismatch {
                index,
                frustum_visible_chunks_u64,
                frustum_culled_chunks_u64,
                allocated_chunks_u64,
            } => write!(
                f,
                "ScaleWorkloadReportV1 entry {index} frustum_visible ({frustum_visible_chunks_u64}) + frustum_culled ({frustum_culled_chunks_u64}) != allocated_chunks ({allocated_chunks_u64})"
            ),
            Self::RaycastChunkSumMismatch {
                index,
                raycast_tested_chunks_u64,
                raycast_pruned_chunks_u64,
                allocated_chunks_u64,
            } => write!(
                f,
                "ScaleWorkloadReportV1 entry {index} raycast_tested ({raycast_tested_chunks_u64}) + raycast_pruned ({raycast_pruned_chunks_u64}) != allocated_chunks ({allocated_chunks_u64})"
            ),
            Self::ZeroCommitmentDigest { index } => write!(
                f,
                "ScaleWorkloadReportV1 entry {index} contains an all-zero commitment digest"
            ),
        }
    }
}

impl std::error::Error for ScaleAssayError {}

/// Fixed 185-byte comparative assay record for one `SCALE-1` workload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScaleWorkloadEntryV1 {
    pub workload_id_u8: u8,
    pub allocated_bits_u64: u64,
    pub allocated_chunks_u64: u64,
    pub active_root_octants_u64: u64,
    pub total_cube_faces_u64: u64,
    pub exposed_surface_faces_u64: u64,
    pub buried_interior_faces_u64: u64,
    pub fully_buried_voxels_u64: u64,
    pub frustum_visible_chunks_u64: u64,
    pub frustum_culled_chunks_u64: u64,
    pub raycast_tested_chunks_u64: u64,
    pub raycast_pruned_chunks_u64: u64,
    pub patricia_root_digest: [u8; 32],
    pub smt_root_digest: [u8; 32],
    pub octree_root_digest: [u8; 32],
}

impl ScaleWorkloadEntryV1 {
    /// Validates occupancy bounds, face-sum identities, chunk splits, and non-zero digests.
    pub fn validate(&self, expected_index: usize) -> Result<(), ScaleAssayError> {
        if usize::from(self.workload_id_u8) != expected_index {
            return Err(ScaleAssayError::InvalidWorkloadId {
                index: expected_index,
                actual: self.workload_id_u8,
            });
        }
        if self.allocated_bits_u64 == 0
            || self.allocated_chunks_u64 == 0
            || self.active_root_octants_u64 == 0
            || self.allocated_chunks_u64 > self.allocated_bits_u64
            || self.active_root_octants_u64 > self.allocated_chunks_u64
            || self.fully_buried_voxels_u64 > self.allocated_bits_u64
        {
            return Err(ScaleAssayError::InvalidOccupancyBounds {
                index: expected_index,
                allocated_bits_u64: self.allocated_bits_u64,
                allocated_chunks_u64: self.allocated_chunks_u64,
                active_root_octants_u64: self.active_root_octants_u64,
                fully_buried_voxels_u64: self.fully_buried_voxels_u64,
            });
        }
        if self.allocated_bits_u64.checked_mul(6) != Some(self.total_cube_faces_u64) {
            return Err(ScaleAssayError::FaceTotalMismatch {
                index: expected_index,
                allocated_bits_u64: self.allocated_bits_u64,
                total_cube_faces_u64: self.total_cube_faces_u64,
            });
        }
        if self
            .exposed_surface_faces_u64
            .checked_add(self.buried_interior_faces_u64)
            != Some(self.total_cube_faces_u64)
        {
            return Err(ScaleAssayError::SurfaceBuriedSumMismatch {
                index: expected_index,
                exposed_surface_faces_u64: self.exposed_surface_faces_u64,
                buried_interior_faces_u64: self.buried_interior_faces_u64,
                total_cube_faces_u64: self.total_cube_faces_u64,
            });
        }
        if self
            .frustum_visible_chunks_u64
            .checked_add(self.frustum_culled_chunks_u64)
            != Some(self.allocated_chunks_u64)
        {
            return Err(ScaleAssayError::FrustumChunkSumMismatch {
                index: expected_index,
                frustum_visible_chunks_u64: self.frustum_visible_chunks_u64,
                frustum_culled_chunks_u64: self.frustum_culled_chunks_u64,
                allocated_chunks_u64: self.allocated_chunks_u64,
            });
        }
        if self
            .raycast_tested_chunks_u64
            .checked_add(self.raycast_pruned_chunks_u64)
            != Some(self.allocated_chunks_u64)
        {
            return Err(ScaleAssayError::RaycastChunkSumMismatch {
                index: expected_index,
                raycast_tested_chunks_u64: self.raycast_tested_chunks_u64,
                raycast_pruned_chunks_u64: self.raycast_pruned_chunks_u64,
                allocated_chunks_u64: self.allocated_chunks_u64,
            });
        }
        if self.patricia_root_digest == [0u8; 32]
            || self.smt_root_digest == [0u8; 32]
            || self.octree_root_digest == [0u8; 32]
        {
            return Err(ScaleAssayError::ZeroCommitmentDigest {
                index: expected_index,
            });
        }
        Ok(())
    }

    /// Encodes this workload entry into its canonical 185-byte big-endian binary record.
    pub fn encode(&self) -> [u8; WORKLOAD_ENTRY_LEN] {
        let mut out = [0u8; WORKLOAD_ENTRY_LEN];
        out[0] = self.workload_id_u8;
        let fields = [
            self.allocated_bits_u64,
            self.allocated_chunks_u64,
            self.active_root_octants_u64,
            self.total_cube_faces_u64,
            self.exposed_surface_faces_u64,
            self.buried_interior_faces_u64,
            self.fully_buried_voxels_u64,
            self.frustum_visible_chunks_u64,
            self.frustum_culled_chunks_u64,
            self.raycast_tested_chunks_u64,
            self.raycast_pruned_chunks_u64,
        ];
        for (idx, val) in fields.iter().enumerate() {
            let offset = 1 + idx * 8;
            out[offset..offset + 8].copy_from_slice(&val.to_be_bytes());
        }
        out[89..121].copy_from_slice(&self.patricia_root_digest);
        out[121..153].copy_from_slice(&self.smt_root_digest);
        out[153..185].copy_from_slice(&self.octree_root_digest);
        out
    }

    /// Decodes and validates a 185-byte canonical record for the workload at `expected_index`.
    pub fn decode(bytes: &[u8], expected_index: usize) -> Result<Self, ScaleAssayError> {
        if bytes.len() != WORKLOAD_ENTRY_LEN {
            return Err(ScaleAssayError::InvalidPayloadLength {
                actual: bytes.len(),
            });
        }
        let read_u64 = |offset: usize| -> u64 {
            let mut buf = [0u8; 8];
            buf.copy_from_slice(&bytes[offset..offset + 8]);
            u64::from_be_bytes(buf)
        };
        let mut patricia_root_digest = [0u8; 32];
        patricia_root_digest.copy_from_slice(&bytes[89..121]);
        let mut smt_root_digest = [0u8; 32];
        smt_root_digest.copy_from_slice(&bytes[121..153]);
        let mut octree_root_digest = [0u8; 32];
        octree_root_digest.copy_from_slice(&bytes[153..185]);

        let entry = Self {
            workload_id_u8: bytes[0],
            allocated_bits_u64: read_u64(1),
            allocated_chunks_u64: read_u64(9),
            active_root_octants_u64: read_u64(17),
            total_cube_faces_u64: read_u64(25),
            exposed_surface_faces_u64: read_u64(33),
            buried_interior_faces_u64: read_u64(41),
            fully_buried_voxels_u64: read_u64(49),
            frustum_visible_chunks_u64: read_u64(57),
            frustum_culled_chunks_u64: read_u64(65),
            raycast_tested_chunks_u64: read_u64(73),
            raycast_pruned_chunks_u64: read_u64(81),
            patricia_root_digest,
            smt_root_digest,
            octree_root_digest,
        };
        entry.validate(expected_index)?;
        Ok(entry)
    }
}

/// Computes the deterministic [`ScaleWorkloadEntryV1`] for a single [`ScaleWorkloadId`].
pub fn assay_workload(id: ScaleWorkloadId) -> ScaleWorkloadEntryV1 {
    let world = build_workload_world(id);
    let allocated_bits_u64 = world.len() as u64;
    let allocated_chunks_u64 = world.allocated_chunk_count() as u64;
    let active_root_octants_u64 = world.root_octants().count() as u64;

    let mut exposed_surface_faces_u64 = 0_u64;
    let mut fully_buried_voxels_u64 = 0_u64;
    for coord in world.iter_on() {
        let mask = world.exposed_faces(coord);
        exposed_surface_faces_u64 += u64::from(mask.count_ones());
        if mask == 0 {
            fully_buried_voxels_u64 += 1;
        }
    }
    let total_cube_faces_u64 = allocated_bits_u64 * 6;
    let buried_interior_faces_u64 = total_cube_faces_u64 - exposed_surface_faces_u64;

    let camera = Camera::for_observer_spawn();
    let projection = camera
        .projection_for_viewport(ASSAY_VIEWPORT_WIDTH, ASSAY_VIEWPORT_HEIGHT)
        .expect("64x64 assay viewport must produce a valid projection");
    let ray = camera.center_ray();

    let mut chunks: Vec<_> = world
        .iter_chunks()
        .map(|(chunk_coord, _count, chunk_bounds, _words)| (chunk_coord, chunk_bounds))
        .collect();
    chunks.sort_by_key(|(c, _)| *c);

    let mut frustum_culled_chunks_u64 = 0_u64;
    let mut raycast_tested_chunks_u64 = 0_u64;
    for (_chunk_coord, chunk_bounds) in chunks {
        let (cmin, cmax) = bounds_aabb(chunk_bounds);
        if projection.aabb_definitely_outside_frustum(cmin, cmax) {
            frustum_culled_chunks_u64 += 1;
        }
        if ray_aabb_entry_distance(ray, cmin, cmax).is_some() {
            raycast_tested_chunks_u64 += 1;
        }
    }
    let frustum_visible_chunks_u64 = allocated_chunks_u64 - frustum_culled_chunks_u64;
    let raycast_pruned_chunks_u64 = allocated_chunks_u64 - raycast_tested_chunks_u64;

    let patricia = MerklePatriciaTrie::from_world(&world);
    let smt = SparseMerkleTree::from_world(&world);
    let octree = SparseOctreeV1::from_world(&world);

    ScaleWorkloadEntryV1 {
        workload_id_u8: id as u8,
        allocated_bits_u64,
        allocated_chunks_u64,
        active_root_octants_u64,
        total_cube_faces_u64,
        exposed_surface_faces_u64,
        buried_interior_faces_u64,
        fully_buried_voxels_u64,
        frustum_visible_chunks_u64,
        frustum_culled_chunks_u64,
        raycast_tested_chunks_u64,
        raycast_pruned_chunks_u64,
        patricia_root_digest: patricia.root_tag().digest(),
        smt_root_digest: smt.root_tag().digest(),
        octree_root_digest: octree.root_tag().digest(),
    }
}

/// Fixed 929-byte `SCALE-1` comparative workload report across all 5 reference workloads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScaleWorkloadReportV1 {
    pub entries: [ScaleWorkloadEntryV1; WORKLOAD_COUNT],
}

impl ScaleWorkloadReportV1 {
    /// Evaluates all five `ALL_SCALE_WORKLOADS` reference worlds into a canonical report.
    pub fn canonical() -> Self {
        Self {
            entries: ALL_SCALE_WORKLOADS.map(assay_workload),
        }
    }

    /// Validates all five workload entries against their expected ordinal indices.
    pub fn validate(&self) -> Result<(), ScaleAssayError> {
        for (idx, entry) in self.entries.iter().enumerate() {
            entry.validate(idx)?;
        }
        Ok(())
    }

    /// Encodes this report into its canonical 929-byte big-endian payload.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(WORKLOAD_REPORT_PAYLOAD_LEN);
        out.extend_from_slice(&(WORKLOAD_COUNT as u32).to_be_bytes());
        for entry in &self.entries {
            out.extend_from_slice(&entry.encode());
        }
        out
    }

    /// Decodes and validates a 929-byte canonical `ScaleWorkloadReportV1` payload.
    pub fn decode(bytes: &[u8]) -> Result<Self, ScaleAssayError> {
        if bytes.len() != WORKLOAD_REPORT_PAYLOAD_LEN {
            return Err(ScaleAssayError::InvalidPayloadLength {
                actual: bytes.len(),
            });
        }
        let count = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if count != WORKLOAD_COUNT as u32 {
            return Err(ScaleAssayError::InvalidWorkloadCount { count });
        }
        let e0 = ScaleWorkloadEntryV1::decode(&bytes[4..189], 0)?;
        let e1 = ScaleWorkloadEntryV1::decode(&bytes[189..374], 1)?;
        let e2 = ScaleWorkloadEntryV1::decode(&bytes[374..559], 2)?;
        let e3 = ScaleWorkloadEntryV1::decode(&bytes[559..744], 3)?;
        let e4 = ScaleWorkloadEntryV1::decode(&bytes[744..929], 4)?;
        Ok(Self {
            entries: [e0, e1, e2, e3, e4],
        })
    }

    /// Computes the canonical `Receipt` `ContentTag` for this workload assay report.
    pub fn tag(&self) -> ContentTag {
        let bytes = self.encode();
        debug_assert_eq!(bytes.len(), WORKLOAD_REPORT_PAYLOAD_LEN);
        hash_canonical(
            TagKind::Receipt,
            SCALE_WORKLOAD_REPORT_V1_SCHEMA,
            WORKLOAD_REPORT_PAYLOAD_LEN as u64,
            |sink| {
                sink(&bytes);
            },
        )
        .expect("ScaleWorkloadReportV1 tag encoding is canonical")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_workload_report_matches_verified_metrics_and_pinned_tag() {
        assert_eq!(SCALE_WORKLOAD_REPORT_V1_SCHEMA.len(), 39);
        assert_eq!(ALL_SCALE_WORKLOADS.len(), 5);

        let report = ScaleWorkloadReportV1::canonical();
        assert_eq!(report.validate(), Ok(()));

        // 0. SingleOrigin
        let e0 = &report.entries[0];
        assert_eq!(e0, &assay_workload(ScaleWorkloadId::SingleOrigin));
        assert_eq!(e0.workload_id_u8, 0);
        assert_eq!(e0.allocated_bits_u64, 1);
        assert_eq!(e0.allocated_chunks_u64, 1);
        assert_eq!(e0.active_root_octants_u64, 1);
        assert_eq!(e0.total_cube_faces_u64, 6);
        assert_eq!(e0.exposed_surface_faces_u64, 6);
        assert_eq!(e0.buried_interior_faces_u64, 0);
        assert_eq!(e0.fully_buried_voxels_u64, 0);
        assert_eq!(e0.frustum_visible_chunks_u64, 1);
        assert_eq!(e0.frustum_culled_chunks_u64, 0);
        assert_eq!(e0.raycast_tested_chunks_u64, 1);
        assert_eq!(e0.raycast_pruned_chunks_u64, 0);
        let oct0 = SparseOctreeV1::from_world(&build_workload_world(ScaleWorkloadId::SingleOrigin));
        assert_eq!(
            oct0.root_tag().to_string(),
            "#bb.world.sha256.lmjzqddt5hcdszpiwkaf3ckxdtk25mcdm4go4zav6i3r4gn6kksa"
        );
        assert_eq!(e0.octree_root_digest, oct0.root_tag().digest());

        // 1. DenseCube16
        let e1 = &report.entries[1];
        assert_eq!(e1.workload_id_u8, 1);
        assert_eq!(e1.allocated_bits_u64, 4_096);
        assert_eq!(e1.allocated_chunks_u64, 1);
        assert_eq!(e1.active_root_octants_u64, 1);
        assert_eq!(e1.total_cube_faces_u64, 24_576);
        assert_eq!(e1.exposed_surface_faces_u64, 1_536);
        assert_eq!(e1.buried_interior_faces_u64, 23_040);
        assert_eq!(e1.fully_buried_voxels_u64, 2_744);
        assert_eq!(e1.frustum_visible_chunks_u64, 1);
        assert_eq!(e1.frustum_culled_chunks_u64, 0);
        assert_eq!(e1.raycast_tested_chunks_u64, 1);
        assert_eq!(e1.raycast_pruned_chunks_u64, 0);

        // 2. HollowShell16
        let e2 = &report.entries[2];
        assert_eq!(e2.workload_id_u8, 2);
        assert_eq!(e2.allocated_bits_u64, 1_352);
        assert_eq!(e2.allocated_chunks_u64, 1);
        assert_eq!(e2.active_root_octants_u64, 1);
        assert_eq!(e2.total_cube_faces_u64, 8_112);
        assert_eq!(e2.exposed_surface_faces_u64, 2_712);
        assert_eq!(e2.buried_interior_faces_u64, 5_400);
        assert_eq!(e2.fully_buried_voxels_u64, 0);
        assert_eq!(e2.frustum_visible_chunks_u64, 1);
        assert_eq!(e2.frustum_culled_chunks_u64, 0);
        assert_eq!(e2.raycast_tested_chunks_u64, 1);
        assert_eq!(e2.raycast_pruned_chunks_u64, 0);

        // 3. NegativeChunkCluster
        let e3 = &report.entries[3];
        assert_eq!(e3.workload_id_u8, 3);
        assert_eq!(e3.allocated_bits_u64, 9);
        assert_eq!(e3.allocated_chunks_u64, 8);
        assert_eq!(e3.active_root_octants_u64, 8);
        assert_eq!(e3.total_cube_faces_u64, 54);
        assert_eq!(e3.exposed_surface_faces_u64, 30);
        assert_eq!(e3.buried_interior_faces_u64, 24);
        assert_eq!(e3.fully_buried_voxels_u64, 0);
        assert_eq!(e3.frustum_visible_chunks_u64, 8);
        assert_eq!(e3.frustum_culled_chunks_u64, 0);
        assert_eq!(e3.raycast_tested_chunks_u64, 2);
        assert_eq!(e3.raycast_pruned_chunks_u64, 6);
        let oct3 = SparseOctreeV1::from_world(&build_workload_world(
            ScaleWorkloadId::NegativeChunkCluster,
        ));
        assert_eq!(
            oct3.root_tag().to_string(),
            "#bb.world.sha256.74xdmb3l7eofhba23gxrrrazc42ozvckup4b27fg6gvtdgtwl6qa"
        );
        assert_eq!(e3.octree_root_digest, oct3.root_tag().digest());

        // 4. SparseFarExtrema
        let e4 = &report.entries[4];
        assert_eq!(e4.workload_id_u8, 4);
        assert_eq!(e4.allocated_bits_u64, 9);
        assert_eq!(e4.allocated_chunks_u64, 9);
        assert_eq!(e4.active_root_octants_u64, 9);
        assert_eq!(e4.total_cube_faces_u64, 54);
        assert_eq!(e4.exposed_surface_faces_u64, 54);
        assert_eq!(e4.buried_interior_faces_u64, 0);
        assert_eq!(e4.fully_buried_voxels_u64, 0);
        assert_eq!(e4.frustum_visible_chunks_u64, 1);
        assert_eq!(e4.frustum_culled_chunks_u64, 8);
        assert_eq!(e4.raycast_tested_chunks_u64, 1);
        assert_eq!(e4.raycast_pruned_chunks_u64, 8);
        let oct4 =
            SparseOctreeV1::from_world(&build_workload_world(ScaleWorkloadId::SparseFarExtrema));
        assert_eq!(
            oct4.root_tag().to_string(),
            "#bb.world.sha256.j2yx5roy6ct2pmqnvnzrx72gsm7rmi3o7zol2p6sokh76lahiyaa"
        );
        assert_eq!(e4.octree_root_digest, oct4.root_tag().digest());

        let encoded = report.encode();
        assert_eq!(encoded.len(), 929);
        assert_eq!(ScaleWorkloadReportV1::decode(&encoded), Ok(report.clone()));
        assert_eq!(
            ScaleWorkloadEntryV1::decode(&e3.encode(), 3),
            Ok(e3.clone())
        );

        assert_eq!(
            report.tag().to_string(),
            "#bb.receipt.sha256.xq6oxiyyao3khyp3i7r6zbz3zwewouc75btw4s733xp62zeic3ma"
        );
    }

    #[test]
    fn scale_workload_decoder_and_validator_reject_malformed_records() {
        let canonical = ScaleWorkloadReportV1::canonical();
        let valid_bytes = canonical.encode();

        assert_eq!(
            ScaleWorkloadReportV1::decode(&valid_bytes[..928]),
            Err(ScaleAssayError::InvalidPayloadLength { actual: 928 })
        );
        assert_eq!(
            ScaleWorkloadEntryV1::decode(&valid_bytes[4..188], 0),
            Err(ScaleAssayError::InvalidPayloadLength { actual: 184 })
        );

        let mut bad_count = valid_bytes.clone();
        bad_count[3] = 4;
        assert_eq!(
            ScaleWorkloadReportV1::decode(&bad_count),
            Err(ScaleAssayError::InvalidWorkloadCount { count: 4 })
        );

        let mut bad_id = canonical.clone();
        bad_id.entries[2].workload_id_u8 = 7;
        assert_eq!(
            ScaleWorkloadReportV1::decode(&bad_id.encode()),
            Err(ScaleAssayError::InvalidWorkloadId {
                index: 2,
                actual: 7,
            })
        );

        let mut bad_occupancy = canonical.clone();
        bad_occupancy.entries[0].allocated_bits_u64 = 0;
        assert_eq!(
            ScaleWorkloadReportV1::decode(&bad_occupancy.encode()),
            Err(ScaleAssayError::InvalidOccupancyBounds {
                index: 0,
                allocated_bits_u64: 0,
                allocated_chunks_u64: 1,
                active_root_octants_u64: 1,
                fully_buried_voxels_u64: 0,
            })
        );

        let mut bad_face_total = canonical.clone();
        bad_face_total.entries[1].total_cube_faces_u64 = 24_575;
        assert_eq!(
            ScaleWorkloadReportV1::decode(&bad_face_total.encode()),
            Err(ScaleAssayError::FaceTotalMismatch {
                index: 1,
                allocated_bits_u64: 4_096,
                total_cube_faces_u64: 24_575,
            })
        );

        let mut bad_surface_sum = canonical.clone();
        bad_surface_sum.entries[2].buried_interior_faces_u64 = 5_399;
        assert_eq!(
            ScaleWorkloadReportV1::decode(&bad_surface_sum.encode()),
            Err(ScaleAssayError::SurfaceBuriedSumMismatch {
                index: 2,
                exposed_surface_faces_u64: 2_712,
                buried_interior_faces_u64: 5_399,
                total_cube_faces_u64: 8_112,
            })
        );

        let mut bad_frustum_sum = canonical.clone();
        bad_frustum_sum.entries[4].frustum_culled_chunks_u64 = 7;
        assert_eq!(
            ScaleWorkloadReportV1::decode(&bad_frustum_sum.encode()),
            Err(ScaleAssayError::FrustumChunkSumMismatch {
                index: 4,
                frustum_visible_chunks_u64: 1,
                frustum_culled_chunks_u64: 7,
                allocated_chunks_u64: 9,
            })
        );

        let mut bad_ray_sum = canonical.clone();
        bad_ray_sum.entries[3].raycast_pruned_chunks_u64 = 5;
        assert_eq!(
            ScaleWorkloadReportV1::decode(&bad_ray_sum.encode()),
            Err(ScaleAssayError::RaycastChunkSumMismatch {
                index: 3,
                raycast_tested_chunks_u64: 2,
                raycast_pruned_chunks_u64: 5,
                allocated_chunks_u64: 8,
            })
        );

        let mut bad_digest = canonical;
        bad_digest.entries[0].patricia_root_digest = [0u8; 32];
        let err = ScaleWorkloadReportV1::decode(&bad_digest.encode()).unwrap_err();
        assert_eq!(err, ScaleAssayError::ZeroCommitmentDigest { index: 0 });
        assert!(err.to_string().contains("all-zero commitment digest"));
        let std_err: &dyn std::error::Error = &err;
        assert!(std_err.source().is_none());
    }
}
