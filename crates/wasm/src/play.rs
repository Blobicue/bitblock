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
    Camera, Vec3, DEFAULT_DOLLY_LOG_STEP, DEFAULT_MOUSE_SENSITIVITY, MAX_OBSERVATION_DISTANCE,
};
use bitblock_core::frame::FrameRef;
use bitblock_core::history::{world_tag, History, TickTransaction};
use bitblock_core::interaction::{AcceptedAction, InteractionTape, MoveControl};
use bitblock_core::program::{Edit, Operation, Program};
use bitblock_core::protocol::ContentTag;
use bitblock_core::render::{
    bounds_aabb, coord_containing, crosshair_overlay_input, raycast, sweep_point, Renderer,
};
use bitblock_core::sidebar::{DisplayStatus, FrameCompositor, SidebarRasterFacts, SidebarSnapshot};
use bitblock_core::tick::{
    TickActionError, TickActionLedger, LANE_AIM_DELETE, LANE_AIM_PLACE, LANE_CLEAR_MOTION,
    LANE_EXACT_EDIT, LANE_FIT_WORLD, LANE_LOOK_DELTA, LANE_MOVE_EDGE, LANE_RESIZE,
    LANE_TOGGLE_SIDEBAR, LANE_WHEEL_LINES,
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
    NonFiniteLane { field: &'static str, bits: u64 },
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
}

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
        session.last_world_tag = world_tag(&world);
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
        session.last_world_tag = world_tag(&world);
        session.camera =
            crate::gallery_camera(&world, crate::GALLERY_YAW_DEG, crate::GALLERY_PITCH_DEG);
        session.world = world;
        session.last_action = format!("world:chem-{}", chem.preset_name());
        session.chem = Some(chem);
        session
    }

    pub fn chem_active(&self) -> bool {
        self.chem.is_some()
    }

    /// Detailed chem readout (lanes, drift, bag, vapor, tick), or `None` outside chem mode.
    pub fn chem_status(&self) -> Option<String> {
        self.chem.as_ref().map(|c| c.status())
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
    pub fn chem_set_dynamics(&mut self, energy: u32, cycle: u32) -> bool {
        self.chem.as_mut().is_some_and(|c| c.set_dynamics(energy, cycle))
    }

    /// `(energy, cycle)` modes of the chem world, or `None` outside chem mode.
    pub fn chem_dynamics(&self) -> Option<(u32, u32)> {
        self.chem.as_ref().map(|c| (c.energy, c.cycle))
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
        let key = (self.last_world_tag, chem.cut, chem.cut_signs(self.camera.position()));
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
        let last_world_tag = world_tag(&world);
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
        }
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
        FrameRef::new(self.output_width(), self.output_height(), self.output_pixels())
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
        let chem = self.chem.as_ref().map(|c| c.status_fields()).unwrap_or_default();
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
                let _ = self.apply_accepted_action(AcceptedAction::MoveEdge(
                    MoveControl::Forward,
                    pressed,
                ));
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
                let _ = self.apply_accepted_action(AcceptedAction::MoveEdge(
                    MoveControl::Left,
                    pressed,
                ));
                self.last_action = if pressed { "key:A:down" } else { "key:A:up" }.to_owned();
            }
            PLAY_KEY_D => {
                let _ = self.apply_accepted_action(AcceptedAction::MoveEdge(
                    MoveControl::Right,
                    pressed,
                ));
                self.last_action = if pressed { "key:D:down" } else { "key:D:up" }.to_owned();
            }
            PLAY_KEY_SPACE => {
                let _ = self.apply_accepted_action(AcceptedAction::MoveEdge(
                    MoveControl::Up,
                    pressed,
                ));
                self.last_action = if pressed {
                    "key:Space:down"
                } else {
                    "key:Space:up"
                }
                .to_owned();
            }
            PLAY_KEY_SHIFT => {
                let _ = self.apply_accepted_action(AcceptedAction::MoveEdge(
                    MoveControl::Down,
                    pressed,
                ));
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
        let source_tick = self.tick;
        self.sim_tick();
        self.render_pending(source_tick, RenderPolicy::Audited)
    }

    /// Browser live step: run `ticks` fixed simulation ticks, then finalise at most ONE frame
    /// for the last of them (live policy). World / camera / history evolve exactly as with
    /// `ticks` calls to `fixed_tick`; only intermediate frames nobody would see are skipped.
    pub fn step(&mut self, ticks: u32) -> bool {
        for _ in 0..ticks {
            self.sim_tick();
        }
        if self.tick == 0 {
            return false;
        }
        self.render_pending(self.tick - 1, RenderPolicy::Live)
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
        for pending in pending_edits {
            match pending {
                PendingEdit::Aim(AimEdit::Delete) => {
                    // With a cross-section on, the aim ray sees what the player sees (the cut
                    // faces); every bit of that view is also on in the play world.
                    let cut = self.refresh_cut_view();
                    let aim_world = match (&self.cut_view, cut) {
                        (Some((_, view)), true) => view,
                        _ => &self.world,
                    };
                    let Some(edit) = resolve_aim_edit(aim_world, &self.camera, AimEdit::Delete)
                    else {
                        continue;
                    };
                    debug_assert!(!edit_would_enclose_observer(
                        &self.world,
                        &self.camera,
                        edit
                    ));
                    if let Some(chem) = self.chem.as_mut() {
                        if !chem.dig(edit.coord) {
                            self.needs_sidebar_render = true;
                            continue;
                        }
                    }
                    self.needs_render |=
                        transaction.apply_edit(&self.history, &mut self.world, edit);
                    self.cut_view = None;
                }
                PendingEdit::Aim(AimEdit::Place) => {
                    let cut = self.refresh_cut_view();
                    let aim_world = match (&self.cut_view, cut) {
                        (Some((_, view)), true) => view,
                        _ => &self.world,
                    };
                    let Some(edit) = resolve_aim_edit(aim_world, &self.camera, AimEdit::Place)
                    else {
                        continue;
                    };
                    if edit_would_enclose_observer(&self.world, &self.camera, edit) {
                        continue;
                    }
                    if let Some(chem) = self.chem.as_mut() {
                        // Never place into the hidden side of a cut (it would be invisible) or
                        // onto a bit the cut only hides.
                        if cut
                            && (self.world.is_on(edit.coord)
                                || chem.cut_hides(edit.coord, chem.cut_signs(self.camera.position())))
                        {
                            chem.last = "place_refused:cut".to_owned();
                            self.needs_sidebar_render = true;
                            continue;
                        }
                        if !chem.place(edit.coord) {
                            self.needs_sidebar_render = true;
                            continue;
                        }
                    }
                    self.needs_render |=
                        transaction.apply_edit(&self.history, &mut self.world, edit);
                    self.cut_view = None;
                }
                PendingEdit::Exact(edit) => {
                    if edit_would_enclose_observer(&self.world, &self.camera, edit) {
                        continue;
                    }
                    if let Some(chem) = self.chem.as_mut() {
                        let on = self.world.is_on(edit.coord);
                        let ok = match edit.operation {
                            Operation::Off => on && chem.dig(edit.coord),
                            Operation::On => !on && chem.place(edit.coord),
                            Operation::Toggle => {
                                if on { chem.dig(edit.coord) } else { chem.place(edit.coord) }
                            }
                        };
                        if !ok {
                            continue;
                        }
                    }
                    self.needs_render |=
                        transaction.apply_edit(&self.history, &mut self.world, edit);
                    self.cut_view = None;
                }
            }
        }

        // Chem tick: due steps run here, then the chem occupancy diff (over the chem domain)
        // lands in this tick's transaction, so the play world mirrors the chem world bit for bit.
        if let Some(chem) = self.chem.as_mut() {
            let due = chem.take_due(self.tick);
            if due > 0 {
                chem.run(due);
                self.needs_render |=
                    mirror_chem(chem, &self.history, &mut self.world, &mut transaction);
            }
        }

        let post_world = match self.history.commit_tick(transaction, &self.world) {
            Some(receipt) => receipt.post_world(),
            None => pre_world,
        };
        self.last_world_tag = post_world;
        self.tick = self.tick.checked_add(1).expect("tick counter overflow");
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
                    _ => self.renderer.render_tinted(&self.world, &self.camera, Some(&tint)),
                }
            }
            None => self.renderer.render(&self.world, &self.camera),
        }
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
            && self.rgba_game_serial.is_some_and(|s| s.wrapping_add(1) == serial);
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
                    let sidebar_width = bitblock_core::sidebar::SIDEBAR_WIDTH.min(width.saturating_sub(1));
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

fn checked_accumulation(
    field: &'static str,
    prior: f64,
    delta: f64,
) -> Result<f64, ApplyError> {
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
            Self::with_gallery_program(scene - BENCH_SCENE_GALLERY_BASE)
                .unwrap_or_else(Self::new)
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
    pub fn bench_chem(preset: u32, radius: u32, sim_width: u32, sim_height: u32, sidebar: bool) -> Option<Self> {
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
                let r = mirror_chem(chem, &self.history, &mut self.world, &mut tx);
                if let Some(receipt) = self.history.commit_tick(tx, &self.world) {
                    self.last_world_tag = receipt.post_world();
                }
                self.tick += 1;
                u64::from(r)
            }
            3 => {
                self.renderer.render(&self.world, &self.camera);
                self.renderer.world_pixel_count()
            }
            4 => {
                let c = |_: Coord| Some(0x808080u32);
                self.renderer.render_tinted(&self.world, &self.camera, Some(&c));
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

/// Applies the chem occupancy diff (over the chem domain) to the play world as history edits
/// and refreshes the material hash. Returns whether the game view must repaint.
fn mirror_chem(
    chem: &mut crate::play_chem::ChemPlay,
    history: &History,
    world: &mut World,
    transaction: &mut TickTransaction,
) -> bool {
    let mut changed = 0u32;
    // FNV-1a over the domain's material ids: a material-only change (rock ->
    // sediment, water -> ice) keeps occupancy but must still repaint the chem tint.
    let mut mat_hash = 0xcbf2_9ce4_8422_2325_u64;
    // §5y: probe occupancy only where the material id changed since the last mirror (same
    // edits as the full scan; the full scan runs when the cache is empty / resized).
    let mut seen = std::mem::take(&mut chem.mirror_mats);
    let full = seen.len() != chem.world.domain_len();
    if full {
        seen.clear();
        seen.resize(chem.world.domain_len(), bitblock_core::chem::MatId::AIR);
    }
    for (i, (coord, mat, _)) in chem.world.domain().enumerate() {
        mat_hash = (mat_hash ^ u64::from(mat.0)).wrapping_mul(0x0100_0000_01b3);
        if !full && seen[i] == mat {
            continue;
        }
        seen[i] = mat;
        let want = chem.world.world().is_on(coord);
        if want != world.is_on(coord) {
            let op = if want { Operation::On } else { Operation::Off };
            if transaction.apply_edit(history, world, Edit::new(op, coord)) {
                changed += 1;
            }
        }
    }
    chem.mirror_mats = seen;
    chem.last_step_changed = changed;
    let recolored = mat_hash != chem.material_hash;
    chem.material_hash = mat_hash;
    changed > 0 || recolored
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
        status.split("frame=").nth(1).expect("frame=").split(' ').next().expect("tag")
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
            assert_eq!(without_frame(&live.status()), without_frame(&audited.status()), "n={n}");
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
            assert_eq!(without_frame(&live.status()), without_frame(&audited.status()), "phase 2 n={n}");
            assert_eq!(live.renderer.pixels(), audited.renderer.pixels(), "phase 2 n={n}");
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
        assert!(!session.step(4), "nothing changed: dirty-only rendering skips the frame");
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
            assert_eq!(live.compositor.pixels(), audited.compositor.pixels(), "frame {i}");
            assert_eq!(live.last_sidebar_facts, audited.last_sidebar_facts, "frame {i}");
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
        for (w, h, sidebar) in [(801, 801, true), (960, 540, false), (1280, 720, true), (333, 200, false)] {
            let mut session = PlaySession::new();
            session.set_layout(w, h, sidebar);
            assert!(session.step(1));
            assert_eq!(session.rgba(), &crate::xrgb_to_rgba(session.output_pixels())[..], "{w}x{h} {sidebar}");
            assert_eq!(
                session.rgba().len(),
                (session.output_width() * session.output_height() * 4) as usize
            );
        }
    }

    #[test]
    fn set_layout_sizes_and_clamps() {
        let mut session = PlaySession::new();
        assert_eq!((session.output_width(), session.output_height()), (1201, 801));
        session.set_layout(1280, 720, true);
        session.step(1);
        assert_eq!(session.simulation_size(), (1280, 720));
        assert_eq!((session.output_width(), session.output_height()), (1680, 720));
        assert!(!session.game_only());
        assert!(session.status().contains("output=1680x720"), "{}", session.status());
        session.set_layout(1280, 720, false);
        assert_eq!(frame_of(&session.status_with(false)), "none", "layout change drops the stale frame");
        session.step(1);
        assert!(session.game_only());
        assert_eq!((session.output_width(), session.output_height()), (1280, 720));
        session.set_cursor(1279.5, 10.5);
        assert!(!session.pointer_in_sidebar(), "game-only has no sidebar strip");
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
        assert_eq!((session.output_width(), session.output_height()), (1201, 801));
        assert_eq!(session.renderer.pixels(), reference.renderer.pixels());
        assert_eq!(&session.rgba()[..4 * 801], &reference.rgba()[..4 * 801], "first game row");
    }

    #[test]
    fn tape_known_answer_survives_live_layouts_on_a_fresh_session() {
        let mut live = PlaySession::new();
        live.set_layout(1920, 1080, false);
        live.step(3);
        // The tape always runs on a reset (native 801+400) session through fixed_tick.
        let tape = play_input_tape_status();
        assert!(tape.contains("tick=5 world=#bb.world.sha256.ziy2sdalw4i2k5uszp32ldvf57moe44kv2q2fcs6dkxwavoertca"), "{tape}");
        assert!(tape.contains("frame=#bb.frame.sha256.vyn2woic4slbg5vo6xxqj4qbx3pg4fsueskewrrh57j7sayknpya"), "{tape}");
        assert!(tape.ends_with("output=1201x801"), "{tape}");
    }
}

/// Start-world known answers. Pinned at the play client's DEFAULT layout — High 1600×900 +
/// 400 sidebar (output 2000×900) — after the client's startup sequence: load world →
/// `set_layout(1600, 900, sidebar)` → `step(1)` (live), then a hashing status read.
#[cfg(test)]
mod start_world_tests {
    use super::*;

    const ARCH_HIGH_KNOWN: &str = "bitblock_browser_play ok tick=1 world=#bb.world.sha256.lqpba5lvzmm5bnj2jfoa7dgtdnm2adv5eborbtvaquic36mtno2q frame=#bb.frame.sha256.5l3xbpo3zj2x65tqjinaliydy7qchd2kw6gsy3yi4gm6awvjrraa pos=-5.838272203333,19.213048230119,-25.261618887543 yaw=0.610865238198 pitch=-0.383972435439 dist=32.602331204992 last=world:architecture sidebar=1 captured=0 output=2000x900";
    const ARCH_HIGH_GAME_NON_BACKGROUND: u32 = 123_265;
    const SYMBOL_HIGH_KNOWN: &str = "bitblock_browser_play ok tick=1 world=#bb.world.sha256.x4emw7263xn5ajefvyissdm6tlv3lrd4miqr47rxsl7fj77h54eq frame=#bb.frame.sha256.araktfzrajbhvmmwo7zgywisgcrbmaewtdxhdl3ebrbaw5syncka pos=-5.208389634264,21.791351208024,-30.502793385755 yaw=0.610865238198 pitch=-0.383972435439 dist=40.819759920901 last=world:symbol sidebar=1 captured=0 output=2000x900";
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
        assert_eq!(crate::count_non_background(s.renderer.pixels()), ARCH_HIGH_GAME_NON_BACKGROUND);
        // Same world as the architecture gallery button.
        let gallery = crate::raster_gallery_program(2, crate::GALLERY_PREVIEW_SIZE).unwrap();
        assert_eq!(s.last_world_tag, gallery.world_tag);
        assert_eq!(s.rgba().len(), 2000 * 900 * 4);
    }

    #[test]
    fn symbol_start_world_at_high_known_answer() {
        let s = client_startup(0);
        assert_eq!(s.status(), SYMBOL_HIGH_KNOWN);
        assert_eq!(crate::count_non_background(s.renderer.pixels()), SYMBOL_HIGH_GAME_NON_BACKGROUND);
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
            assert!(nb > total / 50 && nb < total / 2, "world id {id}: non_background={nb}");
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
        assert_ne!(s.last_world_tag, start, "aimed place edited the gallery world");
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

    fn chem_startup(preset: u32) -> PlaySession {
        let mut s = PlaySession::with_chem(preset, 7, CHEM_RADIUS_DEFAULT).expect("chem genesis");
        s.set_layout(1600, 900, true);
        assert!(s.step(1), "startup step finalises a frame");
        s
    }

    fn assert_mirrors_chem(s: &PlaySession) {
        let chem = s.chem.as_ref().unwrap();
        assert_eq!(world_tag(&s.world), world_tag(chem.world.world()), "play world == chem occupancy");
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
            assert_eq!(s.world.iter_on().count(), chem.world.material_count() + 1, "+ bedrock");
            let nb = crate::count_non_background(s.renderer.pixels());
            let total = 1600 * 900;
            assert!(nb > total / 50 && nb < total * 3 / 4, "preset {preset}: non_background={nb}");
            assert!(s.status().contains(&format!(" chem={} ", CHEM_PRESETS[preset as usize])));
            assert_mirrors_chem(&s);
        }
        assert!(PlaySession::with_chem(3, 7, 12).is_none());
    }

    #[test]
    fn chem_auto_steps_advance_tick_and_keep_books() {
        let mut s = chem_startup(CHEM_PRESET_ASH);
        let t0 = s.chem.as_ref().unwrap().world.tick();
        s.step(4 * 60); // 60 chem steps at the default cadence
        let chem = s.chem.as_ref().unwrap();
        assert!(chem.world.tick() >= t0 + 59, "chem tick advanced: {}", chem.world.tick());
        assert!(chem.world.evaporated() > 0 || chem.world.vapor() != 0, "water cycle moved");
        assert!(chem.error.is_none());
        assert_mirrors_chem(&s);
        // Pause + manual step.
        s.chem_set_auto(0);
        let t1 = s.chem.as_ref().unwrap().world.tick();
        s.step(10);
        assert_eq!(s.chem.as_ref().unwrap().world.tick(), t1);
        s.chem_queue(3);
        s.step(1);
        assert_eq!(s.chem.as_ref().unwrap().world.tick(), t1 + 3);
        assert_mirrors_chem(&s);
    }

    #[test]
    fn dig_and_place_go_through_chem() {
        let mut s = chem_startup(CHEM_PRESET_ROCKY);
        s.chem_set_auto(0);
        let n0 = s.world.iter_on().count();
        let bag0 = s.chem.as_ref().unwrap().world.inventory();
        s.set_capture(true);
        s.pointer_button(0, true); // LMB = aimed dig
        s.step(1);
        let chem = s.chem.as_ref().unwrap();
        assert!(chem.last.starts_with("dig:"), "last={}", chem.last);
        assert_eq!(s.world.iter_on().count(), n0 - 1);
        assert_ne!(chem.world.inventory(), bag0, "dug atoms went to the bag");
        assert!(s.status().contains(" chem_last=dig:"));
        assert_mirrors_chem(&s);
        s.pointer_button(2, true); // RMB = aimed place, paid from the bag
        s.step(1);
        let chem = s.chem.as_ref().unwrap();
        assert!(chem.last.starts_with("place:"), "last={}", chem.last);
        assert_eq!(s.world.iter_on().count(), n0);
        assert_eq!(chem.world.inventory(), bag0, "place paid back what dig returned");
        assert_mirrors_chem(&s);
    }

    fn shade_pixels(s: &PlaySession, rgb: u32) -> usize {
        let shades = bitblock_core::render::tinted_shades(rgb);
        let lit = bitblock_core::render::lit_face_color(rgb);
        s.renderer.pixels().iter().filter(|p| shades.contains(p) || **p == lit).count()
    }

    /// Cut faces are lit: against the same cut rendered without lit faces (the pre-highlight
    /// look), every changed pixel is the lit colour of a chem material, the core and several
    /// bands show lit pixels with distinct hues, and turning the cut off restores the uncut frame.
    #[test]
    fn cross_section_lights_cut_faces_and_keeps_bands_distinct() {
        use bitblock_core::render::lit_face_color;
        for preset in [CHEM_PRESET_ROCKY, CHEM_PRESET_OCEAN] {
            let mut s = chem_startup(preset);
            s.chem_set_auto(0);
            let off_pixels = s.renderer.pixels().to_vec();
            let lit_of: std::collections::BTreeMap<u32, u32> = {
                let c = s.chem.as_ref().unwrap();
                c.colors.by_id.iter().filter(|&&b| b != 0).map(|&b| (lit_face_color(b), b)).collect()
            };
            let core_lit = {
                let c = s.chem.as_ref().unwrap();
                lit_face_color(c.colors.by_id[usize::from(c.world.material(c.center()).0)])
            };
            for mode in [CHEM_CUT_WEDGE, CHEM_CUT_X, CHEM_CUT_Y, CHEM_CUT_Z] {
                for offset in [0, -3] {
                    assert!(s.chem_set_cut(mode, offset));
                    assert!(s.step(1));
                    assert!(!s.cut_faces.is_empty(), "preset {preset} mode {mode}: cut faces");
                    assert!(s.cut_faces.windows(2).all(|w| w[0].0 < w[1].0), "cut faces sorted");
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
                            let base = lit_of.get(p).or_else(|| lit_of.get(&(!p & 0x00ff_ffff))).unwrap_or_else(|| {
                                panic!("preset {preset} mode {mode}: changed pixel {p:06x} not lit")
                            });
                            changed += 1;
                            hues.insert(*base);
                        }
                    }
                    let core = lit.iter().filter(|&&p| p == core_lit).count();
                    assert!(changed > 5000, "preset {preset} mode {mode}:{offset} lit pixels {changed}");
                    assert!(hues.len() >= 3, "preset {preset} mode {mode}:{offset} lit hues {hues:x?}");
                    if offset == 0 {
                        assert!(core > 300, "preset {preset} mode {mode}: core lit pixels {core}");
                    }
                    eprintln!("preset {preset} cut {mode}:{offset}: lit={changed} hues={} core={core}", hues.len());
                }
            }
            assert!(s.chem_set_cut(CHEM_CUT_OFF, 0));
            assert!(s.step(1));
            assert!(s.cut_faces.is_empty());
            assert_eq!(s.renderer.pixels(), &off_pixels[..], "preset {preset}: cut off == uncut");
        }
    }

    #[test]
    fn cross_section_reveals_core_bands_on_rocky_and_ocean() {
        for preset in [CHEM_PRESET_ROCKY, CHEM_PRESET_OCEAN] {
            let mut s = chem_startup(preset);
            s.chem_set_auto(0);
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
                assert!(s.status().ends_with(&format!(" cut={}:0", CHEM_CUT_NAMES[mode as usize]))
                    || s.status().contains(&format!(" cut={}:0", CHEM_CUT_NAMES[mode as usize])));
                assert!(s.chem_status().unwrap().ends_with(&format!(" cut={}:0", CHEM_CUT_NAMES[mode as usize])));
                // Render-only: world, history and chem untouched.
                assert_eq!(world_tag(&s.world), tag0);
                assert!(s.world.is_on(center));
                assert_mirrors_chem(&s);
                s.audit_game_palette();
            }
            // Off again: pixel-identical to the never-cut frame, status back to the old text.
            assert!(s.chem_set_cut(CHEM_CUT_OFF, 0));
            assert!(s.step(1));
            assert_eq!(s.renderer.pixels(), &off_pixels[..], "preset {preset}: cut off == before");
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
        s.chem_set_auto(0);
        assert!(s.chem_set_cut(CHEM_CUT_WEDGE, 0));
        assert!(s.step(1));
        let n0 = s.world.iter_on().count();
        let bag0 = s.chem.as_ref().unwrap().world.inventory();
        s.set_capture(true);
        s.pointer_button(0, true); // dig the cut face under the crosshair
        s.step(1);
        let (dug, last) = {
            let c = s.chem.as_ref().unwrap();
            (c.last.clone(), c.last.starts_with("dig:"))
        };
        assert!(last, "last={dug}");
        assert_eq!(s.world.iter_on().count(), n0 - 1);
        assert_mirrors_chem(&s);
        // The dug cell is a visible (not hidden) cell on the cut face.
        s.pointer_button(2, true);
        s.step(1);
        let c = s.chem.as_ref().unwrap();
        assert!(c.last.starts_with("place:") || c.last == "place_refused:cut", "last={}", c.last);
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
        let view = s.cut_view.as_ref().map(|(_, w)| w.clone()).expect("view built");
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
        assert_ne!(c.world.material(far), MatId::AIR, "refused dig left the cell");
    }

    #[test]
    fn custom_budget_matches_preset_and_spends_entered_lanes() {
        // The preset's own lanes entered as a custom budget build the very same world.
        for preset in 0..CHEM_PRESETS.len() as u32 {
            let lanes = preset_budget(preset, sphere_cells(CHEM_RADIUS_DEFAULT)).unwrap().lanes;
            let p = PlaySession::with_chem(preset, 7, CHEM_RADIUS_DEFAULT).unwrap();
            let c = PlaySession::with_chem_budget(lanes, 7, CHEM_RADIUS_DEFAULT).unwrap();
            assert_eq!(world_tag(&p.world), world_tag(&c.world), "preset {preset}");
            assert_eq!(c.chem.as_ref().unwrap().preset_name(), "custom");
            assert_eq!(c.last_action, "world:chem-custom");
        }
        // A tweaked budget: totals equal exactly what was entered, books balance while ticking.
        let lanes = [9000, 6000, 800, 300, 700, 900, 1500, 200, 1200, 900, 150, 80];
        let mut s = PlaySession::with_chem_budget(lanes, 11, 10).unwrap();
        s.set_layout(1600, 900, true);
        assert!(s.step(1));
        let chem = s.chem.as_ref().unwrap();
        assert_eq!(chem.world.lane_totals(), lanes);
        assert_eq!(chem.world.t0(), lanes);
        assert!(chem.world.material_count() > 500);
        let text = s.chem_status().unwrap();
        assert!(text.starts_with("bitblock_play_chem ok preset=custom seed=11 radius=10 "), "{text}");
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
        assert!(e.starts_with("budget_of_4294967297_atoms_exceeds_cap_4294967296"), "{e}");
        let e = PlaySession::with_chem_budget([i64::MAX, i64::MAX, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 7, 8)
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
        assert!(text.starts_with("bitblock_play_chem ok preset=ocean seed=7 radius=12 tick=1 "));
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
            assert!(hues.len() >= 8, "preset {preset}: tinted colours {}", hues.len());
            // Genesis sizes concentric shells from the budget, so from outside only the
            // outermost layers show (ash: ocean under a methane skin); inner layers are
            // under the crust.
            assert!(visible.len() >= 2, "preset {preset}: visible base colours {visible:x?}");
            s.audit_game_palette();
            eprintln!("chem preset {preset}: tinted_colours={} visible_bases={} {visible:x?}", hues.len(), visible.len());
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
            let before = s.chem.as_ref().unwrap().material_hash;
            s.chem_queue(1);
            s.step(1);
            let chem = s.chem.as_ref().unwrap();
            if chem.material_hash != before {
                assert!(s.renders() > renders, "material change must repaint");
                if chem.last_step_changed == 0 {
                    repaint_without_occupancy += 1;
                }
            }
        }
        assert!(repaint_without_occupancy > 0, "rocky recolours in place (rock -> sediment)");
        assert_mirrors_chem(&s);
    }

    #[test]
    fn non_chem_sessions_are_untouched() {
        let mut s = PlaySession::with_start_world(PLAY_WORLD_DEFAULT).unwrap();
        assert!(!s.chem_set_auto(1) && !s.chem_queue(1));
        assert!(!s.status().contains(" chem="));
    }
}
