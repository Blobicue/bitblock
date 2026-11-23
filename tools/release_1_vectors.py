#!/usr/bin/env python3
"""Clean-room pure-Python independent verifier & vector suite for RELEASE-1@base/@admitted (PS-089/PS-090).

Imports NO Rust or Bitblock code. Independently reproduces all 4 Wave 29 schemas:
  * bitblock/admitted-release-core/v1 (438-byte authorization-free core)
  * bitblock/release-sign-doc/v1 (246-byte release signing document)
  * bitblock/release-authorization-sign-doc/v1 (250-byte detached length-framed sign payload)
  * bitblock/admitted-release-record/v1 (complete 3-of-5 Ed25519 signed release record)
plus RFC 8032 strict Ed25519 signature verification under SigningDomainTagV1::ReleaseArtifact (9)
and negative controls (core/policy/sig/status tampering).
"""

from __future__ import annotations

import hashlib
import struct
import sys

from crypto_d1_vectors import (
    SIGNING_PREIMAGE_SCHEMA,
    content_tag,
    ed_public_from_seed,
    ed_sign,
    ed_verify_strict,
    encode_signing_preimage,
)
from profile_1_vectors import build_profile_1_vectors
from release_r1_vectors import (
    BUILD_INPUTS_SCHEMA,
    CANDIDATE_RECEIPT_SCHEMA,
    RELR1_CONFIG_PREFIX,
    RELR1_FEATURE_PREFIX,
    RELR1_SOURCE_PREFIX,
)
from release_repro_framework import build_framework_manifest

CORE_SCHEMA = b"bitblock/admitted-release-core/v1"
SIGN_DOC_SCHEMA = b"bitblock/release-sign-doc/v1"
AUTH_SIGN_DOC_SCHEMA = b"bitblock/release-authorization-sign-doc/v1"
RECORD_SCHEMA = b"bitblock/admitted-release-record/v1"

REL1_POLICY_PREFIX = b"bb.rel1.trusted_release_authority_policy.v1"


def build_release_1_vectors() -> tuple[str, str, str, str, dict]:
    _, _, p1 = build_profile_1_vectors()
    parity = p1["parity"]
    raw = parity["raw"]
    genesis_id = raw["genesis_id"]
    profile_id = raw["profile_id"]
    dom_reg_d = raw["commit_dom_reg_d"]
    spec_dig = raw["spec_dig"]
    parity_record_id = raw["parity_record_id_bytes"]
    admission_id = p1["manifest_dig"]

    # Reproduce CandidateBuildInputsV1 & CandidateReleaseReceiptV1 bound to CanonicalDevnetGenesisFixtureV1
    fw = build_framework_manifest()
    m = fw["manifest"]
    fw_sha = bytes.fromhex(fw["framework_manifest_sha256"])
    rust_tc_sha = bytes.fromhex(m["rust_toolchain_sha256"])
    lean_tc_sha = bytes.fromhex(m["lean_toolchain_sha256"])
    cargo_lock_sha = bytes.fromhex(m["cargo_lock_sha256"])
    sbom_sha = bytes.fromhex(m["sbom_sha256"])
    wasm_sha = bytes.fromhex(m["committed_wasm_sha256"])

    src_dig = hashlib.sha256(RELR1_SOURCE_PREFIX + b"bitblock-launch-candidate-v1-tree").digest()
    feat_dig = hashlib.sha256(RELR1_FEATURE_PREFIX + b"default+protocol-research").digest()
    cfg_dig = hashlib.sha256(
        RELR1_CONFIG_PREFIX + b"release+locked+remap-path-prefix+wasm32-simd128-panic-abort"
    ).digest()

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
        + bytes([1])
    )
    assert len(inputs_bytes) == 363
    _, inputs_dig = content_tag("policy", BUILD_INPUTS_SCHEMA, inputs_bytes)

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
        + bytes([1, 1, 0])
    )
    assert len(rcpt_bytes) == 217
    _, rcpt_dig = content_tag("receipt", CANDIDATE_RECEIPT_SCHEMA, rcpt_bytes)

    # Deterministic 3-of-5 devnet test release authority policy (DEVNET-TEST-KEYS-NOT-PRODUCTION-CUSTODY)
    key_pairs = sorted(
        [(ed_public_from_seed(bytes([s]) * 32), bytes([s]) * 32) for s in (41, 42, 43, 44, 45)],
        key=lambda x: x[0],
    )
    h_pol = hashlib.sha256(REL1_POLICY_PREFIX + bytes([3, 5]))
    for pk, _ in key_pairs:
        h_pol.update(pk)
    policy_id = h_pol.digest()

    ctx_ref_bytes = bytes([1]) + genesis_id + bytes([0]) + policy_id
    assert len(ctx_ref_bytes) == 66
    hist_ref_bytes = bytes([1]) + genesis_id + policy_id + bytes(32 + 8 + 8 + 1)
    assert len(hist_ref_bytes) == 114

    # 1. AdmittedReleaseCoreV1 (438 bytes)
    core_bytes = (
        struct.pack(">H", 1)
        + profile_id
        + spec_dig
        + admission_id
        + genesis_id
        + parity_record_id
        + inputs_dig
        + rcpt_dig
        + fw_sha
        + nat_dig
        + struct.pack(">Q", 4_194_304)
        + wasm_sha
        + struct.pack(">Q", 1_831_314)
        + bytes([1])  # LinuxAarch64Elf
        + ctx_ref_bytes
        + policy_id
        + bytes([1])  # Active
    )
    assert len(core_bytes) == 438
    core_tag, core_id = content_tag("policy", CORE_SCHEMA, core_bytes)

    # 2. ReleaseSignDocV1 (246 bytes)
    sd_bytes = struct.pack(">H", 1) + core_id + ctx_ref_bytes + hist_ref_bytes + policy_id
    assert len(sd_bytes) == 246
    sd_tag, sd_id = content_tag("policy", SIGN_DOC_SCHEMA, sd_bytes)

    # 3. ReleaseAuthorizationSignDocEnvelopeV1 (250 bytes)
    auth_env_bytes = struct.pack(">I", len(sd_bytes)) + sd_bytes
    assert len(auth_env_bytes) == 250
    auth_env_tag, auth_env_dig = content_tag("policy", AUTH_SIGN_DOC_SCHEMA, auth_env_bytes)

    # SigningPreimageV1 (domain_tag = 9 ReleaseArtifact, launch_manifest_id = [0; 32])
    preimage_bytes = encode_signing_preimage(
        1, 77, genesis_id, bytes(32), profile_id, 9, 1, 1, auth_env_dig
    )
    _, signing_dig = content_tag("receipt", SIGNING_PREIMAGE_SCHEMA, preimage_bytes)

    sigs_out = bytearray()
    for idx in range(3):
        pk, seed = key_pairs[idx]
        sig = ed_sign(seed, signing_dig)
        assert ed_verify_strict(pk, signing_dig, sig)
        sigs_out += bytes([idx]) + pk + sig

    # 4. AdmittedReleaseRecordV1
    rec_bytes = (
        struct.pack(">H", 1)
        + core_bytes
        + core_id
        + struct.pack(">H", 1)
        + sd_bytes
        + sd_id
        + struct.pack(">H", 1)  # Ed25519StrictRfc8032V1
        + policy_id
        + struct.pack(">H", 3)
        + bytes(sigs_out)
    )
    rec_tag, rec_id = content_tag("receipt", RECORD_SCHEMA, rec_bytes)

    # Powered negative controls: core tamper, withdrawal status flip, signature tamper
    for off in (2, 437):
        mut_c = bytearray(core_bytes)
        mut_c[off] ^= 0x01
        assert content_tag("policy", CORE_SCHEMA, bytes(mut_c))[0] != core_tag
    mut_r = bytearray(rec_bytes)
    mut_r[-1] ^= 0x01
    assert content_tag("receipt", RECORD_SCHEMA, bytes(mut_r))[0] != rec_tag

    return core_tag, sd_tag, auth_env_tag, rec_tag, {
        "p1": p1,
        "inputs_bytes": inputs_bytes,
        "inputs_dig": inputs_dig,
        "rcpt_bytes": rcpt_bytes,
        "rcpt_dig": rcpt_dig,
        "policy_id": policy_id,
        "core_bytes": core_bytes,
        "core_id": core_id,
        "rec_bytes": rec_bytes,
        "rec_id": rec_id,
    }


def main() -> int:
    core_tag, sd_tag, auth_env_tag, rec_tag, _ = build_release_1_vectors()
    if "--print-tags" in sys.argv:
        print("CORE_TAG    =", core_tag)
        print("SIGNDOC_TAG =", sd_tag)
        print("AUTHDOC_TAG =", auth_env_tag)
        print("RECORD_TAG  =", rec_tag)
        return 0
    assert core_tag == EXPECTED_CORE_TAG, f"core mismatch: {core_tag}"
    assert sd_tag == EXPECTED_SIGNDOC_TAG, f"signdoc mismatch: {sd_tag}"
    assert auth_env_tag == EXPECTED_AUTHDOC_TAG, f"authdoc mismatch: {auth_env_tag}"
    assert rec_tag == EXPECTED_RECORD_TAG, f"record mismatch: {rec_tag}"
    print(f"release_1_vectors: PASS (core={core_tag[:25]}..., record={rec_tag[:26]}...)")
    return 0


EXPECTED_CORE_TAG = "#bb.policy.sha256.ounjvzefp2yesle4hlbmlo5m6elwvl4uk76v4ajudelek2bgjrga"
EXPECTED_SIGNDOC_TAG = "#bb.policy.sha256.ehdsiakhtw2nbziey26vwmklsii2kjxbsty3c4prpavmcj77noea"
EXPECTED_AUTHDOC_TAG = "#bb.policy.sha256.nbu5622ibijedtdqniy6taoo7xsauu7kuf66xvpwgquqmx2j5yea"
EXPECTED_RECORD_TAG = "#bb.receipt.sha256.qui5yzjsvwkfvbh7ihzva3swlvfgujvvxw3x6hxbmutto3udqlba"

if __name__ == "__main__":
    raise SystemExit(main())
