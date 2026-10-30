//! Element budget, genesis, conservation books, and the planet6 tick for a
//! BitBlock world.
//!
//! This is the gameladder planet chemistry (`games/planet/core.js`: `SYM`,
//! `COMP`, `BIND`, `INV`, `laneTotals`, `energyTotal`, `stepHeat`,
//! `stepPhase`, `stepCombine`, `stepCombine2`, `stepWaterCycle`, `stepErode`,
//! `stepDissolve`, `stepSettle`, `stepFlow`) ported to integer Rust. The
//! contract half (lanes, genesis, books) lives here; the tick lives in
//! [`p6`](self) (`chem/p6.rs`, heat/phase/reactions) and `chem/p6_rest.rs`
//! (the five surface laws), and the generated-network loader plus the v5
//! firing law in `chem/net.rs`. See `BITBLOCK_P6_TICK.md` for the API, the
//! parameters and the parity gaps.
//!
//! # What lives where
//!
//! BitBlock's [`World`] is sparse occupancy bits and nothing else. On-bits are
//! *not* chemistry. [`ChemWorld`] therefore keeps a **dense chemistry domain**
//! beside the occupancy world: a sorted list of cells (planet6's `MASK`), and
//! per cell a material (`MAT`, `AIR` allowed) and a heat `Q` (`i32`, wrapping
//! like planet6's `Int32Array`). Air cells are part of the domain: heat flows
//! through them and they can receive reaction products, exactly as planet6's
//! sphere cells do. Outside the domain is "space" (planet6's off-mask cells):
//! surface cells leak heat into the `space_q` book.
//!
//! Plus an inventory bag (`inv`, planet6's `INV`) holding atoms that are not
//! in any voxel.
//!
//! Invariants checked by [`ChemWorld::check_books`]:
//! * every non-air domain cell is on in the occupancy world and has a
//!   chemistry row;
//! * `lane_totals() == t0`, i.e. `lane_drift()` is all zero;
//! * `energy_total() == t0_energy`, i.e. `energy_drift()` is zero, where
//!   `energy_total = sum(Q) - sum(BIND over non-air) + space_q
//!   + vapor * (LATENT_PAIR - BIND[WATER]) - input_q`; lane totals count each
//!   vapor pair as 4 H + 2 O. `input_q` is the inbound ledger of the M1
//!   booked energy input ([`EnergyInput`], off by default, then always 0).
//!
//! On-bits with no non-air material (the permanent bedrock bit, or bits a
//! caller set through the raw `World`) carry no atoms. The bedrock bit may
//! also hold a material; digging it moves the atoms to the bag while the bit
//! stays on, because bedrock can never be turned off.
//!
//! # Genesis
//!
//! [`genesis`] takes a [`GenesisBudget`] (atoms per element lane), a seed and a
//! [`GenesisShape`], and spends the budget *exactly*: cells of the shape are
//! visited inner-to-outer, each receives the band material for its depth if the
//! remaining budget affords its composition (else the first affordable fallback,
//! ending in single-atom element blocks), and anything left when the shape is
//! full settles into the inventory bag. Every cell of the shape joins the
//! domain (unfilled and hollow cells as air). Heat is the integer radial
//! profile of [`P6Params`]. `t0` (lanes and energy) is then recorded and the
//! books are verified before the world is returned.

use std::fmt;
use std::sync::Arc;

use crate::world::{Coord, World, is_bedrock};

mod net;
mod p6;
mod p6_rest;
#[cfg(test)]
mod tests_p6;
mod vapor;

pub use net::{ChemNet, ChemNetError, NetReaction, NetRow, NetSpecies, NetTable, PackedRow};
pub use p6::{
    ALBEDO_DEN, ALBEDO_ICE, ALBEDO_WATER, CLOUD_DEN, CLOUD_HALF, CLOUD_MAX, CloudAlbedo,
    EnergyInput, FIRE_MAX_ROUNDS, GH_VAPOR_DEN, GH_VAPOR_HALF, GH_VAPOR_MAX, Greenhouse, P6Params,
    StepError, StepReport, SunSweep, WATER_ALBEDO_GRAZE, WATER_ALBEDO_NADIR, WATER_ALBEDO_POW,
    WaterAlbedo, WaterCycle, albedo, albedo_at, cloud_reflect, lattice, vapor_trap, water_albedo,
    wide_roll,
};
pub use p6_rest::{LATENT_PAIR, RAIN_END, RAIN_START, SEASON, STORM_LEN};
pub use vapor::{
    EvapTop, Gravity, RainFall, VAPOR_MIX_TAU, VAPOR_PATCH_N, VaporLocality, vapor_patch,
};

/// Element lane symbols, in planet6 lane order.
pub const SYM: [&str; NE] = [
    "H", "O", "C", "N", "S", "Ca", "Fe", "Ni", "Si", "Mg", "Na", "K",
];
/// Number of element lanes.
pub const NE: usize = 12;
/// Atomic numbers of the lanes (planet6 `EZ`).
pub const LANE_Z: [u8; NE] = [1, 8, 6, 7, 16, 20, 26, 28, 14, 12, 11, 19];

/// Per-lane atom counts.
pub type Lanes = [i64; NE];

/// A material id. Ids are identical to planet6 so the reaction tables index
/// the same numbers; element blocks are `100 + Z`, minted network materials
/// are `41..`.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MatId(pub u8);

impl MatId {
    pub const AIR: Self = Self(0);
    pub const ROCK: Self = Self(1); // CaSO4
    pub const WATER: Self = Self(2); // dihydrol pair 2 x H2O
    pub const ICE: Self = Self(3); // 2 x H2O, solid
    pub const GYP: Self = Self(4); // CaSO4.2H2O
    pub const MIN: Self = Self(5); // rock polymorph
    pub const SED: Self = Self(6); // rock polymorph
    pub const CO2: Self = Self(7);
    pub const NH3: Self = Self(8);
    pub const CARB: Self = Self(9); // CaCO3
    pub const O2: Self = Self(10);
    pub const CAO: Self = Self(11);
    pub const STEAM: Self = Self(12); // one H2O
    pub const H2S: Self = Self(13);
    pub const SO: Self = Self(14);
    pub const CAS: Self = Self(15);
    pub const CS2: Self = Self(16);
    pub const CH4: Self = Self(39);
    pub const CACO: Self = Self(40);

    pub const H: Self = Self::element(0);
    pub const O: Self = Self::element(1);
    pub const C: Self = Self::element(2);
    pub const N: Self = Self::element(3);
    pub const S: Self = Self::element(4);
    pub const CA: Self = Self::element(5);

    /// The single-atom element block for a lane (`100 + Z`).
    pub const fn element(lane: usize) -> Self {
        Self(100 + LANE_Z[lane])
    }
}

const fn unit(lane: usize) -> Lanes {
    let mut c = [0; NE];
    c[lane] = 1;
    c
}

/// The planet6 hand `COMP`/`BIND`/`NAMES` rows for ids 0..=40, verbatim
/// (core.js lines 66-125): (id, name, composition in lane order, BIND).
/// Element blocks are generated (BIND 0). Network-minted ids (41..) live
/// only in a [`MatTable`] built by [`ChemNet::mount`].
const COMPOUNDS: [(MatId, &str, Lanes, i32); 41] = [
    (MatId(0), "air", [0; NE], 0),
    (MatId(1), "rock", [0, 4, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0], 36),
    (MatId(2), "water", [4, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 36),
    (MatId(3), "ice", [4, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 52),
    (MatId(4), "gypsum", [4, 6, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0], 72),
    (
        MatId(5),
        "mineral",
        [0, 4, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0],
        36,
    ),
    (
        MatId(6),
        "sediment",
        [0, 4, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0],
        36,
    ),
    (MatId(7), "co2", [0, 2, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0], 30),
    (MatId(8), "nh3", [3, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0], 12),
    (
        MatId(9),
        "limestone",
        [0, 3, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0],
        30,
    ),
    (MatId(10), "o2", [0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 6),
    (
        MatId(11),
        "quicklime",
        [0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0],
        7,
    ),
    (MatId(12), "steam", [2, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 18),
    (MatId(13), "h2s", [2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0], 8),
    (MatId(14), "so", [0, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0], 12),
    (MatId(15), "cas", [0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0], 12),
    (MatId(16), "cs2", [0, 0, 1, 0, 2, 0, 0, 0, 0, 0, 0, 0], 18),
    (MatId(17), "cah2", [2, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0], 10),
    (MatId(18), "s2o2", [0, 2, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0], 24),
    (MatId(19), "h2so", [2, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0], 16),
    (MatId(20), "h2so2", [2, 2, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0], 24),
    (MatId(21), "h2s2o", [2, 1, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0], 20),
    (MatId(22), "caso2", [0, 2, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0], 28),
    (MatId(23), "caso", [0, 1, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0], 20),
    (
        MatId(24),
        "cah2so",
        [2, 1, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0],
        24,
    ),
    (MatId(25), "cah2o", [2, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0], 18),
    (
        MatId(26),
        "slakelime",
        [2, 2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0],
        28,
    ),
    (MatId(27), "cah2s", [2, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0], 14),
    (MatId(28), "cos", [0, 1, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0], 23),
    (MatId(29), "cso2", [0, 2, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0], 32),
    (MatId(30), "cso3", [0, 3, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0], 40),
    (MatId(31), "cs2o2", [0, 2, 1, 0, 2, 0, 0, 0, 0, 0, 0, 0], 36),
    (MatId(32), "cs2o", [0, 1, 1, 0, 2, 0, 0, 0, 0, 0, 0, 0], 27),
    (MatId(33), "cs3o", [0, 1, 1, 0, 3, 0, 0, 0, 0, 0, 0, 0], 31),
    (MatId(34), "ch2so", [2, 1, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0], 27),
    (MatId(35), "ch2s", [2, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0], 18),
    (
        MatId(36),
        "formaldehyde",
        [2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        21,
    ),
    (
        MatId(37),
        "formicacid",
        [2, 2, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        30,
    ),
    (MatId(38), "ch2s2", [2, 0, 1, 0, 2, 0, 0, 0, 0, 0, 0, 0], 22),
    (
        MatId(39),
        "methane",
        [4, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        14,
    ),
    (MatId(40), "caco", [0, 1, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0], 26),
];

/// Composition of a hand-table material, or `None` for an id with no
/// chemistry row (planet6's "wire-totalness guard": such a material is
/// refused, never placed). Network-minted ids answer through a world's
/// [`MatTable`] instead.
pub fn comp(mat: MatId) -> Option<Lanes> {
    if mat.0 >= 100 {
        let z = mat.0 - 100;
        return LANE_Z.iter().position(|&lz| lz == z).map(unit);
    }
    COMPOUNDS
        .iter()
        .find(|(id, ..)| *id == mat)
        .map(|(_, _, c, _)| *c)
}

/// Human name of a hand-table material, if it has a chemistry row.
pub fn name(mat: MatId) -> Option<&'static str> {
    if mat.0 >= 100 {
        let z = mat.0 - 100;
        return LANE_Z.iter().position(|&lz| lz == z).map(|l| SYM[l]);
    }
    COMPOUNDS
        .iter()
        .find(|(id, ..)| *id == mat)
        .map(|(_, n, ..)| *n)
}

/// Binding energy (planet6 `BIND`) of a hand-table material; elements are 0.
pub fn bind(mat: MatId) -> Option<i32> {
    if mat.0 >= 100 {
        return comp(mat).map(|_| 0);
    }
    COMPOUNDS
        .iter()
        .find(|(id, ..)| *id == mat)
        .map(|(.., b)| *b)
}

/// One chemistry row of a [`MatTable`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MatRow {
    pub comp: Lanes,
    pub bind: i32,
    pub name: String,
}

/// Number of addressable material ids.
pub const MAT_SLOTS: usize = 256;

/// A material table: the hand rows plus (optionally) network-minted rows.
/// The `base()` table answers exactly like [`comp`]/[`bind`]/[`name`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MatTable {
    rows: Vec<Option<MatRow>>,
}

impl Default for MatTable {
    fn default() -> Self {
        Self::base()
    }
}

impl MatTable {
    /// The hand table: ids 0..=40 plus the 12 element blocks.
    pub fn base() -> Self {
        let mut rows = vec![None; MAT_SLOTS];
        for (id, n, c, b) in COMPOUNDS {
            rows[id.0 as usize] = Some(MatRow {
                comp: c,
                bind: b,
                name: n.to_string(),
            });
        }
        for lane in 0..NE {
            rows[MatId::element(lane).0 as usize] = Some(MatRow {
                comp: unit(lane),
                bind: 0,
                name: SYM[lane].to_string(),
            });
        }
        Self { rows }
    }

    pub fn row(&self, m: MatId) -> Option<&MatRow> {
        self.rows[m.0 as usize].as_ref()
    }

    pub fn comp(&self, m: MatId) -> Option<Lanes> {
        self.row(m).map(|r| r.comp)
    }

    /// BIND of a material; 0 for air and for ids with no row.
    pub fn bind(&self, m: MatId) -> i32 {
        self.row(m).map_or(0, |r| r.bind)
    }

    pub fn name(&self, m: MatId) -> Option<&str> {
        self.row(m).map(|r| r.name.as_str())
    }

    /// Number of ids with a row, air included (planet6 `NAMES` key count).
    pub fn len(&self) -> usize {
        self.rows.iter().filter(|r| r.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(crate) fn set(&mut self, m: MatId, row: MatRow) {
        self.rows[m.0 as usize] = Some(row);
    }
}

/// Atoms per element lane that a genesis must spend exactly.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenesisBudget {
    pub lanes: Lanes,
}

impl GenesisBudget {
    pub const fn new(lanes: Lanes) -> Self {
        Self { lanes }
    }

    pub const fn zero() -> Self {
        Self { lanes: [0; NE] }
    }

    /// Sum of all lanes, rejecting negative lanes and overflow.
    pub fn total_atoms(&self) -> Result<i64, GenesisError> {
        let mut total: i64 = 0;
        for (lane, &n) in self.lanes.iter().enumerate() {
            if n < 0 {
                return Err(GenesisError::NegativeLane { lane, value: n });
            }
            total = total.checked_add(n).ok_or(GenesisError::Overflow)?;
        }
        Ok(total)
    }

    /// planet2's genesis ASH (free atoms per cell: H320 O200 C70 N50 S14 Ca14)
    /// scaled by `cells`, on the first six lanes.
    pub fn ash(cells: i64) -> Result<Self, GenesisError> {
        const ASH: [i64; 6] = [320, 200, 70, 50, 14, 14];
        if cells < 0 {
            return Err(GenesisError::NegativeLane {
                lane: 0,
                value: cells,
            });
        }
        let mut lanes = [0; NE];
        for (k, a) in ASH.iter().enumerate() {
            lanes[k] = a.checked_mul(cells).ok_or(GenesisError::Overflow)?;
        }
        Ok(Self { lanes })
    }
}

/// The region a genesis may fill: a solid ball (optionally hollow) around a centre.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenesisShape {
    pub center: Coord,
    /// Cells with `dx^2 + dy^2 + dz^2 <= radius^2` are candidates.
    pub radius: u32,
    /// Cells with squared distance `<= hollow^2` stay air (planet6 `P.hollow`).
    /// `None` fills to the centre.
    pub hollow: Option<u32>,
}

/// Largest radius genesis will enumerate (about 4.6M candidate cells).
pub const MAX_GENESIS_RADIUS: u32 = 103;
/// Default cap on the total number of atoms in a budget.
pub const DEFAULT_ATOM_CAP: i64 = 1 << 40;

/// Everything a genesis needs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenesisConfig {
    pub budget: GenesisBudget,
    pub seed: u64,
    pub shape: GenesisShape,
    /// Reject budgets whose total atom count exceeds this.
    pub atom_cap: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenesisError {
    NegativeLane {
        lane: usize,
        value: i64,
    },
    Overflow,
    OverAtomCap {
        total: i64,
        cap: i64,
    },
    RadiusTooLarge {
        radius: u32,
        max: u32,
    },
    ShapeOutOfRange,
    /// Books did not balance at the end of genesis (a bug, never expected).
    Drift {
        drift: Lanes,
    },
}

impl fmt::Display for GenesisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NegativeLane { lane, value } => {
                write!(
                    f,
                    "budget lane {} ({}) is negative: {value}",
                    lane, SYM[*lane]
                )
            }
            Self::Overflow => write!(f, "budget arithmetic overflowed"),
            Self::OverAtomCap { total, cap } => {
                write!(f, "budget of {total} atoms exceeds cap {cap}")
            }
            Self::RadiusTooLarge { radius, max } => {
                write!(f, "genesis radius {radius} exceeds {max}")
            }
            Self::ShapeOutOfRange => write!(f, "genesis shape leaves the i64 coordinate range"),
            Self::Drift { drift } => write!(f, "genesis books drifted: {drift:?}"),
        }
    }
}

impl std::error::Error for GenesisError {}

/// Why a books check failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BooksError {
    /// A non-air material sits on a coordinate that is off.
    MaterialOnOffBit(Coord),
    /// A material has no chemistry row in the world's table.
    BadMaterial(Coord, MatId),
    /// The inventory bag went negative in some lane.
    NegativeInventory { lane: usize },
    /// `lane_totals() != t0`.
    Drift(Lanes),
    /// `energy_total() != t0_energy` (the value is the drift).
    EnergyDrift(i64),
    /// A cell list handed to [`ChemWorld::from_cells`] repeats a coordinate.
    DuplicateCell(Coord),
}

/// A BitBlock occupancy world plus a dense chemistry domain, heat field and
/// atom/energy books.
#[derive(Clone, Debug)]
pub struct ChemWorld {
    world: World,
    /// Domain cells, sorted by coordinate (planet6 `cells`, in index order).
    cells: Vec<Coord>,
    mat: Vec<MatId>,
    q: Vec<i32>,
    inv: Lanes,
    t0: Lanes,
    t0_energy: i64,
    space_q: i64,
    /// M1 inbound energy ledger: total heat the booked energy input has put
    /// into cell `Q` (0 unless [`P6Params::energy_in`] is on).
    pub(crate) input_q: i64,
    /// Driven-sun albedo diagnostic: total sunlight reflected back to space
    /// (never injected, so not in any book or the fingerprint).
    pub(crate) reflected_q: i64,
    /// Driven-sun water-albedo diagnostic: sunlight reaching WATER and the
    /// part it reflected, `[incident_hi, reflected_hi, incident_lo,
    /// reflected_lo]` split at incidence cosine 0.5 (outside every book and
    /// the fingerprint).
    pub(crate) water_sun: [i64; 4],
    /// Driven vapor-greenhouse diagnostic: total outbound IR held back in rim
    /// cells (never minted; not in any book or the fingerprint).
    pub(crate) trapped_q: i64,
    /// Driven cloud-albedo diagnostics: total sunlight the vapor clouds sent
    /// back to space (never injected), and the cloud reflectance (1/1024ths)
    /// of the last lit tick. Outside every book and the fingerprint.
    pub(crate) cloud_q: i64,
    pub(crate) cloud_last: i64,
    /// planet6 `vapor`: dihydrol pairs in the sky (each = one WATER voxel's
    /// atoms, its binding and [`LATENT_PAIR`] of stored heat).
    vapor: i64,
    /// Local-vapor attribution of `vapor` by region (Driven only; invariant
    /// `sum == vapor`, see [`VaporLocality`]). Not in any book or the
    /// fingerprint (the books read `vapor`).
    pub(crate) vapor_bins: Vec<i64>,
    pub(crate) vgeo: Option<Arc<vapor::VaporGeo>>,
    /// Local-vapor diagnostics (outside every book and the fingerprint):
    /// sun under/at the cloud stage split by the struck cell,
    /// `[incident_wet, cloud_wet, incident_dry, cloud_dry]` (wet = WATER or
    /// ICE), and per region `[incident, incident_wet, cloud, leak0, trapped]`.
    pub(crate) cloud_split: [i64; 4],
    /// Local vapor: drops a region could not land on its own ground and
    /// spilled onto the planet's landing line (diagnostic, not in any book).
    pub(crate) rain_spilled: u64,
    /// Local vapor, [`RainFall::Core`]: drops a region with no landing ground
    /// handed to the nearest regions (region-graph hops) that have ground,
    /// and the summed hop count (diagnostics, not in any book).
    pub(crate) rain_shed: u64,
    pub(crate) rain_shed_hops: u64,
    /// §5v diagnostic (not in any book): Driven `[evap_lower, evap_all,
    /// rain_lower, rain_all]`, lower = the cell's y below the core cell's y.
    pub(crate) hemi_diag: [u64; 4],
    pub(crate) patch_diag: Vec<[i64; 5]>,
    /// planet6 `evaporated` / `rained` counters (not in any book).
    evaporated: u64,
    rained: u64,
    tick: u64,
    params: P6Params,
    table: Arc<MatTable>,
    net: Option<Arc<ChemNet>>,
    topo: Option<Arc<p6::Topology>>,
    /// Per-tick surface-activity marks ([`WaterCycle::Driven`] only; not in
    /// any book or the fingerprint; cleared at the start of every tick).
    pub(crate) marks: Vec<u8>,
    /// Sky-shell headroom in cells added by [`ChemWorld::add_sky_shell`] (0 =
    /// none, the planet6 full-ball domain). Not in any book or the fingerprint.
    pub(crate) sky_height: u32,
    /// Pinned M1 core-input cell (set by [`ChemWorld::add_sky_shell`] so the
    /// lopsided shell does not move the bounding-box core). `None` = planet6
    /// bounding-box rule.
    pub(crate) core_pin: Option<Coord>,
    /// Full radial sky shell geometry `(centre, R, H)` (§5u), set by
    /// [`ChemWorld::add_radial_sky_shell`]; `None` for the full ball and the
    /// polar shell. Not in any book or the fingerprint.
    pub(crate) radial_shell: Option<(Coord, i64, i64)>,
    /// Radial shell guard (§5u): with a radial shell, `flow`, `settle`,
    /// `scour`, `dissolve` and gas rise never *move* material into a
    /// radial-only shell cell (outside the genesis ball and outside the polar
    /// `+y` columns of [`ChemWorld::add_sky_shell`]). Those cells are headroom
    /// for sun, vapor, heat, rain landing and evaporation, but for `-y`
    /// gravity they keep the unpadded ball's "space is a floor" rule, so the
    /// flanks and underside cannot drain. The ball interior and the polar
    /// columns keep §5t behaviour exactly. (A first cut that only blocked
    /// steps with a positive dot on the outward lattice step let the upper
    /// ocean slide down the equatorial band onto the dark underside.) On by
    /// default with a radial shell under world `-y` gravity; [`ChemWorld::set_outward_guard`] is the A/B
    /// handle. §5w: `None` = auto (on iff surface gravity is world `-y`, off
    /// under Driven [`Gravity::Core`], where nothing drains outward and the
    /// guard is redundant); `Some(b)` = forced by `set_outward_guard(b)`.
    pub(crate) outward_guard: Option<bool>,
}

impl Default for ChemWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl ChemWorld {
    /// A bedrock-only world with no atoms and an empty domain; t0 is zero.
    pub fn new() -> Self {
        Self {
            world: World::new(),
            cells: Vec::new(),
            mat: Vec::new(),
            q: Vec::new(),
            inv: [0; NE],
            t0: [0; NE],
            t0_energy: 0,
            space_q: 0,
            input_q: 0,
            reflected_q: 0,
            water_sun: [0; 4],
            trapped_q: 0,
            cloud_q: 0,
            cloud_last: 0,
            vapor: 0,
            vapor_bins: Vec::new(),
            vgeo: None,
            cloud_split: [0; 4],
            rain_spilled: 0,
            rain_shed: 0,
            rain_shed_hops: 0,
            hemi_diag: [0; 4],
            patch_diag: Vec::new(),
            evaporated: 0,
            rained: 0,
            tick: 0,
            params: P6Params::default(),
            table: Arc::new(MatTable::base()),
            net: None,
            topo: None,
            marks: Vec::new(),
            sky_height: 0,
            core_pin: None,
            radial_shell: None,
            outward_guard: None,
        }
    }

    /// Builds a world from explicit `(coord, material, Q)` cells (air allowed)
    /// and a bag, on `table`. Bits are set for non-air cells; t0 (lanes and
    /// energy) is pinned to the result. Used by fixtures and parity tests.
    pub fn from_cells(
        cells: Vec<(Coord, MatId, i32)>,
        inv: Lanes,
        table: Arc<MatTable>,
        params: P6Params,
    ) -> Result<Self, BooksError> {
        let mut cells = cells;
        cells.sort_by_key(|c| c.0);
        let mut w = Self::new();
        w.table = table;
        w.params = params;
        w.inv = inv;
        for (k, &(c, m, q)) in cells.iter().enumerate() {
            if k > 0 && cells[k - 1].0 == c {
                return Err(BooksError::DuplicateCell(c));
            }
            if w.table.comp(m).is_none() {
                return Err(BooksError::BadMaterial(c, m));
            }
            w.cells.push(c);
            w.mat.push(m);
            w.q.push(q);
            if m != MatId::AIR {
                w.world.set_on(c);
            }
        }
        w.pin_t0();
        w.check_books()?;
        Ok(w)
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub(crate) fn index_of(&self, coord: Coord) -> Option<usize> {
        self.cells.binary_search(&coord).ok()
    }

    /// Material at a coordinate (`AIR` outside the domain or for air cells).
    pub fn material(&self, coord: Coord) -> MatId {
        self.index_of(coord).map_or(MatId::AIR, |i| self.mat[i])
    }

    /// Heat at a domain coordinate (`None` outside the domain).
    pub fn heat(&self, coord: Coord) -> Option<i32> {
        self.index_of(coord).map(|i| self.q[i])
    }

    /// Non-air materials in coordinate order.
    pub fn materials(&self) -> impl Iterator<Item = (Coord, MatId)> + '_ {
        self.cells
            .iter()
            .zip(&self.mat)
            .filter(|(_, m)| **m != MatId::AIR)
            .map(|(c, m)| (*c, *m))
    }

    /// Every domain cell (air included) with its material and heat.
    pub fn domain(&self) -> impl Iterator<Item = (Coord, MatId, i32)> + '_ {
        (0..self.cells.len()).map(|i| (self.cells[i], self.mat[i], self.q[i]))
    }

    /// Number of non-air cells.
    pub fn material_count(&self) -> usize {
        self.mat.iter().filter(|m| **m != MatId::AIR).count()
    }

    /// Number of domain cells (air included).
    pub fn domain_len(&self) -> usize {
        self.cells.len()
    }

    /// The inventory bag (atoms not in any voxel).
    pub fn inventory(&self) -> Lanes {
        self.inv
    }

    /// Lane totals recorded at t0.
    pub fn t0(&self) -> Lanes {
        self.t0
    }

    /// Energy total recorded at t0.
    pub fn t0_energy(&self) -> i64 {
        self.t0_energy
    }

    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Heat radiated to space so far (planet6 `spaceQ`).
    pub fn space_q(&self) -> i64 {
        self.space_q
    }

    /// Heat put in by the M1 booked energy input so far (inbound ledger; the
    /// mirror of [`Self::space_q`]).
    pub fn input_q(&self) -> i64 {
        self.input_q
    }

    /// Sunlight reflected by surface albedo so far (Driven only; a diagnostic
    /// outside every book: reflected light is never injected).
    pub fn reflected_q(&self) -> i64 {
        self.reflected_q
    }

    /// Sunlight that reached WATER so far and the part it reflected (Driven
    /// only; a diagnostic outside every book): `[incident_hi, reflected_hi,
    /// incident_lo, reflected_lo]`, split at incidence cosine `mu = 0.5`
    /// (sun at least 30 degrees above a pond top). Effective water albedo is
    /// `reflected / incident` per bin.
    pub fn water_sun_q(&self) -> [i64; 4] {
        self.water_sun
    }

    /// Outbound IR the vapor greenhouse has held back so far (Driven only; a
    /// diagnostic outside every book: the heat stayed in `Q`, nothing minted).
    pub fn trapped_q(&self) -> i64 {
        self.trapped_q
    }

    /// Sunlight the vapor clouds have reflected so far (Driven + greenhouse
    /// only; a diagnostic outside every book: it is never injected). Included
    /// in [`Self::reflected_q`].
    pub fn cloud_q(&self) -> i64 {
        self.cloud_q
    }

    /// Cloud reflectance (1/1024ths) applied on the last lit tick (a
    /// diagnostic; see [`CloudAlbedo`]).
    pub fn cloud_last(&self) -> i64 {
        self.cloud_last
    }

    /// Local vapor by region (Driven; empty before the first Driven tick).
    /// Sums to [`Self::vapor`] after every Driven tick.
    pub fn vapor_bins(&self) -> &[i64] {
        &self.vapor_bins
    }

    /// Rim cells per vapor region (Driven; empty before the first Driven tick).
    pub fn vapor_bin_rim(&self) -> &[i64] {
        self.vgeo.as_ref().map_or(&[], |g| &g.rim)
    }

    /// Vapor region of domain cell `c` (Driven; `None` before the first
    /// Driven tick or outside the domain).
    pub fn vapor_bin_at(&self, c: Coord) -> Option<usize> {
        let g = self.vgeo.as_ref()?;
        self.index_of(c).map(|i| usize::from(g.patch[i]))
    }

    /// Cumulative rain drops spilled onto the planet-wide landing line (see
    /// `VaporLocality`, `RainFall`); always 0 with one region. Under
    /// [`RainFall::Core`] only when no region at all can land a drop.
    pub fn rain_spilled(&self) -> u64 {
        self.rain_spilled
    }

    /// Cumulative rain drops a region with no landing ground of its own (no
    /// AIR over its rock, e.g. a bare flank of a full ball) shed onto the
    /// nearest regions with ground ([`RainFall::Core`] only), and the summed
    /// region-graph hop count of those drops (diagnostics, not in any book).
    pub fn rain_shed(&self) -> (u64, u64) {
        (self.rain_shed, self.rain_shed_hops)
    }

    /// §5v hemisphere split of the Driven water cycle, cumulative and outside
    /// every book: `[evap_lower, evap_all, rain_lower, rain_all]` where
    /// "lower" means the cell's y is below the core cell's y (the night
    /// hemisphere of [`SunSweep::Upper`](crate::chem::SunSweep)).
    pub fn hemi_diag(&self) -> [u64; 4] {
        self.hemi_diag
    }

    /// Sun at the cloud stage split by the struck cell, cumulative:
    /// `[incident_wet, cloud_wet, incident_dry, cloud_dry]`, wet = WATER or
    /// ICE (Driven + greenhouse; a diagnostic outside every book).
    pub fn cloud_split(&self) -> [i64; 4] {
        self.cloud_split
    }

    /// Per vapor region, cumulative: `[sun incident, sun incident on
    /// WATER/ICE, cloud-reflected, rim leak before the greenhouse, trapped]`
    /// (Driven + greenhouse; diagnostics outside every book).
    pub fn vapor_bin_diag(&self) -> &[[i64; 5]] {
        &self.patch_diag
    }

    pub fn params(&self) -> &P6Params {
        &self.params
    }

    pub fn set_params(&mut self, params: P6Params) {
        self.params = params;
    }

    pub fn table(&self) -> &MatTable {
        &self.table
    }

    pub fn net(&self) -> Option<&ChemNet> {
        self.net.as_deref()
    }

    /// Attaches a mounted reaction network: the world's material table becomes
    /// the network's (a superset of the hand table, so existing cells, lanes
    /// and energy are unchanged) and the firing law / `netBurn` / table gas
    /// phases turn on.
    pub fn attach_net(&mut self, net: Arc<ChemNet>) {
        debug_assert!(self.mat.iter().all(|m| net.table().comp(*m).is_some()));
        self.table = net.table_arc();
        self.net = Some(net);
    }

    /// planet6 `vapor`: water pairs held aloft by the water cycle.
    pub fn vapor(&self) -> i64 {
        self.vapor
    }

    /// planet6 `evaporated` and `rained`: cumulative water-cycle counters.
    pub fn evaporated(&self) -> u64 {
        self.evaporated
    }

    pub fn rained(&self) -> u64 {
        self.rained
    }

    /// planet6 `laneTotals`: bag + vapor pairs (4 H + 2 O each) + the
    /// composition of every material voxel.
    pub fn lane_totals(&self) -> Lanes {
        let mut lanes = self.inv;
        lanes[0] += self.vapor * 4;
        lanes[1] += self.vapor * 2;
        for m in &self.mat {
            if *m == MatId::AIR {
                continue;
            }
            if let Some(c) = self.table.comp(*m) {
                for k in 0..NE {
                    lanes[k] += c[k];
                }
            }
        }
        lanes
    }

    /// planet6 `laneDrift`: `lane_totals() - t0`.
    pub fn lane_drift(&self) -> Lanes {
        let t = self.lane_totals();
        std::array::from_fn(|k| t[k] - self.t0[k])
    }

    /// planet6 `energyTotal`: `sum(Q) - sum(BIND over non-air) + space_q +
    /// vapor * (LATENT_PAIR - BIND[WATER])` (a vapor pair stores the latent
    /// heat its evaporation took from the ocean, and keeps its binding),
    /// minus the M1 inbound ledger `input_q` (0 when the energy input is off).
    pub fn energy_total(&self) -> i64 {
        let mut e = self.space_q - self.input_q
            + self.vapor * i64::from(LATENT_PAIR - self.table.bind(MatId::WATER));
        for (m, q) in self.mat.iter().zip(&self.q) {
            e += i64::from(*q);
            if *m != MatId::AIR {
                e -= i64::from(self.table.bind(*m));
            }
        }
        e
    }

    /// planet6 `energyDrift`: `energy_total() - t0_energy`.
    pub fn energy_drift(&self) -> i64 {
        self.energy_total() - self.t0_energy
    }

    /// Re-pins t0 (lanes and energy) to the current totals (what planet6
    /// `reset` does at its end).
    pub fn pin_t0(&mut self) {
        self.t0 = self.lane_totals();
        self.t0_energy = self.energy_total();
    }

    /// Verifies the structural invariants, zero lane drift and zero energy drift.
    pub fn check_books(&self) -> Result<(), BooksError> {
        for (c, m) in self.cells.iter().zip(&self.mat) {
            if self.table.comp(*m).is_none() {
                return Err(BooksError::BadMaterial(*c, *m));
            }
            if *m != MatId::AIR && !self.world.is_on(*c) {
                return Err(BooksError::MaterialOnOffBit(*c));
            }
        }
        if let Some(lane) = self.inv.iter().position(|&n| n < 0) {
            return Err(BooksError::NegativeInventory { lane });
        }
        let d = self.lane_drift();
        if d.iter().any(|&x| x != 0) {
            return Err(BooksError::Drift(d));
        }
        let e = self.energy_drift();
        if e != 0 {
            return Err(BooksError::EnergyDrift(e));
        }
        Ok(())
    }

    /// The identity tick: advances the clock and changes nothing else.
    pub fn tick_identity(&mut self) {
        self.tick += 1;
    }

    /// Writes a material and keeps the occupancy bit in sync (air clears the
    /// bit unless it is bedrock).
    pub(crate) fn set_mat(&mut self, i: usize, m: MatId) {
        let old = self.mat[i];
        if old == m {
            return;
        }
        self.mat[i] = m;
        let c = self.cells[i];
        if m == MatId::AIR {
            if !is_bedrock(c) {
                self.world.set_off(c);
            }
        } else if old == MatId::AIR {
            self.world.set_on(c);
        }
    }

    /// planet6 `dig`: removes the material at `coord`, moving its composition
    /// into the bag and paying its BIND out of the cell's heat. Returns the
    /// dug material, or `None` when there was nothing to dig or the cell is
    /// too cold to break its bonds (`Q < BIND`). Both drifts stay zero.
    pub fn dig(&mut self, coord: Coord) -> Option<MatId> {
        let i = self.index_of(coord)?;
        let m = self.mat[i];
        if m == MatId::AIR {
            return None;
        }
        let u = self.table.bind(m);
        if self.q[i] < u {
            return None; // bonds must break: heat-limited
        }
        let c = self
            .table
            .comp(m)
            .expect("domain holds only rows with chemistry");
        for (inv, &v) in self.inv.iter_mut().zip(&c[..NE]) {
            *inv += v;
        }
        self.q[i] = self.q[i].wrapping_sub(u);
        self.set_mat(i, MatId::AIR);
        Some(m)
    }

    /// planet6 `place`: puts `mat` at an air coordinate, paying its
    /// composition out of the bag; the cell's heat gains the material's BIND.
    /// A coordinate outside the domain joins it (as a Q=0 cell). Refused
    /// (returns false, no change) for air, unknown materials, occupied cells,
    /// or an unaffordable composition.
    pub fn place(&mut self, coord: Coord, mat: MatId) -> bool {
        if mat == MatId::AIR {
            return false;
        }
        let at = self.index_of(coord);
        if at.is_some_and(|i| self.mat[i] != MatId::AIR) {
            return false;
        }
        if self.world.is_on(coord) && !is_bedrock(coord) {
            return false; // an occupied non-chem bit: do not overwrite
        }
        let Some(c) = self.table.comp(mat) else {
            return false;
        };
        if (0..NE).any(|k| self.inv[k] < c[k]) {
            return false;
        }
        for (inv, &v) in self.inv.iter_mut().zip(&c[..NE]) {
            *inv -= v;
        }
        let i = match at {
            Some(i) => i,
            None => self.insert_cell(coord),
        };
        self.q[i] = self.q[i].wrapping_add(self.table.bind(mat));
        self.set_mat(i, mat);
        true
    }

    /// Adds an air, Q=0 cell to the domain and returns its index.
    fn insert_cell(&mut self, coord: Coord) -> usize {
        let i = self.cells.binary_search(&coord).unwrap_err();
        self.cells.insert(i, coord);
        self.mat.insert(i, MatId::AIR);
        self.q.insert(i, 0);
        self.topo = None;
        i
    }

    /// Sky-shell headroom (cells above the genesis surface; 0 = no shell).
    pub fn sky_height(&self) -> u32 {
        self.sky_height
    }

    /// Adds an **air headroom shell** above the genesis ball `shape`: for every
    /// column `(dx, dz)` of the ball's footprint (`dx^2 + dz^2 <= R^2`), the
    /// `height` cells directly above the column's top ball cell
    /// (`dy = isqrt(R^2 - dx^2 - dz^2)`) join the domain as `AIR`, `Q = 0`
    /// (the [`genesis_heat`] value for air). Air with zero heat carries no
    /// atoms, no BIND and no Q, so lanes, energy and t0 are unchanged exactly;
    /// `check_books` holds before and after. The lower hemisphere keeps its
    /// space boundary (gravity is `-y`), so nothing can fall off the ball.
    ///
    /// The shell is ordinary domain air: heat diffuses through it, the space
    /// leak and the M1 sun act on its outer faces (top-of-atmosphere), and
    /// relief laws (uplift, plume) gain a fluid slot above the outer surface.
    /// The M1 core-input cell is pinned to the pre-shell bounding-box core so
    /// the lopsided shell does not move it. Returns the number of cells added
    /// (cells already in the domain or on a non-chem occupancy bit are skipped).
    /// Opt-in: planet6 / Classic worlds never get a shell.
    pub fn add_sky_shell(&mut self, shape: &GenesisShape, height: u32) -> usize {
        if height == 0 {
            return 0;
        }
        if self.core_pin.is_none() {
            let topo = self.topology();
            self.core_pin = topo.core.map(|i| self.cells[i as usize]);
        }
        let ri = i64::from(shape.radius);
        let c = shape.center;
        let mut add: Vec<Coord> = Vec::new();
        for dx in -ri..=ri {
            for dz in -ri..=ri {
                let rho2 = dx * dx + dz * dz;
                if rho2 > ri * ri {
                    continue;
                }
                let top = isqrt((ri * ri - rho2) as u64) as i64;
                for k in 1..=i64::from(height) {
                    let (Some(x), Some(y), Some(z)) = (
                        c.x.checked_add(dx),
                        c.y.checked_add(top + k),
                        c.z.checked_add(dz),
                    ) else {
                        continue;
                    };
                    let at = Coord::new(x, y, z);
                    if self.index_of(at).is_some() || (self.world.is_on(at) && !is_bedrock(at)) {
                        continue;
                    }
                    add.push(at);
                }
            }
        }
        let n_add = self.merge_sky_air(add);
        self.sky_height = self.sky_height.max(height);
        n_add
    }

    /// Merge new AIR (Q = 0, book-neutral) cells into the sorted domain and
    /// drop the cached topology/marks. Returns the number merged.
    fn merge_sky_air(&mut self, add: Vec<Coord>) -> usize {
        let n_add = add.len();
        let mut all: Vec<(Coord, MatId, i32)> = self
            .cells
            .iter()
            .zip(&self.mat)
            .zip(&self.q)
            .map(|((c, m), q)| (*c, *m, *q))
            .chain(add.into_iter().map(|c| (c, MatId::AIR, 0)))
            .collect();
        all.sort_unstable_by_key(|e| e.0);
        self.cells = all.iter().map(|e| e.0).collect();
        self.mat = all.iter().map(|e| e.1).collect();
        self.q = all.iter().map(|e| e.2).collect();
        self.marks.clear();
        self.topo = None;
        n_add
    }

    /// Full radial sky shell (§5u): AIR (Q = 0) on every lattice cell with
    /// `R^2 < |c - centre|^2 <= (R + H)^2`, i.e. `H` cells of headroom along
    /// every outward radial direction (flanks and underside too, not only the
    /// `+y` columns of [`ChemWorld::add_sky_shell`], whose cells are a subset).
    /// Books and t0 are untouched (new air carries no atoms and no heat). The
    /// M1 core cell is pinned exactly as for the polar shell, and the radial
    /// shell guard ([`ChemWorld::set_outward_guard`]) is switched on so `-y`
    /// flow/settle cannot drain flank and underside WATER/SED out into the
    /// lower shell (see the `outward_guard` field). Cells already in the
    /// domain or on a non-chem occupancy bit are skipped. `height = 0` is a no-op (the full-ball control).
    pub fn add_radial_sky_shell(&mut self, shape: &GenesisShape, height: u32) -> usize {
        if height == 0 {
            return 0;
        }
        if self.core_pin.is_none() {
            let topo = self.topology();
            self.core_pin = topo.core.map(|i| self.cells[i as usize]);
        }
        let ri = i64::from(shape.radius);
        let ro = ri + i64::from(height);
        let c = shape.center;
        let mut add: Vec<Coord> = Vec::new();
        for dx in -ro..=ro {
            for dy in -ro..=ro {
                for dz in -ro..=ro {
                    let d2 = dx * dx + dy * dy + dz * dz;
                    if d2 <= ri * ri || d2 > ro * ro {
                        continue;
                    }
                    let (Some(x), Some(y), Some(z)) = (
                        c.x.checked_add(dx),
                        c.y.checked_add(dy),
                        c.z.checked_add(dz),
                    ) else {
                        continue;
                    };
                    let at = Coord::new(x, y, z);
                    if self.index_of(at).is_some() || (self.world.is_on(at) && !is_bedrock(at)) {
                        continue;
                    }
                    add.push(at);
                }
            }
        }
        let n_add = self.merge_sky_air(add);
        self.sky_height = self.sky_height.max(height);
        let h = self.radial_shell.map_or(0, |g| g.2).max(i64::from(height));
        self.radial_shell = Some((c, ri, h));
        self.outward_guard = None; // auto: on under -y gravity, off coreward (§5w)
        n_add
    }

    /// Radial no-outward guard switch (A/B handle; see the `outward_guard`
    /// field). [`ChemWorld::add_radial_sky_shell`] turns it on; it has no
    /// effect without a radial shell.
    pub fn set_outward_guard(&mut self, on: bool) {
        self.outward_guard = Some(on);
    }

    /// Whether the radial no-outward guard is active (on, with a radial shell).
    pub fn outward_guard(&self) -> bool {
        self.radial_shell.is_some() && self.outward_guard.unwrap_or(!self.coreward())
    }

    /// §5w: surface gravity points at the core ([`WaterCycle::Driven`] with
    /// [`Gravity::Core`]); Classic and [`Gravity::Down`] are world `-y`.
    #[inline]
    pub fn coreward(&self) -> bool {
        self.params.cycle == WaterCycle::Driven && self.params.gravity == Gravity::Core
    }

    /// Whether the radial shell was added ([`ChemWorld::add_radial_sky_shell`]).
    pub fn sky_radial(&self) -> bool {
        self.radial_shell.is_some()
    }

    /// §5u guard test: moving material into cell `t` (by flow, settle,
    /// scour, dissolve or gas-rise displacement) is blocked because `t` is a
    /// *radial-only* shell cell: outside the genesis ball and outside the
    /// polar `+y` columns of [`ChemWorld::add_sky_shell`]. Always `false`
    /// without an active guard, so the full ball and polar shell are unchanged.
    #[inline]
    pub(crate) fn shell_blocked(&self, t: usize) -> bool {
        let Some((c, r, h)) = self.radial_shell else {
            return false;
        };
        if !self.outward_guard() {
            return false;
        }
        let tc = self.cells[t];
        let (dx, dy, dz) = (tc.x - c.x, tc.y - c.y, tc.z - c.z);
        let rho2 = dx * dx + dz * dz;
        if rho2 <= r * r {
            let top = isqrt((r * r - rho2) as u64) as i64;
            if dy >= -top && dy <= top + h {
                return false; // genesis ball column or polar shell column
            }
        }
        true
    }

    /// Outer-relief probe for a world grown from genesis ball `shape`: per
    /// footprint column the topmost **land** cell (rock-like: not AIR, WATER,
    /// ICE or a rising gas) and its height `h = y_land - y_datum`, where the
    /// datum is the column's genesis surface (`c.y + isqrt(R^2 - dx^2 - dz^2)`).
    /// `h > 0` is land standing in the sky shell, above the genesis sphere.
    /// Read-only measurement (not a sim path).
    pub fn land_relief(&self, shape: &GenesisShape) -> LandRelief {
        let ri = i64::from(shape.radius);
        let c = shape.center;
        let span = ri + i64::from(self.sky_height) + 1;
        let mut out = LandRelief::default();
        for dx in -ri..=ri {
            for dz in -ri..=ri {
                let rho2 = dx * dx + dz * dz;
                if rho2 > ri * ri {
                    continue;
                }
                let top = isqrt((ri * ri - rho2) as u64) as i64;
                out.columns += 1;
                let mut found = None;
                for dy in (-span..=span).rev() {
                    let at = Coord::new(c.x + dx, c.y + dy, c.z + dz);
                    let Some(i) = self.index_of(at) else { continue };
                    let m = self.mat[i];
                    let land = m != MatId::AIR
                        && m != MatId::WATER
                        && m != MatId::ICE
                        && !self.is_rising(m);
                    if !land {
                        continue;
                    }
                    if found.is_none() {
                        found = Some(dy);
                    }
                    if dy > top {
                        out.above_datum += 1;
                    }
                }
                let Some(dy) = found else {
                    out.no_land += 1;
                    continue;
                };
                let h = dy - top;
                out.sum_h += h;
                out.sum_h2 += h * h;
                out.max_h = if out.land == 0 { h } else { out.max_h.max(h) };
                out.min_h = if out.land == 0 { h } else { out.min_h.min(h) };
                out.land += 1;
                let r2 = (rho2 + dy * dy) as f64;
                out.sum_r += r2.sqrt();
                out.sum_r2 += r2;
            }
        }
        out
    }
}

/// [`ChemWorld::land_relief`]: outer land relief over a genesis footprint.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LandRelief {
    /// Footprint columns (`dx^2 + dz^2 <= R^2`).
    pub columns: u32,
    /// Columns holding at least one land cell.
    pub land: u32,
    /// Columns with no land cell at all.
    pub no_land: u32,
    /// Land cells (anywhere in a footprint column) above the column datum.
    pub above_datum: u32,
    /// Sum and sum of squares of the top-land height `h` over land columns.
    pub sum_h: i64,
    pub sum_h2: i64,
    pub max_h: i64,
    pub min_h: i64,
    /// Sum / sum of squares of the top-land cell's distance from the centre.
    pub sum_r: f64,
    pub sum_r2: f64,
}

impl LandRelief {
    /// Mean top-land height above the genesis surface.
    pub fn mean_h(&self) -> f64 {
        self.sum_h as f64 / f64::from(self.land.max(1))
    }
    /// sigma_h: standard deviation of top-land height (the outer-relief metric).
    pub fn std_h(&self) -> f64 {
        let n = f64::from(self.land.max(1));
        let m = self.sum_h as f64 / n;
        (self.sum_h2 as f64 / n - m * m).max(0.0).sqrt()
    }
    /// sigma_R: standard deviation of the top-land cell's radius.
    pub fn std_r(&self) -> f64 {
        let n = f64::from(self.land.max(1));
        let m = self.sum_r / n;
        (self.sum_r2 / n - m * m).max(0.0).sqrt()
    }
}

/// Primary radial bands, core to surface (planet6 element world crossed with
/// its mineral strata). Genesis first shares the budget among these
/// round-robin (one copy per band per round while affordable), so materials
/// that compete for a lane (rock / mineral / gypsum / limestone all need Ca)
/// split it instead of the innermost one eating it.
const BANDS: [MatId; 12] = [
    MatId::element(6), // Fe
    MatId::element(7), // Ni
    MatId::element(8), // Si
    MatId::element(9), // Mg
    MatId::ROCK,
    MatId::MIN,
    MatId::GYP,
    MatId::CARB,
    MatId::element(10), // Na
    MatId::element(11), // K
    MatId::WATER,
    MatId::CH4,
];

/// Radial order of every material genesis can place (BANDS plus everything in
/// FALLBACK), core to surface. Each layer gets a shell exactly as thick as its
/// planned count of cells: metals in the core, Si/Mg mantle, stray S/Ca and
/// quicklime under the rock crust, salts, ocean, then volatiles and stray
/// light atoms outermost.
const LAYERS: [MatId; 23] = [
    MatId::element(6), // Fe
    MatId::element(7), // Ni
    MatId::element(8), // Si
    MatId::element(9), // Mg
    MatId::element(4), // S
    MatId::element(5), // Ca
    MatId::CAO,
    MatId::ROCK,
    MatId::MIN,
    MatId::GYP,
    MatId::CARB,
    MatId::element(10), // Na
    MatId::element(11), // K
    MatId::WATER,
    MatId::H2S,
    MatId::NH3,
    MatId::CH4,
    MatId::CO2,
    MatId::O2,
    MatId::element(0), // H
    MatId::element(2), // C
    MatId::element(3), // N
    MatId::element(1), // O
];

/// Boundary blur in 1/256 cell: a cell's layer key is its fixed-point radius
/// plus a seeded jitter in `0..BLUR_FX`, so neighbouring shells interleave over
/// ~0.75 cell without changing any layer's cell count.
const BLUR_FX: u64 = 192;

/// Tried in order when a cell's band material is unaffordable. Compounds
/// first (they spend several lanes at once), then single-atom blocks for every
/// lane, so any non-zero remainder can always be placed while cells remain.
const FALLBACK: [MatId; 10 + NE] = [
    MatId::GYP,
    MatId::ROCK,
    MatId::CARB,
    MatId::WATER,
    MatId::CH4,
    MatId::NH3,
    MatId::CO2,
    MatId::H2S,
    MatId::CAO,
    MatId::O2,
    MatId::element(0),
    MatId::element(1),
    MatId::element(2),
    MatId::element(3),
    MatId::element(4),
    MatId::element(5),
    MatId::element(6),
    MatId::element(7),
    MatId::element(8),
    MatId::element(9),
    MatId::element(10),
    MatId::element(11),
];

fn splitmix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

fn cell_hash(seed: u64, d: (i64, i64, i64)) -> u64 {
    splitmix(seed ^ splitmix((d.0 as u64) ^ splitmix((d.1 as u64) ^ splitmix(d.2 as u64))))
}

fn affordable(rem: &Lanes, c: &Lanes) -> bool {
    (0..NE).all(|k| rem[k] >= c[k])
}

/// Largest `k <= cap` with `k * c <= rem` lane-wise (`c` non-zero).
fn max_copies(rem: &Lanes, c: &Lanes, cap: usize) -> usize {
    let mut k = cap as i64;
    for i in 0..NE {
        if c[i] > 0 {
            k = k.min(rem[i] / c[i]);
        }
    }
    k.max(0) as usize
}

fn spend(rem: &mut Lanes, c: &Lanes, k: usize) {
    for i in 0..NE {
        rem[i] -= c[i] * k as i64;
    }
}

/// Budget-sized layer plan: how many cells each [`LAYERS`] entry gets, for a
/// shape of `n` cells, and the remainder that settles into the bag.
///
/// 1. Round-robin over [`BANDS`]: every round each band takes one copy of its
///    material if the remainder covers it, until no band can or `n` cells are
///    planned. Bands with no competition end up with their whole budget;
///    bands sharing a lane split it evenly.
/// 2. Leftovers go through [`FALLBACK`] in order, each taking as many copies
///    as fit (cells permitting); the unit blocks at the end make any non-zero
///    remainder placeable while cells remain.
///
/// Integer-only and deterministic in `(budget, n)`.
fn plan_layers(budget: &Lanes, n: usize) -> ([usize; LAYERS.len()], Lanes) {
    let mut rem = *budget;
    let mut counts = [0usize; LAYERS.len()];
    let slot = |m: MatId| LAYERS.iter().position(|&l| l == m).expect("layer");
    let band_comp: [Lanes; BANDS.len()] =
        std::array::from_fn(|i| comp(BANDS[i]).expect("table rows"));
    let mut used = 0usize;
    let mut live = [true; BANDS.len()];
    while used < n && live.iter().any(|&x| x) {
        // Bulk rounds: every live band can afford `k` more copies each.
        let alive = live.iter().filter(|&&x| x).count();
        let mut k = (n - used) / alive;
        for i in 0..BANDS.len() {
            if live[i] {
                // Copies band i could take if it ran alone; competitors that share
                // a lane are bounded by an even split of that lane.
                let c = &band_comp[i];
                for lane in 0..NE {
                    if c[lane] == 0 {
                        continue;
                    }
                    let need: i64 = (0..BANDS.len())
                        .filter(|&j| live[j])
                        .map(|j| band_comp[j][lane])
                        .sum();
                    k = k.min((rem[lane] / need).max(0) as usize);
                }
            }
        }
        if k > 0 {
            for i in 0..BANDS.len() {
                if live[i] {
                    spend(&mut rem, &band_comp[i], k);
                    counts[slot(BANDS[i])] += k;
                    used += k;
                }
            }
            continue;
        }
        // One single round in band order; a band that cannot afford retires.
        for i in 0..BANDS.len() {
            if !live[i] || used >= n {
                continue;
            }
            if affordable(&rem, &band_comp[i]) {
                spend(&mut rem, &band_comp[i], 1);
                counts[slot(BANDS[i])] += 1;
                used += 1;
            } else {
                live[i] = false;
            }
        }
    }
    for m in FALLBACK {
        if used >= n {
            break;
        }
        let c = comp(m).expect("table rows");
        let k = max_copies(&rem, &c, n - used);
        spend(&mut rem, &c, k);
        counts[slot(m)] += k;
        used += k;
    }
    (counts, rem)
}

/// Floor integer square root.
pub(crate) fn isqrt(n: u64) -> u64 {
    if n < 2 {
        return n;
    }
    // Integer Newton iteration (no float sqrt in sim paths).
    let mut x = n;
    let mut y = n / 2 + n % 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

/// The integer radial heat profile genesis assigns (planet6 `reset`'s
/// `genesisQ -> surfaceQ` ramp without float `hypot`): water/ice get
/// `ocean_q`; other materials get
/// `max(120, gQ - (gQ - sQ) * isqrt(d2 << 16) / (R << 8))`; air gets 0.
pub fn genesis_heat(m: MatId, d2: i64, radius: u32, p: &P6Params) -> i32 {
    if m == MatId::AIR {
        return 0;
    }
    if m == MatId::WATER || m == MatId::ICE {
        return p.ocean_q;
    }
    let gq = i64::from(p.genesis_q);
    let sq = i64::from(p.surface_q);
    if radius == 0 {
        return gq.max(120) as i32;
    }
    let r_fx = isqrt((d2.max(0) as u64) << 16) as i64; // distance in 1/256 cells
    let ramp = (gq - sq) * r_fx / (i64::from(radius) << 8);
    (gq - ramp).max(120) as i32
}

/// Builds a world whose material composition plus bag spends `budget` exactly,
/// pins t0, and verifies the books. Deterministic in `(budget, seed, shape)`.
/// Different seeds may place differently; totals are always the budget. The
/// tick parameters are [`P6Params::default`] with `seed` = the low 32 bits of
/// `cfg.seed`.
pub fn genesis(cfg: &GenesisConfig) -> Result<ChemWorld, GenesisError> {
    let p = P6Params {
        seed: cfg.seed as u32,
        ..P6Params::default()
    };
    genesis_p6(cfg, &p)
}

/// [`genesis`] with explicit tick parameters (they also set the radial heat).
pub fn genesis_p6(cfg: &GenesisConfig, params: &P6Params) -> Result<ChemWorld, GenesisError> {
    let total = cfg.budget.total_atoms()?;
    if total > cfg.atom_cap {
        return Err(GenesisError::OverAtomCap {
            total,
            cap: cfg.atom_cap,
        });
    }
    let r = cfg.shape.radius;
    if r > MAX_GENESIS_RADIUS {
        return Err(GenesisError::RadiusTooLarge {
            radius: r,
            max: MAX_GENESIS_RADIUS,
        });
    }
    let ri = i64::from(r);
    let c = cfg.shape.center;
    for v in [c.x, c.y, c.z] {
        if v.checked_add(ri).is_none() || v.checked_sub(ri).is_none() {
            return Err(GenesisError::ShapeOutOfRange);
        }
    }
    let r2 = ri * ri;
    let h2 = cfg.shape.hollow.map(|h| i64::from(h) * i64::from(h));

    let mut out = ChemWorld::new();
    out.params = *params;
    let mut rem = cfg.budget.lanes;

    // Candidate offsets ordered by a blurred radius (fixed-point radius plus a
    // seeded jitter), hash then offset breaking ties.
    type Offset = (i64, i64, i64);
    let mut cand: Vec<(u64, u64, i64, Offset)> = Vec::new();
    let mut air: Vec<(i64, (i64, i64, i64))> = Vec::new();
    for dx in -ri..=ri {
        for dy in -ri..=ri {
            for dz in -ri..=ri {
                let d2 = dx * dx + dy * dy + dz * dz;
                if d2 > r2 {
                    continue;
                }
                if h2.is_some_and(|h| d2 <= h) {
                    air.push((d2, (dx, dy, dz)));
                    continue;
                }
                let h = cell_hash(cfg.seed, (dx, dy, dz));
                let key = isqrt((d2 as u64) << 16) + h % BLUR_FX;
                cand.push((key, h, d2, (dx, dy, dz)));
            }
        }
    }
    cand.sort_unstable();
    let n = cand.len();
    // Shell thicknesses come from the budget: layer i owns the next counts[i]
    // cells inner to outer. Cells past the plan stay air; what the plan could
    // not place is the bag.
    let (counts, bag) = plan_layers(&rem, n);
    let mut fill = LAYERS
        .iter()
        .zip(counts)
        .flat_map(|(&m, k)| std::iter::repeat_n(m, k));
    let mut cells: Vec<(Coord, MatId, i32)> = Vec::with_capacity(n + air.len());
    for &(_, _, d2, (dx, dy, dz)) in &cand {
        let coord = Coord::new(c.x + dx, c.y + dy, c.z + dz);
        let m = fill.next().unwrap_or(MatId::AIR);
        if m != MatId::AIR {
            let mc = comp(m).expect("table rows");
            spend(&mut rem, &mc, 1);
        }
        cells.push((coord, m, genesis_heat(m, d2, r, params)));
    }
    debug_assert!(fill.next().is_none());
    debug_assert_eq!(rem, bag);
    for &(_, (dx, dy, dz)) in &air {
        cells.push((Coord::new(c.x + dx, c.y + dy, c.z + dz), MatId::AIR, 0));
    }
    cells.sort_unstable_by_key(|e| e.0);
    for (coord, m, q) in cells {
        out.cells.push(coord);
        out.mat.push(m);
        out.q.push(q);
        if m != MatId::AIR {
            out.world.set_on(coord);
        }
    }

    // Whatever the shape could not hold settles into the bag.
    out.inv = rem;
    out.t0 = cfg.budget.lanes;
    out.t0_energy = out.energy_total();
    let drift = out.lane_drift();
    if drift.iter().any(|&x| x != 0) {
        return Err(GenesisError::Drift { drift });
    }
    debug_assert_eq!(out.check_books(), Ok(()));
    Ok(out)
}

/// [`genesis_p6`] plus an air headroom shell `sky` cells deep above the ball
/// ([`ChemWorld::add_sky_shell`]). Books and t0 are the unpadded genesis'
/// exactly; `sky = 0` is [`genesis_p6`].
pub fn genesis_sky(
    cfg: &GenesisConfig,
    params: &P6Params,
    sky: u32,
) -> Result<ChemWorld, GenesisError> {
    let mut w = genesis_p6(cfg, params)?;
    w.add_sky_shell(&cfg.shape, sky);
    debug_assert_eq!(w.check_books(), Ok(()));
    Ok(w)
}

/// [`genesis_p6`] plus a full radial air shell `sky` cells deep
/// ([`ChemWorld::add_radial_sky_shell`], §5u). Books and t0 are the unpadded
/// genesis' exactly; `sky = 0` is [`genesis_p6`].
pub fn genesis_sky_radial(
    cfg: &GenesisConfig,
    params: &P6Params,
    sky: u32,
) -> Result<ChemWorld, GenesisError> {
    let mut w = genesis_p6(cfg, params)?;
    w.add_radial_sky_shell(&cfg.shape, sky);
    debug_assert_eq!(w.check_books(), Ok(()));
    Ok(w)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(r: u32) -> GenesisShape {
        GenesisShape {
            center: Coord::new(0, 0, 0),
            radius: r,
            hollow: None,
        }
    }

    fn cfg(budget: Lanes, seed: u64, r: u32) -> GenesisConfig {
        GenesisConfig {
            budget: GenesisBudget::new(budget),
            seed,
            shape: shape(r),
            atom_cap: DEFAULT_ATOM_CAP,
        }
    }

    const BUDGET: Lanes = [4000, 3000, 400, 120, 300, 350, 200, 60, 180, 150, 40, 30];

    #[test]
    fn genesis_totals_equal_budget_and_drift_is_zero() {
        let w = genesis(&cfg(BUDGET, 7, 10)).unwrap();
        assert_eq!(w.lane_totals(), BUDGET);
        assert_eq!(w.lane_drift(), [0; NE]);
        assert_eq!(w.t0(), BUDGET);
        assert_eq!(w.check_books(), Ok(()));
        assert!(w.material_count() > 0);
        assert_eq!(w.inventory(), [0; NE], "radius 10 holds this budget fully");
        for (coord, _) in w.materials() {
            assert!(w.world().is_on(coord));
        }
    }

    #[test]
    fn two_seeds_same_totals_different_placement() {
        let a = genesis(&cfg(BUDGET, 1, 10)).unwrap();
        let b = genesis(&cfg(BUDGET, 2, 10)).unwrap();
        assert_eq!(a.lane_totals(), BUDGET);
        assert_eq!(b.lane_totals(), BUDGET);
        let pa: Vec<_> = a.materials().collect();
        let pb: Vec<_> = b.materials().collect();
        assert_ne!(pa, pb, "seed should move placement");
    }

    #[test]
    fn genesis_is_deterministic() {
        let a = genesis(&cfg(BUDGET, 99, 9)).unwrap();
        let b = genesis(&cfg(BUDGET, 99, 9)).unwrap();
        assert_eq!(
            a.materials().collect::<Vec<_>>(),
            b.materials().collect::<Vec<_>>()
        );
        assert_eq!(a.inventory(), b.inventory());
    }

    #[test]
    fn zero_budget_is_bedrock_only_and_totals_zero() {
        let w = genesis(&cfg([0; NE], 5, 8)).unwrap();
        assert_eq!(w.material_count(), 0);
        assert_eq!(w.lane_totals(), [0; NE]);
        assert_eq!(w.world().iter_on().count(), 1, "only bedrock");
        assert_eq!(w.check_books(), Ok(()));
    }

    #[test]
    fn overfull_budget_settles_into_bag_exactly() {
        // Radius 1 = 7 cells; most of the budget cannot fit.
        let w = genesis(&cfg(BUDGET, 3, 1)).unwrap();
        assert_eq!(w.material_count(), 7);
        assert!(w.inventory().iter().any(|&n| n > 0));
        assert_eq!(w.lane_totals(), BUDGET);
        assert_eq!(w.check_books(), Ok(()));
    }

    #[test]
    fn hollow_shape_leaves_interior_air() {
        let mut c = cfg(BUDGET, 4, 12);
        c.shape.hollow = Some(5);
        let w = genesis(&c).unwrap();
        for (coord, _) in w.materials() {
            assert!(coord.x * coord.x + coord.y * coord.y + coord.z * coord.z > 25);
        }
        assert_eq!(w.lane_totals(), BUDGET);
    }

    #[test]
    fn offset_center_and_ash_budget() {
        let budget = GenesisBudget::ash(3).unwrap();
        let c = GenesisConfig {
            budget,
            seed: 11,
            shape: GenesisShape {
                center: Coord::new(1_000, -50, 7),
                radius: 8,
                hollow: None,
            },
            atom_cap: DEFAULT_ATOM_CAP,
        };
        let w = genesis(&c).unwrap();
        assert_eq!(w.lane_totals(), budget.lanes);
        assert_eq!(w.lane_totals()[0], 960);
        assert_eq!(w.check_books(), Ok(()));
    }

    #[test]
    fn rejects_negative_overflow_cap_and_radius() {
        let mut b = BUDGET;
        b[3] = -1;
        assert_eq!(
            genesis(&cfg(b, 0, 4)).unwrap_err(),
            GenesisError::NegativeLane { lane: 3, value: -1 }
        );
        let mut b = [0; NE];
        b[0] = i64::MAX;
        b[1] = 1;
        assert_eq!(genesis(&cfg(b, 0, 4)).unwrap_err(), GenesisError::Overflow);
        let mut c = cfg(BUDGET, 0, 4);
        c.atom_cap = 100;
        assert!(matches!(
            genesis(&c).unwrap_err(),
            GenesisError::OverAtomCap { .. }
        ));
        assert!(matches!(
            genesis(&cfg(BUDGET, 0, MAX_GENESIS_RADIUS + 1)).unwrap_err(),
            GenesisError::RadiusTooLarge { .. }
        ));
        let mut c = cfg(BUDGET, 0, 4);
        c.shape.center = Coord::new(i64::MAX - 1, 0, 0);
        assert_eq!(genesis(&c).unwrap_err(), GenesisError::ShapeOutOfRange);
    }

    #[test]
    fn identity_ticks_keep_drift_zero() {
        let mut w = genesis(&cfg(BUDGET, 8, 10)).unwrap();
        for _ in 0..1000 {
            w.tick_identity();
        }
        assert_eq!(w.tick(), 1000);
        assert_eq!(w.lane_drift(), [0; NE]);
        assert_eq!(w.check_books(), Ok(()));
    }

    #[test]
    fn dig_and_place_move_atoms_through_bag_with_zero_drift() {
        let mut w = genesis(&cfg(BUDGET, 8, 10)).unwrap();
        let (coord, mat) = w.materials().find(|(c, _)| !is_bedrock(*c)).unwrap();
        let before = w.inventory();
        assert_eq!(w.dig(coord), Some(mat));
        assert!(!w.world().is_on(coord));
        let c = comp(mat).unwrap();
        for k in 0..NE {
            assert_eq!(w.inventory()[k], before[k] + c[k]);
        }
        assert_eq!(w.check_books(), Ok(()));
        assert_eq!(w.dig(coord), None);
        // Place it back.
        assert!(w.place(coord, mat));
        assert_eq!(w.inventory(), before);
        assert_eq!(w.check_books(), Ok(()));
        // Refusals: occupied, unaffordable, unknown id, air.
        assert!(!w.place(coord, mat));
        let far = Coord::new(500, 500, 500);
        assert!(!w.place(far, MatId(77)));
        assert!(!w.place(far, MatId::AIR));
        let mut empty = ChemWorld::new();
        assert!(!empty.place(far, MatId::ROCK));
        assert_eq!(empty.check_books(), Ok(()));
    }

    #[test]
    fn digging_bedrock_material_keeps_bedrock_bit() {
        let mut w = genesis(&cfg(BUDGET, 8, 3)).unwrap();
        let origin = Coord::new(0, 0, 0);
        assert_ne!(w.material(origin), MatId::AIR);
        w.dig(origin).unwrap();
        assert!(w.world().is_on(origin));
        assert_eq!(w.check_books(), Ok(()));
    }

    #[test]
    fn comp_table_matches_planet6_rows() {
        assert_eq!(
            comp(MatId::ROCK).unwrap(),
            [0, 4, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0]
        );
        assert_eq!(
            comp(MatId::GYP).unwrap(),
            [4, 6, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0]
        );
        assert_eq!(
            comp(MatId::CH4).unwrap(),
            [4, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        );
        assert_eq!(comp(MatId(126)).unwrap(), unit(6)); // Fe, Z=26
        assert_eq!(name(MatId(119)), Some("K"));
        assert_eq!(comp(MatId(77)), None);
        for lane in 0..NE {
            assert_eq!(comp(MatId::element(lane)).unwrap(), unit(lane));
        }
    }

    #[test]
    fn many_random_budgets_balance() {
        let mut s = 0x1234_u64;
        for i in 0..40 {
            let lanes: Lanes = std::array::from_fn(|_| {
                s = splitmix(s);
                (s % 900) as i64
            });
            let w = genesis(&cfg(lanes, i, (i % 7) as u32 + 1)).unwrap();
            assert_eq!(w.lane_totals(), lanes, "seed {i}");
            assert_eq!(w.check_books(), Ok(()));
        }
    }
}

#[cfg(test)]
mod band_tests {
    use super::*;

    fn world(budget: Lanes, seed: u64, r: u32) -> ChemWorld {
        let w = genesis(&GenesisConfig {
            budget: GenesisBudget::new(budget),
            seed,
            shape: GenesisShape {
                center: Coord::new(0, 0, 0),
                radius: r,
                hollow: None,
            },
            atom_cap: DEFAULT_ATOM_CAP,
        })
        .unwrap();
        assert_eq!(w.lane_totals(), budget);
        assert_eq!(w.lane_drift(), [0; NE]);
        assert_eq!(w.check_books(), Ok(()));
        w
    }

    fn d2(c: Coord) -> i64 {
        c.x * c.x + c.y * c.y + c.z * c.z
    }

    /// Mean squared radius of each material present.
    fn mean_d2(w: &ChemWorld) -> std::collections::BTreeMap<MatId, f64> {
        let mut acc = std::collections::BTreeMap::<MatId, (i64, i64)>::new();
        for (c, m) in w.materials() {
            let e = acc.entry(m).or_default();
            e.0 += d2(c);
            e.1 += 1;
        }
        acc.into_iter()
            .map(|(m, (s, n))| (m, s as f64 / n as f64))
            .collect()
    }

    fn count(w: &ChemWorld, m: MatId) -> usize {
        w.materials().filter(|(_, x)| *x == m).count()
    }

    // H O C N S Ca Fe Ni Si Mg Na K
    const MIXED: Lanes = [3000, 6000, 200, 50, 500, 600, 400, 80, 200, 200, 30, 20];

    #[test]
    fn fe_rich_budget_concentrates_iron_in_the_core() {
        for seed in [1, 7, 99] {
            let w = world(MIXED, seed, 9);
            // The sphere has room, so every metal atom is placed, none bagged.
            assert_eq!(w.inventory()[6], 0, "seed {seed}");
            assert_eq!(count(&w, MatId::element(6)), 400);
            // The innermost 400 cells are (almost) all iron.
            let mut cells: Vec<(i64, MatId)> = w.materials().map(|(c, m)| (d2(c), m)).collect();
            cells.sort();
            let fe_inner = cells[..400]
                .iter()
                .filter(|(_, m)| *m == MatId::element(6))
                .count();
            assert!(
                fe_inner >= 360,
                "seed {seed}: {fe_inner}/400 inner cells are Fe"
            );
            // And no iron sits in the outer half of the radius.
            let max_fe = cells
                .iter()
                .filter(|(_, m)| *m == MatId::element(6))
                .map(|x| x.0)
                .max();
            assert!(max_fe.unwrap() <= 36, "seed {seed}: Fe at d2 {max_fe:?}");
        }
    }

    #[test]
    fn ocean_budget_puts_water_outermost() {
        // A rock crust and a lot of water, little else.
        let budget: Lanes = [8000, 7000, 0, 0, 300, 300, 100, 0, 50, 50, 0, 0];
        let w = world(budget, 7, 10);
        // Condensed cells only (leftover O becomes an O2 skin outside the ocean).
        let gas = |m: MatId| LAYERS.iter().position(|&l| l == m).unwrap() > 13;
        let mut solid: Vec<(i64, MatId)> = w
            .materials()
            .filter(|(_, m)| !gas(*m))
            .map(|(c, m)| (d2(c), m))
            .collect();
        solid.sort();
        let outer = &solid[solid.len() - 400..];
        let water = outer.iter().filter(|(_, m)| *m == MatId::WATER).count();
        assert!(
            water * 10 >= outer.len() * 9,
            "outer condensed water {water}/{}",
            outer.len()
        );
        assert!(count(&w, MatId::WATER) > 1500);
        let means = mean_d2(&w);
        assert!(means[&MatId::element(6)] < means[&MatId::ROCK]);
        assert!(means[&MatId::ROCK] < means[&MatId::WATER]);
    }

    #[test]
    fn crust_minerals_share_calcium_instead_of_one_eating_it() {
        let w = world(MIXED, 7, 9);
        let (rock, min, gyp, carb) = (
            count(&w, MatId::ROCK),
            count(&w, MatId::MIN),
            count(&w, MatId::GYP),
            count(&w, MatId::CARB),
        );
        assert!(
            rock > 0 && min > 0 && gyp > 0 && carb > 0,
            "{rock} {min} {gyp} {carb}"
        );
        assert!(
            rock.abs_diff(min) <= 1 && rock.abs_diff(gyp) <= 1,
            "{rock} {min} {gyp}"
        );
        // The old weak spot: gypsum fallback cells inside the metal core.
        let core_gyp = w
            .materials()
            .filter(|(c, m)| *m == MatId::GYP && d2(*c) <= 16)
            .count();
        assert_eq!(core_gyp, 0);
    }

    #[test]
    fn layers_are_radially_ordered() {
        for seed in [3, 7] {
            let w = world(MIXED, seed, 9);
            let means = mean_d2(&w);
            let present: Vec<MatId> = LAYERS
                .iter()
                .copied()
                .filter(|m| means.contains_key(m))
                .collect();
            // Fe -> Ni -> Si -> Mg -> crust -> ocean: each non-trivial layer lies outside the last.
            let big: Vec<MatId> = present
                .into_iter()
                .filter(|m| count(&w, *m) >= 20)
                .collect();
            for pair in big.windows(2) {
                assert!(
                    means[&pair[0]] < means[&pair[1]],
                    "seed {seed}: {:?} {} !< {:?} {}",
                    name(pair[0]),
                    means[&pair[0]],
                    name(pair[1]),
                    means[&pair[1]]
                );
            }
        }
    }

    #[test]
    fn plan_matches_placement_and_caps_at_cell_count() {
        for (budget, n) in [
            (MIXED, 50usize),
            (MIXED, 5000),
            ([0; NE], 10),
            ([1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 7], 3),
        ] {
            let (counts, bag) = plan_layers(&budget, n);
            let placed: usize = counts.iter().sum();
            assert!(placed <= n);
            let mut sum = bag;
            for (m, k) in LAYERS.iter().zip(counts) {
                let c = comp(*m).unwrap();
                for i in 0..NE {
                    sum[i] += c[i] * k as i64;
                }
            }
            assert_eq!(sum, budget);
            assert!(bag.iter().all(|&x| x >= 0));
            if placed < n {
                assert_eq!(bag, [0; NE], "cells to spare means nothing is bagged");
            }
        }
    }
}

#[cfg(test)]
mod example_dump {
    use super::*;
    #[test]
    #[ignore]
    fn dump_example() {
        let budget: Lanes = [4000, 3000, 400, 120, 300, 350, 200, 60, 180, 150, 40, 30];
        for (seed, r) in [(7u64, 10u32), (8, 10), (7, 3)] {
            let w = genesis(&GenesisConfig {
                budget: GenesisBudget::new(budget),
                seed,
                shape: GenesisShape {
                    center: Coord::new(0, 0, 0),
                    radius: r,
                    hollow: None,
                },
                atom_cap: DEFAULT_ATOM_CAP,
            })
            .unwrap();
            let mut counts = std::collections::BTreeMap::new();
            for (_, m) in w.materials() {
                *counts.entry(name(m).unwrap()).or_insert(0) += 1;
            }
            println!(
                "seed={seed} r={r} voxels={} totals={:?} drift={:?} inv={:?}\n  {:?}",
                w.material_count(),
                w.lane_totals(),
                w.lane_drift(),
                w.inventory(),
                counts
            );
        }
    }
}
