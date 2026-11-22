#!/usr/bin/env python3
"""Independent stdlib vectors for TRANSITION-SPEC-D1 base contract and extension interface.

Imports no Rust/Bitblock code. Reconstructs all three canonical schemas:
  * bitblock/transition-spec-d1/base-contract/v1
  * bitblock/transition-spec-d1/extension-slot/v1
  * bitblock/transition-spec-d1/base-receipt/v1

Verifies canonical tag parity, deterministic base occupancy/episode transitions
(mint, already-on no-op, bedrock protection, burn, already-off no-op, fresh re-mint),
and powered negative controls (op-tag collision, disposition collision, owned-domain
collision, undeclared write access, base-tag hijack, zero work, wrong version,
truncation, trailing bytes, and omitted disposition lane).
"""

from __future__ import annotations

import base64
import hashlib
import struct
import sys

TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"

CONTRACT_SCHEMA = b"bitblock/transition-spec-d1/base-contract/v1"
EXTENSION_SLOT_SCHEMA = b"bitblock/transition-spec-d1/extension-slot/v1"
BASE_RECEIPT_SCHEMA = b"bitblock/transition-spec-d1/base-receipt/v1"

BASE_VERSION = 1
BASE_RESERVED_OP_MAX = 15
BASE_RESERVED_DISP_MAX = 15
MAX_BASE_BATCH_OPS = 65_536
BASE_DOMAINS_MASK = 0b111


def field(val: bytes) -> bytes:
    return struct.pack(">H", len(val)) + val


def content_tag(kind: str, schema: bytes, payload: bytes) -> tuple[str, bytes]:
    preimage = (
        field(TAG_DOMAIN)
        + field(kind.encode("ascii"))
        + field(HASH_SUITE)
        + field(schema)
        + struct.pack(">Q", len(payload))
        + payload
    )
    digest = hashlib.sha256(preimage).digest()
    encoded = base64.b32encode(digest).decode("ascii").lower().rstrip("=")
    return f"#bb.{kind}.sha256.{encoded}", digest


def validate_slot(slot: dict) -> bool:
    if slot["slot_id"] == 0:
        return False
    if slot["op_min"] <= BASE_RESERVED_OP_MAX or slot["op_min"] > slot["op_max"]:
        return False
    if slot["disp_min"] <= BASE_RESERVED_DISP_MAX or slot["disp_min"] > slot["disp_max"]:
        return False
    bit = slot["owned_bit"]
    if bit < 3 or bit >= 32:
        return False
    owned_mask = 1 << bit
    wmask = slot["write_mask"]
    if (wmask & BASE_DOMAINS_MASK) != 0 or (wmask & ~owned_mask) != 0:
        return False
    if slot["base_work"] == 0 or (slot["base_work"] + slot["per_item_work"]) >= (1 << 64):
        return False
    if slot["conflict"] not in (0, 1):
        return False
    return True


def encode_slot(slot: dict) -> bytes:
    assert validate_slot(slot)
    return struct.pack(
        ">HBBBBBBIIQQ",
        slot["slot_id"],
        slot["op_min"],
        slot["op_max"],
        slot["disp_min"],
        slot["disp_max"],
        slot["owned_bit"],
        slot["conflict"],
        slot["read_mask"],
        slot["write_mask"],
        slot["base_work"],
        slot["per_item_work"],
    )


def validate_registry(slots: list[dict]) -> bool:
    for s in slots:
        if not validate_slot(s):
            return False
    for i in range(1, len(slots)):
        if slots[i - 1]["slot_id"] >= slots[i]["slot_id"]:
            return False
    for i in range(len(slots)):
        for j in range(i + 1, len(slots)):
            a, b = slots[i], slots[j]
            if a["op_min"] <= b["op_max"] and b["op_min"] <= a["op_max"]:
                return False
            if a["disp_min"] <= b["disp_max"] and b["disp_min"] <= a["disp_max"]:
                return False
            if a["owned_bit"] == b["owned_bit"]:
                return False
    return True


def encode_base_contract(
    slots: list[dict],
    version: int = BASE_VERSION,
    max_ops: int = MAX_BASE_BATCH_OPS,
    work_per_edit: int = 1,
) -> bytes:
    assert validate_registry(slots)
    out = bytearray(
        struct.pack(
            ">HIQqqqIH",
            version,
            max_ops,
            work_per_edit,
            0,
            0,
            0,
            BASE_DOMAINS_MASK,
            len(slots),
        )
    )
    for s in slots:
        out.extend(encode_slot(s))
    return bytes(out)


def decode_base_contract(data: bytes) -> dict | None:
    if len(data) < 44:
        return None
    ver, max_ops, work_per_edit, bx, by, bz, dmask, slot_count = struct.unpack(
        ">HIQqqqIH", data[:44]
    )
    if ver != BASE_VERSION or max_ops == 0 or max_ops > MAX_BASE_BATCH_OPS:
        return None
    if work_per_edit == 0 or (bx, by, bz) != (0, 0, 0) or dmask != BASE_DOMAINS_MASK:
        return None
    if len(data) != 44 + slot_count * 32:
        return None
    slots = []
    for i in range(slot_count):
        off = 44 + i * 32
        (
            slot_id,
            op_min,
            op_max,
            disp_min,
            disp_max,
            owned_bit,
            conflict,
            read_mask,
            write_mask,
            base_work,
            per_item_work,
        ) = struct.unpack(">HBBBBBBIIQQ", data[off : off + 32])
        slots.append(
            {
                "slot_id": slot_id,
                "op_min": op_min,
                "op_max": op_max,
                "disp_min": disp_min,
                "disp_max": disp_max,
                "owned_bit": owned_bit,
                "conflict": conflict,
                "read_mask": read_mask,
                "write_mask": write_mask,
                "base_work": base_work,
                "per_item_work": per_item_work,
            }
        )
    if not validate_registry(slots):
        return None
    return {"version": ver, "max_ops": max_ops, "work_per_edit": work_per_edit, "slots": slots}


def encode_base_receipt(
    spec_digest: bytes,
    tick: int,
    pre_world: bytes,
    pre_history: bytes,
    post_world: bytes,
    post_history: bytes,
    work_units: int,
    dispositions: list[int],
) -> bytes:
    assert all(0 <= d <= 6 for d in dispositions)
    return (
        spec_digest
        + struct.pack(">Q", tick)
        + pre_world
        + pre_history
        + post_world
        + post_history
        + struct.pack(">QI", work_units, len(dispositions))
        + bytes(dispositions)
    )


def simulate_base_batch(occupied: set[tuple[int, int, int]], edits: list[tuple[int, int, int, int]]) -> tuple[set[tuple[int, int, int]], list[int]]:
    # Immutable origin (0,0,0) always present
    staged = set(occupied)
    staged.add((0, 0, 0))
    disps = []
    for op, x, y, z in edits:
        coord = (x, y, z)
        if coord == (0, 0, 0):
            disps.append(2)  # NoOpBedrock
            continue
        if op == 0:  # On
            if coord in staged:
                disps.append(3)  # NoOpAlreadyOn
            else:
                staged.add(coord)
                disps.append(0)  # Mint
        elif op == 1:  # Off
            if coord in staged:
                staged.remove(coord)
                disps.append(1)  # Burn
            else:
                disps.append(4)  # NoOpAlreadyOff
        elif op == 2:  # Toggle
            if coord in staged:
                staged.remove(coord)
                disps.append(1)  # Burn
            else:
                staged.add(coord)
                disps.append(0)  # Mint
        else:
            raise ValueError(f"invalid opcode {op}")
    return staged, disps


def run_all() -> None:
    # 1. Base-only contract canonical encoding & tag
    base_bytes = encode_base_contract([])
    assert len(base_bytes) == 44
    assert decode_base_contract(base_bytes) is not None
    base_tag, base_digest = content_tag("policy", CONTRACT_SCHEMA, base_bytes)
    assert (
        base_tag
        == "#bb.policy.sha256.pesnqo27cmnpc4q2jpudlfm5mmnhysue2tmstbyc7f77dkvus5na"
    )
    for cut in range(len(base_bytes)):
        assert decode_base_contract(base_bytes[:cut]) is None
    assert decode_base_contract(base_bytes + b"\x00") is None

    # Wrong version rejected
    bad_ver = bytearray(base_bytes)
    bad_ver[1] = 2
    assert decode_base_contract(bytes(bad_ver)) is None

    # 2. Valid extension slot descriptor & tag
    slot_a = {
        "slot_id": 1,
        "op_min": 16,
        "op_max": 23,
        "disp_min": 16,
        "disp_max": 23,
        "owned_bit": 3,
        "conflict": 0,
        "read_mask": BASE_DOMAINS_MASK | (1 << 3),
        "write_mask": 1 << 3,
        "base_work": 10,
        "per_item_work": 2,
    }
    slot_a_bytes = encode_slot(slot_a)
    assert len(slot_a_bytes) == 32
    slot_a_tag, _ = content_tag("policy", EXTENSION_SLOT_SCHEMA, slot_a_bytes)
    assert (
        slot_a_tag
        == "#bb.policy.sha256.kxlgpvzyz3bcvdbpzvlda4yiehaph2rae2qsgrqx7dpibzz4y6ya"
    )

    ext_bytes = encode_base_contract([slot_a])
    assert len(ext_bytes) == 76
    assert decode_base_contract(ext_bytes) is not None
    ext_tag, _ = content_tag("policy", CONTRACT_SCHEMA, ext_bytes)
    assert ext_tag != base_tag

    # 3. Powered negative controls on extension interface
    # Base op-tag hijack
    assert not validate_slot({**slot_a, "op_min": 15})
    # Base disposition hijack
    assert not validate_slot({**slot_a, "disp_min": 15})
    # Undeclared write to base domain
    assert not validate_slot({**slot_a, "write_mask": (1 << 0) | (1 << 3)})
    # Undeclared write to unowned extension domain
    assert not validate_slot({**slot_a, "write_mask": (1 << 3) | (1 << 4)})
    # Zero work forbidden
    assert not validate_slot({**slot_a, "base_work": 0})
    # Op-tag collision
    slot_b = {**slot_a, "slot_id": 2, "op_min": 23, "op_max": 30, "disp_min": 24, "disp_max": 30, "owned_bit": 4, "write_mask": 1 << 4}
    assert not validate_registry([slot_a, slot_b])
    # Disposition collision
    slot_b_disp = {**slot_b, "op_min": 24, "disp_min": 23}
    assert not validate_registry([slot_a, slot_b_disp])
    # Domain ownership collision
    slot_b_dom = {**slot_b, "op_min": 24, "owned_bit": 3, "write_mask": 1 << 3}
    assert not validate_registry([slot_a, slot_b_dom])
    # Non-canonical slot order
    slot_b_ok = {**slot_b, "op_min": 24}
    assert validate_registry([slot_a, slot_b_ok])
    assert not validate_registry([slot_b_ok, slot_a])

    # 4. Deterministic base transition simulation + BaseTransitionReceiptV1
    edits = [
        (0, 1, 2, 3),  # Mint
        (0, 1, 2, 3),  # NoOpAlreadyOn
        (1, 0, 0, 0),  # NoOpBedrock
        (1, 1, 2, 3),  # Burn
        (1, 1, 2, 3),  # NoOpAlreadyOff
        (2, 1, 2, 3),  # Toggle -> Mint
    ]
    post_occ, disps = simulate_base_batch({(0, 0, 0)}, edits)
    assert disps == [0, 3, 2, 1, 4, 0]
    assert (1, 2, 3) in post_occ and (0, 0, 0) in post_occ
    receipt_bytes = encode_base_receipt(
        base_digest,
        1,
        b"\x11" * 32,
        b"\x22" * 32,
        b"\x33" * 32,
        b"\x44" * 32,
        len(edits),
        disps,
    )
    assert len(receipt_bytes) == 180 + len(edits)
    r_tag, _ = content_tag("receipt", BASE_RECEIPT_SCHEMA, receipt_bytes)
    # Omitting one disposition lane must move receipt tag
    r_omitted_bytes = encode_base_receipt(
        base_digest,
        1,
        b"\x11" * 32,
        b"\x22" * 32,
        b"\x33" * 32,
        b"\x44" * 32,
        len(edits),
        disps[:-1],
    )
    r_omitted_tag, _ = content_tag("receipt", BASE_RECEIPT_SCHEMA, r_omitted_bytes)
    assert r_tag != r_omitted_tag

    print(
        "transition_spec_d1_vectors: ok (base_tag=%s, slot_tag=%s, 3 schemas and all extension/atomicity controls pass)"
        % (base_tag, slot_a_tag)
    )


if __name__ == "__main__":
    run_all()
    sys.exit(0)
