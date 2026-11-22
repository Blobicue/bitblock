# LANGUAGE-R1: exact strict `+X/+Y/+Z` framed 3D spatial grammar receipt v1

This receipt resolves `PS-044:LANGUAGE-R1@base=BOUNDED` (alongside
`PS-045:LANGUAGE-R1@grammar-choice=BOUNDED`) and unlocks `PS-046:VM-D1@base=READY`.

## Provenance

- **Merged `origin/main` subject (PR #30):** commit `9e6780c4ddb0c2cf47440412634afad0690d66ab`
  (tree `6e1e243a57c6cb00ad89fa3e783d9c3255bb199d`, parent `7aef48be0f6c0990edfff2218fc1b87d583b1a50`,
  CI runs `45030235227` & `45030229935`, 10/10 jobs green).
- **Executable module:** [`crates/research/src/language_r1.rs`](../crates/research/src/language_r1.rs)
  (`wasm32`-clean, strictly behind `--features protocol-research`; default live editing and
  `world_tag` remain untouched).
- **Independent Python parser & vector suite:** [`tools/language_r1_vectors.py`](../tools/language_r1_vectors.py)
  (stdlib-only, zero Rust imports; `111/111` protocol schemas independently vectored).
- **CI integration & mutation evidence:** [`tests/wave3_bootstrap_and_language_r1.rs`](../tests/wave3_bootstrap_and_language_r1.rs)
  and `python3 tools/bbmutate.py language_r1` (`8/8` canonical big-endian sites KILLED, 100%).
- **Visual snapshot exporter:** [`examples/language_r1_visual_probe.rs`](../examples/language_r1_visual_probe.rs)
  (`cargo run --release --features protocol-research --example language_r1_visual_probe -- --dump-snapshots <dir>`).

## Verified golden tags & adversarial parse corpus

| Check | Pinned tag / behavior | Verification |
| --- | --- | --- |
| `ParsedSpatialProgramV1` (`bitblock/language-r1/parsed-program/v1`, `[PushOne, PushOne, Add, Dup, Add, Halt]`, `namespace = 3`) | `#bb.policy.sha256.cx3a3qvyiftv5hunzjklk2re2kyloxczp2ehquqmhvht4uq2zshq` | Exact Rust + Python parity |
| `SpatialParseReportV1` (`bitblock/language-r1/parse-report/v1`, `anchor = (3, 0, 2)`) | `#bb.receipt.sha256.twavx3jvzbwqkbdydkpjxdkel5so33eqlbc3s7obuygltbvv6dkq` | Exact Rust + Python parity |
| All 8 v1 opcodes (`0..=7`: `Nop`, `PushOne`, `PushZero`, `Add`, `Sub`, `Dup`, `Swap`, `Halt`) | Bijective 3×3×1 glyph bitmaps round-trip cleanly at arbitrary signed `i64³` translation (`(-40, 20, -10)`) | CI verified |
| Strict `+X/+Y/+Z` chiral `(4, 3, 2)` L-fiducial orientation | Translation-invariant; all 47 non-identity octahedral rotations & reflections reject fail-closed | Unit + Python verified |
| Delimiter, spacer, glyph, and overlap guards | Missing outer corner -> `MissingDelimiter`; occupied `z=1` spacer voxel -> `DelimiterCollision`; mutated glyph -> `UnknownGlyphBitmap`; colliding anchors -> `OverlappingProgramFrames` | CI verified |

## Merged-head visual verification (`9e6780c`)

- **Rendered showcase:** 46 voxels (permanent orange bedrock `(0, 0, 0)` plus the 45-voxel framed
  program `[PushOne, PushOne, Add, Dup, Add, Halt]` (`| | + || + ■`) anchored at `(3, 0, 2)`).
- **Direct visual inspection:** both `801×801` merged-head PNGs exported by
  `examples/language_r1_visual_probe.rs` (`language_r1_front.png` and `language_r1_oblique.png`)
  were directly inspected, confirming crisp unclipped framing of orange bedrock, the `z=0` chiral
  L-fiducial and header bits, the empty `z=1` spacer plane, all six `z=2` 3D voxel glyphs, and
  outer corner delimiters.

## Non-claims

A canonical static voxel parser does not define VM execution semantics (`VM-D1`, now `READY`),
implement an interpreter (`VM-1`), establish resource bounds (`BOUNDS-1`), or grant visual
programs consensus authority (`PROGRAM-CONSENSUS`). Visual resemblance alone is never canonical
program data.
