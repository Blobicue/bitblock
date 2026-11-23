# Bitblock proof capsule

This directory is a standalone Lean 4 model of Bitblock's smallest world core.
It does not import the Rust implementation and makes no claim about rendering,
camera arithmetic, hashing, or wall-clock scheduling.

`BitblockProof.World` gives every coordinate an unbounded integer address,
starts with only immutable origin bedrock on, stores only the finite
coordinates whose bits are on, and treats an ordered list of `on`, `off`, and
`toggle` edits as a program. Lean checks the initial world, bedrock preservation
for every edit and replay, membership laws, semantic idempotence, toggle
involution, finite support, and replay composition.

`BitblockProof.MintBurn` adds a separate duplicate-free `List` model for
literal supply accounting. Mutable supply excludes origin; mint increments it
exactly for an absent non-origin coordinate; burn decrements it exactly for a
present non-origin coordinate; protected and no-op edits preserve it; and
replay preserves origin. The separate representation is deliberate: raw list
length in `World` is not supply because that model permits duplicate storage
while treating duplicates as observationally irrelevant.

`BitblockProof.QuotedByteCost` is a separate static representation-cost proof
for the content-neutral quoted-byte codec. It proves equal cost for the two
distinct bit terms, the exact S/K/I census and atom/iota cost for every
fixed-width byte, and closed forms for every finite byte-list right spine.
Powered theorems separate left-associated PAIR, reversed-bit, and left-list
mutants even though their costs can coincide. Its SK syntax has no evaluator:
the results do not establish executable edit semantics.

`BitblockProof.ChunkRefinement` proves the arithmetic bijection between 3D
signed coordinates and `(ChunkCoord, localIndex)` under Euclidean division by
16 (`div_euclid`/`rem_euclid`), `i64` chunk-coordinate range bounds, depth-1
octant parent/child division, linear `0..4096` local voxel flattening, and a
negative control showing truncated division (`tdiv`/`tmod`) breaks `-1`.

`BitblockProof.OctreeRefinement` extends `ChunkRefinement` to a sparse chunk-map
model (`ChunkWorld` with unique chunk keys, empty-chunk eviction, and bedrock
immutability) and an 8-level octree summary pyramid (`octreeMaxDepth = 8`). It
proves lookup and full edit-replay simulation refinement against
`BitblockProof.World`, equivalence between iterative `div_euclid(2)`
(`World::flush_octree`) and direct `div_euclid(2^d)` (`World::update_octree_ancestors`)
for all signed coordinates and depths, AABB `Bounds` join-semilattice
monotonicity (`mergeBounds`), zero false-empty octant pruning, positive
occupancy and 8-bit `childMask` bounds, conservative AABB enclosure of all
descendant voxels, general downward-monotone culling soundness (frustum/ray/sweep
pruning cannot drop an occupied voxel), depth-8 root coverage, and a negative
control showing truncated division (`tdiv`) breaks octree partitioning at `-1`.
It remains an abstract Lean model over mathematical `Int` and finite lists, not
a proof over Rust's `HashMap` implementation, `i64` overflow, or `f64` camera
arithmetic.

`BitblockProof.AccountingRefinement` formalizes the `ACCOUNT-1` per-tick
action-accounting ledger corresponding to `crates/core/src/tick.rs`
(`TickActionLedger`, `TickAttempt`, `ACTION_LANE_COUNT = 10`,
`MAX_ACTIONS_PER_TICK = 65_536`). It proves the `0..9` lane-ordinal bijection,
exact partition `submitted = cameraLaneTotal + editLaneTotal + controlLaneTotal`,
fail-closed admission at `65,536` actions before mutation, base+step trace fold
preservation across all 10 lanes, per-lane reconciliation against ordered edit
attempts, and powered negative controls for swapped edit lanes (equal total,
false split), v2 control-lane dropping, and over-limit batch refusal.

`BitblockProof.CommitmentRefinement` formalizes the `COMMIT-1` fixed-width
binary Merkle Patricia Trie normalization and `CoordinateKeyV1` sign-bias
encoding corresponding to `crates/research/src/commit_state.rs` and
`crates/research/src/commit_r1.rs`. It proves exact bijection, range bounds,
strict order preservation, and scalar boundary vectors for `biasSigned64` over
all signed 64-bit integers (with a raw two's-complement negative control),
structural uniqueness of `CanonicalPat` over arbitrary fixed bit-width `rem`
(`canonicalPat_unique_of_alpha_eq` covering both 192-bit `WorldRootV1`/
`ActiveTokenRootV1` and 256-bit `EpisodeRootV1`), soundness and completeness of
canonical point proofs (`verifyPatProof_sound`, `canonicalPat_provePat_complete`),
and powered negative controls rejecting adjacent extensions, extension-to-leaf,
empty branch children, empty extension paths, forged extension-to-leaf proofs,
and non-divergent extension proofs. It remains an abstract model over Lean lists
and mathematical integers, not a verified compiler proof of Rust SHA-256 hashing
or machine code.

`BitblockProof.VmRefinement` formalizes the `VM-1` / `VM-D1` bounded deterministic
spatial interpreter corresponding to `crates/research/src/vm_1.rs` and
`crates/research/src/vm_d1.rs`. It proves the exact 8-opcode bijection
(`VmOpcode` <-> `Fin 8`), strictly positive opcode gas bounds, checked signed
64-bit `Add`/`Sub` overflow trap soundness (with a powered wrapping-addition
negative control), guaranteed finite termination (`<= prog.length` steps), gas
monotonicity, `gasUsed <= preExecutionOpGas` and `gasUsed <= gasLimit`,
stack-depth ceiling (`<= maxDepth`) and `InVmI64` preservation, fail-closed
atomic rollback soundness on trap (with a powered partial-write-leak negative
control), bedrock and program-frame write-exclusion preservation, and
disjoint-write commutation (`applyWrites_comm_of_disjoint`) directly over
`BitblockProof.World`. It remains an abstract model over Lean integers and
finite lists, not a verified compiler proof of Rust machine code or shared
consensus authority.

`LadderLangQuotedByteParity.lean` is deliberately outside the standalone
Bitblock target. Run it from the separately pinned LadderLang checkout at commit
`59042265167b8f40700a891d890fde8015bcade9`; it asks LadderLang's own
`Lam.costChecked` to reproduce the bit, `PAIR`, byte, and two-byte-spine costs,
and verifies that an open term is refused. This is a dependency/parity assay,
not a claim that the two repositories share a verified compiler.

`ladderlang-transition-v1/` is a separate, self-contained copy of the exact
two-source transitive closure needed to run a finite one-bit transition truth
table under LadderLang's actual bounded reducer. Its verifier checks copied
source SHA-256 values before building (`verify.ps1` on Windows, or manifest
SHA-256 verification + `lake build` + `lake env lean --trust=0
BitblockLadderTransition.lean` on Linux/macOS, also enforced in CI). It retains
`HOLD`, `RAN`, and `GREW` outcomes, and is not a Rust refinement or general
semantic bridge.

These are abstract mathematical models. They do not prove that the Rust world,
renderer, hash/receipt encoding, network, or consensus implementation refines
either model.

Build from this directory with:

```powershell
lake build
```
