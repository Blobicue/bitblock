#!/usr/bin/env python3
"""Independent analytic RGB/depth vectors for Bitblock's direct-cube renderer.

This stdlib-only oracle imports no Bitblock or Rust code.  The production path
projects cube faces and rasterizes two triangles per face.  This reference path
instead sends one pinhole ray through each physical-pixel center and intersects
that ray with unit AABBs.  The deliberately small corpus stays outside the
inside-bit, one-pixel aggregate, and subpixel-point shortcuts; equality is only
claimed for these named fixtures, never for every camera/world configuration.

The default gate also reads the Rust test constants as plain text to prove that
the retained vectors are exactly these independently generated rows.  It never
executes or reuses Rust renderer logic.

Run with ``--emit-rust`` to print the sparse color/depth samples pinned by the
Rust mismatch-vector test.  No file is written.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import math
from pathlib import Path
import re
import struct
from dataclasses import dataclass


TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
POLICY_KIND = b"policy"
FRAME_KIND = b"frame"
RECEIPT_KIND = b"receipt"
FRAME_SCHEMA = b"bitblock/frame/xrgb8888-row-major/v1"
ORACLE_PROFILE_SCHEMA = b"bitblock/reference-renderer/analytic-six-face/v1"
DEPTH_SCHEMA = b"bitblock/reference-depth-buffer/analytic-six-face/v1"
CORPUS_SCHEMA = b"bitblock/reference-renderer-corpus-receipt/v1"

BACKGROUND = 0x000000
BEDROCK_PIXEL = 0xFF9F1C
BEDROCK_TOP = 0xFFC15A
BEDROCK_SIDE = 0xC86F0A
BEDROCK_FRONT = 0xE8890C
BEDROCK_BACK = 0xA95508
ORDINARY_TOP = 0xFFFFFF
ORDINARY_SIDE = 0xBDBDBD
ORDINARY_FRONT = 0x989898
ORDINARY_BACK = 0xD8D8D8
RGB_MASK = 0xFFFFFF
CROSSHAIR_GAP_RADIUS = 2
CROSSHAIR_ARM_LENGTH = 3
VERTICAL_FOV = math.pi / 2.0
POSITIVE_INFINITY_BITS = 0x7FF0_0000_0000_0000

EXPECTED_TAGS = {
    "profile": "#bb.policy.sha256.smpqhfhoadryvrzezww32nve3cxkkc7tr4ubctfnhew7cscp64ua",
    "front-bedrock-frame": "#bb.frame.sha256.obrjmzpxoggqqmwukec7lpl6f24bcg63nn2l6q65ync2jxfeetra",
    "front-bedrock-depth": "#bb.receipt.sha256.76vbzm46kufgtyjq7andifkzwbkotvb4p54b2yl35sla4kv3cwea",
    "offset-occlusion-frame": "#bb.frame.sha256.yrvobugmwykavefvg5xmrvzl3ugqb4scyjr32bdv5mrgag642c6a",
    "offset-occlusion-depth": "#bb.receipt.sha256.2r36dtvymqot5hjqb2zkz3ru7fe5ki6waqclvfhgtzg5ngmegcuq",
    "corpus": "#bb.receipt.sha256.x2m63bxhu5ayhccnnij3ohv6ux7x3uk3hgal2qy334qb2atkam5q",
}
CANDIDATE_PROFILE_TAG = "#bb.policy.sha256.tsxgj54fonmrrlkoygcn5dtv65iu6neymvyee3x6hartgevwyynq"

Vec3 = tuple[float, float, float]
Coord = tuple[int, int, int]


@dataclass(frozen=True)
class Fixture:
    name: str
    width: int
    height: int
    camera: Vec3
    observation_distance: float
    blocks: tuple[Coord, ...]


FIXTURES = (
    Fixture(
        name="front-bedrock",
        width=17,
        height=13,
        camera=(0.0, 0.0, -2.5),
        observation_distance=1.0,
        blocks=((0, 0, 0),),
    ),
    Fixture(
        name="offset-occlusion",
        width=31,
        height=23,
        camera=(2.0, 2.0, -5.0),
        observation_distance=1.0,
        blocks=((0, 0, 0), (1, 0, 1), (-1, 1, 2), (0, 0, 2)),
    ),
)


def framed(value: bytes) -> bytes:
    return struct.pack(">H", len(value)) + value


def u32(value: int) -> bytes:
    return struct.pack(">I", value)


def u64(value: int) -> bytes:
    return struct.pack(">Q", value)


def f64(value: float) -> bytes:
    return struct.pack(">d", value)


def canonical_preimage(kind: bytes, schema: bytes, payload: bytes) -> bytes:
    return (
        framed(TAG_DOMAIN)
        + framed(kind)
        + framed(HASH_SUITE)
        + framed(schema)
        + u64(len(payload))
        + payload
    )


def digest(kind: bytes, schema: bytes, payload: bytes) -> bytes:
    return hashlib.sha256(canonical_preimage(kind, schema, payload)).digest()


def readable_tag(kind: bytes, value: bytes) -> str:
    encoded = base64.b32encode(value).decode("ascii").lower().rstrip("=")
    return f"#bb.{kind.decode('ascii')}.sha256.{encoded}"


def readable_digest(tag: str) -> bytes:
    encoded = tag.rsplit(".", 1)[1].upper()
    return base64.b32decode(encoded + "=" * ((8 - len(encoded) % 8) % 8))


def frame_payload(width: int, height: int, pixels: tuple[int, ...]) -> bytes:
    return (
        framed(FRAME_SCHEMA)
        + u32(width)
        + u32(height)
        + u64(len(pixels))
        + b"".join(u32(pixel) for pixel in pixels)
    )


def profile_payload() -> bytes:
    # Version, ray convention, direct-cube-only marker, tie rule, overlay rule,
    # FOV bits, crosshair geometry, and every reachable palette word.
    return (
        struct.pack(">HBBBB", 1, 0, 0, 0, 0)
        + f64(VERTICAL_FOV)
        + u32(CROSSHAIR_GAP_RADIUS)
        + u32(CROSSHAIR_ARM_LENGTH)
        + b"".join(
            u32(color)
            for color in (
                RGB_MASK,
                BACKGROUND,
                BEDROCK_PIXEL,
                BEDROCK_TOP,
                BEDROCK_SIDE,
                BEDROCK_FRONT,
                BEDROCK_BACK,
                ORDINARY_TOP,
                ORDINARY_SIDE,
                ORDINARY_FRONT,
                ORDINARY_BACK,
            )
        )
    )


def add(left: Vec3, right: Vec3) -> Vec3:
    return tuple(a + b for a, b in zip(left, right, strict=True))  # type: ignore[return-value]


def scale(vector: Vec3, scalar: float) -> Vec3:
    return tuple(value * scalar for value in vector)  # type: ignore[return-value]


def dot(left: Vec3, right: Vec3) -> float:
    return sum(a * b for a, b in zip(left, right, strict=True))


def ray_direction(fixture: Fixture, x: int, y: int) -> Vec3:
    # The fixtures use an exact 90-degree vertical FOV, so tan(FOV/2)=1.
    # Keeping the projective ray unnormalised makes its parameter equal to
    # forward/view depth and avoids sharing production's vector normalisation.
    focal = fixture.height * 0.5
    camera_x = (x + 0.5 - fixture.width * 0.5) / focal
    camera_y = -(y + 0.5 - fixture.height * 0.5) / focal
    # All named fixtures deliberately use yaw=pitch=0, so this is the
    # independently written closed form of the pinhole basis.
    return camera_x, camera_y, 1.0


def intersect_unit_aabb_faces(
    origin: Vec3, direction: Vec3, coord: Coord
) -> tuple[float, Vec3] | None:
    """Intersect six face planes, independently of the production slab helper."""

    minima = tuple(value - 0.5 for value in coord)
    maxima = tuple(value + 0.5 for value in coord)
    axes: tuple[Vec3, ...] = ((1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0))
    hits: list[tuple[float, int, Vec3]] = []
    for axis, unit in enumerate(axes):
        for plane, normal in ((minima[axis], scale(unit, -1.0)), (maxima[axis], unit)):
            denominator = direction[axis]
            if denominator == 0.0 or dot(direction, normal) >= 0.0:
                continue
            depth = (plane - origin[axis]) / denominator
            if depth < 0.0:
                continue
            point = add(origin, scale(direction, depth))
            other_axes = tuple(index for index in range(3) if index != axis)
            if all(minima[index] <= point[index] <= maxima[index] for index in other_axes):
                hits.append((depth, face_color(coord, normal), normal))
    if not hits:
        return None
    depth, _color, normal = min(hits)
    return depth, normal


def face_color(coord: Coord, normal: Vec3) -> int:
    if coord == (0, 0, 0):
        if normal[1] > 0.5:
            return BEDROCK_TOP
        if abs(normal[0]) > 0.5:
            return BEDROCK_SIDE
        if normal[2] > 0.5:
            return BEDROCK_BACK
        return BEDROCK_FRONT
    if normal[1] > 0.5:
        return ORDINARY_TOP
    if abs(normal[0]) > 0.5:
        return ORDINARY_SIDE
    if normal[2] > 0.5:
        return ORDINARY_BACK
    return ORDINARY_FRONT


def apply_crosshair(width: int, height: int, pixels: list[int]) -> None:
    center_x = width // 2
    center_y = height // 2
    first = CROSSHAIR_GAP_RADIUS + 1
    last = CROSSHAIR_GAP_RADIUS + CROSSHAIR_ARM_LENGTH
    for distance in range(first, last + 1):
        for x, y in (
            (center_x - distance, center_y),
            (center_x + distance, center_y),
            (center_x, center_y - distance),
            (center_x, center_y + distance),
        ):
            if 0 <= x < width and 0 <= y < height:
                pixels[y * width + x] ^= RGB_MASK


def render_reference(fixture: Fixture) -> tuple[tuple[int, ...], tuple[float, ...]]:
    pixels = [BACKGROUND] * (fixture.width * fixture.height)
    depths = [math.inf] * len(pixels)
    for y in range(fixture.height):
        for x in range(fixture.width):
            direction = ray_direction(fixture, x, y)
            contenders: list[tuple[float, int, Coord]] = []
            for coord in fixture.blocks:
                hit = intersect_unit_aabb_faces(fixture.camera, direction, coord)
                if hit is None:
                    continue
                distance, normal = hit
                depth = distance
                contenders.append((depth, face_color(coord, normal), coord))
            if not contenders:
                continue
            depth, color, _coord = min(contenders)
            index = y * fixture.width + x
            pixels[index] = color
            depths[index] = depth
    apply_crosshair(fixture.width, fixture.height, pixels)
    return tuple(pixels), tuple(depths)


def depth_payload(width: int, height: int, depths: tuple[float, ...]) -> bytes:
    return (
        u32(width)
        + u32(height)
        + u64(len(depths))
        + b"".join(f64(value) for value in depths)
    )


def fixture_payload(fixture: Fixture, frame_hash: bytes, depth_hash: bytes) -> bytes:
    name = fixture.name.encode("ascii")
    position = fixture.camera
    anchor = (position[0], position[1], position[2] + fixture.observation_distance)
    camera_lanes = (
        *position,
        0.0,
        0.0,
        *anchor,
        fixture.observation_distance,
        VERTICAL_FOV,
    )
    return (
        framed(name)
        + u32(fixture.width)
        + u32(fixture.height)
        + b"".join(f64(value) for value in camera_lanes)
        + u32(len(fixture.blocks))
        + b"".join(struct.pack(">qqq", *coord) for coord in fixture.blocks)
        + frame_hash
        + depth_hash
    )


def sample_rows(pixels: tuple[int, ...], depths: tuple[float, ...]) -> list[tuple[int, int, int]]:
    rows = []
    for index, (pixel, depth) in enumerate(zip(pixels, depths, strict=True)):
        depth_bits = struct.unpack(">Q", f64(depth))[0]
        if pixel != BACKGROUND or depth_bits != POSITIVE_INFINITY_BITS:
            rows.append((index, pixel, depth_bits))
    return rows


def emit_rust(name: str, rows: list[tuple[int, int, int]]) -> None:
    const_name = name.upper().replace("-", "_") + "_REFERENCE"
    print(f"const {const_name}: &[(usize, u32, u64)] = &[")
    for index, pixel, depth_bits in rows:
        print(f"    ({index}, 0x{pixel:08x}, 0x{depth_bits:016x}),")
    print("];\n")


def verify_rust_vector_parity(observed: list[tuple[Fixture, bytes, bytes, list[tuple[int, int, int]]]]) -> None:
    source = (Path(__file__).resolve().parents[1] / "src" / "render.rs").read_text(
        encoding="utf-8"
    )
    row_pattern = re.compile(
        r"\((\d+),\s*0x([0-9a-fA-F_]+),\s*0x([0-9a-fA-F_]+)\),"
    )
    for fixture, _frame_hash, _depth_hash, expected_rows in observed:
        const_name = fixture.name.upper().replace("-", "_") + "_REFERENCE"
        block = re.search(
            rf"const {const_name}: &\[\(usize, u32, u64\)\] = &\[(.*?)\n    \];",
            source,
            flags=re.DOTALL,
        )
        if block is None:
            raise AssertionError(f"missing Rust reference constant {const_name}")
        actual_rows = [
            (int(index), int(pixel.replace("_", ""), 16), int(depth.replace("_", ""), 16))
            for index, pixel, depth in row_pattern.findall(block.group(1))
        ]
        if actual_rows != expected_rows:
            raise AssertionError(f"Rust/Python vector drift in {const_name}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--emit-rust", action="store_true")
    args = parser.parse_args()

    profile_hash = digest(POLICY_KIND, ORACLE_PROFILE_SCHEMA, profile_payload())
    corpus_parts = [profile_hash, readable_digest(CANDIDATE_PROFILE_TAG)]
    observed = []
    for fixture in FIXTURES:
        pixels, depths = render_reference(fixture)
        frame_hash = digest(
            FRAME_KIND,
            FRAME_SCHEMA,
            frame_payload(fixture.width, fixture.height, pixels),
        )
        depth_hash = digest(
            RECEIPT_KIND,
            DEPTH_SCHEMA,
            depth_payload(fixture.width, fixture.height, depths),
        )
        corpus_parts.append(fixture_payload(fixture, frame_hash, depth_hash))
        rows = sample_rows(pixels, depths)
        observed.append((fixture, frame_hash, depth_hash, rows))

    corpus_hash = digest(RECEIPT_KIND, CORPUS_SCHEMA, b"".join(corpus_parts))
    actual_tags = {"profile": readable_tag(POLICY_KIND, profile_hash)}
    for fixture, frame_hash, depth_hash, _rows in observed:
        actual_tags[f"{fixture.name}-frame"] = readable_tag(FRAME_KIND, frame_hash)
        actual_tags[f"{fixture.name}-depth"] = readable_tag(RECEIPT_KIND, depth_hash)
    actual_tags["corpus"] = readable_tag(RECEIPT_KIND, corpus_hash)
    if actual_tags != EXPECTED_TAGS:
        raise AssertionError(f"reference renderer KAT drift: {actual_tags!r}")
    verify_rust_vector_parity(observed)

    print(f"profile\t{actual_tags['profile']}")
    for fixture, frame_hash, depth_hash, rows in observed:
        finite = sum(bits != POSITIVE_INFINITY_BITS for _, _, bits in rows)
        print(
            f"{fixture.name}\t{fixture.width}x{fixture.height}\t"
            f"samples={len(rows)}\tfinite={finite}\t"
            f"{actual_tags[f'{fixture.name}-frame']}\t"
            f"{actual_tags[f'{fixture.name}-depth']}"
        )
    print(f"corpus\t{actual_tags['corpus']}")

    # The corpus must observe block position and pixel content rather than merely
    # rehashing declarations.  These are local script controls, not world proof.
    first = FIXTURES[0]
    pixels, depths = render_reference(first)
    moved = Fixture(
        first.name,
        first.width,
        first.height,
        first.camera,
        first.observation_distance,
        ((1, 0, 0),),
    )
    moved_pixels, _ = render_reference(moved)
    mutated = list(pixels)
    mutated[0] ^= 1
    assert pixels != moved_pixels
    assert digest(FRAME_KIND, FRAME_SCHEMA, frame_payload(first.width, first.height, pixels)) != digest(
        FRAME_KIND, FRAME_SCHEMA, frame_payload(first.width, first.height, tuple(mutated))
    )
    assert any(math.isfinite(value) for value in depths)
    print("negative-controls\t3\trejected")

    if args.emit_rust:
        print()
        for fixture, _frame_hash, _depth_hash, rows in observed:
            emit_rust(fixture.name, rows)


if __name__ == "__main__":
    main()
