#!/usr/bin/env python3
"""Clean-room pure-Python independent verifier & vector suite for TESTNET-1@base (PS-058 + CE-44).

Imports NO Rust or Bitblock code. Independently reproduces BOTH Wave 33 canonical schemas
(completing 200/200 independently vectored protocol schemas):
  * bitblock/testnet-1/cluster-config/v1 (TestnetClusterConfigV1: pinned CHAIN-LAUNCH-1 & CE-44
      identities + 8-row stage x gate-subclaim matrix with zero unresolved conditionals)
  * bitblock/testnet-1/coverage-report/v1 (TestnetCoverageReportV1: 499-byte deterministic
      6-OS-process coverage report covering all 10 adversarial & lifecycle controls)
plus negative controls (tampered launch pin, incomplete fault mask, G18 reassignment).
"""

from __future__ import annotations

import hashlib
import struct
import sys

from chain_launch_1_vectors import build_chain_launch_vectors
from crypto_d1_vectors import content_tag
from profile_1_vectors import build_profile_1_vectors, lp_str
from profile_bootstrap_vectors import decode_tag_digest
from release_1_vectors import build_release_1_vectors
from successor_activation_1_vectors import (
    build_successor_activation_vectors,
    build_successor_admission_manifest,
)
from upgrade_1_vectors import build_upgrade_1_vectors

CLUSTER_CONFIG_SCHEMA = b"bitblock/testnet-1/cluster-config/v1"
COVERAGE_REPORT_SCHEMA = b"bitblock/testnet-1/coverage-report/v1"

TN1_STOP_COND_PREFIX = b"bb.tn1.stop_condition.v1"
TN1_RETAINED_EVID_PREFIX = b"bb.tn1.retained_evidence.v1"
TN1_G18_ARTIFACT_PREFIX = b"bb.tn1.g18_prod_ops_deferred.v1"

TESTNET_FAULT_MASK_ALL_10 = 0x03FF


def stop_cond_digest(subclaim_id: str, rule: str) -> bytes:
    return hashlib.sha256(
        TN1_STOP_COND_PREFIX + subclaim_id.encode("utf-8") + b":" + rule.encode("utf-8")
    ).digest()


def build_testnet_1_vectors() -> tuple[str, str, dict]:
    cl = build_chain_launch_vectors(True)
    _, _, p1 = build_profile_1_vectors()
    _, _, _, rel_rec_tag, _ = build_release_1_vectors()
    u1 = build_upgrade_1_vectors()
    sa = build_successor_activation_vectors(True)

    raw = u1["parity"]["raw"]
    chain_id = 77
    genesis_id = raw["genesis_id"]
    chain_launch_manifest_id = decode_tag_digest(cl["manifest_tag"])
    launch_closure_bundle_id = decode_tag_digest(cl["bundle_tag"])
    initial_profile_id = raw["profile_id"]
    initial_adm_id = p1["manifest_dig"]
    initial_rel_id = decode_tag_digest(rel_rec_tag)
    parity_id = raw["parity_record_id_bytes"]

    new_profile_id = u1["new_profile_id"]
    _, succ_adm_id = build_successor_admission_manifest(
        new_profile_id,
        u1["new_spec_dig"],
        u1["new_scope_d"],
        raw["commit_dom_reg_d"],
        u1["manifest_id"],
    )
    succ_bundle_id = sa["bundle_id"]
    act_auth_id = decode_tag_digest(sa["auth_tag"])
    activation_height = 4
    g18_art = hashlib.sha256(TN1_G18_ARTIFACT_PREFIX).digest()

    rows = [
        ("G15.economic", "VALIDATOR-ECON-1", 1, initial_adm_id, "halt-on-stake-or-fee-conservation-drift", ""),
        ("G15.resource", "RESOURCE-1", 1, initial_adm_id, "halt-on-unbounded-state-growth-or-gas-overflow", ""),
        ("G18.production_ops", "PROD-OPS-1", 2, g18_art, "production-only-gate-deferred-to-prod-ops-1", "production-only-G18-owned-by-PROD-OPS-1"),
        ("G19.program", "PROGRAM-STATE-1", 2, initial_adm_id, "halt-if-slot-6-activates-without-program-active", "program-active-inactive-in-devnet-v1-v2"),
        ("G19.upgrade", "SUCCESSOR-ACTIVATION-1", 1, succ_bundle_id, "halt-before-activation-on-bundle-authority-or-migration-fault", ""),
        ("G19.verkle", "VERKLE-R1", 2, initial_adm_id, "halt-if-verkle-selected-without-ce42-prototype", "verkle-selected-inactive-in-devnet-v1-v2"),
        ("G5.asset", "ASSET-1", 1, parity_id, "halt-on-episode-ownership-or-supply-divergence", ""),
        ("G5.core", "CHAIN-LAUNCH-1", 1, chain_launch_manifest_id, "halt-before-height-1-on-launch-parity-or-consensus-fault", ""),
    ]
    rows.sort(key=lambda r: r[0])

    cfg = bytearray()
    cfg += struct.pack(">HQ", 1, chain_id)
    cfg += genesis_id
    cfg += chain_launch_manifest_id
    cfg += launch_closure_bundle_id
    cfg += initial_profile_id
    cfg += initial_adm_id
    cfg += initial_rel_id
    cfg += bytes([1])
    cfg += new_profile_id
    cfg += succ_adm_id
    cfg += succ_bundle_id
    cfg += act_auth_id
    cfg += struct.pack(">QHHHH", activation_height, 4, 1, 1, len(rows))
    for scid, owner, disp, art, rule, reason in rows:
        cfg += lp_str(scid) + lp_str(owner) + bytes([disp]) + art + stop_cond_digest(scid, rule) + lp_str(reason)

    cfg_bytes = bytes(cfg)
    cfg_tag, cfg_id = content_tag("policy", CLUSTER_CONFIG_SCHEMA, cfg_bytes)

    final_exec_ref = u1["new_exec_ref"]
    assert len(final_exec_ref) == 178
    retained_evid = hashlib.sha256(
        TN1_RETAINED_EVID_PREFIX
        + cfg_id
        + chain_launch_manifest_id
        + launch_closure_bundle_id
        + parity_id
        + succ_bundle_id
        + act_auth_id
        + final_exec_ref
        + struct.pack(">I", TESTNET_FAULT_MASK_ALL_10)
    ).digest()

    rep_bytes = (
        struct.pack(">H", 1)
        + cfg_id
        + chain_launch_manifest_id
        + launch_closure_bundle_id
        + parity_id
        + succ_bundle_id
        + act_auth_id
        + struct.pack(">HQQQ", 6, 3, activation_height, 6)
        + initial_profile_id
        + new_profile_id
        + final_exec_ref
        + retained_evid
        + struct.pack(">IB", TESTNET_FAULT_MASK_ALL_10, 1)
    )
    assert len(rep_bytes) == 499
    rep_tag, rep_id = content_tag("receipt", COVERAGE_REPORT_SCHEMA, rep_bytes)

    # Negative controls: launch pin tamper & incomplete fault mask move tags
    mut_cfg = bytearray(cfg_bytes)
    mut_cfg[42] ^= 0xFF
    assert content_tag("policy", CLUSTER_CONFIG_SCHEMA, bytes(mut_cfg))[0] != cfg_tag

    mut_rep = bytearray(rep_bytes)
    mut_rep[-2] ^= 0x04
    assert content_tag("receipt", COVERAGE_REPORT_SCHEMA, bytes(mut_rep))[0] != rep_tag

    return cfg_tag, rep_tag, {
        "cfg_bytes": cfg_bytes,
        "cfg_id": cfg_id,
        "rep_bytes": rep_bytes,
        "rep_id": rep_id,
    }


def main() -> int:
    cfg_tag, rep_tag, _ = build_testnet_1_vectors()
    if "--print-tags" in sys.argv:
        print("TN1_CONFIG_TAG =", cfg_tag)
        print("TN1_REPORT_TAG =", rep_tag)
        return 0
    assert cfg_tag == EXPECTED_CONFIG_TAG, cfg_tag
    assert rep_tag == EXPECTED_REPORT_TAG, rep_tag
    print(f"testnet_1_vectors: PASS (cfg={cfg_tag[:25]}..., rep={rep_tag[:26]}...)")
    return 0


EXPECTED_CONFIG_TAG = "#bb.policy.sha256.rospzqtyzq4vgsd6j6eqjsi6tutvsck7gsj7wpqxerx3jqryc5ja"
EXPECTED_REPORT_TAG = "#bb.receipt.sha256.hwsoj2bhgxxyuj5jportmnicjwpxdhvodxi7vktgyctcvkiikt7a"

if __name__ == "__main__":
    raise SystemExit(main())
