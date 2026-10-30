//! Local vapor (Driven water cycle): the global `vapor` book split into
//! regional bins so clouds and the vapor greenhouse sit where the water
//! actually evaporates.
//!
//! The book quantity is still the single `ChemWorld::vapor` count (lanes:
//! 4 H + 2 O per pair; energy: `LATENT_PAIR - BIND[WATER]` per pair). The
//! bins are an *attribution* of that count with the invariant
//! `sum(vapor_bins) == vapor`, so the books are untouched by construction:
//! evaporation and degassing add a pair to the bin of the cell they leave,
//! rain removes one from the bin it falls in, and transport only moves pairs
//! between bins.
//!
//! Regions ([`VaporLocality::Patches`]) are a cube-sphere split of directions
//! from the M1 core cell: the dominant axis of `cell - core` picks one of six
//! faces, the other two components pick an `n x n` tile. No noise, no
//! texture: a bin holds exactly the pairs its own cells evaporated, minus its
//! own rain, plus/minus integer diffusion with adjacent bins (rim-density
//! difference relaxed over [`VAPOR_MIX_TAU`] ticks per edge, stochastic
//! rounding, exactly conserving).
//!
//! Rain is per region too: a region rains `bin / RAIN_TAU` drops on its own
//! sky-open ground (then its waterline). Under [`RainFall::Core`] (default,
//! §5s) rain falls toward the core: a landing spot is AIR whose outward
//! lattice step (away from the core) is AIR or space and which rests on a
//! non-AIR cell of its inward support set, so every face of the ball, the
//! underside included, has its own landing ground. Under [`RainFall::Down`]
//! (§5b–§5r) rain falls along -y (AIR open above, resting on ground), so the
//! underside and lower flanks have no landing ground and their drops spill
//! onto the planet's whole landing line (the global pick, counted in
//! `ChemWorld::rain_spilled`); without that spill the vapor waits forever,
//! which slowed the ocean water cycle about 3.7x (§5r, v1). Under `Core` the
//! spill survives only as a diagnostic fallback for a region with no landing
//! ground and no waterline at all.
//!
//! [`VaporLocality::Global`] is one bin over the whole rim, which reproduces
//! the single global pool (and the §5q cloud-albedo build) bit-for-bit.

use std::collections::BTreeSet;
use std::sync::Arc;

use super::p6::{wide_roll, Topology, NB6};
use super::ChemWorld;
use crate::world::Coord;

/// Tiles per cube face edge for the default [`VaporLocality::PATCHES`]
/// (`6 * n * n` regions; 3 -> 54).
pub const VAPOR_PATCH_N: u8 = 3;
/// Inter-region vapor mixing time (ticks): each adjacent pair of regions
/// relaxes `1 / VAPOR_MIX_TAU` of its rim-density difference per tick
/// (rain removes `1 / RAIN_TAU = 1/512` of a region's vapor per tick).
pub const VAPOR_MIX_TAU: u32 = 2048;

const SALT_MIX: u32 = 0x5a17_3c11;

/// Where the Driven vapor pool lives (no effect under Classic).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VaporLocality {
    /// One planet-wide pool (the §5o–§5q model).
    Global,
    /// `6 * n * n` cube-sphere regions around the core with adjacent-region
    /// diffusion every tick (`mix_tau == 0`: no transport).
    Patches { n: u8, mix_tau: u32 },
}

impl VaporLocality {
    /// The tuned default: [`VAPOR_PATCH_N`] / [`VAPOR_MIX_TAU`].
    pub const PATCHES: Self = Self::Patches { n: VAPOR_PATCH_N, mix_tau: VAPOR_MIX_TAU };

    /// Number of vapor bins this model uses.
    pub fn patch_count(self) -> usize {
        match self {
            Self::Global => 1,
            Self::Patches { n, .. } => 6 * usize::from(n.max(1)).pow(2),
        }
    }
}

impl Default for VaporLocality {
    fn default() -> Self {
        Self::PATCHES
    }
}

/// Direction of the Driven rain fall test (no effect under Classic).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum RainFall {
    /// World -y (the §5b–§5r test): AIR with AIR or space at +y, resting on a
    /// non-AIR cell at -y. Regions with no such ground spill (see module doc).
    Down,
    /// Toward the core (§5s): AIR whose outward lattice step
    /// ([`super::p6_rest::outward_step`] of `cell - core`) is AIR or space,
    /// resting on a non-AIR cell of its inward support set. A region spills
    /// only if it has no landing ground and no waterline at all; when its
    /// probes all hit cells that already rained this tick the rest wait.
    #[default]
    Core,
}

/// Which way Driven surface gravity pulls water, sediment and rising gas
/// (`flow`, `settle`, `scour`, `entrain`, gas rise, `degas` and the
/// `collapse` direction gate; no effect under Classic, which is planet6's world
/// `-y` bit-for-bit).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum Gravity {
    /// World `-y` (§5b–§5v): water falls to the `-y` cell, else spreads to the
    /// first AIR of `-z, -x, +z, +x`; SED sinks along `-y`; gas rises along
    /// `+y`. Pin this for a flat plate or for parity with pre-§5w soaks.
    Down,
    /// Toward the core (§5w), the frame of [`RainFall::Core`] and
    /// [`EvapTop::Core`]: "down" is the inward support set of the cell
    /// (`-outward_step(cell - core)` and, on a diagonal, its axis
    /// components); "level" is the squared distance from the core, and a
    /// sideways step may raise it by at most 1 (a pure lattice tangent step).
    /// Gas rises along the outward step. With no core cell it falls back to
    /// `Down`.
    #[default]
    Core,
}

/// Which WATER cells count as a pool top for Driven evaporation (no effect
/// under Classic).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum EvapTop {
    /// World +y (the §5b–§5s test): the cell above is not WATER. Flank and
    /// underside puddles stacked along their own outward direction only
    /// evaporate from whichever cell happens to have no WATER at world +y.
    Up,
    /// Away from the core (§5t), the same frame as [`RainFall::Core`]: the
    /// `Up` material test turned radial. The cell's outward lattice step
    /// ([`super::p6_rest::outward_step`] of `cell - core`) is space or not
    /// WATER, so every face of the ball, the underside included, evaporates
    /// from the outer end of its own water column (and, as under `Up`,
    /// rock- or ice-capped water still evaporates). The core is the domain
    /// cell nearest the bounding-box centre, so a flat plate should pin `Up`
    /// (as it pins [`RainFall::Down`]).
    #[default]
    Core,
    /// Strict variant (§5t A/B, not the default): the outward step must be
    /// AIR or space, rain's landing test. Measured: it starves the cycle
    /// (evap -16..-44% vs §5s at 300k) because ice-lidded and rock-pocketed
    /// water stops evaporating and pools.
    CoreAir,
}

/// Cube-sphere region of direction `d` (cell minus core) with `n` tiles per
/// face edge: face `2 * axis + (d_axis > 0)` for the dominant axis (ties go
/// to the lower axis), tile from the other two components. `d = 0` -> 0.
pub fn vapor_patch(d: (i64, i64, i64), n: u8) -> usize {
    let n = i64::from(n.max(1));
    let c = [d.0, d.1, d.2];
    let a = c.map(i64::abs);
    let m = a[0].max(a[1]).max(a[2]);
    if m == 0 {
        return 0;
    }
    let ax = if a[0] == m { 0 } else if a[1] == m { 1 } else { 2 };
    let face = 2 * ax + usize::from(c[ax] > 0);
    let (u, v) = match ax {
        0 => (c[1], c[2]),
        1 => (c[0], c[2]),
        _ => (c[0], c[1]),
    };
    let tile = |x: i64| ((x + m) * n / (2 * m + 1)).clamp(0, n - 1);
    ((face as i64 * n + tile(u)) * n + tile(v)) as usize
}

/// Cached region geometry for one topology and locality model.
#[derive(Debug)]
pub(crate) struct VaporGeo {
    topo: Arc<Topology>,
    pub(crate) mode: VaporLocality,
    /// Region of every domain cell.
    pub(crate) patch: Vec<u16>,
    /// Rim cells (cells with a space face) per region.
    pub(crate) rim: Vec<i64>,
    /// Adjacent region pairs `(a, b)`, `a < b`, from 6-adjacent rim cells.
    pub(crate) edges: Vec<(u16, u16)>,
    /// Region adjacency (from `edges`), ascending neighbour index.
    pub(crate) adj: Vec<Vec<u16>>,
}

impl VaporGeo {
    fn build(cells: &[Coord], topo: &Arc<Topology>, mode: VaporLocality) -> Self {
        let pc = mode.patch_count();
        let core = topo.core.map(|c| cells[c as usize]);
        let patch: Vec<u16> = match (mode, core) {
            (VaporLocality::Patches { n, .. }, Some(o)) => cells
                .iter()
                .map(|c| vapor_patch((c.x - o.x, c.y - o.y, c.z - o.z), n) as u16)
                .collect(),
            _ => vec![0; cells.len()],
        };
        let mut rim = vec![0i64; pc];
        let mut is_rim = vec![false; cells.len()];
        for &(i, _) in &topo.space_faces {
            rim[patch[i as usize] as usize] += 1;
            is_rim[i as usize] = true;
        }
        let mut edges = BTreeSet::new();
        for &(i, _) in &topo.space_faces {
            let i = i as usize;
            for d in NB6 {
                if let Some(j) = topo.offset(cells, i, d) {
                    if is_rim[j] && patch[j] != patch[i] {
                        edges.insert((patch[i].min(patch[j]), patch[i].max(patch[j])));
                    }
                }
            }
        }
        let edges: Vec<(u16, u16)> = edges.into_iter().collect();
        let mut adj = vec![Vec::new(); pc];
        for &(a, b) in &edges {
            adj[usize::from(a)].push(b);
            adj[usize::from(b)].push(a);
        }
        for v in &mut adj {
            v.sort_unstable();
        }
        Self { topo: topo.clone(), mode, patch, rim, edges, adj }
    }
}

/// Spread `v` pairs over regions in proportion to their rim (largest
/// remainder, lowest index first on ties); all in region 0 when `v <= 0` or
/// there is no rim.
fn spread(v: i64, rim: &[i64]) -> Vec<i64> {
    let mut out = vec![0i64; rim.len().max(1)];
    let total: i64 = rim.iter().sum();
    if v <= 0 || total <= 0 {
        out[0] = v;
        return out;
    }
    let mut rem: Vec<(i128, usize)> = Vec::with_capacity(rim.len());
    let mut left = v;
    for (p, &r) in rim.iter().enumerate() {
        let x = i128::from(v) * i128::from(r);
        let q = x / i128::from(total);
        out[p] = q as i64;
        left -= q as i64;
        rem.push((x % i128::from(total), p));
    }
    rem.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    for &(_, p) in rem.iter().take(left as usize) {
        out[p] += 1;
    }
    out
}

impl ChemWorld {
    /// Makes the region geometry and the bins valid for the current topology
    /// and [`super::P6Params::vapor_local`] (Driven only). Bins whose count
    /// or sum no longer matches the booked `vapor` (a test or loader wrote
    /// `vapor` directly, or the model changed) are re-spread by rim.
    pub(crate) fn vapor_sync(&mut self, topo: &Arc<Topology>) {
        let mode = self.params.vapor_local;
        let ok = self
            .vgeo
            .as_ref()
            .is_some_and(|g| Arc::ptr_eq(&g.topo, topo) && g.mode == mode && g.patch.len() == self.cells.len());
        if !ok {
            self.vgeo = Some(Arc::new(VaporGeo::build(&self.cells, topo, mode)));
        }
        let g = self.vgeo.clone().expect("vapor geometry");
        let pc = g.rim.len();
        if self.patch_diag.len() != pc {
            self.patch_diag = vec![[0; 5]; pc];
        }
        if self.vapor_bins.len() != pc
            || self.vapor_bins.iter().sum::<i64>() != self.vapor
            || self.vapor_bins.iter().any(|v| *v < 0)
        {
            self.vapor_bins = spread(self.vapor, &g.rim);
        }
    }

    /// Region of domain cell `i` under the synced geometry (0 if none).
    #[inline]
    pub(crate) fn vapor_bin_of(&self, i: usize) -> usize {
        self.vgeo.as_ref().map_or(0, |g| usize::from(g.patch[i]))
    }

    /// Adds `d` pairs to the region of cell `i` (keep `vapor` in step).
    #[inline]
    pub(crate) fn vapor_bin_add(&mut self, i: usize, d: i64) {
        let b = self.vapor_bin_of(i);
        if let Some(v) = self.vapor_bins.get_mut(b) {
            *v += d;
        }
    }

    /// Regions with landing ground nearest to region `b` in region-graph
    /// hops (`has[r]`: region `r` can land a drop), and that hop count; empty
    /// when no region can land or there is no geometry. `b` itself at 0 hops.
    pub(crate) fn nearest_ground(&self, b: usize, has: &[bool]) -> (Vec<usize>, u32) {
        let Some(g) = self.vgeo.as_ref() else { return (Vec::new(), 0) };
        let pc = g.adj.len();
        if b >= pc || has.len() != pc {
            return (Vec::new(), 0);
        }
        let mut seen = vec![false; pc];
        let mut ring = vec![b];
        seen[b] = true;
        let mut hops = 0u32;
        while !ring.is_empty() {
            let hit: Vec<usize> = ring.iter().copied().filter(|&r| has[r]).collect();
            if !hit.is_empty() {
                let mut hit = hit;
                hit.sort_unstable();
                return (hit, hops);
            }
            let mut next = Vec::new();
            for &r in &ring {
                for &q in &g.adj[r] {
                    let q = usize::from(q);
                    if !seen[q] {
                        seen[q] = true;
                        next.push(q);
                    }
                }
            }
            ring = next;
            hops += 1;
        }
        (Vec::new(), 0)
    }

    /// Adjacent-region diffusion: every edge moves `1 / mix_tau` of the pair's
    /// equalising transfer (rim density) from a snapshot, stochastically
    /// rounded, applied in edge order clamped to what the source holds.
    /// Moves pairs between bins only, so `vapor` and every book are unchanged.
    pub(crate) fn vapor_mix_pass(&mut self) {
        let Some(g) = self.vgeo.clone() else { return };
        let VaporLocality::Patches { mix_tau, .. } = g.mode else { return };
        if mix_tau == 0 || g.edges.is_empty() || self.vapor_bins.len() != g.rim.len() {
            return;
        }
        let seed = self.params.seed;
        let tick = self.tick;
        let snap = self.vapor_bins.clone();
        let mut moves: Vec<(usize, usize, i64)> = Vec::new();
        for &(a, b) in &g.edges {
            let (a, b) = (usize::from(a), usize::from(b));
            let (ra, rb) = (i128::from(g.rim[a]), i128::from(g.rim[b]));
            let num = i128::from(snap[a]) * rb - i128::from(snap[b]) * ra;
            if num == 0 {
                continue;
            }
            let den = (ra + rb) * i128::from(mix_tau);
            let mag = num.abs();
            let key = Coord::new(a as i64, b as i64, 0);
            let roll = i128::from(
                u64::from(wide_roll(seed ^ SALT_MIX, key, tick)) << 32
                    | u64::from(wide_roll(seed ^ SALT_MIX ^ 0x0100_0000, key, tick)),
            ) % den;
            let x = (mag / den + i128::from(roll < mag % den)) as i64;
            if x == 0 {
                continue;
            }
            if num > 0 {
                moves.push((a, b, x));
            } else {
                moves.push((b, a, x));
            }
        }
        for (s, d, x) in moves {
            let x = x.min(self.vapor_bins[s]).max(0);
            self.vapor_bins[s] -= x;
            self.vapor_bins[d] += x;
        }
    }
}
