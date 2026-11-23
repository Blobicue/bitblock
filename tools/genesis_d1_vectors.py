#!/usr/bin/env python3
"""Independent pure-Python reference implementation and vector suite for GENESIS-D1."""

from __future__ import annotations

import hashlib
import struct
import sys

from crypto_d1_vectors import content_tag, ed_public_from_seed as ed25519_pubkey_from_seed
from profile_bootstrap_vectors import decode_tag_digest
from semantic_batch_vectors import world_tag as canonical_world_tag

BOOTSTRAP_POLICY_SCHEMA = b"bitblock/genesis-d1/bootstrap-availability-policy/v1"
LAUNCH_STATE_SCHEMA = b"bitblock/genesis-d1/launch-state/v1"
PACKAGE_SCHEMA = b"bitblock/genesis-d1/genesis-package/v1"

LAUNCH_PREDICATES = [
    "asset-active",
    "economic-charging",
    "in-place-upgrade",
    "migration-profile",
    "stake-consensus",
    "validator-economics",
    "validator-rewards",
]


def bedrock_world_digest() -> bytes:
    return canonical_world_tag([(0, 0, 0)]).digest


def encode_bootstrap_policy(
    min_seed_peers: int = 8,
    archive_retention_epochs: int = 28,
    max_chunk_bytes: int = 2 * 1024 * 1024,
    checkpoint_interval: int = 1024,
    require_checksum: bool = True,
) -> bytes:
    out = struct.pack(
        ">HHIIIB",
        1,
        min_seed_peers,
        archive_retention_epochs,
        max_chunk_bytes,
        checkpoint_interval,
        1 if require_checksum else 0,
    )
    assert len(out) == 17
    return out


def encode_launch_state(
    scope_digest: bytes,
    domain_reg_digest: bytes,
    chain_id: int,
    world_id: int,
    namespace: str,
    world_digest: bytes,
    resource_digest: bytes,
    asset_digest: bytes,
    econ_digest: bytes,
    gov_digest: bytes,
    bounds_digest: bytes,
    bootstrap_digest: bytes,
    actors: list[tuple[bytes, bytes, int, int]],
    validators: list[tuple[bytes, bytes, bytes, int]],
    total_supply: int,
) -> bytes:
    ns_b = namespace.encode("ascii")
    out = bytearray()
    out += struct.pack(">H", 1)
    out += scope_digest
    out += domain_reg_digest
    out += struct.pack(">QQH", chain_id, world_id, len(ns_b))
    out += ns_b
    out += world_digest
    out += struct.pack(">II", 0, 0)
    out += (0).to_bytes(16, "big")
    out += resource_digest
    out += asset_digest
    out += econ_digest
    out += gov_digest
    out += bounds_digest
    out += bootstrap_digest
    out += struct.pack(">H", len(LAUNCH_PREDICATES))
    for p in LAUNCH_PREDICATES:
        pb = p.encode("ascii")
        out += struct.pack(">H", len(pb)) + pb
    domains = [(1, 1), (2, 1), (3, 1), (4, 1), (5, 1), (6, 1), (7, 0), (8, 0)]
    out += struct.pack(">H", len(domains))
    for code, act in domains:
        out += struct.pack(">BB", code, act)
    out += struct.pack(">I", len(actors))
    for aid, ddig, nonce, bal in actors:
        out += aid + ddig + struct.pack(">Q", nonce) + bal.to_bytes(16, "big")
    out += struct.pack(">I", len(validators))
    for vid, vpk, ppk, stake in validators:
        out += vid + vpk + ppk + stake.to_bytes(16, "big")
    out += total_supply.to_bytes(16, "big")
    return bytes(out)


def encode_genesis_package(state_bytes: bytes, genesis_id: bytes) -> bytes:
    return struct.pack(">I", len(state_bytes)) + state_bytes + genesis_id


def build_golden_devnet_vectors() -> tuple[str, str, str]:
    boot_bytes = encode_bootstrap_policy()
    boot_tag, boot_d = content_tag("policy", BOOTSTRAP_POLICY_SCHEMA, boot_bytes)

    # Upstream digests matching tests/wave8a_genesis_d1.rs canonical fixture
    # (deterministic devnet launch fixture using 4 equal 25% validators + 2 genesis actors)
    scope_d = bytes([0xA1] * 32)
    dom_d = bytes([0xA2] * 32)
    res_d = decode_tag_digest("#bb.policy.sha256.6yvp73mvdkouoxk2xiolnj33lbxucxta5ikhjcwnvd267tzfrfyq")
    asset_d = decode_tag_digest("#bb.policy.sha256.nxy3syo3wcq5ch7ayryyut25sbyustu4snj5vphox52fsnnlo4ca")
    econ_d = decode_tag_digest("#bb.policy.sha256.uzhiakc4bqij66hyg6h75kxsrlxqfaunxd7yzx36kdlxvii2gpaa")
    gov_d = bytes([0xA6] * 32)
    bounds_d = decode_tag_digest("#bb.policy.sha256.yecxf5k5a5gs3y7qm76pzax2f5e6bkidmxytmsthsxhdqozzplaa")

    actors = [
        (bytes([0x10] * 32), hashlib.sha256(b"devnet-actor-1").digest(), 0, 100_000_000_000),
        (bytes([0x20] * 32), hashlib.sha256(b"devnet-actor-2").digest(), 0, 100_000_000_000),
    ]
    validators = [
        (
            bytes([i] * 32),
            ed25519_pubkey_from_seed(bytes([100 + i] * 32)),
            ed25519_pubkey_from_seed(bytes([200 + i] * 32)),
            200_000_000_000,
        )
        for i in range(1, 5)
    ]
    total_supply = 1_000_000_000_000
    state_bytes = encode_launch_state(
        scope_d,
        dom_d,
        1,
        1,
        "bitblock.devnet.v1",
        bedrock_world_digest(),
        res_d,
        asset_d,
        econ_d,
        gov_d,
        bounds_d,
        boot_d,
        actors,
        validators,
        total_supply,
    )
    state_tag, genesis_id = content_tag("policy", LAUNCH_STATE_SCHEMA, state_bytes)
    pkg_bytes = encode_genesis_package(state_bytes, genesis_id)
    pkg_tag, _ = content_tag("receipt", PACKAGE_SCHEMA, pkg_bytes)
    return boot_tag, state_tag, pkg_tag


def main() -> int:
    boot_tag, state_tag, pkg_tag = build_golden_devnet_vectors()
    assert boot_tag == "#bb.policy.sha256.c6rrz6ronxsgc7unwdubse4epp7xobqqu4jkzgnghkpel3ozydta", boot_tag
    assert state_tag == "#bb.policy.sha256.yzeh72ixpeuu4jyvt3uakqtznxkhmdqbeigcmwl4jlhrsp2dk6va", state_tag
    assert pkg_tag == "#bb.receipt.sha256.adu7bin3lwr47yecbamuvl6zjiy5qxes46dtvwkm3zfwuzmm3rsq", pkg_tag
    if "--print-tags" in sys.argv:
        print(f"BOOTSTRAP_TAG={boot_tag}")
        print(f"LAUNCH_STATE_TAG={state_tag}")
        print(f"PACKAGE_TAG={pkg_tag}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
