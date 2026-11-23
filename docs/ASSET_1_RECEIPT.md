# ASSET-1: dual-domain authenticated ownership and transfer receipt v1

This receipt resolves `PS-038:ASSET-1@base=BOUNDED` (keeping consensus/network value transfer
downstream) and unlocks **`PS-040:ECONOMIC-STATE-1@base=READY`** (while keeping
`VALIDATOR-SCHEDULE-1`, `EXECUTOR-1`, and `EXECUTION-ROOT-1` honestly `LOCKED`).

## Provenance

- **Merged `origin/main` subject (PR #58):** commit `25fbbf645940461531d98d3c5f80235441df09f2`
  (tree `1610998d5a096e45b9e3d54d01d15b071d86ecf9`, parent `f0c0cbca05889a72a067f3e91726acf0b159a06e`,
  CI runs `45111042490` & `45111048541`, 10/10 jobs green).
- **Executable module:** [`crates/research/src/asset_1.rs`](../crates/research/src/asset_1.rs)
  (`wasm32`-clean, strictly behind `--features protocol-research`; zero changes to `crates/core/**`,
  `crates/research/src/commit_state.rs`, default binary, `world_tag`, `crates/wasm/www/*`, or
  `Cargo.lock`).
- **Independent Python reference suite:** [`tools/asset_1_vectors.py`](../tools/asset_1_vectors.py)
  (`148/148` protocol schemas independently vectored; reuses `DomainPatriciaPy`).
- **CI & mutation evidence:** [`tests/wave14a_asset_1.rs`](../tests/wave14a_asset_1.rs)
  and `python3 tools/bbmutate.py asset_1` (`17/17` canonical big-endian sites KILLED, 100%).
- **Release probe:** [`examples/asset_1_conservation_probe.rs`](../examples/asset_1_conservation_probe.rs).

## Verified golden tags & normative invariants

- **Dual-domain authenticated roots (`AssetStateRootsV1`,
  `bitblock/asset-1/asset-state-roots/v1`):**
  - Separately typed `NativeBalanceRootV1` (`DomainIdV1::NativeBalance = 3`) and
    `EpisodeOwnershipRootV1` (`DomainIdV1::EpisodeOwnership = 4`) over `DomainPatriciaTrieV1`;
    cross-domain proof substitution against peer domains and `AUTH-COMMIT-1` rejects fail-closed.
  - Genesis dual-domain roots tag:
    `#bb.receipt.sha256.d4c462nqb6gg3rzzrraufi4cjv752ys33gfmep44wkltiposlvkq`.
  - Post-transfer/burn dual-domain roots tag:
    `#bb.receipt.sha256.yehegtsuyrnm7w6qa3myuotil2egmf6eeagwc2wgmyonqeublfoa`.
- **Proof-carrying batch transition receipts (`AssetTransitionReceiptV1`,
  `bitblock/asset-1/asset-transition-receipt/v1`):**
  - Mint batch receipt tag:
    `#bb.receipt.sha256.vvsftdwi462ds43vitk3d5qx3d3jy2nfxvdk7mupjntewyutrlna`.
  - Transfer + burn/refund batch receipt tag:
    `#bb.receipt.sha256.6s7fa3n5ez24u455z7ossxtldebkmbia3svdwuwhdk4txiccss4q`.
- **Normative ownership, deposit, and conservation gates (`7/7` verified):**
  - `TransferEpisode` atomically transfers both occupancy-episode ownership and the refundable
    `locked_deposit` claim while preserving immutable `mint_episode_id`.
  - Burn-after-transfer by the old owner rejects fail-closed (`RejectedNotEpisodeOwner`) and rolls
    back both asset and `AUTH-COMMIT-1` nonce state; burn by the current owner refunds
    `locked_deposit` and deletes the occupancy key with verifiable absence proof.
  - Bedrock `(0, 0, 0)` holds zero economic record (`RejectedBedrockForbidden`).
  - Concurrent conflicting writes targeting the same coordinate in one batch reject fail-closed
    (`RejectedConcurrentWrite`) with full atomic rollback.
  - Closed supply conservation (`sum(liquid) + sum(locked_deposits) + total_burned_fees ==
    genesis_supply`) and cold full-rebuild Patricia parity hold across every transition.
- **Visual verification:** `asset_1_conservation_chart.png` rendered on merged `25fbbf6` (`3.1 us`
  mint, `2.8 us` transfer, `2.2 us` burn & refund, `0.9 us` verify) was directly inspected via
  `read_file`.

## Non-claims

`BOUNDED` is mandatory: a consistent authenticated asset ledger is not yet a cryptocurrency network
or safe place for production value.
