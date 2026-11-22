//! `EMERGENCE-R1`: bounded, falsifiable 4-metric emergence assays over `RULE-1` (`B5/S45`)
//! compared against matched-density deterministic null-model soups and negative-control rules
//! (`B6/S45` subcritical rapid die-out and `B4/S45` supercritical rapid expansion).
//!
//! Computes four predeclared exact integer/fixed-point metrics (zero floating-point nondeterminism)
//! over a frozen deterministic `SplitMix64` seed corpus:
//! 1. **Survival & activity:** `final_mutable_cells`, `peak_mutable_cells`, `survival_ticks`,
//!    `active_mutation_ticks`, and finite `cap_hit` detection.
//! 2. **Spatial compressibility ratio:** exact `MortonGapStreamV1::encoded_len()` divided by raw
//!    `24 * live_cells` in integer basis points (`0..=10_000+`; lower = stronger spatial structure).
//! 3. **Perturbation sensitivity (1-bit Hamming divergence):** exact symmetric difference
//!    `|W_T(S) Δ W_T(S ⊕ {p})|` at final tick `T` after toggling one deterministic voxel at `t = 0`.
//! 4. **Coherent transport & periodicity:** scaled L1 centroid displacement (`centroid_l1_shift_x100`)
//!    and exact `world_tag` recurrence period (`detected_period`).
//!
//! Qualitative 3D renders remain labeled observations and never substitute for these quantitative
//! metrics. Bounded corpus separation does not claim universal computation, life, or intelligence.

use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::commit_r1::MortonGapStreamV1;
use crate::history::world_tag;
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::rule_1::{
    BAYS_4555_BIRTH_MASK, BAYS_4555_SURVIVE_MASK, bays_4555_glider_coords,
    octahedron_still_life_coords, period_4_oscillator_coords,
};
use crate::world::{BEDROCK_COORD, Coord, World};

/// Canonical schema string for an [`EmergenceCorpusSpecV1`] assay specification.
pub const EMERGENCE_R1_CORPUS_SPEC_SCHEMA: &[u8] = b"bitblock/emergence-r1/corpus-spec/v1";
/// Canonical schema string for an [`EmergenceAssayReportV1`] deterministic assay receipt.
pub const EMERGENCE_R1_ASSAY_REPORT_SCHEMA: &[u8] = b"bitblock/emergence-r1/assay-report/v1";

/// Rule code for candidate `RULE-1` Carter Bays 3D Life (`B5/S45`).
pub const RULE_CODE_B5S45: u8 = 1;
/// Rule code for subcritical negative control (`B6/S45`, rapid die-out).
pub const RULE_CODE_B6S45_SUBCRITICAL: u8 = 2;
/// Rule code for supercritical negative control (`B4/S45`, rapid expansion / cap trip).
pub const RULE_CODE_B4S45_SUPERCRITICAL: u8 = 3;

/// Subject kind: Carter Bays' 10-cell period-4 diagonal glider.
pub const SUBJECT_BAYS_GLIDER: u8 = 1;
/// Subject kind: 10-cell period-4 3D oscillator.
pub const SUBJECT_PERIOD4_OSCILLATOR: u8 = 2;
/// Subject kind: 6-cell 3D octahedron still life.
pub const SUBJECT_OCTAHEDRON_STILL_LIFE: u8 = 3;
/// Subject kind: deterministic `SplitMix64` matched-density 3D soup.
pub const SUBJECT_DETERMINISTIC_SOUP: u8 = 4;

/// Default frozen `SplitMix64` seeds for the bounded `EMERGENCE-R1` soup corpus.
pub const DEFAULT_EMERGENCE_SOUP_SEEDS: [u64; 4] =
    [0xB17B_0001, 0xB17B_0002, 0xB17B_0003, 0xB17B_0004];
/// Canonical world-space anchor corner of the `8×8×8` soup box (well clear of bedrock `(0,0,0)`).
pub const SOUP_BOX_ORIGIN: Coord = Coord::new(12, 12, 12);

/// Errors returned by `EMERGENCE-R1` corpus validation or wire decoding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EmergenceError {
    /// Corpus specification had zero ticks, invalid box edge, invalid density, or empty seeds.
    InvalidSpec(&'static str),
    /// Canonical wire buffer had invalid length or trailing bytes.
    InvalidWire(&'static str),
}

impl fmt::Display for EmergenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for EmergenceError {}

/// Canonical frozen corpus specification for `EMERGENCE-R1`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EmergenceCorpusSpecV1 {
    /// Schema version (`1`).
    pub schema_version: u16,
    /// Number of synchronous ticks executed per trajectory (`1..=256`).
    pub ticks: u16,
    /// Edge length of the cubic random soup box (`4..=16`).
    pub soup_box_edge: u8,
    /// Soup initial fill probability in basis points (`1..=9_999`; `2_000` = `20.00%`).
    pub soup_density_bps: u16,
    /// Preflight Moore candidate-cell cap per tick (`>= 1`).
    pub max_candidate_cells: u32,
    /// Preflight mutation cap per tick (`>= 1`, cleanly bounds supercritical blow-up).
    pub max_mutations_per_tick: u32,
    /// Ordered deterministic 64-bit `SplitMix64` seeds (`1..=32`).
    pub soup_seeds: Vec<u64>,
}

impl EmergenceCorpusSpecV1 {
    /// Returns the default frozen `EMERGENCE-R1` assay corpus specification (`20` ticks, `8³`
    /// soups at `20%` density, 4 frozen seeds across `B5/S45`, `B6/S45`, and `B4/S45`).
    pub fn v1_default() -> Self {
        Self {
            schema_version: 1,
            ticks: 20,
            soup_box_edge: 8,
            soup_density_bps: 2_000,
            max_candidate_cells: 60_000,
            max_mutations_per_tick: 4_096,
            soup_seeds: DEFAULT_EMERGENCE_SOUP_SEEDS.to_vec(),
        }
    }

    /// Validates all corpus bounds and deterministic seed parameters.
    pub fn validate(&self) -> Result<(), EmergenceError> {
        if self.schema_version != 1 {
            return Err(EmergenceError::InvalidSpec("schema_version must be 1"));
        }
        if self.ticks == 0 || self.ticks > 256 {
            return Err(EmergenceError::InvalidSpec("ticks must lie in 1..=256"));
        }
        if self.soup_box_edge < 4 || self.soup_box_edge > 16 {
            return Err(EmergenceError::InvalidSpec(
                "soup_box_edge must lie in 4..=16",
            ));
        }
        if self.soup_density_bps == 0 || self.soup_density_bps >= 10_000 {
            return Err(EmergenceError::InvalidSpec(
                "soup_density_bps must lie in 1..=9999",
            ));
        }
        if self.max_candidate_cells == 0 || self.max_mutations_per_tick == 0 {
            return Err(EmergenceError::InvalidSpec("caps must be > 0"));
        }
        if self.soup_seeds.is_empty() || self.soup_seeds.len() > 32 {
            return Err(EmergenceError::InvalidSpec(
                "soup_seeds length must lie in 1..=32",
            ));
        }
        Ok(())
    }

    /// Encodes the corpus spec into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, EmergenceError> {
        self.validate()?;
        let mut out = Vec::with_capacity(17 + self.soup_seeds.len() * 8);
        out.extend_from_slice(&self.schema_version.to_be_bytes());
        out.extend_from_slice(&self.ticks.to_be_bytes());
        out.push(self.soup_box_edge);
        out.extend_from_slice(&self.soup_density_bps.to_be_bytes());
        out.extend_from_slice(&self.max_candidate_cells.to_be_bytes());
        out.extend_from_slice(&self.max_mutations_per_tick.to_be_bytes());
        out.extend_from_slice(&(self.soup_seeds.len() as u16).to_be_bytes());
        for &seed in &self.soup_seeds {
            out.extend_from_slice(&seed.to_be_bytes());
        }
        Ok(out)
    }

    /// Decodes and validates a canonical `EmergenceCorpusSpecV1`.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, EmergenceError> {
        if bytes.len() < 17 {
            return Err(EmergenceError::InvalidWire(
                "truncated EmergenceCorpusSpecV1",
            ));
        }
        let seed_count = u16::from_be_bytes([bytes[15], bytes[16]]) as usize;
        if bytes.len() != 17 + seed_count * 8 {
            return Err(EmergenceError::InvalidWire(
                "wrong EmergenceCorpusSpecV1 byte length",
            ));
        }
        let mut soup_seeds = Vec::with_capacity(seed_count);
        for i in 0..seed_count {
            let off = 17 + i * 8;
            soup_seeds.push(u64::from_be_bytes(
                bytes[off..off + 8].try_into().expect("8"),
            ));
        }
        let spec = Self {
            schema_version: u16::from_be_bytes([bytes[0], bytes[1]]),
            ticks: u16::from_be_bytes([bytes[2], bytes[3]]),
            soup_box_edge: bytes[4],
            soup_density_bps: u16::from_be_bytes([bytes[5], bytes[6]]),
            max_candidate_cells: u32::from_be_bytes(bytes[7..11].try_into().expect("4")),
            max_mutations_per_tick: u32::from_be_bytes(bytes[11..15].try_into().expect("4")),
            soup_seeds,
        };
        spec.validate()?;
        Ok(spec)
    }

    /// Computes the canonical `Policy` `ContentTag` for this corpus spec.
    pub fn tag(&self) -> Result<ContentTag, EmergenceError> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            EMERGENCE_R1_CORPUS_SPEC_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and bounded length"))
    }
}

/// Deterministic `SplitMix64` PRNG step (zero ambient randomness; bit-identical in Rust & Python).
pub const fn splitmix64_next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Generates a deterministic cubic soup anchored at [`SOUP_BOX_ORIGIN`] using `SplitMix64`.
pub fn generate_deterministic_soup(seed: u64, box_edge: u8, density_bps: u16) -> World {
    let mut world = World::new();
    let mut state = seed;
    let edge = i64::from(box_edge);
    for dx in 0..edge {
        for dy in 0..edge {
            for dz in 0..edge {
                if (splitmix64_next(&mut state) % 10_000) < u64::from(density_bps) {
                    world.set_on(Coord::new(
                        SOUP_BOX_ORIGIN.x + dx,
                        SOUP_BOX_ORIGIN.y + dy,
                        SOUP_BOX_ORIGIN.z + dz,
                    ));
                }
            }
        }
    }
    world
}

/// Returns `(birth_mask, survive_mask)` for one of the three assay rules (`1..=3`).
pub const fn rule_masks_for_code(rule_code: u8) -> Option<(u32, u32)> {
    match rule_code {
        RULE_CODE_B5S45 => Some((BAYS_4555_BIRTH_MASK, BAYS_4555_SURVIVE_MASK)),
        RULE_CODE_B6S45_SUBCRITICAL => Some((1u32 << 6, BAYS_4555_SURVIVE_MASK)),
        RULE_CODE_B4S45_SUPERCRITICAL => Some((1u32 << 4, BAYS_4555_SURVIVE_MASK)),
        _ => None,
    }
}

/// Executes one synchronous bounded 26-neighbor Moore step for `rule_code` (keeping bedrock `1`).
/// Returns `Some((births, deaths))` or `None` when `max_candidate_cells` or `max_mutations` trips
/// fail-closed before mutation.
pub fn step_moore_rule_bounded(
    world: &mut World,
    rule_code: u8,
    max_candidate_cells: u32,
    max_mutations_per_tick: u32,
) -> Option<(u32, u32)> {
    let (birth_mask, survive_mask) = rule_masks_for_code(rule_code)?;
    let mut counts: HashMap<Coord, u8> = HashMap::new();
    let mut live_mutable: Vec<Coord> = Vec::new();
    for coord in world.iter_on() {
        if coord != BEDROCK_COORD {
            live_mutable.push(coord);
        }
        for dx in -1_i64..=1 {
            for dy in -1_i64..=1 {
                for dz in -1_i64..=1 {
                    if dx == 0 && dy == 0 && dz == 0 {
                        continue;
                    }
                    let (Some(nx), Some(ny), Some(nz)) = (
                        coord.x.checked_add(dx),
                        coord.y.checked_add(dy),
                        coord.z.checked_add(dz),
                    ) else {
                        continue;
                    };
                    let nb = Coord::new(nx, ny, nz);
                    if nb == BEDROCK_COORD {
                        continue;
                    }
                    let entry = counts.entry(nb).or_insert(0);
                    *entry = entry.saturating_add(1);
                    if counts.len() > max_candidate_cells as usize {
                        return None;
                    }
                }
            }
        }
    }
    let mut to_turn_on: Vec<Coord> = Vec::new();
    let mut to_turn_off: Vec<Coord> = Vec::new();
    for (&coord, &cnt) in &counts {
        if !world.is_on(coord) && ((birth_mask >> cnt) & 1) == 1 {
            to_turn_on.push(coord);
        }
    }
    for &coord in &live_mutable {
        let cnt = counts.get(&coord).copied().unwrap_or(0);
        if ((survive_mask >> cnt) & 1) == 0 {
            to_turn_off.push(coord);
        }
    }
    let total_muts = to_turn_on.len().saturating_add(to_turn_off.len());
    if total_muts > max_mutations_per_tick as usize {
        return None;
    }
    for &c in &to_turn_on {
        world.set_on(c);
    }
    for &c in &to_turn_off {
        world.set_off(c);
    }
    Some((to_turn_on.len() as u32, to_turn_off.len() as u32))
}

fn morton_compress_ratio_bps(world: &World) -> u32 {
    let coords: Vec<Coord> = world.iter_on().collect();
    if coords.is_empty() {
        return 0;
    }
    let encoded = MortonGapStreamV1::from_coords(&coords)
        .expect("occupied_coords are unique")
        .encoded_len() as u64;
    let raw = (coords.len() as u64) * 24;
    ((encoded * 10_000) / raw) as u32
}

fn mutable_coord_sum_and_count(world: &World) -> (i128, i128, i128, i128) {
    let (mut sx, mut sy, mut sz, mut n) = (0i128, 0i128, 0i128, 0i128);
    for c in world.iter_on() {
        if c != BEDROCK_COORD {
            sx += i128::from(c.x);
            sy += i128::from(c.y);
            sz += i128::from(c.z);
            n += 1;
        }
    }
    (sx, sy, sz, n)
}

fn scaled_centroid_l1_shift_x100(
    init: (i128, i128, i128, i128),
    fin: (i128, i128, i128, i128),
) -> u32 {
    if init.3 == 0 || fin.3 == 0 {
        return 0;
    }
    let axis = |s0: i128, n0: i128, s1: i128, n1: i128| -> i128 {
        let num = (s1 * n0 - s0 * n1).abs() * 100;
        let den = n0 * n1;
        num / den
    };
    let total = axis(init.0, init.3, fin.0, fin.3)
        + axis(init.1, init.3, fin.1, fin.3)
        + axis(init.2, init.3, fin.2, fin.3);
    total.min(i128::from(u32::MAX)) as u32
}

fn world_hamming_distance(a: &World, b: &World) -> u32 {
    let sa: HashSet<Coord> = a.iter_on().collect();
    let sb: HashSet<Coord> = b.iter_on().collect();
    sa.symmetric_difference(&sb).count() as u32
}

/// Exact 4-metric deterministic result for one trajectory in [`EmergenceAssayReportV1`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EmergenceTrajectoryMetricsV1 {
    /// Rule code (`1 = B5/S45`, `2 = B6/S45`, `3 = B4/S45`).
    pub rule_code: u8,
    /// Subject kind (`1..=4`).
    pub subject_kind: u8,
    /// Frozen 64-bit seed (`0` for engineered structures).
    pub seed: u64,
    /// Initial mutable live cell count (excluding permanent bedrock).
    pub initial_mutable_cells: u32,
    /// Final mutable live cell count at tick `T` (or last tick before cap hit).
    pub final_mutable_cells: u32,
    /// Peak mutable live cell count observed across the trajectory.
    pub peak_mutable_cells: u32,
    /// Number of simulated ticks with `mutable_cells > 0`.
    pub survival_ticks: u16,
    /// Number of simulated ticks with `births + deaths > 0`.
    pub active_mutation_ticks: u16,
    /// `true` iff preflight candidate/mutation cap tripped (supercritical blow-up indicator).
    pub cap_hit: bool,
    /// Final-state `MortonGapStreamV1` byte ratio vs raw `24 B/voxel` in integer basis points.
    pub morton_compress_bps: u32,
    /// Final-tick symmetric-difference Hamming distance from flipping 1 voxel at `t = 0`.
    pub perturbation_hamming_t_end: u32,
    /// Exact integer `100×` L1 centroid displacement from `t = 0` to final tick.
    pub centroid_l1_shift_x100: u32,
    /// Exact `world_tag` recurrence period (`0` if non-periodic within `T`, `1` still life, `4` osc).
    pub detected_period: u16,
    /// Final `world_tag` digest of the unperturbed trajectory.
    pub final_world_digest: [u8; 32],
}

impl EmergenceTrajectoryMetricsV1 {
    /// Fixed canonical big-endian wire length per trajectory (`73` bytes).
    pub const WIRE_LEN: usize = 73;
}

/// Canonical 4-metric emergence assay receipt binding all engineered + soup + control trajectories.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EmergenceAssayReportV1 {
    /// Schema version (`1`).
    pub schema_version: u16,
    /// Digest of the governing [`EmergenceCorpusSpecV1`].
    pub corpus_spec_digest: [u8; 32],
    /// Ordered deterministic trajectories covering `B5/S45` structures/soups and `B6`/`B4` controls.
    pub trajectories: Vec<EmergenceTrajectoryMetricsV1>,
}

impl EmergenceAssayReportV1 {
    /// Encodes the assay report into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(
            38 + self.trajectories.len() * EmergenceTrajectoryMetricsV1::WIRE_LEN,
        );
        out.extend_from_slice(&self.schema_version.to_be_bytes());
        out.extend_from_slice(&self.corpus_spec_digest);
        out.extend_from_slice(&(self.trajectories.len() as u32).to_be_bytes());
        for t in &self.trajectories {
            out.push(t.rule_code);
            out.push(t.subject_kind);
            out.extend_from_slice(&t.seed.to_be_bytes());
            out.extend_from_slice(&t.initial_mutable_cells.to_be_bytes());
            out.extend_from_slice(&t.final_mutable_cells.to_be_bytes());
            out.extend_from_slice(&t.peak_mutable_cells.to_be_bytes());
            out.extend_from_slice(&t.survival_ticks.to_be_bytes());
            out.extend_from_slice(&t.active_mutation_ticks.to_be_bytes());
            out.push(u8::from(t.cap_hit));
            out.extend_from_slice(&t.morton_compress_bps.to_be_bytes());
            out.extend_from_slice(&t.perturbation_hamming_t_end.to_be_bytes());
            out.extend_from_slice(&t.centroid_l1_shift_x100.to_be_bytes());
            out.extend_from_slice(&t.detected_period.to_be_bytes());
            out.extend_from_slice(&t.final_world_digest);
        }
        out
    }

    /// Computes the canonical `Receipt` `ContentTag` for this emergence assay report.
    pub fn tag(&self) -> ContentTag {
        let bytes = self.encode_canonical();
        hash_canonical(
            TagKind::Receipt,
            EMERGENCE_R1_ASSAY_REPORT_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and bounded length")
    }
}

/// Runs one trajectory (`initial_world` + 1-bit `perturb_coord`) under `rule_code` for `spec.ticks`.
pub fn evaluate_trajectory(
    spec: &EmergenceCorpusSpecV1,
    rule_code: u8,
    subject_kind: u8,
    seed: u64,
    initial_world: &World,
    perturb_coord: Coord,
) -> EmergenceTrajectoryMetricsV1 {
    let mut world = initial_world.clone();
    let mut perturbed = initial_world.clone();
    perturbed.toggle(perturb_coord);

    let init_stats = mutable_coord_sum_and_count(&world);
    let initial_mutable_cells = init_stats.3 as u32;
    let t0_tag = world_tag(&world);
    let mut peak_mutable_cells = initial_mutable_cells;
    let mut survival_ticks = 0u16;
    let mut active_mutation_ticks = 0u16;
    let mut cap_hit = false;
    let mut detected_period = 0u16;
    let mut executed_ticks = 0u16;

    for tick in 1..=spec.ticks {
        match step_moore_rule_bounded(
            &mut world,
            rule_code,
            spec.max_candidate_cells,
            spec.max_mutations_per_tick,
        ) {
            Some((births, deaths)) => {
                executed_ticks = tick;
                let live_mut = (world.len().saturating_sub(1)) as u32;
                if live_mut > peak_mutable_cells {
                    peak_mutable_cells = live_mut;
                }
                if live_mut > 0 {
                    survival_ticks = survival_ticks.saturating_add(1);
                }
                if births > 0 || deaths > 0 {
                    active_mutation_ticks = active_mutation_ticks.saturating_add(1);
                }
                if detected_period == 0 && world_tag(&world) == t0_tag {
                    detected_period = tick;
                }
            }
            None => {
                cap_hit = true;
                break;
            }
        }
    }

    for _ in 0..executed_ticks {
        if step_moore_rule_bounded(
            &mut perturbed,
            rule_code,
            spec.max_candidate_cells,
            spec.max_mutations_per_tick,
        )
        .is_none()
        {
            break;
        }
    }

    let final_stats = mutable_coord_sum_and_count(&world);
    EmergenceTrajectoryMetricsV1 {
        rule_code,
        subject_kind,
        seed,
        initial_mutable_cells,
        final_mutable_cells: final_stats.3 as u32,
        peak_mutable_cells,
        survival_ticks,
        active_mutation_ticks,
        cap_hit,
        morton_compress_bps: morton_compress_ratio_bps(&world),
        perturbation_hamming_t_end: world_hamming_distance(&world, &perturbed),
        centroid_l1_shift_x100: scaled_centroid_l1_shift_x100(init_stats, final_stats),
        detected_period,
        final_world_digest: world_tag(&world).digest(),
    }
}

/// Runs the complete frozen `EMERGENCE-R1` assay corpus (3 engineered `B5/S45` structures plus
/// matched-density deterministic soups across `B5/S45`, `B6/S45`, and `B4/S45`).
pub fn run_emergence_assay(
    spec: &EmergenceCorpusSpecV1,
) -> Result<EmergenceAssayReportV1, EmergenceError> {
    let spec_digest = spec.tag()?.digest();
    let mut trajectories = Vec::with_capacity(3 + 3 * spec.soup_seeds.len());

    // 1. Engineered B5/S45 reference structures (glider, period-4 oscillator, octahedron still life)
    let glider_origin = Coord::new(20, 20, 20);
    let mut w_glider = World::new();
    let glider_cells = bays_4555_glider_coords(glider_origin);
    for c in glider_cells {
        w_glider.set_on(c);
    }
    trajectories.push(evaluate_trajectory(
        spec,
        RULE_CODE_B5S45,
        SUBJECT_BAYS_GLIDER,
        0,
        &w_glider,
        glider_cells[0],
    ));

    let osc_origin = Coord::new(20, 20, 20);
    let mut w_osc = World::new();
    let osc_cells = period_4_oscillator_coords(osc_origin);
    for c in osc_cells {
        w_osc.set_on(c);
    }
    trajectories.push(evaluate_trajectory(
        spec,
        RULE_CODE_B5S45,
        SUBJECT_PERIOD4_OSCILLATOR,
        0,
        &w_osc,
        osc_cells[0],
    ));

    let oct_center = Coord::new(20, 20, 20);
    let mut w_oct = World::new();
    let oct_cells = octahedron_still_life_coords(oct_center);
    for c in oct_cells {
        w_oct.set_on(c);
    }
    trajectories.push(evaluate_trajectory(
        spec,
        RULE_CODE_B5S45,
        SUBJECT_OCTAHEDRON_STILL_LIFE,
        0,
        &w_oct,
        oct_cells[0],
    ));

    // 2. Matched-density deterministic soups across B5/S45, B6/S45 subcritical, B4/S45 supercritical
    let perturb_soup = Coord::new(
        SOUP_BOX_ORIGIN.x + i64::from(spec.soup_box_edge / 2),
        SOUP_BOX_ORIGIN.y + i64::from(spec.soup_box_edge / 2),
        SOUP_BOX_ORIGIN.z + i64::from(spec.soup_box_edge / 2),
    );
    for &rule_code in &[
        RULE_CODE_B5S45,
        RULE_CODE_B6S45_SUBCRITICAL,
        RULE_CODE_B4S45_SUPERCRITICAL,
    ] {
        for &seed in &spec.soup_seeds {
            let soup = generate_deterministic_soup(seed, spec.soup_box_edge, spec.soup_density_bps);
            trajectories.push(evaluate_trajectory(
                spec,
                rule_code,
                SUBJECT_DETERMINISTIC_SOUP,
                seed,
                &soup,
                perturb_soup,
            ));
        }
    }

    Ok(EmergenceAssayReportV1 {
        schema_version: 1,
        corpus_spec_digest: spec_digest,
        trajectories,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emergence_r1_corpus_round_trip_and_metric_separation() {
        let spec = EmergenceCorpusSpecV1::v1_default();
        let raw = spec.encode_canonical().unwrap();
        assert_eq!(EmergenceCorpusSpecV1::decode_canonical(&raw).unwrap(), spec);

        let report = run_emergence_assay(&spec).unwrap();
        assert_eq!(report.trajectories.len(), 15);
        // Engineered structures: glider transports 10.00 voxels L1, osc period=4, octahedron period=1
        assert_eq!(report.trajectories[0].centroid_l1_shift_x100, 1_000);
        assert_eq!(report.trajectories[0].final_mutable_cells, 10);
        assert_eq!(report.trajectories[1].detected_period, 4);
        assert_eq!(report.trajectories[2].detected_period, 1);
    }
}
