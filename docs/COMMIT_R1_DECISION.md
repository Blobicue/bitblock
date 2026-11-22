# COMMIT-R1 authenticated-map decision v1

This record resolves `PS-015:COMMIT-R1@base` as **BOUNDED**. It records the side-by-side R0–R5
hash-tree comparison between **Candidate A** (fixed-key binary Merkle Patricia Trie) and
**Candidate B** (192-level Sparse Merkle Tree) over `CoordinateKeyV1` (`WorldRootV1`) specified in
[`AUTHENTICATED_STATE_PLAN.md`](AUTHENTICATED_STATE_PLAN.md), selects Candidate A as the hash-tree
structure for the future `COMMIT-1` checkpoint, and preserves all non-claims: neither candidate is
wired into live `World`, `world_tag`, or any accepted receipt/block, and the separate `VERKLE-R1`
vector-commitment lane remains explicitly deferred.

The raw local measurements and provenance are retained in
[`commit-r1-assay-v1.json`](commit-r1-assay-v1.json).

## Runtime subject and provenance

- **Remote subject:** `origin/main` commit `db0ede5f518924a2bebf5ce0269cd50ca69af763`
  (tree `944ceb99e8c6774d07c84321037aab6e43e042ec`, parent
  `f512ad5ee86ab13e6daaee6ec918ca7508250399`, PR #12).
- **Exact-head CI evidence:** GitHub Actions runs `44976235042` and `44976260396` (10/10 jobs green
  across `rust (windows-latest)`, `rust (ubuntu-latest)` + `wasm32` `+simd128` V8 parity,
  `python vector tools`, `generated index`, and `lean proof capsule`).
- **Host and build profile:** Rust `1.96.0 (ac68faa20 2026-05-25)`, LLVM `22.1.2`,
  `aarch64-unknown-linux-gnu`, stripped release profile with `--features protocol-research`,
  20-core ARM64 (`Cortex-X925` / `Cortex-A725`), 130,660,974,592 bytes RAM, Linux
  `7.0.0-1019-nvidia` (non-isolated host).
- **Authoritative implementation & independent parity surfaces:**
  - Rust prototypes, bounded verifiers (`ProofPolicyLimits`, `verify_proof_bytes_with_limits`), and
    R2/R3/R4 unit controls: `crates/research/src/commit_r1.rs` (`bbmutate.py commit_r1`: 19/19
    canonical endianness sites KILLED).
  - Independent stdlib Python reconstructions & negative controls: `tools/commit_r1_vectors.py`,
    `tools/codec_parity_corpus.py`, and `tests/codec_parity_v1.rs`.
  - Reproducible release assay binary: `examples/commit_r1_assay_probe.rs`.

## Comparative assay results (`commit_r1_assay_probe`, 7 repeats, p50)

Input-key sorting is timed separately from tree rebuild per R0 (`0.06 µs` at 1–9 keys,
`10.83–12.14 µs` at 1,000 keys, `196.35 µs` at 4,096 keys). Every workload verified bounded point
proofs under `ProofPolicyLimits::PATRICIA_R1_DEFAULT` (7,168 B / 192 steps / 193 hashes) and
`ProofPolicyLimits::SMT_R1_DEFAULT` (6,171 B / 192 siblings / 193 hashes), plus live `Arc` sibling
fork isolation and delete/reinsert root + node-structure restoration parity.

| Workload | Keys | Chunks | MPT rebuild (µs) | SMT rebuild (µs) | MPT nodes | SMT non-def branches | MPT proof (B) | SMT proof (B) | MPT verify (µs) | SMT verify (µs) | MPT ins hashes / µs | SMT ins hashes / µs |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `SingleOrigin` | 1 | 1 | 0.66 | 143.65 | 1 | 192 | 28 | 27 | 0.78 | 126.00 | 4 / 2.56 | 193 / 141.60 |
| `DenseCube16` | 4,096 | 1 | 5,434.84 | 10,284.05 | 8,464 | 20,475 | 484 | 411 | 5.68 | 67.92 | 4 / 5.17 | 193 / 84.93 |
| `HollowShell16` | 1,352 | 1 | 1,013.74 | 8,377.81 | 2,976 | 18,907 | 484 | 411 | 5.68 | 67.94 | 4 / 1.63 | 193 / 80.86 |
| `NegativeChunkCluster` | 9 | 8 | 8.19 | 422.77 | 24 | 1,020 | 197 | 155 | 2.78 | 67.92 | 5 / 1.74 | 193 / 78.58 |
| `SparseFarExtrema` | 9 | 9 | 8.19 | 430.01 | 24 | 1,037 | 108 | 91 | 1.46 | 67.82 | 6 / 2.16 | 193 / 78.91 |
| `Clustered1000` | 1,000 | 1 | 750.41 | 3,576.44 | 2,221 | 7,881 | 484 | 411 | 5.70 | 67.92 | 4 / 1.78 | 193 / 80.83 |
| `Dispersed1000` | 1,000 | 1,000 | 788.14 | 62,207.94 | 2,264 | 142,326 | 457 | 411 | 4.99 | 67.84 | 17 / 6.77 | 193 / 80.70 |

Multiproof wire bytes (8-key clustered / 8-key dispersed):
- `DenseCube16`: Patricia `552 / 552 B` vs SMT `315 / 315 B`.
- `HollowShell16`: Patricia `552 / 1,232 B` vs SMT `315 / 603 B`.
- `Clustered1000`: Patricia `552 / 761 B` vs SMT `315 / 315 B`.
- `Dispersed1000`: Patricia `404 / 2,215 B` vs SMT `187 / 1,883 B`.

Reference note on `SparseOctreeV1` (`crates/research/src/octree_commit.rs`): chunk-aligned arity-8
octree rebuild is `1.38 µs` on single-chunk worlds (`DenseCube16`, `HollowShell16`, `Clustered1000`)
and `1,781.96 µs` on 1,000 single-bit chunks (`Dispersed1000`). That confirms spatial chunking wins
on dense single-chunk worlds, while staying strictly outside this two-candidate 192-bit key
decision.

## R5 decision record

### Selected structure
**Candidate A — Fixed-Key Binary Merkle Patricia Trie** (`bitblock/commit-r1/world-patricia/v1`,
`bitblock/commit-r1/patricia-proof/v1`) is selected as the hash-tree structure for `WorldRootV1`
in the upcoming `COMMIT-1` design.

### Why Candidate A beats Candidate B beyond the materiality threshold
1. **Dispersed multi-chunk rebuild & memory blowup:** On `Dispersed1000` (1,000 keys across 1,000
   signed chunks), Candidate B allocates **142,326 non-default branch nodes** and takes **62.21 ms**
   to rebuild, whereas Candidate A allocates **2,264 total nodes** (**62.9× fewer**) and rebuilds in
   **0.79 ms** (**78.9× faster**). Because Bitblock's signed `i64³` world explicitly permits sparse,
   far-apart chunks, an uncompressed 192-level tree suffers severe storage and rebuild amplification
   precisely in sparse worlds.
2. **Incremental update work:** Candidate A performs **4–17 node hashes** (`1.63–6.77 µs`) per
   single-key insert/delete versus Candidate B's fixed **193-level hash floor** (`78.58–141.60 µs`)
   — an **11.4×–48.3× reduction in hashes** and **11.9×–55.3× reduction in wall latency**.
3. **Bounded point-proof verification:** Candidate A verifies in **0.78–5.70 µs** versus Candidate
   B's fixed **67.82–126.00 µs** (**11.9×–161.5× faster**), while point-proof wire sizes remain
   within 1–73 bytes (`28–484 B` vs `27–443 B`), and absence proofs on clustered worlds are
   *smaller* (`46 B` vs `59 B`).
4. **Canonicalization & soundness closure:** All R2/R3/R4 controls pass identically in Rust and
   independent Python — including depth 0/191/192 zero-suffix leaves, off-by-one child-depth
   rejection, nonzero extension padding rejection, cross-key proof spoofing rejection, exact
   boundary/one-over `ProofPolicyLimits` checks, local-cap widening refusal, 6-permutation rebuild
   parity, `Arc` sibling-fork isolation, and delete/reinsert root + node-tree restoration.

### Rejected alternative and acknowledged trade-off
**Candidate B (192-level Sparse Merkle Tree)** is rejected as the `WorldRootV1` baseline. Its
genuine advantages — simpler fixed-shape recurrence with fewer normalization rules and ~15%–43%
smaller deduplicated 8-key multiproofs (`315–1,883 B` vs `552–2,215 B`) — do not offset its 63× node
amplification, 79× dispersed rebuild cost, and 12×–161× slower point-proof verification.

### Falsifiers
This selection must be reopened if any of the following occurs:
1. Any insertion/deletion sequence or fork schedule produces two distinct `PatriciaNode` structures
   or roots for the same occupied coordinate set.
2. Any malformed, truncated, over-limit, cross-key, or non-canonical witness passes
   `MerklePatriciaTrie::verify_proof_bytes_with_limits` or disagrees with independent Python decode.
3. During `COMMIT-1` integration, Patricia's persistent node-store manifest, pin/GC barrier, or
   crash-recovery assay exceeds the frozen `CommitmentOperationalLimitsDescriptorV1` envelope.
4. During `REFINE-1@commitment`, formalizing Patricia's 4-variant canonical normalization in Lean 4
   uncovers an unresolved soundness ambiguity.

### Reversal and migration cost
- **Before `COMMIT-1` / `GENESIS-D1`:** Zero protocol migration cost — neither candidate is wired
  into live `World`, `world_tag`, or any external genesis/profile artifact. Reversal only requires
  replacing the research decision record before freezing `PROFILE-SCOPE-D0`.
- **After network profile activation:** Reversal requires a versioned `UPGRADE-1` deterministic
  full-state migration from `WorldRootV1` to a new typed root schema (`WorldRootV2`) with
  independent old/new root parity and historical verification support.

## Non-claims

This decision record does **not**:
- wire `MerklePatriciaTrie` into live `World::set_on`/`set_off`, `world_tag`, rendering, collision,
  or `COMMIT-1` (`COMMIT-1` remains `LOCKED` pending `TRANSITION-SPEC-D1`);
- select or validate key/value codecs for non-world maps (`ActiveTokenRootV1`, `EpisodeRootV1`,
  `AuthorizationRootV1`, `NonceRootV1`, `AssetStateRootV1`), each of which requires its own domain
  vectors;
- rank, select, or disqualify the separate `VERKLE-R1` feasibility lane;
- prove persistent node-store durability, mark-and-sweep GC crash safety, network availability,
  consensus finality, or Lean 4 refinement of Rust code.
