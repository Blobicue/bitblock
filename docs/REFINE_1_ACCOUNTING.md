# REFINE-1@accounting: Rust-to-Lean tick-action accounting correspondence v1

This record resolves `PS-023:REFINE-1@accounting` as **BOUNDED** (keeping `PS-021:REFINE-1@base=READY`
and `PS-025:REFINE-1@vm=LOCKED` until `VM-1` exists). It connects the verified Lean 4 mathematical
tick-action accounting theorems directly to the live Rust `bitblock::tick::{TickActionLedger,
TickAttempt, TickOutcome}` implementation via a named abstraction function, exhaustive 10-lane and
7-attempt attribution checks, exact `65,536` vs. `65,537` fail-closed boundary traces, live
`TickOutcome::new` reconciliation, and powered negative controls.

## Provenance and verified capsules

- **Merged `origin/main` subject:** commit `e22312bfb778adce1cc7ba3917e368765e91d8e9`
  (tree `7481353f1996ea5d1883f44a36136884da80bb2c`, PR #18, CI runs `44994236191` & `44994240648`,
  10/10 jobs green).
- **Lean 4 `--trust=0` proof capsule (`proof/BitblockProof/AccountingRefinement.lean`, 21 theorems,
  0 warnings, 0 `sorry`/custom axioms under Lean `v4.33.0`):**
  - `ActionLane` / `laneOrdinal` / `laneOfOrdinal`: exact `0..9` bijection (`laneOfOrdinal_laneOrdinal`,
    `laneOrdinal_laneOfOrdinal`).
  - `AttemptKind` / `attemptLane` / `forEditAttempts`: exact 7-variant edit-attempt attribution and
    minimal ledger laws (`forEditAttempts_camera_zero`, `forEditAttempts_control_zero`,
    `forEditAttempts_editLaneTotal_eq_length`, `forEditAttempts_laneCount_eq_count`).
  - `ActionLedger` / `alphaAccounting`: named abstraction function and extensionality
    (`actionLedger_ext`), plus exact lane-group partition (`submitted_eq_camera_plus_edit_plus_control`:
    `submitted = cameraLaneTotal + editLaneTotal + controlLaneTotal`).
  - Fail-closed admission & sequential trace fold (`maxActionsPerTick = 65536`): `recordAction_eq_some_iff`,
    `recordAction_over_limit_none`, `foldAcceptedFrom_ok`, `foldAcceptedFrom_over_limit_none`,
    `foldAccepted_ok`, `foldAccepted_over_limit_none`.
  - Per-lane reconciliation (`Reconciles`, `ValidLedger`): `validLedger_editLaneTotal_eq_attempts_length`,
    `validLedger_attempts_le_submitted`.
  - Powered negative controls: `swapped_edit_lanes_preserve_total_but_fail_reconciles`,
    `v2DropControl_collapses_control_actions_but_v3_distinguishes`, and `foldAccepted_over_limit_none`.
- **Rust correspondence harness:** `tests/accounting_refinement_correspondence.rs` (3 default-feature
  integration tests executed in both `quick` and `full` tiers on Windows and Linux CI).

## Named abstraction function and correspondence map

| Lean 4 model definition | Rust runtime counterpart (`crates/core/src/tick.rs`) | Correspondence check in `tests/accounting_refinement_correspondence.rs` |
| --- | --- | --- |
| `alphaAccounting : ActionLedger -> ActionLane -> Nat` | `alpha_accounting(&TickActionLedger) -> [u32; 10]` | Step-by-step equality against `lean_fold_accepted(&trace)` across asymmetric mixed multi-round traces covering all 10 lanes. |
| `laneOrdinal` / `laneOfOrdinal` (`Fin 10`) | `LANE_MOVE_EDGE` (0) .. `LANE_CLEAR_MOTION` (9), `ACTION_LANE_COUNT = 10` | **Exhaustive** check over all 10 lane indices plus unknown-lane (`lane = 10`) fail-closed refusal (`TickActionError::UnknownLane`) with zero mutation. |
| `attemptLane` / `forEditAttempts` (`AttemptKind`) | `TickAttempt` (all 7 variants) & `TickActionLedger::for_edit_attempts` | **Exhaustive** attribution over all 7 `TickAttempt` variants verifying exact `aim_delete = 2`, `aim_place = 3`, `exact_edit = 2`, and zero camera/control counts. |
| `submitted_eq_camera_plus_edit_plus_control` | `submitted()`, `camera_lane_total()`, `edit_lane_total()`, `control_lane_total()` | Asserted at every step of the mixed 10-lane trace and at the `65,536` ceiling. |
| `foldAccepted_ok` / `foldAccepted_over_limit_none` | `TickActionLedger::record` with `MAX_ACTIONS_PER_TICK = 65_536` | Exact `65,536` saturation matches `lean_fold_accepted`; 65,537th action fails closed (`TickActionError::Overflow`) across **every** lane `0..9` leaving the ledger snapshot unchanged. |
| `Reconciles` / `ValidLedger` + powered negative controls | `TickOutcome::new` (`ActionAccountingMismatch`) & canonical `TickOutcome::tag` | Swapped edit lanes (`2 AimDelete + 0 AimPlace` vs. `[AimDeleteUnresolved, AimPlaceUnresolved]`) preserve `edit_lane_total = 2` yet reject with `ActionAccountingMismatch`; recording `toggleSidebar`/`resize`/`clearMotion` strictly separates `TickOutcome::tag` from an idle tick. |

## Explicit boundedness and non-claims

`BOUNDED` is mandatory per `CAPABILITY_TREE.md`:
- The 10 lane ordinals, 7 `TickAttempt` variants, and exact `65,536 -> 65,537` boundary refusal
  across all 10 lanes are verified exhaustively against the live Rust types, while the multi-step
  trace and `TickOutcome::new` checks are finite differential witnesses paired with universal Lean 4
  proofs over arbitrary lists (`List ActionLane`, `List AttemptKind`).
- Neither Lean 4 nor these tests formally extract or verify `rustc`/LLVM compiled machine code,
  OS window-event capture completeness outside accepted actions, or consensus ordering.
- VM refinement (`REFINE-1@vm`) remains `LOCKED` on `VM-1`.
