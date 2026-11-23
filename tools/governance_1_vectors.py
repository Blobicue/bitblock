#!/usr/bin/env python3
"""Independent stdlib Python vectors for GOVERNANCE-1 (PS-081).

Imports no Rust/Bitblock code. Reuses DomainPatriciaPy from auth_commit_1_vectors
and independently reconstructs both Wave 14C canonical schemas:
  * bitblock/governance-1/governance-root/v1
  * bitblock/governance-1/governance-receipt/v1
"""

from __future__ import annotations

import hashlib
import struct
import sys

from auth_commit_1_vectors import DOM_GOVERNANCE, DomainPatriciaPy
from crypto_d1_vectors import content_tag

GOVERNANCE_1_ROOT_SCHEMA = b"bitblock/governance-1/governance-root/v1"
GOVERNANCE_1_RECEIPT_SCHEMA = b"bitblock/governance-1/governance-receipt/v1"
BOUNDS_SAFE_HALT_SCHEMA = b"bitblock/bounds-1/safe-halt-evidence/v1"

EXPECTED_QUEUED_ROOT_TAG = (
    "#bb.receipt.sha256.ro25qjtz5x6jglbdtk4pzqtlm5tqwfkxfpod53hn2amne7otuhxq"
)
EXPECTED_ACT_ROOT_TAG = (
    "#bb.receipt.sha256.u5vnfvqypcrivjeu7d7jrsgfeaottqchqap6dxosvlksqdry5f2q"
)
EXPECTED_PAUSED_ROOT_TAG = (
    "#bb.receipt.sha256.6zrhhqdbeao6dhzcbotcfxyfiolbmrbj2wvoc4axkk2owomfpb6q"
)
EXPECTED_HALTED_ROOT_TAG = (
    "#bb.receipt.sha256.hn4fvvlm3baxpa6ycedxmisfz4zx32j55r2sna5enjzzxfqx4tnq"
)
EXPECTED_RCPT_Q_TAG = (
    "#bb.receipt.sha256.lwhmyl2rjlhlpflygohdh2rpiccdeksimz75mjdo23jdqtktp5ha"
)


def proposal_trie_key(proposal_id: bytes) -> bytes:
    return hashlib.sha256(b"bb.gov1.proposal.v1" + proposal_id).digest()


def param_trie_key(param_key: int) -> bytes:
    return hashlib.sha256(b"bb.gov1.param.v1" + struct.pack(">I", param_key)).digest()


def encode_proposal_record(
    proposal_id: bytes,
    param_key: int,
    param_value: int,
    approved_epoch: int,
    activation_epoch: int,
    expiry_epoch: int,
    status: int,
    is_restart: bool,
) -> bytes:
    out = (
        proposal_id
        + struct.pack(
            ">IQQQQBB",
            param_key,
            param_value,
            approved_epoch,
            activation_epoch,
            expiry_epoch,
            status,
            1 if is_restart else 0,
        )
    )
    assert len(out) == 70
    return out


def encode_gov_root(
    profile_id: bytes,
    current_epoch: int,
    lifecycle: int,
    pause_until: int,
    pause_cooldown: int,
    halted_epoch: int,
    fault_digest: bytes,
    queued_count: int,
    param_count: int,
    trie_root: bytes,
) -> bytes:
    out = (
        struct.pack(">H", 1)
        + profile_id
        + struct.pack(">QBQQQ", current_epoch, lifecycle, pause_until, pause_cooldown, halted_epoch)
        + fault_digest
        + struct.pack(">II", queued_count, param_count)
        + trie_root
    )
    assert len(out) == 139
    return out


def encode_gov_receipt(
    profile_id: bytes,
    epoch: int,
    action_kind: int,
    disposition: int,
    target_digest: bytes,
    pre_sha: bytes,
    post_sha: bytes,
) -> bytes:
    out = (
        struct.pack(">H", 1)
        + profile_id
        + struct.pack(">QBB", epoch, action_kind, disposition)
        + target_digest
        + pre_sha
        + post_sha
    )
    assert len(out) == 140
    return out


def main() -> int:
    profile_id = bytes([0x55]) * 32
    zero32 = bytes(32)
    pid = bytes([0xAA]) * 32
    k_param1 = param_trie_key(1)
    k_pid = proposal_trie_key(pid)

    # Genesis root
    t0 = DomainPatriciaPy(DOM_GOVERNANCE, {k_param1: struct.pack(">Q", 50_000_000)})
    r0_bytes = encode_gov_root(profile_id, 0, 0, 0, 0, 0, zero32, 0, 1, t0.root())
    r0_sha = hashlib.sha256(r0_bytes).digest()

    # 1. Queued at epoch 1
    p_queued = encode_proposal_record(pid, 1, 60_000_000, 1, 22, 30, 0, False)
    t1 = DomainPatriciaPy(
        DOM_GOVERNANCE,
        {k_param1: struct.pack(">Q", 50_000_000), k_pid: p_queued},
    )
    r1_bytes = encode_gov_root(profile_id, 1, 0, 0, 0, 0, zero32, 1, 1, t1.root())
    r1_sha = hashlib.sha256(r1_bytes).digest()
    queued_tag, _ = content_tag("receipt", GOVERNANCE_1_ROOT_SCHEMA, r1_bytes)
    rcpt_q_bytes = encode_gov_receipt(profile_id, 1, 1, 0, pid, r0_sha, r1_sha)
    rcpt_q_tag, _ = content_tag("receipt", GOVERNANCE_1_RECEIPT_SCHEMA, rcpt_q_bytes)

    # 2. Activated at epoch 22
    p_act = encode_proposal_record(pid, 1, 60_000_000, 1, 22, 30, 1, False)
    t2 = DomainPatriciaPy(
        DOM_GOVERNANCE,
        {k_param1: struct.pack(">Q", 60_000_000), k_pid: p_act},
    )
    r2_bytes = encode_gov_root(profile_id, 22, 0, 0, 0, 0, zero32, 0, 1, t2.root())
    act_tag, _ = content_tag("receipt", GOVERNANCE_1_ROOT_SCHEMA, r2_bytes)

    # 3. Emergency paused at epoch 25 (duration 3 -> until 28, cooldown 31)
    r3_bytes = encode_gov_root(profile_id, 25, 1, 28, 31, 0, zero32, 0, 1, t2.root())
    paused_tag, _ = content_tag("receipt", GOVERNANCE_1_ROOT_SCHEMA, r3_bytes)

    # 4. Safe-Halted at epoch 35 from BOUNDS-1 SafeHaltEvidenceV1
    ev_bytes = (
        struct.pack(">H", 1)
        + bytes([0x77]) * 32
        + bytes([0x88]) * 32
        + struct.pack(">BQQ", 3, 20_000, 12_288)
    )
    assert len(ev_bytes) == 83
    _, fault_digest = content_tag("receipt", BOUNDS_SAFE_HALT_SCHEMA, ev_bytes)
    r4_bytes = encode_gov_root(profile_id, 35, 2, 0, 31, 35, fault_digest, 0, 1, t2.root())
    halted_tag, _ = content_tag("receipt", GOVERNANCE_1_ROOT_SCHEMA, r4_bytes)

    if "--print" in sys.argv:
        print("QUEUED_ROOT:", queued_tag)
        print("ACT_ROOT:   ", act_tag)
        print("PAUSED_ROOT:", paused_tag)
        print("HALTED_ROOT:", halted_tag)
        print("RCPT_Q:     ", rcpt_q_tag)
        return 0

    assert queued_tag == EXPECTED_QUEUED_ROOT_TAG, f"{queued_tag} != {EXPECTED_QUEUED_ROOT_TAG}"
    assert act_tag == EXPECTED_ACT_ROOT_TAG, f"{act_tag} != {EXPECTED_ACT_ROOT_TAG}"
    assert paused_tag == EXPECTED_PAUSED_ROOT_TAG, f"{paused_tag} != {EXPECTED_PAUSED_ROOT_TAG}"
    assert halted_tag == EXPECTED_HALTED_ROOT_TAG, f"{halted_tag} != {EXPECTED_HALTED_ROOT_TAG}"
    assert rcpt_q_tag == EXPECTED_RCPT_Q_TAG, f"{rcpt_q_tag} != {EXPECTED_RCPT_Q_TAG}"

    print(
        "governance_1_vectors PASS: 5 golden tags verified across 2 schemas "
        "(GovernanceStateRootV1, GovernanceTransitionReceiptV1)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
