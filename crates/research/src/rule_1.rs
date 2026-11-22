//! `RULE-1`: opt-in finite synchronous 3D Moore-neighborhood Life (`B5/S45`) over Bitblock's
//! 1-bit sparse `World`.
//!
//! Implements the transformation law frozen in `docs/RULE_1_DECISION.md`:
//! - **Pure 1-bit state:** operates strictly over `{0, 1}` coordinate occupancy with zero hidden
//!   cell state, kept opt-in and strictly separate from default live editing, `ASSET-D1` balances,
//!   and Layer 1 consensus authority.
//! - **Carter Bays' Life 4555 (`B5/S45`):** synchronous 26-neighbor Moore stencil where an off
//!   coordinate turns `1` iff it has exactly `5` live neighbors, and a live mutable coordinate
//!   stays `1` iff it has `4` or `5` live neighbors.
//! - **Permanent bedrock `(0, 0, 0)`:** always clamped to `1`; contributes to its 26 neighbors'
//!   live counts and never dies or emits an `Off` edit.
//! - **Finite resource bounds:** enforces `max_candidate_cells` and `max_mutations_per_tick`
//!   preflight caps fail-closed before mutating `World` or `History`.
//! - **48-way octahedral isotropy & deterministic replay:** synchronous evaluation eliminates
//!   evaluation-order conflicts and replays identically through `History` (`ACCOUNT-1`).

use std::collections::HashMap;
use std::fmt;

use sha2::{Digest, Sha256};

use crate::history::{History, world_tag};
use crate::program::{Edit, Operation};
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::world::{BEDROCK_COORD, Coord, World};

/// Canonical schema string for a [`Rule1DescriptorV1`] policy object.
pub const RULE_1_DESCRIPTOR_SCHEMA: &[u8] = b"bitblock/rule-1/descriptor/v1";
/// Canonical schema string for a [`Rule1StepReceiptV1`] deterministic tick receipt.
pub const RULE_1_STEP_RECEIPT_SCHEMA: &[u8] = b"bitblock/rule-1/step-receipt/v1";

/// Default preflight cap on candidate Moore-neighborhood coordinates evaluated per tick.
pub const DEFAULT_RULE_1_MAX_CANDIDATE_CELLS: u32 = 1_000_000;
/// Default preflight cap on atomic `On`/`Off` mutations committed per tick.
pub const DEFAULT_RULE_1_MAX_MUTATIONS_PER_TICK: u32 = 65_536;

/// Canonical birth bitmask for Carter Bays' Life 4555 (`B5`: bit 5 set).
pub const BAYS_4555_BIRTH_MASK: u32 = 1 << 5;
/// Canonical survival bitmask for Carter Bays' Life 4555 (`S45`: bits 4 and 5 set).
pub const BAYS_4555_SURVIVE_MASK: u32 = (1 << 4) | (1 << 5);

/// Errors returned by `RULE-1` validation, preflight limits, or `History` commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rule1Error {
    /// Descriptor violated canonical `B5/S45`, bedrock, or limit invariants.
    InvalidDescriptor(&'static str),
    /// Wire buffer length or flag encoding was invalid.
    InvalidWire(&'static str),
    /// Candidate neighborhood exceeded `max_candidate_cells` before any state mutation.
    CandidateLimitExceeded { limit: u32, observed: usize },
    /// Pending diff edits exceeded `max_mutations_per_tick` before any state mutation.
    MutationLimitExceeded { limit: u32, observed: usize },
}

impl fmt::Display for Rule1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Rule1Error {}

/// Canonical versioned descriptor for the opt-in `RULE-1` autonomous transformation rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rule1DescriptorV1 {
    /// Descriptor schema version (`1`).
    pub schema_version: u16,
    /// Rule identifier (`1` = synchronous 3D Moore-neighborhood Life `B5/S45`).
    pub rule_code: u16,
    /// 27-bit birth mask over neighbor counts `0..=26` (`1 << 5` for `B5`).
    pub birth_mask: u32,
    /// 27-bit survival mask over neighbor counts `0..=26` (`(1 << 4) | (1 << 5)` for `S45`).
    pub survive_mask: u32,
    /// Must be `true`: origin `(0, 0, 0)` stays permanently `1`.
    pub bedrock_clamped_on: bool,
    /// Hard preflight cap on evaluated candidate coordinates (`>= 1`).
    pub max_candidate_cells: u32,
    /// Hard preflight cap on committed mutations per tick (`>= 1`).
    pub max_mutations_per_tick: u32,
}

impl Rule1DescriptorV1 {
    /// Fixed canonical wire byte length (`21` bytes).
    pub const WIRE_LEN: usize = 21;

    /// Returns the default frozen `RULE-1` descriptor (`B5/S45`, bedrock clamped, default bounds).
    pub const fn bays_4555_default() -> Self {
        Self {
            schema_version: 1,
            rule_code: 1,
            birth_mask: BAYS_4555_BIRTH_MASK,
            survive_mask: BAYS_4555_SURVIVE_MASK,
            bedrock_clamped_on: true,
            max_candidate_cells: DEFAULT_RULE_1_MAX_CANDIDATE_CELLS,
            max_mutations_per_tick: DEFAULT_RULE_1_MAX_MUTATIONS_PER_TICK,
        }
    }

    /// Returns a `B5/S45` descriptor with custom finite resource caps.
    pub const fn bays_4555_with_limits(
        max_candidate_cells: u32,
        max_mutations_per_tick: u32,
    ) -> Self {
        Self {
            max_candidate_cells,
            max_mutations_per_tick,
            ..Self::bays_4555_default()
        }
    }

    /// Validates all `RULE-1` rule, mask, bedrock, and resource-bound invariants.
    pub fn validate(&self) -> Result<(), Rule1Error> {
        if self.schema_version != 1 {
            return Err(Rule1Error::InvalidDescriptor("schema_version must be 1"));
        }
        if self.rule_code != 1 {
            return Err(Rule1Error::InvalidDescriptor(
                "rule_code must be 1 (MooreLifeB5S45V1)",
            ));
        }
        if (self.birth_mask & !((1u32 << 27) - 1)) != 0
            || (self.survive_mask & !((1u32 << 27) - 1)) != 0
        {
            return Err(Rule1Error::InvalidDescriptor(
                "neighbor mask bits above 26 are invalid",
            ));
        }
        if (self.birth_mask & 1) != 0 {
            return Err(Rule1Error::InvalidDescriptor(
                "B0 is forbidden on infinite sparse i64^3 space",
            ));
        }
        if self.birth_mask != BAYS_4555_BIRTH_MASK || self.survive_mask != BAYS_4555_SURVIVE_MASK {
            return Err(Rule1Error::InvalidDescriptor(
                "rule_code 1 pins B5/S45 masks exclusively",
            ));
        }
        if !self.bedrock_clamped_on {
            return Err(Rule1Error::InvalidDescriptor(
                "bedrock_clamped_on must be true",
            ));
        }
        if self.max_candidate_cells == 0 || self.max_mutations_per_tick == 0 {
            return Err(Rule1Error::InvalidDescriptor(
                "candidate and mutation limits must be > 0",
            ));
        }
        Ok(())
    }

    /// Encodes the descriptor into canonical big-endian bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], Rule1Error> {
        self.validate()?;
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.schema_version.to_be_bytes());
        out[2..4].copy_from_slice(&self.rule_code.to_be_bytes());
        out[4..8].copy_from_slice(&self.birth_mask.to_be_bytes());
        out[8..12].copy_from_slice(&self.survive_mask.to_be_bytes());
        out[12] = u8::from(self.bedrock_clamped_on);
        out[13..17].copy_from_slice(&self.max_candidate_cells.to_be_bytes());
        out[17..21].copy_from_slice(&self.max_mutations_per_tick.to_be_bytes());
        Ok(out)
    }

    /// Decodes and validates a canonical `Rule1DescriptorV1`.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Rule1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(Rule1Error::InvalidWire("invalid Rule1DescriptorV1 length"));
        }
        let bedrock_clamped_on = match bytes[12] {
            0 => false,
            1 => true,
            _ => return Err(Rule1Error::InvalidWire("non-boolean bedrock flag")),
        };
        let desc = Self {
            schema_version: u16::from_be_bytes([bytes[0], bytes[1]]),
            rule_code: u16::from_be_bytes([bytes[2], bytes[3]]),
            birth_mask: u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            survive_mask: u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
            bedrock_clamped_on,
            max_candidate_cells: u32::from_be_bytes([bytes[13], bytes[14], bytes[15], bytes[16]]),
            max_mutations_per_tick: u32::from_be_bytes([
                bytes[17], bytes[18], bytes[19], bytes[20],
            ]),
        };
        desc.validate()?;
        Ok(desc)
    }

    /// Computes the canonical `ContentTag` for this rule descriptor.
    pub fn tag(&self) -> Result<ContentTag, Rule1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            RULE_1_DESCRIPTOR_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

/// Pure pre-commit diff plan produced by evaluating one synchronous `RULE-1` tick.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule1StepPlan {
    /// Total distinct candidate coordinates evaluated in the 26-neighbor frontier.
    pub candidates_evaluated: u32,
    /// Canonically sorted atomic `Edit` list (`On` for births, `Off` for deaths).
    pub edits: Vec<Edit>,
    /// Number of off -> on births in `edits`.
    pub births: u32,
    /// Number of on -> off deaths in `edits`.
    pub deaths: u32,
    /// SHA-256 digest committing to the canonical sorted `edits` stream.
    pub edits_hash: [u8; 32],
}

/// Canonical deterministic receipt committing one executed `RULE-1` step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rule1StepReceiptV1 {
    /// Digest of the [`Rule1DescriptorV1`] tag active on this step.
    pub rule_descriptor_digest: [u8; 32],
    /// Monotonic tick number executed.
    pub tick: u64,
    /// Pre-step canonical `world_tag` digest.
    pub pre_world_digest: [u8; 32],
    /// Post-step canonical `world_tag` digest.
    pub post_world_digest: [u8; 32],
    /// Number of distinct Moore-neighborhood candidates evaluated.
    pub candidates_evaluated: u32,
    /// Number of births applied (`off -> on`).
    pub births: u32,
    /// Number of deaths applied (`on -> off`).
    pub deaths: u32,
    /// `true` iff zero mutations occurred (`births == 0 && deaths == 0`, fixed point).
    pub quiescent: bool,
    /// SHA-256 commitment to the canonical sorted edit list.
    pub edits_hash: [u8; 32],
}

impl Rule1StepReceiptV1 {
    /// Fixed canonical wire byte length (`149` bytes).
    pub const WIRE_LEN: usize = 149;

    /// Encodes the receipt into canonical big-endian bytes.
    pub fn encode_canonical(&self) -> [u8; Self::WIRE_LEN] {
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..32].copy_from_slice(&self.rule_descriptor_digest);
        out[32..40].copy_from_slice(&self.tick.to_be_bytes());
        out[40..72].copy_from_slice(&self.pre_world_digest);
        out[72..104].copy_from_slice(&self.post_world_digest);
        out[104..108].copy_from_slice(&self.candidates_evaluated.to_be_bytes());
        out[108..112].copy_from_slice(&self.births.to_be_bytes());
        out[112..116].copy_from_slice(&self.deaths.to_be_bytes());
        out[116] = u8::from(self.quiescent);
        out[117..149].copy_from_slice(&self.edits_hash);
        out
    }

    /// Decodes a canonical `Rule1StepReceiptV1`.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Rule1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(Rule1Error::InvalidWire("invalid Rule1StepReceiptV1 length"));
        }
        let quiescent = match bytes[116] {
            0 => false,
            1 => true,
            _ => return Err(Rule1Error::InvalidWire("non-boolean quiescent flag")),
        };
        let mut rule_descriptor_digest = [0u8; 32];
        let mut pre_world_digest = [0u8; 32];
        let mut post_world_digest = [0u8; 32];
        let mut edits_hash = [0u8; 32];
        rule_descriptor_digest.copy_from_slice(&bytes[0..32]);
        pre_world_digest.copy_from_slice(&bytes[40..72]);
        post_world_digest.copy_from_slice(&bytes[72..104]);
        edits_hash.copy_from_slice(&bytes[117..149]);
        Ok(Self {
            rule_descriptor_digest,
            tick: u64::from_be_bytes(bytes[32..40].try_into().expect("8 bytes")),
            pre_world_digest,
            post_world_digest,
            candidates_evaluated: u32::from_be_bytes([
                bytes[104], bytes[105], bytes[106], bytes[107],
            ]),
            births: u32::from_be_bytes([bytes[108], bytes[109], bytes[110], bytes[111]]),
            deaths: u32::from_be_bytes([bytes[112], bytes[113], bytes[114], bytes[115]]),
            quiescent,
            edits_hash,
        })
    }

    /// Computes the canonical `ContentTag` for this step receipt.
    pub fn tag(&self) -> ContentTag {
        let bytes = self.encode_canonical();
        hash_canonical(
            TagKind::Receipt,
            RULE_1_STEP_RECEIPT_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length")
    }
}

/// Evaluates one synchronous `RULE-1` step against `world` without mutating it, enforcing
/// `max_candidate_cells` and `max_mutations_per_tick` fail-closed.
pub fn compute_rule_1_plan(
    world: &World,
    descriptor: &Rule1DescriptorV1,
) -> Result<Rule1StepPlan, Rule1Error> {
    descriptor.validate()?;
    let max_cand = descriptor.max_candidate_cells as usize;
    let mut neighbor_counts: HashMap<Coord, u8> =
        HashMap::with_capacity(world.len().saturating_mul(16).min(max_cand));

    for live in world.iter_on() {
        // Ensure every live cell (even isolated with 0 neighbors) is evaluated for death.
        if !neighbor_counts.contains_key(&live) {
            if neighbor_counts.len() >= max_cand {
                return Err(Rule1Error::CandidateLimitExceeded {
                    limit: descriptor.max_candidate_cells,
                    observed: neighbor_counts.len() + 1,
                });
            }
            neighbor_counts.insert(live, 0);
        }
        for dx in -1_i64..=1 {
            let Some(nx) = live.x.checked_add(dx) else {
                continue;
            };
            for dy in -1_i64..=1 {
                let Some(ny) = live.y.checked_add(dy) else {
                    continue;
                };
                for dz in -1_i64..=1 {
                    if dx == 0 && dy == 0 && dz == 0 {
                        continue;
                    }
                    let Some(nz) = live.z.checked_add(dz) else {
                        continue;
                    };
                    let nb = Coord::new(nx, ny, nz);
                    if let Some(cnt) = neighbor_counts.get_mut(&nb) {
                        *cnt += 1;
                    } else {
                        if neighbor_counts.len() >= max_cand {
                            return Err(Rule1Error::CandidateLimitExceeded {
                                limit: descriptor.max_candidate_cells,
                                observed: neighbor_counts.len() + 1,
                            });
                        }
                        neighbor_counts.insert(nb, 1);
                    }
                }
            }
        }
    }

    let candidates_evaluated = neighbor_counts.len() as u32;
    let mut edits = Vec::new();
    let mut births = 0u32;
    let mut deaths = 0u32;
    for (coord, count) in neighbor_counts {
        if coord == BEDROCK_COORD {
            continue;
        }
        let bit = 1u32 << count;
        let is_alive = world.is_on(coord);
        if is_alive {
            if (descriptor.survive_mask & bit) == 0 {
                edits.push(Edit::new(Operation::Off, coord));
                deaths += 1;
            }
        } else if (descriptor.birth_mask & bit) != 0 {
            edits.push(Edit::new(Operation::On, coord));
            births += 1;
        }
    }

    if edits.len() > descriptor.max_mutations_per_tick as usize {
        return Err(Rule1Error::MutationLimitExceeded {
            limit: descriptor.max_mutations_per_tick,
            observed: edits.len(),
        });
    }

    edits.sort_by_key(|e| (e.coord.x, e.coord.y, e.coord.z, e.operation as u8));
    let mut h = Sha256::new();
    h.update((edits.len() as u32).to_be_bytes());
    for e in &edits {
        let code = match e.operation {
            Operation::On => 0u8,
            Operation::Off => 1u8,
            Operation::Toggle => 2u8,
        };
        h.update([code]);
        h.update(e.coord.x.to_be_bytes());
        h.update(e.coord.y.to_be_bytes());
        h.update(e.coord.z.to_be_bytes());
    }

    Ok(Rule1StepPlan {
        candidates_evaluated,
        edits,
        births,
        deaths,
        edits_hash: h.finalize().into(),
    })
}

/// Executes one atomic synchronous `RULE-1` step directly on `world`.
pub fn step_rule_1(
    world: &mut World,
    descriptor: &Rule1DescriptorV1,
    tick: u64,
) -> Result<Rule1StepReceiptV1, Rule1Error> {
    let desc_tag = descriptor.tag()?;
    let pre_world_digest = world_tag(world).digest();
    let plan = compute_rule_1_plan(world, descriptor)?;
    for edit in &plan.edits {
        match edit.operation {
            Operation::On => {
                world.set_on(edit.coord);
            }
            Operation::Off => {
                world.set_off(edit.coord);
            }
            Operation::Toggle => {
                world.toggle(edit.coord);
            }
        }
    }
    let post_world_digest = world_tag(world).digest();
    Ok(Rule1StepReceiptV1 {
        rule_descriptor_digest: desc_tag.digest(),
        tick,
        pre_world_digest,
        post_world_digest,
        candidates_evaluated: plan.candidates_evaluated,
        births: plan.births,
        deaths: plan.deaths,
        quiescent: plan.births == 0 && plan.deaths == 0,
        edits_hash: plan.edits_hash,
    })
}

/// Executes one synchronous `RULE-1` step and commits its canonical sorted edits through
/// `History` (`ACCOUNT-1` deterministic replay seam).
pub fn step_rule_1_with_history(
    world: &mut World,
    history: &mut History,
    descriptor: &Rule1DescriptorV1,
    tick: u64,
) -> Result<Rule1StepReceiptV1, Rule1Error> {
    let desc_tag = descriptor.tag()?;
    let pre_world_digest = world_tag(world).digest();
    let plan = compute_rule_1_plan(world, descriptor)?;
    let mut tx = history.begin_tick(tick);
    for &edit in &plan.edits {
        let _ = tx.apply_edit(history, world, edit);
    }
    let _ = history.commit_tick(tx, world);
    let post_world_digest = world_tag(world).digest();
    Ok(Rule1StepReceiptV1 {
        rule_descriptor_digest: desc_tag.digest(),
        tick,
        pre_world_digest,
        post_world_digest,
        candidates_evaluated: plan.candidates_evaluated,
        births: plan.births,
        deaths: plan.deaths,
        quiescent: plan.births == 0 && plan.deaths == 0,
        edits_hash: plan.edits_hash,
    })
}

/// Applies one of the 48 octahedral cube symmetries (`sym_index` in `0..48`) to `coord`:
/// 6 axis permutations × 8 sign flips.
pub const fn octahedral_transform(coord: Coord, sym_index: u8) -> Coord {
    let perm = (sym_index / 8) % 6;
    let signs = sym_index % 8;
    let (a, b, c) = match perm {
        0 => (coord.x, coord.y, coord.z),
        1 => (coord.x, coord.z, coord.y),
        2 => (coord.y, coord.x, coord.z),
        3 => (coord.y, coord.z, coord.x),
        4 => (coord.z, coord.x, coord.y),
        _ => (coord.z, coord.y, coord.x),
    };
    let sx = if (signs & 1) != 0 { -a } else { a };
    let sy = if (signs & 2) != 0 { -b } else { b };
    let sz = if (signs & 4) != 0 { -c } else { c };
    Coord::new(sx, sy, sz)
}

/// Returns Carter Bays' verified 10-cell period-4 3D glider in `B5/S45` (displaces `(0, +1, +1)`
/// every 4 ticks), translated by `offset`.
pub fn bays_4555_glider_coords(offset: Coord) -> [Coord; 10] {
    const BASE: [(i64, i64, i64); 10] = [
        (0, 0, 0),
        (0, 0, 1),
        (1, 0, 2),
        (1, 1, 0),
        (1, 1, 1),
        (2, 0, 2),
        (2, 1, 0),
        (2, 1, 1),
        (3, 0, 0),
        (3, 0, 1),
    ];
    BASE.map(|(x, y, z)| Coord::new(offset.x + x, offset.y + y, offset.z + z))
}

/// Returns the 6-cell 3D octahedron still life in `B5/S45` (quiescent on tick 1), centered at
/// `center`.
pub fn octahedron_still_life_coords(center: Coord) -> [Coord; 6] {
    const BASE: [(i64, i64, i64); 6] = [
        (-1, 0, 0),
        (1, 0, 0),
        (0, -1, 0),
        (0, 1, 0),
        (0, 0, -1),
        (0, 0, 1),
    ];
    BASE.map(|(x, y, z)| Coord::new(center.x + x, center.y + y, center.z + z))
}

/// Returns the 10-cell period-4 3D oscillator in `B5/S45` (`10 -> 8 -> 10 -> 8 -> 10`),
/// translated by `offset`.
pub fn period_4_oscillator_coords(offset: Coord) -> [Coord; 10] {
    const BASE: [(i64, i64, i64); 10] = [
        (0, 0, 0),
        (0, 1, 0),
        (1, 0, 0),
        (1, 2, 0),
        (2, 1, 0),
        (0, 0, 1),
        (0, 1, 1),
        (1, 0, 1),
        (1, 2, 1),
        (2, 1, 1),
    ];
    BASE.map(|(x, y, z)| Coord::new(offset.x + x, offset.y + y, offset.z + z))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bays_glider_translates_and_bedrock_survives_overcrowding() {
        let desc = Rule1DescriptorV1::bays_4555_default();
        let raw = desc.encode_canonical().unwrap();
        assert_eq!(Rule1DescriptorV1::decode_canonical(&raw).unwrap(), desc);

        // Place Bays glider far from bedrock (20, 20, 20) -> after 4 ticks shifts by (0, +1, +1).
        let mut world = World::new();
        let origin = Coord::new(20, 20, 20);
        for c in bays_4555_glider_coords(origin) {
            assert!(world.set_on(c));
        }
        for tick in 1..=4 {
            let r = step_rule_1(&mut world, &desc, tick).unwrap();
            let rr = r.encode_canonical();
            assert_eq!(Rule1StepReceiptV1::decode_canonical(&rr).unwrap(), r);
            assert!(!r.quiescent);
        }
        assert!(world.is_on(BEDROCK_COORD));
        assert_eq!(world.len(), 11); // bedrock + 10 glider cells
        for expected in bays_4555_glider_coords(Coord::new(20, 21, 21)) {
            assert!(
                world.is_on(expected),
                "missing shifted glider cell {expected:?}"
            );
        }

        // Overcrowded 2x2x2 cube touching bedrock (0,0,0): all 7 mutable cells die, bedrock stays 1.
        let mut crowded = World::new();
        for x in 0..=1 {
            for y in 0..=1 {
                for z in 0..=1 {
                    crowded.set_on(Coord::new(x, y, z));
                }
            }
        }
        assert_eq!(crowded.len(), 8);
        let r = step_rule_1(&mut crowded, &desc, 1).unwrap();
        assert_eq!(r.deaths, 7);
        assert_eq!(r.births, 0);
        assert_eq!(crowded.len(), 1);
        assert!(crowded.is_on(BEDROCK_COORD));
    }
}
