# Bitblock agent handoff

This is the short operational handoff for continuing Bitblock. It is intentionally smaller than
the transcript audit: use it to orient quickly, then use the linked documents for the full
semantic history.

## Current line of work (2026-11-21), read before the history below

- Active remote and branch: `Blobicue/bitblock`, branch `main` (see branch tip; do not pin a moving
  HEAD SHA here). The earlier `Gaia-0-0-1-1` remote, `codex/*` branches, and `chem/emergent-scale-2026-10`
  / `chem/render-scale-2026-11` waves are merged history preserved below.
- Current milestone: the complete local authority trunk (`ACCOUNT-1`, `LOCAL-BOUNDS-1`,
  `STORAGE-1`, `LEDGER-1`, `COMMIT-1`) plus `SCALE-2` (chunk-relative queries, hierarchical Hi-Z
  culling, ephemeral sub-pixel LOD, and bounded spatial cache) are `IMPLEMENTED` alongside bounded
  contracts/assays/proofs (`TRANSITION-SPEC-D1`, `COMMIT-R1`, `VERKLE-R1@feasibility`, `CODEC-1`,
  `REFINE-1@world`, `REFINE-1@accounting`, `REFINE-1@commitment`, `THREAT-D0`,
  `RELEASE-R1@framework`, `SCALE-1`, `COST-1@budget` + `COMMIT-1` measurements), the
  `chem/render-scale-2026-11` SIMD128 worker/octree/delta-render stack is merged on `main`, and the
  Loop 11 planning reconciliation aligns the mutable planning docs and powered checker controls.
- Agent onboarding / knowledge graph: `docs/AGENT_ONBOARDING.md`. It explains bbgraph `index/`
  for protocol, metrics, and gates, and GitNexus `.gitnexus/` (local only; MCP repo `bitblock`) for symbols, calls,
  flows, and blast radius.

## Chem session checklist (standing)

1. One active chem branch `chem/<topic>-YYYY-MM` off current main. Rebase often; never sit CONFLICTING.
2. Desktop = playtest artifact only (wasm/play/lab hashes + short receipt). Git is source of truth.
3. Two indexes: bbgraph for protocol/gates/metrics; GitNexus for call graph/blast radius. Cold start: HANDOFF → `docs/AGENT_ONBOARDING.md` → `AGENTS.md`.
4. Wave log lives on the emergent-scale Page (§5…). PR description links the latest § only.
5. Hard fence: soft DDA / bcc-voxel-demo / abi16 never go on a chem branch.
6. Merge when CI green + a named checkpoint (§5y-level), then cut the next chem branch. No unbounded PRs.
7. Self-playtest chem at http://127.0.0.1:8765 before Desktop sync.
8. Split commits: land-the-world vs docs/tooling separate when practical.

## Current repository state

- Remote: `https://github.com/Gaia-0-0-1-1/bitblock`
- Continuation branch: `main` — the only line of development. The
  `codex/bitblock-progress-2026-08-26` branch is merged into it, not deleted,
  because work is still in flight there. See `docs/TIMELINES.md`.
- `694650f` is the exact clean design-distillation checkpoint: a source-receipted compression of
  the downloaded state/Verkle/tick/proof conversation. It did not activate its three schematic
  research packages or change the capability DAG.
- `a996002` is the first-addition commit for `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT.md`. It freezes
  15 gaps found in one read-only sweep over `694650f`; its Git blob
  `73d53bc6f3ca007b5faca8c499660b207ff048e2` is pinned by the planning checker.
- `ea9294ba08b5e4bd339f82894ecb761461f5af13` is the one coordinated correction for DR-01 through
  DR-15. Its planning checker and whitespace gates passed, but Rust and Lean were not run for that
  documentation/checker-only commit.
- `fb5f5ca048144f84b0d600619202b3a6f400a02c` is the first-addition commit for
  `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP2.md`. It freezes the next read-only sweep over
  `ea9294b`, 11 successor findings, and audit blob
  `e8bd912f171d64c345b6fa71413bd9503e12f5f3`.
- `54926f52fa8ba9768e817ecb6139dcca41960e6a` is the coordinated correction for SG-01 through
  SG-11. It freezes the pre-ID hash, separates migration evidence/admission/activation, adds the
  availability carrier and Verkle feasibility edge, and powers the provenance/checker/CI controls.
  Its narrow planning and whitespace checks passed; broad Rust, Python-vector, Lean, and display
  suites were not run for that documentation/tooling checkpoint.
- `37a8a8a9a411462997f0f8c945f72c3e938bf7b8` is the first-addition commit for
  `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP3.md`. It freezes the complete successor sweep over
  `54926f5`, eight new findings, and audit blob
  `fbee935d27617f47f913617ca2d8e19aba618c6e`.
- `b23ddc95e5075c35d0dbf918b89f353fcdd40ddd` is the coordinated correction for Loop3/TG-01 through
  Loop3/TG-08. It binds the post-ID launch anchor, deterministic migration envelope/body, verifier work
  ordering, candidate-bound Verkle/current-scope evidence, and cross-platform planning provenance.
  Its Git tree is `3de2d45aa20b08318f1a1af4d93308daccf1f55e`. The narrow planning self-test and staged whitespace
  check passed; broad Rust, Python-vector, headless, Lean, network, performance, and display suites
  were not run for that documentation/tooling checkpoint.
- `8a5743595a0aa8ff13ede576f282b92540db5d9b` is the first-addition commit for
  `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP4.md`. It freezes the complete successor sweep over
  `b23ddc9`/tree `3de2d45`, eight new findings, and audit blob
  `350d1af6279d7f441f45fba2689481c0dbea79a5`.
- `2cb1cc02320d180ac9ee34bd103bd44459f128fa` is the coordinated correction for QG-01 through
  QG-08. Its Git tree is `60e9714c6d691f383fc75be03f6c9927c7665b84`. It binds the complete
  initial launch carrier, canonical available block context, total execution branches, fixed outer
  framing, profile-scoped conditional predicates, launch owner/gate, and the corrected review
  chronology. It remains documentation, checker, and checkout-policy work.
- `0d0122d9bf84664511db7588bb09d5c590a006f9` is the first-addition commit for
  `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP5.md`. It freezes the complete successor sweep over
  `2cb1cc0`/tree `60e9714`, eight material findings, and audit blob
  `519451b120b458735afaa59daa01aa23466e165e`. The coordinated PG-01 through PG-08 correction closes
  the proposed successor carrier, pre-work limits and lookup identity, embedded launch equations,
  detached release authorization, structured dependency/status metadata, and historical-audit
  provenance only at the documentation/checker level. It must not be mistaken for runtime, proof,
  cryptographic, consensus, or Layer 1 validation.
- `1821f37d1f042115a4ffb3bd50292885e581d812` is the coordinated correction for PG-01 through
  PG-08. Its Git tree is `9b8c08b10c611dd62ac8dbd7b4e46d1b0a02bf90`. It remains proposed-protocol,
  planning, checker, and checkout-policy work; no runtime or Layer 1 capability was activated.
- `80b58e0a40408e03f16e2d0c739c177e05c22903` is the first-addition commit for
  `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP6.md`. It freezes the complete successor sweep over
  `1821f37`/tree `9b8c08b`, four material findings, and audit blob
  `5374b160424b78bd516ffc598e9bbd3b29392227`. The coordinated RG-01 through RG-04 correction
  separates unique accepted proposal identity from plural evidence, makes failure of the frozen
  protocol identity hash terminal, types the sole external launch-genesis root, and checks every
  phase-status prose projection. It remains documentation/checker work, not implementation or
  security validation.

- The coordinated RG-01 through RG-04 correction is the exact input identified by
  `Loop7ProvenanceV1`. It remains proposed-protocol, planning, checker, and checkout-policy work,
  with no runtime or Layer 1 capability activated.

- `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP7.md` is identified only through the canonical
  `Loop7ProvenanceV1` projection below. Its LG-01 through LG-08 descendant correction covers
  proposal credentials, live authority-binding compromise, availability dependency and replay
  normalization, the actual launch DAG, successor internal-only equations, and structured
  frontier/handoff projections. It remains documentation/checker work, not implementation,
  testing, or security validation.

- The coordinated LG-01 through LG-08 correction is the exact input identified by
  `Loop8ProvenanceV1`. It remains proposed-protocol, planning, checker, and checkout-policy work,
  with no runtime or Layer 1 capability activated.

- `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP8.md` is identified only through the canonical
  `Loop8ProvenanceV1` projection below. Its EG-01 through EG-06 descendant correction covers
  successor authority purposes, replay archive/history proof, the unavailable current recovery
  path, exact core/collision guards, and exact provenance/count projections. It remains
  documentation/checker work, not implementation, testing, or security validation.

- The coordinated EG-01 through EG-06 correction is the exact input identified by
  `Loop9ProvenanceV1`. It remains proposed-protocol, planning, checker, and checkout-policy work,
  with no runtime or Layer 1 capability activated.

- `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP9.md` is identified only through the canonical
  `Loop9ProvenanceV1` projection below. Its frozen findings cover live certificate occurrence-
  universe completeness, occurrence chronology, current-paper provenance, compatible dependency-
  graph union safety, exact authority/archive schema guards, audit-cardinality derivation, semantic
  projection coverage, first-addition reachability, and mechanism-specific cycle controls. The
  coordinated descendant closes those bounded break witnesses in proposed schemas, projections, and
  powered checker controls. It does not establish implementation or Layer 1 security.

- The coordinated NG-01 through NG-09 correction is the exact input identified by
  `Loop10ProvenanceV1`. It remains proposed-protocol, planning, checker, and checkout-policy work,
  with no runtime or Layer 1 capability activated.

- `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP10.md` is identified only through the canonical
  `Loop10ProvenanceV1` projection below. Its frozen findings cover occurrence-range lower-bound
  authority, unprovable historical signing chronology, consumer-context exclusion, quoted-domain
  normalization, blank-line assignment boundaries, transitive envelope helpers, wrapped/negated
  semantic projections, first-addition operational/merge failure, implementation/proposal wording,
  convergence, chronology, and current-paper review provenance.

- The paper-owned Loop10/TG-01 through Loop10/TG-03, checker-owned Loop10/TG-04 through Loop10/TG-08,
  and cross-document Loop10/TG-09 through Loop10/TG-12 corrections are projected below. Their
  proposed-only and narrow-test boundaries remain explicit; no break witness is silently promoted
  into runtime or security evidence.

- `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP11.md` is identified only through the canonical
  `Loop11ProvenanceV1` projection below. Its UG-01 through UG-14 coordinated reconciliation propagates
  the profile-derived history-start equality and safe lower-bound invariant, names the cross-profile
  occurrence-universe owner/handoff, records `ACCOUNT-1` closure, closes lexical token fusion /
  HTML-comment & emphasis blindness / fail-open unsupported claim values / frozen Git-index checks in
  the planning checker, qualifies `Loop3/TG-*` vs `Loop10/TG-*`, corrects the deferred Verkle wording,
  labels historical pre-split test counts, and distinguishes routine automatic GitHub Actions CI from
  an attributed Layer-1 convergence/release pass. It remains proposed-protocol, planning, and checker
  work; no Layer 1 network execution exists in Rust.

- `docs/FUTURE_PLAN_AUDIT_LOOP2.md` predates the design distillation. Its 16-finding closure and
  validation boundary do not audit `694650f` or the later correction.
- `2a9e667` is the exact clean input to the second audit/fix loop. Its result is the first
  descendant commit that adds `docs/FUTURE_PLAN_AUDIT_LOOP2.md`; that audit also records why Verkle
  is a separate feasibility lane rather than a silently selected third trie.
- `1624068` is the exact clean input to the one-sweep future-plan audit. The result is the first
  descendant commit that adds `docs/FUTURE_PLAN_AUDIT.md`; the audit file intentionally avoids an
  impossible self-embedded commit hash. Its first-addition Git blob
  `8a02f4b7b00b440e14a35a098b87a7493fd887b4` is frozen by the retained checker.
- `7075c54` is the motive/capability-tree documentation checkpoint: it adds the project-intent
  owner, the dependency DAG, the MPT/SMT research plan, and transcript checkpoints 16–23.
- `88fcf55` is that checkpoint's clean input tip and preserves the concurrent-paper provenance
  note.
- `018ca52` is an intermediate shared-worktree bank of the in-progress checkpoint-0.3 paper;
  do not treat it as the reviewed result.
- The hardened checkpoint-0.3 Layer 1 paper is pinned at `034b7b2`; its second-loop audit result is
  at `9eace50`, where `docs/LAYER1_DESIGN_AUDIT.md` had blob
  `ee2075547d7c7895c7d0ebaabfc64d1e09f5ba3a`. Later provenance and F-12 errata produced the current
  canonical corrected reading copy, blob `124e066b418a61279abf90c1b1b2c2cc0604ef2a`, which is frozen
  from the fifth-successor correction onward. Do not describe that corrected copy as retrospectively
  immutable or as a review of later paper revisions.
- The branch contains the full non-build local checkpoint plus the semantic and transcript
  handoffs. Generated `target`, Lean `.lake`, and Python cache files are intentionally absent.
- The original non-Git working folder was preserved separately; a new agent should clone the
  remote branch instead of assuming that folder has Git metadata.

### Loop7ProvenanceV1

| Provenance field | Canonical value |
| --- | --- |
| `audited_input_commit` | `62afdd24a8525704f5949584859bd7cbad300d4d` |
| `audited_input_tree` | `ee346fb3aa9bc54591f851dcb482fd92be9da84a` |
| `addition_commit` | `78d5494f930c7412c2d60162b44c0b72050f9f22` |
| `addition_tree` | `07aa851e422a5da9453d4409f91b62ad9442a7d5` |
| `audit_git_blob` | `dfff19eddc3ab2d4d36f8b7f4e38c296c26f10c2` |
| `material_finding_count_u32` | `8` |

### Loop8ProvenanceV1

| Provenance field | Canonical value |
| --- | --- |
| `audited_input_commit` | `e3650eb5d4ea390ac0023cb67325bae3c5351c60` |
| `audited_input_tree` | `91378e4fe123a96298d73905ea2b1b3974137791` |
| `addition_commit` | `b9d441097c6c4dce013a3dad71cefb7ec2d5f0bf` |
| `addition_tree` | `916f7c22304d969d809f2f01f5032602a1a81b3b` |
| `audit_git_blob` | `b247d7585a9e4be06558b0076511fa1064252049` |
| `material_finding_count_u32` | `6` |

### Loop9ProvenanceV1

| Provenance field | Canonical value |
| --- | --- |
| `audited_input_commit` | `e6baa331f89d8b367d97ae218ad8506c103eb82c` |
| `audited_input_tree` | `044f210a7ae854a9416573a8f326d414da3ad4b5` |
| `addition_commit` | `bf62cb4848eeb3481eb413112cd33c06a63f1451` |
| `addition_tree` | `8f4c14e4926a5d5bcf77147cbfe724e55e234492` |
| `audit_git_blob` | `0fa04587f3715c22f3b39017583e32000d8ba871` |
| `material_finding_count_u32` | `9` |

### Loop10ProvenanceV1

| Provenance field | Canonical value |
| --- | --- |
| `audited_input_commit` | `c6474e97a65fccee6c02ff07ad7a46bb516b7f9b` |
| `audited_input_tree` | `29c72d3a26a72213e399b8846f8b6c24cdaeeed8` |
| `addition_commit` | `1f7ec0f2ca094414264f5172a4f07fd18a8dc3f3` |
| `addition_tree` | `462120e5e33b1e45932e3891ce7e5c43b9a35825` |
| `audit_git_blob` | `eb730338c6ec00c0285cc3ee7d8245f2c1ea2ebe` |
| `material_finding_count_u32` | `12` |

### Loop11ProvenanceV1

| Provenance field | Canonical value |
| --- | --- |
| `audited_input_commit` | `3d7164ce3e6a81eec724aff18e0eeaf133e48a10` |
| `audited_input_tree` | `b71509263f77d31e00b735baaa83f71b3d931846` |
| `addition_commit` | `e5a8bf78c5231cc1f849626448620dd3933a97de` |
| `addition_tree` | `926f15e952c311cd2c0c0511fabe140d790fd433` |
| `audit_git_blob` | `fe6578e10320fd954a5d8ac3d5a386634f734388` |
| `material_finding_count_u32` | `14` |

Start from a fresh clone or an existing checkout with:

```powershell
git fetch origin
git switch main
git status --short --branch
```

Read in this order:

1. `docs/HANDOFF.md`
2. `docs/PROJECT_INTENT.md`
3. `docs/CAPABILITY_TREE.md`
4. `docs/DESIGN_RESEARCH_DISTILLATION.md`
5. `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT.md`
6. `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP2.md`
7. `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP3.md`
8. `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP4.md`
9. `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP5.md`
10. `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP6.md`
11. `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP7.md`
12. `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP8.md`
13. `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP9.md`
14. `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP10.md`
15. `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP11.md`
16. `docs/FUTURE_PLAN_AUDIT_LOOP2.md`
17. `docs/FUTURE_PLAN_AUDIT.md`
18. `docs/SEMANTICS.md`
19. `docs/TRANSCRIPT_AUDIT.md`
20. `docs/AUTHENTICATED_STATE_PLAN.md`
21. `docs/LAYER1_DESIGN_PAPER.md`
22. `docs/LAYER1_DESIGN_AUDIT.md`
23. `README.md`
24. `docs/AUDIT_HISTORY.md`
25. `docs/PROTOCOL.md`
26. `docs/TIMELINES.md`

`AGENTS.md` describes the generated index over this tree. Rather than
reading the tree, query it:

```powershell
python tools/bbgraph.py ask "canonical framebuffer bytes"
python tools/bbgraph.py node frame
python tools/bbgraph.py dag
```

Stdlib Python only, no network, no build. Every line of output cites a path.

## Current proposed design delta during the tenth correction

The bullets below preserve the ninth-correction base and project only stabilized parts of the
coordinated Loop 10 descendant. They describe proposed protocol, planning, and checker surfaces
only. The frozen tenth sweep was nonzero, so diminishing returns has not been reached.

- `AcceptedProposalCoreV1` is now the one byte-exact signature-free authority/header/availability
  preimage under `accepted_parent_lookup_id`. Each `VerifiedProposalCredentialEntryV1` has a
  `TypedProposalCredentialIdV1`; the bounded `AcceptedProposalCredentialSetV1` is sorted/deduplicated
  by that ID. At least one credential must qualify, but multiplicity adds no weight. Consensus and
  availability evidence remain a separate plural normalized set feeding the purpose-specific
  monotone parent-eligibility function. `AcceptedProposalCoreFieldManifestV1` and its exclusions
  manifest make that exact field boundary checker-visible. `AcceptedParentCollisionDispositionV1`
  keeps the two collision classes directional instead of relying on marker presence.

- `ConsensusEntailedAvailabilityV1` must name the consensus evidence in the same accepted-record
  wrapper, which is verified first and retained atomically through
  `VerifiedConsensusEntailedAvailabilityDeltaV1` and
  `SameWrapperConsensusDependencyWitnessV1`. Wrappers are bounded ingestion/archive carriers only.
  Independent replay additionally proves one shared `ReplayEvidenceViewV1` from ordered
  `ReplayEvidenceEventV1` values, an authorized `ReplayEvidenceArchiveCheckpointV1`, inclusion and
  prefix-completeness proofs, and exact normalized-entry proofs. Each entry has one proof-free
  `NormalizedAcceptedAuthorityEntryCoreV1`; its ID is derived from that core and the view root from
  sorted unique `(lookup_id, entry_id)` pairs. View/proof fields cannot define their own commitment,
  and retained subsets require a full-core `ReplayEvidenceRetentionProjectionProofV1`. A
  `SameWrapperAcceptedRecordOccurrenceWitnessV1` binds the claimed consensus/assessment pair to one
  committed wrapper occurrence instead of synthesizing it from separate carriers.

- The parent consensus state atomically inserts each accepted wrapper into an authenticated ordered
  occurrence universe. `ConsensusNativeAcceptedRecordOccurrenceReceiptV1` binds its exact live
  position and bounded outer bytes. The admitted parent profile, occurrence-index policy, and
  immutable start rule resolve one inclusive
  `ConsensusNativeAcceptedRecordOccurrenceHistoryStartV1`; that exact object is serialized in the
  cutoff and scope, and `VerifiedConsensusNativeAcceptedRecordOccurrenceHistoryStartEqualityV1`
  must pass before range verification. An internally valid later-start suffix therefore rejects
  before it can hide an earlier conflict. The parent-profile resolver fixes the cutoff rule; exact
  `ConsensusNativeParentAuthorityPositionV1` plus parent consensus history derive the concrete
  `ConsensusNativeParentAuthorityEvidenceCutoffV1`, which consensus authenticates.
  `CompleteConsensusNativeAcceptedRecordOccurrenceUniverseV1` binds the exact inclusive history-
  start/cutoff scope, range-derived receipt set, and authenticated lower-boundary exclusion, gap-free
  range, and upper-boundary exclusion proof. Missing whole wrappers or conflict evidence therefore
  cannot be hidden by a candidate-selected subset. Replay may archive byte-identical bytes later only
  as recovery of an already positioned receipt; archive order is custody order and never live
  authority. Archive bytes absent from the pre-admission universe cannot justify a certificate.

- Capability ownership is acyclic: `CONSENSUS-1@base` owns live certificate issuance/admission;
  `SYNC-1@base` transports and retains bounded opaque bytes only; `PD-06` makes
  `SYNC-1@replay-authority` post-consensus; and `PD-07` makes `TESTNET-1@base` consume that replay
  phase. Replay never feeds live authority backward into consensus.

- The checker constructs one compatible inherited-base/phase/conditional role union and powers a
  matched cross-class cycle. It exactly guards the named live-certificate/replay assignment and hash-
  context chain, derives current audit cardinalities from contiguous finding IDs in frozen tables,
  scans every mutable planning authority for malformed semantic projections, proves first-addition
  commits reach the explicit checked lineage, and requires the base-cycle control to reach the named
  cycle mechanism. The latest checker correction makes the exact-assignment normalizer preserve every quoted literal byte,
  continues an assignment across blank lines until the next top-level boundary, and rejects orphan
  variant, field, or hash tails. It exact-guards `EnvelopeOwnerProfileContextV1`,
  `EnvelopeResolutionV1`, `EnvelopeHashV1`, and `EnvelopeCommitOrderedV1`, including their declared
  transitive helper graph and explicit abstract-primitive boundary. Semantic projection scanning now
  normalizes intra-paragraph whitespace and recognizes numeric/word counts plus `zero`, `no`, and
  `none`, including wrapped and reverse-order forms. First-addition proof uses a fail-closed Git tree
  query, inspects every parent for prior path presence, and separately requires the addition commit
  to reach the explicit checked lineage. These remain narrow checker controls; they do not prove
  arbitrary prose, semantic dependency completeness, protocol security, or implementation
  conformance.

- The mutable paper is now labeled as the current proposed descendant, links the complete successor-
  audit chronology, preserves the pinned checkpoint-0.3 paper and audit as historical, and scopes its
  old checkpoint-obligations table only to that historical artifact.

- `COMMIT-1@base` is `IMPLEMENTED` ([`COMMIT_1_RECEIPT.md`](COMMIT_1_RECEIPT.md)), while
  `COMMIT-R1@base` ([`COMMIT_R1_DECISION.md`](COMMIT_R1_DECISION.md) /
  [`commit-r1-assay-v1.json`](commit-r1-assay-v1.json), selecting Candidate A fixed-key binary
  Patricia for `WorldRootV1`), `CODEC-1@base` (`local-v1`), `SCALE-1@base`
  ([`SCALE_1_ASSAY.md`](SCALE_1_ASSAY.md)), `TRANSITION-SPEC-D1@base`
  ([`TRANSITION_SPEC_D1.md`](TRANSITION_SPEC_D1.md)), `REFINE-1@world`
  ([`REFINE_1_WORLD.md`](REFINE_1_WORLD.md)), `REFINE-1@accounting`
  ([`REFINE_1_ACCOUNTING.md`](REFINE_1_ACCOUNTING.md)), `REFINE-1@commitment`
  ([`REFINE_1_COMMITMENT.md`](REFINE_1_COMMITMENT.md)), `THREAT-D0@base`
  ([`THREAT_D0_ENVELOPE.md`](THREAT_D0_ENVELOPE.md)), and `RELEASE-R1@framework`
  ([`RELEASE_R1_FRAMEWORK.md`](RELEASE_R1_FRAMEWORK.md)) are `BOUNDED`, keeping live
  `World`/`world_tag` unwired.

- The transcript chronology now reaches frozen Loop 10 and this coordinated descendant. The
  checkpoint-0.3 audit owns only its historical corrected reading; the frozen successor chain owns
  later review chronology. A nonconverged successor sweep is frozen and corrected while broad tests
  stay gated; only a frozen convergence result can open them.

- `PreventiveProfileOwnedCryptographicMigrationV1` is allowed only while the exact last-safe parent
  and every active authority binding remain independently sound. An established collision, forgery,
  or equivalent failure in a live profile-owned authority role is
  `ProfileOwnedAuthorityBindingCompromiseV1`. The current profile reserves
  `PrecommittedIndependentRecoveryEnvelopeV1` only as `ReservedFutureUninhabitedTypeV1`; it has no
  constructor or acceptance branch, so
  `CurrentEstablishedAuthorityCompromiseDispositionV1` is exactly
  `SafeHaltAndSeparatelyTrustedNewGenesisV1`. `FutureIndependentRecoveryExtensionV1` is an
  uninhabited future-conditional lane until its complete carrier, authority, precommitment,
  last-safe selection, transition, availability, and tests are frozen. `ProtocolIdentityHashCompromiseV1`
  remains the terminal pre-profile identity case.

- The already outer-carried `GenesisPackageV1` remains the sole external launch artifact and is
  counted once, but `LaunchClosureExternalGenesisDependencyNodeV1` now has outgoing dependency refs
  and a nonzero checked rank in one combined DAG.
  Actual edges are scope→registry, genesis→scope/registry, and candidate→genesis; a valid prefix is
  registry rank 0, scope rank 1, genesis rank 2, candidate rank 3. Internal-to-internal, internal-to-
  external, and external-to-internal edge counts are separate; reversed/missing edges, rank-zero
  genesis, duplicate genesis bytes, or another external artifact reject.

- `SuccessorActivationBundleV1` is explicitly an internal-only seven-plus-supplementary closure
  relative to a pre-bundle semantic `VerifiedSuccessorPredecessorBaseContextV1`. After the release
  core ID exists, `VerifiedSuccessorReleaseAuthorityContextV1` admits release authority; its
  certificate must verify before the release sign document, ID, or authorization is accepted or
  used. Only after that release completes the bundle and yields its ID may
  `VerifiedSuccessorActivationAuthorityContextV1` admit activation authority. None is serialized or
  counted. The exact internal artifact/node equality is
  `internal_artifact_count_u32 = 7 + Length(S_s) = Length(D_s)`; its
  `SuccessorActivationInternalDependencyNodeV1` and
  `SuccessorActivationInternalDependencyRefV1` graph plus exact internal edge/byte/length fields
  exclude prior accepted-core/evidence, launch/genesis, old-state, framing, counter, and dependency-
  node bytes. Internal dependency IDs cannot name external context, while every predecessor/launch/
  genesis/old-root/governance projection must match that unique base context. The base context carries
  separate `verified_release_parent_eligibility` and `verified_activation_parent_eligibility`
  results. They must resolve the same unique parent core and governance source, but retain distinct
  purpose, policy, predicate, status, and evidence; the staged release and activation contexts bind
  their separate consuming refs and certificate admissions without a release-core/bundle-ID cycle.

- Release and activation chronology is only mandatory verifier acceptance/use order: the complete
  purpose-specific certificate must verify before the corresponding document, ID, or authorization
  is accepted or used. The proposal does not claim physical signature-production time without a
  separately authenticated authorization position; byte-identical artifacts with opposite alleged
  signing histories therefore receive the same disposition.

- `VerifiedParentAuthorityCertificateConsumerContextFieldGuardV1` recursively rejects every current
  certificate and consumer-chain semantic role named by
  `ParentAuthorityCertificateConsumerContextCurrentChainForbiddenFieldManifestV1`. Its sole
  authorization-bearing exception is the exact prior migration path in
  `EnvelopeMigrationConsumerContextPriorAuthorizationAllowedPathManifestV1`, and only after that
  distinct activation-purpose authorization has independently verified. It grants no exception to
  a current envelope-certificate field.

- Retained Loop 5 rules still require limits-before-work shallow framing, an authorization-free
  `AdmittedReleaseCoreV1` plus detached release authorization, and an availability-covered successor
  carrier verified before migration execution. These corrections do not weaken those boundaries.

- The capability tree's current unlock frontier is a structured exact projection of authoritative
  `PS-NNN:NODE@phase=READY` rows. Its canonical `PlanningCheckerSummaryV1` projection is derived from
  the parsed graph/inventories, the current audit-provenance tables are derived from Git, and this handoff's
  path-only read order is checked against the canonical planning inventory. Phase/status prose
  remains projection-only; none of these checker surfaces admits a locked node.

These coordinated corrections are specification, planning, attribute-policy, and checker work only.
No proposal-
credential/evidence index, atomic availability dependency store, evidence archive/checkpoint,
purpose-scoped historical replay, authority-compromise safe halt or recovery ceremony, combined launch-DAG verifier, successor
context/closure verifier, release authority, network intake, authenticated state, proposed network
cryptographic suite, or Layer 1 execution exists in Rust. The latest frozen sweep did not satisfy
diminishing returns. Broad implementation and proof tests remain gated on a later complete
convergence sweep.

## What is implemented

The current product surface is deliberately small: a free observer in a sparse binary coordinate
world. `(0, 0, 0)` is permanent bedrock; the observer starts at `(0, 0, -1)`; all other absent
coordinates are off. There are no entities, gravity, terrain, inventory, or autonomous block rules.

The fixed 60 Hz reducer consumes aggregate live-input flags/floats plus ordered edit attempts and
updates local camera state. Each pending edit passes through `admit_pending_edit`; only
`TickAttempt::executed_edit()` enters the camera-free `TickTransaction::apply_edit` path. The fold
then commits coordinate history, appends an aggregate tick outcome, and derives a frame only when
the surface is dirty. Resize, sidebar-toggle, and clear-motion still mutate local application
state before `fixed_tick`; presentation remains separate. Empty ticks still produce outcome
history and may reuse the last raster.

The observer has movement, look, logarithmic wheel dolly, placement, deletion, fit, crosshair, and
an F3-style metrics sidebar. Point sweeps prevent movement through occupied voxels; an edit that
would occupy the observer's containing cell is rejected with an explicit disposition. The live
default starts beside the origin at distance `1.0`; the exact one-pixel camera constructor exists
but is not the default startup path.

Supplied `InteractionTape` values are bounded replay inputs: they store contiguous tick records,
preserve action order and floating-point bit patterns, enforce canonical payload limits, and
receive a `#bb.intent.sha256...` tag. They are not evidence that the live application records
every accepted action. `Resize` is a canonical action, so sessions with different window
dimensions produce different tapes; replay of either complete tape remains deterministic.
`TickOutcome` aggregates non-edit controls, while coordinate history retains only executed exact
edits and omits aim misses, observer-overlap rejections, camera/control actions, and idle ticks.
`tools/simulate_sequence.py` converts JSON scenarios into tapes; the headless Rust path reports
per-tick outcome/world/history/camera/frame commitments. Ordinary stdout is not a canonical run
receipt, and the opt-in `LocalReplayEvidence` carrier is feature-gated, in-memory, and locally
self-verified. Neither path produces physical display evidence.

## File ownership map

- `src/world.rs`: coordinate type, bedrock, sparse chunk storage, bounds, on/off/toggle.
- `src/camera.rs`: observer pose, projection, dolly, fitting, and camera bit lanes.
- `src/tick.rs`: canonical tick input/outcome codecs and successor checks.
- `src/history.rs`, `src/frame.rs`, `src/frame_v2.rs`: local coordinate/frame commitments.
- `src/interaction.rs`: accepted-action tape parser, codec, limits, and content tag.
- `src/main.rs`: live event adapter, accepted-action reducer, fixed tick, headless replay, and
  presentation orchestration.
- `src/render.rs`: CPU voxel projection, depth ordering, palette audit, and crosshair overlay.
- `src/sidebar.rs`: metric snapshot, clipped compositor, and sidebar raster facts.
- `src/protocol.rs`: SHA-256 canonical content tags and schema/name bindings.
- `src/display*`, `src/presentation*`, `src/windows_*`, `src/physical_sensor.rs`: opt-in research
  carriers; they are not proof of monitor light, scanout, ownership, or consensus.
- `tools/`: independent vector/oracle scripts and the scenario runner.
- `proof/`: bounded Lean world/representation capsules and the separate pinned LadderLang
  transition capsule.
- `METRICS.md`, `docs/METRIC_EXHAUSTION_AUDIT.md`, and `docs/metrics/`: the metric catalog and the
  cheap runtime subset.
- `docs/PROJECT_INTENT.md`: durable motive chain and architectural separations.
- `docs/CAPABILITY_TREE.md`: capability dependencies, unlock gates, and current frontier.
- `docs/DESIGN_RESEARCH_DISTILLATION.md`: fingerprinted compression of the external design
  conversation into typed objects, retained rules, rejected claims, and notes-only candidates.
- `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT.md`: frozen input, 15 finite findings, correction map,
  source receipts, and residual boundary for the first successor audit.
- `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP2.md`: frozen `ea9294b` input, 11 finite findings,
  correction map, no-test boundary, and convergence criterion for the second successor audit.
- `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP3.md`: frozen `54926f5` input, eight finite findings,
  rejected candidates, one-sweep correction boundary, and the same zero-new-gap convergence rule.
- `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP4.md`: frozen `b23ddc9`/tree `3de2d45` input, eight
  finite findings, rejected candidates, one-sweep correction boundary, and the same finite-lens
  zero-new-gap convergence rule.
- `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP5.md`: frozen `2cb1cc0`/tree `60e9714` input, eight
  finite findings, challenged candidates, one-sweep correction boundary, and the same finite-lens
  zero-new-gap convergence rule.
- `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP6.md`: frozen `1821f37`/tree `9b8c08b` input, four
  finite findings, challenged candidates, one-sweep correction boundary, and the same finite-lens
  zero-new-gap convergence rule.

- `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP7.md`: exact identity and LG-set cardinality in
  `Loop7ProvenanceV1`; reproduced challenges, one-sweep correction boundary, and the same
  finite-lens convergence rule.

- `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP8.md`: exact identity and EG-set cardinality in
  `Loop8ProvenanceV1`; independently reproduced protocol/replay/checker challenges, one-sweep
  correction boundary, and the same finite-lens convergence rule.

- `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP9.md`: exact identity and NG-set cardinality in
  `Loop9ProvenanceV1`; frozen occurrence-universe/chronology, paper-provenance, combined-graph,
  exact-schema, provenance-projection, ancestry, and powered-control findings plus the unchanged
  finite-lens convergence rule.

- `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP10.md`: exact identity and Loop10/TG-set cardinality in
  `Loop10ProvenanceV1`; frozen lower-bound, historical-signing, consumer-context, parser/helper,
  semantic-projection, ancestry/topology, implementation-wording, convergence, chronology, and
  review-provenance findings plus the unchanged finite-lens convergence rule.

- `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP11.md`: exact identity and UG-set cardinality in
  `Loop11ProvenanceV1`; frozen history-start propagation, admitted lower-bound safety, `ACCOUNT-1`
  coordination, cross-profile occurrence-universe handoff, lexical token-fusion, Markdown context,
  fail-open claim value, frozen Git-index, convergence attribution, current-paper identity, retired
  mirror topology, verification inventory, `TG` namespace collision, and deferred-Verkle findings.

- `docs/LAYER1_DESIGN_AUDIT.md`: canonical corrected checkpoint-0.3 reading copy, frozen at current
  blob `124e066b418a61279abf90c1b1b2c2cc0604ef2a` from the fifth-successor correction onward; Git
  history retains the distinct original result blob.
- `docs/design-source-receipt-v1.json`: byte-frozen external-source and pre-audit-distillation
  receipt. The checker reproduces its historical Git-object fields; it still does not contain,
  recover, or authenticate the external raw source.
- `docs/AUTHENTICATED_STATE_PLAN.md`: unimplemented authenticated-map comparison and integration
  constraints.
- `docs/FUTURE_PLAN_AUDIT.md`: the finite audit/fix map for planning input `1624068`.
- `docs/FUTURE_PLAN_AUDIT_LOOP2.md`: the second finite audit/fix map for input `2a9e667`, including
  the authenticated-state, activation-graph, checker, provenance, and Verkle corrections.
- `.gitattributes`: forces LF checkout bytes for every path in the canonical frozen-text inventory,
  so its Git-blob guards are reproducible under Windows `core.autocrlf=true`. Exact inventory counts
  are projected once in `PlanningCheckerSummaryV1`.
- `tools/check_planning_docs.py`: retained dependency/Markdown checker with powered negative
  controls, structured unconditional-base and phase-specific dependency/status manifests, and exact
  equality for every canonical node-status projection. It derives and validates the exact values in
  `PlanningCheckerSummaryV1`, the current audit-provenance projections, the path-only authority read order,
  effective frozen-file EOL policy, frozen-audit/receipt guards, historical receipt-object
  reconstruction, exact signature-free core/collision projections, exact ordered Loop 8 authority
  schemas and variants with duplicate-assignment/purpose-collapse controls, the complete named Loop 9
  assignment/hash-context chain, the Loop 10 quote-preserving assignment parser and transitive
  envelope-helper closure, one compatible inherited-base/phase/conditional role union, frozen-table-
  derived audit cardinalities, paragraph-normalized semantic-shape projection guards across every
  mutable planning authority, fail-closed all-parent first-addition inspection plus ancestry to the
  explicit checked lineage, mechanism-specific cycle controls, capability ownership, and repository-
  contained tracked link/fragment checks. It
  does not validate semantic dependency completeness, arbitrary prose, external URLs, runtime
  semantics, or cryptographic security.

## Claims that are safe to make

- The world is sparse and lazily allocated in 16 x 16 x 16 chunks.
- Coordinates are signed 64-bit, so the address domain is large but finite.
- The origin is immutable and the observer/world separation is enforced by the current point
  sweep/edit guards.
- A replay of the same accepted tape under the same build and protocol limits has deterministic
  simulation semantics.
- Given the same coordinate prestate, receipt parent and local tick, and ordered admitted exact
  edits, coordinate reduction is independent of viewport context. Different resize-bearing tapes
  may still admit different streams.
- A content tag identifies canonical bytes under a named schema.
- The Lean/LadderLang artifacts establish only their declared abstract or finite scopes.

## Claims that must remain closed or qualified

- Do not call the coordinate domain mathematically infinite or the renderer infinitely scalable.
- Do not call a frame tag proof that a physical monitor displayed the pixels.
- Do not call local history Proof-of-History, Proof-of-Stake, consensus, currency, or Layer 1.
- Do not claim that a state root contains its values, that a recursive proof supplies discarded
  data, that a sequential clock selects the canonical fork, or that stateless verification makes
  the whole network storage-free.
- Do not implement the removed `TickCoreV1`/`TickEnvelopeV1` sketch. The Layer 1 paper owns the sole
  proposed network byte graph, and that graph remains unimplemented and non-authoritative.
- Do not verify a typed map proof directly under a composed execution root or use a touched-state
  witness as proof of an old/new commitment-profile migration.
- Do not present feature-gated `CoordinateEpisodeEvidence` or `LocalReplayEvidence` as default,
  durable, independently held authority.
- Do not claim the user's ternary hash is integrated; its source and contract have not been
  identified in this repository.
- Do not call the text edit language a self-executing programming language; no autonomous block
  transformation rules exist.
- Do not call local mint/burn provenance a cryptocurrency, ownership system, or balance ledger.
- Do not claim that `COMMIT-1` authenticated roots replace legacy `world_tag`, prove consensus
  finality, guarantee data availability, or wire Patricia traversals into rendering/collision;
  `COMMIT-1` (`WorldRootV1`, `ActiveTokenRootV1`, `EpisodeRootV1`, `CommitmentSetV1`) is a
  separately typed research-gated sidecar while legacy `world_tag` remains the default full-set
  world identity.
- Do not interpret the earlier zoomed-out multi-pixel observation as a hidden fractal without a
  captured state/frame artifact.
- Do not claim that the full 1,112-metric registry is collected on every frame; the sidebar shows a
  bounded cheap subset.

## Recommended next work order

The motive and skill-tree checkpoint has been selected and documented. For later work, use the
node gates in `docs/CAPABILITY_TREE.md`, make one semantic change at a time, and update
`docs/SEMANTICS.md`, `docs/TRANSCRIPT_AUDIT.md`, and this handoff when their truth changes.
The candidate labels in `docs/DESIGN_RESEARCH_DISTILLATION.md` are research notes, not new graph
nodes; promote one only through an explicit later capability-tree decision with dependencies,
artifact, gate, and non-claim.

1. Maintain the truthful verification boundary (`UG-09`): routine GitHub Actions CI (`rust (windows-latest)`,
   `rust (ubuntu-latest)` + `wasm32` `+simd128` V8 parity, `python vector tools`, `generated index`,
   `lean proof capsule`) executes on every push/PR and is recorded as automatic CI evidence, while
   attribution of a complete Layer-1 convergence/release evidence pass remains gated on a frozen
   zero-gap successor sweep and cannot replace physical display or external custody review.
2. The entire local authority trunk (`ACCOUNT-1`, `LOCAL-BOUNDS-1`, `STORAGE-1`, `LEDGER-1`, and
   `COMMIT-1` in [`COMMIT_1_RECEIPT.md`](COMMIT_1_RECEIPT.md)) is `IMPLEMENTED` (with
   `STORAGE-1@research-journals` kept `BOUNDED` and `COST-1@budget` `BOUNDED`), while
   `CODEC-1@base` (`local-v1`), `COMMIT-R1@base` ([`COMMIT_R1_DECISION.md`](COMMIT_R1_DECISION.md)),
   `TRANSITION-SPEC-D1@base` ([`TRANSITION_SPEC_D1.md`](TRANSITION_SPEC_D1.md)), `REFINE-1@world`
   ([`REFINE_1_WORLD.md`](REFINE_1_WORLD.md)), `REFINE-1@accounting`
   ([`REFINE_1_ACCOUNTING.md`](REFINE_1_ACCOUNTING.md)), `REFINE-1@commitment`
   ([`REFINE_1_COMMITMENT.md`](REFINE_1_COMMITMENT.md)), `THREAT-D0@base`
   ([`THREAT_D0_ENVELOPE.md`](THREAT_D0_ENVELOPE.md)), `RELEASE-R1@framework`
   ([`RELEASE_R1_FRAMEWORK.md`](RELEASE_R1_FRAMEWORK.md)), and `SCALE-1@base`
   ([`SCALE_1_ASSAY.md`](SCALE_1_ASSAY.md)) are `BOUNDED`. All non-VM `REFINE-1` phases are now
   verified under Lean 4 `--trust=0` (`REFINE-1@vm` remains `LOCKED` on `VM-1`).
3. `COMMIT-1` implements Candidate A (fixed-key binary Patricia selected in
   [`COMMIT_R1_DECISION.md`](COMMIT_R1_DECISION.md)) as a separately typed sidecar (`WorldRootV1`,
   `ActiveTokenRootV1`, `EpisodeRootV1`, `CommitmentSetV1`) with full-rebuild parity, independent
   projection verification, pin-epoch write-barrier GC, and batched checksummed `nodes/nodes.pack`
   filesystem durability without reinterpreting legacy `world_tag` or coupling trie walks into
   rendering or collision. All six pre-genesis design/rule `OPEN-DECISION` tuples are now resolved
   by human decision and backed by executable contracts (`PR #26` `ac503dc`): `CRYPTO-D1`
   ([`CRYPTO_D1_DECISION.md`](CRYPTO_D1_DECISION.md)), `IDENTITY-D1`
   ([`IDENTITY_D1_DECISION.md`](IDENTITY_D1_DECISION.md)), `ASSET-D1` & `VALIDATOR-ECON-D1`
   ([`ASSET_D1_DECISION.md`](ASSET_D1_DECISION.md)), `CONSENSUS-D1`
   ([`CONSENSUS_D1_DECISION.md`](CONSENSUS_D1_DECISION.md)), `GOVERNANCE-D1`
   ([`GOVERNANCE_D1_DECISION.md`](GOVERNANCE_D1_DECISION.md)), `RULE-1@rule-choice`
   ([`RULE_1_DECISION.md`](RULE_1_DECISION.md)), and `LANGUAGE-R1@grammar-choice`
   ([`LANGUAGE_R1_DECISION.md`](LANGUAGE_R1_DECISION.md)), unlocking `RULE-1@base=READY` and
   `PROFILE-SCOPE-D0@base=READY`.
4. `VERKLE-R1@feasibility` is now `BOUNDED` ([`VERKLE_R1_FEASIBILITY.md`](VERKLE_R1_FEASIBILITY.md)
   / [`verkle-r1-feasibility-v1.json`](verkle-r1-feasibility-v1.json)), pinning the self-excluding
   Bandersnatch IPA-256 profile descriptor, injective 32-byte coordinate embedding (`< 2^184 << r`),
   rejected KZG/RSA/PQ alternatives, 7-world cost & batch crossover comparison, and migration design
   while explicitly deferring prototype admission (`VERKLE-R1@candidate-scope=LOCKED`,
   `VERKLE-R1@prototype=LOCKED`). `COST-1` measurements for `COMMIT-1` (including the `3.18×` p50 /
   `3.88×` max batched `nodes.pack` speedup) are recorded in
   [`COST_1_COMMIT_1_MEASUREMENTS.md`](COST_1_COMMIT_1_MEASUREMENTS.md).
5. **Startup choice closed:** adjacent observer spawn at `distance = 1.0` (`(0, 0, -1)` facing
   bedrock) is formally retained as Bitblock's canonical default, preserving immediate visual
   legibility and all pinned camera/frame golden vectors.
6. `SCALE-2@base` is now `IMPLEMENTED` ([`SCALE_2_RECEIPT.md`](SCALE_2_RECEIPT.md) /
   [`scale-2-measurement-v1.json`](scale-2-measurement-v1.json)), closing both `SCALE-1` failure
   brackets (`HollowShell_58809`: `3.459 ms`, `HollowShell_149787`: `5.577 ms`, exact `±10^17` &
   `i64::MIN`/`MAX` chunk-relative precision, bounded `SpatialRenderCache`, and ephemeral sub-pixel
   LOD strictly outside `world_tag`; no “near-infinite performance” claim is licensed).
7. `HASH-SOURCE-R1` remains explicitly parked (`READY` / `NOT-STARTED`) with `SHA-256` frozen as
   the sole v1 hash suite (with versioned suite-ID agility). Only open `HASH-R1` if/when the user
   supplies a pinned source repository/commit; preserve the disqualification of Aethor TSH/PDH and
   IterHash.
8. `RULE-1@base` is now `IMPLEMENTED` ([`RULE_1_RECEIPT.md`](RULE_1_RECEIPT.md): opt-in synchronous
   3D Moore-neighborhood Life `B5/S45`, permanent bedrock `1`, fail-closed frontier caps, 48-way
   octahedral symmetry, independent Python stepper parity, `History` replay parity, and visually
   verified multi-tick PNG snapshots). Wave 3–6 receipts
   ([`PROFILE_SCOPE_D0_RECEIPT.md`](PROFILE_SCOPE_D0_RECEIPT.md),
   [`LANGUAGE_R1_RECEIPT.md`](LANGUAGE_R1_RECEIPT.md),
   [`THREAT_1_MODEL.md`](THREAT_1_MODEL.md), [`VM_D1_SPEC.md`](VM_D1_SPEC.md),
   [`BOUNDS_1_RECEIPT.md`](BOUNDS_1_RECEIPT.md) /
   [`bounds-1-measurement-v1.json`](bounds-1-measurement-v1.json),
   [`EMERGENCE_R1_RECEIPT.md`](EMERGENCE_R1_RECEIPT.md) /
   [`emergence-r1-assay-v1.json`](emergence-r1-assay-v1.json),
   [`RESOURCE_1_RECEIPT.md`](RESOURCE_1_RECEIPT.md) /
   [`resource-1-simulation-v1.json`](resource-1-simulation-v1.json),
   [`GENESIS_D1_RECEIPT.md`](GENESIS_D1_RECEIPT.md) /
   [`genesis-d1-package-v1.json`](genesis-d1-package-v1.json),
   [`PROTOCOL_ID_1_RECEIPT.md`](PROTOCOL_ID_1_RECEIPT.md),
   [`VM_1_RECEIPT.md`](VM_1_RECEIPT.md) /
   [`vm-1-benchmark-v1.json`](vm-1-benchmark-v1.json),
   [`REFINE_1_VM.md`](REFINE_1_VM.md),
   [`CRYPTO_1_RECEIPT.md`](CRYPTO_1_RECEIPT.md) /
   [`crypto-1-audit-v1.json`](crypto-1-audit-v1.json), and
   [`PROFILE_D1_RECEIPT.md`](PROFILE_D1_RECEIPT.md) /
   [`profile-d1-manifest-v1.json`](profile-d1-manifest-v1.json),
   [`IDENTITY_1_RECEIPT.md`](IDENTITY_1_RECEIPT.md),
   [`TRANSITION_SPEC_1_RECEIPT.md`](TRANSITION_SPEC_1_RECEIPT.md),
   [`AUTH_COMMIT_1_RECEIPT.md`](AUTH_COMMIT_1_RECEIPT.md),
   [`ASSET_1_RECEIPT.md`](ASSET_1_RECEIPT.md),
   [`GOVERNANCE_1_RECEIPT.md`](GOVERNANCE_1_RECEIPT.md),
   [`ECONOMIC_STATE_1_RECEIPT.md`](ECONOMIC_STATE_1_RECEIPT.md),
   [`VALIDATOR_SCHEDULE_1_RECEIPT.md`](VALIDATOR_SCHEDULE_1_RECEIPT.md),
   [`EXECUTOR_1_RECEIPT.md`](EXECUTOR_1_RECEIPT.md),
   [`EXECUTION_ROOT_1_RECEIPT.md`](EXECUTION_ROOT_1_RECEIPT.md),
   [`UPGRADE_FRAMEWORK_1_RECEIPT.md`](UPGRADE_FRAMEWORK_1_RECEIPT.md),
   [`GENESIS_ROOT_PARITY_1_RECEIPT.md`](GENESIS_ROOT_PARITY_1_RECEIPT.md),
   [`MULTI_1_RECEIPT.md`](MULTI_1_RECEIPT.md),
   [`SEQUENCER_1_RECEIPT.md`](SEQUENCER_1_RECEIPT.md),
   [`SYNC_1_BASE_RECEIPT.md`](SYNC_1_BASE_RECEIPT.md),
   [`CONSENSUS_1_RECEIPT.md`](CONSENSUS_1_RECEIPT.md),
   [`SYNC_1_REPLAY_AUTHORITY_RECEIPT.md`](SYNC_1_REPLAY_AUTHORITY_RECEIPT.md),
   [`CLIENT_1_RECEIPT.md`](CLIENT_1_RECEIPT.md),
   [`VALIDATOR_ECON_1_RECEIPT.md`](VALIDATOR_ECON_1_RECEIPT.md),
   [`SIGNER_OPS_1_RECEIPT.md`](SIGNER_OPS_1_RECEIPT.md),
   [`RELEASE_R1_CANDIDATE_RECEIPT.md`](RELEASE_R1_CANDIDATE_RECEIPT.md) /
   [`release-r1-candidate-manifest-v1.json`](release-r1-candidate-manifest-v1.json),
   [`PROFILE_1_RECEIPT.md`](PROFILE_1_RECEIPT.md), and
   [`RELEASE_1_RECEIPT.md`](RELEASE_1_RECEIPT.md)) promote
   `PROFILE-SCOPE-D0`, `LANGUAGE-R1`, `THREAT-1`, `VM-D1`, `BOUNDS-1`, `EMERGENCE-R1`,
   `RESOURCE-1`, `GENESIS-D1`, `PROTOCOL-ID-1`, `CRYPTO-1`, `PROFILE-D1`, `IDENTITY-1`,
   `TRANSITION-SPEC-1`, `AUTH-COMMIT-1`, `ASSET-1`, `GOVERNANCE-1`, `ECONOMIC-STATE-1`,
   `VALIDATOR-SCHEDULE-1`, `EXECUTOR-1`, `EXECUTION-ROOT-1`, `UPGRADE-FRAMEWORK-1`,
   `GENESIS-ROOT-PARITY-1`, `MULTI-1`, `SEQUENCER-1`, `SYNC-1@base`, `CONSENSUS-1@base`,
   `SYNC-1@replay-authority` (`PD-06`), `CLIENT-1@base`, `VALIDATOR-ECON-1@base`,
   `SIGNER-OPS-1@base`, `RELEASE-R1@candidate` (`PD-04` + `PD-05`), `RELEASE-R1@base`,
   `PROFILE-1@base`, `RELEASE-1@base`, `RELEASE-1@admitted`, `REFINE-1@vm`, and
   `REFINE-1@base` (completing all 4/4 `REFINE-1` phases plus `ConsensusSafety.lean` across 198
   `--trust=0` Lean 4 theorems,
   all Waves 13–15 launch state producers, dual-language independent execution, L1 §5 two-layer
   execution-root composition, version-lifecycle / compromise verification, 4-way preimage-verified
   logical-genesis to typed-root parity, deterministic multi-actor signed-stream folding,
   accountable centralized multiplayer publication over `STORAGE-1` and loopback TCP, 4-role
   peer synchronization & fault recovery with opaque-only authority retention, HotStuff 2-phase
   BFT finality + live parent-authority certificate admission, independent historical
   replay-authority certificate reconstruction, proof-bound 6-state client finality & reorg
   rollback, receipt-exact consensus-to-economic incentive correspondence + 7-vector
   adversarial simulation, role-separated validator signer safety with 7/7 operational drills,
   `PROFILE-D1`-bound isolated dual-target clean-build candidate reproducibility,
   `PROFILE-1` admitted activated-feature closure, and `RELEASE-1` 3-of-5 signed admitted release
   integrity for `bitblock.devnet.v1`)
   to `BOUNDED` and
   `VM-1` to `IMPLEMENTED`, unlocking **`CHAIN-LAUNCH-1@base=READY`** alongside
   `UPGRADE-1@base=READY` and `PROGRAM-STATE-1@base=READY`
   (while `TESTNET-1`, `SUCCESSOR-ACTIVATION-1`, and `PROD-OPS-1` stay locked on
   their remaining launch/testnet prerequisites, with `program-active` and `verkle-selected`
   kept explicitly inactive at launch genesis).

Useful entry points (run from the repository root; these commands are not a substitute for
physical display verification):

```powershell
cargo run -- --simulate-actions examples/observer_walk.tape
python -B tools/semantic_batch_vectors.py
python -B tools/simulate_sequence.py examples/observer_walk_sequence.json --verbose
cd proof
lake build
cd ..
cd proof/ladderlang-transition-v1
.\verify.ps1
```

The earlier `7075c54` check list is an unretained historical report; do not inherit it as a fresh
pass. The two frozen future-plan artifacts predate the design distillation. The first design audit
freezes input `694650f`; its exact correction is `ea9294b`. The stable whitespace receipts for that
historical correction are:

```powershell
git diff --check 694650f4028f5c1fb715384597326e0b0e31f0e1..ea9294ba08b5e4bd339f82894ecb761461f5af13
git show --check --format=fuller ea9294ba08b5e4bd339f82894ecb761461f5af13
```

The exact inputs and first-addition identities for the current successor audits are in their
canonical `Loop7ProvenanceV1`, `Loop8ProvenanceV1`, `Loop9ProvenanceV1`, `Loop10ProvenanceV1`, and
`Loop11ProvenanceV1` tables; earlier frozen
audits retain their own identities. The
stable whitespace receipts for the complete second and third corrections are:

```powershell
git diff --check fb5f5ca048144f84b0d600619202b3a6f400a02c..54926f52fa8ba9768e817ecb6139dcca41960e6a
git show --check --format=fuller 54926f52fa8ba9768e817ecb6139dcca41960e6a
git diff --check 37a8a8a9a411462997f0f8c945f72c3e938bf7b8..b23ddc95e5075c35d0dbf918b89f353fcdd40ddd
git show --check --format=fuller b23ddc95e5075c35d0dbf918b89f353fcdd40ddd
```

Each later documentation/checker correction is admitted only after:

```powershell
python -B tools/check_planning_docs.py --self-test
git diff --cached --check
```

The first command validates the pinned node inventory and structured phase statuses, the compatible
inherited-base/phase/conditional role union, structured conditional edges and predicate entailments,
Markdown shape, repository-contained tracked relative files and fragments, frozen audit/receipt
blobs, the canonical source receipt plus Markdown agreement, its historical Git objects and current-
lineage ancestry, frozen-table-derived audit-provenance projections, the exact core/collision and
live-certificate/replay assignment manifests, quote-preserving assignment parsing and envelope-
helper closure, paragraph-normalized semantic-shape projection rules, all-parent first-addition
inspection, and the derived
`PlanningCheckerSummaryV1`. Powered controls exercise those paths and require mechanism-specific
cycle failures. It does not decide
predicate truth, prove arbitrary prose or closure completeness, inspect external-link contents,
establish future-profile satisfiability, validate runtime behavior, or prove cryptographic security.
The second command checks the complete staged correction for whitespace errors. The tenth correction
has only these narrow classes of repository evidence; its predecessor audit found a nonzero material
gap/residual set. A later complete bounded audit must pin the tenth correction and find zero new
material gaps and zero falsely
closed material residuals before broad tests are attributed; moving `HEAD` is never a stable
historical receipt.
