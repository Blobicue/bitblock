#!/usr/bin/env python3
"""Independent stdlib Python vectors for EXECUTION-ROOT-1 (PS-030).

Imports NO Rust or Bitblock code. Reconstructs the exact L1 §5 two-layer 11-slot
execution-state root composer (`application_root` -> `execution_state_root` ->
external-context `ExecutionRootRefV1`) and both Wave 17A canonical schemas:
  * bitblock/execution-root-1/domain-registry/v1
  * bitblock/execution-root-1/root-composition/v1
"""

from __future__ import annotations

import hashlib
import struct
import sys

from crypto_d1_vectors import content_tag

REGISTRY_SCHEMA = b"bitblock/execution-root-1/domain-registry/v1"
COMPOSITION_SCHEMA = b"bitblock/execution-root-1/root-composition/v1"

REGISTRY_DIGEST_PREFIX = b"bb.exec_root.domain_registry.v1"
REQUIRED_SET_PREFIX = b"bb.exec_root.required_set.v1"
ROOT_ENTRY_SET_PREFIX = b"bb.exec_root.entry_set.v1"
APP_ROOT_PREFIX = b"bb.exec_root.app.v1"
EXEC_ROOT_PREFIX = b"bb.exec_root.exec.v1"
EXEC_ROOT_REF_PREFIX = b"bb.exec_root.exec_ref.v1"

LAYER_APP = 1
LAYER_PROTO = 2

SENTINEL_UNACTIVATED_LAUNCH_PROGRAM = 1
SENTINEL_SCHEMA = b"bb.exec_root.sentinel.unactivated_launch_program.v1"

DOMAIN_SCHEMAS = [
    (1, LAYER_APP, True, b"bb.exec_root.slot.world_v1"),
    (2, LAYER_APP, True, b"bb.exec_root.slot.active_token_v1"),
    (3, LAYER_APP, True, b"bb.exec_root.slot.episode_v1"),
    (4, LAYER_APP, True, b"bb.exec_root.slot.native_balance_v1"),
    (5, LAYER_APP, True, b"bb.exec_root.slot.episode_ownership_v1"),
    (6, LAYER_APP, False, SENTINEL_SCHEMA),
    (7, LAYER_PROTO, True, b"bb.exec_root.slot.authorization_v1"),
    (8, LAYER_PROTO, True, b"bb.exec_root.slot.nonce_v1"),
    (9, LAYER_PROTO, True, b"bb.exec_root.slot.governance_v1"),
    (10, LAYER_PROTO, True, b"bb.exec_root.slot.economic_stake_v1"),
    (11, LAYER_PROTO, True, b"bb.exec_root.slot.validator_schedule_v1"),
]


def sha256(b: bytes) -> bytes:
    return hashlib.sha256(b).digest()


def encode_launch_registry() -> tuple[bytes, bytes, bytes]:
    entries = []
    active_entries = []
    for tag, layer, active, schema in DOMAIN_SCHEMAS:
        dig = sha256(schema)
        if active:
            e = bytes([1, tag, layer, 0]) + dig
            active_entries.append(e)
        else:
            e = bytes([2, tag, layer, SENTINEL_UNACTIVATED_LAUNCH_PROGRAM]) + dig
        assert len(e) == 36
        entries.append(e)
    reg_bytes = struct.pack(">HH", 1, len(entries)) + b"".join(entries)
    reg_digest = sha256(REGISTRY_DIGEST_PREFIX + struct.pack(">I", len(reg_bytes)) + reg_bytes)
    req_set_digest = sha256(
        REQUIRED_SET_PREFIX + struct.pack(">H", len(active_entries)) + b"".join(active_entries)
    )
    return reg_bytes, reg_digest, req_set_digest


def encode_preimage_ctx(
    proto_ver: int, chain_id: int, world_id: int, genesis_id: bytes, scope_digest: bytes
) -> bytes:
    out = struct.pack(">HQQ", proto_ver, chain_id, world_id) + genesis_id + scope_digest
    assert len(out) == 82
    return out


def compose_execution_root(
    batch_height: int,
    epoch: int,
    profile_id: bytes,
    proto_ver: int,
    chain_id: int,
    world_id: int,
    genesis_id: bytes,
    scope_digest: bytes,
    roots_by_tag: dict[int, bytes],
) -> tuple[bytes, bytes, bytes, bytes, str]:
    _, reg_digest, req_digest = encode_launch_registry()
    ctx_bytes = encode_preimage_ctx(proto_ver, chain_id, world_id, genesis_id, scope_digest)
    slots = []
    app_slots = []
    proto_slots = []
    for tag, layer, active, schema in DOMAIN_SCHEMAS:
        dig = sha256(schema)
        if active:
            r = roots_by_tag[tag]
            assert len(r) == 32 and r != bytes(32)
            s = bytes([1, tag, layer, 0]) + dig + r
        else:
            s = bytes([2, tag, layer, SENTINEL_UNACTIVATED_LAUNCH_PROGRAM]) + dig + bytes(32)
        assert len(s) == 68
        slots.append(s)
        if layer == LAYER_APP:
            app_slots.append(s)
        else:
            proto_slots.append(s)

    entry_set_digest = sha256(ROOT_ENTRY_SET_PREFIX + struct.pack(">H", len(slots)) + b"".join(slots))
    app_root = sha256(
        APP_ROOT_PREFIX
        + ctx_bytes
        + reg_digest
        + struct.pack(">H", len(app_slots))
        + b"".join(app_slots)
    )
    exec_root = sha256(
        EXEC_ROOT_PREFIX
        + ctx_bytes
        + reg_digest
        + app_root
        + struct.pack(">H", len(proto_slots))
        + b"".join(proto_slots)
    )
    exec_ref = (
        struct.pack(">HQQ", proto_ver, chain_id, world_id)
        + genesis_id
        + profile_id
        + scope_digest
        + reg_digest
        + exec_root
    )
    assert len(exec_ref) == 178
    comp_bytes = (
        struct.pack(">HQQ", 1, batch_height, epoch)
        + profile_id
        + ctx_bytes
        + reg_digest
        + req_digest
        + entry_set_digest
        + app_root
        + exec_root
        + exec_ref
        + struct.pack(">H", len(slots))
        + b"".join(slots)
    )
    tag, _ = content_tag("receipt", COMPOSITION_SCHEMA, comp_bytes)
    return comp_bytes, app_root, exec_root, exec_ref, tag


def main() -> int:
    reg_bytes, _, _ = encode_launch_registry()
    reg_tag, _ = content_tag("policy", REGISTRY_SCHEMA, reg_bytes)

    roots_a = {
        1: bytes([0x11]) * 32,
        2: bytes([0x22]) * 32,
        3: bytes([0x33]) * 32,
        4: bytes([0x66]) * 32,
        5: bytes([0x77]) * 32,
        7: bytes([0x44]) * 32,
        8: bytes([0x55]) * 32,
        9: bytes([0x88]) * 32,
        10: bytes([0x99]) * 32,
        11: bytes([0xAA]) * 32,
    }
    _, app1, exec1, ref1, comp1_tag = compose_execution_root(
        0, 0, bytes([0xD3]) * 32, 1, 77, 1, bytes([0xB1]) * 32, bytes([0xC2]) * 32, roots_a
    )
    _, app2, exec2, ref2, comp2_tag = compose_execution_root(
        0, 0, bytes([0xE4]) * 32, 1, 77, 1, bytes([0xB1]) * 32, bytes([0xC2]) * 32, roots_a
    )
    # Cycle-free check: external profile_id does not perturb app_root or execution_state_root
    assert app1 == app2 and exec1 == exec2 and ref1 != ref2

    # Swapping two active roots must change execution_state_root
    roots_swap = dict(roots_a)
    roots_swap[7], roots_swap[8] = roots_swap[8], roots_swap[7]
    _, _, exec_swap, _, _ = compose_execution_root(
        0, 0, bytes([0xD3]) * 32, 1, 77, 1, bytes([0xB1]) * 32, bytes([0xC2]) * 32, roots_swap
    )
    assert exec_swap != exec1

    if "--print-tags" in sys.argv:
        print("REG_TAG   =", reg_tag)
        print("COMP1_TAG =", comp1_tag)
        print("COMP2_TAG =", comp2_tag)
        return 0

    assert reg_tag == EXPECTED_REG_TAG, f"reg mismatch: {reg_tag}"
    assert comp1_tag == EXPECTED_COMP1_TAG, f"comp1 mismatch: {comp1_tag}"
    assert comp2_tag == EXPECTED_COMP2_TAG, f"comp2 mismatch: {comp2_tag}"
    print(f"execution_root_1_vectors: PASS (reg={reg_tag[:26]}..., comp1={comp1_tag[:26]}...)")
    return 0


EXPECTED_REG_TAG = (
    "#bb.policy.sha256.3wkwyds7dgkm5hd4vwj26lkokfi6xkeft6d7yl3hzddm3tq6i62q"
)
EXPECTED_COMP1_TAG = (
    "#bb.receipt.sha256.klerr3wgoyagb24baztzztdolokhthmpyo4ohbx6bwxb6nzkd2mq"
)
EXPECTED_COMP2_TAG = (
    "#bb.receipt.sha256.n36urqqdjll77eppiiprbnoifbgzruwzv6ztxkqyn22opx6zdwwq"
)

if __name__ == "__main__":
    raise SystemExit(main())
