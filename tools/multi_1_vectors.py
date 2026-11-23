#!/usr/bin/env python3
"""Clean-room pure-Python independent verifier & vector suite for MULTI-1 (PS-032).

Imports NO Rust or Bitblock code. Independently builds the 3-actor delegated devnet fixture
(Actor A -> Delegate C -> Actor B), signs all 8 mixed race/conflict envelopes under strict
RFC 8032 Ed25519, sorts them by (priority_tip DESC, principal_actor_id ASC, nonce ASC,
envelope_digest ASC), folds them through IndependentPythonExecutorV1 + compose_execution_root,
proves arrival-order permutation invariance, and reproduces both Wave 19 canonical schemas:
  * bitblock/multi-1/signed-stream-bundle/v1
  * bitblock/multi-1/fold-receipt/v1
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
from asset_1_vectors import encode_episode_value, episode_trie_key
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

BUNDLE_SCHEMA = b"bitblock/multi-1/signed-stream-bundle/v1"
RECEIPT_SCHEMA = b"bitblock/multi-1/fold-receipt/v1"
AUTH_DESC_SCHEMA = b"bitblock/identity-d1/authorization-descriptor/v1"

TX_PAYLOAD_PREFIX = b"bb.multi1.tx_payload.v1"
TX_ENVELOPE_PREFIX = b"bb.multi1.tx_env.v1"
INPUT_SET_PREFIX = b"bb.multi1.input_set.v1"
OUTCOMES_PREFIX = b"bb.multi1.outcomes.v1"
MINT_ID_PREFIX = b"bb.multi1.mint_id.v1"

OP_PLACE = 1
OP_DELETE = 2
OP_MINT = 3
OP_BURN = 4
OP_TRANSFER = 5

DISP_ADMITTED = 0
DISP_BAD_SIG = 1
DISP_BAD_DELEG = 2
DISP_STALE_ANCHOR = 3
DISP_BAD_NONCE = 4
DISP_RACE_LOST = 5
DISP_NOOP = 6
DISP_ADMITTED_FAIL = 7

NOMINAL_GAS = {
    OP_PLACE: 2_384,
    OP_DELETE: 2_384,
    OP_MINT: 28_792,
    OP_BURN: 28_280,
    OP_TRANSFER: 26_280,
}


def encode_delegation(delegate_id: bytes, allow_on: bool, allow_off: bool, exp: int) -> bytes:
    flags = (1 if allow_on else 0) | (2 if allow_off else 0)
    return delegate_id + struct.pack(">BQ", flags, exp)


def sign_envelope(
    chain_id: int,
    genesis_id: bytes,
    profile_id: bytes,
    principal: bytes,
    signer: bytes,
    seed: bytes,
    nonce: int,
    tip: int,
    anchor: bytes,
    op: int,
    coord: tuple[int, int, int],
    target: bytes,
    amount: int,
) -> dict:
    payload_dig = hashlib.sha256(
        TX_PAYLOAD_PREFIX
        + struct.pack(">HQ", 1, chain_id)
        + principal
        + signer
        + struct.pack(">Q", nonce)
        + tip.to_bytes(16, "big")
        + anchor
        + struct.pack(">Bqqq", op, *coord)
        + target
        + amount.to_bytes(16, "big")
    ).digest()
    pre = encode_signing_preimage(1, chain_id, genesis_id, genesis_id, profile_id, 1, 1, 1, payload_dig)
    _, sig_dig = content_tag("receipt", SIGNING_PREIMAGE_SCHEMA, pre)
    pubkey = ed_public_from_seed(seed)
    sig = ed_sign(seed, sig_dig)
    raw = (
        struct.pack(">HQ", 1, chain_id)
        + principal
        + signer
        + struct.pack(">Q", nonce)
        + tip.to_bytes(16, "big")
        + anchor
        + struct.pack(">Bqqq", op, *coord)
        + target
        + amount.to_bytes(16, "big")
        + pubkey
        + sig
    )
    assert len(raw) == 299
    env_dig = hashlib.sha256(TX_ENVELOPE_PREFIX + raw).digest()
    return {
        "chain_id": chain_id,
        "principal": principal,
        "signer": signer,
        "nonce": nonce,
        "tip": tip,
        "anchor": anchor,
        "op": op,
        "coord": coord,
        "target": target,
        "amount": amount,
        "pubkey": pubkey,
        "sig": sig,
        "raw": raw,
        "digest": env_dig,
    }


def derive_mint_id(height: int, cidx: int, actor: bytes, coord: tuple[int, int, int]) -> bytes:
    return hashlib.sha256(
        MINT_ID_PREFIX
        + struct.pack(">QI", height, cidx)
        + actor
        + struct.pack(">qqq", *coord)
    ).digest()


def rebuild_multi_1_vectors() -> tuple[str, str, str]:
    # 1. Upstream policies & scope (identical to genesis_root_parity_1_vectors)
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
    _, bounds_d = content_tag(
        "policy", PROTOCOL_BOUNDS_SCHEMA, encode_protocol_bounds(threat_d, default_local_bounds_digest())
    )
    _, resource_d = content_tag("policy", RESOURCE_POLICY_SCHEMA, encode_resource_policy(bounds_d, asset_d))
    _, boot_d = content_tag("policy", BOOTSTRAP_POLICY_SCHEMA, encode_bootstrap_policy())

    chain_id, world_id, ns = 77, 1, "bitblock.devnet.v1"

    def mk_desc(act_s: int, adm_s: int, delegs: list[tuple[bytes, bool, bool, int]]):
        ap = encode_threshold_policy(0, 1, [ed_public_from_seed(bytes([act_s]) * 32)])
        ad = encode_threshold_policy(1, 1, [ed_public_from_seed(bytes([adm_s]) * 32)])
        # encode_auth_descriptor expects raw tuples or encoded? Let's encode directly matching identity_d1.rs!
        out = bytearray(struct.pack(">Q", 1))
        out += ap + ad
        out += struct.pack(">H", len(delegs))
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

    actors_meta = sorted(
        [(aid_a, desc_a, adm_a), (aid_b, desc_b, adm_b), (aid_c, desc_c, adm_c)],
        key=lambda x: x[0],
    )
    gen_actors = [
        (aid, content_tag("policy", AUTH_DESC_SCHEMA, db)[1], 0, 100_000_000_000)
        for aid, db, _ in actors_meta
    ]
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
        struct.pack(">H", 1)
        + profile_id
        + base_contract_digest()
        + bounds_d
        + resource_d
        + struct.pack(">IQQIBH", 16_384, 100_000_000, 2_000, BASE_MASK, 0, len(bands))
        + b"".join(encode_band(*b) for b in bands)
    )
    _, spec_dig = content_tag("policy", RESOLVED_SPEC_SCHEMA, spec_b)
    fork_ctx = struct.pack(">QQ", chain_id, world_id) + genesis_id + profile_id + spec_dig

    # Build COMMIT-1 profiles & initial genesis 10-domain roots
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

    def compose_state(
        height: int,
        world_set: set[tuple[int, int, int]],
        active_mints: dict[tuple[int, int, int], bytes],
        episodes: dict[bytes, tuple[tuple[int, int, int], bytes | None]],
        balances: dict[bytes, int],
        asset_eps: dict[tuple[int, int, int], tuple[bytes, bytes, int]],
    ):
        wr = build_patricia(prof_w, COMMIT_1_WORLD_ROOT_SCHEMA, 192, [(coord_key(*c), ON_V1) for c in world_set]).hash
        ar = build_patricia(prof_a, COMMIT_1_ACTIVE_TOKEN_ROOT_SCHEMA, 192, [(coord_key(*c), m) for c, m in active_mints.items()]).hash
        er = build_patricia(prof_e, COMMIT_1_EPISODE_ROOT_SCHEMA, 256, [(mid, episode_record(c, mid, b)) for mid, (c, b) in episodes.items()]).hash
        br = DomainPatriciaPy(DOM_NATIVE_BALANCE, {a: bal.to_bytes(16, "big") for a, bal in balances.items()}).root()
        aep = DomainPatriciaPy(DOM_EPISODE_OWNERSHIP, {episode_trie_key(*c): encode_episode_value(o, m, d) for c, (o, m, d) in asset_eps.items()}).root()
        roots = {1: wr, 2: ar, 3: er, 4: br, 5: aep, 7: auth_root, 8: nonce_root, 9: gov_root, 10: econ_root, 11: sched_root}
        cb, app_r, exec_r, exec_ref, comp_tag = compose_execution_root(
            height, 0, profile_id, 1, chain_id, world_id, genesis_id, scope_d, roots
        )
        return wr, app_r, exec_r, exec_ref, cb[196:228], comp_tag

    balances = {aid_a: 100_000_000_000, aid_b: 100_000_000_000, aid_c: 100_000_000_000}
    pre_wr, _, _, pre_ref, _, _ = compose_state(0, {(0, 0, 0)}, {}, {}, balances, {})

    # 2. Construct exact 8-tx canonical mixed race bundle matching Rust fixture
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
    bundle_tag, _ = content_tag("receipt", BUNDLE_SCHEMA, bundle_bytes)

    # 3. Canonical sort: (-tip, principal, nonce, digest) — permutation invariant!
    sorted_envs = sorted(envs, key=lambda e: (-e["tip"], e["principal"], e["nonce"], e["digest"]))
    rev_sorted = sorted(list(reversed(envs)), key=lambda e: (-e["tip"], e["principal"], e["nonce"], e["digest"]))
    assert [e["digest"] for e in sorted_envs] == [e["digest"] for e in rev_sorted]
    input_set_dig = hashlib.sha256(
        INPUT_SET_PREFIX + struct.pack(">I", len(sorted_envs)) + b"".join(e["digest"] for e in sorted_envs)
    ).digest()

    # 4. Fold independently & compute COMMIT-1 history/episode state
    world_set = {(0, 0, 0)}
    _, hist_head = content_tag("receipt", HIST_GENESIS_SCHEMA, w0)
    hist_tick = 0
    active_mints: dict[tuple[int, int, int], bytes] = {}
    episodes: dict[bytes, tuple[tuple[int, int, int], bytes | None]] = {}
    asset_eps: dict[tuple[int, int, int], tuple[bytes, bytes, int]] = {}
    occ_owners: dict[tuple[int, int, int], bytes] = {}
    nonces = {aid_a: 0, aid_b: 0, aid_c: 0}

    op_index = 0

    def commit_place_history(c: tuple[int, int, int]) -> bytes:
        nonlocal hist_head, op_index
        world_set.add(c)
        tick = 1 * 1024 + op_index
        edit_b = struct.pack(">Bqqq", 0, *c)
        _, ev_dig = content_tag("event", HIST_EVENT_SCHEMA, hist_head + struct.pack(">Q", tick) + edit_b)
        post_w = world_tag_py(world_set)
        _, rcpt_dig = content_tag(
            "receipt",
            HIST_RECEIPT_SCHEMA,
            hist_head + struct.pack(">QQ", tick, 1) + edit_b + post_w + ev_dig + b"\x00\x00",
        )
        hist_head = rcpt_dig
        op_index += 2  # BasePlaceVoxel followed by AssetMintDeposit
        active_mints[c] = ev_dig
        episodes[ev_dig] = (c, None)
        return ev_dig

    outcomes = []
    admitted = rejected = total_gas = total_fee = 0
    fee_burn, dep = 1_000, 1_000_000
    for idx, e in enumerate(sorted_envs):
        # Verify strict RFC 8032 signature
        p_dig = hashlib.sha256(
            TX_PAYLOAD_PREFIX
            + struct.pack(">HQ", 1, e["chain_id"])
            + e["principal"]
            + e["signer"]
            + struct.pack(">Q", e["nonce"])
            + e["tip"].to_bytes(16, "big")
            + e["anchor"]
            + struct.pack(">Bqqq", e["op"], *e["coord"])
            + e["target"]
            + e["amount"].to_bytes(16, "big")
        ).digest()
        pre = encode_signing_preimage(1, chain_id, genesis_id, genesis_id, profile_id, 1, 1, 1, p_dig)
        _, verify_dig = content_tag("receipt", SIGNING_PREIMAGE_SCHEMA, pre)
        assert ed_verify_strict(e["pubkey"], verify_dig, e["sig"])

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
                commit_place_history(e["coord"])
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

        if disp == DISP_ADMITTED:
            admitted += 1
            g = NOMINAL_GAS[e["op"]]
            fb = g * 2
        elif disp == DISP_ADMITTED_FAIL:
            rejected += 1
            g = NOMINAL_GAS[e["op"]]
            fb = g * 2
        else:
            rejected += 1
            g = fb = 0
        total_gas += g
        total_fee += fb
        o_bytes = (
            struct.pack(">I", idx)
            + e["digest"]
            + e["principal"]
            + struct.pack(">QBB Q", e["nonce"], e["op"], disp, g)
            + fb.to_bytes(16, "big")
        )
        assert len(o_bytes) == 102
        outcomes.append(o_bytes)

    _, post_app, _, post_ref, post_entry_set, post_comp_tag = compose_state(
        1, world_set, active_mints, episodes, balances, asset_eps
    )
    outcomes_dig = hashlib.sha256(OUTCOMES_PREFIX + struct.pack(">I", len(outcomes)) + b"".join(outcomes)).digest()
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
    rcpt_tag, _ = content_tag("receipt", RECEIPT_SCHEMA, rcpt_bytes)
    return bundle_tag, rcpt_tag, post_comp_tag


def main() -> int:
    b_tag, r_tag, c_tag = rebuild_multi_1_vectors()
    if "--print-tags" in sys.argv:
        print("BUNDLE_TAG =", b_tag)
        print("RCPT_TAG   =", r_tag)
        print("COMP_TAG   =", c_tag)
        return 0
    assert b_tag == EXPECTED_BUNDLE_TAG, f"bundle mismatch: {b_tag}"
    assert r_tag == EXPECTED_RCPT_TAG, f"rcpt mismatch: {r_tag}"
    assert c_tag == EXPECTED_COMP_TAG, f"comp mismatch: {c_tag}"
    print(f"multi_1_vectors: PASS (bundle={b_tag[:26]}..., rcpt={r_tag[:26]}..., comp={c_tag[:26]}...)")
    return 0


EXPECTED_BUNDLE_TAG = "#bb.receipt.sha256.h6deuodmzgmgpkyqv2do2rkaonc7gnn7rjgi5hp3oqwkvs7l5kgq"
EXPECTED_RCPT_TAG = "#bb.receipt.sha256.ssqa3yacg6tg6jk6d7eddccshdlikpzwxrqov6cfr5mcldjqbr3q"
EXPECTED_COMP_TAG = "#bb.receipt.sha256.lbjy45lcbkxe7qle5cwdo3nlvh3qd253kn5m433mphgkmeua5gia"

if __name__ == "__main__":
    raise SystemExit(main())
