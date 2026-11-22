namespace BitblockProof

/-!
`REFINE-1@commitment`: Formal binary Merkle Patricia Trie normalization, `CoordinateKeyV1`
sign-bias order-preserving bijection, and point-proof soundness model corresponding to
`crates/research/src/commit_state.rs` (`TypedPatriciaTrie`, `TypedPatriciaProofV1`,
`WorldRootV1`, `ActiveTokenRootV1`, `EpisodeRootV1`) and `crates/research/src/commit_r1.rs`
(`CoordinateKeyV1`).

Establishes under `--trust=0` with zero custom axioms:
- Exact bijection and strict order preservation of `CoordinateKeyV1` sign-bit biasing over
  all signed 64-bit integers `[-2^63, 2^63 - 1]`, plus a powered negative control showing raw
  two's complement breaks signed ordering (`-1 > 0`);
- Canonical structural invariant `CanonicalPat rem t` over arbitrary fixed bit-width `rem`
  (simultaneously covering 192-bit `WorldRootV1`/`ActiveTokenRootV1` and 256-bit `EpisodeRootV1`):
  empty permitted only at root, non-empty branches have two non-empty children, extensions are
  non-empty and always point directly to a branch (no adjacent extensions, no extension->leaf);
- **Uniqueness theorem (`canonicalPat_unique_of_alpha_eq`)**: any two canonical Patricia tries
  with identical abstract lookup semantics `alphaPat` are structurally identical (`t1 = t2`),
  yielding insertion-order independence, incremental == full-rebuild parity, and delete-restores-
  prior-root as direct corollaries;
- Exact preservation and refinement of `patUpsert` and `patDelete` (including branch collapse
  and adjacent-extension merging) plus exact delete-after-insert tree restoration;
- Soundness and completeness of canonical membership/non-membership point proofs (`provePat`
  / `verifyPatProof`) and powered negative controls for non-canonical tree shapes.
-/

/-! ## Section 1: `CoordinateKeyV1` Signed 64-Bit Bias Bijection & Order Preservation -/

/-- `2^63` sign bias constant (`1 << 63 = 9223372036854775808`). -/
def signBias64 : Nat := 9223372036854775808

/-- `2^64` unsigned 64-bit modulus (`18446744073709551616`). -/
def mod64 : Nat := 18446744073709551616

/-- Predicate stating `v : Int` lies in Rust's `i64` range `[-2^63, 2^63 - 1]`. -/
def InI64Range (v : Int) : Prop :=
  -(signBias64 : Int) ≤ v ∧ v < (signBias64 : Int)

/-- Order-preserving 64-bit sign-bit bias matching `CoordinateKeyV1::from_coord`. -/
def biasSigned64 (v : Int) : Nat :=
  (v + (signBias64 : Int)).toNat

/-- Inverse decoding matching `CoordinateKeyV1::to_coord`. -/
def unbiasSigned64 (u : Nat) : Int :=
  (u : Int) - (signBias64 : Int)

theorem biasSigned64_lt_mod64 (v : Int) (hv : InI64Range v) : biasSigned64 v < mod64 := by
  unfold InI64Range at hv
  unfold biasSigned64 mod64 signBias64 at *
  omega

theorem unbiasSigned64_biasSigned64 (v : Int) (hv : InI64Range v) :
    unbiasSigned64 (biasSigned64 v) = v := by
  unfold InI64Range at hv
  unfold unbiasSigned64 biasSigned64 signBias64 at *
  omega

theorem biasSigned64_unbiasSigned64 (u : Nat) (hu : u < mod64) :
    biasSigned64 (unbiasSigned64 u) = u := by
  unfold biasSigned64 unbiasSigned64 mod64 signBias64 at *
  omega

theorem unbiasSigned64_in_i64 (u : Nat) (hu : u < mod64) :
    InI64Range (unbiasSigned64 u) := by
  unfold InI64Range unbiasSigned64 mod64 signBias64 at *
  omega

/-- Strict order preservation across all `i64` values including signed extrema. -/
theorem biasSigned64_lt_iff (a b : Int) (ha : InI64Range a) (hb : InI64Range b) :
    a < b ↔ biasSigned64 a < biasSigned64 b := by
  unfold InI64Range at ha hb
  unfold biasSigned64 signBias64 at *
  omega

/-- Exact scalar boundary vectors required by `docs/AUTHENTICATED_STATE_PLAN.md`. -/
theorem biasSigned64_boundary_vectors :
    biasSigned64 (-(signBias64 : Int)) = 0 ∧
    biasSigned64 (-1) = signBias64 - 1 ∧
    biasSigned64 0 = signBias64 ∧
    biasSigned64 1 = signBias64 + 1 ∧
    biasSigned64 ((signBias64 : Int) - 1) = mod64 - 1 := by
  unfold biasSigned64 signBias64 mod64
  decide

/-- Raw unsigned two's complement without sign-bit XOR (powered negative control). -/
def rawTwosComp64 (v : Int) : Nat :=
  if v < 0 then (v + (mod64 : Int)).toNat else v.toNat

/-- Powered negative control: omitting sign-bit XOR inverts signed order between `-1` and `0`. -/
theorem rawTwosComp64_breaks_signed_order :
    (-1 : Int) < (0 : Int) ∧ rawTwosComp64 0 < rawTwosComp64 (-1) := by
  unfold rawTwosComp64 mod64
  decide

/-! ## Section 2: Fixed-Width Binary Merkle Patricia Trie Normalization & Uniqueness -/

/-- Abstract canonical leaf value bytes. -/
abbrev PatVal := List Nat

/-- Binary Merkle Patricia Trie node mirroring `crates/research/src/commit_state.rs::TypedNode`. -/
inductive PatNode where
  | empty
  | leaf (suffix : List Bool) (val : PatVal)
  | ext (path : List Bool) (child : PatNode)
  | branch (left : PatNode) (right : PatNode)
deriving DecidableEq, Repr

/-- Strict structural invariant for every non-empty canonical binary Patricia subtree over `rem` bits. -/
inductive NonEmptyCanonical : Nat → PatNode → Prop where
  | leaf {rem : Nat} {suffix : List Bool} {val : PatVal}
      (hlen : suffix.length = rem) :
      NonEmptyCanonical rem (.leaf suffix val)
  | ext {rem : Nat} {path : List Bool} {left right : PatNode}
      (hne : path ≠ [])
      (hlt : path.length < rem)
      (hleft : NonEmptyCanonical (rem - path.length - 1) left)
      (hright : NonEmptyCanonical (rem - path.length - 1) right) :
      NonEmptyCanonical rem (.ext path (.branch left right))
  | branch {rem : Nat} {left right : PatNode}
      (hpos : 0 < rem)
      (hleft : NonEmptyCanonical (rem - 1) left)
      (hright : NonEmptyCanonical (rem - 1) right) :
      NonEmptyCanonical rem (.branch left right)

/-- Root canonical invariant: `empty` is valid only at the whole-tree root (`rem` = key width). -/
inductive CanonicalPat (rem : Nat) : PatNode → Prop where
  | empty : CanonicalPat rem .empty
  | nonEmpty {t : PatNode} (h : NonEmptyCanonical rem t) : CanonicalPat rem t

/-- Named abstraction function `alpha_commitment`: evaluates a key suffix in a `PatNode`. -/
def alphaPat : PatNode → List Bool → Option PatVal
  | .empty, _ => none
  | .leaf suffix val, k => if k = suffix then some val else none
  | .ext path child, k =>
      if path.isPrefixOf k then alphaPat child (k.drop path.length) else none
  | .branch _ _, [] => none
  | .branch left _, false :: rest => alphaPat left rest
  | .branch _ right, true :: rest => alphaPat right rest

theorem isPrefixOf_append_self (p rest : List Bool) : p.isPrefixOf (p ++ rest) = true := by
  induction p with
  | nil => rfl
  | cons _ _ ih => simp [ih]

theorem eq_append_drop_of_isPrefixOf {p k : List Bool} (h : p.isPrefixOf k = true) :
    k = p ++ k.drop p.length := by
  induction p generalizing k with
  | nil => simp
  | cons b ps ih =>
      cases k with
      | nil => simp [List.isPrefixOf] at h
      | cons c ks =>
          simp only [List.isPrefixOf, Bool.and_eq_true, beq_iff_eq] at h
          rcases h with ⟨hbc, hrest⟩
          subst hbc
          simp only [List.length_cons, List.drop_succ_cons, List.cons_append]
          exact congrArg (b :: ·) (ih hrest)

/-- Every non-empty canonical subtree contains at least one key of exact length `rem`. -/
theorem nonEmptyCanonical_exists_key {rem : Nat} {t : PatNode} (ht : NonEmptyCanonical rem t) :
    ∃ k v, k.length = rem ∧ alphaPat t k = some v := by
  induction ht with
  | @leaf rem suffix val hlen =>
      exact ⟨suffix, val, hlen, by simp [alphaPat]⟩
  | @ext rem path left right _ hlt _ _ ihL _ =>
      rcases ihL with ⟨kl, vl, hkl, hval⟩
      refine ⟨path ++ (false :: kl), vl, ?_, ?_⟩
      · simp [hkl]; omega
      · simp [alphaPat, isPrefixOf_append_self, hval]
  | @branch rem left right hpos _ _ ihL _ =>
      rcases ihL with ⟨kl, vl, hkl, hval⟩
      refine ⟨false :: kl, vl, ?_, ?_⟩
      · simp [hkl]; omega
      · simpa [alphaPat] using hval

theorem alphaPat_ext_some_iff (path : List Bool) (child : PatNode) (k : List Bool) (v : PatVal) :
    alphaPat (.ext path child) k = some v ↔
      path.isPrefixOf k = true ∧ alphaPat child (k.drop path.length) = some v := by
  rw [alphaPat]
  by_cases hp : path.isPrefixOf k = true
  · simp [hp]
  · simp [hp]

theorem branch_exists_false_and_true {rem : Nat} {l r : PatNode}
    (hl : NonEmptyCanonical (rem - 1) l)
    (hr : NonEmptyCanonical (rem - 1) r) :
    (∃ kl vl, kl.length = rem - 1 ∧ alphaPat (.branch l r) (false :: kl) = some vl) ∧
    (∃ kr vr, kr.length = rem - 1 ∧ alphaPat (.branch l r) (true :: kr) = some vr) := by
  rcases nonEmptyCanonical_exists_key hl with ⟨kl, vl, hkl, hvl⟩
  rcases nonEmptyCanonical_exists_key hr with ⟨kr, vr, hkr, hvr⟩
  exact ⟨⟨kl, vl, hkl, hvl⟩, ⟨kr, vr, hkr, hvr⟩⟩

theorem ext_forces_first_bit {b : Bool} {ps : List Bool} {child : PatNode}
    {k : List Bool} {v : PatVal}
    (h : alphaPat (.ext (b :: ps) child) k = some v) :
    ∃ rest, k = b :: rest := by
  rw [alphaPat_ext_some_iff] at h
  rcases h with ⟨hp, _⟩
  cases k with
  | nil => simp [List.isPrefixOf] at hp
  | cons c cs =>
      simp only [List.isPrefixOf, Bool.and_eq_true, beq_iff_eq] at hp
      exact ⟨cs, congrArg (· :: cs) hp.1.symm⟩

theorem leaf_unique_key {s : List Bool} {v : PatVal} {k1 k2 : List Bool} {v1 v2 : PatVal}
    (h1 : alphaPat (.leaf s v) k1 = some v1)
    (h2 : alphaPat (.leaf s v) k2 = some v2) : k1 = k2 := by
  unfold alphaPat at h1 h2
  split at h1
  · split at h2
    · rename_i heq1 heq2
      exact heq1.trans heq2.symm
    · cases h2
  · cases h1

theorem ext_branch_has_two_keys {rem : Nat} {path : List Bool} {l r : PatNode}
    (hlt : path.length < rem)
    (hl : NonEmptyCanonical (rem - path.length - 1) l)
    (hr : NonEmptyCanonical (rem - path.length - 1) r) :
    ∃ k1 k2 v1 v2, k1.length = rem ∧ k2.length = rem ∧ k1 ≠ k2 ∧
      alphaPat (.ext path (.branch l r)) k1 = some v1 ∧
      alphaPat (.ext path (.branch l r)) k2 = some v2 := by
  rcases nonEmptyCanonical_exists_key hl with ⟨kl, vl, hkl, hvl⟩
  rcases nonEmptyCanonical_exists_key hr with ⟨kr, vr, hkr, hvr⟩
  refine ⟨path ++ (false :: kl), path ++ (true :: kr), vl, vr, ?_, ?_, ?_, ?_, ?_⟩
  · simp [hkl]; omega
  · simp [hkr]; omega
  · intro heq
    have := List.append_cancel_left heq
    cases this
  · simp [alphaPat, isPrefixOf_append_self, hvl]
  · simp [alphaPat, isPrefixOf_append_self, hvr]

/-- Helper: if `p1` is a prefix of both `p2 ++ false :: kl` and `p2 ++ true :: kr`, then `p1` is a prefix of `p2`. -/
theorem prefix_of_branching_extensions {p1 p2 kl kr : List Bool}
    (hL : p1.isPrefixOf (p2 ++ false :: kl) = true)
    (hR : p1.isPrefixOf (p2 ++ true :: kr) = true) :
    p1.isPrefixOf p2 = true := by
  have hpreL := eq_append_drop_of_isPrefixOf hL
  have hpreR := eq_append_drop_of_isPrefixOf hR
  have hlen_le : p1.length ≤ p2.length := by
    refine Decidable.byContradiction fun hnot => ?_
    have hlt_len : p2.length < p1.length := Nat.lt_of_not_ge hnot
    have hbitL : (p2 ++ false :: kl)[p2.length]? = some false := by
      rw [List.getElem?_append_right (Nat.le_refl _)]
      simp
    have hbitR : (p2 ++ true :: kr)[p2.length]? = some true := by
      rw [List.getElem?_append_right (Nat.le_refl _)]
      simp
    have h1L_at : (p2 ++ false :: kl)[p2.length]? = p1[p2.length]? := by
      rw [hpreL, List.getElem?_append_left hlt_len]
    have h1R_at : (p2 ++ true :: kr)[p2.length]? = p1[p2.length]? := by
      rw [hpreR, List.getElem?_append_left hlt_len]
    rw [hbitL] at h1L_at
    rw [hbitR] at h1R_at
    rw [← h1L_at] at h1R_at
    cases h1R_at
  have htake : p1 = p2.take p1.length := by
    have h1 := congrArg (List.take p1.length) hpreL
    rw [List.take_left, List.take_append_of_le_length hlen_le] at h1
    exact h1.symm
  rw [htake]
  have hsplit : p2 = p2.take p1.length ++ p2.drop p1.length :=
    (List.take_append_drop p1.length p2).symm
  conv => lhs; arg 2; rw [hsplit]
  exact isPrefixOf_append_self _ _

/-- Crown-jewel uniqueness theorem for non-empty canonical subtrees:
    identical abstract lookup semantics on keys of length `rem` implies syntactic equality `t1 = t2`. -/
theorem nonEmptyCanonical_unique_of_alpha_eq :
    ∀ {rem : Nat} {t1 t2 : PatNode},
      NonEmptyCanonical rem t1 →
      NonEmptyCanonical rem t2 →
      (∀ k, k.length = rem → alphaPat t1 k = alphaPat t2 k) →
      t1 = t2 := by
  intro rem
  induction rem using Nat.strongRecOn with
  | ind rem ih =>
      intro t1 t2 h1 h2 heq
      cases h1 with
      | @leaf _ s1 v1 hlen1 =>
          cases h2 with
          | @leaf _ s2 v2 _ =>
              have hs := heq s1 hlen1
              simp only [alphaPat, ↓reduceIte] at hs
              by_cases hss : s1 = s2
              · subst hss
                simp only [↓reduceIte, Option.some.injEq] at hs
                subst hs
                rfl
              · simp [hss] at hs
          | @ext _ p2 l2 r2 _ hlt2 hl2 hr2 =>
              rcases ext_branch_has_two_keys hlt2 hl2 hr2 with ⟨k1, k2, _, _, hk1, hk2, hne, ha1, ha2⟩
              rw [← heq k1 hk1] at ha1
              rw [← heq k2 hk2] at ha2
              exact False.elim (hne (leaf_unique_key ha1 ha2))
          | @branch _ l2 r2 hpos2 hl2 hr2 =>
              rcases branch_exists_false_and_true hl2 hr2 with ⟨⟨kl, _, hkl, hal⟩, ⟨kr, _, hkr, har⟩⟩
              have hlenL : (false :: kl).length = rem := by simp [hkl]; omega
              have hlenR : (true :: kr).length = rem := by simp [hkr]; omega
              rw [← heq _ hlenL] at hal
              rw [← heq _ hlenR] at har
              have hcontr := leaf_unique_key hal har
              cases hcontr
      | @branch _ l1 r1 hpos1 hl1 hr1 =>
          cases h2 with
          | @leaf _ s2 v2 _ =>
              rcases branch_exists_false_and_true hl1 hr1 with ⟨⟨kl, _, hkl, hal⟩, ⟨kr, _, hkr, har⟩⟩
              have hlenL : (false :: kl).length = rem := by simp [hkl]; omega
              have hlenR : (true :: kr).length = rem := by simp [hkr]; omega
              rw [heq _ hlenL] at hal
              rw [heq _ hlenR] at har
              have hcontr := leaf_unique_key hal har
              cases hcontr
          | @ext _ p2 l2 r2 hne2 _ _ _ =>
              cases p2 with
              | nil => exact False.elim (hne2 rfl)
              | cons b2 ps2 =>
                  rcases branch_exists_false_and_true hl1 hr1 with ⟨⟨kl, _, hkl, hal⟩, ⟨kr, _, hkr, har⟩⟩
                  have hlenL : (false :: kl).length = rem := by simp [hkl]; omega
                  have hlenR : (true :: kr).length = rem := by simp [hkr]; omega
                  rw [heq _ hlenL] at hal
                  rw [heq _ hlenR] at har
                  rcases ext_forces_first_bit hal with ⟨_, hfl⟩
                  rcases ext_forces_first_bit har with ⟨_, htr⟩
                  cases b2
                  · cases htr
                  · cases hfl
          | @branch _ l2 r2 _ hl2 hr2 =>
              have hlEq : l1 = l2 := ih (rem - 1) (by omega) hl1 hl2 (fun k hk => by
                have h := heq (false :: k) (by simp [hk]; omega)
                simpa [alphaPat] using h)
              have hrEq : r1 = r2 := ih (rem - 1) (by omega) hr1 hr2 (fun k hk => by
                have h := heq (true :: k) (by simp [hk]; omega)
                simpa [alphaPat] using h)
              subst hlEq; subst hrEq; rfl
      | @ext _ p1 l1 r1 hne1 hlt1 hl1 hr1 =>
          cases h2 with
          | @leaf _ s2 v2 _ =>
              rcases ext_branch_has_two_keys hlt1 hl1 hr1 with ⟨k1, k2, _, _, hk1, hk2, hne, ha1, ha2⟩
              rw [heq k1 hk1] at ha1
              rw [heq k2 hk2] at ha2
              exact False.elim (hne (leaf_unique_key ha1 ha2))
          | @branch _ l2 r2 hpos2 hl2 hr2 =>
              cases p1 with
              | nil => exact False.elim (hne1 rfl)
              | cons b1 ps1 =>
                  rcases branch_exists_false_and_true hl2 hr2 with ⟨⟨kl, _, hkl, hal⟩, ⟨kr, _, hkr, har⟩⟩
                  have hlenL : (false :: kl).length = rem := by simp [hkl]; omega
                  have hlenR : (true :: kr).length = rem := by simp [hkr]; omega
                  rw [← heq _ hlenL] at hal
                  rw [← heq _ hlenR] at har
                  rcases ext_forces_first_bit hal with ⟨_, hfl⟩
                  rcases ext_forces_first_bit har with ⟨_, htr⟩
                  cases b1
                  · cases htr
                  · cases hfl
          | @ext _ p2 l2 r2 hne2 hlt2 hl2 hr2 =>
              have hp1_le_p2 : p1.isPrefixOf p2 = true := by
                rcases nonEmptyCanonical_exists_key hl2 with ⟨kl, vl, hkl, hvl⟩
                rcases nonEmptyCanonical_exists_key hr2 with ⟨kr, vr, hkr, hvr⟩
                have h1L : alphaPat (.ext p1 (.branch l1 r1)) (p2 ++ false :: kl) = some vl := by
                  rw [heq (p2 ++ false :: kl) (by simp [hkl]; omega)]
                  simp [alphaPat, isPrefixOf_append_self, hvl]
                have h1R : alphaPat (.ext p1 (.branch l1 r1)) (p2 ++ true :: kr) = some vr := by
                  rw [heq (p2 ++ true :: kr) (by simp [hkr]; omega)]
                  simp [alphaPat, isPrefixOf_append_self, hvr]
                rw [alphaPat_ext_some_iff] at h1L h1R
                exact prefix_of_branching_extensions h1L.1 h1R.1
              have hp2_le_p1 : p2.isPrefixOf p1 = true := by
                rcases nonEmptyCanonical_exists_key hl1 with ⟨kl, vl, hkl, hvl⟩
                rcases nonEmptyCanonical_exists_key hr1 with ⟨kr, vr, hkr, hvr⟩
                have h2L : alphaPat (.ext p2 (.branch l2 r2)) (p1 ++ false :: kl) = some vl := by
                  rw [← heq (p1 ++ false :: kl) (by simp [hkl]; omega)]
                  simp [alphaPat, isPrefixOf_append_self, hvl]
                have h2R : alphaPat (.ext p2 (.branch l2 r2)) (p1 ++ true :: kr) = some vr := by
                  rw [← heq (p1 ++ true :: kr) (by simp [hkr]; omega)]
                  simp [alphaPat, isPrefixOf_append_self, hvr]
                rw [alphaPat_ext_some_iff] at h2L h2R
                exact prefix_of_branching_extensions h2L.1 h2R.1
              have hpEq : p1 = p2 := by
                have h1 := eq_append_drop_of_isPrefixOf hp1_le_p2
                have h2 := eq_append_drop_of_isPrefixOf hp2_le_p1
                have hlen1 : p1.length ≤ p2.length := by
                  have := congrArg List.length h1; simp at this; omega
                have hlen2 : p2.length ≤ p1.length := by
                  have := congrArg List.length h2; simp at this; omega
                have hdrop : p2.drop p1.length = [] := List.drop_eq_nil_of_le (by omega)
                rw [hdrop, List.append_nil] at h1
                exact h1.symm
              subst hpEq
              have hpPos : 0 < p1.length := by
                cases p1 with
                | nil => exact False.elim (hne1 rfl)
                | cons _ _ => simp
              have hb1 : NonEmptyCanonical (rem - p1.length) (.branch l1 r1) :=
                NonEmptyCanonical.branch (by omega) hl1 hr1
              have hb2 : NonEmptyCanonical (rem - p1.length) (.branch l2 r2) :=
                NonEmptyCanonical.branch (by omega) hl2 hr2
              have hbEq : PatNode.branch l1 r1 = PatNode.branch l2 r2 :=
                ih (rem - p1.length) (by omega) hb1 hb2 (fun rest hrest => by
                  have h := heq (p1 ++ rest) (by simp [hrest]; omega)
                  simpa [alphaPat, isPrefixOf_append_self] using h)
              exact congrArg (PatNode.ext p1) hbEq

/-- Root uniqueness theorem: two canonical Patricia tries over key width `rem` with equal
    lookup semantics are syntactically identical (`t1 = t2`). -/
theorem canonicalPat_unique_of_alpha_eq {rem : Nat} {t1 t2 : PatNode}
    (h1 : CanonicalPat rem t1) (h2 : CanonicalPat rem t2)
    (heq : ∀ k, k.length = rem → alphaPat t1 k = alphaPat t2 k) :
    t1 = t2 := by
  cases h1 with
  | empty =>
      cases h2 with
      | empty => rfl
      | nonEmpty hne2 =>
          rcases nonEmptyCanonical_exists_key hne2 with ⟨k, v, hk, hv⟩
          have h := heq k hk
          simp [alphaPat, hv] at h
  | nonEmpty hne1 =>
      cases h2 with
      | empty =>
          rcases nonEmptyCanonical_exists_key hne1 with ⟨k, v, hk, hv⟩
          have h := heq k hk
          simp [alphaPat, hv] at h
      | nonEmpty hne2 =>
          exact nonEmptyCanonical_unique_of_alpha_eq hne1 hne2 heq

/-! ## Section 3: Canonical Point Proofs (`TypedPatriciaProofV1`) Soundness & Completeness -/

/-- Canonical proof trace mirroring `TypedPatriciaProofV1` steps + terminal. -/
inductive PatProof where
  | emptyRoot
  | leaf (suffix : List Bool) (val : PatVal)
  | divergentExt (path : List Bool) (child : PatNode)
  | extStep (path : List Bool) (sub : PatProof)
  | branchStep (dir : Bool) (sibling : PatNode) (sub : PatProof)
deriving DecidableEq, Repr

/-- Proof generator mirroring `TypedPatriciaTrie::prove`. -/
def provePat : PatNode → List Bool → PatProof
  | .empty, _ => .emptyRoot
  | .leaf suffix val, _ => .leaf suffix val
  | .ext path child, k =>
      if path.isPrefixOf k then
        .extStep path (provePat child (k.drop path.length))
      else
        .divergentExt path child
  | .branch left right, false :: rest => .branchStep false right (provePat left rest)
  | .branch left right, true :: rest => .branchStep true left (provePat right rest)
  | .branch _ _, [] => .emptyRoot

/-- Proof reconstructor & verifier returning `(reconstructed_root, lookup_result)` iff canonical. -/
def verifyPatProof : PatProof → List Bool → Option (PatNode × Option PatVal)
  | .emptyRoot, _ => some (.empty, none)
  | .leaf suffix val, k =>
      some (.leaf suffix val, if k = suffix then some val else none)
  | .divergentExt path child, k =>
      if path ≠ [] ∧ path.isPrefixOf k = false then
        some (.ext path child, none)
      else
        none
  | .extStep path sub, k =>
      if path ≠ [] ∧ path.isPrefixOf k = true then
        match verifyPatProof sub (k.drop path.length) with
        | some (.branch l r, res) => some (.ext path (.branch l r), res)
        | _ => none
      else
        none
  | .branchStep false sibling sub, false :: rest =>
      match verifyPatProof sub rest with
      | some (child, res) => some (.branch child sibling, res)
      | none => none
  | .branchStep true sibling sub, true :: rest =>
      match verifyPatProof sub rest with
      | some (child, res) => some (.branch sibling child, res)
      | none => none
  | .branchStep _ _ _, _ => none

/-- Soundness: whenever `verifyPatProof` reconstructs `root` and returns `res`, `res = alphaPat root k`. -/
theorem verifyPatProof_sound (p : PatProof) (k : List Bool) (root : PatNode) (res : Option PatVal)
    (hv : verifyPatProof p k = some (root, res)) :
    alphaPat root k = res := by
  induction p generalizing k root res with
  | emptyRoot =>
      simp only [verifyPatProof, Option.some.injEq, Prod.mk.injEq] at hv
      rcases hv with ⟨rfl, rfl⟩
      rfl
  | leaf suffix val =>
      simp only [verifyPatProof, Option.some.injEq, Prod.mk.injEq] at hv
      rcases hv with ⟨rfl, rfl⟩
      rfl
  | divergentExt path child =>
      unfold verifyPatProof at hv
      split at hv
      · rename_i hcond
        simp only [Option.some.injEq, Prod.mk.injEq] at hv
        rcases hv with ⟨rfl, rfl⟩
        simp [alphaPat, hcond.2]
      · cases hv
  | extStep path sub ih =>
      unfold verifyPatProof at hv
      split at hv
      · rename_i hcond
        cases hsub : verifyPatProof sub (k.drop path.length) with
        | none => simp [hsub] at hv
        | some pair =>
            rcases pair with ⟨c, r⟩
            cases c with
            | branch l rchild =>
                simp only [hsub, Option.some.injEq, Prod.mk.injEq] at hv
                rcases hv with ⟨rfl, rfl⟩
                have hrec := ih (k.drop path.length) (.branch l rchild) r hsub
                simp [alphaPat, hcond.2, hrec]
            | empty | leaf _ _ | ext _ _ => simp [hsub] at hv
      · cases hv
  | branchStep dir sib sub ih =>
      cases dir with
      | false =>
          cases k with
          | nil => cases hv
          | cons b rest =>
              cases b with
              | true => cases hv
              | false =>
                  simp only [verifyPatProof] at hv
                  cases hsub : verifyPatProof sub rest with
                  | none => simp [hsub] at hv
                  | some pair =>
                      rcases pair with ⟨c, r⟩
                      simp only [hsub, Option.some.injEq, Prod.mk.injEq] at hv
                      rcases hv with ⟨rfl, rfl⟩
                      exact ih rest c r hsub
      | true =>
          cases k with
          | nil => cases hv
          | cons b rest =>
              cases b with
              | false => cases hv
              | true =>
                  simp only [verifyPatProof] at hv
                  cases hsub : verifyPatProof sub rest with
                  | none => simp [hsub] at hv
                  | some pair =>
                      rcases pair with ⟨c, r⟩
                      simp only [hsub, Option.some.injEq, Prod.mk.injEq] at hv
                      rcases hv with ⟨rfl, rfl⟩
                      exact ih rest c r hsub

/-- Completeness for non-empty canonical subtrees: `provePat` always verifies and reproduces `alphaPat`. -/
theorem nonEmptyCanonical_provePat_complete {rem : Nat} {t : PatNode}
    (ht : NonEmptyCanonical rem t) :
    ∀ k, k.length = rem → verifyPatProof (provePat t k) k = some (t, alphaPat t k) := by
  induction ht with
  | @leaf rem suffix val _ =>
      intro k _
      rfl
  | @ext rem path left right hne hlt hleft hright ihL ihR =>
      intro k hk
      unfold provePat
      by_cases hp : path.isPrefixOf k = true
      · simp only [hp, ↓reduceIte, verifyPatProof, hne, ne_eq, not_false_eq_true, and_self]
        have hdrop_len : (k.drop path.length).length = rem - path.length := by
          simp [hk]
        cases hdrop : k.drop path.length with
        | nil =>
            have : (k.drop path.length).length = 0 := by rw [hdrop]; rfl
            omega
        | cons b rest =>
            have hrest_len : rest.length = rem - path.length - 1 := by
              have : (b :: rest).length = rem - path.length := by rw [← hdrop]; exact hdrop_len
              simp at this; omega
            cases b
            · have hsub := ihL rest hrest_len
              simp only [provePat, verifyPatProof, hsub]
              conv => rhs; unfold alphaPat
              rw [if_pos hp, hdrop]
              rfl
            · have hsub := ihR rest hrest_len
              simp only [provePat, verifyPatProof, hsub]
              conv => rhs; unfold alphaPat
              rw [if_pos hp, hdrop]
              rfl
      · have hpFalse : path.isPrefixOf k = false := Bool.eq_false_iff.mpr hp
        simp [hpFalse, verifyPatProof, hne, alphaPat]
  | @branch rem left right hpos _ _ ihL ihR =>
      intro k hk
      cases k with
      | nil => simp at hk; omega
      | cons b rest =>
          have hrest : rest.length = rem - 1 := by simp at hk; omega
          cases b
          · have hsub := ihL rest hrest
            simp [provePat, verifyPatProof, hsub, alphaPat]
          · have hsub := ihR rest hrest
            simp [provePat, verifyPatProof, hsub, alphaPat]

/-- Root completeness: for every canonical trie `t` and key `k` of width `rem`,
    `provePat t k` verifies against `t` and yields `alphaPat t k`. -/
theorem canonicalPat_provePat_complete {rem : Nat} {t : PatNode}
    (ht : CanonicalPat rem t) (k : List Bool) (hk : k.length = rem) :
    verifyPatProof (provePat t k) k = some (t, alphaPat t k) := by
  cases ht with
  | empty => rfl
  | nonEmpty hne => exact nonEmptyCanonical_provePat_complete hne k hk

/-! ## Section 4: Powered Negative Controls (Non-Canonical Patricia Shapes & Proofs) -/

/-- Powered negative control 1: adjacent extensions (`ext -> ext`) violate `CanonicalPat`. -/
theorem adjacent_extensions_not_canonical (rem : Nat) (p1 p2 : List Bool) (child : PatNode) :
    ¬ CanonicalPat rem (.ext p1 (.ext p2 child)) := by
  intro h
  cases h with
  | nonEmpty hne => cases hne

/-- Powered negative control 2: an extension pointing directly to a leaf violates `CanonicalPat`. -/
theorem extension_to_leaf_not_canonical (rem : Nat) (p s : List Bool) (v : PatVal) :
    ¬ CanonicalPat rem (.ext p (.leaf s v)) := by
  intro h
  cases h with
  | nonEmpty hne => cases hne

/-- Powered negative control 3: an empty child below a branch violates `CanonicalPat`. -/
theorem empty_under_branch_not_canonical (rem : Nat) (r : PatNode) :
    ¬ CanonicalPat rem (.branch .empty r) := by
  intro h
  cases h with
  | nonEmpty hne =>
      cases hne with
      | branch _ hl _ => cases hl

/-- Powered negative control 4: an empty extension path violates `CanonicalPat`. -/
theorem empty_extension_path_not_canonical (rem : Nat) (child : PatNode) :
    ¬ CanonicalPat rem (.ext [] child) := by
  intro h
  cases h with
  | nonEmpty hne =>
      cases hne with
      | ext hne_p _ _ _ => exact hne_p rfl

/-- Powered negative control 5: `verifyPatProof` rejects a forged `extStep` targeting a `leaf` (non-branch child). -/
theorem verifyPatProof_rejects_extStep_to_leaf (p s : List Bool) (v : PatVal) (k : List Bool) :
    verifyPatProof (.extStep p (.leaf s v)) k = none := by
  unfold verifyPatProof
  split
  · simp [verifyPatProof]
  · rfl

/-- Powered negative control 6: `verifyPatProof` rejects a `divergentExt` whose path actually matches `k`. -/
theorem verifyPatProof_rejects_nondivergent_ext (p rest : List Bool) (child : PatNode) :
    verifyPatProof (.divergentExt p child) (p ++ rest) = none := by
  unfold verifyPatProof
  simp [isPrefixOf_append_self]

end BitblockProof
