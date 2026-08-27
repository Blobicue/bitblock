//! Physical-pixel projection of the bit world.
//!
//! Rendering never changes world or observer state. At the top depth the complete finite
//! world is represented by one framebuffer pixel; below that depth, checkpoint 1 rasterizes
//! the currently allocated bits directly. A hierarchy for intermediate-scale LOD is
//! intentionally deferred until a later checkpoint demonstrates the need.

use std::sync::OnceLock;

use crate::camera::{Camera, CameraProjection, ONE_PIXEL_EPSILON, ProjectedPoint, Ray, Vec3};
use crate::frame::{FrameObservation, FrameRef, OverlayInput};
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::world::{BEDROCK_COORD, Bounds, Coord, World};

const BACKGROUND: u32 = 0x000000;
const AGGREGATE_BIT: u32 = 0x00ffffff;
const BEDROCK_PIXEL: u32 = 0x00ff9f1c;
const BEDROCK_TOP: u32 = 0x00ffc15a;
const BEDROCK_SIDE: u32 = 0x00c86f0a;
const BEDROCK_FRONT: u32 = 0x00e8890c;
const BEDROCK_BACK: u32 = 0x00a95508;
const RGB_MASK: u32 = 0x00ffffff;
const NEAR_PLANE: f64 = 1.0e-7;
const CROSSHAIR_GAP_RADIUS: i64 = 2;
const CROSSHAIR_ARM_LENGTH: i64 = 3;
const BARYCENTRIC_EPSILON: f64 = -1.0e-10;
/// Buckets used to order occupied bits front to back.
///
/// The order is a heuristic and every order produces the same frame, so a linear quantization
/// of view depth is enough. Two counting passes cost a fraction of a comparison sort, which at
/// 12,699 bits spent more time ordering than the ordering saved.
const DEPTH_BUCKETS: usize = 1_024;
// Slack terms for the hidden-bit test in `nearest_possible_depth`. They bound the arithmetic
// of the inner loop rather than describing the frame, so they are deliberately absent from
// `renderer_profile_tag`: a skipped bit writes exactly what rasterising it would have written,
// which is nothing, and no declared constant or ordering rule changes.
//
// A rasterised fragment's depth is `1.0 / (wa/za + wb/zb + wc/zc)`, which is at least
// `min(za, zb, zc) / S` for `S` the sum of the positive weights. The three `edge` values sum
// exactly to `area`, so `S` exceeds one only by the weights' own rounding plus the three
// admitted negative weights:
//
// * `OCCLUSION_WEIGHT_SLACK` covers `3 * |BARYCENTRIC_EPSILON|` and the rounding of the sum.
// * `OCCLUSION_EDGE_SLACK` is `34 * 2^-53 = 3.8e-15` rounded up: each `edge` evaluation carries
//   at most `8.4 * 2^-53 * span_x * span_y` of absolute error, and four of them (three weights
//   and the area itself) reach the weight sum divided by `area`.
// * `OCCLUSION_ROUNDING_SLACK` covers the six roundings between the weights and the reciprocal.
const OCCLUSION_WEIGHT_SLACK: f64 = 1.0e-9;
const OCCLUSION_EDGE_SLACK: f64 = 1.0e-14;
const OCCLUSION_ROUNDING_SLACK: f64 = 1.0e-12;
const SMALL_BIT_EXTENT: f64 = 1.0;
const PIXEL_CENTER_OFFSET: f64 = 0.5;
const ORDINARY_TOP: u32 = 0x00ffffff;
const ORDINARY_SIDE: u32 = 0x00bdbdbd;
const ORDINARY_BACK: u32 = 0x00d8d8d8;
const ORDINARY_FRONT: u32 = 0x00989898;
const FINAL_GAME_BASE_PALETTE: [u32; 10] = [
    BACKGROUND,
    AGGREGATE_BIT,
    BEDROCK_PIXEL,
    BEDROCK_TOP,
    BEDROCK_SIDE,
    BEDROCK_FRONT,
    BEDROCK_BACK,
    ORDINARY_SIDE,
    ORDINARY_BACK,
    ORDINARY_FRONT,
];
const RENDERER_PROFILE_SCHEMA: &[u8] = b"bitblock/renderer-profile/host-f64/v1";
const CROSSHAIR_POLICY_SCHEMA: &[u8] = b"bitblock/overlay-policy/crosshair/v1";

static RENDERER_PROFILE_TAG: OnceLock<ContentTag> = OnceLock::new();
static CROSSHAIR_POLICY_TAG: OnceLock<ContentTag> = OnceLock::new();

/// Declared semantic profile of the current renderer constants and ordering rules.
///
/// This tag identifies the declared profile; it is not a hash of the executable and therefore
/// does not prove that an implementation followed the declaration.
pub fn renderer_profile_tag() -> ContentTag {
    *RENDERER_PROFILE_TAG.get_or_init(|| {
        hash_canonical(TagKind::Policy, RENDERER_PROFILE_SCHEMA, 106, |sink| {
            for revision in [1_u16, 1, 1, 1] {
                sink(&revision.to_be_bytes());
            }
            sink(&[0]); // current host Rust f64 arithmetic
            sink(&[0]); // total_cmp depth; equal depth chooses lower RGB
            for value in [
                NEAR_PLANE,
                BARYCENTRIC_EPSILON,
                ONE_PIXEL_EPSILON,
                SMALL_BIT_EXTENT,
                PIXEL_CENTER_OFFSET,
                f64::EPSILON,
            ] {
                sink(&value.to_bits().to_be_bytes());
            }
            for color in [
                RGB_MASK,
                BACKGROUND,
                AGGREGATE_BIT,
                BEDROCK_PIXEL,
                BEDROCK_TOP,
                BEDROCK_SIDE,
                BEDROCK_FRONT,
                BEDROCK_BACK,
                ORDINARY_TOP,
                ORDINARY_SIDE,
                ORDINARY_BACK,
                ORDINARY_FRONT,
            ] {
                sink(&color.to_be_bytes());
            }
        })
        .expect("renderer profile encoding has a fixed length")
    })
}

pub fn crosshair_policy_tag() -> ContentTag {
    *CROSSHAIR_POLICY_TAG.get_or_init(|| {
        hash_canonical(TagKind::Policy, CROSSHAIR_POLICY_SCHEMA, 16, |sink| {
            sink(&1_u16.to_be_bytes());
            sink(&[0]); // integer-divided physical-pixel center
            sink(&(CROSSHAIR_GAP_RADIUS as u32).to_be_bytes());
            sink(&(CROSSHAIR_ARM_LENGTH as u32).to_be_bytes());
            sink(&RGB_MASK.to_be_bytes());
            sink(&[0]); // overlay never writes depth
        })
        .expect("crosshair policy encoding has a fixed length")
    })
}

/// Exact policy-plus-payload object for the current static crosshair.
pub fn crosshair_overlay_input() -> OverlayInput {
    OverlayInput::new(crosshair_policy_tag(), None)
        .expect("crosshair policy has the required policy tag kind")
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub coord: Coord,
    pub adjacent: Option<Coord>,
    pub distance: f64,
}

/// The first occupied voxel reached by a finite point movement.
///
/// `distance` is measured from the segment start in world units. Voxel AABBs are closed:
/// touching a face, edge, or corner is a collision, and a point that starts inside or on an
/// occupied voxel reports distance zero. Callers that move the observer should therefore retain
/// a small clearance before `distance` rather than placing it exactly on the closed boundary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SweepHit {
    pub coord: Coord,
    pub distance: f64,
}

pub struct Renderer {
    width: u32,
    height: u32,
    pixels: Vec<u32>,
    depth: Vec<f64>,
    world_pixels: u64,
    /// Occupied bits paired with their view depth, retained between frames for its allocation.
    keyed: Vec<(f64, Coord)>,
    /// The same bits in front-to-back traversal order.
    order: Vec<Coord>,
    /// Test-only switch back to unordered traversal with no hidden-bit test, so a fixture can
    /// compare the two traversals rather than compare the optimized one against itself.
    #[cfg(test)]
    reference_traversal: bool,
    /// Test-only count of bits the hidden-bit test declined to rasterise, so a fixture can show
    /// it is exercising that path rather than passing vacuously.
    #[cfg(test)]
    hidden_bits: u64,
    /// Test-only switch that rasterises every skipped bit into a copy of the buffers and
    /// requires that nothing moved. See `assert_hidden_bit_writes_nothing`.
    #[cfg(test)]
    verify_hidden_bits: bool,
}

/// First witness for a post-overlay game pixel outside the declared discrete RGB palette.
///
/// The high byte is intentionally retained but not judged here: XRGB unused-byte conformance is
/// a separate frame fact and visual metric. This audit concerns only the low 24-bit RGB payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IllegalFinalGameColor {
    pixel_index: usize,
    word: u32,
    rgb: u32,
}

impl IllegalFinalGameColor {
    pub const fn pixel_index(self) -> usize {
        self.pixel_index
    }

    pub const fn word(self) -> u32 {
        self.word
    }

    pub const fn rgb(self) -> u32 {
        self.rgb
    }
}

/// Exact palette-membership projection of one final, post-crosshair game framebuffer.
///
/// It recognizes the renderer's declared background/world colors and their RGB inversions under
/// the fixed crosshair rule. It is not an equivalence proof: substituting one legal color for
/// another still needs the complete frame tag and independent raster oracle to be detected.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FinalGamePaletteAudit {
    pixel_count: u64,
    illegal_rgb_count: u64,
    first_illegal: Option<IllegalFinalGameColor>,
}

impl FinalGamePaletteAudit {
    pub const fn pixel_count(self) -> u64 {
        self.pixel_count
    }

    pub const fn illegal_rgb_count(self) -> u64 {
        self.illegal_rgb_count
    }

    pub const fn first_illegal(self) -> Option<IllegalFinalGameColor> {
        self.first_illegal
    }

    pub const fn is_valid(self) -> bool {
        self.illegal_rgb_count == 0
    }

    fn inspect(&mut self, pixel_index: usize, word: u32) {
        self.pixel_count += 1;
        let rgb = word & RGB_MASK;
        if !is_legal_final_game_rgb(rgb) {
            self.illegal_rgb_count += 1;
            self.first_illegal.get_or_insert(IllegalFinalGameColor {
                pixel_index,
                word,
                rgb,
            });
        }
    }
}

impl Renderer {
    pub fn new(width: u32, height: u32) -> Self {
        let mut renderer = Self {
            width: 0,
            height: 0,
            pixels: Vec::new(),
            depth: Vec::new(),
            world_pixels: 0,
            keyed: Vec::new(),
            order: Vec::new(),
            #[cfg(test)]
            reference_traversal: false,
            #[cfg(test)]
            hidden_bits: 0,
            #[cfg(test)]
            verify_hidden_bits: false,
        };
        renderer.resize(width, height);
        renderer
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        let len = (width as usize)
            .checked_mul(height as usize)
            .expect("framebuffer dimensions overflow usize");
        self.pixels.resize(len, BACKGROUND);
        self.depth.resize(len, f64::INFINITY);
    }

    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels(&self) -> &[u32] {
        &self.pixels
    }

    /// Observes the final post-crosshair game frame and its legal-palette conformance together.
    ///
    /// The palette projection is fused into the existing complete frame observation; it does not
    /// make a second framebuffer pass or alter the canonical frame tag.
    pub fn observe_final_game_frame(&self) -> (FrameObservation, FinalGamePaletteAudit) {
        let frame = FrameRef::new(self.width, self.height, &self.pixels)
            .expect("renderer must retain a dimension-consistent game framebuffer");
        let mut palette = FinalGamePaletteAudit::default();
        let observation = frame.observe_with(|index, word| palette.inspect(index, word));
        (observation, palette)
    }

    /// Number of non-background pixels written by the world pass, before overlays.
    pub const fn world_pixel_count(&self) -> u64 {
        self.world_pixels
    }

    pub fn render(&mut self, world: &World, camera: &Camera) {
        self.pixels.fill(BACKGROUND);
        self.depth.fill(f64::INFINITY);
        self.world_pixels = 0;
        #[cfg(test)]
        {
            self.hidden_bits = 0;
        }

        if self.width == 0 || self.height == 0 || world.is_empty() {
            return;
        }

        if let Some(camera_coord) = coord_containing(camera.position())
            && world.is_on(camera_coord)
        {
            self.pixels.fill(bit_pixel_color(camera_coord));
            self.depth.fill(0.0);
            self.world_pixels = self.pixels.len() as u64;
            return;
        }

        let projection = camera
            .projection_for_viewport(self.width, self.height)
            .expect("nonzero renderer viewport must produce a projection context");

        if let Some(bounds) = world.bounds() {
            let (min, max) = bounds_aabb(bounds);
            if projection.bounds_fit_one_pixel(min, max) {
                if let Some(projected) = projection.project(min + (max - min) * 0.5) {
                    self.plot(
                        projected.x.floor() as i64,
                        projected.y.floor() as i64,
                        projected.depth,
                        aggregate_color(world),
                    );
                }
                return;
            }
        }

        if !self.optimized_traversal() {
            for coord in world.iter_on() {
                self.draw_bit(projection, coord);
            }
            return;
        }

        // Nearest first. The depth buffer then holds the closest surfaces early, which is what
        // makes the hidden-bit test in `draw_bit` succeed often enough to pay for itself.
        // Traversal order is free to change: `plot` resolves equal depths by the lower RGB word,
        // so the frame is a per-pixel minimum over the bits and does not depend on their order.
        let mut keyed = std::mem::take(&mut self.keyed);
        keyed.clear();
        let mut nearest = f64::INFINITY;
        let mut farthest = f64::NEG_INFINITY;
        for coord in world.iter_on() {
            let key = projection.view_depth(coord_center(coord));
            nearest = f64::min(nearest, key);
            farthest = f64::max(farthest, key);
            keyed.push((key, coord));
        }

        let span = farthest - nearest;
        let scale = if span > 0.0 && span.is_finite() {
            (DEPTH_BUCKETS - 1) as f64 / span
        } else {
            0.0
        };
        // `as usize` saturates, so a non-finite or out-of-range key lands in a real bucket
        // instead of panicking. Which one does not matter: this only chooses an order.
        let bucket = |key: f64| (((key - nearest) * scale) as usize).min(DEPTH_BUCKETS - 1);

        let mut offsets = [0_usize; DEPTH_BUCKETS];
        for &(key, _) in &keyed {
            offsets[bucket(key)] += 1;
        }
        let mut start = 0;
        for offset in &mut offsets {
            let count = *offset;
            *offset = start;
            start += count;
        }

        let mut order = std::mem::take(&mut self.order);
        order.clear();
        order.resize(keyed.len(), BEDROCK_COORD);
        for &(key, coord) in &keyed {
            let slot = &mut offsets[bucket(key)];
            order[*slot] = coord;
            *slot += 1;
        }

        for &coord in &order {
            self.draw_bit(projection, coord);
        }
        self.keyed = keyed;
        self.order = order;
    }

    #[cfg(test)]
    const fn optimized_traversal(&self) -> bool {
        !self.reference_traversal
    }

    #[cfg(not(test))]
    const fn optimized_traversal(&self) -> bool {
        true
    }

    /// Draws a one-physical-pixel crosshair without covering the exact aim pixel.
    /// Its arms invert the world color beneath them so they remain visible at every depth.
    pub fn draw_crosshair(&mut self, overlay: OverlayInput) {
        debug_assert_eq!(
            overlay,
            crosshair_overlay_input(),
            "renderer received an unsupported crosshair overlay input"
        );
        if self.width == 0 || self.height == 0 {
            return;
        }

        let center_x = i64::from(self.width / 2);
        let center_y = i64::from(self.height / 2);
        let first = CROSSHAIR_GAP_RADIUS + 1;
        let last = CROSSHAIR_GAP_RADIUS + CROSSHAIR_ARM_LENGTH;

        for distance in first..=last {
            self.invert_pixel(center_x - distance, center_y);
            self.invert_pixel(center_x + distance, center_y);
            self.invert_pixel(center_x, center_y - distance);
            self.invert_pixel(center_x, center_y + distance);
        }
    }

    fn invert_pixel(&mut self, x: i64, y: i64) {
        if x < 0 || y < 0 || x >= i64::from(self.width) || y >= i64::from(self.height) {
            return;
        }
        let index = y as usize * self.width as usize + x as usize;
        self.pixels[index] ^= RGB_MASK;
    }

    fn draw_bit(&mut self, projection: CameraProjection, coord: Coord) {
        let center = coord_center(coord);
        let vertices = cube_vertices(center);
        let projected = vertices.map(|vertex| projection.project(vertex));

        let mut visible = false;
        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        let mut nearest_vertex = f64::INFINITY;
        for point in projected.iter().flatten() {
            visible = true;
            min_x = f64::min(min_x, point.x);
            max_x = f64::max(max_x, point.x);
            min_y = f64::min(min_y, point.y);
            max_y = f64::max(max_y, point.y);
            nearest_vertex = f64::min(nearest_vertex, point.depth);
        }
        if !visible {
            return;
        }

        if max_x - min_x <= SMALL_BIT_EXTENT && max_y - min_y <= SMALL_BIT_EXTENT {
            if let Some(point) = projection.project(center) {
                let nearest_depth = projected
                    .iter()
                    .flatten()
                    .map(|p| p.depth)
                    .fold(point.depth, f64::min);
                self.plot(
                    point.x.floor() as i64,
                    point.y.floor() as i64,
                    nearest_depth,
                    bit_pixel_color(coord),
                );
            }
            return;
        }

        if self.optimized_traversal() {
            // Every face's clamped raster box is contained in this one, because each is built
            // from a subset of these vertices by the same monotone `floor`/`ceil`/clamp
            // expressions. An empty box means no face reaches a pixel inside the viewport.
            let left = min_x.floor().max(0.0) as i64;
            let right = max_x.ceil().min(self.width as f64) as i64;
            let top = min_y.floor().max(0.0) as i64;
            let bottom = max_y.ceil().min(self.height as f64) as i64;
            if left >= right || top >= bottom {
                return;
            }

            // The hidden-bit test. Front-to-back traversal fills the near surfaces first, so by
            // the time a covered bit is reached the depth buffer already holds something strictly
            // nearer at every pixel the bit could touch. Rasterising it would then lose every
            // depth comparison in `plot` and write nothing, so skipping it writes nothing
            // different.
            if let Some(covering) = self.covering_depth(left, right, top, bottom, nearest_vertex)
                && covering
                    < nearest_possible_depth(
                        projection,
                        center,
                        &projected,
                        nearest_vertex,
                        max_x - min_x,
                        max_y - min_y,
                    )
            {
                #[cfg(test)]
                {
                    self.hidden_bits += 1;
                    self.assert_hidden_bit_writes_nothing(projection, coord, center, &projected);
                }
                return;
            }
        }

        self.draw_faces(projection, coord, center, &projected);
    }

    fn draw_faces(
        &mut self,
        projection: CameraProjection,
        coord: Coord,
        center: Vec3,
        projected: &[Option<ProjectedPoint>; 8],
    ) {
        for face in FACES {
            if face.normal.dot(projection.position() - center) <= 0.0 {
                continue;
            }
            let Some(a) = projected[face.corners[0]] else {
                continue;
            };
            let Some(b) = projected[face.corners[1]] else {
                continue;
            };
            let Some(c) = projected[face.corners[2]] else {
                continue;
            };
            let Some(d) = projected[face.corners[3]] else {
                continue;
            };
            let color = face_color(coord, face.normal);
            self.raster_triangle(a, b, c, color);
            self.raster_triangle(a, c, d, color);
        }
    }

    /// Rasterises a bit the hidden-bit test just rejected and requires that nothing moved.
    ///
    /// Comparing two complete frames can only show that a wrong skip survived to the end; a
    /// second bit writing the same pixel would hide it. This checks the claim the test actually
    /// makes -- that this bit, at this moment, would write nothing -- for every bit it skips.
    /// It copies both buffers per skipped bit, so it is a fixture tool, not a debug assertion.
    #[cfg(test)]
    fn assert_hidden_bit_writes_nothing(
        &mut self,
        projection: CameraProjection,
        coord: Coord,
        center: Vec3,
        projected: &[Option<ProjectedPoint>; 8],
    ) {
        if !self.verify_hidden_bits {
            return;
        }
        let pixels = self.pixels.clone();
        let depth = self.depth.clone();
        let world_pixels = self.world_pixels;

        self.draw_faces(projection, coord, center, projected);

        let pixel = self
            .pixels
            .iter()
            .zip(&pixels)
            .position(|(after, before)| after != before);
        assert_eq!(
            pixel, None,
            "skipped bit {coord:?} would have written a pixel"
        );
        let word = self
            .depth
            .iter()
            .zip(&depth)
            .position(|(after, before)| after.to_bits() != before.to_bits());
        assert_eq!(
            word, None,
            "skipped bit {coord:?} would have written a depth"
        );
        assert_eq!(self.world_pixels, world_pixels);
    }

    /// The farthest retained depth over a half-open pixel box, or `None` if any pixel there is
    /// already at or beyond `limit`.
    ///
    /// The early return is what keeps the test cheap for the bits that are actually visible:
    /// an untouched pixel holds `f64::INFINITY`, so a bit over open background usually stops
    /// after one word. A `NaN` would also stop it, which is the conservative direction.
    fn covering_depth(
        &self,
        left: i64,
        right: i64,
        top: i64,
        bottom: i64,
        limit: f64,
    ) -> Option<f64> {
        let width = self.width as usize;
        let mut covering = f64::NEG_INFINITY;
        for y in top..bottom {
            let row = y as usize * width;
            for &depth in &self.depth[row + left as usize..row + right as usize] {
                // Written as a `<` test rather than a `>=` one so that a `NaN` stops the scan.
                // The depth buffer cannot hold one, but `plot` would treat it as farther than
                // anything under `total_cmp`, so it must not be swallowed by `f64::max` below.
                let below = depth < limit;
                if !below {
                    return None;
                }
                covering = f64::max(covering, depth);
            }
        }
        Some(covering)
    }

    fn raster_triangle(
        &mut self,
        a: ProjectedPoint,
        b: ProjectedPoint,
        c: ProjectedPoint,
        color: u32,
    ) {
        if a.depth <= NEAR_PLANE || b.depth <= NEAR_PLANE || c.depth <= NEAR_PLANE {
            return;
        }

        let area = edge(a.x, a.y, b.x, b.y, c.x, c.y);
        if !area.is_finite() || area.abs() < f64::EPSILON {
            return;
        }

        let min_x = a.x.min(b.x).min(c.x).floor().max(0.0) as i64;
        let max_x = a.x.max(b.x).max(c.x).ceil().min(self.width as f64) as i64;
        let min_y = a.y.min(b.y).min(c.y).floor().max(0.0) as i64;
        let max_y = a.y.max(b.y).max(c.y).ceil().min(self.height as f64) as i64;

        for y in min_y..max_y {
            for x in min_x..max_x {
                let px = x as f64 + PIXEL_CENTER_OFFSET;
                let py = y as f64 + PIXEL_CENTER_OFFSET;
                let wa = edge(b.x, b.y, c.x, c.y, px, py) / area;
                let wb = edge(c.x, c.y, a.x, a.y, px, py) / area;
                let wc = edge(a.x, a.y, b.x, b.y, px, py) / area;
                if wa < BARYCENTRIC_EPSILON || wb < BARYCENTRIC_EPSILON || wc < BARYCENTRIC_EPSILON
                {
                    continue;
                }

                let inverse_depth = wa / a.depth + wb / b.depth + wc / c.depth;
                if inverse_depth <= 0.0 || !inverse_depth.is_finite() {
                    continue;
                }
                self.plot(x, y, 1.0 / inverse_depth, color);
            }
        }
    }

    fn plot(&mut self, x: i64, y: i64, depth: f64, color: u32) {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return;
        }
        let index = y as usize * self.width as usize + x as usize;
        let depth_order = depth.total_cmp(&self.depth[index]);
        // Equal-depth fragments choose the lower RGB word so unspecified world iteration order
        // cannot change the final frame.
        if depth_order.is_lt() || (depth_order.is_eq() && color < self.pixels[index]) {
            if self.pixels[index] == BACKGROUND && color != BACKGROUND {
                self.world_pixels += 1;
            }
            self.depth[index] = depth;
            self.pixels[index] = color;
        }
    }
}

fn select_raycast_hit(best: Option<Hit>, candidate: Hit) -> Option<Hit> {
    let replace = best.is_none_or(|current| {
        candidate
            .distance
            .total_cmp(&current.distance)
            .then_with(|| candidate.coord.cmp(&current.coord))
            .is_lt()
    });
    replace.then_some(candidate).or(best)
}

pub fn raycast(world: &World, ray: Ray) -> Option<Hit> {
    let mut best: Option<Hit> = None;

    for coord in world.iter_on() {
        let center = coord_center(coord);
        let min = center - Vec3::splat(0.5);
        let max = center + Vec3::splat(0.5);
        let Some((distance, normal)) = ray_box(ray, min, max) else {
            continue;
        };
        let adjacent = offset_coord(coord, normal);
        let candidate = Hit {
            coord,
            adjacent,
            distance,
        };
        best = select_raycast_hit(best, candidate);
    }

    best
}

/// Sweeps a point from `start` through the complete finite segment `start + delta`.
///
/// This tests the segment against every occupied voxel AABB, so a large delta cannot tunnel
/// through a block. Exact simultaneous hits are resolved by [`Coord`]'s lexicographic order,
/// making the result independent of the world's unspecified iteration order. The segment start,
/// delta, endpoint, and Euclidean delta length must all be finite.
pub fn sweep_point(world: &World, start: Vec3, delta: Vec3) -> Option<SweepHit> {
    assert!(
        start.is_finite() && delta.is_finite(),
        "swept point inputs must be finite"
    );
    assert!(
        (start + delta).is_finite(),
        "swept point endpoint must be finite"
    );
    let movement_length = stable_length(delta);
    assert!(
        movement_length.is_finite(),
        "swept point distance must be representable"
    );

    let mut best: Option<(f64, Coord)> = None;
    for coord in world.iter_on() {
        let center = coord_center(coord);
        let min = center - Vec3::splat(0.5);
        let max = center + Vec3::splat(0.5);
        let Some(fraction) = segment_box_entry_fraction(start, delta, min, max) else {
            continue;
        };

        let candidate = (fraction, coord);
        let replace = best.is_none_or(|current| {
            fraction
                .total_cmp(&current.0)
                .then_with(|| coord.cmp(&current.1))
                .is_lt()
        });
        if replace {
            best = Some(candidate);
        }
    }

    best.map(|(fraction, coord)| SweepHit {
        coord,
        distance: movement_length * fraction,
    })
}

pub fn coord_containing(point: Vec3) -> Option<Coord> {
    fn axis(value: f64) -> Option<i64> {
        let centered = (value + 0.5).floor();
        (centered.is_finite() && centered >= i64::MIN as f64 && centered <= i64::MAX as f64)
            .then_some(centered as i64)
    }

    Some(Coord::new(axis(point.x)?, axis(point.y)?, axis(point.z)?))
}

pub fn bounds_aabb(bounds: Bounds) -> (Vec3, Vec3) {
    (
        Vec3::new(
            bounds.min.x as f64 - 0.5,
            bounds.min.y as f64 - 0.5,
            bounds.min.z as f64 - 0.5,
        ),
        Vec3::new(
            bounds.max.x as f64 + 0.5,
            bounds.max.y as f64 + 0.5,
            bounds.max.z as f64 + 0.5,
        ),
    )
}

fn coord_center(coord: Coord) -> Vec3 {
    Vec3::new(coord.x as f64, coord.y as f64, coord.z as f64)
}

fn offset_coord(coord: Coord, normal: Vec3) -> Option<Coord> {
    Some(Coord::new(
        coord.x.checked_add(normal.x as i64)?,
        coord.y.checked_add(normal.y as i64)?,
        coord.z.checked_add(normal.z as i64)?,
    ))
}

fn ray_box(ray: Ray, min: Vec3, max: Vec3) -> Option<(f64, Vec3)> {
    let interval = box_interval(ray.origin, ray.direction, min, max)?;

    if interval.exit < 0.0 {
        return None;
    }
    if interval.entry >= 0.0 {
        Some((interval.entry, interval.entry_normal))
    } else {
        Some((0.0, interval.exit_normal))
    }
}

fn segment_box_entry_fraction(origin: Vec3, delta: Vec3, min: Vec3, max: Vec3) -> Option<f64> {
    let interval = box_interval(origin, delta, min, max)?;
    if interval.exit < 0.0 || interval.entry > 1.0 {
        return None;
    }
    Some(interval.entry.max(0.0))
}

#[derive(Clone, Copy)]
struct BoxInterval {
    entry: f64,
    exit: f64,
    entry_normal: Vec3,
    exit_normal: Vec3,
}

fn box_interval(origin: Vec3, direction: Vec3, min: Vec3, max: Vec3) -> Option<BoxInterval> {
    if !origin.is_finite() || !direction.is_finite() || !min.is_finite() || !max.is_finite() {
        return None;
    }

    let origins = [origin.x, origin.y, origin.z];
    let directions = [direction.x, direction.y, direction.z];
    let minima = [min.x, min.y, min.z];
    let maxima = [max.x, max.y, max.z];
    let axes = [Vec3::X, Vec3::Y, Vec3::Z];
    let mut entry = f64::NEG_INFINITY;
    let mut exit = f64::INFINITY;
    let mut entry_normal = Vec3::ZERO;
    let mut exit_normal = Vec3::ZERO;

    for axis in 0..3 {
        let origin = origins[axis];
        let direction = directions[axis];
        if direction == 0.0 {
            if origin < minima[axis] || origin > maxima[axis] {
                return None;
            }
            continue;
        }

        let mut near = (minima[axis] - origin) / direction;
        let mut far = (maxima[axis] - origin) / direction;
        let mut near_normal = -axes[axis];
        let mut far_normal = axes[axis];
        if near > far {
            std::mem::swap(&mut near, &mut far);
            std::mem::swap(&mut near_normal, &mut far_normal);
        }
        if near > entry {
            entry = near;
            entry_normal = near_normal;
        }
        if far < exit {
            exit = far;
            exit_normal = far_normal;
        }
        if entry > exit {
            return None;
        }
    }

    Some(BoxInterval {
        entry,
        exit,
        entry_normal,
        exit_normal,
    })
}

fn stable_length(vector: Vec3) -> f64 {
    let scale = vector.x.abs().max(vector.y.abs()).max(vector.z.abs());
    if scale == 0.0 {
        return 0.0;
    }
    let scaled = vector / scale;
    scale * scaled.length()
}

fn cube_vertices(center: Vec3) -> [Vec3; 8] {
    let h = 0.5;
    [
        center + Vec3::new(-h, -h, -h),
        center + Vec3::new(h, -h, -h),
        center + Vec3::new(h, h, -h),
        center + Vec3::new(-h, h, -h),
        center + Vec3::new(-h, -h, h),
        center + Vec3::new(h, -h, h),
        center + Vec3::new(h, h, h),
        center + Vec3::new(-h, h, h),
    ]
}

#[derive(Clone, Copy)]
struct Face {
    corners: [usize; 4],
    normal: Vec3,
}

const FACES: [Face; 6] = [
    Face {
        corners: [0, 3, 2, 1],
        normal: Vec3::new(0.0, 0.0, -1.0),
    },
    Face {
        corners: [4, 5, 6, 7],
        normal: Vec3::new(0.0, 0.0, 1.0),
    },
    Face {
        corners: [0, 4, 7, 3],
        normal: Vec3::new(-1.0, 0.0, 0.0),
    },
    Face {
        corners: [1, 2, 6, 5],
        normal: Vec3::new(1.0, 0.0, 0.0),
    },
    Face {
        corners: [0, 1, 5, 4],
        normal: Vec3::new(0.0, -1.0, 0.0),
    },
    Face {
        corners: [3, 7, 6, 2],
        normal: Vec3::new(0.0, 1.0, 0.0),
    },
];

fn aggregate_color(world: &World) -> u32 {
    if world.len() == 1 && world.is_on(BEDROCK_COORD) {
        BEDROCK_PIXEL
    } else {
        AGGREGATE_BIT
    }
}

fn bit_pixel_color(coord: Coord) -> u32 {
    if coord == BEDROCK_COORD {
        BEDROCK_PIXEL
    } else {
        AGGREGATE_BIT
    }
}

fn face_color(coord: Coord, normal: Vec3) -> u32 {
    if coord == BEDROCK_COORD {
        return if normal.y > 0.5 {
            BEDROCK_TOP
        } else if normal.x.abs() > 0.5 {
            BEDROCK_SIDE
        } else if normal.z > 0.5 {
            BEDROCK_BACK
        } else {
            BEDROCK_FRONT
        };
    }

    if normal.y > 0.5 {
        ORDINARY_TOP
    } else if normal.x.abs() > 0.5 {
        ORDINARY_SIDE
    } else if normal.z > 0.5 {
        ORDINARY_BACK
    } else {
        ORDINARY_FRONT
    }
}

fn is_legal_final_game_rgb(rgb: u32) -> bool {
    FINAL_GAME_BASE_PALETTE.contains(&rgb) || FINAL_GAME_BASE_PALETTE.contains(&(rgb ^ RGB_MASK))
}

/// A lower bound on every depth the faces of one bit can write, given its projected vertices.
///
/// `raster_triangle` writes `1.0 / (wa/za + wb/zb + wc/zc)` at a pixel whose three weights are
/// all at least `BARYCENTRIC_EPSILON`. Dropping the negative terms, that inverse depth is at
/// most `S / nearest`, where `S` is the sum of the positive weights and `nearest` the smallest
/// vertex depth of the bit, so the depth itself is at least `nearest / S`.
///
/// `S` would be exactly one in real arithmetic: the three `edge` numerators sum to the shared
/// `area` denominator, and a weight is only negative by the admitted epsilon. What is left is
/// rounding, which is bounded by the span of the coordinates entering `edge` divided by the
/// smallest `area` any of the bit's drawable triangles will use. A sliver triangle therefore
/// widens the bound rather than escaping it, and an unrepresentable span collapses it to zero
/// or `NaN`, which fails the caller's strict comparison and rasterises.
fn nearest_possible_depth(
    projection: CameraProjection,
    center: Vec3,
    projected: &[Option<ProjectedPoint>; 8],
    nearest: f64,
    span_x: f64,
    span_y: f64,
) -> f64 {
    let mut smallest_area = f64::INFINITY;
    for face in FACES {
        if face.normal.dot(projection.position() - center) <= 0.0 {
            continue;
        }
        let corners = face.corners.map(|corner| projected[corner]);
        let [Some(a), Some(b), Some(c), Some(d)] = corners else {
            continue;
        };
        // The same two triangles `draw_bit` would submit, rejected on the same terms.
        for area in [
            edge(a.x, a.y, b.x, b.y, c.x, c.y),
            edge(a.x, a.y, c.x, c.y, d.x, d.y),
        ] {
            if area.is_finite() && area.abs() >= f64::EPSILON {
                smallest_area = f64::min(smallest_area, area.abs());
            }
        }
    }

    // A pixel center lies within one pixel of the vertex box on each axis, so no operand of an
    // `edge` evaluation exceeds the span plus two.
    let conditioning = OCCLUSION_EDGE_SLACK * (span_x + 2.0) * (span_y + 2.0) / smallest_area;
    nearest * (1.0 - OCCLUSION_ROUNDING_SLACK) / (1.0 + OCCLUSION_WEIGHT_SLACK + conditioning)
}

fn edge(ax: f64, ay: f64, bx: f64, by: f64, px: f64, py: f64) -> f64 {
    (px - ax) * (by - ay) - (py - ay) * (bx - ax)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::Camera;
    use crate::frame::{FrameFacts, FrameRef};
    use std::f64::consts::PI;

    /// `tools/render_policy_vectors.py` rebuilds all three payloads from the declared constants
    /// and applies the content-tag wrapper without running any Rust, so a moved constant fails
    /// from both sides.
    #[test]
    fn declared_renderer_and_crosshair_profiles_match_independent_vectors() {
        assert_eq!(
            renderer_profile_tag().to_string(),
            "#bb.policy.sha256.tsxgj54fonmrrlkoygcn5dtv65iu6neymvyee3x6hartgevwyynq"
        );
        assert_eq!(
            crosshair_policy_tag().to_string(),
            "#bb.policy.sha256.pyj2osdlm4onqqyrhhlvy2qjfxxkfpfsg7flmjxeodoho55i4hta"
        );
        let overlay = crosshair_overlay_input();
        assert_eq!(overlay.policy(), crosshair_policy_tag());
        assert_eq!(overlay.payload(), None);
        assert_eq!(
            overlay.tag().to_string(),
            "#bb.policy.sha256.3cjxsg2du7fhxwr6ihi7hno4sdwd7xbojoidnpvsrq5ubbaa7wfa"
        );
        assert!(overlay.verify().is_ok());
    }

    fn assert_close(actual: f64, expected: f64) {
        let tolerance = 1.0e-12 * expected.abs().max(1.0);
        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected:.15}, got {actual:.15}"
        );
    }

    #[test]
    fn initial_world_is_exactly_one_physical_pixel() {
        let size = 801;
        let world = World::new_with_origin();
        let camera = Camera::for_single_pixel_voxel(size);
        let mut renderer = Renderer::new(size, size);

        renderer.render(&world, &camera);

        let lit: Vec<usize> = renderer
            .pixels()
            .iter()
            .enumerate()
            .filter_map(|(index, pixel)| (*pixel != BACKGROUND).then_some(index))
            .collect();
        assert_eq!(
            lit,
            vec![(size as usize / 2) * size as usize + size as usize / 2]
        );
        assert_eq!(renderer.pixels()[lit[0]], BEDROCK_PIXEL);
    }

    #[test]
    fn final_game_palette_audit_accepts_declared_base_and_inverted_colors() {
        let mut declared = Vec::with_capacity(FINAL_GAME_BASE_PALETTE.len() * 2);
        for &rgb in &FINAL_GAME_BASE_PALETTE {
            declared.push(rgb);
            declared.push(rgb ^ RGB_MASK);
        }
        let mut renderer = Renderer::new(declared.len() as u32, 1);
        renderer.pixels.copy_from_slice(&declared);

        let (observation, audit) = renderer.observe_final_game_frame();

        assert_eq!(observation.facts.pixel_count, declared.len() as u64);
        assert_eq!(audit.pixel_count(), declared.len() as u64);
        assert_eq!(audit.illegal_rgb_count(), 0);
        assert_eq!(audit.first_illegal(), None);
        assert!(audit.is_valid());
    }

    #[test]
    fn final_game_palette_audit_reports_rgb_violations_but_leaves_xrgb_to_frame_facts() {
        let mut renderer = Renderer::new(4, 1);
        renderer
            .pixels
            .copy_from_slice(&[BACKGROUND, 0x0012_3456, 0xaa00_0000, 0x0001_0203]);

        let (observation, audit) = renderer.observe_final_game_frame();
        let first = audit
            .first_illegal()
            .expect("fixture has an illegal RGB word");

        assert_eq!(observation.facts.invalid_high_byte, 1);
        assert_eq!(audit.pixel_count(), 4);
        assert_eq!(audit.illegal_rgb_count(), 2);
        assert_eq!(first.pixel_index(), 1);
        assert_eq!(first.word(), 0x0012_3456);
        assert_eq!(first.rgb(), 0x0012_3456);
        assert!(!audit.is_valid());
    }

    #[test]
    fn renderer_and_crosshair_produce_only_declared_final_rgb_colors() {
        let mut renderer = Renderer::new(101, 101);
        renderer.render(&World::new_with_origin(), &Camera::for_observer_spawn());
        renderer.draw_crosshair(crosshair_overlay_input());

        let (_, audit) = renderer.observe_final_game_frame();

        assert!(audit.is_valid(), "unexpected palette witness: {audit:?}");
    }

    #[test]
    fn named_frame_fixtures_match_full_protocol_tags_and_facts() {
        let size = 801;
        let initial = World::new();
        assert_frame_fixture(
            &initial,
            &Camera::for_single_pixel_voxel(size),
            "#bb.frame.sha256.v3unl2j3pmtir5r46woypwxrytacujbgb4vcdfsmn7vsm2n6qisa",
            FrameFacts {
                pixel_count: 641_601,
                non_background: 13,
                invalid_high_byte: 0,
                rgb_one_bits: 305,
                red_sum: 3_315,
                green_sum: 3_219,
                blue_sum: 3_088,
            },
        );
        assert_frame_fixture(
            &initial,
            &Camera::for_observer_spawn(),
            "#bb.frame.sha256.vb5a7qopjxv2ifpngcqyxk5qsosnllmzhk5nogmngayblzi426ja",
            FrameFacts {
                pixel_count: 641_601,
                non_background: 641_601,
                invalid_high_byte: 0,
                rgb_one_bits: 5_774_481,
                red_sum: 148_848_924,
                green_sum: 87_899_109,
                blue_sum: 7_701_984,
            },
        );

        let mut aggregate = World::new();
        for coord in [
            Coord::new(-7, 3, 2),
            Coord::new(1, 0, 0),
            Coord::new(11, -4, 9),
        ] {
            aggregate.set_on(coord);
        }
        let mut camera = Camera::for_observer_spawn();
        let (min, max) = bounds_aabb(aggregate.bounds().unwrap());
        camera.fit_bounds(min, max, size, size);
        assert_frame_fixture(
            &aggregate,
            &camera,
            "#bb.frame.sha256.2fglu6quf3euqjid6ajlqyttijiau6afvv33z3blzg5vyig2kieq",
            FrameFacts {
                pixel_count: 641_601,
                non_background: 13,
                invalid_high_byte: 0,
                rgb_one_bits: 312,
                red_sum: 3_315,
                green_sum: 3_315,
                blue_sum: 3_315,
            },
        );
    }

    fn assert_frame_fixture(
        world: &World,
        camera: &Camera,
        expected_tag: &str,
        expected_facts: FrameFacts,
    ) {
        let mut renderer = Renderer::new(801, 801);
        renderer.render(world, camera);
        renderer.draw_crosshair(crosshair_overlay_input());
        let (observation, palette_audit) = renderer.observe_final_game_frame();
        assert!(
            palette_audit.is_valid(),
            "fixture emitted an illegal final RGB color: {palette_audit:?}"
        );
        assert_eq!(observation.tag.to_string(), expected_tag);
        assert_eq!(observation.facts, expected_facts);
    }

    const FRONT_BEDROCK_REFERENCE: &[(usize, u32, u64)] = &[
        (25, 0x00ff_ffff, 0x7ff0_0000_0000_0000),
        (42, 0x00ff_ffff, 0x7ff0_0000_0000_0000),
        (59, 0x00ff_ffff, 0x7ff0_0000_0000_0000),
        (92, 0x00e8_890c, 0x4000_0000_0000_0000),
        (93, 0x00e8_890c, 0x4000_0000_0000_0000),
        (94, 0x00e8_890c, 0x4000_0000_0000_0000),
        (105, 0x00ff_ffff, 0x7ff0_0000_0000_0000),
        (106, 0x00ff_ffff, 0x7ff0_0000_0000_0000),
        (107, 0x00ff_ffff, 0x7ff0_0000_0000_0000),
        (109, 0x00e8_890c, 0x4000_0000_0000_0000),
        (110, 0x00e8_890c, 0x4000_0000_0000_0000),
        (111, 0x00e8_890c, 0x4000_0000_0000_0000),
        (113, 0x00ff_ffff, 0x7ff0_0000_0000_0000),
        (114, 0x00ff_ffff, 0x7ff0_0000_0000_0000),
        (115, 0x00ff_ffff, 0x7ff0_0000_0000_0000),
        (126, 0x00e8_890c, 0x4000_0000_0000_0000),
        (127, 0x00e8_890c, 0x4000_0000_0000_0000),
        (128, 0x00e8_890c, 0x4000_0000_0000_0000),
        (161, 0x00ff_ffff, 0x7ff0_0000_0000_0000),
        (178, 0x00ff_ffff, 0x7ff0_0000_0000_0000),
        (195, 0x00ff_ffff, 0x7ff0_0000_0000_0000),
    ];

    const OFFSET_OCCLUSION_REFERENCE: &[(usize, u32, u64)] = &[
        (201, 0x00ff_ffff, 0x7ff0_0000_0000_0000),
        (232, 0x00ff_ffff, 0x7ff0_0000_0000_0000),
        (263, 0x00ff_ffff, 0x7ff0_0000_0000_0000),
        (351, 0x00ff_ffff, 0x7ff0_0000_0000_0000),
        (352, 0x00ff_ffff, 0x7ff0_0000_0000_0000),
        (353, 0x00ff_ffff, 0x7ff0_0000_0000_0000),
        (359, 0x00ff_ffff, 0x7ff0_0000_0000_0000),
        (360, 0x00ff_ffff, 0x7ff0_0000_0000_0000),
        (361, 0x00ff_ffff, 0x7ff0_0000_0000_0000),
        (381, 0x0098_9898, 0x401a_0000_0000_0000),
        (382, 0x0098_9898, 0x401a_0000_0000_0000),
        (383, 0x00bd_bdbd, 0x401c_c000_0000_0000),
        (412, 0x0098_9898, 0x401a_0000_0000_0000),
        (413, 0x0098_9898, 0x401a_0000_0000_0000),
        (414, 0x00bd_bdbd, 0x401c_c000_0000_0000),
        (445, 0x0098_9898, 0x401a_0000_0000_0000),
        (446, 0x00ff_ffff, 0x4017_0000_0000_0000),
        (447, 0x00ff_ffff, 0x4017_0000_0000_0000),
        (448, 0x00bd_bdbd, 0x4017_0000_0000_0000),
        (449, 0x00ff_ffff, 0x7ff0_0000_0000_0000),
        (474, 0x00e8_890c, 0x4012_0000_0000_0000),
        (475, 0x00e8_890c, 0x4012_0000_0000_0000),
        (476, 0x00e8_890c, 0x4012_0000_0000_0000),
        (477, 0x0098_9898, 0x4016_0000_0000_0000),
        (478, 0x0098_9898, 0x4016_0000_0000_0000),
        (479, 0x00bd_bdbd, 0x4017_0000_0000_0000),
        (480, 0x00ff_ffff, 0x7ff0_0000_0000_0000),
        (505, 0x00e8_890c, 0x4012_0000_0000_0000),
        (506, 0x00e8_890c, 0x4012_0000_0000_0000),
        (507, 0x00e8_890c, 0x4012_0000_0000_0000),
        (508, 0x0098_9898, 0x4016_0000_0000_0000),
        (509, 0x0098_9898, 0x4016_0000_0000_0000),
        (510, 0x00bd_bdbd, 0x4017_0000_0000_0000),
        (511, 0x00ff_ffff, 0x7ff0_0000_0000_0000),
        (536, 0x00e8_890c, 0x4012_0000_0000_0000),
        (537, 0x00e8_890c, 0x4012_0000_0000_0000),
        (538, 0x00e8_890c, 0x4012_0000_0000_0000),
    ];

    const FRONT_BEDROCK_BLOCKS: &[Coord] = &[BEDROCK_COORD];
    const OFFSET_OCCLUSION_BLOCKS: &[Coord] = &[
        BEDROCK_COORD,
        Coord::new(1, 0, 1),
        Coord::new(-1, 1, 2),
        Coord::new(0, 0, 2),
    ];
    const OFFSET_OCCLUSION_DEPTH_RESIDUALS: &[(usize, u64, u64, u64)] = &[
        (382, 0x401a_0000_0000_0000, 0x401a_0000_0000_0001, 1),
        (383, 0x401c_c000_0000_0000, 0x401c_c000_0000_0002, 2),
        (413, 0x401a_0000_0000_0000, 0x401a_0000_0000_0001, 1),
        (414, 0x401c_c000_0000_0000, 0x401c_c000_0000_0002, 2),
        (446, 0x4017_0000_0000_0000, 0x4017_0000_0000_0001, 1),
        (447, 0x4017_0000_0000_0000, 0x4016_ffff_ffff_ffff, 1),
        (448, 0x4017_0000_0000_0000, 0x4016_ffff_ffff_fffe, 2),
        (479, 0x4017_0000_0000_0000, 0x4016_ffff_ffff_ffff, 1),
        (510, 0x4017_0000_0000_0000, 0x4016_ffff_ffff_fffe, 2),
    ];
    const ANALYTIC_ORACLE_PROFILE_DIGEST: [u8; 32] = [
        0x93, 0x1f, 0x03, 0x94, 0xee, 0x00, 0xe3, 0x8a, 0xc7, 0x24, 0xcd, 0xad, 0xbd, 0x36, 0xa4,
        0xd8, 0xae, 0xa5, 0x0b, 0xf3, 0x8f, 0x28, 0x11, 0x4c, 0xad, 0x39, 0x2d, 0xf1, 0x48, 0x4f,
        0xf7, 0x28,
    ];
    const FRONT_BEDROCK_DEPTH_DIGEST: [u8; 32] = [
        0xff, 0xaa, 0x1c, 0xb3, 0x9e, 0x55, 0x0a, 0x69, 0xe1, 0x30, 0xf8, 0x1a, 0x34, 0x15, 0x59,
        0xb0, 0x54, 0xe9, 0xd4, 0x3c, 0x7f, 0x78, 0x1d, 0x61, 0x7b, 0xec, 0x96, 0x0e, 0x2a, 0xbb,
        0x15, 0x88,
    ];
    const OFFSET_OCCLUSION_DEPTH_DIGEST: [u8; 32] = [
        0xd4, 0x77, 0xe1, 0xce, 0xb8, 0x64, 0x1d, 0x3e, 0x9d, 0x30, 0x0e, 0xb2, 0xac, 0xee, 0x34,
        0xf9, 0x49, 0xd5, 0x23, 0xd6, 0x04, 0x04, 0xba, 0x94, 0xe6, 0x9e, 0x4d, 0xd6, 0x99, 0x84,
        0x30, 0xa9,
    ];
    const ANALYTIC_CORPUS_SCHEMA: &[u8] = b"bitblock/reference-renderer-corpus-receipt/v1";
    const ANALYTIC_OBSERVATION_DISTANCE: f64 = 1.0;
    const ANALYTIC_VERTICAL_FOV: f64 = std::f64::consts::FRAC_PI_2;

    struct AnalyticReferenceFixture {
        name: &'static str,
        width: u32,
        height: u32,
        camera_position: Vec3,
        blocks: &'static [Coord],
        samples: &'static [(usize, u32, u64)],
        depth_residuals: &'static [(usize, u64, u64, u64)],
        depth_digest: [u8; 32],
        frame_tag: &'static str,
    }

    fn reference_buffers(fixture: &AnalyticReferenceFixture) -> (Vec<u32>, Vec<f64>) {
        let len = fixture.width as usize * fixture.height as usize;
        let mut pixels = vec![BACKGROUND; len];
        let mut depths = vec![f64::INFINITY; len];
        let mut previous = None;
        for &(index, pixel, depth_bits) in fixture.samples {
            assert!(index < len, "{} reference index {index}", fixture.name);
            assert!(previous.is_none_or(|prior| prior < index));
            assert_eq!(pixel & !RGB_MASK, 0);
            pixels[index] = pixel;
            depths[index] = f64::from_bits(depth_bits);
            previous = Some(index);
        }
        (pixels, depths)
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct SliceShapeMismatch {
        expected: usize,
        actual: usize,
    }

    fn shape_mismatch(expected: usize, actual: usize) -> Option<SliceShapeMismatch> {
        (expected != actual).then_some(SliceShapeMismatch { expected, actual })
    }

    type RgbMismatch = (usize, u32, u32);

    fn rgb_comparison_vector(
        expected: &[u32],
        actual: &[u32],
    ) -> (Option<SliceShapeMismatch>, Vec<RgbMismatch>) {
        let mismatches = expected
            .iter()
            .zip(actual)
            .enumerate()
            .filter_map(|(index, (&expected, &actual))| {
                (expected != actual).then_some((index, expected, actual))
            })
            .collect();
        (shape_mismatch(expected.len(), actual.len()), mismatches)
    }

    type DepthClassMismatch = (usize, u64, u64);
    type DepthResidual = (usize, u64, u64, u64);

    fn depth_comparison_vectors(
        expected: &[f64],
        actual: &[f64],
    ) -> (
        Option<SliceShapeMismatch>,
        Vec<DepthClassMismatch>,
        Vec<DepthResidual>,
    ) {
        let mut class_mismatches = Vec::new();
        let mut residuals = Vec::new();
        for (index, (&expected, &actual)) in expected.iter().zip(actual).enumerate() {
            match (expected.is_finite(), actual.is_finite()) {
                (true, true) if expected >= 0.0 && actual >= 0.0 => {
                    let expected_bits = expected.to_bits();
                    let actual_bits = actual.to_bits();
                    if expected_bits != actual_bits {
                        residuals.push((
                            index,
                            expected_bits,
                            actual_bits,
                            expected_bits.abs_diff(actual_bits),
                        ));
                    }
                }
                (false, false) if expected == f64::INFINITY && actual == f64::INFINITY => {}
                _ => class_mismatches.push((index, expected.to_bits(), actual.to_bits())),
            }
        }
        (
            shape_mismatch(expected.len(), actual.len()),
            class_mismatches,
            residuals,
        )
    }

    fn fixture_camera(fixture: &AnalyticReferenceFixture) -> Camera {
        Camera::from_position(
            fixture.camera_position,
            0.0,
            0.0,
            ANALYTIC_OBSERVATION_DISTANCE,
            ANALYTIC_VERTICAL_FOV,
        )
    }

    fn append_fixture_corpus_payload(
        payload: &mut Vec<u8>,
        fixture: &AnalyticReferenceFixture,
        camera: &Camera,
        frame_tag: ContentTag,
    ) {
        let name = fixture.name.as_bytes();
        payload.extend_from_slice(&(name.len() as u16).to_be_bytes());
        payload.extend_from_slice(name);
        payload.extend_from_slice(&fixture.width.to_be_bytes());
        payload.extend_from_slice(&fixture.height.to_be_bytes());
        for value in [
            camera.position().x,
            camera.position().y,
            camera.position().z,
            camera.yaw_radians(),
            camera.pitch_radians(),
            camera.observation_anchor().x,
            camera.observation_anchor().y,
            camera.observation_anchor().z,
            camera.observation_distance(),
            camera.vertical_fov_radians(),
        ] {
            payload.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        payload.extend_from_slice(&(fixture.blocks.len() as u32).to_be_bytes());
        for coord in fixture.blocks {
            payload.extend_from_slice(&coord.x.to_be_bytes());
            payload.extend_from_slice(&coord.y.to_be_bytes());
            payload.extend_from_slice(&coord.z.to_be_bytes());
        }
        payload.extend_from_slice(&frame_tag.digest());
        payload.extend_from_slice(&fixture.depth_digest);
    }

    fn analytic_corpus_tag(
        fixtures: &[AnalyticReferenceFixture],
        frame_tags: &[ContentTag],
    ) -> ContentTag {
        assert_eq!(fixtures.len(), frame_tags.len());
        let mut payload = Vec::new();
        payload.extend_from_slice(&ANALYTIC_ORACLE_PROFILE_DIGEST);
        payload.extend_from_slice(&renderer_profile_tag().digest());
        for (fixture, &frame_tag) in fixtures.iter().zip(frame_tags) {
            append_fixture_corpus_payload(
                &mut payload,
                fixture,
                &fixture_camera(fixture),
                frame_tag,
            );
        }
        hash_canonical(
            TagKind::Receipt,
            ANALYTIC_CORPUS_SCHEMA,
            payload.len() as u64,
            |sink| sink(&payload),
        )
        .unwrap()
    }

    fn assert_analytic_reference(fixture: &AnalyticReferenceFixture) -> ContentTag {
        let mut world = World::new();
        for &coord in fixture.blocks {
            world.set_on(coord);
        }
        let camera = fixture_camera(fixture);

        let (min, max) = bounds_aabb(world.bounds().unwrap());
        assert!(!camera.bounds_fit_one_pixel(min, max, fixture.width, fixture.height));
        for &coord in fixture.blocks {
            let projected = cube_vertices(coord_center(coord)).map(|point| {
                camera
                    .project(point, fixture.width, fixture.height)
                    .unwrap()
            });
            let x_extent = projected
                .iter()
                .map(|point| point.x)
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(min, max), value| {
                    (min.min(value), max.max(value))
                });
            let y_extent = projected
                .iter()
                .map(|point| point.y)
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(min, max), value| {
                    (min.min(value), max.max(value))
                });
            assert!(x_extent.1 - x_extent.0 > SMALL_BIT_EXTENT);
            assert!(y_extent.1 - y_extent.0 > SMALL_BIT_EXTENT);
        }

        let mut renderer = Renderer::new(fixture.width, fixture.height);
        renderer.render(&world, &camera);
        renderer.draw_crosshair(crosshair_overlay_input());
        let (expected_pixels, expected_depths) = reference_buffers(fixture);

        let (rgb_shape, rgb_mismatches) = rgb_comparison_vector(&expected_pixels, &renderer.pixels);
        assert_eq!(rgb_shape, None, "{} RGB shape", fixture.name);
        assert!(
            rgb_mismatches.is_empty(),
            "{} RGB mismatch vector: {rgb_mismatches:?}",
            fixture.name
        );
        let frame_tag = FrameRef::new(fixture.width, fixture.height, &renderer.pixels)
            .unwrap()
            .content_tag();
        assert_eq!(frame_tag.to_string(), fixture.frame_tag);

        let (depth_shape, depth_class_mismatches, depth_residuals) =
            depth_comparison_vectors(&expected_depths, &renderer.depth);
        assert_eq!(depth_shape, None, "{} depth shape", fixture.name);
        assert!(
            depth_class_mismatches.is_empty(),
            "{} depth classification mismatches: {depth_class_mismatches:?}",
            fixture.name
        );
        assert_eq!(depth_residuals, fixture.depth_residuals, "{}", fixture.name);
        let max_ulp_residual = depth_residuals
            .iter()
            .map(|residual| residual.3)
            .max()
            .unwrap_or(0);
        assert!(
            max_ulp_residual <= 4,
            "{} maximum analytic/raster depth residual was {max_ulp_residual} ULP",
            fixture.name
        );
        assert_eq!(
            renderer.world_pixel_count(),
            expected_depths
                .iter()
                .filter(|depth| depth.is_finite())
                .count() as u64
        );

        let mut corrupted = renderer.pixels.clone();
        corrupted[0] ^= 1;
        let (shape, mismatches) = rgb_comparison_vector(&expected_pixels, &corrupted);
        assert_eq!(shape, None);
        assert_eq!(mismatches, vec![(0, expected_pixels[0], corrupted[0])]);
        corrupted.push(BACKGROUND);
        assert_eq!(
            rgb_comparison_vector(&expected_pixels, &corrupted).0,
            Some(SliceShapeMismatch {
                expected: expected_pixels.len(),
                actual: expected_pixels.len() + 1,
            })
        );

        let finite_index = expected_depths
            .iter()
            .position(|depth| depth.is_finite())
            .unwrap();
        let mut corrupted_depth = renderer.depth.clone();
        corrupted_depth[finite_index] = f64::from_bits(expected_depths[finite_index].to_bits() + 5);
        let (shape, classes, residuals) =
            depth_comparison_vectors(&expected_depths, &corrupted_depth);
        assert_eq!(shape, None);
        assert!(classes.is_empty());
        assert!(residuals.iter().any(|residual| residual.3 > 4));
        corrupted_depth.push(f64::INFINITY);
        assert_eq!(
            depth_comparison_vectors(&expected_depths, &corrupted_depth).0,
            Some(SliceShapeMismatch {
                expected: expected_depths.len(),
                actual: expected_depths.len() + 1,
            })
        );
        frame_tag
    }

    #[test]
    fn independent_analytic_ray_oracle_matches_bounded_rgb_and_depth_corpus() {
        let fixtures = [
            AnalyticReferenceFixture {
                name: "front-bedrock",
                width: 17,
                height: 13,
                camera_position: Vec3::new(0.0, 0.0, -2.5),
                blocks: FRONT_BEDROCK_BLOCKS,
                samples: FRONT_BEDROCK_REFERENCE,
                depth_residuals: &[],
                depth_digest: FRONT_BEDROCK_DEPTH_DIGEST,
                frame_tag: "#bb.frame.sha256.obrjmzpxoggqqmwukec7lpl6f24bcg63nn2l6q65ync2jxfeetra",
            },
            AnalyticReferenceFixture {
                name: "offset-occlusion",
                width: 31,
                height: 23,
                camera_position: Vec3::new(2.0, 2.0, -5.0),
                blocks: OFFSET_OCCLUSION_BLOCKS,
                samples: OFFSET_OCCLUSION_REFERENCE,
                depth_residuals: OFFSET_OCCLUSION_DEPTH_RESIDUALS,
                depth_digest: OFFSET_OCCLUSION_DEPTH_DIGEST,
                frame_tag: "#bb.frame.sha256.yrvobugmwykavefvg5xmrvzl3ugqb4scyjr32bdv5mrgag642c6a",
            },
        ];

        let frame_tags: Vec<_> = fixtures.iter().map(assert_analytic_reference).collect();
        assert_eq!(
            analytic_corpus_tag(&fixtures, &frame_tags).to_string(),
            "#bb.receipt.sha256.x2m63bxhu5ayhccnnij3ohv6ux7x3uk3hgal2qy334qb2atkam5q"
        );
    }

    #[test]
    fn bedrock_is_distinct_at_cube_scale() {
        let world = World::new_with_origin();
        let camera = Camera::for_observer_spawn();
        let mut renderer = Renderer::new(801, 801);

        crate::camera::reset_projection_context_build_count();
        renderer.render(&world, &camera);
        assert_eq!(
            crate::camera::projection_context_build_count(),
            1,
            "a direct render must build one shared projection context"
        );

        let center = (renderer.height() as usize / 2) * renderer.width() as usize
            + renderer.width() as usize / 2;
        assert_eq!(renderer.pixels()[center], BEDROCK_FRONT);
        assert_ne!(BEDROCK_FRONT, face_color(Coord::new(1, 0, 0), -Vec3::Z));
    }

    /// A hollow cube shell: the cheapest world in which most bits are hidden behind other bits.
    fn shell_world(side: i64) -> World {
        block_world(side, false)
    }

    /// A solid cube. Its interior bits are hidden by covering surfaces at many different depths,
    /// which is the case a shell cannot produce: a shell's hidden bits sit far behind one smooth
    /// near face, so the hidden-bit test never has to decide anything close.
    fn solid_world(side: i64) -> World {
        block_world(side, true)
    }

    fn block_world(side: i64, solid: bool) -> World {
        let mut world = World::new();
        for x in 0..side {
            for y in 0..side {
                for z in 0..side {
                    let interior =
                        x > 0 && y > 0 && z > 0 && x < side - 1 && y < side - 1 && z < side - 1;
                    if solid || !interior {
                        world.set_on(Coord::new(x - side / 2, y - side / 2, z - side / 2));
                    }
                }
            }
        }
        world
    }

    fn views(world: &World) -> [Camera; 5] {
        let (min, max) = bounds_aabb(world.bounds().unwrap());
        let anchor = min + (max - min) * 0.5;
        let radius = (max - min).length();
        let fov = crate::camera::DEFAULT_VERTICAL_FOV_RADIANS;
        [
            Camera::from_anchor(anchor, radius * 1.5, 0.0, 0.0, fov),
            Camera::from_anchor(anchor, radius * 1.5, 0.7, -0.4, fov),
            Camera::from_anchor(anchor, radius * 0.9, 0.35, 0.15, fov),
            Camera::from_anchor(anchor, radius * 0.55, -2.1, 0.3, fov),
            Camera::from_anchor(anchor, radius * 0.12, 1.2, 1.1, fov),
        ]
    }

    /// The frame fixtures above all use four bits or fewer, so none of them reaches the
    /// hidden-bit test at scale. These do: most of each world hides behind the rest of it, and
    /// the ordered traversal with the test enabled must still agree with the unordered traversal
    /// without it, pixel for pixel and depth bit for depth bit.
    ///
    /// Whole-frame agreement is the weaker half. `verify_hidden_bits` also rasterises every bit
    /// the test skips and requires that it moved nothing at the moment it was skipped, which a
    /// later bit covering the same pixel cannot hide.
    #[test]
    fn hidden_bit_test_leaves_dense_worlds_identical_to_unordered_traversal() {
        let size = 201;
        for (name, world) in [("shell", shell_world(14)), ("solid", solid_world(9))] {
            let mut hidden = 0;
            for (index, camera) in views(&world).iter().enumerate() {
                let mut reference = Renderer::new(size, size);
                reference.reference_traversal = true;
                reference.render(&world, camera);

                let mut ordered = Renderer::new(size, size);
                ordered.verify_hidden_bits = true;
                ordered.render(&world, camera);

                let pixel = ordered
                    .pixels
                    .iter()
                    .zip(&reference.pixels)
                    .position(|(actual, expected)| actual != expected);
                assert_eq!(pixel, None, "{name} camera {index} pixel");
                let word = ordered
                    .depth
                    .iter()
                    .zip(&reference.depth)
                    .position(|(actual, expected)| actual.to_bits() != expected.to_bits());
                assert_eq!(word, None, "{name} camera {index} depth");
                assert_eq!(
                    ordered.world_pixel_count(),
                    reference.world_pixel_count(),
                    "{name} camera {index} world pixels"
                );
                assert_eq!(reference.hidden_bits, 0, "the reference must not skip bits");
                hidden += ordered.hidden_bits;
            }

            // The closest camera sits inside the world, where almost nothing is hidden; the
            // outside views are where the test does its work. Asserting on the total keeps the
            // fixture from going quietly vacuous without demanding that every camera occlude.
            assert!(
                hidden > world.len() as u64,
                "the {name} cameras skipped {hidden} bits of a {}-bit world in total, so this                  fixture no longer exercises the hidden-bit test",
                world.len()
            );
        }
    }

    /// xorshift64*, so the corpus below is a fixed sequence rather than a fresh one per run.
    struct Rng(u64);

    impl Rng {
        fn next_u64(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
        }

        fn below(&mut self, bound: u64) -> u64 {
            self.next_u64() % bound
        }

        /// A value in `[low, high)`.
        fn range(&mut self, low: f64, high: f64) -> f64 {
            let unit = (self.next_u64() >> 11) as f64 / (1_u64 << 53) as f64;
            low + (high - low) * unit
        }
    }

    /// Hand-built worlds agree too easily: in a shell or a solid block, a bit whose screen box is
    /// fully covered is fully hidden, so even a deliberately broken depth bound keeps passing.
    /// The configurations that could separate them -- two bits at nearly one depth, one of them
    /// poking through the other, silhouettes, sliver faces, bits straddling the near plane and
    /// the viewport edge -- are easier to find by search than by hand.
    ///
    /// Small frames and small worlds, so the whole corpus is a fraction of a second, with
    /// `verify_hidden_bits` on throughout: every skipped bit is rasterised into a copy of the
    /// buffers and must move nothing.
    #[test]
    fn hidden_bit_test_survives_a_seeded_corpus_of_worlds_and_cameras() {
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        let mut hidden = 0;
        for case in 0..320 {
            let extent = 1 + rng.below(3) as i64;
            let density = rng.range(0.3, 1.0);
            let mut world = World::new();
            for x in -extent..=extent {
                for y in -extent..=extent {
                    for z in -extent..=extent {
                        if rng.range(0.0, 1.0) < density {
                            world.set_on(Coord::new(x, y, z));
                        }
                    }
                }
            }
            let anchor = Vec3::new(
                rng.range(-2.0, 2.0),
                rng.range(-2.0, 2.0),
                rng.range(-2.0, 2.0),
            );
            let camera = Camera::from_anchor(
                anchor,
                rng.range(0.4, 16.0),
                rng.range(-PI, PI),
                rng.range(-1.5, 1.5),
                rng.range(0.2, 2.8),
            );
            let width = 9 + rng.below(48) as u32;
            let height = 9 + rng.below(48) as u32;

            let mut reference = Renderer::new(width, height);
            reference.reference_traversal = true;
            reference.render(&world, &camera);

            let mut ordered = Renderer::new(width, height);
            ordered.verify_hidden_bits = true;
            ordered.render(&world, &camera);

            let pixel = ordered
                .pixels
                .iter()
                .zip(&reference.pixels)
                .position(|(actual, expected)| actual != expected);
            assert_eq!(pixel, None, "case {case} pixel");
            let word = ordered
                .depth
                .iter()
                .zip(&reference.depth)
                .position(|(actual, expected)| actual.to_bits() != expected.to_bits());
            assert_eq!(word, None, "case {case} depth");
            assert_eq!(
                ordered.world_pixel_count(),
                reference.world_pixel_count(),
                "case {case} world pixels"
            );
            hidden += ordered.hidden_bits;
        }
        assert!(
            hidden > 2_000,
            "the corpus skipped only {hidden} bits, so it no longer exercises the test"
        );
    }

    #[test]
    fn equal_depth_pixels_do_not_depend_on_bit_iteration_order() {
        let camera = Camera::from_anchor(
            Vec3::ZERO,
            2_000.0,
            0.0,
            0.0,
            crate::camera::DEFAULT_VERTICAL_FOV_RADIANS,
        );
        let ordinary = Coord::new(1, 0, 0);
        let projection = camera.projection_for_viewport(801, 801).unwrap();

        let mut bedrock_first = Renderer::new(801, 801);
        bedrock_first.draw_bit(projection, BEDROCK_COORD);
        bedrock_first.draw_bit(projection, ordinary);

        let mut ordinary_first = Renderer::new(801, 801);
        ordinary_first.draw_bit(projection, ordinary);
        ordinary_first.draw_bit(projection, BEDROCK_COORD);

        assert_eq!(bedrock_first.pixels, ordinary_first.pixels);
        assert_eq!(bedrock_first.depth, ordinary_first.depth);
    }

    #[test]
    fn raycast_returns_hit_and_adjacent_coordinate() {
        let world = World::new_with_origin();
        let ray = Ray {
            origin: Vec3::new(0.0, 0.0, -10.0),
            direction: Vec3::Z,
        };

        let hit = raycast(&world, ray).unwrap();
        assert_eq!(hit.coord, Coord::new(0, 0, 0));
        assert_eq!(hit.adjacent, Some(Coord::new(0, 0, -1)));
        assert!((hit.distance - 9.5).abs() < 1.0e-12);
    }

    #[test]
    fn raycast_candidate_selection_is_permutation_invariant() {
        let epsilon = 1.0e-10;
        let nearest = Hit {
            coord: Coord::new(2, 0, 0),
            adjacent: None,
            distance: 10.0,
        };
        let middle = Hit {
            coord: Coord::new(1, 0, 0),
            adjacent: None,
            distance: 10.0 + 0.75 * epsilon,
        };
        let farthest = Hit {
            coord: Coord::new(0, 0, 0),
            adjacent: None,
            distance: 10.0 + 1.5 * epsilon,
        };
        let select = |order: [Hit; 3]| {
            order
                .into_iter()
                .fold(None, select_raycast_hit)
                .expect("three candidates contain one winner")
        };

        let forward = select([nearest, middle, farthest]);
        let reverse = select([farthest, middle, nearest]);

        assert_eq!(forward, reverse);
        assert_eq!(forward, nearest);
    }

    #[test]
    fn swept_point_has_no_hit_across_clear_space() {
        let world = World::new_with_origin();

        assert_eq!(
            sweep_point(
                &world,
                Vec3::new(-20.0, 2.0, -3.0),
                Vec3::new(40.0, 0.0, 0.0),
            ),
            None
        );
    }

    #[test]
    fn swept_point_reports_boundary_so_caller_can_stop_before_block() {
        let world = World::new_with_origin();
        let start = Vec3::new(0.0, 0.0, -2.0);
        let delta = Vec3::new(0.0, 0.0, 3.0);

        let hit = sweep_point(&world, start, delta).unwrap();
        assert_eq!(hit.coord, BEDROCK_COORD);
        assert_close(hit.distance, 1.5);

        let direction = delta.normalized();
        let stopped = start + direction * (hit.distance - 1.0e-9);
        assert_eq!(coord_containing(stopped), Some(Coord::new(0, 0, -1)));
        assert_eq!(
            coord_containing(start + direction * hit.distance),
            Some(BEDROCK_COORD),
            "the exact closed boundary is occupied; movement must retain clearance"
        );
    }

    #[test]
    fn swept_point_cannot_tunnel_through_block_at_high_speed() {
        let world = World::new_with_origin();
        let start = Vec3::new(0.0, 0.0, -1_000_000.0);
        let delta = Vec3::new(0.0, 0.0, 2_000_000.0);

        let hit = sweep_point(&world, start, delta).unwrap();
        assert_eq!(hit.coord, BEDROCK_COORD);
        assert_close(hit.distance, 999_999.5);
    }

    #[test]
    fn swept_point_handles_diagonal_and_closed_tangent_hits() {
        let world = World::new_with_origin();

        let diagonal = sweep_point(
            &world,
            Vec3::new(-2.0, -2.0, -2.0),
            Vec3::new(4.0, 4.0, 4.0),
        )
        .unwrap();
        assert_eq!(diagonal.coord, BEDROCK_COORD);
        assert_close(diagonal.distance, 1.5 * 3.0_f64.sqrt());

        let tangent =
            sweep_point(&world, Vec3::new(0.0, 0.5, -2.0), Vec3::new(0.0, 0.0, 4.0)).unwrap();
        assert_eq!(tangent.coord, BEDROCK_COORD);
        assert_close(tangent.distance, 1.5);
    }

    #[test]
    fn swept_point_breaks_simultaneous_closed_face_ties_by_coordinate() {
        let mut world = World::new_with_origin();
        world.set_on(Coord::new(1, 0, 0));

        let hit = sweep_point(&world, Vec3::new(0.5, 0.0, -2.0), Vec3::new(0.0, 0.0, 4.0)).unwrap();
        assert_eq!(hit.coord, BEDROCK_COORD);
        assert_close(hit.distance, 1.5);
    }

    #[test]
    fn swept_point_starting_inside_reports_zero_distance() {
        let world = World::new_with_origin();

        let hit = sweep_point(&world, Vec3::ZERO, Vec3::new(0.0, 0.0, -10.0)).unwrap();
        assert_eq!(hit.coord, BEDROCK_COORD);
        assert_eq!(hit.distance, 0.0);
    }

    #[test]
    fn empty_space_maps_to_anchor_coordinate() {
        assert_eq!(
            coord_containing(Vec3::new(-0.6, 15.6, 0.49)),
            Some(Coord::new(-1, 16, 0))
        );
    }

    #[test]
    fn camera_inside_bit_fills_the_frame() {
        let world = World::new_with_origin();
        let camera = Camera::from_position(
            Vec3::ZERO,
            0.0,
            0.0,
            1.0,
            crate::camera::DEFAULT_VERTICAL_FOV_RADIANS,
        );
        let mut renderer = Renderer::new(9, 7);
        renderer.render(&world, &camera);
        assert!(
            renderer
                .pixels()
                .iter()
                .all(|pixel| *pixel == BEDROCK_PIXEL)
        );
    }

    #[test]
    fn crosshair_inverts_arms_and_preserves_the_exact_center_pixel() {
        let mut renderer = Renderer::new(11, 11);
        renderer.pixels.fill(AGGREGATE_BIT);
        let center = 5_usize;

        renderer.draw_crosshair(crosshair_overlay_input());

        assert_eq!(renderer.pixels[center * 11 + center], AGGREGATE_BIT);
        assert_eq!(renderer.pixels[center * 11 + center + 2], AGGREGATE_BIT);
        for distance in 3..=5 {
            assert_eq!(renderer.pixels[center * 11 + center - distance], BACKGROUND);
            assert_eq!(renderer.pixels[center * 11 + center + distance], BACKGROUND);
            assert_eq!(
                renderer.pixels[(center - distance) * 11 + center],
                BACKGROUND
            );
            assert_eq!(
                renderer.pixels[(center + distance) * 11 + center],
                BACKGROUND
            );
        }
    }
}
