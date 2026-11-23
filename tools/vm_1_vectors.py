#!/usr/bin/env python3
"""Independent pure-Python reference interpreter and vector suite for VM-1 (PS-047)."""

from __future__ import annotations

import struct
import sys

from crypto_d1_vectors import content_tag
from language_r1_vectors import PROGRAM_SCHEMA, encode_program_source, stamp_program
from profile_bootstrap_vectors import decode_tag_digest
from semantic_batch_vectors import world_tag as canonical_world_tag

STEP_TRACE_SCHEMA = b"bitblock/vm-1/step-trace/v1"
TICK_RECEIPT_SCHEMA = b"bitblock/vm-1/tick-execution-receipt/v1"

VM_SPEC_TAG = "#bb.policy.sha256.nvshpvblkenho2me3rp5qwwz4mjrtsssrzz6k7uippu2plpw2ava"
LOCAL_VM_BOUNDS_TAG = "#bb.policy.sha256.o2xw5sig3l5n43lfwyxh5cpsa5kaydtgtrqerw4liahezlrn7j6a"

OPCODE_GAS = [1, 2, 2, 3, 3, 2, 2, 1]
GAS_PER_WRITE = 10
I64_MIN = -(1 << 63)
I64_MAX = (1 << 63) - 1


def source_digest(namespace: int, opcodes: list[int]) -> bytes:
    payload = encode_program_source(namespace, opcodes)
    _, d = content_tag("policy", PROGRAM_SCHEMA, payload)
    return d


def eval_program_ref(
    spec_d: bytes, src_d: bytes, opcodes: list[int], gas_limit: int, max_depth: int = 64
) -> tuple[bool, list[int] | int, list[tuple[int, int, int, int, int, int]], int]:
    stack: list[int] = []
    gas = 0
    steps: list[tuple[int, int, int, int, int, int]] = []
    for pc, op in enumerate(opcodes):
        ng = gas + OPCODE_GAS[op]
        if ng > gas_limit:
            top = (stack[-1] & 0xFFFFFFFFFFFFFFFF) if stack else 0
            steps.append((pc, op, 4, gas, len(stack), top))
            return False, 4, steps, gas
        gas = ng
        trap = 0
        halted = False
        if op == 0:
            pass
        elif op == 1:
            if len(stack) >= max_depth:
                trap = 2
            else:
                stack.append(1)
        elif op == 2:
            if len(stack) >= max_depth:
                trap = 2
            else:
                stack.append(0)
        elif op == 3:
            if len(stack) < 2:
                trap = 1
            else:
                s = stack[-2] + stack[-1]
                if s < I64_MIN or s > I64_MAX:
                    trap = 3
                else:
                    stack[-2:] = [s]
        elif op == 4:
            if len(stack) < 2:
                trap = 1
            else:
                s = stack[-2] - stack[-1]
                if s < I64_MIN or s > I64_MAX:
                    trap = 3
                else:
                    stack[-2:] = [s]
        elif op == 5:
            if not stack:
                trap = 1
            elif len(stack) >= max_depth:
                trap = 2
            else:
                stack.append(stack[-1])
        elif op == 6:
            if len(stack) < 2:
                trap = 1
            else:
                stack[-1], stack[-2] = stack[-2], stack[-1]
        elif op == 7:
            halted = True
        top = (stack[-1] & 0xFFFFFFFFFFFFFFFF) if stack else 0
        steps.append((pc, op, trap, gas, len(stack), top))
        if trap:
            return False, trap, steps, gas
        if halted:
            return True, stack, steps, gas
    return False, 5, steps, gas


def encode_trace(
    spec_d: bytes,
    src_d: bytes,
    gas_limit: int,
    final_status: int,
    final_gas: int,
    final_top: int,
    steps: list[tuple[int, int, int, int, int, int]],
) -> bytes:
    out = bytearray()
    out += spec_d + src_d
    out += struct.pack(">QBQQH", gas_limit, final_status, final_gas, final_top, len(steps))
    for pc, op, st, g, depth, top in steps:
        out += struct.pack(">HBBQH Q".replace(" ", ""), pc, op, st, g, depth, top)
    return bytes(out)


def encode_tick_receipt(
    spec_d: bytes,
    bounds_d: bytes,
    pre_d: bytes,
    post_d: bytes,
    tick_gas: int,
    tick_writes: int,
    entries: list[tuple[tuple[int, int, int], bytes, bytes, int, int, int, int]],
) -> bytes:
    out = bytearray()
    out += spec_d + bounds_d + pre_d + post_d
    out += struct.pack(">QHI", tick_gas, tick_writes, len(entries))
    for (ax, ay, az), src_d, tr_d, st, g, res_bits, writes in entries:
        out += struct.pack(">qqq", ax, ay, az) + src_d + tr_d + struct.pack(">BQQH", st, g, res_bits, writes)
    return bytes(out)


def run_golden_scenario() -> tuple[str, str]:
    spec_d = decode_tag_digest(VM_SPEC_TAG)
    bounds_d = decode_tag_digest(LOCAL_VM_BOUNDS_TAG)

    # 3 framed programs matching examples/vm_1_visual_probe.rs & tests/wave9a_vm_1.rs:
    # A (3, 0, 2): PushOne, PushOne, Add, Dup, Add, PushOne, Add, Halt = 5 (0b0101) -> out (5, 8, 2) w=4
    # B (3, 10, 2): PushOne, PushOne, Add, Halt = 2, gas_override=4 -> GasExhausted! (rollback preserves stale voxel!)
    # C (3, 20, 2): PushOne, PushOne, Add, PushOne, Add, Halt = 3 (0b0011) -> out (5, 28, 2) w=4
    ops_a = [1, 1, 3, 5, 3, 1, 3, 7]
    ops_b = [1, 1, 3, 7]
    ops_c = [1, 1, 3, 1, 3, 7]
    world: set[tuple[int, int, int]] = {(0, 0, 0)}
    world |= stamp_program((3, 0, 2), 1, ops_a)
    world |= stamp_program((3, 10, 2), 2, ops_b)
    world |= stamp_program((3, 20, 2), 3, ops_c)
    # Stale sentinel bits: bit 1 of A's output (cleared Off) and bit 0 of B's output (MUST survive rollback!)
    world.add((6, 8, 2))
    world.add((5, 18, 2))
    pre_d = canonical_world_tag(world).digest

    # Evaluate A (limit 2048)
    src_a = source_digest(1, ops_a)
    ok_a, st_a, steps_a, gas_a = eval_program_ref(spec_d, src_a, ops_a, 2048)
    assert ok_a and isinstance(st_a, list) and st_a[-1] == 5
    gas_a += 4 * GAS_PER_WRITE
    tr_a_bytes = encode_trace(spec_d, src_a, 2048, 0, gas_a, 5, steps_a)
    tr_a_tag, tr_a_d = content_tag("receipt", STEP_TRACE_SCHEMA, tr_a_bytes)
    for b in range(4):
        c = (5 + b, 8, 2)
        if (5 >> b) & 1:
            world.add(c)
        else:
            world.discard(c)

    # Evaluate B (tightened gas limit = 4 -> traps GasExhausted on Add; atomic rollback!)
    src_b = source_digest(2, ops_b)
    ok_b, trap_b, steps_b, gas_b = eval_program_ref(spec_d, src_b, ops_b, 4)
    assert not ok_b and trap_b == 4
    tr_b_bytes = encode_trace(spec_d, src_b, 4, 4, gas_b, 0, steps_b)
    _, tr_b_d = content_tag("receipt", STEP_TRACE_SCHEMA, tr_b_bytes)

    # Evaluate C (limit 2048)
    src_c = source_digest(3, ops_c)
    ok_c, st_c, steps_c, gas_c = eval_program_ref(spec_d, src_c, ops_c, 2048)
    assert ok_c and isinstance(st_c, list) and st_c[-1] == 3
    gas_c += 4 * GAS_PER_WRITE
    tr_c_bytes = encode_trace(spec_d, src_c, 2048, 0, gas_c, 3, steps_c)
    _, tr_c_d = content_tag("receipt", STEP_TRACE_SCHEMA, tr_c_bytes)
    for b in range(4):
        c = (5 + b, 28, 2)
        if (3 >> b) & 1:
            world.add(c)
        else:
            world.discard(c)

    assert (5, 18, 2) in world, "Program B rollback must preserve untouched sentinel voxel"
    post_d = canonical_world_tag(world).digest
    entries = [
        ((3, 0, 2), src_a, tr_a_d, 0, gas_a, 5, 4),
        ((3, 10, 2), src_b, tr_b_d, 4, gas_b, 0, 0),
        ((3, 20, 2), src_c, tr_c_d, 0, gas_c, 3, 4),
    ]
    rec_bytes = encode_tick_receipt(
        spec_d, bounds_d, pre_d, post_d, gas_a + gas_b + gas_c, 8, entries
    )
    rec_tag, _ = content_tag("receipt", TICK_RECEIPT_SCHEMA, rec_bytes)
    return tr_a_tag, rec_tag


EXPECTED_TRACE_A_TAG = "#bb.receipt.sha256.zr6heiln6uepluy6k7h6af3idgwgupnezvrk4ovajdf3t4hqegdq"
EXPECTED_TICK1_RECEIPT_TAG = "#bb.receipt.sha256.vka355inmn2tfu6ebpz73rxtu33c3rrkmfyvh2mvnm7olmeilxkq"


def main() -> int:
    tr_a_tag, rec_tag = run_golden_scenario()
    if "--print-tags" in sys.argv:
        print(f"TRACE_A_TAG={tr_a_tag}")
        print(f"TICK1_RECEIPT_TAG={rec_tag}")
    assert tr_a_tag == EXPECTED_TRACE_A_TAG, tr_a_tag
    assert rec_tag == EXPECTED_TICK1_RECEIPT_TAG, rec_tag

    # Negative control: perturbing final_gas_used by 1 flips trace tag.
    spec_d = decode_tag_digest(VM_SPEC_TAG)
    src_a = source_digest(1, [1, 1, 3, 5, 3, 1, 3, 7])
    _, _, steps_a, gas_a = eval_program_ref(spec_d, src_a, [1, 1, 3, 5, 3, 1, 3, 7], 2048)
    bad_tr_tag, _ = content_tag(
        "receipt",
        STEP_TRACE_SCHEMA,
        encode_trace(spec_d, src_a, 2048, 0, gas_a + 41, 5, steps_a),
    )
    assert bad_tr_tag != tr_a_tag
    print(f"vm_1_vectors: ok (trace_a={tr_a_tag}, tick1={rec_tag}, 2/2 golden tags pinned)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
