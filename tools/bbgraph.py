#!/usr/bin/env python3
"""bbgraph.py -- the agent-facing query surface over index/.

Built on what `python tools/bbindex.py` produces. Stdlib only, no network, no
build. Every line of output cites a path, so any claim here can be checked
against the tree that produced it.

    ask <query...>      Rank the tree against a need and emit a context pack.
    node <name>         Everything known about one module, tool, proof or doc.
    sym <name>          One symbol or Type::method: span, signature, doc, types.
    type <TypeName>     Where a type is defined, constructed, implemented, used.
    returns <TypeName>  Every function whose signature returns TypeName.
    takes <TypeName>    Every function whose signature takes TypeName.
    callers <target>    Production and #[test] callers of a function or method.
    impact <modules...> 4-tier blast-radius and verification recipe for modules.
    coverage            Module //! and public symbol /// documentation coverage.
    eval                Self-supervised holdout retrieval benchmark (module/symbol/test).
    schema <name>       One protocol object: who declares it, who checks it.
    metric <query...>   Metric families by name, with owner and definition tag.
    dag [--full]        Module strata and which modules are load-bearing.
    wave <modules...>   Partition modules into parallel subagent waves (deps + test_deps).
    dup [test]          Where the tree repeats itself, and where a shared
                        helper could legally live. Production scope by default.
    dead                Public items nothing else in the tree names.
    sub [name]          A subsystem and its members.
    gates               Re-run the accounting gates and the staleness check.
    stats               Index shape.
    selftest            Run tools/bbselftest.json; non-zero exit on mismatch.
    html [path]         Compile index/ into a standalone interactive HTML observatory.

    python tools/bbgraph.py ask "canonical framebuffer bytes"
    python tools/bbgraph.py schema bitblock/tick-outcome/v3
    python tools/bbgraph.py node frame
    python tools/bbgraph.py html target/bitblock_graph.html

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
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
INDEX = ROOT / "index"
NODES = INDEX / "nodes.jsonl"
EDGES = INDEX / "edges.jsonl"
DAG = INDEX / "dag.json"
SUBSYSTEMS = INDEX / "subsystems.json"
COVERAGE = INDEX / "coverage.json"
REUSE = INDEX / "reuse.json"
SELFTEST = ROOT / "tools" / "bbselftest.json"

STOP = {
    "the", "a", "an", "and", "or", "of", "to", "in", "is", "it", "that", "this",
    "for", "on", "as", "by", "with", "not", "but", "are", "be", "was", "its",
    "from", "at", "one", "no", "any", "every", "must", "may", "can", "does",
    "which", "when", "than", "then", "into", "only", "each", "same", "other",
    "their", "them", "they", "there", "here", "have", "has", "had", "were",
    "will", "would", "should", "could", "rather", "still", "also", "both",
    "over", "under", "after", "before", "because", "done",
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


RE_TAG_QUERY = re.compile(
    r"^(?:#bb\.([a-z0-9-]+)\.sha256\.([a-z0-9]*)|([a-z2-7]{52}))$"
)
RE_B32_DIGEST = re.compile(r"^[a-z2-7]{52}$")


def resolve_tag(nodes: list[dict], query: str) -> dict | None:
    q = query.strip().strip("\"'").strip().lower()
    m = RE_TAG_QUERY.match(q)
    if not m:
        return None
    kind = m.group(1)
    digest = m.group(2) if kind is not None else m.group(3)
    if not digest or not RE_B32_DIGEST.match(digest):
        return {
            "query": query.strip(),
            "kind": kind,
            "digest": digest or "",
            "tags": [],
            "nodes": [],
            "ids": [],
        }
    target_full = f"#bb.{kind}.sha256.{digest}" if kind else None
    suffix = "." + digest
    matched_tags: set[str] = set()
    hits: list[dict] = []
    for n in nodes:
        if n["kind"] in ("module", "tool", "doc", "example"):
            tags = [
                t
                for t in n.get("pinned_tags", ())
                if (t == target_full if target_full else t.endswith(suffix))
            ]
            if tags:
                matched_tags.update(tags)
                hits.append(n)
        elif n["kind"] == "metric":
            t = n.get("tag", "")
            if t == target_full if target_full else t.endswith(suffix):
                matched_tags.add(t)
                hits.append(n)
    hits.sort(key=lambda n: n["id"])
    return {
        "query": query.strip(),
        "kind": kind,
        "digest": digest,
        "tags": sorted(matched_tags),
        "nodes": hits,
        "ids": [n["id"] for n in hits],
    }


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
    raw_ident = {
        w.lower() for w in re.findall(r"[A-Za-z0-9_]+", query)
        if "_" in w or any(c.isupper() for c in w[1:])
    }
    quals = re.findall(r"([a-z0-9_]+)::([a-z0-9_]+)", query.lower())
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
        exact = bool(name) and (name in rawset or name.replace("_", "-") in rawset)
        # Module names are compounds. `windows present` names windows_present as
        # plainly as `tick` names tick, so a name whose every part is present in
        # the query counts as naming it -- otherwise every multi-word module is
        # unreachable by the words it is actually made of.
        parts = [p for p in re.split(r"[_\-/]", name) if len(p) > 2]
        spelled = bool(parts) and len(parts) > 1 and all(p in rawset for p in parts)
        sym_hit = False
        if node["kind"] == "module" and (quals or raw_ident) and node.get("symbols"):
            for s in node["symbols"]:
                sname = s["name"].lower()
                simpl = (s.get("impl") or "").lower()
                if quals and any((q == simpl or q == name) and m == sname for q, m in quals):
                    sym_hit = True
                    matched_terms.update(tokens(s["name"]) + tokens(simpl))
                    break
                if sname in raw_ident and ("_" in sname or s["kind"] in ("struct", "enum", "trait", "type", "const")):
                    sym_hit = True
                    matched_terms.update(tokens(s["name"]))
        score = hit
        # A name match is the strongest single signal and is not length-
        # normalised: it is a lookup, not a search.
        if exact:
            score += 10.0
            named = True
            matched_terms.update(t for t in qt if t in tokens(name))
        elif sym_hit:
            score += 8.0
            named = True
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
    tag_hit = resolve_tag(nodes, query)
    if tag_hit is not None:
        print('QUERY  "%s"' % query)
        if not tag_hit["nodes"]:
            print("\nNOTHING IN THE INDEX MATCHED THIS WORDING.")
            print("Browse with: bbgraph.py sub   |   bbgraph.py stats")
            print("\nCONFIDENCE LOW  nothing matched")
            return 0
        print("MATCHED CONTENT TAG  %s" % ", ".join(tag_hit["tags"]))
        for title, kinds, note in GROUPS:
            picks = [n for n in tag_hit["nodes"] if n["kind"] in kinds]
            if not picks:
                continue
            print("\n%s  -- %s" % (title, note))
            for node in picks:
                print("  %-52s %s" % (node["name"][:52], cite(node)))
                if node["kind"] == "module":
                    print("      %s" % describe(node))
                    print("      cfg(%s), %d tests, %d modules depend on it"
                          % (node["gate"], node["tests"], load_bearing(dag, node)))
                elif node["kind"] == "metric":
                    print("      %s  (owner %s)" % (node["alias"], node["owner"]))
                elif describe(node):
                    print("      %s" % describe(node)[:100])
        print("\nCONFIDENCE high  exact content tag match")
        return 0
    scored, qt, coverage, anchored, named, want = rank(nodes, subs, dag, query)
    by_id = {n["id"]: n for n in nodes}
    per_kind, _ = document_frequency(nodes)
    missing = [
        t for t in dict.fromkeys(qt)
        if all(per_kind.get(k, {}).get(t, 0) == 0 for k in per_kind)
    ]
    print('QUERY  "%s"' % query)
    if missing and not named:
        print("NOT IN THIS INDEX: %s" % ", ".join(missing))
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
    lead = next((n for _s, n in scored if n["kind"] == "module"), None)
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
                    "test fixture, not protocol surface" if node["fixture"]
                    else "independently vectored by "
                    + ", ".join(v.split(":", 1)[1] for v in vec) if vec
                    else "NO independent vectors -- nothing reimplements this payload"))
            elif node["kind"] == "metric":
                print("      %s  (owner %s)" % (node["alias"], node["owner"]))
            elif describe(node):
                print("      %s" % describe(node)[:100])

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


def fmt_syms(syms: list[str], limit: int = 6) -> str:
    return ", ".join(syms[:limit]) + (
        f", +{len(syms) - limit} more" if len(syms) > limit else ""
    )


def find_node(nodes, name: str):
    name = name.strip().strip("\"'").strip()
    exact = [
        n
        for n in nodes
        if n["id"] == name or n.get("name") == name or n.get("alias") == name
    ]
    if exact:
        return exact[0]
    lowered = name.lower()
    ci_exact = [
        n
        for n in nodes
        if n["id"].lower() == lowered
        or (n.get("name") or "").lower() == lowered
        or (n.get("alias") or "").lower() == lowered
    ]
    if ci_exact:
        return ci_exact[0]
    partial = [
        n
        for n in nodes
        if lowered in (n.get("name") or "").lower()
        or lowered in (n.get("alias") or "").lower()
    ]
    partial.sort(key=lambda n: len(n.get("name") or ""))
    return partial[0] if partial else None


def cmd_node(name: str) -> int:
    if not name.strip():
        return die("node needs a name")
    nodes, subs, dag = load()
    tag_hit = resolve_tag(nodes, name)
    if tag_hit is not None:
        if not tag_hit["nodes"]:
            return die("no node matched content tag %r" % name)
        print("CONTENT TAG  %s" % ", ".join(tag_hit["tags"]))
        for hn in tag_hit["nodes"]:
            loc = cite(hn)
            extra = "  cfg(%s)" % hn["gate"] if hn.get("gate") else ""
            print("  %-46s %-8s %s%s" % (hn["id"], hn["kind"], loc, extra))
            if describe(hn):
                print("      %s" % describe(hn)[:100])
        return 0
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
        print("  size      %d lines, %d public items, %d tests, %d mutation sites"
              % (node["lines"], len(node["pub_items"]), node["tests"], node.get("mutation_sites", 0)))
        pub_cnt = node.get("pub_symbol_count", 0)
        pub_doc = node.get("pub_documented", 0)
        if pub_cnt > 0:
            print(
                "  symbol docs  %d / %d (%d%%)"
                % (pub_doc, pub_cnt, round(100.0 * pub_doc / pub_cnt))
            )
        else:
            print("  symbol docs  0 / 0 (n/a)")
        print("  stratum   %d  (0 = depends on nothing else in the crate)"
              % dag["stratum"].get(node["name"], 0))
        print("  reached   by %d other modules"
              % dag["transitive_dependents"].get(node["name"], 0))
        if node["deps"]:
            print("\nDEPENDS ON")
            for dep in node["deps"]:
                child = next((n for n in nodes if n["id"] == "module:" + dep), None)
                syms = node.get("symbol_deps", {}).get(dep, [])
                sym_str = " [%s]" % fmt_syms(syms) if syms else ""
                print(
                    "  %-24s %s%s"
                    % (dep, describe(child)[:62] if child else "", sym_str)
                )
        if node.get("test_deps") or node.get("test_symbol_deps"):
            print("\nTEST DEPS  (#[cfg(test)] only -- does not move DAG stratum)")
            for dep in sorted(
                set(node.get("test_deps", ())) | set(node.get("test_symbol_deps", {}))
            ):
                child = next((n for n in nodes if n["id"] == "module:" + dep), None)
                syms = node.get("test_symbol_deps", {}).get(dep, [])
                sym_str = " [%s]" % fmt_syms(syms) if syms else ""
                extra = " (#[cfg(test)] extra)" if dep in node.get("deps", ()) else ""
                print(
                    "  %-24s %s%s%s"
                    % (dep, describe(child)[:62] if child else "", sym_str, extra)
                )
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
        proofs = sorted(
            e["from"] for e in edges if e["kind"] == "proves" and e["to"] == node["id"]
        )
        if proofs:
            print("\nPROVED BY")
            for pid in proofs:
                pnode = next((n for n in nodes if n["id"] == pid), None)
                print("  %-36s %s" % (pid.split(":", 1)[1], describe(pnode) if pnode else ""))
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
        if node["fixture"]:
            print("  scope     test fixture -- declared only inside #[cfg(test)],")
            print("            not part of the protocol surface")
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
        targets = sorted(
            e["to"].split(":", 1)[1]
            for e in edges
            if e["kind"] == "proves" and e["from"] == node["id"]
        )
        if targets:
            print("  proves    %s" % ", ".join(targets))
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

    elif node["kind"] == "doc":
        if node.get("cites_src"):
            print("\nCITES MODULES")
            for src in node["cites_src"]:
                print("  %s" % src)
        if node.get("metric_refs"):
            print("\nCITES METRICS")
            for ref in node["metric_refs"]:
                print("  %s" % ref)

    elif node["kind"] == "example":
        if node["uses_modules"]:
            print("  uses      %s" % ", ".join(node["uses_modules"]))

    pinned = node.get("pinned_tags", ())
    if pinned:
        print("\nPINNED TAGS  (%d)" % len(pinned))
        for tag in pinned[:12]:
            peers = sorted(
                other["id"]
                for other in nodes
                if other["id"] != node["id"]
                and (
                    tag in other.get("pinned_tags", ())
                    or (other["kind"] == "metric" and other.get("tag") == tag)
                )
            )
            peer_str = "  <-> " + ", ".join(peers) if peers else ""
            print("  %s%s" % (tag, peer_str))
        if len(pinned) > 12:
            print("  ... and %d more" % (len(pinned) - 12))
    return 0


def _resolve_type_name(modules: list[dict], query: str) -> str | None:
    q = query.strip()
    if not q:
        return None
    all_types: set[str] = set()
    for m in modules:
        for s in m.get("symbols", ()):
            if s["kind"] in ("struct", "enum", "trait", "type"):
                all_types.add(s["name"])
            if s.get("impl"):
                all_types.add(s["impl"])
            all_types.update(s.get("takes", ()))
            all_types.update(s.get("returns", ()))
    if "::" in q and q not in all_types:
        q = q.rsplit("::", 1)[-1]
    if q in all_types:
        return q
    low = q.lower()
    ci = sorted(t for t in all_types if t.lower() == low)
    return ci[0] if ci else None


def cmd_sym(query: str) -> int:
    q = query.strip()
    if not q:
        return die("sym needs a symbol name (e.g. bbgraph.py sym SparseOctreeV1::from_world)")
    nodes, _subs, _dag = load()
    modules = [n for n in nodes if n["kind"] == "module"]

    def match_sym(m: dict, s: dict, target: str, ci: bool) -> bool:
        name = s["name"]
        impl_t = s.get("impl")
        mod_n = m["name"]
        candidates = [name, "%s::%s" % (mod_n, name)]
        if impl_t:
            candidates.append("%s::%s" % (impl_t, name))
            candidates.append("%s::%s::%s" % (mod_n, impl_t, name))
        if ci:
            t_low = target.lower()
            return any(c.lower() == t_low for c in candidates)
        return target in candidates

    hits = [
        (m, s)
        for m in modules
        for s in m.get("symbols", ())
        if match_sym(m, s, q, ci=False)
    ]
    if not hits:
        hits = [
            (m, s)
            for m in modules
            for s in m.get("symbols", ())
            if match_sym(m, s, q, ci=True)
        ]
    if not hits:
        return die("no symbol matched %r. Try: bbgraph.py ask %s" % (q, q))

    for idx, (m, s) in enumerate(hits):
        if idx > 0:
            print()
        qual = "%s::%s" % (s["impl"], s["name"]) if s.get("impl") else s["name"]
        vis = "pub " if s.get("pub") else ""
        print("SYMBOL  %s%s %s  (%s)" % (vis, s["kind"], qual, m["name"]))
        print("  at        %s:%d-%d" % (m["path"], s["line"], s["end"]))
        print("  gate      cfg(%s)" % m["gate"])
        print("  kind      %s%s" % ("pub " if s.get("pub") else "private ", s["kind"]))
        if s.get("impl"):
            print("  impl      %s" % s["impl"])
        if s.get("sig"):
            print("  sig       %s" % s["sig"])
        if s.get("doc"):
            print("  doc       %s" % s["doc"])
        if s.get("takes"):
            print("  takes     %s" % ", ".join(s["takes"]))
        if s.get("returns"):
            print("  returns   %s" % ", ".join(s["returns"]))
        if s.get("calls"):
            print("  calls     %s" % ", ".join(s["calls"]))
    return 0


def cmd_type(query: str) -> int:
    q = query.strip()
    if not q:
        return die("type needs a type name (e.g. bbgraph.py type SparseOctreeV1)")
    nodes, _subs, _dag = load()
    modules = [n for n in nodes if n["kind"] == "module"]
    tname = _resolve_type_name(modules, q)
    if tname is None:
        return die("no type matched %r. Try: bbgraph.py ask %s" % (q, q))

    defs = [
        (m, s)
        for m in modules
        for s in m.get("symbols", ())
        if s["name"] == tname and s["kind"] in ("struct", "enum", "trait", "type")
    ]
    returned_by = [
        (m, s)
        for m in modules
        for s in m.get("symbols", ())
        if tname in s.get("returns", ())
    ]
    returned_by.sort(
        key=lambda p: (p[1].get("impl") != tname, p[0]["name"], p[1]["line"])
    )
    methods = [
        (m, s)
        for m in modules
        for s in m.get("symbols", ())
        if s.get("impl") == tname
    ]
    methods.sort(key=lambda p: (not p[1]["pub"], p[0]["name"], p[1]["line"]))
    taken_by = [
        (m, s)
        for m in modules
        for s in m.get("symbols", ())
        if tname in s.get("takes", ()) and s.get("impl") != tname
    ]
    taken_by.sort(key=lambda p: (p[0]["name"], p[1]["line"]))

    print("TYPE  %s" % tname)
    print("\nDEFINED AT")
    if defs:
        for m, s in defs:
            vis = "pub " if s.get("pub") else ""
            print(
                "  %-42s %s:%d-%d  cfg(%s)"
                % ("%s%s %s" % (vis, s["kind"], s["name"]), m["path"], s["line"], s["end"], m["gate"])
            )
            if s.get("sig"):
                print("      sig: %s" % s["sig"])
            if s.get("doc"):
                print("      doc: %s" % s["doc"])
    else:
        print("  (not declared as a top-level struct/enum/trait/type in workspace)")

    print("\nRETURNED BY (constructors / producers)  (%d)" % len(returned_by))
    if returned_by:
        for m, s in returned_by:
            qual = (
                "%s::%s" % (s["impl"], s["name"])
                if s.get("impl")
                else "%s::%s" % (m["name"], s["name"])
            )
            tag = "[ctor] " if s.get("impl") == tname else ""
            print(
                "  %-46s %s:%d-%d  cfg(%s)"
                % (tag + qual, m["path"], s["line"], s["end"], m["gate"])
            )
            if s.get("sig"):
                print("      %s" % s["sig"])
    else:
        print("  (none)")

    print("\nMETHODS (impl %s)  (%d)" % (tname, len(methods)))
    if methods:
        for m, s in methods:
            vis = "pub " if s.get("pub") else ""
            print(
                "  %-46s %s:%d-%d  cfg(%s)"
                % ("%s%s %s::%s" % (vis, s["kind"], tname, s["name"]), m["path"], s["line"], s["end"], m["gate"])
            )
            if s.get("sig"):
                print("      %s" % s["sig"])
            if s.get("doc"):
                print("      doc: %s" % s["doc"])
    else:
        print("  (none)")

    print("\nTAKEN BY (external consumers)  (%d)" % len(taken_by))
    if taken_by:
        for m, s in taken_by:
            qual = (
                "%s::%s" % (s["impl"], s["name"])
                if s.get("impl")
                else "%s::%s" % (m["name"], s["name"])
            )
            print(
                "  %-46s %s:%d-%d  cfg(%s)"
                % (qual, m["path"], s["line"], s["end"], m["gate"])
            )
            if s.get("sig"):
                print("      %s" % s["sig"])
    else:
        print("  (none)")
    return 0


def cmd_returns(query: str) -> int:
    q = query.strip()
    if not q:
        return die("returns needs a type name (e.g. bbgraph.py returns ContentTag)")
    nodes, _subs, _dag = load()
    modules = [n for n in nodes if n["kind"] == "module"]
    tname = _resolve_type_name(modules, q)
    if tname is None:
        return die("no type matched %r" % q)
    hits = [
        (m, s)
        for m in modules
        for s in m.get("symbols", ())
        if tname in s.get("returns", ())
    ]
    hits.sort(key=lambda p: (p[1].get("impl") != tname, p[0]["name"], p[1]["line"]))
    if not hits:
        return die("no functions return %s" % tname)
    print("RETURNS  %s  (%d functions)\n" % (tname, len(hits)))
    for m, s in hits:
        qual = (
            "%s::%s" % (s["impl"], s["name"])
            if s.get("impl")
            else "%s::%s" % (m["name"], s["name"])
        )
        print(
            "  %-46s %s:%d-%d  cfg(%s)"
            % (qual, m["path"], s["line"], s["end"], m["gate"])
        )
        if s.get("sig"):
            print("      %s" % s["sig"])
    return 0


def cmd_takes(query: str) -> int:
    q = query.strip()
    if not q:
        return die("takes needs a type name (e.g. bbgraph.py takes World)")
    nodes, _subs, _dag = load()
    modules = [n for n in nodes if n["kind"] == "module"]
    tname = _resolve_type_name(modules, q)
    if tname is None:
        return die("no type matched %r" % q)
    hits = [
        (m, s)
        for m in modules
        for s in m.get("symbols", ())
        if tname in s.get("takes", ())
    ]
    hits.sort(key=lambda p: (p[1].get("impl") == tname, p[0]["name"], p[1]["line"]))
    if not hits:
        return die("no functions take %s" % tname)
    print("TAKES  %s  (%d functions)\n" % (tname, len(hits)))
    for m, s in hits:
        qual = (
            "%s::%s" % (s["impl"], s["name"])
            if s.get("impl")
            else "%s::%s" % (m["name"], s["name"])
        )
        print(
            "  %-46s %s:%d-%d  cfg(%s)"
            % (qual, m["path"], s["line"], s["end"], m["gate"])
        )
        if s.get("sig"):
            print("      %s" % s["sig"])
    return 0


def cmd_callers(target: str) -> int:
    q = target.strip()
    if not q:
        return die("callers needs a function or Type::method name (e.g. bbgraph.py callers World::exposed_faces)")
    nodes, _subs, _dag = load()
    modules = [n for n in nodes if n["kind"] == "module"]
    module_names = {m["name"] for m in modules}

    norm_q = q
    qual_mod: str | None = None
    qual_fn: str | None = None
    if q.count("::") == 2:
        p0, p1, p2 = q.split("::")
        if p0 in module_names:
            norm_q = "%s::%s" % (p1, p2)
    elif q.count("::") == 1:
        p0, p1 = q.split("::")
        if p0 in module_names:
            qual_mod, qual_fn = p0, p1

    def match_call(c: str, caller_mod: dict, is_test: bool, ci: bool) -> bool:
        if qual_mod is not None and qual_fn is not None:
            reach = {caller_mod["name"]} | set(caller_mod.get("deps", ()))
            if is_test:
                reach |= set(caller_mod.get("test_deps", ()))
            if qual_mod not in reach:
                return False
            if ci:
                return c.lower() in (norm_q.lower(), qual_fn.lower())
            return c in (norm_q, qual_fn)
        if "::" in norm_q:
            return c.lower() == norm_q.lower() if ci else c == norm_q
        if ci:
            cl = c.lower()
            nl = norm_q.lower()
            return cl == nl or cl.endswith("::" + nl)
        return c == norm_q or c.endswith("::" + norm_q)

    def collect_callers(ci: bool):
        prod_hits = []
        test_hits = []
        for m in modules:
            for s in m.get("symbols", ()):
                if s["kind"] != "fn":
                    continue
                if any(
                    match_call(c, m, is_test=False, ci=ci)
                    for c in s.get("calls", ())
                ):
                    prod_hits.append((m, s))
            for tf in m.get("test_fns", ()):
                if any(
                    match_call(c, m, is_test=True, ci=ci)
                    for c in tf.get("calls", ())
                ):
                    test_hits.append((m, tf))
        return prod_hits, test_hits

    prod_hits, test_hits = collect_callers(ci=False)
    if not prod_hits and not test_hits:
        prod_hits, test_hits = collect_callers(ci=True)

    if not prod_hits and not test_hits:
        known = False
        ql = norm_q.lower()
        for m in modules:
            for s in m.get("symbols", ()):
                cands = [s["name"], "%s::%s" % (m["name"], s["name"])]
                if s.get("impl"):
                    cands.append("%s::%s" % (s["impl"], s["name"]))
                    cands.append("%s::%s::%s" % (m["name"], s["impl"], s["name"]))
                if any(c.lower() == ql or c.lower() == q.lower() for c in cands):
                    known = True
                    break
            if known:
                break
        if not known:
            return die("no callers or symbol matched %r. Try: bbgraph.py sym %s" % (q, q))

    prod_hits.sort(key=lambda p: (p[0]["name"], p[1]["line"]))
    test_hits.sort(key=lambda p: (p[0]["name"], p[1]["line"]))

    print("CALLERS  %s" % norm_q)
    print("\nPRODUCTION CALLERS  (%d)" % len(prod_hits))
    if prod_hits:
        for m, s in prod_hits:
            qual = (
                "%s::%s::%s" % (m["name"], s["impl"], s["name"])
                if s.get("impl")
                else "%s::%s" % (m["name"], s["name"])
            )
            print(
                "  %-48s %s:%d-%d  cfg(%s)"
                % (qual, m["path"], s["line"], s["end"], m["gate"])
            )
    else:
        print("  (none)")

    print("\nUNIT TEST CALLERS (#[test])  (%d)" % len(test_hits))
    if test_hits:
        for m, tf in test_hits:
            qual = "%s::tests::%s" % (m["name"], tf["name"])
            print(
                "  %-48s %s:%d-%d  cfg(%s)"
                % (qual, m["path"], tf["line"], tf["end"], m["gate"])
            )
    else:
        print("  (none)")

    active_mods = [m for m, _ in test_hits] or [m for m, _ in prod_hits]
    if active_mods:
        crates = {m.get("crate") for m in active_mods}
        if crates == {"core"}:
            cargo_cmd = "cargo test -p bitblock-core"
        elif crates == {"research"}:
            cargo_cmd = "cargo test -p bitblock-research"
        elif crates == {"binary"}:
            cargo_cmd = "cargo test --bin bitblock --features protocol-research"
        else:
            needs_feat = any(
                m.get("crate") in ("research", "binary") or m.get("gate") != "default"
                for m in active_mods
            )
            cargo_cmd = (
                "cargo test --workspace --features protocol-research"
                if needs_feat
                else "cargo test --workspace"
            )
        if len(test_hits) == 1:
            print("\n  Run: %s %s" % (cargo_cmd, test_hits[0][1]["name"]))
        else:
            print("\n  Run: %s" % cargo_cmd)
    return 0


def compute_impact(
    nodes: list[dict], edges: list[dict], dag: dict, targets: list[str]
) -> dict:
    mods = {n["name"]: n for n in nodes if n["kind"] == "module"}
    tset = set(targets)
    direct_prod = sorted(
        {m["name"] for m in mods.values() if set(m.get("deps", ())) & tset} - tset
    )
    direct_test_only = sorted(
        m["name"]
        for m in mods.values()
        if (set(m.get("test_deps", ())) & tset)
        and m["name"] not in (tset | set(direct_prod))
    )
    rev_prod: dict[str, set[str]] = {n: set() for n in dag.get("adjacency", {})}
    for u, deps in dag.get("adjacency", {}).items():
        for dep in deps:
            rev_prod.setdefault(dep, set()).add(u)
    all_trans_prod: set[str] = set()
    stack = [u for t in targets for u in rev_prod.get(t, ())]
    while stack:
        cur = stack.pop()
        if cur in all_trans_prod:
            continue
        all_trans_prod.add(cur)
        stack.extend(rev_prod.get(cur, ()))
    trans_2plus = sorted(all_trans_prod - tset - set(direct_prod))

    def mod_vectors(mname: str) -> set[str]:
        schemas = {"schema:" + s for s in mods[mname].get("declares", ())}
        return {
            e["from"].split(":", 1)[1]
            for e in edges
            if e["kind"] == "vectors"
            and e["to"] in schemas
            and e["from"] != "tool:metric_registry"
        }

    direct_vecs = sorted({v for t in targets for v in mod_vectors(t)})
    hop1_vecs = sorted(
        {v for d in direct_prod for v in mod_vectors(d)} - set(direct_vecs)
    )
    mut_targets = [t for t in targets if mods[t].get("mutation_sites", 0) > 0]
    zero_targets = {t for t in targets if mods[t].get("mutation_sites", 0) == 0}
    mut_1hop = sorted(
        d
        for d in direct_prod
        if mods[d].get("mutation_sites", 0) > 0
        and (set(mods[d].get("deps", ())) & zero_targets)
    )
    target_ids = {"module:" + t for t in targets}
    proofs = sorted(
        {
            e["from"].split(":", 1)[1]
            for e in edges
            if e["kind"] == "proves" and e["to"] in target_ids
        }
    )
    return {
        "targets": list(targets),
        "direct_prod": direct_prod,
        "direct_test_only": direct_test_only,
        "trans_2plus": trans_2plus,
        "direct_vecs": direct_vecs,
        "hop1_vecs": hop1_vecs,
        "mut_targets": mut_targets,
        "mut_1hop": mut_1hop,
        "proofs": proofs,
    }


def cmd_impact(raw: str) -> int:
    raw_tokens = [t.strip() for t in re.split(r"[\s,]+", raw.strip()) if t.strip()]
    if not raw_tokens:
        return die("impact needs at least one module name")
    nodes, _subs, dag = load()
    edges = load_edges()
    mods = {n["name"]: n for n in nodes if n["kind"] == "module"}
    mod_list = [mods[k] for k in sorted(mods)]

    raw_targets: list[str] = []
    for tok in raw_tokens:
        clean_tok = tok[len("module:") :] if tok.startswith("module:") else tok
        if clean_tok.endswith(".rs"):
            clean_tok = Path(clean_tok).stem
        if clean_tok in mods:
            raw_targets.append(clean_tok)
            continue
        if "::" in clean_tok and clean_tok.split("::", 1)[0] in mods:
            mname = clean_tok.split("::", 1)[0]
            print(
                "RESOLVED SYMBOL  %s -> module %s  (for function-level callers, run: python3 tools/bbgraph.py callers %s)"
                % (tok, mname, tok)
            )
            raw_targets.append(mname)
            continue
        matched_mods: list[str] = []
        for m in mod_list:
            for s in m.get("symbols", ()):
                qual = "%s::%s" % (s["impl"], s["name"]) if s.get("impl") else None
                if s["name"] == clean_tok or qual == clean_tok:
                    if m["name"] not in matched_mods:
                        matched_mods.append(m["name"])
        if not matched_mods:
            low = clean_tok.lower()
            for m in mod_list:
                for s in m.get("symbols", ()):
                    qual = (
                        "%s::%s" % (s["impl"], s["name"]) if s.get("impl") else None
                    )
                    if s["name"].lower() == low or (qual and qual.lower() == low):
                        if m["name"] not in matched_mods:
                            matched_mods.append(m["name"])
        if matched_mods:
            for mname in matched_mods:
                print(
                    "RESOLVED SYMBOL  %s -> module %s  (for function-level callers, run: python3 tools/bbgraph.py callers %s)"
                    % (tok, mname, tok)
                )
                raw_targets.append(mname)
        else:
            return die("unknown module or symbol: %s" % tok)

    targets = list(dict.fromkeys(raw_targets))
    imp = compute_impact(nodes, edges, dag, targets)
    stratum_map = dag.get("stratum", dag.get("strata", {}))

    print("IMPACT ANALYSIS  %s" % ", ".join(targets))
    print("\nTIER 1 -- TARGET MODULES  (%d)" % len(targets))
    for t in targets:
        m = mods[t]
        st = stratum_map.get(t, 0)
        mut_sites = m.get("mutation_sites", 0)
        test_scope_set = set(m.get("test_scope", ()))
        prod_schemas = [s for s in m.get("declares", ()) if s not in test_scope_set]
        test_schemas = list(m.get("test_scope", ()))
        t_schemas = {"schema:" + s for s in m.get("declares", ())}
        t_vecs = sorted(
            {
                e["from"].split(":", 1)[1]
                for e in edges
                if e["kind"] == "vectors"
                and e["to"] in t_schemas
                and e["from"] != "tool:metric_registry"
            }
        )
        t_proofs = sorted(
            {
                e["from"].split(":", 1)[1]
                for e in edges
                if e["kind"] == "proves" and e["to"] == m["id"]
            }
        )
        print(
            "  %-24s %s  cfg(%s), stratum %d, %d mutation sites, %d tests"
            % (t, m["path"], m["gate"], st, mut_sites, m["tests"])
        )
        if prod_schemas:
            schema_str = ", ".join(prod_schemas)
            if test_schemas:
                schema_str += " (+%d test-scope schema%s: %s)" % (
                    len(test_schemas),
                    "" if len(test_schemas) == 1 else "s",
                    ", ".join(test_schemas),
                )
        elif test_schemas:
            schema_str = "none in prod (+%d test-scope schema%s: %s)" % (
                len(test_schemas),
                "" if len(test_schemas) == 1 else "s",
                ", ".join(test_schemas),
            )
        else:
            schema_str = "none"
        print("    schemas:        %s" % schema_str)
        print("    vectors:        %s" % (", ".join(t_vecs) if t_vecs else "none"))
        print("    proofs:         %s" % (", ".join(t_proofs) if t_proofs else "none"))
        if len(targets) > 1:
            other_targets = set(targets) - {t}
            intra_prod = sorted(set(m.get("deps", ())) & other_targets)
            intra_test = sorted(set(m.get("test_deps", ())) & other_targets)
            intra_parts = []
            for d in intra_prod:
                syms = m.get("symbol_deps", {}).get(d, [])
                sym_s = "[%s]" % fmt_syms(syms) if syms else ""
                tsyms = m.get("test_symbol_deps", {}).get(d, [])
                if tsyms:
                    sym_s += " +#[cfg(test)]: [%s]" % fmt_syms(tsyms)
                intra_parts.append("%s%s" % (d, sym_s))
            for d in intra_test:
                tsyms = m.get("test_symbol_deps", {}).get(d, [])
                sym_s = "[%s]" % fmt_syms(tsyms) if tsyms else ""
                intra_parts.append("%s%s (#[cfg(test)] only)" % (d, sym_s))
            print(
                "    intra-target:   %s"
                % (
                    ", ".join(intra_parts)
                    if intra_parts
                    else "none (independent in target set)"
                )
            )

    print(
        "\nTIER 2 -- DIRECT 1-HOP DEPENDENTS  (%d prod, %d test-only)"
        % (len(imp["direct_prod"]), len(imp["direct_test_only"]))
    )
    print(
        "  PRODUCTION 1-HOP DEPENDENTS (depends_on)  (%d)"
        % len(imp["direct_prod"])
    )
    if imp["direct_prod"]:
        for d in imp["direct_prod"]:
            dm = mods[d]
            parts = []
            for t in targets:
                if t in dm.get("deps", ()):
                    syms = dm.get("symbol_deps", {}).get(t, [])
                    s_txt = fmt_syms(syms) if syms else "(module)"
                    tsyms = dm.get("test_symbol_deps", {}).get(t, [])
                    if tsyms:
                        s_txt += " +#[cfg(test)]: %s" % fmt_syms(tsyms)
                    parts.append("%s: %s" % (t, s_txt))
                elif t in dm.get("test_deps", ()):
                    tsyms = dm.get("test_symbol_deps", {}).get(t, [])
                    s_txt = fmt_syms(tsyms) if tsyms else "(module)"
                    parts.append("%s (#[cfg(test)]): %s" % (t, s_txt))
            print(
                "    %-22s %-36s cfg(%s)  <- %s"
                % (d, dm["path"], dm["gate"], "; ".join(parts))
            )
    else:
        print("    (none)")

    print(
        "\n  TEST-ONLY 1-HOP DEPENDENTS (tests_with)  (%d)"
        % len(imp["direct_test_only"])
    )
    if imp["direct_test_only"]:
        trans_set = set(imp["trans_2plus"])
        for d in imp["direct_test_only"]:
            dm = mods[d]
            parts = []
            for t in targets:
                if t in dm.get("test_deps", ()) or t in dm.get("test_symbol_deps", {}):
                    tsyms = dm.get("test_symbol_deps", {}).get(t, [])
                    s_txt = fmt_syms(tsyms) if tsyms else "(module)"
                    parts.append("%s: %s" % (t, s_txt))
            note = "  (also 2+ hop prod dependent)" if d in trans_set else ""
            print(
                "    %-22s %-36s cfg(%s)  <- %s%s"
                % (d, dm["path"], dm["gate"], "; ".join(parts), note)
            )
    else:
        print("    (none)")

    print(
        "\nTIER 3 -- TRANSITIVE DEPENDENTS (2+ HOPS)  (%d)"
        % len(imp["trans_2plus"])
    )
    if imp["trans_2plus"]:
        by_stratum: dict[int, list[str]] = {}
        for mname in imp["trans_2plus"]:
            st = stratum_map.get(mname, 0)
            by_stratum.setdefault(st, []).append(mname)
        for st in sorted(by_stratum):
            print("  stratum %2d: %s" % (st, ", ".join(sorted(by_stratum[st]))))
    else:
        print("  (none)")

    print("\nTIER 4 -- VERIFICATION RECIPE")
    scope_mods = (
        set(targets) | set(imp["direct_prod"]) | set(imp["direct_test_only"])
    )
    needs_research = any(
        mods[m].get("crate") == "research"
        or "protocol-research" in mods[m].get("gate", "")
        or mods[m].get("name") == "main"
        for m in scope_mods
        if m in mods
    )
    cargo_cmd = (
        "cargo test --workspace --features protocol-research"
        if needs_research
        else "cargo test --workspace"
    )
    print("  rust tests:       %s" % cargo_cmd)

    if imp["direct_vecs"]:
        if len(imp["direct_vecs"]) <= 8:
            vec_cmds = " && ".join(
                "python3 -B tools/%s.py" % v for v in imp["direct_vecs"]
            )
            print(
                "  direct vectors:   %s  (%s)"
                % (", ".join(imp["direct_vecs"]), vec_cmds)
            )
        else:
            print(
                "  direct vectors:   %s  (%d suites -> python3 tools/bbverify.py full)"
                % (", ".join(imp["direct_vecs"]), len(imp["direct_vecs"]))
            )
    else:
        print("  direct vectors:   none")

    if imp["hop1_vecs"]:
        if len(imp["hop1_vecs"]) <= 8:
            vec_cmds = " && ".join(
                "python3 -B tools/%s.py" % v for v in imp["hop1_vecs"]
            )
            print(
                "  1-hop vectors:    %s  (%s)"
                % (", ".join(imp["hop1_vecs"]), vec_cmds)
            )
        else:
            print(
                "  1-hop vectors:    %s  (%d suites -> python3 tools/bbverify.py full)"
                % (", ".join(imp["hop1_vecs"]), len(imp["hop1_vecs"]))
            )
    else:
        print("  1-hop vectors:    none")

    if imp["mut_targets"] and imp["mut_1hop"]:
        print(
            "  mutation:         python3 tools/bbmutate.py %s"
            % " ".join(imp["mut_targets"])
        )
        print(
            "                    python3 tools/bbmutate.py %s  # 1-hop encoders for 0-site target(s)"
            % " ".join(imp["mut_1hop"])
        )
    elif imp["mut_targets"]:
        print(
            "  mutation:         python3 tools/bbmutate.py %s"
            % " ".join(imp["mut_targets"])
        )
    elif imp["mut_1hop"]:
        print(
            "  mutation:         python3 tools/bbmutate.py %s  # 1-hop encoders for 0-site target(s)"
            % " ".join(imp["mut_1hop"])
        )
    else:
        print(
            "  mutation:         none (0 canonical encoder sites in target or 1-hop scope)"
        )

    if imp["proofs"]:
        print(
            "  lean proofs:      %s  ((cd proof && lake build))"
            % ", ".join(imp["proofs"])
        )
    else:
        print("  lean proofs:      none")
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


def compute_waves(nodes: list[dict], names: list[str]) -> list[list[str]]:
    """Partition target modules into parallel execution waves.

    Uses both production `deps` and `#[cfg(test)]` `test_deps` within the target
    set so a module whose unit tests import a sibling module is scheduled in a
    strictly later wave.
    """
    by_name = {n["name"]: n for n in nodes if n["kind"] == "module"}
    target = {m for m in names if m in by_name}
    prereqs: dict[str, set[str]] = {}
    for name in target:
        mod = by_name[name]
        coupled = set(mod.get("deps", ())) | set(mod.get("test_deps", ()))
        prereqs[name] = (coupled & target) - {name}
    remaining = set(target)
    waves: list[list[str]] = []
    while remaining:
        ready = sorted(m for m in remaining if not (prereqs[m] & remaining))
        if not ready:
            waves.append(sorted(remaining))
            break
        waves.append(ready)
        remaining -= set(ready)
    return waves


def cmd_wave(query: str) -> int:
    nodes, _subs, dag = load()
    edges = load_edges()
    by_name = {n["name"]: n for n in nodes if n["kind"] == "module"}
    by_id = {n["id"]: n for n in nodes}
    parts = [p for p in re.split(r"[\s,]+", query.strip()) if p]
    if not parts:
        return die("wave needs module names (e.g. bbgraph.py wave world camera render)")
    unknown = [p for p in parts if p not in by_name]
    if unknown:
        return die("unknown module(s): %s" % ", ".join(unknown))
    target = set(parts)
    waves = compute_waves(nodes, parts)
    print("WAVE TOPOLOGY   %d modules across %d parallel wave(s)" % (len(target), len(waves)))
    print("Includes both production `deps` and `#[cfg(test)]` `test_deps` so every")
    print("wave compiles and passes `cargo test` before the next wave begins.")
    for idx, wave in enumerate(waves, 1):
        print("\n  WAVE %d  (%d parallel subagent lock%s)"
              % (idx, len(wave), "" if len(wave) == 1 else "s"))
        for name in wave:
            mod = by_name[name]
            prod_in = sorted(set(mod.get("deps", ())) & target)
            test_in = sorted(set(mod.get("test_deps", ())) & target)
            prod_waits = []
            for d in prod_in:
                syms = mod.get("symbol_deps", {}).get(d, [])
                prod_waits.append("%s[%s]" % (d, fmt_syms(syms)) if syms else d)
            test_waits = []
            for d in test_in:
                tsyms = mod.get("test_symbol_deps", {}).get(d, [])
                test_waits.append(
                    "%s[%s] (test)" % (d, fmt_syms(tsyms)) if tsyms else "%s (test)" % d
                )
            waits = prod_waits + test_waits
            wait_str = "waits on: " + ", ".join(waits) if waits else "root in wave set"
            print("    %-22s %-36s cfg(%s), %d dependents"
                  % (name, mod["path"], mod["gate"], load_bearing(dag, mod)))
            print("      %s" % wait_str)
            declared = {"schema:" + s for s in mod.get("declares", ())}
            vecs = sorted({
                e["from"].split(":", 1)[1]
                for e in edges
                if e["kind"] == "vectors" and e["to"] in declared
            })
            if vecs:
                print("      vectors:  %s" % ", ".join(vecs))
            proofs = sorted({
                e["from"].split(":", 1)[1]
                for e in edges
                if e["kind"] == "proves" and e["to"] == mod["id"]
            })
            if proofs:
                print("      proves:   %s" % ", ".join(proofs))
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
    before = {p.name: p.read_bytes() for p in sorted(INDEX.glob("*")) if p.is_file()}
    result = subprocess.run(
        [sys.executable, str(ROOT / "tools" / "bbindex.py")],
        capture_output=True, text=True,
    )
    after = {p.name: p.read_bytes() for p in sorted(INDEX.glob("*")) if p.is_file()}
    for line in result.stdout.splitlines():
        if "gate" in line or "!" in line:
            print(" " + line.strip())
    if result.stderr.strip():
        print(result.stderr.rstrip())
    accounting = result.returncode == 0

    # Staleness. The reference method runs this as a CI job that regenerates the
    # artifacts and fails on any diff; bitblock has no CI, so it runs here.
    # The run above already regenerated index/ in place, so this compares the
    # bytes as they were on entry against the bytes the tree produces now. A
    # failure therefore leaves the tree ALREADY corrected -- the fix is to
    # commit the diff, not to re-run anything.
    print("\nSTALENESS GATE")
    drifted = sorted(
        name for name in set(before) | set(after) if before.get(name) != after.get(name)
    )
    if drifted:
        print("  FAIL  index/ did not match the tree: %s" % ", ".join(drifted))
        print("        It has just been regenerated. Commit the diff.")
    else:
        print("  pass  index/ regenerates byte-identically from the tree")
    fresh = not drifted

    print("\n%s" % ("ALL GATES PASS" if accounting and fresh else "GATES FAILED"))
    return 0 if accounting and fresh else 1


def reachable(dag: dict, module: str) -> set:
    seen: set = set()
    stack = list(dag["adjacency"].get(module, ()))
    while stack:
        node = stack.pop()
        if node in seen:
            continue
        seen.add(node)
        stack.extend(dag["adjacency"].get(node, ()))
    return seen


GATE_RANK = {"default": 0, "binary": 0, "windows": 1}


def gate_rank(gate: str) -> int:
    if gate in GATE_RANK:
        return GATE_RANK[gate]
    return 2 if "protocol-research" in gate else 1


def cmd_dup(scope: str) -> int:
    """Repeated regions, with the home a shared helper could legally take.

    The home has to be a module BOTH sides already reach, or extracting adds
    dependency edges and can cycle; and it has to sit under a cfg gate at least
    as permissive as the more permissive user, or a default build would end up
    depending on research-gated code. That is the routing question, and the DAG
    is what answers it.
    """
    scope = (scope or "production").strip().lower()
    if scope not in ("production", "test"):
        scope = "test" if scope.startswith("t") else "production"
    reuse = json.loads(REUSE.read_text(encoding="utf-8"))
    nodes, _subs, dag = load()
    mods = {n["name"]: n for n in nodes if n["kind"] == "module"}
    data = reuse["duplication"][scope]

    print("REPEATED REGIONS -- %s scope" % scope)
    print("  %d of %d code lines sit in a repeated region (%.1f%%)"
          % (data["duplicated_lines"], data["code_lines"],
             100.0 * data["duplicated_lines"] / max(data["code_lines"], 1)))
    if scope == "production":
        other = reuse["duplication"]["test"]
        print("  test scope, counted separately: %d of %d (%.1f%%)"
              % (other["duplicated_lines"], other["code_lines"],
                 100.0 * other["duplicated_lines"] / max(other["code_lines"], 1)))
    print()
    for region in data["regions"][:12]:
        left = region["left"].split(":")[0]
        right = region["right"].split(":")[0]
        print("  %3d lines  %-28s <-> %s" % (region["lines"], region["left"], region["right"]))
        a, b = mods.get(left), mods.get(right)
        if not (a and b):
            continue
        weakest = min(gate_rank(a["gate"]), gate_rank(b["gate"]))
        # If one side already depends on the other, the depended-upon side is
        # the home: the dependent reaches it for free and no edge is added.
        # Taking only the intersection of what BOTH reach misses this and sends
        # the pair to a distant common ancestor -- output_color depends on
        # output_topology, so their shared code belongs in output_topology, but
        # the intersection surfaces `protocol`, which would drag research-gated
        # helpers into every default build.
        if left in reachable(dag, right):
            homes = [left]
        elif right in reachable(dag, left):
            homes = [right]
        else:
            common = reachable(dag, left) & reachable(dag, right)
            homes = [c for c in common if gate_rank(mods[c]["gate"]) <= weakest]
        homes = [h for h in homes if gate_rank(mods[h]["gate"]) <= weakest]
        homes.sort(key=lambda m: (-dag["stratum"].get(m, 0), m))
        print("             home: %s" % (", ".join(homes[:2]) if homes
                                         else "none shared -- a new module would be needed"))
    print()
    print("A home must be a module both sides already reach, under a gate at")
    print("least as permissive as the more permissive user.")
    return 0


def cmd_dead() -> int:
    reuse = json.loads(REUSE.read_text(encoding="utf-8"))
    dead = reuse["dead"]
    print("PUBLIC SURFACE  %d items" % dead["pub_items"])
    print()
    print("NOTHING IN THE TREE NAMES THESE  (%d)" % len(dead["unreferenced"]))
    for row in dead["unreferenced"]:
        print("  %-38s %s" % (row["item"], ", ".join(row["modules"])))
    print()
    print("USED ONLY INSIDE THEIR OWN MODULE  (%d)  -- could drop `pub`"
          % len(dead["module_private_candidates"]))
    for row in dead["module_private_candidates"][:20]:
        print("  %-38s %s" % (row["item"], ", ".join(row["modules"])))
    rest = len(dead["module_private_candidates"]) - 20
    if rest > 0:
        print("  ... and %d more" % rest)
    print()
    print("clippy cannot report either list: a `pub` item in a library is")
    print("reachable by definition. Confirm by deleting and compiling -- a name")
    print("can also be reached through a trait impl or a macro.")
    return 0


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
        print("  %-46s %3d" % ("cfg(%s)" % gate, count))
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


def cmd_coverage() -> int:
    nodes, _subs, _dag = load()
    coverage = json.loads(COVERAGE.read_text(encoding="utf-8"))
    modules = [n for n in nodes if n["kind"] == "module"]
    mod_total = len(modules)
    mod_doc = sum(1 for m in modules if m.get("doc"))
    pub_total = sum(m.get("pub_symbol_count", 0) for m in modules)
    pub_doc = sum(m.get("pub_documented", 0) for m in modules)
    schemas_total = coverage["schemas"]
    schemas_vec = coverage["schemas_vectored"]

    print("DOCUMENTATION & VECTOR COVERAGE")
    print(
        "  module //! docs      %4d / %4d (%3d%%)"
        % (mod_doc, mod_total, round(100.0 * mod_doc / max(mod_total, 1)))
    )
    print(
        "  public symbol ///    %4d / %4d (%3d%%)"
        % (pub_doc, pub_total, round(100.0 * pub_doc / max(pub_total, 1)))
    )
    print(
        "  protocol vectors     %4d / %4d (%3d%%)"
        % (schemas_vec, schemas_total, round(100.0 * schemas_vec / max(schemas_total, 1)))
    )
    print("\nPER-MODULE PUBLIC SYMBOL DOCS (/// on pub items)")
    ordered = sorted(
        modules,
        key=lambda m: (
            m.get("pub_symbol_count", 0) == 0,
            m.get("pub_documented", 0) / max(m.get("pub_symbol_count", 1), 1),
            m["name"],
        ),
    )
    for m in ordered:
        pc = m.get("pub_symbol_count", 0)
        pd = m.get("pub_documented", 0)
        if pc == 0:
            print("  %-26s   0 /   0 (n/a)   %s" % (m["name"], m["path"]))
        else:
            pct = round(100.0 * pd / pc)
            warn = "  <-- use sym/type/callers here" if pct < 25 else ""
            print(
                "  %-26s %3d / %3d (%3d%%)  %s%s"
                % (m["name"], pd, pc, pct, m["path"], warn)
            )
    return 0


def tokens_from_module_without_summary(m: dict) -> list[str]:
    doc = m.get("doc", "")
    summary = m.get("summary", "")
    if summary:
        rem_doc = doc[len(summary) :] if doc.startswith(summary) else doc.replace(summary, "", 1)
    else:
        rem_doc = doc
    return sorted(
        set(
            tokens(m["name"].replace("_", " "))
            + tokens(rem_doc)
            + tokens(" ".join(m.get("pub_items", ())))
            + tokens(" ".join(m.get("declares", ())))
        )
    )


def compute_eval_rows(nodes: list[dict], subs: list[dict], dag: dict) -> list[dict]:
    mod_nodes = [n for n in nodes if n["kind"] == "module"]

    def score_holdout(cases: list[tuple[dict, str]]) -> tuple[int, int, int, int, int]:
        n = len(cases)
        s1 = s3 = h1 = h3 = 0
        for m, q in cases:
            scored, *_ = rank(mod_nodes, subs, dag, q)
            top_mods = [node["name"] for _sc, node in scored]
            strict_ok = {m["name"]}
            hop1_ok = (
                {m["name"]}
                | set(m.get("deps", ()))
                | set(m.get("test_deps", ()))
            )
            if top_mods and top_mods[0] in strict_ok:
                s1 += 1
            if set(top_mods[:3]) & strict_ok:
                s3 += 1
            if top_mods and top_mods[0] in hop1_ok:
                h1 += 1
            if set(top_mods[:3]) & hop1_ok:
                h3 += 1
        return n, s1, s3, h1, h3

    # 1. module-doc holdout (strip summary from module's own terms)
    md_n = md_s1 = md_s3 = md_h1 = md_h3 = 0
    for m in mod_nodes:
        summary = m.get("summary", "")
        if not summary:
            continue
        md_n += 1
        orig_terms = m.get("terms", [])
        m["terms"] = tokens_from_module_without_summary(m)
        try:
            scored, *_ = rank(mod_nodes, subs, dag, summary)
        finally:
            m["terms"] = orig_terms
        top_mods = [node["name"] for _sc, node in scored]
        strict_ok = {m["name"]}
        hop1_ok = (
            {m["name"]}
            | set(m.get("deps", ()))
            | set(m.get("test_deps", ()))
        )
        if top_mods and top_mods[0] in strict_ok:
            md_s1 += 1
        if set(top_mods[:3]) & strict_ok:
            md_s3 += 1
        if top_mods and top_mods[0] in hop1_ok:
            md_h1 += 1
        if set(top_mods[:3]) & hop1_ok:
            md_h3 += 1

    # 2. symbol-doc holdout
    sym_cases = [
        (m, s["doc"])
        for m in mod_nodes
        for s in m.get("symbols", ())
        if s.get("pub")
        and s.get("doc")
        and len([t for t in tokens(s["doc"]) if t not in KIND_WORDS]) >= 3
    ]
    sd_n, sd_s1, sd_s3, sd_h1, sd_h3 = score_holdout(sym_cases)

    # 3. test-name holdout (all modules and library-module subset)
    test_all_cases = [
        (m, tf["name"].replace("_", " "))
        for m in mod_nodes
        for tf in m.get("test_fns", ())
        if len(tokens(tf["name"].replace("_", " "))) >= 2
    ]
    test_lib_cases = [(m, q) for m, q in test_all_cases if m["name"] != "main"]
    ta_n, ta_s1, ta_s3, ta_h1, ta_h3 = score_holdout(test_all_cases)
    tl_n, tl_s1, tl_s3, tl_h1, tl_h3 = score_holdout(test_lib_cases)

    def make_row(label: str, n: int, s1: int, s3: int, h1: int, h3: int) -> dict:
        denom = max(n, 1)
        return {
            "label": label,
            "n": n,
            "s1": s1,
            "s3": s3,
            "h1": h1,
            "h3": h3,
            "s1_pct": round(100.0 * s1 / denom, 1),
            "s3_pct": round(100.0 * s3 / denom, 1),
            "h1_pct": round(100.0 * h1 / denom, 1),
            "h3_pct": round(100.0 * h3 / denom, 1),
        }

    return [
        make_row("module-doc (//! summary)", md_n, md_s1, md_s3, md_h1, md_h3),
        make_row("symbol-doc (pub /// >=3t)", sd_n, sd_s1, sd_s3, sd_h1, sd_h3),
        make_row("test-name (all #[test])", ta_n, ta_s1, ta_s3, ta_h1, ta_h3),
        make_row("test-name (lib, no main)", tl_n, tl_s1, tl_s3, tl_h1, tl_h3),
    ]


def cmd_eval() -> int:
    nodes, subs, dag = load()
    rows = compute_eval_rows(nodes, subs, dag)

    print("SELF-SUPERVISED HOLDOUT RETRIEVAL BENCHMARK (37 modules)")
    print("  %-28s %4s   %-15s   %-15s" % ("holdout set", "N", "strict T1 / T3", "1-hop T1 / T3"))
    for r in rows:
        print(
            "  %-28s %4d   %5.1f%% / %5.1f%%   %5.1f%% / %5.1f%%"
            % (r["label"], r["n"], r["s1_pct"], r["s3_pct"], r["h1_pct"], r["h3_pct"])
        )
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
        # The leading MODULE, not the leading node: for a code question that is
        # the file the caller opens, and it does not churn when a doc section
        # is reworded.
        top = next((n["name"] for _s, n in scored if n["kind"] == "module"), None)
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

    reuse_spec = spec.get("reuse")
    if reuse_spec:
        reuse = json.loads(REUSE.read_text(encoding="utf-8"))
        actual = {
            "production_duplicated_lines":
                reuse["duplication"]["production"]["duplicated_lines"],
            "test_duplicated_lines":
                reuse["duplication"]["test"]["duplicated_lines"],
            "unreferenced_pub_items": len(reuse["dead"]["unreferenced"]),
        }
        for key, expected in reuse_spec.items():
            checks += 1
            if actual[key] != expected:
                failures.append(
                    "reuse %s -> %r, expected %r" % (key, actual[key], expected)
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

    for case in spec.get("waves", []):
        checks += 1
        got = compute_waves(nodes, case["modules"])
        if got != case["expect"]:
            failures.append(
                "wave %r -> %r, expected %r" % (case["modules"], got, case["expect"])
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

    mod_by_name = {n["name"]: n for n in nodes if n["kind"] == "module"}
    edges = load_edges()

    for case in spec.get("symbol_deps", []):
        checks += 1
        mod = mod_by_name.get(case["module"], {})
        got = mod.get("symbol_deps", {}).get(case["dep"], [])
        if got != case["expect"]:
            failures.append(
                "symbol_deps %s -> %s: got %r, expected %r"
                % (case["module"], case["dep"], got, case["expect"])
            )

    for case in spec.get("test_symbol_deps", []):
        checks += 1
        mod = mod_by_name.get(case["module"], {})
        got_test = mod.get("test_symbol_deps", {}).get(case["dep"], [])
        got_prod = mod.get("symbol_deps", {}).get(case["dep"], [])
        if "expect" in case and got_test != case["expect"]:
            failures.append(
                "test_symbol_deps %s -> %s: got %r, expected %r"
                % (case["module"], case["dep"], got_test, case["expect"])
            )
        if "expect_include" in case and not all(
            s in got_test for s in case["expect_include"]
        ):
            failures.append(
                "test_symbol_deps %s -> %s: got %r, expected to include %r"
                % (case["module"], case["dep"], got_test, case["expect_include"])
            )
        for s in case.get("not_in_prod", []):
            if s in got_prod:
                failures.append(
                    "test_symbol_deps %s -> %s: %r leaked into prod symbol_deps %r"
                    % (case["module"], case["dep"], s, got_prod)
                )

    sym_spec = spec.get("symbols", {})
    if sym_spec.get("sig_type_consistency"):
        checks += 1
        bad_sigs = []
        for m in mod_by_name.values():
            for s in m.get("symbols", ()):
                if s.get("kind") != "fn":
                    continue
                sig = s.get("sig", "")
                for t in list(s.get("takes", ())) + list(s.get("returns", ())):
                    if not (
                        t in sig
                        or (t == s.get("impl") and ("Self" in sig or "self" in sig))
                    ):
                        bad_sigs.append((m["name"], s["name"], t, sig))
        if bad_sigs:
            failures.append(
                "symbols sig_type_consistency had %d violation(s): %r"
                % (len(bad_sigs), bad_sigs[:5])
            )

    if sym_spec.get("test_fns_match_coverage"):
        checks += 1
        got_tests = sum(len(m.get("test_fns", ())) for m in mod_by_name.values())
        exp_tests = coverage.get("tests")
        if got_tests != exp_tests:
            failures.append(
                "symbols test_fns_match_coverage -> %r, expected %r"
                % (got_tests, exp_tests)
            )

    if "pub_counts" in sym_spec:
        checks += 1
        pub_counts = sym_spec["pub_counts"]
        got_total = sum(m.get("pub_symbol_count", 0) for m in mod_by_name.values())
        got_doc = sum(m.get("pub_documented", 0) for m in mod_by_name.values())
        if (
            got_total != pub_counts["total"]
            or got_doc != pub_counts["documented"]
        ):
            failures.append(
                "symbols pub_counts -> total=%r documented=%r, expected total=%r documented=%r"
                % (
                    got_total,
                    got_doc,
                    pub_counts["total"],
                    pub_counts["documented"],
                )
            )

    for tc in sym_spec.get("type_checks", []):
        checks += 1
        tname = tc["type"]
        if "returns_include" in tc:
            ret_fns = {
                s["name"]
                for m in mod_by_name.values()
                for s in m.get("symbols", ())
                if s.get("kind") == "fn" and tname in s.get("returns", ())
            }
            if not all(fn in ret_fns for fn in tc["returns_include"]):
                failures.append(
                    "type_check %s returns_include %r not in %r"
                    % (tname, tc["returns_include"], sorted(ret_fns))
                )
        if "method_count" in tc:
            mcount = sum(
                1
                for m in mod_by_name.values()
                for s in m.get("symbols", ())
                if s.get("kind") == "fn" and s.get("impl") == tname
            )
            if mcount != tc["method_count"]:
                failures.append(
                    "type_check %s method_count -> %d, expected %d"
                    % (tname, mcount, tc["method_count"])
                )
        if "min_returning_fns" in tc:
            rcount = sum(
                1
                for m in mod_by_name.values()
                for s in m.get("symbols", ())
                if s.get("kind") == "fn" and tname in s.get("returns", ())
            )
            if rcount < tc["min_returning_fns"]:
                failures.append(
                    "type_check %s returning_fns -> %d, expected >= %d"
                    % (tname, rcount, tc["min_returning_fns"])
                )

    for case in spec.get("callers", []):
        checks += 1
        target = case["target"]
        prod_mods = sorted(
            {
                m["name"]
                for m in mod_by_name.values()
                for s in m.get("symbols", ())
                if s.get("kind") == "fn" and target in s.get("calls", ())
            }
        )
        test_mods = sorted(
            {
                m["name"]
                for m in mod_by_name.values()
                for tf in m.get("test_fns", ())
                if target in tf.get("calls", ())
            }
        )
        prod_callers = sorted(
            {
                (
                    "%s::%s::%s" % (m["name"], s["impl"], s["name"])
                    if s.get("impl")
                    else "%s::%s" % (m["name"], s["name"])
                )
                for m in mod_by_name.values()
                for s in m.get("symbols", ())
                if s.get("kind") == "fn" and target in s.get("calls", ())
            }
        )
        if "prod_modules" in case and prod_mods != case["prod_modules"]:
            failures.append(
                "callers %s prod_modules -> %r, expected %r"
                % (target, prod_mods, case["prod_modules"])
            )
        if "test_modules" in case and test_mods != case["test_modules"]:
            failures.append(
                "callers %s test_modules -> %r, expected %r"
                % (target, test_mods, case["test_modules"])
            )
        if "prod_callers" in case and prod_callers != case["prod_callers"]:
            failures.append(
                "callers %s prod_callers -> %r, expected %r"
                % (target, prod_callers, case["prod_callers"])
            )

    for case in spec.get("impact", []):
        checks += 1
        if "module" in case:
            mname = case["module"]
            imp = compute_impact(nodes, edges, dag, [mname])
            proof_set = set(imp["proofs"]) | {
                Path(p).stem.lower() for p in imp["proofs"]
            }
            if "direct_dependents_include" in case and not all(
                d in imp["direct_prod"] for d in case["direct_dependents_include"]
            ):
                failures.append(
                    "impact %s direct_prod %r missing %r"
                    % (mname, imp["direct_prod"], case["direct_dependents_include"])
                )
            if "proofs_include" in case and not all(
                p in proof_set for p in case["proofs_include"]
            ):
                failures.append(
                    "impact %s proofs %r missing %r"
                    % (mname, sorted(proof_set), case["proofs_include"])
                )
            if "mutation_sites" in case:
                got_sites = mod_by_name.get(mname, {}).get("mutation_sites", 0)
                if got_sites != case["mutation_sites"]:
                    failures.append(
                        "impact %s mutation_sites -> %r, expected %r"
                        % (mname, got_sites, case["mutation_sites"])
                    )
        if "total_mutation_sites" in case:
            tot_sites = sum(
                m.get("mutation_sites", 0) for m in mod_by_name.values()
            )
            mods_with_sites = sum(
                1 for m in mod_by_name.values() if m.get("mutation_sites", 0) > 0
            )
            if tot_sites != case["total_mutation_sites"]:
                failures.append(
                    "impact total_mutation_sites -> %r, expected %r"
                    % (tot_sites, case["total_mutation_sites"])
                )
            if (
                "modules_with_mutation_sites" in case
                and mods_with_sites != case["modules_with_mutation_sites"]
            ):
                failures.append(
                    "impact modules_with_mutation_sites -> %r, expected %r"
                    % (mods_with_sites, case["modules_with_mutation_sites"])
                )
            for mname, exp_sites in case.get("module_sites", {}).items():
                got_sites = mod_by_name.get(mname, {}).get("mutation_sites", 0)
                if got_sites != exp_sites:
                    failures.append(
                        "impact module_sites[%s] -> %r, expected %r"
                        % (mname, got_sites, exp_sites)
                    )

    for case in spec.get("pinned_tags", []):
        checks += 1
        hit = resolve_tag(nodes, case["tag"])
        got = hit["ids"] if hit else []
        if got != case["expect"]:
            failures.append(
                "pinned_tags %s -> %r, expected %r"
                % (case["tag"], got, case["expect"])
            )

    print("selftest  %d checks, %d failed" % (checks, len(failures)))
    for line in failures:
        print("  FAIL  " + line)
    if not failures:
        print("  all expectations hold")
    return 1 if failures else 0


def build_html_payload() -> dict:
    nodes, subs, dag = load()
    edges = load_edges()
    coverage = json.loads(COVERAGE.read_text(encoding="utf-8"))
    reuse = json.loads(REUSE.read_text(encoding="utf-8"))
    selftest_spec = json.loads(SELFTEST.read_text(encoding="utf-8"))
    eval_rows = compute_eval_rows(nodes, subs, dag)

    mods = {n["name"]: n for n in nodes if n["kind"] == "module"}
    for scope in ("production", "test"):
        for region in reuse.get("duplication", {}).get(scope, {}).get("regions", []):
            left = region["left"].split(":")[0]
            right = region["right"].split(":")[0]
            a, b = mods.get(left), mods.get(right)
            if not (a and b):
                region["homes"] = []
                continue
            weakest = min(gate_rank(a["gate"]), gate_rank(b["gate"]))
            if left in reachable(dag, right):
                homes = [left]
            elif right in reachable(dag, left):
                homes = [right]
            else:
                common = reachable(dag, left) & reachable(dag, right)
                homes = [c for c in common if gate_rank(mods[c]["gate"]) <= weakest]
            homes = [h for h in homes if gate_rank(mods[h]["gate"]) <= weakest]
            homes.sort(key=lambda m: (-dag["stratum"].get(m, 0), m))
            region["homes"] = homes[:3]

    return {
        "nodes": nodes,
        "edges": edges,
        "dag": dag,
        "subs": subs,
        "coverage": coverage,
        "reuse": reuse,
        "selftest": selftest_spec,
        "eval_rows": eval_rows,
        "stop": sorted(STOP),
        "kind_words": KIND_WORDS,
    }


HTML_TEMPLATE = r"""<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Bitblock — Knowledge Graph &amp; GraphRAG Auditor Observatory</title>
  <script src="https://www.gstatic.com/antigravity/web/dev/tailwindcss.min.js"></script>
  <style>
    *, *::before, *::after { box-sizing: border-box; }
    body {
      margin: 0;
      background: var(--background);
      color: var(--foreground);
      font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
      font-size: 13px;
      line-height: 1.45;
    }
    code, pre, .mono {
      font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace;
    }
    .surface-card {
      background: var(--card);
      color: var(--foreground);
      border: 1px solid var(--border);
      border-radius: 10px;
    }
    .surface-subtle {
      background: color-mix(in srgb, var(--card) 75%, var(--background) 25%);
      border: 1px solid var(--border);
      border-radius: 8px;
    }
    .muted { color: var(--muted-foreground); }
    .pill {
      display: inline-flex;
      align-items: center;
      gap: 4px;
      padding: 2px 8px;
      border-radius: 9999px;
      font-size: 12px;
      font-weight: 500;
      border: 1px solid var(--border);
      background: color-mix(in srgb, var(--card) 85%, var(--foreground) 6%);
      color: var(--foreground);
      white-space: nowrap;
    }
    .pill-core { border-color: rgba(16, 185, 129, 0.45); color: #10b981; background: rgba(16, 185, 129, 0.1); }
    .pill-research { border-color: rgba(59, 130, 246, 0.45); color: #3b82f6; background: rgba(59, 130, 246, 0.1); }
    .pill-binary { border-color: rgba(245, 158, 11, 0.45); color: #f59e0b; background: rgba(245, 158, 11, 0.1); }
    .pill-win { border-color: rgba(168, 85, 247, 0.45); color: #a855f7; background: rgba(168, 85, 247, 0.1); }
    .pill-high { border-color: rgba(16, 185, 129, 0.55); color: #10b981; background: rgba(16, 185, 129, 0.14); font-weight: 600; }
    .pill-low { border-color: rgba(245, 158, 11, 0.55); color: #f59e0b; background: rgba(245, 158, 11, 0.14); font-weight: 600; }
    .pill-danger { border-color: rgba(239, 68, 68, 0.55); color: #ef4444; background: rgba(239, 68, 68, 0.14); font-weight: 600; }
    .btn {
      cursor: pointer;
      display: inline-flex;
      align-items: center;
      gap: 6px;
      padding: 6px 12px;
      border-radius: 7px;
      font-size: 12px;
      font-weight: 500;
      border: 1px solid var(--border);
      background: var(--card);
      color: var(--foreground);
      transition: all 0.12s ease;
    }
    .btn:hover {
      border-color: var(--primary);
      background: color-mix(in srgb, var(--card) 82%, var(--primary) 18%);
    }
    .btn-active {
      background: var(--primary);
      color: var(--primary-foreground, #ffffff);
      border-color: var(--primary);
    }
    .tab-btn {
      cursor: pointer;
      padding: 8px 14px;
      border-radius: 8px;
      font-size: 13px;
      font-weight: 600;
      border: 1px solid transparent;
      background: transparent;
      color: var(--muted-foreground);
      transition: all 0.15s ease;
      display: inline-flex;
      align-items: center;
      gap: 6px;
    }
    .tab-btn:hover {
      color: var(--foreground);
      background: color-mix(in srgb, var(--card) 80%, var(--foreground) 6%);
    }
    .tab-btn.active {
      color: var(--foreground);
      background: var(--card);
      border-color: var(--primary);
      box-shadow: 0 1px 3px rgba(0,0,0,0.12);
    }
    .input-box {
      width: 100%;
      padding: 8px 12px;
      border-radius: 8px;
      border: 1px solid var(--border);
      background: var(--background);
      color: var(--foreground);
      font-size: 13px;
      outline: none;
    }
    .input-box:focus {
      border-color: var(--primary);
      box-shadow: 0 0 0 2px color-mix(in srgb, var(--primary) 25%, transparent);
    }
    .clickable-row {
      cursor: pointer;
      transition: background 0.12s ease, border-color 0.12s ease;
    }
    .clickable-row:hover {
      border-color: var(--primary);
      background: color-mix(in srgb, var(--card) 88%, var(--primary) 12%);
    }
    .dag-node {
      cursor: pointer;
      transition: opacity 0.15s ease, transform 0.12s ease;
    }
    .dag-node:hover rect {
      stroke-width: 2.4px;
    }
    .layout-grid {
      display: grid;
      grid-template-columns: minmax(0, 1fr) 410px;
      gap: 16px;
      align-items: start;
    }
    @media (max-width: 1180px) {
      .layout-grid { grid-template-columns: 1fr; }
    }
    .sticky-drawer {
      position: sticky;
      top: 12px;
      max-height: calc(100vh - 24px);
      overflow-y: auto;
    }
    table.audit-table {
      width: 100%;
      border-collapse: collapse;
      font-size: 12px;
    }
    table.audit-table th, table.audit-table td {
      padding: 7px 10px;
      border-bottom: 1px solid var(--border);
      text-align: left;
      vertical-align: top;
    }
    table.audit-table th {
      font-weight: 600;
      color: var(--muted-foreground);
      background: color-mix(in srgb, var(--card) 92%, var(--foreground) 4%);
      position: sticky;
      top: 0;
      z-index: 1;
    }
  </style>
</head>
<body class="antialiased p-4">
  <!-- Header & Global KPI Ribbon -->
  <header class="surface-card p-4 mb-4">
    <div class="flex flex-wrap items-center justify-between gap-3 mb-3">
      <div>
        <div class="flex items-center gap-2 flex-wrap">
          <span class="pill pill-core mono">BITBLOCK INDEX OBSERVATORY</span>
          <h1 class="text-lg font-bold tracking-tight m-0">Knowledge Graph &amp; Glass-Box GraphRAG Compiler</h1>
        </div>
        <p class="muted text-xs mt-1 mb-0">
          Compiled directly from <code>index/nodes.jsonl</code>, <code>index/edges.jsonl</code>, <code>index/dag.json</code>, <code>index/subsystems.json</code>, <code>index/reuse.json</code>, <code>index/coverage.json</code> &amp; <code>tools/bbselftest.json</code>. Zero server, 100% deterministic parity with <code>tools/bbgraph.py</code>.
        </p>
      </div>
      <div class="flex items-center gap-2">
        <span id="header-selftest-badge" class="pill pill-high mono">SELFTEST 90/90 PASS</span>
        <span id="header-gates-badge" class="pill pill-high mono">3/3 ACCOUNTING GATES</span>
        <button id="theme-toggle-btn" class="btn mono" title="Toggle standalone light/dark palette">Theme</button>
      </div>
    </div>

    <div id="kpi-ribbon" class="grid grid-cols-2 sm:grid-cols-4 lg:grid-cols-8 gap-2"></div>

    <!-- Navigation Tabs -->
    <nav class="flex flex-wrap gap-1.5 mt-4 pt-3 border-t border-[var(--border)]" role="tablist">
      <button class="tab-btn active" data-tab="ask" id="tab-btn-ask">
        <span>1. GraphRAG Search</span>
        <span class="pill mono">ask</span>
      </button>
      <button class="tab-btn" data-tab="dag" id="tab-btn-dag">
        <span>2. 10-Stratum DAG &amp; Subsystems</span>
        <span class="pill mono">dag / sub</span>
      </button>
      <button class="tab-btn" data-tab="impact" id="tab-btn-impact">
        <span>3. Blast-Radius &amp; Waves</span>
        <span class="pill mono">impact / wave</span>
      </button>
      <button class="tab-btn" data-tab="symbols" id="tab-btn-symbols">
        <span>4. Symbols, Types &amp; Callers</span>
        <span class="pill mono">type / callers</span>
      </button>
      <button class="tab-btn" data-tab="protocol" id="tab-btn-protocol">
        <span>5. Schemas, #bb. Tags &amp; Proofs</span>
        <span class="pill mono">schema / tag</span>
      </button>
      <button class="tab-btn" data-tab="audit" id="tab-btn-audit">
        <span>6. Auditor Gates &amp; Benchmarks</span>
        <span class="pill mono">selftest / eval</span>
      </button>
    </nav>
  </header>

  <!-- Main Workspace + Persistent Deep-Inspector Drawer -->
  <div class="layout-grid">
    <main id="main-workspace" class="min-w-0">
      <!-- TAB 1: GRAPHRAG SEARCH (ask) -->
      <section id="panel-ask" class="tab-panel space-y-4">
        <div class="surface-card p-4">
          <div class="flex flex-wrap items-center justify-between gap-2 mb-2">
            <h2 class="text-base font-bold m-0">Interactive Glass-Box GraphRAG Engine (<code>bbgraph.py ask</code>)</h2>
            <span class="muted text-xs mono" id="ask-cli-cmd">python3 tools/bbgraph.py ask "canonical framebuffer bytes"</span>
          </div>
          <p class="muted text-xs mb-3">
            Type any architectural question, Rust symbol (<code>SparseOctreeV1::from_world</code>), schema string (<code>bitblock/commit-r1/morton-key/v1</code>), or literal content tag (<code>#bb.&lt;kind&gt;.sha256.&lt;digest&gt;</code>). Every result exposes its exact 5-step mathematical ranking breakdown.
          </p>
          <div class="flex gap-2 mb-3">
            <input id="ask-input" type="text" class="input-box mono" value="canonical framebuffer bytes" placeholder="Ask the Bitblock knowledge graph (e.g. canonical framebuffer bytes, SparseOctreeV1::from_world, #bb.receipt.sha256...)" />
            <button id="ask-run-btn" class="btn btn-active">Run Query</button>
          </div>
          <div class="space-y-1.5">
            <div class="text-xs font-semibold muted">Auditor Preset Queries (click to execute):</div>
            <div id="ask-presets" class="flex flex-wrap gap-1.5"></div>
          </div>
        </div>

        <!-- 5-Stage Glass-Box Pipeline Explainer -->
        <div id="ask-pipeline-card" class="surface-card p-4"></div>

        <!-- 5-Group Context Pack Output -->
        <div id="ask-results" class="space-y-3"></div>
      </section>

      <!-- TAB 2: 10-STRATUM DAG & SUBSYSTEMS -->
      <section id="panel-dag" class="tab-panel space-y-4" style="display:none;">
        <div class="surface-card p-4">
          <div class="flex flex-wrap items-center justify-between gap-2 mb-3">
            <div>
              <h2 class="text-base font-bold m-0">10-Stratum Topological Module DAG (37 Modules, 130 Prod Edges, 34 Test Edges)</h2>
              <p class="muted text-xs mt-0.5 mb-0">
                Stratum 0 modules depend on nothing in the workspace; Stratum 9 (<code>main</code>) is the binary entrypoint. Click any module node to highlight its upstream dependencies (cyan), downstream dependents (amber), and symbol-level coupling (<code>symbol_deps</code>).
              </p>
            </div>
            <div class="flex flex-wrap gap-1.5 items-center">
              <select id="dag-crate-filter" class="btn mono">
                <option value="all">Crate: All (37)</option>
                <option value="core">bitblock-core (13)</option>
                <option value="research">bitblock-research (22)</option>
                <option value="binary">bitblock root/bin (2)</option>
              </select>
              <select id="dag-gate-filter" class="btn mono">
                <option value="all">Gate: All cfg</option>
                <option value="default">cfg(default) (15)</option>
                <option value="protocol-research">cfg(protocol-research) (18)</option>
                <option value="all(windows, feature = &quot;protocol-research&quot;)">cfg(windows + research) (3)</option>
                <option value="binary">cfg(binary) (1)</option>
              </select>
              <label class="btn mono"><input type="checkbox" id="dag-show-prod" checked /> Prod Edges (130)</label>
              <label class="btn mono"><input type="checkbox" id="dag-show-test" /> #[cfg(test)] Edges (34)</label>
            </div>
          </div>

          <div class="flex flex-wrap gap-2 mb-3 text-xs">
            <span class="pill pill-core">bitblock-core (default)</span>
            <span class="pill pill-research">bitblock-research (protocol-research)</span>
            <span class="pill pill-win">windows + protocol-research</span>
            <span class="pill pill-binary">bitblock binary / facade</span>
            <span class="pill" style="border-color:#06b6d4;color:#06b6d4;">Cyan = Upstream Dependency (DEPENDS ON)</span>
            <span class="pill" style="border-color:#f59e0b;color:#f59e0b;">Amber = Downstream Dependent (DEPENDED ON BY)</span>
          </div>

          <div id="dag-svg-wrap" class="overflow-x-auto surface-subtle p-2"></div>
          <div id="dag-edge-detail" class="mt-3"></div>
        </div>

        <div class="surface-card p-4">
          <div class="flex flex-wrap items-center justify-between gap-2 mb-2">
            <h3 class="text-sm font-bold m-0">46 Schema-Derived Collaborative Subsystems (<code>bbgraph.py sub</code>)</h3>
            <input id="sub-filter-input" type="text" class="input-box mono" style="max-width:260px;" placeholder="Filter subsystems..." />
          </div>
          <p class="muted text-xs mb-3">
            Subsystem membership is derived automatically from declared <code>bitblock/&lt;namespace&gt;/vN</code> schemas. Click any subsystem to highlight its collaborating modules on the 10-Stratum DAG above.
          </p>
          <div id="sub-grid" class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-2"></div>
        </div>
      </section>

      <!-- TAB 3: 4-TIER BLAST RADIUS (impact) & PARALLEL WAVES (wave) -->
      <section id="panel-impact" class="tab-panel space-y-4" style="display:none;">
        <div class="surface-card p-4">
          <div class="flex flex-wrap items-center justify-between gap-2 mb-2">
            <h2 class="text-base font-bold m-0">4-Tier Blast-Radius (<code>impact</code>) &amp; Parallel Subagent Wave (<code>wave</code>) Simulator</h2>
            <span id="impact-cli-cmd" class="muted text-xs mono">python3 tools/bbgraph.py impact world camera render</span>
          </div>
          <p class="muted text-xs mb-3">
            Select target modules below (or enter a symbol name like <code>World::exposed_faces</code> to auto-resolve to its owning module) to compute the 4-tier blast radius, runnable verification recipe, and conflict-free subagent execution waves over <code>deps &cup; test_deps</code>.
          </p>
          <div class="flex flex-wrap gap-1.5 mb-3" id="impact-presets"></div>
          <div class="flex gap-2 mb-3">
            <input id="impact-input" type="text" class="input-box mono" value="world camera render octree_commit scale_assay" placeholder="Enter module or symbol names (e.g. world, render, World::exposed_faces)..." />
            <button id="impact-run-btn" class="btn btn-active">Simulate Impact &amp; Waves</button>
          </div>
          <div class="text-xs font-semibold muted mb-1.5">Click modules to toggle in target set:</div>
          <div id="impact-mod-pills" class="flex flex-wrap gap-1"></div>
        </div>

        <div id="impact-output" class="space-y-4"></div>
      </section>

      <!-- TAB 4: SYMBOLS, SIGNATURE TYPES & 2-PASS CALLERS -->
      <section id="panel-symbols" class="tab-panel space-y-4" style="display:none;">
        <div class="grid grid-cols-1 xl:grid-cols-2 gap-4">
          <!-- Left: Type Dataflow Explorer -->
          <div class="surface-card p-4">
            <div class="flex items-center justify-between gap-2 mb-2">
              <h2 class="text-base font-bold m-0">Signature Type Dataflow (<code>type / returns / takes</code>)</h2>
              <span class="pill mono">1,723 fn sigs</span>
            </div>
            <p class="muted text-xs mb-3">
              4-step paren-balanced signature parser: inspect where any type is defined, constructed (<code>RETURNED BY</code>), implemented (<code>METHODS</code>), and consumed (<code>TAKEN BY</code>).
            </p>
            <div class="flex gap-2 mb-2">
              <input id="type-input" type="text" class="input-box mono" value="SparseOctreeV1" placeholder="Type name (e.g. SparseOctreeV1, ContentTag, World, JournalIo)..." />
              <button id="type-run-btn" class="btn btn-active">Inspect Type</button>
            </div>
            <div id="type-presets" class="flex flex-wrap gap-1 mb-3"></div>
            <div id="type-output" class="space-y-3"></div>
          </div>

          <!-- Right: 2-Pass Caller & Symbol Explorer -->
          <div class="surface-card p-4">
            <div class="flex items-center justify-between gap-2 mb-2">
              <h2 class="text-base font-bold m-0">2-Pass 3-Rule Caller Graph (<code>callers / sym</code>)</h2>
              <span class="pill mono">2,475 syms + 448 #[test]</span>
            </div>
            <p class="muted text-xs mb-3">
              Resolves qualified calls (<code>Type::method</code>), dot calls (<code>recv.method()</code>), and rare top-level functions across production spans and <code>#[test]</code> spans, emitting crate-aware <code>cargo test</code> commands.
            </p>
            <div class="flex gap-2 mb-2">
              <input id="caller-input" type="text" class="input-box mono" value="CameraProjection::aabb_definitely_outside_frustum" placeholder="Function or Type::method (e.g. World::exposed_faces, JournalIo::metadata_len)..." />
              <button id="caller-run-btn" class="btn btn-active">Trace Callers</button>
            </div>
            <div id="caller-presets" class="flex flex-wrap gap-1 mb-3"></div>
            <div id="caller-output" class="space-y-3"></div>
          </div>
        </div>

        <!-- Bottom: Full Workspace Symbol Search Table -->
        <div class="surface-card p-4">
          <div class="flex flex-wrap items-center justify-between gap-2 mb-3">
            <h3 class="text-sm font-bold m-0">Workspace Symbol Catalog (2,475 Production Symbols across 37 Modules)</h3>
            <div class="flex flex-wrap gap-2">
              <input id="sym-table-search" type="text" class="input-box mono" style="width:240px;" placeholder="Filter by symbol, type, sig, doc..." />
              <select id="sym-table-kind" class="btn mono">
                <option value="all">Kind: All</option>
                <option value="fn">fn</option>
                <option value="struct">struct</option>
                <option value="enum">enum</option>
                <option value="trait">trait</option>
                <option value="const">const</option>
                <option value="type">type</option>
              </select>
              <select id="sym-table-mod" class="btn mono">
                <option value="all">Module: All (37)</option>
              </select>
            </div>
          </div>
          <div id="sym-table-wrap" class="max-h-96 overflow-y-auto surface-subtle"></div>
        </div>
      </section>

      <!-- TAB 5: PROTOCOL SCHEMAS, #bb. TAG TRIANGULATOR, PROOFS & METRICS -->
      <section id="panel-protocol" class="tab-panel space-y-4" style="display:none;">
        <div class="surface-card p-4">
          <div class="flex flex-wrap items-center justify-between gap-2 mb-3">
            <div class="flex flex-wrap gap-1.5">
              <button class="btn btn-active proto-sub-btn" data-sub="tags">5A. #bb. ContentTag Triangulator</button>
              <button class="btn proto-sub-btn" data-sub="schemas">5B. Protocol Surface (74 Schemas + 23 Fixtures)</button>
              <button class="btn proto-sub-btn" data-sub="proofs">5C. Lean 4 Proofs (9) &amp; Metric Registry (1,112)</button>
            </div>
            <input id="proto-search" type="text" class="input-box mono" style="max-width:320px;" placeholder="Filter tags, schemas, theorems, or metrics..." />
          </div>
          <div id="proto-content"></div>
        </div>
      </section>

      <!-- TAB 6: AUDITOR GATES, 90/90 SELFTEST, HOLDOUT EVAL & COVERAGE -->
      <section id="panel-audit" class="tab-panel space-y-4" style="display:none;">
        <!-- Top Row: 3 Accounting Gates + Holdout Eval Benchmark -->
        <div class="grid grid-cols-1 lg:grid-cols-2 gap-4">
          <div class="surface-card p-4" id="audit-gates-card"></div>
          <div class="surface-card p-4" id="audit-eval-card"></div>
        </div>

        <!-- Live In-Browser 90/90 Selftest Suite -->
        <div class="surface-card p-4">
          <div class="flex flex-wrap items-center justify-between gap-2 mb-3">
            <div>
              <h2 class="text-base font-bold m-0">Live In-Browser <code>bbselftest.json</code> Verification Runner (90 Assertions)</h2>
              <p class="muted text-xs mt-0.5 mb-0">
                Every expectation in <code>tools/bbselftest.json</code> is executed directly by this page's JavaScript GraphRAG engine. Click any row to load that exact test case in the Observatory.
              </p>
            </div>
            <div class="flex items-center gap-2">
              <select id="selftest-cat-filter" class="btn mono">
                <option value="all">Category: All (90)</option>
              </select>
              <button id="selftest-rerun-btn" class="btn btn-active">Re-Run All 90 Checks Now</button>
            </div>
          </div>
          <div id="selftest-table-wrap" class="max-h-96 overflow-y-auto surface-subtle"></div>
        </div>

        <!-- Per-Module Documentation & Mutation Coverage Matrix -->
        <div class="surface-card p-4">
          <div class="flex flex-wrap items-center justify-between gap-2 mb-2">
            <h3 class="text-sm font-bold m-0">Per-Module Documentation (<code>///</code>), Test &amp; Canonical Mutation (<code>.to_be_bytes()</code>) Matrix (37 Modules)</h3>
            <span class="pill mono">245 mutation sites across 29 modules</span>
          </div>
          <div id="coverage-table-wrap" class="max-h-96 overflow-y-auto surface-subtle"></div>
        </div>

        <!-- Structural Duplication & Dead Surface -->
        <div class="surface-card p-4" id="audit-reuse-card"></div>
      </section>
    </main>

    <!-- Persistent Right Drawer: Universal Node / Symbol / Tag Deep-Inspector -->
    <aside id="inspector-drawer" class="surface-card p-4 sticky-drawer">
      <div id="inspector-content"></div>
    </aside>
  </div>

  <script id="bb-graph-data" type="application/json">__BB_GRAPH_PAYLOAD__</script>
  <script>
  (() => {
    "use strict";

    // Standalone theme fallback ONLY when opened outside Antigravity host iframe
    const rootStyle = getComputedStyle(document.documentElement);
    let standaloneDark = true;
    function applyStandaloneTheme(dark) {
      const s = document.documentElement.style;
      if (dark) {
        s.setProperty("--background", "#0b0f17");
        s.setProperty("--card", "#111827");
        s.setProperty("--foreground", "#f3f4f6");
        s.setProperty("--muted-foreground", "#9ca3af");
        s.setProperty("--border", "#1f2937");
        s.setProperty("--primary", "#3b82f6");
        s.setProperty("--primary-foreground", "#ffffff");
      } else {
        s.setProperty("--background", "#f8fafc");
        s.setProperty("--card", "#ffffff");
        s.setProperty("--foreground", "#0f172a");
        s.setProperty("--muted-foreground", "#475569");
        s.setProperty("--border", "#e2e8f0");
        s.setProperty("--primary", "#2563eb");
        s.setProperty("--primary-foreground", "#ffffff");
      }
    }
    if (!rootStyle.getPropertyValue("--background").trim()) {
      applyStandaloneTheme(true);
    }
    document.getElementById("theme-toggle-btn").addEventListener("click", () => {
      standaloneDark = !standaloneDark;
      applyStandaloneTheme(standaloneDark);
    });

    // Load compiled payload
    const DATA = JSON.parse(document.getElementById("bb-graph-data").textContent);
    const NODES = DATA.nodes;
    const EDGES = DATA.edges;
    const DAG = DATA.dag;
    const SUBS = DATA.subs;
    const COVERAGE = DATA.coverage;
    const REUSE = DATA.reuse;
    const SELFTEST = DATA.selftest;
    const EVAL_ROWS = DATA.eval_rows;
    const STOP_SET = new Set(DATA.stop);
    const KIND_WORDS = DATA.kind_words;

    const NODE_BY_ID = new Map(NODES.map(n => [n.id, n]));
    const MODULES = NODES.filter(n => n.kind === "module");
    const MOD_BY_NAME = new Map(MODULES.map(m => [m.name, m]));
    const MODULE_NAMES = new Set(MODULES.map(m => m.name));

    // Precompute DF & typical median lengths per kind (exact match with bbgraph.py)
    const PER_KIND_DF = {};
    const KIND_TOTALS = {};
    const KIND_TYPICAL = {};
    {
      const sizes = {};
      for (const node of NODES) {
        const k = node.kind;
        if (!PER_KIND_DF[k]) PER_KIND_DF[k] = {};
        KIND_TOTALS[k] = (KIND_TOTALS[k] || 0) + 1;
        const tset = new Set(node.terms || []);
        for (const t of tset) {
          PER_KIND_DF[k][t] = (PER_KIND_DF[k][t] || 0) + 1;
        }
        if (node.terms && node.terms.length) {
          if (!sizes[k]) sizes[k] = [];
          sizes[k].push(node.terms.length);
        }
      }
      for (const [k, arr] of Object.entries(sizes)) {
        const sorted = arr.slice().sort((a, b) => a - b);
        KIND_TYPICAL[k] = sorted[Math.floor(sorted.length / 2)] || 1;
      }
    }

    function esc(s) {
      return String(s ?? "")
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/"/g, "&quot;");
    }

    function tokens(text) {
      const words = String(text || "").toLowerCase().match(/[a-z0-9]+/g) || [];
      return words.filter(w => w.length > 2 && !STOP_SET.has(w));
    }

    function idf(term, kind) {
      const total = Math.max(KIND_TOTALS[kind] || 1, 1);
      const seen = (PER_KIND_DF[kind] && PER_KIND_DF[kind][term]) || 0;
      return Math.max(Math.log(total / (1 + seen)), 0.0);
    }

    function cite(node) {
      if (!node) return "";
      if (node.kind === "doc") return `${node.path}:${node.line}`;
      if (node.kind === "schema") return (node.declared_at && node.declared_at[0]) || "(undeclared)";
      return node.path || "";
    }

    function describe(node) {
      return String((node && node.summary) || "").trim().replace(/\s+/g, " ").slice(0, 150);
    }

    function loadBearing(node) {
      if (!node || node.kind !== "module") return 0;
      return DAG.transitive_dependents[node.name] || 0;
    }

    function gatePillClass(gate, crateName) {
      if (crateName === "core") return "pill-core";
      if (String(gate).includes("windows")) return "pill-win";
      if (crateName === "research" || String(gate).includes("protocol-research")) return "pill-research";
      return "pill-binary";
    }

    function fmtSyms(syms, limit = 6) {
      if (!syms || !syms.length) return "";
      const head = syms.slice(0, limit).join(", ");
      return syms.length > limit ? `${head}, +${syms.length - limit} more` : head;
    }

    // Tag triangulation resolver (exact match with bbgraph.py resolve_tag)
    const RE_TAG_QUERY = /^(?:#bb\.([a-z0-9-]+)\.sha256\.([a-z0-9]*)|([a-z2-7]{52}))$/;
    const RE_B32_DIGEST = /^[a-z2-7]{52}$/;

    function resolveTag(query) {
      const q = String(query || "").trim().replace(/^["']+|["']+$/g, "").trim().toLowerCase();
      const m = q.match(RE_TAG_QUERY);
      if (!m) return null;
      const kind = m[1] || null;
      const digest = kind !== null ? m[2] : m[3];
      if (!digest || !RE_B32_DIGEST.test(digest)) {
        return { query: query.trim(), kind, digest: digest || "", tags: [], nodes: [], ids: [] };
      }
      const targetFull = kind ? `#bb.${kind}.sha256.${digest}` : null;
      const suffix = "." + digest;
      const matchedTags = new Set();
      const hits = [];
      for (const n of NODES) {
        if (["module", "tool", "doc", "example"].includes(n.kind)) {
          const tags = (n.pinned_tags || []).filter(t => targetFull ? t === targetFull : t.endsWith(suffix));
          if (tags.length) {
            tags.forEach(t => matchedTags.add(t));
            hits.push(n);
          }
        } else if (n.kind === "metric") {
          const t = n.tag || "";
          if (targetFull ? t === targetFull : t.endsWith(suffix)) {
            matchedTags.add(t);
            hits.push(n);
          }
        }
      }
      hits.sort((a, b) => a.id.localeCompare(b.id));
      return {
        query: query.trim(),
        kind,
        digest,
        tags: Array.from(matchedTags).sort(),
        nodes: hits,
        ids: hits.map(n => n.id),
      };
    }

    // 5-Step GraphRAG rank() (bit-exact with bbgraph.py + glass-box breakdown)
    function rank(query, customNodes = null) {
      const nodePool = customNodes || NODES;
      let qt = tokens(query);
      const raw = String(query || "").toLowerCase().match(/[a-z0-9_/.-]+/g) || [];
      const wantKinds = new Set(raw.filter(w => KIND_WORDS[w]).map(w => KIND_WORDS[w]));
      qt = qt.filter(t => !KIND_WORDS[t]);

      // Recompute DF if customNodes passed (for module-doc holdout eval)
      let localIdf = idf;
      let localTypical = KIND_TYPICAL;
      if (customNodes) {
        const pk = {};
        const tot = {};
        const sz = {};
        for (const n of nodePool) {
          const k = n.kind;
          if (!pk[k]) pk[k] = {};
          tot[k] = (tot[k] || 0) + 1;
          for (const t of new Set(n.terms || [])) pk[k][t] = (pk[k][t] || 0) + 1;
          if (n.terms && n.terms.length) {
            if (!sz[k]) sz[k] = [];
            sz[k].push(n.terms.length);
          }
        }
        localTypical = {};
        for (const [k, arr] of Object.entries(sz)) {
          const s = arr.slice().sort((a, b) => a - b);
          localTypical[k] = s[Math.floor(s.length / 2)] || 1;
        }
        localIdf = (term, kind) => {
          const total = Math.max(tot[kind] || 1, 1);
          const seen = (pk[kind] && pk[kind][term]) || 0;
          return Math.max(Math.log(total / (1 + seen)), 0.0);
        };
      }

      const anchored = {};
      const matchedSubs = [];
      for (const sub of SUBS) {
        const subTerms = new Set(sub.terms || []);
        let hit = 0.0;
        for (const t of qt) {
          if (subTerms.has(t)) hit += localIdf(t, "module");
        }
        if (hit > 0) {
          matchedSubs.push({ name: sub.name, score: hit, members: sub.members });
          for (const member of sub.members) {
            anchored[member] = Math.max(anchored[member] || 0.0, hit);
          }
        }
      }
      matchedSubs.sort((a, b) => b.score - a.score);

      const matchedTerms = new Set();
      let named = false;
      const scored = [];
      const rawIdent = new Set(
        (String(query || "").match(/[A-Za-z0-9_]+/g) || [])
          .filter(w => w.includes("_") || /[A-Z]/.test(w.slice(1)))
          .map(w => w.toLowerCase())
      );
      const quals = Array.from(
        String(query || "").toLowerCase().matchAll(/([a-z0-9_]+)::([a-z0-9_]+)/g),
        m => [m[1], m[2]]
      );
      const rawset = new Set(raw.map(t => t.toLowerCase()));

      for (const node of nodePool) {
        const terms = new Set(node.terms || []);
        const name = String(node.name || "").toLowerCase();
        let hit = 0.0;
        const nodeMatchedTerms = [];
        for (const term of qt) {
          if (terms.has(term)) {
            hit += localIdf(term, node.kind);
            matchedTerms.add(term);
            nodeMatchedTerms.push(term);
          }
        }
        if (hit > 0) {
          const baseline = localTypical[node.kind] || 1;
          hit /= Math.sqrt(Math.max(terms.size, 1) / baseline);
        }
        const exact = Boolean(name) && (rawset.has(name) || rawset.has(name.replace(/_/g, "-")));
        const parts = name.split(/[_\-/]/).filter(p => p.length > 2);
        const spelled = parts.length > 1 && parts.every(p => rawset.has(p));
        let symHit = false;
        let matchedSymName = null;
        if (node.kind === "module" && (quals.length || rawIdent.size) && node.symbols) {
          for (const s of node.symbols) {
            const sname = s.name.toLowerCase();
            const simpl = String(s.impl || "").toLowerCase();
            if (quals.length && quals.some(([q, m]) => (q === simpl || q === name) && m === sname)) {
              symHit = true;
              matchedSymName = s.impl ? `${s.impl}::${s.name}` : s.name;
              for (const t of tokens(s.name).concat(tokens(simpl))) matchedTerms.add(t);
              break;
            }
            if (rawIdent.has(sname) && (sname.includes("_") || ["struct", "enum", "trait", "type", "const"].includes(s.kind))) {
              symHit = true;
              matchedSymName = s.impl ? `${s.impl}::${s.name}` : s.name;
              for (const t of tokens(s.name)) matchedTerms.add(t);
            }
          }
        }
        let score = hit;
        let boostLabel = null;
        let boostVal = 0.0;
        if (exact) {
          score += 10.0;
          boostLabel = "exact (+10.0)";
          boostVal = 10.0;
          named = true;
          const nameToks = new Set(tokens(name));
          for (const t of qt) if (nameToks.has(t)) matchedTerms.add(t);
        } else if (symHit) {
          score += 8.0;
          boostLabel = `sym_hit (+8.0: ${matchedSymName})`;
          boostVal = 8.0;
          named = true;
        } else if (spelled) {
          score += 7.0;
          boostLabel = "spelled (+7.0)";
          boostVal = 7.0;
          named = true;
          const spToks = new Set(tokens(name.replace(/_/g, " ")));
          for (const t of qt) if (spToks.has(t)) matchedTerms.add(t);
        } else if (name && raw.some(r => r.length > 3 && name.split("/").includes(r))) {
          score += 3.0;
          boostLabel = "path (+3.0)";
          boostVal = 3.0;
        }
        if (score <= 0) continue;
        const anchor = anchored[node.id] || 0.0;
        const anchorAdd = anchor * 0.5;
        score += anchorAdd;
        let dagMult = 1.0;
        if (anchor > 0) {
          dagMult = 1.0 + 0.12 * Math.log1p(loadBearing(node));
          score *= dagMult;
        }
        scored.push({
          score,
          node,
          breakdown: {
            bm25: hit,
            boostLabel,
            boostVal,
            anchorAdd,
            dagMult,
            matchedTerms: nodeMatchedTerms,
          },
        });
      }

      scored.sort((a, b) => (b.score - a.score) || a.node.id.localeCompare(b.node.id));
      const cov = qt.length ? matchedTerms.size / qt.length : 0.0;
      const missing = Array.from(new Set(qt)).filter(t =>
        Object.keys(PER_KIND_DF).every(k => !(PER_KIND_DF[k] && PER_KIND_DF[k][t]))
      );
      return {
        scored,
        qt,
        coverage: cov,
        anchored: Object.keys(anchored).length > 0,
        named,
        wantKinds,
        missing,
        matchedSubs,
      };
    }

    function confidence(cov, scored, anchored, named) {
      if (!scored || !scored.length) return ["LOW", "nothing matched"];
      if (named) return ["high", "exact name match"];
      if (anchored && cov >= 0.6) {
        return ["high", `${Math.round(cov * 100)}% of the query matched, inside a matched subsystem`];
      }
      if (!anchored) {
        return ["LOW", `no subsystem matched; ${Math.round(cov * 100)}% term overlap only`];
      }
      return ["LOW", `only ${Math.round(cov * 100)}% of the query matched the index`];
    }

    // 4-Tier computeImpact (exact match with bbgraph.py)
    function computeImpact(targets) {
      const tset = new Set(targets);
      const directProd = MODULES
        .filter(m => !tset.has(m.name) && (m.deps || []).some(d => tset.has(d)))
        .map(m => m.name)
        .sort();
      const directProdSet = new Set(directProd);
      const directTestOnly = MODULES
        .filter(m => !tset.has(m.name) && !directProdSet.has(m.name) && (m.test_deps || []).some(d => tset.has(d)))
        .map(m => m.name)
        .sort();

      const revProd = {};
      for (const [u, deps] of Object.entries(DAG.adjacency || {})) {
        for (const dep of deps) {
          if (!revProd[dep]) revProd[dep] = new Set();
          revProd[dep].add(u);
        }
      }
      const allTrans = new Set();
      const stack = [];
      for (const t of targets) {
        for (const u of (revProd[t] || [])) stack.push(u);
      }
      while (stack.length) {
        const cur = stack.pop();
        if (allTrans.has(cur)) continue;
        allTrans.add(cur);
        for (const nxt of (revProd[cur] || [])) stack.push(nxt);
      }
      const trans2Plus = Array.from(allTrans)
        .filter(m => !tset.has(m) && !directProdSet.has(m))
        .sort();

      function modVectors(mname) {
        const mod = MOD_BY_NAME.get(mname);
        if (!mod) return [];
        const schemas = new Set((mod.declares || []).map(s => "schema:" + s));
        const out = new Set();
        for (const e of EDGES) {
          if (e.kind === "vectors" && schemas.has(e.to) && e.from !== "tool:metric_registry") {
            out.add(e.from.split(":")[1]);
          }
        }
        return Array.from(out).sort();
      }

      const directVecs = Array.from(new Set(targets.flatMap(t => modVectors(t)))).sort();
      const directVecSet = new Set(directVecs);
      const hop1Vecs = Array.from(new Set(directProd.flatMap(d => modVectors(d))))
        .filter(v => !directVecSet.has(v))
        .sort();

      const mutTargets = targets.filter(t => (MOD_BY_NAME.get(t)?.mutation_sites || 0) > 0);
      const zeroTargets = new Set(targets.filter(t => (MOD_BY_NAME.get(t)?.mutation_sites || 0) === 0));
      const mut1Hop = directProd.filter(d => {
        const dm = MOD_BY_NAME.get(d);
        return (dm?.mutation_sites || 0) > 0 && (dm?.deps || []).some(dep => zeroTargets.has(dep));
      }).sort();

      const targetIds = new Set(targets.map(t => "module:" + t));
      const proofs = Array.from(new Set(
        EDGES.filter(e => e.kind === "proves" && targetIds.has(e.to)).map(e => e.from.split(":")[1])
      )).sort();

      return {
        targets,
        direct_prod: directProd,
        direct_test_only: directTestOnly,
        trans_2plus: trans2Plus,
        direct_vecs: directVecs,
        hop1_vecs: hop1Vecs,
        mut_targets: mutTargets,
        mut_1hop: mut1Hop,
        proofs,
      };
    }

    // Parallel Subagent Wave Planner (exact match with bbgraph.py compute_waves)
    function computeWaves(names) {
      const target = new Set(names.filter(n => MOD_BY_NAME.has(n)));
      const prereqs = {};
      for (const name of target) {
        const mod = MOD_BY_NAME.get(name);
        const coupled = new Set([...(mod.deps || []), ...(mod.test_deps || [])]);
        prereqs[name] = new Set([...coupled].filter(x => target.has(x) && x !== name));
      }
      const remaining = new Set(target);
      const waves = [];
      while (remaining.size > 0) {
        const ready = [...remaining].filter(m => ![...prereqs[m]].some(p => remaining.has(p))).sort();
        if (!ready.length) {
          waves.push([...remaining].sort());
          break;
        }
        waves.push(ready);
        for (const r of ready) remaining.delete(r);
      }
      return waves;
    }

    // Universal Deep-Inspector Drawer
    function inspectItem(spec) {
      const box = document.getElementById("inspector-content");
      if (!spec) return;

      if (spec.type === "tag") {
        const hit = resolveTag(spec.tag);
        if (!hit || !hit.nodes.length) {
          box.innerHTML = `<div class="muted">No node matched content tag <code>${esc(spec.tag)}</code></div>`;
          return;
        }
        box.innerHTML = `
          <div class="flex items-center justify-between gap-2 mb-2">
            <span class="pill pill-high mono">CONTENT TAG TRIANGULATION</span>
            <span class="muted text-xs mono">${hit.nodes.length} artifacts</span>
          </div>
          <div class="mono text-xs font-bold break-all mb-3 p-2 surface-subtle">${esc(hit.tags.join(", "))}</div>
          <div class="text-xs muted mb-2">CLI: <code>python3 tools/bbgraph.py node "${esc(hit.tags[0])}"</code></div>
          <div class="space-y-2">
            ${hit.nodes.map(n => `
              <div class="surface-subtle p-2.5 clickable-row" data-inspect-id="${esc(n.id)}">
                <div class="flex items-center justify-between gap-2">
                  <span class="font-semibold mono text-xs">${esc(n.id)}</span>
                  <span class="pill mono">${esc(n.kind)}</span>
                </div>
                <div class="muted text-xs mono mt-0.5">${esc(cite(n))}</div>
                ${describe(n) ? `<div class="text-xs mt-1">${esc(describe(n))}</div>` : ""}
              </div>
            `).join("")}
          </div>
        `;
        bindInspectClicks(box);
        return;
      }

      if (spec.type === "symbol") {
        const m = MOD_BY_NAME.get(spec.mod);
        const s = (m?.symbols || []).find(x => x.name === spec.name && x.line === spec.line) ||
                  (m?.symbols || []).find(x => x.name === spec.name);
        if (!m || !s) return;
        const qual = s.impl ? `${s.impl}::${s.name}` : s.name;
        box.innerHTML = `
          <div class="flex items-center justify-between gap-2 mb-2">
            <span class="pill ${gatePillClass(m.gate, m.crate)} mono">${esc(s.pub ? "pub " : "private ")}${esc(s.kind)}</span>
            <button class="btn text-xs" data-inspect-id="module:${esc(m.name)}">Module: ${esc(m.name)}</button>
          </div>
          <h3 class="mono text-sm font-bold m-0 break-all">${esc(qual)}</h3>
          <div class="muted text-xs mono mt-1 mb-2">${esc(m.path)}:${s.line}-${s.end} &bull; cfg(${esc(m.gate)})</div>
          ${s.doc ? `<div class="p-2.5 surface-subtle text-xs mb-3">${esc(s.doc)}</div>` : ""}
          ${s.sig ? `
            <div class="text-xs font-semibold muted mb-1">SIGNATURE</div>
            <pre class="p-2.5 surface-subtle mono text-xs overflow-x-auto whitespace-pre-wrap mb-3">${esc(s.sig)}</pre>
          ` : ""}
          ${(s.takes && s.takes.length) ? `
            <div class="text-xs font-semibold muted mb-1">TAKES TYPES (${s.takes.length})</div>
            <div class="flex flex-wrap gap-1 mb-3">
              ${s.takes.map(t => `<button class="btn text-xs mono" data-jump-type="${esc(t)}">${esc(t)}</button>`).join("")}
            </div>
          ` : ""}
          ${(s.returns && s.returns.length) ? `
            <div class="text-xs font-semibold muted mb-1">RETURNS TYPES (${s.returns.length})</div>
            <div class="flex flex-wrap gap-1 mb-3">
              ${s.returns.map(t => `<button class="btn text-xs mono" data-jump-type="${esc(t)}">${esc(t)}</button>`).join("")}
            </div>
          ` : ""}
          ${(s.calls && s.calls.length) ? `
            <div class="text-xs font-semibold muted mb-1">OUTGOING CALLS IN SPAN (${s.calls.length})</div>
            <div class="flex flex-wrap gap-1 mb-3">
              ${s.calls.map(c => `<button class="btn text-xs mono" data-jump-caller="${esc(c)}">${esc(c)}</button>`).join("")}
            </div>
          ` : ""}
          <div class="flex flex-wrap gap-2 mt-3 pt-3 border-t border-[var(--border)]">
            <button class="btn btn-active text-xs" data-jump-caller="${esc(qual)}">Trace Callers (${esc(qual)})</button>
            <button class="btn text-xs" data-jump-impact="${esc(m.name)}">Simulate Impact (${esc(m.name)})</button>
          </div>
        `;
        bindInspectClicks(box);
        return;
      }

      const node = NODE_BY_ID.get(spec.id) || MOD_BY_NAME.get(spec.id);
      if (!node) return;

      if (node.kind === "module") {
        const st = DAG.stratum[node.name] ?? 0;
        const reach = loadBearing(node);
        const pubCnt = node.pub_symbol_count || 0;
        const pubDoc = node.pub_documented || 0;
        const docPct = pubCnt > 0 ? Math.round((100 * pubDoc) / pubCnt) : null;
        const parents = EDGES
          .filter(e => e.kind === "depends_on" && e.to === node.id)
          .map(e => e.from.split(":")[1])
          .sort();
        const proofs = EDGES
          .filter(e => e.kind === "proves" && e.to === node.id)
          .map(e => e.from);

        box.innerHTML = `
          <div class="flex items-center justify-between gap-2 mb-2">
            <span class="pill ${gatePillClass(node.gate, node.crate)} mono">MODULE &bull; ${esc(node.crate)}</span>
            <span class="pill mono">Stratum ${st} &bull; ${reach} dependents</span>
          </div>
          <h3 class="mono text-base font-bold m-0">${esc(node.name)}</h3>
          <div class="muted text-xs mono mt-0.5 mb-2">${esc(node.path)} &bull; cfg(${esc(node.gate)})</div>
          ${node.summary ? `<div class="p-2.5 surface-subtle text-xs mb-3">${esc(node.summary)}</div>` : ""}

          <div class="grid grid-cols-2 gap-2 mb-3 text-xs">
            <div class="surface-subtle p-2">
              <div class="muted">Size &amp; Tests</div>
              <div class="font-bold mono mt-0.5">${node.lines.toLocaleString()} LOC &bull; ${node.tests} #[test]</div>
            </div>
            <div class="surface-subtle p-2">
              <div class="muted">Mutation &amp; Docs</div>
              <div class="font-bold mono mt-0.5">${node.mutation_sites || 0} .to_be_bytes &bull; ${docPct !== null ? `${pubDoc}/${pubCnt} (${docPct}%)` : "n/a"}</div>
            </div>
          </div>

          <div class="flex flex-wrap gap-1.5 mb-3">
            <button class="btn btn-active text-xs" data-jump-impact="${esc(node.name)}">Simulate Blast Radius (impact)</button>
            <button class="btn text-xs" data-jump-dag="${esc(node.name)}">Highlight in 10-Stratum DAG</button>
          </div>

          ${(node.deps && node.deps.length) ? `
            <div class="text-xs font-semibold muted mb-1">DEPENDS ON (${node.deps.length})</div>
            <div class="space-y-1 mb-3">
              ${node.deps.map(dep => {
                const syms = (node.symbol_deps && node.symbol_deps[dep]) || [];
                return `
                  <div class="surface-subtle p-2 clickable-row text-xs" data-inspect-id="module:${esc(dep)}">
                    <div class="flex items-center justify-between">
                      <span class="font-semibold mono">${esc(dep)}</span>
                      <span class="muted mono">${syms.length} syms</span>
                    </div>
                    ${syms.length ? `<div class="muted mono mt-0.5">[${esc(fmtSyms(syms, 8))}]</div>` : ""}
                  </div>
                `;
              }).join("")}
            </div>
          ` : ""}

          ${((node.test_deps && node.test_deps.length) || (node.test_symbol_deps && Object.keys(node.test_symbol_deps).length)) ? `
            <div class="text-xs font-semibold muted mb-1">#[cfg(test)] TEST DEPS</div>
            <div class="space-y-1 mb-3">
              ${Array.from(new Set([...(node.test_deps || []), ...Object.keys(node.test_symbol_deps || {})])).sort().map(dep => {
                const syms = (node.test_symbol_deps && node.test_symbol_deps[dep]) || [];
                const extra = (node.deps || []).includes(dep) ? " (#[cfg(test)] extra)" : "";
                return `
                  <div class="surface-subtle p-2 clickable-row text-xs" data-inspect-id="module:${esc(dep)}">
                    <span class="font-semibold mono">${esc(dep)}</span><span class="muted">${esc(extra)}</span>
                    ${syms.length ? `<div class="muted mono mt-0.5">[${esc(fmtSyms(syms, 8))}]</div>` : ""}
                  </div>
                `;
              }).join("")}
            </div>
          ` : ""}

          ${parents.length ? `
            <div class="text-xs font-semibold muted mb-1">DEPENDED ON BY (${parents.length})</div>
            <div class="flex flex-wrap gap-1 mb-3">
              ${parents.map(p => `<button class="btn text-xs mono" data-inspect-id="module:${esc(p)}">${esc(p)}</button>`).join("")}
            </div>
          ` : ""}

          ${(node.declares && node.declares.length) ? `
            <div class="text-xs font-semibold muted mb-1">DECLARES SCHEMAS (${node.declares.length})</div>
            <div class="space-y-1 mb-3">
              ${node.declares.map(sc => `
                <div class="surface-subtle p-1.5 text-xs mono clickable-row" data-inspect-id="schema:${esc(sc)}">${esc(sc)}</div>
              `).join("")}
            </div>
          ` : ""}

          ${proofs.length ? `
            <div class="text-xs font-semibold muted mb-1">FORMALLY PROVED BY (${proofs.length})</div>
            <div class="flex flex-wrap gap-1 mb-3">
              ${proofs.map(pid => `<button class="btn text-xs mono" data-inspect-id="${esc(pid)}">${esc(pid.split(":")[1])}</button>`).join("")}
            </div>
          ` : ""}

          ${(node.pinned_tags && node.pinned_tags.length) ? `
            <div class="text-xs font-semibold muted mb-1">PINNED CONTENT TAGS (${node.pinned_tags.length})</div>
            <div class="space-y-1 mb-3 max-h-40 overflow-y-auto">
              ${node.pinned_tags.slice(0, 15).map(t => `
                <div class="surface-subtle p-1.5 text-xs mono break-all clickable-row" data-inspect-tag="${esc(t)}">${esc(t)}</div>
              `).join("")}
            </div>
          ` : ""}

          ${(node.symbols && node.symbols.length) ? `
            <div class="text-xs font-semibold muted mb-1">SYMBOLS (${node.symbols.length})</div>
            <div class="space-y-1 max-h-64 overflow-y-auto">
              ${node.symbols.slice(0, 80).map(s => {
                const qual = s.impl ? `${s.impl}::${s.name}` : s.name;
                return `
                  <div class="surface-subtle p-1.5 text-xs clickable-row" data-inspect-sym="${esc(node.name)}|${esc(s.name)}|${s.line}">
                    <div class="flex items-center justify-between gap-1">
                      <span class="mono font-semibold">${esc(qual)}</span>
                      <span class="muted mono">${esc(s.kind)} :${s.line}-${s.end}</span>
                    </div>
                    ${s.doc ? `<div class="muted truncate mt-0.5">${esc(s.doc)}</div>` : ""}
                  </div>
                `;
              }).join("")}
            </div>
          ` : ""}
        `;
        bindInspectClicks(box);
        return;
      }

      // Generic node inspector (schema, tool, proof, doc, metric, example)
      box.innerHTML = `
        <div class="flex items-center justify-between gap-2 mb-2">
          <span class="pill mono">${esc(node.kind.toUpperCase())}</span>
          <span class="muted text-xs mono">${esc(cite(node))}</span>
        </div>
        <h3 class="mono text-sm font-bold m-0 break-all">${esc(node.name)}</h3>
        <div class="muted text-xs mono mt-1 mb-3">id: ${esc(node.id)}</div>
        ${describe(node) ? `<div class="p-2.5 surface-subtle text-xs mb-3">${esc(describe(node))}</div>` : ""}
        ${node.kind === "schema" ? `
          <div class="space-y-2 text-xs">
            <div><span class="muted">Namespace:</span> <code class="mono">${esc(node.namespace)}</code></div>
            <div><span class="muted">Scope:</span> ${node.fixture ? `<span class="pill pill-low">#[cfg(test)] fixture</span>` : `<span class="pill pill-high">Production Protocol Surface</span>`}</div>
            <div><span class="muted">Declared by:</span> ${(node.declared_by || []).map(m => `<button class="btn text-xs mono" data-inspect-id="${esc(m)}">${esc(m.split(":")[1])}</button>`).join(" ")}</div>
            <div><span class="muted">Vectored by:</span> ${(node.vectored_by || []).length ? node.vectored_by.map(v => `<button class="btn text-xs mono" data-inspect-id="${esc(v)}">${esc(v.split(":")[1])}</button>`).join(" ") : `<span class="pill pill-danger">NONE</span>`}</div>
          </div>
        ` : ""}
        ${node.kind === "proof" ? `
          <div class="space-y-2 text-xs">
            <div class="font-semibold muted">THEOREMS (${(node.theorems || []).length})</div>
            <div class="space-y-1">${(node.theorems || []).map(t => `<div class="surface-subtle p-1.5 mono">${esc(t)}</div>`).join("")}</div>
            <div class="font-semibold muted mt-2">PROVES MODULES</div>
            <div class="flex flex-wrap gap-1">${(node.cites_src || []).map(m => `<button class="btn text-xs mono" data-inspect-id="module:${esc(m)}">${esc(m)}</button>`).join("")}</div>
          </div>
        ` : ""}
        ${node.kind === "metric" ? `
          <div class="space-y-2 text-xs">
            <div><span class="muted">Owner:</span> <span class="pill mono">${esc(node.owner)}</span> (<code>docs/metrics/${esc(node.owner)}.md</code>)</div>
            <div><span class="muted">Alias:</span> <code class="mono break-all">${esc(node.alias)}</code></div>
            <div class="surface-subtle p-2 mono break-all clickable-row" data-inspect-tag="${esc(node.tag)}">${esc(node.tag)}</div>
          </div>
        ` : ""}
        ${(node.schemas && node.schemas.length) ? `
          <div class="text-xs font-semibold muted mt-3 mb-1">RECONSTRUCTS SCHEMAS (${node.schemas.length})</div>
          <div class="space-y-1 max-h-48 overflow-y-auto">
            ${node.schemas.map(s => `<div class="surface-subtle p-1.5 text-xs mono clickable-row" data-inspect-id="schema:${esc(s)}">${esc(s)}</div>`).join("")}
          </div>
        ` : ""}
        ${(node.pinned_tags && node.pinned_tags.length) ? `
          <div class="text-xs font-semibold muted mt-3 mb-1">PINNED CONTENT TAGS (${node.pinned_tags.length})</div>
          <div class="space-y-1 max-h-48 overflow-y-auto">
            ${node.pinned_tags.slice(0, 20).map(t => `<div class="surface-subtle p-1.5 text-xs mono break-all clickable-row" data-inspect-tag="${esc(t)}">${esc(t)}</div>`).join("")}
          </div>
        ` : ""}
      `;
      bindInspectClicks(box);
    }

    function bindInspectClicks(container) {
      container.querySelectorAll("[data-inspect-id]").forEach(el => {
        el.addEventListener("click", e => {
          e.stopPropagation();
          inspectItem({ id: el.getAttribute("data-inspect-id") });
        });
      });
      container.querySelectorAll("[data-inspect-tag]").forEach(el => {
        el.addEventListener("click", e => {
          e.stopPropagation();
          inspectItem({ type: "tag", tag: el.getAttribute("data-inspect-tag") });
        });
      });
      container.querySelectorAll("[data-inspect-sym]").forEach(el => {
        el.addEventListener("click", e => {
          e.stopPropagation();
          const [mod, name, line] = el.getAttribute("data-inspect-sym").split("|");
          inspectItem({ type: "symbol", mod, name, line: Number(line) });
        });
      });
      container.querySelectorAll("[data-jump-impact]").forEach(el => {
        el.addEventListener("click", e => {
          e.stopPropagation();
          switchTab("impact");
          document.getElementById("impact-input").value = el.getAttribute("data-jump-impact");
          renderImpact();
        });
      });
      container.querySelectorAll("[data-jump-dag]").forEach(el => {
        el.addEventListener("click", e => {
          e.stopPropagation();
          switchTab("dag");
          selectedDagMod = el.getAttribute("data-jump-dag");
          renderDag();
        });
      });
      container.querySelectorAll("[data-jump-type]").forEach(el => {
        el.addEventListener("click", e => {
          e.stopPropagation();
          switchTab("symbols");
          document.getElementById("type-input").value = el.getAttribute("data-jump-type");
          renderTypeExplorer();
        });
      });
      container.querySelectorAll("[data-jump-caller]").forEach(el => {
        el.addEventListener("click", e => {
          e.stopPropagation();
          switchTab("symbols");
          document.getElementById("caller-input").value = el.getAttribute("data-jump-caller");
          renderCallerExplorer();
        });
      });
    }

    // KPI Ribbon
    function renderKpiRibbon() {
      const totSyms = MODULES.reduce((a, m) => a + (m.symbols ? m.symbols.length : 0), 0);
      const totMut = MODULES.reduce((a, m) => a + (m.mutation_sites || 0), 0);
      const kpis = [
        { label: "Indexed Nodes", val: NODES.length.toLocaleString(), sub: `${EDGES.length.toLocaleString()} typed edges` },
        { label: "Rust Modules", val: COVERAGE.modules, sub: `${COVERAGE.source_lines.toLocaleString()} LOC across 3 crates` },
        { label: "DAG Topology", val: "10 Strata", sub: `${DAG.edges} intra-crate edges` },
        { label: "Symbols & Tests", val: totSyms.toLocaleString(), sub: `${COVERAGE.tests} #[test] functions` },
        { label: "Protocol Surface", val: `${COVERAGE.schemas_vectored}/${COVERAGE.schemas}`, sub: "100% Python vectored" },
        { label: "Mutation Sites", val: totMut, sub: "29 modules (.to_be_bytes)" },
        { label: "Metric Families", val: COVERAGE.metrics.toLocaleString(), sub: "world / view / system" },
        { label: "Lean 4 Proofs", val: COVERAGE.proofs, sub: "formal proof modules" },
      ];
      document.getElementById("kpi-ribbon").innerHTML = kpis.map(k => `
        <div class="surface-subtle p-2.5">
          <div class="muted text-xs">${esc(k.label)}</div>
          <div class="text-base font-bold mono mt-0.5">${esc(k.val)}</div>
          <div class="muted text-xs mt-0.5 truncate">${esc(k.sub)}</div>
        </div>
      `).join("");
    }

    // Tab switching
    function switchTab(tabId) {
      document.querySelectorAll(".tab-btn").forEach(b => {
        b.classList.toggle("active", b.getAttribute("data-tab") === tabId);
      });
      document.querySelectorAll(".tab-panel").forEach(p => {
        p.style.display = p.id === `panel-${tabId}` ? "" : "none";
      });
      if (tabId === "dag") renderDag();
    }
    document.querySelectorAll(".tab-btn").forEach(btn => {
      btn.addEventListener("click", () => switchTab(btn.getAttribute("data-tab")));
    });

    // =========================================================================
    // TAB 1: GRAPHRAG SEARCH (ask)
    // =========================================================================
    const ASK_PRESETS = [
      { label: "Framebuffer bytes", q: "canonical framebuffer bytes" },
      { label: "SparseOctreeV1::from_world", q: "SparseOctreeV1::from_world" },
      { label: "Frustum AABB culling", q: "CameraProjection::aabb_definitely_outside_frustum" },
      { label: "World::exposed_faces", q: "World::exposed_faces" },
      { label: "Journal recovery", q: "append-only journal recovery" },
      { label: "MortonKeyV1", q: "bitblock/commit-r1/morton-key/v1" },
      { label: "Windows present", q: "windows present" },
      { label: "Crosshair overlay", q: "crosshair overlay" },
      { label: "Tag: receipt", q: "#bb.receipt.sha256.j3xacn27gfhq6f4wrxvn5upcvvufq5soltxncytvglb2recohp7a" },
      { label: "Tag: octree-node", q: "#bb.octree-node.sha256.mpxk6r6w7mgephgbv3bhmhbwj3rkc3t7l3z3a7p5n7w4v4z4q2aa" },
      { label: "Tag: multiline output-topology", q: "#bb.output-topology.sha256.fw2l66z7jd2eu6suedannr77x5yrugb6sot7y3ojduamdiuemtpq" },
      { label: "Control: no_match", q: "kubernetes postgres oauth" },
    ];

    document.getElementById("ask-presets").innerHTML = ASK_PRESETS.map(p => `
      <button class="btn text-xs mono" data-ask-preset="${esc(p.q)}">${esc(p.label)}</button>
    `).join("");
    document.querySelectorAll("[data-ask-preset]").forEach(b => {
      b.addEventListener("click", () => {
        document.getElementById("ask-input").value = b.getAttribute("data-ask-preset");
        renderAsk();
      });
    });
    document.getElementById("ask-run-btn").addEventListener("click", renderAsk);
    document.getElementById("ask-input").addEventListener("keydown", e => {
      if (e.key === "Enter") renderAsk();
    });

    const GROUPS = [
      ["CODE", ["module"], "read these"],
      ["PROTOCOL", ["schema"], "the byte-level objects involved"],
      ["DOCS", ["doc"], "what the tree says about it"],
      ["EVIDENCE", ["proof", "tool", "example"], "what independently checks it"],
      ["METRICS", ["metric"], "registry families that measure it"],
    ];

    function renderAsk() {
      const query = document.getElementById("ask-input").value.trim();
      document.getElementById("ask-cli-cmd").textContent = `python3 tools/bbgraph.py ask "${query}"`;
      const pipeBox = document.getElementById("ask-pipeline-card");
      const resBox = document.getElementById("ask-results");

      // Step 0: Check ContentTag direct interception
      const tagHit = resolveTag(query);
      if (tagHit !== null) {
        const found = tagHit.nodes.length > 0;
        pipeBox.innerHTML = `
          <div class="flex flex-wrap items-center justify-between gap-2 mb-2">
            <span class="font-bold text-sm">Glass-Box Pipeline: Direct ContentTag Interception (<code>resolve_tag</code>)</span>
            <span class="pill ${found ? "pill-high" : "pill-low"} mono">CONFIDENCE ${found ? "high (exact content tag match)" : "LOW (nothing matched)"}</span>
          </div>
          <div class="text-xs muted">
            Detected canonical <code>#bb.&lt;kind&gt;.sha256.&lt;digest&gt;</code> query. Bypassed BM25 and triangulated digest <code>${esc(tagHit.digest)}</code> directly across Rust modules, Python vector suites, Markdown docs, and Metric definitions (${tagHit.nodes.length} matches).
          </div>
        `;
        if (!found) {
          resBox.innerHTML = `<div class="surface-card p-4 text-xs mono muted">NOTHING IN THE INDEX MATCHED THIS CONTENT TAG.</div>`;
          return;
        }
        resBox.innerHTML = `
          <div class="surface-card p-4">
            <div class="text-xs font-bold mono mb-2">MATCHED CONTENT TAG: ${esc(tagHit.tags.join(", "))}</div>
            <div class="grid grid-cols-1 md:grid-cols-2 gap-2">
              ${tagHit.nodes.map(n => `
                <div class="surface-subtle p-3 clickable-row" data-inspect-id="${esc(n.id)}">
                  <div class="flex items-center justify-between">
                    <span class="font-bold mono text-xs">${esc(n.name)}</span>
                    <span class="pill mono">${esc(n.kind)}</span>
                  </div>
                  <div class="muted text-xs mono mt-0.5">${esc(cite(n))}</div>
                  ${describe(n) ? `<div class="text-xs mt-1">${esc(describe(n))}</div>` : ""}
                </div>
              `).join("")}
            </div>
          </div>
        `;
        bindInspectClicks(resBox);
        if (tagHit.nodes[0]) inspectItem({ id: tagHit.nodes[0].id });
        return;
      }

      const r = rank(query);
      const [confLevel, confWhy] = confidence(r.coverage, r.scored, r.anchored, r.named);
      const leadMod = r.scored.find(x => x.node.kind === "module")?.node || null;

      pipeBox.innerHTML = `
        <div class="flex flex-wrap items-center justify-between gap-2 mb-3">
          <div class="flex items-center gap-2">
            <span class="font-bold text-sm">5-Step Glass-Box Ranking Pipeline</span>
            <span class="muted text-xs mono">${r.scored.length} nodes matched</span>
          </div>
          <span class="pill ${confLevel === "high" ? "pill-high" : "pill-low"} mono">CONFIDENCE ${esc(confLevel)} &mdash; ${esc(confWhy)}</span>
        </div>
        ${(r.missing.length && !r.named) ? `
          <div class="p-2.5 mb-3 rounded-lg border border-amber-500/50 bg-amber-500/10 text-xs mono">
            <strong>NOT IN THIS INDEX:</strong> ${esc(r.missing.join(", "))}
          </div>
        ` : ""}
        ${(!r.anchored && !r.named && r.scored.length) ? `
          <div class="p-2.5 mb-3 rounded-lg border border-amber-500/40 bg-amber-500/10 text-xs">
            <strong>NO SUBSYSTEM MATCHED THIS WORDING</strong> &mdash; ranked on per-kind IDF term overlap alone. Treat as a hypothesis and verify.
          </div>
        ` : ""}
        <div class="grid grid-cols-1 md:grid-cols-5 gap-2 text-xs">
          <div class="surface-subtle p-2.5">
            <div class="font-semibold muted">1. Query Tokens</div>
            <div class="mono mt-1">${r.qt.length ? r.qt.map(t => `<span class="pill mono mr-1 mb-1">${esc(t)}</span>`).join("") : "<span class='muted'>(none)</span>"}</div>
            ${r.wantKinds.size ? `<div class="muted mt-1">Kind filter: <code>${esc([...r.wantKinds].join(", "))}</code></div>` : ""}
          </div>
          <div class="surface-subtle p-2.5">
            <div class="font-semibold muted">2. Subsystem Anchor</div>
            <div class="mt-1">
              ${r.matchedSubs.length
                ? r.matchedSubs.slice(0, 3).map(s => `<div class="mono text-xs">${esc(s.name)} <span class="muted">(+${(s.score * 0.5).toFixed(2)})</span></div>`).join("")
                : `<span class="muted">0 of 46 subsystems matched</span>`}
            </div>
          </div>
          <div class="surface-subtle p-2.5">
            <div class="font-semibold muted">3. Per-Kind BM25 Norm</div>
            <div class="muted mt-1">
              Medians: <code>module=${KIND_TYPICAL.module}</code>, <code>doc=${KIND_TYPICAL.doc}</code>, <code>metric=${KIND_TYPICAL.metric}</code> terms
            </div>
          </div>
          <div class="surface-subtle p-2.5">
            <div class="font-semibold muted">4. Name / Symbol Lookup</div>
            <div class="mt-1 mono">
              ${r.named ? `<span class="pill pill-high">Named Match Active</span>` : `<span class="muted">No exact/sym/spelled hit</span>`}
            </div>
          </div>
          <div class="surface-subtle p-2.5">
            <div class="font-semibold muted">5. Load-Bearing Gate</div>
            <div class="muted mt-1">
              ${r.anchored ? `Active: <code>&times;(1 + 0.12 ln(1+reach))</code> inside anchored subsystems` : `Suppressed (unanchored query)`}
            </div>
          </div>
        </div>
      `;

      if (!r.scored.length) {
        resBox.innerHTML = `<div class="surface-card p-4 text-xs mono muted">NOTHING IN THE INDEX MATCHED THIS WORDING.</div>`;
        return;
      }

      const orderedGroups = GROUPS.slice().sort((a, b) => {
        const aHit = a[1].some(k => r.wantKinds.has(k)) ? 0 : 1;
        const bHit = b[1].some(k => r.wantKinds.has(k)) ? 0 : 1;
        return aHit - bHit;
      });

      let html = "";
      for (const [title, kinds, note] of orderedGroups) {
        const picks = r.scored.filter(item => kinds.includes(item.node.kind));
        if (!picks.length) continue;
        const limit = title !== "METRICS" ? 4 : 3;
        html += `
          <div class="surface-card p-4">
            <div class="flex items-center justify-between mb-2">
              <div>
                <span class="font-bold text-sm mono">${esc(title)}</span>
                <span class="muted text-xs ml-2">&mdash; ${esc(note)}</span>
              </div>
              <span class="muted text-xs mono">showing ${Math.min(picks.length, limit)} of ${picks.length}</span>
            </div>
            <div class="grid grid-cols-1 md:grid-cols-2 gap-2.5">
              ${picks.slice(0, limit).map(({ score, node, breakdown }) => `
                <div class="surface-subtle p-3 clickable-row" data-inspect-id="${esc(node.id)}">
                  <div class="flex items-center justify-between gap-2">
                    <span class="font-bold mono text-xs truncate">${esc(node.name)}</span>
                    <span class="pill mono">score ${score.toFixed(2)}</span>
                  </div>
                  <div class="muted text-xs mono mt-0.5">${esc(cite(node))}</div>
                  ${describe(node) ? `<div class="text-xs mt-1.5">${esc(describe(node))}</div>` : ""}
                  ${node.kind === "module" ? `
                    <div class="flex flex-wrap gap-1 mt-2">
                      <span class="pill ${gatePillClass(node.gate, node.crate)} mono">cfg(${esc(node.gate)})</span>
                      <span class="pill mono">${node.tests} tests</span>
                      <span class="pill mono">${loadBearing(node)} dependents</span>
                    </div>
                  ` : ""}
                  <div class="flex flex-wrap gap-1 mt-2 pt-2 border-t border-[var(--border)] text-xs mono muted">
                    <span>BM25: ${breakdown.bm25.toFixed(2)}</span>
                    ${breakdown.boostLabel ? `<span>&bull; ${esc(breakdown.boostLabel)}</span>` : ""}
                    ${breakdown.anchorAdd > 0 ? `<span>&bull; SubAnchor: +${breakdown.anchorAdd.toFixed(2)}</span>` : ""}
                    ${breakdown.dagMult > 1 ? `<span>&bull; DAG: &times;${breakdown.dagMult.toFixed(2)}</span>` : ""}
                  </div>
                </div>
              `).join("")}
            </div>
          </div>
        `;
      }

      if (leadMod && leadMod.deps && leadMod.deps.length) {
        html += `
          <div class="surface-card p-4">
            <div class="font-bold text-xs mono muted mb-2">WHAT LEAD MODULE (${esc(leadMod.name.toUpperCase())}) PULLS IN (${leadMod.deps.length} DIRECT DEPENDENCIES)</div>
            <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-2">
              ${leadMod.deps.map(dep => {
                const child = MOD_BY_NAME.get(dep);
                const syms = (leadMod.symbol_deps && leadMod.symbol_deps[dep]) || [];
                return `
                  <div class="surface-subtle p-2.5 clickable-row text-xs" data-inspect-id="module:${esc(dep)}">
                    <div class="font-bold mono">${esc(dep)}</div>
                    <div class="muted truncate mt-0.5">${esc(describe(child))}</div>
                    ${syms.length ? `<div class="mono text-xs muted mt-1">[${esc(fmtSyms(syms, 5))}]</div>` : ""}
                  </div>
                `;
              }).join("")}
            </div>
          </div>
        `;
      }

      resBox.innerHTML = html;
      bindInspectClicks(resBox);
      if (leadMod) inspectItem({ id: leadMod.id });
      else if (r.scored[0]) inspectItem({ id: r.scored[0].node.id });
    }

    // =========================================================================
    // TAB 2: 10-STRATUM ARCHITECTURE DAG & SUBSYSTEMS
    // =========================================================================
    let selectedDagMod = "octree_commit";
    let highlightedSubMembers = new Set();

    ["dag-crate-filter", "dag-gate-filter", "dag-show-prod", "dag-show-test"].forEach(id => {
      document.getElementById(id).addEventListener("change", renderDag);
    });
    document.getElementById("sub-filter-input").addEventListener("input", renderSubsystems);

    function renderDag() {
      const crateFilter = document.getElementById("dag-crate-filter").value;
      const gateFilter = document.getElementById("dag-gate-filter").value;
      const showProd = document.getElementById("dag-show-prod").checked;
      const showTest = document.getElementById("dag-show-test").checked;

      const strata = {};
      for (const m of MODULES) {
        const st = DAG.stratum[m.name] ?? 0;
        if (!strata[st]) strata[st] = [];
        strata[st].push(m);
      }
      for (const st of Object.keys(strata)) {
        strata[st].sort((a, b) => a.name.localeCompare(b.name));
      }

      const maxCols = Math.max(...Object.values(strata).map(arr => arr.length), 1);
      const colW = 184;
      const rowH = 64;
      const padX = 95;
      const padY = 28;
      const width = padX + maxCols * colW + 40;
      const height = padY * 2 + 10 * rowH;

      // Position nodes: Stratum 9 at top (row 0), Stratum 0 at bottom (row 9)
      const pos = {};
      for (let st = 0; st <= 9; st++) {
        const row = 9 - st;
        const arr = strata[st] || [];
        const y = padY + row * rowH;
        arr.forEach((m, idx) => {
          const x = padX + idx * colW;
          pos[m.name] = { x, y, w: 168, h: 42, mod: m, st };
        });
      }

      const selMod = MOD_BY_NAME.get(selectedDagMod);
      const upstreamSet = new Set(selMod ? (selMod.deps || []) : []);
      const testUpstreamSet = new Set(selMod ? (selMod.test_deps || []) : []);
      const downstreamSet = new Set(
        EDGES.filter(e => e.kind === "depends_on" && e.to === `module:${selectedDagMod}`)
             .map(e => e.from.split(":")[1])
      );

      let edgesSvg = "";
      if (showProd) {
        for (const e of EDGES) {
          if (e.kind !== "depends_on") continue;
          const u = e.from.split(":")[1];
          const v = e.to.split(":")[1];
          const p1 = pos[u];
          const p2 = pos[v];
          if (!p1 || !p2) continue;
          const isUp = u === selectedDagMod;
          const isDown = v === selectedDagMod;
          const active = isUp || isDown;
          const stroke = isUp ? "#06b6d4" : isDown ? "#f59e0b" : "rgba(148,163,184,0.22)";
          const sw = active ? 2.2 : 1.0;
          const x1 = p1.x + p1.w / 2;
          const y1 = p1.y + p1.h;
          const x2 = p2.x + p2.w / 2;
          const y2 = p2.y;
          const cy = (y1 + y2) / 2;
          edgesSvg += `<path d="M${x1},${y1} C${x1},${cy} ${x2},${cy} ${x2},${y2}" fill="none" stroke="${stroke}" stroke-width="${sw}" />`;
        }
      }
      if (showTest) {
        for (const e of EDGES) {
          if (e.kind !== "tests_with") continue;
          const u = e.from.split(":")[1];
          const v = e.to.split(":")[1];
          const p1 = pos[u];
          const p2 = pos[v];
          if (!p1 || !p2) continue;
          const active = u === selectedDagMod || v === selectedDagMod;
          const stroke = active ? "#a855f7" : "rgba(168,85,247,0.3)";
          const sw = active ? 2.0 : 1.1;
          const x1 = p1.x + p1.w / 2;
          const y1 = p1.y + p1.h / 2;
          const x2 = p2.x + p2.w / 2;
          const y2 = p2.y + p2.h / 2;
          edgesSvg += `<line x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}" stroke="${stroke}" stroke-width="${sw}" stroke-dasharray="4,3" />`;
        }
      }

      let strataLabels = "";
      for (let st = 0; st <= 9; st++) {
        const row = 9 - st;
        const y = padY + row * rowH + 25;
        strataLabels += `
          <text x="14" y="${y}" fill="var(--muted-foreground)" font-size="12" font-family="monospace" font-weight="600">
            Stratum ${st}
          </text>
        `;
      }

      let nodesSvg = "";
      for (const [mname, p] of Object.entries(pos)) {
        const m = p.mod;
        const matchCrate = crateFilter === "all" || m.crate === crateFilter;
        const matchGate = gateFilter === "all" || m.gate === gateFilter;
        const inSub = highlightedSubMembers.size === 0 || highlightedSubMembers.has(`module:${mname}`);
        const visible = matchCrate && matchGate && inSub;
        const opacity = visible ? 1.0 : 0.2;

        let fill = "rgba(30,41,59,0.9)";
        let stroke = "#475569";
        if (m.crate === "core") stroke = "#10b981";
        else if (String(m.gate).includes("windows")) stroke = "#a855f7";
        else if (m.crate === "research") stroke = "#3b82f6";
        else stroke = "#f59e0b";

        let sw = 1.4;
        if (mname === selectedDagMod) {
          stroke = "#ffffff";
          sw = 2.8;
          fill = "rgba(59,130,246,0.28)";
        } else if (upstreamSet.has(mname)) {
          stroke = "#06b6d4";
          sw = 2.4;
        } else if (downstreamSet.has(mname)) {
          stroke = "#f59e0b";
          sw = 2.4;
        } else if (testUpstreamSet.has(mname)) {
          stroke = "#a855f7";
          sw = 2.0;
        }

        const reach = loadBearing(m);
        nodesSvg += `
          <g class="dag-node" data-dag-mod="${esc(mname)}" style="opacity:${opacity}">
            <rect x="${p.x}" y="${p.y}" width="${p.w}" height="${p.h}" rx="7" fill="${fill}" stroke="${stroke}" stroke-width="${sw}" />
            <text x="${p.x + 8}" y="${p.y + 17}" fill="#f8fafc" font-size="12" font-family="monospace" font-weight="700">
              ${esc(mname.slice(0, 22))}
            </text>
            <text x="${p.x + 8}" y="${p.y + 33}" fill="#94a3b8" font-size="12" font-family="monospace">
              ${esc(m.crate)} &bull; &uarr;${reach} &bull; ${m.mutation_sites || 0}m
            </text>
          </g>
        `;
      }

      document.getElementById("dag-svg-wrap").innerHTML = `
        <svg width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">
          ${strataLabels}
          ${edgesSvg}
          ${nodesSvg}
        </svg>
      `;

      document.querySelectorAll("[data-dag-mod]").forEach(g => {
        g.addEventListener("click", () => {
          highlightedSubMembers.clear();
          selectedDagMod = g.getAttribute("data-dag-mod");
          renderDag();
          inspectItem({ id: `module:${selectedDagMod}` });
        });
      });

      if (selMod) {
        const upCards = (selMod.deps || []).map(d => {
          const syms = (selMod.symbol_deps && selMod.symbol_deps[d]) || [];
          return `<span class="pill mono" style="border-color:#06b6d4;">${esc(d)}: ${esc(fmtSyms(syms, 5) || "(module)")}</span>`;
        }).join(" ");
        const downCards = Array.from(downstreamSet).sort().map(u => {
          const um = MOD_BY_NAME.get(u);
          const syms = (um?.symbol_deps && um.symbol_deps[selectedDagMod]) || [];
          return `<span class="pill mono" style="border-color:#f59e0b;">${esc(u)} &larr; [${esc(fmtSyms(syms, 4) || "module")}]</span>`;
        }).join(" ");
        document.getElementById("dag-edge-detail").innerHTML = `
          <div class="surface-subtle p-3 text-xs space-y-2">
            <div class="font-bold mono">Selected in DAG: <code>${esc(selectedDagMod)}</code> (Stratum ${DAG.stratum[selectedDagMod] ?? 0}, ${loadBearing(selMod)} transitive dependents)</div>
            <div><strong style="color:#06b6d4;">Upstream DEPENDS ON (${(selMod.deps || []).length}):</strong> ${upCards || "<span class='muted'>none (Stratum 0 root)</span>"}</div>
            <div><strong style="color:#f59e0b;">Downstream DEPENDED ON BY (${downstreamSet.size}):</strong> ${downCards || "<span class='muted'>none (top leaf)</span>"}</div>
          </div>
        `;
      }
    }

    function renderSubsystems() {
      const q = document.getElementById("sub-filter-input").value.trim().toLowerCase();
      const filtered = SUBS
        .filter(s => !q || s.name.toLowerCase().includes(q) || s.members.some(m => m.toLowerCase().includes(q)))
        .sort((a, b) => (b.members.length - a.members.length) || a.name.localeCompare(b.name));

      document.getElementById("sub-grid").innerHTML = filtered.map(sub => `
        <div class="surface-subtle p-2.5 clickable-row text-xs" data-sub-name="${esc(sub.name)}">
          <div class="flex items-center justify-between">
            <span class="font-bold mono">${esc(sub.name)}</span>
            <span class="pill mono">${sub.members.length} modules</span>
          </div>
          <div class="muted mono mt-1">${esc(sub.members.map(m => m.split(":")[1]).join(", "))}</div>
        </div>
      `).join("");

      document.querySelectorAll("[data-sub-name]").forEach(el => {
        el.addEventListener("click", () => {
          const sname = el.getAttribute("data-sub-name");
          const sub = SUBS.find(s => s.name === sname);
          if (!sub) return;
          highlightedSubMembers = new Set(sub.members);
          if (sub.members[0]) selectedDagMod = sub.members[0].split(":")[1];
          renderDag();
        });
      });
    }

    // =========================================================================
    // TAB 3: 4-TIER BLAST-RADIUS (impact) & PARALLEL WAVES (wave)
    // =========================================================================
    const IMPACT_PRESETS = [
      { label: "Octree & Scale Assay Wave (7 mods)", val: "world camera render commit_r1 octree_commit scale_assay optimization" },
      { label: "Core Hub: world (0 mut sites -> 1-hop fallback)", val: "world" },
      { label: "Core Hub: protocol (34 dependents)", val: "protocol" },
      { label: "Render & Framebuffer", val: "frame render ui" },
      { label: "Symbol Auto-Resolve: World::exposed_faces", val: "World::exposed_faces" },
    ];

    document.getElementById("impact-presets").innerHTML = IMPACT_PRESETS.map(p => `
      <button class="btn text-xs mono" data-impact-preset="${esc(p.val)}">${esc(p.label)}</button>
    `).join("");
    document.querySelectorAll("[data-impact-preset]").forEach(b => {
      b.addEventListener("click", () => {
        document.getElementById("impact-input").value = b.getAttribute("data-impact-preset");
        renderImpact();
      });
    });
    document.getElementById("impact-run-btn").addEventListener("click", renderImpact);
    document.getElementById("impact-input").addEventListener("keydown", e => {
      if (e.key === "Enter") renderImpact();
    });

    function resolveImpactTokens(rawStr) {
      const rawTokens = rawStr.split(/[\s,]+/).map(t => t.trim()).filter(Boolean);
      const targets = [];
      const resolvedNotes = [];
      for (const tok of rawTokens) {
        let clean = tok.startsWith("module:") ? tok.slice(7) : tok;
        if (clean.endsWith(".rs")) clean = clean.slice(0, -3).split("/").pop();
        if (MOD_BY_NAME.has(clean)) {
          if (!targets.includes(clean)) targets.push(clean);
          continue;
        }
        if (clean.includes("::")) {
          const p0 = clean.split("::")[0];
          if (MOD_BY_NAME.has(p0)) {
            resolvedNotes.push(`Resolved symbol ${tok} -> module ${p0}`);
            if (!targets.includes(p0)) targets.push(p0);
            continue;
          }
        }
        const low = clean.toLowerCase();
        for (const m of MODULES) {
          for (const s of (m.symbols || [])) {
            const qual = s.impl ? `${s.impl}::${s.name}` : "";
            if (s.name.toLowerCase() === low || qual.toLowerCase() === low) {
              resolvedNotes.push(`Resolved symbol ${tok} -> module ${m.name}`);
              if (!targets.includes(m.name)) targets.push(m.name);
              break;
            }
          }
        }
      }
      return { targets, resolvedNotes };
    }

    function renderImpact() {
      const raw = document.getElementById("impact-input").value;
      const { targets, resolvedNotes } = resolveImpactTokens(raw);
      document.getElementById("impact-cli-cmd").textContent = `python3 tools/bbgraph.py impact ${targets.join(" ")}`;

      // Render module toggle pills
      const tset = new Set(targets);
      document.getElementById("impact-mod-pills").innerHTML = MODULES.map(m => `
        <button class="btn text-xs mono ${tset.has(m.name) ? "btn-active" : ""}" data-toggle-impact="${esc(m.name)}">
          ${esc(m.name)}
        </button>
      `).join("");
      document.querySelectorAll("[data-toggle-impact]").forEach(b => {
        b.addEventListener("click", () => {
          const mname = b.getAttribute("data-toggle-impact");
          const next = new Set(targets);
          if (next.has(mname)) next.delete(mname);
          else next.add(mname);
          document.getElementById("impact-input").value = [...next].join(" ");
          renderImpact();
        });
      });

      const outBox = document.getElementById("impact-output");
      if (!targets.length) {
        outBox.innerHTML = `<div class="surface-card p-4 muted text-xs">Select at least one module above to compute blast radius and parallel execution waves.</div>`;
        return;
      }

      const imp = computeImpact(targets);
      const waves = computeWaves(targets);
      const scopeMods = new Set([...targets, ...imp.direct_prod, ...imp.direct_test_only]);
      const needsResearch = [...scopeMods].some(mname => {
        const m = MOD_BY_NAME.get(mname);
        return m && (m.crate === "research" || String(m.gate).includes("protocol-research") || m.name === "main");
      });
      const cargoCmd = needsResearch
        ? "cargo test --workspace --features protocol-research"
        : "cargo test --workspace";

      const byStratum = {};
      for (const mname of imp.trans_2plus) {
        const st = DAG.stratum[mname] ?? 0;
        if (!byStratum[st]) byStratum[st] = [];
        byStratum[st].push(mname);
      }

      outBox.innerHTML = `
        ${resolvedNotes.length ? `
          <div class="surface-card p-3 text-xs mono border-l-4 border-blue-500">
            ${resolvedNotes.map(n => `<div>${esc(n)}</div>`).join("")}
          </div>
        ` : ""}

        <div class="grid grid-cols-1 lg:grid-cols-2 gap-4">
          <!-- TIER 1: TARGET MODULES -->
          <div class="surface-card p-4">
            <div class="flex items-center justify-between mb-2">
              <span class="font-bold text-sm">TIER 1 &mdash; TARGET MODULES (${targets.length})</span>
              <span class="pill pill-danger mono">Direct Edit Scope</span>
            </div>
            <div class="space-y-2">
              ${targets.map(t => {
                const m = MOD_BY_NAME.get(t);
                const otherTargets = new Set(targets.filter(x => x !== t));
                const intraProd = (m.deps || []).filter(d => otherTargets.has(d));
                return `
                  <div class="surface-subtle p-2.5 clickable-row text-xs" data-inspect-id="module:${esc(t)}">
                    <div class="flex items-center justify-between">
                      <span class="font-bold mono">${esc(t)}</span>
                      <span class="pill ${gatePillClass(m.gate, m.crate)} mono">cfg(${esc(m.gate)}) &bull; S${DAG.stratum[t] ?? 0}</span>
                    </div>
                    <div class="muted mono mt-0.5">${esc(m.path)} &bull; ${m.mutation_sites || 0} mutation sites &bull; ${m.tests} tests</div>
                    ${intraProd.length ? `<div class="mono text-xs mt-1">intra-target: ${esc(intraProd.map(d => `${d}[${fmtSyms((m.symbol_deps && m.symbol_deps[d]) || [], 4)}]`).join(", "))}</div>` : ""}
                  </div>
                `;
              }).join("")}
            </div>
          </div>

          <!-- TIER 4: VERIFICATION RECIPE -->
          <div class="surface-card p-4">
            <div class="flex items-center justify-between mb-2">
              <span class="font-bold text-sm">TIER 4 &mdash; RUNNABLE VERIFICATION RECIPE</span>
              <span class="pill pill-high mono">Minimal Gate Set</span>
            </div>
            <div class="space-y-2.5 text-xs mono">
              <div class="surface-subtle p-2.5">
                <div class="muted font-sans font-semibold mb-1">1. Rust Workspace Unit &amp; Integration Tests:</div>
                <code>${esc(cargoCmd)}</code>
              </div>
              <div class="surface-subtle p-2.5">
                <div class="muted font-sans font-semibold mb-1">2. Direct Independent Python Vector Suites (${imp.direct_vecs.length}):</div>
                <code>${imp.direct_vecs.length
                  ? (imp.direct_vecs.length <= 6 ? imp.direct_vecs.map(v => `python3 -B tools/${v}.py`).join(" && ") : `python3 tools/bbverify.py full  # (${imp.direct_vecs.length} suites: ${imp.direct_vecs.join(", ")})`)
                  : "none"}</code>
              </div>
              <div class="surface-subtle p-2.5">
                <div class="muted font-sans font-semibold mb-1">3. 1-Hop Dependent Vector Suites (${imp.hop1_vecs.length}):</div>
                <code>${imp.hop1_vecs.length ? imp.hop1_vecs.join(", ") : "none"}</code>
              </div>
              <div class="surface-subtle p-2.5">
                <div class="muted font-sans font-semibold mb-1">4. Canonical Byte-Order Mutation Gate (bbmutate.py):</div>
                ${imp.mut_targets.length ? `<div><code>python3 tools/bbmutate.py ${esc(imp.mut_targets.join(" "))}</code></div>` : ""}
                ${imp.mut_1hop.length ? `<div class="mt-1"><code>python3 tools/bbmutate.py ${esc(imp.mut_1hop.join(" "))}  # 1-hop encoders for 0-site target(s)</code></div>` : ""}
                ${(!imp.mut_targets.length && !imp.mut_1hop.length) ? `<div><code>none (0 canonical encoder sites in target or 1-hop scope)</code></div>` : ""}
              </div>
              <div class="surface-subtle p-2.5">
                <div class="muted font-sans font-semibold mb-1">5. Lean 4 Formal Proof Capsule:</div>
                <code>${imp.proofs.length ? `${imp.proofs.join(", ")}  ((cd proof && lake build))` : "none"}</code>
              </div>
            </div>
          </div>
        </div>

        <div class="grid grid-cols-1 lg:grid-cols-2 gap-4">
          <!-- TIER 2: 1-HOP DEPENDENTS -->
          <div class="surface-card p-4">
            <div class="flex items-center justify-between mb-2">
              <span class="font-bold text-sm">TIER 2 &mdash; DIRECT 1-HOP DEPENDENTS (${imp.direct_prod.length} prod, ${imp.direct_test_only.length} #[cfg(test)])</span>
              <span class="pill pill-low mono">Symbol Coupling</span>
            </div>
            <div class="space-y-1.5 max-h-80 overflow-y-auto">
              ${imp.direct_prod.map(d => {
                const dm = MOD_BY_NAME.get(d);
                const parts = targets.filter(t => (dm.deps || []).includes(t)).map(t => {
                  const syms = (dm.symbol_deps && dm.symbol_deps[t]) || [];
                  const tsyms = (dm.test_symbol_deps && dm.test_symbol_deps[t]) || [];
                  return `${t}: [${fmtSyms(syms, 5)}]${tsyms.length ? ` +#[cfg(test)]: [${fmtSyms(tsyms, 3)}]` : ""}`;
                });
                return `
                  <div class="surface-subtle p-2 clickable-row text-xs" data-inspect-id="module:${esc(d)}">
                    <div class="flex items-center justify-between">
                      <span class="font-bold mono">${esc(d)}</span>
                      <span class="muted mono">cfg(${esc(dm.gate)})</span>
                    </div>
                    <div class="muted mono mt-0.5">&larr; ${esc(parts.join("; "))}</div>
                  </div>
                `;
              }).join("") || "<div class='muted text-xs'>(no production 1-hop dependents)</div>"}

              ${imp.direct_test_only.map(d => {
                const dm = MOD_BY_NAME.get(d);
                const parts = targets.filter(t => (dm.test_deps || []).includes(t)).map(t => {
                  const tsyms = (dm.test_symbol_deps && dm.test_symbol_deps[t]) || [];
                  return `${t} (#[cfg(test)]): [${fmtSyms(tsyms, 5)}]`;
                });
                return `
                  <div class="surface-subtle p-2 clickable-row text-xs" data-inspect-id="module:${esc(d)}">
                    <div class="flex items-center justify-between">
                      <span class="font-bold mono">${esc(d)}</span>
                      <span class="pill pill-win mono">#[cfg(test)] only</span>
                    </div>
                    <div class="muted mono mt-0.5">&larr; ${esc(parts.join("; "))}</div>
                  </div>
                `;
              }).join("")}
            </div>
          </div>

          <!-- TIER 3 & PARALLEL SUBAGENT WAVES -->
          <div class="space-y-4">
            <div class="surface-card p-4">
              <div class="flex items-center justify-between mb-2">
                <span class="font-bold text-sm">TIER 3 &mdash; TRANSITIVE DEPENDENTS (2+ HOPS) (${imp.trans_2plus.length})</span>
                <span class="pill mono">By Stratum</span>
              </div>
              <div class="space-y-1.5 text-xs">
                ${Object.keys(byStratum).sort((a, b) => Number(a) - Number(b)).map(st => `
                  <div class="surface-subtle p-2">
                    <span class="font-bold mono muted">Stratum ${st}:</span>
                    <span class="mono ml-2">${esc(byStratum[st].join(", "))}</span>
                  </div>
                `).join("") || "<div class='muted text-xs'>(none)</div>"}
              </div>
            </div>

            <div class="surface-card p-4">
              <div class="flex items-center justify-between mb-2">
                <span class="font-bold text-sm">PARALLEL SUBAGENT WAVE SCHEDULE (${waves.length} Wave${waves.length === 1 ? "" : "s"})</span>
                <span class="pill pill-core mono">deps &cup; test_deps</span>
              </div>
              <div class="space-y-2 text-xs">
                ${waves.map((w, idx) => `
                  <div class="surface-subtle p-2.5">
                    <div class="font-bold mono mb-1">WAVE ${idx + 1} (${w.length} parallel subagent lock${w.length === 1 ? "" : "s"})</div>
                    <div class="space-y-1">
                      ${w.map(mname => {
                        const mod = MOD_BY_NAME.get(mname);
                        const prodWait = (mod.deps || []).filter(d => tset.has(d)).map(d => `${d}[${fmtSyms((mod.symbol_deps && mod.symbol_deps[d]) || [], 4)}]`);
                        const testWait = (mod.test_deps || []).filter(d => tset.has(d)).map(d => `${d}[${fmtSyms((mod.test_symbol_deps && mod.test_symbol_deps[d]) || [], 4)}] (test)`);
                        const allWait = [...prodWait, ...testWait];
                        return `
                          <div class="mono">
                            <strong>${esc(mname)}</strong>
                            <span class="muted"> &mdash; ${allWait.length ? `waits on: ${esc(allWait.join(", "))}` : "root in wave set"}</span>
                          </div>
                        `;
                      }).join("")}
                    </div>
                  </div>
                `).join("")}
              </div>
            </div>
          </div>
        </div>
      `;
      bindInspectClicks(outBox);
    }

    // =========================================================================
    // TAB 4: SYMBOLS, SIGNATURE TYPES & 2-PASS CALLERS
    // =========================================================================
    const TYPE_PRESETS = [
      "SparseOctreeV1", "ContentTag", "World", "Coord", "CameraProjection",
      "Aabb", "JournalIo", "MortonKeyV1", "Frame", "Receipt", "TickOutcome",
    ];
    const CALLER_PRESETS = [
      "CameraProjection::aabb_definitely_outside_frustum",
      "World::exposed_faces",
      "JournalIo::metadata_len",
      "SparseOctreeV1::from_world",
      "MortonKeyV1::from_coord",
      "ray_aabb_entry_distance",
      "bounds_aabb",
      "hash_canonical",
    ];

    document.getElementById("type-presets").innerHTML = TYPE_PRESETS.map(t => `
      <button class="btn text-xs mono" data-type-preset="${esc(t)}">${esc(t)}</button>
    `).join("");
    document.querySelectorAll("[data-type-preset]").forEach(b => {
      b.addEventListener("click", () => {
        document.getElementById("type-input").value = b.getAttribute("data-type-preset");
        renderTypeExplorer();
      });
    });
    document.getElementById("type-run-btn").addEventListener("click", renderTypeExplorer);
    document.getElementById("type-input").addEventListener("keydown", e => {
      if (e.key === "Enter") renderTypeExplorer();
    });

    document.getElementById("caller-presets").innerHTML = CALLER_PRESETS.map(c => `
      <button class="btn text-xs mono" data-caller-preset="${esc(c)}">${esc(c.split("::").pop())}</button>
    `).join("");
    document.querySelectorAll("[data-caller-preset]").forEach(b => {
      b.addEventListener("click", () => {
        document.getElementById("caller-input").value = b.getAttribute("data-caller-preset");
        renderCallerExplorer();
      });
    });
    document.getElementById("caller-run-btn").addEventListener("click", renderCallerExplorer);
    document.getElementById("caller-input").addEventListener("keydown", e => {
      if (e.key === "Enter") renderCallerExplorer();
    });

    function resolveTypeName(query) {
      let q = String(query || "").trim();
      if (!q) return null;
      const allTypes = new Set();
      for (const m of MODULES) {
        for (const s of (m.symbols || [])) {
          if (["struct", "enum", "trait", "type"].includes(s.kind)) allTypes.add(s.name);
          if (s.impl) allTypes.add(s.impl);
          for (const t of (s.takes || [])) allTypes.add(t);
          for (const t of (s.returns || [])) allTypes.add(t);
        }
      }
      if (q.includes("::") && !allTypes.has(q)) q = q.split("::").pop();
      if (allTypes.has(q)) return q;
      const low = q.toLowerCase();
      const ci = [...allTypes].filter(t => t.toLowerCase() === low).sort();
      return ci[0] || null;
    }

    function renderTypeExplorer() {
      const q = document.getElementById("type-input").value;
      const tname = resolveTypeName(q);
      const box = document.getElementById("type-output");
      if (!tname) {
        box.innerHTML = `<div class="muted text-xs">No type matched <code>${esc(q)}</code>.</div>`;
        return;
      }

      const defs = [];
      const returnedBy = [];
      const methods = [];
      const takenBy = [];
      for (const m of MODULES) {
        for (const s of (m.symbols || [])) {
          if (s.name === tname && ["struct", "enum", "trait", "type"].includes(s.kind)) defs.push([m, s]);
          if ((s.returns || []).includes(tname)) returnedBy.push([m, s]);
          if (s.impl === tname) methods.push([m, s]);
          if ((s.takes || []).includes(tname) && s.impl !== tname) takenBy.push([m, s]);
        }
      }
      returnedBy.sort((a, b) => ((a[1].impl !== tname) - (b[1].impl !== tname)) || a[0].name.localeCompare(b[0].name) || (a[1].line - b[1].line));
      methods.sort((a, b) => ((!a[1].pub) - (!b[1].pub)) || a[0].name.localeCompare(b[0].name) || (a[1].line - b[1].line));

      box.innerHTML = `
        <div class="surface-subtle p-2.5 text-xs">
          <div class="font-bold mono mb-1">DEFINED AT (${defs.length})</div>
          ${defs.map(([m, s]) => `
            <div class="clickable-row p-1.5 rounded" data-inspect-sym="${esc(m.name)}|${esc(s.name)}|${s.line}">
              <span class="mono font-bold">${esc(s.pub ? "pub " : "")}${esc(s.kind)} ${esc(s.name)}</span>
              <span class="muted mono ml-2">${esc(m.path)}:${s.line}-${s.end}</span>
              ${s.doc ? `<div class="muted mt-0.5">${esc(s.doc)}</div>` : ""}
            </div>
          `).join("") || "<div class='muted'>(external or built-in type)</div>"}
        </div>

        <div class="surface-subtle p-2.5 text-xs">
          <div class="font-bold mono mb-1">RETURNED BY &mdash; CONSTRUCTORS &amp; PRODUCERS (${returnedBy.length})</div>
          <div class="max-h-40 overflow-y-auto space-y-1">
            ${returnedBy.slice(0, 40).map(([m, s]) => {
              const qual = s.impl ? `${s.impl}::${s.name}` : `${m.name}::${s.name}`;
              return `
                <div class="clickable-row p-1.5 rounded" data-inspect-sym="${esc(m.name)}|${esc(s.name)}|${s.line}">
                  <div class="flex items-center justify-between">
                    <span class="mono font-semibold">${s.impl === tname ? `<span class="pill pill-core mr-1">[ctor]</span>` : ""}${esc(qual)}</span>
                    <span class="muted mono">${esc(m.name)}:${s.line}</span>
                  </div>
                </div>
              `;
            }).join("") || "<div class='muted'>(none)</div>"}
          </div>
        </div>

        <div class="surface-subtle p-2.5 text-xs">
          <div class="font-bold mono mb-1">METHODS (impl ${esc(tname)}) (${methods.length})</div>
          <div class="max-h-44 overflow-y-auto space-y-1">
            ${methods.map(([m, s]) => `
              <div class="clickable-row p-1.5 rounded" data-inspect-sym="${esc(m.name)}|${esc(s.name)}|${s.line}">
                <div class="flex items-center justify-between">
                  <span class="mono font-semibold">${esc(s.pub ? "pub " : "")}${esc(tname)}::${esc(s.name)}</span>
                  <span class="muted mono">:${s.line}-${s.end}</span>
                </div>
                ${s.sig ? `<div class="muted mono truncate mt-0.5">${esc(s.sig)}</div>` : ""}
              </div>
            `).join("") || "<div class='muted'>(none)</div>"}
          </div>
        </div>

        <div class="surface-subtle p-2.5 text-xs">
          <div class="font-bold mono mb-1">TAKEN BY &mdash; EXTERNAL CONSUMERS (${takenBy.length})</div>
          <div class="max-h-40 overflow-y-auto space-y-1">
            ${takenBy.slice(0, 40).map(([m, s]) => {
              const qual = s.impl ? `${s.impl}::${s.name}` : `${m.name}::${s.name}`;
              return `
                <div class="clickable-row p-1.5 rounded" data-inspect-sym="${esc(m.name)}|${esc(s.name)}|${s.line}">
                  <span class="mono font-semibold">${esc(qual)}</span>
                  <span class="muted mono ml-2">${esc(m.path)}:${s.line}</span>
                </div>
              `;
            }).join("") || "<div class='muted'>(none)</div>"}
          </div>
        </div>
      `;
      bindInspectClicks(box);
    }

    function collectCallers(targetQuery) {
      const q = String(targetQuery || "").trim();
      let normQ = q;
      let qualMod = null;
      let qualFn = null;
      const parts = q.split("::");
      if (parts.length === 3 && MODULE_NAMES.has(parts[0])) {
        normQ = `${parts[1]}::${parts[2]}`;
      } else if (parts.length === 2 && MODULE_NAMES.has(parts[0])) {
        qualMod = parts[0];
        qualFn = parts[1];
      }

      function matchCall(c, callerMod, isTest, ci) {
        if (qualMod && qualFn) {
          const reach = new Set([callerMod.name, ...(callerMod.deps || []), ...(isTest ? (callerMod.test_deps || []) : [])]);
          if (!reach.has(qualMod)) return false;
          return ci
            ? (c.toLowerCase() === normQ.toLowerCase() || c.toLowerCase() === qualFn.toLowerCase())
            : (c === normQ || c === qualFn);
        }
        if (normQ.includes("::")) {
          return ci ? c.toLowerCase() === normQ.toLowerCase() : c === normQ;
        }
        const cl = ci ? c.toLowerCase() : c;
        const nl = ci ? normQ.toLowerCase() : normQ;
        return cl === nl || cl.endsWith("::" + nl);
      }

      function runPass(ci) {
        const prodHits = [];
        const testHits = [];
        for (const m of MODULES) {
          for (const s of (m.symbols || [])) {
            if (s.kind === "fn" && (s.calls || []).some(c => matchCall(c, m, false, ci))) {
              prodHits.push([m, s]);
            }
          }
          for (const tf of (m.test_fns || [])) {
            if ((tf.calls || []).some(c => matchCall(c, m, true, ci))) {
              testHits.push([m, tf]);
            }
          }
        }
        return { prodHits, testHits };
      }

      let { prodHits, testHits } = runPass(false);
      if (!prodHits.length && !testHits.length) {
        ({ prodHits, testHits } = runPass(true));
      }
      return { normQ, prodHits, testHits };
    }

    function renderCallerExplorer() {
      const q = document.getElementById("caller-input").value.trim();
      const { normQ, prodHits, testHits } = collectCallers(q);
      const box = document.getElementById("caller-output");

      const activeMods = (testHits.length ? testHits : prodHits).map(([m]) => m);
      let cargoCmd = "cargo test --workspace";
      if (activeMods.length) {
        const crates = new Set(activeMods.map(m => m.crate));
        if (crates.size === 1 && crates.has("core")) cargoCmd = "cargo test -p bitblock-core";
        else if (crates.size === 1 && crates.has("research")) cargoCmd = "cargo test -p bitblock-research";
        else if (crates.size === 1 && crates.has("binary")) cargoCmd = "cargo test --bin bitblock --features protocol-research";
        else if (activeMods.some(m => m.crate !== "core" || m.gate !== "default")) {
          cargoCmd = "cargo test --workspace --features protocol-research";
        }
        if (testHits.length === 1) cargoCmd += " " + testHits[0][1].name;
      }

      box.innerHTML = `
        <div class="surface-subtle p-2.5 text-xs">
          <div class="flex items-center justify-between mb-1">
            <span class="font-bold mono">PRODUCTION CALLERS (${prodHits.length})</span>
            <span class="pill mono">${esc(normQ)}</span>
          </div>
          <div class="space-y-1 max-h-44 overflow-y-auto">
            ${prodHits.map(([m, s]) => {
              const qual = s.impl ? `${m.name}::${s.impl}::${s.name}` : `${m.name}::${s.name}`;
              return `
                <div class="clickable-row p-1.5 rounded" data-inspect-sym="${esc(m.name)}|${esc(s.name)}|${s.line}">
                  <div class="flex items-center justify-between">
                    <span class="mono font-semibold">${esc(qual)}</span>
                    <span class="muted mono">${esc(m.path)}:${s.line}-${s.end}</span>
                  </div>
                </div>
              `;
            }).join("") || "<div class='muted'>(none)</div>"}
          </div>
        </div>

        <div class="surface-subtle p-2.5 text-xs">
          <div class="font-bold mono mb-1">UNIT TEST CALLERS (#[test]) (${testHits.length})</div>
          <div class="space-y-1 max-h-44 overflow-y-auto">
            ${testHits.map(([m, tf]) => `
              <div class="clickable-row p-1.5 rounded" data-inspect-id="module:${esc(m.name)}">
                <div class="flex items-center justify-between">
                  <span class="mono font-semibold">${esc(m.name)}::tests::${esc(tf.name)}</span>
                  <span class="muted mono">${esc(m.path)}:${tf.line}-${tf.end}</span>
                </div>
              </div>
            `).join("") || "<div class='muted'>(none)</div>"}
          </div>
        </div>

        <div class="surface-subtle p-2.5 text-xs mono">
          <span class="muted font-sans font-semibold">Run Crate-Aware Test Command:</span>
          <div class="mt-1"><code>${esc(cargoCmd)}</code></div>
        </div>
      `;
      bindInspectClicks(box);
    }

    // Populate symbol catalog module dropdown & table
    {
      const modSel = document.getElementById("sym-table-mod");
      for (const m of MODULES.slice().sort((a, b) => a.name.localeCompare(b.name))) {
        const opt = document.createElement("option");
        opt.value = m.name;
        opt.textContent = `${m.name} (${(m.symbols || []).length})`;
        modSel.appendChild(opt);
      }
      ["sym-table-search", "sym-table-kind", "sym-table-mod"].forEach(id => {
        document.getElementById(id).addEventListener("input", renderSymCatalog);
        document.getElementById(id).addEventListener("change", renderSymCatalog);
      });
    }

    function renderSymCatalog() {
      const q = document.getElementById("sym-table-search").value.trim().toLowerCase();
      const kindFilter = document.getElementById("sym-table-kind").value;
      const modFilter = document.getElementById("sym-table-mod").value;

      const rows = [];
      for (const m of MODULES) {
        if (modFilter !== "all" && m.name !== modFilter) continue;
        for (const s of (m.symbols || [])) {
          if (kindFilter !== "all" && s.kind !== kindFilter) continue;
          const qual = s.impl ? `${s.impl}::${s.name}` : s.name;
          if (q && !qual.toLowerCase().includes(q) && !String(s.sig || "").toLowerCase().includes(q) && !String(s.doc || "").toLowerCase().includes(q)) {
            continue;
          }
          rows.push({ m, s, qual });
          if (rows.length >= 150) break;
        }
        if (rows.length >= 150) break;
      }

      const wrap = document.getElementById("sym-table-wrap");
      wrap.innerHTML = `
        <table class="audit-table">
          <thead>
            <tr>
              <th>Symbol</th>
              <th>Kind</th>
              <th>Span</th>
              <th>Signature / Types</th>
              <th>Doc</th>
            </tr>
          </thead>
          <tbody>
            ${rows.map(({ m, s, qual }) => `
              <tr class="clickable-row" data-inspect-sym="${esc(m.name)}|${esc(s.name)}|${s.line}">
                <td class="mono font-semibold">${esc(qual)}</td>
                <td class="mono">${esc(s.pub ? "pub " : "")}${esc(s.kind)}</td>
                <td class="mono muted">${esc(m.path)}:${s.line}-${s.end}</td>
                <td class="mono text-xs">${esc(String(s.sig || "").slice(0, 95))}</td>
                <td class="muted text-xs">${esc(s.doc || "")}</td>
              </tr>
            `).join("")}
          </tbody>
        </table>
      `;
      bindInspectClicks(wrap);
    }

    // =========================================================================
    // TAB 5: PROTOCOL SCHEMAS, #bb. TAG TRIANGULATOR, PROOFS & METRICS
    // =========================================================================
    let activeProtoSub = "tags";
    document.querySelectorAll(".proto-sub-btn").forEach(btn => {
      btn.addEventListener("click", () => {
        activeProtoSub = btn.getAttribute("data-sub");
        document.querySelectorAll(".proto-sub-btn").forEach(b => {
          b.classList.toggle("btn-active", b.getAttribute("data-sub") === activeProtoSub);
        });
        renderProtoTab();
      });
    });
    document.getElementById("proto-search").addEventListener("input", renderProtoTab);

    // Precompute tag triangulation index across all nodes
    const TAG_INDEX = new Map();
    for (const n of NODES) {
      const tags = n.kind === "metric" ? (n.tag ? [n.tag] : []) : (n.pinned_tags || []);
      for (const t of tags) {
        if (!TAG_INDEX.has(t)) TAG_INDEX.set(t, []);
        TAG_INDEX.get(t).push(n);
      }
    }

    function renderProtoTab() {
      const q = document.getElementById("proto-search").value.trim().toLowerCase();
      const box = document.getElementById("proto-content");

      if (activeProtoSub === "tags") {
        const entries = Array.from(TAG_INDEX.entries())
          .filter(([t, arr]) => (arr.length > 1 || !t.startsWith("#bb.metric-definition.")) && (!q || t.toLowerCase().includes(q) || arr.some(n => n.id.toLowerCase().includes(q))))
          .sort((a, b) => (b[1].length - a[1].length) || a[0].localeCompare(b[0]));

        box.innerHTML = `
          <div class="muted text-xs mb-3">
            Showing ${Math.min(entries.length, 120)} of ${entries.length} content-addressed <code>#bb.&lt;kind&gt;.sha256.&lt;digest&gt;</code> tags (sorted by cross-language artifact count: Rust modules + Python vector suites + Protocol Markdown docs).
          </div>
          <div class="max-h-[540px] overflow-y-auto surface-subtle">
            <table class="audit-table">
              <thead>
                <tr>
                  <th>Canonical ContentTag</th>
                  <th>Peers</th>
                  <th>Rust Modules</th>
                  <th>Python Vector Suites</th>
                  <th>Docs / Examples</th>
                </tr>
              </thead>
              <tbody>
                ${entries.slice(0, 120).map(([tag, arr]) => {
                  const mods = arr.filter(n => n.kind === "module").map(n => n.name);
                  const tools = arr.filter(n => n.kind === "tool").map(n => n.name);
                  const docs = arr.filter(n => n.kind === "doc" || n.kind === "example").map(n => n.id);
                  return `
                    <tr class="clickable-row" data-inspect-tag="${esc(tag)}">
                      <td class="mono font-semibold break-all">${esc(tag)}</td>
                      <td><span class="pill ${arr.length >= 2 ? "pill-high" : ""} mono">${arr.length}</span></td>
                      <td class="mono">${esc(mods.join(", ") || "-")}</td>
                      <td class="mono">${esc(tools.join(", ") || "-")}</td>
                      <td class="mono muted">${esc(docs.join(", ") || "-")}</td>
                    </tr>
                  `;
                }).join("")}
              </tbody>
            </table>
          </div>
        `;
        bindInspectClicks(box);
        return;
      }

      if (activeProtoSub === "schemas") {
        const schemas = NODES.filter(n => n.kind === "schema" && (!q || n.name.toLowerCase().includes(q) || (n.declared_by || []).some(m => m.toLowerCase().includes(q))));
        box.innerHTML = `
          <div class="muted text-xs mb-3">
            ${COVERAGE.schemas} production protocol schemas (${COVERAGE.schemas_vectored}/${COVERAGE.schemas} = 100% independently vectored in Python) + ${COVERAGE.schemas_fixtures} <code>#[cfg(test)]</code> fixtures.
          </div>
          <div class="max-h-[540px] overflow-y-auto surface-subtle">
            <table class="audit-table">
              <thead>
                <tr>
                  <th>Schema String</th>
                  <th>Scope</th>
                  <th>Declared By (Rust)</th>
                  <th>Independent Python Vector Suites</th>
                </tr>
              </thead>
              <tbody>
                ${schemas.map(s => `
                  <tr class="clickable-row" data-inspect-id="${esc(s.id)}">
                    <td class="mono font-semibold">${esc(s.name)}</td>
                    <td>${s.fixture ? `<span class="pill pill-low">fixture</span>` : `<span class="pill pill-high">protocol</span>`}</td>
                    <td class="mono">${esc((s.declared_by || []).map(x => x.split(":")[1]).join(", "))} <span class="muted">(${esc((s.declared_at || [])[0] || "")})</span></td>
                    <td class="mono">${esc((s.vectored_by || []).map(x => x.split(":")[1]).join(", ") || "NONE")}</td>
                  </tr>
                `).join("")}
              </tbody>
            </table>
          </div>
        `;
        bindInspectClicks(box);
        return;
      }

      // Proofs & Metrics
      const proofs = NODES.filter(n => n.kind === "proof" && (!q || n.name.toLowerCase().includes(q) || (n.theorems || []).some(t => t.toLowerCase().includes(q))));
      const metrics = NODES.filter(n => n.kind === "metric" && (!q || n.name.toLowerCase().includes(q) || n.alias.toLowerCase().includes(q) || n.owner.toLowerCase().includes(q)));
      box.innerHTML = `
        <div class="mb-4">
          <h3 class="text-sm font-bold mb-2">Lean 4 Formal Proof Capsule (${proofs.length} Modules)</h3>
          <div class="grid grid-cols-1 md:grid-cols-3 gap-2.5">
            ${proofs.map(p => `
              <div class="surface-subtle p-3 clickable-row text-xs" data-inspect-id="${esc(p.id)}">
                <div class="flex items-center justify-between">
                  <span class="font-bold mono">${esc(p.name)}</span>
                  <span class="pill pill-core mono">${(p.theorems || []).length} theorems</span>
                </div>
                <div class="muted mono mt-0.5">${esc(p.path)} (${p.lines} lines)</div>
                <div class="mt-1.5">${esc(describe(p))}</div>
                <div class="mono muted mt-1.5">Proves: ${esc((p.cites_src || []).join(", "))}</div>
              </div>
            `).join("")}
          </div>
        </div>
        <div>
          <h3 class="text-sm font-bold mb-2">1,112-Family Metric Registry (showing ${Math.min(metrics.length, 80)} of ${metrics.length})</h3>
          <div class="max-h-96 overflow-y-auto surface-subtle">
            <table class="audit-table">
              <thead>
                <tr>
                  <th>Owner</th>
                  <th>Metric Family</th>
                  <th>Canonical Alias</th>
                  <th>Authority Definition Tag</th>
                </tr>
              </thead>
              <tbody>
                ${metrics.slice(0, 80).map(m => `
                  <tr class="clickable-row" data-inspect-id="${esc(m.id)}">
                    <td><span class="pill mono">${esc(m.owner)}</span></td>
                    <td class="font-semibold">${esc(m.name)}</td>
                    <td class="mono text-xs">${esc(m.alias)}</td>
                    <td class="mono text-xs muted">${esc(m.tag)}</td>
                  </tr>
                `).join("")}
              </tbody>
            </table>
          </div>
        </div>
      `;
      bindInspectClicks(box);
    }

    // =========================================================================
    // TAB 6: AUDITOR GATES, 90/90 IN-BROWSER SELFTEST, EVAL & COVERAGE
    // =========================================================================
    function runBrowserSelftests() {
      const results = [];

      // 1. queries (15)
      for (const c of (SELFTEST.queries || [])) {
        const r = rank(c.q);
        const top = r.scored.find(x => x.node.kind === "module")?.node.name || null;
        results.push({
          cat: "queries",
          name: `ask "${c.q}"`,
          expected: c.top,
          actual: top,
          pass: top === c.top,
          action: () => { switchTab("ask"); document.getElementById("ask-input").value = c.q; renderAsk(); },
        });
      }
      // 2. no_match (2)
      for (const c of (SELFTEST.no_match || [])) {
        const r = rank(c.q);
        const topScore = r.scored.length ? r.scored[0].score : 0.0;
        const maxAllowed = c.max_score ?? 3.0;
        results.push({
          cat: "no_match",
          name: `no_match "${c.q}"`,
          expected: `<= ${maxAllowed.toFixed(1)}`,
          actual: topScore.toFixed(2),
          pass: topScore <= maxAllowed,
          action: () => { switchTab("ask"); document.getElementById("ask-input").value = c.q; renderAsk(); },
        });
      }
      // 3. confidence (6)
      for (const c of (SELFTEST.confidence || [])) {
        const r = rank(c.q);
        const [lvl] = confidence(r.coverage, r.scored, r.anchored, r.named);
        results.push({
          cat: "confidence",
          name: `confidence "${c.q}"`,
          expected: c.expect,
          actual: lvl,
          pass: lvl.toLowerCase() === c.expect.toLowerCase(),
          action: () => { switchTab("ask"); document.getElementById("ask-input").value = c.q; renderAsk(); },
        });
      }
      // 4. reuse (3)
      if (SELFTEST.reuse) {
        const actualReuse = {
          production_duplicated_lines: REUSE.duplication.production.duplicated_lines,
          test_duplicated_lines: REUSE.duplication.test.duplicated_lines,
          unreferenced_pub_items: REUSE.dead.unreferenced.length,
        };
        for (const [k, exp] of Object.entries(SELFTEST.reuse)) {
          results.push({
            cat: "reuse",
            name: `reuse.${k}`,
            expected: String(exp),
            actual: String(actualReuse[k]),
            pass: actualReuse[k] === exp,
          });
        }
      }
      // 5. counts (32)
      for (const [k, exp] of Object.entries(SELFTEST.counts || {})) {
        let act = COVERAGE[k];
        let e = exp;
        if (Array.isArray(exp)) {
          act = (act || []).length;
          e = exp.length;
        }
        results.push({
          cat: "counts",
          name: `counts.${k}`,
          expected: String(e),
          actual: String(act),
          pass: act === e,
        });
      }
      // 6. structure (3)
      for (const c of (SELFTEST.structure || [])) {
        const got = DAG.transitive_dependents[c.module];
        results.push({
          cat: "structure",
          name: `dagdependents(${c.module})`,
          expected: String(c.dependents),
          actual: String(got),
          pass: got === c.dependents,
          action: () => { switchTab("dag"); selectedDagMod = c.module; renderDag(); },
        });
      }
      // 7. waves (2)
      for (const c of (SELFTEST.waves || [])) {
        const got = computeWaves(c.modules);
        results.push({
          cat: "waves",
          name: `wave(${c.modules.join(", ")})`,
          expected: JSON.stringify(c.expect),
          actual: JSON.stringify(got),
          pass: JSON.stringify(got) === JSON.stringify(c.expect),
          action: () => { switchTab("impact"); document.getElementById("impact-input").value = c.modules.join(" "); renderImpact(); },
        });
      }
      // 8. schemas (3)
      for (const c of (SELFTEST.schemas || [])) {
        const node = NODES.find(n => n.kind === "schema" && n.name === c.schema);
        const vectored = Boolean(node && node.vectored_by && node.vectored_by.length);
        results.push({
          cat: "schemas",
          name: `schema ${c.schema}`,
          expected: `vectored=${c.vectored}`,
          actual: `vectored=${vectored}`,
          pass: Boolean(node) && vectored === c.vectored,
          action: () => node && inspectItem({ id: node.id }),
        });
      }
      // 9. symbol_deps (3)
      for (const c of (SELFTEST.symbol_deps || [])) {
        const got = (MOD_BY_NAME.get(c.module)?.symbol_deps || {})[c.dep] || [];
        results.push({
          cat: "symbol_deps",
          name: `symbol_deps ${c.module} -> ${c.dep}`,
          expected: JSON.stringify(c.expect),
          actual: JSON.stringify(got),
          pass: JSON.stringify(got) === JSON.stringify(c.expect),
          action: () => inspectItem({ id: `module:${c.module}` }),
        });
      }
      // 10. test_symbol_deps (2)
      for (const c of (SELFTEST.test_symbol_deps || [])) {
        const mod = MOD_BY_NAME.get(c.module) || {};
        const gotTest = (mod.test_symbol_deps || {})[c.dep] || [];
        const gotProd = (mod.symbol_deps || {})[c.dep] || [];
        let ok = true;
        if (c.expect && JSON.stringify(gotTest) !== JSON.stringify(c.expect)) ok = false;
        if (c.expect_include && !c.expect_include.every(s => gotTest.includes(s))) ok = false;
        if (c.not_in_prod && c.not_in_prod.some(s => gotProd.includes(s))) ok = false;
        results.push({
          cat: "test_symbol_deps",
          name: `test_symbol_deps ${c.module} -> ${c.dep}`,
          expected: JSON.stringify(c.expect || c.expect_include),
          actual: JSON.stringify(gotTest),
          pass: ok,
          action: () => inspectItem({ id: `module:${c.module}` }),
        });
      }
      // 11. symbols (6)
      const symSpec = SELFTEST.symbols || {};
      if (symSpec.sig_type_consistency) {
        let bad = 0;
        for (const m of MODULES) {
          for (const s of (m.symbols || [])) {
            if (s.kind !== "fn") continue;
            const sig = s.sig || "";
            for (const t of [...(s.takes || []), ...(s.returns || [])]) {
              if (!(sig.includes(t) || (t === s.impl && (sig.includes("Self") || sig.includes("self"))))) bad++;
            }
          }
        }
        results.push({
          cat: "symbols",
          name: "symbols.sig_type_consistency",
          expected: "0 violations",
          actual: `${bad} violations`,
          pass: bad === 0,
        });
      }
      if (symSpec.test_fns_match_coverage) {
        const gotTests = MODULES.reduce((a, m) => a + (m.test_fns ? m.test_fns.length : 0), 0);
        results.push({
          cat: "symbols",
          name: "symbols.test_fns_match_coverage",
          expected: String(COVERAGE.tests),
          actual: String(gotTests),
          pass: gotTests === COVERAGE.tests,
        });
      }
      if (symSpec.pub_counts) {
        const gotTot = MODULES.reduce((a, m) => a + (m.pub_symbol_count || 0), 0);
        const gotDoc = MODULES.reduce((a, m) => a + (m.pub_documented || 0), 0);
        results.push({
          cat: "symbols",
          name: "symbols.pub_counts",
          expected: `${symSpec.pub_counts.documented}/${symSpec.pub_counts.total}`,
          actual: `${gotDoc}/${gotTot}`,
          pass: gotTot === symSpec.pub_counts.total && gotDoc === symSpec.pub_counts.documented,
        });
      }
      for (const tc of (symSpec.type_checks || [])) {
        const tname = tc.type;
        let ok = true;
        let desc = "";
        if (tc.returns_include) {
          const retFns = new Set(
            MODULES.flatMap(m => (m.symbols || []).filter(s => s.kind === "fn" && (s.returns || []).includes(tname)).map(s => s.name))
          );
          ok = tc.returns_include.every(fn => retFns.has(fn));
          desc = `returns include ${tc.returns_include.join(", ")}`;
        } else if (tc.method_count !== undefined) {
          const mcnt = MODULES.reduce((a, m) => a + (m.symbols || []).filter(s => s.kind === "fn" && s.impl === tname).length, 0);
          ok = mcnt === tc.method_count;
          desc = `methods=${mcnt} (expect ${tc.method_count})`;
        } else if (tc.min_returning_fns !== undefined) {
          const rcnt = MODULES.reduce((a, m) => a + (m.symbols || []).filter(s => s.kind === "fn" && (s.returns || []).includes(tname)).length, 0);
          ok = rcnt >= tc.min_returning_fns;
          desc = `returning_fns=${rcnt} (>= ${tc.min_returning_fns})`;
        }
        results.push({
          cat: "symbols",
          name: `type_check(${tname})`,
          expected: desc,
          actual: ok ? "OK" : "FAIL",
          pass: ok,
          action: () => { switchTab("symbols"); document.getElementById("type-input").value = tname; renderTypeExplorer(); },
        });
      }
      // 12. callers (3)
      for (const c of (SELFTEST.callers || [])) {
        const target = c.target;
        const prodMods = Array.from(new Set(
          MODULES.filter(m => (m.symbols || []).some(s => s.kind === "fn" && (s.calls || []).includes(target))).map(m => m.name)
        )).sort();
        const testMods = Array.from(new Set(
          MODULES.filter(m => (m.test_fns || []).some(tf => (tf.calls || []).includes(target))).map(m => m.name)
        )).sort();
        const prodCallers = Array.from(new Set(
          MODULES.flatMap(m => (m.symbols || []).filter(s => s.kind === "fn" && (s.calls || []).includes(target)).map(s => s.impl ? `${m.name}::${s.impl}::${s.name}` : `${m.name}::${s.name}`))
        )).sort();
        let ok = true;
        if (c.prod_modules && JSON.stringify(prodMods) !== JSON.stringify(c.prod_modules)) ok = false;
        if (c.test_modules && JSON.stringify(testMods) !== JSON.stringify(c.test_modules)) ok = false;
        if (c.prod_callers && JSON.stringify(prodCallers) !== JSON.stringify(c.prod_callers)) ok = false;
        results.push({
          cat: "callers",
          name: `callers ${target}`,
          expected: JSON.stringify(c.prod_modules || c.prod_callers),
          actual: JSON.stringify(c.prod_modules ? prodMods : prodCallers),
          pass: ok,
          action: () => { switchTab("symbols"); document.getElementById("caller-input").value = target; renderCallerExplorer(); },
        });
      }
      // 13. impact (5)
      for (const c of (SELFTEST.impact || [])) {
        if (c.module) {
          const imp = computeImpact([c.module]);
          const proofSet = new Set([...imp.proofs, ...imp.proofs.map(p => p.replace(/\.lean$/i, "").split("/").pop().toLowerCase())]);
          let ok = true;
          if (c.direct_dependents_include && !c.direct_dependents_include.every(d => imp.direct_prod.includes(d))) ok = false;
          if (c.proofs_include && !c.proofs_include.every(p => proofSet.has(p))) ok = false;
          if (c.mutation_sites !== undefined && (MOD_BY_NAME.get(c.module)?.mutation_sites || 0) !== c.mutation_sites) ok = false;
          results.push({
            cat: "impact",
            name: `impact ${c.module}`,
            expected: JSON.stringify(c.direct_dependents_include || c.proofs_include || c.mutation_sites),
            actual: ok ? "verified" : "mismatch",
            pass: ok,
            action: () => { switchTab("impact"); document.getElementById("impact-input").value = c.module; renderImpact(); },
          });
        } else if (c.total_mutation_sites !== undefined) {
          const tot = MODULES.reduce((a, m) => a + (m.mutation_sites || 0), 0);
          const mcnt = MODULES.filter(m => (m.mutation_sites || 0) > 0).length;
          let ok = tot === c.total_mutation_sites && mcnt === c.modules_with_mutation_sites;
          for (const [mname, expS] of Object.entries(c.module_sites || {})) {
            if ((MOD_BY_NAME.get(mname)?.mutation_sites || 0) !== expS) ok = false;
          }
          results.push({
            cat: "impact",
            name: "impact.total_mutation_sites",
            expected: `${c.total_mutation_sites} across ${c.modules_with_mutation_sites} mods`,
            actual: `${tot} across ${mcnt} mods`,
            pass: ok,
          });
        }
      }
      // 14. pinned_tags (5)
      for (const c of (SELFTEST.pinned_tags || [])) {
        const hit = resolveTag(c.tag);
        const got = hit ? hit.ids : [];
        results.push({
          cat: "pinned_tags",
          name: `tag ${c.tag.slice(0, 34)}...`,
          expected: JSON.stringify(c.expect),
          actual: JSON.stringify(got),
          pass: JSON.stringify(got) === JSON.stringify(c.expect),
          action: () => inspectItem({ type: "tag", tag: c.tag }),
        });
      }
      return results;
    }

    let cachedSelftests = [];
    function renderAuditTab() {
      // 1. Gates card
      const gates = COVERAGE.gates || {};
      document.getElementById("audit-gates-card").innerHTML = `
        <h3 class="text-sm font-bold m-0 mb-2">Three Accounting Gates (<code>bbindex.py audit</code>)</h3>
        <p class="muted text-xs mb-3">
          Enforced on every index regeneration: module declarations &amp; dependencies resolve, tool schemas match <code>src/</code> or <code>schema_local.json</code>, and all 1,112 metric families match <code>METRICS.md</code> and <code>registry.tsv</code>.
        </p>
        <div class="space-y-2 text-xs">
          ${Object.entries(gates).map(([g, ok]) => `
            <div class="surface-subtle p-2.5 flex items-center justify-between">
              <span class="mono font-semibold">${esc(g)}</span>
              <span class="pill ${ok ? "pill-high" : "pill-danger"} mono">${ok ? "PASS" : "FAIL"}</span>
            </div>
          `).join("")}
        </div>
      `;

      // 2. Eval benchmark card
      document.getElementById("audit-eval-card").innerHTML = `
        <h3 class="text-sm font-bold m-0 mb-2">Self-Supervised Holdout Retrieval Benchmark (<code>bbgraph.py eval</code>)</h3>
        <p class="muted text-xs mb-3">
          Measures whether <code>rank()</code> recovers the owning module (Strict) or its 1-hop dependency neighborhood (1-Hop) from held-out docstrings and test names.
        </p>
        <div class="space-y-2 text-xs">
          ${EVAL_ROWS.map(r => `
            <div class="surface-subtle p-2">
              <div class="flex items-center justify-between mono">
                <span class="font-semibold">${esc(r.label)} (N=${r.n})</span>
                <span>Strict: <strong>${r.s1_pct}% / ${r.s3_pct}%</strong> &bull; 1-Hop: <strong style="color:#10b981;">${r.h1_pct}% / ${r.h3_pct}%</strong></span>
              </div>
              <div class="w-full h-2 bg-black/20 rounded overflow-hidden mt-1.5 flex">
                <div style="width:${r.s3_pct}%;background:#3b82f6;" title="Strict Top-3: ${r.s3_pct}%"></div>
                <div style="width:${Math.max(0, r.h3_pct - r.s3_pct)}%;background:#10b981;" title="1-Hop Top-3: ${r.h3_pct}%"></div>
              </div>
            </div>
          `).join("")}
        </div>
      `;

      // 3. Selftests
      cachedSelftests = runBrowserSelftests();
      const passCnt = cachedSelftests.filter(r => r.pass).length;
      const badge = document.getElementById("header-selftest-badge");
      badge.textContent = `SELFTEST ${passCnt}/${cachedSelftests.length} PASS`;
      badge.className = `pill ${passCnt === cachedSelftests.length ? "pill-high" : "pill-danger"} mono`;

      const catSel = document.getElementById("selftest-cat-filter");
      if (catSel.options.length <= 1) {
        const cats = {};
        for (const r of cachedSelftests) cats[r.cat] = (cats[r.cat] || 0) + 1;
        for (const [c, cnt] of Object.entries(cats)) {
          const opt = document.createElement("option");
          opt.value = c;
          opt.textContent = `${c} (${cnt})`;
          catSel.appendChild(opt);
        }
      }
      renderSelftestTable();

      // 4. Coverage table
      const sortedMods = MODULES.slice().sort((a, b) => {
        const aZero = (a.pub_symbol_count || 0) === 0 ? 1 : 0;
        const bZero = (b.pub_symbol_count || 0) === 0 ? 1 : 0;
        if (aZero !== bZero) return aZero - bZero;
        const aPct = (a.pub_documented || 0) / Math.max(a.pub_symbol_count || 1, 1);
        const bPct = (b.pub_documented || 0) / Math.max(b.pub_symbol_count || 1, 1);
        return (aPct - bPct) || a.name.localeCompare(b.name);
      });
      const covWrap = document.getElementById("coverage-table-wrap");
      covWrap.innerHTML = `
        <table class="audit-table">
          <thead>
            <tr>
              <th>Module</th>
              <th>Crate / Gate</th>
              <th>Public Symbol /// Coverage</th>
              <th>Mutation Sites (.to_be_bytes)</th>
              <th>Tests</th>
              <th>LOC</th>
            </tr>
          </thead>
          <tbody>
            ${sortedMods.map(m => {
              const pc = m.pub_symbol_count || 0;
              const pd = m.pub_documented || 0;
              const pct = pc > 0 ? Math.round((100 * pd) / pc) : null;
              return `
                <tr class="clickable-row" data-inspect-id="module:${esc(m.name)}">
                  <td class="mono font-bold">${esc(m.name)}</td>
                  <td><span class="pill ${gatePillClass(m.gate, m.crate)} mono">cfg(${esc(m.gate)})</span></td>
                  <td class="mono">${pct !== null ? `${pd} / ${pc} (${pct}%)` : "0 / 0 (n/a)"}</td>
                  <td class="mono">${m.mutation_sites || 0}</td>
                  <td class="mono">${m.tests}</td>
                  <td class="mono muted">${m.lines.toLocaleString()}</td>
                </tr>
              `;
            }).join("")}
          </tbody>
        </table>
      `;
      bindInspectClicks(covWrap);

      // 5. Duplication & Dead surface
      const prodDup = REUSE.duplication.production;
      const testDup = REUSE.duplication.test;
      document.getElementById("audit-reuse-card").innerHTML = `
        <div class="grid grid-cols-1 lg:grid-cols-2 gap-4">
          <div>
            <h3 class="text-sm font-bold m-0 mb-2">Repeated Regions &amp; Legal DAG Helper Routing (<code>bbgraph.py dup</code>)</h3>
            <div class="muted text-xs mb-2">
              Production: ${prodDup.duplicated_lines} / ${prodDup.code_lines} lines (${((100 * prodDup.duplicated_lines) / prodDup.code_lines).toFixed(1)}%) &bull;
              Test scope: ${testDup.duplicated_lines} / ${testDup.code_lines} lines (${((100 * testDup.duplicated_lines) / testDup.code_lines).toFixed(1)}%)
            </div>
            <div class="space-y-1.5 text-xs mono max-h-56 overflow-y-auto">
              ${(prodDup.regions || []).slice(0, 10).map(r => `
                <div class="surface-subtle p-2">
                  <div><strong>${r.lines} lines:</strong> ${esc(r.left)} &harr; ${esc(r.right)}</div>
                  <div class="muted">DAG legal home: ${esc((r.homes && r.homes.length) ? r.homes.join(", ") : "none shared")}</div>
                </div>
              `).join("")}
            </div>
          </div>
          <div>
            <h3 class="text-sm font-bold m-0 mb-2">Public Dead-Surface Audit (<code>bbgraph.py dead</code>)</h3>
            <div class="muted text-xs mb-2">
              Unreferenced across workspace (${REUSE.dead.unreferenced.length}) &amp; Module-private candidates (${REUSE.dead.module_private_candidates.length})
            </div>
            <div class="space-y-1.5 text-xs mono max-h-56 overflow-y-auto">
              ${REUSE.dead.unreferenced.map(u => `
                <div class="surface-subtle p-2">
                  <span class="pill pill-low mr-1">unreferenced</span>
                  <strong>${esc(u.item)}</strong> in ${esc(u.modules.join(", "))}
                </div>
              `).join("")}
              ${REUSE.dead.module_private_candidates.slice(0, 12).map(u => `
                <div class="surface-subtle p-2">
                  <span class="pill mr-1">module-only</span>
                  <strong>${esc(u.item)}</strong> in ${esc(u.modules.join(", "))}
                </div>
              `).join("")}
            </div>
          </div>
        </div>
      `;
    }

    function renderSelftestTable() {
      const cat = document.getElementById("selftest-cat-filter").value;
      const rows = cachedSelftests.filter(r => cat === "all" || r.cat === cat);
      const wrap = document.getElementById("selftest-table-wrap");
      wrap.innerHTML = `
        <table class="audit-table">
          <thead>
            <tr>
              <th>#</th>
              <th>Category</th>
              <th>Check</th>
              <th>Expected</th>
              <th>Actual</th>
              <th>Status</th>
            </tr>
          </thead>
          <tbody>
            ${rows.map((r, i) => `
              <tr class="clickable-row" data-st-idx="${i}">
                <td class="mono muted">${i + 1}</td>
                <td><span class="pill mono">${esc(r.cat)}</span></td>
                <td class="mono font-semibold">${esc(r.name)}</td>
                <td class="mono text-xs muted">${esc(r.expected)}</td>
                <td class="mono text-xs">${esc(r.actual)}</td>
                <td><span class="pill ${r.pass ? "pill-high" : "pill-danger"} mono">${r.pass ? "PASS" : "FAIL"}</span></td>
              </tr>
            `).join("")}
          </tbody>
        </table>
      `;
      wrap.querySelectorAll("[data-st-idx]").forEach(tr => {
        tr.addEventListener("click", () => {
          const item = rows[Number(tr.getAttribute("data-st-idx"))];
          if (item && item.action) item.action();
        });
      });
    }

    document.getElementById("selftest-cat-filter").addEventListener("change", renderSelftestTable);
    document.getElementById("selftest-rerun-btn").addEventListener("click", renderAuditTab);

    // Initial render of all views
    renderKpiRibbon();
    renderAsk();
    renderSubsystems();
    renderImpact();
    renderTypeExplorer();
    renderCallerExplorer();
    renderSymCatalog();
    renderProtoTab();
    renderAuditTab();
  })();
  </script>
</body>
</html>
"""


def cmd_html(out_arg: str) -> int:
    out_path = (
        Path(out_arg.strip())
        if out_arg.strip()
        else ROOT / "target" / "bitblock_graph.html"
    )
    if not out_path.is_absolute():
        out_path = ROOT / out_path
    payload = build_html_payload()
    raw_json = json.dumps(payload, separators=(",", ":"), sort_keys=True).replace(
        "</", "<\\/"
    )
    html_text = HTML_TEMPLATE.replace("__BB_GRAPH_PAYLOAD__", raw_json)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(html_text, encoding="utf-8", newline="\n")
    tot_syms = sum(
        len(n.get("symbols", ())) for n in payload["nodes"] if n["kind"] == "module"
    )
    print(
        "HTML COMPILER  %s  (%.1f KB)"
        % (out_path, len(html_text.encode("utf-8")) / 1024.0)
    )
    print(
        "  embedded: %d nodes, %d edges, %d DAG strata, %d subsystems, %d symbols, %d tests"
        % (
            len(payload["nodes"]),
            len(payload["edges"]),
            max(payload["dag"]["stratum"].values()) + 1,
            len(payload["subs"]),
            tot_syms,
            payload["coverage"]["tests"],
        )
    )
    return 0


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
    if cmd == "wave":
        return cmd_wave(joined)
    if cmd == "dup":
        return cmd_dup(joined)
    if cmd == "dead":
        return cmd_dead()
    if cmd == "sub":
        return cmd_sub(" ".join(r for r in rest if not r.startswith("--")))
    if cmd == "gates":
        return cmd_gates()
    if cmd == "stats":
        return cmd_stats()
    if cmd == "selftest":
        return cmd_selftest()
    if cmd == "sym":
        return cmd_sym(joined)
    if cmd == "type":
        return cmd_type(joined)
    if cmd == "returns":
        return cmd_returns(joined)
    if cmd == "takes":
        return cmd_takes(joined)
    if cmd == "callers":
        return cmd_callers(joined)
    if cmd == "impact":
        return cmd_impact(joined)
    if cmd == "coverage":
        return cmd_coverage()
    if cmd == "eval":
        return cmd_eval()
    if cmd == "html":
        return cmd_html(joined)
    print(__doc__)
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
