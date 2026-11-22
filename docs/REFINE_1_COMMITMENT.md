# REFINE-1@commitment: Rust-to-Lean Patricia commitment correspondence v1

This record resolves `PS-024:REFINE-1@commitment` as **BOUNDED** (keeping `PS-021:REFINE-1@base=READY`
and `PS-025:REFINE-1@vm=LOCKED` until `VM-1` exists). It connects the verified Lean 4 mathematical
sign-biased key bijection, canonical binary Merkle Patricia Trie uniqueness, and point-proof
soundness/completeness theorems directly to the live Rust `bitblock::commit_state::TypedPatriciaTrie`
and `bitblock::commit_r1::CoordinateKeyV1` implementations via a named abstraction function,
signed-extrema pair checks, exhaustive `4! = 24` permutation checks over 192-bit and 256-bit tries,
delete-after-insert restoration, and powered non-canonical shape/proof negative controls.

## Provenance and verified capsules

- **Merged `origin/main` subject:** commit `e22312bfb778adce1cc7ba3917e368765e91d8e9`
  (tree `7481353f1996ea5d1883f44a36136884da80bb2c`, PR #18, CI runs `44994236191` & `44994240648`,
  10/10 jobs green).
- **Lean 4 `--trust=0` proof capsule (`proof/BitblockProof/CommitmentRefinement.lean`, 27 theorems,
  0 warnings, 0 `sorry`/custom axioms under Lean `v4.33.0`):**
  - `CoordinateKeyV1` sign-bit biasing over `InI64Range = [-2^63, 2^63 - 1]`: exact bijection and
    range bounds (`biasSigned64_lt_mod64`, `unbiasSigned64_biasSigned64`,
    `biasSigned64_unbiasSigned64`, `unbiasSigned64_in_i64`), strict signed order preservation
    (`biasSigned64_lt_iff`), exact scalar boundary vectors (`biasSigned64_boundary_vectors`), and
    powered negative control (`rawTwosComp64_breaks_signed_order`).
  - Canonical fixed-width binary Merkle Patricia Trie (`NonEmptyCanonical rem t`, `CanonicalPat rem t`,
    parametric over bit-width `rem`, simultaneously covering 192-bit `WorldRootV1`/`ActiveTokenRootV1`
    and 256-bit `EpisodeRootV1`) with named abstraction function `alphaPat : PatNode -> List Bool ->
    Option PatVal`.
  - **Structural uniqueness (`nonEmptyCanonical_unique_of_alpha_eq` & `canonicalPat_unique_of_alpha_eq`):**
    any two canonical Patricia tries over bit-width `rem` with equal lookup semantics `alphaPat` on
    all keys of length `rem` are syntactically identical (`t1 = t2`), yielding insertion-order
    independence, incremental == full-rebuild parity, and delete-after-insert prior-root restoration
    as direct corollaries.
  - Point-proof soundness and completeness (`PatProof`, `provePat`, `verifyPatProof`):
    `verifyPatProof_sound`, `nonEmptyCanonical_provePat_complete`, `canonicalPat_provePat_complete`.
  - Six powered negative controls: `adjacent_extensions_not_canonical`,
    `extension_to_leaf_not_canonical`, `empty_under_branch_not_canonical`,
    `empty_extension_path_not_canonical`, `verifyPatProof_rejects_extStep_to_leaf`, and
    `verifyPatProof_rejects_nondivergent_ext`.
- **Rust correspondence harness:** `tests/commitment_refinement_correspondence.rs` (3
  `protocol-research` integration tests executed in `full` CI on Windows and Linux).

## Named abstraction function and correspondence map

| Lean 4 model definition | Rust runtime counterpart (`crates/research/src/commit_state.rs`, `commit_r1.rs`) | Correspondence check in `tests/commitment_refinement_correspondence.rs` |
| --- | --- | --- |
| `biasSigned64` / `unbiasSigned64` / `biasSigned64_lt_iff` | `CoordinateKeyV1::from_coord` / `to_coord` (XOR `1 << 63` big-endian) | Exact 5 scalar boundary byte vectors (`i64::MIN`, `-1`, `0`, `1`, `i64::MAX`), `13^2 = 169` signed boundary/extrema pair comparisons, and powered raw two's-complement negative control (`(0u64) < (-1i64 as u64)`). |
| `alphaPat : PatNode -> List Bool -> Option PatVal` | `alpha_commitment(&TypedPatriciaTrie, &[Vec<u8>]) -> BTreeMap<Vec<u8>, Vec<u8>>` | Logical map equality checked alongside canonical root and node-record set equality across all permutations and delete restoration. |
| `canonicalPat_unique_of_alpha_eq` (`rem = 192` & `256`) | `TypedPatriciaTrie::{empty, from_entries, upsert, delete, root, collect_node_records}` | **All `4! = 24` insertion permutations** for both 192-bit `WorldRootV1` (including signed extrema and depth-192 zero-suffix twins `(0,0,0)`/`(0,0,1)`) and 256-bit `EpisodeRootV1` (including depth-256 twins) produce identical roots AND identical canonical node records as cold `from_entries`, plus exact delete-after-insert restoration. |
| `verifyPatProof_sound` & `canonicalPat_provePat_complete` | `TypedPatriciaTrie::prove` & `verify_lookup` (`ExpectedProofContextV1`) | Verifies all present keys plus divergent-extension and leaf-divergence absence probes on 256-bit `EpisodeRootV1` against `TypedPatriciaTrie::get`. |
| Six powered negative controls (`adjacent_extensions`, `extension_to_leaf`, `empty_under_branch`, `empty_extension_path`, `extStep_to_leaf`, `nondivergent_ext`) | `verify_lookup` & `decode_and_verify_node_record` | Constructs explicit forged `TypedPatriciaProofV1` and malformed node records for every control and asserts fail-closed rejection in Rust. |

## Explicit boundedness and non-claims

`BOUNDED` is mandatory per `CAPABILITY_TREE.md`:
- The Lean 4 theorems hold universally over all `InI64Range` integers and all `CanonicalPat rem`
  trees of arbitrary bit-width `rem`, whereas the Rust differential tests exercise finite boundary,
  permutation, and adversarial corpora (`169` signed pairs, `24` 192-bit permutations, `24` 256-bit
  permutations, and explicit forged proofs/records).
- Neither Lean 4 nor these tests formally extract or verify `rustc`/LLVM compiled machine code,
  SHA-256 collision resistance, physical sudden-power filesystem durability, consensus finality, or
  data availability.
- VM refinement (`REFINE-1@vm`) remains `LOCKED` on `VM-1`.
