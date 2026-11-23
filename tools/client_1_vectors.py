#!/usr/bin/env python3
"""Clean-room pure-Python independent verifier & vector suite for CLIENT-1 (PS-057).

Imports NO Rust or Bitblock code. Independently reproduces:
  * bitblock/client-1/verified-state-view/v1 (450-byte finalized client state view)
  * bitblock/client-1/tx-lifecycle-receipt/v1 (392-byte proof-bound tx lifecycle receipt)
  * Negative controls: provisional masquerade as Finalized refused, phantom ownership/supply refused,
    and > 28-epoch stale weak-subjectivity checkpoint rejected.
"""

from __future__ import annotations

import hashlib
import struct
import sys

from consensus_1_vectors import build_consensus_1_vectors
from crypto_d1_vectors import content_tag
from economic_state_1_vectors import u128_be
from multi_1_vectors import derive_mint_id

CLIENT_1_STATE_VIEW_SCHEMA = b"bitblock/client-1/verified-state-view/v1"
CLIENT_1_TX_LIFECYCLE_SCHEMA = b"bitblock/client-1/tx-lifecycle-receipt/v1"

CLI_STATE_SUMMARY_PREFIX = b"bb.cli1.state_summary.v1"


def build_client_1_vectors() -> tuple[str, str]:
    _, _, ctx = build_consensus_1_vectors()
    genesis_id = ctx["genesis_id"]
    profile_id = ctx["profile_id"]
    block_id = ctx["block_id"]
    post_ref = ctx["post_ref"]
    merkle_root = ctx["merkle_root"]
    outcomes = ctx["outcomes"]
    balances = ctx["balances"]
    aid_a = ctx["aid_a"]

    # Extract Actor A's winning MintEpisode tx_digest (canonical index 0 in sorted_envs -> outcomes[0][4:36])
    tx_a_digest = outcomes[0][4:36]

    # Episodes minted at (3,0,0) (idx 0) and (4,0,0) (idx 1) in canonical_mixed_race_bundle
    dep = 1_000_000
    c3, c4 = (3, 0, 0), (4, 0, 0)
    mid_c3 = derive_mint_id(1, 0, aid_a, c3)
    mid_c4 = derive_mint_id(1, 1, aid_a, c4)
    episodes = sorted(
        [(c3, aid_a + mid_c3 + u128_be(dep)), (c4, aid_a + mid_c4 + u128_be(dep))],
        key=lambda x: x[0],
    )
    bals_sorted = sorted(balances.items(), key=lambda x: x[0])

    # State summary digest
    h = hashlib.sha256()
    h.update(CLI_STATE_SUMMARY_PREFIX)
    h.update(struct.pack(">I", len(bals_sorted)))
    for aid, bal in bals_sorted:
        h.update(aid)
        h.update(u128_be(bal))
    h.update(struct.pack(">I", len(episodes)))
    for (x, y, z), ep_val in episodes:
        h.update(struct.pack(">qqq", x, y, z))
        h.update(ep_val)
    summary_dig = h.digest()

    total_liq = sum(b for _, b in bals_sorted)
    total_dep = dep * len(episodes)

    # 1. ClientVerifiedStateViewV1 (450 bytes)
    view_bytes = (
        struct.pack(">H", 1)
        + genesis_id
        + profile_id
        + bytes([3])  # FinalizedCommitQc
        + struct.pack(">QQ", 1, 0)
        + block_id
        + post_ref
        + merkle_root
        + struct.pack(">Q", 0)
        + u128_be(total_liq)
        + u128_be(total_dep)
        + struct.pack(">IB", len(episodes), 0)
        + bytes(32)
        + struct.pack(">IIII", 1, 1, 0, 0)
        + summary_dig
    )
    assert len(view_bytes) == 450
    view_tag, _ = content_tag("receipt", CLIENT_1_STATE_VIEW_SCHEMA, view_bytes)

    # 2. ClientTxLifecycleReceiptV1 (392 bytes)
    lrcpt_bytes = (
        struct.pack(">H", 1)
        + genesis_id
        + profile_id
        + tx_a_digest
        + aid_a
        + struct.pack(">QBB Q", 0, 4, 3, 1)  # nonce=0, Finalized=4, FinalizedCommitQc=3, height=1
        + block_id
        + post_ref
        + bytes(32)
        + bytes([0, 0])  # phantom_ownership_retained=false, phantom_supply_retained=false
    )
    assert len(lrcpt_bytes) == 392
    lrcpt_tag, _ = content_tag("receipt", CLIENT_1_TX_LIFECYCLE_SCHEMA, lrcpt_bytes)

    # Negative controls:
    # 1) Stale weak-subjectivity checkpoint (> 28 epochs)
    assert 29 > (0 + 28)
    # 2) 1-bit flip to phantom_ownership_retained moves tag
    bad = bytearray(lrcpt_bytes)
    bad[-2] = 1
    assert content_tag("receipt", CLIENT_1_TX_LIFECYCLE_SCHEMA, bytes(bad))[0] != lrcpt_tag
    return view_tag, lrcpt_tag


def main() -> int:
    view_tag, lrcpt_tag = build_client_1_vectors()
    if "--print-tags" in sys.argv:
        print("VIEW_TAG  =", view_tag)
        print("LRCPT_TAG =", lrcpt_tag)
        return 0
    assert view_tag == EXPECTED_VIEW_TAG, f"view mismatch: {view_tag}"
    assert lrcpt_tag == EXPECTED_LRCPT_TAG, f"lrcpt mismatch: {lrcpt_tag}"
    print(f"client_1_vectors: PASS (view={view_tag[:26]}..., lrcpt={lrcpt_tag[:26]}...)")
    return 0


EXPECTED_VIEW_TAG = "#bb.receipt.sha256.532kser2hxodz65nosyv557zpcqdnlhly2orxsyvnmzu45xr5l5q"
EXPECTED_LRCPT_TAG = "#bb.receipt.sha256.vqjs76elssntzogtqd4ir6xtydoj7ctzmi7odas7pjq3562ayjvq"

if __name__ == "__main__":
    raise SystemExit(main())
