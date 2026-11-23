# MULTI-1: deterministic multi-actor signed-stream harness receipt v1

This receipt resolves `PS-032:MULTI-1@base=BOUNDED` and unlocks
**`PS-033:SEQUENCER-1@base=READY`** (while keeping `SYNC-1`, `CONSENSUS-1`, `CLIENT-1`,
`VALIDATOR-ECON-1`, `PROFILE-1`, `SIGNER-OPS-1`, `UPGRADE-1`, and `CHAIN-LAUNCH-1` honestly
`LOCKED`).

## Provenance

- **Merged `origin/main` subject (PR #74):** commit `d76b4942e382697d89c340da1535475e51c4f863`
  (tree `c5f14febff63ab9e1aa8803490b0383cd0f2d159`, parent `bcad32ad579fb0f5f735338e1aa382e7bd182126`,
  CI runs `45141916155` & `45141923089`, 10/10 jobs green).
- **Executable module:** [`crates/research/src/multi_1.rs`](../crates/research/src/multi_1.rs)
  (`wasm32`-clean, strictly behind `--features protocol-research`; zero changes to `crates/core/**`,
  `crates/research/src/commit_state.rs`, `crates/research/src/genesis_d1.rs`,
  `crates/research/src/storage.rs`, `crates/research/src/journal.rs`, default binary, `world_tag`,
  `crates/wasm/**`, `Cargo.lock`, or Lean 4 proofs).
- **Clean-room pure-Python verifier:** [`tools/multi_1_vectors.py`](../tools/multi_1_vectors.py)
  (`164/164` protocol schemas independently vectored).
- **CI & mutation evidence:** [`tests/wave19_multi_1.rs`](../tests/wave19_multi_1.rs) and
  `python3 tools/bbmutate.py multi_1` (`40/40` canonical big-endian sites KILLED, 100%).
- **Release probe:** [`examples/multi_1_race_probe.rs`](../examples/multi_1_race_probe.rs).

## Verified golden tags & normative invariants

Both Rust (`MultiActorHarnessV1` wrapping `PrimaryLaunchExecutorV1` + `ExecutionRootCompositionV1`
bootstrapped through `GENESIS-ROOT-PARITY-1`) and the clean-room pure-Python verifier
(`tools/multi_1_vectors.py`) match bit-for-bit over the canonical 3-actor delegated devnet fixture
(`Actor A -> Delegate C -> Actor B`, `chain_id = 77`, 8 signed mixed race/conflict envelopes sorted
canonically by `(priority_tip DESC, principal_actor_id ASC, nonce ASC, envelope_digest ASC)`):

- **Canonical `MultiActorStreamBundleV1` (`bitblock/multi-1/signed-stream-bundle/v1`):**
  `#bb.receipt.sha256.h6deuodmzgmgpkyqv2do2rkaonc7gnn7rjgi5hp3oqwkvs7l5kgq`.
- **Arrival-order-invariant `MultiActorFoldReceiptV1` (`bitblock/multi-1/fold-receipt/v1`):**
  `#bb.receipt.sha256.ssqa3yacg6tg6jk6d7eddccshdlikpzwxrqov6cfr5mcldjqbr3q` (`3` admitted, `5`
  rejected/rolled back).
- **Post-fold 11-slot `ExecutionRootCompositionV1` (`bitblock/execution-root-1/root-composition/v1`):**
  `#bb.receipt.sha256.lbjy45lcbkxe7qle5cwdo3nlvh3qd253kn5m433mphgkmeua5gia`.
- **Arrival-order permutation invariance:** all 24 (`4!`) exhaustive permutations of the first 4
  envelopes plus 16 deterministic SplitMix full-bundle shuffles produce byte-identical
  `MultiActorFoldReceiptV1`, 11-slot `ExecutionRootCompositionV1`, and `MintIdV1` sets.
- **Same-batch burn→re-mint provenance:** burning an occupied episode via depth-1 delegate C and
  re-minting the same coordinate for Actor B in one batch (even under reversed arrival order) yields
  distinct `MintIdV1` provenance and exact `ASSET-1` conservation.
- **Complete 8/8 adversarial conflict & authorization matrix:** covers same-coordinate race winner
  arbitration (`G1`), depth-1 delegation vs depth-2 transitive/scope-violation rejection (`G2`),
  same-batch burn→re-mint (`G3`), stale world anchor (`G4`), duplicate/stale nonce (`G5`), bedrock
  delete no-op (`G6`), admitted balance overdraft atomic rollback (`G7`), and tampered RFC 8032
  signature / cross-chain replay rejection (`G8`).
- **Merged-head release timings (`d76b494`, 50-run p50):** canonical sort + pre-check `10.69 us`,
  full 10-domain fold + 11-slot root composition `574.96 us`, 24-permutation convergence sweep
  `13,799.01 us` (`574.96 us` per-permutation mean).
- **Visual verification:** `multi_1_permutation_chart.png` rendered on merged `d76b494` was
  directly inspected via `read_file`.

## Non-claims

`BOUNDED` is mandatory: multiple actors folded in one process are not networking, decentralization,
Sybil resistance, availability, or consensus.
