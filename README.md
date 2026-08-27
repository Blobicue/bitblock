# bitblock

Bitblock is a binary coordinate space observed through a free camera.

For the next agent, start with [`docs/HANDOFF.md`](docs/HANDOFF.md). The broader motive chain is in
[`docs/PROJECT_INTENT.md`](docs/PROJECT_INTENT.md), and the dependency-gated future plan is in
[`docs/CAPABILITY_TREE.md`](docs/CAPABILITY_TREE.md). Read [`docs/SEMANTICS.md`](docs/SEMANTICS.md)
and [`docs/TRANSCRIPT_AUDIT.md`](docs/TRANSCRIPT_AUDIT.md) before changing the simulation boundary.

The forward-looking security architecture is documented in
[`docs/LAYER1_DESIGN_PAPER.md`](docs/LAYER1_DESIGN_PAPER.md). It separates the implemented
local deterministic world from the proposed identity, ownership, replication, consensus,
availability, and Layer 1 requirements.

The paper's historical checkpoint-0.3 gap review is pinned in
[`docs/LAYER1_DESIGN_AUDIT.md`](docs/LAYER1_DESIGN_AUDIT.md). It distinguishes that checkpoint's
prose fixes from repository obligations that remained unimplemented, but it is not the latest
review of the subsequently changed paper. Current review chronology continues through the frozen
[`docs/DESIGN_RESEARCH_DISTILLATION_AUDIT.md`](docs/DESIGN_RESEARCH_DISTILLATION_AUDIT.md),
[`docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP2.md`](docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP2.md),
[`docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP3.md`](docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP3.md),
and
[`docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP4.md`](docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP4.md)
successor records.

The proposed authenticated-state checkpoint—including a candidate 192-bit canonical coordinate-key
encoding and the still-open comparison between a constrained binary Merkle Patricia trie and a
192-level sparse Merkle tree—is isolated in
[`docs/AUTHENTICATED_STATE_PLAN.md`](docs/AUTHENTICATED_STATE_PLAN.md). No authenticated trie or
proof codec is implemented in the current runtime. Verkle is recorded there as a separate,
cryptography-gated feasibility lane; it is not selected and does not share the two hash candidates'
`NodeHash` assay.

The immutable first finite audit is [`docs/FUTURE_PLAN_AUDIT.md`](docs/FUTURE_PLAN_AUDIT.md). The
latest second-loop audit, including the Verkle disposition and its own frozen input, is
[`docs/FUTURE_PLAN_AUDIT_LOOP2.md`](docs/FUTURE_PLAN_AUDIT_LOOP2.md). They audit specifications and
dependencies, not implementation or security evidence.

The large state/Verkle/tick/proof research conversation from `design.txt` is source-receipted and
compressed in
[`docs/DESIGN_RESEARCH_DISTILLATION.md`](docs/DESIGN_RESEARCH_DISTILLATION.md). That file is a
notes-only question map with corrected boundaries, rejected claims, and candidate research lanes;
it does not activate a capability-tree node or change the runtime.
Its first bounded successor review is frozen in
[`docs/DESIGN_RESEARCH_DISTILLATION_AUDIT.md`](docs/DESIGN_RESEARCH_DISTILLATION_AUDIT.md), and the
external input identity/count method is pinned by
[`docs/design-source-receipt-v1.json`](docs/design-source-receipt-v1.json). The audit corrects notes
and checker coverage only; it is not cryptographic or implementation validation.

The first correction is pinned at `ea9294ba08b5e4bd339f82894ecb761461f5af13`. A second complete
read-only sweep of that corrected state is frozen in
[`docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP2.md`](docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP2.md).
It records 11 successor type, dependency, provenance, checker, and CI gaps before their one
coordinated documentation/tooling correction at
`54926f52fa8ba9768e817ecb6139dcca41960e6a`. A third complete read-only sweep of that correction is
frozen in
[`docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP3.md`](docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP3.md).
It records eight launch-anchor, migration-envelope/body, validation-order, Verkle-admission,
checkout-EOL, authority-inventory, and successor-scope gaps before correction
`b23ddc95e5075c35d0dbf918b89f353fcdd40ddd`. A fourth complete read-only sweep of that exact
correction and Git tree `3de2d45aa20b08318f1a1af4d93308daccf1f55e` is frozen at commit
`8a5743595a0aa8ff13ede576f282b92540db5d9b` in
[`docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP4.md`](docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP4.md),
whose first-addition Git blob is `350d1af6279d7f441f45fba2689481c0dbea79a5`. It records eight
launch-closure, block-context availability, execution-input, outer-framing, conditional-predicate,
capability-owner, and review-provenance gaps before this descendant correction.
None of these audits or corrections activates a runtime capability or constitutes Rust, Lean,
cryptographic, consensus, or Layer 1 evidence. Broad implementation tests are intentionally
deferred until a later complete sweep records zero new material design gaps.

- `(0, 0, 0)` is permanent orange bedrock; it is always on and cannot be broken.
- The observer starts at `(0, 0, -1)`, in the adjacent empty cell facing bedrock.
- Every other unspecified coordinate is off.
- The observer is camera state, not an entity.
- There is no gravity, body, terrain, inventory, or autonomous simulation.
- The observer is a point that cannot enter or tunnel through an on block.
- The CPU renderer writes an XRGB client raster.
- Each final game frame also receives a CPU-side exact RGB palette-membership audit after the
  crosshair is drawn. It checks renderer output only; it is not evidence of OS composition,
  scanout, monitor light, or observer perception.
- On Windows, the app owns that XRGB slice and admits a submission only when
  `SetDIBitsToDevice` reports every requested scanline; this is checked GDI acceptance,
  not compositor or monitor proof.
- A research-only Windows timing audit can bind that exact composite receipt to raw
  before/after DWM session timing, QPC brackets, and barrier outcomes. Its per-frame
  attribution is always `Unknown`; it does not claim displayed, dropped, scanout, or light.
- A one-shot research bundle can additionally retain bounded active-output topology and raw
  Windows Advanced Color configuration before and after one OS screen-raster capture. The raw
  flags, encoding word, and bits-per-channel are configuration reports only: stable routes or
  matching configuration do not prove HDR presentation, per-frame DWM attribution, scanout, or
  physical light.
- A feature-gated physical-sensor carrier can retain a full presentation tag, declared integer
  RGB tolerance profile, ordered XRGB challenge positions, and caller-supplied RGB16 values. It
  has no device collector, source attestation, or physical observation yet: `N/A`, unavailable,
  and unbound states remain first-class, and a synthetic codec test is not a monitor-light result.
- One fixed 60 Hz tick consumes accepted input and commits observer/world state. Dirty ticks
  derive a new CPU frame; idle ticks still emit a tick outcome but may reuse the last raster.
  Presentation is a separate window-boundary step.
- Scroll moves logarithmically between the one-pixel view and a block-filling view.

## Controls

- Crosshair: the hollow center is the exact aimed client-raster pixel
- Mouse: look
- `W A S D`: move
- `Space` / `Left Shift`: move up / down
- Wheel: change observation depth
- Left mouse: delete the aimed bit; bedrock ignores the attempt
- Right mouse: place beside the aimed bit, or at the observation anchor in empty space;
  placement in the observer's cell is rejected
- `F`: fit all existing bits into the center pixel
- `F3`: show or hide the metrics page without changing the simulation viewport
- `F4`: with a fresh research display journal configured, archive one exact committed composite
  frame and one Windows desktop-raster observation without creating a simulation tick
- `F5`: arm the next normally changed successful presentation, then archive its exact receipt,
  composition timing, paired pre/post active-output topology/raw configuration, and one
  desktop-raster observation; an idle request waits visibly and does not resubmit the old surface
- `Escape`: release the mouse; click to capture it again

## Shared edit language

Mouse edits and text edits now resolve to the same exact integer-coordinate
instructions. A program is applied in source order inside the first simulation
tick, after the origin bit is created:

```text
# comments and blank lines are ignored
off 0 0 0       # recorded no-op: origin bedrock is immutable
on 1 0 0
toggle 1 1 0
```

The three instructions are `on`, `off`, and `toggle`, followed by signed 64-bit
`x y z` coordinates. Parsing is all-or-nothing and serialization has one
canonical spelling. No-op instructions remain in the program even when they do
not change the world, so an attempted edit is not silently erased from history.
Both `off 0 0 0` and `toggle 0 0 0` are permanent no-ops.

An instruction that would create a block around the observer is counted as an
attempt but omitted from the executable tick batch, so replay cannot reproduce
an invalid observer/world overlap. The richer tick-outcome sidecar preserves
the rejected instruction and its named disposition without executing it.

This is deliberately an **edit language**, not yet a self-executing programming
language. Blocks are memory and ticks are the execution clock; transformation
rules will be a later checkpoint.

## Headless interaction tapes

An interaction tape can now execute a specific observer-action sequence without opening or
controlling a window. Each `tick` starts one contiguous fixed-tick record; the lines below it are
the ordered actions accepted before that tick. Empty records are meaningful idle ticks.

```text
tick
move forward down
look 2.5 -1.25

tick
move forward up
wheel -10
place
sidebar
```

The commands are `move <forward|backward|left|right|up|down> <down|up>`, `look <x> <y>`,
`wheel <lines>`, `delete`, `place`, `fit`, `sidebar`, `resize <width> <height>`, and
`clear-motion`. `exact <on|off|toggle> <x> <y> <z>` is also available as an explicitly scripted
Edit-channel action; no current mouse or keyboard adapter emits it. Comments and blank lines are
ignored. Parsing is all-or-nothing, finite floating-point bit patterns and action order are
preserved, and the canonical binary tape is named by a `#bb.intent.sha256...` content tag.

This boundary begins *after* platform input acceptance. It does not claim to reproduce device or
window identity, focus/capture success, synthetic/repeat filtering, pixel-wheel conversion, raw
timestamps, redraw scheduling, or F4/F5 evidence capture. Live input and tapes do share the same
accepted-action reducer and fixed-tick function.

For a smaller source-semantics check, run:

```powershell
python -B tools/semantic_batch_vectors.py
```

For longer scripted interaction checks, use a structured sequence:

```powershell
python -B tools/simulate_sequence.py examples/observer_walk_sequence.json --verbose
```

The script converts each `ticks[i]` entry into one fixed tick on the accepted-action side:

- each tick is a list of action strings, or an object with `actions` (or `commands`),
- command text is passed through to the tape parser exactly as text,
- optional `expected` fields can be checked with `--expect`.

Example JSON schema:

```json
{
  "name": "observer_walk_sequence",
  "ticks": [
    [],
    ["wheel -10", "place"],
    ["move right down", "look 2.5 -1.25"]
  ],
  "expected": {
    "final": {
      "on_bits": 1
    }
  }
}
```

It independently reconstructs eight bounded one-coordinate history cases (mint, burn, no-op,
bedrock, same-tick reuse, and coordinate extremes) without importing or launching Bitblock. The
Rust history test compares the same frozen known-answer tags. This is evidence for those finite
cases only; it is not a general proof, an executed LadderLang bridge, or a blockchain claim.

Headless runs always start from the empty program, permanent bedrock, observer spawn, visible
sidebar, and the default 1201 x 801 output. Before state is constructed, a pure shadow interpreter
checks the complete tape against payload/action limits and cumulative resize, composition, raster,
world-coordinate, sparse-chunk-word, edit, and canonical-ordering work budgets. Invalid finite
accumulation and over-budget work fail closed.

The ordinary `headless-console` report contains the tape tag, per-tick
outcome/world/history/camera and CPU-frame commitments, and the final state. Those lines remain
lightweight reproducible projections. With the opt-in `protocol-research` feature, the same
command additionally constructs and structurally verifies one in-memory
`#bb.replay-evidence.sha256...` v1 carrier. It embeds the complete canonical tape, a streamed
fingerprint of the current local executable, declared headless genesis/rules/admission policy
tags, and the exact final fixed-tick/world/history/camera/latest-frame endpoint. It is not written
to a file, signed, or treated as custody, ownership, consensus, time, display, scanout, or
physical-light evidence. The research command is:

```powershell
cargo run --release --features "headless-console protocol-research" -- --simulate-actions examples/observer_place_delete.tape
```

Headless frames are intentionally unsubmitted: frame admissions and receipt heads remain empty,
and no OS raster, scanout, or physical-light evidence is produced. The default build does not
include the local-replay carrier code.

The app retains the most recent tick's resolved, ordered integer-coordinate
batch in memory. That batch can be replayed without the camera or renderer and
is the shared semantic surface for mouse input, text input, Rust tests, and
formal proofs. Receipt-bearing ticks also enter an unsigned in-memory SHA-256
chain with exact mint, burn, protected-no-op, and idempotent-no-op provenance.
Empty ticks do not grow this coordinate history. Durable append storage remains
separate from the live app: the opt-in research journal can persist generic
canonical storage records, and its typed coordinate-history layer now admits a
caller-supplied genesis plus canonical receipts only after exact semantic
replay. It reconstructs the world, receipt chain, and active mint provenance
before exposing a writable handle. This feature-gated research path is not yet
wired into the default observer executable and does not establish external
custody, rollback resistance, ownership, or consensus.

Every completed fixed tick, including an idle tick, is also appended to one
in-memory content-addressed `TickOutcome` v2 chain. Each outcome's parent tag, exact accumulated
controls, ordered unresolved/rejected/executed edit dispositions, pre/post
world and coordinate-history tags, post-camera bits, and optional game/sidebar
frame commitments form one contiguous local fold history. Window resize no
longer mutates the camera between folds: one named viewport-refit input is
queued and consumed first by the next tick. Release-path boundary checks compare
the actual world with coordinate-history identity and the actual pre-camera with
the preceding outcome. A rendered tick also reconstructs its game `FrameInput`
tag from post-world, post-camera, simulation shape, renderer profile, and
crosshair overlay before publishing the new head. This sidecar is not yet
durable or independently replay-verified. An additive `FrameReceiptV2` schema
content-binds a declared `game`/`composite` stream selector, outcome tag, and
input/frame tags in 130 canonical bytes. Live presentation now resolves the
game and composite endpoints independently, retains every skipped outcome,
prepares and reserves both structural v2 admissions, invokes the backend once,
and publishes neither receipt if validation or submission fails. The Windows
backend no longer uses Softbuffer's unchecked `BitBlt` wrapper: it copies the
complete canonical slice into an owned buffer and requires the exact
`SetDIBitsToDevice` scanline count before either frame receipt is published.
`FrameReceipt`
v1 bytes remain unchanged as a staged compatibility sidecar for the sidebar
predecessor field. The research-only optimization consumer now accepts only
typed game/composite `FrameLedgerV2` evidence, checks the complete retained
outcome ancestry against coordinate history, and binds its synthetic display to
the full committed composite receipt. The opt-in Windows display audit accepts only a
typed committed composite `FrameReceiptV2`; its separate `DisplayReceiptV2`
derives the expected framebuffer from that object while preserving the historic
display-v1 epoch and vectors. The live retained chain and both v2 frame ledgers
remain in-memory, duplicate some path evidence, and are not yet shared or
independently semantic-replay verified.

The research-only `DisplayCaptureArtifact` v1 codec can now bind one strict
`DisplayReceiptV2` to the complete expected frame and absent, reused, or distinct
captured pixels as a single content-addressed object. It recomputes the full
pixel observations and exact mismatch facts, rejects impossible capture-stage
transcripts, and fits the initial distinct 1201 x 801 pair under the existing
8 MiB payload cap. A feature-gated typed research journal now admits this object
only after full typed preflight, appends it as one storage record, calls
`sync_all`, rereads the exact record bytes, and close/reopen checks both the
storage-record and embedded display-receipt parent chains. It exposes the
artifact, display-receipt, frame-receipt, expected/captured-frame, policy,
TickOutcome, and FrameInput tags while marking the latter references as
reference-only rather than claiming their bytes. A Windows build with the
`protocol-research` feature can now create one explicitly new journal and use
F4 to archive a selected committed composite occurrence plus one OS screen-
raster sample. The request first checks the current composite receipt head and
unchanged expected pixels, does not dirty or advance a tick, and is consumed by
one capture attempt. A nonpoisoning append refusal retains that exact artifact
for an explicit F4 retry rather than resampling; an uncertain/poisoned append
requires typed reopen. The default observer does not link this path, and no mode
records continuously. Local sync/reread is not sudden-power durability,
latest-prefix authority, external custody, scanout, or physical-light evidence.

The same journal can mix legacy display artifacts with `PresentationOutput` v1
and v2 records. F5 waits for the next normal successful composite presentation and
uses the timing envelope produced around that exact backend call even when the
continuous timing logger is disabled. It then records bounded active-output
topology plus raw Advanced Color configuration, captures the visible client once,
records the paired observations again, and admits one outer object only when the
nested display and timing receipts are exactly equal, their QPC frequencies agree,
and capture did not begin before the timing post-call bracket. The v2 wrapper
binds each raw configuration report to its exact pre/post topology; it does not
interpret those raw words as HDR, color-managed output, scanout, or light. F4 and
F5 are mutually exclusive. A nonpoisoning F5 storage refusal retains the exact
whole wrapper for a storage-only retry; it never recaptures or resamples
timing/topology/configuration. The live mode remains create-only, so a poisoned
journal needs external typed recovery rather than an in-process retry.

A separate stdlib-only Python oracle now checks a bounded renderer corpus without
calling the Rust renderer: it intersects six cube-face planes per client-raster-pixel
ray, while production projects faces and rasterizes triangles. Rust reconstructs
the oracle's complete implicit background/infinite-depth complement and compares
all 934 post-overlay XRGB words, every depth class, and every finite depth-bit
residual across two named direct-cube fixtures. RGB and depth-class mismatch
vectors are empty; the nine nonzero finite-depth residuals are at most two ULP
under a declared four-ULP bound. Powered mutations of the production face shade
and perspective-depth expression made the gate fail before each was restored.
This is independent evidence for that corpus only: zero yaw/pitch, 90-degree FOV,
and the direct-cube branch. It does not cover aggregate, subpixel-point,
inside-bit, near-plane, arbitrary-camera, scanout, or physical-light behavior.

## Metric registry

[`METRICS.md`](METRICS.md) records 1,112 distinct metric families spanning the
world, observer, rendering, edits, ticks, proof, hashing, performance, future
emergence, and multiscale analysis. The first F3 page now projects the cheap
current world, observer, view, time, and provenance subset into a separately
clipped 400-pixel panel. Each projected family is bound to its readable alias
and full metric-definition tag; the complete catalog remains available for
later pages and offline analysis. This checkpoint does not pretend to collect
all 1,112 values at runtime. Expensive metrics remain explicitly separated from
values safe to snapshot only when a changed composite is needed.

The initial client raster is 1201 x 801 XRGB pixels: the original 801 x 801 game
frame is copied exactly into the left region and the sidebar is rasterized
directly into the right 400-pixel strided region of the same final buffer,
without a second sidebar scratch buffer. Camera projection and the crosshair
use only the simulation region. An uncaptured pointer inside the sidebar cannot
recapture the camera or feed its wheel input. Sidebar self-metrics that depend
on their own raster show the prior successfully submitted facts, avoiding a
self-referential frame hash.

## Hash tags, frame validity, and history modes

[`docs/PROTOCOL.md`](docs/PROTOCOL.md) defines the current protocol boundary:
canonical framebuffer bytes, semantic content tags and name registries,
mint/burn provenance, the bounded storage envelope, the Windows
exclusive-writer/recovery journal, typed coordinate-history admission and
publication gating, receipt modes, the verifier-first
representation-only Edit-to-LadderLang/static-iotaUpper bridge, and the
requirements that remain open before any Proof-of-History, Proof-of-Stake,
currency, or Layer 1 claim would be truthful.

The opt-in research surface now also contains a bounded local
sequential-history transcript. It pins one SHA-256 recurrence, strictly decodes
and recomputes parent-linked segments, anchors the first state to the supplied
coordinate-history genesis only after bounded semantic replay, and requires
exact coverage of every receipt event in order. Separate segment and whole-
chain limits prevent a helper from silently ignoring cumulative policy fields.
A Python known-answer fixture agrees with the Rust recurrence, segment, and
evidence vectors. Open-local admission also carries a history-evidence tag
binding its verified genesis, head, and final world, so equal replay usage
cannot alias distinct histories. This proves only that those local bytes replay to that result;
it does not prove elapsed time, an unavoidable sequential lower bound, the
latest branch, Proof of History, stake, consensus, or Layer 1 status. The
closed mode boundary always refuses a stake-consensus request.

The same opt-in surface now materializes the existing issuance-provenance
episode metric as `CoordinateEpisodeEvidence` v1. Each post-genesis mutable
placement is named by its existing `#bb.event.sha256...` mint tag; a deletion
records the exact consumed mint tag, and a re-placement receives a distinct
tag. The carrier binds a derivation-policy tag plus verified history genesis,
head, final-world, and receipt-ordered episodes—there are no numeric episode
identifiers, balances, owners, transfers, or asset tags. Initially occupied
mutable coordinates remain separately genesis-provenanced rather than being
rewritten as new issuance. This is local history evidence only, not a currency,
Proof-of-History, Proof-of-Stake, consensus, or Layer 1. Its independent
canonical vector is:

```powershell
python -B tools/coordinate_episode_vectors.py
```

The storage, journal, and LadderLang bridge modules are research/proof surfaces,
not hot render-path dependencies. The sequential transcript follows the same
boundary. Unit tests always compile them; external Rust callers enable them
explicitly with `--features protocol-research`. The default game binary
therefore pays no linked-code size for those modules.

[`docs/DISPLAY_AUDIT.md`](docs/DISPLAY_AUDIT.md) preserves the bounded Windows
screen-raster assay, including the two failed controls that exposed rounded
corner clipping and an asynchronous DWM composition race before the exact
full-slice pass.

## Proof capsule

[`proof/`](proof/) is a standalone Lean 4 model over unbounded integer
coordinates. It proves the one-bedrock initial state, bedrock preservation over
every instruction and replay, the universal membership meaning of every
instruction, on/off idempotence, toggle involution, finite support, and ordered
replay composition. A separate representation theorem proves the exact S/K/I,
atom, iotaUpper, and derived tick-count formulas for every finite quoted byte list, with powered
association and bit-order mutants. `proof/LadderLangQuotedByteParity.lean`
additionally runs the complete frozen 112-byte quote against the pinned
LadderLang checkout's own `Lam.costChecked`, including an off-by-one rejection
control.

`proof/ladderlang-transition-v1/` is a separate frozen two-source executable
capsule. It copies and SHA-256 verifies only the pinned `Ladderlang.Core` and
`Ladderlang.Entries` sources, then uses their real bounded reducer to check the
finite mutable `on/off/toggle × on/off` truth table, valid immutable-origin
rows, and explicit `RAN`/`GREW` resource controls. Run `verify.ps1` inside that
directory. It establishes only that finite target under its declared reducer
and Lean dependencies; it does not yet prove that Rust executes a translated
program or establish iota execution cost.

```powershell
cd proof
lake build
```

The capsules do not prove that the chunked Rust implementation refines the
world model or that quoted bytes execute Edit semantics. They make no claim
about camera floats, rendering, hashing, wall-clock scheduling, currency, or
consensus.

## Current boundary

Coordinates are signed 64-bit integers. Memory is allocated only for 16 x 16 x 16 chunks that
contain on bits, and an empty non-origin chunk is released. The origin chunk remains because
bedrock makes the world permanently non-empty. There is no artificial world edge or draw distance,
but real memory and compute are finite. At this checkpoint, views below the one-pixel saturation
level rasterize every on bit; a hierarchical renderer is intentionally left for a later checkpoint
after this interaction model is judged.

The live app currently boots with `Camera::for_observer_spawn()` beside the origin at an
observation distance of `1.0`. The reusable `Camera::for_single_pixel_voxel()` constructor
provides the exact one-pixel setup for a single voxel, and wheel-out or `F` can reach the
one-pixel saturation view, but exact one-pixel startup is not the default live-app behavior at
this checkpoint.

Observer movement and wheel dolly use a complete swept-point test, so a large tick cannot tunnel
through a voxel. Contact stops the whole movement vector; face sliding is not implemented yet.
`F` remains an explicit camera reframe to a safe position outside the world's complete bounds,
not physical travel through the intervening space.
The sweep currently examines every on bit, making it correctness-first rather than the final
near-infinite scaling design. Also, the camera remains absolute `f64`; exact collision geometry over
the entire `i64` coordinate range will require chunk-relative camera coordinates.

## Run

```powershell
cargo run --release

# Start from the origin and apply a text program during tick zero
cargo run --release -- examples/first.bb

# Replay normalized observer actions without opening a window
cargo run -- --simulate-actions examples/observer_walk.tape

# Windows release builds need the console feature for a reliable captured report
cargo run --release --features headless-console -- --simulate-actions examples/observer_walk.tape

# Replay one UI-reachable place/delete cycle without opening a window.
# It ends with only the protected origin on: one local occupancy-on (Mint) event and one
# local occupancy-off (Burn) event.
cargo run --release --features headless-console -- --simulate-actions examples/observer_place_delete.tape

# Opt in to an expensive full-client OS screen-raster audit on each changed frame.
# Use a debug run to retain the detailed terminal receipt; the window title also
# reports Match, Mismatch, or Unavailable.
$env:BITBLOCK_DISPLAY_AUDIT = "1"
cargo run

# Create a fresh bounded display-evidence journal. The target must not exist,
# and this mode is mutually exclusive with BITBLOCK_DISPLAY_AUDIT. Press F4 to
# archive the current committed composite occurrence, or press F5 and then make
# one normal visual change (movement/edit/F3) to archive the next presentation.
Remove-Item Env:BITBLOCK_DISPLAY_AUDIT -ErrorAction SilentlyContinue
$env:BITBLOCK_DISPLAY_JOURNAL_NEW = "$PWD\display-session.bbj"
cargo run --features protocol-research

# Re-read a retained F4/F5 journal without opening the observer window. Quote the
# complete storage-record tag printed by the original archive report.
cargo run --release --features protocol-research,headless-console -- --verify-display-journal "$PWD\display-session.bbj" '#bb.storage-record.sha256...'

# Opt in to one raw composition-timing envelope around every checked presentation.
# This writes content-tagged observations to stderr; the cheap default does not link it.
$env:BITBLOCK_PRESENTATION_AUDIT = "1"
cargo run --features protocol-research
```

The audit compares every visible client pixel after the checked Windows
submission plus `GdiFlush` and `DwmFlush` barriers. It rejects minimized, zero-size, off-monitor,
cross-monitor, DPI-changed, and geometry-raced samples rather than retrying them
away. A match proves one OS-exposed desktop-raster sample, including occlusion;
it does not prove scanout or monitor photons. Continuous 1201 x 801 auditing is
intentionally opt-in because each capture reads 3,848,004 bytes (about 3.67
MiB). The 801 x 801 game-frame tag remains a separate inner artifact beneath
the submitted full-client composite tag. F4 journal mode is one-shot and
create-only: it performs the same bounded GDI/DWM observation, constructs the
canonical artifact, synchronizes one typed record, and rereads it before
reporting `display-artifact-stored`. It does not silently reopen, rotate,
truncate, or overwrite a prior journal.

`--verify-display-journal` is a separate, read-only replay of retained F4/F5
bytes. It requires a full, prior `storage-record` tag as an anchor, rechecks the
complete typed parent-linked carrier and embedded artifact semantics, and prints
the complete retained inventory only for a clean supplied file. A damaged suffix
returns nonzero and any reported valid prefix excludes that suffix. This is local
retained-byte verification only: it
does not prove the file is newest, externally held, compositor-attributed,
scanned out, or visible as physical light. The printed tick-outcome and
frame-input tags are retained references, not a replay of an interaction tape,
world history, or renderer.

The presentation-timing audit calls `DwmGetCompositionTimingInfo(NULL, ...)`
before and after the checked presenter, with every API occurrence bracketed by
`QueryPerformanceCounter`. It also retains `GdiFlush` and `DwmFlush` outcomes.
The resulting fixed 948-byte artifact can classify only session-level timing as
unavailable, comparable without progress, comparable with progress, or
discontinuous/reset. Modern Windows exposes this query at desktop-composition
session scope rather than Bitblock-frame scope, so counter movement never changes
the exact frame's fate from `Unknown`.

F5 also queries the active route set and raw Advanced Color configuration before
and after capture. The canonical
observation retains exact UTF-16 source/target names, source/monitor geometry,
mode and signal words, every clone target route, raw flags, and a bounded query
attempt transcript, while excluding transient HWND/HMONITOR/LUID/source/target
join identifiers. The v2 wrapper additionally preserves opaque per-route
Advanced Color flags, encoding word, and bits-per-channel (or explicit query
unavailability) in the same canonical route order. Query failure is explicit.
The derived route and raw-configuration results are `Stable`, `Changed`, or
`Unavailable`; neither is encoded as a claimed display mode and neither by
itself establishes collection time or displayed-frame fate. ICC/profile state,
applied transform order, scanout, and panel light remain unobserved.

## External-sensor boundary

`physical_sensor` is an opt-in research protocol module, not a hardware driver. Its canonical
`PhysicalSensorObservation` has only three serialized states: `NotApplicable`, `Unavailable`,
and `ReportedSamples`. The record names a full `PresentationOutputArtifact` tag, a fixed policy
tag, a declared (not independently certified) sensor profile, ordered framebuffer coordinates
with expected XRGB words, bounded raw source bytes, and one RGB16 triplet per challenge point.
It does not contain a self-declared “pass.”

A separate `verify_against(parent)` call can return `BoundSampleMatch` only when the caller
supplies the exact named presentation artifact, that artifact structurally verifies and records
an exact complete OS desktop-raster match, every challenge XRGB word equals the artifact's
expected framebuffer word, and every caller-supplied RGB16 channel is within its declared integer
tolerance of `channel8 * 257`. Missing/wrong parents, unavailable GDI captures, bad challenges,
and residuals outside the bound stay `Unbound`, `ChallengeMismatch`, or `SampleMismatch`.

For a future finite sensor assay, `RgbGradientChallenge::from_exact_presentation(parent, k)`
derives up to `k` canonical row-major positions from the already verified expected XRGB slice.
It ranks each pixel by the sum of RGB8 absolute-difference scores to its existing right and lower
neighbours, with score-descending then row/column tie breaking. This makes a small sensor sample
more diagnostic of a wrong pixel mapping; it is not a continuous renderer gradient, a depth or
light measurement, or extra evidence beyond the fully named parent raster. Its separate
stdlib-only vector check is:

```powershell
python -B tools/rgb_gradient_challenge_vectors.py
```

That narrow result still proves neither that an external device produced the numbers nor that
they came from the panel. Device identity, calibration custody, camera-to-panel geometry,
temporal synchronization, ICC/HDR transforms, scanout, and emitted light remain outside the
current evidence. The physical-display-sensor metric therefore remains `N/A` until an actual
external instrument is bound under a future protocol.

The codec's independent, stdlib-only tag vectors can be checked with:

```powershell
python -B tools/physical_sensor_vectors.py
```
