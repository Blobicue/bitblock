# LANGUAGE-R1: spatial grammar and authority-boundary decision v1

This decision record resolves `PS-045:LANGUAGE-R1@grammar-choice=BOUNDED`, keeping
`PS-044:LANGUAGE-R1@base=READY` for independent dual-language spatial parser implementation in
Wave 3.

## Selected grammar model: Strict `+X/+Y/+Z` Framed 3D Regions

1. **Canonical coordinate framing & orientation:**
   - A spatial program occupies an explicit finite axis-aligned 3D frame anchored at a local origin
     `anchor = (ax, ay, az)` in **strict canonical `+X/+Y/+Z` world orientation** (translation-
     invariant by subtracting `anchor`, with zero ambiguity from camera angle or rendering).
   - An asymmetric 3-axis start fiducial marker at `(0, 0, 0)` + fixed-width header bitfields encode
     grammar version, bounding dimensions `(dx, dy, dz)`, namespace ID, and fixed-size 3D opcode
     glyph slots separated by canonical delimiter planes.
2. **Explicit fail-closed parse classification:**
   - Rotated placements, reflected placements, overlapping program frames, missing/corrupt
     delimiters, unknown glyph bitmaps, out-of-range dimensions, and visually similar but
     coordinate-distinct patterns all produce distinct typed parse errors rather than heuristic
     guesses.
3. **Authority boundary:**
   - Parsed spatial programs execute first strictly as an **opt-in local deterministic VM (`VM-1`)**
     with **zero randomness input** in v1 — **not** Layer 1 consensus code (`PROGRAM-CONSENSUS`
     remains `LOCKED` until `VM-D1`, `BOUNDS-1`, `VM-1`, and `REFINE-1@vm` pass).

## Rejected alternatives

- **24-rotation chiral fiducial search:** rejected for v1 to keep independent parsers (`Rust` +
  `Python` + future Lean 4 `REFINE-1@vm`) minimal and eliminate rotation-search ambiguity.
- **Free-form connected-component grammar (no frame box):** rejected due to graph-isomorphism cost
  and delimiter ambiguity when unrelated world blocks touch a program.
- **Direct admission as v1 Layer 1 consensus smart contracts:** rejected to avoid coupling VM
  metering and audit TCB into the initial Layer 1 launch profile.

## Acceptance gate for `PS-044:LANGUAGE-R1@base` (Wave 3)

Independent Rust and stdlib Python parsers agreeing 100% across valid programs, translations,
rotations, reflections, overlaps, missing delimiters, and visually similar non-canonical patterns.
