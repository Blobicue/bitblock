# Using this repo as an index

Bitblock is ~52,000 lines of Rust across 33 modules, ~8,600 lines of protocol
prose, a 1,112-family metric registry, 28 independent Python vector suites, and
a Lean proof capsule. `index/` turns that into something an agent can query
instead of scan.

**The source tree is the only source of truth. Everything in `index/` is
generated — regenerate it, never hand-edit it.**

> **New agent?** Cold-start order: `docs/HANDOFF.md` → `docs/AGENT_ONBOARDING.md`
> (layer map, subsystem map, chem/wasm/play entry points, hubs, hard rules) → this page (bbgraph) →
> the GitNexus block at the end of this file. bbgraph does not index `crates/core/src/chem/*.rs` submodules or
> `crates/wasm/**`; use GitNexus for those.

`docs/HANDOFF.md` is the project's operational handoff: what is implemented, which
claims are safe to make, and the recommended work order. Read it first. This page
covers only the index — how to query the tree, and what the index will not tell you.

## Query it

```bash
python tools/bbgraph.py ask "canonical framebuffer bytes"
```

Also: `bbverify` and `bbmutate` (see **Proving a change did not break anything**),
`dup` and `dead` (see **How compressible is this tree**),
`node <name>` (everything known about one module, tool, proof, doc, or
`#bb.<kind>.sha256.<digest>` content tag; `ask` also resolves content tags
directly), `sym <name>` (one symbol or `Type::method`: span, signature, doc,
and types), `type <Type>` (where a type is defined, constructed, implemented,
and consumed), `returns <Type>` and `takes <Type>` (signature type index),
`callers <target>` (production and `#[test]` callers with crate-aware `cargo
test` commands), `impact <modules...>` (4-tier blast radius and verification
recipe), `wave <modules...>` (parallel subagent execution waves across `deps` +
`test_deps`), `schema [name]` (the protocol surface, and what independently
checks each object), `metric <words>` (registry families), `dag` (dependency
strata and which modules are load-bearing), `sub [name]` (modules that move
together), `coverage` (`//!` and public `///` documentation coverage), `eval`
(self-supervised holdout retrieval benchmark), `html [path]` (standalone
interactive HTML GraphRAG and DAG observatory compiler), `stats`, `gates`,
`selftest`.

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

`schema` is the coverage report the tree cannot state about itself. 58 protocol objects, every one with an independent Python reconstruction. That
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
python tools/bbverify.py quick     # per edit
python tools/bbverify.py full      # per commit
python tools/bbverify.py list      # what each check reads
```

"Nothing broke" means something specific in this repository: **no canonical byte
moved**. 101 literal content tags are asserted across the Rust tests, and 24
Python suites rebuild those same payloads without importing any Rust, so a moved
byte fails both from opposite directions.

| what changed | full | quick |
|---|---:|---:|
| nothing | 0.1s | 0.0s |
| a document | 1.0s | 0.0s |
| a source file | 6.2s | 5.1s |
| nothing, but `--force` | 1.7s | 0.7s |

After a source edit the cost is the Rust toolchain, not the checks: a rebuild is
5.2s and clippy another 3.2s, against 0.5s to run all 413 tests. Clippy is given
its own `CARGO_TARGET_DIR` and started alongside the build rather than after it,
because sharing one directory makes the two cargo invocations queue on the same
lock -- 7.1s against 4.7s when they genuinely overlap, for ~158MB inside an
already-ignored `target/`.

`[profile.dev] opt-level = 3` is why the test half is small. At opt-level 0 one
headless replay of a five-record tape took 1.56s; optimised it takes 0.06s, and
every slow test in the profile was slow for that one reason. Optimising also
makes the *rebuild* faster, which is not obvious -- unoptimised builds produce
27MB binaries and the edit path is link-bound. Measured edit-to-answer: 11.1s at
opt 0, 6.3s at 1, 5.7s at 2, 5.3s at 3; only the cold build pays, 39.6s against
31.0s. `debug-assertions` and `overflow-checks` stay on, and Rust does not
enable fast-math at any level, so the f64 camera geometry is bit-identical.

Those are steady-state figures. Alternating the tiers costs roughly two extra
seconds on the first run after the switch, because they use different feature
sets and each invalidates the other's build profile; repeating one tier is the
fast path.

Three things get it there.

**Every check declares what it reads.** Editing `docs/PROTOCOL.md` cannot change
what `cargo test` does, so a check whose inputs are byte-identical to the last
run that passed is skipped instead of repeated -- the same content-addressing
the crate itself is built on. `bbverify list` prints the input set of each.

**One build, then everything at once.** `cargo test` compiles three test
binaries and walks them one after another -- 2.5s + 2.6s + 2.3s of mostly idle
CPU on sixteen cores. `cargo test --no-run` costs 0.4s warm and says where they
are, so they run concurrently with clippy and the Python work. A tier costs its
slowest single job rather than the sum.

**The cache is conservative, and that matters more than the speed.** It records
only checks that passed, it keys on `bbverify.py`'s own bytes as well as the
subject's, and an unreadable input makes it run rather than skip. It was watched
doing the right thing in each case: a failing tree fails again on the next run
rather than being skipped, a `touch` with no content change correctly skips, and
a byte-order flip in `frame.rs` still fails 28 tests. A stale skip would be a
silent false green, which is worse than any amount of waiting.

The two tiers split by what each detects, not by thoroughness. `quick` is
`cargo test` plus clippy -- the literal tag assertions live inside the Rust
tests, so that is what actually catches a moved byte. `full` adds fmt, the
metric registry, the index gates and the independent vectors, and runs the Rust
tests with `--features protocol-research`. The vector suites execute no Rust at
all; what they detect is the two implementations *disagreeing*, which is why
they belong to the per-commit tier.

**How many tests ran is itself a gate.** The workspace split made `cargo test`
run only the root package -- 43 of 397 -- and every check stayed green,
including CI, because each one asked "did what ran pass" and none asked "did
everything run". A suite that quietly stops running is indistinguishable from a
suite that passes. `bbverify` now records a floor per tier and fails below it,
and both it and CI use `--workspace`.

**All 58 protocol objects now have an independent vector suite**, closed by two
agents working disjoint halves of the tree. That number was wrong here before
they ran: this page claimed every object was guarded while the index said 44 of
58, and the agents flagged it rather than quietly matching the prose.

Six of the seven core-crate objects already had literal tags in Rust and no
second implementation behind them -- a program agreeing with itself -- and every
one reproduced from the Python side unchanged. `sidebar/composite-frame-input/v1`
had no literal anywhere: it hashes under kind `frame-input`, the same kind as
the inner game input it contains, so grepping for its kind found the wrong
object. `LocalReplayBuild::tag` was likewise unchecked, because the existing
carrier suite spelled its two fields inline through a different encoder.

### Where the time goes

```bash
python tools/bbtime.py            # every test, attributed to its module
```

Bottlenecks should surface themselves rather than be guessed at. Every speedup
here was found by measuring and every one was somewhere other than where it
looked: the index's 18s was a per-item regex sweep, not the obvious string
joining; the full tier's 8s was one test hashing a 27MB executable six times;
and a 20s regression turned out to be the vector sweep running the profiler
itself, which is only visible because each job now reports its own seconds.

Cost lands on the same module nodes `bbgraph dag` ranks, so "what does this
module cost to verify" and "what depends on it" are answerable together. Results
go to `target/bbtiming.json` -- a measurement of one machine on one day, not a
property of the tree, so it stays out of `index/`.

Two cautions. Absolute numbers are inflated by contention under the default
8-way profiling (`--serial` avoids that at several times the wall clock), and
the ranking is what to trust: a gap under about 20% between two rows is not a
real difference.

What sets the wall clock is not the total. Summed test cost is ~78s while a tier
costs about four seconds, because the binaries run concurrently and libtest
parallelises within each. The floor is the *slowest single binary*, so the thing
worth optimising is always the longest pole, never the largest sum.

### The workspace

```
crates/core/       bitblock-core       13 modules, no dependencies
crates/research/   bitblock-research   18 modules, depends on core
src/               bitblock            the binary, plus a facade lib
```

The module DAG always said `bitblock-research` was downstream of everything;
now cargo knows it too, and the boundary is the compiler's rather than a
convention's. The split was possible because the dependency direction was
already one-way -- no default module reached a research module, not even in
test scope -- and it cost five `pub(crate)` promotions plus `PreparedTick`.

`bitblock-research` re-exports core's modules, so `crate::protocol::ContentTag`
resolves inside it exactly as before. That was deliberate: the point was to give
the compiler the graph, not to rewrite 84 import paths through a byte-exact
codec and hope no transcription error slipped in. `src/lib.rs` is a facade over
both, so the binary, the examples and the integration test are untouched.

**Every one of the 106 literal content tags is byte-identical across the split,**
and all 397 tests pass. That was the acceptance test, and it is why the split was
attemptable at all.

### What it costs to touch a module

```bash
python tools/bbtime.py --build
```

Once the tests take half a second, verification *is* the Rust toolchain, and
this is the part `bbgraph dag` cannot see:

Before the workspace split the compiler charged almost the same wherever an
edit landed, because the crate was one compilation unit and the module DAG was
not a build boundary. `coordinate_episode`, a leaf nothing depends on, cost 7.9s
to touch -- more than `protocol`, which 29 of 33 modules reach.

After the split, over all 32 modules:

| | before | after |
|---|---:|---:|
| mean touch cost | ~6.2s | **4.9s** |
| modules with >= 10 dependents | — | 5.9s |
| modules with 0 dependents | — | 4.3s |

The mean improved by about a fifth, which is less than the 2x the estimate
suggested. The shape matters more than the mean: cost now tracks blast radius,
where before a leaf and a hub cost the same. `coordinate_episode` went 7.9s to
3.2s and `world` 8.0s to 3.9s.

It is not uniformly better, and pretending otherwise would be dishonest.
`camera` went 5.4s to 8.5s and `protocol` 6.4s to 7.8s, because editing the most
upstream core module now rebuilds four crates in a chain -- core, research,
facade, binary -- that cargo cannot overlap. Making the hubs more expensive and
the leaves cheaper is the correct direction, but it is a trade rather than a
free win.

What is *not* left: incremental is already on (turning it off costs 17-20s),
codegen-units is already at the dev default of 256 and lowering it is worse, and
`debug = 0` buys about 10% while removing line numbers from every panic. Those
were measured, not assumed.

### How sensitive it actually is

```bash
python tools/bbmutate.py           # 25 modules, a few minutes
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
2. **A vector proves agreement, not correctness.** Every protocol object now
   has an independent Python reconstruction, so the two implementations pin each
   other -- but both could encode the same wrong thing. What a vector rules out
   is one side drifting, which is the failure that actually happens. Note also
   that `overlay-policy/crosshair/v1` has no directly asserted Rust literal: it
   is pinned transitively, because the headless replay-rules tag names it by
   digest and `main.rs` asserts that. Real, but weaker than a direct assertion.
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

<!-- gitnexus:start -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **bitblock** (9876 symbols, 35289 relationships, 882 execution flows).

> Index stale? Run `node .gitnexus/run.cjs analyze --index-only` from the project root — it auto-selects an available runner. No `.gitnexus/run.cjs` yet? Bootstrap with `npx`, `bunx`, or `pnpm dlx` — e.g. `bunx gitnexus@latest analyze` (npm 11 npx crash; #1939).

## Always Do

- **MUST run impact before editing.** Use `impact({target: "symbolName", direction: "upstream"})` or `node .gitnexus/run.cjs impact "symbolName" --direction upstream --repo .`; report callers, processes, and risk. Never substitute grep for graph analysis.
- **MUST analyze graph changes before committing.** Use `detect_changes({scope: "all"})` (MCP) or `node .gitnexus/run.cjs detect-changes --scope all --repo .` (CLI fallback). `partial: true` or `truncated: true` is not a clean check — a zero means unseen, not unaffected; re-run it. For regression review: `detect_changes({scope: "compare", base_ref: "main"})` or `node .gitnexus/run.cjs detect-changes --scope compare --base-ref "main" --repo .`.
- MUST warn on HIGH/CRITICAL `risk` pre-edit; never use `riskSharedAxes` to waive a HIGH/CRITICAL `risk` warning. Compare File/symbol: MCP File omits axes; Graph-RAG expands File.
- **MUST treat `risk: UNKNOWN` as unresolved, not as low.** An empty caller set is not evidence the symbol is unused — it can also mean the callers are not resolvable by the index (plain-object property access, dynamic dispatch, cross-language calls). `impact` pairs `UNKNOWN` with a `riskNote` saying so. Confirm with a text search before treating the symbol as safe to change or delete; do not proceed on the strength of a zero.
- **MUST use `query({search_query: "concept"})` for concepts/flows, `context({name: "symbolName"})` for a named symbol, or `impact` for blast radius, on read-only callers, dependencies, imports, or execution flow.** Graph first; text search only for empty/`UNKNOWN`/literals.
- For security review, `explain({target: "fileOrSymbol"})` lists taint findings (source→sink flows; needs `analyze --pdg`).

## Never Do

- NEVER edit a function, class, or method before MCP/CLI impact analysis.
- NEVER ignore HIGH or CRITICAL risk warnings from impact analysis, and never read `UNKNOWN` as an all-clear — it means the walk could not answer, which is the one verdict that requires confirming by other means.
- NEVER rename symbols with find-and-replace — use `rename` which understands the call graph.
- NEVER commit before MCP/CLI graph change analysis.

## Resources

| Resource | Use for |
| --- | --- |
| `gitnexus://repo/bitblock/context` | Codebase overview, check index freshness |
| `gitnexus://repo/bitblock/clusters` | All functional areas |
| `gitnexus://repo/bitblock/processes` | All execution flows |
| `gitnexus://repo/bitblock/process/{name}` | Step-by-step execution trace |

## CLI

| Task | Read this skill file |
| --- | --- |
| Understand architecture / "How does X work?" | `.claude/skills/gitnexus-exploring/SKILL.md` |
| Blast radius / "What breaks if I change X?" | `.claude/skills/gitnexus-impact-analysis/SKILL.md` |
| Trace bugs / "Why is X failing?" | `.claude/skills/gitnexus-debugging/SKILL.md` |
| Rename / extract / split / refactor | `.claude/skills/gitnexus-refactoring/SKILL.md` |
| Tools, resources, schema reference | `.claude/skills/gitnexus-guide/SKILL.md` |
| Index, status, clean, wiki CLI commands | `.claude/skills/gitnexus-cli/SKILL.md` |
| Work in the Tools area (697 symbols) | `.claude/skills/gitnexus-area-tools/SKILL.md` |
| Work in the Chem area (335 symbols) | `.claude/skills/gitnexus-area-chem/SKILL.md` |
| Work in the Examples area (246 symbols) | `.claude/skills/gitnexus-area-examples/SKILL.md` |
| Work in the Cluster_71 area (69 symbols) | `.claude/skills/gitnexus-area-cluster-71/SKILL.md` |
| Work in the Cluster_0 area (63 symbols) | `.claude/skills/gitnexus-area-cluster-0/SKILL.md` |
| Work in the Cluster_59 area (63 symbols) | `.claude/skills/gitnexus-area-cluster-59/SKILL.md` |
| Work in the Pages area (47 symbols) | `.claude/skills/gitnexus-area-pages/SKILL.md` |
| Work in the Cluster_55 area (45 symbols) | `.claude/skills/gitnexus-area-cluster-55/SKILL.md` |
| Work in the Cluster_131 area (41 symbols) | `.claude/skills/gitnexus-area-cluster-131/SKILL.md` |
| Work in the Cluster_56 area (37 symbols) | `.claude/skills/gitnexus-area-cluster-56/SKILL.md` |
| Work in the Cluster_163 area (36 symbols) | `.claude/skills/gitnexus-area-cluster-163/SKILL.md` |
| Work in the Cluster_107 area (35 symbols) | `.claude/skills/gitnexus-area-cluster-107/SKILL.md` |
| Work in the Cluster_74 area (25 symbols) | `.claude/skills/gitnexus-area-cluster-74/SKILL.md` |
| Work in the Cluster_209 area (25 symbols) | `.claude/skills/gitnexus-area-cluster-209/SKILL.md` |
| Work in the Cluster_64 area (24 symbols) | `.claude/skills/gitnexus-area-cluster-64/SKILL.md` |
| Work in the Cluster_244 area (23 symbols) | `.claude/skills/gitnexus-area-cluster-244/SKILL.md` |
| Work in the Cluster_73 area (22 symbols) | `.claude/skills/gitnexus-area-cluster-73/SKILL.md` |
| Work in the Cluster_237 area (22 symbols) | `.claude/skills/gitnexus-area-cluster-237/SKILL.md` |
| Work in the Cluster_69 area (21 symbols) | `.claude/skills/gitnexus-area-cluster-69/SKILL.md` |
| Work in the Cluster_146 area (21 symbols) | `.claude/skills/gitnexus-area-cluster-146/SKILL.md` |

<!-- gitnexus:end -->

See `docs/AGENT_ONBOARDING.md` for when to use GitNexus and when to use bbgraph,
plus safe first queries. The block above is regenerated by `gitnexus analyze`, so keep hand edits outside its markers.
