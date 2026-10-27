#!/usr/bin/env python3
"""Independent stdlib vectors for COMMIT-R1 authenticated-state candidates.

This file intentionally imports no Bitblock/Rust code. It reconstructs both
Candidate A (Fixed-Key Binary Merkle Patricia Trie) and Candidate B (192-level
Sparse Merkle Tree) directly from the specification in docs/AUTHENTICATED_STATE_PLAN.md
and asserts that both candidates produce bit-identical roots and verifiable witnesses.

The schemas vectored here:
  * bitblock/commit-profile/r1
  * bitblock/world-root/v1
  * bitblock/commit-r1/world-patricia/v1
  * bitblock/commit-r1/world-smt/v1
  * bitblock/commit-r1/patricia-proof/v1
  * bitblock/commit-r1/smt-proof/v1
  * bitblock/commit-r1/assay-report/v1
  * bitblock/commit-r1/morton-key/v1
  * bitblock/commit-r1/morton-gap-stream/v1
"""

from __future__ import annotations

import base64
import hashlib
import struct
import sys

KEY_BITS = 192
KEY_BYTES = 24
COORD_BITS = 64
SIGN_BIAS = 1 << 63

TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
COMMIT_PROFILE_ID = b"bitblock/commit-profile/r1"
TREE_DOMAIN = b"bitblock/world-root/v1"
CODEC_VERSION = 1

COMMIT_R1_WORLD_PATRICIA_SCHEMA = b"bitblock/commit-r1/world-patricia/v1"
COMMIT_R1_WORLD_SMT_SCHEMA = b"bitblock/commit-r1/world-smt/v1"
COMMIT_R1_PATRICIA_PROOF_SCHEMA = b"bitblock/commit-r1/patricia-proof/v1"
COMMIT_R1_SMT_PROOF_SCHEMA = b"bitblock/commit-r1/smt-proof/v1"
COMMIT_R1_ASSAY_REPORT_SCHEMA = b"bitblock/commit-r1/assay-report/v1"
MORTON_KEY_V1_SCHEMA = b"bitblock/commit-r1/morton-key/v1"
MORTON_GAP_STREAM_V1_SCHEMA = b"bitblock/commit-r1/morton-gap-stream/v1"

SMT_EMPTY_LEAF_TYPE = 0
SMT_PRESENT_LEAF_TYPE = 1
SMT_BRANCH_TYPE = 2

MPT_EMPTY_TYPE = 0
MPT_LEAF_TYPE = 1
MPT_EXTENSION_TYPE = 2
MPT_BRANCH_TYPE = 3

STEP_TAG_BRANCH = 1
STEP_TAG_EXTENSION = 2
TERM_TAG_EMPTY = 0
TERM_TAG_LEAF = 1
TERM_TAG_DIVERGENT_EXT = 2

PRESENT_VALUE_ON = bytes([1])

# Pinned golden answers matching crates/research/src/commit_r1.rs
EXPECTED_EMPTY_SMT_HEX = (
    "1e43d15c5ca2ab68222b35b035b98cfa9f4b92bd952553e24c9922102445685e"
)
EXPECTED_EMPTY_PATRICIA_HEX = (
    "ce7ffb25b9babd0735a4803e66480adfab6e6876c20c684969776dd39fa472dd"
)
EXPECTED_BEDROCK_SMT_TAG = (
    "#bb.world.sha256.s62smydfcpkdz2fmdnmmiwn3k5o2uurmtqhhymvtxl5wk73cim3a"
)
EXPECTED_BEDROCK_PATRICIA_TAG = (
    "#bb.world.sha256.szkkbgkgckrmeembx2lifcleyel2zydi2i4e6lsdi6c3kr25uwcq"
)
EXPECTED_3KEY_SMT_PROOF_TAG = (
    "#bb.receipt.sha256.ke5w46p75ibf4owqccyjxqaryc6dpq3u5lglb7a56upvck4tinia"
)
EXPECTED_3KEY_PATRICIA_PROOF_TAG = (
    "#bb.receipt.sha256.rjm2d4ykhamhdxkbc3gv2pmeqdgjeiop7gfvq3cf4py4rejonnqq"
)
EXPECTED_ZERO_MORTON_KEY_TAG = (
    "#bb.world.sha256.ml3dfiypdfxv6jfv5stpsau5fmux6wffg3vdw62lul2gmcjc5r7q"
)
EXPECTED_NONZERO_MORTON_KEY_TAG = (
    "#bb.world.sha256.qdyxv5zm5jugwjmfbfyz5zjvqfxylnv3qedxqvik4kazxqrabuja"
)
EXPECTED_EMPTY_MORTON_GAP_STREAM_TAG = (
    "#bb.receipt.sha256.2rpg7dirkby5eyu7glmwwfsocoslxvrc2a6obmoaqykgmgxhfsza"
)
EXPECTED_5KEY_MORTON_GAP_STREAM_TAG = (
    "#bb.receipt.sha256.zs7hrcgejo572r64u3f3m2voxtty62sp242jk7k66ldu55vfou5q"
)


def field(val: bytes) -> bytes:
    return struct.pack(">H", len(val)) + val


def content_tag_render(kind: str, schema: bytes, payload: bytes) -> str:
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
    return f"#bb.{kind}.sha256.{encoded}"


def bias64(val: int) -> int:
    """Bijective map: two's complement 64-bit int with sign bit flipped."""
    return (val & 0xFFFFFFFFFFFFFFFF) ^ SIGN_BIAS


def coord_to_key_bytes(x: int, y: int, z: int) -> bytes:
    bx = bias64(x)
    by = bias64(y)
    bz = bias64(z)
    return struct.pack(">QQQ", bx, by, bz)


def key_bit_at(key_bytes: bytes, depth: int) -> int:
    byte_idx = depth // 8
    bit_idx = 7 - (depth % 8)
    return (key_bytes[byte_idx] >> bit_idx) & 1


def morton_key_from_coord(x: int, y: int, z: int) -> bytes:
    axes = (bias64(x), bias64(y), bias64(z))
    out = bytearray(KEY_BYTES)
    for level in range(64):
        shift = 63 - level
        for axis, u_axis in enumerate(axes):
            bit_pos = 3 * level + axis
            bit = (u_axis >> shift) & 1
            out[bit_pos // 8] |= bit << (7 - (bit_pos % 8))
    return bytes(out)


def morton_key_to_coord(key_bytes: bytes) -> tuple[int, int, int]:
    assert len(key_bytes) == KEY_BYTES
    axes = [0, 0, 0]
    for level in range(64):
        shift = 63 - level
        for axis in range(3):
            bit_pos = 3 * level + axis
            bit = (key_bytes[bit_pos // 8] >> (7 - (bit_pos % 8))) & 1
            axes[axis] |= bit << shift
    res = []
    for u_axis in axes:
        raw = u_axis ^ SIGN_BIAS
        if raw >= (1 << 63):
            raw -= 1 << 64
        res.append(raw)
    return (res[0], res[1], res[2])


def morton_octant_at(key_bytes: bytes, level: int) -> int:
    if level < 0 or level >= 64:
        return 0
    base = 3 * level
    b0 = (key_bytes[base // 8] >> (7 - (base % 8))) & 1
    b1 = (key_bytes[(base + 1) // 8] >> (7 - ((base + 1) % 8))) & 1
    b2 = (key_bytes[(base + 2) // 8] >> (7 - ((base + 2) % 8))) & 1
    return (b0 << 2) | (b1 << 1) | b2


def morton_chunk_prefix_180b(key_bytes: bytes) -> bytes:
    out = bytearray(23)
    out[:22] = key_bytes[:22]
    out[22] = key_bytes[22] & 0xF0
    return bytes(out)


def morton_gap_stream_encode(coords: list[tuple[int, int, int]]) -> bytes:
    keys = sorted(morton_key_from_coord(x, y, z) for (x, y, z) in coords)
    for i in range(1, len(keys)):
        if keys[i - 1] == keys[i]:
            raise ValueError("duplicate coordinate in morton gap stream")
    out = bytearray(struct.pack(">I", len(keys)))
    if not keys:
        return bytes(out)
    out.extend(keys[0])
    for i in range(1, len(keys)):
        prev = keys[i - 1]
        curr = keys[i]
        k = 0
        while k < KEY_BYTES and prev[k] == curr[k]:
            k += 1
        assert k < KEY_BYTES and curr[k] > prev[k]
        out.append(k)
        out.extend(curr[k:])
    return bytes(out)


def morton_gap_stream_decode(data: bytes) -> list[bytes] | None:
    if len(data) < 4:
        return None
    (count,) = struct.unpack(">I", data[:4])
    cursor = 4
    if count == 0:
        return [] if cursor == len(data) else None
    if cursor + KEY_BYTES > len(data):
        return None
    first = data[cursor : cursor + KEY_BYTES]
    cursor += KEY_BYTES
    keys = [first]
    prev = first
    for _ in range(1, count):
        if cursor >= len(data):
            return None
        k = data[cursor]
        cursor += 1
        if k >= KEY_BYTES:
            return None
        suffix_len = KEY_BYTES - k
        if cursor + suffix_len > len(data):
            return None
        suffix = data[cursor : cursor + suffix_len]
        cursor += suffix_len
        if suffix[0] <= prev[k]:
            return None
        curr = prev[:k] + suffix
        keys.append(curr)
        prev = curr
    if cursor != len(data):
        return None
    return keys


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


# =========================================================================
# SPARSE MERKLE TREE
# =========================================================================

def compute_smt_defaults() -> list[bytes]:
    d = [b""] * (KEY_BITS + 1)
    d[KEY_BITS] = node_hash(SMT_EMPTY_LEAF_TYPE, KEY_BITS, b"")
    depth = KEY_BITS
    while depth > 0:
        depth -= 1
        payload = d[depth + 1] + d[depth + 1]
        d[depth] = node_hash(SMT_BRANCH_TYPE, depth, payload)
    return d


SMT_DEFAULTS = compute_smt_defaults()


def smt_build_subtree(sorted_keys: list[bytes], depth: int) -> bytes:
    if not sorted_keys:
        return SMT_DEFAULTS[depth]
    if depth == KEY_BITS:
        return node_hash(SMT_PRESENT_LEAF_TYPE, KEY_BITS, PRESENT_VALUE_ON)

    split = 0
    while split < len(sorted_keys) and key_bit_at(sorted_keys[split], depth) == 0:
        split += 1

    left = sorted_keys[:split]
    right = sorted_keys[split:]

    left_hash = smt_build_subtree(left, depth + 1)
    right_hash = smt_build_subtree(right, depth + 1)

    payload = left_hash + right_hash
    return node_hash(SMT_BRANCH_TYPE, depth, payload)


def smt_prove(sorted_keys: list[bytes], query_key: bytes) -> tuple[bytes, list[bytes], bool]:
    is_present = query_key in sorted_keys
    bitmap = bytearray(KEY_BYTES)
    non_default_siblings = []

    current_slice = sorted_keys
    for d in range(KEY_BITS):
        bit = key_bit_at(query_key, d)
        split = 0
        while split < len(current_slice) and key_bit_at(current_slice[split], d) == 0:
            split += 1
        left = current_slice[:split]
        right = current_slice[split:]

        if bit == 0:
            path_slice, sibling_slice = left, right
        else:
            path_slice, sibling_slice = right, left

        current_slice = path_slice

        if sibling_slice:
            bitmap[d // 8] |= 1 << (7 - (d % 8))
            sibling_hash = smt_build_subtree(sibling_slice, d + 1)
            non_default_siblings.append(sibling_hash)

    return bytes(bitmap), non_default_siblings, is_present


def smt_encode_proof(bitmap: bytes, siblings: list[bytes], is_present: bool) -> bytes:
    out = bytearray(bitmap)
    out.append(1 if is_present else 0)
    out.extend(struct.pack(">H", len(siblings)))
    for sib in siblings:
        out.extend(sib)
    return bytes(out)


def smt_verify(
    expected_root: bytes,
    query_key: bytes,
    bitmap: bytes,
    siblings: list[bytes],
    is_present: bool,
) -> bool:
    popcount = sum(bin(b).count("1") for b in bitmap)
    if popcount != len(siblings):
        return False

    current = (
        node_hash(SMT_PRESENT_LEAF_TYPE, KEY_BITS, PRESENT_VALUE_ON)
        if is_present
        else SMT_DEFAULTS[KEY_BITS]
    )

    sib_idx = len(siblings)
    d = KEY_BITS
    while d > 0:
        d -= 1
        bit = (bitmap[d // 8] >> (7 - (d % 8))) & 1
        if bit == 1:
            if sib_idx == 0:
                return False
            sib_idx -= 1
            sibling = siblings[sib_idx]
            # Reject non-canonical explicit default sibling
            if sibling == SMT_DEFAULTS[d + 1]:
                return False
        else:
            sibling = SMT_DEFAULTS[d + 1]

        key_bit = key_bit_at(query_key, d)
        if key_bit == 0:
            payload = current + sibling
        else:
            payload = sibling + current
        current = node_hash(SMT_BRANCH_TYPE, d, payload)

    return sib_idx == 0 and current == expected_root


# =========================================================================
# MERKLE PATRICIA TRIE
# =========================================================================

def common_prefix_len(k1: bytes, k2: bytes, start_depth: int) -> int:
    matched = 0
    for d in range(start_depth, KEY_BITS):
        if key_bit_at(k1, d) == key_bit_at(k2, d):
            matched += 1
        else:
            break
    return matched


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


def mpt_leaf_hash(depth: int, key_bytes: bytes) -> bytes:
    suffix_len = KEY_BITS - depth
    bits = [key_bit_at(key_bytes, d) for d in range(depth, KEY_BITS)]
    payload = struct.pack(">H", suffix_len) + mpt_pack_bits(bits) + PRESENT_VALUE_ON
    return node_hash(MPT_LEAF_TYPE, depth, payload)


def mpt_extension_hash(depth: int, shared_bits: list[int], child_hash: bytes) -> bytes:
    payload = struct.pack(">H", len(shared_bits)) + mpt_pack_bits(shared_bits) + child_hash
    return node_hash(MPT_EXTENSION_TYPE, depth, payload)


def mpt_branch_hash(depth: int, left_hash: bytes, right_hash: bytes) -> bytes:
    return node_hash(MPT_BRANCH_TYPE, depth, left_hash + right_hash)


def mpt_build_root(sorted_keys: list[bytes], depth: int = 0) -> bytes:
    if not sorted_keys:
        return node_hash(MPT_EMPTY_TYPE, 0, b"")
    if len(sorted_keys) == 1:
        return mpt_leaf_hash(depth, sorted_keys[0])

    common = common_prefix_len(sorted_keys[0], sorted_keys[-1], depth)
    if common > 0:
        shared_bits = [key_bit_at(sorted_keys[0], depth + i) for i in range(common)]
        child_hash = mpt_build_root(sorted_keys, depth + common)
        return mpt_extension_hash(depth, shared_bits, child_hash)

    split = 0
    while split < len(sorted_keys) and key_bit_at(sorted_keys[split], depth) == 0:
        split += 1
    left_hash = mpt_build_root(sorted_keys[:split], depth + 1)
    right_hash = mpt_build_root(sorted_keys[split:], depth + 1)
    return mpt_branch_hash(depth, left_hash, right_hash)


def mpt_prove_and_encode(sorted_keys: list[bytes], query_key: bytes) -> tuple[list[tuple], tuple, bytes]:
    steps = []
    current_keys = sorted_keys
    depth = 0
    while True:
        if not current_keys:
            terminal = ("empty",)
            break
        if len(current_keys) == 1:
            terminal = ("leaf", depth, current_keys[0])
            break
        common = common_prefix_len(current_keys[0], current_keys[-1], depth)
        if common > 0:
            shared_bits = [key_bit_at(current_keys[0], depth + i) for i in range(common)]
            matches = all(key_bit_at(query_key, depth + i) == shared_bits[i] for i in range(common))
            if not matches:
                child_hash = mpt_build_root(current_keys, depth + common)
                terminal = ("divergent_ext", depth, shared_bits, child_hash)
                break
            steps.append(("ext", depth, shared_bits))
            depth += common
        else:
            split = 0
            while split < len(current_keys) and key_bit_at(current_keys[split], depth) == 0:
                split += 1
            left_keys = current_keys[:split]
            right_keys = current_keys[split:]
            direction = key_bit_at(query_key, depth)
            if direction == 0:
                sib_hash = mpt_build_root(right_keys, depth + 1)
                steps.append(("branch", depth, 0, sib_hash))
                current_keys = left_keys
            else:
                sib_hash = mpt_build_root(left_keys, depth + 1)
                steps.append(("branch", depth, 1, sib_hash))
                current_keys = right_keys
            depth += 1

    # Encode canonical wire bytes matching PatriciaPointProof::to_canonical_bytes
    out = bytearray(struct.pack(">H", len(steps)))
    for s in steps:
        if s[0] == "branch":
            _, d, dir_bit, sib = s
            out.append(STEP_TAG_BRANCH)
            out.append(d)
            out.append(dir_bit)
            out.extend(sib)
        else:
            _, d, sbits = s
            out.append(STEP_TAG_EXTENSION)
            out.append(d)
            out.extend(struct.pack(">H", len(sbits)))
            out.extend(mpt_pack_bits(sbits))

    if terminal[0] == "empty":
        out.append(TERM_TAG_EMPTY)
    elif terminal[0] == "leaf":
        _, d, lk = terminal
        out.append(TERM_TAG_LEAF)
        out.append(d)
        out.extend(lk)
    else:
        _, d, sbits, chash = terminal
        out.append(TERM_TAG_DIVERGENT_EXT)
        out.append(d)
        out.extend(struct.pack(">H", len(sbits)))
        out.extend(mpt_pack_bits(sbits))
        out.extend(chash)

    return steps, terminal, bytes(out)


def mpt_verify(expected_root: bytes, query_key: bytes, steps: list[tuple], terminal: tuple) -> bool | None:
    empty_hash = node_hash(MPT_EMPTY_TYPE, 0, b"")
    if terminal[0] == "empty":
        if steps:
            return None
        current_hash = empty_hash
        is_present = False
        current_depth = 0
        prev_was_ext = False
    elif terminal[0] == "leaf":
        _, d, lk = terminal
        if d > 0 and (not steps or steps[-1][0] != "branch"):
            return None
        if common_prefix_len(lk, query_key, 0) < d:
            return None
        current_hash = mpt_leaf_hash(d, lk)
        is_present = lk == query_key
        current_depth = d
        prev_was_ext = False
    elif terminal[0] == "divergent_ext":
        _, d, sbits, chash = terminal
        if not sbits or d + len(sbits) >= KEY_BITS or chash == empty_hash:
            return None
        if all(key_bit_at(query_key, d + i) == sbits[i] for i in range(len(sbits))):
            return None
        current_hash = mpt_extension_hash(d, sbits, chash)
        is_present = False
        current_depth = d
        prev_was_ext = True
    else:
        return None

    for s in reversed(steps):
        if s[0] == "ext":
            _, d, sbits = s
            if prev_was_ext or not sbits or d + len(sbits) != current_depth:
                return None
            if any(key_bit_at(query_key, d + i) != sbits[i] for i in range(len(sbits))):
                return None
            current_hash = mpt_extension_hash(d, sbits, current_hash)
            current_depth = d
            prev_was_ext = True
        elif s[0] == "branch":
            _, d, dir_bit, sib = s
            if d + 1 != current_depth or key_bit_at(query_key, d) != dir_bit or sib == empty_hash:
                return None
            if dir_bit == 0:
                current_hash = mpt_branch_hash(d, current_hash, sib)
            else:
                current_hash = mpt_branch_hash(d, sib, current_hash)
            current_depth = d
            prev_was_ext = False

    if current_depth != 0 or current_hash != expected_root:
        return None
    return is_present


def run_all():
    # 1. Check empty roots
    smt_empty = SMT_DEFAULTS[0].hex()
    assert smt_empty == EXPECTED_EMPTY_SMT_HEX, f"SMT empty root mismatch: {smt_empty}"

    mpt_empty = mpt_build_root([]).hex()
    assert (
        mpt_empty == EXPECTED_EMPTY_PATRICIA_HEX
    ), f"MPT empty root mismatch: {mpt_empty}"

    # 2. Check bedrock (0, 0, 0) roots
    bedrock_key = coord_to_key_bytes(0, 0, 0)
    smt_bedrock = smt_build_subtree([bedrock_key], 0)
    smt_bedrock_tag = content_tag_render("world", COMMIT_R1_WORLD_SMT_SCHEMA, smt_bedrock)
    assert (
        smt_bedrock_tag == EXPECTED_BEDROCK_SMT_TAG
    ), f"SMT bedrock tag mismatch: {smt_bedrock_tag}"

    mpt_bedrock = mpt_build_root([bedrock_key], 0)
    mpt_bedrock_tag = content_tag_render("world", COMMIT_R1_WORLD_PATRICIA_SCHEMA, mpt_bedrock)
    assert (
        mpt_bedrock_tag == EXPECTED_BEDROCK_PATRICIA_TAG
    ), f"MPT bedrock tag mismatch: {mpt_bedrock_tag}"

    # 3. Check 3-key world proof tags matching Rust literal assertions
    three_keys = sorted([
        bedrock_key,
        coord_to_key_bytes(1, 2, 3),
        coord_to_key_bytes(4, 5, 6),
    ])
    q_123 = coord_to_key_bytes(1, 2, 3)
    bm_3, sib_3, isp_3 = smt_prove(three_keys, q_123)
    smt_3_tag = content_tag_render("receipt", COMMIT_R1_SMT_PROOF_SCHEMA, smt_encode_proof(bm_3, sib_3, isp_3))
    assert smt_3_tag == EXPECTED_3KEY_SMT_PROOF_TAG, f"SMT 3-key proof tag mismatch: {smt_3_tag}"

    _, _, mpt_3_bytes = mpt_prove_and_encode(three_keys, q_123)
    mpt_3_tag = content_tag_render("receipt", COMMIT_R1_PATRICIA_PROOF_SCHEMA, mpt_3_bytes)
    assert mpt_3_tag == EXPECTED_3KEY_PATRICIA_PROOF_TAG, f"MPT 3-key proof tag mismatch: {mpt_3_tag}"

    # 4. Check SMT and MPT proofs, canonical proof tags, and soundness negative controls
    other_key = coord_to_key_bytes(1, 2, 3)
    keys = sorted([
        bedrock_key,
        other_key,
        coord_to_key_bytes(-10, 20, -30),
        coord_to_key_bytes(100, 0, 0),
    ])
    smt_root = smt_build_subtree(keys, 0)
    mpt_root = mpt_build_root(keys, 0)

    # SMT present + proof tag
    bitmap, siblings, is_present = smt_prove(keys, bedrock_key)
    assert is_present
    assert smt_verify(smt_root, bedrock_key, bitmap, siblings, True)
    smt_proof_bytes = smt_encode_proof(bitmap, siblings, is_present)
    smt_proof_tag = content_tag_render("receipt", COMMIT_R1_SMT_PROOF_SCHEMA, smt_proof_bytes)
    assert smt_proof_tag.startswith("#bb.receipt.sha256.")

    # MPT present + proof tag + cross-key spoofing rejection
    mpt_steps, mpt_term, mpt_proof_bytes = mpt_prove_and_encode(keys, bedrock_key)
    assert mpt_verify(mpt_root, bedrock_key, mpt_steps, mpt_term) is True
    assert mpt_verify(mpt_root, other_key, mpt_steps, mpt_term) is None
    mpt_proof_tag = content_tag_render("receipt", COMMIT_R1_PATRICIA_PROOF_SCHEMA, mpt_proof_bytes)
    assert mpt_proof_tag.startswith("#bb.receipt.sha256.")

    # Prove absent on both
    absent_key = coord_to_key_bytes(999, 999, 999)
    bitmap_a, siblings_a, is_present_a = smt_prove(keys, absent_key)
    assert not is_present_a
    assert smt_verify(smt_root, absent_key, bitmap_a, siblings_a, False)

    mpt_steps_a, mpt_term_a, _ = mpt_prove_and_encode(keys, absent_key)
    assert mpt_verify(mpt_root, absent_key, mpt_steps_a, mpt_term_a) is False

    # Negative control: flipped presence in SMT
    assert not smt_verify(smt_root, absent_key, bitmap_a, siblings_a, True)

    # Negative control: corrupted sibling in SMT
    if siblings:
        bad_siblings = list(siblings)
        bad_siblings[0] = bytes([bad_siblings[0][0] ^ 1]) + bad_siblings[0][1:]
        assert not smt_verify(smt_root, bedrock_key, bitmap, bad_siblings, True)

    # Assay report schema tag check
    assay_payload = mpt_root + smt_root + struct.pack(">13I", 4, len(mpt_proof_bytes), len(smt_proof_bytes), 50, 123, 7, 2, 3, 195, 4, 193, 120, 220)
    assay_tag = content_tag_render("receipt", COMMIT_R1_ASSAY_REPORT_SCHEMA, assay_payload)
    assert assay_tag.startswith("#bb.receipt.sha256.")

    # 5. MortonKeyV1 bijection, octant trios, 180-bit chunk prefixes, and canonical tags
    i64_min = -(1 << 63)
    i64_max = (1 << 63) - 1
    assert morton_key_from_coord(i64_min, i64_min, i64_min) == b"\x00" * KEY_BYTES
    assert morton_key_from_coord(i64_max, i64_max, i64_max) == b"\xff" * KEY_BYTES

    morton_probe_coords = [
        (0, 0, 0),
        (-1, -1, -1),
        (-16, -16, -16),
        (-17, 0, 15),
        (i64_min, 0, i64_max),
    ]
    for cx, cy, cz in morton_probe_coords:
        mk = morton_key_from_coord(cx, cy, cz)
        assert morton_key_to_coord(mk) == (cx, cy, cz)
        assert morton_octant_at(mk, 64) == 0
        assert morton_octant_at(mk, 255) == 0

    zero_morton = morton_key_from_coord(0, 0, 0)
    assert morton_octant_at(zero_morton, 0) == 7
    assert all(morton_octant_at(zero_morton, lvl) == 0 for lvl in range(1, 64))
    zero_morton_tag = content_tag_render("world", MORTON_KEY_V1_SCHEMA, zero_morton)
    assert (
        zero_morton_tag == EXPECTED_ZERO_MORTON_KEY_TAG
    ), f"MortonKeyV1 zero tag mismatch: {zero_morton_tag}"

    nonzero_morton = morton_key_from_coord(-17, 0, 15)
    assert morton_octant_at(nonzero_morton, 0) == 0b011
    assert morton_octant_at(nonzero_morton, 63) == 0b101
    nonzero_morton_tag = content_tag_render("world", MORTON_KEY_V1_SCHEMA, nonzero_morton)
    assert (
        nonzero_morton_tag == EXPECTED_NONZERO_MORTON_KEY_TAG
    ), f"MortonKeyV1 nonzero tag mismatch: {nonzero_morton_tag}"

    # Verify all 4,096 coordinates in chunk (-1, 0, 2) share the exact same 180-bit chunk prefix
    expected_prefix = morton_chunk_prefix_180b(morton_key_from_coord(-16, 0, 32))
    assert (expected_prefix[22] & 0x0F) == 0
    chunk_morton_set = set()
    for dx in range(16):
        for dy in range(16):
            for dz in range(16):
                mk = morton_key_from_coord(-16 + dx, dy, 32 + dz)
                assert morton_chunk_prefix_180b(mk) == expected_prefix
                chunk_morton_set.add(mk)
    assert len(chunk_morton_set) == 4096
    for ncx, ncy, ncz in [
        (-17, 0, 32),
        (0, 0, 32),
        (-16, -1, 32),
        (-16, 16, 32),
        (-16, 0, 31),
        (-16, 0, 48),
    ]:
        assert morton_chunk_prefix_180b(morton_key_from_coord(ncx, ncy, ncz)) != expected_prefix

    # 6. MortonGapStreamV1 wire codec, canonical tags, and soundness negative controls
    empty_stream_bytes = morton_gap_stream_encode([])
    assert empty_stream_bytes == b"\x00\x00\x00\x00"
    assert morton_gap_stream_decode(empty_stream_bytes) == []
    empty_stream_tag = content_tag_render(
        "receipt", MORTON_GAP_STREAM_V1_SCHEMA, empty_stream_bytes
    )
    assert (
        empty_stream_tag == EXPECTED_EMPTY_MORTON_GAP_STREAM_TAG
    ), f"MortonGapStreamV1 empty tag mismatch: {empty_stream_tag}"

    stream_coords = [
        (0, 0, 0),
        (-1, -1, -1),
        (-16, -16, -16),
        (-17, 0, 15),
        (1, 2, 3),
    ]
    stream_bytes = morton_gap_stream_encode(stream_coords)
    assert len(stream_bytes) == 83
    decoded_keys = morton_gap_stream_decode(stream_bytes)
    assert decoded_keys is not None
    assert [morton_key_to_coord(k) for k in decoded_keys] == [
        (-16, -16, -16),
        (-1, -1, -1),
        (-17, 0, 15),
        (0, 0, 0),
        (1, 2, 3),
    ]
    stream_tag = content_tag_render("receipt", MORTON_GAP_STREAM_V1_SCHEMA, stream_bytes)
    assert (
        stream_tag == EXPECTED_5KEY_MORTON_GAP_STREAM_TAG
    ), f"MortonGapStreamV1 5-key tag mismatch: {stream_tag}"

    # Negative controls for MortonGapStreamV1
    assert morton_gap_stream_decode(b"\x00\x00\x00") is None
    assert morton_gap_stream_decode(b"\x00\x00\x00\x00\xff") is None
    assert morton_gap_stream_decode(stream_bytes[:-1]) is None
    assert morton_gap_stream_decode(stream_bytes + b"\x00") is None
    assert (
        morton_gap_stream_decode(struct.pack(">I", 2) + decoded_keys[0] + bytes([24]))
        is None
    )
    assert (
        morton_gap_stream_decode(
            struct.pack(">I", 2) + decoded_keys[0] + bytes([0]) + decoded_keys[1]
        )
        is None
    )

    print("commit_r1_vectors: ok (empty, bedrock, SMT, MPT, MortonKeyV1, MortonGapStreamV1, point proofs, wire tags, and soundness controls pass)")


if __name__ == "__main__":
    run_all()
    sys.exit(0)
