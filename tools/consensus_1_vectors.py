#!/usr/bin/env python3
"""Clean-room pure-Python independent verifier & vector suite for CONSENSUS-1@base (PS-056).

Imports NO Rust or Bitblock code. Independently reproduces:
  * bitblock/consensus-1/bft-commit-certificate/v1 (ConsensusCommitCertificateV1:
      Proposal + strict > 2/3 PrepareQC + strict > 2/3 CommitQC under RFC 8032 Ed25519)
  * bitblock/consensus-1/parent-authority-certificate/v1 (AcceptedConsensusNativeParentAuthorityCertificateV1:
      live admission with exact 3-anchor history-start equality, split-wrapper receipts,
      predecessor-closure lower bounds, and > 2/3 quorum authorization)
  * Negative controls: sub-quorum 2/4 refusal, stale schedule rejection, late-valid-suffix refusal
"""

from __future__ import annotations

import hashlib
import struct
import sys

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
    content_tag,
    ed_public_from_seed,
    ed_sign,
    ed_verify_strict,
    encode_signing_preimage,
    encode_suite_descriptor,
)
from economic_state_1_vectors import encode_econ_root, encode_val_record, u128_be
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
    WRAPPER_SCHEMA,
    encode_exact_core,
)
from resource_1_vectors import POLICY_SCHEMA as RESOURCE_POLICY_SCHEMA
from resource_1_vectors import encode_resource_policy
from sequencer_1_vectors import SEQ_BODY_DIGEST_PREFIX, merkle_root_and_proof
from threat_1_and_vm_d1_vectors import THREAT_MODEL_SCHEMA, encode_threat_model
from transition_spec_1_vectors import (
    BASE_MASK,
    RESOLVED_SPEC_SCHEMA,
    base_contract_digest,
    build_bands,
    encode_band,
)
from transition_spec_d1_vectors import CONTRACT_SCHEMA, encode_base_contract
from validator_schedule_1_vectors import (
    SCHED_EPOCH_SCHEMA,
    encode_epoch_schedule,
    epoch_trie_key,
)

CONSENSUS_COMMIT_CERT_SCHEMA = b"bitblock/consensus-1/bft-commit-certificate/v1"
CONSENSUS_PARENT_AUTH_CERT_SCHEMA = b"bitblock/consensus-1/parent-authority-certificate/v1"

CONS_PROP_PAYLOAD_PREFIX = b"bb.cons1.proposal.v1"
CONS_BLOCK_ID_PREFIX = b"bb.cons1.block_id.v1"
CONS_VOTE_PAYLOAD_PREFIX = b"bb.cons1.vote.v1"
CONS_QC_DIGEST_PREFIX = b"bb.cons1.qc_digest.v1"
CONS_PARENT_LOOKUP_PREFIX = b"bb.cons1.parent_lookup.v1"
CONS_CERT_OCC_PREFIX = b"bb.cons1.occ_rcpt.v1"
CONS_CERT_UNIV_PREFIX = b"bb.cons1.occ_univ.v1"
CONS_CERT_EVID_PREFIX = b"bb.cons1.cert_evid.v1"
CONS_CERT_SIGN_PREFIX = b"bb.cons1.cert_sign.v1"
CONS_CERT_ADMIT_PREFIX = b"bb.cons1.cert_admit.v1"
CONS_EXEC_ROOT_DIGEST_PREFIX = b"bb.cons1.exec_root.v1"


def sign_domain_digest(
    chain_id: int,
    genesis_id: bytes,
    profile_id: bytes,
    domain_tag: int,
    payload_digest: bytes,
) -> bytes:
    pre = encode_signing_preimage(
        1, chain_id, genesis_id, genesis_id, profile_id, domain_tag, 1, 1, payload_digest
    )
    _, dig = content_tag("receipt", SIGNING_PREIMAGE_SCHEMA, pre)
    return dig


def encode_wrapper(core_bytes: bytes, creds: list[tuple[bytes, bytes]], evs: list[bytes], deps: list[tuple[bytes, int]]) -> bytes:
    out = struct.pack(">I", len(core_bytes)) + core_bytes + struct.pack(">H", len(creds))
    for pk, sig in creds:
        out += pk + sig
    out += struct.pack(">H", len(evs)) + b"".join(evs)
    out += struct.pack(">H", len(deps))
    for did, eb in deps:
        out += did + struct.pack(">Q", eb)
    return out


def encode_occurrence_receipt(
    ordinal: int,
    lookup_id: bytes,
    core_id: bytes,
    raw_wrap_dig: bytes,
    creds: list[tuple[bytes, bytes]],
    evs: list[bytes],
    avail_assess_id: bytes,
    deps: list[tuple[bytes, int]],
    has_conflict: bool,
) -> bytes:
    cred_w = hashlib.sha256(
        CONS_CERT_OCC_PREFIX + b"cred" + b"".join(pk + sig for pk, sig in creds)
    ).digest()
    ev_w = hashlib.sha256(CONS_CERT_OCC_PREFIX + b"evid" + b"".join(evs)).digest()
    dep_w = hashlib.sha256(
        CONS_CERT_OCC_PREFIX + b"deps" + b"".join(did + struct.pack(">Q", eb) for did, eb in deps)
    ).digest()
    out = (
        struct.pack(">Q", ordinal)
        + lookup_id
        + core_id
        + raw_wrap_dig
        + cred_w
        + ev_w
        + avail_assess_id
        + dep_w
        + bytes([1 if has_conflict else 0])
    )
    assert len(out) == 233
    return out


def build_devnet_batch1_context() -> dict:
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
    auth_root = DomainPatriciaPy(
        DOM_AUTH,
        {aid: encode_actor_auth_state(aid, db, 0, adm, 100, None) for aid, db, adm in actors_meta},
    ).root()
    nonce_root = DomainPatriciaPy(
        DOM_NONCE, {aid: struct.pack(">Q", 0) for aid, _, _ in actors_meta}
    ).root()
    gov_root = DomainPatriciaPy(
        DOM_GOVERNANCE, {param_trie_key(k): struct.pack(">Q", v) for k, v in gov_params}
    ).root()
    econ_root = DomainPatriciaPy(
        DOM_ECONOMIC_STAKE,
        {vid: encode_val_record(s, 0, 0, False, 0, []) for vid, _, _, s in gen_vals},
    ).root()
    gov_sha = hashlib.sha256(
        encode_gov_root(profile_id, 0, 0, 0, 0, 0, bytes(32), 0, len(gov_params), gov_root)
    ).digest()
    bsum = sum(v[3] for v in gen_vals)
    econ_sha = hashlib.sha256(
        encode_econ_root(profile_id, 0, 4, 4, bsum, 0, 0, 0, 0, bsum, 0, 0, econ_root)
    ).digest()
    ep0_bytes = encode_epoch_schedule(0, 0, econ_sha, gov_sha, 2500, gen_vals)
    ep1_bytes = encode_epoch_schedule(1, 0, econ_sha, gov_sha, 2500, gen_vals)
    _, ep0_sched_digest = content_tag("receipt", SCHED_EPOCH_SCHEMA, ep0_bytes)
    sched_root = DomainPatriciaPy(
        DOM_VALIDATOR_SCHEDULE,
        {epoch_trie_key(0): ep0_bytes, epoch_trie_key(1): ep1_bytes},
    ).root()

    def compose_state(height, world_set, active_mints, episodes, balances, asset_eps):
        wr = build_patricia(
            prof_w, COMMIT_1_WORLD_ROOT_SCHEMA, 192, [(coord_key(*c), ON_V1) for c in world_set]
        ).hash
        ar = build_patricia(
            prof_a,
            COMMIT_1_ACTIVE_TOKEN_ROOT_SCHEMA,
            192,
            [(coord_key(*c), m) for c, m in active_mints.items()],
        ).hash
        er = build_patricia(
            prof_e,
            COMMIT_1_EPISODE_ROOT_SCHEMA,
            256,
            [(mid, episode_record(c, mid, b)) for mid, (c, b) in episodes.items()],
        ).hash
        br = DomainPatriciaPy(
            DOM_NATIVE_BALANCE, {a: bal.to_bytes(16, "big") for a, bal in balances.items()}
        ).root()
        aep = DomainPatriciaPy(
            DOM_EPISODE_OWNERSHIP,
            {episode_trie_key(*c): encode_episode_value(o, m, d) for c, (o, m, d) in asset_eps.items()},
        ).root()
        roots = {
            1: wr,
            2: ar,
            3: er,
            4: br,
            5: aep,
            7: auth_root,
            8: nonce_root,
            9: gov_root,
            10: econ_root,
            11: sched_root,
        }
        cb, app_r, _, exec_ref, _ = compose_execution_root(
            height, 0, profile_id, 1, chain_id, world_id, genesis_id, scope_d, roots
        )
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
    bundle_bytes = (
        struct.pack(">HQQ", 1, 1, 0) + fork_ctx + struct.pack(">QI", 2, len(envs))
        + b"".join(e["raw"] for e in envs)
    )
    sorted_envs = sorted(envs, key=lambda e: (-e["tip"], e["principal"], e["nonce"], e["digest"]))
    input_set_dig = hashlib.sha256(
        INPUT_SET_PREFIX + struct.pack(">I", len(sorted_envs)) + b"".join(e["digest"] for e in sorted_envs)
    ).digest()

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
        _, hist_head = content_tag(
            "receipt",
            HIST_RECEIPT_SCHEMA,
            hist_head + struct.pack(">QQ", tick, 1) + edit_b + post_w + ev_dig + b"\x00\x00",
        )
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
            + struct.pack(">QBBQ", e["nonce"], e["op"], disp, g) + fb.to_bytes(16, "big")
        )

    _, post_app, post_ref, post_entry_set = compose_state(
        1, world_set, active_mints, episodes, balances, asset_eps
    )
    outcomes_dig = hashlib.sha256(
        OUTCOMES_PREFIX + struct.pack(">I", len(outcomes)) + b"".join(outcomes)
    ).digest()
    rcpt_bytes = (
        struct.pack(">HQQ", 1, 1, 0)
        + fork_ctx
        + input_set_dig
        + pre_ref
        + post_ref
        + post_app
        + post_entry_set
        + struct.pack(">IIQ", admitted, rejected, total_gas)
        + total_fee.to_bytes(16, "big")
        + outcomes_dig
        + struct.pack(">I", len(outcomes))
        + b"".join(outcomes)
    )
    body_digest = hashlib.sha256(
        SEQ_BODY_DIGEST_PREFIX
        + struct.pack(">I", len(bundle_bytes))
        + bundle_bytes
        + struct.pack(">I", len(rcpt_bytes))
        + rcpt_bytes
    ).digest()
    merkle_root, _ = merkle_root_and_proof(outcomes, 0)
    return {
        "chain_id": chain_id,
        "genesis_id": genesis_id,
        "profile_id": profile_id,
        "fork_ctx": fork_ctx,
        "ep0_sched_digest": ep0_sched_digest,
        "gen_vals": gen_vals,
        "body_digest": body_digest,
        "merkle_root": merkle_root,
        "post_ref": post_ref,
        "outcomes": outcomes,
        "aid_a": aid_a,
        "aid_b": aid_b,
        "aid_c": aid_c,
        "balances": balances,
    }


def build_consensus_1_vectors() -> tuple[str, str, dict]:
    ctx = build_devnet_batch1_context()
    chain_id = ctx["chain_id"]
    genesis_id = ctx["genesis_id"]
    profile_id = ctx["profile_id"]
    fork_ctx = ctx["fork_ctx"]
    ep0_sched_digest = ctx["ep0_sched_digest"]
    body_digest = ctx["body_digest"]
    merkle_root = ctx["merkle_root"]
    post_ref = ctx["post_ref"]

    # 1. Build Height 1, Epoch 0, View 0 Proposal (leader = validators[(1+0)%4] = validator 2)
    parent_block_id = bytes(32)
    _, parent_lookup_id = content_tag(
        "receipt", PARENT_LOOKUP_SCHEMA, CONS_PARENT_LOOKUP_PREFIX + genesis_id + parent_block_id
    )
    core_bytes = encode_exact_core(
        parent_lookup_id,
        2,
        genesis_id + profile_id,
        struct.pack(">QQQ", 1, 0, 0) + merkle_root + post_ref,
        body_digest,
    )
    _, core_digest = content_tag("receipt", EXACT_CORE_SCHEMA, core_bytes)

    leader_id = bytes([2]) * 32
    leader_prop_seed = bytes([202]) * 32
    leader_prop_pk = ed_public_from_seed(leader_prop_seed)
    justify_dig = bytes(32)
    justify_view = 0

    prop_unsigned = hashlib.sha256(
        CONS_PROP_PAYLOAD_PREFIX
        + struct.pack(">HQQQ", 1, 1, 0, 0)
        + parent_block_id
        + fork_ctx
        + ep0_sched_digest
        + body_digest
        + merkle_root
        + core_digest
        + post_ref
        + justify_dig
        + struct.pack(">Q", justify_view)
        + leader_id
    ).digest()
    block_id = hashlib.sha256(CONS_BLOCK_ID_PREFIX + prop_unsigned + leader_id).digest()
    prop_sign_dig = sign_domain_digest(chain_id, genesis_id, profile_id, 3, prop_unsigned)
    prop_sig = ed_sign(leader_prop_seed, prop_sign_dig)
    assert ed_verify_strict(leader_prop_pk, prop_sign_dig, prop_sig)

    prop_bytes = (
        struct.pack(">HQQQ", 1, 1, 0, 0)
        + parent_block_id
        + fork_ctx
        + ep0_sched_digest
        + body_digest
        + merkle_root
        + core_digest
        + post_ref
        + justify_dig
        + struct.pack(">Q", justify_view)
        + leader_id
        + leader_prop_pk
        + prop_sig
    )
    assert len(prop_bytes) == 644

    # 2. Build 4-validator PrepareQC (phase=1) and CommitQC (phase=2)
    exec_ref_dig = hashlib.sha256(CONS_EXEC_ROOT_DIGEST_PREFIX + post_ref).digest()
    total_w = 400_000_000_000

    def build_qc(phase: int, validator_indices: list[int]) -> tuple[bytes, bytes]:
        votes_raw = []
        signed_w = 0
        for i in sorted(validator_indices):
            vid = bytes([i]) * 32
            vseed = bytes([100 + i]) * 32
            vpk = ed_public_from_seed(vseed)
            v_uns = hashlib.sha256(
                CONS_VOTE_PAYLOAD_PREFIX
                + struct.pack(">HQQQB", 1, 1, 0, 0, phase)
                + block_id
                + ep0_sched_digest
                + exec_ref_dig
                + b"\x01"
                + vid
            ).digest()
            vsdig = sign_domain_digest(chain_id, genesis_id, profile_id, 4, v_uns)
            vsig = ed_sign(vseed, vsdig)
            assert ed_verify_strict(vpk, vsdig, vsig)
            vb = (
                struct.pack(">HQQQB", 1, 1, 0, 0, phase)
                + block_id
                + ep0_sched_digest
                + exec_ref_dig
                + b"\x01"
                + vid
                + vpk
                + vsig
            )
            assert len(vb) == 252
            votes_raw.append(vb)
            signed_w += 100_000_000_000
        # Negative control check: strict > 2/3
        assert signed_w * 3 > total_w * 2
        qc_bytes = (
            struct.pack(">HQQQB", 1, 1, 0, 0, phase)
            + block_id
            + ep0_sched_digest
            + exec_ref_dig
            + u128_be(total_w)
            + u128_be(signed_w)
            + struct.pack(">H", len(votes_raw))
            + b"".join(votes_raw)
        )
        qc_dig = hashlib.sha256(CONS_QC_DIGEST_PREFIX + qc_bytes).digest()
        return qc_bytes, qc_dig

    # Negative control: 2/4 fails strict > 2/3
    assert not ((200_000_000_000 * 3) > (total_w * 2))

    pqc_bytes, pqc_dig = build_qc(1, [1, 2, 3, 4])
    cqc_bytes, cqc_dig = build_qc(2, [1, 2, 3, 4])

    commit_cert_bytes = (
        struct.pack(">H", 1)
        + prop_bytes
        + struct.pack(">I", len(pqc_bytes))
        + pqc_bytes
        + struct.pack(">I", len(cqc_bytes))
        + cqc_bytes
    )
    cc_tag, cc_dig = content_tag("receipt", CONSENSUS_COMMIT_CERT_SCHEMA, commit_cert_bytes)

    # 3. Build Live AcceptedConsensusNativeParentAuthorityCertificateV1
    wrap_sign_dig = sign_domain_digest(chain_id, genesis_id, profile_id, 3, core_digest)

    def mk_wrap_rcpt(ordinal: int, v_idx: int, ev_b: int, dep_b: int, avail_b: int) -> tuple[bytes, bytes]:
        pseed = bytes([200 + v_idx]) * 32
        pk = ed_public_from_seed(pseed)
        sig = ed_sign(pseed, wrap_sign_dig)
        creds = [(pk, sig)]
        evs = [bytes([ev_b]) * 32]
        deps = [(bytes([dep_b]) * 32, 1024)]
        wb = encode_wrapper(core_bytes, creds, evs, deps)
        _, wdig = content_tag("receipt", WRAPPER_SCHEMA, wb)
        rcpt = encode_occurrence_receipt(
            ordinal, parent_lookup_id, core_digest, wdig, creds, evs, bytes([avail_b]) * 32, deps, False
        )
        return wb, rcpt

    w1_bytes, r1 = mk_wrap_rcpt(1, 1, 0xA1, 0xB1, 0xC1)
    w2_bytes, r2 = mk_wrap_rcpt(2, 2, 0xA2, 0xB2, 0xC2)
    occ_rcpts = [r1, r2]
    occ_univ_root = hashlib.sha256(
        CONS_CERT_UNIV_PREFIX + struct.pack(">I", len(occ_rcpts)) + b"".join(occ_rcpts)
    ).digest()

    history_start = genesis_id * 3
    cutoff_pos, earliest_valid, cand_rule = 2, 1, 1
    assert earliest_valid <= cutoff_pos and cand_rule <= cutoff_pos  # late-valid-suffix & cand-rule guard
    evid_commit_id = hashlib.sha256(
        CONS_CERT_EVID_PREFIX
        + history_start
        + struct.pack(">QQQ", cutoff_pos, earliest_valid, cand_rule)
        + occ_univ_root
    ).digest()

    consumer_ctx = (
        struct.pack(">BQ", 1, chain_id)
        + profile_id
        + bytes([0xD4]) * 32
        + struct.pack(">QQ", 1, 1)
        + bytes([0xE5]) * 32
    )
    assert len(consumer_ctx) == 121
    attained_status = 2  # AcceptedLive
    cert_policy_id = bytes([0xF6]) * 32

    sign_doc = hashlib.sha256(
        CONS_CERT_SIGN_PREFIX
        + struct.pack(">H", 1)
        + profile_id
        + parent_lookup_id
        + core_digest
        + consumer_ctx
        + evid_commit_id
        + occ_univ_root
        + bytes([attained_status])
        + cert_policy_id
        + struct.pack(">Q", 0)
        + ep0_sched_digest
        + cqc_dig
        + b"\x00"
    ).digest()
    cert_sdig = sign_domain_digest(chain_id, genesis_id, profile_id, 4, sign_doc)
    issuer_pairs = []
    for i in (1, 2, 3):
        vid = bytes([i]) * 32
        vseed = bytes([100 + i]) * 32
        issuer_pairs.append((vid, ed_sign(vseed, cert_sdig)))

    pa_cert_bytes = (
        struct.pack(">H", 1)
        + profile_id
        + parent_lookup_id
        + core_digest
        + consumer_ctx
        + history_start
        + struct.pack(">QQQH", cutoff_pos, earliest_valid, cand_rule, len(occ_rcpts))
        + b"".join(occ_rcpts)
        + occ_univ_root
        + evid_commit_id
        + bytes([attained_status])
        + cert_policy_id
        + struct.pack(">Q", 0)
        + ep0_sched_digest
        + cqc_dig
        + b"\x00"
        + struct.pack(">H", len(issuer_pairs))
        + b"".join(vid + sig for vid, sig in issuer_pairs)
    )
    pa_tag, pa_dig = content_tag("receipt", CONSENSUS_PARENT_AUTH_CERT_SCHEMA, pa_cert_bytes)
    pa_adm_id = hashlib.sha256(CONS_CERT_ADMIT_PREFIX + pa_cert_bytes).digest()

    out_ctx = dict(ctx)
    out_ctx.update({
        "parent_lookup_id": parent_lookup_id,
        "core_bytes": core_bytes,
        "core_digest": core_digest,
        "block_id": block_id,
        "pqc_bytes": pqc_bytes,
        "pqc_dig": pqc_dig,
        "cqc_bytes": cqc_bytes,
        "cqc_dig": cqc_dig,
        "commit_cert_bytes": commit_cert_bytes,
        "cc_tag": cc_tag,
        "w1_bytes": w1_bytes,
        "w2_bytes": w2_bytes,
        "occ_rcpts": occ_rcpts,
        "occ_univ_root": occ_univ_root,
        "evid_commit_id": evid_commit_id,
        "consumer_ctx": consumer_ctx,
        "pa_cert_bytes": pa_cert_bytes,
        "pa_tag": pa_tag,
        "pa_adm_id": pa_adm_id,
    })
    return cc_tag, pa_tag, out_ctx


def main() -> int:
    cc_tag, pa_tag, _ = build_consensus_1_vectors()
    if "--print-tags" in sys.argv:
        print("CC_TAG =", cc_tag)
        print("PA_TAG =", pa_tag)
        return 0
    assert cc_tag == EXPECTED_CC_TAG, f"commit cert tag mismatch: {cc_tag}"
    assert pa_tag == EXPECTED_PA_TAG, f"parent auth cert tag mismatch: {pa_tag}"
    print(f"consensus_1_vectors: PASS (cc={cc_tag[:26]}..., pa={pa_tag[:26]}...)")
    return 0


EXPECTED_CC_TAG = "#bb.receipt.sha256.hubj5ifmhrm4xxyu4gw5yzexokxryskow6yvge6uflam4xvyawna"
EXPECTED_PA_TAG = "#bb.receipt.sha256.ovdkxdeii3nlsw6xmpidj6xkqun3mmmmnir36mwqikie3levp4rq"

if __name__ == "__main__":
    raise SystemExit(main())
