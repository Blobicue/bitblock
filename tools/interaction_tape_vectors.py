#!/usr/bin/env python3
"""Independent canonical vectors and strict negative controls for InteractionTape v1.

This script deliberately imports no Bitblock or Rust code. It reconstructs the payload and
content-tag preimage from the schema, strictly decodes the payload, re-encodes it, and proves that
representative malformed forms are refused before they can be treated as replay input.
"""

from __future__ import annotations

import base64
import hashlib
import math
import struct
from collections.abc import Callable


TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
KIND = b"intent"
SCHEMA = b"bitblock/interaction-tape/v1"

MAX_PAYLOAD_BYTES = 16 * 1024 * 1024
MAX_RECORDS = 16_384
MAX_ACTIONS_PER_RECORD = 65_536
MAX_TOTAL_ACTIONS = 262_144

MOVE_EDGE = 0
LOOK_DELTA = 1
WHEEL_LINES = 2
AIM_DELETE = 3
AIM_PLACE = 4
EXACT_EDIT = 5
FIT_WORLD = 6
TOGGLE_SIDEBAR = 7
RESIZE = 8
CLEAR_MOTION = 9

MOVE_CONTROLS = {
    "forward": 0,
    "backward": 1,
    "left": 2,
    "right": 3,
    "up": 4,
    "down": 5,
}
EDIT_OPERATIONS = {"on": 0, "off": 1, "toggle": 2}

EXPECTED_HEX = (
    "00000000000000010000000000000006"
    "000000000000000000000006"
    "000301"
    "0180000000000000003ff8000000000000"
    "02c002000000000000"
    "05028000000000000000fffffffffffffffe7fffffffffffffff"
    "081234567889abcdef"
    "09"
)
EXPECTED_TAG = (
    "#bb.intent.sha256.zhqcq35tj65fukzzkpix6lo45nxsyfrqhzlun2ktpqsegsthiuwa"
)


def u16(value: int) -> bytes:
    return struct.pack(">H", value)


def u32(value: int) -> bytes:
    return struct.pack(">I", value)


def u64(value: int) -> bytes:
    return struct.pack(">Q", value)


def i64(value: int) -> bytes:
    return struct.pack(">q", value)


def framed(value: bytes) -> bytes:
    return u16(len(value)) + value


def f64_bits(value: float) -> int:
    if not math.isfinite(value):
        raise ValueError("canonical interaction floats must be finite")
    return struct.unpack(">Q", struct.pack(">d", value))[0]


def bits_f64(bits: int) -> float:
    return struct.unpack(">d", u64(bits))[0]


def encode_action(action: tuple) -> bytes:
    kind = action[0]
    if kind == "move":
        _, control, down = action
        if control not in MOVE_CONTROLS:
            raise ValueError("undefined move control")
        if type(down) is not bool:
            raise ValueError("move state must be bool")
        return bytes((MOVE_EDGE, MOVE_CONTROLS[control], int(down)))
    if kind == "look_bits":
        _, horizontal_bits, vertical_bits = action
        if not all(
            math.isfinite(bits_f64(bits))
            for bits in (horizontal_bits, vertical_bits)
        ):
            raise ValueError("look delta must be finite")
        return bytes((LOOK_DELTA,)) + u64(horizontal_bits) + u64(vertical_bits)
    if kind == "wheel_bits":
        _, bits = action
        if not math.isfinite(bits_f64(bits)):
            raise ValueError("wheel lines must be finite")
        return bytes((WHEEL_LINES,)) + u64(bits)
    if kind == "delete":
        return bytes((AIM_DELETE,))
    if kind == "place":
        return bytes((AIM_PLACE,))
    if kind == "exact":
        _, operation, x, y, z = action
        if operation not in EDIT_OPERATIONS:
            raise ValueError("undefined edit operation")
        return bytes((EXACT_EDIT, EDIT_OPERATIONS[operation])) + i64(x) + i64(y) + i64(z)
    if kind == "fit":
        return bytes((FIT_WORLD,))
    if kind == "sidebar":
        return bytes((TOGGLE_SIDEBAR,))
    if kind == "resize":
        _, width, height = action
        return bytes((RESIZE,)) + u32(width) + u32(height)
    if kind == "clear-motion":
        return bytes((CLEAR_MOTION,))
    raise ValueError(f"undefined action kind {kind!r}")


def encode(records: tuple[tuple, ...]) -> bytes:
    if len(records) > MAX_RECORDS:
        raise ValueError("record limit")
    total_actions = sum(len(actions) for actions in records)
    if total_actions > MAX_TOTAL_ACTIONS:
        raise ValueError("total action limit")

    payload = bytearray(u64(len(records)) + u64(total_actions))
    for ordinal, actions in enumerate(records):
        if len(actions) > MAX_ACTIONS_PER_RECORD:
            raise ValueError("per-record action limit")
        payload += u64(ordinal) + u32(len(actions))
        for action in actions:
            payload += encode_action(action)
    if len(payload) > MAX_PAYLOAD_BYTES:
        raise ValueError("payload limit")
    return bytes(payload)


class Decoder:
    def __init__(self, payload: bytes):
        self.payload = payload
        self.cursor = 0

    def remaining(self) -> int:
        return len(self.payload) - self.cursor

    def take(self, size: int, field: str) -> bytes:
        if size > self.remaining():
            raise ValueError(f"truncated {field}")
        start = self.cursor
        self.cursor += size
        return self.payload[start : self.cursor]

    def byte(self, field: str) -> int:
        return self.take(1, field)[0]

    def u32(self, field: str) -> int:
        return struct.unpack(">I", self.take(4, field))[0]

    def u64(self, field: str) -> int:
        return struct.unpack(">Q", self.take(8, field))[0]


def decode_action(decoder: Decoder) -> tuple:
    opcode = decoder.byte("action opcode")
    if opcode == MOVE_EDGE:
        control = decoder.byte("move control")
        if control not in MOVE_CONTROLS.values():
            raise ValueError("undefined move control")
        state = decoder.byte("move state")
        if state not in (0, 1):
            raise ValueError("noncanonical bool or padding")
        name = next(name for name, code in MOVE_CONTROLS.items() if code == control)
        return ("move", name, bool(state))
    if opcode == LOOK_DELTA:
        horizontal = decoder.u64("look horizontal")
        vertical = decoder.u64("look vertical")
        if not math.isfinite(bits_f64(horizontal)) or not math.isfinite(
            bits_f64(vertical)
        ):
            raise ValueError("non-finite look delta")
        return ("look_bits", horizontal, vertical)
    if opcode == WHEEL_LINES:
        bits = decoder.u64("wheel lines")
        if not math.isfinite(bits_f64(bits)):
            raise ValueError("non-finite wheel lines")
        return ("wheel_bits", bits)
    if opcode == AIM_DELETE:
        return ("delete",)
    if opcode == AIM_PLACE:
        return ("place",)
    if opcode == EXACT_EDIT:
        operation_code = decoder.byte("edit operation")
        if operation_code not in EDIT_OPERATIONS.values():
            raise ValueError("undefined edit operation")
        operation = next(
            name for name, code in EDIT_OPERATIONS.items() if code == operation_code
        )
        coordinates = struct.unpack(">qqq", decoder.take(24, "edit coordinates"))
        return ("exact", operation, *coordinates)
    if opcode == FIT_WORLD:
        return ("fit",)
    if opcode == TOGGLE_SIDEBAR:
        return ("sidebar",)
    if opcode == RESIZE:
        return ("resize", decoder.u32("resize width"), decoder.u32("resize height"))
    if opcode == CLEAR_MOTION:
        return ("clear-motion",)
    raise ValueError(f"undefined action opcode {opcode}")


def decode(payload: bytes) -> tuple[tuple, ...]:
    if len(payload) > MAX_PAYLOAD_BYTES:
        raise ValueError("payload limit")
    decoder = Decoder(payload)
    record_count = decoder.u64("record count")
    total_action_count = decoder.u64("total action count")
    if record_count > MAX_RECORDS:
        raise ValueError("record limit")
    if total_action_count > MAX_TOTAL_ACTIONS:
        raise ValueError("total action limit")
    if 16 + record_count * 12 > len(payload):
        raise ValueError("record headers cannot fit")

    records = []
    observed_total = 0
    for expected_ordinal in range(record_count):
        ordinal = decoder.u64("tick ordinal")
        if ordinal != expected_ordinal:
            raise ValueError("noncontiguous tick ordinal")
        action_count = decoder.u32("record action count")
        if action_count > MAX_ACTIONS_PER_RECORD:
            raise ValueError("per-record action limit")
        observed_total += action_count
        if observed_total > MAX_TOTAL_ACTIONS:
            raise ValueError("observed total action limit")
        if observed_total > total_action_count:
            raise ValueError("declared total action count too small")
        # The opcode is the shortest possible action. This gate precedes list allocation.
        if action_count > decoder.remaining():
            raise ValueError("action bytes cannot fit")
        actions = []
        for _ in range(action_count):
            actions.append(decode_action(decoder))
        records.append(tuple(actions))

    if observed_total != total_action_count:
        raise ValueError("declared total action count mismatch")
    if decoder.remaining() != 0:
        raise ValueError("trailing bytes or padding")
    result = tuple(records)
    if encode(result) != payload:
        raise ValueError("noncanonical re-encoding")
    return result


def canonical_preimage(payload: bytes) -> bytes:
    return (
        framed(TAG_DOMAIN)
        + framed(KIND)
        + framed(HASH_SUITE)
        + framed(SCHEMA)
        + u64(len(payload))
        + payload
    )


def content_tag(payload: bytes) -> str:
    digest = hashlib.sha256(canonical_preimage(payload)).digest()
    encoded = base64.b32encode(digest).decode("ascii").lower().rstrip("=")
    return f"#bb.intent.sha256.{encoded}"


def sample_records() -> tuple[tuple, ...]:
    return (
        (
            ("move", "right", True),
            ("look_bits", f64_bits(-0.0), f64_bits(1.5)),
            ("wheel_bits", f64_bits(-2.25)),
            ("exact", "toggle", -(1 << 63), -2, (1 << 63) - 1),
            ("resize", 0x12345678, 0x89ABCDEF),
            ("clear-motion",),
        ),
    )


def with_bytes(payload: bytes, start: int, replacement: bytes) -> bytes:
    mutant = bytearray(payload)
    mutant[start : start + len(replacement)] = replacement
    return bytes(mutant)


def expect_rejected(name: str, operation: Callable[[], object]) -> None:
    try:
        operation()
    except (ValueError, struct.error, OverflowError):
        return
    raise AssertionError(f"negative control {name!r} was accepted")


def main() -> None:
    payload = encode(sample_records())
    if payload.hex() != EXPECTED_HEX:
        raise SystemExit("sample canonical payload drifted")
    if decode(payload) != sample_records():
        raise SystemExit("strict sample decode did not reconstruct exact actions")
    actual_tag = content_tag(payload)
    if actual_tag != EXPECTED_TAG:
        raise SystemExit(f"sample tag drifted: {actual_tag}")

    # Header = 16 bytes; first record header = 12 bytes; first action begins at 28.
    action_offset = 28
    controls: tuple[tuple[str, Callable[[], object]], ...] = (
        ("truncated", lambda: decode(payload[:-1])),
        ("trailing-padding", lambda: decode(payload + b"\x00")),
        ("wrong-ordinal", lambda: decode(with_bytes(payload, 16, u64(1)))),
        ("small-total", lambda: decode(with_bytes(payload, 8, u64(5)))),
        ("large-total", lambda: decode(with_bytes(payload, 8, u64(7)))),
        ("unknown-opcode", lambda: decode(with_bytes(payload, action_offset, b"\xff"))),
        (
            "unknown-move-control",
            lambda: decode(with_bytes(payload, action_offset + 1, b"\x06")),
        ),
        (
            "noncanonical-bool",
            lambda: decode(with_bytes(payload, action_offset + 2, b"\x02")),
        ),
        (
            "nonfinite-look",
            lambda: decode(
                with_bytes(payload, action_offset + 4, u64(f64_bits(1.0) | 0x7FF0000000000000))
            ),
        ),
        ("bad-edit-operation", lambda: decode(with_bytes(payload, 58, b"\x03"))),
        ("record-cap", lambda: decode(u64(MAX_RECORDS + 1) + u64(0))),
        ("total-cap", lambda: decode(u64(0) + u64(MAX_TOTAL_ACTIONS + 1))),
        (
            "per-record-cap",
            lambda: decode(
                u64(1)
                + u64(0)
                + u64(0)
                + u32(MAX_ACTIONS_PER_RECORD + 1)
            ),
        ),
        (
            "preallocation-fit",
            lambda: decode(
                u64(1)
                + u64(MAX_ACTIONS_PER_RECORD)
                + u64(0)
                + u32(MAX_ACTIONS_PER_RECORD)
            ),
        ),
        ("payload-cap", lambda: decode(bytes(MAX_PAYLOAD_BYTES + 1))),
    )
    for name, operation in controls:
        expect_rejected(name, operation)

    print(f"sample_payload_len\t{len(payload)}")
    print(f"sample_payload_hex\t{payload.hex()}")
    print(f"sample_tag\t{actual_tag}")
    print(f"negative_controls\t{len(controls)}/{len(controls)} rejected")


if __name__ == "__main__":
    main()
