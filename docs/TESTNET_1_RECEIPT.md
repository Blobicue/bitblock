# TESTNET-1@base: replicated 6-OS-process adversarial testnet receipt v1

This receipt resolves `PS-058:TESTNET-1@base=BOUNDED` (including `CE-44` live `v1 -> v2`
in-place successor activation across six independent operating-system processes) and unlocks
`PS-094:PROD-OPS-1@base=READY`, while keeping `L1-CANDIDATE`, `L1-PRODUCTION`, and `VALUE-1`
strictly `LOCKED` and leaving production-only `G18` owned by `PROD-OPS-1`.

## Provenance

- **Merged `origin/main` subject (PR #102):** commit `de0fc44a90ea667cdc8df8ce1f8a1c1e60edf863`
  (tree `4db4f1ebc01127025fe6f069997dff61f88d9488`, parent `f8b05c24e9f62d459b91aa6b9b1d794f62f6964f`,
  CI runs `45341374594` & `45341380984`, 10/10 jobs green across Linux, Windows, `wasm32` V8
  parity, Python vectors, generated index, and Lean 4 `--trust=0`).
- **Executable module & standalone OS node binary:**
  [`crates/research/src/testnet_1.rs`](../crates/research/src/testnet_1.rs) (`wasm32`-clean core
  types, canonical codecs, stage×gate matrix validation, and native `testnet_proc` loopback TCP
  RPC + 6-process OS orchestrator behind `--features protocol-research`) and
  [`examples/bin/testnet_1_node.rs`](../examples/bin/testnet_1_node.rs) (`[[bin]] bitblock-testnet-node`,
  `default-run = "bitblock"`, zero edits to `src/main.rs` or any frozen crate/module, zero new
  Cargo dependencies).
- **Clean-room pure-Python verifier:**
  [`tools/testnet_1_vectors.py`](../tools/testnet_1_vectors.py) (**200/200** protocol schemas
  independently vectored across the entire workspace; `0` without vectors).
- **Machine-readable manifest:**
  [`docs/testnet-1-manifest-v1.json`](testnet-1-manifest-v1.json).
- **CI & mutation evidence:**
  [`tests/wave33_testnet_1.rs`](../tests/wave33_testnet_1.rs) (live 6-OS-process adversarial
  cluster + 7th respawned validator PID + 7th tampered-boot halt control + 8 fail-closed
  boot/matrix/ambient-registry controls, green on both `ubuntu-latest` and `windows-latest`) and
  `python3 tools/bbmutate.py testnet_1` (`22/22` canonical big-endian sites KILLED, 100%).
- **Release probe:**
  [`examples/testnet_1_orchestrator_probe.rs`](../examples/testnet_1_orchestrator_probe.rs).

## Verified golden tags & 6-OS-process adversarial coverage

Both Rust (`TestnetClusterConfigV1`, `TestnetCoverageReportV1`, `verify_testnet_cluster_config`,
and `run_adversarial_testnet_1_cluster`) and clean-room pure Python (`tools/testnet_1_vectors.py`)
match bit-for-bit across both newly vectored schemas, bringing Bitblock to **200/200**
independently vectored protocol schemas:

- **`TestnetClusterConfigV1` (`bitblock/testnet-1/cluster-config/v1`, `1,225` B wire):**
  `#bb.policy.sha256.rospzqtyzq4vgsd6j6eqjsi6tutvsck7gsj7wpqxerx3jqryc5ja`
- **`TestnetCoverageReportV1` (`bitblock/testnet-1/coverage-report/v1`, `499` B wire):**
  `#bb.receipt.sha256.hwsoj2bhgxxyuj5jportmnicjwpxdhvodxi7vktgyctcvkiikt7a`

### Live 6-process OS topology, `CE-44` switchover & 10/10 fault mask

- **Six independent OS processes over loopback TCP (`BBTN1` framing):** four validators
  (`V1..V4`), one independent full verifier, and one archive/recovery node — every process loads
  the exact same authorized `CHAIN-LAUNCH-1` package and closure bundle plus isolated on-disk
  `STORAGE-1` journals and kernel-locked `SIGNER-OPS-1` watermarks.
- **Admitted 8-row stage×gate-subclaim matrix:** names every owner, artifact, and stop condition
  with zero unresolved conditionals (`G5.core`, `G15.resource`, active economic/consensus/sync
  subclaims, `CE-44` `in-place-upgrade`, explicit inactive exclusions for `program-active` and
  `verkle-selected`, and production-only `G18` explicitly owned by `PROD-OPS-1`).
- **10/10 adversarial fault & lifecycle controls passed (bitmask decimal `1023`, all 10 bits set):**
  1. Pre-upgrade `v1` BFT finality across heights `1..=3` across all 6 OS processes.
  2. Real OS `Child::kill()` crash of validator `V4` + 3-of-4 partition finality.
  3. Validator `V4` respawn under a fresh OS PID (`2297878` in merged release probe) with on-disk
     `STORAGE-1` journal replay and kernel-locked `SIGNER-OPS-1` watermark reload.
  4. Anti-double-sign equivocation refusal at previously signed height/round.
  5. Wrong-root block rejection across verifiers.
  6. 7th OS process booted with tampered launch-closure identity halts fail-closed before height 1.
  7. Live `CE-44` `UPGRADE-1 -> SUCCESSOR-ACTIVATION-1` `v1 -> v2` switchover at height `4` across
     all 6 OS processes using the exact verified `SuccessorActivationCarrierV1` (`|S_s| = 3`).
  8. Post-upgrade `v2` BFT finality across heights `5..=6` across all 6 OS processes.
  9. Post-upgrade stale `v1` downgrade rejection.
  10. Archive-node historical `v1` block verification after `v2` activation.
- **8/8 additional fail-closed boot/matrix/ambient-registry controls:** missing launch preimage,
  substituted launch/successor reference, ambient successor registry injection, unresolved
  conditional row, missing `G5.core`/`G15.resource`, `G18` ownership theft, and inactive-feature
  activation all reject fail-closed before height 1 or activation.
- **Merged-head release timings (`de0fc44`, 6 real OS processes on Linux):**
  - Boot 6 OS processes + pre-upgrade `v1` (`h1..h3`): `93.63 ms`
  - Real OS kill + 3-of-4 partition + respawn/replay + fault drills: `181.62 ms`
  - Live `CE-44` `v1 -> v2` switchover (`h4` across all 6 OS processes): `30.87 ms`
  - Post-upgrade `v2` (`h5..h6`) + downgrade reject + archive historical `v1`: `115.18 ms`
  - End-to-end 6-OS-process adversarial run total: `431.32 ms`
- **Visual verification:** `testnet_1_multiprocess_chart.png` rendered on merged `de0fc44` was
  directly inspected via `read_file`.

## Non-claims

`BOUNDED` is mandatory and all deterministic devnet validator/governance/release signing seeds are
explicitly labeled **`DEVNET-TEST-KEYS-NOT-PRODUCTION-CUSTODY`**. A loopback multi-process
adversarial testnet run, hash agreement, or clean `CE-44` switchover does not establish production
security, wide-area internet latency/eclipse resilience, physical HSM custody, third-party
cryptographic/code audit clearance, or production operations readiness (`PROD-OPS-1`, `G18`,
`L1-CANDIDATE`, and `L1-PRODUCTION` remain separate gates).
