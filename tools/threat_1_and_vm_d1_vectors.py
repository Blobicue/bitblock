#!/usr/bin/env python3
"""Independent stdlib vectors & reference evaluator for THREAT-1 and VM-D1 (Wave 4).

Imports no Rust/Bitblock code. Independently reconstructs all four Wave 4 canonical schemas:
  * bitblock/threat-1/adversary-model/v1
  * bitblock/threat-1/property-claim-map/v1
  * bitblock/vm-d1/semantics-spec/v1
  * bitblock/vm-d1/model-step-receipt/v1
"""

from __future__ import annotations

import struct
import sys

from crypto_d1_vectors import content_tag
from language_r1_vectors import (
    BEDROCK,
    FRAME_Y,
    FRAME_Z,
    PROGRAM_SCHEMA,
    encode_program_source,
    parse_program,
    stamp_program,
)
from profile_bootstrap_vectors import decode_tag_digest
from semantic_batch_vectors import tag, world_tag

THREAT_MODEL_SCHEMA = b"bitblock/threat-1/adversary-model/v1"
THREAT_CLAIM_MAP_SCHEMA = b"bitblock/threat-1/property-claim-map/v1"
VM_SPEC_SCHEMA = b"bitblock/vm-d1/semantics-spec/v1"
VM_RECEIPT_SCHEMA = b"bitblock/vm-d1/model-step-receipt/v1"

EXPECTED_SCOPE_TAG = "#bb.policy.sha256.yzu3porxqsiiivkwgepymeiuphc7er4wdl226g6go4kfaingn2eq"
OPCODE_GAS = [1, 2, 2, 3, 3, 2, 2, 1]
GAS_PER_OUT_BIT = 10
I64_MIN = -(1 << 63)
I64_MAX = (1 << 63) - 1


def encode_threat_model(scope_digest: bytes) -> bytes:
    return (
        struct.pack(">H", 1)
        + scope_digest
        + struct.pack(
            ">HIIQQQHBBBHHB",
            3333,    # max_corrupt_weight_bps (< 1/3)
            2000,    # target_block_time_ms
            10000,   # gst_max_delay_ms
            1,       # adaptive_corruption_delay_epochs
            21,      # unbonding_delay_epochs
            28,      # rolling_retention_epochs
            8,       # min_honest_peers
            3,       # release_builder_threshold_m
            5,       # release_builder_total_n
            0,       # sequencer_censorship_guaranteed (false)
            3,       # consensus_max_inclusion_window_factor
            128,     # suite_classical_security_bits
            0,       # external_bridges_admitted (false)
        )
    )


def encode_threat_claim_map(model_digest: bytes) -> bytes:
    rows = [
        (1, 3333, 101, 201, 0),
        (2, 10000, 102, 202, 0),
        (3, 1, 103, 203, 0),
        (4, 128, 104, 204, 0),
        (5, 2097152, 105, 205, 0),
        (6, 4096, 106, 206, 0),
        (7, 3, 107, 207, 1),
        (8, 28, 108, 208, 0),
        (9, 3, 109, 209, 0),
        (10, 8, 110, 210, 0),
        (11, 21, 111, 211, 1),
    ]
    out = bytearray(struct.pack(">H", 1) + model_digest)
    for cid, thr, lc, mc, excl in rows:
        out += struct.pack(">BQHHB", cid, thr, lc, mc, excl)
    assert len(out) == 188
    return bytes(out)


def encode_vm_spec() -> bytes:
    flags = 0x0F
    out = struct.pack(">HHBHH", 1, 1, flags, 64, 64)
    for g in OPCODE_GAS:
        out += struct.pack(">H", g)
    out += struct.pack(">H", GAS_PER_OUT_BIT)
    assert len(out) == 27
    return out


def eval_vm_stack(ops: list[int], gas_limit: int, max_stack: int = 64):
    stack: list[int] = []
    gas = 0
    for op in ops:
        step = OPCODE_GAS[op]
        if gas + step > gas_limit:
            return None, 4, gas  # GasExhausted
        gas += step
        if op == 0:
            pass
        elif op == 1:
            if len(stack) >= max_stack:
                return None, 2, gas
            stack.append(1)
        elif op == 2:
            if len(stack) >= max_stack:
                return None, 2, gas
            stack.append(0)
        elif op == 3:
            if len(stack) < 2:
                return None, 1, gas
            b, a = stack.pop(), stack.pop()
            s = a + b
            if not (I64_MIN <= s <= I64_MAX):
                return None, 3, gas
            stack.append(s)
        elif op == 4:
            if len(stack) < 2:
                return None, 1, gas
            b, a = stack.pop(), stack.pop()
            d = a - b
            if not (I64_MIN <= d <= I64_MAX):
                return None, 3, gas
            stack.append(d)
        elif op == 5:
            if not stack:
                return None, 1, gas
            if len(stack) >= max_stack:
                return None, 2, gas
            stack.append(stack[-1])
        elif op == 6:
            if len(stack) < 2:
                return None, 1, gas
            stack[-1], stack[-2] = stack[-2], stack[-1]
        elif op == 7:
            return stack, 0, gas
    return None, 5, gas  # MissingHalt


def main() -> int:
    scope_digest = decode_tag_digest(EXPECTED_SCOPE_TAG)
    model_bytes = encode_threat_model(scope_digest)
    assert len(model_bytes) == 78
    model_tag, model_digest = content_tag("policy", THREAT_MODEL_SCHEMA, model_bytes)
    claim_bytes = encode_threat_claim_map(model_digest)
    claim_tag, _ = content_tag("receipt", THREAT_CLAIM_MAP_SCHEMA, claim_bytes)

    spec_bytes = encode_vm_spec()
    spec_tag, spec_digest = content_tag("policy", VM_SPEC_SCHEMA, spec_bytes)

    # Independent VM-D1 evaluation of golden program [PushOne, PushOne, Add, Dup, Add, Halt] (= 4)
    ops = [1, 1, 3, 5, 3, 7]
    anchor = (3, 0, 2)
    world = {BEDROCK} | stamp_program(anchor, 3, ops)
    pre_world_digest = world_tag(world).digest

    ns, parsed_ops = parse_program(world, anchor)
    src_digest = tag("policy", PROGRAM_SCHEMA, encode_program_source(ns, parsed_ops)).digest
    stack, status, gas = eval_vm_stack(parsed_ops, 100)
    assert status == 0 and stack == [4] and gas == 13
    write_gas = 4 * GAS_PER_OUT_BIT
    gas += write_gas
    out_origin = (5, 8, 2)
    bits = stack[-1] & ((1 << 64) - 1)
    for k in range(4):
        c = (out_origin[0] + k, out_origin[1], out_origin[2])
        assert c != BEDROCK
        assert not (
            anchor[0] <= c[0] < anchor[0] + (4 * len(ops) + 3)
            and anchor[1] <= c[1] < anchor[1] + FRAME_Y
            and anchor[2] <= c[2] < anchor[2] + FRAME_Z
        )
        if (bits >> k) & 1:
            world.add(c)
        else:
            world.discard(c)
    post_world_digest = world_tag(world).digest

    receipt_bytes = (
        spec_digest
        + pre_world_digest
        + post_world_digest
        + struct.pack(">QI", gas, 1)
        + struct.pack(">qqq", *anchor)
        + src_digest
        + struct.pack(">BQQH", 0, gas, bits, 4)
    )
    receipt_tag, _ = content_tag("receipt", VM_RECEIPT_SCHEMA, receipt_bytes)

    # Negative controls: stack underflow, missing halt, gas exhaustion
    assert eval_vm_stack([3, 7], 100)[1] == 1       # StackUnderflow
    assert eval_vm_stack([1, 1, 3], 100)[1] == 5    # MissingHalt
    assert eval_vm_stack([1, 1, 3, 7], 5)[1] == 4   # GasExhausted

    assert model_tag == "#bb.policy.sha256.qnfdid2kwcnzhyxinyitgiu62cimnl5a37ugwjye7wolmu7qurmq", model_tag
    assert claim_tag == "#bb.receipt.sha256.kf6z6jjjbalzphsawlx7b3q7qpy4geyxxpr2vhlpmnnr5k374apq", claim_tag
    assert spec_tag == "#bb.policy.sha256.nvshpvblkenho2me3rp5qwwz4mjrtsssrzz6k7uippu2plpw2ava", spec_tag
    assert receipt_tag == "#bb.receipt.sha256.dhvpeyzrvab6dzqxxkmbkd7agfockzkszfrqgywkkef2jyw2k6aq", receipt_tag
    print(f"threat_1_and_vm_d1_vectors: ok (model={model_tag}, vm_spec={spec_tag}, 4/4 golden tags pinned)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
