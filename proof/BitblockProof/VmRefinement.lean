import BitblockProof.World

namespace BitblockProof

/-!
`REFINE-1@vm`: Formal bounded spatial-interpreter refinement model corresponding to
`crates/research/src/vm_1.rs` (`LocalSpatialVmV1`, `VmStepTraceV1`, `VmTickExecutionReceiptV1`,
`step_opcode`, `evaluate_program_traced`) and `crates/research/src/vm_d1.rs` (`VmSpecD1`,
`VmTrapV1`, `VmOutputRegisterV1`, `schedule_and_evaluate_model`).

Establishes under `lake env lean --trust=0` with zero custom axioms:
- Exact 8-opcode bijection (`VmOpcode` <-> `Fin 8`) and strictly positive per-opcode gas schedule;
- Checked signed 64-bit arithmetic (`[-2^63, 2^63 - 1]`) overflow trap soundness plus a powered
  negative control showing wrapping 64-bit addition silently wraps `i64::MAX + 1` to `i64::MIN`;
- Total bounded interpreter `evalProgram`: guaranteed finite termination (`<= prog.length` steps),
  gas monotonicity, `gasUsed <= preExecutionOpGas` and `gasUsed <= gasLimit`, stack-depth ceiling
  (`<= maxDepth`), and `InVmI64` stack-element preservation;
- Atomic rollback soundness (`trap -> emittedWrites = [] ∧ postWorld = preWorld`) plus a powered
  partial-write-leak negative control;
- Bedrock and program-frame write-exclusion preservation, plus disjoint-write commutation
  (`SameWorld (applyWrites (applyWrites w ws1) ws2) (applyWrites (applyWrites w ws2) ws1)`).

This is an abstract mathematical model over Lean `Int` and finite lists. It does not formally
verify `rustc`/LLVM compiled machine code, SHA-256 hashing, or shared consensus authority.
-/

/-! ## Section 1: Exact 8-Opcode Bijection (`VmOpcode` <-> `Fin 8`) & Gas Schedule -/

/-- Exact 8 `LANGUAGE-R1` v1 spatial opcodes mirroring `SpatialOpcodeV1`. -/
inductive VmOpcode where
  | nop
  | pushOne
  | pushZero
  | add
  | sub
  | dup
  | swap
  | halt
deriving DecidableEq, Repr

/-- Exact wire code (`0..7`) matching `SpatialOpcodeV1::code`. -/
def opcodeCode : VmOpcode → Fin 8
  | .nop => ⟨0, by decide⟩
  | .pushOne => ⟨1, by decide⟩
  | .pushZero => ⟨2, by decide⟩
  | .add => ⟨3, by decide⟩
  | .sub => ⟨4, by decide⟩
  | .dup => ⟨5, by decide⟩
  | .swap => ⟨6, by decide⟩
  | .halt => ⟨7, by decide⟩

/-- Inverse decoding from `Fin 8` back to `VmOpcode`. -/
def opcodeOfCode : Fin 8 → VmOpcode
  | ⟨0, _⟩ => .nop
  | ⟨1, _⟩ => .pushOne
  | ⟨2, _⟩ => .pushZero
  | ⟨3, _⟩ => .add
  | ⟨4, _⟩ => .sub
  | ⟨5, _⟩ => .dup
  | ⟨6, _⟩ => .swap
  | ⟨7, _⟩ => .halt

theorem opcodeOfCode_opcodeCode (op : VmOpcode) : opcodeOfCode (opcodeCode op) = op := by
  cases op <;> rfl

theorem opcodeCode_opcodeOfCode (i : Fin 8) : opcodeCode (opcodeOfCode i) = i := by
  rcases i with ⟨n, hn⟩
  match n, hn with
  | 0, _ | 1, _ | 2, _ | 3, _ | 4, _ | 5, _ | 6, _ | 7, _ => rfl

/-- Exact per-opcode gas schedule matching `DEFAULT_VM_D1_OPCODE_GAS = [1, 2, 2, 3, 3, 2, 2, 1]`. -/
def opcodeGas : VmOpcode → Nat
  | .nop => 1
  | .pushOne => 2
  | .pushZero => 2
  | .add => 3
  | .sub => 3
  | .dup => 2
  | .swap => 2
  | .halt => 1

/-- Default gas per emitted explicit output voxel write matching `DEFAULT_VM_D1_GAS_PER_OUTPUT_BIT`. -/
def gasPerOutputBit : Nat := 10

theorem opcodeGas_pos (op : VmOpcode) : 0 < opcodeGas op := by
  cases op <;> decide

theorem opcodeGas_le_three (op : VmOpcode) : opcodeGas op ≤ 3 := by
  cases op <;> decide

/-! ## Section 2: Checked Signed 64-Bit Arithmetic (`i64`) & Powered Negative Control -/

/-- Rust `i64::MIN = -2^63`. -/
def i64Min : Int := -9223372036854775808

/-- Rust `i64::MAX = 2^63 - 1`. -/
def i64Max : Int := 9223372036854775807

/-- Predicate stating `v : Int` lies within signed 64-bit bounds `[-2^63, 2^63 - 1]`. -/
def InVmI64 (v : Int) : Prop :=
  i64Min ≤ v ∧ v ≤ i64Max

instance (v : Int) : Decidable (InVmI64 v) :=
  inferInstanceAs (Decidable (i64Min ≤ v ∧ v ≤ i64Max))

/-- Checked signed 64-bit addition matching `i64::checked_add`. -/
def checkedAdd (a b : Int) : Option Int :=
  let s := a + b
  if InVmI64 s then some s else none

/-- Checked signed 64-bit subtraction matching `i64::checked_sub`. -/
def checkedSub (a b : Int) : Option Int :=
  let d := a - b
  if InVmI64 d then some d else none

theorem checkedAdd_sound (a b s : Int) (h : checkedAdd a b = some s) :
    s = a + b ∧ InVmI64 s := by
  unfold checkedAdd at h
  by_cases hr : InVmI64 (a + b)
  · simp [hr] at h
    subst h
    exact ⟨rfl, hr⟩
  · simp [hr] at h

theorem checkedAdd_none_iff (a b : Int) : checkedAdd a b = none ↔ ¬ InVmI64 (a + b) := by
  unfold checkedAdd
  by_cases hr : InVmI64 (a + b) <;> simp [hr]

theorem checkedSub_sound (a b d : Int) (h : checkedSub a b = some d) :
    d = a - b ∧ InVmI64 d := by
  unfold checkedSub at h
  by_cases hr : InVmI64 (a - b)
  · simp [hr] at h
    subst h
    exact ⟨rfl, hr⟩
  · simp [hr] at h

theorem checkedSub_none_iff (a b : Int) : checkedSub a b = none ↔ ¬ InVmI64 (a - b) := by
  unfold checkedSub
  by_cases hr : InVmI64 (a - b) <;> simp [hr]

theorem checkedAdd_max_one_traps : checkedAdd i64Max 1 = none := by
  decide

theorem checkedSub_min_one_traps : checkedSub i64Min 1 = none := by
  decide

/-- Powered negative control: two's-complement wrapping 64-bit addition (`i64::wrapping_add`). -/
def wrappingAdd64 (a b : Int) : Int :=
  let m : Int := 18446744073709551616
  let bias : Int := 9223372036854775808
  ((a + b + bias) % m) - bias

/-- Powered negative control: wrapping addition silently wraps `i64Max + 1` to `i64Min` instead of trapping. -/
theorem wrappingAdd64_silently_wraps_max :
    wrappingAdd64 i64Max 1 = i64Min ∧ checkedAdd i64Max 1 = none := by
  decide

/-! ## Section 3: Bounded Interpreter, Termination, Gas Ceiling & Stack-Depth Invariants -/

/-- Exact 7 deterministic trap classes mirroring `VmTrapV1`. -/
inductive VmTrap where
  | stackUnderflow
  | stackOverflow
  | arithmeticOverflow
  | gasExhausted
  | missingHalt
  | forbiddenOutputTarget (coord : Coord)
  | outputWriteConflict (coord : Coord)
deriving DecidableEq, Repr

/-- Canonical wire status byte (`1..=7`; `0` is reserved for `Halted`). -/
def trapCode : VmTrap → Nat
  | .stackUnderflow => 1
  | .stackOverflow => 2
  | .arithmeticOverflow => 3
  | .gasExhausted => 4
  | .missingHalt => 5
  | .forbiddenOutputTarget _ => 6
  | .outputWriteConflict _ => 7

theorem trapCode_bounds (t : VmTrap) : 1 ≤ trapCode t ∧ trapCode t ≤ 7 := by
  cases t <;> simp [trapCode]

/-- Single-opcode stack transition (head of list = top of stack) mirroring `vm_1::step_opcode`. -/
def stepOpcode (stack : List Int) (op : VmOpcode) (maxDepth : Nat) :
    Except VmTrap (Bool × List Int) :=
  match op with
  | .nop => .ok (false, stack)
  | .pushOne =>
      if stack.length ≥ maxDepth then .error .stackOverflow else .ok (false, 1 :: stack)
  | .pushZero =>
      if stack.length ≥ maxDepth then .error .stackOverflow else .ok (false, 0 :: stack)
  | .add =>
      match stack with
      | rhs :: lhs :: rest =>
          match checkedAdd lhs rhs with
          | some s => .ok (false, s :: rest)
          | none => .error .arithmeticOverflow
      | _ => .error .stackUnderflow
  | .sub =>
      match stack with
      | rhs :: lhs :: rest =>
          match checkedSub lhs rhs with
          | some d => .ok (false, d :: rest)
          | none => .error .arithmeticOverflow
      | _ => .error .stackUnderflow
  | .dup =>
      match stack with
      | top :: rest =>
          if stack.length ≥ maxDepth then .error .stackOverflow
          else .ok (false, top :: top :: rest)
      | [] => .error .stackUnderflow
  | .swap =>
      match stack with
      | a :: b :: rest => .ok (false, b :: a :: rest)
      | _ => .error .stackUnderflow
  | .halt => .ok (true, stack)

theorem stepOpcode_preserves_depth_bound (stack stack' : List Int) (op : VmOpcode)
    (maxDepth : Nat) (halted : Bool)
    (hlen : stack.length ≤ maxDepth)
    (hstep : stepOpcode stack op maxDepth = .ok (halted, stack')) :
    stack'.length ≤ maxDepth := by
  cases op with
  | nop =>
      simp [stepOpcode] at hstep
      rcases hstep with ⟨_, rfl⟩
      exact hlen
  | pushOne =>
      simp [stepOpcode] at hstep
      split at hstep
      · cases hstep
      · rcases hstep with ⟨_, rfl⟩
        simp only [List.length_cons]
        omega
  | pushZero =>
      simp [stepOpcode] at hstep
      split at hstep
      · cases hstep
      · rcases hstep with ⟨_, rfl⟩
        simp only [List.length_cons]
        omega
  | add =>
      cases stack with
      | nil => simp [stepOpcode] at hstep
      | cons rhs tl =>
          cases tl with
          | nil => simp [stepOpcode] at hstep
          | cons lhs rest =>
              simp [stepOpcode] at hstep
              cases hadd : checkedAdd lhs rhs with
              | none => simp [hadd] at hstep
              | some s =>
                  simp [hadd] at hstep
                  rcases hstep with ⟨_, rfl⟩
                  simp only [List.length_cons] at hlen ⊢
                  omega
  | sub =>
      cases stack with
      | nil => simp [stepOpcode] at hstep
      | cons rhs tl =>
          cases tl with
          | nil => simp [stepOpcode] at hstep
          | cons lhs rest =>
              simp [stepOpcode] at hstep
              cases hsub : checkedSub lhs rhs with
              | none => simp [hsub] at hstep
              | some d =>
                  simp [hsub] at hstep
                  rcases hstep with ⟨_, rfl⟩
                  simp only [List.length_cons] at hlen ⊢
                  omega
  | dup =>
      cases stack with
      | nil => simp [stepOpcode] at hstep
      | cons top rest =>
          simp [stepOpcode] at hstep
          split at hstep
          · cases hstep
          · rcases hstep with ⟨_, rfl⟩
            simp only [List.length_cons] at *
            omega
  | swap =>
      cases stack with
      | nil => simp [stepOpcode] at hstep
      | cons a tl =>
          cases tl with
          | nil => simp [stepOpcode] at hstep
          | cons b rest =>
              simp [stepOpcode] at hstep
              rcases hstep with ⟨_, rfl⟩
              simp only [List.length_cons] at hlen ⊢
              exact hlen
  | halt =>
      simp [stepOpcode] at hstep
      rcases hstep with ⟨_, rfl⟩
      exact hlen

theorem stepOpcode_preserves_inI64 (stack stack' : List Int) (op : VmOpcode)
    (maxDepth : Nat) (halted : Bool)
    (hin : ∀ v ∈ stack, InVmI64 v)
    (hstep : stepOpcode stack op maxDepth = .ok (halted, stack')) :
    ∀ v ∈ stack', InVmI64 v := by
  cases op with
  | nop =>
      simp [stepOpcode] at hstep
      rcases hstep with ⟨_, rfl⟩
      exact hin
  | pushOne =>
      simp [stepOpcode] at hstep
      split at hstep
      · cases hstep
      · rcases hstep with ⟨_, rfl⟩
        intro v hv
        simp only [List.mem_cons] at hv
        rcases hv with rfl | hrest
        · decide
        · exact hin v hrest
  | pushZero =>
      simp [stepOpcode] at hstep
      split at hstep
      · cases hstep
      · rcases hstep with ⟨_, rfl⟩
        intro v hv
        simp only [List.mem_cons] at hv
        rcases hv with rfl | hrest
        · decide
        · exact hin v hrest
  | add =>
      cases stack with
      | nil => simp [stepOpcode] at hstep
      | cons rhs tl =>
          cases tl with
          | nil => simp [stepOpcode] at hstep
          | cons lhs rest =>
              simp [stepOpcode] at hstep
              cases hadd : checkedAdd lhs rhs with
              | none => simp [hadd] at hstep
              | some s =>
                  simp [hadd] at hstep
                  rcases hstep with ⟨_, rfl⟩
                  intro v hv
                  rw [List.mem_cons] at hv
                  rcases hv with rfl | hrest
                  · exact (checkedAdd_sound lhs rhs _ hadd).2
                  · exact hin v (List.mem_cons_of_mem rhs (List.mem_cons_of_mem lhs hrest))
  | sub =>
      cases stack with
      | nil => simp [stepOpcode] at hstep
      | cons rhs tl =>
          cases tl with
          | nil => simp [stepOpcode] at hstep
          | cons lhs rest =>
              simp [stepOpcode] at hstep
              cases hsub : checkedSub lhs rhs with
              | none => simp [hsub] at hstep
              | some d =>
                  simp [hsub] at hstep
                  rcases hstep with ⟨_, rfl⟩
                  intro v hv
                  rw [List.mem_cons] at hv
                  rcases hv with rfl | hrest
                  · exact (checkedSub_sound lhs rhs _ hsub).2
                  · exact hin v (List.mem_cons_of_mem rhs (List.mem_cons_of_mem lhs hrest))
  | dup =>
      cases stack with
      | nil => simp [stepOpcode] at hstep
      | cons top rest =>
          simp [stepOpcode] at hstep
          split at hstep
          · cases hstep
          · rcases hstep with ⟨_, rfl⟩
            intro v hv
            rw [List.mem_cons] at hv
            rcases hv with rfl | hrest
            · exact hin _ (List.mem_cons_self ..)
            · exact hin v hrest
  | swap =>
      cases stack with
      | nil => simp [stepOpcode] at hstep
      | cons a tl =>
          cases tl with
          | nil => simp [stepOpcode] at hstep
          | cons b rest =>
              simp [stepOpcode] at hstep
              rcases hstep with ⟨_, rfl⟩
              intro v hv
              rw [List.mem_cons, List.mem_cons] at hv
              rcases hv with rfl | rfl | hrest
              · exact hin _ (List.mem_cons_of_mem a (List.mem_cons_self ..))
              · exact hin _ (List.mem_cons_self ..)
              · exact hin v (List.mem_cons_of_mem a (List.mem_cons_of_mem b hrest))
  | halt =>
      simp [stepOpcode] at hstep
      rcases hstep with ⟨_, rfl⟩
      exact hin

/-- Static sum of opcode gas matching the instruction half of `pre_execution_work_upper_bound`. -/
def preExecutionOpGas : List VmOpcode → Nat
  | [] => 0
  | op :: rest => opcodeGas op + preExecutionOpGas rest

/-- Exact static work upper bound mirroring `VmSpecD1::pre_execution_work_upper_bound`. -/
def preExecutionWorkUpperBound (prog : List VmOpcode) (outputBits : Nat) : Nat :=
  preExecutionOpGas prog + outputBits * gasPerOutputBit

/-- Bounded evaluation outcome returned by `evalLoop` / `evalProgram`. -/
structure VmEvalResult where
  outcome : Except VmTrap (List Int)
  gasUsed : Nat
  stepsExecuted : Nat
deriving Repr

/-- Structurally recursive (provably terminating) instruction loop mirroring `evaluate_program_traced`. -/
def evalLoop (prog : List VmOpcode) (gasLimit maxDepth gasUsed stepsDone : Nat)
    (stack : List Int) : VmEvalResult :=
  match prog with
  | [] => { outcome := .error .missingHalt, gasUsed := gasUsed, stepsExecuted := stepsDone }
  | op :: rest =>
      let nextGas := gasUsed + opcodeGas op
      if nextGas > gasLimit then
        { outcome := .error .gasExhausted, gasUsed := gasUsed, stepsExecuted := stepsDone + 1 }
      else
        match stepOpcode stack op maxDepth with
        | .error trap =>
            { outcome := .error trap, gasUsed := nextGas, stepsExecuted := stepsDone + 1 }
        | .ok (true, stack') =>
            { outcome := .ok stack', gasUsed := nextGas, stepsExecuted := stepsDone + 1 }
        | .ok (false, stack') =>
            evalLoop rest gasLimit maxDepth nextGas (stepsDone + 1) stack'

/-- Top-level bounded program evaluator starting from empty stack and zero gas. -/
def evalProgram (prog : List VmOpcode) (gasLimit maxDepth : Nat) : VmEvalResult :=
  evalLoop prog gasLimit maxDepth 0 0 []

theorem evalLoop_steps_le_length (prog : List VmOpcode) (gasLimit maxDepth gasUsed stepsDone : Nat)
    (stack : List Int) :
    (evalLoop prog gasLimit maxDepth gasUsed stepsDone stack).stepsExecuted ≤
      stepsDone + prog.length := by
  induction prog generalizing gasUsed stepsDone stack with
  | nil =>
      simp [evalLoop]
  | cons op rest ih =>
      simp only [evalLoop, List.length_cons]
      split
      · simp only
        omega
      · cases hstep : stepOpcode stack op maxDepth with
        | error trap =>
            simp only
            omega
        | ok pair =>
            rcases pair with ⟨halted, stack'⟩
            cases halted
            · simp only
              have h := ih (gasUsed + opcodeGas op) (stepsDone + 1) stack'
              omega
            · simp only
              omega

/-- Guaranteed finite termination: `evalProgram` executes at most `prog.length` steps. -/
theorem evalProgram_terminates_bounded (prog : List VmOpcode) (gasLimit maxDepth : Nat) :
    (evalProgram prog gasLimit maxDepth).stepsExecuted ≤ prog.length := by
  have h := evalLoop_steps_le_length prog gasLimit maxDepth 0 0 []
  simpa [evalProgram] using h

theorem evalLoop_gas_monotone (prog : List VmOpcode) (gasLimit maxDepth gasUsed stepsDone : Nat)
    (stack : List Int) :
    gasUsed ≤ (evalLoop prog gasLimit maxDepth gasUsed stepsDone stack).gasUsed := by
  induction prog generalizing gasUsed stepsDone stack with
  | nil => simp [evalLoop]
  | cons op rest ih =>
      simp only [evalLoop]
      split
      · exact Nat.le_refl _
      · cases hstep : stepOpcode stack op maxDepth with
        | error trap =>
            simp only
            omega
        | ok pair =>
            rcases pair with ⟨halted, stack'⟩
            cases halted
            · simp only
              have h := ih (gasUsed + opcodeGas op) (stepsDone + 1) stack'
              omega
            · simp only
              omega

theorem evalLoop_gas_le_upper_bound (prog : List VmOpcode)
    (gasLimit maxDepth gasUsed stepsDone : Nat) (stack : List Int) :
    (evalLoop prog gasLimit maxDepth gasUsed stepsDone stack).gasUsed ≤
      gasUsed + preExecutionOpGas prog := by
  induction prog generalizing gasUsed stepsDone stack with
  | nil => simp [evalLoop, preExecutionOpGas]
  | cons op rest ih =>
      simp only [evalLoop, preExecutionOpGas]
      split
      · simp only
        omega
      · cases hstep : stepOpcode stack op maxDepth with
        | error trap =>
            simp only
            omega
        | ok pair =>
            rcases pair with ⟨halted, stack'⟩
            cases halted
            · simp only
              have h := ih (gasUsed + opcodeGas op) (stepsDone + 1) stack'
              omega
            · simp only
              omega

/-- Static pre-execution gas soundness: metered instruction gas never exceeds `preExecutionOpGas`. -/
theorem evalProgram_gas_le_preExecutionOpGas (prog : List VmOpcode) (gasLimit maxDepth : Nat) :
    (evalProgram prog gasLimit maxDepth).gasUsed ≤ preExecutionOpGas prog := by
  have h := evalLoop_gas_le_upper_bound prog gasLimit maxDepth 0 0 []
  simpa [evalProgram] using h

theorem evalLoop_gas_le_limit (prog : List VmOpcode) (gasLimit maxDepth gasUsed stepsDone : Nat)
    (stack : List Int) (hle : gasUsed ≤ gasLimit) :
    (evalLoop prog gasLimit maxDepth gasUsed stepsDone stack).gasUsed ≤ gasLimit := by
  induction prog generalizing gasUsed stepsDone stack with
  | nil =>
      simpa [evalLoop] using hle
  | cons op rest ih =>
      simp only [evalLoop]
      split
      · exact hle
      · rename_i hnotgt
        have hnext : gasUsed + opcodeGas op ≤ gasLimit := by omega
        cases hstep : stepOpcode stack op maxDepth with
        | error trap => exact hnext
        | ok pair =>
            rcases pair with ⟨halted, stack'⟩
            cases halted
            · exact ih (gasUsed + opcodeGas op) (stepsDone + 1) stack' hnext
            · exact hnext

/-- Caller/bounds gas ceiling soundness: metered instruction gas never exceeds `gasLimit`. -/
theorem evalProgram_gas_le_limit (prog : List VmOpcode) (gasLimit maxDepth : Nat) :
    (evalProgram prog gasLimit maxDepth).gasUsed ≤ gasLimit :=
  evalLoop_gas_le_limit prog gasLimit maxDepth 0 0 [] (Nat.zero_le _)

theorem evalLoop_stack_depth_bound (prog : List VmOpcode)
    (gasLimit maxDepth gasUsed stepsDone : Nat) (stack s : List Int)
    (hlen : stack.length ≤ maxDepth)
    (hok : (evalLoop prog gasLimit maxDepth gasUsed stepsDone stack).outcome = .ok s) :
    s.length ≤ maxDepth := by
  induction prog generalizing gasUsed stepsDone stack with
  | nil => simp [evalLoop] at hok
  | cons op rest ih =>
      simp only [evalLoop] at hok
      split at hok
      · cases hok
      · cases hstep : stepOpcode stack op maxDepth with
        | error trap => simp [hstep] at hok
        | ok pair =>
            rcases pair with ⟨halted, stack'⟩
            have hlen' := stepOpcode_preserves_depth_bound stack stack' op maxDepth halted hlen hstep
            cases halted
            · simp [hstep] at hok
              exact ih (gasUsed + opcodeGas op) (stepsDone + 1) stack' hlen' hok
            · simp [hstep] at hok
              subst hok
              exact hlen'

/-- Stack-depth ceiling invariant: any halted stack satisfies `s.length ≤ maxDepth`. -/
theorem evalProgram_stack_depth_bound (prog : List VmOpcode) (gasLimit maxDepth : Nat)
    (s : List Int) (hok : (evalProgram prog gasLimit maxDepth).outcome = .ok s) :
    s.length ≤ maxDepth :=
  evalLoop_stack_depth_bound prog gasLimit maxDepth 0 0 [] s (Nat.zero_le _) hok

theorem evalLoop_stack_inI64 (prog : List VmOpcode)
    (gasLimit maxDepth gasUsed stepsDone : Nat) (stack s : List Int)
    (hin : ∀ v ∈ stack, InVmI64 v)
    (hok : (evalLoop prog gasLimit maxDepth gasUsed stepsDone stack).outcome = .ok s) :
    ∀ v ∈ s, InVmI64 v := by
  induction prog generalizing gasUsed stepsDone stack with
  | nil => simp [evalLoop] at hok
  | cons op rest ih =>
      simp only [evalLoop] at hok
      split at hok
      · cases hok
      · cases hstep : stepOpcode stack op maxDepth with
        | error trap => simp [hstep] at hok
        | ok pair =>
            rcases pair with ⟨halted, stack'⟩
            have hin' := stepOpcode_preserves_inI64 stack stack' op maxDepth halted hin hstep
            cases halted
            · simp [hstep] at hok
              exact ih (gasUsed + opcodeGas op) (stepsDone + 1) stack' hin' hok
            · simp [hstep] at hok
              subst hok
              exact hin'

/-- Every value on any halted stack remains strictly inside signed 64-bit bounds `InVmI64`. -/
theorem evalProgram_stack_inI64 (prog : List VmOpcode) (gasLimit maxDepth : Nat)
    (s : List Int) (hok : (evalProgram prog gasLimit maxDepth).outcome = .ok s) :
    ∀ v ∈ s, InVmI64 v :=
  evalLoop_stack_inI64 prog gasLimit maxDepth 0 0 [] s (fun _ h => nomatch h) hok

/-! ## Section 4: Atomic Rollback, Bedrock/Frame Exclusion & Disjoint-Write Commutation -/

/-- Explicit non-toggle voxel write emitted by a halted `VM-1` output register. -/
inductive VmWriteOp where
  | on
  | off
deriving DecidableEq, Repr

/-- One explicit coordinate write emitted by an output register. -/
structure VmOutputWrite where
  coord : Coord
  op : VmWriteOp
deriving DecidableEq, Repr

/-- Lift a `VM-1` explicit write to a `World.lean` `Edit`. -/
def VmOutputWrite.toEdit (w : VmOutputWrite) : Edit :=
  match w.op with
  | .on => .on w.coord
  | .off => .off w.coord

/-- Apply an ordered list of explicit `VM-1` writes to `World`. -/
def applyWrites (world : World) (ws : List VmOutputWrite) : World :=
  replay world (ws.map VmOutputWrite.toEdit)

/-- Result of one invocation commit step (`LocalSpatialVmV1::execute_tick` per-program body). -/
structure InvocationCommit where
  outcome : Except VmTrap Int
  gasUsed : Nat
  emittedWrites : List VmOutputWrite
  postWorld : World
deriving Repr

/-- Atomic per-invocation staging and commit mirroring `LocalSpatialVmV1::execute_tick`. -/
def commitInvocation
    (preWorld : World)
    (prog : List VmOpcode)
    (gasLimit maxDepth : Nat)
    (stageOutput : Int → Nat → Except VmTrap (List VmOutputWrite × Nat)) : InvocationCommit :=
  let evalRes := evalProgram prog gasLimit maxDepth
  match evalRes.outcome with
  | .error trap =>
      { outcome := .error trap, gasUsed := evalRes.gasUsed, emittedWrites := [], postWorld := preWorld }
  | .ok stack =>
      match stack with
      | [] =>
          { outcome := .error .stackUnderflow, gasUsed := evalRes.gasUsed,
            emittedWrites := [], postWorld := preWorld }
      | top :: _ =>
          match stageOutput top evalRes.gasUsed with
          | .error trap =>
              { outcome := .error trap, gasUsed := evalRes.gasUsed,
                emittedWrites := [], postWorld := preWorld }
          | .ok (writes, totalGas) =>
              { outcome := .ok top, gasUsed := totalGas,
                emittedWrites := writes, postWorld := applyWrites preWorld writes }

/-- Atomic rollback soundness: ANY trap commits zero writes and leaves `postWorld = preWorld`. -/
theorem commitInvocation_trap_atomic_rollback
    (preWorld : World) (prog : List VmOpcode) (gasLimit maxDepth : Nat)
    (stageOutput : Int → Nat → Except VmTrap (List VmOutputWrite × Nat))
    (t : VmTrap)
    (htrap : (commitInvocation preWorld prog gasLimit maxDepth stageOutput).outcome = .error t) :
    (commitInvocation preWorld prog gasLimit maxDepth stageOutput).emittedWrites = [] ∧
      (commitInvocation preWorld prog gasLimit maxDepth stageOutput).postWorld = preWorld := by
  unfold commitInvocation at htrap ⊢
  cases heval : (evalProgram prog gasLimit maxDepth).outcome with
  | error e =>
      simp [heval]
  | ok stack =>
      cases stack with
      | nil => simp [heval]
      | cons top rest =>
          cases hstage : stageOutput top (evalProgram prog gasLimit maxDepth).gasUsed with
          | error e => simp [heval, hstage]
          | ok pair =>
              rcases pair with ⟨ws, g⟩
              simp [heval, hstage] at htrap

/-- Powered negative control: an un-staged leaky executor that applies one write before trapping
    mutates the world and violates atomic rollback. -/
def partialWriteLeak (preWorld : World) (c : Coord) : InvocationCommit :=
  let leak : VmOutputWrite := { coord := c, op := .on }
  { outcome := .error (.outputWriteConflict c),
    gasUsed := 10,
    emittedWrites := [leak],
    postWorld := applyWrites preWorld [leak] }

theorem partialWriteLeak_violates_rollback :
    let c : Coord := { x := 5, y := 18, z := 2 }
    let res := partialWriteLeak initialWorld c
    res.outcome = .error (.outputWriteConflict c) ∧
      res.emittedWrites ≠ [] ∧
      bit res.postWorld c ∧
      ¬ bit initialWorld c := by
  refine ⟨rfl, by decide, ?_, ?_⟩
  · simp [partialWriteLeak, applyWrites, replay, VmOutputWrite.toEdit, applyEdit, bit]
  · simp [initialWorld, origin, bit]

/-- Fail-closed output write validator excluding any coordinate satisfying `forbidden`
    (instantiated with `c = origin ∨ InAnyProgramFrame c`). -/
def validateWrites (forbidden : Coord → Bool) (ws : List VmOutputWrite) :
    Except VmTrap (List VmOutputWrite) :=
  match ws with
  | [] => .ok []
  | w :: rest =>
      if forbidden w.coord then
        .error (.forbiddenOutputTarget w.coord)
      else
        match validateWrites forbidden rest with
        | .error t => .error t
        | .ok rest' => .ok (w :: rest')

theorem validateWrites_sound (forbidden : Coord → Bool) (ws ws' : List VmOutputWrite)
    (hok : validateWrites forbidden ws = .ok ws') :
    ws' = ws ∧ ∀ w ∈ ws', forbidden w.coord = false := by
  induction ws generalizing ws' with
  | nil =>
      simp [validateWrites] at hok
      subst hok
      exact ⟨rfl, fun _ h => nomatch h⟩
  | cons w rest ih =>
      simp only [validateWrites] at hok
      cases hf : forbidden w.coord with
      | true => simp [hf] at hok
      | false =>
          simp only [hf, Bool.false_eq_true, ↓reduceIte] at hok
          cases hrec : validateWrites forbidden rest with
          | error t => simp [hrec] at hok
          | ok rest' =>
              simp only [hrec, Except.ok.injEq] at hok
              subst hok
              have ⟨heq, hall⟩ := ih rest' hrec
              refine ⟨by rw [heq], ?_⟩
              intro x hx
              simp only [List.mem_cons] at hx
              rcases hx with rfl | hmem
              · exact hf
              · exact hall x hmem

theorem applyWrites_preserves_unwritten_coord (world : World) (ws : List VmOutputWrite) (q : Coord)
    (hnone : ∀ w ∈ ws, w.coord ≠ q) :
    bit (applyWrites world ws) q ↔ bit world q := by
  induction ws generalizing world with
  | nil => simp [applyWrites, replay]
  | cons w rest ih =>
      have hw : w.coord ≠ q := hnone w (List.mem_cons_self ..)
      have hrest : ∀ x ∈ rest, x.coord ≠ q := fun x hx => hnone x (List.mem_cons_of_mem w hx)
      have hstep : bit (applyEdit world w.toEdit) q ↔ bit world q := by
        cases hop : w.op with
        | on =>
            simp [VmOutputWrite.toEdit, hop, bit_on, Ne.symm hw]
        | off =>
            simp only [VmOutputWrite.toEdit, hop, bit_off]
            split
            · rfl
            · exact ⟨fun h => h.2, fun h => ⟨Ne.symm hw, h⟩⟩
      simpa [applyWrites, replay, List.map_cons] using (ih (applyEdit world w.toEdit) hrest).trans hstep

/-- Bedrock and program-frame exclusion theorem: every validated write list leaves every
    forbidden coordinate (including bedrock `origin` and all framed program voxels) bit-invariant. -/
theorem validated_writes_preserve_forbidden (world : World) (forbidden : Coord → Bool)
    (ws ws' : List VmOutputWrite) (q : Coord)
    (hok : validateWrites forbidden ws = .ok ws')
    (hforb : forbidden q = true) :
    bit (applyWrites world ws') q ↔ bit world q := by
  have ⟨_, hall⟩ := validateWrites_sound forbidden ws ws' hok
  apply applyWrites_preserves_unwritten_coord
  intro w hw heq
  subst heq
  have hf := hall w hw
  rw [hf] at hforb
  cases hforb

/-- Single-write commutation for distinct coordinates. -/
theorem applyEdit_comm_of_ne (world : World) (w1 w2 : VmOutputWrite) (hne : w1.coord ≠ w2.coord) :
    SameWorld
      (applyEdit (applyEdit world w1.toEdit) w2.toEdit)
      (applyEdit (applyEdit world w2.toEdit) w1.toEdit) := by
  intro q
  cases h1 : w1.op <;> cases h2 : w2.op <;>
    simp only [VmOutputWrite.toEdit, h1, h2, bit_on, bit_off]
  · exact ⟨fun h => by rcases h with hq | hq | hw
                       · exact Or.inr (Or.inl hq)
                       · exact Or.inl hq
                       · exact Or.inr (Or.inr hw),
           fun h => by rcases h with hq | hq | hw
                       · exact Or.inr (Or.inl hq)
                       · exact Or.inl hq
                       · exact Or.inr (Or.inr hw)⟩
  · by_cases ho2 : w2.coord = origin
    · simp [ho2]
    · simp only [ho2, ↓reduceIte]
      constructor
      · rintro ⟨hq2, hq1 | hw⟩
        · exact Or.inl hq1
        · exact Or.inr ⟨hq2, hw⟩
      · rintro (hq1 | ⟨hq2, hw⟩)
        · exact ⟨by rw [hq1]; exact hne, Or.inl hq1⟩
        · exact ⟨hq2, Or.inr hw⟩
  · by_cases ho1 : w1.coord = origin
    · simp [ho1]
    · simp only [ho1, ↓reduceIte]
      constructor
      · rintro (hq2 | ⟨hq1, hw⟩)
        · exact ⟨by rw [hq2]; exact Ne.symm hne, Or.inl hq2⟩
        · exact ⟨hq1, Or.inr hw⟩
      · rintro ⟨hq1, hq2 | hw⟩
        · exact Or.inl hq2
        · exact Or.inr ⟨hq1, hw⟩
  · by_cases ho1 : w1.coord = origin <;> by_cases ho2 : w2.coord = origin <;>
      simp [ho1, ho2, and_left_comm]

theorem applyEdit_sameWorld (w1 w2 : World) (e : Edit) (h : SameWorld w1 w2) :
    SameWorld (applyEdit w1 e) (applyEdit w2 e) := by
  intro q
  cases e with
  | on c => simp [bit_on, h q]
  | off c =>
      simp only [bit_off]
      split
      · exact h q
      · simp [h q]
  | toggle c =>
      simp only [bit_toggle]
      split
      · exact h q
      · split
        · simp [h q]
        · exact h q

theorem applyWrites_sameWorld (w1 w2 : World) (ws : List VmOutputWrite) (h : SameWorld w1 w2) :
    SameWorld (applyWrites w1 ws) (applyWrites w2 ws) := by
  induction ws generalizing w1 w2 with
  | nil => simpa [applyWrites, replay] using h
  | cons w rest ih =>
      exact ih _ _ (applyEdit_sameWorld w1 w2 w.toEdit h)

theorem applyWrites_comm_single (world : World) (w1 : VmOutputWrite) (ws2 : List VmOutputWrite)
    (hdisj : ∀ w2 ∈ ws2, w1.coord ≠ w2.coord) :
    SameWorld
      (applyWrites (applyEdit world w1.toEdit) ws2)
      (applyEdit (applyWrites world ws2) w1.toEdit) := by
  induction ws2 generalizing world with
  | nil =>
      intro q
      simp [applyWrites, replay]
  | cons w2 rest ih =>
      have hne : w1.coord ≠ w2.coord := hdisj w2 (List.mem_cons_self ..)
      have hrest : ∀ x ∈ rest, w1.coord ≠ x.coord := fun x hx => hdisj x (List.mem_cons_of_mem w2 hx)
      have hswap := applyEdit_comm_of_ne world w1 w2 hne
      have h1 := applyWrites_sameWorld _ _ rest hswap
      have h2 := ih (applyEdit world w2.toEdit) hrest
      intro q
      simpa [applyWrites, replay, List.map_cons] using (h1 q).trans (h2 q)

/-- Disjoint-write commutation theorem: two non-conflicting `VM-1` program output batches
    commute observationally on `World`, matching `LocalSpatialVmV1`'s disjoint-claim invariant. -/
theorem applyWrites_comm_of_disjoint (world : World) (ws1 ws2 : List VmOutputWrite)
    (hdisj : ∀ w1 ∈ ws1, ∀ w2 ∈ ws2, w1.coord ≠ w2.coord) :
    SameWorld
      (applyWrites (applyWrites world ws1) ws2)
      (applyWrites (applyWrites world ws2) ws1) := by
  induction ws1 generalizing world with
  | nil =>
      intro q
      simp [applyWrites, replay]
  | cons w1 rest1 ih =>
      have hw1 : ∀ w2 ∈ ws2, w1.coord ≠ w2.coord :=
        fun w2 hw2 => hdisj w1 (List.mem_cons_self ..) w2 hw2
      have hrest1 : ∀ x ∈ rest1, ∀ w2 ∈ ws2, x.coord ≠ w2.coord :=
        fun x hx w2 hw2 => hdisj x (List.mem_cons_of_mem w1 hx) w2 hw2
      have h1 := ih (applyEdit world w1.toEdit) hrest1
      have h2 := applyWrites_sameWorld _ _ rest1 (applyWrites_comm_single world w1 ws2 hw1)
      intro q
      simpa [applyWrites, replay, List.map_cons] using (h1 q).trans (h2 q)

end BitblockProof
