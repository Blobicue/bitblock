# Bitblock semantic checkpoint

This document records the meaning of the current Bitblock system. It is intentionally
separate from a feature list: a feature says what exists, while a semantic boundary says
what a state, tick, receipt, or proof is allowed to mean.

## The implemented model

Bitblock is a sparse binary coordinate space observed through a camera.

- A signed 64-bit coordinate is either on (a block exists) or off (no block exists).
- `(0, 0, 0)` is permanent orange bedrock. It is the origin and cannot be deleted or toggled
  off.
- The observer starts in the adjacent empty cell `(0, 0, -1)`, facing the origin.
- The observer is camera state, not an entity. There is no entity system, terrain generator,
  gravity, physics simulation, inventory, or autonomous rule system in this checkpoint.
- The observer is a point that may not enter an on cell or tunnel through one. Movement uses a
  swept test; a collision stops the attempted movement vector.
- Coordinates are stored lazily in 16 x 16 x 16 sparse chunks. Empty non-origin chunks can be
  released, while the origin chunk remains allocated. There is no artificial world edge, but
  memory and compute are finite.

The absence of a block is meaningful state: an unspecified coordinate is not an unloaded
coordinate and not a hidden default block; it is off.

## Tick semantics

The tick is the simulation clock and the semantic commit boundary.

1. Platform input is normalized into accepted actions.
2. The accepted actions for one contiguous tick ordinal are applied in their exact order.
3. The fixed tick reducer updates camera and world state, applies edit rules, and commits the
   resulting world/history identities.
4. A tick outcome exposes the post-world, post-history, camera, and frame-input/frame-output
   commitments that were derived from that fold.

Empty ticks are real idle ticks. Wall-clock scheduling chooses when a tick is attempted; it is
not a hidden input to the reducer. The same accepted action tape therefore has one deterministic
simulation meaning when replayed with the same build and protocol limits.

Rendering is a projection of the committed tick state. It does not create a second simulation
clock. The current target is a fixed 60 Hz loop, with the window presenting the CPU-produced
XRGB buffer after the tick has derived it.

## Edit semantics

The text edit language and mouse edits name the same integer-coordinate operations:

```text
on x y z
off x y z
toggle x y z
```

Programs are parsed all-or-nothing and applied in source order inside the first simulation
tick. `on` and `off` are idempotent. `toggle` is an involution except at bedrock, where both
`off` and `toggle` are permanent no-ops. Source no-ops remain represented in the program so an
attempt is not silently erased.

An edit that would place a block in the observer's cell is recorded as an attempted action but
is omitted from the executable batch. The richer tick-outcome sidecar preserves that refusal and
its disposition. This makes invalid overlap observable without allowing an impossible world
state.

The edit language is not yet a self-executing programming language. Blocks currently provide
memory and ticks provide execution order; block-to-block transformation rules are a later
checkpoint.

## Camera and pixel semantics

The camera has position, yaw, pitch, viewport, and observation depth. Mouse look, six-direction
observer movement, wheel dolly, and an explicit fit operation all update camera state through the
same tick path. The crosshair is drawn into the simulation viewport and identifies the exact
aimed client-raster pixel.

Scroll depth spans the one-pixel view through a block-filling view. Below the one-pixel
saturation level, the current correctness-first renderer traverses every on bit and rasterizes
its projected contribution. This preserves the intended projection semantics but is not yet a
hierarchical near-infinite renderer: large worlds still cost real traversal time.

`F` is a camera reframe to a safe view of the complete current bounds. It is not physical travel
through every intervening coordinate. Camera values remain absolute `f64`; exact collision
geometry across the full signed 64-bit range is an identified future scaling boundary.

## Interaction tapes and replay

`InteractionTape` is a bounded, canonical, line-oriented protocol for accepted observer actions.
Each `tick` starts one contiguous record; the record may be empty. Supported actions include
movement edges, look deltas, wheel lines, place/delete, fit, sidebar toggle, resize,
clear-motion, and an explicitly scripted exact edit.

The canonical binary encoding preserves tick ordinals, action order, and finite floating-point
bit patterns. Its content is named by a `#bb.intent.sha256...` tag and is subject to explicit
record, action, and byte limits before allocation. `tools/simulate_sequence.py` converts JSON
scenarios into this tape and the Rust binary replays it without opening a window.

Headless replay reports one tick record at a time: outcome tag, world tag, history tag, camera
bits, and any derived frame commitments, followed by final world/history/camera metrics. The
headless path intentionally refuses to publish ordinary frame receipts; an optional
`protocol-research` build can add an in-memory local replay-evidence carrier.

This protocol begins after platform input acceptance. It does not prove focus, capture, repeat
filtering, OS timestamps, pixel-wheel conversion, redraw scheduling, F4/F5 capture behavior, or
what a physical display showed. A deterministic action tape proves the reducer and replay
boundary, not the external device path.

## History, hashes, and proofs

Content tags are identities of canonical bytes under named schemas. A world tag identifies the
canonical coordinate state; history tags link coordinate/edit receipts; frame tags identify
renderer projections; an interaction tag identifies the accepted-action tape. A matching tag is
evidence that the named bytes match, not an independent claim that those bytes are true, newest,
owned, persisted, consensually accepted, or visible as light.

The default binary keeps research/proof surfaces off the hot render path. Optional surfaces cover
typed storage, local sequential replay, coordinate issuance episodes, presentation artifacts,
Windows timing/topology/configuration observations, RGB challenge carriers, and a verifier-first
representation bridge. Each surface keeps unavailable and unbound states explicit.

The Lean capsules prove bounded statements about the abstract coordinate world, edit/replay
rules, and quoted-byte representation formulas. The LadderLang transition capsule checks a
separate finite target under its pinned reducer. These capsules do not yet prove that the chunked
Rust implementation refines the abstract world, that camera floats are exact over all `i64`
coordinates, that a physical monitor emitted the raster, or that Bitblock has Proof-of-History,
Proof-of-Stake, consensus, currency, or Layer 1 status.

## Semantic learnings carried forward

1. **A coordinate space is not automatically a game world.** The first useful primitive is a
   sparse addressable bitmap plus a camera. Adding entities or autonomous rules would change the
   semantic object and should be a new checkpoint, not an invisible extension of this one.
2. **The tick is the authority for state change.** Input collection, simulation, and frame
   derivation can have different implementation layers, but the semantic transition is one
   ordered fixed-tick fold. This is what makes replay and headless scenario generation possible.
3. **“Accepted action” is a real boundary.** A replay can be exact after normalization while still
   saying nothing about whether Windows delivered the original mouse/keyboard event. That
   distinction prevents a simulator receipt from being mistaken for a screen observation.
4. **Sparse storage gives practical infinity, not infinite performance.** Lazy chunks remove a
   preallocated world bound. They do not remove the cost of storing touched bits, traversing
   visible bits, or representing camera precision. Hierarchical indexing and chunk-relative
   camera coordinates remain engineering work.
5. **The origin is a semantic anchor, not scenery.** Immutable bedrock makes the empty world
   non-empty, supplies a reference coordinate, and gives edit/history invariants a permanent
   base case.
6. **History evidence is provenance, not consensus.** Parent-linked local receipts can explain
   how a state was derived. They do not establish elapsed time, the latest branch, ownership,
   stake, distributed agreement, or external custody.
7. **Pixel saturation is a projection property.** Making one block occupy one pixel, or making a
   view fill the screen, describes a camera projection. It does not by itself prove an unlimited
   renderer or physical monitor output; those are separate measured boundaries.
8. **Emergence is currently an open hypothesis.** With no autonomous transition rules, the
   system can express user-built structure and exact history, but it has not yet demonstrated
   emergent computation. Any later rule set should be introduced as an explicit, replayable,
   tick-scoped semantics and measured against this baseline.

## Reproduction entry points

From the repository root:

```powershell
cargo run -- --simulate-actions examples/observer_walk.tape
python -B tools/semantic_batch_vectors.py
python -B tools/simulate_sequence.py examples/observer_walk_sequence.json --verbose
```

The proof capsule can be built with `cd proof; lake build`; the separate LadderLang transition
capsule has its own `verify.ps1`. These commands exercise different boundaries and should not be
collapsed into a single claim of physical display or universal scaling.
