#!/usr/bin/env python3
"""Independent stdlib vectors for SCALE-1 workload assay and comparative synthesis.

This script imports no Bitblock/Rust code. It independently reconstructs all 5
SCALE-1 reference worlds (SingleOrigin, DenseCube16, HollowShell16,
NegativeChunkCluster, SparseFarExtrema), computes chunk/octant occupancy,
6-face surface-vs-buried accounting, view-frustum chunk culling, raycast chunk
pruning, and the three authenticated-state root digests (MerklePatriciaTrie,
SparseMerkleTree, SparseOctreeV1), and verifies bit-exact ContentTag parity for:
  * bitblock/scale-assay/workload-report/v1
"""

from __future__ import annotations

import base64
import hashlib
import math
import struct
import sys

KEY_BITS = 192
KEY_BYTES = 24
SIGN_BIAS = 1 << 63

TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
COMMIT_PROFILE_ID = b"bitblock/commit-profile/r1"
TREE_DOMAIN = b"bitblock/world-root/v1"
CODEC_VERSION = 1

COMMIT_R1_WORLD_PATRICIA_SCHEMA = b"bitblock/commit-r1/world-patricia/v1"
COMMIT_R1_WORLD_SMT_SCHEMA = b"bitblock/commit-r1/world-smt/v1"
OCTREE_CHUNK_LEAF_V1_SCHEMA = b"bitblock/octree-commit/chunk-leaf/v1"
OCTREE_INTERNAL_NODE_V1_SCHEMA = b"bitblock/octree-commit/internal-node/v1"
OCTREE_ROOT_V1_SCHEMA = b"bitblock/octree-commit/root/v1"
SCALE_WORKLOAD_REPORT_V1_SCHEMA = b"bitblock/scale-assay/workload-report/v1"

EXPECTED_SCALE_WORKLOAD_REPORT_V1_TAG = (
    "#bb.receipt.sha256.xq6oxiyyao3khyp3i7r6zbz3zwewouc75btw4s733xp62zeic3ma"
)
EXPECTED_SINGLE_ORIGIN_OCTREE_TAG = (
    "#bb.world.sha256.lmjzqddt5hcdszpiwkaf3ckxdtk25mcdm4go4zav6i3r4gn6kksa"
)
EXPECTED_NEGATIVE_CLUSTER_OCTREE_TAG = (
    "#bb.world.sha256.74xdmb3l7eofhba23gxrrrazc42ozvckup4b27fg6gvtdgtwl6qa"
)
EXPECTED_SPARSE_FAR_EXTREMA_OCTREE_TAG = (
    "#bb.world.sha256.j2yx5roy6ct2pmqnvnzrx72gsm7rmi3o7zol2p6sokh76lahiyaa"
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


def coord_to_key_bytes(x: int, y: int, z: int) -> bytes:
    return struct.pack(">QQQ", bias64(x), bias64(y), bias64(z))


def key_bit_at(key_bytes: bytes, depth: int) -> int:
    return (key_bytes[depth // 8] >> (7 - (depth % 8))) & 1


def node_hash(node_type: int, depth: int, payload: bytes) -> bytes:
    preimage = (
        struct.pack(">H", len(COMMIT_PROFILE_ID))
        + COMMIT_PROFILE_ID
        + struct.pack(">H", len(TREE_DOMAIN))
        + TREE_DOMAIN
        + struct.pack(">H", len(HASH_SUITE))
        + HASH_SUITE
        + struct.pack(">H", CODEC_VERSION)
        + bytes([node_type, depth])
        + struct.pack(">I", len(payload))
        + payload
    )
    return hashlib.sha256(preimage).digest()


def compute_smt_defaults() -> list[bytes]:
    d = [b""] * (KEY_BITS + 1)
    d[KEY_BITS] = node_hash(0, KEY_BITS, b"")
    for depth in range(KEY_BITS - 1, -1, -1):
        d[depth] = node_hash(2, depth, d[depth + 1] + d[depth + 1])
    return d


SMT_DEFAULTS = compute_smt_defaults()
SMT_PRESENT_LEAF = node_hash(1, KEY_BITS, b"\x01")


def smt_build_root(sorted_keys: list[bytes], depth: int = 0) -> bytes:
    if not sorted_keys:
        return SMT_DEFAULTS[depth]
    if depth == KEY_BITS:
        return SMT_PRESENT_LEAF
    split = 0
    while split < len(sorted_keys) and key_bit_at(sorted_keys[split], depth) == 0:
        split += 1
    left_h = smt_build_root(sorted_keys[:split], depth + 1)
    right_h = smt_build_root(sorted_keys[split:], depth + 1)
    return node_hash(2, depth, left_h + right_h)


def mpt_pack_bits(bits: list[int]) -> bytes:
    out = bytearray()
    curr = 0
    count = 0
    for b in bits:
        curr |= (b & 1) << (7 - count)
        count += 1
        if count == 8:
            out.append(curr)
            curr = 0
            count = 0
    if count > 0:
        out.append(curr)
    return bytes(out)


def mpt_build_root(sorted_keys: list[bytes], depth: int = 0) -> bytes:
    if not sorted_keys:
        return node_hash(0, 0, b"")
    if len(sorted_keys) == 1:
        suffix_len = KEY_BITS - depth
        bits = [key_bit_at(sorted_keys[0], d) for d in range(depth, KEY_BITS)]
        payload = struct.pack(">H", suffix_len) + mpt_pack_bits(bits) + b"\x01"
        return node_hash(1, depth, payload)
    common = 0
    for d in range(depth, KEY_BITS):
        if key_bit_at(sorted_keys[0], d) == key_bit_at(sorted_keys[-1], d):
            common += 1
        else:
            break
    if common > 0:
        sbits = [key_bit_at(sorted_keys[0], depth + i) for i in range(common)]
        chash = mpt_build_root(sorted_keys, depth + common)
        payload = struct.pack(">H", len(sbits)) + mpt_pack_bits(sbits) + chash
        return node_hash(2, depth, payload)
    split = 0
    while split < len(sorted_keys) and key_bit_at(sorted_keys[split], depth) == 0:
        split += 1
    lh = mpt_build_root(sorted_keys[:split], depth + 1)
    rh = mpt_build_root(sorted_keys[split:], depth + 1)
    return node_hash(3, depth, lh + rh)


def morton_chunk_prefix_180b(x: int, y: int, z: int) -> bytes:
    axes = (bias64(x), bias64(y), bias64(z))
    out = bytearray(24)
    for level in range(64):
        shift = 63 - level
        for axis, u_axis in enumerate(axes):
            bit_pos = 3 * level + axis
            bit = (u_axis >> shift) & 1
            out[bit_pos // 8] |= bit << (7 - (bit_pos % 8))
    return bytes(out[:22]) + bytes([out[22] & 0xF0])


def octant_at_prefix(prefix: bytes, depth: int) -> int:
    base = 3 * depth
    b0 = (prefix[base // 8] >> (7 - (base % 8))) & 1
    b1 = (prefix[(base + 1) // 8] >> (7 - ((base + 1) % 8))) & 1
    b2 = (prefix[(base + 2) // 8] >> (7 - ((base + 2) % 8))) & 1
    return (b0 << 2) | (b1 << 1) | b2


def mask_prefix_180b(raw: bytes, branch_depth: int) -> bytes:
    out = bytearray(23)
    keep_bits = branch_depth * 3
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


def octree_root_tag_and_digest(coords: set[tuple[int, int, int]]) -> tuple[str, bytes]:
    chunks: dict[tuple[int, int, int], list[int]] = {}
    for x, y, z in coords:
        cx, cy, cz = x // 16, y // 16, z // 16
        lx, ly, lz = x % 16, y % 16, z % 16
        local_idx = lx + 16 * (ly + 16 * lz)
        words = chunks.setdefault((cx, cy, cz), [0] * 64)
        words[local_idx // 64] |= 1 << (local_idx % 64)

    leaves = []
    for (cx, cy, cz), words in chunks.items():
        pref = morton_chunk_prefix_180b(cx * 16, cy * 16, cz * 16)
        occ = sum(bin(w).count("1") for w in words)
        bits_512b = b"".join(struct.pack(">Q", w) for w in words)
        lpayload = pref + struct.pack(">H", occ) + bits_512b
        _, ldig = content_tag("world", OCTREE_CHUNK_LEAF_V1_SCHEMA, lpayload)
        leaves.append((pref, occ, ldig))

    leaves.sort(key=lambda item: item[0])
    occ_chunks = len(leaves)
    occ_bits = sum(item[1] for item in leaves)
    if occ_chunks == 1:
        rpayload = bytes([1]) + struct.pack(">QQ", 1, occ_bits) + leaves[0][2]
        return content_tag("world", OCTREE_ROOT_V1_SCHEMA, rpayload)

    def build_sub(sub: list[tuple[bytes, int, bytes]]) -> bytes:
        if len(sub) == 1:
            return sub[0][2]
        bd = common_octant_prefix_len(sub[0][0], sub[-1][0])
        mp = mask_prefix_180b(sub[0][0], bd)
        cm = 0
        cdigs = []
        i = 0
        while i < len(sub):
            oct_idx = octant_at_prefix(sub[i][0], bd)
            j = i + 1
            while j < len(sub) and octant_at_prefix(sub[j][0], bd) == oct_idx:
                j += 1
            cm |= 1 << oct_idx
            cdigs.append(build_sub(sub[i:j]))
            i = j
        npayload = bytes([bd]) + mp + bytes([cm]) + b"".join(cdigs)
        _, ndig = content_tag("world", OCTREE_INTERNAL_NODE_V1_SCHEMA, npayload)
        return ndig

    root_node_dig = build_sub(leaves)
    rpayload = bytes([2]) + struct.pack(">QQ", occ_chunks, occ_bits) + root_node_dig
    return content_tag("world", OCTREE_ROOT_V1_SCHEMA, rpayload)


def build_workload_coords(workload_id: int) -> set[tuple[int, int, int]]:
    coords: set[tuple[int, int, int]] = {(0, 0, 0)}
    if workload_id == 0:
        return coords
    if workload_id == 1:
        for x in range(16):
            for y in range(16):
                for z in range(16):
                    coords.add((x, y, z))
        return coords
    if workload_id == 2:
        for x in range(16):
            for y in range(16):
                for z in range(16):
                    if x in (0, 15) or y in (0, 15) or z in (0, 15):
                        coords.add((x, y, z))
        return coords
    if workload_id == 3:
        for x in (-1, 0):
            for y in (-1, 0):
                for z in (-1, 0):
                    coords.add((x, y, z))
        coords.add((-16, -16, -16))
        return coords
    if workload_id == 4:
        span = 1_048_576
        for sx in (-span, span):
            for sy in (-span, span):
                for sz in (-span, span):
                    coords.add((sx, sy, sz))
        return coords
    raise ValueError(f"unknown workload_id: {workload_id}")


def aabb_outside_spawn_frustum(
    min_pt: tuple[float, float, float], max_pt: tuple[float, float, float]
) -> bool:
    # Camera::for_observer_spawn() at (0, 0, -1), forward=(0, 0, 1), right=(1, 0, 0), up=(0, 1, 0)
    # 64x64 viewport, fov = pi / 3
    focal = 64.0 / (2.0 * math.tan(math.pi / 6.0))
    hw = 32.0
    hh = 32.0
    corners = [
        (x, y, z)
        for x in (min_pt[0], max_pt[0])
        for y in (min_pt[1], max_pt[1])
        for z in (min_pt[2], max_pt[2])
    ]
    all_behind = True
    all_left = True
    all_right = True
    all_above = True
    all_below = True
    for cx, cy, cz in corners:
        x_cam = cx
        y_cam = cy
        z_cam = cz - (-1.0)
        all_behind = all_behind and (z_cam <= 0.0)
        all_left = all_left and (z_cam > 0.0 and focal * x_cam < -(hw + 1.0) * z_cam)
        all_right = all_right and (z_cam > 0.0 and focal * x_cam > (hw + 1.0) * z_cam)
        all_above = all_above and (z_cam > 0.0 and focal * y_cam > (hh + 1.0) * z_cam)
        all_below = all_below and (z_cam > 0.0 and focal * y_cam < -(hh + 1.0) * z_cam)
    return all_behind or all_left or all_right or all_above or all_below


def ray_aabb_intersects_spawn_center_ray(
    min_pt: tuple[float, float, float], max_pt: tuple[float, float, float]
) -> bool:
    # Ray origin = (0, 0, -1), direction = (0, 0, 1)
    if 0.0 < min_pt[0] or 0.0 > max_pt[0]:
        return False
    if 0.0 < min_pt[1] or 0.0 > max_pt[1]:
        return False
    t_exit = (max_pt[2] - (-1.0)) / 1.0
    return t_exit >= 0.0


def assay_workload_entry(workload_id: int) -> dict:
    coords = build_workload_coords(workload_id)
    allocated_bits = len(coords)

    chunk_coords_map: dict[tuple[int, int, int], list[tuple[int, int, int]]] = {}
    for x, y, z in coords:
        chunk_coords_map.setdefault((x // 16, y // 16, z // 16), []).append((x, y, z))
    allocated_chunks = len(chunk_coords_map)

    root_octants = {
        (cx // 256, cy // 256, cz // 256) for (cx, cy, cz) in chunk_coords_map
    }
    active_root_octants = len(root_octants)

    exposed_faces = 0
    buried_voxels = 0
    neighbors = ((0, 0, -1), (0, 0, 1), (-1, 0, 0), (1, 0, 0), (0, -1, 0), (0, 1, 0))
    for x, y, z in coords:
        exp = sum(1 for dx, dy, dz in neighbors if (x + dx, y + dy, z + dz) not in coords)
        exposed_faces += exp
        if exp == 0:
            buried_voxels += 1
    total_faces = allocated_bits * 6
    buried_faces = total_faces - exposed_faces

    frustum_culled = 0
    ray_tested = 0
    for chunk_key in sorted(chunk_coords_map):
        pts = chunk_coords_map[chunk_key]
        min_pt = (
            min(p[0] for p in pts) - 0.5,
            min(p[1] for p in pts) - 0.5,
            min(p[2] for p in pts) - 0.5,
        )
        max_pt = (
            max(p[0] for p in pts) + 0.5,
            max(p[1] for p in pts) + 0.5,
            max(p[2] for p in pts) + 0.5,
        )
        if aabb_outside_spawn_frustum(min_pt, max_pt):
            frustum_culled += 1
        if ray_aabb_intersects_spawn_center_ray(min_pt, max_pt):
            ray_tested += 1
    frustum_vis = allocated_chunks - frustum_culled
    ray_pruned = allocated_chunks - ray_tested

    sorted_keys = sorted(coord_to_key_bytes(x, y, z) for (x, y, z) in coords)
    _, patricia_dig = content_tag(
        "world", COMMIT_R1_WORLD_PATRICIA_SCHEMA, mpt_build_root(sorted_keys, 0)
    )
    _, smt_dig = content_tag(
        "world", COMMIT_R1_WORLD_SMT_SCHEMA, smt_build_root(sorted_keys, 0)
    )
    oct_tag, oct_dig = octree_root_tag_and_digest(coords)

    payload = (
        struct.pack(
            ">B11Q",
            workload_id,
            allocated_bits,
            allocated_chunks,
            active_root_octants,
            total_faces,
            exposed_faces,
            buried_faces,
            buried_voxels,
            frustum_vis,
            frustum_culled,
            ray_tested,
            ray_pruned,
        )
        + patricia_dig
        + smt_dig
        + oct_dig
    )
    assert len(payload) == 185
    return {
        "workload_id": workload_id,
        "metrics": (
            allocated_bits,
            allocated_chunks,
            active_root_octants,
            total_faces,
            exposed_faces,
            buried_faces,
            buried_voxels,
            frustum_vis,
            frustum_culled,
            ray_tested,
            ray_pruned,
        ),
        "octree_tag": oct_tag,
        "payload": payload,
    }


def decode_workload_report(data: bytes) -> list[tuple] | None:
    if len(data) != 929:
        return None
    (count,) = struct.unpack(">I", data[:4])
    if count != 5:
        return None
    entries = []
    for idx in range(5):
        base = 4 + idx * 185
        rec = data[base : base + 185]
        vals = struct.unpack(">B11Q", rec[:89])
        wid = vals[0]
        (
            bits,
            chunks,
            roots,
            total_f,
            exp_f,
            bur_f,
            bur_v,
            f_vis,
            f_cul,
            r_tst,
            r_prn,
        ) = vals[1:]
        p_dig = rec[89:121]
        s_dig = rec[121:153]
        o_dig = rec[153:185]
        if wid != idx:
            return None
        if (
            bits == 0
            or chunks == 0
            or roots == 0
            or chunks > bits
            or roots > chunks
            or bur_v > bits
        ):
            return None
        if total_f != bits * 6 or exp_f + bur_f != total_f:
            return None
        if f_vis + f_cul != chunks or r_tst + r_prn != chunks:
            return None
        if p_dig == b"\x00" * 32 or s_dig == b"\x00" * 32 or o_dig == b"\x00" * 32:
            return None
        entries.append(vals)
    return entries


def run_all() -> None:
    entries = [assay_workload_entry(i) for i in range(5)]

    # Verify integer metrics and pinned octree roots for all 5 workloads
    assert entries[0]["metrics"] == (1, 1, 1, 6, 6, 0, 0, 1, 0, 1, 0)
    assert entries[0]["octree_tag"] == EXPECTED_SINGLE_ORIGIN_OCTREE_TAG

    assert entries[1]["metrics"] == (4096, 1, 1, 24576, 1536, 23040, 2744, 1, 0, 1, 0)
    assert entries[2]["metrics"] == (1352, 1, 1, 8112, 2712, 5400, 0, 1, 0, 1, 0)

    assert entries[3]["metrics"] == (9, 8, 8, 54, 30, 24, 0, 8, 0, 2, 6)
    assert entries[3]["octree_tag"] == EXPECTED_NEGATIVE_CLUSTER_OCTREE_TAG

    assert entries[4]["metrics"] == (9, 9, 9, 54, 54, 0, 0, 1, 8, 1, 8)
    assert entries[4]["octree_tag"] == EXPECTED_SPARSE_FAR_EXTREMA_OCTREE_TAG

    report_payload = struct.pack(">I", 5) + b"".join(e["payload"] for e in entries)
    assert len(report_payload) == 929
    assert decode_workload_report(report_payload) is not None

    report_tag, _ = content_tag(
        "receipt", SCALE_WORKLOAD_REPORT_V1_SCHEMA, report_payload
    )
    assert (
        report_tag == EXPECTED_SCALE_WORKLOAD_REPORT_V1_TAG
    ), f"ScaleWorkloadReportV1 tag mismatch: {report_tag}"

    # Negative controls
    assert decode_workload_report(report_payload[:-1]) is None
    assert decode_workload_report(struct.pack(">I", 4) + report_payload[4:]) is None

    bad_id = bytearray(report_payload)
    bad_id[4] = 9
    assert decode_workload_report(bytes(bad_id)) is None

    bad_total_faces = bytearray(report_payload)
    bad_total_faces[4 + 25 + 7] ^= 1
    assert decode_workload_report(bytes(bad_total_faces)) is None

    bad_frustum = bytearray(report_payload)
    bad_frustum[4 + 57 + 7] ^= 1
    assert decode_workload_report(bytes(bad_frustum)) is None

    bad_ray = bytearray(report_payload)
    bad_ray[4 + 73 + 7] ^= 1
    assert decode_workload_report(bytes(bad_ray)) is None

    bad_digest = bytearray(report_payload)
    bad_digest[4 + 89 : 4 + 121] = b"\x00" * 32
    assert decode_workload_report(bytes(bad_digest)) is None

    print(
        "scale_assay_vectors: ok (all 5 SCALE-1 workloads, surface/frustum/ray metrics, 15 root digests, canonical report tag, and negative controls pass)"
    )


if __name__ == "__main__":
    run_all()
    sys.exit(0)
