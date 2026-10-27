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

    python tools/bbgraph.py ask "canonical framebuffer bytes"
    python tools/bbgraph.py schema bitblock/tick-outcome/v3
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


def cmd_eval() -> int:
    nodes, subs, dag = load()
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

    def fmt_row(label: str, n: int, s1: int, s3: int, h1: int, h3: int) -> str:
        denom = max(n, 1)
        return (
            "  %-28s %4d   %5.1f%% / %5.1f%%   %5.1f%% / %5.1f%%"
            % (
                label,
                n,
                100.0 * s1 / denom,
                100.0 * s3 / denom,
                100.0 * h1 / denom,
                100.0 * h3 / denom,
            )
        )

    print("SELF-SUPERVISED HOLDOUT RETRIEVAL BENCHMARK (37 modules)")
    print("  %-28s %4s   %-15s   %-15s" % ("holdout set", "N", "strict T1 / T3", "1-hop T1 / T3"))
    print(fmt_row("module-doc (//! summary)", md_n, md_s1, md_s3, md_h1, md_h3))
    print(fmt_row("symbol-doc (pub /// >=3t)", sd_n, sd_s1, sd_s3, sd_h1, sd_h3))
    print(fmt_row("test-name (all #[test])", ta_n, ta_s1, ta_s3, ta_h1, ta_h3))
    print(fmt_row("test-name (lib, no main)", tl_n, tl_s1, tl_s3, tl_h1, tl_h3))
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
    print(__doc__)
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
