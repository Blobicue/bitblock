#!/usr/bin/env python3
"""Generate the gallery .bb programs (docs/gallery/*.bb).

Each program is one thesis about what a one-bit world can express:

  symbol.bb       bits carry SYMBOLS      - the word BIT in a 5x7 font, extruded
  recursion.bb    bits carry RECURSION    - a level-2 Menger sponge (20^2 cells)
  recognition.bb  bits carry RECOGNITION  - the classic 16x16 smiley, extruded
  architecture.bb bits carry ARCHITECTURE - a gate you can walk the observer through

Structures stand on the y=0 ground plane and sit beside the origin bedrock,
which is always on. Cells are inflated (cell=N) so one-bit reads at gallery
distance. Idempotent: writes the same bytes for the same version of this file.
"""

from __future__ import annotations

from pathlib import Path

OUT = Path(__file__).parent

FONT5x7 = {
    # Each letter: 7 rows of 5 bits, top row first.
    "B": [
        "####.",
        "#...#",
        "#...#",
        "####.",
        "#...#",
        "#...#",
        "####.",
    ],
    "I": [
        "###",
        ".#.",
        ".#.",
        ".#.",
        ".#.",
        ".#.",
        "###",
    ],
    "T": [
        "#####",
        "..#..",
        "..#..",
        "..#..",
        "..#..",
        "..#..",
        "..#..",
    ],
}

SMILEY16 = [
    "....########....",
    "...##......##...",
    "..#..........#..",
    ".#....##..##..#.",
    "#....####..####.#"[:16],
    "#..............#",
    "#..............#",
    "#..............#",
    "#..#........#..#",
    "#..#........#..#",
    ".#...##..##...#.",
    ".#....####....#."[:16],
    "..#..........#..",
    "...##......##...",
    "....########....",
    "................",
]


def emit(lines: set[tuple[int, int, int]]) -> str:
    out = []
    for x, y, z in sorted(lines):
        out.append(f"on {x} {y} {z}")
    return "\n".join(out) + "\n"


def block(lines: set, x0: int, y0: int, z0: int, sx: int, sy: int, sz: int) -> None:
    for dx in range(sx):
        for dy in range(sy):
            for dz in range(sz):
                lines.add((x0 + dx, y0 + dy, z0 + dz))


def gen_symbol() -> str:
    cell, depth = 2, 2
    x = 4  # leave the bedrock column clear at x 0..3
    lines: set[tuple[int, int, int]] = set()
    for ch in "BIT":
        rows = FONT5x7[ch]
        width = len(rows[0])
        for ry, row in enumerate(rows):
            for rx, c in enumerate(row):
                if c == "#":
                    block(lines, x + rx * cell, (6 - ry) * cell, 0, cell, cell, depth)
        x += (width + 1) * cell
    return emit(lines)


def gen_recursion() -> str:
    cell = 4

    def menger(x0: int, y0: int, z0: int, level: int) -> set[tuple[int, int, int]]:
        if level == 0:
            return {(x0, y0, z0)}
        size = 3 ** (level - 1)
        pts: set[tuple[int, int, int]] = set()
        for dx in (0, 1, 2):
            for dy in (0, 1, 2):
                for dz in (0, 1, 2):
                    faces = (dx == 1) + (dy == 1) + (dz == 1)
                    if faces >= 2:
                        continue  # keep edges and corners: the Menger rule
                    pts |= menger(x0 + dx * size, y0 + dy * size, z0 + dz * size, level - 1)
        return pts

    # Level 2 (20^2 = 400 of 729 cells, 55% air): the level whose recursion is VISIBLE -
    # level 3's face holes open onto interior white walls and read as a solid mass.
    # Centered on the origin so the bedrock sits inside the sponge's hollow heart,
    # visible through the central tunnel; the camera then aligns with the tunnel axis.
    cells = menger(0, 0, 0, 2)
    cells = {(cx - 4, cy - 4, cz - 4) for (cx, cy, cz) in cells}
    lines = {
        (cx * cell + dx, cy * cell + dy, cz * cell + dz)
        for (cx, cy, cz) in cells
        for dx in range(cell)
        for dy in range(cell)
        for dz in range(cell)
    }
    lines = {
        (cx * cell + dx, cy * cell + dy, cz * cell + dz)
        for (cx, cy, cz) in cells
        for dx in range(cell)
        for dy in range(cell)
        for dz in range(cell)
    }
    return emit(lines)


def gen_recognition() -> str:
    cell, depth = 2, 2
    lines: set[tuple[int, int, int]] = set()
    for ry, row in enumerate(SMILEY16):
        for rx, c in enumerate(row):
            if c == "#":
                block(lines, 4 + rx * cell, (15 - ry) * cell, 0, cell, cell, depth)
    return emit(lines)


def gen_architecture() -> None:  # noqa: RUF029 - returns str like the others
    lines: set[tuple[int, int, int]] = set()
    # A gate: two flanking walls (x 4..7 and x 20..23), z depth 4, height 12,
    # a lintel spanning y 12..15, a pediment above it, and a threshold step.
    block(lines, 4, 0, 0, 4, 12, 4)    # left wall
    block(lines, 20, 0, 0, 4, 12, 4)   # right wall
    block(lines, 4, 12, 0, 20, 2, 4)   # lintel across the opening
    block(lines, 8, 14, 1, 12, 1, 2)   # pediment
    block(lines, 11, 14, 2, 6, 1, 2)   # pediment crown
    block(lines, 8, 0, 0, 12, 1, 4)    # threshold you can walk across
    # steps descending from the threshold toward the viewer (-z)
    block(lines, 8, 0, -2, 12, 1, 2)
    block(lines, 10, 0, -4, 8, 1, 2)
    return emit(lines)


def main() -> None:
    (OUT / "symbol.bb").write_text(gen_symbol(), encoding="ascii", newline="\n")
    (OUT / "recursion.bb").write_text(gen_recursion(), encoding="ascii", newline="\n")
    (OUT / "recognition.bb").write_text(gen_recognition(), encoding="ascii", newline="\n")
    (OUT / "architecture.bb").write_text(gen_architecture(), encoding="ascii", newline="\n")
    for name in ("symbol", "recursion", "recognition", "architecture"):
        count = sum(1 for line in (OUT / f"{name}.bb").read_text().splitlines() if line.startswith("on "))
        print(f"{name}.bb: {count} on-instructions")


if __name__ == "__main__":
    main()
