#!/usr/bin/env python3
"""Independent stdlib vectors for COMMIT-1 authenticated local state.

This validator imports no Bitblock or Rust code. It reconstructs all nine
COMMIT-1 schemas directly from docs/AUTHENTICATED_STATE_PLAN.md and
crates/research/src/commit_state.rs, verifying deterministic descriptors,
registries, 192-bit and 256-bit binary Merkle Patricia Tries, context-bound
point proofs, CommitmentSetV1 manifests, and powered negative controls.

Schemas vectored here:
  * bitblock/commit-1/proof-policy/v1
  * bitblock/commit-1/operational-limits/v1
  * bitblock/commit-1/commitment-profile/v1
  * bitblock/commit-1/domain-registry/v1
  * bitblock/commit-1/world-root/v1
  * bitblock/commit-1/active-token-root/v1
  * bitblock/commit-1/episode-root/v1
  * bitblock/commit-1/patricia-proof/v1
  * bitblock/commit-1/commitment-set/v1
"""

from __future__ import annotations

import base64
import hashlib
import struct
import sys

TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"

COMMIT_1_PROOF_POLICY_SCHEMA = b"bitblock/commit-1/proof-policy/v1"
COMMIT_1_OP_LIMITS_SCHEMA = b"bitblock/commit-1/operational-limits/v1"
COMMIT_1_PROFILE_SCHEMA = b"bitblock/commit-1/commitment-profile/v1"
COMMIT_1_DOMAIN_REGISTRY_SCHEMA = b"bitblock/commit-1/domain-registry/v1"
COMMIT_1_WORLD_ROOT_SCHEMA = b"bitblock/commit-1/world-root/v1"
COMMIT_1_ACTIVE_TOKEN_ROOT_SCHEMA = b"bitblock/commit-1/active-token-root/v1"
COMMIT_1_EPISODE_ROOT_SCHEMA = b"bitblock/commit-1/episode-root/v1"
COMMIT_1_POINT_PROOF_SCHEMA = b"bitblock/commit-1/patricia-proof/v1"
COMMIT_1_COMMITMENT_SET_SCHEMA = b"bitblock/commit-1/commitment-set/v1"

SCHEMA_VERSION = 1
NODE_CODEC_VERSION = 1
ROOT_CODEC_VERSION = 1

DOMAIN_WORLD = 1
DOMAIN_ACTIVE_TOKEN = 2
DOMAIN_EPISODE = 3
DOMAIN_AUTHORIZATION = 4
DOMAIN_NONCE = 5
DOMAIN_ASSET_STATE = 6

LAYER_LOCAL = 1
LAYER_NETWORK = 2
LAYER_EXTENSION = 3

ROOT_WORLD = 1
ROOT_ACTIVE_TOKEN = 2
ROOT_EPISODE = 3

KEY_COORD_V1 = 1
KEY_EVENT_V1 = 2

VAL_ON_V1 = 1
VAL_MINT_V1 = 2
VAL_EPISODE_V1 = 3

ALGO_MPT_V1 = 1
SUITE_SHA256_V1 = 1
EMPTY_RULE_V1 = 1
INACTIVE_SENTINEL_V1 = 1

NODE_EMPTY = 0
NODE_LEAF = 1
NODE_EXTENSION = 2
NODE_BRANCH = 3

STEP_BRANCH = 1
STEP_EXTENSION = 2
TERM_EMPTY = 0
TERM_LEAF = 1
TERM_DIVERGENT_EXT = 2

ON_V1 = b"\x01"
SIGN_BIAS = 1 << 63


def field(val: bytes) -> bytes:
    return struct.pack(">H", len(val)) + val


def content_tag(kind: str, schema: bytes, payload: bytes) -> tuple[str, bytes]:
    preimage = (
        field(TAG_DOMAIN)
        + field(kind.encode("ascii"))
        + field(HASH_SUITE)
        + field(schema)
        + struct.pack(">Q", len(payload))
        + payload
    )
    digest = hashlib.sha256(preimage).digest()
    encoded = base64.b32encode(digest).decode("ascii").lower().rstrip("=")
    return f"#bb.{kind}.sha256.{encoded}", digest


def encode_proof_policy(
    max_proof_bytes: int,
    max_proof_records: int,
    max_reconstructed_depth: int,
    max_hash_invocations: int,
    max_hashed_preimage_bytes: int,
    max_single_encoded_node_bytes: int,
    max_total_encoded_node_bytes: int,
) -> bytes:
    return struct.pack(
        ">HIHHHIII",
        SCHEMA_VERSION,
        max_proof_bytes,
        max_proof_records,
        max_reconstructed_depth,
        max_hash_invocations,
        max_hashed_preimage_bytes,
        max_single_encoded_node_bytes,
        max_total_encoded_node_bytes,
    )


def encode_op_limits() -> bytes:
    return struct.pack(
        ">HQIIQQIIQQQQ",
        SCHEMA_VERSION,
        262_144,
        65_536,
        512,
        524_288,
        64 * 1024 * 1024,
        4_096,
        64,
        524_288,
        524_288,
        65_536,
        1_048_576,
    )


def encode_profile(
    root_type: int,
    domain_tag: int,
    tree_domain: bytes,
    key_codec: int,
    key_bit_width: int,
    value_codec: int,
    proof_policy_id: bytes,
    op_limits_id: bytes,
) -> bytes:
    return (
        struct.pack(">HBBB", SCHEMA_VERSION, ALGO_MPT_V1, root_type, domain_tag)
        + field(tree_domain)
        + struct.pack(
            ">BHBBBHHB",
            key_codec,
            key_bit_width,
            value_codec,
            SUITE_SHA256_V1,
            32,
            NODE_CODEC_VERSION,
            ROOT_CODEC_VERSION,
            EMPTY_RULE_V1,
        )
        + proof_policy_id
        + op_limits_id
    )


def encode_required_domain(
    domain_tag: int, layer: int, root_type: int, schema: bytes, profile_id: bytes
) -> bytes:
    return (
        struct.pack(">BBB", domain_tag, layer, root_type)
        + field(schema)
        + profile_id
    )


def encode_domain_registry(
    world_pid: bytes, active_pid: bytes, episode_pid: bytes
) -> tuple[bytes, list[bytes]]:
    req_w = encode_required_domain(
        DOMAIN_WORLD, LAYER_LOCAL, ROOT_WORLD, COMMIT_1_WORLD_ROOT_SCHEMA, world_pid
    )
    req_a = encode_required_domain(
        DOMAIN_ACTIVE_TOKEN,
        LAYER_LOCAL,
        ROOT_ACTIVE_TOKEN,
        COMMIT_1_ACTIVE_TOKEN_ROOT_SCHEMA,
        active_pid,
    )
    req_e = encode_required_domain(
        DOMAIN_EPISODE,
        LAYER_LOCAL,
        ROOT_EPISODE,
        COMMIT_1_EPISODE_ROOT_SCHEMA,
        episode_pid,
    )
    entries = [
        b"\x01" + req_w,
        b"\x01" + req_a,
        b"\x01" + req_e,
        struct.pack(">BBBB", 0, DOMAIN_AUTHORIZATION, LAYER_NETWORK, INACTIVE_SENTINEL_V1),
        struct.pack(">BBBB", 0, DOMAIN_NONCE, LAYER_NETWORK, INACTIVE_SENTINEL_V1),
        struct.pack(">BBBB", 0, DOMAIN_ASSET_STATE, LAYER_EXTENSION, INACTIVE_SENTINEL_V1),
    ]
    payload = struct.pack(">HH", SCHEMA_VERSION, len(entries)) + b"".join(entries)
    return payload, [req_w, req_a, req_e]


def bias64(v: int) -> int:
    return (v & 0xFFFFFFFFFFFFFFFF) ^ SIGN_BIAS


def coord_key(x: int, y: int, z: int) -> bytes:
    return struct.pack(">QQQ", bias64(x), bias64(y), bias64(z))


def episode_record(
    coord: tuple[int, int, int], mint: bytes, burn: bytes | None
) -> bytes:
    out = struct.pack(">qqq", coord[0], coord[1], coord[2]) + mint
    if burn is None:
        return out + b"\x00"
    return out + b"\x01" + burn


def key_bit(key: bytes, depth: int) -> int:
    return (key[depth // 8] >> (7 - (depth % 8))) & 1


def pack_bits(bits: list[int]) -> bytes:
    out = bytearray((len(bits) + 7) // 8)
    for i, b in enumerate(bits):
        out[i // 8] |= (b & 1) << (7 - (i % 8))
    return bytes(out)


def pack_key_suffix(key: bytes, depth: int, key_bits: int) -> bytes:
    return pack_bits([key_bit(key, d) for d in range(depth, key_bits)])


def node_hash(
    profile_id: bytes, tree_domain: bytes, node_type: int, depth: int, payload: bytes
) -> bytes:
    h = hashlib.sha256()
    h.update(profile_id)
    h.update(field(tree_domain))
    h.update(field(HASH_SUITE))
    h.update(struct.pack(">HBHI", NODE_CODEC_VERSION, node_type, depth, len(payload)))
    h.update(payload)
    return h.digest()


def common_prefix(a: bytes, b: bytes, start: int, key_bits: int) -> int:
    n = 0
    while start + n < key_bits and key_bit(a, start + n) == key_bit(b, start + n):
        n += 1
    return n


class PyTrieNode:
    pass


class PyEmpty(PyTrieNode):
    def __init__(self, h: bytes):
        self.hash = h


class PyLeaf(PyTrieNode):
    def __init__(self, depth: int, key: bytes, val: bytes, h: bytes):
        self.depth = depth
        self.key = key
        self.val = val
        self.hash = h


class PyExt(PyTrieNode):
    def __init__(self, depth: int, bits: list[int], child: PyTrieNode, h: bytes):
        self.depth = depth
        self.bits = bits
        self.child = child
        self.hash = h


class PyBranch(PyTrieNode):
    def __init__(self, depth: int, left: PyTrieNode, right: PyTrieNode, h: bytes):
        self.depth = depth
        self.left = left
        self.right = right
        self.hash = h


def build_patricia(
    profile_id: bytes,
    tree_domain: bytes,
    key_bits: int,
    entries: list[tuple[bytes, bytes]],
) -> PyTrieNode:
    ordered = sorted(entries, key=lambda kv: kv[0])
    for i in range(1, len(ordered)):
        assert ordered[i - 1][0] < ordered[i][0], "duplicate key"
    empty_h = node_hash(profile_id, tree_domain, NODE_EMPTY, 0, b"")

    def rec(sub: list[tuple[bytes, bytes]], depth: int) -> PyTrieNode:
        if not sub:
            return PyEmpty(empty_h)
        if len(sub) == 1:
            k, v = sub[0]
            payload = (
                struct.pack(">H", key_bits - depth)
                + pack_key_suffix(k, depth, key_bits)
                + struct.pack(">I", len(v))
                + v
            )
            return PyLeaf(
                depth, k, v, node_hash(profile_id, tree_domain, NODE_LEAF, depth, payload)
            )
        shared = common_prefix(sub[0][0], sub[-1][0], depth, key_bits)
        bd = depth + shared
        left_sub = [kv for kv in sub if key_bit(kv[0], bd) == 0]
        right_sub = [kv for kv in sub if key_bit(kv[0], bd) == 1]
        left = rec(left_sub, bd + 1)
        right = rec(right_sub, bd + 1)
        branch = PyBranch(
            bd,
            left,
            right,
            node_hash(profile_id, tree_domain, NODE_BRANCH, bd, left.hash + right.hash),
        )
        if shared == 0:
            return branch
        bits = [key_bit(sub[0][0], d) for d in range(depth, bd)]
        ext_payload = struct.pack(">H", len(bits)) + pack_bits(bits) + branch.hash
        return PyExt(
            depth,
            bits,
            branch,
            node_hash(profile_id, tree_domain, NODE_EXTENSION, depth, ext_payload),
        )

    return rec(ordered, 0)


def prove_patricia(root: PyTrieNode, query_key: bytes) -> bytes:
    steps: list[bytes] = []
    cur = root
    while True:
        if isinstance(cur, PyEmpty):
            term = bytes([TERM_EMPTY])
            break
        if isinstance(cur, PyLeaf):
            term = (
                struct.pack(">BHH", TERM_LEAF, cur.depth, len(cur.key))
                + cur.key
                + struct.pack(">I", len(cur.val))
                + cur.val
            )
            break
        if isinstance(cur, PyExt):
            if any(key_bit(query_key, cur.depth + i) != b for i, b in enumerate(cur.bits)):
                term = (
                    struct.pack(">BHH", TERM_DIVERGENT_EXT, cur.depth, len(cur.bits))
                    + pack_bits(cur.bits)
                    + cur.child.hash
                )
                break
            steps.append(
                struct.pack(">BHH", STEP_EXTENSION, cur.depth, len(cur.bits))
                + pack_bits(cur.bits)
            )
            cur = cur.child
        elif isinstance(cur, PyBranch):
            d = key_bit(query_key, cur.depth)
            sib = cur.right.hash if d == 0 else cur.left.hash
            steps.append(struct.pack(">BHB", STEP_BRANCH, cur.depth, d) + sib)
            cur = cur.left if d == 0 else cur.right
        else:
            raise AssertionError("unreachable node")
    return struct.pack(">HH", SCHEMA_VERSION, len(steps)) + b"".join(steps) + term


def verify_patricia_proof(
    profile_id: bytes,
    tree_domain: bytes,
    key_bits: int,
    expected_root: bytes,
    query_key: bytes,
    proof_bytes: bytes,
) -> tuple[bool, bytes | None]:
    """Returns (present, value_bytes) or raises ValueError on any canonical violation."""
    empty_h = node_hash(profile_id, tree_domain, NODE_EMPTY, 0, b"")
    key_bytes = key_bits // 8
    pos = 0

    def take(n: int) -> bytes:
        nonlocal pos
        if pos + n > len(proof_bytes):
            raise ValueError("truncated proof")
        s = proof_bytes[pos : pos + n]
        pos += n
        return s

    ver, step_count = struct.unpack(">HH", take(4))
    if ver != SCHEMA_VERSION:
        raise ValueError("wrong proof version")
    steps = []
    for _ in range(step_count):
        tag = take(1)[0]
        if tag == STEP_BRANCH:
            depth, direction = struct.unpack(">HB", take(3))
            sib = take(32)
            if depth >= key_bits or direction > 1:
                raise ValueError("invalid branch step")
            steps.append((STEP_BRANCH, depth, direction, sib))
        elif tag == STEP_EXTENSION:
            depth, blen = struct.unpack(">HH", take(4))
            if blen == 0 or depth + blen >= key_bits:
                raise ValueError("invalid extension step")
            packed = take((blen + 7) // 8)
            rem = blen % 8
            if rem != 0 and (packed[-1] & ((1 << (8 - rem)) - 1)) != 0:
                raise ValueError("nonzero padding bits")
            bits = [(packed[i // 8] >> (7 - (i % 8))) & 1 for i in range(blen)]
            steps.append((STEP_EXTENSION, depth, bits))
        else:
            raise ValueError("unknown step tag")

    term_tag = take(1)[0]
    if term_tag == TERM_EMPTY:
        if steps:
            raise ValueError("nonempty steps on EmptyRoot")
        cur_h, present, val, cur_d, prev_ext = empty_h, False, None, 0, False
    elif term_tag == TERM_LEAF:
        depth, klen = struct.unpack(">HH", take(4))
        if depth > key_bits or klen != key_bytes:
            raise ValueError("invalid leaf header")
        if depth == 0 and steps:
            raise ValueError("depth-0 leaf with steps")
        if depth > 0 and (not steps or steps[-1][0] != STEP_BRANCH):
            raise ValueError("non-root leaf not under branch")
        lkey = take(klen)
        (vlen,) = struct.unpack(">I", take(4))
        lval = take(vlen)
        if common_prefix(lkey, query_key, 0, key_bits) < depth:
            raise ValueError("leaf prefix mismatch")
        payload = (
            struct.pack(">H", key_bits - depth)
            + pack_key_suffix(lkey, depth, key_bits)
            + struct.pack(">I", len(lval))
            + lval
        )
        cur_h = node_hash(profile_id, tree_domain, NODE_LEAF, depth, payload)
        present = lkey == query_key
        val = lval if present else None
        cur_d, prev_ext = depth, False
    elif term_tag == TERM_DIVERGENT_EXT:
        depth, blen = struct.unpack(">HH", take(4))
        if blen == 0 or depth + blen >= key_bits:
            raise ValueError("invalid divergent ext")
        packed = take((blen + 7) // 8)
        rem = blen % 8
        if rem != 0 and (packed[-1] & ((1 << (8 - rem)) - 1)) != 0:
            raise ValueError("nonzero padding bits")
        bits = [(packed[i // 8] >> (7 - (i % 8))) & 1 for i in range(blen)]
        ch = take(32)
        if ch == empty_h:
            raise ValueError("empty child under divergent ext")
        if not any(key_bit(query_key, depth + i) != b for i, b in enumerate(bits)):
            raise ValueError("divergent ext does not diverge")
        payload = struct.pack(">H", len(bits)) + packed + ch
        cur_h = node_hash(profile_id, tree_domain, NODE_EXTENSION, depth, payload)
        present, val, cur_d, prev_ext = False, None, depth, True
    else:
        raise ValueError("unknown terminal tag")

    if pos != len(proof_bytes):
        raise ValueError("trailing bytes")

    for step in reversed(steps):
        if step[0] == STEP_EXTENSION:
            _, depth, bits = step
            if prev_ext or depth + len(bits) != cur_d:
                raise ValueError("adjacent ext or depth mismatch")
            if any(key_bit(query_key, depth + i) != b for i, b in enumerate(bits)):
                raise ValueError("ext bit mismatch")
            payload = struct.pack(">H", len(bits)) + pack_bits(bits) + cur_h
            cur_h = node_hash(profile_id, tree_domain, NODE_EXTENSION, depth, payload)
            cur_d, prev_ext = depth, True
        else:
            _, depth, direction, sib = step
            if depth + 1 != cur_d or sib == empty_h or key_bit(query_key, depth) != direction:
                raise ValueError("branch mismatch")
            payload = (cur_h + sib) if direction == 0 else (sib + cur_h)
            cur_h = node_hash(profile_id, tree_domain, NODE_BRANCH, depth, payload)
            cur_d, prev_ext = depth, False

    if cur_d != 0 or cur_h != expected_root:
        raise ValueError("root mismatch")
    return present, val


def encode_commitment_set(
    local_storage_profile_id: bytes,
    scope_digest: bytes,
    domain_registry_digest: bytes,
    ledger_sequence: int,
    ledger_head_id: bytes,
    req_domains: list[bytes],
    roots: list[bytes],
    prior_set_id: bytes | None,
) -> bytes:
    assert len(req_domains) == len(roots)
    out = bytearray()
    out.extend(struct.pack(">HB", SCHEMA_VERSION, 1))
    out.extend(local_storage_profile_id)
    out.extend(scope_digest)
    out.extend(domain_registry_digest)
    out.extend(struct.pack(">Q", ledger_sequence))
    out.extend(ledger_head_id)
    out.extend(struct.pack(">H", len(req_domains)))
    for rd in req_domains:
        out.extend(rd)
    out.extend(struct.pack(">H", len(roots)))
    for rd, rt in zip(req_domains, roots):
        out.extend(rd)
        out.extend(rt)
    if prior_set_id is None:
        out.append(0)
    else:
        out.append(1)
        out.extend(prior_set_id)
    return bytes(out)


def main() -> int:
    pp_world_bytes = encode_proof_policy(8192, 193, 192, 193, 32768, 128, 16384)
    pp_active_bytes = encode_proof_policy(8192, 193, 192, 193, 32768, 160, 16384)
    pp_episode_bytes = encode_proof_policy(12288, 257, 256, 257, 49152, 256, 24576)

    pp_world_tag, pp_world_id = content_tag("policy", COMMIT_1_PROOF_POLICY_SCHEMA, pp_world_bytes)
    pp_active_tag, pp_active_id = content_tag("policy", COMMIT_1_PROOF_POLICY_SCHEMA, pp_active_bytes)
    pp_episode_tag, pp_episode_id = content_tag("policy", COMMIT_1_PROOF_POLICY_SCHEMA, pp_episode_bytes)

    op_limits_bytes = encode_op_limits()
    op_limits_tag, op_limits_id = content_tag("policy", COMMIT_1_OP_LIMITS_SCHEMA, op_limits_bytes)

    prof_w_bytes = encode_profile(
        ROOT_WORLD, DOMAIN_WORLD, COMMIT_1_WORLD_ROOT_SCHEMA, KEY_COORD_V1, 192, VAL_ON_V1, pp_world_id, op_limits_id
    )
    prof_a_bytes = encode_profile(
        ROOT_ACTIVE_TOKEN, DOMAIN_ACTIVE_TOKEN, COMMIT_1_ACTIVE_TOKEN_ROOT_SCHEMA, KEY_COORD_V1, 192, VAL_MINT_V1, pp_active_id, op_limits_id
    )
    prof_e_bytes = encode_profile(
        ROOT_EPISODE, DOMAIN_EPISODE, COMMIT_1_EPISODE_ROOT_SCHEMA, KEY_EVENT_V1, 256, VAL_EPISODE_V1, pp_episode_id, op_limits_id
    )

    prof_w_tag, prof_w_id = content_tag("policy", COMMIT_1_PROFILE_SCHEMA, prof_w_bytes)
    prof_a_tag, prof_a_id = content_tag("policy", COMMIT_1_PROFILE_SCHEMA, prof_a_bytes)
    prof_e_tag, prof_e_id = content_tag("policy", COMMIT_1_PROFILE_SCHEMA, prof_e_bytes)

    reg_bytes, req_domains = encode_domain_registry(prof_w_id, prof_a_id, prof_e_id)
    reg_tag, reg_digest = content_tag("policy", COMMIT_1_DOMAIN_REGISTRY_SCHEMA, reg_bytes)

    # WorldRootV1: empty, bedrock-only, and 3-coord world
    w_empty = build_patricia(prof_w_id, COMMIT_1_WORLD_ROOT_SCHEMA, 192, [])
    w_bedrock = build_patricia(
        prof_w_id, COMMIT_1_WORLD_ROOT_SCHEMA, 192, [(coord_key(0, 0, 0), ON_V1)]
    )
    w_three = build_patricia(
        prof_w_id,
        COMMIT_1_WORLD_ROOT_SCHEMA,
        192,
        [
            (coord_key(0, 0, 0), ON_V1),
            (coord_key(7, -3, 19), ON_V1),
            (coord_key(-(1 << 63), 0, (1 << 63) - 1), ON_V1),
        ],
    )
    w_bedrock_tag, _ = content_tag("world", COMMIT_1_WORLD_ROOT_SCHEMA, prof_w_id + w_bedrock.hash)
    w_three_tag, _ = content_tag("world", COMMIT_1_WORLD_ROOT_SCHEMA, prof_w_id + w_three.hash)

    # ActiveTokenRootV1: synthetic deterministic mint IDs at two mutable coords
    mint1 = hashlib.sha256(b"commit-1-vector-mint-1").digest()
    mint2 = hashlib.sha256(b"commit-1-vector-mint-2").digest()
    burn1 = hashlib.sha256(b"commit-1-vector-burn-1").digest()
    a_empty = build_patricia(prof_a_id, COMMIT_1_ACTIVE_TOKEN_ROOT_SCHEMA, 192, [])
    a_two = build_patricia(
        prof_a_id,
        COMMIT_1_ACTIVE_TOKEN_ROOT_SCHEMA,
        192,
        [
            (coord_key(7, -3, 19), mint1),
            (coord_key(-16, 15, 0), mint2),
        ],
    )
    a_two_tag, _ = content_tag("receipt", COMMIT_1_ACTIVE_TOKEN_ROOT_SCHEMA, prof_a_id + a_two.hash)

    # EpisodeRootV1: 256-bit keys including a depth-256 twin pair differing only in bit 255!
    twin_a = bytes([0xAA] * 31 + [0x00])
    twin_b = bytes([0xAA] * 31 + [0x01])
    e_empty = build_patricia(prof_e_id, COMMIT_1_EPISODE_ROOT_SCHEMA, 256, [])
    e_three = build_patricia(
        prof_e_id,
        COMMIT_1_EPISODE_ROOT_SCHEMA,
        256,
        [
            (mint1, episode_record((7, -3, 19), mint1, burn1)),
            (twin_a, episode_record((1, 2, 3), twin_a, None)),
            (twin_b, episode_record((4, 5, 6), twin_b, None)),
        ],
    )
    e_three_tag, _ = content_tag("receipt", COMMIT_1_EPISODE_ROOT_SCHEMA, prof_e_id + e_three.hash)

    # Proofs across empty, presence, leaf-absence, divergent-extension-absence, and depth-256 leaf!
    p_empty = prove_patricia(w_empty, coord_key(0, 0, 0))
    assert verify_patricia_proof(
        prof_w_id, COMMIT_1_WORLD_ROOT_SCHEMA, 192, w_empty.hash, coord_key(0, 0, 0), p_empty
    ) == (False, None)
    p_world_hit = prove_patricia(w_three, coord_key(7, -3, 19))
    assert verify_patricia_proof(
        prof_w_id, COMMIT_1_WORLD_ROOT_SCHEMA, 192, w_three.hash, coord_key(7, -3, 19), p_world_hit
    ) == (True, ON_V1)
    p_world_div = prove_patricia(w_three, coord_key(1, 0, 0))
    assert verify_patricia_proof(
        prof_w_id, COMMIT_1_WORLD_ROOT_SCHEMA, 192, w_three.hash, coord_key(1, 0, 0), p_world_div
    ) == (False, None)
    p_ep_d256 = prove_patricia(e_three, twin_b)
    assert verify_patricia_proof(
        prof_e_id, COMMIT_1_EPISODE_ROOT_SCHEMA, 256, e_three.hash, twin_b, p_ep_d256
    ) == (True, episode_record((4, 5, 6), twin_b, None))

    p_world_hit_tag, _ = content_tag("receipt", COMMIT_1_POINT_PROOF_SCHEMA, p_world_hit)
    p_ep_d256_tag, _ = content_tag("receipt", COMMIT_1_POINT_PROOF_SCHEMA, p_ep_d256)

    # Powered negative controls:
    # 1. Cross-domain proof replay (world proof presented against active-token profile) must fail.
    for bad_case, fn in [
        (
            "cross_domain",
            lambda: verify_patricia_proof(
                prof_a_id,
                COMMIT_1_ACTIVE_TOKEN_ROOT_SCHEMA,
                192,
                w_three.hash,
                coord_key(7, -3, 19),
                p_world_hit,
            ),
        ),
        (
            "trailing_byte",
            lambda: verify_patricia_proof(
                prof_w_id,
                COMMIT_1_WORLD_ROOT_SCHEMA,
                192,
                w_three.hash,
                coord_key(7, -3, 19),
                p_world_hit + b"\x00",
            ),
        ),
        (
            "wrong_version",
            lambda: verify_patricia_proof(
                prof_w_id,
                COMMIT_1_WORLD_ROOT_SCHEMA,
                192,
                w_three.hash,
                coord_key(7, -3, 19),
                b"\x00\x02" + p_world_hit[2:],
            ),
        ),
        (
            "wrong_query_key",
            lambda: verify_patricia_proof(
                prof_w_id,
                COMMIT_1_WORLD_ROOT_SCHEMA,
                192,
                w_three.hash,
                coord_key(8, -3, 19),
                p_world_hit,
            ),
        ),
    ]:
        try:
            fn()
        except ValueError:
            pass
        else:
            raise AssertionError(f"negative control {bad_case} unexpectedly verified")

    # CommitmentSetV1 genesis + successor vectors
    syn_lsp = hashlib.sha256(b"commit-1-vector-lsp").digest()
    syn_scope = hashlib.sha256(b"commit-1-vector-scope").digest()
    syn_head0 = hashlib.sha256(b"commit-1-vector-head-0").digest()
    syn_head1 = hashlib.sha256(b"commit-1-vector-head-1").digest()

    cset0_bytes = encode_commitment_set(
        syn_lsp,
        syn_scope,
        reg_digest,
        0,
        syn_head0,
        req_domains,
        [w_bedrock.hash, a_empty.hash, e_empty.hash],
        None,
    )
    cset0_tag, cset0_id = content_tag("receipt", COMMIT_1_COMMITMENT_SET_SCHEMA, cset0_bytes)
    cset1_bytes = encode_commitment_set(
        syn_lsp,
        syn_scope,
        reg_digest,
        1,
        syn_head1,
        req_domains,
        [w_three.hash, a_two.hash, e_three.hash],
        cset0_id,
    )
    cset1_tag, _ = content_tag("receipt", COMMIT_1_COMMITMENT_SET_SCHEMA, cset1_bytes)

    if "--print-golden" in sys.argv:
        for name, val in [
            ("PP_WORLD_TAG", pp_world_tag),
            ("PP_ACTIVE_TAG", pp_active_tag),
            ("PP_EPISODE_TAG", pp_episode_tag),
            ("OP_LIMITS_TAG", op_limits_tag),
            ("PROF_WORLD_TAG", prof_w_tag),
            ("PROF_ACTIVE_TAG", prof_a_tag),
            ("PROF_EPISODE_TAG", prof_e_tag),
            ("DOMAIN_REG_TAG", reg_tag),
            ("WORLD_EMPTY_HEX", w_empty.hash.hex()),
            ("WORLD_BEDROCK_TAG", w_bedrock_tag),
            ("WORLD_THREE_TAG", w_three_tag),
            ("ACTIVE_EMPTY_HEX", a_empty.hash.hex()),
            ("ACTIVE_TWO_TAG", a_two_tag),
            ("EPISODE_EMPTY_HEX", e_empty.hash.hex()),
            ("EPISODE_THREE_TAG", e_three_tag),
            ("PROOF_WORLD_HIT_TAG", p_world_hit_tag),
            ("PROOF_EPISODE_D256_TAG", p_ep_d256_tag),
            ("CSET0_TAG", cset0_tag),
            ("CSET1_TAG", cset1_tag),
        ]:
            print(f"{name} = {val}")
        return 0

    assert pp_world_tag == EXPECTED_PP_WORLD_TAG, pp_world_tag
    assert pp_active_tag == EXPECTED_PP_ACTIVE_TAG, pp_active_tag
    assert pp_episode_tag == EXPECTED_PP_EPISODE_TAG, pp_episode_tag
    assert op_limits_tag == EXPECTED_OP_LIMITS_TAG, op_limits_tag
    assert prof_w_tag == EXPECTED_PROF_WORLD_TAG, prof_w_tag
    assert prof_a_tag == EXPECTED_PROF_ACTIVE_TAG, prof_a_tag
    assert prof_e_tag == EXPECTED_PROF_EPISODE_TAG, prof_e_tag
    assert reg_tag == EXPECTED_DOMAIN_REG_TAG, reg_tag
    assert w_empty.hash.hex() == EXPECTED_WORLD_EMPTY_HEX, w_empty.hash.hex()
    assert w_bedrock_tag == EXPECTED_WORLD_BEDROCK_TAG, w_bedrock_tag
    assert w_three_tag == EXPECTED_WORLD_THREE_TAG, w_three_tag
    assert a_empty.hash.hex() == EXPECTED_ACTIVE_EMPTY_HEX, a_empty.hash.hex()
    assert a_two_tag == EXPECTED_ACTIVE_TWO_TAG, a_two_tag
    assert e_empty.hash.hex() == EXPECTED_EPISODE_EMPTY_HEX, e_empty.hash.hex()
    assert e_three_tag == EXPECTED_EPISODE_THREE_TAG, e_three_tag
    assert p_world_hit_tag == EXPECTED_PROOF_WORLD_HIT_TAG, p_world_hit_tag
    assert p_ep_d256_tag == EXPECTED_PROOF_EPISODE_D256_TAG, p_ep_d256_tag
    assert cset0_tag == EXPECTED_CSET0_TAG, cset0_tag
    assert cset1_tag == EXPECTED_CSET1_TAG, cset1_tag
    print("commit_state_vectors: 9 schemas, 19 golden assertions, 4 negative controls OK")
    return 0


EXPECTED_PP_WORLD_TAG = (
    "#bb.policy.sha256.vkvqipvpzty76ylzflzkwdyuwq3u2ikjwumbtu52e45u5a6jbo3q"
)
EXPECTED_PP_ACTIVE_TAG = (
    "#bb.policy.sha256.3rof4k6xgp5dfl5mc3fj63cuetrjetcxrucd6qhhvry3xvertw5q"
)
EXPECTED_PP_EPISODE_TAG = (
    "#bb.policy.sha256.qdewuvwl266u2dkfpowshnv3n3ihd52i2j366rivbph5ef4zh5ea"
)
EXPECTED_OP_LIMITS_TAG = (
    "#bb.policy.sha256.u72lxb77ydfzak3eamjl4v53km4yb7nfkobb5dtmmzvoqicxsaua"
)
EXPECTED_PROF_WORLD_TAG = (
    "#bb.policy.sha256.pta7j7uqpw2rtx2wmez3zobxicou5updtavrhc6kxvewbaexta6a"
)
EXPECTED_PROF_ACTIVE_TAG = (
    "#bb.policy.sha256.m42tb7fel3fz6ktrir6huv6xq3uy67egs74xd22n2vlfsim4dbka"
)
EXPECTED_PROF_EPISODE_TAG = (
    "#bb.policy.sha256.3qk2vnttiwowmjhklrckm2p2dqwv6eiqozwex2wqnvk4k2uh7x2q"
)
EXPECTED_DOMAIN_REG_TAG = (
    "#bb.policy.sha256.g7mygkuhz6daeuzqztg5bvftvhl7kutqmrvx7hsj53s22fzhpzlq"
)
EXPECTED_WORLD_EMPTY_HEX = (
    "04a2eb261391b21b279448163dcac2a74245d7bde867a782c4a16cb7da8e60e6"
)
EXPECTED_WORLD_BEDROCK_TAG = (
    "#bb.world.sha256.tlnd3xyl3cp3aa6wsli3jd2uwx35srbomu4uxoarmlpgpffmxdyq"
)
EXPECTED_WORLD_THREE_TAG = (
    "#bb.world.sha256.3zkllbtffqgaeh7reojch6q7awq3juce4sdzsbrzoyxc65auihza"
)
EXPECTED_ACTIVE_EMPTY_HEX = (
    "09c7fc26a85988495c3999385dadda83b7de9f61551287b8cf299f71ff7459b2"
)
EXPECTED_ACTIVE_TWO_TAG = (
    "#bb.receipt.sha256.6uqfwbjqfpfllv3wsoxlgy5adzqhc2ltwlfkbnb347p5wygr56eq"
)
EXPECTED_EPISODE_EMPTY_HEX = (
    "55de32c3131e8013f2008a3d247bad53d867cfa006bb6eaa27ee9fdbe569afd7"
)
EXPECTED_EPISODE_THREE_TAG = (
    "#bb.receipt.sha256.cmyepgr23n5evmmnakckviwf7j6xvqcgquv4lh7zmo6fnzbwq4da"
)
EXPECTED_PROOF_WORLD_HIT_TAG = (
    "#bb.receipt.sha256.tkvgvorsld35rwag7apuxabdai2siirjuajm7ms4yo5kjfhqsfka"
)
EXPECTED_PROOF_EPISODE_D256_TAG = (
    "#bb.receipt.sha256.f4uts2kyf6oart7zfdzqme4x3blrmwgf2x6bvrtoscoqgoenet7a"
)
EXPECTED_CSET0_TAG = (
    "#bb.receipt.sha256.mrlbnlwnwjezmgrj5nqpngbcvng2oge4tvs2kvuzfi4beumci2cq"
)
EXPECTED_CSET1_TAG = (
    "#bb.receipt.sha256.ss22ue2amqyrmaytjmxgvzhzou35fysojcbqlgys22b25wuhar7q"
)


if __name__ == "__main__":
    raise SystemExit(main())
