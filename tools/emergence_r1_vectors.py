#!/usr/bin/env python3
"""Independent stdlib 4-metric emergence assay & vectors for EMERGENCE-R1 (Wave 6).

Imports no Rust/Bitblock code. Reconstructs both canonical Wave 6 schemas:
  * bitblock/emergence-r1/corpus-spec/v1
  * bitblock/emergence-r1/assay-report/v1

Independently executes SplitMix64 soup generation, synchronous 3D Moore stepping across
B5/S45 (RULE-1), B6/S45 (subcritical control), and B4/S45 (supercritical control),
MortonGapStreamV1 byte lengths, 1-bit perturbation Hamming divergence, scaled L1 centroid
displacement, and world_tag periodicity.
"""

from __future__ import annotations

import struct
import sys

from crypto_d1_vectors import content_tag
from semantic_batch_vectors import world_tag

CORPUS_SPEC_SCHEMA = b"bitblock/emergence-r1/corpus-spec/v1"
ASSAY_REPORT_SCHEMA = b"bitblock/emergence-r1/assay-report/v1"

BEDROCK = (0, 0, 0)
SOUP_ORIGIN = (12, 12, 12)
SEEDS = [0xB17B_0001, 0xB17B_0002, 0xB17B_0003, 0xB17B_0004]
TICKS = 20
BOX_EDGE = 8
DENSITY_BPS = 2000
MAX_CANDIDATES = 60_000
MAX_MUTATIONS = 4_096

RULE_MASKS = {
    1: (1 << 5, (1 << 4) | (1 << 5)),  # B5/S45
    2: (1 << 6, (1 << 4) | (1 << 5)),  # B6/S45 subcritical
    3: (1 << 4, (1 << 4) | (1 << 5)),  # B4/S45 supercritical
}

GLIDER = [
    (0, 0, 0), (0, 0, 1), (1, 0, 2), (1, 1, 0), (1, 1, 1),
    (2, 0, 2), (2, 1, 0), (2, 1, 1), (3, 0, 0), (3, 0, 1),
]
OSCILLATOR = [
    (0, 0, 0), (0, 1, 0), (1, 0, 0), (1, 2, 0), (2, 1, 0),
    (0, 0, 1), (0, 1, 1), (1, 0, 1), (1, 2, 1), (2, 1, 1),
]
OCTAHEDRON = [(-1, 0, 0), (1, 0, 0), (0, -1, 0), (0, 1, 0), (0, 0, -1), (0, 0, 1)]
MASK64 = 0xFFFF_FFFF_FFFF_FFFF


def splitmix64_next(state: int) -> tuple[int, int]:
    state = (state + 0x9E37_79B9_7F4A_7C15) & MASK64
    z = state
    z = ((z ^ (z >> 30)) * 0xBF58_476D_1CE4_E5B9) & MASK64
    z = ((z ^ (z >> 27)) * 0x94D0_49BB_1331_11EB) & MASK64
    return state, (z ^ (z >> 31)) & MASK64


def generate_soup(seed: int) -> set[tuple[int, int, int]]:
    w = {BEDROCK}
    s = seed
    ox, oy, oz = SOUP_ORIGIN
    for dx in range(BOX_EDGE):
        for dy in range(BOX_EDGE):
            for dz in range(BOX_EDGE):
                s, v = splitmix64_next(s)
                if (v % 10_000) < DENSITY_BPS:
                    w.add((ox + dx, oy + dy, oz + dz))
    return w


def morton_key(c: tuple[int, int, int]) -> bytes:
    axes = [(v & MASK64) ^ 0x8000_0000_0000_0000 for v in c]
    out = bytearray(24)
    for level in range(64):
        shift = 63 - level
        for axis, u in enumerate(axes):
            if (u >> shift) & 1:
                bp = 3 * level + axis
                out[bp >> 3] |= 1 << (7 - (bp & 7))
    return bytes(out)


def morton_compress_bps(w: set[tuple[int, int, int]]) -> int:
    if not w:
        return 0
    keys = sorted(morton_key(c) for c in w)
    enc_len = 4 + 24
    for prev, cur in zip(keys, keys[1:]):
        shared = 0
        while shared < 24 and prev[shared] == cur[shared]:
            shared += 1
        enc_len += 1 + (24 - shared)
    return (enc_len * 10_000) // (len(keys) * 24)


def step_moore(w: set[tuple[int, int, int]], rule_code: int):
    birth_mask, survive_mask = RULE_MASKS[rule_code]
    counts: dict[tuple[int, int, int], int] = {}
    for x, y, z in w:
        for dx in (-1, 0, 1):
            for dy in (-1, 0, 1):
                for dz in (-1, 0, 1):
                    if dx == 0 and dy == 0 and dz == 0:
                        continue
                    nb = (x + dx, y + dy, z + dz)
                    if nb == BEDROCK:
                        continue
                    counts[nb] = counts.get(nb, 0) + 1
                    if len(counts) > MAX_CANDIDATES:
                        return None
    on_cells = [c for c, cnt in counts.items() if c not in w and ((birth_mask >> cnt) & 1)]
    off_cells = [
        c for c in w if c != BEDROCK and not ((survive_mask >> counts.get(c, 0)) & 1)
    ]
    if len(on_cells) + len(off_cells) > MAX_MUTATIONS:
        return None
    for c in on_cells:
        w.add(c)
    for c in off_cells:
        w.discard(c)
    return len(on_cells), len(off_cells)


def sum_and_count(w: set[tuple[int, int, int]]) -> tuple[int, int, int, int]:
    sx = sy = sz = n = 0
    for c in w:
        if c != BEDROCK:
            sx += c[0]
            sy += c[1]
            sz += c[2]
            n += 1
    return sx, sy, sz, n


def centroid_l1_x100(init_sc, fin_sc) -> int:
    if init_sc[3] == 0 or fin_sc[3] == 0:
        return 0
    tot = 0
    for i in range(3):
        tot += (abs(fin_sc[i] * init_sc[3] - init_sc[i] * fin_sc[3]) * 100) // (init_sc[3] * fin_sc[3])
    return min(tot, 0xFFFF_FFFF)


def eval_traj(rule_code: int, subject_kind: int, seed: int, init_w: set[tuple[int, int, int]], pcoord):
    w = set(init_w)
    pw = set(init_w)
    if pcoord in pw:
        pw.remove(pcoord)
    else:
        pw.add(pcoord)
    init_sc = sum_and_count(w)
    t0_dig = world_tag(w).digest
    peak = init_sc[3]
    surv = act = period = executed = 0
    cap_hit = 0
    for tick in range(1, TICKS + 1):
        res = step_moore(w, rule_code)
        if res is None:
            cap_hit = 1
            break
        births, deaths = res
        executed = tick
        lm = len(w) - 1
        if lm > peak:
            peak = lm
        if lm > 0:
            surv += 1
        if births or deaths:
            act += 1
        if period == 0 and world_tag(w).digest == t0_dig:
            period = tick
    for _ in range(executed):
        if step_moore(pw, rule_code) is None:
            break
    fin_sc = sum_and_count(w)
    return struct.pack(
        ">BBQIIIHHBIIIH",
        rule_code,
        subject_kind,
        seed,
        init_sc[3],
        fin_sc[3],
        peak,
        surv,
        act,
        cap_hit,
        morton_compress_bps(w),
        len(w ^ pw),
        centroid_l1_x100(init_sc, fin_sc),
        period,
    ) + world_tag(w).digest


def main() -> int:
    spec_bytes = struct.pack(">HHBHIIH", 1, TICKS, BOX_EDGE, DENSITY_BPS, MAX_CANDIDATES, MAX_MUTATIONS, len(SEEDS))
    for s in SEEDS:
        spec_bytes += struct.pack(">Q", s)
    spec_tag, spec_digest = content_tag("policy", CORPUS_SPEC_SCHEMA, spec_bytes)

    entries = []
    # Engineered B5/S45 structures at (20,20,20)
    for subj, base in ((1, GLIDER), (2, OSCILLATOR), (3, OCTAHEDRON)):
        pts = [(20 + x, 20 + y, 20 + z) for x, y, z in base]
        entries.append(eval_traj(1, subj, 0, {BEDROCK} | set(pts), pts[0]))
    perturb_soup = (SOUP_ORIGIN[0] + BOX_EDGE // 2, SOUP_ORIGIN[1] + BOX_EDGE // 2, SOUP_ORIGIN[2] + BOX_EDGE // 2)
    for rc in (1, 2, 3):
        for s in SEEDS:
            entries.append(eval_traj(rc, 4, s, generate_soup(s), perturb_soup))

    report_bytes = struct.pack(">H", 1) + spec_digest + struct.pack(">I", len(entries))
    for e in entries:
        assert len(e) == 73
        report_bytes += e
    report_tag, _ = content_tag("receipt", ASSAY_REPORT_SCHEMA, report_bytes)

    assert spec_tag == "#bb.policy.sha256.5cby7wdtezi5y64vo6pcsjt5kdcurkmezxnjsq5oidbavlca4mza", spec_tag
    assert report_tag == "#bb.receipt.sha256.54o2gvb4dxly2gxewna3zzvw6ufdtmiiveyc6msducfjbstadzca", report_tag
    print(f"emergence_r1_vectors: ok (spec={spec_tag}, report={report_tag}, 2/2 golden tags pinned)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
