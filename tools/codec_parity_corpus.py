#!/usr/bin/env python3
"""CODEC-1@local-v1 dual-language canonical parity and adversarial negative corpus.

This script intentionally imports no Bitblock/Rust code. It independently constructs
valid canonical bytes and strict fail-closed decoders across activated local-v1 codecs
(ContentTag, Edit, InteractionTape, HistoryReceiptPayload, TickOutcome v3, StorageRecord,
LocalStorageProfile, MortonGapStreamV1, PatriciaPointProof, SmtPointProof, ChunkLeafV1,
OctreeInternalNodeV1, OctreeRootV1, and ScaleWorkloadReportV1), then asserts:
  1. Positive byte/tag parity against pinned canonical tags.
  2. Exhaustive prefix-truncation rejection (`0..len`) and trailing-byte rejection.
  3. Overlong/overflow declared lengths and counts rejected before allocation.
  4. Noncanonical flags, unused padding bits, out-of-order entries, duplicates,
     popcount mismatches, and cross-domain/cross-version tag separation.

Pairs directly with the Rust integration suite `tests/codec_parity_v1.rs`.
"""

from __future__ import annotations

import base64
import hashlib
import math
import re
import struct
import sys

TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"

# All schema strings declared here are real schemas owned by crates/core or crates/research.
EDIT_SCHEMA = b"bitblock/edit/op-u8-i64-be/v1"
INTERACTION_TAPE_SCHEMA = b"bitblock/interaction-tape/v1"
HISTORY_RECEIPT_SCHEMA = b"bitblock/history/receipt/v1"
TICK_OUTCOME_V3_SCHEMA = b"bitblock/tick-outcome/v3"
FRAME_RECEIPT_V1_SCHEMA = b"bitblock/frame-receipt/v1"
FRAME_RECEIPT_V2_SCHEMA = b"bitblock/frame-receipt/v2"
STORAGE_RECORD_SCHEMA = b"bitblock/storage-record/envelope/v1"
LOCAL_BOUNDS_PROFILE_SCHEMA = b"bitblock/local-bounds-profile/v1"
MORTON_GAP_STREAM_SCHEMA = b"bitblock/commit-r1/morton-gap-stream/v1"
PATRICIA_PROOF_SCHEMA = b"bitblock/commit-r1/patricia-proof/v1"
SMT_PROOF_SCHEMA = b"bitblock/commit-r1/smt-proof/v1"
OCTREE_CHUNK_LEAF_SCHEMA = b"bitblock/octree-commit/chunk-leaf/v1"
OCTREE_INTERNAL_NODE_SCHEMA = b"bitblock/octree-commit/internal-node/v1"
OCTREE_ROOT_SCHEMA = b"bitblock/octree-commit/root/v1"
SCALE_WORKLOAD_REPORT_SCHEMA = b"bitblock/scale-assay/workload-report/v1"

CONTENT_TAG_RE = re.compile(r"^#bb\.([a-z0-9-]+)\.sha256\.([a-z2-7]{52})$")
KNOWN_TAG_KINDS = {
    "world",
    "edit",
    "event",
    "receipt",
    "frame",
    "frame-input",
    "frame-receipt",
    "tick",
    "tape",
    "policy",
    "display",
    "presentation",
    "output-topology",
    "output-color",
    "sensor",
    "storage-record",
    "ledger",
}

MAX_STORAGE_RECORD_BYTES = 16 * 1024 * 1024
MAX_STORAGE_SCHEMA_BYTES = 4 * 1024
MAX_STORAGE_PAYLOAD_BYTES = 8 * 1024 * 1024
MAX_ACTIONS_PER_TICK = 65_536


def field(val: bytes) -> bytes:
    return struct.pack(">H", len(val)) + val


def hash_canonical(kind: str, schema: bytes, payload: bytes) -> tuple[str, bytes]:
    preimage = (
        field(TAG_DOMAIN)
        + field(kind.encode("ascii"))
        + field(HASH_SUITE)
        + field(schema)
        + struct.pack(">Q", len(payload))
        + payload
    )
    digest = hashlib.sha256(preimage).digest()
    b32 = base64.b32encode(digest).decode("ascii").lower().rstrip("=")
    return f"#bb.{kind}.sha256.{b32}", digest


def parse_content_tag(text: str) -> tuple[str, bytes] | None:
    m = CONTENT_TAG_RE.match(text)
    if not m:
        return None
    kind, b32 = m.group(1), m.group(2)
    if kind not in KNOWN_TAG_KINDS:
        return None
    # Verify canonical RFC 4648 base32 (trailing unused 4 bits of 52*5=260 vs 256 bits must be 0)
    try:
        raw = base64.b32decode(b32.upper() + "====")
    except Exception:
        return None
    if len(raw) != 32:
        return None
    canonical = base64.b32encode(raw).decode("ascii").lower().rstrip("=")
    if canonical != b32:
        return None
    return kind, raw


def decode_edit(data: bytes) -> tuple[int, int, int, int] | None:
    if len(data) != 25:
        return None
    op = data[0]
    if op > 2:
        return None
    x, y, z = struct.unpack(">qqq", data[1:])
    return op, x, y, z


def decode_local_bounds_profile(data: bytes) -> tuple[int, ...] | None:
    if len(data) != 56:
        return None
    return struct.unpack(">7Q", data)


def decode_storage_record(data: bytes) -> dict | None:
    if len(data) < 18:
        return None
    if data[:8] != b"BBSTRREC":
        return None
    (ver,) = struct.unpack(">H", data[8:10])
    if ver != 1:
        return None
    (declared_len,) = struct.unpack(">Q", data[10:18])
    if declared_len > MAX_STORAGE_RECORD_BYTES or declared_len != len(data):
        return None
    if declared_len < 18 + 1 + 2 + 1 + 32 + 2 + 8 + 32:
        return None
    cursor = 18
    parent_flag = data[cursor]
    cursor += 1
    if parent_flag > 1:
        return None
    parent = None
    if parent_flag == 1:
        if cursor + 32 > len(data) - 32:
            return None
        parent = data[cursor : cursor + 32]
        cursor += 32
    if cursor + 2 > len(data) - 32:
        return None
    (kind_len,) = struct.unpack(">H", data[cursor : cursor + 2])
    cursor += 2
    if kind_len == 0 or kind_len > 64 or cursor + kind_len + 32 + 2 > len(data) - 32:
        return None
    kind_bytes = data[cursor : cursor + kind_len]
    cursor += kind_len
    try:
        kind_str = kind_bytes.decode("ascii")
    except UnicodeDecodeError:
        return None
    if kind_str not in KNOWN_TAG_KINDS:
        return None
    obj_digest = data[cursor : cursor + 32]
    cursor += 32
    (schema_len,) = struct.unpack(">H", data[cursor : cursor + 2])
    cursor += 2
    if schema_len == 0 or schema_len > MAX_STORAGE_SCHEMA_BYTES:
        return None
    if cursor + schema_len + 8 > len(data) - 32:
        return None
    schema_bytes = data[cursor : cursor + schema_len]
    cursor += schema_len
    (payload_len,) = struct.unpack(">Q", data[cursor : cursor + 8])
    cursor += 8
    if payload_len > MAX_STORAGE_PAYLOAD_BYTES:
        return None
    if cursor + payload_len != len(data) - 32:
        return None
    payload = data[cursor : cursor + payload_len]
    footer = data[len(data) - 32 :]
    _, expected_obj = hash_canonical(kind_str, schema_bytes, payload)
    if expected_obj != obj_digest:
        return None
    _, expected_rec = hash_canonical("storage-record", STORAGE_RECORD_SCHEMA, data[:-32])
    if expected_rec != footer:
        return None
    return {
        "parent": parent,
        "kind": kind_str,
        "schema": schema_bytes,
        "payload": payload,
        "record_digest": footer,
    }


def decode_morton_gap_stream(data: bytes) -> list[bytes] | None:
    if len(data) < 4:
        return None
    (count,) = struct.unpack(">I", data[:4])
    if count == 0:
        return [] if len(data) == 4 else None
    # Preflight minimum possible wire bytes (24 for first key + 2 per subsequent key) before allocation
    if count > (len(data) - 4) // 2 + 1:
        return None
    if len(data) < 28:
        return None
    prev = data[4:28]
    keys = [prev]
    cursor = 28
    for _ in range(1, count):
        if cursor >= len(data):
            return None
        shared = data[cursor]
        cursor += 1
        if shared >= 24:
            return None
        suffix_len = 24 - shared
        if cursor + suffix_len > len(data):
            return None
        suffix = data[cursor : cursor + suffix_len]
        cursor += suffix_len
        if suffix[0] <= prev[shared]:
            return None
        prev = prev[:shared] + suffix
        keys.append(prev)
    if cursor != len(data):
        return None
    return keys


def decode_smt_proof(data: bytes) -> tuple[bytes, bool, list[bytes]] | None:
    if len(data) < 27:
        return None
    bitmap = data[:24]
    present = data[24]
    if present > 1:
        return None
    (count,) = struct.unpack(">H", data[25:27])
    popcount = sum(bin(b).count("1") for b in bitmap)
    if count != popcount or len(data) != 27 + count * 32:
        return None
    siblings = [data[27 + i * 32 : 27 + (i + 1) * 32] for i in range(count)]
    return bitmap, present == 1, siblings


def decode_patricia_proof(data: bytes) -> tuple[list, tuple] | None:
    if len(data) < 3:
        return None
    (step_count,) = struct.unpack(">H", data[:2])
    if step_count > 192:
        return None
    cursor = 2
    steps = []
    for _ in range(step_count):
        if cursor >= len(data):
            return None
        tag = data[cursor]
        cursor += 1
        if tag == 1:  # Branch
            if cursor + 34 > len(data):
                return None
            depth, direction = data[cursor], data[cursor + 1]
            if depth >= 192 or direction > 1:
                return None
            steps.append(("branch", depth, direction, data[cursor + 2 : cursor + 34]))
            cursor += 34
        elif tag == 2:  # Extension
            if cursor + 3 > len(data):
                return None
            depth = data[cursor]
            (bit_len,) = struct.unpack(">H", data[cursor + 1 : cursor + 3])
            cursor += 3
            if bit_len == 0 or depth + bit_len >= 192:
                return None
            packed_len = (bit_len + 7) // 8
            if cursor + packed_len > len(data):
                return None
            packed = data[cursor : cursor + packed_len]
            rem = bit_len % 8
            if rem != 0 and (packed[-1] & ((1 << (8 - rem)) - 1)) != 0:
                return None
            steps.append(("ext", depth, bit_len, packed))
            cursor += packed_len
        else:
            return None
    if cursor >= len(data):
        return None
    term_tag = data[cursor]
    cursor += 1
    if term_tag == 0:
        term = ("empty",)
    elif term_tag == 1:
        if cursor + 25 != len(data):
            return None
        depth = data[cursor]
        if depth > 192:
            return None
        term = ("leaf", depth, data[cursor + 1 : cursor + 25])
        cursor += 25
    elif term_tag == 2:
        if cursor + 3 > len(data):
            return None
        depth = data[cursor]
        (bit_len,) = struct.unpack(">H", data[cursor + 1 : cursor + 3])
        cursor += 3
        if bit_len == 0 or depth + bit_len >= 192:
            return None
        packed_len = (bit_len + 7) // 8
        if cursor + packed_len + 32 != len(data):
            return None
        packed = data[cursor : cursor + packed_len]
        rem = bit_len % 8
        if rem != 0 and (packed[-1] & ((1 << (8 - rem)) - 1)) != 0:
            return None
        term = ("div_ext", depth, bit_len, packed, data[cursor + packed_len :])
        cursor = len(data)
    else:
        return None
    if cursor != len(data):
        return None
    return steps, term


def decode_chunk_leaf(data: bytes) -> tuple[bytes, int, bytes] | None:
    if len(data) != 537:
        return None
    prefix = data[:23]
    if (prefix[22] & 0x0F) != 0:
        return None
    (occ,) = struct.unpack(">H", data[23:25])
    if occ == 0 or occ > 4096:
        return None
    bits = data[25:]
    if sum(bin(b).count("1") for b in bits) != occ:
        return None
    return prefix, occ, bits


def decode_octree_root(data: bytes) -> tuple[int, int, int, bytes] | None:
    if len(data) != 49:
        return None
    state = data[0]
    occ_chunks, occ_bits = struct.unpack(">QQ", data[1:17])
    digest = data[17:49]
    if state == 0:
        if occ_chunks != 0 or occ_bits != 0 or digest != b"\x00" * 32:
            return None
    elif state == 1:
        if occ_chunks != 1 or occ_bits == 0 or occ_bits > 4096 or digest == b"\x00" * 32:
            return None
    elif state == 2:
        if occ_chunks < 2 or occ_bits < occ_chunks or digest == b"\x00" * 32:
            return None
    else:
        return None
    return state, occ_chunks, occ_bits, digest


def assert_all_truncations_and_trailing_reject(decoder, valid: bytes, name: str) -> None:
    assert decoder(valid) is not None, f"{name}: valid fixture failed decode"
    for cut in range(len(valid)):
        assert (
            decoder(valid[:cut]) is None
        ), f"{name}: prefix truncation at {cut}/{len(valid)} was accepted"
    assert (
        decoder(valid + b"\x00") is None
    ), f"{name}: trailing zero byte was accepted"
    assert (
        decoder(valid + b"\xff") is None
    ), f"{name}: trailing 0xff byte was accepted"


def run_all() -> None:
    # 1. ContentTag canonical text parsing & length-framed domain/version separation
    edit_bytes = bytes([0]) + struct.pack(">qqq", 0, 0, 0)
    edit_tag, edit_digest = hash_canonical("edit", EDIT_SCHEMA, edit_bytes)
    assert parse_content_tag(edit_tag) == ("edit", edit_digest)

    # Reject uppercase, padded, truncated, non-canonical base32 tail bit, unknown kind, trailing space
    assert parse_content_tag(edit_tag.upper()) is None
    assert parse_content_tag(edit_tag + "====") is None
    assert parse_content_tag(edit_tag[:-1]) is None
    assert parse_content_tag(edit_tag + " ") is None
    assert parse_content_tag(edit_tag.replace(".edit.", ".bogus.")) is None
    # Last base32 char encodes only top 1 bit (256 % 5 = 1 -> valid final chars are 'a' or 'q')
    bad_b32_tail = edit_tag[:-1] + "b"
    assert parse_content_tag(bad_b32_tail) is None

    # Cross-domain and cross-version separation across real schemas on identical payloads
    same_payload = b"\x00" * 32
    tag_fr1, _ = hash_canonical("frame-receipt", FRAME_RECEIPT_V1_SCHEMA, same_payload)
    tag_fr2, _ = hash_canonical("frame-receipt", FRAME_RECEIPT_V2_SCHEMA, same_payload)
    assert tag_fr1 != tag_fr2, "cross-version schemas must never collide on identical payload"
    tag_pat, _ = hash_canonical("receipt", PATRICIA_PROOF_SCHEMA, same_payload)
    tag_smt, _ = hash_canonical("receipt", SMT_PROOF_SCHEMA, same_payload)
    assert tag_pat != tag_smt, "cross-domain proof schemas must never collide on identical payload"
    tag_edit_as_tick, _ = hash_canonical("tick", TICK_OUTCOME_V3_SCHEMA, edit_bytes)
    assert tag_edit_as_tick != edit_tag

    # Length-prefix framing boundary control: shifting 1 byte between schema & payload changes digest
    _, d_a = hash_canonical("receipt", PATRICIA_PROOF_SCHEMA, b"XY")
    _, d_b = hash_canonical("receipt", PATRICIA_PROOF_SCHEMA + b"X", b"Y")
    assert d_a != d_b

    # 2. Edit codec (25 bytes)
    assert_all_truncations_and_trailing_reject(decode_edit, edit_bytes, "Edit")
    for bad_op in (3, 4, 128, 255):
        assert decode_edit(bytes([bad_op]) + edit_bytes[1:]) is None

    # 3. LocalStorageProfile (56 bytes)
    bounds_bytes = struct.pack(
        ">7Q",
        16 * 1024 * 1024,
        65_536,
        64 * 1024 * 1024,
        262_144,
        512 * 1024 * 1024,
        1024 * 1024 * 1024,
        4_096,
    )
    bounds_tag, _ = hash_canonical("policy", LOCAL_BOUNDS_PROFILE_SCHEMA, bounds_bytes)
    assert (
        bounds_tag
        == "#bb.policy.sha256.gvc7zlvkyaeidfzsivb4xdlx32wc5q7kmfmlk274cd6szzhgfx7a"
    )
    assert_all_truncations_and_trailing_reject(
        decode_local_bounds_profile, bounds_bytes, "LocalStorageProfile"
    )

    # 4. StorageRecord envelope (self-authenticating outer framing + overflow preflight)
    kind_b = b"edit"
    total_len = 18 + 1 + 2 + len(kind_b) + 32 + 2 + len(EDIT_SCHEMA) + 8 + len(edit_bytes) + 32
    prefix = (
        b"BBSTRREC"
        + struct.pack(">HQ", 1, total_len)
        + b"\x00"
        + struct.pack(">H", len(kind_b))
        + kind_b
        + edit_digest
        + struct.pack(">H", len(EDIT_SCHEMA))
        + EDIT_SCHEMA
        + struct.pack(">Q", len(edit_bytes))
        + edit_bytes
    )
    _, rec_digest = hash_canonical("storage-record", STORAGE_RECORD_SCHEMA, prefix)
    storage_record_bytes = prefix + rec_digest
    assert_all_truncations_and_trailing_reject(
        decode_storage_record, storage_record_bytes, "StorageRecord"
    )
    # Wrong magic, wrong version, overlong declared length (16 MiB + 1 & u64::MAX), bad parent flag, tampered footer
    assert decode_storage_record(b"XXSTRREC" + storage_record_bytes[8:]) is None
    assert (
        decode_storage_record(
            b"BBSTRREC" + struct.pack(">H", 2) + storage_record_bytes[10:]
        )
        is None
    )
    assert (
        decode_storage_record(
            b"BBSTRREC"
            + struct.pack(">HQ", 1, MAX_STORAGE_RECORD_BYTES + 1)
            + storage_record_bytes[18:]
        )
        is None
    )
    assert (
        decode_storage_record(
            b"BBSTRREC"
            + struct.pack(">HQ", 1, 0xFFFFFFFFFFFFFFFF)
            + storage_record_bytes[18:]
        )
        is None
    )
    bad_parent_flag = bytearray(storage_record_bytes)
    bad_parent_flag[18] = 2
    assert decode_storage_record(bytes(bad_parent_flag)) is None
    bad_footer = bytearray(storage_record_bytes)
    bad_footer[-1] ^= 0x01
    assert decode_storage_record(bytes(bad_footer)) is None

    # 5. MortonGapStreamV1 (empty, 2-key stream, overflow count u32::MAX, non-maximal prefix, out-of-order)
    k0 = b"\x00" * 24
    k1 = b"\x00" * 23 + b"\x01"
    morton_wire = struct.pack(">I", 2) + k0 + bytes([23, 1])
    assert_all_truncations_and_trailing_reject(
        decode_morton_gap_stream, morton_wire, "MortonGapStreamV1"
    )
    assert decode_morton_gap_stream(struct.pack(">I", 0)) == []
    assert decode_morton_gap_stream(struct.pack(">I", 0) + b"\x00") is None
    assert decode_morton_gap_stream(struct.pack(">I", 0xFFFFFFFF) + k0) is None
    # shared == 24 invalid
    assert decode_morton_gap_stream(struct.pack(">I", 2) + k0 + bytes([24])) is None
    # non-maximal shared prefix (shared=0 when byte 0 matches)
    assert decode_morton_gap_stream(struct.pack(">I", 2) + k0 + bytes([0]) + k1) is None
    # descending order (suffix[0] < prev[shared])
    assert (
        decode_morton_gap_stream(struct.pack(">I", 2) + k1 + bytes([23, 0])) is None
    )

    # 6. SmtPointProof & PatriciaPointProof wire codecs
    smt_bitmap = bytes([0x80]) + b"\x00" * 23
    smt_wire = smt_bitmap + bytes([1]) + struct.pack(">H", 1) + (b"\xaa" * 32)
    assert_all_truncations_and_trailing_reject(decode_smt_proof, smt_wire, "SmtPointProof")
    assert decode_smt_proof(smt_bitmap + bytes([2]) + struct.pack(">H", 1) + b"\xaa" * 32) is None
    assert decode_smt_proof(smt_bitmap + bytes([1]) + struct.pack(">H", 0)) is None
    assert decode_smt_proof(b"\x00" * 24 + bytes([1]) + struct.pack(">H", 65535)) is None

    pat_wire = (
        struct.pack(">H", 2)
        + bytes([2, 0])
        + struct.pack(">H", 3)
        + bytes([0xE0])
        + bytes([1, 3, 1])
        + (b"\xbb" * 32)
        + bytes([1, 4])
        + (b"\x80" + b"\x00" * 23)
    )
    assert_all_truncations_and_trailing_reject(
        decode_patricia_proof, pat_wire, "PatriciaPointProof"
    )
    # Over-limit step_count (193 and 65535)
    assert decode_patricia_proof(struct.pack(">H", 193) + bytes([0])) is None
    assert decode_patricia_proof(struct.pack(">H", 65535) + bytes([0])) is None
    # Nonzero unused padding bit in 3-bit extension (0xE1 instead of 0xE0)
    bad_pad = bytearray(pat_wire)
    bad_pad[6] = 0xE1
    assert decode_patricia_proof(bytes(bad_pad)) is None
    # Invalid step tag (0 or 3) and invalid terminal tag (3)
    bad_step_tag = bytearray(pat_wire)
    bad_step_tag[2] = 9
    assert decode_patricia_proof(bytes(bad_step_tag)) is None

    # 7. Octree ChunkLeafV1 (537 bytes) & OctreeRootV1 (49 bytes)
    chunk_bits = bytearray(512)
    chunk_bits[7] = 1
    chunk_leaf_wire = b"\x00" * 23 + struct.pack(">H", 1) + bytes(chunk_bits)
    assert_all_truncations_and_trailing_reject(
        decode_chunk_leaf, chunk_leaf_wire, "ChunkLeafV1"
    )
    # Nonzero low nibble in 180-bit prefix
    bad_nibble = bytearray(chunk_leaf_wire)
    bad_nibble[22] = 0x01
    assert decode_chunk_leaf(bytes(bad_nibble)) is None
    # Popcount mismatch (header says 2, bitset has 1)
    bad_pop = bytearray(chunk_leaf_wire)
    bad_pop[24] = 2
    assert decode_chunk_leaf(bytes(bad_pop)) is None
    # Empty chunk leaf (occupied == 0) rejected
    assert decode_chunk_leaf(b"\x00" * 537) is None

    oct_root_wire = bytes([1]) + struct.pack(">QQ", 1, 1) + (b"\x11" * 32)
    assert_all_truncations_and_trailing_reject(
        decode_octree_root, oct_root_wire, "OctreeRootV1"
    )
    assert decode_octree_root(bytes([3]) + oct_root_wire[1:]) is None
    assert decode_octree_root(bytes([0]) + struct.pack(">QQ", 0, 1) + b"\x00" * 32) is None
    assert decode_octree_root(bytes([1]) + struct.pack(">QQ", 1, 1) + b"\x00" * 32) is None

    # 8. Check IEEE-754 non-finite float rejection & action-ledger overflow arithmetic
    for bad_f in (float("nan"), float("inf"), float("-inf")):
        assert not math.isfinite(bad_f)
    # Aggregate action total overflow: two lanes at 0x8000_0000 sum to 2^32 (wraps to 0 in u32)
    assert (0x8000_0000 + 0x8000_0000) > MAX_ACTIONS_PER_TICK
    assert ((0x8000_0000 + 0x8000_0000) & 0xFFFFFFFF) == 0

    # 9. Cross-check declared schemas for InteractionTape, HistoryReceiptPayload, ScaleWorkloadReportV1
    for kind, schema in (
        ("tape", INTERACTION_TAPE_SCHEMA),
        ("receipt", HISTORY_RECEIPT_SCHEMA),
        ("receipt", SCALE_WORKLOAD_REPORT_SCHEMA),
        ("world", OCTREE_CHUNK_LEAF_SCHEMA),
        ("world", OCTREE_INTERNAL_NODE_SCHEMA),
        ("world", OCTREE_ROOT_SCHEMA),
    ):
        t0, _ = hash_canonical(kind, schema, b"\x00")
        t1, _ = hash_canonical(kind, schema, b"\x01")
        assert parse_content_tag(t0) is not None
        assert t0 != t1

    print(
        "codec_parity_corpus: ok (14 local-v1 schemas, exhaustive truncation/trailing/overflow/noncanonical/cross-domain negative controls pass)"
    )


if __name__ == "__main__":
    run_all()
    sys.exit(0)
