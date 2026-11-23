#!/usr/bin/env python3
"""Independent stdlib Python vectors for PROFILE-D1 (PS-079).

Imports no Rust/Bitblock code. Reconstructs both Wave 11 canonical schemas:
  * bitblock/profile-d1/candidate-manifest/v1
  * bitblock/profile-d1/bound-profile-manifest/v1
"""

from __future__ import annotations

import struct
import sys

from crypto_d1_vectors import content_tag
from profile_bootstrap_vectors import decode_tag_digest

CANDIDATE_SCHEMA = b"bitblock/profile-d1/candidate-manifest/v1"
BOUND_SCHEMA = b"bitblock/profile-d1/bound-profile-manifest/v1"

DEVNET_GENESIS_STATE_TAG = "#bb.policy.sha256.jowjx6wyiayipiaypxgmjc2xgynt335d7mrx3qfx2e6pxegyshgq"
CRYPTO_SUITE_TAG = "#bb.policy.sha256.jczjgonfconrys2yy7cgehmsk6mh6757m4r4piyg5ynglq2uanlq"
PROTOCOL_BOUNDS_TAG = "#bb.policy.sha256.yecxf5k5a5gs3y7qm76pzax2f5e6bkidmxytmsthsxhdqozzplaa"
RESOURCE_POLICY_TAG = "#bb.policy.sha256.6yvp73mvdkouoxk2xiolnj33lbxucxta5ikhjcwnvd267tzfrfyq"

LAUNCH_PREDS = [
    "asset-active",
    "economic-charging",
    "in-place-upgrade",
    "migration-profile",
    "stake-consensus",
    "validator-economics",
    "validator-rewards",
]
LAUNCH_CAPS = [
    "ASSET-1",
    "AUTH-COMMIT-1",
    "BOUNDS-1",
    "CODEC-1",
    "COMMIT-1",
    "CONSENSUS-1",
    "CRYPTO-1",
    "EXECUTION-ROOT-1",
    "EXECUTOR-1",
    "GOVERNANCE-1",
    "IDENTITY-1",
    "PROTOCOL-ID-1",
    "RESOURCE-1",
    "TRANSITION-SPEC-1",
    "VALIDATOR-SCHEDULE-1",
]


def pack_str(s: str) -> bytes:
    b = s.encode("utf-8")
    return struct.pack(">H", len(b)) + b


def canonical_slots(asset_active: bool, program_active: bool, verkle_selected: bool) -> list[tuple[int, int, int]]:
    return [
        (0, 1, 1),
        (1, 1, 1),
        (2, 1, 1),
        (3, 1, 1),
        (4, 1, 1),
        (5, 1 if asset_active else 0, 1 if asset_active else 0),
        (6, 1 if program_active else 0, 1 if program_active else 0),
        (7, 1 if verkle_selected else 0, 2 if verkle_selected else 0),
    ]


def canonical_subclaims(preds: set[str]) -> list[tuple[str, int, str, int]]:
    rows = [
        ("G15.economic", 1, "economic-charging", 0),
        ("G15.resource", 0, "", 0),
        (
            "G19.program",
            0 if "program-active" in preds else 2,
            "program-active" if "program-active" in preds else "program-active-inactive-at-launch",
            1 if "program-active" in preds else 0,
        ),
        (
            "G19.upgrade",
            0 if "in-place-upgrade" in preds else 2,
            "in-place-upgrade" if "in-place-upgrade" in preds else "in-place-upgrade-inactive",
            1,
        ),
        (
            "G19.verkle",
            0 if "verkle-selected" in preds else 2,
            "verkle-selected" if "verkle-selected" in preds else "verkle-selected-inactive-at-launch",
            0,
        ),
        ("G5.asset", 1, "asset-active", 1),
        ("G5.core", 0, "", 0),
    ]
    rows.sort(key=lambda r: r[0])
    return rows


def encode_candidate(
    lineage_kind: int,
    predecessor: bytes | None,
    genesis_id: bytes,
    genesis_scope_d: bytes,
    current_scope_d: bytes,
    dom_reg_d: bytes,
    bounds_d: bytes,
    crypto_d: bytes,
    resource_d: bytes,
    preds: list[str],
    caps: list[str],
) -> bytes:
    out = bytearray()
    out += struct.pack(">HBB", 1, lineage_kind, 1 if predecessor is not None else 0)
    out += predecessor if predecessor is not None else (b"\x00" * 32)
    out += genesis_id + genesis_scope_d + current_scope_d + dom_reg_d + bounds_d + crypto_d + resource_d
    out += struct.pack(">H", len(preds))
    for p in preds:
        out += pack_str(p)
    out += struct.pack(">H", len(caps))
    for c in caps:
        out += pack_str(c)
    pset = set(preds)
    slots = canonical_slots("asset-active" in pset, "program-active" in pset, "verkle-selected" in pset)
    out += struct.pack(">H", len(slots))
    for sid, act, cpid in slots:
        out += struct.pack(">BBH", sid, act, cpid)
    subs = canonical_subclaims(pset)
    out += struct.pack(">H", len(subs))
    for sid, disp, pr, ev in subs:
        out += pack_str(sid) + struct.pack(">B", disp) + pack_str(pr) + struct.pack(">B", ev)
    return bytes(out)


def seal_bound(cand_bytes: bytes) -> tuple[str, bytes, str]:
    cand_tag, pid = content_tag("policy", CANDIDATE_SCHEMA, cand_bytes)
    bound_bytes = struct.pack(">I", len(cand_bytes)) + cand_bytes + pid
    bound_tag, _ = content_tag("receipt", BOUND_SCHEMA, bound_bytes)
    return cand_tag, pid, bound_tag


def compute_tags() -> tuple[str, str, str, str]:
    genesis_id = decode_tag_digest(DEVNET_GENESIS_STATE_TAG)
    genesis_scope_d = bytes([0x21]) * 32
    dom_reg_d = bytes([0x31]) * 32
    bounds_d = decode_tag_digest(PROTOCOL_BOUNDS_TAG)
    crypto_d = decode_tag_digest(CRYPTO_SUITE_TAG)
    resource_d = decode_tag_digest(RESOURCE_POLICY_TAG)

    lc_bytes = encode_candidate(
        0, None, genesis_id, genesis_scope_d, genesis_scope_d, dom_reg_d, bounds_d, crypto_d, resource_d,
        LAUNCH_PREDS, LAUNCH_CAPS,
    )
    lc_tag, launch_pid, lb_tag = seal_bound(lc_bytes)

    succ_preds = sorted(LAUNCH_PREDS + ["post-genesis-program", "program-active"])
    succ_caps = sorted(LAUNCH_CAPS + ["PROGRAM-STATE-1", "VM-1"])
    sc_bytes = encode_candidate(
        1, launch_pid, genesis_id, genesis_scope_d, bytes([0x71]) * 32, dom_reg_d, bounds_d, crypto_d, resource_d,
        succ_preds, succ_caps,
    )
    sc_tag, _, sb_tag = seal_bound(sc_bytes)
    return lc_tag, lb_tag, sc_tag, sb_tag


EXPECTED_LAUNCH_CAND_TAG = "#bb.policy.sha256.r4ldc6k33p7evc4zg67ksajcpr6dxeiwuazimy6pxhwgaepepaoq"
EXPECTED_LAUNCH_BOUND_TAG = "#bb.receipt.sha256.24biozgdf4dc7n4z5llunavtkegk433r5qfh4plknnjgtvw4naoq"
EXPECTED_SUCC_CAND_TAG = "#bb.policy.sha256.q3sxi64vigpwbcm2yhh4jgtwm7dfxvsva7k4phdjsswbeqeje6ha"
EXPECTED_SUCC_BOUND_TAG = "#bb.receipt.sha256.6sik5vhte62uajm53n43axcz3lylfpiindawhjvried475wxxxka"


def main() -> int:
    lc_tag, lb_tag, sc_tag, sb_tag = compute_tags()
    if "--print-tags" in sys.argv:
        print(f"LAUNCH_CAND_TAG={lc_tag}")
        print(f"LAUNCH_BOUND_TAG={lb_tag}")
        print(f"SUCC_CAND_TAG={sc_tag}")
        print(f"SUCC_BOUND_TAG={sb_tag}")
    assert lc_tag == EXPECTED_LAUNCH_CAND_TAG, lc_tag
    assert lb_tag == EXPECTED_LAUNCH_BOUND_TAG, lb_tag
    assert sc_tag == EXPECTED_SUCC_CAND_TAG, sc_tag
    assert sb_tag == EXPECTED_SUCC_BOUND_TAG, sb_tag
    assert lc_tag != sc_tag and lb_tag != sb_tag
    print(f"profile_d1_vectors: ok (launch={lc_tag}, succ={sc_tag}, 4/4 golden tags pinned)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
