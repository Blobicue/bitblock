# REFINE-1@vm: Rust-to-Lean bounded VM-1 refinement correspondence v1

This record resolves `PS-025:REFINE-1@vm` as **BOUNDED** and closes **`PS-021:REFINE-1@base`** as
**BOUNDED**, completing all **4/4 `REFINE-1` phases** (`world`, `accounting`, `commitment`, and
`vm`: **172 machine-checked Lean 4 `--trust=0` theorems** in total with zero custom axioms).

## Provenance and verified capsules

- **Merged `origin/main` subject (PR #46):** commit `28f89fe29b71149efbb37bf371679d3ae13e931e`
  (tree `2d50fefb0fc6cee57d637298e937f2d0e9308b3c`, parent `64afab3981d6e04b3255f3b3359b80eecc633a85`,
  CI runs `45092955791` & `45092961178`, 10/10 jobs green).
- **Lean 4 `--trust=0` proof capsule (`proof/BitblockProof/VmRefinement.lean`, 35 theorems,
  0 warnings, 0 `sorry`/admit/axiom/unsafe/`native_decide`, axiom footprint `['Quot.sound', 'propext']`
  only under `leanprover/lean4:v4.33.0`):**
  - **8-Opcode Bijection & Gas Schedule (4 theorems):** `opcodeOfCode_opcodeCode`,
    `opcodeCode_opcodeOfCode`, `opcodeGas_pos`, `opcodeGas_le_three`.
  - **Checked Signed 64-Bit Arithmetic & Powered Negative Control (7 theorems):**
    `checkedAdd_sound`, `checkedAdd_none_iff`, `checkedSub_sound`, `checkedSub_none_iff`,
    `checkedAdd_max_one_traps`, `checkedSub_min_one_traps`, and
    `wrappingAdd64_silently_wraps_max` (proving `i64::wrapping_add` silently wraps `i64::MAX + 1`
    to `i64::MIN` while `checkedAdd` traps).
  - **Trap Codes & Single-Step Invariants (3 theorems):** `trapCode_bounds` (`1..=7`),
    `stepOpcode_preserves_depth_bound`, `stepOpcode_preserves_inI64`.
  - **Bounded Termination & Gas Ceilings (7 theorems):** `evalLoop_steps_le_length`,
    `evalProgram_terminates_bounded` (`stepsExecuted <= prog.length`), `evalLoop_gas_monotone`,
    `evalLoop_gas_le_upper_bound`, `evalProgram_gas_le_preExecutionOpGas`, `evalLoop_gas_le_limit`,
    `evalProgram_gas_le_limit`.
  - **Stack-Depth & `InVmI64` Preservation (4 theorems):** `evalLoop_stack_depth_bound`,
    `evalProgram_stack_depth_bound`, `evalLoop_stack_inI64`, `evalProgram_stack_inI64`.
  - **Atomic Rollback Soundness & Powered Negative Control (2 theorems):**
    `commitInvocation_trap_atomic_rollback` (`trap -> emittedWrites = [] ∧ postWorld = preWorld`)
    and `partialWriteLeak_violates_rollback` (showing an un-staged leaky write flips an untouched
    world coordinate despite trapping).
  - **Bedrock/Frame Write Exclusion & Disjoint-Write Commutation (8 theorems):**
    `validateWrites_sound`, `applyWrites_preserves_unwritten_coord`,
    `validated_writes_preserve_forbidden`, `applyEdit_comm_of_ne`, `applyEdit_sameWorld`,
    `applyWrites_sameWorld`, `applyWrites_comm_single`, and `applyWrites_comm_of_disjoint`
    directly over `BitblockProof.World`.
- **Rust correspondence harness:** [`tests/vm_refinement_correspondence.rs`](../tests/vm_refinement_correspondence.rs)
  (3 `protocol-research` integration tests executed in `full` CI on Windows and Linux).
- **Visual verification:** `refine_1_vm_summary_chart.png` generated from the merged `28f89fe`
  Lean 4 `--trust=0` audit was directly inspected via `read_file`.

## Named abstraction function and correspondence map

| Lean 4 model definition | Rust runtime counterpart (`crates/research/src/vm_1.rs`, `vm_d1.rs`) | Correspondence check in `tests/vm_refinement_correspondence.rs` |
| --- | --- | --- |
| `opcodeCode` / `opcodeOfCode` (`Fin 8`) & `opcodeGas` | `SpatialOpcodeV1::{code, from_code}` & `DEFAULT_VM_D1_OPCODE_GAS` | **Exhaustive** check across all 8 valid opcodes (`0..=7`) plus all 248 invalid codes (`8..=255`). |
| `checkedAdd` / `checkedSub` / `wrappingAdd64_silently_wraps_max` | `vm_1::step_opcode` (`i64::checked_add`/`checked_sub`) | **Exhaustive `7x7 = 49`** signed boundary/extrema pairs (`i64::MIN`, `MIN+1`, `-1`, `0`, `1`, `MAX-1`, `MAX`) verifying non-mutating stack on trap plus powered wrapping-addition control. |
| `evalProgram` (`steps <= prog.length`, `gas <= preExecutionOpGas`, `gas <= gasLimit`, `stack.len <= maxDepth`) | `alpha_vm(&evaluate_program_traced(...))` | Step-by-step agreement against `lean_eval_program` across 120 seeded differential programs and all 7 `VmTrapV1` status codes (`1..=7`). |
| `commitInvocation_trap_atomic_rollback` & `partialWriteLeak_violates_rollback` | `LocalSpatialVmV1::execute_tick` atomic write staging | Verifies `emitted_writes == 0`, invariant `world_tag`, and untouched sentinel voxel on trap plus powered leaky-write control. |
| `validated_writes_preserve_forbidden` & `applyWrites_comm_of_disjoint` | `stage_output_writes` bedrock/frame guards & `LocalSpatialVmV1::execute_tick` | Verifies fail-closed `ForbiddenOutputTarget` on bedrock `(0,0,0)` and sibling frames, and exact `world_tag` commutation (`A then B == B then A`) over disjoint output registers. |

## Complete 4/4 `REFINE-1` closure summary

| Phase | Receipt | Lean 4 Capsule (`--trust=0`) | Theorems | Rust Correspondence Harness |
| --- | --- | --- | ---: | --- |
| `REFINE-1@world` | [`REFINE_1_WORLD.md`](REFINE_1_WORLD.md) | `World.lean`, `ChunkRefinement.lean`, `OctreeRefinement.lean` | `89` | `tests/world_refinement_correspondence.rs` |
| `REFINE-1@accounting` | [`REFINE_1_ACCOUNTING.md`](REFINE_1_ACCOUNTING.md) | `AccountingRefinement.lean` | `21` | `tests/accounting_refinement_correspondence.rs` |
| `REFINE-1@commitment` | [`REFINE_1_COMMITMENT.md`](REFINE_1_COMMITMENT.md) | `CommitmentRefinement.lean` | `27` | `tests/commitment_refinement_correspondence.rs` |
| `REFINE-1@vm` | `REFINE_1_VM.md` (this file) | `VmRefinement.lean` | `35` | `tests/vm_refinement_correspondence.rs` |
| **Total** | **`PS-021:REFINE-1@base=BOUNDED`** | **All 4 phases complete (`0` custom axioms)** | **`172`** | **4 Rust correspondence suites** |

## Explicit boundedness and non-claims

`BOUNDED` is mandatory per `CAPABILITY_TREE.md`:
- The Lean 4 theorems hold universally over arbitrary `List VmOpcode`, `List VmOutputWrite`, and
  `Int` values within `InVmI64`, whereas the Rust differential harness exercises exhaustive small
  domains (8 opcodes, 7 traps, 49 signed boundary pairs) and finite randomized/adversarial traces
  (`120` seeded programs, rollback/exclusion/commutation scenarios).
- Neither Lean 4 nor these tests formally extract or verify `rustc`/LLVM compiled machine code,
  SHA-256 collision resistance, program safety/utility, or shared consensus authority
  (`PROGRAM-STATE-1` and `PROGRAM-CONSENSUS` remain locked).
