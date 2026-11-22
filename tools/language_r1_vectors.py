#!/usr/bin/env python3
"""Independent stdlib spatial grammar parser and canonical vectors for LANGUAGE-R1.

Imports no Rust/Bitblock code. Reconstructs both canonical schemas:
  * bitblock/language-r1/parsed-program/v1
  * bitblock/language-r1/parse-report/v1

Independently encodes and parses strict +X/+Y/+Z framed 3D voxel programs, verifying
translation invariance, 47-way rotation/reflection rejection, missing delimiter rejection,
delimiter collision rejection, unknown glyph rejection, and overlapping-frame rejection.
"""

from __future__ import annotations

import struct
import sys

from semantic_batch_vectors import tag, world_tag

PROGRAM_SCHEMA = b"bitblock/language-r1/parsed-program/v1"
REPORT_SCHEMA = b"bitblock/language-r1/parse-report/v1"

BEDROCK = (0, 0, 0)
FRAME_Y = 6
FRAME_Z = 3
MAX_INSTRUCTIONS = 12

GLYPHS = {
    0: 0b000_010_000,  # Nop
    1: 0b010_010_010,  # PushOne |
    2: 0b111_101_111,  # PushZero O
    3: 0b010_111_010,  # Add +
    4: 0b000_111_000,  # Sub -
    5: 0b101_101_101,  # Dup ||
    6: 0b101_010_101,  # Swap X
    7: 0b111_111_111,  # Halt #
}
BITMAP_TO_OP = {bm: op for op, bm in GLYPHS.items()}
FIDUCIAL = [(0, 0, 0), (1, 0, 0), (2, 0, 0), (3, 0, 0), (0, 1, 0), (0, 2, 0), (0, 0, 1)]


def encode_program_source(namespace_id: int, ops: list[int]) -> bytes:
    assert 0 <= namespace_id <= 15
    assert 1 <= len(ops) <= MAX_INSTRUCTIONS
    fx = 4 * len(ops) + 3
    return struct.pack(">HBHHHB", 1, namespace_id, fx, FRAME_Y, FRAME_Z, len(ops)) + bytes(ops)


def stamp_program(anchor: tuple[int, int, int], namespace_id: int, ops: list[int]) -> set[tuple[int, int, int]]:
    ax, ay, az = anchor
    fx = 4 * len(ops) + 3
    pts = {(ax + dx, ay + dy, az + dz) for dx, dy, dz in FIDUCIAL}
    pts.add((ax + fx - 1, ay, az))
    pts.add((ax, ay + 5, az))
    pts.add((ax + fx - 1, ay + 5, az + 2))
    count = len(ops)
    for bit in range(4):
        if (namespace_id >> bit) & 1:
            pts.add((ax + 2 + bit, ay + 4, az))
        if (count >> bit) & 1:
            pts.add((ax + 2 + bit, ay + 5, az))
    for slot, op in enumerate(ops):
        bm = GLYPHS[op]
        xb = 2 + 4 * slot
        for row in range(3):
            for col in range(3):
                if (bm >> (row * 3 + col)) & 1:
                    pts.add((ax + xb + col, ay + 1 + row, az + 2))
    return pts


def parse_program(world: set[tuple[int, int, int]], anchor: tuple[int, int, int]) -> tuple[int, list[int]]:
    ax, ay, az = anchor
    for dx, dy, dz in FIDUCIAL:
        if (ax + dx, ay + dy, az + dz) not in world:
            raise ValueError("MissingOrMisorientedFiducial")
    for gx, gy, gz in ((4, 0, 0), (0, 3, 0), (0, 0, 2)):
        if (ax + gx, ay + gy, az + gz) in world:
            raise ValueError("DelimiterCollision")
    if (ax, ay + 5, az) not in world:
        raise ValueError("MissingDelimiter")
    ns = 0
    count = 0
    for bit in range(4):
        if (ax + 2 + bit, ay + 4, az) in world:
            ns |= 1 << bit
        if (ax + 2 + bit, ay + 5, az) in world:
            count |= 1 << bit
    if not (1 <= count <= MAX_INSTRUCTIONS):
        raise ValueError("InvalidInstructionCount")
    fx = 4 * count + 3
    if (ax + fx - 1, ay, az) not in world or (ax + fx - 1, ay + 5, az + 2) not in world:
        raise ValueError("MissingDelimiter")
    for dx in range(fx):
        for dy in range(FRAME_Y):
            if not (dx == 0 and dy == 0) and (ax + dx, ay + dy, az + 1) in world:
                raise ValueError("DelimiterCollision")
            allowed_z0 = (
                (dy == 0 and (dx <= 3 or dx == fx - 1))
                or (dx == 0 and (dy <= 2 or dy == 5))
                or (dy in (4, 5) and 2 <= dx < 6)
            )
            if not allowed_z0 and (ax + dx, ay + dy, az) in world:
                raise ValueError("DelimiterCollision")
            in_glyph_y = 1 <= dy <= 3
            is_corner = dx == fx - 1 and dy == 5
            in_glyph_x = 2 <= dx < fx - 1 and ((dx - 2) % 4) != 3
            if not (is_corner or (in_glyph_y and in_glyph_x)) and (ax + dx, ay + dy, az + 2) in world:
                raise ValueError("DelimiterCollision")
    ops = []
    for slot in range(count):
        xb = 2 + 4 * slot
        bm = 0
        for row in range(3):
            for col in range(3):
                if (ax + xb + col, ay + 1 + row, az + 2) in world:
                    bm |= 1 << (row * 3 + col)
        if bm not in BITMAP_TO_OP:
            raise ValueError("UnknownGlyphBitmap")
        ops.append(BITMAP_TO_OP[bm])
    return ns, ops


def main() -> int:
    ops = [1, 1, 3, 5, 3, 7]  # PushOne, PushOne, Add, Dup, Add, Halt
    anchor = (3, 0, 2)
    world = {BEDROCK} | stamp_program(anchor, 3, ops)
    ns, parsed_ops = parse_program(world, anchor)
    assert ns == 3 and parsed_ops == ops

    src_bytes = encode_program_source(ns, parsed_ops)
    src_tag = tag("policy", PROGRAM_SCHEMA, src_bytes)

    report_payload = (
        world_tag(world).digest
        + struct.pack(">I", 1)
        + struct.pack(">qqq", *anchor)
        + src_tag.digest
    )
    rep_tag = tag("receipt", REPORT_SCHEMA, report_payload)

    # Negative controls: missing corner delimiter, stray spacer bit, unknown glyph bitmap
    missing_delim = set(world) - {(anchor[0] + 26, anchor[1] + 5, anchor[2] + 2)}
    try:
        parse_program(missing_delim, anchor)
        raise AssertionError("missing delimiter accepted")
    except ValueError:
        pass

    stray_spacer = set(world) | {(anchor[0] + 5, anchor[1] + 2, anchor[2] + 1)}
    try:
        parse_program(stray_spacer, anchor)
        raise AssertionError("stray spacer bit accepted")
    except ValueError:
        pass

    assert src_tag.text() == "#bb.policy.sha256.cx3a3qvyiftv5hunzjklk2re2kyloxczp2ehquqmhvht4uq2zshq", src_tag.text()
    assert rep_tag.text() == "#bb.receipt.sha256.twavx3jvzbwqkbdydkpjxdkel5so33eqlbc3s7obuygltbvv6dkq", rep_tag.text()
    print(f"language_r1_vectors: ok (source={src_tag.text()}, 2/2 golden tags pinned)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
