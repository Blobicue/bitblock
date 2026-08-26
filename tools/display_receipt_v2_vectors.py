#!/usr/bin/env python3
"""Independent stdlib vector for Bitblock DisplayReceipt v2.

This tool imports no Bitblock/Rust code. It reconstructs the frame artifact, typed
FrameReceipt v2, display policy, captured-frame facts, and DisplayReceipt v2 preimage.
"""

from __future__ import annotations

import base64
import hashlib
import struct


TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
FRAME_SCHEMA = b"bitblock/frame/xrgb8888-row-major/v1"
FRAME_RECEIPT_V2_SCHEMA = b"bitblock/frame-receipt/v2"
DISPLAY_POLICY_SCHEMA = b"bitblock/display-capture-policy/gdi-screen-dc/v1"
DISPLAY_RECEIPT_V2_SCHEMA = b"bitblock/display-receipt/v2"
TEST_OBJECT_SCHEMA = b"bitblock/display-test/v1"
FRAME_RECEIPT_V2_LEN = 130
DISPLAY_RECEIPT_V2_LEN = 378


def framed(value: bytes) -> bytes:
    return struct.pack(">H", len(value)) + value


def preimage(kind: bytes, schema: bytes, payload: bytes) -> bytes:
    return (
        framed(TAG_DOMAIN)
        + framed(kind)
        + framed(HASH_SUITE)
        + framed(schema)
        + struct.pack(">Q", len(payload))
        + payload
    )


def digest(kind: bytes, schema: bytes, payload: bytes) -> bytes:
    return hashlib.sha256(preimage(kind, schema, payload)).digest()


def tag(kind: bytes, value: bytes) -> str:
    encoded = base64.b32encode(value).decode("ascii").lower().rstrip("=")
    return f"#bb.{kind.decode('ascii')}.sha256.{encoded}"


def optional_digest(value: bytes | None) -> bytes:
    return b"\x00" + bytes(32) if value is None else b"\x01" + value


def frame_artifact() -> tuple[bytes, tuple[int, ...]]:
    words = (0x00112233, 0x00445566)
    payload = (
        framed(FRAME_SCHEMA)
        + struct.pack(">IIQ", 2, 1, len(words))
        + b"".join(struct.pack(">I", word) for word in words)
    )
    return digest(b"frame", FRAME_SCHEMA, payload), words


def display_policy() -> bytes:
    payload = (
        struct.pack(">H", 1)
        + b"\x00\x00"
        + struct.pack(">I", 0x40CC0020)
        + b"\x00\x00\x00\x00\x00"
    )
    if len(payload) != 13:
        raise AssertionError("display policy payload length drifted")
    return digest(b"policy", DISPLAY_POLICY_SCHEMA, payload)


def frame_receipt_v2(frame: bytes, stream: int = 1) -> bytes:
    outcome = digest(b"tick-outcome", TEST_OBJECT_SCHEMA, b"outcome")
    frame_input = digest(b"frame-input", TEST_OBJECT_SCHEMA, b"input")
    payload = optional_digest(None) + bytes((stream,)) + outcome + frame_input + frame
    if len(payload) != FRAME_RECEIPT_V2_LEN:
        raise AssertionError("frame-receipt-v2 payload length drifted")
    return payload


def frame_facts(words: tuple[int, ...]) -> tuple[int, ...]:
    rgb = tuple(word & 0x00FFFFFF for word in words)
    return (
        len(words),
        sum(value != 0 for value in rgb),
        sum(word & 0xFF000000 != 0 for word in words),
        sum(value.bit_count() for value in rgb),
        sum((value >> 16) & 0xFF for value in rgb),
        sum((value >> 8) & 0xFF for value in rgb),
        sum(value & 0xFF for value in rgb),
    )


def capture_facts(frame: bytes, words: tuple[int, ...]) -> bytes:
    rect = struct.pack(">iiii", -20, 30, -18, 31)
    return (
        b"\x00"  # Match
        + b"\x00"  # Complete
        + struct.pack(">I", 0)
        + b"\x01\x01\x01"  # GDI, compositor, and post-capture flushes
        + rect
        + rect
        + struct.pack(">Iqqq", 144, 100, 120, 10_000_000)
        + b"\x01"
        + frame
        + b"".join(struct.pack(">Q", value) for value in frame_facts(words))
        + struct.pack(">Q", 0)
        + b"\x00"
        + struct.pack(">Q", 0)
    )


def display_receipt_v2_payload(stream: int = 1) -> bytes:
    frame, words = frame_artifact()
    payload = (
        optional_digest(None)
        + frame_receipt_v2(frame, stream)
        + struct.pack(">II", 2, 1)
        + display_policy()
        + capture_facts(frame, words)
    )
    if len(payload) != DISPLAY_RECEIPT_V2_LEN:
        raise AssertionError(f"display-receipt-v2 encoder produced {len(payload)} bytes")
    return payload


def decode_display_receipt_v2(payload: bytes) -> None:
    if len(payload) != DISPLAY_RECEIPT_V2_LEN:
        raise ValueError("wrong display-receipt-v2 payload length")
    if payload[0] not in (0, 1):
        raise ValueError("unknown display parent marker")
    if payload[0] == 0 and payload[1:33] != bytes(32):
        raise ValueError("nonzero absent display parent padding")
    frame_receipt = payload[33 : 33 + FRAME_RECEIPT_V2_LEN]
    if frame_receipt[0] not in (0, 1):
        raise ValueError("unknown frame parent marker")
    if frame_receipt[0] == 0 and frame_receipt[1:33] != bytes(32):
        raise ValueError("nonzero absent frame parent padding")
    if frame_receipt[33] != 1:
        raise ValueError("display receipt requires composite stream")
    captured_marker = payload[272]
    first_marker = payload[369]
    if captured_marker not in (0, 1) or first_marker not in (0, 1):
        raise ValueError("unknown optional field marker")


EXPECTED_PAYLOAD_SHA256 = "56757bc08e43022b5c22253819ba157b207931bd7591aa32d0be2830be5930cc"
EXPECTED_TAG = "#bb.display-receipt.sha256.6y74whydlo2cdhs5pl4fuwp66iwpqobgivpkrpeln2ntpni545pq"


def expect_reject(name: str, payload: bytes) -> None:
    try:
        decode_display_receipt_v2(payload)
    except ValueError:
        return
    raise SystemExit(f"negative control accepted: {name}")


def main() -> None:
    payload = display_receipt_v2_payload()
    decode_display_receipt_v2(payload)
    payload_hash = hashlib.sha256(payload).hexdigest()
    receipt_digest = digest(b"display-receipt", DISPLAY_RECEIPT_V2_SCHEMA, payload)
    receipt_tag = tag(b"display-receipt", receipt_digest)
    print(
        f"typed-composite\t{len(payload)}\t{len(preimage(b'display-receipt', DISPLAY_RECEIPT_V2_SCHEMA, payload))}"
        f"\t{payload_hash}\t{receipt_tag}"
    )
    if EXPECTED_PAYLOAD_SHA256 and payload_hash != EXPECTED_PAYLOAD_SHA256:
        raise SystemExit(f"payload hash mismatch: {payload_hash}")
    if EXPECTED_TAG and receipt_tag != EXPECTED_TAG:
        raise SystemExit(f"tag mismatch: {receipt_tag}")

    mutant = bytearray(payload)
    mutant[0] = 2
    expect_reject("unknown display parent marker", bytes(mutant))
    mutant = bytearray(payload)
    mutant[1] = 1
    expect_reject("nonzero absent display parent padding", bytes(mutant))
    expect_reject("game stream", display_receipt_v2_payload(stream=0))
    expect_reject("truncated", payload[:-1])
    expect_reject("overlong", payload + b"\x00")
    print("negative-controls\t5\trejected")


if __name__ == "__main__":
    main()
