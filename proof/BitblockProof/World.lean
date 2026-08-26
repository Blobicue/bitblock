import Std

namespace BitblockProof

/-- An address in the abstract Bitblock universe. `Int` makes every axis unbounded. -/
structure Coord where
  x : Int
  y : Int
  z : Int
deriving DecidableEq, Repr

/--
A world stores only coordinates whose bit is on. A list is deliberately used as
the tiny trusted representation: it is finite by construction. World equality
below is observational, so list order and duplicate entries carry no meaning.
-/
abbrev World := List Coord

/-- The immutable bedrock coordinate and the one-bit initial world used by Bitblock. -/
def origin : Coord := { x := 0, y := 0, z := 0 }

def initialWorld : World := [origin]

/-- The observable bit at a coordinate. -/
def bit (world : World) (coord : Coord) : Prop := coord ∈ world

instance (world : World) (coord : Coord) : Decidable (bit world coord) :=
  inferInstanceAs (Decidable (coord ∈ world))

theorem initial_world_bit (coord : Coord) :
    bit initialWorld coord ↔ coord = origin := by
  simp [bit, initialWorld]

/-- Two representations describe the same infinite bit-space. -/
def SameWorld (left right : World) : Prop :=
  ∀ coord, bit left coord ↔ bit right coord

/-- A valid world always contains the immutable bedrock bit at the origin. -/
def ValidWorld (world : World) : Prop := bit world origin

theorem initial_world_valid : ValidWorld initialWorld := by
  simp [ValidWorld, bit, initialWorld]

/-- The first Bitblock instruction language. -/
inductive Edit where
  | on (coord : Coord)
  | off (coord : Coord)
  | toggle (coord : Coord)
deriving DecidableEq, Repr

/-- Remove every stored occurrence of one coordinate. -/
def remove (world : World) (coord : Coord) : World :=
  world.filter (· != coord)

/-- Execute one instruction. -/
def applyEdit (world : World) : Edit → World
  | .on coord => coord :: world
  | .off coord =>
      if coord = origin then world else remove world coord
  | .toggle coord =>
      if coord = origin then world
      else if bit world coord then remove world coord else coord :: world

/-- Execute an ordered edit program. -/
def replay : World → List Edit → World
  | world, [] => world
  | world, edit :: rest => replay (applyEdit world edit) rest

theorem bit_on (world : World) (placed queried : Coord) :
    bit (applyEdit world (.on placed)) queried ↔
      queried = placed ∨ bit world queried := by
  simp [applyEdit, bit, eq_comm]

theorem bit_off (world : World) (removed queried : Coord) :
    bit (applyEdit world (.off removed)) queried ↔
      if removed = origin then bit world queried
      else queried ≠ removed ∧ bit world queried := by
  by_cases bedrock : removed = origin
  · simp [applyEdit, bedrock]
  · simp [applyEdit, bedrock, remove, bit, and_comm]

theorem bit_toggle (world : World) (changed queried : Coord) :
    bit (applyEdit world (.toggle changed)) queried ↔
      if changed = origin then bit world queried
      else if queried = changed then ¬bit world queried else bit world queried := by
  by_cases bedrock : changed = origin
  · simp [applyEdit, bedrock]
  · by_cases present : bit world changed
    · change changed ∈ world at present
      by_cases same : queried = changed
      · subst queried
        simp [applyEdit, bedrock, present, remove, bit]
      · simp [applyEdit, bedrock, present, same, remove, bit]
    · change ¬changed ∈ world at present
      by_cases same : queried = changed
      · subst queried
        simp [applyEdit, bedrock, present, bit]
      · simp [applyEdit, bedrock, present, same, bit]

theorem off_origin_noop (world : World) :
    applyEdit world (.off origin) = world := by
  simp [applyEdit]

theorem toggle_origin_noop (world : World) :
    applyEdit world (.toggle origin) = world := by
  simp [applyEdit]

theorem applyEdit_preserves_validity (world : World) (edit : Edit)
    (valid : ValidWorld world) : ValidWorld (applyEdit world edit) := by
  cases edit with
  | on coord =>
      rw [ValidWorld, bit_on]
      exact Or.inr valid
  | off coord =>
      by_cases bedrock : coord = origin
      · subst coord
        simpa [off_origin_noop] using valid
      · have origin_ne : origin ≠ coord := Ne.symm bedrock
        rw [ValidWorld, bit_off]
        simp only [bedrock, if_false]
        exact ⟨origin_ne, valid⟩
  | toggle coord =>
      by_cases bedrock : coord = origin
      · subst coord
        simpa [toggle_origin_noop] using valid
      · have origin_ne : origin ≠ coord := Ne.symm bedrock
        have valid_origin : bit world origin := by
          simpa [ValidWorld] using valid
        rw [ValidWorld, bit_toggle]
        simpa [bedrock, origin_ne] using valid_origin

theorem on_idempotent (world : World) (coord : Coord) :
    SameWorld
      (applyEdit (applyEdit world (.on coord)) (.on coord))
      (applyEdit world (.on coord)) := by
  intro queried
  simp [bit_on]

theorem off_idempotent (world : World) (coord : Coord) :
    SameWorld
      (applyEdit (applyEdit world (.off coord)) (.off coord))
      (applyEdit world (.off coord)) := by
  intro queried
  by_cases bedrock : coord = origin
  · subst coord
    simp [off_origin_noop]
  · simp [bit_off, bedrock]

theorem toggle_involutive (world : World) (coord : Coord) :
    SameWorld
      (applyEdit (applyEdit world (.toggle coord)) (.toggle coord))
      world := by
  intro queried
  by_cases bedrock : coord = origin
  · subst coord
    simp [toggle_origin_noop]
  · by_cases same : queried = coord
    · subst queried
      simp [bit_toggle, bedrock]
    · simp [bit_toggle, bedrock, same]

theorem replay_append (world : World) (first second : List Edit) :
    replay world (first ++ second) = replay (replay world first) second := by
  induction first generalizing world with
  | nil => rfl
  | cons edit rest ih =>
      simp only [List.cons_append, replay]
      exact ih (applyEdit world edit)

theorem replay_preserves_validity (world : World) (edits : List Edit)
    (valid : ValidWorld world) : ValidWorld (replay world edits) := by
  induction edits generalizing world with
  | nil => exact valid
  | cons edit rest ih =>
      exact ih (applyEdit world edit) (applyEdit_preserves_validity world edit valid)

theorem replay_initial_world_valid (edits : List Edit) :
    ValidWorld (replay initialWorld edits) := by
  exact replay_preserves_validity initialWorld edits initial_world_valid

theorem finite_support (world : World) :
    ∃ support : List Coord, ∀ coord, bit world coord ↔ coord ∈ support := by
  exact ⟨world, fun _ => Iff.rfl⟩

end BitblockProof
