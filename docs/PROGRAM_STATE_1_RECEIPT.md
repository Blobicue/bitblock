# PROGRAM-STATE-1@base: Domain-2 deterministic code and program-state commitments receipt v1

This receipt promotes `PS-048:PROGRAM-STATE-1@base` from `READY` to **`BOUNDED`**, recording reproducible merged-head provenance, independent clean-room Python vector parity, canonical byte mutation resistance, and powered adversarial gate results for Bitblock's Domain-2 deterministic code and program-state commitment layer.

Raw JSON measurements are retained in [`program-state-1-v1.json`](program-state-1-v1.json).

## Provenance

- **Merged `origin/main` subject (PR #117):** commit `ff8df59bcacc2c31d5ebc9c94cfaff3657178926` (tree `2131b8133279f092bcbd4d7fc3335486d61858dd`, parent `d7f9d32b46f2ad714a43be554c01de073a8b0a14`, GitHub Actions CI runs `45560665769` & `45560658455`, **10/10 jobs green**).
- **Canonical module & independent vectors:** [`crates/research/src/program_state_1.rs`](../crates/research/src/program_state_1.rs) (`wasm32`-clean, `bbmutate program_state_1`: **20/20 sites KILLED 100%**) + clean-room pure-Python verifier [`tools/program_state_1_vectors.py`](../tools/program_state_1_vectors.py) (**213/213** independently vectored workspace schemas, `0` without).
- **Three canonical schemas (byte-exact Rust <-> Python parity):**
  - `bitblock/program-state-1/code-record/v1` (`ProgramCodeRecordV1`): `#bb.policy.sha256.qmmm3akwhwhy42jblombezj6chppxzdfxzrtualsrmone6i3xtfa`
  - `bitblock/program-state-1/state-roots/v1` (`ProgramStateRootsV1`, golden vector): `#bb.receipt.sha256.gbzdt7lyv2ej5ewweyqw3xf6bce4pghgs5vqzs4x5h4piln2lj4q`
  - `bitblock/program-state-1/exec-receipt/v1` (`ProgramExecReceiptV1`, golden vector): `#bb.receipt.sha256.4pvmwu57ahsryqvsiyox5qwgsfnljtdcjx752cynhpag4y2zbqla`
- **Command & host:** `cargo run --release --quiet --features protocol-research --example program_state_1_probe` (`64` runs) on non-isolated `aarch64-unknown-linux-gnu` (`rust 1.96.0`).

## Merged-Head Release Probe Latencies (`ff8df59`, 64 runs)

| Operation | Measured `p50` (ARM64 release) | Notes |
|---|---:|---|
| `verify_proof` (domain-separated Merkle inclusion) | `0.91 us` | Verifies `Code` or `Instance` proof against exact domain prefix |
| `trap_rollback` (atomic rollback on VM trap) | `1.01 us` | Zero storage or state-root mutation on any trap |
| `prove_instance` (Domain-2 instance proof) | `3.38 us` | Sorted Merkle inclusion proof over committed instances |
| `register_code` (deterministic bytecode + SHA-256 `code_id`) | `3.63 us` | Validates bounded `vm_1` opcodes + availability digest |
| `rebuild_parity` (cold full-tree rebuild) | `8.21 us` | Byte-identical `code_root`, `instance_root`, and `state_root` |
| `execute_admitted` (CE-43 Successor `vm_1` step + storage write) | `9.25 us` | Gas-metered execution + canonical `ProgramExecReceiptV1` |

- **Probe `codeTag`:** `#bb.policy.sha256.qmmm3akwhwhy42jblombezj6chppxzdfxzrtualsrmone6i3xtfa`
- **Probe `rootsTag`:** `#bb.receipt.sha256.wvr3k63g6xrjbna3rqb2bgglel6gczugfo7wlkkegvwhxdmutjsq`
- **Probe `execReceiptTag`:** `#bb.receipt.sha256.wtvpxbka7ar2btkkw5o2j6rj62htv43qnh6codkp42gc6jotfzpa`

## Powered Adversarial & Parity Gates (`tests/wave36_program_state_1.rs`)

1. **Launch-profile wall (`PROFILE-D1` + `TRANSITION-SPEC-1`):** `canonical_program_state_profile_specs().0` keeps `program_slot_active == false` and refuses code registration, instantiation, and execution with `ProfileSlotInactive` before any mutation.
2. **CE-43 Successor activation:** `canonical_program_state_profile_specs().1` enables `program_slot_active == true` (`op_tag` band `80..=95`) and executes bounded `vm_1` programs deterministically.
3. **Atomic trap rollback:** `GasExhausted`, `DivideByZero`, `StackUnderflow`, `StackOverflow`, and `SlotLimitExceeded` leave instance storage and `ProgramStateRootsV1` bit-identical (`post_state_root == pre_state_root`).
4. **Domain-separated Merkle proof isolation:** Tampering any leaf, sibling, or key fails verification, and swapping `Code` <-> `Instance` proofs fails closed via distinct SHA-256 domain prefixes.
5. **Cold full-rebuild parity:** Rebuilding `ProgramStateEngineV1` from raw registered code records and instance storage reproduces bit-identical `code_root`, `instance_root`, and composed `state_root`.

## Explicit Non-Claims

- Committed Domain-2 program state does **not** grant shared consensus authority, prove useful or safe user-authored gameplay logic, establish network-wide code availability, or make post-genesis activation governance- or consensus-safe (`PROGRAM-CONSENSUS` remains `LOCKED`).
- Testnet/devnet research scope only: zero changes to `node/` or the running Phase 2 testnet soak; `PROD-OPS-1`, `L1-CANDIDATE`, `L1-PRODUCTION`, and `VALUE-1` remain untouched and excluded.
