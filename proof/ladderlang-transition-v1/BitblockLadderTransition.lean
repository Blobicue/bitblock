/-
Pinned executable truth table for Bitblock's one-bit transition algebra.

The two copied LadderLang sources are identified in source-manifest.tsv.  This
file checks the finite `on/off/toggle × prestate` table with the actual
LadderLang reducer.  It does not claim a Rust refinement, coordinate encoding,
mint/burn provenance, rendering, iota cost of execution, distributed time,
stake, consensus, or a Layer 1.

`HOLD` is a bounded reducer outcome, not proof-of-history.  `RAN` and `GREW`
are deliberately retained controls for the chosen fuel and size limits.
-/
import Ladderlang.Entries

namespace BitblockLadderTransition

open Ladderlang

/-- Select the first branch: placing makes a mutable bit on. -/
def Place : Lam := .lam (.lam (.lam (.var 2)))

/-- Select the second branch: deleting makes a mutable bit off. -/
def Delete : Lam := .lam (.lam (.lam (.var 1)))

/-- Select the third branch: toggling flips a mutable bit. -/
def Toggle : Lam := .lam (.lam (.lam (.var 0)))

/--
`guard bit (op TRUE FALSE (NOT bit))`: preserve an origin bit when guarded;
otherwise select its mutable operation result.
-/
def transition : Lam :=
  .lam (.lam (.lam
    (app2 (.var 2) (.var 0)
      (app3 (.var 1) TRUE FALSE (.app NOT (.var 0))))))

def subject (guarded op state : Lam) : Lam :=
  .app (.app (.app transition guarded) op) state

def run (guarded op state : Lam) : Verdict :=
  (subject guarded op state).eval { fuel := 12, sizeCap := 96 }

/- The complete mutable truth table. -/
theorem nonorigin_place_off : run FALSE Place FALSE = .HOLD TRUE := by rfl
theorem nonorigin_place_on : run FALSE Place TRUE = .HOLD TRUE := by rfl
theorem nonorigin_delete_off : run FALSE Delete FALSE = .HOLD FALSE := by rfl
theorem nonorigin_delete_on : run FALSE Delete TRUE = .HOLD FALSE := by rfl
theorem nonorigin_toggle_off : run FALSE Toggle FALSE = .HOLD TRUE := by rfl
theorem nonorigin_toggle_on : run FALSE Toggle TRUE = .HOLD FALSE := by rfl

/- The admissible immutable-origin rows. -/
theorem origin_place_on : run TRUE Place TRUE = .HOLD TRUE := by rfl
theorem origin_delete_on : run TRUE Delete TRUE = .HOLD TRUE := by rfl
theorem origin_toggle_on : run TRUE Toggle TRUE = .HOLD TRUE := by rfl

/- Powered controls: a wrong operation result and underprovisioned evaluation. -/
theorem toggle_on_is_not_place :
    (run FALSE Toggle TRUE == .HOLD TRUE) = false := by rfl

theorem toggle_needs_twelve :
    (subject FALSE Toggle TRUE).eval { fuel := 11, sizeCap := 96 } = .RAN := by rfl

theorem cap_control :
    (subject FALSE Toggle TRUE).eval { fuel := 12, sizeCap := 1 } = .GREW := by rfl

#print axioms nonorigin_place_off
#print axioms nonorigin_place_on
#print axioms nonorigin_delete_off
#print axioms nonorigin_delete_on
#print axioms nonorigin_toggle_off
#print axioms nonorigin_toggle_on
#print axioms origin_place_on
#print axioms origin_delete_on
#print axioms origin_toggle_on
#print axioms toggle_on_is_not_place
#print axioms toggle_needs_twelve
#print axioms cap_control

end BitblockLadderTransition
