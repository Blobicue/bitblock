# SCALE-2 chunk-relative queries and hierarchical observation receipt

This record promotes `PS-052:SCALE-2@base` from `READY` to **IMPLEMENTED**. It pins exact
`origin/main` measurements, parity evidence, visual benchmark snapshot checks, and bounded-cache
guarantees addressing both hard failure brackets identified in [`SCALE_1_ASSAY.md`](SCALE_1_ASSAY.md)
without modifying authenticated state or `world_tag`.

The raw local measurements and provenance are retained in
[`scale-2-measurement-v1.json`](scale-2-measurement-v1.json).

## Runtime subject and provenance

- **Remote subject:** `origin/main` commit `7ba33058be3c1695c06bc80de69fdd296ff365a6`
  (tree `0c58e9db0836953f6cd70ca0bf1c85348f89f0f6`, parent
  `5e3dc6cd345b13bd63e7d871921edfa9fc1cff31`, PR #22).
- **Exact-head CI evidence:** GitHub Actions runs `45000629176` (`push`) and `45000632946`
  (`pull_request`), 10/10 jobs green (`ubuntu-latest`, `windows-latest`, `wasm32-unknown-unknown`
  `+simd128` V8 parity `39/39` constants, 42 Python validators, generated index + `106/106`
  selftest, and both Lean 4 `--trust=0` proof capsules).
- **Canonical byte sensitivity:** `python3 tools/bbmutate.py render` — `7/7` canonical encoder sites
  KILLED (`100%`).
- **Host and build profile:** Rust `1.96.0 (ac68faa20 2026-05-25)`, LLVM `22.1.2`,
  `aarch64-unknown-linux-gnu`, stripped release profile with `--features protocol-research`,
  20-core ARM64 (`Cortex-X925` / `Cortex-A725`), 130,660,974,592 bytes RAM, Linux
  `7.0.0-1019-nvidia` (non-isolated host).
- **Implementation & CI test artifacts:**
  - `crates/core/src/render.rs`, `crates/core/src/world.rs`, `crates/core/src/camera.rs`:
    `ChunkRelativePos`, `ChunkRelativeCamera`, `chunk_relative_coord`, `chunk_relative_bounds_aabb`,
    `Renderer::render_chunk_relative` / `render_tinted_chunk_relative`, `raycast_chunk_relative` /
    `_reference`, `sweep_point_chunk_relative` / `_reference`, hierarchical front-to-back min-heap
    octant/chunk Hi-Z culling with conservative projective-determinant depth bounds and lazy
    `render_tinted_delta` unpacking, `SubpixelLodPolicy::HierarchicalSubpixel`, and bounded
    `SpatialRenderCache` (`DEFAULT_SPATIAL_CACHE_MAX_CHUNKS = 2_048`).
  - `tests/scale_2_parity.rs`: 6 default-feature CI integration tests running on both Windows and
    Linux (negative chunk edges `-17..=17`, `±10^17` & `i64::MIN`/`i64::MAX` extrema with powered
    raw-`f64` negative control, hierarchical Hi-Z parity + delta hole-drill unpacking, sub-pixel LOD
    parity + `world_tag` invariance, and `SpatialRenderCache` hard cap & revision/world-switch
    invalidation).
  - `examples/scale_2_probe.rs`: reproducible 13-workload benchmark, parity harness, and
    zero-dependency 24-bit RGB PNG snapshot exporter (`--dump-snapshots`).

## Before (`SCALE-1`) versus After (`SCALE-2`) at 801×801 (15 raw samples)

Every `exact-hiz` and `chunk-relative` workload passed `pixel_parity = true`,
`raycast_parity = true`, and `sweep_parity = true` against the unoptimized reference traversal.

| Workload | Mode | Bits | `SCALE-1` p50 / p95 (ms) | `SCALE-2` p50 / p95 (ms) | Ref p50 (ms) | Hi-Z / LOD culled bits | Budget disposition |
| --- | --- | ---: | ---: | ---: | ---: | ---: | --- |
| `HollowShell_1` | `exact-hiz` | 1 | 0.490 / 0.502 | **0.491 / 0.667** | 0.804 | 0 / 0 | PASS (< 8.33 ms) |
| `HollowShell_99` | `exact-hiz` | 99 | 0.776 / 0.791 | **0.783 / 1.016** | 2.576 | 58 / 0 | PASS (< 8.33 ms) |
| `HollowShell_2649` | `exact-hiz` | 2,649 | 1.666 / 1.723 | **1.522 / 1.673** | 4.327 | 1,325 / 0 | PASS (< 8.33 ms) |
| `HollowShell_12699` | `exact-hiz` | 12,699 | 3.341 / 3.433 | **2.247 / 2.400** | 6.730 | 9,386 / 0 | PASS (< 8.33 ms, 3.0× vs ref) |
| `HollowShell_58809` | `exact-hiz` | 58,809 | 9.438 / 9.497 (**MISS**) | **3.459 / 3.705** | 14.188 | 48,413 / 0 | **PASS** (< 8.33 ms; 2.7× vs `SCALE-1`, 4.1× vs ref) |
| `HollowShell_149787` | `exact-hiz` | 149,787 | 19.963 / 20.158 (**MISS**) | **5.577 / 5.664** | 26.084 | 115,658 / 0 | **PASS** (< 8.33 ms & < 16.67 ms; 3.6× vs `SCALE-1`, 4.7× vs ref) |
| `DenseCube16` | `exact-hiz` | 4,096 | 1.256 / 1.299 | **1.257 / 1.414** | 10.455 | 0 / 0 | PASS (< 8.33 ms, 8.3× vs ref) |
| `SparseFarExtrema` | `exact-hiz` | 9 | 2.785 / 2.894 | **2.890 / 3.097** | 4.908 | 0 / 0 | PASS (< 8.33 ms) |
| `HollowShell_149787_Angled` | `exact-hiz` | 149,787 | — | **22.861 / 25.791** | 35.088 | 61,491 / 0 | Oblique 3-face upper bracket (~75k visible 3-face bits; 1.5× vs ref) |
| `HollowShell_149787_FarSubpixel` | `subpixel-lod` | 149,787 | — | **0.062 / 0.075** | 6.481 | 21,795 / 127,992 | **PASS** (104.5× vs ref via O(1) sub-pixel collapse) |
| `NegativeChunkEdges` | `exact-hiz` | 165 | — | **0.340 / 0.447** | 0.371 | 0 / 0 | PASS (exact across `-17..=17`) |
| `ExtremeFarCluster_1e17` | `chunk-relative` | 223 | raw `f64` collapse | **1.755 / 1.941** | 6.238 | 0 / 0 | **PASS** (0 silhouette diffs vs origin truth; raycast 1.12 µs, sweep 1.10 µs) |
| `SignedExtremaCorner` | `chunk-relative` | 223 | raw `f64` collapse | **1.785 / 1.913** | 6.195 | 0 / 0 | **PASS** (0 silhouette diffs vs origin truth at `i64::MAX-32 / i64::MIN+32`) |

## Closure of both `SCALE-1` failure brackets & visual verification

1. **Failure bracket #1 (visible-surface projection/rasterization):** Front-to-back min-heap
   octant/chunk traversal with conservative projective-determinant Hi-Z occlusion culling, O(1)
   5-point corner fast-reject, L1-local per-chunk depth sorting, tiny-triangle direct rasterization
   (`<= 16` bounding-box pixels), and per-bit center-depth fast-reject cuts `HollowShell_58809`
   from `9.438 ms` to **`3.459 ms`** and `HollowShell_149787` from `19.963 ms` to **`5.577 ms`**
   with 100% pixel-exact parity and 100% depth-bit parity against `set_reference_traversal(true)`.
   At far distances where octants/chunks project `<= 1.0 px`, `SubpixelLodPolicy::HierarchicalSubpixel`
   collapses `127,992` bits in **`0.062 ms`** (`104.5×` speedup) while agreeing bit-for-bit above
   the one-pixel threshold.
2. **Failure bracket #2 (far-coordinate `f64` catastrophic cancellation beyond ±2^53):** Powered
   negative control confirms `(100_000_000_000_000_000_i64 as f64) == (100_000_000_000_000_001_i64 as f64)`.
   `ChunkRelativePos` and `ChunkRelativeCamera` use exact `i128` chunk subtraction so rendering,
   raycasting, and collision sweeps at `(10^17, -10^17, 10^17)` and `(i64::MAX - 32, i64::MIN + 32, i64::MAX - 32)`
   retain full 53-bit sub-voxel precision with zero silhouette or hit divergence from origin-translated
   ground truth.
3. **Bounded ephemeral spatial cache (`SpatialRenderCache`):** Keyed by `(world.instance_id(), world.revision())`
   so any `set_on`/`set_off` mutation or world switch invalidates cached open-word masks immediately
   (`8,128` hits, `541` misses, `16` invalidations, `243,712` bytes during `scale_2_probe`). Hard
   chunk ceilings (`DEFAULT_SPATIAL_CACHE_MAX_CHUNKS = 2_048`) are enforced strictly and never enter
   `world_tag` or authenticated state.
4. **Visual verification:** All benchmark PNG snapshots generated by `scale_2_probe --dump-snapshots`
   (including `HollowShell_58809`, `HollowShell_149787`, `HollowShell_149787_Angled`,
   `HollowShell_149787_FarSubpixel`, `NegativeChunkEdges`, `ExtremeFarCluster_1e17`,
   `SignedExtremaCorner`, and `DenseCube16`) plus live Chrome 155 renders of `play.html` (default
   architecture monument + rocky chem planet at `1600×900` with `lane_drift=0 energy_drift=0`) and
   `bitblock-lab.html` (`abi=17`, `match_expected=true`) were directly inspected with zero missing
   faces, cracks, palette violations, or coordinate-collapse artifacts.

## Non-claims

- **No unlimited-scale or GPU-rendering claim:** Single-threaded CPU software rasterization still
  scales with *unoccluded* multi-pixel visible surface geometry. On `HollowShell_149787_Angled`
  (~75,000 simultaneously visible 3-face surface voxels = ~450,000 exact `f64` triangles), exact
  rendering takes `22.861 ms` p50 (`1.53×` faster than reference `35.088 ms`, but above the
  `16.67 ms` 60 Hz tick budget).
- **No authenticated-state or consensus claim:** Rendering LOD summaries and `SpatialRenderCache`
  are strictly ephemeral local renderer state and never enter `world_tag`, `COMMIT-1`, or any
  consensus state root.
- **No quiet-host Windows x86_64 p99 claim:** Timing distributions above were captured on one
  non-isolated Linux `aarch64` host; CI verifies functional parity and deterministic tests on both
  `windows-latest` and `ubuntu-latest`.
