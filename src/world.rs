use std::collections::HashMap;

const CHUNK_SIDE: i64 = 16;
const CHUNK_WORDS: usize = 64;

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

#[derive(Clone, Debug)]
struct Chunk {
    bits: [u64; CHUNK_WORDS],
    count: u16,
}

impl Chunk {
    fn empty() -> Self {
        Self {
            bits: [0; CHUNK_WORDS],
            count: 0,
        }
    }

    fn is_on(&self, index: usize) -> bool {
        let (word, mask) = word_and_mask(index);
        self.bits[word] & mask != 0
    }

    fn set_on(&mut self, index: usize) -> bool {
        let (word, mask) = word_and_mask(index);
        if self.bits[word] & mask != 0 {
            return false;
        }

        self.bits[word] |= mask;
        self.count += 1;
        true
    }

    fn set_off(&mut self, index: usize) -> bool {
        let (word, mask) = word_and_mask(index);
        if self.bits[word] & mask == 0 {
            return false;
        }

        self.bits[word] &= !mask;
        self.count -= 1;
        true
    }

    fn iter_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.bits
            .iter()
            .copied()
            .enumerate()
            .flat_map(|(word_index, mut bits)| {
                std::iter::from_fn(move || {
                    if bits == 0 {
                        return None;
                    }

                    let bit_index = bits.trailing_zeros() as usize;
                    bits &= bits - 1;
                    Some(word_index * u64::BITS as usize + bit_index)
                })
            })
    }
}

/// A sparse bit world. Coordinates not present in an allocated chunk are off.
#[derive(Clone, Debug)]
pub struct World {
    chunks: HashMap<ChunkCoord, Chunk>,
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

    /// Turns a coordinate on. Returns true only when the world changed.
    pub fn set_on(&mut self, coord: Coord) -> bool {
        let (chunk_coord, local_index) = split_coord(coord);
        let changed = self
            .chunks
            .entry(chunk_coord)
            .or_insert_with(Chunk::empty)
            .set_on(local_index);

        if changed {
            self.len += 1;
            self.extend_bounds(coord);
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
                let changed = chunk.set_off(local_index);
                (changed, chunk.count == 0)
            }
            None => (false, false),
        };

        let touched_bounds = changed && self.coord_touches_bounds(coord);
        if changed {
            self.len -= 1;
        }
        if chunk_is_empty {
            self.chunks.remove(&chunk_coord);
        }
        if changed {
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

    fn extend_bounds(&mut self, coord: Coord) {
        match &mut self.bounds {
            Some(bounds) => {
                bounds.min.x = bounds.min.x.min(coord.x);
                bounds.min.y = bounds.min.y.min(coord.y);
                bounds.min.z = bounds.min.z.min(coord.z);
                bounds.max.x = bounds.max.x.max(coord.x);
                bounds.max.y = bounds.max.y.max(coord.y);
                bounds.max.z = bounds.max.z.max(coord.z);
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
        self.bounds.is_some_and(|bounds| {
            coord.x == bounds.min.x
                || coord.y == bounds.min.y
                || coord.z == bounds.min.z
                || coord.x == bounds.max.x
                || coord.y == bounds.max.y
                || coord.z == bounds.max.z
        })
    }

    fn recompute_bounds(&self) -> Option<Bounds> {
        let mut coords = self.iter_on();
        let first = coords.next()?;
        let mut bounds = Bounds {
            min: first,
            max: first,
        };

        for coord in coords {
            bounds.min.x = bounds.min.x.min(coord.x);
            bounds.min.y = bounds.min.y.min(coord.y);
            bounds.min.z = bounds.min.z.min(coord.z);
            bounds.max.x = bounds.max.x.max(coord.x);
            bounds.max.y = bounds.max.y.max(coord.y);
            bounds.max.z = bounds.max.z.max(coord.z);
        }

        Some(bounds)
    }

    /// Iterates over every coordinate that is on. Iteration order is unspecified.
    pub fn iter_on(&self) -> impl Iterator<Item = Coord> + '_ {
        self.chunks.iter().flat_map(|(chunk_coord, chunk)| {
            chunk
                .iter_indices()
                .map(move |index| join_coord(*chunk_coord, index))
        })
    }
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

        let coords = world.iter_on().collect::<HashSet<_>>();
        assert_eq!(coords.len(), world.len());
        assert_eq!(world.bounds(), world.recompute_bounds());

        let mut counted_bits = 0_usize;
        for chunk in world.chunks.values() {
            assert_ne!(chunk.count, 0, "allocated chunks must not be empty");
            let bits = chunk
                .bits
                .iter()
                .map(|word| word.count_ones() as usize)
                .sum::<usize>();
            assert_eq!(usize::from(chunk.count), bits);
            counted_bits += bits;
        }
        assert_eq!(world.len(), counted_bits);
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
}
