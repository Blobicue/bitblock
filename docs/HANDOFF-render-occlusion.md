# Handoff: occlusion culling in the rasteriser

One task, one constraint. The rasteriser is 90% of a frame's cost and most of
that work is thrown away by the depth test. Skip the bits that were never going
to be visible — **without changing a single pixel of any frame.**

## The measurement that motivates it

```bash
cargo run --release --example render_phase_probe
```

| bits | drawn px | clear | project | raster | total |
|---:|---:|---:|---:|---:|---:|
| 1 | 109,561 | 0.10ms | 0.00ms | 0.89ms | 0.99ms |
| 99 | 109,561 | 0.10ms | 0.05ms | 3.25ms | 3.40ms |
| 2,649 | 109,561 | 0.10ms | 0.60ms | 4.98ms | 5.68ms |
| 12,699 | 109,561 | 0.10ms | 1.30ms | 12.63ms | 14.02ms |

The final pixel count is **identical** in every row — the worlds are framed to
the same on-screen size. Clearing is 0.10ms and projection is 1.30ms at worst.
Everything else is the rasteriser, and what makes it grow is bits drawing over
each other. At 12,699 bits a frame is 84% of the 16.67ms budget at 60 Hz.

The worlds are hollow cube shells, so roughly half the bits face away from the
camera and are covered by the near face. Those are the target.

## The constraint that decides everything

This crate's value is byte-exact canonical encodings. A frame has a content tag,
101 literal tags are asserted across the test suite, and `docs/PROTOCOL.md`
requires that "occupied-bit traversal permutations must agree".

That cuts both ways, and both matter:

- **Reordering traversal is explicitly legal.** Front-to-back is available.
- **Any skipped bit must be provably invisible.** Not "probably", not "within an
  epsilon". If a bit could have written one pixel, rasterise it. A conservative
  test that occasionally fails to skip is correct and merely slower; a test that
  skips something visible is a wrong frame, and the wrongness is permanent
  because receipts commit to it.

Prefer under-culling to any risk of over-culling.

## How to know you did not break it

```bash
python tools/bbverify.py quick     # per edit, ~5s after a source change
python tools/bbverify.py full      # per commit
python tools/bbmutate.py render    # does the suite still notice a byte moving?
```

`bbverify full` runs 413 tests including every literal content tag, the 34
independent Python vector suites that rebuild the same payloads without
importing any Rust, `cargo fmt`, clippy with `-D warnings`, the metric registry,
and the index gates. Green means no canonical byte moved.

`tools/bbmutate.py` is the reason to believe that: it flips every `to_be_bytes`
in a module to `to_le_bytes` and confirms the suite fails. All 25 modules are
killed, so the net demonstrably notices.

The most direct check for this task specifically: `render`'s tests include
`named_frame_fixtures_match_full_protocol_tags_and_facts` and
`independent_analytic_ray_oracle_matches_bounded_rgb_and_depth_corpus`, which
compare against an independent analytic renderer rather than against the
implementation itself.

## Already tried, do not repeat

**Short-circuiting the barycentric test.** The inner loop computes three weights
then tests all three; testing each as it is computed is exactly equivalent and
skips two thirds of the divisions on the ~half of pixels outside the triangle.
It is **15–17% slower** at every world size, median of five interleaved A/B
rounds. The three divisions are independent and the hardware overlaps them; a
branch between each serialises what was already pipelined. Full write-up under
"Rejected: short-circuiting the barycentric test" in `docs/PROTOCOL.md`.

**Hoisting the divisions.** `w / area` is not `w * (1.0 / area)` in IEEE 754,
and nor is `w / depth` and its reciprocal. Both change the frame. Ruled out by
exactness, not by measurement.

The lesson from the first one is worth carrying: doing less work is not the same
as taking less time. Measure, do not reason, and A/B interleaved because this
machine drifts.

## Where things are

```
crates/core/src/render.rs      the rasteriser: render, draw_bit, raster_triangle, plot
crates/core/src/camera.rs      projection; `fit` means one-pixel saturation, not framing
crates/core/src/world.rs       sparse chunk storage, iter_on
examples/render_phase_probe.rs the decomposition above
examples/render_time_probe.rs  whole-frame cost against the 60Hz budget
```

`render()` already does back-face culling per cube face. `plot()` resolves
equal-depth fragments by choosing the lower RGB word, which is what makes the
frame independent of iteration order — preserve that, it is load-bearing for the
reordering permission above.

Two traps that cost three attempts when the probes were written:

- `Renderer::new` allocates 7.7MB. Build it outside any timed region.
- `World::empty()` is **not** empty — origin bedrock is immutable, so it holds
  one bit, and from the spawn camera that bit covers most of the screen.

## What a good result looks like

A measured reduction in `raster` on the 2,649- and 12,699-bit rows, with
`bbverify full` green and the frame tags unchanged. Interleaved A/B, median of
at least five rounds, with the numbers in the report.

"No improvement was available, and here is the measurement" is a real result and
should be reported as one. The short-circuit attempt above is the precedent.
