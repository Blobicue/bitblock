# REFINE-1@world: Rust-to-Lean world/chunk/octree correspondence v1

This record resolves `PS-022:REFINE-1@world` as **BOUNDED** (keeping `PS-021:REFINE-1@base=READY`
and the accounting/commitment/VM refinement phases `LOCKED` until their owning implementations are
refined). It connects the verified Lean 4 mathematical world/chunk/octree theorems directly to the
live Rust `bitblock::world::World` implementation via a named abstraction function, exhaustive
local-chunk bijection tests, signed-boundary/extrema differential checks, base+step simulation
correspondence, and powered negative controls.

## Provenance and verified capsules

- **Merged `origin/main` subject:** commit `58cefcb9e9b4ae24cb90861571489560266d6b23`
  (tree `2166e50bcce90d14ea80abec8b2190dd9ba6b93c`, PR #14, CI runs `44978821303` & `44978824816`,
  10/10 jobs green).
- **Lean 4 `--trust=0` proof capsules (`proof/BitblockProof.lean`):**
  - `proof/BitblockProof/World.lean`: abstract finite-support `World`, immutable origin bedrock,
    edit replay & conservation laws.
  - `proof/BitblockProof/ChunkRefinement.lean` (27 theorems): Euclidean `splitAxis`/`joinAxis`
    bijection over `Int` (`Int.ediv`/`Int.emod` by 16), `localIndex`/`unflattenIndex` bijection over
    `0..4096`, `octantAxis`, and `i64` chunk bounds.
  - `proof/BitblockProof/OctreeRefinement.lean` (54 theorems): `ChunkWorld` sparse chunk map
    refinement (`toAbstractWorld`), empty-chunk eviction, `defer_octree`/`flush_octree` ancestor
    equivalence, truncated-division negative control (`truncDiv_breaks_octree_partitioning`),
    join-semilattice `Bounds`, zero false-empty pruning, and depth-8 macro-octant coverage.
- **Rust correspondence harness:** `tests/world_refinement_correspondence.rs` (default-feature
  integration test executed in both `quick` and `full` tiers on Windows and Linux CI).

## Named abstraction function and correspondence map

| Lean 4 model definition | Rust runtime counterpart (`crates/core/src/world.rs`) | Correspondence check in `tests/world_refinement_correspondence.rs` |
| --- | --- | --- |
| `toAbstractWorld : ChunkWorld -> World` | `alpha_world(&World) -> BTreeSet<Coord>` via `iter_on()` | Base state `World::new() == {(0,0,0)}`; step-by-step set equality across `set_on`, `set_off`, `toggle`, and `for_each_on_ordered`. |
| `localIndex` / `unflattenIndex` | `World::chunk_of(coord).1` & `Chunk::words` (`[u64; 64]`) | **Exhaustive** over all `4,096` `(lx, ly, lz)` triples in both positive `(0,0,0)` and negative `(-1,-1,-1)` chunks; verifies saturated `u64::MAX` word layout. |
| `splitAxis` / `joinAxis` | `World::chunk_of(coord)` (`div_euclid(16)` / `rem_euclid(16)`) | **Exact boundary grid** of `13^3 = 2,197` signed triples spanning `{i64::MIN, i64::MIN+1, -17, -16, -15, -1, 0, 1, 15, 16, 17, i64::MAX-1, i64::MAX}`. |
| `WellFormedChunkWorld` (empty eviction + bedrock) | `World::allocated_chunk_count()`, `iter_chunks()`, `BEDROCK_COORD` | Verifies every allocated chunk has `count > 0`, empty non-origin chunks evict immediately, and `(0,0,0)` stays occupied under `On`/`Off`/`Toggle`. |
| `flush_octree` equivalence & `octantSummary` AABB | `defer_octree()` + `flush_octree()`, `root_octants()`, `octant_summary()` | Side-by-side eager vs deferred world yields identical depth-8 root octants & occupied counts; every occupied voxel lies inside its root octant's conservative AABB. |
| Powered negative controls | `truncDiv_breaks_octree_partitioning` & bedrock drop | Asserts Rust truncating `/` (`-1 / 16 == 0`) disagrees with `World::chunk_of((-1,0,0)).0.x == -1`, and dropping `(0,0,0)` breaks `alpha_world` equality. |

## Explicit boundedness and non-claims

`BOUNDED` is mandatory per `CAPABILITY_TREE.md`:
- The 4,096-index local bijection is exhaustive, whereas the signed-coordinate grid (`2,197`
  triples) and simulation script are finite differential checks paired with universal Lean 4 proofs
  over `Int` (`ChunkRefinement.lean` + `OctreeRefinement.lean`).
- Neither Lean 4 nor these tests formally extract or verify the `rustc`/LLVM compiled machine code,
  and no silent appeal is made from a finite test sample to all `2^192` `i64³` coordinates.
- Accounting (`REFINE-1@accounting`), commitment (`REFINE-1@commitment`), and VM (`REFINE-1@vm`)
  refinements remain separate phases.
