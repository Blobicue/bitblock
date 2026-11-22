# COST-1@commit-1: authenticated local-state measurements v1

This measurement addendum extends [`COST_1_BUDGET.md`](COST_1_BUDGET.md) (`PS-063:COST-1@budget=BOUNDED`)
for the newly implemented `COMMIT-1` research-gated authenticated local-state profile
([`COMMIT_1_RECEIPT.md`](COMMIT_1_RECEIPT.md)). Per `COST-1` discipline, activating `COMMIT-1`
moves authenticated-tree rows out of `N/A` into explicit measured counters and timings with powered
controls, without modifying or retroactively re-grading the historical `6f80857` Windows baseline.

The raw local release probe output is retained in
[`cost-1-commit-1-v1.json`](cost-1-commit-1-v1.json).

## Provenance and host boundary

- **Merged `origin/main` subject:** commit `2ee1e0c874a6849b67d14997cb35bc23e651b93c`
  (tree `734c0fa3565c3d3cb070caee48ef3d10199126e8`, PR #20, CI runs `44996624134` & `44996627977`,
  10/10 jobs green).
- **Build & command:** `cargo run --release --quiet --features protocol-research --example commit_1_cost_probe -- --json`
  under Rust `1.96.0 (ac68faa20 2026-05-25)`, LLVM `22.1.2`, `aarch64-unknown-linux-gnu`
  (Linux `7.0.0-1019-nvidia`, 20-core ARM64, non-isolated host).
- **Default gameplay unchanged:** `COMMIT-1` remains strictly behind `--features protocol-research`;
  the default binary still executes zero trie updates and zero proof verifications.

## Measured `COMMIT-1` profile envelope vs. `CommitmentOperationalLimitsDescriptorV1` & `ProofPolicyDescriptorV1`

| Metric category | Frozen `COMMIT-1` envelope limit | Measured value (`commit_1_cost_probe`, 7 repeats) | Disposition |
| --- | --- | --- | --- |
| 64-tick incremental 3-domain sidecar advance (`World` + `ActiveToken` + `Episode` + cold-rebuild parity check) | Within `CommitmentOperationalLimitsDescriptorV1::LOCAL_DEFAULT` (`max_batch_updates = 65,536`, `max_staged_nodes = 65,536`) | `p50 = 124.74 µs`, `max = 223.78 µs` per tick; `106` / `104` / `144` unique nodes (`354` live across 3 domains); `CommitmentSetV1 = 698 B` | `MET` (deterministic counters & ARM64 timing; Windows x86_64 quiet-host p99 `UNMEASURED`) |
| Write-barrier GC mark + sweep (`MemoryCommitmentStore`, 64-tick history with genesis + head pinned) | `max_gc_mark_nodes = 65,536`, `max_gc_sweep_nodes = 65,536` | `1,452` nodes before GC -> **`1,097` reclaimed**, `355` retained (`100%` reachability preservation) | `MET` (deterministic counter) |
| Context-bound point proof bytes (`WorldRootV1` / `ActiveTokenRootV1` / `EpisodeRootV1`) | `ProofPolicyDescriptorV1` caps: `7,168 B` (World), `8,192 B` (ActiveToken), `12,288 B` (Episode) | `266 B` / `297 B` / `366 B` (`< 4%` of policy cap) | `MET` (powered 1-byte-tighter cap `265 B` rejected fail-closed) |
| Context-bound point proof verification (`verify_lookup`) | Deterministic step/hash caps (`192`/`193` for 192-bit; `256`/`257` for 256-bit) | `p50 = 4.67 µs` (`max = 4.86 µs`) World; `4.78 µs` ActiveToken; `5.30 µs` Episode | `MET` (ARM64 release; quiet-host Windows p99 `UNMEASURED`) |
| Independent full projection rebuild (`verify_independent_projection`, 64 ticks) | `HistoryReplayLimits::default()` & `LOCAL_DEFAULT` | `p50 = 1,018.50 µs` (`1.02 ms`), `max = 1,127.96 µs` | `MET` (deterministic parity verified; 65,536-tick ceiling `UNMEASURED`) |
| 4,096-key `DenseCube16` cold `WorldRootV1` build | `max_full_build_keys = 65,536` | `p50 = 3,226.78 µs` (`3.23 ms`), `max = 3,395.32 µs`; `8,464` structural nodes deduplicated to **`16` unique CAS records** (`529×` content-addressed deduplication!) | `MET` (measured workload; `65,536`-key max-envelope timing `UNMEASURED`) |
| Durable filesystem publication (`DurableAuthenticatedLedger::append_tick`, 16 ticks, per-node + pin + manifest + head `fsync`) | Crash-safe ordered durability (no 60 Hz interactive claim) | `p50 = 112.42 ms`, `max = 147.87 ms` per tick; `206` persisted node files | **HONEST BOTTLENECK (`MISS` against a 16.67 ms 60 Hz interactive tick; acceptable only for offline/batched durable commits)** |
| Durable restart recovery (`DurableAuthenticatedLedger::open_and_recover`, 16 ticks) | `COST-1` recovery ceiling `5,000 ms` | `p50 = 3.31 ms`, `max = 3.45 ms` | `MET` (16-tick sample; 65,536-record recovery ceiling `UNMEASURED`) |

## Key engineering observations & powered control

1. **Powered subject control:** Tightening `ProofPolicyDescriptorV1::max_proof_bytes` by exactly
   `1` byte below the observed `WorldRootV1` proof length (`266 B -> 265 B`) rejected fail-closed
   (`powered_one_byte_tighter_cap_rejected: true`), proving the proof-budget gate is live.
2. **Content-addressed storage deduplication:** Because `TypedNode` hashes depend only on
   `(profile_id, domain, consumed_depth, canonical_payload)`, symmetric dense worlds like
   `DenseCube16` (`4,096` keys, `8,464` structural tree nodes) collapse to just **16 unique
   content-addressed node records** (`529×` deduplication).
3. **Filesystem `fsync` per-file durability cost:** `DurableCommitmentStore` currently stages one
   fsynced file per node plus provisional pin, manifest, and head pointer, averaging **112.4 ms**
   per tick on this Linux host. That proves durable per-tick filesystem publication must **never**
   be placed synchronously on a 60 Hz (`16.67 ms`) interactive loop; batching node staging into a
   single append-only pack/WAL file is the concrete optimization path if low-latency durable
   commits are ever needed.

## Explicit non-claims

- Measurements on this non-isolated ARM64 Linux host do **not** certify quiet-host Windows x86_64
  p99 latency, SSD sudden-power durability, maximum `65,536`-key envelope worst-case timings, or
  network/consensus throughput.
- `COST-1@base` remains `READY` (repeating for every future activated feature) and `COST-1@budget`
  remains `BOUNDED`.
