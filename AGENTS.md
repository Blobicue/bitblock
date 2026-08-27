# Using this repo as an index

Bitblock is ~52,000 lines of Rust across 33 modules, ~8,600 lines of protocol
prose, a 1,112-family metric registry, 22 independent Python vector suites, and
a Lean proof capsule. `index/` turns that into something an agent can query
instead of scan.

**The source tree is the only source of truth. Everything in `index/` is
generated — regenerate it, never hand-edit it.**

`docs/HANDOFF.md` is the project's operational handoff: what is implemented, which
claims are safe to make, and the recommended work order. Read it first. This page
covers only the index — how to query the tree, and what the index will not tell you.

## Query it

```bash
python tools/bbgraph.py ask "canonical framebuffer bytes"
```

Also: `bbverify` and `bbmutate` (see **Proving a change did not break anything**),
`dup` and `dead` (see **How compressible is this tree**),
`node <name>` (everything known about one module, tool, proof or doc),
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
| `reuse` | the module sources again | `index/reuse.json` |
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

## Proving a change did not break anything

```bash
python tools/bbverify.py quick     # 11s, per edit
python tools/bbverify.py full      # 40s, per commit
```

"Nothing broke" means something specific in this repository: **no canonical byte
moved**. 101 literal content tags are asserted across the Rust tests, and 24
Python suites rebuild those same payloads without importing any Rust, so a moved
byte fails both from opposite directions.

The two tiers split by what each detects, not by thoroughness. `quick` is
`cargo test` plus clippy -- the literal tag assertions live inside the Rust
tests, so that is what actually catches a moved byte. `full` adds fmt, the
metric registry, the index gates and the independent vectors, and runs the Rust
tests with `--features protocol-research`. The vector suites execute no Rust at
all; what they detect is the two implementations *disagreeing*, which is why
they belong to the per-commit tier.

All 58 protocol objects are guarded. The last two -- `journal/rejected-tail/v1`
and `storage-record/envelope/v1` -- had neither an independent vector nor a
literal tag until both were added, with each side asserting the same digest.

### How sensitive it actually is

```bash
python tools/bbmutate.py           # 25 modules, ~2.5 minutes
python tools/bbmutate.py frame     # before refactoring one module
```

A green suite proves nothing until it has been watched failing for the change
you care about, which is this repository's own rule. `bbmutate` performs exactly
that change and nothing else: every `to_be_bytes()` in a module's production
scope becomes `to_le_bytes()` -- a byte-order flip inside the hashed domain, no
behaviour, no error text, no control flow. If a module's canonical bytes are
pinned, this must fail.

**25 of 25 modules are killed.** That is the number that licenses refactoring:
the suite demonstrably notices a canonical byte moving in every module that
encodes one. Run it against a module before restructuring it; it takes seconds
and tells you whether the net is under you.

It found one genuine hole. `main.rs`'s only canonical encoders are three
headless local-replay policy tags, and nothing asserted their values -- all
eighteen encoding sites could be reordered with the suite still green. They are
pinned now.

The first run reported that survivor for the wrong reason, which is the more
useful lesson. It ran `cargo test --lib`, so `main.rs` was not a target at all,
and without `--features protocol-research` those three functions are not
compiled. **A mutation inside code that was never built cannot fail anything,
and reports identically to a real gap.** The harness now runs every target with
the feature enabled. The finding survived the correction; it could as easily
have been an artefact.

`bbmutate` is local-only and never runs in CI -- it rewrites `src/` and restores
it from git.

### What still is not covered

CI's rust job runs bare `cargo test` on windows-latest, so the
`all(windows, feature = "protocol-research")` modules are never compiled there.
`bbverify full` does compile them. Until CI gains a feature-enabled step, that
gap is closed locally or not at all.

## How compressible is this tree

Measured, because "make it smaller" deserves a number rather than an intuition.

```bash
python tools/bbgraph.py dup
python tools/bbgraph.py dead
```

| Avenue | Recoverable | What it costs |
|---|---:|---|
| Repeated production regions | 416 lines (1.4%) | extraction overhead eats much of it |
| Repeated test-scope regions | 341 lines (1.9%) | fixtures are also the refactor's gate |
| Public items nothing names | 4 items | nothing, but ~20 lines |
| Error enum + `Display` behind a macro | ~2,100 lines (4%) | **removes ~800 lines from `cargo fmt --check`** |
| Deleting superseded v1 codecs | ~1,800 lines | orphans recorded receipt history |

**The tree is already tight.** 37% of `src/` is tests, and the rest does not
repeat itself much. There is no large byte-preserving compression available
here, and the two large-looking levers both pay for their lines with something
this repository values more.

The first scan said otherwise, and was wrong for an instructive reason. It
reported `presentation_output` <-> `_v2` as an 87-block target, the biggest in
the crate. Most of that overlap is test fixtures: `fn qpc`, `fn bracket`,
`fn timing`. Production overlap between those two files is a third the size.
`dup` therefore counts the two scopes apart and refuses to add them.

`dup` also answers the routing half. A shared helper has to live somewhere both
sides already reach, or extracting it adds dependency edges and can cycle, and
under a cfg gate at least as permissive as the more permissive user, or a
default build ends up depending on research-gated code. That is what the DAG is
for, and it is why the `output_color` <-> `output_topology` pair routes to
`protocol` while the `presentation_output` pair routes to `display_artifact`.

`dead` reports what `clippy -D warnings` structurally cannot: a `pub` item in a
library is reachable by definition, so an unused one raises nothing. 4 items are
named nowhere in the tree and 72 are named only inside their own module. Confirm
by deleting and compiling before acting on either list -- a name can also be
reached through a trait impl or a macro, which the scan does not model.

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

**Putting the error boilerplate behind a macro.** 45 hand-written `Error` enums
and 53 `Display` impls are 3,766 lines, 7.3% of `src/`, and 38 of the impls are
exactly one `write!` per arm. A `macro_rules!` declaring the message beside the
variant would remove roughly 2,100 of those lines and colocate two things that
currently sit far apart in the same file.

It was not done, for a measured reason: **rustfmt does not format inside a macro
invocation.** A probe crate with deliberately mis-indented code inside and
outside a macro body confirmed it -- `cargo fmt` fixed the enum and left the
macro body untouched. CI enforces `cargo fmt --check`, so the conversion would
buy a 4% line reduction by moving ~800 lines outside a gate. In a repository
whose claims rest on its gates, that is the wrong direction. (The same probe
established that clippy does *not* object to `write!` with a placeholder-free
literal, so the 110 `write_str` arms are convention rather than enforcement.)

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
