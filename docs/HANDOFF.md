# Bitblock agent handoff

This is the short operational handoff for continuing Bitblock. It is intentionally smaller than
the transcript audit: use it to orient quickly, then use the linked documents for the full
semantic history.

## Current repository state

- Remote: `https://github.com/Gaia-0-0-1-1/bitblock`
- Continuation branch: `codex/bitblock-progress-2026-08-26`
- Base branch: `timeline-2`
- The prior published documentation checkpoint was `dd19c5f` (`docs: audit and close Layer 1 paper gaps`).
- `018ca52` is an intermediate shared-worktree bank of the in-progress checkpoint-0.3 paper;
  do not treat it as the reviewed result.
- The hardened checkpoint-0.3 Layer 1 paper is pinned at `034b7b2`; the second-loop
  audit/handoff revision supersedes the earlier audit at the branch tip.
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
2. `docs/SEMANTICS.md` (current meaning of state/ticks/receipts)
3. `docs/TRANSCRIPT_AUDIT.md` (design chronology and truth labels)
4. `docs/LAYER1_DESIGN_PAPER.md` (proposed identity, ledger, replication, and security path)
5. `docs/LAYER1_DESIGN_AUDIT.md` (paper gap findings and residual repository obligations)
6. `README.md` (controls, commands, and feature boundaries)
7. `docs/PROTOCOL.md` (canonical bytes, hash/tag, history, and research boundaries)

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
- `docs/METRICS*` and `docs/metrics/`: the metric catalog and the cheap runtime subset.

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
- Do not interpret the earlier zoomed-out multi-pixel observation as a hidden fractal without a
  captured state/frame artifact.
- Do not claim that the full 1,112-metric registry is collected on every frame; the sidebar shows a
  bounded cheap subset.

## Recommended next work order

Ask the user which checkpoint they want to see next, then make one semantic change at a time
and update `docs/SEMANTICS.md` and `docs/TRANSCRIPT_AUDIT.md` in the same checkpoint.

1. If continuing the Layer 1 path, start with checkpoint A in
   `docs/LAYER1_DESIGN_PAPER.md`: reconcile the complete action/result lanes, then promote the
   feature-gated coordinate-episode prototype into an optional durable local ledger. Do not
   add networking, ownership, and economics in the same change.
2. Resolve the open startup choice: adjacent spawn at distance `1.0` versus exact one-pixel
   startup. If changed, update the constructor path, vectors, controls, and docs together.
3. Measure the current renderer/collision cost before designing a hierarchical chunk/LOD path.
   The present below-saturation renderer traverses on bits; no “near-infinite performance” claim
   is licensed yet.
4. Locate the user's intended balanced-trit hashing implementation and write down its exact
   input/output and security contract. Preserve the recorded disqualification of the examined
   Aethor TSH/PDH constructions and IterHash; do not conflate them with a different missing design.
5. Only after the baseline is stable, add one finite local transformation rule as an explicit
   tick-scoped mode. Preserve deterministic `(initial state, accepted tape)` replay.

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

The last documentation checkpoint ran `git diff --check` and published cleanly. It did not rerun
the Cargo/Lean suites; report any fresh command's exit status explicitly rather than inheriting a
historical “passed” label.
