//! `REFINE-1@accounting`: Rust-to-Lean `AccountingRefinement` correspondence.
//!
//! Mirrors the named abstraction function `alphaAccounting : ActionLedger -> ActionLane -> Nat`,
//! the `0..9` lane-ordinal bijection, partition law (`submitted = camera + edit + control`),
//! fail-closed `MAX_ACTIONS_PER_TICK = 65_536` admission, per-lane `TickAttempt` reconciliation,
//! and powered negative controls from `proof/BitblockProof/AccountingRefinement.lean` against
//! live `bitblock::tick::{TickActionLedger, TickAttempt, TickOutcome}`.
//!
//! Explicitly bounded: verifies all 10 lane ordinals exhaustively, all 7 `TickAttempt` variants,
//! exact boundary `65,536` vs `65,537` admission traces, live `TickOutcome::new` reconciliation,
//! and powered swapped-lane / dropped-control / over-limit negative controls. Does not certify
//! `rustc`/LLVM code generation.

use bitblock::camera::Camera;
use bitblock::history::{History, world_tag};
use bitblock::program::{Edit, Operation};
use bitblock::tick::{
    ACTION_LANE_COUNT, LANE_AIM_DELETE, LANE_AIM_PLACE, LANE_CLEAR_MOTION, LANE_EXACT_EDIT,
    LANE_FIT_WORLD, LANE_LOOK_DELTA, LANE_MOVE_EDGE, LANE_RESIZE, LANE_TOGGLE_SIDEBAR,
    LANE_WHEEL_LINES, MAX_ACTIONS_PER_TICK, TickActionError, TickActionLedger, TickAttempt,
    TickCameraState, TickInputFlags, TickInputState, TickOutcome, TickProtocolError,
};
use bitblock::world::{Coord, World};

/// Named abstraction function `alpha_accounting`: projects a `TickActionLedger` to its 10 lane counts.
fn alpha_accounting(ledger: &TickActionLedger) -> [u32; ACTION_LANE_COUNT] {
    let mut out = [0u32; ACTION_LANE_COUNT];
    for (lane, slot) in out.iter_mut().enumerate() {
        *slot = ledger.lane(lane);
    }
    out
}

/// Mirror of Lean `BitblockProof.foldAccepted`: folds a lane sequence fail-closed at `65,536`.
fn lean_fold_accepted(actions: &[usize]) -> Option<[u32; ACTION_LANE_COUNT]> {
    let mut counts = [0u32; ACTION_LANE_COUNT];
    for (total, &lane) in actions.iter().enumerate() {
        if lane >= ACTION_LANE_COUNT || (total as u64) >= u64::from(MAX_ACTIONS_PER_TICK) {
            return None;
        }
        counts[lane] += 1;
    }
    Some(counts)
}

#[test]
fn exhaustive_lane_ordinals_attempt_attribution_and_partition_match_lean_model() {
    let all_lanes = [
        LANE_MOVE_EDGE,
        LANE_LOOK_DELTA,
        LANE_WHEEL_LINES,
        LANE_AIM_DELETE,
        LANE_AIM_PLACE,
        LANE_EXACT_EDIT,
        LANE_FIT_WORLD,
        LANE_TOGGLE_SIDEBAR,
        LANE_RESIZE,
        LANE_CLEAR_MOTION,
    ];
    assert_eq!(all_lanes.len(), ACTION_LANE_COUNT);
    for (expected_idx, &lane) in all_lanes.iter().enumerate() {
        assert_eq!(lane, expected_idx);
    }

    // Exhaustive 7-variant `TickAttempt` attribution matching Lean `attemptLane` & `forEditAttempts`
    let c1 = Coord::new(1, 0, 0);
    let c2 = Coord::new(2, 0, 0);
    let all_attempts = [
        TickAttempt::AimDeleteUnresolved,
        TickAttempt::AimDeleteExecuted(Edit::new(Operation::Off, c1)),
        TickAttempt::AimPlaceUnresolved,
        TickAttempt::AimPlaceRejectedObserver(Edit::new(Operation::On, c1)),
        TickAttempt::AimPlaceExecuted(Edit::new(Operation::On, c2)),
        TickAttempt::ExactRejectedObserver(Edit::new(Operation::On, c1)),
        TickAttempt::ExactExecuted(Edit::new(Operation::Toggle, c2)),
    ];
    let minimal = TickActionLedger::for_edit_attempts(&all_attempts);
    assert_eq!(minimal.lane(LANE_AIM_DELETE), 2);
    assert_eq!(minimal.lane(LANE_AIM_PLACE), 3);
    assert_eq!(minimal.lane(LANE_EXACT_EDIT), 2);
    assert_eq!(minimal.edit_lane_total(), all_attempts.len() as u32);
    assert_eq!(minimal.camera_lane_total(), 0);
    assert_eq!(minimal.control_lane_total(), 0);
    assert_eq!(minimal.submitted(), all_attempts.len() as u64);

    // Base + step mixed trace across all 10 lanes checking partition identity at every step
    let mut ledger = TickActionLedger::empty();
    let mut trace = Vec::new();
    for round in 0..3 {
        for &lane in &all_lanes {
            // Vary counts across lanes (round + 1) so no lane symmetry hides a misattribution
            for _ in 0..=(lane % 3 + round) {
                ledger.record(lane).unwrap();
                trace.push(lane);
                assert_eq!(
                    ledger.submitted(),
                    u64::from(ledger.camera_lane_total())
                        + u64::from(ledger.edit_lane_total())
                        + u64::from(ledger.control_lane_total())
                );
                assert_eq!(Some(alpha_accounting(&ledger)), lean_fold_accepted(&trace));
            }
        }
    }
}

#[test]
fn boundary_65536_admission_and_fail_closed_65537_overflow_match_lean_model() {
    let mut ledger = TickActionLedger::empty();
    let max = MAX_ACTIONS_PER_TICK as usize;
    assert_eq!(max, 65_536);

    // Fill to exact ceiling `65,536` cycling across all 10 lanes
    let mut trace = Vec::with_capacity(max);
    for i in 0..max {
        let lane = i % ACTION_LANE_COUNT;
        ledger.record(lane).unwrap();
        trace.push(lane);
    }
    assert_eq!(ledger.submitted(), u64::from(MAX_ACTIONS_PER_TICK));
    assert_eq!(Some(alpha_accounting(&ledger)), lean_fold_accepted(&trace));
    let snapshot_at_ceiling = alpha_accounting(&ledger);

    // Powered negative control (`foldAccepted_over_limit_none`):
    // 65,537th action in EVERY lane must fail closed without mutating the ledger!
    for lane in 0..ACTION_LANE_COUNT {
        assert_eq!(
            ledger.record(lane),
            Err(TickActionError::Overflow {
                limit: MAX_ACTIONS_PER_TICK
            })
        );
        assert_eq!(alpha_accounting(&ledger), snapshot_at_ceiling);
    }

    // Unknown lane (`>= ACTION_LANE_COUNT`) fails closed before mutation even on empty ledger
    let mut fresh = TickActionLedger::empty();
    assert_eq!(
        fresh.record(ACTION_LANE_COUNT),
        Err(TickActionError::UnknownLane {
            lane: ACTION_LANE_COUNT
        })
    );
    assert_eq!(fresh, TickActionLedger::empty());
}

#[test]
fn live_tick_outcome_reconciliation_and_powered_swapped_lane_and_control_lane_controls() {
    let world = World::new();
    let w_tag = world_tag(&world);
    let history = History::new(&world);
    let h_tag = history.head();
    let camera = Camera::for_observer_spawn();
    let cam_state = TickCameraState::capture(&camera);
    let input =
        TickInputState::new(TickInputFlags::default(), 0.0, 0.0, 0.0, 64, 64, 64, 64).unwrap();

    // 1. Powered negative control 1 (`swapped_edit_lanes_preserve_total_but_fail_reconciles`):
    // Two edit attempts (1 AimDeleteUnresolved + 1 AimPlaceUnresolved) vs ledger claiming 2 AimDelete + 0 AimPlace.
    let attempts = [
        TickAttempt::AimDeleteUnresolved,
        TickAttempt::AimPlaceUnresolved,
    ];
    let mut swapped = TickActionLedger::empty();
    swapped.record(LANE_AIM_DELETE).unwrap();
    swapped.record(LANE_AIM_DELETE).unwrap();
    // Same total (2 == 2), wrong per-lane split!
    assert_eq!(swapped.edit_lane_total(), attempts.len() as u32);
    let err = TickOutcome::new(
        None, 0, w_tag, h_tag, input, swapped, attempts, w_tag, h_tag, cam_state, None, None,
    )
    .unwrap_err();
    assert_eq!(
        err,
        TickProtocolError::ActionAccountingMismatch {
            lane: LANE_AIM_DELETE,
            submitted: 2,
            dispositions: 1,
        }
    );

    // Matching per-lane ledger succeeds cleanly
    let valid_edit_ledger = TickActionLedger::for_edit_attempts(&attempts);
    let ok_outcome = TickOutcome::new(
        None,
        0,
        w_tag,
        h_tag,
        input,
        valid_edit_ledger,
        attempts,
        w_tag,
        h_tag,
        cam_state,
        None,
        None,
    )
    .unwrap();
    assert_eq!(ok_outcome.actions(), valid_edit_ledger);

    // 2. Powered negative control 2 (`v2DropControl_collapses_control_actions_but_v3_distinguishes`):
    // Recording control actions (`toggleSidebar`, `resize`, `clearMotion`) strictly separates the
    // committed `TickOutcome` tag from an idle tick, whereas dropping control lanes collapses them!
    let idle = TickOutcome::new(
        None,
        0,
        w_tag,
        h_tag,
        input,
        TickActionLedger::empty(),
        [],
        w_tag,
        h_tag,
        cam_state,
        None,
        None,
    )
    .unwrap();
    let mut ctrl = TickActionLedger::empty();
    ctrl.record(LANE_TOGGLE_SIDEBAR).unwrap();
    ctrl.record(LANE_RESIZE).unwrap();
    ctrl.record(LANE_CLEAR_MOTION).unwrap();
    assert_eq!(ctrl.control_lane_total(), 3);
    let with_ctrl = TickOutcome::new(
        None,
        0,
        w_tag,
        h_tag,
        input,
        ctrl,
        [],
        w_tag,
        h_tag,
        cam_state,
        None,
        None,
    )
    .unwrap();
    assert_ne!(idle.tag(), with_ctrl.tag());
}
