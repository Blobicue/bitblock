namespace BitblockProof

/-!
`CHAIN-LAUNCH-1`: Formal launch-closure bundle count/edge/byte conservation, combined DAG
strict-rank acyclicity, mandatory external genesis rank lower bound (`rank(X) ≥ 2`), reversed
genesis edge impossibility, and non-self-referential release/bundle/manifest/authorization
derivation corresponding to `crates/research/src/chain_launch_1.rs` (`LaunchClosureBundleV1`,
`LaunchClosureExternalGenesisDependencyNodeV1`, `LaunchClosureInternalDependencyNodeV1`,
`ChainLaunchManifestV1`, `ChainLaunchAuthorizationV1`, `ChainLaunchPackageV1`,
`preflight_launch_closure_bundle_bytes`, and `verify_chain_launch_package`).

Establishes under `lake env lean --trust=0` with zero custom axioms (`Quot.sound` and `propext`
only):
- Exact `LAYER1_DESIGN_PAPER.md` §2 launch-closure arithmetic equations:
  `internal = 7 + |S|`, `external = 1`, `complete = 8 + |S| = internal + 1`,
  `complete_edges = i2i + i2e + 2`, and `total_bytes = sum(E) + sum(S) + external_bytes`
  (covering both canonical `|S| = 3` -> `10/11` artifacts & `27` edges and minimal `|S| = 0` ->
  `7/8` artifacts & `18` edges);
- General path acyclicity of any combined dependency DAG satisfying strict rank monotonicity
  (`∀ e ∈ edges, rank e.dst < rank e.src`): every reachable path strictly lowers rank, forbidding
  self-loops, 2-cycles, and arbitrary cycles;
- Mandatory non-zero external genesis rank lower bound (`rank(DomainRegistry) < rank(ProfileScope)
  < rank(ExternalGenesis) < rank(CandidateProfile)`), proving `rank(ExternalGenesis) ≥ 2`,
  `rank(ExternalGenesis) ≠ 0`, `rank(CandidateProfile) ≥ 3`, and impossibility of reversed
  `ProfileScope -> ExternalGenesis` or `DomainRegistry -> ExternalGenesis` edges;
- Non-self-referential launch pipeline ordering proving `AdmittedReleaseCoreV1` cannot reach
  `ChainLaunchManifestV1`, `LaunchClosureBundleV1` cannot reach `ChainLaunchManifestV1`, and
  `ChainLaunchManifestV1` cannot reach `ChainLaunchAuthorizationV1`;
- Concrete verification that the exact canonical `|S| = 3` edge set and rank assignment
  (`registry = 0, scope = 1, genesis = 2, candidate = 3, spec = 4, admission/comp/inputs = 5,
  receipt/parity = 6, release = 7`) satisfies strict rank monotonicity;
- Powered negative controls proving off-by-one internal counts, duplicate external roots,
  double-counted embedded registry bytes, and rank-zero genesis assignments are unsatisfiable.

This is an abstract mathematical model over Lean `Nat` and finite lists. It does not formally
verify `rustc`/LLVM machine code, SHA-256 hashing, RFC 8032 Ed25519 cryptography, or network
liveness.
-/

/-! ## Section 1: Launch-Closure Count, Edge, and Byte-Conservation Equations -/

/-- Sum of byte lengths across a list of canonical preimages. -/
def byteSum : List Nat → Nat
  | [] => 0
  | n :: rest => n + byteSum rest

/-- Fixed-width header and preimage-length witness mirroring `LaunchClosureBundleV1`. -/
structure LaunchClosureHeader where
  /-- Declared `internal_artifact_count_u32`. -/
  internalArtifacts : Nat
  /-- Declared `external_root_count_u32`. -/
  externalRoots : Nat
  /-- Declared `complete_artifact_count_u32`. -/
  completeArtifacts : Nat
  /-- Declared `supplementary_artifact_count_u32`. -/
  supplementaryArtifacts : Nat
  /-- Declared `internal_to_internal_dependency_edge_count_u32`. -/
  i2iEdges : Nat
  /-- Declared `internal_to_external_dependency_edge_count_u32`. -/
  i2eEdges : Nat
  /-- Declared `external_to_internal_dependency_edge_count_u32`. -/
  e2iEdges : Nat
  /-- Declared `complete_dependency_edge_count_u32`. -/
  completeEdges : Nat
  /-- Declared `internal_canonical_preimage_bytes_u64`. -/
  internalBytes : Nat
  /-- Declared `external_root_canonical_preimage_bytes_u64` (`Length(G)`). -/
  externalBytes : Nat
  /-- Declared `total_canonical_preimage_bytes_u64`. -/
  totalBytes : Nat
  /-- Lengths of the seven explicit preimages `E[0..7]`. -/
  explicitLengths : List Nat
  /-- Lengths of the embedded supplementary preimages `S[0..|S|]`. -/
  supplementaryLengths : List Nat
deriving DecidableEq, Repr

/-- Exact `LAYER1_DESIGN_PAPER.md` §2 checked equations for `LaunchClosureBundleV1`. -/
structure ValidLaunchClosureEquations (h : LaunchClosureHeader) : Prop where
  explicitLenEq : h.explicitLengths.length = 7
  suppLenEq : h.supplementaryArtifacts = h.supplementaryLengths.length
  internalEq : h.internalArtifacts = 7 + h.supplementaryArtifacts
  externalEq : h.externalRoots = 1
  completeEq : h.completeArtifacts = h.internalArtifacts + h.externalRoots
  e2iEq : h.e2iEdges = 2
  completeEdgesEq : h.completeEdges = h.i2iEdges + h.i2eEdges + h.e2iEdges
  internalBytesEq : h.internalBytes = byteSum h.explicitLengths + byteSum h.supplementaryLengths
  totalBytesEq : h.totalBytes = h.internalBytes + h.externalBytes

theorem complete_artifact_count_eq_eight_plus_supp (h : LaunchClosureHeader)
    (hv : ValidLaunchClosureEquations h) :
    h.completeArtifacts = 8 + h.supplementaryArtifacts := by
  have h1 := hv.internalEq
  have h2 := hv.externalEq
  have h3 := hv.completeEq
  omega

theorem complete_artifact_count_eq_internal_plus_one (h : LaunchClosureHeader)
    (hv : ValidLaunchClosureEquations h) :
    h.completeArtifacts = h.internalArtifacts + 1 := by
  have h2 := hv.externalEq
  have h3 := hv.completeEq
  omega

theorem canonical_s3_artifact_counts (h : LaunchClosureHeader)
    (hv : ValidLaunchClosureEquations h) (hs : h.supplementaryArtifacts = 3) :
    h.internalArtifacts = 10 ∧ h.completeArtifacts = 11 := by
  have h1 := hv.internalEq
  have h2 := hv.externalEq
  have h3 := hv.completeEq
  constructor <;> omega

theorem minimal_s0_artifact_counts (h : LaunchClosureHeader)
    (hv : ValidLaunchClosureEquations h) (hs : h.supplementaryArtifacts = 0) :
    h.internalArtifacts = 7 ∧ h.completeArtifacts = 8 := by
  have h1 := hv.internalEq
  have h2 := hv.externalEq
  have h3 := hv.completeEq
  constructor <;> omega

theorem complete_edge_count_partition (h : LaunchClosureHeader)
    (hv : ValidLaunchClosureEquations h) :
    h.completeEdges = h.i2iEdges + h.i2eEdges + 2 := by
  have he := hv.e2iEq
  have hc := hv.completeEdgesEq
  omega

theorem canonical_s3_edge_count (h : LaunchClosureHeader)
    (hv : ValidLaunchClosureEquations h) (hi : h.i2iEdges = 22) (he : h.i2eEdges = 3) :
    h.completeEdges = 27 := by
  have hp := complete_edge_count_partition h hv
  omega

theorem minimal_s0_edge_count (h : LaunchClosureHeader)
    (hv : ValidLaunchClosureEquations h) (hi : h.i2iEdges = 13) (he : h.i2eEdges = 3) :
    h.completeEdges = 18 := by
  have hp := complete_edge_count_partition h hv
  omega

theorem total_bytes_exact_sum_no_double_count (h : LaunchClosureHeader)
    (hv : ValidLaunchClosureEquations h) :
    h.totalBytes = byteSum h.explicitLengths + byteSum h.supplementaryLengths + h.externalBytes := by
  have hi := hv.internalBytesEq
  have ht := hv.totalBytesEq
  omega

theorem byteSum_cons (x : Nat) (xs : List Nat) :
    byteSum (x :: xs) = x + byteSum xs := rfl

theorem byteSum_append (xs ys : List Nat) :
    byteSum (xs ++ ys) = byteSum xs + byteSum ys := by
  induction xs with
  | nil => simp [byteSum]
  | cons a rest ih =>
    simp [byteSum, ih]
    omega

theorem adding_supplementary_artifact_updates_totals (h : LaunchClosureHeader)
    (hv : ValidLaunchClosureEquations h) (newLen : Nat) :
    let h' : LaunchClosureHeader := {
      h with
      internalArtifacts := h.internalArtifacts + 1
      completeArtifacts := h.completeArtifacts + 1
      supplementaryArtifacts := h.supplementaryArtifacts + 1
      internalBytes := h.internalBytes + newLen
      totalBytes := h.totalBytes + newLen
      supplementaryLengths := newLen :: h.supplementaryLengths
    }
    ValidLaunchClosureEquations h' := by
  refine
    { explicitLenEq := hv.explicitLenEq
      suppLenEq := ?_
      internalEq := ?_
      externalEq := hv.externalEq
      completeEq := ?_
      e2iEq := hv.e2iEq
      completeEdgesEq := hv.completeEdgesEq
      internalBytesEq := ?_
      totalBytesEq := ?_ }
  · simp [hv.suppLenEq]
  · dsimp only; have := hv.internalEq; omega
  · dsimp only; have := hv.completeEq; omega
  · dsimp only; simp [byteSum, hv.internalBytesEq]; omega
  · dsimp only; have := hv.totalBytesEq; omega

/-! ## Section 2: Combined Dependency DAG & General Path Acyclicity -/

/-- Directed dependency edge `src -> dst` meaning `src` depends on `dst`. -/
structure DagEdge where
  /-- Referring artifact node ID. -/
  src : Nat
  /-- Referenced dependency target node ID. -/
  dst : Nat
deriving DecidableEq, Repr

/-- Strict checked topological rank monotonicity (`rank(target) < rank(source)` on every edge). -/
def StrictRankMonotone (rank : Nat → Nat) (edges : List DagEdge) : Prop :=
  ∀ e, e ∈ edges → rank e.dst < rank e.src

/-- Non-empty directed dependency reachability (`u` transitively depends on `v`). -/
inductive Reachable (edges : List DagEdge) : Nat → Nat → Prop where
  | step {u v : Nat} : ⟨u, v⟩ ∈ edges → Reachable edges u v
  | trans {u v w : Nat} : ⟨u, v⟩ ∈ edges → Reachable edges v w → Reachable edges u w

theorem reachable_rank_strict_lt (rank : Nat → Nat) (edges : List DagEdge)
    (hm : StrictRankMonotone rank edges) {u v : Nat} (hr : Reachable edges u v) :
    rank v < rank u := by
  induction hr with
  | step hedge =>
    exact hm _ hedge
  | trans hedge _ ih =>
    have h1 := hm _ hedge
    dsimp only at h1
    omega

theorem no_self_loop_of_strict_rank (rank : Nat → Nat) (edges : List DagEdge)
    (hm : StrictRankMonotone rank edges) (u : Nat) :
    ¬ Reachable edges u u := by
  intro hr
  have hlt := reachable_rank_strict_lt rank edges hm hr
  omega

theorem no_two_cycle_of_strict_rank (rank : Nat → Nat) (edges : List DagEdge)
    (hm : StrictRankMonotone rank edges) {u v : Nat}
    (huv : Reachable edges u v) :
    ¬ Reachable edges v u := by
  intro hvu
  have h1 := reachable_rank_strict_lt rank edges hm huv
  have h2 := reachable_rank_strict_lt rank edges hm hvu
  omega

theorem edge_implies_distinct_endpoints (rank : Nat → Nat) (edges : List DagEdge)
    (hm : StrictRankMonotone rank edges) {e : DagEdge} (he : e ∈ edges) :
    e.src ≠ e.dst := by
  intro heq
  have hlt := hm e he
  rw [heq] at hlt
  omega

theorem strict_rank_monotone_subset (rank : Nat → Nat) (e1 e2 : List DagEdge)
    (hsub : ∀ e, e ∈ e1 → e ∈ e2) (hm : StrictRankMonotone rank e2) :
    StrictRankMonotone rank e1 := by
  intro e he
  exact hm e (hsub e he)

/-! ## Section 3: Mandatory External Genesis Rank Lower Bound (`X` Never Rank 0) -/

/-- Canonical node identifiers mirroring `crates/research/src/chain_launch_1.rs`. -/
def nodeDomainRegistry : Nat := 0
def nodeProfileScope : Nat := 1
def nodeExternalGenesis : Nat := 2
def nodeCandidateProfile : Nat := 3
def nodeTransitionSpec : Nat := 4
def nodeProfileAdmission : Nat := 5
def nodeExecComposition : Nat := 6
def nodeCandidateInputs : Nat := 7
def nodeCandidateReceipt : Nat := 8
def nodeParityRecord : Nat := 9
def nodeAdmittedRelease : Nat := 10
def nodeLaunchBundle : Nat := 11
def nodeLaunchManifest : Nat := 12
def nodeLaunchAuth : Nat := 13

/-- Mandatory core genesis chain present in every valid `CHAIN-LAUNCH-1` combined DAG. -/
structure HasCanonicalGenesisChain (edges : List DagEdge) : Prop where
  scopeToRegistry : ⟨nodeProfileScope, nodeDomainRegistry⟩ ∈ edges
  genesisToScope : ⟨nodeExternalGenesis, nodeProfileScope⟩ ∈ edges
  genesisToRegistry : ⟨nodeExternalGenesis, nodeDomainRegistry⟩ ∈ edges
  candidateToGenesis : ⟨nodeCandidateProfile, nodeExternalGenesis⟩ ∈ edges

theorem profile_scope_rank_ge_one (rank : Nat → Nat) (edges : List DagEdge)
    (hm : StrictRankMonotone rank edges) (hc : HasCanonicalGenesisChain edges) :
    rank nodeProfileScope ≥ 1 := by
  have h1 := hm _ hc.scopeToRegistry
  dsimp only at h1
  omega

theorem external_genesis_rank_ge_two (rank : Nat → Nat) (edges : List DagEdge)
    (hm : StrictRankMonotone rank edges) (hc : HasCanonicalGenesisChain edges) :
    rank nodeExternalGenesis ≥ 2 := by
  have h1 := hm _ hc.scopeToRegistry
  have h2 := hm _ hc.genesisToScope
  dsimp only at h1 h2
  omega

theorem external_genesis_rank_nonzero (rank : Nat → Nat) (edges : List DagEdge)
    (hm : StrictRankMonotone rank edges) (hc : HasCanonicalGenesisChain edges) :
    rank nodeExternalGenesis ≠ 0 := by
  have h2 := external_genesis_rank_ge_two rank edges hm hc
  omega

theorem candidate_profile_rank_ge_three (rank : Nat → Nat) (edges : List DagEdge)
    (hm : StrictRankMonotone rank edges) (hc : HasCanonicalGenesisChain edges) :
    rank nodeCandidateProfile ≥ 3 := by
  have h2 := external_genesis_rank_ge_two rank edges hm hc
  have h3 := hm _ hc.candidateToGenesis
  dsimp only at h3
  omega

theorem reversed_scope_to_genesis_impossible (rank : Nat → Nat) (edges : List DagEdge)
    (hm : StrictRankMonotone rank edges) (hc : HasCanonicalGenesisChain edges) :
    ⟨nodeProfileScope, nodeExternalGenesis⟩ ∉ edges := by
  intro hrev
  have hf := hm _ hc.genesisToScope
  have hb := hm _ hrev
  dsimp only at hf hb
  omega

theorem reversed_registry_to_genesis_impossible (rank : Nat → Nat) (edges : List DagEdge)
    (hm : StrictRankMonotone rank edges) (hc : HasCanonicalGenesisChain edges) :
    ⟨nodeDomainRegistry, nodeExternalGenesis⟩ ∉ edges := by
  intro hrev
  have hf := hm _ hc.genesisToRegistry
  have hb := hm _ hrev
  dsimp only at hf hb
  omega

/-! ## Section 4: Non-Self-Referential Release, Bundle, Manifest & Authorization Pipeline -/

/-- Acyclic post-candidate launch pipeline from `CandidateProfile` through `ChainLaunchAuth`. -/
structure HasLaunchPipelineEdges (edges : List DagEdge) : Prop where
  specToCandidate : ⟨nodeTransitionSpec, nodeCandidateProfile⟩ ∈ edges
  admissionToSpec : ⟨nodeProfileAdmission, nodeTransitionSpec⟩ ∈ edges
  releaseToAdmission : ⟨nodeAdmittedRelease, nodeProfileAdmission⟩ ∈ edges
  bundleToRelease : ⟨nodeLaunchBundle, nodeAdmittedRelease⟩ ∈ edges
  manifestToBundle : ⟨nodeLaunchManifest, nodeLaunchBundle⟩ ∈ edges
  authToManifest : ⟨nodeLaunchAuth, nodeLaunchManifest⟩ ∈ edges

theorem release_cannot_depend_on_launch_manifest (rank : Nat → Nat) (edges : List DagEdge)
    (hm : StrictRankMonotone rank edges) (hp : HasLaunchPipelineEdges edges) :
    ¬ Reachable edges nodeAdmittedRelease nodeLaunchManifest := by
  have hbr := hm _ hp.bundleToRelease
  have hmb := hm _ hp.manifestToBundle
  dsimp only at hbr hmb
  intro hr
  have hlt := reachable_rank_strict_lt rank edges hm hr
  omega

theorem bundle_cannot_depend_on_launch_manifest (rank : Nat → Nat) (edges : List DagEdge)
    (hm : StrictRankMonotone rank edges) (hp : HasLaunchPipelineEdges edges) :
    ¬ Reachable edges nodeLaunchBundle nodeLaunchManifest := by
  have hmb := hm _ hp.manifestToBundle
  dsimp only at hmb
  intro hr
  have hlt := reachable_rank_strict_lt rank edges hm hr
  omega

theorem manifest_cannot_depend_on_launch_auth (rank : Nat → Nat) (edges : List DagEdge)
    (hm : StrictRankMonotone rank edges) (hp : HasLaunchPipelineEdges edges) :
    ¬ Reachable edges nodeLaunchManifest nodeLaunchAuth := by
  have ham := hm _ hp.authToManifest
  dsimp only at ham
  intro hr
  have hlt := reachable_rank_strict_lt rank edges hm hr
  omega

/-- Concrete canonical `|S| = 3` rank assignment realized in `crates/research/src/chain_launch_1.rs`. -/
def canonicalS3Rank : Nat → Nat
  | 0 => 0   -- DomainRegistryV1
  | 1 => 1   -- ProfileScopeV1
  | 2 => 2   -- ExternalGenesis X
  | 3 => 3   -- CandidateProfileV1
  | 4 => 4   -- TransitionSpecAtProfileV1
  | 5 => 5   -- ProfileAdmissionManifestV1
  | 6 => 5   -- ExecutionRootCompositionV1 (S0)
  | 7 => 5   -- CandidateBuildInputsV1 (S1)
  | 8 => 6   -- CandidateReleaseReceiptV1 (S2)
  | 9 => 6   -- GenesisRootParityRecordV1
  | 10 => 7  -- AdmittedReleaseRecordV1
  | 11 => 8  -- LaunchClosureBundleV1
  | 12 => 9  -- ChainLaunchManifestV1
  | 13 => 10 -- ChainLaunchAuthorizationV1
  | _ => 11

/-- Complete 27-edge combined DAG for the canonical `|S| = 3` launch closure. -/
def canonicalS3Edges : List DagEdge :=
  [ ⟨nodeProfileScope, nodeDomainRegistry⟩,
    ⟨nodeExternalGenesis, nodeProfileScope⟩,
    ⟨nodeExternalGenesis, nodeDomainRegistry⟩,
    ⟨nodeCandidateProfile, nodeDomainRegistry⟩,
    ⟨nodeCandidateProfile, nodeProfileScope⟩,
    ⟨nodeCandidateProfile, nodeExternalGenesis⟩,
    ⟨nodeTransitionSpec, nodeProfileScope⟩,
    ⟨nodeTransitionSpec, nodeCandidateProfile⟩,
    ⟨nodeProfileAdmission, nodeCandidateProfile⟩,
    ⟨nodeProfileAdmission, nodeTransitionSpec⟩,
    ⟨nodeExecComposition, nodeDomainRegistry⟩,
    ⟨nodeExecComposition, nodeCandidateProfile⟩,
    ⟨nodeCandidateInputs, nodeDomainRegistry⟩,
    ⟨nodeCandidateInputs, nodeCandidateProfile⟩,
    ⟨nodeCandidateReceipt, nodeCandidateInputs⟩,
    ⟨nodeCandidateReceipt, nodeCandidateProfile⟩,
    ⟨nodeParityRecord, nodeCandidateProfile⟩,
    ⟨nodeParityRecord, nodeTransitionSpec⟩,
    ⟨nodeParityRecord, nodeExecComposition⟩,
    ⟨nodeParityRecord, nodeExternalGenesis⟩,
    ⟨nodeAdmittedRelease, nodeCandidateProfile⟩,
    ⟨nodeAdmittedRelease, nodeTransitionSpec⟩,
    ⟨nodeAdmittedRelease, nodeProfileAdmission⟩,
    ⟨nodeAdmittedRelease, nodeParityRecord⟩,
    ⟨nodeAdmittedRelease, nodeCandidateInputs⟩,
    ⟨nodeAdmittedRelease, nodeCandidateReceipt⟩,
    ⟨nodeAdmittedRelease, nodeExternalGenesis⟩ ]

theorem canonical_s3_edge_list_len_eq_27 : canonicalS3Edges.length = 27 := rfl

theorem canonical_s3_concrete_ranks_strict_monotone :
    StrictRankMonotone canonicalS3Rank canonicalS3Edges := by
  intro e he
  simp only [canonicalS3Edges, List.mem_cons, List.not_mem_nil, or_false] at he
  rcases he with rfl | rfl | rfl | rfl | rfl | rfl | rfl | rfl | rfl | rfl | rfl | rfl | rfl |
    rfl | rfl | rfl | rfl | rfl | rfl | rfl | rfl | rfl | rfl | rfl | rfl | rfl | rfl <;>
    decide

/-! ## Section 5: Powered Negative Controls -/

/-- Negative control 1: incrementing `internalArtifacts` by 1 (`8 + |S|` instead of `7 + |S|`)
violates `ValidLaunchClosureEquations`. -/
theorem off_by_one_internal_count_rejected (h : LaunchClosureHeader)
    (hbad : h.internalArtifacts = 8 + h.supplementaryArtifacts) :
    ¬ ValidLaunchClosureEquations h := by
  intro hv
  have hgood := hv.internalEq
  omega

/-- Negative control 2: injecting a second external root (`externalRoots = 2`) violates
`ValidLaunchClosureEquations`. -/
theorem duplicate_external_root_rejected (h : LaunchClosureHeader)
    (hbad : h.externalRoots = 2) :
    ¬ ValidLaunchClosureEquations h := by
  intro hv
  have hgood := hv.externalEq
  omega

/-- Negative control 3: double-counting an embedded registry of positive length `regBytes > 0`
inside `totalBytes` violates `ValidLaunchClosureEquations`. -/
theorem double_counted_registry_bytes_rejected (h : LaunchClosureHeader) (regBytes : Nat)
    (hpos : regBytes > 0)
    (hbad : h.totalBytes = byteSum h.explicitLengths + byteSum h.supplementaryLengths + h.externalBytes + regBytes) :
    ¬ ValidLaunchClosureEquations h := by
  intro hv
  have hgood := total_bytes_exact_sum_no_double_count h hv
  omega

/-- Negative control 4: forcing the external genesis node `X` to topological rank `0` is
mathematically unsatisfiable under any `StrictRankMonotone` combined DAG. -/
theorem rank_zero_genesis_unsatisfiable (rank : Nat → Nat) (edges : List DagEdge)
    (hc : HasCanonicalGenesisChain edges) (hzero : rank nodeExternalGenesis = 0) :
    ¬ StrictRankMonotone rank edges := by
  intro hm
  have hne := external_genesis_rank_nonzero rank edges hm hc
  exact hne hzero

end BitblockProof
