#!/usr/bin/env python3
"""Independent stdlib KATs for Bitblock's output-color observation codec.

This file imports no Bitblock code and never launches Rust. It reconstructs the canonical payload,
strict decoder, and domain-separated SHA-256 tag for raw per-route Advanced Color reports.
"""

from __future__ import annotations

import base64
import hashlib
import struct


TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
KIND = b"output-color-observation"
SCHEMA = b"bitblock/output-color-observation/v1"

AVAILABLE = 0
UNAVAILABLE = 1
ROUTE_AVAILABLE = 0
ROUTE_UNAVAILABLE = 1
MAX_ROUTES = 256
MAX_PAYLOAD = 32 + 1 + 2 + MAX_ROUTES * 13


def u8(value: int) -> bytes:
    return struct.pack(">B", value)


def u16(value: int) -> bytes:
    return struct.pack(">H", value)


def u32(value: int) -> bytes:
    return struct.pack(">I", value)


def tag(payload: bytes) -> str:
    preimage = (
        u16(len(TAG_DOMAIN))
        + TAG_DOMAIN
        + u16(len(KIND))
        + KIND
        + u16(len(HASH_SUITE))
        + HASH_SUITE
        + u16(len(SCHEMA))
        + SCHEMA
        + struct.pack(">Q", len(payload))
        + payload
    )
    digest = hashlib.sha256(preimage).digest()
    encoded = base64.b32encode(digest).rstrip(b"=").lower().decode("ascii")
    return f"#bb.{KIND.decode()}.sha256.{encoded}"


def encode_available(parent: bytes, routes: list[tuple[str, tuple[int, ...]]]) -> bytes:
    if len(parent) != 32:
        raise ValueError("parent must be exactly one SHA-256 digest")
    if not 1 <= len(routes) <= MAX_ROUTES:
        raise ValueError("route count is outside canonical bounds")
    payload = bytearray(parent + u8(AVAILABLE) + u16(len(routes)))
    for outcome, values in routes:
        if outcome == "available":
            flags, encoding, bits = values
            payload += u8(ROUTE_AVAILABLE) + u32(flags) + u32(encoding) + u32(bits)
        elif outcome == "unavailable":
            (error,) = values
            payload += u8(ROUTE_UNAVAILABLE) + u32(error)
        else:
            raise ValueError("unknown route outcome")
    return bytes(payload)


def encode_unavailable(parent: bytes, reason: int, error: int) -> bytes:
    if len(parent) != 32:
        raise ValueError("parent must be exactly one SHA-256 digest")
    return parent + u8(UNAVAILABLE) + u32(reason) + u32(error)


class Decoder:
    def __init__(self, payload: bytes) -> None:
        self.payload = payload
        self.cursor = 0

    def take(self, count: int) -> bytes:
        end = self.cursor + count
        if end > len(self.payload):
            raise ValueError("unexpected end")
        result = self.payload[self.cursor : end]
        self.cursor = end
        return result

    def u8(self) -> int:
        return self.take(1)[0]

    def u16(self) -> int:
        return struct.unpack(">H", self.take(2))[0]

    def u32(self) -> int:
        return struct.unpack(">I", self.take(4))[0]

    def finish(self) -> None:
        if self.cursor != len(self.payload):
            raise ValueError("trailing bytes")


def decode(payload: bytes) -> tuple[bytes, str, tuple[tuple[str, tuple[int, ...]], ...] | tuple[int, int]]:
    if not payload or len(payload) > MAX_PAYLOAD:
        raise ValueError("payload length")
    dec = Decoder(payload)
    parent = dec.take(32)
    marker = dec.u8()
    if marker == AVAILABLE:
        count = dec.u16()
        if not 1 <= count <= MAX_ROUTES:
            raise ValueError("route count")
        if len(payload) - dec.cursor < count * 5:
            raise ValueError("short route minimum")
        routes: list[tuple[str, tuple[int, ...]]] = []
        for _ in range(count):
            route_marker = dec.u8()
            if route_marker == ROUTE_AVAILABLE:
                routes.append(("available", (dec.u32(), dec.u32(), dec.u32())))
            elif route_marker == ROUTE_UNAVAILABLE:
                routes.append(("unavailable", (dec.u32(),)))
            else:
                raise ValueError("route outcome")
        dec.finish()
        return parent, "available", tuple(routes)
    if marker == UNAVAILABLE:
        result = (dec.u32(), dec.u32())
        dec.finish()
        return parent, "unavailable", result
    raise ValueError("observation outcome")


def expect_rejected(label: str, payload: bytes) -> None:
    try:
        decode(payload)
    except ValueError:
        return
    raise AssertionError(f"{label} unexpectedly decoded")


def collect_with_transient_ids(
    _adapter_luid: int,
    _target_id: int,
    _mode_index: int,
) -> bytes:
    # These collector-local Windows join keys deliberately have no encoded lane.
    return encode_available(
        PARENT,
        [
            ("available", (0x8000000F, 0xFFFFFFFE, 0xFFFFFFFF)),
            ("unavailable", (1168,)),
        ],
    )


PARENT = hashlib.sha256(b"bitblock/output-color-vector-parent/v1").digest()
AVAILABLE_PAYLOAD = collect_with_transient_ids(1, 2, 3)
UNAVAILABLE_PAYLOAD = encode_unavailable(PARENT, 0x80000001, 87)


def main() -> None:
    assert decode(AVAILABLE_PAYLOAD) == (
        PARENT,
        "available",
        (
            ("available", (0x8000000F, 0xFFFFFFFE, 0xFFFFFFFF)),
            ("unavailable", (1168,)),
        ),
    )
    assert decode(UNAVAILABLE_PAYLOAD) == (PARENT, "unavailable", (0x80000001, 87))
    assert AVAILABLE_PAYLOAD == collect_with_transient_ids(0xFFFFFFFFFFFFFFFF, 99, 100)

    bad_outcome = bytearray(AVAILABLE_PAYLOAD)
    bad_outcome[32] = 2
    bad_count = bytearray(AVAILABLE_PAYLOAD)
    bad_count[33:35] = u16(0)
    bad_route = bytearray(AVAILABLE_PAYLOAD)
    bad_route[35] = 2
    altered_parent = bytearray(AVAILABLE_PAYLOAD)
    altered_parent[0] ^= 1
    altered_raw_word = bytearray(AVAILABLE_PAYLOAD)
    altered_raw_word[36] ^= 1

    rejected = [
        ("empty", b""),
        ("truncated", AVAILABLE_PAYLOAD[:-1]),
        ("trailing", AVAILABLE_PAYLOAD + b"\0"),
        ("unknown-outcome", bytes(bad_outcome)),
        ("zero-routes", bytes(bad_count)),
        ("unknown-route", bytes(bad_route)),
    ]
    for label, payload in rejected:
        expect_rejected(label, payload)
    assert tag(AVAILABLE_PAYLOAD) != tag(bytes(altered_parent))
    assert tag(AVAILABLE_PAYLOAD) != tag(bytes(altered_raw_word))

    print(
        "available",
        len(AVAILABLE_PAYLOAD),
        AVAILABLE_PAYLOAD.hex(),
        tag(AVAILABLE_PAYLOAD),
    )
    print(
        "unavailable",
        len(UNAVAILABLE_PAYLOAD),
        UNAVAILABLE_PAYLOAD.hex(),
        tag(UNAVAILABLE_PAYLOAD),
    )
    print("negative-controls", len(rejected), "rejected")


if __name__ == "__main__":
    main()
