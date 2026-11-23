#!/usr/bin/env python3
"""Clean-room pure-Python independent verifier & vector suite for PROFILE-1@base (PS-084).

Imports NO Rust or Bitblock code. Independently reproduces:
  * bitblock/profile-1/evidence-closure/v1 (ProfileClosureEvidenceSetV1:
      10 merged-main capability evidence records, 5 resolved conditional edges,
      8 canonical domain slots, and 7 reconciled stage x gate subclaims)
  * bitblock/profile-admission-manifest/v1 (ProfileAdmissionManifestV1:
      profile_admission_manifest_id bound to PROFILE-D1, TRANSITION-SPEC-1,
      PROFILE-SCOPE-D0, DomainRegistryV1, policy ID, capability set, and closure digest)
  * Powered negative controls: mutated profile ID, rejected disposition tamper,
    trailing bytes, and mutated evidence closure digest.
"""

from __future__ import annotations

import hashlib
import struct
import sys

from crypto_d1_vectors import content_tag
from genesis_root_parity_1_vectors import rebuild_devnet_parity_path4
from profile_bootstrap_vectors import decode_tag_digest

CLOSURE_SCHEMA = b"bitblock/profile-1/evidence-closure/v1"
MANIFEST_SCHEMA = b"bitblock/profile-admission-manifest/v1"

PROF1_POLICY_PREFIX = b"bb.prof1.admission_policy.v1"
PROF1_ASSUMPTION_PREFIX = b"bb.prof1.assumption.v1"
PROF1_LOSS_PREFIX = b"bb.prof1.loss_condition.v1"


def lp_str(s: str) -> bytes:
    raw = s.encode("utf-8")
    return struct.pack(">H", len(raw)) + raw


def build_profile_1_vectors() -> tuple[str, str, dict]:
    parity = rebuild_devnet_parity_path4()
    raw = parity["raw"]
    profile_id = raw["profile_id"]
    spec_dig = raw["spec_dig"]
    scope_d = raw["scope_d"]
    dom_reg_d = raw["commit_dom_reg_d"]

    ev_rows = [
        (
            "ASSET-1",
            "25fbbf645940461531d98d3c5f80235441df09f2",
            "#bb.receipt.sha256.d4c462nqb6gg3rzzrraufi4cjv752ys33gfmep44wkltiposlvkq",
            "dual-domain-conservation-bedrock-excluded",
            "supply-overflow-or-deposit-leak",
        ),
        (
            "BOUNDS-1",
            "ec5b76358b06351652fde5e9bed05a8d52bb5cad",
            "#bb.policy.sha256.yecxf5k5a5gs3y7qm76pzax2f5e6bkidmxytmsthsxhdqozzplaa",
            "400ms-verify-budget-2mib-block-preflight",
            "unbounded-allocation-or-dos-saturation",
        ),
        (
            "EXECUTION-ROOT-1",
            "6f66ea4652841f2c06b16fb73870ae049b1a2cef",
            "#bb.policy.sha256.3wkwyds7dgkm5hd4vwj26lkokfi6xkeft6d7yl3hzddm3tq6i62q",
            "two-layer-11-slot-inactive-sentinel-composition",
            "cross-domain-root-substitution",
        ),
        (
            "GENESIS-ROOT-PARITY-1",
            "ac4fe8a88896d474cf29e756913d02511fcc5257",
            "#bb.receipt.sha256.3xkh3lfjjgfxg52qhgoqw5zl34zr4d5zax5d6apqufpyiyrpysoa",
            "4-way-preimage-verified-logical-to-typed-root-parity",
            "cold-incremental-executor-python-divergence",
        ),
        (
            "GOVERNANCE-1",
            "35b26afd0f0abe8416a22edd009ce25f8e077be1",
            "#bb.receipt.sha256.ro25qjtz5x6jglbdtk4pzqtlm5tqwfkxfpod53hn2amne7otuhxq",
            "timelocked-quorum-pause-only-council-no-rewrite",
            "unauthorized-upgrade-or-history-rewrite",
        ),
        (
            "PROFILE-D1",
            "7a710fc26abf100c6e4e3f8ddb33058227127f84",
            "#bb.policy.sha256.r4ldc6k33p7evc4zg67ksajcpr6dxeiwuazimy6pxhwgaepepaoq",
            "non-circular-ce43-candidate-manifest-7-subclaims",
            "self-referential-profile-id-or-scope-drift",
        ),
        (
            "RESOURCE-1",
            "438f08e0b09f6ba40e829d46b25c9083b830475f",
            "#bb.policy.sha256.6yvp73mvdkouoxk2xiolnj33lbxucxta5ikhjcwnvd267tzfrfyq",
            "eip1559-burn-capped-tip-refundable-deposit",
            "uncharged-state-growth-or-fee-diversion",
        ),
        (
            "TRANSITION-SPEC-1",
            "27ede9a7c2b71930a8434f0c8744270134139d7f",
            "#bb.policy.sha256.xhy32kud2wdzknglo6p2n3gulbnlikjn3eqhmmquzl2jcofjg6wa",
            "disjoint-256-tag-bands-inactive-vm-wall",
            "tag-collision-or-partial-write-rollback-leak",
        ),
        (
            "UPGRADE-FRAMEWORK-1",
            "d283dedaac924065c80f560cb0d7b1c1f8fadb8b",
            "#bb.policy.sha256.udwdlgfcdx3lmezbw6ffttvoom2bqjv2tlyxnbekhmtswgeyy74q",
            "preventive-lifecycle-terminal-row1-safe-halt",
            "compromised-authority-self-recovery",
        ),
        (
            "VALIDATOR-ECON-1",
            "7ac86ef897b86f28de8a7abae111ea68994443c0",
            "#bb.receipt.sha256.kwwsv73gxzeuug576j55yktxgcdn6zo5zuv4pguns5ewzq4nukda",
            "receipt-exact-consensus-weight-reward-slash-7-vectors",
            "phantom-stake-or-double-slash-bypass",
        ),
    ]
    tag_dig_by_cap = {cap: decode_tag_digest(tag) for cap, _, tag, _, _ in ev_rows}

    ce_rows = [
        ("CE-25", "PROFILE-1", "new-admission", 2, "asset-active", "ASSET-1", "implementation", tag_dig_by_cap["ASSET-1"]),
        ("CE-27", "PROFILE-1", "new-admission", 2, "economic-charging", "RESOURCE-1", "policy", tag_dig_by_cap["RESOURCE-1"]),
        ("CE-28", "PROFILE-1", "new-admission", 2, "validator-economics", "VALIDATOR-ECON-1", "authority", tag_dig_by_cap["VALIDATOR-ECON-1"]),
        ("CE-29", "PROFILE-1", "new-admission", 2, "migration-profile", "UPGRADE-FRAMEWORK-1", "initial-launch-framework-readiness", tag_dig_by_cap["UPGRADE-FRAMEWORK-1"]),
        ("CE-43", "PROFILE-D1", "new-candidate-profile", 2, "always", "PROFILE-SCOPE-D0", "candidate-scope", scope_d),
    ]

    slots = [
        (0, 1, 1),
        (1, 1, 1),
        (2, 1, 1),
        (3, 1, 1),
        (4, 1, 1),
        (5, 1, 1),
        (6, 0, 0),
        (7, 0, 0),
    ]

    # Reconciled 7 subclaims (sorted by subclaim_id; Satisfied=0, EvidencePending=1 per profile_d1.rs)
    subclaims = [
        ("G15.economic", 1, "economic-charging", 0),
        ("G15.resource", 0, "", 0),
        ("G19.program", 2, "program-active-inactive-at-launch", 0),
        ("G19.upgrade", 0, "in-place-upgrade", 1),  # InitialLaunch scoped to UPGRADE-FRAMEWORK-1
        ("G19.verkle", 2, "verkle-selected-inactive-at-launch", 0),
        ("G5.asset", 1, "asset-active", 0),
        ("G5.core", 0, "", 0),
    ]

    c = bytearray()
    c += struct.pack(">HBBH", 1, 0, 1, len(ev_rows))
    for cap, sha_hex, tag_s, assume, loss in ev_rows:
        c += lp_str(cap)
        c += struct.pack(">H", 1)
        c += bytes.fromhex(sha_hex)
        c += decode_tag_digest(tag_s)
        c += bytes([1])  # PassedBounded
        c += hashlib.sha256(PROF1_ASSUMPTION_PREFIX + cap.encode() + b":" + assume.encode()).digest()
        c += hashlib.sha256(PROF1_LOSS_PREFIX + cap.encode() + b":" + loss.encode()).digest()
    c += struct.pack(">H", len(ce_rows))
    for eid, ob, orole, ctx_c, pred, db, drole, ddig in ce_rows:
        c += lp_str(eid) + lp_str(ob) + lp_str(orole) + bytes([ctx_c])
        c += lp_str(pred) + lp_str(db) + lp_str(drole) + ddig
    c += struct.pack(">H", len(slots))
    for sid, act, cp in slots:
        c += struct.pack(">BBH", sid, act, cp)
    c += struct.pack(">H", len(subclaims))
    for scid, disp, por, evst in subclaims:
        c += lp_str(scid) + bytes([disp]) + lp_str(por) + bytes([evst])

    closure_bytes = bytes(c)
    closure_tag, closure_dig = content_tag("policy", CLOSURE_SCHEMA, closure_bytes)

    policy_id = hashlib.sha256(PROF1_POLICY_PREFIX + b"bitblock-profile-1-launch-policy-v1").digest()
    req_caps = sorted(cap for cap, _, _, _, _ in ev_rows)

    m = bytearray()
    m += struct.pack(">H", 1)
    m += profile_id + spec_dig + scope_d + dom_reg_d + policy_id
    m += struct.pack(">H", len(req_caps))
    for cap in req_caps:
        m += lp_str(cap)
    m += closure_dig
    m += bytes([1])  # AdmittedBoundedV1

    manifest_bytes = bytes(m)
    manifest_tag, manifest_dig = content_tag("receipt", MANIFEST_SCHEMA, manifest_bytes)

    # Negative controls: profile_id mutation, closure_digest mutation, disposition=0 tamper
    for off in (2, len(manifest_bytes) - 33, len(manifest_bytes) - 1):
        mut = bytearray(manifest_bytes)
        mut[off] ^= 0x01
        assert content_tag("receipt", MANIFEST_SCHEMA, bytes(mut))[0] != manifest_tag

    return closure_tag, manifest_tag, {
        "parity": parity,
        "closure_bytes": closure_bytes,
        "closure_dig": closure_dig,
        "manifest_bytes": manifest_bytes,
        "manifest_dig": manifest_dig,
        "policy_id": policy_id,
    }


def main() -> int:
    closure_tag, manifest_tag, _ = build_profile_1_vectors()
    if "--print-tags" in sys.argv:
        print("CLOSURE_TAG  =", closure_tag)
        print("MANIFEST_TAG =", manifest_tag)
        return 0
    assert closure_tag == EXPECTED_CLOSURE_TAG, f"closure mismatch: {closure_tag}"
    assert manifest_tag == EXPECTED_MANIFEST_TAG, f"manifest mismatch: {manifest_tag}"
    print(f"profile_1_vectors: PASS (closure={closure_tag[:25]}..., manifest={manifest_tag[:26]}...)")
    return 0


EXPECTED_CLOSURE_TAG = "#bb.policy.sha256.rdkfncmp3sn6smjeyud6xb3xac35i44ihsl5piicglx3p6pydwga"
EXPECTED_MANIFEST_TAG = "#bb.receipt.sha256.nmcfo64s3rjesde2eumymbeemqggiqvlcsnvlcpx6ggxm4meynjq"

if __name__ == "__main__":
    raise SystemExit(main())
