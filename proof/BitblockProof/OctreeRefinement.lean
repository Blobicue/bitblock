import BitblockProof.ChunkRefinement

namespace BitblockProof

/-!
Sparse chunk storage refinement, 8-level octree ancestor algebra (`defer_octree` /
`flush_octree` equivalence), AABB `Bounds` join-semilattice monotonicity, and
conservative octree occupancy/pruning soundness corresponding to
`crates/core/src/world.rs` (`World`, `Chunk`, `Bounds`, `merge_bounds`,
`update_octant`, `update_octree_ancestors`, `flush_octree`, `OCTREE_MAX_DEPTH = 8`).
-/

/-- Maximum octree pyramid depth matching `crates/core/src/world.rs::OCTREE_MAX_DEPTH`. -/
def octreeMaxDepth : Nat := 8

/-! ## Section 1: Sparse Chunk Map Refinement of `World.lean` -/

/-- One allocated `16 × 16 × 16` chunk entry storing occupied local indices in `0..4096`. -/
structure ChunkEntry where
  coord : ChunkCoord
  bits : List Nat
deriving DecidableEq, Repr

/-- Sparse chunk map storing only non-empty chunks with unique `ChunkCoord` keys. -/
abbrev ChunkWorld := List ChunkEntry

/-- Look up the occupied local indices of a chunk coordinate. -/
def lookupChunk : ChunkWorld → ChunkCoord → Option (List Nat)
  | [], _ => none
  | e :: rest, cc => if e.coord = cc then some e.bits else lookupChunk rest cc

/-- Structural invariant mirroring Rust's `HashMap<ChunkCoord, Chunk>` with empty-chunk eviction. -/
def UniqueChunks : ChunkWorld → Prop
  | [] => True
  | e :: rest => lookupChunk rest e.coord = none ∧ UniqueChunks rest

/-- Every stored chunk is non-empty and every local index lies in `0..4096`. -/
def WellFormedChunkWorld (cw : ChunkWorld) : Prop :=
  UniqueChunks cw ∧
    (∀ e ∈ cw, e.bits ≠ []) ∧
    (∀ e ∈ cw, ∀ idx ∈ e.bits, idx < 4096)

/-- Concrete bit membership test corresponding to `World::is_on`. -/
def chunkHasBit (cw : ChunkWorld) (c : Coord) : Prop :=
  match lookupChunk cw (concreteSplitCoord c).1 with
  | some bits => (concreteSplitCoord c).2 ∈ bits
  | none => False

instance (cw : ChunkWorld) (c : Coord) : Decidable (chunkHasBit cw c) :=
  match h : lookupChunk cw (concreteSplitCoord c).1 with
  | none => isFalse (by simp [chunkHasBit, h])
  | some bits =>
      if hm : (concreteSplitCoord c).2 ∈ bits then
        isTrue (by simp [chunkHasBit, h, hm])
      else
        isFalse (by simp [chunkHasBit, h, hm])

/-- All world coordinates stored inside one chunk entry. -/
def chunkToCoords (e : ChunkEntry) : List Coord :=
  e.bits.map (fun idx => abstractChunkCoord (e.coord, idx))

/-- Abstraction function mapping a sparse `ChunkWorld` to an abstract `World`. -/
def toAbstractWorld (cw : ChunkWorld) : World :=
  cw.flatMap chunkToCoords

/-- Lookup returns `some bits` iff there is a matching entry in a unique-key `ChunkWorld`. -/
theorem lookupChunk_eq_some_iff (cw : ChunkWorld) (hu : UniqueChunks cw)
    (cc : ChunkCoord) (bits : List Nat) :
    lookupChunk cw cc = some bits ↔ ChunkEntry.mk cc bits ∈ cw := by
  induction cw with
  | nil => simp [lookupChunk]
  | cons e rest ih =>
      rcases hu with ⟨hfresh, hrest⟩
      by_cases hcc : e.coord = cc
      · subst hcc
        cases e with
        | mk ec ebits =>
            simp only [lookupChunk, ↓reduceIte, Option.some.injEq, List.mem_cons,
              ChunkEntry.mk.injEq, true_and]
            constructor
            · intro heq
              exact Or.inl heq.symm
            · intro hmem
              rcases hmem with heq | hmemRest
              · exact heq.symm
              · have hsome := (ih hrest).mpr hmemRest
                rw [hfresh] at hsome
                cases hsome
      · simp only [lookupChunk, hcc, ↓reduceIte, List.mem_cons]
        rw [ih hrest]
        constructor
        · exact Or.inr
        · intro hmem
          rcases hmem with heq | hmemRest
          · cases heq
            exact False.elim (hcc rfl)
          · exact hmemRest

theorem lookupChunk_none_of_not_mem (cw : ChunkWorld) (cc : ChunkCoord) :
    lookupChunk cw cc = none ↔ ∀ e ∈ cw, e.coord ≠ cc := by
  induction cw with
  | nil => simp [lookupChunk]
  | cons e rest ih =>
      by_cases hcc : e.coord = cc
      · simp [lookupChunk, hcc]
      · simp [lookupChunk, hcc, ih]

/-- Lookup refinement: abstract `bit (toAbstractWorld cw) c` matches `chunkHasBit cw c`. -/
theorem toAbstractWorld_bit_iff (cw : ChunkWorld) (hwf : WellFormedChunkWorld cw) (c : Coord) :
    bit (toAbstractWorld cw) c ↔ chunkHasBit cw c := by
  rcases hwf with ⟨hu, _, hidx⟩
  unfold bit toAbstractWorld chunkHasBit
  rw [List.mem_flatMap]
  constructor
  · rintro ⟨e, heMem, hcMem⟩
    unfold chunkToCoords at hcMem
    rw [List.mem_map] at hcMem
    rcases hcMem with ⟨idx, hidxMem, hcoord⟩
    have hlt : idx < 4096 := hidx e heMem idx hidxMem
    have hsplit : concreteSplitCoord c = (e.coord, idx) := by
      rw [← hcoord]
      exact concreteSplitCoord_abstractChunkCoord e.coord idx hlt
    rw [hsplit]
    have hlook : lookupChunk cw e.coord = some e.bits :=
      (lookupChunk_eq_some_iff cw hu e.coord e.bits).mpr heMem
    simpa [hlook] using hidxMem
  · intro hhas
    cases hlook : lookupChunk cw (concreteSplitCoord c).1 with
    | none =>
        rw [hlook] at hhas
        exact False.elim hhas
    | some bits =>
        rw [hlook] at hhas
        have heMem : ChunkEntry.mk (concreteSplitCoord c).1 bits ∈ cw :=
          (lookupChunk_eq_some_iff cw hu (concreteSplitCoord c).1 bits).mp hlook
        refine ⟨ChunkEntry.mk (concreteSplitCoord c).1 bits, heMem, ?_⟩
        unfold chunkToCoords
        rw [List.mem_map]
        exact ⟨(concreteSplitCoord c).2, hhas, abstractChunkCoord_concreteSplitCoord c⟩

/-- Insert one local bit index into a sparse chunk map, allocating the chunk lazily. -/
def insertBit : ChunkWorld → ChunkCoord → Nat → ChunkWorld
  | [], cc, idx => [{ coord := cc, bits := [idx] }]
  | e :: rest, cc, idx =>
      if e.coord = cc then
        { coord := e.coord, bits := if idx ∈ e.bits then e.bits else idx :: e.bits } :: rest
      else
        e :: insertBit rest cc idx

/-- Remove one local bit index from a sparse chunk map, evicting the chunk if empty. -/
def removeBit : ChunkWorld → ChunkCoord → Nat → ChunkWorld
  | [], _, _ => []
  | e :: rest, cc, idx =>
      if e.coord = cc then
        let remaining := e.bits.filter (· != idx)
        if remaining.isEmpty then rest else { coord := e.coord, bits := remaining } :: rest
      else
        e :: removeBit rest cc idx

theorem lookupChunk_insertBit (cw : ChunkWorld) (cc q : ChunkCoord) (idx : Nat) :
    lookupChunk (insertBit cw cc idx) q =
      if cc = q then
        match lookupChunk cw q with
        | none => some [idx]
        | some bits => some (if idx ∈ bits then bits else idx :: bits)
      else
        lookupChunk cw q := by
  induction cw with
  | nil =>
      by_cases h : cc = q <;> simp [insertBit, lookupChunk, h]
  | cons e rest ih =>
      by_cases hec : e.coord = cc
      · subst hec
        by_cases hcq : e.coord = q
        · subst hcq
          simp [insertBit, lookupChunk]
        · simp [insertBit, lookupChunk, hcq]
      · by_cases heq : e.coord = q
        · subst heq
          have hne : cc ≠ e.coord := Ne.symm hec
          simp [insertBit, lookupChunk, hec, hne]
        · simp [insertBit, lookupChunk, hec, heq, ih]

theorem lookupChunk_removeBit (cw : ChunkWorld) (hu : UniqueChunks cw)
    (cc q : ChunkCoord) (idx : Nat) :
    lookupChunk (removeBit cw cc idx) q =
      if cc = q then
        match lookupChunk cw q with
        | none => none
        | some bits =>
            let remaining := bits.filter (· != idx)
            if remaining.isEmpty then none else some remaining
      else
        lookupChunk cw q := by
  induction cw with
  | nil =>
      by_cases h : cc = q <;> simp [removeBit, lookupChunk, h]
  | cons e rest ih =>
      rcases hu with ⟨hfresh, hrest⟩
      by_cases hec : e.coord = cc
      · subst hec
        by_cases hcq : e.coord = q
        · subst hcq
          simp only [removeBit, ↓reduceIte, lookupChunk]
          cases hemp : (e.bits.filter (· != idx)).isEmpty
          · simp [lookupChunk]
          · simp [hfresh]
        · simp only [removeBit, ↓reduceIte, lookupChunk, hcq]
          cases hemp : (e.bits.filter (· != idx)).isEmpty
          · simp [lookupChunk, hcq]
          · simp
      · by_cases heq : e.coord = q
        · subst heq
          have hne : cc ≠ e.coord := Ne.symm hec
          simp [removeBit, lookupChunk, hec, hne]
        · simp [removeBit, lookupChunk, hec, heq, ih hrest]

theorem insertBit_wellFormed (cw : ChunkWorld) (hwf : WellFormedChunkWorld cw)
    (cc : ChunkCoord) (idx : Nat) (hlt : idx < 4096) :
    WellFormedChunkWorld (insertBit cw cc idx) := by
  rcases hwf with ⟨hu, hne, hidx⟩
  induction cw with
  | nil =>
      refine ⟨⟨rfl, trivial⟩, ?_, ?_⟩
      · intro x hx
        simp [insertBit] at hx
        subst hx
        simp
      · intro x hx j hj
        simp [insertBit] at hx
        subst hx
        simp at hj
        omega
  | cons e rest ih =>
      rcases hu with ⟨hfresh, hrest⟩
      have hneSelf : e.bits ≠ [] := hne e (List.mem_cons_self ..)
      have hidxSelf : ∀ j ∈ e.bits, j < 4096 := hidx e (List.mem_cons_self ..)
      have hneRest : ∀ x ∈ rest, x.bits ≠ [] := fun x hx => hne x (List.mem_cons_of_mem e hx)
      have hidxRest : ∀ x ∈ rest, ∀ j ∈ x.bits, j < 4096 :=
        fun x hx => hidx x (List.mem_cons_of_mem e hx)
      by_cases hec : e.coord = cc
      · simp only [insertBit, hec, ↓reduceIte]
        refine ⟨⟨?_, hrest⟩, ?_, ?_⟩
        · simpa [hec] using hfresh
        · intro x hx
          simp only [List.mem_cons] at hx
          rcases hx with rfl | hxRest
          · by_cases hmem : idx ∈ e.bits
            · simpa [hmem] using hneSelf
            · simp [hmem]
          · exact hneRest x hxRest
        · intro x hx j hj
          simp only [List.mem_cons] at hx
          rcases hx with rfl | hxRest
          · by_cases hmem : idx ∈ e.bits
            · simp only [hmem, ↓reduceIte] at hj
              exact hidxSelf j hj
            · simp only [hmem, ↓reduceIte, List.mem_cons] at hj
              rcases hj with rfl | hjOld
              · exact hlt
              · exact hidxSelf j hjOld
          · exact hidxRest x hxRest j hj
      · have ihWf := ih hrest hneRest hidxRest
        rcases ihWf with ⟨hu', hne', hidx'⟩
        simp only [insertBit, hec, ↓reduceIte]
        refine ⟨⟨?_, hu'⟩, ?_, ?_⟩
        · rw [lookupChunk_insertBit]
          have hne' : cc ≠ e.coord := Ne.symm hec
          simp [hne', hfresh]
        · intro x hx
          simp only [List.mem_cons] at hx
          rcases hx with rfl | hxRest
          · exact hneSelf
          · exact hne' x hxRest
        · intro x hx j hj
          simp only [List.mem_cons] at hx
          rcases hx with rfl | hxRest
          · exact hidxSelf j hj
          · exact hidx' x hxRest j hj

theorem removeBit_wellFormed (cw : ChunkWorld) (hwf : WellFormedChunkWorld cw)
    (cc : ChunkCoord) (idx : Nat) :
    WellFormedChunkWorld (removeBit cw cc idx) := by
  rcases hwf with ⟨hu, hne, hidx⟩
  induction cw with
  | nil =>
      refine ⟨trivial, ?_, ?_⟩
      · intro x hx; cases hx
      · intro x hx; cases hx
  | cons e rest ih =>
      rcases hu with ⟨hfresh, hrest⟩
      have hneSelf : e.bits ≠ [] := hne e (List.mem_cons_self ..)
      have hidxSelf : ∀ j ∈ e.bits, j < 4096 := hidx e (List.mem_cons_self ..)
      have hneRest : ∀ x ∈ rest, x.bits ≠ [] := fun x hx => hne x (List.mem_cons_of_mem e hx)
      have hidxRest : ∀ x ∈ rest, ∀ j ∈ x.bits, j < 4096 :=
        fun x hx => hidx x (List.mem_cons_of_mem e hx)
      by_cases hec : e.coord = cc
      · simp only [removeBit, hec, ↓reduceIte]
        cases hemp : (e.bits.filter (· != idx)).isEmpty
        · simp only [Bool.false_eq_true, ↓reduceIte]
          refine ⟨⟨?_, hrest⟩, ?_, ?_⟩
          · simpa [hec] using hfresh
          · intro x hx
            simp only [List.mem_cons] at hx
            rcases hx with rfl | hxRest
            · intro hnil
              dsimp only at hnil
              rw [hnil] at hemp
              simp at hemp
            · exact hneRest x hxRest
          · intro x hx j hj
            simp only [List.mem_cons] at hx
            rcases hx with rfl | hxRest
            · simp only [List.mem_filter] at hj
              exact hidxSelf j hj.1
            · exact hidxRest x hxRest j hj
        · simp only [↓reduceIte]
          exact ⟨hrest, hneRest, hidxRest⟩
      · have ihWf := ih hrest hneRest hidxRest
        rcases ihWf with ⟨hu', hne', hidx'⟩
        simp only [removeBit, hec, ↓reduceIte]
        refine ⟨⟨?_, hu'⟩, ?_, ?_⟩
        · rw [lookupChunk_removeBit rest hrest]
          have hne' : cc ≠ e.coord := Ne.symm hec
          simp [hne', hfresh]
        · intro x hx
          simp only [List.mem_cons] at hx
          rcases hx with rfl | hxRest
          · exact hneSelf
          · exact hne' x hxRest
        · intro x hx j hj
          simp only [List.mem_cons] at hx
          rcases hx with rfl | hxRest
          · exact hidxSelf j hj
          · exact hidx' x hxRest j hj

/-- Initial sparse chunk world containing only immutable bedrock at `(0, 0, 0)`. -/
def initialChunkWorld : ChunkWorld :=
  [{ coord := (0, 0, 0), bits := [0] }]

theorem initialChunkWorld_wellFormed : WellFormedChunkWorld initialChunkWorld := by
  refine ⟨⟨rfl, trivial⟩, ?_, ?_⟩
  · intro e he
    simp [initialChunkWorld] at he
    subst he
    simp
  · intro e he idx hidx
    simp [initialChunkWorld] at he
    subst he
    simp at hidx
    omega

theorem initialChunkWorld_refines :
    SameWorld (toAbstractWorld initialChunkWorld) initialWorld := by
  intro c
  simp [toAbstractWorld, initialChunkWorld, chunkToCoords, abstractChunkCoord,
    unflattenIndex, joinAxis, initialWorld, origin, bit]

/-- Concrete `set_on` matching `World::set_on`. -/
def chunkSetOn (cw : ChunkWorld) (c : Coord) : ChunkWorld :=
  insertBit cw (concreteSplitCoord c).1 (concreteSplitCoord c).2

/-- Concrete `set_off` matching `World::set_off` (bedrock-preserving + empty-chunk eviction). -/
def chunkSetOff (cw : ChunkWorld) (c : Coord) : ChunkWorld :=
  if c = origin then cw
  else removeBit cw (concreteSplitCoord c).1 (concreteSplitCoord c).2

/-- Concrete instruction execution over sparse chunk storage. -/
def chunkApplyEdit (cw : ChunkWorld) : Edit → ChunkWorld
  | .on c => chunkSetOn cw c
  | .off c => chunkSetOff cw c
  | .toggle c =>
      if c = origin then cw
      else if chunkHasBit cw c then chunkSetOff cw c else chunkSetOn cw c

/-- Concrete ordered program replay over sparse chunk storage. -/
def chunkReplay : ChunkWorld → List Edit → ChunkWorld
  | cw, [] => cw
  | cw, edit :: rest => chunkReplay (chunkApplyEdit cw edit) rest

theorem chunkSetOn_wellFormed (cw : ChunkWorld) (hwf : WellFormedChunkWorld cw) (c : Coord) :
    WellFormedChunkWorld (chunkSetOn cw c) :=
  insertBit_wellFormed cw hwf (concreteSplitCoord c).1 (concreteSplitCoord c).2
    (concreteSplitCoord_index_lt c)

theorem chunkSetOff_wellFormed (cw : ChunkWorld) (hwf : WellFormedChunkWorld cw) (c : Coord) :
    WellFormedChunkWorld (chunkSetOff cw c) := by
  unfold chunkSetOff
  by_cases h : c = origin
  · simpa [h] using hwf
  · simpa [h] using removeBit_wellFormed cw hwf (concreteSplitCoord c).1 (concreteSplitCoord c).2

theorem chunkApplyEdit_wellFormed (cw : ChunkWorld) (hwf : WellFormedChunkWorld cw) (e : Edit) :
    WellFormedChunkWorld (chunkApplyEdit cw e) := by
  cases e with
  | on c => exact chunkSetOn_wellFormed cw hwf c
  | off c => exact chunkSetOff_wellFormed cw hwf c
  | toggle c =>
      unfold chunkApplyEdit
      by_cases ho : c = origin
      · simpa [ho] using hwf
      · by_cases hb : chunkHasBit cw c
        · simpa [ho, hb] using chunkSetOff_wellFormed cw hwf c
        · simpa [ho, hb] using chunkSetOn_wellFormed cw hwf c

theorem chunkReplay_wellFormed (cw : ChunkWorld) (hwf : WellFormedChunkWorld cw)
    (edits : List Edit) :
    WellFormedChunkWorld (chunkReplay cw edits) := by
  induction edits generalizing cw with
  | nil => exact hwf
  | cons edit rest ih =>
      exact ih (chunkApplyEdit cw edit) (chunkApplyEdit_wellFormed cw hwf edit)

/-- `chunkSetOn` refines `applyEdit _ (.on c)` for all `WellFormedChunkWorld`. -/
theorem chunkSetOn_refines (cw : ChunkWorld) (hwf : WellFormedChunkWorld cw) (c : Coord) :
    SameWorld (toAbstractWorld (chunkSetOn cw c)) (applyEdit (toAbstractWorld cw) (.on c)) := by
  intro q
  rw [toAbstractWorld_bit_iff _ (chunkSetOn_wellFormed cw hwf c) q]
  rw [bit_on, toAbstractWorld_bit_iff cw hwf q]
  unfold chunkHasBit chunkSetOn
  rw [lookupChunk_insertBit]
  by_cases hcc : (concreteSplitCoord c).1 = (concreteSplitCoord q).1
  · simp only [hcc, ↓reduceIte]
    cases hlook : lookupChunk cw (concreteSplitCoord q).1 with
    | none =>
        simp only [List.mem_singleton]
        constructor
        · intro hidx
          left
          have hpair : concreteSplitCoord q = concreteSplitCoord c :=
            Prod.ext hcc.symm hidx
          have := congrArg abstractChunkCoord hpair
          simpa [abstractChunkCoord_concreteSplitCoord] using this
        · rintro (rfl | hfalse)
          · rfl
          · exact False.elim hfalse
    | some bits =>
        by_cases hmem : (concreteSplitCoord c).2 ∈ bits
        · simp only [hmem, ↓reduceIte]
          constructor
          · exact Or.inr
          · rintro (rfl | hq)
            · exact hmem
            · exact hq
        · simp only [hmem, ↓reduceIte, List.mem_cons]
          constructor
          · rintro (hidx | hq)
            · left
              have hpair : concreteSplitCoord q = concreteSplitCoord c :=
                Prod.ext hcc.symm hidx
              have := congrArg abstractChunkCoord hpair
              simpa [abstractChunkCoord_concreteSplitCoord] using this
            · exact Or.inr hq
          · rintro (rfl | hq)
            · exact Or.inl rfl
            · exact Or.inr hq
  · simp only [hcc, ↓reduceIte]
    constructor
    · exact Or.inr
    · rintro (rfl | hq)
      · exact False.elim (hcc rfl)
      · exact hq

/-- `chunkSetOff` refines `applyEdit _ (.off c)` (including bedrock protection & chunk eviction). -/
theorem chunkSetOff_refines (cw : ChunkWorld) (hwf : WellFormedChunkWorld cw) (c : Coord) :
    SameWorld (toAbstractWorld (chunkSetOff cw c)) (applyEdit (toAbstractWorld cw) (.off c)) := by
  intro q
  by_cases ho : c = origin
  · subst ho
    simp [chunkSetOff, off_origin_noop]
  · rw [toAbstractWorld_bit_iff _ (chunkSetOff_wellFormed cw hwf c) q]
    rw [bit_off]
    simp only [ho, ↓reduceIte]
    rw [toAbstractWorld_bit_iff cw hwf q]
    unfold chunkHasBit chunkSetOff
    simp only [ho, ↓reduceIte]
    rw [lookupChunk_removeBit cw hwf.1]
    by_cases hcc : (concreteSplitCoord c).1 = (concreteSplitCoord q).1
    · simp only [hcc, ↓reduceIte]
      cases hlook : lookupChunk cw (concreteSplitCoord q).1 with
      | none => simp
      | some bits =>
          cases hemp : (bits.filter (· != (concreteSplitCoord c).2)).isEmpty
          · simp only [hemp, Bool.false_eq_true, ↓reduceIte, List.mem_filter, bne_iff_ne]
            constructor
            · rintro ⟨hq, hneIdx⟩
              refine ⟨?_, hq⟩
              intro heq
              subst heq
              exact hneIdx rfl
            · rintro ⟨hne, hq⟩
              refine ⟨hq, ?_⟩
              intro heqIdx
              apply hne
              have hpair : concreteSplitCoord q = concreteSplitCoord c :=
                Prod.ext hcc.symm heqIdx
              have := congrArg abstractChunkCoord hpair
              simpa [abstractChunkCoord_concreteSplitCoord] using this
          · simp only [hemp, ↓reduceIte, false_iff, not_and]
            intro hne hq
            have hmemFilt : (concreteSplitCoord q).2 ∈ bits.filter (· != (concreteSplitCoord c).2) := by
              rw [List.mem_filter]
              refine ⟨hq, ?_⟩
              simp only [bne_iff_ne]
              intro heqIdx
              apply hne
              have hpair : concreteSplitCoord q = concreteSplitCoord c :=
                Prod.ext hcc.symm heqIdx
              have := congrArg abstractChunkCoord hpair
              simpa [abstractChunkCoord_concreteSplitCoord] using this
            cases hfilt : bits.filter (· != (concreteSplitCoord c).2) with
            | nil =>
                rw [hfilt] at hmemFilt
                cases hmemFilt
            | cons _ _ =>
                rw [hfilt] at hemp
                simp at hemp
    · simp only [hcc, ↓reduceIte]
      constructor
      · intro hq
        refine ⟨?_, hq⟩
        intro heq
        subst heq
        exact hcc rfl
      · exact And.right

/-- Single-edit refinement: `chunkApplyEdit` simulates `World.lean::applyEdit`. -/
theorem chunkApplyEdit_refines (cw : ChunkWorld) (hwf : WellFormedChunkWorld cw) (e : Edit) :
    SameWorld (toAbstractWorld (chunkApplyEdit cw e)) (applyEdit (toAbstractWorld cw) e) := by
  cases e with
  | on c => exact chunkSetOn_refines cw hwf c
  | off c => exact chunkSetOff_refines cw hwf c
  | toggle c =>
      intro q
      by_cases ho : c = origin
      · subst ho
        simp [chunkApplyEdit, toggle_origin_noop]
      · by_cases hb : chunkHasBit cw c
        · have hab : bit (toAbstractWorld cw) c :=
            (toAbstractWorld_bit_iff cw hwf c).mpr hb
          have h1 : bit (toAbstractWorld (chunkApplyEdit cw (.toggle c))) q ↔
              bit (applyEdit (toAbstractWorld cw) (.off c)) q := by
            simpa [chunkApplyEdit, ho, hb] using chunkSetOff_refines cw hwf c q
          rw [h1, bit_off, bit_toggle]
          simp only [ho, ↓reduceIte]
          by_cases hq : q = c
          · subst hq
            simp [hab]
          · simp [hq]
        · have hnab : ¬bit (toAbstractWorld cw) c := fun h =>
            hb ((toAbstractWorld_bit_iff cw hwf c).mp h)
          have h1 : bit (toAbstractWorld (chunkApplyEdit cw (.toggle c))) q ↔
              bit (applyEdit (toAbstractWorld cw) (.on c)) q := by
            simpa [chunkApplyEdit, ho, hb] using chunkSetOn_refines cw hwf c q
          rw [h1, bit_on, bit_toggle]
          simp only [ho, ↓reduceIte]
          by_cases hq : q = c
          · subst hq
            simp [hnab]
          · simp [hq]

theorem sameWorld_applyEdit_congr (w₁ w₂ : World) (hs : SameWorld w₁ w₂) (e : Edit) :
    SameWorld (applyEdit w₁ e) (applyEdit w₂ e) := by
  intro q
  cases e with
  | on c => simp [bit_on, hs q]
  | off c => simp [bit_off, hs q]
  | toggle c => simp [bit_toggle, hs q]

theorem sameWorld_replay_congr (w₁ w₂ : World) (hs : SameWorld w₁ w₂) (edits : List Edit) :
    SameWorld (replay w₁ edits) (replay w₂ edits) := by
  induction edits generalizing w₁ w₂ with
  | nil => exact hs
  | cons edit rest ih =>
      exact ih _ _ (sameWorld_applyEdit_congr w₁ w₂ hs edit)

/-- Full trace simulation refinement: replaying any edit program on `ChunkWorld` matches `World.lean::replay`. -/
theorem chunkReplay_refines (cw : ChunkWorld) (hwf : WellFormedChunkWorld cw)
    (edits : List Edit) :
    SameWorld (toAbstractWorld (chunkReplay cw edits)) (replay (toAbstractWorld cw) edits) := by
  induction edits generalizing cw with
  | nil =>
      intro q
      rfl
  | cons edit rest ih =>
      have hstep := chunkApplyEdit_refines cw hwf edit
      have hwf' := chunkApplyEdit_wellFormed cw hwf edit
      have htail := ih (chunkApplyEdit cw edit) hwf'
      have hcongr := sameWorld_replay_congr _ _ hstep rest
      intro q
      exact (htail q).trans (hcongr q)

/-- Corollary: replaying any program from `initialChunkWorld` preserves bedrock and refines `initialWorld`. -/
theorem chunkReplay_initial_refines (edits : List Edit) :
    WellFormedChunkWorld (chunkReplay initialChunkWorld edits) ∧
      ValidWorld (toAbstractWorld (chunkReplay initialChunkWorld edits)) ∧
      SameWorld (toAbstractWorld (chunkReplay initialChunkWorld edits))
        (replay initialWorld edits) := by
  have hwf := chunkReplay_wellFormed initialChunkWorld initialChunkWorld_wellFormed edits
  have href := chunkReplay_refines initialChunkWorld initialChunkWorld_wellFormed edits
  have hinit := sameWorld_replay_congr _ _ initialChunkWorld_refines edits
  have hsame : SameWorld (toAbstractWorld (chunkReplay initialChunkWorld edits))
      (replay initialWorld edits) := fun q => (href q).trans (hinit q)
  have hvalid : ValidWorld (toAbstractWorld (chunkReplay initialChunkWorld edits)) :=
    (hsame origin).mpr (replay_initial_world_valid edits)
  exact ⟨hwf, hvalid, hsame⟩

/-! ## Section 2: Multi-Level Octree Ancestor Algebra (`defer_octree` / `flush_octree`) -/

private theorem two_pow_pos (d : Nat) : (0 : Int) < ((2 ^ d : Nat) : Int) := by
  exact_mod_cast Nat.two_pow_pos d

private theorem ediv_mul_two (a : Int) (m : Int) (hm : 0 < m) :
    (a / m) / 2 = a / (m * 2) := by
  have h2m : 0 < m * 2 := by omega
  have hdecomp : (m * 2) * (a / (m * 2)) + a % (m * 2) = a := Int.mul_ediv_add_emod a (m * 2)
  have hmul : (m * 2) * (a / (m * 2)) = m * (2 * (a / (m * 2))) := Int.mul_assoc m 2 _
  have hr0 : 0 ≤ a % (m * 2) := Int.emod_nonneg a (by omega)
  have hrlt : a % (m * 2) < m * 2 := Int.emod_lt_of_pos a h2m
  by_cases hlow : a % (m * 2) < m
  · have hstep : a / m = 2 * (a / (m * 2)) := by
      have ha : a = m * (2 * (a / (m * 2))) + a % (m * 2) := by omega
      calc
        a / m = (m * (2 * (a / (m * 2))) + a % (m * 2)) / m := congrArg (· / m) ha
        _ = 2 * (a / (m * 2)) + a % (m * 2) / m := Int.mul_add_ediv_left _ _ (by omega)
        _ = 2 * (a / (m * 2)) := by rw [Int.ediv_eq_zero_of_lt hr0 hlow]; omega
    rw [hstep]
    omega
  · have hstep : a / m = 2 * (a / (m * 2)) + 1 := by
      have hr'0 : 0 ≤ a % (m * 2) - m := by omega
      have hr'lt : a % (m * 2) - m < m := by omega
      have hdist : m * (2 * (a / (m * 2)) + 1) = m * (2 * (a / (m * 2))) + m := by
        rw [Int.mul_add, Int.mul_one]
      have ha : a = m * (2 * (a / (m * 2)) + 1) + (a % (m * 2) - m) := by omega
      calc
        a / m = (m * (2 * (a / (m * 2)) + 1) + (a % (m * 2) - m)) / m := congrArg (· / m) ha
        _ = (2 * (a / (m * 2)) + 1) + (a % (m * 2) - m) / m := Int.mul_add_ediv_left _ _ (by omega)
        _ = 2 * (a / (m * 2)) + 1 := by rw [Int.ediv_eq_zero_of_lt hr'0 hr'lt]; omega
    rw [hstep]
    omega

/--
Iterative division by `2` (used by `World::flush_octree`) yields the exact same
ancestor coordinate as direct division by `2^(d+1)` (used by `World::update_octree_ancestors`)
for every signed coordinate `cx` and every depth `d`.
-/
theorem octantAxis_step_eq_ediv2 (cx : Int) (d : Nat) :
    octantAxis (octantAxis cx d) 1 = octantAxis cx (d + 1) := by
  unfold octantAxis
  have hpow : ((2 ^ (d + 1) : Nat) : Int) = ((2 ^ d : Nat) : Int) * 2 := by
    rw [Nat.pow_succ]
    omega
  change (cx / ((2 ^ d : Nat) : Int)) / 2 = cx / ((2 ^ (d + 1) : Nat) : Int)
  rw [hpow]
  exact ediv_mul_two cx ((2 ^ d : Nat) : Int) (two_pow_pos d)

/-- 3D octant coordinate of a `ChunkCoord` at pyramid depth `d`. -/
def octantCoord (cc : ChunkCoord) (d : Nat) : ChunkCoord :=
  (octantAxis cc.1 d, octantAxis cc.2.1 d, octantAxis cc.2.2 d)

theorem octantCoord_zero (cc : ChunkCoord) : octantCoord cc 0 = cc := by
  rcases cc with ⟨cx, cy, cz⟩
  simp [octantCoord, octantAxis_zero]

theorem octantCoord_succ (cc : ChunkCoord) (d : Nat) :
    octantCoord cc (d + 1) = octantCoord (octantCoord cc d) 1 := by
  rcases cc with ⟨cx, cy, cz⟩
  simp only [octantCoord]
  exact Prod.ext (octantAxis_step_eq_ediv2 cx d).symm
    (Prod.ext (octantAxis_step_eq_ediv2 cy d).symm (octantAxis_step_eq_ediv2 cz d).symm)

/-- Iterated `div_euclid(2)` applied `d` times, matching `World::flush_octree`. -/
def iterParentCoord : Nat → ChunkCoord → ChunkCoord
  | 0, cc => cc
  | d + 1, cc => octantCoord (iterParentCoord d cc) 1

/-- `flush_octree`'s iterated parent mapping matches `update_octree_ancestors` at every depth `d`. -/
theorem iterParentCoord_eq_octantCoord (cc : ChunkCoord) (d : Nat) :
    iterParentCoord d cc = octantCoord cc d := by
  induction d with
  | zero => exact (octantCoord_zero cc).symm
  | succ d ih =>
      simp only [iterParentCoord, ih]
      exact (octantCoord_succ cc d).symm

/-- General depth-`d` monotonicity of `octantAxis`. -/
theorem octantAxis_mono (cx cy : Int) (d : Nat) (h : cx ≤ cy) :
    octantAxis cx d ≤ octantAxis cy d := by
  induction d with
  | zero => simpa [octantAxis_zero] using h
  | succ d ih =>
      rw [← octantAxis_step_eq_ediv2 cx d, ← octantAxis_step_eq_ediv2 cy d]
      exact octantAxis_depth1_mono _ _ ih

/-- Every chunk `cx` under depth-`d` octant `px` lies in `[px * 2^d, (px + 1) * 2^d)`. -/
theorem octantAxis_chunk_range (cx : Int) (d : Nat) :
    octantAxis cx d * ((2 ^ d : Nat) : Int) ≤ cx ∧
      cx < (octantAxis cx d + 1) * ((2 ^ d : Nat) : Int) := by
  have hm : (0 : Int) < ((2 ^ d : Nat) : Int) := two_pow_pos d
  have hdecomp : ((2 ^ d : Nat) : Int) * (cx / ((2 ^ d : Nat) : Int)) +
      cx % ((2 ^ d : Nat) : Int) = cx := Int.mul_ediv_add_emod cx ((2 ^ d : Nat) : Int)
  have hr0 : 0 ≤ cx % ((2 ^ d : Nat) : Int) := Int.emod_nonneg cx (by omega)
  have hrlt : cx % ((2 ^ d : Nat) : Int) < ((2 ^ d : Nat) : Int) := Int.emod_lt_of_pos cx hm
  unfold octantAxis
  change (cx / ((2 ^ d : Nat) : Int)) * ((2 ^ d : Nat) : Int) ≤ cx ∧
    cx < (cx / ((2 ^ d : Nat) : Int) + 1) * ((2 ^ d : Nat) : Int)
  have hcomm : (cx / ((2 ^ d : Nat) : Int)) * ((2 ^ d : Nat) : Int) =
      ((2 ^ d : Nat) : Int) * (cx / ((2 ^ d : Nat) : Int)) := Int.mul_comm _ _
  have hnext : (cx / ((2 ^ d : Nat) : Int) + 1) * ((2 ^ d : Nat) : Int) =
      ((2 ^ d : Nat) : Int) * (cx / ((2 ^ d : Nat) : Int)) + ((2 ^ d : Nat) : Int) := by
    rw [Int.add_mul, Int.one_mul, Int.mul_comm]
  omega

/-- Every world voxel `x` in a chunk under depth-`d` octant `px` lies in `[px * 16*2^d, (px+1) * 16*2^d)`. -/
theorem octantAxis_voxel_range (x : Int) (d : Nat) :
    octantAxis (splitAxis x).1 d * (16 * ((2 ^ d : Nat) : Int)) ≤ x ∧
      x < (octantAxis (splitAxis x).1 d + 1) * (16 * ((2 ^ d : Nat) : Int)) := by
  have hjoin := joinAxis_splitAxis x
  have hloc := splitAxis_local_lt x
  have hchunk := octantAxis_chunk_range (splitAxis x).1 d
  have hx : x = (splitAxis x).1 * 16 + ((splitAxis x).2 : Int) := by
    simpa [joinAxis, Int.ofNat_eq_natCast] using hjoin.symm
  have hmul1 :
      octantAxis (splitAxis x).1 d * (16 * ((2 ^ d : Nat) : Int)) =
        (octantAxis (splitAxis x).1 d * ((2 ^ d : Nat) : Int)) * 16 := by
    rw [Int.mul_comm 16 _, ← Int.mul_assoc]
  have hmul2 :
      (octantAxis (splitAxis x).1 d + 1) * (16 * ((2 ^ d : Nat) : Int)) =
        ((octantAxis (splitAxis x).1 d + 1) * ((2 ^ d : Nat) : Int)) * 16 := by
    rw [Int.mul_comm 16 _, ← Int.mul_assoc]
  rw [hmul1, hmul2]
  omega

/-- Child octant coordinate for `childIdx ∈ 0..8`, matching `world.rs:796-798`. -/
def octantChildCoord (p : ChunkCoord) (childIdx : Nat) : ChunkCoord :=
  ( p.1 * 2 + Int.ofNat ((childIdx / 4) % 2),
    p.2.1 * 2 + Int.ofNat ((childIdx / 2) % 2),
    p.2.2 * 2 + Int.ofNat (childIdx % 2) )

/-- Inverse child slot index `0..8` of a child coordinate inside its depth+1 parent. -/
def childSlotOf (c : ChunkCoord) : Nat :=
  4 * (c.1 % 2).toNat + 2 * (c.2.1 % 2).toNat + (c.2.2 % 2).toNat

theorem childSlotOf_lt (c : ChunkCoord) : childSlotOf c < 8 := by
  unfold childSlotOf
  omega

theorem octantChildCoord_parent (p : ChunkCoord) (childIdx : Nat) :
    octantCoord (octantChildCoord p childIdx) 1 = p := by
  rcases p with ⟨px, py, pz⟩
  simp only [octantCoord, octantChildCoord]
  have hx := octantAxis_parent_child px ((childIdx / 4) % 2) (by omega)
  have hy := octantAxis_parent_child py ((childIdx / 2) % 2) (by omega)
  have hz := octantAxis_parent_child pz (childIdx % 2) (by omega)
  simp only [hx, hy, hz]

theorem octantChildCoord_of_parent (c : ChunkCoord) :
    octantChildCoord (octantCoord c 1) (childSlotOf c) = c := by
  rcases c with ⟨cx, cy, cz⟩
  let bx := (cx % 2).toNat
  let by_ := (cy % 2).toNat
  let bz := (cz % 2).toNat
  have hbx : bx < 2 := by omega
  have hby : by_ < 2 := by omega
  have hbz : bz < 2 := by omega
  have hslot : childSlotOf (cx, cy, cz) = 4 * bx + 2 * by_ + bz := rfl
  have hslot_x : (childSlotOf (cx, cy, cz) / 4) % 2 = bx := by rw [hslot]; omega
  have hslot_y : (childSlotOf (cx, cy, cz) / 2) % 2 = by_ := by rw [hslot]; omega
  have hslot_z : childSlotOf (cx, cy, cz) % 2 = bz := by rw [hslot]; omega
  dsimp only [octantChildCoord, octantCoord, octantAxis]
  rw [hslot_x, hslot_y, hslot_z]
  refine Prod.ext ?_ (Prod.ext ?_ ?_)
  · change (cx / (2 ^ 1 : Int)) * 2 + Int.ofNat (cx % 2).toNat = cx
    rw [Int.ofNat_eq_natCast]; omega
  · change (cy / (2 ^ 1 : Int)) * 2 + Int.ofNat (cy % 2).toNat = cy
    rw [Int.ofNat_eq_natCast]; omega
  · change (cz / (2 ^ 1 : Int)) * 2 + Int.ofNat (cz % 2).toNat = cz
    rw [Int.ofNat_eq_natCast]; omega

/-- Negative control: truncated division (`tdiv`) collapses `-1` and `+1` into parent `0`, breaking partitioning. -/
theorem truncDiv_breaks_octree_partitioning :
    (-1 : Int).tdiv 2 = (1 : Int).tdiv 2 ∧ octantAxis (-1) 1 ≠ octantAxis 1 1 := by
  decide

/-! ## Section 3: AABB `Bounds` Join-Semilattice & Monotonicity -/

/-- Inclusive integer AABB matching `crates/core/src/world.rs::Bounds`. -/
structure Bounds where
  min : Coord
  max : Coord
deriving DecidableEq, Repr

/-- Exact single-voxel AABB. -/
def Bounds.singleton (c : Coord) : Bounds := { min := c, max := c }

/-- Inclusive AABB point containment. -/
def Bounds.contains (b : Bounds) (c : Coord) : Prop :=
  b.min.x ≤ c.x ∧ c.x ≤ b.max.x ∧
    b.min.y ≤ c.y ∧ c.y ≤ b.max.y ∧
    b.min.z ≤ c.z ∧ c.z ≤ b.max.z

/-- AABB subsumption (`inner ⊆ outer`). -/
def Bounds.subsumes (outer inner : Bounds) : Prop :=
  outer.min.x ≤ inner.min.x ∧ inner.max.x ≤ outer.max.x ∧
    outer.min.y ≤ inner.min.y ∧ inner.max.y ≤ outer.max.y ∧
    outer.min.z ≤ inner.min.z ∧ inner.max.z ≤ outer.max.z

/-- Componentwise AABB union matching `crates/core/src/world.rs::merge_bounds`. -/
def mergeBounds (a b : Bounds) : Bounds :=
  { min := { x := min a.min.x b.min.x, y := min a.min.y b.min.y, z := min a.min.z b.min.z },
    max := { x := max a.max.x b.max.x, y := max a.max.y b.max.y, z := max a.max.z b.max.z } }

theorem Bounds.contains_singleton (c : Coord) : (Bounds.singleton c).contains c := by
  simp [Bounds.singleton, Bounds.contains]

theorem Bounds.subsumes_refl (b : Bounds) : b.subsumes b := by
  simp [Bounds.subsumes]

theorem Bounds.subsumes_trans {a b c : Bounds} (hab : a.subsumes b) (hbc : b.subsumes c) :
    a.subsumes c := by
  rcases hab with ⟨hx1, hx2, hy1, hy2, hz1, hz2⟩
  rcases hbc with ⟨gx1, gx2, gy1, gy2, gz1, gz2⟩
  exact ⟨Int.le_trans hx1 gx1, Int.le_trans gx2 hx2,
         Int.le_trans hy1 gy1, Int.le_trans gy2 hy2,
         Int.le_trans hz1 gz1, Int.le_trans gz2 hz2⟩

theorem Bounds.contains_iff_subsumes_singleton (b : Bounds) (c : Coord) :
    b.contains c ↔ b.subsumes (Bounds.singleton c) := by
  rfl

theorem Bounds.contains_of_subsumes {outer inner : Bounds} {c : Coord}
    (hsub : outer.subsumes inner) (hcon : inner.contains c) :
    outer.contains c :=
  (Bounds.contains_iff_subsumes_singleton outer c).mpr
    (Bounds.subsumes_trans hsub ((Bounds.contains_iff_subsumes_singleton inner c).mp hcon))

theorem Bounds.subsumes_merge_left (a b : Bounds) : (mergeBounds a b).subsumes a := by
  simp [mergeBounds, Bounds.subsumes]
  omega

theorem Bounds.subsumes_merge_right (a b : Bounds) : (mergeBounds a b).subsumes b := by
  simp [mergeBounds, Bounds.subsumes]
  omega

theorem Bounds.merge_least_upper_bound {outer a b : Bounds}
    (ha : outer.subsumes a) (hb : outer.subsumes b) :
    outer.subsumes (mergeBounds a b) := by
  rcases ha with ⟨ax1, ax2, ay1, ay2, az1, az2⟩
  rcases hb with ⟨bx1, bx2, by1, by2, bz1, bz2⟩
  simp [mergeBounds, Bounds.subsumes]
  omega

theorem mergeBounds_comm (a b : Bounds) : mergeBounds a b = mergeBounds b a := by
  rcases a with ⟨⟨ax1, ay1, az1⟩, ⟨ax2, ay2, az2⟩⟩
  rcases b with ⟨⟨bx1, by1, bz1⟩, ⟨bx2, by2, bz2⟩⟩
  simp only [mergeBounds, Bounds.mk.injEq, Coord.mk.injEq]
  omega

theorem mergeBounds_assoc (a b c : Bounds) :
    mergeBounds (mergeBounds a b) c = mergeBounds a (mergeBounds b c) := by
  rcases a with ⟨⟨ax1, ay1, az1⟩, ⟨ax2, ay2, az2⟩⟩
  rcases b with ⟨⟨bx1, by1, bz1⟩, ⟨bx2, by2, bz2⟩⟩
  rcases c with ⟨⟨cx1, cy1, cz1⟩, ⟨cx2, cy2, cz2⟩⟩
  simp only [mergeBounds, Bounds.mk.injEq, Coord.mk.injEq]
  omega

theorem mergeBounds_idem (a : Bounds) : mergeBounds a a = a := by
  rcases a with ⟨⟨ax1, ay1, az1⟩, ⟨ax2, ay2, az2⟩⟩
  simp only [mergeBounds, Bounds.mk.injEq, Coord.mk.injEq]
  omega

/-! ## Section 4: 8-Level Octree Summaries & Conservative Pruning Soundness -/

/-- Exact octant summary matching `crates/core/src/world.rs::OctantSummary` (sans revision stamp). -/
structure OctantSummary where
  occupiedBits : Nat
  childMask : Nat
  bounds : Bounds
deriving DecidableEq, Repr

/-- Merge a list of coordinates into their tight inclusive AABB. -/
def coordsBounds : List Coord → Option Bounds
  | [] => none
  | c :: rest =>
      match coordsBounds rest with
      | none => some (Bounds.singleton c)
      | some b => some (mergeBounds (Bounds.singleton c) b)

theorem coordsBounds_some_iff (cs : List Coord) :
    (∃ b, coordsBounds cs = some b) ↔ cs ≠ [] := by
  induction cs with
  | nil => simp [coordsBounds]
  | cons c rest _ =>
      cases h : coordsBounds rest <;> simp [coordsBounds, h]

theorem coordsBounds_contains_mem (cs : List Coord) (b : Bounds)
    (hb : coordsBounds cs = some b) (c : Coord) (hmem : c ∈ cs) :
    b.contains c := by
  induction cs generalizing b with
  | nil => cases hmem
  | cons x rest ih =>
      simp only [List.mem_cons] at hmem
      cases hrest : coordsBounds rest with
      | none =>
          simp only [coordsBounds, hrest, Option.some.injEq] at hb
          subst hb
          rcases hmem with hEq | hrestMem
          · subst hEq
            exact Bounds.contains_singleton c
          · have hnil : rest = [] := by
              cases rest with
              | nil => rfl
              | cons y ys =>
                  have ⟨b', hb'⟩ := (coordsBounds_some_iff (y :: ys)).mpr (by simp)
                  rw [hrest] at hb'
                  cases hb'
            subst hnil
            cases hrestMem
      | some br =>
          simp only [coordsBounds, hrest, Option.some.injEq] at hb
          subst hb
          rcases hmem with hEq | hrestMem
          · subst hEq
            exact Bounds.contains_of_subsumes (Bounds.subsumes_merge_left _ _)
              (Bounds.contains_singleton c)
          · exact Bounds.contains_of_subsumes (Bounds.subsumes_merge_right _ _)
              (ih br hrest hrestMem)

/-- Fold over child slots `0..8`, matching `World::update_octant` (`world.rs:795-838`). -/
def combineChildSlots (f : Nat → Option OctantSummary) : List Nat → Option OctantSummary
  | [] => none
  | idx :: rest =>
      match f idx, combineChildSlots f rest with
      | none, acc => acc
      | some cs, none =>
          some { occupiedBits := cs.occupiedBits, childMask := 2 ^ idx, bounds := cs.bounds }
      | some cs, some acc =>
          some {
            occupiedBits := cs.occupiedBits + acc.occupiedBits,
            childMask := 2 ^ idx + acc.childMask,
            bounds := mergeBounds cs.bounds acc.bounds
          }

/-- Canonical 8-child slot order `0..8`. -/
def allChildSlots : List Nat := [0, 1, 2, 3, 4, 5, 6, 7]

theorem mem_allChildSlots_iff (idx : Nat) : idx ∈ allChildSlots ↔ idx < 8 := by
  simp [allChildSlots]
  omega

/-- Bottom-up octree summary at pyramid depth `d` for octant `p` (`d = 0` is chunk level). -/
def octreeSummary (cw : ChunkWorld) : Nat → ChunkCoord → Option OctantSummary
  | 0, cc =>
      match lookupChunk cw cc with
      | none => none
      | some bits =>
          match coordsBounds (bits.map (fun idx => abstractChunkCoord (cc, idx))) with
          | none => none
          | some b => some { occupiedBits := bits.length, childMask := 0, bounds := b }
  | d + 1, p =>
      combineChildSlots (fun idx => octreeSummary cw d (octantChildCoord p idx)) allChildSlots

theorem combineChildSlots_some_iff (f : Nat → Option OctantSummary) (slots : List Nat) :
    (∃ s, combineChildSlots f slots = some s) ↔ (∃ idx ∈ slots, ∃ cs, f idx = some cs) := by
  induction slots with
  | nil => simp [combineChildSlots]
  | cons idx rest ih =>
      cases hfi : f idx with
      | none =>
          simp only [combineChildSlots, hfi, List.mem_cons]
          rw [ih]
          constructor
          · rintro ⟨j, hj, cs, hfj⟩
            exact ⟨j, Or.inr hj, cs, hfj⟩
          · rintro ⟨j, rfl | hj, cs, hfj⟩
            · rw [hfi] at hfj; cases hfj
            · exact ⟨j, hj, cs, hfj⟩
      | some cs =>
          cases hacc : combineChildSlots f rest <;>
            simp [combineChildSlots, hfi, hacc] <;>
            exact ⟨idx, Or.inl rfl, cs, hfi⟩

theorem combineChildSlots_subsumes (f : Nat → Option OctantSummary) (slots : List Nat)
    (s : OctantSummary) (hs : combineChildSlots f slots = some s)
    (idx : Nat) (hmem : idx ∈ slots) (cs : OctantSummary) (hfi : f idx = some cs) :
    s.bounds.subsumes cs.bounds := by
  induction slots generalizing s with
  | nil => cases hmem
  | cons k rest ih =>
      simp only [List.mem_cons] at hmem
      cases hfk : f k with
      | none =>
          simp only [combineChildSlots, hfk] at hs
          rcases hmem with rfl | hrestMem
          · rw [hfk] at hfi; cases hfi
          · exact ih s hs hrestMem
      | some csk =>
          cases hacc : combineChildSlots f rest with
          | none =>
              simp only [combineChildSlots, hfk, hacc, Option.some.injEq] at hs
              subst hs
              rcases hmem with rfl | hrestMem
              · rw [hfk] at hfi
                cases hfi
                exact Bounds.subsumes_refl _
              · have ⟨s', hs'⟩ := (combineChildSlots_some_iff f rest).mpr ⟨idx, hrestMem, cs, hfi⟩
                rw [hacc] at hs'
                cases hs'
          | some acc =>
              simp only [combineChildSlots, hfk, hacc, Option.some.injEq] at hs
              subst hs
              rcases hmem with rfl | hrestMem
              · rw [hfk] at hfi
                cases hfi
                exact Bounds.subsumes_merge_left _ _
              · exact Bounds.subsumes_trans (Bounds.subsumes_merge_right _ _)
                  (ih acc hacc hrestMem)

theorem combineChildSlots_occupied_pos (f : Nat → Option OctantSummary) (slots : List Nat)
    (hpos : ∀ idx ∈ slots, ∀ cs, f idx = some cs → 0 < cs.occupiedBits)
    (s : OctantSummary) (hs : combineChildSlots f slots = some s) :
    0 < s.occupiedBits ∧ 0 < s.childMask := by
  induction slots generalizing s with
  | nil => simp [combineChildSlots] at hs
  | cons k rest ih =>
      have hposRest : ∀ idx ∈ rest, ∀ cs, f idx = some cs → 0 < cs.occupiedBits :=
        fun idx hidx => hpos idx (List.mem_cons_of_mem k hidx)
      cases hfk : f k with
      | none =>
          simp only [combineChildSlots, hfk] at hs
          exact ih hposRest s hs
      | some csk =>
          have hck : 0 < csk.occupiedBits := hpos k (List.mem_cons_self ..) csk hfk
          have hpow : 0 < 2 ^ k := Nat.two_pow_pos k
          cases hacc : combineChildSlots f rest with
          | none =>
              simp only [combineChildSlots, hfk, hacc, Option.some.injEq] at hs
              subst hs
              exact ⟨hck, hpow⟩
          | some acc =>
              simp only [combineChildSlots, hfk, hacc, Option.some.injEq] at hs
              subst hs
              have ⟨ha1, ha2⟩ := ih hposRest acc hacc
              dsimp only
              exact ⟨Nat.add_pos_left hck _, Nat.add_pos_left hpow _⟩

theorem combineChildSlots_mask_lt (f : Nat → Option OctantSummary)
    (s : OctantSummary) (hs : combineChildSlots f allChildSlots = some s) :
    s.childMask < 256 := by
  suffices h : ∀ (slots : List Nat) (s : OctantSummary),
      combineChildSlots f slots = some s →
      s.childMask ≤ (slots.map (fun k => 2 ^ k)).sum by
    have hsum : (allChildSlots.map (fun k => 2 ^ k)).sum = 255 := by decide
    have hle := h allChildSlots s hs
    omega
  intro slots
  induction slots with
  | nil => intro s hs; simp [combineChildSlots] at hs
  | cons k rest ih =>
      intro s hs
      cases hfk : f k with
      | none =>
          simp only [combineChildSlots, hfk] at hs
          have hle := ih s hs
          simp only [List.map_cons, List.sum_cons]
          exact Nat.le_trans hle (Nat.le_add_left _ _)
      | some csk =>
          cases hacc : combineChildSlots f rest with
          | none =>
              simp only [combineChildSlots, hfk, hacc, Option.some.injEq] at hs
              subst hs
              simp only [List.map_cons, List.sum_cons]
              exact Nat.le_add_right _ _
          | some acc =>
              simp only [combineChildSlots, hfk, hacc, Option.some.injEq] at hs
              subst hs
              have hle := ih acc hacc
              simp only [List.map_cons, List.sum_cons]
              exact Nat.add_le_add_left hle _

/--
Zero false-empty pruning: an octant summary exists at depth `d` iff there is at least
one occupied voxel in `cw` whose depth-`d` octant is `p`.
-/
theorem octreeSummary_some_iff_exists_bit (cw : ChunkWorld) (hwf : WellFormedChunkWorld cw)
    (d : Nat) (p : ChunkCoord) :
    (∃ s, octreeSummary cw d p = some s) ↔
      (∃ c : Coord, bit (toAbstractWorld cw) c ∧ octantCoord (concreteSplitCoord c).1 d = p) := by
  induction d generalizing p with
  | zero =>
      constructor
      · rintro ⟨s, hs⟩
        unfold octreeSummary at hs
        cases hlook : lookupChunk cw p with
        | none => simp [hlook] at hs
        | some bits =>
            simp only [hlook] at hs
            cases hcb : coordsBounds (bits.map (fun idx => abstractChunkCoord (p, idx))) with
            | none => simp [hcb] at hs
            | some b =>
                have hne : bits.map (fun idx => abstractChunkCoord (p, idx)) ≠ [] :=
                  (coordsBounds_some_iff _).mp ⟨b, hcb⟩
                cases hbits : bits with
                | nil => simp [hbits] at hne
                | cons idx rest =>
                    have heMem : ChunkEntry.mk p (idx :: rest) ∈ cw :=
                      (lookupChunk_eq_some_iff cw hwf.1 p (idx :: rest)).mp (hbits ▸ hlook)
                    have hlt : idx < 4096 := hwf.2.2 _ heMem idx (List.mem_cons_self ..)
                    refine ⟨abstractChunkCoord (p, idx), ?_, ?_⟩
                    · rw [toAbstractWorld_bit_iff cw hwf]
                      unfold chunkHasBit
                      rw [concreteSplitCoord_abstractChunkCoord p idx hlt, hlook, hbits]
                      exact List.mem_cons_self ..
                    · rw [concreteSplitCoord_abstractChunkCoord p idx hlt]
                      exact octantCoord_zero p
      · rintro ⟨c, hbit, hoct⟩
        have hhas := (toAbstractWorld_bit_iff cw hwf c).mp hbit
        unfold chunkHasBit at hhas
        have hp : (concreteSplitCoord c).1 = p := by
          rw [octantCoord_zero] at hoct
          exact hoct
        rw [hp] at hhas
        cases hlook : lookupChunk cw p with
        | none => rw [hlook] at hhas; exact False.elim hhas
        | some bits =>
            rw [hlook] at hhas
            have hne : bits.map (fun idx => abstractChunkCoord (p, idx)) ≠ [] := by
              intro hnil
              cases bits with
              | nil => cases hhas
              | cons _ _ => simp at hnil
            obtain ⟨b, hb⟩ := (coordsBounds_some_iff _).mpr hne
            exact ⟨{ occupiedBits := bits.length, childMask := 0, bounds := b }, by
              simp [octreeSummary, hlook, hb]⟩
  | succ d ih =>
      unfold octreeSummary
      rw [combineChildSlots_some_iff]
      constructor
      · rintro ⟨slot, _, sChild, hChild⟩
        obtain ⟨c, hbit, hoct⟩ := (ih (octantChildCoord p slot)).mp ⟨sChild, hChild⟩
        refine ⟨c, hbit, ?_⟩
        rw [octantCoord_succ, hoct, octantChildCoord_parent]
      · rintro ⟨c, hbit, hoct⟩
        let child := octantCoord (concreteSplitCoord c).1 d
        have hparent : octantCoord child 1 = p := by
          change octantCoord (octantCoord (concreteSplitCoord c).1 d) 1 = p
          rw [← octantCoord_succ, hoct]
        have hchildEq : octantChildCoord p (childSlotOf child) = child := by
          rw [← hparent]
          exact octantChildCoord_of_parent child
        have hslotMem : childSlotOf child ∈ allChildSlots :=
          (mem_allChildSlots_iff _).mpr (childSlotOf_lt child)
        obtain ⟨sChild, hsChild⟩ := (ih child).mpr ⟨c, hbit, rfl⟩
        exact ⟨childSlotOf child, hslotMem, sChild, by simpa [hchildEq] using hsChild⟩

/-- Every active octree summary has strictly positive `occupiedBits` (and valid non-zero 8-bit `childMask` for `d ≥ 1`). -/
theorem octreeSummary_occupiedBits_pos (cw : ChunkWorld) (hwf : WellFormedChunkWorld cw)
    (d : Nat) (p : ChunkCoord) (s : OctantSummary) (hs : octreeSummary cw d p = some s) :
    0 < s.occupiedBits ∧ (1 ≤ d → 0 < s.childMask ∧ s.childMask < 256) := by
  induction d generalizing p s with
  | zero =>
      refine ⟨?_, fun h => by omega⟩
      unfold octreeSummary at hs
      cases hlook : lookupChunk cw p with
      | none => simp [hlook] at hs
      | some bits =>
          simp only [hlook] at hs
          cases hcb : coordsBounds (bits.map (fun idx => abstractChunkCoord (p, idx))) with
          | none => simp [hcb] at hs
          | some b =>
              simp only [hcb, Option.some.injEq] at hs
              subst hs
              have heMem := (lookupChunk_eq_some_iff cw hwf.1 p bits).mp hlook
              have hne := hwf.2.1 _ heMem
              cases bits with
              | nil => exact False.elim (hne rfl)
              | cons _ _ => simp
  | succ d ih =>
      unfold octreeSummary at hs
      have hposChildren : ∀ idx ∈ allChildSlots, ∀ cs,
          octreeSummary cw d (octantChildCoord p idx) = some cs → 0 < cs.occupiedBits :=
        fun idx _ cs hcs => (ih (octantChildCoord p idx) cs hcs).1
      have ⟨hocc, hmaskPos⟩ := combineChildSlots_occupied_pos _ allChildSlots hposChildren s hs
      have hmaskLt := combineChildSlots_mask_lt _ s hs
      exact ⟨hocc, fun _ => ⟨hmaskPos, hmaskLt⟩⟩

/--
Conservative AABB enclosure: if an octant summary `s` exists at depth `d` for `p`,
then `s.bounds` contains every occupied coordinate `c` in `cw` beneath octant `p`.
-/
theorem octreeSummary_bounds_contain_descendant (cw : ChunkWorld) (hwf : WellFormedChunkWorld cw)
    (d : Nat) (p : ChunkCoord) (s : OctantSummary) (hs : octreeSummary cw d p = some s)
    (c : Coord) (hbit : bit (toAbstractWorld cw) c)
    (hoct : octantCoord (concreteSplitCoord c).1 d = p) :
    s.bounds.contains c := by
  induction d generalizing p s with
  | zero =>
      have hhas := (toAbstractWorld_bit_iff cw hwf c).mp hbit
      unfold chunkHasBit at hhas
      have hp : (concreteSplitCoord c).1 = p := by
        rw [octantCoord_zero] at hoct
        exact hoct
      rw [hp] at hhas
      unfold octreeSummary at hs
      cases hlook : lookupChunk cw p with
      | none => simp [hlook] at hs
      | some bits =>
          rw [hlook] at hhas
          simp only [hlook] at hs
          cases hcb : coordsBounds (bits.map (fun idx => abstractChunkCoord (p, idx))) with
          | none => simp [hcb] at hs
          | some b =>
              simp only [hcb, Option.some.injEq] at hs
              subst hs
              have hmemMap : c ∈ bits.map (fun idx => abstractChunkCoord (p, idx)) := by
                rw [List.mem_map]
                refine ⟨(concreteSplitCoord c).2, hhas, ?_⟩
                rw [← hp]
                exact abstractChunkCoord_concreteSplitCoord c
              exact coordsBounds_contains_mem _ b hcb c hmemMap
  | succ d ih =>
      let child := octantCoord (concreteSplitCoord c).1 d
      have hparent : octantCoord child 1 = p := by
        change octantCoord (octantCoord (concreteSplitCoord c).1 d) 1 = p
        rw [← octantCoord_succ, hoct]
      have hchildEq : octantChildCoord p (childSlotOf child) = child := by
        rw [← hparent]
        exact octantChildCoord_of_parent child
      have hslotMem : childSlotOf child ∈ allChildSlots :=
        (mem_allChildSlots_iff _).mpr (childSlotOf_lt child)
      obtain ⟨sChild, hsChild⟩ :=
        (octreeSummary_some_iff_exists_bit cw hwf d child).mpr ⟨c, hbit, rfl⟩
      have hchildContains : sChild.bounds.contains c :=
        ih child sChild hsChild rfl
      unfold octreeSummary at hs
      have hsub : s.bounds.subsumes sChild.bounds :=
        combineChildSlots_subsumes _ allChildSlots s hs (childSlotOf child) hslotMem sChild
          (by simpa [hchildEq] using hsChild)
      exact Bounds.contains_of_subsumes hsub hchildContains

/--
Conservative hierarchical pruning soundness (frustum culling, ray-AABB rejection,
and near-to-far sweep cutoff): for ANY downward-monotone predicate `Outside : Bounds → Prop`,
if `Outside s.bounds` holds for a depth-`d` octant summary `s`, then `Outside (Bounds.singleton c)`
holds for every occupied coordinate `c` in `cw` beneath that octant.
-/
theorem octreeSummary_pruning_soundness (cw : ChunkWorld) (hwf : WellFormedChunkWorld cw)
    (Outside : Bounds → Prop)
    (hmono : ∀ outer inner, outer.subsumes inner → Outside outer → Outside inner)
    (d : Nat) (p : ChunkCoord) (s : OctantSummary) (hs : octreeSummary cw d p = some s)
    (hout : Outside s.bounds)
    (c : Coord) (hbit : bit (toAbstractWorld cw) c)
    (hoct : octantCoord (concreteSplitCoord c).1 d = p) :
    Outside (Bounds.singleton c) := by
  have hcon := octreeSummary_bounds_contain_descendant cw hwf d p s hs c hbit hoct
  exact hmono s.bounds (Bounds.singleton c) hcon hout

/--
Macro-octant root coverage at `OCTREE_MAX_DEPTH = 8`: every occupied voxel `c` in any
well-formed `ChunkWorld` belongs to an active depth-8 root octant whose summary bounds
contain `c` and whose `occupiedBits > 0` and `0 < childMask < 256`.
-/
theorem octree_root_covers_every_occupied_voxel (cw : ChunkWorld) (hwf : WellFormedChunkWorld cw)
    (c : Coord) (hbit : bit (toAbstractWorld cw) c) :
    ∃ (p : ChunkCoord) (s : OctantSummary),
      p = octantCoord (concreteSplitCoord c).1 octreeMaxDepth ∧
      octreeSummary cw octreeMaxDepth p = some s ∧
      0 < s.occupiedBits ∧
      0 < s.childMask ∧ s.childMask < 256 ∧
      s.bounds.contains c := by
  let p := octantCoord (concreteSplitCoord c).1 octreeMaxDepth
  obtain ⟨s, hs⟩ :=
    (octreeSummary_some_iff_exists_bit cw hwf octreeMaxDepth p).mpr ⟨c, hbit, rfl⟩
  have ⟨hocc, hmask⟩ := octreeSummary_occupiedBits_pos cw hwf octreeMaxDepth p s hs
  have ⟨hmaskPos, hmaskLt⟩ := hmask (by decide)
  have hcon := octreeSummary_bounds_contain_descendant cw hwf octreeMaxDepth p s hs c hbit rfl
  exact ⟨p, s, rfl, hs, hocc, hmaskPos, hmaskLt, hcon⟩

end BitblockProof
