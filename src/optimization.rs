//! Research-only exact equivalence gate for candidate optimizations.
//!
//! A pass is evidence only for the finite case or corpus named by `case_tag`. It is not a
//! universal claim over all worlds, histories, cameras, floating-point views, or displays.

use crate::display::{DisplayLedgerV2, DisplayOutcome, DisplayProtocolError};
use crate::frame::{
    FrameInput, FrameObservation, FrameProtocolError, FrameRef, FrameShapeError, OverlayInput,
};
use crate::frame_v2::{
    FRAME_RECEIPT_V2_PAYLOAD_LEN, FrameLedgerV2, FrameLedgerV2Error, FrameStream,
};
use crate::history::{History, HistoryError, world_tag};
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::sidebar::CompositeFrameInput;
use crate::tick::{TickOutcome, TickProtocolError};
use crate::world::{Coord, World};

const RECEIPT_SCHEMA: &[u8] = b"bitblock/optimization-equivalence/bounded-corpus/v3";
const OUTPUT_SCHEMA: &[u8] = b"bitblock/optimization-equivalence/output/v3";
const LEDGER_EVIDENCE_SCHEMA: &[u8] = b"bitblock/optimization-equivalence/frame-ledger-evidence/v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssaySide {
    Reference,
    Candidate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityField {
    Implementation,
    Policy,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OptimizationIdentity {
    pub implementation: Option<ContentTag>,
    pub policy: Option<ContentTag>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompositeFrameArtifact {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u32>,
    pub input: CompositeFrameInput,
    pub overlay_input: OverlayInput,
    pub observation: FrameObservation,
    pub frame_ledger: FrameLedgerV2,
}

/// Exact display comparison is admitted only through this deterministic synthetic-fixture
/// wrapper. Live host captures contain nondeterministic timing and geometry observations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeterministicSyntheticDisplay(DisplayLedgerV2);

impl DeterministicSyntheticDisplay {
    pub fn new(ledger: DisplayLedgerV2) -> Result<Self, DisplayProtocolError> {
        ledger.verify()?;
        Ok(Self(ledger))
    }

    pub fn ledger(&self) -> &DisplayLedgerV2 {
        &self.0
    }
}

#[derive(Clone, Debug)]
pub struct OptimizationArtifact {
    pub identity: OptimizationIdentity,
    pub final_world: World,
    pub final_world_tag: ContentTag,
    pub history: History,
    pub frame_input: FrameInput,
    pub game_pixels: Vec<u32>,
    pub game_observation: FrameObservation,
    pub frame_ledger: FrameLedgerV2,
    pub composite: Option<CompositeFrameArtifact>,
    pub synthetic_display: Option<DeterministicSyntheticDisplay>,
}

#[derive(Clone, Debug)]
pub struct OptimizationCase {
    pub case_tag: Option<ContentTag>,
    pub initial_world: World,
    pub reference: OptimizationArtifact,
    pub candidate: OptimizationArtifact,
}

/// First refusal in comparison order; sequence differences carry their exact first index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OptimizationMismatch {
    WorldCoordinate {
        index: usize,
    },
    WorldTag,
    InvalidWorldTag {
        side: AssaySide,
    },
    HistoryReceipt {
        index: usize,
    },
    HistoryHead,
    InvalidHistory {
        side: AssaySide,
        source: HistoryError,
    },
    ReplayedFinalWorldTag,
    HistoryFinalWorldTag {
        side: AssaySide,
    },
    FrameInput,
    InvalidFrameInput {
        side: AssaySide,
        source: FrameProtocolError,
    },
    GamePixel {
        index: usize,
    },
    InvalidGameShape {
        side: AssaySide,
        source: FrameShapeError,
    },
    InvalidGameObservation {
        side: AssaySide,
    },
    FrameObservation,
    FrameLedgerReceipt {
        index: usize,
    },
    FrameLedgerGenesis,
    FrameLedgerAncestry {
        admission: usize,
        edge: usize,
    },
    WrongFrameLedgerStream {
        side: AssaySide,
        actual: FrameStream,
    },
    InvalidFrameLedger {
        side: AssaySide,
        source: FrameLedgerV2Error,
    },
    InvalidFrameOutcomeHistoryBinding {
        side: AssaySide,
    },
    InvalidGameFrameInputBinding {
        side: AssaySide,
        source: TickProtocolError,
    },
    FrameLedgerEndpoint {
        side: AssaySide,
    },
    CompositePresence,
    CompositeViewport,
    CompositePixel {
        index: usize,
    },
    InvalidCompositeShape {
        side: AssaySide,
        source: FrameShapeError,
    },
    InvalidCompositeObservation {
        side: AssaySide,
    },
    CompositeInput,
    InvalidCompositeInputBinding {
        side: AssaySide,
    },
    CompositeObservation,
    CompositeFrameLedgerReceipt {
        index: usize,
    },
    CompositeFrameLedgerGenesis,
    CompositeFrameLedgerAncestry {
        admission: usize,
        edge: usize,
    },
    WrongCompositeFrameLedgerStream {
        side: AssaySide,
        actual: FrameStream,
    },
    InvalidCompositeFrameLedger {
        side: AssaySide,
        source: FrameLedgerV2Error,
    },
    InvalidCompositeOutcomeHistoryBinding {
        side: AssaySide,
    },
    CrossStreamOutcomeAncestry {
        side: AssaySide,
    },
    CompositeFrameLedgerEndpoint {
        side: AssaySide,
    },
    SyntheticDisplayPresence,
    SyntheticDisplayWithoutComposite {
        side: AssaySide,
    },
    SyntheticDisplayEndpoint {
        side: AssaySide,
    },
    SyntheticDisplayReceiptCount {
        side: AssaySide,
        actual: usize,
    },
    SyntheticDisplayNotMatch {
        side: AssaySide,
    },
    InvalidSyntheticDisplay {
        side: AssaySide,
        source: DisplayProtocolError,
    },
    SyntheticDisplayExpectedOutput {
        side: AssaySide,
    },
    SyntheticDisplayCapturedOutput {
        side: AssaySide,
    },
    DisplayLedgerReceipt {
        index: usize,
    },
    LedgerEvidenceLengthOverflow {
        side: AssaySide,
        stream: FrameStream,
    },
    OutputCommitment,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OptimizationRefusal {
    Mismatch(OptimizationMismatch),
    MissingCaseTag,
    MissingIdentity {
        side: AssaySide,
        field: IdentityField,
    },
    WrongIdentityKind {
        side: AssaySide,
        field: IdentityField,
        expected: TagKind,
        actual: TagKind,
    },
}

impl From<OptimizationMismatch> for OptimizationRefusal {
    fn from(value: OptimizationMismatch) -> Self {
        Self::Mismatch(value)
    }
}

/// Receipt for bounded-corpus equality, never universal equivalence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OptimizationEquivalenceReceipt {
    tag: ContentTag,
    case: ContentTag,
    reference: OptimizationIdentity,
    candidate: OptimizationIdentity,
    output: ContentTag,
}

impl OptimizationEquivalenceReceipt {
    pub const fn tag(self) -> ContentTag {
        self.tag
    }

    pub const fn case(self) -> ContentTag {
        self.case
    }

    pub const fn reference_identity(self) -> OptimizationIdentity {
        self.reference
    }

    pub const fn candidate_identity(self) -> OptimizationIdentity {
        self.candidate
    }

    pub const fn output(self) -> ContentTag {
        self.output
    }
}

pub fn assay_optimization_equivalence(
    case: &OptimizationCase,
) -> Result<OptimizationEquivalenceReceipt, OptimizationRefusal> {
    let case_tag = case.case_tag.ok_or(OptimizationRefusal::MissingCaseTag)?;
    let reference = complete_identity(case.reference.identity, AssaySide::Reference)?;
    let candidate = complete_identity(case.candidate.identity, AssaySide::Candidate)?;

    compare_worlds(&case.reference, &case.candidate)?;
    compare_histories(case)?;
    if case.reference.frame_input != case.candidate.frame_input {
        return Err(OptimizationMismatch::FrameInput.into());
    }
    validate_frame_input(AssaySide::Reference, case.reference.frame_input)?;
    validate_frame_input(AssaySide::Candidate, case.candidate.frame_input)?;
    if let Some(index) = first_mismatch(&case.reference.game_pixels, &case.candidate.game_pixels) {
        return Err(OptimizationMismatch::GamePixel { index }.into());
    }
    validate_game_frame(AssaySide::Reference, &case.reference)?;
    validate_game_frame(AssaySide::Candidate, &case.candidate)?;
    if case.reference.game_observation != case.candidate.game_observation {
        return Err(OptimizationMismatch::FrameObservation.into());
    }
    compare_game_ledgers(&case.reference.frame_ledger, &case.candidate.frame_ledger)?;
    validate_frame_ledger(AssaySide::Reference, &case.initial_world, &case.reference)?;
    validate_frame_ledger(AssaySide::Candidate, &case.initial_world, &case.candidate)?;
    compare_composite(case)?;
    compare_display(&case.reference, &case.candidate)?;

    let reference_output = output_tag(&case.reference).map_err(|stream| {
        OptimizationMismatch::LedgerEvidenceLengthOverflow {
            side: AssaySide::Reference,
            stream,
        }
    })?;
    let candidate_output = output_tag(&case.candidate).map_err(|stream| {
        OptimizationMismatch::LedgerEvidenceLengthOverflow {
            side: AssaySide::Candidate,
            stream,
        }
    })?;
    if reference_output != candidate_output {
        return Err(OptimizationMismatch::OutputCommitment.into());
    }
    let tags = [
        case_tag,
        world_tag(&case.initial_world),
        reference.implementation.unwrap(),
        reference.policy.unwrap(),
        candidate.implementation.unwrap(),
        candidate.policy.unwrap(),
        reference_output,
        candidate_output,
    ];
    let tag = hash_canonical(TagKind::Receipt, RECEIPT_SCHEMA, 256, |sink| {
        for tag in tags {
            sink(&tag.digest());
        }
    })
    .expect("eight full assay digests have fixed length");
    Ok(OptimizationEquivalenceReceipt {
        tag,
        case: case_tag,
        reference,
        candidate,
        output: reference_output,
    })
}

fn validate_frame_input(side: AssaySide, input: FrameInput) -> Result<(), OptimizationMismatch> {
    input
        .verify()
        .map_err(|source| OptimizationMismatch::InvalidFrameInput { side, source })
}

fn validate_game_frame(
    side: AssaySide,
    artifact: &OptimizationArtifact,
) -> Result<(), OptimizationMismatch> {
    let (width, height) = artifact.frame_input.viewport();
    let observation = FrameRef::new(width, height, &artifact.game_pixels)
        .map_err(|source| OptimizationMismatch::InvalidGameShape { side, source })?
        .observe();
    if observation != artifact.game_observation {
        return Err(OptimizationMismatch::InvalidGameObservation { side });
    }
    Ok(())
}

fn validate_frame_ledger(
    side: AssaySide,
    initial_world: &World,
    artifact: &OptimizationArtifact,
) -> Result<(), OptimizationMismatch> {
    if artifact.frame_ledger.stream() != FrameStream::Game {
        return Err(OptimizationMismatch::WrongFrameLedgerStream {
            side,
            actual: artifact.frame_ledger.stream(),
        });
    }
    artifact
        .frame_ledger
        .verify()
        .map_err(|source| OptimizationMismatch::InvalidFrameLedger { side, source })?;
    if !outcomes_bind_history(initial_world, artifact, &artifact.frame_ledger) {
        return Err(OptimizationMismatch::InvalidFrameOutcomeHistoryBinding { side });
    }
    verify_all_game_inputs(
        &artifact.frame_ledger,
        artifact.frame_input.renderer_profile(),
        artifact.frame_input.overlay_input(),
    )
    .map_err(|source| OptimizationMismatch::InvalidGameFrameInputBinding { side, source })?;
    let endpoint = artifact
        .frame_ledger
        .admissions()
        .last()
        .map(|admission| admission.receipt());
    if !endpoint.is_some_and(|receipt| {
        receipt.input() == artifact.frame_input.tag()
            && receipt.frame() == artifact.game_observation.tag
            && receipt.outcome() == artifact.frame_ledger.outcome_head().tag()
    }) {
        return Err(OptimizationMismatch::FrameLedgerEndpoint { side });
    }
    Ok(())
}

fn verify_all_game_inputs(
    ledger: &FrameLedgerV2,
    renderer_profile: ContentTag,
    overlay_input: ContentTag,
) -> Result<(), TickProtocolError> {
    ledger
        .genesis()
        .verify_game_input_binding(renderer_profile, overlay_input)?;
    for admission in ledger.admissions() {
        for outcome in admission.successors() {
            outcome.verify_game_input_binding(renderer_profile, overlay_input)?;
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LedgerDifference {
    Genesis,
    Receipt { index: usize },
    Ancestry { admission: usize, edge: usize },
}

fn first_ledger_difference(
    reference: &FrameLedgerV2,
    candidate: &FrameLedgerV2,
) -> Option<LedgerDifference> {
    if reference.genesis() != candidate.genesis() {
        return Some(LedgerDifference::Genesis);
    }
    let reference_admissions = reference.admissions();
    let candidate_admissions = candidate.admissions();
    let common = reference_admissions.len().min(candidate_admissions.len());
    for admission in 0..common {
        if let Some(edge) = first_mismatch(
            reference_admissions[admission].successors(),
            candidate_admissions[admission].successors(),
        ) {
            return Some(LedgerDifference::Ancestry { admission, edge });
        }
        if reference_admissions[admission].receipt() != candidate_admissions[admission].receipt() {
            return Some(LedgerDifference::Receipt { index: admission });
        }
    }
    (reference_admissions.len() != candidate_admissions.len())
        .then_some(LedgerDifference::Receipt { index: common })
}

fn compare_game_ledgers(
    reference: &FrameLedgerV2,
    candidate: &FrameLedgerV2,
) -> Result<(), OptimizationMismatch> {
    match first_ledger_difference(reference, candidate) {
        None => Ok(()),
        Some(LedgerDifference::Genesis) => Err(OptimizationMismatch::FrameLedgerGenesis),
        Some(LedgerDifference::Receipt { index }) => {
            Err(OptimizationMismatch::FrameLedgerReceipt { index })
        }
        Some(LedgerDifference::Ancestry { admission, edge }) => {
            Err(OptimizationMismatch::FrameLedgerAncestry { admission, edge })
        }
    }
}

fn outcomes_bind_history(
    initial_world: &World,
    artifact: &OptimizationArtifact,
    ledger: &FrameLedgerV2,
) -> bool {
    let genesis = ledger.genesis();
    if genesis.pre_world() != world_tag(initial_world)
        || genesis.pre_history() != artifact.history.genesis()
    {
        return false;
    }

    let receipts = artifact.history.receipts();
    let mut receipt_index = 0_usize;
    let mut check = |outcome: &TickOutcome| {
        let has_executed = outcome.executed_edits().next().is_some();
        if !has_executed {
            return outcome.pre_history() == outcome.post_history();
        }
        let Some(receipt) = receipts.get(receipt_index) else {
            return false;
        };
        receipt_index += 1;
        receipt.tick() == outcome.tick()
            && receipt.parent() == outcome.pre_history()
            && receipt.tag() == outcome.post_history()
            && receipt.post_world() == outcome.post_world()
            && receipt.edits().iter().copied().eq(outcome.executed_edits())
    };

    if !check(genesis) {
        return false;
    }
    for admission in ledger.admissions() {
        for outcome in admission.successors() {
            if !check(outcome) {
                return false;
            }
        }
    }
    receipt_index == receipts.len()
        && ledger.outcome_head().post_world() == artifact.final_world_tag
        && ledger.outcome_head().post_history() == artifact.history.head()
}

fn compare_worlds(
    reference: &OptimizationArtifact,
    candidate: &OptimizationArtifact,
) -> Result<(), OptimizationMismatch> {
    let mut reference_coords: Vec<Coord> = reference.final_world.iter_on().collect();
    let mut candidate_coords: Vec<Coord> = candidate.final_world.iter_on().collect();
    reference_coords.sort_unstable();
    candidate_coords.sort_unstable();
    if let Some(index) = first_mismatch(&reference_coords, &candidate_coords) {
        return Err(OptimizationMismatch::WorldCoordinate { index });
    }
    if reference.final_world_tag != candidate.final_world_tag {
        return Err(OptimizationMismatch::WorldTag);
    }
    for (side, artifact) in [
        (AssaySide::Reference, reference),
        (AssaySide::Candidate, candidate),
    ] {
        if world_tag(&artifact.final_world) != artifact.final_world_tag {
            return Err(OptimizationMismatch::InvalidWorldTag { side });
        }
    }
    Ok(())
}

fn compare_histories(case: &OptimizationCase) -> Result<(), OptimizationMismatch> {
    if let Some(index) = first_mismatch(
        case.reference.history.receipts(),
        case.candidate.history.receipts(),
    ) {
        return Err(OptimizationMismatch::HistoryReceipt { index });
    }
    if case.reference.history.head() != case.candidate.history.head() {
        return Err(OptimizationMismatch::HistoryHead);
    }
    let replay = |side, history: &History| {
        history
            .verify(&case.initial_world)
            .map_err(|source| OptimizationMismatch::InvalidHistory { side, source })
            .map(|world| world_tag(&world))
    };
    let reference = replay(AssaySide::Reference, &case.reference.history)?;
    let candidate = replay(AssaySide::Candidate, &case.candidate.history)?;
    if reference != candidate {
        return Err(OptimizationMismatch::ReplayedFinalWorldTag);
    }
    if reference != case.reference.final_world_tag {
        return Err(OptimizationMismatch::HistoryFinalWorldTag {
            side: AssaySide::Reference,
        });
    }
    if candidate != case.candidate.final_world_tag {
        return Err(OptimizationMismatch::HistoryFinalWorldTag {
            side: AssaySide::Candidate,
        });
    }
    Ok(())
}

fn compare_composite(case: &OptimizationCase) -> Result<(), OptimizationMismatch> {
    let reference = &case.reference;
    let candidate = &case.candidate;
    match (&reference.composite, &candidate.composite) {
        (None, None) => Ok(()),
        (Some(reference), Some(candidate)) => {
            if (reference.width, reference.height) != (candidate.width, candidate.height) {
                return Err(OptimizationMismatch::CompositeViewport);
            }
            if let Some(index) = first_mismatch(&reference.pixels, &candidate.pixels) {
                return Err(OptimizationMismatch::CompositePixel { index });
            }
            validate_composite_frame(AssaySide::Reference, reference)?;
            validate_composite_frame(AssaySide::Candidate, candidate)?;
            if reference.input != candidate.input {
                return Err(OptimizationMismatch::CompositeInput);
            }
            if reference.observation != candidate.observation {
                return Err(OptimizationMismatch::CompositeObservation);
            }
            match first_ledger_difference(&reference.frame_ledger, &candidate.frame_ledger) {
                None => {}
                Some(LedgerDifference::Genesis) => {
                    return Err(OptimizationMismatch::CompositeFrameLedgerGenesis);
                }
                Some(LedgerDifference::Receipt { index }) => {
                    return Err(OptimizationMismatch::CompositeFrameLedgerReceipt { index });
                }
                Some(LedgerDifference::Ancestry { admission, edge }) => {
                    return Err(OptimizationMismatch::CompositeFrameLedgerAncestry {
                        admission,
                        edge,
                    });
                }
            }
            validate_composite_ledger(
                AssaySide::Reference,
                &case.initial_world,
                &case.reference,
                reference,
            )?;
            validate_composite_ledger(
                AssaySide::Candidate,
                &case.initial_world,
                &case.candidate,
                candidate,
            )?;
            Ok(())
        }
        _ => Err(OptimizationMismatch::CompositePresence),
    }
}

fn validate_composite_ledger(
    side: AssaySide,
    initial_world: &World,
    optimization: &OptimizationArtifact,
    artifact: &CompositeFrameArtifact,
) -> Result<(), OptimizationMismatch> {
    if artifact.frame_ledger.stream() != FrameStream::Composite {
        return Err(OptimizationMismatch::WrongCompositeFrameLedgerStream {
            side,
            actual: artifact.frame_ledger.stream(),
        });
    }
    artifact
        .frame_ledger
        .verify()
        .map_err(|source| OptimizationMismatch::InvalidCompositeFrameLedger { side, source })?;
    if !outcomes_bind_history(initial_world, optimization, &artifact.frame_ledger) {
        return Err(OptimizationMismatch::InvalidCompositeOutcomeHistoryBinding { side });
    }
    if !outcome_evidence_equal(&optimization.frame_ledger, &artifact.frame_ledger) {
        return Err(OptimizationMismatch::CrossStreamOutcomeAncestry { side });
    }
    let (simulation_width, simulation_height) = optimization.frame_input.viewport();
    let expected_input = CompositeFrameInput::new(
        optimization.frame_input.tag(),
        optimization.game_observation.tag,
        artifact.overlay_input,
        simulation_width,
        simulation_height,
        artifact.width,
        artifact.height,
    );
    if artifact.overlay_input.verify().is_err()
        || artifact.input != expected_input
        || artifact.frame_ledger.outcome_head().input().dimensions()
            != [
                simulation_width,
                simulation_height,
                artifact.width,
                artifact.height,
            ]
    {
        return Err(OptimizationMismatch::InvalidCompositeInputBinding { side });
    }
    let endpoint = artifact
        .frame_ledger
        .admissions()
        .last()
        .map(|admission| admission.receipt());
    if !endpoint.is_some_and(|receipt| {
        receipt.input() == artifact.input.tag()
            && receipt.frame() == artifact.observation.tag
            && receipt.outcome() == artifact.frame_ledger.outcome_head().tag()
    }) {
        return Err(OptimizationMismatch::CompositeFrameLedgerEndpoint { side });
    }
    Ok(())
}

fn outcome_evidence_equal(left: &FrameLedgerV2, right: &FrameLedgerV2) -> bool {
    std::iter::once(left.genesis())
        .chain(
            left.admissions()
                .iter()
                .flat_map(|admission| admission.successors()),
        )
        .eq(std::iter::once(right.genesis()).chain(
            right
                .admissions()
                .iter()
                .flat_map(|admission| admission.successors()),
        ))
}

fn validate_composite_frame(
    side: AssaySide,
    artifact: &CompositeFrameArtifact,
) -> Result<(), OptimizationMismatch> {
    let observation = FrameRef::new(artifact.width, artifact.height, &artifact.pixels)
        .map_err(|source| OptimizationMismatch::InvalidCompositeShape { side, source })?
        .observe();
    if observation != artifact.observation {
        return Err(OptimizationMismatch::InvalidCompositeObservation { side });
    }
    Ok(())
}

fn compare_display(
    reference_artifact: &OptimizationArtifact,
    candidate_artifact: &OptimizationArtifact,
) -> Result<(), OptimizationMismatch> {
    match (
        &reference_artifact.synthetic_display,
        &candidate_artifact.synthetic_display,
    ) {
        (None, None) => Ok(()),
        (Some(reference_display), Some(candidate_display)) => {
            reference_display.ledger().verify().map_err(|source| {
                OptimizationMismatch::InvalidSyntheticDisplay {
                    side: AssaySide::Reference,
                    source,
                }
            })?;
            candidate_display.ledger().verify().map_err(|source| {
                OptimizationMismatch::InvalidSyntheticDisplay {
                    side: AssaySide::Candidate,
                    source,
                }
            })?;
            for (side, display) in [
                (AssaySide::Reference, reference_display),
                (AssaySide::Candidate, candidate_display),
            ] {
                if display.ledger().is_empty() {
                    return Err(OptimizationMismatch::SyntheticDisplayEndpoint { side });
                }
                if display.ledger().len() != 1 {
                    return Err(OptimizationMismatch::SyntheticDisplayReceiptCount {
                        side,
                        actual: display.ledger().len(),
                    });
                }
                if display.ledger().receipts()[0].facts().outcome != DisplayOutcome::Match {
                    return Err(OptimizationMismatch::SyntheticDisplayNotMatch { side });
                }
            }
            if let Some(index) = first_mismatch(
                reference_display.ledger().receipts(),
                candidate_display.ledger().receipts(),
            ) {
                return Err(OptimizationMismatch::DisplayLedgerReceipt { index });
            }
            validate_display_endpoint(AssaySide::Reference, reference_artifact, reference_display)?;
            validate_display_endpoint(AssaySide::Candidate, candidate_artifact, candidate_display)?;
            Ok(())
        }
        _ => Err(OptimizationMismatch::SyntheticDisplayPresence),
    }
}

fn validate_display_endpoint(
    side: AssaySide,
    artifact: &OptimizationArtifact,
    display: &DeterministicSyntheticDisplay,
) -> Result<(), OptimizationMismatch> {
    let composite = artifact
        .composite
        .as_ref()
        .ok_or(OptimizationMismatch::SyntheticDisplayWithoutComposite { side })?;
    let receipt = display
        .ledger()
        .receipts()
        .last()
        .expect("display endpoint presence was checked before validation");
    let expected_output_receipt = composite
        .frame_ledger
        .admissions()
        .last()
        .map(|admission| admission.receipt())
        .expect("composite ledger endpoint was validated before display");
    if receipt.expected_receipt() != expected_output_receipt
        || receipt.expected_viewport() != (composite.width, composite.height)
    {
        return Err(OptimizationMismatch::SyntheticDisplayExpectedOutput { side });
    }
    if receipt.facts().captured != Some(composite.observation) {
        return Err(OptimizationMismatch::SyntheticDisplayCapturedOutput { side });
    }
    Ok(())
}

fn first_mismatch<T: PartialEq>(reference: &[T], candidate: &[T]) -> Option<usize> {
    let common = reference.len().min(candidate.len());
    (0..common)
        .find(|&index| reference[index] != candidate[index])
        .or_else(|| (reference.len() != candidate.len()).then_some(common))
}

fn complete_identity(
    identity: OptimizationIdentity,
    side: AssaySide,
) -> Result<OptimizationIdentity, OptimizationRefusal> {
    let implementation = identity
        .implementation
        .ok_or(OptimizationRefusal::MissingIdentity {
            side,
            field: IdentityField::Implementation,
        })?;
    let policy = identity
        .policy
        .ok_or(OptimizationRefusal::MissingIdentity {
            side,
            field: IdentityField::Policy,
        })?;
    for (field, tag, expected) in [
        (
            IdentityField::Implementation,
            implementation,
            TagKind::Registry,
        ),
        (IdentityField::Policy, policy, TagKind::Policy),
    ] {
        if tag.kind() != expected {
            return Err(OptimizationRefusal::WrongIdentityKind {
                side,
                field,
                expected,
                actual: tag.kind(),
            });
        }
    }
    Ok(identity)
}

fn output_tag(artifact: &OptimizationArtifact) -> Result<ContentTag, FrameStream> {
    let game_evidence =
        frame_ledger_evidence_tag(&artifact.frame_ledger).ok_or(FrameStream::Game)?;
    let composite_evidence = artifact
        .composite
        .as_ref()
        .map(|composite| {
            frame_ledger_evidence_tag(&composite.frame_ledger).ok_or(FrameStream::Composite)
        })
        .transpose()?;
    let tag = hash_canonical(TagKind::Receipt, OUTPUT_SCHEMA, 290, |sink| {
        for tag in [
            artifact.final_world_tag,
            artifact.history.head(),
            artifact.frame_input.tag(),
            artifact.game_observation.tag,
            game_evidence,
        ] {
            sink(&tag.digest());
        }
        match &artifact.composite {
            Some(composite) => {
                sink(&[1]);
                sink(&composite.input.tag().digest());
                sink(&composite.observation.tag.digest());
                sink(
                    &composite_evidence
                        .expect("a composite artifact prepared one evidence tag")
                        .digest(),
                );
            }
            None => {
                sink(&[0]);
                sink(&[0; 96]);
            }
        }
        match &artifact.synthetic_display {
            Some(display) => {
                sink(&[1]);
                sink(
                    &display
                        .ledger()
                        .head()
                        .expect("a synthetic display fixture has a receipt")
                        .digest(),
                );
            }
            None => {
                sink(&[0]);
                sink(&[0; 32]);
            }
        }
    })
    .expect("validated optimization output encoding has fixed length");
    Ok(tag)
}

fn frame_ledger_evidence_tag(ledger: &FrameLedgerV2) -> Option<ContentTag> {
    let payload_len = frame_ledger_evidence_len(ledger)?;
    let stream = match ledger.stream() {
        FrameStream::Game => 0,
        FrameStream::Composite => 1,
    };
    Some(
        hash_canonical(
            TagKind::Receipt,
            LEDGER_EVIDENCE_SCHEMA,
            payload_len,
            |sink| {
                sink(&[stream]);
                emit_outcome_evidence(ledger.genesis(), sink);
                sink(
                    &u64::try_from(ledger.admissions().len())
                        .expect("payload length already admitted the count")
                        .to_be_bytes(),
                );
                for admission in ledger.admissions() {
                    sink(&admission.receipt().encode_canonical());
                    sink(
                        &u64::try_from(admission.successors().len())
                            .expect("payload length already admitted the count")
                            .to_be_bytes(),
                    );
                    for outcome in admission.successors() {
                        emit_outcome_evidence(outcome, sink);
                    }
                }
            },
        )
        .expect("checked ledger-evidence length matches the streamed retained bytes"),
    )
}

fn frame_ledger_evidence_len(ledger: &FrameLedgerV2) -> Option<u64> {
    let outcome_len =
        |outcome: &TickOutcome| u64::try_from(outcome.encoded_len()).ok()?.checked_add(8);
    let mut length = 1_u64.checked_add(outcome_len(ledger.genesis())?)?;
    length = length.checked_add(8)?;
    u64::try_from(ledger.admissions().len()).ok()?;
    for admission in ledger.admissions() {
        u64::try_from(admission.successors().len()).ok()?;
        length = length.checked_add(FRAME_RECEIPT_V2_PAYLOAD_LEN as u64 + 8)?;
        for outcome in admission.successors() {
            length = length.checked_add(outcome_len(outcome)?)?;
        }
    }
    Some(length)
}

fn emit_outcome_evidence(outcome: &TickOutcome, sink: &mut dyn FnMut(&[u8])) {
    sink(
        &u64::try_from(outcome.encoded_len())
            .expect("payload length already admitted the outcome")
            .to_be_bytes(),
    );
    outcome.encode_canonical_into(sink);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::Infallible;

    use crate::camera::Camera;
    use crate::display::{CaptureRect, CaptureStage, DisplayCaptureFacts, DisplayOutcome};
    use crate::frame::FrameRef;
    use crate::frame_v2::{FrameReceiptV2, admit_frame_pair_derived};
    use crate::history::{NoOpReason, TransitionEffect};
    use crate::program::{Edit, Operation};
    use crate::render::{crosshair_overlay_input, renderer_profile_tag};
    use crate::sidebar::{
        DisplayStatus, FrameCompositor, SidebarRasterFacts, SidebarSnapshot,
        TwoBufferReferenceCompositor, composite_overlay_input, composite_policy_tag,
    };
    use crate::tick::{
        TickAttempt, TickCameraState, TickFrameCommitment, TickInputFlags, TickInputState,
    };
    use crate::world::BEDROCK_COORD;

    const A: Coord = Coord::new(1, 0, 0);
    const B: Coord = Coord::new(2, 0, 0);
    const C: Coord = Coord::new(3, 0, 0);
    const D: Coord = Coord::new(4, 0, 0);

    fn tag(kind: TagKind, bytes: &[u8]) -> ContentTag {
        hash_canonical(
            kind,
            b"bitblock/optimization-test/v1",
            bytes.len() as u64,
            |sink| sink(bytes),
        )
        .unwrap()
    }

    fn plans() -> [Vec<Edit>; 2] {
        [
            vec![
                Edit::new(Operation::On, A),
                Edit::new(Operation::On, A),
                Edit::new(Operation::Off, B),
                Edit::new(Operation::Off, BEDROCK_COORD),
                Edit::new(Operation::On, C),
                Edit::new(Operation::Off, C),
            ],
            vec![
                Edit::new(Operation::Off, A),
                Edit::new(Operation::On, A),
                Edit::new(Operation::On, D),
                Edit::new(Operation::Off, D),
            ],
        ]
    }

    fn execute(plans: &[Vec<Edit>]) -> (World, History) {
        let mut world = World::new();
        let mut history = History::new(&world);
        for (index, edits) in plans.iter().enumerate() {
            let mut transaction = history.begin_tick(index as u64 + 1);
            for &edit in edits {
                transaction.apply_edit(&history, &mut world, edit);
            }
            history.commit_tick(transaction, &world).unwrap();
        }
        (world, history)
    }

    fn display(
        expected: &[u32],
        captured: &[u32],
        width: u32,
        height: u32,
        frame_receipt: FrameReceiptV2,
    ) -> DeterministicSyntheticDisplay {
        let observation = FrameRef::new(width, height, captured).unwrap().observe();
        let first = expected
            .iter()
            .zip(captured)
            .position(|(left, right)| left != right);
        let facts = DisplayCaptureFacts {
            outcome: if first.is_some() {
                DisplayOutcome::Mismatch
            } else {
                DisplayOutcome::Match
            },
            stage: CaptureStage::Complete,
            error_code: 0,
            flush_before: true,
            compositor_flush: true,
            flush_after: true,
            rect_before: CaptureRect {
                left: 0,
                top: 0,
                right: width as i32,
                bottom: height as i32,
            },
            rect_after: CaptureRect {
                left: 0,
                top: 0,
                right: width as i32,
                bottom: height as i32,
            },
            dpi: 96,
            qpc_start: 1,
            qpc_end: 2,
            qpc_frequency: 1,
            captured: Some(observation),
            mismatch_count: u64::from(first.is_some()),
            first_mismatch: first.map(|index| index as u64),
        };
        let mut ledger = DisplayLedgerV2::default();
        ledger.append(frame_receipt, width, height, facts).unwrap();
        DeterministicSyntheticDisplay::new(ledger).unwrap()
    }

    #[derive(Clone)]
    struct FixtureFrame {
        input: FrameInput,
        game_pixels: Vec<u32>,
        game_observation: FrameObservation,
        composite_input: CompositeFrameInput,
        composite_overlay: OverlayInput,
        composite_pixels: Vec<u32>,
        composite_observation: FrameObservation,
    }

    fn fixture_frame(
        post_world: ContentTag,
        camera: &Camera,
        game_pixels: Vec<u32>,
    ) -> FixtureFrame {
        let overlay = crosshair_overlay_input();
        let input = FrameInput::capture(
            post_world,
            camera,
            2,
            2,
            renderer_profile_tag(),
            overlay.tag(),
        )
        .unwrap();
        let game_observation = FrameRef::new(2, 2, &game_pixels).unwrap().observe();
        let composite_overlay =
            composite_overlay_input(tag(TagKind::MetricSnapshot, b"snapshot")).unwrap();
        let composite_input = CompositeFrameInput::new(
            input.tag(),
            game_observation.tag,
            composite_overlay,
            2,
            2,
            3,
            2,
        );
        let composite_pixels = vec![
            game_pixels[0],
            game_pixels[1],
            7,
            game_pixels[2],
            game_pixels[3],
            0,
        ];
        let composite_observation = FrameRef::new(3, 2, &composite_pixels).unwrap().observe();
        FixtureFrame {
            input,
            game_pixels,
            game_observation,
            composite_input,
            composite_overlay,
            composite_pixels,
            composite_observation,
        }
    }

    struct FixtureRun {
        initial_world: World,
        final_world: World,
        history: History,
        genesis: TickOutcome,
        successors: Vec<TickOutcome>,
        final_frame: FixtureFrame,
    }

    const PRODUCTION_SIMULATION_WIDTH: u32 = 801;
    const PRODUCTION_HEIGHT: u32 = 801;
    const PRODUCTION_OUTPUT_WIDTH: u32 = 1_201;

    enum ExecutedCompositor {
        Reference(TwoBufferReferenceCompositor),
        Candidate(FrameCompositor),
    }

    impl ExecutedCompositor {
        fn compose(
            &mut self,
            game_pixels: &[u32],
            game_frame: ContentTag,
            snapshot: &SidebarSnapshot,
        ) -> SidebarRasterFacts {
            match self {
                Self::Reference(compositor) => compositor.compose(game_pixels, snapshot),
                Self::Candidate(compositor) => {
                    compositor.compose(game_pixels, game_frame, snapshot)
                }
            }
        }

        fn pixels(&self) -> &[u32] {
            match self {
                Self::Reference(compositor) => compositor.pixels(),
                Self::Candidate(compositor) => compositor.pixels(),
            }
        }
    }

    struct CompositorExecution {
        artifact: OptimizationArtifact,
        facts: [SidebarRasterFacts; 2],
        snapshots: [ContentTag; 2],
        game_frames: [ContentTag; 2],
        composite_frames: [Vec<u32>; 2],
    }

    fn compositor_snapshot(
        tick: u64,
        world: &World,
        history: &History,
        camera: &Camera,
        visible: bool,
        previous_raster: Option<SidebarRasterFacts>,
    ) -> SidebarSnapshot {
        SidebarSnapshot {
            frame_source_tick: tick,
            tick_index: tick + 1,
            occupied_bits: world.len() as u64,
            mutable_bits: world.len().saturating_sub(1) as u64,
            bounds: world.bounds(),
            observer_position: camera.position(),
            observer_cell: None,
            yaw_radians: camera.yaw_radians(),
            pitch_radians: camera.pitch_radians(),
            observation_distance: camera.observation_distance(),
            origin_distance: camera.position().length(),
            simulation_width: PRODUCTION_SIMULATION_WIDTH,
            simulation_height: PRODUCTION_HEIGHT,
            output_width: PRODUCTION_OUTPUT_WIDTH,
            output_height: PRODUCTION_HEIGHT,
            sidebar_width: PRODUCTION_OUTPUT_WIDTH - PRODUCTION_SIMULATION_WIDTH,
            world_pixels: u64::from(PRODUCTION_SIMULATION_WIDTH) * u64::from(PRODUCTION_HEIGHT),
            top_saturated: false,
            world: world_tag(world),
            history_head: history.head(),
            history_len: history.receipts().len() as u64,
            prior_frame_head: None,
            prior_frame_len: 0,
            prior_display_head: None,
            prior_display_len: 0,
            display_status: DisplayStatus::AuditDisabled,
            visible,
            previous_raster,
        }
    }

    fn execute_compositor_arm(reference: bool) -> CompositorExecution {
        let world = World::new();
        let history = History::new(&world);
        let camera = Camera::for_observer_spawn();
        let camera_state = TickCameraState::capture(&camera);
        let world_root = world_tag(&world);
        let render_overlay = crosshair_overlay_input();
        let frame_input = FrameInput::capture(
            world_root,
            &camera,
            PRODUCTION_SIMULATION_WIDTH,
            PRODUCTION_HEIGHT,
            renderer_profile_tag(),
            render_overlay.tag(),
        )
        .unwrap();
        let idle_input = TickInputState::new(
            TickInputFlags::default(),
            0.0,
            0.0,
            0.0,
            PRODUCTION_SIMULATION_WIDTH,
            PRODUCTION_HEIGHT,
            PRODUCTION_OUTPUT_WIDTH,
            PRODUCTION_HEIGHT,
        )
        .unwrap();
        let genesis = TickOutcome::new(
            None,
            0,
            world_root,
            history.genesis(),
            idle_input,
            [],
            world_root,
            history.genesis(),
            camera_state,
            None,
            None,
        )
        .unwrap();
        let mut compositor = if reference {
            ExecutedCompositor::Reference(TwoBufferReferenceCompositor::new(
                PRODUCTION_OUTPUT_WIDTH,
                PRODUCTION_HEIGHT,
                PRODUCTION_SIMULATION_WIDTH,
            ))
        } else {
            ExecutedCompositor::Candidate(FrameCompositor::new(
                PRODUCTION_OUTPUT_WIDTH,
                PRODUCTION_HEIGHT,
                PRODUCTION_SIMULATION_WIDTH,
            ))
        };
        let mut parent = genesis.tag();
        let mut outcomes = Vec::with_capacity(2);
        let mut facts = [SidebarRasterFacts::default(); 2];
        let mut snapshots = [world_root; 2];
        let mut game_frames = [world_root; 2];
        let mut composite_frames = std::array::from_fn(|_| Vec::new());
        let mut final_game = None;
        let mut final_composite = None;

        for index in 0..2 {
            let visible = index == 0;
            let game_pixels = (0_u32
                ..PRODUCTION_SIMULATION_WIDTH.saturating_mul(PRODUCTION_HEIGHT))
                .map(|pixel| {
                    let base = pixel.wrapping_mul(0x0001_0101) & 0x00ff_ffff;
                    if index == 0 { base } else { base ^ 0x00ff_ffff }
                })
                .collect::<Vec<_>>();
            let game_observation =
                FrameRef::new(PRODUCTION_SIMULATION_WIDTH, PRODUCTION_HEIGHT, &game_pixels)
                    .unwrap()
                    .observe();
            game_frames[index] = game_observation.tag;
            let snapshot = compositor_snapshot(
                index as u64 + 1,
                &world,
                &history,
                &camera,
                visible,
                if index == 0 {
                    None
                } else {
                    Some(facts[index - 1])
                },
            );
            snapshots[index] = snapshot.tag();
            facts[index] = compositor.compose(&game_pixels, game_observation.tag, &snapshot);
            let composite_pixels = compositor.pixels().to_vec();
            composite_frames[index] = composite_pixels.clone();
            let composite_observation = FrameRef::new(
                PRODUCTION_OUTPUT_WIDTH,
                PRODUCTION_HEIGHT,
                &composite_pixels,
            )
            .unwrap()
            .observe();
            let composite_overlay = composite_overlay_input(snapshot.tag()).unwrap();
            let composite_input = CompositeFrameInput::new(
                frame_input.tag(),
                game_observation.tag,
                composite_overlay,
                PRODUCTION_SIMULATION_WIDTH,
                PRODUCTION_HEIGHT,
                PRODUCTION_OUTPUT_WIDTH,
                PRODUCTION_HEIGHT,
            );
            let input = TickInputState::new(
                TickInputFlags::new(
                    false, false, false, false, false, false, false, true, true, visible, false,
                ),
                0.0,
                0.0,
                0.0,
                PRODUCTION_SIMULATION_WIDTH,
                PRODUCTION_HEIGHT,
                PRODUCTION_OUTPUT_WIDTH,
                PRODUCTION_HEIGHT,
            )
            .unwrap();
            let game_commitment =
                TickFrameCommitment::new(frame_input.tag(), game_observation.tag).unwrap();
            let composite_commitment =
                TickFrameCommitment::new(composite_input.tag(), composite_observation.tag).unwrap();
            let outcome = TickOutcome::new(
                Some(parent),
                index as u64 + 1,
                world_root,
                history.head(),
                input,
                [],
                world_root,
                history.head(),
                camera_state,
                Some(game_commitment),
                Some(composite_commitment),
            )
            .unwrap();
            parent = outcome.tag();
            outcomes.push(outcome);
            final_game = Some((game_pixels, game_observation));
            final_composite = Some((
                composite_pixels,
                composite_observation,
                composite_overlay,
                composite_input,
            ));
        }

        let mut frame_ledger =
            FrameLedgerV2::new_structural_genesis(FrameStream::Game, genesis.clone()).unwrap();
        let mut composite_ledger =
            FrameLedgerV2::new_structural_genesis(FrameStream::Composite, genesis).unwrap();
        admit_frame_pair_derived(
            Some((&mut frame_ledger, outcomes.clone().into_boxed_slice())),
            (&mut composite_ledger, outcomes.into_boxed_slice()),
            |_| Ok::<(), Infallible>(()),
        )
        .unwrap();
        let (game_pixels, game_observation) = final_game.unwrap();
        let (composite_pixels, composite_observation, composite_overlay, composite_input) =
            final_composite.unwrap();
        let composite = CompositeFrameArtifact {
            width: PRODUCTION_OUTPUT_WIDTH,
            height: PRODUCTION_HEIGHT,
            pixels: composite_pixels,
            input: composite_input,
            overlay_input: composite_overlay,
            observation: composite_observation,
            frame_ledger: composite_ledger,
        };
        CompositorExecution {
            artifact: OptimizationArtifact {
                identity: OptimizationIdentity::default(),
                final_world: world,
                final_world_tag: world_root,
                history,
                frame_input,
                game_pixels,
                game_observation,
                frame_ledger,
                composite: Some(composite),
                synthetic_display: None,
            },
            facts,
            snapshots,
            game_frames,
            composite_frames,
        }
    }

    fn current_test_executable_tag() -> ContentTag {
        let path = std::env::current_exe().unwrap();
        let bytes = std::fs::read(path).unwrap();
        hash_canonical(
            TagKind::Registry,
            b"bitblock/optimization-test-executable/v1",
            bytes.len() as u64,
            |sink| sink(&bytes),
        )
        .unwrap()
    }

    fn compositor_implementation_tag(executable: ContentTag, arm: &[u8]) -> ContentTag {
        let sidebar_source = tag(TagKind::Registry, include_bytes!("sidebar.rs"));
        let cargo_lock = tag(TagKind::Registry, include_bytes!("../Cargo.lock"));
        let toolchain = tag(TagKind::Registry, include_bytes!("../rust-toolchain.toml"));
        hash_canonical(
            TagKind::Registry,
            b"bitblock/compositor-implementation-manifest/v1",
            130 + arm.len() as u64,
            |sink| {
                sink(&executable.digest());
                sink(&sidebar_source.digest());
                sink(&cargo_lock.digest());
                sink(&toolchain.digest());
                sink(&(arm.len() as u16).to_be_bytes());
                sink(arm);
            },
        )
        .unwrap()
    }

    fn compositor_case_tag(snapshots: [ContentTag; 2], game_frames: [ContentTag; 2]) -> ContentTag {
        hash_canonical(
            TagKind::Registry,
            b"bitblock/compositor-stride-case/v1",
            144,
            |sink| {
                for value in [
                    PRODUCTION_SIMULATION_WIDTH,
                    PRODUCTION_HEIGHT,
                    PRODUCTION_OUTPUT_WIDTH,
                    PRODUCTION_HEIGHT,
                ] {
                    sink(&value.to_be_bytes());
                }
                sink(&snapshots[0].digest());
                sink(&snapshots[1].digest());
                sink(&game_frames[0].digest());
                sink(&game_frames[1].digest());
            },
        )
        .unwrap()
    }

    fn execute_v2_fixture() -> FixtureRun {
        let initial_world = World::new();
        let mut world = initial_world.clone();
        let mut history = History::new(&world);
        let camera = Camera::for_single_pixel_voxel(2);
        let camera_state = TickCameraState::capture(&camera);
        let idle_input =
            TickInputState::new(TickInputFlags::default(), 0.0, 0.0, 0.0, 2, 2, 3, 2).unwrap();
        let initial_world_tag = world_tag(&world);
        let genesis = TickOutcome::new(
            None,
            0,
            initial_world_tag,
            history.genesis(),
            idle_input,
            [],
            initial_world_tag,
            history.genesis(),
            camera_state,
            None,
            None,
        )
        .unwrap();

        let rendered_input = TickInputState::new(
            TickInputFlags::new(
                false, false, false, false, false, false, false, true, true, true, false,
            ),
            0.0,
            0.0,
            0.0,
            2,
            2,
            3,
            2,
        )
        .unwrap();
        let mut parent = genesis.tag();
        let mut successors = Vec::new();
        let mut final_frame = None;
        for (index, edits) in plans().iter().enumerate() {
            let tick = index as u64 + 1;
            let pre_world = world_tag(&world);
            let pre_history = history.head();
            let mut transaction = history.begin_tick(tick);
            for &edit in edits {
                transaction.apply_edit(&history, &mut world, edit);
            }
            history.commit_tick(transaction, &world).unwrap();
            let post_world = world_tag(&world);
            let post_history = history.head();
            let pixels = if index == 0 {
                vec![0x0000_0001, 0x0000_0002, 0x0000_0003, 0x0000_0004]
            } else {
                vec![0x0001_0203, 0x0004_0506, 0, 0x00ff_ffff]
            };
            let frame = fixture_frame(post_world, &camera, pixels);
            let game =
                TickFrameCommitment::new(frame.input.tag(), frame.game_observation.tag).unwrap();
            let composite = TickFrameCommitment::new(
                frame.composite_input.tag(),
                frame.composite_observation.tag,
            )
            .unwrap();
            let outcome = TickOutcome::new(
                Some(parent),
                tick,
                pre_world,
                pre_history,
                rendered_input,
                edits.iter().copied().map(TickAttempt::ExactExecuted),
                post_world,
                post_history,
                camera_state,
                Some(game),
                Some(composite),
            )
            .unwrap();
            outcome
                .verify_game_input_binding(renderer_profile_tag(), crosshair_overlay_input().tag())
                .unwrap();
            parent = outcome.tag();
            successors.push(outcome);
            final_frame = Some(frame);
        }
        FixtureRun {
            initial_world,
            final_world: world,
            history,
            genesis,
            successors,
            final_frame: final_frame.expect("the fixture has two rendered ticks"),
        }
    }

    fn fixture() -> OptimizationCase {
        let run = execute_v2_fixture();
        let FixtureRun {
            initial_world,
            final_world,
            history,
            genesis,
            successors,
            final_frame,
        } = run;
        let final_world_tag = world_tag(&final_world);
        let FixtureFrame {
            input: frame_input,
            game_pixels,
            game_observation,
            composite_input,
            composite_overlay,
            composite_pixels,
            composite_observation,
        } = final_frame;
        let mut frame_ledger =
            FrameLedgerV2::new_structural_genesis(FrameStream::Game, genesis.clone()).unwrap();
        let mut composite_frame_ledger =
            FrameLedgerV2::new_structural_genesis(FrameStream::Composite, genesis).unwrap();
        let receipts = admit_frame_pair_derived(
            Some((&mut frame_ledger, successors.clone().into_boxed_slice())),
            (&mut composite_frame_ledger, successors.into_boxed_slice()),
            |_| Ok::<(), Infallible>(()),
        )
        .unwrap();
        let composite_receipt = receipts.composite();
        let composite = CompositeFrameArtifact {
            width: 3,
            height: 2,
            observation: composite_observation,
            pixels: composite_pixels.clone(),
            input: composite_input,
            overlay_input: composite_overlay,
            frame_ledger: composite_frame_ledger,
        };
        let synthetic_display = display(
            &composite_pixels,
            &composite_pixels,
            3,
            2,
            composite_receipt,
        );
        let reference = OptimizationArtifact {
            identity: OptimizationIdentity {
                implementation: Some(tag(TagKind::Registry, b"reference")),
                policy: Some(tag(TagKind::Policy, b"reference-policy")),
            },
            final_world,
            final_world_tag,
            history,
            frame_input,
            game_pixels,
            game_observation,
            frame_ledger,
            composite: Some(composite),
            synthetic_display: Some(synthetic_display),
        };
        let mut candidate = reference.clone();
        candidate.identity = OptimizationIdentity {
            implementation: Some(tag(TagKind::Registry, b"candidate")),
            policy: Some(tag(TagKind::Policy, b"candidate-policy")),
        };
        OptimizationCase {
            case_tag: Some(tag(TagKind::Registry, b"two-tick-case")),
            initial_world,
            reference,
            candidate,
        }
    }

    fn rebuild_outcome(
        base: &TickOutcome,
        parent: Option<ContentTag>,
        input: TickInputState,
        post_history: ContentTag,
        game: Option<TickFrameCommitment>,
    ) -> TickOutcome {
        TickOutcome::new(
            parent,
            base.tick(),
            base.pre_world(),
            base.pre_history(),
            input,
            base.attempts().iter().copied(),
            base.post_world(),
            post_history,
            base.post_camera(),
            game,
            base.composite(),
        )
        .unwrap()
    }

    fn rebuild_ledger(
        source: &FrameLedgerV2,
        stream: FrameStream,
        successors: Vec<TickOutcome>,
    ) -> FrameLedgerV2 {
        let mut ledger =
            FrameLedgerV2::new_structural_genesis(stream, source.genesis().clone()).unwrap();
        ledger.admit_derived(successors.into_boxed_slice()).unwrap();
        ledger
    }

    fn alternate_ancestry(source: &FrameLedgerV2, stream: FrameStream) -> FrameLedgerV2 {
        let mut successors = source.admissions()[0].successors().to_vec();
        let first = successors[0].clone();
        let alternate_input = TickInputState::new(
            TickInputFlags::new(
                false, false, false, false, false, false, false, true, true, false, false,
            ),
            0.0,
            0.0,
            0.0,
            2,
            2,
            3,
            2,
        )
        .unwrap();
        successors[0] = rebuild_outcome(
            &first,
            first.parent(),
            alternate_input,
            first.post_history(),
            first.game(),
        );
        let second = successors[1].clone();
        successors[1] = rebuild_outcome(
            &second,
            Some(successors[0].tag()),
            second.input(),
            second.post_history(),
            second.game(),
        );
        rebuild_ledger(source, stream, successors)
    }

    #[test]
    fn fixture_covers_effects_and_receipt_binds_identities() {
        let mut case = fixture();
        let effects: Vec<_> = case
            .reference
            .history
            .receipts()
            .iter()
            .flat_map(|receipt| receipt.effects().iter().copied())
            .collect();
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, TransitionEffect::Mint { .. }))
        );
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, TransitionEffect::Burn { .. }))
        );
        for reason in [
            NoOpReason::AlreadyOn,
            NoOpReason::AlreadyOff,
            NoOpReason::BedrockProtected,
        ] {
            assert!(effects.iter().any(|effect| matches!(effect,
                TransitionEffect::NoOp { reason: actual, .. } if *actual == reason)));
        }
        assert!(!case.reference.final_world.is_on(C) && !case.reference.final_world.is_on(D));
        let first = assay_optimization_equivalence(&case).unwrap();
        case.reference.composite = None;
        case.candidate.composite = None;
        case.reference.synthetic_display = None;
        case.candidate.synthetic_display = None;
        let without_output = assay_optimization_equivalence(&case).unwrap();
        assert_ne!(first.output(), without_output.output());
        assert_ne!(first.tag(), without_output.tag());
        case.candidate.identity.implementation = Some(tag(TagKind::Registry, b"candidate-2"));
        assert_ne!(
            without_output.tag(),
            assay_optimization_equivalence(&case).unwrap().tag()
        );
        case.candidate.identity.implementation = Some(tag(TagKind::Policy, b"wrong-kind"));
        assert_eq!(
            assay_optimization_equivalence(&case),
            Err(OptimizationRefusal::WrongIdentityKind {
                side: AssaySide::Candidate,
                field: IdentityField::Implementation,
                expected: TagKind::Registry,
                actual: TagKind::Policy,
            })
        );
        case.candidate.identity.implementation = Some(tag(TagKind::Registry, b"candidate-2"));
        case.candidate.identity.policy = None;
        assert_eq!(
            assay_optimization_equivalence(&case),
            Err(OptimizationRefusal::MissingIdentity {
                side: AssaySide::Candidate,
                field: IdentityField::Policy,
            })
        );
    }

    #[test]
    fn production_strided_compositor_is_v3_equivalent_to_two_buffer_reference() {
        let mut reference = execute_compositor_arm(true);
        let mut candidate = execute_compositor_arm(false);
        assert_eq!(reference.facts, candidate.facts);
        assert_eq!(reference.snapshots, candidate.snapshots);
        assert_eq!(reference.game_frames, candidate.game_frames);
        let raw_mismatches = std::array::from_fn(|index| {
            first_mismatch(
                &reference.composite_frames[index],
                &candidate.composite_frames[index],
            )
        });

        let executable = current_test_executable_tag();
        let reference_implementation =
            compositor_implementation_tag(executable, b"two-buffer-reference-compositor");
        let candidate_implementation =
            compositor_implementation_tag(executable, b"production-strided-compositor");
        assert_ne!(reference_implementation, candidate_implementation);
        let policy = composite_policy_tag();
        reference.artifact.identity = OptimizationIdentity {
            implementation: Some(reference_implementation),
            policy: Some(policy),
        };
        candidate.artifact.identity = OptimizationIdentity {
            implementation: Some(candidate_implementation),
            policy: Some(policy),
        };
        let case_tag = compositor_case_tag(reference.snapshots, reference.game_frames);
        let case = OptimizationCase {
            case_tag: Some(case_tag),
            initial_world: World::new(),
            reference: reference.artifact,
            candidate: candidate.artifact,
        };
        let assay = assay_optimization_equivalence(&case);
        assert_eq!(
            raw_mismatches,
            [None, None],
            "v3 assay result for the same executed frames: {assay:?}"
        );
        let receipt = assay.unwrap();
        eprintln!(
            "strided_compositor_receipt case={} reference_implementation={} candidate_implementation={} output={} receipt={}",
            case_tag,
            reference_implementation,
            candidate_implementation,
            receipt.output(),
            receipt.tag()
        );
        assert_eq!(receipt.case(), case_tag);
        assert_eq!(receipt.reference_identity().policy, Some(policy));
        assert_eq!(receipt.candidate_identity().policy, Some(policy));
    }

    #[test]
    fn required_metadata_refuses_before_artifact_validation() {
        let mut case = fixture();
        let case_tag = case.case_tag;
        let reference_identity = case.reference.identity;

        case.candidate.game_pixels.clear();
        case.case_tag = None;
        assert_eq!(
            assay_optimization_equivalence(&case),
            Err(OptimizationRefusal::MissingCaseTag)
        );

        case.case_tag = case_tag;
        case.reference.identity.implementation = None;
        assert_eq!(
            assay_optimization_equivalence(&case),
            Err(OptimizationRefusal::MissingIdentity {
                side: AssaySide::Reference,
                field: IdentityField::Implementation,
            })
        );

        case.reference.identity = reference_identity;
        case.candidate.identity.policy = None;
        assert_eq!(
            assay_optimization_equivalence(&case),
            Err(OptimizationRefusal::MissingIdentity {
                side: AssaySide::Candidate,
                field: IdentityField::Policy,
            })
        );
    }

    #[test]
    fn history_drop_reorder_and_no_op_controls_refuse() {
        let mut dropped = fixture();
        dropped.candidate.history = execute(&plans()[..1]).1;
        assert!(matches!(
            assay_optimization_equivalence(&dropped),
            Err(OptimizationRefusal::Mismatch(
                OptimizationMismatch::HistoryReceipt { index: 1 }
            ))
        ));
        let mut reordered = fixture();
        let mut reordered_plans = plans();
        reordered_plans.swap(0, 1);
        reordered.candidate.history = execute(&reordered_plans).1;
        assert!(matches!(
            assay_optimization_equivalence(&reordered),
            Err(OptimizationRefusal::Mismatch(
                OptimizationMismatch::HistoryReceipt { index: 0 }
            ))
        ));
        let mut no_op = fixture();
        let mut changed_plans = plans();
        changed_plans[0][1] = Edit::new(Operation::Off, Coord::new(99, 0, 0));
        no_op.candidate.history = execute(&changed_plans).1;
        assert!(matches!(
            assay_optimization_equivalence(&no_op),
            Err(OptimizationRefusal::Mismatch(
                OptimizationMismatch::HistoryReceipt { index: 0 }
            ))
        ));
    }

    #[test]
    fn input_pixel_observation_and_display_controls_refuse() {
        let mut input = fixture();
        input.candidate.frame_input = FrameInput::capture(
            input.candidate.final_world_tag,
            &Camera::for_single_pixel_voxel(2),
            2,
            2,
            tag(TagKind::Policy, b"mutated-renderer"),
            input.candidate.frame_input.overlay_input(),
        )
        .unwrap();
        assert!(matches!(
            assay_optimization_equivalence(&input),
            Err(OptimizationRefusal::Mismatch(
                OptimizationMismatch::FrameInput
            ))
        ));
        let mut pixel = fixture();
        pixel.candidate.game_pixels[2] ^= 1;
        assert!(matches!(
            assay_optimization_equivalence(&pixel),
            Err(OptimizationRefusal::Mismatch(
                OptimizationMismatch::GamePixel { index: 2 }
            ))
        ));
        let mut observation = fixture();
        observation.candidate.game_observation.facts.red_sum += 1;
        assert!(matches!(
            assay_optimization_equivalence(&observation),
            Err(OptimizationRefusal::Mismatch(
                OptimizationMismatch::InvalidGameObservation {
                    side: AssaySide::Candidate
                }
            ))
        ));
        let mut shown = fixture();
        let composite = shown.candidate.composite.as_ref().unwrap();
        let expected = composite.pixels.clone();
        let mut captured = expected.clone();
        captured[0] ^= 1;
        let receipt = composite.frame_ledger.admissions()[0].receipt();
        shown.candidate.synthetic_display = Some(display(
            &expected,
            &captured,
            composite.width,
            composite.height,
            receipt,
        ));
        assert!(matches!(
            assay_optimization_equivalence(&shown),
            Err(OptimizationRefusal::Mismatch(
                OptimizationMismatch::SyntheticDisplayNotMatch {
                    side: AssaySide::Candidate
                }
            ))
        ));
    }

    #[test]
    fn typed_stream_roles_and_exact_ancestry_are_authoritative() {
        let mut wrong_game_role = fixture();
        let composite = wrong_game_role.reference.composite.as_ref().unwrap();
        let wrong = composite.frame_ledger.clone();
        wrong_game_role.reference.frame_ledger = wrong.clone();
        wrong_game_role.candidate.frame_ledger = wrong;
        assert!(matches!(
            assay_optimization_equivalence(&wrong_game_role),
            Err(OptimizationRefusal::Mismatch(
                OptimizationMismatch::WrongFrameLedgerStream {
                    side: AssaySide::Reference,
                    actual: FrameStream::Composite
                }
            ))
        ));

        let mut wrong_composite_role = fixture();
        let wrong = wrong_composite_role.reference.frame_ledger.clone();
        wrong_composite_role
            .reference
            .composite
            .as_mut()
            .unwrap()
            .frame_ledger = wrong.clone();
        wrong_composite_role
            .candidate
            .composite
            .as_mut()
            .unwrap()
            .frame_ledger = wrong;
        assert!(matches!(
            assay_optimization_equivalence(&wrong_composite_role),
            Err(OptimizationRefusal::Mismatch(
                OptimizationMismatch::WrongCompositeFrameLedgerStream {
                    side: AssaySide::Reference,
                    actual: FrameStream::Game
                }
            ))
        ));

        let mut candidate_path = fixture();
        candidate_path.candidate.frame_ledger =
            alternate_ancestry(&candidate_path.reference.frame_ledger, FrameStream::Game);
        assert_ne!(
            output_tag(&candidate_path.reference).unwrap(),
            output_tag(&candidate_path.candidate).unwrap(),
            "the output receipt must bind intermediate retained ancestry"
        );
        assert!(matches!(
            assay_optimization_equivalence(&candidate_path),
            Err(OptimizationRefusal::Mismatch(
                OptimizationMismatch::FrameLedgerAncestry {
                    admission: 0,
                    edge: 0
                }
            ))
        ));

        let mut split_streams = fixture();
        let alternate = alternate_ancestry(
            &split_streams
                .reference
                .composite
                .as_ref()
                .unwrap()
                .frame_ledger,
            FrameStream::Composite,
        );
        split_streams
            .reference
            .composite
            .as_mut()
            .unwrap()
            .frame_ledger = alternate.clone();
        split_streams
            .candidate
            .composite
            .as_mut()
            .unwrap()
            .frame_ledger = alternate;
        assert!(matches!(
            assay_optimization_equivalence(&split_streams),
            Err(OptimizationRefusal::Mismatch(
                OptimizationMismatch::CrossStreamOutcomeAncestry {
                    side: AssaySide::Reference
                }
            ))
        ));
    }

    #[test]
    fn typed_outcomes_bind_history_and_game_context() {
        let mut wrong_history = fixture();
        let source = &wrong_history.reference.frame_ledger;
        let mut successors = source.admissions()[0].successors().to_vec();
        let final_outcome = successors.last().unwrap().clone();
        *successors.last_mut().unwrap() = rebuild_outcome(
            &final_outcome,
            final_outcome.parent(),
            final_outcome.input(),
            tag(TagKind::Receipt, b"wrong-history-head"),
            final_outcome.game(),
        );
        let wrong = rebuild_ledger(source, FrameStream::Game, successors);
        wrong_history.reference.frame_ledger = wrong.clone();
        wrong_history.candidate.frame_ledger = wrong;
        assert!(matches!(
            assay_optimization_equivalence(&wrong_history),
            Err(OptimizationRefusal::Mismatch(
                OptimizationMismatch::InvalidFrameOutcomeHistoryBinding {
                    side: AssaySide::Reference
                }
            ))
        ));

        let mut wrong_context = fixture();
        let source = &wrong_context.reference.frame_ledger;
        let mut successors = source.admissions()[0].successors().to_vec();
        let final_outcome = successors.last().unwrap().clone();
        let wrong_game = TickFrameCommitment::new(
            tag(TagKind::FrameInput, b"wrong-game-input"),
            final_outcome.game().unwrap().frame(),
        )
        .unwrap();
        *successors.last_mut().unwrap() = rebuild_outcome(
            &final_outcome,
            final_outcome.parent(),
            final_outcome.input(),
            final_outcome.post_history(),
            Some(wrong_game),
        );
        let wrong = rebuild_ledger(source, FrameStream::Game, successors);
        wrong_context.reference.frame_ledger = wrong.clone();
        wrong_context.candidate.frame_ledger = wrong;
        assert!(matches!(
            assay_optimization_equivalence(&wrong_context),
            Err(OptimizationRefusal::Mismatch(
                OptimizationMismatch::InvalidGameFrameInputBinding {
                    side: AssaySide::Reference,
                    ..
                }
            ))
        ));

        let mut wrong_earlier_context = fixture();
        let source = &wrong_earlier_context.reference.frame_ledger;
        let mut successors = source.admissions()[0].successors().to_vec();
        let first = successors[0].clone();
        let wrong_game = TickFrameCommitment::new(
            tag(TagKind::FrameInput, b"wrong-earlier-game-input"),
            first.game().unwrap().frame(),
        )
        .unwrap();
        successors[0] = rebuild_outcome(
            &first,
            first.parent(),
            first.input(),
            first.post_history(),
            Some(wrong_game),
        );
        let second = successors[1].clone();
        successors[1] = rebuild_outcome(
            &second,
            Some(successors[0].tag()),
            second.input(),
            second.post_history(),
            second.game(),
        );
        let wrong = rebuild_ledger(source, FrameStream::Game, successors);
        wrong_earlier_context.reference.frame_ledger = wrong.clone();
        wrong_earlier_context.candidate.frame_ledger = wrong;
        assert!(matches!(
            assay_optimization_equivalence(&wrong_earlier_context),
            Err(OptimizationRefusal::Mismatch(
                OptimizationMismatch::InvalidGameFrameInputBinding {
                    side: AssaySide::Reference,
                    ..
                }
            ))
        ));
    }

    #[test]
    fn composite_input_is_reconstructed_from_the_game_artifact() {
        let mut case = fixture();
        let wrong_overlay =
            composite_overlay_input(tag(TagKind::MetricSnapshot, b"wrong-snapshot")).unwrap();
        case.reference.composite.as_mut().unwrap().overlay_input = wrong_overlay;
        case.candidate.composite.as_mut().unwrap().overlay_input = wrong_overlay;
        assert!(matches!(
            assay_optimization_equivalence(&case),
            Err(OptimizationRefusal::Mismatch(
                OptimizationMismatch::InvalidCompositeInputBinding {
                    side: AssaySide::Reference
                }
            ))
        ));
    }

    #[test]
    fn display_v2_refuses_game_receipts_without_mutation() {
        let case = fixture();
        let game_receipt = case.reference.frame_ledger.admissions()[0].receipt();
        let facts = case
            .reference
            .synthetic_display
            .as_ref()
            .unwrap()
            .ledger()
            .receipts()[0]
            .facts();
        let mut ledger = DisplayLedgerV2::default();
        let before = ledger.clone();
        assert!(ledger.append(game_receipt, 3, 2, facts).is_err());
        assert_eq!(ledger, before);
    }

    #[test]
    fn synthetic_display_requires_one_matched_composite_receipt() {
        let mut multiple = fixture();
        let composite = multiple.reference.composite.as_ref().unwrap();
        let frame_receipt = composite.frame_ledger.admissions()[0].receipt();
        let facts = multiple
            .reference
            .synthetic_display
            .as_ref()
            .unwrap()
            .ledger()
            .receipts()[0]
            .facts();
        let mut ledger = DisplayLedgerV2::default();
        ledger
            .append(frame_receipt, composite.width, composite.height, facts)
            .unwrap();
        ledger
            .append(frame_receipt, composite.width, composite.height, facts)
            .unwrap();
        let two_receipt_display = DeterministicSyntheticDisplay::new(ledger).unwrap();
        multiple.reference.synthetic_display = Some(two_receipt_display.clone());
        multiple.candidate.synthetic_display = Some(two_receipt_display);
        assert!(matches!(
            assay_optimization_equivalence(&multiple),
            Err(OptimizationRefusal::Mismatch(
                OptimizationMismatch::SyntheticDisplayReceiptCount {
                    side: AssaySide::Reference,
                    actual: 2
                }
            ))
        ));

        let mut mismatched = fixture();
        let composite = mismatched.reference.composite.as_ref().unwrap();
        let expected = composite.pixels.clone();
        let mut captured = expected.clone();
        captured[0] ^= 1;
        let display = display(
            &expected,
            &captured,
            composite.width,
            composite.height,
            composite.frame_ledger.admissions()[0].receipt(),
        );
        mismatched.reference.synthetic_display = Some(display.clone());
        mismatched.candidate.synthetic_display = Some(display);
        assert!(matches!(
            assay_optimization_equivalence(&mismatched),
            Err(OptimizationRefusal::Mismatch(
                OptimizationMismatch::SyntheticDisplayNotMatch {
                    side: AssaySide::Reference
                }
            ))
        ));

        let mut forged = fixture();
        let composite = forged.reference.composite.as_ref().unwrap();
        let frame_receipt = composite.frame_ledger.admissions()[0].receipt();
        let mut facts = forged
            .reference
            .synthetic_display
            .as_ref()
            .unwrap()
            .ledger()
            .receipts()[0]
            .facts();
        facts.captured.as_mut().unwrap().facts.red_sum += 1;
        let mut ledger = DisplayLedgerV2::default();
        ledger
            .append(frame_receipt, composite.width, composite.height, facts)
            .unwrap();
        let forged_display = DeterministicSyntheticDisplay::new(ledger).unwrap();
        forged.reference.synthetic_display = Some(forged_display.clone());
        forged.candidate.synthetic_display = Some(forged_display);
        assert!(matches!(
            assay_optimization_equivalence(&forged),
            Err(OptimizationRefusal::Mismatch(
                OptimizationMismatch::SyntheticDisplayCapturedOutput {
                    side: AssaySide::Reference
                }
            ))
        ));
    }

    #[test]
    fn v3_optimization_kat_and_prior_domains_are_distinct() {
        let case = fixture();
        let composite = case.reference.composite.as_ref().unwrap();
        let actual_game_evidence = frame_ledger_evidence_tag(&case.reference.frame_ledger).unwrap();
        let actual_composite_evidence = frame_ledger_evidence_tag(&composite.frame_ledger).unwrap();
        let actual_output = output_tag(&case.reference).unwrap();
        let actual_receipt = assay_optimization_equivalence(&case).unwrap();
        assert_eq!(
            actual_game_evidence.to_string(),
            "#bb.receipt.sha256.yr4vgqk3uz4slvbekyrsfmgf2qaspq252kcy2olqfqd6yporhhkq"
        );
        assert_eq!(
            actual_composite_evidence.to_string(),
            "#bb.receipt.sha256.n5y3jp4ahvhadzw7qbjxm2sxbnh7lwu4cmydyhkhvij367fshjfa"
        );
        assert_eq!(
            actual_output.to_string(),
            "#bb.receipt.sha256.ta633j5g7y5zhrxht5qml6sejmbjcdrsvd37whc4ykn3hgwgbs5q"
        );
        assert_eq!(
            actual_receipt.tag().to_string(),
            "#bb.receipt.sha256.haywsy6jofj3k6gehasulniebg2kvzpdzoxwpys7fejucsaujmpa"
        );
        let payload = [0x5a_u8; 290];
        let old_output = hash_canonical(
            TagKind::Receipt,
            b"bitblock/optimization-equivalence/output/v1",
            payload.len() as u64,
            |sink| sink(&payload),
        )
        .unwrap();
        let draft_output = hash_canonical(
            TagKind::Receipt,
            b"bitblock/optimization-equivalence/output/v2",
            payload.len() as u64,
            |sink| sink(&payload),
        )
        .unwrap();
        let new_output = hash_canonical(
            TagKind::Receipt,
            OUTPUT_SCHEMA,
            payload.len() as u64,
            |sink| sink(&payload),
        )
        .unwrap();
        assert_ne!(old_output, new_output);
        assert_ne!(draft_output, new_output);

        let receipt_payload = [0xa5_u8; 256];
        let old_receipt = hash_canonical(
            TagKind::Receipt,
            b"bitblock/optimization-equivalence/bounded-corpus/v1",
            receipt_payload.len() as u64,
            |sink| sink(&receipt_payload),
        )
        .unwrap();
        let draft_receipt = hash_canonical(
            TagKind::Receipt,
            b"bitblock/optimization-equivalence/bounded-corpus/v2",
            receipt_payload.len() as u64,
            |sink| sink(&receipt_payload),
        )
        .unwrap();
        let new_receipt = hash_canonical(
            TagKind::Receipt,
            RECEIPT_SCHEMA,
            receipt_payload.len() as u64,
            |sink| sink(&receipt_payload),
        )
        .unwrap();
        assert_ne!(old_receipt, new_receipt);
        assert_ne!(draft_receipt, new_receipt);
    }

    #[test]
    fn shared_invalid_cached_surfaces_cannot_self_confirm() {
        let mut observation = fixture();
        observation.reference.game_observation.facts.red_sum += 1;
        observation.candidate.game_observation.facts.red_sum += 1;
        assert!(matches!(
            assay_optimization_equivalence(&observation),
            Err(OptimizationRefusal::Mismatch(
                OptimizationMismatch::InvalidGameObservation {
                    side: AssaySide::Reference
                }
            ))
        ));

        let mut endpoint = fixture();
        let other_frame = FrameRef::new(2, 2, &[1, 2, 3, 4]).unwrap().observe();
        let original = &endpoint.reference.frame_ledger;
        let genesis = original.genesis().clone();
        let mut successors = original.admissions()[0].successors().to_vec();
        let base = successors.last().unwrap().clone();
        let wrong_game =
            TickFrameCommitment::new(base.game().unwrap().input(), other_frame.tag).unwrap();
        *successors.last_mut().unwrap() = TickOutcome::new(
            base.parent(),
            base.tick(),
            base.pre_world(),
            base.pre_history(),
            base.input(),
            base.attempts().iter().copied(),
            base.post_world(),
            base.post_history(),
            base.post_camera(),
            Some(wrong_game),
            base.composite(),
        )
        .unwrap();
        let mut wrong_ledger =
            FrameLedgerV2::new_structural_genesis(FrameStream::Game, genesis).unwrap();
        wrong_ledger
            .admit_derived(successors.into_boxed_slice())
            .unwrap();
        endpoint.reference.frame_ledger = wrong_ledger.clone();
        endpoint.candidate.frame_ledger = wrong_ledger;
        assert!(matches!(
            assay_optimization_equivalence(&endpoint),
            Err(OptimizationRefusal::Mismatch(
                OptimizationMismatch::FrameLedgerEndpoint {
                    side: AssaySide::Reference
                }
            ))
        ));

        let mut displayed = fixture();
        let composite = displayed.reference.composite.as_ref().unwrap();
        let alternate_receipt = FrameReceiptV2::new(
            None,
            FrameStream::Composite,
            tag(TagKind::TickOutcome, b"alternate-outcome"),
            composite.input.tag(),
            composite.observation.tag,
        )
        .unwrap();
        let composite_pixels = composite.pixels.clone();
        let wrong_display = display(
            &composite_pixels,
            &composite_pixels,
            composite.width,
            composite.height,
            alternate_receipt,
        );
        displayed.reference.synthetic_display = Some(wrong_display.clone());
        displayed.candidate.synthetic_display = Some(wrong_display);
        assert!(matches!(
            assay_optimization_equivalence(&displayed),
            Err(OptimizationRefusal::Mismatch(
                OptimizationMismatch::SyntheticDisplayExpectedOutput {
                    side: AssaySide::Reference
                }
            ))
        ));
    }
}
