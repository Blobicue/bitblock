# Bitblock transcript audit and memory handoff

This is the durable handoff for the Bitblock design conversation through the current
checkpoint. It was reconstructed from the user/assistant transcript and reconciled against the
source tree on the progress branch. It is not a claim that every historical command was rerun
while writing this file.

The words below have deliberately narrow meanings:

- **Implemented** means the behavior or surface exists in the current source tree.
- **Bounded evidence** means a test, vector, or proof covers the named finite/model scope only.
- **Historical report** means the conversation recorded a prior run; it was not rerun in this
  audit pass.
- **Open** means do not treat the statement as settled.
- **Not claimed** means the surrounding machinery explicitly refuses that interpretation.

## Source-of-truth order

When memories disagree, use this order:

1. The current Rust implementation and its explicit tests/vectors.
2. The Lean/LadderLang capsules and their declared scope.
3. The README, protocol, metrics, semantic, and display documents.
4. This transcript audit for intent, rejected interpretations, and unresolved questions.
5. A visual or verbal recollection that has no artifact is an observation to investigate, not a
   world-state fact.

## Conversation checkpoints reconciled with the repository

### 1. “A block is one bit at a coordinate”

User intent: copy the readable cube/voxel shape of Minecraft, but make existence itself the
binary value: coordinate occupied means on; absent means off.

Current truth: `World` stores signed 64-bit `Coord` values in lazily allocated 16 x 16 x 16
chunks. Missing chunks and missing bits are off. `set_on`, `set_off`, and `toggle` implement the
binary edit surface. This is **implemented**.

Important correction: signed 64-bit coordinates give a very large finite address domain, not
mathematical infinity. The implementation has no artificial map edge, but memory, CPU time, and
the coordinate representation remain finite.

### 2. “The tick powers the simulation and rendering”

User intent: make time a fixed tick that drives the entire experiment, in the spirit of the
single-fold GameLadder model.

Current truth: `TICK_RATE` is 60 Hz. `App::fixed_tick` consumes normalized input, updates camera
and world state, commits the coordinate/history identity, appends a `TickOutcome`, and derives
frame inputs/outputs when rendering is dirty. The live accumulator caps catch-up work. This is
**implemented**.

The semantic boundary is narrower than “every tick draws”: idle ticks still emit outcome/history
state, but `fixed_tick` can return without deriving a new frame. Window submission/presentation
is a separate boundary. This distinction is now reflected in `README.md` and `SEMANTICS.md`.

### 3. “Make it cheap in Rust”

User intent: keep the core minimal and avoid paying for entities or a preallocated world.

Current truth: the core is Rust; chunks are allocated only when bits are on; research/proof
surfaces are feature-gated. This is **implemented**.

What remains open: the renderer currently traverses every on bit below top saturation, collision
sweeps are correctness-first, camera coordinates are absolute `f64`, and no benchmark establishes
near-infinite performance. Sparse allocation is practical scalability, not a proof of unlimited
throughput.

### 4. “Start as an observer beside one origin block”

User intent: no entities, gravity, or physics; spawn next to one permanent reference block and
let the observer move through empty space.

Current truth: `(0, 0, 0)` is permanent orange bedrock; the camera starts at `(0, 0, -1)` facing
it. The observer is camera state, not an entity. There is no gravity, terrain, inventory, or
autonomous world rule. This is **implemented**.

The later request that the observer cannot enter blocks added point occupancy and swept movement
collision. That is now a deliberate invariant, not a return to a general physics engine.

### 5. “Make the first block one physical pixel and zoom from pixel to screen”

User intent: at one end of depth, the whole finite existence can saturate one pixel; at the other
end, a single block can fill the view; wheel depth and camera movement work throughout.

Current truth: logarithmic dolly, top-saturation fitting, `Camera::for_single_pixel_voxel`,
one-pixel aggregation, direct below-saturation rasterization, and `F` refitting exist. This is
**partially implemented**.

The live application currently constructs `Camera::for_observer_spawn()` at distance `1.0`, not
`for_single_pixel_voxel()`. Therefore exact one-pixel startup is a design requirement recorded
for a later checkpoint, not a fact about the current default binary. A hierarchical renderer for
intermediate or very large scales is also still open.

### 6. “Add a crosshair”

User intent: make the exact aimed client pixel visible without changing the simulation meaning.

Current truth: the crosshair is a declared overlay policy, is drawn after the world pass, and its
center pixel remains the underlying world pixel. The overlay affects the final frame projection,
not world/history state. This is **implemented**.

### 7. “Hunt for bugs and logical fallacies”

User intent: actively look for hidden state, rendering artifacts, and claims that do not follow
from the model.

Current truth: the source contains invariant tests and bounded vector/proof surfaces for world
edits, camera/raster behavior, tick outcomes, histories, and display carriers. This is
**bounded evidence**, not an exhaustive bug proof. The current known logical traps are recorded
below rather than hidden behind a green label.

### 8. “Use my ternary hash and LadderLang to prove the space”

User intent: investigate whether a ternary hashing/proof tool and LadderLang can formally verify
the coordinate universe and its transitions.

Current truth: the Bitblock protocol currently uses SHA-256 content tags for canonical byte
identity. `proof/` contains Lean models for the abstract coordinate/edit world and quoted-byte
formulas; `proof/ladderlang-transition-v1/` is a separate pinned finite transition capsule. The
representation bridge is verifier-first and explicitly bounded. This is **bounded evidence**.

The user's specific ternary hashing implementation was not identified or integrated into this
Bitblock checkpoint. Nothing here proves that Rust, rendering, a physical display, a distributed
ledger, or an unbounded universe has been verified by that algorithm. Locating and integrating a
specific external algorithm is an **open** task that needs its source and contract first.

### 9. “Make the language interactable from both sides”

User intent: let a person or an agent express the same actions and edits in a replayable form.

Current truth: the text edit language (`on`, `off`, `toggle`) and the bounded line-oriented
`InteractionTape` share the accepted-action/fixed-tick reducer. A JSON scenario helper can produce
tapes for exact sequences. This is **implemented**.

The edit language is not self-executing in the programming-language sense. Blocks are memory,
ticks are execution order, and no autonomous block transformation semantics exist yet. Calling
it a general programming language or an emergent computer would be an **open** claim.

### 10. “Simulate a sequence without computer use”

User intent: replay the same action sequence headlessly when the user cannot operate the window.

Current truth: `tools/simulate_sequence.py` builds a tape; the Rust binary replays accepted
actions one fixed tick at a time and reports outcome/world/history/camera/frame commitments. The
preflight path charges bounded input, raster, coordinate, chunk, and ordering work before
allocation. This is **implemented**.

The boundary starts after platform input acceptance. It cannot prove focus, mouse capture,
synthetic-event filtering, OS timestamps, redraw scheduling, a physical monitor, scanout, or
photons. Headless frame commitments are renderer/state evidence, not screen evidence.

The prior conversation recorded one sample run of the walk scenario as successful: 7 ticks, 10
actions, final `on_bits=2`, and 3 coordinate receipts. That is a **historical report**, not a
fresh execution of this audit.

### 11. “I saw pixels in multiple places after zooming out”

User intent: determine whether unexpected pixels imply an undiscovered fractal or hidden world
state.

Current truth: no fractal generator or autonomous rule exists. At top saturation the renderer
uses an aggregate projection; below it, it projects/rasterizes each currently on bit. Multiple
screen pixels can therefore come from projection scale, face rasterization, depth ordering,
crosshair/palette overlays, or a display-side artifact. The transcript observation alone cannot
identify which one occurred, and it does not prove blocks were secretly placed.

This remains **open for visual confirmation**. A headless tag can prove the renderer's canonical
buffer for a given state; it cannot reconstruct an unrecorded physical screen observation.

### 12. “Add immutable origin/bedrock”

User intent: guarantee one unbreakable reference point and spawn beside it.

Current truth: `World::new()` inserts `BEDROCK_COORD`; `set_off` refuses it; `toggle` leaves it on;
the origin chunk cannot disappear. This is **implemented** and is the base case for world/history
proofs.

### 13. “The observer must not go inside blocks”

User intent: movement and placement must preserve observer/world separation.

Current truth: movement uses a swept point test with a clearance; contact stops the requested
vector. A placement that would turn on the observer's containing cell is rejected and retained as
an explicit tick disposition. This is **implemented**. Face sliding and exact far-`i64` camera
geometry remain future work.

### 14. “Show every useful metric in a sidebar”

User intent: add an F3-like surface for coordinates, world state, view, tick, hashes, and unusual
diagnostics.

Current truth: the registry documents 1,112 metric families; the first 400-pixel sidebar page
projects a deliberately cheap subset and binds displayed aliases to definitions. This is
**implemented as a catalog plus a bounded runtime projection**. It does not collect all 1,112
families on every frame.

### 15. “Publish the progress and explanations”

Current truth: the non-build local checkpoint was overlaid onto a clone of `timeline-2`, committed,
and pushed as `codex/bitblock-progress-2026-08-26`. The original `C:\Users\Owen\bitblock` folder
was not a Git checkout and was left untouched. Generated `target`, Lean `.lake`, and Python cache
files were excluded. This is **historical repository state**; future commits should update this
handoff when semantics change.

## Semantic lessons that should survive future checkpoints

1. **Coordinate memory is the primitive.** An on bit is a fact about one address; absence means
   off, not unloaded and not a hidden default block.
2. **The tick is the state authority.** Input adapters, reducers, renderers, and presenters may
   be separate code, but only the fixed-tick fold changes simulation meaning.
3. **Accepted input is a boundary.** A replay receipt begins after OS acceptance; it cannot be
   promoted into device or display evidence.
4. **Camera, world, frame, and physical display are different objects.** A camera coordinate is
   not an entity; a frame tag is not a world tag; a GDI submission is not monitor light.
5. **The origin is a semantic anchor.** It keeps the world non-empty and supplies a permanent
   base case for edits and proofs.
6. **Sparse is not infinite.** Lazy chunks remove preallocation, not resource limits, traversal
   cost, or floating-point precision limits.
7. **Hashes name bytes; they do not grant authority.** Content tags and local history explain
   provenance. They do not establish truth, ownership, time, consensus, Proof-of-History,
   Proof-of-Stake, or Layer 1 status.
8. **No autonomous rules means no demonstrated emergence.** User-built structure and exact
   replay are present; emergent computation remains a falsifiable future hypothesis.
9. **Proof scope must travel with the proof.** A Lean model or finite LadderLang capsule can be
   correct within its declared domain while saying nothing about Rust refinement, arbitrary
   camera floats, physical displays, or unlimited performance.
10. **Keep failed and unavailable states visible.** A refused edit, absent frame, unbound sensor,
    or skipped capture is evidence about the boundary and must not be rewritten as success.

## Open questions for the next checkpoint

- Should live startup be changed to the exact one-pixel camera, or should adjacent spawn remain
  the default with one-pixel fit as an explicit action?
- Where is the user's ternary hashing implementation, and what exact input/output contract would
  let it be compared with the existing SHA-256 content-tag protocol without replacing it by
  assumption?
- What hierarchical chunk/LOD index is needed before “near-infinite rendering” becomes a measured
  performance claim rather than a sparse-storage intuition?
- Which finite autonomous rule, if any, should be introduced to test emergence while preserving
  deterministic `(initial_state, accepted_action_tape)` replay?
- Which screen-level observations need a real window/capture run, and which can be reduced to an
  offscreen renderer receipt?

Until those questions are answered with artifacts, they remain open rather than implied by the
current branch.
