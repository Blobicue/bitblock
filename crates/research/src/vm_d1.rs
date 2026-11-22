//! `VM-D1`: frozen deterministic local visual-machine semantics over `LANGUAGE-R1` v1 opcodes.
//!
//! Defines the canonical instruction semantics, checked `i64` stack machine, pre-execution gas/work
//! formula, explicit deterministic traps, fail-closed atomic rollback, canonical ascending-anchor
//! multi-program scheduler, and deterministic voxel output register mapping strictly before
//! `BOUNDS-1@program` (`CE-05`) and `VM-1`:
//! - **Exact `LANGUAGE-R1` v1 opcode set:** operates over all 8 canonical opcodes (`Nop`,
//!   `PushOne`, `PushZero`, `Add`, `Sub`, `Dup`, `Swap`, `Halt`) with zero randomness and zero
//!   camera/renderer input.
//! - **Deterministic world output:** when an invocation halts cleanly and specifies an explicit
//!   `output_target = Some(VmOutputRegisterV1 { origin, bit_width })`, the low `bit_width` bits
//!   (`1..=64`) of the top-of-stack `i64` (unsigned two's-complement bit pattern) map to explicit
//!   `On`/`Off` voxel writes at `origin + (k, 0, 0)` for `k in 0..bit_width`, strictly forbidding
//!   bedrock `(0, 0, 0)`, every parsed program bounding box, and any coordinate already written by
//!   an earlier program in the tick.
//! - **Fail-closed atomic rollback & `History` parity:** any trap produces zero world mutations
//!   while accounting its metered work in [`VmBatchReceiptV1`]; accepted explicit `On`/`Off` writes
//!   commit through `History` (`ACCOUNT-1`).
//!
//! Frozen VM semantics do not implement the production `VM-1` interpreter or grant consensus
//! authority (`PROGRAM-CONSENSUS` remains locked).

use std::collections::BTreeSet;
use std::fmt;

use crate::history::{History, world_tag};
use crate::language_r1::{
    ParsedSpatialProgramV1, SPATIAL_FRAME_Y, SPATIAL_FRAME_Z, SPATIAL_GRAMMAR_VERSION,
    SpatialOpcodeV1, SpatialParseError, parse_world_programs,
};
use crate::program::{Edit, Operation};
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::world::{BEDROCK_COORD, Coord, World};

/// Canonical schema string for a frozen [`VmSpecD1`] semantics specification.
pub const VM_D1_SPEC_SCHEMA: &[u8] = b"bitblock/vm-d1/semantics-spec/v1";
/// Canonical schema string for a [`VmBatchReceiptV1`] deterministic model execution receipt.
pub const VM_D1_MODEL_RECEIPT_SCHEMA: &[u8] = b"bitblock/vm-d1/model-step-receipt/v1";

/// Default maximum stack depth for `VM-D1` v1.
pub const DEFAULT_VM_D1_MAX_STACK_DEPTH: u16 = 64;
/// Default maximum output register bit width (`1..=64`) for `VM-D1` v1.
pub const DEFAULT_VM_D1_MAX_OUTPUT_BITS: u16 = 64;
/// Default per-opcode gas schedule indexed by `SpatialOpcodeV1::code()` (`0..8`).
pub const DEFAULT_VM_D1_OPCODE_GAS: [u16; 8] = [1, 2, 2, 3, 3, 2, 2, 1];
/// Default gas cost per emitted explicit output voxel write bit.
pub const DEFAULT_VM_D1_GAS_PER_OUTPUT_BIT: u16 = 10;

/// Errors returned by `VM-D1` spec validation, parsing, or batch invocation setup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VmD1Error {
    /// Specification parameter violated `VM-D1` determinism or non-zero gas invariants.
    InvalidSpec(&'static str),
    /// Wire buffer had wrong length, invalid flags, or trailing bytes.
    InvalidWire(&'static str),
    /// Spatial grammar parsing failed before VM scheduling.
    Parse(SpatialParseError),
    /// Two invocations targeted the same anchor coordinate.
    DuplicateInvocationAnchor(Coord),
    /// Output register requested zero bits or exceeded `max_output_bits`.
    InvalidOutputWidth(u8),
}

impl From<SpatialParseError> for VmD1Error {
    fn from(value: SpatialParseError) -> Self {
        Self::Parse(value)
    }
}

impl fmt::Display for VmD1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for VmD1Error {}

/// Canonical frozen `VM-D1` semantics specification (`bitblock/vm-d1/semantics-spec/v1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VmSpecD1 {
    /// Specification version (`1`).
    pub spec_version: u16,
    /// Bound `LANGUAGE-R1` spatial grammar version (`1`).
    pub grammar_version: u16,
    /// Must be `true`: zero randomness input in v1.
    pub zero_randomness: bool,
    /// Must be `true`: strictly independent of camera pose and renderer state.
    pub camera_renderer_independent: bool,
    /// Must be `true`: checked signed `i64` arithmetic traps on overflow.
    pub checked_i64_arithmetic: bool,
    /// Must be `true`: any trap rolls back all pending writes for that invocation atomically.
    pub atomic_rollback_on_trap: bool,
    /// Maximum integer stack depth (`1..=256`).
    pub max_stack_depth: u16,
    /// Maximum output voxel register bit width (`1..=64`).
    pub max_output_bits: u16,
    /// Fixed deterministic pre-execution gas charge per `SpatialOpcodeV1` (`0..8`), each `> 0`.
    pub opcode_gas: [u16; 8],
    /// Fixed deterministic gas charge per emitted output register voxel write (`> 0`).
    pub gas_per_output_bit: u16,
}

impl VmSpecD1 {
    /// Fixed canonical big-endian wire length (`27` bytes).
    pub const WIRE_LEN: usize = 27;

    /// Returns the canonical v1 `VM-D1` semantics specification.
    pub const fn v1_default() -> Self {
        Self {
            spec_version: 1,
            grammar_version: SPATIAL_GRAMMAR_VERSION,
            zero_randomness: true,
            camera_renderer_independent: true,
            checked_i64_arithmetic: true,
            atomic_rollback_on_trap: true,
            max_stack_depth: DEFAULT_VM_D1_MAX_STACK_DEPTH,
            max_output_bits: DEFAULT_VM_D1_MAX_OUTPUT_BITS,
            opcode_gas: DEFAULT_VM_D1_OPCODE_GAS,
            gas_per_output_bit: DEFAULT_VM_D1_GAS_PER_OUTPUT_BIT,
        }
    }

    /// Validates every determinism, arithmetic, rollback, and positive-gas invariant fail-closed.
    pub fn validate(&self) -> Result<(), VmD1Error> {
        if self.spec_version != 1 {
            return Err(VmD1Error::InvalidSpec("spec_version must be 1"));
        }
        if self.grammar_version != SPATIAL_GRAMMAR_VERSION {
            return Err(VmD1Error::InvalidSpec("grammar_version must match v1"));
        }
        if !self.zero_randomness
            || !self.camera_renderer_independent
            || !self.checked_i64_arithmetic
            || !self.atomic_rollback_on_trap
        {
            return Err(VmD1Error::InvalidSpec(
                "all four VM-D1 determinism and atomic-rollback flags must be true",
            ));
        }
        if self.max_stack_depth == 0 || self.max_stack_depth > 256 {
            return Err(VmD1Error::InvalidSpec(
                "max_stack_depth must lie in 1..=256",
            ));
        }
        if self.max_output_bits == 0 || self.max_output_bits > 64 {
            return Err(VmD1Error::InvalidSpec("max_output_bits must lie in 1..=64"));
        }
        if self.opcode_gas.contains(&0) || self.gas_per_output_bit == 0 {
            return Err(VmD1Error::InvalidSpec(
                "all opcode and output-bit gas charges must be > 0",
            ));
        }
        Ok(())
    }

    /// Returns the deterministic pre-execution gas charge for one instruction opcode.
    pub const fn gas_for_opcode(&self, op: SpatialOpcodeV1) -> u64 {
        self.opcode_gas[op.code() as usize] as u64
    }

    /// Computes the exact worst-case pre-execution work upper bound for `program` and `output_bits`.
    pub fn pre_execution_work_upper_bound(
        &self,
        program: &ParsedSpatialProgramV1,
        output_bits: u8,
    ) -> Result<u64, VmD1Error> {
        self.validate()?;
        if u16::from(output_bits) > self.max_output_bits {
            return Err(VmD1Error::InvalidOutputWidth(output_bits));
        }
        let mut total = 0u64;
        for &op in &program.instructions {
            total = total
                .checked_add(self.gas_for_opcode(op))
                .ok_or(VmD1Error::InvalidSpec("gas sum overflow"))?;
            if op == SpatialOpcodeV1::Halt {
                break;
            }
        }
        let write_gas = u64::from(output_bits)
            .checked_mul(u64::from(self.gas_per_output_bit))
            .ok_or(VmD1Error::InvalidSpec("write gas overflow"))?;
        total
            .checked_add(write_gas)
            .ok_or(VmD1Error::InvalidSpec("total gas overflow"))
    }

    /// Encodes the specification into 27 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], VmD1Error> {
        self.validate()?;
        let flags = u8::from(self.zero_randomness)
            | (u8::from(self.camera_renderer_independent) << 1)
            | (u8::from(self.checked_i64_arithmetic) << 2)
            | (u8::from(self.atomic_rollback_on_trap) << 3);
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.spec_version.to_be_bytes());
        out[2..4].copy_from_slice(&self.grammar_version.to_be_bytes());
        out[4] = flags;
        out[5..7].copy_from_slice(&self.max_stack_depth.to_be_bytes());
        out[7..9].copy_from_slice(&self.max_output_bits.to_be_bytes());
        for (i, &g) in self.opcode_gas.iter().enumerate() {
            out[9 + 2 * i..11 + 2 * i].copy_from_slice(&g.to_be_bytes());
        }
        out[25..27].copy_from_slice(&self.gas_per_output_bit.to_be_bytes());
        Ok(out)
    }

    /// Decodes and validates a 27-byte canonical `VmSpecD1`.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, VmD1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(VmD1Error::InvalidWire("wrong VmSpecD1 wire length"));
        }
        let flags = bytes[4];
        if (flags & !0x0f) != 0 {
            return Err(VmD1Error::InvalidWire("reserved VmSpecD1 flag bits set"));
        }
        let mut opcode_gas = [0u16; 8];
        for (i, slot) in opcode_gas.iter_mut().enumerate() {
            *slot = u16::from_be_bytes([bytes[9 + 2 * i], bytes[10 + 2 * i]]);
        }
        let spec = Self {
            spec_version: u16::from_be_bytes([bytes[0], bytes[1]]),
            grammar_version: u16::from_be_bytes([bytes[2], bytes[3]]),
            zero_randomness: (flags & 1) != 0,
            camera_renderer_independent: (flags & 2) != 0,
            checked_i64_arithmetic: (flags & 4) != 0,
            atomic_rollback_on_trap: (flags & 8) != 0,
            max_stack_depth: u16::from_be_bytes([bytes[5], bytes[6]]),
            max_output_bits: u16::from_be_bytes([bytes[7], bytes[8]]),
            opcode_gas,
            gas_per_output_bit: u16::from_be_bytes([bytes[25], bytes[26]]),
        };
        spec.validate()?;
        Ok(spec)
    }

    /// Computes the canonical `Policy` `ContentTag` for this VM specification.
    pub fn tag(&self) -> Result<ContentTag, VmD1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            VM_D1_SPEC_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

/// Explicit deterministic trap classifications for `VM-D1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VmTrapV1 {
    /// Stack lacked enough operands for `Add`, `Sub`, `Dup`, `Swap`, or output emission.
    StackUnderflow,
    /// Push or `Dup` would exceed `max_stack_depth`.
    StackOverflow,
    /// Checked `i64` `Add` or `Sub` overflowed.
    ArithmeticOverflow,
    /// Metered work exceeded the caller-supplied `gas_limit`.
    GasExhausted,
    /// Instruction stream finished without reaching `SpatialOpcodeV1::Halt`.
    MissingHalt,
    /// Output voxel write targeted bedrock `(0,0,0)`, a parsed program frame, or `i64` overflow.
    ForbiddenOutputTarget(Coord),
    /// Output voxel write collided with a coordinate already claimed by an earlier program in the tick.
    OutputWriteConflict(Coord),
}

impl VmTrapV1 {
    /// Returns the canonical 1-byte wire status code (`1..=7`; `0` is reserved for `Halted`).
    pub const fn status_code(self) -> u8 {
        match self {
            Self::StackUnderflow => 1,
            Self::StackOverflow => 2,
            Self::ArithmeticOverflow => 3,
            Self::GasExhausted => 4,
            Self::MissingHalt => 5,
            Self::ForbiddenOutputTarget(_) => 6,
            Self::OutputWriteConflict(_) => 7,
        }
    }
}

/// Optional deterministic voxel output register mapped from the halted top-of-stack integer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VmOutputRegisterV1 {
    /// World-space coordinate for bit `0` (LSB); bit `k` maps to `origin + (k, 0, 0)`.
    pub origin: Coord,
    /// Number of low bits (`1..=64`) emitted as explicit `On`/`Off` edits.
    pub bit_width: u8,
}

/// One scheduled spatial program invocation anchored at `anchor`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VmInvocationV1 {
    /// Anchor coordinate of a valid `LANGUAGE-R1` framed program inside the current `World`.
    pub anchor: Coord,
    /// Optional explicit voxel output register (`None` = pure integer evaluation with zero writes).
    pub output_register: Option<VmOutputRegisterV1>,
}

/// Deterministic execution outcome for one invocation inside a [`VmBatchReceiptV1`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct VmInvocationOutcomeV1 {
    /// World anchor of the executed program.
    pub anchor: Coord,
    /// Translation-invariant `ParsedSpatialProgramV1::source_tag()` digest.
    pub source_digest: [u8; 32],
    /// `Ok(halted_top_or_zero)` when cleanly halted, or `Err(trap)` with zero partial writes.
    pub outcome: Result<i64, VmTrapV1>,
    /// Exact metered work units charged to this invocation.
    pub gas_used: u64,
    /// Number of explicit `On`/`Off` voxel writes committed to `World`/`History` (`0` on trap).
    pub emitted_writes: u16,
}

impl VmInvocationOutcomeV1 {
    /// Fixed canonical wire byte length per outcome entry (`75` bytes).
    pub const WIRE_LEN: usize = 24 + 32 + 1 + 8 + 8 + 2;

    /// Returns the canonical status byte (`0 = Halted`, `1..=7 = VmTrapV1`).
    pub const fn status_code(&self) -> u8 {
        match self.outcome {
            Ok(_) => 0,
            Err(trap) => trap.status_code(),
        }
    }

    /// Returns the halted two's-complement bit pattern (`0` on trap).
    pub const fn result_bits(&self) -> u64 {
        match self.outcome {
            Ok(v) => v as u64,
            Err(_) => 0,
        }
    }
}

/// Canonical deterministic batch execution receipt (`bitblock/vm-d1/model-step-receipt/v1`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct VmBatchReceiptV1 {
    /// Digest of the governing [`VmSpecD1`].
    pub spec_digest: [u8; 32],
    /// `world_tag` digest immediately before the tick batch.
    pub pre_world_digest: [u8; 32],
    /// `world_tag` digest immediately after committing all non-trapping explicit writes.
    pub post_world_digest: [u8; 32],
    /// Sum of `gas_used` across all ordered invocations.
    pub total_gas_used: u64,
    /// Per-invocation outcomes strictly sorted by ascending `anchor`.
    pub outcomes: Vec<VmInvocationOutcomeV1>,
}

impl VmBatchReceiptV1 {
    /// Encodes the receipt into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Vec<u8> {
        let mut out =
            Vec::with_capacity(96 + 8 + 4 + self.outcomes.len() * VmInvocationOutcomeV1::WIRE_LEN);
        out.extend_from_slice(&self.spec_digest);
        out.extend_from_slice(&self.pre_world_digest);
        out.extend_from_slice(&self.post_world_digest);
        out.extend_from_slice(&self.total_gas_used.to_be_bytes());
        out.extend_from_slice(&(self.outcomes.len() as u32).to_be_bytes());
        for entry in &self.outcomes {
            out.extend_from_slice(&entry.anchor.x.to_be_bytes());
            out.extend_from_slice(&entry.anchor.y.to_be_bytes());
            out.extend_from_slice(&entry.anchor.z.to_be_bytes());
            out.extend_from_slice(&entry.source_digest);
            out.push(entry.status_code());
            out.extend_from_slice(&entry.gas_used.to_be_bytes());
            out.extend_from_slice(&entry.result_bits().to_be_bytes());
            out.extend_from_slice(&entry.emitted_writes.to_be_bytes());
        }
        out
    }

    /// Computes the canonical `Receipt` `ContentTag` for this batch execution receipt.
    pub fn tag(&self) -> ContentTag {
        let bytes = self.encode_canonical();
        hash_canonical(
            TagKind::Receipt,
            VM_D1_MODEL_RECEIPT_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and bounded length")
    }
}

/// Evaluates one parsed `LANGUAGE-R1` program under `spec` and `gas_limit`, returning
/// `(Result<stack, trap>, gas_used)`. Pure function: zero world, camera, or randomness access.
pub fn evaluate_program_stack(
    spec: &VmSpecD1,
    program: &ParsedSpatialProgramV1,
    gas_limit: u64,
) -> Result<(Result<Vec<i64>, VmTrapV1>, u64), VmD1Error> {
    spec.validate()?;
    let mut stack: Vec<i64> = Vec::new();
    let mut gas_used = 0u64;
    for &op in &program.instructions {
        let step_gas = spec.gas_for_opcode(op);
        let next_gas = gas_used.saturating_add(step_gas);
        if next_gas > gas_limit {
            return Ok((Err(VmTrapV1::GasExhausted), gas_used));
        }
        gas_used = next_gas;
        match op {
            SpatialOpcodeV1::Nop => {}
            SpatialOpcodeV1::PushOne => {
                if stack.len() >= usize::from(spec.max_stack_depth) {
                    return Ok((Err(VmTrapV1::StackOverflow), gas_used));
                }
                stack.push(1);
            }
            SpatialOpcodeV1::PushZero => {
                if stack.len() >= usize::from(spec.max_stack_depth) {
                    return Ok((Err(VmTrapV1::StackOverflow), gas_used));
                }
                stack.push(0);
            }
            SpatialOpcodeV1::Add => {
                let (Some(b), Some(a)) = (stack.pop(), stack.pop()) else {
                    return Ok((Err(VmTrapV1::StackUnderflow), gas_used));
                };
                match a.checked_add(b) {
                    Some(sum) => stack.push(sum),
                    None => return Ok((Err(VmTrapV1::ArithmeticOverflow), gas_used)),
                }
            }
            SpatialOpcodeV1::Sub => {
                let (Some(b), Some(a)) = (stack.pop(), stack.pop()) else {
                    return Ok((Err(VmTrapV1::StackUnderflow), gas_used));
                };
                match a.checked_sub(b) {
                    Some(diff) => stack.push(diff),
                    None => return Ok((Err(VmTrapV1::ArithmeticOverflow), gas_used)),
                }
            }
            SpatialOpcodeV1::Dup => {
                let Some(&top) = stack.last() else {
                    return Ok((Err(VmTrapV1::StackUnderflow), gas_used));
                };
                if stack.len() >= usize::from(spec.max_stack_depth) {
                    return Ok((Err(VmTrapV1::StackOverflow), gas_used));
                }
                stack.push(top);
            }
            SpatialOpcodeV1::Swap => {
                let len = stack.len();
                if len < 2 {
                    return Ok((Err(VmTrapV1::StackUnderflow), gas_used));
                }
                stack.swap(len - 1, len - 2);
            }
            SpatialOpcodeV1::Halt => {
                return Ok((Ok(stack), gas_used));
            }
        }
    }
    Ok((Err(VmTrapV1::MissingHalt), gas_used))
}

fn coord_in_program_frame(c: Coord, prog: &ParsedSpatialProgramV1) -> bool {
    let a = prog.anchor;
    let max_x = a.x.saturating_add(i64::from(prog.frame_x) - 1);
    let max_y = a.y.saturating_add(i64::from(SPATIAL_FRAME_Y) - 1);
    let max_z = a.z.saturating_add(i64::from(SPATIAL_FRAME_Z) - 1);
    c.x >= a.x && c.x <= max_x && c.y >= a.y && c.y <= max_y && c.z >= a.z && c.z <= max_z
}

/// Schedules `invocations` in canonical ascending `anchor` order, parses all spatial programs from
/// `world`, enforces disjoint-write & forbidden-target guards with fail-closed atomic rollback on
/// trap, commits accepted explicit `On`/`Off` writes atomically through `History`, and returns
/// [`VmBatchReceiptV1`].
pub fn schedule_and_evaluate_model(
    world: &mut World,
    history: &mut History,
    spec: &VmSpecD1,
    invocations: &[VmInvocationV1],
    gas_limit_per_program: u64,
) -> Result<VmBatchReceiptV1, VmD1Error> {
    let spec_digest = spec.tag()?.digest();
    let pre_world_digest = world_tag(world).digest();

    let mut ordered = invocations.to_vec();
    ordered.sort_by_key(|inv| inv.anchor);
    for pair in ordered.windows(2) {
        if pair[0].anchor == pair[1].anchor {
            return Err(VmD1Error::DuplicateInvocationAnchor(pair[0].anchor));
        }
    }
    for inv in &ordered {
        if let Some(reg) = inv.output_register
            && (reg.bit_width == 0 || u16::from(reg.bit_width) > spec.max_output_bits)
        {
            return Err(VmD1Error::InvalidOutputWidth(reg.bit_width));
        }
    }

    let anchors: Vec<Coord> = ordered.iter().map(|i| i.anchor).collect();
    let (parsed_programs, _) = parse_world_programs(world, &anchors)?;

    let mut claimed_outputs: BTreeSet<Coord> = BTreeSet::new();
    let mut staged_edits: Vec<Edit> = Vec::new();
    let mut outcomes: Vec<VmInvocationOutcomeV1> = Vec::with_capacity(ordered.len());
    let mut total_gas_used = 0u64;

    for (inv, prog) in ordered.iter().zip(parsed_programs.iter()) {
        let source_digest = prog.source_tag()?.digest();
        let (stack_res, mut gas_used) = evaluate_program_stack(spec, prog, gas_limit_per_program)?;
        let (outcome, emitted_writes) = match stack_res {
            Err(trap) => (Err(trap), 0u16),
            Ok(stack) => match inv.output_register {
                None => (Ok(stack.last().copied().unwrap_or(0)), 0u16),
                Some(reg) => {
                    let Some(&top) = stack.last() else {
                        total_gas_used = total_gas_used.saturating_add(gas_used);
                        outcomes.push(VmInvocationOutcomeV1 {
                            anchor: inv.anchor,
                            source_digest,
                            outcome: Err(VmTrapV1::StackUnderflow),
                            gas_used,
                            emitted_writes: 0,
                        });
                        continue;
                    };
                    let write_gas =
                        u64::from(reg.bit_width).saturating_mul(u64::from(spec.gas_per_output_bit));
                    if gas_used.saturating_add(write_gas) > gas_limit_per_program {
                        total_gas_used = total_gas_used.saturating_add(gas_used);
                        outcomes.push(VmInvocationOutcomeV1 {
                            anchor: inv.anchor,
                            source_digest,
                            outcome: Err(VmTrapV1::GasExhausted),
                            gas_used,
                            emitted_writes: 0,
                        });
                        continue;
                    }
                    gas_used = gas_used.saturating_add(write_gas);
                    let bits = top as u64;
                    let mut local_edits: Vec<Edit> = Vec::with_capacity(usize::from(reg.bit_width));
                    let mut local_coords: Vec<Coord> =
                        Vec::with_capacity(usize::from(reg.bit_width));
                    let mut trap: Option<VmTrapV1> = None;
                    for bit_idx in 0..reg.bit_width {
                        let Some(x) = reg.origin.x.checked_add(i64::from(bit_idx)) else {
                            trap = Some(VmTrapV1::ForbiddenOutputTarget(reg.origin));
                            break;
                        };
                        let target = Coord::new(x, reg.origin.y, reg.origin.z);
                        if target == BEDROCK_COORD
                            || parsed_programs
                                .iter()
                                .any(|p| coord_in_program_frame(target, p))
                        {
                            trap = Some(VmTrapV1::ForbiddenOutputTarget(target));
                            break;
                        }
                        if claimed_outputs.contains(&target) {
                            trap = Some(VmTrapV1::OutputWriteConflict(target));
                            break;
                        }
                        let op = if ((bits >> bit_idx) & 1) == 1 {
                            Operation::On
                        } else {
                            Operation::Off
                        };
                        local_coords.push(target);
                        local_edits.push(Edit {
                            operation: op,
                            coord: target,
                        });
                    }
                    if let Some(t) = trap {
                        (Err(t), 0u16)
                    } else {
                        for c in local_coords {
                            claimed_outputs.insert(c);
                        }
                        let count = local_edits.len() as u16;
                        staged_edits.extend(local_edits);
                        (Ok(top), count)
                    }
                }
            },
        };
        total_gas_used = total_gas_used.saturating_add(gas_used);
        outcomes.push(VmInvocationOutcomeV1 {
            anchor: inv.anchor,
            source_digest,
            outcome,
            gas_used,
            emitted_writes,
        });
    }

    if !staged_edits.is_empty() {
        let next_tick = history
            .receipts()
            .last()
            .map_or(0u64, |r| r.tick())
            .saturating_add(1);
        let mut tx = history.begin_tick(next_tick);
        for edit in staged_edits {
            let _ = tx.apply_edit(history, world, edit);
        }
        let _ = history.commit_tick(tx, world);
    }

    let post_world_digest = world_tag(world).digest();
    Ok(VmBatchReceiptV1 {
        spec_digest,
        pre_world_digest,
        post_world_digest,
        total_gas_used,
        outcomes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language_r1::encode_program_to_world;

    #[test]
    fn vm_d1_spec_eval_atomic_output_and_traps() {
        let spec = VmSpecD1::v1_default();
        let raw = spec.encode_canonical().unwrap();
        assert_eq!(VmSpecD1::decode_canonical(&raw).unwrap(), spec);

        let anchor = Coord::new(3, 0, 2);
        let prog = ParsedSpatialProgramV1::new(
            3,
            anchor,
            vec![
                SpatialOpcodeV1::PushOne,
                SpatialOpcodeV1::PushOne,
                SpatialOpcodeV1::Add,
                SpatialOpcodeV1::Dup,
                SpatialOpcodeV1::Add,
                SpatialOpcodeV1::Halt,
            ],
        )
        .unwrap();
        // Exact pre-execution upper bound: 2+2+3+2+3+1 + 4*10 = 53
        assert_eq!(spec.pre_execution_work_upper_bound(&prog, 4).unwrap(), 53);

        let mut world = World::new();
        encode_program_to_world(&mut world, &prog).unwrap();
        let mut history = History::new(&world);
        let out_origin = Coord::new(5, 8, 2);
        let receipt = schedule_and_evaluate_model(
            &mut world,
            &mut history,
            &spec,
            &[VmInvocationV1 {
                anchor,
                output_register: Some(VmOutputRegisterV1 {
                    origin: out_origin,
                    bit_width: 4,
                }),
            }],
            100,
        )
        .unwrap();
        assert_eq!(receipt.outcomes[0].outcome, Ok(4));
        assert_eq!(receipt.total_gas_used, 53);
        assert!(!world.is_on(Coord::new(5, 8, 2)));
        assert!(!world.is_on(Coord::new(6, 8, 2)));
        assert!(world.is_on(Coord::new(7, 8, 2)));
        assert!(!world.is_on(Coord::new(8, 8, 2)));
    }
}
