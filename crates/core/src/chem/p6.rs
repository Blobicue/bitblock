//! The planet6 tick on a [`ChemWorld`]: `stepHeat`, `stepPhase`,
//! `stepCombine` and the `stepCombine2` stage (hand fusion families, the
//! generic network law v5, `netBurn`, condensation, gas rise), then the
//! surface laws `stepWaterCycle`, `stepErode`, `stepDissolve`, `stepSettle`
//! and `stepFlow` (in [`super::p6_rest`]), ported bit-for-bit from
//! `games/planet/core.js` in integer arithmetic.
//!
//! Cell visiting order is the domain order (sorted [`Coord`], which is
//! planet6's `idx = (x*G + y)*G + z` order on its grid). Neighbour orders are
//! planet6's: 6-neighbours `-x,+x,-y,+y,-z,+z`; 26- and +-2 neighbourhoods
//! `dx`, then `dy`, then `dz`, ascending. A neighbour "exists" iff it is a
//! domain cell (planet6 `MASK`). Rolls use [`lattice`] keyed by
//! [`cell_key`] (= planet6 `idx` on its 48^3 grid, wrapping elsewhere).

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use super::vapor::{EvapTop, Gravity, RainFall, VaporLocality};
use super::{ChemWorld, MatId};
use crate::world::Coord;

/// planet6 `SL_FUSE`, `FREEZE_AT`, `MELT_AT`.
pub const SL_FUSE: i32 = 8;
pub const FREEZE_AT: i32 = 12;
pub const MELT_AT: i32 = 34;
/// Rounds the v5 claim-rule protocol may take per tick before it is declared stuck.
pub const FIRE_MAX_ROUNDS: u32 = 32;
/// planet6's grid side; [`cell_key`] uses it so rolls match planet6 on its grid.
pub const P6_GRID: i64 = 48;

/// How the 0-D greenhouse scales the surface leak.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Greenhouse {
    /// No greenhouse (`P.greenhouse === false`).
    Off,
    /// planet6 bit-for-bit: `leak = trunc(leak * (1 / (1 + co2n / 300)))` in
    /// IEEE-754 doubles. Only correctly rounded `/ + *` are used, so the
    /// result is identical on every conforming platform (deterministic), but
    /// it is the one float expression in the tick.
    Planet6,
    /// Exact rational: `leak = trunc(leak * 300 / (300 + co2n))`. All-integer;
    /// may differ from planet6 by one unit of leak on some ticks.
    Rational,
}

/// M1 booked energy input (BitBlock-only: planet6 has no inbound energy, so a
/// planet6 world cools to heat death). **Off by default** ([`EnergyInput::OFF`]),
/// and when off the tick is bit-for-bit planet6.
///
/// When on, every tick (before `stepHeat`) adds integer heat to cell `Q` and
/// books exactly the same amount in the inbound ledger
/// [`ChemWorld::input_q`](super::ChemWorld::input_q), which
/// [`ChemWorld::energy_total`](super::ChemWorld::energy_total) subtracts. So the
/// energy book stays closed: `sum(Q) - sum(BIND) + vapor*(LATENT-BIND_W) +
/// space_q - input_q == t0_energy` every tick, i.e.
/// `E_world(t) = E_world(t0) + input_q - space_q`.
///
/// Two sources, both pure functions of (topology, tick):
/// * **sun** — a directional light rotating about the z axis once per `day`
///   ticks, `s = (cos, sin, 0)` as integer triangle waves in `[-1024, 1024]`.
///   It is day while `s.y > 0` (the default [`SunSweep::Upper`]; under
///   [`SunSweep::Full`] there is no global night: the sun always shines and
///   every face with `d.s > 0` is lit, optionally with a seasonal `s.z`
///   tilt); each domain cell face that borders space (no
///   domain neighbour in that direction `d`) receives `sun * max(0, d.s) / 1024`
///   per tick. No shadowing (the outer rim is the absorber). Under
///   [`WaterCycle::Driven`] AIR is transparent: each lit face's share is carried
///   inward along the face normal through AIR cells and lands on the first
///   non-AIR cell (see [`ChemWorld::sun_target`](super::ChemWorld::sun_target));
///   a ray that crosses only AIR leaves the domain unbooked. **Albedo**
///   (Driven only): the first non-AIR cell keeps only `1 - albedo` of the
///   ray ([`albedo_at`]: ICE 0.60; WATER angle-dependent per [`WaterAlbedo`],
///   Fresnel-like by default, 0.06 at high sun rising to 1.0 at grazing
///   incidence, or the fixed 0.15 of [`albedo`]; everything else 0); the
///   reflected share goes back out to space and is never injected, so it is
///   in no book (it is only counted in
///   [`ChemWorld::reflected_q`](super::ChemWorld::reflected_q) and, for water,
///   [`ChemWorld::water_sun_q`](super::ChemWorld::water_sun_q)).
///   Classic keeps the
///   rim absorber bit-for-bit. `day == 0` pins
///   the sun at zenith (`s = (0, 1024, 0)`). Day/night, warm sun-facing slopes
///   and cold ±z / underside caps come from geometry, not from a calendar.
/// * **core** — radiogenic/mantle heat: `core` per tick into the core of the
///   ball. The centre cell is the domain cell nearest the domain's
///   bounding-box centre (pinned to the pre-shell centre by a sky shell).
///   With `core_r == 0` (the default, and [`EnergyInput::M1`]) all of it lands
///   on that one cell, bit-for-bit the original M1. With `core_r > 0` the same
///   `core` per tick is spread over the **core sphere**: every domain cell
///   whose squared distance from the centre cell is `<= core_r^2`, in domain
///   index order (`n` cells). Each gets `core / n`; the `core % n` remainder
///   units go one each to the cells `k` with `(k + tick) % n < core % n`, a
///   window that rotates with the tick so every cell receives the same total
///   over `n` ticks. Integer exact: the per-tick total is still `core`.
///
/// A deposit that would overflow the cell's `i32` is skipped (and not booked),
/// so the ledger only ever records heat that actually landed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EnergyInput {
    /// Peak insolation per space-facing face per tick (0 = no sun).
    pub sun: i32,
    /// Day length in ticks (0 = sun fixed at zenith).
    pub day: u32,
    /// Core heat per tick (0 = off).
    pub core: i32,
    /// Core-sphere radius in cells (0 = all core heat into the single centre
    /// cell, the original M1).
    pub core_r: u32,
}

/// Which part of the sphere the M1 sun lights (BitBlock-only, §5v).
///
/// * [`SunSweep::Upper`] (default) is the §5b–§5u sun bit-for-bit: the sun
///   turns in the x–y plane and only shines while `s.y > 0`. Every face with a
///   `-y` normal is never lit and, with a radial sky shell, the `y < core`
///   hemisphere is permanent night.
/// * [`SunSweep::Full`] treats the sun as a distant source at all times: no
///   `s.y > 0` gate, so at every tick the faces with `d.s > 0` are lit (the
///   half of the sphere facing the sun) and over one day every x–y longitude
///   (including `-y`) gets noon. `tilt > 0` (1/1024ths, capped at 1024) adds
///   a seasonal axial tilt: `s.z = tilt * tri_sin(year phase) / 1024` over a
///   year of `year` days, with `s.x, s.y` scaled by `(1024 - |s.z|) / 1024`
///   so `|s|_1` stays 1024 (the same total sun on a closed surface); `tilt =
///   0` or `year = 0` keeps `s.z = 0` (equinox: the ±z caps stay unlit).
///
/// Bookkeeping is unchanged in both modes: each face share `sun * d / 1024`
/// is deposited (minus cloud and surface albedo, never injected) and the same
/// amount is booked in `input_q`. Note `Full` roughly doubles the day-mean
/// sun of `Upper` at the same `sun` (no half-day of night).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SunSweep {
    #[default]
    Upper,
    Full {
        tilt: u16,
        year: u32,
    },
}

impl SunSweep {
    /// Full-sphere sun, no tilt (equinox).
    pub const FULL: Self = Self::Full { tilt: 0, year: 0 };

    /// The `NB6` faces (bit `k`) this sun can ever light: `-x, +x, +y` for
    /// `Upper`; adds `-y` for `Full`, and `±z` with a tilt.
    pub const fn track_mask(self) -> u8 {
        match self {
            Self::Upper => 0b00_1011,
            Self::Full { tilt, year } if tilt > 0 && year > 0 => 0b11_1111,
            Self::Full { .. } => 0b00_1111,
        }
    }
}

/// Albedo denominator: [`albedo`] is in 1/1024ths.
pub const ALBEDO_DEN: i64 = 1024;
/// Snow/ice albedo (~0.60; fresh snow is 0.8-0.9, bare sea ice 0.5-0.7).
pub const ALBEDO_ICE: i64 = 614;
/// Fixed open-water albedo (~0.15: ~0.06 at high sun rising steeply at grazing
/// incidence; a day-and-angle average of the low-sun-heavy rim faces). Used
/// by [`WaterAlbedo::Fixed`]; the default [`WaterAlbedo::FRESNEL`] varies
/// water's albedo with the incidence angle instead ([`water_albedo`]).
pub const ALBEDO_WATER: i64 = 154;

/// Angle-free fraction of incoming sunlight (out of [`ALBEDO_DEN`]) a cell of `m`
/// reflects straight back to space under the Driven transparent-air sun (the
/// sun pass uses [`albedo_at`], which swaps in [`water_albedo`] for WATER). ICE
/// reflects most, WATER some; rock, sediment, minerals and gases absorb all
/// (dark land ~0.1-0.3 in reality, rounded to 0 so land heating is the
/// unchanged §5l baseline).
pub fn albedo(m: MatId) -> i64 {
    match m {
        MatId::ICE => ALBEDO_ICE,
        MatId::WATER => ALBEDO_WATER,
        _ => 0,
    }
}

/// How open water's albedo depends on the sun's incidence angle (Driven sun
/// only; ICE and every other material keep the fixed [`albedo`]).
///
/// * [`WaterAlbedo::Fixed`]: [`ALBEDO_WATER`] (0.15) at every angle, the §5n /
///   §5o behaviour bit-for-bit.
/// * [`WaterAlbedo::Fresnel`] (default): a Schlick-style curve in the cosine
///   of incidence `mu` (1/1024ths) on the face the ray enters,
///   `a(mu) = nadir + (graze - nadir) * (1 - mu)^pow`. `mu` is the entry
///   face's outward normal dotted with the **unit** sun direction (see
///   [`water_albedo`]); because Driven fluids settle under world `-y` gravity,
///   a water surface's free face is axis-aligned (a pond top is a `+y` face),
///   so the entry face normal *is* the water surface normal and `mu` on a pond
///   top is the sine of the sun's elevation. All-integer and deterministic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaterAlbedo {
    Fixed,
    Fresnel {
        /// Albedo at normal incidence (`mu = 1`), 1/1024ths.
        nadir: u16,
        /// Albedo at grazing incidence (`mu -> 0`), 1/1024ths (<= 1024).
        graze: u16,
        /// Exponent on `(1 - mu)` (Schlick uses 5).
        pow: u8,
    },
}

/// Water albedo at normal incidence (~0.06: calm open ocean at high sun is
/// ~0.03 specular plus a little diffuse/whitecap; 0.06 is the usual bulk value).
pub const WATER_ALBEDO_NADIR: u16 = 61;
/// Water albedo at grazing incidence (1.0: Fresnel reflectance -> 1 as the
/// angle of incidence -> 90 degrees).
pub const WATER_ALBEDO_GRAZE: u16 = 1024;
/// Schlick exponent on `(1 - mu)`.
pub const WATER_ALBEDO_POW: u8 = 5;

impl WaterAlbedo {
    /// The default Fresnel-like curve: nadir 0.06, grazing 1.0, Schlick
    /// power 5. Flux-weighted over the M1 day on a horizontal pond top this
    /// averages ~0.08 (below the fixed 0.15: high sun is absorbed more, the
    /// low dawn/dusk sun reflects more).
    pub const FRESNEL: Self = Self::Fresnel {
        nadir: WATER_ALBEDO_NADIR,
        graze: WATER_ALBEDO_GRAZE,
        pow: WATER_ALBEDO_POW,
    };
}

impl Default for WaterAlbedo {
    fn default() -> Self {
        Self::FRESNEL
    }
}

/// Water albedo (1/1024ths) at incidence cosine `mu` (1/1024ths, clamped to
/// `[0, 1024]`) under `model`. `Fixed` ignores `mu`. `Fresnel` is
/// `nadir + floor((graze - nadir) * x)` with `x = (1 - mu)^pow` evaluated by
/// repeated floor-multiplication in 1/1024ths; monotone non-increasing in
/// `mu`, equal to `nadir` at `mu = 1024` and `graze` at `mu = 0`.
pub fn water_albedo(mu: i64, model: WaterAlbedo) -> i64 {
    match model {
        WaterAlbedo::Fixed => ALBEDO_WATER,
        WaterAlbedo::Fresnel { nadir, graze, pow } => {
            let (n, g) = (
                i64::from(nadir).min(ALBEDO_DEN),
                i64::from(graze).min(ALBEDO_DEN),
            );
            let c = ALBEDO_DEN - mu.clamp(0, ALBEDO_DEN);
            let mut x = ALBEDO_DEN;
            for _ in 0..pow {
                x = x * c / ALBEDO_DEN;
            }
            n + (g - n) * x / ALBEDO_DEN
        }
    }
}

/// The albedo (1/1024ths) a ray entering a cell of `m` at incidence cosine
/// `mu` (1/1024ths) sees under the Driven sun: [`water_albedo`] for WATER,
/// the fixed [`albedo`] for everything else (ICE 0.60, land/gas 0).
pub fn albedo_at(m: MatId, mu: i64, model: WaterAlbedo) -> i64 {
    if m == MatId::WATER {
        water_albedo(mu, model)
    } else {
        albedo(m)
    }
}

/// The absorbed part of a sun quantum `f` on a cell with albedo `a`
/// (1/1024ths): `floor((f * (1024 - a) + dither) / 1024)` with a stateless
/// dither in `[0, 1024)` hashed from `(tick, cell, face)`. Its expectation is
/// exactly `f * (1 - a/1024)` (no truncation bias against small dawn quanta),
/// it is deterministic, and it never exceeds `f`. `a == 0` returns `f`.
pub(crate) fn absorbed_share(f: i64, a: i64, tick: u64, cell: usize, face: usize) -> i64 {
    if a <= 0 || f <= 0 {
        return f;
    }
    let mut h = tick.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (cell as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ (face as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
    h ^= h >> 31;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 29;
    let dither = (h >> 54) as i64; // top 10 bits: [0, 1024)
    ((f * (ALBEDO_DEN - a) + dither) / ALBEDO_DEN).clamp(0, f)
}

/// Vapor greenhouse denominator: [`vapor_trap`] is in 1/1024ths.
pub const GH_VAPOR_DEN: i64 = 1024;
/// Most outbound IR the vapor pool can hold back (1/1024ths; 0.50). Water
/// vapor is the dominant real greenhouse gas, but even a saturated column
/// re-emits roughly half its absorbed IR upward, so trapping stays below 1.
pub const GH_VAPOR_MAX: i64 = 512;
/// Vapor column at half of [`GH_VAPOR_MAX`], in 1/1024 pairs per rim cell
/// (4096 = four vapor pairs per domain-boundary cell). Tuned on r12 M1 +
/// Driven soaks (§5o): 4096 pulls ocean H=8 about halfway back from the
/// albedo cooling and puts ocean H=0 ice at its pre-albedo level; 1024 and
/// 2048 overheat the H=0 ocean well past its pre-albedo state.
pub const GH_VAPOR_HALF: i64 = 4096;

/// The share of outbound IR (1/1024ths) the vapor pool traps under
/// [`WaterCycle::Driven`]: `MAX * c / (c + HALF)` with the column density
/// `c = vapor / rim` (vapor pairs per domain-boundary cell, the cells that
/// leak to space). A saturating curve: a dry sky traps ~0, an ocean sky a
/// bounded fraction, never more than [`GH_VAPOR_MAX`]. All-integer.
pub fn vapor_trap(vapor: i64, rim: i64) -> i64 {
    if vapor <= 0 || rim <= 0 {
        return 0;
    }
    let v = vapor.saturating_mul(GH_VAPOR_DEN);
    (GH_VAPOR_MAX * v / (v + GH_VAPOR_HALF.saturating_mul(rim))).clamp(0, GH_VAPOR_MAX)
}

/// Vapor cloud albedo denominator: [`cloud_reflect`] is in 1/1024ths.
pub const CLOUD_DEN: i64 = 1024;
/// Most incoming sunlight the vapor clouds can reflect (1/1024ths) under
/// [`CloudAlbedo::VAPOR`]. Earth's clouds send back ~0.2 of the incoming
/// shortwave at ~0.6-0.7 cover; this cap keeps a saturated sky well below a
/// white-out. Tuned on r12 M1 + Driven soaks (§5q).
pub const CLOUD_MAX: u16 = 256;
/// Vapor column at half of the cap, in 1/1024 pairs per rim cell (same unit
/// as [`GH_VAPOR_HALF`], so cloud and greenhouse read one column density).
pub const CLOUD_HALF: u32 = 4096;

/// How the vapor pool reflects sunlight before it reaches the surface (Driven
/// sun only, and only with a greenhouse on, mirroring the vapor greenhouse
/// gate; Classic has no albedo at all).
///
/// * [`CloudAlbedo::Off`]: no cloud stage, the §5p stack bit-for-bit.
/// * [`CloudAlbedo::Vapor`] (default [`CloudAlbedo::VAPOR`]): every sun ray
///   that would land on a non-AIR cell first loses
///   `cloud = max * c / (c + half)` (1/1024ths) of its quantum to space, with
///   `c = vapor * 1024 / rim` the same global column density the greenhouse
///   uses ([`vapor_trap`]); the surface albedo ([`albedo_at`]) then acts on
///   the rest. Cloud-reflected light is never injected, so it is in no book.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloudAlbedo {
    Off,
    Vapor {
        /// Cap on the reflected share, 1/1024ths (<= 1024).
        max: u16,
        /// Column density (1/1024 pairs per rim cell) at half the cap.
        half: u32,
    },
}

impl CloudAlbedo {
    /// The tuned default: [`CLOUD_MAX`] / [`CLOUD_HALF`].
    pub const VAPOR: Self = Self::Vapor {
        max: CLOUD_MAX,
        half: CLOUD_HALF,
    };
}

impl Default for CloudAlbedo {
    fn default() -> Self {
        Self::VAPOR
    }
}

/// Cloud reflectance (1/1024ths of incoming sunlight) for a global vapor
/// pool of `vapor` pairs over `rim` domain-boundary cells under `model`:
/// `max * c / (c + half)` with `c = vapor * 1024 / rim`. Zero when dry, when
/// `rim == 0`, or under [`CloudAlbedo::Off`]; monotone in `vapor`, never above
/// `max`. All-integer.
pub fn cloud_reflect(vapor: i64, rim: i64, model: CloudAlbedo) -> i64 {
    let CloudAlbedo::Vapor { max, half } = model else {
        return 0;
    };
    if vapor <= 0 || rim <= 0 || max == 0 {
        return 0;
    }
    let m = i64::from(max).min(CLOUD_DEN);
    // i128: no overflow for any i64 pool or rim
    let v = i128::from(vapor) * i128::from(CLOUD_DEN);
    let h = i128::from(half) * i128::from(rim);
    ((i128::from(m) * v / (v + h).max(1)) as i64).clamp(0, m)
}

impl EnergyInput {
    /// No inbound energy: planet6 behaviour.
    pub const OFF: Self = Self {
        sun: 0,
        day: 0,
        core: 0,
        core_r: 0,
    };
    /// The M1 soak setting: a 600-tick day (40 s at the 15 Hz play cadence) at
    /// 16 units per space face at noon, plus a weak 40/tick core.
    pub const M1: Self = Self {
        sun: 16,
        day: 600,
        core: 40,
        core_r: 0,
    };
    /// Default core-sphere radius for sky-shell soaks at r12 (see
    /// [`EnergyInput::with_core_r`]): reaches past the plume roots under the
    /// pole (`R - PLUME_DEPTH = 4` at r12).
    pub const CORE_R_SPREAD: u32 = 6;

    /// The same input with the core heat spread over a sphere of radius `r`.
    pub const fn with_core_r(self, r: u32) -> Self {
        Self { core_r: r, ..self }
    }

    pub const fn is_off(&self) -> bool {
        self.sun <= 0 && self.core <= 0
    }

    /// The integer sun direction at `tick`: `(sx, sy)` in `[-1024, 1024]`
    /// (triangle-wave cos/sin of the day phase).
    pub fn sun_dir(&self, tick: u64) -> (i64, i64) {
        if self.day == 0 {
            return (0, 1024);
        }
        let day = u64::from(self.day);
        let u = ((tick % day) * 4096 / day) as i64; // 0..4096 = one turn
        let tri_cos = |u: i64| {
            let u = u.rem_euclid(4096);
            if u < 2048 { 1024 - u } else { u - 3072 }
        };
        (tri_cos(u), tri_cos(u - 1024))
    }

    /// The 3-D sun direction at `tick` under `sweep` and whether it shines.
    /// `Upper`: `(sx, sy, 0)` from [`EnergyInput::sun_dir`], lit while
    /// `sy > 0`. `Full`: always lit; with a tilt, `sz` follows a triangle
    /// sine over `year * day` ticks and `sx, sy` shrink so `|s|_1 = 1024`
    /// (to integer truncation).
    pub fn sun_dir3(&self, tick: u64, sweep: SunSweep) -> (i64, i64, i64, bool) {
        let (sx, sy) = self.sun_dir(tick);
        match sweep {
            SunSweep::Upper => (sx, sy, 0, sy > 0),
            SunSweep::Full { tilt, year } => {
                if tilt == 0 || year == 0 || self.day == 0 {
                    return (sx, sy, 0, true);
                }
                let ylen = u64::from(self.day) * u64::from(year);
                let u = ((tick % ylen) * 4096 / ylen) as i64;
                // triangle sine: 0 at u=0, +1024 at u=1024, 0 at 2048, -1024 at 3072
                let tri_sin = if u < 1024 {
                    u
                } else if u < 3072 {
                    2048 - u
                } else {
                    u - 4096
                };
                let sz = i64::from(tilt.min(1024)) * tri_sin / 1024;
                let k = 1024 - sz.abs();
                (sx * k / 1024, sy * k / 1024, sz, true)
            }
        }
    }
}

/// How the surface water cycle and weathering are scheduled (BitBlock-only).
///
/// * [`WaterCycle::Classic`] (default) is planet6 bit-for-bit: the `SEASON=240`
///   calendar gates storms (erode, dissolve) and the rain window, rain lands at
///   the lowest-index AIR cell beside WATER, and rolls use planet6's 16-bit
///   `lattice` keys (cell key and tick both masked to 16 bits).
/// * [`WaterCycle::Driven`] removes every calendar: evaporation odds come from
///   the water cell's own heat, the rain rate from the vapor inventory, rain
///   placement from local surface state (sky-open AIR resting on ground,
///   weighted toward cold cells, content-addressed pick), and erosion /
///   dissolution / sediment entrainment fire next to water that actually
///   moved (fell, rained, sloshed) or froze/thawed this tick. Every roll in
///   the tick (reaction laws included) uses [`wide_roll`] on the full 64-bit
///   coordinate and tick, so no 65,536-tick or 48-lattice repeat. Books are
///   unchanged: every driven law is a planet6 transition (evaporate, rain,
///   erode, dissolve) or a pure material swap (flow, entrain, settle).
///   Four relief laws keep late terrain changing (see `p6_rest`): dry
///   weathering (sky-/sun-exposed ROCK/MIN -> SED without water), scour
///   (moving water strips SED lying on rock sideways), and core-heat uplift
///   (a heat-driven convection roll that rotates materials one cell along a
///   down-across-up path, lifting a hot-rooted column and lowering its cooler
///   neighbour), and deep convective overturn (a longer-range roll rooted
///   8 cells down: a cold root takes in surface fluid, a hotter column 2..6
///   cells away rises a cell). All four are polymorph swaps or permutations:
///   lanes, BIND and Q are untouched. Hydrothermal degassing (`core > 0`)
///   returns hot submerged water on a rock bed to the vapor pool with
///   evaporation's exact bookkeeping (WATER -> AIR + one vapor pair, the cell
///   pays `LATENT_PAIR`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum WaterCycle {
    #[default]
    Classic,
    Driven,
}

/// Wide deterministic roll for the driven mode: a splitmix64 chain over the
/// seed/salt, the full cell coordinate and the full 64-bit time key.
pub fn wide_roll(s: u32, c: Coord, t: u64) -> u32 {
    wide_roll_keyed(wide_roll_key(s), c, t)
}

#[inline]
fn wide_mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// The first link of [`wide_roll`]'s chain, which depends on the seed/salt alone: a scan
/// that rolls many cells under one salt computes it once.
#[inline]
pub(crate) fn wide_roll_key(s: u32) -> u64 {
    wide_mix(u64::from(s) ^ 0x5bd1_e995)
}

/// [`wide_roll`] from its precomputed [`wide_roll_key`].
#[inline]
pub(crate) fn wide_roll_keyed(key: u64, c: Coord, t: u64) -> u32 {
    wide_roll_finish(wide_roll_cell(key, c), t)
}

/// The links of [`wide_roll`]'s chain that depend on the salt and the cell alone: static for a
/// cell under one salt, so a scan repeated every tick can keep them.
#[inline]
pub(crate) fn wide_roll_cell(key: u64, c: Coord) -> u64 {
    let mix = wide_mix;
    let mut h = key;
    h = mix(h ^ c.x as u64);
    h = mix(h ^ c.y as u64);
    mix(h ^ c.z as u64)
}

/// The last link of [`wide_roll`]'s chain: the time key over [`wide_roll_cell`].
#[inline]
pub(crate) fn wide_roll_finish(cell: u64, t: u64) -> u32 {
    (wide_mix(cell ^ t) >> 32) as u32
}

impl Default for EnergyInput {
    fn default() -> Self {
        Self::OFF
    }
}

/// planet6 `P`, the tick parameters (only the laws that are ported).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct P6Params {
    /// Genesis core heat (`P.genesisQ`).
    pub genesis_q: i32,
    /// Genesis surface heat (`P.surfaceQ`).
    pub surface_q: i32,
    /// Genesis heat of water/ice (`P.oceanQ`).
    pub ocean_q: i32,
    /// Diffusion meter: each cell sends `trunc(Q*dq/6000)` per tick (`P.dQ`).
    pub dq: i32,
    /// Base surface leak per tick (`P.leakQ`).
    pub leak_q: i32,
    /// Roll seed (`P.seed`).
    pub seed: u32,
    pub greenhouse: Greenhouse,
    /// Element world on (`P.elements`): gates every reaction law, condensation
    /// and gas rise exactly like planet6's early returns.
    pub elements: bool,
    /// Hand fusion families (`P.combine`).
    pub combine: bool,
    /// Generic network firing law (`P.netChem`), when a net is attached.
    pub net_chem: bool,
    /// Network-commuted combustion (`P.netBurn`), when the net has a CO row.
    pub net_burn: bool,
    /// Steam condensation (`P.condense`).
    pub condense: bool,
    /// Gas rise (`P.gasRise`).
    pub gas_rise: bool,
    /// Water cycle: evaporation into the vapor pool and rain (`P.rain`).
    pub rain: bool,
    /// Storm weathering of exposed rock/mineral to sediment (`P.erode`).
    pub erode: bool,
    /// Storm dissolution of wet gypsum (claim-rule v5.1) (`P.dissolve`).
    pub dissolve: bool,
    /// Loose sediment sinks through air/water (`P.settle`).
    pub settle: bool,
    /// Water falls into air, else spreads sideways (`P.flow`).
    pub flow: bool,
    /// BitBlock-only switches (planet6 always runs these).
    pub heat: bool,
    pub phase: bool,
    /// M1 booked energy input (BitBlock-only; [`EnergyInput::OFF`] by default).
    pub energy_in: EnergyInput,
    /// Water-cycle schedule ([`WaterCycle::Classic`] = planet6 by default).
    pub cycle: WaterCycle,
    /// Angle dependence of open-water albedo under the Driven sun
    /// ([`WaterAlbedo::FRESNEL`] by default; no effect under Classic, which
    /// has no albedo).
    pub water_albedo: WaterAlbedo,
    /// Vapor cloud albedo on the Driven sun ([`CloudAlbedo::VAPOR`] by
    /// default; engages only under [`WaterCycle::Driven`] with a greenhouse
    /// on, like the vapor greenhouse).
    pub cloud: CloudAlbedo,
    /// Where the Driven vapor pool lives ([`VaporLocality::PATCHES`] by
    /// default: regional bins, so clouds and the vapor greenhouse sit over the
    /// water that evaporated; [`VaporLocality::Global`] = the one-pool
    /// §5o–§5q model). No effect under Classic.
    pub vapor_local: VaporLocality,
    /// Driven rain fall test ([`RainFall::Core`] by default: rain falls
    /// toward the core so every region lands its own drops;
    /// [`RainFall::Down`] = the §5b–§5r world -y test). No effect under
    /// Classic.
    pub rain_fall: RainFall,
    /// Driven evaporation top test ([`EvapTop::Core`] by default, §5t: a
    /// WATER cell evaporates when its outward step from the core is space or
    /// not WATER, in [`RainFall::Core`]'s frame; [`EvapTop::Up`] = the
    /// §5b–§5s world +y test; [`EvapTop::CoreAir`] = strict A/B). No effect
    /// under Classic.
    pub evap_top: EvapTop,
    /// Which part of the sphere the M1 sun lights ([`SunSweep::Upper`] by
    /// default = the §5b–§5u `s.y > 0` day; [`SunSweep::Full`] = no global
    /// night, §5v).
    pub sun_sweep: SunSweep,
    /// Driven surface gravity ([`Gravity::Core`] by default, §5w: water,
    /// sediment and rising gas move toward / away from the core, the rain and
    /// evaporation frame; [`Gravity::Down`] = the §5b–§5v world `-y`). No
    /// effect under Classic.
    pub gravity: Gravity,
    /// §5y gas convection (BitBlock-only, opt-in, `false` = every earlier
    /// world bit-identical): under [`WaterCycle::Driven`] with core gravity,
    /// a rising-gas cell swaps (material and heat) with the rising-gas cell
    /// one outward step above it when it is the lighter parcel by the
    /// ideal-gas density test `M_below * T_above < M_above * T_below`
    /// (`M` = molar mass of the cell's composition, `T = Q + CONVECT_T0`).
    /// Books are untouched (a swap moves atoms and heat, never mints them).
    pub convect: bool,
}

impl Default for P6Params {
    /// planet6 defaults, except `elements: true` (BitBlock genesis is an
    /// element world; planet6's default is the mineral world).
    fn default() -> Self {
        Self {
            elements: true,
            ..Self::planet6_defaults()
        }
    }
}

impl P6Params {
    /// planet6 `P` defaults verbatim (`elements: false`).
    pub const fn planet6_defaults() -> Self {
        Self {
            genesis_q: 3000,
            surface_q: 900,
            ocean_q: 40,
            dq: 300,
            leak_q: 3,
            seed: 7,
            greenhouse: Greenhouse::Planet6,
            elements: false,
            combine: true,
            net_chem: true,
            net_burn: true,
            condense: true,
            gas_rise: true,
            rain: true,
            erode: true,
            dissolve: true,
            settle: true,
            flow: true,
            heat: true,
            phase: true,
            energy_in: EnergyInput::OFF,
            cycle: WaterCycle::Classic,
            water_albedo: WaterAlbedo::FRESNEL,
            cloud: CloudAlbedo::VAPOR,
            vapor_local: VaporLocality::PATCHES,
            rain_fall: RainFall::Core,
            evap_top: EvapTop::Core,
            sun_sweep: SunSweep::Upper,
            gravity: Gravity::Core,
            convect: false,
        }
    }

    /// Heat diffusion and leak only: every reaction law, phase change,
    /// condensation, gas rise and every surface law (water cycle, erosion,
    /// dissolution, settling, flow) off.
    pub const fn heat_only() -> Self {
        Self {
            elements: false,
            phase: false,
            ..Self::planet6_defaults().without_surface_laws()
        }
    }

    /// The same parameters with planet6's five surface laws off (`rain`,
    /// `erode`, `dissolve`, `settle`, `flow` all false): the configuration
    /// of the heat/reaction goldens A-E.
    pub const fn without_surface_laws(self) -> Self {
        Self {
            rain: false,
            erode: false,
            dissolve: false,
            settle: false,
            flow: false,
            ..self
        }
    }
}

/// planet6 `lattice(s, a, b)`: the stateless integer roll hash.
pub fn lattice(s: u32, a: u32, b: u32) -> u32 {
    let mut h: u64 = u64::from(s ^ 0x1b87_3593);
    h = (h + u64::from(a & 0xffff) * 374_761_393 + u64::from(b & 0xffff) * 668_265_263)
        % 4_294_967_296;
    h ^= h >> 13;
    h = ((h & 0xffff) * 1_274_126_177 + (h >> 16) * 971) % 4_294_967_296;
    (h ^ (h >> 16)) as u32
}

/// The roll key of a cell: planet6's `idx = (x*48 + y)*48 + z` (wrapping;
/// only its low 16 bits enter [`lattice`]).
pub fn cell_key(c: Coord) -> u32 {
    (c.x.wrapping_mul(P6_GRID).wrapping_add(c.y))
        .wrapping_mul(P6_GRID)
        .wrapping_add(c.z) as u32
}

/// What one [`ChemWorld::step`] did.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StepReport {
    pub tick: u64,
    /// Heat radiated to space this tick.
    pub leaked: i64,
    pub froze: u32,
    pub melted: u32,
    /// 4H + 2O -> WATER firings.
    pub water: u32,
    /// Hand families, in planet6 order: C+2O->CO2, Ca+S+4O->ROCK, N+3H->NH3,
    /// Ca+C+3O->CARB, O+O->O2, Ca+O->CAO.
    pub hand: [u32; 6],
    /// Network-law firings (all rounds) and rounds run.
    pub net_fired: u32,
    pub net_rounds: u32,
    pub net_burn: u32,
    pub condensed: u32,
    pub rose: u32,
    /// Water voxels lifted into the vapor pool / pairs rained back.
    pub evaporated: u32,
    pub rained: u32,
    /// Rock/mineral weathered to sediment.
    pub eroded: u32,
    /// Gypsum dissolved (gypsum -> sediment, its air pocket -> water).
    pub dissolved: u32,
    /// Sediment/fluid swaps and water moves.
    pub settled: u32,
    pub flowed: u32,
    /// Heat injected by the booked energy input this tick (0 when off).
    pub injected: i64,
    /// Sunlight reflected by surface albedo this tick ([`WaterCycle::Driven`]
    /// only; never injected, so in no book). `injected + reflected` is the
    /// sun that reached a non-AIR cell (plus core heat).
    pub reflected: i64,
    /// Outbound IR the vapor greenhouse held back this tick: heat that stayed
    /// in rim-cell `Q` instead of leaking to `space_q` ([`WaterCycle::Driven`]
    /// only; a diagnostic outside every book, since nothing is minted).
    pub trapped: i64,
    /// Sediment carried one cell by running water ([`WaterCycle::Driven`] only).
    pub entrained: u32,
    /// Dry (sky/sun) weathering of exposed ROCK/MIN to SED (Driven only).
    pub weathered: u32,
    /// Sediment blankets on rock stripped sideways by moving water (Driven only).
    pub scoured: u32,
    /// Core-heat convection rolls: one column lifted a cell, its neighbour
    /// lowered (Driven only, needs `energy_in.core > 0`).
    pub uplifted: u32,
    /// Deep convective overturns: a cold column root took in its surface
    /// fluid and a hotter column `PLUME_REACH_MIN..=MAX` cells away rose a
    /// cell, through a root-depth channel (Driven only, needs
    /// `energy_in.core > 0`).
    pub plumed: u32,
    /// Hydrothermal degassing: hot submerged WATER on a rock bed boiled into
    /// the vapor pool (Driven only, needs `energy_in.core > 0`).
    pub degassed: u32,
    /// Void collapse: an unsupported rock-like cell dropped one cell toward
    /// the core into the AIR under it (Driven only).
    pub collapsed: u32,
    /// §5y gas-convection swaps: a lighter (hotter or lower molar mass) gas
    /// parcel traded places with the heavier gas parcel above it (Driven
    /// with `convect` only).
    pub convected: u32,
}

/// A tick that could not complete.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StepError {
    /// planet6 "FIRE-PROTOCOL STUCK": firings were still happening at the
    /// round cap. The world is left mid-tick (as planet6's throw leaves it);
    /// books still balance because every committed firing conserves.
    FireProtocolStuck { tick: u64 },
}

impl fmt::Display for StepError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FireProtocolStuck { tick } => write!(
                f,
                "FIRE-PROTOCOL STUCK at tick {tick}: firings still occurring at round cap {FIRE_MAX_ROUNDS}"
            ),
        }
    }
}

impl std::error::Error for StepError {}

/// Neighbour lookup over a domain: a dense slot box when the domain is
/// compact, else a hash map.
#[derive(Clone, Debug)]
enum Lookup {
    Boxed {
        min: [i64; 3],
        dim: [i64; 3],
        slot: Vec<u32>,
    },
    Hashed(HashMap<Coord, u32>),
}

pub(crate) const NO_SLOT: u32 = u32::MAX;

/// A salt's precomputed part of a [`ChemWorld::roll`] (see [`ChemWorld::roll_key`]).
#[derive(Clone, Copy, Debug)]
pub(crate) enum RollKey {
    Lattice(u32),
    Wide(u64),
}

/// The first index at or after `from` whose material is `a` or `b` ([`find_mat`] for two ids,
/// with the same visiting guarantee).
#[inline]
pub(crate) fn find_mat2(mat: &[MatId], from: usize, a: MatId, b: MatId) -> Option<usize> {
    const BLOCK: usize = 64;
    let mut i = from;
    while i < mat.len() {
        let end = (i + BLOCK).min(mat.len());
        let block = &mat[i..end];
        if block
            .iter()
            .fold(false, |hit, &x| hit | (x == a) | (x == b))
        {
            return block.iter().position(|&x| x == a || x == b).map(|k| i + k);
        }
        i = end;
    }
    None
}

/// The first index at or after `from` whose material is `m`. A scan that walks the cells of one
/// material in index order calls it again after each visit, so a visit's writes are seen
/// exactly as a cell-by-cell scan would see them (the cells it skips are not `m` and nothing
/// writes between the call and the visit). Blocks of cells are tested at once.
#[inline]
pub(crate) fn find_mat(mat: &[MatId], from: usize, m: MatId) -> Option<usize> {
    const BLOCK: usize = 64;
    let mut i = from;
    while i < mat.len() {
        let end = (i + BLOCK).min(mat.len());
        let block = &mat[i..end];
        if block.iter().fold(false, |hit, &x| hit | (x == m)) {
            return block.iter().position(|&x| x == m).map(|k| i + k);
        }
        i = end;
    }
    None
}

/// §6y: which of `ids` occur in `mat`, in one pass over 64-cell blocks (the same block
/// test as [`find_mat`]); stops once every id has been seen.
pub(crate) fn mats_present<const K: usize>(mat: &[MatId], ids: [MatId; K]) -> [bool; K] {
    let mut seen = [false; K];
    for block in mat.chunks(64) {
        for (s, &id) in seen.iter_mut().zip(&ids) {
            *s |= block.iter().fold(false, |hit, &x| hit | (x == id));
        }
        if seen.iter().all(|&s| s) {
            break;
        }
    }
    seen
}

/// Bit `k` set iff `hit(mat[k], q[k])`, for one block of at most 64 cells. A scan whose visits
/// write only the visited cell (and test only that cell's own material and heat to decide
/// whether it acts) may take a block's hits at once: no visit changes another cell's test.
#[inline]
pub(crate) fn hit_mask(mat: &[MatId], q: &[i32], hit: impl Fn(MatId, i32) -> bool) -> u64 {
    debug_assert!(mat.len() <= 64 && mat.len() == q.len());
    if let (Ok(m), Ok(q)) = (<&[MatId; 64]>::try_from(mat), <&[i32; 64]>::try_from(q)) {
        // a whole block: a fixed trip count the compiler turns into vector compares
        let mut b = 0u64;
        for k in 0..64 {
            b |= (hit(m[k], q[k]) as u64) << k;
        }
        return b;
    }
    mat.iter()
        .zip(q)
        .enumerate()
        .fold(0u64, |b, (k, (&m, &x))| b | ((hit(m, x) as u64) << k))
}

/// `dst[k] += src[k]` wrapping, the heat law's share additions over one row slice.
#[inline]
fn add_shares(dst: &mut [i32], src: &[i32]) {
    for (d, s) in dst.iter_mut().zip(src) {
        *d = d.wrapping_add(*s);
    }
}

/// The heat law's first sweep assuming six neighbours. Rim cells are patched with their
/// actual live count before any share is read. `emit(q)` is the same truncating quotient as
/// the law (this path is used only when dq divides 6000).
#[inline(always)]
fn heat_shares_six(q: &[i32], per: &mut [i32], nq: &mut [i32], emit: impl Fn(i32) -> i32) {
    for ((&qi, p), k) in q.iter().zip(per.iter_mut()).zip(nq.iter_mut()) {
        let share = emit(qi);
        *p = share;
        *k = qi - share * 6;
    }
}

/// `o / live` for a face-neighbour count `live` in `1..=6`, with each divisor a constant (the
/// same truncating quotient; a constant divisor compiles to a multiply instead of a division).
#[inline]
pub(crate) fn div_by_live(o: i64, live: usize) -> i64 {
    match live {
        1 => o,
        2 => o / 2,
        3 => o / 3,
        4 => o / 4,
        5 => o / 5,
        6 => o / 6,
        _ => o / live as i64,
    }
}

/// Per-cell buffers of [`ChemWorld::heat_pass`], kept across steps so a step does not
/// allocate (and fault in) a fresh heat field. Every entry is written before it is read in
/// a step: the z-run path writes each cell's `nq` and `per` in its first sweep, the general
/// path clears `nq`. After the pass `nq` holds the previous heat field, which nothing reads.
#[derive(Default)]
pub(crate) struct HeatScratch {
    nq: Vec<i32>,
    per: Vec<i32>,
}

/// A clone starts with empty scratch: the buffers hold no state between steps.
impl Clone for HeatScratch {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl std::fmt::Debug for HeatScratch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HeatScratch({} cells)", self.nq.len())
    }
}

/// One `(x, y)` row of the domain: cells `start..=start + (z1 - z0)` are `(x, y, z0..=z1)`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ZRun {
    pub(crate) x: i64,
    pub(crate) y: i64,
    pub(crate) z0: i64,
    pub(crate) z1: i64,
    pub(crate) start: usize,
}

/// The domain as z-runs: each `(x, y)` row of it is one or more contiguous z ranges, in index
/// order (a ball or a box has one per row). A cell's face neighbours are then plain index
/// arithmetic on the runs of four neighbouring rows. §6x: a row may hold several runs (a
/// domain-growing place off the end of a row leaves a gap), so such a domain keeps the row
/// slices instead of every cell's face lookups; passes that read a row as one run check
/// [`Self::contiguous`].
#[derive(Clone, Debug)]
pub(crate) struct ZRuns {
    pub(crate) runs: Vec<ZRun>,
    /// Runs that follow another run of the same row (0 when every row is one z range).
    split: usize,
    x0: i64,
    y0: i64,
    nx: i64,
    ny: i64,
    /// `(x - x0) * ny + (y - y0)` to the index of the row's first run, or `NO_SLOT`.
    at: Vec<u32>,
    /// Per cell (domain order), its number of face neighbours in the domain (`0..=6`).
    pub(crate) live: Vec<u8>,
}

/// §6w: spare capacity kept in the per-cell tables of an `n`-cell domain (1/64 more).
fn growth_room(n: usize) -> usize {
    n / 64 + 16
}

impl ZRuns {
    /// §6w: the rows after domain cell `at` was inserted (`cells` is the new domain,
    /// `nbrs` the new cell's face neighbours by new index), exactly as [`Self::build`] makes
    /// them for `cells`: the new cell extends its row at one end or starts a row of its own,
    /// later rows start one cell later, and the new cell and its neighbours count each other.
    /// Falls back to `build` where the row box would change or the row has or gets a gap.
    fn inserted(mut self, cells: &[Coord], at: usize, nbrs: &[Option<usize>]) -> Option<Self> {
        let c = cells[at];
        let (rx, ry) = (c.x - self.x0, c.y - self.y0);
        if rx < 0
            || ry < 0
            || rx >= self.nx
            || ry >= self.ny
            || self.runs.len() + 1 >= NO_SLOT as usize
        {
            return Self::build(cells);
        }
        let cell = (rx * self.ny + ry) as usize;
        let k = match self.at[cell] {
            NO_SLOT => {
                let k = self.runs.partition_point(|r| (r.x, r.y) < (c.x, c.y));
                for v in &mut self.at {
                    if *v != NO_SLOT && *v as usize >= k {
                        *v += 1;
                    }
                }
                self.at[cell] = k as u32;
                self.runs.insert(
                    k,
                    ZRun {
                        x: c.x,
                        y: c.y,
                        z0: c.z,
                        z1: c.z,
                        start: at,
                    },
                );
                k
            }
            k => {
                let k = k as usize;
                if self
                    .runs
                    .get(k + 1)
                    .is_some_and(|n| (n.x, n.y) == (c.x, c.y))
                {
                    return Self::build(cells); // a row of several runs
                }
                let r = &mut self.runs[k];
                if c.z.checked_add(1) == Some(r.z0) {
                    r.z0 = c.z;
                } else if r.z1.checked_add(1) == Some(c.z) {
                    r.z1 = c.z;
                } else {
                    return Self::build(cells); // a gap in the row: a second run, as `build` makes it
                }
                k
            }
        };
        for r in &mut self.runs[k + 1..] {
            r.start += 1;
        }
        self.live.insert(at, nbrs.iter().flatten().count() as u8);
        for &j in nbrs.iter().flatten() {
            self.live[j] += 1;
        }
        Some(self)
    }

    fn build(cells: &[Coord]) -> Option<Self> {
        let first = cells.first()?;
        let mut runs: Vec<ZRun> = Vec::new();
        let mut split = 0;
        for (i, c) in cells.iter().enumerate() {
            match runs.last_mut() {
                // cells are sorted and distinct, so the run continues iff z steps by one
                Some(r) if r.x == c.x && r.y == c.y && r.z1.checked_add(1) == Some(c.z) => {
                    r.z1 = c.z;
                }
                Some(r) if r.x == c.x && r.y == c.y => {
                    split += 1; // a gap: the row's next run
                    runs.push(ZRun {
                        x: c.x,
                        y: c.y,
                        z0: c.z,
                        z1: c.z,
                        start: i,
                    });
                }
                _ => runs.push(ZRun {
                    x: c.x,
                    y: c.y,
                    z0: c.z,
                    z1: c.z,
                    start: i,
                }),
            }
        }
        let (mut x1, mut y0, mut y1) = (first.x, first.y, first.y);
        for r in &runs {
            x1 = x1.max(r.x);
            y0 = y0.min(r.y);
            y1 = y1.max(r.y);
        }
        let nx = x1.checked_sub(first.x)?.checked_add(1)?;
        let ny = y1.checked_sub(y0)?.checked_add(1)?;
        let area = nx.checked_mul(ny)?;
        if area > 4 * runs.len() as i64 + 64 || runs.len() >= NO_SLOT as usize {
            return None;
        }
        let mut at = vec![NO_SLOT; area as usize];
        for (k, r) in runs.iter().enumerate().rev() {
            at[((r.x - first.x) * ny + (r.y - y0)) as usize] = k as u32; // the row's first run
        }
        let mut rows = Self {
            runs,
            split,
            x0: first.x,
            y0,
            nx,
            ny,
            at,
            live: Vec::new(),
        };
        let mut live = vec![0_u8; cells.len()];
        for run in &rows.runs {
            let beside = rows.beside(run);
            for z in run.z0..=run.z1 {
                let mut n = u8::from(z > run.z0) + u8::from(z < run.z1);
                for r in beside.iter().copied().flatten() {
                    n += u8::from(z >= r.z0 && z <= r.z1);
                }
                live[run.start + (z - run.z0) as usize] = n;
            }
        }
        rows.live = live;
        Some(rows)
    }

    /// Whether every row is one z range, so [`Self::row`] is the whole row.
    pub(crate) fn contiguous(&self) -> bool {
        self.split == 0
    }

    /// The first run of row `(x, y)`, if the domain has one: the whole row when
    /// [`Self::contiguous`].
    pub(crate) fn row(&self, x: Option<i64>, y: Option<i64>) -> Option<ZRun> {
        self.row_runs(x, y).first().copied()
    }

    /// The runs of row `(x, y)` in z order (empty if the domain has none there).
    pub(crate) fn row_runs(&self, x: Option<i64>, y: Option<i64>) -> &[ZRun] {
        let (Some(rx), Some(ry)) = (
            x.and_then(|x| x.checked_sub(self.x0)),
            y.and_then(|y| y.checked_sub(self.y0)),
        ) else {
            return &[];
        };
        if rx < 0 || ry < 0 || rx >= self.nx || ry >= self.ny {
            return &[];
        }
        let k = self.at[(rx * self.ny + ry) as usize];
        if k == NO_SLOT {
            return &[];
        }
        let k = k as usize;
        let (x, y) = (self.runs[k].x, self.runs[k].y);
        let mut end = k + 1;
        if self.split > 0 {
            while self.runs.get(end).is_some_and(|r| (r.x, r.y) == (x, y)) {
                end += 1;
            }
        }
        &self.runs[k..end]
    }

    /// The runs of the four rows beside `run` in `NB6` order: `x - 1`, `x + 1`, `y - 1`,
    /// `y + 1`.
    pub(crate) fn beside(&self, run: &ZRun) -> [&[ZRun]; 4] {
        [
            self.row_runs(run.x.checked_sub(1), Some(run.y)),
            self.row_runs(run.x.checked_add(1), Some(run.y)),
            self.row_runs(Some(run.x), run.y.checked_sub(1)),
            self.row_runs(Some(run.x), run.y.checked_add(1)),
        ]
    }

    /// The face neighbours of cell `(run.x, run.y, z)` (index `i`), in `NB6` order, as the
    /// first `n` entries of the array (the tests check the rows against the offset lookups).
    #[cfg(test)]
    pub(crate) fn neighbours(
        run: &ZRun,
        beside: &[&[ZRun]; 4],
        z: i64,
        i: usize,
    ) -> ([usize; 6], usize) {
        let mut js = [0usize; 6];
        let mut n = 0;
        for r in beside.iter().copied().flatten() {
            if z >= r.z0 && z <= r.z1 {
                js[n] = r.start + (z - r.z0) as usize;
                n += 1;
            }
        }
        if z > run.z0 {
            js[n] = i - 1;
            n += 1;
        }
        if z < run.z1 {
            js[n] = i + 1;
            n += 1;
        }
        (js, n)
    }
}

/// Cached domain topology (rebuilt when the domain changes).
#[derive(Clone, Debug)]
pub(crate) struct Topology {
    lookup: Lookup,
    pub(crate) keys: Vec<u32>,
    /// `(cell, cell above)` pairs in planet6's gas-rise order: y descending,
    /// then x, then z ascending.
    pub(crate) rise: Vec<(u32, u32)>,
    /// Every cell in planet6's settle/flow scan order: y ascending, then x,
    /// then z ascending.
    pub(crate) ysort: Vec<u32>,
    /// Cells with at least one face on space, with the face mask (bit k set
    /// iff [`NB6`]`[k]` is not a domain cell). Energy input only.
    pub(crate) space_faces: Vec<(u32, u8)>,
    /// The domain cell nearest the bounding-box centre (lowest index on ties).
    pub(crate) core: Option<u32>,
    /// §5w coreward frame, built by [`Topology::radial_frame`] once `core` is
    /// final: squared distance of every cell from the core cell (empty with
    /// no core).
    pub(crate) r2: Vec<i64>,
    /// Every cell in coreward scan order: `r2` ascending, then planet6's
    /// `(y, x, z)` (empty with no core).
    pub(crate) rsort: Vec<u32>,
    /// The outward neighbour of `rsort[k]` at index `k`, as an rsort position: the `k'` with
    /// `rsort[k']` the domain cell one [`outward_step`](super::p6_rest::outward_step) from
    /// `rsort[k]` away from the core, or `NO_SLOT` at the core or where that step leaves the
    /// domain. Static for the topology's life (the domain and the core are fixed), so the
    /// outermost-first gas-rise and convection scans read it in scan order instead of
    /// recomputing the step and probing the lookup per cell, and a scan over parcels gathered
    /// into rsort order stays in that order.
    pub(crate) shell_up: Vec<u32>,
    /// [`wide_roll_cell`] of every domain cell, in domain order, under one wide-roll key (the
    /// first key asked for; built on first use). Lets convection, which rolls the cells that
    /// pass its density test every tick, read one word per cell instead of its coordinate.
    cell_roll: std::sync::OnceLock<(u64, Vec<u64>)>,
    /// Each cell's position in `rsort` (the inverse permutation, built on first use).
    rpos: std::sync::OnceLock<Vec<u32>>,
    /// The outward neighbour of each domain cell as a cell index (`shell_up` in domain order:
    /// `cell_up[rsort[k]] == rsort[shell_up[k]]`, `NO_SLOT` where `shell_up` is), built on
    /// first use. Lets the convection density test run in domain order.
    cell_up: std::sync::OnceLock<Vec<u32>>,
    /// Each cell's position in `rise` as its lower cell, `NO_SLOT` for a cell with no cell
    /// above (built on first use). Lets the +y gas rise visit only its rising cells.
    rise_pos: std::sync::OnceLock<Vec<u32>>,
    /// The inverse of `ysort` (built on first use): `ysort[ypos[i]] == i`. Lets a sparse scan
    /// visit a few cells in `ysort` order without walking all of it.
    ypos: std::sync::OnceLock<Vec<u32>>,
    /// The domain's rows as z-runs, when each row is one contiguous run.
    pub(crate) zruns: Option<ZRuns>,
}

pub(crate) const NB6: [(i64, i64, i64); 6] = [
    (-1, 0, 0),
    (1, 0, 0),
    (0, -1, 0),
    (0, 1, 0),
    (0, 0, -1),
    (0, 0, 1),
];

/// §6t: `(y0, span)` of a domain in strictly ascending coordinate order whose y values span a
/// modest range, for [`counting_sort`] by y; `None` sends the caller to its comparison sort
/// (always under the test-only full scans).
fn counting_span(cells: &[Coord]) -> Option<(i64, usize)> {
    if super::p6_rest::full_scans() || cells.is_empty() || cells.len() >= NO_SLOT as usize {
        return None;
    }
    if !cells.windows(2).all(|w| w[0] < w[1]) {
        return None;
    }
    let (lo, hi) = cells.iter().fold((i64::MAX, i64::MIN), |(lo, hi), c| {
        (lo.min(c.y), hi.max(c.y))
    });
    let span = hi.checked_sub(lo)?.checked_add(1)?;
    (span as u64 <= 4 * cells.len() as u64 + 1024).then_some((lo, span as usize))
}

/// §6t: `items` stably sorted by `key` (each below `span`): the order a stable comparison
/// sort by that key alone gives.
fn counting_sort<T: Copy>(items: &[T], span: usize, key: impl Fn(T) -> usize) -> Vec<T> {
    let mut start = vec![0usize; span + 1];
    for &it in items {
        start[key(it) + 1] += 1;
    }
    for k in 0..span {
        start[k + 1] += start[k];
    }
    let Some(&first) = items.first() else {
        return Vec::new();
    };
    let mut out = vec![first; items.len()];
    for &it in items {
        let k = key(it);
        out[start[k]] = it;
        start[k] += 1;
    }
    out
}

impl Topology {
    pub(crate) fn build(cells: &[Coord]) -> Self {
        let lookup = Self::boxed(cells).unwrap_or_else(|| {
            Lookup::Hashed(
                cells
                    .iter()
                    .enumerate()
                    .map(|(i, c)| (*c, i as u32))
                    .collect(),
            )
        });
        let mut t = Self {
            lookup,
            keys: cells.iter().map(|c| cell_key(*c)).collect(),
            rise: Vec::new(),
            ysort: Vec::new(),
            r2: Vec::new(),
            rsort: Vec::new(),
            shell_up: Vec::new(),
            cell_roll: std::sync::OnceLock::new(),
            rpos: std::sync::OnceLock::new(),
            cell_up: std::sync::OnceLock::new(),
            rise_pos: std::sync::OnceLock::new(),
            ypos: std::sync::OnceLock::new(),
            zruns: ZRuns::build(cells),
            space_faces: Vec::new(),
            core: None,
        };
        // §6t: a domain in strictly ascending (x, y, z) order (as the chem world keeps it) is,
        // within one y, in (x, z) order, so a stable counting sort by y alone gives the same
        // lists as the comparison sorts by (y, x, z) and (-y, x, z) (the keys are distinct).
        let y_span = counting_span(cells);
        let mut ysort: Vec<u32> = (0..cells.len() as u32).collect();
        match y_span {
            Some((y0, span)) => {
                ysort = counting_sort(&ysort, span, |i| (cells[i as usize].y - y0) as usize);
            }
            None => ysort.sort_by_key(|&i| {
                let c = cells[i as usize];
                (c.y, c.x, c.z)
            }),
        }
        t.ysort = ysort;
        let mut rise: Vec<(u32, u32)> = Vec::new();
        for i in 0..cells.len() {
            if let Some(up) = t.offset(cells, i, (0, 1, 0)) {
                rise.push((i as u32, up as u32));
            }
        }
        match y_span {
            Some((y0, span)) => {
                rise = counting_sort(&rise, span, |(i, _)| {
                    span - 1 - (cells[i as usize].y - y0) as usize
                });
            }
            None => rise.sort_by_key(|&(i, _)| {
                let c = cells[i as usize];
                (std::cmp::Reverse(c.y), c.x, c.z)
            }),
        }
        t.rise = rise;
        for i in 0..cells.len() {
            let mut mask = 0u8;
            for (k, d) in NB6.iter().enumerate() {
                if t.offset(cells, i, *d).is_none() {
                    mask |= 1 << k;
                }
            }
            if mask != 0 {
                t.space_faces.push((i as u32, mask));
            }
        }
        if let Some(first) = cells.first() {
            let (mut lo, mut hi) = ([first.x, first.y, first.z], [first.x, first.y, first.z]);
            for c in cells {
                for (k, v) in [c.x, c.y, c.z].into_iter().enumerate() {
                    lo[k] = lo[k].min(v);
                    hi[k] = hi[k].max(v);
                }
            }
            // twice the centre, to stay integer
            let m2: [i128; 3] = std::array::from_fn(|k| i128::from(lo[k]) + i128::from(hi[k]));
            let mut best: Option<(i128, u32)> = None;
            for (i, c) in cells.iter().enumerate() {
                let d: i128 = [c.x, c.y, c.z]
                    .into_iter()
                    .enumerate()
                    .map(|(k, v)| {
                        let e = 2 * i128::from(v) - m2[k];
                        e * e
                    })
                    .sum();
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, i as u32));
                }
            }
            t.core = best.map(|b| b.1);
        }
        t
    }

    /// §6t: this topology with one cell inserted at domain index `at` (`cells` is the new
    /// domain, `cells[at]` the new cell), exactly as [`Self::build`] plus
    /// [`Self::radial_frame`] would make it for `cells` with the core cell `core` (the pinned
    /// core's new index, or `None` to take the bounding-box rule). Every list is the old one
    /// with indices at or after `at` shifted by one and the new cell's entries (and its
    /// neighbours' changed ones) put in their sorted places: linear passes, no sort. Returns
    /// `None` where that does not hold cheaply, so the caller rebuilds: a hashed lookup, a cell
    /// outside the lookup box (the box and the core rule would change), or a core that moves.
    /// Lazily built tables start empty, as after `build`.
    pub(crate) fn inserted(
        mut self,
        cells: &[Coord],
        at: usize,
        core: Option<u32>,
    ) -> Option<Self> {
        // §6w: the tables are updated in place (shifted where they hold cell indices, the new
        // entries inserted), so a growing place neither allocates nor copies a second set of
        // whole-domain arrays; the result is the same topology as before.
        let n_old = self.keys.len();
        if cells.len() != n_old + 1 || at >= cells.len() || n_old + 1 >= NO_SLOT as usize {
            return None;
        }
        let c = cells[at];
        let at32 = at as u32;
        let shift = |v: &mut u32| {
            if *v != NO_SLOT && *v >= at32 {
                *v += 1;
            }
        };
        // Lookup: the same box (the new cell must lie in it, so the bounding box, and with it
        // `boxed`'s choice and the core rule's centre, stay as they were).
        let (min, dim) = match &self.lookup {
            Lookup::Boxed { min, dim, .. } => (*min, *dim),
            _ => return None,
        };
        let (rx, ry, rz) = (c.x - min[0], c.y - min[1], c.z - min[2]);
        if rx < 0 || ry < 0 || rz < 0 || rx >= dim[0] || ry >= dim[1] || rz >= dim[2] {
            return None;
        }
        // Core: pinned (given), or the bounding-box rule over the same box, where the new cell
        // can only take over as the strictly nearer cell or the lower index on a tie.
        let old_core = self.core.map(|mut v| {
            shift(&mut v);
            v
        });
        let core = match core {
            Some(pin) => Some(pin),
            None => {
                let m2: [i128; 3] =
                    std::array::from_fn(|k| 2 * i128::from(min[k]) + i128::from(dim[k]) - 1);
                let d2 = |q: Coord| -> i128 {
                    [q.x, q.y, q.z]
                        .into_iter()
                        .enumerate()
                        .map(|(k, v)| {
                            let e = 2 * i128::from(v) - m2[k];
                            e * e
                        })
                        .sum()
                };
                let o = old_core?;
                let (dn, dold) = (d2(c), d2(cells[o as usize]));
                if dn < dold || (dn == dold && at32 < o) {
                    return None; // the core moves: every radial table changes
                }
                Some(o)
            }
        };
        if core != old_core {
            return None;
        }
        if let Lookup::Boxed { slot, .. } = &mut self.lookup {
            slot.iter_mut().for_each(shift);
            slot[((rx * dim[1] + ry) * dim[2] + rz) as usize] = at32;
        }
        self.core = core;
        let nbrs = NB6.map(|d| self.offset(cells, at, d));
        self.zruns = match self.zruns.take() {
            Some(z) => z.inserted(cells, at, &nbrs),
            None => ZRuns::build(cells),
        };
        // The lazy tables: the wide-roll words are per cell in domain order, so the new cell's
        // word is inserted; the inverse tables of `rise` and `ysort` are rebuilt on next use.
        let cell_roll = self.cell_roll.take().map(|(key, mut table)| {
            table.insert(at, wide_roll_cell(key, c));
            (key, table)
        });
        self.cell_roll = cell_roll.map_or_else(std::sync::OnceLock::new, std::sync::OnceLock::from);
        self.rise_pos = std::sync::OnceLock::new();
        self.ypos = std::sync::OnceLock::new();
        let old_cell_up = self.cell_up.take();
        let old_rpos = self.rpos.take();
        self.keys.insert(at, cell_key(c));
        // ysort: (y, x, z) ascending.
        let yk = |i: u32| {
            let q = cells[i as usize];
            (q.y, q.x, q.z)
        };
        self.ysort.iter_mut().for_each(shift);
        let k = self.ysort.partition_point(|&v| yk(v) < yk(at32));
        self.ysort.insert(k, at32);
        // rise: (cell, cell above) by the lower cell's (y descending, x, z); the new cell gains
        // the pair to the cell above it, and the cell below it gains the pair to it.
        let rk = |i: u32| {
            let q = cells[i as usize];
            (std::cmp::Reverse(q.y), q.x, q.z)
        };
        for (a, b) in self.rise.iter_mut() {
            shift(a);
            shift(b);
        }
        if let Some(up) = self.offset(cells, at, (0, 1, 0)) {
            let k = self.rise.partition_point(|&(a, _)| rk(a) < rk(at32));
            self.rise.insert(k, (at32, up as u32));
        }
        if let Some(below) = self.offset(cells, at, (0, -1, 0)) {
            let b = below as u32;
            let k = self.rise.partition_point(|&(a, _)| rk(a) < rk(b));
            self.rise.insert(k, (b, at32));
        }
        // space_faces, in index order: the new cell's mask, and each face neighbour loses the
        // face toward it (dropped when it no longer touches space).
        for (i, _) in self.space_faces.iter_mut() {
            shift(i);
        }
        let mut mask = 0u8;
        for (k, d) in NB6.iter().enumerate() {
            match self.offset(cells, at, *d) {
                None => mask |= 1 << k,
                Some(j) => {
                    // NB6 pairs opposite directions as (0, 1), (2, 3), (4, 5).
                    let back = k ^ 1;
                    let faces = &mut self.space_faces;
                    let pos = faces.partition_point(|&(i, _)| (i as usize) < j);
                    if let Some(e) = faces.get_mut(pos)
                        && e.0 as usize == j
                    {
                        e.1 &= !(1 << back);
                        if e.1 == 0 {
                            faces.remove(pos);
                        }
                    }
                }
            }
        }
        if mask != 0 {
            let pos = self.space_faces.partition_point(|&(i, _)| i < at32);
            self.space_faces.insert(pos, (at32, mask));
        }
        // Radial frame around the same core cell.
        if let Some(co) = core {
            let o = cells[co as usize];
            let r2c = {
                let (dx, dy, dz) = (c.x - o.x, c.y - o.y, c.z - o.z);
                dx * dx + dy * dy + dz * dz
            };
            self.r2.insert(at, r2c);
            let r2 = &self.r2;
            let sk = |i: u32| {
                let q = cells[i as usize];
                (r2[i as usize], q.y, q.x, q.z)
            };
            let rsort = &mut self.rsort;
            rsort.iter_mut().for_each(shift);
            let k0 = rsort.partition_point(|&v| sk(v) < sk(at32));
            rsort.insert(k0, at32);
            let k032 = k0 as u32;
            let kshift = |v: &mut u32| {
                if *v != NO_SLOT && *v >= k032 {
                    *v += 1;
                }
            };
            let up = &mut self.shell_up;
            up.iter_mut().for_each(kshift);
            up.insert(k0, NO_SLOT);
            let rsort = &self.rsort;
            let pos_of = |i: usize| rsort.partition_point(|&v| sk(v) < sk(i as u32));
            let step_of =
                |q: Coord| super::p6_rest::outward_step((q.x - o.x, q.y - o.y, q.z - o.z));
            // The new cell's own outward neighbour ...
            let st = step_of(c);
            let mut own_up = NO_SLOT;
            let mut landed: Vec<usize> = Vec::new();
            let mut up_writes: Vec<(usize, u32)> = Vec::new();
            if st != (0, 0, 0)
                && let Some(j) = self.offset(cells, at, st)
            {
                up_writes.push((k0, pos_of(j) as u32));
                own_up = j as u32;
            }
            // ... and the cells whose outward step lands on it (they had none before).
            for dx in -1_i64..=1 {
                for dy in -1_i64..=1 {
                    for dz in -1_i64..=1 {
                        if (dx, dy, dz) == (0, 0, 0) {
                            continue;
                        }
                        let Some(j) = self.offset(cells, at, (dx, dy, dz)) else {
                            continue;
                        };
                        let q = cells[j];
                        let s = step_of(q);
                        if s != (0, 0, 0) && (q.x + s.0, q.y + s.1, q.z + s.2) == (c.x, c.y, c.z) {
                            up_writes.push((pos_of(j), k032));
                            landed.push(j);
                        }
                    }
                }
            }
            for (k, v) in up_writes {
                self.shell_up[k] = v;
            }
            // The two tables `radial_frame` fills as it builds (`cell_up`, `rsort_pos`), carried
            // over the same way when the old topology had them.
            if let (Some(mut cell_up), Some(mut rpos)) = (old_cell_up, old_rpos) {
                cell_up.iter_mut().for_each(shift);
                cell_up.insert(at, own_up);
                for &j in &landed {
                    cell_up[j] = at32;
                }
                rpos.iter_mut().for_each(kshift);
                rpos.insert(at, k032);
                self.cell_up = std::sync::OnceLock::from(cell_up);
                self.rpos = std::sync::OnceLock::from(rpos);
            }
        }
        Some(self)
    }

    /// §6w: reserves [`growth_room`] in each per-cell table [`Self::inserted`] grows.
    fn reserve_growth(&mut self) {
        let room = growth_room(self.keys.len());
        self.keys.reserve(room);
        self.ysort.reserve(room);
        self.rise.reserve(room);
        self.space_faces.reserve(room);
        self.r2.reserve(room);
        self.rsort.reserve(room);
        self.shell_up.reserve(room);
        if let Some(z) = &mut self.zruns {
            z.live.reserve(room);
        }
        for v in [self.cell_up.get_mut(), self.rpos.get_mut()]
            .into_iter()
            .flatten()
        {
            v.reserve(room);
        }
    }

    fn boxed(cells: &[Coord]) -> Option<Lookup> {
        let first = cells.first()?;
        let mut lo = [first.x, first.y, first.z];
        let mut hi = lo;
        for c in cells {
            for (k, v) in [c.x, c.y, c.z].into_iter().enumerate() {
                lo[k] = lo[k].min(v);
                hi[k] = hi[k].max(v);
            }
        }
        let mut dim = [0i64; 3];
        let mut vol: i64 = 1;
        for k in 0..3 {
            dim[k] = hi[k].checked_sub(lo[k])?.checked_add(1)?;
            vol = vol.checked_mul(dim[k])?;
        }
        let budget = (cells.len() as i64).saturating_mul(8).max(4096);
        if vol > budget || vol > i64::from(u32::MAX - 1) {
            return None;
        }
        let mut slot = vec![NO_SLOT; vol as usize];
        for (i, c) in cells.iter().enumerate() {
            let at = ((c.x - lo[0]) * dim[1] + (c.y - lo[1])) * dim[2] + (c.z - lo[2]);
            slot[at as usize] = i as u32;
        }
        Some(Lookup::Boxed { min: lo, dim, slot })
    }

    /// Fill [`Topology::r2`], [`Topology::rsort`] and [`Topology::shell_up`] from the
    /// (possibly pinned) core cell; clears them with no core.
    pub(crate) fn radial_frame(&mut self, cells: &[Coord]) {
        self.r2.clear();
        self.rsort.clear();
        self.shell_up.clear();
        let Some(o) = self.core else { return };
        let o = cells[o as usize];
        self.r2 = cells
            .iter()
            .map(|c| {
                let (dx, dy, dz) = (c.x - o.x, c.y - o.y, c.z - o.z);
                dx * dx + dy * dy + dz * dz
            })
            .collect();
        // §6t: `ysort` is in (y, x, z) order, so a stable counting sort of it by r2 is the
        // (r2, y, x, z) order (the keys are distinct), when r2 spans a modest range.
        let r2max = self.r2.iter().copied().max().unwrap_or(0);
        let rs: Vec<u32> = if self.ysort.len() == cells.len()
            && r2max >= 0
            && (r2max as usize) <= 4 * cells.len() + 1024
            && !super::p6_rest::full_scans()
        {
            counting_sort(&self.ysort, r2max as usize + 1, |i| {
                self.r2[i as usize] as usize
            })
        } else {
            let mut rs: Vec<u32> = (0..cells.len() as u32).collect();
            rs.sort_by_key(|&i| {
                let c = cells[i as usize];
                (self.r2[i as usize], c.y, c.x, c.z)
            });
            rs
        };
        // §6t: each cell's outward neighbour in domain order (the cells are read in order), then
        // gathered into rsort order; the domain-order list is `cell_up` and the ranks are
        // `rsort_pos`, so both lazy tables are filled here with the values they would build.
        let up_dom: Vec<u32> = (0..cells.len())
            .map(|i| {
                let c = cells[i];
                let st = super::p6_rest::outward_step((c.x - o.x, c.y - o.y, c.z - o.z));
                if st == (0, 0, 0) {
                    return NO_SLOT;
                }
                self.offset(cells, i, st).map_or(NO_SLOT, |j| j as u32)
            })
            .collect();
        let mut rank = vec![NO_SLOT; cells.len()];
        for (k, &i) in rs.iter().enumerate() {
            rank[i as usize] = k as u32;
        }
        self.shell_up = rs
            .iter()
            .map(|&i| {
                let up = up_dom[i as usize];
                if up == NO_SLOT {
                    NO_SLOT
                } else {
                    rank[up as usize]
                }
            })
            .collect();
        self.rsort = rs;
        if !super::p6_rest::full_scans() {
            self.cell_up = std::sync::OnceLock::from(up_dom);
            self.rpos = std::sync::OnceLock::from(rank);
        } else {
            self.cell_up = std::sync::OnceLock::new();
            self.rpos = std::sync::OnceLock::new();
        }
    }

    /// [`Topology::cell_up`]: each domain cell's outward neighbour (built on first use).
    pub(crate) fn cell_up(&self) -> &[u32] {
        self.cell_up.get_or_init(|| {
            let mut up = vec![NO_SLOT; self.rsort.len()];
            for (&i, &ku) in self.rsort.iter().zip(&self.shell_up) {
                if ku != NO_SLOT {
                    up[i as usize] = self.rsort[ku as usize];
                }
            }
            up
        })
    }

    /// [`Topology::rise_pos`]: each cell's position in `rise` (built on first use).
    pub(crate) fn rise_pos(&self) -> &[u32] {
        self.rise_pos.get_or_init(|| {
            let mut pos = vec![NO_SLOT; self.ysort.len()];
            for (k, &(i, _)) in self.rise.iter().enumerate() {
                pos[i as usize] = k as u32;
            }
            pos
        })
    }

    /// Each cell's position in `ysort` (the inverse permutation, built on first use).
    pub(crate) fn ysort_pos(&self) -> &[u32] {
        self.ypos.get_or_init(|| {
            let mut pos = vec![0u32; self.ysort.len()];
            for (k, &i) in self.ysort.iter().enumerate() {
                pos[i as usize] = k as u32;
            }
            pos
        })
    }

    /// [`wide_roll_cell`] of every domain cell under `key`, in domain order; `None` when the
    /// cached table was built for another key (the caller then rolls per cell).
    /// The key of the wide-roll words, when they are built.
    #[cfg(test)]
    pub(crate) fn cell_roll_key(&self) -> Option<u64> {
        self.cell_roll.get().map(|&(k, _)| k)
    }

    pub(crate) fn cell_roll_cells(&self, key: u64, cells: &[Coord]) -> Option<&[u64]> {
        let (k, table) = self.cell_roll.get_or_init(|| {
            let mut table = Vec::with_capacity(cells.len() + growth_room(cells.len()));
            table.extend(cells.iter().map(|&c| wide_roll_cell(key, c)));
            (key, table)
        });
        (*k == key).then_some(table.as_slice())
    }

    /// Each cell's position in `rsort` (the inverse permutation, built on first use).
    pub(crate) fn rsort_pos(&self) -> &[u32] {
        self.rpos.get_or_init(|| {
            let mut pos = vec![0u32; self.rsort.len()];
            for (k, &i) in self.rsort.iter().enumerate() {
                pos[i as usize] = k as u32;
            }
            pos
        })
    }

    /// The `rsort` scan outermost first: `(cell, outward neighbour)` with the
    /// neighbour `None` at the core or where the outward step leaves the domain (the tests
    /// check it against the offset lookups; the scans read `shell_up` directly).
    #[cfg(test)]
    pub(crate) fn rsort_outward_first(&self) -> impl Iterator<Item = (usize, Option<usize>)> + '_ {
        self.rsort
            .iter()
            .zip(&self.shell_up)
            .rev()
            .map(|(&i, &ku)| {
                (
                    i as usize,
                    (ku != NO_SLOT).then(|| self.rsort[ku as usize] as usize),
                )
            })
    }

    /// The outward neighbour of `rsort[k]` as a cell index, or `NO_SLOT`.
    #[cfg(test)]
    pub(crate) fn rsort_up(&self, k: usize) -> u32 {
        let ku = self.shell_up[k];
        if ku == NO_SLOT {
            NO_SLOT
        } else {
            self.rsort[ku as usize]
        }
    }

    /// The cell index of `c`, or `None` when `c` is not a domain cell: the same answer as a
    /// binary search of the sorted cell list, in one box probe.
    pub(crate) fn locate(&self, c: Coord) -> Option<usize> {
        match &self.lookup {
            Lookup::Boxed { min, dim, slot } => {
                let rx = c.x.checked_sub(min[0])?;
                let ry = c.y.checked_sub(min[1])?;
                let rz = c.z.checked_sub(min[2])?;
                if rx < 0 || ry < 0 || rz < 0 || rx >= dim[0] || ry >= dim[1] || rz >= dim[2] {
                    return None;
                }
                let s = slot[((rx * dim[1] + ry) * dim[2] + rz) as usize];
                (s != NO_SLOT).then_some(s as usize)
            }
            Lookup::Hashed(m) => m.get(&c).map(|s| *s as usize),
        }
    }

    /// Domain index of `cells[i] + d`, if that cell is in the domain.
    pub(crate) fn offset(&self, cells: &[Coord], i: usize, d: (i64, i64, i64)) -> Option<usize> {
        let c = cells[i];
        let (x, y, z) = (
            c.x.checked_add(d.0)?,
            c.y.checked_add(d.1)?,
            c.z.checked_add(d.2)?,
        );
        match &self.lookup {
            Lookup::Boxed { min, dim, slot } => {
                let (rx, ry, rz) = (x - min[0], y - min[1], z - min[2]);
                if rx < 0 || ry < 0 || rz < 0 || rx >= dim[0] || ry >= dim[1] || rz >= dim[2] {
                    return None;
                }
                let s = slot[((rx * dim[1] + ry) * dim[2] + rz) as usize];
                (s != NO_SLOT).then_some(s as usize)
            }
            Lookup::Hashed(m) => m.get(&Coord::new(x, y, z)).map(|s| *s as usize),
        }
    }

    /// The 6 face neighbours in planet6 order.
    #[inline]
    pub(crate) fn nb6(&self, cells: &[Coord], i: usize) -> [Option<usize>; 6] {
        if let Lookup::Boxed { min, dim, slot } = &self.lookup {
            // The same six `offset` lookups, with the box index built once. `cells[i]` lies in
            // the box, so `r*` is in `0..dim` and no step below can overflow. A face step
            // leaves the box exactly when `offset`'s range test fails (an absolute coordinate
            // whose `checked_add` overflows is outside the box too).
            let c = cells[i];
            let (rx, ry, rz) = (c.x - min[0], c.y - min[1], c.z - min[2]);
            let (sy, sx) = (dim[2], dim[1] * dim[2]);
            let base = (rx * dim[1] + ry) * dim[2] + rz;
            let at = |inside: bool, k: i64| {
                if !inside {
                    return None;
                }
                let s = slot[k as usize];
                (s != NO_SLOT).then_some(s as usize)
            };
            return [
                at(rx > 0, base - sx),
                at(rx + 1 < dim[0], base + sx),
                at(ry > 0, base - sy),
                at(ry + 1 < dim[1], base + sy),
                at(rz > 0, base - 1),
                at(rz + 1 < dim[2], base + 1),
            ];
        }
        NB6.map(|d| self.offset(cells, i, d))
    }

    /// Face neighbour `k` of cell `i` ([`NB6`] order): `nb6(cells, i)[k]`, the same answer as
    /// `offset(cells, i, NB6[k])`, with the one box probe.
    #[inline]
    pub(crate) fn face(&self, cells: &[Coord], i: usize, k: usize) -> Option<usize> {
        if let Lookup::Boxed { min, dim, slot } = &self.lookup {
            // As in `nb6`: `cells[i]` lies in the box, so no step below can overflow.
            let c = cells[i];
            let (rx, ry, rz) = (c.x - min[0], c.y - min[1], c.z - min[2]);
            let base = (rx * dim[1] + ry) * dim[2] + rz;
            let (inside, at) = match k {
                0 => (rx > 0, base - dim[1] * dim[2]),
                1 => (rx + 1 < dim[0], base + dim[1] * dim[2]),
                2 => (ry > 0, base - dim[2]),
                3 => (ry + 1 < dim[1], base + dim[2]),
                4 => (rz > 0, base - 1),
                _ => (rz + 1 < dim[2], base + 1),
            };
            if !inside {
                return None;
            }
            let s = slot[at as usize];
            return (s != NO_SLOT).then_some(s as usize);
        }
        self.offset(cells, i, NB6[k])
    }

    /// The in-domain cells of the `(2r+1)^3 - 1` cube in planet6 order.
    pub(crate) fn cube(&self, cells: &[Coord], i: usize, r: i64, out: &mut Vec<usize>) {
        out.clear();
        if let Lookup::Boxed { min, dim, slot } = &self.lookup {
            // The same cells in the same order as the `offset` loop below, with the box-relative
            // index built once per axis. `cells[i]` lies in the box and `dim` fits a `u32`, so
            // nothing here can overflow; an absolute coordinate whose `checked_add` would
            // overflow lies outside the box, which the range tests reject just the same.
            let c = cells[i];
            let (rx, ry, rz) = (c.x - min[0], c.y - min[1], c.z - min[2]);
            for dx in -r..=r {
                let x = rx + dx;
                if x < 0 || x >= dim[0] {
                    continue;
                }
                for dy in -r..=r {
                    let y = ry + dy;
                    if y < 0 || y >= dim[1] {
                        continue;
                    }
                    let base = (x * dim[1] + y) * dim[2];
                    for dz in -r..=r {
                        if dx == 0 && dy == 0 && dz == 0 {
                            continue;
                        }
                        let z = rz + dz;
                        if z < 0 || z >= dim[2] {
                            continue;
                        }
                        let s = slot[(base + z) as usize];
                        if s != NO_SLOT {
                            out.push(s as usize);
                        }
                    }
                }
            }
            return;
        }
        for dx in -r..=r {
            for dy in -r..=r {
                for dz in -r..=r {
                    if dx == 0 && dy == 0 && dz == 0 {
                        continue;
                    }
                    if let Some(j) = self.offset(cells, i, (dx, dy, dz)) {
                        out.push(j);
                    }
                }
            }
        }
    }
}

impl ChemWorld {
    /// §6t: keeps the cached topology through one inserted domain cell (index `at`), when
    /// [`Topology::inserted`] can; otherwise drops it, to be rebuilt on next use.
    pub(crate) fn topology_after_insert(&mut self, at: usize) {
        let Some(old) = self.topo.take() else { return };
        let pin = self
            .core_pin
            .and_then(|p| self.cells.binary_search(&p).ok())
            .map(|i| i as u32);
        if self.core_pin.is_some() && pin.is_none() {
            return;
        }
        // The vapor regions built for the old topology carry over (§6w): the same core cell, so
        // every old cell keeps its region.
        // Driven only: there the next tick's `vapor_sync` would build exactly this geometry
        // before anything reads it.
        let driven = self.params.cycle == WaterCycle::Driven;
        let mode = self.params.vapor_local;
        let n = self.cells.len();
        let geo = self.vgeo.take_if(|g| {
            driven && g.for_topology(&old) && g.mode == mode && g.patch.len() + 1 == n
        });
        self.topo = Arc::unwrap_or_clone(old)
            .inserted(&self.cells, at, pin)
            .map(Arc::new);
        if let Some(g) = geo {
            self.vgeo = Some(match &self.topo {
                Some(t) => Arc::new(g.inserted(&self.cells, t, at)),
                None => g, // the topology is rebuilt on next use, and the regions with it
            });
        }
    }

    pub(crate) fn topology(&mut self) -> Arc<Topology> {
        if let Some(t) = &self.topo {
            return t.clone();
        }
        let mut t = Topology::build(&self.cells);
        if let Some(pin) = self.core_pin
            && let Some(i) = self.index_of(pin)
        {
            t.core = Some(i as u32);
        }
        t.radial_frame(&self.cells);
        // §6w: room for a growing domain, so the first places after a (re)build insert in place
        // rather than copying every per-cell array into a larger one.
        t.reserve_growth();
        let room = growth_room(self.cells.len());
        self.cells.reserve(room);
        self.mat.reserve(room);
        self.q.reserve(room);
        let t = Arc::new(t);
        self.topo = Some(t.clone());
        t
    }

    #[inline]
    pub(crate) fn roll(&self, topo: &Topology, salt: u32, i: usize) -> u32 {
        self.roll_at(topo, salt, i, self.tick)
    }

    /// [`Self::roll`] for a scan that rolls many cells under one salt: the salt's part of the
    /// roll is computed once by [`Self::roll_key`].
    #[inline]
    pub(crate) fn roll_keyed(&self, topo: &Topology, key: RollKey, i: usize) -> u32 {
        match key {
            RollKey::Lattice(s) => lattice(s, topo.keys[i], self.tick as u32) % 1000,
            RollKey::Wide(k) => wide_roll_keyed(k, self.cells[i], self.tick) % 1000,
        }
    }

    /// The per-salt part of [`Self::roll`], for [`Self::roll_keyed`].
    pub(crate) fn roll_key(&self, salt: u32) -> RollKey {
        match self.params.cycle {
            WaterCycle::Classic => RollKey::Lattice(self.params.seed ^ salt),
            WaterCycle::Driven => RollKey::Wide(wide_roll_key(self.params.seed ^ salt)),
        }
    }

    /// A roll in `0..1000` on an explicit time key (evaporation uses
    /// `tick >> 3`). Classic: planet6 `lattice` (16-bit keys). Driven:
    /// [`wide_roll`] on the full coordinate and time key.
    #[inline]
    pub(crate) fn roll_at(&self, topo: &Topology, salt: u32, i: usize, b: u64) -> u32 {
        match self.params.cycle {
            WaterCycle::Classic => lattice(self.params.seed ^ salt, topo.keys[i], b as u32) % 1000,
            WaterCycle::Driven => wide_roll(self.params.seed ^ salt, self.cells[i], b) % 1000,
        }
    }

    /// One planet6 tick in planet6 `step()` order: `tick++`, heat, phase,
    /// combine, combine2 (hand families, network law, netBurn, condense, gas
    /// rise), water cycle, erode, dissolve, settle, flow.
    pub fn step(&mut self) -> Result<StepReport, StepError> {
        // Laws flip occupancy bits through `set_mat`; the occupancy world's octree is rebuilt
        // once per changed chunk at the end instead of on every flip (exact, see
        // `World::defer_octree`). No law reads the octree or the world bounds.
        self.world.defer_octree();
        let report = self.step_laws();
        self.world.flush_octree();
        report
    }

    fn step_laws(&mut self) -> Result<StepReport, StepError> {
        self.tick += 1;
        let topo = self.topology();
        if self.params.cycle == WaterCycle::Driven {
            self.vapor_sync(&topo);
        }
        let mut rep = StepReport {
            tick: self.tick,
            ..StepReport::default()
        };
        self.reset_marks();
        if !self.params.energy_in.is_off() {
            (rep.injected, rep.reflected) = self.energy_input_pass(&topo);
        }
        if self.params.heat {
            (rep.leaked, rep.trapped) = self.heat_pass(&topo);
        }
        if self.params.phase {
            self.phase_pass(&mut rep);
        }
        self.combine_pass(&topo, &mut rep);
        self.combine2_pass(&topo, &mut rep)?;
        if self.params.cycle == WaterCycle::Driven {
            self.convect_pass(&topo, &mut rep);
            self.driven_water_pass(&topo, &mut rep);
            self.flow_pass(&topo, &mut rep);
            self.driven_erode_pass(&topo, &mut rep);
            self.driven_dissolve_pass(&topo, &mut rep);
            self.entrain_pass(&topo, &mut rep);
            self.scour_pass(&topo, &mut rep);
            self.weather_pass(&topo, &mut rep);
            self.uplift_pass(&topo, &mut rep);
            self.plume_pass(&topo, &mut rep);
            self.degas_pass(&topo, &mut rep);
            self.collapse_pass(&topo, &mut rep);
            self.settle_pass(&topo, &mut rep);
        } else {
            self.water_cycle_pass(&topo, &mut rep);
            self.erode_pass(&topo, &mut rep);
            self.dissolve_pass(&topo, &mut rep);
            self.settle_pass(&topo, &mut rep);
            self.flow_pass(&topo, &mut rep);
        }
        Ok(rep)
    }

    /// Driven-only fine profile (§5y): ns per pass, in `step()` order:
    /// vapor_sync, sun, heat, phase, combine, combine2, water, flow, erode,
    /// dissolve, entrain, scour, weather, uplift, plume, degas, collapse, settle.
    /// Profiling only; identical state change to [`Self::step`] under Driven.
    #[cfg(not(target_arch = "wasm32"))]
    #[doc(hidden)]
    pub fn step_profiled_fine(&mut self) -> Result<(StepReport, [u64; 18]), StepError> {
        // Same deferred octree upkeep as [`Self::step`], so the stage times match it.
        self.world.defer_octree();
        let profiled = self.step_profiled_fine_laws();
        self.world.flush_octree();
        profiled
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn step_profiled_fine_laws(&mut self) -> Result<(StepReport, [u64; 18]), StepError> {
        use std::time::Instant;
        assert_eq!(self.params.cycle, WaterCycle::Driven);
        let mut ns = [0u64; 18];
        let mut t = Instant::now();
        let mut lap = |k: usize, t: &mut Instant| {
            ns[k] += t.elapsed().as_nanos() as u64;
            *t = Instant::now();
        };
        self.tick += 1;
        let topo = self.topology();
        self.vapor_sync(&topo);
        let mut rep = StepReport {
            tick: self.tick,
            ..StepReport::default()
        };
        self.reset_marks();
        lap(0, &mut t);
        if !self.params.energy_in.is_off() {
            (rep.injected, rep.reflected) = self.energy_input_pass(&topo);
        }
        lap(1, &mut t);
        if self.params.heat {
            (rep.leaked, rep.trapped) = self.heat_pass(&topo);
        }
        lap(2, &mut t);
        if self.params.phase {
            self.phase_pass(&mut rep);
        }
        lap(3, &mut t);
        self.combine_pass(&topo, &mut rep);
        lap(4, &mut t);
        self.combine2_pass(&topo, &mut rep)?;
        self.convect_pass(&topo, &mut rep);
        lap(5, &mut t);
        self.driven_water_pass(&topo, &mut rep);
        lap(6, &mut t);
        self.flow_pass(&topo, &mut rep);
        lap(7, &mut t);
        self.driven_erode_pass(&topo, &mut rep);
        lap(8, &mut t);
        self.driven_dissolve_pass(&topo, &mut rep);
        lap(9, &mut t);
        self.entrain_pass(&topo, &mut rep);
        lap(10, &mut t);
        self.scour_pass(&topo, &mut rep);
        lap(11, &mut t);
        self.weather_pass(&topo, &mut rep);
        lap(12, &mut t);
        self.uplift_pass(&topo, &mut rep);
        lap(13, &mut t);
        self.plume_pass(&topo, &mut rep);
        lap(14, &mut t);
        self.degas_pass(&topo, &mut rep);
        lap(15, &mut t);
        self.collapse_pass(&topo, &mut rep);
        lap(16, &mut t);
        self.settle_pass(&topo, &mut rep);
        lap(17, &mut t);
        Ok((rep, ns))
    }

    /// [`Self::step`] with per-stage wall time in ns (profiling only; identical state change).
    /// Stages: heat, phase, combine, combine2, water, erode, dissolve, settle, flow.
    #[cfg(not(target_arch = "wasm32"))]
    #[doc(hidden)]
    pub fn step_profiled(&mut self) -> Result<(StepReport, [u64; 9]), StepError> {
        // Same deferred octree upkeep as [`Self::step`], so the stage times match it.
        self.world.defer_octree();
        let profiled = self.step_profiled_laws();
        self.world.flush_octree();
        profiled
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn step_profiled_laws(&mut self) -> Result<(StepReport, [u64; 9]), StepError> {
        use std::time::Instant;
        let mut ns = [0u64; 9];
        let mut t = Instant::now();
        let mut lap = |k: usize, t: &mut Instant| {
            ns[k] += t.elapsed().as_nanos() as u64;
            *t = Instant::now();
        };
        self.tick += 1;
        let topo = self.topology();
        if self.params.cycle == WaterCycle::Driven {
            self.vapor_sync(&topo);
        }
        let mut rep = StepReport {
            tick: self.tick,
            ..StepReport::default()
        };
        self.reset_marks();
        if !self.params.energy_in.is_off() {
            (rep.injected, rep.reflected) = self.energy_input_pass(&topo);
        }
        if self.params.heat {
            (rep.leaked, rep.trapped) = self.heat_pass(&topo);
        }
        lap(0, &mut t);
        if self.params.phase {
            self.phase_pass(&mut rep);
        }
        lap(1, &mut t);
        self.combine_pass(&topo, &mut rep);
        lap(2, &mut t);
        self.combine2_pass(&topo, &mut rep)?;
        if self.params.cycle == WaterCycle::Driven {
            self.convect_pass(&topo, &mut rep);
        }
        lap(3, &mut t);
        if self.params.cycle == WaterCycle::Driven {
            self.driven_water_pass(&topo, &mut rep);
            lap(4, &mut t);
            self.flow_pass(&topo, &mut rep);
            lap(8, &mut t);
            self.driven_erode_pass(&topo, &mut rep);
            lap(5, &mut t);
            self.driven_dissolve_pass(&topo, &mut rep);
            self.entrain_pass(&topo, &mut rep);
            self.scour_pass(&topo, &mut rep);
            lap(6, &mut t);
            self.weather_pass(&topo, &mut rep);
            self.uplift_pass(&topo, &mut rep);
            self.plume_pass(&topo, &mut rep);
            self.degas_pass(&topo, &mut rep);
            self.collapse_pass(&topo, &mut rep);
            lap(5, &mut t);
            self.settle_pass(&topo, &mut rep);
            lap(7, &mut t);
        } else {
            self.water_cycle_pass(&topo, &mut rep);
            lap(4, &mut t);
            self.erode_pass(&topo, &mut rep);
            lap(5, &mut t);
            self.dissolve_pass(&topo, &mut rep);
            lap(6, &mut t);
            self.settle_pass(&topo, &mut rep);
            lap(7, &mut t);
            self.flow_pass(&topo, &mut rep);
            lap(8, &mut t);
        }
        Ok((rep, ns))
    }

    /// planet6 `stepHeat` alone (does not advance the tick). Returns the heat
    /// radiated to space.
    pub fn step_heat(&mut self) -> i64 {
        let topo = self.topology();
        if self.params.cycle == WaterCycle::Driven {
            self.vapor_sync(&topo);
        }
        self.heat_pass(&topo).0
    }

    /// planet6 `stepPhase` alone (does not advance the tick).
    pub fn step_phase(&mut self) -> StepReport {
        let mut rep = StepReport::default();
        self.phase_pass(&mut rep);
        rep
    }

    /// M1 booked energy input (see [`EnergyInput`]); returns the heat injected,
    /// which is also added to the `input_q` ledger.
    fn energy_input_pass(&mut self, topo: &Topology) -> (i64, i64) {
        let ein = self.params.energy_in;
        let mut total: i64 = 0;
        let mut reflected: i64 = 0;
        if ein.sun > 0 {
            let (sx, sy, sz, lit) = ein.sun_dir3(self.tick, self.params.sun_sweep);
            if lit {
                // d.s for NB6 = -x, +x, -y, +y, -z, +z
                let dots = [-sx, sx, -sy, sy, -sz, sz];
                let sun = i64::from(ein.sun);
                // |s| of the triangle-wave sun (724..=1024 untilted), so the
                // incidence cosine on a face is d / |s| (unit sun direction).
                let s_len = (sx * sx + sy * sy + sz * sz).isqrt().max(1);
                let wmodel = self.params.water_albedo;
                // Cloud albedo (Driven + greenhouse on): one reflectance per
                // tick per vapor region (its local pool over its own rim
                // cells; one region = the global pool).
                let gated = self.params.cycle == WaterCycle::Driven
                    && self.params.greenhouse != Greenhouse::Off;
                let geo = if gated { self.vgeo.clone() } else { None };
                let clouds: Vec<i64> = match &geo {
                    Some(g) if self.vapor_bins.len() == g.rim.len() => self
                        .vapor_bins
                        .iter()
                        .zip(&g.rim)
                        .map(|(&v, &r)| cloud_reflect(v, r, self.params.cloud))
                        .collect(),
                    _ if gated => vec![cloud_reflect(
                        self.vapor,
                        topo.space_faces.len() as i64,
                        self.params.cloud,
                    )],
                    _ => vec![0],
                };
                let local = clouds.len() > 1;
                let cloud_of = |i: usize| -> i64 {
                    if local {
                        geo.as_ref().map_or(0, |g| clouds[usize::from(g.patch[i])])
                    } else {
                        clouds[0]
                    }
                };
                // rim-weighted mean (exactly the global value with one region)
                self.cloud_last = match &geo {
                    Some(g) if local => {
                        let rt: i64 = g.rim.iter().sum();
                        let s: i128 = clouds
                            .iter()
                            .zip(&g.rim)
                            .map(|(&c, &r)| i128::from(c) * i128::from(r))
                            .sum();
                        if rt > 0 {
                            (s / i128::from(rt)) as i64
                        } else {
                            0
                        }
                    }
                    _ => clouds[0],
                };
                for &(i, mask) in &topo.space_faces {
                    let mut f: i64 = 0;
                    for (k, &d) in dots.iter().enumerate() {
                        if mask & (1 << k) != 0 && d > 0 {
                            f += sun * d / 1024;
                        }
                    }
                    if f == 0 {
                        continue;
                    }
                    let i = i as usize;
                    let cloud = cloud_of(i);
                    let pb = if gated {
                        geo.as_ref().map(|g| usize::from(g.patch[i]))
                    } else {
                        None
                    };
                    if self.params.cycle == WaterCycle::Driven {
                        // Transparent air: each lit face's share rides its own ray
                        // inward through AIR and lands on the first non-AIR cell.
                        for (k, &d) in dots.iter().enumerate() {
                            if mask & (1 << k) == 0 || d <= 0 {
                                continue;
                            }
                            let fk = sun * d / 1024;
                            if fk == 0 {
                                continue;
                            }
                            let Some(t) = self.sun_target(topo, i, k) else {
                                continue;
                            };
                            // Albedo: the reflected share leaves the domain again
                            // and is never injected (so never booked).
                            // Water's albedo depends on the incidence cosine on the
                            // face the ray enters (the ray rides face k's normal,
                            // so that face is the surface it strikes).
                            // Clouds first: the vapor-cloud share goes back to
                            // space before the surface sees the ray (own dither
                            // stream, face k + 8), never injected.
                            let fc = absorbed_share(fk, cloud, self.tick, t, k + 8);
                            self.cloud_q += fk - fc;
                            let mu = (d * ALBEDO_DEN / s_len).min(ALBEDO_DEN);
                            let mt = self.mat[t];
                            if gated {
                                // Diagnostics only: ocean-vs-land cloud split by
                                // the struck cell, and per-region sun/cloud.
                                let wet = mt == MatId::WATER || mt == MatId::ICE;
                                let b = if wet { 0 } else { 2 };
                                self.cloud_split[b] += fk;
                                self.cloud_split[b + 1] += fk - fc;
                                if let Some(pd) = pb.and_then(|p| self.patch_diag.get_mut(p)) {
                                    pd[0] += fk;
                                    if wet {
                                        pd[1] += fk;
                                    }
                                    pd[2] += fk - fc;
                                }
                            }
                            let ak = absorbed_share(fc, albedo_at(mt, mu, wmodel), self.tick, t, k);
                            reflected += fk - ak;
                            if mt == MatId::WATER {
                                // Diagnostic only: [incident, reflected] at high
                                // sun (mu >= 0.5) and at low sun (mu < 0.5), for
                                // the light under the clouds.
                                let b = if mu >= ALBEDO_DEN / 2 { 0 } else { 2 };
                                self.water_sun[b] += fc;
                                self.water_sun[b + 1] += fc - ak;
                            }
                            if ak == 0 {
                                continue;
                            }
                            if let Some(nq) = i32::try_from(ak)
                                .ok()
                                .and_then(|f| self.q[t].checked_add(f))
                            {
                                self.q[t] = nq;
                                total += ak;
                            }
                        }
                        continue;
                    }
                    if let Some(nq) = i32::try_from(f).ok().and_then(|f| self.q[i].checked_add(f)) {
                        self.q[i] = nq;
                        total += f;
                    }
                }
            }
        }
        if ein.core > 0
            && let Some(c) = topo.core
        {
            let c = c as usize;
            if ein.core_r == 0 {
                if let Some(nq) = self.q[c].checked_add(ein.core) {
                    self.q[c] = nq;
                    total += i64::from(ein.core);
                }
            } else {
                total += self.core_sphere_input(topo, c, ein);
            }
        }
        self.input_q += total;
        self.reflected_q += reflected;
        (total, reflected)
    }

    /// Where a sun ray entering cell `i` through its space face `k` (an
    /// [`NB6`] index) is absorbed under transparent air (Driven only): march
    /// from `i` along `-NB6[k]` while the cell is AIR and return the first
    /// non-AIR cell. A ray that crosses only AIR and leaves the domain again
    /// returns `None`: the light passes straight through and is neither
    /// deposited nor booked. Space (out of domain) is transparent by
    /// construction; WATER, ICE, rising gases and every solid absorb.
    pub(crate) fn sun_target(&self, topo: &Topology, i: usize, k: usize) -> Option<usize> {
        let (dx, dy, dz) = NB6[k];
        let step = (-dx, -dy, -dz);
        let mut c = i;
        loop {
            if self.mat[c] != MatId::AIR {
                return Some(c);
            }
            c = topo.offset(&self.cells, c, step)?;
        }
    }

    /// The core-sphere cells of [`EnergyInput`] (`core_r > 0`): every domain
    /// cell within squared distance `r^2` of the centre cell `c`, ascending
    /// domain index (the centre included).
    pub(crate) fn core_sphere(&self, topo: &Topology, c: usize, r: u32) -> Vec<usize> {
        let r = i64::from(r);
        let mut ball: Vec<usize> = Vec::new();
        topo.cube(&self.cells, c, r, &mut ball);
        let o = self.cells[c];
        ball.retain(|&j| {
            let p = self.cells[j];
            let (dx, dy, dz) = (p.x - o.x, p.y - o.y, p.z - o.z);
            dx * dx + dy * dy + dz * dz <= r * r
        });
        ball.push(c);
        ball.sort_unstable();
        ball
    }

    /// Spread `ein.core` over the core sphere (exact integer split, rotating
    /// remainder; see [`EnergyInput`]). Returns the heat that landed (a cell
    /// whose `i32` would overflow is skipped, not booked).
    pub(crate) fn core_sphere_input(&mut self, topo: &Topology, c: usize, ein: EnergyInput) -> i64 {
        let ball = self.core_sphere(topo, c, ein.core_r);
        let n = ball.len() as u64;
        let core = ein.core as u64; // > 0
        let (base, rem) = (core / n, core % n);
        let shift = self.tick % n;
        let mut total: i64 = 0;
        for (k, &j) in ball.iter().enumerate() {
            let add = base + u64::from((k as u64 + shift) % n < rem);
            if add == 0 {
                continue;
            }
            let add = add as i32; // <= core
            if let Some(nq) = self.q[j].checked_add(add) {
                self.q[j] = nq;
                total += i64::from(add);
            }
        }
        total
    }

    /// Metered, exactly conserving 6-neighbour diffusion (gather form) plus the
    /// surface leak into `space_q`. Returns `(leaked, trapped)`.
    ///
    /// **Vapor greenhouse** ([`WaterCycle::Driven`] with a greenhouse on): the
    /// leak is the world's only outbound IR. After planet6's CO2 factor, only
    /// `1 - vapor_trap(vapor, rim)` of each rim cell's leak reaches space
    /// (dithered like albedo, [`absorbed_share`] with face 6). The held-back
    /// part is back-radiated: it moves to the first non-AIR cell inward from
    /// one of the rim cell's space faces ([`ChemWorld::sun_target`]; faces
    /// tried in a tick-rotating order, first column that is not all AIR; the
    /// rim cell itself if it is not AIR or every column is all AIR). The rim cell
    /// sheds exactly what it would have without vapor, so the greenhouse warms
    /// the ground, not the top of the sky. Heat only moves between cells;
    /// nothing is minted, so the energy book closes with no new term
    /// (`trapped` is a diagnostic). Classic is planet6 bit-for-bit.
    fn heat_pass(&mut self, topo: &Topology) -> (i64, i64) {
        let n = self.cells.len();
        let dq = i64::from(self.params.dq);
        let co2n: i64 = match self.params.greenhouse {
            Greenhouse::Off => 0,
            // Counted 255 cells at a time in `u8` lanes (the same count, vectorised).
            _ => self
                .mat
                .chunks(255)
                .map(|c| i64::from(c.iter().map(|m| u8::from(*m == MatId::CO2)).sum::<u8>()))
                .sum(),
        };
        let mut nq = std::mem::take(&mut self.heat_scratch.nq);
        // Each emitting cell adds the same `per` to each face neighbour and keeps the rest;
        // the additions wrap in `i32`, so their order does not matter.
        if let Some(rows) = &topo.zruns {
            // Gathered: every cell's share `per` first, then each cell takes its own rest plus
            // the shares of its face neighbours (the face relation is symmetric), row slice by
            // row slice. Every cell lies in exactly one run, so the first sweep writes all of
            // `nq` and `per`.
            let mut per = std::mem::take(&mut self.heat_scratch.per);
            if nq.len() != n {
                nq.clear();
                nq.resize(n, 0);
            }
            if per.len() != n {
                per.clear();
                per.resize(n, 0);
            }
            // The runs tile the domain in index order, so one sweep over the cells is the same
            // as run by run. When `dq` divides 6000, `q * dq / 6000` truncated is `q / (6000 /
            // dq)` truncated (the same rational), and every quantity fits `i32`: `0 <= p * live
            // <= o <= q` for an emitting cell.
            match self.params.dq {
                d if d > 0 && 6000 % d == 0 => {
                    let divisor = 6000 / d;
                    // All but the rim cells have six neighbours. Do that branch-free whole-
                    // slice case first, then patch just the rim (live < 6) with the original
                    // per-live quotient, before any cell reads a share.
                    // `(q / divisor).max(0) / 6` in one quotient: for `q >= 0` truncation
                    // composes (`q / d / 6 == q / (6 d)`), and for `q < 0` both are 0.
                    if divisor == 20 {
                        heat_shares_six(&self.q, &mut per, &mut nq, |q| q.max(0) / 120);
                    } else {
                        let six = 6 * divisor;
                        heat_shares_six(&self.q, &mut per, &mut nq, |q| q.max(0) / six);
                    }
                    for &(i, _) in &topo.space_faces {
                        let i = i as usize;
                        let o = self.q[i] / divisor;
                        let live = usize::from(rows.live[i]);
                        let p = if o > 0 && live > 0 {
                            div_by_live(i64::from(o), live) as i32
                        } else {
                            0
                        };
                        per[i] = p;
                        nq[i] = self.q[i] - p * live as i32;
                    }
                }
                _ => {
                    for i in 0..n {
                        let qi = self.q[i];
                        let o = i64::from(qi) * dq / 6000; // trunc toward zero, like `| 0`
                        let live = usize::from(rows.live[i]);
                        let p = if o > 0 && live > 0 {
                            div_by_live(o, live)
                        } else {
                            0
                        };
                        per[i] = p as i32;
                        nq[i] = (i64::from(qi) - p * live as i64) as i32;
                    }
                }
            }
            for run in &rows.runs {
                let s = run.start;
                let len = (run.z1 - run.z0 + 1) as usize;
                add_shares(&mut nq[s + 1..s + len], &per[s..s + len - 1]);
                add_shares(&mut nq[s..s + len - 1], &per[s + 1..s + len]);
                for r in rows.beside(run).into_iter().flatten() {
                    let (lo, hi) = (run.z0.max(r.z0), run.z1.min(r.z1));
                    if lo > hi {
                        continue;
                    }
                    let m = (hi - lo + 1) as usize;
                    let a = s + (lo - run.z0) as usize;
                    let b = r.start + (lo - r.z0) as usize;
                    add_shares(&mut nq[a..a + m], &per[b..b + m]);
                }
            }
            self.heat_scratch.per = per;
        } else {
            nq.clear();
            nq.resize(n, 0);
            for i in 0..n {
                let qi = self.q[i];
                let o = i64::from(qi) * dq / 6000; // trunc toward zero, like `| 0`
                let mut deg: i64 = 0;
                if o > 0 {
                    let nbs = topo.nb6(&self.cells, i);
                    let live = nbs.iter().filter(|j| j.is_some()).count() as i64;
                    if live > 0 {
                        let per = o / live;
                        if per != 0 {
                            for j in nbs.into_iter().flatten() {
                                nq[j] = nq[j].wrapping_add(per as i32);
                                deg += per;
                            }
                        }
                    }
                }
                nq[i] = nq[i].wrapping_add((i64::from(qi) - deg) as i32);
            }
        }
        let gh_on =
            self.params.cycle == WaterCycle::Driven && self.params.greenhouse != Greenhouse::Off;
        // Vapor greenhouse per region (its local pool over its own rim cells;
        // one region = the global pool).
        let geo = if gh_on { self.vgeo.clone() } else { None };
        let traps: Vec<i64> = match &geo {
            Some(g) if self.vapor_bins.len() == g.rim.len() && g.rim.len() > 1 => self
                .vapor_bins
                .iter()
                .zip(&g.rim)
                .map(|(&v, &r)| vapor_trap(v, r))
                .collect(),
            _ if gh_on => vec![vapor_trap(self.vapor, topo.space_faces.len() as i64)],
            _ => vec![0],
        };
        let local = traps.len() > 1;
        let mut leaked: i64 = 0;
        let mut trapped: i64 = 0;
        let mut back: Vec<(usize, usize, i64)> = Vec::new();
        // The rim cells (a face on space) in index order, with their space-face masks (bit k
        // set = the NB6[k] face borders space): static for the topology's life.
        for &(i, m) in &topo.space_faces {
            let i = i as usize;
            let v = i64::from(nq[i]);
            let mut leak = i64::from(self.params.leak_q) + v * 2 / 1000;
            if co2n > 0 {
                leak = match self.params.greenhouse {
                    Greenhouse::Off => leak,
                    Greenhouse::Planet6 => {
                        let gh = 1.0 / (1.0 + co2n as f64 / 300.0);
                        js_to_int32(leak as f64 * gh)
                    }
                    Greenhouse::Rational => leak * 300 / (300 + co2n),
                };
            }
            let pb = geo.as_ref().map(|g| usize::from(g.patch[i]));
            let gh_vap = if local {
                pb.map_or(0, |p| traps[p])
            } else {
                traps[0]
            };
            if gh_on
                && leak > 0
                && let Some(pd) = pb.and_then(|p| self.patch_diag.get_mut(p))
            {
                pd[3] += v.min(leak); // diagnostic: leak before the vapor
            }
            if gh_vap > 0 && leak > 0 {
                let l0 = v.min(leak);
                leak = absorbed_share(leak, gh_vap, self.tick, i, 6);
                let held = l0 - v.min(leak);
                if held > 0 {
                    // Back-radiation: the held-back IR still leaves the rim
                    // cell, but downward, to the first non-AIR cell inward from
                    // one of its space faces: faces tried in an order rotating
                    // by tick and cell, first one whose column is not all AIR.
                    let faces: Vec<usize> = (0..6).filter(|k| m & (1 << k) != 0).collect();
                    let r0 = (self.tick as usize + i) % faces.len();
                    let t = (0..faces.len())
                        .find_map(|j| self.sun_target(topo, i, faces[(r0 + j) % faces.len()]))
                        .unwrap_or(i);
                    back.push((i, t, held));
                    trapped += held;
                    if let Some(pd) = pb.and_then(|p| self.patch_diag.get_mut(p)) {
                        pd[4] += held;
                    }
                }
                leak = l0; // the rim cell sheds the same as without vapor
            }
            let l = v.min(leak);
            nq[i] = (v - l) as i32;
            leaked += l;
        }
        for (i, t, held) in back {
            // `held` already left nq[i] with the leak; land it on `t`, or put
            // it back where it was if `t` would overflow.
            leaked -= held;
            match nq[t].checked_add(held as i32) {
                Some(x) => nq[t] = x,
                None => nq[i] += held as i32,
            }
        }
        self.space_q += leaked;
        self.trapped_q += trapped;
        self.heat_scratch.nq = std::mem::replace(&mut self.q, nq);
        (leaked, trapped)
    }

    fn phase_pass(&mut self, rep: &mut StepReport) {
        // Only WATER below FREEZE_AT and ICE above MELT_AT change, and each visit writes
        // only its own cell, so a block's hits are known before its first visit.
        let n = self.mat.len();
        let mut base = 0;
        while base < n {
            let end = (base + 64).min(n);
            if find_mat2(&self.mat[base..end], 0, MatId::WATER, MatId::ICE).is_none() {
                base = end;
                continue;
            }
            let mut bits = hit_mask(&self.mat[base..end], &self.q[base..end], |m, q| {
                (m == MatId::WATER) & (q < FREEZE_AT) | (m == MatId::ICE) & (q > MELT_AT)
            });
            while bits != 0 {
                let i = base + bits.trailing_zeros() as usize;
                bits &= bits - 1;
                self.phase_cell(i, rep);
            }
            base = end;
        }
    }

    #[inline]
    fn phase_cell(&mut self, i: usize, rep: &mut StepReport) {
        let m = self.mat[i];
        if m == MatId::WATER && self.q[i] < FREEZE_AT {
            self.set_mat(i, MatId::ICE);
            self.q[i] = self.q[i].wrapping_add(2 * SL_FUSE);
            rep.froze += 1;
            self.mark(i, super::p6_rest::MARK_PHASE);
        } else if m == MatId::ICE && self.q[i] > MELT_AT {
            self.set_mat(i, MatId::WATER);
            self.q[i] = self.q[i].wrapping_sub(2 * SL_FUSE);
            rep.melted += 1;
            self.mark(i, super::p6_rest::MARK_PHASE);
        }
    }

    #[inline]
    fn add_bind(&mut self, i: usize, m: MatId) {
        let b = self.table.bind(m);
        self.q[i] = self.q[i].wrapping_add(b);
    }

    /// planet6 `stepCombine`: 4 H + 2 O -> WATER.
    fn combine_pass(&mut self, topo: &Topology, rep: &mut StepReport) {
        if !self.params.elements || !self.params.combine {
            return;
        }
        let mut hs = Vec::new();
        let mut cube = Vec::new();
        let mut next = 0;
        while let Some(i) = find_mat(&self.mat, next, MatId::O) {
            next = i + 1;
            if self.q[i] < 300 {
                continue;
            }
            let Some(partner) = topo
                .nb6(&self.cells, i)
                .into_iter()
                .flatten()
                .find(|&j| self.mat[j] == MatId::O && self.q[j] >= 300)
            else {
                continue;
            };
            topo.cube(&self.cells, i, 1, &mut cube);
            hs.clear();
            hs.extend(
                cube.iter()
                    .copied()
                    .filter(|&j| j != partner && self.mat[j] == MatId::H),
            );
            if hs.len() < 4 {
                continue;
            }
            if self.roll(topo, 0xe17e21, i) < 300 {
                self.set_mat(i, MatId::WATER);
                self.set_mat(partner, MatId::AIR);
                for &h in &hs[..4] {
                    self.set_mat(h, MatId::AIR);
                }
                self.add_bind(i, MatId::WATER);
                rep.water += 1;
            }
        }
    }

    fn first_nb6(&self, topo: &Topology, i: usize, f: impl Fn(usize) -> bool) -> Option<usize> {
        topo.nb6(&self.cells, i)
            .into_iter()
            .flatten()
            .find(|&j| f(j))
    }

    /// planet6 `stepCombine2` (everything after its `elements` gate).
    pub(crate) fn combine2_pass(
        &mut self,
        topo: &Topology,
        rep: &mut StepReport,
    ) -> Result<(), StepError> {
        if !self.params.elements {
            return Ok(());
        }
        let mut cube = Vec::new();
        // §6y: no rule below makes C, Ca, N, O, H or STEAM (the burn rewrites a C cell as C),
        // so a scan for an id the domain lacks when the pass starts (or, after the network,
        // which can make any material, when it ends) finds nothing: one fused presence pass
        // stands in for those scans. The test-only full scans skip nothing.
        let present = |mat: &[MatId], ids| {
            if super::p6_rest::full_scans() {
                [true; 6]
            } else {
                mats_present(mat, ids)
            }
        };
        const IDS: [MatId; 6] = [
            MatId::C,
            MatId::CA,
            MatId::N,
            MatId::O,
            MatId::H,
            MatId::STEAM,
        ];
        let needs_scan = self.params.combine || self.params.condense || self.net.is_some();
        let [mut has_c, has_ca, has_n, has_o, has_h, mut has_steam] = if needs_scan {
            present(&self.mat, IDS)
        } else {
            [true; 6]
        };
        let end = self.mat.len();
        let from = |has: bool| if has { 0 } else { end };
        if self.params.combine {
            // C + 2 O -> CO2
            let mut next = from(has_c);
            while let Some(i) = find_mat(&self.mat, next, MatId::C) {
                next = i + 1;
                if self.q[i] < 300 {
                    continue;
                }
                let os: Vec<usize> = topo
                    .nb6(&self.cells, i)
                    .into_iter()
                    .flatten()
                    .filter(|&j| self.mat[j] == MatId::O)
                    .collect();
                if os.len() < 2 {
                    continue;
                }
                if self.roll(topo, 0xc0ffee, i) < 250 {
                    self.set_mat(i, MatId::CO2);
                    self.set_mat(os[0], MatId::AIR);
                    self.set_mat(os[1], MatId::AIR);
                    self.add_bind(i, MatId::CO2);
                    rep.hand[0] += 1;
                }
            }
            // Ca + S + 4 O -> ROCK (O from the +-2 cube)
            let mut next = from(has_ca);
            while let Some(i) = find_mat(&self.mat, next, MatId::CA) {
                next = i + 1;
                if self.q[i] < 300 {
                    continue;
                }
                let Some(sj) = self.first_nb6(topo, i, |j| self.mat[j] == MatId::S) else {
                    continue;
                };
                topo.cube(&self.cells, i, 2, &mut cube);
                let os: Vec<usize> = cube
                    .iter()
                    .copied()
                    .filter(|&j| j != sj && self.mat[j] == MatId::O)
                    .take(4)
                    .collect();
                if os.len() < 4 {
                    continue;
                }
                if self.roll(topo, 0xca504, i) < 200 {
                    self.set_mat(i, MatId::ROCK);
                    self.set_mat(sj, MatId::AIR);
                    for &o in &os {
                        self.set_mat(o, MatId::AIR);
                    }
                    self.add_bind(i, MatId::ROCK);
                    rep.hand[1] += 1;
                }
            }
            // N + 3 H -> NH3. Exact skip: every firing needs three H cells and this
            // scan never makes H, so with fewer than three H in the domain no N cell can
            // fire (the test-only full scans skip only with none).
            let need = if super::p6_rest::full_scans() { 1 } else { 3 };
            let mut h_cells = 0;
            let mut at = from(has_h);
            while h_cells < need
                && let Some(h) = find_mat(&self.mat, at, MatId::H)
            {
                h_cells += 1;
                at = h + 1;
            }
            let mut next = if h_cells >= need { from(has_n) } else { end };
            while let Some(i) = find_mat(&self.mat, next, MatId::N) {
                next = i + 1;
                if self.q[i] < 250 {
                    continue;
                }
                topo.cube(&self.cells, i, 1, &mut cube);
                let hs: Vec<usize> = cube
                    .iter()
                    .copied()
                    .filter(|&j| self.mat[j] == MatId::H)
                    .take(3)
                    .collect();
                if hs.len() < 3 {
                    continue;
                }
                if self.roll(topo, 0xa3a01a, i) < 250 {
                    self.set_mat(i, MatId::NH3);
                    for &h in &hs {
                        self.set_mat(h, MatId::AIR);
                    }
                    self.add_bind(i, MatId::NH3);
                    rep.hand[2] += 1;
                }
            }
            // Ca + C + 3 O -> CARB (C and O from the +-2 cube)
            let mut next = from(has_ca);
            while let Some(i) = find_mat(&self.mat, next, MatId::CA) {
                next = i + 1;
                if self.q[i] < 300 {
                    continue;
                }
                topo.cube(&self.cells, i, 2, &mut cube);
                let Some(cj) = cube.iter().copied().find(|&j| self.mat[j] == MatId::C) else {
                    continue;
                };
                let os: Vec<usize> = cube
                    .iter()
                    .copied()
                    .filter(|&j| j != cj && self.mat[j] == MatId::O)
                    .take(3)
                    .collect();
                if os.len() < 3 {
                    continue;
                }
                if self.roll(topo, 0x1eca42, i) < 200 {
                    self.set_mat(i, MatId::CARB);
                    self.set_mat(cj, MatId::AIR);
                    for &o in &os {
                        self.set_mat(o, MatId::AIR);
                    }
                    self.add_bind(i, MatId::CARB);
                    rep.hand[3] += 1;
                }
            }
            // O + O -> O2
            let mut next = from(has_o);
            while let Some(i) = find_mat(&self.mat, next, MatId::O) {
                next = i + 1;
                if self.q[i] < 250 {
                    continue;
                }
                let Some(p) =
                    self.first_nb6(topo, i, |j| self.mat[j] == MatId::O && self.q[j] >= 250)
                else {
                    continue;
                };
                if self.roll(topo, 0x0202, i) < 150 {
                    self.set_mat(i, MatId::O2);
                    self.set_mat(p, MatId::AIR);
                    self.add_bind(i, MatId::O2);
                    rep.hand[4] += 1;
                }
            }
            // Ca + O -> CAO
            let mut next = from(has_ca);
            while let Some(i) = find_mat(&self.mat, next, MatId::CA) {
                next = i + 1;
                if self.q[i] < 300 {
                    continue;
                }
                let Some(oj) = self.first_nb6(topo, i, |j| self.mat[j] == MatId::O) else {
                    continue;
                };
                if self.roll(topo, 0xca061, i) < 120 {
                    self.set_mat(i, MatId::CAO);
                    self.set_mat(oj, MatId::AIR);
                    self.add_bind(i, MatId::CAO);
                    rep.hand[5] += 1;
                }
            }
        }

        if let Some(net) = self.net.clone() {
            if self.params.net_chem && net.mounted_rows() > 0 {
                let (fired, rounds) = self.network_pass(topo, &net)?;
                rep.net_fired = fired;
                rep.net_rounds = rounds;
                // §6aa: without a firing the network wrote neither a material nor heat, so
                // the presence bits from before it are still true. Test-only full scans still
                // re-check (and report every id present).
                if fired != 0 || super::p6_rest::full_scans() {
                    let [c, .., steam] = present(&self.mat, IDS);
                    (has_c, has_steam) = (c, steam);
                }
            }
            if self.params.net_burn && net.net_row().is_some() {
                let mut next = from(has_c);
                while let Some(i) = find_mat(&self.mat, next, MatId::C) {
                    next = i + 1;
                    if self.q[i] < 250 {
                        continue;
                    }
                    topo.cube(&self.cells, i, 1, &mut cube);
                    let mut cs = [i, 0];
                    let mut nc = 1;
                    let mut os = [0usize; 2];
                    let mut no = 0;
                    for &j in &cube {
                        if nc >= 2 && no >= 2 {
                            break;
                        }
                        if self.mat[j] == MatId::C && nc < 2 {
                            cs[nc] = j;
                            nc += 1;
                        } else if self.mat[j] == MatId::O && no < 2 {
                            os[no] = j;
                            no += 1;
                        }
                    }
                    if nc < 2 || no < 2 {
                        continue;
                    }
                    if self.roll(topo, 0xb0124e, i) < 180 {
                        self.set_mat(i, MatId::CO2);
                        self.set_mat(cs[1], MatId::C);
                        self.set_mat(os[0], MatId::AIR);
                        self.set_mat(os[1], MatId::AIR);
                        self.add_bind(i, MatId::CO2);
                        rep.net_burn += 1;
                    }
                }
            }
        }

        if self.params.condense {
            let mut next = from(has_steam);
            while let Some(i) = find_mat(&self.mat, next, MatId::STEAM) {
                next = i + 1;
                if self.q[i] >= 150 {
                    continue;
                }
                let Some(j) =
                    self.first_nb6(topo, i, |j| self.mat[j] == MatId::STEAM && self.q[j] < 150)
                else {
                    continue;
                };
                // planet6 re-rolls the same (seed, i, tick) for later partners,
                // so the first partner decides.
                if self.roll(topo, 0xc00d12, i) < 300 {
                    self.set_mat(i, MatId::WATER);
                    self.set_mat(j, MatId::AIR);
                    rep.condensed += 1;
                }
            }
        }

        if self.params.gas_rise {
            // §5w: under Gravity::Core gas rises along the outward step,
            // outermost first (planet6's +y list otherwise)
            // (the outward neighbours are precomputed in `Topology::shell_up`)
            if self.grav_core(topo).is_some() {
                self.gas_rise_outward(topo, rep);
            } else if !super::p6_rest::full_scans() {
                self.gas_rise_up(topo, rep);
            } else {
                for &(i, up) in &topo.rise {
                    let (i, up) = (i as usize, up as usize);
                    if !self.is_rising(self.mat[i]) {
                        continue;
                    }
                    let above = self.mat[up];
                    if above != MatId::AIR && above != MatId::WATER {
                        continue;
                    }
                    // §5u: displaced WATER may not be pushed down into a radial-only shell cell
                    if above == MatId::WATER && self.shell_blocked(i) {
                        continue;
                    }
                    if self.roll(topo, 0xb0b81e, i) < 500 {
                        let below = self.mat[i];
                        self.set_mat(up, below);
                        self.set_mat(i, above);
                        rep.rose += 1;
                    }
                }
            }
        }
        Ok(())
    }

    /// Whether a material rises (planet6 `RISING`): STEAM and O2 always, plus
    /// every mounted material whose table species is a gas.
    pub fn is_rising(&self, m: MatId) -> bool {
        match &self.net {
            Some(net) => net.is_rising(m),
            None => m == MatId::STEAM || m == MatId::O2,
        }
    }

    /// planet6 `fingerprint`: FNV-1a over 3 bytes of each `MAT`, `Q & 0xffff`
    /// (domain order), then each bag lane.
    pub fn fingerprint(&self) -> u32 {
        let mut h: u32 = 0x811c_9dc5;
        let mut mix = |v: u32| {
            for s in [0, 8, 16] {
                h ^= (v >> s) & 0xff;
                h = h.wrapping_mul(0x0100_0193);
            }
        };
        for (m, q) in self.mat.iter().zip(&self.q) {
            mix(u32::from(m.0));
            mix((*q as u32) & 0xffff);
        }
        for k in 0..super::NE {
            mix(self.inv[k] as i32 as u32);
        }
        h
    }
}

/// JS `x | 0` for a finite double: truncate toward zero, then wrap to i32.
fn js_to_int32(x: f64) -> i64 {
    let t = x.trunc();
    let m = t.rem_euclid(4_294_967_296.0);
    let v = m as i64;
    i64::from(if v >= 2_147_483_648 {
        (v - 4_294_967_296) as i32
    } else {
        v as i32
    })
}
