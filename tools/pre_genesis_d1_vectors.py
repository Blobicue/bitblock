#!/usr/bin/env python3
"""Independent stdlib vectors for IDENTITY-D1, ASSET-D1, VALIDATOR-ECON-D1, CONSENSUS-D1,
and GOVERNANCE-D1 pre-genesis canonical schemas.

Imports no Rust/Bitblock code. Reconstructs all six canonical schemas:
  * bitblock/identity-d1/authorization-descriptor/v1
  * bitblock/identity-d1/transaction-body/v1
  * bitblock/asset-d1/policy/v1
  * bitblock/validator-econ-d1/policy/v1
  * bitblock/consensus-d1/policy/v1
  * bitblock/governance-d1/policy/v1
"""

from __future__ import annotations

import base64
import hashlib
import struct
import sys

# Reuse pure-Python Ed25519 seed-to-pubkey derivation locally (stdlib only, no Rust).
from crypto_d1_vectors import content_tag, ed_public_from_seed

IDENTITY_AUTH_SCHEMA = b"bitblock/identity-d1/authorization-descriptor/v1"
IDENTITY_TX_BODY_SCHEMA = b"bitblock/identity-d1/transaction-body/v1"
ASSET_POLICY_SCHEMA = b"bitblock/asset-d1/policy/v1"
VALIDATOR_ECON_POLICY_SCHEMA = b"bitblock/validator-econ-d1/policy/v1"
CONSENSUS_POLICY_SCHEMA = b"bitblock/consensus-d1/policy/v1"
VALIDATOR_SET_ROOT_SCHEMA = b"bitblock/consensus-d1/validator-set-root/v1"
VOTE_SIGN_DOC_SCHEMA = b"bitblock/consensus-d1/vote-sign-doc/v1"
GOVERNANCE_POLICY_SCHEMA = b"bitblock/governance-d1/policy/v1"


def u128_be(v: int) -> bytes:
    return v.to_bytes(16, "big")


def encode_threshold_policy(role: int, threshold: int, keys: list[bytes]) -> bytes:
    assert 1 <= threshold <= len(keys) <= 32
    assert keys == sorted(set(keys))
    return struct.pack(">BHH", role, threshold, len(keys)) + b"".join(keys)


def encode_auth_descriptor(
    version: int,
    action_policy: bytes,
    admin_policy: bytes,
    delegations: list[tuple[bytes, bool, bool, int]],
    revoked: bool,
) -> bytes:
    out = bytearray()
    out += struct.pack(">Q", version)
    out += action_policy
    out += admin_policy
    out += struct.pack(">H", len(delegations))
    for actor_id, allow_on, allow_off, exp in delegations:
        flags = (1 if allow_on else 0) | ((1 if allow_off else 0) << 1)
        out += actor_id + struct.pack(">BQ", flags, exp)
    out += struct.pack(">B", 1 if revoked else 0)
    return bytes(out)


def derive_actor_id(chain_id: int, namespace: str, desc_bytes: bytes) -> bytes:
    ns = namespace.encode("utf-8")
    return hashlib.sha256(
        b"bitblock/actor-identity/v1"
        + struct.pack(">QI", chain_id, len(ns))
        + ns
        + struct.pack(">I", len(desc_bytes))
        + desc_bytes
    ).digest()


def encode_tx_body(
    actor_id: bytes,
    nonce: int,
    valid_after: int,
    valid_until: int,
    expected_root: bytes | None,
    max_fee: int,
    ops: list[tuple[int, int, int, int]],
) -> bytes:
    out = bytearray()
    out += actor_id
    out += struct.pack(">QQQ", nonce, valid_after, valid_until)
    if expected_root is None:
        out += b"\x00"
    else:
        out += b"\x01" + expected_root
    out += u128_be(max_fee)
    out += struct.pack(">H", len(ops))
    for code, x, y, z in ops:
        assert code in (0, 1), "Toggle forbidden on wire"
        out += struct.pack(">Bqqq", code, x, y, z)
    return bytes(out)


def encode_asset_policy() -> bytes:
    return (
        struct.pack(">HBBBB", 1, 1, 0, 1, 0)
        + u128_be(1_000_000)
        + u128_be(1_000)
        + u128_be(1_000_000_000_000_000_000)
    )


def encode_validator_econ_policy() -> bytes:
    return (
        struct.pack(">H", 1)
        + u128_be(100_000_000_000)
        + struct.pack(">QQQHHH", 21, 2, 21, 5_000, 100, 2_500)
        + u128_be(1_000_000_000)
    )


def encode_consensus_policy() -> bytes:
    return struct.pack(">HBHHHQBBBBBB", 1, 1, 2, 3, 128, 28, 1, 1, 1, 1, 1, 0)


def encode_governance_policy(council_keys: list[bytes]) -> bytes:
    council = encode_threshold_policy(5, 2, sorted(council_keys))
    return struct.pack(">HHHQQBBB", 1, 2, 3, 21, 3, 1, 1, 1) + council


def main() -> int:
    pk1 = ed_public_from_seed(bytes([1]) * 32)
    pk2 = ed_public_from_seed(bytes([2]) * 32)
    pk3 = ed_public_from_seed(bytes([3]) * 32)
    action_pol = encode_threshold_policy(0, 2, sorted([pk1, pk2]))
    admin_pol = encode_threshold_policy(1, 1, [pk3])
    desc_bytes = encode_auth_descriptor(0, action_pol, admin_pol, [], False)
    auth_tag, _ = content_tag("policy", IDENTITY_AUTH_SCHEMA, desc_bytes)
    actor_id = derive_actor_id(1, "bitblock-mainnet-v1", desc_bytes)

    tx_bytes = encode_tx_body(
        bytes([0x07]) * 32,
        0,
        10,
        20,
        bytes([0x55]) * 32,
        1_000,
        [(0, 1, 2, 3), (1, -4, 5, -6)],
    )
    tx_tag, _ = content_tag("intent", IDENTITY_TX_BODY_SCHEMA, tx_bytes)

    asset_bytes = encode_asset_policy()
    assert len(asset_bytes) == 54
    asset_tag, _ = content_tag("policy", ASSET_POLICY_SCHEMA, asset_bytes)

    econ_bytes = encode_validator_econ_policy()
    assert len(econ_bytes) == 64
    econ_tag, _ = content_tag("policy", VALIDATOR_ECON_POLICY_SCHEMA, econ_bytes)

    cons_bytes = encode_consensus_policy()
    assert len(cons_bytes) == 23
    cons_tag, _ = content_tag("policy", CONSENSUS_POLICY_SCHEMA, cons_bytes)

    council_keys = [ed_public_from_seed(bytes([s]) * 32) for s in (31, 32, 33)]
    gov_bytes = encode_governance_policy(council_keys)
    gov_tag, _ = content_tag("policy", GOVERNANCE_POLICY_SCHEMA, gov_bytes)

    assert auth_tag == "#bb.policy.sha256.ewnhqdwxbhpuvwzeghndqdsssfsaz7u26xgwbxnporebo5poqd3q", auth_tag
    assert actor_id.hex() == "c830de027d46f85cbf460a219bd3098e2452476e30b2e598a25743c9a94b30de", actor_id.hex()
    assert tx_tag == "#bb.intent.sha256.foomjcxk5bnt4xzqhmt4zhwxa7hkdq37ykbkijliwasy4gwymela", tx_tag
    assert asset_tag == "#bb.policy.sha256.nxy3syo3wcq5ch7ayryyut25sbyustu4snj5vphox52fsnnlo4ca", asset_tag
    assert econ_tag == "#bb.policy.sha256.uzhiakc4bqij66hyg6h75kxsrlxqfaunxd7yzx36kdlxvii2gpaa", econ_tag
    assert cons_tag == "#bb.policy.sha256.7g333rrr5tpxptksywvawnlgbqvyppsfpxkefmtzanv6rfsz5tla", cons_tag
    assert gov_tag == "#bb.policy.sha256.advjjkb4urtxkycounxi54i7x4x7uvefeqcatwnrzatpys7n2lwa", gov_tag

    # Validator set root and VoteSignDocV1 independent reconstructions
    val_h = hashlib.sha256()
    val_h.update(VALIDATOR_SET_ROOT_SCHEMA)
    val_h.update(struct.pack(">H", 4))
    for i in range(1, 5):
        val_h.update(bytes([i]) * 32)
        val_h.update(ed_public_from_seed(bytes([40 + i]) * 32))
        val_h.update(ed_public_from_seed(bytes([50 + i]) * 32))
        val_h.update(u128_be(100))
    vset_root = val_h.digest()
    vote_digest = hashlib.sha256(
        VOTE_SIGN_DOC_SCHEMA
        + struct.pack(">Q", 0)
        + vset_root
        + struct.pack(">QIB", 1, 0, 1)
        + bytes([0x77]) * 32
    ).digest()
    assert (
        vset_root.hex() == "aea25c84571e7148551fe19253af28955045e03ed99352edb2b5ff3ed74a31f9"
    ), vset_root.hex()
    assert (
        vote_digest.hex() == "1c03d0a247b7dcaca4752f85fa31aaa20bdb520951fe8e3fb62b5049cc5ce48d"
    ), vote_digest.hex()

    # Negative controls: distinct schemas/fields never collide and Toggle discriminant rejected.
    assert len({auth_tag, tx_tag, asset_tag, econ_tag, cons_tag, gov_tag}) == 6
    try:
        encode_tx_body(bytes([0x07]) * 32, 0, 10, 20, None, 1_000, [(2, 0, 0, 0)])
    except AssertionError:
        pass
    else:
        raise AssertionError("Toggle op code 2 unexpectedly allowed")

    print(
        f"pre_genesis_d1_vectors: ok (8/8 schemas; vset_root={vset_root.hex()}, vote_digest={vote_digest.hex()})"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
