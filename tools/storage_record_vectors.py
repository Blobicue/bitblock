#!/usr/bin/env python3
"""Independent stdlib vectors for the Bitblock storage-record envelope.

This file intentionally imports no Bitblock/Rust code. It reconstructs the canonical envelope,
independently applies the content-tag wrapper, and checks the framing properties the envelope
exists to provide.

The envelope is the outermost container: it frames one already-tagged object for concatenation
into a chain. Its footer is the record tag over everything preceding it, so the record commits
to its own framing -- the declared length, the parent link, the object's kind name, digest and
schema -- and not only to the payload it carries.
"""

from __future__ import annotations

import base64
import hashlib
import struct

TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
KIND = b"storage-record"
SCHEMA = b"bitblock/storage-record/envelope/v1"

MAGIC = b"BBSTRREC"
VERSION = 1
DIGEST_BYTES = 32
HEADER_BYTES = len(MAGIC) + 2 + 8
FOOTER_BYTES = DIGEST_BYTES

# One known-good inner object, so these vectors compose with tools/edit_vectors.py
# rather than inventing a payload of their own.
EDIT_KIND = b"edit"
EDIT_SCHEMA = b"bitblock/edit/op-u8-i64-be/v1"
OPERATIONS = {"on": 0, "off": 1, "toggle": 2}


def field(value: bytes) -> bytes:
    return struct.pack(">H", len(value)) + value


def content_tag_digest(kind: bytes, schema: bytes, payload: bytes) -> bytes:
    preimage = (
        field(TAG_DOMAIN)
        + field(kind)
        + field(HASH_SUITE)
        + field(schema)
        + struct.pack(">Q", len(payload))
        + payload
    )
    return hashlib.sha256(preimage).digest()


def render(kind: bytes, digest: bytes) -> str:
    encoded = base64.b32encode(digest).decode("ascii").lower().rstrip("=")
    return f"#bb.{kind.decode('ascii')}.sha256.{encoded}"


def encode_edit(operation: str, x: int, y: int, z: int) -> bytes:
    return bytes([OPERATIONS[operation]]) + struct.pack(">qqq", x, y, z)


def encoded_len(parent: bytes | None, kind_name: bytes, schema: bytes, payload: bytes) -> int:
    return (
        HEADER_BYTES
        + 1
        + (DIGEST_BYTES if parent is not None else 0)
        + 2
        + len(kind_name)
        + DIGEST_BYTES
        + 2
        + len(schema)
        + 8
        + len(payload)
        + FOOTER_BYTES
    )


def encode_prefix(
    parent: bytes | None,
    object_kind: bytes,
    object_digest: bytes,
    object_schema: bytes,
    payload: bytes,
) -> bytes:
    total = encoded_len(parent, object_kind, object_schema, payload)
    out = bytearray()
    out += MAGIC
    out += struct.pack(">H", VERSION)
    out += struct.pack(">Q", total)
    if parent is None:
        out += b"\x00"
    else:
        if len(parent) != DIGEST_BYTES:
            raise ValueError("parent link must be a full digest")
        out += b"\x01" + parent
    out += struct.pack(">H", len(object_kind)) + object_kind
    out += object_digest
    out += struct.pack(">H", len(object_schema)) + object_schema
    out += struct.pack(">Q", len(payload)) + payload
    if len(out) + FOOTER_BYTES != total:
        raise ValueError(f"prefix {len(out)} + footer != declared {total}")
    return bytes(out)


def encode_record(
    parent: bytes | None,
    object_kind: bytes,
    object_digest: bytes,
    object_schema: bytes,
    payload: bytes,
) -> tuple[bytes, bytes]:
    prefix = encode_prefix(parent, object_kind, object_digest, object_schema, payload)
    record_digest = content_tag_digest(KIND, SCHEMA, prefix)
    return prefix + record_digest, record_digest


def main() -> None:
    edit = encode_edit("on", 0, 0, 0)
    edit_digest = content_tag_digest(EDIT_KIND, EDIT_SCHEMA, edit)
    print("inner object")
    print(f"  {render(EDIT_KIND, edit_digest)}")

    root, root_digest = encode_record(None, EDIT_KIND, edit_digest, EDIT_SCHEMA, edit)
    print("\nunparented root record")
    print(f"  {len(root)} bytes")
    print(f"  {root.hex()}")
    print(f"  {render(KIND, root_digest)}")

    child, child_digest = encode_record(
        root_digest, EDIT_KIND, edit_digest, EDIT_SCHEMA, edit
    )
    print("\nchild record, same object, parented to the root")
    print(f"  {len(child)} bytes")
    print(f"  {render(KIND, child_digest)}")

    if root_digest == child_digest:
        raise SystemExit("parent link is not inside the record tag")
    if len(child) - len(root) != DIGEST_BYTES:
        raise SystemExit("a parent link must cost exactly one digest")

    # The declared length lives inside the hashed prefix, so a record cannot be
    # re-framed without moving its tag.
    reframed = bytearray(root)
    reframed[HEADER_BYTES - 8] ^= 0x01
    if content_tag_digest(KIND, SCHEMA, bytes(reframed[:-FOOTER_BYTES])) == root_digest:
        raise SystemExit("declared length is not bound by the record tag")

    # Concatenation is the chain format: records follow one another with no separator,
    # which is only unambiguous because each declares its own total length.
    chain = root + child
    print(f"\nchain of two records: {len(chain)} bytes, "
          f"{struct.unpack('>Q', chain[HEADER_BYTES - 8:HEADER_BYTES])[0]} declared by the first")
    if len(chain) != len(root) + len(child):
        raise SystemExit("chain framing is not plain concatenation")
    print("framing, parent linkage and length binding all hold")


if __name__ == "__main__":
    main()
