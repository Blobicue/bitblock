#!/usr/bin/env python3
"""Independent stdlib Python vectors for ASSET-1 (PS-038).

Imports no Rust/Bitblock code. Reuses DomainPatriciaPy from auth_commit_1_vectors
and independently reconstructs both Wave 14A canonical schemas:
  * bitblock/asset-1/asset-state-roots/v1
  * bitblock/asset-1/asset-transition-receipt/v1
"""

from __future__ import annotations

import hashlib
import struct
import sys

from auth_commit_1_vectors import (
    DOM_EPISODE_OWNERSHIP,
    DOM_NATIVE_BALANCE,
    DomainPatriciaPy,
)
from crypto_d1_vectors import content_tag

ASSET_1_ROOTS_SCHEMA = b"bitblock/asset-1/asset-state-roots/v1"
ASSET_1_RECEIPT_SCHEMA = b"bitblock/asset-1/asset-transition-receipt/v1"

EXPECTED_GEN_ROOTS_TAG = (
    "#bb.receipt.sha256.d4c462nqb6gg3rzzrraufi4cjv752ys33gfmep44wkltiposlvkq"
)
EXPECTED_RCPT1_TAG = (
    "#bb.receipt.sha256.vvsftdwi462ds43vitk3d5qx3d3jy2nfxvdk7mupjntewyutrlna"
)
EXPECTED_RCPT2_TAG = (
    "#bb.receipt.sha256.6s7fa3n5ez24u455z7ossxtldebkmbia3svdwuwhdk4txiccss4q"
)
EXPECTED_POST_ROOTS_TAG = (
    "#bb.receipt.sha256.yehegtsuyrnm7w6qa3myuotil2egmf6eeagwc2wgmyonqeublfoa"
)


def encode_u128_be(v: int) -> bytes:
    assert 0 <= v < (1 << 128)
    return v.to_bytes(16, "big")


def coord_key_192(x: int, y: int, z: int) -> bytes:
    sign = 1 << 63
    ux = (x & 0xFFFFFFFFFFFFFFFF) ^ sign
    uy = (y & 0xFFFFFFFFFFFFFFFF) ^ sign
    uz = (z & 0xFFFFFFFFFFFFFFFF) ^ sign
    return struct.pack(">QQQ", ux, uy, uz)


def episode_trie_key(x: int, y: int, z: int) -> bytes:
    return hashlib.sha256(b"bb.asset1.coord_key.v1" + coord_key_192(x, y, z)).digest()


def encode_episode_value(owner: bytes, mint_episode_id: bytes, locked_deposit: int) -> bytes:
    out = owner + mint_episode_id + encode_u128_be(locked_deposit)
    assert len(out) == 80
    return out


def encode_asset_roots(
    profile_id: bytes,
    block_height: int,
    account_count: int,
    episode_count: int,
    total_liquid: int,
    total_locked: int,
    total_burned: int,
    genesis_supply: int,
    balance_root: bytes,
    ownership_root: bytes,
) -> bytes:
    assert total_liquid + total_locked + total_burned == genesis_supply
    out = (
        struct.pack(">H", 1)
        + profile_id
        + struct.pack(">QII", block_height, account_count, episode_count)
        + encode_u128_be(total_liquid)
        + encode_u128_be(total_locked)
        + encode_u128_be(total_burned)
        + encode_u128_be(genesis_supply)
        + balance_root
        + ownership_root
    )
    assert len(out) == 178
    return out


def encode_asset_receipt(
    profile_id: bytes,
    block_height: int,
    op_count: int,
    disposition: int,
    fees_burned_delta: int,
    deposits_locked_delta: int,
    deposits_refunded_delta: int,
    pre_roots_sha256: bytes,
    post_roots_sha256: bytes,
) -> bytes:
    out = (
        struct.pack(">H", 1)
        + profile_id
        + struct.pack(">QHB", block_height, op_count, disposition)
        + encode_u128_be(fees_burned_delta)
        + encode_u128_be(deposits_locked_delta)
        + encode_u128_be(deposits_refunded_delta)
        + pre_roots_sha256
        + post_roots_sha256
    )
    assert len(out) == 157
    return out


def main() -> int:
    profile_id = bytes([0x44]) * 32
    actor_a = bytes([0x11]) * 32
    actor_b = bytes([0x22]) * 32
    fee = 1_000
    dep = 1_000_000
    genesis_supply = 10_000_000

    # Stage 0: Genesis
    bal_0 = {actor_a: encode_u128_be(6_000_000), actor_b: encode_u128_be(4_000_000)}
    ep_0: dict[bytes, bytes] = {}
    r0_bytes = encode_asset_roots(
        profile_id,
        0,
        2,
        0,
        10_000_000,
        0,
        0,
        genesis_supply,
        DomainPatriciaPy(DOM_NATIVE_BALANCE, bal_0).root(),
        DomainPatriciaPy(DOM_EPISODE_OWNERSHIP, ep_0).root(),
    )
    r0_tag, _ = content_tag("receipt", ASSET_1_ROOTS_SCHEMA, r0_bytes)
    r0_sha = hashlib.sha256(r0_bytes).digest()

    # Stage 1: h=10, A mints (1,2,3) and (-4,5,-6)
    bal_1 = {
        actor_a: encode_u128_be(6_000_000 - 2 * (fee + dep)),
        actor_b: encode_u128_be(4_000_000),
    }
    k1 = episode_trie_key(1, 2, 3)
    k2 = episode_trie_key(-4, 5, -6)
    ep_1 = {
        k1: encode_episode_value(actor_a, bytes([0xA1]) * 32, dep),
        k2: encode_episode_value(actor_a, bytes([0xA2]) * 32, dep),
    }
    r1_bytes = encode_asset_roots(
        profile_id,
        10,
        2,
        2,
        10_000_000 - 2 * (fee + dep),
        2 * dep,
        2 * fee,
        genesis_supply,
        DomainPatriciaPy(DOM_NATIVE_BALANCE, bal_1).root(),
        DomainPatriciaPy(DOM_EPISODE_OWNERSHIP, ep_1).root(),
    )
    r1_sha = hashlib.sha256(r1_bytes).digest()
    rcpt1_bytes = encode_asset_receipt(
        profile_id, 10, 2, 0, 2 * fee, 2 * dep, 0, r0_sha, r1_sha
    )
    rcpt1_tag, _ = content_tag("receipt", ASSET_1_RECEIPT_SCHEMA, rcpt1_bytes)

    # Stage 2: h=20, A transfers (1,2,3) -> B, A burns (-4,5,-6)
    bal_2 = {
        actor_a: encode_u128_be(6_000_000 - 2 * (fee + dep) - 2 * fee + dep),
        actor_b: encode_u128_be(4_000_000),
    }
    ep_2 = {
        k1: encode_episode_value(actor_b, bytes([0xA1]) * 32, dep),
    }
    r2_bytes = encode_asset_roots(
        profile_id,
        20,
        2,
        1,
        10_000_000 - dep - 4 * fee,
        dep,
        4 * fee,
        genesis_supply,
        DomainPatriciaPy(DOM_NATIVE_BALANCE, bal_2).root(),
        DomainPatriciaPy(DOM_EPISODE_OWNERSHIP, ep_2).root(),
    )
    r2_tag, _ = content_tag("receipt", ASSET_1_ROOTS_SCHEMA, r2_bytes)
    r2_sha = hashlib.sha256(r2_bytes).digest()
    rcpt2_bytes = encode_asset_receipt(
        profile_id, 20, 2, 0, 2 * fee, 0, dep, r1_sha, r2_sha
    )
    rcpt2_tag, _ = content_tag("receipt", ASSET_1_RECEIPT_SCHEMA, rcpt2_bytes)

    if "--print" in sys.argv:
        print("GEN_ROOTS:", r0_tag)
        print("RCPT1:    ", rcpt1_tag)
        print("RCPT2:    ", rcpt2_tag)
        print("POST_ROOTS:", r2_tag)
        return 0

    assert r0_tag == EXPECTED_GEN_ROOTS_TAG, f"{r0_tag} != {EXPECTED_GEN_ROOTS_TAG}"
    assert rcpt1_tag == EXPECTED_RCPT1_TAG, f"{rcpt1_tag} != {EXPECTED_RCPT1_TAG}"
    assert rcpt2_tag == EXPECTED_RCPT2_TAG, f"{rcpt2_tag} != {EXPECTED_RCPT2_TAG}"
    assert r2_tag == EXPECTED_POST_ROOTS_TAG, f"{r2_tag} != {EXPECTED_POST_ROOTS_TAG}"

    print(
        "asset_1_vectors PASS: 4 golden tags verified across 2 schemas "
        "(AssetStateRootsV1, AssetTransitionReceiptV1)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
