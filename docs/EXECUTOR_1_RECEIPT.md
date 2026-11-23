# EXECUTOR-1: independent dual-language transition executor receipt v1

This receipt resolves `PS-065:EXECUTOR-1@base=BOUNDED` and unlocks both
**`PS-030:EXECUTION-ROOT-1@base=READY`** and **`PS-082:UPGRADE-FRAMEWORK-1@base=READY`** (while
keeping `GENESIS-ROOT-PARITY-1`, `MULTI-1`, `PROFILE-1`, `UPGRADE-1`, and `CONSENSUS-1` honestly
`LOCKED`).

## Provenance

- **Merged `origin/main` subject (PR #66):** commit `fe7873ca16eee3c0ab7adbd97b6b36ab8145e526`
  (tree `9b815ad6344c8c3e543e110adc2f965f0911e516`, parent `d18cf4ffb2609f31323795654c068aa6b2646fed`,
  CI runs `45125269954` & `45125274434`, 10/10 jobs green).
- **Primary Rust executor:** [`crates/research/src/executor_1.rs`](../crates/research/src/executor_1.rs)
  (`wasm32`-clean, strictly behind `--features protocol-research`; zero changes to `crates/core/**`,
  `crates/research/src/commit_state.rs`, `crates/research/src/genesis_d1.rs`, default binary,
  `world_tag`, `crates/wasm/**`, `Cargo.lock`, or Lean 4 proofs).
- **Clean-room independent Python executor:** [`tools/executor_1_vectors.py`](../tools/executor_1_vectors.py)
  (zero shared authority-path code; pure stdlib execution over all 10 active launch domains +
  strict RFC 8032 Ed25519 verification; `156/156` protocol schemas independently vectored).
- **CI & mutation evidence:** [`tests/wave16_executor_1.rs`](../tests/wave16_executor_1.rs) and
  `python3 tools/bbmutate.py executor_1` (`42/42` canonical big-endian sites KILLED, 100%).
- **Release probe:** [`examples/executor_1_differential_probe.rs`](../examples/executor_1_differential_probe.rs).

## Verified golden tags & normative invariants

- **Canonical block batch (`ExecutorBlockBatchV1`, `bitblock/executor-1/block-batch/v1`):**
  - Batch 1 (`8` ops: Base voxel + Ed25519 Auth tx + Asset transfer + Asset mint deposit +
    Validator bond + Delayed schedule + INACTIVE Program/VM tag `80` + Unknown tag `200`):
    `#bb.receipt.sha256.fs5gowdymm4iv5rafhzuqk4uh5aoupwiqwxu7wydzthq56jql7pq`.
- **Canonical execution receipts (`ExecutorBlockReceiptV1`, `bitblock/executor-1/execution-receipt/v1`):**
  - Batch 1 receipt (`6` admitted, `2` rejected, `150,552` gas, `301,104` fee burned, Rust == Python
    exact across all 10 active domains):
    `#bb.receipt.sha256.d4ibxky7rkr5257wxkw6vt6nhladett7vrs5l2gkzgzlltazpwca`.
  - Batch 2 receipt (`4` ops: bedrock delete rollback, insufficient-balance rollback, automatic
    Safe-Halt admitted, post-halt schedule blocked):
    `#bb.receipt.sha256.lewpee4g4wfchckl3ibszv5nxdyagqlt4sjwx7ee66tfcx3kf2nq`.
- **Merged-head release timings (`fe7873c`, 50-run p50):** Batch 1 `134.35 us`, Batch 2 `38.53 us`,
  full 10-domain cold-rebuild parity verification `49.26 us`.
- **Normative independence & mutant gates:**
  - True dual-language independence (`PrimaryLaunchExecutorV1` in Rust vs `IndependentPythonExecutorV1`
    in pure Python) with 100% bit-exact agreement on every PRE/POST root across all 10 active launch
    domains (`World`, `ActiveToken`, `Episode`, and Domains `1..=7`) and `program_state_active = false`.
  - Inactive `Program/VM` (`80..=95`) and unassigned (`96..=255`) tags reject with `0` gas, `0`
    write mask, and atomic rollback; fork-context mismatch and non-monotonic heights fail closed;
    all `5/5` powered receipt-mutant classes (omitted step, leaked partial write, inactive band
    admitted, tampered outcomes digest, uncharged admitted work) caught fail-closed.
- **Visual verification:** `executor_1_differential_chart.png` rendered on merged `fe7873c` was
  directly inspected via `read_file`.

## Non-claims

`BOUNDED` is mandatory: two wrappers around the same reducer are not independent, and independent
execution over serialized batches does not establish root composition (`EXECUTION-ROOT-1`), genesis
root parity (`GENESIS-ROOT-PARITY-1`), consensus finality, data availability, or network security.
