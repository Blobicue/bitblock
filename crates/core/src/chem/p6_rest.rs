//! planet6's surface laws, run after `stepCombine2` in `step()` order:
//! `stepWaterCycle`, `stepErode`, `stepDissolve`, `stepSettle`, `stepFlow`
//! (`games/planet/core.js`), ported bit-for-bit in integer arithmetic.
//!
//! Books, law by law:
//! * **water cycle** — evaporation turns a surface WATER voxel into AIR and
//!   adds one pair to `vapor` (lanes count a pair as 4 H + 2 O; energy counts
//!   it as `LATENT_PAIR - BIND[WATER]`), paying `LATENT_PAIR` out of the
//!   cell's heat. Rain is the exact inverse (AIR -> WATER, `vapor -= 1`,
//!   `Q += LATENT_PAIR`). Both conserve lanes and energy exactly (planet6
//!   gates evaporation on `Q > 200`, so its `max(0, Q - 30)` never clips).
//! * **erode** — ROCK/MINERAL -> SEDIMENT: identical composition and BIND.
//! * **dissolve** — GYPSUM -> SEDIMENT + its first AIR pocket -> WATER:
//!   CaSO4.2H2O = CaSO4 + 2 H2O and 72 = 36 + 36.
//! * **settle / flow** — material swaps; heat stays with the cell, as in
//!   planet6.
//!
//! Domain generalisation (as for the other laws): a neighbour "exists" iff it
//! is a domain cell; rolls use [`super::p6::cell_key`]; the settle/flow scan
//! is y ascending, then x, then z over the whole domain (on planet6's grid
//! that is exactly its `for y, for x, for z` with the MASK test).

use super::p6::{StepReport, Topology, WaterCycle, NB6, wide_roll};
use super::vapor::{EvapTop, RainFall};
use super::{ChemWorld, MatId, NE};
use crate::world::Coord;

/// Heat a vapor pair carries aloft, released on rain (planet6 `LATENT_PAIR`).
pub const LATENT_PAIR: i32 = 30;
/// planet6's season length (`tick % 240`).
pub const SEASON: u64 = 240;
/// Storm window: `tick % SEASON < STORM_LEN` (erode, dissolve).
pub const STORM_LEN: u64 = 48;
/// Rain window: `RAIN_START <= tick % SEASON < RAIN_END` (with vapor > 0).
pub const RAIN_START: u64 = 96;
pub const RAIN_END: u64 = 144;
/// Rain drops per tick (planet6 `Math.min(vapor, 3)`).
const DROPS: i64 = 3;

const SALT_EVAP: u32 = 0x3aa77;
const SALT_DISSOLVE: u32 = 0x77aa11;

// ---- WaterCycle::Driven (BitBlock-only; no calendar anywhere) ----

/// Per-tick activity marks on a cell (driven mode).
pub(crate) const MARK_FALL: u8 = 1; // water fell into this cell this tick
pub(crate) const MARK_SIDE: u8 = 2; // water spread sideways into this cell
pub(crate) const MARK_RAIN: u8 = 4; // rain landed here this tick
pub(crate) const MARK_PHASE: u8 = 8; // froze or thawed this tick
const MARK_RUN: u8 = MARK_FALL | MARK_RAIN;

/// Evaporation gate (same as planet6): surface WATER with `Q > EVAP_Q`.
pub const EVAP_Q: i32 = 200;
/// Driven evaporation odds per tick, in 1/65536: `EVAP_BASE + (Q-EVAP_Q)*EVAP_SLOPE`,
/// capped at `EVAP_MAX`. At Q=216 that is ~0.1%/tick (planet6's mean rate,
/// 8/1000 per 8 ticks); hot water evaporates faster.
pub const EVAP_BASE: u32 = 48;
pub const EVAP_SLOPE: u32 = 1;
pub const EVAP_MAX: u32 = 1024;
/// Driven rain rate: `vapor / RAIN_TAU` drops per tick (fractional part by a
/// roll), so the vapor pool relaxes to `evap_rate * RAIN_TAU` instead of
/// being drained by a calendar window.
pub const RAIN_TAU: i64 = 512;
/// Hard cap of drops per tick (cost bound only).
pub const RAIN_MAX_DROPS: i64 = 64;
/// Dew point for rain placement: sky-open AIR cells with `Q < DEW_Q` get up
/// to 16x the weight of warm ones.
pub const DEW_Q: i32 = 256;
/// Driven erosion odds (per 1000): rock/mineral beside falling or freshly
/// rained water, beside sloshing water, beside a freeze/thaw (frost wedging).
pub const ERODE_RUN: u32 = 40;
pub const ERODE_SLOSH: u32 = 1;
pub const ERODE_FROST: u32 = 20;
/// Driven dissolve odds (per 1000) beside running / sloshing water.
pub const DISSOLVE_RUN: u32 = 60;
pub const DISSOLVE_SLOSH: u32 = 4;
/// Entrainment odds (per 1000): sediment beside running / sloshing water at
/// its own level or below swaps into the water's cell (carried downstream).
pub const ENTRAIN_RUN: u32 = 120;
pub const ENTRAIN_SLOSH: u32 = 8;


// ---- Driven relief laws (late-relief follow-up; all book-neutral) ----

/// Dry weathering odds per tick, in 1/65536, for ROCK/MIN with at least one
/// face on AIR or space (no water needed): `WEATHER_AIR` for the exposure,
/// plus `WEATHER_SUN` per space face on the sun's day track (-x, +x, +y under
/// [`SunSweep::Upper`](super::p6::SunSweep); `Full` adds -y, and ±z with a
/// tilt; see `SunSweep::track_mask`), scaled by `energy_in.sun / 16`. Those
/// faces are the ones the day/night cycle heats and cools, so their rock sees
/// the thermal swing; faces off the track never get sun. Rock covered by anything (including its own SED) stops weathering.
pub const WEATHER_AIR: u32 = 2;
pub const WEATHER_SUN: u32 = 3;
/// Scour odds (per 1000): SED lying directly on ROCK/MIN with a moving WATER
/// neighbour (any face, above included) is pushed one cell sideways into a
/// fluid (AIR/WATER) side cell, preferring a side whose own floor is fluid
/// (a drop: the grain then settles downhill). The rock it covered is bared.
pub const SCOUR_RUN: u32 = 250;
pub const SCOUR_SLOSH: u32 = 20;
/// Core-heat uplift (convection roll), see [`ChemWorld::uplift_pass`].
/// Root depth: the roll turns `UPLIFT_DEPTH - 1` cells below a column top.
pub const UPLIFT_DEPTH: i64 = 4;
/// Odds per candidate column per tick, in 1/2^32, per unit of root heat
/// contrast `Q[root] - Q[neighbour root]` (only hotter roots rise).
pub const UPLIFT_K: u64 = 4096;
/// Contrast cap (keeps the odds bounded at hot spots).
pub const UPLIFT_DQ_MAX: i64 = 400;
/// Relief guard: after a roll the risen top may stand at most this many
/// cells above the lowered neighbour top.
pub const UPLIFT_MAX_STEP: i64 = 3;
/// At most this many rolls per tick (cost and coupling bound).
pub const UPLIFT_MAX_PER_TICK: u32 = 1;

/// Deep convective overturn ("plume" law, the longer-range relief driver),
/// see [`ChemWorld::plume_pass`]. Root depth below the cold column top.
pub const PLUME_DEPTH: i64 = 8;
/// Lateral reach of the root-level return channel: the rising (hot) column
/// stands `PLUME_REACH_MIN..=PLUME_REACH_MAX` cells from the sinking one.
pub const PLUME_REACH_MIN: i64 = 2;
pub const PLUME_REACH_MAX: i64 = 6;
/// Odds per candidate column per tick, in 1/2^32, per unit of root contrast.
pub const PLUME_K: u64 = 262_144;
/// Contrast cap for the plume odds.
pub const PLUME_DQ_MAX: i64 = 400;
/// Slope guard: the risen hot top may stand at most `PLUME_SLOPE * reach`
/// above the cold column top.
pub const PLUME_SLOPE: i64 = 2;
/// At most this many plume firings per tick.
pub const PLUME_MAX_PER_TICK: u32 = 2;
/// Whole-column plume headroom: the hot column's fluid slot must have this
/// many more fluid cells straight above it, so a rising top never fills the
/// outermost domain layer (the sky-shell ceiling).
pub const PLUME_HEADROOM: i64 = 1;
/// Whole-column plume repose guard: after a firing the sunk cold top may sit
/// at most this many cells below each of its four lateral neighbour tops,
/// and the risen hot top at most this many above each of its own (no shafts
/// or spires; broad relief only).
pub const PLUME_REPOSE: i64 = 4;

/// Radial frame for uplift and plume root selection (Driven only), see
/// [`ChemWorld::radial_peer`] and [`ChemWorld::heated_from_below`]. The
/// cold side of a pair is compared at the hot root's own radius: the cell of
/// the cold column whose squared distance from the core is nearest the hot
/// root's, searched at most this many cells up or down from the root level
/// (a cold column that never reaches that radius within the window is skipped).
pub const RADIAL_PEER_SPAN: i64 = 8;

/// Outward lattice step for an offset `d = c - o` from the core (Driven radial
/// frame, see [`ChemWorld::heated_from_below`]): the nearest 26-neighbour
/// direction, taking every axis whose component is at least half the largest
/// (`2 |d_i| >= max |d|`), each with its sign. On an axis this is the old single
/// step; within ~26.6 degrees of a diagonal it is the diagonal. `(0, 0, 0)` only
/// at the core.
pub(crate) fn outward_step(d: (i64, i64, i64)) -> (i64, i64, i64) {
    let m = d.0.abs().max(d.1.abs()).max(d.2.abs());
    let s = |v: i64| if 2 * v.abs() >= m { v.signum() } else { 0 };
    (s(d.0), s(d.1), s(d.2))
}

/// Hydrothermal degassing (Driven, `core > 0`), see [`ChemWorld::degas_pass`]:
/// submerged WATER (another WATER cell above it) on a rock-like bed, hotter
/// than `DEGAS_Q`, boils into the vapor pool. Odds per tick in 1/65536:
/// `min((Q - DEGAS_Q) * DEGAS_SLOPE, DEGAS_MAX)`.
pub const DEGAS_Q: i32 = 400;
pub const DEGAS_SLOPE: u32 = 1;
pub const DEGAS_MAX: u32 = 512;
/// At most this many degassing firings per tick.
pub const DEGAS_MAX_PER_TICK: u32 = 2;

/// Void collapse (Driven), see [`ChemWorld::collapse_pass`]: odds per tick, in
/// 1/65536, that a rock-like cell with no inward support drops one cell
/// toward the core into the AIR void under it.
pub const COLLAPSE_ODDS: u32 = 4096;

const SALT_WEATHER: u32 = 0x3ea7_43e2;
const SALT_SCOUR: u32 = 0x5c0_0e2;
const SALT_UPLIFT: u32 = 0x0b1f_7ed;
const SALT_PLUME: u32 = 0x9_1a3e;
const SALT_DEGAS: u32 = 0xde6a_5ed;
const SALT_RAIN: u32 = 0x5a17_0d0b;
const SALT_ERODE: u32 = 0xe2_0de;
const SALT_ENTRAIN: u32 = 0xe47_2a1;
const SALT_COLLAPSE: u32 = 0xc011_a95e;
const SALT_FLOW: u32 = 0xf10_c0de;

/// Fixed sideways order of `stepFlow`: `(dx, dz)`.
const FLOW_SIDE: [(i64, i64); 4] = [(0, -1), (-1, 0), (0, 1), (1, 0)];

/// §5y gas convection temperature offset: `T = Q + CONVECT_T0` in the
/// ideal-gas density test (Q is 0-based heat, so T must stay positive).
pub const CONVECT_T0: i64 = 300;
/// Hysteresis of the density test, per mille: the lower parcel must be at
/// least this much lighter (`M_b * T_a * 1000 + margin < M_a * T_b * 1000`)
/// so equal parcels never shuffle.
pub const CONVECT_MARGIN_PERMILLE: i64 = 10;
/// Odds (of 1000) that an unstable pair overturns this tick.
pub const CONVECT_ODDS: u32 = 500;
const SALT_CONVECT: u32 = 0xc0_e7ec;
/// Molar mass per lane (H O C N S Ca Fe Ni Si Mg Na K), g/mol rounded.
const LANE_MASS: [i64; NE] = [1, 16, 12, 14, 32, 40, 56, 59, 28, 24, 23, 39];

impl ChemWorld {
    /// §5y gas convection (Driven, `params.convect`, core gravity only):
    /// outermost first, each rising-gas cell whose outward step is another
    /// rising-gas cell trades places with it (material and `Q` together) when
    /// it is the lighter parcel, `M_b * T_a < M_a * T_b` with the
    /// [`CONVECT_MARGIN_PERMILLE`] hysteresis, at [`CONVECT_ODDS`]. A parcel
    /// moves at most once per tick. Atom and heat totals are unchanged.
    pub(crate) fn convect_pass(&mut self, topo: &Topology, rep: &mut StepReport) {
        if !self.params.convect {
            return;
        }
        let Some(o) = self.grav_core(topo) else { return };
        let mut mass = [0i64; 256];
        for (m, slot) in mass.iter_mut().enumerate() {
            let id = MatId(m as u8);
            if self.is_rising(id) {
                if let Some(c) = self.table().comp(id) {
                    *slot = (0..NE).map(|k| c[k] * LANE_MASS[k]).sum();
                }
            }
        }
        let len = topo.rsort.len();
        let mut moved = vec![false; self.cells.len()];
        for k in 0..len {
            let i = topo.rsort[len - 1 - k] as usize;
            let mb = mass[self.mat[i].0 as usize];
            if mb == 0 || moved[i] {
                continue;
            }
            let p = self.cells[i];
            let st = outward_step((p.x - o.x, p.y - o.y, p.z - o.z));
            if st == (0, 0, 0) {
                continue;
            }
            let Some(up) = topo.offset(&self.cells, i, st) else { continue };
            let ma = mass[self.mat[up].0 as usize];
            if ma == 0 || moved[up] {
                continue;
            }
            let tb = i64::from(self.q[i]) + CONVECT_T0;
            let ta = i64::from(self.q[up]) + CONVECT_T0;
            if tb <= 0 || ta <= 0 {
                continue;
            }
            // lower parcel lighter: rho_b = M_b / T_b < rho_a = M_a / T_a
            if mb * ta * (1000 + CONVECT_MARGIN_PERMILLE) >= ma * tb * 1000 {
                continue;
            }
            if self.roll(topo, SALT_CONVECT, i) >= CONVECT_ODDS {
                continue;
            }
            let (m_b, m_a) = (self.mat[i], self.mat[up]);
            self.set_mat(i, m_a);
            self.set_mat(up, m_b);
            self.q.swap(i, up);
            moved[i] = true;
            moved[up] = true;
            rep.convected += 1;
        }
    }

    /// §5w coreward frame for this tick: the core cell's coordinate when
    /// surface gravity points at it ([`ChemWorld::coreward`] and a core
    /// exists), else `None` (world `-y`).
    #[inline]
    pub(crate) fn grav_core(&self, topo: &Topology) -> Option<Coord> {
        if !self.coreward() || topo.r2.is_empty() {
            return None;
        }
        topo.core.map(|c| self.cells[c as usize])
    }

    /// Coreward "below" of cell `i` (§5w): the first cell of its inward
    /// support set ([`Self::inward_support`]) whose material passes `open`
    /// and that the shell guard does not block. A diagonal inward step is
    /// taken only through an open corner (every axis component in the domain
    /// and AIR/WATER), so nothing squeezes between two solids; otherwise an
    /// open axis component is the fall target. `None` at the core.
    pub(crate) fn inward_target(&self, topo: &Topology, o: Coord, i: usize, open: impl Fn(MatId) -> bool) -> Option<usize> {
        let (set, k) = Self::inward_support(o, self.cells[i]);
        for q in 0..k {
            let Some(u) = topo.offset(&self.cells, i, set[q]) else { continue };
            if !open(self.mat[u]) || self.shell_blocked(u) {
                continue;
            }
            if q == 0 && k > 1 {
                let corner = set[1..k].iter().all(|d| {
                    topo.offset(&self.cells, i, *d).is_some_and(|v| self.fluid(self.mat[v]))
                });
                if !corner {
                    continue;
                }
            }
            return Some(u);
        }
        None
    }

    /// Coreward sideways target of cell `i` (§5w): among the six face
    /// neighbours whose material passes `open` (not shell-blocked) and whose
    /// squared core distance is at most `r2[i] + 1` (level, or the one-unit
    /// rise of a pure lattice tangent step), the lowest; ties in face order
    /// rotated by `roll` so no direction is favoured.
    pub(crate) fn side_target(&self, topo: &Topology, i: usize, roll: u32, open: impl Fn(MatId) -> bool) -> Option<usize> {
        let lim = topo.r2[i] + 1;
        let start = (roll % 6) as usize;
        let mut best: Option<(usize, i64)> = None;
        for k in 0..6 {
            let Some(s) = topo.offset(&self.cells, i, NB6[(start + k) % 6]) else { continue };
            let r = topo.r2[s];
            if r > lim || !open(self.mat[s]) || self.shell_blocked(s) {
                continue;
            }
            if best.is_none_or(|(_, br)| r < br) {
                best = Some((s, r));
            }
        }
        best.map(|b| b.0)
    }

    /// Clear the per-tick marks (driven mode only; classic never touches them).
    pub(crate) fn reset_marks(&mut self) {
        if self.params.cycle == WaterCycle::Driven {
            let n = self.cells.len();
            self.marks.clear();
            self.marks.resize(n, 0);
        }
    }

    #[inline]
    pub(crate) fn mark(&mut self, i: usize, m: u8) {
        if let Some(x) = self.marks.get_mut(i) {
            *x |= m;
        }
    }

    #[inline]
    fn nb_marks(&self, topo: &Topology, i: usize) -> u8 {
        topo.nb6(&self.cells, i)
            .into_iter()
            .flatten()
            .fold(0u8, |a, j| a | self.marks[j])
    }

    /// Evaporation frame for [`Self::evap_open`]: the core cell (nearest the
    /// bounding-box centre) under [`EvapTop::Core`]/[`EvapTop::CoreAir`],
    /// `None` (world +y) under [`EvapTop::Up`] or on an empty domain.
    pub(crate) fn evap_frame(&self, topo: &Topology) -> Option<Coord> {
        match self.params.evap_top {
            EvapTop::Up => None,
            EvapTop::Core | EvapTop::CoreAir => topo.core.map(|c| self.cells[c as usize]),
        }
    }

    /// Is WATER cell `i` a pool top that may evaporate? With no frame: the
    /// world +y cell is out of the domain or not WATER (§5b–§5s). With a core
    /// frame: the outward lattice step from the core is out of the domain
    /// (space) or not WATER ([`EvapTop::Core`]) / AIR ([`EvapTop::CoreAir`],
    /// strict, rain's landing test). The core cell itself never is.
    pub(crate) fn evap_open(&self, topo: &Topology, frame: Option<Coord>, i: usize) -> bool {
        let Some(o) = frame else {
            return topo
                .offset(&self.cells, i, (0, 1, 0))
                .is_none_or(|u| self.mat[u] != MatId::WATER);
        };
        let p = self.cells[i];
        let s = outward_step((p.x - o.x, p.y - o.y, p.z - o.z));
        if s == (0, 0, 0) {
            return false;
        }
        let air_only = self.params.evap_top == EvapTop::CoreAir;
        topo.offset(&self.cells, i, s).is_none_or(|u| {
            let m = self.mat[u];
            if air_only { m == MatId::AIR } else { m != MatId::WATER }
        })
    }

    /// Driven water cycle: heat-scaled evaporation, vapor-scaled rain placed
    /// on sky-open ground by a content-addressed weighted pick.
    ///
    /// Evaporation's pool-top test follows [`EvapTop`]: `Core` (default, §5t)
    /// takes a WATER cell whose outward step from the core is space or not
    /// WATER (rain's frame, so flank and underside puddles cycle from their
    /// own outer end); `Up` is the §5b–§5s world +y test (the cell above is
    /// not WATER); `CoreAir` is the strict A/B (outward AIR or space).
    ///
    /// Landing ground follows [`RainFall`]: `Down` is world -y (an AIR cell
    /// with AIR/space above and a non-AIR cell below); `Core` (default, §5s)
    /// falls toward the core (an AIR cell whose outward step is AIR/space and
    /// whose inward support holds a non-AIR cell). A region with no landing
    /// ground and no waterline sheds its drop onto the nearest regions with
    /// ground (region-graph hops, `rain_shed`); the planet-wide spill
    /// (`rain_spilled`) is left for `Down` or a world with no ground anywhere.
    /// Every drop is a pair moved from the pool to a WATER cell: books exact.
    pub(crate) fn driven_water_pass(&mut self, topo: &Topology, rep: &mut StepReport) {
        if !self.params.rain {
            return;
        }
        let n = self.cells.len();
        let tick = self.tick;
        let seed = self.params.seed;
        // evaporation top test: world +y (§5b–§5s) or away from the core
        // (§5t, the rain frame)
        let frame = self.evap_frame(topo);
        // §5v diagnostic (outside every book): evaporation / rain split by
        // the core's world y (lower = y < core y)
        let core_y = topo.core.map(|c| self.cells[c as usize].y);
        for i in 0..n {
            if self.mat[i] != MatId::WATER || self.q[i] <= EVAP_Q {
                continue;
            }
            if !self.evap_open(topo, frame, i) {
                continue;
            }
            let odds = (EVAP_BASE + (self.q[i] - EVAP_Q) as u32 * EVAP_SLOPE).min(EVAP_MAX);
            if wide_roll(seed ^ SALT_EVAP, self.cells[i], tick) & 0xffff < odds {
                self.set_mat(i, MatId::AIR);
                self.vapor += 1;
                self.vapor_bin_add(i, 1);
                self.evaporated += 1;
                self.hemi_diag[1] += 1;
                if core_y.is_some_and(|y| self.cells[i].y < y) {
                    self.hemi_diag[0] += 1;
                }
                self.q[i] -= LATENT_PAIR; // Q > 200: never clips
                rep.evaporated += 1;
            }
        }
        // local vapor: regions exchange pairs before it rains (no-op with
        // one region)
        self.vapor_mix_pass();
        if self.vapor <= 0 {
            return;
        }
        // Rain per vapor region: each region rains its own share on its own
        // ground. One region reproduces the global-pool keys exactly
        // (fraction key (-1,-1,-1), pick keys (k,0,0)).
        if self.vapor_bins.len() != self.vgeo.as_ref().map_or(1, |g| g.rim.len())
            || self.vapor_bins.iter().sum::<i64>() != self.vapor
        {
            // not synced (pass called outside step): treat as one pool
            self.vapor_bins = vec![self.vapor];
            self.vgeo = None;
        }
        let pc = self.vapor_bins.len();
        let mut want = vec![0i64; pc];
        let mut cap = RAIN_MAX_DROPS;
        for (b, w) in want.iter_mut().enumerate() {
            let v = self.vapor_bins[b];
            if v <= 0 || cap <= 0 {
                continue;
            }
            let frac = i64::from(wide_roll(seed ^ SALT_RAIN, Coord::new(-1, -1, -1 - b as i64), tick)) % RAIN_TAU;
            let d = (v / RAIN_TAU + i64::from(frac < v % RAIN_TAU)).min(v).min(cap);
            cap -= d;
            *w = d;
        }
        if want.iter().all(|d| *d == 0) {
            return;
        }
        // candidates: sky-open AIR (+y absent or AIR) resting on non-AIR
        // (-y in the domain and not AIR); weight 1..16 by coldness
        let mut cand: Vec<Vec<(u32, u64)>> = vec![Vec::new(); pc];
        let mut total: Vec<u64> = vec![0; pc];
        // spill line (regions only): every landing spot on the planet, for
        // drops a region cannot land on its own ground (no sky-open ground,
        // e.g. the underside of the ball under the -y fall test)
        let spill_on = pc > 1;
        // fall direction: toward the core (§5s, default) or world -y (§5r)
        let core = match self.params.rain_fall {
            RainFall::Core => topo.core.map(|c| self.cells[c as usize]),
            RainFall::Down => None,
        };
        let mut all: Vec<(u32, u64)> = Vec::new();
        let mut all_total: u64 = 0;
        for i in 0..n {
            if self.mat[i] != MatId::AIR {
                continue;
            }
            let b = self.vapor_bin_of(i);
            if want[b] == 0 && !spill_on {
                continue;
            }
            let lands = if let Some(o) = core {
                // radial: open along the outward step, resting on the inward
                // support set (the core cell itself never lands)
                let p = self.cells[i];
                let s = outward_step((p.x - o.x, p.y - o.y, p.z - o.z));
                let (set, k) = Self::inward_support(o, p);
                k > 0
                    && topo
                        .offset(&self.cells, i, s)
                        .is_none_or(|u| self.mat[u] == MatId::AIR)
                    && set[..k].iter().any(|d| {
                        topo.offset(&self.cells, i, *d)
                            .is_some_and(|u| self.mat[u] != MatId::AIR)
                    })
            } else {
                let open = topo
                    .offset(&self.cells, i, (0, 1, 0))
                    .is_none_or(|u| self.mat[u] == MatId::AIR);
                let rest = topo
                    .offset(&self.cells, i, (0, -1, 0))
                    .is_some_and(|d| self.mat[d] != MatId::AIR);
                open && rest
            };
            if !lands {
                continue;
            }
            let w = 1 + ((DEW_Q - self.q[i]).clamp(0, 15 * 16) as u64 >> 4);
            // Core fall: every region's ground is listed (a dry region's drops
            // may shed onto it)
            if want[b] > 0 || (core.is_some() && spill_on) {
                total[b] += w;
                cand[b].push((i as u32, total[b]));
            }
            if spill_on {
                all_total += w;
                all.push((i as u32, all_total));
            }
        }
        if (0..pc).any(|b| want[b] > 0 && cand[b].is_empty()) {
            // no open ground in a raining region: fall back to its waterline
            // (AIR beside WATER/ICE)
            let need: Vec<bool> = (0..pc).map(|b| want[b] > 0 && cand[b].is_empty()).collect();
            for i in 0..n {
                if self.mat[i] != MatId::AIR {
                    continue;
                }
                let b = self.vapor_bin_of(i);
                if need[b]
                    && topo
                        .nb6(&self.cells, i)
                        .into_iter()
                        .flatten()
                        .any(|j| self.mat[j] == MatId::WATER || self.mat[j] == MatId::ICE)
                {
                    total[b] += 1;
                    cand[b].push((i as u32, total[b]));
                }
            }
        }
        let has: Vec<bool> = cand.iter().map(|c| !c.is_empty()).collect();
        for b in 0..pc {
            let mut drops = want[b];
            if drops == 0 || (cand[b].is_empty() && all.is_empty()) {
                continue; // humid sky: vapor waits
            }
            // Core fall, region with no ground of its own (bare rock, no AIR
            // over it): shed onto the nearest regions that have ground
            if core.is_some() && spill_on && cand[b].is_empty() {
                let (near, hops) = self.nearest_ground(b, &has);
                if !near.is_empty() {
                    let mut line: Vec<(u32, u64)> = Vec::new();
                    let mut lt: u64 = 0;
                    for &r in &near {
                        let mut prev = 0u64;
                        for &(i, cum) in &cand[r] {
                            lt += cum - prev;
                            prev = cum;
                            line.push((i, lt));
                        }
                    }
                    let mut kh: u64 = 0;
                    while drops > 0 {
                        let mut hit = None;
                        for _ in 0..4 {
                            let key = Coord::new(kh as i64, 2, b as i64);
                            let h = (u64::from(wide_roll(seed ^ SALT_RAIN, key, tick)) << 32
                                | u64::from(wide_roll(seed ^ SALT_RAIN ^ 0x0100_0000, key, tick)))
                                % lt;
                            kh += 1;
                            let slot = line.partition_point(|&(_, cum)| cum <= h);
                            let i = line[slot].0 as usize;
                            if self.mat[i] == MatId::AIR {
                                hit = Some(i);
                                break;
                            }
                        }
                        let Some(i) = hit else { break }; // the rest wait
                        self.set_mat(i, MatId::WATER);
                        self.vapor -= 1;
                        self.vapor_bins[b] -= 1;
                        self.rained += 1;
                        self.hemi_diag[3] += 1;
                        if core_y.is_some_and(|y| self.cells[i].y < y) {
                            self.hemi_diag[2] += 1;
                        }
                        self.rain_shed += 1;
                        self.rain_shed_hops += u64::from(hops);
                        drops -= 1;
                        self.q[i] = self.q[i].wrapping_add(LATENT_PAIR);
                        rep.rained += 1;
                        self.mark(i, MARK_RAIN);
                    }
                    continue;
                }
            }
            let cand = &cand[b];
            let total = total[b];
            let mut k: u64 = 0;
            let mut ks: u64 = 0;
            let mut local = !cand.is_empty();
            while drops > 0 {
                // content-addressed pick: hash(tick, drop#, region) into the
                // weight line; up to 4 probes if the pick already rained.
                // A region that cannot land a drop on its own ground spills
                // the rest of this tick's drops onto the planet's landing
                // line (keys (k, 1, region)); one region never spills.
                let mut hit = None;
                if local {
                    for _ in 0..4 {
                        let key = Coord::new(k as i64, 0, b as i64);
                        let h = (u64::from(wide_roll(seed ^ SALT_RAIN, key, tick)) << 32
                            | u64::from(wide_roll(seed ^ SALT_RAIN ^ 0x0100_0000, key, tick)))
                            % total;
                        k += 1;
                        let slot = cand.partition_point(|&(_, cum)| cum <= h);
                        let i = cand[slot].0 as usize;
                        if self.mat[i] == MatId::AIR {
                            hit = Some(i);
                            break;
                        }
                    }
                    if hit.is_none() {
                        local = false;
                    }
                }
                // Core fall: spill only from a region with no landing ground
                // and no waterline; failed probes on its own ground wait
                if hit.is_none() && spill_on && !all.is_empty() && (core.is_none() || cand.is_empty()) {
                    for _ in 0..4 {
                        let key = Coord::new(ks as i64, 1, b as i64);
                        let h = (u64::from(wide_roll(seed ^ SALT_RAIN, key, tick)) << 32
                            | u64::from(wide_roll(seed ^ SALT_RAIN ^ 0x0100_0000, key, tick)))
                            % all_total;
                        ks += 1;
                        let slot = all.partition_point(|&(_, cum)| cum <= h);
                        let i = all[slot].0 as usize;
                        if self.mat[i] == MatId::AIR {
                            hit = Some(i);
                            self.rain_spilled += 1;
                            break;
                        }
                    }
                }
                let Some(i) = hit else { break };
                self.set_mat(i, MatId::WATER);
                self.vapor -= 1;
                self.vapor_bins[b] -= 1;
                self.rained += 1;
                self.hemi_diag[3] += 1;
                if core_y.is_some_and(|y| self.cells[i].y < y) {
                    self.hemi_diag[2] += 1;
                }
                drops -= 1;
                self.q[i] = self.q[i].wrapping_add(LATENT_PAIR);
                rep.rained += 1;
                self.mark(i, MARK_RAIN);
            }
        }
    }

    /// Driven erosion: exposed ROCK/MINERAL weathers to SED beside water
    /// that moved or rained this tick, or beside a freeze/thaw.
    pub(crate) fn driven_erode_pass(&mut self, topo: &Topology, rep: &mut StepReport) {
        if !self.params.erode {
            return;
        }
        for i in 0..self.cells.len() {
            let m = self.mat[i];
            if m != MatId::ROCK && m != MatId::MIN {
                continue;
            }
            let nm = self.nb_marks(topo, i);
            if nm == 0 {
                continue;
            }
            let odds = if nm & MARK_RUN != 0 {
                ERODE_RUN
            } else if nm & MARK_PHASE != 0 {
                ERODE_FROST
            } else {
                ERODE_SLOSH
            };
            if self.roll(topo, SALT_ERODE, i) < odds {
                self.set_mat(i, MatId::SED);
                rep.eroded += 1;
            }
        }
    }

    /// Driven dissolve: [`Self::dissolve_pass`]'s claim law with the storm
    /// gate replaced by "a neighbouring water cell moved/rained this tick".
    pub(crate) fn driven_dissolve_pass(&mut self, topo: &Topology, rep: &mut StepReport) {
        if !self.params.dissolve {
            return;
        }
        let n = self.cells.len();
        let mut intents: Vec<(usize, usize)> = Vec::new();
        for i in 0..n {
            if self.mat[i] != MatId::GYP {
                continue;
            }
            let mut wet = 0u8;
            let mut air = None;
            for j in topo.nb6(&self.cells, i).into_iter().flatten() {
                if self.mat[j] == MatId::WATER {
                    wet |= self.marks[j] & (MARK_RUN | MARK_SIDE);
                } else if self.mat[j] == MatId::AIR && air.is_none() {
                    if !self.shell_blocked(j) {
                        air = Some(j);
                    }
                }
            }
            let Some(a) = air else { continue };
            if wet == 0 {
                continue;
            }
            let odds = if wet & MARK_RUN != 0 { DISSOLVE_RUN } else { DISSOLVE_SLOSH };
            if self.roll(topo, SALT_DISSOLVE, i) < odds {
                intents.push((i, a));
            }
        }
        let mut claim: std::collections::HashMap<usize, usize> = Default::default();
        for &(i, a) in &intents {
            claim.entry(a).or_insert(i);
        }
        for &(i, a) in &intents {
            if claim[&a] != i {
                continue;
            }
            self.set_mat(i, MatId::SED);
            self.set_mat(a, MatId::WATER);
            rep.dissolved += 1;
        }
    }

    /// Driven entrainment: loose SED beside running water at its own level or
    /// below is carried into that water cell (a pure swap; each water cell
    /// carries at most one grain per tick). Exposes what the sediment was
    /// armouring, so weathering can continue.
    pub(crate) fn entrain_pass(&mut self, topo: &Topology, rep: &mut StepReport) {
        if !self.params.settle {
            return;
        }
        for i in 0..self.cells.len() {
            if self.mat[i] != MatId::SED {
                continue;
            }
            let yi = self.cells[i].y;
            // §5w: "its own level or below" in r2 (one-unit tangent slack)
            let core = self.coreward() && !topo.r2.is_empty();
            let mut pick = None;
            for j in topo.nb6(&self.cells, i).into_iter().flatten() {
                let mk = self.marks[j];
                let above = if core { topo.r2[j] > topo.r2[i] + 1 } else { self.cells[j].y > yi };
                if self.mat[j] != MatId::WATER || mk & (MARK_RUN | MARK_SIDE) == 0 || above {
                    continue;
                }
                let odds = if mk & MARK_RUN != 0 { ENTRAIN_RUN } else { ENTRAIN_SLOSH };
                if pick.is_none_or(|(_, o)| odds > o) {
                    pick = Some((j, odds));
                }
            }
            let Some((j, odds)) = pick else { continue };
            if self.roll(topo, SALT_ENTRAIN, i) < odds {
                self.set_mat(j, MatId::SED);
                self.set_mat(i, MatId::WATER);
                self.marks[j] = 0; // spent
                rep.entrained += 1;
            }
        }
    }

    #[inline]
    fn fluid(&self, m: MatId) -> bool {
        m == MatId::AIR || m == MatId::WATER
    }

    /// Solid for the uplift roll: not AIR, not WATER, not a rising gas.
    #[inline]
    fn rock_like(&self, m: MatId) -> bool {
        !self.fluid(m) && !self.is_rising(m)
    }

    /// Driven dry weathering: exposed ROCK/MIN -> SED (same COMP and BIND)
    /// at [`WEATHER_AIR`] + sun-track faces x [`WEATHER_SUN`] per 65536 per
    /// tick, with no water needed.
    pub(crate) fn weather_pass(&mut self, topo: &Topology, rep: &mut StepReport) {
        if !self.params.erode {
            return;
        }
        let sun = self.params.energy_in.sun.max(0) as u32;
        let track_mask = self.params.sun_sweep.track_mask();
        let seed = self.params.seed;
        let tick = self.tick;
        // With a sky shell the sun-track proxy sees through open air: a face on
        // a sky-open AIR cell (an all-AIR column up to space) counts as lit,
        // like a face on space. No shell: planet6 space faces only.
        let sky_open: Vec<bool> = if self.sky_height > 0 {
            let mut o: Vec<bool> = self.mat.iter().map(|m| *m == MatId::AIR).collect();
            for &(i, up) in &topo.rise {
                // `rise` is ordered top-down, so `up` is final before `i`
                let v = o[i as usize] && o[up as usize];
                o[i as usize] = v;
            }
            o
        } else {
            Vec::new()
        };
        for i in 0..self.cells.len() {
            let m = self.mat[i];
            if m != MatId::ROCK && m != MatId::MIN {
                continue;
            }
            let mut open = false;
            let mut lit = 0u32;
            for (k, j) in topo.nb6(&self.cells, i).into_iter().enumerate() {
                // NB6 order: -x, +x, -y, +y, -z, +z; the sun turns in x-y
                // (Upper: -x, +x, +y; Full adds -y, and ±z with a tilt)
                let track = track_mask & (1 << k) != 0;
                match j {
                    None => {
                        open = true;
                        if track {
                            lit += 1;
                        }
                    }
                    Some(j) => {
                        open |= self.mat[j] == MatId::AIR;
                        if track && sky_open.get(j).copied().unwrap_or(false) {
                            lit += 1;
                        }
                    }
                }
            }
            if !open {
                continue;
            }
            let odds = WEATHER_AIR + WEATHER_SUN * lit * sun / 16;
            if wide_roll(seed ^ SALT_WEATHER, self.cells[i], tick) & 0xffff < odds {
                self.set_mat(i, MatId::SED);
                rep.weathered += 1;
            }
        }
    }

    /// Driven scour: SED resting on ROCK/MIN beside moving water (any face)
    /// is pushed one cell sideways into a fluid side cell (a pure swap),
    /// preferring a side with a fluid floor. Strips the sediment armour off
    /// runoff paths so the rock under them weathers again.
    pub(crate) fn scour_pass(&mut self, topo: &Topology, rep: &mut StepReport) {
        if !self.params.settle {
            return;
        }
        let seed = self.params.seed;
        let tick = self.tick;
        // §5w: "below" and "level" toward the core under Gravity::Core
        let gc = self.grav_core(topo);
        for i in 0..self.cells.len() {
            if self.mat[i] != MatId::SED {
                continue;
            }
            let below = match gc {
                Some(o) => {
                    let (set, k) = Self::inward_support(o, self.cells[i]);
                    if k == 0 {
                        continue;
                    }
                    topo.offset(&self.cells, i, set[0])
                }
                None => topo.offset(&self.cells, i, (0, -1, 0)),
            };
            let Some(b) = below else { continue };
            if self.mat[b] != MatId::ROCK && self.mat[b] != MatId::MIN {
                continue;
            }
            let mut mk = 0u8;
            for j in topo.nb6(&self.cells, i).into_iter().flatten() {
                if self.mat[j] == MatId::WATER {
                    mk |= self.marks[j] & (MARK_RUN | MARK_SIDE);
                }
            }
            if mk == 0 {
                continue;
            }
            let odds = if mk & MARK_RUN != 0 { SCOUR_RUN } else { SCOUR_SLOSH };
            let r = wide_roll(seed ^ SALT_SCOUR, self.cells[i], tick);
            if r % 1000 >= odds {
                continue;
            }
            // rotate the side order by a roll so no direction is favoured
            let mut best: Option<(usize, bool)> = None;
            if let Some(o) = gc {
                // §5w: face neighbours at most one r2 unit above, preferring
                // one whose own inward step is fluid (a drop)
                let start = ((r >> 16) % 6) as usize;
                let lim = topo.r2[i] + 1;
                for k in 0..6 {
                    let Some(s) = topo.offset(&self.cells, i, NB6[(start + k) % 6]) else { continue };
                    if topo.r2[s] > lim || !self.fluid(self.mat[s]) || self.shell_blocked(s) {
                        continue;
                    }
                    let (set, kk) = Self::inward_support(o, self.cells[s]);
                    let drop = kk > 0
                        && topo
                            .offset(&self.cells, s, set[0])
                            .is_some_and(|d| self.fluid(self.mat[d]));
                    if best.is_none_or(|(_, bd)| drop && !bd) {
                        best = Some((s, drop));
                    }
                }
            }
            let start = ((r >> 16) % 4) as usize;
            for k in 0..if gc.is_some() { 0 } else { 4 } {
                let (dx, dz) = FLOW_SIDE[(start + k) % 4];
                let Some(s) = topo.offset(&self.cells, i, (dx, 0, dz)) else { continue };
                if !self.fluid(self.mat[s]) || self.shell_blocked(s) {
                    continue;
                }
                let drop = topo
                    .offset(&self.cells, s, (0, -1, 0))
                    .is_some_and(|d| self.fluid(self.mat[d]));
                if best.is_none_or(|(_, bd)| drop && !bd) {
                    best = Some((s, drop));
                }
            }
            let Some((s, _)) = best else { continue };
            let other = self.mat[s];
            self.set_mat(s, MatId::SED);
            self.set_mat(i, other);
            self.marks[s] = 0; // spent
            rep.scoured += 1;
        }
    }

    /// Driven core-heat uplift: a heat-driven convection roll.
    ///
    /// A candidate is a column top `t` (rock-like, with AIR or WATER above,
    /// `a`) whose `UPLIFT_DEPTH` cells `t, t-1, .., h` (downward) are all
    /// rock-like. One roll picks a side; `h'` = `h` + side must be rock-like,
    /// and the neighbour column is walked up from `h'` through rock-like
    /// cells to its top `n` (whose cell above is fluid or space). If the
    /// root is hotter (`dQ = Q[h] - Q[h'] > 0`) the roll fires with odds
    /// `min(dQ, UPLIFT_DQ_MAX) * UPLIFT_K / 2^32`, provided the risen top
    /// would stand at most [`UPLIFT_MAX_STEP`] above the lowered one.
    ///
    /// Firing cyclically rotates the materials one step along the path
    /// `n -> .. -> h' -> h -> .. -> t -> a -> n`: every cell takes the
    /// material of its predecessor, so the hot column rises one cell (its top
    /// material moves into `a`, fresh material from below takes `t`), the
    /// neighbour column sinks one cell, and `a`'s old fluid fills `n`. Heat
    /// stays with the cells (Q is not moved), so lanes, BIND and Q are all
    /// unchanged: a pure permutation of materials, no atoms created.
    pub(crate) fn uplift_pass(&mut self, topo: &Topology, rep: &mut StepReport) {
        if !self.params.settle || self.params.energy_in.core <= 0 {
            return;
        }
        let seed = self.params.seed;
        let tick = self.tick;
        let mut fired = 0u32;
        let mut path: Vec<usize> = Vec::new();
        for t in 0..self.cells.len() {
            if fired >= UPLIFT_MAX_PER_TICK {
                break;
            }
            if !self.rock_like(self.mat[t]) {
                continue;
            }
            let Some(a) = topo.offset(&self.cells, t, (0, 1, 0)) else { continue };
            if !self.fluid(self.mat[a]) {
                continue;
            }
            // cheap gate first: the roll bounds the odds at UPLIFT_DQ_MAX
            let r = wide_roll(seed ^ SALT_UPLIFT, self.cells[t], tick);
            let cap = (UPLIFT_DQ_MAX as u64) * UPLIFT_K;
            if u64::from(r) >= cap {
                continue;
            }
            // target column t (top) .. h (root)
            path.clear();
            let mut h = t;
            let mut ok = true;
            for _ in 1..UPLIFT_DEPTH {
                match topo.offset(&self.cells, h, (0, -1, 0)) {
                    Some(d) if self.rock_like(self.mat[d]) => h = d,
                    _ => {
                        ok = false;
                        break;
                    }
                }
            }
            if !ok {
                continue;
            }
            let r2 = wide_roll(seed ^ SALT_UPLIFT ^ 0x0200_0000, self.cells[t], tick);
            let (dx, dz) = FLOW_SIDE[(r2 % 4) as usize];
            let Some(h2) = topo.offset(&self.cells, h, (dx, 0, dz)) else { continue };
            if !self.rock_like(self.mat[h2]) {
                continue;
            }
            // radial frame: the hot root must be heated from the core side and
            // is compared with the neighbour column at its own radius
            let Some(o) = topo.core else { continue };
            let o = self.cells[o as usize];
            if !self.heated_from_below(topo, o, h) {
                continue;
            }
            let Some(p2) = self.radial_peer(topo, o, h, h2) else { continue };
            let dq = i64::from(self.q[h]) - i64::from(self.q[p2]);
            if dq <= 0 || u64::from(r) >= (dq.min(UPLIFT_DQ_MAX) as u64) * UPLIFT_K {
                continue;
            }
            // neighbour column h2 .. n (top)
            let mut n = h2;
            let mut top_ok = false;
            for _ in 0..(UPLIFT_DEPTH + UPLIFT_MAX_STEP + 2) {
                match topo.offset(&self.cells, n, (0, 1, 0)) {
                    None => {
                        top_ok = true;
                        break;
                    }
                    Some(u) if self.fluid(self.mat[u]) => {
                        top_ok = true;
                        break;
                    }
                    Some(u) if self.rock_like(self.mat[u]) => n = u,
                    Some(_) => break, // a rising gas cell: no clean top
                }
            }
            if !top_ok {
                continue;
            }
            let (ty, ny) = (self.cells[t].y, self.cells[n].y);
            if (ty + 1) - (ny - 1) > UPLIFT_MAX_STEP {
                continue;
            }
            // path in rotation order: n, .., h2 (down), h, .., t (up), a
            let mut c = n;
            loop {
                path.push(c);
                if c == h2 {
                    break;
                }
                c = topo.offset(&self.cells, c, (0, -1, 0)).expect("walked up from h2");
            }
            let mut c = h;
            loop {
                path.push(c);
                if c == t {
                    break;
                }
                c = topo.offset(&self.cells, c, (0, 1, 0)).expect("walked down from t");
            }
            path.push(a);
            // rotate: new[path[k+1]] = old[path[k]], new[path[0]] = old[last]
            let old: Vec<MatId> = path.iter().map(|&p| self.mat[p]).collect();
            let len = path.len();
            for k in 0..len {
                let src = old[(k + len - 1) % len];
                self.set_mat(path[k], src);
            }
            fired += 1;
            rep.uplifted += 1;
        }
    }

    /// Deep convective overturn (Driven mode, `core > 0`): the longer-range
    /// relief driver that goes past the shallow uplift/scour balance.
    ///
    /// A candidate is a cold column top `t` (rock-like, surface fluid `a`
    /// above) with a rock-like column down to its root `h`, [`PLUME_DEPTH`]
    /// cells deep. One roll picks a side and a reach `L` in
    /// `PLUME_REACH_MIN..=PLUME_REACH_MAX`; the `L` root-level cells from `h`
    /// along that side must all be rock-like, ending at the hot root `h'`,
    /// whose column is walked up to its top `n` with a fluid cell `f` above it
    /// (inside the domain). With `dQ = Q[h'] - Q[h] > 0` it fires at odds
    /// `min(dQ, PLUME_DQ_MAX) * PLUME_K / 2^32` when the risen hot top stands at
    /// most `PLUME_SLOPE * L` above `t`. Firing rotates materials one step along
    /// `a -> h -> (root channel) -> h' -> .. -> n -> f -> a`:
    /// the cold root takes in the surface fluid (a downwelling that drags
    /// volatiles to depth, subduction-flavoured), the deep channel flows one
    /// cell toward the hot side, the hot column rises a cell into its fluid
    /// slot and that fluid refills the cold top. A pure permutation (Q stays
    /// with the cells), so lanes, BIND and Q are untouched.
    ///
    /// Whole-column form (since the transparent-air sun): the cold leg is the
    /// whole column `t .. h`, not just its root, so
    /// `a -> t -> .. -> h -> channel -> h' -> .. -> n -> f -> a`. The cold
    /// column subsides one cell as a block (its top cell takes in the
    /// surface fluid) while the hot column rises one cell as a block: the
    /// surface relief moves at once, instead of through a buried void that
    /// later collapses back. Three guards keep it to broad relief:
    /// [`PLUME_HEADROOM`] fluid cells above the hot slot `f` (no ceiling pin),
    /// a flank gate (both tops `t` and `n` must have `+y` in their nearest
    /// outward lattice direction, [`outward_step`], so a vertical column is
    /// a radial one and the ball's flank does not stack sideways), and the
    /// [`PLUME_REPOSE`] step bound against the neighbour tops.
    pub(crate) fn plume_pass(&mut self, topo: &Topology, rep: &mut StepReport) {
        if !self.params.settle || self.params.energy_in.core <= 0 {
            return;
        }
        let seed = self.params.seed;
        let tick = self.tick;
        let mut fired = 0u32;
        let cap = (PLUME_DQ_MAX as u64) * PLUME_K;
        let span = (PLUME_REACH_MAX - PLUME_REACH_MIN + 1) as u32;
        let mut path: Vec<usize> = Vec::new();
        let mut chan: Vec<usize> = Vec::new();
        for t in 0..self.cells.len() {
            if fired >= PLUME_MAX_PER_TICK {
                break;
            }
            if !self.rock_like(self.mat[t]) {
                continue;
            }
            let Some(a) = topo.offset(&self.cells, t, (0, 1, 0)) else { continue };
            if !self.fluid(self.mat[a]) {
                continue;
            }
            let r = wide_roll(seed ^ SALT_PLUME, self.cells[t], tick);
            if u64::from(r) >= cap {
                continue;
            }
            let mut h = t;
            let mut ok = true;
            for _ in 1..PLUME_DEPTH {
                match topo.offset(&self.cells, h, (0, -1, 0)) {
                    Some(d) if self.rock_like(self.mat[d]) => h = d,
                    _ => {
                        ok = false;
                        break;
                    }
                }
            }
            if !ok {
                continue;
            }
            let r2 = wide_roll(seed ^ SALT_PLUME ^ 0x0400_0000, self.cells[t], tick);
            let (dx, dz) = FLOW_SIDE[(r2 % 4) as usize];
            let reach = PLUME_REACH_MIN + i64::from((r2 >> 8) % span);
            // root channel h+1 .. h+L (h' = last), all rock-like
            chan.clear();
            let mut c = h;
            for _ in 0..reach {
                match topo.offset(&self.cells, c, (dx, 0, dz)) {
                    Some(s) if self.rock_like(self.mat[s]) => {
                        chan.push(s);
                        c = s;
                    }
                    _ => {
                        ok = false;
                        break;
                    }
                }
            }
            if !ok {
                continue;
            }
            let h2 = c;
            // radial frame: the hot root must be heated from the core side and
            // is compared with the cold column at its own radius
            let Some(o) = topo.core else { continue };
            let o = self.cells[o as usize];
            if !self.heated_from_below(topo, o, h2) {
                continue;
            }
            let Some(p) = self.radial_peer(topo, o, h2, h) else { continue };
            let dq = i64::from(self.q[h2]) - i64::from(self.q[p]);
            if dq <= 0 || u64::from(r) >= (dq.min(PLUME_DQ_MAX) as u64) * PLUME_K {
                continue;
            }
            // hot column h2 .. n (top), fluid slot f above n
            let mut n = h2;
            let mut slot = None;
            for _ in 0..(PLUME_DEPTH + PLUME_SLOPE * reach + 2) {
                match topo.offset(&self.cells, n, (0, 1, 0)) {
                    Some(u) if self.fluid(self.mat[u]) => {
                        slot = Some(u);
                        break;
                    }
                    Some(u) if self.rock_like(self.mat[u]) => n = u,
                    _ => break,
                }
            }
            let Some(f) = slot else { continue };
            if (self.cells[n].y + 1) - self.cells[t].y > PLUME_SLOPE * reach {
                continue;
            }
            // headroom above the hot slot
            let mut hd = f;
            let mut room = true;
            for _ in 0..PLUME_HEADROOM {
                match topo.offset(&self.cells, hd, (0, 1, 0)) {
                    Some(u) if self.fluid(self.mat[u]) => hd = u,
                    _ => {
                        room = false;
                        break;
                    }
                }
            }
            if !room {
                continue;
            }
            // flank gate: both columns run along the local outward direction
            let up = |c: Coord| outward_step((c.x - o.x, c.y - o.y, c.z - o.z)).1 == 1;
            if !up(self.cells[t]) || !up(self.cells[n]) {
                continue;
            }
            // repose: no shaft at the sunk cold top, no spire at the risen hot top
            let (ty, ny) = (self.cells[t].y - 1, self.cells[n].y + 1);
            if !self.within_repose(topo, t, |yn| yn - ty) || !self.within_repose(topo, n, |yn| ny - yn) {
                continue;
            }
            // path in rotation order: a, t .. h (whole cold column), channel .. h2, up to n, f
            path.clear();
            path.push(a);
            let mut c = t;
            loop {
                path.push(c);
                if c == h {
                    break;
                }
                c = topo.offset(&self.cells, c, (0, -1, 0)).expect("walked down from t");
            }
            path.extend_from_slice(&chan);
            let mut c = h2;
            while c != n {
                c = topo.offset(&self.cells, c, (0, 1, 0)).expect("walked down from n");
                path.push(c);
            }
            path.push(f);
            let old: Vec<MatId> = path.iter().map(|&p| self.mat[p]).collect();
            let len = path.len();
            for k in 0..len {
                self.set_mat(path[k], old[(k + len - 1) % len]);
            }
            fired += 1;
            rep.plumed += 1;
        }
    }

    /// [`PLUME_REPOSE`] check around column top `c`: every in-domain lateral
    /// neighbour column must have a rock-like top within `PLUME_REPOSE + 2`
    /// cells of `c`'s level, and `gap(neighbour top y)` (the step the move
    /// would leave) must not exceed `PLUME_REPOSE`. A lateral neighbour off
    /// the domain edge does not count.
    fn within_repose(&self, topo: &Topology, c: usize, gap: impl Fn(i64) -> i64) -> bool {
        for (sx, sz) in FLOW_SIDE {
            if topo.offset(&self.cells, c, (sx, 0, sz)).is_none() {
                continue;
            }
            match self.col_top_near(topo, c, sx, sz, PLUME_REPOSE + 2) {
                Some(y) if gap(y) <= PLUME_REPOSE => {}
                _ => return false,
            }
        }
        true
    }

    /// `y` of the rock-like top of the column beside `c` (lateral offset
    /// `sx, sz`), searched at most `w` cells up (from a rock-like start) or
    /// down (from a fluid start); `None` when outside that window, when the
    /// walk meets a rising gas cell, or when the neighbour is off the domain.
    fn col_top_near(&self, topo: &Topology, c: usize, sx: i64, sz: i64, w: i64) -> Option<i64> {
        let mut u = topo.offset(&self.cells, c, (sx, 0, sz))?;
        if self.rock_like(self.mat[u]) {
            for _ in 0..w {
                match topo.offset(&self.cells, u, (0, 1, 0)) {
                    Some(v) if self.rock_like(self.mat[v]) => u = v,
                    Some(v) if self.fluid(self.mat[v]) => return Some(self.cells[u].y),
                    None => return Some(self.cells[u].y),
                    _ => return None,
                }
            }
            None
        } else {
            for _ in 0..w {
                match topo.offset(&self.cells, u, (0, -1, 0)) {
                    Some(v) if self.rock_like(self.mat[v]) => return Some(self.cells[v].y),
                    Some(v) if self.fluid(self.mat[v]) => u = v,
                    _ => return None,
                }
            }
            None
        }
    }

    /// Radial-frame peer of the hot root `hot` in the column of `col` (same
    /// x, z as `col`): the rock-like cell of that column whose squared
    /// distance from the core `o` is nearest `|hot - o|^2` (ties: the lower
    /// cell), within [`RADIAL_PEER_SPAN`] cells of `col`'s level, staying on
    /// `col`'s side of the core's y. `None` when that column never comes
    /// within reach of the radius or the matched cell is not rock-like.
    ///
    /// Uplift and plume compare `Q` along the planet's local outward
    /// direction rather than at a common world `y`: on a ball a horizontal
    /// slice cuts across the radial isotherms, so the more central root at a
    /// given `y` always reads hotter (that bias stacked the hotspot dome
    /// under the pole, and a bare "hot root not more central" veto just moved
    /// the pin to the sun-warmed rim). Matching radii cancels the radial
    /// background in both directions, so only lateral contrasts drive.
    pub(crate) fn radial_peer(&self, topo: &Topology, o: Coord, hot: usize, col: usize) -> Option<usize> {
        let d2 = |c: Coord| (c.x - o.x).pow(2) + (c.y - o.y).pow(2) + (c.z - o.z).pow(2);
        let target = d2(self.cells[hot]);
        let c = self.cells[col];
        let flat = (c.x - o.x).pow(2) + (c.z - o.z).pow(2);
        let want = target - flat; // (y' - o.y)^2 wanted
        if want < 0 {
            return None;
        }
        let mut k = want.isqrt();
        if (k + 1) * (k + 1) - want < want - k * k {
            k += 1;
        }
        let y = if c.y >= o.y { o.y + k } else { o.y - k };
        let dy = y - c.y;
        if dy.abs() > RADIAL_PEER_SPAN {
            return None;
        }
        let p = topo.offset(&self.cells, col, (0, dy, 0))?;
        self.rock_like(self.mat[p]).then_some(p)
    }

    /// True when cell `c` is a buried root heated from the core side: its
    /// outward neighbour is in the domain, rock-like (radial overburden), and
    /// no hotter than `c`. A root whose outward cell is space or fluid sits on
    /// the ball's flank, where a vertical column runs along the surface
    /// rather than down into it; a root colder than its overburden is heated
    /// from outside (the rim under the sun-warmed sky shell). Neither drives
    /// uplift or plumes. The core cell itself counts as heated.
    ///
    /// The outward neighbour is the nearest of the 26 lattice directions to
    /// `c - o` (see [`outward_step`]), not just the dominant axis: near the
    /// 45 degree diagonals and below the equator the single-axis step lands
    /// on buried rock while the true outward cell is already space or fluid,
    /// which let flank roots keep lifting the rim to the shell ceiling.
    pub(crate) fn heated_from_below(&self, topo: &Topology, o: Coord, c: usize) -> bool {
        let p = self.cells[c];
        let step = outward_step((p.x - o.x, p.y - o.y, p.z - o.z));
        if step == (0, 0, 0) {
            return true; // the core cell itself
        }
        match topo.offset(&self.cells, c, step) {
            Some(u) => self.rock_like(self.mat[u]) && self.q[c] >= self.q[u],
            None => false,
        }
    }

    /// Hydrothermal degassing (Driven mode, `rain` on and `core > 0`): the
    /// return leg for the volatiles the plume drags to depth.
    ///
    /// A candidate is a WATER cell that surface evaporation cannot reach
    /// (the cell above is also WATER: it is submerged in a pool or a buried
    /// aquifer) resting on a rock-like bed (the cell below is in the domain
    /// and rock-like, so the crust heats it), with `Q > DEGAS_Q`. It boils
    /// with odds `min((Q - DEGAS_Q) * DEGAS_SLOPE, DEGAS_MAX) / 65536` per tick,
    /// at most [`DEGAS_MAX_PER_TICK`] per tick: WATER -> AIR, one pair into the
    /// vapor pool, `LATENT_PAIR` paid out of the cell's heat. That is exactly
    /// evaporation's bookkeeping (lanes: a pair is 4 H + 2 O in either place;
    /// energy: `BIND[WATER] + Q` becomes `LATENT_PAIR - BIND[WATER]` of vapor
    /// plus `Q - LATENT_PAIR` left in the cell; `Q > DEGAS_Q > LATENT_PAIR`, so
    /// no clip). The bubble rises through its water column in the next flow
    /// pass (water falls into the AIR below it), so the gas vents to the pool
    /// top and the vapor comes back down as rain on open ground.
    pub(crate) fn degas_pass(&mut self, topo: &Topology, rep: &mut StepReport) {
        if !self.params.rain || self.params.energy_in.core <= 0 {
            return;
        }
        let gc = self.grav_core(topo);
        let seed = self.params.seed;
        let tick = self.tick;
        let mut fired = 0u32;
        for i in 0..self.cells.len() {
            if fired >= DEGAS_MAX_PER_TICK {
                break;
            }
            if self.mat[i] != MatId::WATER || self.q[i] <= DEGAS_Q {
                continue;
            }
            let odds = ((self.q[i] - DEGAS_Q) as u32).saturating_mul(DEGAS_SLOPE).min(DEGAS_MAX);
            if wide_roll(seed ^ SALT_DEGAS, self.cells[i], tick) & 0xffff >= odds {
                continue;
            }
            // §5w: "above" / "bed" along the outward / inward step under Gravity::Core
            let (up, dn) = match gc {
                Some(o) => {
                    let p = self.cells[i];
                    let st = outward_step((p.x - o.x, p.y - o.y, p.z - o.z));
                    if st == (0, 0, 0) {
                        continue;
                    }
                    (st, (-st.0, -st.1, -st.2))
                }
                None => ((0, 1, 0), (0, -1, 0)),
            };
            let submerged = topo
                .offset(&self.cells, i, up)
                .is_some_and(|u| self.mat[u] == MatId::WATER);
            let bed = topo
                .offset(&self.cells, i, dn)
                .is_some_and(|d| self.rock_like(self.mat[d]));
            if !(submerged && bed) {
                continue;
            }
            self.set_mat(i, MatId::AIR);
            self.vapor += 1;
            self.vapor_bin_add(i, 1);
            self.q[i] -= LATENT_PAIR; // Q > DEGAS_Q > LATENT_PAIR: never clips
            fired += 1;
            rep.degassed += 1;
        }
    }

    /// Inward support set of cell `c` for [`Self::collapse_pass`]: the
    /// inward lattice step `-outward_step(c - o)` and, when that step is a
    /// diagonal, each of its single-axis components. A cell rests on any
    /// rock-like cell of the set (a diagonal staircase is held by its treads);
    /// the first entry is the fall target. Empty at the core.
    pub(crate) fn inward_support(o: Coord, p: Coord) -> ([(i64, i64, i64); 4], usize) {
        let s = outward_step((p.x - o.x, p.y - o.y, p.z - o.z));
        let d = (-s.0, -s.1, -s.2);
        let mut out = [(0, 0, 0); 4];
        if d == (0, 0, 0) {
            return (out, 0);
        }
        out[0] = d;
        let mut n = 1;
        let axes = [(d.0, 0, 0), (0, d.1, 0), (0, 0, d.2)];
        let nz = axes.iter().filter(|a| **a != (0, 0, 0)).count();
        if nz > 1 {
            for a in axes {
                if a != (0, 0, 0) {
                    out[n] = a;
                    n += 1;
                }
            }
        }
        (out, n)
    }

    /// Driven void collapse: gravity toward the planet's core for solids.
    ///
    /// A rock-like cell `c` is unsupported when every cell of its inward
    /// support set ([`Self::inward_support`]: the nearest-lattice inward step
    /// toward the core cell, plus that step's axis components on a diagonal)
    /// is AIR (a dry void or an overhang; out-of-domain or any other material
    /// supports, and water-filled pores are held up by their water). It then
    /// drops one cell along the inward step with odds [`COLLAPSE_ODDS`]/65536
    /// per tick: a pure swap of materials (`c` takes the AIR, the target takes
    /// the solid; Q stays with the cells), so lanes, BIND and Q are unchanged.
    /// Each cell moves at most once per tick, so a void migrates one cell
    /// outward per firing and buried pores work their way up to the surface,
    /// where the column above them has compacted by the void's volume.
    ///
    /// This closes the deep AIR pores the plume law drags under cold columns
    /// (cold land subsides instead of standing on foam) and drops overhangs.
    /// The core cell itself never falls, and only cells whose inward step has a
    /// downward (-y) component fall (the upper half-ball and its flanks): the
    /// planet6 `settle_pass` pulls SED along -y everywhere, and a radial pull
    /// against it below the equator would bounce grains between the two.
    pub(crate) fn collapse_pass(&mut self, topo: &Topology, rep: &mut StepReport) {
        if !self.params.settle {
            return;
        }
        let Some(o) = topo.core else { return };
        let o = self.cells[o as usize];
        let coreward = self.coreward();
        let seed = self.params.seed;
        let tick = self.tick;
        let n = self.cells.len();
        let mut moved = vec![false; n];
        for c in 0..n {
            if moved[c] || !self.rock_like(self.mat[c]) {
                continue;
            }
            let (set, k) = Self::inward_support(o, self.cells[c]);
            // only where the inward step also points down (-y): never against
            // world gravity, so it cannot ping-pong with `settle_pass` (every
            // solid move lowers y) and the underside of the ball keeps hanging
            // §5w: under Gravity::Core settle also pulls toward the core, so
            // the -y gate is lifted and the underside compacts too
            if k == 0 || (!coreward && set[0].1 >= 0) {
                continue;
            }
            let mut target = None;
            let mut held = false;
            for (q, d) in set[..k].iter().enumerate() {
                match topo.offset(&self.cells, c, *d) {
                    Some(u) if self.mat[u] == MatId::AIR => {
                        if q == 0 {
                            target = Some(u);
                        }
                    }
                    _ => {
                        held = true;
                        break;
                    }
                }
            }
            if held {
                continue;
            }
            let Some(u) = target else { continue };
            if moved[u] {
                continue; // vacated this tick: one cell of void travel per tick
            }
            if wide_roll(seed ^ SALT_COLLAPSE, self.cells[c], tick) & 0xffff >= COLLAPSE_ODDS {
                continue;
            }
            let m = self.mat[c];
            self.set_mat(u, m);
            self.set_mat(c, MatId::AIR);
            moved[u] = true;
            moved[c] = true;
            rep.collapsed += 1;
        }
    }

    fn storm(&self) -> bool {
        self.tick % SEASON < STORM_LEN
    }

    /// planet6 `stepWaterCycle`: evaporation, then rain in the rain window.
    pub(crate) fn water_cycle_pass(&mut self, topo: &Topology, rep: &mut StepReport) {
        if !self.params.rain {
            return;
        }
        let n = self.cells.len();
        let tkey = self.tick >> 3;
        for i in 0..n {
            if self.mat[i] != MatId::WATER {
                continue;
            }
            if let Some(up) = topo.offset(&self.cells, i, (0, 1, 0)) {
                if self.mat[up] == MatId::WATER {
                    continue; // buried: not surface
                }
            }
            if self.q[i] > 200 && self.roll_at(topo, SALT_EVAP, i, tkey) < 8 {
                self.set_mat(i, MatId::AIR);
                self.vapor += 1;
                self.evaporated += 1;
                self.q[i] = (self.q[i] - LATENT_PAIR).max(0);
                rep.evaporated += 1;
            }
        }
        let phase = self.tick % SEASON;
        let raining = self.vapor > 0 && (RAIN_START..RAIN_END).contains(&phase);
        if !raining {
            return;
        }
        let mut drops = self.vapor.min(DROPS);
        // each drop rescans from the first cell (planet6's restart loop)
        while drops > 0 {
            let hit = (0..n).find(|&i| {
                self.mat[i] == MatId::AIR
                    && topo
                        .nb6(&self.cells, i)
                        .into_iter()
                        .flatten()
                        .any(|j| self.mat[j] == MatId::WATER)
            });
            let Some(i) = hit else {
                break; // no waterline: vapor waits (humid sky)
            };
            self.set_mat(i, MatId::WATER);
            self.vapor -= 1;
            self.rained += 1;
            drops -= 1;
            self.q[i] = self.q[i].wrapping_add(LATENT_PAIR);
            rep.rained += 1;
        }
    }

    /// planet6 `stepErode`: in storms, exposed ROCK/MINERAL weathers to SED.
    pub(crate) fn erode_pass(&mut self, topo: &Topology, rep: &mut StepReport) {
        if !self.params.erode || !self.storm() {
            return;
        }
        for i in 0..self.cells.len() {
            let m = self.mat[i];
            if m != MatId::ROCK && m != MatId::MIN {
                continue;
            }
            let exposed = topo.nb6(&self.cells, i).into_iter().any(|j| match j {
                None => true,
                Some(j) => self.mat[j] == MatId::AIR || self.mat[j] == MatId::WATER,
            });
            if exposed && self.roll(topo, 0, i) < 10 {
                self.set_mat(i, MatId::SED);
                rep.eroded += 1;
            }
        }
    }

    /// planet6 `stepDissolve` (LAW v5.1, one claim round): in storms, wet
    /// GYPSUM with an AIR pocket collapses to SED and fills the pocket with
    /// its crystallisation water. Intents read the snapshot; the lowest
    /// domain index claims a contested pocket.
    pub(crate) fn dissolve_pass(&mut self, topo: &Topology, rep: &mut StepReport) {
        if !self.params.dissolve || !self.storm() {
            return;
        }
        let n = self.cells.len();
        // pass A - intent (pure snapshot reads)
        let mut intents: Vec<(usize, usize)> = Vec::new();
        for i in 0..n {
            if self.mat[i] != MatId::GYP {
                continue;
            }
            let mut wet = false;
            let mut air = None;
            for j in topo.nb6(&self.cells, i).into_iter().flatten() {
                if self.mat[j] == MatId::WATER {
                    wet = true;
                } else if self.mat[j] == MatId::AIR && air.is_none() {
                    air = Some(j);
                }
            }
            if let (true, Some(a)) = (wet, air) {
                if self.roll(topo, SALT_DISSOLVE, i) < 60 {
                    intents.push((i, a));
                }
            }
        }
        if intents.is_empty() {
            return;
        }
        // pass B - claim the pocket (serial min == atomicMin); intents are
        // in ascending i, so the first claimant of a pocket wins
        let mut claim: std::collections::HashMap<usize, usize> = Default::default();
        for &(i, a) in &intents {
            claim.entry(a).or_insert(i);
        }
        // pass C - commit iff the claim holds
        for &(i, a) in &intents {
            if claim[&a] != i {
                continue;
            }
            self.set_mat(i, MatId::SED);
            self.set_mat(a, MatId::WATER);
            rep.dissolved += 1;
        }
    }

    /// planet6 `stepSettle`: bottom-up, SED sinks through AIR/WATER by one
    /// cell (the displaced fluid moves up). Bedrock-class solids hold.
    pub(crate) fn settle_pass(&mut self, topo: &Topology, rep: &mut StepReport) {
        if !self.params.settle {
            return;
        }
        if let Some(o) = self.grav_core(topo) {
            self.settle_core(topo, rep, o);
            return;
        }
        for &i in &topo.ysort {
            let i = i as usize;
            if self.mat[i] != MatId::SED {
                continue;
            }
            let Some(j) = topo.offset(&self.cells, i, (0, -1, 0)) else {
                continue;
            };
            let below = self.mat[j];
            if (below == MatId::AIR || below == MatId::WATER) && !self.shell_blocked(j) {
                self.set_mat(j, MatId::SED);
                self.set_mat(i, below);
                rep.settled += 1;
            }
        }
    }

    /// planet6 `stepFlow`: bottom-up, WATER falls into AIR below, else moves
    /// into the first AIR side cell in `-z, -x, +z, +x` order.
    pub(crate) fn flow_pass(&mut self, topo: &Topology, rep: &mut StepReport) {
        if !self.params.flow {
            return;
        }
        if let Some(o) = self.grav_core(topo) {
            self.flow_core(topo, rep, o);
            return;
        }
        for &i in &topo.ysort {
            let i = i as usize;
            if self.mat[i] != MatId::WATER {
                continue;
            }
            let down = topo
                .offset(&self.cells, i, (0, -1, 0))
                .filter(|&b| self.mat[b] == MatId::AIR && !self.shell_blocked(b));
            let target = down.or_else(|| {
                FLOW_SIDE.iter().find_map(|&(dx, dz)| {
                    topo.offset(&self.cells, i, (dx, 0, dz))
                        .filter(|&s| self.mat[s] == MatId::AIR && !self.shell_blocked(s))
                })
            });
            if let Some(t) = target {
                self.set_mat(t, MatId::WATER);
                self.set_mat(i, MatId::AIR);
                rep.flowed += 1;
                self.mark(t, if down.is_some() { MARK_FALL } else { MARK_SIDE });
            }
        }
    }

    /// §5w coreward settle: inner cells first, a SED grain sinks one cell
    /// along its inward step when its whole inward support set is AIR/WATER
    /// in the domain (the planet6 `-y` rule, turned radial: a grain on a
    /// diagonal staircase is held by any solid tread); the displaced fluid
    /// moves out. A pure swap.
    fn settle_core(&mut self, topo: &Topology, rep: &mut StepReport, o: Coord) {
        for &i in &topo.rsort {
            let i = i as usize;
            if self.mat[i] != MatId::SED {
                continue;
            }
            let (set, k) = Self::inward_support(o, self.cells[i]);
            let mut target = None;
            let mut held = k == 0;
            for (q, d) in set[..k].iter().enumerate() {
                match topo.offset(&self.cells, i, *d) {
                    Some(u) if self.fluid(self.mat[u]) => {
                        if q == 0 {
                            target = Some(u);
                        }
                    }
                    _ => {
                        held = true;
                        break;
                    }
                }
            }
            if held {
                continue;
            }
            let Some(j) = target else { continue };
            if self.shell_blocked(j) {
                continue;
            }
            let below = self.mat[j];
            self.set_mat(j, MatId::SED);
            self.set_mat(i, below);
            rep.settled += 1;
        }
    }

    /// §5w coreward flow: inner cells first, each WATER cell moves at most
    /// once per tick. It falls into the AIR of its inward support set
    /// ([`Self::inward_target`]), else spreads to the lowest AIR face
    /// neighbour no more than one `r2` unit above it ([`Self::side_target`]).
    /// A pure swap; marks as `stepFlow` (fall / side).
    fn flow_core(&mut self, topo: &Topology, rep: &mut StepReport, o: Coord) {
        let n = self.cells.len();
        let seed = self.params.seed;
        let tick = self.tick;
        let mut moved = vec![false; n];
        for &i in &topo.rsort {
            let i = i as usize;
            if self.mat[i] != MatId::WATER || moved[i] {
                continue;
            }
            let down = self.inward_target(topo, o, i, |m| m == MatId::AIR);
            let target = down.or_else(|| {
                let r = wide_roll(seed ^ SALT_FLOW, self.cells[i], tick);
                self.side_target(topo, i, r, |m| m == MatId::AIR)
            });
            if let Some(t) = target {
                self.set_mat(t, MatId::WATER);
                self.set_mat(i, MatId::AIR);
                moved[t] = true;
                moved[i] = true;
                rep.flowed += 1;
                self.mark(t, if down.is_some() { MARK_FALL } else { MARK_SIDE });
            }
        }
    }
}
