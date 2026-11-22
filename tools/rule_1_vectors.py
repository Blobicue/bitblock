#!/usr/bin/env python3
"""Independent stdlib reference stepper and canonical vectors for RULE-1 (synchronous 3D
Moore-neighborhood Life B5/S45).

Imports no Rust/Bitblock code. Reconstructs both canonical schemas:
  * bitblock/rule-1/descriptor/v1
  * bitblock/rule-1/step-receipt/v1

Verifies Bays' 10-cell period-4 3D glider translation (0, +1, +1) per 4 ticks, 6-cell 3D
octahedron still-life quiescence, 10-cell period-4 oscillator return, permanent bedrock (0,0,0)
survival under 2x2x2 overcrowding, and powered rule-mutation divergence (B6/S45).
"""

from __future__ import annotations

import hashlib
import struct
import sys

from semantic_batch_vectors import tag, world_tag

RULE_1_DESCRIPTOR_SCHEMA = b"bitblock/rule-1/descriptor/v1"
RULE_1_STEP_RECEIPT_SCHEMA = b"bitblock/rule-1/step-receipt/v1"

BEDROCK = (0, 0, 0)
BIRTH_MASK_B5 = 1 << 5
SURVIVE_MASK_S45 = (1 << 4) | (1 << 5)
DEFAULT_MAX_CAND = 1_000_000
DEFAULT_MAX_MUT = 65_536
I64_MIN = -(1 << 63)
I64_MAX = (1 << 63) - 1


def encode_descriptor(
    max_cand: int = DEFAULT_MAX_CAND,
    max_mut: int = DEFAULT_MAX_MUT,
    birth_mask: int = BIRTH_MASK_B5,
    survive_mask: int = SURVIVE_MASK_S45,
) -> bytes:
    return struct.pack(">HHIIBII", 1, 1, birth_mask, survive_mask, 1, max_cand, max_mut)


def step_life_3d(
    occupied: set[tuple[int, int, int]],
    tick: int,
    desc_digest: bytes,
    birth_mask: int = BIRTH_MASK_B5,
    survive_mask: int = SURVIVE_MASK_S45,
    max_cand: int = DEFAULT_MAX_CAND,
    max_mut: int = DEFAULT_MAX_MUT,
) -> tuple[set[tuple[int, int, int]], bytes, str, dict]:
    pre_world = world_tag(occupied).digest
    counts: dict[tuple[int, int, int], int] = {}
    for x, y, z in occupied:
        if (x, y, z) not in counts:
            counts[(x, y, z)] = 0
        for dx in (-1, 0, 1):
            nx = x + dx
            if not (I64_MIN <= nx <= I64_MAX):
                continue
            for dy in (-1, 0, 1):
                ny = y + dy
                if not (I64_MIN <= ny <= I64_MAX):
                    continue
                for dz in (-1, 0, 1):
                    if dx == 0 and dy == 0 and dz == 0:
                        continue
                    nz = z + dz
                    if not (I64_MIN <= nz <= I64_MAX):
                        continue
                    nb = (nx, ny, nz)
                    counts[nb] = counts.get(nb, 0) + 1
    assert len(counts) <= max_cand, "candidate limit exceeded"

    edits: list[tuple[int, int, int, int]] = []  # (x, y, z, op_code)
    births = 0
    deaths = 0
    nxt = set(occupied)
    nxt.add(BEDROCK)
    for coord, cnt in counts.items():
        if coord == BEDROCK:
            continue
        bit = 1 << cnt
        if coord in occupied:
            if (survive_mask & bit) == 0:
                edits.append((coord[0], coord[1], coord[2], 1))
                nxt.remove(coord)
                deaths += 1
        elif (birth_mask & bit) != 0:
            edits.append((coord[0], coord[1], coord[2], 0))
            nxt.add(coord)
            births += 1
    assert len(edits) <= max_mut, "mutation limit exceeded"

    edits.sort()
    h = hashlib.sha256()
    h.update(struct.pack(">I", len(edits)))
    for x, y, z, op in edits:
        h.update(struct.pack(">Bqqq", op, x, y, z))
    edits_hash = h.digest()

    post_world = world_tag(nxt).digest
    quiescent = 1 if (births == 0 and deaths == 0) else 0
    payload = (
        desc_digest
        + struct.pack(">Q", tick)
        + pre_world
        + post_world
        + struct.pack(">IIIB", len(counts), births, deaths, quiescent)
        + edits_hash
    )
    assert len(payload) == 149
    rcpt = tag("receipt", RULE_1_STEP_RECEIPT_SCHEMA, payload)
    meta = {
        "candidates": len(counts),
        "births": births,
        "deaths": deaths,
        "quiescent": bool(quiescent),
        "post_world": world_tag(nxt).text(),
    }
    return nxt, rcpt.digest, rcpt.text(), meta


def bays_glider(ox: int, oy: int, oz: int) -> set[tuple[int, int, int]]:
    base = [
        (0, 0, 0), (0, 0, 1), (1, 0, 2), (1, 1, 0), (1, 1, 1),
        (2, 0, 2), (2, 1, 0), (2, 1, 1), (3, 0, 0), (3, 0, 1),
    ]
    return {(ox + x, oy + y, oz + z) for x, y, z in base}


def octahedron(cx: int, cy: int, cz: int) -> set[tuple[int, int, int]]:
    return {
        (cx - 1, cy, cz), (cx + 1, cy, cz),
        (cx, cy - 1, cz), (cx, cy + 1, cz),
        (cx, cy, cz - 1), (cx, cy, cz + 1),
    }


def main() -> int:
    desc_bytes = encode_descriptor()
    assert len(desc_bytes) == 21
    desc_tag = tag("policy", RULE_1_DESCRIPTOR_SCHEMA, desc_bytes)

    # 1. Bays 10-cell glider at (20, 20, 20) + bedrock (0, 0, 0) for 4 ticks -> shifts (0, +1, +1)
    world = {BEDROCK} | bays_glider(20, 20, 20)
    t1_tag = ""
    t4_tag = ""
    for tk in range(1, 5):
        world, _, rtxt, _ = step_life_3d(world, tk, desc_tag.digest)
        if tk == 1:
            t1_tag = rtxt
        if tk == 4:
            t4_tag = rtxt
    assert world == ({BEDROCK} | bays_glider(20, 21, 21)), "glider failed to translate (0,+1,+1)"

    # 2. Still-life octahedron away from bedrock (at (0, 0, 6)) -> quiescent immediately on tick 1
    still = {BEDROCK} | octahedron(0, 0, 6)
    still_after, _, still_tag, meta = step_life_3d(still, 1, desc_tag.digest)
    assert still_after == still and meta["quiescent"], "octahedron still life mutated"

    # 3. Overcrowded 2x2x2 cube touching bedrock -> all 7 mutable cells die, bedrock survives
    crowded = {(x, y, z) for x in (0, 1) for y in (0, 1) for z in (0, 1)}
    crowded_after, _, _, cmeta = step_life_3d(crowded, 1, desc_tag.digest)
    assert crowded_after == {BEDROCK} and cmeta["deaths"] == 7 and cmeta["births"] == 0

    # 4. Powered mutant negative control: B6/S45 diverges from B5/S45 on Bays glider tick 1
    mut_world, _, _, _ = step_life_3d({BEDROCK} | bays_glider(20, 20, 20), 1, desc_tag.digest, birth_mask=(1 << 6))
    ref_world, _, _, _ = step_life_3d({BEDROCK} | bays_glider(20, 20, 20), 1, desc_tag.digest)
    assert mut_world != ref_world, "powered B6/S45 mutant failed to diverge"

    assert desc_tag.text() == "#bb.policy.sha256.jckp2uhbjj2fa4evhth5sqyzde45yy7bnpl4htsbuezxw55itdqq", desc_tag.text()
    assert t1_tag == "#bb.receipt.sha256.iixge4c2kypetpd2futpdd3cdiyr4e5pm2kpuf3zcx5oknbii57a", t1_tag
    assert t4_tag == "#bb.receipt.sha256.onankhpta54nmiufuyfkxpgqohp3nxjghr3dvvhlewqpyc64jfyq", t4_tag
    assert still_tag == "#bb.receipt.sha256.pp2w2naumy5a7v3dwwr72lmjd2qtkklbrwhbamb52xbabsjr74pa", still_tag

    print(f"rule_1_vectors: ok (desc={desc_tag.text()}, 4/4 golden tags pinned)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
