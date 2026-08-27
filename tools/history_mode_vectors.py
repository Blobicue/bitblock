#!/usr/bin/env python3
"""Independent stdlib vectors for Bitblock's two history-mode admission policies.

This file intentionally imports no Bitblock/Rust code. It reconstructs each canonical payload
and independently applies the content-tag wrapper.

A history mode is the policy an admitted history is judged under. Two exist:

* `bitblock/history-mode/open-local/v1` declares *no* sequential-work claim. Its payload is
  empty, so the schema string is the only thing inside the hashed domain -- the tag is a pure
  domain separator, and the vectors below show that the same empty payload under any other
  schema is a different object.
* `bitblock/history-mode/sequential-history/v1` declares the exact iterated function a
  transcript must have been produced by. Its payload is that function policy's full digest,
  so the mode cannot be reused across a changed function: the vectors vary the function and
  show the mode tag move with it.

Neither tag proves elapsed time, an unavoidable sequential lower bound, custody, or consensus.
It names which admission rule a history claims, and nothing more.
"""

from __future__ import annotations

import base64
import hashlib
import struct

TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
KIND = b"policy"

OPEN_LOCAL_MODE_SCHEMA = b"bitblock/history-mode/open-local/v1"
SEQUENTIAL_MODE_SCHEMA = b"bitblock/history-mode/sequential-history/v1"
SEQUENTIAL_FUNCTION_SCHEMA = b"bitblock/sequential-history/function/sha256-chain-v1"

DIGEST_BYTES = 32


def field(value: bytes) -> bytes:
    return struct.pack(">H", len(value)) + value


def content_digest(schema: bytes, payload: bytes) -> bytes:
    preimage = (
        field(TAG_DOMAIN)
        + field(KIND)
        + field(HASH_SUITE)
        + field(schema)
        + struct.pack(">Q", len(payload))
        + payload
    )
    return hashlib.sha256(preimage).digest()


def render(digest: bytes) -> str:
    encoded = base64.b32encode(digest).decode("ascii").lower().rstrip("=")
    return f"#bb.{KIND.decode('ascii')}.sha256.{encoded}"


def sequential_function_digest() -> bytes:
    """The iterated-SHA-256 function policy. Nullary: the schema string is the whole claim."""
    return content_digest(SEQUENTIAL_FUNCTION_SCHEMA, b"")


def open_local_mode_digest() -> bytes:
    """The mode that admits a history with no sequential-work claim at all."""
    return content_digest(OPEN_LOCAL_MODE_SCHEMA, b"")


def sequential_mode_digest(function_digest: bytes) -> bytes:
    """u64-free fixed payload: exactly one 32-byte function-policy digest."""
    if len(function_digest) != DIGEST_BYTES:
        raise ValueError(
            f"sequential mode binds one full digest, found {len(function_digest)} bytes"
        )
    return content_digest(SEQUENTIAL_MODE_SCHEMA, function_digest)


def main() -> None:
    function = sequential_function_digest()
    print(f"sequential function  payload 0   {render(function)}")

    open_local = open_local_mode_digest()
    print(f"open-local mode      payload 0   {render(open_local)}")

    sequential = sequential_mode_digest(function)
    print(f"sequential mode      payload 32  {render(sequential)}")

    # The two modes carry byte-identical payloads only in the trivial sense that open-local has
    # none. They must never collide, and the sequential mode must not equal its own function.
    if open_local == sequential:
        raise SystemExit("the two history modes share a tag")
    if sequential == function:
        raise SystemExit("the sequential mode is indistinguishable from its function")

    # Domain separation: the same empty payload under a neighbouring schema is a different tag.
    for neighbour in (
        b"bitblock/history-mode/open-local/v2",
        b"bitblock/history-mode/open-local-evidence/v1",
        b"bitblock/history-mode/sequential-history/v1",
    ):
        if content_digest(neighbour, b"") == open_local:
            raise SystemExit(f"open-local collides with {neighbour!r} on an empty payload")

    # The sequential mode exists to bind its function. Move the function, and the mode must move.
    moved = bytearray(function)
    moved[0] ^= 0x01
    if sequential_mode_digest(bytes(moved)) == sequential:
        raise SystemExit("the function digest is not inside the sequential mode tag")
    print("mode payload 32 with one flipped function bit -> ", end="")
    print(render(sequential_mode_digest(bytes(moved))))

    # A truncated digest is not an alternate spelling of the same mode; it is refused outright.
    for wrong in (b"", function[:31], function + b"\x00"):
        try:
            sequential_mode_digest(wrong)
        except ValueError:
            continue
        raise SystemExit(f"accepted a {len(wrong)}-byte function digest")

    print("2 modes, 1 function, 3 domain controls, 1 moved-function control, 3 length refusals")


if __name__ == "__main__":
    main()
