namespace BitblockProof

/-!
`REFINE-1@accounting`: Formal accounting-lane refinement model corresponding to
`crates/core/src/tick.rs` (`TickActionLedger`, `TickAttempt`, `ACTION_LANE_COUNT = 10`,
`MAX_ACTIONS_PER_TICK = 65_536`, `LANE_MOVE_EDGE`..`LANE_CLEAR_MOTION`, `record`,
`for_edit_attempts`, `submitted`, `edit_lane_total`, `control_lane_total`,
`camera_lane_total`, and per-lane `validate`).

Establishes:
- Exact partition of `submitted` across the three disjoint lane groups
  (`cameraLaneTotal + editLaneTotal + controlLaneTotal`);
- Fail-closed admission at `maxActionsPerTick = 65536` before state mutation;
- Base + step trace fold preservation (every accepted action counted in its exact lane);
- Exact per-lane reconciliation between `TickActionLedger` and ordered `TickAttempt` lists;
- Powered negative controls: swapped edit lanes (same total, wrong split), v2 control-lane
  dropping collapsing control actions into idle, and over-limit batch refusal.
-/

/-- Maximum accepted actions admitted per tick (`crates/core/src/tick.rs::MAX_ACTIONS_PER_TICK`). -/
def maxActionsPerTick : Nat := 65536

/-- Exact 10 accepted-action lanes matching `LANE_MOVE_EDGE` (0) through `LANE_CLEAR_MOTION` (9). -/
inductive ActionLane where
  | moveEdge
  | lookDelta
  | wheelLines
  | aimDelete
  | aimPlace
  | exactEdit
  | fitWorld
  | toggleSidebar
  | resize
  | clearMotion
deriving DecidableEq, Repr

/-- Exact wire/ledger ordinal (`0..9`) matching `crates/core/src/tick.rs`. -/
def laneOrdinal : ActionLane → Fin 10
  | .moveEdge => ⟨0, by decide⟩
  | .lookDelta => ⟨1, by decide⟩
  | .wheelLines => ⟨2, by decide⟩
  | .aimDelete => ⟨3, by decide⟩
  | .aimPlace => ⟨4, by decide⟩
  | .exactEdit => ⟨5, by decide⟩
  | .fitWorld => ⟨6, by decide⟩
  | .toggleSidebar => ⟨7, by decide⟩
  | .resize => ⟨8, by decide⟩
  | .clearMotion => ⟨9, by decide⟩

/-- Inverse decoding from ordinal `0..9` back to `ActionLane`. -/
def laneOfOrdinal : Fin 10 → ActionLane
  | ⟨0, _⟩ => .moveEdge
  | ⟨1, _⟩ => .lookDelta
  | ⟨2, _⟩ => .wheelLines
  | ⟨3, _⟩ => .aimDelete
  | ⟨4, _⟩ => .aimPlace
  | ⟨5, _⟩ => .exactEdit
  | ⟨6, _⟩ => .fitWorld
  | ⟨7, _⟩ => .toggleSidebar
  | ⟨8, _⟩ => .resize
  | ⟨9, _⟩ => .clearMotion

theorem laneOfOrdinal_laneOrdinal (l : ActionLane) : laneOfOrdinal (laneOrdinal l) = l := by
  cases l <;> rfl

theorem laneOrdinal_laneOfOrdinal (i : Fin 10) : laneOrdinal (laneOfOrdinal i) = i := by
  rcases i with ⟨n, hn⟩
  match n, hn with
  | 0, _ | 1, _ | 2, _ | 3, _ | 4, _ | 5, _ | 6, _ | 7, _ | 8, _ | 9, _ => rfl

/-- Exact 7 edit-attempt disposition variants matching `crates/core/src/tick.rs::TickAttempt`. -/
inductive AttemptKind where
  | aimDeleteUnresolved
  | aimDeleteExecuted
  | aimPlaceUnresolved
  | aimPlaceRejectedObserver
  | aimPlaceExecuted
  | exactRejectedObserver
  | exactExecuted
deriving DecidableEq, Repr

/-- Exact lane attribution matching `TickActionLedger::for_edit_attempts`. -/
def attemptLane : AttemptKind → ActionLane
  | .aimDeleteUnresolved | .aimDeleteExecuted => .aimDelete
  | .aimPlaceUnresolved | .aimPlaceRejectedObserver | .aimPlaceExecuted => .aimPlace
  | .exactRejectedObserver | .exactExecuted => .exactEdit

/-- Concrete 10-counter per-tick ledger mirroring `TickActionLedger`. -/
structure ActionLedger where
  moveEdge : Nat
  lookDelta : Nat
  wheelLines : Nat
  aimDelete : Nat
  aimPlace : Nat
  exactEdit : Nat
  fitWorld : Nat
  toggleSidebar : Nat
  resize : Nat
  clearMotion : Nat
deriving DecidableEq, Repr

/-- Zeroed action ledger matching `TickActionLedger::empty`. -/
def emptyLedger : ActionLedger :=
  { moveEdge := 0, lookDelta := 0, wheelLines := 0, aimDelete := 0, aimPlace := 0,
    exactEdit := 0, fitWorld := 0, toggleSidebar := 0, resize := 0, clearMotion := 0 }

/-- Named abstraction function `alpha_accounting`: maps `ActionLedger` to its lane-count function. -/
def alphaAccounting (l : ActionLedger) : ActionLane → Nat
  | .moveEdge => l.moveEdge
  | .lookDelta => l.lookDelta
  | .wheelLines => l.wheelLines
  | .aimDelete => l.aimDelete
  | .aimPlace => l.aimPlace
  | .exactEdit => l.exactEdit
  | .fitWorld => l.fitWorld
  | .toggleSidebar => l.toggleSidebar
  | .resize => l.resize
  | .clearMotion => l.clearMotion

theorem actionLedger_ext (a b : ActionLedger)
    (h : ∀ lane, alphaAccounting a lane = alphaAccounting b lane) : a = b := by
  cases a; cases b
  have h0 := h .moveEdge
  have h1 := h .lookDelta
  have h2 := h .wheelLines
  have h3 := h .aimDelete
  have h4 := h .aimPlace
  have h5 := h .exactEdit
  have h6 := h .fitWorld
  have h7 := h .toggleSidebar
  have h8 := h .resize
  have h9 := h .clearMotion
  simp only [alphaAccounting] at h0 h1 h2 h3 h4 h5 h6 h7 h8 h9
  subst h0; subst h1; subst h2; subst h3; subst h4; subst h5; subst h6; subst h7; subst h8; subst h9
  rfl

/-- Total accepted actions matching `TickActionLedger::submitted`. -/
def submitted (l : ActionLedger) : Nat :=
  l.moveEdge + l.lookDelta + l.wheelLines + l.aimDelete + l.aimPlace +
    l.exactEdit + l.fitWorld + l.toggleSidebar + l.resize + l.clearMotion

/-- Edit-lane total matching `TickActionLedger::edit_lane_total`. -/
def editLaneTotal (l : ActionLedger) : Nat :=
  l.aimDelete + l.aimPlace + l.exactEdit

/-- Control-lane total matching `TickActionLedger::control_lane_total`. -/
def controlLaneTotal (l : ActionLedger) : Nat :=
  l.toggleSidebar + l.resize + l.clearMotion

/-- Camera-lane total matching `TickActionLedger::camera_lane_total`. -/
def cameraLaneTotal (l : ActionLedger) : Nat :=
  l.moveEdge + l.lookDelta + l.wheelLines + l.fitWorld

/-- Exact partition invariant: every accepted action belongs to exactly one of the three lane groups. -/
theorem submitted_eq_camera_plus_edit_plus_control (l : ActionLedger) :
    submitted l = cameraLaneTotal l + editLaneTotal l + controlLaneTotal l := by
  unfold submitted cameraLaneTotal editLaneTotal controlLaneTotal
  omega

/-- Increment one lane unconditionally (internal step used after bound check). -/
def bumpLane (l : ActionLedger) : ActionLane → ActionLedger
  | .moveEdge => { l with moveEdge := l.moveEdge + 1 }
  | .lookDelta => { l with lookDelta := l.lookDelta + 1 }
  | .wheelLines => { l with wheelLines := l.wheelLines + 1 }
  | .aimDelete => { l with aimDelete := l.aimDelete + 1 }
  | .aimPlace => { l with aimPlace := l.aimPlace + 1 }
  | .exactEdit => { l with exactEdit := l.exactEdit + 1 }
  | .fitWorld => { l with fitWorld := l.fitWorld + 1 }
  | .toggleSidebar => { l with toggleSidebar := l.toggleSidebar + 1 }
  | .resize => { l with resize := l.resize + 1 }
  | .clearMotion => { l with clearMotion := l.clearMotion + 1 }

/-- Fail-closed action admission matching `TickActionLedger::record`. -/
def recordAction (l : ActionLedger) (lane : ActionLane) : Option ActionLedger :=
  if submitted l < maxActionsPerTick then
    some (bumpLane l lane)
  else
    none

theorem alphaAccounting_bumpLane_same (l : ActionLedger) (lane : ActionLane) :
    alphaAccounting (bumpLane l lane) lane = alphaAccounting l lane + 1 := by
  cases lane <;> rfl

theorem alphaAccounting_bumpLane_other (l : ActionLedger) (lane other : ActionLane)
    (hne : other ≠ lane) :
    alphaAccounting (bumpLane l lane) other = alphaAccounting l other := by
  cases lane <;> cases other <;> first | exact False.elim (hne rfl) | rfl

theorem submitted_bumpLane (l : ActionLedger) (lane : ActionLane) :
    submitted (bumpLane l lane) = submitted l + 1 := by
  cases lane <;> simp [bumpLane, submitted] <;> omega

theorem recordAction_eq_some_iff (l : ActionLedger) (lane : ActionLane) (l' : ActionLedger) :
    recordAction l lane = some l' ↔ submitted l < maxActionsPerTick ∧ l' = bumpLane l lane := by
  unfold recordAction
  by_cases h : submitted l < maxActionsPerTick
  · simp [h, eq_comm]
  · simp [h]

theorem recordAction_over_limit_none (l : ActionLedger) (lane : ActionLane)
    (h : maxActionsPerTick ≤ submitted l) :
    recordAction l lane = none := by
  unfold recordAction
  have hnot : ¬ submitted l < maxActionsPerTick := by omega
  simp [hnot]

/-- Fail-closed sequential fold of a batch of accepted action lanes into a tick ledger. -/
def foldAcceptedFrom (l : ActionLedger) : List ActionLane → Option ActionLedger
  | [] => some l
  | lane :: rest =>
      match recordAction l lane with
      | none => none
      | some l' => foldAcceptedFrom l' rest

def foldAccepted (actions : List ActionLane) : Option ActionLedger :=
  foldAcceptedFrom emptyLedger actions

theorem foldAcceptedFrom_ok (l : ActionLedger) (actions : List ActionLane)
    (hbound : submitted l + actions.length ≤ maxActionsPerTick) :
    ∃ l', foldAcceptedFrom l actions = some l' ∧
      submitted l' = submitted l + actions.length ∧
      ∀ lane, alphaAccounting l' lane = alphaAccounting l lane + actions.count lane := by
  induction actions generalizing l with
  | nil =>
      refine ⟨l, rfl, by simp, fun lane => by simp⟩
  | cons a rest ih =>
      have hlt : submitted l < maxActionsPerTick := by simp at hbound; omega
      have hstep : recordAction l a = some (bumpLane l a) := by
        simp [recordAction, hlt]
      have hnext : submitted (bumpLane l a) + rest.length ≤ maxActionsPerTick := by
        rw [submitted_bumpLane]; simp at hbound; omega
      rcases ih (bumpLane l a) hnext with ⟨l', hfold, hsub, hlane⟩
      refine ⟨l', ?_, ?_, ?_⟩
      · simp [foldAcceptedFrom, hstep, hfold]
      · rw [hsub, submitted_bumpLane]; simp; omega
      · intro lane
        rw [hlane lane]
        by_cases heq : lane = a
        · subst heq
          rw [alphaAccounting_bumpLane_same, List.count_cons_self]
          omega
        · rw [alphaAccounting_bumpLane_other l a lane heq, List.count_cons_of_ne (Ne.symm heq)]

theorem foldAcceptedFrom_over_limit_none (l : ActionLedger) (actions : List ActionLane)
    (hle : submitted l ≤ maxActionsPerTick)
    (hgt : maxActionsPerTick < submitted l + actions.length) :
    foldAcceptedFrom l actions = none := by
  induction actions generalizing l with
  | nil =>
      simp at hgt
      omega
  | cons a rest ih =>
      by_cases hlt : submitted l < maxActionsPerTick
      · have hstep : recordAction l a = some (bumpLane l a) := by
          simp [recordAction, hlt]
        have hle' : submitted (bumpLane l a) ≤ maxActionsPerTick := by
          rw [submitted_bumpLane]; omega
        have hgt' : maxActionsPerTick < submitted (bumpLane l a) + rest.length := by
          rw [submitted_bumpLane]; simp at hgt; omega
        simp [foldAcceptedFrom, hstep, ih (bumpLane l a) hle' hgt']
      · have hnone : recordAction l a = none := recordAction_over_limit_none l a (by omega)
        simp [foldAcceptedFrom, hnone]

theorem foldAccepted_ok (actions : List ActionLane) (h : actions.length ≤ maxActionsPerTick) :
    ∃ l', foldAccepted actions = some l' ∧
      submitted l' = actions.length ∧
      ∀ lane, alphaAccounting l' lane = actions.count lane := by
  have hsub0 : submitted emptyLedger = 0 := rfl
  have hbound : submitted emptyLedger + actions.length ≤ maxActionsPerTick := by omega
  rcases foldAcceptedFrom_ok emptyLedger actions hbound with ⟨l', hfold, hsub, hlane⟩
  refine ⟨l', hfold, by omega, fun lane => ?_⟩
  have h0 : alphaAccounting emptyLedger lane = 0 := by cases lane <;> rfl
  simpa [h0] using hlane lane

/-- Powered negative control: any batch exceeding `maxActionsPerTick` (65,536) fails closed. -/
theorem foldAccepted_over_limit_none (actions : List ActionLane)
    (h : maxActionsPerTick < actions.length) :
    foldAccepted actions = none := by
  apply foldAcceptedFrom_over_limit_none emptyLedger actions
  · decide
  · simpa [submitted, emptyLedger] using h

/-- Minimal ledger consistent with an ordered attempt list matching `TickActionLedger::for_edit_attempts`. -/
def forEditAttempts : List AttemptKind → ActionLedger
  | [] => emptyLedger
  | a :: rest => bumpLane (forEditAttempts rest) (attemptLane a)

theorem forEditAttempts_camera_zero (attempts : List AttemptKind) :
    cameraLaneTotal (forEditAttempts attempts) = 0 := by
  induction attempts with
  | nil => rfl
  | cons a rest ih =>
      cases a <;> simpa [forEditAttempts, attemptLane, bumpLane, cameraLaneTotal] using ih

theorem forEditAttempts_control_zero (attempts : List AttemptKind) :
    controlLaneTotal (forEditAttempts attempts) = 0 := by
  induction attempts with
  | nil => rfl
  | cons a rest ih =>
      cases a <;> simpa [forEditAttempts, attemptLane, bumpLane, controlLaneTotal] using ih

theorem forEditAttempts_editLaneTotal_eq_length (attempts : List AttemptKind) :
    editLaneTotal (forEditAttempts attempts) = attempts.length := by
  induction attempts with
  | nil => rfl
  | cons a rest ih =>
      unfold editLaneTotal at ih
      cases a <;>
        (simp only [forEditAttempts, attemptLane, bumpLane, editLaneTotal, List.length_cons]; omega)

theorem forEditAttempts_laneCount_eq_count (attempts : List AttemptKind) (lane : ActionLane) :
    alphaAccounting (forEditAttempts attempts) lane = (attempts.map attemptLane).count lane := by
  induction attempts with
  | nil => cases lane <;> rfl
  | cons a rest ih =>
      simp only [forEditAttempts, List.map_cons]
      by_cases heq : lane = attemptLane a
      · subst heq
        rw [alphaAccounting_bumpLane_same, ih, List.count_cons_self]
      · rw [alphaAccounting_bumpLane_other _ _ _ heq, ih, List.count_cons_of_ne (Ne.symm heq)]

/-- Exact per-lane reconciliation matching `TickActionLedger::validate`. -/
def Reconciles (l : ActionLedger) (attempts : List AttemptKind) : Prop :=
  let expected := forEditAttempts attempts
  l.aimDelete = expected.aimDelete ∧
    l.aimPlace = expected.aimPlace ∧
    l.exactEdit = expected.exactEdit

/-- Complete ledger admission invariant matching `TickActionLedger::validate`. -/
def ValidLedger (l : ActionLedger) (attempts : List AttemptKind) : Prop :=
  submitted l ≤ maxActionsPerTick ∧ Reconciles l attempts

theorem validLedger_editLaneTotal_eq_attempts_length (l : ActionLedger) (attempts : List AttemptKind)
    (hv : ValidLedger l attempts) :
    editLaneTotal l = attempts.length := by
  rcases hv with ⟨_, hd, hp, he⟩
  have hlen := forEditAttempts_editLaneTotal_eq_length attempts
  unfold editLaneTotal at *
  omega

/-- Every valid ledger has `attempts.length ≤ submitted l ≤ maxActionsPerTick`. -/
theorem validLedger_attempts_le_submitted (l : ActionLedger) (attempts : List AttemptKind)
    (hv : ValidLedger l attempts) :
    attempts.length ≤ submitted l ∧ submitted l ≤ maxActionsPerTick := by
  have hedit := validLedger_editLaneTotal_eq_attempts_length l attempts hv
  have hpart := submitted_eq_camera_plus_edit_plus_control l
  exact ⟨by omega, hv.1⟩

/-- Powered negative control 1 (ACCOUNT-1 swapped-lane guard):
    A ledger claiming 2 aim-deletes and 0 aim-places against `[aimDeleteExecuted, aimPlaceExecuted]`
    preserves `editLaneTotal = 2` yet strictly FAILS `Reconciles`. -/
theorem swapped_edit_lanes_preserve_total_but_fail_reconciles :
    let attempts : List AttemptKind := [.aimDeleteExecuted, .aimPlaceExecuted]
    let swapped : ActionLedger := { emptyLedger with aimDelete := 2, aimPlace := 0 }
    editLaneTotal swapped = attempts.length ∧ ¬ Reconciles swapped attempts := by
  dsimp only
  unfold Reconciles
  decide

/-- Powered negative control 2 (ACCOUNT-1 v3 control-lane visibility):
    A v2-style dropper that ignores `toggleSidebar`, `resize`, and `clearMotion` collapses
    control actions to `emptyLedger`, whereas `foldAccepted` separates every control lane from idle. -/
def v2DropControl (actions : List ActionLane) : List ActionLane :=
  actions.filter fun
    | .toggleSidebar | .resize | .clearMotion => false
    | _ => true

theorem v2DropControl_collapses_control_actions_but_v3_distinguishes :
    let batch : List ActionLane := [.toggleSidebar, .resize, .clearMotion]
    foldAccepted (v2DropControl batch) = some emptyLedger ∧
    ∃ l, foldAccepted batch = some l ∧
      l ≠ emptyLedger ∧
      controlLaneTotal l = 3 ∧
      submitted l = 3 := by
  refine ⟨rfl, ?_⟩
  refine ⟨{ emptyLedger with toggleSidebar := 1, resize := 1, clearMotion := 1 }, ?_, ?_, rfl, rfl⟩
  · decide
  · decide

end BitblockProof
