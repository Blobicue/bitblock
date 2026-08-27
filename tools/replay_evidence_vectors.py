#!/usr/bin/env python3
"""Independent canonical vectors and negative controls for local replay evidence v1.

This script imports no Bitblock code. It reconstructs one complete carrier, independently frames
the SHA-256 content-tag preimage, then strictly decodes the carrier before checking malformed
length, Boolean, optional, prefix, and suffix controls. It validates the carrier codec and hash
domain, not an executable, its simulation semantics, persistence, custody, display, or consensus.
"""

from __future__ import annotations

import base64
import hashlib
import math
import struct


TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
KIND = b"replay-evidence"
SCHEMA = b"bitblock/local-replay-evidence/v1"
MAX_TAPE_BYTES = 16 * 1024 * 1024
FIXED_BYTES = 468
TAPE_LENGTH_OFFSET = 136

# Filled from this independent encoder and pinned by the matching Rust KAT.
EXPECTED_TAG = "#bb.replay-evidence.sha256.r7ukwnl52rten5bwquqztobeuhfn362ywca7wrcudy5mvpache4q"


def u16(value: int) -> bytes:
    return struct.pack(">H", value)


def u32(value: int) -> bytes:
    return struct.pack(">I", value)


def u64(value: int) -> bytes:
    return struct.pack(">Q", value)


def framed(value: bytes) -> bytes:
    return u16(len(value)) + value


def digest_range(start: int) -> bytes:
    return bytes((start + index) & 0xFF for index in range(32))


def canonical_tag(payload: bytes) -> str:
    preimage = (
        framed(TAG_DOMAIN)
        + framed(KIND)
        + framed(HASH_SUITE)
        + framed(SCHEMA)
        + u64(len(payload))
        + payload
    )
    digest = hashlib.sha256(preimage).digest()
    encoded = base64.b32encode(digest).decode("ascii").lower().rstrip("=")
    return f"#bb.replay-evidence.sha256.{encoded}"


def tape_payload() -> bytes:
    # One tick, preserving the exact -0.0 wheel lane, then a place action.
    return (
        u64(1)
        + u64(2)
        + u64(0)
        + u32(2)
        + b"\x02"
        + u64(0x8000000000000000)
        + b"\x04"
    )


def optional_tag(value: bytes | None) -> bytes:
    if value is None:
        return bytes(33)
    assert len(value) == 32
    return b"\x01" + value


def optional_frame(input_digest: bytes | None, frame_digest: bytes | None) -> bytes:
    if input_digest is None or frame_digest is None:
        assert input_digest is None and frame_digest is None
        return bytes(65)
    assert len(input_digest) == len(frame_digest) == 32
    return b"\x01" + input_digest + frame_digest


def encode() -> bytes:
    tape = tape_payload()
    payload = (
        u64(0x0102030405060708)
        + digest_range(0x00)
        + digest_range(0x20)
        + digest_range(0x40)
        + digest_range(0x60)
        + u64(len(tape))
        + tape
        + optional_tag(digest_range(0x80))
        + digest_range(0xA0)
        + digest_range(0xC0)
        + b"".join(u64(0x0102030405060708 + index) for index in range(10))
        + u32(801)
        + u32(601)
        + u32(1001)
        + u32(601)
        + b"\x01"
        + optional_frame(digest_range(0xE0), digest_range(0x00))
        + optional_frame(digest_range(0x20), digest_range(0x40))
    )
    assert len(payload) == FIXED_BYTES + len(tape)
    return payload


class Decoder:
    def __init__(self, payload: bytes):
        self.payload = payload
        self.cursor = 0

    def take(self, size: int, label: str) -> bytes:
        end = self.cursor + size
        if end > len(self.payload):
            raise ValueError(f"truncated {label}")
        answer = self.payload[self.cursor : end]
        self.cursor = end
        return answer

    def u32(self, label: str) -> int:
        return struct.unpack(">I", self.take(4, label))[0]

    def u64(self, label: str) -> int:
        return struct.unpack(">Q", self.take(8, label))[0]

    def optional_tag(self, label: str) -> bytes | None:
        value = self.take(33, label)
        if value[0] == 0:
            if value[1:] != bytes(32):
                raise ValueError(f"noncanonical absent {label}")
            return None
        if value[0] != 1:
            raise ValueError(f"invalid {label} marker")
        return value[1:]

    def optional_frame(self, label: str) -> tuple[bytes, bytes] | None:
        value = self.take(65, label)
        if value[0] == 0:
            if value[1:] != bytes(64):
                raise ValueError(f"noncanonical absent {label}")
            return None
        if value[0] != 1:
            raise ValueError(f"invalid {label} marker")
        return value[1:33], value[33:]


def decode_tape(payload: bytes) -> None:
    dec = Decoder(payload)
    records = dec.u64("tape record count")
    total_actions = dec.u64("tape total action count")
    if records != 1 or total_actions != 2:
        raise ValueError("fixture tape header changed")
    if dec.u64("tape ordinal") != 0 or dec.u32("tape action count") != 2:
        raise ValueError("fixture tape record changed")
    if dec.take(1, "wheel opcode") != b"\x02":
        raise ValueError("fixture wheel opcode changed")
    wheel_bits = dec.u64("wheel bits")
    if not math.isfinite(struct.unpack(">d", u64(wheel_bits))[0]):
        raise ValueError("non-finite wheel")
    if dec.take(1, "place opcode") != b"\x04":
        raise ValueError("fixture place opcode changed")
    if dec.cursor != len(payload):
        raise ValueError("trailing tape bytes")


def decode(payload: bytes) -> None:
    if len(payload) < FIXED_BYTES:
        raise ValueError("short carrier")
    if len(payload) > FIXED_BYTES + MAX_TAPE_BYTES:
        raise ValueError("oversized carrier")
    tape_len = struct.unpack(">Q", payload[TAPE_LENGTH_OFFSET : TAPE_LENGTH_OFFSET + 8])[0]
    if tape_len > MAX_TAPE_BYTES or len(payload) != FIXED_BYTES + tape_len:
        raise ValueError("carrier/tape length mismatch")

    dec = Decoder(payload)
    dec.u64("artifact length")
    dec.take(32, "artifact SHA-256")
    dec.take(32, "genesis policy")
    dec.take(32, "rules policy")
    dec.take(32, "admission policy")
    if dec.u64("tape byte length") != tape_len:
        raise ValueError("tape byte length changed while decoding")
    decode_tape(dec.take(tape_len, "interaction tape"))
    outcome = dec.optional_tag("outcome")
    dec.take(32, "world")
    dec.take(32, "history")
    for _ in range(10):
        dec.u64("camera lane")
    simulation_width = dec.u32("simulation width")
    simulation_height = dec.u32("simulation height")
    output_width = dec.u32("output width")
    output_height = dec.u32("output height")
    sidebar = dec.take(1, "sidebar")[0]
    if sidebar not in (0, 1):
        raise ValueError("noncanonical sidebar bool")
    game = dec.optional_frame("game")
    composite = dec.optional_frame("composite")
    if dec.cursor != len(payload):
        raise ValueError("trailing carrier bytes")
    if (
        simulation_width == 0
        or simulation_height == 0
        or output_width < simulation_width
        or output_height != simulation_height
    ):
        raise ValueError("invalid dimensions")
    if outcome is None or game is None or composite is None:
        raise ValueError("nonempty fixture lacks endpoint fields")


def expect_rejected(name: str, operation: object) -> None:
    try:
        operation()  # type: ignore[operator]
    except (ValueError, struct.error, OverflowError):
        return
    raise AssertionError(f"negative control {name!r} was accepted")


def main() -> None:
    payload = encode()
    decode(payload)
    actual_tag = canonical_tag(payload)
    if actual_tag != EXPECTED_TAG:
        raise SystemExit(f"replay evidence KAT drifted: {actual_tag}")

    for end in range(len(payload)):
        expect_rejected(f"prefix-{end}", lambda end=end: decode(payload[:end]))
    expect_rejected("trailing", lambda: decode(payload + b"\x00"))

    wrong_length = bytearray(payload)
    wrong_length[TAPE_LENGTH_OFFSET : TAPE_LENGTH_OFFSET + 8] = u64(len(tape_payload()) + 1)
    expect_rejected("wrong-tape-length", lambda: decode(bytes(wrong_length)))

    outcome_offset = TAPE_LENGTH_OFFSET + 8 + len(tape_payload())
    wrong_marker = bytearray(payload)
    wrong_marker[outcome_offset] = 2
    expect_rejected("wrong-optional-marker", lambda: decode(bytes(wrong_marker)))

    sidebar_offset = outcome_offset + 33 + 32 + 32 + 10 * 8 + 4 * 4
    wrong_boolean = bytearray(payload)
    wrong_boolean[sidebar_offset] = 2
    expect_rejected("wrong-sidebar-boolean", lambda: decode(bytes(wrong_boolean)))

    changed = bytearray(payload)
    changed[0] ^= 1
    if canonical_tag(bytes(changed)) == actual_tag:
        raise SystemExit("artifact-length mutation did not change the outer tag")

    print(f"replay evidence v1 vectors: PASS ({len(payload)} bytes, {actual_tag})")


if __name__ == "__main__":
    main()
