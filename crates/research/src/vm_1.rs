//! `VM-1`: production bounded local deterministic spatial interpreter over `VM-D1` and
//! `LocalVmBoundsV1` (`PS-047`).
//!
//! Executes framed `LANGUAGE-R1` spatial programs directly inside a `World` under strict
//! `BOUNDS-1` (`CE-05`) preflight ceilings, emitting step-indexed deterministic traces
//! ([`VmStepTraceV1`]) and canonical tick execution receipts ([`VmTickExecutionReceiptV1`]):
//! - **Fail-closed preflight before execution:** enforces `LocalVmBoundsV1::preflight_vm_batch`
//!   (program count `<= 64`, instructions/program `<= 12`, per-program gas `<= 2,048`, tick gas
//!   `<= 65,536`, stack depth `<= 64`, output writes `<= 1,024`) before executing any opcode.
//! - **Exact `VM-D1` semantics + step traces:** zero randomness, zero camera/renderer input,
//!   checked `i64` arithmetic, per-instruction `VmTraceStepV1` recording, and caller-tightenable
//!   per-invocation `gas_limit_override` (never widening past `LocalVmBoundsV1`).
//! - **Atomic per-invocation rollback + `History` commit parity:** any of the 7 typed `VmTrapV1`
//!   traps rolls back that invocation's pending writes to zero while accounting metered gas and
//!   committing independent non-conflicting programs through `History` (`ACCOUNT-1`).
//!
//! Local deterministic execution does not grant shared consensus authority (`PROGRAM-STATE-1` and
//! `PROGRAM-CONSENSUS` remain locked).

use std::collections::BTreeSet;
use std::fmt;

use crate::bounds_1::{Bounds1Error, BoundsViolationV1, LocalVmBoundsV1};
use crate::history::{History, world_tag};
use crate::language_r1::{
    ParsedSpatialProgramV1, SPATIAL_FRAME_Y, SPATIAL_FRAME_Z, SpatialOpcodeV1, SpatialParseError,
    parse_world_programs,
};
use crate::program::{Edit, Operation};
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::vm_d1::{VmD1Error, VmOutputRegisterV1, VmSpecD1, VmTrapV1};
use crate::world::{BEDROCK_COORD, Coord, World};

/// Canonical schema string for a step-indexed [`VmStepTraceV1`] execution trace (`VM-1`).
pub const VM_1_STEP_TRACE_SCHEMA: &[u8] = b"bitblock/vm-1/step-trace/v1";
/// Canonical schema string for a [`VmTickExecutionReceiptV1`] tick execution receipt (`VM-1`).
pub const VM_1_TICK_RECEIPT_SCHEMA: &[u8] = b"bitblock/vm-1/tick-execution-receipt/v1";

/// Errors returned by `VM-1` configuration, bounds preflight, parsing, or batch scheduling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Vm1Error {
    /// Underlying `VM-D1` spec or scheduling invariant failed.
    Spec(VmD1Error),
    /// Underlying `BOUNDS-1` `LocalVmBoundsV1` validation failed.
    Bounds(Bounds1Error),
    /// `LocalVmBoundsV1` preflight rejected an over-bound batch before execution.
    BoundsPreflight(BoundsViolationV1),
    /// Spatial grammar parsing failed fail-closed before execution.
    Parse(SpatialParseError),
    /// `LocalVmBoundsV1::vm_spec_digest` did not match the governing `VmSpecD1`.
    SpecBoundsDigestMismatch,
    /// Wire buffer had wrong length, invalid opcode/status, or trailing bytes.
    InvalidWire(&'static str),
}

impl From<VmD1Error> for Vm1Error {
    fn from(value: VmD1Error) -> Self {
        Self::Spec(value)
    }
}

impl From<Bounds1Error> for Vm1Error {
    fn from(value: Bounds1Error) -> Self {
        Self::Bounds(value)
    }
}

impl From<SpatialParseError> for Vm1Error {
    fn from(value: SpatialParseError) -> Self {
        Self::Parse(value)
    }
}

impl fmt::Display for Vm1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Vm1Error {}

/// One step record inside [`VmStepTraceV1`], pinning per-instruction state evolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VmTraceStepV1 {
    /// Zero-based program counter of the executed instruction.
    pub pc: u16,
    /// Canonical `SpatialOpcodeV1::code()` (`0..=7`).
    pub opcode_code: u8,
    /// Post-step status (`0` = Running/Halted, `1..=7` = `VmTrapV1::status_code()`).
    pub status_after: u8,
    /// Cumulative metered gas after attempting this instruction.
    pub gas_after: u64,
    /// Stack height immediately after this instruction.
    pub stack_depth_after: u16,
    /// Two's-complement bit pattern of stack top after this instruction (`0` if empty).
    pub top_after_bits: u64,
}

impl VmTraceStepV1 {
    /// Fixed canonical big-endian wire length per step (`22` bytes).
    pub const WIRE_LEN: usize = 2 + 1 + 1 + 8 + 2 + 8;
}

/// Canonical step-indexed deterministic execution trace (`bitblock/vm-1/step-trace/v1`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct VmStepTraceV1 {
    /// Digest of the governing [`VmSpecD1`].
    pub spec_digest: [u8; 32],
    /// Translation-invariant `ParsedSpatialProgramV1::source_tag()` digest.
    pub source_digest: [u8; 32],
    /// Effective per-program gas ceiling enforced for this invocation.
    pub gas_limit: u64,
    /// Final status (`0` = Halted, `1..=7` = `VmTrapV1::status_code()`).
    pub final_status_code: u8,
    /// Exact total metered gas consumed (including output-register write gas if committed).
    pub final_gas_used: u64,
    /// Halted top-of-stack two's-complement bits (`0` on empty stack or trap).
    pub final_top_bits: u64,
    /// Ordered per-instruction trace records.
    pub steps: Vec<VmTraceStepV1>,
}

impl VmStepTraceV1 {
    /// Encodes the step trace into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(91 + self.steps.len() * VmTraceStepV1::WIRE_LEN);
        out.extend_from_slice(&self.spec_digest);
        out.extend_from_slice(&self.source_digest);
        out.extend_from_slice(&self.gas_limit.to_be_bytes());
        out.push(self.final_status_code);
        out.extend_from_slice(&self.final_gas_used.to_be_bytes());
        out.extend_from_slice(&self.final_top_bits.to_be_bytes());
        out.extend_from_slice(&(self.steps.len() as u16).to_be_bytes());
        for s in &self.steps {
            out.extend_from_slice(&s.pc.to_be_bytes());
            out.push(s.opcode_code);
            out.push(s.status_after);
            out.extend_from_slice(&s.gas_after.to_be_bytes());
            out.extend_from_slice(&s.stack_depth_after.to_be_bytes());
            out.extend_from_slice(&s.top_after_bits.to_be_bytes());
        }
        out
    }

    /// Decodes a canonical [`VmStepTraceV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Vm1Error> {
        if bytes.len() < 91 {
            return Err(Vm1Error::InvalidWire("truncated VmStepTraceV1 header"));
        }
        let step_count = u16::from_be_bytes([bytes[89], bytes[90]]) as usize;
        let expected = 91 + step_count * VmTraceStepV1::WIRE_LEN;
        if bytes.len() != expected {
            return Err(Vm1Error::InvalidWire("VmStepTraceV1 length mismatch"));
        }
        let mut steps = Vec::with_capacity(step_count);
        for i in 0..step_count {
            let o = 91 + i * VmTraceStepV1::WIRE_LEN;
            steps.push(VmTraceStepV1 {
                pc: u16::from_be_bytes([bytes[o], bytes[o + 1]]),
                opcode_code: bytes[o + 2],
                status_after: bytes[o + 3],
                gas_after: u64::from_be_bytes(bytes[o + 4..o + 12].try_into().expect("8")),
                stack_depth_after: u16::from_be_bytes([bytes[o + 12], bytes[o + 13]]),
                top_after_bits: u64::from_be_bytes(bytes[o + 14..o + 22].try_into().expect("8")),
            });
        }
        Ok(Self {
            spec_digest: bytes[0..32].try_into().expect("32"),
            source_digest: bytes[32..64].try_into().expect("32"),
            gas_limit: u64::from_be_bytes(bytes[64..72].try_into().expect("8")),
            final_status_code: bytes[72],
            final_gas_used: u64::from_be_bytes(bytes[73..81].try_into().expect("8")),
            final_top_bits: u64::from_be_bytes(bytes[81..89].try_into().expect("8")),
            steps,
        })
    }

    /// Computes the canonical `Receipt` `ContentTag` for this step trace.
    pub fn tag(&self) -> ContentTag {
        let bytes = self.encode_canonical();
        hash_canonical(
            TagKind::Receipt,
            VM_1_STEP_TRACE_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and bounded length")
    }
}

/// One scheduled program invocation submitted to [`LocalSpatialVmV1::execute_tick`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VmProgramRequestV1 {
    /// Anchor coordinate of a framed `LANGUAGE-R1` program in the current `World`.
    pub anchor: Coord,
    /// Optional explicit voxel output register (`None` = pure integer evaluation).
    pub output_register: Option<VmOutputRegisterV1>,
    /// Optional tightened per-invocation gas ceiling (clamped to `<= LocalVmBoundsV1::max_gas_per_program`).
    pub gas_limit_override: Option<u64>,
}

/// Per-program execution outcome recorded inside [`VmTickExecutionReceiptV1`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct VmProgramExecutionEntryV1 {
    /// Program anchor coordinate.
    pub anchor: Coord,
    /// Translation-invariant `ParsedSpatialProgramV1::source_tag()` digest.
    pub source_digest: [u8; 32],
    /// Canonical [`VmStepTraceV1::tag`] digest.
    pub trace_digest: [u8; 32],
    /// `Ok(halted_top_or_zero)` on clean halt or `Err(trap)` with atomic rollback.
    pub outcome: Result<i64, VmTrapV1>,
    /// Total metered gas billed to this invocation.
    pub gas_used: u64,
    /// Number of explicit `On`/`Off` voxel writes committed (`0` on trap).
    pub emitted_writes: u16,
}

impl VmProgramExecutionEntryV1 {
    /// Fixed canonical wire length per tick entry (`107` bytes).
    pub const WIRE_LEN: usize = 24 + 32 + 32 + 1 + 8 + 8 + 2;

    /// Returns the halted two's-complement bit pattern (`0` on trap).
    pub const fn result_bits(&self) -> u64 {
        if let Ok(v) = self.outcome {
            v as u64
        } else {
            0
        }
    }

    /// Returns the canonical status byte (`0 = Halted`, `1..=7 = VmTrapV1`).
    pub const fn status_code(&self) -> u8 {
        if let Err(t) = self.outcome {
            t.status_code()
        } else {
            0
        }
    }
}

/// Canonical per-tick `VM-1` execution receipt (`bitblock/vm-1/tick-execution-receipt/v1`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct VmTickExecutionReceiptV1 {
    /// Digest of the governing [`VmSpecD1`].
    pub spec_digest: [u8; 32],
    /// Digest of the governing [`LocalVmBoundsV1`] (`CE-05`).
    pub local_vm_bounds_digest: [u8; 32],
    /// `world_tag` digest before executing the tick.
    pub pre_world_digest: [u8; 32],
    /// `world_tag` digest after committing all non-trapping explicit writes.
    pub post_world_digest: [u8; 32],
    /// Aggregate metered gas across all ordered invocations in the tick.
    pub tick_gas_used: u64,
    /// Total explicit `On`/`Off` voxel writes committed in the tick.
    pub tick_emitted_writes: u16,
    /// Per-program execution entries strictly sorted by ascending `anchor`.
    pub entries: Vec<VmProgramExecutionEntryV1>,
}

impl VmTickExecutionReceiptV1 {
    /// Encodes the tick execution receipt into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Vec<u8> {
        let mut out =
            Vec::with_capacity(142 + self.entries.len() * VmProgramExecutionEntryV1::WIRE_LEN);
        out.extend_from_slice(&self.spec_digest);
        out.extend_from_slice(&self.local_vm_bounds_digest);
        out.extend_from_slice(&self.pre_world_digest);
        out.extend_from_slice(&self.post_world_digest);
        out.extend_from_slice(&self.tick_gas_used.to_be_bytes());
        out.extend_from_slice(&self.tick_emitted_writes.to_be_bytes());
        out.extend_from_slice(&(self.entries.len() as u32).to_be_bytes());
        for e in &self.entries {
            out.extend_from_slice(&e.anchor.x.to_be_bytes());
            out.extend_from_slice(&e.anchor.y.to_be_bytes());
            out.extend_from_slice(&e.anchor.z.to_be_bytes());
            out.extend_from_slice(&e.source_digest);
            out.extend_from_slice(&e.trace_digest);
            out.push(e.status_code());
            out.extend_from_slice(&e.gas_used.to_be_bytes());
            out.extend_from_slice(&e.result_bits().to_be_bytes());
            out.extend_from_slice(&e.emitted_writes.to_be_bytes());
        }
        out
    }

    /// Computes the canonical `Receipt` `ContentTag` for this tick receipt.
    pub fn tag(&self) -> ContentTag {
        let bytes = self.encode_canonical();
        hash_canonical(
            TagKind::Receipt,
            VM_1_TICK_RECEIPT_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and bounded length")
    }
}

/// Executes one single `SpatialOpcodeV1` step against `stack`, returning `Ok(true)` on `Halt`,
/// `Ok(false)` on continue, or `Err(trap)` fail-closed. Mirrors Lean 4 `VmRefinement.evalStep`.
pub fn step_opcode(
    stack: &mut Vec<i64>,
    op: SpatialOpcodeV1,
    max_stack_depth: usize,
) -> Result<bool, VmTrapV1> {
    match op {
        SpatialOpcodeV1::Nop => Ok(false),
        SpatialOpcodeV1::PushOne => push_checked(stack, 1, max_stack_depth),
        SpatialOpcodeV1::PushZero => push_checked(stack, 0, max_stack_depth),
        SpatialOpcodeV1::Add => binop_checked(stack, i64::checked_add),
        SpatialOpcodeV1::Sub => binop_checked(stack, i64::checked_sub),
        SpatialOpcodeV1::Dup => {
            let top = *stack.last().ok_or(VmTrapV1::StackUnderflow)?;
            push_checked(stack, top, max_stack_depth)
        }
        SpatialOpcodeV1::Swap => {
            let n = stack.len();
            if n < 2 {
                return Err(VmTrapV1::StackUnderflow);
            }
            stack.swap(n - 1, n - 2);
            Ok(false)
        }
        SpatialOpcodeV1::Halt => Ok(true),
    }
}

fn push_checked(stack: &mut Vec<i64>, val: i64, max_depth: usize) -> Result<bool, VmTrapV1> {
    if stack.len() >= max_depth {
        return Err(VmTrapV1::StackOverflow);
    }
    stack.push(val);
    Ok(false)
}

fn binop_checked(stack: &mut Vec<i64>, op: fn(i64, i64) -> Option<i64>) -> Result<bool, VmTrapV1> {
    let n = stack.len();
    if n < 2 {
        return Err(VmTrapV1::StackUnderflow);
    }
    let res = op(stack[n - 2], stack[n - 1]).ok_or(VmTrapV1::ArithmeticOverflow)?;
    stack.truncate(n - 2);
    stack.push(res);
    Ok(false)
}

fn in_frame_box(coord: Coord, prog: &ParsedSpatialProgramV1) -> bool {
    let a = prog.anchor;
    coord.x >= a.x
        && coord.x <= a.x.saturating_add(i64::from(prog.frame_x) - 1)
        && coord.y >= a.y
        && coord.y <= a.y.saturating_add(i64::from(SPATIAL_FRAME_Y) - 1)
        && coord.z >= a.z
        && coord.z <= a.z.saturating_add(i64::from(SPATIAL_FRAME_Z) - 1)
}

/// Evaluates one parsed `LANGUAGE-R1` program under `spec`, `bounds`, and optional tightened
/// `gas_limit_override`, returning `(Result<stack, trap>, VmStepTraceV1)`.
pub fn evaluate_program_traced(
    spec: &VmSpecD1,
    bounds: &LocalVmBoundsV1,
    program: &ParsedSpatialProgramV1,
    gas_limit_override: Option<u64>,
) -> Result<(Result<Vec<i64>, VmTrapV1>, VmStepTraceV1), Vm1Error> {
    let spec_digest = spec.tag()?.digest();
    bounds.validate(Some(spec))?;
    if bounds.vm_spec_digest != spec_digest {
        return Err(Vm1Error::SpecBoundsDigestMismatch);
    }
    let source_digest = program.source_tag()?.digest();
    let eff_limit = gas_limit_override
        .unwrap_or(bounds.max_gas_per_program)
        .min(bounds.max_gas_per_program);
    let max_depth = usize::from(spec.max_stack_depth.min(bounds.max_stack_depth));

    let mut stack: Vec<i64> = Vec::new();
    let mut gas_used = 0u64;
    let mut steps = Vec::with_capacity(program.instructions.len());

    for (idx, &op) in program.instructions.iter().enumerate() {
        let next_gas = gas_used.saturating_add(spec.gas_for_opcode(op));
        if next_gas > eff_limit {
            let trap = VmTrapV1::GasExhausted;
            steps.push(VmTraceStepV1 {
                pc: idx as u16,
                opcode_code: op.code(),
                status_after: trap.status_code(),
                gas_after: gas_used,
                stack_depth_after: stack.len() as u16,
                top_after_bits: stack.last().copied().unwrap_or(0) as u64,
            });
            return Ok((
                Err(trap),
                VmStepTraceV1 {
                    spec_digest,
                    source_digest,
                    gas_limit: eff_limit,
                    final_status_code: trap.status_code(),
                    final_gas_used: gas_used,
                    final_top_bits: 0,
                    steps,
                },
            ));
        }
        gas_used = next_gas;
        match step_opcode(&mut stack, op, max_depth) {
            Ok(halted) => {
                let top_bits = stack.last().copied().unwrap_or(0) as u64;
                steps.push(VmTraceStepV1 {
                    pc: idx as u16,
                    opcode_code: op.code(),
                    status_after: 0,
                    gas_after: gas_used,
                    stack_depth_after: stack.len() as u16,
                    top_after_bits: top_bits,
                });
                if halted {
                    return Ok((
                        Ok(stack),
                        VmStepTraceV1 {
                            spec_digest,
                            source_digest,
                            gas_limit: eff_limit,
                            final_status_code: 0,
                            final_gas_used: gas_used,
                            final_top_bits: top_bits,
                            steps,
                        },
                    ));
                }
            }
            Err(trap) => {
                steps.push(VmTraceStepV1 {
                    pc: idx as u16,
                    opcode_code: op.code(),
                    status_after: trap.status_code(),
                    gas_after: gas_used,
                    stack_depth_after: stack.len() as u16,
                    top_after_bits: stack.last().copied().unwrap_or(0) as u64,
                });
                return Ok((
                    Err(trap),
                    VmStepTraceV1 {
                        spec_digest,
                        source_digest,
                        gas_limit: eff_limit,
                        final_status_code: trap.status_code(),
                        final_gas_used: gas_used,
                        final_top_bits: 0,
                        steps,
                    },
                ));
            }
        }
    }

    let trap = VmTrapV1::MissingHalt;
    Ok((
        Err(trap),
        VmStepTraceV1 {
            spec_digest,
            source_digest,
            gas_limit: eff_limit,
            final_status_code: trap.status_code(),
            final_gas_used: gas_used,
            final_top_bits: 0,
            steps,
        },
    ))
}

/// Production bounded local deterministic spatial interpreter (`VM-1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalSpatialVmV1 {
    /// Frozen `VM-D1` semantics specification.
    pub spec: VmSpecD1,
    /// Committed `BOUNDS-1` (`CE-05`) local-VM ceiling profile.
    pub bounds: LocalVmBoundsV1,
    spec_digest: [u8; 32],
    bounds_digest: [u8; 32],
}

impl LocalSpatialVmV1 {
    /// Creates a validated [`LocalSpatialVmV1`] bound to `spec` and `bounds`.
    pub fn new(spec: VmSpecD1, bounds: LocalVmBoundsV1) -> Result<Self, Vm1Error> {
        let spec_digest = spec.tag()?.digest();
        bounds.validate(Some(&spec))?;
        if bounds.vm_spec_digest != spec_digest {
            return Err(Vm1Error::SpecBoundsDigestMismatch);
        }
        let bounds_digest = bounds.tag()?.digest();
        Ok(Self {
            spec,
            bounds,
            spec_digest,
            bounds_digest,
        })
    }

    /// Creates the default v1 local spatial VM from `VmSpecD1::v1_default()` and
    /// `LocalVmBoundsV1::v1_default()`.
    pub fn v1_default() -> Result<Self, Vm1Error> {
        let spec = VmSpecD1::v1_default();
        let bounds = LocalVmBoundsV1::v1_default(&spec)?;
        Self::new(spec, bounds)
    }

    /// Executes one deterministic local VM tick over `world` and commits accepted explicit
    /// `On`/`Off` writes atomically through `history`, returning `(receipt, traces)`.
    pub fn execute_tick(
        &self,
        world: &mut World,
        history: &mut History,
        requests: &[VmProgramRequestV1],
    ) -> Result<(VmTickExecutionReceiptV1, Vec<VmStepTraceV1>), Vm1Error> {
        let pre_world_digest = world_tag(world).digest();
        let mut ordered = requests.to_vec();
        ordered.sort_by_key(|r| r.anchor);
        for pair in ordered.windows(2) {
            if pair[0].anchor == pair[1].anchor {
                return Err(Vm1Error::Spec(VmD1Error::DuplicateInvocationAnchor(
                    pair[0].anchor,
                )));
            }
        }
        let mut widths = Vec::with_capacity(ordered.len());
        for req in &ordered {
            if let Some(reg) = req.output_register {
                if reg.bit_width == 0 || u16::from(reg.bit_width) > self.spec.max_output_bits {
                    return Err(Vm1Error::Spec(VmD1Error::InvalidOutputWidth(reg.bit_width)));
                }
                widths.push(reg.bit_width);
            } else {
                widths.push(0);
            }
        }
        let anchors: Vec<Coord> = ordered.iter().map(|r| r.anchor).collect();
        let (parsed, _) = parse_world_programs(world, &anchors)?;

        // Fail-closed BOUNDS-1 (CE-05) preflight BEFORE executing any instruction.
        self.bounds
            .preflight_vm_batch(&self.spec, &parsed, &widths)
            .map_err(Vm1Error::BoundsPreflight)?;

        let mut claimed: BTreeSet<Coord> = BTreeSet::new();
        let mut staged_edits: Vec<Edit> = Vec::new();
        let mut entries = Vec::with_capacity(ordered.len());
        let mut traces = Vec::with_capacity(ordered.len());
        let mut tick_gas_used = 0u64;
        let mut tick_emitted_writes = 0u16;

        for (req, prog) in ordered.iter().zip(parsed.iter()) {
            let (stack_res, mut trace) =
                evaluate_program_traced(&self.spec, &self.bounds, prog, req.gas_limit_override)?;
            let eff_limit = trace.gas_limit;
            let (outcome, emitted_writes) = match stack_res {
                Err(trap) => (Err(trap), 0u16),
                Ok(stack) => match req.output_register {
                    None => (Ok(stack.last().copied().unwrap_or(0)), 0u16),
                    Some(reg) => {
                        if let Some(&top) = stack.last() {
                            let write_gas = u64::from(reg.bit_width)
                                .saturating_mul(u64::from(self.spec.gas_per_output_bit));
                            if trace.final_gas_used.saturating_add(write_gas) > eff_limit {
                                let trap = VmTrapV1::GasExhausted;
                                trace.final_status_code = trap.status_code();
                                trace.final_top_bits = 0;
                                (Err(trap), 0u16)
                            } else {
                                trace.final_gas_used =
                                    trace.final_gas_used.saturating_add(write_gas);
                                match stage_output_writes(top, reg, &parsed, &claimed) {
                                    Ok(edits) => {
                                        for e in &edits {
                                            claimed.insert(e.coord);
                                        }
                                        let count = edits.len() as u16;
                                        staged_edits.extend(edits);
                                        (Ok(top), count)
                                    }
                                    Err(trap) => {
                                        trace.final_status_code = trap.status_code();
                                        trace.final_top_bits = 0;
                                        (Err(trap), 0u16)
                                    }
                                }
                            }
                        } else {
                            let trap = VmTrapV1::StackUnderflow;
                            trace.final_status_code = trap.status_code();
                            trace.final_top_bits = 0;
                            (Err(trap), 0u16)
                        }
                    }
                },
            };

            let trace_digest = trace.tag().digest();
            tick_gas_used = tick_gas_used.saturating_add(trace.final_gas_used);
            tick_emitted_writes = tick_emitted_writes.saturating_add(emitted_writes);
            entries.push(VmProgramExecutionEntryV1 {
                anchor: req.anchor,
                source_digest: trace.source_digest,
                trace_digest,
                outcome,
                gas_used: trace.final_gas_used,
                emitted_writes,
            });
            traces.push(trace);
        }

        commit_staged_edits(history, world, staged_edits);

        let post_world_digest = world_tag(world).digest();
        let receipt = VmTickExecutionReceiptV1 {
            spec_digest: self.spec_digest,
            local_vm_bounds_digest: self.bounds_digest,
            pre_world_digest,
            post_world_digest,
            tick_gas_used,
            tick_emitted_writes,
            entries,
        };
        Ok((receipt, traces))
    }
}

fn commit_staged_edits(history: &mut History, world: &mut World, edits: Vec<Edit>) {
    if edits.is_empty() {
        return;
    }
    let tick_no = history
        .receipts()
        .last()
        .map_or(0u64, |r| r.tick())
        .saturating_add(1);
    let mut tx = history.begin_tick(tick_no);
    edits.into_iter().for_each(|e| {
        let _ = tx.apply_edit(history, world, e);
    });
    let _ = history.commit_tick(tx, world);
}

fn stage_output_writes(
    top: i64,
    reg: VmOutputRegisterV1,
    parsed: &[ParsedSpatialProgramV1],
    claimed: &BTreeSet<Coord>,
) -> Result<Vec<Edit>, VmTrapV1> {
    let bits = top as u64;
    let mut edits = Vec::with_capacity(usize::from(reg.bit_width));
    for bit_idx in 0..reg.bit_width {
        let x = reg
            .origin
            .x
            .checked_add(i64::from(bit_idx))
            .ok_or(VmTrapV1::ForbiddenOutputTarget(reg.origin))?;
        let target = Coord::new(x, reg.origin.y, reg.origin.z);
        if target == BEDROCK_COORD || parsed.iter().any(|p| in_frame_box(target, p)) {
            return Err(VmTrapV1::ForbiddenOutputTarget(target));
        }
        if claimed.contains(&target) {
            return Err(VmTrapV1::OutputWriteConflict(target));
        }
        let operation = if ((bits >> bit_idx) & 1) == 1 {
            Operation::On
        } else {
            Operation::Off
        };
        edits.push(Edit {
            operation,
            coord: target,
        });
    }
    Ok(edits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language_r1::encode_program_to_world;

    #[test]
    fn vm_1_traced_execution_rollback_and_history_parity() {
        let vm = LocalSpatialVmV1::v1_default().unwrap();
        let p1 = ParsedSpatialProgramV1::new(
            1,
            Coord::new(3, 0, 2),
            vec![
                SpatialOpcodeV1::PushOne,
                SpatialOpcodeV1::PushOne,
                SpatialOpcodeV1::Add,
                SpatialOpcodeV1::Dup,
                SpatialOpcodeV1::Add,
                SpatialOpcodeV1::PushOne,
                SpatialOpcodeV1::Add,
                SpatialOpcodeV1::Halt,
            ],
        )
        .unwrap();
        let mut world = World::new();
        encode_program_to_world(&mut world, &p1).unwrap();
        let mut history = History::new(&world);

        // First execute with deliberately too-tight gas override -> GasExhausted + atomic rollback!
        let (r_trap, t_trap) = vm
            .execute_tick(
                &mut world,
                &mut history,
                &[VmProgramRequestV1 {
                    anchor: Coord::new(3, 0, 2),
                    output_register: Some(VmOutputRegisterV1 {
                        origin: Coord::new(5, 8, 2),
                        bit_width: 4,
                    }),
                    gas_limit_override: Some(5),
                }],
            )
            .unwrap();
        assert_eq!(r_trap.entries[0].outcome, Err(VmTrapV1::GasExhausted));
        assert_eq!(r_trap.pre_world_digest, r_trap.post_world_digest);
        let raw_t = t_trap[0].encode_canonical();
        assert_eq!(VmStepTraceV1::decode_canonical(&raw_t).unwrap(), t_trap[0]);

        // Now execute with full budget -> value = 5 (0b0101) commits cleanly through History!
        let (r_ok, _) = vm
            .execute_tick(
                &mut world,
                &mut history,
                &[VmProgramRequestV1 {
                    anchor: Coord::new(3, 0, 2),
                    output_register: Some(VmOutputRegisterV1 {
                        origin: Coord::new(5, 8, 2),
                        bit_width: 4,
                    }),
                    gas_limit_override: None,
                }],
            )
            .unwrap();
        assert_eq!(r_ok.entries[0].outcome, Ok(5));
        assert!(world.is_on(Coord::new(5, 8, 2)));
        assert!(!world.is_on(Coord::new(6, 8, 2)));
        assert!(world.is_on(Coord::new(7, 8, 2)));
        assert_eq!(
            history.receipts().last().unwrap().post_world().digest(),
            r_ok.post_world_digest
        );
    }
}
