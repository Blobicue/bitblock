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

/-- Encode one frozen octet most-significant bit first.  The finite quote below is kept as
    octets, rather than a pre-expanded Lam, so the audit still exercises this file's byte codec. -/
def encodeOctet (octet : Nat) : Lam :=
  encodeByte
    (octet / 128 % 2 == 1)
    (octet / 64 % 2 == 1)
    (octet / 32 % 2 == 1)
    (octet / 16 % 2 == 1)
    (octet / 8 % 2 == 1)
    (octet / 4 % 2 == 1)
    (octet / 2 % 2 == 1)
    (octet % 2 == 1)

/-- `bb-lam/v1 || content-tag-preimage(Edit(Toggle, (i64::MIN, -2, i64::MAX)))`.
    Rust and the independent Python vector bind this exact byte sequence to its content tags;
    this pinned-LadderLang assay checks the resulting cost with the implementation that defines it. -/
def canonicalQuoteBytes : List Nat :=
  [98, 98, 45, 108, 97, 109, 47, 118, 49, 0, 23, 98, 105, 116, 98, 108,
    111, 99, 107, 47, 99, 111, 110, 116, 101, 110, 116, 45, 116, 97, 103, 47,
    118, 49, 0, 4, 101, 100, 105, 116, 0, 6, 115, 104, 97, 50, 53, 54, 0, 29,
    98, 105, 116, 98, 108, 111, 99, 107, 47, 101, 100, 105, 116, 47, 111, 112,
    45, 117, 56, 45, 105, 54, 52, 45, 98, 101, 47, 118, 49, 0, 0, 0, 0, 0, 0,
    0, 25, 2, 128, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255,
    254, 127, 255, 255, 255, 255, 255, 255, 255]

def canonicalQuoteTerm : Lam :=
  encodeBytes (canonicalQuoteBytes.map encodeOctet)

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

theorem canonical_quote_has_112_bytes : canonicalQuoteBytes.length = 112 := by
  decide

/-- The direct finite parity assay: this exact 112-byte Bitblock quote is closed and receives
    the same static representation upper bounds reported by the Rust bridge. -/
theorem canonical_quote_cost_checked :
    costIs canonicalQuoteTerm 20_722 82_438 = true := by
  set_option maxHeartbeats 2000000 in
    decide

/-- Powered value control: the same admitted quote must reject an off-by-one atom claim. -/
theorem canonical_quote_rejects_wrong_atoms :
    costIs canonicalQuoteTerm 20_721 82_438 = false := by
  set_option maxHeartbeats 2000000 in
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
#print axioms canonical_quote_has_112_bytes
#print axioms canonical_quote_cost_checked
#print axioms canonical_quote_rejects_wrong_atoms
#print axioms open_term_is_refused

end BitblockLadderLangParity
