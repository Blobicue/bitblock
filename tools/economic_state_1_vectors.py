#!/usr/bin/env python3
"""Independent stdlib Python vectors for ECONOMIC-STATE-1 (PS-040).

Imports no Rust/Bitblock code. Reuses DomainPatriciaPy from auth_commit_1_vectors
and independently reconstructs both Wave 15A canonical schemas:
  * bitblock/economic-state-1/economic-root/v1
  * bitblock/economic-state-1/economic-receipt/v1
"""

from __future__ import annotations

import hashlib
import struct
import sys

from auth_commit_1_vectors import DOM_ECONOMIC_STAKE, DomainPatriciaPy
from crypto_d1_vectors import content_tag

ECONOMIC_ROOT_SCHEMA = b"bitblock/economic-state-1/economic-root/v1"
ECONOMIC_RECEIPT_SCHEMA = b"bitblock/economic-state-1/economic-receipt/v1"

EXPECTED_REW_RCPT_TAG = (
    "#bb.receipt.sha256.r2jxjtxll4lptlqmd7d5gkbvlzphwe2m2rkb5rlr364sjqq43ndq"
)
EXPECTED_SLASH_ROOT_TAG = (
    "#bb.receipt.sha256.x4sivbvgyjbvy7od5mxdfoiptaywamcdg42754yayqdqnjpaes7q"
)
EXPECTED_FINAL_ROOT_TAG = (
    "#bb.receipt.sha256.kc7wgdidj5xxkz7v345cskrfi6x6vpfs6fbije4pdhv3m5wysreq"
)
EXPECTED_SLASH_RCPT_TAG = (
    "#bb.receipt.sha256.kzjsjcb6un22a2u45uyci43gky5bh7o7r3h6xpxzv5aisfx5o7xa"
)


def u128_be(v: int) -> bytes:
    assert 0 <= v < (1 << 128)
    return v.to_bytes(16, "big")


def encode_val_record(
    bonded: int,
    rewards: int,
    slashed: int,
    jailed: bool,
    slash_count: int,
    unbonding: list[tuple[int, int]],
) -> bytes:
    out = (
        u128_be(bonded)
        + u128_be(rewards)
        + u128_be(slashed)
        + struct.pack(">BIH", 1 if jailed else 0, slash_count, len(unbonding))
    )
    for amt, unlock in unbonding:
        out += u128_be(amt) + struct.pack(">Q", unlock)
    return out


def encode_econ_root(
    profile_id: bytes,
    epoch: int,
    val_count: int,
    active_count: int,
    active_bonded: int,
    unbonding: int,
    accrued: int,
    withdrawn: int,
    slashed_burned: int,
    genesis_pool: int,
    added_bonds: int,
    issued_rewards: int,
    trie_root: bytes,
) -> bytes:
    assert (
        genesis_pool + added_bonds + issued_rewards
        == active_bonded + unbonding + accrued + withdrawn + slashed_burned
    )
    out = (
        struct.pack(">H", 1)
        + profile_id
        + struct.pack(">QII", epoch, val_count, active_count)
        + u128_be(active_bonded)
        + u128_be(unbonding)
        + u128_be(accrued)
        + u128_be(withdrawn)
        + u128_be(slashed_burned)
        + u128_be(genesis_pool)
        + u128_be(added_bonds)
        + u128_be(issued_rewards)
        + trie_root
    )
    assert len(out) == 210
    return out


def encode_econ_receipt(
    profile_id: bytes,
    epoch: int,
    action_kind: int,
    disposition: int,
    amount_delta: int,
    dust_unissued: int,
    subject_digest: bytes,
    pre_sha: bytes,
    post_sha: bytes,
) -> bytes:
    out = (
        struct.pack(">H", 1)
        + profile_id
        + struct.pack(">QBB", epoch, action_kind, disposition)
        + u128_be(amount_delta)
        + u128_be(dust_unissued)
        + subject_digest
        + pre_sha
        + post_sha
    )
    assert len(out) == 172
    return out


def main() -> int:
    profile_id = bytes([0x66]) * 32
    v = [bytes([i]) * 32 for i in (1, 2, 3, 4)]
    stake = 100_000_000_000
    gen_pool = 4 * stake

    # Genesis
    t0 = DomainPatriciaPy(
        DOM_ECONOMIC_STAKE,
        {vid: encode_val_record(stake, 0, 0, False, 0, []) for vid in v},
    )
    r0_bytes = encode_econ_root(
        profile_id, 0, 4, 4, gen_pool, 0, 0, 0, 0, gen_pool, 0, 0, t0.root()
    )
    r0_sha = hashlib.sha256(r0_bytes).digest()

    # 1. Rewards at epoch 1: budget 999_999_999 -> 249_999_999 each, issued=999_999_996, dust=3
    share = 249_999_999
    issued = 4 * share
    t1 = DomainPatriciaPy(
        DOM_ECONOMIC_STAKE,
        {vid: encode_val_record(stake, share, 0, False, 0, []) for vid in v},
    )
    r1_bytes = encode_econ_root(
        profile_id, 1, 4, 4, gen_pool, 0, issued, 0, 0, gen_pool, 0, issued, t1.root()
    )
    r1_sha = hashlib.sha256(r1_bytes).digest()
    rew_subj = hashlib.sha256(struct.pack(">Q", 1)).digest()
    rew_rcpt_bytes = encode_econ_receipt(
        profile_id, 1, 4, 0, issued, 3, rew_subj, r0_sha, r1_sha
    )
    rew_rcpt_tag, _ = content_tag("receipt", ECONOMIC_RECEIPT_SCHEMA, rew_rcpt_bytes)

    # 2. V1 bonds +10B (e=1), V3 unbonds 20B (e=2 -> unlock=23)
    add_bond = 10_000_000_000
    t2 = DomainPatriciaPy(
        DOM_ECONOMIC_STAKE,
        {
            v[0]: encode_val_record(stake + add_bond, share, 0, False, 0, []),
            v[1]: encode_val_record(stake, share, 0, False, 0, []),
            v[2]: encode_val_record(
                stake - 20_000_000_000, share, 0, False, 0, [(20_000_000_000, 23)]
            ),
            v[3]: encode_val_record(stake, share, 0, False, 0, []),
        },
    )
    r2_bytes = encode_econ_root(
        profile_id,
        2,
        4,
        3,
        gen_pool + add_bond - 20_000_000_000,
        20_000_000_000,
        issued,
        0,
        0,
        gen_pool,
        add_bond,
        issued,
        t2.root(),
    )
    r2_sha = hashlib.sha256(r2_bytes).digest()

    # 3. V3 equivocation slash at e=5 (50% across active 80B->40B & unbonding 20B->10B, 50B burned)
    t3 = DomainPatriciaPy(
        DOM_ECONOMIC_STAKE,
        {
            v[0]: encode_val_record(stake + add_bond, share, 0, False, 0, []),
            v[1]: encode_val_record(stake, share, 0, False, 0, []),
            v[2]: encode_val_record(
                40_000_000_000, share, 50_000_000_000, True, 1, [(10_000_000_000, 23)]
            ),
            v[3]: encode_val_record(stake, share, 0, False, 0, []),
        },
    )
    r3_bytes = encode_econ_root(
        profile_id,
        5,
        4,
        3,
        gen_pool + add_bond - 60_000_000_000,
        10_000_000_000,
        issued,
        0,
        50_000_000_000,
        gen_pool,
        add_bond,
        issued,
        t3.root(),
    )
    r3_sha = hashlib.sha256(r3_bytes).digest()
    slash_root_tag, _ = content_tag("receipt", ECONOMIC_ROOT_SCHEMA, r3_bytes)
    slash_rcpt_bytes = encode_econ_receipt(
        profile_id, 5, 5, 0, 50_000_000_000, 0, bytes([0xE1]) * 32, r2_sha, r3_sha
    )
    slash_rcpt_tag, _ = content_tag("receipt", ECONOMIC_RECEIPT_SCHEMA, slash_rcpt_bytes)

    # 4. V3 matured withdraw at e=23 (10B withdrawn)
    t4 = DomainPatriciaPy(
        DOM_ECONOMIC_STAKE,
        {
            v[0]: encode_val_record(stake + add_bond, share, 0, False, 0, []),
            v[1]: encode_val_record(stake, share, 0, False, 0, []),
            v[2]: encode_val_record(40_000_000_000, share, 50_000_000_000, True, 1, []),
            v[3]: encode_val_record(stake, share, 0, False, 0, []),
        },
    )
    r4_bytes = encode_econ_root(
        profile_id,
        23,
        4,
        3,
        gen_pool + add_bond - 60_000_000_000,
        0,
        issued,
        10_000_000_000,
        50_000_000_000,
        gen_pool,
        add_bond,
        issued,
        t4.root(),
    )
    final_root_tag, _ = content_tag("receipt", ECONOMIC_ROOT_SCHEMA, r4_bytes)

    if "--print" in sys.argv:
        print("REW_RCPT:  ", rew_rcpt_tag)
        print("SLASH_ROOT:", slash_root_tag)
        print("FINAL_ROOT:", final_root_tag)
        print("SLASH_RCPT:", slash_rcpt_tag)
        return 0

    assert rew_rcpt_tag == EXPECTED_REW_RCPT_TAG, f"{rew_rcpt_tag} != {EXPECTED_REW_RCPT_TAG}"
    assert slash_root_tag == EXPECTED_SLASH_ROOT_TAG, (
        f"{slash_root_tag} != {EXPECTED_SLASH_ROOT_TAG}"
    )
    assert final_root_tag == EXPECTED_FINAL_ROOT_TAG, (
        f"{final_root_tag} != {EXPECTED_FINAL_ROOT_TAG}"
    )
    assert slash_rcpt_tag == EXPECTED_SLASH_RCPT_TAG, (
        f"{slash_rcpt_tag} != {EXPECTED_SLASH_RCPT_TAG}"
    )

    print(
        "economic_state_1_vectors PASS: 4 golden tags verified across 2 schemas "
        "(EconomicStateRootV1, EconomicTransitionReceiptV1)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
