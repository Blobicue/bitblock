//! Local sequential-history transcript and coordinate-event coverage.
//!
//! This module defines one canonical iterated SHA-256 recurrence, strict segment bytes, bounded
//! recomputation, exact coordinate-history event coverage, and a fail-closed mode-admission
//! boundary. It does not prove elapsed time, an unavoidable sequential lower bound, external
//! custody, authorship, consensus, Proof of Stake, cryptocurrency, or Layer 1 status.

use std::fmt;

use crate::history::History;
use crate::history_store::{
    HistoryReplayLimits, HistoryReplayUsage, InMemoryHistoryVerificationError,
    verify_in_memory_history_with_limits,
};
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::world::World;

pub const SEQUENTIAL_FUNCTION_SCHEMA: &[u8] =
    b"bitblock/sequential-history/function/sha256-chain-v1";
pub const SEQUENTIAL_INPUT_SCHEMA: &[u8] = b"bitblock/sequential-history/input/v1";
pub const SEQUENTIAL_STEP_SCHEMA: &[u8] = b"bitblock/sequential-history/step/v1";
pub const SEQUENTIAL_SEGMENT_SCHEMA: &[u8] = b"bitblock/sequential-history/segment/v1";
pub const OPEN_LOCAL_MODE_SCHEMA: &[u8] = b"bitblock/history-mode/open-local/v1";
pub const OPEN_LOCAL_EVIDENCE_SCHEMA: &[u8] = b"bitblock/history-mode/open-local-evidence/v1";
pub const SEQUENTIAL_MODE_SCHEMA: &[u8] = b"bitblock/history-mode/sequential-history/v1";
pub const COORDINATE_EVIDENCE_SCHEMA: &[u8] = b"bitblock/sequential-history/coordinate-evidence/v1";

const SEGMENT_FIXED_BYTES: usize = 32 + 1 + 32 + 8 + 8 + 32 + 8 + 32;
const INJECTION_BYTES: usize = 8 + 32;
const STEP_FIXED_BYTES: usize = 32 + 8 + 32 + 8;

/// Local computation and allocation policy for sequential verification.
///
/// These limits are not part of the sequential function or protocol authority. A transcript
/// refused only by this policy can be retried under an explicitly larger bounded policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SequentialLimits {
    max_segments: usize,
    max_iterations_per_segment: u64,
    max_total_iterations: u64,
    max_injections_per_segment: usize,
    max_total_injections: usize,
}

impl SequentialLimits {
    pub const DEFAULT: Self = Self {
        max_segments: 4_096,
        max_iterations_per_segment: 1_048_576,
        max_total_iterations: 16_777_216,
        max_injections_per_segment: 65_536,
        max_total_injections: 262_144,
    };

    pub const fn new(
        max_segments: usize,
        max_iterations_per_segment: u64,
        max_total_iterations: u64,
        max_injections_per_segment: usize,
        max_total_injections: usize,
    ) -> Self {
        Self {
            max_segments,
            max_iterations_per_segment,
            max_total_iterations,
            max_injections_per_segment,
            max_total_injections,
        }
    }

    pub const fn max_segments(self) -> usize {
        self.max_segments
    }

    pub const fn max_iterations_per_segment(self) -> u64 {
        self.max_iterations_per_segment
    }

    pub const fn max_total_iterations(self) -> u64 {
        self.max_total_iterations
    }

    pub const fn max_injections_per_segment(self) -> usize {
        self.max_injections_per_segment
    }

    pub const fn max_total_injections(self) -> usize {
        self.max_total_injections
    }

    /// The fields applicable while decoding or building one segment in isolation.
    pub const fn segment_limits(self) -> SequentialSegmentLimits {
        SequentialSegmentLimits::new(
            self.max_iterations_per_segment,
            self.max_injections_per_segment,
        )
    }
}

impl Default for SequentialLimits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Local allocation and computation policy for one segment in isolation.
///
/// Whole-chain segment, iteration, and injection totals are deliberately absent: only
/// [`verify_sequential_chain`] can enforce them with the complete prefix in hand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SequentialSegmentLimits {
    max_iterations: u64,
    max_injections: usize,
}

impl SequentialSegmentLimits {
    pub const DEFAULT: Self = SequentialLimits::DEFAULT.segment_limits();

    pub const fn new(max_iterations: u64, max_injections: usize) -> Self {
        Self {
            max_iterations,
            max_injections,
        }
    }

    pub const fn max_iterations(self) -> u64 {
        self.max_iterations
    }

    pub const fn max_injections(self) -> usize {
        self.max_injections
    }
}

impl Default for SequentialSegmentLimits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// One event occurrence injected before the step at a relative iteration offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SequentialInjection {
    offset: u64,
    event: ContentTag,
}

impl SequentialInjection {
    pub fn new(offset: u64, event: ContentTag) -> Result<Self, SequentialSemanticError> {
        if event.kind() != TagKind::Event {
            return Err(SequentialSemanticError::EventKind {
                index: 0,
                actual: event.kind(),
            });
        }
        Ok(Self { offset, event })
    }

    pub const fn offset(self) -> u64 {
        self.offset
    }

    pub const fn event(self) -> ContentTag {
        self.event
    }
}

/// Canonical claimed fields of one sequential segment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SequentialSegmentPayload {
    function: ContentTag,
    parent: Option<ContentTag>,
    start_iteration: u64,
    iteration_count: u64,
    input: ContentTag,
    injections: Vec<SequentialInjection>,
    output: ContentTag,
}

impl SequentialSegmentPayload {
    pub const fn function(&self) -> ContentTag {
        self.function
    }

    pub const fn parent(&self) -> Option<ContentTag> {
        self.parent
    }

    pub const fn start_iteration(&self) -> u64 {
        self.start_iteration
    }

    pub const fn iteration_count(&self) -> u64 {
        self.iteration_count
    }

    pub const fn input(&self) -> ContentTag {
        self.input
    }

    pub fn injections(&self) -> &[SequentialInjection] {
        &self.injections
    }

    pub const fn output(&self) -> ContentTag {
        self.output
    }

    pub fn end_iteration(&self) -> u64 {
        self.start_iteration
            .checked_add(self.iteration_count)
            .expect("an admitted segment has a representable end iteration")
    }

    pub fn encoded_len(&self) -> usize {
        canonical_segment_payload_len(self.injections.len())
            .expect("an admitted segment has a representable encoded length")
    }

    pub fn encode_canonical(&self) -> Vec<u8> {
        let mut encoded = Vec::with_capacity(self.encoded_len());
        encoded.extend_from_slice(&self.function.digest());
        match self.parent {
            Some(parent) => {
                encoded.push(1);
                encoded.extend_from_slice(&parent.digest());
            }
            None => {
                encoded.push(0);
                encoded.extend_from_slice(&[0; 32]);
            }
        }
        encoded.extend_from_slice(&self.start_iteration.to_be_bytes());
        encoded.extend_from_slice(&self.iteration_count.to_be_bytes());
        encoded.extend_from_slice(&self.input.digest());
        encoded.extend_from_slice(&(self.injections.len() as u64).to_be_bytes());
        for injection in &self.injections {
            encoded.extend_from_slice(&injection.offset.to_be_bytes());
            encoded.extend_from_slice(&injection.event.digest());
        }
        encoded.extend_from_slice(&self.output.digest());
        debug_assert_eq!(encoded.len(), self.encoded_len());
        encoded
    }

    pub fn content_tag(&self) -> ContentTag {
        let encoded = self.encode_canonical();
        hash_canonical(
            TagKind::SequentialHistory,
            SEQUENTIAL_SEGMENT_SCHEMA,
            encoded.len() as u64,
            |sink| sink(&encoded),
        )
        .expect("canonical sequential segment emits its declared length")
    }

    /// Reads and validates the declared injection count and exact canonical byte length without
    /// allocating an injection vector.
    pub fn declared_injection_count(bytes: &[u8]) -> Result<usize, SequentialCodecError> {
        if bytes.len() < SEGMENT_FIXED_BYTES {
            return Err(SequentialCodecError::WrongLength {
                expected: SEGMENT_FIXED_BYTES,
                actual: bytes.len(),
            });
        }
        let count_u64 = u64::from_be_bytes(
            bytes[113..121]
                .try_into()
                .expect("fixed injection-count field"),
        );
        let count = usize::try_from(count_u64).map_err(|_| SequentialCodecError::LengthOverflow)?;
        let expected = canonical_segment_payload_len(count)?;
        if bytes.len() != expected {
            return Err(SequentialCodecError::WrongLength {
                expected,
                actual: bytes.len(),
            });
        }
        Ok(count)
    }

    fn decode_canonical_with_count(
        bytes: &[u8],
        injection_count: usize,
    ) -> Result<Self, SequentialCodecError> {
        let function = ContentTag::from_digest(
            TagKind::Policy,
            bytes[..32]
                .try_into()
                .expect("function digest has fixed length"),
        );
        let parent_digest: [u8; 32] = bytes[33..65]
            .try_into()
            .expect("parent digest has fixed length");
        let parent = match bytes[32] {
            0 => {
                if parent_digest != [0; 32] {
                    return Err(SequentialCodecError::NonzeroNilParent);
                }
                None
            }
            1 => Some(ContentTag::from_digest(
                TagKind::SequentialHistory,
                parent_digest,
            )),
            actual => return Err(SequentialCodecError::ParentFlag { actual }),
        };
        let start_iteration = u64::from_be_bytes(
            bytes[65..73]
                .try_into()
                .expect("start iteration has fixed length"),
        );
        let iteration_count = u64::from_be_bytes(
            bytes[73..81]
                .try_into()
                .expect("iteration count has fixed length"),
        );
        if iteration_count == 0 {
            return Err(SequentialCodecError::ZeroIterations);
        }
        start_iteration
            .checked_add(iteration_count)
            .ok_or(SequentialCodecError::IterationOverflow)?;
        let input = ContentTag::from_digest(
            TagKind::SequentialHistory,
            bytes[81..113]
                .try_into()
                .expect("input digest has fixed length"),
        );
        let mut cursor = 121;
        let mut injections = Vec::new();
        injections.try_reserve_exact(injection_count).map_err(|_| {
            SequentialCodecError::AllocationFailed {
                requested: injection_count,
            }
        })?;
        let mut previous_offset = None;
        for index in 0..injection_count {
            let offset = u64::from_be_bytes(
                bytes[cursor..cursor + 8]
                    .try_into()
                    .expect("injection offset has fixed length"),
            );
            if offset >= iteration_count {
                return Err(SequentialCodecError::InjectionOffsetOutOfRange {
                    index,
                    offset,
                    iteration_count,
                });
            }
            if let Some(previous) = previous_offset
                && offset < previous
            {
                return Err(SequentialCodecError::InjectionOrder {
                    index,
                    previous,
                    actual: offset,
                });
            }
            let event = ContentTag::from_digest(
                TagKind::Event,
                bytes[cursor + 8..cursor + INJECTION_BYTES]
                    .try_into()
                    .expect("event digest has fixed length"),
            );
            injections.push(SequentialInjection { offset, event });
            previous_offset = Some(offset);
            cursor += INJECTION_BYTES;
        }
        let output = ContentTag::from_digest(
            TagKind::SequentialHistory,
            bytes[cursor..cursor + 32]
                .try_into()
                .expect("output digest has fixed length"),
        );
        let payload = Self {
            function,
            parent,
            start_iteration,
            iteration_count,
            input,
            injections,
            output,
        };
        if payload.encode_canonical() != bytes {
            return Err(SequentialCodecError::NonCanonicalEncoding);
        }
        Ok(payload)
    }
}

/// One canonical but not necessarily semantically valid segment claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SequentialSegment {
    tag: ContentTag,
    payload: SequentialSegmentPayload,
}

impl SequentialSegment {
    pub fn first(
        input: ContentTag,
        iteration_count: u64,
        injections: Vec<SequentialInjection>,
        limits: SequentialSegmentLimits,
    ) -> Result<Self, SequentialError> {
        build_segment(None, 0, input, iteration_count, injections, limits, 0)
    }

    pub fn next(
        &self,
        iteration_count: u64,
        injections: Vec<SequentialInjection>,
        limits: SequentialSegmentLimits,
    ) -> Result<Self, SequentialError> {
        build_segment(
            Some(self.tag),
            self.payload.end_iteration(),
            self.payload.output,
            iteration_count,
            injections,
            limits,
            0,
        )
    }

    pub fn decode_canonical(
        bytes: &[u8],
        limits: SequentialSegmentLimits,
    ) -> Result<Self, SequentialError> {
        let injection_count = SequentialSegmentPayload::declared_injection_count(bytes)?;
        check_injection_limit(injection_count, limits, 0)?;
        let start_iteration = u64::from_be_bytes(
            bytes[65..73]
                .try_into()
                .expect("validated segment has a fixed start field"),
        );
        let iteration_count = u64::from_be_bytes(
            bytes[73..81]
                .try_into()
                .expect("validated segment has a fixed iteration-count field"),
        );
        if iteration_count == 0 {
            return Err(SequentialCodecError::ZeroIterations.into());
        }
        start_iteration
            .checked_add(iteration_count)
            .ok_or(SequentialCodecError::IterationOverflow)?;
        check_iteration_limit(iteration_count, limits, 0)?;
        let payload =
            SequentialSegmentPayload::decode_canonical_with_count(bytes, injection_count)?;
        check_segment_shape(&payload)?;
        Ok(Self::from_payload(payload))
    }

    fn from_payload(payload: SequentialSegmentPayload) -> Self {
        let tag = payload.content_tag();
        Self { tag, payload }
    }

    pub const fn tag(&self) -> ContentTag {
        self.tag
    }

    pub const fn payload(&self) -> &SequentialSegmentPayload {
        &self.payload
    }

    pub fn encode_canonical(&self) -> Vec<u8> {
        self.payload.encode_canonical()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SequentialUsage {
    pub segment_count: usize,
    pub total_iterations: u64,
    pub total_injections: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SequentialVerification {
    pub function: ContentTag,
    pub head: ContentTag,
    pub output: ContentTag,
    pub usage: SequentialUsage,
}

pub fn sequential_function_tag() -> ContentTag {
    hash_canonical(TagKind::Policy, SEQUENTIAL_FUNCTION_SCHEMA, 0, |_| {})
        .expect("sequential function policy has an empty payload")
}

/// Binds an initial sequential state to the full kind and digest of a source object.
pub fn sequential_initial_state(source: ContentTag) -> ContentTag {
    let kind = source.kind().name().as_bytes();
    let kind_len = u16::try_from(kind.len()).expect("closed tag kind exceeds u16");
    let payload_len = 2 + kind.len() + 32;
    hash_canonical(
        TagKind::SequentialHistory,
        SEQUENTIAL_INPUT_SCHEMA,
        payload_len as u64,
        |sink| {
            sink(&kind_len.to_be_bytes());
            sink(kind);
            sink(&source.digest());
        },
    )
    .expect("sequential input encoding emits its measured length")
}

pub fn open_local_mode_tag() -> ContentTag {
    hash_canonical(TagKind::Policy, OPEN_LOCAL_MODE_SCHEMA, 0, |_| {})
        .expect("open-local mode has an empty payload")
}

fn open_local_evidence_tag(history: &History, mode: ContentTag) -> ContentTag {
    open_local_evidence_tag_from_fields(
        mode,
        history.genesis(),
        history.head(),
        history.current_world(),
    )
}

fn open_local_evidence_tag_from_fields(
    mode: ContentTag,
    history_genesis: ContentTag,
    history_head: ContentTag,
    final_world: ContentTag,
) -> ContentTag {
    hash_canonical(TagKind::Receipt, OPEN_LOCAL_EVIDENCE_SCHEMA, 128, |sink| {
        sink(&mode.digest());
        sink(&history_genesis.digest());
        sink(&history_head.digest());
        sink(&final_world.digest());
    })
    .expect("open-local evidence has a fixed payload length")
}

pub fn sequential_mode_tag() -> ContentTag {
    let function = sequential_function_tag();
    hash_canonical(TagKind::Policy, SEQUENTIAL_MODE_SCHEMA, 32, |sink| {
        sink(&function.digest());
    })
    .expect("sequential mode encodes one function digest")
}

pub fn canonical_segment_payload_len(
    injection_count: usize,
) -> Result<usize, SequentialCodecError> {
    injection_count
        .checked_mul(INJECTION_BYTES)
        .and_then(|bytes| SEGMENT_FIXED_BYTES.checked_add(bytes))
        .ok_or(SequentialCodecError::LengthOverflow)
}

pub fn verify_sequential_chain(
    initial_state: ContentTag,
    segments: &[SequentialSegment],
    limits: SequentialLimits,
) -> Result<SequentialVerification, SequentialError> {
    require_state_kind("initial state", initial_state)?;
    let usage = preflight_sequential_chain(segments, limits)?;

    let function = sequential_function_tag();
    let mut expected_parent = None;
    let mut expected_start = 0_u64;
    let mut expected_input = initial_state;

    for (segment_index, segment) in segments.iter().enumerate() {
        let payload = &segment.payload;
        if payload.function != function {
            return Err(SequentialSemanticError::UnsupportedFunction {
                segment_index,
                expected: function,
                actual: payload.function,
            }
            .into());
        }
        if payload.parent != expected_parent {
            return Err(SequentialSemanticError::ParentMismatch {
                segment_index,
                expected: expected_parent,
                actual: payload.parent,
            }
            .into());
        }
        if payload.start_iteration != expected_start {
            return Err(SequentialSemanticError::StartIterationMismatch {
                segment_index,
                expected: expected_start,
                actual: payload.start_iteration,
            }
            .into());
        }
        if payload.input != expected_input {
            return Err(SequentialSemanticError::InputMismatch {
                segment_index,
                expected: expected_input,
                actual: payload.input,
            }
            .into());
        }
        check_segment_shape(payload)?;
        let expected_tag = payload.content_tag();
        if segment.tag != expected_tag {
            return Err(SequentialSemanticError::SegmentTagMismatch {
                segment_index,
                expected: expected_tag,
                actual: segment.tag,
            }
            .into());
        }

        let derived = run_steps(
            function,
            payload.start_iteration,
            payload.iteration_count,
            payload.input,
            &payload.injections,
        );
        if derived != payload.output {
            return Err(SequentialSemanticError::OutputMismatch {
                segment_index,
                expected: derived,
                actual: payload.output,
            }
            .into());
        }

        expected_parent = Some(segment.tag);
        expected_start = payload.end_iteration();
        expected_input = payload.output;
    }

    Ok(SequentialVerification {
        function,
        head: expected_parent.expect("a nonempty verified chain has a head"),
        output: expected_input,
        usage,
    })
}

fn preflight_sequential_chain(
    segments: &[SequentialSegment],
    limits: SequentialLimits,
) -> Result<SequentialUsage, SequentialError> {
    if segments.is_empty() {
        return Err(SequentialSemanticError::EmptyChain.into());
    }
    if segments.len() > limits.max_segments {
        return Err(SequentialLimitError::SegmentCount {
            actual: segments.len(),
            max: limits.max_segments,
        }
        .into());
    }

    let segment_limits = limits.segment_limits();
    let mut total_iterations = 0_u64;
    let mut total_injections = 0_usize;

    for (segment_index, payload) in segments.iter().map(|segment| &segment.payload).enumerate() {
        let injection_count = payload.injections.len();
        check_iteration_limit(payload.iteration_count, segment_limits, segment_index)?;
        check_injection_limit(injection_count, segment_limits, segment_index)?;
        if payload.iteration_count == 0 {
            return Err(SequentialCodecError::ZeroIterations.into());
        }
        payload
            .start_iteration
            .checked_add(payload.iteration_count)
            .ok_or(SequentialCodecError::IterationOverflow)?;

        total_iterations = total_iterations
            .checked_add(payload.iteration_count)
            .ok_or(SequentialLimitError::ArithmeticOverflow {
                field: "total iteration count",
            })?;
        if total_iterations > limits.max_total_iterations {
            return Err(SequentialLimitError::TotalIterations {
                segment_index,
                actual: total_iterations,
                max: limits.max_total_iterations,
            }
            .into());
        }
        total_injections = total_injections.checked_add(injection_count).ok_or(
            SequentialLimitError::ArithmeticOverflow {
                field: "total injection count",
            },
        )?;
        if total_injections > limits.max_total_injections {
            return Err(SequentialLimitError::TotalInjections {
                segment_index,
                actual: total_injections,
                max: limits.max_total_injections,
            }
            .into());
        }
    }

    Ok(SequentialUsage {
        segment_count: segments.len(),
        total_iterations,
        total_injections,
    })
}

/// Requested coordinate-history admission mode.
///
/// `StakeConsensus` is representable as a request only so the boundary can refuse it explicitly;
/// there is no stake-policy tag or consensus codec in this checkpoint.
pub enum CoordinateHistoryModeClaim<'a> {
    OpenLocal,
    SequentialHistory { segments: &'a [SequentialSegment] },
    StakeConsensus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdmittedCoordinateHistoryMode {
    OpenLocal {
        mode: ContentTag,
        evidence: ContentTag,
        history_usage: HistoryReplayUsage,
    },
    SequentialHistory {
        mode: ContentTag,
        evidence: ContentTag,
        verification: SequentialVerification,
        covered_events: usize,
        history_usage: HistoryReplayUsage,
    },
}

pub fn admit_coordinate_history_mode(
    history: &History,
    initial_world: &World,
    claim: CoordinateHistoryModeClaim<'_>,
    sequential_limits: SequentialLimits,
) -> Result<AdmittedCoordinateHistoryMode, ModeAdmissionError> {
    admit_coordinate_history_mode_with_limits(
        history,
        initial_world,
        claim,
        HistoryReplayLimits::DEFAULT,
        sequential_limits,
    )
}

pub fn admit_coordinate_history_mode_with_limits(
    history: &History,
    initial_world: &World,
    claim: CoordinateHistoryModeClaim<'_>,
    history_limits: HistoryReplayLimits,
    sequential_limits: SequentialLimits,
) -> Result<AdmittedCoordinateHistoryMode, ModeAdmissionError> {
    match claim {
        CoordinateHistoryModeClaim::StakeConsensus => {
            Err(ModeAdmissionError::StakeConsensusUndefined)
        }
        CoordinateHistoryModeClaim::OpenLocal => {
            let verified =
                verify_in_memory_history_with_limits(initial_world, history, history_limits)?;
            let mode = open_local_mode_tag();
            Ok(AdmittedCoordinateHistoryMode::OpenLocal {
                mode,
                evidence: open_local_evidence_tag(verified.history(), mode),
                history_usage: verified.usage(),
            })
        }
        CoordinateHistoryModeClaim::SequentialHistory { segments } => {
            preflight_sequential_chain(segments, sequential_limits)?;
            let verified =
                verify_in_memory_history_with_limits(initial_world, history, history_limits)?;
            let history = verified.history();
            let required_events =
                history_event_count(history, sequential_limits.max_total_injections)?;
            verify_coordinate_event_coverage(history, segments, required_events)?;
            let initial_state = sequential_initial_state(history.genesis());
            let verification = verify_sequential_chain(initial_state, segments, sequential_limits)?;
            let mode = sequential_mode_tag();
            let evidence = coordinate_evidence_tag(history, mode, verification, required_events);
            Ok(AdmittedCoordinateHistoryMode::SequentialHistory {
                mode,
                evidence,
                verification,
                covered_events: required_events,
                history_usage: verified.usage(),
            })
        }
    }
}

fn build_segment(
    parent: Option<ContentTag>,
    start_iteration: u64,
    input: ContentTag,
    iteration_count: u64,
    injections: Vec<SequentialInjection>,
    limits: SequentialSegmentLimits,
    segment_index: usize,
) -> Result<SequentialSegment, SequentialError> {
    require_state_kind("segment input", input)?;
    let function = sequential_function_tag();
    let placeholder_output = input;
    let mut payload = SequentialSegmentPayload {
        function,
        parent,
        start_iteration,
        iteration_count,
        input,
        injections,
        output: placeholder_output,
    };
    check_iteration_limit(iteration_count, limits, segment_index)?;
    check_injection_limit(payload.injections.len(), limits, segment_index)?;
    if iteration_count == 0 {
        return Err(SequentialCodecError::ZeroIterations.into());
    }
    start_iteration
        .checked_add(iteration_count)
        .ok_or(SequentialCodecError::IterationOverflow)?;
    check_segment_shape(&payload)?;
    payload.output = run_steps(
        function,
        start_iteration,
        iteration_count,
        input,
        &payload.injections,
    );
    Ok(SequentialSegment::from_payload(payload))
}

fn check_segment_shape(payload: &SequentialSegmentPayload) -> Result<(), SequentialError> {
    if payload.iteration_count == 0 {
        return Err(SequentialCodecError::ZeroIterations.into());
    }
    payload
        .start_iteration
        .checked_add(payload.iteration_count)
        .ok_or(SequentialCodecError::IterationOverflow)?;
    require_state_kind("segment input", payload.input)?;
    require_state_kind("segment output", payload.output)?;
    if let Some(parent) = payload.parent {
        require_state_kind("segment parent", parent)?;
    }
    let mut previous = None;
    for (index, injection) in payload.injections.iter().enumerate() {
        if injection.event.kind() != TagKind::Event {
            return Err(SequentialSemanticError::EventKind {
                index,
                actual: injection.event.kind(),
            }
            .into());
        }
        if injection.offset >= payload.iteration_count {
            return Err(SequentialCodecError::InjectionOffsetOutOfRange {
                index,
                offset: injection.offset,
                iteration_count: payload.iteration_count,
            }
            .into());
        }
        if let Some(previous) = previous
            && injection.offset < previous
        {
            return Err(SequentialCodecError::InjectionOrder {
                index,
                previous,
                actual: injection.offset,
            }
            .into());
        }
        previous = Some(injection.offset);
    }
    canonical_segment_payload_len(payload.injections.len())?;
    Ok(())
}

fn require_state_kind(field: &'static str, tag: ContentTag) -> Result<(), SequentialSemanticError> {
    if tag.kind() != TagKind::SequentialHistory {
        return Err(SequentialSemanticError::StateKind {
            field,
            actual: tag.kind(),
        });
    }
    Ok(())
}

fn check_iteration_limit(
    actual: u64,
    limits: SequentialSegmentLimits,
    segment_index: usize,
) -> Result<(), SequentialLimitError> {
    if actual > limits.max_iterations {
        return Err(SequentialLimitError::IterationsPerSegment {
            segment_index,
            actual,
            max: limits.max_iterations,
        });
    }
    Ok(())
}

fn check_injection_limit(
    actual: usize,
    limits: SequentialSegmentLimits,
    segment_index: usize,
) -> Result<(), SequentialLimitError> {
    if actual > limits.max_injections {
        return Err(SequentialLimitError::InjectionsPerSegment {
            segment_index,
            actual,
            max: limits.max_injections,
        });
    }
    Ok(())
}

fn run_steps(
    function: ContentTag,
    start_iteration: u64,
    iteration_count: u64,
    input: ContentTag,
    injections: &[SequentialInjection],
) -> ContentTag {
    let mut state = input;
    let mut injection_cursor = 0_usize;
    let mut relative = 0_u64;
    while relative < iteration_count {
        let first = injection_cursor;
        while injection_cursor < injections.len() && injections[injection_cursor].offset == relative
        {
            injection_cursor += 1;
        }
        let absolute = start_iteration
            .checked_add(relative)
            .expect("an admitted segment has representable iterations");
        state = step_state(
            function,
            absolute,
            state,
            &injections[first..injection_cursor],
        );
        relative += 1;
    }
    debug_assert_eq!(injection_cursor, injections.len());
    state
}

fn step_state(
    function: ContentTag,
    absolute_iteration: u64,
    previous: ContentTag,
    injections: &[SequentialInjection],
) -> ContentTag {
    let injection_bytes = injections
        .len()
        .checked_mul(32)
        .expect("bounded injection bytes are representable");
    let payload_len = STEP_FIXED_BYTES
        .checked_add(injection_bytes)
        .expect("bounded step bytes are representable");
    hash_canonical(
        TagKind::SequentialHistory,
        SEQUENTIAL_STEP_SCHEMA,
        payload_len as u64,
        |sink| {
            sink(&function.digest());
            sink(&absolute_iteration.to_be_bytes());
            sink(&previous.digest());
            sink(&(injections.len() as u64).to_be_bytes());
            for injection in injections {
                sink(&injection.event.digest());
            }
        },
    )
    .expect("sequential step emits its measured length")
}

fn history_event_count(
    history: &History,
    max_total_injections: usize,
) -> Result<usize, SequentialLimitError> {
    let mut count = 0_usize;
    for receipt in history.receipts() {
        count = count.checked_add(receipt.effects().len()).ok_or(
            SequentialLimitError::ArithmeticOverflow {
                field: "coordinate-history event count",
            },
        )?;
        if count > max_total_injections {
            return Err(SequentialLimitError::TotalInjections {
                segment_index: 0,
                actual: count,
                max: max_total_injections,
            });
        }
    }
    Ok(count)
}

fn verify_coordinate_event_coverage(
    history: &History,
    segments: &[SequentialSegment],
    required_count: usize,
) -> Result<(), EventCoverageError> {
    let actual_count = segments.iter().try_fold(0_usize, |count, segment| {
        count.checked_add(segment.payload.injections.len())
    });
    let Some(actual_count) = actual_count else {
        return Err(EventCoverageError::CountOverflow);
    };
    if actual_count != required_count {
        return Err(EventCoverageError::CountMismatch {
            required: required_count,
            actual: actual_count,
        });
    }

    let expected = history
        .receipts()
        .iter()
        .flat_map(|receipt| receipt.effects().iter().map(|effect| effect.event()));
    let actual = segments.iter().flat_map(|segment| {
        segment
            .payload
            .injections
            .iter()
            .map(|injection| injection.event)
    });
    for (index, (expected, actual)) in expected.zip(actual).enumerate() {
        if expected != actual {
            return Err(EventCoverageError::Mismatch {
                index,
                expected,
                actual,
            });
        }
    }
    Ok(())
}

fn coordinate_evidence_tag(
    history: &History,
    mode: ContentTag,
    verification: SequentialVerification,
    event_count: usize,
) -> ContentTag {
    let event_count = u64::try_from(event_count).expect("bounded event count fits u64");
    coordinate_evidence_tag_from_fields(
        mode,
        history.genesis(),
        history.head(),
        verification.head,
        verification.output,
        verification.usage.total_iterations,
        event_count,
    )
}

fn coordinate_evidence_tag_from_fields(
    mode: ContentTag,
    history_genesis: ContentTag,
    history_head: ContentTag,
    segment_head: ContentTag,
    output: ContentTag,
    total_iterations: u64,
    event_count: u64,
) -> ContentTag {
    hash_canonical(
        TagKind::SequentialHistory,
        COORDINATE_EVIDENCE_SCHEMA,
        176,
        |sink| {
            sink(&mode.digest());
            sink(&history_genesis.digest());
            sink(&history_head.digest());
            sink(&segment_head.digest());
            sink(&output.digest());
            sink(&total_iterations.to_be_bytes());
            sink(&event_count.to_be_bytes());
        },
    )
    .expect("coordinate evidence has a fixed payload length")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SequentialCodecError {
    LengthOverflow,
    AllocationFailed {
        requested: usize,
    },
    WrongLength {
        expected: usize,
        actual: usize,
    },
    ParentFlag {
        actual: u8,
    },
    NonzeroNilParent,
    ZeroIterations,
    IterationOverflow,
    InjectionOffsetOutOfRange {
        index: usize,
        offset: u64,
        iteration_count: u64,
    },
    InjectionOrder {
        index: usize,
        previous: u64,
        actual: u64,
    },
    NonCanonicalEncoding,
}

impl fmt::Display for SequentialCodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LengthOverflow => {
                formatter.write_str("sequential segment length is not representable")
            }
            Self::AllocationFailed { requested } => write!(
                formatter,
                "could not allocate {requested} sequential injection entries"
            ),
            Self::WrongLength { expected, actual } => write!(
                formatter,
                "sequential segment requires {expected} bytes, found {actual}"
            ),
            Self::ParentFlag { actual } => {
                write!(
                    formatter,
                    "sequential parent flag is {actual}, expected 0 or 1"
                )
            }
            Self::NonzeroNilParent => formatter
                .write_str("sequential segment without a parent must encode a zero parent digest"),
            Self::ZeroIterations => {
                formatter.write_str("sequential segment must execute at least one iteration")
            }
            Self::IterationOverflow => {
                formatter.write_str("sequential segment end iteration overflowed u64")
            }
            Self::InjectionOffsetOutOfRange {
                index,
                offset,
                iteration_count,
            } => write!(
                formatter,
                "sequential injection {index} offset {offset} is outside {iteration_count} iterations"
            ),
            Self::InjectionOrder {
                index,
                previous,
                actual,
            } => write!(
                formatter,
                "sequential injection {index} offset {actual} precedes prior offset {previous}"
            ),
            Self::NonCanonicalEncoding => {
                formatter.write_str("sequential segment does not re-encode canonically")
            }
        }
    }
}

impl std::error::Error for SequentialCodecError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SequentialSemanticError {
    StateKind {
        field: &'static str,
        actual: TagKind,
    },
    EventKind {
        index: usize,
        actual: TagKind,
    },
    EmptyChain,
    UnsupportedFunction {
        segment_index: usize,
        expected: ContentTag,
        actual: ContentTag,
    },
    SegmentTagMismatch {
        segment_index: usize,
        expected: ContentTag,
        actual: ContentTag,
    },
    ParentMismatch {
        segment_index: usize,
        expected: Option<ContentTag>,
        actual: Option<ContentTag>,
    },
    StartIterationMismatch {
        segment_index: usize,
        expected: u64,
        actual: u64,
    },
    InputMismatch {
        segment_index: usize,
        expected: ContentTag,
        actual: ContentTag,
    },
    OutputMismatch {
        segment_index: usize,
        expected: ContentTag,
        actual: ContentTag,
    },
}

impl fmt::Display for SequentialSemanticError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StateKind { field, actual } => write!(
                formatter,
                "{field} must be sequential-history, received {}",
                actual.name()
            ),
            Self::EventKind { index, actual } => write!(
                formatter,
                "sequential injection {index} must be event, received {}",
                actual.name()
            ),
            Self::EmptyChain => formatter.write_str("sequential chain cannot be empty"),
            Self::UnsupportedFunction {
                segment_index,
                expected,
                actual,
            } => write!(
                formatter,
                "sequential segment {segment_index} function is {actual}, supported function is {expected}"
            ),
            Self::SegmentTagMismatch {
                segment_index,
                expected,
                actual,
            } => write!(
                formatter,
                "sequential segment {segment_index} tag is {actual}, canonical payload names {expected}"
            ),
            Self::ParentMismatch {
                segment_index,
                expected,
                actual,
            } => write!(
                formatter,
                "sequential segment {segment_index} parent is {actual:?}, expected {expected:?}"
            ),
            Self::StartIterationMismatch {
                segment_index,
                expected,
                actual,
            } => write!(
                formatter,
                "sequential segment {segment_index} starts at {actual}, expected {expected}"
            ),
            Self::InputMismatch {
                segment_index,
                expected,
                actual,
            } => write!(
                formatter,
                "sequential segment {segment_index} input is {actual}, expected {expected}"
            ),
            Self::OutputMismatch {
                segment_index,
                expected,
                actual,
            } => write!(
                formatter,
                "sequential segment {segment_index} output is {actual}, recomputation produced {expected}"
            ),
        }
    }
}

impl std::error::Error for SequentialSemanticError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SequentialLimitError {
    SegmentCount {
        actual: usize,
        max: usize,
    },
    IterationsPerSegment {
        segment_index: usize,
        actual: u64,
        max: u64,
    },
    TotalIterations {
        segment_index: usize,
        actual: u64,
        max: u64,
    },
    InjectionsPerSegment {
        segment_index: usize,
        actual: usize,
        max: usize,
    },
    TotalInjections {
        segment_index: usize,
        actual: usize,
        max: usize,
    },
    ArithmeticOverflow {
        field: &'static str,
    },
}

impl fmt::Display for SequentialLimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SegmentCount { actual, max } => write!(
                formatter,
                "sequential chain has {actual} segments, local limit is {max}"
            ),
            Self::IterationsPerSegment {
                segment_index,
                actual,
                max,
            } => write!(
                formatter,
                "sequential segment {segment_index} has {actual} iterations, local per-segment limit is {max}"
            ),
            Self::TotalIterations {
                segment_index,
                actual,
                max,
            } => write!(
                formatter,
                "sequential chain reaches {actual} iterations at segment {segment_index}, local total limit is {max}"
            ),
            Self::InjectionsPerSegment {
                segment_index,
                actual,
                max,
            } => write!(
                formatter,
                "sequential segment {segment_index} has {actual} injections, local per-segment limit is {max}"
            ),
            Self::TotalInjections {
                segment_index,
                actual,
                max,
            } => write!(
                formatter,
                "sequential chain reaches {actual} injections at segment {segment_index}, local total limit is {max}"
            ),
            Self::ArithmeticOverflow { field } => {
                write!(formatter, "sequential {field} arithmetic overflowed")
            }
        }
    }
}

impl std::error::Error for SequentialLimitError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SequentialError {
    Codec(SequentialCodecError),
    Semantic(SequentialSemanticError),
    Limit(SequentialLimitError),
}

impl fmt::Display for SequentialError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Codec(source) => write!(formatter, "sequential codec failed: {source}"),
            Self::Semantic(source) => write!(formatter, "sequential semantics failed: {source}"),
            Self::Limit(source) => write!(formatter, "sequential local policy refused: {source}"),
        }
    }
}

impl std::error::Error for SequentialError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Codec(source) => Some(source),
            Self::Semantic(source) => Some(source),
            Self::Limit(source) => Some(source),
        }
    }
}

impl From<SequentialCodecError> for SequentialError {
    fn from(source: SequentialCodecError) -> Self {
        Self::Codec(source)
    }
}

impl From<SequentialSemanticError> for SequentialError {
    fn from(source: SequentialSemanticError) -> Self {
        Self::Semantic(source)
    }
}

impl From<SequentialLimitError> for SequentialError {
    fn from(source: SequentialLimitError) -> Self {
        Self::Limit(source)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventCoverageError {
    CountOverflow,
    CountMismatch {
        required: usize,
        actual: usize,
    },
    Mismatch {
        index: usize,
        expected: ContentTag,
        actual: ContentTag,
    },
}

impl fmt::Display for EventCoverageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CountOverflow => {
                formatter.write_str("sequential injected-event count overflowed usize")
            }
            Self::CountMismatch { required, actual } => write!(
                formatter,
                "coordinate history requires {required} events, sequential chain injects {actual}"
            ),
            Self::Mismatch {
                index,
                expected,
                actual,
            } => write!(
                formatter,
                "sequential event {index} is {actual}, coordinate history requires {expected}"
            ),
        }
    }
}

impl std::error::Error for EventCoverageError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModeAdmissionError {
    History(InMemoryHistoryVerificationError),
    Sequential(SequentialError),
    EventCoverage(EventCoverageError),
    StakeConsensusUndefined,
}

impl fmt::Display for ModeAdmissionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::History(source) => write!(formatter, "coordinate history refused: {source}"),
            Self::Sequential(source) => write!(formatter, "sequential mode refused: {source}"),
            Self::EventCoverage(source) => {
                write!(formatter, "sequential event coverage refused: {source}")
            }
            Self::StakeConsensusUndefined => formatter.write_str(
                "stake-consensus mode is undefined: sequential work supplies no stake, validator set, signatures, quorum, fork choice, finality, slashing, or network protocol",
            ),
        }
    }
}

impl std::error::Error for ModeAdmissionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::History(source) => Some(source),
            Self::Sequential(source) => Some(source),
            Self::EventCoverage(source) => Some(source),
            Self::StakeConsensusUndefined => None,
        }
    }
}

impl From<InMemoryHistoryVerificationError> for ModeAdmissionError {
    fn from(source: InMemoryHistoryVerificationError) -> Self {
        Self::History(source)
    }
}

impl From<SequentialError> for ModeAdmissionError {
    fn from(source: SequentialError) -> Self {
        Self::Sequential(source)
    }
}

impl From<SequentialSemanticError> for ModeAdmissionError {
    fn from(source: SequentialSemanticError) -> Self {
        Self::Sequential(source.into())
    }
}

impl From<SequentialLimitError> for ModeAdmissionError {
    fn from(source: SequentialLimitError) -> Self {
        Self::Sequential(source.into())
    }
}

impl From<EventCoverageError> for ModeAdmissionError {
    fn from(source: EventCoverageError) -> Self {
        Self::EventCoverage(source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::program::{Edit, Operation};
    use crate::world::{BEDROCK_COORD, Coord, World};

    fn hex32(value: &str) -> [u8; 32] {
        assert_eq!(value.len(), 64);
        let mut digest = [0_u8; 32];
        for (index, byte) in digest.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).unwrap();
        }
        digest
    }

    fn range_digest(start: u8) -> [u8; 32] {
        let mut digest = [0_u8; 32];
        for (offset, byte) in digest.iter_mut().enumerate() {
            *byte = start.wrapping_add(offset as u8);
        }
        digest
    }

    fn event(start: u8) -> ContentTag {
        ContentTag::from_digest(TagKind::Event, range_digest(start))
    }

    fn kat_segments() -> (ContentTag, SequentialSegment, SequentialSegment) {
        let source = ContentTag::from_digest(TagKind::Receipt, range_digest(0));
        let initial = sequential_initial_state(source);
        let first = SequentialSegment::first(
            initial,
            4,
            vec![
                SequentialInjection::new(0, event(0x20)).unwrap(),
                SequentialInjection::new(0, event(0x40)).unwrap(),
                SequentialInjection::new(3, event(0x60)).unwrap(),
            ],
            SequentialSegmentLimits::DEFAULT,
        )
        .unwrap();
        let second = first
            .next(
                2,
                vec![SequentialInjection::new(1, event(0x80)).unwrap()],
                SequentialSegmentLimits::DEFAULT,
            )
            .unwrap();
        (initial, first, second)
    }

    fn sample_history() -> (World, History) {
        let mut world = World::new();
        let initial_world = world.clone();
        let mut history = History::new(&world);
        let mutable = Coord::new(2, -1, 3);

        let mut first = history.begin_tick(5);
        first.apply_edit(&history, &mut world, Edit::new(Operation::On, mutable));
        first.apply_edit(&history, &mut world, Edit::new(Operation::On, mutable));
        history.commit_tick(first, &world).unwrap();

        let mut second = history.begin_tick(12);
        second.apply_edit(&history, &mut world, Edit::new(Operation::Off, mutable));
        second.apply_edit(&history, &mut world, Edit::new(Operation::Off, mutable));
        second.apply_edit(
            &history,
            &mut world,
            Edit::new(Operation::Off, BEDROCK_COORD),
        );
        history.commit_tick(second, &world).unwrap();
        (initial_world, history)
    }

    fn history_events(history: &History) -> Vec<ContentTag> {
        history
            .receipts()
            .iter()
            .flat_map(|receipt| receipt.effects().iter().map(|effect| effect.event()))
            .collect()
    }

    /// The two history-mode admission policies, pinned against `tools/history_mode_vectors.py`.
    ///
    /// A mode is the rule an admitted history is judged under, and the pair is deliberately
    /// asymmetric. Open-local carries an empty payload -- the schema string is its entire
    /// claim -- so nothing but the domain wrapper separates it from any other nullary policy.
    /// The sequential mode carries exactly the function policy's digest, which is what stops
    /// a transcript admitted under one recurrence from being read under another.
    ///
    /// The literals below were produced by the Python reconstruction and never by this
    /// module; a disagreement means one of the two encoders moved.
    #[test]
    fn independent_python_vectors_pin_both_history_modes() {
        assert_eq!(
            open_local_mode_tag().to_string(),
            "#bb.policy.sha256.aglwue2ddgg2owhe57dz3r3w4233jdfs2fq43iyhubeswe7v3aeq"
        );
        assert_eq!(
            sequential_mode_tag().to_string(),
            "#bb.policy.sha256.k5hvqhzkrstz4qvds534qftmj7zy5qhcr2limfqafdbisyrw3zzq"
        );

        // The sequential mode exists to bind its function, so it must not equal that function,
        // and the two modes must never collide with each other.
        assert_ne!(sequential_mode_tag(), sequential_function_tag());
        assert_ne!(open_local_mode_tag(), sequential_mode_tag());

        // Both are policies. A mode tag can never be mistaken for the history it admits.
        assert_eq!(open_local_mode_tag().kind(), TagKind::Policy);
        assert_eq!(sequential_mode_tag().kind(), TagKind::Policy);
    }

    #[test]
    fn independent_python_vectors_pin_every_recurrence_step() {
        let function = sequential_function_tag();
        assert_eq!(
            function.digest(),
            hex32("dba40edbdd77458572cfb1664e7834ab9d4e7f85c3f2ba5bbd21d7a54f2142f0")
        );
        assert_eq!(
            function.to_string(),
            "#bb.policy.sha256.3osa5w65o5cyk4wpwfte46buvoou474fypzluw55ehl2ktzbilya"
        );

        let (initial, first, second) = kat_segments();
        assert_eq!(
            initial.digest(),
            hex32("67f3794d9202b4d670973d981c3816c7777f88a1b3b0c691b446fddac5e3a3a0")
        );

        let injections = first.payload.injections();
        let state_0 = step_state(function, 0, initial, &injections[..2]);
        let state_1 = step_state(function, 1, state_0, &[]);
        let state_2 = step_state(function, 2, state_1, &[]);
        let state_3 = step_state(function, 3, state_2, &injections[2..]);
        assert_eq!(
            state_0.digest(),
            hex32("3687283f436d425bd82234eadfc1ba288bcde511f3e17a186ae060aab8dfbdc5")
        );
        assert_eq!(
            state_1.digest(),
            hex32("6c5f21bdde4a84c2ef949f8e6d637990823e2cc9de2549354ce3cab20af4f031")
        );
        assert_eq!(
            state_2.digest(),
            hex32("8fc88d67db62dceace956d3f6c717943b4bd0452abe8dfb922fa567cb8889f58")
        );
        assert_eq!(
            state_3.digest(),
            hex32("05ae4e8ef013fab927d04dff53285a8dc91e7607fe7b52079c508d96321015c5")
        );
        assert_eq!(first.payload.output(), state_3);
        assert_eq!(first.encode_canonical().len(), 273);
        assert_eq!(
            first.tag().digest(),
            hex32("65a11acf802c61ea3fd09b08b5d594d767841a33413b842f3a5085473ebacb96")
        );

        let state_4 = step_state(function, 4, state_3, &[]);
        let state_5 = step_state(function, 5, state_4, second.payload.injections());
        assert_eq!(
            state_4.digest(),
            hex32("36be1a5d5775c05c7b3285f70b538058b0a65935b14ed32b6ee966c84b780406")
        );
        assert_eq!(
            state_5.digest(),
            hex32("14434c2973e199790a467b75c1abbcbf6d93a54244c540fbe247543443e652d9")
        );
        assert_eq!(second.payload.output(), state_5);
        assert_eq!(second.encode_canonical().len(), 193);
        assert_eq!(
            second.tag().digest(),
            hex32("81f76fa460f9999e29c3f6a85ca1ac5ec0db2a96d98527a4e019fca540a95097")
        );

        let joined = SequentialSegment::first(
            initial,
            6,
            vec![
                SequentialInjection::new(0, event(0x20)).unwrap(),
                SequentialInjection::new(0, event(0x40)).unwrap(),
                SequentialInjection::new(3, event(0x60)).unwrap(),
                SequentialInjection::new(5, event(0x80)).unwrap(),
            ],
            SequentialSegmentLimits::DEFAULT,
        )
        .unwrap();
        assert_eq!(joined.payload.output(), second.payload.output());
        assert_eq!(joined.encode_canonical().len(), 313);
        assert_eq!(
            joined.tag().digest(),
            hex32("1a5f7b91b68ab89d9da14b4accb7e428e4386afa0545f5c6523a07449d518a12")
        );
        assert_ne!(joined.tag(), second.tag());
    }

    #[test]
    fn independent_evidence_vector_pins_every_bound_field() {
        let mode = ContentTag::from_digest(TagKind::Policy, range_digest(0x00));
        let genesis = ContentTag::from_digest(TagKind::Receipt, range_digest(0x20));
        let history_head = ContentTag::from_digest(TagKind::Receipt, range_digest(0x40));
        let segment_head = ContentTag::from_digest(TagKind::SequentialHistory, range_digest(0x60));
        let output = ContentTag::from_digest(TagKind::SequentialHistory, range_digest(0x80));
        let baseline = coordinate_evidence_tag_from_fields(
            mode,
            genesis,
            history_head,
            segment_head,
            output,
            6,
            4,
        );
        assert_eq!(
            baseline.digest(),
            hex32("f12be205157bd0371862856852e144d5567cf4d60442ef60c28dcc026a489fa2")
        );

        assert_ne!(
            baseline,
            coordinate_evidence_tag_from_fields(
                ContentTag::from_digest(TagKind::Policy, [0xff; 32]),
                genesis,
                history_head,
                segment_head,
                output,
                6,
                4,
            )
        );
        assert_ne!(
            baseline,
            coordinate_evidence_tag_from_fields(
                mode,
                ContentTag::from_digest(TagKind::Receipt, [0xff; 32]),
                history_head,
                segment_head,
                output,
                6,
                4,
            )
        );
        assert_ne!(
            baseline,
            coordinate_evidence_tag_from_fields(
                mode,
                genesis,
                ContentTag::from_digest(TagKind::Receipt, [0xff; 32]),
                segment_head,
                output,
                6,
                4,
            )
        );
        assert_ne!(
            baseline,
            coordinate_evidence_tag_from_fields(
                mode,
                genesis,
                history_head,
                ContentTag::from_digest(TagKind::SequentialHistory, [0xff; 32]),
                output,
                6,
                4,
            )
        );
        assert_ne!(
            baseline,
            coordinate_evidence_tag_from_fields(
                mode,
                genesis,
                history_head,
                segment_head,
                ContentTag::from_digest(TagKind::SequentialHistory, [0xff; 32]),
                6,
                4,
            )
        );
        assert_ne!(
            baseline,
            coordinate_evidence_tag_from_fields(
                mode,
                genesis,
                history_head,
                segment_head,
                output,
                7,
                4,
            )
        );
        assert_ne!(
            baseline,
            coordinate_evidence_tag_from_fields(
                mode,
                genesis,
                history_head,
                segment_head,
                output,
                6,
                5,
            )
        );
    }

    #[test]
    fn independent_open_local_evidence_binds_the_verified_history() {
        let mode = ContentTag::from_digest(TagKind::Policy, range_digest(0x00));
        let genesis = ContentTag::from_digest(TagKind::Receipt, range_digest(0x20));
        let history_head = ContentTag::from_digest(TagKind::Receipt, range_digest(0x40));
        let final_world = ContentTag::from_digest(TagKind::World, range_digest(0x60));
        let baseline =
            open_local_evidence_tag_from_fields(mode, genesis, history_head, final_world);
        assert_eq!(
            baseline.digest(),
            hex32("df7f4e9c3aa676520d67e697fa70ecf32b8508d6083d08832c0b25bfe4529f09")
        );
        assert_ne!(
            baseline,
            open_local_evidence_tag_from_fields(
                ContentTag::from_digest(TagKind::Policy, [0xff; 32]),
                genesis,
                history_head,
                final_world,
            )
        );
        assert_ne!(
            baseline,
            open_local_evidence_tag_from_fields(
                mode,
                ContentTag::from_digest(TagKind::Receipt, [0xff; 32]),
                history_head,
                final_world,
            )
        );
        assert_ne!(
            baseline,
            open_local_evidence_tag_from_fields(
                mode,
                genesis,
                ContentTag::from_digest(TagKind::Receipt, [0xff; 32]),
                final_world,
            )
        );
        assert_ne!(
            baseline,
            open_local_evidence_tag_from_fields(
                mode,
                genesis,
                history_head,
                ContentTag::from_digest(TagKind::World, [0xff; 32]),
            )
        );

        let mut first_world = World::new();
        assert!(first_world.set_on(Coord::new(1, 0, 0)));
        let first_history = History::new(&first_world);
        let mut second_world = World::new();
        assert!(second_world.set_on(Coord::new(2, 0, 0)));
        let second_history = History::new(&second_world);
        let first = admit_coordinate_history_mode(
            &first_history,
            &first_world,
            CoordinateHistoryModeClaim::OpenLocal,
            SequentialLimits::DEFAULT,
        )
        .unwrap();
        let second = admit_coordinate_history_mode(
            &second_history,
            &second_world,
            CoordinateHistoryModeClaim::OpenLocal,
            SequentialLimits::DEFAULT,
        )
        .unwrap();
        let (
            AdmittedCoordinateHistoryMode::OpenLocal {
                evidence: first_evidence,
                history_usage: first_usage,
                ..
            },
            AdmittedCoordinateHistoryMode::OpenLocal {
                evidence: second_evidence,
                history_usage: second_usage,
                ..
            },
        ) = (first, second)
        else {
            unreachable!("both claims requested open-local admission")
        };
        assert_eq!(first_usage, second_usage);
        assert_ne!(first_evidence, second_evidence);
        assert_ne!(first, second);
    }

    #[test]
    fn strict_segment_codec_round_trips_and_rejects_every_boundary_mutant() {
        let (_, first, _) = kat_segments();
        let bytes = first.encode_canonical();
        assert_eq!(
            SequentialSegmentPayload::declared_injection_count(&bytes),
            Ok(3)
        );
        let decoded =
            SequentialSegment::decode_canonical(&bytes, SequentialSegmentLimits::DEFAULT).unwrap();
        assert_eq!(decoded, first);

        for cut in 0..bytes.len() {
            assert!(
                SequentialSegment::decode_canonical(
                    &bytes[..cut],
                    SequentialSegmentLimits::DEFAULT,
                )
                .is_err()
            );
        }
        let mut overlong = bytes.clone();
        overlong.push(0);
        assert!(matches!(
            SequentialSegment::decode_canonical(&overlong, SequentialSegmentLimits::DEFAULT),
            Err(SequentialError::Codec(
                SequentialCodecError::WrongLength { .. }
            ))
        ));

        let mut wrong_count = bytes.clone();
        wrong_count[113..121].copy_from_slice(&4_u64.to_be_bytes());
        assert!(matches!(
            SequentialSegment::decode_canonical(&wrong_count, SequentialSegmentLimits::new(4, 0)),
            Err(SequentialError::Codec(
                SequentialCodecError::WrongLength { .. }
            ))
        ));

        let mut parent_flag = bytes.clone();
        parent_flag[32] = 2;
        assert!(matches!(
            SequentialSegment::decode_canonical(&parent_flag, SequentialSegmentLimits::DEFAULT),
            Err(SequentialError::Codec(SequentialCodecError::ParentFlag {
                actual: 2
            }))
        ));
        let mut nil_parent = bytes.clone();
        nil_parent[33] = 1;
        assert!(matches!(
            SequentialSegment::decode_canonical(&nil_parent, SequentialSegmentLimits::DEFAULT),
            Err(SequentialError::Codec(
                SequentialCodecError::NonzeroNilParent
            ))
        ));
        let mut zero_iterations = bytes.clone();
        zero_iterations[73..81].copy_from_slice(&0_u64.to_be_bytes());
        assert!(matches!(
            SequentialSegment::decode_canonical(&zero_iterations, SequentialSegmentLimits::DEFAULT),
            Err(SequentialError::Codec(SequentialCodecError::ZeroIterations))
        ));
        let mut overflow = bytes.clone();
        overflow[65..73].copy_from_slice(&u64::MAX.to_be_bytes());
        assert!(matches!(
            SequentialSegment::decode_canonical(&overflow, SequentialSegmentLimits::DEFAULT),
            Err(SequentialError::Codec(
                SequentialCodecError::IterationOverflow
            ))
        ));
        let mut out_of_range = bytes.clone();
        out_of_range[121..129].copy_from_slice(&4_u64.to_be_bytes());
        assert!(matches!(
            SequentialSegment::decode_canonical(&out_of_range, SequentialSegmentLimits::DEFAULT),
            Err(SequentialError::Codec(
                SequentialCodecError::InjectionOffsetOutOfRange { index: 0, .. }
            ))
        ));
        let mut descending = bytes.clone();
        descending[121..129].copy_from_slice(&2_u64.to_be_bytes());
        descending[161..169].copy_from_slice(&1_u64.to_be_bytes());
        assert!(matches!(
            SequentialSegment::decode_canonical(&descending, SequentialSegmentLimits::DEFAULT),
            Err(SequentialError::Codec(
                SequentialCodecError::InjectionOrder { index: 1, .. }
            ))
        ));

        assert!(
            SequentialSegment::decode_canonical(&bytes, SequentialSegmentLimits::new(4, 3)).is_ok()
        );
        assert!(matches!(
            SequentialSegment::decode_canonical(&bytes, SequentialSegmentLimits::new(4, 2)),
            Err(SequentialError::Limit(
                SequentialLimitError::InjectionsPerSegment {
                    actual: 3,
                    max: 2,
                    ..
                }
            ))
        ));
    }

    #[test]
    fn chain_verification_refuses_repaired_semantic_mutants_and_selects_no_branch() {
        let (initial, first, second) = kat_segments();
        let exact = SequentialLimits::new(2, 4, 6, 3, 4);
        let verified =
            verify_sequential_chain(initial, &[first.clone(), second.clone()], exact).unwrap();
        assert_eq!(
            verified.usage,
            SequentialUsage {
                segment_count: 2,
                total_iterations: 6,
                total_injections: 4,
            }
        );
        assert_eq!(verified.head, second.tag());
        assert_eq!(verified.output, second.payload.output());

        let mut unsupported_payload = first.payload.clone();
        unsupported_payload.function =
            hash_canonical(TagKind::Policy, b"test/cheap-function/v1", 0, |_| {}).unwrap();
        let unsupported = SequentialSegment::from_payload(unsupported_payload);
        assert!(matches!(
            verify_sequential_chain(initial, &[unsupported], exact),
            Err(SequentialError::Semantic(
                SequentialSemanticError::UnsupportedFunction {
                    segment_index: 0,
                    ..
                }
            ))
        ));

        let mut wrong_output_payload = first.payload.clone();
        wrong_output_payload.output =
            sequential_initial_state(ContentTag::from_digest(TagKind::Receipt, [0xff; 32]));
        let wrong_output = SequentialSegment::from_payload(wrong_output_payload);
        assert!(matches!(
            verify_sequential_chain(initial, &[wrong_output], exact),
            Err(SequentialError::Semantic(
                SequentialSemanticError::OutputMismatch {
                    segment_index: 0,
                    ..
                }
            ))
        ));

        let mut wrong_parent_payload = second.payload.clone();
        wrong_parent_payload.parent = None;
        let wrong_parent = SequentialSegment::from_payload(wrong_parent_payload);
        assert!(matches!(
            verify_sequential_chain(initial, &[first.clone(), wrong_parent], exact),
            Err(SequentialError::Semantic(
                SequentialSemanticError::ParentMismatch {
                    segment_index: 1,
                    ..
                }
            ))
        ));

        let mut wrong_start_payload = second.payload.clone();
        wrong_start_payload.start_iteration = 5;
        let wrong_start = SequentialSegment::from_payload(wrong_start_payload);
        assert!(matches!(
            verify_sequential_chain(initial, &[first.clone(), wrong_start], exact),
            Err(SequentialError::Semantic(
                SequentialSemanticError::StartIterationMismatch {
                    segment_index: 1,
                    ..
                }
            ))
        ));

        let mut wrong_input_payload = second.payload.clone();
        wrong_input_payload.input = initial;
        let wrong_input = SequentialSegment::from_payload(wrong_input_payload);
        assert!(matches!(
            verify_sequential_chain(initial, &[first.clone(), wrong_input], exact),
            Err(SequentialError::Semantic(
                SequentialSemanticError::InputMismatch {
                    segment_index: 1,
                    ..
                }
            ))
        ));

        let mut wrong_tag = first.clone();
        wrong_tag.tag =
            sequential_initial_state(ContentTag::from_digest(TagKind::Receipt, [0xee; 32]));
        assert!(matches!(
            verify_sequential_chain(initial, &[wrong_tag], exact),
            Err(SequentialError::Semantic(
                SequentialSemanticError::SegmentTagMismatch {
                    segment_index: 0,
                    ..
                }
            ))
        ));

        let branch_limits = SequentialLimits::new(2, 6, 6, 4, 4);
        let joined = SequentialSegment::first(
            initial,
            6,
            vec![
                SequentialInjection::new(0, event(0x20)).unwrap(),
                SequentialInjection::new(0, event(0x40)).unwrap(),
                SequentialInjection::new(3, event(0x60)).unwrap(),
                SequentialInjection::new(5, event(0x80)).unwrap(),
            ],
            branch_limits.segment_limits(),
        )
        .unwrap();
        let joined_verification =
            verify_sequential_chain(initial, std::slice::from_ref(&joined), branch_limits).unwrap();
        assert_eq!(joined_verification.output, verified.output);
        assert_ne!(joined_verification.head, verified.head);
        assert!(verify_sequential_chain(initial, &[first], exact).is_ok());
    }

    #[test]
    fn local_limits_refuse_work_without_classifying_the_transcript_as_corrupt() {
        let (initial, first, second) = kat_segments();
        let chain = [first.clone(), second.clone()];
        let exact = SequentialLimits::new(2, 4, 6, 3, 4);
        assert!(verify_sequential_chain(initial, &chain, exact).is_ok());

        assert!(matches!(
            verify_sequential_chain(initial, &chain, SequentialLimits::new(1, 4, 6, 3, 4)),
            Err(SequentialError::Limit(SequentialLimitError::SegmentCount {
                actual: 2,
                max: 1
            }))
        ));
        assert!(matches!(
            verify_sequential_chain(initial, &chain, SequentialLimits::new(2, 3, 6, 3, 4)),
            Err(SequentialError::Limit(
                SequentialLimitError::IterationsPerSegment {
                    segment_index: 0,
                    actual: 4,
                    max: 3
                }
            ))
        ));
        assert!(matches!(
            verify_sequential_chain(initial, &chain, SequentialLimits::new(2, 4, 5, 3, 4)),
            Err(SequentialError::Limit(
                SequentialLimitError::TotalIterations {
                    segment_index: 1,
                    actual: 6,
                    max: 5
                }
            ))
        ));
        assert!(matches!(
            verify_sequential_chain(initial, &chain, SequentialLimits::new(2, 4, 6, 3, 3)),
            Err(SequentialError::Limit(
                SequentialLimitError::TotalInjections {
                    segment_index: 1,
                    actual: 4,
                    max: 3
                }
            ))
        ));
        assert!(verify_sequential_chain(initial, &chain, exact).is_ok());

        let mut wrong_tag = first;
        wrong_tag.tag =
            sequential_initial_state(ContentTag::from_digest(TagKind::Receipt, [0xdd; 32]));
        assert!(matches!(
            verify_sequential_chain(
                initial,
                std::slice::from_ref(&wrong_tag),
                SequentialLimits::new(1, 4, 4, 2, 4)
            ),
            Err(SequentialError::Limit(
                SequentialLimitError::InjectionsPerSegment {
                    segment_index: 0,
                    actual: 3,
                    max: 2
                }
            ))
        ));
        assert!(matches!(
            verify_sequential_chain(
                initial,
                &[wrong_tag, second],
                SequentialLimits::new(2, 4, 6, 3, 3)
            ),
            Err(SequentialError::Limit(
                SequentialLimitError::TotalInjections {
                    segment_index: 1,
                    actual: 4,
                    max: 3
                }
            ))
        ));
    }

    #[test]
    fn coordinate_mode_derives_complete_event_coverage_and_refuses_stake_consensus() {
        let (initial_world, history) = sample_history();
        let events = history_events(&history);
        assert_eq!(events.len(), 5);
        let initial = sequential_initial_state(history.genesis());
        let limits = SequentialLimits::new(2, 16, 32, 8, 8);
        let injections = events
            .iter()
            .enumerate()
            .map(|(index, &event)| SequentialInjection::new(index as u64, event).unwrap())
            .collect();
        let segment =
            SequentialSegment::first(initial, 8, injections, limits.segment_limits()).unwrap();

        assert!(matches!(
            admit_coordinate_history_mode(
                &history,
                &initial_world,
                CoordinateHistoryModeClaim::OpenLocal,
                limits
            ),
            Ok(AdmittedCoordinateHistoryMode::OpenLocal { .. })
        ));
        let admitted = admit_coordinate_history_mode(
            &history,
            &initial_world,
            CoordinateHistoryModeClaim::SequentialHistory {
                segments: std::slice::from_ref(&segment),
            },
            limits,
        )
        .unwrap();
        let AdmittedCoordinateHistoryMode::SequentialHistory {
            mode,
            evidence,
            verification,
            covered_events,
            history_usage,
        } = admitted
        else {
            panic!("sequential claim must return sequential admission")
        };
        assert_eq!(mode, sequential_mode_tag());
        assert_eq!(evidence.kind(), TagKind::SequentialHistory);
        assert_eq!(covered_events, events.len());
        assert_eq!(verification.usage.total_injections, events.len());
        assert_eq!(
            history_usage,
            HistoryReplayUsage {
                receipt_count: 2,
                total_edits: 5,
                peak_occupied_bits: 2,
                peak_allocated_chunks: 2,
                world_tag_coordinate_visits: 4,
            }
        );

        assert!(matches!(
            admit_coordinate_history_mode_with_limits(
                &history,
                &initial_world,
                CoordinateHistoryModeClaim::SequentialHistory {
                    segments: std::slice::from_ref(&segment),
                },
                HistoryReplayLimits::new(1, 8, 8, 8, 8, 8),
                limits,
            ),
            Err(ModeAdmissionError::History(
                InMemoryHistoryVerificationError::Limit(
                    crate::history_store::HistoryReplayLimitError::ReceiptCount {
                        actual: 2,
                        max: 1,
                    }
                )
            ))
        ));

        let split_first = SequentialSegment::first(
            initial,
            4,
            events[..3]
                .iter()
                .enumerate()
                .map(|(index, &event)| SequentialInjection::new(index as u64, event).unwrap())
                .collect(),
            limits.segment_limits(),
        )
        .unwrap();
        let split_second = split_first
            .next(
                4,
                events[3..]
                    .iter()
                    .enumerate()
                    .map(|(index, &event)| {
                        SequentialInjection::new((index * 3) as u64, event).unwrap()
                    })
                    .collect(),
                limits.segment_limits(),
            )
            .unwrap();
        let split_chain = [split_first, split_second];
        let split_admission = admit_coordinate_history_mode(
            &history,
            &initial_world,
            CoordinateHistoryModeClaim::SequentialHistory {
                segments: &split_chain,
            },
            limits,
        )
        .unwrap();
        assert_ne!(split_admission, admitted);

        let wrong_anchor =
            sequential_initial_state(ContentTag::from_digest(TagKind::Receipt, [0xaa; 32]));
        let wrong_anchor_segment = SequentialSegment::first(
            wrong_anchor,
            8,
            events
                .iter()
                .enumerate()
                .map(|(index, &event)| SequentialInjection::new(index as u64, event).unwrap())
                .collect(),
            limits.segment_limits(),
        )
        .unwrap();
        assert!(matches!(
            admit_coordinate_history_mode(
                &history,
                &initial_world,
                CoordinateHistoryModeClaim::SequentialHistory {
                    segments: &[wrong_anchor_segment]
                },
                limits
            ),
            Err(ModeAdmissionError::Sequential(SequentialError::Semantic(
                SequentialSemanticError::InputMismatch {
                    segment_index: 0,
                    ..
                }
            )))
        ));

        let missing = SequentialSegment::first(
            initial,
            8,
            events[..4]
                .iter()
                .enumerate()
                .map(|(index, &event)| SequentialInjection::new(index as u64, event).unwrap())
                .collect(),
            limits.segment_limits(),
        )
        .unwrap();
        assert!(matches!(
            admit_coordinate_history_mode(
                &history,
                &initial_world,
                CoordinateHistoryModeClaim::SequentialHistory {
                    segments: &[missing]
                },
                limits
            ),
            Err(ModeAdmissionError::EventCoverage(
                EventCoverageError::CountMismatch {
                    required: 5,
                    actual: 4
                }
            ))
        ));

        let mut same_offset_events = events.clone();
        same_offset_events.swap(0, 1);
        let same_offset_swap = SequentialSegment::first(
            initial,
            8,
            same_offset_events
                .iter()
                .enumerate()
                .map(|(index, &event)| SequentialInjection::new((index / 2) as u64, event).unwrap())
                .collect(),
            limits.segment_limits(),
        )
        .unwrap();
        assert!(matches!(
            admit_coordinate_history_mode(
                &history,
                &initial_world,
                CoordinateHistoryModeClaim::SequentialHistory {
                    segments: &[same_offset_swap]
                },
                limits
            ),
            Err(ModeAdmissionError::EventCoverage(
                EventCoverageError::Mismatch { index: 0, .. }
            ))
        ));

        let mut reordered_events = events.clone();
        reordered_events.swap(1, 2);
        let reordered = SequentialSegment::first(
            initial,
            8,
            reordered_events
                .iter()
                .enumerate()
                .map(|(index, &event)| SequentialInjection::new(index as u64, event).unwrap())
                .collect(),
            limits.segment_limits(),
        )
        .unwrap();
        assert!(matches!(
            admit_coordinate_history_mode(
                &history,
                &initial_world,
                CoordinateHistoryModeClaim::SequentialHistory {
                    segments: &[reordered]
                },
                limits
            ),
            Err(ModeAdmissionError::EventCoverage(
                EventCoverageError::Mismatch { index: 1, .. }
            ))
        ));

        let shifted = SequentialSegment::first(
            initial,
            8,
            events
                .iter()
                .enumerate()
                .map(|(index, &event)| SequentialInjection::new((index + 2) as u64, event).unwrap())
                .collect(),
            limits.segment_limits(),
        )
        .unwrap();
        let shifted_admission = admit_coordinate_history_mode(
            &history,
            &initial_world,
            CoordinateHistoryModeClaim::SequentialHistory {
                segments: std::slice::from_ref(&shifted),
            },
            limits,
        )
        .unwrap();
        assert_ne!(shifted.tag(), segment.tag());
        assert_ne!(shifted_admission, admitted);

        assert_eq!(
            admit_coordinate_history_mode_with_limits(
                &history,
                &initial_world,
                CoordinateHistoryModeClaim::StakeConsensus,
                HistoryReplayLimits::new(0, 0, 0, 0, 0, 0),
                limits
            ),
            Err(ModeAdmissionError::StakeConsensusUndefined)
        );
    }

    #[test]
    fn empty_coordinate_history_accepts_only_a_zero_injection_sequential_chain() {
        let initial_world = World::new();
        let history = History::new(&initial_world);
        let initial = sequential_initial_state(history.genesis());
        let limits = SequentialLimits::new(1, 4, 4, 1, 1);
        let empty =
            SequentialSegment::first(initial, 1, Vec::new(), limits.segment_limits()).unwrap();
        assert!(
            admit_coordinate_history_mode(
                &history,
                &initial_world,
                CoordinateHistoryModeClaim::SequentialHistory { segments: &[empty] },
                limits
            )
            .is_ok()
        );

        let extra = SequentialSegment::first(
            initial,
            1,
            vec![SequentialInjection::new(0, event(0x20)).unwrap()],
            limits.segment_limits(),
        )
        .unwrap();
        assert!(matches!(
            admit_coordinate_history_mode(
                &history,
                &initial_world,
                CoordinateHistoryModeClaim::SequentialHistory { segments: &[extra] },
                limits
            ),
            Err(ModeAdmissionError::EventCoverage(
                EventCoverageError::CountMismatch {
                    required: 0,
                    actual: 1
                }
            ))
        ));
    }

    #[test]
    fn mode_admission_replays_history_before_trusting_cached_effects() {
        let mutable_a = Coord::new(10, 0, 0);
        let mutable_b = Coord::new(20, 0, 0);

        let mut initial_world = World::new();
        assert!(initial_world.set_on(mutable_a));
        let mut history = History::new(&initial_world);

        let mut unrelated_same_size_world = World::new();
        assert!(unrelated_same_size_world.set_on(mutable_b));
        let mut transaction = history.begin_tick(1);
        assert!(!transaction.apply_edit(
            &history,
            &mut unrelated_same_size_world,
            Edit::new(Operation::On, mutable_b),
        ));
        history
            .commit_tick(transaction, &unrelated_same_size_world)
            .unwrap();

        assert!(history.verify(&initial_world).is_err());
        assert!(matches!(
            admit_coordinate_history_mode(
                &history,
                &initial_world,
                CoordinateHistoryModeClaim::OpenLocal,
                SequentialLimits::DEFAULT,
            ),
            Err(ModeAdmissionError::History(_))
        ));

        assert_eq!(
            admit_coordinate_history_mode_with_limits(
                &history,
                &initial_world,
                CoordinateHistoryModeClaim::StakeConsensus,
                HistoryReplayLimits::new(0, 0, 0, 0, 0, 0),
                SequentialLimits::new(0, 0, 0, 0, 0),
            ),
            Err(ModeAdmissionError::StakeConsensusUndefined)
        );

        let (_, first, second) = kat_segments();
        assert!(matches!(
            admit_coordinate_history_mode_with_limits(
                &history,
                &initial_world,
                CoordinateHistoryModeClaim::SequentialHistory {
                    segments: &[first, second],
                },
                HistoryReplayLimits::DEFAULT,
                SequentialLimits::new(1, 4, 6, 3, 4),
            ),
            Err(ModeAdmissionError::Sequential(SequentialError::Limit(
                SequentialLimitError::SegmentCount { actual: 2, max: 1 }
            )))
        ));
    }

    #[test]
    fn constructors_reject_wrong_kinds_zero_work_and_out_of_order_offsets() {
        let wrong_event = ContentTag::from_digest(TagKind::World, [1; 32]);
        assert!(matches!(
            SequentialInjection::new(0, wrong_event),
            Err(SequentialSemanticError::EventKind {
                actual: TagKind::World,
                ..
            })
        ));
        let wrong_input = ContentTag::from_digest(TagKind::Receipt, [2; 32]);
        assert!(matches!(
            SequentialSegment::first(wrong_input, 1, Vec::new(), SequentialSegmentLimits::DEFAULT),
            Err(SequentialError::Semantic(
                SequentialSemanticError::StateKind {
                    actual: TagKind::Receipt,
                    ..
                }
            ))
        ));
        let initial = sequential_initial_state(wrong_input);
        assert!(matches!(
            SequentialSegment::first(initial, 0, Vec::new(), SequentialSegmentLimits::DEFAULT),
            Err(SequentialError::Codec(SequentialCodecError::ZeroIterations))
        ));
        assert!(matches!(
            SequentialSegment::first(
                initial,
                2,
                vec![
                    SequentialInjection::new(1, event(0x20)).unwrap(),
                    SequentialInjection::new(0, event(0x40)).unwrap(),
                ],
                SequentialSegmentLimits::DEFAULT
            ),
            Err(SequentialError::Codec(
                SequentialCodecError::InjectionOrder { index: 1, .. }
            ))
        ));
    }
}
