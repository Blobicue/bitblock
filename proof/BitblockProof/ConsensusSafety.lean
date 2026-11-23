namespace BitblockProof

/-!
`CONSENSUS-1`: Formal weighted BFT quorum-intersection, 2-phase HotStuff finality safety,
data-before-vote obligation, and live parent-authority certificate admission model corresponding to
`crates/research/src/consensus_1.rs` (`HotStuffTwoPhaseBftPosV1`, `QuorumCertificateV1`,
`ConsensusCommitCertificateV1`, `AcceptedConsensusNativeParentAuthorityCertificateV1`,
`VerifiedConsensusNativeAcceptedRecordOccurrenceHistoryStartEqualityV1`,
`LiveParentAuthorityRegistryV1`).

Establishes under `lake env lean --trust=0` with zero custom axioms (`Quot.sound` and `propext`
only):
- Weighted strict `> 2/3` quorum intersection (`3 * W(Q1) > 2 * W`, `3 * W(Q2) > 2 * W`,
  `3 * W(Byz) < W` implies `Q1 ∩ Q2` has strictly greater weight than `Byz` and contains at least
  one honest positive-weight validator);
- Full data-before-vote obligation (`hasFullBody = true`) extracted from every valid QC signer;
- Same-view QC uniqueness (`qc_unique_same_view_phase`, `commit_qc_unique_same_view`);
- Cross-view 2-phase HotStuff lock persistence (`no_prepare_qc_conflicting_after_commit`) and full
  deterministic finality safety (`hotstuff_two_phase_finality_safety`) by strong induction on view;
- Unique live parent-authority certificate admission (`admitLiveCert`) rejecting replay-sourced
  inputs, history-start mismatches, late-valid-suffix candidates, candidate-advanced rules, and
  duplicate `(parent_core_id, purpose, required_position)` slots while preserving slot uniqueness;
- Powered negative controls proving that relaxing `> 2/3` to `≥ 2/3` at `3 * F = W` admits
  conflicting quorums with zero honest overlap, and that a `3,334 bps` (`> 1/3`) single validator
  breaks the BFT fault bound.

This is an abstract mathematical model over Lean `Nat` and finite lists. It does not formally
verify `rustc`/LLVM compiled machine code, RFC 8032 Ed25519 cryptography, SHA-256 hashing, or
asynchronous network liveness.
-/

/-! ## Section 1: Weighted Validator Sets & Strict `> 2/3` Quorum Intersection -/

/-- One scheduled validator entry mirroring `ScheduledValidatorEntryV1` (`VALIDATOR-SCHEDULE-1`). -/
structure ValidatorEntry where
  /-- Unique validator identifier. -/
  id : Nat
  /-- Strictly positive stake weight for the active epoch. -/
  weight : Nat
deriving DecidableEq, Repr

/-- Total active validator weight `W` across an epoch validator schedule. -/
def weightSum : List ValidatorEntry → Nat
  | [] => 0
  | e :: rest => e.weight + weightSum rest

/-- Total weight of all entries satisfying predicate `p`. -/
def subsetWeight (p : Nat → Bool) : List ValidatorEntry → Nat
  | [] => 0
  | e :: rest => (if p e.id then e.weight else 0) + subsetWeight p rest

/-- Strict `> 2/3` weighted quorum predicate (`3 * signed_weight > 2 * total_weight`). -/
def IsStrictTwoThirdsQuorum (q : Nat → Bool) (vs : List ValidatorEntry) : Prop :=
  3 * subsetWeight q vs > 2 * weightSum vs

/-- Strict `< 1/3` Byzantine stake bound (`3 * byzantine_weight < total_weight`). -/
def UnderBftFaultBound (byz : Nat → Bool) (vs : List ValidatorEntry) : Prop :=
  3 * subsetWeight byz vs < weightSum vs

theorem subsetWeight_le_total (p : Nat → Bool) (vs : List ValidatorEntry) :
    subsetWeight p vs ≤ weightSum vs := by
  induction vs with
  | nil => simp [subsetWeight, weightSum]
  | cons e rest ih =>
    simp only [subsetWeight, weightSum]
    cases p e.id <;> simp <;> omega

theorem subsetWeight_mono (p1 p2 : Nat → Bool) (vs : List ValidatorEntry)
    (himp : ∀ v, p1 v = true → p2 v = true) :
    subsetWeight p1 vs ≤ subsetWeight p2 vs := by
  induction vs with
  | nil => simp [subsetWeight]
  | cons e rest ih =>
    simp only [subsetWeight]
    cases h1 : p1 e.id
    · cases p2 e.id <;> simp <;> omega
    · have h2 : p2 e.id = true := himp e.id h1
      simp [h2]
      omega

theorem subsetWeight_add_compl (p : Nat → Bool) (vs : List ValidatorEntry) :
    subsetWeight p vs + subsetWeight (fun v => !p v) vs = weightSum vs := by
  induction vs with
  | nil => simp [subsetWeight, weightSum]
  | cons e rest ih =>
    simp only [subsetWeight, weightSum]
    by_cases hp : p e.id = true <;> simp [hp] <;> omega

theorem subsetWeight_inter_add_union (q1 q2 : Nat → Bool) (vs : List ValidatorEntry) :
    subsetWeight (fun v => q1 v && q2 v) vs + subsetWeight (fun v => q1 v || q2 v) vs =
      subsetWeight q1 vs + subsetWeight q2 vs := by
  induction vs with
  | nil => simp [subsetWeight]
  | cons e rest ih =>
    simp only [subsetWeight]
    by_cases h1 : q1 e.id = true <;> by_cases h2 : q2 e.id = true <;> simp [h1, h2] <;> omega

theorem subsetWeight_split_by (p byz : Nat → Bool) (vs : List ValidatorEntry) :
    subsetWeight (fun v => p v && byz v) vs + subsetWeight (fun v => p v && !byz v) vs =
      subsetWeight p vs := by
  induction vs with
  | nil => simp [subsetWeight]
  | cons e rest ih =>
    simp only [subsetWeight]
    by_cases hp : p e.id = true <;> by_cases hb : byz e.id = true <;> simp [hp, hb] <;> omega

theorem subsetWeight_inter_ge (q1 q2 : Nat → Bool) (vs : List ValidatorEntry) :
    subsetWeight q1 vs + subsetWeight q2 vs ≤
      weightSum vs + subsetWeight (fun v => q1 v && q2 v) vs := by
  have hiu := subsetWeight_inter_add_union q1 q2 vs
  have hu := subsetWeight_le_total (fun v => q1 v || q2 v) vs
  omega

theorem quorum_intersection_exceeds_one_third (q1 q2 : Nat → Bool) (vs : List ValidatorEntry)
    (hq1 : IsStrictTwoThirdsQuorum q1 vs) (hq2 : IsStrictTwoThirdsQuorum q2 vs) :
    3 * subsetWeight (fun v => q1 v && q2 v) vs > weightSum vs := by
  unfold IsStrictTwoThirdsQuorum at hq1 hq2
  have hge := subsetWeight_inter_ge q1 q2 vs
  omega

theorem quorum_intersection_exceeds_byzantine (q1 q2 byz : Nat → Bool) (vs : List ValidatorEntry)
    (hq1 : IsStrictTwoThirdsQuorum q1 vs)
    (hq2 : IsStrictTwoThirdsQuorum q2 vs)
    (hb : UnderBftFaultBound byz vs) :
    subsetWeight (fun v => q1 v && q2 v) vs > subsetWeight byz vs := by
  have hinter := quorum_intersection_exceeds_one_third q1 q2 vs hq1 hq2
  unfold UnderBftFaultBound at hb
  omega

theorem quorum_intersection_honest_weight_pos (q1 q2 byz : Nat → Bool) (vs : List ValidatorEntry)
    (hq1 : IsStrictTwoThirdsQuorum q1 vs)
    (hq2 : IsStrictTwoThirdsQuorum q2 vs)
    (hb : UnderBftFaultBound byz vs) :
    subsetWeight (fun v => (q1 v && q2 v) && !byz v) vs > 0 := by
  have hgt := quorum_intersection_exceeds_byzantine q1 q2 byz vs hq1 hq2 hb
  have hsplit := subsetWeight_split_by (fun v => q1 v && q2 v) byz vs
  have hbyz :
      subsetWeight (fun v => (q1 v && q2 v) && byz v) vs ≤ subsetWeight byz vs := by
    apply subsetWeight_mono
    intro v hv
    cases hq : (q1 v && q2 v) <;> cases hbv : byz v <;> simp_all
  omega

theorem exists_mem_of_subsetWeight_pos (p : Nat → Bool) (vs : List ValidatorEntry)
    (hpos : subsetWeight p vs > 0) :
    ∃ e, e ∈ vs ∧ p e.id = true ∧ e.weight > 0 := by
  induction vs with
  | nil =>
    simp [subsetWeight] at hpos
  | cons e rest ih =>
    simp only [subsetWeight] at hpos
    cases hp : p e.id
    · simp [hp] at hpos
      obtain ⟨e', hmem, hp', hw'⟩ := ih hpos
      exact ⟨e', List.Mem.tail e hmem, hp', hw'⟩
    · simp [hp] at hpos
      by_cases hw : e.weight > 0
      · exact ⟨e, List.Mem.head rest, hp, hw⟩
      · have hrest : subsetWeight p rest > 0 := by omega
        obtain ⟨e', hmem, hp', hw'⟩ := ih hrest
        exact ⟨e', List.Mem.tail e hmem, hp', hw'⟩

/-- Core weighted BFT quorum-intersection theorem: any two strict `> 2/3` quorums under
`3 * F < W` share at least one honest positive-weight validator. -/
theorem quorum_intersection_has_honest_validator (q1 q2 byz : Nat → Bool)
    (vs : List ValidatorEntry)
    (hq1 : IsStrictTwoThirdsQuorum q1 vs)
    (hq2 : IsStrictTwoThirdsQuorum q2 vs)
    (hb : UnderBftFaultBound byz vs) :
    ∃ e, e ∈ vs ∧ q1 e.id = true ∧ q2 e.id = true ∧ byz e.id = false ∧ e.weight > 0 := by
  have hpos := quorum_intersection_honest_weight_pos q1 q2 byz vs hq1 hq2 hb
  obtain ⟨e, hmem, hp, hw⟩ := exists_mem_of_subsetWeight_pos _ vs hpos
  have h1 : q1 e.id = true := by
    cases hq1e : q1 e.id <;> simp_all
  have h2 : q2 e.id = true := by
    cases hq2e : q2 e.id <;> simp_all
  have h3 : byz e.id = false := by
    cases hbe : byz e.id <;> simp_all
  exact ⟨e, hmem, h1, h2, h3, hw⟩

/-! ## Section 2: 2-Phase HotStuff Lock & Deterministic Finality Safety -/

/-- Two-phase HotStuff BFT vote phase mirroring `ConsensusVotePhaseV1`. -/
inductive VotePhase where
  | prepare
  | commit
deriving DecidableEq, Repr

/-- Signed validator vote message with explicit full-body possession attestation. -/
structure VoteMsg where
  /-- Signing validator identifier (`vote_key` owner). -/
  voter : Nat
  /-- View number at the current height. -/
  view : Nat
  /-- Vote phase (`prepare` or `commit`). -/
  phase : VotePhase
  /-- Block digest / identifier voted for. -/
  blockId : Nat
  /-- Data-before-vote invariant: validator possessed and executed the complete block body. -/
  hasFullBody : Bool
deriving DecidableEq, Repr

/-- Weighted quorum certificate mirroring `QuorumCertificateV1`. -/
structure QuorumCert where
  /-- View number bound by the certificate. -/
  view : Nat
  /-- Phase (`prepare` for `PrepareQC`, `commit` for `CommitQC`). -/
  phase : VotePhase
  /-- Target block identifier. -/
  blockId : Nat
  /-- Signer membership predicate. -/
  signers : Nat → Bool

/-- A quorum certificate is valid iff its signers achieve strict `> 2/3` weight and every signing
validator in `vs` emitted a matching data-before-vote message in `votes`. -/
def ValidQC (vs : List ValidatorEntry) (votes : List VoteMsg) (qc : QuorumCert) : Prop :=
  IsStrictTwoThirdsQuorum qc.signers vs ∧
    ∀ e, e ∈ vs → qc.signers e.id = true →
      ∃ m, m ∈ votes ∧ m.voter = e.id ∧ m.view = qc.view ∧ m.phase = qc.phase ∧
        m.blockId = qc.blockId ∧ m.hasFullBody = true

/-- Honest validator safety invariant 1: an honest validator never equivocates within a `(view, phase)`. -/
def HonestUniqueVotePerViewPhase (byz : Nat → Bool) (votes : List VoteMsg) : Prop :=
  ∀ m1 m2, m1 ∈ votes → m2 ∈ votes → byz m1.voter = false →
    m1.voter = m2.voter → m1.view = m2.view → m1.phase = m2.phase →
    m1.blockId = m2.blockId

/-- Honest validator safety invariant 2 (2-phase HotStuff lock rule): after locking on `(v1, b1)`
and casting a `commit` vote at `v1`, an honest validator only casts a `prepare` vote for a
conflicting block `b2 ≠ b1` at a later view `v2 > v1` if unlocked by a valid justifying
`PrepareQC` at some strictly intermediate view `v1 < vj < v2` for a block distinct from `b1`. -/
def HonestLockSafe (byz : Nat → Bool) (vs : List ValidatorEntry) (votes : List VoteMsg) : Prop :=
  ∀ mCommit mPrep,
    mCommit ∈ votes → mPrep ∈ votes →
    byz mCommit.voter = false →
    mCommit.voter = mPrep.voter →
    mCommit.phase = .commit →
    mPrep.phase = .prepare →
    mCommit.view < mPrep.view →
    mPrep.blockId ≠ mCommit.blockId →
    ∃ jqc, ValidQC vs votes jqc ∧ jqc.phase = .prepare ∧
      mCommit.view < jqc.view ∧ jqc.view < mPrep.view ∧ jqc.blockId ≠ mCommit.blockId

/-- Two-phase HotStuff sequencing rule: a valid `CommitQC` at `(view, blockId)` requires a valid
same-view `PrepareQC` for the exact same `(view, blockId)`. -/
def CommitRequiresPrepareQC (vs : List ValidatorEntry) (votes : List VoteMsg) : Prop :=
  ∀ cqc, ValidQC vs votes cqc → cqc.phase = .commit →
    ∃ pqc, ValidQC vs votes pqc ∧ pqc.phase = .prepare ∧
      pqc.view = cqc.view ∧ pqc.blockId = cqc.blockId

theorem data_before_vote_from_valid_qc (vs : List ValidatorEntry) (votes : List VoteMsg)
    (qc : QuorumCert) (hqc : ValidQC vs votes qc)
    (e : ValidatorEntry) (hmem : e ∈ vs) (hsig : qc.signers e.id = true) :
    ∃ m, m ∈ votes ∧ m.voter = e.id ∧ m.hasFullBody = true := by
  obtain ⟨m, hm, hv, _, _, _, hbody⟩ := hqc.2 e hmem hsig
  exact ⟨m, hm, hv, hbody⟩

theorem qc_unique_same_view_phase (byz : Nat → Bool) (vs : List ValidatorEntry)
    (votes : List VoteMsg) (qc1 qc2 : QuorumCert)
    (hb : UnderBftFaultBound byz vs)
    (huniq : HonestUniqueVotePerViewPhase byz votes)
    (h1 : ValidQC vs votes qc1) (h2 : ValidQC vs votes qc2)
    (hview : qc1.view = qc2.view) (hphase : qc1.phase = qc2.phase) :
    qc1.blockId = qc2.blockId := by
  obtain ⟨e, hmem, hs1, hs2, hhonest, _⟩ :=
    quorum_intersection_has_honest_validator qc1.signers qc2.signers byz vs h1.1 h2.1 hb
  obtain ⟨m1, hm1, hv1, hvw1, hph1, hb1, _⟩ := h1.2 e hmem hs1
  obtain ⟨m2, hm2, hv2, hvw2, hph2, hb2, _⟩ := h2.2 e hmem hs2
  have heq := huniq m1 m2 hm1 hm2 (by rw [hv1]; exact hhonest)
    (by rw [hv1, hv2]) (by rw [hvw1, hvw2, hview]) (by rw [hph1, hph2, hphase])
  omega

theorem commit_qc_unique_same_view (byz : Nat → Bool) (vs : List ValidatorEntry)
    (votes : List VoteMsg) (c1 c2 : QuorumCert)
    (hb : UnderBftFaultBound byz vs)
    (huniq : HonestUniqueVotePerViewPhase byz votes)
    (h1 : ValidQC vs votes c1) (h2 : ValidQC vs votes c2)
    (hc1 : c1.phase = .commit) (hc2 : c2.phase = .commit)
    (hview : c1.view = c2.view) :
    c1.blockId = c2.blockId :=
  qc_unique_same_view_phase byz vs votes c1 c2 hb huniq h1 h2 hview (by rw [hc1, hc2])

/-- Strong-induction lock persistence theorem: once a block `b1` attains a valid `CommitQC` at view
`v1`, no conflicting block can attain a valid `PrepareQC` at any strictly higher view `v2 > v1`. -/
theorem no_prepare_qc_conflicting_after_commit (byz : Nat → Bool) (vs : List ValidatorEntry)
    (votes : List VoteMsg) (cqc : QuorumCert)
    (hb : UnderBftFaultBound byz vs)
    (hlock : HonestLockSafe byz vs votes)
    (hcqc : ValidQC vs votes cqc) (hcommit : cqc.phase = .commit) :
    ∀ v2, ∀ pqc, ValidQC vs votes pqc → pqc.phase = .prepare →
      pqc.view = v2 → cqc.view < v2 → pqc.blockId = cqc.blockId := by
  intro v2
  induction v2 using Nat.strongRecOn with
  | ind n ih =>
    intro pqc hpqc hprep hview hlt
    by_cases heq : pqc.blockId = cqc.blockId
    · exact heq
    · obtain ⟨e, hmem, hsc, hsp, hhonest, _⟩ :=
        quorum_intersection_has_honest_validator cqc.signers pqc.signers byz vs hcqc.1 hpqc.1 hb
      obtain ⟨mc, hmc, hvc, hvwc, hphc, hbc, _⟩ := hcqc.2 e hmem hsc
      obtain ⟨mp, hmp, hvp, hvwp, hphp, hbp, _⟩ := hpqc.2 e hmem hsp
      obtain ⟨jqc, hjvalid, hjprep, hjgt, hjlt, hjne⟩ := hlock mc mp hmc hmp
        (by rw [hvc]; exact hhonest)
        (by rw [hvc, hvp])
        (by rw [hphc, hcommit])
        (by rw [hphp, hprep])
        (by rw [hvwc, hvwp, hview]; exact hlt)
        (by rw [hbp, hbc]; exact heq)
      have hjview_lt : jqc.view < n := by rw [hvwp, hview] at hjlt; exact hjlt
      have hjview_gt : cqc.view < jqc.view := by rw [hvwc] at hjgt; exact hjgt
      have hjeq := ih jqc.view hjview_lt jqc hjvalid hjprep rfl hjview_gt
      rw [hbc] at hjne
      exact False.elim (hjne hjeq)

/-- Main 2-phase HotStuff deterministic-finality safety theorem (`HotStuffTwoPhaseBftPosV1`):
under `3 * F < W`, honest single-voting per `(view, phase)`, honest 2-phase locking, and
commit-requires-prepare sequencing, ANY two valid `CommitQC`s across ANY views (`=`, `<`, or `>`)
must finalize the exact same block. -/
theorem hotstuff_two_phase_finality_safety (byz : Nat → Bool) (vs : List ValidatorEntry)
    (votes : List VoteMsg) (c1 c2 : QuorumCert)
    (hb : UnderBftFaultBound byz vs)
    (huniq : HonestUniqueVotePerViewPhase byz votes)
    (hlock : HonestLockSafe byz vs votes)
    (hseq : CommitRequiresPrepareQC vs votes)
    (h1 : ValidQC vs votes c1) (h2 : ValidQC vs votes c2)
    (hc1 : c1.phase = .commit) (hc2 : c2.phase = .commit) :
    c1.blockId = c2.blockId := by
  by_cases heq : c1.view = c2.view
  · exact commit_qc_unique_same_view byz vs votes c1 c2 hb huniq h1 h2 hc1 hc2 heq
  · by_cases hlt : c1.view < c2.view
    · obtain ⟨p2, hp2, hprep2, hvw2, hb2⟩ := hseq c2 h2 hc2
      have hsame :=
        no_prepare_qc_conflicting_after_commit byz vs votes c1 hb hlock h1 hc1 c2.view p2 hp2
          hprep2 hvw2 hlt
      omega
    · have hgt : c2.view < c1.view := by omega
      obtain ⟨p1, hp1, hprep1, hvw1, hb1⟩ := hseq c1 h1 hc1
      have hsame :=
        no_prepare_qc_conflicting_after_commit byz vs votes c2 hb hlock h2 hc2 c1.view p1 hp1
          hprep1 hvw1 hgt
      omega

/-! ## Section 3: Unique Live Parent-Authority Certificate Admission -/

/-- Live parent-authority certificate purpose mirroring `ParentAuthorityPurposeV1`. -/
inductive ParentAuthorityPurpose where
  | releaseAuthority
  | activationAuthority
deriving DecidableEq, Repr

/-- Exact three-anchor history-start tuple checked by
`VerifiedConsensusNativeAcceptedRecordOccurrenceHistoryStartEqualityV1`. -/
structure HistoryStartTriple where
  /-- Resolver history start anchor. -/
  resolverStart : Nat
  /-- Evidence cutoff history start anchor. -/
  cutoffStart : Nat
  /-- Occurrence universe scope history start anchor. -/
  scopeStart : Nat
deriving DecidableEq, Repr

/-- Exact byte-equality predicate across all three history-start anchors. -/
def HistoryStartExactEqual (h : HistoryStartTriple) : Prop :=
  h.resolverStart = h.cutoffStart ∧ h.cutoffStart = h.scopeStart

/-- Unique live admission key `(parent_core_id, purpose, required_position)`. -/
structure AdmissionSlot where
  /-- Unique accepted parent proposal core digest. -/
  parentCoreId : Nat
  /-- Bound consumer purpose (`releaseAuthority` or `activationAuthority`). -/
  purpose : ParentAuthorityPurpose
  /-- Required consensus-native parent-authority position. -/
  requiredPos : Nat
deriving DecidableEq, Repr

/-- Candidate live consensus-native parent-authority certificate presented for admission. -/
structure LiveCertCandidate where
  /-- Unique live admission key. -/
  slot : AdmissionSlot
  /-- Canonical certificate identifier. -/
  certId : Nat
  /-- History-start equality triple. -/
  historyStart : HistoryStartTriple
  /-- Must be `false`: replay-archive reconstruction can never enter live admission. -/
  fromReplayArchive : Bool
  /-- Consensus-native evidence cutoff position. -/
  cutoffPos : Nat
  /-- Earliest unbounded valid occurrence position (must be `≤ cutoffPos`; blocks late-valid-suffix). -/
  earliestUnboundedValidPos : Nat
  /-- Candidate rule activation position (must be `≤ cutoffPos`; blocks candidate-advanced rules). -/
  candidateRulePos : Nat
deriving DecidableEq, Repr

/-- Semantic admissibility predicate enforced by `LiveParentAuthorityRegistryV1::admit`. -/
def AdmissibleLiveCert (c : LiveCertCandidate) : Prop :=
  HistoryStartExactEqual c.historyStart ∧
    c.fromReplayArchive = false ∧
    c.earliestUnboundedValidPos ≤ c.cutoffPos ∧
    c.candidateRulePos ≤ c.cutoffPos

/-- Registry uniqueness invariant: at most one admitted certificate ID per `(parent_core, purpose, pos)`. -/
def UniqueSlots : List (AdmissionSlot × Nat) → Prop
  | [] => True
  | (s, _) :: rest => (∀ id, (s, id) ∉ rest) ∧ UniqueSlots rest

/-- Fail-closed live parent-authority certificate admission function. -/
def admitLiveCert (reg : List (AdmissionSlot × Nat)) (c : LiveCertCandidate) :
    Option (List (AdmissionSlot × Nat)) :=
  if c.fromReplayArchive then
    none
  else if c.historyStart.resolverStart != c.historyStart.cutoffStart ||
          c.historyStart.cutoffStart != c.historyStart.scopeStart then
    none
  else if c.earliestUnboundedValidPos > c.cutoffPos then
    none
  else if c.candidateRulePos > c.cutoffPos then
    none
  else if reg.any (fun p => p.1 == c.slot) then
    none
  else
    some ((c.slot, c.certId) :: reg)

theorem admitLiveCert_rejects_replay_input (reg : List (AdmissionSlot × Nat))
    (c : LiveCertCandidate) (h : c.fromReplayArchive = true) :
    admitLiveCert reg c = none := by
  simp [admitLiveCert, h]

theorem admitLiveCert_rejects_history_start_mismatch (reg : List (AdmissionSlot × Nat))
    (c : LiveCertCandidate) (h : ¬ HistoryStartExactEqual c.historyStart) :
    admitLiveCert reg c = none := by
  unfold HistoryStartExactEqual at h
  unfold admitLiveCert
  by_cases hr : c.fromReplayArchive = true
  · simp [hr]
  · simp only [hr, Bool.false_eq_true, ↓reduceIte]
    have hne : (c.historyStart.resolverStart != c.historyStart.cutoffStart ||
                c.historyStart.cutoffStart != c.historyStart.scopeStart) = true := by
      by_cases h1 : c.historyStart.resolverStart = c.historyStart.cutoffStart
      · have h2 : c.historyStart.cutoffStart ≠ c.historyStart.scopeStart := fun heq => h ⟨h1, heq⟩
        simp [bne_iff_ne, h2]
      · simp [bne_iff_ne, h1]
    simp [hne]

theorem admitLiveCert_rejects_late_valid_suffix (reg : List (AdmissionSlot × Nat))
    (c : LiveCertCandidate) (h : c.cutoffPos < c.earliestUnboundedValidPos) :
    admitLiveCert reg c = none := by
  have hgt : c.earliestUnboundedValidPos > c.cutoffPos := h
  simp [admitLiveCert, hgt]

theorem admitLiveCert_rejects_candidate_advanced_rule (reg : List (AdmissionSlot × Nat))
    (c : LiveCertCandidate) (h : c.cutoffPos < c.candidateRulePos) :
    admitLiveCert reg c = none := by
  have hgt : c.candidateRulePos > c.cutoffPos := h
  simp [admitLiveCert, hgt]

theorem admitLiveCert_rejects_duplicate_slot (reg : List (AdmissionSlot × Nat))
    (c : LiveCertCandidate) (existingId : Nat) (hmem : (c.slot, existingId) ∈ reg) :
    admitLiveCert reg c = none := by
  unfold admitLiveCert
  split
  · rfl
  · split
    · rfl
    · split
      · rfl
      · split
        · rfl
        · have hany : reg.any (fun p => p.1 == c.slot) = true := by
            rw [List.any_eq_true]
            exact ⟨(c.slot, existingId), hmem, by simp⟩
          simp [hany]

theorem admitLiveCert_sound_and_preserves_unique (reg reg' : List (AdmissionSlot × Nat))
    (c : LiveCertCandidate) (hu : UniqueSlots reg)
    (hadmit : admitLiveCert reg c = some reg') :
    AdmissibleLiveCert c ∧ UniqueSlots reg' ∧ reg' = (c.slot, c.certId) :: reg := by
  unfold admitLiveCert at hadmit
  split at hadmit
  · cases hadmit
  · split at hadmit
    · cases hadmit
    · split at hadmit
      · cases hadmit
      · split at hadmit
        · cases hadmit
        · split at hadmit
          · cases hadmit
          · rename_i hnr hhs hle1 hle2 hnoany
            cases hadmit
            have hrf : c.fromReplayArchive = false := by
              cases h : c.fromReplayArchive <;> simp_all
            have hhs1 : c.historyStart.resolverStart = c.historyStart.cutoffStart := by
              by_cases h1 : c.historyStart.resolverStart = c.historyStart.cutoffStart
              · exact h1
              · exfalso
                have : (c.historyStart.resolverStart != c.historyStart.cutoffStart ||
                        c.historyStart.cutoffStart != c.historyStart.scopeStart) = true := by
                  simp [bne_iff_ne, h1]
                exact hhs this
            have hhs2 : c.historyStart.cutoffStart = c.historyStart.scopeStart := by
              by_cases h2 : c.historyStart.cutoffStart = c.historyStart.scopeStart
              · exact h2
              · exfalso
                have : (c.historyStart.resolverStart != c.historyStart.cutoffStart ||
                        c.historyStart.cutoffStart != c.historyStart.scopeStart) = true := by
                  simp [bne_iff_ne, h2]
                exact hhs this
            have hnotmem : ∀ id, (c.slot, id) ∉ reg := by
              intro id hm
              apply hnoany
              rw [List.any_eq_true]
              exact ⟨(c.slot, id), hm, by simp⟩
            refine ⟨⟨⟨hhs1, hhs2⟩, hrf, by omega, by omega⟩, ⟨hnotmem, hu⟩, rfl⟩

/-! ## Section 4: Powered Negative Controls (Tightness of Strict `> 2/3` & `< 1/3` Bounds) -/

/-- Canonical 4-validator equal-stake schedule matching `CONSENSUS-1` devnet bootstrap (`W = 400`). -/
def fourEqualValidators : List ValidatorEntry :=
  [⟨1, 100⟩, ⟨2, 100⟩, ⟨3, 100⟩, ⟨4, 100⟩]

/-- 3-of-4 validators satisfy strict `> 2/3` quorum (`3 * 300 = 900 > 800 = 2 * 400`). -/
theorem fourEqualValidators_three_is_quorum :
    IsStrictTwoThirdsQuorum (fun id => id == 1 || id == 2 || id == 3) fourEqualValidators := by
  unfold IsStrictTwoThirdsQuorum
  decide

/-- 2-of-4 validators fail strict `> 2/3` quorum (`3 * 200 = 600 ≤ 800 = 2 * 400`). -/
theorem fourEqualValidators_two_not_quorum :
    ¬ IsStrictTwoThirdsQuorum (fun id => id == 1 || id == 2) fourEqualValidators := by
  unfold IsStrictTwoThirdsQuorum
  decide

/-- Relaxed `≥ 2/3` quorum threshold (powered negative-control mutant). -/
def IsRelaxedTwoThirdsQuorum (q : Nat → Bool) (vs : List ValidatorEntry) : Prop :=
  3 * subsetWeight q vs ≥ 2 * weightSum vs

/-- Relaxed `≤ 1/3` Byzantine fault bound (`3 * F ≤ W`, powered negative-control mutant). -/
def IsRelaxedByzantineBound (byz : Nat → Bool) (vs : List ValidatorEntry) : Prop :=
  3 * subsetWeight byz vs ≤ weightSum vs

/-- 3-validator equal-stake schedule (`W = 300`, `F = 100`, `3 * F = W`). -/
def threeEqualValidators : List ValidatorEntry :=
  [⟨1, 100⟩, ⟨2, 100⟩, ⟨3, 100⟩]

/-- Powered negative control: relaxing strict `> 2/3` to `≥ 2/3` with `3 * F = W` permits two
conflicting quorums (`{1, 2}` and `{2, 3}`) whose overlap `{2}` is 100% Byzantine (`0` honest
overlap), proving the strict inequality in `IsStrictTwoThirdsQuorum` and `UnderBftFaultBound` is
load-bearing. -/
theorem relaxed_two_thirds_admits_conflicting_quorums_without_honest_overlap :
    IsRelaxedTwoThirdsQuorum (fun id => id == 1 || id == 2) threeEqualValidators ∧
    IsRelaxedTwoThirdsQuorum (fun id => id == 2 || id == 3) threeEqualValidators ∧
    IsRelaxedByzantineBound (fun id => id == 2) threeEqualValidators ∧
    subsetWeight
      (fun id => ((id == 1 || id == 2) && (id == 2 || id == 3)) && !(id == 2))
      threeEqualValidators = 0 := by
  unfold IsRelaxedTwoThirdsQuorum IsRelaxedByzantineBound
  decide

/-- Powered negative control: a single validator holding `3,334 bps` (`> 3,333 bps` cap enforced by
`VALIDATOR-SCHEDULE-1`) violates `UnderBftFaultBound` (`3 * 3334 = 10002 ≥ 10000`). -/
theorem single_validator_over_one_third_breaks_bft_bound :
    ¬ UnderBftFaultBound (fun id => id == 1)
      [⟨1, 3334⟩, ⟨2, 2222⟩, ⟨3, 2222⟩, ⟨4, 2222⟩] := by
  unfold UnderBftFaultBound
  decide

end BitblockProof
