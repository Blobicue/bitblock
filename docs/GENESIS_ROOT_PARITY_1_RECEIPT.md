# GENESIS-ROOT-PARITY-1: 4-way preimage-verified logical-genesis to typed-root parity receipt v1

This receipt resolves `PS-031:GENESIS-ROOT-PARITY-1@base=BOUNDED` and unlocks
**`PS-032:MULTI-1@base=READY`** (while keeping `SEQUENCER-1`, `SYNC-1`, `CONSENSUS-1`,
`PROFILE-1`, `UPGRADE-1`, and `CHAIN-LAUNCH-1` honestly `LOCKED`).

## Provenance

- **Merged `origin/main` subject (PR #72):** commit `ac4fe8a88896d474cf29e756913d02511fcc5257`
  (tree `bc47239f771f4afe4139f4a2bf64007d6dec7885`, parent `9afecd035e39d02879cc86cfdced4742e2b3cee9`,
  CI runs `45136005208` & `45136012659`, 10/10 jobs green).
- **Executable module:** [`crates/research/src/genesis_root_parity_1.rs`](../crates/research/src/genesis_root_parity_1.rs)
  (`wasm32`-clean, strictly behind `--features protocol-research`; zero changes to `crates/core/**`,
  `crates/research/src/commit_state.rs`, `crates/research/src/genesis_d1.rs`, default binary,
  `world_tag`, `crates/wasm/**`, `Cargo.lock`, or Lean 4 proofs).
- **Clean-room pure-Python Path 4 rebuilder:** [`tools/genesis_root_parity_1_vectors.py`](../tools/genesis_root_parity_1_vectors.py)
  (`162/162` protocol schemas independently vectored).
- **CI & mutation evidence:** [`tests/wave18_genesis_root_parity_1.rs`](../tests/wave18_genesis_root_parity_1.rs)
  and `python3 tools/bbmutate.py genesis_root_parity_1` (`12/12` canonical big-endian sites KILLED, 100%).
- **Release probe:** [`examples/genesis_root_parity_1_probe.rs`](../examples/genesis_root_parity_1_probe.rs).

## Verified 4-way golden tags & normative invariants

All four independent reconstruction paths — **Path 1** (cold 10-domain Patricia full rebuild),
**Path 2** (initialized incremental `from_genesis()` domain states with `verify_rebuild_parity()`),
**Path 3** (`PrimaryLaunchExecutorV1::from_genesis()` + `EXECUTION-ROOT-1` composition & manifest
correspondence), and **Path 4** (clean-room pure-Python full decode & rebuild in
`tools/genesis_root_parity_1_vectors.py`) — match bit-for-bit across `bitblock.devnet.v1`
(`chain_id = 77`, 2 genesis actors, 4 equal 25% validators, bedrock-only `B_0`, inactive
`ProgramState` sentinel):

- **Decoded `GenesisPackageV1` (`bitblock/genesis-d1/genesis-package/v1`):**
  `#bb.receipt.sha256.3xkh3lfjjgfxg52qhgoqw5zl34zr4d5zax5d6apqufpyiyrpysoa`.
- **Canonical `GenesisPreimageBundleV1` (`bitblock/genesis-root-parity-1/preimage-bundle/v1`):**
  `#bb.policy.sha256.6mcfpltkrno445wrrzhsecwowfll6m5ekyhakh6aw5n2vcdirpsa`.
- **Composed 11-slot `ExecutionRootCompositionV1` (`bitblock/execution-root-1/root-composition/v1`):**
  `#bb.receipt.sha256.uihe33whofr5752ptw2dxfq3tuvtee7ssqrukxxldfolnjsvay2q`
  (`application_root = 43415b3292cf87839607a03b550e469a2f396f960a310274a2813b397d28d577`,
  `execution_state_root = 83252c002106d2c24793984d13a553957344d726a54dc0cd56de1fc4a8bf8f90`).
- **Canonical `GenesisRootParityRecordV1` (`bitblock/genesis-root-parity-1/parity-record/v1`):**
  `#bb.receipt.sha256.xkaeumdnpaz77nlkk3oolxx2surjpvay2ycpksqckx4oddbm67fq`
  (`parity_record_id = 2e72dd4d8c6d61ff0158dc7460a8bdaf75e6d9a381e542bb97a9a80a520a2eaa`).
- **Merged-head release timings (`ac4fe8a`, 50-run p50):** Path 1 cold 10-domain full rebuild
  `143.84 us`, Path 2 incremental `from_genesis()` `111.58 us`, Path 3 `EXECUTOR-1` +
  `EXECUTION-ROOT-1` boot `162.96 us`.
- **Complete 8/8 adversarial mutation gate matrix:** rejects 1-bit policy/descriptor preimage
  tamper (`G1`), omitted actor/validator/domain (`G2`), duplicated entries (`G3`), cross-domain
  `Authorization <-> Nonce` root substitution (`G4`), stale/`CE-43` successor profile substitution
  (`G5`), synthetic-zero inactive slot-6 sentinel (`G6`), self-derived `genesis_id`/`profile_id`
  mutation (`G7`), and non-bedrock `B_0` world mutation (`G8`).
- **Visual verification:** `genesis_root_parity_1_chart.png` rendered on merged `ac4fe8a` was
  directly inspected via `read_file`.

## Non-claims

`BOUNDED` is mandatory: genesis/root parity does not admit the profile (`PROFILE-1`), authenticate
distribution, establish data availability, or prove consensus safety.
