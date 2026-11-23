# EXECUTION-ROOT-1: complete L1 §5 two-layer execution-state root composer receipt v1

This receipt resolves `PS-030:EXECUTION-ROOT-1@base=BOUNDED` and unlocks
**`PS-031:GENESIS-ROOT-PARITY-1@base=READY`** (while keeping `MULTI-1`, `SEQUENCER-1`, `PROFILE-1`,
`UPGRADE-1`, and `CONSENSUS-1` honestly `LOCKED`).

## Provenance

- **Merged `origin/main` subject (PR #68):** commit `6f66ea4652841f2c06b16fb73870ae049b1a2cef`
  (tree `d4c8abf14cbe8f807420413916697e9955c05be4`, parent `71b1ac2f981b6a1196bfe03ac0c2791644e01fb4`,
  CI runs `45128478094` & `45128489785`, 10/10 jobs green).
- **Executable module:** [`crates/research/src/execution_root_1.rs`](../crates/research/src/execution_root_1.rs)
  (`wasm32`-clean, strictly behind `--features protocol-research`; zero changes to `crates/core/**`,
  `crates/research/src/commit_state.rs`, `crates/research/src/genesis_d1.rs`, default binary,
  `world_tag`, `crates/wasm/**`, `Cargo.lock`, or Lean 4 proofs).
- **Independent Python reference suite:** [`tools/execution_root_1_vectors.py`](../tools/execution_root_1_vectors.py)
  (`158/158` protocol schemas independently vectored).
- **CI & mutation evidence:** [`tests/wave17a_execution_root_1.rs`](../tests/wave17a_execution_root_1.rs)
  and `python3 tools/bbmutate.py execution_root_1` (`20/20` canonical big-endian sites KILLED, 100%).
- **Release probe:** [`examples/execution_root_1_composer_probe.rs`](../examples/execution_root_1_composer_probe.rs).

## Verified golden tags & normative invariants

- **Canonical 11-slot execution domain registry (`ExecutionDomainRegistryV1`,
  `bitblock/execution-root-1/domain-registry/v1`):**
  - Launch registry policy tag (slots `1..=5` & `7..=11` active, slot `6` `ProgramState` bound to
    typed `InactiveDomainSentinelV1::UnactivatedLaunchProgramV1`):
    `#bb.policy.sha256.3wkwyds7dgkm5hd4vwj26lkokfi6xkeft6d7yl3hzddm3tq6i62q`.
- **Canonical two-layer root compositions (`ExecutionRootCompositionV1`,
  `bitblock/execution-root-1/root-composition/v1`):**
  - Profile A composition tag (`profile_id = [0xD3; 32]`):
    `#bb.receipt.sha256.klerr3wgoyagb24baztzztdolokhthmpyo4ohbx6bwxb6nzkd2mq`.
  - Profile B composition tag (`profile_id = [0xE4; 32]` — proves L1 §5 cycle-free invariant:
    `application_root` and `execution_state_root` are byte-identical while external
    `ExecutionRootRefV1` differs):
    `#bb.receipt.sha256.n36urqqdjll77eppiiprbnoifbgzruwzv6ztxkqyn22opx6zdwwq`.
- **Merged-head release timings (`6f66ea4`, 200-run p50):** genesis 11-slot composition `23.22 us`,
  post-execution composition `23.25 us`, full header + `BoundProfileManifestV1` correspondence
  verification `30.21 us`.
- **Normative composition & 10/10 adversarial gate controls:**
  - Application layer (`1..=6`) folds into `application_root`; protocol layer (`7..=11`) folds
    alongside `application_root` into `execution_state_root`; `profile_id` and
    `chain_launch_manifest_id` stay strictly outside root preimages and bind externally in
    `ExecutionRootRefV1` and non-interchangeable `AuthenticatedRootRefV1`.
  - Rejects omitted required domains, duplicates, unknown tags, misordered slots, zero/empty
    sentinel substitutes, active bytes in inactive slot `6`, inactive sentinel in active slots,
    swapped domain schemas, header/chain/genesis/world/version tamper, and mixed-manifest (`CE-43`
    successor vs launch registry) publications.
- **Visual verification:** `execution_root_1_composer_chart.png` rendered on merged `6f66ea4` was
  directly inspected via `read_file`.

## Non-claims

`BOUNDED` is mandatory: a complete execution root does not establish logical-genesis parity
(`GENESIS-ROOT-PARITY-1`), valid transaction ordering, consensus finality, data availability, or
correct rendering.
