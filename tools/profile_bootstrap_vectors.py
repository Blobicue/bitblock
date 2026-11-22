#!/usr/bin/env python3
"""Independent stdlib vectors for PROFILE-SCOPE-D0, GENESIS-D1, and PROTOCOL-ID-1.

Imports no Rust/Bitblock code. Reconstructs all three canonical bootstrap schemas:
  * bitblock/profile-scope-d0/scope/v1
  * bitblock/genesis-d1/chain-genesis-payload/v1
  * bitblock/protocol-id-1/accepted-proposal-core/v1
"""

from __future__ import annotations

import base64
import struct
import sys

from commit_state_vectors import EXPECTED_DOMAIN_REG_TAG
from crypto_d1_vectors import content_tag, ed_public_from_seed, encode_suite_descriptor
from pre_genesis_d1_vectors import (
    ASSET_POLICY_SCHEMA,
    CONSENSUS_POLICY_SCHEMA,
    GOVERNANCE_POLICY_SCHEMA,
    VALIDATOR_ECON_POLICY_SCHEMA,
    encode_asset_policy,
    encode_consensus_policy,
    encode_governance_policy,
    encode_validator_econ_policy,
    u128_be,
)
from transition_spec_d1_vectors import CONTRACT_SCHEMA, encode_base_contract

SCOPE_SCHEMA = b"bitblock/profile-scope-d0/scope/v1"
GENESIS_SCHEMA = b"bitblock/genesis-d1/chain-genesis-payload/v1"
PROPOSAL_CORE_SCHEMA = b"bitblock/protocol-id-1/accepted-proposal-core/v1"


def decode_tag_digest(tag_str: str) -> bytes:
    b32 = tag_str.rsplit(".", 1)[1].upper()
    pad = "=" * ((8 - len(b32) % 8) % 8)
    return base64.b32decode(b32 + pad)


def main() -> int:
    _, base_spec_digest = content_tag("policy", CONTRACT_SCHEMA, encode_base_contract([]))
    domain_reg_digest = decode_tag_digest(EXPECTED_DOMAIN_REG_TAG)
    _, crypto_digest = content_tag(
        "policy", b"bitblock/crypto-d1/suite-descriptor/v1", encode_suite_descriptor()
    )
    _, asset_digest = content_tag("policy", ASSET_POLICY_SCHEMA, encode_asset_policy())
    _, econ_digest = content_tag(
        "policy", VALIDATOR_ECON_POLICY_SCHEMA, encode_validator_econ_policy()
    )
    _, cons_digest = content_tag("policy", CONSENSUS_POLICY_SCHEMA, encode_consensus_policy())
    council_keys = [ed_public_from_seed(bytes([s]) * 32) for s in (31, 32, 33)]
    _, gov_digest = content_tag(
        "policy", GOVERNANCE_POLICY_SCHEMA, encode_governance_policy(council_keys)
    )

    preds = [
        "asset-active",
        "economic-charging",
        "stake-consensus",
        "validator-economics",
    ]
    scope_bytes = bytearray()
    scope_bytes += struct.pack(">H", 1)
    scope_bytes += (
        base_spec_digest
        + domain_reg_digest
        + crypto_digest
        + asset_digest
        + econ_digest
        + cons_digest
        + gov_digest
    )
    scope_bytes += struct.pack(">IIIIIH", 16_384, 1_024, 65_536, 12_288, 65_536, len(preds))
    for p in preds:
        pb = p.encode("utf-8")
        scope_bytes += struct.pack(">H", len(pb)) + pb
    scope_tag, scope_digest = content_tag("policy", SCOPE_SCHEMA, bytes(scope_bytes))

    ns = b"bitblock-mainnet-v1"
    genesis_bytes = (
        struct.pack(">H", 1)
        + scope_digest
        + domain_reg_digest
        + struct.pack(">QQH", 1, 1, len(ns))
        + ns
        + bytes([0x11]) * 32
        + bytes([0x22]) * 32
        + bytes([0x33]) * 32
        + u128_be(1_000_000_000_000_000)
    )
    genesis_tag, genesis_digest = content_tag("policy", GENESIS_SCHEMA, genesis_bytes)

    core_bytes = (
        struct.pack(">HQ", 1, 1)
        + genesis_digest
        + bytes([0x44]) * 32
        + struct.pack(">QQ", 1, 0)
        + genesis_digest
        + bytes([0x33]) * 32
        + bytes([0x55]) * 32
        + bytes([0x66]) * 32
        + bytes([0x77]) * 32
    )
    assert len(core_bytes) == 250
    core_tag, _ = content_tag("receipt", PROPOSAL_CORE_SCHEMA, core_bytes)

    assert scope_tag == "#bb.policy.sha256.yzu3porxqsiiivkwgepymeiuphc7er4wdl226g6go4kfaingn2eq", scope_tag
    assert genesis_tag == "#bb.policy.sha256.q7pz74ykis4uuqina62xd6immkzczt4c77y6g4elrum5z37xdhcq", genesis_tag
    assert core_tag == "#bb.receipt.sha256.7a5oxjjdyl5sfkec3va6ziw4q5mxomxmqkwhsfrkwuay6mmfr65q", core_tag
    print(f"profile_bootstrap_vectors: ok (scope={scope_tag}, 3/3 golden tags pinned)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
