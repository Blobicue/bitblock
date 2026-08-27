# Bitblock agent handoff

This is the short operational handoff for continuing Bitblock. It is intentionally smaller than
the transcript audit: use it to orient quickly, then use the linked documents for the full
semantic history.

## Current repository state

- Remote: `https://github.com/Gaia-0-0-1-1/bitblock`
- Continuation branch: `codex/bitblock-progress-2026-08-26`
- Base branch: `timeline-2`
- `1624068` is the exact clean input to the one-sweep future-plan audit. The result is the first
  descendant commit that adds `docs/FUTURE_PLAN_AUDIT.md`; the audit file intentionally avoids an
  impossible self-embedded commit hash.
- `7075c54` is the motive/capability-tree documentation checkpoint: it adds the project-intent
  owner, the dependency DAG, the MPT/SMT research plan, and transcript checkpoints 16–23.
- `88fcf55` is that checkpoint's clean input tip and preserves the concurrent-paper provenance
  note.
- `018ca52` is an intermediate shared-worktree bank of the in-progress checkpoint-0.3 paper;
  do not treat it as the reviewed result.
- The hardened checkpoint-0.3 Layer 1 paper is pinned at `034b7b2`; its second-loop audit is
  `9eace50`. Later motive/tree docs link to that pinned result without silently re-auditing it.
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
4. `docs/FUTURE_PLAN_AUDIT.md` (frozen audit input, findings, patches, and residual boundary)
5. `docs/SEMANTICS.md` (current meaning of state/ticks/receipts)
6. `docs/TRANSCRIPT_AUDIT.md` (design chronology and truth labels)
7. `docs/AUTHENTICATED_STATE_PLAN.md` (research-only MPT/SMT comparison checkpoint)
8. `docs/LAYER1_DESIGN_PAPER.md` (proposed identity, ledger, replication, and security path)
9. `docs/LAYER1_DESIGN_AUDIT.md` (pinned paper findings and residual repository obligations)
10. `README.md` (controls, commands, and feature boundaries)
11. `docs/PROTOCOL.md` (canonical bytes, hash/tag, history, and research boundaries)

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
- `docs/AUTHENTICATED_STATE_PLAN.md`: unimplemented authenticated-map comparison and integration
  constraints.
- `docs/FUTURE_PLAN_AUDIT.md`: the finite audit/fix map for planning input `1624068`.
- `tools/check_planning_docs.py`: retained dependency/Markdown checker with powered negative
  controls; it does not validate runtime semantics.

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

1. The preferred next semantic implementation node is `ACCOUNT-1`: reconcile submitted, admitted,
   executed, rejected, no-op, camera/control, edit-miss/overlap, and idle action/result lanes. This
   precedes durable episodes, signed actors, and executable rules.
2. After `ACCOUNT-1`, freeze `LOCAL-BOUNDS-1` before `STORAGE-1`; do not make local durability wait
   on a circular future network profile. `LEDGER-1` and `TRANSITION-SPEC-D1` follow their exact
   dependency fields.
3. `COMMIT-R1` can proceed as a research-only checkpoint: freeze the common key/node/proof/resource
   profiles and compare fixed-key binary Patricia with a 192-level sparse Merkle tree. Do not hook
   either into `World`, history publication, receipts, rendering, or collision until the shared
   assay passes and `LEDGER-1` exists.
4. Resolve the open startup choice: adjacent spawn at distance `1.0` versus exact one-pixel
   startup. If changed, update the constructor path, vectors, controls, and docs together.
5. Run `SCALE-1`/`COST-1` measurements before designing a hierarchical chunk/LOD path.
   The present below-saturation renderer traverses on bits; no “near-infinite performance” claim
   is licensed yet.
6. Locate the user's intended balanced-trit hashing implementation under `HASH-SOURCE-R1`; only
   then open `HASH-R1` to define and test its exact input/output and security contract. Preserve the
   recorded disqualification of the examined
   Aethor TSH/PDH constructions and IterHash; do not conflate them with a different missing design.
7. Only after `ACCOUNT-1`, consider `RULE-1`: one finite opt-in local transformation rule with a
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
pass. For the one-sweep result whose input is `1624068`, the retained audit artifact is
`docs/FUTURE_PLAN_AUDIT.md`. Before its commit, these exact checks exited zero:

```powershell
python -B tools/check_planning_docs.py --self-test
git diff --check
```

The first command validates the 62-node conservative DAG, fields, references, planning links,
fences, tables, and in-memory negative controls. The second checks the complete patch for whitespace
errors. Rust and Lean suites were not rerun because this sweep changes documentation and a
read-only planning checker only; it provides no runtime/proof validation.
