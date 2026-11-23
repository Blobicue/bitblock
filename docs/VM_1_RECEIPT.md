# VM-1: production bounded deterministic spatial interpreter receipt v1

This receipt promotes `PS-047:VM-1@base=IMPLEMENTED` and unlocks **`PS-025:REFINE-1@vm=READY`**
via `PD-03`.

## Provenance

- **Merged `origin/main` subject (PR #44):** commit `4470c5c3df1faa4b658786e6fd4dbfa4f91a7fd6`
  (tree `6a9ce10200fdf7cdf29125db7f558b493a98121c`, parent `482e27a118867502a5de29ded1e9d96661a899a0`,
  CI runs `45090810751` & `45090818732`, 10/10 jobs green).
- **Executable module:** [`crates/research/src/vm_1.rs`](../crates/research/src/vm_1.rs)
  (`wasm32`-clean, strictly behind `--features protocol-research`; zero churn to
  `bitblock-core`, default binary, `world_tag`, `LANGUAGE-R1` v1 glyphs, or `crates/wasm/www/*`).
- **Independent Python reference interpreter:** [`tools/vm_1_vectors.py`](../tools/vm_1_vectors.py)
  (`131/131` protocol schemas independently vectored).
- **CI & mutation evidence:** [`tests/wave9a_vm_1.rs`](../tests/wave9a_vm_1.rs) and
  `python3 tools/bbmutate.py vm_1` (`17/17` canonical big-endian sites KILLED, 100%).
- **Visual execution probe & benchmark:** [`examples/vm_1_visual_probe.rs`](../examples/vm_1_visual_probe.rs)
  and [`vm-1-benchmark-v1.json`](vm-1-benchmark-v1.json).

## Verified golden tags & invariants

- **Step-indexed trace (`VmStepTraceV1`, `bitblock/vm-1/step-trace/v1`):**
  Program A (`PushOne PushOne Add Dup Add PushOne Add Halt = 5`) trace tag
  `#bb.receipt.sha256.zr6heiln6uepluy6k7h6af3idgwgupnezvrk4ovajdf3t4hqegdq` (byte-exact Rust ==
  independent pure-Python reference interpreter; canonical decode round-trip & truncation rejection).
- **Tick execution receipt (`VmTickExecutionReceiptV1`, `bitblock/vm-1/tick-execution-receipt/v1`):**
  - Tick 1 receipt `#bb.receipt.sha256.vka355inmn2tfu6ebpz73rxtu33c3rrkmfyvh2mvnm7olmeilxkq`
    (`115/65,536` gas, `8` writes: Program A writes `5 = 0b0101` clearing stale bit 1, Program B
    traps `GasExhausted` and rolls back atomically leaving sentinel bit 0 untouched, Program C
    writes `3 = 0b0011`).
  - Tick 2 receipt `#bb.receipt.sha256.jcytfmhh2nwadjicgufyo7enlbd4tzdlqugvhtrqr75uhlqjoydq`
    (`101` gas, `4` writes: Program B writes `2 = 0b0010` clearing bit 0 and lighting bit 1;
    Program C collides on B's output register, traps `OutputWriteConflict`, and rolls back atomically).
- **Fail-closed `BOUNDS-1` (`CE-05`) preflight:** `LocalVmBoundsV1::preflight_vm_batch` rejects
  over-bound batches before executing any opcode with zero world mutation and zero `History` commits.
- **Differential, adversarial & invariance coverage:** 120-program seeded differential fuzzing
  matching `vm_d1::schedule_and_evaluate_model` step-for-step, all 7 `VmTrapV1` classes with
  non-mutating trap stack semantics and atomic write rollback, camera-pose invariance, and exact
  `History` commit parity (`history_parity = true`).
- **Release throughput (merged `4470c5c`, 200 runs of 3-program tick):** `37.47 us` p50,
  `38.99 us` p95, `55.09 us` max.
- **Visual verification:** `vm_1_execution_contact_sheet.png` (3× `801x801` 3D snapshots +
  latency panel) rendered on merged `4470c5c` was directly inspected via `read_file`.

## Non-claims

Local deterministic `VM-1` execution does not activate `PROGRAM-STATE-1`, prove utility or program
safety, or grant shared consensus authority (`PROGRAM-CONSENSUS` remains locked).
