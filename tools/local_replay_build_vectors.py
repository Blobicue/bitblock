#!/usr/bin/env python3
"""Independent stdlib vectors for the Bitblock local-replay build fingerprint.

This file intentionally imports no Bitblock/Rust code. It reconstructs the canonical payload and
independently applies the content-tag wrapper.

`bitblock/local-replay-build/executable-sha256/v1` names the bytes of the executable that emitted
one local replay carrier. Its payload is deliberately two fields, not one:

    u64_be(artifact_len) || sha256(artifact)[32]

The length is inside the hashed domain because the fingerprint's whole claim is "exactly these
bytes, all of them". A digest alone would be satisfied by a stream the reader stopped early on;
carrying the declared length means a short or extended read cannot be re-labelled as the same
build. The vectors below therefore vary the length and the digest independently, and show that a
digest reused at a different declared length is a different build object.

The tag names local bytes. It does not identify source, compiler, toolchain, custody, a trusted
build, or behaviour on another host.
"""

from __future__ import annotations

import base64
import hashlib
import struct

TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
KIND = b"build"
SCHEMA = b"bitblock/local-replay-build/executable-sha256/v1"

DIGEST_BYTES = 32
CANONICAL_BYTES = 8 + DIGEST_BYTES


def field(value: bytes) -> bytes:
    return struct.pack(">H", len(value)) + value


def encode_build(artifact_len: int, sha256: bytes) -> bytes:
    """u64_be(artifact_len) || the artifact's raw SHA-256 digest."""
    if not 0 <= artifact_len < (1 << 64):
        raise ValueError(f"artifact length must fit u64, found {artifact_len}")
    if len(sha256) != DIGEST_BYTES:
        raise ValueError(f"artifact digest must be 32 bytes, found {len(sha256)}")
    payload = struct.pack(">Q", artifact_len) + sha256
    if len(payload) != CANONICAL_BYTES:
        raise ValueError(f"canonical build payload must be {CANONICAL_BYTES} bytes")
    return payload


def tag(payload: bytes) -> str:
    if len(payload) != CANONICAL_BYTES:
        raise ValueError(
            f"canonical build payload must be {CANONICAL_BYTES} bytes, found {len(payload)}"
        )
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
    return f"#bb.{KIND.decode('ascii')}.sha256.{encoded}"


def fingerprint(artifact: bytes) -> bytes:
    return encode_build(len(artifact), hashlib.sha256(artifact).digest())


def main() -> None:
    # A zero-length artifact is still a well-formed declaration: it names the empty digest at
    # length zero. Pinning it fixes the field order at the smallest possible input.
    empty = fingerprint(b"")
    print(f"empty artifact                 {len(empty)} bytes  {tag(empty)}")

    body = bytes(range(256)) * 4
    real = fingerprint(body)
    print(f"1024-byte artifact             {len(real)} bytes  {tag(real)}")

    # Same digest, a lied-about length: a truncated read must not name the complete artifact.
    lied = encode_build(len(body) - 1, hashlib.sha256(body).digest())
    print(f"same digest, length - 1        {len(lied)} bytes  {tag(lied)}")

    # Same length, one flipped digest bit: one changed artifact byte must move the build tag.
    flipped_body = bytearray(body)
    flipped_body[512] ^= 0x01
    flipped = fingerprint(bytes(flipped_body))
    print(f"one flipped artifact bit       {len(flipped)} bytes  {tag(flipped)}")

    distinct = {tag(payload) for payload in (empty, real, lied, flipped)}
    if len(distinct) != 4:
        raise SystemExit("build tags collided across distinct declarations")
    if tag(lied) == tag(real):
        raise SystemExit("the declared length is not inside the build tag")
    if tag(flipped) == tag(real):
        raise SystemExit("the artifact digest is not inside the build tag")

    # The length lane is big-endian. A little-endian encoder agrees only on a palindrome, and
    # 1024 is not one, so this control would catch that exact byte-order slip.
    little_endian = struct.pack("<Q", len(body)) + hashlib.sha256(body).digest()
    if tag(little_endian) == tag(real):
        raise SystemExit("the length lane is not big-endian")

    # Length refusals: this payload is fixed at 40 bytes and has no short or long spelling.
    for wrong in (b"", real[:39], real + b"\x00"):
        try:
            tag(wrong)
        except ValueError:
            continue
        raise SystemExit(f"accepted a {len(wrong)}-byte build payload")
    for bad_digest in (b"", hashlib.sha256(body).digest()[:31]):
        try:
            encode_build(0, bad_digest)
        except ValueError:
            continue
        raise SystemExit(f"accepted a {len(bad_digest)}-byte artifact digest")

    print("4 distinct builds, 1 endianness control, 5 length refusals")


if __name__ == "__main__":
    main()
