import BitblockProof.World

namespace BitblockProof

/-!
This file is a finite, duplicate-free model of Bitblock mint/burn accounting.
It is intentionally abstract: it is not a refinement proof for the Rust world,
the renderer, a hash function, a receipt encoding, or a consensus protocol.
-/

/-- A finite list of on-bits together with a construction-time no-duplicates proof. -/
structure SupplyWorld where
  bits : List Coord
  nodup : bits.Nodup

/-- Supply counts mutable on-bits only; immutable origin bedrock is erased first. -/
def mutableSupply (world : SupplyWorld) : Nat :=
  (world.bits.erase origin).length

/-- Turn a mutable bit on. Origin and an already-on coordinate are no-ops. -/
def mint (world : SupplyWorld) (coord : Coord) : SupplyWorld :=
  if _atOrigin : coord = origin then world
  else if present : coord ∈ world.bits then world
  else
    { bits := coord :: world.bits
      nodup := List.nodup_cons.mpr ⟨present, world.nodup⟩ }

/-- Turn a mutable bit off. Origin and an already-off coordinate are no-ops. -/
def burn (world : SupplyWorld) (coord : Coord) : SupplyWorld :=
  if _atOrigin : coord = origin then world
  else if _present : coord ∈ world.bits then
    { bits := world.bits.erase coord
      nodup := world.nodup.erase coord }
  else world

inductive SupplyEdit where
  | mint (coord : Coord)
  | burn (coord : Coord)
deriving DecidableEq, Repr

def applySupplyEdit (world : SupplyWorld) : SupplyEdit → SupplyWorld
  | .mint coord => mint world coord
  | .burn coord => burn world coord

def replaySupply : SupplyWorld → List SupplyEdit → SupplyWorld
  | world, [] => world
  | world, edit :: rest => replaySupply (applySupplyEdit world edit) rest

/-- Mutable supply is total duplicate-free support minus origin exactly when present. -/
theorem mutable_supply_excludes_origin (world : SupplyWorld) :
    mutableSupply world =
      if origin ∈ world.bits then world.bits.length - 1 else world.bits.length := by
  by_cases hasOrigin : origin ∈ world.bits
  · simp [mutableSupply, hasOrigin, List.length_erase_of_mem]
  · simp [mutableSupply, hasOrigin, List.erase_of_not_mem]

theorem mint_origin_noop (world : SupplyWorld) :
    mint world origin = world := by
  simp [mint]

theorem burn_origin_noop (world : SupplyWorld) :
    burn world origin = world := by
  simp [burn]

theorem mint_present_noop (world : SupplyWorld) (coord : Coord)
    (present : coord ∈ world.bits) : mint world coord = world := by
  simp [mint, present]

theorem burn_absent_noop (world : SupplyWorld) (coord : Coord)
    (absent : coord ∉ world.bits) : burn world coord = world := by
  simp [burn, absent]

/-- Mint increases mutable supply exactly for an absent, non-origin coordinate. -/
theorem mint_increments_supply_iff (world : SupplyWorld) (coord : Coord) :
    mutableSupply (mint world coord) = mutableSupply world + 1 ↔
      coord ≠ origin ∧ coord ∉ world.bits := by
  by_cases atOrigin : coord = origin
  · simp [mint, mutableSupply, atOrigin]
  · by_cases present : coord ∈ world.bits
    · simp [mint, mutableSupply, atOrigin, present]
    · simp [mint, mutableSupply, atOrigin, present]

/-- Burn decreases mutable supply exactly for a present, non-origin coordinate. -/
theorem burn_decrements_supply_iff (world : SupplyWorld) (coord : Coord) :
    mutableSupply (burn world coord) + 1 = mutableSupply world ↔
      coord ≠ origin ∧ coord ∈ world.bits := by
  by_cases atOrigin : coord = origin
  · simp [burn, mutableSupply, atOrigin]
  · by_cases present : coord ∈ world.bits
    · have presentMutable : coord ∈ world.bits.erase origin := by
        simp [atOrigin, present]
      have shorter := List.length_erase_of_mem presentMutable
      constructor
      · intro _
        exact ⟨atOrigin, present⟩
      · intro _
        simp only [burn, atOrigin, present, ↓reduceDIte, mutableSupply]
        rw [List.erase_comm coord origin]
        rw [shorter]
        apply Nat.sub_add_cancel
        cases mutableBits : world.bits.erase origin with
        | nil => simp [mutableBits] at presentMutable
        | cons head tail => simp
    · simp [burn, mutableSupply, atOrigin, present]

/-- Every protected or semantically redundant edit preserves mutable supply. -/
theorem protected_or_noop_preserves_supply (world : SupplyWorld) (edit : SupplyEdit)
    (noop : applySupplyEdit world edit = world) :
    mutableSupply (applySupplyEdit world edit) = mutableSupply world := by
  rw [noop]

/-- Supply replay never removes origin from a world that already contains it. -/
theorem applySupplyEdit_preserves_origin (world : SupplyWorld) (edit : SupplyEdit)
    (hasOrigin : origin ∈ world.bits) : origin ∈ (applySupplyEdit world edit).bits := by
  cases edit with
  | mint coord =>
      by_cases atOrigin : coord = origin
      · simpa [applySupplyEdit, mint, atOrigin] using hasOrigin
      · by_cases present : coord ∈ world.bits
        · simpa [applySupplyEdit, mint, atOrigin, present] using hasOrigin
        · simp [applySupplyEdit, mint, atOrigin, present, hasOrigin]
  | burn coord =>
      by_cases atOrigin : coord = origin
      · simpa [applySupplyEdit, burn, atOrigin] using hasOrigin
      · by_cases present : coord ∈ world.bits
        · have originNe : origin ≠ coord := Ne.symm atOrigin
          simpa [applySupplyEdit, burn, atOrigin, present, originNe] using hasOrigin
        · simpa [applySupplyEdit, burn, atOrigin, present] using hasOrigin

theorem replaySupply_preserves_origin (world : SupplyWorld) (edits : List SupplyEdit)
    (hasOrigin : origin ∈ world.bits) : origin ∈ (replaySupply world edits).bits := by
  induction edits generalizing world with
  | nil => exact hasOrigin
  | cons edit rest ih =>
      exact ih (applySupplyEdit world edit)
        (applySupplyEdit_preserves_origin world edit hasOrigin)

end BitblockProof
