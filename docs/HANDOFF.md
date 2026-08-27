# Bitblock agent handoff

This is the short operational handoff for continuing Bitblock. It is intentionally smaller than
the transcript audit: use it to orient quickly, then use the linked documents for the full
semantic history.

## Current repository state

- Remote: `https://github.com/Gaia-0-0-1-1/bitblock`
- Continuation branch: `codex/bitblock-progress-2026-08-26`
- Base branch: `timeline-2`
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
- `b23ddc95e5075c35d0dbf918b89f353fcdd40ddd` is the coordinated correction for TG-01 through
  TG-08. It binds the post-ID launch anchor, deterministic migration envelope/body, verifier work
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

Start from a fresh clone or an existing checkout with:

```powershell
git fetch origin
git switch --track origin/codex/bitblock-progress-2026-08-26
git status --short --branch
```

Read in this order:

1. `docs/HANDOFF.md` (this file)
2. `docs/PROJECT_INTENT.md` (why Bitblock exists and what must remain separate)
3. `docs/CAPABILITY_TREE.md` (dependency DAG, statuses, artifacts, gates, and non-claims)
4. `docs/DESIGN_RESEARCH_DISTILLATION.md` (source receipt, corrected research map, and open questions)
5. `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT.md` (frozen 15-gap successor audit and correction map)
6. `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP2.md` (frozen 11-gap second successor audit)
7. `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP3.md` (frozen eight-gap third successor audit)
8. `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP4.md` (frozen eight-gap fourth successor audit)
9. `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP5.md` (frozen eight-gap fifth successor audit)
10. `docs/FUTURE_PLAN_AUDIT_LOOP2.md` (second future-plan audit/fix map and Verkle disposition)
11. `docs/FUTURE_PLAN_AUDIT.md` (immutable first future-plan audit)
12. `docs/SEMANTICS.md` (current meaning of state/ticks/receipts)
13. `docs/TRANSCRIPT_AUDIT.md` (design chronology and truth labels)
14. `docs/AUTHENTICATED_STATE_PLAN.md` (research-only MPT/SMT plus separate Verkle feasibility)
15. `docs/LAYER1_DESIGN_PAPER.md` (sole proposed network byte graph; still unimplemented)
16. `docs/LAYER1_DESIGN_AUDIT.md` (canonical corrected checkpoint-0.3 reading copy; not a later review)
17. `README.md` (controls, commands, and feature boundaries)
18. `docs/PROTOCOL.md` (canonical local bytes, hashes, history, and research boundaries)
19. `docs/TIMELINES.md` (actual CI trigger and command contract)

## Current proposed design delta after the fifth audit

- Successor migration is closed at the carrier level by old-envelope, availability-covered
  `SuccessorActivationCarrierV1`. Its embedded branch carries `SuccessorActivationBundleV1` and
  `ActivationAuthorizationV1`; the migration context and `SuccessorActivationSignDocV1` bind their
  IDs, the exact target admission, migration specification, height, and old/new roots. Its admitted
  profile/release/`SuccessorUpgradeEvidenceManifestV1`/implementation closure must verify before
  execution or activation.
- Pre-profile intake uses `AcceptedParentLookupPayloadV1`/`accepted_parent_lookup_id`, resolves the
  envelope owner and `AuthorityOwnedParseAndWorkLimitsV1` before profile-owned work, and uses
  `ProposalInnerShallowFrameV1` for header, availability-payload, and signature slices. The launch
  closure is embedded-only with exact checked count, edge, per-preimage-length, and byte-sum
  equations. An authorization-free `AdmittedReleaseCoreV1`, exact `ReleaseSignDocV1`, and detached
  `ReleaseAuthorizationV1` remove the release-signing cycle.
- Capability planning now distinguishes unconditional base dependencies from structured
  phase-specific edges, and every normative secondary readiness/lock/open state is a structured
  phase-status tuple rather than an unguarded prose suffix. `SUCCESSOR-ACTIVATION-1` is the explicit
  owner and remains `LOCKED`.

These are specification and checker changes only. No successor migration, release authority,
network intake, authenticated state, cryptographic suite, or Layer 1 execution exists in Rust.

## What is implemented

The current product surface is deliberately small: a free observer in a sparse binary coordinate
world. `(0, 0, 0)` is permanent bedrock; the observer starts at `(0, 0, -1)`; all other absent
coordinates are off. There are no entities, gravity, terrain, inventory, or autonomous block rules.

The fixed 60 Hz reducer consumes aggregate live-input flags/floats plus ordered edit attempts,
updates camera/world state, commits resolved exact edits to coordinate history, appends an
aggregate tick outcome, and derives a new frame only when the surface is dirty. Resize,
sidebar-toggle, and clear-motion actions mutate local application state before `fixed_tick`.
The window presentation step is separate. Empty ticks still produce outcome history and may
reuse the last raster.

The observer has movement, look, logarithmic wheel dolly, placement, deletion, fit, crosshair, and
an F3-style metrics sidebar. Point sweeps prevent movement through occupied voxels; an edit that
would occupy the observer's containing cell is rejected with an explicit disposition. The live
default starts beside the origin at distance `1.0`; the exact one-pixel camera constructor exists
but is not the default startup path.

Supplied `InteractionTape` values are bounded replay inputs: they store contiguous tick records,
preserve action order and floating-point bit patterns, enforce canonical payload limits, and
receive a `#bb.intent.sha256...` tag. They are not evidence that the live application records
every accepted action. `TickOutcome` aggregates non-edit controls, while coordinate history omits
aim misses, observer-overlap rejections, camera-only actions, and idle ticks.
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
- `.gitattributes`: forces LF checkout bytes for all nine byte-frozen audit/receipt artifacts so
  their Git-blob guards are reproducible under Windows `core.autocrlf=true`.
- `tools/check_planning_docs.py`: retained dependency/Markdown checker with powered negative
  controls, structured unconditional-base and phase-specific dependency/status manifests,
  72 nodes, 94 structured phase-status tuples, five structured phase dependencies, 44 conditional
  instance edges, predicate-entailment manifests, exact 19-document authority inventory, effective
  frozen-file EOL policy, frozen-audit/receipt guards, historical receipt-object reconstruction,
  and repository-contained tracked link/fragment checks; it does not validate semantic dependency
  completeness, prose meaning, external URLs, runtime semantics, or cryptographic security.

## Claims that are safe to make

- The world is sparse and lazily allocated in 16 x 16 x 16 chunks.
- Coordinates are signed 64-bit, so the address domain is large but finite.
- The origin is immutable and the observer/world separation is enforced by the current point
  sweep/edit guards.
- A replay of the same accepted tape under the same build and protocol limits has deterministic
  simulation semantics.
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
- Do not claim a Merkle Patricia trie or sparse Merkle tree is selected or implemented; both are
  research candidates, and the legacy `world_tag` remains the current full-set identity.
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

1. Finish the convergence protocol first: pin this fifth-audit correction, run another complete
   independent read-only sweep, and freeze either its new findings or a zero-new-material-gap
   result. Only the zero-gap result opens the broad Rust, Python-vector, headless, Lean, and
   release-path test pass requested by the user. It still cannot replace physical display review.
2. The preferred next semantic implementation node is `ACCOUNT-1`: reconcile submitted, admitted,
   executed, rejected, no-op, camera/control, edit-miss/overlap, and idle action/result lanes. This
   precedes durable episodes, signed actors, and executable rules.
3. After `ACCOUNT-1`, freeze `LOCAL-BOUNDS-1` before `STORAGE-1`; do not make local durability wait
   on a circular future network profile. `LEDGER-1` and `TRANSITION-SPEC-D1` follow their exact
   dependency fields.
4. `COMMIT-R1` can proceed as a research-only checkpoint: freeze the common key/node/proof/resource
   profiles and compare fixed-key binary Patricia with a 192-level sparse Merkle tree. Do not hook
   either into `World`, history publication, receipts, rendering, or collision until the shared
   assay passes and `LEDGER-1` exists.
5. `VERKLE-R1` is separate feasibility work. It must freeze its vector-commitment scheme, curve or
   field, generator/setup and transcript rules, 192-bit embedding, independent proof corpus,
   end-to-end cost/TCB, and migration policy before a prototype can challenge the hash baseline.
   Proof-size intuition alone cannot select it or delay `COMMIT-R1`.
6. Resolve the open startup choice: adjacent spawn at distance `1.0` versus exact one-pixel
   startup. If changed, update the constructor path, vectors, controls, and docs together.
7. Run `SCALE-1`/`COST-1` measurements before designing a hierarchical chunk/LOD path.
   The present below-saturation renderer traverses on bits; no “near-infinite performance” claim
   is licensed yet.
8. Locate the user's intended balanced-trit hashing implementation under `HASH-SOURCE-R1`; only
   then open `HASH-R1` to define and test its exact input/output and security contract. Preserve the
   recorded disqualification of the examined
   Aethor TSH/PDH constructions and IterHash; do not conflate them with a different missing design.
9. Only after `ACCOUNT-1`, consider `RULE-1`: one finite opt-in local transformation rule with a
   named neighborhood, ordering, resource bound, and deterministic
   `(initial state, accepted tape, rule version)` replay. Do not combine it with asset or network
   authority.

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

The second through fifth successor audits freeze `ea9294b`, `54926f5`, `b23ddc9`, and `2cb1cc0`
before their corrections. The stable whitespace receipts for the complete second and third
corrections are:

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

The first command validates the pinned node inventory and structured phase statuses, declared
unconditional-base/phase dependency and role graph, structured conditional edges and predicate
entailments, Markdown shape,
repository-contained tracked relative files and fragments, frozen audit/receipt blobs, the
canonical source receipt plus Markdown agreement, and its historical Git-object/count claims.
It also pins the exact 19-document authority inventory and the effective LF checkout policy for
all nine frozen text artifacts. Powered controls exercise those paths. It does not decide predicate
truth, prove prose or closure completeness, inspect external-link contents, establish
future-profile satisfiability, validate runtime behavior, or prove cryptographic security. The
second command checks the complete staged correction for whitespace errors. A later
zero-new-material-gap audit must pin this fifth correction before broad tests are attributed to it;
moving `HEAD` is never a stable historical receipt.
