#!/usr/bin/env python3
"""Independent stdlib vectors for Bitblock's journal rejected-tail tag.

This file intentionally imports no Bitblock/Rust code. It reconstructs the canonical payload
and independently applies the content-tag wrapper.

The rejected-tail tag is what a recovery journal commits to when it finds bytes past the last
valid record: the two offsets that bound the rejection plus the exact rejected suffix. Binding
all three is the point -- the same suffix at a different offset, or a different suffix at the
same offset, must not produce the same tag, so the vectors below vary each independently.
"""

from __future__ import annotations

import base64
import hashlib
import struct

TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
KIND = b"journal-rejected-tail"
SCHEMA = b"bitblock/journal/rejected-tail/v1"
OFFSET_BYTES = 16


def field(value: bytes) -> bytes:
    return struct.pack(">H", len(value)) + value


def encode_rejected_tail(valid_prefix_bytes: int, file_bytes: int, tail: bytes) -> bytes:
    """u64_be(valid_prefix) || u64_be(file_bytes) || tail."""
    for name, value in (("valid_prefix_bytes", valid_prefix_bytes), ("file_bytes", file_bytes)):
        if not 0 <= value < (1 << 64):
            raise ValueError(f"{name} must fit u64, found {value}")
    if valid_prefix_bytes > file_bytes:
        raise ValueError("valid prefix cannot exceed the file length")
    if file_bytes - valid_prefix_bytes != len(tail):
        raise ValueError(
            f"tail must be exactly the rejected span: "
            f"{file_bytes - valid_prefix_bytes} bytes, found {len(tail)}"
        )
    return struct.pack(">QQ", valid_prefix_bytes, file_bytes) + tail


def tag(payload: bytes) -> str:
    if len(payload) < OFFSET_BYTES:
        raise ValueError(f"payload must carry both offsets, found {len(payload)} bytes")
    preimage = (
        field(TAG_DOMAIN)
        + field(KIND)
        + field(HASH_SUITE)
        + field(SCHEMA)
        + struct.pack(">Q", len(payload))
        + payload
    )
    digest = hashlib.sha256(preimage).digest()
    encoded = base64.b32encode(digest).decode("ascii").lower().rstrip("=")
    return f"#bb.journal-rejected-tail.sha256.{encoded}"


def main() -> None:
    vectors = (
        ("empty tail at origin", 0, 0, b""),
        ("one rejected byte", 64, 65, b"\x00"),
        ("partial record", 96, 103, b"partial"),
        # Same suffix, different offset: the tag must move.
        ("partial record, later offset", 97, 104, b"partial"),
        # Same offsets, one bit flipped in the suffix: the tag must move.
        ("partial record, flipped bit", 96, 103, b"partia\x6d"),
    )
    for label, valid_prefix, file_bytes, tail in vectors:
        payload = encode_rejected_tail(valid_prefix, file_bytes, tail)
        print(f"{label}: valid_prefix={valid_prefix} file_bytes={file_bytes} tail={tail!r}")
        print(payload.hex())
        print(tag(payload))

    distinct = {
        tag(encode_rejected_tail(v[1], v[2], v[3])) for v in vectors
    }
    if len(distinct) != len(vectors):
        raise SystemExit("rejected-tail tags collided across distinct inputs")
    print(f"\n{len(distinct)} distinct tags for {len(vectors)} distinct inputs")


if __name__ == "__main__":
    main()
