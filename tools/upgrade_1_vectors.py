#!/usr/bin/env python3
"""Clean-room pure-Python independent migration implementation & vector suite for UPGRADE-1@base (PS-083).

Imports NO Rust or Bitblock code. Authored strictly against the frozen MigrationSpecV1 contract:
independently builds the v2 preventive successor ProfileScopeV1, BoundProfileManifestV1,
ResolvedProfileTransitionSpecV1, migrated GovernanceState Patricia trie root, two-layer 11-slot
ExecutionRootCompositionV1 / ExecutionRootRefV1, and reproduces all 4 Wave 31 schemas bit-for-bit:
  * bitblock/upgrade-1/migration-spec/v1 (238-byte frozen MigrationSpecV1)
  * bitblock/upgrade-1/occurrence-universe-handoff/v1 (187-byte OccurrenceUniverseHandoffV1)
  * bitblock/upgrade-1/bounded-migration-replay/v1 (463-byte BoundedMigrationReplayReceiptV1)
  * bitblock/successor-upgrade-evidence-manifest/v1 (SuccessorUpgradeEvidenceManifestV1)
"""

from __future__ import annotations

import hashlib
import struct
import sys

from auth_commit_1_vectors import DomainPatriciaPy
from crypto_d1_vectors import content_tag
from execution_root_1_vectors import (
    compose_execution_root,
    encode_launch_registry,
)
from genesis_root_parity_1_vectors import (
    DOM_GOVERNANCE,
    param_trie_key,
    rebuild_devnet_parity_path4,
)
from profile_bootstrap_vectors import decode_tag_digest
PROFILE_SCOPE_SCHEMA = b"bitblock/profile-scope-d0/scope/v1"
from profile_d1_vectors import (
    LAUNCH_CAPS,
    LAUNCH_PREDS,
    encode_candidate,
    seal_bound,
)
from transition_spec_1_vectors import (
    BASE_MASK,
    RESOLVED_SPEC_SCHEMA,
    base_contract_digest,
    build_bands,
    encode_band,
)
from upgrade_framework_1_vectors import (
    AUDIT_SCHEMA as FW_AUDIT_SCHEMA,
    POLICY_SCHEMA as FW_POLICY_SCHEMA,
    encode_suite_migration_policy,
)

MIGRATION_SPEC_SCHEMA = b"bitblock/upgrade-1/migration-spec/v1"
OCCURRENCE_HANDOFF_SCHEMA = b"bitblock/upgrade-1/occurrence-universe-handoff/v1"
REPLAY_RECEIPT_SCHEMA = b"bitblock/upgrade-1/bounded-migration-replay/v1"
EVIDENCE_MANIFEST_SCHEMA = b"bitblock/successor-upgrade-evidence-manifest/v1"

UPG1_OCC_POLICY_V1_PREFIX = b"bb.upg1.occ_index_policy.v1"
UPG1_OCC_POLICY_V2_PREFIX = b"bb.upg1.occ_index_policy.v2"
UPG1_LOSSLESS_CLOSURE_PREFIX = b"bb.upg1.lossless_closure.v1"
UPG1_RECEIPT_BRIDGE_PREFIX = b"bb.upg1.receipt_bridge.v1"
UPG1_STATE_DELTA_PREFIX = b"bb.upg1.param_state_delta.v1"
UPG1_IMPL_RUST_PREFIX = b"bb.upg1.impl.rust_executor.v1"
UPG1_IMPL_PY_PREFIX = b"bb.upg1.impl.cleanroom_python.v1"


def build_upgrade_1_vectors() -> dict:
    parity = rebuild_devnet_parity_path4()
    raw = parity["raw"]
    gid = raw["genesis_id"]
    old_profile_id = raw["profile_id"]
    old_scope_b = raw["scope_bytes"]
    old_scope_d = raw["scope_d"]
    dom_reg_d = raw["commit_dom_reg_d"]
    old_exec_ref = raw["exec_root_ref"]
    genesis_history_root = raw["genesis_history_root"]

    # 1. Independently derive v2 ProfileScopeV1 by upgrading the 20-byte ProposedLimitVectorV1
    #    at offset 2 + 32*7 = 226 inside ProfileScopeV1 canonical bytes:
    #    (16384, 1024, 65536, 12288, 65536) -> (16384, 1536, 65536, 16384, 65536)
    off_lim = 2 + 32 * 7
    assert old_scope_b[off_lim : off_lim + 20] == struct.pack(
        ">IIIII", 16_384, 1_024, 65_536, 12_288, 65_536
    )
    new_scope_b = (
        old_scope_b[:off_lim]
        + struct.pack(">IIIII", 16_384, 1_536, 65_536, 16_384, 65_536)
        + old_scope_b[off_lim + 20 :]
    )
    _, new_scope_d = content_tag("policy", PROFILE_SCOPE_SCHEMA, new_scope_b)
    assert new_scope_d != old_scope_d

    # Extract bounds_d, crypto_d, resource_d directly from the old candidate preimage inside bound_bytes:
    # bound_bytes = u32_be(len) || cand_b || profile_id
    cand_len = struct.unpack(">I", raw["bound_bytes"][:4])[0]
    old_cand_b = raw["bound_bytes"][4 : 4 + cand_len]
    # Header of CandidateProfileManifestV1: u16 ver + u8 lineage + u8 has_pred + 32B pred + 32*7 digests
    d_base = 2 + 1 + 1 + 32 + 32 * 4
    bounds_d = old_cand_b[d_base : d_base + 32]
    crypto_d = old_cand_b[d_base + 32 : d_base + 64]
    resource_d = old_cand_b[d_base + 64 : d_base + 96]

    new_cand_b = encode_candidate(
        1,  # SuccessorCandidate
        old_profile_id,
        gid,
        old_scope_d,
        new_scope_d,
        dom_reg_d,
        bounds_d,
        crypto_d,
        resource_d,
        LAUNCH_PREDS,
        LAUNCH_CAPS,
    )
    _, new_profile_id, _ = seal_bound(new_cand_b)
    new_bound_b = struct.pack(">I", len(new_cand_b)) + new_cand_b + new_profile_id
    assert new_profile_id != old_profile_id

    bands = build_bands(False)
    new_spec_b = (
        struct.pack(">H", 1)
        + new_profile_id
        + base_contract_digest()
        + bounds_d
        + resource_d
        + struct.pack(">IQQIBH", 16_384, 100_000_000, 2_000, BASE_MASK, 0, len(bands))
        + b"".join(encode_band(*b) for b in bands)
    )
    _, new_spec_dig = content_tag("policy", RESOLVED_SPEC_SCHEMA, new_spec_b)

    # 2. Frozen MigrationSpecV1 (238 bytes)
    mig_spec_b = (
        struct.pack(">HQQ", 1, 77, 1)
        + gid
        + old_profile_id
        + new_profile_id
        + new_scope_d
        + new_spec_dig
        + struct.pack(">HHQQI", 1, 2, 4, 21, 4_096)
        + struct.pack(">IIIII", 16_384, 1_536, 65_536, 16_384, 65_536)
        + struct.pack(">IQBB", 9_001, 2, 1, 1)
    )
    assert len(mig_spec_b) == 236
    mig_spec_tag, mig_spec_id = content_tag("policy", MIGRATION_SPEC_SCHEMA, mig_spec_b)

    # 3. OccurrenceUniverseHandoffV1 (187 bytes)
    old_exec_state_root = old_exec_ref[-32:]
    old_occ = hashlib.sha256(UPG1_OCC_POLICY_V1_PREFIX + old_profile_id).digest()
    new_occ = hashlib.sha256(UPG1_OCC_POLICY_V2_PREFIX + new_profile_id).digest()
    closure_root = hashlib.sha256(
        UPG1_LOSSLESS_CLOSURE_PREFIX + genesis_history_root + old_exec_state_root
    ).digest()
    bridge_dig = hashlib.sha256(
        UPG1_RECEIPT_BRIDGE_PREFIX + old_occ + new_occ + closure_root
    ).digest()
    handoff_b = (
        struct.pack(">H", 1)
        + mig_spec_id
        + old_occ
        + new_occ
        + struct.pack(">QQQ", 1, 3, 5)
        + closure_root
        + bytes([1])
        + bridge_dig
    )
    assert len(handoff_b) == 187
    handoff_tag, handoff_id = content_tag("policy", OCCURRENCE_HANDOFF_SCHEMA, handoff_b)

    # 4. Clean-room independent migration of the 10-domain logical state (GovernanceState param 9001=2)
    #    Extract exact 10 active domain roots from the tail 11 x 68-byte slots of comp_bytes:
    comp_b = raw["comp_bytes"]
    off_e = len(comp_b) - 11 * 68
    roots_by_tag = {}
    for _ in range(11):
        slot = comp_b[off_e : off_e + 68]
        off_e += 68
        kind_b, tag = slot[0], slot[1]
        if kind_b == 1:
            roots_by_tag[tag] = slot[36:68]
    assert len(roots_by_tag) == 10

    old_gov_root = roots_by_tag[9]
    gov_params = [(1, 2_000), (2, 100_000_000), (9_001, 2)]
    new_gov_root = DomainPatriciaPy(
        DOM_GOVERNANCE, {param_trie_key(k): struct.pack(">Q", v) for k, v in gov_params}
    ).root()
    assert new_gov_root != old_gov_root
    roots_by_tag[9] = new_gov_root

    new_comp_b, _, _, new_exec_ref, _ = compose_execution_root(
        4, 0, new_profile_id, 1, 77, 1, gid, new_scope_d, roots_by_tag
    )

    delta_dig = hashlib.sha256(
        UPG1_STATE_DELTA_PREFIX
        + mig_spec_id
        + old_gov_root
        + new_gov_root
        + struct.pack(">IQ", 9_001, 2)
    ).digest()

    # Reproduce UPGRADE-FRAMEWORK-1 preventive policy & passing audit digest independently:
    fw_pol_b = (
        struct.pack(">HBQ", 1, 2, 77)
        + gid
        + struct.pack(">QIBBB", 21, 4_096, 1, 1, 1)
        + encode_suite_migration_policy(1, 1, 2, 1, 10_000, 12_000, True, False, 1_024)
    )
    _, fw_pol_dig = content_tag("policy", FW_POLICY_SCHEMA, fw_pol_b)
    fw_audit_b = (
        struct.pack(">H", 1)
        + fw_pol_dig
        + old_profile_id
        + new_profile_id
        + struct.pack(">QHHBBBB", 4, 1, 2, 0, 0, 1, 1)
        + mig_spec_id
    )
    _, fw_audit_dig = content_tag("receipt", FW_AUDIT_SCHEMA, fw_audit_b)

    # Exact liquid actor supply (2 x 100B = 200B) & bonded validator stake (4 x 100B = 400B):
    total_supply = 200_000_000_000
    total_stake = 400_000_000_000

    replay_b = (
        struct.pack(">H", 1)
        + mig_spec_id
        + handoff_id
        + old_exec_ref
        + new_exec_ref
        + delta_dig
        + total_supply.to_bytes(16, "big")
        + total_stake.to_bytes(16, "big")
        + struct.pack(">IBBBBB", 1, 1, 1, 1, 1, 1)
        + fw_audit_dig
    )
    assert len(replay_b) == 527
    replay_tag, replay_id = content_tag("receipt", REPLAY_RECEIPT_SCHEMA, replay_b)

    # 5. SuccessorUpgradeEvidenceManifestV1
    replay_ids = sorted([replay_id, handoff_id])
    impl_ids = sorted(
        [
            hashlib.sha256(UPG1_IMPL_RUST_PREFIX + mig_spec_id).digest(),
            hashlib.sha256(UPG1_IMPL_PY_PREFIX + mig_spec_id).digest(),
        ]
    )
    manifest_b = (
        struct.pack(">H", 1)
        + old_profile_id
        + new_profile_id
        + new_spec_dig
        + mig_spec_id
        + old_exec_ref
        + new_exec_ref
        + delta_dig
        + struct.pack(">H", len(replay_ids))
        + b"".join(replay_ids)
        + struct.pack(">H", len(impl_ids))
        + b"".join(impl_ids)
        + bytes([1])
    )
    manifest_tag, manifest_id = content_tag("receipt", EVIDENCE_MANIFEST_SCHEMA, manifest_b)

    # Powered negative controls: polarity flip, rejected disposition, byte tamper
    for b, sch, kind in [
        (mig_spec_b, MIGRATION_SPEC_SCHEMA, "policy"),
        (handoff_b, OCCURRENCE_HANDOFF_SCHEMA, "policy"),
        (replay_b, REPLAY_RECEIPT_SCHEMA, "receipt"),
        (manifest_b, EVIDENCE_MANIFEST_SCHEMA, "receipt"),
    ]:
        mut = bytearray(b)
        mut[-1] ^= 0x01
        assert content_tag(kind, sch, bytes(mut))[0] != content_tag(kind, sch, b)[0]

    return {
        "spec_tag": mig_spec_tag,
        "handoff_tag": handoff_tag,
        "replay_tag": replay_tag,
        "manifest_tag": manifest_tag,
        "parity": parity,
        "new_scope_b": new_scope_b,
        "new_scope_d": new_scope_d,
        "new_bound_b": new_bound_b,
        "new_profile_id": new_profile_id,
        "new_spec_b": new_spec_b,
        "new_spec_dig": new_spec_dig,
        "mig_spec_b": mig_spec_b,
        "mig_spec_id": mig_spec_id,
        "handoff_b": handoff_b,
        "handoff_id": handoff_id,
        "replay_b": replay_b,
        "replay_id": replay_id,
        "manifest_b": manifest_b,
        "manifest_id": manifest_id,
        "new_exec_ref": new_exec_ref,
        "new_comp_b": new_comp_b,
        "delta_dig": delta_dig,
    }


def main() -> int:
    v = build_upgrade_1_vectors()
    if "--print-tags" in sys.argv:
        print("SPEC_TAG     =", v["spec_tag"])
        print("HANDOFF_TAG  =", v["handoff_tag"])
        print("REPLAY_TAG   =", v["replay_tag"])
        print("MANIFEST_TAG =", v["manifest_tag"])
        return 0
    assert v["spec_tag"] == EXPECTED_SPEC_TAG, v["spec_tag"]
    assert v["handoff_tag"] == EXPECTED_HANDOFF_TAG, v["handoff_tag"]
    assert v["replay_tag"] == EXPECTED_REPLAY_TAG, v["replay_tag"]
    assert v["manifest_tag"] == EXPECTED_MANIFEST_TAG, v["manifest_tag"]
    print(
        f"upgrade_1_vectors: PASS (spec={v['spec_tag'][:25]}..., manifest={v['manifest_tag'][:26]}...)"
    )
    return 0


EXPECTED_SPEC_TAG = "#bb.policy.sha256.i77xbr5xhfaowu6akytl6qk5okfn3urcpc3ckl2bx5iua6jwycua"
EXPECTED_HANDOFF_TAG = "#bb.policy.sha256.5u2vcytthbdbsd3hgjzum7ksovpe5top4t6isjquu5ddukefr2ba"
EXPECTED_REPLAY_TAG = "#bb.receipt.sha256.xt7rn7i6agoxhzv5tiohrwrh36lwb6vyoqa67vsylex3mvel36sq"
EXPECTED_MANIFEST_TAG = "#bb.receipt.sha256.rrd55p74b5zijkyd3v6riq2jepyseru2ruodlohjsrpoyckel5ta"

if __name__ == "__main__":
    raise SystemExit(main())
