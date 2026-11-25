#!/usr/bin/env python3
"""Independent stdlib Python vectors for BITS-ECONOMY-1 (Phase 1 Merge, PR #105).

Imports no Rust/Bitblock code. Independently reconstructs all 8 canonical Bits
creator-economy schemas, Amendment 1 ActorIdV1 derivation, exact 80/20 splits,
strict RFC 8032 Ed25519 signing/verification, and AUTH-COMMIT-1 DomainPatriciaPy
balance/nonce roots:
  * bitblock/bits-1/genesis-config/v1
  * bitblock/bits-1/transfer/v1
  * bitblock/bits-1/purchase-split/v1
  * bitblock/bits-1/testnet-grant/v1
  * bitblock/bits-1/simulated-payout/v1
  * bitblock/bits-1/identity-bind/v1
  * bitblock/bits-1/l2-anchor/v1
  * bitblock/bits-1/state-receipt/v1
"""

from __future__ import annotations

import hashlib
import struct
import sys

from auth_commit_1_vectors import DOM_NATIVE_BALANCE, DOM_NONCE, DomainPatriciaPy
from crypto_d1_vectors import (
    BASE,
    L,
    SIGNING_PREIMAGE_SCHEMA,
    content_tag,
    ed_encode,
    ed_mul,
    ed_public_from_seed,
)
from identity_1_vectors import derive_actor_id, encode_auth_descriptor, encode_threshold_policy

BITS_1_GENESIS_CONFIG_SCHEMA = b"bitblock/bits-1/genesis-config/v1"
BITS_1_TRANSFER_SCHEMA = b"bitblock/bits-1/transfer/v1"
BITS_1_PURCHASE_SPLIT_SCHEMA = b"bitblock/bits-1/purchase-split/v1"
BITS_1_TESTNET_GRANT_SCHEMA = b"bitblock/bits-1/testnet-grant/v1"
BITS_1_SIMULATED_PAYOUT_SCHEMA = b"bitblock/bits-1/simulated-payout/v1"
BITS_1_IDENTITY_BIND_SCHEMA = b"bitblock/bits-1/identity-bind/v1"
BITS_1_L2_ANCHOR_SCHEMA = b"bitblock/bits-1/l2-anchor/v1"
BITS_1_STATE_RECEIPT_SCHEMA = b"bitblock/bits-1/state-receipt/v1"

CHAIN_ID = 0x4249_5453_0000_0001
GENESIS_SUPPLY = 1_000_000_000
BITS_NAMESPACE = "bits.gameladder.v1"

EXPECTED_CFG_TAG = "#bb.policy.sha256.ffoppkqwuspygjt4uroahhuly56g6rhcvxvly6uqyavaeebamm2q"
EXPECTED_GRANT_TAG = "#bb.receipt.sha256.evchb43nwqjxbe6yzeei6azy3szjk2k3utbmhifxwbkrvriz25iq"
EXPECTED_SPLIT_TAG = "#bb.receipt.sha256.pvzttfomarxcv5y62nspvwxmty647hbqaol6u3ryhwcak7fta4dq"
EXPECTED_RECEIPT_TAG = "#bb.receipt.sha256.2nvkbca2plvh6ftaj5yknwj7svq7kzb7q3jbprptli7pth7vnpva"
EXPECTED_TRANSFER_TAG = "#bb.receipt.sha256.d7f3f4762dfz3s5dtqkdcj4ba6h3ssfdwbjw5m2lsvvegwqvhn6q"
EXPECTED_PAYOUT_TAG = "#bb.receipt.sha256.o3yqx3h5qo7fjxlbleywjr5frgb37c5dhwzreykoqvovpbqdctwa"
EXPECTED_BIND_TAG = "#bb.receipt.sha256.5paczgevchguypppkwxu3iehum7x5o6xrjd5u5knc3ntijtag5da"
EXPECTED_ANCHOR_TAG = "#bb.receipt.sha256.js5krkkb4hixcxoh3w4cdrfex6j7t7cvwefhdtky5q3ovcurqjba"


def encode_u128_be(v: int) -> bytes:
    assert 0 <= v < (1 << 128)
    return v.to_bytes(16, "big")


def ed_sign_rfc8032(seed32: bytes, message: bytes) -> bytes:
    assert len(seed32) == 32
    digest = hashlib.sha512(seed32).digest()
    a_bytes = bytearray(digest[:32])
    a_bytes[0] &= 248
    a_bytes[31] &= 127
    a_bytes[31] |= 64
    a = int.from_bytes(a_bytes, "little")
    prefix = digest[32:]
    pub = ed_encode(ed_mul(BASE, a))
    r = int.from_bytes(hashlib.sha512(prefix + message).digest(), "little") % L
    r_enc = ed_encode(ed_mul(BASE, r))
    k = int.from_bytes(hashlib.sha512(r_enc + pub + message).digest(), "little") % L
    s = (r + k * a) % L
    return r_enc + s.to_bytes(32, "little")


def derive_system_actor(chain_id: int, label: str) -> bytes:
    lb = label.encode("utf-8")
    return hashlib.sha256(
        b"bb.bits1.system_actor.v1" + struct.pack(">QI", chain_id, len(lb)) + lb
    ).digest()


def derive_default_bits_actor(chain_id: int, pk32: bytes) -> bytes:
    action = encode_threshold_policy(0, 1, [pk32])
    admin = encode_threshold_policy(1, 1, [pk32])
    desc = encode_auth_descriptor(0, action, admin, [], False)
    return derive_actor_id(chain_id, BITS_NAMESPACE, desc)


def compute_order_memo(order_id: bytes, world_id: str, item_id: str, buyer: bytes) -> bytes:
    wb = world_id.encode("utf-8")
    ib = item_id.encode("utf-8")
    return hashlib.sha256(
        b"gameladder-order/v1\x00"
        + order_id
        + struct.pack(">I", len(wb))
        + wb
        + struct.pack(">I", len(ib))
        + ib
        + buyer
    ).digest()


def encode_genesis_config(
    lab: bool,
    chain_id: int,
    genesis_supply: int,
    initial_grant_pool: int,
    treasury: bytes,
    grant_pool: bytes,
    payout_escrow: bytes,
    max_signup: int = 1000,
    max_faucet: int = 50,
    min_purchase: int = 10,
    creator_bps: int = 8000,
    min_payout: int = 100,
) -> bytes:
    out = (
        struct.pack(">HBQ", 1, 1 if lab else 0, chain_id)
        + encode_u128_be(genesis_supply)
        + encode_u128_be(initial_grant_pool)
        + treasury
        + grant_pool
        + payout_escrow
        + encode_u128_be(max_signup)
        + encode_u128_be(max_faucet)
        + encode_u128_be(min_purchase)
        + struct.pack(">H", creator_bps)
        + encode_u128_be(min_payout)
    )
    assert len(out) == 205
    return out


def encode_signing_preimage(
    protocol_version: int,
    chain_id: int,
    genesis_id: bytes,
    launch_manifest_id: bytes,
    profile_id: bytes,
    domain_code: int,
    payload_digest: bytes,
) -> bytes:
    out = (
        struct.pack(">HQ", protocol_version, chain_id)
        + genesis_id
        + launch_manifest_id
        + profile_id
        + struct.pack(">BHH", domain_code, 1, 1)
        + payload_digest
    )
    assert len(out) == 143
    return out


def build_signed_tx(
    chain_id: int,
    genesis_id: bytes,
    sender_actor: bytes,
    signer_seed: bytes,
    seq: int,
    valid_until: int,
    opcode: int,
    payload_bytes: bytes,
) -> tuple[bytes, bytes]:
    signer_pk = ed_public_from_seed(signer_seed)
    unsigned = (
        struct.pack(">HQ", 1, chain_id)
        + genesis_id
        + sender_actor
        + signer_pk
        + struct.pack(">QQBH", seq, valid_until, opcode, len(payload_bytes))
        + payload_bytes
    )
    payload_digest = hashlib.sha256(b"bb.bits1.signed_tx.v1" + unsigned).digest()
    preimage = encode_signing_preimage(
        1, chain_id, genesis_id, b"\x00" * 32, genesis_id, 1, payload_digest
    )
    _, signing_digest = content_tag("receipt", SIGNING_PREIMAGE_SCHEMA, preimage)
    sig = ed_sign_rfc8032(signer_seed, signing_digest)
    wire = unsigned + sig
    tx_hash = hashlib.sha256(b"bb.bits1.tx_hash.v1" + wire).digest()
    return wire, tx_hash


def main() -> int:
    treasury = derive_system_actor(CHAIN_ID, "platform-treasury")
    grant_pool = derive_system_actor(CHAIN_ID, "testnet-grant-pool")
    payout_escrow = derive_system_actor(CHAIN_ID, "simulated-payout-escrow")

    cfg_bytes = encode_genesis_config(
        True, CHAIN_ID, GENESIS_SUPPLY, GENESIS_SUPPLY, treasury, grant_pool, payout_escrow
    )
    cfg_tag, genesis_id = content_tag("policy", BITS_1_GENESIS_CONFIG_SCHEMA, cfg_bytes)
    assert cfg_tag == EXPECTED_CFG_TAG, cfg_tag

    s1 = bytes([0x11]) * 32
    s2 = bytes([0x22]) * 32
    s_rel = bytes([0x99]) * 32
    pk1 = ed_public_from_seed(s1)
    pk2 = ed_public_from_seed(s2)
    a1 = derive_default_bits_actor(CHAIN_ID, pk1)
    a2 = derive_default_bits_actor(CHAIN_ID, pk2)

    grant_payload = a1 + pk1 + encode_u128_be(1000) + struct.pack(">B", 1) + bytes([0xA1]) * 32
    assert len(grant_payload) == 113
    grant_tag, _ = content_tag("receipt", BITS_1_TESTNET_GRANT_SCHEMA, grant_payload)
    assert grant_tag == EXPECTED_GRANT_TAG, grant_tag

    order_id = bytes([0xB2]) * 32
    memo_hash = compute_order_memo(order_id, "aurora_forge_plaza", "pass_vip", a1)
    split_payload = (
        a1 + a2 + treasury + encode_u128_be(250) + struct.pack(">H", 8000) + order_id + memo_hash
    )
    assert len(split_payload) == 178
    split_tag, _ = content_tag("receipt", BITS_1_PURCHASE_SPLIT_SCHEMA, split_payload)
    assert split_tag == EXPECTED_SPLIT_TAG, split_tag
    _, split_tx_hash = build_signed_tx(
        CHAIN_ID, genesis_id, a1, s1, 1, 10, 0x02, split_payload
    )

    bal_trie = DomainPatriciaPy(
        DOM_NATIVE_BALANCE,
        {
            grant_pool: encode_u128_be(GENESIS_SUPPLY - 1000),
            a1: encode_u128_be(750),
            a2: encode_u128_be(200),
            treasury: encode_u128_be(50),
        },
    )
    nonce_trie = DomainPatriciaPy(
        DOM_NONCE,
        {
            grant_pool: struct.pack(">Q", 1),
            a1: struct.pack(">Q", 1),
        },
    )

    receipt_bytes = (
        struct.pack(">HQ", 1, 2)
        + split_tx_hash
        + struct.pack(">BB", 0x02, 0)
        + a1
        + bal_trie.root()
        + nonce_trie.root()
        + encode_u128_be(GENESIS_SUPPLY - 1000)
        + encode_u128_be(50)
        + encode_u128_be(0)
        + encode_u128_be(GENESIS_SUPPLY)
        + encode_u128_be(200)
        + encode_u128_be(50)
    )
    assert len(receipt_bytes) == 236
    receipt_tag, _ = content_tag("receipt", BITS_1_STATE_RECEIPT_SCHEMA, receipt_bytes)
    assert receipt_tag == EXPECTED_RECEIPT_TAG, receipt_tag

    transfer_payload = a1 + a2 + encode_u128_be(75) + bytes([0xD3]) * 32
    assert len(transfer_payload) == 112
    transfer_tag, _ = content_tag("receipt", BITS_1_TRANSFER_SCHEMA, transfer_payload)
    assert transfer_tag == EXPECTED_TRANSFER_TAG, transfer_tag

    payout_payload = a2 + encode_u128_be(150) + bytes([0xE4]) * 32
    assert len(payout_payload) == 80
    payout_tag, _ = content_tag("receipt", BITS_1_SIMULATED_PAYOUT_SCHEMA, payout_payload)
    assert payout_tag == EXPECTED_PAYOUT_TAG, payout_tag

    ext_hash = bytes([0xCC]) * 32
    pop_msg = hashlib.sha256(
        b"bb.bits1.identity_bind.v1"
        + struct.pack(">Q", CHAIN_ID)
        + genesis_id
        + a1
        + pk1
        + struct.pack(">B", 3)
        + ext_hash
    ).digest()
    pop_sig = ed_sign_rfc8032(s1, pop_msg)
    bind_payload = a1 + pk1 + struct.pack(">B", 3) + ext_hash + pop_sig
    assert len(bind_payload) == 161
    bind_tag, _ = content_tag("receipt", BITS_1_IDENTITY_BIND_SCHEMA, bind_payload)
    assert bind_tag == EXPECTED_BIND_TAG, bind_tag

    app_hash = hashlib.sha256(b"gameladder").digest()
    anchor_payload = app_hash + struct.pack(">Q", 1) + bytes([0xF5]) * 32 + struct.pack(">I", 42)
    assert len(anchor_payload) == 76
    anchor_tag, _ = content_tag("receipt", BITS_1_L2_ANCHOR_SCHEMA, anchor_payload)
    assert anchor_tag == EXPECTED_ANCHOR_TAG, anchor_tag

    for price in (10, 11, 13, 99, 100, 250, 999, 100_000):
        creator_bits = (price * 8000) // 10_000
        platform_bits = price - creator_bits
        assert creator_bits + platform_bits == price

    print(f"RECEIPT_TAG={receipt_tag}")
    print(f"BIND_TAG={bind_tag}")
    print("bits_economy_1_vectors: PASS (8/8 schemas verified)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
