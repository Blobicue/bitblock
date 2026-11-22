# BOUNDS-1: protocol admission, DoS, local-VM (`CE-05`), and safe-halt receipt v1

This receipt resolves `PS-074:BOUNDS-1@base=BOUNDED`, unlocking **`PS-078:GENESIS-D1@base=READY`**,
**`PS-039:RESOURCE-1@base=READY`**, and **`PS-047:VM-1@base=READY`** (while keeping
`PS-027:PROTOCOL-ID-1@base` honestly `LOCKED` until `GENESIS-D1` completes).

## Provenance

- **Merged `origin/main` subject (PR #34):** commit `ec5b76358b06351652fde5e9bed05a8d52bb5cad`
  (tree `6fd92a28796d9d5d8abccdaf01dce1b90ab4d7c2`, parent `3ddba5f5dec2f69d17141573fc75596cafa6806e`,
  CI runs `45035548471` & `45035544563`, 10/10 jobs green).
- **Executable module:** [`crates/research/src/bounds_1.rs`](../crates/research/src/bounds_1.rs)
  (`wasm32`-clean, strictly behind `--features protocol-research`).
- **Independent Python reference suite:** [`tools/bounds_1_vectors.py`](../tools/bounds_1_vectors.py)
  (`118/118` protocol schemas independently vectored).
- **CI & mutation evidence:** [`tests/wave5_bounds_1.rs`](../tests/wave5_bounds_1.rs) and
  `python3 tools/bbmutate.py bounds_1` (`24/24` canonical big-endian sites KILLED, 100%).
- **Worst-case stress probe & JSON receipt:** [`examples/bounds_1_stress_probe.rs`](../examples/bounds_1_stress_probe.rs)
  and [`bounds-1-measurement-v1.json`](bounds-1-measurement-v1.json).

## Verified golden tags & committed limits

- **`ProtocolBoundsProfileV1` (`bitblock/bounds-1/protocol-profile/v1`):**
  `#bb.policy.sha256.yecxf5k5a5gs3y7qm76pzax2f5e6bkidmxytmsthsxhdqozzplaa`
  (`max_block_bytes = 2 MiB`, `max_tx_bytes = 16 KiB`, `max_block_txs = 1,024`,
  `max_block_signatures = 2,048`, `max_block_ops = 16,384`, `max_state_writes_per_block = 4,096`
  — tightened below the unverified `65,536` scope draft — `max_proof_bytes = 12 KiB`,
  `max_proof_depth = 256`, `max_hashes_per_block = 1,048,576`, `max_alloc_bytes_per_block = 32 MiB`,
  `max_verify_budget_ms = 400`, per-peer `4 MiB` inflight & `64 req/s`).
- **`LocalVmBoundsV1` (`bitblock/bounds-1/local-vm-bounds/v1`, `CE-05`):**
  `#bb.policy.sha256.o2xw5sig3l5n43lfwyxh5cpsa5kaydtgtrqerw4liahezlrn7j6a`
  (`<= 12` instructions/program, `<= 64` programs/tick, `<= 2,048` gas/program, `<= 65,536` gas/tick,
  `<= 64` stack depth, `<= 1,024` output voxel writes/tick).
- **`SafeHaltEvidenceV1` (`bitblock/bounds-1/safe-halt-evidence/v1`):**
  `#bb.receipt.sha256.blh4qbetfmqq2yu7pq32phgsyu66mqakmf7ffjfmkorzvxkbfvta`
  (deterministic safe-halt emitted when a consensus-accepted artifact breaches committed bounds;
  `tighten_only` forbids silent widening).

## Merged-head release stress measurements (`ec5b763`, ARM64 Linux)

| Workload component | Measured p50 | Measured max | Committed ceiling | Disposition |
| --- | ---: | ---: | ---: | --- |
| `10,000×` streaming preflight (`preflight_block_envelope`) | `0.1034 ms` total (`~10.3 ns`/block) | — | Zero allocation before check | **PASS** |
| `2,048×` strict RFC 8032 Ed25519 verifications | `68.905 ms` | `74.547 ms` | `<= 2,048` sigs/block | **PASS** |
| `4,096×` `COMMIT-1` Patricia `WorldRootV1` state writes | `3.187 ms` | `3.650 ms` | `<= 4,096` writes/block | **PASS** |
| `64×` context-bound `TypedPatriciaProofV1` verifications | `0.430 ms` | `0.478 ms` | `<= 12 KiB`, depth `<= 256` | **PASS** |
| **Combined worst-case block CPU verification** | **`72.523 ms`** | **`78.675 ms`** | **`400 ms` (`20%` of `2 s` slot)** | **PASS (`5.5×` headroom)** |

- **Storage-growth reality (explicitly quantified):** under `28`-epoch rolling retention
  (`28 × 1,024 = 28,672` blocks × `4,096` writes/block), worst-case sustained validator storage
  caps at **`7.0 GiB`** (`64 B/write`) to **`112.0 GiB`** (`1 KiB/write`). Conversely, an
  unpruned archival node sustaining 100% saturated `4,096`-write blocks every `2 s` for 365 days
  would accumulate **`3.76 TiB/yr`** (`64 B/write`) to **`60.15 TiB/yr`** (`61,593.75 GiB/yr` at
  `1 KiB/write`), mathematically demonstrating why `CONSENSUS-D1`'s rolling retention and
  `ASSET-D1`/`RESOURCE-1`'s refundable storage deposit are mandatory.
- **Visual verification:** `bounds_1_budget_chart.png` rendered from the merged-head JSON receipt
  was directly inspected via `read_file`.

## Non-claims

Checked protocol bounds and single-host ARM64 stress measurements do not prove economic fee
sufficiency (`RESOURCE-1`), cross-host x86_64 p99 latency, fair inclusion, chain liveness, or
Layer 1 production security.
