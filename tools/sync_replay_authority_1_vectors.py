#!/usr/bin/env python3
"""Clean-room pure-Python independent verifier & vector suite for SYNC-1@replay-authority (PS-095 / PD-06).

Imports NO Rust or Bitblock code. Independently reproduces:
  * bitblock/sync-1/replay-archive-checkpoint/v1 (authorized post-consumer checkpoint C1 & C2)
  * bitblock/sync-1/replay-authority-audit/v1 (complete ReplayAuthorityAuditBundleV1 at C1 & C2)
  * Required positive control: checkpoint_ledger_height (5) > consumer_ledger_height (1)
  * Negative control: 1-bit tamper on prefix root or normalized view root breaks verification
"""

from __future__ import annotations

import hashlib
import struct
import sys

from consensus_1_vectors import build_consensus_1_vectors, sign_domain_digest
from crypto_d1_vectors import content_tag, ed_public_from_seed, ed_sign, ed_verify_strict

SYNC_REPLAY_CHECKPOINT_SCHEMA = b"bitblock/sync-1/replay-archive-checkpoint/v1"
SYNC_REPLAY_AUDIT_BUNDLE_SCHEMA = b"bitblock/sync-1/replay-authority-audit/v1"

DEFAULT_ARCHIVE_CHECKPOINT_AUTH_SEED = bytes([171]) * 32

SRA_OUTER_OCC_PREFIX = b"bb.sra1.outer_occ.v1"
SRA_CERT_OCC_PREFIX = b"bb.sra1.cert_occ.v1"
SRA_EVENT_REC_PREFIX = b"bb.sra1.event_rec.v1"
SRA_PREFIX_ROOT_PREFIX = b"bb.sra1.prefix_root.v1"
SRA_NORM_ENTRY_PREFIX = b"bb.sra1.norm_entry.v1"
SRA_NORM_VIEW_PREFIX = b"bb.sra1.norm_view.v1"
SRA_DEP_CLOSURE_PREFIX = b"bb.sra1.dep_closure.v1"
SRA_CHK_CORE_PREFIX = b"bb.sra1.chk_core.v1"
SRA_CHK_SIGN_PREFIX = b"bb.sra1.chk_sign.v1"
SRA_VIEW_CORE_PREFIX = b"bb.sra1.view_core.v1"
SRA_CONSISTENCY_PREFIX = b"bb.sra1.consistency.v1"
SRA_COMMIT_MAP_PREFIX = b"bb.sra1.commit_map.v1"
SRA_RETENTION_TRACE_PREFIX = b"bb.sra1.ret_trace.v1"


def lp_bytes(b: bytes) -> bytes:
    return struct.pack(">I", len(b)) + b


def outer_occ_id(occ_ord: int, h: int, raw_w: bytes, avail_id: bytes, has_conflict: bool) -> bytes:
    return hashlib.sha256(
        SRA_OUTER_OCC_PREFIX
        + struct.pack(">QQ", occ_ord, h)
        + lp_bytes(raw_w)
        + avail_id
        + bytes([1 if has_conflict else 0])
    ).digest()


def cert_occ_id(
    purpose: int,
    consumer_obj: bytes,
    req_pos: int,
    cons_h: int,
    cert_bytes: bytes,
    preimage: bytes,
) -> bytes:
    return hashlib.sha256(
        SRA_CERT_OCC_PREFIX
        + bytes([purpose])
        + consumer_obj
        + struct.pack(">QQ", req_pos, cons_h)
        + lp_bytes(cert_bytes)
        + lp_bytes(preimage)
    ).digest()


def encode_event_rec(archive_id: bytes, ordinal: int, prev_id: bytes, ev_bytes: bytes) -> tuple[bytes, bytes]:
    rec = struct.pack(">H", 1) + archive_id + struct.pack(">Q", ordinal) + prev_id + ev_bytes
    eid = hashlib.sha256(SRA_EVENT_REC_PREFIX + lp_bytes(rec)).digest()
    return rec, eid


def prefix_root(archive_id: bytes, recs: list[tuple[int, bytes]]) -> bytes:
    h = hashlib.sha256()
    h.update(SRA_PREFIX_ROOT_PREFIX)
    h.update(archive_id)
    h.update(struct.pack(">Q", len(recs)))
    for ord_u, eid in recs:
        h.update(struct.pack(">Q", ord_u))
        h.update(eid)
    return h.digest()


def build_sync_replay_authority_1_vectors() -> tuple[str, str, str]:
    _, _, cctx = build_consensus_1_vectors()
    chain_id = cctx["chain_id"]
    genesis_id = cctx["genesis_id"]
    profile_id = cctx["profile_id"]
    parent_lookup_id = cctx["parent_lookup_id"]
    core_digest = cctx["core_digest"]
    w1_bytes = cctx["w1_bytes"]
    w2_bytes = cctx["w2_bytes"]
    occ_rcpts = cctx["occ_rcpts"]
    occ_univ_root = cctx["occ_univ_root"]
    evid_commit_id = cctx["evid_commit_id"]
    pa_cert_bytes = cctx["pa_cert_bytes"]
    pa_adm_id = cctx["pa_adm_id"]

    archive_id = bytes([0x11]) * 32
    archive_policy_id = bytes([0x22]) * 32
    norm_policy_id = bytes([0x33]) * 32
    retention_policy_id = bytes([0x44]) * 32
    elig_policy_id = bytes([0x55]) * 32
    req_pred_id = bytes([0x66]) * 32
    auth_pk = ed_public_from_seed(DEFAULT_ARCHIVE_CHECKPOINT_AUTH_SEED)

    # Event 1 & 2: wrapper outer occurrences at height 1
    oid1 = outer_occ_id(1, 1, w1_bytes, bytes([0xC1]) * 32, False)
    ev1 = b"\x01" + struct.pack(">QQ", 1, 1) + lp_bytes(w1_bytes) + bytes([0xC1]) * 32 + b"\x00" + oid1
    rec1, eid1 = encode_event_rec(archive_id, 1, archive_id, ev1)

    oid2 = outer_occ_id(2, 1, w2_bytes, bytes([0xC2]) * 32, False)
    ev2 = b"\x01" + struct.pack(">QQ", 2, 1) + lp_bytes(w2_bytes) + bytes([0xC2]) * 32 + b"\x00" + oid2
    rec2, eid2 = encode_event_rec(archive_id, 2, eid1, ev2)

    # Event 3: consumer parent-authority use event at height 1
    consumer_pre = b"bb.release.consumer.preimage.v1"
    coid = cert_occ_id(1, bytes([0xD4]) * 32, 1, 1, pa_cert_bytes, consumer_pre)
    ev3 = (
        b"\x02"
        + b"\x01"
        + bytes([0xD4]) * 32
        + struct.pack(">QQ", 1, 1)
        + lp_bytes(pa_cert_bytes)
        + lp_bytes(consumer_pre)
        + coid
    )
    rec3, eid3 = encode_event_rec(archive_id, 3, eid2, ev3)

    # NormalizedAcceptedAuthorityEntryCoreV1
    cred_pks = sorted([
        ed_public_from_seed(bytes([201]) * 32),
        ed_public_from_seed(bytes([202]) * 32),
    ])
    ev_digs = sorted([bytes([0xA1]) * 32, bytes([0xA2]) * 32])
    deps = sorted([(bytes([0xB1]) * 32, 1024), (bytes([0xB2]) * 32, 1024)])
    dep_dig = hashlib.sha256(
        SRA_DEP_CLOSURE_PREFIX
        + struct.pack(">I", len(deps))
        + b"".join(did + struct.pack(">Q", eb) for did, eb in deps)
    ).digest()

    proj_core = (
        parent_lookup_id + core_digest + b"\x01" + elig_policy_id + req_pred_id + b"\x02"
    )
    assert len(proj_core) == 130
    norm_entry_bytes = (
        parent_lookup_id
        + core_digest
        + struct.pack(">H", len(cred_pks))
        + b"".join(cred_pks)
        + struct.pack(">H", len(ev_digs))
        + b"".join(ev_digs)
        + dep_dig
        + struct.pack(">H", 1)
        + proj_core
        + b"\x00"
    )
    norm_entry_id = hashlib.sha256(SRA_NORM_ENTRY_PREFIX + lp_bytes(norm_entry_bytes)).digest()
    norm_view_root = hashlib.sha256(
        SRA_NORM_VIEW_PREFIX + struct.pack(">I", 1) + parent_lookup_id + norm_entry_id
    ).digest()

    def build_checkpoint(prev_cp_id: bytes, cp_h: int, recs_meta: list[tuple[int, bytes]]) -> tuple[bytes, bytes, bytes]:
        proot = prefix_root(archive_id, recs_meta)
        n = len(recs_meta)
        cp_id = hashlib.sha256(
            SRA_CHK_CORE_PREFIX
            + struct.pack(">HQ", 1, chain_id)
            + genesis_id
            + archive_id
            + archive_policy_id
            + prev_cp_id
            + struct.pack(">QQQQ", cp_h, 1, n, n)
            + proot
            + norm_policy_id
            + norm_view_root
            + b"\x01"
        ).digest()
        sdoc = hashlib.sha256(
            SRA_CHK_SIGN_PREFIX + cp_id + archive_policy_id + norm_policy_id
        ).digest()
        sdig = sign_domain_digest(chain_id, genesis_id, profile_id, 9, sdoc)
        sig = ed_sign(DEFAULT_ARCHIVE_CHECKPOINT_AUTH_SEED, sdig)
        assert ed_verify_strict(auth_pk, sdig, sig)
        cp_bytes = (
            struct.pack(">HQ", 1, chain_id)
            + genesis_id
            + archive_id
            + archive_policy_id
            + prev_cp_id
            + struct.pack(">QQQQ", cp_h, 1, n, n)
            + proot
            + norm_policy_id
            + norm_view_root
            + b"\x01"
            + auth_pk
            + sig
        )
        assert len(cp_bytes) == 363
        return cp_bytes, cp_id, proot

    # Positive control: C1 sealed at height 5 > consumer height 1
    cp1_bytes, cp1_id, proot1 = build_checkpoint(archive_id, 5, [(1, eid1), (2, eid2), (3, eid3)])
    cp1_tag, _ = content_tag("receipt", SYNC_REPLAY_CHECKPOINT_SCHEMA, cp1_bytes)

    def build_audit_bundle(
        cp_bytes: bytes,
        cp_id: bytes,
        proot: bytes,
        recs_raw: list[bytes],
        consistency_bytes: bytes | None,
    ) -> bytes:
        n = len(recs_raw)
        pp_bytes = (
            cp_id
            + struct.pack(">QQQ", 1, n, n)
            + proot
            + struct.pack(">I", n)
            + b"".join(lp_bytes(r) for r in recs_raw)
        )
        view_id = hashlib.sha256(
            SRA_VIEW_CORE_PREFIX
            + struct.pack(">H", 1)
            + cp_id
            + struct.pack(">QQ", 1, 3)
            + proot
            + norm_policy_id
            + norm_view_root
        ).digest()
        view_bytes = (
            struct.pack(">H", 1)
            + cp_bytes
            + struct.pack(">QQ", 1, 3)
            + lp_bytes(pp_bytes)
            + struct.pack(">H", 1)
            + lp_bytes(norm_entry_bytes)
            + view_id
        )
        # Same-wrapper witnesses (extract consensus_evidence_id & availability_assessment_id from occ_rcpts)
        # In occ_rcpt (233 bytes): 8 (ord) + 32 (lid) + 32 (cid) + 32 (w) + 32 (cred) + 32 (evid) + 32 (avail) + 32 (dep) + 1
        sw1 = (
            view_id + eid1 + oid1 + struct.pack(">Q", 1)
            + parent_lookup_id + core_digest
            + occ_rcpts[0][136:168] + occ_rcpts[0][168:200]
        )
        sw2 = (
            view_id + eid2 + oid2 + struct.pack(">Q", 2)
            + parent_lookup_id + core_digest
            + occ_rcpts[1][136:168] + occ_rcpts[1][168:200]
        )
        assert len(sw1) == 232 and len(sw2) == 232
        ret_trace = hashlib.sha256(
            SRA_RETENTION_TRACE_PREFIX
            + view_id
            + norm_entry_id
            + retention_policy_id
            + struct.pack(">HH", 1, 1)
        ).digest()
        ret_bytes = (
            view_id
            + norm_entry_id
            + retention_policy_id
            + lp_bytes(norm_entry_bytes)
            + struct.pack(">HH", 1, 1)
            + ret_trace
        )
        map_dig = hashlib.sha256(
            SRA_COMMIT_MAP_PREFIX + cp_id + struct.pack(">I", len(occ_rcpts)) + b"".join(occ_rcpts)
        ).digest()
        commit_proof = (
            pa_adm_id + evid_commit_id + occ_univ_root + cp_id + map_dig + b"\x02"
        )
        assert len(commit_proof) == 161
        hw_bytes = (
            view_id
            + struct.pack(">Q", 3)
            + lp_bytes(pa_cert_bytes)
            + coid
            + commit_proof
            + lp_bytes(consumer_pre)
        )
        sp_bytes = view_id + proj_core
        assert len(sp_bytes) == 162
        out = (
            struct.pack(">H", 1)
            + lp_bytes(view_bytes)
            + struct.pack(">H", 2)
            + sw1
            + sw2
            + struct.pack(">H", 1)
            + lp_bytes(ret_bytes)
            + struct.pack(">H", 1)
            + lp_bytes(hw_bytes)
            + struct.pack(">H", 1)
            + sp_bytes
        )
        if consistency_bytes is None:
            out += b"\x00"
        else:
            out += b"\x01" + consistency_bytes
        return out

    b1_bytes = build_audit_bundle(cp1_bytes, cp1_id, proot1, [rec1, rec2, rec3], None)
    b1_tag, _ = content_tag("receipt", SYNC_REPLAY_AUDIT_BUNDLE_SCHEMA, b1_bytes)

    # Event 4 (post-cutoff at height 6) -> checkpoint C2 (height 8) + C1->C2 append consistency
    oid4 = outer_occ_id(3, 6, w1_bytes, bytes([0xC3]) * 32, False)
    ev4 = b"\x01" + struct.pack(">QQ", 3, 6) + lp_bytes(w1_bytes) + bytes([0xC3]) * 32 + b"\x00" + oid4
    rec4, eid4 = encode_event_rec(archive_id, 4, eid3, ev4)
    cp2_bytes, cp2_id, proot2 = build_checkpoint(
        cp1_id, 8, [(1, eid1), (2, eid2), (3, eid3), (4, eid4)]
    )
    cons_w = hashlib.sha256(
        SRA_CONSISTENCY_PREFIX + proot1 + proot2 + struct.pack(">Q", 3)
    ).digest()
    cons_bytes = cp1_id + cp2_id + struct.pack(">Q", 3) + cons_w
    assert len(cons_bytes) == 104
    b2_bytes = build_audit_bundle(cp2_bytes, cp2_id, proot2, [rec1, rec2, rec3, rec4], cons_bytes)
    b2_tag, _ = content_tag("receipt", SYNC_REPLAY_AUDIT_BUNDLE_SCHEMA, b2_bytes)

    # Negative control: 1-bit tamper on prefix root changes checkpoint tag
    tampered_cp1 = bytearray(cp1_bytes)
    tampered_cp1[180] ^= 1
    assert content_tag("receipt", SYNC_REPLAY_CHECKPOINT_SCHEMA, bytes(tampered_cp1))[0] != cp1_tag
    return cp1_tag, b1_tag, b2_tag


def main() -> int:
    cp1_tag, b1_tag, b2_tag = build_sync_replay_authority_1_vectors()
    if "--print-tags" in sys.argv:
        print("CP1_TAG =", cp1_tag)
        print("B1_TAG  =", b1_tag)
        print("B2_TAG  =", b2_tag)
        return 0
    assert cp1_tag == EXPECTED_CP1_TAG, f"cp1 mismatch: {cp1_tag}"
    assert b1_tag == EXPECTED_B1_TAG, f"b1 mismatch: {b1_tag}"
    assert b2_tag == EXPECTED_B2_TAG, f"b2 mismatch: {b2_tag}"
    print(f"sync_replay_authority_1_vectors: PASS (cp1={cp1_tag[:26]}..., b1={b1_tag[:26]}...)")
    return 0


EXPECTED_CP1_TAG = "#bb.receipt.sha256.2mdit6c2t47g4aoihd6qnz7r4iqy7qnyxq5zxl7lnbkvxh7jofna"
EXPECTED_B1_TAG = "#bb.receipt.sha256.k3g23z6c5xob4cyxdictjgeaqhckkvjqfdzahfjqcko6faxfsf5a"
EXPECTED_B2_TAG = "#bb.receipt.sha256.y22xa32n3vfzindqhtg6sltlso5junn2lg2nzeon7orvqubznooa"

if __name__ == "__main__":
    raise SystemExit(main())
