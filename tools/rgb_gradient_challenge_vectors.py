#!/usr/bin/env python3
"""Independent vectors for Bitblock's RGB-transition sensor-challenge selector.

This stdlib-only script reimplements the finite, deterministic selector without
importing or launching Bitblock. It chooses diagnostic framebuffer coordinates
from a named XRGB slice; it does not read a display, collect sensor values,
establish a camera/panel mapping, or prove scanout, depth, or emitted light.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import struct


TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
POLICY_KIND = b"policy"
POLICY_SCHEMA = b"bitblock/sensor-rgb-challenge/v1"
SELECTION_POLICY_SCHEMA = b"bitblock/sensor-rgb-gradient-challenge-policy/v1"
SELECTION_POLICY_PAYLOAD = (
    b"parent=verified-presentation-output-with-exact-desktop-raster;"
    b"pixels=expected-xrgb-row-major;"
    b"adjacency=right-and-down-existing-neighbours;"
    b"edge-score=sum-abs-rgb8-channel-deltas;"
    b"point-score=sum-existing-right-down-edge-scores;"
    b"rank=point-score-descending-then-y-ascending-then-x-ascending;"
    b"retain=top-requested-count;output=row-major;"
    b"purpose=diagnostic-sensor-challenge-selection-not-physical-proof"
)

# 2x2 frame: two equal strongest edges, deliberately testing row-before-column
# tie breaking and final row-major challenge ordering.
WIDTH = 2
HEIGHT = 2
PIXELS = (0x0000_0000, 0x0000_0000, 0x0000_0000, 0x00FF_FFFF)

EXPECTED = {
    "selection-policy": "#bb.policy.sha256.3fhlsd4o5bd75gk2mdfuf2aerog44kqj3z6e5w6kzr24emxryltq",
    "challenge-budget-1": "#bb.policy.sha256.3spwnsqwiesp2d3zjl2el34r2mbcdroqdvdwebkwxpfxcyiqsgpq",
    "challenge-budget-2": "#bb.policy.sha256.n2v2w67sapkkqtrkuwgt5p4ksuug6rinjyklrimetjjxk3pptruq",
    "facts-budget-2": "pixels=4,horizontal=2,vertical=2,nonzero=2,total=1530,max-edge=765,max-point=765",
}


def field(value: bytes) -> bytes:
    return struct.pack(">H", len(value)) + value


def content_tag(kind: bytes, schema: bytes, payload: bytes) -> bytes:
    preimage = (
        field(TAG_DOMAIN)
        + field(kind)
        + field(HASH_SUITE)
        + field(schema)
        + struct.pack(">Q", len(payload))
        + payload
    )
    return hashlib.sha256(preimage).digest()


def text(kind: bytes, digest: bytes) -> str:
    encoded = base64.b32encode(digest).decode("ascii").lower().rstrip("=")
    return f"#bb.{kind.decode('ascii')}.sha256.{encoded}"


def rgb_delta(left: int, right: int) -> int:
    return sum(
        abs((left >> shift & 0xFF) - (right >> shift & 0xFF))
        for shift in (16, 8, 0)
    )


def challenge_payload(points: tuple[tuple[int, int, int], ...]) -> bytes:
    if not points:
        raise ValueError("challenge must not be empty")
    if tuple((y, x) for x, y, _ in points) != tuple(
        sorted((y, x) for x, y, _ in points)
    ):
        raise ValueError("challenge must be canonical row-major")
    return struct.pack(">H", len(points)) + b"".join(
        struct.pack(">III", x, y, xrgb) for x, y, xrgb in points
    )


def select(point_budget: int) -> tuple[tuple[tuple[int, int, int], ...], str]:
    candidates: list[tuple[int, int, int, int]] = []
    horizontal = vertical = nonzero = total = max_edge = max_point = 0
    for y in range(HEIGHT):
        for x in range(WIDTH):
            index = y * WIDTH + x
            xrgb = PIXELS[index]
            score = 0
            if x + 1 < WIDTH:
                edge = rgb_delta(xrgb, PIXELS[index + 1])
                horizontal += 1
                nonzero += int(edge != 0)
                total += edge
                max_edge = max(max_edge, edge)
                score += edge
            if y + 1 < HEIGHT:
                edge = rgb_delta(xrgb, PIXELS[index + WIDTH])
                vertical += 1
                nonzero += int(edge != 0)
                total += edge
                max_edge = max(max_edge, edge)
                score += edge
            max_point = max(max_point, score)
            candidates.append((score, x, y, xrgb))

    strongest = sorted(candidates, key=lambda item: (-item[0], item[2], item[1]))[
        :point_budget
    ]
    points = tuple(sorted(((x, y, xrgb) for _, x, y, xrgb in strongest), key=lambda p: (p[1], p[0])))
    facts = (
        f"pixels={WIDTH * HEIGHT},horizontal={horizontal},vertical={vertical},"
        f"nonzero={nonzero},total={total},max-edge={max_edge},max-point={max_point}"
    )
    return points, facts


def vector_values() -> dict[str, str]:
    one, _ = select(1)
    two, facts = select(2)
    return {
        "selection-policy": text(
            POLICY_KIND,
            content_tag(POLICY_KIND, SELECTION_POLICY_SCHEMA, SELECTION_POLICY_PAYLOAD),
        ),
        "challenge-budget-1": text(
            POLICY_KIND,
            content_tag(POLICY_KIND, POLICY_SCHEMA, challenge_payload(one)),
        ),
        "challenge-budget-2": text(
            POLICY_KIND,
            content_tag(POLICY_KIND, POLICY_SCHEMA, challenge_payload(two)),
        ),
        "facts-budget-2": facts,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--emit", action="store_true")
    args = parser.parse_args()
    actual = vector_values()
    if args.emit:
        for name, value in actual.items():
            print(f"{name}\t{value}")
        return 0
    if any(not value for value in EXPECTED.values()):
        raise AssertionError("frozen expected RGB-transition vectors are not populated")
    if actual != EXPECTED:
        raise AssertionError(f"RGB-transition vector mismatch:\nactual={actual}\nexpected={EXPECTED}")
    print(f"rgb-gradient-challenge-vectors\tPASS\tcases={len(actual)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
