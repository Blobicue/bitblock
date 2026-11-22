#!/usr/bin/env python3
"""Independent stdlib vectors and strict RFC 8032 Ed25519 verifier for CRYPTO-D1.

Imports no Rust/Bitblock code. Reconstructs both canonical CRYPTO-D1 schemas:
  * bitblock/crypto-d1/suite-descriptor/v1
  * bitblock/crypto-d1/signing-preimage/v1

Additionally implements pure-Python strict Edwards25519 (RFC 8032) point decoding,
small-order torsion rejection (all 8 points), non-canonical y >= p rejection,
non-canonical scalar S >= L rejection, and strict Ed25519 signing/verification over
RFC 8032 test vectors and domain-separated SigningPreimageV1 payloads.
"""

from __future__ import annotations

import base64
import hashlib
import struct
import sys

TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"

SUITE_DESCRIPTOR_SCHEMA = b"bitblock/crypto-d1/suite-descriptor/v1"
SIGNING_PREIMAGE_SCHEMA = b"bitblock/crypto-d1/signing-preimage/v1"
KEY_ID_SCHEMA = b"bitblock/crypto-d1/key-id/v1"

# Edwards25519 curve constants (RFC 8032 §5.1)
P = (1 << 255) - 19
L = (1 << 252) + 27742317777372353535851937790883648493
D = (-121665 * pow(121666, P - 2, P)) % P
I_SQRT_M1 = pow(2, (P - 1) // 4, P)
B_Y = (4 * pow(5, P - 2, P)) % P
B_X_SQ = ((B_Y * B_Y - 1) * pow(D * B_Y * B_Y + 1, P - 2, P)) % P
B_X = pow(B_X_SQ, (P + 3) // 8, P)
if (B_X * B_X - B_X_SQ) % P != 0:
    B_X = (B_X * I_SQRT_M1) % P
if B_X & 1:
    B_X = P - B_X
BASE = (B_X, B_Y, 1, (B_X * B_Y) % P)
IDENTITY = (0, 1, 1, 0)


def ed_add(p1: tuple[int, int, int, int], p2: tuple[int, int, int, int]) -> tuple[int, int, int, int]:
    x1, y1, z1, t1 = p1
    x2, y2, z2, t2 = p2
    a = ((y1 - x1) * (y2 - x2)) % P
    b = ((y1 + x1) * (y2 + x2)) % P
    c = (2 * t1 * t2 * D) % P
    d = (2 * z1 * z2) % P
    e = (b - a) % P
    f = (d - c) % P
    g = (d + c) % P
    h = (b + a) % P
    return ((e * f) % P, (g * h) % P, (f * g) % P, (e * h) % P)


def ed_mul(pt: tuple[int, int, int, int], k: int) -> tuple[int, int, int, int]:
    res = IDENTITY
    acc = pt
    while k > 0:
        if k & 1:
            res = ed_add(res, acc)
        acc = ed_add(acc, acc)
        k >>= 1
    return res


def ed_eq(p1: tuple[int, int, int, int], p2: tuple[int, int, int, int]) -> bool:
    return (p1[0] * p2[2] - p2[0] * p1[2]) % P == 0 and (p1[1] * p2[2] - p2[1] * p1[2]) % P == 0


def ed_encode(pt: tuple[int, int, int, int]) -> bytes:
    zinv = pow(pt[2], P - 2, P)
    x = (pt[0] * zinv) % P
    y = (pt[1] * zinv) % P
    out = bytearray(y.to_bytes(32, "little"))
    out[31] |= (x & 1) << 7
    return bytes(out)


def ed_decode_strict_non_weak(raw: bytes) -> tuple[int, int, int, int]:
    if len(raw) != 32:
        raise ValueError("point must be 32 bytes")
    sign = (raw[31] >> 7) & 1
    y_bytes = bytearray(raw)
    y_bytes[31] &= 0x7F
    y = int.from_bytes(y_bytes, "little")
    if y >= P:
        raise ValueError("non-canonical y >= p")
    u = (y * y - 1) % P
    v = (D * y * y + 1) % P
    x2 = (u * pow(v, P - 2, P)) % P
    x = pow(x2, (P + 3) // 8, P)
    if (x * x - x2) % P != 0:
        x = (x * I_SQRT_M1) % P
    if (x * x - x2) % P != 0:
        raise ValueError("not on curve")
    if x == 0 and sign == 1:
        raise ValueError("non-canonical negative zero x")
    if (x & 1) != sign:
        x = P - x
    pt = (x, y, 1, (x * y) % P)
    # Reject all 8 small-order torsion points: 8 * pt == IDENTITY
    if ed_eq(ed_mul(pt, 8), IDENTITY):
        raise ValueError("small-order torsion point")
    return pt


def ed_public_from_seed(seed: bytes) -> bytes:
    h = hashlib.sha512(seed).digest()
    a = bytearray(h[:32])
    a[0] &= 248
    a[31] &= 63
    a[31] |= 64
    scalar = int.from_bytes(a, "little")
    return ed_encode(ed_mul(BASE, scalar))


def ed_sign(seed: bytes, msg: bytes) -> bytes:
    h = hashlib.sha512(seed).digest()
    a = bytearray(h[:32])
    a[0] &= 248
    a[31] &= 63
    a[31] |= 64
    scalar = int.from_bytes(a, "little")
    pk = ed_encode(ed_mul(BASE, scalar))
    r = int.from_bytes(hashlib.sha512(h[32:] + msg).digest(), "little") % L
    r_enc = ed_encode(ed_mul(BASE, r))
    k = int.from_bytes(hashlib.sha512(r_enc + pk + msg).digest(), "little") % L
    s = (r + k * scalar) % L
    return r_enc + s.to_bytes(32, "little")


def ed_verify_strict(pk_bytes: bytes, msg: bytes, sig_bytes: bytes) -> bool:
    if len(sig_bytes) != 64:
        return False
    s = int.from_bytes(sig_bytes[32:], "little")
    if s >= L:
        return False
    try:
        a_pt = ed_decode_strict_non_weak(pk_bytes)
        r_pt = ed_decode_strict_non_weak(sig_bytes[:32])
    except ValueError:
        return False
    k = int.from_bytes(hashlib.sha512(sig_bytes[:32] + pk_bytes + msg).digest(), "little") % L
    # Strict un-cofactored equation: [s]B == R + [k]A
    return ed_eq(ed_mul(BASE, s), ed_add(r_pt, ed_mul(a_pt, k)))


def field(val: bytes) -> bytes:
    return struct.pack(">H", len(val)) + val


def content_tag(kind: str, schema: bytes, payload: bytes) -> tuple[str, bytes]:
    preimage = (
        field(TAG_DOMAIN)
        + field(kind.encode("ascii"))
        + field(HASH_SUITE)
        + field(schema)
        + struct.pack(">Q", len(payload))
        + payload
    )
    digest = hashlib.sha256(preimage).digest()
    encoded = base64.b32encode(digest).decode("ascii").lower().rstrip("=")
    return f"#bb.{kind}.sha256.{encoded}", digest


def encode_suite_descriptor() -> bytes:
    return struct.pack(">HHHHHBBB", 1, 1, 1, 32, 64, 1, 1, 1)


def encode_signing_preimage(
    protocol_version: int,
    chain_id: int,
    genesis_id: bytes,
    launch_manifest_id: bytes,
    profile_id: bytes,
    domain_tag: int,
    sig_suite: int,
    hash_suite: int,
    payload_digest: bytes,
) -> bytes:
    return (
        struct.pack(">HQ", protocol_version, chain_id)
        + genesis_id
        + launch_manifest_id
        + profile_id
        + struct.pack(">BHH", domain_tag, sig_suite, hash_suite)
        + payload_digest
    )


def main() -> int:
    # 1. Suite descriptor canonical tag parity
    suite_bytes = encode_suite_descriptor()
    assert len(suite_bytes) == 13
    suite_tag, _ = content_tag("policy", SUITE_DESCRIPTOR_SCHEMA, suite_bytes)
    assert suite_tag == "#bb.policy.sha256.jczjgonfconrys2yy7cgehmsk6mh6757m4r4piyg5ynglq2uanlq", suite_tag

    # 2. RFC 8032 §7.1 TEST 1 + TEST 2 parity
    sk1 = bytes.fromhex("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60")
    pk1 = bytes.fromhex("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a")
    sig1 = bytes.fromhex(
        "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06522490155"
        "5fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b"
    )
    assert ed_public_from_seed(sk1) == pk1
    assert ed_sign(sk1, b"") == sig1
    assert ed_verify_strict(pk1, b"", sig1)

    sk2 = bytes.fromhex("4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb")
    pk2 = bytes.fromhex("3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c")
    sig2 = bytes.fromhex(
        "92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da"
        "085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00"
    )
    assert ed_public_from_seed(sk2) == pk2
    assert ed_sign(sk2, b"\x72") == sig2
    assert ed_verify_strict(pk2, b"\x72", sig2)

    # 3. Malleated scalar S + L must reject
    s_mal = (int.from_bytes(sig1[32:], "little") + L).to_bytes(32, "little")
    assert not ed_verify_strict(pk1, b"", sig1[:32] + s_mal)

    # 4. All 8 small-order torsion points and non-canonical y = p must reject
    bad_points = [
        "0100000000000000000000000000000000000000000000000000000000000000",
        "ecffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f",
        "0000000000000000000000000000000000000000000000000000000000000000",
        "0000000000000000000000000000000000000000000000000000000000000080",
        "c7176a703d4dd84fba3c0b760d10670f2a2053fa2c39ccc64ec7fd7792ac037a",
        "c7176a703d4dd84fba3c0b760d10670f2a2053fa2c39ccc64ec7fd7792ac03fa",
        "26e8958fc2b227b045c3f489f2ef98f0d5dfac05d3c63339b13802886d53fc05",
        "26e8958fc2b227b045c3f489f2ef98f0d5dfac05d3c63339b13802886d53fc85",
        "edffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f",
    ]
    for hx in bad_points:
        try:
            ed_decode_strict_non_weak(bytes.fromhex(hx))
        except ValueError:
            pass
        else:
            raise AssertionError(f"bad point accepted: {hx}")

    # 5. Domain-separated SigningPreimageV1 parity + cross-domain rejection
    seed = bytes([0x42]) * 32
    pk = ed_public_from_seed(seed)
    pre_bytes = encode_signing_preimage(
        1, 77, bytes([0x11]) * 32, bytes([0x22]) * 32, bytes([0x33]) * 32, 1, 1, 1, bytes([0x44]) * 32
    )
    assert len(pre_bytes) == 143
    pre_tag, pre_digest = content_tag("receipt", SIGNING_PREIMAGE_SCHEMA, pre_bytes)
    assert (
        pre_tag == "#bb.receipt.sha256.blgh6lg27ohjxiv2royj2wbkii7zny2innes6coaeqpy2ckcwoeq"
    ), pre_tag
    sig = ed_sign(seed, pre_digest)
    assert ed_verify_strict(pk, pre_digest, sig)

    wrong_domain_bytes = encode_signing_preimage(
        1, 77, bytes([0x11]) * 32, bytes([0x22]) * 32, bytes([0x33]) * 32, 4, 1, 1, bytes([0x44]) * 32
    )
    _, wrong_digest = content_tag("receipt", SIGNING_PREIMAGE_SCHEMA, wrong_domain_bytes)
    assert not ed_verify_strict(pk, wrong_digest, sig)

    # 6. Key ID derivation parity (bitblock/crypto-d1/key-id/v1)
    pk_seed1 = ed_public_from_seed(bytes([1]) * 32)
    key_id_seed1 = hashlib.sha256(KEY_ID_SCHEMA + struct.pack(">H", 1) + pk_seed1).digest()
    assert (
        key_id_seed1.hex() == "2c9bab9f88a372cb0f49d5fc2b9c7acf52376ec95e0d5cbd7bdfa865ff39bb95"
    ), key_id_seed1.hex()
    print(f"crypto_d1_vectors: ok (suite={suite_tag}, preimage={pre_tag})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
