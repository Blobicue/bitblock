namespace BitblockProof

/-!
`SUCCESSOR-ACTIVATION-1`: Formal internal-only successor-activation bundle equations,
strict-rank internal DAG acyclicity, non-serialized 3-stage acyclic authority ordering
(`PredecessorBase -> ReleaseAuthority -> BundleComplete -> ActivationAuthority -> MigrationExec`),
strict release-vs-activation purpose disjointness, replay-archive exclusion, and dedicated
migration body exclusivity corresponding to `crates/research/src/successor_activation_1.rs`
(`SuccessorActivationBundleV1`, `SuccessorActivationInternalDependencyNodeV1`,
`SuccessorActivationSignDocV1`, `ActivationAuthorizationV1`, `DedicatedMigrationBodyV1`,
`SuccessorActivationCarrierV1`, `VerifiedSuccessorPredecessorBaseContextV1`,
`VerifiedSuccessorReleaseAuthorityContextV1`, `VerifiedSuccessorActivationAuthorityContextV1`,
`preflight_successor_activation_bundle_bytes`, and `verify_successor_activation_carrier`).

Establishes under `lake env lean --trust=0` with zero custom axioms (`Quot.sound` and `propext`
only):
- Exact internal-only `LAYER1_DESIGN_PAPER.md` §2 successor equations:
  `externalRoots = 0`, `Length(E_s) = 7`, `internalArtifacts = 7 + |S_s| = |D_s|`,
  `internalBytes = sum(E_s) + sum(S_s)` (excluding all prior launch/genesis/predecessor bytes;
  covering both canonical `|S_s| = 3` -> `10` artifacts / `23` edges and minimal `|S_s| = 0` ->
  `7` artifacts / `15` edges);
- General internal-only DAG acyclicity from strict rank monotonicity
  (`∀ e ∈ edges, rank e.dst < rank e.src`): every reachable path strictly decreases rank,
  forbidding self-loops, 2-cycles, and arbitrary cycles;
- Concrete verification that the 23-edge canonical `|S_s| = 3` internal DAG
  (`reg=0, scope=1, candidate=2, spec=3, migSpec=4, inputs=4, receipt=5, evidence=5,
  admission=6, release=7`) satisfies strict rank monotonicity AND `evidence < admission < release`;
- Acyclic 3-stage semantic authority ordering proving Stage 3 (`ActivationAuthority`) and
  `ActivationAuthorization` strictly follow `BundleComplete`, so neither can enter or pre-resolve
  `SuccessorActivationBundleV1`;
- Strict purpose disjointness (`ReleaseAuthority ≠ ActivationAuthority`) and replay exclusion
  (`fromReplayArchive = false`), proving purpose swapping, purpose coercion, single-purpose
  substitution, and replay-archive substitution are unsatisfiable;
- Exclusive `DedicatedMigrationBodyV1` enforcement (`txCount = 0`, `appSubtickCount = 0`,
  `migrationTransitionCount = 1`) for `InPlaceMigrationTransitionV1`;
- Powered negative controls proving off-by-one internal counts, non-zero external root injection,
  duplicated prior bytes, backward admission-to-evidence edge, swapped purposes, replay inputs,
  and non-empty migration transactions are unsatisfiable.

This is an abstract mathematical model over Lean `Nat` and finite lists. It does not formally
verify `rustc`/LLVM machine code, SHA-256 hashing, RFC 8032 Ed25519 cryptography, or network
liveness.
-/

/-! ## Section 1: Internal-Only Successor Bundle Count, Edge, and Byte Equations -/

/-- Sum of canonical preimage lengths across an internal artifact list. -/
def succByteSum : List Nat → Nat
  | [] => 0
  | n :: rest => n + succByteSum rest

/-- Fixed-width header witness mirroring internal-only `SuccessorActivationBundleV1`. -/
structure SuccessorBundleHeader where
  /-- Declared `internal_artifact_count_u32`. -/
  internalArtifacts : Nat
  /-- External root count (MUST be strictly `0`: internal-only). -/
  externalRoots : Nat
  /-- Declared `supplementary_internal_artifact_count_u32`. -/
  supplementaryArtifacts : Nat
  /-- Count of internal dependency nodes `|D_s|`. -/
  depNodeCount : Nat
  /-- Declared `internal_dependency_edge_count_u32`. -/
  internalEdges : Nat
  /-- Declared `internal_canonical_preimage_bytes_u64`. -/
  internalBytes : Nat
  /-- Count of prior launch/genesis/predecessor bytes duplicated inside `S_s` (MUST be `0`). -/
  duplicatedPriorBytes : Nat
  /-- Lengths of the seven explicit internal preimages `E_s[0..7]`. -/
  explicitLengths : List Nat
  /-- Lengths of embedded supplementary internal preimages `S_s[0..|S_s|]`. -/
  supplementaryLengths : List Nat
deriving DecidableEq, Repr

/-- Exact `LAYER1_DESIGN_PAPER.md` §2 internal-only successor bundle equations. -/
structure ValidSuccessorBundleEquations (h : SuccessorBundleHeader) : Prop where
  externalZero : h.externalRoots = 0
  explicitLenEq : h.explicitLengths.length = 7
  suppLenEq : h.supplementaryArtifacts = h.supplementaryLengths.length
  internalEq : h.internalArtifacts = 7 + h.supplementaryArtifacts
  depNodeEq : h.depNodeCount = h.internalArtifacts
  priorZero : h.duplicatedPriorBytes = 0
  internalBytesEq :
    h.internalBytes = succByteSum h.explicitLengths + succByteSum h.supplementaryLengths

theorem succ_dep_nodes_eq_seven_plus_supp (h : SuccessorBundleHeader)
    (hv : ValidSuccessorBundleEquations h) :
    h.depNodeCount = 7 + h.supplementaryArtifacts := by
  have h1 := hv.internalEq
  have h2 := hv.depNodeEq
  omega

theorem succ_external_roots_strictly_zero (h : SuccessorBundleHeader)
    (hv : ValidSuccessorBundleEquations h) :
    h.externalRoots = 0 ∧ h.internalArtifacts + h.externalRoots = h.internalArtifacts := by
  have hz := hv.externalZero
  constructor <;> omega

theorem succ_canonical_s3_counts (h : SuccessorBundleHeader)
    (hv : ValidSuccessorBundleEquations h) (hs : h.supplementaryArtifacts = 3) :
    h.internalArtifacts = 10 ∧ h.depNodeCount = 10 := by
  have h1 := hv.internalEq
  have h2 := hv.depNodeEq
  constructor <;> omega

theorem succ_minimal_s0_counts (h : SuccessorBundleHeader)
    (hv : ValidSuccessorBundleEquations h) (hs : h.supplementaryArtifacts = 0) :
    h.internalArtifacts = 7 ∧ h.depNodeCount = 7 := by
  have h1 := hv.internalEq
  have h2 := hv.depNodeEq
  constructor <;> omega

theorem succ_internal_bytes_no_prior_duplication (h : SuccessorBundleHeader)
    (hv : ValidSuccessorBundleEquations h) :
    h.internalBytes + h.duplicatedPriorBytes =
      succByteSum h.explicitLengths + succByteSum h.supplementaryLengths := by
  have hb := hv.internalBytesEq
  have hp := hv.priorZero
  omega

theorem succByteSum_cons (x : Nat) (xs : List Nat) :
    succByteSum (x :: xs) = x + succByteSum xs := rfl

theorem succByteSum_append (xs ys : List Nat) :
    succByteSum (xs ++ ys) = succByteSum xs + succByteSum ys := by
  induction xs with
  | nil => simp [succByteSum]
  | cons x rest ih =>
    simp [succByteSum, ih, Nat.add_assoc]

/-! ## Section 2: Internal-Only Successor DAG Strict-Rank Acyclicity -/

/-- Directed internal-only dependency edge `src -> dst` inside `D_s`. -/
structure SuccEdge where
  src : Nat
  dst : Nat
deriving DecidableEq, Repr

/-- Strict topological rank monotonicity: every internal edge points to a strictly lower rank. -/
def SuccStrictRankMonotone (rank : Nat → Nat) (edges : List SuccEdge) : Prop :=
  ∀ e, e ∈ edges → rank e.dst < rank e.src

/-- Non-empty directed path reachability inside the internal-only DAG. -/
inductive SuccReachable (edges : List SuccEdge) : Nat → Nat → Prop where
  | step {u v : Nat} : SuccEdge.mk u v ∈ edges → SuccReachable edges u v
  | trans {u v w : Nat} :
      SuccEdge.mk u v ∈ edges → SuccReachable edges v w → SuccReachable edges u w

theorem succ_reachable_rank_lt {rank : Nat → Nat} {edges : List SuccEdge}
    (hm : SuccStrictRankMonotone rank edges) {u v : Nat} (hr : SuccReachable edges u v) :
    rank v < rank u := by
  induction hr with
  | step h => exact hm _ h
  | trans h _ ih => exact Nat.lt_trans ih (hm _ h)

theorem succ_dag_no_self_loop {rank : Nat → Nat} {edges : List SuccEdge}
    (hm : SuccStrictRankMonotone rank edges) (u : Nat) :
    ¬ SuccReachable edges u u := by
  intro hr
  exact Nat.lt_irrefl (rank u) (succ_reachable_rank_lt hm hr)

theorem succ_dag_no_two_cycle {rank : Nat → Nat} {edges : List SuccEdge}
    (hm : SuccStrictRankMonotone rank edges) {u v : Nat}
    (huv : SuccReachable edges u v) (hvu : SuccReachable edges v u) : False := by
  have h1 := succ_reachable_rank_lt hm huv
  have h2 := succ_reachable_rank_lt hm hvu
  omega

/-- Node codes for the canonical 10-node `|S_s| = 3` internal-only successor DAG. -/
def succNodeReg : Nat := 0
def succNodeScope : Nat := 1
def succNodeCand : Nat := 2
def succNodeSpec : Nat := 3
def succNodeMigSpec : Nat := 4
def succNodeInputs : Nat := 5
def succNodeReceipt : Nat := 6
def succNodeEvidence : Nat := 7
def succNodeAdmission : Nat := 8
def succNodeRelease : Nat := 9

/-- Canonical topological rank function for `|S_s| = 3` internal nodes. -/
def succCanonicalRank : Nat → Nat
  | 0 => 0  -- DomainRegistryV1
  | 1 => 1  -- ProfileScopeV1
  | 2 => 2  -- BoundProfileManifestV1
  | 3 => 3  -- ResolvedProfileTransitionSpecV1
  | 4 => 4  -- MigrationSpecV1 (supplementary)
  | 5 => 4  -- CandidateBuildInputsV1 (supplementary)
  | 6 => 5  -- CandidateReleaseReceiptV1 (supplementary)
  | 7 => 5  -- SuccessorUpgradeEvidenceManifestV1
  | 8 => 6  -- ProfileAdmissionManifestV1
  | 9 => 7  -- AdmittedReleaseRecordV1
  | _ => 8

/-- Exact 23-edge canonical `|S_s| = 3` internal-only edge list. -/
def succCanonicalS3Edges : List SuccEdge :=
  [ ⟨1, 0⟩,
    ⟨2, 0⟩, ⟨2, 1⟩,
    ⟨3, 1⟩, ⟨3, 2⟩,
    ⟨4, 2⟩, ⟨4, 3⟩,
    ⟨5, 2⟩,
    ⟨6, 2⟩, ⟨6, 5⟩,
    ⟨7, 2⟩, ⟨7, 3⟩, ⟨7, 4⟩,
    ⟨8, 0⟩, ⟨8, 1⟩, ⟨8, 2⟩, ⟨8, 3⟩, ⟨8, 7⟩,
    ⟨9, 2⟩, ⟨9, 3⟩, ⟨9, 5⟩, ⟨9, 6⟩, ⟨9, 8⟩ ]

/-- Exact 15-edge minimal `|S_s| = 0` internal-only edge list. -/
def succMinimalS0Edges : List SuccEdge :=
  [ ⟨1, 0⟩,
    ⟨2, 0⟩, ⟨2, 1⟩,
    ⟨3, 1⟩, ⟨3, 2⟩,
    ⟨7, 2⟩, ⟨7, 3⟩,
    ⟨8, 0⟩, ⟨8, 1⟩, ⟨8, 2⟩, ⟨8, 3⟩, ⟨8, 7⟩,
    ⟨9, 2⟩, ⟨9, 3⟩, ⟨9, 8⟩ ]

/-- Boolean checker verifying strict rank monotonicity on every edge of a finite list. -/
def checkSuccEdgesStrict (rank : Nat → Nat) : List SuccEdge → Bool
  | [] => true
  | e :: rest => decide (rank e.dst < rank e.src) && checkSuccEdgesStrict rank rest

theorem checkSuccEdgesStrict_sound {rank : Nat → Nat} {edges : List SuccEdge}
    (hc : checkSuccEdgesStrict rank edges = true) :
    SuccStrictRankMonotone rank edges := by
  induction edges with
  | nil =>
    intro e he
    cases he
  | cons e rest ih =>
    simp [checkSuccEdgesStrict, Bool.and_eq_true] at hc
    intro x hx
    cases hx with
    | head => exact hc.1
    | tail _ ht => exact ih hc.2 x ht

theorem succ_s3_edge_count_eq_23 : succCanonicalS3Edges.length = 23 := rfl

theorem succ_s0_edge_count_eq_15 : succMinimalS0Edges.length = 15 := rfl

theorem succ_canonical_s3_dag_monotone :
    SuccStrictRankMonotone succCanonicalRank succCanonicalS3Edges :=
  checkSuccEdgesStrict_sound (by decide)

theorem succ_minimal_s0_dag_monotone :
    SuccStrictRankMonotone succCanonicalRank succMinimalS0Edges :=
  checkSuccEdgesStrict_sound (by decide)

theorem succ_evidence_precedes_admission_and_release :
    succCanonicalRank succNodeEvidence < succCanonicalRank succNodeAdmission ∧
    succCanonicalRank succNodeAdmission < succCanonicalRank succNodeRelease := by
  decide

theorem succ_admission_cannot_precede_evidence
    (h : succCanonicalRank succNodeAdmission < succCanonicalRank succNodeEvidence) : False := by
  simp [succCanonicalRank, succNodeAdmission, succNodeEvidence] at h

/-! ## Section 3: Non-Serialized 3-Stage Acyclic Verifier & Purpose Disjointness -/

/-- Non-serialized verifier pipeline phases enforcing strict causal acyclicity. -/
inductive SuccessorStage where
  | stage1PredecessorBase
  | releaseCoreDerived
  | stage2ReleaseAuthority
  | bundleCompleted
  | stage3ActivationAuthority
  | dedicatedMigrationExec
deriving DecidableEq, Repr

/-- Strictly increasing stage ordinal for `SUCCESSOR-ACTIVATION-1`. -/
def stageOrdinal : SuccessorStage → Nat
  | .stage1PredecessorBase => 0
  | .releaseCoreDerived => 1
  | .stage2ReleaseAuthority => 2
  | .bundleCompleted => 3
  | .stage3ActivationAuthority => 4
  | .dedicatedMigrationExec => 5

theorem stage2_after_release_core_before_bundle :
    stageOrdinal .releaseCoreDerived < stageOrdinal .stage2ReleaseAuthority ∧
    stageOrdinal .stage2ReleaseAuthority < stageOrdinal .bundleCompleted := by
  decide

theorem stage3_strictly_after_bundle_before_migration :
    stageOrdinal .bundleCompleted < stageOrdinal .stage3ActivationAuthority ∧
    stageOrdinal .stage3ActivationAuthority < stageOrdinal .dedicatedMigrationExec := by
  decide

theorem activation_stage_cannot_feed_into_bundle :
    ¬ (stageOrdinal .stage3ActivationAuthority ≤ stageOrdinal .bundleCompleted) := by
  decide

/-- Distinct live parent-authority certificate purposes mirroring `ParentAuthorityPurposeV1`. -/
inductive SuccAuthorityPurpose where
  | releaseAuthority
  | activationAuthority
deriving DecidableEq, Repr

/-- Abstract live parent-authority certificate admission record for Stage 2 / Stage 3. -/
structure LiveCertWitness where
  parentCoreId : Nat
  govRootId : Nat
  purpose : SuccAuthorityPurpose
  policyId : Nat
  consumerDigest : Nat
  fromReplayArchive : Bool
  completeOccurrenceCoverage : Bool
deriving DecidableEq, Repr

/-- Soundness predicate for paired Stage 2 (`Release`) and Stage 3 (`Activation`) live certs. -/
structure ValidTwoPurposeAuthority (rel act : LiveCertWitness)
    (releaseCoreId bundleId : Nat) : Prop where
  sameParentCore : rel.parentCoreId = act.parentCoreId
  sameGovRoot : rel.govRootId = act.govRootId
  relPurpose : rel.purpose = .releaseAuthority
  actPurpose : act.purpose = .activationAuthority
  distinctPolicies : rel.policyId ≠ act.policyId
  relConsumer : rel.consumerDigest = releaseCoreId
  actConsumer : act.consumerDigest = bundleId
  distinctConsumers : releaseCoreId ≠ bundleId
  relNotReplay : rel.fromReplayArchive = false
  actNotReplay : act.fromReplayArchive = false
  relCompleteOcc : rel.completeOccurrenceCoverage = true
  actCompleteOcc : act.completeOccurrenceCoverage = true

theorem two_purpose_certs_strictly_distinct (rel act : LiveCertWitness)
    (rcId bId : Nat) (hv : ValidTwoPurposeAuthority rel act rcId bId) :
    rel ≠ act := by
  intro heq
  have hp1 := hv.relPurpose
  have hp2 := hv.actPurpose
  rw [heq] at hp1
  rw [hp1] at hp2
  cases hp2

theorem single_purpose_cert_cannot_satisfy_both (c : LiveCertWitness)
    (rcId bId : Nat) (hv : ValidTwoPurposeAuthority c c rcId bId) : False :=
  two_purpose_certs_strictly_distinct c c rcId bId hv rfl

theorem replay_cert_rejected_in_stage2 (rel act : LiveCertWitness)
    (rcId bId : Nat) (hv : ValidTwoPurposeAuthority rel act rcId bId)
    (hr : rel.fromReplayArchive = true) : False := by
  have h := hv.relNotReplay
  rw [hr] at h
  cases h

theorem replay_cert_rejected_in_stage3 (rel act : LiveCertWitness)
    (rcId bId : Nat) (hv : ValidTwoPurposeAuthority rel act rcId bId)
    (ha : act.fromReplayArchive = true) : False := by
  have h := hv.actNotReplay
  rw [ha] at h
  cases h

theorem partial_occurrence_cert_rejected (rel act : LiveCertWitness)
    (rcId bId : Nat) (hv : ValidTwoPurposeAuthority rel act rcId bId)
    (hp : act.completeOccurrenceCoverage = false) : False := by
  have h := hv.actCompleteOcc
  rw [hp] at h
  cases h

/-! ## Section 4: Dedicated Migration Body Exclusivity -/

/-- Block body kind mirroring `Body = OrdinaryBodyV1 | DedicatedMigrationBodyV1`. -/
inductive SuccBodyKind where
  | ordinaryBody
  | dedicatedMigrationBody
deriving DecidableEq, Repr

/-- Abstract migration-block body witness mirroring `DedicatedMigrationBodyV1`. -/
structure MigrationBodyWitness where
  bodyKind : SuccBodyKind
  signedTxCount : Nat
  appSubtickCount : Nat
  migrationTransitionCount : Nat
  matchesClosureIds : Bool
deriving DecidableEq, Repr

/-- Exact `LAYER1_DESIGN_PAPER.md` §2 rule for `InPlaceMigrationTransitionV1`. -/
structure ValidDedicatedMigrationBody (b : MigrationBodyWitness) : Prop where
  isDedicated : b.bodyKind = .dedicatedMigrationBody
  emptyTxs : b.signedTxCount = 0
  emptySubticks : b.appSubtickCount = 0
  singleTransition : b.migrationTransitionCount = 1
  idsMatch : b.matchesClosureIds = true

theorem ordinary_body_rejected_for_migration (b : MigrationBodyWitness)
    (hv : ValidDedicatedMigrationBody b) (ho : b.bodyKind = .ordinaryBody) : False := by
  have hd := hv.isDedicated
  rw [ho] at hd
  cases hd

theorem nonempty_tx_rejected_for_migration (b : MigrationBodyWitness)
    (hv : ValidDedicatedMigrationBody b) (htx : b.signedTxCount > 0) : False := by
  have he := hv.emptyTxs
  omega

theorem zero_or_multi_migration_transitions_rejected (b : MigrationBodyWitness)
    (hv : ValidDedicatedMigrationBody b) (hm : b.migrationTransitionCount ≠ 1) : False := by
  exact hm hv.singleTransition

/-! ## Section 5: Powered Negative Controls -/

theorem neg_external_root_injected_unsat (h : SuccessorBundleHeader)
    (hv : ValidSuccessorBundleEquations h) (hext : h.externalRoots = 1) : False := by
  have hz := hv.externalZero
  omega

theorem neg_off_by_one_internal_count_unsat (h : SuccessorBundleHeader)
    (hv : ValidSuccessorBundleEquations h)
    (hbad : h.internalArtifacts = 8 + h.supplementaryArtifacts) : False := by
  have hi := hv.internalEq
  omega

theorem neg_duplicated_prior_bytes_unsat (h : SuccessorBundleHeader)
    (hv : ValidSuccessorBundleEquations h) (hdup : h.duplicatedPriorBytes > 0) : False := by
  have hp := hv.priorZero
  omega

theorem neg_swapped_purposes_unsat (rel act : LiveCertWitness)
    (rcId bId : Nat) (hv : ValidTwoPurposeAuthority rel act rcId bId)
    (hswap : rel.purpose = .activationAuthority) : False := by
  have hr := hv.relPurpose
  rw [hswap] at hr
  cases hr

theorem neg_different_parent_core_unsat (rel act : LiveCertWitness)
    (rcId bId : Nat) (hv : ValidTwoPurposeAuthority rel act rcId bId)
    (hdiff : rel.parentCoreId ≠ act.parentCoreId) : False :=
  hdiff hv.sameParentCore

end BitblockProof
