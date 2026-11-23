#!/usr/bin/env python3
"""Independent stdlib Python vectors for IDENTITY-1 (PS-028).

Imports no Rust/Bitblock code. Independently reconstructs both Wave 12A canonical schemas:
  * bitblock/identity-1/actor-auth-state/v1
  * bitblock/identity-1/authorization-receipt/v1
and pins byte-exact ContentTag agreement with `crates/research/src/identity_1.rs` and
`tests/wave12a_identity_1.rs`.
"""

from __future__ import annotations

import hashlib
import struct
import sys

from crypto_d1_vectors import content_tag, ed_public_from_seed
from semantic_batch_vectors import world_tag

AUTH_STATE_SCHEMA = b"bitblock/identity-1/actor-auth-state/v1"
AUTH_RECEIPT_SCHEMA = b"bitblock/identity-1/authorization-receipt/v1"
AUTH_DESC_SCHEMA = b"bitblock/identity-d1/authorization-descriptor/v1"
TX_BODY_SCHEMA = b"bitblock/identity-d1/transaction-body/v1"

EXPECTED_AUTH_STATE_A_TAG = (
    "#bb.receipt.sha256.hthq4z3wjktuwvlwzg4uilsby2w4g2qia5tky55bk6meccqm26eq"
)
EXPECTED_DIRECT_RECEIPT_TAG = (
    "#bb.receipt.sha256.63wc5rwka53hfx4nqtais7zz4lsxyfimvkt3mola6mqjqey4indq"
)
EXPECTED_DELEGATED_RECEIPT_TAG = (
    "#bb.receipt.sha256.q46n6n3ms6vjtbhy3marabz7d4jjhtvq6mjhuwyk7azceajbspmq"
)
EXPECTED_RECOVERY_RECEIPT_TAG = (
    "#bb.receipt.sha256.yo2g77yhasl2edcaxur5teoh4je2qz4chztcwsnmhqd7txduplha"
)


def pk(seed_byte: int) -> bytes:
    return ed_public_from_seed(bytes([seed_byte]) * 32)


def encode_threshold_policy(role_code: int, threshold: int, keys: list[bytes]) -> bytes:
    ordered = sorted(keys)
    out = struct.pack(">BHH", role_code, threshold, len(ordered))
    for k in ordered:
        assert len(k) == 32
        out += k
    return out


def encode_auth_descriptor(
    version: int,
    action_policy: bytes,
    admin_policy: bytes,
    delegations: list[tuple[bytes, bool, bool, int]],
    revoked: bool,
) -> bytes:
    out = struct.pack(">Q", version) + action_policy + admin_policy
    out += struct.pack(">H", len(delegations))
    for delegate_id, allow_on, allow_off, expires_h in sorted(delegations, key=lambda x: x[0]):
        assert len(delegate_id) == 32
        flags = (1 if allow_on else 0) | ((1 if allow_off else 0) << 1)
        out += delegate_id + struct.pack(">BQ", flags, expires_h)
    out += struct.pack(">B", 1 if revoked else 0)
    return out


def derive_actor_id(chain_id: int, ns: str, desc_bytes: bytes) -> bytes:
    ns_b = ns.encode("utf-8")
    h = hashlib.sha256()
    h.update(b"bitblock/actor-identity/v1")
    h.update(struct.pack(">Q", chain_id))
    h.update(struct.pack(">I", len(ns_b)))
    h.update(ns_b)
    h.update(struct.pack(">I", len(desc_bytes)))
    h.update(desc_bytes)
    return h.digest()


def encode_tx_body(
    actor_id: bytes,
    nonce: int,
    valid_after: int,
    valid_until: int,
    expected_root: bytes | None,
    max_fee: int,
    ops: list[tuple[int, int, int, int]],
) -> bytes:
    out = actor_id + struct.pack(">QQQ", nonce, valid_after, valid_until)
    if expected_root is None:
        out += b"\x00"
    else:
        assert len(expected_root) == 32
        out += b"\x01" + expected_root
    out += struct.pack(">QQH", max_fee >> 64, max_fee & ((1 << 64) - 1), len(ops))
    for code, x, y, z in ops:
        out += struct.pack(">Bqqq", code, x, y, z)
    return out


def encode_actor_auth_state(
    actor_id: bytes,
    desc_bytes: bytes,
    next_nonce: int,
    guardian_policy: bytes,
    delay_heights: int,
    pending: tuple[int, int, bytes] | None,
) -> bytes:
    out = (
        struct.pack(">H", 1)
        + actor_id
        + struct.pack(">I", len(desc_bytes))
        + desc_bytes
        + struct.pack(">QI", next_nonce, len(guardian_policy))
        + guardian_policy
        + struct.pack(">Q", delay_heights)
    )
    if pending is None:
        out += b"\x00"
    else:
        init_h, unlock_h, prop_bytes = pending
        out += struct.pack(">BQQI", 1, init_h, unlock_h, len(prop_bytes)) + prop_bytes
    return out


def encode_auth_receipt(
    action_kind: int,
    disposition: int,
    principal_id: bytes,
    signer_id: bytes,
    pre_nonce: int,
    post_nonce: int,
    desc_version: int,
    block_height: int,
    payload_digest: bytes,
) -> bytes:
    out = (
        struct.pack(">HBB", 1, action_kind, disposition)
        + principal_id
        + signer_id
        + struct.pack(">QQQQ", pre_nonce, post_nonce, desc_version, block_height)
        + payload_digest
    )
    assert len(out) == 132
    return out


def main() -> int:
    chain_id = 77
    bedrock_world_digest = world_tag([(0, 0, 0)]).digest

    # Actor C -> Actor B -> Actor A (matching canonical fixture)
    desc_c = encode_auth_descriptor(
        0,
        encode_threshold_policy(0, 1, [pk(31)]),
        encode_threshold_policy(1, 1, [pk(32)]),
        [],
        False,
    )
    actor_c = derive_actor_id(chain_id, "actor-c", desc_c)

    desc_b = encode_auth_descriptor(
        0,
        encode_threshold_policy(0, 1, [pk(21)]),
        encode_threshold_policy(1, 1, [pk(22)]),
        [(actor_c, True, True, 200)],
        False,
    )
    actor_b = derive_actor_id(chain_id, "actor-b", desc_b)

    desc_a = encode_auth_descriptor(
        0,
        encode_threshold_policy(0, 1, [pk(11)]),
        encode_threshold_policy(1, 1, [pk(12)]),
        [(actor_b, True, False, 100)],
        False,
    )
    actor_a = derive_actor_id(chain_id, "actor-a", desc_a)

    # 1. ActorAuthStateV1 golden parity
    state_a_bytes = encode_actor_auth_state(
        actor_a,
        desc_a,
        0,
        encode_threshold_policy(1, 1, [pk(13)]),
        100,
        None,
    )
    state_a_tag, _ = content_tag("receipt", AUTH_STATE_SCHEMA, state_a_bytes)
    assert state_a_tag == EXPECTED_AUTH_STATE_A_TAG, (
        f"ActorAuthStateV1 mismatch: {state_a_tag} != {EXPECTED_AUTH_STATE_A_TAG}"
    )

    # Control: changing next_nonce or delay moves tag
    state_a_nonce1, _ = content_tag(
        "receipt",
        AUTH_STATE_SCHEMA,
        encode_actor_auth_state(
            actor_a, desc_a, 1, encode_threshold_policy(1, 1, [pk(13)]), 100, None
        ),
    )
    assert state_a_nonce1 != state_a_tag

    # 2. Direct transaction receipt golden parity
    tx0_bytes = encode_tx_body(actor_a, 0, 1, 50, bedrock_world_digest, 100, [(0, 1, 0, 0)])
    _, tx0_digest = content_tag("intent", TX_BODY_SCHEMA, tx0_bytes)
    rcpt0_bytes = encode_auth_receipt(0, 0, actor_a, actor_a, 0, 1, 0, 10, tx0_digest)
    rcpt0_tag, _ = content_tag("receipt", AUTH_RECEIPT_SCHEMA, rcpt0_bytes)
    assert rcpt0_tag == EXPECTED_DIRECT_RECEIPT_TAG, (
        f"Direct receipt mismatch: {rcpt0_tag} != {EXPECTED_DIRECT_RECEIPT_TAG}"
    )

    # 3. Depth-1 delegated transaction receipt golden parity
    tx1_bytes = encode_tx_body(actor_a, 1, 1, 120, bedrock_world_digest, 100, [(0, 2, 0, 0)])
    _, tx1_digest = content_tag("intent", TX_BODY_SCHEMA, tx1_bytes)
    rcpt1_bytes = encode_auth_receipt(1, 0, actor_a, actor_b, 1, 2, 0, 20, tx1_digest)
    rcpt1_tag, _ = content_tag("receipt", AUTH_RECEIPT_SCHEMA, rcpt1_bytes)
    assert rcpt1_tag == EXPECTED_DELEGATED_RECEIPT_TAG, (
        f"Delegated receipt mismatch: {rcpt1_tag} != {EXPECTED_DELEGATED_RECEIPT_TAG}"
    )

    # 4. Recovery finalize receipt golden parity
    rec_desc_bytes = encode_auth_descriptor(
        1,
        encode_threshold_policy(0, 1, [pk(41)]),
        encode_threshold_policy(1, 1, [pk(42)]),
        [],
        False,
    )
    _, rec_digest = content_tag("policy", AUTH_DESC_SCHEMA, rec_desc_bytes)
    rcpt_rec_bytes = encode_auth_receipt(6, 0, actor_a, actor_a, 2, 2, 1, 250, rec_digest)
    rcpt_rec_tag, _ = content_tag("receipt", AUTH_RECEIPT_SCHEMA, rcpt_rec_bytes)
    assert rcpt_rec_tag == EXPECTED_RECOVERY_RECEIPT_TAG, (
        f"Recovery receipt mismatch: {rcpt_rec_tag} != {EXPECTED_RECOVERY_RECEIPT_TAG}"
    )

    # Control: disposition or signer change moves receipt tag
    rcpt_rej_tag, _ = content_tag(
        "receipt",
        AUTH_RECEIPT_SCHEMA,
        encode_auth_receipt(1, 11, actor_a, actor_c, 1, 1, 0, 20, tx1_digest),
    )
    assert rcpt_rej_tag != rcpt1_tag

    print(
        "identity_1_vectors PASS: 4 golden tags verified across 2 schemas "
        "(ActorAuthStateV1, IdentityAuthReceiptV1)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
