#!/usr/bin/env python3
"""Clean-room pure-Python independent verifier & vector suite for SUCCESSOR-ACTIVATION-1 (PS-092/093).

Imports NO Rust or Bitblock code. Independently reproduces all 4 Wave 32 schemas:
  * bitblock/successor-activation-bundle/v1 (internal-only bundle, external_root_count = 0, |S|=3 & |S|=0)
  * bitblock/successor-activation-sign-doc/v1 (799-byte activation sign doc with live cert historical ref)
  * bitblock/successor-activation-authorization/v1 (3-of-5 Ed25519 GovernanceAction activation auth)
  * bitblock/successor-activation-1/carrier/v1 (EmbeddedSuccessorActivationV1 + DedicatedMigrationBodyV1)
plus independent zero-allocation streaming preflight, live two-purpose CONSENSUS-1 certificates
(ReleaseAuthority vs ActivationAuthority), RFC 8032 Ed25519 verification, and negative controls.
"""

from __future__ import annotations

import hashlib
import struct
import sys

from auth_commit_1_vectors import DOM_ECONOMIC_STAKE, DOM_GOVERNANCE, DomainPatriciaPy
from chain_launch_1_vectors import build_chain_launch_vectors, build_commit_domain_registry_bytes
from consensus_1_vectors import (
    CONS_BLOCK_ID_PREFIX,
    CONS_CERT_ADMIT_PREFIX,
    CONS_CERT_EVID_PREFIX,
    CONS_CERT_SIGN_PREFIX,
    CONS_CERT_UNIV_PREFIX,
    CONS_EXEC_ROOT_DIGEST_PREFIX,
    CONS_PARENT_LOOKUP_PREFIX,
    CONS_PROP_PAYLOAD_PREFIX,
    CONS_QC_DIGEST_PREFIX,
    CONS_VOTE_PAYLOAD_PREFIX,
    CONSENSUS_PARENT_AUTH_CERT_SCHEMA,
    encode_occurrence_receipt,
    encode_wrapper,
    sign_domain_digest,
)
from crypto_d1_vectors import (
    SIGNING_PREIMAGE_SCHEMA,
    content_tag,
    ed_public_from_seed,
    ed_sign,
    ed_verify_strict,
    encode_signing_preimage,
)
from economic_state_1_vectors import encode_econ_root, encode_val_record, u128_be
from governance_1_vectors import encode_gov_root, param_trie_key
from profile_1_vectors import (
    CLOSURE_SCHEMA,
    MANIFEST_SCHEMA as PROFILE_MANIFEST_SCHEMA,
    PROF1_ASSUMPTION_PREFIX,
    PROF1_LOSS_PREFIX,
    PROF1_POLICY_PREFIX,
    lp_str,
)
from profile_bootstrap_vectors import decode_tag_digest
from protocol_id_1_vectors import (
    EXACT_CORE_SCHEMA,
    PARENT_LOOKUP_SCHEMA,
    WRAPPER_SCHEMA,
    encode_exact_core,
)
from release_1_vectors import (
    AUTH_SIGN_DOC_SCHEMA,
    CORE_SCHEMA,
    RECORD_SCHEMA,
    REL1_POLICY_PREFIX,
    SIGN_DOC_SCHEMA as REL_SIGN_DOC_SCHEMA,
)
from release_r1_vectors import (
    BUILD_INPUTS_SCHEMA,
    CANDIDATE_RECEIPT_SCHEMA,
    RELR1_CONFIG_PREFIX,
    RELR1_FEATURE_PREFIX,
    RELR1_SOURCE_PREFIX,
)
from release_repro_framework import build_framework_manifest
from upgrade_1_vectors import MIGRATION_SPEC_SCHEMA, build_upgrade_1_vectors
from validator_schedule_1_vectors import SCHED_EPOCH_SCHEMA, encode_epoch_schedule

BUNDLE_SCHEMA = b"bitblock/successor-activation-bundle/v1"
SIGN_DOC_SCHEMA = b"bitblock/successor-activation-sign-doc/v1"
AUTH_SCHEMA = b"bitblock/successor-activation-authorization/v1"
CARRIER_SCHEMA = b"bitblock/successor-activation-1/carrier/v1"

UPGRADE_1_MERGED_COMMIT_HEX = "246ef87a5d2e5f7b682fda23cf88ffec3c50b9c6"

SA1_ACTIVATION_POLICY_PREFIX = b"bb.sa1.trusted_activation_authority_policy.v1"
SA1_AVAIL_COMMIT_PREFIX = b"bb.sa1.availability_commitment.v1"
SA1_GOV_AUTH_ROOT_PREFIX = b"bb.sa1.predecessor_gov_authority_root.v1"
SA1_RELEASE_CERT_POLICY_PREFIX = b"bb.sa1.release_cert_policy.v1"
SA1_ACT_CERT_POLICY_PREFIX = b"bb.sa1.activation_cert_policy.v1"


def build_successor_admission_manifest(
    new_profile_id: bytes,
    new_spec_dig: bytes,
    new_scope_d: bytes,
    dom_reg_d: bytes,
    upgrade_manifest_id: bytes,
) -> tuple[bytes, bytes]:
    ev_rows = [
        ("ASSET-1", "25fbbf645940461531d98d3c5f80235441df09f2", decode_tag_digest("#bb.receipt.sha256.d4c462nqb6gg3rzzrraufi4cjv752ys33gfmep44wkltiposlvkq"), "dual-domain-conservation-bedrock-excluded", "supply-overflow-or-deposit-leak"),
        ("BOUNDS-1", "ec5b76358b06351652fde5e9bed05a8d52bb5cad", decode_tag_digest("#bb.policy.sha256.yecxf5k5a5gs3y7qm76pzax2f5e6bkidmxytmsthsxhdqozzplaa"), "400ms-verify-budget-2mib-block-preflight", "unbounded-allocation-or-dos-saturation"),
        ("EXECUTION-ROOT-1", "6f66ea4652841f2c06b16fb73870ae049b1a2cef", decode_tag_digest("#bb.policy.sha256.3wkwyds7dgkm5hd4vwj26lkokfi6xkeft6d7yl3hzddm3tq6i62q"), "two-layer-11-slot-inactive-sentinel-composition", "cross-domain-root-substitution"),
        ("GENESIS-ROOT-PARITY-1", "ac4fe8a88896d474cf29e756913d02511fcc5257", decode_tag_digest("#bb.receipt.sha256.3xkh3lfjjgfxg52qhgoqw5zl34zr4d5zax5d6apqufpyiyrpysoa"), "4-way-preimage-verified-logical-to-typed-root-parity", "cold-incremental-executor-python-divergence"),
        ("GOVERNANCE-1", "35b26afd0f0abe8416a22edd009ce25f8e077be1", decode_tag_digest("#bb.receipt.sha256.ro25qjtz5x6jglbdtk4pzqtlm5tqwfkxfpod53hn2amne7otuhxq"), "timelocked-quorum-pause-only-council-no-rewrite", "unauthorized-upgrade-or-history-rewrite"),
        ("PROFILE-D1", "7a710fc26abf100c6e4e3f8ddb33058227127f84", decode_tag_digest("#bb.policy.sha256.r4ldc6k33p7evc4zg67ksajcpr6dxeiwuazimy6pxhwgaepepaoq"), "non-circular-ce43-candidate-manifest-7-subclaims", "self-referential-profile-id-or-scope-drift"),
        ("RESOURCE-1", "438f08e0b09f6ba40e829d46b25c9083b830475f", decode_tag_digest("#bb.policy.sha256.6yvp73mvdkouoxk2xiolnj33lbxucxta5ikhjcwnvd267tzfrfyq"), "eip1559-burn-capped-tip-refundable-deposit", "uncharged-state-growth-or-fee-diversion"),
        ("TRANSITION-SPEC-1", "27ede9a7c2b71930a8434f0c8744270134139d7f", decode_tag_digest("#bb.policy.sha256.xhy32kud2wdzknglo6p2n3gulbnlikjn3eqhmmquzl2jcofjg6wa"), "disjoint-256-tag-bands-inactive-vm-wall", "tag-collision-or-partial-write-rollback-leak"),
        ("UPGRADE-1", UPGRADE_1_MERGED_COMMIT_HEX, upgrade_manifest_id, "preventive-v1-to-v2-bounds-occurrence-handoff", "conservation-drift-or-downgrade-bypass"),
        ("UPGRADE-FRAMEWORK-1", "d283dedaac924065c80f560cb0d7b1c1f8fadb8b", decode_tag_digest("#bb.policy.sha256.udwdlgfcdx3lmezbw6ffttvoom2bqjv2tlyxnbekhmtswgeyy74q"), "preventive-lifecycle-terminal-row1-safe-halt", "compromised-authority-self-recovery"),
        ("VALIDATOR-ECON-1", "7ac86ef897b86f28de8a7abae111ea68994443c0", decode_tag_digest("#bb.receipt.sha256.kwwsv73gxzeuug576j55yktxgcdn6zo5zuv4pguns5ewzq4nukda"), "receipt-exact-consensus-weight-reward-slash-7-vectors", "phantom-stake-or-double-slash-bypass"),
    ]
    ev_rows.sort(key=lambda r: r[0])
    dig_by_cap = {cap: dig for cap, _, dig, _, _ in ev_rows}
    ce_rows = [
        ("CE-25", "PROFILE-1", "new-admission", 2, "asset-active", "ASSET-1", "implementation", dig_by_cap["ASSET-1"]),
        ("CE-27", "PROFILE-1", "new-admission", 2, "economic-charging", "RESOURCE-1", "policy", dig_by_cap["RESOURCE-1"]),
        ("CE-28", "PROFILE-1", "new-admission", 2, "validator-economics", "VALIDATOR-ECON-1", "authority", dig_by_cap["VALIDATOR-ECON-1"]),
        ("CE-29", "PROFILE-1", "new-admission", 2, "migration-profile", "UPGRADE-1", "migration", upgrade_manifest_id),
        ("CE-43", "PROFILE-D1", "new-candidate-profile", 2, "always", "PROFILE-SCOPE-D0", "candidate-scope", new_scope_d),
    ]
    slots = [(0, 1, 1), (1, 1, 1), (2, 1, 1), (3, 1, 1), (4, 1, 1), (5, 1, 1), (6, 0, 0), (7, 0, 0)]
    subclaims = [
        ("G15.economic", 1, "economic-charging", 0),
        ("G15.resource", 0, "", 0),
        ("G19.program", 2, "program-active-inactive-at-launch", 0),
        ("G19.upgrade", 0, "in-place-upgrade", 0),  # SuccessorCandidate -> Satisfied (0)
        ("G19.verkle", 2, "verkle-selected-inactive-at-launch", 0),
        ("G5.asset", 1, "asset-active", 0),
        ("G5.core", 0, "", 0),
    ]
    c = bytearray()
    c += struct.pack(">HBBH", 1, 1, 0, len(ev_rows))
    for cap, sha_hex, rdig, assume, loss in ev_rows:
        c += lp_str(cap) + struct.pack(">H", 1) + bytes.fromhex(sha_hex) + rdig + bytes([1])
        c += hashlib.sha256(PROF1_ASSUMPTION_PREFIX + cap.encode() + b":" + assume.encode()).digest()
        c += hashlib.sha256(PROF1_LOSS_PREFIX + cap.encode() + b":" + loss.encode()).digest()
    c += struct.pack(">H", len(ce_rows))
    for eid, ob, orole, ctx_c, pred, db, drole, ddig in ce_rows:
        c += lp_str(eid) + lp_str(ob) + lp_str(orole) + bytes([ctx_c]) + lp_str(pred) + lp_str(db) + lp_str(drole) + ddig
    c += struct.pack(">H", len(slots))
    for sid, act, cp in slots:
        c += struct.pack(">BBH", sid, act, cp)
    c += struct.pack(">H", len(subclaims))
    for scid, disp, por, evst in subclaims:
        c += lp_str(scid) + bytes([disp]) + lp_str(por) + bytes([evst])
    _, closure_dig = content_tag("policy", CLOSURE_SCHEMA, bytes(c))

    policy_id = hashlib.sha256(PROF1_POLICY_PREFIX + b"bitblock-profile-1-launch-policy-v1").digest()
    req_caps = sorted(cap for cap, _, _, _, _ in ev_rows)
    m = bytearray()
    m += struct.pack(">H", 1) + new_profile_id + new_spec_dig + new_scope_d + dom_reg_d + policy_id
    m += struct.pack(">H", len(req_caps))
    for cap in req_caps:
        m += lp_str(cap)
    m += closure_dig + bytes([1])
    adm_b = bytes(m)
    _, adm_dig = content_tag("receipt", PROFILE_MANIFEST_SCHEMA, adm_b)
    return adm_b, adm_dig


def issue_live_parent_cert_py(
    c1: dict,
    purpose_code: int,
    consumer_obj_dig: bytes,
    sign_dep_dig: bytes,
    cert_policy_id: bytes,
) -> tuple[bytes, bytes, bytes]:
    chain_id = c1["chain_id"]
    genesis_id = c1["genesis_id"]
    old_pid = c1["profile_id"]
    parent_lookup_id = c1["parent_lookup_id"]
    parent_core_id = c1["core_digest"]
    occ_rcpts = c1["occ_rcpts"]
    occ_univ_root = c1["occ_univ_root"]
    evid_commit_id = c1["evid_commit_id"]
    ep0_sched_digest = c1["ep0_sched_digest"]
    cqc_dig = c1["cqc_dig"]

    consumer_ctx = (
        struct.pack(">BQ", purpose_code, chain_id)
        + old_pid
        + consumer_obj_dig
        + struct.pack(">QQ", 1, 1)
        + sign_dep_dig
    )
    assert len(consumer_ctx) == 121
    attained_status = 2
    history_start = genesis_id * 3
    sign_doc = hashlib.sha256(
        CONS_CERT_SIGN_PREFIX
        + struct.pack(">H", 1)
        + old_pid
        + parent_lookup_id
        + parent_core_id
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
    cert_sdig = sign_domain_digest(chain_id, genesis_id, old_pid, 4, sign_doc)
    issuer_pairs = [(bytes([i]) * 32, ed_sign(bytes([100 + i]) * 32, cert_sdig)) for i in (1, 2, 3)]
    cert_bytes = (
        struct.pack(">H", 1)
        + old_pid
        + parent_lookup_id
        + parent_core_id
        + consumer_ctx
        + history_start
        + struct.pack(">QQQH", 2, 1, 1, len(occ_rcpts))
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
    _, cert_id = content_tag("receipt", CONSENSUS_PARENT_AUTH_CERT_SCHEMA, cert_bytes)
    adm_id = hashlib.sha256(CONS_CERT_ADMIT_PREFIX + cert_bytes).digest()
    return cert_bytes, cert_id, adm_id


def preflight_successor_bundle_py(raw: bytes) -> tuple[int, int, int, int]:
    """Independent zero-allocation streaming preflight mirroring Rust preflight."""
    if len(raw) > 1_048_576:
        raise ValueError("over bundle frame cap")
    p = 0

    def rd(n: int) -> bytes:
        nonlocal p
        if p + n > len(raw):
            raise ValueError("truncated")
        s = raw[p : p + n]
        p += n
        return s

    (ver,) = struct.unpack(">H", rd(2))
    if ver != 1:
        raise ValueError("bad version")
    old_pid = rd(32)
    new_pid = rd(32)
    if old_pid == bytes(32) or new_pid == bytes(32) or old_pid == new_pid:
        raise ValueError("bad profile polarity")
    rd(32 * 5)
    n_int, n_supp, n_edges, b_int = struct.unpack(">IIIQ", rd(20))
    if n_supp > 64 or n_edges > 256 or b_int > 1_048_576:
        raise ValueError("over cap")
    if n_int != 7 + n_supp:
        raise ValueError("7 + |S| mismatch")
    expl_lens = struct.unpack(">7Q", rd(56))
    sum_b = 0
    for l in expl_lens:
        if l == 0 or l > 262_144:
            raise ValueError("bad explicit len")
        sum_b += l
    for decl_l in expl_lens:
        mode = rd(1)[0]
        if mode != 1:
            raise ValueError("locator forbidden")
        (wl,) = struct.unpack(">Q", rd(8))
        if wl != decl_l:
            raise ValueError("explicit slice len mismatch")
        rd(wl)
    (n_nodes,) = struct.unpack(">I", rd(4))
    if n_nodes != n_int:
        raise ValueError("|D| != n_int")
    counted_edges = 0
    prev_nid = None
    for _ in range(n_nodes):
        rd(4)
        nid = rd(32)
        if prev_nid is not None and prev_nid >= nid:
            raise ValueError("nodes not sorted")
        prev_nid = nid
        (nd,) = struct.unpack(">H", rd(2))
        counted_edges += nd
        prev_dep = None
        for _ in range(nd):
            tag = rd(1)[0]
            if tag != 1:
                raise ValueError("external dep variant forbidden")
            did = rd(32)
            if prev_dep is not None and prev_dep >= did:
                raise ValueError("deps not sorted")
            prev_dep = did
    if counted_edges != n_edges:
        raise ValueError("edge count mismatch")
    (ns2,) = struct.unpack(">I", rd(4))
    if ns2 != n_supp:
        raise ValueError("|S| mismatch")
    prev_sid = None
    for _ in range(ns2):
        if rd(1)[0] != 1:
            raise ValueError("locator in S forbidden")
        (slen,) = struct.unpack(">H", rd(2))
        if slen == 0:
            raise ValueError("empty schema")
        rd(slen)
        sid = rd(32)
        if prev_sid is not None and prev_sid >= sid:
            raise ValueError("supp not sorted")
        prev_sid = sid
        (pl,) = struct.unpack(">Q", rd(8))
        if pl == 0 or pl > 262_144:
            raise ValueError("bad supp len")
        sum_b += pl
        rd(pl)
    if sum_b != b_int:
        raise ValueError("internal preimage byte sum mismatch")
    if p != len(raw):
        raise ValueError("trailing bytes")
    return n_int, n_supp, n_edges, b_int


def build_launch_height1_consensus_context(
    chain_id: int,
    world_id: int,
    genesis_id: bytes,
    old_pid: bytes,
    old_spec_dig: bytes,
    genesis_history_root: bytes,
    old_exec_ref: bytes,
    launch_bundle_id: bytes,
) -> dict:
    """Reproduces the exact height-1 HotStuff commit cert & split-wrapper occurrence universe
    directly over CanonicalDevnetGenesisFixtureV1 (exact launch genesis_id & old_profile_id)."""
    gen_vals = [
        (
            bytes([i]) * 32,
            ed_public_from_seed(bytes([100 + i]) * 32),
            ed_public_from_seed(bytes([200 + i]) * 32),
            100_000_000_000,
        )
        for i in range(1, 5)
    ]
    gov_params = [(1, 2_000), (2, 100_000_000)]
    gov_root = DomainPatriciaPy(
        DOM_GOVERNANCE, {param_trie_key(k): struct.pack(">Q", v) for k, v in gov_params}
    ).root()
    econ_root = DomainPatriciaPy(
        DOM_ECONOMIC_STAKE,
        {vid: encode_val_record(stake, 0, 0, False, 0, []) for vid, _, _, stake in gen_vals},
    ).root()
    gov_root_sha = hashlib.sha256(
        encode_gov_root(old_pid, 0, 0, 0, 0, 0, bytes(32), 0, len(gov_params), gov_root)
    ).digest()
    bonded_sum = 400_000_000_000
    econ_root_sha = hashlib.sha256(
        encode_econ_root(old_pid, 0, 4, 4, bonded_sum, 0, 0, 0, 0, bonded_sum, 0, 0, econ_root)
    ).digest()
    ep0_sched_b = encode_epoch_schedule(0, 0, econ_root_sha, gov_root_sha, 2500, gen_vals)
    _, ep0_sched_digest = content_tag("receipt", SCHED_EPOCH_SCHEMA, ep0_sched_b)

    fork_ctx = struct.pack(">QQ", chain_id, world_id) + genesis_id + old_pid + old_spec_dig
    parent_block_id = bytes(32)
    _, parent_lookup_id = content_tag(
        "receipt", PARENT_LOOKUP_SCHEMA, CONS_PARENT_LOOKUP_PREFIX + genesis_id + parent_block_id
    )
    core_bytes = encode_exact_core(
        parent_lookup_id,
        2,
        genesis_id + old_pid,
        struct.pack(">QQQ", 1, 0, 0) + genesis_history_root + old_exec_ref,
        launch_bundle_id,
    )
    _, core_digest = content_tag("receipt", EXACT_CORE_SCHEMA, core_bytes)

    leader_id = bytes([2]) * 32
    prop_unsigned = hashlib.sha256(
        CONS_PROP_PAYLOAD_PREFIX
        + struct.pack(">HQQQ", 1, 1, 0, 0)
        + parent_block_id
        + fork_ctx
        + ep0_sched_digest
        + launch_bundle_id
        + genesis_history_root
        + core_digest
        + old_exec_ref
        + bytes(32)
        + struct.pack(">Q", 0)
        + leader_id
    ).digest()
    block_id = hashlib.sha256(CONS_BLOCK_ID_PREFIX + prop_unsigned + leader_id).digest()
    exec_ref_dig = hashlib.sha256(CONS_EXEC_ROOT_DIGEST_PREFIX + old_exec_ref).digest()

    votes_raw = []
    for i in (1, 2, 3, 4):
        vid = bytes([i]) * 32
        vseed = bytes([100 + i]) * 32
        vpk = ed_public_from_seed(vseed)
        v_uns = hashlib.sha256(
            CONS_VOTE_PAYLOAD_PREFIX
            + struct.pack(">HQQQB", 1, 1, 0, 0, 2)
            + block_id
            + ep0_sched_digest
            + exec_ref_dig
            + b"\x01"
            + vid
        ).digest()
        vsdig = sign_domain_digest(chain_id, genesis_id, old_pid, 4, v_uns)
        vsig = ed_sign(vseed, vsdig)
        votes_raw.append(
            struct.pack(">HQQQB", 1, 1, 0, 0, 2)
            + block_id
            + ep0_sched_digest
            + exec_ref_dig
            + b"\x01"
            + vid
            + vpk
            + vsig
        )
    cqc_bytes = (
        struct.pack(">HQQQB", 1, 1, 0, 0, 2)
        + block_id
        + ep0_sched_digest
        + exec_ref_dig
        + u128_be(bonded_sum)
        + u128_be(bonded_sum)
        + struct.pack(">H", len(votes_raw))
        + b"".join(votes_raw)
    )
    cqc_dig = hashlib.sha256(CONS_QC_DIGEST_PREFIX + cqc_bytes).digest()

    wrap_sign_dig = sign_domain_digest(chain_id, genesis_id, old_pid, 3, core_digest)

    def mk_rcpt(ordinal: int, v_idx: int, ev_b: int, dep_b: int, av_b: int) -> bytes:
        pseed = bytes([200 + v_idx]) * 32
        pk = ed_public_from_seed(pseed)
        sig = ed_sign(pseed, wrap_sign_dig)
        creds = [(pk, sig)]
        evs = [bytes([ev_b]) * 32]
        deps = [(bytes([dep_b]) * 32, 1024)]
        wb = encode_wrapper(core_bytes, creds, evs, deps)
        _, wdig = content_tag("receipt", WRAPPER_SCHEMA, wb)
        return encode_occurrence_receipt(
            ordinal, parent_lookup_id, core_digest, wdig, creds, evs, bytes([av_b]) * 32, deps, False
        )

    occ_rcpts = [mk_rcpt(1, 1, 0xA1, 0xB1, 0xC1), mk_rcpt(2, 2, 0xA2, 0xB2, 0xC2)]
    occ_univ_root = hashlib.sha256(
        CONS_CERT_UNIV_PREFIX + struct.pack(">I", len(occ_rcpts)) + b"".join(occ_rcpts)
    ).digest()
    evid_commit_id = hashlib.sha256(
        CONS_CERT_EVID_PREFIX + (genesis_id * 3) + struct.pack(">QQQ", 2, 1, 1) + occ_univ_root
    ).digest()
    return {
        "chain_id": chain_id,
        "genesis_id": genesis_id,
        "profile_id": old_pid,
        "parent_lookup_id": parent_lookup_id,
        "core_digest": core_digest,
        "occ_rcpts": occ_rcpts,
        "occ_univ_root": occ_univ_root,
        "evid_commit_id": evid_commit_id,
        "ep0_sched_digest": ep0_sched_digest,
        "cqc_dig": cqc_dig,
    }


def build_successor_activation_vectors(include_supplementary: bool) -> dict:
    cl = build_chain_launch_vectors(True)
    chain_launch_manifest_id = decode_tag_digest(cl["manifest_tag"])
    launch_bundle_id = decode_tag_digest(cl["bundle_tag"])
    u1 = build_upgrade_1_vectors()

    raw = u1["parity"]["raw"]
    chain_id = 77
    genesis_id = raw["genesis_id"]
    old_pid = raw["profile_id"]
    c1 = build_launch_height1_consensus_context(
        chain_id,
        1,
        genesis_id,
        old_pid,
        raw["spec_dig"],
        raw["genesis_history_root"],
        raw["exec_root_ref"],
        launch_bundle_id,
    )
    dom_reg_b, dom_reg_d = build_commit_domain_registry_bytes()
    assert dom_reg_d == raw["commit_dom_reg_d"]
    parity_record_id = raw["parity_record_id_bytes"]

    new_scope_b = u1["new_scope_b"]
    new_scope_d = u1["new_scope_d"]
    new_bound_b = u1["new_bound_b"]
    new_pid = u1["new_profile_id"]
    new_spec_b = u1["new_spec_b"]
    new_spec_d = u1["new_spec_dig"]
    mig_spec_b = u1["mig_spec_b"]
    mig_spec_id = u1["mig_spec_id"]
    ev_manifest_b = u1["manifest_b"]
    ev_manifest_id = u1["manifest_id"]
    old_exec_ref = raw["exec_root_ref"]
    new_exec_ref = u1["new_exec_ref"]
    delta_dig = u1["delta_dig"]

    # 1. Successor ProfileAdmissionManifestV1
    succ_adm_b, succ_adm_id = build_successor_admission_manifest(
        new_pid, new_spec_d, new_scope_d, dom_reg_d, ev_manifest_id
    )

    # 2. Successor CandidateBuildInputsV1 & CandidateReleaseReceiptV1
    fw = build_framework_manifest()
    fm = fw["manifest"]
    fw_sha = bytes.fromhex(fw["framework_manifest_sha256"])
    rust_tc_sha = bytes.fromhex(fm["rust_toolchain_sha256"])
    lean_tc_sha = bytes.fromhex(fm["lean_toolchain_sha256"])
    cargo_lock_sha = bytes.fromhex(fm["cargo_lock_sha256"])
    sbom_sha = bytes.fromhex(fm["sbom_sha256"])
    wasm_sha = bytes.fromhex(fm["committed_wasm_sha256"])
    src_dig = hashlib.sha256(RELR1_SOURCE_PREFIX + b"bitblock-launch-candidate-v1-tree").digest()
    feat_dig = hashlib.sha256(RELR1_FEATURE_PREFIX + b"default+protocol-research").digest()
    cfg_dig = hashlib.sha256(
        RELR1_CONFIG_PREFIX + b"release+locked+remap-path-prefix+wasm32-simd128-panic-abort"
    ).digest()
    inputs_b = (
        struct.pack(">H", 1) + new_pid + genesis_id + dom_reg_d + fw_sha + src_dig
        + rust_tc_sha + lean_tc_sha + cargo_lock_sha + sbom_sha
        + struct.pack(">II", 220, 11) + feat_dig + cfg_dig + bytes([1])
    )
    _, inputs_id = content_tag("policy", BUILD_INPUTS_SCHEMA, inputs_b)
    nat_dig = hashlib.sha256(b"bb.relr1.canonical_native_elf_fixture.v1").digest()
    rcpt_b = (
        struct.pack(">H", 1) + inputs_id + new_pid + genesis_id + fw_sha + nat_dig
        + struct.pack(">Q", 4_194_304) + wasm_sha + struct.pack(">QI", 1_831_314, 2) + bytes([1, 1, 0])
    )
    _, rcpt_id = content_tag("receipt", CANDIDATE_RECEIPT_SCHEMA, rcpt_b)

    # 3. Stage 2 ReleaseAuthority live cert + successor AdmittedReleaseRecordV1
    parent_lookup_id = c1["parent_lookup_id"]
    parent_core_id = c1["core_digest"]
    predecessor_gov_root = hashlib.sha256(
        SA1_GOV_AUTH_ROOT_PREFIX + genesis_id + old_pid + parent_core_id
    ).digest()

    rel_keys = sorted(
        [(ed_public_from_seed(bytes([s]) * 32), bytes([s]) * 32) for s in (41, 42, 43, 44, 45)],
        key=lambda x: x[0],
    )
    h_rpol = hashlib.sha256(REL1_POLICY_PREFIX + bytes([3, 5]))
    for pk, _ in rel_keys:
        h_rpol.update(pk)
    rel_policy_id = h_rpol.digest()

    rel_ctx_ref = bytes([2]) + parent_lookup_id + bytes([2]) + predecessor_gov_root
    rel_core_b = (
        struct.pack(">H", 1) + new_pid + new_spec_d + succ_adm_id + genesis_id + parity_record_id
        + inputs_id + rcpt_id + fw_sha + nat_dig + struct.pack(">Q", 4_194_304)
        + wasm_sha + struct.pack(">Q", 1_831_314) + bytes([1]) + rel_ctx_ref + rel_policy_id + bytes([1])
    )
    assert len(rel_core_b) == 438
    _, rel_core_id = content_tag("policy", CORE_SCHEMA, rel_core_b)

    rel_cert_pol = hashlib.sha256(SA1_RELEASE_CERT_POLICY_PREFIX).digest()
    _, rel_cert_id, _ = issue_live_parent_cert_py(c1, 1, rel_core_id, mig_spec_id, rel_cert_pol)

    rel_hist_ref = (
        bytes([2]) + rel_cert_id + c1["evid_commit_id"] + c1["occ_univ_root"]
        + struct.pack(">QQB", 1, 0, 2)
    )
    assert len(rel_hist_ref) == 114
    rel_sd_b = struct.pack(">H", 1) + rel_core_id + rel_ctx_ref + rel_hist_ref + rel_policy_id
    assert len(rel_sd_b) == 246
    _, rel_sd_id = content_tag("policy", REL_SIGN_DOC_SCHEMA, rel_sd_b)
    rel_env_b = struct.pack(">I", len(rel_sd_b)) + rel_sd_b
    _, rel_env_dig = content_tag("policy", AUTH_SIGN_DOC_SCHEMA, rel_env_b)
    rel_pre = encode_signing_preimage(1, chain_id, genesis_id, bytes(32), new_pid, 9, 1, 1, rel_env_dig)
    _, rel_pre_dig = content_tag("receipt", SIGNING_PREIMAGE_SCHEMA, rel_pre)
    rel_sigs_b = bytearray()
    for idx in (0, 1, 2):
        pk, seed = rel_keys[idx]
        sig = ed_sign(seed, rel_pre_dig)
        assert ed_verify_strict(pk, rel_pre_dig, sig)
        rel_sigs_b += bytes([idx]) + pk + sig
    succ_rel_b = (
        struct.pack(">H", 1) + rel_core_b + rel_core_id + struct.pack(">H", 1) + rel_sd_b
        + rel_sd_id + struct.pack(">H", 1) + rel_policy_id + struct.pack(">H", 3) + bytes(rel_sigs_b)
    )
    _, succ_rel_id = content_tag("receipt", RECORD_SCHEMA, succ_rel_b)

    # 4. Internal-only SuccessorActivationBundleV1
    supp_list = []
    extra_nodes = []
    ev_deps = [new_pid, new_spec_d]
    rel_deps = [new_pid, new_spec_d, succ_adm_id]
    if include_supplementary:
        supp_list = [
            (MIGRATION_SPEC_SCHEMA, mig_spec_id, mig_spec_b),
            (BUILD_INPUTS_SCHEMA, inputs_id, inputs_b),
            (CANDIDATE_RECEIPT_SCHEMA, rcpt_id, rcpt_b),
        ]
        supp_list.sort(key=lambda x: x[1])
        extra_nodes = [
            (4, mig_spec_id, sorted([new_pid, new_spec_d])),
            (4, inputs_id, [new_pid]),
            (5, rcpt_id, sorted([new_pid, inputs_id])),
        ]
        ev_deps.append(mig_spec_id)
        rel_deps.extend([inputs_id, rcpt_id])

    nodes = [
        (0, dom_reg_d, []),
        (1, new_scope_d, [dom_reg_d]),
        (2, new_pid, sorted([dom_reg_d, new_scope_d])),
        (3, new_spec_d, sorted([new_scope_d, new_pid])),
        (5, ev_manifest_id, sorted(set(ev_deps))),
        (6, succ_adm_id, sorted([dom_reg_d, new_scope_d, new_pid, new_spec_d, ev_manifest_id])),
        (7, succ_rel_id, sorted(set(rel_deps))),
    ] + extra_nodes
    nodes.sort(key=lambda x: x[1])

    explicit = [new_scope_b, dom_reg_b, new_bound_b, new_spec_b, succ_adm_b, succ_rel_b, ev_manifest_b]
    expl_lens = [len(e) for e in explicit]
    n_supp = len(supp_list)
    n_int = 7 + n_supp
    n_edges = sum(len(d) for _, _, d in nodes)
    b_int = sum(expl_lens) + sum(len(s[2]) for s in supp_list)

    bb = bytearray()
    bb += struct.pack(">H", 1) + old_pid + new_pid + succ_adm_id + new_spec_d + succ_rel_id + mig_spec_id + ev_manifest_id
    bb += struct.pack(">IIIQ", n_int, n_supp, n_edges, b_int)
    bb += struct.pack(">7Q", *expl_lens)
    for e in explicit:
        bb += bytes([1]) + struct.pack(">Q", len(e)) + e
    bb += struct.pack(">I", len(nodes))
    for rank, nid, deps in nodes:
        bb += struct.pack(">I", rank) + nid + struct.pack(">H", len(deps))
        for did in deps:
            bb += bytes([1]) + did
    bb += struct.pack(">I", len(supp_list))
    for schema, sid, pre_b in supp_list:
        bb += bytes([1]) + struct.pack(">H", len(schema)) + schema + sid + struct.pack(">Q", len(pre_b)) + pre_b
    bundle_b = bytes(bb)
    assert preflight_successor_bundle_py(bundle_b) == (n_int, n_supp, n_edges, b_int)
    bundle_tag, bundle_id = content_tag("receipt", BUNDLE_SCHEMA, bundle_b)

    # 5. Stage 3 ActivationAuthority live cert (ONLY AFTER bundle_id!) + SignDoc + 3-of-5 Auth
    act_cert_pol = hashlib.sha256(SA1_ACT_CERT_POLICY_PREFIX).digest()
    _, act_cert_id, act_cert_adm_id = issue_live_parent_cert_py(
        c1, 2, bundle_id, succ_rel_id, act_cert_pol
    )
    assert act_cert_id != rel_cert_id

    act_keys = sorted(
        [(ed_public_from_seed(bytes([s]) * 32), bytes([s]) * 32) for s in (61, 62, 63, 64, 65)],
        key=lambda x: x[0],
    )
    h_apol = hashlib.sha256(SA1_ACTIVATION_POLICY_PREFIX + bytes([3, 5]))
    for pk, _ in act_keys:
        h_apol.update(pk)
    act_policy_id = h_apol.digest()

    hist_act_ref = (
        act_cert_id + act_cert_adm_id + c1["evid_commit_id"] + c1["occ_univ_root"]
        + struct.pack(">QBBHI", 1, 2, 2, 0, len(c1["occ_rcpts"]))
    )
    assert len(hist_act_ref) == 144
    activation_height = 4
    act_sd_b = (
        struct.pack(">H", 1) + chain_launch_manifest_id + parent_lookup_id + bytes([2])
        + predecessor_gov_root + hist_act_ref + old_pid + new_pid + succ_adm_id
        + mig_spec_id + bundle_id + struct.pack(">Q", activation_height)
        + old_exec_ref + new_exec_ref + act_policy_id
    )
    assert len(act_sd_b) == 799
    sd_tag, act_sd_id = content_tag("policy", SIGN_DOC_SCHEMA, act_sd_b)

    act_pre = encode_signing_preimage(
        1, chain_id, genesis_id, chain_launch_manifest_id, old_pid, 7, 1, 1, act_sd_id
    )
    _, act_pre_dig = content_tag("receipt", SIGNING_PREIMAGE_SCHEMA, act_pre)
    act_sigs_b = bytearray()
    for idx in (0, 1, 2):
        pk, seed = act_keys[idx]
        sig = ed_sign(seed, act_pre_dig)
        assert ed_verify_strict(pk, act_pre_dig, sig)
        act_sigs_b += bytes([idx]) + pk + sig
    auth_b = (
        struct.pack(">H", 1) + act_sd_b + act_sd_id + act_policy_id + struct.pack(">H", 1)
        + act_policy_id + struct.pack(">H", 3) + bytes(act_sigs_b)
    )
    auth_tag, auth_id = content_tag("receipt", AUTH_SCHEMA, auth_b)

    # 6. DedicatedMigrationBodyV1 + availability commitment + Carrier
    body_b = (
        struct.pack(">IIH", 0, 0, 1)
        + succ_adm_id + mig_spec_id + bundle_id + auth_id + struct.pack(">Q", activation_height)
        + delta_dig
    )
    avail_commit = hashlib.sha256(
        SA1_AVAIL_COMMIT_PREFIX
        + old_pid
        + struct.pack(">Q", len(bundle_b)) + bundle_b
        + struct.pack(">Q", len(auth_b)) + auth_b
        + struct.pack(">Q", len(body_b)) + body_b
    ).digest()
    carrier_b = (
        struct.pack(">HBI", 1, 1, len(bundle_b)) + bundle_b + bundle_id
        + struct.pack(">I", len(auth_b)) + auth_b + auth_id
        + struct.pack(">I", len(body_b)) + body_b + avail_commit
    )
    carrier_tag, carrier_id = content_tag("receipt", CARRIER_SCHEMA, carrier_b)
    return {
        "bundle_b": bundle_b,
        "bundle_tag": bundle_tag,
        "bundle_id": bundle_id,
        "sd_tag": sd_tag,
        "auth_tag": auth_tag,
        "carrier_b": carrier_b,
        "carrier_tag": carrier_tag,
        "carrier_id": carrier_id,
        "n_int": n_int,
        "n_supp": n_supp,
        "n_edges": n_edges,
        "b_int": b_int,
    }


def main() -> int:
    s3 = build_successor_activation_vectors(True)
    s0 = build_successor_activation_vectors(False)
    assert (s3["n_int"], s3["n_supp"], s3["n_edges"]) == (10, 3, 23)
    assert (s0["n_int"], s0["n_supp"], s0["n_edges"]) == (7, 0, 15)

    # Negative preflight controls
    for label, off, val in [
        ("off_by_one_internal", 2 + 32 * 7 + 3, bytes([s3["bundle_b"][2 + 32 * 7 + 3] ^ 1])),
        ("off_by_one_edges", 2 + 32 * 7 + 11, bytes([s3["bundle_b"][2 + 32 * 7 + 11] ^ 1])),
        ("u64_max_expl0", 2 + 32 * 7 + 20, struct.pack(">Q", (1 << 64) - 1)),
        ("locator_mode", 2 + 32 * 7 + 20 + 56, b"\x02"),
    ]:
        mut = bytearray(s3["bundle_b"])
        mut[off : off + len(val)] = val
        try:
            preflight_successor_bundle_py(bytes(mut))
        except ValueError:
            pass
        else:
            raise AssertionError(f"negative control {label} unexpectedly passed")

    if "--print-tags" in sys.argv:
        print("BUNDLE_S3_TAG  =", s3["bundle_tag"])
        print("SIGN_DOC_TAG   =", s3["sd_tag"])
        print("AUTH_TAG       =", s3["auth_tag"])
        print("CARRIER_S3_TAG =", s3["carrier_tag"])
        print("CARRIER_S0_TAG =", s0["carrier_tag"])
        return 0

    assert s3["bundle_tag"] == EXPECTED_BUNDLE_S3_TAG, s3["bundle_tag"]
    assert s3["sd_tag"] == EXPECTED_SIGN_DOC_TAG, s3["sd_tag"]
    assert s3["auth_tag"] == EXPECTED_AUTH_TAG, s3["auth_tag"]
    assert s3["carrier_tag"] == EXPECTED_CARRIER_S3_TAG, s3["carrier_tag"]
    assert s0["carrier_tag"] == EXPECTED_CARRIER_S0_TAG, s0["carrier_tag"]
    print(
        f"successor_activation_1_vectors: PASS (s3={s3['carrier_tag'][:26]}..., s0={s0['carrier_tag'][:26]}...)"
    )
    return 0


EXPECTED_BUNDLE_S3_TAG = "#bb.receipt.sha256.cxa644n2sko6qt6o7duozk233o44gcnnyewn5fmi675sa3xxdowa"
EXPECTED_SIGN_DOC_TAG = "#bb.policy.sha256.6iyowm3ua37gjffzadytwdxkfuzpczstvicug62kmoevvyjwfamq"
EXPECTED_AUTH_TAG = "#bb.receipt.sha256.75nq5siuuiml4z32d2rspnijjz5vub3rt7nejvmpeks3zdzvfnla"
EXPECTED_CARRIER_S3_TAG = "#bb.receipt.sha256.rsp5f3zibybn6kywjbeqjld2i6ccuta6daczhzkwyqacfc22klwq"
EXPECTED_CARRIER_S0_TAG = "#bb.receipt.sha256.7jwabo3fyxhattv2hjveqsahguti3sdkc3eyivy6352cnf7hrnda"

if __name__ == "__main__":
    raise SystemExit(main())
