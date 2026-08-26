#!/usr/bin/env python3
"""Independently reconstruct canonical Bitblock edit bytes and full SHA-256 tags."""

from __future__ import annotations

import base64
import hashlib
import struct


TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
EDIT_KIND = b"edit"
EDIT_SCHEMA = b"bitblock/edit/op-u8-i64-be/v1"
OPERATIONS = {"on": 0, "off": 1, "toggle": 2}


def field(value: bytes) -> bytes:
    return struct.pack(">H", len(value)) + value


def encode_edit(operation: str, x: int, y: int, z: int) -> bytes:
    try:
        code = OPERATIONS[operation]
    except KeyError as error:
        raise ValueError(f"unknown operation {operation!r}") from error
    return bytes([code]) + struct.pack(">qqq", x, y, z)


def tag(payload: bytes) -> str:
    if len(payload) != 25:
        raise ValueError(f"canonical edit must be 25 bytes, found {len(payload)}")
    preimage = (
        field(TAG_DOMAIN)
        + field(EDIT_KIND)
        + field(HASH_SUITE)
        + field(EDIT_SCHEMA)
        + struct.pack(">Q", len(payload))
        + payload
    )
    digest = hashlib.sha256(preimage).digest()
    encoded = base64.b32encode(digest).decode("ascii").lower().rstrip("=")
    return f"#bb.edit.sha256.{encoded}"


def main() -> None:
    vectors = (
        ("on", 0, 0, 0),
        ("off", 1, -2, 3),
        ("toggle", -(1 << 63), -2, (1 << 63) - 1),
    )
    for vector in vectors:
        payload = encode_edit(*vector)
        print(" ".join(map(str, vector)))
        print(payload.hex())
        print(tag(payload))


if __name__ == "__main__":
    main()
