#!/usr/bin/env python3
"""Independent stdlib vector for Bitblock's checked Win32 presentation policy."""

from __future__ import annotations

import base64
import hashlib
import struct


TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
KIND = b"policy"
SCHEMA = b"bitblock/windows-present-policy/set-dibits-to-device/v1"
EXPECTED = "#bb.policy.sha256.opgpybj32tw72pep2xusit4lzzgsiozs5f5reppq67iiejueetvq"


def framed(value: bytes) -> bytes:
    return struct.pack(">H", len(value)) + value


def main() -> None:
    payload = (
        struct.pack(">H", 1)  # policy revision
        + bytes([0])  # SetDIBitsToDevice into a window DC
        + struct.pack(">I", 0)  # BI_RGB
        + struct.pack(">H", 32)
        + bytes([1])  # top-down DIB
        + struct.pack(">I", 0)  # DIB_RGB_COLORS
        + bytes([1, 1, 1])  # full rows; per-call DC; retain release result
    )
    assert len(payload) == 17
    preimage = (
        framed(TAG_DOMAIN)
        + framed(KIND)
        + framed(HASH_SUITE)
        + framed(SCHEMA)
        + struct.pack(">Q", len(payload))
        + payload
    )
    digest = hashlib.sha256(preimage).digest()
    encoded = base64.b32encode(digest).decode("ascii").lower().rstrip("=")
    actual = f"#bb.policy.sha256.{encoded}"
    if actual != EXPECTED:
        raise SystemExit(f"Windows presentation policy drift: {actual} != {EXPECTED}")
    print(actual)


if __name__ == "__main__":
    main()
