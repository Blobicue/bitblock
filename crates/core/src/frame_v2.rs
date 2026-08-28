//! Version-distinct frame receipts carrying a declared tick outcome identity.
//!
//! This module is additive: [`crate::frame::FrameReceipt`] remains the byte-identical v1
//! protocol. A v2 receipt replaces its unauthenticated raw source-tick lane with the exact
//! [`crate::tick::TickOutcome`] tag. Resolving that outcome and checking its selected frame
//! commitment is a separate admission step layered on this strict byte codec.

use std::fmt;

use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::tick::{TickFrameCommitment, TickOutcome, TickProtocolError, verify_successor};

pub const FRAME_RECEIPT_V2_SCHEMA: &[u8] = b"bitblock/frame-receipt/v2";
pub const FRAME_RECEIPT_V2_PAYLOAD_LEN: usize = 130;

/// Which outcome commitment the receipt declares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameStream {
    Game = 0,
    Composite = 1,
}

impl FrameStream {
    const fn code(self) -> u8 {
        self as u8
    }

    #[cfg(any(test, feature = "protocol-research"))]
    fn from_code(code: u8) -> Result<Self, FrameReceiptV2Error> {
        match code {
            0 => Ok(Self::Game),
            1 => Ok(Self::Composite),
            code => Err(FrameReceiptV2Error::UnknownStreamCode { code }),
        }
    }

    const fn select(self, outcome: &TickOutcome) -> Option<TickFrameCommitment> {
        match self {
            Self::Game => outcome.game(),
            Self::Composite => outcome.composite(),
        }
    }
}

/// Canonical v2 receipt declaring one frame pair and a tick-outcome identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameReceiptV2 {
    tag: ContentTag,
    parent: Option<ContentTag>,
    stream: FrameStream,
    outcome: ContentTag,
    input: ContentTag,
    frame: ContentTag,
}

impl FrameReceiptV2 {
    pub fn new(
        parent: Option<ContentTag>,
        stream: FrameStream,
        outcome: ContentTag,
        input: ContentTag,
        frame: ContentTag,
    ) -> Result<Self, FrameReceiptV2Error> {
        if let Some(parent) = parent {
            require_kind("frame-receipt-v2 parent", parent, TagKind::FrameReceipt)?;
        }
        require_kind("frame-receipt-v2 outcome", outcome, TagKind::TickOutcome)?;
        require_kind("frame-receipt-v2 input", input, TagKind::FrameInput)?;
        require_kind("frame-receipt-v2 frame", frame, TagKind::Frame)?;
        Ok(Self {
            tag: frame_receipt_v2_tag(parent, stream, outcome, input, frame),
            parent,
            stream,
            outcome,
            input,
            frame,
        })
    }

    pub const fn tag(self) -> ContentTag {
        self.tag
    }

    pub const fn parent(self) -> Option<ContentTag> {
        self.parent
    }

    pub const fn stream(self) -> FrameStream {
        self.stream
    }

    pub const fn outcome(self) -> ContentTag {
        self.outcome
    }

    pub const fn input(self) -> ContentTag {
        self.input
    }

    pub const fn frame(self) -> ContentTag {
        self.frame
    }

    pub fn encode_canonical(self) -> [u8; FRAME_RECEIPT_V2_PAYLOAD_LEN] {
        let mut bytes = [0_u8; FRAME_RECEIPT_V2_PAYLOAD_LEN];
        if let Some(parent) = self.parent {
            bytes[0] = 1;
            bytes[1..33].copy_from_slice(&parent.digest());
        }
        bytes[33] = self.stream.code();
        bytes[34..66].copy_from_slice(&self.outcome.digest());
        bytes[66..98].copy_from_slice(&self.input.digest());
        bytes[98..130].copy_from_slice(&self.frame.digest());
        bytes
    }

    #[cfg(any(test, feature = "protocol-research"))]
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, FrameReceiptV2Error> {
        if bytes.len() != FRAME_RECEIPT_V2_PAYLOAD_LEN {
            return Err(FrameReceiptV2Error::WrongPayloadLength {
                expected: FRAME_RECEIPT_V2_PAYLOAD_LEN,
                actual: bytes.len(),
            });
        }
        let parent = match bytes[0] {
            0 => {
                if bytes[1..33] != [0; 32] {
                    return Err(FrameReceiptV2Error::NonZeroAbsentParentPadding);
                }
                None
            }
            1 => Some(ContentTag::from_digest(
                TagKind::FrameReceipt,
                bytes[1..33].try_into().expect("fixed parent digest lane"),
            )),
            marker => return Err(FrameReceiptV2Error::UnknownParentMarker { marker }),
        };
        Self::new(
            parent,
            FrameStream::from_code(bytes[33])?,
            ContentTag::from_digest(
                TagKind::TickOutcome,
                bytes[34..66].try_into().expect("fixed outcome digest lane"),
            ),
            ContentTag::from_digest(
                TagKind::FrameInput,
                bytes[66..98].try_into().expect("fixed input digest lane"),
            ),
            ContentTag::from_digest(
                TagKind::Frame,
                bytes[98..130].try_into().expect("fixed frame digest lane"),
            ),
        )
    }

    pub fn verify(self) -> Result<(), FrameReceiptV2Error> {
        if let Some(parent) = self.parent {
            require_kind("frame-receipt-v2 parent", parent, TagKind::FrameReceipt)?;
        }
        require_kind(
            "frame-receipt-v2 outcome",
            self.outcome,
            TagKind::TickOutcome,
        )?;
        require_kind("frame-receipt-v2 input", self.input, TagKind::FrameInput)?;
        require_kind("frame-receipt-v2 frame", self.frame, TagKind::Frame)?;
        if self.tag != self.recompute_tag() {
            return Err(FrameReceiptV2Error::ReceiptTagMismatch);
        }
        Ok(())
    }

    /// Derives every non-parent field from one structurally verified outcome.
    pub fn from_outcome(
        parent: Option<ContentTag>,
        stream: FrameStream,
        outcome: &TickOutcome,
    ) -> Result<Self, FrameReceiptV2AdmissionError> {
        outcome
            .verify()
            .map_err(|source| FrameReceiptV2AdmissionError::InvalidOutcome { source })?;
        let commitment = stream.select(outcome).ok_or(
            FrameReceiptV2AdmissionError::MissingStreamCommitment {
                outcome: outcome.tag(),
                stream,
            },
        )?;
        Self::new(
            parent,
            stream,
            outcome.tag(),
            commitment.input(),
            commitment.frame(),
        )
        .map_err(|source| FrameReceiptV2AdmissionError::InvalidReceipt { source })
    }

    /// Verifies this declaration against the exact selected pair in a resolved outcome.
    pub fn verify_selected_outcome(
        self,
        outcome: &TickOutcome,
    ) -> Result<(), FrameReceiptV2AdmissionError> {
        self.verify()
            .map_err(|source| FrameReceiptV2AdmissionError::InvalidReceipt { source })?;
        outcome
            .verify()
            .map_err(|source| FrameReceiptV2AdmissionError::InvalidOutcome { source })?;
        if self.outcome != outcome.tag() {
            return Err(FrameReceiptV2AdmissionError::OutcomeMismatch {
                expected: self.outcome,
                actual: outcome.tag(),
            });
        }
        let commitment = self.stream.select(outcome).ok_or(
            FrameReceiptV2AdmissionError::MissingStreamCommitment {
                outcome: outcome.tag(),
                stream: self.stream,
            },
        )?;
        if self.input != commitment.input() {
            return Err(FrameReceiptV2AdmissionError::StreamInputMismatch {
                stream: self.stream,
                expected: commitment.input(),
                actual: self.input,
            });
        }
        if self.frame != commitment.frame() {
            return Err(FrameReceiptV2AdmissionError::StreamFrameMismatch {
                stream: self.stream,
                expected: commitment.frame(),
                actual: self.frame,
            });
        }
        Ok(())
    }

    fn recompute_tag(self) -> ContentTag {
        frame_receipt_v2_tag(
            self.parent,
            self.stream,
            self.outcome,
            self.input,
            self.frame,
        )
    }
}

fn frame_receipt_v2_tag(
    parent: Option<ContentTag>,
    stream: FrameStream,
    outcome: ContentTag,
    input: ContentTag,
    frame: ContentTag,
) -> ContentTag {
    hash_canonical(
        TagKind::FrameReceipt,
        FRAME_RECEIPT_V2_SCHEMA,
        FRAME_RECEIPT_V2_PAYLOAD_LEN as u64,
        |sink| {
            match parent {
                Some(parent) => {
                    sink(&[1]);
                    sink(&parent.digest());
                }
                None => {
                    sink(&[0]);
                    sink(&[0; 32]);
                }
            }
            sink(&[stream.code()]);
            sink(&outcome.digest());
            sink(&input.digest());
            sink(&frame.digest());
        },
    )
    .expect("frame-receipt-v2 encoding has a fixed length")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameReceiptV2Error {
    WrongKind {
        field: &'static str,
        expected: TagKind,
        actual: TagKind,
    },
    WrongPayloadLength {
        expected: usize,
        actual: usize,
    },
    UnknownParentMarker {
        marker: u8,
    },
    UnknownStreamCode {
        code: u8,
    },
    NonZeroAbsentParentPadding,
    ReceiptTagMismatch,
}

impl fmt::Display for FrameReceiptV2Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::WrongKind {
                field,
                expected,
                actual,
            } => write!(
                formatter,
                "{field} requires a {} tag, received {}",
                expected.name(),
                actual.name()
            ),
            Self::WrongPayloadLength { expected, actual } => write!(
                formatter,
                "frame-receipt-v2 requires {expected} bytes, found {actual}"
            ),
            Self::UnknownParentMarker { marker } => {
                write!(formatter, "frame-receipt-v2 uses parent marker {marker}")
            }
            Self::UnknownStreamCode { code } => {
                write!(formatter, "frame-receipt-v2 uses stream code {code}")
            }
            Self::NonZeroAbsentParentPadding => {
                formatter.write_str("absent frame-receipt-v2 parent has nonzero canonical padding")
            }
            Self::ReceiptTagMismatch => {
                formatter.write_str("frame-receipt-v2 tag does not match its canonical fields")
            }
        }
    }
}

impl std::error::Error for FrameReceiptV2Error {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrameReceiptV2AdmissionError {
    InvalidReceipt {
        source: FrameReceiptV2Error,
    },
    InvalidOutcome {
        source: TickProtocolError,
    },
    OutcomeMismatch {
        expected: ContentTag,
        actual: ContentTag,
    },
    MissingStreamCommitment {
        outcome: ContentTag,
        stream: FrameStream,
    },
    StreamInputMismatch {
        stream: FrameStream,
        expected: ContentTag,
        actual: ContentTag,
    },
    StreamFrameMismatch {
        stream: FrameStream,
        expected: ContentTag,
        actual: ContentTag,
    },
}

impl fmt::Display for FrameReceiptV2AdmissionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidReceipt { source } => {
                write!(formatter, "invalid frame receipt v2: {source}")
            }
            Self::InvalidOutcome { source } => write!(formatter, "invalid tick outcome: {source}"),
            Self::OutcomeMismatch { expected, actual } => write!(
                formatter,
                "frame receipt v2 declares outcome {expected}, resolved {actual}"
            ),
            Self::MissingStreamCommitment { outcome, stream } => write!(
                formatter,
                "tick outcome {outcome} has no {:?} frame commitment",
                stream
            ),
            Self::StreamInputMismatch {
                stream,
                expected,
                actual,
            } => write!(
                formatter,
                "{:?} input commitment mismatch: expected {expected}, received {actual}",
                stream
            ),
            Self::StreamFrameMismatch {
                stream,
                expected,
                actual,
            } => write!(
                formatter,
                "{:?} frame commitment mismatch: expected {expected}, received {actual}",
                stream
            ),
        }
    }
}

impl std::error::Error for FrameReceiptV2AdmissionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidReceipt { source } => Some(source),
            Self::InvalidOutcome { source } => Some(source),
            Self::OutcomeMismatch { .. }
            | Self::MissingStreamCommitment { .. }
            | Self::StreamInputMismatch { .. }
            | Self::StreamFrameMismatch { .. } => None,
        }
    }
}

/// Local work limits for one frame admission path.
///
/// These bounds do not make a larger path noncanonical; they let a bounded verifier refuse work
/// before retaining it. Cumulative ledger growth remains explicit and unbounded by this policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameLedgerV2Limits {
    pub max_successor_outcomes: usize,
    pub max_successor_attempts: usize,
}

impl Default for FrameLedgerV2Limits {
    fn default() -> Self {
        Self {
            max_successor_outcomes: 1_000_000,
            max_successor_attempts: 1_000_000,
        }
    }
}

/// One admitted v2 receipt plus every strict outcome successor used to prove its ancestry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameAdmissionV2 {
    receipt: FrameReceiptV2,
    successors: Box<[TickOutcome]>,
}

impl FrameAdmissionV2 {
    pub const fn receipt(&self) -> FrameReceiptV2 {
        self.receipt
    }

    pub fn successors(&self) -> &[TickOutcome] {
        &self.successors
    }
}

/// A v2-only, fixed-stream ledger rooted at one structurally verified tick-zero outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameLedgerV2 {
    stream: FrameStream,
    genesis: TickOutcome,
    limits: FrameLedgerV2Limits,
    admissions: Vec<FrameAdmissionV2>,
}

impl FrameLedgerV2 {
    pub fn new_structural_genesis(
        stream: FrameStream,
        genesis: TickOutcome,
    ) -> Result<Self, FrameLedgerV2Error> {
        Self::with_limits(stream, genesis, FrameLedgerV2Limits::default())
    }

    pub fn with_limits(
        stream: FrameStream,
        genesis: TickOutcome,
        limits: FrameLedgerV2Limits,
    ) -> Result<Self, FrameLedgerV2Error> {
        verify_successor(None, &genesis)
            .map_err(|source| FrameLedgerV2Error::InvalidGenesis { source })?;
        Ok(Self {
            stream,
            genesis,
            limits,
            admissions: Vec::new(),
        })
    }

    pub const fn stream(&self) -> FrameStream {
        self.stream
    }

    pub const fn limits(&self) -> FrameLedgerV2Limits {
        self.limits
    }

    pub fn genesis(&self) -> &TickOutcome {
        &self.genesis
    }

    pub fn admissions(&self) -> &[FrameAdmissionV2] {
        &self.admissions
    }

    pub fn len(&self) -> usize {
        self.admissions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.admissions.is_empty()
    }

    pub fn head(&self) -> Option<ContentTag> {
        self.admissions
            .last()
            .map(|admission| admission.receipt.tag())
    }

    pub fn outcome_head(&self) -> &TickOutcome {
        self.admissions
            .last()
            .and_then(|admission| admission.successors.last())
            .unwrap_or(&self.genesis)
    }

    /// Derives a receipt from the verified path endpoint, then admits both atomically.
    pub fn admit_derived(
        &mut self,
        successors: Box<[TickOutcome]>,
    ) -> Result<&FrameAdmissionV2, FrameLedgerV2Error> {
        let prepared = self.prepare_derived(successors)?;
        self.reserve_one()?;
        Ok(self.push_prepared(prepared))
    }

    fn prepare_derived(
        &self,
        successors: Box<[TickOutcome]>,
    ) -> Result<FrameAdmissionV2, FrameLedgerV2Error> {
        let admission = self.admissions.len();
        self.validate_path_limits(admission, &successors)?;
        let endpoint = verify_successor_path(
            self.outcome_head(),
            !self.admissions.is_empty(),
            admission,
            &successors,
        )?;
        let receipt = FrameReceiptV2::from_outcome(self.head(), self.stream, endpoint)
            .map_err(|source| FrameLedgerV2Error::OutcomeBinding { source })?;
        Ok(FrameAdmissionV2 {
            receipt,
            successors,
        })
    }

    /// Admits an externally decoded v2 receipt only after replaying its complete successor path.
    pub fn admit_existing(
        &mut self,
        receipt: FrameReceiptV2,
        successors: Box<[TickOutcome]>,
    ) -> Result<&FrameAdmissionV2, FrameLedgerV2Error> {
        let prepared = self.prepare_existing(receipt, successors)?;
        self.reserve_one()?;
        Ok(self.push_prepared(prepared))
    }

    fn prepare_existing(
        &self,
        receipt: FrameReceiptV2,
        successors: Box<[TickOutcome]>,
    ) -> Result<FrameAdmissionV2, FrameLedgerV2Error> {
        receipt
            .verify()
            .map_err(|source| FrameLedgerV2Error::InvalidReceipt { source })?;
        if receipt.parent() != self.head() {
            return Err(FrameLedgerV2Error::ParentMismatch {
                expected: self.head(),
                actual: receipt.parent(),
            });
        }
        if receipt.stream() != self.stream {
            return Err(FrameLedgerV2Error::StreamMismatch {
                expected: self.stream,
                actual: receipt.stream(),
            });
        }

        let admission = self.admissions.len();
        self.validate_path_limits(admission, &successors)?;
        let endpoint = verify_successor_path(
            self.outcome_head(),
            !self.admissions.is_empty(),
            admission,
            &successors,
        )?;
        receipt
            .verify_selected_outcome(endpoint)
            .map_err(|source| FrameLedgerV2Error::OutcomeBinding { source })?;
        Ok(FrameAdmissionV2 {
            receipt,
            successors,
        })
    }

    /// Replays the structural genesis, every stored successor edge, receipt link, and stream pair.
    pub fn verify(&self) -> Result<(), FrameLedgerV2Error> {
        verify_successor(None, &self.genesis)
            .map_err(|source| FrameLedgerV2Error::InvalidGenesis { source })?;
        let mut outcome = &self.genesis;
        let mut receipt_head = None;
        for (index, admission) in self.admissions.iter().enumerate() {
            admission
                .receipt
                .verify()
                .map_err(|source| FrameLedgerV2Error::InvalidReceipt { source })?;
            if admission.receipt.parent() != receipt_head {
                return Err(FrameLedgerV2Error::ParentMismatch {
                    expected: receipt_head,
                    actual: admission.receipt.parent(),
                });
            }
            if admission.receipt.stream() != self.stream {
                return Err(FrameLedgerV2Error::StreamMismatch {
                    expected: self.stream,
                    actual: admission.receipt.stream(),
                });
            }
            self.validate_path_limits(index, &admission.successors)?;
            outcome = verify_successor_path(outcome, index != 0, index, &admission.successors)?;
            admission
                .receipt
                .verify_selected_outcome(outcome)
                .map_err(|source| FrameLedgerV2Error::OutcomeBinding { source })?;
            receipt_head = Some(admission.receipt.tag());
        }
        Ok(())
    }

    fn validate_path_limits(
        &self,
        admission: usize,
        successors: &[TickOutcome],
    ) -> Result<(), FrameLedgerV2Error> {
        if successors.len() > self.limits.max_successor_outcomes {
            return Err(FrameLedgerV2Error::PathLimitExceeded {
                admission,
                resource: "successor outcomes",
                limit: self.limits.max_successor_outcomes,
                actual: successors.len(),
            });
        }
        let attempts = successors.iter().try_fold(0_usize, |total, outcome| {
            total.checked_add(outcome.attempts().len())
        });
        let Some(attempts) = attempts else {
            return Err(FrameLedgerV2Error::PathAttemptCountOverflow { admission });
        };
        if attempts > self.limits.max_successor_attempts {
            return Err(FrameLedgerV2Error::PathLimitExceeded {
                admission,
                resource: "successor attempts",
                limit: self.limits.max_successor_attempts,
                actual: attempts,
            });
        }
        Ok(())
    }

    fn reserve_one(&mut self) -> Result<(), FrameLedgerV2Error> {
        self.admissions
            .try_reserve(1)
            .map_err(|_| FrameLedgerV2Error::AdmissionAllocationFailed)?;
        Ok(())
    }

    fn push_prepared(&mut self, prepared: FrameAdmissionV2) -> &FrameAdmissionV2 {
        debug_assert!(self.admissions.len() < self.admissions.capacity());
        self.admissions.push(prepared);
        self.admissions
            .last()
            .expect("one prepared admission was pushed after successful reservation")
    }
}

/// The exact receipt identities prepared and published by one paired frame submission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameReceiptPairV2 {
    game: Option<FrameReceiptV2>,
    composite: FrameReceiptV2,
}

impl FrameReceiptPairV2 {
    pub const fn game(self) -> Option<FrameReceiptV2> {
        self.game
    }

    pub const fn composite(self) -> FrameReceiptV2 {
        self.composite
    }
}

/// Validates and publishes an optional game plus required composite admission atomically.
///
/// Both paths and both ledger roles are checked before either ledger is mutated or `submit` is
/// called. Storage for both admissions is reserved next. A backend refusal leaves the logical
/// ledgers unchanged; after backend success, the already prepared admissions are pushed without
/// another fallible operation.
pub fn admit_frame_pair_derived<E>(
    game: Option<(&mut FrameLedgerV2, Box<[TickOutcome]>)>,
    composite: (&mut FrameLedgerV2, Box<[TickOutcome]>),
    submit: impl FnOnce(FrameReceiptPairV2) -> Result<(), E>,
) -> Result<FrameReceiptPairV2, FramePairAdmissionV2Error<E>> {
    let (composite_ledger, composite_successors) = composite;
    if composite_ledger.stream() != FrameStream::Composite {
        return Err(FramePairAdmissionV2Error::Composite {
            source: FrameLedgerV2Error::StreamMismatch {
                expected: FrameStream::Composite,
                actual: composite_ledger.stream(),
            },
        });
    }

    let (mut game_ledger, game_prepared) = match game {
        Some((ledger, successors)) => {
            if ledger.stream() != FrameStream::Game {
                return Err(FramePairAdmissionV2Error::Game {
                    source: FrameLedgerV2Error::StreamMismatch {
                        expected: FrameStream::Game,
                        actual: ledger.stream(),
                    },
                });
            }
            let prepared = ledger
                .prepare_derived(successors)
                .map_err(|source| FramePairAdmissionV2Error::Game { source })?;
            (Some(ledger), Some(prepared))
        }
        None => (None, None),
    };
    let composite_prepared = composite_ledger
        .prepare_derived(composite_successors)
        .map_err(|source| FramePairAdmissionV2Error::Composite { source })?;

    if let Some(ledger) = game_ledger.as_deref_mut() {
        ledger
            .reserve_one()
            .map_err(|source| FramePairAdmissionV2Error::Game { source })?;
    }
    composite_ledger
        .reserve_one()
        .map_err(|source| FramePairAdmissionV2Error::Composite { source })?;

    let receipts = FrameReceiptPairV2 {
        game: game_prepared.as_ref().map(FrameAdmissionV2::receipt),
        composite: composite_prepared.receipt(),
    };
    submit(receipts).map_err(FramePairAdmissionV2Error::Backend)?;

    if let (Some(ledger), Some(prepared)) = (game_ledger, game_prepared) {
        ledger.push_prepared(prepared);
    }
    composite_ledger.push_prepared(composite_prepared);
    Ok(receipts)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FramePairAdmissionV2Error<E> {
    Game { source: FrameLedgerV2Error },
    Composite { source: FrameLedgerV2Error },
    Backend(E),
}

impl<E: fmt::Display> fmt::Display for FramePairAdmissionV2Error<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Game { source } => write!(formatter, "game-frame admission failed: {source}"),
            Self::Composite { source } => {
                write!(formatter, "composite-frame admission failed: {source}")
            }
            Self::Backend(source) => write!(formatter, "frame backend submission failed: {source}"),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for FramePairAdmissionV2Error<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Game { source } | Self::Composite { source } => Some(source),
            Self::Backend(source) => Some(source),
        }
    }
}

fn verify_successor_path<'a>(
    current: &'a TickOutcome,
    require_advance: bool,
    admission: usize,
    successors: &'a [TickOutcome],
) -> Result<&'a TickOutcome, FrameLedgerV2Error> {
    if require_advance && successors.is_empty() {
        return Err(FrameLedgerV2Error::NonAdvancingOutcome {
            admission,
            outcome: current.tag(),
        });
    }
    let mut previous = current;
    for (edge, next) in successors.iter().enumerate() {
        verify_successor(Some(previous), next).map_err(|source| {
            FrameLedgerV2Error::InvalidAncestry {
                admission,
                edge,
                source,
            }
        })?;
        previous = next;
    }
    Ok(previous)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrameLedgerV2Error {
    InvalidGenesis {
        source: TickProtocolError,
    },
    InvalidReceipt {
        source: FrameReceiptV2Error,
    },
    ParentMismatch {
        expected: Option<ContentTag>,
        actual: Option<ContentTag>,
    },
    StreamMismatch {
        expected: FrameStream,
        actual: FrameStream,
    },
    NonAdvancingOutcome {
        admission: usize,
        outcome: ContentTag,
    },
    InvalidAncestry {
        admission: usize,
        edge: usize,
        source: TickProtocolError,
    },
    OutcomeBinding {
        source: FrameReceiptV2AdmissionError,
    },
    PathLimitExceeded {
        admission: usize,
        resource: &'static str,
        limit: usize,
        actual: usize,
    },
    PathAttemptCountOverflow {
        admission: usize,
    },
    AdmissionAllocationFailed,
}

impl fmt::Display for FrameLedgerV2Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidGenesis { source } => {
                write!(formatter, "invalid structural tick genesis: {source}")
            }
            Self::InvalidReceipt { source } => write!(formatter, "invalid v2 receipt: {source}"),
            Self::ParentMismatch { expected, actual } => write!(
                formatter,
                "v2 receipt parent mismatch: expected {expected:?}, received {actual:?}"
            ),
            Self::StreamMismatch { expected, actual } => write!(
                formatter,
                "v2 ledger stream mismatch: expected {expected:?}, received {actual:?}"
            ),
            Self::NonAdvancingOutcome { admission, outcome } => write!(
                formatter,
                "v2 admission {admission} reuses outcome {outcome} without a successor"
            ),
            Self::InvalidAncestry {
                admission,
                edge,
                source,
            } => write!(
                formatter,
                "v2 admission {admission} has invalid outcome edge {edge}: {source}"
            ),
            Self::OutcomeBinding { source } => {
                write!(formatter, "v2 outcome binding failed: {source}")
            }
            Self::PathLimitExceeded {
                admission,
                resource,
                limit,
                actual,
            } => write!(
                formatter,
                "v2 admission {admission} exceeds {resource} limit {limit} with {actual}"
            ),
            Self::PathAttemptCountOverflow { admission } => {
                write!(
                    formatter,
                    "v2 admission {admission} attempt count overflowed"
                )
            }
            Self::AdmissionAllocationFailed => {
                formatter.write_str("v2 admission storage allocation failed")
            }
        }
    }
}

impl std::error::Error for FrameLedgerV2Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidGenesis { source } | Self::InvalidAncestry { source, .. } => Some(source),
            Self::InvalidReceipt { source } => Some(source),
            Self::OutcomeBinding { source } => Some(source),
            Self::ParentMismatch { .. }
            | Self::StreamMismatch { .. }
            | Self::NonAdvancingOutcome { .. }
            | Self::PathLimitExceeded { .. }
            | Self::PathAttemptCountOverflow { .. }
            | Self::AdmissionAllocationFailed => None,
        }
    }
}

fn require_kind(
    field: &'static str,
    tag: ContentTag,
    expected: TagKind,
) -> Result<(), FrameReceiptV2Error> {
    if tag.kind() != expected {
        return Err(FrameReceiptV2Error::WrongKind {
            field,
            expected,
            actual: tag.kind(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::FrameReceipt;
    use crate::tick::{TickAttempt, TickCameraState, TickInputFlags, TickInputState};
    use sha2::{Digest, Sha256};

    fn tag(kind: TagKind, byte: u8) -> ContentTag {
        ContentTag::from_digest(kind, [byte; 32])
    }

    fn commitment(input: u8, frame: u8) -> TickFrameCommitment {
        TickFrameCommitment::new(tag(TagKind::FrameInput, input), tag(TagKind::Frame, frame))
            .unwrap()
    }

    fn framed_outcome(
        game: Option<TickFrameCommitment>,
        composite: Option<TickFrameCommitment>,
    ) -> TickOutcome {
        outcome_after(None, game, composite, [])
    }

    fn outcome_after(
        previous: Option<&TickOutcome>,
        game: Option<TickFrameCommitment>,
        composite: Option<TickFrameCommitment>,
        attempts: impl IntoIterator<Item = TickAttempt>,
    ) -> TickOutcome {
        let input = TickInputState::new(
            TickInputFlags::new(
                false,
                false,
                false,
                false,
                false,
                false,
                false,
                game.is_some(),
                game.is_none() && composite.is_some(),
                true,
                false,
            ),
            0.0,
            0.0,
            0.0,
            1,
            1,
            2,
            1,
        )
        .unwrap();
        let camera = TickCameraState::from_bits([
            0.0_f64.to_bits(),
            0.0_f64.to_bits(),
            (-1.0_f64).to_bits(),
            0.0_f64.to_bits(),
            0.0_f64.to_bits(),
            0.0_f64.to_bits(),
            0.0_f64.to_bits(),
            0.0_f64.to_bits(),
            1.0_f64.to_bits(),
            1.0_f64.to_bits(),
        ])
        .unwrap();
        let parent = previous.map(TickOutcome::tag);
        let tick = previous.map_or(0, |outcome| outcome.tick() + 1);
        let pre_world = previous.map_or(tag(TagKind::World, 0x10), TickOutcome::post_world);
        let pre_history = previous.map_or(tag(TagKind::Receipt, 0x20), TickOutcome::post_history);
        let post_camera = previous.map_or(camera, TickOutcome::post_camera);
        TickOutcome::with_edit_attempts_only(
            parent,
            tick,
            pre_world,
            pre_history,
            input,
            attempts,
            pre_world,
            pre_history,
            post_camera,
            game,
            composite,
        )
        .unwrap()
    }

    #[test]
    fn codec_round_trips_both_parent_shapes_and_refuses_noncanonical_bytes() {
        let first = FrameReceiptV2::new(
            None,
            FrameStream::Game,
            tag(TagKind::TickOutcome, 1),
            tag(TagKind::FrameInput, 2),
            tag(TagKind::Frame, 3),
        )
        .unwrap();
        let second = FrameReceiptV2::new(
            Some(first.tag()),
            FrameStream::Composite,
            tag(TagKind::TickOutcome, 4),
            tag(TagKind::FrameInput, 5),
            tag(TagKind::Frame, 6),
        )
        .unwrap();
        for receipt in [first, second] {
            let bytes = receipt.encode_canonical();
            assert_eq!(FrameReceiptV2::decode_canonical(&bytes), Ok(receipt));
            assert_eq!(receipt.verify(), Ok(()));
        }

        let mut padding = first.encode_canonical();
        padding[1] = 1;
        assert_eq!(
            FrameReceiptV2::decode_canonical(&padding),
            Err(FrameReceiptV2Error::NonZeroAbsentParentPadding)
        );
        let mut marker = first.encode_canonical();
        marker[0] = 2;
        assert_eq!(
            FrameReceiptV2::decode_canonical(&marker),
            Err(FrameReceiptV2Error::UnknownParentMarker { marker: 2 })
        );
        let mut stream = first.encode_canonical();
        stream[33] = 2;
        assert_eq!(
            FrameReceiptV2::decode_canonical(&stream),
            Err(FrameReceiptV2Error::UnknownStreamCode { code: 2 })
        );
        assert!(matches!(
            FrameReceiptV2::decode_canonical(&first.encode_canonical()[..129]),
            Err(FrameReceiptV2Error::WrongPayloadLength { .. })
        ));
    }

    #[test]
    fn nested_kinds_and_tampered_cached_tag_fail_closed() {
        assert!(matches!(
            FrameReceiptV2::new(
                None,
                FrameStream::Game,
                tag(TagKind::World, 1),
                tag(TagKind::FrameInput, 2),
                tag(TagKind::Frame, 3),
            ),
            Err(FrameReceiptV2Error::WrongKind {
                expected: TagKind::TickOutcome,
                actual: TagKind::World,
                ..
            })
        ));

        let mut receipt = FrameReceiptV2::new(
            None,
            FrameStream::Game,
            tag(TagKind::TickOutcome, 1),
            tag(TagKind::FrameInput, 2),
            tag(TagKind::Frame, 3),
        )
        .unwrap();
        receipt.tag = FrameReceiptV2::new(
            None,
            FrameStream::Game,
            tag(TagKind::TickOutcome, 9),
            tag(TagKind::FrameInput, 2),
            tag(TagKind::Frame, 3),
        )
        .unwrap()
        .tag();
        assert_eq!(
            receipt.verify(),
            Err(FrameReceiptV2Error::ReceiptTagMismatch)
        );
    }

    #[test]
    fn selected_outcome_binding_derives_and_verifies_both_streams() {
        let game = commitment(0x31, 0x41);
        let composite = commitment(0x51, 0x61);
        let outcome = framed_outcome(Some(game), Some(composite));

        for (stream, expected) in [
            (FrameStream::Game, game),
            (FrameStream::Composite, composite),
        ] {
            let receipt = FrameReceiptV2::from_outcome(None, stream, &outcome).unwrap();
            assert_eq!(receipt.outcome(), outcome.tag());
            assert_eq!(receipt.input(), expected.input());
            assert_eq!(receipt.frame(), expected.frame());
            assert_eq!(receipt.verify_selected_outcome(&outcome), Ok(()));
        }
    }

    #[test]
    fn selected_outcome_binding_refuses_missing_mismatched_and_wrong_outcomes() {
        let game = commitment(0x31, 0x41);
        let composite = commitment(0x51, 0x61);
        let outcome = framed_outcome(Some(game), Some(composite));
        let sidebar_only = framed_outcome(None, Some(composite));

        assert!(matches!(
            FrameReceiptV2::from_outcome(None, FrameStream::Game, &sidebar_only),
            Err(FrameReceiptV2AdmissionError::MissingStreamCommitment {
                stream: FrameStream::Game,
                ..
            })
        ));

        let stream_swap = FrameReceiptV2::new(
            None,
            FrameStream::Game,
            outcome.tag(),
            composite.input(),
            composite.frame(),
        )
        .unwrap();
        assert!(matches!(
            stream_swap.verify_selected_outcome(&outcome),
            Err(FrameReceiptV2AdmissionError::StreamInputMismatch {
                stream: FrameStream::Game,
                ..
            })
        ));

        let input_only = FrameReceiptV2::new(
            None,
            FrameStream::Game,
            outcome.tag(),
            composite.input(),
            game.frame(),
        )
        .unwrap();
        assert!(matches!(
            input_only.verify_selected_outcome(&outcome),
            Err(FrameReceiptV2AdmissionError::StreamInputMismatch { .. })
        ));
        let frame_only = FrameReceiptV2::new(
            None,
            FrameStream::Game,
            outcome.tag(),
            game.input(),
            composite.frame(),
        )
        .unwrap();
        assert!(matches!(
            frame_only.verify_selected_outcome(&outcome),
            Err(FrameReceiptV2AdmissionError::StreamFrameMismatch { .. })
        ));

        let other = framed_outcome(Some(commitment(0x71, 0x81)), Some(composite));
        let receipt = FrameReceiptV2::from_outcome(None, FrameStream::Game, &outcome).unwrap();
        assert!(matches!(
            receipt.verify_selected_outcome(&other),
            Err(FrameReceiptV2AdmissionError::OutcomeMismatch { .. })
        ));

        let mut tampered = receipt;
        tampered.tag = stream_swap.tag();
        assert_eq!(
            tampered.verify_selected_outcome(&outcome),
            Err(FrameReceiptV2AdmissionError::InvalidReceipt {
                source: FrameReceiptV2Error::ReceiptTagMismatch,
            })
        );
    }

    #[test]
    fn ledger_admits_anchor_and_complete_skipped_outcome_paths_then_reverifies() {
        let genesis = framed_outcome(Some(commitment(0x10, 0x11)), Some(commitment(0x12, 0x13)));
        let mut ledger =
            FrameLedgerV2::new_structural_genesis(FrameStream::Game, genesis.clone()).unwrap();

        let first = ledger.admit_derived(Box::new([])).unwrap().receipt();
        assert_eq!(first.parent(), None);
        assert_eq!(first.outcome(), genesis.tag());

        let idle_one = outcome_after(Some(&genesis), None, None, []);
        let idle_two = outcome_after(Some(&idle_one), None, None, []);
        let rendered = outcome_after(
            Some(&idle_two),
            Some(commitment(0x20, 0x21)),
            Some(commitment(0x22, 0x23)),
            [],
        );
        let second = ledger
            .admit_derived(
                vec![idle_one.clone(), idle_two.clone(), rendered.clone()].into_boxed_slice(),
            )
            .unwrap()
            .receipt();

        assert_eq!(second.parent(), Some(first.tag()));
        assert_eq!(second.outcome(), rendered.tag());
        assert_eq!(ledger.len(), 2);
        assert_eq!(ledger.outcome_head(), &rendered);
        assert_eq!(ledger.admissions()[1].successors().len(), 3);
        assert_eq!(ledger.verify(), Ok(()));
    }

    #[test]
    fn ledger_refuses_missing_reordered_and_forked_edges_without_mutation() {
        let genesis = framed_outcome(Some(commitment(0x10, 0x11)), Some(commitment(0x12, 0x13)));
        let mut ledger =
            FrameLedgerV2::new_structural_genesis(FrameStream::Game, genesis.clone()).unwrap();
        ledger.admit_derived(Box::new([])).unwrap();

        let idle_one = outcome_after(Some(&genesis), None, None, []);
        let idle_two = outcome_after(Some(&idle_one), None, None, []);
        let rendered = outcome_after(
            Some(&idle_two),
            Some(commitment(0x20, 0x21)),
            Some(commitment(0x22, 0x23)),
            [],
        );
        for path in [
            vec![idle_two.clone(), rendered.clone()],
            vec![idle_one.clone(), rendered.clone()],
            vec![idle_two.clone(), idle_one.clone(), rendered.clone()],
        ] {
            let before = ledger.clone();
            assert!(matches!(
                ledger.admit_derived(path.into_boxed_slice()),
                Err(FrameLedgerV2Error::InvalidAncestry { .. })
            ));
            assert_eq!(ledger, before);
        }

        let main = outcome_after(
            Some(&genesis),
            Some(commitment(0x30, 0x31)),
            Some(commitment(0x32, 0x33)),
            [],
        );
        ledger
            .admit_derived(vec![main.clone()].into_boxed_slice())
            .unwrap();
        let fork = outcome_after(
            Some(&genesis),
            Some(commitment(0x40, 0x41)),
            Some(commitment(0x42, 0x43)),
            [],
        );
        let fork_child = outcome_after(
            Some(&fork),
            Some(commitment(0x50, 0x51)),
            Some(commitment(0x52, 0x53)),
            [],
        );
        let fork_receipt =
            FrameReceiptV2::from_outcome(ledger.head(), FrameStream::Game, &fork_child).unwrap();
        let before = ledger.clone();
        assert!(matches!(
            ledger.admit_existing(fork_receipt, vec![fork_child].into_boxed_slice()),
            Err(FrameLedgerV2Error::InvalidAncestry { .. })
        ));
        assert_eq!(ledger, before);
    }

    #[test]
    fn ledger_refuses_parent_stream_pair_and_nonadvancing_faults_atomically() {
        let game = commitment(0x10, 0x11);
        let composite = commitment(0x12, 0x13);
        let genesis = framed_outcome(Some(game), Some(composite));
        let mut ledger =
            FrameLedgerV2::new_structural_genesis(FrameStream::Game, genesis.clone()).unwrap();

        let v1_parent = FrameReceipt::new(None, 0, game.input(), game.frame())
            .unwrap()
            .tag();
        let wrong_parent =
            FrameReceiptV2::from_outcome(Some(v1_parent), FrameStream::Game, &genesis).unwrap();
        let wrong_stream =
            FrameReceiptV2::from_outcome(None, FrameStream::Composite, &genesis).unwrap();
        let wrong_pair = FrameReceiptV2::new(
            None,
            FrameStream::Game,
            genesis.tag(),
            composite.input(),
            composite.frame(),
        )
        .unwrap();

        for (receipt, expected) in [
            (wrong_parent, "parent"),
            (wrong_stream, "stream"),
            (wrong_pair, "pair"),
        ] {
            let before = ledger.clone();
            let error = ledger.admit_existing(receipt, Box::new([])).unwrap_err();
            match expected {
                "parent" => assert!(matches!(error, FrameLedgerV2Error::ParentMismatch { .. })),
                "stream" => assert!(matches!(error, FrameLedgerV2Error::StreamMismatch { .. })),
                "pair" => assert!(matches!(
                    error,
                    FrameLedgerV2Error::OutcomeBinding {
                        source: FrameReceiptV2AdmissionError::StreamInputMismatch { .. }
                    }
                )),
                _ => unreachable!(),
            }
            assert_eq!(ledger, before);
        }

        ledger.admit_derived(Box::new([])).unwrap();
        let before = ledger.clone();
        assert!(matches!(
            ledger.admit_derived(Box::new([])),
            Err(FrameLedgerV2Error::NonAdvancingOutcome { .. })
        ));
        assert_eq!(ledger, before);

        let sidebar_only = framed_outcome(None, Some(composite));
        let mut missing =
            FrameLedgerV2::new_structural_genesis(FrameStream::Game, sidebar_only.clone()).unwrap();
        let declaration = FrameReceiptV2::new(
            None,
            FrameStream::Game,
            sidebar_only.tag(),
            game.input(),
            game.frame(),
        )
        .unwrap();
        let before = missing.clone();
        assert!(matches!(
            missing.admit_existing(declaration, Box::new([])),
            Err(FrameLedgerV2Error::OutcomeBinding {
                source: FrameReceiptV2AdmissionError::MissingStreamCommitment { .. }
            })
        ));
        assert_eq!(missing, before);
    }

    #[test]
    fn ledger_limits_and_stored_evidence_fail_closed() {
        let genesis = framed_outcome(Some(commitment(0x10, 0x11)), Some(commitment(0x12, 0x13)));
        let limits = FrameLedgerV2Limits {
            max_successor_outcomes: 1,
            max_successor_attempts: 0,
        };
        let mut ledger =
            FrameLedgerV2::with_limits(FrameStream::Game, genesis.clone(), limits).unwrap();
        ledger.admit_derived(Box::new([])).unwrap();

        let idle_one = outcome_after(Some(&genesis), None, None, []);
        let idle_two = outcome_after(Some(&idle_one), None, None, []);
        let before = ledger.clone();
        assert_eq!(
            ledger.admit_derived(vec![idle_one.clone(), idle_two].into_boxed_slice()),
            Err(FrameLedgerV2Error::PathLimitExceeded {
                admission: 1,
                resource: "successor outcomes",
                limit: 1,
                actual: 2,
            })
        );
        assert_eq!(ledger, before);

        let attempted = outcome_after(
            Some(&genesis),
            None,
            None,
            [TickAttempt::AimDeleteUnresolved],
        );
        let before = ledger.clone();
        assert_eq!(
            ledger.admit_derived(vec![attempted].into_boxed_slice()),
            Err(FrameLedgerV2Error::PathLimitExceeded {
                admission: 1,
                resource: "successor attempts",
                limit: 0,
                actual: 1,
            })
        );
        assert_eq!(ledger, before);

        let rendered = outcome_after(
            Some(&genesis),
            Some(commitment(0x20, 0x21)),
            Some(commitment(0x22, 0x23)),
            [],
        );
        ledger
            .admit_derived(vec![rendered.clone()].into_boxed_slice())
            .unwrap();
        assert_eq!(ledger.verify(), Ok(()));

        let alternate = outcome_after(
            Some(&genesis),
            Some(commitment(0x30, 0x31)),
            Some(commitment(0x32, 0x33)),
            [],
        );
        ledger.admissions[1].successors[0] = alternate;
        assert!(matches!(
            ledger.verify(),
            Err(FrameLedgerV2Error::OutcomeBinding {
                source: FrameReceiptV2AdmissionError::OutcomeMismatch { .. }
            })
        ));
    }

    #[test]
    fn paired_admission_prepares_both_before_backend_and_commits_both_after_success() {
        let genesis = framed_outcome(Some(commitment(0x10, 0x11)), Some(commitment(0x12, 0x13)));
        let mut game =
            FrameLedgerV2::new_structural_genesis(FrameStream::Game, genesis.clone()).unwrap();
        let mut composite =
            FrameLedgerV2::new_structural_genesis(FrameStream::Composite, genesis.clone()).unwrap();
        let mut backend_calls = 0;

        let receipts = admit_frame_pair_derived(
            Some((&mut game, Box::new([]))),
            (&mut composite, Box::new([])),
            |prepared| {
                backend_calls += 1;
                assert!(prepared.game().is_some());
                assert_eq!(prepared.game().unwrap().outcome(), genesis.tag());
                assert_eq!(prepared.composite().outcome(), genesis.tag());
                Ok::<(), &'static str>(())
            },
        )
        .unwrap();

        assert_eq!(backend_calls, 1);
        assert_eq!(game.len(), 1);
        assert_eq!(composite.len(), 1);
        assert_eq!(game.head(), receipts.game().map(FrameReceiptV2::tag));
        assert_eq!(composite.head(), Some(receipts.composite().tag()));
        assert_eq!(game.verify(), Ok(()));
        assert_eq!(composite.verify(), Ok(()));

        let sidebar_only = outcome_after(Some(&genesis), None, Some(commitment(0x20, 0x21)), []);
        let receipts = admit_frame_pair_derived(
            None,
            (
                &mut composite,
                vec![sidebar_only.clone()].into_boxed_slice(),
            ),
            |_| Ok::<(), &'static str>(()),
        )
        .unwrap();
        assert_eq!(receipts.game(), None);
        assert_eq!(receipts.composite().outcome(), sidebar_only.tag());
        assert_eq!(game.len(), 1);
        assert_eq!(composite.len(), 2);
        assert_eq!(composite.outcome_head(), &sidebar_only);
    }

    #[test]
    fn paired_admission_refuses_before_backend_and_publishes_nothing_on_backend_error() {
        let genesis = framed_outcome(Some(commitment(0x10, 0x11)), Some(commitment(0x12, 0x13)));
        let mut game =
            FrameLedgerV2::new_structural_genesis(FrameStream::Game, genesis.clone()).unwrap();
        let limits = FrameLedgerV2Limits {
            max_successor_outcomes: 0,
            max_successor_attempts: 0,
        };
        let mut composite =
            FrameLedgerV2::with_limits(FrameStream::Composite, genesis.clone(), limits).unwrap();
        let composite_child = outcome_after(Some(&genesis), None, Some(commitment(0x20, 0x21)), []);
        let before_game = game.clone();
        let before_composite = composite.clone();
        let mut backend_calls = 0;

        let refused = admit_frame_pair_derived(
            Some((&mut game, Box::new([]))),
            (&mut composite, vec![composite_child].into_boxed_slice()),
            |_| {
                backend_calls += 1;
                Ok::<(), &'static str>(())
            },
        );
        assert!(matches!(
            refused,
            Err(FramePairAdmissionV2Error::Composite {
                source: FrameLedgerV2Error::PathLimitExceeded { .. }
            })
        ));
        assert_eq!(backend_calls, 0);
        assert_eq!(game, before_game);
        assert_eq!(composite, before_composite);

        let mut composite =
            FrameLedgerV2::new_structural_genesis(FrameStream::Composite, genesis).unwrap();
        let before_game = game.clone();
        let before_composite = composite.clone();
        let refused = admit_frame_pair_derived(
            Some((&mut game, Box::new([]))),
            (&mut composite, Box::new([])),
            |_| {
                backend_calls += 1;
                Err("powered backend refusal")
            },
        );
        assert_eq!(
            refused,
            Err(FramePairAdmissionV2Error::Backend(
                "powered backend refusal"
            ))
        );
        assert_eq!(backend_calls, 1);
        assert_eq!(game, before_game);
        assert_eq!(composite, before_composite);
    }

    #[test]
    fn paired_admission_refuses_swapped_stream_roles_before_backend() {
        let genesis = framed_outcome(Some(commitment(0x10, 0x11)), Some(commitment(0x12, 0x13)));
        let mut wrong_game =
            FrameLedgerV2::new_structural_genesis(FrameStream::Composite, genesis.clone()).unwrap();
        let mut composite =
            FrameLedgerV2::new_structural_genesis(FrameStream::Composite, genesis).unwrap();
        let before_game = wrong_game.clone();
        let before_composite = composite.clone();
        let mut backend_called = false;

        let refused = admit_frame_pair_derived(
            Some((&mut wrong_game, Box::new([]))),
            (&mut composite, Box::new([])),
            |_| {
                backend_called = true;
                Ok::<(), &'static str>(())
            },
        );
        assert_eq!(
            refused,
            Err(FramePairAdmissionV2Error::Game {
                source: FrameLedgerV2Error::StreamMismatch {
                    expected: FrameStream::Game,
                    actual: FrameStream::Composite,
                }
            })
        );
        assert!(!backend_called);
        assert_eq!(wrong_game, before_game);
        assert_eq!(composite, before_composite);
    }

    #[test]
    fn independent_python_known_answers_freeze_parent_absent_and_linked_shapes() {
        let sequence_tag = |kind, start: u8| {
            ContentTag::from_digest(
                kind,
                std::array::from_fn(|index| start.wrapping_add(index as u8)),
            )
        };
        let first = FrameReceiptV2::new(
            None,
            FrameStream::Game,
            sequence_tag(TagKind::TickOutcome, 0x00),
            sequence_tag(TagKind::FrameInput, 0x20),
            sequence_tag(TagKind::Frame, 0x40),
        )
        .unwrap();
        assert_vector(
            first,
            "25f4e033da416361d7fea59be7ce4b2967918a7cd571bcfc96dde7b5ccb5ed22",
            "#bb.frame-receipt.sha256.5izrwvevcldss6sdjwnj7lqq6rphz2h6re7oe4helz5mft6crnhq",
        );

        let linked = FrameReceiptV2::new(
            Some(first.tag()),
            FrameStream::Composite,
            sequence_tag(TagKind::TickOutcome, 0x60),
            sequence_tag(TagKind::FrameInput, 0x80),
            sequence_tag(TagKind::Frame, 0xa0),
        )
        .unwrap();
        assert_vector(
            linked,
            "dd68009ad702d093231247a6d8189f6962687a14ee7571d0a44c841af4dce1e4",
            "#bb.frame-receipt.sha256.yq44ktjoqs3axwvxvkesiatpwldh7mmgdzoecer47h5rzpkekejq",
        );
    }

    fn assert_vector(receipt: FrameReceiptV2, payload_hash: &str, tag: &str) {
        let payload = receipt.encode_canonical();
        assert_eq!(payload.len(), FRAME_RECEIPT_V2_PAYLOAD_LEN);
        assert_eq!(format!("{:x}", Sha256::digest(payload)), payload_hash);
        assert_eq!(receipt.tag().to_string(), tag);
        assert_eq!(FrameReceiptV2::decode_canonical(&payload), Ok(receipt));
    }
}
