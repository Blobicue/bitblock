/-
  LADDERLANG — ENTRIES

  The library is DATA. Adding a term adds a row here; no code changes.

  Every term is written once, as a `Lam`. Coin, depth, layer, cost and cites
  are all computed off it by Core — nothing below is annotated by hand.
-/
import Ladderlang.Core

namespace Ladderlang
open Lam

/-! ## Notation helpers -/

def app2 (f a b : Lam) : Lam := .app (.app f a) b
def app3 (f a b c : Lam) : Lam := .app (.app (.app f a) b) c

/-- Wrap a body in `k` binders. -/
def nlam : Nat → Lam → Lam
  | 0,   t => t
  | k+1, t => .lam (nlam k t)

/-! ## L2 — Identity. One binder, used once. Only GIVE exists here. -/

def I : Lam := .lam (.var 0)

/-! ## L3 — Selection. HOLD and COPY arrive together, and so does recursion. -/

/-- Weakening. The unique information-destroying rule, and the arrow of time. -/
def K : Lam := .lam (.lam (.var 1))
/-- Contraction. -/
def W : Lam := .lam (.lam (app2 (.var 1) (.var 0) (.var 0)))
/-- INADMISSIBLE — recorded, never cited. `Y : (A→A)→A` inhabits every type,
    so the logic goes inconsistent here, one layer before the basis closes.
    Turing-completeness is cheaper than consistency. -/
def Y : Lam :=
  let half := Lam.lam (.app (.var 1) (.app (.var 0) (.var 0)))
  .lam (.app half half)
def TRUE  : Lam := .lam (.lam (.var 1))
def FALSE : Lam := .lam (.lam (.var 0))
def ZERO  : Lam := .lam (.lam (.var 0))
def ONE   : Lam := .lam (.lam (.app (.var 1) (.var 0)))
/-- THE FIRST ENTRY THIS REPO DID NOT CHOOSE. Item 68's diagonal ranked `(W B)` highest of the
    gap — a behaviour the targets need 25 times and the ledger lacked — and `W B` reduces to exactly
    this term. The ledger had ZERO and ONE and no TWO, while `intTwo` puts `church 2` inside every
    target.

    Minted as the numeral rather than as `W B`: the composition was the DISCOVERY ROUTE, but `.app W
    B` is 15 tokens against this term's 7, and item 70's mint gate refuses an entry that enumeration
    expresses more cheaply. Found by composition, stored at its minimum. -/
def TWO   : Lam := .lam (.lam (.app (.var 1) (.app (.var 1) (.var 0))))

/-! ## L4 — Composition. The basis closes. This is the last layer that adds power. -/

def S    : Lam := .lam (.lam (.lam (app2 (.var 2) (.var 0) (.app (.var 1) (.var 0)))))
def B    : Lam := .lam (.lam (.lam (.app (.var 2) (.app (.var 1) (.var 0)))))
def C    : Lam := .lam (.lam (.lam (app2 (.var 2) (.var 0) (.var 1))))
def MULT : Lam := .lam (.lam (.lam (.app (.var 2) (.app (.var 1) (.var 0)))))
def PAIR : Lam := .lam (.lam (.lam (app2 (.var 0) (.var 2) (.var 1))))
def PLUS : Lam :=
  .lam (.lam (.lam (.lam (app2 (.var 3) (.var 1) (app2 (.var 2) (.var 1) (.var 0))))))
def SUCC : Lam := .lam (.lam (.lam (.app (.var 1) (app2 (.var 2) (.var 1) (.var 0)))))
def AND  : Lam := .lam (.lam (app2 (.var 1) (.var 0) FALSE))
def OR   : Lam := .lam (.lam (app2 (.var 1) TRUE (.var 0)))
def NOT  : Lam := .lam (app2 (.var 0) FALSE TRUE)
def FST  : Lam := .lam (.app (.var 0) TRUE)
def SND  : Lam := .lam (.app (.var 0) FALSE)

/-! ## L5 — State. No new power; the basis already closed. Vocabulary only. -/

def PRED : Lam :=
  .lam (.lam (.lam (
    app2 (.app (.var 2) (.lam (.lam (.app (.var 0) (.app (.var 1) (.var 3))))))
         (.lam (.var 1))
         (.lam (.var 0)))))
def FLIPFLOP : Lam := .lam (.lam (app2 PAIR (.var 0) (.var 1)))
def XOR      : Lam := .lam (.lam (app2 (.var 1) (.app NOT (.var 0)) (.var 0)))

/-! ## L6 — Arithmetic. One bit, with its carry. -/

def ADD_SUM : Lam := .lam (.lam (.lam (app2 XOR (app2 XOR (.var 2) (.var 1)) (.var 0))))
def ADD_CAR : Lam :=
  .lam (.lam (.lam (
    app2 OR (app2 AND (.var 2) (.var 1))
            (app2 AND (.var 0) (app2 XOR (.var 2) (.var 1))))))

/-! ## The recorded placements — the ladder's 24, as published -/

structure Node where
  name     : String
  term     : Lam
  /-- The ladder's published placement. 0 means "never published" — see
      `published`, and note that gate 0 scores only against published rows. -/
  layerRec  : Nat := 0
  atomsRec  : Nat := 0
  admiss    : Bool := true
  published : Bool := true

def recorded : List Node :=
  [ { name := "I",        term := I,        layerRec := 2, atomsRec := 1   }
  , { name := "Y",        term := Y,        layerRec := 3, atomsRec := 25,
      admiss := false }
  , { name := "W",        term := W,        layerRec := 3, atomsRec := 16  }
  , { name := "K",        term := K,        layerRec := 3, atomsRec := 4   }
  , { name := "TRUE",     term := TRUE,     layerRec := 3, atomsRec := 4   }
  , { name := "FALSE",    term := FALSE,    layerRec := 3, atomsRec := 2   }
  , { name := "ZERO",     term := ZERO,     layerRec := 3, atomsRec := 2   }
  , { name := "PLUS",     term := PLUS,     layerRec := 4, atomsRec := 58  }
  , { name := "SUCC",     term := SUCC,     layerRec := 4, atomsRec := 37  }
  , { name := "S",        term := S,        layerRec := 4, atomsRec := 31  }
  , { name := "B",        term := B,        layerRec := 4, atomsRec := 25  }
  , { name := "C",        term := C,        layerRec := 4, atomsRec := 25  }
  , { name := "MULT",     term := MULT,     layerRec := 4, atomsRec := 25  }
  , { name := "PAIR",     term := PAIR,     layerRec := 4, atomsRec := 23  }
  , { name := "AND",      term := AND,      layerRec := 4, atomsRec := 18  }
  , { name := "OR",       term := OR,       layerRec := 4, atomsRec := 16  }
  , { name := "NOT",      term := NOT,      layerRec := 4, atomsRec := 11  }
  , { name := "FST",      term := FST,      layerRec := 4, atomsRec := 7   }
  , { name := "SND",      term := SND,      layerRec := 4, atomsRec := 5   }
  , { name := "PRED",     term := PRED,     layerRec := 5, atomsRec := 61  }
  , { name := "FLIPFLOP", term := FLIPFLOP, layerRec := 5, atomsRec := 33  }
  , { name := "XOR",      term := XOR,      layerRec := 5, atomsRec := 29  }
  , { name := "ADD_CAR",  term := ADD_CAR,  layerRec := 6, atomsRec := 136 }
  , { name := "ADD_SUM",  term := ADD_SUM,  layerRec := 6, atomsRec := 91  }
  ]

/-! ## The ripple adder family — where the ladder scales by WIDTH

    Nothing here computes anything a full adder cannot. What the budget buys
    is bits. `depth = (binders per input)·n + (depth of the deepest fixed
    component)` = 2n + 8, so the layer index reads off the widest word the
    budget affords. -/

/-- Bit `i` of the first operand, under `2n` binders. -/
def bitA (n i : Nat) : Lam := .var (n + i)
/-- Bit `i` of the second operand. -/
def bitB (i : Nat) : Lam := .var i

/-- Carry into position `i`, threaded from FALSE through the ripple. -/
def carryInto (n : Nat) : Nat → Lam
  | 0    => FALSE
  | i+1  => app3 ADD_CAR (bitA n i) (bitB i) (carryInto n i)

def sumBitAt (n i : Nat) : Lam := app3 ADD_SUM (bitA n i) (bitB i) (carryInto n i)

/-- The output: every sum bit, then the final carry, nested with PAIR. -/
def adderBody (n : Nat) : Nat → Lam
  | 0    => carryInto n n
  | k+1  => app2 PAIR (sumBitAt n (n - (k+1))) (adderBody n k)

/-- An `n`-bit ripple adder: `2n` bits held open, carry threaded through `n`
    stages. Constructed, not transcribed. -/
def mkAdder (n : Nat) : Lam := nlam (2 * n) (adderBody n n)

/-! ## Word packing (hash-horizon Stage 0) — PRE-REGISTERED in WORKLOG.md

    Nested PAIR, LSB at FST, 4 bits. Not CONS (TAIL unminted). Not a recorded
    placement. Gate 11 watched `word4RoundTrip` fail on a FALSE stub, then this
    spine. -/

/-- `TRUE` if bit `i` of `n` is set. Desk-side packing, not a combinator. -/
def bitFromNat (n i : Nat) : Lam := if (n >>> i) % 2 == 1 then TRUE else FALSE

/-- Nested PAIR spine: `PAIR b0 (PAIR b1 (PAIR b2 b3))`, LSB at FST. -/
def word4 (n : Nat) : Lam :=
  app2 PAIR (bitFromNat n 0)
    (app2 PAIR (bitFromNat n 1)
      (app2 PAIR (bitFromNat n 2) (bitFromNat n 3)))

/-- Bit `i` of a 4-bit PAIR spine. Same wiring as Gate 11 `peelBit4`. -/
def bitOfWord4 (w : Lam) : Nat → Lam
  | 0 => .app FST w
  | 1 => .app FST (.app SND w)
  | 2 => .app FST (.app SND (.app SND w))
  | 3 => .app SND (.app SND (.app SND w))
  | _ => FALSE

/-! ## ADD4 (hash-horizon Stage 1) — PRE-REGISTERED in WORKLOG.md

    Feed `mkAdder 4`. Not a recorded placement. Stub first so Gate 12 can
    go red. -/

/-- Apply left to right. Named apart from `AdderTerm.feedAll` (same namespace). -/
def feedBits : Lam → List Lam → Lam
  | t, []      => t
  | t, a :: as => feedBits (.app t a) as

/-- Feed `mkAdder 4`: `[a3,a2,a1,a0, b3,b2,b1,b0]`. Output
    `PAIR s0 (PAIR s1 (PAIR s2 (PAIR s3 cout)))`. Wrap = drop cout. -/
def add4 (a b : Nat) : Lam :=
  feedBits (mkAdder 4)
    [ bitFromNat a 3, bitFromNat a 2, bitFromNat a 1, bitFromNat a 0
    , bitFromNat b 3, bitFromNat b 2, bitFromNat b 1, bitFromNat b 0 ]

/-! ## ROTR1 / mixers / TOY_HASH — PRE-REGISTERED Stages 2–4 in WORKLOG.md

    Four loose bits, not nested PAIR. Peeling FST/SND through a mixer
    hung Gate compile (fuel wall). Same combinators: AND/XOR/NOT.
    Packing (`word4`) stays Stage 0. -/

structure Bits4 where
  b0 : Lam
  b1 : Lam
  b2 : Lam
  b3 : Lam

def bits4 (n : Nat) : Bits4 where
  b0 := bitFromNat n 0
  b1 := bitFromNat n 1
  b2 := bitFromNat n 2
  b3 := bitFromNat n 3

/-- ROTR1: bit i ← bit (i+1) mod 4. Rewire, no combinator. -/
def rotr1Bits (w : Bits4) : Bits4 where
  b0 := w.b1
  b1 := w.b2
  b2 := w.b3
  b3 := w.b0

def xorBits (a b : Bits4) : Bits4 where
  b0 := app2 XOR a.b0 b.b0
  b1 := app2 XOR a.b1 b.b1
  b2 := app2 XOR a.b2 b.b2
  b3 := app2 XOR a.b3 b.b3

def andBits (a b : Bits4) : Bits4 where
  b0 := app2 AND a.b0 b.b0
  b1 := app2 AND a.b1 b.b1
  b2 := app2 AND a.b2 b.b2
  b3 := app2 AND a.b3 b.b3

/-- One bit of SHA's Choice: `(x AND y) XOR (NOT x AND z)`. -/
def chBit (x y z : Lam) : Lam :=
  app2 XOR (app2 AND x y) (app2 AND (.app NOT x) z)

def chBits (x y z : Bits4) : Bits4 where
  b0 := chBit x.b0 y.b0 z.b0
  b1 := chBit x.b1 y.b1 z.b1
  b2 := chBit x.b2 y.b2 z.b2
  b3 := chBit x.b3 y.b3 z.b3

def bitAt (w : Bits4) : Nat → Lam
  | 0 => w.b0
  | 1 => w.b1
  | 2 => w.b2
  | 3 => w.b3
  | _ => FALSE

/-- One toy round: xor K, rotr1, Ch(_, rotr1, K), xor rotr1. K = 11. -/
def toyRound (w : Bits4) : Bits4 :=
  let y := xorBits w (bits4 11)
  let y := rotr1Bits y
  let y := chBits y (rotr1Bits y) (bits4 11)
  xorBits y (rotr1Bits y)

def toyHash (m : Nat) : Bits4 := toyRound (toyRound (bits4 (m &&& 15)))

/-- Mutant: last XOR of the round is AND. Must differ on ≥1 of 0..15. -/
def toyRoundMutant (w : Bits4) : Bits4 :=
  let y := xorBits w (bits4 11)
  let y := rotr1Bits y
  let y := chBits y (rotr1Bits y) (bits4 11)
  andBits y (rotr1Bits y)

def toyHashMutant (m : Nat) : Bits4 :=
  toyRoundMutant (toyRoundMutant (bits4 (m &&& 15)))

/-- Majority of three bits: `(x∧y) ∨ (x∧z) ∨ (y∧z)`. Minted AND/OR. -/
def majBit (x y z : Lam) : Lam :=
  app2 OR (app2 AND x y) (app2 OR (app2 AND x z) (app2 AND y z))

def majBits (x y z : Bits4) : Bits4 where
  b0 := majBit x.b0 y.b0 z.b0
  b1 := majBit x.b1 y.b1 z.b1
  b2 := majBit x.b2 y.b2 z.b2
  b3 := majBit x.b3 y.b3 z.b3

/-- ROTR2: host wiring, same class as ROTR1. Not a combinator. -/
def rotr2Bits (w : Bits4) : Bits4 where
  b0 := w.b2
  b1 := w.b3
  b2 := w.b0
  b3 := w.b1

/-- ROTRk by iterating ROTR1. k=0 is identity. Host wiring. -/
def rotrKBits : Nat → Bits4 → Bits4
  | 0,   w => w
  | k+1, w => rotrKBits k (rotr1Bits w)

/-- Tiny Σ: XOR of two rotates. XOR is the mint. -/
def sigmaBits (w : Bits4) : Bits4 := xorBits (rotr1Bits w) (rotr2Bits w)

/-- SHA-shaped Σ: XOR of three rotates. n=4 uses k=1,2,3. -/
def sigma3Bits (w : Bits4) : Bits4 :=
  xorBits (xorBits (rotr1Bits w) (rotr2Bits w)) (rotrKBits 3 w)

/-- SHR1: host wiring, incoming FALSE. Not a combinator. -/
def shr1Bits (w : Bits4) : Bits4 where
  b0 := w.b1
  b1 := w.b2
  b2 := w.b3
  b3 := FALSE

/-- One full-adder stage on loose bits. Direct AND/XOR/OR trees, not the
    91-atom `ADD_SUM` combinator (that hung Gate compile). Same minted ops. -/
def sumBit (x y cin : Lam) : Lam := app2 XOR (app2 XOR x y) cin
def carBit (x y cin : Lam) : Lam :=
  app2 OR (app2 AND x y) (app2 AND cin (app2 XOR x y))

/-- 4-bit wrap add on Bits4. Carry-in FALSE; drop carry-out. Composable
    with the hasher (PAIR `add4` is not). -/
def addBits (a b : Bits4) : Bits4 :=
  let c0 := FALSE
  let s0 := sumBit a.b0 b.b0 c0
  let c1 := carBit a.b0 b.b0 c0
  let s1 := sumBit a.b1 b.b1 c1
  let c2 := carBit a.b1 b.b1 c1
  let s2 := sumBit a.b2 b.b2 c2
  let c3 := carBit a.b2 b.b2 c2
  let s3 := sumBit a.b3 b.b3 c3
  { b0 := s0, b1 := s1, b2 := s2, b3 := s3 }

/-- SHA-shaped round with add: xor K, Ch, add Σ. Does not replace `toyRound`. -/
def toyRoundAdd (w : Bits4) : Bits4 :=
  let y := xorBits w (bits4 11)
  let y := chBits y (rotr1Bits y) (bits4 11)
  addBits y (sigmaBits y)

def toyHashAdd (m : Nat) : Bits4 :=
  toyRoundAdd (toyRoundAdd (bits4 (m &&& 15)))

/-- Mutant: last add replaced by XOR. -/
def toyRoundAddMutant (w : Bits4) : Bits4 :=
  let y := xorBits w (bits4 11)
  let y := chBits y (rotr1Bits y) (bits4 11)
  xorBits y (sigmaBits y)

def toyHashAddMutant (m : Nat) : Bits4 :=
  toyRoundAddMutant (toyRoundAddMutant (bits4 (m &&& 15)))

/-! ## FIB — the ladder's own generator, written as a term

    PREDICTION, registered before minting: the ladder measured FIB at depth 12
    or less, so `layer FIB ≤ 6`. The exact value was NOT recorded, so this is
    a bound, not a point. -/

/-- One step of the pair-shuffle: (a, b) ↦ (b, a+b). -/
def FIB_STEP : Lam :=
  .lam (app2 PAIR (.app SND (.var 0)) (app2 PLUS (.app FST (.var 0)) (.app SND (.var 0))))

def FIB : Lam :=
  .lam (.app FST (app2 (.var 0) FIB_STEP (app2 PAIR ZERO ONE)))

/-- Equality on a DECIDABLE DOMAIN is mintable — XNOR, verified on all four
    rows. Equality on the calculus itself never is: by Scott-Curry no term
    separates beta-equal pairs from beta-inequal ones. -/
def EQ_bool : Lam := .lam (.lam (app2 (.var 1) (.var 0) (.app NOT (.var 0))))

/-! ## Comparison and subtraction — item 2a

    PREDICTIONS, registered in WORKLOG.md and committed BEFORE these terms existed
    (commit "PRE-REGISTER item 2a"), as exact values rather than bounds:

        IS_ZERO  depth 4   L4          SUB     depth 7   L5
        LEQ      depth 9   L6          EQ_nat  depth 11  L6

    Nothing new is admitted to the basis. Each is built from entries already here, which is the
    testable half of "no operator adds power above L4": if one of these needed a primitive the
    basis does not provide, it could not be written as a `Lam` over the library at all. -/

/-- `λn. n (λx.FALSE) TRUE` — a Church numeral applied to a constant-FALSE step returns TRUE only
    when it never steps. The one place a numeral's own iteration is used as a test. -/
def IS_ZERO : Lam := .lam (app2 (.var 0) (.lam FALSE) TRUE)

/-- `λm.λn. n PRED m` — subtraction is iterated predecessor, so it inherits PRED's depth and pays
    two binders on top. Truncated at zero, as Church subtraction always is. -/
def SUB : Lam := .lam (.lam (app2 (.var 0) PRED (.var 1)))

/-- `λm.λn. IS_ZERO (SUB m n)` — m ≤ n exactly when m − n truncates to zero. -/
def LEQ : Lam := .lam (.lam (.app IS_ZERO (app2 SUB (.var 1) (.var 0))))

/-- `λm.λn. AND (LEQ m n) (LEQ n m)` — antisymmetry, spent as a definition. Note this is equality
    on NUMERALS, a decidable domain; equality on the calculus itself is forbidden by Scott-Curry,
    which is why `EQ_bool` carries the same caveat. -/
def EQ_nat : Lam := .lam (.lam (app2 AND (app2 LEQ (.var 1) (.var 0)) (app2 LEQ (.var 0) (.var 1))))

/-! ## Division, modulo, exponentiation — item 2b

    PREDICTIONS, registered in WORKLOG.md and committed before these existed:

        POW  depth 2   L3          DIV  depth 12  L6          MOD  depth 12  L6

    The question the queue asked was whether recursion costs a layer. `Y` has depth 2 and `app`
    takes the max, so a fixpoint is invisible against a body deeper than 2 — predicted free at the
    layer level, expensive only in ticks. -/

/-- `λb.λe. e b`. Exponentiation is APPLICATION: a Church numeral is already an iterator, so
    raising to a power is handing one numeral to another. No new binders, which is why this is
    predicted BELOW `MULT` — it is structurally simpler and only looks harder because of what it
    computes. -/
def POW : Lam := .lam (.lam (.app (.var 0) (.var 1)))

/-- Truncated integer division by repeated subtraction. Normal-order reduction means the unused
    branch is never entered, so no thunking is needed — the reducer's leftmost-outermost strategy
    is doing real work here, not just being a convention. -/
def DIV : Lam :=
  .app Y (.lam (.lam (.lam (
    app2 (app2 LEQ (.var 0) (.var 1))
         (.app SUCC (app2 (.var 2) (app2 SUB (.var 1) (.var 0)) (.var 0)))
         ZERO))))

/-- The same recursion, returning the remainder instead of counting the steps. -/
def MOD : Lam :=
  .app Y (.lam (.lam (.lam (
    app2 (app2 LEQ (.var 0) (.var 1))
         (app2 (.var 2) (app2 SUB (.var 1) (.var 0)) (.var 0))
         (.var 1)))))

/-! ## Lists — item 2c

    PREDICTIONS, registered before these existed:

        NIL depth 2 L3    CONS depth 4 L4    HEAD depth 4 L4    ISNIL depth 5 L5

    and separately: `NIL` is byte-identical to `FALSE` and `ZERO`.

    FOLD-ENCODED, not the pair aliases. Building lists from `PAIR`/`FST`/`SND` would make `CONS`
    and `HEAD` renames of existing entries — true, and a finding, but it mints nothing. `TAIL` is
    absent on purpose: on this encoding it needs the pair-shuffle `PRED` uses, so it belongs with a
    tier carrying that machinery rather than smuggled in here. -/

/-- `λc.λn. n` — the empty fold returns its nil case. Predicted to collide with `FALSE` and `ZERO`,
    which is not a defect: on this encoding the empty list, falsehood and zero ARE one term. -/
def NIL : Lam := .lam (.lam (.var 0))

/-- `λh.λt.λc.λn. c h (t c n)` — cons is the fold's cons case, threading the tail's own fold. -/
def CONS : Lam :=
  .lam (.lam (.lam (.lam (app2 (.var 1) (.var 3) (app2 (.var 2) (.var 1) (.var 0))))))

/-- `λl.λd. l (λh.λt. h) d` — a total head: the default is an ARGUMENT, so there is no partial
    function and no exception. Same discipline as the ternary verdict — the failure case is
    represented rather than thrown. -/
def HEAD : Lam := .lam (.lam (app2 (.var 1) (.lam (.lam (.var 1))) (.var 0)))

/-- `λl. l (λh.λt. FALSE) TRUE` — only the empty fold reaches its nil case. -/
def ISNIL : Lam := .lam (app2 (.var 0) (.lam (.lam FALSE)) TRUE)

/-! ## Iteration over structure — item 2d

    PREDICTIONS, registered before these existed:

        FOLD depth 1 L2    LENGTH depth 6 L5    APPEND depth 6 L5    MAP depth 8 L6

    and separately: `FOLD` is byte-identical to `I`.

    The queue asked where iteration over structure arrives. Predicted answer: NOWHERE, because it
    is already here. A fold-encoded list IS its own fold, so folding is the identity — and what
    `MAP` and `LENGTH` cost is the FUNCTION THEY THREAD, never the traversal. -/

/-- `λl. l`. Folding a fold-encoded list is the identity, so this is `I` under another name — which
    is the point rather than an embarrassment: it prices the encoding's defining property at one
    atom, on the first layer. -/
def FOLD : Lam := .lam (.var 0)

/-- `λl. l (λh.λt. SUCC t) ZERO` — count by ignoring every element and stepping the accumulator. -/
def LENGTH : Lam := .lam (app2 (.var 0) (.lam (.lam (.app SUCC (.var 0)))) ZERO)

/-- `λa.λb. a CONS b` — concatenation is a fold whose nil case is the second list. `CONS` is
    handed in as the step, which is the whole algorithm. -/
def APPEND : Lam := .lam (.lam (app2 (.var 1) CONS (.var 0)))

/-- `λf.λl. l (λh.λt. CONS (f h) t) NIL` — rebuild, applying `f` at each element. -/
def MAP : Lam :=
  .lam (.lam (app2 (.var 0)
    (.lam (.lam (app2 CONS (.app (.var 3) (.var 1)) (.var 0))))
    NIL))

/-! ## Number towers — item 2e

    THE TOWER LAW, registered before these existed: each level of a number tower costs exactly
    +1 binder depth.

        INT_NEG depth 4 L4    RAT_NEG depth 5 L5    CPX_NEG depth 6 L5    INT_ADD depth 6 L5

    Negation is the probe because it is the SAME OPERATION at every level, which isolates the cost
    of the level from the cost of the operation. An integer is a pair of naturals read as a
    difference, a rational a pair of integers, a Gaussian rational a pair of rationals.

    The prediction worth failing on: +1 depth per level is NOT +1 layer per level. Depths 4, 5, 6
    land on L4, L5, L5, because `pool 5 = 7` spans three consecutive depths. Layer boundaries are
    Fibonacci-spaced, so abstraction costs differently depending on where you already are. -/

/-- An integer is `(n, m)` read as `n − m`, so negation is a swap. No arithmetic at all. -/
def INT_NEG : Lam := .lam (app2 PAIR (.app SND (.var 0)) (.app FST (.var 0)))

/-- Componentwise, since `(a−b) + (c−d) = (a+c) − (b+d)`. -/
def INT_ADD : Lam :=
  .lam (.lam (app2 PAIR
    (app2 PLUS (.app FST (.var 1)) (.app FST (.var 0)))
    (app2 PLUS (.app SND (.var 1)) (.app SND (.var 0)))))

/-- A rational is `(num, den)` with `num` an integer, so negation touches only the numerator. -/
def RAT_NEG : Lam := .lam (app2 PAIR (.app INT_NEG (.app FST (.var 0))) (.app SND (.var 0)))

/-- A Gaussian rational is `(re, im)` with both rational, and negation touches both. -/
def CPX_NEG : Lam :=
  .lam (app2 PAIR (.app RAT_NEG (.app FST (.var 0))) (.app RAT_NEG (.app SND (.var 0))))

/-- `(a,b) - (c,d) = (a+d, b+c)`. Subtraction on the difference encoding is addition against the
    swapped pair, so it needs PLUS and nothing else. No truncation at zero, unlike `SUB`. -/
def INT_SUB : Lam :=
  .lam (.lam (app2 PAIR
    (app2 PLUS (.app FST (.var 1)) (.app SND (.var 0)))
    (app2 PLUS (.app SND (.var 1)) (.app FST (.var 0)))))

/-- `(a,b) * (c,d) = (a*c + b*d, a*d + b*c)`. The cross terms ARE the sign rule, spent as
    arithmetic on numerals rather than as a case split. -/
def INT_MUL : Lam :=
  .lam (.lam (app2 PAIR
    (app2 PLUS (app2 MULT (.app FST (.var 1)) (.app FST (.var 0)))
               (app2 MULT (.app SND (.var 1)) (.app SND (.var 0))))
    (app2 PLUS (app2 MULT (.app FST (.var 1)) (.app SND (.var 0)))
               (app2 MULT (.app SND (.var 1)) (.app FST (.var 0))))))

/-- `n` becomes `(n, ZERO)`. Lifting a numeral into the integers is one pairing and no
    arithmetic, which is what makes the level boundary cheap. -/
def INT_FROM : Lam := .lam (app2 PAIR (.var 0) ZERO)

/-- `(p,q) + (r,s) = (p * INT_FROM s + r * INT_FROM q, q*s)`. A denominator is a NUMERAL and a
    numerator is an INTEGER, so the denominators must be lifted before they can cross-multiply. -/
def RAT_ADD : Lam :=
  .lam (.lam (app2 PAIR
    (app2 INT_ADD
      (app2 INT_MUL (.app FST (.var 1)) (.app INT_FROM (.app SND (.var 0))))
      (app2 INT_MUL (.app FST (.var 0)) (.app INT_FROM (.app SND (.var 1)))))
    (app2 MULT (.app SND (.var 1)) (.app SND (.var 0)))))

/-- `(p,q) * (r,s) = (INT_MUL p r, MULT q s)`. The two components multiply in DIFFERENT
    arithmetics, and no lifting is needed because neither one crosses the other. -/
def RAT_MUL : Lam :=
  .lam (.lam (app2 PAIR
    (app2 INT_MUL (.app FST (.var 1)) (.app FST (.var 0)))
    (app2 MULT (.app SND (.var 1)) (.app SND (.var 0)))))

/-- `(a,b) + (c,d) = (RAT_ADD a c, RAT_ADD b d)`. Componentwise, exactly as `CPX_NEG` is:
    addition never mixes the real and imaginary parts, so the level costs one pairing. -/
def CPX_ADD : Lam :=
  .lam (.lam (app2 PAIR
    (app2 RAT_ADD (.app FST (.var 1)) (.app FST (.var 0)))
    (app2 RAT_ADD (.app SND (.var 1)) (.app SND (.var 0)))))

/-! ## Linear algebra — item 2f

    REGISTERED BEFORE MINTING: matrix multiplication does NOT climb the ladder with size.
    `mkMatMul n` has depth 6 and layer L5 for every n, against `mkAdder n`'s `2n + 8`.

    The reason is the whole content of the layer index. Element access is `FST` after some `SND`s,
    and application takes the MAX, so reaching element 47 is exactly as deep as reaching element 0.
    Summing n products nests `PLUS`; building the result nests `PAIR`; both take the max too.
    Nothing in that chain grows with n.

    An n-bit adder holds `2n` bits open AS BINDERS, so its depth is forced to grow. An n x n
    matmul holds TWO MATRICES open, whatever n is. Same explosion in work, opposite layer
    behaviour — because layer counts what is held open at once, not how much is computed. -/

/-- Element `i` of a nested-pair vector: `SND` down, then `FST`. Depth 3 for every `i`, which is
    the fact the whole prediction rests on. -/
def vecAt (v : Lam) : Nat → Lam
  | 0    => .app FST v
  | i+1  => vecAt (.app SND v) i

/-- Entry `(i, j)`: row then column. -/
def matAt (m : Lam) (i j : Nat) : Lam := vecAt (vecAt m i) j

/-- A nested-pair vector, `ZERO` terminated. -/
def mkVec : List Lam → Lam
  | []      => ZERO
  | x :: xs => app2 PAIR x (mkVec xs)

def sumOf : List Lam → Lam
  | []      => ZERO
  | [x]     => x
  | x :: xs => app2 PLUS x (sumOf xs)

/-- The `(i, j)` entry of the product: sum over `k` of `A[i][k] * B[k][j]`. -/
def dotAt (a b : Lam) (n i j : Nat) : Lam :=
  sumOf ((List.range n).map fun k => app2 MULT (matAt a i k) (matAt b k j))

/-- An `n x n` matrix multiply, CONSTRUCTED for any `n`, not transcribed — the same move as
    `mkAdder`. Two binders: the two matrices. That is the entire reason depth is flat. -/
def mkMatMul (n : Nat) : Lam :=
  .lam (.lam (mkVec ((List.range n).map fun i =>
    mkVec ((List.range n).map fun j => dotAt (.var 1) (.var 0) n i j))))

/-! ## Proposed by the discovery loop — item 7b

    These were not chosen. `differential.jsonl` holds a normal form for all 2622 closed terms of
    size <= 9; grouping by normal form gives 1065 distinct behaviours, of which only 13 were named
    here. Ranking the unnamed by PREIMAGE COUNT — how many terms fall into each behaviour, the
    Hebbian statistic — put these at the top, and the search did not know what a projection was.

    PREDICTIONS, registered before minting:
        OMEGA_W depth 1 L2      P3_i depth 3 L4      P4_i depth 4 L4

    THE FAMILY THE LIBRARY STOPPED SHORT OF. `I`, `K` and `FALSE` are the arity-1 and arity-2
    projections. Nobody minted arity 3 or 4; 58, 52 and 52 terms respectively fall into them. -/

/-- `λx. x x` — the duplicator, 32 preimages, and **the correction the loop found**.

    Its binder depth is 1, identical to `I`, so it sits at L2. The ladder's gloss for L2 reads
    "a term can only give back what it was handed — so the only thing assertable is that a thing is
    itself." That is FALSE: this term is at L2 and gives back the argument applied to ITSELF.
    Self-application, the seed of `Y` and of non-termination, is available at the FIRST layer, not
    at L3 where `Y` is recorded. -/
def OMEGA_W : Lam := .lam (.app (.var 0) (.var 0))

def P3_0 : Lam := .lam (.lam (.lam (.var 0)))
def P3_1 : Lam := .lam (.lam (.lam (.var 1)))
def P3_2 : Lam := .lam (.lam (.lam (.var 2)))
def P4_0 : Lam := .lam (.lam (.lam (.lam (.var 0))))
def P4_1 : Lam := .lam (.lam (.lam (.lam (.var 1))))
def P4_2 : Lam := .lam (.lam (.lam (.lam (.var 2))))
def P4_3 : Lam := .lam (.lam (.lam (.lam (.var 3))))

/-- FOUND BY `lake exe mint`, and this time by a SORT rather than by an agent reading an
    unsorted table. Rank key net = k*(d-1) - d over the 39-behaviour gap, net DESCENDING with ties
    broken by behaviour key ascending; selection on the BEHAVIOUR KEY, never on the name.

    net 20 = 22*(2-1) - 2: the targets' subterms need this behaviour 22 times, the ledger
    lacked it, and `(INT_FROM TWO)` reaches it from names that are already certified.

    THE ENCODING RULE, applied mechanically: the discovery route `.app INT_FROM TWO` is 23 tokens and its
    normal form is 14, so the NORMAL FORM is what is stored. Item 70's mint gate refuses an
    entry that enumeration expresses more cheaply. -/
def INT_FROM_TWO : Lam := .lam (.app (.app (.var 0) (.lam (.lam (.app (.var 1) (.app (.var 1) (.var 0)))))) (.lam (.lam (.var 0))))

/-! ## Minted, but never published by the ladder

    These carry `published := false`, so gate 0 does not score against them —
    there is no recorded placement to score against. Their layers are computed
    exactly as every other row's is. -/

def derived : List Node :=
  [ { name := "ONE",     term := ONE,     published := false }
  , { name := "TWO",     term := TWO,     published := false }
  , { name := "EQ_bool", term := EQ_bool, published := false }
  , { name := "FIB",     term := FIB,     published := false }
  , { name := "IS_ZERO", term := IS_ZERO, published := false }
  , { name := "SUB",     term := SUB,     published := false }
  , { name := "LEQ",     term := LEQ,     published := false }
  , { name := "EQ_nat",  term := EQ_nat,  published := false }
  , { name := "POW",     term := POW,     published := false }
  , { name := "DIV",     term := DIV,     published := false }
  , { name := "MOD",     term := MOD,     published := false }
  , { name := "NIL",     term := NIL,     published := false }
  , { name := "CONS",    term := CONS,    published := false }
  , { name := "HEAD",    term := HEAD,    published := false }
  , { name := "ISNIL",   term := ISNIL,   published := false }
  , { name := "FOLD",    term := FOLD,    published := false }
  , { name := "LENGTH",  term := LENGTH,  published := false }
  , { name := "APPEND",  term := APPEND,  published := false }
  , { name := "MAP",     term := MAP,     published := false }
  , { name := "INT_NEG", term := INT_NEG, published := false }
  , { name := "INT_ADD", term := INT_ADD, published := false }
  , { name := "RAT_NEG", term := RAT_NEG, published := false }
  , { name := "CPX_NEG", term := CPX_NEG, published := false }
  , { name := "INT_SUB", term := INT_SUB, published := false }
  , { name := "INT_MUL", term := INT_MUL, published := false }
  , { name := "INT_FROM", term := INT_FROM, published := false }
  , { name := "RAT_ADD", term := RAT_ADD, published := false }
  , { name := "RAT_MUL", term := RAT_MUL, published := false }
  , { name := "CPX_ADD", term := CPX_ADD, published := false }
  , { name := "OMEGA_W", term := OMEGA_W, published := false }
  , { name := "P3_0",    term := P3_0,    published := false }
  , { name := "P3_1",    term := P3_1,    published := false }
  , { name := "P3_2",    term := P3_2,    published := false }
  , { name := "P4_0",    term := P4_0,    published := false }
  , { name := "P4_1",    term := P4_1,    published := false }
  , { name := "P4_2",    term := P4_2,    published := false }
  , { name := "P4_3",    term := P4_3,    published := false }
  , { name := "INT_FROM_TWO", term := INT_FROM_TWO, published := false }
  ]

def ledger : List Node := recorded ++ derived

end Ladderlang
