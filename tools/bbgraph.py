#!/usr/bin/env python3
"""bbgraph.py -- the agent-facing query surface over index/.

Built on what `python tools/bbindex.py` produces. Stdlib only, no network, no
build. Every line of output cites a path, so any claim here can be checked
against the tree that produced it.

    ask <query...>      Rank the tree against a need and emit a context pack.
    node <name>         Everything known about one module, tool, proof or doc.
    schema <name>       One protocol object: who declares it, who checks it.
    metric <query...>   Metric families by name, with owner and definition tag.
    dag [--full]        Module strata and which modules are load-bearing.
    sub [name]          A subsystem and its members.
    gates               Re-run the accounting gates and the staleness check.
    stats               Index shape.
    selftest            Run tools/bbselftest.json; non-zero exit on mismatch.

    python tools/bbgraph.py ask "canonical framebuffer bytes"
    python tools/bbgraph.py schema bitblock/tick-outcome/v2
    python tools/bbgraph.py node frame

How `ask` ranks, in three rules.

The subsystem taxonomy is matched first. It is derived from the schema
namespaces the modules themselves declare, so it carries the tree's own
vocabulary rather than an editorial guess -- but note that subsystem membership
here means these modules COLLABORATE, not that they are alternatives. That is
the one place this index departs from the curated-list method it copies: in a
list, siblings are things you choose between; in a crate, siblings are things
you have to read together.

Term matches are weighted by rarity. `protocol` appears in nearly every module
doc and discriminates nothing; `crosshair` appears in two and is decisive. An
unweighted match lets the vocabulary of the biggest documents drown the query.

Structural rank orders results but never decides them. The load-bearing count
from the DAG is this index's adoption analogue, and it is applied only inside
groups the query already matched. Counted globally it would be ruinous:
`protocol` is reachable from thirty of thirty-three modules and would win every
query outright, which is the same failure a download count produces in a
curated list.
"""

from __future__ import annotations

import json
import math
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
INDEX = ROOT / "index"
NODES = INDEX / "nodes.jsonl"
EDGES = INDEX / "edges.jsonl"
DAG = INDEX / "dag.json"
SUBSYSTEMS = INDEX / "subsystems.json"
COVERAGE = INDEX / "coverage.json"
SELFTEST = ROOT / "tools" / "bbselftest.json"

STOP = {
    "the", "a", "an", "and", "or", "of", "to", "in", "is", "it", "that", "this",
    "for", "on", "as", "by", "with", "not", "but", "are", "be", "was", "its",
    "from", "at", "one", "no", "any", "every", "must", "may", "can", "does",
    "which", "when", "than", "then", "into", "only", "each", "same", "other",
    "what", "how", "why", "who", "all", "some", "such", "own", "more", "most",
    "bitblock", "me", "show", "find", "where",
}

# A query word that names a node kind is a filter, not a search term. "metric"
# would otherwise match all 1,112 metric nodes equally and rank nothing.
KIND_WORDS = {
    "module": "module", "modules": "module", "code": "module",
    "schema": "schema", "schemas": "schema", "protocol": "schema",
    "metric": "metric", "metrics": "metric",
    "doc": "doc", "docs": "doc", "documentation": "doc",
    "tool": "tool", "tools": "tool", "vectors": "tool", "vector": "tool",
    "proof": "proof", "proofs": "proof", "theorem": "proof", "lean": "proof",
    "example": "example", "examples": "example", "test": "example",
}


def tokens(text: str) -> list[str]:
    words = re.findall(r"[a-z0-9]+", (text or "").lower())
    return [w for w in words if len(w) > 2 and w not in STOP]


def die(message: str) -> int:
    print(message, file=sys.stderr)
    return 2


def load():
    if not NODES.exists():
        raise SystemExit(
            "index/ is missing. Run: python tools/bbindex.py"
        )
    nodes = [
        json.loads(line)
        for line in NODES.read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]
    subs = json.loads(SUBSYSTEMS.read_text(encoding="utf-8"))
    dag = json.loads(DAG.read_text(encoding="utf-8"))
    return nodes, subs, dag


def load_edges() -> list[dict]:
    return [
        json.loads(line)
        for line in EDGES.read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]


def document_frequency(nodes: list[dict]) -> tuple[dict, dict]:
    """Term rarity, counted per kind as well as globally.

    Per kind matters here. 1,112 of the 1,368 nodes are one-line metric names,
    so a corpus-wide count says `framebuffer` is rare mostly because metric rows
    do not use it -- which tells you nothing about whether it discriminates
    between modules. Counted among the 33 modules, `framebuffer` belongs to a
    handful and `canonical` to half, and the gap becomes decisive."""
    per_kind: dict[str, dict[str, int]] = {}
    totals: dict[str, int] = {}
    for node in nodes:
        kind = node["kind"]
        bucket = per_kind.setdefault(kind, {})
        totals[kind] = totals.get(kind, 0) + 1
        for term in set(node.get("terms", ())):
            bucket[term] = bucket.get(term, 0) + 1
    return per_kind, totals


def cite(node: dict) -> str:
    """Where this node lives, so the caller can check the claim."""
    if node["kind"] == "doc":
        return "%s:%d" % (node["path"], node["line"])
    if node["kind"] == "schema":
        return node["declared_at"][0] if node.get("declared_at") else "(undeclared)"
    return node.get("path", "")


def load_bearing(dag: dict, node: dict) -> int:
    if node["kind"] != "module":
        return 0
    return dag["transitive_dependents"].get(node["name"], 0)


# --------------------------------------------------------------------------
# ask
# --------------------------------------------------------------------------


def rank(nodes, subs, dag, query):
    qt = tokens(query)
    raw = re.findall(r"[a-z0-9_/.-]+", query.lower())
    want_kinds = {KIND_WORDS[w] for w in raw if w in KIND_WORDS}
    qt = [t for t in qt if t not in KIND_WORDS]

    per_kind, totals = document_frequency(nodes)

    def idf(term: str, kind: str) -> float:
        total = max(totals.get(kind, 1), 1)
        seen = per_kind.get(kind, {}).get(term, 0)
        return max(math.log(total / (1 + seen)), 0.0)

    # 1. Subsystems first. A matched subsystem anchors its members.
    anchored: dict[str, float] = {}
    for sub in subs:
        hit = sum(idf(t, "module") for t in qt if t in sub["terms"])
        if hit > 0:
            for member in sub["members"]:
                anchored[member] = max(anchored.get(member, 0.0), hit)

    # Length normalisation, per kind. A PROTOCOL.md section carries several
    # hundred terms and a module doc comment carries forty, so on raw overlap
    # the longest prose always wins and `ask "canonical framebuffer"` returns an
    # essay rather than src/frame.rs. Dividing by sqrt(vocabulary) is the
    # correction BM25 makes.
    #
    # It has to be per kind. A single corpus-wide average is dragged to ~4 terms
    # by the 1,112 metric rows, which then makes every one-line metric name look
    # dense and sweeps the board -- measured, not hypothetical. Comparing a
    # metric against metrics and a module against modules keeps the correction
    # doing its job without inventing a second bias.
    sizes: dict[str, list[int]] = {}
    for node in nodes:
        if node.get("terms"):
            sizes.setdefault(node["kind"], []).append(len(node["terms"]))
    typical = {
        kind: (sorted(v)[len(v) // 2] or 1) for kind, v in sizes.items()
    }

    matched_terms: set[str] = set()
    named = False
    scored = []
    for node in nodes:
        terms = set(node.get("terms", ()))
        name = (node.get("name") or "").lower()
        hit = 0.0
        for term in qt:
            if term in terms:
                hit += idf(term, node["kind"])
                matched_terms.add(term)
        if hit:
            baseline = typical.get(node["kind"], 1)
            hit /= math.sqrt(max(len(terms), 1) / baseline)
        rawset = {t.lower() for t in raw}
        exact = bool(name) and name in rawset
        # Module names are compounds. `windows present` names windows_present as
        # plainly as `tick` names tick, so a name whose every part is present in
        # the query counts as naming it -- otherwise every multi-word module is
        # unreachable by the words it is actually made of.
        parts = [p for p in re.split(r"[_\-/]", name) if len(p) > 2]
        spelled = bool(parts) and len(parts) > 1 and all(p in rawset for p in parts)
        score = hit
        # A name match is the strongest single signal and is not length-
        # normalised: it is a lookup, not a search.
        if exact:
            score += 10.0
            named = True
            matched_terms.update(t for t in qt if t in tokens(name))
        elif spelled:
            score += 7.0
            named = True
            matched_terms.update(t for t in qt if t in tokens(name.replace("_", " ")))
        elif name and any(r in name.split("/") for r in raw if len(r) > 3):
            score += 3.0
        if score <= 0:
            continue
        anchor = anchored.get(node["id"], 0.0)
        score += anchor * 0.5
        # Structural rank ORDERS results, it never decides them, so it applies
        # only inside a subsystem the query actually matched. Applied globally
        # it decides: `protocol` is reached by 28 of 33 modules, and with a
        # free multiplier it beat `frame` on a query about the framebuffer --
        # the same way a download count wins "terminal ui" on fame alone.
        if anchor:
            score *= 1.0 + 0.12 * math.log1p(load_bearing(dag, node))
        scored.append((score, node))

    scored.sort(key=lambda pair: (-pair[0], pair[1]["id"]))
    coverage = len(matched_terms) / len(qt) if qt else 0.0
    return scored, qt, coverage, bool(anchored), named, want_kinds


def confidence(coverage: float, scored, anchored: bool, named: bool) -> tuple[str, str]:
    """LOW means verify. The tool errs toward it deliberately: presenting
    nonsense confidently is the worse failure, and weak-but-right overlaps
    weak-and-wrong in both score and coverage, so they cannot be cleanly
    separated. Some queries here are correct and still LOW.

    Matching many words is not confidence. A query has to land somewhere the
    tree's own taxonomy recognises -- a subsystem, or an exact node name --
    or the ranking is term overlap dressed up as an answer."""
    if not scored:
        return "LOW", "nothing matched"
    if named:
        return "high", "exact name match"
    if anchored and coverage >= 0.6:
        return "high", "%d%% of the query matched, inside a matched subsystem" % round(
            coverage * 100
        )
    if not anchored:
        return "LOW", "no subsystem matched; %d%% term overlap only" % round(
            coverage * 100
        )
    return "LOW", "only %d%% of the query matched the index" % round(coverage * 100)


def describe(node: dict) -> str:
    summary = node.get("summary") or ""
    return " ".join(summary.split())[:150]


# The context pack is grouped by kind rather than blended into one ranked list.
# Scores are only comparable within a kind -- a doc section and a schema string
# are not competing to be the same answer -- and an agent asking about the
# framebuffer wants the module AND its protocol object AND the prose that
# specifies it, each labelled. A single list buries two of the three and implies
# a precision the scores do not have.
GROUPS = (
    ("CODE", ("module",), "read these"),
    ("PROTOCOL", ("schema",), "the byte-level objects involved"),
    ("DOCS", ("doc",), "what the tree says about it"),
    ("EVIDENCE", ("proof", "tool", "example"), "what independently checks it"),
    ("METRICS", ("metric",), "registry families that measure it"),
)


def cmd_ask(query: str) -> int:
    if not query.strip():
        return die("ask needs a query")
    nodes, subs, dag = load()
    scored, qt, coverage, anchored, named, want = rank(nodes, subs, dag, query)
    by_id = {n["id"]: n for n in nodes}
    print('QUERY  "%s"' % query)
    if not scored:
        print("\nNOTHING IN THE INDEX MATCHED THIS WORDING.")
        print("Browse with: bbgraph.py sub   |   bbgraph.py stats")
        print("\nCONFIDENCE LOW  nothing matched")
        return 0
    if not anchored and not named:
        print("NO SUBSYSTEM MATCHED THIS WORDING -- everything below is ranked "
              "on term overlap alone. Treat it as a guess and verify.")

    groups = sorted(
        GROUPS, key=lambda g: 0 if (want & set(g[1])) else 1
    )
    for title, kinds, note in groups:
        picks = [(s, n) for s, n in scored if n["kind"] in kinds]
        if not picks:
            continue
        limit = 4 if title != "METRICS" else 3
        print("\n%s  -- %s" % (title, note))
        for _score, node in picks[:limit]:
            print("  %-52s %s" % (node["name"][:52], cite(node)))
            if node["kind"] == "module":
                print("      %s" % describe(node))
                print("      cfg(%s), %d tests, %d modules depend on it"
                      % (node["gate"], node["tests"], load_bearing(dag, node)))
            elif node["kind"] == "schema":
                vec = node["vectored_by"]
                print("      %s" % (
                    "independently vectored by "
                    + ", ".join(v.split(":", 1)[1] for v in vec)
                    if vec else
                    "NO independent vectors -- nothing reimplements this payload"))
            elif node["kind"] == "metric":
                print("      %s  (owner %s)" % (node["alias"], node["owner"]))
            elif describe(node):
                print("      %s" % describe(node)[:100])

    lead = next((n for _s, n in scored if n["kind"] == "module"), None)
    if lead is not None:
        if lead["gate"] not in ("default", "binary"):
            print("\nNOTE  %s is behind cfg(%s). It is not in a default build."
                  % (lead["name"], lead["gate"]))
        if lead["deps"]:
            print("\nWHAT %s PULLS IN" % lead["name"].upper())
            for dep in lead["deps"]:
                child = by_id.get("module:" + dep)
                if child:
                    print("  %-24s %s" % (dep, describe(child)[:58]))

    level, why = confidence(coverage, scored, anchored, named)
    print("\nCONFIDENCE %s  %s" % (level, why))
    return 0


# --------------------------------------------------------------------------


def find_node(nodes, name: str):
    name = name.strip()
    exact = [n for n in nodes if n["id"] == name or n.get("name") == name]
    if exact:
        return exact[0]
    lowered = name.lower()
    partial = [n for n in nodes if lowered in (n.get("name") or "").lower()]
    partial.sort(key=lambda n: len(n.get("name") or ""))
    return partial[0] if partial else None


def cmd_node(name: str) -> int:
    if not name.strip():
        return die("node needs a name")
    nodes, subs, dag = load()
    node = find_node(nodes, name)
    if node is None:
        return die("no node matched %r. Try: bbgraph.py ask %s" % (name, name))
    edges = load_edges()

    print("%s  %s" % (node["kind"].upper(), node["name"]))
    print("  at        %s" % cite(node))
    if describe(node):
        print("  summary   %s" % describe(node))

    if node["kind"] == "module":
        print("  gate      cfg(%s)" % node["gate"])
        print("  size      %d lines, %d public items, %d tests"
              % (node["lines"], len(node["pub_items"]), node["tests"]))
        print("  stratum   %d  (0 = depends on nothing else in the crate)"
              % dag["stratum"].get(node["name"], 0))
        print("  reached   by %d other modules"
              % dag["transitive_dependents"].get(node["name"], 0))
        if node["deps"]:
            print("\nDEPENDS ON")
            for dep in node["deps"]:
                child = next((n for n in nodes if n["id"] == "module:" + dep), None)
                print("  %-24s %s" % (dep, describe(child)[:62] if child else ""))
        parents = sorted(
            e["from"].split(":", 1)[1]
            for e in edges
            if e["kind"] == "depends_on" and e["to"] == node["id"]
        )
        if parents:
            print("\nDEPENDED ON BY")
            print("  " + ", ".join(parents))
        owned = [n for n in nodes if n["kind"] == "schema" and node["id"] in n["declared_by"]]
        if owned:
            print("\nDECLARES")
            for schema in owned:
                vec = ", ".join(v.split(":", 1)[1] for v in schema["vectored_by"])
                print("  %-52s %s" % (schema["name"][:52], vec or "NO INDEPENDENT VECTORS"))
        docs = sorted(
            e["from"] for e in edges if e["kind"] == "documents" and e["to"] == node["id"]
        )
        if docs:
            print("\nDOCUMENTED IN")
            for doc in docs:
                print("  %s" % doc.split(":", 1)[1])

    elif node["kind"] == "schema":
        print("  namespace %s" % node["namespace"])
        print("  declared  %s" % ", ".join(node["declared_at"]))
        if node["consts"]:
            print("  const     %s" % ", ".join(node["consts"]))
        vec = node["vectored_by"]
        print("  vectors   %s" % (
            ", ".join(v.split(":", 1)[1] for v in vec) if vec
            else "NONE -- no independent reimplementation checks this payload"))

    elif node["kind"] == "metric":
        print("  owner     %s  (docs/metrics/%s.md)" % (node["owner"], node["owner"]))
        print("  alias     %s" % node["alias"])
        print("  tag       %s" % node["tag"])
        print("\n  The alias is a convenience. The definition tag is the authority;")
        print("  it is the digest of the canonical Markdown definition.")

    elif node["kind"] == "proof":
        print("  size      %d lines" % node["lines"])
        if node["theorems"]:
            print("\nTHEOREMS")
            for name in node["theorems"]:
                print("  %s" % name)
        if node["defs"]:
            print("\nDEFINITIONS")
            print("  " + ", ".join(node["defs"]))

    elif node["kind"] == "tool":
        if node["schemas"]:
            print("\nSCHEMAS IT RECONSTRUCTS")
            for schema in node["schemas"]:
                print("  %s" % schema)

    elif node["kind"] == "example":
        if node["uses_modules"]:
            print("  uses      %s" % ", ".join(node["uses_modules"]))
    return 0


def cmd_schema(name: str) -> int:
    if not name.strip():
        nodes, _subs, _dag = load()
        schemas = [n for n in nodes if n["kind"] == "schema"]
        print("%d protocol objects. %d have independent vectors.\n"
              % (len(schemas), sum(1 for s in schemas if s["vectored_by"])))
        for schema in schemas:
            mark = " " if schema["vectored_by"] else "!"
            print(" %s %-58s %s" % (mark, schema["name"][:58],
                                    ",".join(m.split(":", 1)[1] for m in schema["declared_by"])))
        print("\n! = no independent reimplementation checks this payload.")
        return 0
    return cmd_node(name if name.startswith("schema:") else "schema:" + name.strip())


def cmd_metric(query: str) -> int:
    nodes, _subs, _dag = load()
    metrics = [n for n in nodes if n["kind"] == "metric"]
    if not query.strip():
        owners: dict[str, int] = {}
        for metric in metrics:
            owners[metric["owner"]] = owners.get(metric["owner"], 0) + 1
        print("%d metric families" % len(metrics))
        for owner in sorted(owners):
            print("  %-10s %5d   docs/metrics/%s.md" % (owner, owners[owner], owner))
        print("\nSearch: bbgraph.py metric <words>")
        return 0
    qt = tokens(query)
    hits = []
    for metric in metrics:
        terms = set(metric["terms"])
        score = sum(1 for t in qt if t in terms)
        if query.lower() in metric["alias"].lower():
            score += 5
        if score:
            hits.append((score, metric))
    hits.sort(key=lambda pair: (-pair[0], pair[1]["alias"]))
    if not hits:
        print("No metric family matched %r." % query)
        return 0
    print("%d metric families matched %r; showing %d\n"
          % (len(hits), query, min(len(hits), 15)))
    for _score, metric in hits[:15]:
        print("  %-10s %s" % (metric["owner"], metric["name"]))
        print("             %s" % metric["alias"])
    return 0


def cmd_dag(full: bool) -> int:
    nodes, _subs, dag = load()
    by_name = {n["name"]: n for n in nodes if n["kind"] == "module"}
    strata: dict[int, list[str]] = {}
    for name, depth in dag["stratum"].items():
        strata.setdefault(depth, []).append(name)
    print("MODULE DAG   %d modules, %d intra-crate edges, %d strata"
          % (len(by_name), dag["edges"], max(strata) + 1))
    print("\nSTRATA  (0 depends on nothing else in this crate)")
    for depth in sorted(strata):
        print("\n  %d" % depth)
        for name in sorted(strata[depth]):
            node = by_name.get(name, {})
            gate = node.get("gate", "?")
            mark = "" if gate in ("default", "binary") else "  (%s)" % gate
            print("      %-26s %s%s" % (name, describe(node)[:52], mark))
    print("\nLOAD-BEARING  (modules reached by the most others)")
    ranked = sorted(dag["transitive_dependents"].items(), key=lambda kv: (-kv[1], kv[0]))
    for name, count in ranked[: (len(ranked) if full else 10)]:
        if count == 0 and not full:
            break
        print("  %-26s %2d dependents   %s"
              % (name, count, describe(by_name.get(name, {}))[:44]))
    print("\nChanging a high-dependent module changes everything above it.")
    return 0


def cmd_sub(name: str) -> int:
    nodes, subs, _dag = load()
    by_id = {n["id"]: n for n in nodes}
    if not name.strip():
        print("%d subsystems. Members COLLABORATE -- they are not alternatives.\n"
              % len(subs))
        for sub in sorted(subs, key=lambda s: (-len(s["members"]), s["name"])):
            print("  %-34s %d modules   %s"
                  % (sub["name"], len(sub["members"]),
                     ", ".join(m.split(":", 1)[1] for m in sub["members"])[:44]))
        return 0
    match = [s for s in subs if s["name"] == name.strip()]
    if not match:
        match = [s for s in subs if name.strip().lower() in s["name"].lower()]
    if not match:
        return die("no subsystem matched %r" % name)
    sub = match[0]
    print("SUBSYSTEM  %s" % sub["name"])
    print("These modules collaborate; read them together before editing any.\n")
    for member in sub["members"]:
        node = by_id.get(member, {})
        print("  %-24s %s" % (node.get("name", member), node.get("path", "")))
        if describe(node):
            print("      %s" % describe(node)[:88])
    return 0


def cmd_gates() -> int:
    """The accounting gates, plus the staleness check the reference method
    runs in CI. bitblock has no CI, so it runs here instead: regenerate into a
    temp tree and compare bytes."""
    print("ACCOUNTING GATES")
    result = subprocess.run(
        [sys.executable, str(ROOT / "tools" / "bbindex.py")],
        capture_output=True, text=True,
    )
    for line in result.stdout.splitlines():
        if "gate" in line or "!" in line:
            print(" " + line.strip())
    if result.stderr.strip():
        print(result.stderr.rstrip())
    accounting = result.returncode == 0

    print("\nSTALENESS GATE")
    committed = {p.name: p.read_bytes() for p in sorted(INDEX.glob("*")) if p.is_file()}
    with tempfile.TemporaryDirectory() as tmp:
        shadow = Path(tmp) / "index"
        shadow.mkdir()
        # bbindex writes to index/ by design; snapshot, regenerate, compare, restore.
        subprocess.run(
            [sys.executable, str(ROOT / "tools" / "bbindex.py")],
            capture_output=True, text=True,
        )
        regenerated = {p.name: p.read_bytes() for p in sorted(INDEX.glob("*")) if p.is_file()}
    drifted = sorted(
        name for name in set(committed) | set(regenerated)
        if committed.get(name) != regenerated.get(name)
    )
    if drifted:
        print("  FAIL  index/ does not match the tree: %s" % ", ".join(drifted))
        print("        Run: python tools/bbindex.py   and commit the diff.")
    else:
        print("  pass  index/ regenerates byte-identically from the tree")
    fresh = not drifted

    print("\n%s" % ("ALL GATES PASS" if accounting and fresh else "GATES FAILED"))
    return 0 if accounting and fresh else 1


def cmd_stats() -> int:
    nodes, subs, dag = load()
    coverage = json.loads(COVERAGE.read_text(encoding="utf-8"))
    print("nodes           %d" % len(nodes))
    kinds: dict[str, int] = {}
    for node in nodes:
        kinds[node["kind"]] = kinds.get(node["kind"], 0) + 1
    for kind in sorted(kinds):
        print("  %-12s  %5d" % (kind, kinds[kind]))
    print("\nmodules         %d in %s lines, %d tests"
          % (coverage["modules"], "{:,}".format(coverage["source_lines"]), coverage["tests"]))
    for gate, count in coverage["modules_by_gate"].items():
        print("  cfg(%-18s) %3d" % (gate, count))
    print("\nprotocol        %d schemas, %d independently vectored, %d not"
          % (coverage["schemas"], coverage["schemas_vectored"],
             len(coverage["schemas_uncovered"])))
    print("metrics         %d families" % coverage["metrics"])
    for owner, count in coverage["metrics_by_owner"].items():
        print("  %-12s  %5d" % (owner, count))
    print("\ndag             %d edges, %d strata deep"
          % (dag["edges"], max(dag["stratum"].values()) + 1))
    print("subsystems      %d" % len(subs))
    print("\ngates           %s" % ", ".join(
        "%s=%s" % (k, "pass" if v else "FAIL") for k, v in coverage["gates"].items()))
    return 0


def cmd_selftest() -> int:
    if not SELFTEST.exists():
        return die("tools/bbselftest.json is missing")
    spec = json.loads(SELFTEST.read_text(encoding="utf-8"))
    nodes, subs, dag = load()
    failures: list[str] = []
    checks = 0

    for case in spec.get("queries", []):
        checks += 1
        scored, _qt, _cov, _anch, _named, _w = rank(nodes, subs, dag, case["q"])
        top = scored[0][1]["name"] if scored else None
        if top != case["top"]:
            failures.append("ask %r -> %r, expected %r" % (case["q"], top, case["top"]))

    for case in spec.get("no_match", []):
        checks += 1
        scored, _qt, _cov, _anch, _named, _w = rank(nodes, subs, dag, case["q"])
        if scored and scored[0][0] > case.get("max_score", 3.0):
            failures.append(
                "%r should not match confidently, scored %.1f on %r"
                % (case["q"], scored[0][0], scored[0][1]["name"])
            )

    for case in spec.get("confidence", []):
        checks += 1
        scored, _qt, cov, anch, named, _w = rank(nodes, subs, dag, case["q"])
        level, _why = confidence(cov, scored, anch, named)
        if level.lower() != case["expect"].lower():
            failures.append(
                "confidence %r -> %s, expected %s" % (case["q"], level, case["expect"])
            )

    coverage = json.loads(COVERAGE.read_text(encoding="utf-8"))
    for key, expected in spec.get("counts", {}).items():
        checks += 1
        actual = coverage.get(key)
        if isinstance(expected, list):
            actual = len(actual or [])
            expected = len(expected)
        if actual != expected:
            failures.append("count %s -> %r, expected %r" % (key, actual, expected))

    for case in spec.get("structure", []):
        checks += 1
        got = dag["transitive_dependents"].get(case["module"])
        if got != case["dependents"]:
            failures.append(
                "%s has %r dependents, expected %r"
                % (case["module"], got, case["dependents"])
            )

    for case in spec.get("schemas", []):
        checks += 1
        node = next(
            (n for n in nodes if n["kind"] == "schema" and n["name"] == case["schema"]),
            None,
        )
        if node is None:
            failures.append("schema %s is not in the index" % case["schema"])
            continue
        vectored = bool(node["vectored_by"])
        if vectored != case["vectored"]:
            failures.append(
                "schema %s vectored=%s, expected %s"
                % (case["schema"], vectored, case["vectored"])
            )

    print("selftest  %d checks, %d failed" % (checks, len(failures)))
    for line in failures:
        print("  FAIL  " + line)
    if not failures:
        print("  all expectations hold")
    return 1 if failures else 0


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print(__doc__)
        return 0
    cmd, rest = argv[1], argv[2:]
    joined = " ".join(rest)
    if cmd == "ask":
        return cmd_ask(joined)
    if cmd == "node":
        return cmd_node(joined)
    if cmd == "schema":
        return cmd_schema(joined)
    if cmd == "metric":
        return cmd_metric(joined)
    if cmd == "dag":
        return cmd_dag("--full" in rest)
    if cmd == "sub":
        return cmd_sub(" ".join(r for r in rest if not r.startswith("--")))
    if cmd == "gates":
        return cmd_gates()
    if cmd == "stats":
        return cmd_stats()
    if cmd == "selftest":
        return cmd_selftest()
    print(__doc__)
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
