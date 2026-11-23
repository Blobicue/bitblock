# GENESIS-D1: reproducible non-self-referential launch genesis receipt v1

This receipt resolves `PS-078:GENESIS-D1@base=BOUNDED`, unlocking **`PS-027:PROTOCOL-ID-1@base=READY`**
while preserving `GENESIS-ROOT-PARITY-1` as the deferred post-commitment root-parity gate.

## Provenance

- **Merged `origin/main` subject (PR #40):** commit `e563e3d484fc660a1b5a0d4e43c9450c75112521`
  (tree `0ed82e04083b1d96346b7dd394ef0f78ca594abe`, parent `b11fb99641e33dd453e24c841d2da1b86a35eaaa`,
  CI runs `45085689796` & `45085694469`, 10/10 jobs green).
- **Executable module:** [`crates/research/src/genesis_d1.rs`](../crates/research/src/genesis_d1.rs)
  (`wasm32`-clean, strictly behind `--features protocol-research`; zero churn to
  `profile_bootstrap.rs`, `bitblock-core`, or `world_tag`).
- **Independent Python reference suite:** [`tools/genesis_d1_vectors.py`](../tools/genesis_d1_vectors.py)
  (`125/125` protocol schemas independently vectored).
- **CI & mutation evidence:** [`tests/wave8a_genesis_d1.rs`](../tests/wave8a_genesis_d1.rs) and
  `python3 tools/bbmutate.py genesis_d1` (`22/22` canonical big-endian sites KILLED, 100%).
- **DAG probe & JSON receipt:** [`examples/genesis_d1_dag_probe.rs`](../examples/genesis_d1_dag_probe.rs)
  and [`genesis-d1-package-v1.json`](genesis-d1-package-v1.json).

## Verified golden tags & immutable launch configuration

- **`BootstrapAvailabilityPolicyV1` (`bitblock/genesis-d1/bootstrap-availability-policy/v1`):**
  `#bb.policy.sha256.c6rrz6ronxsgc7unwdubse4epp7xobqqu4jkzgnghkpel3ozydta`
  (`min_seed_peers = 8`, `archive_retention_epochs = 28`, `max_genesis_chunk_bytes = 2 MiB`,
  `checkpoint_interval_blocks = 1,024`, `require_checksummed_mirror = true`).
- **`LaunchGenesisStateV1` (`bitblock/genesis-d1/launch-state/v1`):**
  - Dual-language parity vector: `#bb.policy.sha256.yzeh72ixpeuu4jyvt3uakqtznxkhmdqbeigcmwl4jlhrsp2dk6va`
  - Full Wave 1–7 devnet launch state: `#bb.policy.sha256.jowjx6wyiayipiaypxgmjc2xgynt335d7mrx3qfx2e6pxegyshgq`
  - Binds `B_0` (`World::new()` bedrock-only `world_tag`), `M_0 = 0`, `E_0 = 0`, `initial_locked_deposits = 0`,
    `A_0`/`N_0 = 0`/`X_0`, `P_0` (`RESOURCE-1` `CE-08`, `ASSET-D1` `CE-06`, `VALIDATOR-ECON-D1` `CE-09`),
    `G_0` (`GOVERNANCE-D1`), `Q_0` (4 deterministic test-genesis validators in `"bitblock.devnet.v1"`,
    `25%` = `2,500 bps <= 3,333 bps` `< 1/3` BFT ceiling, distinct vote/proposal Ed25519 keys),
    validated `BOUNDS-1` limit vector, and `BootstrapAvailabilityPolicyV1`.
  - **Locked launch predicates (7 active / 3 explicitly inactive):** activates `asset-active`,
    `economic-charging`, `in-place-upgrade`, `migration-profile`, `stake-consensus`,
    `validator-economics`, and `validator-rewards`; keeps **`program-active` (on-chain VM)**,
    `post-genesis-program`, and **`verkle-selected`** explicitly **INACTIVE** at genesis (6 active /
    2 inactive logical domain slots).
- **`GenesisPackageV1` (`bitblock/genesis-d1/genesis-package/v1`):**
  - Dual-language parity vector: `#bb.receipt.sha256.adu7bin3lwr47yecbamuvl6zjiy5qxes46dtvwkm3zfwuzmm3rsq`
  - Full Wave 1–7 devnet package: `#bb.receipt.sha256.opfwpshgxba5nu5y4jvuorzlgigbvklier5dymchaauouhvivvgq`
  - Rejects self-reference (`SelfReferenceDetected`), ID mismatch, forbidden domain/predicate
    activation, `> 1/3` validator concentration, unsorted/duplicate entries, non-zero initial
    nonces, reused keys, supply mismatch, and trailing wire bytes fail-closed.
- **Visual verification:** `genesis_d1_lineage_chart.png` rendered on merged `e563e3d` was directly
  inspected via `read_file`.

## Non-claims

Reproducible pre-ID genesis bytes do not prove trusted distribution, fair initial allocation,
secure validator key custody, archive availability, or consensus safety. Commitment root parity
is deliberately deferred to `GENESIS-ROOT-PARITY-1` (remains `LOCKED`).
