#!/usr/bin/env python3
"""Independent known-answer vectors for Bitblock MetricSnapshot v1."""

from __future__ import annotations

import base64
import hashlib
import struct


TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
KIND = b"metric-snapshot"
SCHEMA = b"bitblock/sidebar/metric-snapshot/v1"
PAYLOAD_LEN = 368


def be16(value: bytes) -> bytes:
    return struct.pack(">H", len(value)) + value


def be32(value: int) -> bytes:
    return struct.pack(">I", value & 0xFFFF_FFFF)


def be64(value: int) -> bytes:
    return struct.pack(">Q", value & 0xFFFF_FFFF_FFFF_FFFF)


def i64(value: int) -> bytes:
    return struct.pack(">q", value)


def sequence(start: int) -> bytes:
    return bytes((start + index) & 0xFF for index in range(32))


def tag(payload: bytes) -> str:
    preimage = (
        be16(TAG_DOMAIN)
        + be16(KIND)
        + be16(HASH_SUITE)
        + be16(SCHEMA)
        + be64(len(payload))
        + payload
    )
    digest = hashlib.sha256(preimage).digest()
    encoded = base64.b32encode(digest).decode("ascii").lower().rstrip("=")
    return f"#bb.metric-snapshot.sha256.{encoded}"


def none_zero() -> bytes:
    payload = bytearray()
    payload += be64(0) * 4
    payload += b"\x00" + bytes(48)  # no bounds
    payload += be64(0) * 7
    payload += b"\x00" + bytes(24)  # no observer cell
    payload += be32(0) * 5
    payload += be64(0) + b"\x00"
    payload += sequence(0x00) + sequence(0x20)
    payload += be64(0)
    payload += b"\x00" + bytes(32) + be64(0)  # no prior frame
    payload += b"\x00" + bytes(32) + be64(0)  # no prior display
    payload += bytes((0, 0))  # unsupported, hidden
    payload += b"\x00" + bytes(20)  # no previous raster facts
    return bytes(payload)


def all_edge() -> bytes:
    payload = bytearray()
    for value in (0x0123_4567_89AB_CDEF, 0xFEDC_BA98_7654_3210, 0, -1):
        payload += be64(value)
    payload += b"\x01"
    for value in (-(1 << 63), -1, 0, (1 << 63) - 1, 0, 1):
        payload += i64(value)
    for bits in (
        0,
        1 << 63,
        1,
        0x7FF0_0000_0000_0000,
        0xFFF0_0000_0000_0000,
        0x7FF8_0000_0000_0001,
        0x0123_4567_89AB_CDEF,
    ):
        payload += be64(bits)
    payload += b"\x01"
    for value in (-(1 << 63), 0, (1 << 63) - 1):
        payload += i64(value)
    for value in (0, 1, 0x8000_0000, 0xFFFF_FFFF, 0x0123_4567):
        payload += be32(value)
    payload += be64(0) + b"\x01"
    payload += sequence(0x00) + sequence(0x20)
    payload += be64(0)
    payload += b"\x01" + sequence(0x40) + be64(0)
    payload += b"\x01" + sequence(0x60) + be64(0)
    payload += bytes((5, 1, 1))  # unavailable, visible, raster present
    for value in (0, 1, 0x8000_0000, 0xFFFF_FFFF, 0x0123_4567):
        payload += be32(value)
    return bytes(payload)


VECTORS = {
    "none-zero": (
        none_zero,
        "c3ddcbc14d0caa9de44d88ab10e00bdc853f28636cdcd8b445ae5455807497ea",
        "#bb.metric-snapshot.sha256.ayjgabj4ttvzgagldztg4ceaxtob6oob4icpxuoha5r3xvb44tga",
    ),
    "all-edge": (
        all_edge,
        "dac5efcaa64838852fcd9ee6905060b7efeba5bf367fbe636c18127c981ec076",
        "#bb.metric-snapshot.sha256.hhgascw5h7giyvfcm5z2zpohvnr6jjgxc6dennd73dm7uvvtmaaa",
    ),
}


def main() -> None:
    for name, (build, expected_payload_hash, expected_tag) in VECTORS.items():
        payload = build()
        if len(payload) != PAYLOAD_LEN:
            raise SystemExit(f"{name}: payload length {len(payload)} != {PAYLOAD_LEN}")
        payload_hash = hashlib.sha256(payload).hexdigest()
        actual_tag = tag(payload)
        if payload_hash != expected_payload_hash:
            raise SystemExit(f"{name}: payload hash mismatch: {payload_hash}")
        if actual_tag != expected_tag:
            raise SystemExit(f"{name}: tag mismatch: {actual_tag}")
        print(f"{name}\t{PAYLOAD_LEN}\t{payload_hash}\t{actual_tag}")


if __name__ == "__main__":
    main()
