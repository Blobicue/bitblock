#!/usr/bin/env python3
"""Independent stdlib Python vectors for TRANSITION-SPEC-1 (PS-080).

Imports no Rust/Bitblock code. Independently reconstructs both Wave 12C canonical schemas:
  * bitblock/transition-spec-1/resolved-profile-spec/v1
  * bitblock/transition-spec-1/spec-eval-receipt/v1
and pins byte-exact ContentTag agreement with `crates/research/src/transition_spec_1.rs` and
`tests/wave12c_transition_spec_1.rs` across both Initial Launch and CE-43 Successor profiles.
"""

from __future__ import annotations

import hashlib
import struct
import sys

from crypto_d1_vectors import content_tag
from profile_bootstrap_vectors import decode_tag_digest
from profile_d1_vectors import (
    EXPECTED_LAUNCH_CAND_TAG,
    EXPECTED_SUCC_CAND_TAG,
    PROTOCOL_BOUNDS_TAG,
    RESOURCE_POLICY_TAG,
)

RESOLVED_SPEC_SCHEMA = b"bitblock/transition-spec-1/resolved-profile-spec/v1"
EVAL_RECEIPT_SCHEMA = b"bitblock/transition-spec-1/spec-eval-receipt/v1"
BASE_CONTRACT_SCHEMA = b"bitblock/transition-spec-d1/base-contract/v1"

EXPECTED_LAUNCH_SPEC_TAG = (
    "#bb.policy.sha256.xhy32kud2wdzknglo6p2n3gulbnlikjn3eqhmmquzl2jcofjg6wa"
)
EXPECTED_SUCC_SPEC_TAG = (
    "#bb.policy.sha256.ny3mlc3tk7f7ebqexdcz3azhleksz554tgoh2rrqohjp3bx6ldta"
)
EXPECTED_LAUNCH_EVAL_RECEIPT_TAG = (
    "#bb.receipt.sha256.ekrbspma4yaxej5loltysz3bznmy3ksyvqi3aaavui2fjvvjr3tq"
)
EXPECTED_SUCC_EVAL_RECEIPT_TAG = (
    "#bb.receipt.sha256.ozw6nmwf2si4ukpj25l6ppiacte5xfiwd7npftacave272nzissq"
)

BASE_MASK = 0b111
AUTH_MASK = (1 << 3) | (1 << 4)
ASSET_MASK = (1 << 5) | (1 << 6)
VAL_MASK = (1 << 7) | (1 << 8)
GOV_MASK = 1 << 9
PROG_MASK = 1 << 10


def base_contract_digest() -> bytes:
    payload = struct.pack(">HIQqqqIH", 1, 65_536, 1, 0, 0, 0, BASE_MASK, 0)
    assert len(payload) == 44
    _, digest = content_tag("policy", BASE_CONTRACT_SCHEMA, payload)
    return digest


def encode_band(
    slot_id: int,
    op_min: int,
    op_max: int,
    disp_min: int,
    disp_max: int,
    active: bool,
    conflict: int,
    rmask: int,
    wmask: int,
    base_gas: int,
    per_byte_gas: int,
    pred_gate: int,
) -> bytes:
    out = struct.pack(
        ">HBBBBBBIIQQB",
        slot_id,
        op_min,
        op_max,
        disp_min,
        disp_max,
        1 if active else 0,
        conflict,
        rmask,
        wmask,
        base_gas,
        per_byte_gas,
        pred_gate,
    )
    assert len(out) == 33
    return out


def build_bands(program_active: bool) -> list[tuple[int, int, int, int, int, bool, int, int, int, int, int, int]]:
    return [
        (1, 16, 31, 16, 31, True, 1, BASE_MASK | AUTH_MASK, AUTH_MASK, 21_000, 16, 0),
        (2, 32, 47, 32, 47, True, 0, BASE_MASK | AUTH_MASK | ASSET_MASK, ASSET_MASK, 25_000, 16, 1),
        (3, 48, 63, 48, 63, True, 1, BASE_MASK | AUTH_MASK | ASSET_MASK | VAL_MASK, VAL_MASK, 35_000, 16, 0),
        (4, 64, 79, 64, 79, True, 0, BASE_MASK | AUTH_MASK | ASSET_MASK | VAL_MASK | GOV_MASK, GOV_MASK, 40_000, 16, 0),
        (5, 80, 95, 80, 95, program_active, 0, BASE_MASK | AUTH_MASK | ASSET_MASK | VAL_MASK | GOV_MASK | PROG_MASK, PROG_MASK, 50_000, 32, 2),
    ]


def encode_resolved_spec(profile_id: bytes, program_active: bool) -> bytes:
    base_d = base_contract_digest()
    bounds_d = decode_tag_digest(PROTOCOL_BOUNDS_TAG)
    res_d = decode_tag_digest(RESOURCE_POLICY_TAG)
    bands = build_bands(program_active)
    out = (
        struct.pack(">H", 1)
        + profile_id
        + base_d
        + bounds_d
        + res_d
        + struct.pack(">IQQIBH", 16_384, 100_000_000, 2_000, BASE_MASK, 1 if program_active else 0, len(bands))
    )
    for b in bands:
        out += encode_band(*b)
    return out


def eval_admitted_receipt(
    spec_tag_digest: bytes,
    profile_id: bytes,
    program_active: bool,
    block_height: int,
    pre_root: bytes,
    ops: list[tuple[int, bytes, bytes]],
) -> bytes:
    bands = build_bands(program_active)
    staged: dict[tuple[int, bytes], bytes] = {}
    disps: list[int] = []
    total_gas = 0
    for op_tag, target_key, payload in ops:
        if op_tag <= 15:
            wmask, gas, dcode = BASE_MASK, 2_000 + len(payload) * 16, 0
        else:
            band = next(b for b in bands if b[1] <= op_tag <= b[2])
            assert band[5], f"inactive band for op_tag={op_tag}"
            wmask, gas, dcode = band[8], band[9] + len(payload) * band[10], band[3]
        total_gas += gas
        eff = hashlib.sha256(
            b"bb.ts1.op_effect.v1" + bytes([op_tag]) + target_key + struct.pack(">H", len(payload)) + payload
        ).digest()
        staged[(wmask, target_key)] = eff
        disps.append(dcode)

    eff_h = hashlib.sha256()
    eff_h.update(b"bb.ts1.effects.v1")
    eff_h.update(struct.pack(">I", len(disps)))
    eff_h.update(bytes(disps))
    for (wmask, key) in sorted(staged.keys()):
        eff_h.update(struct.pack(">I", wmask))
        eff_h.update(key)
        eff_h.update(staged[(wmask, key)])
    effects_digest = eff_h.digest()
    post_root = hashlib.sha256(b"bb.ts1.post_root.v1" + pre_root + effects_digest).digest()

    return (
        struct.pack(">H", 1)
        + spec_tag_digest
        + profile_id
        + struct.pack(">QB", block_height, 0)
        + pre_root
        + post_root
        + struct.pack(">QI", total_gas, len(disps))
        + effects_digest
        + struct.pack(">H", len(disps))
        + bytes(disps)
    )


def main() -> int:
    launch_pid = decode_tag_digest(EXPECTED_LAUNCH_CAND_TAG)
    succ_pid = decode_tag_digest(EXPECTED_SUCC_CAND_TAG)

    launch_spec_bytes = encode_resolved_spec(launch_pid, False)
    launch_spec_tag, launch_spec_d = content_tag("policy", RESOLVED_SPEC_SCHEMA, launch_spec_bytes)
    assert launch_spec_tag == EXPECTED_LAUNCH_SPEC_TAG, (
        f"launch spec mismatch: {launch_spec_tag} != {EXPECTED_LAUNCH_SPEC_TAG}"
    )

    succ_spec_bytes = encode_resolved_spec(succ_pid, True)
    succ_spec_tag, succ_spec_d = content_tag("policy", RESOLVED_SPEC_SCHEMA, succ_spec_bytes)
    assert succ_spec_tag == EXPECTED_SUCC_SPEC_TAG, (
        f"succ spec mismatch: {succ_spec_tag} != {EXPECTED_SUCC_SPEC_TAG}"
    )
    assert launch_spec_tag != succ_spec_tag

    launch_rcpt_bytes = eval_admitted_receipt(
        launch_spec_d,
        launch_pid,
        False,
        100,
        bytes([0x10]) * 32,
        [
            (0, bytes([1]) * 8, bytes([1, 2, 3, 4])),
            (16, bytes([2]) * 8, bytes([5, 6])),
            (32, bytes([3]) * 8, bytes([7, 8])),
            (64, bytes([4]) * 8, bytes([9])),
        ],
    )
    launch_rcpt_tag, _ = content_tag("receipt", EVAL_RECEIPT_SCHEMA, launch_rcpt_bytes)
    assert launch_rcpt_tag == EXPECTED_LAUNCH_EVAL_RECEIPT_TAG, (
        f"launch receipt mismatch: {launch_rcpt_tag} != {EXPECTED_LAUNCH_EVAL_RECEIPT_TAG}"
    )

    succ_rcpt_bytes = eval_admitted_receipt(
        succ_spec_d,
        succ_pid,
        True,
        200,
        bytes([0x20]) * 32,
        [(80, bytes([8]) * 8, bytes([0xDE, 0xAD, 0xBE, 0xEF]))],
    )
    succ_rcpt_tag, _ = content_tag("receipt", EVAL_RECEIPT_SCHEMA, succ_rcpt_bytes)
    assert succ_rcpt_tag == EXPECTED_SUCC_EVAL_RECEIPT_TAG, (
        f"succ receipt mismatch: {succ_rcpt_tag} != {EXPECTED_SUCC_EVAL_RECEIPT_TAG}"
    )

    print(
        "transition_spec_1_vectors PASS: 4 golden tags verified across 2 schemas "
        "(ResolvedProfileTransitionSpecV1, ProfileTransitionEvalReceiptV1)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
