//! Browser stand-in for the native Bitblock window.
//!
//! Mirrors `App::apply_accepted_action` + the physics / edit / render / compose
//! order inside `App::fixed_tick` (`src/main.rs`), calling the same bitblock-core
//! APIs. Omits the TickOutcome / FrameLedger / display-audit chain that lives in
//! the winit-coupled `App` (those do not feed world, camera, history, or pixels).
//! Collision helpers below are byte-equal copies of the private main.rs helpers
//! that sit on top of `sweep_point`.

use std::cell::Cell;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use bitblock_core::camera::{
    Camera, DEFAULT_DOLLY_LOG_STEP, DEFAULT_MOUSE_SENSITIVITY, MAX_OBSERVATION_DISTANCE, Vec3,
};
use bitblock_core::frame::FrameRef;
use bitblock_core::history::{History, TickTransaction, world_tag};
use bitblock_core::interaction::{AcceptedAction, InteractionTape, MoveControl};
use bitblock_core::program::{Edit, Operation, Program};
use bitblock_core::protocol::ContentTag;
use bitblock_core::render::{
    Renderer, bounds_aabb, coord_containing, crosshair_overlay_input, raycast, sweep_point,
};
use bitblock_core::sidebar::{DisplayStatus, FrameCompositor, SidebarRasterFacts, SidebarSnapshot};
use bitblock_core::tick::{
    LANE_AIM_DELETE, LANE_AIM_PLACE, LANE_CLEAR_MOTION, LANE_EXACT_EDIT, LANE_FIT_WORLD,
    LANE_LOOK_DELTA, LANE_MOVE_EDGE, LANE_RESIZE, LANE_TOGGLE_SIDEBAR, LANE_WHEEL_LINES,
    TickActionError, TickActionLedger,
};
use bitblock_core::world::{Coord, World};

use crate::{SIDEBAR_OUTPUT_HEIGHT, SIDEBAR_OUTPUT_WIDTH, SIDEBAR_SIMULATION};

/// Same as `INITIAL_VIEWPORT` / `SIDEBAR_WIDTH` in `src/main.rs`.
pub const PLAY_VIEWPORT: u32 = SIDEBAR_SIMULATION;
pub const PLAY_OUTPUT_WIDTH: u32 = SIDEBAR_OUTPUT_WIDTH;
pub const PLAY_OUTPUT_HEIGHT: u32 = SIDEBAR_OUTPUT_HEIGHT;
const BLOCK_FILL_DISTANCE: f64 = 0.25;
const OBSERVER_COLLISION_CLEARANCE: f64 = 1.0e-6;
const TICK_RATE: f64 = 60.0;
/// Browser layout bounds for the live resolution control.
pub const PLAY_MIN_DIM: u32 = 64;
pub const PLAY_MAX_WIDTH: u32 = 3840;
pub const PLAY_MAX_HEIGHT: u32 = 2160;

/// Fixed InteractionTape text. Same AcceptedAction stream the JS play handlers
/// emit for the play-tape button (move, look/yaw, place, delete, sidebar, fit).
pub const PLAY_INPUT_TAPE: &str = "\
# browser play determinism tape — native InteractionTape v1
tick
move forward down
look 80 0

tick
move forward up
wheel -8
place

tick
delete

tick
sidebar

tick
fit
";

/// Physical-key codes mirrored from native `App::key` (WASD / Space / Shift / F / F3 / Esc).
pub const PLAY_KEY_W: u32 = 0;
pub const PLAY_KEY_S: u32 = 1;
pub const PLAY_KEY_A: u32 = 2;
pub const PLAY_KEY_D: u32 = 3;
pub const PLAY_KEY_SPACE: u32 = 4;
pub const PLAY_KEY_SHIFT: u32 = 5;
pub const PLAY_KEY_F: u32 = 6;
pub const PLAY_KEY_F3: u32 = 7;
pub const PLAY_KEY_ESCAPE: u32 = 8;

#[derive(Clone, Copy)]
enum AimEdit {
    Place,
    Delete,
}

#[derive(Clone, Copy)]
enum PendingEdit {
    Aim(AimEdit),
    Exact(Edit),
}

#[derive(Default)]
struct Inputs {
    forward: bool,
    backward: bool,
    left: bool,
    right: bool,
    up: bool,
    down: bool,
    mouse_x: f64,
    mouse_y: f64,
    wheel: f64,
    edits: Vec<PendingEdit>,
    fit_world: bool,
    viewport_refit: bool,
    actions: TickActionLedger,
}

impl Inputs {
    fn clear_motion(&mut self) {
        self.forward = false;
        self.backward = false;
        self.left = false;
        self.right = false;
        self.up = false;
        self.down = false;
        self.mouse_x = 0.0;
        self.mouse_y = 0.0;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ApplyError {
    NonFiniteLane {
        field: &'static str,
        bits: u64,
    },
    NonFiniteAccumulation {
        field: &'static str,
        prior_bits: u64,
        delta_bits: u64,
    },
    Unaccountable(TickActionError),
}

pub struct PlaySession {
    world: World,
    history: History,
    camera: Camera,
    renderer: Renderer,
    compositor: FrameCompositor,
    inputs: Inputs,
    mouse_captured: bool,
    cursor_x: f64,
    cursor_y: f64,
    sidebar_visible: bool,
    needs_render: bool,
    needs_sidebar_render: bool,
    tick: u64,
    last_action: String,
    /// True once any frame has been rendered into the output buffer.
    has_frame: bool,
    /// Canonical tag of the current output buffer. Eager on the audited (tape) path,
    /// lazy on the live path: computed on first status read that asks for it.
    frame_tag: Cell<Option<ContentTag>>,
    last_world_tag: ContentTag,
    last_sidebar_facts: Option<SidebarRasterFacts>,
    /// Browser-only layout: game view fills the whole output, no sidebar panel.
    game_only: bool,
    /// Whether the compositor cache is bound to the current game frame (audited compose ran
    /// since the last resize / unbound compose), i.e. `compose_sidebar_only` is admissible.
    compositor_bound: bool,
    /// Output as RGBA8888 bytes (stored as native-endian words, reused across frames).
    rgba: Vec<u32>,
    /// `Some(serial)` when `rgba` holds exactly the conversion of the game-only renderer frame
    /// with that `Renderer::frame_serial`, so the next one may convert only its changed rows.
    rgba_game_serial: Option<u64>,
    renders: u64,
    dirty: bool,
    /// Opt-in chem mode (`with_chem`): genesis + planet6 tick; dig/place go through chem.
    chem: Option<crate::play_chem::ChemPlay>,
    /// Chem cross-section view: the play world minus the cut's hidden bits, keyed by the
    /// world tag, cut and camera side it was built for. Render / aim only; never history.
    cut_view: Option<(CutViewKey, World)>,
    /// Lit-face masks of the cut faces for `cut_view` (rebuilt with it; see `ChemPlay::cut_faces`).
    cut_faces: Vec<(Coord, u8)>,
    /// §6f delta render: `Some(tag)` while every play-world and chem-tint change since the last
    /// game render is a chem mirror or (§6s) a dig / place recorded in `delta_changed` and the
    /// world now has `tag`.
    delta_chain: Option<ContentTag>,
    /// Bits whose chem material changed since the last game render, each flagged when its
    /// occupancy may have changed too (see `Renderer::render_tinted_delta`).
    delta_changed: Vec<(Coord, bool)>,
    /// Allow delta renders (on by default; a switch so fixtures can compare with full renders).
    delta_render: bool,
    /// §6ah chem schedule (lagged engine; worker or fallback). Unused outside chem mode.
    link: crate::chem_link::ChemLink,
}

/// Recorded changes past which the delta chain is dropped (the next render is a full one).
const DELTA_CHANGED_MAX: usize = 1 << 20;

type CutViewKey = (ContentTag, crate::play_chem::ChemCut, [i64; 3]);

/// How a rendered tick finalises its frame.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RenderPolicy {
    /// Native-faithful: palette audit, game-frame tag binds the compositor cache, composite
    /// tag computed eagerly. Used by `fixed_tick` (tape / host known answers).
    Audited,
    /// Browser live loop: same pixels, but no per-frame SHA-256 passes; palette audit only in
    /// debug builds; composite tag left for a lazy status read.
    Live,
}

/// Play-client start worlds: `0..=3` are the gallery programs (symbol, recognition,
/// architecture, recursion); [`PLAY_WORLD_EMPTY`] is the native empty spawn (bedrock only).
pub const PLAY_WORLD_EMPTY: u32 = 4;
/// World the play client loads on start: architecture (readable from outside, cheap at High).
pub const PLAY_WORLD_DEFAULT: u32 = 2;

impl Default for PlaySession {
    fn default() -> Self {
        Self::new()
    }
}

impl PlaySession {
    /// A fresh session whose world is gallery program `program_id`, built on the same
    /// `Program::parse` → `World::new_with_origin` → `apply` path as the gallery buttons, and
    /// viewed through the gallery camera (`Camera::from_anchor`, yaw 35°, pitch −22°).
    /// History starts at that world (tick 0); live edits continue from there.
    pub fn with_gallery_program(program_id: u32) -> Option<Self> {
        let (name, source) = crate::GALLERY_PROGRAMS.get(program_id as usize).copied()?;
        let program = Program::parse(source).expect("gallery program parses");
        let mut world = World::new_with_origin();
        let _ = program.apply(&mut world);
        let mut session = Self::new();
        session.history = History::new(&world);
        // History::new already computed this genesis world's canonical tag.
        session.last_world_tag = session.history.current_world();
        session.camera =
            crate::gallery_camera(&world, crate::GALLERY_YAW_DEG, crate::GALLERY_PITCH_DEG);
        session.world = world;
        session.last_action = format!("world:{name}");
        Some(session)
    }

    /// Start world by id: gallery `0..=3`, or [`PLAY_WORLD_EMPTY`] for the empty spawn
    /// (identical to [`PlaySession::new`], the audited tape start). Unknown ids → `None`.
    pub fn with_start_world(world_id: u32) -> Option<Self> {
        if world_id == PLAY_WORLD_EMPTY {
            Some(Self::new())
        } else {
            Self::with_gallery_program(world_id)
        }
    }

    /// A chem world: `genesis` from preset `preset`'s element budget (ash / rocky / ocean),
    /// `seed`, sphere `radius` (clamped to the browser cap) resting on the bedrock bit, with
    /// the trimmed ChemNet attached. Every non-air genesis cell is an on-bit, so the normal
    /// occupancy renderer shows the planet. Viewed through the gallery camera.
    pub fn with_chem(preset: u32, seed: u32, radius: u32) -> Option<Self> {
        crate::play_chem::ChemPlay::new(preset, seed, radius).map(Self::from_chem)
    }

    /// A chem world from a user-entered element budget (lanes H O C N S Ca Fe Ni Si Mg Na K);
    /// otherwise identical to [`PlaySession::with_chem`]. `Err` carries the genesis rejection
    /// (negative lane, over the atom cap, ...) as a space-free token.
    pub fn with_chem_budget(
        lanes: bitblock_core::chem::Lanes,
        seed: u32,
        radius: u32,
    ) -> Result<Self, String> {
        crate::play_chem::ChemPlay::with_budget(lanes, seed, radius).map(Self::from_chem)
    }

    fn from_chem(chem: crate::play_chem::ChemPlay) -> Self {
        let world = chem.world.world().clone();
        let mut session = Self::new();
        session.history = History::new(&world);
        // History::new already computed this genesis world's canonical tag.
        session.last_world_tag = session.history.current_world();
        session.camera =
            crate::gallery_camera(&world, crate::GALLERY_YAW_DEG, crate::GALLERY_PITCH_DEG);
        session.world = world;
        session.last_action = format!("world:chem-{}", chem.preset_name());
        session.link = crate::chem_link::ChemLink::new(&chem);
        // §6ah: the play world is the genesis occupancy, so the mirror cache starts as the
        // genesis materials and the first landing mirrors incrementally (no full scan and no
        // full re-render for it).
        let mut chem = chem;
        chem.mirror_mats = chem.world.mats().to_vec();
        session.chem = Some(chem);
        session
    }

    pub fn chem_active(&self) -> bool {
        self.chem.is_some()
    }

    /// Detailed chem readout (lanes, drift, bag, vapor, tick), or `None` outside chem mode.
    pub fn chem_status(&self) -> Option<String> {
        let dynamics = self.chem_dynamics()?;
        self.chem
            .as_ref()
            .map(|c| c.status_with(&self.link.report, dynamics))
    }

    /// Auto-step cadence in sim ticks (0 pauses). Returns false outside chem mode.
    pub fn chem_set_auto(&mut self, every: u32) -> bool {
        match self.chem.as_mut() {
            Some(c) => {
                c.every = every;
                true
            }
            None => false,
        }
    }

    /// Render-only cross-section (`mode` one of `play_chem::CHEM_CUT_*`, `offset` in cells).
    /// Returns false outside chem mode or for an unknown mode. Repaints on change.
    pub fn chem_set_cut(&mut self, mode: u32, offset: i32) -> bool {
        let Some(c) = self.chem.as_mut() else {
            return false;
        };
        let before = c.cut;
        if !c.set_cut(mode, offset) {
            return false;
        }
        if c.cut != before {
            self.needs_render = true;
            if !c.cut.active() {
                self.cut_view = None;
            }
        }
        true
    }

    /// Tick dynamics on the live chem world (`energy` one of `play_chem::CHEM_ENERGY_*`, `cycle`
    /// one of `play_chem::CHEM_CYCLE_*`). Returns false outside chem mode or for an unknown mode.
    /// Pixels change only through later chem steps.
    /// §6ah: queued like an edit; the engine applies it before the steps of the next batch.
    pub fn chem_set_dynamics(&mut self, energy: u32, cycle: u32) -> bool {
        use crate::play_chem::{CHEM_CYCLE_NAMES, CHEM_ENERGY_NAMES};
        if self.chem.is_none()
            || energy as usize >= CHEM_ENERGY_NAMES.len()
            || cycle as usize >= CHEM_CYCLE_NAMES.len()
        {
            return false;
        }
        self.link
            .pending
            .push(crate::chem_link::ChemOp::Dynamics(energy, cycle));
        true
    }

    /// `(energy, cycle)` modes of the chem world, or `None` outside chem mode. §6ah: the last
    /// requested modes, including ones still queued for the engine.
    pub fn chem_dynamics(&self) -> Option<(u32, u32)> {
        let c = self.chem.as_ref()?;
        let queued = self
            .link
            .pending
            .iter()
            .rev()
            .chain(self.link.in_flight.iter().flat_map(|b| b.ops.iter().rev()))
            .find_map(|op| match op {
                crate::chem_link::ChemOp::Dynamics(e, y) => Some((*e, *y)),
                _ => None,
            });
        Some(queued.unwrap_or((c.energy, c.cycle)))
    }

    /// The active cross-section's cut (off outside chem mode).
    pub fn chem_cut(&self) -> crate::play_chem::ChemCut {
        self.chem.as_ref().map(|c| c.cut).unwrap_or_default()
    }

    /// Rebuilds the cross-section view if the world, cut or camera side changed. Returns
    /// whether a cut is active (and `cut_view` is current).
    fn refresh_cut_view(&mut self) -> bool {
        let Some(chem) = self.chem.as_ref().filter(|c| c.cut.active()) else {
            self.cut_view = None;
            if !self.cut_faces.is_empty() {
                self.cut_faces = Vec::new();
            }
            return false;
        };
        let key = (
            self.last_world_tag,
            chem.cut,
            chem.cut_signs(self.camera.position()),
        );
        if self.cut_view.as_ref().is_none_or(|(k, _)| *k != key) {
            self.cut_view = Some((key, chem.cut_view(&self.world, key.2)));
            self.cut_faces = chem.cut_faces(&self.world, key.2);
        }
        true
    }

    /// Queue `n` manual chem steps, run inside the next sim tick(s).
    pub fn chem_queue(&mut self, n: u32) -> bool {
        match self.chem.as_mut() {
            Some(c) => {
                c.queued = c.queued.saturating_add(n);
                true
            }
            None => false,
        }
    }

    pub fn new() -> Self {
        let world = World::new();
        let history = History::new(&world);
        let last_world_tag = history.current_world();
        Self {
            world,
            history,
            camera: Camera::for_observer_spawn(),
            renderer: Renderer::new(PLAY_VIEWPORT, PLAY_VIEWPORT),
            compositor: FrameCompositor::new(PLAY_OUTPUT_WIDTH, PLAY_OUTPUT_HEIGHT, PLAY_VIEWPORT),
            inputs: Inputs::default(),
            mouse_captured: false,
            cursor_x: 0.0,
            cursor_y: 0.0,
            sidebar_visible: true,
            needs_render: true,
            needs_sidebar_render: false,
            tick: 0,
            last_action: "reset".to_owned(),
            has_frame: false,
            frame_tag: Cell::new(None),
            last_world_tag,
            last_sidebar_facts: None,
            game_only: false,
            compositor_bound: false,
            rgba: Vec::new(),
            rgba_game_serial: None,
            renders: 0,
            dirty: true,
            chem: None,
            cut_view: None,
            cut_faces: Vec::new(),
            delta_chain: None,
            delta_changed: Vec::new(),
            delta_render: true,
            link: crate::chem_link::ChemLink::default(),
        }
    }

    /// Turns delta game renders on or off (on by default). Frames are identical either way.
    pub fn set_delta_render(&mut self, on: bool) {
        self.delta_render = on;
        self.delta_chain = None;
    }

    /// Game frames drawn as deltas and delta index compactions so far (diagnostics only).
    pub fn delta_counts(&self) -> (u64, u64) {
        (
            self.renderer.delta_frames(),
            self.renderer.delta_compactions(),
        )
    }

    /// Output buffer width/height (game + sidebar, or game only).
    pub fn output_width(&self) -> u32 {
        if self.game_only {
            self.renderer.width()
        } else {
            self.compositor.width()
        }
    }

    pub fn output_height(&self) -> u32 {
        if self.game_only {
            self.renderer.height()
        } else {
            self.compositor.height()
        }
    }

    pub fn simulation_size(&self) -> (u32, u32) {
        (self.renderer.width(), self.renderer.height())
    }

    pub fn game_only(&self) -> bool {
        self.game_only
    }

    /// Number of frames finalised into the output buffer since reset.
    pub fn renders(&self) -> u64 {
        self.renders
    }

    fn output_pixels(&self) -> &[u32] {
        if self.game_only {
            self.renderer.pixels()
        } else {
            self.compositor.pixels()
        }
    }

    fn output_frame_tag(&self) -> ContentTag {
        FrameRef::new(
            self.output_width(),
            self.output_height(),
            self.output_pixels(),
        )
        .expect("play output framebuffer")
        .content_tag()
    }

    /// Full status, hashing the current output frame if its tag is not cached yet.
    pub fn status(&self) -> String {
        self.status_with(true)
    }

    /// Status line. With `hash_frame == false` an uncached live frame reports `frame=pending`
    /// instead of paying a full-frame SHA-256 on the hot path.
    pub fn status_with(&self, hash_frame: bool) -> String {
        let frame = if !self.has_frame {
            "none".to_owned()
        } else if let Some(tag) = self.frame_tag.get() {
            tag.to_string()
        } else if hash_frame {
            let tag = self.output_frame_tag();
            self.frame_tag.set(Some(tag));
            tag.to_string()
        } else {
            "pending".to_owned()
        };
        let pos = self.camera.position();
        let chem = self
            .chem
            .as_ref()
            .zip(self.chem_dynamics())
            .map(|(c, d)| c.status_fields_with(&self.link.report, d))
            .unwrap_or_default();
        format!(
            "bitblock_browser_play ok tick={} world={} frame={} pos={:.12},{:.12},{:.12} yaw={:.12} pitch={:.12} dist={:.12} last={} sidebar={} captured={} output={}x{}{}",
            self.tick,
            self.last_world_tag,
            frame,
            pos.x,
            pos.y,
            pos.z,
            self.camera.yaw_radians(),
            self.camera.pitch_radians(),
            self.camera.observation_distance(),
            self.last_action,
            u8::from(self.sidebar_visible),
            u8::from(self.mouse_captured),
            self.output_width(),
            self.output_height(),
            chem,
        )
    }

    pub fn rgba(&self) -> &[u8] {
        // SAFETY: u32 -> u8 reinterpretation of an initialised, live slice; u8 has alignment 1
        // and every byte pattern is valid. Words were built with `from_ne_bytes([r, g, b, 255])`
        // so memory order is RGBA on any endianness.
        unsafe { std::slice::from_raw_parts(self.rgba.as_ptr().cast::<u8>(), self.rgba.len() * 4) }
    }

    /// Browser resolution / layout control.
    ///
    /// `sidebar == true` is the native layout and goes through the same `Resize` action the
    /// native window applies (`width = sim + 400`). `sidebar == false` is a browser-only layout
    /// in which the game view is the whole output. Both refit the camera when the world was
    /// top-saturated, as native `App::resize` does.
    pub fn set_layout(&mut self, sim_width: u32, sim_height: u32, sidebar: bool) {
        let sim_width = sim_width.clamp(PLAY_MIN_DIM, PLAY_MAX_WIDTH);
        let sim_height = sim_height.clamp(PLAY_MIN_DIM, PLAY_MAX_HEIGHT);
        if sidebar {
            self.game_only = false;
            let width = sim_width + bitblock_core::sidebar::SIDEBAR_WIDTH;
            let _ = self.apply_accepted_action(AcceptedAction::Resize(width, sim_height));
        } else {
            let was_top = self.was_top_saturated();
            self.game_only = true;
            self.renderer.resize(sim_width, sim_height);
            self.needs_render = true;
            self.inputs.viewport_refit |= was_top;
        }
        self.has_frame = false;
        self.frame_tag.set(None);
    }

    fn was_top_saturated(&self) -> bool {
        self.world.bounds().is_some_and(|bounds| {
            let (min, max) = bounds_aabb(bounds);
            self.camera.bounds_fit_one_pixel(
                min,
                max,
                self.renderer.width(),
                self.renderer.height(),
            )
        })
    }

    pub fn dirty(&self) -> bool {
        self.dirty
    }

    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    pub fn set_cursor(&mut self, x: f64, y: f64) {
        self.cursor_x = x;
        self.cursor_y = y;
    }

    pub fn set_capture(&mut self, capture: bool) {
        if capture {
            self.mouse_captured = true;
            self.last_action = "capture".to_owned();
        } else if self.mouse_captured {
            self.mouse_captured = false;
            let _ = self.apply_accepted_action(AcceptedAction::ClearMotion);
            self.last_action = "release".to_owned();
        }
    }

    pub fn key(&mut self, code: u32, pressed: bool) {
        match code {
            PLAY_KEY_W => {
                let _ = self
                    .apply_accepted_action(AcceptedAction::MoveEdge(MoveControl::Forward, pressed));
                self.last_action = if pressed { "key:W:down" } else { "key:W:up" }.to_owned();
            }
            PLAY_KEY_S => {
                let _ = self.apply_accepted_action(AcceptedAction::MoveEdge(
                    MoveControl::Backward,
                    pressed,
                ));
                self.last_action = if pressed { "key:S:down" } else { "key:S:up" }.to_owned();
            }
            PLAY_KEY_A => {
                let _ = self
                    .apply_accepted_action(AcceptedAction::MoveEdge(MoveControl::Left, pressed));
                self.last_action = if pressed { "key:A:down" } else { "key:A:up" }.to_owned();
            }
            PLAY_KEY_D => {
                let _ = self
                    .apply_accepted_action(AcceptedAction::MoveEdge(MoveControl::Right, pressed));
                self.last_action = if pressed { "key:D:down" } else { "key:D:up" }.to_owned();
            }
            PLAY_KEY_SPACE => {
                let _ =
                    self.apply_accepted_action(AcceptedAction::MoveEdge(MoveControl::Up, pressed));
                self.last_action = if pressed {
                    "key:Space:down"
                } else {
                    "key:Space:up"
                }
                .to_owned();
            }
            PLAY_KEY_SHIFT => {
                let _ = self
                    .apply_accepted_action(AcceptedAction::MoveEdge(MoveControl::Down, pressed));
                self.last_action = if pressed {
                    "key:Shift:down"
                } else {
                    "key:Shift:up"
                }
                .to_owned();
            }
            PLAY_KEY_F if pressed => {
                let _ = self.apply_accepted_action(AcceptedAction::FitWorld);
                self.last_action = "key:F".to_owned();
            }
            PLAY_KEY_F3 if pressed => {
                let _ = self.apply_accepted_action(AcceptedAction::ToggleSidebar);
                self.last_action = "key:F3".to_owned();
            }
            PLAY_KEY_ESCAPE if pressed => {
                self.set_capture(false);
                self.last_action = "key:Escape".to_owned();
            }
            _ => {}
        }
    }

    pub fn look(&mut self, dx: f64, dy: f64) {
        if !self.mouse_captured {
            return;
        }
        if self
            .apply_accepted_action(AcceptedAction::LookDelta(dx, dy))
            .is_ok()
        {
            self.last_action = format!("look:{dx}:{dy}");
        }
    }

    pub fn wheel(&mut self, lines: f64) {
        if !self.mouse_captured && self.pointer_in_sidebar() {
            return;
        }
        if self
            .apply_accepted_action(AcceptedAction::WheelLines(lines))
            .is_ok()
        {
            self.last_action = format!("wheel:{lines}");
        }
    }

    /// Native mouse-button gate: uncaptured click on the game view captures;
    /// captured LMB → AimDelete, RMB → AimPlace.
    pub fn pointer_button(&mut self, button: u32, pressed: bool) {
        if !pressed {
            return;
        }
        if !self.mouse_captured {
            if !self.pointer_in_sidebar() {
                self.set_capture(true);
            }
            return;
        }
        match button {
            0 => {
                let _ = self.apply_accepted_action(AcceptedAction::AimDelete);
                self.last_action = "aim:delete".to_owned();
            }
            2 => {
                let _ = self.apply_accepted_action(AcceptedAction::AimPlace);
                self.last_action = "aim:place".to_owned();
            }
            _ => {}
        }
    }

    /// Applies one InteractionTape record's actions then one fixed tick — same
    /// order as headless `simulate_interaction_tape` in main.rs.
    pub fn apply_tape(&mut self, tape: &InteractionTape) {
        for record in tape.records() {
            for &action in record.actions() {
                let _ = self.apply_accepted_action(action);
                // Same last-action labels the browser key/mouse handlers emit,
                // so the host tape string matches the live JS play-tape path.
                self.last_action = handler_label(action);
            }
            self.fixed_tick();
        }
    }

    /// One native-faithful fixed tick: simulate, then render + compose with the audited policy
    /// (palette audit, eager frame tags). The tape and host known answers run through here.
    pub fn fixed_tick(&mut self) -> bool {
        if self.chem_blocked() {
            return false;
        }
        let source_tick = self.tick;
        self.sim_tick();
        self.render_pending(source_tick, RenderPolicy::Audited)
    }

    /// Browser live step: run `ticks` fixed simulation ticks, then finalise at most ONE frame
    /// for the last of them (live policy). World / camera / history evolve exactly as with
    /// `ticks` calls to `fixed_tick`; only intermediate frames nobody would see are skipped.
    pub fn step(&mut self, ticks: u32) -> bool {
        for _ in 0..ticks {
            if self.chem_blocked() {
                break;
            }
            self.sim_tick();
        }
        if self.tick == 0 {
            return false;
        }
        self.render_pending(self.tick - 1, RenderPolicy::Live)
    }

    /// §6s browser live step that never catches up with a second chem step: runs up to `ticks`
    /// fixed simulation ticks but stops before a tick on which a chem step is due once one has
    /// run in this call, then finalises at most ONE frame (live policy) like [`Self::step`].
    /// Returns whether a frame was finalised and how many ticks ran; the caller owes the rest to
    /// later calls (§6ab, bounded) or drops them. It never runs two chem steps in one frame,
    /// which after one late frame kept every later frame late. The ticks that run are exactly
    /// the next ones `fixed_tick` would run, so world / history / books evolve as with that
    /// many `fixed_tick` calls; only which ticks get presented differs.
    pub fn step_paced(&mut self, ticks: u32) -> (bool, u32) {
        let (painted, _, ran) = self.step_lagged_inner(ticks, false);
        (painted, ran)
    }

    /// §6ah worker-path live step: [`Self::step_paced`] that also stops before a tick that
    /// must wait for the worker's result, and returns right after a tick that sent a batch
    /// (without rendering) so the client can post it to the worker at once. Returns
    /// `(painted, sent, ran)`; after a send the client calls again for the rest of its ticks
    /// (zero ticks just finalises the pending frame).
    pub fn step_lagged(&mut self, ticks: u32) -> (bool, bool, u32) {
        self.step_lagged_inner(ticks, true)
    }

    fn step_lagged_inner(&mut self, ticks: u32, stop_on_send: bool) -> (bool, bool, u32) {
        let mut ran = 0;
        let mut chem_ran = false;
        while ran < ticks {
            if self.chem_blocked() {
                break;
            }
            // §6ah: the expensive chem tick on this thread is a landing (the fallback runs the
            // batch there; the worker path mirrors its result).
            let heavy = self
                .chem
                .as_ref()
                .is_some_and(|c| self.link.lands_on(c, self.tick) || c.due_on(self.tick));
            if heavy && chem_ran {
                break;
            }
            chem_ran |= heavy;
            let sent = self.link.sent;
            self.sim_tick();
            ran += 1;
            if stop_on_send && self.link.remote && self.link.sent != sent {
                return (false, true, ran);
            }
        }
        if self.tick == 0 {
            return (false, false, ran);
        }
        (
            self.render_pending(self.tick - 1, RenderPolicy::Live),
            false,
            ran,
        )
    }

    /// §6ah worker path: the next tick must wait for the worker's result.
    pub fn chem_blocked(&self) -> bool {
        self.chem
            .as_ref()
            .is_some_and(|c| self.link.blocked(c, self.tick))
    }

    /// §6ah: switch this chem session to the worker path (its chem world becomes a display
    /// replica of the worker's). Only before the first batch; false otherwise or outside chem.
    pub fn chem_set_remote(&mut self, remote: bool) -> bool {
        if self.chem.is_none() || self.link.sent > 0 {
            return false;
        }
        self.link.remote = remote;
        true
    }

    /// Recover a failed worker without resetting the play session or losing edits. Rebuild
    /// the engine from genesis and replay only the already-published batches. The in-flight
    /// batch and pending ops keep their original landing/order on the local lagged schedule.
    /// Replay does not touch the play world, camera, inputs, bag or history.
    pub fn chem_recover_local(&mut self) -> bool {
        if !self.link.remote {
            return false;
        }
        let Some(c) = self.chem.as_mut() else {
            return false;
        };
        let mut engine = if c.preset == crate::play_chem::CHEM_PRESET_CUSTOM {
            crate::play_chem::ChemPlay::with_budget(c.budget, c.seed, c.radius).ok()
        } else {
            crate::play_chem::ChemPlay::new(c.preset, c.seed, c.radius)
        }
        .expect("same genesis already loaded successfully");
        for batch in &self.link.published_batches {
            engine.process(batch);
        }
        assert_eq!(engine.world.cells(), c.world.cells(), "recovery domain");
        assert_eq!(engine.world.mats(), c.world.mats(), "recovery materials");
        // Only replace the replica's engine data. Its mirror cache/render state, requested
        // cadence, manual queue and cut settings still describe the published play world.
        c.world = engine.world;
        c.place_mat = engine.place_mat;
        c.last = engine.last;
        c.steps_run = engine.steps_run;
        c.energy = engine.energy;
        c.cycle = engine.cycle;
        c.error = engine.error;
        c.books_ahead = None;
        self.link.remote = false;
        self.link.inbox = None;
        self.link.outbox = None;
        self.link.published_batches.clear();
        true
    }

    /// §6ah worker path: the encoded batch of the last send tick, if not taken yet.
    pub fn chem_take_outbox(&mut self) -> Option<Vec<u8>> {
        self.link.outbox.take()
    }

    /// §6ah worker path: delivers the worker's encoded result for the batch in flight. False
    /// (ignored) if no batch is in flight or a result is already waiting.
    pub fn chem_put_inbox(&mut self, bytes: Vec<u8>) -> bool {
        if !self.link.remote || self.link.in_flight.is_none() || self.link.inbox.is_some() {
            return false;
        }
        self.link.inbox = Some(bytes);
        true
    }

    /// §6ah: the engine's chem tick as of the last landing (what the play world shows).
    pub fn chem_published_tick(&self) -> Option<u64> {
        self.chem.as_ref().map(|_| self.link.report.tick)
    }

    fn sim_tick(&mut self) {
        let pre_world = self.history.current_world();
        debug_assert_eq!(world_tag(&self.world), pre_world);

        if self.inputs.viewport_refit {
            self.fit_world();
            self.inputs.viewport_refit = false;
            self.needs_render = true;
        }

        if self.inputs.mouse_x != 0.0 || self.inputs.mouse_y != 0.0 {
            self.camera.apply_mouse_delta(
                self.inputs.mouse_x,
                self.inputs.mouse_y,
                DEFAULT_MOUSE_SENSITIVITY,
            );
            self.inputs.mouse_x = 0.0;
            self.inputs.mouse_y = 0.0;
            self.needs_render = true;
        }

        let local = Vec3::new(
            axis(self.inputs.right, self.inputs.left),
            axis(self.inputs.up, self.inputs.down),
            axis(self.inputs.forward, self.inputs.backward),
        );
        if let Some(direction) = local.try_normalized() {
            let speed = (self.camera.observation_distance() * 0.75).max(1.0);
            self.needs_render |= self.translate_observer(direction * (speed / TICK_RATE));
        }

        if self.inputs.fit_world {
            self.fit_world();
            self.inputs.fit_world = false;
            self.needs_render = true;
        }

        if self.inputs.wheel != 0.0 {
            let wheel = self.inputs.wheel;
            self.inputs.wheel = 0.0;
            let max_distance = self
                .world
                .bounds()
                .map(|bounds| {
                    let (min, max) = bounds_aabb(bounds);
                    self.camera.fit_distance_for_bounds(
                        min,
                        max,
                        self.renderer.width().max(1),
                        self.renderer.height().max(1),
                    )
                })
                .unwrap_or(MAX_OBSERVATION_DISTANCE)
                .max(BLOCK_FILL_DISTANCE);
            self.needs_render |= self.dolly_observer(wheel, BLOCK_FILL_DISTANCE, max_distance);
        }

        let _consumed = std::mem::take(&mut self.inputs.actions);
        let pending_edits = std::mem::take(&mut self.inputs.edits);
        let mut transaction = self.history.begin_tick(self.tick);
        if self.chem.is_some() {
            self.queue_chem_edits(pending_edits);
            self.chem_tick(&mut transaction);
        } else {
            self.apply_edits(pending_edits, &mut transaction);
        }

        let post_world = match self.history.commit_tick(transaction, &self.world) {
            Some(receipt) => receipt.post_world(),
            None => pre_world,
        };
        self.last_world_tag = post_world;
        self.tick = self.tick.checked_add(1).expect("tick counter overflow");
    }

    /// Applies a non-chem tick's edits to the play world (chem worlds queue them instead).
    fn apply_edits(&mut self, pending_edits: Vec<PendingEdit>, transaction: &mut TickTransaction) {
        debug_assert!(self.chem.is_none());
        for pending in pending_edits {
            let edit = match pending {
                PendingEdit::Aim(aim) => {
                    self.refresh_cut_view();
                    let Some(edit) = resolve_aim_edit(&self.world, &self.camera, aim) else {
                        continue;
                    };
                    match aim {
                        AimEdit::Delete => debug_assert!(!edit_would_enclose_observer(
                            &self.world,
                            &self.camera,
                            edit
                        )),
                        AimEdit::Place => {
                            if edit_would_enclose_observer(&self.world, &self.camera, edit) {
                                continue;
                            }
                        }
                    }
                    edit
                }
                PendingEdit::Exact(edit) => {
                    if edit_would_enclose_observer(&self.world, &self.camera, edit) {
                        continue;
                    }
                    edit
                }
            };
            let applied = transaction.apply_edit(&self.history, &mut self.world, edit);
            self.needs_render |= applied;
            self.cut_view = None;
        }
    }

    /// §6ah: resolves a chem tick's edits against the play world as last published (aim ray,
    /// with a cross-section on through the view the player sees; enclose-observer check; cut
    /// refusal) and queues them, in order, for the chem engine's next batch. The play world is
    /// not edited here: an applied edit reaches it through the mirror of its batch.
    fn queue_chem_edits(&mut self, pending_edits: Vec<PendingEdit>) {
        use crate::chem_link::ChemOp;
        for pending in pending_edits {
            let op = match pending {
                PendingEdit::Aim(aim) => {
                    let cut = self.refresh_cut_view();
                    let aim_world = match (&self.cut_view, cut) {
                        (Some((_, view)), true) => view,
                        _ => &self.world,
                    };
                    let Some(edit) = resolve_aim_edit(aim_world, &self.camera, aim) else {
                        continue;
                    };
                    match aim {
                        AimEdit::Delete => ChemOp::Dig(edit.coord),
                        AimEdit::Place => {
                            if edit_would_enclose_observer(&self.world, &self.camera, edit) {
                                continue;
                            }
                            let chem = self.chem.as_ref().expect("chem session");
                            // Never place into the hidden side of a cut (it would be invisible)
                            // or onto a bit the cut only hides.
                            if cut
                                && (self.world.is_on(edit.coord)
                                    || chem.cut_hides(
                                        edit.coord,
                                        chem.cut_signs(self.camera.position()),
                                    ))
                            {
                                ChemOp::Refuse("place_refused:cut".to_owned())
                            } else {
                                ChemOp::Place(edit.coord)
                            }
                        }
                    }
                }
                PendingEdit::Exact(edit) => {
                    if edit_would_enclose_observer(&self.world, &self.camera, edit) {
                        continue;
                    }
                    ChemOp::Exact(edit.operation, edit.coord)
                }
            };
            self.link.pending.push(op);
        }
    }

    /// §6ah chem schedule for this tick: land the batch in flight if it is due (fallback:
    /// run it now; worker path: apply the worker's result to the replica), mirror the chem
    /// world into this tick's transaction, then send the next batch if one is due.
    fn chem_tick(&mut self, transaction: &mut TickTransaction) {
        let tick = self.tick;
        let Some(chem) = self.chem.as_mut() else {
            return;
        };
        if self.link.lands_on(chem, tick) {
            let batch = self.link.in_flight.take().expect("a batch in flight");
            let refused = if self.link.remote {
                let bytes = self.link.inbox.take().expect("a blocked tick never runs");
                let delta =
                    crate::chem_link::decode_delta(&bytes).expect("well-formed chem result");
                assert!(
                    chem.replicate(&delta, &batch),
                    "chem result fits the replica"
                );
                self.link.report = delta.report;
                self.link.published_batches.push(batch.clone());
                delta.refused
            } else {
                let (report, refused) = chem.process(&batch);
                self.link.report = report;
                refused
            };
            self.link.landed += 1;
            if refused > 0 {
                self.needs_sidebar_render = true;
            }
            // The chem occupancy diff (over the chem domain) lands in this tick's transaction,
            // so the play world mirrors the published chem world bit for bit.
            let pre = self.delta_chain.map(|_| world_tag(&self.world));
            let (repaint, full) = mirror_chem(
                chem,
                &self.history,
                &mut self.world,
                transaction,
                &mut self.delta_changed,
            );
            self.needs_render |= repaint;
            self.delta_chain = match self.delta_chain {
                Some(chain)
                    if !full
                        && pre == Some(chain)
                        && self.delta_changed.len() <= DELTA_CHANGED_MAX =>
                {
                    Some(world_tag(&self.world))
                }
                _ => None,
            };
        }
        if self.link.sends_on(chem, tick) {
            let batch = crate::chem_link::ChemBatch {
                steps: chem.take_due(tick),
                ops: std::mem::take(&mut self.link.pending),
            };
            if self.link.remote {
                self.link.outbox = Some(crate::chem_link::encode_batch(&batch));
            }
            self.link.in_flight = Some(batch);
            self.link.sent += 1;
        }
    }

    /// Mirrors the render + compose tail of native `fixed_tick` for `source_tick`.
    fn render_pending(&mut self, source_tick: u64, policy: RenderPolicy) -> bool {
        let game_rendered = std::mem::take(&mut self.needs_render);
        let sidebar_rendered = std::mem::take(&mut self.needs_sidebar_render);
        let rendered = game_rendered || (sidebar_rendered && !self.game_only);
        if !rendered {
            return false;
        }

        if game_rendered {
            self.render_game();
            if policy == RenderPolicy::Audited || cfg!(debug_assertions) {
                self.audit_game_palette();
            }
        }

        if !self.game_only {
            let snapshot = self.sidebar_snapshot(source_tick);
            let facts = match policy {
                RenderPolicy::Audited => {
                    let game_tag = FrameRef::new(
                        self.renderer.width(),
                        self.renderer.height(),
                        self.renderer.pixels(),
                    )
                    .expect("play game framebuffer")
                    .content_tag();
                    let facts = if game_rendered || !self.compositor_bound {
                        self.compositor
                            .compose(self.renderer.pixels(), game_tag, &snapshot)
                    } else {
                        self.compositor.compose_sidebar_only(game_tag, &snapshot)
                    };
                    self.compositor_bound = true;
                    facts
                }
                // Same pixels as compose / compose_sidebar_only (the game prefix copy is a
                // memcpy of the unchanged game frame), without hashing the game frame.
                RenderPolicy::Live => {
                    self.compositor_bound = false;
                    self.compositor
                        .compose_unbound(self.renderer.pixels(), &snapshot)
                }
            };
            self.last_sidebar_facts = Some(facts);
        }

        self.has_frame = true;
        self.frame_tag.set(match policy {
            RenderPolicy::Audited => Some(self.output_frame_tag()),
            RenderPolicy::Live => None,
        });
        self.convert_rgba();
        self.renders += 1;
        self.dirty = true;
        true
    }

    fn render_game(&mut self) {
        let overlay = crosshair_overlay_input();
        self.refresh_cut_view();
        match self.chem.as_ref() {
            // Chem worlds: each bit is tinted by its chem material (one table lookup per drawn
            // bit); the bedrock bit and non-chem bits keep the ordinary occupancy colours.
            Some(chem) => {
                let tint = |coord: Coord| {
                    let rgb = chem.colors.by_id[usize::from(chem.world.material(coord).0)];
                    (rgb != 0).then_some(rgb)
                };
                // Cross-section on: draw the filtered view, with the faces the cut exposes lit
                // (flat, slightly lifted) so the bands read; off: the play world, as before.
                match (&self.cut_view, chem.cut.active()) {
                    (Some((_, view)), true) => {
                        let faces = &self.cut_faces;
                        let lit = |coord: Coord| {
                            tint(coord).map(|rgb| {
                                match faces.binary_search_by_key(&coord, |&(c, _)| c) {
                                    Ok(i) => bitblock_core::render::tint_with_lit_faces(
                                        rgb,
                                        u32::from(faces[i].1),
                                    ),
                                    Err(_) => rgb,
                                }
                            })
                        };
                        self.renderer.render_tinted(view, &self.camera, Some(&lit));
                    }
                    _ => {
                        if self.delta_render
                            && chem.colors.never_black()
                            && self.delta_chain == Some(world_tag(&self.world))
                        {
                            self.renderer.render_tinted_delta(
                                &self.world,
                                &self.camera,
                                Some(&tint),
                                &self.delta_changed,
                            );
                        } else {
                            self.renderer
                                .render_tinted(&self.world, &self.camera, Some(&tint));
                        }
                        if self.delta_render && chem.colors.never_black() {
                            self.renderer.prepare_delta_lists();
                        }
                    }
                }
            }
            None => self.renderer.render(&self.world, &self.camera),
        }
        // The next delta starts from this frame (only the plain tinted chem view chains).
        self.delta_changed.clear();
        self.delta_chain = self
            .chem
            .as_ref()
            .filter(|c| !c.cut.active())
            .map(|_| world_tag(&self.world));
        self.renderer.draw_crosshair(overlay);
    }

    fn audit_game_palette(&self) {
        let (_game_frame, palette_audit) = match self.chem.as_ref() {
            Some(chem) => self
                .renderer
                .observe_final_game_frame_tinted(&|rgb| chem.colors.is_legal(rgb)),
            None => self.renderer.observe_final_game_frame(),
        };
        assert!(
            palette_audit.is_valid(),
            "play renderer produced illegal final RGB colors"
        );
    }

    /// XRGB8888 -> RGBA8888 into the reused word buffer (no per-frame allocation).
    ///
    /// In the game-only layout, when the buffer already holds the previous renderer frame, only
    /// the per-row ranges the renderer reports as possibly changed are reconverted: every other
    /// pixel is identical in both frames, so the result equals a full conversion.
    fn convert_rgba(&mut self) {
        let len = self.output_pixels().len();
        let mut rgba = std::mem::take(&mut self.rgba);
        let serial = self.renderer.frame_serial();
        let incremental = self.game_only
            && rgba.len() == len
            && self
                .rgba_game_serial
                .is_some_and(|s| s.wrapping_add(1) == serial);
        let mut done = false;
        if incremental && let Some(rows) = self.renderer.changed_rows() {
            let width = self.renderer.width() as usize;
            let pixels = self.renderer.pixels();
            for (y, (lo, hi)) in rows.enumerate() {
                if lo < hi {
                    let row = y * width;
                    convert_words(&mut rgba[row + lo..row + hi], &pixels[row + lo..row + hi]);
                }
            }
            done = true;
        }
        if !done {
            rgba.resize(len, 0);
            convert_words(&mut rgba, self.output_pixels());
        }
        self.rgba_game_serial = self.game_only.then_some(serial);
        self.rgba = rgba;
    }

    fn apply_accepted_action(&mut self, action: AcceptedAction) -> Result<(), ApplyError> {
        let mut accounted = self.inputs.actions;
        accounted
            .record(action_lane(action))
            .map_err(ApplyError::Unaccountable)?;

        match action {
            AcceptedAction::MoveEdge(control, pressed) => match control {
                MoveControl::Forward => self.inputs.forward = pressed,
                MoveControl::Backward => self.inputs.backward = pressed,
                MoveControl::Left => self.inputs.left = pressed,
                MoveControl::Right => self.inputs.right = pressed,
                MoveControl::Up => self.inputs.up = pressed,
                MoveControl::Down => self.inputs.down = pressed,
            },
            AcceptedAction::LookDelta(x, y) => {
                let next_x = checked_accumulation("look_x", self.inputs.mouse_x, x)?;
                let next_y = checked_accumulation("look_y", self.inputs.mouse_y, y)?;
                self.inputs.mouse_x = next_x;
                self.inputs.mouse_y = next_y;
            }
            AcceptedAction::WheelLines(lines) => {
                if !lines.is_finite() {
                    return Err(ApplyError::NonFiniteLane {
                        field: "wheel",
                        bits: lines.to_bits(),
                    });
                }
                self.inputs.wheel = (self.inputs.wheel + lines).clamp(-64.0, 64.0);
            }
            AcceptedAction::AimDelete => {
                self.inputs.edits.push(PendingEdit::Aim(AimEdit::Delete));
            }
            AcceptedAction::AimPlace => {
                self.inputs.edits.push(PendingEdit::Aim(AimEdit::Place));
            }
            AcceptedAction::ExactEdit(edit) => {
                self.inputs.edits.push(PendingEdit::Exact(edit));
            }
            AcceptedAction::FitWorld => self.inputs.fit_world = true,
            AcceptedAction::ToggleSidebar => {
                self.sidebar_visible = !self.sidebar_visible;
                self.needs_sidebar_render = true;
            }
            AcceptedAction::Resize(width, height) => {
                if width > 0 && height > 0 {
                    let was_top = self.was_top_saturated();
                    let sidebar_width =
                        bitblock_core::sidebar::SIDEBAR_WIDTH.min(width.saturating_sub(1));
                    let simulation_width = width - sidebar_width;
                    self.renderer.resize(simulation_width, height);
                    self.compositor.resize(width, height, simulation_width);
                    self.compositor_bound = false;
                    self.needs_render = true;
                    self.inputs.viewport_refit |= was_top;
                }
            }
            AcceptedAction::ClearMotion => self.inputs.clear_motion(),
        }
        self.inputs.actions = accounted;
        Ok(())
    }

    fn pointer_in_sidebar(&self) -> bool {
        !self.game_only
            && self.cursor_x >= f64::from(self.compositor.simulation_width())
            && self.cursor_x < f64::from(self.compositor.width())
            && self.cursor_y >= 0.0
            && self.cursor_y < f64::from(self.compositor.height())
    }

    fn fit_world(&mut self) {
        let Some(bounds) = self.world.bounds() else {
            return;
        };
        if self.renderer.width() == 0 || self.renderer.height() == 0 {
            return;
        }
        let (min, max) = bounds_aabb(bounds);
        self.camera
            .fit_bounds(min, max, self.renderer.width(), self.renderer.height());
    }

    fn translate_observer(&mut self, local_delta: Vec3) -> bool {
        let start = self.camera.position();
        let mut candidate = self.camera.clone();
        candidate.translate(local_delta);
        let allowed = collision_limited_delta(&self.world, start, candidate.position() - start);
        if allowed.length_squared() == 0.0 {
            return false;
        }
        self.camera.translate_world(allowed);
        true
    }

    fn dolly_observer(&mut self, wheel: f64, min_distance: f64, max_distance: f64) -> bool {
        let start = self.camera.position();
        let anchor = self.camera.observation_anchor();
        let mut candidate = self.camera.clone();
        candidate.dolly_clamped(wheel, DEFAULT_DOLLY_LOG_STEP, min_distance, max_distance);
        let requested = candidate.position() - start;
        if requested.length_squared() == 0.0 {
            return false;
        }
        let allowed = collision_limited_delta(&self.world, start, requested);
        if allowed.length_squared() == 0.0 {
            return false;
        }
        if allowed == requested {
            self.camera = candidate;
        } else {
            let safe_position = start + allowed;
            self.camera
                .set_observation_distance((anchor - safe_position).length());
        }
        true
    }

    fn sidebar_snapshot(&self, source_tick: u64) -> SidebarSnapshot {
        let top_saturated = self.world.bounds().is_some_and(|bounds| {
            let (min, max) = bounds_aabb(bounds);
            self.camera.bounds_fit_one_pixel(
                min,
                max,
                self.renderer.width(),
                self.renderer.height(),
            )
        });
        SidebarSnapshot {
            frame_source_tick: source_tick,
            tick_index: source_tick
                .checked_add(1)
                .expect("completed tick count overflow"),
            occupied_bits: self.world.len() as u64,
            mutable_bits: self.history.circulating_supply() as u64,
            bounds: self.world.bounds(),
            observer_position: self.camera.position(),
            observer_cell: coord_containing(self.camera.position()),
            yaw_radians: self.camera.yaw_radians(),
            pitch_radians: self.camera.pitch_radians(),
            observation_distance: self.camera.observation_distance(),
            origin_distance: self.camera.position().length(),
            simulation_width: self.renderer.width(),
            simulation_height: self.renderer.height(),
            output_width: self.compositor.width(),
            output_height: self.compositor.height(),
            sidebar_width: self.compositor.sidebar_width(),
            world_pixels: self.renderer.world_pixel_count(),
            top_saturated,
            world: self.history.current_world(),
            history_head: self.history.head(),
            history_len: self.history.receipts().len() as u64,
            prior_frame_head: None,
            prior_frame_len: 0,
            prior_display_head: None,
            prior_display_len: 0,
            display_status: DisplayStatus::Unsupported,
            visible: self.sidebar_visible,
            previous_raster: self.last_sidebar_facts,
        }
    }
}

fn action_lane(action: AcceptedAction) -> usize {
    match action {
        AcceptedAction::MoveEdge(_, _) => LANE_MOVE_EDGE,
        AcceptedAction::LookDelta(_, _) => LANE_LOOK_DELTA,
        AcceptedAction::WheelLines(_) => LANE_WHEEL_LINES,
        AcceptedAction::AimDelete => LANE_AIM_DELETE,
        AcceptedAction::AimPlace => LANE_AIM_PLACE,
        AcceptedAction::ExactEdit(_) => LANE_EXACT_EDIT,
        AcceptedAction::FitWorld => LANE_FIT_WORLD,
        AcceptedAction::ToggleSidebar => LANE_TOGGLE_SIDEBAR,
        AcceptedAction::Resize(_, _) => LANE_RESIZE,
        AcceptedAction::ClearMotion => LANE_CLEAR_MOTION,
    }
}

fn handler_label(action: AcceptedAction) -> String {
    match action {
        AcceptedAction::MoveEdge(MoveControl::Forward, true) => "key:W:down".to_owned(),
        AcceptedAction::MoveEdge(MoveControl::Forward, false) => "key:W:up".to_owned(),
        AcceptedAction::MoveEdge(MoveControl::Backward, true) => "key:S:down".to_owned(),
        AcceptedAction::MoveEdge(MoveControl::Backward, false) => "key:S:up".to_owned(),
        AcceptedAction::MoveEdge(MoveControl::Left, true) => "key:A:down".to_owned(),
        AcceptedAction::MoveEdge(MoveControl::Left, false) => "key:A:up".to_owned(),
        AcceptedAction::MoveEdge(MoveControl::Right, true) => "key:D:down".to_owned(),
        AcceptedAction::MoveEdge(MoveControl::Right, false) => "key:D:up".to_owned(),
        AcceptedAction::MoveEdge(MoveControl::Up, true) => "key:Space:down".to_owned(),
        AcceptedAction::MoveEdge(MoveControl::Up, false) => "key:Space:up".to_owned(),
        AcceptedAction::MoveEdge(MoveControl::Down, true) => "key:Shift:down".to_owned(),
        AcceptedAction::MoveEdge(MoveControl::Down, false) => "key:Shift:up".to_owned(),
        AcceptedAction::LookDelta(x, y) => format!("look:{x}:{y}"),
        AcceptedAction::WheelLines(lines) => format!("wheel:{lines}"),
        AcceptedAction::AimDelete => "aim:delete".to_owned(),
        AcceptedAction::AimPlace => "aim:place".to_owned(),
        AcceptedAction::ExactEdit(edit) => format!("exact:{edit}"),
        AcceptedAction::FitWorld => "key:F".to_owned(),
        AcceptedAction::ToggleSidebar => "key:F3".to_owned(),
        AcceptedAction::Resize(w, h) => format!("resize:{w}x{h}"),
        AcceptedAction::ClearMotion => "release".to_owned(),
    }
}

fn checked_accumulation(field: &'static str, prior: f64, delta: f64) -> Result<f64, ApplyError> {
    if !delta.is_finite() {
        return Err(ApplyError::NonFiniteLane {
            field,
            bits: delta.to_bits(),
        });
    }
    let next = prior + delta;
    if !next.is_finite() {
        return Err(ApplyError::NonFiniteAccumulation {
            field,
            prior_bits: prior.to_bits(),
            delta_bits: delta.to_bits(),
        });
    }
    Ok(next)
}

fn resolve_aim_edit(world: &World, camera: &Camera, edit: AimEdit) -> Option<Edit> {
    let hit = raycast(world, camera.center_ray());
    match edit {
        AimEdit::Delete => hit.map(|hit| Edit::new(Operation::Off, hit.coord)),
        AimEdit::Place => {
            let target = hit
                .and_then(|hit| hit.adjacent)
                .or_else(|| coord_containing(camera.observation_anchor()));
            target.map(|target| Edit::new(Operation::On, target))
        }
    }
}

fn collision_limited_delta(world: &World, start: Vec3, requested: Vec3) -> Vec3 {
    let distance = requested.length();
    if distance == 0.0 {
        return Vec3::ZERO;
    }
    let Some(hit) = sweep_point(world, start, requested) else {
        return requested;
    };
    let coordinate_scale = start
        .x
        .abs()
        .max(start.y.abs())
        .max(start.z.abs())
        .max(hit.distance)
        .max(1.0);
    let clearance = OBSERVER_COLLISION_CLEARANCE.max(coordinate_scale * f64::EPSILON * 16.0);
    let allowed_distance = (hit.distance - clearance).max(0.0);
    requested * (allowed_distance / distance)
}

fn edit_would_enclose_observer(world: &World, camera: &Camera, edit: Edit) -> bool {
    let turns_on = match edit.operation {
        Operation::On => !world.is_on(edit.coord),
        Operation::Toggle => !world.is_on(edit.coord),
        Operation::Off => false,
    };
    turns_on && coord_containing(camera.position()) == Some(edit.coord)
}

fn axis(positive: bool, negative: bool) -> f64 {
    f64::from(positive as u8) - f64::from(negative as u8)
}

/// Bench scenes for the stage profiler (host harness and wasm/Node harness share these).
pub const BENCH_SCENE_SPAWN: u32 = 0;
pub const BENCH_SCENE_FIT: u32 = 1;
pub const BENCH_SCENE_SYMBOL: u32 = 2;
/// `BENCH_SCENE_GALLERY_BASE + id` = gallery program `id` (0 symbol … 3 recursion) as a play world.
pub const BENCH_SCENE_GALLERY_BASE: u32 = 3;

/// Profiler stages. Each runs one piece of the per-frame pipeline on the current state.
pub const BENCH_STAGE_SIM: u32 = 0;
pub const BENCH_STAGE_RENDER: u32 = 1;
pub const BENCH_STAGE_PALETTE_AUDIT: u32 = 2;
pub const BENCH_STAGE_GAME_TAG: u32 = 3;
pub const BENCH_STAGE_COMPOSE: u32 = 4;
pub const BENCH_STAGE_OUTPUT_TAG: u32 = 5;
pub const BENCH_STAGE_RGBA_ALLOC: u32 = 6;
pub const BENCH_STAGE_RGBA_REUSE: u32 = 7;
pub const BENCH_STAGE_PUBLISH_COPY: u32 = 8;
pub const BENCH_STAGE_LIVE_FRAME: u32 = 9;
pub const BENCH_STAGE_AUDITED_FRAME: u32 = 10;
pub const BENCH_STAGES: u32 = 11;

impl PlaySession {
    /// A session posed for profiling: `scene` at `sim_width × sim_height`, with or without the
    /// 400-px sidebar, after one audited tick so every buffer is populated.
    pub fn bench(scene: u32, sim_width: u32, sim_height: u32, sidebar: bool) -> Self {
        let mut session = if scene == BENCH_SCENE_SYMBOL {
            Self::with_gallery_program(0).expect("symbol program id")
        } else if scene >= BENCH_SCENE_GALLERY_BASE {
            Self::with_gallery_program(scene - BENCH_SCENE_GALLERY_BASE).unwrap_or_default()
        } else {
            Self::new()
        };
        session.set_layout(sim_width, sim_height, sidebar);
        if scene == BENCH_SCENE_FIT {
            session.key(PLAY_KEY_F, true);
            session.key(PLAY_KEY_F, false);
        }
        session.fixed_tick();
        session
    }

    /// A chem session posed for profiling (auto-step paused; stages drive chem explicitly).
    pub fn bench_chem(
        preset: u32,
        radius: u32,
        sim_width: u32,
        sim_height: u32,
        sidebar: bool,
    ) -> Option<Self> {
        let mut session = Self::with_chem(preset, 7, radius)?;
        session.chem_set_auto(0);
        session.set_layout(sim_width, sim_height, sidebar);
        session.fixed_tick();
        Some(session)
    }

    /// Chem profiler stage: `0` one planet6 step only, `1` the occupancy mirror + material hash
    /// only, `2` one full sim tick with a chem step due (step + mirror + history commit).
    /// Native only: returns per-p6-stage ns for stage 0 in `p6_ns`.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn bench_chem_stage(&mut self, stage: u32, p6_ns: &mut [u64; 9]) -> u64 {
        match stage {
            0 => {
                let chem = self.chem.as_mut().expect("chem session");
                let (_, ns) = chem.world.step_profiled().expect("chem step");
                *p6_ns = ns;
                chem.steps_run += 1;
                chem.world.tick()
            }
            1 => {
                let chem = self.chem.as_mut().expect("chem session");
                let mut tx = self.history.begin_tick(self.tick);
                self.delta_chain = None;
                let (r, _) = mirror_chem(
                    chem,
                    &self.history,
                    &mut self.world,
                    &mut tx,
                    &mut self.delta_changed,
                );
                if let Some(receipt) = self.history.commit_tick(tx, &self.world) {
                    self.last_world_tag = receipt.post_world();
                }
                self.tick += 1;
                u64::from(r)
            }
            3 => {
                self.delta_chain = None;
                self.renderer.render(&self.world, &self.camera);
                self.renderer.world_pixel_count()
            }
            4 => {
                self.delta_chain = None;
                let c = |_: Coord| Some(0x808080u32);
                self.renderer
                    .render_tinted(&self.world, &self.camera, Some(&c));
                self.renderer.world_pixel_count()
            }
            2 => {
                self.chem_queue(1);
                self.sim_tick();
                self.chem.as_ref().map_or(0, |c| c.world.tick())
            }
            _ => 0,
        }
    }

    /// Runs one profiler stage and returns a value derived from its output (defeats DCE).
    pub fn bench_stage(&mut self, stage: u32, scratch: &mut Vec<u8>) -> u64 {
        match stage {
            BENCH_STAGE_SIM => {
                // A look delta: camera changes, no edits; world tick + history commit.
                self.inputs.mouse_x = 1.0e-9;
                self.sim_tick();
                self.needs_render = false;
                self.tick
            }
            BENCH_STAGE_RENDER => {
                self.render_game();
                self.renderer.world_pixel_count()
            }
            BENCH_STAGE_PALETTE_AUDIT => {
                self.audit_game_palette();
                1
            }
            BENCH_STAGE_GAME_TAG => FrameRef::new(
                self.renderer.width(),
                self.renderer.height(),
                self.renderer.pixels(),
            )
            .expect("game frame")
            .content_tag()
            .digest()[0] as u64,
            BENCH_STAGE_COMPOSE => {
                if self.game_only {
                    return 0;
                }
                let snapshot = self.sidebar_snapshot(self.tick.saturating_sub(1));
                self.compositor_bound = false;
                let facts = self
                    .compositor
                    .compose_unbound(self.renderer.pixels(), &snapshot);
                facts.glyphs as u64
            }
            BENCH_STAGE_OUTPUT_TAG => self.output_frame_tag().digest()[0] as u64,
            BENCH_STAGE_RGBA_ALLOC => crate::xrgb_to_rgba(self.output_pixels()).len() as u64,
            BENCH_STAGE_RGBA_REUSE => {
                self.convert_rgba();
                self.rgba.len() as u64
            }
            BENCH_STAGE_PUBLISH_COPY => {
                scratch.clear();
                scratch.extend_from_slice(self.rgba());
                scratch.len() as u64
            }
            BENCH_STAGE_LIVE_FRAME => {
                self.needs_render = true;
                u64::from(self.render_pending(self.tick.saturating_sub(1), RenderPolicy::Live))
            }
            BENCH_STAGE_AUDITED_FRAME => {
                self.needs_render = true;
                u64::from(self.render_pending(self.tick.saturating_sub(1), RenderPolicy::Audited))
            }
            _ => 0,
        }
    }
}

/// Applies the chem occupancy diff (over the chem domain) to the play world as history edits.
/// Returns whether the game view must repaint, and whether
/// this was a full scan (the cache was empty or resized: `changed` is then not a full record).
/// Every bit whose material changed is appended to `changed`, flagged when its occupancy did.
/// §6u: the material ids of up to 8 cells as one little-endian word (missing bytes 0).
/// §6w: a full group of 8 is converted as one array, so it compiles to a single word load.
#[inline]
fn pack8(m: &[bitblock_core::chem::MatId]) -> u64 {
    if let Ok(m) = <&[bitblock_core::chem::MatId; 8]>::try_from(m) {
        return u64::from_le_bytes(m.map(|x| x.0));
    }
    let mut b = [0u8; 8];
    for (d, s) in b.iter_mut().zip(m) {
        *d = s.0;
    }
    u64::from_le_bytes(b)
}

/// §6u: bit `k` set iff byte `k` of `x` is non-zero.
#[inline]
fn nonzero_bytes(x: u64) -> u64 {
    const LO7: u64 = 0x7f7f_7f7f_7f7f_7f7f;
    let hi = (((x & LO7) + LO7) | x) & !LO7; // 0x80 in each non-zero byte
    ((hi >> 7).wrapping_mul(0x0102_0408_1020_4080)) >> 56
}

/// §6ae: the domain's cells as z-runs `(first index, first cell)` (see
/// [`crate::play_chem::ChemPlay::mirror_rows`]).
fn domain_rows(cells: &[Coord]) -> Vec<(usize, Coord)> {
    let mut rows = Vec::new();
    for (i, &c) in cells.iter().enumerate() {
        let continues = i > 0 && {
            let p = cells[i - 1];
            p.x == c.x && p.y == c.y && p.z.checked_add(1) == Some(c.z)
        };
        if !continues {
            rows.push((i, c));
        }
    }
    rows
}

/// §6ae: the cell at domain index `i` from [`domain_rows`], for ascending `i` (the row cursor
/// `r` only moves forward).
#[inline]
fn row_cell(rows: &[(usize, Coord)], r: &mut usize, i: usize) -> Coord {
    while *r + 1 < rows.len() && rows[*r + 1].0 <= i {
        *r += 1;
    }
    let (start, c) = rows[*r];
    Coord::new(c.x, c.y, c.z + (i - start) as i64)
}

fn mirror_chem(
    chem: &mut crate::play_chem::ChemPlay,
    history: &History,
    world: &mut World,
    transaction: &mut TickTransaction,
    changed_bits: &mut Vec<(Coord, bool)>,
) -> (bool, bool) {
    let mut changed = 0u32;
    // §6s: cells whose material id differs from the mirror cache (all of them on a full scan).
    // A material-only change (rock -> sediment, water -> ice) keeps occupancy but must still
    // repaint the chem tint; comparing the ids exactly replaces the former FNV-1a hash of the
    // whole domain (which repainted on the same changes, up to hash collisions).
    let mut mats_changed = 0u64;
    // §5y: probe occupancy only where the material id changed since the last mirror (same
    // edits as the full scan; the full scan runs when the cache is empty / resized).
    let mut seen = std::mem::take(&mut chem.mirror_mats);
    let full = seen.len() != chem.world.domain_len();
    // Octree upkeep of the play world runs once per changed chunk at the end (exact, see
    // `World::defer_octree`); `apply_edit` reads only bits.
    world.defer_octree();
    if full {
        seen.clear();
        seen.resize(chem.world.domain_len(), bitblock_core::chem::MatId::AIR);
    }
    // Coordinates and occupancy are read only for cells whose material changed, both worlds
    // through chunk-caching probes. The scan runs in blocks of 64 cells: a mask of the changed
    // cells first (no branch per cell), then the changed cells in order.
    // The edits are applied after the scan, in the same order: each domain cell is visited
    // once and an edit flips only its own bit, so every occupancy read before them sees what
    // it would see between them.
    let mats = chem.world.mats();
    let cells = chem.world.cells();
    // §6ae: changed cells are named from the domain's z-runs (a few entries per row) instead
    // of the cell list (24 bytes per cell, most of them read on a busy tick).
    let mut rows = std::mem::take(&mut chem.mirror_rows);
    if rows.0 != cells.len() || rows.1.is_empty() {
        rows = (cells.len(), domain_rows(cells));
    }
    let mut row = 0usize;
    let mut want_probe = chem.world.world().probe();
    let mut have_probe = world.probe();
    let mut edits = std::mem::take(&mut chem.mirror_edits);
    edits.clear();
    for (block, (mats, seen)) in mats.chunks(64).zip(seen.chunks_mut(64)).enumerate() {
        // §6ae: an unchanged block (most blocks on a quiet tick) is skipped whole.
        if !full {
            let mut diff = 0u64;
            for (m8, s8) in mats.chunks(8).zip(seen.chunks(8)) {
                diff |= pack8(m8) ^ pack8(s8);
            }
            if diff == 0 {
                continue;
            }
        }
        let mut mask = 0u64;
        // §6u: changed cells where neither the old nor the new material is AIR (a gas or rock
        // swap). The play world holds each domain cell's bit as the chem occupancy of its
        // mirrored material (a full scan sets every bit, each later mirror applies every
        // toggle, and dig / place apply their edit with the cache update, and nothing else
        // edits the play world), so such a cell is on in both worlds: no probe, no toggle.
        let mut solid = 0u64;
        if full {
            mask = if mats.len() == 64 {
                !0
            } else {
                (1u64 << mats.len()) - 1
            };
        } else {
            for (k, (m8, s8)) in mats.chunks(8).zip(seen.chunks(8)).enumerate() {
                let (a, b) = (pack8(m8), pack8(s8));
                mask |= nonzero_bytes(a ^ b) << (8 * k);
                solid |= (nonzero_bytes(a) & nonzero_bytes(b)) << (8 * k);
            }
        }
        mats_changed += u64::from(mask.count_ones());
        seen.copy_from_slice(mats);
        while mask != 0 {
            let j = mask.trailing_zeros() as usize;
            let i = block * 64 + j;
            mask &= mask - 1;
            let coord = row_cell(&rows.1, &mut row, i);
            debug_assert_eq!(coord, cells[i]);
            if solid >> j & 1 == 1 {
                debug_assert!(want_probe.is_on(coord) && have_probe.is_on(coord));
                changed_bits.push((coord, false));
                continue;
            }
            // The chem world's bit follows from the material except at the bedrock cell.
            let want = bitblock_core::chem::ChemWorld::occupancy_of(mats[i - block * 64], coord)
                .unwrap_or_else(|| want_probe.is_on(coord));
            debug_assert_eq!(want, want_probe.is_on(coord));
            let have = have_probe.is_on(coord);
            let toggled = want != have;
            if !full {
                changed_bits.push((coord, toggled));
            }
            if toggled {
                edits.push((coord, want));
            }
        }
    }
    for &(coord, want) in &edits {
        let op = if want { Operation::On } else { Operation::Off };
        if transaction.apply_edit(history, world, Edit::new(op, coord)) {
            changed += 1;
        }
    }
    chem.mirror_edits = edits;
    chem.mirror_rows = rows;
    world.flush_octree();
    chem.mirror_mats = seen;
    chem.last_step_changed = changed;
    chem.material_changes += mats_changed;
    let recolored = full || mats_changed > 0;
    (changed > 0 || recolored, full)
}

/// Host known-answer: fold [`PLAY_INPUT_TAPE`] through a fresh session.
pub fn play_input_tape_status() -> String {
    let tape = InteractionTape::parse(PLAY_INPUT_TAPE).expect("play input tape parses");
    let mut session = PlaySession::new();
    // Same capture gate the JS play-tape path opens before look / aim clicks.
    session.set_capture(true);
    session.apply_tape(&tape);
    session.status()
}

/// Timed one 1201×801 composed frame (render + compose) on a fresh spawn world.
/// Returns nanoseconds wall time as measured by `Instant` on the host.
/// On `wasm32` Instant is not available without a JS clock; returns 0 there.
#[cfg(not(target_arch = "wasm32"))]
pub fn measure_composed_frame_ns() -> u64 {
    let mut session = PlaySession::new();
    session.needs_render = true;
    let start = Instant::now();
    let _ = session.fixed_tick();
    start.elapsed().as_nanos() as u64
}

#[cfg(target_arch = "wasm32")]
pub fn measure_composed_frame_ns() -> u64 {
    0
}

/// XRGB8888 -> RGBA8888: each output word's memory bytes are `[r, g, b, 255]`
/// (`u32::from_ne_bytes`), built with shifts and masks so the loop vectorises.
fn convert_words(dst: &mut [u32], src: &[u32]) {
    for (dst, &word) in dst.iter_mut().zip(src) {
        let le = ((word >> 16) & 0xFF) | (word & 0xFF00) | ((word & 0xFF) << 16) | 0xFF00_0000;
        *dst = u32::from_le(le);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drive(session: &mut PlaySession) {
        session.set_capture(true);
        session.key(PLAY_KEY_W, true);
        session.look(30.0, 5.0);
    }

    fn frame_of(status: &str) -> &str {
        status
            .split("frame=")
            .nth(1)
            .expect("frame=")
            .split(' ')
            .next()
            .expect("tag")
    }

    fn without_frame(status: &str) -> String {
        status.replace(frame_of(status), "<frame>")
    }

    #[test]
    fn step_n_matches_n_audited_ticks_in_state_and_game_pixels() {
        // Live `step(n)` leaves world / camera / history / game pixels exactly where n
        // native-faithful `fixed_tick`s would. The composite may differ only in the sidebar
        // metrics, whose `previous_raster` line describes the last *presented* raster (one
        // per step instead of one per tick); n == 1 and game-only are therefore identical.
        for n in [1_u32, 3, 7] {
            let mut audited = PlaySession::new();
            let mut live = PlaySession::new();
            drive(&mut audited);
            drive(&mut live);
            for _ in 0..n {
                audited.fixed_tick();
            }
            assert!(live.step(n), "a camera change must finalise a frame");
            assert_eq!(
                without_frame(&live.status()),
                without_frame(&audited.status()),
                "n={n}"
            );
            assert_eq!(live.renderer.pixels(), audited.renderer.pixels(), "n={n}");
            if n == 1 {
                assert_eq!(live.status(), audited.status());
                assert_eq!(live.rgba(), audited.rgba());
            }
            for session in [&mut audited, &mut live] {
                session.key(PLAY_KEY_W, false);
                session.pointer_button(2, true);
                session.key(PLAY_KEY_F3, true);
                session.key(PLAY_KEY_F3, false);
                session.key(PLAY_KEY_F, true);
                session.key(PLAY_KEY_F, false);
                session.wheel(-3.0);
            }
            audited.fixed_tick();
            audited.fixed_tick();
            assert!(live.step(2));
            assert_eq!(
                without_frame(&live.status()),
                without_frame(&audited.status()),
                "phase 2 n={n}"
            );
            assert_eq!(
                live.renderer.pixels(),
                audited.renderer.pixels(),
                "phase 2 n={n}"
            );
        }
    }

    #[test]
    fn game_only_step_n_output_equals_audited_output() {
        let mut audited = PlaySession::new();
        let mut live = PlaySession::new();
        for session in [&mut audited, &mut live] {
            session.set_layout(960, 540, false);
            drive(session);
        }
        for _ in 0..4 {
            audited.fixed_tick();
        }
        assert!(live.step(4));
        assert_eq!(live.status(), audited.status());
        assert_eq!(live.rgba(), audited.rgba());
    }

    #[test]
    fn idle_step_finalises_nothing() {
        let mut session = PlaySession::new();
        assert!(!session.step(0), "no tick yet");
        assert!(session.step(1), "first tick renders the spawn frame");
        let renders = session.renders();
        assert!(
            !session.step(4),
            "nothing changed: dirty-only rendering skips the frame"
        );
        assert_eq!(session.renders(), renders);
        assert!(session.status().contains("tick=5"));
    }

    #[test]
    fn live_frame_tag_is_lazy_and_equals_audited_tag() {
        let mut audited = PlaySession::new();
        let mut live = PlaySession::new();
        audited.fixed_tick();
        live.step(1);
        assert_eq!(frame_of(&live.status_with(false)), "pending");
        let tag = live.status();
        assert_eq!(frame_of(&tag), frame_of(&audited.status()));
        // Cached afterwards: the cheap status now reports it too.
        assert_eq!(live.status_with(false), tag);
    }

    #[test]
    fn live_unbound_compose_equals_audited_compose_frame_by_frame() {
        // One render per tick on both paths, so previous_raster chains agree and the full
        // composite (game + sidebar) must be byte-identical: compose_unbound == compose.
        let mut audited = PlaySession::new();
        let mut live = PlaySession::new();
        for i in 0..4 {
            for session in [&mut audited, &mut live] {
                session.set_capture(true);
                session.look(10.0 + f64::from(i), -2.0);
                if i == 2 {
                    session.key(PLAY_KEY_F3, true);
                    session.key(PLAY_KEY_F3, false);
                }
            }
            audited.fixed_tick();
            assert!(live.step(1));
            assert_eq!(
                live.compositor.pixels(),
                audited.compositor.pixels(),
                "frame {i}"
            );
            assert_eq!(
                live.last_sidebar_facts, audited.last_sidebar_facts,
                "frame {i}"
            );
            assert_eq!(live.status(), audited.status(), "frame {i}");
        }
        // Audited path after a live (unbound) compose must re-bind, not panic.
        audited.key(PLAY_KEY_F3, true);
        live.key(PLAY_KEY_F3, true);
        audited.fixed_tick();
        live.fixed_tick();
        assert_eq!(live.status(), audited.status());
    }

    #[test]
    fn rgba_buffer_matches_reference_conversion_in_every_layout() {
        for (w, h, sidebar) in [
            (801, 801, true),
            (960, 540, false),
            (1280, 720, true),
            (333, 200, false),
        ] {
            let mut session = PlaySession::new();
            session.set_layout(w, h, sidebar);
            assert!(session.step(1));
            assert_eq!(
                session.rgba(),
                &crate::xrgb_to_rgba(session.output_pixels())[..],
                "{w}x{h} {sidebar}"
            );
            assert_eq!(
                session.rgba().len(),
                (session.output_width() * session.output_height() * 4) as usize
            );
        }
    }

    #[test]
    fn set_layout_sizes_and_clamps() {
        let mut session = PlaySession::new();
        assert_eq!(
            (session.output_width(), session.output_height()),
            (1201, 801)
        );
        session.set_layout(1280, 720, true);
        session.step(1);
        assert_eq!(session.simulation_size(), (1280, 720));
        assert_eq!(
            (session.output_width(), session.output_height()),
            (1680, 720)
        );
        assert!(!session.game_only());
        assert!(
            session.status().contains("output=1680x720"),
            "{}",
            session.status()
        );
        session.set_layout(1280, 720, false);
        assert_eq!(
            frame_of(&session.status_with(false)),
            "none",
            "layout change drops the stale frame"
        );
        session.step(1);
        assert!(session.game_only());
        assert_eq!(
            (session.output_width(), session.output_height()),
            (1280, 720)
        );
        session.set_cursor(1279.5, 10.5);
        assert!(
            !session.pointer_in_sidebar(),
            "game-only has no sidebar strip"
        );
        session.set_layout(1, 1, false);
        assert_eq!(session.simulation_size(), (PLAY_MIN_DIM, PLAY_MIN_DIM));
        session.set_layout(100_000, 100_000, true);
        assert_eq!(session.simulation_size(), (PLAY_MAX_WIDTH, PLAY_MAX_HEIGHT));
        assert_eq!(session.output_width(), PLAY_MAX_WIDTH + 400);
    }

    #[test]
    fn layout_round_trip_back_to_native_restores_native_pixels() {
        let mut reference = PlaySession::new();
        reference.fixed_tick();
        reference.fixed_tick();
        let mut session = PlaySession::new();
        session.set_layout(1600, 900, false);
        assert!(session.step(1));
        session.set_layout(801, 801, true);
        session.fixed_tick();
        assert_eq!(
            (session.output_width(), session.output_height()),
            (1201, 801)
        );
        assert_eq!(session.renderer.pixels(), reference.renderer.pixels());
        assert_eq!(
            &session.rgba()[..4 * 801],
            &reference.rgba()[..4 * 801],
            "first game row"
        );
    }

    #[test]
    fn tape_known_answer_survives_live_layouts_on_a_fresh_session() {
        let mut live = PlaySession::new();
        live.set_layout(1920, 1080, false);
        live.step(3);
        // The tape always runs on a reset (native 801+400) session through fixed_tick.
        let tape = play_input_tape_status();
        assert!(
            tape.contains(
                "tick=5 world=#bb.world.sha256.lpqtwwq5lo3dbyid2hrdvmdr3r6wfy5t5xnhd3grwcis5bhu4nra"
            ),
            "{tape}"
        );
        assert!(
            tape.contains(
                "frame=#bb.frame.sha256.vyn2woic4slbg5vo6xxqj4qbx3pg4fsueskewrrh57j7sayknpya"
            ),
            "{tape}"
        );
        assert!(tape.ends_with("output=1201x801"), "{tape}");
    }
}

/// Start-world known answers. Pinned at the play client's DEFAULT layout — High 1600×900 +
/// 400 sidebar (output 2000×900) — after the client's startup sequence: load world →
/// `set_layout(1600, 900, sidebar)` → `step(1)` (live), then a hashing status read.
#[cfg(test)]
mod start_world_tests {
    use super::*;

    const ARCH_HIGH_KNOWN: &str = "bitblock_browser_play ok tick=1 world=#bb.world.sha256.pmbj7za55emc3mjtjvom5domewgjf2rr7qezwpxaxyoohzs5e6gq frame=#bb.frame.sha256.nx4b62u5ozefqrb5m5snhkhcvdtcwsljdgsvzhez5fppgv27unaa pos=-5.838272203333,19.213048230119,-25.261618887543 yaw=0.610865238198 pitch=-0.383972435439 dist=32.602331204992 last=world:architecture sidebar=1 captured=0 output=2000x900";
    const ARCH_HIGH_GAME_NON_BACKGROUND: u32 = 123_265;
    const SYMBOL_HIGH_KNOWN: &str = "bitblock_browser_play ok tick=1 world=#bb.world.sha256.y6n5uuhnnaj25bfdepwlxziifs7einl4bexr5fcqskyvzenf3xba frame=#bb.frame.sha256.djgq2svfynmviug6ngvgsxcga7nhpjlj5qxkeu3wf5isqz5hg4yq pos=-5.208389634264,21.791351208024,-30.502793385755 yaw=0.610865238198 pitch=-0.383972435439 dist=40.819759920901 last=world:symbol sidebar=1 captured=0 output=2000x900";
    const SYMBOL_HIGH_GAME_NON_BACKGROUND: u32 = 82_409;

    fn client_startup(world_id: u32) -> PlaySession {
        let mut s = PlaySession::with_start_world(world_id).expect("start world id");
        s.set_layout(1600, 900, true);
        assert!(s.step(1), "startup step finalises a frame");
        s
    }

    #[test]
    fn default_start_world_is_architecture_at_high_known_answer() {
        assert_eq!(PLAY_WORLD_DEFAULT, 2);
        let s = client_startup(PLAY_WORLD_DEFAULT);
        assert_eq!(s.status(), ARCH_HIGH_KNOWN);
        assert_eq!(
            crate::count_non_background(s.renderer.pixels()),
            ARCH_HIGH_GAME_NON_BACKGROUND
        );
        // Same world as the architecture gallery button.
        let gallery = crate::raster_gallery_program(2, crate::GALLERY_PREVIEW_SIZE).unwrap();
        assert_eq!(s.last_world_tag, gallery.world_tag);
        assert_eq!(s.rgba().len(), 2000 * 900 * 4);
    }

    #[test]
    fn symbol_start_world_at_high_known_answer() {
        let s = client_startup(0);
        assert_eq!(s.status(), SYMBOL_HIGH_KNOWN);
        assert_eq!(
            crate::count_non_background(s.renderer.pixels()),
            SYMBOL_HIGH_GAME_NON_BACKGROUND
        );
    }

    #[test]
    fn audited_tick_gives_the_same_startup_frame() {
        let mut s = PlaySession::with_start_world(PLAY_WORLD_DEFAULT).unwrap();
        s.set_layout(1600, 900, true);
        assert!(s.fixed_tick());
        assert_eq!(s.status(), ARCH_HIGH_KNOWN);
    }

    #[test]
    fn every_gallery_start_world_shows_its_structure() {
        for id in 0..PLAY_WORLD_EMPTY {
            let s = client_startup(id);
            let gallery = crate::raster_gallery_program(id, crate::GALLERY_PREVIEW_SIZE).unwrap();
            assert_eq!(s.last_world_tag, gallery.world_tag, "world id {id}");
            let total = 1600 * 900;
            let nb = crate::count_non_background(s.renderer.pixels());
            // A visible structure on a black sky: neither empty nor a wall filling the view.
            assert!(
                nb > total / 50 && nb < total / 2,
                "world id {id}: non_background={nb}"
            );
        }
        assert!(PlaySession::with_start_world(PLAY_WORLD_EMPTY + 1).is_none());
    }

    #[test]
    fn empty_start_world_is_the_audited_spawn() {
        let a = PlaySession::with_start_world(PLAY_WORLD_EMPTY).unwrap();
        assert_eq!(a.status(), PlaySession::new().status());
    }

    #[test]
    fn live_edits_continue_from_the_start_world() {
        let mut s = client_startup(PLAY_WORLD_DEFAULT);
        let start = s.last_world_tag;
        s.set_capture(true);
        s.pointer_button(2, true); // RMB = aimed place (falls back to the view anchor cell)
        s.pointer_button(2, false);
        assert!(s.step(1));
        assert_ne!(
            s.last_world_tag, start,
            "aimed place edited the gallery world"
        );
        assert_eq!(s.world.iter_on().count(), 664);
        let before = s.camera.position();
        s.key(PLAY_KEY_W, true);
        s.step(30);
        assert_ne!(s.camera.position(), before, "W moves the observer");
    }
}

#[cfg(test)]
mod chem_play_tests {
    use super::*;
    use crate::play_chem::*;
    use bitblock_core::chem::MatId;

    /// §6f: delta game renders change nothing. Chem sessions with and without them, driven
    /// through chem steps, looks, wheel, dig / place clicks and a cross-section on and off,
    /// produce the same output buffer and status (world, frame and history tags) every step,
    /// in the sidebar and the game-only layouts.
    #[test]
    fn delta_render_frames_equal_full_render_frames() {
        for (preset, radius, sidebar) in [
            (2_u32, 14_u32, true),
            (4, 14, true),
            (0, 12, false),
            (2, 16, false),
        ] {
            let mut runs: Vec<PlaySession> = [true, false]
                .into_iter()
                .map(|delta| {
                    let mut s = PlaySession::with_chem(preset, 7, radius).expect("chem genesis");
                    s.set_layout(320, 180, sidebar);
                    s.set_delta_render(delta);
                    s.set_capture(true);
                    s
                })
                .collect();
            for k in 1..=48_u32 {
                for s in &mut runs {
                    if k % 11 == 0 {
                        s.look(23.0, -7.0);
                    }
                    if k % 17 == 0 {
                        s.wheel(1.0);
                    }
                    if k == 9 || k == 30 {
                        s.pointer_button(0, true);
                        s.pointer_button(0, false);
                    }
                    if k == 21 {
                        s.pointer_button(2, true);
                        s.pointer_button(2, false);
                    }
                    if k == 24 {
                        s.chem_set_cut(1, 0);
                    }
                    if k == 28 {
                        s.chem_set_cut(0, 0);
                    }
                    s.step(4);
                }
                let at = format!("preset {preset} r{radius} sidebar {sidebar} step {k}");
                assert!(runs[0].rgba() == runs[1].rgba(), "{at} output");
                assert_eq!(
                    runs[0].status_with(true),
                    runs[1].status_with(true),
                    "{at} status"
                );
            }
            let used = runs[0].renderer.delta_frames();
            assert!(used >= 12, "preset {preset}: only {used} delta frames");
            assert_eq!(runs[1].renderer.delta_frames(), 0);
        }
    }

    /// §6s: dig / place frames go through the delta path (no full re-render) and equal the
    /// full-render frames, on ticks with and without a chem step; a still-camera full render
    /// builds the delta tile lists before the first delta frame, a moving one leaves them to it.
    /// §6t: a place outside the chem domain (which grows it) is a delta frame too.
    /// §6u: after every mirror the play world holds each chem domain cell's bit as the chem
    /// occupancy of the cell's material (what lets the mirror skip the probes of a solid-to-solid
    /// change), through convection, digs, places that grow the domain, and camera moves.
    #[test]
    fn swar_mask_matches_bytewise_compare() {
        use bitblock_core::chem::MatId;
        let mut r = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = || {
            r ^= r << 13;
            r ^= r >> 7;
            r ^= r << 17;
            r
        };
        for len in [1usize, 5, 8, 13, 63, 64] {
            for _ in 0..2000 {
                let pick = |x: u64| {
                    MatId(match x % 4 {
                        0 => 0,
                        1 => 0x80,
                        2 => 1,
                        _ => (x >> 8) as u8,
                    })
                };
                let a: Vec<MatId> = (0..len).map(|_| pick(next())).collect();
                let b: Vec<MatId> = (0..len)
                    .map(|_| if next() % 3 == 0 { a[0] } else { pick(next()) })
                    .collect();
                let (mut m0, mut s0, mut m1, mut s1) = (0u64, 0u64, 0u64, 0u64);
                for j in 0..len {
                    m0 |= u64::from(a[j] != b[j]) << j;
                    s0 |= u64::from(a[j] != MatId::AIR && b[j] != MatId::AIR) << j;
                }
                for (k, (x, y)) in a.chunks(8).zip(b.chunks(8)).enumerate() {
                    let (p, q) = (super::pack8(x), super::pack8(y));
                    m1 |= super::nonzero_bytes(p ^ q) << (8 * k);
                    s1 |= (super::nonzero_bytes(p) & super::nonzero_bytes(q)) << (8 * k);
                }
                assert_eq!((m0, s0), (m1, s1));
            }
        }
    }

    #[test]
    fn play_world_mirrors_chem_occupancy_through_edits() {
        for (preset, radius) in [(4_u32, 14_u32), (2, 14), (0, 12)] {
            let mut s = PlaySession::with_chem(preset, 7, radius).expect("chem genesis");
            s.set_layout(320, 180, true);
            s.set_capture(true);
            let mut swaps = 0u64;
            for k in 1..=36_u32 {
                if k % 3 != 0 {
                    let button = if k % 4 == 1 { 2 } else { 0 };
                    s.pointer_button(button, true);
                    s.pointer_button(button, false);
                }
                if k == 20 {
                    s.look(25.0, -4.0);
                }
                s.step(4);
                let chem = s.chem.as_ref().unwrap();
                swaps = chem.material_changes;
                for (&c, &m) in chem.world.cells().iter().zip(chem.world.mats()) {
                    let want = bitblock_core::chem::ChemWorld::occupancy_of(m, c)
                        .unwrap_or_else(|| chem.world.world().is_on(c));
                    assert_eq!(s.world.is_on(c), want, "preset {preset} step {k} at {c:?}");
                }
            }
            assert!(swaps > 0, "preset {preset}: no material changed");
        }
    }

    #[test]
    fn edit_frames_are_deltas_equal_to_full_frames() {
        let mut growths = 0;
        for (preset, radius) in [(2_u32, 14_u32), (0, 12), (4, 14)] {
            let mut runs: Vec<PlaySession> = [true, false]
                .into_iter()
                .map(|delta| {
                    let mut s = PlaySession::with_chem(preset, 7, radius).expect("chem genesis");
                    s.set_layout(320, 180, true);
                    s.set_delta_render(delta);
                    s.set_capture(true);
                    s
                })
                .collect();
            for s in &mut runs {
                assert!(s.step(1));
            }
            assert!(
                runs[0].renderer.delta_lists_built(),
                "the first (still) full frame builds the tile lists"
            );
            let (mut edit_frames, mut edit_deltas) = (0, 0);
            for k in 1..=40_u32 {
                let edit = k % 3 != 0;
                let deltas = runs[0].renderer.delta_frames();
                let renders = runs[0].renders();
                let tag = runs[0].last_world_tag;
                let domain = runs[0].chem.as_ref().unwrap().world.domain_len();
                for s in &mut runs {
                    if edit {
                        let button = if k % 5 == 1 { 2 } else { 0 };
                        s.pointer_button(button, true);
                        s.pointer_button(button, false);
                    }
                    if k == 26 {
                        s.look(31.0, -5.0);
                    }
                    s.step(if k % 4 == 0 { 4 } else { 1 });
                }
                let at = format!("preset {preset} r{radius} step {k}");
                assert!(runs[0].rgba() == runs[1].rgba(), "{at} output");
                assert_eq!(runs[0].status_with(true), runs[1].status_with(true), "{at}");
                if k == 26 {
                    assert!(
                        !runs[0].renderer.delta_lists_built(),
                        "{at}: a moved camera leaves the tile lists to the first delta frame"
                    );
                }
                let chem = runs[0].chem.as_ref().unwrap();
                growths += usize::from(chem.world.domain_len() != domain);
                if k != 26 && edit && runs[0].last_world_tag != tag && k != 27 {
                    edit_frames += 1;
                    assert!(runs[0].renders() > renders, "{at}: an edit repaints");
                    if runs[0].renderer.delta_frames() > deltas {
                        edit_deltas += 1;
                    }
                }
                assert_mirrors_chem(&runs[0]);
            }
            assert!(
                edit_frames >= 6,
                "preset {preset}: only {edit_frames} edit frames"
            );
            assert_eq!(
                edit_deltas, edit_frames,
                "preset {preset}: every edit frame is a delta"
            );
            assert_eq!(runs[1].renderer.delta_frames(), 0);
        }
        assert!(growths >= 2, "only {growths} places grew the chem domain");
    }

    /// Digs on ticks without a chem step, then chem steps that refill the dug cells (ocean
    /// water flows back): the play world keeps mirroring the chem occupancy after every step.
    #[test]
    fn digs_refilled_before_the_next_mirror_stay_mirrored() {
        let mut s = PlaySession::with_chem(CHEM_PRESET_OCEAN, 7, 14).expect("chem genesis");
        s.set_layout(320, 180, true);
        s.set_capture(true);
        assert!(s.step(1));
        let mut digs = 0;
        for k in 1..=24_u32 {
            s.pointer_button(0, true);
            s.pointer_button(0, false);
            // §6ah: one click per batch window (a second click inside one window aims at the
            // cell the first dug, as the play world shows it until the dig is published).
            s.step(if k % 3 == 0 { 8 } else { 4 });
            digs += u32::from(s.chem.as_ref().unwrap().last.starts_with("dig:"));
            assert_mirrors_chem(&s);
        }
        // §6ah: the crosshair cell freezes to ice after the first dig (heat-gated from then
        // on); every later click is answered with a refusal, and the mirror holds throughout.
        assert!(digs >= 1, "only {digs} digs");
        assert!(s.chem.as_ref().unwrap().last.starts_with("dig_"));
    }

    fn chem_startup(preset: u32) -> PlaySession {
        let mut s = PlaySession::with_chem(preset, 7, CHEM_RADIUS_DEFAULT).expect("chem genesis");
        s.set_layout(1600, 900, true);
        assert!(s.step(1), "startup step finalises a frame");
        s
    }

    /// §6ah: pauses auto-step and lands the batch in flight (the startup tick sent one), so
    /// the chem world holds still from here on.
    fn pause_chem(s: &mut PlaySession) {
        s.chem_set_auto(0);
        s.step(1);
        assert!(s.link.in_flight.is_none());
    }

    fn assert_mirrors_chem(s: &PlaySession) {
        let chem = s.chem.as_ref().unwrap();
        assert_eq!(
            world_tag(&s.world),
            world_tag(chem.world.world()),
            "play world == chem occupancy"
        );
        assert_eq!(s.history.current_world(), s.last_world_tag);
        assert_eq!(chem.world.check_books(), Ok(()));
        assert!(chem.world.lane_drift().iter().all(|&x| x == 0));
        assert_eq!(chem.world.energy_drift(), 0);
    }

    #[test]
    fn every_preset_genesis_is_visible_and_balanced() {
        for preset in 0..CHEM_PRESETS.len() as u32 {
            let s = chem_startup(preset);
            let chem = s.chem.as_ref().unwrap();
            assert!(chem.world.material_count() > 1000, "preset {preset}");
            assert_eq!(
                s.world.iter_on().count(),
                chem.world.material_count() + 1,
                "+ bedrock"
            );
            let nb = crate::count_non_background(s.renderer.pixels());
            let total = 1600 * 900;
            assert!(
                nb > total / 50 && nb < total * 3 / 4,
                "preset {preset}: non_background={nb}"
            );
            assert!(
                s.status()
                    .contains(&format!(" chem={} ", CHEM_PRESETS[preset as usize]))
            );
            assert_mirrors_chem(&s);
        }
        assert!(PlaySession::with_chem(3, 7, 12).is_none());
    }

    #[test]
    fn step_paced_runs_at_most_one_chem_step_and_the_same_ticks() {
        // Gas at its default cadence (one chem step every 4 ticks): paced steps of up to 9
        // ticks run one chem step each and drop the rest; the ticks that do run leave world,
        // history, books and game pixels exactly where as many `fixed_tick`s would.
        // The audited session renders every frame in full, so each paced frame (a delta drawn
        // after skipped frames and dropped ticks) is checked against a full render of the
        // same world: the paced view converges to it on every frame it presents.
        let mut paced = PlaySession::with_chem(CHEM_PRESET_GAS, 7, CHEM_RADIUS_DEFAULT).unwrap();
        let mut audited = PlaySession::with_chem(CHEM_PRESET_GAS, 7, CHEM_RADIUS_DEFAULT).unwrap();
        audited.set_delta_render(false);
        for s in [&mut paced, &mut audited] {
            s.set_layout(320, 180, true);
        }
        let mut dropped = 0;
        for (k, ticks) in [1_u32, 9, 4, 5, 8, 2, 9, 6, 9, 3].into_iter().enumerate() {
            let before = paced.chem.as_ref().unwrap().steps_run;
            let (painted, ran) = paced.step_paced(ticks);
            assert!(ran >= 1 && ran <= ticks, "k={k}");
            dropped += ticks - ran;
            let steps = paced.chem.as_ref().unwrap().steps_run - before;
            assert!(steps <= 1, "k={k}: {steps} chem steps in one paced step");
            for _ in 0..ran {
                audited.fixed_tick();
            }
            assert_eq!(paced.tick, audited.tick, "k={k}");
            assert_eq!(paced.last_world_tag, audited.last_world_tag, "k={k}");
            assert_eq!(paced.chem_status(), audited.chem_status(), "k={k}");
            if painted {
                assert_eq!(paced.renderer.pixels(), audited.renderer.pixels(), "k={k}");
            }
            assert_mirrors_chem(&paced);
        }
        assert!(dropped > 0, "the 9-tick steps must drop a second chem step");
        assert!(paced.renderer.delta_frames() > 0, "paced frames are deltas");
        // Without chem a paced step is a plain step.
        let mut plain = PlaySession::new();
        let mut live = PlaySession::new();
        assert_eq!(plain.step_paced(7), (live.step(7), 7));
        assert_eq!(plain.status(), live.status());
    }

    #[test]
    fn chem_auto_steps_advance_tick_and_keep_books() {
        let mut s = chem_startup(CHEM_PRESET_ASH);
        let t0 = s.chem.as_ref().unwrap().world.tick();
        s.step(4 * 60); // 60 chem steps at the default cadence
        let chem = s.chem.as_ref().unwrap();
        assert!(
            chem.world.tick() >= t0 + 59,
            "chem tick advanced: {}",
            chem.world.tick()
        );
        assert!(
            chem.world.evaporated() > 0 || chem.world.vapor() != 0,
            "water cycle moved"
        );
        assert!(chem.error.is_none());
        assert_mirrors_chem(&s);
        // Pause + manual step. §6ah: the step in flight still lands (on the next tick), then
        // the world holds; queued steps are sent on one tick and published on the next.
        s.chem_set_auto(0);
        let t1 = s.chem.as_ref().unwrap().world.tick();
        s.step(10);
        assert_eq!(s.chem.as_ref().unwrap().world.tick(), t1 + 1);
        s.chem_queue(3);
        s.step(2);
        assert_eq!(s.chem.as_ref().unwrap().world.tick(), t1 + 4);
        assert_eq!(s.chem_published_tick(), Some(t1 + 4));
        assert_mirrors_chem(&s);
    }

    #[test]
    fn dig_and_place_go_through_chem() {
        let mut s = chem_startup(CHEM_PRESET_ROCKY);
        pause_chem(&mut s);
        let n0 = s.world.iter_on().count();
        let bag0 = s.chem.as_ref().unwrap().world.inventory();
        s.set_capture(true);
        s.pointer_button(0, true); // LMB = aimed dig
        // §6ah: queued and sent on the next tick, published (and mirrored) the tick after.
        s.step(1);
        assert_eq!(s.world.iter_on().count(), n0, "not published yet");
        s.step(1);
        let chem = s.chem.as_ref().unwrap();
        assert!(chem.last.starts_with("dig:"), "last={}", chem.last);
        assert_eq!(s.world.iter_on().count(), n0 - 1);
        assert_ne!(chem.world.inventory(), bag0, "dug atoms went to the bag");
        assert!(s.status().contains(" chem_last=dig:"));
        assert_mirrors_chem(&s);
        s.pointer_button(2, true); // RMB = aimed place, paid from the bag
        s.step(2);
        let chem = s.chem.as_ref().unwrap();
        assert!(chem.last.starts_with("place:"), "last={}", chem.last);
        assert_eq!(s.world.iter_on().count(), n0);
        assert_eq!(
            chem.world.inventory(),
            bag0,
            "place paid back what dig returned"
        );
        assert_mirrors_chem(&s);
    }

    fn shade_pixels(s: &PlaySession, rgb: u32) -> usize {
        let shades = bitblock_core::render::tinted_shades(rgb);
        let lit = bitblock_core::render::lit_face_color(rgb);
        s.renderer
            .pixels()
            .iter()
            .filter(|p| shades.contains(p) || **p == lit)
            .count()
    }

    /// Cut faces are lit: against the same cut rendered without lit faces (the pre-highlight
    /// look), every changed pixel is the lit colour of a chem material, the core and several
    /// bands show lit pixels with distinct hues, and turning the cut off restores the uncut frame.
    #[test]
    fn cross_section_lights_cut_faces_and_keeps_bands_distinct() {
        use bitblock_core::render::lit_face_color;
        for preset in [CHEM_PRESET_ROCKY, CHEM_PRESET_OCEAN] {
            let mut s = chem_startup(preset);
            pause_chem(&mut s);
            let off_pixels = s.renderer.pixels().to_vec();
            let lit_of: std::collections::BTreeMap<u32, u32> = {
                let c = s.chem.as_ref().unwrap();
                c.colors
                    .by_id
                    .iter()
                    .filter(|&&b| b != 0)
                    .map(|&b| (lit_face_color(b), b))
                    .collect()
            };
            let core_lit = {
                let c = s.chem.as_ref().unwrap();
                lit_face_color(c.colors.by_id[usize::from(c.world.material(c.center()).0)])
            };
            for mode in [CHEM_CUT_WEDGE, CHEM_CUT_X, CHEM_CUT_Y, CHEM_CUT_Z] {
                for offset in [0, -3] {
                    assert!(s.chem_set_cut(mode, offset));
                    assert!(s.step(1));
                    assert!(
                        !s.cut_faces.is_empty(),
                        "preset {preset} mode {mode}: cut faces"
                    );
                    assert!(
                        s.cut_faces.windows(2).all(|w| w[0].0 < w[1].0),
                        "cut faces sorted"
                    );
                    s.audit_game_palette();
                    let lit = s.renderer.pixels().to_vec();
                    // Same cut, lit faces suppressed: the pre-highlight frame.
                    let faces = std::mem::take(&mut s.cut_faces);
                    s.render_game();
                    let plain = s.renderer.pixels().to_vec();
                    s.cut_faces = faces;
                    let (mut changed, mut hues) = (0usize, std::collections::BTreeSet::new());
                    for (p, q) in lit.iter().zip(&plain) {
                        if p != q {
                            // Crosshair pixels are the inverse of what lies under them.
                            let base = lit_of
                                .get(p)
                                .or_else(|| lit_of.get(&(!p & 0x00ff_ffff)))
                                .unwrap_or_else(|| {
                                    panic!(
                                        "preset {preset} mode {mode}: changed pixel {p:06x} not lit"
                                    )
                                });
                            changed += 1;
                            hues.insert(*base);
                        }
                    }
                    let core = lit.iter().filter(|&&p| p == core_lit).count();
                    assert!(
                        changed > 5000,
                        "preset {preset} mode {mode}:{offset} lit pixels {changed}"
                    );
                    assert!(
                        hues.len() >= 3,
                        "preset {preset} mode {mode}:{offset} lit hues {hues:x?}"
                    );
                    if offset == 0 {
                        assert!(
                            core > 300,
                            "preset {preset} mode {mode}: core lit pixels {core}"
                        );
                    }
                    eprintln!(
                        "preset {preset} cut {mode}:{offset}: lit={changed} hues={} core={core}",
                        hues.len()
                    );
                }
            }
            assert!(s.chem_set_cut(CHEM_CUT_OFF, 0));
            assert!(s.step(1));
            assert!(s.cut_faces.is_empty());
            assert_eq!(
                s.renderer.pixels(),
                &off_pixels[..],
                "preset {preset}: cut off == uncut"
            );
        }
    }

    #[test]
    fn cross_section_reveals_core_bands_on_rocky_and_ocean() {
        for preset in [CHEM_PRESET_ROCKY, CHEM_PRESET_OCEAN] {
            let mut s = chem_startup(preset);
            pause_chem(&mut s);
            let (center, core_rgb) = {
                let c = s.chem.as_ref().unwrap();
                let m = c.world.material(c.center());
                assert_ne!(m, MatId::AIR, "preset {preset}: solid core");
                (c.center(), c.colors.by_id[usize::from(m.0)])
            };
            let off_pixels = s.renderer.pixels().to_vec();
            let off_core = shade_pixels(&s, core_rgb);
            let tag0 = world_tag(&s.world);
            for mode in [CHEM_CUT_WEDGE, CHEM_CUT_X, CHEM_CUT_Y, CHEM_CUT_Z] {
                assert!(s.chem_set_cut(mode, 0));
                assert!(s.step(1), "cut repaints");
                let core = shade_pixels(&s, core_rgb);
                assert!(
                    core > off_core + 500,
                    "preset {preset} mode {mode}: core pixels {core} vs uncut {off_core}"
                );
                assert!(
                    s.status()
                        .ends_with(&format!(" cut={}:0", CHEM_CUT_NAMES[mode as usize]))
                        || s.status()
                            .contains(&format!(" cut={}:0", CHEM_CUT_NAMES[mode as usize]))
                );
                assert!(
                    s.chem_status()
                        .unwrap()
                        .ends_with(&format!(" cut={}:0", CHEM_CUT_NAMES[mode as usize]))
                );
                // Render-only: world, history and chem untouched.
                assert_eq!(world_tag(&s.world), tag0);
                assert!(s.world.is_on(center));
                assert_mirrors_chem(&s);
                s.audit_game_palette();
            }
            // Off again: pixel-identical to the never-cut frame, status back to the old text.
            assert!(s.chem_set_cut(CHEM_CUT_OFF, 0));
            assert!(s.step(1));
            assert_eq!(
                s.renderer.pixels(),
                &off_pixels[..],
                "preset {preset}: cut off == before"
            );
            assert!(!s.status().contains(" cut="));
            assert!(!s.chem_status().unwrap().contains(" cut="));
            // Unknown mode refused; offset clamps to the radius.
            assert!(!s.chem_set_cut(9, 0));
            assert!(s.chem_set_cut(CHEM_CUT_X, 1000));
            assert_eq!(s.chem_cut().offset, CHEM_RADIUS_DEFAULT as i32);
        }
    }

    #[test]
    fn cross_section_dig_and_place_keep_books_and_chem_ticks() {
        let mut s = chem_startup(CHEM_PRESET_ROCKY);
        pause_chem(&mut s);
        assert!(s.chem_set_cut(CHEM_CUT_WEDGE, 0));
        assert!(s.step(1));
        let n0 = s.world.iter_on().count();
        let bag0 = s.chem.as_ref().unwrap().world.inventory();
        s.set_capture(true);
        s.pointer_button(0, true); // dig the cut face under the crosshair
        s.step(2); // §6ah: sent on the next tick, published the tick after
        let (dug, last) = {
            let c = s.chem.as_ref().unwrap();
            (c.last.clone(), c.last.starts_with("dig:"))
        };
        assert!(last, "last={dug}");
        assert_eq!(s.world.iter_on().count(), n0 - 1);
        assert_mirrors_chem(&s);
        // The dug cell is a visible (not hidden) cell on the cut face.
        s.pointer_button(2, true);
        s.step(2);
        let c = s.chem.as_ref().unwrap();
        assert!(
            c.last.starts_with("place:") || c.last == "place_refused:cut",
            "last={}",
            c.last
        );
        assert_mirrors_chem(&s);
        let _ = bag0;
        // Chem tick still runs with the cut on; the view follows the world.
        let t1 = s.chem.as_ref().unwrap().world.tick();
        s.chem_queue(5);
        s.step(2);
        assert_eq!(s.chem.as_ref().unwrap().world.tick(), t1 + 5);
        assert_mirrors_chem(&s);
        s.chem_set_auto(4);
        s.step(40);
        assert!(s.chem.as_ref().unwrap().world.tick() > t1 + 5);
        assert_mirrors_chem(&s);
        // Cut view == play world minus exactly the hidden domain cells.
        s.step(1);
        let view = s
            .cut_view
            .as_ref()
            .map(|(_, w)| w.clone())
            .expect("view built");
        let c = s.chem.as_ref().unwrap();
        let signs = c.cut_signs(s.camera.position());
        for coord in s.world.iter_on() {
            assert_eq!(view.is_on(coord), !c.cut_hides(coord, signs), "{coord:?}");
        }
        assert!(view.iter_on().count() < s.world.iter_on().count());
    }

    #[test]
    fn cut_export_is_inert_outside_chem() {
        let mut s = PlaySession::new();
        assert!(!s.chem_set_cut(CHEM_CUT_WEDGE, 0));
        assert!(!s.chem_cut().active());
        assert!(s.cut_view.is_none());
    }

    /// §6ae: the mirror names changed cells from the domain's z-runs; they give every domain
    /// cell's coordinate, before and after a place grows the domain, and the cached runs follow
    /// the domain.
    #[test]
    fn mirror_rows_name_every_domain_cell() {
        fn check(s: &PlaySession) {
            let chem = s.chem.as_ref().unwrap();
            let cells = chem.world.cells();
            let rows = domain_rows(cells);
            let mut r = 0;
            for (i, &c) in cells.iter().enumerate() {
                assert_eq!(row_cell(&rows, &mut r, i), c, "cell {i}");
            }
            assert_eq!(chem.mirror_rows.0, cells.len());
            assert_eq!(chem.mirror_rows.1, rows);
        }
        let mut s = chem_startup(CHEM_PRESET_ASH);
        s.step(8);
        check(&s);
        let before = s.chem.as_ref().unwrap().world.domain_len();
        let far = bitblock_core::world::Coord::new(0, 200, 0);
        s.inputs
            .edits
            .push(PendingEdit::Exact(Edit::new(Operation::On, far)));
        s.step(8);
        assert_eq!(s.chem.as_ref().unwrap().world.domain_len(), before + 1);
        check(&s);
        assert_mirrors_chem(&s);
    }

    #[test]
    fn heat_gate_refuses_cold_dig() {
        let mut s = chem_startup(CHEM_PRESET_ASH);
        let c = s.chem.as_mut().unwrap();
        // An isolated placed cell starts at Q = BIND and radiates to space: it goes cold.
        let far = bitblock_core::world::Coord::new(0, 200, 0);
        assert!(c.world.place(far, MatId::ROCK));
        c.run(30);
        let bind = c.world.table().bind(MatId::ROCK);
        let q = c.world.heat(far).unwrap();
        assert!(q < bind, "q={q} bind={bind}");
        assert!(!c.dig(far), "cold rock: bonds cannot break");
        assert!(c.last.starts_with("dig_refused_heat:"), "last={}", c.last);
        assert_ne!(
            c.world.material(far),
            MatId::AIR,
            "refused dig left the cell"
        );
    }

    #[test]
    fn custom_budget_matches_preset_and_spends_entered_lanes() {
        // The preset's own lanes entered as a custom budget build the very same world.
        for preset in 0..CHEM_PRESETS.len() as u32 {
            let lanes = preset_budget(preset, sphere_cells(CHEM_RADIUS_DEFAULT))
                .unwrap()
                .lanes;
            let p = PlaySession::with_chem(preset, 7, CHEM_RADIUS_DEFAULT).unwrap();
            let c = PlaySession::with_chem_budget(lanes, 7, CHEM_RADIUS_DEFAULT).unwrap();
            assert_eq!(world_tag(&p.world), world_tag(&c.world), "preset {preset}");
            assert_eq!(c.chem.as_ref().unwrap().preset_name(), "custom");
            assert_eq!(c.last_action, "world:chem-custom");
        }
        // A tweaked budget: totals equal exactly what was entered, books balance while ticking.
        let lanes = [
            9000, 6000, 800, 300, 700, 900, 1500, 200, 1200, 900, 150, 80,
        ];
        let mut s = PlaySession::with_chem_budget(lanes, 11, 10).unwrap();
        s.set_layout(1600, 900, true);
        assert!(s.step(1));
        let chem = s.chem.as_ref().unwrap();
        assert_eq!(chem.world.lane_totals(), lanes);
        assert_eq!(chem.world.t0(), lanes);
        assert!(chem.world.material_count() > 500);
        let text = s.chem_status().unwrap();
        assert!(
            text.starts_with("bitblock_play_chem ok preset=custom seed=11 radius=10 "),
            "{text}"
        );
        assert!(text.contains(" lanes=H:9000,O:6000,C:800,N:300,S:700,Ca:900,Fe:1500,Ni:200,Si:1200,Mg:900,Na:150,K:80 "));
        assert!(text.ends_with(" budget=H:9000,O:6000,C:800,N:300,S:700,Ca:900,Fe:1500,Ni:200,Si:1200,Mg:900,Na:150,K:80 atoms=21730"), "{text}");
        assert!(s.status().contains(" chem=custom seed=11 radius=10 "));
        s.step(4 * 30);
        assert!(s.chem.as_ref().unwrap().world.tick() >= 30);
        assert_mirrors_chem(&s);
        assert_eq!(s.chem.as_ref().unwrap().world.lane_totals(), lanes);
        // Empty budget: a legal (if barren) world; the tick still runs.
        let mut z = PlaySession::with_chem_budget([0; 12], 1, 4).unwrap();
        assert_eq!(z.chem.as_ref().unwrap().world.material_count(), 0);
        z.step(8);
        assert_mirrors_chem(&z);
    }

    #[test]
    fn custom_budget_rejects_negative_and_oversized_lanes() {
        let mut lanes = [100; 12];
        lanes[4] = -1;
        let e = PlaySession::with_chem_budget(lanes, 7, 8).err().unwrap();
        assert_eq!(e, "budget_lane_4_(S)_is_negative:_-1");
        let mut big = [0; 12];
        big[0] = CHEM_ATOM_CAP + 1;
        let e = PlaySession::with_chem_budget(big, 7, 8).err().unwrap();
        assert!(
            e.starts_with("budget_of_4294967297_atoms_exceeds_cap_4294967296"),
            "{e}"
        );
        let e =
            PlaySession::with_chem_budget([i64::MAX, i64::MAX, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 7, 8)
                .err()
                .unwrap();
        assert_eq!(e, "budget_arithmetic_overflowed");
        // Exactly at the cap is accepted (the surplus waits in the bag).
        big[0] = CHEM_ATOM_CAP;
        let s = PlaySession::with_chem_budget(big, 7, 4).unwrap();
        assert_mirrors_chem(&s);
    }

    #[test]
    fn chem_status_reports_lanes_drift_and_bag() {
        let s = chem_startup(CHEM_PRESET_OCEAN);
        let text = s.chem_status().unwrap();
        // §6ah: the startup tick sent the first step; it is published four ticks later.
        assert!(text.starts_with("bitblock_play_chem ok preset=ocean seed=7 radius=12 tick=0 "));
        assert!(text.contains(" books=ok "));
        assert!(text.contains(" drift=H:0,O:0,C:0,N:0,S:0,Ca:0,Fe:0,Ni:0,Si:0,Mg:0,Na:0,K:0 "));
        assert!(PlaySession::new().chem_status().is_none());
    }

    /// Distinct non-grey RGBs in the game frame (greys = the ordinary occupancy palette).
    fn tinted_colors(s: &PlaySession) -> std::collections::BTreeSet<u32> {
        s.renderer
            .pixels()
            .iter()
            .map(|p| p & 0x00ff_ffff)
            .filter(|&p| !(p >> 16 == (p >> 8) & 0xff && (p >> 8) & 0xff == p & 0xff))
            .collect()
    }

    #[test]
    fn chem_presets_render_distinct_material_colours() {
        for preset in 0..CHEM_PRESETS.len() as u32 {
            let s = chem_startup(preset);
            // Base material colours visible as (top-face) pixels.
            let visible: std::collections::BTreeSet<u32> = CHEM_PALETTE
                .iter()
                .map(|&(_, rgb)| rgb)
                .filter(|rgb| s.renderer.pixels().iter().any(|p| p & 0x00ff_ffff == *rgb))
                .collect();
            let hues = tinted_colors(&s);
            assert!(
                hues.len() >= 8,
                "preset {preset}: tinted colours {}",
                hues.len()
            );
            // Genesis sizes concentric shells from the budget, so from outside only the
            // outermost layers show (ash: ocean under a methane skin); inner layers are
            // under the crust.
            assert!(
                visible.len() >= 2,
                "preset {preset}: visible base colours {visible:x?}"
            );
            s.audit_game_palette();
            eprintln!(
                "chem preset {preset}: tinted_colours={} visible_bases={} {visible:x?}",
                hues.len(),
                visible.len()
            );
        }
    }

    #[test]
    fn material_only_change_repaints_chem_frame() {
        let mut s = chem_startup(CHEM_PRESET_ROCKY);
        s.chem_set_auto(0);
        s.step(2);
        let mut repaint_without_occupancy = 0;
        for _ in 0..40 {
            let renders = s.renders();
            let before = s.chem.as_ref().unwrap().material_changes;
            s.chem_queue(1);
            s.step(1);
            let chem = s.chem.as_ref().unwrap();
            if chem.material_changes != before {
                assert!(s.renders() > renders, "material change must repaint");
                if chem.last_step_changed == 0 {
                    repaint_without_occupancy += 1;
                }
            }
        }
        assert!(
            repaint_without_occupancy > 0,
            "rocky recolours in place (rock -> sediment)"
        );
        assert_mirrors_chem(&s);
    }

    /// §6ah worker path in-process: the batch bytes go to a second chem world (the engine a
    /// worker would own) and its result bytes come back.
    fn shuttle(s: &mut PlaySession, host: &mut ChemPlay) -> bool {
        let Some(bytes) = s.chem_take_outbox() else {
            return false;
        };
        let batch = crate::chem_link::decode_batch(&bytes).expect("batch decodes");
        let (report, refused) = host.process(&batch);
        let delta = crate::chem_link::encode_delta(host, &report, refused);
        assert!(s.chem_put_inbox(delta));
        // The worker precomputes the next report's books while idle (on alternate batches
        // here): the reports must not depend on it.
        if s.link.sent.is_multiple_of(2) {
            host.precompute_books();
        }
        true
    }

    /// The worker path's replica shows what the fallback's engine (the published state)
    /// holds: cells, materials, occupancy; and the worker's engine keeps its books closed.
    fn assert_replica(remote: &PlaySession, local: &PlaySession, host: &ChemPlay) {
        let shown = remote.chem.as_ref().unwrap();
        let engine = local.chem.as_ref().unwrap();
        assert_eq!(shown.world.cells(), engine.world.cells());
        assert_eq!(shown.world.mats(), engine.world.mats());
        assert_eq!(
            world_tag(shown.world.world()),
            world_tag(engine.world.world())
        );
        assert_eq!(world_tag(&remote.world), world_tag(shown.world.world()));
        assert_eq!(host.world.check_books(), Ok(()));
        assert_eq!(host.world.energy_drift(), 0);
    }

    /// The scripted inputs of tick `k` (looks, wheel, dig / place clicks, dynamics, pause /
    /// manual steps / resume, a cross-section on and off, exact edits).
    fn script(s: &mut PlaySession, k: u32) {
        if k % 7 == 3 {
            s.pointer_button(0, true);
            s.pointer_button(0, false);
        }
        if k % 11 == 5 {
            s.pointer_button(2, true);
            s.pointer_button(2, false);
        }
        if k.is_multiple_of(13) {
            s.look(17.0, -3.0);
        }
        if k == 20 {
            s.wheel(2.0);
        }
        if k == 30 {
            assert!(s.chem_set_dynamics(1, 1));
        }
        if k == 41 {
            s.chem_set_auto(0);
        }
        if k == 47 {
            s.chem_queue(3);
        }
        if k == 52 {
            s.chem_set_auto(3);
        }
        if k == 60 {
            assert!(s.chem_set_cut(CHEM_CUT_X, 0));
        }
        if k == 70 {
            assert!(s.chem_set_cut(CHEM_CUT_OFF, 0));
        }
        if k % 17 == 9 {
            let c = s.chem.as_ref().unwrap();
            let cell = c.world.cells()[c.world.domain_len() / 2 + k as usize];
            for op in [Operation::Toggle, Operation::Off, Operation::On] {
                s.inputs.edits.push(PendingEdit::Exact(Edit::new(op, cell)));
            }
        }
    }

    /// §6ah: the worker path (results through bytes from a second chem world, in-process
    /// here) and the single-thread fallback publish the same chem states on the same ticks:
    /// play world, history, frames (output buffer), status and chem readouts are equal after
    /// every tick, with edits, dynamics, pause / manual steps, a cut and camera moves; and
    /// the replica equals the engine's cells, materials and occupancy.
    #[test]
    fn lagged_worker_path_equals_fallback() {
        let mut refused = 0;
        for (preset, radius) in [(2_u32, 10_u32), (4, 10), (1, 8)] {
            let mk = || {
                let mut s = PlaySession::with_chem(preset, 7, radius).unwrap();
                s.set_layout(320, 180, true);
                s.set_capture(true);
                s
            };
            let (mut local, mut remote) = (mk(), mk());
            assert!(remote.chem_set_remote(true));
            let mut host = ChemPlay::new(preset, 7, radius).unwrap();
            for k in 1..=90_u32 {
                script(&mut local, k);
                script(&mut remote, k);
                let a = local.step(1);
                assert!(!remote.chem_blocked(), "k={k}");
                let b = remote.step(1);
                assert_eq!(a, b, "preset {preset} k={k} painted");
                shuttle(&mut remote, &mut host);
                let at = format!("preset {preset} k={k}");
                assert_eq!(local.status_with(true), remote.status_with(true), "{at}");
                assert_eq!(local.chem_status(), remote.chem_status(), "{at}");
                assert!(local.rgba() == remote.rgba(), "{at} rgba");
                assert_mirrors_chem(&local);
                assert_replica(&remote, &local, &host);
                refused += u32::from(local.link.report.last.contains("refused"));
            }
            assert!(
                local.link.landed > 20,
                "preset {preset}: {} landings",
                local.link.landed
            );
            assert_eq!(local.link.landed, remote.link.landed);
        }
        assert!(refused > 0, "the script exercises refusals");
    }

    /// §6ah: a late worker result holds the session at the landing tick (ticks before it
    /// run; the landing waits) and changes nothing once delivered: the session then equals
    /// one that got every result at once, tick for tick.
    #[test]
    fn worker_failure_replays_only_published_batches_without_losing_edits() {
        for failure_tick in [0, 1, 4, 11, 36, 69] {
            let mk = || {
                let mut s = PlaySession::with_chem(2, 7, 10).unwrap();
                s.set_layout(320, 180, true);
                s.set_capture(true);
                s
            };
            let (mut local, mut remote) = (mk(), mk());
            assert!(remote.chem_set_remote(true));
            let mut host = ChemPlay::new(2, 7, 10).unwrap();
            for k in 0..90 {
                script(&mut local, k);
                script(&mut remote, k);
                if k == failure_tick {
                    assert!(remote.chem_recover_local());
                    assert!(!remote.chem_recover_local());
                }
                local.step(1);
                remote.step(1);
                shuttle(&mut remote, &mut host);
                assert_eq!(local.status(), remote.status(), "fail={failure_tick} k={k}");
                assert_eq!(local.chem_status(), remote.chem_status());
                assert_eq!(local.rgba(), remote.rgba());
            }
            assert_eq!(local.link.landed, remote.link.landed);
            assert_mirrors_chem(&remote);
            assert_eq!(remote.chem.as_ref().unwrap().world.check_books(), Ok(()));
            assert_eq!(remote.chem.as_ref().unwrap().world.energy_drift(), 0);
        }
    }

    #[test]
    fn late_worker_results_stall_at_the_landing_tick_only() {
        let mk = || {
            let mut s = PlaySession::with_chem(CHEM_PRESET_GAS, 7, 10).unwrap();
            s.set_layout(320, 180, true);
            assert!(s.chem_set_remote(true));
            s
        };
        let (mut prompt, mut late) = (mk(), mk());
        let mut hp = ChemPlay::new(CHEM_PRESET_GAS, 7, 10).unwrap();
        let mut hl = ChemPlay::new(CHEM_PRESET_GAS, 7, 10).unwrap();
        let mut held: Option<Vec<u8>> = None;
        for k in 0..60_u32 {
            prompt.step(1);
            shuttle(&mut prompt, &mut hp);
            // the late session runs paced steps of 3 ticks and gets each result 2 calls late
            let (_, sent, ran) = late.step_lagged(3);
            assert!(ran <= 3);
            if sent {
                let bytes = late.chem_take_outbox().unwrap();
                assert!(held.is_none());
                held = Some(bytes);
            }
            if k % 3 == 2
                && let Some(bytes) = held.take()
            {
                let batch = crate::chem_link::decode_batch(&bytes).unwrap();
                let (r, n) = hl.process(&batch);
                assert!(late.chem_put_inbox(crate::chem_link::encode_delta(&hl, &r, n)));
            }
            if late.chem_blocked() {
                assert!(late.link.inbox.is_none());
            }
        }
        // catch the prompt one up to the late one's tick (or the reverse) and compare
        while prompt.tick < late.tick {
            prompt.step(1);
            shuttle(&mut prompt, &mut hp);
        }
        while late.tick < prompt.tick {
            if let Some(bytes) = held.take() {
                let batch = crate::chem_link::decode_batch(&bytes).unwrap();
                let (r, n) = hl.process(&batch);
                assert!(late.chem_put_inbox(crate::chem_link::encode_delta(&hl, &r, n)));
            }
            late.step(1);
            if let Some(b) = late.chem_take_outbox() {
                held = Some(b);
            }
        }
        assert!(late.tick > 40);
        assert_eq!(prompt.last_world_tag, late.last_world_tag);
        assert_eq!(prompt.chem_status(), late.chem_status());
        assert_eq!(hp.world.mats(), hl.world.mats());
    }

    /// §6ah: two fresh fallback sessions under the same script are identical tick for tick.
    #[test]
    fn lagged_schedule_is_reproducible() {
        let mk = || {
            let mut s = PlaySession::with_chem(CHEM_PRESET_OCEAN, 7, 10).unwrap();
            s.set_layout(320, 180, true);
            s.set_capture(true);
            s
        };
        let (mut a, mut b) = (mk(), mk());
        for k in 1..=60 {
            script(&mut a, k);
            script(&mut b, k);
            a.step(1);
            b.step(1);
            assert_eq!(a.status_with(true), b.status_with(true), "k={k}");
            assert_eq!(a.chem_status(), b.chem_status(), "k={k}");
        }
    }

    /// §6ah: every queued edit is applied or refused exactly once, by the engine, against the
    /// chem state at application: a second dig of a cell dug earlier in the same batch is
    /// refused, a place pays from the bag once, and books stay closed.
    #[test]
    fn queued_edits_apply_or_refuse_once_in_order() {
        let mut s = chem_startup(CHEM_PRESET_ROCKY);
        pause_chem(&mut s);
        let (cell, bag0, n0) = {
            let c = s.chem.as_ref().unwrap();
            let cell = c
                .world
                .cells()
                .iter()
                .copied()
                .find(|&x| {
                    let mut probe = c.world.clone();
                    probe.dig(x).is_some()
                })
                .expect("a diggable cell");
            (cell, c.world.inventory(), s.world.iter_on().count())
        };
        for op in [Operation::Off, Operation::Off, Operation::On, Operation::On] {
            s.inputs.edits.push(PendingEdit::Exact(Edit::new(op, cell)));
        }
        s.step(1);
        assert_eq!(s.world.iter_on().count(), n0, "queued, not yet published");
        assert_eq!(s.link.in_flight.as_ref().map(|b| b.ops.len()), Some(4));
        s.step(1);
        let c = s.chem.as_ref().unwrap();
        // off (dig) ok; off again: the bit is already off, nothing to do; on (place) ok, paid
        // from the bag; on again: the bit is on, nothing to do. One cell, net unchanged.
        assert_eq!(s.world.iter_on().count(), n0);
        assert!(c.world.material(cell) != MatId::AIR);
        assert!(c.last.starts_with("place:"), "last={}", c.last);
        assert_eq!(c.world.inventory(), bag0, "the dig paid the place");
        assert_mirrors_chem(&s);
        // Two aimed digs in one window target the same (still shown) cell: one applies.
        s.set_capture(true);
        s.pointer_button(0, true);
        s.pointer_button(0, false);
        s.pointer_button(0, true);
        s.pointer_button(0, false);
        s.step(1);
        let ops = s.link.in_flight.as_ref().unwrap().ops.clone();
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0], ops[1]);
        s.step(1);
        let c = s.chem.as_ref().unwrap();
        assert!(c.last.starts_with("dig_refused"), "second dig: {}", c.last);
        assert_eq!(s.world.iter_on().count(), n0 - 1);
        assert_mirrors_chem(&s);
    }

    #[test]
    fn non_chem_sessions_are_untouched() {
        let mut s = PlaySession::with_start_world(PLAY_WORLD_DEFAULT).unwrap();
        assert!(!s.chem_set_auto(1) && !s.chem_queue(1));
        assert!(!s.status().contains(" chem="));
    }
}
