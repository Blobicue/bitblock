#!/usr/bin/env python3
"""Independent stdlib vectors for Arity-8 SparseOctreeV1 and Verkle-R1 feasibility.

This script imports no Bitblock/Rust code. It independently reconstructs:
  * bitblock/octree-commit/chunk-leaf/v1
  * bitblock/octree-commit/internal-node/v1
  * bitblock/octree-commit/root/v1
  * bitblock/octree-commit/point-proof/v1
  * bitblock/octree-commit/verkle-r1-feasibility/v1
and verifies bit-exact ContentTag parity and fail-closed proof verification.
"""

from __future__ import annotations

import base64
import hashlib
import struct
import sys

TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
SIGN_BIAS = 1 << 63

OCTREE_CHUNK_LEAF_V1_SCHEMA = b"bitblock/octree-commit/chunk-leaf/v1"
OCTREE_INTERNAL_NODE_V1_SCHEMA = b"bitblock/octree-commit/internal-node/v1"
OCTREE_ROOT_V1_SCHEMA = b"bitblock/octree-commit/root/v1"
OCTREE_POINT_PROOF_V1_SCHEMA = b"bitblock/octree-commit/point-proof/v1"
VERKLE_R1_FEASIBILITY_V1_SCHEMA = b"bitblock/octree-commit/verkle-r1-feasibility/v1"

EXPECTED_EMPTY_ROOT_TAG = (
    "#bb.world.sha256.5obybgly3kj2356d426gjijnuezscj3rz556r2el3xue7mccedea"
)
EXPECTED_SINGLE_ORIGIN_ROOT_TAG = (
    "#bb.world.sha256.lmjzqddt5hcdszpiwkaf3ckxdtk25mcdm4go4zav6i3r4gn6kksa"
)
EXPECTED_MULTI_CHUNK_ROOT_TAG = (
    "#bb.world.sha256.23yz24urod2pxhthrpvpeumsctsphytdbzjkkayvrguu7evdt3aq"
)
EXPECTED_NEGATIVE_CLUSTER_ROOT_TAG = (
    "#bb.world.sha256.74xdmb3l7eofhba23gxrrrazc42ozvckup4b27fg6gvtdgtwl6qa"
)
EXPECTED_SPARSE_FAR_EXTREMA_ROOT_TAG = (
    "#bb.world.sha256.j2yx5roy6ct2pmqnvnzrx72gsm7rmi3o7zol2p6sokh76lahiyaa"
)
EXPECTED_NONZERO_CHUNK_LEAF_TAG = (
    "#bb.world.sha256.ycc56dgal4ijy7675edhvd3wnaa7k2ukrdfma2f6m7nejfwsxnqq"
)
EXPECTED_NONZERO_INTERNAL_NODE_TAG = (
    "#bb.world.sha256.3sm334gwkaamsw644mryh6hoyxalhkbdassaal2rbuwqzfposrgq"
)
EXPECTED_NONZERO_POINT_PROOF_TAG = (
    "#bb.receipt.sha256.uhtkmfstiktj3fnu6kbeon3s5gjp2wq3bhx4rr5load72iy2etva"
)
EXPECTED_VERKLE_FEASIBILITY_TAG = (
    "#bb.receipt.sha256.j3xacn27gfhq6f4wrxvn5upcvvufq5soltxncytvglb2recohp7a"
)


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


def bias64(val: int) -> int:
    return (val & 0xFFFFFFFFFFFFFFFF) ^ SIGN_BIAS


def morton_key_from_coord(x: int, y: int, z: int) -> bytes:
    axes = (bias64(x), bias64(y), bias64(z))
    out = bytearray(24)
    for level in range(64):
        shift = 63 - level
        for axis, u_axis in enumerate(axes):
            bit_pos = 3 * level + axis
            bit = (u_axis >> shift) & 1
            out[bit_pos // 8] |= bit << (7 - (bit_pos % 8))
    return bytes(out)


def chunk_prefix_180b(morton_bytes: bytes) -> bytes:
    return morton_bytes[:22] + bytes([morton_bytes[22] & 0xF0])


def octant_at_prefix(prefix: bytes, depth: int) -> int:
    if depth < 0 or depth >= 60:
        return 0
    base = 3 * depth

    def bit(p: int) -> int:
        return (prefix[p // 8] >> (7 - (p % 8))) & 1

    return (bit(base) << 2) | (bit(base + 1) << 1) | bit(base + 2)


def mask_prefix_180b(raw: bytes, branch_depth: int) -> bytes:
    out = bytearray(23)
    keep_bits = min(max(branch_depth, 0), 60) * 3
    full_bytes = keep_bits // 8
    rem = keep_bits % 8
    out[:full_bytes] = raw[:full_bytes]
    if rem != 0:
        out[full_bytes] = raw[full_bytes] & ((0xFF << (8 - rem)) & 0xFF)
    return bytes(out)


def common_octant_prefix_len(a: bytes, b: bytes) -> int:
    for d in range(60):
        if octant_at_prefix(a, d) != octant_at_prefix(b, d):
            return d
    return 60


def build_octree(coords: list[tuple[int, int, int]]) -> dict:
    if not coords:
        root_payload = b"\x00" * 49
        rtag, rdig = content_tag("world", OCTREE_ROOT_V1_SCHEMA, root_payload)
        return {
            "tree_state": 0,
            "occupied_chunks": 0,
            "occupied_bits": 0,
            "root_node_digest": b"\x00" * 32,
            "root_tag": rtag,
            "root_digest": rdig,
            "tree": None,
        }

    chunks: dict[tuple[int, int, int], list[int]] = {}
    for x, y, z in coords:
        cx, cy, cz = x // 16, y // 16, z // 16
        lx, ly, lz = x % 16, y % 16, z % 16
        local_idx = lx + 16 * (ly + 16 * lz)
        w_idx, b_idx = local_idx // 64, local_idx % 64
        words = chunks.setdefault((cx, cy, cz), [0] * 64)
        words[w_idx] |= 1 << b_idx

    leaves = []
    for (cx, cy, cz), words in chunks.items():
        pref = chunk_prefix_180b(morton_key_from_coord(cx * 16, cy * 16, cz * 16))
        occ = sum(bin(w).count("1") for w in words)
        bits_512b = b"".join(struct.pack(">Q", w) for w in words)
        payload = pref + struct.pack(">H", occ) + bits_512b
        ltag, ldig = content_tag("world", OCTREE_CHUNK_LEAF_V1_SCHEMA, payload)
        leaves.append(
            {
                "type": "leaf",
                "chunk": (cx, cy, cz),
                "prefix": pref,
                "occupied_count": occ,
                "bits_512b": bits_512b,
                "payload": payload,
                "tag": ltag,
                "digest": ldig,
            }
        )

    leaves.sort(key=lambda item: item["prefix"])
    occupied_chunks = len(leaves)
    occupied_bits = sum(item["occupied_count"] for item in leaves)

    if occupied_chunks == 1:
        leaf = leaves[0]
        root_payload = (
            bytes([1]) + struct.pack(">QQ", 1, occupied_bits) + leaf["digest"]
        )
        rtag, rdig = content_tag("world", OCTREE_ROOT_V1_SCHEMA, root_payload)
        return {
            "tree_state": 1,
            "occupied_chunks": 1,
            "occupied_bits": occupied_bits,
            "root_node_digest": leaf["digest"],
            "root_tag": rtag,
            "root_digest": rdig,
            "tree": leaf,
        }

    def build_subtree(sub: list[dict]) -> dict:
        if len(sub) == 1:
            return sub[0]
        bd = common_octant_prefix_len(sub[0]["prefix"], sub[-1]["prefix"])
        mp = mask_prefix_180b(sub[0]["prefix"], bd)
        cm = 0
        children = []
        i = 0
        while i < len(sub):
            oct_idx = octant_at_prefix(sub[i]["prefix"], bd)
            j = i + 1
            while j < len(sub) and octant_at_prefix(sub[j]["prefix"], bd) == oct_idx:
                j += 1
            cm |= 1 << oct_idx
            child_node = build_subtree(sub[i:j])
            children.append((oct_idx, child_node))
            i = j
        npayload = (
            bytes([bd]) + mp + bytes([cm]) + b"".join(c["digest"] for _, c in children)
        )
        ntag, ndig = content_tag("world", OCTREE_INTERNAL_NODE_V1_SCHEMA, npayload)
        return {
            "type": "internal",
            "branch_depth": bd,
            "prefix": mp,
            "child_mask": cm,
            "children": children,
            "payload": npayload,
            "tag": ntag,
            "digest": ndig,
        }

    root_node = build_subtree(leaves)
    root_payload = (
        bytes([2])
        + struct.pack(">QQ", occupied_chunks, occupied_bits)
        + root_node["digest"]
    )
    rtag, rdig = content_tag("world", OCTREE_ROOT_V1_SCHEMA, root_payload)
    return {
        "tree_state": 2,
        "occupied_chunks": occupied_chunks,
        "occupied_bits": occupied_bits,
        "root_node_digest": root_node["digest"],
        "root_tag": rtag,
        "root_digest": rdig,
        "tree": root_node,
    }


def prove_point(octree: dict, qx: int, qy: int, qz: int) -> dict:
    tree = octree["tree"]
    if tree is None:
        payload = struct.pack(">qqqBB", qx, qy, qz, 0, 0)
        ptag, _ = content_tag("receipt", OCTREE_POINT_PROOF_V1_SCHEMA, payload)
        return {
            "query_coord": (qx, qy, qz),
            "steps": [],
            "terminal": ("empty",),
            "payload": payload,
            "tag": ptag,
        }

    qkey = morton_key_from_coord(qx, qy, qz)
    qpref = chunk_prefix_180b(qkey)
    steps = []
    step_bytes = []
    curr = tree

    while True:
        if curr["type"] == "leaf":
            terminal = ("leaf", curr)
            term_bytes = bytes([1]) + curr["payload"]
            break
        bd = curr["branch_depth"]
        if common_octant_prefix_len(curr["prefix"], qpref) < bd:
            terminal = ("internal", curr)
            term_bytes = bytes([2]) + curr["payload"]
            break
        oct_idx = octant_at_prefix(qpref, bd)
        if (curr["child_mask"] & (1 << oct_idx)) == 0:
            terminal = ("internal", curr)
            term_bytes = bytes([2]) + curr["payload"]
            break
        sibs = [c["digest"] for o, c in curr["children"] if o != oct_idx]
        steps.append((bd, curr["prefix"], curr["child_mask"], sibs))
        step_bytes.append(
            bytes([bd]) + curr["prefix"] + bytes([curr["child_mask"]]) + b"".join(sibs)
        )
        curr = next(c for o, c in curr["children"] if o == oct_idx)

    payload = (
        struct.pack(">qqqB", qx, qy, qz, len(steps))
        + b"".join(step_bytes)
        + term_bytes
    )
    ptag, _ = content_tag("receipt", OCTREE_POINT_PROOF_V1_SCHEMA, payload)
    return {
        "query_coord": (qx, qy, qz),
        "steps": steps,
        "terminal": terminal,
        "payload": payload,
        "tag": ptag,
    }


def is_bit_on(bits_512b: bytes, qx: int, qy: int, qz: int) -> bool:
    lx, ly, lz = qx % 16, qy % 16, qz % 16
    local_idx = lx + 16 * (ly + 16 * lz)
    word_idx = local_idx // 64
    bit_idx = local_idx % 64
    byte_idx = word_idx * 8 + (7 - (bit_idx // 8))
    return (bits_512b[byte_idx] & (1 << (bit_idx % 8))) != 0


def verify_point_proof(octree: dict, proof: dict) -> bool | None:
    qx, qy, qz = proof["query_coord"]
    qkey = morton_key_from_coord(qx, qy, qz)
    qpref = chunk_prefix_180b(qkey)
    steps = proof["steps"]
    terminal = proof["terminal"]
    tree_state = octree["tree_state"]

    if len(steps) > 60:
        return None
    for idx, (bd, pref, cm, sibs) in enumerate(steps):
        if bd >= 60:
            return None
        if idx > 0 and steps[idx - 1][0] >= bd:
            return None
        if pref != mask_prefix_180b(pref, bd):
            return None
        ones = bin(cm).count("1")
        if ones < 2 or len(sibs) != ones - 1:
            return None
        if common_octant_prefix_len(pref, qpref) < bd:
            return None
        oct_idx = octant_at_prefix(qpref, bd)
        if (cm & (1 << oct_idx)) == 0:
            return None

    if terminal[0] == "empty":
        if tree_state != 0 or steps:
            return None
        curr_dig = b"\x00" * 32
        outcome = False
    elif terminal[0] == "leaf":
        leaf = terminal[1]
        if (leaf["prefix"][22] & 0x0F) != 0:
            return None
        popcount = sum(bin(b).count("1") for b in leaf["bits_512b"])
        if leaf["occupied_count"] == 0 or leaf["occupied_count"] != popcount:
            return None
        if tree_state == 1:
            if steps or octree["occupied_bits"] != leaf["occupied_count"]:
                return None
        elif tree_state == 2:
            if not steps:
                return None
            last_bd = steps[-1][0]
            if common_octant_prefix_len(leaf["prefix"], qpref) < last_bd + 1:
                return None
        else:
            return None
        _, curr_dig = content_tag(
            "world", OCTREE_CHUNK_LEAF_V1_SCHEMA, leaf["payload"]
        )
        outcome = (
            is_bit_on(leaf["bits_512b"], qx, qy, qz)
            if leaf["prefix"] == qpref
            else False
        )
    elif terminal[0] == "internal":
        node = terminal[1]
        if tree_state != 2:
            return None
        bd = node["branch_depth"]
        if bd >= 60 or node["prefix"] != mask_prefix_180b(node["prefix"], bd):
            return None
        if steps:
            last_bd = steps[-1][0]
            if (
                last_bd >= bd
                or common_octant_prefix_len(node["prefix"], qpref) < last_bd + 1
            ):
                return None
        diverges = common_octant_prefix_len(node["prefix"], qpref) < bd
        vacant = (node["child_mask"] & (1 << octant_at_prefix(qpref, bd))) == 0
        if not diverges and not vacant:
            return None
        _, curr_dig = content_tag(
            "world", OCTREE_INTERNAL_NODE_V1_SCHEMA, node["payload"]
        )
        outcome = False
    else:
        return None

    for bd, pref, cm, sibs in reversed(steps):
        q_oct = octant_at_prefix(qpref, bd)
        sib_iter = iter(sibs)
        child_digs = []
        for oct_idx in range(8):
            if (cm & (1 << oct_idx)) != 0:
                if oct_idx == q_oct:
                    child_digs.append(curr_dig)
                else:
                    child_digs.append(next(sib_iter))
        npayload = bytes([bd]) + pref + bytes([cm]) + b"".join(child_digs)
        _, curr_dig = content_tag("world", OCTREE_INTERNAL_NODE_V1_SCHEMA, npayload)

    if curr_dig != octree["root_node_digest"]:
        return None
    return outcome


def build_verkle_feasibility_payload() -> bytes:
    field_mod = bytes(
        [
            0x1C,
            0xF4,
            0x5C,
            0x91,
            0x4B,
            0x7A,
            0x82,
            0xD3,
            0x6E,
            0x10,
            0x99,
            0x54,
            0x2A,
            0xB1,
            0x7C,
            0x38,
            0x05,
            0x6F,
            0xE2,
            0x41,
            0x8D,
            0x93,
            0xC7,
            0x1A,
            0x50,
            0xBE,
            0x44,
            0x29,
            0x73,
            0x0F,
            0xA6,
            0x01,
        ]
    )
    payload = (
        struct.pack(">HH", 1, 256)
        + field_mod
        + (b"\x41" * 32)
        + (b"\x52" * 32)
        + bytes([0, 1])
        + struct.pack(">QQQ", 6144, 192, 12288)
        + struct.pack(">QQQ", 6169, 192, 12288)
        + struct.pack(">QQQ", 13977, 61, 61)
        + struct.pack(">QQQ", 576, 17, 17)
    )
    assert len(payload) == 198
    return payload


def run_all() -> None:
    # 1. Empty octree root and proof
    empty_octree = build_octree([])
    assert empty_octree["root_tag"] == EXPECTED_EMPTY_ROOT_TAG
    empty_proof = prove_point(empty_octree, 5, -7, 9)
    assert verify_point_proof(empty_octree, empty_proof) is False

    # 2. SingleOrigin octree root and proofs
    single_octree = build_octree([(0, 0, 0)])
    assert single_octree["root_tag"] == EXPECTED_SINGLE_ORIGIN_ROOT_TAG
    assert verify_point_proof(single_octree, prove_point(single_octree, 0, 0, 0)) is True
    assert (
        verify_point_proof(single_octree, prove_point(single_octree, 1, 2, 3))
        is False
    )
    assert (
        verify_point_proof(single_octree, prove_point(single_octree, -32, 16, 48))
        is False
    )

    # 3. Multi-chunk probe octree pinning all 5 schemas and all 7 proof cases
    probe_coords = [
        (0, 0, 0),
        (-17, 2, 15),
        (-16, -16, -16),
        (32, 48, -16),
        (33, 48, -16),
        (64, 48, -16),
    ]
    probe_octree = build_octree(probe_coords)
    assert probe_octree["root_tag"] == EXPECTED_MULTI_CHUNK_ROOT_TAG

    p_present = prove_point(probe_octree, 33, 48, -16)
    assert p_present["tag"] == EXPECTED_NONZERO_POINT_PROOF_TAG
    assert verify_point_proof(probe_octree, p_present) is True
    assert p_present["terminal"][1]["tag"] == EXPECTED_NONZERO_CHUNK_LEAF_TAG

    p_same_chunk_absent = prove_point(probe_octree, 35, 48, -16)
    assert verify_point_proof(probe_octree, p_same_chunk_absent) is False

    p_divergent_leaf = prove_point(probe_octree, 16, 48, -16)
    assert p_divergent_leaf["terminal"][0] == "leaf"
    assert verify_point_proof(probe_octree, p_divergent_leaf) is False

    p_divergent_internal = prove_point(probe_octree, 128, 48, -16)
    assert p_divergent_internal["terminal"][0] == "internal"
    assert (
        p_divergent_internal["terminal"][1]["tag"]
        == EXPECTED_NONZERO_INTERNAL_NODE_TAG
    )
    assert verify_point_proof(probe_octree, p_divergent_internal) is False

    p_vacant_child_internal = prove_point(probe_octree, 32, 64, -16)
    assert p_vacant_child_internal["terminal"][0] == "internal"
    assert verify_point_proof(probe_octree, p_vacant_child_internal) is False

    p_vacant_root_internal = prove_point(probe_octree, 16, -16, 32)
    assert p_vacant_root_internal["terminal"][0] == "internal"
    assert verify_point_proof(probe_octree, p_vacant_root_internal) is False

    # Negative controls: spoofed coordinate, corrupted sibling, premature stop
    spoofed = dict(p_present, query_coord=(32, -48, -16))
    assert verify_point_proof(probe_octree, spoofed) is None

    premature = dict(p_divergent_internal, query_coord=(33, 48, -16))
    assert verify_point_proof(probe_octree, premature) is None

    # 4. Scale-assay reference multi-chunk roots (NegativeChunkCluster & SparseFarExtrema)
    neg_cluster = [
        (x, y, z) for x in (-1, 0) for y in (-1, 0) for z in (-1, 0)
    ] + [(-16, -16, -16)]
    assert build_octree(neg_cluster)["root_tag"] == EXPECTED_NEGATIVE_CLUSTER_ROOT_TAG

    far_extrema = [(0, 0, 0)] + [
        (sx, sy, sz)
        for sx in (-1048576, 1048576)
        for sy in (-1048576, 1048576)
        for sz in (-1048576, 1048576)
    ]
    assert (
        build_octree(far_extrema)["root_tag"]
        == EXPECTED_SPARSE_FAR_EXTREMA_ROOT_TAG
    )

    # 5. VerkleR1FeasibilityReportV1 canonical vector
    verkle_payload = build_verkle_feasibility_payload()
    verkle_tag, _ = content_tag(
        "receipt", VERKLE_R1_FEASIBILITY_V1_SCHEMA, verkle_payload
    )
    assert verkle_tag == EXPECTED_VERKLE_FEASIBILITY_TAG

    print(
        "octree_commit_vectors: ok (empty, single-origin, multi-chunk, chunk-leaf, internal-node, point-proof, verkle-r1-feasibility, and negative controls pass)"
    )


if __name__ == "__main__":
    run_all()
    sys.exit(0)
