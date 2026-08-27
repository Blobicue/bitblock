# Using this repo as an index

Bitblock is ~52,000 lines of Rust across 33 modules, ~5,900 lines of protocol
prose, a 1,112-family metric registry, 22 independent Python vector suites, and
a Lean proof capsule. `index/` turns that into something an agent can query
instead of scan.

**The source tree is the only source of truth. Everything in `index/` is
generated — regenerate it, never hand-edit it.**

`AGENT-PROMPT.md` says all of this in one self-contained page, for pasting to an
agent that does not have the repo yet.

## Query it

```bash
python tools/bbgraph.py ask "canonical framebuffer bytes"
```

Also: `node <name>` (everything known about one module, tool, proof or doc),
`schema [name]` (the protocol surface, and what independently checks each
object), `metric <words>` (registry families), `dag` (dependency strata and
which modules are load-bearing), `sub [name]` (modules that move together),
`stats`, `gates`, `selftest`.

Every line of output cites a path, so any claim can be checked against the tree
that produced it.

```bash
python tools/bbgraph.py node frame
```

`node` answers the question you actually have before editing: what a module
pulls in, what breaks if you change it, which protocol objects it owns, whether
anything independently checks those objects, and **which cfg gate it lives
behind** — 18 of 33 modules do not exist in a default build, which is the single
most common way to waste an hour here.

```bash
python tools/bbgraph.py schema
```

`schema` is the coverage report the tree cannot state about itself. 58 protocol
objects, 42 with an independent Python reconstruction, **16 without**. That
number is derived, not authored: a tool that declares the identical schema
string has independently rebuilt that payload, so the uncovered set is a fact
about the tree rather than a TODO list someone forgot to update.

**How `ask` ranks.** The subsystem taxonomy is matched first, because it is
built from the schema namespaces the modules themselves declare and carries the
tree's own vocabulary rather than an editorial guess. Term matches are then
weighted by rarity *within their kind*: `protocol` appears in nearly every
module doc and discriminates nothing, `crosshair` appears in two and is
decisive. Structural rank — how many modules transitively reach a module —
orders results but never decides them, and applies only inside a subsystem the
query already matched.

That last rule is not decoration. `protocol` is reached by 29 of 33 modules;
with an unconditional load-bearing multiplier it won `ask "canonical framebuffer
bytes"` outright, over `frame`. It is the same failure a download count produces
in a curated list, and `bbselftest.json` guards it.

**Confidence.** Every `ask` ends in `CONFIDENCE high|LOW`. LOW means verify. The
tool errs toward it: matching many words is not confidence, so a query that
lands on no subsystem and no exact node name is reported LOW even when its
answer is right. `ask "canonical framebuffer bytes"` returns `frame` and is
still LOW, and that is recorded as correct behaviour rather than a bug.

## Regenerate it

```bash
python tools/bbindex.py
```

Stdlib only, no network, no build. Three stages, each printing its own numbers:

| Stage | Reads | Writes |
|---|---|---|
| `extract` | `src/*.rs`, `tools/*.py`, `docs/**`, `proof/**`, `examples/*` | `index/nodes.jsonl` |
| `link` | the extracted nodes | `index/edges.jsonl`, `index/dag.json`, `index/subsystems.json` |
| `audit` | the whole index | `index/coverage.json`, and the three gates |

Run one stage alone with `python tools/bbindex.py extract|link|audit`.

Metric records are **not** parsed here. `tools/metric_registry.py` already owns
that parse, computes the definition tags, and generates
`docs/metrics/registry.tsv`; `bbindex.py` imports it. A second parser would
drift, and the metric gate would then be checking a reimplementation instead of
the registry.

## What's committed

All of `index/` — about 1.2MB. The tree is the only input, so a fresh clone can
query it with no network and no build. Artifacts are written with LF and pinned
in `.gitattributes`; without that, autocrlf rewrites them on checkout and every
regeneration shows a whole-file diff.

## The three gates

**Accounting — catches parsing drift.** `audit` enforces, and exits non-zero on:

- every `src/*.rs` is a declared `pub mod`, or `lib.rs`/`main.rs`;
- every `crate::`/`bitblock::` target resolves to a real module;
- every schema a tool declares exists in `src/`, or is classified in
  `tools/schema_local.json`;
- `registry.tsv` rows == `metric_registry.py`'s own parse == the count
  `METRICS.md` claims (1,112), and every `#bb.metric.*` reference in any doc
  resolves to a row.

The dependency check earns its place. `crate::([a-z_]+)` — no digit — silently
truncates `frame_v2` to `frame_v` and grows a phantom node that looks entirely
plausible in the output. Resolving every target against the declared module set
is what turns that into a failure instead of a wrong graph.

**Staleness — catches an index older than the tree.** There is no CI here, so it
runs locally:

```bash
python tools/bbgraph.py gates
```

It regenerates and compares bytes. A failure leaves the tree **already
corrected** — the fix is to commit the diff, not to re-run anything.

**Answers — catches the query surface regressing.** `tools/bbselftest.json`
records what "the index works" means: twelve queries with their expected leading
module, two that must match nothing, six confidence expectations, the index
counts, three DAG shape assertions, and three schema-coverage assertions.

```bash
python tools/bbgraph.py selftest
```

> **If `selftest` fails**, decide which happened. A ranking change you did not
> intend is a bug. A source change legitimately moving an answer is not — update
> the expectation in the same commit and say why. An unexplained edit here is
> how a ranking bug gets ratified.

## Model

**Modules** are the 33 files in `src/`, each carrying its `//!` doc comment, its
public items, its test count, its cfg gate, and the schemas it declares.
Dependencies come from `crate::` and `bitblock::` paths — the binary reaches the
library through the latter, and matching only `crate::` reported `main.rs`,
5,967 lines over 22 modules, as a leaf.

**Schemas** are the canonical `bitblock/<path>/vN` strings. They are the real
spine of this repo: every content-addressed object names one inside its hashed
domain, so the schema set *is* the protocol surface.

**Subsystems** group modules by schema namespace. Note the departure from the
curated-list method this copies: there, a category's members are mutual
*alternatives*; here, subsystem membership means these modules **collaborate** —
the set to read together before editing any of them.

**Metrics** are the 1,112 registry families, each with its owning section and
the full definition tag that is its actual authority.

**Test fixtures are excluded from the protocol surface.** A schema string
declared inside a `#[cfg(test)] mod` is scaffolding. Counting them reported 81
objects and 50 covered, overstating both the surface and the gap —
`bitblock/test-object/v1` is a throwaway name that `frame.rs` and `storage.rs`
each coined in their own tests, and it was fusing them into a shared subsystem.

## Tried and corrected

Recorded because each looked obviously right until it was measured.

**Corpus-wide term rarity.** 1,112 of the 1,368 nodes are one-line metric names,
so a global count says `framebuffer` is rare mostly because metric rows do not
use it — which says nothing about whether it separates one module from another.
Counting rarity within each kind is what made `frame` win a query about the
framebuffer.

**Corpus-wide length normalisation.** Same cause, opposite symptom: a single
average document length is dragged to ~4 terms by those metric rows, every
one-line metric name then looks maximally dense, and metrics swept every query.
Per-kind normalisation fixed it.

**One blended ranked list.** Scores across kinds are not comparable — a doc
section and a schema string are not competing to be the same answer — and the
longest prose won regardless. `ask` groups its output instead, because an agent
asking about the framebuffer wants the module *and* its protocol object *and*
the prose that specifies it, each labelled.

**Keying test scope on `#[cfg(test)]`.** The boundary is the `mod`, not the
attribute. `#[cfg(test)]` also guards test-only imports, and `history.rs` has
two of them above its schema constants — so the first-attribute rule demoted
`GENESIS_SCHEMA` and `RECEIPT_SCHEMA`, both `pub`, on line 18 of a 1,424-line
file.

The generalisation: reach for the derived signal over the authored one, and
measure the diagnosis before acting on it. Three of the four above were correct
diagnoses with wrong implementations, and only the output showed it.

## Known limits — read before trusting output

1. **`index/` describes the tree, not the running system.** It is parsed from
   source text. It does not build, run, or verify anything, and a module's
   presence here is not evidence that its tests pass.
2. **16 of 58 protocol objects have no independent vectors.** They are not
   wrong, they are unchecked by any second implementation. `schema` marks them
   with `!`.
3. **Dependency edges are textual.** A `crate::foo` inside a comment or a
   `#[cfg]`-disabled block still counts. The DAG is a good map of coupling, not
   a compiler's answer.
4. **Doc-to-module edges need an explicit path mention.** Only 18 exist, because
   the prose usually names a type rather than `src/thing.rs`. Absence of an edge
   is not absence of documentation.
5. **Metric aliases are conveniences.** The definition tag is the authority, and
   a readable name is resolved through `docs/metrics/registry.tsv`. Never quote
   an alias as though it pinned a definition.
6. **The proof capsule is separate.** Lean models the smallest world core and
   imports no Rust. An indexed `proves` relation would be an assertion the
   capsule does not make.
