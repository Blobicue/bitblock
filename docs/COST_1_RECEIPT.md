# COST-1@base: browser-game-platform cost envelope receipt v1

This receipt promotes `PS-062:COST-1@base` from `READY` to **`BOUNDED`**, extending the frozen local-reference budget ([`COST_1_BUDGET.md`](COST_1_BUDGET.md), `PS-063:COST-1@budget=BOUNDED`) and `COMMIT-1` durable-store addendum ([`COST_1_COMMIT_1_MEASUREMENTS.md`](COST_1_COMMIT_1_MEASUREMENTS.md)) with canonical browser-game-platform cost budgets and reproducible merged-head measurements for the activated `BITS-ECONOMY-1` creator economy (`Gameladder` + `Bitblock Studio` + `bitblock-light-wasm`).

Raw JSON measurements are retained in [`cost-1-platform-v1.json`](cost-1-platform-v1.json).

## Provenance

- **Merged `origin/main` subject (PR #115):** commit `66d79dbc017f0751322abb94b67a35b47dc9b175` (tree `87f06850f95990906201e7e947f4f4ed27429f63`, parent `3f0838e4489513b8b4dbcb717da7ba5f18431845`, GitHub Actions CI runs `45555898162` & `45555891444`, **10/10 jobs green**).
- **Canonical module & independent vectors:** [`crates/research/src/cost_1.rs`](../crates/research/src/cost_1.rs) (`wasm32`-clean, `bbmutate cost_1`: **25/25 sites KILLED 100%**) + clean-room pure-Python verifier [`tools/cost_1_vectors.py`](../tools/cost_1_vectors.py) (**210/210** independently vectored schemas, `0` without).
- **Pinned policy profile tag:** `#bb.policy.sha256.2lyhmz5uypgxkhwjmvlho7psyiaoc7glzrhtcx4gyikqqgtohrkq` (`46` canonical bytes).
- **Command & host:** `cargo run --release --quiet --features protocol-research --example cost_1_platform_probe` on non-isolated `aarch64-unknown-linux-gnu` (`rust 1.96.0`).

## Measured Browser-Game-Platform Cost Envelope

| Budget Category | Fixed Ceiling (`Cost1PlatformBudgetProfileV1`) | Measured (`64` accounts / `256` accounts, `66d79db`) | Headroom | Disposition |
|---|---:|---:|---:|---|
| Browser light verifier (`bitblock-light-wasm` gzip) | `<= 256,000 B` (`250 KiB`) | `149,181 B` (`145.7 KiB`, SHA-256 `a4f8a6da…`) | `1.72×` | `MET` |
| Daily phone/browser proof-door fetch (`/l1.html`) | `<= 1,048,576 B` (`1 MiB`) | `30,720 B` (`30.0 KiB` over 5 door reads) | `34.1×` | `MET` |
| Signed `BitsPurchaseSplitV1` wire bytes | `<= 512 B` | `367 B` | `1.40×` | `MET` |
| Canonical `BitsStateReceiptV1` wire bytes | `<= 256 B` | `236 B` | `1.08×` | `MET` |
| Domain-3 Patricia wallet inclusion proof | `<= 12,288 B` (`12 KiB`) | `371 B` (`64` accts) / `476 B` (`256` accts) | `25.8×` | `MET` |
| Domain-3 Patricia wallet absence proof | `<= 12,288 B` (`12 KiB`) | `277 B` (`64` accts) / `342 B` (`256` accts) | `35.9×` | `MET` |
| `BitsPurchaseSplitV1` apply latency (`p50` / `max`) | `<= 1,000 us` (`p50`) | `260 / 297 us` (`64`) · `442 / 553 us` (`256`) | `2.26×` | `MET` (ARM64) |
| `verify_bits_wallet_proof` latency (`p50` / `max`) | `<= 500 us` (`p50`) | `4 / 10 us` (`64`) · `2 / 3 us` (`256`) | `125×` | `MET` (ARM64) |
| `BitsTestnetGrantV1` apply latency (`p50`) | informative | `151 us` (`64`) · `211 us` (`256`) | — | measured |
| Sustained `10 TPS` annual raw tx+receipt archive | `<= 262,144 MiB/yr` (`256 GiB/yr`) | `181,353 MiB/yr` (`177.1 GiB/yr`) | `1.45×` | `MET` |

- **64-account receipt tag:** `#bb.receipt.sha256.2nclfommvfk7nsh7bwloyg3j3a4l5tiar7pnucuaqd5s5lxq666a`
- **256-account receipt tag:** `#bb.receipt.sha256.72oqwrg2g6k2f4unalqkr4qxj4s6npntuaxrij6quujxz72k4oma`

## Powered Negative Controls (`0x3F` = 6/6 Verified in `tests/wave35_cost_1.rs`)

1. Tightening `max_wallet_proof_bytes` by `1` byte below observed inclusion proof trips `Miss`.
2. Raising `light_wasm_gzip_bytes` by `1` byte above `256,000 B` trips `Miss`.
3. Raising `purchase_tx_wire_bytes` by `1` byte above `512 B` trips `Miss`.
4. Tightening `max_verify_wallet_proof_us` by `1 us` below observed verification p50 trips `Miss`.
5. Raising `phone_session_door_bytes` by `1` byte above `1 MiB` trips `Miss`.
6. Clearing any powered-control bit (`powered_controls_mask != 0x3F`) forces `Unmeasured` (cannot silently pass `Met`).

## Explicit Non-Claims

- Non-isolated ARM64 timings do not certify quiet-host Windows x86_64 p99 latency, hardware SSD durability, or internet-scale network congestion.
- `COST-1` remains strictly `BOUNDED` and repeats for every newly activated capability (e.g., `PROGRAM-STATE-1`). Testnet/devnet only: `PROD-OPS-1`, `L1-CANDIDATE`, `L1-PRODUCTION`, and `VALUE-1` remain excluded.
