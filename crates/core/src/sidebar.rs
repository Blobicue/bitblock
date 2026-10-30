//! A separately clipped, content-bound metrics panel and full-client compositor.
//!
//! The simulation renderer never sees the sidebar width. A full composition copies its pixels
//! row-for-row into the left region; a sidebar-only composition reuses that left region only when
//! it is bound to the exact selected game-frame tag. Text is rasterized only inside the right
//! region. Dynamic sidebar data is a `MetricSnapshot`; the composite frame input binds that
//! snapshot, the exact game frame, and the named layout policy without making a receipt depend on
//! itself.

use std::sync::OnceLock;

use crate::camera::Vec3;
use crate::frame::{FrameProtocolError, OverlayInput};
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::render::{crosshair_policy_tag, renderer_profile_tag};
use crate::world::{Bounds, Coord};

/// Fixed horizontal width in pixels reserved for the F3 diagnostic metrics sidebar panel.
pub const SIDEBAR_WIDTH: u32 = 400;

const SNAPSHOT_SCHEMA: &[u8] = b"bitblock/sidebar/metric-snapshot/v1";
const SNAPSHOT_PAYLOAD_LEN: u64 = 368;
const SIDEBAR_POLICY_SCHEMA: &[u8] = b"bitblock/sidebar/raster-policy/v1";
const COMPOSITE_POLICY_SCHEMA: &[u8] = b"bitblock/sidebar/composite-policy/v1";
const COMPOSITE_INPUT_SCHEMA: &[u8] = b"bitblock/sidebar/composite-frame-input/v1";
const SIDEBAR_POLICY_TEXT: &[u8] =
    b"font=bitblock-3x5-ascii/v1;glyph-scale=1;row-gap=2;predecessor-only=v1";
const SIDEBAR_POLICY_FIXED_BYTES: u64 =
    size_of::<u32>() as u64 + 5 * size_of::<u32>() as u64 + SIDEBAR_POLICY_TEXT.len() as u64;

static SIDEBAR_POLICY_TAG: OnceLock<ContentTag> = OnceLock::new();
static COMPOSITE_POLICY_TAG: OnceLock<ContentTag> = OnceLock::new();

const PANEL_BACKGROUND: u32 = 0x0010_1318;
const PANEL_RULE: u32 = 0x0038_424d;
const HEADING: u32 = 0x00f2_c14e;
const LABEL: u32 = 0x008d_a2b8;
const VALUE: u32 = 0x00e8_edf2;

/// Static pairing between a human-readable sidebar metric alias and its canonical registry definition tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MetricBinding {
    pub alias: &'static str,
    pub definition: &'static str,
}

macro_rules! binding {
    ($alias:literal, $definition:literal) => {
        MetricBinding {
            alias: $alias,
            definition: $definition,
        }
    };
}

/// Every source family used by the first sidebar page has one unique registry binding.
/// Derived presentation views such as aspect and simulated age reuse their bound inputs.
pub const DISPLAYED_METRICS: &[MetricBinding] = &[
    binding!(
        "#bb.metric.world.occupied-bit-population",
        "#bb.metric-definition.sha256.xdihkp3mq6ovlems6u6rjsp6d7kp7slnqd55c3mc2rnr6dltijta"
    ),
    binding!(
        "#bb.metric.world.mutable-occupied-bit-population",
        "#bb.metric-definition.sha256.sztpdocvi2yv442ctsvh7lyqd22kbdxwk52asazvcbswxi3iyxya"
    ),
    binding!(
        "#bb.metric.world.coordinate-bounds",
        "#bb.metric-definition.sha256.4t5rhohfmpkpt7nfkzd6iesnbejwuaua65jlflshdrcahabugaqa"
    ),
    binding!(
        "#bb.metric.cam.observer-position",
        "#bb.metric-definition.sha256.xgvfvh3x4scm6r2n5xwcr4pl4irj6yvn2rzlbmmwlazk25actr6q"
    ),
    binding!(
        "#bb.metric.cam.observer-cell",
        "#bb.metric-definition.sha256.mnnyoro62sjuy35r5uxhzscvdqnjakuhjbcpqmgdh6g7ctjor26a"
    ),
    binding!(
        "#bb.metric.cam.yaw",
        "#bb.metric-definition.sha256.a47lgvzs6afcdjipljyckw2zrdytdxzrohytwcpohurjh2tv5uqa"
    ),
    binding!(
        "#bb.metric.cam.pitch",
        "#bb.metric-definition.sha256.bji2ssbth7qla3rbzb6zreekmf4lkswxln6zn2eakz4mlwfoufkq"
    ),
    binding!(
        "#bb.metric.cam.observation-distance",
        "#bb.metric-definition.sha256.ibckdmfmi7jssjjugwoyqo7tna62qikjmzjgnacb2xqn6lgoab5a"
    ),
    binding!(
        "#bb.metric.cam.origin-distance",
        "#bb.metric-definition.sha256.c5y5tzk3rkfy46fszvpes6veb4xtta5zxiza6wnhioypijs5aqlq"
    ),
    binding!(
        "#bb.metric.view.physical-viewport-size",
        "#bb.metric-definition.sha256.jhatyfbhfhsp7hcmid2do5fzfxrsugvgskfuwjf2t56oekl6elna"
    ),
    binding!(
        "#bb.metric.view.viewport-pixel-count",
        "#bb.metric-definition.sha256.qeoxn4ncsc5wu4pmagaj4aznrp4cmz6g65vv5dur4wnmtxibdapa"
    ),
    binding!(
        "#bb.metric.view.top-saturation-state",
        "#bb.metric-definition.sha256.y5tsp2gkm73sxmwlqtymzduqyysm4boyvvhrq7bo7qav542m2rfq"
    ),
    binding!(
        "#bb.metric.view.active-scale-mode",
        "#bb.metric-definition.sha256.n2f6gqidbhxpcc72mi2r42aw3eeh5y2y5h4vbkel3v5qsu4k7voq"
    ),
    binding!(
        "#bb.metric.render.world-pixel-count",
        "#bb.metric-definition.sha256.yxe6kxzou5fanpvta3vsxm5ymfe32iv523pna7hripem7s4cq2uq"
    ),
    binding!(
        "#bb.metric.tick.tick-index",
        "#bb.metric-definition.sha256.r6y6ivgw4itq5rjuj4p6csqmehwfoopdaqpmb57g5qvsp32jvyfq"
    ),
    binding!(
        "#bb.metric.tick.configured-tick-rate",
        "#bb.metric-definition.sha256.57dror3zan62nbbuw2tocn7vd426liatn7fs3gjt2iqzpeeybkpq"
    ),
    binding!(
        "#bb.metric.hash.canonical-world-state-root",
        "#bb.metric-definition.sha256.2wqnpxvk7usvvnwjhj4gm47zggllqguklkcexkt4sgpiwgbmhmca"
    ),
    binding!(
        "#bb.metric.hash.receipt-chain-head",
        "#bb.metric-definition.sha256.xpe3h7ekeeorkwxscdneahtpjqtxqz6xqfprkmloid35vy5kfwnq"
    ),
    binding!(
        "#bb.metric.hash.receipt-chain-length",
        "#bb.metric-definition.sha256.tat7mrma3nrykbnwf5unjqrz2b7jhmup4fdnbq5us5iytmbvbxca"
    ),
    binding!(
        "#bb.metric.hash.frame-provenance-receipt-digest",
        "#bb.metric-definition.sha256.ntg7gxatkwreu2rhm7g72hunnpka7arngljoagz3bpdfozblrtma"
    ),
    binding!(
        "#bb.metric.visual.compositor-capture-comparison",
        "#bb.metric-definition.sha256.frvbp7vzozw7sag5klwkekaipni4peiv4lx4qswmmi7ypbfc7giq"
    ),
    binding!(
        "#bb.metric.sidebar.sidebar-visibility",
        "#bb.metric-definition.sha256.p4so6b2xxwpbbpk3r6gs3eyhy63avykzbclhv7a5752mqxysvmpa"
    ),
    binding!(
        "#bb.metric.sidebar.sidebar-rectangle",
        "#bb.metric-definition.sha256.q7kne43fs5lkwo7i2z7qehgk2b4zwbrziqy53f5fwon75nm7veta"
    ),
    binding!(
        "#bb.metric.sidebar.simulation-viewport-rectangle",
        "#bb.metric-definition.sha256.tkyi6wpwanrehnzqgbsyycude2p2aaqbookzkyvxmgbei7xs75jq"
    ),
    binding!(
        "#bb.metric.sidebar.row-capacity",
        "#bb.metric-definition.sha256.jdqmc5mkaeods66sklsjz6ludgabqcrm4zbvyqbkydxacr7hxlvq"
    ),
    binding!(
        "#bb.metric.sidebar.displayed-metric-count",
        "#bb.metric-definition.sha256.xgjijqxpcanjftdxw56g5656v3tfywfkt32pcfkvj4t4qcwsllmq"
    ),
    binding!(
        "#bb.metric.sidebar.visible-section-count",
        "#bb.metric-definition.sha256.zsfwtilchzehyvs5hpdkzbf7e52s7koy5nirtkf3ewqoxrlzyo2q"
    ),
    binding!(
        "#bb.metric.sidebar.unavailable-field-count",
        "#bb.metric-definition.sha256.aqxu4qky3ophuv6upvosxzojtxu5llq7jkz3guaxg5ypp36clbmq"
    ),
    binding!(
        "#bb.metric.sidebar.glyph-count",
        "#bb.metric-definition.sha256.yvk53whwmdunhe3mhrs6tfsnywkjnkfhsbt5kzptqp4xv4y5koiq"
    ),
    binding!(
        "#bb.metric.sidebar.lit-text-pixels",
        "#bb.metric-definition.sha256.q7ft5bs3gdx2msh2pxllencmbawlbe542dn3f7grhukcx2hil6qa"
    ),
    binding!(
        "#bb.metric.sidebar.clipped-text-count",
        "#bb.metric-definition.sha256.3pxbx33rhaj6ojacdd4p7jnysgbhw37yfy74tm5ep55alat7usza"
    ),
    binding!(
        "#bb.metric.sidebar.glyph-scale",
        "#bb.metric-definition.sha256.s5irx2hcvs2ltsnr6e74fotwu4odjxfn5xsucixjjpdz72fm2fkq"
    ),
    binding!(
        "#bb.metric.sidebar.minimum-simulation-view-breach",
        "#bb.metric-definition.sha256.wrdk6xdbfld4uzovn6ysv5ym2ld5mgsdslb435wjwluzmfexabka"
    ),
];

/// Predecessor display-capture verification status reported inside a sidebar metric snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum DisplayStatus {
    Unsupported = 0,
    AuditDisabled = 1,
    NoReceipt = 2,
    Match = 3,
    Mismatch = 4,
    Unavailable = 5,
}

impl DisplayStatus {
    const fn label(self) -> &'static str {
        match self {
            Self::Unsupported => "UNSUPPORTED",
            Self::AuditDisabled => "AUDIT OFF",
            Self::NoReceipt => "NO PRIOR RECEIPT",
            Self::Match => "PRIOR MATCH",
            Self::Mismatch => "PRIOR MISMATCH",
            Self::Unavailable => "PRIOR UNAVAILABLE",
        }
    }
}

/// Measured glyph, pixel, and clipping counts emitted while rasterizing the sidebar text region.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SidebarRasterFacts {
    pub displayed_metrics: u32,
    pub glyphs: u32,
    pub lit_text_pixels: u32,
    pub clipped_text: u32,
    pub unavailable_fields: u32,
}

/// Complete predecessor-bound telemetry payload hashed and rendered into the F3 sidebar panel.
#[derive(Clone, Debug, PartialEq)]
pub struct SidebarSnapshot {
    pub frame_source_tick: u64,
    /// Number of completed fixed ticks; therefore source tick + 1 for this frame.
    pub tick_index: u64,
    pub occupied_bits: u64,
    pub mutable_bits: u64,
    pub bounds: Option<Bounds>,
    pub observer_position: Vec3,
    pub observer_cell: Option<Coord>,
    pub yaw_radians: f64,
    pub pitch_radians: f64,
    pub observation_distance: f64,
    pub origin_distance: f64,
    pub simulation_width: u32,
    pub simulation_height: u32,
    pub output_width: u32,
    pub output_height: u32,
    pub sidebar_width: u32,
    pub world_pixels: u64,
    pub top_saturated: bool,
    pub world: ContentTag,
    pub history_head: ContentTag,
    pub history_len: u64,
    pub prior_frame_head: Option<ContentTag>,
    pub prior_frame_len: u64,
    pub prior_display_head: Option<ContentTag>,
    pub prior_display_len: u64,
    pub display_status: DisplayStatus,
    pub visible: bool,
    pub previous_raster: Option<SidebarRasterFacts>,
}

impl SidebarSnapshot {
    /// Computes the canonical `MetricSnapshot` content tag over all fixed-width sidebar telemetry fields.
    pub fn tag(&self) -> ContentTag {
        assert_eq!(self.world.kind(), TagKind::World);
        assert_eq!(self.history_head.kind(), TagKind::Receipt);
        assert!(
            self.prior_frame_head
                .is_none_or(|tag| tag.kind() == TagKind::FrameReceipt)
        );
        assert!(
            self.prior_display_head
                .is_none_or(|tag| tag.kind() == TagKind::DisplayReceipt)
        );
        hash_canonical(
            TagKind::MetricSnapshot,
            SNAPSHOT_SCHEMA,
            SNAPSHOT_PAYLOAD_LEN,
            |sink| self.encode_canonical(sink),
        )
        .expect("metric snapshot encoding has a fixed length")
    }

    fn encode_canonical(&self, sink: &mut dyn FnMut(&[u8])) {
        sink_u64(sink, self.frame_source_tick);
        sink_u64(sink, self.tick_index);
        sink_u64(sink, self.occupied_bits);
        sink_u64(sink, self.mutable_bits);
        sink_bounds(sink, self.bounds);
        for value in [
            self.observer_position.x,
            self.observer_position.y,
            self.observer_position.z,
            self.yaw_radians,
            self.pitch_radians,
            self.observation_distance,
            self.origin_distance,
        ] {
            sink_u64(sink, value.to_bits());
        }
        sink_coord(sink, self.observer_cell);
        for value in [
            self.simulation_width,
            self.simulation_height,
            self.output_width,
            self.output_height,
            self.sidebar_width,
        ] {
            sink(&value.to_be_bytes());
        }
        sink_u64(sink, self.world_pixels);
        sink(&[u8::from(self.top_saturated)]);
        sink(&self.world.digest());
        sink(&self.history_head.digest());
        sink_u64(sink, self.history_len);
        sink_optional_tag(sink, self.prior_frame_head);
        sink_u64(sink, self.prior_frame_len);
        sink_optional_tag(sink, self.prior_display_head);
        sink_u64(sink, self.prior_display_len);
        sink(&[self.display_status as u8]);
        sink(&[u8::from(self.visible)]);
        match self.previous_raster {
            Some(facts) => {
                sink(&[1]);
                for value in [
                    facts.displayed_metrics,
                    facts.glyphs,
                    facts.lit_text_pixels,
                    facts.clipped_text,
                    facts.unavailable_fields,
                ] {
                    sink(&value.to_be_bytes());
                }
            }
            None => {
                sink(&[0]);
                sink(&[0; 20]);
            }
        }
    }

    #[cfg(test)]
    fn canonical_bytes_v1_reference(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(360);
        reference_push_u64(&mut bytes, self.frame_source_tick);
        reference_push_u64(&mut bytes, self.tick_index);
        reference_push_u64(&mut bytes, self.occupied_bits);
        reference_push_u64(&mut bytes, self.mutable_bits);
        reference_push_bounds(&mut bytes, self.bounds);
        for value in [
            self.observer_position.x,
            self.observer_position.y,
            self.observer_position.z,
            self.yaw_radians,
            self.pitch_radians,
            self.observation_distance,
            self.origin_distance,
        ] {
            reference_push_u64(&mut bytes, value.to_bits());
        }
        reference_push_coord(&mut bytes, self.observer_cell);
        for value in [
            self.simulation_width,
            self.simulation_height,
            self.output_width,
            self.output_height,
            self.sidebar_width,
        ] {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        reference_push_u64(&mut bytes, self.world_pixels);
        bytes.push(u8::from(self.top_saturated));
        bytes.extend_from_slice(&self.world.digest());
        bytes.extend_from_slice(&self.history_head.digest());
        reference_push_u64(&mut bytes, self.history_len);
        reference_push_optional_tag(&mut bytes, self.prior_frame_head);
        reference_push_u64(&mut bytes, self.prior_frame_len);
        reference_push_optional_tag(&mut bytes, self.prior_display_head);
        reference_push_u64(&mut bytes, self.prior_display_len);
        bytes.push(self.display_status as u8);
        bytes.push(u8::from(self.visible));
        match self.previous_raster {
            Some(facts) => {
                bytes.push(1);
                for value in [
                    facts.displayed_metrics,
                    facts.glyphs,
                    facts.lit_text_pixels,
                    facts.clipped_text,
                    facts.unavailable_fields,
                ] {
                    bytes.extend_from_slice(&value.to_be_bytes());
                }
            }
            None => {
                bytes.push(0);
                bytes.extend_from_slice(&[0; 20]);
            }
        }
        bytes
    }
}

/// Canonical `FrameInput` commitment binding a simulation frame, overlay snapshot, and composite viewport geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompositeFrameInput {
    tag: ContentTag,
}

impl CompositeFrameInput {
    /// Hashes a composite `FrameInput` tag over the game frame input, rendered game frame, overlay, and dimensions.
    pub fn new(
        game_input: ContentTag,
        game_frame: ContentTag,
        overlay: OverlayInput,
        simulation_width: u32,
        simulation_height: u32,
        output_width: u32,
        output_height: u32,
    ) -> Self {
        assert_eq!(game_input.kind(), TagKind::FrameInput);
        assert_eq!(game_frame.kind(), TagKind::Frame);
        let payload_len = 32_u64 * 3 + 4_u64 * 4;
        let tag = hash_canonical(
            TagKind::FrameInput,
            COMPOSITE_INPUT_SCHEMA,
            payload_len,
            |sink| {
                sink(&game_input.digest());
                sink(&game_frame.digest());
                sink(&overlay.tag().digest());
                sink(&simulation_width.to_be_bytes());
                sink(&simulation_height.to_be_bytes());
                sink(&output_width.to_be_bytes());
                sink(&output_height.to_be_bytes());
            },
        )
        .expect("composite frame input encoding has a fixed length");
        Self { tag }
    }

    /// Returns the canonical `FrameInput` content tag for this composite client frame input.
    pub const fn tag(self) -> ContentTag {
        self.tag
    }
}

/// Wraps a `MetricSnapshot` content tag inside an `OverlayInput` governed by `composite_policy_tag`.
pub fn composite_overlay_input(snapshot: ContentTag) -> Result<OverlayInput, FrameProtocolError> {
    assert_eq!(snapshot.kind(), TagKind::MetricSnapshot);
    OverlayInput::new(composite_policy_tag(), Some(snapshot))
}

/// Returns the memoized `Policy` content tag for the 3x5 ASCII sidebar rasterizer and metric bindings.
pub fn sidebar_policy_tag() -> ContentTag {
    *SIDEBAR_POLICY_TAG.get_or_init(|| {
        hash_canonical(
            TagKind::Policy,
            SIDEBAR_POLICY_SCHEMA,
            sidebar_policy_payload_len(),
            feed_sidebar_policy,
        )
        .expect("sidebar policy byte length and exact encoding are fixed")
    })
}

fn sidebar_policy_payload_len() -> u64 {
    DISPLAYED_METRICS
        .iter()
        .fold(SIDEBAR_POLICY_FIXED_BYTES, |total, binding| {
            let alias_len =
                u64::try_from(binding.alias.len()).expect("metric alias length fits u64");
            let definition_len =
                u64::try_from(binding.definition.len()).expect("definition tag length fits u64");
            total
                .checked_add(size_of::<u16>() as u64 + alias_len)
                .and_then(|value| value.checked_add(size_of::<u16>() as u64 + definition_len))
                .expect("static sidebar policy length does not overflow")
        })
}

fn feed_sidebar_policy(sink: &mut dyn FnMut(&[u8])) {
    sink(&SIDEBAR_WIDTH.to_be_bytes());
    for color in [PANEL_BACKGROUND, PANEL_RULE, HEADING, LABEL, VALUE] {
        sink(&color.to_be_bytes());
    }
    sink(SIDEBAR_POLICY_TEXT);
    for binding in DISPLAYED_METRICS {
        let alias_len = u16::try_from(binding.alias.len()).expect("metric alias exceeds u16");
        let definition_len =
            u16::try_from(binding.definition.len()).expect("definition tag exceeds u16");
        sink(&alias_len.to_be_bytes());
        sink(binding.alias.as_bytes());
        sink(&definition_len.to_be_bytes());
        sink(binding.definition.as_bytes());
    }
}

#[cfg(test)]
fn sidebar_policy_payload_reference() -> Vec<u8> {
    let mut payload = Vec::with_capacity(
        usize::try_from(sidebar_policy_payload_len()).expect("static sidebar policy fits usize"),
    );
    payload.extend_from_slice(&SIDEBAR_WIDTH.to_be_bytes());
    for color in [PANEL_BACKGROUND, PANEL_RULE, HEADING, LABEL, VALUE] {
        payload.extend_from_slice(&color.to_be_bytes());
    }
    payload.extend_from_slice(SIDEBAR_POLICY_TEXT);
    for binding in DISPLAYED_METRICS {
        let alias_len = u16::try_from(binding.alias.len()).expect("metric alias exceeds u16");
        let definition_len =
            u16::try_from(binding.definition.len()).expect("definition tag exceeds u16");
        payload.extend_from_slice(&alias_len.to_be_bytes());
        payload.extend_from_slice(binding.alias.as_bytes());
        payload.extend_from_slice(&definition_len.to_be_bytes());
        payload.extend_from_slice(binding.definition.as_bytes());
    }
    payload
}

/// Returns the memoized `Policy` content tag uniting renderer profile, crosshair policy, and sidebar policy.
pub fn composite_policy_tag() -> ContentTag {
    *COMPOSITE_POLICY_TAG.get_or_init(|| {
        hash_canonical(TagKind::Policy, COMPOSITE_POLICY_SCHEMA, 96, |sink| {
            sink(&renderer_profile_tag().digest());
            sink(&crosshair_policy_tag().digest());
            sink(&sidebar_policy_tag().digest());
        })
        .expect("composite policy encoding has a fixed length")
    })
}

/// Single-buffer XRGB8888 client compositor uniting the left simulation viewport and right F3 sidebar panel.
pub struct FrameCompositor {
    width: u32,
    height: u32,
    simulation_width: u32,
    pixels: Vec<u32>,
    /// Exact game frame whose row prefixes currently inhabit `pixels`.
    ///
    /// Sidebar-only composition is allowed only when its selected frame agrees with this value;
    /// a resize or an unbound/failed full composition clears the cache rather than reusing stale
    /// simulation pixels.
    cached_game_frame: Option<ContentTag>,
    /// Logical simulation-prefix words copied by the most recent composition.
    ///
    /// This counts requested CPU slice-copy payload words, not hardware memory traffic, timing,
    /// cache activity, presentation work, or display output.
    last_game_prefix_copy_words: u64,
}

impl FrameCompositor {
    /// Allocates a `FrameCompositor` sized for the full output dimensions and left simulation viewport width.
    pub fn new(width: u32, height: u32, simulation_width: u32) -> Self {
        let mut compositor = Self {
            width: 0,
            height: 0,
            simulation_width: 0,
            pixels: Vec::new(),
            cached_game_frame: None,
            last_game_prefix_copy_words: 0,
        };
        compositor.resize(width, height, simulation_width);
        compositor
    }

    /// Resizes the composite XRGB8888 output buffer and invalidates any cached simulation frame prefix.
    pub fn resize(&mut self, width: u32, height: u32, simulation_width: u32) {
        assert!(simulation_width <= width);
        self.width = width;
        self.height = height;
        self.simulation_width = simulation_width;
        self.pixels.resize(pixel_len(width, height), 0);
        self.cached_game_frame = None;
        self.last_game_prefix_copy_words = 0;
    }

    /// Returns the total horizontal width in pixels of the composite client framebuffer.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Returns the vertical height in pixels of the composite client framebuffer.
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Returns the horizontal width in pixels of the left simulation viewport region.
    pub const fn simulation_width(&self) -> u32 {
        self.simulation_width
    }

    /// Returns the horizontal width in pixels of the right sidebar panel region.
    pub const fn sidebar_width(&self) -> u32 {
        self.width - self.simulation_width
    }

    /// Borrows the row-major XRGB8888 pixel slice of the composite client framebuffer.
    pub fn pixels(&self) -> &[u32] {
        &self.pixels
    }

    /// Logical game-prefix words copied by the most recent composition.
    pub const fn last_game_prefix_copy_words(&self) -> u64 {
        self.last_game_prefix_copy_words
    }

    /// Copies simulation row prefixes and rasterizes the right-hand F3 sidebar from `snapshot`.
    pub fn compose(
        &mut self,
        game_pixels: &[u32],
        game_frame: ContentTag,
        snapshot: &SidebarSnapshot,
    ) -> SidebarRasterFacts {
        assert_eq!(game_frame.kind(), TagKind::Frame);
        let facts = self.compose_unbound(game_pixels, snapshot);
        self.cached_game_frame = Some(game_frame);
        facts
    }

    /// Full composition without binding the retained game prefix to a frame identity.
    ///
    /// Produces exactly the pixels [`Self::compose`] would, but leaves the cache unbound, so a
    /// following [`Self::compose_sidebar_only`] is refused until a tagged composition runs. For
    /// callers (the browser live loop) that recompose fully and do not want to hash the game
    /// frame merely to name the cache.
    pub fn compose_unbound(
        &mut self,
        game_pixels: &[u32],
        snapshot: &SidebarSnapshot,
    ) -> SidebarRasterFacts {
        self.cached_game_frame = None;
        self.last_game_prefix_copy_words = 0;
        assert_eq!(
            game_pixels.len(),
            pixel_len(self.simulation_width, self.height)
        );
        assert_eq!(snapshot.simulation_width, self.simulation_width);
        assert_eq!(snapshot.simulation_height, self.height);
        assert_eq!(snapshot.output_width, self.width);
        assert_eq!(snapshot.output_height, self.height);
        assert_eq!(snapshot.sidebar_width, self.sidebar_width());

        let output_stride = self.width as usize;
        let game_stride = self.simulation_width as usize;
        for row in 0..self.height as usize {
            let output = &mut self.pixels[row * output_stride..(row + 1) * output_stride];
            let game = &game_pixels[row * game_stride..(row + 1) * game_stride];
            output[..game_stride].copy_from_slice(game);
        }
        self.last_game_prefix_copy_words =
            u64::from(self.simulation_width) * u64::from(self.height);
        let sidebar_width = self.sidebar_width();
        raster_sidebar(
            &mut self.pixels,
            self.width,
            self.simulation_width,
            sidebar_width,
            self.height,
            snapshot,
        )
    }

    /// Re-rasterizes only the sidebar after proving that the retained game prefix belongs to the
    /// selected game frame. This preserves the complete output buffer while avoiding a redundant
    /// simulation-region copy on sidebar-only ticks.
    pub fn compose_sidebar_only(
        &mut self,
        game_frame: ContentTag,
        snapshot: &SidebarSnapshot,
    ) -> SidebarRasterFacts {
        assert_eq!(game_frame.kind(), TagKind::Frame);
        assert_eq!(
            self.cached_game_frame,
            Some(game_frame),
            "sidebar-only composition requires the currently cached game-frame identity"
        );
        assert_eq!(snapshot.simulation_width, self.simulation_width);
        assert_eq!(snapshot.simulation_height, self.height);
        assert_eq!(snapshot.output_width, self.width);
        assert_eq!(snapshot.output_height, self.height);
        assert_eq!(snapshot.sidebar_width, self.sidebar_width());

        self.last_game_prefix_copy_words = 0;
        let sidebar_width = self.sidebar_width();
        raster_sidebar(
            &mut self.pixels,
            self.width,
            self.simulation_width,
            sidebar_width,
            self.height,
            snapshot,
        )
    }
}

/// Frozen two-buffer compositor used only as the reference arm of bounded optimization assays.
#[cfg(any(test, feature = "protocol-research"))]
pub struct TwoBufferReferenceCompositor {
    width: u32,
    height: u32,
    simulation_width: u32,
    pixels: Vec<u32>,
    sidebar_pixels: Vec<u32>,
}

#[cfg(any(test, feature = "protocol-research"))]
impl TwoBufferReferenceCompositor {
    /// Allocates the two-buffer reference compositor with separate sidebar and composite pixel buffers.
    pub fn new(width: u32, height: u32, simulation_width: u32) -> Self {
        assert!(simulation_width <= width);
        Self {
            width,
            height,
            simulation_width,
            pixels: vec![0; pixel_len(width, height)],
            sidebar_pixels: vec![PANEL_BACKGROUND; pixel_len(width - simulation_width, height)],
        }
    }

    /// Rasterizes the sidebar into a staging buffer before copying both regions into the output buffer.
    pub fn compose(
        &mut self,
        game_pixels: &[u32],
        snapshot: &SidebarSnapshot,
    ) -> SidebarRasterFacts {
        assert_eq!(
            game_pixels.len(),
            pixel_len(self.simulation_width, self.height)
        );
        assert_eq!(snapshot.simulation_width, self.simulation_width);
        assert_eq!(snapshot.simulation_height, self.height);
        assert_eq!(snapshot.output_width, self.width);
        assert_eq!(snapshot.output_height, self.height);
        let sidebar_width = self.width - self.simulation_width;
        assert_eq!(snapshot.sidebar_width, sidebar_width);

        let facts = raster_sidebar(
            &mut self.sidebar_pixels,
            sidebar_width,
            0,
            sidebar_width,
            self.height,
            snapshot,
        );
        let output_stride = self.width as usize;
        let game_stride = self.simulation_width as usize;
        let sidebar_stride = sidebar_width as usize;
        for row in 0..self.height as usize {
            let output = &mut self.pixels[row * output_stride..(row + 1) * output_stride];
            let game = &game_pixels[row * game_stride..(row + 1) * game_stride];
            let sidebar = &self.sidebar_pixels[row * sidebar_stride..(row + 1) * sidebar_stride];
            output[..game_stride].copy_from_slice(game);
            output[game_stride..].copy_from_slice(sidebar);
        }
        facts
    }

    /// Borrows the composed row-major XRGB8888 output pixel slice from the reference compositor.
    pub fn pixels(&self) -> &[u32] {
        &self.pixels
    }
}

fn raster_sidebar(
    pixels: &mut [u32],
    stride: u32,
    origin_x: u32,
    width: u32,
    height: u32,
    snapshot: &SidebarSnapshot,
) -> SidebarRasterFacts {
    let mut region = RasterRegion::new(pixels, stride, origin_x, width, height);
    region.fill(PANEL_BACKGROUND);
    if width == 0 || height == 0 {
        return SidebarRasterFacts::default();
    }
    for y in 0..height {
        region.set(0, y, PANEL_RULE);
    }

    let mut painter = Painter::new(region);
    painter.text(10, 9, 2, HEADING, "BITBLOCK / F3 METRICS");
    painter.text(10, 25, 1, LABEL, "NAMED VALUES - SNAPSHOT BOUND");
    if !snapshot.visible {
        painter.text(10, 48, 2, VALUE, "F3 TO SHOW METRICS");
        return painter.facts;
    }

    let mut y = 42;
    painter.section(&mut y, "WORLD");
    painter.metric(&mut y, "BITS", snapshot.occupied_bits.to_string());
    painter.metric(&mut y, "MUTABLE", snapshot.mutable_bits.to_string());
    painter.metric(&mut y, "BOUNDS", format_bounds(snapshot.bounds));
    painter.metric(&mut y, "WORLD PIXELS", snapshot.world_pixels.to_string());

    painter.section(&mut y, "OBSERVER");
    painter.metric(
        &mut y,
        "POSITION ~",
        format!(
            "{} {} {}",
            format_scalar(snapshot.observer_position.x),
            format_scalar(snapshot.observer_position.y),
            format_scalar(snapshot.observer_position.z)
        ),
    );
    painter.metric(&mut y, "CELL", format_coord(snapshot.observer_cell));
    painter.metric(
        &mut y,
        "YAW DEG ~",
        format_scalar(snapshot.yaw_radians.to_degrees()),
    );
    painter.metric(
        &mut y,
        "PITCH DEG ~",
        format_scalar(snapshot.pitch_radians.to_degrees()),
    );
    painter.metric(
        &mut y,
        "DEPTH ~",
        format_scalar(snapshot.observation_distance),
    );
    painter.metric(
        &mut y,
        "ORIGIN DIST ~",
        format_scalar(snapshot.origin_distance),
    );

    painter.section(&mut y, "VIEW");
    painter.metric(
        &mut y,
        "SIM VIEW",
        format!(
            "{} X {}",
            snapshot.simulation_width, snapshot.simulation_height
        ),
    );
    painter.metric(
        &mut y,
        "SIM PIXELS",
        (u64::from(snapshot.simulation_width) * u64::from(snapshot.simulation_height)).to_string(),
    );
    painter.metric(
        &mut y,
        "ASPECT ~",
        format_scalar(f64::from(snapshot.simulation_width) / f64::from(snapshot.simulation_height)),
    );
    painter.metric(
        &mut y,
        "TOP SATURATION",
        if snapshot.top_saturated { "YES" } else { "NO" }.to_owned(),
    );
    painter.metric(
        &mut y,
        "SCALE MODE",
        if snapshot.top_saturated {
            "AGGREGATE / ONE PIXEL"
        } else {
            "SPATIAL"
        }
        .to_owned(),
    );

    painter.section(&mut y, "TIME");
    painter.metric(
        &mut y,
        "FRAME SOURCE TICK",
        snapshot.frame_source_tick.to_string(),
    );
    painter.metric(&mut y, "COMPLETED TICKS", snapshot.tick_index.to_string());
    painter.metric(
        &mut y,
        "SIM AGE SEC",
        format_scalar(snapshot.tick_index as f64 / 60.0),
    );
    painter.metric(&mut y, "TICK RATE", "60 HZ".to_owned());

    painter.section(&mut y, "PROVENANCE - PREDECESSOR ONLY");
    painter.metric(&mut y, "COORD RECEIPTS", snapshot.history_len.to_string());
    painter.tag(&mut y, "WORLD ROOT", snapshot.world);
    painter.tag(&mut y, "COORD HEAD", snapshot.history_head);
    painter.optional_tag(&mut y, "PRIOR FRAME HEAD", snapshot.prior_frame_head);
    painter.metric(
        &mut y,
        "PRIOR FRAME COUNT",
        snapshot.prior_frame_len.to_string(),
    );
    painter.optional_tag(&mut y, "PRIOR DISPLAY HEAD", snapshot.prior_display_head);
    painter.metric(
        &mut y,
        "PRIOR DISPLAY COUNT",
        snapshot.prior_display_len.to_string(),
    );
    painter.metric(
        &mut y,
        "DISPLAY AUDIT",
        snapshot.display_status.label().to_owned(),
    );

    painter.section(&mut y, "SIDEBAR");
    painter.metric(&mut y, "VISIBLE", "YES".to_owned());
    painter.metric(
        &mut y,
        "SIDEBAR RECT",
        format!(
            "{} 0 {} {}",
            snapshot.simulation_width, snapshot.sidebar_width, snapshot.output_height
        ),
    );
    painter.metric(
        &mut y,
        "SIM RECT",
        format!(
            "0 0 {} {}",
            snapshot.simulation_width, snapshot.simulation_height
        ),
    );
    painter.metric(
        &mut y,
        "ROW CAPACITY",
        ((height.saturating_sub(42)) / 7).to_string(),
    );
    painter.metric(&mut y, "SECTIONS", "6".to_owned());
    painter.metric(&mut y, "GLYPH SCALE", "1 PX / 2 PX HEADER".to_owned());
    painter.metric(&mut y, "SIM PIXEL BREACH", "0".to_owned());
    match snapshot.previous_raster {
        Some(previous) => {
            painter.metric(
                &mut y,
                "PRIOR DISPLAYED ROWS",
                previous.displayed_metrics.to_string(),
            );
            painter.metric(&mut y, "PRIOR GLYPHS", previous.glyphs.to_string());
            painter.metric(
                &mut y,
                "PRIOR LIT TEXT PX",
                previous.lit_text_pixels.to_string(),
            );
            painter.metric(
                &mut y,
                "PRIOR CLIPPED TEXT",
                previous.clipped_text.to_string(),
            );
            painter.metric(
                &mut y,
                "PRIOR UNAVAILABLE",
                previous.unavailable_fields.to_string(),
            );
        }
        None => {
            for label in [
                "PRIOR DISPLAYED ROWS",
                "PRIOR GLYPHS",
                "PRIOR LIT TEXT PX",
                "PRIOR CLIPPED TEXT",
                "PRIOR UNAVAILABLE",
            ] {
                painter.unavailable(&mut y, label);
            }
        }
    }
    painter.facts
}

struct RasterRegion<'a> {
    pixels: &'a mut [u32],
    stride: u32,
    origin_x: u32,
    width: u32,
    height: u32,
}

impl<'a> RasterRegion<'a> {
    fn new(pixels: &'a mut [u32], stride: u32, origin_x: u32, width: u32, height: u32) -> Self {
        assert!(origin_x <= stride);
        assert!(width <= stride - origin_x);
        assert_eq!(pixels.len(), pixel_len(stride, height));
        Self {
            pixels,
            stride,
            origin_x,
            width,
            height,
        }
    }

    fn fill(&mut self, color: u32) {
        for y in 0..self.height {
            let start = self.row_start(y);
            let end = start + self.width as usize;
            self.pixels[start..end].fill(color);
        }
    }

    fn set(&mut self, x: u32, y: u32, color: u32) {
        assert!(x < self.width && y < self.height);
        let index = self.row_start(y) + x as usize;
        self.pixels[index] = color;
    }

    fn row_start(&self, y: u32) -> usize {
        debug_assert!(y < self.height);
        y as usize * self.stride as usize + self.origin_x as usize
    }
}

struct Painter<'a> {
    region: RasterRegion<'a>,
    facts: SidebarRasterFacts,
}

impl<'a> Painter<'a> {
    fn new(region: RasterRegion<'a>) -> Self {
        Self {
            region,
            facts: SidebarRasterFacts::default(),
        }
    }

    fn section(&mut self, y: &mut u32, label: &str) {
        *y += 3;
        self.text(10, *y, 1, HEADING, label);
        *y += 7;
    }

    fn metric(&mut self, y: &mut u32, label: &str, value: String) {
        self.facts.displayed_metrics += 1;
        self.text(10, *y, 1, LABEL, label);
        let value_x = 152;
        self.text(value_x, *y, 1, VALUE, &value);
        *y += 7;
    }

    fn unavailable(&mut self, y: &mut u32, label: &str) {
        self.facts.unavailable_fields += 1;
        self.metric(y, label, "N/A - GENESIS".to_owned());
    }

    fn optional_tag(&mut self, y: &mut u32, label: &str, tag: Option<ContentTag>) {
        match tag {
            Some(tag) => self.tag(y, label, tag),
            None => self.unavailable(y, label),
        }
    }

    fn tag(&mut self, y: &mut u32, label: &str, tag: ContentTag) {
        self.facts.displayed_metrics += 1;
        self.text(10, *y, 1, LABEL, label);
        *y += 7;
        let value = tag.to_string();
        let split = value.find("sha256.").map_or(0, |index| index + 7);
        self.text(18, *y, 1, VALUE, &value[..split]);
        *y += 7;
        self.text(18, *y, 1, VALUE, &value[split..]);
        *y += 7;
    }

    fn text(&mut self, x: u32, y: u32, scale: u32, color: u32, text: &str) {
        let mut cursor_x = x;
        for character in text.chars() {
            self.facts.glyphs += 1;
            let rows = glyph(character);
            let glyph_right = cursor_x.saturating_add(3 * scale);
            let glyph_bottom = y.saturating_add(5 * scale);
            if glyph_right > self.region.width || glyph_bottom > self.region.height {
                self.facts.clipped_text += 1;
            }
            for (glyph_y, row) in rows.into_iter().enumerate() {
                for glyph_x in 0..3_u32 {
                    if row & (1 << (2 - glyph_x)) == 0 {
                        continue;
                    }
                    for dy in 0..scale {
                        for dx in 0..scale {
                            let px = cursor_x + glyph_x * scale + dx;
                            let py = y + glyph_y as u32 * scale + dy;
                            if px < self.region.width && py < self.region.height {
                                self.region.set(px, py, color);
                                self.facts.lit_text_pixels += 1;
                            }
                        }
                    }
                }
            }
            cursor_x = cursor_x.saturating_add(4 * scale);
        }
    }
}

fn glyph(character: char) -> [u8; 5] {
    match character.to_ascii_uppercase() {
        'A' => [0b010, 0b101, 0b111, 0b101, 0b101],
        'B' => [0b110, 0b101, 0b110, 0b101, 0b110],
        'C' => [0b011, 0b100, 0b100, 0b100, 0b011],
        'D' => [0b110, 0b101, 0b101, 0b101, 0b110],
        'E' => [0b111, 0b100, 0b110, 0b100, 0b111],
        'F' => [0b111, 0b100, 0b110, 0b100, 0b100],
        'G' => [0b011, 0b100, 0b101, 0b101, 0b011],
        'H' => [0b101, 0b101, 0b111, 0b101, 0b101],
        'I' => [0b111, 0b010, 0b010, 0b010, 0b111],
        'J' => [0b001, 0b001, 0b001, 0b101, 0b010],
        'K' => [0b101, 0b101, 0b110, 0b101, 0b101],
        'L' => [0b100, 0b100, 0b100, 0b100, 0b111],
        'M' => [0b101, 0b111, 0b111, 0b101, 0b101],
        'N' => [0b101, 0b111, 0b111, 0b111, 0b101],
        'O' => [0b010, 0b101, 0b101, 0b101, 0b010],
        'P' => [0b110, 0b101, 0b110, 0b100, 0b100],
        'Q' => [0b010, 0b101, 0b101, 0b111, 0b011],
        'R' => [0b110, 0b101, 0b110, 0b101, 0b101],
        'S' => [0b011, 0b100, 0b010, 0b001, 0b110],
        'T' => [0b111, 0b010, 0b010, 0b010, 0b010],
        'U' => [0b101, 0b101, 0b101, 0b101, 0b111],
        'V' => [0b101, 0b101, 0b101, 0b101, 0b010],
        'W' => [0b101, 0b101, 0b111, 0b111, 0b101],
        'X' => [0b101, 0b101, 0b010, 0b101, 0b101],
        'Y' => [0b101, 0b101, 0b010, 0b010, 0b010],
        'Z' => [0b111, 0b001, 0b010, 0b100, 0b111],
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b110, 0b001, 0b010, 0b100, 0b111],
        '3' => [0b110, 0b001, 0b010, 0b001, 0b110],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b110, 0b001, 0b110],
        '6' => [0b011, 0b100, 0b111, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b010, 0b010, 0b010],
        '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b111, 0b001, 0b110],
        '.' => [0, 0, 0, 0, 0b010],
        ',' => [0, 0, 0, 0b010, 0b100],
        ':' => [0, 0b010, 0, 0b010, 0],
        '-' => [0, 0, 0b111, 0, 0],
        '/' => [0b001, 0b001, 0b010, 0b100, 0b100],
        '#' => [0b101, 0b111, 0b101, 0b111, 0b101],
        '+' => [0, 0b010, 0b111, 0b010, 0],
        '=' => [0, 0b111, 0, 0b111, 0],
        '(' | '[' => [0b001, 0b010, 0b010, 0b010, 0b001],
        ')' | ']' => [0b100, 0b010, 0b010, 0b010, 0b100],
        '_' => [0, 0, 0, 0, 0b111],
        ' ' => [0; 5],
        _ => [0b111, 0b001, 0b010, 0, 0b010],
    }
}

fn format_scalar(value: f64) -> String {
    if !value.is_finite() {
        return value.to_string().to_ascii_uppercase();
    }
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.000000".to_owned()
        } else {
            "0.000000".to_owned()
        };
    }
    if value.abs() >= 10_000_000.0 || value.abs() < 0.000_001 {
        format!("{value:.6e}").to_ascii_uppercase()
    } else {
        format!("{value:.6}")
    }
}

fn format_coord(coord: Option<Coord>) -> String {
    coord.map_or_else(
        || "N/A - UNREPRESENTABLE".to_owned(),
        |coord| format!("{} {} {}", coord.x, coord.y, coord.z),
    )
}

fn format_bounds(bounds: Option<Bounds>) -> String {
    bounds.map_or_else(
        || "N/A".to_owned(),
        |bounds| {
            format!(
                "{} {} {} / {} {} {}",
                bounds.min.x, bounds.min.y, bounds.min.z, bounds.max.x, bounds.max.y, bounds.max.z
            )
        },
    )
}

fn sink_u64(sink: &mut dyn FnMut(&[u8]), value: u64) {
    sink(&value.to_be_bytes());
}

fn sink_coord(sink: &mut dyn FnMut(&[u8]), coord: Option<Coord>) {
    match coord {
        Some(coord) => {
            sink(&[1]);
            for component in [coord.x, coord.y, coord.z] {
                sink(&component.to_be_bytes());
            }
        }
        None => {
            sink(&[0]);
            sink(&[0; 24]);
        }
    }
}

fn sink_bounds(sink: &mut dyn FnMut(&[u8]), bounds: Option<Bounds>) {
    match bounds {
        Some(bounds) => {
            sink(&[1]);
            for component in [
                bounds.min.x,
                bounds.min.y,
                bounds.min.z,
                bounds.max.x,
                bounds.max.y,
                bounds.max.z,
            ] {
                sink(&component.to_be_bytes());
            }
        }
        None => {
            sink(&[0]);
            sink(&[0; 48]);
        }
    }
}

fn sink_optional_tag(sink: &mut dyn FnMut(&[u8]), tag: Option<ContentTag>) {
    match tag {
        Some(tag) => {
            sink(&[1]);
            sink(&tag.digest());
        }
        None => {
            sink(&[0]);
            sink(&[0; 32]);
        }
    }
}

#[cfg(test)]
fn reference_push_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

#[cfg(test)]
fn reference_push_coord(bytes: &mut Vec<u8>, coord: Option<Coord>) {
    match coord {
        Some(coord) => {
            bytes.push(1);
            for component in [coord.x, coord.y, coord.z] {
                bytes.extend_from_slice(&component.to_be_bytes());
            }
        }
        None => {
            bytes.push(0);
            bytes.extend_from_slice(&[0; 24]);
        }
    }
}

#[cfg(test)]
fn reference_push_bounds(bytes: &mut Vec<u8>, bounds: Option<Bounds>) {
    match bounds {
        Some(bounds) => {
            bytes.push(1);
            for component in [
                bounds.min.x,
                bounds.min.y,
                bounds.min.z,
                bounds.max.x,
                bounds.max.y,
                bounds.max.z,
            ] {
                bytes.extend_from_slice(&component.to_be_bytes());
            }
        }
        None => {
            bytes.push(0);
            bytes.extend_from_slice(&[0; 48]);
        }
    }
}

#[cfg(test)]
fn reference_push_optional_tag(bytes: &mut Vec<u8>, tag: Option<ContentTag>) {
    match tag {
        Some(tag) => {
            bytes.push(1);
            bytes.extend_from_slice(&tag.digest());
        }
        None => {
            bytes.push(0);
            bytes.extend_from_slice(&[0; 32]);
        }
    }
}

fn pixel_len(width: u32, height: u32) -> usize {
    (width as usize)
        .checked_mul(height as usize)
        .expect("frame dimensions overflow usize")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{FrameInput, FrameRef};
    use crate::history::{History, world_tag};
    use crate::protocol::encode_canonical_preimage;
    use crate::render::{Renderer, crosshair_overlay_input};
    use crate::world::World;
    use sha2::{Digest, Sha256};
    use std::collections::HashSet;

    fn snapshot(width: u32, height: u32, output_width: u32) -> SidebarSnapshot {
        let world = World::new();
        let history = History::new(&world);
        SidebarSnapshot {
            frame_source_tick: 0,
            tick_index: 1,
            occupied_bits: 1,
            mutable_bits: 0,
            bounds: world.bounds(),
            observer_position: Vec3::new(0.0, 0.0, -1.0),
            observer_cell: Some(Coord::new(0, 0, -1)),
            yaw_radians: 0.0,
            pitch_radians: 0.0,
            observation_distance: 1.0,
            origin_distance: 1.0,
            simulation_width: width,
            simulation_height: height,
            output_width,
            output_height: height,
            sidebar_width: output_width - width,
            world_pixels: 1,
            top_saturated: false,
            world: world_tag(&world),
            history_head: history.head(),
            history_len: 0,
            prior_frame_head: None,
            prior_frame_len: 0,
            prior_display_head: None,
            prior_display_len: 0,
            display_status: DisplayStatus::AuditDisabled,
            visible: true,
            previous_raster: None,
        }
    }

    fn tag_with_digest(kind: TagKind, digest: [u8; 32]) -> ContentTag {
        ContentTag::from_digest(kind, digest)
    }

    fn sequential_digest(start: u8) -> [u8; 32] {
        std::array::from_fn(|index| start.wrapping_add(index as u8))
    }

    fn populated_zero_snapshot() -> SidebarSnapshot {
        SidebarSnapshot {
            frame_source_tick: 0,
            tick_index: 0,
            occupied_bits: 0,
            mutable_bits: 0,
            bounds: Some(Bounds {
                min: Coord::new(0, 0, 0),
                max: Coord::new(0, 0, 0),
            }),
            observer_position: Vec3::new(0.0, 0.0, 0.0),
            observer_cell: Some(Coord::new(0, 0, 0)),
            yaw_radians: 0.0,
            pitch_radians: 0.0,
            observation_distance: 0.0,
            origin_distance: 0.0,
            simulation_width: 0,
            simulation_height: 0,
            output_width: 0,
            output_height: 0,
            sidebar_width: 0,
            world_pixels: 0,
            top_saturated: false,
            world: tag_with_digest(TagKind::World, [0; 32]),
            history_head: tag_with_digest(TagKind::Receipt, [0; 32]),
            history_len: 0,
            prior_frame_head: Some(tag_with_digest(TagKind::FrameReceipt, [0; 32])),
            prior_frame_len: 0,
            prior_display_head: Some(tag_with_digest(TagKind::DisplayReceipt, [0; 32])),
            prior_display_len: 0,
            display_status: DisplayStatus::Unsupported,
            visible: false,
            previous_raster: Some(SidebarRasterFacts::default()),
        }
    }

    fn compose_two_buffer_reference(
        width: u32,
        height: u32,
        simulation_width: u32,
        game_pixels: &[u32],
        snapshot: &SidebarSnapshot,
    ) -> (Vec<u32>, SidebarRasterFacts) {
        let mut reference = TwoBufferReferenceCompositor::new(width, height, simulation_width);
        let facts = reference.compose(game_pixels, snapshot);
        (reference.pixels().to_vec(), facts)
    }

    fn observed_game_tag(width: u32, height: u32, game_pixels: &[u32]) -> ContentTag {
        FrameRef::new(width, height, game_pixels)
            .unwrap()
            .content_tag()
    }

    fn assert_pixels_equal(case: &str, candidate: &[u32], reference: &[u32]) {
        assert_eq!(candidate.len(), reference.len(), "{case} length diverged");
        if let Some(index) = candidate
            .iter()
            .zip(reference)
            .position(|(left, right)| left != right)
        {
            panic!(
                "{case} first pixel mismatch at {index}: candidate={:#010x}, reference={:#010x}",
                candidate[index], reference[index]
            );
        }
    }

    fn streamed_payload(snapshot: &SidebarSnapshot) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(SNAPSHOT_PAYLOAD_LEN as usize);
        snapshot.encode_canonical(&mut |chunk| bytes.extend_from_slice(chunk));
        bytes
    }

    fn assert_snapshot_stream_parity(snapshot: &SidebarSnapshot) {
        let streamed = streamed_payload(snapshot);
        let reference = snapshot.canonical_bytes_v1_reference();
        assert_eq!(streamed.len(), SNAPSHOT_PAYLOAD_LEN as usize);
        assert_eq!(streamed, reference);
        let reference_tag = hash_canonical(
            TagKind::MetricSnapshot,
            SNAPSHOT_SCHEMA,
            SNAPSHOT_PAYLOAD_LEN,
            |sink| sink(&reference),
        )
        .unwrap();
        assert_eq!(snapshot.tag(), reference_tag);
        let preimage = encode_canonical_preimage(
            TagKind::MetricSnapshot,
            SNAPSHOT_SCHEMA,
            SNAPSHOT_PAYLOAD_LEN,
            |sink| snapshot.encode_canonical(sink),
        )
        .unwrap();
        assert_eq!(preimage.len(), 463);
    }

    fn set_u64_lane(snapshot: &mut SidebarSnapshot, field: usize, value: u64) {
        match field {
            0 => snapshot.frame_source_tick = value,
            1 => snapshot.tick_index = value,
            2 => snapshot.occupied_bits = value,
            3 => snapshot.mutable_bits = value,
            4..=9 => {
                let mut bounds = snapshot.bounds.unwrap();
                let target = match field {
                    4 => &mut bounds.min.x,
                    5 => &mut bounds.min.y,
                    6 => &mut bounds.min.z,
                    7 => &mut bounds.max.x,
                    8 => &mut bounds.max.y,
                    9 => &mut bounds.max.z,
                    _ => unreachable!(),
                };
                *target = value as i64;
                snapshot.bounds = Some(bounds);
            }
            10 => snapshot.observer_position.x = f64::from_bits(value),
            11 => snapshot.observer_position.y = f64::from_bits(value),
            12 => snapshot.observer_position.z = f64::from_bits(value),
            13 => snapshot.yaw_radians = f64::from_bits(value),
            14 => snapshot.pitch_radians = f64::from_bits(value),
            15 => snapshot.observation_distance = f64::from_bits(value),
            16 => snapshot.origin_distance = f64::from_bits(value),
            17..=19 => {
                let mut coord = snapshot.observer_cell.unwrap();
                let target = match field {
                    17 => &mut coord.x,
                    18 => &mut coord.y,
                    19 => &mut coord.z,
                    _ => unreachable!(),
                };
                *target = value as i64;
                snapshot.observer_cell = Some(coord);
            }
            20 => snapshot.world_pixels = value,
            21 => snapshot.history_len = value,
            22 => snapshot.prior_frame_len = value,
            23 => snapshot.prior_display_len = value,
            _ => panic!("unknown 64-bit snapshot lane {field}"),
        }
    }

    fn set_u32_lane(snapshot: &mut SidebarSnapshot, field: usize, value: u32) {
        match field {
            0 => snapshot.simulation_width = value,
            1 => snapshot.simulation_height = value,
            2 => snapshot.output_width = value,
            3 => snapshot.output_height = value,
            4 => snapshot.sidebar_width = value,
            5..=9 => {
                let mut facts = snapshot.previous_raster.unwrap();
                let target = match field {
                    5 => &mut facts.displayed_metrics,
                    6 => &mut facts.glyphs,
                    7 => &mut facts.lit_text_pixels,
                    8 => &mut facts.clipped_text,
                    9 => &mut facts.unavailable_fields,
                    _ => unreachable!(),
                };
                *target = value;
                snapshot.previous_raster = Some(facts);
            }
            _ => panic!("unknown 32-bit snapshot lane {field}"),
        }
    }

    fn set_digest_lane(snapshot: &mut SidebarSnapshot, field: usize, digest: [u8; 32]) {
        match field {
            0 => snapshot.world = tag_with_digest(TagKind::World, digest),
            1 => snapshot.history_head = tag_with_digest(TagKind::Receipt, digest),
            2 => {
                snapshot.prior_frame_head = Some(tag_with_digest(TagKind::FrameReceipt, digest));
            }
            3 => {
                snapshot.prior_display_head =
                    Some(tag_with_digest(TagKind::DisplayReceipt, digest));
            }
            _ => panic!("unknown digest snapshot lane {field}"),
        }
    }

    #[test]
    fn snapshot_stream_matches_v1_reference_branch_product() {
        let statuses = [
            DisplayStatus::Unsupported,
            DisplayStatus::AuditDisabled,
            DisplayStatus::NoReceipt,
            DisplayStatus::Match,
            DisplayStatus::Mismatch,
            DisplayStatus::Unavailable,
        ];
        let mut cases = 0;
        for option_mask in 0_u8..32 {
            for status in statuses {
                for top_saturated in [false, true] {
                    for visible in [false, true] {
                        let mut value = populated_zero_snapshot();
                        value.bounds = (option_mask & 1 != 0).then_some(Bounds {
                            min: Coord::new(-11, 12, -13),
                            max: Coord::new(14, -15, 16),
                        });
                        value.observer_cell =
                            (option_mask & 2 != 0).then_some(Coord::new(-21, 22, -23));
                        value.prior_frame_head = (option_mask & 4 != 0).then_some(tag_with_digest(
                            TagKind::FrameReceipt,
                            sequential_digest(0x40),
                        ));
                        value.prior_display_head = (option_mask & 8 != 0).then_some(
                            tag_with_digest(TagKind::DisplayReceipt, sequential_digest(0x60)),
                        );
                        value.previous_raster =
                            (option_mask & 16 != 0).then_some(SidebarRasterFacts {
                                displayed_metrics: 1,
                                glyphs: 2,
                                lit_text_pixels: 3,
                                clipped_text: 4,
                                unavailable_fields: 5,
                            });
                        value.display_status = status;
                        value.top_saturated = top_saturated;
                        value.visible = visible;
                        assert_snapshot_stream_parity(&value);
                        cases += 1;
                    }
                }
            }
        }
        assert_eq!(cases, 768);
    }

    #[test]
    fn snapshot_stream_matches_v1_reference_walking_bits() {
        let mut cases = 0;
        for field in 0..24 {
            for bit in 0..64 {
                let mut value = populated_zero_snapshot();
                set_u64_lane(&mut value, field, 1_u64 << bit);
                assert_snapshot_stream_parity(&value);
                cases += 1;
            }
        }
        for field in 0..10 {
            for bit in 0..32 {
                let mut value = populated_zero_snapshot();
                set_u32_lane(&mut value, field, 1_u32 << bit);
                assert_snapshot_stream_parity(&value);
                cases += 1;
            }
        }
        for field in 0..4 {
            for bit in 0..256 {
                let mut value = populated_zero_snapshot();
                let mut digest = [0_u8; 32];
                digest[bit / 8] = 1_u8 << (bit % 8);
                set_digest_lane(&mut value, field, digest);
                assert_snapshot_stream_parity(&value);
                cases += 1;
            }
        }
        assert_eq!(cases, 2_880);
    }

    #[test]
    fn snapshot_stream_preserves_named_float_bit_patterns() {
        let patterns = [
            0_u64,
            1_u64 << 63,
            1,
            (1_u64 << 63) | 1,
            0x0010_0000_0000_0000,
            0x7fef_ffff_ffff_ffff,
            0xffef_ffff_ffff_ffff,
            0x7ff0_0000_0000_0000,
            0xfff0_0000_0000_0000,
            0x7ff0_0000_0000_0001,
            0x7ff8_0000_0000_0001,
            0xfff8_0000_0000_0001,
            0x0123_4567_89ab_cdef,
        ];
        for field in 10..=16 {
            for pattern in patterns {
                let mut value = populated_zero_snapshot();
                set_u64_lane(&mut value, field, pattern);
                assert_snapshot_stream_parity(&value);
            }
        }
    }

    #[test]
    fn snapshot_v1_matches_independent_known_answers() {
        let mut none_zero = populated_zero_snapshot();
        none_zero.bounds = None;
        none_zero.observer_cell = None;
        none_zero.world = tag_with_digest(TagKind::World, sequential_digest(0x00));
        none_zero.history_head = tag_with_digest(TagKind::Receipt, sequential_digest(0x20));
        none_zero.prior_frame_head = None;
        none_zero.prior_display_head = None;
        none_zero.previous_raster = None;
        assert_snapshot_stream_parity(&none_zero);
        assert_eq!(
            format!("{:x}", Sha256::digest(streamed_payload(&none_zero))),
            "c3ddcbc14d0caa9de44d88ab10e00bdc853f28636cdcd8b445ae5455807497ea"
        );
        assert_eq!(
            none_zero.tag().to_string(),
            "#bb.metric-snapshot.sha256.ayjgabj4ttvzgagldztg4ceaxtob6oob4icpxuoha5r3xvb44tga"
        );

        let mut all_edge = populated_zero_snapshot();
        all_edge.frame_source_tick = 0x0123_4567_89ab_cdef;
        all_edge.tick_index = 0xfedc_ba98_7654_3210;
        all_edge.occupied_bits = 0;
        all_edge.mutable_bits = u64::MAX;
        all_edge.bounds = Some(Bounds {
            min: Coord::new(i64::MIN, -1, 0),
            max: Coord::new(i64::MAX, 0, 1),
        });
        let float_bits = [
            0,
            1_u64 << 63,
            1,
            0x7ff0_0000_0000_0000,
            0xfff0_0000_0000_0000,
            0x7ff8_0000_0000_0001,
            0x0123_4567_89ab_cdef,
        ];
        all_edge.observer_position = Vec3::new(
            f64::from_bits(float_bits[0]),
            f64::from_bits(float_bits[1]),
            f64::from_bits(float_bits[2]),
        );
        all_edge.yaw_radians = f64::from_bits(float_bits[3]);
        all_edge.pitch_radians = f64::from_bits(float_bits[4]);
        all_edge.observation_distance = f64::from_bits(float_bits[5]);
        all_edge.origin_distance = f64::from_bits(float_bits[6]);
        all_edge.observer_cell = Some(Coord::new(i64::MIN, 0, i64::MAX));
        all_edge.simulation_width = 0;
        all_edge.simulation_height = 1;
        all_edge.output_width = 0x8000_0000;
        all_edge.output_height = u32::MAX;
        all_edge.sidebar_width = 0x0123_4567;
        all_edge.world = tag_with_digest(TagKind::World, sequential_digest(0x00));
        all_edge.history_head = tag_with_digest(TagKind::Receipt, sequential_digest(0x20));
        all_edge.prior_frame_head = Some(tag_with_digest(
            TagKind::FrameReceipt,
            sequential_digest(0x40),
        ));
        all_edge.prior_display_head = Some(tag_with_digest(
            TagKind::DisplayReceipt,
            sequential_digest(0x60),
        ));
        all_edge.display_status = DisplayStatus::Unavailable;
        all_edge.top_saturated = true;
        all_edge.visible = true;
        all_edge.previous_raster = Some(SidebarRasterFacts {
            displayed_metrics: 0,
            glyphs: 1,
            lit_text_pixels: 0x8000_0000,
            clipped_text: u32::MAX,
            unavailable_fields: 0x0123_4567,
        });
        assert_snapshot_stream_parity(&all_edge);
        assert_eq!(
            format!("{:x}", Sha256::digest(streamed_payload(&all_edge))),
            "dac5efcaa64838852fcd9ee6905060b7efeba5bf367fbe636c18127c981ec076"
        );
        assert_eq!(
            all_edge.tag().to_string(),
            "#bb.metric-snapshot.sha256.hhgascw5h7giyvfcm5z2zpohvnr6jjgxc6dennd73dm7uvvtmaaa"
        );
    }

    #[test]
    fn displayed_bindings_are_unique_and_exact_registry_rows() {
        let registry = include_str!("../../../docs/metrics/registry.tsv");
        let mut aliases = HashSet::new();
        let mut definitions = HashSet::new();
        for binding in DISPLAYED_METRICS {
            assert!(aliases.insert(binding.alias), "duplicate {}", binding.alias);
            assert!(
                definitions.insert(binding.definition),
                "duplicate {}",
                binding.definition
            );
            let prefix = format!("{}\t{}\t", binding.alias, binding.definition);
            assert_eq!(
                registry
                    .lines()
                    .filter(|line| line.starts_with(&prefix))
                    .count(),
                1,
                "binding is not exactly authoritative: {}",
                binding.alias
            );
            assert!(!binding.alias.split('.').any(|part| {
                !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())
            }));
        }
    }

    #[test]
    fn sidebar_policies_match_independent_python_vectors() {
        assert_eq!(
            sidebar_policy_tag().to_string(),
            "#bb.policy.sha256.eg4egguiubrb5l45qbwvf76gwbkgtlb7pymc3zi4uq6jrqgl4b5q"
        );
        assert_eq!(
            composite_policy_tag().to_string(),
            "#bb.policy.sha256.kyd57oandtrj4iuis54zozpv6pwgmcoy4hmsvlkp2fqvhy6noe5q"
        );
    }

    #[test]
    fn streamed_sidebar_policy_matches_the_collected_reference_bytes() {
        let reference = sidebar_policy_payload_reference();
        assert_eq!(reference.len() as u64, sidebar_policy_payload_len());
        let reference_tag = hash_canonical(
            TagKind::Policy,
            SIDEBAR_POLICY_SCHEMA,
            reference.len() as u64,
            |sink| sink(&reference),
        )
        .unwrap();
        assert_eq!(sidebar_policy_tag(), reference_tag);
    }

    #[test]
    fn strided_compositor_matches_frozen_two_buffer_reference() {
        let mut cases = vec![
            snapshot(801, 801, 1_201),
            snapshot(11, 53, 82),
            snapshot(13, 9, 13),
            populated_zero_snapshot(),
        ];

        let mut hidden = snapshot(17, 80, 117);
        hidden.visible = false;
        cases.push(hidden);

        let mut complete = snapshot(31, 190, 431);
        complete.top_saturated = true;
        complete.prior_frame_head = Some(tag_with_digest(TagKind::FrameReceipt, [0x5a; 32]));
        complete.prior_frame_len = 41;
        complete.prior_display_head = Some(tag_with_digest(TagKind::DisplayReceipt, [0xa5; 32]));
        complete.prior_display_len = 17;
        complete.previous_raster = Some(SidebarRasterFacts {
            displayed_metrics: 1,
            glyphs: 2,
            lit_text_pixels: 3,
            clipped_text: 4,
            unavailable_fields: 5,
        });
        cases.push(complete);

        for status in [
            DisplayStatus::Unsupported,
            DisplayStatus::AuditDisabled,
            DisplayStatus::NoReceipt,
            DisplayStatus::Match,
            DisplayStatus::Mismatch,
            DisplayStatus::Unavailable,
        ] {
            let mut status_case = snapshot(19, 90, 119);
            status_case.display_status = status;
            cases.push(status_case);
        }

        for (case_index, snapshot) in cases.into_iter().enumerate() {
            let width = snapshot.output_width;
            let height = snapshot.output_height;
            let simulation_width = snapshot.simulation_width;
            let game = (0_u32..simulation_width.saturating_mul(height))
                .map(|index| 0xa500_0000 ^ index.wrapping_mul(0x0001_0101))
                .collect::<Vec<_>>();
            let (reference_pixels, reference_facts) =
                compose_two_buffer_reference(width, height, simulation_width, &game, &snapshot);
            let mut candidate = FrameCompositor::new(width, height, simulation_width);
            let candidate_facts = candidate.compose(
                &game,
                observed_game_tag(simulation_width, height, &game),
                &snapshot,
            );

            assert_eq!(
                candidate_facts, reference_facts,
                "raster facts diverged in case {case_index}"
            );
            assert_pixels_equal(
                &format!("case {case_index}"),
                candidate.pixels(),
                &reference_pixels,
            );
            assert_eq!(
                FrameRef::new(width, height, candidate.pixels())
                    .unwrap()
                    .observe(),
                FrameRef::new(width, height, &reference_pixels)
                    .unwrap()
                    .observe(),
                "composite observation diverged in case {case_index}"
            );
        }

        let width = 1_201;
        let height = 801;
        let simulation_width = 801;
        let game = vec![0x005a_a55a; pixel_len(simulation_width, height)];
        let visible = snapshot(simulation_width, height, width);
        let mut hidden = visible.clone();
        hidden.visible = false;
        let mut candidate = FrameCompositor::new(width, height, simulation_width);
        let game_tag = observed_game_tag(simulation_width, height, &game);
        candidate.compose(&game, game_tag, &visible);
        let (hidden_reference, hidden_facts) =
            compose_two_buffer_reference(width, height, simulation_width, &game, &hidden);
        let candidate_facts = candidate.compose(&game, game_tag, &hidden);
        assert_eq!(candidate_facts, hidden_facts);
        assert_pixels_equal(
            "sequential hidden case",
            candidate.pixels(),
            &hidden_reference,
        );
    }

    #[test]
    fn sidebar_only_reuse_matches_the_frozen_full_compositor_and_skips_game_copy() {
        let simulation_width = 801;
        let height = 801;
        let output_width = 1_201;
        let game = (0_u32..simulation_width * height)
            .map(|index| index.wrapping_mul(0x0001_0101) & 0x00ff_ffff)
            .collect::<Vec<_>>();
        let game_tag = observed_game_tag(simulation_width, height, &game);
        let first = snapshot(simulation_width, height, output_width);
        let mut reference =
            TwoBufferReferenceCompositor::new(output_width, height, simulation_width);
        let mut candidate = FrameCompositor::new(output_width, height, simulation_width);

        let reference_first = reference.compose(&game, &first);
        let candidate_first = candidate.compose(&game, game_tag, &first);
        assert_eq!(candidate_first, reference_first);
        assert_eq!(
            candidate.last_game_prefix_copy_words(),
            u64::from(simulation_width) * u64::from(height)
        );
        assert_pixels_equal("sidebar-only setup", candidate.pixels(), reference.pixels());

        let mut second = first.clone();
        second.tick_index += 1;
        second.visible = false;
        second.previous_raster = Some(candidate_first);
        let reference_second = reference.compose(&game, &second);
        let candidate_second = candidate.compose_sidebar_only(game_tag, &second);

        assert_eq!(candidate_second, reference_second);
        assert_eq!(candidate.last_game_prefix_copy_words(), 0);
        assert_pixels_equal("sidebar-only reuse", candidate.pixels(), reference.pixels());
        assert_eq!(
            FrameRef::new(output_width, height, candidate.pixels())
                .unwrap()
                .observe(),
            FrameRef::new(output_width, height, reference.pixels())
                .unwrap()
                .observe()
        );
    }

    #[test]
    fn sidebar_only_reuse_refuses_a_stale_identity_and_a_resized_cache() {
        let simulation_width = 11;
        let height = 7;
        let output_width = 24;
        let game_a = vec![0x0012_3456; pixel_len(simulation_width, height)];
        let game_b = vec![0x0065_4321; pixel_len(simulation_width, height)];
        let tag_a = observed_game_tag(simulation_width, height, &game_a);
        let tag_b = observed_game_tag(simulation_width, height, &game_b);
        let snapshot = snapshot(simulation_width, height, output_width);
        let mut candidate = FrameCompositor::new(output_width, height, simulation_width);

        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                candidate.compose_sidebar_only(tag_a, &snapshot)
            }))
            .is_err()
        );

        candidate.compose(&game_a, tag_a, &snapshot);
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                candidate.compose_sidebar_only(tag_b, &snapshot)
            }))
            .is_err()
        );

        candidate.resize(output_width, height, simulation_width);
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                candidate.compose_sidebar_only(tag_a, &snapshot)
            }))
            .is_err()
        );
    }

    #[test]
    fn compositor_preserves_every_simulation_word_exactly() {
        let width = 13;
        let height = 9;
        let output_width = width + 17;
        let game = (0_u32..width * height)
            .map(|index| index.wrapping_mul(0x0001_0101) & 0x00ff_ffff)
            .collect::<Vec<_>>();
        let mut compositor = FrameCompositor::new(output_width, height, width);
        compositor.compose(
            &game,
            observed_game_tag(width, height, &game),
            &snapshot(width, height, output_width),
        );

        for y in 0..height as usize {
            let source = &game[y * width as usize..(y + 1) * width as usize];
            let output = &compositor.pixels()
                [y * output_width as usize..y * output_width as usize + width as usize];
            assert_eq!(output, source);
        }
    }

    #[test]
    fn one_sidebar_pixel_changes_composite_but_not_game_tag() {
        let width = 11;
        let height = 7;
        let output_width = width + 13;
        let game = vec![0x0012_3456; pixel_len(width, height)];
        let game_tag = FrameRef::new(width, height, &game).unwrap().content_tag();
        let mut compositor = FrameCompositor::new(output_width, height, width);
        compositor.compose(&game, game_tag, &snapshot(width, height, output_width));
        let first = FrameRef::new(output_width, height, compositor.pixels())
            .unwrap()
            .content_tag();
        let index = width as usize;
        compositor.pixels[index] ^= 0x0000_0001;
        let second = FrameRef::new(output_width, height, compositor.pixels())
            .unwrap()
            .content_tag();

        assert_ne!(first, second);
        assert_eq!(
            FrameRef::new(width, height, &game).unwrap().content_tag(),
            game_tag
        );
    }

    #[test]
    fn snapshot_tag_commits_every_dynamic_field() {
        let base = snapshot(801, 801, 1201);
        let tag = base.tag();
        let mut mutations = Vec::new();
        let mut changed = base.clone();
        changed.tick_index += 1;
        mutations.push(changed);
        let mut changed = base.clone();
        changed.observer_position.x = -0.0;
        mutations.push(changed);
        let mut changed = base.clone();
        changed.visible = false;
        mutations.push(changed);
        let mut changed = base.clone();
        changed.previous_raster = Some(SidebarRasterFacts::default());
        mutations.push(changed);
        let mut changed = base.clone();
        changed.display_status = DisplayStatus::Unavailable;
        mutations.push(changed);
        for changed in mutations {
            assert_ne!(changed.tag(), tag);
        }
    }

    #[test]
    fn composite_input_binds_game_snapshot_and_both_shapes() {
        let width = 801;
        let height = 801;
        let output_width = 1201;
        let camera = crate::camera::Camera::for_observer_spawn();
        let world = World::new();
        let game_input = FrameInput::capture(
            world_tag(&world),
            &camera,
            width,
            height,
            renderer_profile_tag(),
            crosshair_overlay_input().tag(),
        )
        .unwrap();
        let mut renderer = Renderer::new(width, height);
        renderer.render(&world, &camera);
        renderer.draw_crosshair(crosshair_overlay_input());
        let game_frame = FrameRef::new(width, height, renderer.pixels())
            .unwrap()
            .content_tag();
        let snapshot = snapshot(width, height, output_width);
        let overlay = composite_overlay_input(snapshot.tag()).unwrap();
        let base = CompositeFrameInput::new(
            game_input.tag(),
            game_frame,
            overlay,
            width,
            height,
            output_width,
            height,
        );
        let changed = CompositeFrameInput::new(
            game_input.tag(),
            game_frame,
            overlay,
            width,
            height,
            output_width + 1,
            height,
        );
        assert_ne!(base.tag(), changed.tag());
    }

    /// `tools/composite_frame_input_vectors.py` derives these literals without running any Rust.
    /// Every nested object is one an existing independent suite already reconstructs: the game
    /// input and frame from `tools/frame_chain_vectors.py`, the composition policy from
    /// `tools/sidebar_vectors.py`, and the metric snapshot from
    /// `tools/sidebar_snapshot_vectors.py`.
    #[test]
    fn composite_frame_input_matches_independent_python_vectors() {
        let game_input = FrameInput::from_camera_bit_lanes(
            tag_with_digest(TagKind::World, sequential_digest(0x00)),
            crate::frame::VECTOR_CAMERA_LANES.map(f64::to_bits),
            801,
            801,
            renderer_profile_tag(),
            crosshair_overlay_input().tag(),
        )
        .unwrap();
        assert_eq!(
            game_input.tag().to_string(),
            "#bb.frame-input.sha256.oiwdnnaxuumavgducowfx7n4uc4gh4zoawgxj56zn5aaux2vzuvq"
        );
        let game_frame = FrameRef::new(1, 1, &[0x0012_3456]).unwrap().content_tag();

        let mut none_zero = populated_zero_snapshot();
        none_zero.bounds = None;
        none_zero.observer_cell = None;
        none_zero.world = tag_with_digest(TagKind::World, sequential_digest(0x00));
        none_zero.history_head = tag_with_digest(TagKind::Receipt, sequential_digest(0x20));
        none_zero.prior_frame_head = None;
        none_zero.prior_display_head = None;
        none_zero.previous_raster = None;
        let overlay = composite_overlay_input(none_zero.tag()).unwrap();
        assert_eq!(
            overlay.tag().to_string(),
            "#bb.policy.sha256.mzcw7fxkb3owzo4ofsu5dxdecdphjgwiqfmxehkx6sjirjvhmoba"
        );

        let composite =
            CompositeFrameInput::new(game_input.tag(), game_frame, overlay, 801, 801, 1201, 801);
        assert_eq!(composite.tag().kind(), TagKind::FrameInput);
        assert_eq!(
            composite.tag().to_string(),
            "#bb.frame-input.sha256.xt5sklsl47sta5tf5oespjwu3vmtcc5llli4yls23icku7gdlkdq"
        );

        // The sidebar width is never a stored field: it is exactly the gap between the two
        // committed widths, so hiding the sidebar moves the composite tag while the inner game
        // input and frame it names are untouched.
        let hidden =
            CompositeFrameInput::new(game_input.tag(), game_frame, overlay, 801, 801, 801, 801);
        assert_eq!(
            hidden.tag().to_string(),
            "#bb.frame-input.sha256.7gmgdzcu2mveylwzudf2ed6cb6ix3jt4f3mhykkr4ulokdrqcd7q"
        );
        assert_ne!(composite.tag(), hidden.tag());
    }

    #[test]
    fn raster_never_writes_outside_its_nonzero_origin_region() {
        let stride = 71;
        let origin_x = 19;
        let width = 37;
        let height = 53;
        let sentinel = 0x00ab_cdef;
        let mut guarded = vec![sentinel; pixel_len(stride, height) + 2];
        let before = guarded[0];
        let after = *guarded.last().unwrap();
        let snapshot = snapshot(11, height, stride);
        raster_sidebar(
            &mut guarded[1..pixel_len(stride, height) + 1],
            stride,
            origin_x,
            width,
            height,
            &snapshot,
        );
        assert_eq!(guarded[0], before);
        assert_eq!(*guarded.last().unwrap(), after);
        let pixels = &guarded[1..pixel_len(stride, height) + 1];
        for y in 0..height {
            for x in 0..stride {
                if x < origin_x || x >= origin_x + width {
                    assert_eq!(pixels[(y * stride + x) as usize], sentinel, "x={x}, y={y}");
                }
            }
        }
        assert!(pixels.iter().any(|&pixel| pixel != sentinel));
    }
}
