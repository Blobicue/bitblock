#!/usr/bin/env python3
"""Independent stdlib vectors for BOUNDS-1 protocol, local-VM (CE-05), and safe-halt codecs.

Imports no Rust/Bitblock code. Reconstructs all three Wave 5 canonical schemas:
  * bitblock/bounds-1/protocol-profile/v1
  * bitblock/bounds-1/local-vm-bounds/v1
  * bitblock/bounds-1/safe-halt-evidence/v1
"""

from __future__ import annotations

import struct
import sys

from crypto_d1_vectors import content_tag
from profile_bootstrap_vectors import decode_tag_digest

PROTOCOL_BOUNDS_SCHEMA = b"bitblock/bounds-1/protocol-profile/v1"
LOCAL_VM_BOUNDS_SCHEMA = b"bitblock/bounds-1/local-vm-bounds/v1"
SAFE_HALT_EVIDENCE_SCHEMA = b"bitblock/bounds-1/safe-halt-evidence/v1"

EXPECTED_THREAT_MODEL_TAG = "#bb.policy.sha256.qnfdid2kwcnzhyxinyitgiu62cimnl5a37ugwjye7wolmu7qurmq"
EXPECTED_VM_SPEC_TAG = "#bb.policy.sha256.nvshpvblkenho2me3rp5qwwz4mjrtsssrzz6k7uippu2plpw2ava"
LOCAL_BOUNDS_SCHEMA = b"bitblock/local-bounds-profile/v1"


def default_local_bounds_digest() -> bytes:
    payload = struct.pack(
        ">QQQQQQQ",
        16 * 1024 * 1024,
        65_536,
        64 * 1024 * 1024,
        262_144,
        512 * 1024 * 1024,
        1024 * 1024 * 1024,
        4_096,
    )
    _, digest = content_tag("policy", LOCAL_BOUNDS_SCHEMA, payload)
    return digest


def encode_protocol_bounds(threat_digest: bytes, local_digest: bytes) -> bytes:
    out = (
        struct.pack(">H", 1)
        + threat_digest
        + local_digest
        + struct.pack(
            ">IIIIIIIHIIIIH",
            2 * 1024 * 1024,   # max_block_bytes (2 MiB)
            16 * 1024,         # max_tx_bytes (16 KiB)
            1_024,             # max_block_txs
            2_048,             # max_block_signatures
            16_384,            # max_block_ops
            4_096,             # max_state_writes_per_block
            12 * 1024,         # max_proof_bytes (12 KiB)
            256,               # max_proof_depth
            1_048_576,         # max_hashes_per_block
            32 * 1024 * 1024,  # max_alloc_bytes_per_block (32 MiB)
            400,               # max_verify_budget_ms
            4 * 1024 * 1024,   # max_peer_inflight_bytes (4 MiB)
            64,                # max_peer_requests_per_sec
        )
    )
    assert len(out) == 114
    return out


def encode_local_vm_bounds(vm_spec_digest: bytes) -> bytes:
    out = (
        struct.pack(">H", 1)
        + vm_spec_digest
        + struct.pack(">HHQQHH", 12, 64, 2_048, 65_536, 64, 1_024)
    )
    assert len(out) == 58
    return out


def encode_safe_halt_evidence(bounds_digest: bytes, artifact_digest: bytes, code: int, obs: int, lim: int) -> bytes:
    out = (
        struct.pack(">H", 1)
        + bounds_digest
        + artifact_digest
        + struct.pack(">BQQ", code, obs, lim)
    )
    assert len(out) == 83
    return out


def main() -> int:
    threat_digest = decode_tag_digest(EXPECTED_THREAT_MODEL_TAG)
    local_digest = default_local_bounds_digest()
    vm_spec_digest = decode_tag_digest(EXPECTED_VM_SPEC_TAG)

    pb_tag, pb_digest = content_tag(
        "policy", PROTOCOL_BOUNDS_SCHEMA, encode_protocol_bounds(threat_digest, local_digest)
    )
    vm_tag, _ = content_tag(
        "policy", LOCAL_VM_BOUNDS_SCHEMA, encode_local_vm_bounds(vm_spec_digest)
    )
    halt_tag, _ = content_tag(
        "receipt",
        SAFE_HALT_EVIDENCE_SCHEMA,
        encode_safe_halt_evidence(pb_digest, bytes([0xAB]) * 32, 1, 2 * 1024 * 1024 + 1, 2 * 1024 * 1024),
    )

    # Negative control: 1-byte widening flips tag
    widened = bytearray(encode_protocol_bounds(threat_digest, local_digest))
    widened[66:70] = struct.pack(">I", 2 * 1024 * 1024 + 1)
    w_tag, _ = content_tag("policy", PROTOCOL_BOUNDS_SCHEMA, bytes(widened))
    assert w_tag != pb_tag

    assert pb_tag == "#bb.policy.sha256.yecxf5k5a5gs3y7qm76pzax2f5e6bkidmxytmsthsxhdqozzplaa", pb_tag
    assert vm_tag == "#bb.policy.sha256.o2xw5sig3l5n43lfwyxh5cpsa5kaydtgtrqerw4liahezlrn7j6a", vm_tag
    assert halt_tag == "#bb.receipt.sha256.blh4qbetfmqq2yu7pq32phgsyu66mqakmf7ffjfmkorzvxkbfvta", halt_tag
    print(f"bounds_1_vectors: ok (protocol_bounds={pb_tag}, local_vm={vm_tag}, 3/3 golden tags pinned)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
