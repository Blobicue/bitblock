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
/// A bit whose centre lies deeper than this in view space has every cube vertex in front of
/// the camera: a vertex is at most sqrt(3)/2 from the centre, so its depth (the same dot
/// product, rounding error ~1e-12 at play coordinates) stays above 1, and it projects to
/// finite screen coordinates.
const DEFERRED_BOX_MIN_DEPTH: f64 = 2.0;
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

/// Declared semantic policy tag for the static physical-pixel crosshair overlay.
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

/// Nearest occupied voxel intersection along a world-space ray, with adjacent placement coordinate.
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

/// Optional per-bit base colour override for the world pass (e.g. chem materials).
///
/// The callback returns the lit (top-face) RGB for a bit, or `None` to keep the declared
/// ordinary / bedrock colours. Faces are shaded from that base by [`tinted_face_color`] with the
/// same top / back / side / front ordering as the ordinary palette. Called at most once per drawn
/// bit per frame, never per pixel. [`Renderer::render`] passes no tint, so every untinted frame is
/// bit-identical to the declared renderer profile.
pub type BitTint<'a> = &'a dyn Fn(Coord) -> Option<u32>;

/// Face-shade multipliers (out of 256) applied to a tint base: top, back (+z), side (x), front (-z).
/// They follow the ordinary palette's ratios (255 / 216 / 189 / 152 of white).
const TINT_SHADES: [u32; 4] = [256, 217, 190, 153];

/// Shades a tint base RGB for one face normal (top / back / side / front, as `face_color`).
pub fn tinted_face_color(base: u32, normal: Vec3) -> u32 {
    let k = if normal.y > 0.5 {
        TINT_SHADES[0]
    } else if normal.x.abs() > 0.5 {
        TINT_SHADES[2]
    } else if normal.z > 0.5 {
        TINT_SHADES[1]
    } else {
        TINT_SHADES[3]
    };
    shade_rgb(base, k)
}

/// Every RGB a tint base can produce in the world pass (the pixel-sized colour plus the four
/// face shades), for palette audits of tinted frames.
pub fn tinted_shades(base: u32) -> [u32; 5] {
    let base = base & RGB_MASK;
    [
        base,
        shade_rgb(base, TINT_SHADES[0]),
        shade_rgb(base, TINT_SHADES[1]),
        shade_rgb(base, TINT_SHADES[2]),
        shade_rgb(base, TINT_SHADES[3]),
    ]
}

/// Bit offset of the optional lit-face mask in a [`BitTint`] result. Bits `24..30` name faces
/// (one bit per [`TINT_LIT_DIRS`] entry) drawn with [`lit_face_color`] instead of the directional
/// shade: flat (no falloff) and lifted slightly toward white, so a highlighted face reads as
/// distinct from the shaded surface while keeping its hue. A zero mask (every plain RGB) is
/// exactly the ordinary tinted path.
pub const TINT_LIT_SHIFT: u32 = 24;

/// Face directions of the lit-face mask bits: +x, -x, +y, -y, +z, -z.
pub const TINT_LIT_DIRS: [[i64; 3]; 6] = [
    [1, 0, 0],
    [-1, 0, 0],
    [0, 1, 0],
    [0, -1, 0],
    [0, 0, 1],
    [0, 0, -1],
];

/// Lift toward white (out of 256) for lit faces.
const TINT_LIT_LIFT: u32 = 48;

/// Lit-mask bit for an axis-aligned face normal (see [`TINT_LIT_DIRS`]).
pub fn tint_lit_bit(normal: Vec3) -> u32 {
    let i = if normal.x > 0.5 {
        0
    } else if normal.x < -0.5 {
        1
    } else if normal.y > 0.5 {
        2
    } else if normal.y < -0.5 {
        3
    } else if normal.z > 0.5 {
        4
    } else {
        5
    };
    1 << i
}

/// A tint base with its lit-face mask packed into the high byte (`mask` uses the low 6 bits).
pub const fn tint_with_lit_faces(base: u32, mask: u32) -> u32 {
    (base & RGB_MASK) | ((mask & 0x3f) << TINT_LIT_SHIFT)
}

/// Colour of a lit face for a tint base: unshaded base lifted `TINT_LIT_LIFT`/256 toward white.
pub fn lit_face_color(base: u32) -> u32 {
    let lift = |c: u32| c + (255 - c) * TINT_LIT_LIFT / 256;
    let r = lift((base >> 16) & 0xff);
    let g = lift((base >> 8) & 0xff);
    let b = lift(base & 0xff);
    (r << 16) | (g << 8) | b
}

const fn shade_rgb(base: u32, k: u32) -> u32 {
    let r = ((base >> 16) & 0xff) * k / 256;
    let g = ((base >> 8) & 0xff) * k / 256;
    let b = (base & 0xff) * k / 256;
    (r << 16) | (g << 8) | b
}

/// Software voxel rasterizer maintaining XRGB pixel and `f64` depth framebuffers.
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
    /// Per-row scratch for the span rasteriser: interpolated inverse depth and its reciprocal.
    span_inverse: Vec<f64>,
    span_depth: Vec<f64>,
    /// Lower bound on every depth the bit being drawn can write (`nearest_possible_depth`), or
    /// negative infinity when none applies. Never compared with `<=`, so a `NaN` disables it.
    occlusion_bound: f64,
    /// While `occlusion_bound` applies: for each pixel row of the bit's clamped raster box
    /// (from `open_top`), the half-open range from its first to one past its last pixel *not*
    /// strictly nearer than the bound. Empty when the whole row is. The set of such pixels
    /// cannot change while the bit is drawn (it only writes depths `>= bound`, and only over
    /// farther ones), so one scan per bit serves all six triangles.
    open_rows: Vec<(i64, i64)>,
    open_top: i64,
    /// Per `COVER_TILE`-square pixel tile, an upper bound on every depth in it (or `NaN`, which
    /// never passes a `<` test). Reset to infinity with the depth buffer; depths only ever
    /// decrease within a frame, so a bound stays valid until refreshed by an exact scan.
    tile_max: Vec<f64>,
    tiles_wide: usize,
    /// Per `COVER_COARSE`-square block of tiles, an upper bound on every depth in it: the
    /// largest of its tiles' bounds when last recomputed (`NaN` if any was). Every tile bound is
    /// an upper bound that stays valid while depths only decrease, so their maximum does too.
    coarse_max: Vec<f64>,
    coarse_wide: usize,
    /// Per pixel row, a half-open column range covering every pixel or depth written since the
    /// last clear (`lo >= hi` when none was). Lets the next frame restore the background and
    /// infinite depth over just those ranges instead of the whole framebuffer.
    dirty_rows: Vec<(u32, u32)>,
    /// Set when the dirty ranges do not describe the buffers (resize, whole-frame fill).
    all_dirty: bool,
    /// The previous frame's `dirty_rows`, kept by the partial clear (`prev_dirty_valid`), so
    /// `changed_rows` can bound what differs from that frame.
    prev_dirty_rows: Vec<(u32, u32)>,
    prev_dirty_valid: bool,
    /// The projection of the frame now in the buffers when that frame is a complete ordinary
    /// traversal (`render_tinted_delta` may then update it in place), else `None`.
    delta_base: Option<CameraProjection>,
    /// Whether `draw_crosshair` has inverted its pixels since the last render.
    crosshair_on: bool,
    /// Per pixel, the `owner_key` of the bit whose fragment it holds. Pixels still at the
    /// background may hold a stale key; that only ever adds pixels to a delta's reset set.
    owner: Vec<u32>,
    /// `owner_key` of the bit being drawn.
    owner_key: u32,
    /// Grown raster boxes of every bit the last full traversal reached and of bits delta frames
    /// added since; the first `delta_full_len` entries are in the tile lists (the traversal's,
    /// or every entry kept by the last `compact_delta_index`). See `render_tinted_delta`.
    delta_index: Vec<DeltaEntry>,
    delta_full_len: usize,
    /// Entries the last full traversal recorded (`delta_full_len` can differ after
    /// `compact_delta_index` folds later entries into the tile lists).
    delta_traversal_len: usize,
    /// Index growth allowed past 1.5x before a compaction (and past 1.5x a traversal's after
    /// one before the full path); tests shrink it to exercise both.
    delta_slack: usize,
    /// Compactions so far (diagnostics only).
    delta_compactions: u64,
    /// Whether `draw_bit_with` appends to `delta_index`, and whether every appended bit had all
    /// eight vertices in front of the camera (else its box may not bound its writes).
    recording: bool,
    /// Whether the draws are the recolour pass of a tint-only delta frame: a fragment that
    /// ties the stored depth of a pixel its own bit holds, with a higher RGB word, replaces the
    /// stored colour and records the pixel in `recolour_marks` instead of losing.
    marking: bool,
    /// Strictly hidden bits proved during recolour or, §6x, during a toggling delta frame's
    /// redraws (after its resets, so every later write that frame only nears a depth). Tint-only
    /// frames keep the same depths;
    /// full renders discard these proofs before any reuse, and a delta frame that toggles bits
    /// drops those whose box may meet a pixel it resets (§6x, `drop_proofs_over`). §6v: per-chunk
    /// bit words rather than a hash set of coordinates, so a busy gas tick neither hashes each
    /// loud bit nor stalls a frame rehashing the set as it grows.
    recolour_hidden: IndexedBits,
    recolour_marks: Vec<usize>,
    /// Per pixel, whether a second fragment may sit at exactly its stored depth: set by every
    /// equal-depth meeting in `plot` and the span writer, cleared by a strictly nearer write.
    /// Only `false` is a promise -- exactly one fragment of the frame lies at the stored depth,
    /// the owner's -- and it holds because every fragment at a pixel's final depth reaches the
    /// writers (the culls and the hidden-bit test skip only bits strictly behind) and a delta
    /// frame draws every bit that gains a fragment. Read by the recolour pass.
    tie: Vec<bool>,
    /// Whether the draws are the recolour pass's second step, which only has to reach the
    /// pixels `delta_mask` marks: a bit whose depth bound lies behind all of them is skipped.
    marks_only: bool,
    /// Whether the draws are a toggle delta frame's redraw or the recolour pass's second step
    /// (§6s), with `delta_mask` marking the pixels the frame reset or marked. Every pixel such a
    /// draw writes is added to the mask (and to `delta_written`). At a pixel outside the mask the
    /// stored fragment is one of the frame before, which still stands, and `tie` there already
    /// records every equal-depth meeting of that frame's fragments; a bit meeting its own stored
    /// fragment there again (it is redrawn because its box reaches a reset or marked pixel) is
    /// not a second fragment, so it leaves `tie` as it is. Each bit is drawn once per pass, so
    /// inside the mask a bit meets its own stored fragment only face against face (a crease),
    /// which `tie` records as before. Without this, a redrawn bit set `tie` at every pixel it
    /// kept, and each later recolour of it marked those pixels and redrew their tiles.
    delta_redraw: bool,
    delta_written: Vec<usize>,
    /// Tint-only delta frames so far (diagnostics only).
    delta_recolours: u64,
    /// Pixels the recolour passes marked (tests only).
    #[cfg(test)]
    recolour_marked: u64,
    /// Test-only count of recolour draws answered by an earlier hidden proof.
    #[cfg(test)]
    recolour_reused: u64,
    /// Proved bits a toggling delta frame kept (tests only, §6x).
    #[cfg(test)]
    recolour_kept: u64,
    /// Changed bits a toggling delta frame dropped as proved hidden, and proofs its redraws
    /// made (tests only, §6x).
    #[cfg(test)]
    toggle_skipped: u64,
    #[cfg(test)]
    toggle_proved: u64,
    delta_index_ok: bool,
    /// The bits holding an entry in `delta_index`, when `delta_members_ok` (built on the first
    /// delta frame that records, kept by compaction, dropped by a full traversal).
    delta_members: IndexedBits,
    /// A delta frame's bits within one step of a toggled bit (kept for its storage).
    toggled_near: IndexedBits,
    delta_members_ok: bool,
    /// Per `COVER_TILE` tile, the full-traversal entries whose box touches it (CSR; built on
    /// the first delta frame after a full one, `delta_csr_built`).
    delta_tile_start: Vec<u32>,
    delta_tile_items: Vec<u32>,
    delta_csr_built: bool,
    /// Set by a full traversal from the projection of the frame before it (see
    /// `prepare_delta_lists`).
    delta_lists_eager: bool,
    /// Delta scratch: per-entry and per-tile visit stamps, the reset-pixel mask and list.
    delta_seen: Vec<u32>,
    delta_tile_seen: Vec<u32>,
    delta_stamp: u32,
    delta_mask: Vec<bool>,
    delta_pixels: Vec<usize>,
    /// §6v: storage kept across delta frames for their classification (see
    /// `render_tinted_delta`), so a busy tick does not allocate and grow them every frame.
    delta_scratch: DeltaScratch,
    /// Frames `render_tinted_delta` drew as deltas (diagnostics only).
    delta_frames: u64,
    /// Frames rendered so far; identifies which frame `changed_rows` is relative to.
    frame_serial: u64,
    /// Switch back to unordered traversal with no hidden-bit test or chunk frustum culling, so a
    /// fixture can compare the two traversals rather than compare the optimized one against itself.
    reference_traversal: bool,
    /// Test-only count of bits the hidden-bit test declined to rasterise, so a fixture can show
    /// it is exercising that path rather than passing vacuously.
    #[cfg(test)]
    hidden_bits: u64,
    /// Test-only switch that rasterises every skipped bit into a copy of the buffers and
    /// requires that nothing moved. See `assert_hidden_bit_writes_nothing`.
    #[cfg(test)]
    verify_hidden_bits: bool,
    /// Test-only switch that bypasses the coarse level of `covering_depth`, and the count of
    /// bits that level alone proved hidden, so a fixture can compare with and without it.
    #[cfg(test)]
    skip_coarse_cover: bool,
    /// Test-only switch that turns off the enclosed-bit cull, so a fixture can compare with and
    /// without it.
    #[cfg(test)]
    skip_enclosed_cull: bool,
    /// Test-only count of enclosed bits the cull left out of the traversal, and (while
    /// `verify_hidden_bits` is on) the bits themselves, checked against the finished frame.
    #[cfg(test)]
    enclosed_bits: u64,
    #[cfg(test)]
    enclosed_skipped: Vec<Coord>,
    /// Test-only switch for that finished-frame check alone, without the (much dearer)
    /// per-skip check of `verify_hidden_bits`.
    #[cfg(test)]
    verify_enclosed_bits: bool,
    #[cfg(test)]
    coarse_hidden: u64,
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
    /// Returns the linear row-major framebuffer index of the first illegal game pixel.
    pub const fn pixel_index(self) -> usize {
        self.pixel_index
    }

    /// Returns the full 32-bit XRGB framebuffer word recorded at the illegal pixel.
    pub const fn word(self) -> u32 {
        self.word
    }

    /// Returns the low 24-bit RGB payload of the illegal post-overlay game pixel.
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
    /// Returns the total number of game framebuffer pixels inspected by this palette audit.
    pub const fn pixel_count(self) -> u64 {
        self.pixel_count
    }

    /// Returns the count of game framebuffer pixels outside the declared final RGB palette.
    pub const fn illegal_rgb_count(self) -> u64 {
        self.illegal_rgb_count
    }

    /// Returns the first out-of-palette pixel witness encountered during the framebuffer scan.
    pub const fn first_illegal(self) -> Option<IllegalFinalGameColor> {
        self.first_illegal
    }

    /// Returns `true` when every inspected game pixel belongs to the declared final RGB palette.
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
    /// Allocates a software voxel renderer sized to `width` by `height` physical pixels.
    pub fn new(width: u32, height: u32) -> Self {
        let mut renderer = Self {
            width: 0,
            height: 0,
            pixels: Vec::new(),
            depth: Vec::new(),
            world_pixels: 0,
            keyed: Vec::new(),
            order: Vec::new(),
            span_inverse: Vec::new(),
            span_depth: Vec::new(),
            occlusion_bound: f64::NEG_INFINITY,
            open_rows: Vec::new(),
            open_top: 0,
            tile_max: Vec::new(),
            tiles_wide: 0,
            coarse_max: Vec::new(),
            coarse_wide: 0,
            dirty_rows: Vec::new(),
            all_dirty: true,
            prev_dirty_rows: Vec::new(),
            prev_dirty_valid: false,
            delta_base: None,
            crosshair_on: false,
            owner: Vec::new(),
            owner_key: 0,
            delta_index: Vec::new(),
            delta_full_len: 0,
            delta_traversal_len: 0,
            delta_slack: 4096,
            delta_compactions: 0,
            recording: false,
            marking: false,
            recolour_hidden: IndexedBits::default(),
            recolour_marks: Vec::new(),
            tie: Vec::new(),
            marks_only: false,
            delta_redraw: false,
            delta_written: Vec::new(),
            delta_recolours: 0,
            #[cfg(test)]
            recolour_marked: 0,
            #[cfg(test)]
            recolour_reused: 0,
            #[cfg(test)]
            recolour_kept: 0,
            #[cfg(test)]
            toggle_skipped: 0,
            #[cfg(test)]
            toggle_proved: 0,
            delta_index_ok: false,
            delta_members: IndexedBits::default(),
            toggled_near: IndexedBits::default(),
            delta_members_ok: false,
            delta_tile_start: Vec::new(),
            delta_tile_items: Vec::new(),
            delta_csr_built: false,
            delta_lists_eager: false,
            delta_seen: Vec::new(),
            delta_tile_seen: Vec::new(),
            delta_stamp: 0,
            delta_mask: Vec::new(),
            delta_pixels: Vec::new(),
            delta_scratch: DeltaScratch::default(),
            delta_frames: 0,
            frame_serial: 0,
            reference_traversal: false,
            #[cfg(test)]
            hidden_bits: 0,
            #[cfg(test)]
            verify_hidden_bits: false,
            #[cfg(test)]
            skip_coarse_cover: false,
            #[cfg(test)]
            skip_enclosed_cull: false,
            #[cfg(test)]
            enclosed_bits: 0,
            #[cfg(test)]
            enclosed_skipped: Vec::new(),
            #[cfg(test)]
            verify_enclosed_bits: false,
            #[cfg(test)]
            coarse_hidden: 0,
        };
        renderer.resize(width, height);
        renderer
    }

    /// Resizes the renderer pixel and depth buffers to `width` by `height` physical pixels.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        let len = (width as usize)
            .checked_mul(height as usize)
            .expect("framebuffer dimensions overflow usize");
        self.pixels.resize(len, BACKGROUND);
        self.depth.resize(len, f64::INFINITY);
        self.owner.resize(len, 0);
        self.tie.resize(len, false);
        self.delta_mask.clear();
        self.delta_mask.resize(len, false);
        self.all_dirty = true;
        self.delta_base = None;
        self.recolour_hidden.clear();
    }

    /// Restores every pixel to `BACKGROUND` and every depth to infinity: the whole buffers
    /// after a resize or whole-frame fill, otherwise only the recorded dirty ranges (every
    /// pixel outside them already holds exactly those values).
    fn clear_frame(&mut self) {
        let width = self.width as usize;
        let height = self.height as usize;
        if self.all_dirty || self.dirty_rows.len() != height {
            self.pixels.fill(BACKGROUND);
            self.depth.fill(f64::INFINITY);
            self.dirty_rows.clear();
            self.dirty_rows.resize(height, (u32::MAX, 0));
            self.all_dirty = false;
            self.prev_dirty_valid = false;
            return;
        }
        self.prev_dirty_rows.clear();
        self.prev_dirty_rows.extend_from_slice(&self.dirty_rows);
        self.prev_dirty_valid = true;
        for (y, range) in self.dirty_rows.iter_mut().enumerate() {
            let (lo, hi) = *range;
            if lo < hi {
                let row = y * width;
                self.pixels[row + lo as usize..row + hi as usize].fill(BACKGROUND);
                self.depth[row + lo as usize..row + hi as usize].fill(f64::INFINITY);
                *range = (u32::MAX, 0);
            }
        }
    }

    /// Serial number of the frame now in the buffers (counts `render` calls).
    pub const fn frame_serial(&self) -> u64 {
        self.frame_serial
    }

    /// Per pixel row, a half-open column range outside which the current frame (including any
    /// crosshair drawn since) is identical to frame `frame_serial() - 1`; `lo >= hi` means the
    /// row is identical. `None` when no such bound is known (first frame, resize, or a
    /// whole-frame fill in either frame), in which case every pixel may differ.
    pub fn changed_rows(&self) -> Option<impl Iterator<Item = (usize, usize)> + '_> {
        if self.all_dirty || !self.prev_dirty_valid || self.dirty_rows.len() != self.height as usize
        {
            return None;
        }
        Some(
            self.dirty_rows
                .iter()
                .zip(&self.prev_dirty_rows)
                .map(|(&now, &before)| {
                    let lo = now.0.min(before.0) as usize;
                    let hi = now.1.max(before.1) as usize;
                    (lo, hi)
                }),
        )
    }

    /// Records that columns `[lo, hi)` of row `y` may have been written.
    #[inline]
    fn mark_dirty(&mut self, y: usize, lo: usize, hi: usize) {
        if let Some(range) = self.dirty_rows.get_mut(y) {
            range.0 = range.0.min(lo as u32);
            range.1 = range.1.max(hi as u32);
        } else {
            self.all_dirty = true;
        }
    }

    /// Returns the current game framebuffer width in physical pixels.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Returns the current game framebuffer height in physical pixels.
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Returns the row-major slice of 32-bit XRGB game framebuffer pixels.
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

    /// [`Self::observe_final_game_frame`] for a tinted frame: an RGB (or its crosshair inversion)
    /// is also legal when `extra_legal` accepts it. Untinted frames should use the strict form.
    pub fn observe_final_game_frame_tinted(
        &self,
        extra_legal: &dyn Fn(u32) -> bool,
    ) -> (FrameObservation, FinalGamePaletteAudit) {
        let frame = FrameRef::new(self.width, self.height, &self.pixels)
            .expect("renderer must retain a dimension-consistent game framebuffer");
        let mut palette = FinalGamePaletteAudit::default();
        let observation = frame.observe_with(|index, word| {
            let rgb = word & RGB_MASK;
            if extra_legal(rgb) || extra_legal(rgb ^ RGB_MASK) {
                palette.pixel_count += 1;
            } else {
                palette.inspect(index, word);
            }
        });
        (observation, palette)
    }

    /// Delta index compactions by [`Self::render_tinted_delta`] so far (diagnostics only).
    pub const fn delta_compactions(&self) -> u64 {
        self.delta_compactions
    }

    /// Frames drawn as deltas by [`Self::render_tinted_delta`] so far (diagnostics only).
    pub const fn delta_frames(&self) -> u64 {
        self.delta_frames
    }

    /// Number of non-background pixels written by the world pass, before overlays.
    pub const fn world_pixel_count(&self) -> u64 {
        self.world_pixels
    }

    /// Enables or disables unoptimized reference voxel traversal without frustum or occlusion culling.
    pub fn set_reference_traversal(&mut self, enabled: bool) {
        self.reference_traversal = enabled;
    }

    /// Returns `true` if the renderer is configured to use unoptimized reference voxel traversal.
    pub const fn is_reference_traversal(&self) -> bool {
        self.reference_traversal
    }

    /// Projects and rasterizes all visible occupied voxels of `world` from `camera` into the framebuffer.
    pub fn render(&mut self, world: &World, camera: &Camera) {
        self.render_tinted(world, camera, None);
    }

    /// [`Self::render`] with an optional per-bit base colour override ([`BitTint`]).
    /// With `None` the frame is exactly what [`Self::render`] produces.
    pub fn render_tinted(&mut self, world: &World, camera: &Camera, tint: Option<BitTint<'_>>) {
        self.recolour_hidden.clear();
        let previous = self.delta_base.take();
        self.delta_lists_eager = false;
        self.crosshair_on = false;
        self.clear_frame();
        self.frame_serial += 1;
        self.world_pixels = 0;
        #[cfg(test)]
        {
            self.hidden_bits = 0;
            self.coarse_hidden = 0;
            self.enclosed_bits = 0;
            self.enclosed_skipped.clear();
        }

        if self.width == 0 || self.height == 0 || world.is_empty() {
            return;
        }

        if let Some(camera_coord) = coord_containing(camera.position())
            && world.is_on(camera_coord)
        {
            let color = tint
                .and_then(|t| t(camera_coord))
                .map_or_else(|| bit_pixel_color(camera_coord), |c| c & RGB_MASK);
            self.pixels.fill(color);
            self.depth.fill(0.0);
            self.all_dirty = true;
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
                self.draw_bit_with(projection, coord, tint);
            }
            return;
        }

        // Nearest first. The depth buffer then holds the closest surfaces early, which is what
        // makes the hidden-bit test in `draw_bit` succeed often enough to pay for itself.
        // Traversal order is free to change: `plot` resolves equal depths by the lower RGB word,
        // so the frame is a per-pixel minimum over the bits and does not depend on their order.
        self.tiles_wide = (self.width as usize).div_ceil(COVER_TILE);
        let tiles = self.tiles_wide * (self.height as usize).div_ceil(COVER_TILE);
        self.tile_max.clear();
        self.tile_max.resize(tiles, f64::INFINITY);
        self.coarse_wide = self.tiles_wide.div_ceil(COVER_COARSE);
        let tiles_high = (self.height as usize).div_ceil(COVER_TILE);
        self.coarse_max.clear();
        self.coarse_max.resize(
            self.coarse_wide * tiles_high.div_ceil(COVER_COARSE),
            f64::INFINITY,
        );

        self.delta_index.clear();
        self.delta_members_ok = false;
        self.delta_index_ok =
            self.width <= u32::from(u16::MAX) && self.height <= u32::from(u16::MAX);
        self.recording = true;
        self.traverse(world, projection, tint);
        self.recording = false;
        self.delta_full_len = self.delta_index.len();
        self.delta_traversal_len = self.delta_full_len;
        self.delta_csr_built = false;
        if self.delta_index_ok {
            self.delta_base = Some(projection);
            self.delta_lists_eager = previous.is_none_or(|p| p.bit_identical(&projection));
        }
    }

    /// §6s: builds the delta tile lists right after a full traversal whose projection is the
    /// one of the frame before it (or that had no ordinary frame before it), so the first
    /// delta frame after a still-camera full render does not pay for them on top of its own
    /// redraw. A moving camera (a new projection per full frame) keeps building them lazily,
    /// on the first delta frame. The lists are the same either way (built from the same index
    /// entries), so no frame changes; only which frame pays for them.
    ///
    /// §6x: the set of indexed bits (`delta_members`) that the first toggling delta frame
    /// needs is built here too, under the same rule. Only a full traversal and a toggling
    /// delta frame add index entries, and compaction rebuilds the set, so it is the same set
    /// whichever frame builds it.
    pub fn prepare_delta_lists(&mut self) {
        if std::mem::take(&mut self.delta_lists_eager) && self.delta_base.is_some() {
            self.build_delta_csr();
            self.build_delta_members();
        }
    }

    /// The bits holding an entry in `delta_index`, when not yet built since the last full
    /// traversal.
    fn build_delta_members(&mut self) {
        if !self.delta_members_ok {
            let mut members = std::mem::take(&mut self.delta_members);
            members.clear();
            for e in &self.delta_index {
                members.insert(e.coord);
            }
            self.delta_members = members;
            self.delta_members_ok = true;
        }
    }

    /// Whether the delta tile lists are built for the frame now in the buffers (tests and
    /// diagnostics).
    pub const fn delta_lists_built(&self) -> bool {
        self.delta_csr_built
    }

    /// The ordered traversal of `render_tinted`: every bit the chunk frustum and enclosed-bit
    /// culls keep, drawn nearest first.
    fn traverse(&mut self, world: &World, projection: CameraProjection, tint: Option<BitTint<'_>>) {
        let mut keyed = std::mem::take(&mut self.keyed);
        keyed.clear();
        let mut nearest = f64::INFINITY;
        let mut farthest = f64::NEG_INFINITY;
        for (chunk_coord, _count, chunk_bounds, words) in world.iter_chunks() {
            let (chunk_min, chunk_max) = bounds_aabb(chunk_bounds);
            if projection.aabb_definitely_outside_frustum(chunk_min, chunk_max) {
                #[cfg(test)]
                if self.verify_hidden_bits {
                    self.verify_hidden_bits = false;
                    let (pixels, depth, world_pixels, hidden_bits) = (
                        self.pixels.clone(),
                        self.depth.clone(),
                        self.world_pixels,
                        self.hidden_bits,
                    );
                    for coord in crate::world::World::iter_chunk_coords(chunk_coord, words) {
                        self.draw_bit(projection, coord);
                    }
                    assert_eq!(
                        self.pixels, pixels,
                        "culled chunk {chunk_coord:?} would have written a pixel"
                    );
                    let word = self
                        .depth
                        .iter()
                        .zip(&depth)
                        .position(|(after, before)| after.to_bits() != before.to_bits());
                    assert_eq!(
                        word, None,
                        "culled chunk {chunk_coord:?} would have written a depth"
                    );
                    assert_eq!(self.world_pixels, world_pixels);
                    self.hidden_bits = hidden_bits;
                    self.verify_hidden_bits = true;
                }
                continue;
            }
            // The enclosed-bit cull. A bit whose whole 3 x 3 x 3 neighbourhood is on can never
            // own a pixel of the finished frame: every point of its surface, edges and corners
            // included, lies at least one unit inside that block, so the ray to it crosses the
            // block's boundary -- a front face of some neighbour -- strictly and well nearer.
            // (Six face neighbours are not enough: an edge of such a bit can lie on a concave
            // crease, where an exact depth tie lets the lower-RGB rule pick its face.) Leaving
            // it out of the traversal therefore changes nothing, and the work then scales with
            // the surface of a solid rather than its volume. `enclosed_cull_applies` limits it
            // to chunks where that argument holds for the rasteriser as written: every bit and
            // neighbour fully in front of the camera (no face dropped for a vertex behind it)
            // and none small enough for the single-pixel path (which plots the nearest vertex
            // depth at the centre pixel, where a neighbour need not land). Tests rasterise every
            // culled bit over the finished frame.
            let exposed;
            let words = if self.enclosed_cull_enabled()
                && enclosed_cull_applies(projection, chunk_bounds)
            {
                exposed = world.chunk_open_words(chunk_coord);
                #[cfg(test)]
                for (word, (&all, &kept)) in words.iter().zip(&exposed).enumerate() {
                    let culled = all & !kept;
                    self.enclosed_bits += u64::from(culled.count_ones());
                    if (self.verify_hidden_bits || self.verify_enclosed_bits) && culled != 0 {
                        let mut only = [0_u64; crate::world::CHUNK_WORDS];
                        only[word] = culled;
                        self.enclosed_skipped
                            .extend(crate::world::World::iter_chunk_coords(chunk_coord, &only));
                    }
                }
                &exposed
            } else {
                words
            };
            for coord in crate::world::World::iter_chunk_coords(chunk_coord, words) {
                let key = projection.view_depth(coord_center(coord));
                nearest = f64::min(nearest, key);
                farthest = f64::max(farthest, key);
                keyed.push((key, coord));
            }
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
            self.draw_bit_with(projection, coord, tint);
        }
        self.keyed = keyed;
        self.order = order;
        #[cfg(test)]
        self.assert_enclosed_bits_write_nothing(projection, tint);
    }

    /// [`Self::render_tinted`] for a frame that differs from the one now in the buffers only at
    /// a few bits, redrawing just what those bits can affect. Returns `false` (having rendered
    /// the whole frame with `render_tinted` instead) when the buffers do not hold a complete
    /// ordinary frame from the same camera projection, or the change is too large.
    ///
    /// The caller guarantees that the previous render (full or delta) drew a world and tint
    /// that agree with `world` and `tint` at every bit except those in `changed`, that the flag
    /// beside each is `true` when its occupancy may have changed (not only its tint), and that
    /// no face colour either tint produces is `BACKGROUND`, so `world_pixels` (counted on first
    /// write) is the number of non-background pixels for every traversal order.
    ///
    /// Why the frame is bit-identical to `render_tinted` (pixels, depths, `world_pixels`):
    /// that frame is, per pixel, the minimum by depth then RGB word over the fragments of every
    /// on bit; the culls and the hidden-bit test only leave out bits that own no pixel. Write
    /// `U` for the unchanged bits and `C` for the changed ones. At a pixel the old frame gave to
    /// a bit of `U`, the old value is already `U`'s minimum there, so the new value is its
    /// minimum with the new fragments of `C`: drawing the changed bits over the old frame
    /// produces it. Every other pixel (held by a bit of `C`, per the owner buffer) is reset to
    /// the background and every bit whose grown raster box reaches one is drawn again. Those
    /// bits come from the full traversal's box index plus the bits added since: a bit missing
    /// from both is deeply enclosed where the enclosed-bit cull applies (owns no pixel) or was
    /// never reachable, and the only bits that can stop being enclosed are the on neighbours
    /// of a bit that turned off, which are drawn and indexed here when the index lacks them.
    /// Changed bits and those neighbours that are enclosed in the new world own no pixel of it
    /// and are skipped.
    pub fn render_tinted_delta(
        &mut self,
        world: &World,
        camera: &Camera,
        tint: Option<BitTint<'_>>,
        changed: &[(Coord, bool)],
    ) -> bool {
        let Some(projection) = self.delta_projection(world, camera) else {
            self.render_tinted(world, camera, tint);
            return false;
        };
        if self.delta_index.len() > self.delta_full_len * 3 / 2 + self.delta_slack {
            // Fold the entries added since into the tile lists instead of redrawing the whole
            // frame; only when that leaves the index well above a full traversal's does the
            // full path rebuild it.
            self.compact_delta_index(world);
            if self.delta_index.len() > self.delta_traversal_len * 3 / 2 + self.delta_slack {
                self.render_tinted(world, camera, tint);
                return false;
            }
        }
        // Classify the changed bits by the new world's occupancy and deep enclosure (the
        // enclosed-bit cull's own test, per chunk). A bit is `Quiet` when it owns no pixel in
        // either frame: off in both, or deeply enclosed (with the cull's geometry) in both,
        // which holds when it is enclosed now and no bit of its 3 x 3 x 3 block toggled.
        let tint_only = changed.iter().all(|&(_, toggled)| !toggled);
        // §6x: a frame that toggles keeps the recolour proofs it cannot disturb (see
        // `drop_proofs_over`, after the reset below) instead of discarding them all.
        // The bits within one step of a toggled bit, as per-chunk bit words (no hashing per
        // neighbour; the toggled bits and the changed bits both arrive in chunk runs).
        let mut toggled_near = std::mem::take(&mut self.toggled_near);
        toggled_near.clear();
        if !tint_only {
            for &(c, toggled) in changed {
                if toggled {
                    for n in neighbourhood27(c) {
                        toggled_near.insert(n);
                    }
                }
            }
        }
        // Per chunk, the bits that count as exposed: its open words where the enclosed-bit
        // cull applies to it, else its on bits (open words are a subset of the on bits). The
        // words of every chunk met are built once and kept by index, and the slots of recently
        // met chunks sit in a small direct-mapped cache keyed by the chunk's low coordinate
        // bits, so most lookups hash nothing (the changed bits of a busy tick jump between
        // nearby chunks rather than arriving in one run per chunk).
        let mut scratch = std::mem::take(&mut self.delta_scratch);
        let DeltaScratch {
            open_words,
            open_cache,
            loud,
        } = &mut scratch;
        open_words.clear();
        open_cache.clear();
        loud.clear();
        let mut recent: [Option<(crate::world::ChunkCoord, usize)>; RECENT_WAYS] =
            [None; RECENT_WAYS];
        let cull_enabled = self.enclosed_cull_enabled();
        // §6w: on a tint-only frame a bit already proved strictly hidden (`recolour_hidden`)
        // marks and writes nothing, in the recolour and in any redraw that reaches it (the
        // depths it lost to are unchanged), so it is dropped with the enclosed bits: each
        // chunk's words are its exposed bits less its proved ones.
        let hidden = std::mem::take(&mut self.recolour_hidden);
        let drop_hidden = tint_only;
        let mut exposed = |coord: Coord, world: &World| -> bool {
            let (chunk, index) = World::chunk_of(coord);
            let way = recent_way(chunk);
            let slot = match recent[way] {
                Some((c, slot)) if c == chunk => slot,
                _ => {
                    let slot = *open_cache.entry(chunk).or_insert_with(|| {
                        let applies = cull_enabled
                            && world
                                .chunk_bounds(chunk)
                                .is_some_and(|b| enclosed_cull_applies(projection, b));
                        let mut words = if applies {
                            world.chunk_open_words(chunk)
                        } else {
                            world
                                .chunk_words(chunk)
                                .map_or([0; crate::world::CHUNK_WORDS], |(_, bits)| *bits)
                        };
                        if drop_hidden && let Some(proved) = hidden.chunk_words(chunk) {
                            for (w, p) in words.iter_mut().zip(proved) {
                                *w &= !p;
                            }
                        }
                        open_words.push(words);
                        open_words.len() - 1
                    });
                    recent[way] = Some((chunk, slot));
                    slot
                }
            };
            open_words[slot][index / 64] >> (index % 64) & 1 == 1
        };
        #[cfg(test)]
        let mut proved_skips: Vec<Coord> = Vec::new();
        for &(coord, toggled) in changed {
            // §6x: a toggling frame drops a changed bit proved strictly hidden that no toggle
            // is near: it holds no pixel, its proof still stands (its box meets no reset pixel
            // yet, and a reset it meets drops it below), and a reset pixel it may now win
            // brings it back as a redraw candidate through the index, like any unchanged bit.
            let near = toggled || (!tint_only && toggled_near.contains(coord));
            if !tint_only && !near && {
                let (chunk, index) = World::chunk_of(coord);
                hidden
                    .chunk_words(chunk)
                    .is_some_and(|w| w[index / 64] >> (index % 64) & 1 == 1)
            } {
                #[cfg(test)]
                {
                    proved_skips.push(coord);
                    self.toggle_skipped += 1;
                }
                continue;
            }
            let open_now = exposed(coord, world);
            if !near && !open_now {
                #[cfg(test)]
                if drop_hidden {
                    let (chunk, index) = World::chunk_of(coord);
                    if hidden
                        .chunk_words(chunk)
                        .is_some_and(|w| w[index / 64] >> (index % 64) & 1 == 1)
                    {
                        proved_skips.push(coord);
                    }
                }
                continue; // off in both, enclosed in both, or (tint-only) proved hidden
            }
            // A tint-only frame reads a loud bit's grown box only when its recolour marks a
            // pixel, so the box is built there, on demand, when every vertex is safely in front
            // of the camera (then each vertex projects: the result below cannot be `Err`).
            let entry = if tint_only
                && projection.view_depth(coord_center(coord)) > DEFERRED_BOX_MIN_DEPTH
            {
                None
            } else {
                match self.bit_grown_box(projection, coord) {
                    Ok(entry) => entry,
                    Err(()) => {
                        self.delta_scratch = scratch;
                        self.recolour_hidden = hidden;
                        self.render_tinted(world, camera, tint);
                        return false;
                    }
                }
            };
            loud.push((coord, toggled, open_now, entry));
            if loud.len() > self.delta_full_len / 2 + 4096 {
                self.delta_scratch = scratch;
                self.recolour_hidden = hidden;
                self.render_tinted(world, camera, tint);
                return false;
            }
        }
        // §6z: only the neighbours missing from the index are drawn and recorded. An indexed
        // neighbour is unchanged and was on in the frame before, so outside the reset pixels
        // the buffers already hold its fragments' minimum (a redraw there writes and marks
        // nothing); at a reset pixel its entry, which depends only on the bit and this
        // projection, brings it in as a redraw candidate. Filtering before the sort leaves the
        // same sorted list as filtering after it.
        self.build_delta_members();
        let members = &mut self.delta_members;
        let mut neighbours: Vec<Coord> = changed
            .iter()
            .filter(|&&(c, toggled)| toggled && !world.is_on(c))
            .flat_map(|&(c, _)| neighbourhood27(c))
            .filter(|&c| !members.contains(c))
            .collect();
        neighbours.sort_unstable();
        neighbours.dedup();
        neighbours.retain(|&c| exposed(c, world));
        self.toggled_near = toggled_near;
        self.recolour_hidden = hidden;
        // Each proved bit the classification dropped still writes nothing (the recolour's
        // check of a proved bit, run here on the unchanged buffers).
        #[cfg(test)]
        for coord in proved_skips {
            self.marking = true;
            self.draw_bit_with(projection, coord, tint);
            self.marking = false;
        }

        if self.crosshair_on {
            self.invert_crosshair();
            self.crosshair_on = false;
        }
        self.frame_serial += 1;
        #[cfg(test)]
        {
            self.hidden_bits = 0;
            self.coarse_hidden = 0;
            self.enclosed_bits = 0;
            self.enclosed_skipped.clear();
        }
        self.build_delta_csr();
        self.delta_stamp = self.delta_stamp.wrapping_add(1);
        if self.delta_stamp == 0 {
            self.delta_seen.fill(0);
            self.delta_tile_seen.fill(0);
            self.delta_stamp = 1;
        }
        let stamp = self.delta_stamp;

        if tint_only {
            self.recolour_delta(world, projection, tint, loud, stamp);
            self.delta_scratch = scratch;
            self.delta_base = self.delta_index_ok.then_some(projection);
            self.delta_frames += 1;
            self.delta_recolours += 1;
            return true;
        }

        // 1. Reset every pixel a changed bit holds.
        let width = self.width as usize;
        self.prev_dirty_rows.clear();
        self.prev_dirty_rows
            .resize(self.height as usize, (u32::MAX, 0));
        self.prev_dirty_valid = true;
        let mut reset = std::mem::take(&mut self.delta_pixels);
        reset.clear();
        for &(coord, _, _, entry) in loud.iter() {
            let Some(entry) = entry else { continue };
            let key = owner_key(coord);
            for y in usize::from(entry.y0)..usize::from(entry.y1) {
                for x in usize::from(entry.x0)..usize::from(entry.x1) {
                    let index = y * width + x;
                    if self.owner[index] == key && !self.delta_mask[index] {
                        self.delta_mask[index] = true;
                        reset.push(index);
                    }
                }
            }
        }
        for &index in &reset {
            let (x, y) = (index % width, index / width);
            if self.pixels[index] != BACKGROUND {
                self.world_pixels -= 1;
            }
            self.pixels[index] = BACKGROUND;
            self.depth[index] = f64::INFINITY;
            let tile = (y / COVER_TILE) * self.tiles_wide + x / COVER_TILE;
            self.tile_max[tile] = f64::INFINITY;
            let block =
                (y / COVER_TILE / COVER_COARSE) * self.coarse_wide + x / COVER_TILE / COVER_COARSE;
            self.coarse_max[block] = f64::INFINITY;
            let range = &mut self.prev_dirty_rows[y];
            range.0 = range.0.min(x as u32);
            range.1 = range.1.max(x as u32 + 1);
        }

        self.drop_proofs_over(projection, &reset);

        // 2. Bits to draw: indexed bits reaching a reset pixel ...
        let mut redraw: Vec<(f64, Coord)> = Vec::new();
        self.collect_delta_candidates(world, projection, &reset, stamp, &mut redraw);
        // ... the changed bits, and the on neighbours of every bit that turned off; less those
        // the new world deeply encloses where the enclosed-bit cull applies. Bits that turned
        // on and those neighbours may be missing from the index, so they are recorded into it.
        let mut fresh: Vec<Coord> = neighbours;
        for &(coord, toggled, open_now, _) in loud.iter() {
            if !open_now {
                continue;
            }
            if toggled {
                fresh.push(coord);
            } else {
                redraw.push((projection.view_depth(coord_center(coord)), coord));
            }
        }

        dedup_draws(&mut redraw, &mut fresh);

        // 3. Draw, nearest first (any order gives the same frame).
        self.delta_redraw = true;
        for &(_, coord) in &redraw {
            self.draw_bit_with(projection, coord, tint);
        }
        self.build_delta_members();
        // A bit already indexed is not recorded again: its entry would repeat the one held
        // (an entry depends only on its bit and this projection), and compaction keeps only a
        // bit's first entry anyway. A delta frame reads entries only for on bits and draws a bit
        // that two entries reach twice to the same effect as once.
        self.recording = true;
        for &coord in &fresh {
            self.draw_bit_with(projection, coord, tint);
        }
        self.recording = false;
        self.delta_redraw = false;
        for index in self.delta_written.drain(..) {
            self.delta_mask[index] = false;
        }
        reset
            .iter()
            .for_each(|&index| self.delta_mask[index] = false);
        self.delta_pixels = reset;
        self.delta_base = self.delta_index_ok.then_some(projection);
        self.delta_frames += 1;
        self.delta_scratch = scratch;
        true
    }

    /// §6x: drops the recolour proofs a toggling delta frame may break, keeping the rest. A
    /// proof says every pixel of the bit's box holds a depth strictly nearer than any the bit
    /// can draw (same projection: delta frames keep it). Such a frame raises depths only at
    /// the pixels it resets (step 1); everywhere else it only draws, and a draw keeps the
    /// lesser depth. So a proof stays true unless its box meets a reset pixel. Proofs are kept
    /// per chunk: a chunk whose whole box (every bit's cube) lies in front of the camera and
    /// projects, with two pixels of margin for rounding, clear of the reset pixels' bounding
    /// rectangle keeps its proofs (each bit's box lies in the hull of the chunk's corners);
    /// every other chunk's proofs are dropped, which only sends those bits back to be drawn.
    fn drop_proofs_over(&mut self, projection: CameraProjection, reset: &[usize]) {
        if reset.is_empty() || self.recolour_hidden.words.is_empty() {
            return;
        }
        let width = self.width as usize;
        let (mut x0, mut x1, mut y0, mut y1) = (usize::MAX, 0, usize::MAX, 0);
        for &index in reset {
            let (x, y) = (index % width, index / width);
            (x0, x1, y0, y1) = (x0.min(x), x1.max(x), y0.min(y), y1.max(y));
        }
        let (x0, x1, y0, y1) = (x0 as f64, x1 as f64 + 1.0, y0 as f64, y1 as f64 + 1.0);
        let side = crate::world::CHUNK_SIDE as f64;
        self.recolour_hidden.retain_chunks(|chunk| {
            let lo = Vec3::new(
                chunk.x as f64 * side - 0.5,
                chunk.y as f64 * side - 0.5,
                chunk.z as f64 * side - 0.5,
            );
            let hi = lo + Vec3::splat(side);
            let (mut l, mut r, mut t, mut b) = (
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
            );
            for corner in crate::camera::bounds_corners(lo, hi) {
                let Some(p) = projection.project(corner) else {
                    return false;
                };
                (l, r, t, b) = (l.min(p.x), r.max(p.x), t.min(p.y), b.max(p.y));
            }
            let (l, r, t, b) = (
                l.floor() - 2.0,
                r.ceil() + 2.0,
                t.floor() - 2.0,
                b.ceil() + 2.0,
            );
            if !(l.is_finite() && r.is_finite() && t.is_finite() && b.is_finite()) {
                return false;
            }
            r <= x0 || l >= x1 || b <= y0 || t >= y1
        });
        #[cfg(test)]
        {
            self.recolour_kept += self
                .recolour_hidden
                .words
                .iter()
                .flatten()
                .map(|w| u64::from(w.count_ones()))
                .sum::<u64>();
        }
    }

    /// The projection a delta frame may reuse the buffers under, or `None` when it must not
    /// (no complete ordinary frame, another projection, or a frame `render_tinted` would not
    /// draw by the ordered traversal).
    fn delta_projection(&self, world: &World, camera: &Camera) -> Option<CameraProjection> {
        let base = self.delta_base?;
        if self.width == 0
            || self.height == 0
            || world.is_empty()
            || self.all_dirty
            || !self.optimized_traversal()
            || self.dirty_rows.len() != self.height as usize
        {
            return None;
        }
        if coord_containing(camera.position()).is_some_and(|c| world.is_on(c)) {
            return None;
        }
        let projection = camera.projection_for_viewport(self.width, self.height)?;
        if !projection.bit_identical(&base) {
            return None;
        }
        if let Some(bounds) = world.bounds() {
            let (min, max) = bounds_aabb(bounds);
            if projection.bounds_fit_one_pixel(min, max) {
                return None;
            }
        }
        Some(projection)
    }

    /// The clamped pixel box `[floor(min) - 1, ceil(max) + 1)` of a bit whose projected
    /// vertices span `[min, max]`, or `None` when it holds no viewport pixel. Every write the
    /// bit makes lands inside: each face's raster box is built from a subset of those vertices
    /// by the same `floor` / `ceil` / clamp, and the single-pixel path plots the floor of its
    /// centre's projection, which lies in their hull; the one-pixel growth absorbs rounding.
    /// A bit's grown box from the projections of its cube's vertices; `Err` when a vertex
    /// does not project (behind the camera), `Ok(None)` when the box covers no pixel.
    fn bit_grown_box(
        &self,
        projection: CameraProjection,
        coord: Coord,
    ) -> Result<Option<DeltaEntry>, ()> {
        let mut extent = (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        );
        for vertex in cube_vertices(coord_center(coord)) {
            let p = projection.project(vertex).ok_or(())?;
            extent = (
                extent.0.min(p.x),
                extent.1.max(p.x),
                extent.2.min(p.y),
                extent.3.max(p.y),
            );
        }
        Ok(self.grown_box(coord, extent.0, extent.1, extent.2, extent.3))
    }

    fn grown_box(
        &self,
        coord: Coord,
        min_x: f64,
        max_x: f64,
        min_y: f64,
        max_y: f64,
    ) -> Option<DeltaEntry> {
        let (w, h) = (f64::from(self.width), f64::from(self.height));
        let left = (min_x.floor() - 1.0).max(0.0);
        let right = (max_x.ceil() + 1.0).min(w);
        let top = (min_y.floor() - 1.0).max(0.0);
        let bottom = (max_y.ceil() + 1.0).min(h);
        // `!(a < b)` also rejects NaN.
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        if !(left < right && top < bottom) {
            return None;
        }
        Some(DeltaEntry {
            coord,
            x0: left as u16,
            x1: right as u16,
            y0: top as u16,
            y1: bottom as u16,
        })
    }

    /// The rest of [`Self::render_tinted_delta`] when no changed bit toggled, only recoloured.
    ///
    /// Why the frame is still bit-identical: every bit has the geometry it had, so a bit draws
    /// exactly the fragments (pixel, depth) it drew before, now with its new colours, and a
    /// changed bit that is not drawn (`loud` holds every changed bit exposed in the new world)
    /// is deeply enclosed in both worlds and owns no pixel of either. Each pixel must end as the
    /// minimum by depth then RGB word of its fragments, and no depth changes: nothing lies
    /// nearer than a pixel's stored depth, so only the fragments at exactly that depth compete.
    ///
    /// 1. Draw every loud bit over the old frame in marking mode: an ordinary `plot` (keep the
    ///    lesser), except where a fragment meets the stored depth of a pixel its own bit holds
    ///    (owner buffer). There it replaces the stored colour, which was this bit's old one, and
    ///    when the new colour is higher and `tie` admits another fragment at that depth, marks
    ///    the pixel. At an unmarked pixel the result is the minimum: if a changed bit held it,
    ///    that bit drew there again at the same depth (the culls and the hidden-bit test skip
    ///    only fragments strictly behind what is stored), and its new fragment is either no
    ///    higher than the old minimum, so still below every other fragment, or the only one at
    ///    that depth; the other changed bits' new fragments meet it through ordinary `plot`.
    ///    A pixel an unchanged bit holds keeps that fragment, the least of the unchanged ones,
    ///    and `plot` keeps the least of it and the new ones.
    /// 2. At a marked pixel the stored value is a true new fragment, so drawing every bit that
    ///    has a fragment there again with ordinary `plot` leaves the minimum. Those bits are the
    ///    indexed ones reaching the pixel (the index covers every on bit not deeply enclosed, as
    ///    for a reset pixel) plus the loud ones; the rest of the frame already holds its minima,
    ///    so a bit bounded strictly behind every marked pixel of its box is skipped. Depths never
    ///    change and no pixel turns background, so the tile and coarse depth bounds stay valid
    ///    and `world_pixels` stays exact.
    fn recolour_delta(
        &mut self,
        world: &World,
        projection: CameraProjection,
        tint: Option<BitTint<'_>>,
        loud: &[(Coord, bool, bool, Option<DeltaEntry>)],
        stamp: u32,
    ) {
        let width = self.width as usize;
        self.prev_dirty_rows.clear();
        self.prev_dirty_rows
            .resize(self.height as usize, (u32::MAX, 0));
        self.prev_dirty_valid = true;

        let mut marks = std::mem::take(&mut self.delta_pixels);
        marks.clear();
        self.recolour_marks = marks;
        self.marking = true;
        for &(coord, _, open_now, _) in loud {
            if open_now {
                self.draw_bit_with(projection, coord, tint);
            }
        }
        self.marking = false;
        let mut marks = std::mem::take(&mut self.recolour_marks);
        marks.retain(|&index| !std::mem::replace(&mut self.delta_mask[index], true));
        #[cfg(test)]
        {
            self.recolour_marked += marks.len() as u64;
        }

        let mut redraw: Vec<(f64, Coord)> = Vec::new();
        if !marks.is_empty() {
            self.collect_delta_candidates(world, projection, &marks, stamp, &mut redraw);
            for &(coord, _, open_now, entry) in loud {
                // A deferred box (see `render_tinted_delta`) is built now; its vertices all
                // project, so this is the box the classification would have built.
                let entry = match entry {
                    Some(e) => Some(e),
                    None => self.bit_grown_box(projection, coord).unwrap_or(None),
                };
                let Some(e) = entry else { continue };
                let reaches = (usize::from(e.y0)..usize::from(e.y1)).any(|y| {
                    self.delta_mask[y * width + usize::from(e.x0)..y * width + usize::from(e.x1)]
                        .iter()
                        .any(|&m| m)
                });
                if open_now && reaches {
                    redraw.push((projection.view_depth(coord_center(coord)), coord));
                }
            }
        }

        // Nearest first, so the hidden-bit test culls what lies behind. Only the marked pixels
        // can still change (every other pixel already holds its minimum, which any fragment
        // these bits make there cannot beat), so a bit bounded behind all the marked pixels in
        // its box writes nothing and is skipped.
        dedup_draws(&mut redraw, &mut Vec::new());
        self.marks_only = true;
        self.delta_redraw = true;
        for &(_, coord) in &redraw {
            self.draw_bit_with(projection, coord, tint);
        }
        self.delta_redraw = false;
        self.marks_only = false;
        for index in self.delta_written.drain(..) {
            self.delta_mask[index] = false;
        }
        marks
            .iter()
            .for_each(|&index| self.delta_mask[index] = false);
        self.delta_pixels = marks;
    }

    /// Appends to `redraw` (with its view depth) every on bit of the box index whose grown box
    /// reaches a pixel of `reset`, which must be the pixels `delta_mask` marks. Each indexed
    /// entry is visited at most once per `stamp`.
    fn collect_delta_candidates(
        &mut self,
        world: &World,
        projection: CameraProjection,
        reset: &[usize],
        stamp: u32,
        redraw: &mut Vec<(f64, Coord)>,
    ) {
        let width = self.width as usize;
        let hits = |mask: &[bool], e: &DeltaEntry| {
            (usize::from(e.y0)..usize::from(e.y1)).any(|y| {
                mask[y * width + usize::from(e.x0)..y * width + usize::from(e.x1)]
                    .iter()
                    .any(|&m| m)
            })
        };
        for &index in reset {
            let tile =
                (index / width / COVER_TILE) * self.tiles_wide + (index % width) / COVER_TILE;
            if self.delta_tile_seen[tile] == stamp {
                continue;
            }
            self.delta_tile_seen[tile] = stamp;
            let (lo, hi) = (
                self.delta_tile_start[tile] as usize,
                self.delta_tile_start[tile + 1] as usize,
            );
            for k in lo..hi {
                let item = self.delta_tile_items[k] as usize;
                if self.delta_seen[item] == stamp {
                    continue;
                }
                self.delta_seen[item] = stamp;
                let entry = self.delta_index[item];
                if hits(&self.delta_mask, &entry) && world.is_on(entry.coord) {
                    redraw.push((
                        projection.view_depth(coord_center(entry.coord)),
                        entry.coord,
                    ));
                }
            }
        }
        if !reset.is_empty() {
            for entry in &self.delta_index[self.delta_full_len..] {
                if hits(&self.delta_mask, entry) && world.is_on(entry.coord) {
                    redraw.push((
                        projection.view_depth(coord_center(entry.coord)),
                        entry.coord,
                    ));
                }
            }
        }
    }

    /// Drops the index entries of bits that are off in `world` and every repeat of a bit
    /// already indexed (keeping its first entry), then marks the tile lists for a rebuild over
    /// all that remain. The delta frames read the index only for on bits, an entry depends only
    /// on its bit and the projection (`grown_box` of the bit's projected vertices), and a bit
    /// that turns on again is recorded by the frame that sees it toggle, so the index still
    /// holds a box for every on bit the old index did and the frames drawn from it are the same.
    fn compact_delta_index(&mut self, world: &World) {
        let mut probe = world.probe();
        let mut kept = std::mem::take(&mut self.delta_members);
        kept.clear();
        self.delta_index
            .retain(|e| probe.is_on(e.coord) && kept.insert(e.coord));
        // `kept` now holds exactly the bits with an entry.
        self.delta_members = kept;
        self.delta_members_ok = true;
        self.delta_full_len = self.delta_index.len();
        self.delta_csr_built = false;
        self.delta_compactions += 1;
    }

    /// Builds the per-tile lists of the index entries (the full traversal's, or all of them
    /// after `compact_delta_index`), once per full frame or compaction.
    fn build_delta_csr(&mut self) {
        if self.delta_csr_built {
            return;
        }
        let tiles = self.tile_max.len();
        let mut start = std::mem::take(&mut self.delta_tile_start);
        start.clear();
        start.resize(tiles + 1, 0);
        let tile_range = |e: &DeltaEntry| {
            (
                usize::from(e.x0) / COVER_TILE..=(usize::from(e.x1) - 1) / COVER_TILE,
                usize::from(e.y0) / COVER_TILE..=(usize::from(e.y1) - 1) / COVER_TILE,
            )
        };
        let full = &self.delta_index[..self.delta_full_len];
        for e in full {
            let (xs, ys) = tile_range(e);
            for ty in ys {
                for tx in xs.clone() {
                    start[ty * self.tiles_wide + tx + 1] += 1;
                }
            }
        }
        for t in 0..tiles {
            start[t + 1] += start[t];
        }
        let mut items = std::mem::take(&mut self.delta_tile_items);
        items.clear();
        items.resize(start[tiles] as usize, 0);
        let mut fill = start.clone();
        for (i, e) in full.iter().enumerate() {
            let (xs, ys) = tile_range(e);
            for ty in ys {
                for tx in xs.clone() {
                    let slot = &mut fill[ty * self.tiles_wide + tx];
                    items[*slot as usize] = i as u32;
                    *slot += 1;
                }
            }
        }
        self.delta_tile_start = start;
        self.delta_tile_items = items;
        self.delta_seen.clear();
        self.delta_seen.resize(self.delta_full_len, 0);
        self.delta_tile_seen.clear();
        self.delta_tile_seen.resize(tiles, 0);
        self.delta_stamp = 0;
        self.delta_csr_built = true;
    }

    const fn enclosed_cull_enabled(&self) -> bool {
        #[cfg(test)]
        if self.skip_enclosed_cull {
            return false;
        }
        true
    }

    /// Rasterises every bit the enclosed-bit cull left out over the finished frame and requires
    /// that none of them moves a pixel or a depth: each is then not the per-pixel minimum
    /// anywhere, which is exactly the condition for leaving it out to change nothing.
    #[cfg(test)]
    fn assert_enclosed_bits_write_nothing(
        &mut self,
        projection: CameraProjection,
        tint: Option<BitTint<'_>>,
    ) {
        if !(self.verify_hidden_bits || self.verify_enclosed_bits)
            || self.enclosed_skipped.is_empty()
        {
            return;
        }
        let width = self.width as usize;
        let world_pixels = self.world_pixels;
        for coord in std::mem::take(&mut self.enclosed_skipped) {
            let center = coord_center(coord);
            let projected = cube_vertices(center).map(|vertex| projection.project(vertex));
            assert!(
                projected.iter().all(Option::is_some),
                "enclosed bit {coord:?} has a vertex behind the camera"
            );
            let (mut min_x, mut max_x) = (f64::INFINITY, f64::NEG_INFINITY);
            let (mut min_y, mut max_y) = (f64::INFINITY, f64::NEG_INFINITY);
            for p in projected.iter().flatten() {
                (min_x, max_x) = (min_x.min(p.x), max_x.max(p.x));
                (min_y, max_y) = (min_y.min(p.y), max_y.max(p.y));
            }
            assert!(
                max_x - min_x > SMALL_BIT_EXTENT || max_y - min_y > SMALL_BIT_EXTENT,
                "enclosed bit {coord:?} is small enough for the single-pixel path"
            );
            // Every write lands in the bit's clamped raster box (see `draw_bit_with`); one
            // pixel of slack on each side costs nothing and guards the claim.
            let left = (min_x.floor() - 1.0).max(0.0) as usize;
            let right = (max_x.ceil() + 1.0).min(self.width as f64).max(0.0) as usize;
            let top = (min_y.floor() - 1.0).max(0.0) as usize;
            let bottom = (max_y.ceil() + 1.0).min(self.height as f64).max(0.0) as usize;
            let snapshot = |r: &Self| -> Vec<(u32, u64)> {
                (top..bottom)
                    .flat_map(|y| (left..right).map(move |x| y * width + x))
                    .map(|i| (r.pixels[i], r.depth[i].to_bits()))
                    .collect()
            };
            let before = snapshot(self);
            self.draw_faces(projection, coord, center, &projected, tint);
            assert!(
                snapshot(self) == before,
                "enclosed bit {coord:?} would have written a pixel or a depth"
            );
            assert_eq!(self.world_pixels, world_pixels);
        }
    }

    const fn optimized_traversal(&self) -> bool {
        !self.reference_traversal
    }

    /// Draws a one-physical-pixel crosshair without covering the exact aim pixel.
    /// Its arms invert the world color beneath them so they remain visible at every depth.
    pub fn draw_crosshair(&mut self, overlay: OverlayInput) {
        debug_assert_eq!(
            overlay,
            crosshair_overlay_input(),
            "renderer received an unsupported crosshair overlay input"
        );
        self.invert_crosshair();
        self.crosshair_on = !self.crosshair_on;
    }

    /// XORs the crosshair arms (an involution: twice restores the pixels).
    fn invert_crosshair(&mut self) {
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
        self.mark_dirty(y as usize, x as usize, x as usize + 1);
        self.pixels[index] ^= RGB_MASK;
    }

    #[cfg(test)]
    fn draw_bit(&mut self, projection: CameraProjection, coord: Coord) {
        self.draw_bit_with(projection, coord, None);
    }

    fn draw_bit_with(
        &mut self,
        projection: CameraProjection,
        coord: Coord,
        tint: Option<BitTint<'_>>,
    ) {
        // A prior tint-only frame already proved that this bit loses every depth test
        // strictly. Recolour changes no fragment depth or geometry, so it still writes and
        // marks nothing. A projection change or full render clears the proofs first, and a
        // toggling delta frame drops every proof its resets may break (§6x).
        if self.marking && self.recolour_hidden.contains(coord) {
            #[cfg(test)]
            {
                let center = coord_center(coord);
                let projected = cube_vertices(center).map(|v| projection.project(v));
                self.owner_key = owner_key(coord);
                self.recolour_reused += 1;
                self.assert_hidden_bit_writes_nothing(projection, coord, center, &projected, tint);
            }
            return;
        }
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
        self.owner_key = owner_key(coord);
        if self.recording {
            if projected.iter().all(Option::is_some) {
                if let Some(entry) = self.grown_box(coord, min_x, max_x, min_y, max_y)
                    && (!self.delta_members_ok || self.delta_members.insert(coord))
                {
                    self.delta_index.push(entry);
                }
            } else {
                self.delta_index_ok = false;
            }
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
                    tint.and_then(|t| t(coord))
                        .map_or_else(|| bit_pixel_color(coord), |c| c & RGB_MASK),
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
            //
            // The bound is needed first: the coarse level of `covering_depth` answers against it.
            let nearest_possible = nearest_possible_depth(
                projection,
                center,
                &projected,
                nearest_vertex,
                max_x - min_x,
                max_y - min_y,
            );
            if self.marks_only && self.behind_marks(left, right, top, bottom, nearest_possible) {
                return;
            }
            let covering =
                self.covering_depth(left, right, top, bottom, nearest_vertex, nearest_possible);
            let trims =
                max_x - min_x > OCCLUSION_TRIM_EXTENT || max_y - min_y > OCCLUSION_TRIM_EXTENT;
            if let Some(covering) = covering
                && covering < nearest_possible
            {
                if self.marking || (self.delta_redraw && !self.marks_only) {
                    self.recolour_hidden.insert(coord);
                    #[cfg(test)]
                    if !self.marking {
                        self.toggle_proved += 1;
                    }
                }
                #[cfg(test)]
                {
                    self.hidden_bits += 1;
                    self.assert_hidden_bit_writes_nothing(
                        projection, coord, center, &projected, tint,
                    );
                }
                return;
            }
            // The same bound, applied per pixel run by the span rasteriser: a pixel already
            // holding something strictly nearer than anything this bit can write is skipped.
            self.occlusion_bound = if trims {
                nearest_possible
            } else {
                f64::NEG_INFINITY
            };
            if trims {
                self.scan_open_rows(left, right, top, bottom, nearest_possible);
            }
            self.draw_faces(projection, coord, center, &projected, tint);
            self.occlusion_bound = f64::NEG_INFINITY;
            self.open_rows.clear();
            return;
        }

        self.draw_faces(projection, coord, center, &projected, tint);
    }

    fn draw_faces(
        &mut self,
        projection: CameraProjection,
        coord: Coord,
        center: Vec3,
        projected: &[Option<ProjectedPoint>; 8],
        tint: Option<BitTint<'_>>,
    ) {
        // Looked up once per drawn bit (never per pixel or per face).
        let tinted = tint.and_then(|t| t(coord));
        let lit = tinted.map_or(0, |t| (t >> TINT_LIT_SHIFT) & 0x3f);
        let base = tinted.map(|t| t & RGB_MASK);
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
            let color = match base {
                Some(base) if lit & tint_lit_bit(face.normal) != 0 => lit_face_color(base),
                Some(base) => tinted_face_color(base, face.normal),
                None => face_color(coord, face.normal),
            };
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
        tint: Option<BitTint<'_>>,
    ) {
        if !self.verify_hidden_bits {
            return;
        }
        let pixels = self.pixels.clone();
        let depth = self.depth.clone();
        let world_pixels = self.world_pixels;

        self.draw_faces(projection, coord, center, projected, tint);

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

    /// An upper bound on the farthest retained depth over a half-open pixel box, or `None` if
    /// any pixel there is already at or beyond `limit`.
    ///
    /// The caller skips the bit only when the bound is strictly nearer than anything it can
    /// write, so returning a bound above the exact maximum can only make it draw a bit it could
    /// have skipped -- which writes nothing anyway -- never skip one it must draw. The `None`
    /// answer stays exact.
    ///
    /// Works over `COVER_TILE`-square tiles: a tile whose recorded bound is already below
    /// `limit` contributes that bound with no scan. Otherwise the tile is rescanned exactly
    /// (refreshing its bound) and, if it still is not below `limit`, the pixels it shares with
    /// the box decide. Front-to-back traversal makes the interior bits of a solid the ones that
    /// reach this test over pixels already settled, so most of their tiles are answered from
    /// the record. An untouched pixel holds `f64::INFINITY`, so a bit over open background
    /// still stops at its first tile. A `NaN` fails the `<` tests, the conservative direction.
    ///
    /// Before any tile is read, the coarse blocks (`coarse_max`) meeting the box are tried
    /// against `skip_below`, the caller's skip threshold: if every one is bounded strictly below
    /// it (recomputing a failing block from its tile bounds once), their largest bound is
    /// returned. It covers every pixel of the box and is below `skip_below`, so the caller's
    /// skip is proven by it alone. Otherwise the tile walk runs exactly as without the coarse
    /// level. When `skip_below <= limit`, a coarse success implies every tile bound in the box
    /// is below `limit` and `skip_below`, so the tile walk would have skipped too, without
    /// refreshing a tile: the skip decisions and the tile record are unchanged.
    // `!(a < b)` is deliberate: it is true for NaN, which `a >= b` is not.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    fn covering_depth(
        &mut self,
        left: i64,
        right: i64,
        top: i64,
        bottom: i64,
        limit: f64,
        skip_below: f64,
    ) -> Option<f64> {
        let width = self.width as usize;
        let height = self.height as usize;
        let (left, right, top, bottom) =
            (left as usize, right as usize, top as usize, bottom as usize);

        let (tile_left, tile_right) = (left / COVER_TILE, (right - 1) / COVER_TILE);
        let (tile_top, tile_bottom) = (top / COVER_TILE, (bottom - 1) / COVER_TILE);
        let mut coarse = f64::NEG_INFINITY;
        let mut coarse_covers = true;
        #[cfg(test)]
        #[allow(clippy::reversed_empty_ranges)] // `1..=0` is the intended empty range
        let coarse_range = if self.skip_coarse_cover {
            coarse_covers = false;
            1..=0
        } else {
            tile_top / COVER_COARSE..=tile_bottom / COVER_COARSE
        };
        #[cfg(not(test))]
        let coarse_range = tile_top / COVER_COARSE..=tile_bottom / COVER_COARSE;
        'coarse: for cy in coarse_range {
            for cx in tile_left / COVER_COARSE..=tile_right / COVER_COARSE {
                let block = cy * self.coarse_wide + cx;
                let mut bound = self.coarse_max[block];
                if !(bound < skip_below) {
                    bound = self.coarse_bound(cx, cy);
                    self.coarse_max[block] = bound;
                    if !(bound < skip_below) {
                        coarse_covers = false;
                        break 'coarse;
                    }
                }
                if bound > coarse {
                    coarse = bound;
                }
            }
        }
        if coarse_covers {
            #[cfg(test)]
            {
                self.coarse_hidden += 1;
            }
            return Some(coarse);
        }

        let mut covering = f64::NEG_INFINITY;
        for ty in top / COVER_TILE..=(bottom - 1) / COVER_TILE {
            let y0 = ty * COVER_TILE;
            let y1 = (y0 + COVER_TILE).min(height);
            for tx in left / COVER_TILE..=(right - 1) / COVER_TILE {
                let tile = ty * self.tiles_wide + tx;
                let bound = self.tile_max[tile];
                if bound < limit {
                    if bound > covering {
                        covering = bound;
                    }
                    continue;
                }
                let x0 = tx * COVER_TILE;
                let x1 = (x0 + COVER_TILE).min(width);
                let mut exact = f64::NEG_INFINITY;
                for y in y0..y1 {
                    let row = tile_row_max(&self.depth[y * width + x0..y * width + x1]);
                    // `NaN` poisons the tile bound so it is never trusted.
                    exact = if row.is_nan() || exact.is_nan() {
                        f64::NAN
                    } else if row > exact {
                        row
                    } else {
                        exact
                    };
                }
                self.tile_max[tile] = exact;
                if exact < limit {
                    if exact > covering {
                        covering = exact;
                    }
                    continue;
                }
                let (cx0, cx1) = (x0.max(left), x1.min(right));
                for y in y0.max(top)..y1.min(bottom) {
                    let row_max =
                        covering_row(&self.depth[y * width + cx0..y * width + cx1], limit)?;
                    if row_max > covering {
                        covering = row_max;
                    }
                }
            }
        }
        Some(covering)
    }

    /// Whether every pixel `delta_mask` marks in the box `[left, right) x [top, bottom)` holds a
    /// depth strictly nearer than `bound` (vacuously when it marks none). `NaN` fails.
    // `!(a < b)` is deliberate: it is true for NaN, which `a >= b` is not.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    fn behind_marks(&self, left: i64, right: i64, top: i64, bottom: i64, bound: f64) -> bool {
        let width = self.width as usize;
        let (left, right) = (left as usize, right as usize);
        for y in top as usize..bottom as usize {
            let row = y * width;
            for x in left..right {
                if self.delta_mask[row + x] && !(self.depth[row + x] < bound) {
                    return false;
                }
            }
        }
        true
    }

    /// The largest recorded tile bound over coarse block `(cx, cy)`, or `NaN` if any is one.
    fn coarse_bound(&self, cx: usize, cy: usize) -> f64 {
        let tiles_high = (self.height as usize).div_ceil(COVER_TILE);
        let x0 = cx * COVER_COARSE;
        let x1 = (x0 + COVER_COARSE).min(self.tiles_wide);
        let mut nan = false;
        let mut max = f64::NEG_INFINITY;
        for ty in cy * COVER_COARSE..(cy * COVER_COARSE + COVER_COARSE).min(tiles_high) {
            for &bound in &self.tile_max[ty * self.tiles_wide + x0..ty * self.tiles_wide + x1] {
                nan |= bound.is_nan();
                max = if bound > max { bound } else { max };
            }
        }
        if nan { f64::NAN } else { max }
    }

    /// Fills `open_rows` for the box `[left, right) x [top, bottom)` against `bound`, with the
    /// same `depth < bound` predicate the span trimming uses.
    // `!(a < b)` is deliberate: it is true for NaN, which `a >= b` is not.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    fn scan_open_rows(&mut self, left: i64, right: i64, top: i64, bottom: i64, bound: f64) {
        let width = self.width as usize;
        let mut rows = std::mem::take(&mut self.open_rows);
        rows.clear();
        for y in top..bottom {
            let row = &self.depth
                [y as usize * width + left as usize..y as usize * width + right as usize];
            let range = match row.iter().position(|&depth| !(depth < bound)) {
                None => (right, right),
                Some(first) => {
                    let last = row
                        .iter()
                        .rposition(|&depth| !(depth < bound))
                        .unwrap_or(first);
                    (left + first as i64, left + last as i64 + 1)
                }
            };
            rows.push(range);
        }
        self.open_rows = rows;
        self.open_top = top;
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
        if min_x >= max_x || min_y >= max_y {
            return;
        }

        if span_raster_applies(a, b, c) && self.span_raster_enabled() {
            self.raster_triangle_spans(a, b, c, color, area, min_x, max_x, min_y, max_y);
            return;
        }

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

    const fn span_raster_enabled(&self) -> bool {
        !self.reference_traversal
    }

    /// The per-pixel loop above, restricted to the pixels it would accept, bit for bit.
    ///
    /// Along one row each barycentric weight is `((px - ax) * k - r) / area` with `k`, `r` and
    /// `area` fixed. Every operation in that chain is a correctly rounded, monotone function of
    /// its varying operand, so the weight is monotone in `px` and the set of pixels passing its
    /// `!(w < BARYCENTRIC_EPSILON)` test is a prefix or a suffix of the row. The three tests
    /// therefore accept one contiguous run, which a binary search over the *same* predicate
    /// locates exactly. `span_raster_applies` keeps every operand small enough that no product
    /// overflows, so no weight is NaN and the monotonicity argument holds.
    ///
    /// Inside the run the weights, the interpolated inverse depth and its reciprocal are
    /// evaluated with the identical expressions and operation order (no fused or reassociated
    /// arithmetic), first into row scratch with no branches so the compiler can vectorise the
    /// divisions, then resolved against the depth buffer exactly as `plot` would.
    #[allow(clippy::too_many_arguments)]
    fn raster_triangle_spans(
        &mut self,
        a: ProjectedPoint,
        b: ProjectedPoint,
        c: ProjectedPoint,
        color: u32,
        area: f64,
        min_x: i64,
        max_x: i64,
        min_y: i64,
        max_y: i64,
    ) {
        // edge(p, q, px, py) = (px - p.x) * (q.y - p.y) - (py - p.y) * (q.x - p.x)
        let width = self.width as usize;
        let width_f = width as f64;
        let edges = [
            RowEdge::new(b, c, area, width_f),
            RowEdge::new(c, a, area, width_f),
            RowEdge::new(a, b, area, width_f),
        ];
        let span_len = (max_x - min_x) as usize;
        let mut inverse = std::mem::take(&mut self.span_inverse);
        let mut depths = std::mem::take(&mut self.span_depth);
        if inverse.len() < span_len {
            inverse.resize(span_len, 0.0);
            depths.resize(span_len, 0.0);
        }
        let (ad, bd, cd) = (a.depth, b.depth, c.depth);

        for y in min_y..max_y {
            let py = y as f64 + PIXEL_CENTER_OFFSET;
            let mut lo = min_x;
            let mut hi = max_x;
            // Every pixel outside the row's open range is strictly nearer than the bound, so the
            // trimming below would remove it anyway; and each edge's accepted set is a prefix or
            // suffix, so narrowing first and searching after finds the same run.
            if let Some(&(open_lo, open_hi)) = self.open_rows.get((y - self.open_top) as usize) {
                lo = lo.max(open_lo);
                hi = hi.min(open_hi);
                if lo >= hi {
                    continue;
                }
            }
            let rows = edges.map(|e| e.row(py));
            for row in &rows {
                (lo, hi) = row.accepted(lo, hi);
                if lo >= hi {
                    break;
                }
            }
            if lo >= hi {
                continue;
            }
            // Trim pixels that already hold a strictly nearer depth than this bit can produce;
            // `plot` would reject every one of them. `NaN` bounds compare false and trim nothing.
            let row_base = y as usize * width;
            let bound = self.occlusion_bound;
            while lo < hi && self.depth[row_base + lo as usize] < bound {
                lo += 1;
            }
            while lo < hi && self.depth[row_base + hi as usize - 1] < bound {
                hi -= 1;
            }
            if lo >= hi {
                continue;
            }

            let n = (hi - lo) as usize;
            let inverse = &mut inverse[..n];
            let depths = &mut depths[..n];
            let [ra, rb, rc] = rows;
            for (i, (inv, dep)) in inverse.iter_mut().zip(depths.iter_mut()).enumerate() {
                let px = (lo + i as i64) as f64 + PIXEL_CENTER_OFFSET;
                let wa = ra.value(px) / area;
                let wb = rb.value(px) / area;
                let wc = rc.value(px) / area;
                let inverse_depth = wa / ad + wb / bd + wc / cd;
                *inv = inverse_depth;
                *dep = 1.0 / inverse_depth;
            }

            self.mark_dirty(y as usize, lo as usize, hi as usize);
            let base = y as usize * width + lo as usize;
            let pixels = &mut self.pixels[base..base + n];
            let buffer = &mut self.depth[base..base + n];
            let owners = &mut self.owner[base..base + n];
            let ties = &mut self.tie[base..base + n];
            let key = self.owner_key;
            let mut written = 0_u64;
            let fragments = inverse
                .iter()
                .zip(depths.iter())
                .zip(pixels.iter_mut())
                .zip(buffer.iter_mut())
                .zip(owners.iter_mut())
                .zip(ties.iter_mut());
            if self.delta_redraw {
                let mask = &mut self.delta_mask[base..base + n];
                let written_at = &mut self.delta_written;
                for (i, (((((&inverse_depth, &depth), pixel), stored), owner), (tie, here))) in
                    inverse
                        .iter()
                        .zip(depths.iter())
                        .zip(pixels.iter_mut())
                        .zip(buffer.iter_mut())
                        .zip(owners.iter_mut())
                        .zip(ties.iter_mut().zip(mask.iter_mut()))
                        .enumerate()
                {
                    if inverse_depth <= 0.0 || !inverse_depth.is_finite() {
                        continue;
                    }
                    let depth_order = depth.total_cmp(stored);
                    if depth_order.is_eq() && *owner == key && !*here {
                        continue; // its own fragment of the frame before (see `delta_redraw`)
                    }
                    if depth_order.is_lt() || (depth_order.is_eq() && color < *pixel) {
                        if *pixel == BACKGROUND && color != BACKGROUND {
                            written += 1;
                        }
                        *stored = depth;
                        *pixel = color;
                        *owner = key;
                        *tie = depth_order.is_eq();
                        if !*here {
                            *here = true;
                            written_at.push(base + i);
                        }
                    } else if depth_order.is_eq() {
                        *tie = true;
                    }
                }
            } else if !self.marking {
                for (((((&inverse_depth, &depth), pixel), stored), owner), tie) in fragments {
                    if inverse_depth <= 0.0 || !inverse_depth.is_finite() {
                        continue;
                    }
                    let depth_order = depth.total_cmp(stored);
                    if depth_order.is_lt() || (depth_order.is_eq() && color < *pixel) {
                        if *pixel == BACKGROUND && color != BACKGROUND {
                            written += 1;
                        }
                        *stored = depth;
                        *pixel = color;
                        *owner = key;
                        *tie = depth_order.is_eq();
                    } else if depth_order.is_eq() {
                        *tie = true;
                    }
                }
            } else {
                let marks = &mut self.recolour_marks;
                for (i, (((((&inverse_depth, &depth), pixel), stored), owner), tie)) in
                    fragments.enumerate()
                {
                    if inverse_depth <= 0.0 || !inverse_depth.is_finite() {
                        continue;
                    }
                    let depth_order = depth.total_cmp(stored);
                    if depth_order.is_eq() && *owner == key {
                        // `recolour`: this bit's own stored fragment.
                        if color > *pixel && *tie {
                            marks.push(base + i);
                        }
                        *pixel = color;
                    } else if depth_order.is_lt() || (depth_order.is_eq() && color < *pixel) {
                        if *pixel == BACKGROUND && color != BACKGROUND {
                            written += 1;
                        }
                        *stored = depth;
                        *pixel = color;
                        *owner = key;
                        *tie = depth_order.is_eq();
                    } else if depth_order.is_eq() {
                        *tie = true;
                    }
                }
            }
            self.world_pixels += written;
        }
        self.span_inverse = inverse;
        self.span_depth = depths;
    }

    fn plot(&mut self, x: i64, y: i64, depth: f64, color: u32) {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return;
        }
        let index = y as usize * self.width as usize + x as usize;
        let depth_order = depth.total_cmp(&self.depth[index]);
        if self.marking && depth_order.is_eq() && self.owner[index] == self.owner_key {
            // The recolour pass meets this bit's own stored fragment (see `recolour_delta`).
            if color != self.pixels[index] {
                self.mark_dirty(y as usize, x as usize, x as usize + 1);
                if color > self.pixels[index] && self.tie[index] {
                    self.recolour_marks.push(index);
                }
                self.pixels[index] = color;
            }
            return;
        }
        if self.delta_redraw
            && depth_order.is_eq()
            && self.owner[index] == self.owner_key
            && !self.delta_mask[index]
        {
            return; // its own fragment of the frame before (see `delta_redraw`)
        }
        // Equal-depth fragments choose the lower RGB word so unspecified world iteration order
        // cannot change the final frame.
        if depth_order.is_lt() || (depth_order.is_eq() && color < self.pixels[index]) {
            self.mark_dirty(y as usize, x as usize, x as usize + 1);
            if self.pixels[index] == BACKGROUND && color != BACKGROUND {
                self.world_pixels += 1;
            }
            self.depth[index] = depth;
            self.pixels[index] = color;
            self.owner[index] = self.owner_key;
            self.tie[index] = depth_order.is_eq();
            if self.delta_redraw && !self.delta_mask[index] {
                self.delta_mask[index] = true;
                self.delta_written.push(index);
            }
        } else if depth_order.is_eq() {
            self.tie[index] = true;
        }
    }
}

/// Sorts `redraw` nearest first and leaves each bit in `redraw` and `fresh` once.
fn dedup_draws(redraw: &mut Vec<(f64, Coord)>, fresh: &mut Vec<Coord>) {
    redraw.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    redraw.dedup_by_key(|e| e.1);
    if fresh.is_empty() {
        return;
    }
    let mut drawn = crate::world::CoordMap::default();
    fresh.retain(|&c| drawn.insert(c, ()).is_none());
    redraw.retain(|e| !drawn.contains_key(&e.1));
}

/// The largest depth in a short row, or `NaN` if the row holds one (`>`-select max, so the
/// sign of a zero is unspecified, as for `f64::max`).
#[inline]
fn tile_row_max(row: &[f64]) -> f64 {
    let mut max = f64::NEG_INFINITY;
    let mut nan = false;
    for &depth in row {
        nan |= depth.is_nan();
        max = if depth > max { depth } else { max };
    }
    if nan { f64::NAN } else { max }
}

/// One row of [`Renderer::covering_depth`]: `None` unless every depth is `< limit`, else the
/// row's largest depth. Branch-free over the row (four independent lanes) so it vectorises; the
/// early exit is per row instead of per pixel, which changes only how much is scanned.
///
/// Written as a `<` test rather than a `>=` one so that a `NaN` fails the row. The depth buffer
/// cannot hold one, but `plot` would treat it as farther than anything under `total_cmp`, so it
/// must not be swallowed by the max. With every depth `< limit` there is no `NaN`, so the
/// `>`-select max equals `f64::max` up to the sign of a zero, which no caller can observe (the
/// result is only compared).
#[inline]
fn covering_row(row: &[f64], limit: f64) -> Option<f64> {
    let mut max = [f64::NEG_INFINITY; 4];
    let mut below = [true; 4];
    let chunks = row.chunks_exact(4);
    let rest = chunks.remainder();
    for chunk in chunks {
        for lane in 0..4 {
            let depth = chunk[lane];
            below[lane] &= depth < limit;
            max[lane] = if depth > max[lane] { depth } else { max[lane] };
        }
    }
    for &depth in rest {
        below[0] &= depth < limit;
        max[0] = if depth > max[0] { depth } else { max[0] };
    }
    if !(below[0] & below[1] & below[2] & below[3]) {
        return None;
    }
    let a = if max[1] > max[0] { max[1] } else { max[0] };
    let b = if max[3] > max[2] { max[3] } else { max[2] };
    Some(if b > a { b } else { a })
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

fn test_raycast_coord(best: Option<Hit>, ray: Ray, coord: Coord) -> Option<Hit> {
    let center = coord_center(coord);
    let min = center - Vec3::splat(0.5);
    let max = center + Vec3::splat(0.5);
    let Some((distance, normal)) = ray_box(ray, min, max) else {
        return best;
    };
    let adjacent = offset_coord(coord, normal);
    let candidate = Hit {
        coord,
        adjacent,
        distance,
    };
    select_raycast_hit(best, candidate)
}

/// Returns the non-negative entry distance along `ray` into the closed axis-aligned box `min..=max`.
pub fn ray_aabb_entry_distance(ray: Ray, min: Vec3, max: Vec3) -> Option<f64> {
    let interval = box_interval(ray.origin, ray.direction, min, max)?;
    if interval.exit < 0.0 {
        return None;
    }
    Some(if interval.entry >= 0.0 {
        interval.entry
    } else {
        0.0
    })
}

/// Exhaustively tests every occupied coordinate in `world` to find the nearest ray intersection.
pub fn raycast_reference(world: &World, ray: Ray) -> Option<Hit> {
    let mut best: Option<Hit> = None;

    for coord in world.iter_on() {
        best = test_raycast_coord(best, ray, coord);
    }

    best
}

/// Finds the nearest occupied voxel hit along `ray` using chunk-bounds AABB pruning.
pub fn raycast(world: &World, ray: Ray) -> Option<Hit> {
    let mut best: Option<Hit> = None;

    for (chunk_coord, _count, chunk_bounds, words) in world.iter_chunks() {
        let (cmin, cmax) = bounds_aabb(chunk_bounds);
        let Some(chunk_entry) = ray_aabb_entry_distance(ray, cmin, cmax) else {
            continue;
        };
        if best.is_some_and(|current| chunk_entry > current.distance) {
            continue;
        }
        for coord in crate::world::World::iter_chunk_coords(chunk_coord, words) {
            best = test_raycast_coord(best, ray, coord);
        }
    }

    best
}

fn validate_sweep_movement(start: Vec3, delta: Vec3) -> f64 {
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
    movement_length
}

fn test_sweep_coord(
    best: Option<(f64, Coord)>,
    start: Vec3,
    delta: Vec3,
    coord: Coord,
) -> Option<(f64, Coord)> {
    let center = coord_center(coord);
    let min = center - Vec3::splat(0.5);
    let max = center + Vec3::splat(0.5);
    let Some(fraction) = segment_box_entry_fraction(start, delta, min, max) else {
        return best;
    };

    let candidate = (fraction, coord);
    let replace = best.is_none_or(|current| {
        fraction
            .total_cmp(&current.0)
            .then_with(|| coord.cmp(&current.1))
            .is_lt()
    });
    if replace { Some(candidate) } else { best }
}

/// Exhaustively sweeps a point segment across all occupied voxels without chunk-level pruning.
pub fn sweep_point_reference(world: &World, start: Vec3, delta: Vec3) -> Option<SweepHit> {
    let movement_length = validate_sweep_movement(start, delta);
    let mut best: Option<(f64, Coord)> = None;

    for coord in world.iter_on() {
        best = test_sweep_coord(best, start, delta, coord);
    }

    best.map(|(fraction, coord)| SweepHit {
        coord,
        distance: movement_length * fraction,
    })
}

/// Sweeps a point from `start` through the complete finite segment `start + delta`.
///
/// This tests the segment against every occupied voxel AABB, so a large delta cannot tunnel
/// through a block. Exact simultaneous hits are resolved by [`Coord`]'s lexicographic order,
/// making the result independent of the world's unspecified iteration order. The segment start,
/// delta, endpoint, and Euclidean delta length must all be finite.
pub fn sweep_point(world: &World, start: Vec3, delta: Vec3) -> Option<SweepHit> {
    let movement_length = validate_sweep_movement(start, delta);
    let mut best: Option<(f64, Coord)> = None;

    for (chunk_coord, _count, chunk_bounds, words) in world.iter_chunks() {
        let (cmin, cmax) = bounds_aabb(chunk_bounds);
        let Some(chunk_fraction) = segment_box_entry_fraction(start, delta, cmin, cmax) else {
            continue;
        };
        if best.is_some_and(|(best_fraction, _)| chunk_fraction > best_fraction) {
            continue;
        }
        for coord in crate::world::World::iter_chunk_coords(chunk_coord, words) {
            best = test_sweep_coord(best, start, delta, coord);
        }
    }

    best.map(|(fraction, coord)| SweepHit {
        coord,
        distance: movement_length * fraction,
    })
}

/// Maps a finite world-space point to the integer voxel coordinate whose closed unit cube contains it.
pub fn coord_containing(point: Vec3) -> Option<Coord> {
    fn axis(value: f64) -> Option<i64> {
        let centered = (value + 0.5).floor();
        (centered.is_finite() && centered >= i64::MIN as f64 && centered < i64::MAX as f64)
            .then_some(centered as i64)
    }

    Some(Coord::new(axis(point.x)?, axis(point.y)?, axis(point.z)?))
}

/// Expands inclusive integer voxel `Bounds` into its world-space `min` and `max` AABB corners.
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

/// Least view depth, in world units, of every point of a chunk (and its neighbours) for
/// the enclosed-bit cull. Far above rounding in `view_depth`, far below any useful view.
const ENCLOSED_MIN_DEPTH: f64 = 1.0e-3;
/// Least projected width, in pixels, of every bit near a chunk for the enclosed-bit cull.
///
/// The single-pixel path needs *both* raster extents `<= SMALL_BIT_EXTENT`. Every bit in the
/// grown box has both extents above `focal / d` (see [`enclosed_cull_applies`]), so any bound
/// `>= SMALL_BIT_EXTENT` keeps it out. The extra quarter pixel absorbs rounding in the
/// projection. (§5z used `2 * SMALL_BIT_EXTENT`; §6a tightened it so the cull also runs on the
/// 1.25-2 px bits of r64-r96 planets.)
const ENCLOSED_MIN_SPAN: f64 = 1.25 * SMALL_BIT_EXTENT;

/// Whether the enclosed-bit cull may run over a chunk with tight bounds `bounds`.
///
/// Every bit of the chunk and its whole 3 x 3 x 3 neighbourhood lies in the chunk's cube box
/// grown by one unit. If all eight corners of that box are at view depth above `ENCLOSED_MIN_DEPTH`, so
/// is every vertex inside it (depth is affine), and every face of those bits is rasterised. A
/// unit cube holds a ball of radius 1/2. Its image under the pinhole projection (one focal
/// length for both axes) is an ellipse with minor semi-axis `focal * (1/2) / sqrt(D^2 - 1/4)`,
/// where `D` is the camera-to-centre distance. That is more than `focal / (2 D)`, so the
/// cube's raster box is more than `focal / D` pixels wide and tall. The box's farthest corner
/// bounds `D` from above, so if `focal / d >= ENCLOSED_MIN_SPAN` (`d` = that corner's distance),
/// no bit there takes the single-pixel path.
fn enclosed_cull_applies(projection: CameraProjection, bounds: Bounds) -> bool {
    let (min, max) = bounds_aabb(bounds);
    let (min, max) = (min - Vec3::splat(1.0), max + Vec3::splat(1.0));
    let focal = projection.focal_length();
    if !(focal.is_finite() && focal > 0.0) {
        return false;
    }
    let mut farthest_squared = 0.0_f64;
    for corner in crate::camera::bounds_corners(min, max) {
        // `!(a > b)` is deliberate: it rejects NaN.
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        if !(projection.view_depth(corner) > ENCLOSED_MIN_DEPTH) {
            return false;
        }
        let relative = corner - projection.position();
        farthest_squared = farthest_squared.max(relative.dot(relative));
    }
    farthest_squared.is_finite()
        && focal * focal >= ENCLOSED_MIN_SPAN * ENCLOSED_MIN_SPAN * farthest_squared
}

/// Storage of a delta frame's classification, kept by the renderer between frames: the
/// exposed words of each chunk met and their slots, and the loud bits (bit, whether it
/// toggled, whether it is exposed in the new world, its grown box). Each frame clears it.
#[derive(Default)]
struct DeltaScratch {
    open_words: Vec<[u64; crate::world::CHUNK_WORDS]>,
    open_cache: crate::world::CoordMap<crate::world::ChunkCoord, usize>,
    loud: Vec<(Coord, bool, bool, Option<DeltaEntry>)>,
}

/// A set of bits, as one bit per cell in per-chunk words (the bits of the delta index).
struct IndexedBits {
    slot: crate::world::CoordMap<crate::world::ChunkCoord, usize>,
    words: Vec<[u64; crate::world::CHUNK_WORDS]>,
    /// Recently met chunks and their slots, direct-mapped by the chunk's low coordinate bits
    /// (`recent_way`): nearby inserts and lookups then rarely hash.
    recent: [Option<(crate::world::ChunkCoord, usize)>; RECENT_WAYS],
}

impl Default for IndexedBits {
    fn default() -> Self {
        Self {
            slot: Default::default(),
            words: Vec::new(),
            recent: [None; RECENT_WAYS],
        }
    }
}

/// The `IndexedBits::recent` way of a chunk.
fn recent_way(chunk: crate::world::ChunkCoord) -> usize {
    (chunk.x & 3 | (chunk.y & 3) << 2 | (chunk.z & 15) << 4) as usize
}

/// Ways of the recent-chunk caches: 4 x 4 chunks across, 16 along `z`, so a domain row of
/// cells (`z` fastest, as the chem domain and its changed bits are ordered) meets each of
/// its chunks in its own way.
const RECENT_WAYS: usize = 256;

impl IndexedBits {
    /// Adds `coord`; `true` when it was not in the set.
    fn insert(&mut self, coord: Coord) -> bool {
        let (chunk, index) = World::chunk_of(coord);
        let way = recent_way(chunk);
        let slot = match self.recent[way] {
            Some((c, slot)) if c == chunk => slot,
            _ => {
                let words = &mut self.words;
                let slot = *self.slot.entry(chunk).or_insert_with(|| {
                    words.push([0; crate::world::CHUNK_WORDS]);
                    words.len() - 1
                });
                self.recent[way] = Some((chunk, slot));
                slot
            }
        };
        let word = &mut self.words[slot][index / 64];
        let bit = 1_u64 << (index % 64);
        let fresh = *word & bit == 0;
        *word |= bit;
        fresh
    }

    /// Whether `coord` is in the set.
    fn contains(&mut self, coord: Coord) -> bool {
        let (chunk, index) = World::chunk_of(coord);
        let way = recent_way(chunk);
        let slot = match self.recent[way] {
            Some((c, slot)) if c == chunk => slot,
            _ => match self.slot.get(&chunk) {
                Some(&slot) => {
                    self.recent[way] = Some((chunk, slot));
                    slot
                }
                None => return false,
            },
        };
        self.words[slot][index / 64] >> (index % 64) & 1 == 1
    }

    /// The words of `chunk`, when any of its bits was ever added since the last clear.
    fn chunk_words(
        &self,
        chunk: crate::world::ChunkCoord,
    ) -> Option<&[u64; crate::world::CHUNK_WORDS]> {
        self.slot.get(&chunk).map(|&slot| &self.words[slot])
    }

    /// §6x: empties every chunk's words for which `keep` is false (the chunk stays indexed).
    fn retain_chunks(&mut self, mut keep: impl FnMut(crate::world::ChunkCoord) -> bool) {
        for (&chunk, &slot) in self.slot.iter() {
            if !keep(chunk) {
                self.words[slot] = [0; crate::world::CHUNK_WORDS];
            }
        }
    }

    /// Empties the set, keeping its storage.
    fn clear(&mut self) {
        self.slot.clear();
        self.words.clear();
        self.recent = [None; RECENT_WAYS];
    }
}

/// A bit and its grown, clamped raster box `[x0, x1) x [y0, y1)` (see `Renderer::grown_box`).
#[derive(Clone, Copy, Debug)]
struct DeltaEntry {
    coord: Coord,
    x0: u16,
    x1: u16,
    y0: u16,
    y1: u16,
}

/// A nonzero 32-bit key of `coord` for the per-pixel owner buffer. Collisions are harmless:
/// they only add pixels to a delta frame's reset set.
fn owner_key(coord: Coord) -> u32 {
    let mut h = 0x9e37_79b9_7f4a_7c15_u64;
    for v in [coord.x, coord.y, coord.z] {
        h = (h ^ v as u64).wrapping_mul(0xff51_afd7_ed55_8ccd);
        h ^= h >> 29;
    }
    (h >> 32) as u32 | 1
}

/// `coord` and its 26 neighbours (those inside the `i64` range).
fn neighbourhood27(coord: Coord) -> impl Iterator<Item = Coord> {
    (-1_i64..=1).flat_map(move |dx| {
        (-1_i64..=1).flat_map(move |dy| {
            (-1_i64..=1).filter_map(move |dz| {
                Some(Coord::new(
                    coord.x.checked_add(dx)?,
                    coord.y.checked_add(dy)?,
                    coord.z.checked_add(dz)?,
                ))
            })
        })
    })
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

/// Projected extent, in pixels, above which a drawn bit carries its depth bound into the span
/// rasteriser to trim already-occluded pixels. Smaller bits are not worth the bound.
const OCCLUSION_TRIM_EXTENT: f64 = 8.0;

/// Side, in pixels, of the tiles whose depth bounds `covering_depth` records.
const COVER_TILE: usize = 4;

/// Side, in tiles, of the coarse blocks whose depth bounds `covering_depth` tries first.
const COVER_COARSE: usize = 4;

/// Largest vertex coordinate for which the span rasteriser's products provably stay finite.
/// Pixel centres are below 2^32, so every factor is below 2^101 and every product below 2^202.
const SPAN_COORDINATE_LIMIT: f64 = 1.0e30;

fn span_raster_applies(a: ProjectedPoint, b: ProjectedPoint, c: ProjectedPoint) -> bool {
    [a.x, a.y, b.x, b.y, c.x, c.y]
        .iter()
        .all(|v| v.abs() < SPAN_COORDINATE_LIMIT)
}

/// Relative margin around `BARYCENTRIC_EPSILON` outside which [`RowEdge::passes`] decides from
/// the multiplied weight without the exact division. It only has to exceed the few-ulp
/// disagreement between `v * (1 / area)` and `v / area` (see `passes`); anything wider merely
/// sends more pixels down the exact path.
const WEIGHT_FILTER_MARGIN: f64 = 1.0e-9;
/// A multiplied weight above this is certainly accepted, below [`WEIGHT_REJECT_BELOW`]
/// certainly rejected (`BARYCENTRIC_EPSILON` is negative).
const WEIGHT_ACCEPT_ABOVE: f64 = BARYCENTRIC_EPSILON * (1.0 - WEIGHT_FILTER_MARGIN);
const WEIGHT_REJECT_BELOW: f64 = BARYCENTRIC_EPSILON * (1.0 + WEIGHT_FILTER_MARGIN);

/// Relative slack, against the magnitudes entering it, of the closed-form crossing bound in
/// [`RowEdge::accepted`]. The rounding it must cover is below `16 * 2^-53` of those magnitudes;
/// 2^-40 is 512 times that, so the bound stays valid with room for its own rounding and that of
/// `crossing - bound` / `crossing + bound`. On screen-sized coordinates the bound is a few
/// 1e-9 px, so a crossing falls back to the search only that close to a pixel column.
const CROSSING_SLACK: f64 = 1.0 / (1u64 << 40) as f64;
/// Smallest `|k|` for which the closed form is used: with `|px - ax| >= 2^-53` (or zero) and
/// `|area| >= f64::EPSILON`, every product near the threshold is then a normal number.
const CROSSING_MIN_SLOPE: f64 = 1.0 / (1u64 << 30) as f64;

/// One `edge` term with its per-triangle and per-row factors split out. The split is exact: it
/// stores the same intermediate values `edge` computes and combines them in the same order.
#[derive(Clone, Copy, Debug)]
struct RowEdge {
    ax: f64,
    ay: f64,
    /// `by - ay`
    k: f64,
    /// `bx - ax`
    dx: f64,
    /// `(py - ay) * (bx - ax)` for the current row.
    r: f64,
    /// The triangle's signed `area`, the exact weight denominator.
    area: f64,
    /// `1 / area`, for the division-free filter in `passes` only.
    inv_area: f64,
    /// `1 / k`, for the crossing estimate only (never decides acceptance alone).
    inv_k: f64,
    /// `BARYCENTRIC_EPSILON * area`, rounded: shifts the crossing estimate onto the threshold.
    eps_area: f64,
    /// Row-independent part of the crossing estimate's error bound (see `accepted`), or
    /// infinity when the closed form is not used for this edge.
    bound_base: f64,
}

impl RowEdge {
    /// `width` bounds every pixel column the edge will be asked about (`hi <= width`).
    fn new(p: ProjectedPoint, q: ProjectedPoint, area: f64, width: f64) -> Self {
        let k = q.y - p.y;
        let inv_k = 1.0 / k;
        let eps_area = BARYCENTRIC_EPSILON * area;
        let bound_base = if k.abs() >= CROSSING_MIN_SLOPE {
            CROSSING_SLACK * (p.x.abs() + (eps_area * inv_k).abs() + width + 2.0)
        } else {
            f64::INFINITY
        };
        Self {
            ax: p.x,
            ay: p.y,
            k,
            dx: q.x - p.x,
            r: 0.0,
            area,
            inv_area: 1.0 / area,
            inv_k,
            eps_area,
            bound_base,
        }
    }

    fn row(self, py: f64) -> Self {
        Self {
            r: (py - self.ay) * self.dx,
            ..self
        }
    }

    #[inline(always)]
    fn value(self, px: f64) -> f64 {
        (px - self.ax) * self.k - self.r
    }

    /// Exactly `!(self.value(px) / area < BARYCENTRIC_EPSILON)`, the per-pixel loop's test.
    ///
    /// The division is only evaluated near the threshold. `t = v * fl(1 / area)` differs from
    /// the real quotient `z = v / area` by a relative `(1 + d1)(1 + d2)` with `|d| <= 2^-53`
    /// (`area` is finite with `|area| >= f64::EPSILON` and the operands are bounded by
    /// `SPAN_COORDINATE_LIMIT`, so `1 / area` is a normal number and the product cannot
    /// overflow), and `t` has the sign of `z`. So:
    /// * `t > E (1 - m)` (with `E < 0`): either `t >= 0`, hence `z >= 0 > E`, or `|z| <= |t| /
    ///   (1 - 2^-53)^2 < |E|`, hence `z > E`; rounding is monotone and `E` is a double, so
    ///   `fl(z) >= E` and the pixel passes. An underflowed `t` lands here too, and then `|z|`
    ///   is just as far below `|E|`.
    /// * `t < E (1 + m)`: `z < E` by a relative gap of about `m`, far more than one ulp, so
    ///   `fl(z) < E` and the pixel fails.
    /// * Otherwise, or if `t` is `NaN`, the exact expression decides.
    #[inline(always)]
    // `!(a < b)` is deliberate: it is true for NaN, which `a >= b` is not.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    fn passes(self, x: i64) -> bool {
        let px = x as f64 + PIXEL_CENTER_OFFSET;
        let v = self.value(px);
        let t = v * self.inv_area;
        if t > WEIGHT_ACCEPT_ABOVE {
            return true;
        }
        if t < WEIGHT_REJECT_BELOW {
            return false;
        }
        !(v / self.area < BARYCENTRIC_EPSILON)
    }

    /// Narrows `[lo, hi)` to the pixels this edge accepts. The weight is monotone in `x`
    /// (non-decreasing when `k / area > 0`), so acceptance is a suffix or a prefix.
    fn accepted(self, lo: i64, hi: i64) -> (i64, i64) {
        if lo >= hi {
            return (lo, hi);
        }
        if self.k == 0.0 {
            // Constant along the row.
            return if self.passes(lo) { (lo, hi) } else { (lo, lo) };
        }
        let increasing = (self.k > 0.0) == (self.area > 0.0);
        // Closed form. In real arithmetic the weight reaches `BARYCENTRIC_EPSILON` at the pixel
        // column `X = ax + (r + E * area) / k - 1/2`; pixels on the accepted side of `X` pass
        // and the others fail. Every rounding in `passes` (four operations, plus the threshold
        // comparison) and in `crossing` below (five operations) moves either point by a relative
        // few `2^-53` of the magnitudes summed in `bound`, so the computed predicate agrees with
        // the side of `crossing` at every column farther than `bound` from it. `bound` carries a
        // slack of 2^-40, thousands of times that error (see `CROSSING_SLACK`), and
        // `CROSSING_MIN_SLOPE` keeps every intermediate a normal number near the threshold, so
        // the relative-error model holds. When no integer column lies within `bound` of the
        // crossing, the run boundary is therefore exact without evaluating the predicate;
        // otherwise (or on any non-finite value) the search below decides.
        let crossing = (self.ax + (self.r + self.eps_area) * self.inv_k) - PIXEL_CENTER_OFFSET;
        let bound =
            self.bound_base + CROSSING_SLACK * ((self.r + self.eps_area) * self.inv_k).abs();
        let below = crossing - bound;
        let first = below.ceil();
        if first > crossing + bound {
            let found = first.clamp(lo as f64, hi as f64) as i64;
            #[cfg(test)]
            assert_eq!(
                found,
                if increasing {
                    self.first_where(lo, hi, lo, true)
                } else {
                    self.first_where(lo, hi, lo, false)
                },
                "closed-form edge boundary disagrees with the exact predicate"
            );
            return if increasing { (found, hi) } else { (lo, found) };
        }
        // Where the weight crosses zero, from the real-valued line: only a starting guess. The
        // exact boundary is then found by evaluating the exact predicate, so the guess cannot
        // change which pixels are accepted, only how many evaluations that takes. (Ties to even:
        // one native instruction on wasm, where `round` is a libm call; and `1 / k` is multiplied
        // in, since a guess needs no exact quotient.)
        let guess = if crossing.is_finite() {
            (crossing.round_ties_even().clamp(lo as f64, hi as f64)) as i64
        } else {
            lo
        };
        if increasing {
            // First accepted pixel in [lo, hi]; the predicate runs false..true.
            let found = self.first_where(lo, hi, guess, true);
            (found, hi)
        } else {
            // One past the last accepted pixel; the predicate runs true..false.
            let found = self.first_where(lo, hi, guess, false);
            (lo, found)
        }
    }

    /// The first `x` in `[lo, hi)` whose predicate equals `want`, or `hi`, given that the
    /// predicate is monotone over the range and flips (at most once) to `want`. Steps outward
    /// from `guess` a few pixels, then falls back to bisection of what is left.
    fn first_where(self, lo: i64, hi: i64, guess: i64, want: bool) -> i64 {
        const STEPS: u32 = 3;
        let at = |x: i64| self.passes(x) == want;
        let mut l = lo;
        let mut h = hi;
        let g = guess.clamp(lo, hi);
        if g < hi && at(g) {
            // Answer is at or before g: walk left while the predecessor still holds.
            h = g;
            let mut x = g;
            for _ in 0..STEPS {
                if x == lo || !at(x - 1) {
                    return x;
                }
                x -= 1;
                h = x;
            }
        } else if g >= hi {
            // Nothing past the range is known: walk left from `hi` exactly as above. (Taking
            // `hi` on trust here would accept a whole row whenever the guess lands past it.)
            let mut x = hi;
            for _ in 0..STEPS {
                if x == lo || !at(x - 1) {
                    return x;
                }
                x -= 1;
                h = x;
            }
        } else {
            // Answer is after g.
            l = g + 1;
            let mut x = l;
            for _ in 0..STEPS {
                if x >= hi || at(x) {
                    return x;
                }
                x += 1;
                l = x;
            }
        }
        while l < h {
            let m = l + (h - l) / 2;
            if at(m) {
                h = m;
            } else {
                l = m + 1;
            }
        }
        l
    }
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
    /// The tinted world pass: `None` is bit-identical to `render`, a per-bit tint keeps the
    /// ordered traversal (hidden-bit test included) identical to the unordered reference, and
    /// every emitted world colour is one of the tint shades.
    #[test]
    fn tinted_render_matches_reference_and_untinted_is_unchanged() {
        let size = 161;
        let world = solid_world(9);
        let palette = [0x002f6fd6_u32, 0x00b04020, 0x00e8dcc0];
        let tint = |c: Coord| -> Option<u32> {
            if c.y > 2 {
                None
            } else {
                Some(palette[(c.x + c.z).rem_euclid(3) as usize])
            }
        };
        let mut legal: Vec<u32> = palette.iter().flat_map(|&b| tinted_shades(b)).collect();
        legal.sort_unstable();
        for (index, camera) in views(&world).iter().enumerate() {
            let mut plain = Renderer::new(size, size);
            plain.render(&world, camera);
            let mut none = Renderer::new(size, size);
            none.render_tinted(&world, camera, None);
            assert_eq!(plain.pixels, none.pixels, "camera {index} untinted pixels");

            let mut reference = Renderer::new(size, size);
            reference.reference_traversal = true;
            reference.render_tinted(&world, camera, Some(&tint));
            let mut ordered = Renderer::new(size, size);
            ordered.verify_hidden_bits = true;
            ordered.render_tinted(&world, camera, Some(&tint));
            assert_eq!(
                ordered.pixels, reference.pixels,
                "camera {index} tinted pixels"
            );

            ordered.draw_crosshair(crosshair_overlay_input());
            let (_, strict) = ordered.observe_final_game_frame();
            let (_, audit) =
                ordered.observe_final_game_frame_tinted(&|rgb| legal.binary_search(&rgb).is_ok());
            assert!(audit.is_valid(), "camera {index} tinted audit {audit:?}");
            if index < 3 {
                assert!(
                    !strict.is_valid(),
                    "camera {index} should show tinted colours"
                );
                let distinct: std::collections::BTreeSet<u32> =
                    ordered.pixels.iter().map(|p| p & RGB_MASK).collect();
                assert!(
                    distinct.len() >= 6,
                    "camera {index} distinct {}",
                    distinct.len()
                );
            }
        }
        assert_eq!(tinted_face_color(0x00ffffff, Vec3::Y), 0x00ffffff);
        assert_eq!(
            tinted_face_color(0x00808080, -Vec3::Z),
            shade_rgb(0x00808080, 153)
        );
    }

    /// Lit-face masks: a zero mask is the plain tinted frame; a nonzero mask recolours exactly the
    /// masked faces to `lit_face_color` (hue kept, never equal to a directional shade), traversal
    /// order stays irrelevant, and the hidden-bit test still writes nothing.
    #[test]
    fn tinted_lit_faces_recolour_only_masked_faces() {
        let size = 161;
        let world = solid_world(9);
        let palette = [0x002f6fd6_u32, 0x00b04020, 0x00e8dcc0];
        let plain = |c: Coord| -> Option<u32> {
            if c.y > 2 {
                None
            } else {
                Some(palette[(c.x + c.z).rem_euclid(3) as usize])
            }
        };
        let zero = |c: Coord| plain(c).map(|b| tint_with_lit_faces(b, 0));
        let lit =
            |c: Coord| plain(c).map(|b| tint_with_lit_faces(b, if c.x >= 0 { 0x3f } else { 0 }));
        let mut legal: Vec<u32> = palette
            .iter()
            .flat_map(|&b| tinted_shades(b).into_iter().chain([lit_face_color(b)]))
            .collect();
        legal.sort_unstable();
        let lit_colors: Vec<u32> = palette.iter().map(|&b| lit_face_color(b)).collect();
        for &b in &palette {
            assert!(!tinted_shades(b).contains(&lit_face_color(b)), "{b:06x}");
        }
        let mut any_lit = false;
        for (index, camera) in views(&world).iter().enumerate() {
            let mut a = Renderer::new(size, size);
            a.render_tinted(&world, camera, Some(&plain));
            let mut z = Renderer::new(size, size);
            z.render_tinted(&world, camera, Some(&zero));
            assert_eq!(a.pixels, z.pixels, "camera {index} zero mask");

            let mut reference = Renderer::new(size, size);
            reference.reference_traversal = true;
            reference.render_tinted(&world, camera, Some(&lit));
            let mut ordered = Renderer::new(size, size);
            ordered.verify_hidden_bits = true;
            ordered.render_tinted(&world, camera, Some(&lit));
            assert_eq!(
                ordered.pixels, reference.pixels,
                "camera {index} lit pixels"
            );
            for (p, q) in ordered.pixels.iter().zip(&a.pixels) {
                if p != q {
                    assert!(
                        lit_colors.contains(p),
                        "camera {index} changed pixel {p:06x}"
                    );
                    any_lit = true;
                }
            }
            ordered.draw_crosshair(crosshair_overlay_input());
            let (_, audit) =
                ordered.observe_final_game_frame_tinted(&|rgb| legal.binary_search(&rgb).is_ok());
            assert!(audit.is_valid(), "camera {index} lit audit {audit:?}");
        }
        assert!(any_lit, "some view shows a lit face");
        assert_eq!(lit_face_color(0x00ffffff), 0x00ffffff);
        assert_eq!(lit_face_color(0), 0x002f2f2f);
        for (i, d) in TINT_LIT_DIRS.iter().enumerate() {
            let n = Vec3::new(d[0] as f64, d[1] as f64, d[2] as f64);
            assert_eq!(tint_lit_bit(n), 1 << i);
        }
    }

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

    /// The coarse level of `covering_depth` must change nothing but the work done: the same
    /// pixels, depths, skipped bits and tile record as the tile walk alone, while answering for
    /// many bits by itself. Odd sizes leave partial tiles and coarse blocks on the right and
    /// bottom edges.
    #[test]
    fn coarse_cover_level_changes_no_decision() {
        let mut coarse_total = 0;
        for (width, height) in [(201_u32, 201_u32), (203, 131), (97, 150)] {
            for (name, world) in [("shell", shell_world(14)), ("solid", solid_world(9))] {
                for (index, camera) in views(&world).iter().enumerate() {
                    let mut tiles_only = Renderer::new(width, height);
                    tiles_only.skip_coarse_cover = true;
                    tiles_only.render(&world, camera);

                    let mut coarse = Renderer::new(width, height);
                    coarse.verify_hidden_bits = true;
                    coarse.render(&world, camera);

                    let at = format!("{name} {width}x{height} camera {index}");
                    assert_eq!(tiles_only.coarse_hidden, 0);
                    assert!(coarse.pixels == tiles_only.pixels, "{at} pixels");
                    let bits = |v: &[f64]| v.iter().map(|d| d.to_bits()).collect::<Vec<_>>();
                    assert_eq!(bits(&coarse.depth), bits(&tiles_only.depth), "{at} depth");
                    assert_eq!(
                        bits(&coarse.tile_max),
                        bits(&tiles_only.tile_max),
                        "{at} tiles"
                    );
                    assert_eq!(coarse.hidden_bits, tiles_only.hidden_bits, "{at} hidden");
                    assert_eq!(coarse.world_pixel_count(), tiles_only.world_pixel_count());
                    assert!(coarse.coarse_hidden <= coarse.hidden_bits);
                    coarse_total += coarse.coarse_hidden;
                }
            }
        }
        assert!(
            coarse_total > 1000,
            "the coarse level proved only {coarse_total} bits hidden"
        );
    }

    /// The span rasteriser must accept exactly the pixels the per-pixel loop accepts and write
    /// bit-identical depths. Triangles are drawn on top of a pseudo-random depth buffer so the
    /// equal-depth tie-break and partial occlusion are exercised, and include slivers, both
    /// windings, vertices on pixel centres and edges, near-plane depths and coordinates far
    /// outside the viewport.
    #[test]
    fn span_raster_matches_per_pixel_raster_bit_for_bit() {
        let mut rng = Rng(0x005e_ed5a_110f_b17b);
        let (width, height) = (61_u32, 37_u32);
        let mut spans = 0_u64;
        for case in 0..6000 {
            let point = |rng: &mut Rng| {
                let scale = match rng.below(5) {
                    0 => 4.0,
                    1 => 80.0,
                    2 => 1.0e6,
                    _ => 30.0,
                };
                let mut x = rng.range(-scale, scale) + 30.0;
                let mut y = rng.range(-scale, scale) + 18.0;
                if rng.below(4) == 0 {
                    x = x.round() + 0.5;
                    y = y.round();
                }
                let depth = match rng.below(6) {
                    0 => NEAR_PLANE * 2.0,
                    1 => rng.range(1.0e3, 1.0e6),
                    _ => rng.range(0.05, 20.0),
                };
                ProjectedPoint { x, y, depth }
            };
            let a = point(&mut rng);
            let mut b = point(&mut rng);
            let mut c = point(&mut rng);
            if case % 7 == 0 {
                // Sliver: c nearly on the line ab.
                let t = rng.range(-0.5, 1.5);
                c.x = a.x + (b.x - a.x) * t + rng.range(-1.0e-6, 1.0e-6);
                c.y = a.y + (b.y - a.y) * t;
            }
            if case % 11 == 0 {
                b.y = a.y; // horizontal edge: constant weight along a row
            }
            let color = [ORDINARY_TOP, ORDINARY_SIDE, BEDROCK_TOP][rng.below(3) as usize];

            let mut reference = Renderer::new(width, height);
            for (index, (pixel, depth)) in reference
                .pixels
                .iter_mut()
                .zip(reference.depth.iter_mut())
                .enumerate()
            {
                if (index as u64 + case).is_multiple_of(3) {
                    *depth = [0.5, 2.0, 7.0, f64::INFINITY][index % 4];
                    *pixel = [ORDINARY_BACK, ORDINARY_FRONT, BACKGROUND, BEDROCK_SIDE][index % 4];
                }
            }
            let mut fast = Renderer::new(width, height);
            fast.pixels.clone_from(&reference.pixels);
            fast.depth.clone_from(&reference.depth);
            reference.reference_traversal = true;

            reference.raster_triangle(a, b, c, color);
            fast.raster_triangle(a, b, c, color);
            if span_raster_applies(a, b, c) {
                spans += 1;
            }
            assert_eq!(
                fast.pixels, reference.pixels,
                "case {case}: {a:?} {b:?} {c:?}"
            );
            assert!(
                fast.depth
                    .iter()
                    .zip(&reference.depth)
                    .all(|(x, y)| x.to_bits() == y.to_bits()),
                "case {case}: depth differs"
            );
            assert_eq!(fast.world_pixels, reference.world_pixels, "case {case}");
        }
        assert!(spans > 4000, "span path exercised {spans} times");
    }

    // `!(a < b)` is deliberate: it is true for NaN, which `a >= b` is not.
    /// The closed-form run boundary in `RowEdge::accepted` against a linear scan of the exact
    /// per-pixel predicate, on rows built to put the real crossing within a few ulps (or
    /// exactly on) a pixel column, with slopes down to `CROSSING_MIN_SLOPE`, huge offsets, both
    /// windings and tiny or huge areas. A wrong closed-form answer fails here even where the
    /// search fallback would have been taken by a looser bound.
    #[test]
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    fn closed_form_edge_boundary_matches_exact_scan() {
        let mut rng = Rng(0xc105_edf0_4a11);
        let width = 64_i64;
        let mut closed = 0_u64;
        for case in 0..200_000_u64 {
            let ax = match rng.below(5) {
                0 => rng.range(-40.0, 100.0),
                1 => rng.range(-1.0e6, 1.0e6),
                2 => rng.range(-1.0e15, 1.0e15),
                3 => (rng.range(-40.0, 100.0)).round() + 0.5,
                _ => rng.range(0.0, 64.0),
            };
            let k = match rng.below(5) {
                0 => rng.range(-2.0, 2.0),
                1 => rng.range(-1.0e3, 1.0e3),
                2 => {
                    CROSSING_MIN_SLOPE
                        * rng.range(1.0, 1.5)
                        * if rng.below(2) == 0 { 1.0 } else { -1.0 }
                }
                3 => rng.range(-1.0e-6, 1.0e-6),
                _ => (rng.range(-8.0, 8.0)).round(),
            };
            if k == 0.0 {
                continue;
            }
            let area = match rng.below(4) {
                0 => rng.range(-1.0e4, 1.0e4),
                1 => {
                    f64::EPSILON * rng.range(1.0, 4.0) * if rng.below(2) == 0 { 1.0 } else { -1.0 }
                }
                2 => rng.range(-1.0e20, 1.0e20),
                _ => rng.range(-1.0, 1.0),
            };
            if !area.is_finite() || area.abs() < f64::EPSILON {
                continue;
            }
            let p = ProjectedPoint {
                x: ax,
                y: 3.0,
                depth: 1.0,
            };
            let q = ProjectedPoint {
                x: ax + 1.0,
                y: 3.0 + k,
                depth: 1.0,
            };
            let mut edge = RowEdge::new(p, q, area, width as f64);
            // Aim the real crossing at a column, then nudge it by a few ulps or a hair.
            let column = rng.below(width as u64 + 8) as f64 - 4.0;
            let nudge = match rng.below(5) {
                0 => 0.0,
                1 => rng.range(-1.0e-12, 1.0e-12),
                2 => rng.range(-1.0e-8, 1.0e-8),
                3 => rng.range(-1.0e-15, 1.0e-15) * (column - ax).abs().max(1.0),
                _ => rng.range(-0.5, 0.5),
            };
            let target = column + PIXEL_CENTER_OFFSET + nudge;
            let mut r = (target - ax) * k - BARYCENTRIC_EPSILON * area;
            for _ in 0..rng.below(3) {
                r = f64::from_bits(r.to_bits().wrapping_add(rng.below(5)).wrapping_sub(2));
            }
            if !r.is_finite() {
                continue;
            }
            edge.r = r;
            let lo = rng.below(8) as i64;
            let hi = width - rng.below(8) as i64;
            let exact =
                |x: i64| !(edge.value(x as f64 + PIXEL_CENTER_OFFSET) / area < BARYCENTRIC_EPSILON);
            let increasing = (k > 0.0) == (area > 0.0);
            let expected = if increasing {
                ((lo..hi).find(|&x| exact(x)).unwrap_or(hi), hi)
            } else {
                (lo, (lo..hi).find(|&x| !exact(x)).unwrap_or(hi))
            };
            assert_eq!(edge.accepted(lo, hi), expected, "case {case}: {edge:?}");
            let crossing = (ax + (r + edge.eps_area) * edge.inv_k) - PIXEL_CENTER_OFFSET;
            let bound = edge.bound_base + CROSSING_SLACK * ((r + edge.eps_area) * edge.inv_k).abs();
            if (crossing - bound).ceil() > crossing + bound {
                closed += 1;
            }
        }
        assert!(closed > 50_000, "closed form exercised {closed} times");
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

    /// The enclosed-bit cull against the unordered reference (no cull, no hidden-bit test, no
    /// frustum culling) and against the ordered traversal without it: solid balls spanning many
    /// chunks on both sides of zero, dense random blocks with cavities, cameras outside, close
    /// up, grazing and inside the world, wide and narrow fields of view, tinted and plain. With
    /// `verify_hidden_bits` on, every culled bit is also rasterised over the finished frame and
    /// must move nothing (see `assert_enclosed_bits_write_nothing`).
    #[test]
    fn enclosed_bit_cull_matches_reference_traversal() {
        let mut rng = Rng(0x00e0_c105_ed00_b175);
        let ball = |radius: i64, offset: Coord| {
            let mut world = World::new();
            for x in -radius..=radius {
                for y in -radius..=radius {
                    for z in -radius..=radius {
                        if x * x + y * y + z * z <= radius * radius {
                            world.set_on(Coord::new(x + offset.x, y + offset.y, z + offset.z));
                        }
                    }
                }
            }
            world
        };
        let mut worlds = vec![
            ("ball r7", ball(7, Coord::new(0, 0, 0))),
            ("ball r13 offset", ball(13, Coord::new(-9, 5, 17))),
            ("ball r17", ball(17, Coord::new(3, -2, 1))),
            ("solid 9", solid_world(9)),
            ("shell 14", shell_world(14)),
        ];
        for case in 0..4 {
            let extent = 5 + rng.below(8) as i64;
            let density = rng.range(0.85, 0.99);
            let mut world = World::new();
            for x in -extent..=extent {
                for y in -extent..=extent {
                    for z in -extent..=extent {
                        if rng.range(0.0, 1.0) < density {
                            world.set_on(Coord::new(x, y - 16, z + 8));
                        }
                    }
                }
            }
            worlds.push((["dense a", "dense b", "dense c", "dense d"][case], world));
        }
        let palette = [0x002f6fd6_u32, 0x00b04020, 0x00e8dcc0, 0x0040c040];
        let tint = |c: Coord| Some(palette[(c.x + 2 * c.y + 3 * c.z).rem_euclid(4) as usize]);
        let mut culled = 0_u64;
        for (name, world) in &worlds {
            let mut cameras: Vec<Camera> = views(world).into();
            let (min, max) = bounds_aabb(world.bounds().unwrap());
            let size = (max - min).length();
            for _ in 0..6 {
                let anchor = min + (max - min) * rng.range(0.2, 0.8);
                cameras.push(Camera::from_anchor(
                    anchor,
                    size * rng.range(0.05, 3.0),
                    rng.range(-PI, PI),
                    rng.range(-1.5, 1.5),
                    rng.range(0.3, 2.4),
                ));
            }
            for (index, camera) in cameras.iter().enumerate() {
                let (width, height) = (60 + rng.below(160) as u32, 40 + rng.below(140) as u32);
                for tinted in [false, true] {
                    let t: Option<BitTint<'_>> = if tinted { Some(&tint) } else { None };
                    let mut reference = Renderer::new(width, height);
                    reference.reference_traversal = true;
                    reference.render_tinted(world, camera, t);
                    let mut uncut = Renderer::new(width, height);
                    uncut.skip_enclosed_cull = true;
                    uncut.render_tinted(world, camera, t);
                    let mut ordered = Renderer::new(width, height);
                    ordered.verify_hidden_bits = true;
                    ordered.render_tinted(world, camera, t);
                    let at = format!("{name} camera {index} tinted {tinted} {width}x{height}");
                    assert_eq!(uncut.enclosed_bits, 0, "{at}");
                    for other in [&reference, &uncut] {
                        assert!(ordered.pixels == other.pixels, "{at} pixels");
                        let bits = |v: &[f64]| v.iter().map(|d| d.to_bits()).collect::<Vec<_>>();
                        assert_eq!(bits(&ordered.depth), bits(&other.depth), "{at} depth");
                        assert_eq!(
                            ordered.world_pixel_count(),
                            other.world_pixel_count(),
                            "{at}"
                        );
                    }
                    culled += ordered.enclosed_bits;
                }
            }
        }
        assert!(
            culled > 100_000,
            "the enclosed-bit cull left out only {culled} bits, so this fixture no longer exercises it"
        );
    }

    /// §6a tightened `ENCLOSED_MIN_SPAN` from 2 px to 1.25 px. Here every camera sees each
    /// world at 1.3-1.9 px per bit at its centre, so nearly every culled bit comes from the
    /// tightened band. Frames must still match the reference and
    /// uncut traversals bit for bit, and the end-of-frame check confirms every culled bit is
    /// rasterised (not single-pixel) and writes nothing.
    #[test]
    fn enclosed_bit_cull_is_exact_on_one_to_two_pixel_bits() {
        let mut rng = Rng(0x0001_25b1_75ec_a1e5);
        let ball = |radius: i64, offset: Coord| {
            let mut world = World::new();
            for x in -radius..=radius {
                for y in -radius..=radius {
                    for z in -radius..=radius {
                        if x * x + y * y + z * z <= radius * radius {
                            world.set_on(Coord::new(x + offset.x, y + offset.y, z + offset.z));
                        }
                    }
                }
            }
            world
        };
        let mut dense = World::new();
        for x in -18..=18_i64 {
            for y in -18..=18_i64 {
                for z in -18..=18_i64 {
                    if rng.range(0.0, 1.0) < 0.93 {
                        dense.set_on(Coord::new(x + 40, y, z - 7));
                    }
                }
            }
        }
        let worlds = [
            ("ball r24", ball(24, Coord::new(0, 0, 0))),
            ("ball r30 offset", ball(30, Coord::new(-21, 13, 5))),
            ("dense 18", dense),
        ];
        let palette = [0x002f6fd6_u32, 0x00b04020, 0x00e8dcc0, 0x0040c040];
        let tint = |c: Coord| Some(palette[(c.x + 2 * c.y + 3 * c.z).rem_euclid(4) as usize]);
        let mut culled = 0_u64;
        for (name, world) in &worlds {
            let (min, max) = bounds_aabb(world.bounds().unwrap());
            let centre = (min + max) * 0.5;
            for index in 0..8 {
                let (width, height) = (150 + rng.below(80) as u32, 110 + rng.below(60) as u32);
                let (yaw, pitch, fov) = (
                    rng.range(-PI, PI),
                    rng.range(-1.2, 1.2),
                    rng.range(0.5, 1.6),
                );
                let focal = Camera::from_anchor(centre, 1.0, yaw, pitch, fov)
                    .projection_for_viewport(width, height)
                    .unwrap()
                    .focal_length();
                let scale = rng.range(1.3, 1.9);
                let camera = Camera::from_anchor(centre, focal / scale, yaw, pitch, fov);
                for tinted in [false, true] {
                    let t: Option<BitTint<'_>> = if tinted { Some(&tint) } else { None };
                    let mut reference = Renderer::new(width, height);
                    reference.reference_traversal = true;
                    reference.render_tinted(world, &camera, t);
                    let mut uncut = Renderer::new(width, height);
                    uncut.skip_enclosed_cull = true;
                    uncut.render_tinted(world, &camera, t);
                    let mut ordered = Renderer::new(width, height);
                    ordered.verify_enclosed_bits = true;
                    ordered.render_tinted(world, &camera, t);
                    let at = format!("{name} camera {index} scale {scale:.2} tinted {tinted}");
                    for other in [&reference, &uncut] {
                        assert!(ordered.pixels == other.pixels, "{at} pixels");
                        let bits = |v: &[f64]| v.iter().map(|d| d.to_bits()).collect::<Vec<_>>();
                        assert_eq!(bits(&ordered.depth), bits(&other.depth), "{at} depth");
                    }
                    culled += ordered.enclosed_bits;
                }
            }
        }
        // ~1.41M bits culled at 1.25 px; the old 2 px bound culled ~15k (near-side chunks only).
        assert!(
            culled > 500_000,
            "the 1.25-2 px band culled only {culled} bits, so this fixture no longer exercises it"
        );
    }

    /// The case that rules out a six-face-neighbour cull: the recursion gallery world seen
    /// dead frontal (yaw 0, pitch 0) from the gallery camera, which puts pixel centres exactly
    /// on concave crease lines. There a bit with all six face neighbours on ties the visible
    /// faces in depth to the last bit and the lower-RGB rule picks its face, so leaving it out
    /// changes 35 pixels. The 3 x 3 x 3 cull must leave every pose here identical.
    #[test]
    fn enclosed_bit_cull_is_exact_on_dead_frontal_creases() {
        let mut world = World::new();
        for line in include_str!("../../../docs/gallery/recursion.bb").lines() {
            let v: Vec<i64> = line
                .split_whitespace()
                .skip(1)
                .map(|t| t.parse().expect("recursion.bb coordinate"))
                .collect();
            if let [x, y, z] = v[..] {
                world.set_on(Coord::new(x, y, z));
            }
        }
        let (min, max) = bounds_aabb(world.bounds().unwrap());
        let center = min + (max - min) * 0.5;
        let fov = crate::camera::DEFAULT_VERTICAL_FOV_RADIANS;
        let distance = (max - min).length() / 2.0 / (fov / 2.0).tan() * 1.28;
        let mut culled = 0;
        for (size, yaw, pitch) in [
            (801_u32, 0.0_f64, 0.0_f64),
            (401, 0.0, 0.0),
            (801, 90.0, 0.0),
            (801, 180.0, 0.0),
            (801, 0.0, 45.0),
        ] {
            let camera =
                Camera::from_anchor(center, distance, yaw.to_radians(), pitch.to_radians(), fov);
            let mut reference = Renderer::new(size, size);
            reference.reference_traversal = true;
            reference.render(&world, &camera);
            let mut ordered = Renderer::new(size, size);
            ordered.verify_enclosed_bits = true;
            ordered.render(&world, &camera);
            let at = format!("{size} yaw {yaw} pitch {pitch}");
            assert!(ordered.pixels == reference.pixels, "{at} pixels");
            let bits = |v: &[f64]| v.iter().map(|d| d.to_bits()).collect::<Vec<_>>();
            assert_eq!(bits(&ordered.depth), bits(&reference.depth), "{at} depth");
            culled += ordered.enclosed_bits;
        }
        assert!(culled > 20_000, "only {culled} bits culled");
    }

    /// §6f delta frames: after random toggles and recolourings (enclosed bits included), a
    /// delta render over the previous frame equals a fresh full render bit for bit -- pixels,
    /// depths and `world_pixels` -- with the crosshair drawn between frames, from far, near and
    /// close cameras, and with every bit the hidden-bit test skips checked to write nothing.
    /// §6k: run again with no index slack and longer chains, so compactions (and the full path
    /// after one) happen between frames. §6p: bits already indexed are not recorded again, so
    /// the index grows more slowly and fewer compactions happen (at least 3 still do).
    #[test]
    fn delta_render_matches_full_render() {
        let mut compacted = 0;
        for slack in [4096_usize, 0] {
            compacted += delta_render_matches_full_render_with(slack);
        }
        assert!(compacted >= 3, "only {compacted} compactions");
    }

    fn delta_render_matches_full_render_with(slack: usize) -> u64 {
        let mut seed = 0x2545_f491_4f6c_dd1d_u64;
        let mut next = move |n: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % n
        };
        let palette = [
            0x002f6fd6_u32,
            0x00b04020,
            0x00e8dcc0,
            0x0040c060,
            0x00909090,
        ];
        let mut used = 0;
        let mut culled = 0;
        let mut compactions = 0;
        for (case, size) in [(0_usize, 161_u32), (1, 97), (2, 161), (3, 120), (4, 161)] {
            let mut world = World::new();
            let mut colours: std::collections::HashMap<Coord, u32> =
                std::collections::HashMap::new();
            for x in -7_i64..=7 {
                for y in -7_i64..=7 {
                    for z in -7_i64..=7 {
                        if x * x + y * y + z * z <= 42 {
                            let c = Coord::new(x, y, z);
                            world.set_on(c);
                            colours.insert(c, palette[next(5) as usize]);
                        }
                    }
                }
            }
            let camera = views(&world)[case].clone();
            let mut delta = Renderer::new(size, size);
            delta.verify_hidden_bits = size < 150;
            delta.delta_slack = slack;
            {
                let tint = |c: Coord| colours.get(&c).copied();
                delta.render_tinted(&world, &camera, Some(&tint));
            }
            culled += delta.enclosed_bits;
            delta.draw_crosshair(crosshair_overlay_input());
            for frame in 0..if slack == 0 { 30 } else { 10 } {
                let mut changed = Vec::new();
                let toggles = if frame % 4 == 3 { 0 } else { 1 + next(12) };
                for _ in 0..toggles {
                    let c = Coord::new(
                        next(17) as i64 - 8,
                        next(17) as i64 - 8,
                        next(17) as i64 - 8,
                    );
                    if world.is_on(c) {
                        world.set_off(c);
                    } else {
                        world.set_on(c);
                        colours.insert(c, palette[next(5) as usize]);
                    }
                    changed.push((c, true));
                }
                // A drill hole along one axis, from outside the ball to its middle: it uncovers
                // bits that were deeply enclosed (left out of the index by the cull).
                if frame % 3 != 2 {
                    let (u, v) = (next(7) as i64 - 3, next(7) as i64 - 3);
                    let sign = if next(2) == 0 { 1 } else { -1 };
                    for d in 2..=7_i64 {
                        let c = match frame % 3 {
                            0 => Coord::new(u, v, sign * d),
                            _ => Coord::new(sign * d, u, v),
                        };
                        if world.is_on(c) {
                            world.set_off(c);
                            changed.push((c, true));
                        }
                    }
                }
                for _ in 0..next(40) {
                    let c = Coord::new(
                        next(15) as i64 - 7,
                        next(15) as i64 - 7,
                        next(15) as i64 - 7,
                    );
                    colours.insert(c, palette[next(5) as usize]);
                    changed.push((c, false));
                }
                let tint = |c: Coord| colours.get(&c).copied();
                used +=
                    usize::from(delta.render_tinted_delta(&world, &camera, Some(&tint), &changed));
                delta.draw_crosshair(crosshair_overlay_input());
                let mut full = Renderer::new(size, size);
                full.render_tinted(&world, &camera, Some(&tint));
                full.draw_crosshair(crosshair_overlay_input());
                let at = format!("case {case} frame {frame}");
                assert!(delta.pixels == full.pixels, "{at} pixels");
                let bits = |v: &[f64]| v.iter().map(|d| d.to_bits()).collect::<Vec<_>>();
                assert_eq!(bits(&delta.depth), bits(&full.depth), "{at} depth");
                assert_eq!(delta.world_pixels, full.world_pixels, "{at} world pixels");
                // A tie the full frame sees, the delta frame's record must not deny.
                let denied = delta
                    .tie
                    .iter()
                    .zip(&full.tie)
                    .filter(|&(&d, &f)| f && !d)
                    .count();
                assert_eq!(denied, 0, "{at} tie record");
            }
            compactions += delta.delta_compactions;
        }
        assert!(used >= 30, "delta path taken only {used} times");
        assert!(culled > 0, "no case exercised the enclosed-bit cull");
        compactions
    }

    /// Tint-only delta frames (the recolour pass) must match a full render: a solid ball, a
    /// sparse random block (most bits exposed, like a gas), and a slab, at sizes down to where
    /// bits fall to the single-pixel path, with long recolour chains (colours going up and down
    /// the RGB order, so equal-depth ties flip both ways) and an occasional toggle frame between.
    #[test]
    fn delta_recolour_matches_full_render() {
        let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
        let mut next = move |n: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % n
        };
        let palette = [
            0x002f6fd6_u32,
            0x002f6fd7,
            0x00b04020,
            0x00e8dcc0,
            0x0040c060,
            0x00909090,
            0x00909091,
        ];
        let mut recolours = 0;
        let mut marked = 0;
        let mut reused = 0;
        let mut kept = 0;
        let (mut toggle_skipped, mut toggle_proved) = (0, 0);
        for (shape, case, size) in [
            (0, 0_usize, 161_u32),
            (0, 3, 97),
            (1, 1, 161),
            (1, 2, 120),
            (1, 0, 40),
            (2, 0, 161),
            (2, 2, 64),
            (0, 0, 24),
        ] {
            let mut world = World::new();
            let mut colours: std::collections::HashMap<Coord, u32> =
                std::collections::HashMap::new();
            for x in -6_i64..=6 {
                for y in -6_i64..=6 {
                    for z in -6_i64..=6 {
                        let on = match shape {
                            0 => x * x + y * y + z * z <= 36,
                            1 => next(2) == 0,
                            _ => y.abs() <= 1,
                        };
                        if on {
                            let c = Coord::new(x, y, z);
                            world.set_on(c);
                            colours.insert(c, palette[next(7) as usize]);
                        }
                    }
                }
            }
            let camera = views(&world)[case].clone();
            let mut delta = Renderer::new(size, size);
            delta.verify_hidden_bits = size < 150;
            {
                let tint = |c: Coord| colours.get(&c).copied();
                delta.render_tinted(&world, &camera, Some(&tint));
            }
            delta.draw_crosshair(crosshair_overlay_input());
            for frame in 0..16 {
                let mut changed = Vec::new();
                if frame % 7 == 6 {
                    let c = Coord::new(
                        next(13) as i64 - 6,
                        next(13) as i64 - 6,
                        next(13) as i64 - 6,
                    );
                    if world.is_on(c) {
                        world.set_off(c);
                    } else {
                        world.set_on(c);
                    }
                    changed.push((c, true));
                }
                for _ in 0..1 + next(300) {
                    let c = Coord::new(
                        next(13) as i64 - 6,
                        next(13) as i64 - 6,
                        next(13) as i64 - 6,
                    );
                    colours.insert(c, palette[next(7) as usize]);
                    changed.push((c, false));
                }
                let tint = |c: Coord| colours.get(&c).copied();
                delta.render_tinted_delta(&world, &camera, Some(&tint), &changed);
                delta.draw_crosshair(crosshair_overlay_input());
                let mut full = Renderer::new(size, size);
                full.render_tinted(&world, &camera, Some(&tint));
                full.draw_crosshair(crosshair_overlay_input());
                let at = format!("shape {shape} case {case} size {size} frame {frame}");
                assert!(delta.pixels == full.pixels, "{at} pixels");
                let bits = |v: &[f64]| v.iter().map(|d| d.to_bits()).collect::<Vec<_>>();
                assert_eq!(bits(&delta.depth), bits(&full.depth), "{at} depth");
                assert_eq!(delta.world_pixels, full.world_pixels, "{at} world pixels");
                // A tie the full frame sees, the delta frame's record must not deny.
                let denied = delta
                    .tie
                    .iter()
                    .zip(&full.tie)
                    .filter(|&(&d, &f)| f && !d)
                    .count();
                assert_eq!(denied, 0, "{at} tie record");
            }
            recolours += delta.delta_recolours;
            marked += delta.recolour_marked;
            reused += delta.recolour_reused;
            kept += delta.recolour_kept;
            toggle_skipped += delta.toggle_skipped;
            toggle_proved += delta.toggle_proved;
        }
        assert!(kept > 0, "no toggling frame kept a hidden proof");
        assert!(toggle_skipped > 0, "no toggling frame dropped a proved bit");
        assert!(toggle_proved > 0, "no toggling redraw proved a bit hidden");
        assert!(
            recolours >= 100,
            "recolour pass taken only {recolours} times"
        );
        assert!(marked > 0, "no recolour frame marked a pixel");
        assert!(reused > 0, "no recolour frame reused a hidden proof");
    }

    /// §6s: a bit a delta frame redraws (its box reaches a reset or marked pixel) does not take
    /// its own fragments of the frame before, at the pixels it keeps, for second ones. While
    /// bits are only added (no fragment of the frame before goes away) and others recoloured,
    /// the delta frames' `tie` record stays exactly the full render's: no pixel claims a tie the
    /// full frame does not have, so the recolour pass marks no more pixels than after a fresh
    /// full render. (Before, each redrawn bit set `tie` at every pixel it kept.)
    #[test]
    fn delta_redraws_keep_the_full_tie_record() {
        let mut seed = 0x2545_f491_4f6c_dd1d_u64;
        let mut next = move |n: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % n
        };
        let palette = [
            0x002f6fd6_u32,
            0x002f6fd7,
            0x00b04020,
            0x0040c060,
            0x00909090,
        ];
        for (case, size) in [(0_usize, 161_u32), (2, 120), (3, 97)] {
            let mut world = World::new();
            let mut colours: std::collections::HashMap<Coord, u32> =
                std::collections::HashMap::new();
            let mut shell = Vec::new();
            for x in -7_i64..=7 {
                for y in -7_i64..=7 {
                    for z in -7_i64..=7 {
                        let r = x * x + y * y + z * z;
                        let c = Coord::new(x, y, z);
                        if r <= 25 {
                            world.set_on(c);
                            colours.insert(c, palette[next(5) as usize]);
                        } else if r <= 36 {
                            shell.push(c);
                        }
                    }
                }
            }
            let camera = views(&world)[case].clone();
            let mut delta = Renderer::new(size, size);
            {
                let tint = |c: Coord| colours.get(&c).copied();
                delta.render_tinted(&world, &camera, Some(&tint));
            }
            let mut toggle_frames = 0;
            for frame in 0..24 {
                let mut changed = Vec::new();
                if frame % 2 == 0 {
                    for _ in 0..1 + next(6) {
                        let c = shell[next(shell.len() as u64) as usize];
                        if !world.is_on(c) {
                            world.set_on(c);
                            colours.insert(c, palette[next(5) as usize]);
                            changed.push((c, true));
                        }
                    }
                }
                for _ in 0..1 + next(200) {
                    let c = Coord::new(
                        next(13) as i64 - 6,
                        next(13) as i64 - 6,
                        next(13) as i64 - 6,
                    );
                    colours.insert(c, palette[next(5) as usize]);
                    changed.push((c, false));
                }
                toggle_frames += usize::from(changed.iter().any(|&(_, t)| t));
                let tint = |c: Coord| colours.get(&c).copied();
                assert!(delta.render_tinted_delta(&world, &camera, Some(&tint), &changed));
                let mut full = Renderer::new(size, size);
                full.render_tinted(&world, &camera, Some(&tint));
                let at = format!("case {case} size {size} frame {frame}");
                assert!(delta.pixels == full.pixels, "{at} pixels");
                assert!(delta.tie == full.tie, "{at} tie record");
            }
            assert!(
                toggle_frames >= 8,
                "case {case}: {toggle_frames} toggle frames"
            );
        }
    }

    /// The delta index must keep bits a delta frame uncovered: drill to the deeply enclosed
    /// middle of a ball facing the camera, then add and later remove a plate floating in front
    /// of the hole (not adjacent to anything), whose removal must redraw the hole's floor.
    #[test]
    fn delta_render_keeps_uncovered_bits_indexed() {
        let mut world = World::new();
        for x in -7_i64..=7 {
            for y in -7_i64..=7 {
                for z in -7_i64..=7 {
                    if x * x + y * y + z * z <= 42 {
                        world.set_on(Coord::new(x, y, z));
                    }
                }
            }
        }
        let tint = |c: Coord| {
            Some(if (c.x + c.y + c.z).rem_euclid(2) == 0 {
                0x00b04020
            } else {
                0x002f6fd6
            })
        };
        for view in 0..2 {
            let mut world = world.clone();
            let camera = views(&world)[view].clone();
            let p = camera.position();
            // The axis pointing at the camera, and the cells along it.
            let axis = |d: i64| {
                let (ax, ay, az) = (p.x.abs(), p.y.abs(), p.z.abs());
                if ax >= ay && ax >= az {
                    Coord::new(d * p.x.signum() as i64, 0, 0)
                } else if ay >= az {
                    Coord::new(0, d * p.y.signum() as i64, 0)
                } else {
                    Coord::new(0, 0, d * p.z.signum() as i64)
                }
            };
            let plate = |d: i64| -> Vec<Coord> {
                let c = axis(d);
                let mut v = Vec::new();
                for a in -1..=1_i64 {
                    for b in -1..=1_i64 {
                        v.push(if c.x != 0 {
                            Coord::new(c.x, a, b)
                        } else if c.y != 0 {
                            Coord::new(a, c.y, b)
                        } else {
                            Coord::new(a, b, c.z)
                        });
                    }
                }
                v
            };
            let mut delta = Renderer::new(161, 161);
            delta.render_tinted(&world, &camera, Some(&tint));
            let mut steps: Vec<Vec<(Coord, bool)>> = Vec::new();
            steps.push((2..=7).map(|d| (axis(d), true)).collect());
            steps.push(plate(10).into_iter().map(|c| (c, true)).collect());
            steps.push(plate(10).into_iter().map(|c| (c, true)).collect());
            for (step, changed) in steps.iter().enumerate() {
                for &(c, _) in changed {
                    if world.is_on(c) {
                        world.set_off(c);
                    } else {
                        world.set_on(c);
                    }
                }
                assert!(
                    delta.render_tinted_delta(&world, &camera, Some(&tint), changed),
                    "view {view} step {step} took the full path"
                );
                let mut full = Renderer::new(161, 161);
                full.render_tinted(&world, &camera, Some(&tint));
                assert!(
                    delta.pixels == full.pixels,
                    "view {view} step {step} pixels"
                );
                assert_eq!(
                    delta.world_pixels, full.world_pixels,
                    "view {view} step {step}"
                );
            }
        }
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

    #[test]
    fn chunk_frustum_culling_matches_reference_traversal_across_far_extrema_and_offscreen_chunks() {
        let mut world = World::new();
        for &sx in &[-1_i64, 1] {
            for &sy in &[-1_i64, 1] {
                for &sz in &[-1_i64, 1] {
                    world.set_on(Coord::new(sx * 1_048_576, sy * 1_048_576, sz * 1_048_576));
                }
            }
        }
        for coord in [
            Coord::new(0, 0, 2),
            Coord::new(-32, 0, -32),
            Coord::new(64, 64, 16),
            Coord::new(-64, -64, 16),
        ] {
            world.set_on(coord);
        }

        let bounds = world.bounds().expect("non-empty world bounds");
        let (bmin, bmax) = bounds_aabb(bounds);
        assert_eq!(bmin, Vec3::splat(-1_048_576.5));
        assert_eq!(bmax, Vec3::splat(1_048_576.5));

        let camera = Camera::for_observer_spawn();
        let mut reference = Renderer::new(64, 64);
        assert!(!reference.is_reference_traversal());
        reference.set_reference_traversal(true);
        assert!(reference.is_reference_traversal());
        reference.render(&world, &camera);

        let mut optimized = Renderer::new(64, 64);
        optimized.set_reference_traversal(false);
        assert!(!optimized.is_reference_traversal());
        optimized.verify_hidden_bits = true;
        optimized.render(&world, &camera);

        assert_eq!(optimized.pixels(), reference.pixels());
        let depth_diff = optimized
            .depth
            .iter()
            .zip(&reference.depth)
            .position(|(a, b)| a.to_bits() != b.to_bits());
        assert_eq!(depth_diff, None);
        assert_eq!(optimized.world_pixel_count(), reference.world_pixel_count());
    }

    #[test]
    fn ray_aabb_entry_distance_and_slab_pruned_queries_match_reference_oracles() {
        let (min, max) = bounds_aabb(Bounds {
            min: BEDROCK_COORD,
            max: BEDROCK_COORD,
        });
        let hit_ray = Ray {
            origin: Vec3::new(0.0, 0.0, -4.0),
            direction: Vec3::Z,
        };
        assert_close(
            ray_aabb_entry_distance(hit_ray, min, max).expect("ray hits origin AABB"),
            3.5,
        );
        let inside_ray = Ray {
            origin: Vec3::ZERO,
            direction: Vec3::Z,
        };
        assert_eq!(ray_aabb_entry_distance(inside_ray, min, max), Some(0.0));
        let miss_ray = Ray {
            origin: Vec3::new(0.0, 0.0, 4.0),
            direction: Vec3::Z,
        };
        assert_eq!(ray_aabb_entry_distance(miss_ray, min, max), None);

        let mut world = World::new();
        for coord in [
            Coord::new(-1, -1, -1),
            Coord::new(0, 0, 1),
            Coord::new(16, 0, 0),
            Coord::new(0, 0, 16),
            Coord::new(-16, -16, -16),
            Coord::new(1_048_576, 1_048_576, 1_048_576),
            Coord::new(-1_048_576, -1_048_576, -1_048_576),
        ] {
            world.set_on(coord);
        }

        // Negative-zero ray direction boundary case (`-0.0` vs `+0.0` slab entry).
        let neg_zero_ray = Ray {
            origin: Vec3::new(-0.5, 0.0, -5.0),
            direction: Vec3::new(-0.0, -0.0, 1.0),
        };
        assert_eq!(
            raycast(&world, neg_zero_ray),
            raycast_reference(&world, neg_zero_ray)
        );
        assert!(raycast(&world, neg_zero_ray).is_some());

        // Sub-ULP slab boundary case (`start = -0.5 - f64::EPSILON, delta = 2.0`).
        let boundary_start = Vec3::new(-0.5 - f64::EPSILON, 0.0, 0.0);
        let boundary_delta = Vec3::new(2.0, 0.0, 0.0);
        assert_eq!(
            sweep_point(&world, boundary_start, boundary_delta),
            sweep_point_reference(&world, boundary_start, boundary_delta)
        );
        assert!(sweep_point(&world, boundary_start, boundary_delta).is_some());

        // Seeded randomized multi-chunk parity corpus for raycast and sweep_point.
        let mut rng = Rng(0x517c_c1b7_2722_0a95);
        for trial in 0..128 {
            let mut trial_world = World::new();
            for _ in 0..24 {
                let cx = (rng.below(9) as i64 - 4) * 16 + rng.below(4) as i64;
                let cy = (rng.below(9) as i64 - 4) * 16 + rng.below(4) as i64;
                let cz = (rng.below(9) as i64 - 4) * 16 + rng.below(4) as i64;
                trial_world.set_on(Coord::new(cx, cy, cz));
            }
            let origin = Vec3::new(
                rng.range(-40.0, 40.0),
                rng.range(-40.0, 40.0),
                rng.range(-40.0, 40.0),
            );
            let delta = Vec3::new(
                rng.range(-80.0, 80.0),
                rng.range(-80.0, 80.0),
                rng.range(-80.0, 80.0),
            );
            if let Some(direction) = delta.try_normalized() {
                let ray = Ray { origin, direction };
                assert_eq!(
                    raycast(&trial_world, ray),
                    raycast_reference(&trial_world, ray),
                    "raycast mismatch on trial {trial}"
                );
            }
            assert_eq!(
                sweep_point(&trial_world, origin, delta),
                sweep_point_reference(&trial_world, origin, delta),
                "sweep_point mismatch on trial {trial}"
            );
        }
    }

    #[test]
    fn coord_containing_rejects_two_to_the_63_saturation() {
        let two_pow_63 = i64::MAX as f64;
        assert_eq!(coord_containing(Vec3::new(two_pow_63, 0.0, 0.0)), None);
        assert_eq!(coord_containing(Vec3::new(0.0, two_pow_63, 0.0)), None);
        assert_eq!(coord_containing(Vec3::new(0.0, 0.0, two_pow_63)), None);
        assert_eq!(
            coord_containing(Vec3::new(i64::MIN as f64, 0.0, 0.0)),
            Some(Coord::new(i64::MIN, 0, 0))
        );
    }
}
