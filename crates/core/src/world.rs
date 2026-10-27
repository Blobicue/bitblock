use std::collections::HashMap;

/// Number of coordinates along each axis of one sparse chunk.
pub const CHUNK_SIDE: i64 = 16;
/// Number of `u64` words traversed by one allocated chunk iteration.
pub const CHUNK_WORDS: usize = 64;
/// Number of sparse octree pyramid levels maintained above leaf chunks (`1..=8`).
pub const OCTREE_MAX_DEPTH: u8 = 8;

/// Exposed-face bit for the `-Z` `(0, 0, -1)` neighbor direction, matching `render::FACES[0]`.
pub const FACE_NEG_Z: u8 = 1 << 0;
/// Exposed-face bit for the `+Z` `(0, 0, +1)` neighbor direction, matching `render::FACES[1]`.
pub const FACE_POS_Z: u8 = 1 << 1;
/// Exposed-face bit for the `-X` `(-1, 0, 0)` neighbor direction, matching `render::FACES[2]`.
pub const FACE_NEG_X: u8 = 1 << 2;
/// Exposed-face bit for the `+X` `(+1, 0, 0)` neighbor direction, matching `render::FACES[3]`.
pub const FACE_POS_X: u8 = 1 << 3;
/// Exposed-face bit for the `-Y` `(0, -1, 0)` neighbor direction, matching `render::FACES[4]`.
pub const FACE_NEG_Y: u8 = 1 << 4;
/// Exposed-face bit for the `+Y` `(0, +1, 0)` neighbor direction, matching `render::FACES[5]`.
pub const FACE_POS_Y: u8 = 1 << 5;
/// Bitmask with all six orthogonal cube faces exposed.
pub const ALL_FACES_EXPOSED: u8 = 0b0011_1111;

/// The integer coordinate of one bit in the world.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Coord {
    pub x: i64,
    pub y: i64,
    pub z: i64,
}

impl Coord {
    pub const fn new(x: i64, y: i64, z: i64) -> Self {
        Self { x, y, z }
    }
}

/// The permanent point of reference for every world.
pub const BEDROCK_COORD: Coord = Coord::new(0, 0, 0);

/// Returns whether a coordinate is the permanent bedrock coordinate.
pub const fn is_bedrock(coord: Coord) -> bool {
    coord.x == BEDROCK_COORD.x && coord.y == BEDROCK_COORD.y && coord.z == BEDROCK_COORD.z
}

impl From<(i64, i64, i64)> for Coord {
    fn from((x, y, z): (i64, i64, i64)) -> Self {
        Self::new(x, y, z)
    }
}

/// The coordinate of one lazily allocated 16 x 16 x 16 chunk.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ChunkCoord {
    pub x: i64,
    pub y: i64,
    pub z: i64,
}

impl ChunkCoord {
    const fn new(x: i64, y: i64, z: i64) -> Self {
        Self { x, y, z }
    }
}

/// Inclusive bounds of all bits that are on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Bounds {
    pub min: Coord,
    pub max: Coord,
}

/// The coordinate of one sparse octant at pyramid depth `1..=OCTREE_MAX_DEPTH`.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OctantCoord {
    pub depth: u8,
    pub x: i64,
    pub y: i64,
    pub z: i64,
}

/// Occupancy, child occupancy bitmask, tight bounds, and mutation revision for one octant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OctantSummary {
    pub occupied_bits: usize,
    pub child_mask: u8,
    pub bounds: Bounds,
    pub revision: u64,
}

#[derive(Clone, Debug)]
struct Chunk {
    bits: [u64; CHUNK_WORDS],
    count: u16,
    bounds: Bounds,
}

impl Chunk {
    fn empty() -> Self {
        Self {
            bits: [0; CHUNK_WORDS],
            count: 0,
            bounds: Bounds {
                min: BEDROCK_COORD,
                max: BEDROCK_COORD,
            },
        }
    }

    fn is_on(&self, index: usize) -> bool {
        let (word, mask) = word_and_mask(index);
        self.bits[word] & mask != 0
    }

    fn set_on(&mut self, index: usize, coord: Coord) -> bool {
        let (word, mask) = word_and_mask(index);
        if self.bits[word] & mask != 0 {
            return false;
        }

        let was_empty = self.count == 0;
        self.bits[word] |= mask;
        self.count += 1;
        if was_empty {
            self.bounds = Bounds {
                min: coord,
                max: coord,
            };
        } else {
            self.bounds = merge_bounds(
                self.bounds,
                Bounds {
                    min: coord,
                    max: coord,
                },
            );
        }
        true
    }

    fn set_off(&mut self, chunk_coord: ChunkCoord, index: usize, coord: Coord) -> bool {
        let (word, mask) = word_and_mask(index);
        if self.bits[word] & mask == 0 {
            return false;
        }

        self.bits[word] &= !mask;
        self.count -= 1;
        if self.count > 0 && bounds_touch(self.bounds, coord) {
            let mut coords = World::iter_chunk_coords(chunk_coord, &self.bits);
            if let Some(first) = coords.next() {
                let mut bounds = Bounds {
                    min: first,
                    max: first,
                };
                for remaining in coords {
                    bounds = merge_bounds(
                        bounds,
                        Bounds {
                            min: remaining,
                            max: remaining,
                        },
                    );
                }
                self.bounds = bounds;
            }
        }
        true
    }
}

/// A sparse bit world. Coordinates not present in an allocated chunk are off.
#[derive(Clone, Debug)]
pub struct World {
    chunks: HashMap<ChunkCoord, Chunk>,
    octree: [HashMap<OctantCoord, OctantSummary>; OCTREE_MAX_DEPTH as usize],
    revision_counter: u64,
    len: usize,
    bounds: Option<Bounds>,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    /// Creates a world containing exactly the permanent bedrock bit.
    pub fn new() -> Self {
        let mut world = Self {
            chunks: HashMap::new(),
            octree: std::array::from_fn(|_| HashMap::new()),
            revision_counter: 0,
            len: 0,
            bounds: None,
        };
        let inserted = world.set_on(BEDROCK_COORD);
        debug_assert!(inserted);
        world
    }

    /// Compatibility constructor for code that previously requested an empty world.
    ///
    /// A valid world is never completely empty: it always contains bedrock.
    pub fn empty() -> Self {
        Self::new()
    }

    /// Compatibility constructor for the original checkpoint-one world.
    pub fn new_with_origin() -> Self {
        Self::new()
    }

    /// Returns whether a coordinate is on. Missing chunks are entirely off.
    pub fn is_on(&self, coord: Coord) -> bool {
        let (chunk_coord, local_index) = split_coord(coord);
        self.chunks
            .get(&chunk_coord)
            .is_some_and(|chunk| chunk.is_on(local_index))
    }

    /// Returns the 6-bit exposed-face mask for `coord` (`0` if `coord` is off).
    pub fn exposed_faces(&self, coord: Coord) -> u8 {
        let (chunk_coord, local_index) = split_coord(coord);
        let Some(chunk) = self.chunks.get(&chunk_coord) else {
            return 0;
        };
        if !chunk.is_on(local_index) {
            return 0;
        }

        let side = CHUNK_SIDE as usize;
        let lx = coord.x.rem_euclid(CHUNK_SIDE) as usize;
        let ly = coord.y.rem_euclid(CHUNK_SIDE) as usize;
        let lz = coord.z.rem_euclid(CHUNK_SIDE) as usize;
        let mut mask = 0_u8;

        let neg_z_on = match coord.z.checked_sub(1) {
            Some(nz) => {
                if lz > 0 {
                    chunk.is_on(local_index - side * side)
                } else {
                    self.is_on(Coord::new(coord.x, coord.y, nz))
                }
            }
            None => false,
        };
        if !neg_z_on {
            mask |= FACE_NEG_Z;
        }

        let pos_z_on = match coord.z.checked_add(1) {
            Some(pz) => {
                if lz + 1 < side {
                    chunk.is_on(local_index + side * side)
                } else {
                    self.is_on(Coord::new(coord.x, coord.y, pz))
                }
            }
            None => false,
        };
        if !pos_z_on {
            mask |= FACE_POS_Z;
        }

        let neg_x_on = match coord.x.checked_sub(1) {
            Some(nx) => {
                if lx > 0 {
                    chunk.is_on(local_index - 1)
                } else {
                    self.is_on(Coord::new(nx, coord.y, coord.z))
                }
            }
            None => false,
        };
        if !neg_x_on {
            mask |= FACE_NEG_X;
        }

        let pos_x_on = match coord.x.checked_add(1) {
            Some(px) => {
                if lx + 1 < side {
                    chunk.is_on(local_index + 1)
                } else {
                    self.is_on(Coord::new(px, coord.y, coord.z))
                }
            }
            None => false,
        };
        if !pos_x_on {
            mask |= FACE_POS_X;
        }

        let neg_y_on = match coord.y.checked_sub(1) {
            Some(ny) => {
                if ly > 0 {
                    chunk.is_on(local_index - side)
                } else {
                    self.is_on(Coord::new(coord.x, ny, coord.z))
                }
            }
            None => false,
        };
        if !neg_y_on {
            mask |= FACE_NEG_Y;
        }

        let pos_y_on = match coord.y.checked_add(1) {
            Some(py) => {
                if ly + 1 < side {
                    chunk.is_on(local_index + side)
                } else {
                    self.is_on(Coord::new(coord.x, py, coord.z))
                }
            }
            None => false,
        };
        if !pos_y_on {
            mask |= FACE_POS_Y;
        }

        mask
    }

    /// Turns a coordinate on. Returns true only when the world changed.
    pub fn set_on(&mut self, coord: Coord) -> bool {
        let (chunk_coord, local_index) = split_coord(coord);
        let changed = self
            .chunks
            .entry(chunk_coord)
            .or_insert_with(Chunk::empty)
            .set_on(local_index, coord);

        if changed {
            self.revision_counter += 1;
            self.len += 1;
            self.extend_bounds(coord);
            self.update_octree_ancestors(chunk_coord);
        }
        changed
    }

    /// Turns a coordinate off. Returns true only when the world changed.
    pub fn set_off(&mut self, coord: Coord) -> bool {
        if is_bedrock(coord) {
            return false;
        }

        let (chunk_coord, local_index) = split_coord(coord);

        let (changed, chunk_is_empty) = match self.chunks.get_mut(&chunk_coord) {
            Some(chunk) => {
                let changed = chunk.set_off(chunk_coord, local_index, coord);
                (changed, chunk.count == 0)
            }
            None => (false, false),
        };

        let touched_bounds = changed && self.coord_touches_bounds(coord);
        if changed {
            self.revision_counter += 1;
            self.len -= 1;
        }
        if chunk_is_empty {
            self.chunks.remove(&chunk_coord);
        }
        if changed {
            self.update_octree_ancestors(chunk_coord);
            self.bounds = if self.len == 0 {
                None
            } else if touched_bounds {
                self.recompute_bounds()
            } else {
                self.bounds
            };
        }

        changed
    }

    /// Flips one coordinate and returns its new state.
    pub fn toggle(&mut self, coord: Coord) -> bool {
        if is_bedrock(coord) {
            return true;
        }

        if self.is_on(coord) {
            self.set_off(coord);
            false
        } else {
            self.set_on(coord);
            true
        }
    }

    /// Number of coordinates that are on.
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Number of lazily allocated 16 x 16 x 16 chunks currently carrying on bits.
    pub fn allocated_chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// Returns whether no coordinates are on.
    ///
    /// This is always false for worlds created through the public constructors because
    /// bedrock is permanently on.
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Exact inclusive bounds of the on coordinates.
    pub fn bounds(&self) -> Option<Bounds> {
        self.bounds
    }

    /// Returns the occupied bit count and raw 64-word bitset for an allocated chunk.
    pub fn chunk_words(&self, chunk: ChunkCoord) -> Option<(u16, &[u64; CHUNK_WORDS])> {
        self.chunks
            .get(&chunk)
            .map(|allocated| (allocated.count, &allocated.bits))
    }

    /// Returns the tight inclusive coordinate bounds of all on bits in an allocated chunk.
    pub fn chunk_bounds(&self, chunk: ChunkCoord) -> Option<Bounds> {
        self.chunks.get(&chunk).map(|allocated| allocated.bounds)
    }

    /// Iterates over every allocated chunk with its count, tight bounds, and 64-word bitset.
    pub fn iter_chunks(
        &self,
    ) -> impl Iterator<Item = (ChunkCoord, u16, Bounds, &[u64; CHUNK_WORDS])> + '_ {
        self.chunks
            .iter()
            .map(|(chunk_coord, chunk)| (*chunk_coord, chunk.count, chunk.bounds, &chunk.bits))
    }

    /// Iterates over every on coordinate encoded in a chunk's 64-word bitset.
    pub fn iter_chunk_coords(
        chunk: ChunkCoord,
        words: &[u64; CHUNK_WORDS],
    ) -> impl Iterator<Item = Coord> + '_ {
        words
            .iter()
            .copied()
            .enumerate()
            .flat_map(move |(word_index, mut bits)| {
                std::iter::from_fn(move || {
                    if bits == 0 {
                        return None;
                    }

                    let bit_index = bits.trailing_zeros() as usize;
                    bits &= bits - 1;
                    let index = word_index * u64::BITS as usize + bit_index;
                    Some(join_coord(chunk, index))
                })
            })
    }

    /// Returns the summary for an active octant at depth `1..=OCTREE_MAX_DEPTH`.
    pub fn octant_summary(&self, coord: OctantCoord) -> Option<OctantSummary> {
        if coord.depth == 0 || coord.depth > OCTREE_MAX_DEPTH {
            return None;
        }
        self.octree[(coord.depth - 1) as usize].get(&coord).copied()
    }

    /// Iterates over all active macro-octant roots at `OCTREE_MAX_DEPTH`.
    pub fn root_octants(&self) -> impl Iterator<Item = (OctantCoord, OctantSummary)> + '_ {
        self.octree[(OCTREE_MAX_DEPTH - 1) as usize]
            .iter()
            .map(|(coord, summary)| (*coord, *summary))
    }

    fn update_octree_ancestors(&mut self, chunk_coord: ChunkCoord) {
        for depth in 1..=OCTREE_MAX_DEPTH {
            let divisor = 1_i64 << depth;
            let px = chunk_coord.x.div_euclid(divisor);
            let py = chunk_coord.y.div_euclid(divisor);
            let pz = chunk_coord.z.div_euclid(divisor);
            let parent = OctantCoord {
                depth,
                x: px,
                y: py,
                z: pz,
            };

            let mut occupied_bits = 0_usize;
            let mut child_mask = 0_u8;
            let mut combined_bounds: Option<Bounds> = None;

            for child_idx in 0..8_u8 {
                let cx = px * 2 + i64::from((child_idx >> 2) & 1);
                let cy = py * 2 + i64::from((child_idx >> 1) & 1);
                let cz = pz * 2 + i64::from(child_idx & 1);

                let child_info = if depth == 1 {
                    self.chunks
                        .get(&ChunkCoord::new(cx, cy, cz))
                        .map(|chunk| (usize::from(chunk.count), chunk.bounds))
                } else {
                    let child_coord = OctantCoord {
                        depth: depth - 1,
                        x: cx,
                        y: cy,
                        z: cz,
                    };
                    self.octree[(depth - 2) as usize]
                        .get(&child_coord)
                        .map(|summary| (summary.occupied_bits, summary.bounds))
                };

                if let Some((child_bits, child_bounds)) = child_info {
                    occupied_bits += child_bits;
                    child_mask |= 1_u8 << child_idx;
                    combined_bounds = Some(match combined_bounds {
                        Some(existing) => merge_bounds(existing, child_bounds),
                        None => child_bounds,
                    });
                }
            }

            if let Some(bounds) = combined_bounds {
                self.octree[(depth - 1) as usize].insert(
                    parent,
                    OctantSummary {
                        occupied_bits,
                        child_mask,
                        bounds,
                        revision: self.revision_counter,
                    },
                );
            } else {
                self.octree[(depth - 1) as usize].remove(&parent);
            }
        }
    }

    fn extend_bounds(&mut self, coord: Coord) {
        match &mut self.bounds {
            Some(bounds) => {
                *bounds = merge_bounds(
                    *bounds,
                    Bounds {
                        min: coord,
                        max: coord,
                    },
                );
            }
            None => {
                self.bounds = Some(Bounds {
                    min: coord,
                    max: coord,
                });
            }
        }
    }

    fn coord_touches_bounds(&self, coord: Coord) -> bool {
        self.bounds
            .is_some_and(|bounds| bounds_touch(bounds, coord))
    }

    fn recompute_bounds(&self) -> Option<Bounds> {
        let mut roots = self.octree[(OCTREE_MAX_DEPTH - 1) as usize].values();
        let first = roots.next()?;
        let mut bounds = first.bounds;

        for summary in roots {
            bounds = merge_bounds(bounds, summary.bounds);
        }

        Some(bounds)
    }

    /// Iterates over every coordinate that is on. Iteration order is unspecified.
    pub fn iter_on(&self) -> impl Iterator<Item = Coord> + '_ {
        self.chunks
            .iter()
            .flat_map(|(chunk_coord, chunk)| Self::iter_chunk_coords(*chunk_coord, &chunk.bits))
    }
}

fn merge_bounds(a: Bounds, b: Bounds) -> Bounds {
    Bounds {
        min: Coord::new(
            a.min.x.min(b.min.x),
            a.min.y.min(b.min.y),
            a.min.z.min(b.min.z),
        ),
        max: Coord::new(
            a.max.x.max(b.max.x),
            a.max.y.max(b.max.y),
            a.max.z.max(b.max.z),
        ),
    }
}

fn bounds_touch(bounds: Bounds, coord: Coord) -> bool {
    coord.x == bounds.min.x
        || coord.y == bounds.min.y
        || coord.z == bounds.min.z
        || coord.x == bounds.max.x
        || coord.y == bounds.max.y
        || coord.z == bounds.max.z
}

fn word_and_mask(index: usize) -> (usize, u64) {
    debug_assert!(index < CHUNK_WORDS * u64::BITS as usize);
    (
        index / u64::BITS as usize,
        1_u64 << (index % u64::BITS as usize),
    )
}

fn split_coord(coord: Coord) -> (ChunkCoord, usize) {
    let chunk = ChunkCoord::new(
        coord.x.div_euclid(CHUNK_SIDE),
        coord.y.div_euclid(CHUNK_SIDE),
        coord.z.div_euclid(CHUNK_SIDE),
    );
    let local_x = coord.x.rem_euclid(CHUNK_SIDE) as usize;
    let local_y = coord.y.rem_euclid(CHUNK_SIDE) as usize;
    let local_z = coord.z.rem_euclid(CHUNK_SIDE) as usize;
    let index = local_x + CHUNK_SIDE as usize * (local_y + CHUNK_SIDE as usize * local_z);
    (chunk, index)
}

fn join_coord(chunk: ChunkCoord, index: usize) -> Coord {
    let side = CHUNK_SIDE as usize;
    let local_x = index % side;
    let local_y = (index / side) % side;
    let local_z = index / (side * side);

    Coord::new(
        chunk.x * CHUNK_SIDE + local_x as i64,
        chunk.y * CHUNK_SIDE + local_y as i64,
        chunk.z * CHUNK_SIDE + local_z as i64,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn assert_internal_invariants(world: &World) {
        assert!(world.is_on(BEDROCK_COORD));
        assert!(!world.is_empty());
        assert!(world.revision_counter >= 1);

        let coords = world.iter_on().collect::<HashSet<_>>();
        assert_eq!(coords.len(), world.len());
        assert_eq!(world.bounds(), world.recompute_bounds());

        let mut iter_coords = coords.iter().copied();
        let first_global = iter_coords.next().expect("world must contain bedrock");
        let mut expected_global_bounds = Bounds {
            min: first_global,
            max: first_global,
        };
        for coord in iter_coords {
            expected_global_bounds = merge_bounds(
                expected_global_bounds,
                Bounds {
                    min: coord,
                    max: coord,
                },
            );
        }
        assert_eq!(world.bounds(), Some(expected_global_bounds));

        let mut counted_bits = 0_usize;
        for (chunk_coord, chunk) in &world.chunks {
            assert_ne!(chunk.count, 0, "allocated chunks must not be empty");
            let bits = chunk
                .bits
                .iter()
                .map(|word| word.count_ones() as usize)
                .sum::<usize>();
            assert_eq!(usize::from(chunk.count), bits);

            let chunk_coords =
                World::iter_chunk_coords(*chunk_coord, &chunk.bits).collect::<Vec<_>>();
            assert_eq!(chunk_coords.len(), bits);
            let mut chunk_iter = chunk_coords.iter().copied();
            let first_chunk_coord = chunk_iter.next().expect("non-empty chunk");
            let mut expected_chunk_bounds = Bounds {
                min: first_chunk_coord,
                max: first_chunk_coord,
            };
            for coord in chunk_iter {
                expected_chunk_bounds = merge_bounds(
                    expected_chunk_bounds,
                    Bounds {
                        min: coord,
                        max: coord,
                    },
                );
            }
            assert_eq!(chunk.bounds, expected_chunk_bounds);
            assert_eq!(world.chunk_bounds(*chunk_coord), Some(chunk.bounds));
            assert_eq!(
                world.chunk_words(*chunk_coord),
                Some((chunk.count, &chunk.bits))
            );
            counted_bits += bits;
        }
        assert_eq!(world.len(), counted_bits);

        for depth in 1..=OCTREE_MAX_DEPTH {
            let level = &world.octree[(depth - 1) as usize];
            let mut level_bits = 0_usize;
            for (octant_coord, summary) in level {
                assert_eq!(octant_coord.depth, depth);
                assert!(summary.occupied_bits > 0);
                assert_ne!(summary.child_mask, 0);
                assert!(1 <= summary.revision && summary.revision <= world.revision_counter);
                assert_eq!(world.octant_summary(*octant_coord), Some(*summary));
                level_bits += summary.occupied_bits;
            }
            assert_eq!(level_bits, world.len());
        }
    }

    fn assert_bedrock_only(world: &World) {
        assert_internal_invariants(world);
        assert_eq!(world.len(), 1);
        assert_eq!(world.chunks.len(), 1);
        assert_eq!(world.iter_on().collect::<Vec<_>>(), vec![BEDROCK_COORD]);
        assert_eq!(
            world.bounds(),
            Some(Bounds {
                min: BEDROCK_COORD,
                max: BEDROCK_COORD,
            })
        );
        for depth in 1..=OCTREE_MAX_DEPTH {
            let octant = OctantCoord {
                depth,
                x: 0,
                y: 0,
                z: 0,
            };
            let summary = world
                .octant_summary(octant)
                .expect("bedrock ancestor must be present");
            assert_eq!(summary.occupied_bits, 1);
            assert_eq!(summary.child_mask, 1);
            assert_eq!(
                summary.bounds,
                Bounds {
                    min: BEDROCK_COORD,
                    max: BEDROCK_COORD,
                }
            );
            assert!(1 <= summary.revision && summary.revision <= world.revision_counter);
        }
        let roots = world.root_octants().collect::<Vec<_>>();
        assert_eq!(roots.len(), 1);
        assert_eq!(
            roots[0].0,
            OctantCoord {
                depth: OCTREE_MAX_DEPTH,
                x: 0,
                y: 0,
                z: 0,
            }
        );
    }

    #[test]
    fn bedrock_predicate_identifies_only_the_origin() {
        assert!(is_bedrock(BEDROCK_COORD));
        for coord in [
            Coord::new(-1, 0, 0),
            Coord::new(1, 0, 0),
            Coord::new(0, -1, 0),
            Coord::new(0, 1, 0),
            Coord::new(0, 0, -1),
            Coord::new(0, 0, 1),
            Coord::new(i64::MIN, i64::MIN, i64::MIN),
            Coord::new(i64::MAX, i64::MAX, i64::MAX),
        ] {
            assert!(!is_bedrock(coord), "{coord:?} is not bedrock");
        }
    }

    #[test]
    fn every_public_constructor_contains_exactly_bedrock() {
        for world in [
            World::new(),
            World::default(),
            World::empty(),
            World::new_with_origin(),
        ] {
            assert_bedrock_only(&world);
        }
    }

    #[test]
    fn bedrock_cannot_be_removed_or_toggled_off() {
        let mut world = World::new();

        assert!(!world.set_on(BEDROCK_COORD));
        assert!(!world.set_off(BEDROCK_COORD));
        assert!(world.toggle(BEDROCK_COORD));

        assert_bedrock_only(&world);
    }

    #[test]
    fn setting_bits_is_idempotent_and_non_bedrock_chunks_are_removed() {
        let same_chunk = Coord::new(4, 5, 6);
        let other_chunk = Coord::new(32, 5, 6);
        let mut world = World::new();

        assert!(world.set_on(same_chunk));
        assert!(!world.set_on(same_chunk));
        assert_eq!(world.len(), 2);
        assert_eq!(world.chunks.len(), 1);
        assert!(world.set_off(same_chunk));
        assert_eq!(world.len(), 1);
        assert_eq!(world.chunks.len(), 1);

        assert!(world.set_on(other_chunk));
        assert!(!world.set_on(other_chunk));
        assert_eq!(world.len(), 2);
        assert_eq!(world.chunks.len(), 2);
        assert!(world.set_off(other_chunk));
        assert!(!world.set_off(other_chunk));
        assert_bedrock_only(&world);
    }

    #[test]
    fn negative_coordinates_use_euclidean_chunks() {
        let cases = [
            (Coord::new(-1, -1, -1), ChunkCoord::new(-1, -1, -1), 4095),
            (Coord::new(-16, -16, -16), ChunkCoord::new(-1, -1, -1), 0),
            (Coord::new(-17, 16, 0), ChunkCoord::new(-2, 1, 0), 15),
        ];

        for (coord, expected_chunk, expected_index) in cases {
            let (chunk, index) = split_coord(coord);
            assert_eq!(chunk, expected_chunk);
            assert_eq!(index, expected_index);
            assert_eq!(join_coord(chunk, index), coord);
        }
    }

    #[test]
    fn iteration_round_trips_across_chunks_and_coordinate_extremes() {
        let expected = HashSet::from([
            BEDROCK_COORD,
            Coord::new(i64::MIN, i64::MIN, i64::MIN),
            Coord::new(-17, 31, -33),
            Coord::new(-1, 0, 1),
            Coord::new(15, 16, 17),
            Coord::new(i64::MAX, i64::MAX, i64::MAX),
        ]);
        let mut world = World::new();

        for coord in expected.iter().copied() {
            assert_eq!(world.set_on(coord), !is_bedrock(coord));
        }

        assert_eq!(world.len(), expected.len());
        assert_eq!(world.iter_on().collect::<HashSet<_>>(), expected);
        assert_internal_invariants(&world);
    }

    #[test]
    fn deleting_an_extreme_recomputes_exact_bounds() {
        let mut world = World::new();
        let points = [
            Coord::new(-20, 5, 100),
            Coord::new(2, -30, 7),
            Coord::new(40, 50, -10),
            Coord::new(4, 8, 12),
        ];
        for point in points {
            world.set_on(point);
        }

        assert_eq!(
            world.bounds(),
            Some(Bounds {
                min: Coord::new(-20, -30, -10),
                max: Coord::new(40, 50, 100),
            })
        );

        world.set_off(Coord::new(-20, 5, 100));
        world.set_off(Coord::new(40, 50, -10));

        assert_eq!(
            world.bounds(),
            Some(Bounds {
                min: Coord::new(0, -30, 0),
                max: Coord::new(4, 8, 12),
            })
        );
        assert_internal_invariants(&world);
    }

    #[test]
    fn cached_bounds_survive_interior_deletion_and_refresh_on_boundary_deletion() {
        let mut world = World::new();
        for point in [
            Coord::new(-10, -10, -10),
            Coord::new(5, 5, 5),
            Coord::new(7, 2, 9),
            Coord::new(10, 10, 10),
        ] {
            world.set_on(point);
        }

        let original = Bounds {
            min: Coord::new(-10, -10, -10),
            max: Coord::new(10, 10, 10),
        };
        assert_eq!(world.bounds, Some(original));

        assert!(world.set_off(Coord::new(5, 5, 5)));
        assert_eq!(world.bounds, Some(original));

        assert!(world.set_off(Coord::new(-10, -10, -10)));
        assert_eq!(
            world.bounds,
            Some(Bounds {
                min: BEDROCK_COORD,
                max: Coord::new(10, 10, 10),
            })
        );
        assert_internal_invariants(&world);
    }

    #[test]
    fn toggle_returns_the_new_state() {
        let coord = Coord::new(-16, 16, 0);
        let mut world = World::new();

        assert!(world.toggle(coord));
        assert!(world.is_on(coord));
        assert!(!world.toggle(coord));
        assert!(!world.is_on(coord));
        assert_bedrock_only(&world);
    }

    #[test]
    fn public_mutations_preserve_bedrock_across_chunk_boundaries() {
        let mut world = World::new();
        let axes = [-17, -16, -15, -1, 0, 1, 15, 16, 17];

        for x in axes {
            for y in axes {
                for z in axes {
                    let coord = Coord::new(x, y, z);
                    if is_bedrock(coord) {
                        assert!(!world.set_on(coord));
                        assert!(!world.set_off(coord));
                        assert!(world.toggle(coord));
                    } else {
                        assert!(world.set_on(coord));
                        assert!(!world.set_on(coord));
                        assert!(world.is_on(coord));

                        assert!(!world.toggle(coord));
                        assert!(!world.is_on(coord));

                        assert!(world.toggle(coord));
                        assert!(world.is_on(coord));
                        assert!(world.set_off(coord));
                        assert!(!world.set_off(coord));
                    }

                    assert_bedrock_only(&world);
                }
            }
        }
    }

    #[test]
    fn exposed_faces_orders_bits_and_guards_i64_overflow() {
        assert_eq!(FACE_NEG_Z, 1 << 0);
        assert_eq!(FACE_POS_Z, 1 << 1);
        assert_eq!(FACE_NEG_X, 1 << 2);
        assert_eq!(FACE_POS_X, 1 << 3);
        assert_eq!(FACE_NEG_Y, 1 << 4);
        assert_eq!(FACE_POS_Y, 1 << 5);
        assert_eq!(
            ALL_FACES_EXPOSED,
            FACE_NEG_Z | FACE_POS_Z | FACE_NEG_X | FACE_POS_X | FACE_NEG_Y | FACE_POS_Y
        );

        let mut world = World::new();
        assert_eq!(world.exposed_faces(Coord::new(9, 9, 9)), 0);
        assert_eq!(world.exposed_faces(BEDROCK_COORD), ALL_FACES_EXPOSED);

        let directional_neighbors = [
            (Coord::new(0, 0, -1), FACE_NEG_Z),
            (Coord::new(0, 0, 1), FACE_POS_Z),
            (Coord::new(-1, 0, 0), FACE_NEG_X),
            (Coord::new(1, 0, 0), FACE_POS_X),
            (Coord::new(0, -1, 0), FACE_NEG_Y),
            (Coord::new(0, 1, 0), FACE_POS_Y),
        ];

        for (neighbor, face_bit) in directional_neighbors {
            assert!(world.set_on(neighbor));
            assert_eq!(
                world.exposed_faces(BEDROCK_COORD),
                ALL_FACES_EXPOSED & !face_bit
            );
            assert!(world.set_off(neighbor));
        }

        let interior = Coord::new(5, 6, 7);
        assert!(world.set_on(interior));
        assert_eq!(world.exposed_faces(interior), ALL_FACES_EXPOSED);
        let interior_neighbors = [
            (Coord::new(5, 6, 6), FACE_NEG_Z),
            (Coord::new(5, 6, 8), FACE_POS_Z),
            (Coord::new(4, 6, 7), FACE_NEG_X),
            (Coord::new(6, 6, 7), FACE_POS_X),
            (Coord::new(5, 5, 7), FACE_NEG_Y),
            (Coord::new(5, 7, 7), FACE_POS_Y),
        ];
        let mut expected_mask = ALL_FACES_EXPOSED;
        for (neighbor, face_bit) in interior_neighbors {
            assert!(world.set_on(neighbor));
            expected_mask &= !face_bit;
            assert_eq!(world.exposed_faces(interior), expected_mask);
        }
        assert_eq!(world.exposed_faces(interior), 0);

        let min_corner = Coord::new(i64::MIN, i64::MIN, i64::MIN);
        let max_corner = Coord::new(i64::MAX, i64::MAX, i64::MAX);
        assert!(world.set_on(min_corner));
        assert!(world.set_on(max_corner));
        assert_eq!(world.exposed_faces(min_corner), ALL_FACES_EXPOSED);
        assert_eq!(world.exposed_faces(max_corner), ALL_FACES_EXPOSED);

        assert!(world.set_on(Coord::new(i64::MIN + 1, i64::MIN, i64::MIN)));
        assert!(world.set_on(Coord::new(i64::MAX - 1, i64::MAX, i64::MAX)));
        assert_eq!(
            world.exposed_faces(min_corner),
            ALL_FACES_EXPOSED & !FACE_POS_X
        );
        assert_eq!(
            world.exposed_faces(max_corner),
            ALL_FACES_EXPOSED & !FACE_NEG_X
        );
        assert_internal_invariants(&world);
    }

    #[test]
    fn chunk_bounds_and_accessors_track_non_origin_insertions_and_boundary_deletions() {
        let mut world = World::new();
        let target_chunk = ChunkCoord::new(2, 3, 4);
        assert_eq!(world.chunk_bounds(target_chunk), None);
        assert_eq!(world.chunk_words(target_chunk), None);

        let p1 = Coord::new(35, 50, 68);
        let p2 = Coord::new(40, 55, 70);
        let far_outer = Coord::new(500, 500, 500);

        assert!(world.set_on(p1));
        assert_eq!(
            world.chunk_bounds(target_chunk),
            Some(Bounds { min: p1, max: p1 })
        );

        assert!(world.set_on(p2));
        assert!(world.set_on(far_outer));
        assert_eq!(
            world.chunk_bounds(target_chunk),
            Some(Bounds { min: p1, max: p2 })
        );

        let (count, words) = world
            .chunk_words(target_chunk)
            .expect("target chunk must be allocated");
        assert_eq!(count, 2);
        let round_trip = World::iter_chunk_coords(target_chunk, words).collect::<HashSet<_>>();
        assert_eq!(round_trip, HashSet::from([p1, p2]));

        assert!(world.set_off(p2));
        assert_eq!(
            world.chunk_bounds(target_chunk),
            Some(Bounds { min: p1, max: p1 })
        );

        assert!(world.set_off(far_outer));
        assert_eq!(
            world.bounds(),
            Some(Bounds {
                min: BEDROCK_COORD,
                max: p1,
            })
        );

        let chunk_summaries = world
            .iter_chunks()
            .map(|(coord, c, b, _)| (coord, c, b))
            .collect::<Vec<_>>();
        assert_eq!(chunk_summaries.len(), 2);
        assert_internal_invariants(&world);
    }

    #[test]
    fn sparse_octree_pyramid_tracks_hierarchy_child_masks_and_root_folding() {
        let mut world = World::new();
        assert_eq!(
            world.octant_summary(OctantCoord {
                depth: 0,
                x: 0,
                y: 0,
                z: 0,
            }),
            None
        );
        assert_eq!(
            world.octant_summary(OctantCoord {
                depth: OCTREE_MAX_DEPTH + 1,
                x: 0,
                y: 0,
                z: 0,
            }),
            None
        );

        let sibling_chunk_point = Coord::new(16, 0, 16);
        let negative_macro_point = Coord::new(-5000, -5000, -5000);
        assert!(world.set_on(sibling_chunk_point));
        assert!(world.set_on(negative_macro_point));

        let level1_origin = world
            .octant_summary(OctantCoord {
                depth: 1,
                x: 0,
                y: 0,
                z: 0,
            })
            .expect("depth-1 origin octant must be active");
        assert_eq!(
            level1_origin,
            OctantSummary {
                occupied_bits: 2,
                child_mask: (1 << 0) | (1 << 5),
                bounds: Bounds {
                    min: BEDROCK_COORD,
                    max: sibling_chunk_point,
                },
                revision: level1_origin.revision,
            }
        );

        let roots = world.root_octants().collect::<Vec<_>>();
        assert_eq!(roots.len(), 2);
        assert_internal_invariants(&world);

        assert!(world.set_off(sibling_chunk_point));
        assert!(world.set_off(negative_macro_point));
        assert_bedrock_only(&world);
    }
}
