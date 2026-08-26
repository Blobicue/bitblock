/-
Run this file from the pinned LadderLang checkout, not from Bitblock's standalone
proof package:

  lake env lean C:\Users\Owen\bitblock\proof\LadderLangQuotedByteParity.lean

Audited LadderLang commit: 59042265167b8f40700a891d890fde8015bcade9

Unlike BitblockProof.QuotedByteCost, this assay uses LadderLang's own `Lam`,
naive bracket abstraction, and `Lam.costChecked`. It checks the primitive costs
on which the general standalone proof depends and carries a powered open-term
refusal control. It still proves representation cost only, not edit semantics.
-/
import Ladderlang.Entries
import Ladderlang.Closed

namespace BitblockLadderLangParity

open Ladderlang

def B0 : Lam := .app I FALSE
def B1 : Lam := .app FALSE I

def encodeBit : Bool → Lam
  | false => B0
  | true => B1

def pairNode (left right : Lam) : Lam := app2 PAIR left right

def encodeByte (b0 b1 b2 b3 b4 b5 b6 b7 : Bool) : Lam :=
  pairNode (encodeBit b0)
    (pairNode (encodeBit b1)
      (pairNode (encodeBit b2)
        (pairNode (encodeBit b3)
          (pairNode (encodeBit b4)
            (pairNode (encodeBit b5)
              (pairNode (encodeBit b6) (encodeBit b7)))))))

def encodeBytes : List Lam → Lam
  | [] => NIL
  | byte :: rest => .app byte (encodeBytes rest)

def costIs (term : Lam) (atoms iota : Nat) : Bool :=
  match term.costChecked with
  | some cost => cost.atomsUpper == atoms && cost.iotaUpper == iota
  | none => false

theorem b0_cost_checked :
    costIs B0 3 8 = true := by
  decide

theorem b1_cost_checked :
    costIs B1 3 8 = true := by
  decide

theorem bit_symbols_are_distinct : B0 ≠ B1 := by
  decide

theorem pair_cost_checked :
    costIs PAIR 23 96 = true := by
  decide

theorem zero_byte_cost_checked :
    costIs (encodeByte false false false false false false false false) 185 736 = true := by
  decide

theorem one_byte_cost_checked :
    costIs (encodeByte true true true true true true true true) 185 736 = true := by
  decide

theorem mixed_byte_cost_checked :
    costIs (encodeByte true false true false false true false true) 185 736 = true := by
  decide

theorem two_byte_spine_cost_checked :
    let first := encodeByte false false false false false false false false
    let second := encodeByte true false true false false true false true
    costIs (encodeBytes [first, second]) 372 1478 = true := by
  decide

/-- Powered control: unchecked/open syntax must not receive a cheaper receipt. -/
theorem open_term_is_refused :
    (Lam.var 0).costChecked.isNone = true := by
  decide

#print axioms b0_cost_checked
#print axioms b1_cost_checked
#print axioms pair_cost_checked
#print axioms zero_byte_cost_checked
#print axioms one_byte_cost_checked
#print axioms mixed_byte_cost_checked
#print axioms two_byte_spine_cost_checked
#print axioms open_term_is_refused

end BitblockLadderLangParity
