#!/usr/bin/env python3
"""Clean-room pure-Python independent verifier & vector suite for RELEASE-R1@candidate (PS-088/PS-086).

Imports NO Rust or Bitblock code. Independently reproduces:
  * bitblock/release-r1/candidate-build-inputs/v1 (363-byte profile-bound build inputs)
  * bitblock/release-r1/candidate-build-receipt/v1 (217-byte dual-builder candidate receipt)
  * Cross-checks live RELEASE-R1@framework digests (Cargo.lock 220 pkgs / 11 direct pins,
    rust-toolchain.toml, proof/lean-toolchain, committed wasm32 +simd128 artifact)
  * 7/7 mutation negative controls + admitted_release_authority=true refusal.
"""

from __future__ import annotations

import hashlib
import struct
import sys

from commit_state_vectors import EXPECTED_DOMAIN_REG_TAG
from consensus_1_vectors import build_consensus_1_vectors, decode_tag_digest
from crypto_d1_vectors import content_tag
from release_repro_framework import build_framework_manifest

BUILD_INPUTS_SCHEMA = b"bitblock/release-r1/candidate-build-inputs/v1"
CANDIDATE_RECEIPT_SCHEMA = b"bitblock/release-r1/candidate-build-receipt/v1"

RELR1_FEATURE_PREFIX = b"bb.relr1.feature_profile.v1"
RELR1_CONFIG_PREFIX = b"bb.relr1.build_config.v1"
RELR1_SOURCE_PREFIX = b"bb.relr1.source_tree.v1"


def build_release_r1_vectors() -> tuple[str, str]:
    _, _, ctx = build_consensus_1_vectors()
    genesis_id = ctx["genesis_id"]
    profile_id = ctx["profile_id"]
    dom_reg_d = decode_tag_digest(EXPECTED_DOMAIN_REG_TAG)

    fw = build_framework_manifest()
    m = fw["manifest"]
    fw_sha = bytes.fromhex(fw["framework_manifest_sha256"])
    rust_tc_sha = bytes.fromhex(m["rust_toolchain_sha256"])
    lean_tc_sha = bytes.fromhex(m["lean_toolchain_sha256"])
    cargo_lock_sha = bytes.fromhex(m["cargo_lock_sha256"])
    sbom_sha = bytes.fromhex(m["sbom_sha256"])
    wasm_sha = bytes.fromhex(m["committed_wasm_sha256"])
    assert m["cargo_lock_package_count"] == 220
    assert len(m["direct_dependency_pins"]) == 11

    src_dig = hashlib.sha256(RELR1_SOURCE_PREFIX + b"bitblock-launch-candidate-v1-tree").digest()
    feat_dig = hashlib.sha256(RELR1_FEATURE_PREFIX + b"default+protocol-research").digest()
    cfg_dig = hashlib.sha256(
        RELR1_CONFIG_PREFIX + b"release+locked+remap-path-prefix+wasm32-simd128-panic-abort"
    ).digest()

    # 1. CandidateBuildInputsV1 (363 bytes)
    inputs_bytes = (
        struct.pack(">H", 1)
        + profile_id
        + genesis_id
        + dom_reg_d
        + fw_sha
        + src_dig
        + rust_tc_sha
        + lean_tc_sha
        + cargo_lock_sha
        + sbom_sha
        + struct.pack(">II", 220, 11)
        + feat_dig
        + cfg_dig
        + bytes([1])  # LinuxAarch64Elf
    )
    assert len(inputs_bytes) == 363
    inputs_tag, inputs_dig = content_tag("policy", BUILD_INPUTS_SCHEMA, inputs_bytes)

    # 2. CandidateReleaseReceiptV1 (217 bytes)
    nat_dig = hashlib.sha256(b"bb.relr1.canonical_native_elf_fixture.v1").digest()
    rcpt_bytes = (
        struct.pack(">H", 1)
        + inputs_dig
        + profile_id
        + genesis_id
        + fw_sha
        + nat_dig
        + struct.pack(">Q", 4_194_304)
        + wasm_sha
        + struct.pack(">QI", 1_831_314, 2)
        + bytes([1, 1, 0])  # builders_bit_identical=1, platform_scoped_only=1, admitted_release_authority=0
    )
    assert len(rcpt_bytes) == 217
    rcpt_tag, _ = content_tag("receipt", CANDIDATE_RECEIPT_SCHEMA, rcpt_bytes)

    # 7/7 mutation negative controls: source, toolchain, sbom/dep, feature, profile_id, config, binary
    for offset in (2, 130, 162, 258, 298, 330):
        mut = bytearray(inputs_bytes)
        mut[offset] ^= 0x01
        assert content_tag("policy", BUILD_INPUTS_SCHEMA, bytes(mut))[0] != inputs_tag
    # Binary digest mutation & admitted_release_authority=true mutation
    for offset in (130, 216):
        mut_r = bytearray(rcpt_bytes)
        mut_r[offset] ^= 0x01
        assert content_tag("receipt", CANDIDATE_RECEIPT_SCHEMA, bytes(mut_r))[0] != rcpt_tag
    return inputs_tag, rcpt_tag


def main() -> int:
    inputs_tag, rcpt_tag = build_release_r1_vectors()
    if "--print-tags" in sys.argv:
        print("INPUTS_TAG =", inputs_tag)
        print("RCPT_TAG   =", rcpt_tag)
        return 0
    assert inputs_tag == EXPECTED_INPUTS_TAG, f"inputs mismatch: {inputs_tag}"
    assert rcpt_tag == EXPECTED_RCPT_TAG, f"rcpt mismatch: {rcpt_tag}"
    print(f"release_r1_vectors: PASS (inputs={inputs_tag[:25]}..., rcpt={rcpt_tag[:26]}...)")
    return 0


EXPECTED_INPUTS_TAG = "#bb.policy.sha256.2wlogxgqgwzblmcwtpgvmai2h3s5vagoagxgywtjqmzcwxioryaq"
EXPECTED_RCPT_TAG = "#bb.receipt.sha256.zyk4zg44x3q46h36xnnpghcklfndgulmx7pmuspm3d546vcblnfa"

if __name__ == "__main__":
    raise SystemExit(main())
