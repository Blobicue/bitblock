# COST-1 budget v1

This artifact resolves `PS-063:COST-1@budget` as **BOUNDED**. It fixes a finite local-reference
budget before any later COST-driven optimization, records what the current runtime does and does
not establish, and leaves wider hardware, network, authenticated-state, and production profiles
open. The raw local observations are retained in
[`cost-1-measurement-v1.json`](cost-1-measurement-v1.json).

The runtime subject is remote `main` commit
`6f80857d2a34b0d409afe2efa394a5c3c808da62`, tree
`cbeb1d0069e2e42ed8c41fa4b7911ad0df54a0f6`. Host timings use Rust 1.96.0,
`x86_64-pc-windows-gnu`, the default stripped release profile, an AMD Ryzen 7 7800X3D, Windows 11
build 26200, 67,821,977,600 bytes of physical memory, full 16-logical-processor affinity, and the
Balanced power plan. The machine was not isolated: Codex agents, ordinary desktop services, and
user applications remained active.

## Disposition vocabulary

- `MET` means the named current measurement is within its budget and its detector was powered by a
  production-subject control.
- `MISS` means the current implementation or sampled workload crosses the already fixed budget.
- `UNMEASURED` means the budget is fixed but the required current distribution or deterministic
  counter has not been captured. It is not a pass.
- `N/A` means the current local profile has no such activated surface and therefore has a zero-event
  budget. The first activation moves the row to `UNMEASURED`; it cannot jump directly to `MET`.
- `HISTORICAL` explains a decision but is ineligible to satisfy a current-build budget.

`BOUNDED` is deliberate: a finite research artifact now exists, but current compliance is not all
green and the wider cheapness claim remains open. A miss is not permission to enlarge a budget.

## Fixed budgets and current result

| Category | Fixed local-reference budget | Current result |
| --- | --- | --- |
| Default binary and dependencies | Windows default stripped release executable at most 1,310,720 bytes; at most 4 unique direct external crates, 3 active external crates on one target, and 211 `Cargo.lock` package records. | `MET` at 991,744 bytes, SHA-256 `9552b61fe13293e37968c5e770e7606093bf57f70974df360fcbe508baca65d6`, with 4 / 3 / 211. |
| Touched-state memory | Reference workloads admit at most 65,536 occupied bits and 65,536 allocated chunks; world-owned requested heap at most 67,108,864 bytes. Canonical `world_tag` scratch is one allocation of exactly `24*N` bytes, at most 1,572,864 bytes. Typed recovery retains its stricter 16,384-chunk ceiling. | `MET` only for measured tag scratch at `N=1` and `N=4,098`; total `World` heap is `UNMEASURED`. |
| Tick work | At most 65,536 accepted actions and dispositions; worst-shaped canonical v3 outcome at most `469 + 65,536*26 = 1,704,405` bytes. Reference fixed-tick p99 at most 16.666666 ms. | Deterministic action/decoder limit `MET`; tick-duration distribution `UNMEASURED`. |
| Renderer and collision | 801×801 reference viewport and at most 12,699 occupied AABB candidates per interactive reference query. Render plus frame observation p99 at most 8.33 ms; complete dirty tick at most 16.666666 ms. | True p99 and collision count are `UNMEASURED`. Even the best-of-12 phase statistic is a `MISS` at 58,809 bits for 8.33 ms and at 149,787 bits for 16.666666 ms. |
| Commitment updates | Legacy tag reference envelope `N <= 65,536`; canonical payload at most 1,572,905 bytes; scratch at most 1,572,864 bytes; release tick at most one full tag before an idle fold and two across an edit fold. Authenticated-tree updates have a zero-event budget. | Observed scratch `MET` at two finite workloads; maximum-envelope time is `UNMEASURED`; authenticated-tree work is `N/A`. |
| Proof verification | Default gameplay: exactly 0 proof bytes and 0 verifier calls. Offline release evidence: each Lean or LadderLang capsule at most 60,000 ms wall time and 1,073,741,824 peak resident bytes on the reference host. Any runtime verifier needs a new finite proof-size/work budget first. | Gameplay is `N/A`; offline capsule time and peak memory are `UNMEASURED`. |
| Bandwidth | Exactly 0 application network bytes, messages, sockets, and peers in the local profile. | `N/A`; networking is not implemented. The first byte reopens this row. |
| Database writes | Exactly 0 database operations and bytes. The separate research file journal remains bounded to 8,388,608 payload bytes, 16,777,216 record bytes, 65,536 records, and 67,108,864 stream bytes. | Database work is `N/A`; a research file journal is not a database or sudden-power proof. |
| Retained history | Live TickOutcome canonical bytes and requested heap each at most 67,108,864 bytes. At the 469-byte idle shape, 143,089 outcomes are the last canonical-byte pass and 143,090 is the first failure. | `MISS`: `tick_outcomes` is an unbounded live `Vec`; edit-heavy outcomes reach the ceiling earlier. |
| Recovery | At most 67,108,864 carrier bytes and 65,536 records; semantic replay at most 16,384 receipts, 65,536 edits per receipt, 262,144 total edits, 65,536 peak bits, 16,384 peak chunks, and 4,194,304 world-tag coordinate visits. Reference-host p99 at most 5,000 ms and requested heap at most 134,217,728 bytes. | Deterministic one-over refusal `MET`; wall-time and peak-memory distributions `UNMEASURED`. |
| Idle overhead | After the first changed frame: 0 new render, composition, presentation, storage, or network events; exactly one 469-byte outcome per tick; at most 28,140 canonical bytes/s at 60 Hz; genesis-world idle p99 at most 1.67 ms. | No-rerender semantics `MET`; duration and heap slope `UNMEASURED`; retained history remains `MISS`. |

## Measurements and failure points

### Binary and dependency surface

`cargo build --release --locked --bin bitblock` produced a 991,744-byte executable. The build pins
the exact runtime commit, `Cargo.lock` SHA-256
`997fc3b22912ed4de11f007e04303b9d4a651c816eedd09ce488d5998096b5a5`, toolchain, target,
profile, features, executable hash, and machine in the JSON receipt. The dependency inventory is
four unique direct external crates; `sha2`, `windows-sys`, and `winit` are active on Windows; the
lockfile contains 211 package records.

The powered subject control used a disposable clone at the exact runtime commit. A reachable
400,000-byte production static retained through `std::hint::black_box` produced a 1,391,616-byte
executable, 80,896 bytes over budget, so the size disposition turned red. The exact clone was then
moved to the Windows Recycle Bin. The first byte over 1,310,720, fifth unique direct dependency,
fourth active dependency, or 212th lock package is the corresponding failure point.

### Touched-state memory and legacy commitment

Seven release runs of `world_tag_alloc_probe` were exact and degenerate, as deterministic allocator
counters should be:

| Workload | Occupied bits | Allocations in every run | Requested bytes in every run |
| --- | ---: | ---: | ---: |
| origin | 1 | 1 | 24 |
| spread | 4,098 | 1 | 98,352 |

This proves only the allocation requests made by canonical `world_tag`; it does not measure total
`World` residency, hash-map overhead, RSS, memory traffic, or energy. The powered production-subject
control added one semantically inert reachable 64-byte scratch allocation inside `world_tag`.
Origin became 2 allocations / 88 bytes and spread became 2 / 98,416 while both canonical tags stayed
unchanged, so the allocation detector turned red before the disposable clone was recycled.

The exact legacy payload formula is `2 + 31 + 8 + 24*N`; the 65,536-bit envelope is therefore
1,572,905 canonical bytes and 1,572,864 scratch bytes. The first extra bit, chunk, heap byte,
scratch allocation, or scratch byte is the failure point. This does not establish the unmeasured
67,108,864-byte total-world heap ceiling.

### Tick work

TickOutcome v3 has a 469-byte fixed payload and a worst-shaped resolved attempt adds 26 bytes. The
action ledger and independent decoder cap the total at 65,536 before mutation. The exact hostile
single-lane and `u32::MAX + 1` aggregate controls passed 1/1 in both the dev profile with overflow
checks and the release profile. The 65,537th action or any fixed tick beyond 16.666666 ms is a
failure. No tick-only latency distribution was captured, so the timing half remains
`UNMEASURED`.

### Renderer and collision

Both retained renderer probes were used, without replacing their workload generators. Seven
independent process runs of `render_time_probe` at 12,699 bits produced the process-level
best-of-12 statistic `[5.57, 5.53, 5.51, 5.47, 5.88, 5.42, 5.98]` ms. Its p50 is 5.53 ms and
nearest-rank p95/max is 5.98 ms. Because each row sums separately minimized render and observation
samples, this is not a raw frame-latency p99 and cannot mark the budget `MET`.

`render_phase_probe` supplies an actual failure bracket while holding drawn pixels at 109,561:

| Occupied bits | Seven fastest-of-12 totals (ms) | p50 | p95/max | Disposition |
| ---: | --- | ---: | ---: | --- |
| 12,699 | 3.65, 3.59, 3.55, 3.56, 3.62, 3.65, 3.59 | 3.59 | 3.65 | below 8.33, but not a p99 pass |
| 58,809 | 9.92, 9.66, 9.86, 9.87, 9.77, 10.65, 10.19 | 9.87 | 10.65 | `MISS` at 8.33 even under a best-case statistic |
| 149,787 | 21.45, 21.23, 21.03, 21.57, 21.60, 23.44, 22.35 | 21.57 | 23.44 | `MISS` at 16.666666 |

The historical pre-budget renderer table in `docs/PROTOCOL.md` records 9.02 ms becoming 3.78 ms at
12,699 bits. It influenced the budget but is `HISTORICAL`: its machine identity was not retained,
and optimization preceded this budget. It cannot pass a current row. Collision still scans occupied
coordinates, and no exact per-query counter or latency distribution was retained, so it remains
`UNMEASURED` rather than inheriting the renderer result.

### Verification and proof boundary

`python -B tools/bbtime.py --best-of-3 --top 12` ran serially over 414 tests. It retained 130.2 s
of profiling wall time, 7.5 s of summed isolated best-of-three test cost, and a 0.635 s slowest test.
The JSON output SHA-256 is
`c082bd80772475cefac57e54c232b89806a649cc7ae96c4d00a734f9f59ec93b`. These figures localize
test cost; they are not runtime proof verification or additive suite wall time.

`python -B tools/bbverify.py full --force` passed all 12 checks in 11.3 s, including 414/414 tests,
34 independent Python reconstructions, and 1,112 metric aliases. This is correctness/byte-stability
evidence, not proof-cost compliance. The default gameplay profile invokes no proof verifier. The
offline 60 s / 1 GiB capsule budget remains `UNMEASURED` until raw time and peak-memory
distributions are retained.

### Bandwidth, writes, retention, recovery, and idle

Networking and databases are absent, so their current budgets are exact zero-event `N/A` rows. The
feature-gated file journal's existing logical limits are retained separately and do not become
database, physical-write, or power-loss measurements.

At 469 canonical bytes per idle outcome, 67,108,864 bytes holds 143,089 outcomes
(67,108,741 bytes); outcome 143,090 reaches 67,109,210 bytes and fails. At 60 Hz the last pass is
about 39.75 minutes. The live vector has no byte/count cap, so this is a present `MISS`, not a
future concern and not a reason to enlarge the ceiling.

The typed recovery verifier's exact-usage test passed 1/1 and then reduced each of its six limits by
one in turn; every one-less subject was refused before the exact envelope was restored. This powers
the deterministic recovery counters, not the unmeasured 5 s / 128 MiB host budgets. The existing
idle test also passed 1/1 and proves that an unchanged tick does not render a new frame. It does not
measure idle latency, CPU wakeups, allocation slope, or the unbounded retained-outcome vector.

## Excluded receipts and non-claims

One attempted exact Cargo filter omitted the fully qualified test path and exited green with zero
tests in every binary. It is explicitly excluded. A later package-isolated Windows test build also
failed before execution because it lacked the workspace's dependency-feature unification; the
declared workspace-shaped command was used instead, with explicit 1/1 relevant counts.

This artifact does not prove universal cheapness, near-infinite scale, another host, a quiet-host
p99, energy use, hardware memory traffic, physical disk writes, sudden-power durability, network
capacity, authenticated-tree cost, proof-system cost, or production sufficiency. Sparse allocation
does not erase traversal, retention, hashing, recovery, or presentation cost. Every activated
feature repeats COST-1 with its own exact profile, and no `N/A`, `UNMEASURED`, `MISS`, or historical
row may be silently inherited as `MET`.
