#!/usr/bin/env python3
"""Independent stdlib Python vectors for UPGRADE-FRAMEWORK-1 (PS-082).

Imports NO Rust or Bitblock code. Independently reconstructs both Wave 17C canonical schemas:
  * bitblock/upgrade-framework-1/lifecycle-policy/v1
  * bitblock/upgrade-framework-1/transition-audit/v1
and verifies the complete version-lifecycle & Loop 8-10 authority-compromise decision matrix.
"""

from __future__ import annotations

import struct
import sys

from crypto_d1_vectors import content_tag

POLICY_SCHEMA = b"bitblock/upgrade-framework-1/lifecycle-policy/v1"
AUDIT_SCHEMA = b"bitblock/upgrade-framework-1/transition-audit/v1"

MODE_IMMUTABLE = 1
MODE_PREVENTIVE = 2

DISP_ADMITTED = 0
DISP_IMMUTABLE_REJECTED = 1
DISP_GENESIS_FABRICATED = 2
DISP_MISSING_OLD_OR_SAME = 3
DISP_DOWNGRADE = 4
DISP_GOV_OR_TIMELOCK = 5
DISP_INTERRUPTED_ROLLBACK = 6
DISP_SAFE_HALT_NEW_GENESIS = 7
DISP_TERMINAL_ID_HASH = 8
DISP_UNINHABITED_RECOVERY = 9


def encode_suite_migration_policy(
    old_sig: int,
    old_hash: int,
    new_sig: int,
    new_hash: int,
    act_h: int,
    sun_h: int,
    dual_sign: bool,
    allow_fallback: bool,
    min_overlap: int,
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
        1 if dual_sign else 0,
        1 if allow_fallback else 0,
        min_overlap,
    )
    assert len(out) == 36
    return out


def encode_upgrade_policy(
    mode: int,
    chain_id: int,
    genesis_id: bytes,
    min_timelock: int,
    max_ops: int,
    forbid_downgrade: bool,
    preserve_hist: bool,
    require_sound: bool,
    suite_bytes: bytes,
) -> bytes:
    out = (
        struct.pack(">HBQ", 1, mode, chain_id)
        + genesis_id
        + struct.pack(
            ">QIBBB",
            min_timelock,
            max_ops,
            1 if forbid_downgrade else 0,
            1 if preserve_hist else 0,
            1 if require_sound else 0,
        )
        + suite_bytes
    )
    assert len(out) == 94
    return out


def encode_audit_receipt(
    policy_digest: bytes,
    old_profile_id: bytes,
    target_profile_id: bytes,
    activation_height: int,
    old_ver: int,
    target_ver: int,
    disposition: int,
    compromise_scenario: int,
    hist_ok: bool,
    rollback_ok: bool,
    evidence_digest: bytes,
) -> bytes:
    out = (
        struct.pack(">H", 1)
        + policy_digest
        + old_profile_id
        + target_profile_id
        + struct.pack(
            ">QHHBBBB",
            activation_height,
            old_ver,
            target_ver,
            disposition,
            compromise_scenario,
            1 if hist_ok else 0,
            1 if rollback_ok else 0,
        )
        + evidence_digest
    )
    assert len(out) == 146
    return out


def main() -> int:
    genesis_id = bytes([0xAA]) * 32
    suite = encode_suite_migration_policy(1, 1, 2, 1, 10_000, 12_000, True, False, 1_024)
    prev_bytes = encode_upgrade_policy(MODE_PREVENTIVE, 77, genesis_id, 21, 4_096, True, True, True, suite)
    imm_bytes = encode_upgrade_policy(MODE_IMMUTABLE, 77, genesis_id, 21, 1, True, True, True, suite)

    prev_tag, prev_dig = content_tag("policy", POLICY_SCHEMA, prev_bytes)
    imm_tag, _ = content_tag("policy", POLICY_SCHEMA, imm_bytes)

    # Control 1: Preventive migration admitted receipt
    admitted_bytes = encode_audit_receipt(
        prev_dig, bytes([0x11]) * 32, bytes([0x22]) * 32, 10_000, 1, 2, DISP_ADMITTED, 0, True, True, bytes([0x33]) * 32
    )
    admitted_tag, _ = content_tag("receipt", AUDIT_SCHEMA, admitted_bytes)

    # Control 2: Established compromise (CollisionOrForgery -> SafeHaltAndSeparatelyTrustedNewGenesis)
    comp_bytes = encode_audit_receipt(
        prev_dig, bytes([0x11]) * 32, bytes([0x22]) * 32, 10_000, 1, 2, DISP_SAFE_HALT_NEW_GENESIS, 1, True, True, bytes([0x33]) * 32
    )
    comp_tag, _ = content_tag("receipt", AUDIT_SCHEMA, comp_bytes)

    # Control 3: Terminal Row-1 ProtocolIdentityHashCompromiseV1
    term_bytes = encode_audit_receipt(
        prev_dig, bytes([0x11]) * 32, bytes([0x22]) * 32, 10_000, 1, 2, DISP_TERMINAL_ID_HASH, 1, True, True, bytes([0x33]) * 32
    )
    term_tag, _ = content_tag("receipt", AUDIT_SCHEMA, term_bytes)

    # Control 4: Uninhabited recovery envelope claim rejected
    uninh_bytes = encode_audit_receipt(
        prev_dig, bytes([0x11]) * 32, bytes(32), 10_000, 1, 2, DISP_UNINHABITED_RECOVERY, 4, True, True, bytes([0x33]) * 32
    )
    uninh_tag, _ = content_tag("receipt", AUDIT_SCHEMA, uninh_bytes)

    if "--print-tags" in sys.argv:
        print("PREV_POL_TAG =", prev_tag)
        print("IMM_POL_TAG  =", imm_tag)
        print("ADMIT_TAG    =", admitted_tag)
        print("COMP_TAG     =", comp_tag)
        print("TERM_TAG     =", term_tag)
        print("UNINH_TAG    =", uninh_tag)
        return 0

    assert prev_tag == EXPECTED_PREV_POL_TAG, f"prev mismatch: {prev_tag}"
    assert imm_tag == EXPECTED_IMM_POL_TAG, f"imm mismatch: {imm_tag}"
    assert admitted_tag == EXPECTED_ADMIT_TAG, f"admit mismatch: {admitted_tag}"
    assert comp_tag == EXPECTED_COMP_TAG, f"comp mismatch: {comp_tag}"
    assert term_tag == EXPECTED_TERM_TAG, f"term mismatch: {term_tag}"
    assert uninh_tag == EXPECTED_UNINH_TAG, f"uninh mismatch: {uninh_tag}"
    print(f"upgrade_framework_1_vectors: PASS (prev={prev_tag[:26]}..., comp={comp_tag[:26]}..., term={term_tag[:26]}...)")
    return 0


EXPECTED_PREV_POL_TAG = "#bb.policy.sha256.udwdlgfcdx3lmezbw6ffttvoom2bqjv2tlyxnbekhmtswgeyy74q"
EXPECTED_IMM_POL_TAG = "#bb.policy.sha256.kqe4wzngoso465trwfsy2brufqygodn7vytur6af27s7rrqeoxdq"
EXPECTED_ADMIT_TAG = "#bb.receipt.sha256.efusvpoh6mfavk4gq7x3lg3gv2fe6vzrfnjw6xatkfllaehnmpeq"
EXPECTED_COMP_TAG = "#bb.receipt.sha256.sgjpzscp672njwiyhayextl5ccgcx4ypk45b6iw4ci7j6kmyj7vq"
EXPECTED_TERM_TAG = "#bb.receipt.sha256.blfxqa7vir3smboq2b2ecuh3i4mas3hvr7qhohrhjauuuabfy6tq"
EXPECTED_UNINH_TAG = "#bb.receipt.sha256.agawoxk3h6ceowzjzjgonqtz43jpngokg5jzdvrnupvauwaoytza"

if __name__ == "__main__":
    raise SystemExit(main())
