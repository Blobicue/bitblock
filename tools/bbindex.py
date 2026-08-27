#!/usr/bin/env python3
"""bbindex.py -- build index/ from the Bitblock source tree.

The tree is the only source of truth. Everything under index/ is generated from
it and must never be hand-edited; run this and commit the diff instead.

Stdlib only, no network, no build. Three stages, each printing its own numbers:

    extract   src/*.rs, tools/*.py, docs/**, proof/**, examples/*
              -> index/nodes.jsonl
    link      the extracted nodes
              -> index/edges.jsonl, index/dag.json, index/subsystems.json
    reuse     the module sources again, for repetition and dead surface
              -> index/reuse.json
    audit     the whole index
              -> index/coverage.json  (and the three gates)

    python tools/bbindex.py            # all three
    python tools/bbindex.py extract    # one stage

Metric records are not parsed here. tools/metric_registry.py already owns that
parse, computes the definition tags, and writes docs/metrics/registry.tsv; a
second parser would drift from it and then the accounting gate would be checking
this file's reimplementation rather than the registry. This module imports it.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
INDEX = ROOT / "index"

SRC = ROOT / "src"
TOOLS = ROOT / "tools"
DOCS = ROOT / "docs"
PROOF = ROOT / "proof"
EXAMPLES = ROOT / "examples"
TESTS = ROOT / "tests"
REGISTRY_TSV = DOCS / "metrics" / "registry.tsv"

# Markdown that is prose about the system rather than generated data.
DOC_FILES = (
    ROOT / "README.md",
    ROOT / "METRICS.md",
    DOCS / "PROTOCOL.md",
    DOCS / "DISPLAY_AUDIT.md",
    DOCS / "METRIC_EXHAUSTION_AUDIT.md",
    DOCS / "SEMANTICS.md",
    DOCS / "TRANSCRIPT_AUDIT.md",
    DOCS / "LAYER1_DESIGN_PAPER.md",
    DOCS / "LAYER1_DESIGN_AUDIT.md",
    DOCS / "HANDOFF.md",
    DOCS / "TIMELINES.md",
    DOCS / "GALLERY.md",
    DOCS / "metrics" / "world.md",
    DOCS / "metrics" / "view.md",
    DOCS / "metrics" / "system.md",
    PROOF / "README.md",
)

# `crate::foo` and `bitblock::foo` are the same edge. The binary reaches the
# library through its public name rather than `crate::`, so matching only the
# latter reports main.rs -- 5,967 lines over 22 modules -- as a leaf that
# depends on nothing.
#
# The digit is load-bearing too: without it `frame_v2` truncates to `frame_v`,
# which is not a module, and the graph silently grows a phantom node. Gate A
# re-checks every target against the declared module set for exactly that.
RE_CRATE_DEP = re.compile(r"(?:crate|bitblock)::([a-z0-9_]+)")
RE_PUB_MOD = re.compile(r"^\s*pub mod ([a-z0-9_]+);", re.M)
RE_CFG = re.compile(r'^#\[cfg\((.+)\)\]\s*$')
RE_SCHEMA_LIT = re.compile(r'b"(bitblock/[a-z0-9/_-]+)"')
RE_CONST_SCHEMA = re.compile(
    r'const ([A-Z][A-Z0-9_]*)\s*:\s*&\[u8\]\s*=\s*\n?\s*b"(bitblock/[a-z0-9/_-]+)"'
)
RE_PUB_ITEM = re.compile(
    r"^\s*pub (?:fn|struct|enum|const|trait|type) ([A-Za-z_][A-Za-z0-9_]*)", re.M
)
RE_TEST = re.compile(r"#\[test\]")
RE_HEADING = re.compile(r"^(#{1,6}) +(.+?)\s*$", re.M)
RE_METRIC_REF = re.compile(r"#bb\.metric\.[a-z0-9-]+\.[a-z0-9-]+")
RE_TAG_KIND = re.compile(r"#bb\.([a-z0-9-]+)\.")
RE_SRC_CITE = re.compile(r"`?(src/[a-z0-9_]+\.rs)`?")
RE_IDENT = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
RE_LEAN_DECL = re.compile(r"^(theorem|lemma|def|structure|inductive) ([^\s:]+)", re.M)

STOP = {
    "the", "a", "an", "and", "or", "of", "to", "in", "is", "it", "that", "this",
    "for", "on", "as", "by", "with", "not", "but", "are", "be", "was", "its",
    "from", "at", "one", "no", "any", "every", "must", "may", "can", "does",
    "which", "when", "than", "then", "into", "only", "each", "same", "other",
    "their", "them", "they", "there", "here", "have", "has", "had", "were",
    "will", "would", "should", "could", "rather", "still", "also", "both",
    "over", "under", "after", "before", "because", "so", "if", "do", "done",
    "what", "how", "why", "who", "all", "some", "such", "own", "more", "most",
    "bitblock", "v1", "v2", "v3",
}


def tokens(text: str) -> list[str]:
    words = re.findall(r"[a-z0-9]+", (text or "").lower())
    return [w for w in words if len(w) > 2 and w not in STOP]


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8", errors="replace")


def rel(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


# --------------------------------------------------------------------------
# extract
# --------------------------------------------------------------------------


def module_gate(lib_text: str) -> dict[str, str]:
    """Map module name -> the cfg predicate lib.rs declares it under.

    A module's availability is a fact an agent needs before editing it: two
    thirds of this crate is behind `protocol-research` and does not exist in a
    default build.
    """
    gates: dict[str, str] = {}
    pending: str | None = None
    for line in lib_text.splitlines():
        cfg = RE_CFG.match(line.strip())
        if cfg:
            pending = cfg.group(1)
            continue
        mod = re.match(r"^\s*pub mod ([a-z0-9_]+);", line)
        if mod:
            gates[mod.group(1)] = pending or "default"
            pending = None
    return gates


def summarize_doc_comment(text: str) -> tuple[str, str]:
    """Return (first sentence, full //! block) of a Rust module doc comment."""
    lines: list[str] = []
    for line in text.splitlines():
        if line.startswith("//!"):
            lines.append(line[3:].strip())
        elif lines:
            break
        elif line.strip() and not line.startswith("//"):
            break
    body = " ".join(x for x in lines if x).strip()
    first = body.split(". ")[0].strip()
    if first and not first.endswith("."):
        first += "."
    return first, body


def test_scope_line(text: str) -> int:
    """Line at which a file's `#[cfg(test)] mod` block begins, else unbounded.

    Schemas declared past it are fixtures, not protocol surface. Without this
    distinction `bitblock/test-object/v1` -- a throwaway name that frame.rs and
    storage.rs each coined in their own tests -- becomes a shared subsystem
    joining two unrelated modules, and the count of protocol objects lacking
    independent vectors is inflated with objects that should not have any.

    The boundary is the `mod`, not the attribute. `#[cfg(test)]` also guards
    test-only imports and helpers: history.rs carries two such `use` lines at
    the top, and treating the first attribute as the boundary demoted all four
    of its real schema constants -- GENESIS_SCHEMA and RECEIPT_SCHEMA among them,
    both `pub` -- to fixtures on line 18 of a 1,424-line file.
    """
    lines = text.splitlines()
    for num, line in enumerate(lines, 1):
        if line.strip() != "#[cfg(test)]":
            continue
        for follower in lines[num:]:
            if not follower.strip():
                continue
            if follower.lstrip().startswith("mod "):
                return num
            break
    return 1 << 30


def schema_declarations(text: str, path: Path) -> list[dict]:
    """Every canonical `bitblock/...` schema string a Rust file declares.

    Constants are recorded with their Rust name; bare literals (inline domains)
    are recorded without one. Both are real occurrences, and Gate B needs to
    tell them apart.
    """
    out: list[dict] = []
    named: dict[str, str] = {}
    for match in RE_CONST_SCHEMA.finditer(text):
        named[match.group(2)] = match.group(1)
    boundary = test_scope_line(text)
    seen: set[str] = set()
    for num, line in enumerate(text.splitlines(), 1):
        for match in RE_SCHEMA_LIT.finditer(line):
            schema = match.group(1)
            if schema in seen:
                continue
            seen.add(schema)
            out.append(
                {
                    "schema": schema,
                    "const": named.get(schema),
                    "at": "%s:%d" % (rel(path), num),
                    "in_tests": num > boundary,
                }
            )
    return out


def extract_modules() -> list[dict]:
    lib_text = read(SRC / "lib.rs")
    gates = module_gate(lib_text)
    nodes = []
    for path in sorted(SRC.glob("*.rs")):
        name = path.stem
        text = read(path)
        summary, doc = summarize_doc_comment(text)
        # Dependencies are read from PRODUCTION scope only. A `use crate::world::Coord`
        # inside `mod tests` is a real import but not a real coupling: it does not
        # constrain where shared code may live, and counting it lets a test fixture
        # move a module's stratum and its load-bearing rank. Adding one such import to
        # storage.rs's tests moved `world` from 23 transitive dependents to 25.
        lines = text.splitlines()
        edge = min(test_scope_line(text), len(lines))
        production = "\n".join(lines[:edge])
        behind_tests = "\n".join(lines[edge:])
        deps = sorted({d for d in RE_CRATE_DEP.findall(production) if d != name})
        test_deps = sorted(
            {d for d in RE_CRATE_DEP.findall(behind_tests) if d != name} - set(deps)
        )
        pub_items = sorted(set(RE_PUB_ITEM.findall(text)))
        schemas = schema_declarations(text, path)
        if name == "lib":
            gate = "default"
        elif name == "main":
            gate = "binary"
        else:
            gate = gates.get(name, "undeclared")
        terms = sorted(
            set(
                tokens(name.replace("_", " "))
                + tokens(doc)
                + tokens(" ".join(pub_items))
                + tokens(" ".join(s["schema"].replace("/", " ").replace("-", " ") for s in schemas))
            )
        )
        nodes.append(
            {
                "id": "module:" + name,
                "kind": "module",
                "name": name,
                "path": rel(path),
                "summary": summary,
                "doc": doc,
                "lines": text.count("\n") + 1,
                "pub_items": pub_items,
                "tests": len(RE_TEST.findall(text)),
                "gate": gate,
                "deps": deps,
                "test_deps": test_deps,
                "declares": [s["schema"] for s in schemas],
                "declared_at": {s["schema"]: s["at"] for s in schemas},
                "test_scope": sorted(s["schema"] for s in schemas if s["in_tests"]),
                "schema_consts": {s["schema"]: s["const"] for s in schemas if s["const"]},
                "terms": terms,
            }
        )
    return nodes


def extract_tools() -> list[dict]:
    nodes = []
    for path in sorted(TOOLS.glob("*.py")):
        if path.name in ("bbindex.py", "bbgraph.py"):
            continue
        text = read(path)
        doc = ""
        match = re.match(r'^#!.*\n"""(.*?)"""', text, re.S)
        if match:
            doc = " ".join(match.group(1).split())
        schemas = sorted({m.group(1) for m in RE_SCHEMA_LIT.finditer(text)})
        first = doc.split(". ")[0].strip()
        if first and not first.endswith("."):
            first += "."
        nodes.append(
            {
                "id": "tool:" + path.stem,
                "kind": "tool",
                "name": path.stem,
                "path": rel(path),
                "summary": first,
                "doc": doc,
                "lines": text.count("\n") + 1,
                "schemas": schemas,
                "terms": sorted(
                    set(tokens(path.stem.replace("_", " ")) + tokens(doc))
                ),
            }
        )
    return nodes


def extract_docs() -> list[dict]:
    """One node per `##`/`###` section, so a citation lands on a section."""
    nodes = []
    for path in DOC_FILES:
        if not path.exists():
            continue
        text = read(path)
        lines = text.splitlines()
        marks = [
            (num, len(m.group(1)), m.group(2))
            for num, line in enumerate(lines, 1)
            for m in [RE_HEADING.match(line)]
            if m
        ]
        for pos, (num, depth, title) in enumerate(marks):
            if depth > 3:
                continue
            end = marks[pos + 1][0] - 1 if pos + 1 < len(marks) else len(lines)
            body = "\n".join(lines[num:end])
            slug = re.sub(r"[^a-z0-9]+", "-", title.lower()).strip("-")
            nodes.append(
                {
                    "id": "doc:%s#%s" % (rel(path), slug),
                    "kind": "doc",
                    "name": title,
                    "path": rel(path),
                    "line": num,
                    "depth": depth,
                    "document": path.stem,
                    "summary": " ".join(body.split())[:220],
                    "cites_src": sorted(set(RE_SRC_CITE.findall(body))),
                    "metric_refs": sorted(set(RE_METRIC_REF.findall(body))),
                    "tag_kinds": sorted(set(RE_TAG_KIND.findall(body))),
                    "terms": sorted(set(tokens(title) + tokens(body))),
                }
            )
    return nodes


def extract_proofs() -> list[dict]:
    nodes = []
    for path in sorted(PROOF.rglob("*.lean")):
        if ".lake" in path.parts:
            continue
        text = read(path)
        decls = RE_LEAN_DECL.findall(text)
        theorems = sorted({n for k, n in decls if k in ("theorem", "lemma")})
        defs = sorted({n for k, n in decls if k not in ("theorem", "lemma")})
        nodes.append(
            {
                "id": "proof:" + rel(path),
                "kind": "proof",
                "name": path.stem,
                "path": rel(path),
                "lines": text.count("\n") + 1,
                "theorems": theorems,
                "defs": defs,
                "summary": "%d theorems, %d definitions." % (len(theorems), len(defs)),
                "terms": sorted(
                    set(
                        tokens(path.stem)
                        + tokens(" ".join(theorems + defs).replace("_", " "))
                    )
                ),
            }
        )
    return nodes


def extract_examples() -> list[dict]:
    nodes = []
    paths = sorted(EXAMPLES.glob("*")) + sorted(TESTS.glob("*.rs"))
    for path in paths:
        if not path.is_file():
            continue
        text = read(path)
        schemas = sorted({m.group(1) for m in RE_SCHEMA_LIT.finditer(text)})
        deps = sorted(set(re.findall(r"bitblock::([a-z0-9_]+)", text)))
        nodes.append(
            {
                "id": "example:" + rel(path),
                "kind": "example",
                "name": path.name,
                "path": rel(path),
                "lines": text.count("\n") + 1,
                "uses_modules": deps,
                "schemas": schemas,
                "summary": " ".join(text.splitlines()[0:1])[:160],
                "terms": sorted(set(tokens(path.stem.replace("_", " ")))),
            }
        )
    return nodes


def extract_metrics() -> list[dict]:
    """Metric nodes come from docs/metrics/registry.tsv, which
    tools/metric_registry.py generates. Never re-parse the definitions here."""
    if not REGISTRY_TSV.exists():
        return []
    nodes = []
    for line in read(REGISTRY_TSV).splitlines():
        if not line.strip() or line.startswith("#bb.") is False and line.startswith("#"):
            continue
        parts = line.split("\t")
        if len(parts) != 4:
            continue
        alias, tag, owner, name = parts
        nodes.append(
            {
                "id": "metric:" + alias,
                "kind": "metric",
                "name": name,
                "alias": alias,
                "tag": tag,
                "owner": owner,
                "path": "docs/metrics/%s.md" % owner,
                "summary": name,
                "terms": sorted(
                    set(tokens(name) + tokens(alias.split(".", 3)[-1].replace("-", " ")))
                ),
            }
        )
    return nodes


def extract_schemas(modules: list[dict], tools: list[dict]) -> list[dict]:
    """Schema nodes are the protocol surface, keyed by the canonical string.

    `vectored_by` is derived, not authored: a Python tool that declares the
    identical schema string has independently reconstructed that payload. That
    is what makes the uncovered set a real coverage report rather than a TODO
    list someone forgot to update.
    """
    by_schema: dict[str, dict] = {}
    for mod in modules:
        for schema in mod["declares"]:
            entry = by_schema.setdefault(
                schema,
                {
                    "id": "schema:" + schema,
                    "kind": "schema",
                    "name": schema,
                    "namespace": schema.split("/")[1],
                    "version": schema.rsplit("/", 1)[-1],
                    "declared_by": [],
                    "declared_at": [],
                    "consts": [],
                    "vectored_by": [],
                    "fixture": True,
                },
            )
            entry["declared_by"].append(mod["id"])
            entry["declared_at"].append(mod["declared_at"][schema])
            # A schema is protocol surface if ANY module declares it outside
            # test scope. One module using a name in its tests does not demote
            # an object another module publishes.
            if schema not in mod["test_scope"]:
                entry["fixture"] = False
            const = mod["schema_consts"].get(schema)
            if const:
                entry["consts"].append(const)
    for tool in tools:
        for schema in tool["schemas"]:
            if schema in by_schema:
                by_schema[schema]["vectored_by"].append(tool["id"])
    out = []
    for schema in sorted(by_schema):
        entry = by_schema[schema]
        for key in ("declared_by", "declared_at", "consts", "vectored_by"):
            entry[key] = sorted(set(entry[key]))
        entry["summary"] = "%s, declared by %s%s" % (
            entry["version"],
            ", ".join(m.split(":", 1)[1] for m in entry["declared_by"]) or "nothing",
            " (test fixture)" if entry["fixture"]
            else "" if entry["vectored_by"] else ", NO independent vectors",
        )
        entry["terms"] = sorted(set(tokens(schema.replace("/", " ").replace("-", " "))))
        out.append(entry)
    return out


def stage_extract() -> list[dict]:
    modules = extract_modules()
    tools = extract_tools()
    schemas = extract_schemas(modules, tools)
    nodes = (
        modules
        + tools
        + schemas
        + extract_docs()
        + extract_metrics()
        + extract_proofs()
        + extract_examples()
    )
    nodes.sort(key=lambda n: (n["kind"], n["id"]))
    write_jsonl(INDEX / "nodes.jsonl", nodes)
    counts: dict[str, int] = {}
    for node in nodes:
        counts[node["kind"]] = counts.get(node["kind"], 0) + 1
    print("extract  %d nodes" % len(nodes))
    for kind in sorted(counts):
        print("    %-9s %5d" % (kind, counts[kind]))
    return nodes


# --------------------------------------------------------------------------
# link
# --------------------------------------------------------------------------


def strata(adjacency: dict[str, list[str]]) -> dict[str, int]:
    """Longest-path depth. Stratum 0 depends on nothing else in the crate."""
    depth: dict[str, int] = {}
    visiting: set[str] = set()

    def walk(node: str) -> int:
        if node in depth:
            return depth[node]
        if node in visiting:
            return 0  # a cycle; Rust modules may legitimately be mutually visible
        visiting.add(node)
        best = 0
        for child in adjacency.get(node, []):
            best = max(best, walk(child) + 1)
        visiting.discard(node)
        depth[node] = best
        return best

    for node in adjacency:
        walk(node)
    return depth


def transitive_dependents(adjacency: dict[str, list[str]]) -> dict[str, int]:
    """How many modules reach each module. The adoption analogue: it ORDERS
    results inside a matched group and never decides across groups, because
    `protocol` is reachable from nearly everything and would win every query."""
    reverse: dict[str, set[str]] = {n: set() for n in adjacency}
    for node, children in adjacency.items():
        for child in children:
            reverse.setdefault(child, set()).add(node)
    counts: dict[str, int] = {}
    for target in reverse:
        seen: set[str] = set()
        stack = list(reverse.get(target, ()))
        while stack:
            node = stack.pop()
            if node in seen:
                continue
            seen.add(node)
            stack.extend(reverse.get(node, ()))
        counts[target] = len(seen)
    return counts


def stage_link(nodes: list[dict]) -> list[dict]:
    by_id = {n["id"]: n for n in nodes}
    modules = [n for n in nodes if n["kind"] == "module"]
    module_names = {m["name"] for m in modules}
    edges: list[dict] = []

    for mod in modules:
        for dep in mod["deps"]:
            if dep in module_names:
                edges.append(
                    {"from": mod["id"], "to": "module:" + dep, "kind": "depends_on"}
                )
        for schema in mod["declares"]:
            edges.append(
                {"from": mod["id"], "to": "schema:" + schema, "kind": "declares"}
            )

    for tool in (n for n in nodes if n["kind"] == "tool"):
        for schema in tool["schemas"]:
            if "schema:" + schema in by_id:
                edges.append(
                    {"from": tool["id"], "to": "schema:" + schema, "kind": "vectors"}
                )

    for doc in (n for n in nodes if n["kind"] == "doc"):
        for path in doc["cites_src"]:
            name = Path(path).stem
            if name in module_names:
                edges.append(
                    {"from": doc["id"], "to": "module:" + name, "kind": "documents"}
                )
        for ref in doc["metric_refs"]:
            if "metric:" + ref in by_id:
                edges.append({"from": doc["id"], "to": "metric:" + ref, "kind": "cites"})

    for ex in (n for n in nodes if n["kind"] == "example"):
        for name in ex["uses_modules"]:
            if name in module_names:
                edges.append({"from": ex["id"], "to": "module:" + name, "kind": "uses"})

    edges.sort(key=lambda e: (e["kind"], e["from"], e["to"]))
    write_jsonl(INDEX / "edges.jsonl", edges)

    adjacency = {
        m["name"]: sorted(d for d in m["deps"] if d in module_names) for m in modules
    }
    depth = strata(adjacency)
    dependents = transitive_dependents(adjacency)
    dag = {
        "adjacency": {k: adjacency[k] for k in sorted(adjacency)},
        "stratum": {k: depth[k] for k in sorted(depth)},
        "transitive_dependents": {k: dependents.get(k, 0) for k in sorted(adjacency)},
        "edges": sum(len(v) for v in adjacency.values()),
    }
    write_json(INDEX / "dag.json", dag)

    # Subsystems group modules that move together. Unlike rust-index, where a
    # category's members are mutual ALTERNATIVES, membership here means these
    # collaborate -- the set an agent has to read before editing any of them.
    fixtures = {n["name"] for n in nodes if n["kind"] == "schema" and n["fixture"]}
    subsystems: dict[str, set[str]] = {}
    for mod in modules:
        for schema in mod["declares"]:
            if schema in fixtures:
                continue
            subsystems.setdefault(schema.split("/")[1], set()).add(mod["id"])
    grouped = [
        {
            "id": "sub:" + name,
            "name": name,
            "members": sorted(subsystems[name]),
            "terms": sorted(set(tokens(name.replace("-", " ")))),
        }
        for name in sorted(subsystems)
    ]
    write_json(INDEX / "subsystems.json", grouped)

    kinds: dict[str, int] = {}
    for edge in edges:
        kinds[edge["kind"]] = kinds.get(edge["kind"], 0) + 1
    print("link     %d edges, %d subsystems" % (len(edges), len(grouped)))
    for kind in sorted(kinds):
        print("    %-11s %5d" % (kind, kinds[kind]))
    print("    strata      %5d deep" % (max(depth.values()) + 1 if depth else 0))
    return edges


# --------------------------------------------------------------------------
# reuse -- where the tree repeats itself, and what nothing names
# --------------------------------------------------------------------------


def normalise(line: str) -> str | None:
    """A line reduced to its shape: string literals and numbers collapsed.

    Two arms differing only in a tag index or a byte count are the same shape,
    and shape is what can be extracted. Comments and blanks are not code and
    would otherwise inflate every measurement below.
    """
    text = line.strip()
    if not text or text.startswith("//"):
        return None
    text = re.sub(r'"[^"]*"', '"S"', text)
    text = re.sub(r"\b\d+\b", "N", text)
    return re.sub(r"\s+", " ", text)


def duplication(modules: list[dict], window: int = 8) -> dict:
    """Contiguous repeated regions, production and test scope counted apart.

    They are different findings and must never be added together. A repeated
    test fixture costs a little duplication, but the tests are also what proves
    a refactor preserved canonical bytes, so they are not simply waste.
    Measuring both at once is what made the first pass name
    presentation_output <-> _v2 the largest target in the crate, when most of
    that pair's overlap is fixtures and its production overlap is a third the
    size.
    """
    # Each distinct normalised line becomes an integer, so a window is a tuple of
    # ints rather than a joined string. Comparing and hashing those is what makes
    # this stage cheap: joining eight lines at all ~47,000 positions cost 17.6s
    # of the index's 18, and interning takes the same work to well under a second
    # for an identical result.
    interned: dict[str, int] = {}

    def line_id(text: str) -> int:
        got = interned.get(text)
        if got is None:
            got = interned[text] = len(interned)
        return got

    scopes: dict[str, dict] = {"production": {}, "test": {}}
    for mod in modules:
        text = read(ROOT / mod["path"])
        lines = text.splitlines()
        edge = min(test_scope_line(text), len(lines))
        for scope, block, offset in (
            ("production", lines[:edge], 0),
            ("test", lines[edge:], edge),
        ):
            kept = []
            for i, raw in enumerate(block):
                shape = normalise(raw)
                if shape is not None:
                    kept.append((i + 1 + offset, line_id(shape)))
            scopes[scope][mod["name"]] = kept

    out: dict[str, dict] = {}
    for scope, corpus in scopes.items():
        seeds: dict[tuple, list] = {}
        for name, kept in corpus.items():
            shapes = [x for _, x in kept]
            for i in range(len(shapes) - window + 1):
                seeds.setdefault(tuple(shapes[i:i + window]), []).append((name, i))
        claimed: set = set()
        regions = []
        for occurrences in seeds.values():
            if len({n for n, _ in occurrences}) < 2:
                continue
            (left, li), (right, ri) = occurrences[0], occurrences[1]
            if (left, li) in claimed or (right, ri) in claimed:
                continue
            a, b = corpus[left], corpus[right]
            span = 0
            while (
                li + span < len(a)
                and ri + span < len(b)
                and a[li + span][1] == b[ri + span][1]
            ):
                span += 1
            for k in range(span):
                claimed.add((left, li + k))
                claimed.add((right, ri + k))
            regions.append(
                {
                    "lines": span,
                    "left": "%s:%d" % (left, a[li][0]),
                    "right": "%s:%d" % (right, b[ri][0]),
                }
            )
        regions.sort(key=lambda r: (-r["lines"], r["left"]))
        out[scope] = {
            "code_lines": sum(len(v) for v in corpus.values()),
            "duplicated_lines": sum(r["lines"] for r in regions),
            "regions": regions[:40],
        }
    return out


def dead_surface(modules: list[dict]) -> dict:
    """Public items nothing else in the tree names.

    `clippy -D warnings` cannot report this: a `pub` item in a library is
    reachable by definition, so an unused one raises nothing at all. This crate
    is its own only consumer, so "named nowhere else" is a checkable emptiness
    claim rather than a guess -- but confirm by deleting and compiling before
    acting on it, because a name can also be reached through a trait impl or a
    macro, which this scan does not model.
    """
    # Count identifiers once per file instead of running one `\bitem\b` regex per
    # item across every file. That was 561 items x 40 files of scanning and cost
    # 21.7s of the index's 22; tallying tokens up front is the same answer in a
    # fraction of a second. Rust identifiers are exactly `\w+` runs, so a token
    # tally and a word-boundary search agree by construction.
    corpus: dict[str, tuple] = {}
    for path in (
        sorted(SRC.glob("*.rs"))
        + sorted(EXAMPLES.glob("*.rs"))
        + sorted(TESTS.glob("*.rs"))
    ):
        text = read(path)
        lines = text.splitlines()
        edge = min(test_scope_line(text), len(lines))
        corpus[path.stem] = (
            tally(RE_IDENT.findall("\n".join(lines[:edge]))),
            tally(RE_IDENT.findall("\n".join(lines[edge:]))),
        )

    owners: dict[str, set] = {}
    for mod in modules:
        for item in mod["pub_items"]:
            owners.setdefault(item, set()).add(mod["name"])

    unreferenced, module_only = [], []
    for item in sorted(owners):
        own = other = tests = 0
        for name, (prod, test) in corpus.items():
            here = prod.get(item, 0)
            if name in owners[item]:
                own += max(here - 1, 0)  # discount the declaration itself
            else:
                other += here
            tests += test.get(item, 0)
        if other or tests:
            continue
        row = {"item": item, "modules": sorted(owners[item])}
        (unreferenced if own == 0 else module_only).append(row)
    return {
        "pub_items": len(owners),
        "unreferenced": unreferenced,
        "module_private_candidates": module_only,
    }


def stage_reuse(nodes: list[dict]) -> None:
    modules = [n for n in nodes if n["kind"] == "module"]
    reuse = {"duplication": duplication(modules), "dead": dead_surface(modules)}
    write_json(INDEX / "reuse.json", reuse)
    print("reuse")
    for scope in ("production", "test"):
        s = reuse["duplication"][scope]
        print(
            "    %-11s %6d code lines, %4d in repeated regions across %d"
            % (scope, s["code_lines"], s["duplicated_lines"], len(s["regions"]))
        )
    print(
        "    dead        %d public items unreferenced, %d used only in their own module"
        % (
            len(reuse["dead"]["unreferenced"]),
            len(reuse["dead"]["module_private_candidates"]),
        )
    )


# --------------------------------------------------------------------------
# audit -- the three gates
# --------------------------------------------------------------------------


def stage_audit(nodes: list[dict]) -> int:
    modules = [n for n in nodes if n["kind"] == "module"]
    schemas = [n for n in nodes if n["kind"] == "schema"]
    metrics = [n for n in nodes if n["kind"] == "metric"]
    module_names = {m["name"] for m in modules}
    failures: list[str] = []

    # -- Gate A: module accounting -----------------------------------------
    # Every src/*.rs is a declared `pub mod`, lib.rs, or main.rs -- and every
    # `crate::` target resolves. The second half is what catches a truncating
    # dep regex, which otherwise adds a phantom node that looks plausible.
    declared = set(RE_PUB_MOD.findall(read(SRC / "lib.rs")))
    entrypoints = {"lib", "main"}
    accounted = declared | entrypoints
    unaccounted = sorted(module_names - accounted)
    missing_files = sorted(declared - module_names)
    dangling = sorted(
        {d for m in modules for d in m["deps"] if d not in module_names}
    )
    gate_a = not (unaccounted or missing_files or dangling)
    if unaccounted:
        failures.append("modules not declared in lib.rs: %s" % ", ".join(unaccounted))
    if missing_files:
        failures.append("lib.rs declares absent modules: %s" % ", ".join(missing_files))
    if dangling:
        failures.append("crate:: targets that are not modules: %s" % ", ".join(dangling))

    # -- Gate B: schema accounting -----------------------------------------
    # Every schema a Python tool names must exist in the Rust tree, or be an
    # explicitly classified tool-local one. An unclassified one means a rename
    # landed in one language and not the other. The allowlist is resolved
    # against the tree in both directions, so it cannot rot into a rubber stamp.
    rust_schemas = {s["name"] for s in schemas}
    tool_schemas: dict[str, list[str]] = {}
    for tool in (n for n in nodes if n["kind"] == "tool"):
        for schema in tool["schemas"]:
            tool_schemas.setdefault(schema, []).append(tool["name"])

    local = json.loads(read(TOOLS / "schema_local.json"))["local"]
    local_by_schema = {entry["schema"]: entry for entry in local}
    unclassified = sorted(set(tool_schemas) - rust_schemas - set(local_by_schema))
    stale_local = sorted(s for s in local_by_schema if s not in tool_schemas)
    breached = sorted(
        s
        for s, entry in local_by_schema.items()
        if entry["must_not_exist_in_rust"] and s in rust_schemas
    )
    gate_b = not (unclassified or stale_local or breached)
    if unclassified:
        failures.append(
            "tool schemas absent from src/ and unclassified: %s"
            % ", ".join(unclassified)
        )
    if stale_local:
        failures.append(
            "schema_local.json lists schemas no tool declares: %s"
            % ", ".join(stale_local)
        )
    if breached:
        failures.append(
            "schemas declared independent of src/ now appear there: %s"
            % ", ".join(breached)
        )
    surface = [s for s in schemas if not s["fixture"]]
    uncovered = sorted(s["name"] for s in surface if not s["vectored_by"])

    # -- Gate C: metric accounting -----------------------------------------
    # registry.tsv rows == metric_registry.py's own parse == METRICS.md's claim,
    # and every #bb.metric.* reference anywhere resolves to a row.
    aliases = {m["alias"] for m in metrics}
    parsed = None
    try:
        sys.path.insert(0, str(TOOLS))
        import metric_registry  # noqa: E402  (imported for its parse, not re-implemented)

        parsed = len(metric_registry.all_records())
    except Exception as error:  # pragma: no cover - reported, not raised
        failures.append("metric_registry.py did not parse: %s" % error)
    claimed = None
    claim = re.search(r"records \*\*([\d,]+) unique metric families\*\*", read(ROOT / "METRICS.md"))
    if claim:
        claimed = int(claim.group(1).replace(",", ""))
    unresolved: set[str] = set()
    for path in DOC_FILES:
        if path.exists():
            for ref in RE_METRIC_REF.findall(read(path)):
                if ref not in aliases:
                    unresolved.add(ref)
    gate_c = (
        parsed == len(aliases)
        and claimed == len(aliases)
        and not unresolved
    )
    if parsed is not None and parsed != len(aliases):
        failures.append(
            "registry.tsv has %d rows, metric_registry.py parses %d"
            % (len(aliases), parsed)
        )
    if claimed is not None and claimed != len(aliases):
        failures.append(
            "METRICS.md claims %d families, registry.tsv has %d" % (claimed, len(aliases))
        )
    if unresolved:
        failures.append(
            "unresolved #bb.metric refs: %s" % ", ".join(sorted(unresolved)[:5])
        )

    gates = {
        "module_accounting": gate_a,
        "schema_accounting": gate_b,
        "metric_accounting": gate_c,
    }
    # A module with no `//!` comment contributes no vocabulary beyond its name
    # and its public item names, so it is reachable by `ask` only if the query
    # happens to use those words. This is the doc-side analogue of a schema with
    # no vectors: derived, actionable, and not a failure.
    undocumented = sorted(m["name"] for m in modules if not m["summary"])
    coverage = {
        "modules": len(modules),
        "modules_undocumented": undocumented,
        "modules_by_gate": tally(m["gate"] for m in modules),
        "source_lines": sum(m["lines"] for m in modules),
        "tests": sum(m["tests"] for m in modules),
        "schemas": len(surface),
        "schemas_fixtures": len(schemas) - len(surface),
        "schemas_vectored": len(surface) - len(uncovered),
        "schemas_uncovered": uncovered,
        "schemas_tool_local": sorted(local_by_schema),
        "metrics": len(metrics),
        "metrics_by_owner": tally(m["owner"] for m in metrics),
        "metric_registry_parsed": parsed,
        "metrics_claimed_by_doc": claimed,
        "docs": sum(1 for n in nodes if n["kind"] == "doc"),
        "tools": sum(1 for n in nodes if n["kind"] == "tool"),
        "proofs": sum(1 for n in nodes if n["kind"] == "proof"),
        "examples": sum(1 for n in nodes if n["kind"] == "example"),
        "gates": gates,
    }
    write_json(INDEX / "coverage.json", coverage)

    print("audit")
    print("    modules      %d in %d lines, %d tests" % (
        coverage["modules"], coverage["source_lines"], coverage["tests"]))
    if undocumented:
        print("                 %d without a //! doc comment: %s"
              % (len(undocumented), ", ".join(undocumented)))
    print("    schemas      %d protocol objects (+%d test fixtures)"
          % (len(surface), len(schemas) - len(surface)))
    print("                 %d with independent vectors, %d without"
          % (coverage["schemas_vectored"], len(uncovered)))
    print("    metrics      %d across %d owners" % (
        len(metrics), len(coverage["metrics_by_owner"])))
    for name, ok in gates.items():
        print("    gate %-18s %s" % (name, "pass" if ok else "FAIL"))
    for line in failures:
        print("      ! " + line, file=sys.stderr)
    return 0 if all(gates.values()) else 1


def tally(values) -> dict[str, int]:
    out: dict[str, int] = {}
    for value in values:
        out[value] = out.get(value, 0) + 1
    return {k: out[k] for k in sorted(out)}


# --------------------------------------------------------------------------


def write_jsonl(path: Path, rows: list[dict]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    body = "".join(
        json.dumps(row, sort_keys=True, ensure_ascii=False) + "\n" for row in rows
    )
    path.write_text(body, encoding="utf-8", newline="\n")


def write_json(path: Path, obj) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    body = json.dumps(obj, indent=1, sort_keys=True, ensure_ascii=False) + "\n"
    path.write_text(body, encoding="utf-8", newline="\n")


def load_nodes() -> list[dict]:
    return [
        json.loads(line)
        for line in read(INDEX / "nodes.jsonl").splitlines()
        if line.strip()
    ]


def main(argv: list[str]) -> int:
    stages = argv[1:] or ["extract", "link", "reuse", "audit"]
    unknown = [s for s in stages if s not in ("extract", "link", "reuse", "audit")]
    if unknown:
        print(__doc__)
        return 1
    nodes: list[dict] | None = None
    status = 0
    for stage in stages:
        if stage == "extract":
            nodes = stage_extract()
        elif stage == "link":
            stage_link(nodes if nodes is not None else load_nodes())
        elif stage == "reuse":
            stage_reuse(nodes if nodes is not None else load_nodes())
        elif stage == "audit":
            status = stage_audit(nodes if nodes is not None else load_nodes())
    return status


if __name__ == "__main__":
    sys.exit(main(sys.argv))
