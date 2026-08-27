# bitblock

Bitblock is a binary coordinate space observed through a free camera.

- `(0, 0, 0)` is permanent orange bedrock; it is always on and cannot be broken.
- The observer starts at `(0, 0, -1)`, in the adjacent empty cell facing bedrock.
- Every other unspecified coordinate is off.
- The observer is camera state, not an entity.
- There is no gravity, body, terrain, inventory, or autonomous simulation.
- The observer is a point that cannot enter or tunnel through an on block.
- The framebuffer is written as physical pixels.
- On Windows, the app owns that XRGB slice and admits a submission only when
  `SetDIBitsToDevice` reports every requested scanline; this is checked GDI acceptance,
  not compositor or monitor proof.
- A research-only Windows timing audit can bind that exact composite receipt to raw
  before/after DWM session timing, QPC brackets, and barrier outcomes. Its per-frame
  attribution is always `Unknown`; it does not claim displayed, dropped, scanout, or light.
- A one-shot research bundle can additionally retain bounded active-output topology before and
  after one OS screen-raster capture. Stable routes and matching pixels still do not prove
  per-frame DWM attribution, scanout, or physical light.
- One fixed 60 Hz tick consumes input, changes observer/world state, and produces changed frames;
  the window layer only presents pixels produced by that tick.
- Scroll moves logarithmically between the one-pixel view and a block-filling view.

## Controls

- Crosshair: the hollow center is the exact aimed physical pixel
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
  composition timing, pre/post active-output topology, and one desktop-raster observation; an
  idle request waits visibly and does not resubmit the old surface
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

Headless runs always start from the empty program, permanent bedrock, observer spawn, visible
sidebar, and the default 1201 x 801 output. Before state is constructed, a pure shadow interpreter
checks the complete tape against payload/action limits and cumulative resize, composition, raster,
and world-scan work budgets. Invalid finite accumulation and over-budget work fail closed.

The report contains the tape tag, per-tick outcome/world/history/camera and CPU-frame commitments,
and the final state. Those lines are reproducible projections, not yet one canonical run receipt
binding the tape, build/rules, and result chain. Headless frames are intentionally unsubmitted:
frame admissions and receipt heads remain empty, and no OS raster, scanout, or physical-light
evidence is produced.

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
records. F5 waits for the next normal successful composite presentation and
uses the timing envelope produced around that exact backend call even when the
continuous timing logger is disabled. It then records bounded active-output
topology, captures the visible client once, records topology again, and admits
one outer object only when the nested display and timing receipts are exactly
equal, their QPC frequencies agree, and capture did not begin before the timing
post-call bracket. F4 and F5 are mutually exclusive. A nonpoisoning F5 storage
refusal retains the exact whole wrapper for a storage-only retry; it never
recaptures or resamples timing/topology. The live mode remains create-only, so a
poisoned journal needs external typed recovery rather than an in-process retry.

A separate stdlib-only Python oracle now checks a bounded renderer corpus without
calling the Rust renderer: it intersects six cube-face planes per physical-pixel
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

The initial client is 1201 x 801 physical pixels: the original 801 x 801 game
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
Edit-to-LadderLang/iota-cost bridge, and the
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
atom, iota, and tick formulas for every finite quoted byte list, with powered
association and bit-order mutants. `proof/LadderLangQuotedByteParity.lean`
additionally runs against the pinned LadderLang checkout's own
`Lam.costChecked`.

```powershell
cd proof
lake build
```

The capsule does not prove that the chunked Rust implementation refines the
world model or that quoted bytes execute Edit semantics. It makes no claim
about camera floats, rendering, hashing, wall-clock scheduling, currency, or
consensus.

## Current boundary

Coordinates are signed 64-bit integers. Memory is allocated only for 16 x 16 x 16 chunks that
contain on bits, and an empty non-origin chunk is released. The origin chunk remains because
bedrock makes the world permanently non-empty. There is no artificial world edge or draw distance,
but real memory and compute are finite. At this checkpoint, views below the one-pixel saturation
level rasterize every on bit; a hierarchical renderer is intentionally left for a later checkpoint
after this interaction model is judged.

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

The presentation-timing audit calls `DwmGetCompositionTimingInfo(NULL, ...)`
before and after the checked presenter, with every API occurrence bracketed by
`QueryPerformanceCounter`. It also retains `GdiFlush` and `DwmFlush` outcomes.
The resulting fixed 948-byte artifact can classify only session-level timing as
unavailable, comparable without progress, comparable with progress, or
discontinuous/reset. Modern Windows exposes this query at desktop-composition
session scope rather than Bitblock-frame scope, so counter movement never changes
the exact frame's fate from `Unknown`.

F5 also queries the active route set before and after capture. The canonical
observation retains exact UTF-16 source/target names, source/monitor geometry,
mode and signal words, every clone target route, raw flags, and a bounded query
attempt transcript, while excluding transient HWND/HMONITOR/LUID/source/target
join identifiers. Query failure is explicit. The derived route result is
`Stable`, `Changed`, or `Unavailable`; it is not encoded and does not by itself
establish collection time or displayed-frame fate. ICC/HDR state, output bit
depth, applied transform order, scanout, and panel light remain unobserved.
