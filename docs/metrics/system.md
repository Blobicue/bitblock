# Bitblock system metric catalog

This catalog owns the machinery around Bitblock's world and view: fixed-step
scheduling, the `.bb` language, tick batches and replay, determinism and proof,
hashes and receipts, sparse-container implementation costs, host telemetry,
persistence, future synchronization, and system health.

It deliberately does **not** redefine:

- world cardinality, bounds, shape, density, topology, or chunk occupancy as
  state; those belong to `world.md` and its `#bb.metric.world.*` metrics;
- observer/camera pose, aim, raw input, ray/collision results, projection,
  pixels, crosshair, or rendering results; those belong to `view.md` and its
  `#bb.metric.view.*` metrics.

This file may measure the implementation cost of producing a world or view
result, but the result itself remains owned by the other catalog.

## Metric contract

### Classes

| Code | Meaning |
|---|---|
| `D` | Deterministic projection of the canonical initial world and ordered exact tick-batch stream. |
| `R` | Exact logical/runtime state, but outside the current replay proof surface. |
| `H` | Nondeterministic host observation: clocks, scheduler, allocator, OS, hardware, disk, or network. |
| `A` | Artifact/audit result: test, proof, build, static analysis, or cross-executor assay. |

`D` does not mean formally proved. It means that the metric has no hidden host
input. `H` values must never influence a world transition unless they first
become explicit canonical inputs.

Composite labels such as `D/H` mean the row deliberately carries separate
deterministic and host components; they must remain separate fields. A table
qualifier such as `Now in tests` or `Future hash` narrows, but does not replace,
the leading availability code. Likewise, mixed exactness states identify which
collector supplies the exact versus sampled component.

### Availability and exactness

| Code | Meaning |
|---|---|
| `Now` | The value already exists in current semantics/state, though a public accessor may still be needed. |
| `Inst` | The subsystem exists, but retaining the value requires instrumentation. |
| `Future` | The subsystem itself is not implemented; report `N/A`, never zero. |
| `X` | Exact for the observed state, artifact, or run. Clock values are exact only to the clock's resolution. |
| `S` | Sampled, estimated, forecast, profiler-derived, or otherwise approximate. |

### Cost notation

`A_t` is attempted edit intents in tick `t`; `E_t` is resolved exact edits;
`L` is retained batches; `E` is retained edits; `B` is encoded/source bytes;
`N` and `C` are the on-bit and allocated-chunk values owned by `world.md`;
`R_c` is receipts; `P` is peers. Hash-map operations are expected `O(1)`
unless stated otherwise.

The cost column is the collection/evaluation cost, not the cost of the subject
operation. `+O(1)` means maintainable incrementally.

### Views are not new metrics

For any scalar or event vector, the sidebar/recorder may show current value,
total, delta, minimum, maximum, mean, EWMA, histogram, percentile, rate, or a
fixed tick/wall-time window. Unit conversion and those statistical/temporal
projections do not receive new metric definitions. A reason, operation, phase, peer, or
error-kind dimension is likewise one closed-vector metric, not one metric per
label.

Empty denominators display `N/A`. Ratios must state their numerator and
denominator rather than silently treating zero as success.

## Fixed-tick scheduling

| Registry name | Metric and exact definition | Class | Avail | Exact | Cost |
|---|---|---:|---:|---:|---:|
| `#bb.metric.tick.tick-index` | **Tick index**: the next fixed-tick number, equal to completed fixed ticks. | D | Now | X | `O(1)` |
| `#bb.metric.tick.configured-tick-rate` | **Configured tick rate**: the fixed-step frequency selected by this build, currently 60 Hz. | R | Now | X | `O(1)` |
| `#bb.metric.tick.nominal-tick-period` | **Nominal tick period**: the exact integer duration the scheduler subtracts per fixed step. | R | Now | X | `O(1)` |
| `#bb.metric.tick.batch-emission-event` | **Batch emission event**: emitted batch tick value paired with the fixed-tick index before increment. Retained-stream gaps belong to `#bb.metric.replay.tick-gap-vector`. | D | Inst | X | `+O(1)` |
| `#bb.metric.tick.raw-advance-gap` | **Raw advance gap**: host duration from the previous scheduler advance to the current one before clamping. | H | Now | X | clock read `O(1)` |
| `#bb.metric.tick.accepted-advance-gap` | **Accepted advance gap**: `min(raw_gap, MAX_FRAME_GAP)`. | H | Now | X | `O(1)` |
| `#bb.metric.tick.gap-clamp-loss-event` | **Gap-clamp loss event**: duration `max(raw_gap - MAX_FRAME_GAP, 0)` plus whether clamping occurred. | H | Inst | X | `+O(1)` |
| `#bb.metric.tick.accumulator-debt` | **Accumulator debt**: unconsumed accepted host duration currently held by the fixed-step accumulator. | H | Now | X | `O(1)` |
| `#bb.metric.tick.due-step-count` | **Due-step count**: `floor(accumulator / nominal_tick_period)` at scheduler entry. | H | Inst | X | `O(1)` |
| `#bb.metric.tick.steps-per-advance-event` | **Steps-per-advance event**: fixed ticks executed by one scheduler advance. | H | Inst | X | `+O(1)` |
| `#bb.metric.tick.catch-up-cap-event` | **Catch-up-cap event**: whether an advance reached `MAX_TICKS_PER_FRAME`, currently 15. | H | Inst | X | `+O(1)` |
| `#bb.metric.tick.discarded-debt-event` | **Discarded debt event**: accumulator duration and implied whole steps discarded after a catch-up-cap hit. | H | Inst | X | `+O(1)` |
| `#bb.metric.tick.scheduled-start-lateness` | **Scheduled-start lateness**: `max(actual_tick_start - scheduled_tick_start, 0)`. | H | Inst | X | clock read `O(1)` |
| `#bb.metric.tick.actual-inter-tick-interval` | **Actual inter-tick interval**: host duration between consecutive fixed-tick starts. | H | Inst | X | clock read `O(1)` |
| `#bb.metric.tick.tick-period-error` | **Tick-period error**: `actual_inter_tick_interval - nominal_tick_period`, preserving sign. | H | Inst | X | `O(1)` |
| `#bb.metric.tick.requested-wait` | **Requested wait**: scheduled wake deadline minus the host time when control returns to the event loop. | H | Inst | X | `O(1)` |
| `#bb.metric.tick.wake-error-event` | **Wake error event**: signed `actual_wake - requested_deadline`, distinguishing early wakes from overshoot. | H | Inst | X | clock read `O(1)` |
| `#bb.metric.tick.time-dilation-base-pair` | **Time-dilation base pair**: simulated duration advanced and host duration elapsed over the same observation interval. Their ratio is a view. | H | Inst | X | `+O(1)` |
| `#bb.metric.tick.scheduler-wake-event` | **Scheduler wake event**: every scheduling pass, labeled due, early, catch-up, capped, or stalled. | H | Inst | X | `+O(1)` |

## `.bb` source, parser, and static program

| Registry name | Metric and exact definition | Class | Avail | Exact | Cost |
|---|---|---:|---:|---:|---:|
| `#bb.metric.lang.source-bytes` | **Source bytes**: UTF-8 byte length of the supplied `.bb` text. | D | Now | X | `O(1)` after load |
| `#bb.metric.lang.source-scalar-count` | **Source scalar count**: Unicode scalar values in the source. | D | Inst | X | `O(B)` |
| `#bb.metric.lang.physical-line-count` | **Physical line count**: source line records before blank/comment filtering. | D | Inst | X | `O(B)` |
| `#bb.metric.lang.line-class-vector` | **Line-class vector**: instruction, blank, comment-only, and instruction-with-inline-comment line counts. | D | Inst | X | `O(B)` |
| `#bb.metric.lang.comment-bytes` | **Comment bytes**: bytes from each first `#` through that line's end. | D | Inst | X | `O(B)` |
| `#bb.metric.lang.pre-comment-token-count` | **Pre-comment token count**: whitespace-separated fields remaining before each first `#`. | D | Inst | X | `O(B)` |
| `#bb.metric.lang.line-ending-vector` | **Line-ending vector**: LF, CRLF, and unterminated final-line occurrences. | D | Inst | X | `O(B)` |
| `#bb.metric.lang.canonical-source-bytes` | **Canonical source bytes**: bytes in `Program::to_string()`. | D | Now | X | `O(E)` |
| `#bb.metric.lang.canonicalization-byte-delta` | **Canonicalization byte delta**: signed `source_bytes - canonical_source_bytes`. | D | Inst | X | `O(B+E)` |
| `#bb.metric.lang.noncanonical-numeric-lexeme-count` | **Noncanonical numeric lexeme count**: valid coordinate fields whose spelling differs from canonical decimal. | D | Inst | X | `O(B)` |
| `#bb.metric.lang.parse-outcome` | **Parse outcome**: declared grammar/schema plus accepted or first rejection with one-based line and a closed reason. Program reasons cover field count, operation, and coordinate lanes; InteractionTape v1 additionally covers action-before-tick, command, move control/state, finite float, resize integer, count/payload limits, and allocation refusal. | D | Now for Program and InteractionTape v1 | X | worst `O(B)` |
| `#bb.metric.lang.rejected-field-payload` | **Rejected field payload**: grammar/schema plus the found field count, command/control/state/operation token, invalid numeric token, or declared/observed limit payload attached to `#bb.metric.lang.parse-outcome`. | D | Now for Program and InteractionTape v1 | X | `O(1)` after failure |
| `#bb.metric.lang.all-diagnostics-vector` | **All-diagnostics vector**: every parse error by line/reason under a future nonexecuting lint pass. Current parser intentionally stops first. | D | Future | X | `O(B)` |
| `#bb.metric.lang.instruction-count` | **Instruction count**: `Program::len()`. | D | Now | X | `O(1)` |
| `#bb.metric.lang.operation-vector` | **Operation vector**: exact counts of `on`, `off`, and `toggle`. | D | Inst | X | `O(E)` |
| `#bb.metric.lang.distinct-named-coordinate-count` | **Distinct named-coordinate count**: cardinality of coordinates appearing in program instructions, not world occupancy. | D | Inst | X | expected `O(E)` |
| `#bb.metric.lang.coordinate-multiplicity-vector` | **Coordinate multiplicity vector**: repeated-reference count and maximum instructions naming one coordinate. | D | Inst | X | expected `O(E)` |
| `#bb.metric.lang.exact-duplicate-vector` | **Exact-duplicate vector**: adjacent and anywhere-repeated identical `(operation, coordinate)` instructions. | D | Inst | X | expected `O(E)` |
| `#bb.metric.lang.conflicted-coordinate-count` | **Conflicted-coordinate count**: named coordinates receiving more than one operation kind. | D | Inst | X | expected `O(E)` |
| `#bb.metric.lang.operation-transition-matrix` | **Operation transition matrix**: adjacent `op_i -> op_j` pair counts. | D | Inst | X | `O(E)` |
| `#bb.metric.lang.coordinate-reuse-distance-event` | **Coordinate reuse-distance event**: instructions since the same coordinate's preceding reference. | D | Inst | X | expected `O(E)` |
| `#bb.metric.lang.same-coordinate-run-event` | **Same-coordinate run event**: each maximal contiguous run's instruction count. | D | Inst | X | `O(E)` |
| `#bb.metric.lang.coordinate-token-sign-vector` | **Coordinate token sign vector**: negative, zero, and positive field counts per axis. | D | Inst | X | `O(E)` |
| `#bb.metric.lang.named-coordinate-extrema` | **Named-coordinate extrema**: min/max x, y, z and widened maximum absolute magnitude in the program, not world bounds. | D | Inst | X | `O(E)` |
| `#bb.metric.lang.integer-boundary-token-count` | **Integer-boundary token count**: fields equal to `i64::MIN` or `i64::MAX`. | D | Inst | X | `O(E)` |
| `#bb.metric.lang.canonical-decimal-payload` | **Canonical decimal payload**: total and maximum coordinate digit counts, including a sign byte when present. | D | Inst | X | `O(E)` |
| `#bb.metric.lang.named-chunk-coordinate-count` | **Named chunk-coordinate count**: distinct 16-cube addresses referenced by program coordinates, not allocated world chunks. | D | Inst | X | expected `O(E)` |
| `#bb.metric.lang.instruction-locality-base-events` | **Instruction locality base events**: widened Manhattan and Chebyshev displacement between adjacent named coordinates. | D | Inst | X | `O(E)` |
| `#bb.metric.lang.canonicalization-idempotence` | **Canonicalization idempotence**: canonicalizing already canonical source produces byte-identical source. | A | Inst | X | `O(E)` |
| `#bb.metric.lang.parse-display-round-trip` | **Parse/display round trip**: for each declared canonical source type, `parse(display(value)) == value`, including exact floating-point bit equality for InteractionTape v1. | A | Now for Program and InteractionTape v1 in tests | X | `O(E)` |
| `#bb.metric.lang.ladderlang-bridge-identity` | **LadderLang bridge identity**: full policy tag of the total `bb-lam/v1` mapping from one verified canonical Edit content-tag preimage into an expanded closed `Lam`; every other kind/schema is an explicit refusal. | D | Now library | X | `O(1)` after policy tag |
| `#bb.metric.lang.translated-term-node-vector` | **Translated term node vector**: variable, binder, application, and total node counts in the exact raw-octet `Lam` preorder named by `#bb.metric.lang.ladderlang-bridge-identity`. | D | Now bridge | X | `O(1)` closed form; `O(term_size)` stream |
| `#bb.metric.lang.translated-binder-depth` | **Translated binder depth**: LadderLang `Lam.binderDepth`, where a binder adds one and application takes the maximum. | D | Now bridge | X | `O(1)` closed form |
| `#bb.metric.lang.computed-ladder-layer` | **Computed Ladder layer**: lowest layer whose declared pool covers `#bb.metric.lang.translated-binder-depth`; for `poolFib`, `binderDepth <= fib(L+1)-1`. Exact only for the pinned pool/cut convention. | D | Now bridge | X | `O(1)` for frozen depth |
| `#bb.metric.lang.naive-abstraction-sk-census` | **Naive-abstraction SK census**: exact occurrence counts of `I`, `K`, `S`, application nodes, and surviving variables after the declared `transl`/`abstr` algorithm. | D | Now bridge formula/proof | X | `O(1)` closed form; `O(expanded_term)` independent check |
| `#bb.metric.lang.iota-leaf-cost` | **Iota leaf cost**: `iotaUpper = 2*I + 4*K + 5*S` from `#bb.metric.lang.naive-abstraction-sk-census`; the named tick projection is `iotaUpper - 1`. This is an encoding upper bound, not minimum size, wall time, renderer work, or runtime steps. | D | Now bridge | X | `O(1)` after census |
| `#bb.metric.lang.abstraction-closedness-verdict` | **Abstraction closedness verdict**: a declared closed source term produces zero surviving SK variables. | A | Now for bridge grammar | X | construction plus pinned parity assay |
| `#bb.metric.lang.coin-codec-verdict` | **Coin codec verdict**: for the declared closed SK corpus, `decodeCoin(toCoin(ski)) == some ski`, retaining the first counterexample. | A | Future | X | `O(expanded_term)` |
| `#bb.metric.lang.iota-cost-implementation-agreement` | **Iota-cost implementation agreement**: longest canonical-term prefix on which independent standalone Lean, pinned LadderLang `costChecked`, Rust, Python, and Node implementations agree on SK census, `iotaUpper`, and `iotaUpper-1`, plus first divergence. | A | Now for frozen bridge vectors/formula | X over declared vectors/theorem | `O(corpus * expanded_term)` |
| `#bb.metric.lang.bounded-reduction-outcome-event` | **Bounded reduction outcome event**: canonical start term, leftover term, frozen `(fuel, size_cap)`, and ternary `HOLD\|GREW\|RAN` result. | D | Future | X | reducer cost |
| `#bb.metric.lang.executed-reduction-step-count` | **Executed reduction-step count**: genuine beta steps performed before `#bb.metric.lang.bounded-reduction-outcome-event`; explicitly distinct from iota leaf/tick cost. | D | Future | X | `+O(1)` per step |
| `#bb.metric.lang.mint-admission-outcome-vector` | **Mint/admission outcome vector**: accepted or rejected translated terms by closed gate reason (ill-scoped, codec, cost bound, semantic proof, unsupported bridge form), retaining the exact failing obligation. | A | Future | X | gate-dependent |
| `#bb.metric.lang.bridge-semantic-preservation-outcome` | **Bridge semantic-preservation outcome**: for a canonical Bitblock program/batch and prestate, compare source poststate/effect trace with evaluation and decoding of its pinned LadderLang translation, retaining quantifier scope and first counterexample. | A | Future bridge | X over declared scope | translation plus both evaluators |
| `#bb.metric.lang.translation-provenance-relation` | **Translation provenance relation**: every Bitblock source instruction/span maps to emitted `Lam` nodes, derived SK nodes, and Coin subtree positions, or an explicit refusal. | D | Future bridge | X | `O(source + term)` |
| `#bb.metric.lang.interactive-session-revision-relation` | **Interactive-session revision relation**: every submitted code/command revision records parent revision tag, canonical source tag, base history head, parse/admission outcome, resulting receipt/head if executed, and active/superseded status. | D | Future interactive language | X | `+O(1)` per revision |
| `#bb.metric.lang.transaction-outcome-relation` | **Language transaction outcome relation**: program tag, pre-head, atomicity policy, attempted instruction prefix, first failure, committed receipt/head or rollback, and exact poststate verdict. | D/A | Future interactive language | X | execution plus verification |
| `#bb.metric.lang.program-observational-equivalence-outcome` | **Program observational-equivalence outcome**: candidate/reference Bitblock programs, declared prestate domain, observable set including post-world, ordered effect/provenance trace, outputs and receipt semantics, quantifier scope, verdict, and first counterexample. | A | Future optimizer/equivalence harness | X over declared scope | paired execution/proof |

## Pending edit queue and tick batches

The intent source and aim result are view-owned. This section records only the
queue/batch boundary and exact language transition after any view resolution.

| Registry name | Metric and exact definition | Class | Avail | Exact | Cost |
|---|---|---:|---:|---:|---:|
| `#bb.metric.batch.pending-edit-count` | **Pending edit count**: intents waiting to be consumed by the next fixed tick. | R | Now | X | `O(1)` |
| `#bb.metric.batch.pending-edit-capacity` | **Pending edit capacity**: vector slots currently reserved. | R | Inst | X | `O(1)` |
| `#bb.metric.batch.queue-drain-count` | **Queue drain count**: intents removed at tick start. Under current `mem::take`, this equals attempts. | R | Now | X | `O(1)` |
| `#bb.metric.batch.queue-residue` | **Queue residue**: pending intents remaining after batch construction. | R | Inst | X | `O(1)` |
| `#bb.metric.batch.intent-source-vector` | **Intent source vector**: exact ordered aimed-delete, aimed-place, or already-exact source for every intent consumed by one tick before disposition. The append-only in-memory `TickOutcome` v2 chain retains it for every completed tick. | R | Now retained outcome chain | X | `O(A_t)` |
| `#bb.metric.batch.queue-dwell-ticks` | **Queue dwell ticks**: consuming tick minus capture tick for each intent. | R | Inst | X | `+O(1)` per intent |
| `#bb.metric.batch.queue-dwell-host-duration` | **Queue dwell host duration**: consume timestamp minus capture timestamp. | H | Inst | X | clock reads `O(1)` |
| `#bb.metric.batch.attempt-count` | **Attempt count**: all intents consumed by the tick, including unresolved/rejected ones. | D | Now | X | `O(1)` |
| `#bb.metric.batch.resolved-edit-count` | **Resolved edit count**: exact executable edits retained in tick order. | D | Now | X | `O(1)` |
| `#bb.metric.batch.miss-count` | **Miss count**: `attempts - resolved_edits`; current exact aggregate. | D | Now | X | `O(1)` |
| `#bb.metric.batch.miss-reason-vector` | **Miss reason vector**: exact ordered unresolved-aim or observer-enclosure-rejection disposition for every unexecuted attempt. The append-only in-memory `TickOutcome` v2 chain retains this distinction; the legacy `TickBatch` projection does not. | R | Now retained outcome chain | X | `O(A_t)` |
| `#bb.metric.batch.resolved-operation-vector` | **Resolved operation vector**: on/off/toggle counts in exact execution order. | D | Inst | X | `O(E_t)` |
| `#bb.metric.batch.state-changing-edit-count` | **State-changing edit count**: resolved edits whose individual `apply` changed the world. | D | Inst | X | `O(E_t)` |
| `#bb.metric.batch.state-no-op-vector` | **State-no-op vector**: idempotent-on, idempotent-off, protected-bedrock, and other resolved no-op counts. | D | Inst | X | `O(E_t)` |
| `#bb.metric.batch.transition-direction-vector` | **Transition-direction vector**: off-to-on and on-to-off transitions caused by resolved edits. | D | Inst | X | `O(E_t)` |
| `#bb.metric.batch.toggle-direction-vector` | **Toggle direction vector**: toggle-to-on and toggle-to-off counts. | D | Inst | X | `O(E_t)` |
| `#bb.metric.batch.net-population-delta` | **Net population delta**: signed post-world cardinality minus pre-world cardinality, using the cardinalities owned by `world.md`. | D | Inst | X | `O(1)` from snapshots |
| `#bb.metric.batch.net-hamming-delta` | **Net Hamming delta**: coordinates whose final bit differs between the exact pre- and post-batch worlds. | D | Inst | X | expected `O(E_t)` |
| `#bb.metric.batch.transition-churn` | **Transition churn**: `state_changing_edits - net_Hamming_delta`; transitions canceled later in the same batch. | D | Inst | X | expected `O(E_t)` |
| `#bb.metric.batch.distinct-resolved-coordinate-count` | **Distinct resolved-coordinate count**: coordinates named by at least one resolved edit in the batch. | D | Inst | X | expected `O(E_t)` |
| `#bb.metric.batch.multi-edit-coordinate-vector` | **Multi-edit coordinate vector**: count of coordinates edited more than once and maximum batch-local multiplicity. | D | Inst | X | expected `O(E_t)` |
| `#bb.metric.batch.final-state-equivalent-edit-count` | **Final-state-equivalent edit count**: minimum one-edit-per-coordinate program reproducing only this batch's final world from its prestate. | D | Inst | X | expected `O(E_t)` |
| `#bb.metric.batch.final-state-dominated-edit-count` | **Final-state-dominated edit count**: `resolved_edits - final_state_equivalent_edits`; tick timing/intermediate states are intentionally ignored. | D | Inst | X | `O(1)` after `#bb.metric.batch.final-state-equivalent-edit-count` |
| `#bb.metric.batch.batch-canonical-bytes` | **Batch canonical bytes**: the complete `bitblock/tick-outcome/v2` payload binding parent/tick, prestate tags, exact controls/dimensions, ordered attempt dispositions, poststate/history/camera, and optional frame pairs. Every live fixed tick is retained in the in-memory outcome chain; the legacy `TickBatch` alone has no separate canonical codec. | D | Now retained outcome chain | X | `O(A_t)` |
| `#bb.metric.batch.edit-reversal-event` | **Edit reversal event**: an edit restores a coordinate to the state it held before an earlier edit in the declared batch/window. | D | Inst | X | expected `O(E_t)` |
| `#bb.metric.batch.batch-cardinality-invariant` | **Batch cardinality invariant**: `attempts >= resolved_edits` and `misses == attempts - resolved_edits`. | D | Now | X | `O(1)` |
| `#bb.metric.batch.application-boundary-outcome` | **Application-boundary outcome**: the parent-linked `TickOutcome` v2 admitted after one successful complete fixed fold, including exact attempt dispositions and resulting state/frame commitments. A panic or abort admits no outcome and still requires separate supervisor evidence. | D/H | Now for successful folds | X under declared codec; abort open | tagging plus supervisor/hook |
| `#bb.metric.batch.intent-resolution-occurrence-relation` | **Intent-resolution occurrence relation**: every captured intent/source occurrence maps to capture tick, exact resolved edit or closed rejection reason, event occurrence tag, and receipt/edit index when committed. | D | Future complete provenance | X | `O(attempts)` |

## Replay

| Registry name | Metric and exact definition | Class | Avail | Exact | Cost |
|---|---|---:|---:|---:|---:|
| `#bb.metric.replay.retained-batch-count` | **Retained batch count**: complete fixed-tick outcomes preserved in the append-only in-memory replay chain from tick zero. This includes idle and unpresented ticks and equals the retained outcome-vector length. | D | Now in-memory outcome chain | X | `O(1)` |
| `#bb.metric.replay.retained-edit-count` | **Retained edit count**: sum of resolved edits across retained batches. | D | Now for typed-history replay total; Inst elsewhere | X | `+O(1)` |
| `#bb.metric.replay.tick-coverage-interval` | **Tick coverage interval**: first tick, last tick, and represented tick count in the retained log. | D | Inst | X | `O(L)` or incremental |
| `#bb.metric.replay.tick-gap-vector` | **Tick gap vector**: missing, duplicate, and reversed tick values within retained coverage. | D | Inst | X | `O(L)` or incremental |
| `#bb.metric.replay.canonical-replay-bytes` | **Canonical replay bytes**: schema-bound ordered replay input with explicit tick boundaries. InteractionTape v1 currently encodes normalized accepted actions, including idle ticks; resolved attempts/misses/dispositions and a carrier binding replay input to its result remain separate objects. | D | Now InteractionTape v1 input; Future authoritative result carrier | X | `O(E+L)` |
| `#bb.metric.replay.replay-transition-count` | **Replay transition count**: individual replayed edits that changed state. | D | Now | X | `O(E)` |
| `#bb.metric.replay.replay-final-cardinality` | **Replay final cardinality**: cardinality owned by `world.md` after applying the declared log to its declared initial world. | D | Now | X | `O(E)` |
| `#bb.metric.replay.exact-replay-equality` | **Exact replay equality**: coordinate-set equality between reconstructed and expected post-worlds. | A | Inst | X | expected `O(N_1+N_2)` |
| `#bb.metric.replay.replay-root-equality` | **Replay root equality**: `#bb.metric.hash.canonical-world-state-root` roots agree at the same declared boundary. A matching digest is a commitment check, not mathematical equality. | A | Future | X | `O(1)` after roots |
| `#bb.metric.replay.first-divergent-tick` | **First divergent tick**: earliest state boundary at which exact worlds or roots disagree. | A | Future history | X | prefix replay |
| `#bb.metric.replay.first-divergent-edit` | **First divergent edit**: earliest exact instruction producing disagreement from equal prestate. | A | Future history | X | prefix replay |
| `#bb.metric.replay.divergent-coordinate-count` | **Divergent-coordinate count**: cardinality of the symmetric difference between compared exact worlds. | A | Inst | X | expected `O(N_1+N_2)` |
| `#bb.metric.replay.verified-prefix-length` | **Verified prefix length**: longest contiguous batch/edit prefix with verified expected state boundaries. | A | Future history | X | `O(prefix)` |
| `#bb.metric.replay.append-law-check` | **Append-law check**: replaying `a ++ b` equals replaying `a` and then `b` from the same prestate. | A | Now in proof/tests | X | `O(len(a)+len(b))` |
| `#bb.metric.replay.initial-world-identity` | **Initial-world identity**: exact identifier/root of the state from which a replay claim begins. The research history journal records the caller-supplied canonical world tag in a 32-byte genesis payload; that commitment neither discovers nor authorizes the world. | D | Now local history/research journal | X under declared hash | `O(1)` after root |
| `#bb.metric.replay.state-boundary-retention-frequency` | **State-boundary retention frequency**: which edit/tick boundaries carry expected roots or exact snapshots. | D | Future | X | `+O(1)` |
| `#bb.metric.replay.rollback-reconstruction-work` | **Rollback reconstruction work**: exact edits replayed from the nearest retained checkpoint to a requested historical boundary. | D | Future | X | `O(replayed edits)` |
| `#bb.metric.replay.receipt-tick-gap-evidence-relation` | **Receipt tick-gap evidence relation**: for each consecutive receipt pair, previous/next tick, skipped interval, evidenced-empty/permitted-unreceipted/unknown classification, and evidence tag. The live in-memory FrameReceipt v2 path retains every fixed-tick `TickOutcome`, supplies each stream's exact skipped suffix, and replays every successor edge before one backend submission; semantic tick replay and durable/shared resolution remain separate. | D/A | Now live in-memory FrameReceipt v2 structural evidence; Future durable/semantic evidence | X for retained supplied outcomes | `O(receipts + outcomes + attempts)` |
| `#bb.metric.replay.derived-effect-trace-equality` | **Derived-effect trace equality**: compare every stored transition effect against replay-derived mint, burn with consumed mint, or no-op reason plus event tag and coordinate, retaining all mismatches. Typed journal replay derives effects but stores and checks only final-event/post-world commitments, so it is not an independent stored-effect comparison. | A | Now in in-memory verifier/tests | X | `O(E)` |
| `#bb.metric.replay.active-mint-provenance-map-equality` | **Active-mint provenance-map equality**: exact comparison of the live coordinate-to-mint-event map against reconstruction from genesis and the verified receipt prefix, retaining missing, extra, and wrong-tag coordinates. Typed journal open reconstructs the map but does not compare it to an independently stored map. | A | Now in in-memory verifier/tests | X | `O(N + E)` |

## Determinism, invariants, tests, and formal proof

| Registry name | Metric and exact definition | Class | Avail | Exact | Cost |
|---|---|---:|---:|---:|---:|
| `#bb.metric.proof.bedrock-invariant-check` | **Bedrock invariant check**: permanent origin predicate owned by world semantics holds. | D | Now | X | expected `O(1)` |
| `#bb.metric.proof.world-nonempty-invariant-check` | **World nonempty invariant check**: valid public world has at least bedrock. | D | Now | X | `O(1)` |
| `#bb.metric.proof.stored-length-reconciliation` | **Stored-length reconciliation**: cached length equals the sum of chunk counts and emitted on coordinates. | A | Now in tests | X | `O(C+N)` |
| `#bb.metric.proof.chunk-popcount-reconciliation` | **Chunk-popcount reconciliation**: every cached chunk count equals popcount of its 64 words. | A | Now in tests | X | `O(64C)` |
| `#bb.metric.proof.no-empty-container-invariant` | **No-empty-container invariant**: every allocated sparse chunk contains at least one on bit. | A | Now in tests | X | `O(C)` |
| `#bb.metric.proof.bounds-cache-reconciliation` | **Bounds-cache reconciliation**: cached world bounds equal a full recomputation. The bounds themselves remain `#bb.metric.world.*`. | A | Now in tests | X | `O(N+64C)` |
| `#bb.metric.proof.split-join-inverse-check` | **Split/join inverse check**: `join(split(coord)) == coord` over the declared test/domain set. | A | Now in tests | X | `O(cases)` |
| `#bb.metric.proof.local-index-range-check` | **Local-index range check**: every tested split index lies in `[0,4095]`. | A | Now in tests | X | `O(cases)` |
| `#bb.metric.proof.euclidean-negative-coordinate-check` | **Euclidean negative-coordinate check**: split uses the declared `div_euclid/rem_euclid` mapping. | A | Now in tests | X | `O(cases)` |
| `#bb.metric.proof.batch-invariant-check` | **Batch invariant check**: `#bb.metric.batch.batch-cardinality-invariant` holds for every constructed batch. | D | Now | X | `O(1)` |
| `#bb.metric.proof.transition-replay-check` | **Transition replay check**: replaying a batch from its exact prestate yields its exact poststate. | A | Now in tests | X | `O(E_t+N)` |
| `#bb.metric.proof.origin-preservation-check` | **Origin preservation check**: edit application and replay preserve permanent bedrock. | A | Now in Lean/tests | X | proof check or `O(E)` assay |
| `#bb.metric.proof.repeated-run-agreement` | **Repeated-run agreement**: identical initial world and exact batch log produce identical final exact state in repeated executors. | A | Inst | X | `O(runs*E)` |
| `#bb.metric.proof.build-profile-agreement` | **Build-profile agreement**: debug and release executors produce matching exact states/roots. | A | Inst | X | two builds/replays |
| `#bb.metric.proof.platform-agreement` | **Platform agreement**: declared OS/architecture executors produce matching roots. | A | Future matrix | X | matrix builds/replays |
| `#bb.metric.proof.hash-seed-independence` | **Hash-seed independence**: randomized sparse-map iteration order cannot change canonical serialization or roots. | A | Future hash | X | repeated replays |
| `#bb.metric.proof.parallelism-independence` | **Parallelism independence**: allowed worker counts produce matching canonical results. Current mutation is single-threaded. | A | Future | X | repeated replays |
| `#bb.metric.proof.first-executor-divergence` | **First executor divergence**: earliest tick/edit and exact differing coordinates among compared executors. | A | Inst | X | prefix comparison |
| `#bb.metric.proof.determinism-corpus-size` | **Determinism corpus size**: distinct canonical programs/logs in the agreement suite. | A | Inst | X | `O(corpus)` |
| `#bb.metric.proof.determinism-boundary-coverage` | **Determinism boundary coverage**: declared integer and chunk-boundary equivalence classes exercised. | A | Inst | X | `O(corpus edits)` |
| `#bb.metric.proof.formal-theorem-count` | **Formal theorem count**: checked theorem declarations in the Lean package. | A | Now | X | source/environment scan |
| `#bb.metric.proof.formal-definition-count` | **Formal definition count**: logical definitions constituting the Lean model. | A | Now | X | source/environment scan |
| `#bb.metric.proof.obligation-manifest-size` | **Obligation manifest size**: explicitly enumerated semantic proof obligations. | A | Future manifest | X | `O(obligations)` |
| `#bb.metric.proof.discharged-obligation-count` | **Discharged obligation count**: manifest obligations linked to checked theorems. Coverage is a view over `023/024`. | A | Future manifest | X | `O(obligations)` |
| `#bb.metric.proof.unproved-obligation-vector` | **Unproved obligation vector**: manifest tags lacking a checked theorem, with reason/scope. | A | Future manifest | X | `O(obligations)` |
| `#bb.metric.proof.unsound-placeholder-vector` | **Unsound placeholder vector**: `sorry`, `admit`, and equivalent proof escapes. | A | Now | X | source/compiler scan |
| `#bb.metric.proof.extra-axiom-vector` | **Extra axiom vector**: axioms beyond the declared Lean trusted base. | A | Now | X | assumption inspection |
| `#bb.metric.proof.assumption-inventory` | **Assumption inventory**: exact assumptions reported for each exported theorem. | A | Inst | X | Lean inspection |
| `#bb.metric.proof.lean-build-verdict` | **Lean build verdict**: checker exit status for the pinned proof package. | A | Now | X | proof build |
| `#bb.metric.proof.lean-toolchain-identity` | **Lean toolchain identity**: exact pinned compiler/kernel version and lock identity. | A | Now | X | `O(1)` files |
| `#bb.metric.proof.proof-input-digest` | **Proof input digest**: canonical digest of proof source, toolchain, manifest, and dependencies. | A | Future hash | X | `O(B)` |
| `#bb.metric.proof.modeled-rust-digest` | **Modeled Rust digest**: canonical digest of Rust semantics claimed by the proof receipt. | A | Future hash | X | `O(B)` |
| `#bb.metric.proof.proof-freshness` | **Proof freshness**: receipt's modeled-Rust digest equals current `#bb.metric.proof.modeled-rust-digest`. | A | Future receipts | X | `O(1)` |
| `#bb.metric.proof.rust-lean-definition-drift-vector` | **Rust/Lean definition drift vector**: duplicated constants/types/transition clauses that disagree. | A | Future correspondence | X | source-specific |
| `#bb.metric.proof.correspondence-gap-vector` | **Correspondence gap vector**: Lean model operations lacking a checked link to Rust behavior. | A | Future manifest | X | `O(obligations)` |
| `#bb.metric.proof.test-outcome-vector` | **Test outcome vector**: discovered, passed, failed, ignored, and crashed tests; never reduce crashes to failed assertions. | A | Now | X | test run |
| `#bb.metric.proof.generated-case-count` | **Generated-case count**: property/fuzz cases actually executed, with seed/corpus identity. | A | Future harness | X | harness run |
| `#bb.metric.proof.coverage-base-vector` | **Coverage base vector**: lines, branches, and declared semantic boundary classes reached. Percentages are views. | A | Inst | S for instrumentation | coverage run |
| `#bb.metric.proof.mutation-base-vector` | **Mutation base vector**: attempted, equivalent/invalid, killed, and surviving mutants. Mutation score is a view. | A | Future harness | X after adjudication | mutation run |
| `#bb.metric.proof.renderer-equivalence-quantifier-scope` | **Renderer-equivalence quantifier scope**: candidate/reference implementation and policy identities, shared-dependency disclosures, canonical input domain or corpus digest, checked stages/attachments and comparison policy, finite domain cardinality where declared, sampled-versus-exhaustive mode, and attempted/completed case counts. The current bounded harness compares Rust projected-triangle rasterization with a stdlib-only Python per-pixel six-face oracle over two named direct-cube fixtures, complete post-overlay XRGB, world-pass depth classes, and finite-depth residuals bounded to four ULP. Its corpus tag binds both semantic profile tags, exact fixture camera/block inputs, reference frame tags, and reference depth tags. Source/executable identities, an explicit shared-dependency manifest, the four-ULP policy, and attempted/completed result counts are not yet in a canonical result receipt, so the current realization is partial. Universal equivalence remains unavailable unless exhaustive finite coverage or a theorem discharges it. | A | Now partial bounded independent RGB/depth harness | X within declared corpus | `O(cases)` bookkeeping plus render comparisons |
| `#bb.metric.proof.theorem-statement-identity-relation` | **Theorem-statement identity relation**: every exported Lean declaration maps to a normalized elaborated proposition/type digest and its semantic obligation tag. | A | Future extraction | X | proof-environment scan |
| `#bb.metric.proof.rust-lean-refinement-outcome` | **Rust/Lean refinement outcome**: per declared Rust operation/schema, the Lean relation, theorem or finite-assay scope, pre/post/effect agreement, and first counterexample. | A | Future refinement bridge | X over declared scope | theorem check or corpus |
| `#bb.metric.proof.world-supply-model-refinement-outcome` | **World/supply-model refinement outcome**: checked relation mapping observational `World` lists to duplicate-free `SupplyWorld`, preserving membership, origin, mutable supply, and edit simulation. | A | Future Lean theorem | X | proof build |
| `#bb.metric.proof.claim-quantifier-scope-relation` | **Claim quantifier-scope relation**: every emitted verification claim records theorem, exhaustive finite, symmetry-reduced, inductive, or sampled shape; domain/corpus tag and cardinality; attempted/completed coverage; and any scope overclaim. | A | Future audit receipts | X for recorded scope | `O(claims)` |
| `#bb.metric.proof.declaration-dependency-relation` | **Declaration dependency relation**: direct content-identified edges from Lean definitions and theorem types/bodies to declarations and axioms. | A | Future extraction | X | environment graph scan |

## Hashes, ternary hashing, and receipts

Bitblock now has canonical SHA-256 content tags for local worlds, events,
receipts, metric definitions, final CPU framebuffers, every successful fixed
tick outcome, and a feature-gated local sequential-history transcript, plus a
replay-checked in-memory receipt chain.
Rows requiring chunks, signatures, independent anchors, sequentiality or
elapsed-time evidence, or consensus remain `Future`. A world tag commits to
the finite on-set; it does not prove "infinite space".
Unspecified coordinates are off by the world definition.

| Registry name | Metric and exact definition | Class | Avail | Exact | Cost |
|---|---|---:|---:|---:|---:|
| `#bb.metric.hash.canonical-encoding-version` | **Canonical encoding version**: identifier for byte/trit framing, integer endianness, ordering, and length rules. | D | Now for integrated domains | X | `O(1)` |
| `#bb.metric.hash.algorithm-identity` | **Algorithm identity**: exact hash algorithm, version, parameters, and digest length. | D | Now SHA-256 | X | `O(1)` |
| `#bb.metric.hash.domain-separation-identity` | **Domain-separation identity**: closed tag for world, chunk, edit, program, tick-outcome, receipt, snapshot, proof, render-input, framebuffer, frame-receipt, display-receipt, display-artifact, presentation-artifact, output-topology, presentation-output-artifact, registry, key, policy, intent, and sequential-history objects. | D | Now for integrated domains including InteractionTape v1 under `intent`, TickOutcome v2, FrameReceipt v2, DisplayReceipt v1/v2, DisplayArtifact v1, composition-timing envelope v1 under `presentation-artifact`, OutputTopology v1 under `output-topology`, PresentationOutput v1 under `presentation-output-artifact`, and local history-evidence receipts/sequential history; Future other codecs | X | `O(1)` |
| `#bb.metric.hash.canonical-world-encoding-bytes` | **Canonical world encoding bytes**: byte/trit length of the canonically ordered finite on-set. | D | Now | X | `O(N log N)` raw sort |
| `#bb.metric.hash.per-chunk-content-digest` | **Per-chunk content digest**: hash of domain tag, chunk coordinate, and exact 4096-bit payload. | D | Future | X | `O(64)` per chunk |
| `#bb.metric.hash.canonical-world-state-root` | **Canonical world-state root**: commitment to encoding version and exact finite world state. | D | Now as full world tag | X | `O(N log N)` raw sort and hash |
| `#bb.metric.hash.chunk-tree-root` | **Chunk-tree root**: canonical Merkle root over ordered `#bb.metric.hash.per-chunk-content-digest` leaves. | D | Future | X | full `O(C log C)` |
| `#bb.metric.hash.edit-digest` | **Edit digest**: full content tag of the exact operation byte and three big-endian `i64` coordinates under the standalone Edit schema. | D | Now | X | `O(1)` |
| `#bb.metric.hash.program-digest` | **Program digest**: hash of ordered canonical edits and explicit length. | D | Future | X | `O(E)` |
| `#bb.metric.hash.tick-batch-digest` | **Tick-batch digest**: full `#bb.tick-outcome.sha256.*` tag over the complete TickOutcome v2 canonical payload. Every live fixed tick is retained in the append-only in-memory chain; this is not a digest of the smaller legacy `TickBatch` projection. | D | Now retained outcome chain | X under declared hash/codec | `O(A_t)` |
| `#bb.metric.hash.prestate-root` | **Prestate root**: `#bb.metric.hash.canonical-world-state-root` immediately before each retained tick outcome. | D | Now retained outcome chain | X under declared hash | `O(1)` after world tag |
| `#bb.metric.hash.poststate-root` | **Poststate root**: `#bb.metric.hash.canonical-world-state-root` immediately after each retained tick outcome. | D | Now retained outcome chain | X under declared hash | `O(1)` after world tag |
| `#bb.metric.hash.no-op-root-stability` | **No-op root stability**: pre/post roots agree exactly when the exact pre/post states agree, conditional on hash commitment assumptions. | A | Future | X | state audit plus roots |
| `#bb.metric.hash.receipt-digest` | **Receipt digest**: content tag over schema, predecessor receipt, tick, ordered exact edits, final event occurrence, post-world tag, and explicit absent registry/mode markers. Cached effect records are replay-checked but are not authority bytes. | D | Now local/in-memory and protocol-research journal | X under declared hash | `O(E_t)` |
| `#bb.metric.hash.receipt-chain-head` | **Receipt-chain head**: latest receipt digest. | D | Now local/in-memory and protocol-research journal | X under declared hash | `O(1)` |
| `#bb.metric.hash.receipt-chain-length` | **Receipt-chain length**: receipts in the declared chain. | D | Now local/in-memory and protocol-research journal | X | `O(1)` |
| `#bb.metric.hash.predecessor-continuity-check` | **Predecessor continuity check**: each receipt names the prior receipt digest. | A | Now in verifier | X | `O(R_c)` |
| `#bb.metric.hash.receipt-tick-continuity-check` | **Receipt tick continuity check**: receipt-bearing tick values are strictly increasing with no duplicates or reversals; intentional gaps require separate evidence rather than being treated as continuous history. | A | Now in verifier | X | `O(R_c)` |
| `#bb.metric.hash.receipt-state-continuity-check` | **Receipt state continuity check**: replay from the prior verified poststate produces the next receipt's declared post-world tag. | A | Now in verifier | X | `O(R_c + E)` |
| `#bb.metric.hash.verified-receipt-prefix-length` | **Verified receipt-prefix length**: longest prefix passing strict receipt decode/re-encoding, object tag, semantic parent, increasing tick, exact transition replay, final-event tag, post-world tag, and receipt-tag checks after a valid genesis. Physical storage-envelope admission remains a separate prerequisite. | A | Now typed local replay; Future modes/signatures | X under declared codecs/hash | `O(R_c+E+world-tag work)` |
| `#bb.metric.hash.invalid-receipt-vector` | **Invalid receipt vector**: failures by canonical decode, object/component tag, transition, continuity, signature, or policy reason. Typed replay has a closed local codec/semantic reason set and a distinct retryable local-policy refusal; signatures and consensus policy remain future. | A | Now typed local replay subset | X | verification cost |
| `#bb.metric.hash.first-invalid-receipt` | **First invalid receipt**: earliest storage-record index containing a receipt that fails and its exact codec, semantic, or local-policy reason. | A | Now typed local replay | X | prefix verification |
| `#bb.metric.hash.receipt-topology-vector` | **Receipt topology vector**: gaps, duplicates, forks, and orphans. | D | Future | X | expected `O(R_c)` |
| `#bb.metric.hash.receipt-encoded-bytes` | **Receipt encoded bytes**: canonical receipt payload bytes per receipt and total. The current binary payload is exactly `114 + 25 * edit_count`; its enclosing storage record is measured separately. | D | Now protocol-research journal; Future trits | X | `O(R_c)` |
| `#bb.metric.hash.incremental-full-root-agreement` | **Incremental/full-root agreement**: cached incremental root equals full canonical recomputation. | A | Future | X | full root cost |
| `#bb.metric.hash.hash-cache-event-vector` | **Hash-cache event vector**: hit, miss, invalidation, and recomputation for chunk/tree nodes. | R | Future | X | `+O(1)` per event |
| `#bb.metric.hash.dirty-hash-node-count` | **Dirty hash-node count**: invalidated nodes awaiting recomputation. | R | Future | X | `O(1)` maintained |
| `#bb.metric.hash.root-commit-lag` | **Root commit lag**: tick boundaries between world mutation and committed corresponding state root. | R | Future | X | `+O(1)` |
| `#bb.metric.hash.hash-input-bytes` | **Hash input bytes**: canonical bytes/trits fed to each domain. InteractionTape v1 exposes its complete canonical payload/preimage; sequential segment payload bytes and independent recurrence-vector preimages are also exposed. | R | Now InteractionTape v1 and sequential-history subset; Inst remaining domains | X | `+O(1)` |
| `#bb.metric.hash.inclusion-proof-depth` | **Inclusion proof depth**: sibling levels from claimed chunk/bit leaf to root. | D | Future | X | `O(log C)` |
| `#bb.metric.hash.inclusion-proof-bytes` | **Inclusion proof bytes**: canonical encoded membership/absence proof length. | D | Future | X | `O(log C)` |
| `#bb.metric.hash.inclusion-proof-verdict` | **Inclusion proof verdict**: claimed membership or absence verifies against root. | A | Future | X | `O(log C)` |
| `#bb.metric.hash.digest-symbol-vector` | **Digest symbol vector**: counts of negative/zero/positive trits, or zeros/ones for binary algorithms, over a declared corpus. | A | Future | X | `O(corpus*digest_len)` |
| `#bb.metric.hash.empirical-entropy-inputs` | **Empirical entropy inputs**: digest-symbol and observed-output frequency tables. Entropy is a sampled view, never a security proof. | A | Future | S | corpus scan |
| `#bb.metric.hash.avalanche-event` | **Avalanche event**: output symbol/Hamming distance caused by one declared canonical-input mutation. | A | Future | S | two hashes per trial |
| `#bb.metric.hash.avalanche-trial-identity-count` | **Avalanche trial identity/count**: mutation method, seed/corpus, and trials actually executed. | A | Future | X | `O(trials)` |
| `#bb.metric.hash.empirical-collision-vector` | **Empirical collision vector**: unequal canonical corpus inputs sharing a full or declared truncated digest. Zero is not proof of resistance. | A | Future | X | expected `O(corpus)` memory |
| `#bb.metric.hash.encoding-round-trip-check` | **Encoding round-trip check**: decode(encode(value)) returns exactly the value and rejects ambiguous/noncanonical alternatives. | A | Now for Edit/InteractionTape v1/storage envelope/history receipt/sequential segment/FrameReceipt v2/DisplayReceipt v2/DisplayArtifact v1/composition-timing envelope v1/OutputTopology v1/PresentationOutput v1; Future other objects | X | `O(B)` |
| `#bb.metric.hash.domain-separation-collision-vector` | **Domain-separation collision vector**: unintended equal digests for equal payload under distinct semantic domains. | A | Future | X | domain matrix |
| `#bb.metric.hash.receipt-signature-vector` | **Receipt signature vector**: unsigned, valid, invalid, unknown-signer, and unsupported-algorithm receipts. | A | Future | X | signature verification |
| `#bb.metric.hash.verified-signer-count` | **Verified signer count**: distinct accepted signer identities represented at a boundary. | A | Future | X | expected `O(signatures)` |
| `#bb.metric.hash.threshold-attainment` | **Threshold attainment**: valid independent signer count and declared required threshold. Boolean/ratio are views. | A | Future | X | `O(signatures)` |
| `#bb.metric.hash.external-anchor-event` | **External anchor event**: chain head, external medium identity, and anchoring tick/host timestamp. | H | Future | X | external operation |
| `#bb.metric.hash.external-anchor-verification` | **External anchor verification**: independent record resolves to the expected receipt-chain head. | A | Future | X | external read/verify |
| `#bb.metric.hash.canonical-render-input-digest` | **Canonical render-input digest**: `H(frame-input domain, encoding version, world root, exact stored camera f64 bit patterns, viewport, renderer/profile tag, overlay configuration, overlay payload or root)`. `FrameInput` v1 currently commits the world tag, all ten camera bit lanes, dimensions, renderer-profile tag, and the typed overlay-input tag under a versioned schema; live rendering contextually reconstructs this object before frame publication, and fixed Rust/Python known answers cover the hash encoding. It commits runtime inputs, not real-number or cross-platform float equivalence. | R | Now FrameInput v1 and live contextual binding; Future durable codec and independent semantic replay | X | `O(view_bytes + overlay_bytes)` |
| `#bb.metric.hash.final-framebuffer-digest` | **Final framebuffer digest**: hash of framebuffer domain, encoding version, `#bb.metric.render.final-framebuffer-storage-contract`, dimensions, checked pixel count, and every final post-overlay row-major `u32`. Digest equality remains conditional on hash assumptions; production and backend submission are separate dispositions. | R | Now for produced changed game/composite frames, including headless CPU frames; submitted frames are a subset | X as digest | `O(P)` |
| `#bb.metric.hash.frame-provenance-receipt-digest` | **Frame-provenance receipt digest**: version-distinct hash of the frame-receipt domain, predecessor receipt or nil, then either the v1 source tick or the v2 stream selector and declared `TickOutcome` tag, followed by the canonical render-input and final-framebuffer digests. Live presentation prepares the independent game/composite v2 paths, reserves both ledgers, calls the backend once, and publishes both prepared receipts only after success; v1 remains a staged compatibility sidecar for the sidebar predecessor. The opt-in display audit accepts the typed committed composite v2 receipt and derives its expected frame from that object. F4 accepts only the exact current committed composite occurrence and embeds all canonical receipt bytes in DisplayArtifact v1. F5 arms the next normal successful presentation, requires that same occurrence against the live composite-ledger head and unchanged compositor pixels, and PresentationOutput v1 enforces full equality between the receipt embedded by DisplayArtifact v1 and the receipt embedded by its composition-timing envelope before mixed-journal admission. The research optimization assay accepts only game/composite v2 ledgers, checks their complete retained outcome evidence against verified coordinate history, and binds its synthetic display to the full committed composite receipt. Mixed legacy/wrapper replay retains the embedded composite receipt bytes, full display pixels, timing, and topology objects through one display-parent chain, but referenced TickOutcome and FrameInput objects are not supplied or independently verified. Fixed-genesis independent semantic replay, sudden-power survival, latest-prefix authority, and external custody remain separate. | R | Now v1 sidebar compatibility, live in-memory v2 structural publication, exact F4/F5 one-shot retention, typed-v2 optimization admission, and typed local mixed replay; Future power-durable and independent semantic admission | X for digest and retained structural path, not independent pixel semantics | `O(1)` digest after components; `O(path)` admission or artifact replay |
| `#bb.metric.hash.known-answer-vector-verdict` | **Known-answer-vector verdict**: per versioned vector, canonical input/domain/parameters, expected digest, observed digest, and first differing symbol. | A | Now fixed InteractionTape v1/sequential/history-evidence/Frame v1 analytic-renderer corpus/FrameReceipt v2/DisplayReceipt v1-v2/DisplayArtifact v1/composition-timing envelope v1/OutputTopology v1/PresentationOutput v1/optimization-equivalence rejected-v2 and v3 pass/fail vectors; Inst first-difference diagnostic; Future other domains | X | `O(vectors * hash_cost)` |
| `#bb.metric.hash.cross-implementation-digest-parity` | **Cross-implementation digest parity**: longest corpus prefix on which named independent implementations produce byte/trit-identical digests, plus first divergence. | A | Now fixed Rust/Python InteractionTape v1/sequential/history-evidence/Frame v1 analytic-renderer corpus/FrameReceipt v2/DisplayReceipt v2/DisplayArtifact v1/composition-timing envelope v1/OutputTopology v1/PresentationOutput v1 agreement; Inst corpus-prefix/divergence collector | X | `O(implementations * corpus * hash_cost)` |
| `#bb.metric.hash.representation-substrate-inventory` | **Representation-substrate inventory**: declared input/state/output alphabets, lane widths, bit/trit boundary conversions, and primitive-operation counts, so a binary-word construction cannot be reported as native balanced ternary. | A | Future | X | source trace or instrumented hash |
| `#bb.metric.hash.declared-parameter-liveness-vector` | **Declared-parameter liveness vector**: for every semantic parameter (domain, tick, rounds, depth, output length, key, salt), paired inputs that should differ or reject and whether the implementation actually observes the parameter. | A | Future | X | paired hashes |
| `#bb.metric.hash.round-map-image-cardinality` | **Round-map image cardinality**: distinct outputs of the internal state transition over a declared finite state domain, with bijection verdict and exact coverage. | A | Future | X exhaustive, S sampled | `O(state_domain * round_cost)` |
| `#bb.metric.hash.round-map-orbit-decomposition` | **Round-map orbit decomposition**: cycle-length and transient-length multisets, fixed-point witnesses, and permutation order over a declared finite state domain. | A | Future | X exhaustive, S sampled | `O(state_domain)` after map |
| `#bb.metric.hash.input-output-influence-matrix` | **Input/output influence matrix**: for each input-symbol position and round depth, output positions changed by every declared one-symbol mutation. Full-diffusion round is a view. | A | Future | X exhaustive, S sampled | mutation matrix hashes |
| `#bb.metric.hash.reduced-round-differential-profile` | **Reduced-round differential profile**: frequency table mapping declared input-difference classes to output-difference classes at every round depth. | A | Future | X exhaustive, S sampled | corpus-pair sweep |
| `#bb.metric.hash.ternary-s-box-differential-uniformity` | **Ternary S-box differential uniformity**: maximum multiplicity of `S(x+a)-S(x)=b` over nonzero balanced-ternary `a`, retaining the complete `(a,b)` table. | A | Future | X for finite S-box | `O(q^2)` |
| `#bb.metric.hash.ternary-s-box-affine-distance` | **Ternary S-box affine distance**: minimum Hamming distance from S-box coordinate functions to the declared affine-function family over `GF(3)`, with minimizing witnesses. | A | Future | X for finite S-box | exhaustive affine census |
| `#bb.metric.hash.orthomorphism-verdict` | **Orthomorphism verdict**: whether `t -> t + g(t)` is a permutation for each declared trit map `g`, with image cardinality and collision witnesses. | A | Future | X | `O(q)` per map |
| `#bb.metric.hash.independent-cryptanalysis-evidence-vector` | **Independent cryptanalysis evidence vector**: version-bound external attack artifacts by class, analyst independence, result (`break\|hold\|open`), scope, and reproducible evidence tag. Absence remains `N/A`, never secure. | A | Future | X for artifact census; no security inference | artifact audit |
| `#bb.metric.hash.semantic-tag-validation-vector` | **Semantic-tag validation vector**: encountered tags classified full canonical, malformed/noncanonical, unknown kind/schema/suite, digest mismatch, or truncated-display-only by recomputing the domain-separated full digest. | A | Future tags | X | `O(tag_bytes)` |
| `#bb.metric.hash.authoritative-reference-kind-relation` | **Authoritative reference-kind relation**: every schema field declared identity-bearing classified as full tag, name plus pinned registry tag, unpinned name, numeric/sequence identifier, or truncated tag. Coordinates, ticks, and lengths are values, not identities. | A | Future protocol manifest | X | `O(identity_fields + records)` |
| `#bb.metric.hash.content-reference-closure` | **Content-reference closure**: tags reachable from a declared genesis/head partitioned present-and-domain-valid, missing, wrong-domain, or cyclic where cycles are forbidden. | A | Future object store | X | `O(objects + references + verified_bytes)` |
| `#bb.metric.hash.registry-version-relation` | **Registry-version relation**: `(registry tag, parent registry tag, patch tag, resulting binding root)` for every retained version. Head, gaps, forks, and cycles are views. | D | Future registry | X | `O(patches)` rebuild, `+O(1)` append |
| `#bb.metric.hash.registry-binding-relation` | **Registry-binding relation**: exact `(registry tag, canonical ASCII name, target tag or tombstone)` state after the pinned patch chain. Bind/rebind/tombstone counts are views. | D | Future registry | X | `O(bindings + patches)` |
| `#bb.metric.hash.registry-resolution-outcome` | **Registry-resolution outcome**: each `(pinned registry tag, name)` query yields exactly one target or a closed malformed, unbound, tombstoned, missing-history, or ambiguous reason. Mutable `current` aliases never count as pinned. | D/A | Future registry | X | indexed `O(1)`, otherwise `O(patches)` |
| `#bb.metric.hash.event-occurrence-tag-relation` | **Event-occurrence tag relation**: ordered per-edit tags `q[i+1]=Tag(event,schema,encode(q[i],tick,edit[i]))` with `q0=parent receipt tag`; unlike edit content digests, repeated identical edits receive distinct occurrence identities. | D | Now local/in-memory and typed replay | X under declared hash | `O(E)` |
| `#bb.metric.hash.anchored-head-ancestry-relation` | **Anchored-head ancestry relation**: for each verified external anchor and candidate local head, ancestor/descendant distance or fork/no-path result. This exposes rollback relative to an older pinned head. | A | Now for journal ancestor presence; Future distance/fork index and external custody | X for supplied tag and local prefix | `O(receipts + anchors)`, indexed faster |
| `#bb.metric.hash.receipt-validation-mode-relation` | **Receipt validation-mode relation**: per rule epoch, active mode-policy tag, required open/signature/threshold/sequential/stake checks, predecessor mode tag, and authorized effective boundary. The current sequential result is a whole-history sidecar and does not populate the receipt mode field. | D | Future receipt modes; Now sidecar boundary only | X | `O(mode_epochs)` |
| `#bb.metric.hash.mode-admission-outcome` | **Mode-admission outcome**: per coordinate-history/mode claim, pinned mode tag, verified-history result, required-check verdicts, accepted/rejected state, and exact first reason. | A | Now local whole-history open/sequential/stake-refusal subset; Future durable receipt modes | X | sum of required checks |
| `#bb.metric.hash.sequential-history-segment-relation` | **Sequential-history segment relation**: `(exact function tag, parent segment tag or nil, absolute start, iteration count, input state tag, ordered event-occurrence tags with nondecreasing relative offsets, output state tag)`, encoded in exactly `153 + 40 * injections` bytes. Same-offset order is semantic. | D | Now protocol-research local transcript | X | `O(injections)` record |
| `#bb.metric.hash.sequential-history-verification-outcome` | **Sequential-history verification outcome**: exact function, tag, parent/start/input continuity, bounded recomputed output, usage vector, and first refused segment/reason for `#bb.metric.hash.sequential-history-segment-relation`; event mismatch index is owned separately by coverage. Intermediate states are not retained, so this does not claim the first divergent internal iteration. | A | Now protocol-research local verifier | X for transcript | `O(iterations + injections)` |
| `#bb.metric.hash.anchored-sequential-work-depth` | **Anchored sequential-work depth**: exact declared and reverified iteration sum from each independently verified anchor to a head. Rewrite-cost/security interpretations are assumption-dependent views. | D/A | Future sequential history | X count, not elapsed time | `O(segments since anchor)` |
| `#bb.metric.hash.time-evidence-boundary-relation` | **Time-evidence boundary relation**: every history/time claim linked explicitly to order-chain evidence, sequential-work evidence, host timestamps, external anchors, and/or quorum attestations. Without calibration or anchors, elapsed-time proof is unavailable. | D/H/A | Future PoH claims | X inventory; elapsed bound S/NA | `O(claims + anchors)` |
| `#bb.metric.hash.key-descriptor-relation` | **Key-descriptor relation**: `(key tag, algorithm/parameter tag, canonical public-key-bytes tag)` with full-tag validation. | D | Future keys | X | `O(keys + key_bytes)` |
| `#bb.metric.hash.key-lifecycle-relation` | **Key-lifecycle relation**: authorized rotate, revoke, and supersede edges `(old key, new key/action, effective receipt, proof tag)`. Status at a boundary is a view. | D/A | Future keys | X with complete history | `O(key_events)` |
| `#bb.metric.hash.authorization-policy-state` | **Authorization-policy state**: pinned policy tag plus exact principal-key tags, actions, resource/coordinate scopes, thresholds, and predecessor policy tag. | D | Future authorization | X | `O(policy_bytes + edges)` |
| `#bb.metric.hash.authorization-decision-relation` | **Authorization-decision relation**: per signed intent, `(intent tag, signer keys, action, resource/coordinate, pinned policy, allow\|deny, closed reason, witness tags)`. Unauthorized mint/burn counts are views. | D/A | Future authorization | X | policy dependent |
| `#bb.metric.hash.signed-intent-consumption-replay-relation` | **Signed-intent consumption/replay relation**: intent tags partitioned unseen-and-accepted, already-consumed/replay, expired/wrong-parent, and rejected-without-consumption. | D/A | Future authorization | X | expected `O(1)` lookup, `O(intents)` rebuild |
| `#bb.metric.hash.cross-genesis-rules-mode-signature-replay-assay` | **Cross-genesis/rules/mode signature-replay assay**: whether a signature accepted in one domain also verifies in any distinct declared genesis, rules, or mode domain; every acceptance is a witness. | A | Future signatures | X over tested matrix | `O(vectors * signature_verifies)` |
| `#bb.metric.hash.receipt-admission-coverage-relation` | **Receipt admission-coverage relation**: for each tick in a declared interval, attempts, resolved edits, state changes, emission policy result, observed receipt or none, and intentional-omission/unexpected-missing/unexpected-extra class. | D/A | Now local policy/Inst audit | X | `O(ticks)` |
| `#bb.metric.hash.tag-authority-field-coverage-relation` | **Tag authority-field coverage relation**: for every tagged schema, each field path is hashed, derived-and-reverified, or local-only with canonical byte interval/reference and verifier predicate; authority-affecting uncovered fields are witnesses. | A | Future schema manifest | X over manifest | `O(schema fields)` |
| `#bb.metric.hash.genesis-configuration-closure` | **Genesis-configuration closure**: genesis tag's committed tuple and closure verdict over initial world, codec/rules/registry tags, validation/consensus policy, and all required referenced objects. | D/A | Future network genesis | X with complete closure | `O(references)` |
| `#bb.metric.hash.key-validation-outcome-relation` | **Key-validation outcome relation**: per key, canonical decode, algorithm/parameter match, length/range/subgroup/weak-key checks where applicable, allowed uses, and closed accept/reject reason. | A | Future keys | X | key-dependent verification |
| `#bb.metric.hash.signature-canonicality-malleability-witness-vector` | **Signature canonicality/malleability witness vector**: distinct accepted signature byte strings for the same pinned key and canonical message plus every accepted noncanonical encoding. | A | Future signatures | X over corpus | signature assay |
| `#bb.metric.hash.key-control-domain-relation` | **Key-control-domain relation**: key tag, declared controlling principal/custody domain, evidence source/tag, effective boundary, verification status, and caveat. Distinct keys do not imply independent control. | A/H | Future custody evidence | X for records, S for real control | `O(keys + evidence)` |
| `#bb.metric.hash.sequentiality-evidence-vector` | **Sequentiality evidence vector**: version-bound independent assays recording implementation/hardware, parallel resources, verified iterations/output, observed work depth/speedup, and break/hold/open result. Transcript verification alone is not sequentiality proof. | A/H | Future sequential history | S | assay-dependent |
| `#bb.metric.hash.sequential-history-event-coverage-relation` | **Sequential-history event-coverage relation**: after semantic replay of one supplied `History` against its initial world, the complete receipt/effect sequence flattened in receipt/edit order equals the complete segment-injection sequence in count and occurrence-tag order, retaining the first mismatch or count witness. It is whole-history only, not an independently verifiable receipt interval. | D/A | Now local whole-history sidecar | X for supplied verified history | `O(history replay + events + injections)` |

## Implementation memory and sparse-container mechanics

This section may use the `N` and `C` state values owned by `world.md` as
denominators, but it does not redefine them or chunk occupancy.

| Registry name | Metric and exact definition | Class | Avail | Exact | Cost |
|---|---|---:|---:|---:|---:|
| `#bb.metric.memory.fixed-chunk-bit-payload-bytes` | **Fixed chunk bit-payload bytes**: `512*C` for 64 `u64` words per allocated chunk. | R | Now | X | `O(1)` from `C` |
| `#bb.metric.memory.chunk-struct-footprint` | **Chunk struct footprint**: `size_of::<Chunk>()*C`, including struct padding but excluding map buckets. | R | Inst | X | `O(1)` from `C` |
| `#bb.metric.memory.map-entry-payload-estimate` | **Map entry payload estimate**: `size_of::<(ChunkCoord, Chunk)>()*C`; allocator/bucket overhead is excluded. | R | Inst | S | `O(1)` |
| `#bb.metric.memory.sparse-map-capacity` | **Sparse-map capacity**: reserved hash-table bucket capacity. | R | Inst | X | `O(1)` |
| `#bb.metric.memory.allocator-attributed-world-bytes` | **Allocator-attributed world bytes**: exact requested/live heap bytes owned by sparse world structures under an instrumented allocator. | H | Inst | X | allocator hooks |
| `#bb.metric.memory.chunk-lookup-event-vector` | **Chunk lookup event vector**: allocated-chunk hit versus implicit-off missing-chunk miss. | R | Inst | X | `+O(1)` per lookup |
| `#bb.metric.memory.sparse-map-mutation-event-vector` | **Sparse-map mutation event vector**: entry create, entry remove, replace, reserve, shrink, and rehash. | R | Inst | X | `+O(1)` per event |
| `#bb.metric.memory.rehash-moved-entry-count` | **Rehash moved-entry count**: live entries relocated across capacity changes. | H | Inst | X | instrument rehash |
| `#bb.metric.memory.chunk-lifetime-event` | **Chunk lifetime event**: ticks from sparse entry creation to removal. | R | Inst | X | metadata `+O(1)` |
| `#bb.metric.memory.chunk-resurrection-event` | **Chunk resurrection event**: a chunk coordinate is recreated after an earlier removal. | R | Inst | X | expected `+O(1)` history |
| `#bb.metric.memory.distinct-chunks-touched-by-batch` | **Distinct chunks touched by batch**: sparse entries queried/mutated by resolved edits; this is implementation work, not occupancy. | D | Inst | X | expected `O(E_t)` |
| `#bb.metric.memory.distinct-words-touched-by-batch` | **Distinct words touched by batch**: `(chunk, u64-word-index)` pairs addressed by resolved edits. | D | Inst | X | expected `O(E_t)` |
| `#bb.metric.memory.iterator-word-inspections` | **Iterator word inspections**: fixed 64-bit words examined while emitting on coordinates. | R | Inst | X | `+O(1)` per word |
| `#bb.metric.memory.bounds-maintenance-event-vector` | **Bounds-maintenance event vector**: cached-bound extension, interior removal, boundary removal, and full recomputation. Bounds values stay `#bb.metric.world.*`. | D | Inst | X | `+O(1)` per event |
| `#bb.metric.memory.bounds-recompute-scan-work` | **Bounds-recompute scan work**: chunks, words, and on coordinates inspected by a full recomputation. | R | Inst | X | `O(C+N)` subject cost |
| `#bb.metric.memory.pending-edit-storage` | **Pending-edit storage**: vector length, capacity, and attributed bytes for the system queue. | R | Inst | X | `O(1)` |
| `#bb.metric.memory.retained-batch-storage` | **Retained-batch storage**: length, capacity, and attributed bytes for latest/history batch programs. | R | Inst | X | `O(1)` with allocator data |
| `#bb.metric.memory.replay-log-storage` | **Replay-log storage**: live in-memory bytes attributable to retained replay history and indices. | R | Future history | X | allocator accounting |
| `#bb.metric.memory.receipt-hash-cache-storage` | **Receipt/hash-cache storage**: live bytes attributable to receipts, roots, Merkle nodes, and proof caches. | R | Future hash | X | allocator accounting |
| `#bb.metric.memory.process-resident-set` | **Process resident set**: physical pages resident for the process. | H | Inst | S | OS sample |
| `#bb.metric.memory.process-private-committed-bytes` | **Process private committed bytes**: process-private committed memory. | H | Inst | S | OS sample |
| `#bb.metric.memory.virtual-address-space` | **Virtual address space**: process reserved virtual bytes. | H | Inst | S | OS sample |
| `#bb.metric.memory.heap-live-peak-base-pair` | **Heap live/peak base pair**: current and high-water requested live heap bytes. | H | Inst | X with allocator hooks | allocator hooks |
| `#bb.metric.memory.allocator-traffic-vector` | **Allocator traffic vector**: allocation, deallocation, and reallocation operation counts and requested bytes. | H | Inst | X with hooks | allocator hooks |
| `#bb.metric.memory.reallocation-moved-bytes` | **Reallocation moved bytes**: payload bytes copied/moved by capacity growth. | H | Inst | X with hooks | allocator hooks |
| `#bb.metric.memory.allocator-fragmentation-estimate` | **Allocator fragmentation estimate**: reserved/committed allocator bytes minus requested live payload, under a declared allocator model. | H | Inst | S | allocator query |
| `#bb.metric.memory.page-fault-vector` | **Page-fault vector**: soft and hard process page faults. | H | Inst | S | OS sample |
| `#bb.metric.memory.metric-recorder-memory` | **Metric-recorder memory**: live/peak bytes attributable only to metric labels, counters, histograms, and retained samples. | H | Inst | X with hooks | allocator accounting |
| `#bb.metric.memory.known-resident-dirty-chunk-base-vector` | **Known/resident/dirty chunk base vector**: future durable-known chunks, memory-resident chunks, and modified-since-checkpoint chunks. | R | Future loading | X | maintained `O(1)` |
| `#bb.metric.memory.chunk-cache-event-vector` | **Chunk-cache event vector**: resident hit, storage miss, load, eviction, dirty writeback, and failed eviction. | R | Future loading | X | `+O(1)` per event |
| `#bb.metric.memory.chunk-working-set-event` | **Chunk working-set event**: chunk coordinate accessed in a declared tick window. Cardinality is a window view. | R | Future loading | X | expected `+O(1)` |
| `#bb.metric.memory.cold-resident-event` | **Cold-resident event**: a resident chunk crosses a declared untouched-tick threshold. | R | Future loading | X | cache-dependent |
| `#bb.metric.memory.prefetch-outcome-vector` | **Prefetch outcome vector**: useful-before-eviction, duplicate, unused, canceled, and failed prefetch. | R | Future loading | X | `+O(1)` |
| `#bb.metric.memory.prefetch-wasted-bytes` | **Prefetch wasted bytes**: loaded bytes never requested before eviction. | R | Future loading | X | `+O(1)` |
| `#bb.metric.memory.sparse-map-probe-event` | **Sparse-map probe event**: for each world-map get/entry/remove, present/missing result, control groups or buckets inspected, and key-equality comparisons. | H/R | Future instrumented map | X with hooks | `+O(probes)` |
| `#bb.metric.memory.tick-transaction-storage` | **Tick-transaction storage**: current and peak length, capacity, and attributed bytes for transaction edits, effects, and provenance updates from begin through commit/drop. | R/H | Inst | X with allocator/layout accounting | `O(1)` samples |
| `#bb.metric.memory.thread-stack-inventory` | **Thread-stack inventory**: per thread identity, reserved bytes, committed bytes, measured high-water bytes, and overflow/guard-page event. | H | Inst | S/X by platform | host query/probe |
| `#bb.metric.memory.sparse-map-layout-relation` | **Sparse-map layout relation**: map implementation/hasher identity plus every live chunk key's bucket/control-group and probe displacement, with empty/deleted bucket state where exposed. | R/H | Future instrumented map | X for snapshot | `O(capacity)` |
| `#bb.metric.memory.host-memory-availability-base-vector` | **Host-memory availability base vector**: physical total/available bytes, commit charge/limit, swap/pagefile availability, and applicable process/job/container limit. | H | Inst | S/X by platform | host sample |

## Host performance, CPU, allocation, and timing

`#bb.metric.perf.*` times operations owned by other domains without redefining their
semantic outputs. A closed phase label may include tick scheduling, language
parse, batch resolution/application, replay, proof, hashing, storage, sync, and
the view/world operations declared in their own files.

| Registry name | Metric and exact definition | Class | Avail | Exact | Cost |
|---|---|---:|---:|---:|---:|
| `#bb.metric.perf.phase-wall-duration-event` | **Phase wall duration event**: end monotonic timestamp minus start for a closed phase label. | H | Inst | X | two clock reads |
| `#bb.metric.perf.phase-cpu-duration-event` | **Phase CPU duration event**: user/kernel CPU consumed by a closed phase label. | H | Inst | S or OS-exact | OS counter reads |
| `#bb.metric.perf.tick-overrun-event` | **Tick overrun event**: `max(fixed_tick phase wall duration - nominal_tick_period, 0)`, using the `fixed_tick`-labeled `#bb.metric.perf.phase-wall-duration-event`. | H | Inst | X | `O(1)` |
| `#bb.metric.perf.work-unit-vector` | **Work-unit vector**: attempted/resolved edits, state transitions, chunks/words/coordinates scanned, hash bytes, storage bytes, sync bytes, and view-owned work units completed by phase. Typed replay exposes receipt, edit, and world-tag-coordinate-visit components; sequential verification exposes segment, iteration, injection, and covered-event totals. The compositor now exposes logical game-prefix words copied: a full composition copies `simulation_width * height` words, while a cached-frame-tag-verified sidebar-only recomposition copies zero. Source payload bytes and destination payload bytes are each words times four; their sum is a derived logical copy-event count, never hardware memory traffic, time, energy, or presentation work. | R | Now typed-replay/sequential subsets and compositor copy words; Inst remaining phases | X | counters `+O(1)` |
| `#bb.metric.perf.phase-allocation-vector` | **Phase allocation vector**: allocation/deallocation/reallocation calls and bytes occurring inside a phase. | H | Inst | X with hooks | allocator hooks |
| `#bb.metric.perf.temporary-phase-peak-bytes` | **Temporary phase peak bytes**: maximum heap live bytes above phase-entry baseline. | H | Inst | X with hooks | allocator hooks |
| `#bb.metric.perf.startup-milestone-timestamps` | **Startup milestone timestamps**: process entry, program loaded, app initialized, first tick, first view frame produced, and first presentation. | H | Inst | X | clock reads |
| `#bb.metric.perf.shutdown-duration` | **Shutdown duration**: close/exit request to process termination, under an external observer if needed. | H | Inst | X | clock/supervisor |
| `#bb.metric.perf.process-user-cpu-time` | **Process user CPU time**: cumulative user-mode CPU duration. | H | Inst | S | OS sample |
| `#bb.metric.perf.process-kernel-cpu-time` | **Process kernel CPU time**: cumulative kernel-mode CPU duration. | H | Inst | S | OS sample |
| `#bb.metric.perf.scheduler-context-switch-vector` | **Scheduler context-switch vector**: voluntary and involuntary process/thread switches. | H | Inst | S | OS sample |
| `#bb.metric.perf.core-migration-count` | **Core migration count**: observed movement of the measured thread between logical CPUs. | H | Inst | S | profiler/ETW |
| `#bb.metric.perf.hardware-cycle-count` | **Hardware cycle count**: CPU cycles over a declared phase/window. | H | Inst | S | performance counter |
| `#bb.metric.perf.retired-instruction-count` | **Retired instruction count**: instructions retired over the same declared phase/window. | H | Inst | S | performance counter |
| `#bb.metric.perf.branch-base-vector` | **Branch base vector**: branch instructions and mispredictions. Rates are views. | H | Inst | S | performance counter |
| `#bb.metric.perf.cache-miss-base-vector` | **Cache-miss base vector**: L1, L2, and last-level accesses/misses where available. | H | Inst | S | performance counter |
| `#bb.metric.perf.tlb-miss-vector` | **TLB-miss vector**: instruction/data translation misses by available level. | H | Inst | S | performance counter |
| `#bb.metric.perf.memory-transfer-bytes` | **Memory-transfer bytes**: hardware-observed memory traffic over a declared interval. Bandwidth is a view. | H | Inst | S | performance counter |
| `#bb.metric.perf.effective-cpu-frequency-event` | **Effective CPU frequency event**: measured clock during workload. | H | Inst | S | OS/hardware sample |
| `#bb.metric.perf.throttle-state-event` | **Throttle-state event**: thermal/power throttle reason and duration/cycles. | H | Inst | S | hardware sample |
| `#bb.metric.perf.energy-base-sample` | **Energy base sample**: package/process joules across a declared tick interval. Energy per tick is a view. | H | Inst | S | hardware sample |
| `#bb.metric.perf.process-i-o-vector` | **Process I/O vector**: OS-reported read/write operation and byte counters. | H | Inst | S | OS sample |
| `#bb.metric.perf.profiler-attribution-vector` | **Profiler attribution vector**: samples assigned to closed Bitblock phases versus unknown/external work. | H | Inst | S | profiler |
| `#bb.metric.perf.instrumentation-overhead-assay` | **Instrumentation overhead assay**: matched instrumented and baseline work/time/allocation base vectors. Differences/ratios are views. | A | Inst | S | paired benchmark |
| `#bb.metric.perf.benchmark-identity` | **Benchmark identity**: exact binary/source/toolchain, input log, machine profile, warmup, repetitions, and affinity/power settings. | A | Inst | X | manifest `O(1)` |
| `#bb.metric.perf.measurement-loss-event` | **Measurement loss event**: dropped timer/allocation/profiler samples and reason. | H | Inst | X where detectable | `+O(1)` |
| `#bb.metric.perf.build-artifact-byte-vector` | **Build artifact byte vector**: exact on-disk bytes by executable section/category (code, read-only data, writable data, unwind, symbols/debug, padding) for the `#bb.metric.perf.benchmark-identity` build identity. | A | Inst | X | binary inspection |
| `#bb.metric.perf.reachable-tick-render-code-bytes` | **Reachable tick/render code bytes**: machine-code bytes reachable from declared fixed-tick and render entry points under one linker-map/call-graph method, retaining unattributed bytes. | A | Inst | S | linker map/static analysis |
| `#bb.metric.perf.hash-internal-work-vector` | **Hash internal work vector**: absorbed symbols/blocks, permutation rounds, gate/S-box applications, state-word mixes, squeezes, and temporary bytes for each hash-domain event. | R | Future hash | X with counters | `+O(1)` per internal event |
| `#bb.metric.perf.iota-host-calibration-observations` | **Iota/host calibration observations**: per canonical term, exact iota leaf/tick cost paired with executed beta steps, CPU/wall duration, retired instructions, and output identity. Fits are sampled views and never redefine iota cost. | A/H | Future bridge | X for pairs, S for fits | corpus benchmark |
| `#bb.metric.perf.compiler-size-attribution-vector` | **Compiler size-attribution vector**: bytes attributed to duplicate monomorphizations, inline clones, dead-stripped sections, identical-code folding, unwind tables, padding, and unattributed remainder under a declared toolchain. | A | Inst | S | compiler/linker reports |
| `#bb.metric.perf.source-corpus-bytes` | **Source corpus bytes**: exact bytes by declared first-party source/proof/docs category after explicit generated/vendor/target exclusions; token, line, and ratio summaries are views. | A | Inst | X | `O(source_bytes)` |
| `#bb.metric.perf.canonical-world-ordering-work-event` | **Canonical-world ordering-work event**: coordinates buffered, sort comparisons, swaps/moves, scratch peak bytes, and coordinates emitted for one world-tag computation. | R/H | Inst | X with counters | `O(N log N)` |
| `#bb.metric.perf.monotonic-clock-contract` | **Monotonic-clock contract**: source/API identity, measured effective resolution, zero-delta sample count, and backward/nonfinite observation witnesses for the clock underlying duration metrics. | H/A | Inst | S | calibration plus guarded reads |
| `#bb.metric.perf.host-cpu-pressure-base-vector` | **Host CPU-pressure base vector**: system runnable-thread/run-queue count and CPU time partitioned busy, idle, interrupt/DPC, and steal where available over the declared interval. | H | Inst | S/X by platform | host samples |

## Persistence, checkpoints, and recovery

Apart from reading an optional startup `.bb` file, Bitblock has no persistence;
unimplemented values are `N/A`.

| Registry name | Metric and exact definition | Class | Avail | Exact | Cost |
|---|---|---:|---:|---:|---:|
| `#bb.metric.store.startup-source-identity` | **Startup source identity**: supplied path annotation plus canonical source digest when available. The path is provenance, not deterministic content identity. | R | Now/path; Future/hash | X | file/hash cost |
| `#bb.metric.store.startup-read-outcome` | **Startup read outcome**: bytes returned or exact OS/UTF-8 failure reason. | H | Now | X | file read |
| `#bb.metric.store.snapshot-sequence` | **Snapshot sequence**: monotonic snapshot identifier. | D | Future | X | `O(1)` |
| `#bb.metric.store.snapshot-tick` | **Snapshot tick**: exact completed tick represented by the snapshot. | D | Future | X | `O(1)` |
| `#bb.metric.store.snapshot-schema-identity` | **Snapshot schema identity**: format and canonical-encoding version. | D | Future | X | `O(1)` |
| `#bb.metric.store.snapshot-state-root` | **Snapshot state root**: `#bb.metric.hash.canonical-world-state-root` committed by the snapshot. | D | Future | X | root cost |
| `#bb.metric.store.snapshot-logical-bytes` | **Snapshot logical bytes**: canonical uncompressed payload length. | D | Future | X | `O(B)` serialization |
| `#bb.metric.store.snapshot-physical-bytes` | **Snapshot physical bytes**: actual stored allocation/file bytes, kept distinct from logical bytes. | H | Future | X/S by filesystem | filesystem query |
| `#bb.metric.store.snapshot-state-summary` | **Snapshot state summary**: referenced `#bb.metric.world.*` cardinality and chunk-state tags stored and verified with the snapshot. | D | Future | X | `O(1)` after summary |
| `#bb.metric.store.snapshot-age-base-pair` | **Snapshot age base pair**: current tick minus snapshot tick, and host now minus durable completion. | D/H | Future | X | `O(1)` |
| `#bb.metric.store.dirty-transition-count` | **Dirty transition count**: state-changing edits since the latest durable snapshot. | D | Future | X | `+O(1)` |
| `#bb.metric.store.dirty-chunk-coordinate-count` | **Dirty chunk-coordinate count**: distinct changed chunk addresses since checkpoint; not chunk occupancy. | D | Future | X | expected incremental |
| `#bb.metric.store.save-outcome` | **Save outcome**: committed or failed with exact serialize/compress/write/fsync/rename reason. | H | Future | X | save operation |
| `#bb.metric.store.atomic-replacement-check` | **Atomic replacement check**: no partially written payload can appear at the final snapshot identity. | A | Future | X | fault-injection audit |
| `#bb.metric.store.orphan-temporary-artifact-vector` | **Orphan temporary artifact vector**: incomplete save files discovered, recovered, quarantined, or removed. | H | Future | X | directory scan |
| `#bb.metric.store.load-outcome` | **Load outcome**: accepted snapshot or exact open/read/decompress/decode/schema/root/invariant failure. | H/A | Future | X | load/verify |
| `#bb.metric.store.snapshot-root-verification` | **Snapshot root verification**: decoded exact state recomputes the committed root. | A | Future | X | decode plus root |
| `#bb.metric.store.snapshot-round-trip-equality` | **Snapshot round-trip equality**: save then load yields the identical exact finite world and metadata. | A | Future | X | save/load plus `O(N)` |
| `#bb.metric.store.corruption-location` | **Corruption location**: first byte/record/chunk at which decode or verification can localize failure. The physical scanner reports byte/record position; typed replay reports semantic record index and reason, which need not identify a byte. | A | Now bounded scanner, journal, and typed replay | X | verification scan |
| `#bb.metric.store.journal-open-outcome` | **Journal open outcome**: distinct create-new or open-existing operation, platform, acquired/refused I/O phase, observed file bytes, and physically ready or needs-recovery state. A path is an annotation, not content identity; semantic history admission is a separate metric. | H/A | Now research journal | X for local call | open plus scan |
| `#bb.metric.store.exclusive-writer-handle-outcome` | **Exclusive-writer handle outcome**: target platform, requested sharing contract, acquired/refused result, and retained-handle lifetime. This records Windows `share_mode(0)` enforcement, not permanent custody. | H/A | Now Windows research journal | X for observed opens | OS open |
| `#bb.metric.store.verified-journal-prefix-vector` | **Verified journal prefix vector**: total file bytes, physically admitted envelope/hash prefix bytes, admitted record count, admitted storage-record head tag, and exact first carrier refusal when the prefix is incomplete. It makes no semantic-object claim. | D/A | Now research journal | X under decoder/hash assumptions | `O(file bytes)` |
| `#bb.metric.store.journal-rejection-class` | **Journal rejection class**: structural/authentication failure, local policy limit, or unsupported protocol, paired with the exact decoder error and first refused offset. Only the first class is eligible for explicit suffix discard. | A | Now research journal | X | `O(1)` after scan |
| `#bb.metric.store.coordinate-history-admission-outcome` | **Coordinate-history admission outcome**: physical prefix identity/count, caller-supplied initial-world tag, exact six-value local replay-limit tuple, and one of `accepted`, `semantic-rejected`, or `local-policy-refused`, with first record/reason. Acceptance carries the derived world/history/provenance heads plus receipt/edit totals, transient occupancy/chunk peaks, and world-tag coordinate visits. Policy refusal can precede complete semantic validation, is retryable under a larger explicit policy, does not classify bytes as corrupt, and never authorizes tail deletion. | D/A | Now protocol-research typed journal | X under declared codecs/hash | semantic replay work |
| `#bb.metric.store.synced-append-evidence` | **Synced append evidence**: parent, object, and record tags; start/end offsets; encoded bytes; successful `sync_all` return; and byte-for-byte comparison of the canonical candidate record with the same-length OS-visible appended range after sync, retaining equality or the first differing byte. It is OS evidence, not a power-loss guarantee. | D/H/A | Now research journal | X for local calls and OS-visible bytes | encode, write, sync, reread |
| `#bb.metric.store.journal-poison-transition` | **Journal poison transition**: candidate record tag, failing carrier append phase, start/expected-end offsets, accepted byte prefix, pre-head retained, and refusal of later appends until reopen. Semantic publication poisoning is owned separately; direct typed I/O-phase fault injection is not yet powered. | H/A | Now research journal | X for carrier-instrumented failures | `O(1)` plus failure injection |
| `#bb.metric.store.coordinate-history-publication-gate-outcome` | **Coordinate-history publication-gate outcome**: candidate tick/receipt/post-world, pre/post semantic world/history/usage heads, referenced carrier append result, poison state, and exact disposition: empty batch, semantic/preflight refusal, replay-policy refusal before I/O, carrier failure/uncertainty, synchronized-and-reread carrier followed by complete semantic publication, or post-carrier internal tag mismatch with no publication. It proves local publication ordering, not power-loss survival; current tag-mismatch checks sharing one Rust encoder are consistency guards rather than independent evidence. | D/H/A | Now protocol-research typed journal | X for observed local path | candidate replay plus carrier append |
| `#bb.metric.store.rejected-tail-export-verdict` | **Rejected-tail export verdict**: verified-prefix and total offsets, exact exported suffix bytes/length, suffix tag, and rehash equality while the exclusive handle remains retained. | A | Now research journal | X | `O(rejected bytes)` |
| `#bb.metric.store.explicit-tail-discard-outcome` | **Explicit tail-discard outcome**: prior bytes, retained bytes/head, discarded bytes/tag/error, set-length and sync phases, and exact retained-prefix rescan equality or first failure. Policy-limit and unsupported-protocol tails are refused. | H/A | Now research journal | X for local operation | truncate, sync, `O(prefix bytes)` verify |
| `#bb.metric.store.no-resynchronization-verdict` | **No-resynchronization verdict**: first refused boundary/error, remaining suffix bytes, later valid-magic presence, and whether any later record was admitted. Current scanner requires zero later admissions. | A | Now storage/journal controls | X over scanned bytes and cut corpus | `O(prefix)` |
| `#bb.metric.store.local-rollback-detectability-boundary` | **Local rollback-detectability boundary**: boundary-truncated or alternate canonical bytes, unanchored clean/failed result, required ancestor tag, ancestor-presence result, and candidate head. It records that locally clean EOF alone cannot prove latest history. | A | Now journal and typed-history negative controls | X over supplied chains/anchor | `O(records)` |
| `#bb.metric.store.eof-consistency-verdict` | **EOF consistency verdict**: tracked verified offset versus handle metadata before append, and expected versus observed EOF after sync, with exact failing phase. | H/A | Now research journal | X for handle observations | metadata queries |
| `#bb.metric.store.retention-inventory` | **Retention inventory**: retained snapshot tags, ticks, roots, and physical bytes. | R | Future | X | `O(snapshots)` |
| `#bb.metric.store.content-reuse-bytes` | **Content reuse bytes**: logical bytes avoided by verified content-addressed deduplication. | H | Future | X | dedup index |
| `#bb.metric.store.physical-write-bytes` | **Physical write bytes**: bytes reaching the storage stack for declared logical changes. Write amplification is a view. | H | Future | X/S | OS/device counters |
| `#bb.metric.store.physical-read-bytes` | **Physical read bytes**: bytes read for declared logical state requests. Read amplification is a view. | H | Future | X/S | OS/device counters |
| `#bb.metric.store.compaction-event` | **Compaction event**: input bytes/records, output bytes/records, and reclaimed bytes for one compaction. | H | Future | X | compaction accounting |
| `#bb.metric.store.filesystem-allocation-gap` | **Filesystem allocation gap**: allocated filesystem bytes minus logical file bytes. | H | Future | S | filesystem query |
| `#bb.metric.store.available-storage-bytes` | **Available storage bytes**: free bytes on the target volume. Exhaustion forecasts are sampled views. | H | Future | S | OS sample |
| `#bb.metric.store.wal-sequence-head` | **WAL sequence head**: latest appended durable log sequence. | D | Future | X | `O(1)` |
| `#bb.metric.store.uncheckpointed-wal-length` | **Uncheckpointed WAL length**: records after the selected snapshot boundary. | D | Future | X | `O(1)` maintained |
| `#bb.metric.store.unflushed-wal-count` | **Unflushed WAL count**: accepted records not confirmed durable. | R | Future | X | `O(1)` maintained |
| `#bb.metric.store.wal-sequence-anomaly-vector` | **WAL sequence anomaly vector**: gaps, duplicates, reversals, and conflicting records. | A | Future | X | `O(records)` |
| `#bb.metric.store.recovery-replay-work` | **Recovery replay work**: records/edits applied after the chosen snapshot. Typed history currently replays from genesis and exposes records, edits, and world-tag coordinate visits; snapshots remain future. | D | Now typed history from genesis | X | `O(recovery records + world-tag work)` |
| `#bb.metric.store.recovery-outcome` | **Recovery outcome**: locally synchronized/admitted carrier head plus typed semantic heads and counts or bytes: world/history for coordinate history, or stored-object/display-artifact/display-receipt for display evidence. The display journal exactly dispatches both legacy DisplayArtifact v1 records and PresentationOutput v1 records, and all four adjacent legacy/wrapper transitions replay through one physical storage-parent chain and one embedded display-receipt chain. Wrapper inventory retains the outer object, nested display, timing, before/after topology tags, and derived route stability. Recovery also retains exact physical-tail classification, first indexed semantic failure, or local-policy failure; explicit discard relies on the carrier's exact retained-prefix recheck. This is not a sudden-power, latest-branch, or external-custody claim; snapshot joining remains future. | A | Now generic, typed-history, and mixed typed-display research journals | X under local assumptions | recovery operation plus typed replay |
| `#bb.metric.store.recovered-transition-vector` | **Recovered transition vector**: locally admitted records, replayed edits, state changes, and valid no-ops, including reconstructed mint/burn provenance. “Recovered” does not mean externally anchored or power-loss durable. | D | Now typed history replay | X under replay semantics | recovery replay |
| `#bb.metric.store.rejected-tail-vector` | **Rejected tail vector**: verified offset/head, total/rejected bytes, first error/class, and full suffix tag for bytes excluded from the writable prefix. | A | Now research journal | X | tail scan/hash |
| `#bb.metric.store.durability-lag-base-pair` | **Durability lag base pair**: live tick minus durable tick and host time since last durable completion. | R/H | Future | X | `O(1)` |
| `#bb.metric.store.migration-path` | **Migration path**: ordered schema-version edges actually applied. | A | Future | X | `O(migrations)` |
| `#bb.metric.store.migration-outcome` | **Migration outcome**: final version/root or first failed edge and exact reason. | A | Future | X | migration operation |
| `#bb.metric.store.migration-semantic-equality` | **Migration semantic equality**: migrated legacy payload yields the declared expected canonical state root. | A | Future | X | migrate plus root |
| `#bb.metric.store.import-export-event` | **Import/export event**: format identity, direction, logical bytes, result, and resulting root. | R/A | Future | X | conversion operation |
| `#bb.metric.store.backup-restore-verification` | **Backup restore verification**: backup was actually restored, decoded, and root-checked at a declared tick/time. | A | Future | X | restore assay |
| `#bb.metric.store.receipt-object-persistence-inventory` | **Receipt/object persistence inventory**: `(content tag, role, present bytes, physical/logical length, digest verdict, durable status)` for every object referenced by the selected head. Unlike reference closure, this measures physical availability. | R/A | Future store | X | `O(objects + bytes_hashed)` |
| `#bb.metric.store.append-only-mutation-outcome-vector` | **Append-only mutation outcome vector**: observed new append, identical duplicate, conflicting overwrite, delete, truncate, torn append, and sync failure with target tag/boundary. Exact only for the instrumented or audited storage scope. | H/A | Now for research-journal append/torn/sync/discard subset; Future external mutations | X observed scope | `+O(1)` hooked or store scan |
| `#bb.metric.store.crash-cutpoint-recovery-matrix` | **Crash-cutpoint recovery matrix**: for every declared persistence mutation cutpoint and injected I/O failure, process stop, or power cut, physical bytes, recovered head/root, and previous-commit/new-commit/rejected/other outcome. | A | Now for every append-byte cut and I/O failpoint model; Future process/power assay | X over declared matrix | persistence protocol times cutpoints |
| `#bb.metric.store.snapshot-wal-join-verdict` | **Snapshot/WAL join verdict**: selected snapshot head/tick and first retained WAL predecessor/sequence with overlap, gap, conflict, or compatible classification and first incompatible record. | A | Future recovery | X | decode plus boundary verification |
| `#bb.metric.store.compaction-semantic-equivalence` | **Compaction semantic equivalence**: for every retained authoritative head, exact recovered head/root before and after compaction with first divergent record/object. | A | Future compaction assay | X | paired recovery |
| `#bb.metric.store.recovery-idempotence` | **Recovery idempotence**: from one frozen crash image, compare bytes/head/root after first recovery with a second recovery performed without new accepted writes. | A | Future recovery assay | X | two recoveries |
| `#bb.metric.store.storage-environment-contract` | **Storage-environment contract**: target volume/device/filesystem identity, logical/physical block sizes, mount/cache mode, and observed or declared atomic-write, rename, flush, and directory-durability capabilities with unavailable fields explicit. | H/A | Future store | S/X by source | environment query plus assay |

## Future synchronization and networking

Networking is absent. These metrics describe observation only; wall clocks,
latency, peer counts, or bandwidth cannot become hidden simulation authority.

| Registry name | Metric and exact definition | Class | Avail | Exact | Cost |
|---|---|---:|---:|---:|---:|
| `#bb.metric.sync.connection-state-vector` | **Connection state vector**: attempting, connected, authenticated, ready, closing, and failed peers. | H | Future | X | maintained `O(1)` |
| `#bb.metric.sync.connection-outcome-vector` | **Connection outcome vector**: success or exact DNS/socket/TLS/auth/protocol failure reason. | H | Future | X | per attempt |
| `#bb.metric.sync.session-identity-count` | **Session identity/count**: logical synchronization sessions and their declared protocol identities. | R | Future | X | maintained `O(1)` |
| `#bb.metric.sync.protocol-version-distribution` | **Protocol-version distribution**: peers by negotiated version/feature set. | R | Future | X | `O(P)` |
| `#bb.metric.sync.incompatibility-vector` | **Incompatibility vector**: rejected peers by version, encoding, hash, rule, or capability mismatch. | R | Future | X | `+O(1)` |
| `#bb.metric.sync.handshake-outcome` | **Handshake outcome**: completed identity/capability/root exchange or exact failing stage. | H/A | Future | X | handshake |
| `#bb.metric.sync.round-trip-event` | **Round-trip event**: request tag, send/receive timestamps, and elapsed host duration. | H | Future | X | clock reads |
| `#bb.metric.sync.transport-sequence-vector` | **Transport sequence vector**: sent, acknowledged, missing, retransmitted, duplicated, and out-of-order units. | H | Future | X where sequenced | maintained counters |
| `#bb.metric.sync.wire-byte-vector` | **Wire byte vector**: sent/received canonical payload, framing, encryption, and total wire bytes. | H | Future | X/S by layer | counters |
| `#bb.metric.sync.message-vector` | **Message vector**: sent/received counts and bytes by closed protocol message type. | H | Future | X | `+O(1)` |
| `#bb.metric.sync.network-compression-base-pair` | **Network compression base pair**: canonical payload bytes and transmitted compressed payload bytes. Ratio is a view. | H | Future | X | `+O(1)` |
| `#bb.metric.sync.batch-transfer-vector` | **Batch transfer vector**: exact tick batches/edits sent, received, accepted, rejected, and duplicated. | R | Future | X | `+O(1)` |
| `#bb.metric.sync.inbound-outbound-queue-base-vector` | **Inbound/outbound queue base vector**: queued message and byte counts. | H | Future | X | maintained `O(1)` |
| `#bb.metric.sync.sequence-anomaly-vector` | **Sequence anomaly vector**: missing tags, duplicates, reversals, and conflicting payloads for one tag. | R | Future | X | expected `O(messages)` |
| `#bb.metric.sync.rejected-message-vector` | **Rejected-message vector**: malformed, oversized, invalid hash/signature, stale, unauthorized, rate-limited, or unsupported. | R | Future | X | `+O(1)` |
| `#bb.metric.sync.verified-remote-tick-head` | **Verified remote tick head**: highest transition boundary locally verified for each peer. | D | Future | X | verification-dependent |
| `#bb.metric.sync.peer-tick-skew` | **Peer tick skew**: signed local verified head minus remote verified head. | R | Future | X | `O(1)` per peer |
| `#bb.metric.sync.confirmation-lag-base-pair` | **Confirmation lag base pair**: ticks and host duration from local batch creation to declared confirmation. | R/H | Future | X | timestamps/counters |
| `#bb.metric.sync.state-root-report-vector` | **State-root report vector**: peer, tick, root, receipt head, and verification result. | D/A | Future | X | `+O(1)` plus verify |
| `#bb.metric.sync.common-root-partition` | **Common-root partition**: peers grouped by verified root at the same tick. | D | Future | X | expected `O(P)` |
| `#bb.metric.sync.first-distributed-divergence` | **First distributed divergence**: earliest common tick with more than one verified root. | A | Future | X | history scan |
| `#bb.metric.sync.divergent-peer-set` | **Divergent-peer set**: peers outside a declared agreed-root set at a tick; no automatic blame is implied. | A | Future | X | `O(P)` |
| `#bb.metric.sync.receipt-head-agreement` | **Receipt-head agreement**: verified peer receipt heads at the same committed sequence. | D/A | Future | X | `O(P)` |
| `#bb.metric.sync.distributed-receipt-topology` | **Distributed receipt topology**: forks, gaps, duplicates, and orphans discovered across peers. | A | Future | X | expected `O(receipts)` |
| `#bb.metric.sync.verified-remote-prefix-length` | **Verified remote-prefix length**: longest peer receipt prefix locally replayed/verified. | A | Future | X | prefix verification |
| `#bb.metric.sync.snapshot-sync-base-vector` | **Snapshot sync base vector**: expected, requested, in-flight, received, decoded, verified, and missing chunks/bytes. | R | Future | X | maintained counters |
| `#bb.metric.sync.local-satisfaction-event` | **Local satisfaction event**: requested content supplied from local verified cache rather than transport. | R | Future | X | `+O(1)` |
| `#bb.metric.sync.initial-sync-boundary` | **Initial-sync boundary**: session start and first verified common root. | H/A | Future | X | timestamps/verify |
| `#bb.metric.sync.full-sync-boundary` | **Full-sync boundary**: session start and attainment of the declared target head/root. | H/A | Future | X | timestamps/verify |
| `#bb.metric.sync.speculative-correction-event` | **Speculative correction event**: predicted transition replaced by verified authority, with exact old/new roots. | R | Future | X | correction operation |
| `#bb.metric.sync.rollback-event` | **Rollback event**: tick/edit depth reverted and checkpoint/replay work used. | R | Future | X | rollback operation |
| `#bb.metric.sync.conflict-event` | **Conflict event**: concurrent exact edits enter a declared deterministic conflict rule. | D | Future | X | protocol-dependent |
| `#bb.metric.sync.conflict-outcome-vector` | **Conflict outcome vector**: accepted, rejected, ordered, or merged under the declared rule. | D | Future | X | protocol-dependent |
| `#bb.metric.sync.commit-index` | **Commit index**: highest transition sequence durably agreed under the selected protocol. | D | Future | X | protocol-maintained |
| `#bb.metric.sync.commit-lag` | **Commit lag**: proposed/local verified head minus committed head. | R | Future | X | `O(1)` |
| `#bb.metric.sync.quorum-base-vector` | **Quorum base vector**: eligible verified participants and required threshold. Attainment is a view. | R | Future | X | `O(P)` |
| `#bb.metric.sync.authority-change-event` | **Authority-change event**: old/new leader or authority identity and committed boundary, if the protocol has one. | R | Future | X | protocol event |
| `#bb.metric.sync.peer-clock-offset-estimate` | **Peer clock-offset estimate**: remote minus local host clock under the declared estimator. | H | Future | S | message samples |
| `#bb.metric.sync.abuse-control-vector` | **Abuse-control vector**: rate-limited, oversized, unauthenticated, quarantined, and banned traffic/peers. | H | Future | X | maintained counters |
| `#bb.metric.sync.authentication-failure-vector` | **Authentication failure vector**: unknown identity, invalid signature, expired credential, unsupported algorithm, or replayed challenge. | H/A | Future | X | verification |
| `#bb.metric.sync.stake-position-relation` | **Stake-position relation**: each canonical `(position/provenance tag, owner key, delegated validator, amount, bond/activation/unbond/withdraw boundaries, eligibility verdict/reason)`. Totals, concentration, graph, and churn are views. | D | Future PoS | X with complete state | `O(positions)`, `+O(1)` per event |
| `#bb.metric.sync.stake-weighted-quorum-operands` | **Stake-weighted quorum operands**: `(validator-set tag, total eligible stake, valid participating stake, required stake)` per proposal, vote, or certificate. Attainment, intersection bound, and Byzantine margin are views. | D/A | Future PoS | X | `O(signers + stake lookups)` |
| `#bb.metric.sync.finality-certificate-outcome` | **Finality-certificate outcome**: target head/boundary, validator-set/policy tags, unique valid signer stake, threshold, and accept/reject reason. | D/A | Future PoS | X | `O(signatures + stake lookups)` |
| `#bb.metric.sync.conflicting-finality-witness` | **Conflicting-finality witness**: pair of nonancestor heads for one consensus boundary, each carrying a valid finality certificate. An empty observed set is not a universal impossibility proof. | A | Future PoS | X over retained certificates | `O(certificates + ancestry_queries)` |
| `#bb.metric.sync.validator-equivocation-evidence-relation` | **Validator-equivocation evidence relation**: `(validator key, boundary, conflicting signed message/head tags, evidence verdict)`. Nothing-at-stake and fork-signing totals are views. | D/A | Future PoS | X over retained messages | expected `O(votes)` indexed |
| `#bb.metric.sync.slashing-disposition-relation` | **Slashing-disposition relation**: `(evidence tag, validator/stake position, declared penalty, proposed\|accepted\|rejected\|executed status, effective boundary)`. Slashed stake totals are views. | D | Future slashing | X | `O(evidence + actions)` |
| `#bb.metric.sync.weak-subjectivity-checkpoint-relation` | **Weak-subjectivity checkpoint relation**: `(checkpoint tag, externally trusted source tag, validator-set tag, finalized head, verification verdict, local descendant relation)`. Age is a view. | D/A | Future PoS/checkpoints | X except trust choice | `O(checkpoints + verification)` |
| `#bb.metric.sync.long-range-exposure-operands` | **Long-range exposure operands**: for each checkpoint, historically authorized stake since unbonded/withdrawn, validator-set transitions, and signed checkpoints covering the interval. It measures protocol exposure, not old-key compromise. | D | Future PoS history | X with complete history | `O(transitions since checkpoint)` |
| `#bb.metric.sync.stake-selection-event-relation` | **Stake-selection event relation**: `(decision boundary, randomness/seed tag, eligible-set tag, selected proposer/committee tags, selection-proof verdict)`. Frequency and bias are views. | D/A | Future PoS | X per event | `O(selection_proof + committee_size)` |
| `#bb.metric.sync.selection-grinding-evidence-vector` | **Selection-grinding evidence vector**: observably attempted, discarded, and published candidate randomness/seed values with resulting selections for a declared actor/boundary. Absence proves nothing. | R/A | Future PoS instrumentation | X observed trials only | `O(observed_trials)` |
| `#bb.metric.sync.validator-set-epoch-relation` | **Validator-set epoch relation**: per effective boundary, epoch/rule tag, validator-set tag, sorted validator keys/weights, activation/deactivation source events, and eligibility policy/verdict. | D | Future PoS | X with complete state | `O(validators)` per transition |
| `#bb.metric.sync.consensus-message-admission-relation` | **Consensus-message admission relation**: every proposal, vote, or certificate contribution records message tag/type, height/round/epoch, parent/head, signer, signature verdict, validator-set/policy tag, and admitted/rejected reason. | D/A | Future consensus | X | `O(messages)` |
| `#bb.metric.sync.fork-choice-decision-relation` | **Fork-choice decision relation**: each decision records policy tag, complete candidate-head set and ancestry/support operands, selected head, rejected heads, and deterministic witness/verdict. | D/A | Future consensus | X | policy-dependent |
| `#bb.metric.sync.stake-asset-transition-relation` | **Stake-asset transition relation**: canonical scarce-asset issue, transfer, bond, unbond, reward, fee, burn, and slash events with source/destination, amount, authorization, and pre/post supply roots. | D/A | Future PoS asset | X with complete history | `O(events)` |
| `#bb.metric.sync.data-availability-obligation-relation` | **Data-availability obligation relation**: per proposed/committed head, required object/chunk tags, coding/replication policy, required witness, provider/certificate evidence, retrieval attempts, and available/unavailable/untested outcome. | D/A/H | Future distributed store | X for records, S for network observation | `O(obligations + attempts)` |
| `#bb.metric.sync.protocol-governance-decision-relation` | **Protocol-governance decision relation**: proposal tag, proposed code/rules/schema/hash/consensus tags, voter/authorization policy, approvals, outcome, activation boundary, and observed activated configuration tag. | D/A | Future governance | X with complete decisions | `O(decisions + votes)` |
| `#bb.metric.sync.consensus-transition-replay-outcome` | **Consensus-transition replay outcome**: replay canonical consensus inputs from genesis and compare validator set, height/round, locks/justifications, fork-choice head, finalized head, and penalties with first divergent input/field. | A | Future consensus | X | replay cost |
| `#bb.metric.sync.observed-network-reachability-relation` | **Observed-network reachability relation**: reporting node, peer node, authenticated session/reachability state, and evidence boundary/timestamp/tag as directed observed edges. Components and partitions are views. | H/A | Future distributed reports | S over observations | `O(reports)` |
| `#bb.metric.sync.pending-signed-intent-pool-relation` | **Pending signed-intent pool relation**: signer, parent/nonce/expiry, resource/priority policy, first-seen boundary, pool/propagation state, and inclusion/rejection/expiry outcome for every admitted intent. | D/H | Future consensus mempool | X locally, S network-wide | `O(intents)` |
| `#bb.metric.sync.deterministic-resource-admission-relation` | **Deterministic resource-admission relation**: per intent/receipt, canonical integer charges and limits for decoded bytes, edits, state reads/writes, new storage, proof bytes, and verification work with pinned policy and accept/reject reason. Typed history exposes an untagged local six-limit replay-policy/usage subset. Sequential verification separately exposes local limits for segment count, iterations per segment/total, and injections per segment/total; isolated codecs use a distinct two-field segment policy. InteractionTape v1 adds local payload/record/action and possible-chunk caps plus a pre-mutation abstract interpreter charging each resize and cumulative composition, raster-candidate, world-coordinate, sparse-chunk-word, edit-coordinate, and canonical world-ordering work. These host-local policies are not pinned protocol authority. | D/A | Now untagged local typed-history/sequential/InteractionTape subsets; Future pinned authority path | X locally | metering cost |
| `#bb.metric.sync.randomness-generation-relation` | **Randomness-generation relation**: per selection seed, policy/function tag, prior finalized entropy, eligible VRF/commit-reveal/contribution inputs and verdicts, observed withheld/rejected inputs, canonical mixing order, and final seed tag. | D/A | Future randomness beacon | X for retained inputs | `O(contributions)` |
| `#bb.metric.sync.consensus-round-transition-relation` | **Consensus-round transition relation**: every height/round/prior-state to next-state or height-advance transition with canonical trigger tag, pinned policy/deadline basis, and transition verdict. Host time cannot become an implicit authority input. | D/A | Future consensus | X | `O(transitions)` |
| `#bb.metric.sync.threat-model-obligation-relation` | **Threat-model obligation relation**: versioned protected property, adversary capability/resources/corruption/partition/custody powers, protocol assumption, mitigation/predicate, evidence tags, and residual/out-of-scope status. | A | Future protocol release manifest | X over declared model | `O(obligations)` |

## System correctness and health

Health metrics report failures and freshness of predicates defined above. They
do not duplicate the underlying world/view measurements. View-specific health
(camera finiteness, aim, projection, collision clearance, framebuffer results)
belongs to `view.md`; this system may only aggregate its reported verdicts.

| Registry name | Metric and exact definition | Class | Avail | Exact | Cost |
|---|---|---:|---:|---:|---:|
| `#bb.metric.health.invariant-verdict-vector` | **Invariant verdict vector**: pass/fail/not-checked for each declared `#bb.metric.proof.*`, `#bb.metric.world.*`, and `#bb.metric.view.*` predicate. | R/A | Inst | X | referenced check costs |
| `#bb.metric.health.invariant-failure-event` | **Invariant failure event**: predicate tag, first failing tick/artifact, observed witness, and reason. | R/A | Inst | X | `+O(1)` after check |
| `#bb.metric.health.invariant-freshness` | **Invariant freshness**: last successful and last attempted tick/artifact for each predicate. | R/A | Inst | X | `+O(1)` |
| `#bb.metric.health.metric-reconciliation-failure-vector` | **Metric reconciliation failure vector**: violated identities such as `attempts=resolved+misses`, cached counts, or chain continuity. | A | Inst | X | identity-dependent |
| `#bb.metric.health.panic-event` | **Panic event**: panic payload/location and tick/phase when observable. Abort-mode panics require a supervisor. | H | Inst | X | hook `+O(1)` |
| `#bb.metric.health.process-exit-event` | **Process exit event**: clean/abnormal exit code, signal/exception, and last verified tick/root under a supervisor. | H | Future supervisor | X | supervisor |
| `#bb.metric.health.restart-event` | **Restart event**: prior exit identity, new process identity, and recovery boundary. | H | Future supervisor | X | supervisor |
| `#bb.metric.health.uptime-base-timestamp` | **Uptime base timestamp**: monotonic process start; elapsed uptime is a view. | H | Now | X | `O(1)` |
| `#bb.metric.health.system-error-vector` | **System error vector**: closed subsystem, operation, error kind, OS code, and recoverable/fatal disposition. | H | Inst | X | `+O(1)` |
| `#bb.metric.health.unknown-error-event` | **Unknown error event**: a failure cannot map to a declared closed reason without discarding its original detail. | H | Inst | X | `+O(1)` |
| `#bb.metric.health.diagnostic-record-loss-event` | **Diagnostic record-loss event**: records/samples dropped and exact buffer/backpressure/shutdown reason. | H | Inst | X where detectable | `+O(1)` |
| `#bb.metric.health.metric-availability-vector` | **Metric availability vector**: available, instrumentation-disabled, permission-denied, unsupported-platform, stale, or future-subsystem-absent. | R | Inst | X | `O(metrics)` |
| `#bb.metric.health.metric-refresh-boundary` | **Metric refresh boundary**: last tick and host timestamp at which each value was recomputed. Staleness is a view. | R/H | Inst | X | `+O(1)` |
| `#bb.metric.health.metric-recorder-overhead-base-vector` | **Metric recorder overhead base vector**: CPU time, allocations, live bytes, and dropped records attributable to telemetry. | H | Inst | S/X by source | instrumentation |
| `#bb.metric.health.tick-overflow-guard` | **Tick overflow guard**: tick counter checked-add failure or crossing a declared headroom threshold. | D | Now/Inst alert | X | `O(1)` |
| `#bb.metric.health.world-length-arithmetic-guard` | **World-length arithmetic guard**: inability to represent cached cardinality in target `usize`; cardinality itself is `#bb.metric.world.*`. | R | Inst | X | `O(1)` |
| `#bb.metric.health.container-capacity-guard-vector` | **Container-capacity guard vector**: pending edits, batches, logs, maps, receipts, and encoded buffers cannot reserve/represent requested length. Typed replay guards receipt/edit/work counter arithmetic; the sequential codec checks exact count-derived length and fallible injection reservation before admission. Other container allocation failures remain instrumented/future. | R/H | Now typed-replay/sequential subsets; Inst elsewhere | X | per growth |
| `#bb.metric.health.coordinate-mapping-overflow-check` | **Coordinate mapping overflow check**: split/join intermediate arithmetic remains representable for exact `i64` coordinates. | A | Now in boundary tests | X | `O(cases)` |
| `#bb.metric.health.bounds-volume-representation-guard` | **Bounds-volume representation guard**: any limited-width consumer reports overflow rather than wrapping. The true shape metric stays `#bb.metric.world.*`. | A | Inst | X | `O(1)` from bounds |
| `#bb.metric.health.batch-replay-mismatch-event` | **Batch replay mismatch event**: exact prestate, batch, expected poststate, reconstructed poststate/root, and first known divergence. | A | Inst | X | replay/check cost |
| `#bb.metric.health.incremental-root-mismatch-event` | **Incremental root mismatch event**: `#bb.metric.hash.incremental-full-root-agreement` fails with dirty-node and full-root witnesses. | A | Future hash | X | full root audit |
| `#bb.metric.health.receipt-verification-failure-event` | **Receipt verification failure event**: receipt sequence, digest, and exact `#bb.metric.hash.invalid-receipt-vector` reason. Typed replay returns this information synchronously; a retained telemetry recorder remains future. | A | Now typed API return; Future recorder | X | receipt verify |
| `#bb.metric.health.persistence-integrity-failure-event` | **Persistence integrity failure event**: snapshot/WAL identity and exact `#bb.metric.store.*` decode/root/durability reason. Journal/history APIs now return local carrier and semantic failures; a retained recorder and hardware-durability evidence remain future. | A/H | Now local API return; Future recorder/hardware evidence | X | store verify |
| `#bb.metric.health.synchronization-integrity-failure-event` | **Synchronization integrity failure event**: peer/session/message identity and exact `#bb.metric.sync.*` sequence/root/auth reason. | A/H | Future sync | X | sync verify |
| `#bb.metric.health.proof-build-failure-event` | **Proof build failure event**: toolchain/input digests, command exit, and first checker diagnostic. | A | Inst | X | proof build |
| `#bb.metric.health.test-regression-event` | **Test regression event**: named test changes from pass to fail against an explicit pinned baseline. | A | Inst | X | compare test manifests |
| `#bb.metric.health.fuzz-failure-vector` | **Fuzz failure vector**: unique minimized crashing/hanging/invariant-breaking inputs and their exact corpus hashes. | A | Future harness | X | fuzz/minimize |
| `#bb.metric.health.undefined-behavior-finding-vector` | **Undefined-behavior finding vector**: unique sanitizer/Miri issue by tool, location, and kind. | A | Inst | X for run | analysis run |
| `#bb.metric.health.compiler-diagnostic-vector` | **Compiler diagnostic vector**: warning/error by toolchain, code, and source location. | A | Now | X | build |
| `#bb.metric.health.clippy-diagnostic-vector` | **Clippy diagnostic vector**: lint, level, and source location under exact pinned flags. | A | Now | X | clippy run |
| `#bb.metric.health.dependency-advisory-vector` | **Dependency advisory vector**: locked package/version matched to advisory database identity and scan time. | A/H | Inst | S over changing DB | advisory scan |
| `#bb.metric.health.dependency-policy-violation-vector` | **Dependency-policy violation vector**: locked package outside declared source/license/version policy. | A | Future policy | X | lock scan |
| `#bb.metric.health.lock-toolchain-drift-vector` | **Lock/toolchain drift vector**: active Cargo lock, Rust pin, Lean pin, and actual tools disagree. | A | Inst | X | file/tool query |
| `#bb.metric.health.reproducible-build-agreement` | **Reproducible-build agreement**: independent builds from identical declared inputs have identical artifact digests. | A | Future assay | X | repeated builds |
| `#bb.metric.health.binary-artifact-bytes-digest` | **Binary artifact bytes/digest**: executable length and content digest for an exact profile/toolchain/input identity. | A | Now size/Future hash | X | file scan |
| `#bb.metric.health.validation-freshness` | **Validation freshness**: current source/input digest equals the digest attached to the latest passing test/proof/build result. | A | Future receipts | X | digest compare |
| `#bb.metric.health.diagnostic-coverage-manifest` | **Diagnostic coverage manifest**: enumerated failure modes and the predicate/counter that detects each; uncovered count is a view. | A | Future manifest | X | `O(modes)` |
| `#bb.metric.health.aggregate-system-state` | **Aggregate system state**: `OK`, `DEGRADED`, `FAULT`, or `UNVERIFIED` derived only from explicit predicate availability/verdicts. | R | Inst | X | `O(predicates)` |
| `#bb.metric.health.aggregate-reason-set` | **Aggregate reason set**: all predicate tags responsible for `#bb.metric.health.aggregate-system-state`; never replace it with one opaque score. | R | Inst | X | `O(predicates)` |
| `#bb.metric.health.last-trusted-boundary` | **Last trusted boundary**: latest tick/root whose required replay, invariant, receipt, persistence, and synchronization predicates all passed. | D/A | Future integrated receipts | X | maintained after checks |
| `#bb.metric.health.kernel-object-inventory` | **Kernel-object inventory**: open process objects/handles partitioned by type, with exact count and identity where the host exposes it. | H | Future host instrumentation | S/X by platform | host query |
| `#bb.metric.health.metric-declaration-extraction-relation` | **Metric-declaration extraction relation**: every declaration-shaped catalog source occurrence maps to zero, one, or multiple parsed records with owner, canonical definition bytes/tag, and failure reason. | A | Now via registry tool/manifest | X | `O(catalog bytes)` |
| `#bb.metric.health.metric-reference-resolution-relation` | **Metric-reference resolution relation**: every metric-name occurrence in catalog documents maps to exactly one owning definition or a closed self/display/external classification, retaining dangling, ambiguous, and wrong-owner references. | A | Now via registry audit | X | `O(document bytes)` |
| `#bb.metric.health.metric-collector-binding-relation` | **Metric-collector binding relation**: each availability claim maps to collector symbol/configuration, sampling boundary, and supporting test/proof receipt; a `Now` claim without a live binding is a fault. | A | Future docs/code audit | X over bindings | `O(metrics + symbols)` |
| `#bb.metric.health.metric-distinctness-review-relation` | **Metric-distinctness review relation**: for every admitted definition, candidate tag, closest prior definition tag, claimed irreducible observable or authority difference, and review outcome/evidence. It records the judgment required by the stopping rule. | A | Manual now/Future ledger | X over recorded judgments | `O(candidates * comparison search)` |
| `#bb.metric.health.metric-registry-independent-extraction-agreement` | **Metric-registry independent-extraction agreement**: compare independently implemented catalog parsers/canonicalizers over identical documents and a powered mutation corpus, retaining implementation identities, ordered extracted records, longest agreeing prefix, and first mismatch. | A | Future independent lens | X over corpus | `O(parsers * catalog bytes)` |

## Collection boundary

The catalog ends at primitive state, event, audit, and host-observation values.
Arbitrary combinations are infinite, so the recorder must not mint definitions
for every ratio or rolling statistic. A new registry name is warranted only when a value has a
new semantic numerator, denominator, event, predicate, or authority boundary.

For the first sidebar checkpoint, a small read-only projection can select from
this registry. Expensive exact audits should display both their result and
`#bb.metric.health.metric-refresh-boundary` freshness, while future metrics remain visibly `N/A` until their
subsystem exists.
