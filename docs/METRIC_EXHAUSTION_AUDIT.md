# Metric-exhaustion audit — 2026-08-26

This checkpoint applied the uniqueness rule in [`METRICS.md`](../METRICS.md)
again after the live TickOutcome/FrameReceipt-v2, typed DisplayReceipt-v2,
bounded independent RGB/depth renderer oracle, explicit one-shot live
display artifact, checked Win32 submission, canonical active-output topology,
PresentationOutput wrapper, mixed display journal, and F5 service checkpoints. It admitted
**zero new metric families**. The authoritative catalog
therefore remains 1,112 families: 489 system, 339 view, and 284 world.

This is a bounded closure result. It covers the current Rust source plus the
declared future language, proof, storage, synchronization, Proof-of-History,
Proof-of-Stake, Layer-1, renderer, scanout, and physical-display surfaces already
named by the catalog. A genuinely new subsystem or observable can reopen the
audit. Unit changes, statistical summaries, labels, carrier versions, and new
collectors for an existing observable cannot.

## Checks

- Every `src/*.rs` state field, event, branch, error, counter, receipt field, and
  protocol boundary was compared against the three owning catalogs.
- The renderer-to-physical-display path was traced separately through the CPU
  framebuffer, composite buffer, mapped surface, backend submission, Windows
  screen raster, bounded source/target route topology, compositor/output state,
  scanout fate, and external sensor.
- Every candidate was compared to the closest admitted family. A candidate was
  rejected when it was derivable from that family, merely another closed label
  or carrier/schema version, or a missing collector for an already named value.
- `python tools/metric_registry.py audit` reported 1,112 records, 1,112 readable
  names, 1,112 unique definition tags, zero legacy numeric identifiers, zero
  unresolved references, and exact manifest agreement.

## Pixel and presentation closure

| Observable boundary | Existing family or authority object | Remaining evidence gap |
|---|---|---|
| Complete post-overlay CPU XRGB slice and world-pass depth | `#bb.metric.render.final-framebuffer-storage-contract`, `#bb.metric.hash.final-framebuffer-digest`, `#bb.metric.visual.candidate-reference-framebuffer-mismatch-vector` | A stdlib-only Python six-face oracle independently matches every XRGB word and depth class in two named direct-cube frames, with retained finite-depth bit residuals bounded to four ULP. The 934-pixel corpus excludes aggregate, point, inside-bit, near-plane, arbitrary-orientation/FOV, and cross-platform cases; it is not universal equivalence. |
| Game/sidebar composition | `#bb.metric.visual.render-input-output-binding-verdict`, `#bb.metric.visual.candidate-reference-framebuffer-mismatch-vector` | A bounded two-arm harness distinguishes the strided production compositor from the frozen two-buffer storage reference, but both compositor arms still share sidebar painter semantics. The independent six-face oracle covers world rendering and the fixed crosshair, not sidebar glyph semantics. |
| Mapped backend surface before submission | `#bb.metric.visual.mapped-surface-copy-mismatch-vector` | Windows now owns the complete copy target, but mismatch witnesses and non-Windows internals remain separate instrumentation; the copy says nothing about later compositor behavior. |
| Backend submission attempt/result | `#bb.metric.render.backend-submission-event` | Windows now checks the exact requested/returned `SetDIBitsToDevice` scanline count and error boundary before receipt publication. A durable event relation and other platform internals remain unimplemented. |
| OS-exposed client raster | `#bb.metric.visual.compositor-capture-comparison` | F4 validates and samples the current committed composite occurrence. F5 waits for the next normal successful composite presentation, binds its exact timing receipt, records pre-topology, captures once, records post-topology, and persists one outer wrapper. Neither path ran a manual HWND assay in this checkpoint, and any finite sample remains OS desktop-raster evidence rather than guaranteed scanout. |
| DPI/ICC/HDR/output configuration and transforms | `#bb.metric.visual.display-pipeline-state-vector`, `#bb.metric.visual.display-transform-provenance-relation` | The research timing envelope retains raw DWM session timing and barriers. A bounded Windows query now retains current GDI source/monitor geometry, source mode, every clone target route, target identity/signal/path facts, raw flags, and the acquisition attempt transcript or explicit unavailability. ICC/HDR configuration, output bit depth, and applied transform order remain unavailable; configured route fields would not prove the transform actually emitted. |
| Drop/duplicate/partial/mixed-frame scanout | `#bb.metric.visual.presentation-fate-relation`, `#bb.metric.visual.scanout-composition-witness-vector` | The outer object now binds one exact composite receipt, null-HWND DWM timing, one screen raster, and before/after topology. Full receipt and QPC-frequency equality prevent cross-occurrence substitution, but session-scoped timing and untimestamped topology cannot attribute compositor movement to the frame. Fate remains `Unknown`; independent compositor attribution or high-speed capture is still required. |
| Panel RGB/luminance or emitted light | `#bb.metric.visual.physical-display-sensor-comparison` | `N/A` without a calibrated external instrument. |
| Durable availability of referenced expected/captured pixels | `#bb.metric.store.receipt-object-persistence-inventory`, `#bb.metric.hash.content-reference-closure` | Exact mixed replay retains legacy or wrapper carrier bytes, the nested display receipt and pixels, and wrapper timing/topology tags through one physical and one display-parent chain. Referenced policy, TickOutcome, and FrameInput objects are not supplied. No manual live sample or sudden-power assay was run here; transitive closure, external custody, and latest-prefix authority remain open. |

The tempting name `literal-displayed-pixels` was rejected because it conflates
three distinct observations: an OS screen raster, scanout composition, and
physical panel output. Those boundaries must remain separate.

## Rejected source-shaped candidates

| Candidate shape | Why it is not a new family |
|---|---|
| FrameReceipt-v2 digest per stream | The stream is a closed label inside `#bb.metric.hash.frame-provenance-receipt-digest`. |
| Successor-path length, skipped outcomes, or skipped attempts | Scalar projections of `#bb.metric.replay.receipt-tick-gap-evidence-relation`; verification cost is a view of the work vector. |
| Paired-publication atomicity counter | The prepare/reserve/backend/publication outcome is already part of frame provenance plus `#bb.metric.render.backend-submission-event`. |
| Frame/display ledger head or length per epoch | Head and length are projections of the ordered receipt relation; schema version is provenance, not a new observation. |
| DisplayReceipt-v2 digest as a standalone metric | It is the content-addressed carrier for compositor comparison and presentation fate. |
| OutputTopology or PresentationOutput digest as a standalone metric | Each is a content-addressed carrier for already named pipeline, capture, provenance, and recovery observables. |
| Active route count, topology retry count/rate, or last topology error | Counts, rates, extrema, and labels are projections of the complete `#bb.metric.visual.display-pipeline-state-vector` acquisition/route observation. |
| Stable/changed/unavailable route result | Derived comparison of two pipeline-state vectors; it is intentionally not encoded as an independent observation. |
| Before/after topology pair | Two pipeline observations inside the existing compositor-capture comparison and presentation-fate relation, not a new semantic dimension. |
| Display mismatch count, first index, bounds, rate, stage, or capture duration | Counts, extrema, labels, bounds, and timestamp arithmetic are views of the complete compositor comparison. |
| Frame RGB means, ratios, popcount, or nonblack ratio | Deterministic summaries of the exact final palette/pixel distribution; invalid high bytes already have their own witness vector. |
| One metric per protocol/storage/display error variant | Error kind is a dimension of the relevant admission or failure relation. |
| One metric per optimization mismatch field | A reason label inside candidate/reference equivalence and framebuffer mismatch. |
| World bounds volume under another integer width | Derived from coordinate bounds/axis extents; inability to represent it is already the bounds-volume guard. |
| Sidebar current-frame display proof | Causally cyclic. The predecessor-only rule is already measured by `#bb.metric.visual.frame-provenance-dependency-cycle-vector`. |

## Stopping result

The catalog is exhausted for this reviewed scope when all three statements hold:

1. the exact registry audit is green;
2. independent source and pixel-boundary reviews produce no irreducible
   observable absent from the registry; and
3. every rejected candidate has a named owner or a stated invalidity mechanism.

All three hold at this checkpoint. Missing implementation, collection,
durability, independent verification, or external instrumentation remains real
work, but does not authorize inventing another name for an existing metric.
