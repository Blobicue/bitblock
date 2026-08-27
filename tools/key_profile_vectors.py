#!/usr/bin/env python3
"""Independent stdlib vectors for coordinate key order and authenticated node arity.

`docs/AUTHENTICATED_STATE_PLAN.md` retains an undecided question this file measures
rather than argues: whether a future spatial/chunk/Morton key profile is worth a
migration, or whether point and multiproofs under plain ``x || y || z`` suffice.

It reduces to counting. A sparse authenticated multiproof over the fixed 192-bit
keyspace must carry, for every node on the union of the queried root-to-leaf paths,
each child of that node that is not itself on the union. Children whose subtree holds
no world coordinate are the well-known empty-subtree constants and cost a bitmap bit
rather than a hash, so only NON-EMPTY children are counted as carried material.

TWO DECISIONS, TWO METRICS, AND THEY ARE NOT THE SAME.

  * Candidates A and B are both BINARY hash trees. Their carried material is the
    non-empty sibling count measured here, so this file speaks directly to them.
  * A later Verkle lane would carry roughly one commitment per LEVEL, with an
    aggregated opening, and the sibling count would not appear in its proof. Node
    arity is therefore a Verkle-lane variable that this file does not price. The
    structural table below reports only which arities can tile the key and which
    align with a cubic chunk; it is not a proof-size comparison for that lane.

WHAT THIS FILE ESTABLISHES. Exact carried-sibling counts for the named finite
fixtures below. Nothing else. It is not a security argument, not an implementation,
not a selection between Candidate A and Candidate B, and not a Verkle admission. It
does not import or launch Bitblock and does not read or write repository state.

WHAT IT DELIBERATELY DOES NOT MODEL. Node encoding overhead, Patricia path
compression (which carries strictly less than the sparse bound counted here), proof
system constants, prover and verifier time, storage layout, and cache behaviour.

Run:  python -B tools/key_profile_vectors.py
"""

from __future__ import annotations

import bisect
from dataclasses import dataclass


KEY_BITS = 192
COORD_BITS = 64
SIGN_BIAS = 1 << (COORD_BITS - 1)
HASH_BYTES = 32
CHUNK = 16

# Byte-spread table: the bits of one byte moved three apart, so three spread bytes
# interleave into one Morton group without a per-bit loop over 64 positions.
_SPREAD = []
for _byte in range(256):
    _acc = 0
    for _i in range(8):
        if (_byte >> _i) & 1:
            _acc |= 1 << (3 * _i)
    _SPREAD.append(_acc)


def bias64(value: int) -> int:
    """CoordinateKeyV1's order-preserving map: two's complement, then sign-bit flip."""
    return (value & ((1 << COORD_BITS) - 1)) ^ SIGN_BIAS


def key_xyz(x: int, y: int, z: int) -> int:
    """CoordinateKeyV1 as specified today: BiasSigned64BE(x) || (y) || (z)."""
    return (bias64(x) << 128) | (bias64(y) << 64) | bias64(z)


def _spread64(value: int) -> int:
    out = 0
    for index in range(8):
        out |= _SPREAD[(value >> (8 * index)) & 0xFF] << (24 * index)
    return out


def key_morton(x: int, y: int, z: int) -> int:
    """The alternative profile: the same biased coordinates, bit-interleaved."""
    return (
        (_spread64(bias64(x)) << 2)
        | (_spread64(bias64(y)) << 1)
        | _spread64(bias64(z))
    )


KEY_ORDERS = (("x||y||z", key_xyz), ("morton", key_morton))


@dataclass(frozen=True)
class Material:
    non_empty: int
    empty: int

    @property
    def carried_bytes(self) -> int:
        return self.non_empty * HASH_BYTES


def multiproof_material(
    world_sorted: list[int], query_keys: list[int], arity_bits: int = 1
) -> Material:
    """Carried sibling material for `query_keys` against sorted `world_sorted`.

    Subtree occupancy is a range test against the sorted world, so the cost is one
    binary search per candidate sibling rather than a scan.
    """
    levels = KEY_BITS // arity_bits
    arity = 1 << arity_bits
    non_empty = 0
    empty = 0
    for level in range(levels):
        shift_parent = KEY_BITS - level * arity_bits
        shift_child = shift_parent - arity_bits
        on_path_parents = {key >> shift_parent for key in query_keys}
        on_path_children = {key >> shift_child for key in query_keys}
        span = 1 << shift_child
        for parent in on_path_parents:
            base = parent << arity_bits
            for digit in range(arity):
                child = base + digit
                if child in on_path_children:
                    continue
                low = child << shift_child
                index = bisect.bisect_left(world_sorted, low)
                if index < len(world_sorted) and world_sorted[index] < low + span:
                    non_empty += 1
                else:
                    empty += 1
    return Material(non_empty=non_empty, empty=empty)


# ------------------------------------------------------------------ structure
def arity_tiles_keyspace(arity_bits: int) -> bool:
    return KEY_BITS % arity_bits == 0


def perfect_cube_root(arity: int) -> int | None:
    root = round(arity ** (1.0 / 3.0))
    for candidate in (root - 1, root, root + 1):
        if candidate > 0 and candidate**3 == arity:
            return candidate
    return None


def chunk_levels(arity_bits: int, chunk_side: int = CHUNK) -> float:
    """Whole node levels a cubic chunk of `chunk_side` occupies, under Morton."""
    return 3 * (chunk_side.bit_length() - 1) / arity_bits


# ------------------------------------------------------------------- fixtures
def solid_cube(side, origin):
    ox, oy, oz = origin
    return [
        (ox + x, oy + y, oz + z)
        for x in range(side)
        for y in range(side)
        for z in range(side)
    ]


def hollow_shell(side, origin):
    ox, oy, oz = origin
    edge = side - 1
    return [
        (ox + x, oy + y, oz + z)
        for x in range(side)
        for y in range(side)
        for z in range(side)
        if x in (0, edge) or y in (0, edge) or z in (0, edge)
    ]


def flat_plane(side, origin):
    ox, oy, oz = origin
    return [(ox + x, oy, oz + z) for x in range(side) for z in range(side)]


def scattered(count, step, origin):
    ox, oy, oz = origin
    out = []
    for index in range(count):
        cx = ox + ((index * 7) % 5) * step
        cy = oy + ((index * 11) % 5) * step
        cz = oz + ((index * 13) % 5) * step
        out.extend(solid_cube(4, (cx, cy, cz)))
    return sorted(set(out))


def cuboid(origin, extent):
    ox, oy, oz = origin
    ex, ey, ez = extent
    return [
        (ox + x, oy + y, oz + z)
        for x in range(ex)
        for y in range(ey)
        for z in range(ez)
    ]


# Frozen known answers for the binary decision, which is the live one. Any drift in
# the key codecs, the fixtures, or the counting rule changes these.
EXPECTED_BINARY = {
    ("dense", "chunk-aligned"): (546, 6),
    ("dense", "chunk-unaligned"): (1092, 48),
    ("dense", "slab-2chunks"): (1090, 10),
    ("dense", "point"): (18, 18),
    ("shell", "chunk-aligned"): (546, 6),
    ("plane", "chunk-aligned"): (19, 2),
    ("scatter", "chunk-aligned"): (6, 4),
    ("scatter", "chunk-unaligned"): (4, 5),
}

# Frozen structural answers: which arities can tile a 192-bit key, and which put a
# cubic chunk inside a whole number of node levels.
EXPECTED_STRUCTURE = {
    2: (True, None, 12.0),
    4: (True, None, 6.0),
    8: (True, 2, 4.0),
    16: (True, None, 3.0),
    64: (True, 4, 2.0),
    256: (True, None, 1.5),
    512: (False, 8, None),
    729: (False, 9, None),
    4096: (True, 16, 1.0),
}


def main() -> None:
    origin = (-24, -24, -24)  # every fixture straddles the origin bedrock cell
    worlds = (
        ("dense", solid_cube(48, origin)),
        ("shell", hollow_shell(48, origin)),
        ("plane", flat_plane(96, (-48, 0, -48))),
        ("scatter", scattered(60, 24, (-64, -64, -64))),
    )
    queries = (
        ("chunk-aligned", cuboid((-16, -16, -16), (CHUNK, CHUNK, CHUNK))),
        ("chunk-unaligned", cuboid((-8, -8, -8), (CHUNK, CHUNK, CHUNK))),
        ("slab-2chunks", cuboid((-16, -16, -16), (2 * CHUNK, CHUNK, CHUNK))),
        ("point", [(-13, -11, -9)]),
    )

    failures: list[str] = []

    print("Bitblock key-profile vectors")
    print(f"  {KEY_BITS}-bit keyspace; carried material = non-empty siblings x {HASH_BYTES} bytes")
    print("  every fixture straddles the origin bedrock cell\n")

    print("=== 1. THE LIVE DECISION: key order for a BINARY hash tree ===")
    print("    Candidates A and B are both binary, so this is the metric that applies.\n")
    print("    world     query              x||y||z      morton     carried KiB   ratio")
    for world_name, world in worlds:
        keys = {name: sorted(fn(*c) for c in world) for name, fn in KEY_ORDERS}
        for query_name, query in queries:
            row = {}
            for order_name, fn in KEY_ORDERS:
                q = [fn(*c) for c in query]
                row[order_name] = multiproof_material(keys[order_name], q, 1)
            a, b = row["x||y||z"], row["morton"]
            pin = EXPECTED_BINARY.get((world_name, query_name))
            if pin is not None and (a.non_empty, b.non_empty) != pin:
                failures.append(
                    f"binary {(world_name, query_name)}: "
                    f"{(a.non_empty, b.non_empty)} != {pin}"
                )
            ratio = (a.non_empty / b.non_empty) if b.non_empty else float("inf")
            print(
                f"    {world_name:<9} {query_name:<17} {a.non_empty:>7} {b.non_empty:>11}"
                f"   {a.carried_bytes / 1024:>6.1f} -> {b.carried_bytes / 1024:<5.1f}"
                f"  {ratio:>6.1f}x"
            )

    print("\n=== 2. STRUCTURE: which arities a 192-bit key and a cubic chunk admit ===")
    print("    A Verkle-lane variable only. This is not a proof-size comparison.\n")
    print("    arity   tiles 192?   perfect cube?   levels for a 16^3 chunk")
    for arity in sorted(EXPECTED_STRUCTURE):
        bits = arity.bit_length() - 1
        tiles = arity_tiles_keyspace(bits)
        cube = perfect_cube_root(arity)
        levels = chunk_levels(bits) if tiles else None
        got = (tiles, cube, levels)
        if got != EXPECTED_STRUCTURE[arity]:
            failures.append(f"structure {arity}: {got} != {EXPECTED_STRUCTURE[arity]}")
        whole = levels is not None and levels == int(levels)
        print(
            f"    {arity:>5}   {str(tiles):>10}   {(f'yes {cube}^3' if cube else 'no'):>13}"
            f"   {('%.2f' % levels) if levels is not None else 'n/a':>10}"
            f"   {'whole' if whole else ('partial' if levels is not None else 'does not tile')}"
        )

    print("\n=== 3. THE COST MORTON DOES CARRY, AT WIDE ARITY ===")
    print("    Morton puts a coordinate's spatial neighbours inside its own node.")
    print("    That is why a chunk is cheap; it is also why a LONE POINT is not.\n")
    dense = sorted(key_morton(*c) for c in worlds[0][1])
    dense_xyz = sorted(key_xyz(*c) for c in worlds[0][1])
    point = [(-13, -11, -9)]
    print("    arity   x||y||z point   morton point   morton penalty")
    for arity_bits, arity in ((1, 2), (3, 8), (6, 64), (8, 256)):
        a = multiproof_material(dense_xyz, [key_xyz(*c) for c in point], arity_bits)
        b = multiproof_material(dense, [key_morton(*c) for c in point], arity_bits)
        print(
            f"    {arity:>5}   {a.non_empty:>13}   {b.non_empty:>12}"
            f"   {b.non_empty / a.non_empty:>10.2f}x"
        )
    print("\n    At binary arity the two orders are equal on a point, so the range gain")
    print("    in section 1 is bought for nothing. The penalty appears only as nodes")
    print("    widen, which is a consideration for the deferred lane and not for A or B.")

    if failures:
        raise SystemExit("key-profile vector drift:\n  " + "\n  ".join(failures))
    print("\n  all pinned vectors agree")


if __name__ == "__main__":
    main()
