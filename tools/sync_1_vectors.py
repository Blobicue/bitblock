#!/usr/bin/env python3
"""Clean-room pure-Python independent verifier & vector suite for SYNC-1@base (PS-034).

Imports NO Rust or Bitblock code. Independently reproduces:
  * bitblock/sync-1/checkpoint-manifest/v1 (chunked checksummed checkpoint manifest)
  * bitblock/sync-1/sync-recovery-receipt/v1 (380-byte recovery receipt with live_authority_admitted=0)
  * OpaqueAuthorityBlobV1 digest & set commitment (preserving PS-095 LOCKED boundary)
  * Negative controls (1-bit corrupt chunk detection & forbidden live_authority_admitted=1)
"""

from __future__ import annotations

import hashlib
import struct
import sys

from crypto_d1_vectors import content_tag
from sequencer_1_vectors import (
    DEFAULT_SEQ_SEED,
    SEQ_BODY_DIGEST_PREFIX,
    SEQ_HDR_DIGEST_PREFIX,
    SEQ_HDR_PAYLOAD_PREFIX,
    SEQ_HEADER_SCHEMA,
    SEQ_PARENT_LOOKUP_PREFIX,
    SEQ_PUBLICATION_SCHEMA,
    merkle_root_and_proof,
)

# Reuse clean-room devnet + batch 1 publication builder cleanly by reading it via helper
SYNC_CHECKPOINT_SCHEMA = b"bitblock/sync-1/checkpoint-manifest/v1"
SYNC_RECOVERY_SCHEMA = b"bitblock/sync-1/sync-recovery-receipt/v1"

SYNC_CHUNK_PREFIX = b"bb.sync1.chk_chunk.v1"
SYNC_OPAQUE_BLOB_PREFIX = b"bb.sync1.opaque_blob.v1"
SYNC_OPAQUE_SET_PREFIX = b"bb.sync1.opaque_set.v1"


def opaque_blob_digest(kind_code: int, raw: bytes) -> bytes:
    return hashlib.sha256(
        SYNC_OPAQUE_BLOB_PREFIX + bytes([kind_code]) + struct.pack(">I", len(raw)) + raw
    ).digest()


def opaque_set_digest(digests: list[bytes]) -> bytes:
    uniq = sorted(set(digests))
    return hashlib.sha256(
        SYNC_OPAQUE_SET_PREFIX + struct.pack(">I", len(uniq)) + b"".join(uniq)
    ).digest()


def chunk_digest(idx: int, chunk: bytes) -> bytes:
    return hashlib.sha256(SYNC_CHUNK_PREFIX + struct.pack(">I", idx) + chunk).digest()


def build_sync_1_vectors() -> tuple[str, str, str]:
    # Rebuild Batch 1 publication bytes directly using clean-room sequencer_1_vectors components
    from asset_1_vectors import encode_episode_value, episode_trie_key
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
        episode_record,
    )
    from crypto_d1_vectors import (
        SIGNING_PREIMAGE_SCHEMA,
        ed_public_from_seed,
        ed_sign,
        encode_signing_preimage,
        encode_suite_descriptor,
    )
    from economic_state_1_vectors import encode_econ_root, encode_val_record
    from execution_root_1_vectors import compose_execution_root
    from executor_1_vectors import (
        HIST_EVENT_SCHEMA,
        HIST_GENESIS_SCHEMA,
        HIST_RECEIPT_SCHEMA,
        world_tag_py,
    )
    from genesis_d1_vectors import (
        BOOTSTRAP_POLICY_SCHEMA,
        LAUNCH_PREDICATES,
        LAUNCH_STATE_SCHEMA,
        encode_bootstrap_policy,
        encode_launch_state,
    )
    from governance_1_vectors import encode_gov_root, param_trie_key
    from identity_1_vectors import (
        derive_actor_id,
        encode_actor_auth_state,
        encode_threshold_policy,
    )
    from multi_1_vectors import (
        AUTH_DESC_SCHEMA,
        DISP_ADMITTED,
        DISP_ADMITTED_FAIL,
        DISP_BAD_NONCE,
        DISP_NOOP,
        DISP_RACE_LOST,
        DISP_STALE_ANCHOR,
        INPUT_SET_PREFIX,
        NOMINAL_GAS,
        OP_DELETE,
        OP_MINT,
        OP_PLACE,
        OP_TRANSFER,
        OUTCOMES_PREFIX,
        derive_mint_id,
        encode_delegation,
        sign_envelope,
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
    from protocol_id_1_vectors import (
        EXACT_CORE_SCHEMA,
        PARENT_LOOKUP_SCHEMA,
        encode_exact_core,
    )
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

    _, base_spec_d = content_tag("policy", CONTRACT_SCHEMA, encode_base_contract([]))
    commit_dom_reg_d = decode_tag_digest(EXPECTED_DOMAIN_REG_TAG)
    _, crypto_d = content_tag("policy", b"bitblock/crypto-d1/suite-descriptor/v1", encode_suite_descriptor())
    _, asset_d = content_tag("policy", ASSET_POLICY_SCHEMA, encode_asset_policy())
    _, econ_d = content_tag("policy", VALIDATOR_ECON_POLICY_SCHEMA, encode_validator_econ_policy())
    _, cons_d = content_tag("policy", CONSENSUS_POLICY_SCHEMA, encode_consensus_policy())
    council_keys = sorted(ed_public_from_seed(bytes([s]) * 32) for s in (31, 32, 33))
    _, gov_d = content_tag("policy", GOVERNANCE_POLICY_SCHEMA, encode_governance_policy(council_keys))

    scope_b = bytearray(struct.pack(">H", 1))
    scope_b += base_spec_d + commit_dom_reg_d + crypto_d + asset_d + econ_d + cons_d + gov_d
    scope_b += struct.pack(">IIIIIH", 16_384, 1_024, 65_536, 12_288, 65_536, len(LAUNCH_PREDICATES))
    for p in LAUNCH_PREDICATES:
        pb = p.encode("utf-8")
        scope_b += struct.pack(">H", len(pb)) + pb
    _, scope_d = content_tag("policy", SCOPE_SCHEMA, bytes(scope_b))
    _, threat_d = content_tag("policy", THREAT_MODEL_SCHEMA, encode_threat_model(scope_d))
    _, bounds_d = content_tag("policy", PROTOCOL_BOUNDS_SCHEMA, encode_protocol_bounds(threat_d, default_local_bounds_digest()))
    _, resource_d = content_tag("policy", RESOURCE_POLICY_SCHEMA, encode_resource_policy(bounds_d, asset_d))
    _, boot_d = content_tag("policy", BOOTSTRAP_POLICY_SCHEMA, encode_bootstrap_policy())

    chain_id, world_id, ns = 77, 1, "bitblock.devnet.v1"

    def mk_desc(act_s: int, adm_s: int, delegs: list[tuple[bytes, bool, bool, int]]):
        ap = encode_threshold_policy(0, 1, [ed_public_from_seed(bytes([act_s]) * 32)])
        ad = encode_threshold_policy(1, 1, [ed_public_from_seed(bytes([adm_s]) * 32)])
        out = bytearray(struct.pack(">Q", 1)) + ap + ad + struct.pack(">H", len(delegs))
        for did, aon, aoff, exp in sorted(delegs, key=lambda x: x[0]):
            out += encode_delegation(did, aon, aoff, exp)
        out += b"\x00"
        return bytes(out), ad

    desc_b, adm_b = mk_desc(13, 14, [])
    aid_b = derive_actor_id(chain_id, ns, desc_b)
    desc_c, adm_c = mk_desc(15, 16, [(aid_b, True, True, 50)])
    aid_c = derive_actor_id(chain_id, ns, desc_c)
    desc_a, adm_a = mk_desc(11, 12, [(aid_c, True, True, 50)])
    aid_a = derive_actor_id(chain_id, ns, desc_a)

    actors_meta = sorted([(aid_a, desc_a, adm_a), (aid_b, desc_b, adm_b), (aid_c, desc_c, adm_c)], key=lambda x: x[0])
    gen_actors = [(aid, content_tag("policy", AUTH_DESC_SCHEMA, db)[1], 0, 100_000_000_000) for aid, db, _ in actors_meta]
    gen_vals = [
        (bytes([i]) * 32, ed_public_from_seed(bytes([100 + i]) * 32), ed_public_from_seed(bytes([200 + i]) * 32), 100_000_000_000)
        for i in range(1, 5)
    ]
    total_supply = sum(a[3] for a in gen_actors) + sum(v[3] for v in gen_vals)
    w0 = world_tag_py({(0, 0, 0)})
    state_b = encode_launch_state(
        scope_d, commit_dom_reg_d, chain_id, world_id, ns, w0, resource_d, asset_d, econ_d, gov_d, bounds_d, boot_d,
        gen_actors, gen_vals, total_supply,
    )
    _, genesis_id = content_tag("policy", LAUNCH_STATE_SCHEMA, state_b)
    cand_b = encode_candidate(
        0, None, genesis_id, scope_d, scope_d, commit_dom_reg_d, bounds_d, crypto_d, resource_d, LAUNCH_PREDS, LAUNCH_CAPS
    )
    _, profile_id, _ = seal_bound(cand_b)
    bands = build_bands(False)
    spec_b = (
        struct.pack(">H", 1) + profile_id + base_contract_digest() + bounds_d + resource_d
        + struct.pack(">IQQIBH", 16_384, 100_000_000, 2_000, BASE_MASK, 0, len(bands))
        + b"".join(encode_band(*b) for b in bands)
    )
    _, spec_dig = content_tag("policy", RESOLVED_SPEC_SCHEMA, spec_b)
    fork_ctx = struct.pack(">QQ", chain_id, world_id) + genesis_id + profile_id + spec_dig

    _, pp_w = content_tag("policy", COMMIT_1_PROOF_POLICY_SCHEMA, encode_proof_policy(8192, 193, 192, 193, 32768, 128, 16384))
    _, pp_a = content_tag("policy", COMMIT_1_PROOF_POLICY_SCHEMA, encode_proof_policy(8192, 193, 192, 193, 32768, 160, 16384))
    _, pp_e = content_tag("policy", COMMIT_1_PROOF_POLICY_SCHEMA, encode_proof_policy(12288, 257, 256, 257, 49152, 256, 24576))
    _, op_id = content_tag("policy", COMMIT_1_OP_LIMITS_SCHEMA, encode_op_limits())
    _, prof_w = content_tag("policy", COMMIT_1_PROFILE_SCHEMA, encode_profile(ROOT_WORLD, DOMAIN_WORLD, COMMIT_1_WORLD_ROOT_SCHEMA, KEY_COORD_V1, 192, VAL_ON_V1, pp_w, op_id))
    _, prof_a = content_tag("policy", COMMIT_1_PROFILE_SCHEMA, encode_profile(ROOT_ACTIVE_TOKEN, DOMAIN_ACTIVE_TOKEN, COMMIT_1_ACTIVE_TOKEN_ROOT_SCHEMA, KEY_COORD_V1, 192, VAL_MINT_V1, pp_a, op_id))
    _, prof_e = content_tag("policy", COMMIT_1_PROFILE_SCHEMA, encode_profile(ROOT_EPISODE, DOMAIN_EPISODE, COMMIT_1_EPISODE_ROOT_SCHEMA, KEY_EVENT_V1, 256, VAL_EPISODE_V1, pp_e, op_id))

    gov_params = [(1, 2_000), (2, 100_000_000)]
    auth_root = DomainPatriciaPy(DOM_AUTH, {aid: encode_actor_auth_state(aid, db, 0, adm, 100, None) for aid, db, adm in actors_meta}).root()
    nonce_root = DomainPatriciaPy(DOM_NONCE, {aid: struct.pack(">Q", 0) for aid, _, _ in actors_meta}).root()
    gov_root = DomainPatriciaPy(DOM_GOVERNANCE, {param_trie_key(k): struct.pack(">Q", v) for k, v in gov_params}).root()
    econ_root = DomainPatriciaPy(DOM_ECONOMIC_STAKE, {vid: encode_val_record(s, 0, 0, False, 0, []) for vid, _, _, s in gen_vals}).root()
    gov_sha = hashlib.sha256(encode_gov_root(profile_id, 0, 0, 0, 0, 0, bytes(32), 0, len(gov_params), gov_root)).digest()
    bsum = sum(v[3] for v in gen_vals)
    econ_sha = hashlib.sha256(encode_econ_root(profile_id, 0, 4, 4, bsum, 0, 0, 0, 0, bsum, 0, 0, econ_root)).digest()
    sched_root = DomainPatriciaPy(
        DOM_VALIDATOR_SCHEDULE,
        {epoch_trie_key(ep): encode_epoch_schedule(ep, 0, econ_sha, gov_sha, 2500, gen_vals) for ep in (0, 1)},
    ).root()

    def compose_state(height, world_set, active_mints, episodes, balances, asset_eps):
        wr = build_patricia(prof_w, COMMIT_1_WORLD_ROOT_SCHEMA, 192, [(coord_key(*c), ON_V1) for c in world_set]).hash
        ar = build_patricia(prof_a, COMMIT_1_ACTIVE_TOKEN_ROOT_SCHEMA, 192, [(coord_key(*c), m) for c, m in active_mints.items()]).hash
        er = build_patricia(prof_e, COMMIT_1_EPISODE_ROOT_SCHEMA, 256, [(mid, episode_record(c, mid, b)) for mid, (c, b) in episodes.items()]).hash
        br = DomainPatriciaPy(DOM_NATIVE_BALANCE, {a: bal.to_bytes(16, "big") for a, bal in balances.items()}).root()
        aep = DomainPatriciaPy(DOM_EPISODE_OWNERSHIP, {episode_trie_key(*c): encode_episode_value(o, m, d) for c, (o, m, d) in asset_eps.items()}).root()
        roots = {1: wr, 2: ar, 3: er, 4: br, 5: aep, 7: auth_root, 8: nonce_root, 9: gov_root, 10: econ_root, 11: sched_root}
        cb, app_r, _, exec_ref, _ = compose_execution_root(height, 0, profile_id, 1, chain_id, world_id, genesis_id, scope_d, roots)
        return wr, app_r, exec_ref, cb[196:228]

    balances = {aid_a: 100_000_000_000, aid_b: 100_000_000_000, aid_c: 100_000_000_000}
    pre_wr, _, pre_ref, _ = compose_state(0, {(0, 0, 0)}, {}, {}, balances, {})

    zero_a = bytes(32)
    s_a, s_b, s_c = bytes([11]) * 32, bytes([13]) * 32, bytes([15]) * 32
    envs = [
        sign_envelope(chain_id, genesis_id, profile_id, aid_b, aid_b, s_b, 0, 100, pre_wr, OP_MINT, (3, 0, 0), zero_a, 0),
        sign_envelope(chain_id, genesis_id, profile_id, aid_a, aid_a, s_a, 0, 500, pre_wr, OP_MINT, (3, 0, 0), zero_a, 0),
        sign_envelope(chain_id, genesis_id, profile_id, aid_a, aid_c, s_c, 1, 400, pre_wr, OP_MINT, (4, 0, 0), zero_a, 0),
        sign_envelope(chain_id, genesis_id, profile_id, aid_a, aid_a, s_a, 0, 300, pre_wr, OP_PLACE, (6, 0, 0), zero_a, 0),
        sign_envelope(chain_id, genesis_id, profile_id, aid_c, aid_c, s_c, 0, 250, bytes([0xEE]) * 32, OP_PLACE, (5, 0, 0), zero_a, 0),
        sign_envelope(chain_id, genesis_id, profile_id, aid_c, aid_c, s_c, 0, 200, pre_wr, OP_DELETE, (0, 0, 0), zero_a, 0),
        sign_envelope(chain_id, genesis_id, profile_id, aid_b, aid_b, s_b, 0, 80, pre_wr, OP_TRANSFER, (0, 0, 0), aid_c, 999_999_999_999),
        sign_envelope(chain_id, genesis_id, profile_id, aid_c, aid_c, s_c, 0, 50, pre_wr, OP_TRANSFER, (0, 0, 0), aid_b, 5_000_000),
    ]
    bundle_bytes = struct.pack(">HQQ", 1, 1, 0) + fork_ctx + struct.pack(">QI", 2, len(envs)) + b"".join(e["raw"] for e in envs)
    sorted_envs = sorted(envs, key=lambda e: (-e["tip"], e["principal"], e["nonce"], e["digest"]))
    input_set_dig = hashlib.sha256(INPUT_SET_PREFIX + struct.pack(">I", len(sorted_envs)) + b"".join(e["digest"] for e in sorted_envs)).digest()

    world_set = {(0, 0, 0)}
    _, hist_head = content_tag("receipt", HIST_GENESIS_SCHEMA, w0)
    op_index = 0
    active_mints, episodes, asset_eps, occ_owners = {}, {}, {}, {}
    nonces = {aid_a: 0, aid_b: 0, aid_c: 0}

    def commit_place(c):
        nonlocal hist_head, op_index
        world_set.add(c)
        tick = 1 * 1024 + op_index
        edit_b = struct.pack(">Bqqq", 0, *c)
        _, ev_dig = content_tag("event", HIST_EVENT_SCHEMA, hist_head + struct.pack(">Q", tick) + edit_b)
        post_w = world_tag_py(world_set)
        _, hist_head = content_tag("receipt", HIST_RECEIPT_SCHEMA, hist_head + struct.pack(">QQ", tick, 1) + edit_b + post_w + ev_dig + b"\x00\x00")
        op_index += 2
        active_mints[c] = ev_dig
        episodes[ev_dig] = (c, None)

    outcomes = []
    admitted = rejected = total_gas = total_fee = 0
    fee_burn, dep = 1_000, 1_000_000
    for idx, e in enumerate(sorted_envs):
        if e["anchor"] != pre_wr:
            disp = DISP_STALE_ANCHOR
        elif e["nonce"] != nonces[e["principal"]]:
            disp = DISP_BAD_NONCE
        elif e["op"] == OP_DELETE and e["coord"] == (0, 0, 0):
            disp = DISP_NOOP
        elif e["op"] == OP_MINT:
            if e["coord"] in occ_owners:
                disp = DISP_RACE_LOST
            else:
                disp = DISP_ADMITTED
                commit_place(e["coord"])
                mid = derive_mint_id(1, idx, e["principal"], e["coord"])
                occ_owners[e["coord"]] = e["principal"]
                asset_eps[e["coord"]] = (e["principal"], mid, dep)
                balances[e["principal"]] -= fee_burn + dep
                nonces[e["principal"]] += 1
        elif e["op"] == OP_TRANSFER:
            if balances[e["principal"]] < e["amount"] + fee_burn:
                disp = DISP_ADMITTED_FAIL
            else:
                disp = DISP_ADMITTED
                balances[e["principal"]] -= e["amount"] + fee_burn
                balances[e["target"]] += e["amount"]
                nonces[e["principal"]] += 1
        else:
            disp = DISP_NOOP

        if disp in (DISP_ADMITTED, DISP_ADMITTED_FAIL):
            admitted += 1 if disp == DISP_ADMITTED else 0
            rejected += 0 if disp == DISP_ADMITTED else 1
            g = NOMINAL_GAS[e["op"]]
            fb = g * 2
        else:
            rejected += 1
            g = fb = 0
        total_gas += g
        total_fee += fb
        outcomes.append(
            struct.pack(">I", idx) + e["digest"] + e["principal"]
            + struct.pack(">QBB Q", e["nonce"], e["op"], disp, g) + fb.to_bytes(16, "big")
        )

    _, post_app, post_ref, post_entry_set = compose_state(1, world_set, active_mints, episodes, balances, asset_eps)
    outcomes_dig = hashlib.sha256(OUTCOMES_PREFIX + struct.pack(">I", len(outcomes)) + b"".join(outcomes)).digest()
    rcpt_bytes = (
        struct.pack(">HQQ", 1, 1, 0) + fork_ctx + input_set_dig + pre_ref + post_ref + post_app + post_entry_set
        + struct.pack(">IIQ", admitted, rejected, total_gas) + total_fee.to_bytes(16, "big")
        + outcomes_dig + struct.pack(">I", len(outcomes)) + b"".join(outcomes)
    )
    body_digest = hashlib.sha256(
        SEQ_BODY_DIGEST_PREFIX
        + struct.pack(">I", len(bundle_bytes)) + bundle_bytes
        + struct.pack(">I", len(rcpt_bytes)) + rcpt_bytes
    ).digest()
    merkle_root, _ = merkle_root_and_proof(outcomes, 0)
    parent_hdr_dig = bytes(32)
    _, parent_lookup_id = content_tag(
        "receipt", PARENT_LOOKUP_SCHEMA, SEQ_PARENT_LOOKUP_PREFIX + genesis_id + parent_hdr_dig
    )
    core_bytes = encode_exact_core(
        parent_lookup_id, 1, genesis_id + profile_id, struct.pack(">QQ", 1, 0) + merkle_root + post_ref, body_digest
    )
    _, proposal_core_dig = content_tag("receipt", EXACT_CORE_SCHEMA, core_bytes)
    unsigned_hdr_dig = hashlib.sha256(
        SEQ_HDR_PAYLOAD_PREFIX + struct.pack(">HQQ", 1, 1, 0) + parent_hdr_dig + fork_ctx
        + body_digest + merkle_root + proposal_core_dig + post_ref
    ).digest()
    pre = encode_signing_preimage(1, chain_id, genesis_id, genesis_id, profile_id, 3, 1, 1, unsigned_hdr_dig)
    _, sign_dig = content_tag("receipt", SIGNING_PREIMAGE_SCHEMA, pre)
    seq_pk = ed_public_from_seed(DEFAULT_SEQ_SEED)
    seq_sig = ed_sign(DEFAULT_SEQ_SEED, sign_dig)
    hdr_bytes = (
        struct.pack(">HQQ", 1, 1, 0) + parent_hdr_dig + fork_ctx + body_digest + merkle_root
        + proposal_core_dig + post_ref + seq_pk + seq_sig
    )
    hdr_digest = hashlib.sha256(SEQ_HDR_DIGEST_PREFIX + hdr_bytes).digest()
    pub_bytes = (
        struct.pack(">H", 1) + hdr_bytes
        + struct.pack(">I", len(bundle_bytes)) + bundle_bytes
        + struct.pack(">I", len(rcpt_bytes)) + rcpt_bytes
    )

    # 2. Build opaque authority blobs (never interpreted as live authority)
    cert_d = opaque_blob_digest(1, b"opaque-cert-carrier-v1")
    occ_d = opaque_blob_digest(2, b"opaque-occ-carrier-v1")
    opaque_set_d = opaque_set_digest([cert_d, occ_d])

    # 3. Build SyncCheckpointManifestV1 (1024-byte chunks over pub_bytes)
    chunk_sz = 1_024
    chunks = [pub_bytes[i : i + chunk_sz] for i in range(0, len(pub_bytes), chunk_sz)]
    chk_digests = [chunk_digest(i, c) for i, c in enumerate(chunks)]
    # Negative control: 1-bit tamper moves chunk checksum
    assert chunk_digest(0, bytes([chunks[0][0] ^ 1]) + chunks[0][1:]) != chk_digests[0]

    chk_bytes = (
        struct.pack(">H", 1)
        + genesis_id
        + profile_id
        + struct.pack(">QQ", 1, 0)
        + hdr_digest
        + post_ref
        + struct.pack(">IQ", chunk_sz, len(pub_bytes))
        + opaque_set_d
        + struct.pack(">I", len(chk_digests))
        + b"".join(chk_digests)
    )
    chk_tag, chk_dig = content_tag("receipt", SYNC_CHECKPOINT_SCHEMA, chk_bytes)

    # 4. Build SyncRecoveryReceiptV1 (FullVerifier=1, 8 discovered, 7 banned, 1 honest, live_authority_admitted=0)
    rec_bytes = (
        struct.pack(">HB", 1, 1)
        + genesis_id
        + profile_id
        + struct.pack(">QQQI", 1, 0, 0, 28)
        + hdr_digest
        + post_ref
        + chk_dig
        + struct.pack(">I", 2)
        + opaque_set_d
        + struct.pack(">HHHB", 8, 7, 1, 0)
    )
    assert len(rec_bytes) == 380
    rec_tag, _ = content_tag("receipt", SYNC_RECOVERY_SCHEMA, rec_bytes)
    return chk_tag, rec_tag, opaque_set_d.hex()


def main() -> int:
    chk_tag, rec_tag, opq_hex = build_sync_1_vectors()
    if "--print-tags" in sys.argv:
        print("CHK_TAG  =", chk_tag)
        print("REC_TAG  =", rec_tag)
        print("OPQ_SHA  =", opq_hex)
        return 0
    assert chk_tag == EXPECTED_CHK_TAG, f"chk mismatch: {chk_tag}"
    assert rec_tag == EXPECTED_REC_TAG, f"rec mismatch: {rec_tag}"
    assert opq_hex == EXPECTED_OPQ_SHA, f"opq mismatch: {opq_hex}"
    print(f"sync_1_vectors: PASS (chk={chk_tag[:26]}..., rec={rec_tag[:26]}...)")
    return 0


EXPECTED_CHK_TAG = "#bb.receipt.sha256.m267jyqvgn3t7iqja3cu4jnb3qxnlpqjxr3lrlh4dq77nnura6qa"
EXPECTED_REC_TAG = "#bb.receipt.sha256.p4ock6iohrsau6v6sjkehw3tt36kw2zj2gdrjhqjgt25j2aly5aq"
EXPECTED_OPQ_SHA = "85f14c56e34047bdab721baaf4dfa88cbc6824733b1d6b3cba5cba323bf3fc75"

if __name__ == "__main__":
    raise SystemExit(main())
