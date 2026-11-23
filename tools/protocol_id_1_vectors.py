#!/usr/bin/env python3
"""Independent pure-Python reference implementation and vector suite for PROTOCOL-ID-1."""

from __future__ import annotations

import struct
import sys

from crypto_d1_vectors import (
    SIGNING_PREIMAGE_SCHEMA,
    content_tag,
    ed_public_from_seed,
    ed_sign,
    encode_signing_preimage,
)

EXACT_CORE_SCHEMA = b"bitblock/protocol-id-1/exact-proposal-core/v1"
WRAPPER_SCHEMA = b"bitblock/protocol-id-1/proposal-wrapper/v1"
COLLISION_AUDIT_SCHEMA = b"bitblock/protocol-id-1/parent-collision-audit/v1"
PARENT_LOOKUP_SCHEMA = b"bitblock/protocol-id-1/parent-lookup-preimage/v1"


def parent_lookup_id(preimage: bytes) -> bytes:
    _, d = content_tag("receipt", PARENT_LOOKUP_SCHEMA, preimage)
    return d


def encode_exact_core(
    lookup_id: bytes,
    authority_ref_kind: int,
    auth_ref: bytes,
    header: bytes,
    avail: bytes,
) -> bytes:
    out = bytearray()
    out += struct.pack(">H", 1)
    out += lookup_id
    out += struct.pack(">BIQQ", authority_ref_kind, len(auth_ref), len(header), len(avail))
    out += auth_ref + header + avail
    return bytes(out)


def encode_wrapper(
    core_bytes: bytes,
    credentials: list[tuple[bytes, bytes]],
    evidence: list[bytes],
    avail_deps: list[tuple[bytes, int]],
) -> bytes:
    out = bytearray()
    out += struct.pack(">I", len(core_bytes)) + core_bytes
    out += struct.pack(">H", len(credentials))
    for pk, sig in credentials:
        out += pk + sig
    out += struct.pack(">H", len(evidence))
    for ev in evidence:
        out += ev
    out += struct.pack(">H", len(avail_deps))
    for did, sz in avail_deps:
        out += did + struct.pack(">Q", sz)
    return bytes(out)


def encode_collision_audit(
    row_code: int, lookup_id: bytes, core_a_digest: bytes, core_b_digest: bytes
) -> bytes:
    out = struct.pack(">HB", 1, row_code) + lookup_id + core_a_digest + core_b_digest + b"\x01"
    assert len(out) == 100
    return out


def build_golden_vectors() -> tuple[str, str, str, str]:
    preimage = b"bitblock.devnet.v1:genesis-parent-lookup"
    lid = parent_lookup_id(preimage)
    core_a = encode_exact_core(
        lid, 1, b"launch-genesis-ref-v1", b"block-1-header-canonical", b"avail-blob-v1"
    )
    core_a_tag, core_a_d = content_tag("receipt", EXACT_CORE_SCHEMA, core_a)

    core_b = encode_exact_core(
        lid, 1, b"launch-genesis-ref-v1", b"block-1-header-equivocated", b"avail-blob-v1"
    )
    _, core_b_d = content_tag("receipt", EXACT_CORE_SCHEMA, core_b)

    chain_id = 1
    gen_id = bytes([0x11] * 32)
    lm_id = bytes([0x22] * 32)
    prof_id = bytes([0x33] * 32)
    pre_bytes = encode_signing_preimage(
        1, chain_id, gen_id, lm_id, prof_id, 3, 1, 1, core_a_d
    )
    _, pre_digest = content_tag("receipt", SIGNING_PREIMAGE_SCHEMA, pre_bytes)
    seed1, seed2 = bytes([41] * 32), bytes([42] * 32)
    creds = sorted(
        [(ed_public_from_seed(seed1), ed_sign(seed1, pre_digest)),
         (ed_public_from_seed(seed2), ed_sign(seed2, pre_digest))],
        key=lambda x: x[0],
    )
    wrapper_bytes = encode_wrapper(
        core_a,
        creds,
        [bytes([0xAA] * 32), bytes([0xBB] * 32)],
        [(bytes([0xCC] * 32), 4096)],
    )
    wrapper_tag, _ = content_tag("receipt", WRAPPER_SCHEMA, wrapper_bytes)

    audit_r2 = encode_collision_audit(2, lid, core_a_d, core_b_d)
    audit_r2_tag, _ = content_tag("receipt", COLLISION_AUDIT_SCHEMA, audit_r2)

    audit_r1 = encode_collision_audit(1, lid, core_a_d, core_b_d)
    audit_r1_tag, _ = content_tag("receipt", COLLISION_AUDIT_SCHEMA, audit_r1)
    return core_a_tag, wrapper_tag, audit_r2_tag, audit_r1_tag


def main() -> int:
    core_tag, wrapper_tag, audit_r2_tag, audit_r1_tag = build_golden_vectors()
    assert core_tag == "#bb.receipt.sha256.zaewefk4edydxkwnzxngps7h4szty4oohcrdtdpxwjrzsjxzi53q", core_tag
    assert wrapper_tag == "#bb.receipt.sha256.aumbzbgscvkmaofwnpaqbujimukmiodgv2pbv7w26mexpr5qepxq", wrapper_tag
    assert audit_r2_tag == "#bb.receipt.sha256.awmkppq2clfbrf7qouyfxdeuseo7kktzcrnriii3pm4tkckk6wha", audit_r2_tag
    assert audit_r1_tag == "#bb.receipt.sha256.k3k4i5i3xkghpncwnwssp4jklfbsvpnqfwwzynbmqwk6hbdotraq", audit_r1_tag
    if "--print-tags" in sys.argv:
        print(f"EXACT_CORE_TAG={core_tag}")
        print(f"WRAPPER_TAG={wrapper_tag}")
        print(f"AUDIT_ROW2_TAG={audit_r2_tag}")
        print(f"AUDIT_ROW1_TAG={audit_r1_tag}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
