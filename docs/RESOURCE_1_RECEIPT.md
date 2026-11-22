# RESOURCE-1: deterministic EIP-1559 fee burn, capped tip, and refundable deposit receipt v1

This receipt resolves `PS-039:RESOURCE-1@base=BOUNDED` and satisfies conditional design/policy
edges **`CE-08`** (required before `GENESIS-D1` under `economic-charging`) and **`CE-14`**
(`ASSET-D1` dual-domain charging alignment).

## Provenance

- **Merged `origin/main` subject (PR #38):** commit `438f08e0b09f6ba40e829d46b25c9083b830475f`
  (tree `0da6006765b4696db9131a487a7a6a178b71b4a0`, parent `f07f32fc6d87c72352d17769540564ba5e70a354`,
  CI runs `45083510072` & `45083513605`, 10/10 jobs green).
- **Executable module:** [`crates/research/src/resource_1.rs`](../crates/research/src/resource_1.rs)
  (`wasm32`-clean, strictly behind `--features protocol-research`; zero changes to `bitblock-core`
  or `world_tag`).
- **Independent Python reference suite:** [`tools/resource_1_vectors.py`](../tools/resource_1_vectors.py)
  (`122/122` protocol schemas independently vectored).
- **CI & mutation evidence:** [`tests/wave7_resource_1.rs`](../tests/wave7_resource_1.rs) and
  `python3 tools/bbmutate.py resource_1` (`27/27` canonical big-endian sites KILLED, 100%).
- **Economics simulation probe & JSON receipt:** [`examples/resource_1_economics_probe.rs`](../examples/resource_1_economics_probe.rs)
  and [`resource-1-simulation-v1.json`](resource-1-simulation-v1.json).

## Verified golden tags & committed policy

- **`ResourcePolicyV1` (`bitblock/resource-1/fee-policy/v1`):**
  `#bb.policy.sha256.6yvp73mvdkouoxk2xiolnj33lbxucxta5ikhjcwnvd267tzfrfyq`
  - **Calibrated gas weights:** `gas_per_tx_base = 21,000`, `gas_per_tx_byte = 16`,
    `gas_per_ed25519_sig = 35,000` (matching `BOUNDS-1`'s `95%` Ed25519 CPU share),
    `gas_per_state_write = 12,000` (pricing Patricia writes & disk pressure),
    `gas_per_proof_byte = 32`, `gas_per_occupancy_op = 2,000`.
  - **Unified EIP-1559 controller:** `target_block_gas = 50,000,000`, `max_block_gas = 100,000,000`
    (`2×` target; strictly tighter than combining worst-case `BOUNDS-1` ceilings simultaneously),
    `base_fee_change_denominator = 8` (`±12.5%` integer max step, zero floats),
    clamped `[1, 1,000,000,000]`.
  - **Burn, tip, and deposit invariants:** `burn_base_fee = true` (100% burned even on admitted
    transaction failure), `charge_admitted_failure = true`, optional proposer tip strictly capped
    at `max_priority_tip_per_gas = 100,000,000`, refundable `deposit_per_occupied_block = 1,000,000`
    matching `ASSET-D1`, `state_rent_enabled = false`, bedrock `(0, 0, 0)` excluded.
- **`BlockResourceSettlementV1` (`bitblock/resource-1/block-settlement/v1`):**
  `#bb.receipt.sha256.5n46g22vwa6oby2idk57kterg5sra3rx6y5xrf4hbjkaqgm6b7fa`
  (exact bit-for-bit Rust vs pure-Python parity covering admitted success, admitted failure burn,
  headroom-capped tip routing, refundable deposit lock/refund, and exact `u128` conservation).

## Merged-head simulation results (`438f08e`)

| Scenario | Metric | Observed result |
| --- | --- | --- |
| **64-block EIP-1559 burst/recovery** | 24 full (`100M`) → 8 target (`50M`) → 32 quiet (`10M`) blocks | Base fee surges `100 -> 1,630` (`16.3×`), holds flat at `1,630` across target blocks, decays cleanly to `60` |
| **`BOUNDS-1` single-dimension ceilings** | `2,048` Ed25519 sigs / `4,096` writes / `2 MiB` bytes / `16,384` ops / `768 KiB` proofs | `71.68M` / `49.15M` / `33.55M` / `32.77M` / `25.17M` gas (each `< 100M` cap; multi-axis worst case capped below `BOUNDS-1` `72.523 ms`) |
| **16-block write-spam attack** (`4,000` writes/block) | Cumulative capital burden & closed supply conservation | `112.04B` burned base fees + `64.00B` locked refundable deposits + `4.82B` capped proposer tips; exact `u128` conservation verified |

- **Visual verification:** `resource_1_economics_chart.png` rendered from the merged-head JSON
  simulation was directly inspected via `read_file`.

## Non-claims

Deterministic fee schedules and refundable storage deposits do not guarantee liveness, fair
inclusion, price stability, market value, or sustainable validator incentives under adversarial
external economics (`ECONOMIC-STATE-1` and `VALIDATOR-ECON-1` remain `LOCKED`).
