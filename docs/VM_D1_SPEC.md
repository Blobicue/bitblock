# VM-D1: frozen deterministic local visual-machine semantics v1

This specification resolves `PS-046:VM-D1@base=BOUNDED` over [`LANGUAGE_R1_RECEIPT.md`](LANGUAGE_R1_RECEIPT.md)
and supplies the pre-execution work formula required by `CE-05` for `BOUNDS-1` and `VM-1`.

## Provenance

- **Merged `origin/main` subject (PR #32):** commit `8bb352dee5cc2c2db7e34634ea2e883cf7e806df`
  (tree `c76bc964ebba793b461d6f6510ec7b00f0769389`, parent `4a88e75b11a60a5140a97572b5da1de00faab963`,
  CI runs `45033334910` & `45033330670`, 10/10 jobs green).
- **Executable reference model:** [`crates/research/src/vm_d1.rs`](../crates/research/src/vm_d1.rs)
  (`wasm32`-clean, strictly behind `--features protocol-research`; zero randomness, zero camera or
  renderer input).
- **Independent Python model evaluator:** [`tools/threat_1_and_vm_d1_vectors.py`](../tools/threat_1_and_vm_d1_vectors.py)
  (`115/115` protocol schemas independently vectored).
- **CI & mutation evidence:** [`tests/wave4_threat_1_and_vm_d1.rs`](../tests/wave4_threat_1_and_vm_d1.rs)
  and `python3 tools/bbmutate.py vm_d1` (`14/14` canonical big-endian sites KILLED, 100%).
- **Visual snapshot exporter:** [`examples/vm_d1_visual_probe.rs`](../examples/vm_d1_visual_probe.rs).

## Frozen instruction, arithmetic, gas, trap & scheduling contract

- **Instruction set:** consumes the exact 8 `LANGUAGE-R1` v1 opcodes untouched (`Nop`, `PushOne`,
  `PushZero`, `Add`, `Sub`, `Dup`, `Swap`, `Halt`) with strict checked signed `i64` stack
  arithmetic (`max_stack_depth = 64`).
- **Pre-execution gas/work formula (`VmSpecD1::pre_execution_work_upper_bound`):**
  `Nop = 1`, `PushOne = 2`, `PushZero = 2`, `Add = 3`, `Sub = 3`, `Dup = 2`, `Swap = 2`,
  `Halt = 1`, plus `10` gas units per emitted output register voxel bit (`1..=64`).
- **Deterministic output register mapping:** on clean `Halt` with `output_register = Some((origin, bit_width))`,
  bit `k in 0..bit_width` of `top_of_stack as u64` emits explicit `On`/`Off` at `origin + (k, 0, 0)`.
  Bedrock `(0,0,0)`, all parsed program frame bounding boxes, `i64` coordinate overflow, and any
  coordinate already claimed by an earlier program in the tick reject fail-closed.
- **Explicit typed traps & atomic rollback (`VmTrapV1`):** `StackUnderflow (1)`,
  `StackOverflow (2)`, `ArithmeticOverflow (3)`, `GasExhausted (4)`, `MissingHalt (5)`,
  `ForbiddenOutputTarget (6)`, `OutputWriteConflict (7)`. Every trap commits **zero** partial
  world writes while recording its exact `gas_used` in `VmBatchReceiptV1`.
- **Multi-program scheduling:** invocations execute in canonical ascending `anchor: Coord` order
  (permutation-invariant; duplicate anchors reject before evaluation), and accepted explicit
  `On`/`Off` writes commit atomically through `History` (`ACCOUNT-1`).

## Verified golden tags & merged-head visual verification (`8bb352d`)

| Artifact | Pinned tag | Verification |
| --- | --- | --- |
| `VmSpecD1::v1_default()` (`bitblock/vm-d1/semantics-spec/v1`) | `#bb.policy.sha256.nvshpvblkenho2me3rp5qwwz4mjrtsssrzz6k7uippu2plpw2ava` | Exact Rust + Python parity |
| Golden execution `[PushOne, PushOne, Add, Dup, Add, Halt]` (`= 4`, `gas = 53`, 4-bit register) | `#bb.receipt.sha256.dhvpeyzrvab6dzqxxkmbkd7agfockzkszfrqgywkkef2jyw2k6aq` | Exact Rust + Python + `History` parity |
| Merged-head 2-program visual showcase (`prog_a = 4`, `prog_b = 3`, `total_gas = 106`) | `#bb.receipt.sha256.ub6yowlofnvjxcq63m4ghzqppfpmplsmrbk4e6knvfn5n4ycopoa` | `vm_d1_before.png` & `vm_d1_after.png` (`801×801`) directly inspected |

Direct inspection of both merged-head PNG snapshots confirmed unclipped orange bedrock `(0,0,0)`,
both framed `LANGUAGE-R1` programs, deterministic `Off` clearing of Program A's pre-existing stale
bit 1 (`x = 6`) alongside `On` lighting of bit 2 (`x = 7`, value `4 = 0b0100`), and `On` lighting
of Program B's bits 0–1 (`x = 5..6`, value `3 = 0b0011`).

## Non-claims

Frozen `VM-D1` semantics do not verify protocol bounds (`BOUNDS-1`), implement the production
dual-executor `VM-1` interpreter, prove Lean 4 refinement (`REFINE-1@vm`), or grant visual
programs consensus authority (`PROGRAM-CONSENSUS`).
