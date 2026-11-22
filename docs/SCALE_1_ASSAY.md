# SCALE-1 workload assay v1

This record resolves `PS-051:SCALE-1@base` as **BOUNDED** and unlocks `PS-052:SCALE-2@base` as
**READY**. It separates allocated bits, chunks, macro-octant roots, exposed versus buried surface
faces, frustum-culled and raycast-pruned chunks, raw multi-sample rendering/raycast/collision
distributions, and incremental `world_tag` v2 updates under one pinned build, identifies the exact
hard failure brackets where the current software renderer misses its fixed budgets, and declares the
exact semantic parity invariants that `SCALE-2` must preserve.

The raw local measurements and provenance are retained in
[`scale-1-measurement-v1.json`](scale-1-measurement-v1.json).

## Runtime subject and provenance

- **Remote subject:** `origin/main` commit `db0ede5f518924a2bebf5ce0269cd50ca69af763`
  (tree `944ceb99e8c6774d07c84321037aab6e43e042ec`, parent
  `f512ad5ee86ab13e6daaee6ec918ca7508250399`, PR #12).
- **Exact-head CI evidence:** GitHub Actions runs `44976235042` and `44976260396` (10/10 jobs green).
- **Host and build profile:** Rust `1.96.0 (ac68faa20 2026-05-25)`, LLVM `22.1.2`,
  `aarch64-unknown-linux-gnu`, stripped release profile with `--features protocol-research`,
  20-core ARM64 (`Cortex-X925` / `Cortex-A725`), 130,660,974,592 bytes RAM, Linux
  `7.0.0-1019-nvidia` (non-isolated host).
- **Deterministic structural report:** `ScaleWorkloadReportV1::canonical()`
  (`bitblock/scale-assay/workload-report/v1`, `crates/research/src/scale_assay.rs`, independently
  reconstructed in `tools/scale_assay_vectors.py`), pinned tag
  `#bb.receipt.sha256.xq6oxiyyao3khyp3i7r6zbz3zwewouc75btw4s733xp62zeic3ma`.
- **Reproducible timing & parity probe:** `examples/scale_1_probe.rs` (801×801 viewport, 15 raw
  samples per workload; asserts 100% pixel-exact render parity, exact `raycast` parity, and exact
  `sweep_point` collision parity against the reference full-scan implementations).

## Deterministic structural workloads (`ScaleWorkloadReportV1`)

| Workload | Bits | Chunks | Root octants | Exposed faces | Buried faces | Buried voxels | Frustum vis / culled | Raycast tested / pruned |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `SingleOrigin` | 1 | 1 | 1 | 6 | 0 | 0 | 1 / 0 | 1 / 0 |
| `DenseCube16` | 4,096 | 1 | 1 | 1,536 | 23,040 | 2,744 | 1 / 0 | 1 / 0 |
| `HollowShell16` | 1,352 | 1 | 1 | 2,712 | 5,400 | 0 | 1 / 0 | 1 / 0 |
| `NegativeChunkCluster` | 9 | 8 | 8 | 30 | 24 | 0 | 8 / 0 | 2 / 6 |
| `SparseFarExtrema` (±1,048,576) | 9 | 9 | 9 | 54 | 0 | 0 | 1 / 8 | 1 / 8 |

## Raw timing distributions and parity (`scale_1_probe`, 801×801, 15 raw samples)

Every row passed `pixel_parity = true`, `raycast_parity = true`, and `sweep_parity = true`.

| Workload | Bits | Chunks | Octree render p50 / p95 (ms) | Ref render p50 (ms) | Raycast p50 / ref (µs) | Sweep p50 / ref (µs) | `world_tag` cold / inc (µs) | Render budget disposition |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `HollowShell_1` | 1 | 1 | 0.490 / 0.502 | 0.818 | 0.32 / 0.05 | 0.05 / 0.06 | 1.73 / 2.40 | below 8.33 ms |
| `HollowShell_99` | 99 | 8 | 0.776 / 0.791 | 2.594 | 0.48 / 0.96 | 0.11 / 1.02 | 10.85 / 3.95 | below 8.33 ms |
| `HollowShell_2649` | 2,649 | 8 | 1.666 / 1.723 | 4.308 | 10.51 / 19.60 | 0.11 / 21.47 | 10.85 / 3.95 | below 8.33 ms |
| `HollowShell_12699` | 12,699 | 57 | 3.341 / 3.433 | 6.541 | 2.21 / 93.50 | 0.11 / 101.84 | 74.16 / 15.22 | below 8.33 ms |
| `HollowShell_58809` | 58,809 | 297 | 9.438 / 9.497 | 13.797 | 8.86 / 435.44 | 0.11 / 473.15 | 385.55 / 72.05 | **MISS** (> 8.33 ms render budget) |
| `HollowShell_149787` | 149,787 | 489 | 19.963 / 20.158 | 25.586 | 2.30 / 1,101.97 | 0.11 / 1,201.85 | 636.22 / 119.04 | **MISS** (> 16.67 ms 60 Hz tick) |
| `DenseCube16` | 4,096 | 1 | 1.256 / 1.299 | 10.218 | 29.57 / 29.89 | 0.05 / 32.69 | 1.71 / 2.38 | below 8.33 ms (8.1× vs ref) |
| `SparseFarExtrema` | 9 | 9 | 2.785 / 2.894 | 4.798 | 0.35 / 0.29 | 0.37 / 0.30 | 12.18 / 4.19 | below 8.33 ms (8/9 culled) |

## Measured index gains and hard failure brackets

1. **8-level sparse octree pruning (`render.rs`, PR #8):** Cuts `DenseCube16` rendering by **8.1×**
   (1.256 ms vs 10.218 ms, because 2,744 interior voxels are fully buried), `HollowShell_12699` by
   **2.0×**, `raycast` at 149,787 bits by **479×** (2.30 µs vs 1,101.97 µs), and `sweep_point`
   collision at 149,787 bits by **10,926×** (0.11 µs vs 1,201.85 µs), while preserving 100%
   pixel/hit/sweep parity.
2. **Cached two-level chunk Merkle `world_tag` v2 (`history.rs`):** Cuts single-bit incremental
   re-tagging at 149,787 bits (489 chunks) by **5.3×** (119.04 µs vs 636.22 µs cold).
3. **Hard failure bracket #1 — visible surface-voxel projection/rasterization:** Because hollow
   shells expose every voxel on the surface, octree culling cannot discard visible surface voxels.
   At **58,809 bits** (297 chunks), raw p50/p95 reaches **9.438 / 9.497 ms** (`MISS` vs 8.33 ms
   render budget); at **149,787 bits** (489 chunks), raw p50/p95 reaches **19.963 / 20.158 ms**
   (`MISS` vs 16.667 ms 60 Hz tick budget).
4. **Hard failure bracket #2 — far-coordinate `f64` camera/collision precision:** Although
   `SparseFarExtrema` (±1,048,576) preserves exact hit/render parity, raw `f64` camera coordinates
   lose unit-voxel precision beyond ±2^53 (~9×10^15) across the full signed `i64³` address space.

## Proposed `SCALE-2` indexes and required semantic parity

Every future `SCALE-2` optimization must name which failure bracket above it reduces and prove exact
parity against the reference path before merging:

| Proposed `SCALE-2` index | Measured cost reduced | Mandatory semantic parity & boundary |
| --- | --- | --- |
| Chunk-relative camera & collision coordinates | Eliminates `f64` catastrophic cancellation beyond ±2^53 | Exact agreement with `raycast_reference` and `sweep_point_reference` across negative chunk boundaries (`-17, -16, -15, -1, 0, 1, 15, 16, 17`) and signed `i64` extrema. |
| Hierarchical occupancy / sub-pixel LOD summaries with top-saturation composition | Reduces per-voxel projection/rasterization above 58,809 visible surface bits | Must agree identically with `Renderer::set_reference_traversal(true)` whenever voxel projection exceeds the exact pixel-saturation threshold; LOD summaries must remain purely ephemeral rendering caches and never enter authenticated state or `world_tag`. |
| Bounded spatial-cache invalidation & memory caps | Prevents unbounded cache residency on edit-heavy multi-chunk worlds | Exact cache invalidation on `set_on`/`set_off`; deterministic memory ceiling under `LOCAL-BOUNDS-1` / `COST-1`. |

## Non-claims

This assay does **not** establish universal hardware performance, quiet-host Windows x86_64 p99
compliance, GPU acceleration, near-infinite rendering scale, or authenticated-state scaling. Sparse
chunk/octree allocation does not eliminate visible-surface rasterization, history retention, or
proof-verification cost.
