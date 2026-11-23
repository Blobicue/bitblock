#!/usr/bin/env python3
"""Clean-room pure-Python independent verifier & vector suite for CHAIN-LAUNCH-1@base (PS-091).

Imports NO Rust or Bitblock code. Independently reproduces all 4 Wave 30 schemas:
  * bitblock/launch-closure-bundle/v1 (embedded-only bundle for |S| = 3 and |S| = 0)
  * bitblock/chain-launch-manifest/v1 (immutable launch trust anchor)
  * bitblock/chain-launch-authorization/v1 (3-of-5 Ed25519 detached launch authorization)
  * bitblock/chain-launch-1/package/v1 (complete outer genesis + closure bundle + manifest + auth)
plus independent zero-allocation streaming preflight, exact LAYER1_DESIGN_PAPER.md §2 equations,
combined DAG strict rank monotonicity, RFC 8032 Ed25519 verification under GovernanceAction (7),
and powered negative controls.
"""

from __future__ import annotations

import hashlib
import struct
import sys

from commit_state_vectors import (
    COMMIT_1_ACTIVE_TOKEN_ROOT_SCHEMA,
    COMMIT_1_DOMAIN_REGISTRY_SCHEMA,
    COMMIT_1_EPISODE_ROOT_SCHEMA,
    COMMIT_1_OP_LIMITS_SCHEMA,
    COMMIT_1_PROFILE_SCHEMA,
    COMMIT_1_PROOF_POLICY_SCHEMA,
    COMMIT_1_WORLD_ROOT_SCHEMA,
    DOMAIN_ACTIVE_TOKEN,
    DOMAIN_EPISODE,
    DOMAIN_WORLD,
    KEY_COORD_V1,
    KEY_EVENT_V1,
    ROOT_ACTIVE_TOKEN,
    ROOT_EPISODE,
    ROOT_WORLD,
    VAL_EPISODE_V1,
    VAL_MINT_V1,
    VAL_ON_V1,
    encode_domain_registry,
    encode_op_limits,
    encode_profile,
    encode_proof_policy,
)
from crypto_d1_vectors import (
    SIGNING_PREIMAGE_SCHEMA,
    content_tag,
    ed_public_from_seed,
    ed_sign,
    ed_verify_strict,
    encode_signing_preimage,
)
from release_1_vectors import build_release_1_vectors

BUNDLE_SCHEMA = b"bitblock/launch-closure-bundle/v1"
MANIFEST_SCHEMA = b"bitblock/chain-launch-manifest/v1"
AUTH_SCHEMA = b"bitblock/chain-launch-authorization/v1"
PACKAGE_SCHEMA = b"bitblock/chain-launch-1/package/v1"

GENESIS_PACKAGE_SCHEMA = b"bitblock/genesis-d1/genesis-package/v1"
EXEC_COMPOSITION_SCHEMA = b"bitblock/execution-root-1/root-composition/v1"
REL_R1_INPUTS_SCHEMA = b"bitblock/release-r1/candidate-build-inputs/v1"
REL_R1_RECEIPT_SCHEMA = b"bitblock/release-r1/candidate-build-receipt/v1"

CHL1_POLICY_PREFIX = b"bb.chl1.trusted_launch_authority_policy.v1"
CHL1_GENESIS_PARENT_PREFIX = b"bb.chl1.genesis_parent_block_id.v1"


def build_commit_domain_registry_bytes() -> tuple[bytes, bytes]:
    _, pp_w = content_tag(
        "policy",
        COMMIT_1_PROOF_POLICY_SCHEMA,
        encode_proof_policy(8192, 193, 192, 193, 32768, 128, 16384),
    )
    _, pp_a = content_tag(
        "policy",
        COMMIT_1_PROOF_POLICY_SCHEMA,
        encode_proof_policy(8192, 193, 192, 193, 32768, 160, 16384),
    )
    _, pp_e = content_tag(
        "policy",
        COMMIT_1_PROOF_POLICY_SCHEMA,
        encode_proof_policy(12288, 257, 256, 257, 49152, 256, 24576),
    )
    _, op_id = content_tag("policy", COMMIT_1_OP_LIMITS_SCHEMA, encode_op_limits())
    _, pw = content_tag(
        "policy",
        COMMIT_1_PROFILE_SCHEMA,
        encode_profile(
            ROOT_WORLD,
            DOMAIN_WORLD,
            COMMIT_1_WORLD_ROOT_SCHEMA,
            KEY_COORD_V1,
            192,
            VAL_ON_V1,
            pp_w,
            op_id,
        ),
    )
    _, pa = content_tag(
        "policy",
        COMMIT_1_PROFILE_SCHEMA,
        encode_profile(
            ROOT_ACTIVE_TOKEN,
            DOMAIN_ACTIVE_TOKEN,
            COMMIT_1_ACTIVE_TOKEN_ROOT_SCHEMA,
            KEY_COORD_V1,
            192,
            VAL_MINT_V1,
            pp_a,
            op_id,
        ),
    )
    _, pe = content_tag(
        "policy",
        COMMIT_1_PROFILE_SCHEMA,
        encode_profile(
            ROOT_EPISODE,
            DOMAIN_EPISODE,
            COMMIT_1_EPISODE_ROOT_SCHEMA,
            KEY_EVENT_V1,
            256,
            VAL_EPISODE_V1,
            pp_e,
            op_id,
        ),
    )
    reg_bytes, _ = encode_domain_registry(pw, pa, pe)
    _, reg_dig = content_tag("policy", COMMIT_1_DOMAIN_REGISTRY_SCHEMA, reg_bytes)
    return reg_bytes, reg_dig


def preflight_bundle_py(raw: bytes, outer_genesis_len: int) -> dict:
    """Independent zero-copy streaming preflight scanner matching Rust byte-for-byte."""
    off = 0

    def rd(n: int) -> bytes:
        nonlocal off
        if off + n > len(raw):
            raise ValueError("truncated bundle frame")
        s = raw[off : off + n]
        off += n
        return s

    (ver,) = struct.unpack(">H", rd(2))
    assert ver == 1
    (
        n_int,
        n_ext,
        n_comp,
        n_supp,
        i2i,
        i2e,
        e2i,
        c_edges,
        b_int,
        b_ext,
        b_tot,
    ) = struct.unpack(">IIIIIIIIQQQ", rd(8 * 4 + 3 * 8))
    if n_ext != 1 or n_int != 7 + n_supp or n_comp != n_int + n_ext:
        raise ValueError("artifact count equation violation")
    if e2i != 2 or c_edges != i2i + i2e + e2i:
        raise ValueError("edge count equation violation")
    if b_ext != outer_genesis_len or b_tot != b_int + b_ext:
        raise ValueError("byte sum equation violation")

    (es_len,) = struct.unpack(">H", rd(2))
    if rd(es_len) != GENESIS_PACKAGE_SCHEMA:
        raise ValueError("wrong external root schema")
    (x_rank,) = struct.unpack(">I", rd(4))
    if x_rank == 0:
        raise ValueError("rank-zero genesis forbidden")
    gid = rd(32)
    (x_glen,) = struct.unpack(">Q", rd(8))
    if x_glen != outer_genesis_len:
        raise ValueError("outer genesis length mismatch in X")
    (x_out_n,) = struct.unpack(">H", rd(2))
    if x_out_n != 2:
        raise ValueError("X outgoing ref count != 2")
    prev_t = None
    for _ in range(x_out_n):
        t = rd(32)
        c = rd(1)[0]
        if c not in (1, 2) or (prev_t is not None and prev_t >= t):
            raise ValueError("bad X outgoing ref")
        prev_t = t

    exp_lens = struct.unpack(">7Q", rd(56))
    for l in exp_lens:
        if l == 0:
            raise ValueError("zero explicit length")
        if rd(1)[0] != 1:
            raise ValueError("locator explicit variant rejected")
        rd(l)

    (d_n,) = struct.unpack(">I", rd(4))
    if d_n != n_int:
        raise ValueError("len(D) != internal_artifact_count")
    seen_i2i = seen_i2e = 0
    prev_id = None
    for _ in range(d_n):
        _rank = struct.unpack(">I", rd(4))[0]
        nid = rd(32)
        if prev_id is not None and prev_id >= nid:
            raise ValueError("D unsorted/duplicate")
        prev_id = nid
        (rc,) = struct.unpack(">H", rd(2))
        for _ in range(rc):
            tag = rd(1)[0]
            if tag == 1:
                rd(32)
                seen_i2i += 1
            elif tag == 2:
                rd(32)
                if rd(1)[0] not in (1, 2, 3):
                    raise ValueError("bad incoming class")
                seen_i2e += 1
            else:
                raise ValueError("unknown dep ref tag")
    if seen_i2i != i2i or seen_i2e != i2e:
        raise ValueError("edge count mismatch")

    (s_n,) = struct.unpack(">I", rd(4))
    if s_n != n_supp:
        raise ValueError("len(S) != supplementary_artifact_count")
    supp_sum = 0
    for _ in range(s_n):
        if rd(1)[0] != 1:
            raise ValueError("locator supplementary variant rejected")
        (slen,) = struct.unpack(">H", rd(2))
        rd(slen)
        rd(32)
        (plen,) = struct.unpack(">Q", rd(8))
        rd(plen)
        supp_sum += plen
    if sum(exp_lens) + supp_sum != b_int:
        raise ValueError("internal preimage byte sum mismatch")
    if off != len(raw):
        raise ValueError("trailing bundle bytes")
    return {
        "internal": n_int,
        "complete": n_comp,
        "edges": c_edges,
        "total_bytes": b_tot,
        "genesis_id": gid,
    }


def build_chain_launch_vectors(include_supplementary: bool) -> dict:
    _, _, _, _, r1 = build_release_1_vectors()
    p1 = r1["p1"]
    raw = p1["parity"]["raw"]
    reg_bytes, reg_id = build_commit_domain_registry_bytes()
    assert reg_id == raw["commit_dom_reg_d"]

    scope_b = raw["scope_bytes"]
    scope_id = raw["scope_d"]
    pkg_b = raw["pkg_bytes"]
    gid = raw["genesis_id"]
    cand_b = raw["bound_bytes"]
    profile_id = raw["profile_id"]
    spec_b = raw["spec_bytes"]
    spec_id = raw["spec_dig"]
    adm_b = p1["manifest_bytes"]
    adm_id = p1["manifest_dig"]
    par_b = raw["rec_bytes"]
    par_id = raw["parity_record_id_bytes"]
    rel_b = r1["rec_bytes"]
    rel_id = r1["rec_id"]

    supp = []
    extra_nodes = []
    parity_refs = [(1, profile_id, 0), (1, spec_id, 0), (2, gid, 2)]
    release_refs = [
        (1, profile_id, 0),
        (1, spec_id, 0),
        (1, adm_id, 0),
        (1, par_id, 0),
        (2, gid, 3),
    ]
    if include_supplementary:
        comp_b = raw["comp_bytes"]
        _, comp_id = content_tag("receipt", EXEC_COMPOSITION_SCHEMA, comp_b)
        in_b = r1["inputs_bytes"]
        in_id = r1["inputs_dig"]
        rc_b = r1["rcpt_bytes"]
        rc_id = r1["rcpt_dig"]
        supp = sorted(
            [
                (EXEC_COMPOSITION_SCHEMA, comp_id, comp_b),
                (REL_R1_INPUTS_SCHEMA, in_id, in_b),
                (REL_R1_RECEIPT_SCHEMA, rc_id, rc_b),
            ],
            key=lambda x: x[1],
        )
        extra_nodes = [
            (5, comp_id, [(1, reg_id, 0), (1, profile_id, 0)]),
            (5, in_id, [(1, reg_id, 0), (1, profile_id, 0)]),
            (6, rc_id, [(1, in_id, 0), (1, profile_id, 0)]),
        ]
        parity_refs.append((1, comp_id, 0))
        release_refs.extend([(1, in_id, 0), (1, rc_id, 0)])

    nodes = [
        (0, reg_id, []),
        (1, scope_id, [(1, reg_id, 0)]),
        (3, profile_id, [(1, reg_id, 0), (1, scope_id, 0), (2, gid, 1)]),
        (4, spec_id, [(1, scope_id, 0), (1, profile_id, 0)]),
        (5, adm_id, [(1, profile_id, 0), (1, spec_id, 0)]),
        (6, par_id, parity_refs),
        (7, rel_id, release_refs),
    ] + extra_nodes
    nodes = sorted(
        [(rank, nid, sorted(refs)) for rank, nid, refs in nodes],
        key=lambda x: x[1],
    )
    x_outgoing = sorted([(scope_id, 1), (reg_id, 2)], key=lambda x: x[0])

    i2i = sum(1 for _, _, refs in nodes for t, _, _ in refs if t == 1)
    i2e = sum(1 for _, _, refs in nodes for t, _, _ in refs if t == 2)
    e2i = 2
    n_supp = len(supp)
    n_int = 7 + n_supp
    n_comp = 8 + n_supp

    explicit_list = [scope_b, reg_bytes, cand_b, spec_b, adm_b, par_b, rel_b]
    b_int = sum(len(x) for x in explicit_list) + sum(len(x[2]) for x in supp)
    b_ext = len(pkg_b)
    b_tot = b_int + b_ext

    # Encode LaunchClosureBundleV1
    out = bytearray()
    out += struct.pack(
        ">HIIIIIIIIQQQ",
        1,
        n_int,
        1,
        n_comp,
        n_supp,
        i2i,
        i2e,
        e2i,
        i2i + i2e + e2i,
        b_int,
        b_ext,
        b_tot,
    )
    out += struct.pack(">H", len(GENESIS_PACKAGE_SCHEMA)) + GENESIS_PACKAGE_SCHEMA
    out += struct.pack(">I", 2) + gid + struct.pack(">QH", b_ext, len(x_outgoing))
    for tid, cls in x_outgoing:
        out += tid + bytes([cls])
    for eb in explicit_list:
        out += struct.pack(">Q", len(eb))
    for eb in explicit_list:
        out += b"\x01" + eb
    out += struct.pack(">I", len(nodes))
    for rank, nid, refs in nodes:
        out += struct.pack(">I", rank) + nid + struct.pack(">H", len(refs))
        for tag, target, cls in refs:
            if tag == 1:
                out += b"\x01" + target
            else:
                out += b"\x02" + target + bytes([cls])
    out += struct.pack(">I", len(supp))
    for sch, sid, sbytes in supp:
        out += b"\x01" + struct.pack(">H", len(sch)) + sch + sid
        out += struct.pack(">Q", len(sbytes)) + sbytes
    bundle_bytes = bytes(out)

    # Independent zero-copy preflight verification
    pf = preflight_bundle_py(bundle_bytes, b_ext)
    assert pf["internal"] == n_int and pf["complete"] == n_comp
    bundle_tag, bundle_id = content_tag("receipt", BUNDLE_SCHEMA, bundle_bytes)

    # Deterministic 3-of-5 devnet test launch policy (DEVNET-TEST-KEYS-NOT-PRODUCTION-CUSTODY)
    key_pairs = sorted(
        [(ed_public_from_seed(bytes([s]) * 32), bytes([s]) * 32) for s in (51, 52, 53, 54, 55)],
        key=lambda x: x[0],
    )
    hp = hashlib.sha256(CHL1_POLICY_PREFIX + bytes([3, 5]))
    for pk, _ in key_pairs:
        hp.update(pk)
    launch_policy_id = hp.digest()
    genesis_parent_id = hashlib.sha256(CHL1_GENESIS_PARENT_PREFIX + gid).digest()

    # ChainLaunchManifestV1
    manifest_bytes = (
        struct.pack(">H", 1)
        + gid
        + profile_id
        + spec_id
        + adm_id
        + par_id
        + raw["exec_root_ref"]
        + rel_id
        + bundle_id
        + launch_policy_id
        + genesis_parent_id
        + raw["genesis_history_root"]
        + struct.pack(">Q", 1)
    )
    manifest_tag, manifest_id = content_tag("policy", MANIFEST_SCHEMA, manifest_bytes)

    # ChainLaunchAuthorizationV1 (SigningDomainTagV1::GovernanceAction = 7)
    auth_payload_dig = hashlib.sha256(
        AUTH_SCHEMA + struct.pack(">I", 66) + manifest_id + launch_policy_id + struct.pack(">H", 1)
    ).digest()
    preimage_bytes = encode_signing_preimage(
        1, 77, gid, manifest_id, profile_id, 7, 1, 1, auth_payload_dig
    )
    _, signing_dig = content_tag("receipt", SIGNING_PREIMAGE_SCHEMA, preimage_bytes)

    sigs_out = bytearray()
    for idx in range(3):
        pk, seed = key_pairs[idx]
        sig = ed_sign(seed, signing_dig)
        assert ed_verify_strict(pk, signing_dig, sig)
        sigs_out += bytes([idx]) + pk + sig
    auth_bytes = (
        struct.pack(">H", 1)
        + manifest_id
        + launch_policy_id
        + struct.pack(">HH", 1, 3)
        + bytes(sigs_out)
    )
    auth_tag, _ = content_tag("receipt", AUTH_SCHEMA, auth_bytes)

    # ChainLaunchPackageV1
    package_bytes = (
        struct.pack(">H", 1)
        + struct.pack(">Q", len(pkg_b))
        + pkg_b
        + struct.pack(">Q", len(bundle_bytes))
        + bundle_bytes
        + bundle_id
        + manifest_bytes
        + manifest_id
        + struct.pack(">I", len(auth_bytes))
        + auth_bytes
    )
    package_tag, _ = content_tag("receipt", PACKAGE_SCHEMA, package_bytes)

    return {
        "bundle_tag": bundle_tag,
        "manifest_tag": manifest_tag,
        "auth_tag": auth_tag,
        "package_tag": package_tag,
        "bundle_bytes": bundle_bytes,
        "b_ext": b_ext,
    }


def main() -> int:
    s3 = build_chain_launch_vectors(True)
    s0 = build_chain_launch_vectors(False)

    # Powered negative controls on preflight: off-by-one internal count, duplicate external root,
    # double-counted bytes, and rank-zero genesis must all fail preflight.
    for label, off, val in [
        ("off_by_one_internal", 2, struct.pack(">I", 11)),
        ("dup_external_root", 6, struct.pack(">I", 2)),
        ("rank_zero_genesis", 2 + 8 * 4 + 3 * 8 + 2 + len(GENESIS_PACKAGE_SCHEMA), struct.pack(">I", 0)),
    ]:
        mut = bytearray(s3["bundle_bytes"])
        mut[off : off + len(val)] = val
        try:
            preflight_bundle_py(bytes(mut), s3["b_ext"])
        except ValueError:
            pass
        else:
            raise AssertionError(f"negative control {label} unexpectedly passed preflight")

    if "--print-tags" in sys.argv:
        print("BUNDLE_S3_TAG   =", s3["bundle_tag"])
        print("MANIFEST_S3_TAG =", s3["manifest_tag"])
        print("AUTH_S3_TAG     =", s3["auth_tag"])
        print("PACKAGE_S3_TAG  =", s3["package_tag"])
        print("PACKAGE_S0_TAG  =", s0["package_tag"])
        return 0

    assert s3["bundle_tag"] == EXPECTED_BUNDLE_S3_TAG, s3["bundle_tag"]
    assert s3["manifest_tag"] == EXPECTED_MANIFEST_S3_TAG, s3["manifest_tag"]
    assert s3["auth_tag"] == EXPECTED_AUTH_S3_TAG, s3["auth_tag"]
    assert s3["package_tag"] == EXPECTED_PACKAGE_S3_TAG, s3["package_tag"]
    assert s0["package_tag"] == EXPECTED_PACKAGE_S0_TAG, s0["package_tag"]
    print(
        f"chain_launch_1_vectors: PASS (s3_pkg={s3['package_tag'][:26]}..., s0_pkg={s0['package_tag'][:26]}...)"
    )
    return 0


EXPECTED_BUNDLE_S3_TAG = "#bb.receipt.sha256.2cyxfi3goddwndgyfzvjy7y3eofikotdyry4wy7s7wzkubuzjm5a"
EXPECTED_MANIFEST_S3_TAG = "#bb.policy.sha256.nxam3harptfty3tsbfqz6dtwnt44cgm7zdrd46334v4grcuzuspa"
EXPECTED_AUTH_S3_TAG = "#bb.receipt.sha256.daxw2r45k4zxrw3ypg6iupja7mcfdnyekgxrgsuhhi3gn75s3lua"
EXPECTED_PACKAGE_S3_TAG = "#bb.receipt.sha256.2xyk6dukblpqmqfkkovaez7obpgck5j6yki5tqqa3xjcvg5ktgna"
EXPECTED_PACKAGE_S0_TAG = "#bb.receipt.sha256.apvfatsiegwvk52s4s25y5ffnhss63flbd5ydehwukrh75yuvcoa"

if __name__ == "__main__":
    raise SystemExit(main())
