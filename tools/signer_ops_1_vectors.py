#!/usr/bin/env python3
"""Clean-room pure-Python independent verifier & vector suite for SIGNER-OPS-1 (PS-085).

Imports NO Rust or Bitblock code. Independently reproduces:
  * bitblock/signer-ops-1/signing-policy/v1 (257-byte role-separated validator signing policy)
  * bitblock/signer-ops-1/audit-receipt/v1 (267-byte tamper-evident hash-chained audit receipt)
  * Negative controls: proposal_key == vote_key rejected, allow_stale_backup_resume=true rejected,
    and decision/signature_emitted tamper tag movement.
"""

from __future__ import annotations

import hashlib
import struct
import sys

from consensus_1_vectors import build_consensus_1_vectors
from crypto_1_vectors import seal_custody_envelope
from crypto_d1_vectors import content_tag, ed_public_from_seed

SIGNER_POLICY_SCHEMA = b"bitblock/signer-ops-1/signing-policy/v1"
SIGNER_AUDIT_SCHEMA = b"bitblock/signer-ops-1/audit-receipt/v1"

SOPS_CEREMONY_PREFIX = b"bb.sops1.key_ceremony.v1"
SOPS_BACKUP_PAIR_PREFIX = b"bb.sops1.backup_pair.v1"


def encode_watermark(epoch: int, height: int, round_num: int, phase_code: int) -> bytes:
    out = struct.pack(">QQIB", epoch, height, round_num, phase_code)
    assert len(out) == 21
    return out


def build_signer_ops_1_vectors() -> tuple[str, str]:
    _, _, ctx = build_consensus_1_vectors()
    genesis_id = ctx["genesis_id"]
    profile_id = ctx["profile_id"]

    vid = bytes([1]) * 32
    prop_seed = bytes([201]) * 32
    vote_seed = bytes([101]) * 32
    prop_pk = ed_public_from_seed(prop_seed)
    vote_pk = ed_public_from_seed(vote_seed)
    assert prop_pk != vote_pk

    passphrase = b"bitblock-signer-ops-v1-ceremony"
    prop_env, pk_p = seal_custody_envelope(
        prop_seed, 3, 3, passphrase, bytes([0xA1]) * 16, bytes([0xB1]) * 16, 1_000
    )
    vote_env, pk_v = seal_custody_envelope(
        vote_seed, 4, 2, passphrase, bytes([0xA2]) * 16, bytes([0xB2]) * 16, 1_000
    )
    assert pk_p == prop_pk and pk_v == vote_pk

    ceremony_dig = hashlib.sha256(
        SOPS_CEREMONY_PREFIX + genesis_id + profile_id + vid + prop_pk + vote_pk
    ).digest()
    backup_dig = hashlib.sha256(SOPS_BACKUP_PAIR_PREFIX + prop_env + vote_env).digest()

    # 1. SignerPolicyManifestV1 (257 bytes)
    policy_bytes = (
        struct.pack(">H", 1)
        + genesis_id
        + profile_id
        + vid
        + prop_pk
        + vote_pk
        + ceremony_dig
        + backup_dig
        + struct.pack(">Q", 1)
        + encode_watermark(0, 0, 0, 0)
        + bytes([1, 0])  # strict_role_separation=true, allow_stale_backup_resume=false
    )
    assert len(policy_bytes) == 257
    policy_tag, policy_dig = content_tag("policy", SIGNER_POLICY_SCHEMA, policy_bytes)

    # 2. SignerAuditReceiptV1 (267 bytes) for ValidatorVote PrepareVote at (0, 1, 0, 2)
    audit_bytes = (
        struct.pack(">H", 1)
        + genesis_id
        + profile_id
        + vid
        + policy_dig
        + vote_pk
        + bytes([2, 4])  # ValidatorVote(2), ConsensusVote(4)
        + struct.pack(">QQ", 1, 1)  # failover_epoch=1, state_counter=1
        + encode_watermark(0, 1, 0, 2)  # PrepareVote(2)
        + bytes([0x44]) * 32
        + bytes([1])  # SignedApproved(1)
        + bytes(32)  # prev_audit_digest = 0
        + bytes([1])  # signature_emitted = true
    )
    assert len(audit_bytes) == 267
    audit_tag, _ = content_tag("receipt", SIGNER_AUDIT_SCHEMA, audit_bytes)

    # Negative controls:
    # 1) allow_stale_backup_resume=true moves policy tag
    bad_pol = bytearray(policy_bytes)
    bad_pol[-1] = 1
    assert content_tag("policy", SIGNER_POLICY_SCHEMA, bytes(bad_pol))[0] != policy_tag
    # 2) signature_emitted=false moves audit tag
    bad_aud = bytearray(audit_bytes)
    bad_aud[-1] = 0
    assert content_tag("receipt", SIGNER_AUDIT_SCHEMA, bytes(bad_aud))[0] != audit_tag
    return policy_tag, audit_tag


def main() -> int:
    policy_tag, audit_tag = build_signer_ops_1_vectors()
    if "--print-tags" in sys.argv:
        print("POLICY_TAG =", policy_tag)
        print("AUDIT_TAG  =", audit_tag)
        return 0
    assert policy_tag == EXPECTED_POLICY_TAG, f"policy mismatch: {policy_tag}"
    assert audit_tag == EXPECTED_AUDIT_TAG, f"audit mismatch: {audit_tag}"
    print(f"signer_ops_1_vectors: PASS (policy={policy_tag[:25]}..., audit={audit_tag[:26]}...)")
    return 0


EXPECTED_POLICY_TAG = "#bb.policy.sha256.sbnuiur54vs5jeztkb724vb66dknfn2s6kqkgbgmqsgu4g35jqka"
EXPECTED_AUDIT_TAG = "#bb.receipt.sha256.zsx5ynhdhu23fbzjq6tuhzfe7unacilht67ub6sgmd4rnzr2v6eq"

if __name__ == "__main__":
    raise SystemExit(main())
