//! Opt-in chem mode for the browser play client: a [`ChemWorld`] built by
//! `genesis` from an element budget, ticked by the full planet6 `step()` (heat,
//! phase, reactions incl. the trimmed ChemNet, water cycle, surface laws), with
//! dig / place routed through the chem heat gate and inventory bag.
//!
//! The play session's occupancy `World` stays the render / history source of
//! truth; after every chem change the occupancy diff over the chem domain is
//! applied as ordinary history edits, so world tags and receipts stay audited.
//! Non-chem worlds never touch this module (their status text is unchanged).

use std::sync::Arc;

use bitblock_core::chem::{
    genesis, ChemNet, ChemWorld, EnergyInput, GenesisBudget, GenesisConfig, GenesisShape, Lanes,
    MatId, SunSweep, WaterCycle, DEFAULT_ATOM_CAP, NE, SYM,
};
use bitblock_core::camera::Vec3;
use bitblock_core::world::{Coord, World};

/// planet6's trimmed reaction table (11 mounted rows, 2.8 KB). The full 24k-row
/// table stays test / PC-only.
const TRIMMED_NET: &str = include_str!("../../core/src/chem/fixtures/chemnet_trimmed.js.txt");

pub const CHEM_PRESET_ASH: u32 = 0;
pub const CHEM_PRESET_ROCKY: u32 = 1;
pub const CHEM_PRESET_OCEAN: u32 = 2;
pub const CHEM_PRESETS: [&str; 3] = ["ash", "rocky", "ocean"];
/// `ChemPlay::preset` of a world built from a user-entered budget (`with_budget`).
pub const CHEM_PRESET_CUSTOM: u32 = 3;
/// §5y gas planet: small Fe/Ni/Si/Mg core under a thick H2S / NH3 / CH4 envelope (no water,
/// no rock crust, no terrain noise). Loads with the matched energy (`CHEM_ENERGY_SUN8`), the
/// driven cycle and §5y gas convection on. Not in [`CHEM_PRESETS`] (that list is the rocky
/// trio the older tests and menus iterate).
pub const CHEM_PRESET_GAS: u32 = 4;
/// Browser cap on a custom budget's total atoms (2^32: ~190x the ash preset at r=20).
/// Atoms the sphere cannot hold settle into the bag, so this only bounds bag size.
pub const CHEM_ATOM_CAP: i64 = 1 << 32;

pub const CHEM_RADIUS_MIN: u32 = 2;
pub const CHEM_RADIUS_DEFAULT: u32 = 12;
/// Browser cap. §5y: after the exact network skip a native chem step is ~1.7–2.0 ms at r=20
/// (33k cells) and ~7.6–8.4 ms classic / ~11.6 ms gas at r=32 (137k cells), so r=32 fits the
/// 15 Hz chem cadence (was 20 when r=20 already cost ~5 ms).
pub const CHEM_RADIUS_MAX: u32 = 32;
/// Default auto-step cadence: one chem step every 4 sim ticks (15 Hz at 60 Hz).
pub const CHEM_EVERY_DEFAULT: u32 = 4;
/// Manual steps drained per sim tick at most.
pub const CHEM_STEPS_PER_TICK_MAX: u32 = 32;

/// Cross-section modes (`ChemCut::mode`): off, the camera-facing octant wedge, or the
/// camera-facing half beyond a plane normal to x / y / z through the sphere centre.
pub const CHEM_CUT_OFF: u32 = 0;
pub const CHEM_CUT_WEDGE: u32 = 1;
pub const CHEM_CUT_X: u32 = 2;
pub const CHEM_CUT_Y: u32 = 3;
pub const CHEM_CUT_Z: u32 = 4;
pub const CHEM_CUT_NAMES: [&str; 5] = ["off", "wedge", "x", "y", "z"];

/// Booked energy input modes (`ChemPlay::energy`): off (planet6, the world cools) or the M1
/// soak setting ([`EnergyInput::M1`]: rotating sun on space-facing faces + weak core heat).
pub const CHEM_ENERGY_OFF: u32 = 0;
pub const CHEM_ENERGY_M1: u32 = 1;
/// §5y: the energy-matched soak baseline (sun 8/face at noon, 600-tick day, core 40/tick) with
/// the full-sphere sun sweep (no global night, §5v).
pub const CHEM_ENERGY_SUN8: u32 = 2;
pub const CHEM_ENERGY_NAMES: [&str; 3] = ["off", "m1", "sun8"];
/// Water-cycle modes (`ChemPlay::cycle`): planet6's calendar ([`WaterCycle::Classic`]) or the
/// unscripted heat / vapor / activity-driven cycle ([`WaterCycle::Driven`]).
pub const CHEM_CYCLE_CLASSIC: u32 = 0;
pub const CHEM_CYCLE_DRIVEN: u32 = 1;
pub const CHEM_CYCLE_NAMES: [&str; 2] = ["classic", "driven"];

/// Render-only cross-section of the chem sphere. Never touches chem atoms, the play world or
/// history: the renderer (and the aim ray) see a filtered copy of the occupancy world with the
/// camera-facing side of the cut removed, so the interior bands show on the cut faces.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChemCut {
    /// One of the `CHEM_CUT_*` modes.
    pub mode: u32,
    /// Plane offset in cells from the centre along each cut axis, toward the camera. 0 keeps
    /// the centre slice; negative values remove more of the sphere, positive less. Clamped to
    /// `-radius..=radius`.
    pub offset: i32,
}

impl ChemCut {
    pub const fn active(self) -> bool {
        self.mode != CHEM_CUT_OFF
    }

    pub fn name(self) -> &'static str {
        CHEM_CUT_NAMES.get(self.mode as usize).copied().unwrap_or("?")
    }
}

/// Sphere cell count for radius `r` (genesis `d2 <= r2` mask).
pub fn sphere_cells(r: u32) -> i64 {
    let ri = i64::from(r);
    let mut n = 0;
    for dx in -ri..=ri {
        for dy in -ri..=ri {
            for dz in -ri..=ri {
                if dx * dx + dy * dy + dz * dz <= ri * ri {
                    n += 1;
                }
            }
        }
    }
    n
}

/// Element budget for a preset over `n` cells. Lanes: H O C N S Ca Fe Ni Si Mg Na K.
pub fn preset_budget(preset: u32, n: i64) -> Option<GenesisBudget> {
    let mut l: Lanes = [0; NE];
    match preset {
        CHEM_PRESET_ASH => return GenesisBudget::ash(n).ok(),
        CHEM_PRESET_ROCKY => {
            // metal core + mantle (Fe Ni Si Mg), rock crust (O4 S Ca), thin water, bag reserve.
            let core = n / 3;
            let crust = n * 3 / 5;
            let water = n / 20;
            l[6] = core * 2 / 5;
            l[7] = core / 10;
            l[8] = core / 4;
            l[9] = core / 4;
            l[1] = crust * 4 + water * 2 + n / 2;
            l[4] = crust + n / 8;
            l[5] = crust + n / 8;
            l[0] = water * 4;
            l[2] = n / 40;
            l[3] = n / 80;
        }
        CHEM_PRESET_OCEAN => {
            // smaller core, crust, deep water shell + methane, bag reserve.
            let core = n / 4;
            let crust = n / 3;
            let water = n * 2 / 5;
            l[6] = core * 2 / 5;
            l[7] = core / 10;
            l[8] = core / 4;
            l[9] = core / 4;
            l[1] = crust * 4 + water * 2 + n / 4;
            l[4] = crust + n / 16;
            l[5] = crust + n / 16;
            l[0] = water * 4 + n / 10;
            l[2] = n / 20;
            l[3] = n / 40;
            l[10] = n / 50;
            l[11] = n / 100;
        }
        CHEM_PRESET_GAS => {
            // §5y: every lane is sized to a cell count, so genesis places it all (empty bag):
            // ~10% Fe/Ni/Si/Mg core, then gas shells outward in LAYERS order: H2S (4%),
            // NH3 (20%), CH4 (the rest, ~66%). No water (a hot core boils a water mantle into
            // the vapor pool and leaves a vacuum gap), no rock crust.
            let (fe, ni, si, mg) = (n / 20, n / 100, n / 50, n / 50);
            let h2s = n / 25;
            let nh3 = n / 5;
            let ch4 = (n - fe - ni - si - mg - h2s - nh3).max(0);
            l[6] = fe;
            l[7] = ni;
            l[8] = si;
            l[9] = mg;
            l[0] = h2s * 2 + nh3 * 3 + ch4 * 4;
            l[4] = h2s;
            l[3] = nh3;
            l[2] = ch4;
        }
        _ => return None,
    }
    Some(GenesisBudget::new(l))
}

pub struct ChemPlay {
    pub world: ChemWorld,
    /// `0..=2` preset, or [`CHEM_PRESET_CUSTOM`].
    pub preset: u32,
    /// The element budget genesis spent (lanes H O C N S Ca Fe Ni Si Mg Na K) = `t0`.
    pub budget: Lanes,
    pub seed: u32,
    pub radius: u32,
    /// Auto-step cadence in sim ticks; 0 = paused.
    pub every: u32,
    /// Manual steps queued for the next sim tick(s).
    pub queued: u32,
    /// Material placed by right-click: the last dug material (initially rock).
    pub place_mat: MatId,
    pub last: String,
    pub error: Option<String>,
    /// Occupancy bits changed by the last chem step (render trigger).
    pub last_step_changed: u32,
    pub steps_run: u64,
    /// Material id -> tint RGB for the play renderer, and the legal tinted palette.
    pub colors: ChemColors,
    /// FNV-1a of the domain's material ids after the last chem step (repaint trigger).
    pub material_hash: u64,
    /// §5y mirror cache: the domain's material ids as of the last occupancy mirror (empty =
    /// full scan next time). Chem occupancy only changes through `set_mat`, so cells whose id
    /// is unchanged since the last mirror cannot need an edit; only changed cells are probed.
    pub mirror_mats: Vec<MatId>,
    /// Render-only cross-section (off by default).
    pub cut: ChemCut,
    /// `CHEM_ENERGY_*` (off by default: planet6 behaviour).
    pub energy: u32,
    /// `CHEM_CYCLE_*` (classic by default: planet6 behaviour).
    pub cycle: u32,
}

impl ChemPlay {
    /// Genesis from `preset`'s budget over a sphere of `radius` sitting on the bedrock bit.
    pub fn new(preset: u32, seed: u32, radius: u32) -> Option<Self> {
        let radius = radius.clamp(CHEM_RADIUS_MIN, CHEM_RADIUS_MAX);
        let budget = preset_budget(preset, sphere_cells(radius))?;
        Self::build(preset, budget.lanes, seed, radius).ok()
    }

    /// Genesis from a user-entered element budget (lanes H O C N S Ca Fe Ni Si Mg Na K) over a
    /// sphere of `radius` (clamped to the browser range). Lanes must be non-negative and total at
    /// most [`CHEM_ATOM_CAP`]; atoms the sphere cannot hold settle into the bag. The error text
    /// has no spaces (it is shown as a status field).
    pub fn with_budget(lanes: Lanes, seed: u32, radius: u32) -> Result<Self, String> {
        let radius = radius.clamp(CHEM_RADIUS_MIN, CHEM_RADIUS_MAX);
        Self::build(CHEM_PRESET_CUSTOM, lanes, seed, radius)
    }

    fn build(preset: u32, lanes: Lanes, seed: u32, radius: u32) -> Result<Self, String> {
        let err = |e: &dyn std::fmt::Display| e.to_string().replace(' ', "_");
        let cfg = GenesisConfig {
            budget: GenesisBudget::new(lanes),
            seed: u64::from(seed),
            shape: GenesisShape {
                center: Coord::new(0, i64::from(radius) + 1, 0),
                radius,
                hollow: None,
            },
            atom_cap: CHEM_ATOM_CAP.min(DEFAULT_ATOM_CAP),
        };
        let mut world = genesis(&cfg).map_err(|e| err(&e))?;
        let net = ChemNet::from_js(TRIMMED_NET).map_err(|e| err(&format!("chemnet: {e:?}")))?;
        world.attach_net(Arc::new(net));
        if preset == CHEM_PRESET_GAS {
            let mut p = *world.params();
            p.convect = true;
            world.set_params(p);
        }
        let mut play = Self {
            world,
            preset,
            budget: lanes,
            seed,
            radius,
            every: CHEM_EVERY_DEFAULT,
            queued: 0,
            place_mat: MatId::ROCK,
            last: "genesis".to_owned(),
            error: None,
            last_step_changed: 0,
            steps_run: 0,
            colors: ChemColors::new(),
            material_hash: 0,
            mirror_mats: Vec::new(),
            cut: ChemCut::default(),
            energy: CHEM_ENERGY_OFF,
            cycle: CHEM_CYCLE_CLASSIC,
        };
        if preset == CHEM_PRESET_GAS {
            play.set_dynamics(CHEM_ENERGY_SUN8, CHEM_CYCLE_DRIVEN);
        }
        Ok(play)
    }

    /// Sets the tick dynamics on the live world: `energy` one of `CHEM_ENERGY_*`, `cycle` one of
    /// `CHEM_CYCLE_*`. Only `P6Params::energy_in` / `cycle` change (seed, laws and books are
    /// kept; the inbound ledger stays closed when switching mid-run). Returns false (state
    /// unchanged) for an unknown mode.
    pub fn set_dynamics(&mut self, energy: u32, cycle: u32) -> bool {
        if energy as usize >= CHEM_ENERGY_NAMES.len() || cycle as usize >= CHEM_CYCLE_NAMES.len() {
            return false;
        }
        let mut p = *self.world.params();
        p.energy_in = match energy {
            CHEM_ENERGY_M1 => EnergyInput::M1,
            CHEM_ENERGY_SUN8 => {
                let mut e = EnergyInput::OFF;
                e.sun = 8;
                e.day = 600;
                e.core = 40;
                e
            }
            _ => EnergyInput::OFF,
        };
        // the full-sphere sweep belongs to the sun8 baseline; M1 keeps its upper-sky day
        p.sun_sweep = if energy == CHEM_ENERGY_SUN8 { SunSweep::FULL } else { SunSweep::Upper };
        p.cycle = if cycle == CHEM_CYCLE_DRIVEN { WaterCycle::Driven } else { WaterCycle::Classic };
        self.world.set_params(p);
        self.energy = energy;
        self.cycle = cycle;
        true
    }

    /// ` energy=m1` / ` cycle=driven` for non-default dynamics, else empty (status text with the
    /// defaults is byte-identical to before the feature).
    pub fn dynamics_suffix(&self) -> String {
        let mut s = String::new();
        if self.energy != CHEM_ENERGY_OFF {
            s += &format!(" energy={}", CHEM_ENERGY_NAMES.get(self.energy as usize).unwrap_or(&"?"));
        }
        if self.cycle != CHEM_CYCLE_CLASSIC {
            s += &format!(" cycle={}", CHEM_CYCLE_NAMES.get(self.cycle as usize).unwrap_or(&"?"));
        }
        s
    }

    pub fn preset_name(&self) -> &'static str {
        if self.preset == CHEM_PRESET_CUSTOM {
            return "custom";
        }
        if self.preset == CHEM_PRESET_GAS {
            return "gas";
        }
        CHEM_PRESETS.get(self.preset as usize).copied().unwrap_or("?")
    }

    /// ` cut=<mode>:<offset>` while a cross-section is active, else empty (status text with the
    /// cut off is byte-identical to before the feature).
    pub fn cut_suffix(&self) -> String {
        if self.cut.active() {
            format!(" cut={}:{}", self.cut.name(), self.cut.offset)
        } else {
            String::new()
        }
    }

    /// The genesis sphere's centre cell.
    pub fn center(&self) -> Coord {
        Coord::new(0, i64::from(self.radius) + 1, 0)
    }

    /// Sets the cross-section (`mode` one of `CHEM_CUT_*`; `offset` clamped to the radius).
    /// Returns false (state unchanged) for an unknown mode.
    pub fn set_cut(&mut self, mode: u32, offset: i32) -> bool {
        if mode as usize >= CHEM_CUT_NAMES.len() {
            return false;
        }
        let r = self.radius as i32;
        self.cut = ChemCut { mode, offset: offset.clamp(-r, r) };
        true
    }

    /// Per-axis side (+1 / -1) of the sphere centre the eye is on: the cut always removes the
    /// camera-facing side, so its faces look at the player from wherever they stand.
    pub fn cut_signs(&self, eye: Vec3) -> [i64; 3] {
        let c = self.center();
        let side = |e: f64, c: i64| if e >= c as f64 + 0.5 { 1 } else { -1 };
        [side(eye.x, c.x), side(eye.y, c.y), side(eye.z, c.z)]
    }

    /// Whether the active cut hides `coord` (chem domain cells only; never the bedrock bit).
    pub fn cut_hides(&self, coord: Coord, signs: [i64; 3]) -> bool {
        self.cut_geometry_hides(coord, signs) && self.world.heat(coord).is_some()
    }

    fn cut_geometry_hides(&self, coord: Coord, signs: [i64; 3]) -> bool {
        if !self.cut.active() {
            return false;
        }
        let c = self.center();
        let k = i64::from(self.cut.offset);
        let beyond = |s: i64, d: i64| s * d > k;
        let bx = beyond(signs[0], coord.x - c.x);
        let by = beyond(signs[1], coord.y - c.y);
        let bz = beyond(signs[2], coord.z - c.z);
        let hit = match self.cut.mode {
            CHEM_CUT_WEDGE => bx && by && bz,
            CHEM_CUT_X => bx,
            CHEM_CUT_Y => by,
            CHEM_CUT_Z => bz,
            _ => false,
        };
        hit
    }

    /// `world` with every on-bit the active cut hides switched off (render / aim view only).
    pub fn cut_view(&self, world: &World, signs: [i64; 3]) -> World {
        let mut view = world.clone();
        if self.cut.active() {
            for (coord, _, _) in self.world.domain() {
                if self.cut_geometry_hides(coord, signs) {
                    view.set_off(coord);
                }
            }
        }
        view
    }

    /// Lit-face masks ([`bitblock_core::render::TINT_LIT_DIRS`] bits) of the cut faces: for each
    /// visible chem cell, the faces whose neighbour is an on-bit the active cut hides. Those are
    /// exactly the interior faces the cross-section exposes; the renderer draws them flat and
    /// slightly lifted so the bands read apart from the shaded sphere surface. Empty with the
    /// cut off. Sorted by coordinate (domain order) for binary search. Render-only, like
    /// [`Self::cut_view`].
    pub fn cut_faces(&self, world: &World, signs: [i64; 3]) -> Vec<(Coord, u8)> {
        let mut faces = Vec::new();
        if !self.cut.active() {
            return faces;
        }
        for (coord, _, _) in self.world.domain() {
            if !world.is_on(coord) || self.cut_geometry_hides(coord, signs) {
                continue;
            }
            let mut mask = 0u8;
            for (bit, d) in bitblock_core::render::TINT_LIT_DIRS.iter().enumerate() {
                let n = Coord::new(coord.x + d[0], coord.y + d[1], coord.z + d[2]);
                if self.cut_geometry_hides(n, signs) && world.is_on(n) && self.world.heat(n).is_some() {
                    mask |= 1 << bit;
                }
            }
            if mask != 0 {
                faces.push((coord, mask));
            }
        }
        faces
    }

    /// Chem steps due on sim tick `tick` (auto cadence plus queued manual steps).
    pub fn take_due(&mut self, tick: u64) -> u32 {
        if self.error.is_some() {
            self.queued = 0;
            return 0;
        }
        let mut n = self.queued.min(CHEM_STEPS_PER_TICK_MAX);
        self.queued -= n;
        if self.every > 0 && tick % u64::from(self.every) == 0 {
            n += 1;
        }
        n
    }

    /// Runs `n` planet6 steps; a stuck fire protocol pauses auto-step and is reported.
    pub fn run(&mut self, n: u32) {
        for _ in 0..n {
            match self.world.step() {
                Ok(_) => self.steps_run += 1,
                Err(e) => {
                    self.error = Some(e.to_string().replace(' ', "_"));
                    self.every = 0;
                    self.queued = 0;
                    break;
                }
            }
        }
    }

    pub fn dig(&mut self, coord: Coord) -> bool {
        let before = self.world.material(coord);
        match self.world.dig(coord) {
            Some(m) => {
                self.place_mat = m;
                self.last = format!("dig:{}", mat_name(m));
                true
            }
            None => {
                self.last = if before == MatId::AIR {
                    "dig_refused:not_chem".to_owned()
                } else {
                    format!("dig_refused_heat:{}", mat_name(before))
                };
                false
            }
        }
    }

    pub fn place(&mut self, coord: Coord) -> bool {
        for m in [self.place_mat, MatId::ROCK] {
            if self.world.place(coord, m) {
                self.last = format!("place:{}", mat_name(m));
                return true;
            }
        }
        self.last = format!("place_refused:{}", mat_name(self.place_mat));
        false
    }

    fn drift_abs(&self) -> i64 {
        self.world.lane_drift().iter().map(|x| x.abs()).sum()
    }

    /// Short fields appended to the play status line in chem mode.
    pub fn status_fields(&self) -> String {
        format!(
            " chem={} seed={} radius={} chem_tick={} auto={} vapor={} materials={} lane_drift={} energy_drift={} chem_last={}{}",
            self.preset_name(),
            self.seed,
            self.radius,
            self.world.tick(),
            self.every,
            self.world.vapor(),
            self.world.material_count(),
            self.drift_abs(),
            self.world.energy_drift(),
            self.last,
            self.error.as_ref().map(|e| format!(" chem_error={e}")).unwrap_or_default(),
        ) + &self.dynamics_suffix()
            + &self.cut_suffix()
    }

    /// Full chem readout for the debug panel.
    pub fn status(&self) -> String {
        let lanes = |l: Lanes| {
            (0..NE)
                .map(|k| format!("{}:{}", SYM[k], l[k]))
                .collect::<Vec<_>>()
                .join(",")
        };
        format!(
            "bitblock_play_chem ok preset={} seed={} radius={} tick={} auto={} queued={} vapor={} evaporated={} rained={} materials={} domain={} space_q={} energy={} energy_drift={} books={} place={} last={} error={} lanes={} drift={} bag={} budget={} atoms={}{}{}",
            self.preset_name(),
            self.seed,
            self.radius,
            self.world.tick(),
            self.every,
            self.queued,
            self.world.vapor(),
            self.world.evaporated(),
            self.world.rained(),
            self.world.material_count(),
            self.world.domain_len(),
            self.world.space_q(),
            self.world.energy_total(),
            self.world.energy_drift(),
            if self.world.check_books().is_ok() { "ok" } else { "BAD" },
            mat_name(self.place_mat),
            self.last,
            self.error.as_deref().unwrap_or("none"),
            lanes(self.world.lane_totals()),
            lanes(self.world.lane_drift()),
            lanes(self.world.inventory()),
            lanes(self.budget),
            self.budget.iter().sum::<i64>(),
            self.dynamics_suffix(),
            self.cut_suffix(),
        )
    }
}

/// Lit (top-face) RGB per material id for the play renderer's chem tint. Faces are shaded from
/// it by `bitblock_core::render::tinted_face_color`; the bedrock bit and anything outside the chem
/// domain keep the ordinary occupancy colours. Readable and distinct beats physically exact.
pub const CHEM_PALETTE: &[(u8, u32)] = &[
    (1, 0x8a8580),   // rock: warm grey
    (2, 0x2f6fd6),   // water: blue
    (3, 0xbfe9ff),   // ice: pale cyan
    (4, 0xe8dcc0),   // gypsum: cream
    (5, 0x6f7f96),   // mineral: slate blue-grey
    (6, 0xb08a5a),   // sediment: sand brown
    (7, 0xc8c8dc),   // co2: pale lavender
    (8, 0xc8f0a0),   // nh3: pale green
    (9, 0xd8cf98),   // limestone: khaki
    (10, 0x9fd8ff),  // o2: sky
    (11, 0xf4f0e0),  // quicklime: off-white
    (12, 0xe6eef2),  // steam: mist
    (13, 0xd8d060),  // h2s: sulfur yellow
    (14, 0xc0a040),  // so: ochre
    (15, 0xa09040),  // cas: olive
    (16, 0xe0e080),  // cs2: pale yellow
    (39, 0x6fc36f),  // methane: green
    (40, 0xb0a090),  // caco: taupe
    (101, 0xff80c0), // H: pink
    (106, 0x383838), // C: charcoal
    (107, 0x6080ff), // N: periwinkle
    (108, 0x80c0ff), // O: light blue
    (111, 0xe0a040), // Na: orange
    (112, 0xc0c8d0), // Mg: silver
    (114, 0x4e5668), // Si: dark slate
    (116, 0xf0e030), // S: yellow
    (119, 0xc070e0), // K: violet
    (120, 0xf0f0f0), // Ca: white
    (126, 0xb04020), // Fe: rust red
    (128, 0x70a090), // Ni: green metal
];

/// Base RGB for any material id: the palette entry, or a deterministic mid-tone hash colour for
/// ids without one (rare hand rows and network-minted materials). Air has no colour.
pub fn material_rgb(m: MatId) -> Option<u32> {
    if m == MatId::AIR {
        return None;
    }
    if let Some(&(_, rgb)) = CHEM_PALETTE.iter().find(|(id, _)| *id == m.0) {
        return Some(rgb);
    }
    let h = u32::from(m.0).wrapping_mul(0x9e37_79b1);
    let c = |shift: u32| 0x50 + ((h >> shift) & 0x7f);
    Some((c(8) << 16) | (c(15) << 8) | c(22))
}

/// Dense id -> RGB table (0 = no tint) plus the sorted set of every RGB the tinted world pass
/// can emit, for the play renderer and its palette audit.
pub struct ChemColors {
    pub by_id: [u32; 256],
    pub legal: Vec<u32>,
}

impl ChemColors {
    pub fn new() -> Self {
        let mut by_id = [0u32; 256];
        let mut legal = Vec::with_capacity(256 * 6);
        for id in 0..=255u8 {
            if let Some(rgb) = material_rgb(MatId(id)) {
                by_id[id as usize] = rgb;
                legal.extend(bitblock_core::render::tinted_shades(rgb));
                legal.push(bitblock_core::render::lit_face_color(rgb));
            }
        }
        legal.sort_unstable();
        legal.dedup();
        Self { by_id, legal }
    }

    pub fn is_legal(&self, rgb: u32) -> bool {
        self.legal.binary_search(&rgb).is_ok()
    }
}

impl Default for ChemColors {
    fn default() -> Self {
        Self::new()
    }
}

fn mat_name(m: MatId) -> String {
    bitblock_core::chem::name(m)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("m{}", m.0))
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chem_palette_is_distinct_and_covers_genesis_materials() {
        let mut seen = std::collections::BTreeSet::new();
        for &(id, rgb) in CHEM_PALETTE {
            assert!(seen.insert(rgb), "duplicate colour for id {id}");
            assert!(bitblock_core::chem::name(MatId(id)).is_some(), "id {id} has no chem row");
        }
        let colors = ChemColors::new();
        assert_eq!(colors.by_id[0], 0);
        // Lit cut-face colours stay one per material (bands remain distinguishable) and legal.
        let lit: std::collections::BTreeSet<u32> = CHEM_PALETTE
            .iter()
            .map(|&(_, rgb)| bitblock_core::render::lit_face_color(rgb))
            .collect();
        assert_eq!(lit.len(), CHEM_PALETTE.len());
        assert!(lit.iter().all(|&rgb| colors.is_legal(rgb)));
        for p in 0..3 {
            let c = ChemPlay::new(p, 7, 12).unwrap();
            let mut ids = std::collections::BTreeSet::new();
            for (_, m) in c.world.materials() {
                ids.insert(m.0);
                assert!(CHEM_PALETTE.iter().any(|(id, _)| *id == m.0), "preset {p} id {} unpaletted", m.0);
            }
            assert!(ids.len() >= 5, "preset {p} materials {ids:?}");
        }
    }

    #[test]
    fn full_ball_core_rain_sheds_locally_where_down_rain_spills() {
        // §5s: the r12 ocean ball is full (no AIR over its flank or underside
        // rock): -y rain spills planet-wide from those regions; core rain
        // lands radially and sheds the rest onto the nearest regions with
        // ground. Books exact either way.
        let mut out = Vec::new();
        for fall in [bitblock_core::chem::RainFall::Down, bitblock_core::chem::RainFall::Core] {
            let mut c = ChemPlay::new(CHEM_PRESET_OCEAN, 1, 12).unwrap();
            assert!(c.set_dynamics(CHEM_ENERGY_M1, CHEM_CYCLE_DRIVEN));
            let mut p = *c.world.params();
            assert_eq!(p.rain_fall, bitblock_core::chem::RainFall::Core, "default");
            p.rain_fall = fall;
            c.world.set_params(p);
            c.run(2500);
            assert!(c.error.is_none(), "{:?}", c.error);
            assert!(c.world.check_books().is_ok());
            assert_eq!(c.world.energy_drift(), 0);
            assert!(c.world.lane_drift().iter().all(|&x| x == 0));
            assert_eq!(c.world.vapor_bins().iter().sum::<i64>(), c.world.vapor());
            out.push((c.world.rained(), c.world.rain_spilled(), c.world.rain_shed()));
        }
        let ((dr, ds, dh), (cr, cs, ch)) = (out[0], out[1]);
        assert!(dr > 0 && cr > 0, "{out:?}");
        assert!(ds > 0 && dh == (0, 0), "down: spills, never sheds {out:?}");
        assert!(cs == 0 && ch.0 > 0 && ch.1 >= ch.0 && ch.0 < cr, "core: no spill, sheds {out:?}");
    }

    #[test]
    fn full_ball_core_evap_keeps_books_and_differs_from_up_evap() {
        // §5t: Driven evaporation tops follow the rain frame (outward step
        // from the core is AIR or space) by default; `Up` is the §5s +y test.
        use bitblock_core::chem::EvapTop;
        let mut out = Vec::new();
        for top in [EvapTop::Up, EvapTop::Core] {
            let mut c = ChemPlay::new(CHEM_PRESET_OCEAN, 1, 12).unwrap();
            assert!(c.set_dynamics(CHEM_ENERGY_M1, CHEM_CYCLE_DRIVEN));
            let mut p = *c.world.params();
            assert_eq!(p.evap_top, EvapTop::Core, "default");
            p.evap_top = top;
            c.world.set_params(p);
            c.run(2500);
            assert!(c.error.is_none(), "{:?}", c.error);
            assert!(c.world.check_books().is_ok());
            assert_eq!(c.world.energy_drift(), 0);
            assert!(c.world.lane_drift().iter().all(|&x| x == 0));
            assert_eq!(c.world.vapor_bins().iter().sum::<i64>(), c.world.vapor());
            assert_eq!(c.world.rain_spilled(), 0);
            out.push((c.world.evaporated(), c.world.rained()));
        }
        assert!(out.iter().all(|&(e, r)| e > 0 && r > 0), "{out:?}");
        assert_ne!(out[0], out[1], "the evaporation frame matters on the full ball {out:?}");
    }

    #[test]
    fn dynamics_default_classic_off_and_switch_keeps_books() {
        let mut c = ChemPlay::new(CHEM_PRESET_OCEAN, 7, 8).unwrap();
        assert_eq!((c.energy, c.cycle), (CHEM_ENERGY_OFF, CHEM_CYCLE_CLASSIC));
        assert_eq!(c.world.params().energy_in, EnergyInput::OFF);
        assert_eq!(c.world.params().cycle, WaterCycle::Classic);
        assert_eq!(c.dynamics_suffix(), "");
        let seed = c.world.params().seed;
        assert!(!c.set_dynamics(3, 0));
        assert!(!c.set_dynamics(0, 2));
        assert_eq!(c.world.params().energy_in, EnergyInput::OFF);
        c.run(5);
        assert!(c.set_dynamics(CHEM_ENERGY_M1, CHEM_CYCLE_DRIVEN));
        assert_eq!(c.world.params().energy_in, EnergyInput::M1);
        assert_eq!(c.world.params().cycle, WaterCycle::Driven);
        assert_eq!(c.world.params().seed, seed);
        assert_eq!(c.dynamics_suffix(), " energy=m1 cycle=driven");
        assert!(c.status_fields().ends_with(" energy=m1 cycle=driven"));
        c.run(20);
        assert!(c.error.is_none(), "{:?}", c.error);
        assert!(c.world.input_q() > 0, "M1 books inbound heat");
        assert!(c.world.check_books().is_ok());
        assert_eq!(c.world.energy_drift(), 0);
        assert!(c.world.lane_drift().iter().all(|&x| x == 0));
        assert!(c.set_dynamics(CHEM_ENERGY_OFF, CHEM_CYCLE_CLASSIC));
        let q = c.world.input_q();
        c.run(5);
        assert_eq!(c.world.input_q(), q, "off books nothing more");
        assert!(c.world.check_books().is_ok());
        assert_eq!(c.dynamics_suffix(), "");
    }

    #[test]
    fn gas_preset_loads_sun8_driven_convect_and_keeps_books() {
        assert!(!CHEM_PRESETS.contains(&"gas"));
        let mut c = ChemPlay::new(CHEM_PRESET_GAS, 7, 10).unwrap();
        assert_eq!(c.preset_name(), "gas");
        assert_eq!((c.energy, c.cycle), (CHEM_ENERGY_SUN8, CHEM_CYCLE_DRIVEN));
        let p = *c.world.params();
        assert!(p.convect);
        assert_eq!((p.energy_in.sun, p.energy_in.day, p.energy_in.core), (8, 600, 40));
        assert_eq!(p.sun_sweep, SunSweep::FULL);
        assert_eq!(p.cycle, WaterCycle::Driven);
        assert!(c.dynamics_suffix().contains("energy=sun8"));
        // envelope is gas, no water / rock crust
        let count = |m: MatId| c.world.domain().filter(|&(_, x, _)| x == m).count();
        assert_eq!(count(MatId::WATER), 0);
        assert_eq!(count(MatId::ROCK), 0);
        assert!(count(MatId::CH4) > count(MatId(126)) * 3, "thick gas envelope over a small core");
        let mut convected = 0u64;
        for _ in 0..40 {
            convected += u64::from(c.world.step().unwrap().convected);
        }
        assert!(convected > 0, "gas envelope convects");
        assert!(c.world.check_books().is_ok());
        assert_eq!(c.world.energy_drift(), 0);
        assert!(c.world.lane_drift().iter().all(|&x| x == 0));
        // rocky presets are untouched: convection stays off
        assert!(!ChemPlay::new(CHEM_PRESET_OCEAN, 7, 8).unwrap().world.params().convect);
    }
}
