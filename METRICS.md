# Bitblock metric registry

This is the canonical catalog of measurements Bitblock could expose. It is a
catalog checkpoint, not a promise to collect every value on every tick. No
entity metrics are included because Bitblock has no entities.

The catalog is deliberately much larger than the future sidebar. The sidebar
should be a read-only projection over this registry; a measurement must never
feed back into world transitions unless it becomes an explicit, replayable
input.

## Catalog sections

- [World, shape, information, emergence, and scale](docs/metrics/world.md)
- [Observer, view, rendering, input, collision, and sidebar](docs/metrics/view.md)
- [Tick, language, replay, proof, hashing, runtime, and storage](docs/metrics/system.md)

Each metric has one owning section and one readable `#bb.metric.*` registry
name. A cross-reference is not a second metric. Names are resolved by
[`docs/metrics/registry.tsv`](docs/metrics/registry.tsv) to full immutable
definition tags; the readable name is not authority by itself.

This checkpoint records **1,112 unique metric families**: 284 world/shape/
emergence metrics, 339 observer/view metrics, and 489 system/proof/runtime
metrics.

The closure pass examined all candidate definitions against the rule below. It
retired 52 alias/view-shaped rows, replaced them with two parameterized source
families where necessary, and therefore reduced the catalog by 50 net rows.
The last removed row was `hit present`, which became exactly the rank-1
presence view of the generalized ray-hit-record family. No high-confidence
missing family remained in the reviewed Bitblock world, view, language,
runtime, proof, storage, display, or declared-future protocol surfaces.

The post-FrameReceipt-v2 source and literal-pixel rerun reached the same zero-
addition result. Its boundary crosswalk and rejected-candidate ledger are
retained in
[`docs/METRIC_EXHAUSTION_AUDIT.md`](docs/METRIC_EXHAUSTION_AUDIT.md).

## What counts as a unique metric

A metric is unique when it measures a different underlying property. These are
display or analysis views of one metric, not new metrics:

- another unit, reciprocal, percentage, logarithm, or normalization;
- current, minimum, maximum, mean, median, EWMA, quantile, histogram, or rate;
- the same statistic over a different rolling window;
- the same parameterized measurement at another radius, offset, scale, order,
  adjacency, direction, region, or threshold.

Parameterized measurements are therefore recorded as families. For example,
`mutual information at displacement d` is one family even though there is one
value for every possible `d`. This is the stopping rule that prevents an
infinite list of aliases while retaining every genuinely distinct idea found
in this checkpoint.

## Common labels

The section files use compact labels with these meanings:

- **Simulation**: determined by canonical initial state and ordered exact edit
  history.
- **View/runtime**: exact state of this running observer or renderer, but not in
  the current replay/proof surface.
- **Host**: wall-clock, operating-system, allocator, or hardware telemetry;
  inherently machine-dependent.
- **Audit**: a test, reconciliation, proof, or cross-executor result.
- **Future**: the named subsystem or semantics do not exist yet.
- **Exact**: exact under the stated finite representation and convention.
- **Float-derived**: deterministic from stored floating-point values, but not a
  proof over real-number geometry.
- **Estimated**: sampled, fitted, heuristic, model-dependent, or numerical to a
  stated tolerance.
- **Derived now**: computable from current state without retaining new history.
- **Instrumented**: requires counters, timestamps, richer diagnostic records,
  or a new buffer.
- **History**: requires a retained event log or snapshot series.

`N` means on bits, `C` allocated chunks, `P` simulation-viewport pixels, `E`
resolved edits, `V` cells in a declared finite region of interest, `T` retained
ticks, and `K` analysis scales or samples where appropriate. Costs are rough
orders of growth, not performance promises.

## Interpretation guards

1. **The intended space and executable space differ.** The Lean model uses
   unbounded integer coordinates. The Rust executable currently accepts
   `i64^3`, exactly `2^192` addresses. Values must say which domain they mean.
2. **There is no useful finite global off-bit count in the intended unbounded
   lattice.** Density, occupancy entropy, porosity, and off counts require a
   named finite region or an explicit limiting-window convention. In the Rust
   address domain, `2^192 - N` is definable but usually not an informative live
   metric.
3. **A finite build is not a proven fractal.** Every finite occupied set has
   asymptotic box-counting dimension zero. Bitblock may report finite-range
   scaling estimates only with the scale interval, number of octaves, fit
   residual, uncertainty, and sensitivity to finite size and grid alignment.
4. **There is no universal emergence or complexity score.** The registry keeps
   distinct definitions distinct and records their assumptions. Geometry alone
   does not create semantics.
5. **A hash commits to bytes; it does not prove the world.** State roots,
   receipts, proofs of transition, proof-system soundness, and external custody
   are separate claims.
6. **Unavailable is not zero.** A metric whose history, rule, buffer, or future
   subsystem does not exist must display `N/A` with the missing prerequisite.
7. **Overflow and precision are results, not excuses.** Coordinate spans and
   volumes need widened or arbitrary-precision arithmetic. Camera/render
   geometry must expose the `f64` exact-integer horizon instead of claiming
   exact geometry over all `i64^3`.
8. **The raw curve outranks a fitted summary.** Scaling exponents, correlation
   lengths, power laws, and uncertainty-bearing estimates retain their source
   curve, parameter range, estimator, and error.

## Collection tiers

The cheap-world goal requires collection to be staged:

1. **Hot projections** — bounded `O(1)` values already in state. Safe to refresh
   every tick or sidebar repaint.
2. **Event counters** — `O(1)` updates only when their event occurs. Disabled
   counters report `N/A`, never zero.
3. **Snapshot scans** — `O(N)`, `O(C)`, or `O(P)` exact analyses. Run on demand
   or at a deliberately lower cadence and show their source tick.
4. **Deep analyses** — graph, homology, compression, spectra, all-pairs, fitting,
   and multiscale work. Run against an immutable snapshot in a cancellable
   worker, never inside the fixed tick.
5. **History analyses** — require an explicit bounded ring buffer or append-only
   receipt/event log with a declared memory budget.
6. **Host diagnostics** — collected separately from deterministic simulation
   data and never admitted into a deterministic receipt as authority.

Every displayed non-hot value should carry its source tick and freshness. Every
estimated value should also carry method, parameters, sample size, and error.

## Sidebar projection rule

The first sidebar should select a small hot subset rather than render this whole
registry: observer pose/cell, origin relation, aim target and rejection reason,
zoom/saturation state, world count/bounds/chunks, tick/edit batch, last
collision, and invariant alerts. Pages or filters can expose the rest later.
The crosshair remains centered in the simulation viewport, not in the combined
simulation-plus-sidebar window.

## Adding another metric

Before adding a registry name, check whether it is a unit, rollup,
normalization, window, parameter value, or alias of an existing family. If it
is genuinely new, add its definition, prerequisites, exactness, collection
cost, and owning section, then regenerate the definition-tag registry. If its
semantics are not yet defined, record the open definition rather than a
plausible-looking number.
