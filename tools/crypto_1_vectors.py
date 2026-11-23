#!/usr/bin/env python3
"""Independent stdlib Python vectors for CRYPTO-1 (PS-070).

Imports no Rust/Bitblock code. Reconstructs all three Wave 10 canonical schemas:
  * bitblock/crypto-1/key-lifecycle-record/v1
  * bitblock/crypto-1/custody-backup-envelope/v1
  * bitblock/crypto-1/suite-migration-policy/v1
"""

from __future__ import annotations

import hashlib
import hmac
import struct
import sys

from crypto_d1_vectors import content_tag, ed_public_from_seed

KEY_LIFECYCLE_SCHEMA = b"bitblock/crypto-1/key-lifecycle-record/v1"
CUSTODY_ENVELOPE_SCHEMA = b"bitblock/crypto-1/custody-backup-envelope/v1"
MIGRATION_POLICY_SCHEMA = b"bitblock/crypto-1/suite-migration-policy/v1"


def hmac_sha256(key: bytes, data: bytes) -> bytes:
    return hmac.new(key, data, hashlib.sha256).digest()


def encode_lifecycle_record(
    domain_code: int,
    role_code: int,
    status_code: int,
    pk: bytes,
    succ_pk: bytes | None,
    created_h: int,
    eff_h: int,
    quarantine_h: int,
) -> bytes:
    flag = 1 if succ_pk is not None else 0
    succ = succ_pk if succ_pk is not None else (b"\x00" * 32)
    out = (
        struct.pack(">HBBB", 1, domain_code, role_code, status_code)
        + pk
        + struct.pack(">B", flag)
        + succ
        + struct.pack(">QQQ", created_h, eff_h, quarantine_h)
    )
    assert len(out) == 94
    return out


def seal_custody_envelope(
    seed: bytes,
    domain_code: int,
    role_code: int,
    passphrase: bytes,
    salt: bytes,
    nonce: bytes,
    iters: int,
) -> tuple[bytes, bytes]:
    pk = ed_public_from_seed(seed)
    ikm = hashlib.pbkdf2_hmac("sha256", passphrase, salt, iters, dklen=32)
    prk = hmac_sha256(b"bitblock/crypto-1/custody-hkdf-extract/v1", ikm)
    enc_key = hmac_sha256(prk, b"bitblock/crypto-1/custody-enc/v1" + bytes([domain_code, role_code, 1]))
    mac_key = hmac_sha256(prk, b"bitblock/crypto-1/custody-mac/v1" + bytes([domain_code, role_code, 1]))
    ks = hashlib.sha256(
        b"bitblock/crypto-1/custody-ctr/v1" + enc_key + nonce + struct.pack(">I", 0)
    ).digest()
    ct = bytes(a ^ b for a, b in zip(seed, ks))
    mac_in = struct.pack(">HBBI", 1, domain_code, role_code, iters) + pk + salt + nonce + ct
    assert len(mac_in) == 104
    mac = hmac_sha256(mac_key, mac_in)
    env = mac_in + mac
    assert len(env) == 136
    # Verify independent restore roundtrip
    assert hmac.compare_digest(hmac_sha256(mac_key, env[:104]), env[104:])
    restored = bytes(a ^ b for a, b in zip(env[72:104], ks))
    assert restored == seed
    return env, pk


def encode_migration_policy(
    old_sig: int,
    old_hash: int,
    new_sig: int,
    new_hash: int,
    act_h: int,
    sun_h: int,
    dual: bool,
    fallback: bool,
    min_ov: int,
) -> bytes:
    out = struct.pack(
        ">HHHHHQQBBQ",
        1,
        old_sig,
        old_hash,
        new_sig,
        new_hash,
        act_h,
        sun_h,
        1 if dual else 0,
        1 if fallback else 0,
        min_ov,
    )
    assert len(out) == 36
    return out


def compute_tags() -> tuple[str, str, str]:
    seed_a = bytes([0x11]) * 32
    seed_b = bytes([0x22]) * 32
    pk_a = ed_public_from_seed(seed_a)
    pk_b = ed_public_from_seed(seed_b)

    rot_bytes = encode_lifecycle_record(1, 0, 1, pk_a, pk_b, 10, 50, 0)
    rot_tag, _ = content_tag("receipt", KEY_LIFECYCLE_SCHEMA, rot_bytes)

    env_bytes, _ = seal_custody_envelope(
        seed_a,
        1,
        0,
        b"bitblock-devnet-passphrase-v1",
        bytes([0xA1]) * 16,
        bytes([0xB2]) * 16,
        1_000,
    )
    env_tag, _ = content_tag("receipt", CUSTODY_ENVELOPE_SCHEMA, env_bytes)

    mig_bytes = encode_migration_policy(1, 1, 2, 1, 1_000, 3_048, True, False, 1_024)
    mig_tag, _ = content_tag("policy", MIGRATION_POLICY_SCHEMA, mig_bytes)
    return rot_tag, env_tag, mig_tag


EXPECTED_ROT_RECORD_TAG = "#bb.receipt.sha256.3dyl5u2pyn7r4svpu5p7sxnny7qgv3hqui3edgasuwfiiwjzvkcq"
EXPECTED_CUSTODY_ENV_TAG = "#bb.receipt.sha256.lc3kdko2lsupgbqwysbcsoyadi3z62p4pde3ch5ioldg3hqriumq"
EXPECTED_MIGRATION_POLICY_TAG = "#bb.policy.sha256.3r2uumuzxwfdhct2e6xniiatblaj555mjjgxpy4dtdy4tvmup6oq"


def main() -> int:
    rot_tag, env_tag, mig_tag = compute_tags()
    if "--print-tags" in sys.argv:
        print(f"ROT_RECORD_TAG={rot_tag}")
        print(f"CUSTODY_ENV_TAG={env_tag}")
        print(f"MIGRATION_POLICY_TAG={mig_tag}")
    assert rot_tag == EXPECTED_ROT_RECORD_TAG, rot_tag
    assert env_tag == EXPECTED_CUSTODY_ENV_TAG, env_tag
    assert mig_tag == EXPECTED_MIGRATION_POLICY_TAG, mig_tag

    # Negative control: flipping 1 bit of custody envelope moves tag.
    seed_a = bytes([0x11]) * 32
    env_bytes, _ = seal_custody_envelope(
        seed_a, 1, 0, b"bitblock-devnet-passphrase-v1", bytes([0xA1]) * 16, bytes([0xB2]) * 16, 1_000
    )
    bad = bytearray(env_bytes)
    bad[72] ^= 1
    bad_tag, _ = content_tag("receipt", CUSTODY_ENVELOPE_SCHEMA, bytes(bad))
    assert bad_tag != env_tag
    print(f"crypto_1_vectors: ok (rot={rot_tag}, custody={env_tag}, mig={mig_tag}, 3/3 golden tags pinned)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
