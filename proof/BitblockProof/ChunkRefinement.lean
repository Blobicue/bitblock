import BitblockProof.World

namespace BitblockProof

/-!
Chunk coordinate refinement, 3D local voxel index bijection, octant hierarchy
division, and negative-coordinate Euclidean vs. truncated division controls
corresponding to `crates/core/src/world.rs` (`split_coord` / `join_coord`).
-/

/-- Signed 64-bit integer coordinate bounds matching Rust `i64::MIN..=i64::MAX`. -/
def inI64Bounds (x : Int) : Prop :=
  -9223372036854775808 ≤ x ∧ x ≤ 9223372036854775807

/-- Euclidean chunk quotient and local offset in `0..16` for one coordinate axis. -/
def splitAxis (x : Int) : Int × Nat := (x / 16, (x % 16).toNat)

/-- Reconstruct a world coordinate axis from a chunk coordinate and local offset. -/
def joinAxis (cx : Int) (lx : Nat) : Int := cx * 16 + Int.ofNat lx

/-- `Int` `/` and `%` in Lean 4 are definitionally `Int.ediv` and `Int.emod`. -/
theorem splitAxis_eq_ediv_emod (x : Int) :
    splitAxis x = (x.ediv 16, (x.emod 16).toNat) := rfl

theorem splitAxis_local_lt (x : Int) :
    (splitAxis x).2 < 16 := by
  simp [splitAxis]
  omega

theorem joinAxis_splitAxis (x : Int) :
    joinAxis (splitAxis x).1 (splitAxis x).2 = x := by
  simp [splitAxis, joinAxis]
  omega

theorem splitAxis_joinAxis (cx : Int) (lx : Nat) (hlx : lx < 16) :
    splitAxis (joinAxis cx lx) = (cx, lx) := by
  ext <;> simp [splitAxis, joinAxis] <;> omega

theorem chunk_coord_bounds (x : Int) (hx : inI64Bounds x) :
    -576460752303423488 ≤ (splitAxis x).1 ∧ (splitAxis x).1 ≤ 576460752303423487 := by
  simp [splitAxis, inI64Bounds] at *
  omega

theorem splitAxis_neg_one :
    splitAxis (-1) = (-1, 15) := by
  decide

theorem splitAxis_neg_sixteen :
    splitAxis (-16) = (-1, 0) := by
  decide

theorem splitAxis_neg_seventeen :
    splitAxis (-17) = (-2, 15) := by
  decide

/-- Octant coordinate along one axis at pyramid depth `d` (`div_euclid(2 ^ d)`). -/
def octantAxis (cx : Int) (d : Nat) : Int := cx / (2 ^ d)

theorem octantAxis_zero (cx : Int) :
    octantAxis cx 0 = cx := by
  simp [octantAxis]

theorem octantAxis_step (cx : Int) :
    octantAxis cx 1 = cx / 2 := by
  simp [octantAxis]

theorem octantAxis_parent_child (px : Int) (bit : Nat) (hbit : bit < 2) :
    octantAxis (px * 2 + Int.ofNat bit) 1 = px := by
  simp [octantAxis]
  omega

theorem octantAxis_depth1_mono (cx cy : Int) (h : cx ≤ cy) :
    octantAxis cx 1 ≤ octantAxis cy 1 := by
  simp [octantAxis]
  omega

/-- Flatten a 3D local chunk offset `(lx, ly, lz)` in `0..16` into `0..4096`. -/
def localIndex (lx ly lz : Nat) : Nat := lx + 16 * (ly + 16 * lz)

/-- Unflatten a linear chunk index `0..4096` into `(lx, ly, lz)`. -/
def unflattenIndex (idx : Nat) : Nat × Nat × Nat :=
  (idx % 16, (idx / 16) % 16, idx / 256)

theorem localIndex_lt (lx ly lz : Nat) (hx : lx < 16) (hy : ly < 16) (hz : lz < 16) :
    localIndex lx ly lz < 4096 := by
  simp [localIndex]
  omega

theorem unflattenIndex_localIndex (lx ly lz : Nat) (hx : lx < 16) (hy : ly < 16) :
    unflattenIndex (localIndex lx ly lz) = (lx, ly, lz) := by
  simp [unflattenIndex, localIndex]
  omega

theorem unflattenIndex_bounds (idx : Nat) (h : idx < 4096) :
    (unflattenIndex idx).1 < 16 ∧
      (unflattenIndex idx).2.1 < 16 ∧
      (unflattenIndex idx).2.2 < 16 := by
  simp [unflattenIndex]
  omega

theorem localIndex_unflattenIndex (idx : Nat) :
    localIndex (unflattenIndex idx).1 (unflattenIndex idx).2.1 (unflattenIndex idx).2.2 = idx := by
  simp [localIndex, unflattenIndex]
  omega

/-- Concrete 3D chunk coordinate `(cx, cy, cz)`. -/
abbrev ChunkCoord := Int × Int × Int

/-- Concrete coordinate decomposition into `(ChunkCoord, localIndex)` matching `world.rs::split_coord`. -/
def concreteSplitCoord (c : Coord) : ChunkCoord × Nat :=
  let (cx, lx) := splitAxis c.x
  let (cy, ly) := splitAxis c.y
  let (cz, lz) := splitAxis c.z
  ((cx, cy, cz), localIndex lx ly lz)

/-- Abstraction function mapping `(ChunkCoord, localIndex)` back to an abstract `Coord` (`world.rs::join_coord`). -/
def abstractChunkCoord (p : ChunkCoord × Nat) : Coord :=
  let (cx, cy, cz) := p.1
  let (lx, ly, lz) := unflattenIndex p.2
  { x := joinAxis cx lx, y := joinAxis cy ly, z := joinAxis cz lz }

theorem concreteSplitCoord_index_lt (c : Coord) :
    (concreteSplitCoord c).2 < 4096 := by
  dsimp [concreteSplitCoord]
  exact localIndex_lt (splitAxis c.x).2 (splitAxis c.y).2 (splitAxis c.z).2
    (splitAxis_local_lt c.x) (splitAxis_local_lt c.y) (splitAxis_local_lt c.z)

theorem abstractChunkCoord_concreteSplitCoord (c : Coord) :
    abstractChunkCoord (concreteSplitCoord c) = c := by
  cases c with
  | mk x y z =>
    dsimp [concreteSplitCoord, abstractChunkCoord]
    rw [unflattenIndex_localIndex (splitAxis x).2 (splitAxis y).2 (splitAxis z).2
      (splitAxis_local_lt x) (splitAxis_local_lt y)]
    dsimp
    rw [joinAxis_splitAxis x, joinAxis_splitAxis y, joinAxis_splitAxis z]

theorem concreteSplitCoord_abstractChunkCoord (chunk : ChunkCoord) (idx : Nat) (h : idx < 4096) :
    concreteSplitCoord (abstractChunkCoord (chunk, idx)) = (chunk, idx) := by
  rcases chunk with ⟨cx, cy, cz⟩
  have hb := unflattenIndex_bounds idx h
  dsimp [abstractChunkCoord, concreteSplitCoord]
  rw [splitAxis_joinAxis cx (unflattenIndex idx).1 hb.1]
  rw [splitAxis_joinAxis cy (unflattenIndex idx).2.1 hb.2.1]
  rw [splitAxis_joinAxis cz (unflattenIndex idx).2.2 hb.2.2]
  dsimp
  rw [localIndex_unflattenIndex idx]

/-- Negative control: truncated division/modulo (`tdiv`/`tmod`) on negative coordinates breaks chunk reconstruction. -/
theorem truncDivSplit_breaks_negative_coord :
    joinAxis ((-1 : Int).tdiv 16) ((-1 : Int).tmod 16).toNat ≠ -1 := by
  decide

end BitblockProof
