#!/usr/bin/env python3
"""Independent vectors for the verifier-first bb-lam/v1 Edit quote."""

from __future__ import annotations

import base64
import hashlib
import struct


TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
EDIT_SCHEMA = b"bitblock/edit/op-u8-i64-be/v1"
POLICY_SCHEMA = b"bitblock/ladderlang/quoted-byte-policy/v1"
TERM_SCHEMA = b"bitblock/ladderlang/lam-prefix-vla-v1"
RECEIPT_SCHEMA = b"bitblock/ladderlang/quote-receipt/v1"
QUOTE_PREFIX = b"bb-lam/v1"
POLICY_PAYLOAD = (
    b"bridge=bb-lam/v1;"
    b"ladderlang-commit=59042265167b8f40700a891d890fde8015bcade9;"
    b"source=full-content-tag-preimage;quoted-prefix=bb-lam/v1;"
    b"bit-order=msb-first;B0=App(I,FALSE);B1=App(FALSE,I);B(0)=B0;B(1)=B1;"
    b"Byte(b7..b0)=Pair(B(b7),Pair(B(b6),Pair(B(b5),Pair(B(b4),"
    b"Pair(B(b3),Pair(B(b2),Pair(B(b1),B(b0)))))));"
    b"Bytes([])=NIL,Bytes(x::xs)=App(Byte(x),Bytes(xs));"
    b"LamPrefix=Var(n):octet(00)||u64be(n),Lam(x):octet(01)||x,"
    b"App(f,a):octet(02)||f||a;"
    b"cost=Lam.costChecked;scope=representation-only"
)


def field(value: bytes) -> bytes:
    return struct.pack(">H", len(value)) + value


def tag_preimage(kind: bytes, schema: bytes, payload: bytes) -> bytes:
    return (
        field(TAG_DOMAIN)
        + field(kind)
        + field(HASH_SUITE)
        + field(schema)
        + struct.pack(">Q", len(payload))
        + payload
    )


def content_digest(kind: bytes, schema: bytes, payload: bytes) -> bytes:
    return hashlib.sha256(tag_preimage(kind, schema, payload)).digest()


def text_tag(kind: str, digest: bytes) -> str:
    encoded = base64.b32encode(digest).decode("ascii").lower().rstrip("=")
    return f"#bb.{kind}.sha256.{encoded}"


# Canonical expanded LadderLang Lam preorder.
def var(index: int) -> bytes:
    return b"\x00" + struct.pack(">Q", index)


def lam(body: bytes) -> bytes:
    return b"\x01" + body


def app(fn: bytes, arg: bytes) -> bytes:
    return b"\x02" + fn + arg


I = lam(var(0))
FALSE = lam(lam(var(0)))
NIL = FALSE
PAIR = lam(lam(lam(app(app(var(0), var(2)), var(1)))))
B0 = app(I, FALSE)
B1 = app(FALSE, I)


def bit(value: int) -> bytes:
    return B1 if value else B0


def pair(left: bytes, right: bytes) -> bytes:
    return app(app(PAIR, left), right)


def byte_term(value: int) -> bytes:
    bits = [(value >> shift) & 1 for shift in range(7, -1, -1)]
    result = bit(bits[-1])
    for value_bit in reversed(bits[:-1]):
        result = pair(bit(value_bit), result)
    return result


def bytes_term(values: bytes) -> bytes:
    result = NIL
    for value in reversed(values):
        result = app(byte_term(value), result)
    return result


def byte_left_mutant(value: int) -> bytes:
    bits = [(value >> shift) & 1 for shift in range(7, -1, -1)]
    result = pair(bit(bits[0]), bit(bits[1]))
    for value_bit in bits[2:]:
        result = pair(result, bit(value_bit))
    return result


def byte_reversed_mutant(value: int) -> bytes:
    bits = [(value >> shift) & 1 for shift in range(0, 8)]
    result = bit(bits[-1])
    for value_bit in reversed(bits[:-1]):
        result = pair(bit(value_bit), result)
    return result


def bytes_left_mutant(values: bytes) -> bytes:
    result = NIL
    for value in values:
        result = app(result, byte_term(value))
    return result


def main() -> None:
    edit = bytes.fromhex(
        "028000000000000000fffffffffffffffe7fffffffffffffff"
    )
    edit_preimage = tag_preimage(b"edit", EDIT_SCHEMA, edit)
    quote = QUOTE_PREFIX + edit_preimage
    policy = content_digest(b"policy", POLICY_SCHEMA, POLICY_PAYLOAD)
    source = hashlib.sha256(edit_preimage).digest()
    expanded = bytes_term(quote)
    term = content_digest(b"ladder-term", TERM_SCHEMA, expanded)

    length = len(quote)
    atoms = 185 * length + 2
    iota = 736 * length + 6
    ticks = iota - 1
    receipt_payload = (
        source
        + policy
        + term
        + struct.pack(">QQQQ", length, atoms, iota, ticks)
    )
    receipt = content_digest(
        b"ladder-quote-receipt", RECEIPT_SCHEMA, receipt_payload
    )

    assert len(edit) == 25
    assert len(edit_preimage) == 103
    assert length == 112
    assert len(expanded) == 46_491
    assert len(receipt_payload) == 128
    assert atoms == 20_722
    assert iota == 82_438
    assert ticks == 82_437

    left_byte = app(byte_left_mutant(quote[0]), bytes_term(quote[1:]))
    reversed_byte = app(byte_reversed_mutant(quote[0]), bytes_term(quote[1:]))
    left_list = bytes_left_mutant(quote)

    print(f"source_preimage_bytes={len(edit_preimage)}")
    print(f"quoted_bytes={length}")
    print(f"expanded_lam_bytes={len(expanded)}")
    print(f"quote_sha256={hashlib.sha256(quote).hexdigest()}")
    print(text_tag("edit", source))
    print(text_tag("policy", policy))
    print(text_tag("ladder-term", term))
    print(text_tag("ladder-quote-receipt", receipt))
    print(f"atoms_upper={atoms} iota_upper={iota} ticks={ticks}")
    print(f"right_vs_left_byte={expanded != left_byte}")
    print(f"msb_vs_reversed_byte={expanded != reversed_byte}")
    print(f"right_vs_left_list={expanded != left_list}")


if __name__ == "__main__":
    main()
