# bitblock

A deterministic bit-world simulator with a content-addressed history protocol.

Every coordinate in `i64³` holds one bit. Immutable origin bedrock is on before
anything else happens, and the world evolves only through an ordered list of
`on`, `off` and `toggle` edits. Given the same initial state and the same edit
history, every derived artifact — the world, the tick outcome, the rendered
framebuffer, the receipt chain — is reproducible byte for byte.

```text
# examples/first.bb
# Immutable origin bedrock already exists when this program begins.
# Build a three-bit corner during the first tick.
on 1 0 0
on 0 1 0
on 0 0 1
```

## What is actually claimed

This repository is careful about the difference between what it computes and
what it proves, and the distinction is worth stating before the feature list.

Authority is one shape: `#bb.<kind>.<suite>.<full-digest>`, where the kind, hash
suite, schema tag, payload length and payload bytes are all inside the hashed
domain. SHA-256 is the only authority suite. A truncated tag is display-only and
cannot resolve or authorize anything.

**A digest commits to bytes. It does not prove those bytes are a valid world,
frame, transition, name, key, or consensus decision.** Those are separate
verification predicates, and `docs/PROTOCOL.md` keeps them separate.

The same discipline runs through the rest. Frame validity is layered across six
levels, and the repository is explicit that a checked `SetDIBitsToDevice` return
proves GDI accepted a complete top-down DIB call — not that the compositor used
it, and not that a monitor displayed it. The metric registry is a catalog of
what *could* be measured, not a promise to collect it, and it states that
unavailable is never zero. The Lean capsule models the smallest world core and
imports no Rust, so it makes no claim about rendering, camera arithmetic,
hashing, or scheduling.

Where a control failed, the failure is kept rather than folded away.
`docs/DISPLAY_AUDIT.md` records a rounded-corner mismatch and a composition race
alongside the eventual clean capture, with the receipt for each.

## Run it

```bash
cargo run --release
```

Headless, deterministic, no window:

```bash
cargo run --release -- --simulate-actions examples/observer_walk.tape
```

Most of the crate is research surface and is not in a default build. 18 of 33
modules sit behind `protocol-research`:

```bash
cargo test --features protocol-research
```

## Layout

| Path | What lives there |
|---|---|
| `src/` | 33 modules, ~52,000 lines. `protocol.rs` is the tag core; `world.rs` and `frame.rs` are the two things almost everything reaches. |
| `docs/PROTOCOL.md` | The boundary between what is implemented and checked locally and what would need cryptographic, custodial or distributed assumptions. |
| `docs/DISPLAY_AUDIT.md` | The Windows display assay, failed controls included. |
| `METRICS.md`, `docs/metrics/` | 1,112 metric families, each bound to an immutable definition tag. |
| `tools/` | 22 independent vector suites that reconstruct canonical payloads without importing any Rust, plus the metric-registry generator, a headless runner, and the index. |
| `proof/` | A standalone Lean 4 model of the world core, supply accounting, and quoted-byte cost. |
| `index/` | Generated. A queryable graph over everything above. |

## Finding your way around

`index/` exists so an agent — or a person returning after a month — can query
this tree instead of reading it.

```bash
python tools/bbgraph.py ask "canonical framebuffer bytes"
python tools/bbgraph.py node frame
python tools/bbgraph.py dag
python tools/bbgraph.py schema
```

Stdlib Python only; no network and no build. Every line of output cites a path,
so any claim it makes can be checked against the source that produced it.

Two things it reports that the tree cannot state about itself: which modules are
load-bearing (`protocol` is reached by 29 of 33; `world` by 23), and which
protocol objects have no independent reimplementation checking them (16 of 58).
Both are derived from the source rather than authored, so neither can quietly
rot into a wrong claim.

`AGENTS.md` documents the index, its three gates, and its limits.
`AGENT-PROMPT.md` is the same orientation on one self-contained page.

## Status

Local and single-machine. There is no network protocol, no consensus, no
signing, and no external custody. The content-addressed history is unsigned and
in-memory unless the research storage features are enabled.
