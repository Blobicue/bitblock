# Bitblock protocol checkpoint

This document fixes the boundary between Bitblock's current local simulation
and the proposed content-addressed history system. It deliberately separates
what can be implemented and checked locally from claims that require
cryptographic assumptions, independent custody, external instruments, or a
distributed consensus protocol.

## One authority shape

Every authoritative object is identified by the full digest of its canonical
content:

```text
#bb.<kind>.<suite>.<full-lowercase-base32-digest>
```

The binary digest, not its text rendering, is the stored identity. `kind`, hash
suite, schema tag, canonical payload length, and payload bytes are all inside
the hashed domain. A truncated tag is display-only and cannot authorize or
resolve anything.

Coordinates, dimensions, lengths, and ticks are values. They are not object
numeric identifiers. Receipt order comes from parent-tag ancestry; the network is named by its
genesis tag; rules, schemas, renderer profiles, palettes, registries, keys, and
policies are content-tagged objects.

Readable names are resolved through a versioned registry:

```text
RegistryPatch {
    parent_registry_tag,
    sorted (lowercase_ascii_name, bind(target_tag) | tombstone) entries
}
```

Every historical receipt pins the registry tag it used. A mutable `current`
registry is a user-interface convenience, never protocol authority. The
metric catalog now uses readable `#bb.metric.<family>.<name>` registry aliases;
`docs/metrics/registry.tsv` binds every alias to the full SHA-256 tag of its
canonical Markdown definition. Neither an alias nor a truncated digest is
protocol authority.

## Hash-suite boundary

Protocol authority currently uses SHA-256 through a pinned RustCrypto
implementation. The domain wrapper has a golden vector independently generated
with Python's standard `hashlib`; cross-implementation corpus parity remains a
separate continuing assay.

The audited local Aethor TSH-512/PDH-512 implementations are binary `u64`
XOR/rotate/multiply constructions, not implementations of the separate
balanced-trit T1-T4 document. They cannot be a Bitblock authority lane. A
powered padding control found the chosen collision below on the current Python
and bit-exact Rust implementations:

```text
x = b"A" * 30
y = x || 0x80
TSH(x) = TSH(y)
       = 40fba4c232257f3761bc1c11815f8dc1389c0dd3f2e78119669c19962ebaa17e
         e4716982d4164d5c543e1b27b1307970af617ba8bcb73ad09899de7870c73545
```

The padding appends `0x80`, zero-fills, and overwrites the final byte with
`0x01`; at this boundary the separator is erased. Aethor node composition also
uses `left || 0x00 || right` without length framing, so `(A, 0x00||B)` and
`(A||0x00, B)` produce the same child bytes even under a perfect hash. PDH's
exposed `depth` argument is unused. The tryte-vm IterHash corpus remains useful
only as a structural assay: its own tests show short positionwise/carry cycles
and explicitly make no security claim.

Therefore SHA-256 remains the only authority suite. TSH, PDH, and IterHash may
appear only as labeled non-authoritative diagnostics with their exact source
artifact pinned. Agreement cannot promote them; any future ternary authority
requires a different frozen construction, unambiguous canonical framing,
adversarial controls, independent cryptanalysis, and an explicit protocol
migration.

A digest commits to exact bytes. It does not prove that those bytes are a valid
world, frame, transition, name, key, or consensus decision. Those are separate
verification predicates.

## Canonical framebuffer

`src/frame.rs` defines the first executable protocol boundary:

```text
u16_be(domain_length)
domain = "bitblock/frame/xrgb8888-row-major/v1"
u32_be(width)
u32_be(height)
u64_be(width * height)
u32_be(pixel[0]) ... u32_be(pixel[P-1])
```

Pixels are complete `0x00RRGGBB` words in row-major order. The high byte is
included in canonical bytes and must be zero, so invalid storage cannot alias a
valid RGB frame. The production path validates the shape, scans every submitted
CPU frame, and copies the complete slice into its platform surface. On Windows,
that surface is a Rust-owned top-down XRGB buffer submitted through
`SetDIBitsToDevice`.

Frame validity is layered:

1. **Structure:** dimensions, length, storage format, high byte, and allowed
   palette/gradient rule are valid.
2. **Deterministic renderer:** identical canonical render inputs under one
   declared renderer/build profile produce byte-identical frames. Occupied-bit
   traversal permutations must agree.
3. **Semantic renderer:** an independent specification agrees at every pixel.
   Re-running the same implementation is not independent verification.
4. **Mapped copy:** exact complete-slice copy into the selected platform surface
   returned successfully.
5. **Submission:** on Windows, `SetDIBitsToDevice` returned exactly the requested
   nonzero scanline count. The result is checked before either frame receipt is
   published. This proves GDI accepted the complete top-down DIB call, not that
   DWM used it or that a monitor displayed it. Other platforms still use their
   Softbuffer backend and retain only that wrapper's return boundary.
6. **OS-exposed desktop raster:** the opt-in GDI screen-DC audit checks its own
   API results and compares the complete visible client rectangle. This is one
   sampled OS raster, including occlusion; it is not guaranteed scanout.
7. **Physical display (future / currently N/A):** a physical-display claim would require a
   separately retained, calibrated external instrument whose samples agree under a declared
   uncertainty model. No such record is currently admitted, so this layer is `N/A`, not pass.

The process cannot prove photons on the monitor from its own framebuffer. OS
composition, scaling, color management, occlusion, cable, and panel behavior
sit outside that trust boundary.

### External-sensor observation carrier

The feature-gated `PhysicalSensorObservation v1` carrier is a bounded bridge for a future
external instrument, not a physical-display certificate. It serializes only `NotApplicable`,
`Unavailable`, or `ReportedSamples`; no canonical payload can declare that it passed. A
reported-sample object contains a full typed `PresentationOutputArtifact` tag, the fixed
sensor-comparison policy tag, a declared profile tag and RGB16 tolerances, an ordered list of
`(x, y, expected_XRGB)` challenge points, one RGB16 sample per point, and at most 64 KiB of raw
attachment bytes. The attachment is hash-bound but is not parsed as a success witness.

`verify_against(parent)` first re-verifies the carrier, then requires the caller-supplied parent
to have the named full tag and pass its own display/timing/topology checks. It requires the
nested display receipt to be `Match` at `Complete` capture stage, checks every challenge position
against the parent's authoritative expected XRGB slice, and compares channels only in the integer
relation `abs(sample_u16 - expected_u8 * 257) <= declared_tolerance_u16`. Any missing/wrong
parent, nonexact desktop capture, challenge drift, out-of-bounds coordinate, count mismatch, or
residual breach yields a named refusal. Only then may it return `BoundSampleMatch`.

`BoundSampleMatch` means that supplied numbers satisfy the declared bounded comparison against
named pixels of one already-matching OS-raster artifact. It does not establish the provenance of
the numbers, an independently held/calibrated instrument, physical camera/panel mapping, timing,
scanout, or photons. No external sensor record has been admitted, so the physical-display metric
is still `N/A`.

`RgbGradientChallenge::from_exact_presentation(parent, k)` is a deterministic optional selector
for a future finite sample. It first requires the same verified `Match`/`Complete` parent, scans
the parent expected XRGB pixels, scores each pixel from its existing right/lower RGB8 absolute
differences, retains the top `k` with a declared stable tie rule, then emits the usual canonical
row-major challenge points. The selector's policy and selected challenge both have full tags.
It makes an eventual sample more discriminating for coordinate/color mismatches only; it does not
measure a physical gradient, establish a sensor mapping, or strengthen the OS-raster boundary.

The implemented frame chain separates input, output, and occurrence:

```text
FrameInput = canonical_world_tag
             || all stored camera fields as exact f64::to_bits values
             || viewport
             || declared renderer-profile tag
             || overlay-input tag

FrameReceipt v1 = parent_frame_receipt_tag | nil
                  || source_tick
                  || FrameInput tag
                  || canonical final-frame tag
```

The sidebar adds a second, outer frame boundary without widening the camera's
viewport:

```text
MetricSnapshot = exact current cheap metric fields
                 || prior submitted frame/display heads
                 || prior successfully submitted sidebar-raster facts

CompositeFrameInput = inner game FrameInput tag
                      || exact inner game Frame tag
                      || composition OverlayInput(MetricSnapshot tag)
                      || simulation and output dimensions

CompositeFrameReceipt v1 = parent composite receipt | nil
                           || source_tick
                           || CompositeFrameInput tag
                           || canonical full-client Frame tag
```

At the initial size, a full composition copies every row of the 801 x 801 game
frame exactly to `x=0..800` and draws the 400 x 801 sidebar only at
`x=801..1200`. A sidebar-only recomposition reuses that prefix only when its
selected game-frame tag equals the compositor's cached full-composition tag;
otherwise it fails closed. The simulation renderer, camera projection,
crosshair, and inner game-frame tag never receive the sidebar width. Separate
powered tests check every copied word, exact reuse against a frozen full-copy
reference, stale-tag refusal, and resize invalidation. One sidebar-pixel mutation
changes the composite tag while leaving the inner game-frame tag unchanged.

Only a changed inner game frame whose composite backend submission returned
success enters the inner chain. Sidebar-only F3 recomposition advances only the
outer chain. Idle ticks, redraw requests, failed submissions, and intermediate
dirty composites overwritten before presentation create no receipts. A failed
backend submission advances neither chain. The renderer and composition
profiles are declared semantic policies, not executable hashes.

A sidebar cannot display the hash or raster facts of the same final frame being
hashed without a dependency cycle. Composite frame `n` displays only heads and
self-metrics completed before `n`; frame `n` can therefore be named in a later
snapshot. The current world tag is safe because it is finalized before render.

## Tick-outcome sidecar checkpoint

`src/tick.rs` freezes the live additive `TickOutcome` v3 proof object. Every
completed `App::fixed_tick`, including an idle tick, now produces exactly one
structurally verified sidecar. The later live-receipt migration retains the
complete in-memory chain from tick zero rather than only its current head:

```text
kind   = tick-outcome
schema = bitblock/tick-outcome/v3

TickOutcome = optional parent TickOutcome tag [33]
              || tick ordinal u64 [8]
              || pre-world World tag [32]
              || pre-history Receipt tag [32]
              || input flags u16 [2]
              || mouse-X, mouse-Y, wheel f64 bit lanes [24]
              || simulation/output dimensions [16]
              || accepted-action counters, one u32 per lane [40]
              || attempt count u64 [8]
              || ordered variable attempt records
              || post-world World tag [32]
              || post-history Receipt tag [32]
              || post-camera ten f64 bit lanes [80]
              || optional game FrameInput/Frame pair [65]
              || optional composite FrameInput/Frame pair [65]
```

The fixed payload is 469 bytes. An unresolved aimed edit adds one byte; an
edit-bearing attempt adds one disposition byte plus the existing canonical
25-byte `Edit`. The complete closed disposition alphabet is:

```text
0  aimed delete unresolved
1  aimed place unresolved
2  aimed delete executed (must carry Off)
3  aimed place rejected for enclosing the observer (must carry On)
4  aimed place executed (must carry On)
5  exact edit rejected for enclosing the observer
6  exact edit executed
```

Codes 7--255 are refused. There is no aimed-delete rejection code because an
aimed delete resolves only to `Off`, while the enclosure guard rejects only an
edit that would turn the observer cell on. Executed no-ops remain executed
records; replay against the pre-world derives `AlreadyOn`, `AlreadyOff`, or
`BedrockProtected` without encoding a second account of the effect.

Input flag bits 0--5 are forward/backward/left/right/up/down, bit 6 is the
fit-world request, bits 7--8 are the game/sidebar dirty states at tick entry,
and bit 9 is sidebar visibility. Bit 10 is a viewport-refit request queued by a
resize that occurred while the complete world was at one-pixel depth; bits
11--15 are reserved zero. The post-camera lanes use exactly the existing
`FrameInput` order: position xyz, yaw, pitch, anchor xyz, observation distance,
and vertical FOV. Pre-camera is not repeated: for a non-genesis outcome it is
the resolved parent's post-camera; tick zero requires the separately fixed
observer-spawn genesis context. The schema name is the fixed-transition policy,
so any change to tick ordering, constants, or meaning requires a schema version
change rather than an extra policy digest that could not prove executable
conformance.

The strict codec refuses missing or extra bytes, count overflow, truncated
edits, unknown discriminants, nonzero absent padding, reserved flags,
non-finite controls/camera lanes, inconsistent dimensions, impossible aimed
operations, an impossible observer-rejected `Off`, wrong nested tag kinds, and
frame-presence claims inconsistent with dirty/fit/mouse/sidebar inputs or a
changed world. It also requires executed-attempt presence to match history-head
advancement exactly, including executed no-ops, and forbids a changed world
without that history advancement. The
link verifier requires a parent only after tick zero, exact
parent identity, contiguous ordinal, and parent-post/child-pre world and
history equality. A child whose post-camera differs from its parent's
post-camera must also commit a game frame. These are structural and
content-chain checks. Semantic
admission still requires resolving the parent, replaying camera/edit/history
behavior from the fixed genesis context, and verifying referenced frame input
and pixel objects.

`tools/tick_outcome_vectors.py` is a Python-stdlib reconstruction with its own
strict decoder. It independently pins four decode/re-encode known answers:

```text
genesis-idle        payload 469  preimage 550  #bb.tick-outcome.sha256.mdrfmhsgskam6cmt4n3zncrclswc26fjfowva6e7itavrs7zx2tq
mixed-all-seven     payload 601  preimage 682  #bb.tick-outcome.sha256.nkazbofarz6mjgss625grz4cxx62itnqhbplgcwwjyrr3657mqda
control-lanes-only  payload 469  preimage 550  #bb.tick-outcome.sha256.ckeafgihrlyc57iiyef4mjvkqrxsyfbiucrrb4zndfvp57h45xhq
sidebar-only        payload 469  preimage 550  #bb.tick-outcome.sha256.bzqggwzw2eygclvjcidemqtrhdpdozhioimfzrl44mbphbgoqvna
```

`control-lanes-only` is the vector v3 was added for: it shares every other byte
with `genesis-idle` and differs only in four accepted occurrences across three
control-action kinds -- two sidebar toggles, one resize, and one motion clear.
Under v2 the two payloads were identical.

### Why the counters, and why only counters

The lanes are the interaction-tape opcodes, so one tape record and the tick that
consumes it count into the same slots and reconcile without a translation table.
Three accepted-action kinds -- `ToggleSidebar`, `Resize`, `ClearMotion` -- apply the
moment they are accepted rather than inside the edit fold. Their effects were
already committed, through the sidebar flag and the viewport dimensions, but
nothing recorded that an action had been submitted, so no count could be checked
against a tape and no replay could be shown to have consumed what the live run
consumed.

Counts, not ordered per-event records, are all this commits. The counts still
reflect event coalescing because one accepted event increments one lane; they do
not retain event values or order, and remain a fixed 40 bytes. Per-lane totals
reconcile exactly against a tape record. The rule the decoder enforces is that submitted edits
equal ordered dispositions: every edit counted has one attempt record, and every
attempt record had a submission.

Rust additionally exhausts all 2,048 valid flag combinations and refusal
branches for option padding, invalid attempt codes/counts/operations, length,
non-finite or unreachable context-free camera ranges, dimensions, frame
presence, local decode limits, and chain discontinuity. The Python decoder runs
47 malformed-payload controls across the same field families. Its default local
resource policy refuses payloads over 16 MiB or one million attempts before
allocation; this is a bounded-verifier refusal, not a verdict that larger bytes
are noncanonical. Successful decode uses fallible attempt reservation and a
streaming canonical-byte comparison rather than allocating a second payload.
A powered mutation of
the production attempt-count encoder from big-endian to little-endian made the
cross-language known-answer gate exit nonzero; restoring the subject returned
it to green.

The live fold captures pre-world/history, all entry flags and accumulated
controls, and all four dimensions before mutation. It consumes viewport refit,
mouse, movement, explicit fit, wheel, and ordered edits; commits coordinate
history; renders optional game/composite outputs; constructs and link-verifies
the outcome; reconstructs any game `FrameInput` tag from post-world,
post-camera bits, simulation dimensions, and the exact renderer/crosshair
policy tags used for that invocation; derives legacy pending-frame
`source_tick` from that admitted outcome; then advances the ordinal. Actual
world tags are scanned and compared with the coordinate-history cache at both
tick boundaries, and the actual pre-camera must equal the preceding outcome's
post-camera. Attempt records preserve source identity,
rejection, execution, and state no-ops. Tests show the retained legacy
`TickBatch` is exactly the lossy projection `(tick, attempt count, executed
edits)` of the richer outcome. `FrameReceipt` remains byte-identical v1; the
isolated v2 codec below does not migrate the live ledgers.

Resize originally refit the camera immediately outside `fixed_tick`. That made
the proposed parent-post-camera to child-pre-camera relation false. No v1
outcome was ever live-produced; schema v1 was rejected before integration.
Resize now queues bit 10 and leaves the camera unchanged until the next fold,
where refit is consumed first, preserving the prior input order. The rejected
preproduction binary is retained only as failed-history evidence at
`target/tick-codec-v1-checkpoint/release/bitblock.exe` (1,033,216 bytes,
SHA-256 `A3B3902BCDD66FBF1628551E7FED953D0E29434C0137D34169564A854AA86255`);
its TickOutcome bytes are not admissible as v2.

The admitted sidecar checkpoint is retained separately at
`target/tick-outcome-v2-sidecar-checkpoint/release/bitblock.exe`: 1,070,080
bytes, SHA-256
`E94B2948F076CBFED641AD4CC740F445DD72A921D59026537A3E6CBC6D916962`.
It is 38,912 bytes larger than the prior snapshot-stream default artifact and
36,864 bytes larger than the rejected codec-v1 artifact. Against the former,
`objdump -h` reports `.text +31,008`, `.rdata +6,976`, `.pdata +444`,
`.xdata +800`, and `.reloc +84` bytes; `.data`, `.eh_fram`, `.bss`, `.idata`,
`.CRT`, and `.tls` retain their sizes. Direct section-byte hashing leaves only
`.eh_fram` and `.tls` byte-identical; equal-sized address-bearing sections are
not claimed identical. The increase is the measured cost of the live outcome
fold, fail-closed boundary scans, contextual game-input binding, and their
release-path diagnostics before a separate equivalence-gated optimization.

The checkpoint gate passed 178 library and 25 application tests in both default
and `protocol-research` all-target builds, clippy across all targets/features
with warnings denied, formatting, all six Python vector tools, the exact 1,112-
definition metric-registry audit, and `lake build`. Powered subject mutations
separately demonstrated failure on camera ancestry, pre/post world identity,
world/frame/history contradictions, camera/frame linkage, the contextual game
input digest, and attempt-count endianness before each subject was restored and
regated.

The outcome contains no current or later frame/display receipt. A composite
commitment can transitively contain only predecessor receipt heads through its
metric snapshot, so the dependency remains historical and acyclic. Full
semantic admission still needs an independently executable replay verifier;
the policy-parameterized game-input equality proves digest agreement but not
referenced-object availability, renderer execution, or pixel correctness. The
producer, structural verifier, and current tests share Rust transition code and
therefore do not constitute an independent proof of all inputs.

## Frame-receipt v2 codec and structural-admission checkpoint

`src/frame_v2.rs` adds a byte-distinct receipt codec without changing or
reinterpreting `FrameReceipt` v1:

```text
kind   = frame-receipt
schema = bitblock/frame-receipt/v2

FrameReceiptV2 = optional parent FrameReceipt tag [33]
                 || stream code [1]
                 || declared TickOutcome tag [32]
                 || FrameInput tag [32]
                 || final Frame tag [32]

stream = game:0 | composite:1
payload bytes = 130 exactly
```

The stream byte is hashed receipt content, not an in-memory label: removing it would
allow one serialized receipt to be relabeled between the inner game raster and
the full client composite. The strict decoder rejects every length other than
130, unknown parent markers, nonzero absent-parent padding, and unknown stream
codes. Construction rejects wrong typed nested tags, and object verification
also recomputes the cached receipt tag. Resolving referenced objects, their
schemas, and their semantic relationship remains admission work. The shared
`frame-receipt` kind intentionally does not
identify the schema version by itself, so a bare tag cannot be admitted without
a typed, version-aware resolver. V1 and v2 ancestry must remain separate typed
epochs.

`tools/frame_receipt_v2_vectors.py` independently reconstructs the wrapper,
strictly decodes and re-encodes both option/stream shapes, and pins:

```text
parent-absent  game       payload 130  preimage 213  #bb.frame-receipt.sha256.5izrwvevcldss6sdjwnj7lqq6rphz2h6re7oe4helz5mft6crnhq
parent-linked  composite  payload 130  preimage 213  #bb.frame-receipt.sha256.yq44ktjoqs3axwvxvkesiatpwldh7mmgdzoecer47h5rzpkekejq
```

Six malformed-byte controls are refused. A powered subject mutation that made
the receipt hash treat every stream as `game` left the first vector unchanged
but made the linked composite vector fail; restoring the stream byte returned
the gate to green. The existing v1 105-byte Python-derived Rust vectors and
their two exact tags remain unchanged.

The codec checkpoint passed 181 library and 25 application tests in both the
default and `protocol-research` all-target configurations, strict Clippy over
all targets/features, formatting, all seven Python vector tools, the exact
1,112-definition metric audit, and the Lean build. The fresh default release is
retained at
`target/frame-receipt-v2-codec-checkpoint/release/bitblock.exe`: 1,070,592
bytes, SHA-256
`772E45D526B09CA1595DB267A5C59A127625E7384684C10CE8BC54E6CEE92AB3`.

One powered release isolation removed only the `frame_v2` module export, built
into a separate target, and then restored the source. That control is retained
at `target/frame-receipt-v2-unlinked-control/release/bitblock.exe`: 1,070,080
bytes, SHA-256
`34D4E663623DD1533488051948FE45A9A36D5980BEC53D1102B123999FD7F9D6`.
Relative to the unlinked control, the codec artifact is exactly 512 file bytes
larger; PE virtual-size deltas are `.text +432`, `.rdata -64`, and `.xdata +4`
bytes, with every other section size unchanged. This is a linked-artifact size
measurement, not proof that the unused codec executes in the live producer.

Updating four existing metric definitions to record this coverage did not add
a metric family: the registry remains exactly 1,112. The one displayed binding
among them reminted the sidebar and composite policy identities to
`#bb.policy.sha256.cwtihnm5mcsld752n5ofhoa63lesoy3d5j2xhoz3g422dbhradrq`
and
`#bb.policy.sha256.q2yfuzavdbxwzyakl2m5zdx5njbbq2qyhdjuvuknv7q6bfcnna6a`.
Their independent Python reconstruction passes. No new live screen-capture
assay was run, so this checkpoint does not claim visual equivalence to the
prior policy identity.

The next isolated gate adds `FrameLedgerV2` without changing the codec or live
producer. A ledger is fixed to one committed stream and one complete,
structurally verified tick-zero outcome. `admit_derived` derives the receipt
parent, outcome, input, and frame from that endpoint. `admit_existing` accepts a
typed decoded v2 receipt, but separately recomputes its tag, checks its exact v2
ledger parent and fixed stream, resolves the endpoint, and requires the exact
selected game/composite pair.

Increasing outcome ordinals is never used as ancestry authority. Each stored
`FrameAdmissionV2` owns every strict `TickOutcome` successor after the prior
admitted frame outcome, including otherwise idle or unpresented ticks and the
new endpoint. Admission calls `TickOutcome::verify_successor` on every edge;
omission, reordering, noncontiguous ordinals, state discontinuity, or a
higher-tick fork therefore fails. The first receipt may reference the configured
tick-zero anchor with an empty successor list; every later receipt must advance.
`verify()` replays the structural genesis, every stored edge, every v2 receipt
parent, and every selected frame pair. Because the ledger contains only typed
v2 receipts and derives its head from them, a same-kind v1 receipt tag cannot
seed or replace v2 ancestry.

Every returned validation error occurs before the one reserved/pushed admission,
so logical ledger state remains unchanged. The default local per-admission work
policy refuses more than one million successor outcomes or one million total
attempt records. Exceeding those limits is a bounded-verifier refusal, not a
claim that the path is noncanonical. Retained evidence costs
`O(successor outcomes + attempts)` cumulatively per stream; the present design
can duplicate outcome objects between game and composite ledgers. That is the
correctness-first baseline for a later shared content store or compact resolver.

Nine focused Rust tests cover anchor admission, a three-outcome skipped path,
full replay, omitted/reordered/forked edges, same-kind v1 parent substitution,
stream and selected-pair mismatches, absent commitments, non-advancement, path
limits, evidence tampering, and atomic refusal. Powered mutations separately
swapped the stream resolver and replaced edge verification with standalone
object verification; the selected-pair and omitted-edge tests failed before
both subjects were restored.

The structural-admission checkpoint passed 187 library and 25 application
tests in both the default and `protocol-research` all-target configurations,
strict Clippy over all targets/features, formatting, all seven independent
Python vector tools, the exact 1,112-definition metric audit, and the Lean
build. Its fresh default release is retained at
`target/frame-receipt-v2-admission-checkpoint/release/bitblock.exe`: 1,070,592
bytes, SHA-256
`5DB137243222FC2781A1953B7A29DB56308690ACC0E9B658F502E8972FE2281C`.
Relative to the codec-only release, total file size is unchanged; PE
virtual-size deltas are `.text +32`, `.rdata +32`, `.pdata -12`, `.xdata -12`,
and `.reloc +12` bytes, with every other section size unchanged. The changed
release fingerprint and linked section layout do not establish that the
default executable invokes the admission ledger; live production remains the
next separate gate.

Recording this new coverage updated two existing metric definitions without
adding a family; the catalog remains 1,112. Because frame provenance is one of
the displayed authority bindings, the sidebar and composite policy identities
for that structural-admission checkpoint were
`#bb.policy.sha256.e52l5e4zgxb2iccczvcfxr5k3w5wciflshw3dwe6vyvbfl6ls7ia`
and
`#bb.policy.sha256.mjjsaavrkokgfxsp3h5qlpcvrci4aegpl2yhy7hevmguinmsw4vq`.
Their Python reconstruction is gated, but no live screen-capture assay was run
for this policy remint.

The live migration is additive and keeps that assurance boundary explicit.
The app now pre-reserves and retains every completed `TickOutcome`, including
idle, coalesced, and unpresented ticks, instead of retaining only the head. Each
pending game or composite frame carries both its legacy source-tick projection
and exact outcome tag. Before presentation, that pair is resolved against the
retained chain and checked against the selected outcome commitment. A sidebar-
only tick may therefore leave game pending at an older endpoint while composite
advances; the two paths are never collapsed into one cursor.

`admit_frame_pair_derived` provides one paired publication boundary. It verifies
both ledger roles and both complete successor paths, constructs both receipts,
and reserves both admission slots before calling the backend exactly once. A
candidate or backend refusal changes neither logical ledger. After backend
success, only pre-reserved `Vec::push` operations remain. The legacy v1 ledgers
are cloned and validated before that boundary, then swapped in after success as
a compatibility sidecar; this preserves their exact bytes and downstream APIs
without allowing a v1 protocol error to occur after display submission.

Application tests cover backend refusal plus retry, tick-zero empty paths,
mixed-age F3 endpoints, and a later game catch-up path that includes the
composite-only tick. Library tests cover valid paired publication, composite-
only admission, invalid second candidates, backend refusal, and swapped roles.
A powered mutation that published the prepared admissions before checking the
backend result made the backend-refusal test fail; restoring backend-before-
publication returned it to green.

The live-publication checkpoint passed 190 library and 28 application tests in
both the default and `protocol-research` all-target configurations, strict
Clippy over all targets/features, formatting, all seven independent Python
vector tools, the exact 1,112-definition metric audit, and the Lean build. Its
fresh default release is retained at
`target/frame-receipt-v2-live-checkpoint/release/bitblock.exe`: 1,121,792 bytes,
SHA-256
`0BADED8C4F4C0AE4B83EE5CEA48D8889F7D703973F3093241D231D71D08B2BA0`.
Relative to the structural-admission checkpoint, the executable grew by 51,200
bytes; PE virtual-size deltas are `.text +42,832`, `.rdata +5,600`,
`.pdata +792`, `.xdata +1,400`, and `.reloc +156` bytes, with every other
section size unchanged. The retained executable contains the distinct
`bitblock/tick-outcome/v2`, `bitblock/frame-receipt/v2`, and legacy
`bitblock/frame-receipt/v1` schema strings. This artifact receipts the live
producer migration; it does not widen the semantic-proof boundary below.

This is live in-memory structural publication, not independent semantic proof.
`verify_successor` and the ledger still share Rust transition predicates; tick
zero is checked structurally but not against the independently fixed observer/
camera genesis context. The retained outcome chain is unbounded and not yet
durable; successor objects are cloned into each stream ledger, and the staged
v1 compatibility ledgers are cloned per presentation. Shared content storage,
pruning/equivalence evidence, independent tick replay, referenced frame-object
resolution, renderer correctness, pixel semantics, and display evidence remain
separate gates. At this checkpoint the downstream v2 display and optimization
consumers were also still separate; the later typed-consumer checkpoints below
close those two in-memory API migrations without widening the proof boundary.

Updating nine existing metric definitions to reflect full-chain retention and
live v2 publication added no family: the catalog remains exactly 1,112. The
displayed frame-provenance definition reminted the current sidebar and composite
policy identities to
`#bb.policy.sha256.5e5j546z4g25phfwxczspagzb67zss66pw7b5ughm63cuq4mwddq`
and
`#bb.policy.sha256.nvdzavsoiecgws3esn2jnxtmdo553u3zuxkl7v4bnbpfmypbcc4q`.
Their independent Python reconstruction is gated. No live screen-capture assay
has been run for this policy identity.

### Typed display provenance

The next additive boundary leaves `DisplayReceipt`/`DisplayLedger` v1 and its
historical exact tags untouched. `DisplayReceiptV2` uses the distinct
`bitblock/display-receipt/v2` schema and a 378-byte payload. Instead of accepting
an unqualified `FrameReceipt`-kind tag plus a separately supplied expected frame,
it stores the complete canonical 130-byte `FrameReceiptV2`. Admission verifies
that typed object, requires its stream to be `Composite`, and derives the
expected framebuffer tag from it before validating the complete capture facts.
The live Windows audit receives this object only from the successfully committed
paired v2 publication. A bare same-kind v1 tag therefore cannot cross the new
API boundary.

The presented-frame observation names both epochs explicitly: the unchanged v1
compatibility receipt and its committed v2 receipt. Sidebar reuse reads the v2
input binding, while the existing sidebar predecessor field deliberately remains
a v1 compatibility consumer. The optimization assay has since moved to v2 as a
separate isolated gate. DisplayReceipt v2 still verifies only the typed receipt's
codec, composite role, and frame binding; it does not independently replay or
persist the referenced TickOutcome ancestry.

An independent Python-stdlib reconstruction pins the parent-absent composite
vector at 378 payload bytes and 465 complete canonical-preimage bytes:

```text
payload SHA-256  56757bc08e43022b5c22253819ba157b207931bd7591aa32d0be2830be5930cc
receipt tag       #bb.display-receipt.sha256.6y74whydlo2cdhs5pl4fuwp66iwpqobgivpkrpeln2ntpni545pq
```

Five malformed/schema-role controls reject unknown or noncanonical display
parents, a game-stream receipt, truncation, and trailing bytes. A powered subject
mutation that inverted the composite-stream guard made the wrong-stream atomic
refusal test fail before the guard was restored and the test returned green.
The existing DisplayReceipt v1 KAT remains unchanged.

### Typed optimization ancestry

The research-only optimization-equivalence authority now uses only
`FrameLedgerV2` and `DisplayLedgerV2`; it does not retain a parallel selectable
v1 path. Both optimization schemas advanced without reinterpreting old bytes:

```text
bitblock/optimization-equivalence/output/v3
bitblock/optimization-equivalence/bounded-corpus/v3
bitblock/optimization-equivalence/frame-ledger-evidence/v1
```

For each candidate and reference, the assay checks the exact structural genesis,
every admitted successor outcome, and every typed receipt. It requires the game
and composite roles, binds tick zero to the case's initial world/history,
matches every receipt-bearing outcome to the verified coordinate history by
tick, parent, post-state, and ordered executed edits, and requires the endpoint
world/history to equal the artifact. The game endpoint is reconstructed from
its post-world, post-camera, renderer profile, and overlay. Game and composite
ledgers must carry the same complete outcome evidence; a same-frame receipt from
another outcome is not interchangeable. The synthetic display stores a complete
`DisplayReceiptV2` and must name the exact committed composite receipt object,
not merely an equal framebuffer tag. Each output lane commits a domain-separated
full-ledger evidence tag containing the stream role, length-prefixed canonical
genesis, admission count, complete typed receipt bytes, successor counts, and
every length-prefixed canonical `TickOutcome`; local verifier limits are excluded.
The implementation computes the complete payload length with checked arithmetic
and streams those bytes directly into SHA-256 rather than materializing a second
ledger. An intermediate successor mutation that preserves the selected final
frame moves this evidence tag and the output receipt.

Twelve optimization tests cover the bounded fixture, history/pixel/cache
refusals, typed stream roles, alternate but structurally valid ancestry,
coordinate-history and every rendered game-input binding, reconstructed
composite input, single matched synthetic-display admission, display atomicity,
v1/v2 domain separation, fail-fast rejection of missing case/identity metadata
before artifact validation, and one execution-bound two-arm compositor assay.
A powered subject mutation inverted the cross-stream ancestry guard; the
dedicated test failed before restoration and returned green afterward.

The unreleased v2 draft committed only typed receipt heads and was superseded
before admission; its final draft vectors remain frozen as rejected history.
An independent Python-stdlib vector reconstructs the exact v3 outer encodings
from fixed upstream content digests:

```text
output   290 bytes  payload SHA-256 6d8b7eaede668663b8ed5a4100338bade7ba271122d5d1902735c8696a7e26e4
                     #bb.receipt.sha256.qe3pcc7hfvgo2pzaxfzt4ofn6j4235mov4bnjmq6ctf4cflwfqxq
receipt  256 bytes  payload SHA-256 7aa852a6a29354bd3cb69a9e1577fa5e8f5f3a5af530897267db4705a6406ec8
                     #bb.receipt.sha256.mekeim44seopxqjyqoh4pei6ww5wi3tcy3zr34z24ppoeazclxea
```

The rejected v2 tags are `#bb.receipt.sha256.yclrpejhfo4pop4ldvowaomk4ywsfbodxj7436iil5amq7n7mkyq`
and `#bb.receipt.sha256.wxmct7gif5ygiueryxymilcsfdlwoncaya5ucxqj65tuvxqr5zta`.
Six controls preserve those v2 vectors and change the v1/v3 domains,
composite-presence lane, or typed receipt positions. This independently checks the new
outer byte framing; it does not reconstruct the upstream world, history,
renderer, or TickOutcome semantics. The assay remains shared-Rust structural/
semantic evidence over a named finite case, not independent replay, universal
renderer equivalence, or literal scanout proof. Artifacts and implementation/
policy identity tags supplied to the generic assay remain caller-controlled;
the assay alone does not prove that the named implementation produced them.
The strided-compositor checkpoint below adds one local execution-bound harness
for its two named arms, without widening that result into generic custody.

The typed-display checkpoint passed 192 library and 28 application tests in
both the default and `protocol-research` all-target configurations, strict
Clippy over all targets/features, formatting, all eight independent Python
vector tools, the exact 1,112-definition metric audit, and the Lean build. Its
fresh default release is retained at
`target/display-receipt-v2-live-checkpoint/release/bitblock.exe`: 1,119,744
bytes, SHA-256
`E2267A82C5D648D528B1E21DEA38C8F58A42E23ADF195C1BB2417416222569AE`.
Relative to the live FrameReceipt-v2 producer checkpoint, the executable is
2,048 bytes smaller; PE virtual-size deltas are `.text -2,128`,
`.rdata +288`, `.pdata -24`, `.xdata -16`, and `.reloc +12` bytes, with every
other section size unchanged. The retained executable contains
`bitblock/tick-outcome/v2`, both `bitblock/frame-receipt/v1` and
`bitblock/frame-receipt/v2`, and `bitblock/display-receipt/v2`. The historical
display-v1 code and KAT remain in the source/test artifact but have no live
default-executable consumer and were removed by release dead-code elimination.

The source and displayed-pixel uniqueness audits admitted no new metric family:
all candidates were aliases, scalar views, reason labels, carrier refinements,
or already-owned pixel/presentation boundaries. Five existing definitions were
updated and reminted, so the catalog remains exactly 1,112. At that checkpoint,
the sidebar and composite policy identities were
`#bb.policy.sha256.7i6srq6tdvbwfjdhc37rami42zfza5pwmqgs4hzqvhiwmpqahs2a`
and
`#bb.policy.sha256.erfoijfyrlhywluoeobxz3rh443rzl7r44uebsyofreyt7koaaxq`.
Their independent Python reconstruction is gated. No fresh live screen capture
was run, so no literal-display result is claimed for those policy identities.

The optimization-v3 ancestry checkpoint passed formatting, 199 library and 28
application tests in both the default and `protocol-research` all-target
configurations, strict Clippy over all targets/features, all nine independent
Python vector tools, the exact 1,112-definition metric audit, and `lake build`
(6 jobs). Its fresh default release is retained at
`target/optimization-v3-ancestry-checkpoint/release/bitblock.exe`: 1,119,744
bytes, SHA-256
`4EEB4CB578F52A14BB3CA0F87858644C4AFF090FD9757695BEAA1D6179FA538A`.
Relative to the typed-display checkpoint, the file size and every PE section
raw size are unchanged. Every raw section is byte-identical except `.rdata`;
in particular `.text` is byte-identical, consistent with the research assay
remaining absent from the default executable. The `.rdata` delta is the
displayed frame-provenance metric-definition remint from
`#bb.metric-definition.sha256.5forxny6mnb3lujus4muvlwc4x7kt5kg57ksoghd6jyqkg4hwwtq`
to
`#bb.metric-definition.sha256.k4ifjocfjgrsche27ugetkl3xtf726hmbejju6dsthmmsldolckq`;
the independently reconstructed sidebar/composite policies above bind the new
definition. No live app or screen-capture assay was run for this release.

## World receipts and mint/burn semantics

Let `W` be the finite occupied set, with immutable origin `o=(0,0,0)`, and let
`M` map each occupied mutable coordinate to the tag of its active mint event.

```text
off -> on   = mint
on  -> off  = burn
on  -> on   = no-op
off -> off  = no-op
any edit of bedrock = protected no-op
```

Every standalone edit has one allocation-free canonical representation:

```text
schema = "bitblock/edit/op-u8-i64-be/v1"
payload = operation:u8 || x:i64_be || y:i64_be || z:i64_be
operation = on:0 | off:1 | toggle:2
payload bytes = 25 exactly
```

The strict decoder rejects every other length and every operation byte outside
that closed set. It round-trips all three operations over coordinate boundary
vectors, and `#bb.edit.sha256.*` commits the complete schema-framed payload.
History event and receipt encoders call this same implementation rather than
maintaining a second spelling.

Toggle is classified by its actual pre/post transition. A re-mint after a burn
creates a new occurrence tag. The old mint and consuming burn remain in
history. The active mutable-coordinate count is `|W|-1`, equal to the cardinality
of the active-mint map; duplicate placement cannot increase it and deletion of
an empty coordinate cannot reduce it.

The implemented unsigned coordinate receipt uses this exact canonical object:

```text
schema = "bitblock/history/receipt/v1"
payload = parent_receipt_digest:32
       || tick:u64_be
       || edit_count:u64_be
       || edit[edit_count]:25 each
       || post_world_digest:32
       || final_event_digest:32
       || registry_patch_present:u8 = 0
       || mode_payload_present:u8 = 0
edit_count > 0
payload bytes = 114 + 25 * edit_count
```

The known field positions restore the `Receipt`, `World`, and `Event` tag kinds;
only their full 32-byte digests are stored in this payload. The strict decoder
checks the declared count, exact total length, nonzero count, every embedded
edit, both zero reserved flags, and canonical re-encoding before admission. The
parent supplies prestate. Replay derives state-change classification,
mint/burn provenance, active-coordinate count, final event, poststate, and the receipt tag; those
facts are not redundantly trusted as effects. Exact per-edit occurrence tags
chain from the parent and prior edit, so repeated identical edit content still
has distinct historical identity.

Camera misses and observer-overlap rejections currently lack a complete
camera-free event payload. They remain a local audit channel. Only exact
resolved integer-coordinate edits enter the local coordinate history.

Receipt-bearing ticks are those with at least one resolved exact edit, including
state no-ops. Empty/camera-only ticks do not grow the coordinate chain at this
checkpoint. Consequently this chain proves ordered coordinate history under its
assumptions; it is not a continuous sequential tick transcript.

## Bounded storage-record envelope

`src/storage.rs` defines an in-memory envelope and a strict read-only scanner:

```text
"BBSTRREC" || u16_be(version=1) || u64_be(total_record_bytes)
parent-present:u8 || optional parent StorageRecord digest
u16_be(kind-name bytes) || canonical kind-name bytes
stored-object digest
u16_be(object-schema bytes) || object-schema bytes
u64_be(payload bytes) || canonical payload bytes
storage-record digest footer
```

The object digest is recomputed under its named kind and schema. The record
footer is recomputed over every preceding record byte under
`bitblock/storage-record/envelope/v1`. Admission also checks the expected
parent tag, exact lengths, canonical re-encoding, and hard record/schema/
payload/count/stream limits. The scanner stops at the first invalid record and
never searches later bytes for a fresh magic marker. Exhaustive proper-prefix,
single-bit-flip, splice, field-swap, repaired-footer/nested-tag, cap, and parent
controls exercise these refusals.

The envelope alone is deliberately semantic-agnostic. `src/storage.rs`
allocates bounded records in memory and itself does not open, append, flush,
rename, or recover a file. The optional journal and typed-history layers below
compose it with those narrower responsibilities. None is connected to the
default live observer, and none proves power-loss durability, rollback
resistance, external custody, availability, signatures, or consensus.

The module is available under Cargo feature `protocol-research` and in unit
tests. It is absent from the default observer executable so this proof/storage
surface adds no linked code to the hot application artifact.

## Exclusive-writer journal checkpoint

`src/journal.rs` composes the envelope with a bounded Windows filesystem
boundary under `protocol-research`. Creation and opening are intentionally
different operations: `create_new` refuses an existing path and
`open_existing` refuses a missing path. The retained read/write handle uses
[`share_mode(0)`](https://doc.rust-lang.org/std/os/windows/fs/trait.OpenOptionsExt.html),
because Rust's Windows default otherwise shares read, write, and delete access.
There is no weaker shared-mode fallback and non-Windows callers receive an
unsupported-platform error rather than an exclusivity claim.

Opening reads no more than the selected hard-bounded stream limit and returns
one of two typed states:

- `Ready`: every byte belongs to the canonical parent-linked prefix;
- `NeedsRecovery`: the report retains total bytes, verified-prefix bytes,
  verified record count/head, external-ancestor result, the exact first error,
  a closed rejection class, rejected suffix length, and a full suffix tag.

The recovery handle can export and re-tag the exact suffix while its exclusive
handle remains open. Scanning stops at the first refused record and never seeks
later magic. Resource-limit refusals and unsupported protocol data are
distinguished from structural/authentication failures; this API forbids
discarding the former two. A structural/authentication suffix is truncated
only through the explicitly named `discard_unverified_tail` operation, which
calls `set_len`, `sync_all`, rereads the retained prefix, and requires the same
record count, head, bytes, and chain before returning a writable journal.

An append builds and verifies the complete record before I/O, checks the
tracked EOF through the held handle, completes all short writes, calls
[`sync_all`](https://doc.rust-lang.org/std/fs/struct.File.html#method.sync_all),
rechecks EOF, rereads the appended range, and compares every OS-visible byte.
Only then is the in-memory head published. A write, sync, EOF, or reread
failure makes the commit uncertain, leaves the logical head unpublished,
poisons the handle, and requires close/reopen recovery. `sync_all` is used
because append changes both content and EOF metadata; its successful return is
the receipt's exact claim.

Powered controls cover every byte cut through one append, one-byte short
writes, write failure after every accepted prefix, sync/EOF/reread failures,
same-length byte substitution, later valid magic after a torn record, all
tail-discard failure phases, exact suffix export, tighter-limit refusal without
deletion, and clean-boundary/alternate-chain negative controls. A record can
span sectors, and Windows does not guarantee multi-sector `WriteFile`
atomicity; the footer detects an admitted complete record but is not a physical
atomic-commit marker. See the official
[`WriteFile`](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-writefile)
and
[`FlushFileBuffers`](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-flushfilebuffers)
contracts.

This checkpoint proves bounded local recovery behavior under its hash, Rust,
OS, filesystem, and hardware assumptions. It does **not** prove that a returned
sync survived sudden power loss, that a discarded suffix was never previously
acknowledged, or that a locally clean EOF is the latest history. Boundary
truncation and a fully valid alternate branch are locally indistinguishable.
`RequireAncestor(tag)` can reject a candidate that omits an externally retained
storage-record tag, but Bitblock does not yet provide that external custody.
The suffix tag names only `(verified offset, total offset, exact suffix)`; it is
not a recovery receipt, authority statement, or semantic-object verifier.

## Typed coordinate-history journal checkpoint

`src/history_store.rs` composes the physical journal with exact coordinate-
history replay under `protocol-research`. Creation first checks the supplied
initial world against both semantic replay limits and the storage envelope
without touching the target path. It then uses exclusive `create_new`, appends
and synchronizes a genesis object under
`bitblock/history/genesis/v1`, rereads it, and requires its 32-byte payload to
equal the canonical initial-world digest. A failure after path creation may
leave an empty or partial file; the path is preserved for explicit generic or
manual inspection, and typed creation will not overwrite it.

Two different chains are checked because they answer different questions:

- the storage-record parent links exact physical envelope records;
- the receipt parent links exact semantic coordinate-history objects.

Opening first admits the bounded physical prefix and then requires a single
genesis followed only by canonical receipt objects. Typed replay verifies the
genesis kind, schema, payload, and object tag; every receipt's strict codec and
object tag; both parent chains; strictly increasing receipt ticks; and the
replay-derived final-event, post-world, and receipt tags. It reconstructs the
world, complete in-memory receipt/effect chain, and active mint provenance from
the caller-supplied initial world. A physical recovery prefix must pass that
same semantic replay before a typed recovery or discard handle is returned.
Repairing carrier hashes therefore cannot turn changed coordinate semantics
into an admitted history.

Semantic replay has a separate local resource policy. The current defaults are
16,384 receipts, 65,536 edits per receipt, 262,144 total edits, 65,536 peak
occupied bits, 16,384 peak allocated chunks, and 4,194,304 cumulative
coordinates visited by world-tag computations. Exact usage retains receipt and
edit totals, transient occupancy and chunk peaks, and cumulative world-tag
visits. Count and canonical-length checks occur before receipt allocation;
transient world limits are checked after every edit. A policy refusal is not
semantic corruption and never makes a suffix eligible for deletion. Callers
may explicitly select a different bounded policy, and live typed appends are
held to the same policy that reopen will use.

The same module can issue a borrowed verification token for an already
materialized in-memory `History`. It rebuilds that history from the supplied
initial world under the same six replay limits and admits the token only after
genesis, every edit/effect/receipt, head, final world, and active mint
provenance agree. The token retains the replay-derived final world and exact
named usage. Powered controls show that this usage equals persistent replay
for the same chain, passes at every exact boundary, and refuses each one-less
boundary. Coordinate-visit usage counts coordinates presented to world-tag
computations; it does not count sort comparisons or every bounded provenance
scan, so it is a named work component rather than total CPU work.

Open-local mode never collapses that token to a static label. Its admitted
`Receipt` tag uses schema
`bitblock/history-mode/open-local-evidence/v1` and commits exactly to the
open-local mode digest, verified history genesis, verified history head, and
replay-derived final-world digest. Thus histories with equal work usage remain
distinct admissions. An independent Python vector and one-field-at-a-time
mutants pin all four fields.

An append applies edits to a private candidate world, seals one prepared
receipt while computing its post-world tag once, then appends, synchronizes,
and rereads its storage record before publishing any semantic state. After the
carrier succeeds, a semantic publication guard remains poisoned until the
exact prepared history, world, and replay-usage values have all been
published. Preflight or carrier failure leaves the prior semantic heads
unchanged; a carrier success followed by an internal tag mismatch leaves the
handle poisoned and requires close plus typed replay.

Powered tests cover exact create/reopen reconstruction, literal-coordinate
replay oracles, both parent chains, malformed and semantically mutated receipt
matrices with repaired carrier hashes, replay-limit boundaries and transient
peaks, semantic refusal before carrier publication, persisted-but-unpublished
reopen recovery, missing/torn genesis preservation, rollback controls, and
physical-tail recovery whose retained prefix must also be semantically valid.
The lower journal separately injects short writes and failures at write, sync,
EOF, and reread phases; those carrier failpoints plus the typed publication
ordering are compositional evidence, not a direct fault-injection test through
the typed `append_tick` API. Likewise, checks that compare object tags produced
and consumed by the same Rust encoders are internal consistency guards, not an
independent implementation or cryptographic proof.

This layer remains unsigned, local, caller-genesis-relative, feature-gated, and
absent from the default executable. It neither identifies the authoritative
initial world nor proves that a locally valid file is the latest branch.

## Local sequential-history transcript

`src/sequential.rs` defines one feature-gated local transcript and verifier.
Its exact function is SHA-256 under these closed schemas:

```text
function = "bitblock/sequential-history/function/sha256-chain-v1"
input    = "bitblock/sequential-history/input/v1"
step     = "bitblock/sequential-history/step/v1"
segment  = "bitblock/sequential-history/segment/v1"
```

The function-policy tag has an empty payload. An initial-state tag commits to
`u16_be(source-kind-name length) || source-kind-name || source digest`. Mode
admission first obtains the bounded in-memory verification token described
above, then derives that source from its `History` genesis; it does not accept
an initial state chosen by the claimant or trust cached effects directly.

Each iteration computes a new `SequentialHistory` tag over:

```text
function digest:32
absolute iteration:u64_be
prior state digest:32
injection count:u64_be
ordered event digests:32 each
```

Injections occur before the step at their relative segment offset. Offsets are
nondecreasing and smaller than the segment iteration count. Multiple events at
one offset are legal and their declared order is semantic; they are neither
sorted nor deduplicated.

One canonical segment payload is:

```text
function digest:32
parent present:u8
parent segment digest:32       # all zero when absent
start iteration:u64_be
iteration count:u64_be         # greater than zero
input state digest:32
injection count:u64_be
(relative offset:u64_be || event digest:32) * injection count
output state digest:32
```

Its exact byte length is `153 + 40 * injection_count`. The decoder checks the
declared count and exact length before allocating, rejects flags other than
zero or one, requires a zero digest for an absent parent, checks iteration
arithmetic and offset order/range, and requires canonical re-encoding. Chain
verification pins the exact function tag and recomputes every iteration while
checking segment tag, parent, start, input, output, and cumulative limits. A
cheap whole-chain preflight checks segment count, per-segment counts, and both
cumulative counts before event coverage, payload hashing, or recurrence work.

Coordinate-history admission derives the complete required event sequence by
flattening every receipt effect in verified history order. The flattened
segment injections must match it exactly in count and order. The admitted
evidence tag commits to the mode, history genesis and head, sequential segment
head and output, reverified iteration total, and covered-event count. This is
a sidecar admission result: receipt bytes still encode the reserved mode flag
as zero, and sequential segments are not yet persisted by the typed journal.
The admission result also carries the exact bounded history-replay usage.

The default local verifier policy is 4,096 segments, 1,048,576 iterations per
segment, 16,777,216 total iterations, 65,536 injections per segment, and
262,144 total injections. These are retryable local computation/allocation
limits, not protocol authority and not evidence that the accepted work had a
minimum real-world cost.

Powered Rust controls cover every proper segment prefix, overlong and repaired
semantic mutants, chain gaps/resets/splices, exact event coverage across
segments, alternate legal offset schedules, preflight/resource refusals, and
fail-closed stake-mode requests. Stake refusal occurs before any history or
segment work, even for an invalid history and all-zero local limits.
`tools/sequential_vectors.py` independently constructs the canonical bytes and
SHA-256 recurrence with Python `hashlib`; frozen Rust vectors agree on every
step, two linked segments, one equivalent joined segment, and the coordinate-
evidence and open-local-evidence tags. Powered mutants independently change
each evidence field.

The result is deliberately named a **local sequential-history transcript**,
not Proof of History. Serial recomputation does not establish an unavoidable
sequential lower bound, a VDF security property, elapsed time, clock rate, an
authoritative/latest branch, external custody, data availability, authorship,
or consensus. Different event-offset schedules and fully recomputed branches
can both be locally valid until an independent authority pins one. The module
is compiled only for tests or Cargo feature `protocol-research` and is absent
from the default observer executable.

A default release built immediately before the metric-family closure remains at
`target/sequential-checkpoint/release/bitblock.exe`. It is 1,033,216 bytes with SHA-256
`CBF823B4F6B6088BB7288000C1F142E3556D8776E39C7F51489F5B2D4FB30F60`.
Against the preceding typed-history checkpoint of the same size, every PE
section has the identical size and content; the only four differing file bytes
are `0x88: 00 -> 4d`, `0x89: ee -> f9` in the COFF timestamp and
`0xd8: 21 -> 6e`, `0xd9: c5 -> d0` in the derived optional-header checksum.
This is direct artifact evidence that the
feature-gated sequential/history-verification code added no default linked
section content, not a general proof about future builds or toolchains.

The current post-closure default release is
`target/metric-closure-checkpoint/release/bitblock.exe`: 1,032,704 bytes,
SHA-256 `AB90871F696B03135EFED6B58922EEBEC021243337F4557BFD9548F98EA99849`,
512 bytes smaller than the preceding checkpoint. The closure intentionally
changed compiled sidebar policy bindings, so a whole-binary comparison no
longer isolates the feature-gated research module. The exact offset/byte diff
record covers 172,683 differing or absent positions and has SHA-256
`076D44CCE4DAF8685B3D7BDE88257A052D338EC65E7413443B5ADDB285F6657D`;
its canonical record is ascending repetitions of little-endian `u64` absolute
offset, `u16` preceding byte, and `u16` closure byte, using 256 for an absent
side. The record can therefore be reconstructed from the two retained files.
This is an artifact receipt, not a claim that every shifted byte is an
independent semantic change.

## Modes and claims

Modes are tagged rule objects, not a label switched over one unchanged receipt:

- **open-local:** deterministic replay and tamper evidence inside one process;
  no authorship, rollback prevention, availability, or consensus.
- **signed-author:** adds key-tagged signatures; proves authorship only under
  signature and key-custody assumptions.
- **threshold-checkpoint:** independent signers attest one history head.
- **sequential-history:** the implemented local mode pins one function,
  replays the transcript, anchors it to the supplied verified history genesis,
  and requires exact full-history event coverage. It does not prove elapsed
  time, minimum work, or branch authority.
- **stake-consensus:** a separate distributed state machine for validator sets,
  stake positions, proposer/committee selection, votes, fork choice, finality,
  slashing, unbonding, checkpoints, and network partitions.

The current closed admission result has only `open-local` and
`sequential-history` variants. A `stake-consensus` request is representable
only so it can be refused with the missing stake, validator, signature, quorum,
fork-choice, finality, and network machinery named; there is no admitted stake
variant and a valid sequential transcript cannot satisfy that request. The
refusal is unconditional and precedes history replay.

The current local history has neither actors nor authorization. Freely placeable
coordinates therefore cannot represent voting stake or scarce currency: any
process allowed to submit unconstrained edits can create and clear mutable
occupancy without bound. “Mint” and “burn” remain local occupancy-event labels.
Mapping placement to a separately scarce owner-authorized asset would require
the missing ownership and authorization machinery. A public Layer 1 claim additionally requires
independent nodes, networking, resource limits, ownership and transfers,
replay protection, finality, availability, synchronization, key lifecycle,
upgrade governance, and an explicit threat model.

Until those exist, the truthful name is **local content-addressed coordinate
history protocol**, not cryptocurrency or Layer 1.

### CoordinateEpisodeEvidence v1

The opt-in research module `src/coordinate_episode.rs` materializes the
existing `#bb.metric.dyn.issuance-provenance-episode-relation` as one bounded,
in-memory, canonical carrier. It first calls
`verify_in_memory_history_with_limits(initial_world, history, limits)` and
refuses before deriving anything if that replay does not reconstruct the full
history genesis, effects, receipt/head, final world, and active mint
provenance. Its policy tag is
`bitblock/coordinate-episode-derivation-policy/v1`; the receipt-kind outer tag
uses the distinct `bitblock/coordinate-episode-evidence/v1` schema. Decode,
encode, and local verification admit only that exact static policy tag, not an
arbitrary tag of the same kind.

The canonical payload is:

```text
derivation-policy tag digest [32]
history-genesis receipt tag digest [32]
history-head receipt tag digest [32]
verified final-world tag digest [32]
episode count u64
repeat receipt/effect order:
    coordinate i64 × 3
    mint event tag digest [32]
    active marker | closed marker + consuming burn event tag digest [32]
```

The mint event tag is the episode identity. Codec counts, traversal positions,
and signed coordinate lanes frame or locate bytes; they are not substitute
asset IDs. A post-genesis `Mint` opens one episode; a `Burn` must consume that
same open mint tag at the same coordinate and closes it. No-op edits create no
episode. A mutable coordinate present at genesis maps to the history genesis
tag, so deleting it closes no post-genesis episode; a later re-mint opens a
new event-tagged episode. `verify_against` first checks the carrier tag, then
derives the full relation again from a bounded verified history and requires
exact equality. The 262,144 cap and allocation budget apply to emitted episodes,
not raw history edits; replay has its own separately declared source-work bounds.

The module is deliberately excluded from the default game binary. The Rust
tests cover same-tick mint/burn/re-mint ordering, genesis separation, no-op
head divergence, policy substitution, duplicate event tags, malformed codec
states, source-field mutations, and replay limits. The stdlib-only known-answer
check is:

```powershell
python -B tools/coordinate_episode_vectors.py
```

This records local provenance only. It does not create balances, ownership,
transfers, scarcity, signatures, custody, stake, elapsed time, consensus,
Proof-of-History, Proof-of-Stake, cryptocurrency, or Layer 1 authority.

## LadderLang and static encoding upper bounds

LadderLang's live definition is:

```text
Coin ::= iota | tick Coin Coin
iotaUpper = 2*I + 4*K + 5*S
ticks = iotaUpper - 1
```

The weights apply to occurrences in the declared naive SK translation. The
cost is a static encoding upper bound. It is not wall time, beta-reduction
steps, CPU instructions, renderer work, sequential critical path, or minimum
program size.

The first bridge is now implemented for one source schema only: canonical
standalone Edit. `bb-lam/v1` refuses every other kind/schema, malformed Edit,
noncanonical round trip, wrong claimed source tag, and arithmetic or protocol
length failure. It first reproduces the complete 103-byte Edit content-tag
preimage, prefixes the nine ASCII bytes `bb-lam/v1`, and quotes all 112 bytes.
The cost may rank already-valid equivalent representations; it may never trade
away state or pixel validity.

The Rust bridge computes the stated closed-term formula after verifying the
canonical source bytes; it does not link or execute LadderLang. The separate
pinned parity assay calls LadderLang's `Lam.costChecked`, which rejects open
terms. The bridge must not use the current tick-receipt token byte spelling
because that spelling reduces variable indices modulo 256 and is non-injective.
The frozen content-neutral policy instead uses two distinct equal-cost closed terms:

```text
B0 = App I FALSE
B1 = App FALSE I
```

`B(0)=B0` and `B(1)=B1`; both have `atoms=3` and `iota=8`. One byte is eight
MSB-first terms in a fixed seven-PAIR right spine. Bytes form an application
right spine ending in NIL. The policy, exact raw-octet Lam preorder, expanded
term, and receipt are independently content-tagged. For quoted byte length `L`:

```text
atomsUpper = 185L + 2
iotaUpper  = 736L + 6
ticks      = 736L + 5
```

The standalone Lean capsule proves these formulas for every finite byte list
and separates right/left PAIR, bit-order, and byte-list mutants. A second assay
at pinned LadderLang commit `59042265167b8f40700a891d890fde8015bcade9`
calls its own `Lam.costChecked`, reproduces primitive/byte/two-byte costs, runs
the complete frozen 112-byte Edit quote at `20,722` atoms and `82,438` iota,
rejects an off-by-one atom claim, and proves the powered open-term control is
refused. Rust, Python, and an independent Node implementation agree on the Edit
vector below:

```text
source Edit tag      #bb.edit.sha256.l3qxygkk2tikiyma5zkvaznrgzvu46ulmtdse56c2tf4r24ugzdq
bridge policy tag    #bb.policy.sha256.veaii6flkafmvwll7rafobhr3dttrjgn6syxvipbzppxbuauaoyq
expanded Lam tag     #bb.ladder-term.sha256.jarqhm3djti62ufflcz7d3xyyvmyv4hku4optzuoarqoqegicndq
quote receipt tag    #bb.ladder-quote-receipt.sha256.5c2ipojlcn2vlpoeabhck5tvnd6rqclwgfjyd2rafkkyepuczxja
quoted bytes         112
Lam prefix bytes     46,491
Lam nodes/depth      13,331 / 3
S / K / I            7,840 / 8,737 / 4,145
atoms / iota / ticks 20,722 / 82,438 / 82,437
```

Costs alone cannot detect association or bit order, so the expanded raw-octet
Lam tag and powered structural mutants are mandatory. Pricing quoted bytes
still measures representation only, not executable `on`/`off`/`toggle`
meaning, beta fuel, simulation time, rendering work, currency value, or
consensus weight. A true interactive compiler requires a separate semantics
and refinement proof.

Like the storage envelope, the Rust bridge is opt-in under
`protocol-research` and always present in unit tests. Its policy and receipt
vectors stay gated without being linked into the default renderer executable.

## Optimization gate

For each candidate change:

1. Freeze renderer/build profile, canonical initial state, exact edit log,
   camera bits, viewport, overlay, and reference artifact.
2. Compare final world state, camera state, depth where in scope, and the full
   final framebuffer slice. Hash agreement alone is weaker than slice equality.
3. State the quantifier: one fixture, seeded corpus, exhaustive finite domain,
   or theorem. A passing finite corpus is bounded evidence, not a universal
   proof over `i64^3` and floating-point views.
4. Only after the correctness gate passes, compare source bytes, binary-section
   bytes, reachable tick/render code bytes, allocations, time, memory traffic,
   energy, and the static iotaUpper encoding measure.
5. Change one optimization at a time and retain the failing powered control that
   demonstrated the check could detect a real renderer defect.

One scalar may rank equivalent candidates after validity. It cannot make a
smaller or faster invalid renderer acceptable.

### Rejected: a tiled coarse-depth occlusion grid

Two occlusion implementations were built independently against the same gate and
measured against each other. A 16x16-pixel grid holding the maximum depth per
tile, with a per-triangle test that skips when every tile the triangle touches is
already nearer, was **not** the one merged. Both are exactly equivalent: each
left all 106 literal content tags byte-identical.

Total frame, medians of interleaved rounds, same probe binary in all three trees
so the renderer is the only variable:

| bits | baseline | merged (sorted) | tiled |
|---:|---:|---:|---:|
| 1 | 0.93ms | 0.94ms | 1.00ms |
| 99 | 3.22ms | 1.50ms | **1.35ms** |
| 2,649 | 5.75ms | 2.31ms | **2.18ms** |
| 12,699 | 9.02ms | **3.78ms** | 4.37ms |
| 58,809 | 21.72ms | **10.50ms** | 14.94ms |
| 149,787 | 45.12ms | **23.43ms** | 34.29ms |

They cross over. The grid is about 6% ahead at 99 and 2,649 bits and 32% behind
at 149,787, where the cost of maintaining tile maxima on every write grows faster
than the savings, while the per-bit scan it competes with exits at the first
pixel that is not occluding and gets *better* as occlusion increases. The tiled
grid is also 8% slower at one bit, paying for structure with nothing to occlude.

The measurement that decided it was the one nearly not taken: the first
comparison stopped at 12,699 bits, where the two are close and the grid had won
two of four rows. Extending to 149,787 is what separated them.

Worth recording for whoever revisits this: at 58,809 bits and above, **every**
version misses the 16.67ms budget — 45.12ms unoptimised, 23.43ms merged. This
change moves the ceiling, it does not remove it.

The tiled implementation is preserved on `try/occlusion-tiled`, verified and
green, in case the crossover point ever matters for a known workload size.

### Rejected: short-circuiting the barycentric test

The rasteriser's inner loop computes three barycentric weights, then tests all
three. Each weight costs one `edge` evaluation and one f64 division, and the
depth interpolation costs three more divisions, so a pixel is six divisions at
roughly thirteen cycles each — measured at about 8ns per rasterised pixel, and
90% of a frame's cost.

Testing each weight as it is computed is exactly equivalent: surviving pixels
evaluate the same expressions in the same order and write the same values, only
unused work disappears, and `NaN < eps` is false either way. A triangle covers
about half its bounding box, so roughly half of these pixels should have exited
after one division instead of six.

It is **15 to 17% slower**, consistently, at every world size:

| bits | current | short-circuited | change |
|---:|---:|---:|---:|
| 1 | 0.80ms | 0.94ms | +17.5% |
| 99 | 3.06ms | 3.58ms | +17.0% |
| 2,649 | 5.12ms | 5.89ms | +15.0% |
| 12,699 | 7.76ms | 9.01ms | +16.1% |

Raster time only, median of five interleaved A/B rounds, so drift cannot
account for it. All 397 tests passed under the change, so this is a performance
result and not a correctness one.

The three divisions are independent, and the divider overlaps them. Putting an
unpredictable branch between each one serialises what the hardware was already
pipelining, and adds two mispredicting branches per pixel. Doing less work is
not the same as taking less time.

Both remaining ideas for this loop are ruled out by exactness rather than by
measurement: `w / area` is not `w * (1.0 / area)` in IEEE 754, and neither is
`w / depth` and its reciprocal, so hoisting either changes the frame. What is
still available is not touching the inner loop at all — a bit that is provably
occluded need not be rasterised, and skipping it writes nothing different. That
needs a coarse depth structure, and traversal reordering is already permitted
above.

### Accepted: front-to-back traversal and a per-bit hidden test

The idea above, without the coarse structure. The depth buffer already is one.

Bits are traversed nearest first, keyed by view depth. That is legal precisely
because of the deterministic-renderer clause: `plot` resolves equal depths by
the lower RGB word, so a frame is a per-pixel minimum over the bits and no
order can change it. The order is a counting pass over 1,024 linear depth
buckets, not a comparison sort — `sort_unstable_by` on 12,699 keys cost 0.94ms,
more than a third of what the ordering saved, and the order only has to be
approximate because it is a heuristic and not a semantic.

The near surfaces are then in the depth buffer before the bits they hide reach
`draw_bit`, which reads the retained depth across the bit's own screen box. If
every pixel there is already strictly nearer than anything the bit could write,
rasterising it would lose every comparison in `plot`, so skipping it writes
nothing different. The scan stops at the first pixel that is not — an untouched
pixel holds infinity — so a visible bit usually pays one word, and only a bit
that really is hidden pays for the whole box, which it then does not rasterise.

The bound has to be a bound, not an estimate. A fragment's depth is
`1.0 / (wa/za + wb/zb + wc/zc)`; dropping the negative terms makes that at
least the nearest projected vertex divided by the sum of the positive weights.
That sum is exactly one in real arithmetic, because the three `edge` numerators
sum to the shared `area` denominator, so what remains is the admitted
barycentric epsilon plus rounding — and rounding is bounded by the coordinate
span divided by the smallest `area` the bit's own triangles will use. A sliver
triangle widens the bound rather than escaping it; a span too large to represent
collapses the bound and the bit is rasterised. A bit whose box falls outside the
viewport entirely is now rejected before its faces are set up.

Raster time only, medians of nine interleaved A/B rounds alternating the same
two binaries:

| bits | current | hidden-bit test | change |
|---:|---:|---:|---:|
| 1 | 1.06ms | 0.85ms | within noise |
| 99 | 3.24ms | 1.35ms | -58% |
| 2,649 | 4.98ms | 1.70ms | -66% |
| 12,699 | 7.88ms | 2.52ms | -68% |

The one-bit row moved both ways across two runs of the same experiment; one bit
hides nothing and there is nothing to gain there. Ordering shows up in the
probe's `project` column rather than `raster`, because it also runs in the
away-facing control: +0.14ms at 12,699 bits, and neutral or better below that,
where iterating a flat vector of coordinates repays the keying. A whole frame
of the largest world falls from 8.88ms to 3.65ms, from 53% of the 60Hz budget
to 22%.

Every canonical byte held: 399 tests including all 101 literal content tags,
the independent analytic ray oracle, the 24 Python vector suites, and
`bbmutate render` still killed. `renderer_profile_tag` is deliberately
unchanged — the new constants bound the inner loop's arithmetic rather than
describing the frame, and no declared constant or ordering rule moved.

Two fixtures were added, because every existing frame vector uses four bits or
fewer and none of them reaches this path. Both compare the ordered traversal
against the unordered one with the test disabled, pixel for pixel and depth bit
for depth bit, and both also rasterise every skipped bit into a copy of the
buffers and require that nothing moved: whole-frame agreement alone would let a
later bit write over a wrong skip and hide it. One fixture is a shell and a
solid block from five views; the other is a seeded corpus of 320 dense random
worlds and cameras, which is where the sliver, near-plane and off-viewport
cases come from. Both were watched failing against a renderer that skips every
covered bit unconditionally.

Worth recording as a limit of that evidence: neither fixture can distinguish
the bound's margin. Inflating the test's depth limit by 5%, or even to the
bit's farthest vertex instead of its nearest, still produces no wrong pixel
anywhere in the corpus, because in front-to-back order a bit whose box is fully
covered has so far always been genuinely hidden by a wide margin. The fixtures
enforce that the test exists and is sound in kind; the margin rests on the
derivation above, which is why it is written out rather than tuned.

The first receipt-gated release optimization changed the crosshair policy
assertion to debug-only. It removed two redundant SHA-256 policy computations
from every release crosshair draw and reduced the Windows release executable
from 973,824 to 971,776 bytes while all three named complete-frame tags stayed
identical. This is a 2,048-byte compiled-artifact reduction; source text grew by
six bytes, so it is not described as source compression.

The Edit/storage/Ladder checkpoint initially raised the same Windows release
artifact from 1,034,240 to 1,036,288 bytes even though the new library surfaces
were not called by the game. One isolated feature-boundary change made storage
and Ladder modules plus their private tag kinds opt-in. All 121 Rust tests,
named complete-frame vectors, strict clippy, both Lean gates, independent
vectors, and the live full-client pixel assay still passed; the default release
returned exactly to 1,034,240 bytes. The resulting artifact is SHA-256
`7809f4c4c6ece862df80a1d5aa43e25d77aa75618d6de616ce33d5c463d35aa4`.
This removes a measured linked-artifact regression; it does not claim that the
research modules themselves became smaller.

The journal checkpoint was rebuilt in a fresh target directory because the
preceding verified executable was still open and Windows correctly refused to
replace it. The new default-feature executable is also exactly 1,034,240 bytes,
at SHA-256
`829bcb057630c06b2290811683cff3dd412dbed818bc8af9fa226da05d20b9ba`.
Against the preceding artifact, the complete files differ at seven byte
positions: two PE timestamp bytes, two dependent PE checksum bytes, and three
read-only source-location line values (`125/308/301` became `131/314/307`).
Every PE section has the same size; `.text` is byte-identical at SHA-256
`738bc96d0c1eeca91844bb94b58ee9d2b5348f885d265f3947ee18b0b87edf8a`.
Every other section hash is also identical except `.rdata`, which contains the
three line values. This is exact artifact evidence that the feature-gated
journal added no default executable code or bytes; it is not a semantic proof
over every runtime input.

The typed-history/metric checkpoint was likewise built with default features
into a fresh target directory. Its executable is 1,033,216 bytes at SHA-256
`28b6569edb3898151446dcf05c2754957810ef4430fd7bfe84f7606e55e04f68`,
1,024 bytes smaller than the journal artifact. PE virtual-size changes are
`.text -1,408`, `.rdata +224`, `.xdata +20`, and `.reloc +4` bytes; raw section
alignment makes the complete-file delta exactly -1,024 bytes. This is not a
pure typed-module isolation: the same checkpoint updated two displayed metric-
definition tags and their derived sidebar/composite policy tags, and it stopped
carrying a research-only state-change count inside default-build prepared
history ticks. The complete default and feature-enabled Rust suites, strict
Clippy, registry audit, independent policy vectors, and Lean build pass, but
the changed policy authority means this artifact comparison is not an output-
equality proof and no new live screen assay was performed for it.

The next checkpoint installed the original v1 test/research-only
optimization-equivalence assay before changing runtime code. It compares sorted occupied coordinates,
the complete ordered history receipt/effect sequence and replay, exact
frame-input commitments, every game and composite pixel before their hashes,
both frame-ledger endpoints, and deterministic synthetic display receipts. It
recomputes observations from the supplied pixels and refuses shared forged
caches or receipts rather than allowing two equally wrong lanes to confirm one
another. A success receipt binds the named case, initial world, reference and
candidate implementation/policy tags, and the verified equal output root. The
claim remains bounded to the named case or corpus. The default release at
`target/optimization-assay-checkpoint/release/bitblock.exe` is 1,032,704 bytes,
SHA-256
`57301D370F481CA6D99064BE78EEFF04CA57C542E0F0D78F13C01650D5AFB71E`.
Against the metric-closure artifact, all eleven PE section sizes and hashes are
identical. Only five file bytes differ: three COFF timestamp bytes at offsets
136--138 and two derived checksum bytes at offsets 216--217. Thus that
historical v1 assay added no default linked section content. The current
research authority is the version-distinct typed-v2 assay described above; the
frozen artifact here remains evidence for the earlier checkpoint.

The first optimization admitted behind that gate streams the fixed 368-byte
`MetricSnapshot` v1 payload directly into SHA-256 instead of allocating and
materializing a temporary `Vec`. The frozen old encoder and the production
streamer agree byte-for-byte over 768 exhaustive Option/status/boolean branch
cases, 2,880 walking-bit cases covering every data bit, 91 named floating-point
bit-pattern cases, and two independently reconstructed Python known answers:
3,741 bounded cases in total. Each case also pins the 463-byte full canonical
preimage and resulting tag. A deliberate production mutation from big-endian
to little-endian made the walking-bit gate exit nonzero; restoring the subject
returned it to green. The warmed release allocation probe reports
`allocations=0` and `allocated_bytes=0` for one snapshot tag call.

The resulting default executable is
`target/snapshot-stream-checkpoint/release/bitblock.exe`: 1,031,168 bytes,
SHA-256 `4589F6275D108DBD490F296F899DA46D201E5B395D308576FFAC43506AD3E7EE`,
1,536 bytes smaller than the assay checkpoint. PE virtual-size deltas are
`.text -1,712`, `.rdata +192`, `.pdata -12`, and `.xdata -100` bytes; raw
alignment produces the complete-file delta. The exact offset/byte diff record
contains 948,942 differing or absent positions and has SHA-256
`77ABCE89E000913ABF2CC464CDD0BC644A1FEDE0028763A9AD6926E0F1FC5832`
under the same record format defined above. Source text grew because the
reference, corpus, independent vector tool, and allocation probe were retained;
this is runtime-allocation and compiled-artifact reduction, not source
compression. It preserves the declared snapshot byte protocol under the
bounded corpus and structural comparison. It does not prove the renderer over
all worlds/views, establish physical scanout, or substitute for a new live
screen-capture assay.

The next admitted runtime optimization removes `FrameCompositor`'s persistent
`sidebar_pixels` scratch allocation and rasterizes the sidebar directly into a
checked nonzero-origin region of the final strided composite buffer. The game
prefix of every row remains outside that region. A frozen two-buffer reference
arm preserves the preceding raster-then-copy implementation under tests and
the opt-in `protocol-research` feature; it is absent from the default release.

The exact bounded comparison includes production geometry (`801 x 801` game,
`1201 x 801` composite), narrow and clipped panels, zero-width and zero-shape
cases, hidden-sidebar clearing, optional predecessor/raster fields, every six
`DisplayStatus` variants, high-byte game words, and sequential visible-to-
hidden reuse. A second two-tick production-geometry harness separately executes
the frozen and production arms, retains both raw composite frames and raster
facts, derives the case from both snapshots and game frames, constructs each
arm's typed game/composite v2 ledgers, and only then invokes optimization v3.
Its local release-profile test run emitted:

```text
case      #bb.registry.sha256.xcw3qvkgytefv7pdel6eo6msdv5dg7sz64bzglusbwc2lz5y6q4q
reference #bb.registry.sha256.gozzeyvanmqhdfqeatg5diz2pwl7davzqe62fgz7l7okyx4z5k4q
candidate #bb.registry.sha256.ocqmzm3obz5oetilbier7tbouljhizv6ul7xuaafhy6n5gb7jula
output    #bb.receipt.sha256.womhc43272qs5frlbs6o6pwjp7q6g7gj2x4j6uqlwmjx6r5ns76a
receipt   #bb.receipt.sha256.k4r6z6anadv5jvofd7bru3qbifu7tcf7qmgtzxyhmz6wm7bp6dwq
```

Each implementation identity binds the current test executable bytes,
`sidebar.rs`, `Cargo.lock`, `rust-toolchain.toml`, and an arm label. A temporary
subject mutation moved the production sidebar origin one pixel left. The raw
comparison first differed at composite index 800 and the v3 path independently
refused at `FrameLedgerAncestry { admission: 0, edge: 0 }`; restoration returned
both checks to green. These are local same-process identities: the executable
contains both arms, and both arms intentionally share sidebar painter semantics.
They prove the storage/stride/copy change only over the named corpus, not an
independent renderer, every resize, universal equivalence, or scanout.

The production-sized constructor allocation gate is an explicitly executed
probe, not a Cargo unit test:

```powershell
cargo run --release --features protocol-research --example compositor_buffer_probe -- reference
# allocations=2 allocated_bytes=5129604 output_bytes=3848004 sidebar_bytes=1281600
cargo run --release --features protocol-research --example compositor_buffer_probe -- candidate
# allocations=1 allocated_bytes=3848004 output_bytes=3848004 sidebar_bytes=1281600
```

Thus the candidate removes one 1,281,600-byte requested allocation. It also
removes one 320,400-word copy per composite: 1,281,600 logical payload bytes, or
2,563,200 source-read plus destination-write byte events. At an assumed 60
compositions per second that is an upper-bound arithmetic rate of 76,896,000
payload bytes/s (about 73.33 MiB/s), not a measured hardware-memory, timing,
energy, peak-residency, or scanout result. Both compositor policy tags remain
unchanged. The existing candidate/reference framebuffer-mismatch definition was
reminted as
`#bb.metric-definition.sha256.4qmv7cnj56akpfrgx3x4dbx7erin2oz5tmx7k35wjwgptippegga`;
the catalog remains exactly 1,112 unique metric families.

The fresh default release is
`target/strided-compositor-checkpoint/release/bitblock.exe`: 1,119,744 bytes,
SHA-256 `947ABEC0575339F4F708F4E18B6D0C648C0AA8DBA5F0C1145BD163EFFF60CCA3`.
Relative to the typed-v3 ancestry checkpoint at SHA-256
`4EEB4CB578F52A14BB3CA0F87858644C4AFF090FD9757695BEAA1D6179FA538A`,
PE virtual-size deltas are `.text -96`, `.pdata -12`, `.xdata -8`, and
`.reloc +4` bytes; every other section virtual size is unchanged and file
alignment keeps the complete executable size unchanged. Extracted `.text`
changed from 880,264 bytes at SHA-256
`BC3BF0D6341C1837BCE0751368FAD12D0C28EDA78EB6E07AA31C8398B168FCE6`
to 880,168 bytes at SHA-256
`F7BE58C16B83BD934A15C4DC845C379DFD1B39C7B791EFF335BB078F69F0F301`.
No live app, OS compositor capture, or physical-display assay was run for this
checkpoint.

Artifact-handling history is retained rather than folded away. An auxiliary
`objcopy --dump-section` invocation omitted a separate output executable and
therefore rewrote the COFF timestamp and derived PE checksum of both compared
checkpoint files. Hash verification caught the mutation immediately: the old
and candidate paths had moved to SHA-256
`20987A7F75A181FE6395A4D84DCFE1929B1618DA2363A7820F33724543B1A7C3` and
`3146F5DCA2928D9F4A12DAEF2321D0A96CB3381612CEC292FC12DD607E8EC8EB`.
A read-only byte comparison against a separately emitted executable showed the
tool changed only timestamp/checksum fields; read-only PE parsing confirmed
section payloads were unchanged. The original timestamps and standard PE
checksums were reconstructed in memory, and no write was admitted until the
complete bytes reproduced the already recorded `4EEB...` and `947A...` target
hashes exactly. Both retained paths now verify at their stated hashes. Source,
tests, metric definitions, and runtime semantics were never changed by this
artifact repair.

## Canonical display-capture artifact checkpoint

The feature-gated research protocol now has one self-contained
`DisplayCaptureArtifact` v1 object. It retains the exact canonical
`DisplayReceiptV2`, the submitted composite frame bytes, and the captured frame
bytes when capture reached readback. At the codec checkpoint this was an
in-memory admission only; the later typed-journal checkpoint below adds bounded
local sync/reread/reopen evidence without wiring artifact construction into the
live observer or claiming external custody or hardware durability.

The readable kind is `display-artifact`; the schema is
`bitblock/display-capture-artifact/v1`. Its payload is:

```text
DisplayReceiptV2 [378 bytes]
u64_be(expected canonical-frame byte length)
expected canonical Frame
captured form [one byte]
u64_be(inline captured canonical-frame byte length)
optional inline captured canonical Frame
```

Captured form `0` means absent and requires zero inline bytes. Form `1` means
the captured frame exactly reuses the expected pixels and also requires zero
inline bytes. Form `2` carries a distinct frame and requires a nonempty
canonical frame. Encoding expected pixels again under form `2` is rejected, so
one semantic object has one byte representation.

Admission strictly decodes the nested composite `FrameReceiptV2`, recomputes
the expected and captured full-frame observations, refuses non-XRGB high bytes,
binds both shapes to the receipt viewport, and recomputes every mismatch plus
its first index. A receipt with repaired local hashes cannot substitute stale
pixel facts. The shared display-receipt validator was tightened at the same
boundary: a complete sample now requires a zero error, nonzero viewport and
DPI, while unavailable transcripts must match the capture procedure's accrued
flush, geometry, clock, and readback state. In particular, pre-readback stages
cannot claim captured pixels; realistic retained pixels remain admissible at
the post-readback `PerformanceClock`, `GeometryAfter`, and `MonitorBoundary`
failure paths.

The existing 8 MiB object-payload limit is applied before frame hashing or
cloning. At the initial 1201 x 801 viewport, one canonical frame is 3,848,058
bytes, a match/reuse artifact is 3,848,453 bytes, and a distinct captured pair
is 7,696,511 bytes, leaving 692,097 bytes. The exact admitted maxima are
2,097,039 pixels for absent/reused capture and 1,048,513 pixels for two distinct
equal-sized frames. Larger objects fail closed. Decoder canonical comparison is
streamed against the input rather than materializing another full payload.

Independent Python reconstruction pins three initial vectors:

```text
match/reuse
  #bb.display-artifact.sha256.fbd5hyvucb3zycqdqletiyrasrflufmnkjuo4zkdkd6iddprjs6q
mismatch/inline
  #bb.display-artifact.sha256.6seho25bidubdwpxr3s5jwku2jm6k252qiibuuqdz7x7inkxt42q
unavailable/absent
  #bb.display-artifact.sha256.prtxaadxkb7lfpawawthysg2jutx2dqikujmp6osg6tvpodkkzmq
```

Eleven independent negative controls reject representative truncations,
unknown forms, duplicate inline expected pixels, stale frame/observation or
comparison facts, and trailing bytes. The Rust suite additionally checks every
proper prefix, frame-domain/shape/count/length mutants, exact storage-envelope
binding, in-memory tag/shape/form tampering, and both payload-cap edges. A
powered subject mutation removed the captured-observation comparison; the
stale-observation test turned red, then returned to green only after the check
was restored.

This checkpoint admits no new metric family. It remints only the five existing
definitions whose implementation status changed:

- `#bb.metric.visual.compositor-capture-comparison` ->
  `#bb.metric-definition.sha256.ui72l6rqdcyekbnhwmaarvtskxc3vbhxilwokeelrplpzfr65tlq`
- `#bb.metric.hash.domain-separation-identity` ->
  `#bb.metric-definition.sha256.p7dtcjvcxdhernlypv27dsnoczzktp2dcpcrpo63exhv3ltmgkza`
- `#bb.metric.hash.encoding-round-trip-check` ->
  `#bb.metric-definition.sha256.7serkvxp6umyrruavnizzar7kngxdbpdg4stp5y2edhaqhk4rpzq`
- `#bb.metric.hash.known-answer-vector-verdict` ->
  `#bb.metric-definition.sha256.mngt6mlssucxnaqlijkkr5n2jkfctm4vetftcxcha54bh3juyxsa`
- `#bb.metric.hash.cross-implementation-digest-parity` ->
  `#bb.metric-definition.sha256.feoscl6pa6kp7r2aeqzjdz2c563uackrycvim5fu42mpxzhbmykq`

`#bb.metric.store.receipt-object-persistence-inventory` remains future. The
later typed journal retains the artifact's embedded receipt and pixel bytes,
but it does not supply every referenced policy, TickOutcome, or FrameInput
object and therefore does not establish transitive selected-head inventory.
Because the sidebar binds the full definition tag of its displayed compositor
metric, that intentional definition remint also changed the then-current sidebar
and composite policy identities to
`#bb.policy.sha256.nklmneqibltbk3hd3zdpvx4opbwqnrfpp56jetimd322u53gprka`
and
`#bb.policy.sha256.4acfb6wlj2wizbuxywh4arvefslzqnpaf2n5lybsxjoexkpmuahq`.
The independent sidebar vector reconstructs both from the exact registry row;
no raster rule or game pixel changed.
Those policy identities were retained inside the optimization fixture's typed
outcome ancestry, so that checkpoint's v3 output/receipt reminted from the prior
`qfo4...`/`tjzo...` pair to the `qe3p...`/`meke...` pair above. The Python
vector independently reconstructs the new v3 outer bytes while separately
retaining the old v2 input fields and rejected v2 tags; the rejected history was
not rewritten to follow the new policy.

The complete checkpoint gate passes 209 library and 28 application tests in
both default and all-feature all-target configurations, strict Clippy over all
targets/features, formatting, all ten independent Python vector tools, the
exact 1,112-definition metric audit, and `lake build` (6 jobs). The powered
stale-observation subject mutation is not counted as a pass: it deliberately
failed while the guard was absent, then the restored subject passed. No app,
live screen capture, compositor assay, or physical-display assay was run.

The fresh default release is
`target/display-artifact-codec-checkpoint/release/bitblock.exe`: 1,121,792
bytes, SHA-256
`05A10F65F9C313C70C28F053BD83A31A98727BEEA6EB3EE57EDD7E54735C5AD4`.
The retained strided-compositor baseline still verifies at 1,119,744 bytes and
SHA-256
`947ABEC0575339F4F708F4E18B6D0C648C0AA8DBA5F0C1145BD163EFFF60CCA3`.
Read-only PE inspection reports virtual-size deltas of `.text +1,584`,
`.rdata +160`, and `.xdata +4` bytes; every other section is unchanged, while
file alignment makes the complete executable 2,048 bytes larger. A read-only
string assay finds the new displayed compositor metric-definition tag, refuses
the superseded one, and finds no `display-artifact` kind or schema string. Thus
the feature-gated artifact codec remains absent from the default executable;
the release delta accompanies the live stricter display-transcript validator
and policy remint, not a claim that durable capture was added to the app.

## Typed display-artifact journal checkpoint

The feature-gated research protocol now places one complete
`DisplayCaptureArtifact` in one canonical storage record. No new on-disk format
or semantic genesis was added. An empty typed journal is valid; its first
artifact must embed a display receipt with no parent, and every later artifact
must embed the immediately preceding display-receipt tag. This semantic chain
is intentionally distinct from the physical storage chain:

```text
record[n].parent                 = record[n-1].record_tag
artifact[n].display.parent       = artifact[n-1].display.tag
```

The frame-receipt parent is retained as a reference but is not required to name
the preceding sampled artifact because display sampling may skip frames. The
inventory distinguishes the embedded artifact, display-receipt, frame-receipt,
expected-frame, and available captured-frame bytes from reference-only policy,
TickOutcome, FrameInput, and optional frame-parent tags. It therefore does not
claim transitive content closure.

Append fully verifies and canonical-encodes the artifact, reserves its compact
inventory entry, constructs and strict-decodes the exact candidate storage
record, and calculates all aggregate lengths before filesystem I/O. The generic
carrier then appends, calls `sync_all`, verifies the resulting end offset, and
rereads the exact record bytes. Only exact equality with the preflighted
candidate advances the typed inventory. No allocation or decode occurs after a
successful carrier append. An impossible post-append disagreement returns the
record tag and exact byte range as `PersistedButUnpublished`, leaves the typed
handle poisoned, and requires anchored reopen.

Open scans the carrier and then replays every admitted record through the
artifact codec and embedded display parent rule. A physically clean but
semantically false carrier returns an indexed error and exposes no writable or
discard handle. A corrupt physical suffix exposes only the semantically valid
prefix plus the exact rejected tail. Discard remains explicit: the generic
carrier truncates, synchronizes, and proves the retained bytes still equal the
already typed-replayed prefix, so the typed layer can reuse that state without
fallible work after truncation. Clean boundary rollback remains locally
invisible unless the caller supplies a required storage-record ancestor.

A reopened writable handle can construct its next `DisplayReceiptV2` through a
nonpublishing `prepare_next_receipt` operation bound to the recovered display
head. Append checks the parent again, so multiple or stale prepared receipts
cannot advance the journal.

The direct typed matrix contains ten tests covering empty create/reopen; reuse,
inline, and absent artifact inventories; exact lane-to-source binding; resumed
receipt preparation after reopen; the two independent parent chains; indexed
kind/schema/codec/storage-parent failures; repaired outer hashes around a false
display parent; the same semantic failure inside a physical recovery prefix;
pre-I/O parent and payload-limit refusal including the exact limit boundary;
exact tail preservation/discard/reappend; and external-anchor-only rollback
detection. Uncertain write, sync, EOF, and reread failures remain compositional
with the generic carrier's exhaustive failpoint matrix; there is no direct
typed-I/O injection seam yet.

At 1201 x 801, a distinct artifact payload remains 7,696,511 bytes. Its first
unparented record is 7,696,658 bytes and each later parented record is 7,696,690
bytes. Eight distinct records consume 61,573,488 bytes; a ninth exceeds the
64 MiB stream cap. Seventeen reuse records fit and an eighteenth does not. This
is bounded local storage, not continuous capture.

No new metric family was admitted; the catalog remains exactly 1,112. Three
existing definitions reminted:

- `#bb.metric.visual.compositor-capture-comparison` ->
  `#bb.metric-definition.sha256.gisdasoflbgdcvxs6woygfzxopa5gjhbi2pzn2ufcxterstplmoq`
- `#bb.metric.hash.frame-provenance-receipt-digest` ->
  `#bb.metric-definition.sha256.u575llnhve7cxdmkpsigxdsd4vcweerjooizbfoddfgb2bdgwkea`
- `#bb.metric.store.recovery-outcome` ->
  `#bb.metric-definition.sha256.7wjixmc3nxielbaemrurfgfzplliul3lccixzqki2hxj7q5s7uuq`

The first two are displayed definition bindings, so the sidebar and composite
policies reminted to
`#bb.policy.sha256.g6mcphlydli46s65l7ktx32e7t6hb7fykbeucyvtg3ojncfciulq`
and
`#bb.policy.sha256.cnvl55sgzhrx4mtg24vmuos64yfpvzv4otgllh3c56oyldyer35a`.
Their typed outcome ancestry intentionally reminted the optimization-v3 fixture:

```text
game evidence      #bb.receipt.sha256.25nwaglcyfen25dqe3q633polt6ty4ijo2uzjncpmx6iwru6smfq
composite input    #bb.frame-input.sha256.zaqvaiecyqsqrloypyz3nnwjkr6pkcqbhtjc76kd44aqycnlz64q
composite evidence #bb.receipt.sha256.uzlh7iy7nycepfpamw46zkortzezrzlgkeswsjiqxkzzlk2xmgua
display receipt    #bb.display-receipt.sha256.ofiv4cyikzdfap7ojewldz62763xlzk52vvybje7q6jvqpx5le2q
output             #bb.receipt.sha256.nfrtpif5xhdtepypbfjxgbglw5zlpkpx77w5virr5vkvaaddwjdq
receipt            #bb.receipt.sha256.wk4ljbygxtgmgsibjc5v25k3zl4h46jspcknv24ztcmrckutz5fq
```

Independent Python reconstruction gives output-payload SHA-256
`132dd2761787309a1db7acc5d683c7a27b3c1bb507a73cc0ed3a12070bf44890`
and receipt-payload SHA-256
`340ee293269338d74d8e86a64635195fb04e3b463deb053b077ff681ad4a4443`.
The prior rejected-v2 fields and tags remain unchanged rather than being
silently rewritten to the new policy.

The default optimized Windows executable for this checkpoint is
`target/display-artifact-journal-checkpoint/release/bitblock.exe`, 1,121,792
bytes, with SHA-256
`4B49B87FE35706C68891371B56F42819298BDD32323D81C2E781BD7729E508A6`.
Against the preceding DisplayArtifact-codec checkpoint, the complete file size
is unchanged and every PE section except `.text` has the same virtual size;
`.text` decreases from `0x000d7458` to `0x000d7418`, a 64-byte reduction. The
journal remains behind the research feature and is absent from this default
executable. Its additional proof surface therefore costs no bytes in the live
release at this checkpoint; the changed executable hash also binds the reminted
metric and policy definitions above.

## Independent analytic RGB/depth renderer checkpoint

The first independent scene-semantic renderer reference is a stdlib-only Python
tool, `tools/reference_renderer_vectors.py`. Production projects visible cube
faces, splits each quad into two triangles, samples pixel centers with
barycentric coordinates, and perspective-interpolates depth. The reference does
not call or translate that path: for an exact 90-degree pinhole camera it sends
one unnormalised projective ray through every pixel center and intersects the
six unit-cube face planes directly. Its only read of Rust source is a textual
gate proving that all 58 retained sparse output rows equal the independently
generated rows; it neither imports nor executes Rust renderer code.

The declared corpus contains two direct-cube fixtures:

```text
front-bedrock      17 x 13  camera (0, 0, -2.5)  1 block   221 pixels   9 finite depths
offset-occlusion   31 x 23  camera (2, 2, -5)    4 blocks  713 pixels  25 finite depths
total                                                      934 pixels  34 finite depths
```

Both cameras have zero yaw and pitch, observation distance 1, and vertical FOV
90 degrees. Every cube is proved larger than the production subpixel threshold
and the complete bounds are proved outside aggregate saturation before the
comparison. The corpus therefore excludes the aggregate, subpixel-point,
inside-bit, and near-plane shortcuts by construction. It also excludes arbitrary
yaw, pitch, FOV, boundary-sensitive cases, other worlds/viewports, and any
cross-platform arithmetic claim.

The independent known answers are frozen as:

```text
reference profile  #bb.policy.sha256.smpqhfhoadryvrzezww32nve3cxkkc7tr4ubctfnhew7cscp64ua
candidate profile  #bb.policy.sha256.tsxgj54fonmrrlkoygcn5dtv65iu6neymvyee3x6hartgevwyynq
front frame         #bb.frame.sha256.obrjmzpxoggqqmwukec7lpl6f24bcg63nn2l6q65ync2jxfeetra
front depth         #bb.receipt.sha256.76vbzm46kufgtyjq7andifkzwbkotvb4p54b2yl35sla4kv3cwea
offset frame        #bb.frame.sha256.yrvobugmwykavefvg5xmrvzl3ugqb4scyjr32bdv5mrgag642c6a
offset depth        #bb.receipt.sha256.2r36dtvymqot5hjqb2zkz3ru7fe5ki6waqclvfhgtzg5ngmegcuq
corpus              #bb.receipt.sha256.x2m63bxhu5ayhccnnij3ohv6ux7x3uk3hgal2qy334qb2atkam5q
```

Rust reconstructs the full 934-word XRGB buffer and full 934-lane depth buffer,
including the 876 implicit background/positive-infinity rows omitted by the
sparse constants. It retains shape mismatches before comparing the common
prefix. Post-crosshair XRGB matches at every index and all depth finite/infinite
classes match. Nine of the 34 finite lanes differ in their raw `f64` bits:
indices 382, 383, 413, 414, 446, 447, 448, 479, and 510 have signed residuals
`+1, +2, +1, +2, +1, -1, -2, -1, -2` ULP respectively. The observed maximum is
2 ULP under a predeclared 4-ULP acceptance bound. Extra trailing RGB and depth
lanes are powered shape failures rather than being silently discarded by
iteration truncation.

The corpus tag is independently reconstructed on the Rust side from the oracle
profile digest, actual candidate renderer-profile digest, exact fixture names,
dimensions, all ten camera bit lanes, ordered block coordinates, actual frame
tags, and frozen reference-depth tags. Thus an output-preserving camera or fully
occluded-block change still breaks input identity. The tag is not yet a complete
equivalence-result receipt: source/executable identities, a shared-dependency
manifest, the four-ULP comparison policy, and attempted/completed counts remain
unbound and are reported as partial proof scope.

Two isolated mutations powered the final semantic gate. Changing production
`ORDINARY_SIDE` from `0x00bdbdbd` to `0x00bdbdbc` produced exact RGB mismatches
at indices 383, 414, 448, 479, and 510. After restoration the test passed.
Separately adding `1.0e-9` to production perspective depth produced nine front
fixture residuals of 2,251,800 ULP at indices 92, 93, 94, 109, 110, 111, 126,
127, and 128. After restoration the test passed. These deliberate red runs are
controls, not passing test counts.

No metric family was added; the catalog remains exactly 1,112. Seven existing
definitions reminted to bind this evidence:

```text
sampled raster false negatives       #bb.metric-definition.sha256.noreb57xvh47usl6tqzs7imebn7x52grecyq5hr52vsq2rydfy3q
sampled raster false positives       #bb.metric-definition.sha256.snlfdc5gcns4erzyabheqqmubvs5m4wekv6qizadrmo4u3mi5ykq
candidate/reference mismatch vector  #bb.metric-definition.sha256.ndpg6puukse3voujhfdxlk4s34pu5bhsz5yg3g4bdwp7f2b4zzra
sampled raster depth residual vector #bb.metric-definition.sha256.5o4zsqxwf6bwdqvooysfyqzuyqejceo232vgqfgzbwmsi2uzo5da
renderer equivalence scope           #bb.metric-definition.sha256.y6kusn4t3os3ss4kpyebz36jpyv3pggokk22elijyo74ifa25ohq
known-answer vector verdict          #bb.metric-definition.sha256.otvsq65p24hcmev5qeefmisee74kb2waspi3sz5ffyzeprnutkaq
cross-implementation digest parity   #bb.metric-definition.sha256.wmmga7w3kg254myf3r4arktvczb4lnrjp7vcon3n77smmptwdldq
```

The same audit corrected one pre-existing stale status: canonical `FrameInput`
v1 hashing and contextual live reconstruction were already implemented, so
`#bb.metric.hash.canonical-render-input-digest` reminted from its obsolete
`Future hash` wording to
`#bb.metric-definition.sha256.jkyz4xxly7x5jcx3lxyik6rxufc2ytyozma2nlomndnabaifpy7a`.
This is an evidence-status correction, not an eighth oracle result and not a new
metric family.

None is bound into the live sidebar source, so the sidebar and composite policy
tags remain `#bb.policy.sha256.g6mcphlydli46s65l7ktx32e7t6hb7fykbeucyvtg3ojncfciulq`
and `#bb.policy.sha256.cnvl55sgzhrx4mtg24vmuos64yfpvzv4otgllh3c56oyldyer35a`.
This CPU-side corpus neither advances the GDI sample nor says anything about
compositor transforms, scanout, panel output, or physical light.

The complete checkpoint gate passes 220 library and 28 application tests in
both default and all-feature all-target configurations, strict Clippy over all
targets/features, formatting, all eleven independent Python vector tools, the
exact 1,112-definition metric audit, and `lake build` (6 jobs). The two powered
subject mutations are deliberately failing controls and are not included in the
passing counts. No app, live screen capture, compositor assay, scanout assay, or
physical-display assay was run.

The default optimized Windows executable is
`target/independent-renderer-oracle-checkpoint/release/bitblock.exe`, 1,121,792
bytes, with raw SHA-256
`038217353C4EB1B07B276ED7CF42AC222C54410B2AD4BF0A7F42A703A220847A`.
Compared byte-for-byte with the preceding typed-journal release, exactly four
bytes differ: two bytes in the PE COFF timestamp field at offsets `0x88..0x89`
and two in its derived checksum at `0xd8..0xd9`. Every mapped PE section byte and
every section size is identical. Replacing the complete timestamp and checksum
fields with the preceding values reproduces the preceding release SHA-256
`4B49B87FE35706C68891371B56F42819298BDD32323D81C2E781BD7729E508A6`.
Thus the test/tool-only oracle adds no mapped live code or data bytes; the raw
file hash still truthfully records the fresh linker timestamp and checksum.

## Explicit live display-artifact archive checkpoint

The Windows observer now has one feature-gated, user-triggered route from a
successfully committed composite presentation to the canonical
`DisplayCaptureArtifact` and typed local journal. It is excluded from the
default build and enabled only with `--features protocol-research`. Startup
reads `BITBLOCK_DISPLAY_JOURNAL_NEW`, creates and synchronizes that exact path
with create-new semantics, and refuses an existing path. This mode is mutually
exclusive with the older continuous `BITBLOCK_DISPLAY_AUDIT` chain, so one
presentation cannot accidentally create two GDI samples under two unrelated
display-receipt ancestries.

F4 is a non-repeating one-shot request. It changes no world, camera, tick,
input, render-dirty, sidebar-dirty, pending-frame, or frame-ledger authority.
When the compositor still contains the most recently committed output, F4 asks
the host for a redraw and samples that occurrence directly without producing a
tick or resubmitting the surface. If a presentation is already pending, the
request waits for that successful submission. Before any GDI call, the path
requires all of the following:

```text
receipt.stream                 = composite
receipt.tag                    = current composite FrameLedgerV2 head
receipt.verify                 = accepted
receipt.frame                  = exact observed compositor frame tag
retained presented observation = exact observed compositor facts and tag
```

The stale control uses an older valid receipt whose framebuffer bytes are
identical to the new head. It is refused, proving that equal pixels cannot
substitute for the selected presentation occurrence.

For a newly submitted frame, both v1/v2 frame ledgers, presented observations,
sidebar facts, and pending-state cleanup are published before capture or
filesystem work begins. A later capture or store refusal therefore cannot
leave already-advanced frame heads paired with stale pending frames and cannot
roll presentation authority back. The observational sequence is then:

```text
capture_visible_client(expected FrameRef)
  -> complete DisplayCapture including any post-readback failure pixels
journal.prepare_capture_artifact(exact returned composite FrameReceiptV2,
                                 expected FrameRef,
                                 DisplayCapture)
  -> owned canonical DisplayCaptureArtifact
journal.append_artifact(artifact)
  -> preflight -> append -> sync_all -> EOF check -> exact reread -> typed publish
```

`Match`, `Mismatch`, pre-readback `Unavailable/Absent`, and post-readback
`Unavailable/Inline` are all completed observations and append as distinct
canonical artifacts. They are never retried away. A nonpoisoning append refusal
retains the exact already-owned artifact; a later explicit F4 retries those
same bytes without another GDI call. A poisoned or uncertain result blocks all
blind retry and requires an explicit typed reopen. This checkpoint is
create-only: it does not silently reopen, discard a tail, rebase a display
parent, rotate, truncate, or overwrite. Anchored reopen and candidate-present
reconciliation remain open.

No new metric family was admitted; the exact registry remains 1,112. The two
definitions whose evidence status changed reminted to:

```text
compositor-capture comparison
  #bb.metric-definition.sha256.jcciaxrmp7sjkxmqepxixydbhh5m77ufqgfyxeirjqiccwuf6pma
frame-provenance receipt digest
  #bb.metric-definition.sha256.usv2hrln45vdwrgz4e4xraime7x53fjyjlv5mfkr6plukbzb5j4a
```

Both definitions are bound into the sidebar source, so the derived policies
are now:

```text
sidebar policy   #bb.policy.sha256.64tvzqwugmd4ozi3ail36qnxgtqb22yffwmgis7uwcpvkwgsxe3q
composite policy #bb.policy.sha256.oplhffpxvhyz3t3a57ruelblclcosthaawz2o3rdemwz4k5ufdfa
```

The optimization-v3 fixture retains that changed composite policy inside its
typed outcome ancestry, producing this independently reconstructed successor
set while leaving the rejected-v2 history unchanged:

```text
game evidence      #bb.receipt.sha256.qxwvrhf7qrqvpv2uqljlmojx5rlcrrpgsoby7daczln3dokynzhq
composite input    #bb.frame-input.sha256.h6p2xcfudpks5pkzojp243mdrjv6oazzbwmbahlqltajd6oabaja
composite frame    #bb.frame.sha256.isqpvp4w6kox3bcrmr53hvkfqqnnsvxgdy6yb77dwqwxjdwd2wha
composite evidence #bb.receipt.sha256.pth2dviss6z3d7k3f3rdw46qqje7sfnszxorm4xgyffw2zqtrjia
display receipt    #bb.display-receipt.sha256.dtfnqhj7n3rixnhnl2fpy575lhuhhp4bittri4ww4cjnhobcckaq
output             #bb.receipt.sha256.o3gvshgcgzi3e3xrr5zhwstrrur2fqvr4jsayfoetfz4ywiknu3a
receipt            #bb.receipt.sha256.qnnz2rhcoeybcdh7fhm63k2pslayiw3mmmeqlnghnndpffb4lrqq
```

The independent output payload is 290 bytes with SHA-256
`57DF46A60DC4FF4B0F940CCFFD6F9AE04D62D13A75F9F9305069C9F40EF7D286`;
the 256-byte equivalence-receipt payload has SHA-256
`334360165BADE9CDBEB98E864BA2E85942EEDC5CABBEEB850A6C41E7180416B6`.
The registry audit still reports 489 system, 339 view, and 284 world families,
with 1,112 unique readable names and definition tags.

The automated path checks positive transient-to-artifact admission for match,
mismatch, pre-readback unavailable, and post-readback unavailable pixels; the
current-head/same-pixel stale refusal; unchanged simulation authority after F4;
and exact artifact retention across repeated nonpoisoning store refusal. The
three powered subject controls removed the live-head equality check, dirtied the
sidebar from F4, and discarded the retained candidate on append refusal; each
named test failed before the exact production line was restored. The
generic carrier failpoint matrix remains the evidence for write/sync/EOF/reread
cuts; direct injection at the typed live adapter is still compositional rather
than one end-to-end fault seam. No app window, live HWND/GDI capture, occluder,
scanout, or physical-display assay was run for this checkpoint.

The complete gate passes 221 library and 29 application tests in the default
all-target build, 221 library and 31 application tests with every research
feature, zero-test example targets, strict Clippy over all targets/features,
formatting, all eleven independent Python vector programs, the exact metric
audit, and `lake build` (6 jobs). The three deliberately failing powered
controls above are not counted as passes.

The cheap default executable is
`target/live-display-default-checkpoint/release/bitblock.exe`: 1,120,768 bytes,
SHA-256
`06373BDD1D1BE3EDF4700A6483F6868C743385D81F5655D7CA0D163E9F66A549`.
Against the preceding independent-renderer checkpoint it is 1,024 bytes
smaller; read-only PE inspection reports `.text -656`, `.rdata +128`,
`.pdata +12`, `.xdata -20`, and `.reloc -8` virtual bytes, with all other
section virtual sizes unchanged. The canonical DisplayArtifact schema string
is absent, confirming that the journal/artifact implementation remains outside
the default linked image. This binary still contains the small fail-closed F4
message/configuration guard and the newly reminted displayed definition tags.

The usable research executable is
`target/live-display-artifact-checkpoint/release/bitblock.exe`: 1,210,880
bytes, SHA-256
`F550E16917BEFC3268472A98933E2F382B8AE527D5ABB64092BF83632A3DE47E`.
Relative to the same-source default image, the feature costs 90,112 file bytes.
Its PE virtual-size deltas are `.text +71,088`, `.data +16`, `.rdata +14,208`,
`.pdata +1,500`, `.xdata +2,716`, `.bss -4`, `.idata +80`, and `.reloc +348`
bytes. A read-only string assay finds `bitblock/display-capture-artifact/v1` in
the feature image and not in the default. These are linked-size facts, not a
runtime memory, latency, sync-duration, or durability measurement.

## Checked Win32 pixel-submission checkpoint

Read-only inspection of the pinned Softbuffer 0.4.8 Windows backend exposed a
proof seam: its internal `BitBlt` Boolean was discarded before the wrapper
returned `Ok(())`. The frame protocol already described that return as backend
acceptance, not monitor output, but there was no way to distinguish a failed
underlying GDI call from a successful one.

Windows now uses a project-owned top-down `0x00RRGGBB` buffer and one direct
`SetDIBitsToDevice` occurrence. The buffer is fully allocated before replacing
the prior resize state. Each present acquires the window DC, submits exactly the
current width and height, captures the returned scanline count and Win32 error,
releases the DC, and admits the frame pair only when the returned count equals
the complete requested height. Zero, partial, and over-reported row counts are
all refusal states. A failed `ReleaseDC` remains a separately reported cleanup
fact after a full-row return, because rewriting an already accepted pixel call
as non-submission would be false history.

The checked policy is independently reconstructed by
`tools/windows_present_vectors.py`:

```text
#bb.policy.sha256.opgpybj32tw72pep2xusit4lzzgsiozs5f5reppq67iiejueetvq
```

The Windows dependency graph is now `bitblock -> windows-sys + winit`; it has no
Softbuffer node. Non-Windows targets retain Softbuffer as a target-specific
backend. This changed the evidence status of two existing metric families but
admitted no new family:

```text
backend submission event
  #bb.metric-definition.sha256.ip6vniaztq3chlyhj6w5fdysj3fi3y7evaiaah3pt65xdoak2ufa
mapped-surface copy mismatch vector
  #bb.metric-definition.sha256.yl4hegsrqnqx2a26j53h6tpp6duv4ozof43eyc5xm2as2k4af3uq
```

The first live checked-backend assay retained a partial external-raster failure
with 516,522 mismatches, first `(565,0)`. A second run retained a whole-client
external-raster failure with 962,001 mismatches, then produced complete
full-client passes in the same process. The first preserved exact pass was:

```text
#bb.display-receipt.sha256.2swl7ga2lqxiswavxqt4mznrxku4u7hcqfsxccvpjytt2hwfz2taq
stage=Complete mismatches=0 first=None
```

A later minimized occurrence was explicitly unavailable rather than counted as
a match. Exact failure and minimized receipts are preserved in
`docs/DISPLAY_AUDIT.md`. The sequence establishes checked GDI acceptance plus a
separate exact OS screen-raster sample for those occurrences. It does not settle
the mechanism of the preceding external rasters and does not prove compositor
attribution, scanout, cable/panel behavior, or emitted light.

Two powered subject controls were observed failing before restoration:

1. replacing complete-row equality with `actual >= 0` made the zero-row control
   fail;
2. changing the Rust policy revision changed its tag and made the independent
   known-answer assertion fail.

The complete gate passes 227 library and 29 application tests in the default
all-target build, 227 library and 31 application tests with all research
features, zero-test example targets, strict Clippy, formatting, all twelve
independent Python vector programs, the exact 1,112-definition registry audit,
and `lake build` (6 jobs). The deliberately failing powered controls are not
counted as passes.

The default executable is
`target/checked-windows-present-default/release/bitblock.exe`, 1,031,168 bytes,
SHA-256
`4BC8528A5C9CA11D2F8EB6CB57F1D81BABA9819A56BA451834B294CF12D8D71A`.
It is 89,600 bytes smaller than the preceding 1,120,768-byte default. Read-only
PE comparison attributes the largest virtual-size reductions to `.text`
(-71,456), `.xdata` (-8,696), `.rdata` (-6,256), and `.pdata` (-2,436), with
smaller `.data` (-176) and `.reloc` (-276) reductions and `.idata` +4 bytes.
The presenter policy and `SetDIBitsToDevice` strings are present; `softbuffer`
and the research display-artifact schema are absent.

The research executable is
`target/checked-windows-present-research/release/bitblock.exe`, 1,122,304 bytes,
SHA-256
`6306D0158BE5C8D2C202B91CB0C2455B3D95E4948FE0F564409DEADF05633B47`.
It is 88,576 bytes smaller than the preceding research image and 91,136 bytes
larger than the same-source default. The display-artifact schema and checked
presenter policy are both present. These are linked file/section facts, not
runtime latency, energy, residency, or compositor evidence.

`DwmGetCompositionTimingInfo` remains a possible diagnostic envelope, not the
next pixel proof. Microsoft requires a null HWND on Windows 8.1 and later, so
its timing/counter vector is compositor-session scoped and cannot name the
Bitblock window or frame. Any future before/after pair must preserve attribution
as `Unknown`; it may not promote counter movement into displayed/dropped fate.

## Open proof obligations

- Canonical registry-patch and key codecs; durable decoders for the currently
  implemented world, frame-input, metric-snapshot, composite-input, overlay,
  frame-receipt, and display-receipt-v1 encodings. DisplayReceipt v2,
  DisplayArtifact v1, OutputTopology v1, and PresentationOutput v1 have strict
  research codecs. The typed local journal now replays mixed legacy display
  artifacts and presentation-output wrappers through one physical chain and
  one embedded display-receipt chain, but has no canonical journal-receipt
  codec, sudden-power assay, external custody, or latest-prefix authority.
- Executable edit semantics and a compiler/refinement proof for the current
  representation-only Edit-to-LadderLang bridge; all other object kinds remain
  refused.
- Independent corpus parity for every canonical SHA-256 tag domain.
- Process-death and sudden-power-loss assays across declared Windows
  filesystem/device/cache configurations; current failpoints prove the byte
  protocol, not hardware persistence.
- Canonical tagged journal-open, append, recovery, and discard receipt codecs;
  current Rust receipt structs are local observations.
- Independent typed-history replay/refinement evidence; the current Rust
  producer and verifier share edit, event, world-tag, and receipt-tag semantics.
- Durable sequential-segment and coordinate-evidence codecs/storage, plus an
  intentional receipt-mode epoch; the current result is a whole-history
  in-memory sidecar and cannot verify independent receipt ranges or suffixes.
- Independent sequentiality/VDF analysis and external calibration/anchors
  before any minimum-work, elapsed-time, or Proof-of-History claim. Serial
  SHA-256 recomputation alone supplies none of those properties.
- Independently retained history/segment heads and an explicit branch/fork
  authority rule before the local verifier can identify a latest history.
- Deliberate persistence in the default observer and explicit anchored reopen,
  recovery, and candidate reconciliation in the feature-gated live observer.
- Direct typed-append fault injection across uncertain write, sync, EOF, and
  reread phases if evidence beyond the current carrier/typed composition is
  required.
- Rust implementation refinement against the Lean world model.
- Independent semantic replay/refinement and a versioned genesis-context
  verifier for the live parent-chained TickOutcome v3 sidecar. Production and
  structural links are active, but their current tests share Rust transition
  code.
- Shared/durable outcome storage and safe pruning. The live app retains every
  outcome, atomically publishes independent game/composite v2 structural
  admissions around one backend call, and binds both the opt-in display ledger
  and research optimization assay through typed v2 evidence. FrameReceipt v1
  remains a separately staged compatibility epoch for the sidebar predecessor
  field only.
- Durable codecs and independent reconstruction for optimization case, output,
  and equivalence receipts; the current assay is a local research API.
- Deterministic/fixed-point or otherwise narrowed cross-platform raster profile.
- Independent reference coverage beyond the current bounded direct-cube oracle:
  aggregate, subpixel-point, inside-bit, near-plane, arbitrary yaw/pitch/FOV,
  boundary-sensitive, larger corpus, and universal or theorem-backed raster
  semantics remain open.
- Repeated live Match/Mismatch/Unavailable sampling with intentional occluder,
  display-mode, DPI, HDR, multi-monitor, and remote-session controls. The current
  local assays cover exact pass, external-raster mismatch, and minimized
  unavailable outcomes on one configuration only.
- Calibrated scanout or physical-display instrumentation; GDI screen capture
  does not settle this boundary.
- External head custody for rollback detection.
- Ownership and authorization before economic mint/burn.
- All distributed Layer 1 requirements listed above.

An unavailable obligation is reported as `N/A` or open. It is never inferred
from a hash, a metric, or a successful local run.

## Composition-timing envelope checkpoint

`src/presentation.rs` adds a feature-gated, platform-neutral canonical object
for raw Windows composition timing around one checked presentation. Its readable
kind is `presentation-artifact`, its schema is
`bitblock/composition-timing-envelope/v1`, and its payload is exactly 948
big-endian bytes:

| bytes | committed field |
|---:|---|
| 32 | composition-timing policy digest |
| 130 | complete composite `FrameReceiptV2` canonical payload |
| 52 | checked presenter policy and result facts |
| 13 | QPC-frequency result |
| 319 | pre-presentation DWM call, bracket, and optional snapshot |
| 26 | checked-presenter QPC bracket |
| 27 | `GdiFlush` result and QPC bracket |
| 30 | `DwmFlush` HRESULT and QPC bracket |
| 319 | post-presentation DWM call, bracket, and optional snapshot |

Each successful DWM call stores all 288 bytes of documented timing fields in
API order after omitting caller-owned `cbSize`. The codec encodes every integer
explicitly; it never copies the packed Windows ABI struct. Failed calls retain
their HRESULT, use an absent marker, and require zero canonical snapshot
padding. Boolean markers, nested tag kinds, the composite stream selector,
presenter row completeness, padding, payload length, and the cached outer tag
all fail closed.

`src/windows_timing.rs` is linked only by `protocol-research`. With
`BITBLOCK_PRESENTATION_AUDIT=1`, the live Windows path executes this order:

```text
QPF
{QPC, DwmGetCompositionTimingInfo(NULL), QPC}
{QPC, checked SetDIBitsToDevice presenter, QPC}
{QPC, GdiFlush, QPC}
{QPC, DwmFlush, QPC}
{QPC, DwmGetCompositionTimingInfo(NULL), QPC}
```

All post-presentation observation boundaries still run when another timing or
barrier observation fails. Only failure of the checked presenter prevents frame
admission. If construction of the observational envelope were unexpectedly
refused after presenter success, the frame receipt remains published and the
observation refusal is reported separately; an instrument may not roll back
the event it observed.

The derived session relation has only four values:

- `Unavailable`: a missing/failed required observation, barrier failure, or
  incomplete/nonmonotonic QPC interval;
- `DiscontinuityOrReset`: both complete snapshots exist but configured timing
  rates differ or a declared sequence lane regresses;
- `ComparableNoProgress`: a complete stable pair contains no admitted progress;
- `ComparableAdvanced`: a complete stable pair contains a sequence advance or
  documented second-call interval activity.

The exact-frame attribution type deliberately has only `Unknown`. Microsoft
requires a null HWND on Windows 8.1 and later, so these counters describe the
desktop composition session rather than the Bitblock window or receipt.
Movement cannot be promoted into displayed, dropped, duplicated, scanout, or
physical-light claims. The official API boundary is documented by
[`DwmGetCompositionTimingInfo`](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/nf-dwmapi-dwmgetcompositiontiminginfo)
and [`DWM_TIMING_INFO`](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ns-dwmapi-dwm_timing_info).

`tools/composition_timing_vectors.py` independently reconstructs the full
layout, Windows presenter policy, nested receipt payload, and content-tag
framing. Rust and Python agree on:

```text
policy    #bb.policy.sha256.t2zuewerksnmvhhjyaaplmlmfb2poptubqxpc6czwvmuwufpanrq
artifact  #bb.presentation-artifact.sha256.sw2i52veohfmc3lqe74tzd72n6ppwxxshejxmazclwikgniqx2sq
```

One powered subject control removed the GDI-barrier requirement from the
derived comparison. The dedicated test failed with `ComparableAdvanced`
instead of `Unavailable`; the exact production condition was restored and the
test passed. A session-owned live run then emitted the content tag
`#bb.presentation-artifact.sha256.pmvfnxmzcjnz5ag26wmpdkdhbpoiofv4grxmwigofifjxj57kgya`
with QPF success, both DWM calls at `S_OK`, successful GDI/DWM barriers, derived
`ComparableAdvanced`, and exact-frame attribution still `Unknown`. The process
was stopped with Ctrl+C after the occurrence. Its raw 948-byte payload was not
journaled, so this terminal receipt is not replayable object custody. This
checkpoint does not yet query active-output topology, persist timing artifacts
in the display journal, attribute a frame inside DWM, or observe scanout/light.

The changed definitions remint these existing families without adding a new
metric family:

```text
display pipeline state vector       #bb.metric-definition.sha256.ptv77nvnpcousi7k5dbv426xfo3ap6x5ypusdbq4cudatrxhfgkq
presentation fate relation          #bb.metric-definition.sha256.zt5psfzfpfyjdhaz3lenuwab363purdwj5ivm4bqfmw7scyavi3a
domain-separation identity           #bb.metric-definition.sha256.ihlgxhjgwamgu57eostm6m2w54gxlssazapkm5r6i5j77uk6m4pa
encoding round-trip check            #bb.metric-definition.sha256.jyxxeqpzmsteqxuquyhlkagnqmjdmxaqxhfg4lyjb4p6erl3llya
known-answer vector verdict          #bb.metric-definition.sha256.mymmhhznsxbc5zctdsyn6hx74bzfw3ofgbl6pc4li6lm6mt4twdq
cross-implementation digest parity   #bb.metric-definition.sha256.jkedpqcfxrjvyq7xvzbnxvsexznxeprbvnw2eq76h7jnuyslucbq
```

The final gate passes 237 library and 29 application tests in the default
build, 240 library and 31 application tests across all research targets, all
zero-test example targets, strict Clippy for both feature sets, formatting,
all thirteen independent Python vector programs, the exact 1,112-definition
registry audit, and `lake build` (6 jobs). The deliberately failing powered
control is not counted as a pass.

The fresh default executable is
`target/composition-timing-default/release/bitblock.exe`, 1,031,168 bytes,
SHA-256
`A4122AFA24BC56EA888A18B261160B8B00D73D150A53E44C9977A2DF4724C980`.
It is exactly the same file size as the preceding checked-presenter default.
The feature-gated timing implementation is absent, but reminted metric
definition strings deliberately change `.rdata`; therefore equal size is not
reported as byte identity.

The research executable is
`target/composition-timing-research/release/bitblock.exe`, 1,144,320 bytes,
SHA-256
`392968151D1BC114B91452588FDD1A533BD3A7CB4D77259D046226678B36739E`.
It is 22,016 bytes larger than the preceding research artifact and 113,152
bytes larger than the same-source default. Relative to the prior research
image, PE virtual-size deltas are `.text +18,800`, `.rdata +2,816`, `.pdata
+120`, `.xdata +240`, `.idata +52`, `.reloc +80`, and zero for `.data` and
`.bss`. These are linked-file facts, not runtime latency, energy, or residency
measurements; the runtime queries execute only when the explicit audit variable
is enabled.

## Active-output topology and presentation-output checkpoint

This checkpoint adds no metric family. The exact registry remains 1,112
readable, uniquely tagged families: 489 system, 339 view, and 284 world. Active
route count, acquisition-attempt count, retry rate, last topology error, route
stability, and the new object digests are projections, labels, or carrier
identities for observables already owned by the display-pipeline,
compositor-capture, presentation-fate, provenance, and recovery families.

### OutputTopology v1

`src/output_topology.rs` defines the readable kind `output-topology` and schema
`bitblock/output-topology-observation/v1`. The object has explicit `Available`
and `Unavailable` forms. An available observation retains:

- client, monitor, and work-area rectangles plus the exact GDI source name;
- source position, dimensions, pixel format, status flags, and optional mode;
- every matched target route, including exact target identity availability,
  output technology, rotation, scaling, refresh/signal/scanline facts, path
  flags, and optional desktop-image facts; and
- the ordered acquisition-attempt transcript, including query profile,
  outcome, error, and reported capacities.

Windows UTF-16 code units are preserved exactly. Routes are canonically sorted,
and two materially identical routes are refused even when only their friendly
annotations differ. The payload is capped at 65,536 bytes, each retained UTF-16
field at 8,192 code units, and both routes and attempts at 256. Transient HWND,
HMONITOR, adapter LUID, source/target numeric IDs, clone-group IDs, and mode
indices are deliberately absent; they are query-time joins rather than durable
identity.

`src/windows_topology.rs` maps the owned HWND to its monitor and GDI source, then
uses `QueryDisplayConfig`. It first requests profile `0x52` and falls back once
to `0x12` only for `ERROR_INVALID_PARAMETER`. Only
`ERROR_INSUFFICIENT_BUFFER` can retry, with four total size/query cycles and
allocation caps of 1,024 paths and 3,072 modes. The source-name join is exact,
all active clone-target routes are retained, and target-name failure is an
explicit route fact rather than silent omission. Production and fake-API paths
apply the same material-duplicate refusal.

The independent Rust/Python vectors agree on:

```text
available    #bb.output-topology.sha256.jbfwsd34kns7uwofvkj6w3u7tq7nlsbqdfw5b725pxpcg44c2h2q
unavailable  #bb.output-topology.sha256.fw2l66z7jd2eu6suedannr77x5yrugb6sot7y3ojduamdiuemtpq
```

The independent oracle also refuses annotation-only material duplicates. A
powered Rust subject control weakened the production comparator to ordinary
struct equality; the annotation-only duplicate test then failed. Restoring the
material comparator returned the test to green.

### PresentationOutput v1, raw output color, and mixed replay

`src/presentation_output.rs` defines kind `presentation-output-artifact` and
schema `bitblock/presentation-output-artifact/v1`. Its canonical layout is:

```text
u64 display_len | DisplayArtifact
948-byte CompositionTimingEnvelope
u64 topology_before_len | OutputTopology before
u64 topology_after_len  | OutputTopology after
```

The fixed outer overhead is 972 bytes. Admission requires exact equality of the
complete nested `FrameReceiptV2`, equal successful timing/capture QPC
frequencies, and a capture start QPC no earlier than the timing envelope's
post-presentation call bracket. `Stable`, `Changed`, and `Unavailable` route
stability are derived by comparing the two topology observations and are not
encoded. The before/after positions record workflow order; without their own
timestamps they do not prove the physical time at which topology changed.

The independent outer vector is:

```text
#bb.presentation-output-artifact.sha256.fhc42zlstxtkh5mzyhpyjtlpuwo5dna5ryklwjithph577okripa
```

Twelve negative controls cover truncation/trailing bytes, nested kind and
receipt substitution, timing/capture QPC-frequency mismatch, capture before the
timing post-call, and malformed nested objects. `src/display_store.rs` now
dispatches strictly by kind and schema and replays every adjacent transition:
legacy-to-legacy, legacy-to-wrapper, wrapper-to-legacy, and wrapper-to-wrapper.
The physical storage-parent chain remains separate from the unified embedded
display-receipt-parent chain. Replay inventories the stored outer object plus
its display, timing, topology, and derived-stability tags. It does not supply
the referenced TickOutcome, FrameInput, or policy objects and therefore is not
transitive content-reference closure.

`src/output_color.rs` adds the separate `output-color-observation` v1 carrier.
It binds an ordered raw Windows Advanced Color result to the exact canonical
`OutputTopology` tag: each route carries either opaque `flags_raw`,
`color_encoding_raw`, and `bits_per_color_channel` words or a query error. The
object never retains transient target IDs, decodes a mode as HDR/WCG, or claims
composition, scanout, emitted light, or physical pixels. Its stdlib-only Python
oracle (`tools/output_color_vectors.py`) checks canonical tags and rejects
malformed, changed-parent, and noncanonical candidates.

`src/presentation_output_v2.rs` adds a schema-v2 wrapper under the existing
`presentation-output-artifact` kind. Its payload retains byte-for-byte v1
bytes plus exact pre/post `OutputColorObservation` bytes. Admission verifies
each color object against its respective nested v1 topology. The only derived
configuration relation is raw `Stable`, `Changed`, or `Unavailable`, and it is
not serialized as a claimed display mode. Journal replay dispatches v1 versus
v2 by exact schema and keeps the legacy/v1/v2 objects in one storage-parent and
embedded display-receipt-parent chain. V1 inventory entries have no color tags;
v2 entries retain their pre/post color tags and raw configuration relation.

### F5 authority order and live assay

F5 arms one observation of the next normally produced successful composite
presentation. Arming requests a redraw only to service UI state; it does not
advance a tick, render a new authoritative frame, or resubmit the current
surface. An invalid or uncommitted frame leaves the request armed. A valid
presentation consumes it exactly once in this order:

```text
checked presentation and committed composite receipt
same-call CompositionTimingEnvelope
paired OutputTopology and raw Advanced Color before capture
one visible-client GDI capture
paired OutputTopology and raw Advanced Color after capture
canonical v2 wrapper admission and synchronized journal append
```

A nonpoisoning storage refusal retains the exact already-observed wrapper for a
storage-only F5 retry; it does not resample timing, topology, raw configuration,
or pixels. F4 and F5 are mutually exclusive while either request or retained
object is pending.

A fresh live Windows run used
`target/live-f5-assay-20260826-02.bbj`. F5 first changed the title to
`waiting for next changed presentation`; F3 then caused an ordinary sidebar
recomposition. The immediate UI snapshot still carried the waiting title while
the asynchronous observation was in flight. A new observation 1.5 seconds
later read `presentation output stored 1`. The terminal and synchronized
journal reported:

```text
timing             #bb.presentation-artifact.sha256.6m6xleoinmqxdxvfzbp3isk2czftvvn5t32ulr3uda7nrfxcxipa
progress           ComparableAdvanced
attribution        Unknown
qpf                true
pre_hr/post_hr     0 / 0
gdi_flush          true
dwm_flush_hr       0
output             #bb.presentation-output-artifact.sha256.ylodhhwp46a643xiuxd4wnwesgrmrkmiprjclucmfi2qrqnio62q
display            #bb.display-artifact.sha256.hcbzgr23hbk3y65uoevxnfcalbg5eqgkmr5ddu2rbrxvvd2pnria
topology_before    #bb.output-topology.sha256.u7en2ajcy5cwiy2ugaozrhdjrko5e3i3apmlm6kafnpmq5s4ohca
topology_after     #bb.output-topology.sha256.u7en2ajcy5cwiy2ugaozrhdjrko5e3i3apmlm6kafnpmq5s4ohca
route_stability    Stable
storage_record     #bb.storage-record.sha256.6zf3y3jxcwelftieje7toltjpeyv4kxpmgpb6svmvxcnlfcbqa2q
record_range       [0, 3850988)
artifact_count     1
```

After the process was stopped, the journal was 3,850,988 bytes with file
SHA-256 `5617A4B05B698E874E8DAF37226B5B4FF4967D54F41779B5979A1BDACF7622CF`.
The live append path synchronously reread its record and called `sync_all`; this
run did not perform a separate anchored reopen or sudden-power interruption.
Identical topology tags establish equality of the two retained route facts for
this sample only. They do not establish continuous stability between samples.
The null-HWND DWM relation remains session scoped, so exact-frame fate is still
`Unknown`. The OS raster, compositor attribution, scanout, cable/panel state,
and emitted light remain separate evidence boundaries.

### Metric remint, policies, and final gates

Nine existing definitions changed at that checkpoint; none was a new family.
Four live registry pointers in this historical list were advanced again by the
later InteractionTape checkpoint:

```text
compositor capture comparison       #bb.metric-definition.sha256.frvbp7vzozw7sag5klwkekaipni4peiv4lx4qswmmi7ypbfc7giq
display pipeline state vector       #bb.metric-definition.sha256.hs534mepahw52fx5ehpa2wuc365usry7akwnimknuavkc3gljyoa
presentation fate relation          #bb.metric-definition.sha256.onmlgqlhezpgwer7pm5jpzumrfsoyhfre2k6cmmemvyt74jryxbq
domain-separation identity          #bb.metric-definition.sha256.b2uxj4hkepxslna7wlktcoe4thlszcxfqs43ze6judtsawi2ym6q
encoding round-trip check            #bb.metric-definition.sha256.2npmiwjtmvulaeodwxgpk4wx2pzpzvvlbbnd4jjn7iozkwnzsv2a
frame-provenance receipt digest      #bb.metric-definition.sha256.ntg7gxatkwreu2rhm7g72hunnpka7arngljoagz3bpdfozblrtma
known-answer vector verdict          #bb.metric-definition.sha256.hgtisylvm66vgwhxo76austomilhsuntnigkrljhxqnok4tgpapa
cross-implementation digest parity   #bb.metric-definition.sha256.e3twhjvyxiudc4mdldvvxkfh3ljog3gcltfarzxw7pknpnseisoa
store recovery outcome               #bb.metric-definition.sha256.ma25hfuvb7h64lips6tvha72wdjdb36ioiq4bxwcnpm2ixw6jx7a
```

The derived sidebar and composite policies are respectively
`#bb.policy.sha256.eg4egguiubrb5l45qbwvf76gwbkgtlb7pymc3zi4uq6jrqgl4b5q`
and
`#bb.policy.sha256.kyd57oandtrj4iuis54zozpv6pwgmcoy4hmsvlkp2fqvhy6noe5q`.
The remint was propagated through the optimization-v3 game, composite-input,
composite-evidence, display-receipt, output, and equivalence known-answer
chain; no stale predecessor tag remains in the exact registry or vector suite.

The final verification passes 290 tests in the default all-target build (261
library, 29 application), 317 across all research targets (281 library, 36
application), every zero-test example target, strict Clippy for both feature
sets, formatting, all fifteen independent Python vector programs, the exact
1,112-definition registry audit, and `lake build` (6 jobs).

The fresh default executable is
`target/presentation-output-default/release/bitblock.exe`, 1,031,168 bytes,
SHA-256
`6F6C320445FD4E4FD5337BE834C17DB0FC58E7760B1ABFF7474761EEAD71B70A`.
The research executable is
`target/presentation-output-research/release/bitblock.exe`, 1,277,952 bytes,
SHA-256
`963EF42F22A75C6DA7666A7A3FC3BAD72049C8BAA607A3365C76FAF20C7233A8`.
It is 246,784 bytes larger than the same-source default. These are linked-file
size and digest facts, not runtime latency, energy, or memory measurements.

## Accepted-action InteractionTape and headless-fold checkpoint

This checkpoint adds no metric family. The registry remains exactly 1,112
readable aliases with 1,112 unique definition tags: 489 system, 339 view, and
284 world. Tape identity, tick/action counts, opcodes, result tags, empty frame
heads, and resource-refusal labels are carriers, projections, or closed labels
owned by existing replay, input, hash, render, provenance, and deterministic
resource-admission families.

### InteractionTape v1

`src/interaction.rs` defines schema `bitblock/interaction-tape/v1` under the
existing readable `intent` kind. It records normalized actions already accepted
by the application, not raw platform events. The canonical big-endian payload
is:

```text
u64 record_count
u64 total_action_count
repeat record_count:
    u64 ordinal
    u32 action_count
    repeat action_count:
        u8 opcode | opcode payload
```

Ordinals must be exactly `0..record_count`; explicit empty records preserve idle
ticks. Opcodes 0 through 9 are respectively move edge, look delta, wheel lines,
aim delete, aim place, exact Edit, fit world, toggle sidebar, resize, and clear
motion. Movement stores one closed control byte plus a canonical Boolean. Look
and wheel store finite IEEE-754 `f64` bits, including signed zero, and resize
stores two `u32` lanes. Exact Edit embeds the existing fixed 25-byte encoding.
The decoder refuses undefined opcodes/controls, noncanonical Booleans,
nonfinite floats, noncontiguous ordinals, impossible/truncated counts, trailing
bytes, and any payload whose strict re-encoding differs.

The text form begins every record with `tick`; subsequent source lines preserve
accepted-action order until the next `tick`. Its commands are `move`, `look`,
`wheel`, `delete`, `place`, `exact`, `fit`, `sidebar`, `resize`, and
`clear-motion`. The `exact` command is explicitly a scripted Edit-channel lane;
no current winit mouse or keyboard adapter emits it. F4/F5, presentation,
topology, capture, and storage operations have no tape opcode.

The closed codec caps are 16 MiB canonical payload, 16,384 records, 65,536
actions per record, and 262,144 total actions. The CLI also bounds source reads
to 16 MiB before UTF-8 decoding, so an oversized source is not first loaded
without limit. Text parsing uses explicit fallible geometric capacity growth
rather than requesting an exact one-element extension for every push. A
maximum-sized action vector therefore has 17 bounded growth steps and the
record vector has 15; a powered schedule test freezes both counts.

The independent Rust/Python canonical vector fixes a 93-byte payload and tag:

```text
#bb.intent.sha256.zhqcq35tj65fukzzkpix6lo45nxsyfrqhzlun2ktpqsegsthiuwa
```

The Python oracle rejects 15/15 malformed controls spanning header/count,
ordinal, opcode, control, Boolean, float, nested Edit, truncation, and trailing
byte faults. An initially debug-only command-token consumption accidentally
disappeared in optimized builds because it lived inside `debug_assert_eq!`.
The parser now consumes the token unconditionally and only asserts its value;
the release parser and release headless CLI are separately exercised.

### Shared reducer and headless execution boundary

Live winit adapters and the tape call one `App::apply_accepted_action` reducer.
Every tape record then calls the production `fixed_tick` exactly once. The
headless constructor pins the sidebar predecessor to `(none, 0, AuditDisabled)`
and never consults `BITBLOCK_DISPLAY_AUDIT`; this removes machine/platform audit
state from composite and TickOutcome tags. A child-process control runs the same
tape with the environment absent and set to `1` and requires byte-identical
stdout.

The accepted-action boundary deliberately excludes device/window identity,
focus and cursor-grab success, synthetic/repeat filtering, pixel-to-line wheel
conversion, raw timestamps, click capture routing, redraw scheduling, and
display evidence requests. It therefore simulates the sequence *after* those
platform decisions, not the physical peripherals. A powered subject control
temporarily mapped physical `W` to backward while the normalized tape retained
forward; the adapter-equivalence test failed with divergent TickOutcome tags.
Restoring the forward mapping returned it to green.

Finite look lanes can still overflow when several individually finite deltas
are accumulated. Both preflight and the shared reducer now use the same checked
finite accumulator; a same-record pair of maximum finite deltas is refused with
its tick/action address before `fixed_tick` can panic.

The ordinary report exposes the tape tag, each tick's outcome/world/history/camera
and optional game/composite CPU-frame commitments, then final world/history/camera
state. It explicitly reports zero game/composite admissions, empty frame-receipt
heads, and unavailable display evidence. In the ordinary build, the first line
labels these values as `scope=fixed-tick-projection` and `run_receipt=unavailable`:
stdout juxtaposition alone is not a canonical carrier binding tape, genesis,
rules/build/policy, and result chain. TickOutcome also retains accumulated non-edit
controls rather than the complete ordered non-edit action stream.

### Local replay evidence v1

`src/replay_evidence.rs` adds an opt-in research carrier under the new readable
`replay-evidence` tag kind. `cargo run --release --features "headless-console
protocol-research" -- --simulate-actions <tape>` samples bytes from the current executable
path before the fold, executes the same bounded headless simulation, structurally verifies
the carrier, and prints its outer tag plus selected component tags. The carrier remains in memory;
the line declares `carrier=in-memory-only` rather than pretending the tag itself is a
durable object.

The schema is `bitblock/local-replay-evidence/v1`. Its exact byte length is
`468 + N`, where `N` is the embedded canonical InteractionTape payload length. In order
it contains an eight-byte executable length plus SHA-256 digest; three policy digests
for fixed headless genesis, rules, and admission; an eight-byte tape length and all `N`
tape bytes; an optional final TickOutcome tag; final World and coordinate-history Receipt tags;
ten exact camera `u64` lanes; simulation/output dimensions; sidebar Boolean; and optional
latest-produced game/composite `(FrameInput tag, Frame tag)` pairs. Empty tapes require all three
optional endpoint fields absent; nonempty tapes require all three present. The decoder
rejects caps, every proper prefix, a suffix, noncanonical option padding, invalid Boolean
values, bad dimensions, and invalid embedded tape bytes before admitting a carrier.

The executable tag names only the length and bytes read through the local process's pre-fold
executable path. The check rejects a type or length change across the sample, but does not rule
out a same-length concurrent replacement. It is not a source, compiler, operating-system,
reproducible-build, custody, or executing-image identity. The three policy tags bind declarations used by the local verifier; they
do not turn their host-local resource limits into protocol authority. The CLI constructs and
structurally verifies only its fresh in-memory carrier; it has no supplied-carrier replay CLI or
persistence format yet. A test-only local verifier first checks a carrier, recomputes its current
local build/policies, then reruns the embedded tape and requires complete endpoint/carrier equality.
Powered controls reject a changed build digest before replay and a structurally valid carrier whose
endpoint belongs to another tape.

An independent Python fixed-vector encoder/decoder has the fixed 506-byte vector:

```text
#bb.replay-evidence.sha256.r7ukwnl52rten5bwquqztobeuhfn362ywca7wrcudy5mvpache4q
```

The Rust codec round-trips the same carrier and checks that every decodable single-byte mutation
changes its outer tag. The test-only semantic re-run shares the Rust simulation implementation
with carrier construction, so it establishes local self-consistency rather than an independent
replay implementation or authority. Persistence, cross-host float agreement, signature, ownership, consensus,
Proof-of-History, Proof-of-Stake, Layer 1, elapsed time, window submission, display, scanout, and
physical light all remain outside the carrier.

The local replay-evidence checkpoint passed `cargo fmt --all -- --check`, 326 default
all-target tests (284 library, 41 application, one integration), 355 all-feature
all-target tests (304 library, 50 application, one integration), and strict default/all-feature
Clippy. All 17 independent Python vector programs passed; the exact 1,112-definition metric
registry audit passed; and `lake build` completed six proof jobs. The release headless
place/delete tape replay ended with `on_bits=1` and `coordinate_receipts=2`; with the research
feature it additionally constructed one 519-byte in-memory local carrier. These are local test
and execution observations, not persistence, custody, or cross-host evidence.

No backend call, OS raster observation, compositor attribution, scanout witness,
or physical-light measurement occurs. CPU framebuffer tags name produced
headless frames only. Exact host-`f64` bits are retained, but bitwise result
agreement across architectures, compilers, or floating-point environments has
not been established.

On Windows, the ordinary optimized observer remains a GUI-subsystem executable.
Automation that needs captured optimized stdout uses:

```powershell
cargo run --release --features headless-console -- --simulate-actions examples/observer_walk.tape
```

### Pre-mutation resource admission

The entire tape is interpreted by a pure shadow before `App` construction. It
tracks movement edges, look/wheel accumulation, clear-motion, dirty game/sidebar
lanes, dimensions, possible/definite exact coordinates, exact sparse chunks,
and monotone unresolved aimed placements. The possible/definite abstraction is
required because an exact On/Toggle can be rejected when it would enclose the
observer; it never lets possible population fall below a real continuation.
Exact Off/Toggle also charges the current possible world before changing the
abstract state, covering `World::recompute_bounds` amplification.

The local fail-closed policy is:

```text
peak output pixels                 16,777,216
cumulative resize pixel writes   268,435,456
cumulative output pixel ticks    268,435,456
cumulative raster sample bound 1,073,741,824  (12 cube triangles per possible bit)
cumulative world coordinates     268,435,456
cumulative edit coordinates      268,435,456
cumulative sparse chunk words  1,073,741,824  (64 u64 words per possible chunk/scan)
cumulative world ordering units 1,073,741,824  (N * ceil(log2 N) per world tag)
peak possible sparse chunks           65,536
```

Every nonzero resize is charged even if a later action shrinks before the tick.
Only dirty lanes are charged for raster/composition, so the full 16,384-record
idle-tape allowance remains admissible. World scans use record-peak possible
population and possible chunk count; world tags additionally charge a
conservative ordering factor. Controls refuse repeated large resize, raster,
aim/world-scan, bounds-recompute, repeated canonical-sort, and widely sparse
chunk-word amplification before allocating the requested runtime state. These
are untagged host-local policy limits, not consensus or Layer-1 authority.
A direct boundary control supplies 65,536 unresolved placements: permanent
bedrock makes the possible chunk count 65,537, and preflight refuses it against
the 65,536 cap.

### Example projection and metric remint

The current live-input-reachable `examples/observer_walk.tape` has five records
and seven actions. A release headless run reported:

```text
tape             #bb.intent.sha256.kq7nusohkdquulz4s3rvc3sopcpoewrdxtozzjkyoe2z7hlts7yq
final world      #bb.world.sha256.vy2bmytgqk6a2gx2f33ne4r2zlty53dcg543xbcfewszp5lcmuwa
final history    #bb.receipt.sha256.pjchsuc7isnai5ewfolrps763l235uo5y2jbdgo6ckpxi7y3yztq
on bits          2
history receipts 1
game admissions  0
composite admits 0
receipt heads    none / none
display evidence unavailable without window
```

Thirteen existing definitions advanced; no sidebar-selected definition changed,
so sidebar/composite policy tags did not remint:

```text
parse outcome                            #bb.metric-definition.sha256.fbspkmipmdtshettyqhufncsrcosr6pevkdtuggaaxutqo7lnxhq
rejected field payload                   #bb.metric-definition.sha256.pmcwz2dihdfybefqjtw376hgm4ocj5wkur3r3to3c5tqsafr4vaa
parse/display round trip                 #bb.metric-definition.sha256.sc5b2rs5dmlpaccdnujkrws6gbf7n6dpl5uzegzkbsjuyifhazra
canonical replay bytes                   #bb.metric-definition.sha256.e3ugirbax7epzvwgqiekws23w73lowmoezfshl3sn4lrnuu6bhiq
domain-separation identity               #bb.metric-definition.sha256.b2uxj4hkepxslna7wlktcoe4thlszcxfqs43ze6judtsawi2ym6q
hash input bytes                         #bb.metric-definition.sha256.ngktv3ej6seazabywg7rd637ynpwhjzsbiq7nzgp6c2fu3g6gnga
encoding round-trip check                #bb.metric-definition.sha256.2npmiwjtmvulaeodwxgpk4wx2pzpzvvlbbnd4jjn7iozkwnzsv2a
final framebuffer digest                 #bb.metric-definition.sha256.xrpzaixabwbivwbv2njwcc6br5pg6y7ugooqzfntp5rvlsiucsga
known-answer vector verdict              #bb.metric-definition.sha256.hgtisylvm66vgwhxo76austomilhsuntnigkrljhxqnok4tgpapa
cross-implementation digest parity       #bb.metric-definition.sha256.e3twhjvyxiudc4mdldvvxkfh3ljog3gcltfarzxw7pknpnseisoa
deterministic resource admission         #bb.metric-definition.sha256.tolccwjudoe3vtikeycfrvgqzi4vrpe3x6iaj3civwj3qk7xlejq
produced-frame disposition               #bb.metric-definition.sha256.wu26gvn6psqy6kha322wdl7z6md3undvaj5fh5pk25nvixtgrcgq
interaction-to-display causal path       #bb.metric-definition.sha256.iz2hk6s5zejuemox72rs4l4dp4gup5az53e4i2le232qpm2un6nq
```

The subsequent local replay-evidence v1 checkpoint advances eight existing
definitions and adds no metric family:

```text
canonical replay bytes                   #bb.metric-definition.sha256.lqtx75tyv3ro3ifmcb5furmfelm7t53admqbvqrnjort2sfs66hq
domain-separation identity               #bb.metric-definition.sha256.4lv3fi6naepsomnyezxqqeoeormhdce6chc7eb4u5tarteldmz2q
hash input bytes                         #bb.metric-definition.sha256.ktd33zqvdnbj2rd43bxmvvjw6bygul52dlp43up47kt327jxd7uq
encoding round-trip check                #bb.metric-definition.sha256.gduldun3htcmfiy2ymxah2uqi5ph2gbcvlmrzmvo6uozans4disa
known-answer vector verdict              #bb.metric-definition.sha256.im56mg23da4rwnd2j5fmhukvppvdofh7yq4vaicrvubhb2ekcv6a
cross-implementation digest parity       #bb.metric-definition.sha256.vpfuuupwg73zrzzup2zvqhjf652k2l5imxoy5j2cm2ntczse4oza
deterministic resource admission         #bb.metric-definition.sha256.rrfid54dw5qpb5pe7fliu5wp6yzmn2podbjzdopbd4kzgyvf6lfa
interaction-to-display causal path       #bb.metric-definition.sha256.m4h4uqbedh3w76nxvyccjrras5pxgarftfvfhkjvv57dmhww3una
```

### Tag-bound sidebar-only compositor reuse

The compositor now separates a full composition from a sidebar-only
recomposition. A full composition copies the complete inner game prefix, then
records the already-observed inner game-frame tag only after sidebar raster
completion. A sidebar-only composition requires that exact cached tag; a stale
selected tag, the absence of a completed full composition, or any resize is a
refusal rather than permission to retain an arbitrary buffer prefix.

At the default 801 x 801 simulation viewport, each skipped F3-only game-prefix
copy is exactly 641,601 `u32` words: 2,566,404 payload bytes read and the same
number written, or 5,132,808 logical source-plus-destination byte events. The
new `#bb.metric.perf.work-unit-vector` definition is
`#bb.metric-definition.sha256.quistqq47z3lx57jlupj6rgtjcby2otxjkdmjzvgd3uykbi5imda`.
It counts logical work in the compositor; it is not a measurement of hardware
memory traffic, elapsed time, energy, scanout, or physical display output.

The complete 1,201 x 801 composite remains hashed and later copied for
presentation, so this does not claim a full-render or executable-byte
reduction. The candidate/reference mismatch definition advanced to
`#bb.metric-definition.sha256.sa4ylbjoqg5setnvlqk2qhmxofagznkqtqlbey5chagbjfsg2yfa`:
the powered assay compares every XRGB word, sidebar facts, and `FrameRef`
observation from a full-then-same-game sidebar-only sequence against the frozen
full-copy reference. Separate controls reject reuse before a full composition,
for a distinct same-shape game tag, and after resize. The live F3 reducer test
also checks that its game prefix remains word-identical to the renderer while
the copy-work counter is zero.

The existing five-tick, seven-action `observer_walk.tape` traverses that F3
case headlessly. Two release replays produced byte-identical eight-line
projections with tape tag
`#bb.intent.sha256.kq7nusohkdquulz4s3rvc3sopcpoewrdxtozzjkyoe2z7hlts7yq`.
This establishes repeatability of the accepted-action-to-fixed-tick projection
on that build and host; it does not simulate raw peripherals, a window,
backend submission, or display evidence.

### Read-only retained display-journal verification

The Windows research CLI now accepts:

```powershell
cargo run --release --features protocol-research,headless-console -- --verify-display-journal <display-session.bbj> '<#bb.storage-record.sha256...>'
```

It is intentionally a separate startup mode: it never opens an observer
window, starts no simulation tick, submits no frame, requests no desktop capture,
and opens no writable journal handle. The full lower-case canonical tag is parsed
as a `storage-record` anchor, not a shortened hash or a readable alias. The
regular input file is bounded by `StorageLimits`, has its length observed before
and after the read, and is typed-replayed from supplied bytes.

For a clean carrier, the verifier checks the caller-supplied anchor in the
storage-parent chain, reconstructs every admitted `DisplayCaptureArtifact` or
`PresentationOutputArtifact`, and emits its exact receipt, frame, policy,
timing/topology, comparison-outcome, mismatch-count, and retained-head tags.
Its printed `TickOutcome` and `FrameInput` values are references carried by the
display artifact; this mode does not receive an InteractionTape, replay world
history, or re-run the renderer.
For a damaged carrier, it returns nonzero and reports only the typed valid
prefix; the rejected suffix is explicitly excluded. A missing anchor also
returns nonzero. It neither repairs nor truncates a file.

This is a local typed-byte replay, not a current-branch or custody proof. A clean
result says nothing about an unseen later prefix, external replication,
independent DWM attribution, scanout, or physical light. It adds no metric
family: it is an additional bounded readout for existing compositor-capture,
pipeline-state, and presentation-fate definitions.

The retained local F5 artifact
`target/live-f5-assay-20260826-02.bbj`, anchored at
`#bb.storage-record.sha256.6zf3y3jxcwelftieje7toltjpeyv4kxpmgpb6svmvxcnlfcbqa2q`,
was read in this mode as one clean 3,850,988-byte typed record. Its stored
observation reports `capture_outcome=Match`, `capture_stage=Complete`, zero
mismatches, and an unchanged before/after route. Those facts remain one local
OS desktop-raster sample, not scanout or light evidence.

The current verifier checkpoint passed `cargo fmt --all`, 316 default
all-target tests (275 library, 40 application, one integration), 344 all-feature
all-target tests (295 library, 48 application, one integration), and strict
default/all-feature Clippy. All sixteen independent Python vector programs
passed; the metric registry audit remained 1,112 exact definitions; and
`lake build` completed six proof jobs. The release Windows research CLI also
cleanly replayed the retained F5 carrier above, while a different syntactically
valid storage-record tag emitted `status=missing-required-ancestor` and exited
nonzero.

### Final verification and release artifacts

The final all-target default pass runs 314 tests (273 library, 40 application,
and one direct CLI integration test). The all-feature pass runs 341 tests (293
library, 47 application, and the same integration test). Both feature sets pass
strict Clippy; formatting is clean. All sixteen independent Python vector
programs pass, including 15/15 malformed InteractionTape controls. The exact
metric audit remains 1,112 definitions with no unresolved or malformed rows,
and `lake build` completes all 6 proof jobs.

Optimized InteractionTape parsing passes all 12 release interaction tests, and
the optimized tape-to-fixed-tick equivalence test passes separately. The direct
release CLI integration test also passes. Running the retained headless release
with `BITBLOCK_DISPLAY_AUDIT` absent and then set to `1` produces byte-identical
captured text: 8 lines and 4,557 UTF-8 bytes under the PowerShell comparison,
with SHA-256
`84276214A256D808D046C8EB8D31D72288533266B87BBC6F34C1C32964AEB9EB`.
That digest is a test-observation of captured report text, not the canonical
local replay-evidence carrier or a durable/authoritative run receipt.

The retained same-source Windows release artifacts are:

```text
default observer
  target/interaction-tape-default/release/bitblock.exe
  1,111,552 bytes
  SHA-256 C845E6A130AA6B9722E4FF4D38274D36FE24080EDCE4BA5E2EC4A9948B3CCE87

protocol-research observer
  target/interaction-tape-research/release/bitblock.exe
  1,357,824 bytes
  SHA-256 95662D2465617E7738612FE5A4D4139FD74A01E165D3C3278DA85D69C1950150

headless-console observer/simulator
  target/interaction-tape-headless-console/release/bitblock.exe
  1,111,552 bytes
  SHA-256 B49C5390E2C330BC3799F6E3DE69372C530C48AA4F0814DDFEE5059E446C6EEF
```

Equal file length between the default and headless-console executables is not
binary equality; their hashes differ because the latter retains a Windows
console subsystem for captured optimized reports. File sizes and hashes do not
measure runtime speed, memory, or energy.
