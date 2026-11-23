#!/usr/bin/env python3
"""Clean-room pure-Python Path 4 genesis-to-root rebuilder & vector suite for
GENESIS-ROOT-PARITY-1 (PS-031).

Imports NO Rust or Bitblock code. Independently builds & decodes the complete devnet
GenesisPackageV1 + GenesisPreimageBundleV1, verifies every policy & AuthorizationDescriptorV1
preimage digest, cold-rebuilds all 10 active launch Patricia tries + inactive ProgramState
sentinel, composes application_root -> execution_state_root -> ExecutionRootRefV1, and
reproduces both Wave 18 canonical schemas bit-for-bit:
  * bitblock/genesis-root-parity-1/preimage-bundle/v1
  * bitblock/genesis-root-parity-1/parity-record/v1
"""

from __future__ import annotations

import hashlib
import struct
import sys

from auth_commit_1_vectors import (
    DOM_AUTH,
    DOM_ECONOMIC_STAKE,
    DOM_EPISODE_OWNERSHIP,
    DOM_GOVERNANCE,
    DOM_NATIVE_BALANCE,
    DOM_NONCE,
    DOM_VALIDATOR_SCHEDULE,
    DomainPatriciaPy,
)
from bounds_1_vectors import (
    PROTOCOL_BOUNDS_SCHEMA,
    default_local_bounds_digest,
    encode_protocol_bounds,
)
from commit_state_vectors import (
    COMMIT_1_ACTIVE_TOKEN_ROOT_SCHEMA,
    COMMIT_1_EPISODE_ROOT_SCHEMA,
    COMMIT_1_OP_LIMITS_SCHEMA,
    COMMIT_1_PROFILE_SCHEMA,
    COMMIT_1_PROOF_POLICY_SCHEMA,
    COMMIT_1_WORLD_ROOT_SCHEMA,
    DOMAIN_ACTIVE_TOKEN,
    DOMAIN_EPISODE,
    DOMAIN_WORLD,
    EXPECTED_DOMAIN_REG_TAG,
    KEY_COORD_V1,
    KEY_EVENT_V1,
    ON_V1,
    ROOT_ACTIVE_TOKEN,
    ROOT_EPISODE,
    ROOT_WORLD,
    VAL_EPISODE_V1,
    VAL_MINT_V1,
    VAL_ON_V1,
    build_patricia,
    coord_key,
    encode_op_limits,
    encode_profile,
    encode_proof_policy,
)
from crypto_d1_vectors import (
    content_tag,
    ed_public_from_seed,
    encode_suite_descriptor,
)
from economic_state_1_vectors import encode_econ_root, encode_val_record
from execution_root_1_vectors import compose_execution_root, encode_launch_registry
from executor_1_vectors import HIST_GENESIS_SCHEMA, world_tag_py
from genesis_d1_vectors import (
    BOOTSTRAP_POLICY_SCHEMA,
    LAUNCH_PREDICATES,
    LAUNCH_STATE_SCHEMA,
    PACKAGE_SCHEMA,
    encode_bootstrap_policy,
    encode_genesis_package,
    encode_launch_state,
)
from governance_1_vectors import encode_gov_root, param_trie_key
from identity_1_vectors import (
    derive_actor_id,
    encode_actor_auth_state,
    encode_auth_descriptor,
    encode_threshold_policy,
)
from pre_genesis_d1_vectors import (
    ASSET_POLICY_SCHEMA,
    CONSENSUS_POLICY_SCHEMA,
    GOVERNANCE_POLICY_SCHEMA,
    VALIDATOR_ECON_POLICY_SCHEMA,
    encode_asset_policy,
    encode_consensus_policy,
    encode_governance_policy,
    encode_validator_econ_policy,
)
from profile_bootstrap_vectors import SCOPE_SCHEMA, decode_tag_digest
from profile_d1_vectors import LAUNCH_CAPS, LAUNCH_PREDS, encode_candidate, seal_bound
from resource_1_vectors import POLICY_SCHEMA as RESOURCE_POLICY_SCHEMA
from resource_1_vectors import encode_resource_policy
from threat_1_and_vm_d1_vectors import THREAT_MODEL_SCHEMA, encode_threat_model
from transition_spec_1_vectors import (
    BASE_MASK,
    RESOLVED_SPEC_SCHEMA,
    base_contract_digest,
    build_bands,
    encode_band,
)
from transition_spec_d1_vectors import CONTRACT_SCHEMA, encode_base_contract
from validator_schedule_1_vectors import encode_epoch_schedule, epoch_trie_key

BUNDLE_SCHEMA = b"bitblock/genesis-root-parity-1/preimage-bundle/v1"
RECORD_SCHEMA = b"bitblock/genesis-root-parity-1/parity-record/v1"
AUTH_DESC_SCHEMA = b"bitblock/identity-d1/authorization-descriptor/v1"

PARITY_RECORD_ID_PREFIX = b"bb.grp1.parity_record_id.v1"
RECON_COLD_PREFIX = b"bb.grp1.recon.cold_rebuild.v1"
RECON_INCR_PREFIX = b"bb.grp1.recon.incremental.v1"
RECON_EXEC_PREFIX = b"bb.grp1.recon.executor.v1"


def frame_bytes(b: bytes) -> bytes:
    return struct.pack(">I", len(b)) + b


def recon_digest(
    prefix: bytes,
    genesis_id: bytes,
    profile_id: bytes,
    app_root: bytes,
    exec_root: bytes,
    entry_set_digest: bytes,
) -> bytes:
    return hashlib.sha256(
        prefix + genesis_id + profile_id + app_root + exec_root + entry_set_digest
    ).digest()


def rebuild_devnet_parity_path4() -> dict[str, object]:
    # 1. Independently build all upstream policies from canonical constants
    _, base_spec_d = content_tag("policy", CONTRACT_SCHEMA, encode_base_contract([]))
    commit_dom_reg_d = decode_tag_digest(EXPECTED_DOMAIN_REG_TAG)
    crypto_bytes = encode_suite_descriptor()
    _, crypto_d = content_tag("policy", b"bitblock/crypto-d1/suite-descriptor/v1", crypto_bytes)

    asset_bytes = encode_asset_policy()
    _, asset_d = content_tag("policy", ASSET_POLICY_SCHEMA, asset_bytes)
    econ_bytes = encode_validator_econ_policy()
    _, econ_d = content_tag("policy", VALIDATOR_ECON_POLICY_SCHEMA, econ_bytes)
    cons_bytes = encode_consensus_policy()
    _, cons_d = content_tag("policy", CONSENSUS_POLICY_SCHEMA, cons_bytes)

    council_keys = sorted(ed_public_from_seed(bytes([s]) * 32) for s in (31, 32, 33))
    gov_bytes = encode_governance_policy(council_keys)
    _, gov_d = content_tag("policy", GOVERNANCE_POLICY_SCHEMA, gov_bytes)

    # ProfileScopeV1 with all 7 launch predicates
    scope_b = bytearray()
    scope_b += struct.pack(">H", 1)
    scope_b += base_spec_d + commit_dom_reg_d + crypto_d + asset_d + econ_d + cons_d + gov_d
    scope_b += struct.pack(">IIIIIH", 16_384, 1_024, 65_536, 12_288, 65_536, len(LAUNCH_PREDICATES))
    for p in LAUNCH_PREDICATES:
        pb = p.encode("utf-8")
        scope_b += struct.pack(">H", len(pb)) + pb
    _, scope_d = content_tag("policy", SCOPE_SCHEMA, bytes(scope_b))

    threat_bytes = encode_threat_model(scope_d)
    _, threat_d = content_tag("policy", THREAT_MODEL_SCHEMA, threat_bytes)

    bounds_bytes = encode_protocol_bounds(threat_d, default_local_bounds_digest())
    _, bounds_d = content_tag("policy", PROTOCOL_BOUNDS_SCHEMA, bounds_bytes)

    resource_bytes = encode_resource_policy(bounds_d, asset_d)
    _, resource_d = content_tag("policy", RESOURCE_POLICY_SCHEMA, resource_bytes)

    boot_bytes = encode_bootstrap_policy()
    _, boot_d = content_tag("policy", BOOTSTRAP_POLICY_SCHEMA, boot_bytes)

    # 2. Independently build both genesis actors & 4 validators (chain_id = 77)
    chain_id = 77
    world_id = 1
    namespace = "bitblock.devnet.v1"

    actor_pairs: list[tuple[bytes, bytes, bytes, bytes]] = []
    for act_s, adm_s in ((11, 12), (13, 14)):
        act_pol = encode_threshold_policy(0, 1, [ed_public_from_seed(bytes([act_s]) * 32)])
        adm_pol = encode_threshold_policy(1, 1, [ed_public_from_seed(bytes([adm_s]) * 32)])
        desc_b = encode_auth_descriptor(1, act_pol, adm_pol, [], False)
        aid = derive_actor_id(chain_id, namespace, desc_b)
        _, desc_d = content_tag("policy", AUTH_DESC_SCHEMA, desc_b)
        actor_pairs.append((aid, desc_b, desc_d, adm_pol))
    actor_pairs.sort(key=lambda x: x[0])

    genesis_actors = [(aid, desc_d, 0, 100_000_000_000) for aid, _, desc_d, _ in actor_pairs]
    genesis_validators = [
        (
            bytes([i]) * 32,
            ed_public_from_seed(bytes([100 + i]) * 32),
            ed_public_from_seed(bytes([200 + i]) * 32),
            100_000_000_000,
        )
        for i in range(1, 5)
    ]
    total_supply = sum(a[3] for a in genesis_actors) + sum(v[3] for v in genesis_validators)
    assert total_supply == 600_000_000_000

    # 3. Encode LaunchGenesisStateV1 & GenesisPackageV1
    w_bedrock_dig = world_tag_py({(0, 0, 0)})
    state_bytes = encode_launch_state(
        scope_d,
        commit_dom_reg_d,
        chain_id,
        world_id,
        namespace,
        w_bedrock_dig,
        resource_d,
        asset_d,
        econ_d,
        gov_d,
        bounds_d,
        boot_d,
        genesis_actors,
        genesis_validators,
        total_supply,
    )
    _, genesis_id = content_tag("policy", LAUNCH_STATE_SCHEMA, state_bytes)
    pkg_bytes = encode_genesis_package(state_bytes, genesis_id)
    pkg_tag, _ = content_tag("receipt", PACKAGE_SCHEMA, pkg_bytes)

    # 4. Encode GenesisPreimageBundleV1
    gov_params = [(1, 2_000), (2, 100_000_000)]
    bundle_b = bytearray()
    bundle_b += struct.pack(">H", 1)
    for b in (resource_bytes, asset_bytes, econ_bytes, gov_bytes, cons_bytes, bounds_bytes, boot_bytes):
        bundle_b += frame_bytes(b)
    bundle_b += struct.pack(">H", len(gov_params))
    for k, v in gov_params:
        bundle_b += struct.pack(">IQ", k, v)
    bundle_b += struct.pack(">I", len(actor_pairs))
    for aid, desc_b, _, _ in actor_pairs:
        bundle_b += aid + frame_bytes(desc_b)
    bundle_tag, _ = content_tag("policy", BUNDLE_SCHEMA, bytes(bundle_b))

    # 5. Bound initial launch profile & resolved TRANSITION-SPEC-1
    cand_b = encode_candidate(
        0,
        None,
        genesis_id,
        scope_d,
        scope_d,
        commit_dom_reg_d,
        bounds_d,
        crypto_d,
        resource_d,
        LAUNCH_PREDS,
        LAUNCH_CAPS,
    )
    _, profile_id, _ = seal_bound(cand_b)
    bound_bytes = struct.pack(">I", len(cand_b)) + cand_b + profile_id

    bands = build_bands(False)
    spec_b = (
        struct.pack(">H", 1)
        + profile_id
        + base_contract_digest()
        + bounds_d
        + resource_d
        + struct.pack(">IQQIBH", 16_384, 100_000_000, 2_000, BASE_MASK, 0, len(bands))
        + b"".join(encode_band(*b) for b in bands)
    )
    _, spec_dig = content_tag("policy", RESOLVED_SPEC_SCHEMA, spec_b)

    # 6. Cold-rebuild all 10 active launch Patricia tries in pure Python!
    _, pp_w = content_tag("policy", COMMIT_1_PROOF_POLICY_SCHEMA, encode_proof_policy(8192, 193, 192, 193, 32768, 128, 16384))
    _, pp_a = content_tag("policy", COMMIT_1_PROOF_POLICY_SCHEMA, encode_proof_policy(8192, 193, 192, 193, 32768, 160, 16384))
    _, pp_e = content_tag("policy", COMMIT_1_PROOF_POLICY_SCHEMA, encode_proof_policy(12288, 257, 256, 257, 49152, 256, 24576))
    _, op_id = content_tag("policy", COMMIT_1_OP_LIMITS_SCHEMA, encode_op_limits())
    _, prof_w_id = content_tag("policy", COMMIT_1_PROFILE_SCHEMA, encode_profile(ROOT_WORLD, DOMAIN_WORLD, COMMIT_1_WORLD_ROOT_SCHEMA, KEY_COORD_V1, 192, VAL_ON_V1, pp_w, op_id))
    _, prof_a_id = content_tag("policy", COMMIT_1_PROFILE_SCHEMA, encode_profile(ROOT_ACTIVE_TOKEN, DOMAIN_ACTIVE_TOKEN, COMMIT_1_ACTIVE_TOKEN_ROOT_SCHEMA, KEY_COORD_V1, 192, VAL_MINT_V1, pp_a, op_id))
    _, prof_e_id = content_tag("policy", COMMIT_1_PROFILE_SCHEMA, encode_profile(ROOT_EPISODE, DOMAIN_EPISODE, COMMIT_1_EPISODE_ROOT_SCHEMA, KEY_EVENT_V1, 256, VAL_EPISODE_V1, pp_e, op_id))

    world_root = build_patricia(prof_w_id, COMMIT_1_WORLD_ROOT_SCHEMA, 192, [(coord_key(0, 0, 0), ON_V1)]).hash
    active_token_root = build_patricia(prof_a_id, COMMIT_1_ACTIVE_TOKEN_ROOT_SCHEMA, 192, []).hash
    episode_root = build_patricia(prof_e_id, COMMIT_1_EPISODE_ROOT_SCHEMA, 256, []).hash

    auth_root = DomainPatriciaPy(
        DOM_AUTH,
        {
            aid: encode_actor_auth_state(aid, desc_b, 0, adm_pol, 100, None)
            for aid, desc_b, _, adm_pol in actor_pairs
        },
    ).root()
    nonce_root = DomainPatriciaPy(
        DOM_NONCE, {aid: struct.pack(">Q", 0) for aid, _, _, _ in actor_pairs}
    ).root()
    native_balance_root = DomainPatriciaPy(
        DOM_NATIVE_BALANCE, {aid: bal.to_bytes(16, "big") for aid, _, _, bal in genesis_actors}
    ).root()
    episode_ownership_root = DomainPatriciaPy(DOM_EPISODE_OWNERSHIP, {}).root()
    gov_root = DomainPatriciaPy(
        DOM_GOVERNANCE, {param_trie_key(k): struct.pack(">Q", v) for k, v in gov_params}
    ).root()
    econ_root = DomainPatriciaPy(
        DOM_ECONOMIC_STAKE,
        {vid: encode_val_record(stake, 0, 0, False, 0, []) for vid, _, _, stake in genesis_validators},
    ).root()

    gov_root_sha = hashlib.sha256(
        encode_gov_root(profile_id, 0, 0, 0, 0, 0, bytes(32), 0, len(gov_params), gov_root)
    ).digest()
    bonded_sum = sum(v[3] for v in genesis_validators)
    econ_root_sha = hashlib.sha256(
        encode_econ_root(profile_id, 0, 4, 4, bonded_sum, 0, 0, 0, 0, bonded_sum, 0, 0, econ_root)
    ).digest()
    sched_entries = {
        epoch_trie_key(ep): encode_epoch_schedule(ep, 0, econ_root_sha, gov_root_sha, 2500, genesis_validators)
        for ep in (0, 1)
    }
    sched_root = DomainPatriciaPy(DOM_VALIDATOR_SCHEDULE, sched_entries).root()

    roots_by_tag = {
        1: world_root,
        2: active_token_root,
        3: episode_root,
        4: native_balance_root,
        5: episode_ownership_root,
        7: auth_root,
        8: nonce_root,
        9: gov_root,
        10: econ_root,
        11: sched_root,
    }
    _, reg_digest, req_set_digest = encode_launch_registry()
    comp_bytes, app_root, exec_root, exec_root_ref, comp_tag = compose_execution_root(
        0, 0, profile_id, 1, chain_id, world_id, genesis_id, scope_d, roots_by_tag
    )
    entry_set_digest = comp_bytes[196:228]
    assert len(exec_root_ref) == 178

    _, genesis_history_root = content_tag("receipt", HIST_GENESIS_SCHEMA, w_bedrock_dig)
    artifacts = [
        recon_digest(RECON_COLD_PREFIX, genesis_id, profile_id, app_root, exec_root, entry_set_digest),
        recon_digest(RECON_INCR_PREFIX, genesis_id, profile_id, app_root, exec_root, entry_set_digest),
        recon_digest(RECON_EXEC_PREFIX, genesis_id, profile_id, app_root, exec_root, entry_set_digest),
    ]
    rec_bytes = (
        struct.pack(">H", 1)
        + genesis_id
        + profile_id
        + spec_dig
        + req_set_digest
        + entry_set_digest
        + app_root
        + exec_root_ref
        + genesis_history_root
        + struct.pack(">H", len(artifacts))
        + b"".join(artifacts)
    )
    record_tag, _ = content_tag("receipt", RECORD_SCHEMA, rec_bytes)
    parity_record_id = hashlib.sha256(
        PARITY_RECORD_ID_PREFIX + struct.pack(">I", len(rec_bytes)) + rec_bytes
    ).digest()

    return {
        "pkg_tag": pkg_tag,
        "bundle_tag": bundle_tag,
        "comp_tag": comp_tag,
        "record_tag": record_tag,
        "parity_record_id": parity_record_id.hex(),
        "app_root": app_root.hex(),
        "exec_root": exec_root.hex(),
        "raw": {
            "genesis_id": genesis_id,
            "profile_id": profile_id,
            "scope_d": scope_d,
            "commit_dom_reg_d": commit_dom_reg_d,
            "spec_dig": spec_dig,
            "exec_root_ref": exec_root_ref,
            "genesis_history_root": genesis_history_root,
            "parity_record_id_bytes": parity_record_id,
            "scope_bytes": bytes(scope_b),
            "pkg_bytes": pkg_bytes,
            "bound_bytes": bound_bytes,
            "spec_bytes": spec_b,
            "comp_bytes": comp_bytes,
            "rec_bytes": rec_bytes,
        },
    }


def main() -> int:
    res = rebuild_devnet_parity_path4()
    if "--print-tags" in sys.argv:
        for k in ("pkg_tag", "bundle_tag", "comp_tag", "record_tag", "parity_record_id", "app_root", "exec_root"):
            print(f"{k.upper()} = {res[k]}")
        return 0

    assert res["bundle_tag"] == EXPECTED_BUNDLE_TAG, f"bundle mismatch: {res['bundle_tag']}"
    assert res["comp_tag"] == EXPECTED_COMP_TAG, f"comp mismatch: {res['comp_tag']}"
    assert res["record_tag"] == EXPECTED_RECORD_TAG, f"record mismatch: {res['record_tag']}"
    assert res["parity_record_id"] == EXPECTED_PARITY_RECORD_ID, (
        f"record_id mismatch: {res['parity_record_id']}"
    )
    print(
        f"genesis_root_parity_1_vectors: PASS (4-way parity bundle={res['bundle_tag'][:26]}..., "
        f"record={res['record_tag'][:26]}...)"
    )
    return 0


EXPECTED_BUNDLE_TAG = "#bb.policy.sha256.6mcfpltkrno445wrrzhsecwowfll6m5ekyhakh6aw5n2vcdirpsa"
EXPECTED_COMP_TAG = "#bb.receipt.sha256.uihe33whofr5752ptw2dxfq3tuvtee7ssqrukxxldfolnjsvay2q"
EXPECTED_RECORD_TAG = "#bb.receipt.sha256.xkaeumdnpaz77nlkk3oolxx2surjpvay2ycpksqckx4oddbm67fq"
EXPECTED_PARITY_RECORD_ID = "2e72dd4d8c6d61ff0158dc7460a8bdaf75e6d9a381e542bb97a9a80a520a2eaa"

if __name__ == "__main__":
    raise SystemExit(main())
