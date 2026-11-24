# SUCCESSOR-ACTIVATION-1@base & @activation: internal-only successor closure receipt v1

This receipt resolves `PS-092:SUCCESSOR-ACTIVATION-1@base=BOUNDED` and
`PS-093:SUCCESSOR-ACTIVATION-1@activation=BOUNDED`, satisfying the `CE-44`
in-place successor activation prerequisite for `TESTNET-1` while keeping
`PROD-OPS-1`, `L1-CANDIDATE`, and `L1-PRODUCTION` strictly `LOCKED`.

## Provenance

- **Merged `origin/main` subject (PR #100):** commit `6c7a8f05579804309bb5833a8aaab46b457dfc7c`
  (tree `03ba1cb1b15ae8e7a88bd6ede6f1f685b252d0a8`, parent `343a44d1a0da4d66b6bac644ac5a5f6939730938`,
  CI runs `45300580476` & `45300589929`, 10/10 jobs green across Linux, Windows, `wasm32` V8
  parity, Python vectors, generated index, and Lean 4 `--trust=0`).
- **Executable module:** [`crates/research/src/successor_activation_1.rs`](../crates/research/src/successor_activation_1.rs)
  (`wasm32`-clean, strictly behind `--features protocol-research`, zero new Cargo dependencies,
  zero edits to frozen crates, modules, or existing Lean 4 proof modules).
- **Additive Lean 4 formal proof capsule:**
  [`proof/BitblockProof/SuccessorActivationRefinement.lean`](../proof/BitblockProof/SuccessorActivationRefinement.lean)
  (**33** machine-checked `--trust=0` theorems, `0` warnings, `[propext, Quot.sound]` axioms only,
  raising total refinement/safety/launch/activation `--trust=0` theorems `229 -> 262`, `285` total
  `theorem` declarations across all `11` `BitblockProof/*.lean` modules; linked via `proves` edge
  in `index/edges.jsonl`).
- **Clean-room pure-Python verifier:**
  [`tools/successor_activation_1_vectors.py`](../tools/successor_activation_1_vectors.py) (`198/198`
  protocol schemas independently vectored; `0` without vectors).
- **Machine-readable manifest:**
  [`docs/successor-activation-1-manifest-v1.json`](successor-activation-1-manifest-v1.json).
- **CI & mutation evidence:**
  [`tests/wave32_successor_activation_1.rs`](../tests/wave32_successor_activation_1.rs) and
  `python3 tools/bbmutate.py successor_activation_1` (`33/33` canonical big-endian sites KILLED, 100%).
- **Release probe:**
  [`examples/successor_activation_1_probe.rs`](../examples/successor_activation_1_probe.rs).

## Verified golden tags & checked equations

Both Rust (`SuccessorActivationBundleV1`, `SuccessorActivationSignDocV1`,
`ActivationAuthorizationV1`, `DedicatedMigrationBodyV1`, `SuccessorActivationCarrierV1`,
`preflight_successor_activation_bundle_bytes`, and `verify_successor_activation_carrier`) and
clean-room pure Python (`tools/successor_activation_1_vectors.py`, including independent streaming
preflight, live two-purpose `CONSENSUS-1` certificates, and strict RFC 8032 Ed25519 verification
under `SigningDomainTagV1::GovernanceAction`) match bit-for-bit across all five golden artifacts
for the preventive `v1 -> v2` `bitblock.devnet.v1` successor activation:

- **`SuccessorActivationBundleV1` (`bitblock/successor-activation-bundle/v1`, `|S_s| = 3`, `6,495` B wire):**
  `#bb.receipt.sha256.cxa644n2sko6qt6o7duozk233o44gcnnyewn5fmi675sa3xxdowa`
- **`SuccessorActivationSignDocV1` (`bitblock/successor-activation-sign-doc/v1`, `799` B wire):**
  `#bb.policy.sha256.6iyowm3ua37gjffzadytwdxkfuzpczstvicug62kmoevvyjwfamq`
- **`ActivationAuthorizationV1` (`bitblock/successor-activation-authorization/v1`, `|S_s| = 3`):**
  `#bb.receipt.sha256.75nq5siuuiml4z32d2rspnijjz5vub3rt7nejvmpeks3zdzvfnla`
- **`SuccessorActivationCarrierV1` (`bitblock/successor-activation-1/carrier/v1`, `|S_s| = 3`):**
  `#bb.receipt.sha256.rsp5f3zibybn6kywjbeqjld2i6ccuta6daczhzkwyqacfc22klwq`
- **`SuccessorActivationCarrierV1` minimal explicit-only control (`|S_s| = 0`):**
  `#bb.receipt.sha256.7jwabo3fyxhattv2hjveqsahguti3sdkc3eyivy6352cnf7hrnda`

### Exact `LAYER1_DESIGN_PAPER.md` §2 internal-only equations & 3-stage authority

- **Canonical `|S_s| = 3` (`MigrationSpecV1`, `CandidateBuildInputsV1`, `CandidateReleaseReceiptV1`):**
  - `external_root_count = 0`, `Length(E_s) = 7`, `supplementary_internal_artifact_count_u32 = 3`,
    `internal_artifact_count_u32 = 10 = |D_s|`, `internal_dependency_edge_count_u32 = 23`,
    `internal_canonical_preimage_bytes_u64 = 4,727` (`6,495` B total bundle wire).
- **Minimal `|S_s| = 0` explicit-only control:**
  - `external_root_count = 0`, `Length(E_s) = 7`, `supplementary_internal_artifact_count_u32 = 0`,
    `internal_artifact_count_u32 = 7 = |D_s|`, `internal_dependency_edge_count_u32 = 15`,
    `internal_canonical_preimage_bytes_u64 = 3,911`.
- **Three non-serialized acyclic verifier stages:**
  1. `VerifiedSuccessorPredecessorBaseContextV1`: binds shared predecessor core & governance root
     while retaining strictly distinct `verified_release_parent_eligibility` (`ReleaseAuthority`)
     and `verified_activation_parent_eligibility` (`ActivationAuthority`).
  2. `VerifiedSuccessorReleaseAuthorityContextV1`: verified ONLY after `admitted_release_core_id`
     exists; verifies the live `CONSENSUS-1` `ReleaseAuthority` cert and invokes frozen
     `release_1::verify_admitted_release_record`.
  3. `VerifiedSuccessorActivationAuthorityContextV1`: verified ONLY AFTER the completed internal
     DAG yields `successor_activation_bundle_id`; verifies the distinct live `CONSENSUS-1`
     `ActivationAuthority` cert, `HistoricalAcceptedParentAuthorityUseRefV1`, `3-of-5` Ed25519
     `ActivationAuthorizationV1` (`GovernanceAction`), and exclusive `DedicatedMigrationBodyV1`.
- **16/16 powered fail-closed adversarial mutants (`tests/wave32_successor_activation_1.rs`):**
  off-by-one internal counts/edges, `u64::MAX` overflow, locator tag, external dependency-ref
  injection, trailing bytes, duplicated prior genesis bytes in `S_s`, bidirectional purpose swaps,
  replay-archive substitution, partial occurrence view, Stage 1 core mismatch, tentative status,
  2-of-5 shortfall, non-empty migration transactions, tampered availability commitment, and
  `NoSuccessorActivationV1` all reject fail-closed before migration state changes.
- **Merged-head release timings (`6c7a8f0`):** zero-allocation streaming preflight `0.35 us`,
  full 3-stage `|S_s| = 3` verification `768.66 us`, mutant preflight reject `0.01 us`.
- **Visual verification:** `successor_activation_1_chart.png` rendered on merged `6c7a8f0` was
  directly inspected via `read_file`.

## Non-claims

`BOUNDED` is mandatory and all deterministic devnet `3-of-5` activation signing seeds (`61..=65`)
are explicitly labeled **`DEVNET-TEST-KEYS-NOT-PRODUCTION-CUSTODY`**. A verified successor closure
bundle and Lean 4 abstract refinement model do not authorize themselves, prove arbitrary migration
correctness, guarantee post-activation archival availability, prove physical key custody, or
establish multi-process testnet liveness (`TESTNET-1`).
