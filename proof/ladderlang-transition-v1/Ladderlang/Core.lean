/-
  LADDERLANG — CORE

  The whole language. Everything else in this project is data.

  Two representations, per DESIGN.md §3:
    Lam   — binder-carrying. What you WRITE. Depth and therefore LAYER live here.
    Coin  — iota + juxtaposition. What it IS. Cost lives here.
  Bracket abstraction is the bridge.
-/

namespace Ladderlang

/-! ## The floor -/

/-- The mint rule, verbatim. `iota` is the base, `tick` is the closure, and
    `inductive` is the minimality clause — which Lean gives for free. -/
inductive Coin where
  | iota : Coin
  | tick : Coin → Coin → Coin
  deriving Repr, BEq, Inhabited

/-- The binder-carrying source, de Bruijn. Depth is BINDER depth, and `Coin`
    has no binders, so this representation is required, not a convenience. -/
inductive Lam where
  | var : Nat → Lam
  | lam : Lam → Lam
  | app : Lam → Lam → Lam
  deriving Repr, BEq, Inhabited, DecidableEq

/-! ## Depth — and the two rival measures the ladder scored against it -/

/-- Nesting depth: the maximum number of binders on any root-to-leaf path.
    An abstraction adds one; an application takes the max. This is how many
    arguments a term must hold open AT ONCE — a register count. -/
def Lam.binderDepth : Lam → Nat
  | .var _   => 0
  | .lam b   => 1 + b.binderDepth
  | .app f a => max f.binderDepth a.binderDepth

/-- RIVAL. Total binders: an application SUMS instead of taking the max.
    Scored 20/24 against the recorded placements. -/
def Lam.totalBinders : Lam → Nat
  | .var _   => 0
  | .lam b   => 1 + b.totalBinders
  | .app f a => f.totalBinders + a.totalBinders

def Lam.size : Lam → Nat
  | .var _   => 1
  | .lam b   => 1 + b.size
  | .app f a => 1 + f.size + a.size

/-! ## The pools -/

def fib : Nat → Nat
  | 0     => 0
  | 1     => 1
  | n+2   => fib n + fib (n+1)

/-- POSIT. `pool L = F(L+1) − 1` = 1, 2, 4, 7, 12, 20, 33, 54.
    This comes from H, the history operator, and inherits its axiom status.
    Coda V: the layer ORDER is derived; the layer CUTS are posited. -/
def poolFib (L : Nat) : Nat := fib (L + 1) - 1

theorem fib_succ_pos : ∀ n, 1 ≤ fib (n + 1) := by
  intro n
  induction n with
  | zero => simp [fib]
  | succ k ih =>
      change 1 ≤ fib k + fib (k + 1)
      exact Nat.le_trans ih (Nat.le_add_left (fib (k + 1)) (fib k))

/-- The subtract-one in `poolFib` does not truncate. -/
theorem poolFib_add (L : Nat) : poolFib L + 1 = fib (L + 1) :=
  Nat.sub_add_cancel (fib_succ_pos L)

/-- RIVAL POOL. Doubling. Scored 22/24. -/
def poolDouble (L : Nat) : Nat := 2 ^ (L - 2)

/-- RIVAL POOL. Linear. Scored 16/24. -/
def poolLinear (L : Nat) : Nat := L - 1

/-- RIVAL POOL. Triangular. Scored 10/24. -/
def poolTriangular (L : Nat) : Nat := ((L - 1) * L) / 2

/-! ## Layer — computed, never annotated -/

/-- The lowest layer whose pool covers the depth. There is no field to fill in
    and no override, so layer misattribution is unconstructible. -/
def layerGo (pool : Nat → Nat) (d : Nat) : Nat → Nat → Nat
  | _, 0      => 0
  | L, fuel+1 => if d ≤ pool L then L else layerGo pool d (L+1) fuel

def layerWith (pool : Nat → Nat) (d : Nat) : Nat := layerGo pool d 2 128

def Lam.layer (t : Lam) : Nat := layerWith poolFib t.binderDepth

/-! ## Bracket abstraction — the bridge from Lam to Coin -/

/-- Named intermediate. Bracket abstraction is stated over names, so de Bruijn
    is converted to levels first. -/
inductive NLam where
  | fv  : Nat → NLam
  | abs : Nat → NLam → NLam
  | app : NLam → NLam → NLam
  deriving Repr, BEq, Inhabited

/-- de Bruijn index → level, so each binder gets a globally distinct name. -/
def toNamed (d : Nat) : Lam → NLam
  | .var i   => .fv (d - 1 - i)
  | .lam b   => .abs d (toNamed (d+1) b)
  | .app f a => .app (toNamed d f) (toNamed d a)

/-- SKI with variables, the working type during abstraction. -/
inductive SKIv where
  | S | K | I
  | var : Nat → SKIv
  | app : SKIv → SKIv → SKIv
  deriving Repr, BEq, Inhabited

def SKIv.freeIn (x : Nat) : SKIv → Bool
  | .var y   => x == y
  | .app f a => f.freeIn x || a.freeIn x
  | _        => false

/-- A[x, E]. Naive — no eta step, no optimisation beyond the not-free rule.
    This is the algorithm the ladder priced with, so its atom counts are the
    ones gate 1 checks against.

    Structurally recursive on `e`, so Lean verifies termination and the
    defining equations are available for reasoning. -/
def abstr (x : Nat) : SKIv → SKIv
  | .var y     => if x == y then .I else .app .K (.var y)
  | .app e1 e2 =>
      if (SKIv.app e1 e2).freeIn x then
        .app (.app .S (abstr x e1)) (abstr x e2)
      else .app .K (.app e1 e2)
  | .S         => .app .K .S
  | .K         => .app .K .K
  | .I         => .app .K .I

/-- T[E]. -/
def transl : NLam → SKIv
  | .fv x    => .var x
  | .app f a => .app (transl f) (transl a)
  | .abs x b => abstr x (transl b)

def SKIv.atoms : SKIv → Nat
  | .app f a => f.atoms + a.atoms
  | _        => 1

/-- ι encodings, verified extensionally by the ladder:
    `ι ι → I` (2 leaves), `ι(ι(ι ι)) → K` (4), `ι(ι(ι(ι ι))) → S` (5). -/
def coinI : Coin := .tick .iota .iota
def coinK : Coin := .tick .iota (.tick .iota (.tick .iota .iota))
def coinS : Coin := .tick .iota (.tick .iota (.tick .iota (.tick .iota .iota)))

def SKIv.toCoin : SKIv → Coin
  | .S       => coinS
  | .K       => coinK
  | .I       => coinI
  | .var _   => .iota
  | .app f a => .tick f.toCoin a.toCoin

def Coin.leaves : Coin → Nat
  | .iota     => 1
  | .tick f a => f.leaves + a.leaves

/-- Every coin has at least one ι. The currency's subtract-one therefore
    does not truncate on the type the price is computed from. An arbitrary
    `Cost` can still hold `iotaUpper = 0`; a `Coin` cannot. -/
theorem Coin.leaves_pos : ∀ c : Coin, 1 ≤ c.leaves := by
  intro c
  induction c with
  | iota => exact Nat.le_refl 1
  | tick f a ihf iha =>
      exact Nat.le_trans (Nat.le_add_right 1 1) (Nat.add_le_add ihf iha)

theorem Coin.leaves_tick (f a : Coin) :
    (Coin.tick f a).leaves = f.leaves + a.leaves := rfl

/-- The truncating subtraction is an inverse on this type. -/
theorem Coin.leaves_succ (c : Coin) : c.leaves - 1 + 1 = c.leaves :=
  Nat.sub_add_cancel (Coin.leaves_pos c)

/-- No variables. Bracket abstraction eliminates every variable, so everything
    in the ledger is closed — which is exactly the condition under which the
    encoding is uniquely decodable. -/
def SKIv.closed : SKIv → Bool
  | .var _   => false
  | .app f a => f.closed && a.closed
  | _        => true

/-- The other half of THE CODEC LAW: every change of representation is a pair
    whose composition is the identity, verified, or it is a loss.

    The discriminator is the left child. A combinator encoding always has
    `iota` on the left; an application always has a tick, because for a
    variable-free term every encoding is tick-rooted. -/
def decodeCoin : Coin → Option SKIv
  | .tick .iota .iota                                              => some .I
  | .tick .iota (.tick .iota (.tick .iota .iota))                  => some .K
  | .tick .iota (.tick .iota (.tick .iota (.tick .iota .iota)))    => some .S
  | .tick (.tick a b) r =>
      match decodeCoin (.tick a b), decodeCoin r with
      | some f, some x => some (.app f x)
      | _,      _      => none
  | _ => none

/-- Lam → SKI. -/
def Lam.toSKI (t : Lam) : SKIv := transl (toNamed 0 t)

/-- Lam → Coin. Computed, never stored beside the source. -/
def Lam.toCoin (t : Lam) : Coin := t.toSKI.toCoin

/-! ## Cost — an upper bound, and the type says so -/

/-- There is no `exact` field and no `minimum` constructor. Every count here
    comes from naive bracket abstraction, which does not minimise: exhaustive
    search to size 7 found TRUE at 1 atom, FALSE at 2, I at 3. Claiming a
    minimum requires a proof, which is the correct price. -/
structure Cost where
  atomsUpper : Nat
  iotaUpper  : Nat
  deriving Repr, BEq, Inhabited

def Lam.cost (t : Lam) : Cost :=
  { atomsUpper := t.toSKI.atoms, iotaUpper := t.toCoin.leaves }

/-- One juxtaposition is one tick — the unit of cost and the unit of time at once.
    Truncates at zero on an arbitrary `Cost`. On every `Lam` the next two
    theorems say it does not. -/
def Cost.ticks (c : Cost) : Nat := c.iotaUpper - 1

theorem Lam.cost_iota_pos (t : Lam) : 1 ≤ t.cost.iotaUpper := by
  simp [Lam.cost]
  exact Coin.leaves_pos _

theorem Lam.cost_ticks_succ (t : Lam) : t.cost.ticks + 1 = t.cost.iotaUpper := by
  simp [Cost.ticks, Lam.cost]
  exact Coin.leaves_succ _

/-! ## Reduction — and the verdict is ternary -/

def shiftUp (c : Nat) : Lam → Lam
  | .var k   => if k < c then .var k else .var (k+1)
  | .lam b   => .lam (shiftUp (c+1) b)
  | .app f a => .app (shiftUp c f) (shiftUp c a)

def shiftDown (c : Nat) : Lam → Lam
  | .var k   => if k < c then .var k else .var (k-1)
  | .lam b   => .lam (shiftDown (c+1) b)
  | .app f a => .app (shiftDown c f) (shiftDown c a)

def substAt (j : Nat) (s : Lam) : Lam → Lam
  | .var k   => if k == j then s else .var k
  | .lam b   => .lam (substAt (j+1) (shiftUp 0 s) b)
  | .app f a => .app (substAt j s f) (substAt j s a)

def betaAt (b arg : Lam) : Lam := shiftDown 0 (substAt 0 (shiftUp 0 arg) b)

/-- Is the head a redex? Pulled out as an `Option` so the reducer's case split
    is on a non-recursive value — which keeps `Laws.step_sound` provable. -/
def Lam.lamBody : Lam → Option Lam
  | .lam b => some b
  | _      => none

/-- Leftmost-outermost. Structurally recursive, so the equations are available
    for reasoning — `Laws.step_sound` proves every step is a genuine β-step and
    `Laws.step_none_normal` proves that stopping means normal form.

    NOT proved here: that leftmost-outermost is COMPLETE (reaches a normal form
    whenever one exists). That is the standardisation theorem, cited from the
    literature, not established in this repo. -/
def step : Lam → Option Lam
  | .var _   => none
  | .lam b   => (step b).map Lam.lam
  | .app f a =>
      match f.lamBody with
      | some b => some (betaAt b a)
      | none   =>
          match step f with
          | some f' => some (.app f' a)
          | none    => (step a).map (Lam.app f)

/-- THE OBSERVER-SHAPED HOLE, in a function signature.
    HOLD is decided by the calculus. GREW and RAN are decided by whoever holds
    the fuel bound — which is not the term. A boolean here would be the
    bottom-conflation defect; the third value is present from the first line. -/
inductive Verdict where
  | HOLD : Lam → Verdict
  | GREW : Verdict
  | RAN  : Verdict
  deriving Repr, BEq, Inhabited

/-- The arena, carried explicitly. A run cannot widen its own bound mid-flight. -/
structure Arena where
  fuel    : Nat := 10000
  sizeCap : Nat := 100000
  deriving Repr, Inhabited

/-- Structurally recursive on the FUEL, which is what makes it total: the
    budget is in the term's verdict, never ambient. -/
def reduceFuel : Nat → Nat → Lam → Verdict
  | 0,   _,   _ => .RAN
  | f+1, cap, t =>
      if t.size > cap then .GREW
      else match step t with
        | none    => .HOLD t
        | some t' => reduceFuel f cap t'

def reduceIn (a : Arena) (t : Lam) : Verdict := reduceFuel a.fuel a.sizeCap t

def Lam.eval (t : Lam) (a : Arena := {}) : Verdict := reduceIn a t

end Ladderlang
