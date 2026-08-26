#!/usr/bin/env python3
"""Independent stdlib vectors for Bitblock FrameReceipt v2.

This file intentionally imports no Bitblock/Rust code. It reconstructs the fixed canonical
payload, strictly decodes and re-encodes it, and independently applies the content-tag wrapper.
"""

from __future__ import annotations

import base64
import hashlib
import struct


TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
KIND = b"frame-receipt"
SCHEMA = b"bitblock/frame-receipt/v2"
PAYLOAD_LEN = 130
HEADER_LEN = 83


def framed(value: bytes) -> bytes:
    return struct.pack(">H", len(value)) + value


def u64(value: int) -> bytes:
    return struct.pack(">Q", value)


def sequence(start: int) -> bytes:
    return bytes((start + index) & 0xFF for index in range(32))


def optional_parent(parent: bytes | None) -> bytes:
    return (b"\x00" + bytes(32)) if parent is None else (b"\x01" + parent)


def encode(fields: dict[str, bytes | None]) -> bytes:
    payload = (
        optional_parent(fields["parent"])
        + bytes((fields["stream"],))
        + fields["outcome"]
        + fields["input"]
        + fields["frame"]
    )
    if len(payload) != PAYLOAD_LEN:
        raise ValueError(f"encoder produced {len(payload)} bytes")
    return payload


def decode(payload: bytes) -> dict[str, bytes | None]:
    if len(payload) != PAYLOAD_LEN:
        raise ValueError(f"payload must be exactly {PAYLOAD_LEN} bytes")
    marker = payload[0]
    parent_lane = payload[1:33]
    if marker == 0:
        if parent_lane != bytes(32):
            raise ValueError("absent parent has nonzero padding")
        parent = None
    elif marker == 1:
        parent = parent_lane
    else:
        raise ValueError(f"unknown parent marker {marker}")
    stream = payload[33]
    if stream not in (0, 1):
        raise ValueError(f"unknown stream code {stream}")
    return {
        "parent": parent,
        "stream": stream,
        "outcome": payload[34:66],
        "input": payload[66:98],
        "frame": payload[98:130],
    }


def canonical_preimage(payload: bytes) -> bytes:
    return (
        framed(TAG_DOMAIN)
        + framed(KIND)
        + framed(HASH_SUITE)
        + framed(SCHEMA)
        + u64(len(payload))
        + payload
    )


def content_digest(payload: bytes) -> bytes:
    return hashlib.sha256(canonical_preimage(payload)).digest()


def content_tag(payload: bytes) -> str:
    encoded = base64.b32encode(content_digest(payload)).decode("ascii").lower().rstrip("=")
    return f"#bb.frame-receipt.sha256.{encoded}"


def first_fields() -> dict[str, bytes | None]:
    return {
        "parent": None,
        "stream": 0,
        "outcome": sequence(0x00),
        "input": sequence(0x20),
        "frame": sequence(0x40),
    }


def linked_fields(parent: bytes) -> dict[str, bytes | None]:
    return {
        "parent": parent,
        "stream": 1,
        "outcome": sequence(0x60),
        "input": sequence(0x80),
        "frame": sequence(0xA0),
    }


# Filled only from this independent implementation, then frozen in Rust as a second checker.
EXPECTED = {
    "parent-absent": (
        "25f4e033da416361d7fea59be7ce4b2967918a7cd571bcfc96dde7b5ccb5ed22",
        "#bb.frame-receipt.sha256.5izrwvevcldss6sdjwnj7lqq6rphz2h6re7oe4helz5mft6crnhq",
    ),
    "parent-linked": (
        "dd68009ad702d093231247a6d8189f6962687a14ee7571d0a44c841af4dce1e4",
        "#bb.frame-receipt.sha256.yq44ktjoqs3axwvxvkesiatpwldh7mmgdzoecer47h5rzpkekejq",
    ),
}


def expect_reject(name: str, payload: bytes) -> None:
    try:
        decode(payload)
    except ValueError:
        return
    raise SystemExit(f"negative control accepted: {name}")


def negative_controls(first: bytes, linked: bytes) -> None:
    mutant = bytearray(first)
    mutant[0] = 2
    expect_reject("unknown parent marker", bytes(mutant))

    mutant = bytearray(first)
    mutant[1] = 1
    expect_reject("nonzero absent-parent padding", bytes(mutant))

    mutant = bytearray(first)
    mutant[33] = 2
    expect_reject("unknown stream code", bytes(mutant))

    expect_reject("truncated payload", first[:-1])
    expect_reject("overlong payload", first + b"\x00")
    expect_reject("linked payload truncated", linked[:-1])


def main() -> None:
    if HEADER_LEN != len(canonical_preimage(b"")):
        raise SystemExit("canonical header length drifted")

    first = encode(first_fields())
    linked = encode(linked_fields(content_digest(first)))
    vectors = (("parent-absent", first), ("parent-linked", linked))
    for name, payload in vectors:
        decoded = decode(payload)
        if encode(decoded) != payload:
            raise SystemExit(f"{name}: decode/re-encode mismatch")
        payload_hash = hashlib.sha256(payload).hexdigest()
        tag = content_tag(payload)
        print(
            f"{name}\t{len(payload)}\t{len(canonical_preimage(payload))}"
            f"\t{payload_hash}\t{tag}"
        )
        expected_payload_hash, expected_tag = EXPECTED[name]
        if expected_payload_hash and payload_hash != expected_payload_hash:
            raise SystemExit(f"{name}: payload hash mismatch: {payload_hash}")
        if expected_tag and tag != expected_tag:
            raise SystemExit(f"{name}: tag mismatch: {tag}")

    negative_controls(first, linked)
    print("negative-controls\t6\trejected")


if __name__ == "__main__":
    main()
