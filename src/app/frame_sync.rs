//! Live display surface handles, input queues, and legacy/v2 frame-ledger admission.

use super::*;

#[cfg(windows)]
pub(super) struct Display {
    pub(super) surface: WindowsPixelSurface,
    pub(super) window: Arc<Window>,
    pub(super) surface_sized: bool,
}

#[cfg(not(windows))]
pub(super) struct Display {
    // Drop the surface before the context and window whose handles it owns.
    pub(super) surface: Surface<Arc<Window>, Arc<Window>>,
    pub(super) _context: Context<Arc<Window>>,
    pub(super) window: Arc<Window>,
    pub(super) surface_sized: bool,
}

#[derive(Clone, Copy)]
pub(super) enum AimEdit {
    Place,
    Delete,
}

#[derive(Clone, Copy)]
pub(super) enum PendingEdit {
    Aim(AimEdit),
    Exact(WorldEdit),
}

#[derive(Default)]
pub(super) struct Inputs {
    pub(super) forward: bool,
    pub(super) backward: bool,
    pub(super) left: bool,
    pub(super) right: bool,
    pub(super) up: bool,
    pub(super) down: bool,
    pub(super) mouse_x: f64,
    pub(super) mouse_y: f64,
    pub(super) wheel: f64,
    pub(super) edits: Vec<PendingEdit>,
    pub(super) fit_world: bool,
    pub(super) viewport_refit: bool,
    /// Accepted actions consumed by the next fixed tick, by lane.
    pub(super) actions: TickActionLedger,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LegacyPresentedFrameObservation {
    pub(super) legacy_receipt: FrameReceipt,
    pub(super) frame: FrameObservation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PresentedFrameObservation {
    pub(super) legacy_receipt: FrameReceipt,
    pub(super) receipt_v2: FrameReceiptV2,
    pub(super) frame: FrameObservation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PendingRenderedFrame {
    pub(super) parent: Option<bitblock::protocol::ContentTag>,
    pub(super) source_tick: u64,
    pub(super) source_outcome: bitblock::protocol::ContentTag,
    pub(super) input: bitblock::protocol::ContentTag,
    pub(super) frame: FrameObservation,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LiveFrameLedgersV2 {
    pub(super) game: FrameLedgerV2,
    pub(super) composite: FrameLedgerV2,
}

#[cfg(any(test, all(windows, feature = "protocol-research")))]
pub(super) fn current_committed_composite_frame<'a>(
    ledger: &FrameLedgerV2,
    presented: PresentedFrameObservation,
    width: u32,
    height: u32,
    pixels: &'a [u32],
) -> Option<FrameRef<'a>> {
    let receipt = presented.receipt_v2;
    if ledger.stream() != FrameStream::Composite
        || receipt.stream() != FrameStream::Composite
        || ledger.head() != Some(receipt.tag())
        || receipt.verify().is_err()
    {
        return None;
    }
    let frame = FrameRef::new(width, height, pixels).ok()?;
    let observation = frame.observe();
    if observation != presented.frame || receipt.frame() != observation.tag {
        return None;
    }
    Some(frame)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PendingFrameProtocolError {
    MissingSourceTick {
        tick: u64,
    },
    SourceTickMismatch {
        expected: u64,
        actual: u64,
    },
    MissingLedgerOutcome {
        tick: u64,
    },
    LedgerOutcomeMismatch {
        tick: u64,
        expected: bitblock::protocol::ContentTag,
        actual: bitblock::protocol::ContentTag,
    },
    EndpointPrecedesLedgerHead {
        head_tick: u64,
        endpoint_tick: u64,
    },
    PathLimitExceeded {
        resource: &'static str,
        limit: usize,
        actual: usize,
    },
    PathAttemptCountOverflow,
    PathAllocationFailed,
    InvalidReceipt(FrameReceiptV2Error),
    InvalidOutcomeBinding(FrameReceiptV2AdmissionError),
}

pub(super) fn verify_pending_frame(
    outcomes: &[TickOutcome],
    stream: FrameStream,
    pending: PendingRenderedFrame,
) -> Result<(), PendingFrameProtocolError> {
    let index = usize::try_from(pending.source_tick).map_err(|_| {
        PendingFrameProtocolError::MissingSourceTick {
            tick: pending.source_tick,
        }
    })?;
    let outcome = outcomes
        .get(index)
        .ok_or(PendingFrameProtocolError::MissingSourceTick {
            tick: pending.source_tick,
        })?;
    if outcome.tick() != pending.source_tick {
        return Err(PendingFrameProtocolError::SourceTickMismatch {
            expected: pending.source_tick,
            actual: outcome.tick(),
        });
    }
    FrameReceiptV2::new(
        None,
        stream,
        pending.source_outcome,
        pending.input,
        pending.frame.tag,
    )
    .map_err(PendingFrameProtocolError::InvalidReceipt)?
    .verify_selected_outcome(outcome)
    .map_err(PendingFrameProtocolError::InvalidOutcomeBinding)
}

pub(super) fn retained_successors(
    outcomes: &[TickOutcome],
    ledger: &FrameLedgerV2,
    stream: FrameStream,
    pending: PendingRenderedFrame,
) -> Result<Box<[TickOutcome]>, PendingFrameProtocolError> {
    verify_pending_frame(outcomes, stream, pending)?;
    let head = ledger.outcome_head();
    let head_index = usize::try_from(head.tick())
        .map_err(|_| PendingFrameProtocolError::MissingLedgerOutcome { tick: head.tick() })?;
    let retained_head = outcomes
        .get(head_index)
        .ok_or(PendingFrameProtocolError::MissingLedgerOutcome { tick: head.tick() })?;
    if retained_head.tag() != head.tag() {
        return Err(PendingFrameProtocolError::LedgerOutcomeMismatch {
            tick: head.tick(),
            expected: head.tag(),
            actual: retained_head.tag(),
        });
    }
    if pending.source_tick < head.tick() {
        return Err(PendingFrameProtocolError::EndpointPrecedesLedgerHead {
            head_tick: head.tick(),
            endpoint_tick: pending.source_tick,
        });
    }
    if pending.source_tick == head.tick() {
        return Ok(Box::new([]));
    }

    let endpoint_index = usize::try_from(pending.source_tick).map_err(|_| {
        PendingFrameProtocolError::MissingSourceTick {
            tick: pending.source_tick,
        }
    })?;
    let successors = &outcomes[(head_index + 1)..=endpoint_index];
    let limits = ledger.limits();
    if successors.len() > limits.max_successor_outcomes {
        return Err(PendingFrameProtocolError::PathLimitExceeded {
            resource: "successor outcomes",
            limit: limits.max_successor_outcomes,
            actual: successors.len(),
        });
    }
    let attempts = successors.iter().try_fold(0_usize, |total, outcome| {
        total.checked_add(outcome.attempts().len())
    });
    let Some(attempts) = attempts else {
        return Err(PendingFrameProtocolError::PathAttemptCountOverflow);
    };
    if attempts > limits.max_successor_attempts {
        return Err(PendingFrameProtocolError::PathLimitExceeded {
            resource: "successor attempts",
            limit: limits.max_successor_attempts,
            actual: attempts,
        });
    }

    let mut retained = Vec::new();
    retained
        .try_reserve_exact(successors.len())
        .map_err(|_| PendingFrameProtocolError::PathAllocationFailed)?;
    retained.extend(successors.iter().cloned());
    Ok(retained.into_boxed_slice())
}

#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
pub(super) enum SubmitFrameError<E> {
    Backend(E),
    Protocol(FrameProtocolError),
}

#[cfg(test)]
pub(super) fn submit_and_record<E>(
    ledger: &mut FrameLedger,
    pending: PendingRenderedFrame,
    submit: impl FnOnce() -> Result<(), E>,
) -> Result<LegacyPresentedFrameObservation, SubmitFrameError<E>> {
    submit().map_err(SubmitFrameError::Backend)?;
    let receipt = FrameReceipt::new(
        pending.parent,
        pending.source_tick,
        pending.input,
        pending.frame.tag,
    )
    .map_err(SubmitFrameError::Protocol)?;
    let receipt = *ledger.append(receipt).map_err(SubmitFrameError::Protocol)?;
    Ok(LegacyPresentedFrameObservation {
        legacy_receipt: receipt,
        frame: pending.frame,
    })
}

#[cfg(test)]
pub(super) fn submit_pair_and_record<E>(
    game_ledger: &mut FrameLedger,
    output_ledger: &mut FrameLedger,
    game: Option<PendingRenderedFrame>,
    output: PendingRenderedFrame,
    submit: impl FnOnce() -> Result<(), E>,
) -> Result<
    (
        Option<LegacyPresentedFrameObservation>,
        LegacyPresentedFrameObservation,
    ),
    SubmitFrameError<E>,
> {
    submit().map_err(SubmitFrameError::Backend)?;
    let game = game
        .map(|pending| submit_and_record(game_ledger, pending, || Ok::<(), E>(())))
        .transpose()?;
    let output = submit_and_record(output_ledger, output, || Ok::<(), E>(()))?;
    Ok((game, output))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PresentedFramePair {
    pub(super) game: Option<PresentedFrameObservation>,
    pub(super) output: PresentedFrameObservation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum LiveFrameSubmissionError<E> {
    Pending(PendingFrameProtocolError),
    Legacy(FrameProtocolError),
    V2(FramePairAdmissionV2Error<E>),
}

pub(super) fn prepare_legacy_receipt(
    ledger: &FrameLedger,
    pending: PendingRenderedFrame,
) -> Result<FrameReceipt, FrameProtocolError> {
    let receipt = FrameReceipt::new(
        pending.parent,
        pending.source_tick,
        pending.input,
        pending.frame.tag,
    )?;
    let expected_parent = ledger.head();
    if receipt.parent() != expected_parent {
        return Err(FrameProtocolError::ParentMismatch {
            expected: expected_parent,
            actual: receipt.parent(),
        });
    }
    if let Some(previous) = ledger.receipts().last()
        && receipt.source_tick() <= previous.source_tick()
    {
        return Err(FrameProtocolError::NonIncreasingTick {
            previous: previous.source_tick(),
            next: receipt.source_tick(),
        });
    }
    Ok(receipt)
}

pub(super) fn submit_live_frame_pair<E>(
    outcomes: &[TickOutcome],
    game_ledger: &mut FrameLedger,
    output_ledger: &mut FrameLedger,
    ledgers_v2: &mut LiveFrameLedgersV2,
    game: Option<PendingRenderedFrame>,
    output: PendingRenderedFrame,
    submit: impl FnOnce(FrameReceiptPairV2) -> Result<(), E>,
) -> Result<PresentedFramePair, LiveFrameSubmissionError<E>> {
    let game_successors = game
        .map(|pending| retained_successors(outcomes, &ledgers_v2.game, FrameStream::Game, pending))
        .transpose()
        .map_err(LiveFrameSubmissionError::Pending)?;
    let composite_successors = retained_successors(
        outcomes,
        &ledgers_v2.composite,
        FrameStream::Composite,
        output,
    )
    .map_err(LiveFrameSubmissionError::Pending)?;

    let prepared_game_receipt = game
        .map(|pending| prepare_legacy_receipt(game_ledger, pending))
        .transpose()
        .map_err(LiveFrameSubmissionError::Legacy)?;
    let prepared_output_receipt =
        prepare_legacy_receipt(output_ledger, output).map_err(LiveFrameSubmissionError::Legacy)?;

    let LiveFrameLedgersV2 {
        game: game_v2,
        composite: composite_v2,
    } = ledgers_v2;
    let game_admission = game_successors.map(|successors| (game_v2, successors));
    let receipts_v2 = admit_frame_pair_derived(
        game_admission,
        (composite_v2, composite_successors),
        |receipts| {
            match (game, receipts.game()) {
                (Some(pending), Some(receipt)) => {
                    assert_eq!(receipt.outcome(), pending.source_outcome);
                    assert_eq!(receipt.input(), pending.input);
                    assert_eq!(receipt.frame(), pending.frame.tag);
                }
                (None, None) => {}
                _ => panic!("prepared game receipt presence diverged from the pending frame"),
            }
            assert_eq!(receipts.composite().outcome(), output.source_outcome);
            assert_eq!(receipts.composite().input(), output.input);
            assert_eq!(receipts.composite().frame(), output.frame.tag);
            submit(receipts)
        },
    )
    .map_err(LiveFrameSubmissionError::V2)?;

    let legacy_game = match (game, prepared_game_receipt) {
        (Some(pending), Some(receipt)) => {
            let committed = *game_ledger
                .append(receipt)
                .expect("prevalidated legacy game receipt must append");
            Some(LegacyPresentedFrameObservation {
                legacy_receipt: committed,
                frame: pending.frame,
            })
        }
        (None, None) => None,
        _ => unreachable!("prepared legacy game receipt presence must match pending game frame"),
    };
    let committed_output = *output_ledger
        .append(prepared_output_receipt)
        .expect("prevalidated legacy composite receipt must append");
    let legacy_output = LegacyPresentedFrameObservation {
        legacy_receipt: committed_output,
        frame: output.frame,
    };
    let presented_game = match (legacy_game, receipts_v2.game()) {
        (Some(legacy), Some(receipt_v2)) => Some(PresentedFrameObservation {
            legacy_receipt: legacy.legacy_receipt,
            receipt_v2,
            frame: legacy.frame,
        }),
        (None, None) => None,
        _ => unreachable!("validated legacy and v2 game receipt presence must agree"),
    };
    let presented_output = PresentedFrameObservation {
        legacy_receipt: legacy_output.legacy_receipt,
        receipt_v2: receipts_v2.composite(),
        frame: legacy_output.frame,
    };
    Ok(PresentedFramePair {
        game: presented_game,
        output: presented_output,
    })
}

impl Inputs {
    pub(super) fn clear_motion(&mut self) {
        self.forward = false;
        self.backward = false;
        self.left = false;
        self.right = false;
        self.up = false;
        self.down = false;
        self.mouse_x = 0.0;
        self.mouse_y = 0.0;
    }
}
