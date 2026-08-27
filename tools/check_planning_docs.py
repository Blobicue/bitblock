#!/usr/bin/env python3
"""Validate Bitblock planning documents without changing repository state."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import unquote


ROOT = Path(__file__).resolve().parents[1]
CAPABILITY = ROOT / "docs" / "CAPABILITY_TREE.md"
FIRST_AUDIT = ROOT / "docs" / "FUTURE_PLAN_AUDIT.md"
FIRST_AUDIT_GIT_BLOB = "8a02f4b7b00b440e14a35a098b87a7493fd887b4"
SECOND_AUDIT = ROOT / "docs" / "FUTURE_PLAN_AUDIT_LOOP2.md"
SECOND_AUDIT_GIT_BLOB = "c4be9e0dc7041bfe79b3e8561308b6c2c4767ce6"
DESIGN_DISTILLATION = ROOT / "docs" / "DESIGN_RESEARCH_DISTILLATION.md"
DESIGN_AUDIT = ROOT / "docs" / "DESIGN_RESEARCH_DISTILLATION_AUDIT.md"
DESIGN_AUDIT_GIT_BLOB = "73d53bc6f3ca007b5faca8c499660b207ff048e2"
SUCCESSOR_AUDIT = ROOT / "docs" / "DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP2.md"
SUCCESSOR_AUDIT_GIT_BLOB = "e8bd912f171d64c345b6fa71413bd9503e12f5f3"
THIRD_AUDIT = ROOT / "docs" / "DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP3.md"
THIRD_AUDIT_GIT_BLOB = "fbee935d27617f47f913617ca2d8e19aba618c6e"
DESIGN_RECEIPT = ROOT / "docs" / "design-source-receipt-v1.json"
DESIGN_RECEIPT_GIT_BLOB = "c3005e7fc98971a7747ab125f1624d08aff31199"
DESIGN_RECEIPT_CANONICAL_SHA256 = "38731565d149bedfc9a9e7570db0ca0a1478e4e7aa3cdeef0aa1a4e5a5888acb"
DESIGN_RECEIPT_AUDITED_COMMIT = "694650f4028f5c1fb715384597326e0b0e31f0e1"
FROZEN_TEXT_FILES = (
    FIRST_AUDIT,
    SECOND_AUDIT,
    DESIGN_AUDIT,
    SUCCESSOR_AUDIT,
    THIRD_AUDIT,
    DESIGN_RECEIPT,
)
EXPECTED_FROZEN_TEXT_PATHS = frozenset(
    {
        "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT.md",
        "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP2.md",
        "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP3.md",
        "docs/FUTURE_PLAN_AUDIT.md",
        "docs/FUTURE_PLAN_AUDIT_LOOP2.md",
        "docs/design-source-receipt-v1.json",
    }
)
PLANNING_FILES = (
    ROOT / "README.md",
    ROOT / "docs" / "PROJECT_INTENT.md",
    DESIGN_DISTILLATION,
    DESIGN_AUDIT,
    SUCCESSOR_AUDIT,
    THIRD_AUDIT,
    CAPABILITY,
    ROOT / "docs" / "AUTHENTICATED_STATE_PLAN.md",
    ROOT / "docs" / "FUTURE_PLAN_AUDIT.md",
    SECOND_AUDIT,
    ROOT / "docs" / "SEMANTICS.md",
    ROOT / "docs" / "TRANSCRIPT_AUDIT.md",
    ROOT / "docs" / "HANDOFF.md",
    ROOT / "docs" / "LAYER1_DESIGN_PAPER.md",
    ROOT / "docs" / "LAYER1_DESIGN_AUDIT.md",
    ROOT / "docs" / "PROTOCOL.md",
    ROOT / "docs" / "TIMELINES.md",
)
EXPECTED_PLANNING_PATHS = frozenset(
    {
        "README.md",
        "docs/AUTHENTICATED_STATE_PLAN.md",
        "docs/CAPABILITY_TREE.md",
        "docs/DESIGN_RESEARCH_DISTILLATION.md",
        "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT.md",
        "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP2.md",
        "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP3.md",
        "docs/FUTURE_PLAN_AUDIT.md",
        "docs/FUTURE_PLAN_AUDIT_LOOP2.md",
        "docs/HANDOFF.md",
        "docs/LAYER1_DESIGN_AUDIT.md",
        "docs/LAYER1_DESIGN_PAPER.md",
        "docs/PROJECT_INTENT.md",
        "docs/PROTOCOL.md",
        "docs/SEMANTICS.md",
        "docs/TIMELINES.md",
        "docs/TRANSCRIPT_AUDIT.md",
    }
)
EXPECTED_PLANNING_INVENTORY_DIGEST = (
    "1e4ea0376a774f4400f0905ccb10b608c8d499ae75df2ebaf7af29ce6eb4bbfe"
)
NODE_ID = r"[A-Z][A-Z0-9-]*"
DETAIL_HEADING = re.compile(rf"^### `({NODE_ID})`(?:\s|$)")
FIELD_BOUNDARY = re.compile(r"^#{1,3}\s")
FIELD = re.compile(r"^- \*\*(Status|Depends on|Motive|Artifact|Gate|Non-claim):\*\*\s*(.*)$")
DEPENDENCY = re.compile(rf"`({NODE_ID})`")
FOUNDATION_ROW = re.compile(rf"^\| `({NODE_ID})` \| \*\*([^|]+)\*\* \| ([^|]+) \|")
CONDITIONAL_ROW = re.compile(
    rf"^\| (CE-[0-9]{{2}}) \| ({NODE_ID}) \| ([a-z0-9]+(?:-[a-z0-9]+)*) \| "
    rf"([a-z0-9]+(?:-[a-z0-9]+)*) \| ({NODE_ID}) \| "
    rf"([a-z0-9]+(?:-[a-z0-9]+)*) \|$"
)
PREDICATE_ENTAILMENT_ROW = re.compile(
    r"^\| (PE-[0-9]{2}) \| ([a-z0-9]+(?:-[a-z0-9]+)*) \| "
    r"([a-z0-9]+(?:-[a-z0-9]+)*) \|$"
)
LINK = re.compile(r"\[[^\]]*\]\(([^)]+)\)")
HEADING = re.compile(r"^#{1,6}\s+(.+?)\s*$")

EXPECTED_NODE_IDS = frozenset(
    """
    ACCOUNT-1 ASSET-1 ASSET-D1 AUTH-COMMIT-1 BOUNDS-1 CLIENT-1 CODEC-1 COMMIT-1
    COMMIT-R1 CONSENSUS-1 CONSENSUS-D1 COST-1 CRYPTO-1 CRYPTO-D1 DOC-0
    ECONOMIC-STATE-1 EMERGENCE-R1 EXECUTION-ROOT-1 EXECUTOR-1 GENESIS-D1
    GENESIS-ROOT-PARITY-1 GOVERNANCE-1 GOVERNANCE-D1 HASH-R1 HASH-SOURCE-R1 IDENTITY-1
    IDENTITY-D1 L1-CANDIDATE L1-PRODUCTION LANGUAGE-R1 LEDGER-1 LOCAL-BOUNDS-1 MULTI-1
    OBSERVE-0 PROD-OPS-1 PROFILE-1 PROFILE-D1 PROFILE-SCOPE-D0 PROGRAM-CONSENSUS
    PROGRAM-STATE-1 PROOF-0 PROTOCOL-ID-1 REFINE-1 RELEASE-1 RELEASE-R1 REPLAY-0
    RESOURCE-1 RULE-1 SCALE-1 SCALE-2 SEQUENCER-1 SIGNER-OPS-1 STORAGE-1 SYNC-1
    TESTNET-1 THREAT-1 THREAT-D0 TICK-0 TRANSITION-SPEC-1 TRANSITION-SPEC-D1
    UPGRADE-1 UPGRADE-FRAMEWORK-1 VALIDATOR-ECON-1 VALIDATOR-ECON-D1
    VALIDATOR-SCHEDULE-1 VALUE-1 VERKLE-R1 VM-1 VM-D1 WORLD-0
    """.split()
)
EXPECTED_STATUS_DIGEST = "61f447102b3c6134d74338b05c6a6e1648d48cd2f9254d1e6a822349bab45dc3"
EXPECTED_CONDITIONAL_IDS = frozenset(f"CE-{number:02d}" for number in range(1, 44))
EXPECTED_CONDITIONAL_DIGEST = "b19b9b0e83222f89e7db4da8224f7455148b77baeaf158bb9b2a006e3e2b200c"
EXPECTED_PREDICATE_ENTAILMENT_IDS = frozenset(f"PE-{number:02d}" for number in range(1, 12))
EXPECTED_PREDICATE_ENTAILMENT_DIGEST = "68d76604c6370932c8dcaab229ed1ed495922d0eac3e7ef741abe80b17534e85"


class PlanningError(ValueError):
    pass


@dataclass(frozen=True)
class Node:
    node_id: str
    dependencies: tuple[str, ...]
    status_class: str
    source: str


@dataclass(frozen=True)
class ConditionalEdge:
    edge_id: str
    owner_base: str
    owner_role: str
    predicate: str
    dependency_base: str
    dependency_role: str


@dataclass(frozen=True)
class PredicateEntailment:
    entailment_id: str
    antecedent: str
    consequent: str


STATUS_CLASSES = (
    "IMPLEMENTED",
    "BOUNDED",
    "READY",
    "LOCKED",
    "OPEN DECISION",
    "NOT STARTED",
)


def _status_class(text: str, node_id: str) -> str:
    plain = re.sub(r"[*`]", "", text).strip().upper()
    for status in STATUS_CLASSES:
        if plain.startswith(status):
            return status
    raise PlanningError(f"{node_id}: invalid primary status vocabulary: {text}")


def _assert_acyclic(adjacency: dict[str, tuple[str, ...]], label: str) -> None:
    state: dict[str, int] = {}
    stack: list[str] = []

    def visit(node_id: str) -> None:
        state[node_id] = 1
        stack.append(node_id)
        for dependency in adjacency.get(node_id, ()):
            if dependency not in adjacency:
                raise PlanningError(f"{label}: missing graph node {dependency}")
            if state.get(dependency) == 1:
                begin = stack.index(dependency)
                cycle = stack[begin:] + [dependency]
                raise PlanningError(f"{label} cycle: " + " -> ".join(cycle))
            if state.get(dependency, 0) == 0:
                visit(dependency)
        stack.pop()
        state[node_id] = 2

    for node_id in adjacency:
        if state.get(node_id, 0) == 0:
            visit(node_id)


def _field_blocks(lines: list[str], start: int, end: int, node_id: str) -> dict[str, str]:
    fields: dict[str, list[str]] = {}
    active: str | None = None
    for line in lines[start:end]:
        match = FIELD.match(line)
        if match:
            active = match.group(1)
            if active in fields:
                raise PlanningError(f"{node_id}: duplicate {active} field")
            fields[active] = [match.group(2).strip()]
        elif active is not None:
            fields[active].append(line.strip())
    required = {"Status", "Depends on", "Motive", "Artifact", "Gate", "Non-claim"}
    missing = sorted(required - fields.keys())
    if missing:
        raise PlanningError(f"{node_id}: missing fields: {', '.join(missing)}")
    joined = {name: " ".join(parts).strip() for name, parts in fields.items()}
    empty = sorted(name for name, value in joined.items() if not value)
    if empty:
        raise PlanningError(f"{node_id}: empty fields: {', '.join(empty)}")
    return joined


def parse_capability(text: str) -> dict[str, Node]:
    lines = text.splitlines()
    nodes: dict[str, Node] = {}

    for number, line in enumerate(lines, 1):
        match = FOUNDATION_ROW.match(line)
        if not match:
            continue
        node_id, _status, prerequisites = match.groups()
        if node_id in nodes:
            raise PlanningError(f"duplicate node {node_id} at line {number}")
        nodes[node_id] = Node(
            node_id,
            tuple(DEPENDENCY.findall(prerequisites)),
            _status_class(_status, node_id),
            f"line {number}",
        )

    headings: list[tuple[int, str]] = []
    for index, line in enumerate(lines):
        match = DETAIL_HEADING.match(line)
        if match:
            headings.append((index, match.group(1)))

    for position, (start, node_id) in enumerate(headings):
        if node_id in nodes:
            raise PlanningError(f"duplicate node {node_id} at line {start + 1}")
        end = next(
            (index for index in range(start + 1, len(lines)) if FIELD_BOUNDARY.match(lines[index])),
            len(lines),
        )
        fields = _field_blocks(lines, start + 1, end, node_id)
        dependency_text = fields["Depends on"]
        dependencies = tuple(DEPENDENCY.findall(dependency_text))
        if not dependencies and "none" not in dependency_text.lower():
            raise PlanningError(f"{node_id}: dependency field names no node and does not say None")
        nodes[node_id] = Node(
            node_id,
            dependencies,
            _status_class(fields["Status"], node_id),
            f"line {start + 1}",
        )

    if not nodes:
        raise PlanningError("no capability nodes parsed")

    actual_ids = frozenset(nodes)
    if actual_ids != EXPECTED_NODE_IDS:
        missing = sorted(EXPECTED_NODE_IDS - actual_ids)
        extra = sorted(actual_ids - EXPECTED_NODE_IDS)
        raise PlanningError(
            "capability inventory changed without updating its intentional inventory guard: "
            f"missing={missing}, extra={extra}"
        )

    status_manifest = "\n".join(
        f"{node_id}={nodes[node_id].status_class}" for node_id in sorted(nodes)
    )
    status_digest = hashlib.sha256(status_manifest.encode("utf-8")).hexdigest()
    if status_digest != EXPECTED_STATUS_DIGEST:
        raise PlanningError(
            "primary status classes changed without updating the intentional status guard: "
            f"expected {EXPECTED_STATUS_DIGEST}, got {status_digest}"
        )

    for node in nodes.values():
        unknown = sorted(set(node.dependencies) - nodes.keys())
        if unknown:
            raise PlanningError(f"{node.node_id}: undefined dependencies: {', '.join(unknown)}")

    _assert_acyclic(
        {node_id: node.dependencies for node_id, node in nodes.items()},
        "base dependency",
    )
    return nodes


def parse_conditional_edges(
    text: str,
    nodes: dict[str, Node],
    *,
    enforce_manifest: bool = True,
) -> tuple[ConditionalEdge, ...]:
    edges: list[ConditionalEdge] = []
    seen: set[str] = set()
    for number, line in enumerate(text.splitlines(), 1):
        if not line.startswith("| CE-"):
            continue
        match = CONDITIONAL_ROW.match(line)
        if not match:
            raise PlanningError(f"conditional edge row {number} is not canonical: {line}")
        edge = ConditionalEdge(*match.groups())
        if edge.edge_id in seen:
            raise PlanningError(f"duplicate conditional edge {edge.edge_id} at line {number}")
        seen.add(edge.edge_id)
        unknown = sorted(
            {edge.owner_base, edge.dependency_base} - nodes.keys()
        )
        if unknown:
            raise PlanningError(
                f"{edge.edge_id}: undefined conditional bases: {', '.join(unknown)}"
            )
        edges.append(edge)

    if not edges:
        raise PlanningError("no structured conditional-instance edges parsed")

    canonical = "\n".join(
        "|".join(
            (
                edge.edge_id,
                edge.owner_base,
                edge.owner_role,
                edge.predicate,
                edge.dependency_base,
                edge.dependency_role,
            )
        )
        for edge in edges
    )
    if enforce_manifest:
        actual_ids = frozenset(seen)
        if actual_ids != EXPECTED_CONDITIONAL_IDS:
            missing = sorted(EXPECTED_CONDITIONAL_IDS - actual_ids)
            extra = sorted(actual_ids - EXPECTED_CONDITIONAL_IDS)
            raise PlanningError(
                "conditional edge inventory changed without updating its intentional guard: "
                f"missing={missing}, extra={extra}"
            )
        digest = hashlib.sha256(canonical.encode("utf-8")).hexdigest()
        if digest != EXPECTED_CONDITIONAL_DIGEST:
            raise PlanningError(
                "conditional roles, predicates, or polarity changed without updating their guard: "
                f"expected {EXPECTED_CONDITIONAL_DIGEST}, got {digest}"
            )

    roles: dict[str, set[str]] = {node_id: {"base"} for node_id in nodes}
    for edge in edges:
        roles[edge.owner_base].add(edge.owner_role)
        roles[edge.dependency_base].add(edge.dependency_role)

    adjacency: dict[str, list[str]] = {}
    for node_id, node_roles in roles.items():
        for role in node_roles:
            adjacency[f"{node_id}@{role}"] = [
                f"{dependency}@base" for dependency in nodes[node_id].dependencies
            ]
    for edge in edges:
        adjacency[f"{edge.owner_base}@{edge.owner_role}"].append(
            f"{edge.dependency_base}@{edge.dependency_role}"
        )

    _assert_acyclic(
        {node_id: tuple(dependencies) for node_id, dependencies in adjacency.items()},
        "role-aware conditional dependency",
    )
    return tuple(edges)


def parse_predicate_entailments(
    text: str,
    conditional_edges: tuple[ConditionalEdge, ...],
    *,
    enforce_manifest: bool = True,
) -> tuple[PredicateEntailment, ...]:
    entailments: list[PredicateEntailment] = []
    seen_ids: set[str] = set()
    seen_pairs: set[tuple[str, str]] = set()
    known_predicates = {edge.predicate for edge in conditional_edges}
    for number, line in enumerate(text.splitlines(), 1):
        if not line.startswith("| PE-"):
            continue
        match = PREDICATE_ENTAILMENT_ROW.match(line)
        if not match:
            raise PlanningError(f"predicate entailment row {number} is not canonical: {line}")
        entailment = PredicateEntailment(*match.groups())
        if entailment.entailment_id in seen_ids:
            raise PlanningError(
                f"duplicate predicate entailment {entailment.entailment_id} at line {number}"
            )
        pair = (entailment.antecedent, entailment.consequent)
        if pair in seen_pairs:
            raise PlanningError(
                f"duplicate predicate entailment pair {pair[0]} -> {pair[1]} at line {number}"
            )
        if entailment.antecedent == entailment.consequent:
            raise PlanningError(
                f"{entailment.entailment_id}: predicate cannot entail itself"
            )
        unknown = sorted(set(pair) - known_predicates)
        if unknown:
            raise PlanningError(
                f"{entailment.entailment_id}: undefined predicates: {', '.join(unknown)}"
            )
        seen_ids.add(entailment.entailment_id)
        seen_pairs.add(pair)
        entailments.append(entailment)

    if not entailments:
        raise PlanningError("no structured predicate entailments parsed")

    canonical = "\n".join(
        "|".join(
            (
                entailment.entailment_id,
                entailment.antecedent,
                entailment.consequent,
            )
        )
        for entailment in entailments
    )
    if enforce_manifest:
        actual_ids = frozenset(seen_ids)
        if actual_ids != EXPECTED_PREDICATE_ENTAILMENT_IDS:
            missing = sorted(EXPECTED_PREDICATE_ENTAILMENT_IDS - actual_ids)
            extra = sorted(actual_ids - EXPECTED_PREDICATE_ENTAILMENT_IDS)
            raise PlanningError(
                "predicate entailment inventory changed without updating its intentional guard: "
                f"missing={missing}, extra={extra}"
            )
        digest = hashlib.sha256(canonical.encode("utf-8")).hexdigest()
        if digest != EXPECTED_PREDICATE_ENTAILMENT_DIGEST:
            raise PlanningError(
                "predicate entailments changed without updating their guard: "
                f"expected {EXPECTED_PREDICATE_ENTAILMENT_DIGEST}, got {digest}"
            )

    adjacency: dict[str, list[str]] = {
        predicate: [] for predicate in known_predicates
    }
    for entailment in entailments:
        adjacency[entailment.antecedent].append(entailment.consequent)
    _assert_acyclic(
        {predicate: tuple(consequents) for predicate, consequents in adjacency.items()},
        "predicate entailment",
    )
    return tuple(entailments)


def expand_predicates(
    seeds: set[str],
    entailments: tuple[PredicateEntailment, ...],
) -> frozenset[str]:
    expanded = set(seeds)
    changed = True
    while changed:
        changed = False
        for entailment in entailments:
            if entailment.antecedent in expanded and entailment.consequent not in expanded:
                expanded.add(entailment.consequent)
                changed = True
    return frozenset(expanded)


def heading_slug(text: str) -> str:
    text = re.sub(r"!?\[([^\]]*)\]\([^)]*\)", r"\1", text)
    text = re.sub(r"<[^>]*>", "", text)
    text = re.sub(r"\s+#+\s*$", "", text)
    text = text.replace("`", "").replace("*", "").lower()
    text = "".join(character for character in text if character.isalnum() or character in " _-")
    return re.sub(r"\s+", "-", text.strip())


def heading_anchors(text: str) -> set[str]:
    anchors: set[str] = set()
    counts: dict[str, int] = {}
    inside_fence = False
    for line in text.splitlines():
        if line.startswith("```"):
            inside_fence = not inside_fence
            continue
        if inside_fence:
            continue
        match = HEADING.match(line)
        if not match:
            continue
        base = heading_slug(match.group(1))
        if not base:
            continue
        duplicate = counts.get(base, 0)
        counts[base] = duplicate + 1
        anchors.add(base if duplicate == 0 else f"{base}-{duplicate}")
    return anchors


def repository_relative_path(path: Path) -> str:
    try:
        return path.resolve().relative_to(ROOT.resolve()).as_posix()
    except ValueError as error:
        raise PlanningError(f"path escapes repository: {path}") from error


def check_planning_inventory(paths: tuple[Path, ...]) -> None:
    normalized = tuple(repository_relative_path(path) for path in paths)
    duplicate_paths = sorted(
        path for path in set(normalized) if normalized.count(path) > 1
    )
    if duplicate_paths:
        raise PlanningError(
            "planning authority inventory contains duplicate paths: "
            + ", ".join(duplicate_paths)
        )
    actual_paths = frozenset(normalized)
    if actual_paths != EXPECTED_PLANNING_PATHS:
        missing = sorted(EXPECTED_PLANNING_PATHS - actual_paths)
        extra = sorted(actual_paths - EXPECTED_PLANNING_PATHS)
        raise PlanningError(
            "planning authority inventory changed without updating its intentional guard: "
            f"missing={missing}, extra={extra}"
        )
    manifest = "\n".join(sorted(actual_paths))
    digest = hashlib.sha256(manifest.encode("utf-8")).hexdigest()
    if digest != EXPECTED_PLANNING_INVENTORY_DIGEST:
        raise PlanningError(
            "planning authority inventory digest changed: "
            f"expected {EXPECTED_PLANNING_INVENTORY_DIGEST}, got {digest}"
        )


def git_effective_eol_policy(paths: tuple[Path, ...]) -> dict[str, dict[str, str]]:
    relative_paths = tuple(repository_relative_path(path) for path in paths)
    try:
        result = subprocess.run(
            ["git", "check-attr", "-z", "text", "eol", "--", *relative_paths],
            cwd=ROOT,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except (OSError, subprocess.CalledProcessError) as error:
        detail = getattr(error, "stderr", b"").decode("utf-8", errors="replace").strip()
        raise PlanningError(
            f"cannot resolve frozen-text Git EOL policy: {detail or error}"
        ) from error
    fields = result.stdout.split(b"\0")
    if fields and fields[-1] == b"":
        fields.pop()
    if len(fields) % 3:
        raise PlanningError("git check-attr returned a malformed NUL-delimited response")
    policy: dict[str, dict[str, str]] = {}
    for index in range(0, len(fields), 3):
        path = fields[index].decode("utf-8")
        attribute = fields[index + 1].decode("utf-8")
        value = fields[index + 2].decode("utf-8")
        policy.setdefault(path, {})[attribute] = value
    return policy


def check_frozen_text_eol_policy(policy: dict[str, dict[str, str]]) -> None:
    expected_paths = EXPECTED_FROZEN_TEXT_PATHS
    actual_paths = frozenset(policy)
    if actual_paths != expected_paths:
        missing = sorted(expected_paths - actual_paths)
        extra = sorted(actual_paths - expected_paths)
        raise PlanningError(
            f"frozen-text EOL policy path mismatch: missing={missing}, extra={extra}"
        )
    for path in sorted(expected_paths):
        attributes = policy[path]
        if attributes.get("text") != "set" or attributes.get("eol") != "lf":
            raise PlanningError(
                f"{path}: frozen text must have effective Git attributes text=set and eol=lf; "
                f"got text={attributes.get('text')}, eol={attributes.get('eol')}"
            )


def git_tracked_or_staged_paths() -> frozenset[Path]:
    try:
        result = subprocess.run(
            ["git", "ls-files", "--cached", "-z"],
            cwd=ROOT,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except (OSError, subprocess.CalledProcessError) as error:
        detail = getattr(error, "stderr", b"").decode("utf-8", errors="replace").strip()
        raise PlanningError(f"cannot enumerate Git tracked/staged paths: {detail or error}") from error
    return frozenset(
        (ROOT / entry.decode("utf-8")).resolve()
        for entry in result.stdout.split(b"\0")
        if entry
    )


def is_git_tracked_or_staged_target(
    resolved: Path,
    tracked_paths: frozenset[Path],
) -> bool:
    if resolved in tracked_paths:
        return True
    if not resolved.is_dir():
        return False
    return any(
        candidate != resolved and resolved in candidate.parents
        for candidate in tracked_paths
    )


def check_markdown(
    path: Path,
    text: str,
    *,
    tracked_paths: frozenset[Path] | None = None,
) -> None:
    if tracked_paths is None:
        tracked_paths = git_tracked_or_staged_paths()
    lines = text.splitlines()
    fence_count = sum(1 for line in lines if line.startswith("```"))
    if fence_count % 2:
        raise PlanningError(f"{path.relative_to(ROOT)}: unbalanced Markdown fences")

    headings: set[str] = set()
    for number, line in enumerate(text.splitlines(), 1):
        if not line.startswith("#"):
            continue
        heading = line.strip()
        if heading in headings:
            raise PlanningError(f"{path.relative_to(ROOT)}:{number}: duplicate heading {heading}")
        headings.add(heading)

    table_width: int | None = None
    for number, line in enumerate(text.splitlines(), 1):
        if line.startswith("|") and line.endswith("|"):
            width = line.count("|")
            if table_width is None:
                table_width = width
            elif width != table_width:
                raise PlanningError(
                    f"{path.relative_to(ROOT)}:{number}: table pipe count {width}, expected {table_width}"
                )
        else:
            table_width = None

    prose: list[str] = []
    inside_fence = False
    for line in lines:
        if line.startswith("```"):
            inside_fence = not inside_fence
            continue
        if not inside_fence:
            prose.append(re.sub(r"`[^`]*`", "", line))

    for match in LINK.finditer("\n".join(prose)):
        raw_target = unquote(match.group(1).strip())
        if raw_target.startswith(("http://", "https://", "mailto:", "/")):
            continue
        target, separator, fragment = raw_target.partition("#")
        resolved = path.resolve() if not target else (path.parent / target).resolve()
        try:
            resolved.relative_to(ROOT)
        except ValueError as error:
            raise PlanningError(
                f"{path.relative_to(ROOT)}: relative link escapes repository: {target}"
            ) from error
        if not resolved.exists():
            raise PlanningError(f"{path.relative_to(ROOT)}: missing relative link target {target}")
        if not is_git_tracked_or_staged_target(resolved, tracked_paths):
            raise PlanningError(
                f"{path.relative_to(ROOT)}: relative link target is not Git tracked/staged: {target}"
            )
        if separator and fragment:
            target_text = text if resolved == path.resolve() else resolved.read_text(encoding="utf-8")
            if fragment not in heading_anchors(target_text):
                raise PlanningError(
                    f"{path.relative_to(ROOT)}: missing relative Markdown fragment "
                    f"{fragment} in {target or path.name}"
                )


def check_frozen_first_audit(data: bytes) -> None:
    header = f"blob {len(data)}\0".encode("ascii")
    actual = hashlib.sha1(header + data).hexdigest()
    if actual != FIRST_AUDIT_GIT_BLOB:
        raise PlanningError(
            "docs/FUTURE_PLAN_AUDIT.md changed after its frozen first addition: "
            f"expected Git blob {FIRST_AUDIT_GIT_BLOB}, got {actual}"
        )


def check_frozen_second_audit(data: bytes) -> None:
    header = f"blob {len(data)}\0".encode("ascii")
    actual = hashlib.sha1(header + data).hexdigest()
    if actual != SECOND_AUDIT_GIT_BLOB:
        raise PlanningError(
            "docs/FUTURE_PLAN_AUDIT_LOOP2.md changed after its frozen first addition: "
            f"expected Git blob {SECOND_AUDIT_GIT_BLOB}, got {actual}"
        )


def check_frozen_design_audit(data: bytes) -> None:
    header = f"blob {len(data)}\0".encode("ascii")
    actual = hashlib.sha1(header + data).hexdigest()
    if actual != DESIGN_AUDIT_GIT_BLOB:
        raise PlanningError(
            "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT.md changed after its frozen first addition: "
            f"expected Git blob {DESIGN_AUDIT_GIT_BLOB}, got {actual}"
        )


def check_frozen_successor_audit(data: bytes) -> None:
    header = f"blob {len(data)}\0".encode("ascii")
    actual = hashlib.sha1(header + data).hexdigest()
    if actual != SUCCESSOR_AUDIT_GIT_BLOB:
        raise PlanningError(
            "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP2.md changed after its frozen first "
            f"addition: expected Git blob {SUCCESSOR_AUDIT_GIT_BLOB}, got {actual}"
        )


def check_frozen_third_audit(data: bytes) -> None:
    header = f"blob {len(data)}\0".encode("ascii")
    actual = hashlib.sha1(header + data).hexdigest()
    if actual != THIRD_AUDIT_GIT_BLOB:
        raise PlanningError(
            "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP3.md changed after its frozen first "
            f"addition: expected Git blob {THIRD_AUDIT_GIT_BLOB}, got {actual}"
        )


def check_frozen_design_receipt_bytes(data: bytes) -> None:
    header = f"blob {len(data)}\0".encode("ascii")
    actual = hashlib.sha1(header + data).hexdigest()
    if actual != DESIGN_RECEIPT_GIT_BLOB:
        raise PlanningError(
            "docs/design-source-receipt-v1.json changed after first admission: "
            f"expected Git blob {DESIGN_RECEIPT_GIT_BLOB}, got {actual}"
        )


def parse_frozen_design_receipt(data: bytes) -> dict[str, object]:
    try:
        receipt = json.loads(data.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise PlanningError(f"invalid design source receipt JSON: {error}") from error
    if not isinstance(receipt, dict):
        raise PlanningError("design source receipt must be a JSON object")
    canonical = json.dumps(
        receipt,
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=False,
    ).encode("utf-8")
    actual = hashlib.sha256(canonical).hexdigest()
    if actual != DESIGN_RECEIPT_CANONICAL_SHA256:
        raise PlanningError(
            "docs/design-source-receipt-v1.json changed after admission: "
            f"expected canonical SHA-256 {DESIGN_RECEIPT_CANONICAL_SHA256}, got {actual}"
        )
    return receipt


def read_historical_git_blob(commit: str, path: Path) -> bytes:
    try:
        relative = path.resolve().relative_to(ROOT).as_posix()
    except ValueError as error:
        raise PlanningError(f"historical receipt path escapes repository: {path}") from error
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise PlanningError(f"historical receipt commit is not a full lowercase Git ID: {commit}")
    object_name = f"{commit}:{relative}"
    try:
        result = subprocess.run(
            ["git", "cat-file", "blob", object_name],
            cwd=ROOT,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except (OSError, subprocess.CalledProcessError) as error:
        detail = getattr(error, "stderr", b"").decode("utf-8", errors="replace").strip()
        raise PlanningError(
            f"cannot resolve historical receipt object {object_name}: {detail or error}"
        ) from error
    return result.stdout


def check_audited_distillation_receipt(receipt: dict[str, object]) -> None:
    audited = receipt.get("audited_distillation_input")
    if not isinstance(audited, dict):
        raise PlanningError("design source receipt missing audited_distillation_input object")
    commit = audited.get("commit")
    if commit != DESIGN_RECEIPT_AUDITED_COMMIT:
        raise PlanningError(
            "design source receipt audited commit changed: "
            f"expected {DESIGN_RECEIPT_AUDITED_COMMIT}, got {commit}"
        )
    data = read_historical_git_blob(commit, DESIGN_DISTILLATION)
    try:
        decoded = data.decode("utf-8-sig")
    except UnicodeDecodeError as error:
        raise PlanningError(f"audited distillation is not UTF-8/BOM text: {error}") from error
    actual = {
        "git_blob_sha1": hashlib.sha1(f"blob {len(data)}\0".encode("ascii") + data).hexdigest(),
        "hash_algorithm": "SHA-256",
        "sha256": hashlib.sha256(data).hexdigest(),
        "raw_bytes": len(data),
        "decoded_utf16_code_units": len(decoded.encode("utf-16-le")) // 2,
        "whitespace_word_count": len(re.findall(r"\S+", decoded)),
        "line_count": decoded.count("\n") + 1,
    }
    for field, actual_value in actual.items():
        if audited.get(field) != actual_value:
            raise PlanningError(
                f"design source receipt audited {field} disagrees with Git object: "
                f"expected {actual_value}, got {audited.get(field)}"
            )


def check_design_source_receipt(text: str, receipt: dict[str, object]) -> None:
    external = receipt.get("external_source")
    if not isinstance(external, dict):
        raise PlanningError("design source receipt missing external_source object")
    interpretation = external.get("text_interpretation")
    if not isinstance(interpretation, dict):
        raise PlanningError("design source receipt missing text_interpretation object")
    expected = {
        "source_path": external.get("path_hint"),
        "source_bytes": external.get("raw_bytes"),
        "source_characters": interpretation.get("decoded_utf16_code_units"),
        "source_words": interpretation.get("whitespace_word_count"),
        "source_lines": interpretation.get("line_count"),
        "source_sha256": external.get("sha256"),
        "source_mtime": external.get("observed_mtime"),
    }
    for label, expected_value in expected.items():
        matches = re.findall(rf"^{re.escape(label)}\s+(\S+)", text, flags=re.MULTILINE)
        if len(matches) != 1:
            raise PlanningError(f"design distillation must contain exactly one {label} receipt field")
        if matches[0] != str(expected_value):
            raise PlanningError(
                f"design distillation {label} disagrees with frozen receipt: "
                f"expected {expected_value}, got {matches[0]}"
            )


def expect_failure(label: str, text: str) -> None:
    try:
        parse_capability(text)
    except PlanningError:
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_markdown_failure(
    label: str,
    path: Path,
    text: str,
    *,
    tracked_paths: frozenset[Path] | None = None,
) -> None:
    try:
        check_markdown(path, text, tracked_paths=tracked_paths)
    except PlanningError:
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_frozen_audit_failure(label: str, data: bytes) -> None:
    try:
        check_frozen_first_audit(data)
    except PlanningError:
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_frozen_second_audit_failure(label: str, data: bytes) -> None:
    try:
        check_frozen_second_audit(data)
    except PlanningError:
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_frozen_design_audit_failure(label: str, data: bytes) -> None:
    try:
        check_frozen_design_audit(data)
    except PlanningError:
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_frozen_successor_audit_failure(label: str, data: bytes) -> None:
    try:
        check_frozen_successor_audit(data)
    except PlanningError:
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_frozen_third_audit_failure(label: str, data: bytes) -> None:
    try:
        check_frozen_third_audit(data)
    except PlanningError:
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_frozen_design_receipt_failure(label: str, data: bytes) -> None:
    try:
        parse_frozen_design_receipt(data)
    except PlanningError:
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_frozen_design_receipt_bytes_failure(label: str, data: bytes) -> None:
    try:
        check_frozen_design_receipt_bytes(data)
    except PlanningError:
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_planning_inventory_failure(label: str, paths: tuple[Path, ...]) -> None:
    try:
        check_planning_inventory(paths)
    except PlanningError:
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_frozen_text_eol_policy_failure(
    label: str,
    policy: dict[str, dict[str, str]],
) -> None:
    try:
        check_frozen_text_eol_policy(policy)
    except PlanningError:
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_audited_distillation_receipt_failure(
    label: str,
    receipt: dict[str, object],
) -> None:
    try:
        check_audited_distillation_receipt(receipt)
    except PlanningError:
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_design_source_receipt_failure(
    label: str,
    text: str,
    receipt: dict[str, object],
) -> None:
    try:
        check_design_source_receipt(text, receipt)
    except PlanningError:
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_conditional_failure(
    label: str,
    text: str,
    nodes: dict[str, Node],
    *,
    enforce_manifest: bool = True,
    expected_message: str | None = None,
) -> None:
    try:
        parse_conditional_edges(text, nodes, enforce_manifest=enforce_manifest)
    except PlanningError as error:
        if expected_message is not None and expected_message not in str(error):
            raise PlanningError(
                f"negative control failed by the wrong mechanism: {label}: {error}"
            ) from error
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_predicate_entailment_failure(
    label: str,
    text: str,
    nodes: dict[str, Node],
    *,
    enforce_manifest: bool = True,
    expected_message: str | None = None,
) -> None:
    conditional_edges = parse_conditional_edges(text, nodes)
    try:
        parse_predicate_entailments(
            text,
            conditional_edges,
            enforce_manifest=enforce_manifest,
        )
    except PlanningError as error:
        if expected_message is not None and expected_message not in str(error):
            raise PlanningError(
                f"negative control failed by the wrong mechanism: {label}: {error}"
            ) from error
        return
    raise PlanningError(f"negative control did not fail: {label}")


def run_self_tests(
    text: str,
    first_audit_bytes: bytes,
    second_audit_bytes: bytes,
    design_audit_bytes: bytes,
    successor_audit_bytes: bytes,
    third_audit_bytes: bytes,
    design_receipt_bytes: bytes,
    design_text: str,
    design_receipt: dict[str, object],
    tracked_paths: frozenset[Path],
    frozen_eol_policy: dict[str, dict[str, str]],
) -> None:
    reduced_inventory = tuple(
        path for path in PLANNING_FILES if repository_relative_path(path) != "docs/PROTOCOL.md"
    )
    if len(reduced_inventory) != len(PLANNING_FILES) - 1:
        raise PlanningError("self-test fixture drift: planning authority inventory")
    expect_planning_inventory_failure(
        "missing planning authority document",
        reduced_inventory,
    )

    wrong_eol_policy = {
        path: dict(attributes) for path, attributes in frozen_eol_policy.items()
    }
    eol_probe_path = repository_relative_path(THIRD_AUDIT)
    wrong_eol_policy[eol_probe_path]["eol"] = "crlf"
    expect_frozen_text_eol_policy_failure(
        "fresh-checkout EOL policy changed from LF",
        wrong_eol_policy,
    )
    missing_eol_policy = dict(frozen_eol_policy)
    missing_eol_policy.pop(repository_relative_path(DESIGN_RECEIPT))
    expect_frozen_text_eol_policy_failure(
        "frozen artifact missing from EOL policy",
        missing_eol_policy,
    )

    unknown = text.replace(
        "- **Depends on:** `TICK-0`, `REPLAY-0`.",
        "- **Depends on:** `TICK-0`, `REPLAY-0`, `MISSING-9`.",
        1,
    )
    if unknown == text:
        raise PlanningError("self-test fixture drift: ACCOUNT-1 dependency")
    expect_failure("undefined dependency", unknown)

    foundation_cycle = text.replace(
        "| `OBSERVE-0` | **IMPLEMENTED, bounded** | `WORLD-0` |",
        "| `OBSERVE-0` | **IMPLEMENTED, bounded** | `TICK-0` |",
        1,
    )
    if foundation_cycle == text:
        raise PlanningError("self-test fixture drift: OBSERVE-0 foundation row")
    expect_failure("foundation dependency cycle", foundation_cycle)

    missing_field = text.replace("- **Gate:**", "- **Removed gate:**", 1)
    expect_failure("missing required field", missing_field)

    duplicate = text + "\n### `ACCOUNT-1` — duplicate negative control\n"
    expect_failure("duplicate node", duplicate)

    bad_status = text.replace(
        "- **Status:** **READY; preferred next semantic implementation checkpoint**.",
        "- **Status:** **MAGICALLY SECURE**.",
        1,
    )
    if bad_status == text:
        raise PlanningError("self-test fixture drift: ACCOUNT-1 status")
    expect_failure("invalid status vocabulary", bad_status)

    false_promotion = text.replace(
        "- **Status:** **READY; preferred next semantic implementation checkpoint**.",
        "- **Status:** **IMPLEMENTED**.",
        1,
    )
    if false_promotion == text:
        raise PlanningError("self-test fixture drift: ACCOUNT-1 false promotion")
    expect_failure("guarded status promotion", false_promotion)

    missing_inventory = re.sub(
        r"^### `HASH-R1`.*?(?=^### |^## )",
        "",
        text,
        count=1,
        flags=re.MULTILINE | re.DOTALL,
    )
    if missing_inventory == text:
        raise PlanningError("self-test fixture drift: HASH-R1 inventory")
    expect_failure("missing capability inventory", missing_inventory)

    boundary_probe = text.replace(
        "## Current unlock frontier",
        "## Current unlock frontier\n\n- **Gate:** outside-node boundary probe",
        1,
    )
    if boundary_probe == text:
        raise PlanningError("self-test fixture drift: post-node section boundary")
    parse_capability(boundary_probe)

    nodes = parse_capability(text)
    conditional_edges = parse_conditional_edges(text, nodes)
    predicate_entailments = parse_predicate_entailments(text, conditional_edges)
    verkle_closure = expand_predicates({"verkle-prototype"}, predicate_entailments)
    if "verkle-selected" not in verkle_closure:
        raise PlanningError(
            "predicate closure did not carry verkle-prototype to verkle-selected"
        )
    missing_conditional = text.replace(
        "| CE-01 | PROFILE-SCOPE-D0 | candidate-scope | asset-active | ASSET-D1 | design |\n",
        "",
        1,
    )
    if missing_conditional == text:
        raise PlanningError("self-test fixture drift: CE-01")
    expect_conditional_failure("missing conditional edge", missing_conditional, nodes)

    wrong_polarity = text.replace(
        "| CE-30 | UPGRADE-1 | migration | migration-profile | PROFILE-1 | old-admitted-profile |",
        "| CE-30 | UPGRADE-1 | migration | migration-profile | PROFILE-1 | new-candidate-profile |",
        1,
    )
    if wrong_polarity == text:
        raise PlanningError("self-test fixture drift: upgrade polarity")
    expect_conditional_failure("changed old/new profile polarity", wrong_polarity, nodes)

    ce42 = "| CE-42 | PROFILE-1 | new-admission | verkle-selected | VERKLE-R1 | prototype |"
    missing_verkle_admission = text.replace(ce42 + "\n", "", 1)
    if missing_verkle_admission == text:
        raise PlanningError("self-test fixture drift: CE-42 admission dependency")
    expect_conditional_failure(
        "missing candidate-bound Verkle admission dependency",
        missing_verkle_admission,
        nodes,
        expected_message="conditional edge inventory changed without updating its intentional guard:",
    )

    wrong_verkle_admission_predicate = text.replace(
        ce42,
        "| CE-42 | PROFILE-1 | new-admission | verkle-prototype | VERKLE-R1 | prototype |",
        1,
    )
    if wrong_verkle_admission_predicate == text:
        raise PlanningError("self-test fixture drift: CE-42 predicate")
    expect_conditional_failure(
        "Verkle admission bypasses selected-profile predicate",
        wrong_verkle_admission_predicate,
        nodes,
        expected_message="conditional roles, predicates, or polarity changed without updating their guard:",
    )

    wrong_verkle_admission_role = text.replace(
        ce42,
        "| CE-42 | PROFILE-1 | old-admitted-profile | verkle-selected | VERKLE-R1 | feasibility |",
        1,
    )
    if wrong_verkle_admission_role == text:
        raise PlanningError("self-test fixture drift: CE-42 candidate binding")
    expect_conditional_failure(
        "Verkle admission loses new-candidate prototype binding",
        wrong_verkle_admission_role,
        nodes,
        expected_message="conditional roles, predicates, or polarity changed without updating their guard:",
    )

    ce43 = "| CE-43 | PROFILE-D1 | new-candidate-profile | always | PROFILE-SCOPE-D0 | candidate-scope |"
    missing_current_scope = text.replace(ce43 + "\n", "", 1)
    if missing_current_scope == text:
        raise PlanningError("self-test fixture drift: CE-43 current scope")
    expect_conditional_failure(
        "missing mandatory current-scope dependency",
        missing_current_scope,
        nodes,
        expected_message="conditional edge inventory changed without updating its intentional guard:",
    )

    optional_current_scope = text.replace(
        ce43,
        "| CE-43 | PROFILE-D1 | new-candidate-profile | migration-profile | PROFILE-SCOPE-D0 | candidate-scope |",
        1,
    )
    if optional_current_scope == text:
        raise PlanningError("self-test fixture drift: CE-43 always predicate")
    expect_conditional_failure(
        "current scope changed from mandatory to candidate-conditional",
        optional_current_scope,
        nodes,
        expected_message="conditional roles, predicates, or polarity changed without updating their guard:",
    )

    collapsed_current_scope_role = text.replace(
        ce43,
        "| CE-43 | PROFILE-D1 | candidate-scope | always | PROFILE-SCOPE-D0 | base |",
        1,
    )
    if collapsed_current_scope_role == text:
        raise PlanningError("self-test fixture drift: CE-43 role distinction")
    expect_conditional_failure(
        "current-scope instance roles collapsed to base aliases",
        collapsed_current_scope_role,
        nodes,
        expected_message="conditional roles, predicates, or polarity changed without updating their guard:",
    )

    conditional_cycle = text.replace(
        "| CE-40 | VERKLE-R1 | prototype | verkle-prototype | PROFILE-D1 | new-candidate-profile |",
        "| CE-40 | VERKLE-R1 | prototype | verkle-prototype | PROFILE-D1 | new-candidate-profile |\n"
        "| CE-44 | PROFILE-D1 | new-candidate-profile | migration-profile | UPGRADE-1 | migration |",
        1,
    )
    if conditional_cycle == text:
        raise PlanningError("self-test fixture drift: conditional cycle")
    expect_conditional_failure(
        "role-aware conditional cycle",
        conditional_cycle,
        nodes,
        enforce_manifest=False,
        expected_message="role-aware conditional dependency cycle:",
    )

    missing_entailment = text.replace(
        "| PE-11 | verkle-prototype | verkle-selected |\n",
        "",
        1,
    )
    if missing_entailment == text:
        raise PlanningError("self-test fixture drift: PE-11")
    expect_predicate_entailment_failure(
        "missing predicate entailment",
        missing_entailment,
        nodes,
    )

    reversed_entailment = text.replace(
        "| PE-11 | verkle-prototype | verkle-selected |",
        "| PE-11 | verkle-selected | verkle-prototype |",
        1,
    )
    if reversed_entailment == text:
        raise PlanningError("self-test fixture drift: predicate entailment polarity")
    expect_predicate_entailment_failure(
        "reversed predicate entailment",
        reversed_entailment,
        nodes,
    )

    entailment_cycle = text.replace(
        "| PE-11 | verkle-prototype | verkle-selected |",
        "| PE-11 | verkle-prototype | verkle-selected |\n"
        "| PE-12 | verkle-selected | verkle-prototype |",
        1,
    )
    if entailment_cycle == text:
        raise PlanningError("self-test fixture drift: predicate entailment cycle")
    expect_predicate_entailment_failure(
        "predicate entailment cycle",
        entailment_cycle,
        nodes,
        enforce_manifest=False,
        expected_message="predicate entailment cycle:",
    )

    expect_markdown_failure(
        "unbalanced Markdown fence",
        CAPABILITY,
        text + "\n```\n",
        tracked_paths=tracked_paths,
    )
    expect_markdown_failure(
        "duplicate Markdown heading",
        CAPABILITY,
        text + "\n# Bitblock capability tree\n",
        tracked_paths=tracked_paths,
    )
    malformed_table = text.replace("| Status | Meaning |", "| Status Meaning |", 1)
    if malformed_table == text:
        raise PlanningError("self-test fixture drift: status table")
    expect_markdown_failure(
        "malformed Markdown table",
        CAPABILITY,
        malformed_table,
        tracked_paths=tracked_paths,
    )
    expect_markdown_failure(
        "missing relative Markdown link",
        CAPABILITY,
        text + "\n[missing](missing-planning-doc-negative-control.md)\n",
        tracked_paths=tracked_paths,
    )
    expect_markdown_failure(
        "relative Markdown link escaping repository",
        DESIGN_DISTILLATION,
        "[escape](../../outside-repository-negative-control.md)\n",
        tracked_paths=tracked_paths,
    )
    expect_markdown_failure(
        "relative Markdown link to untracked content",
        DESIGN_DISTILLATION,
        "[untracked](design-source-receipt-v1.json)\n",
        tracked_paths=tracked_paths - {DESIGN_RECEIPT.resolve()},
    )
    bad_fragment = design_text.replace(
        "AUTHENTICATED_STATE_PLAN.md#world-roots-are-not-history-roots",
        "AUTHENTICATED_STATE_PLAN.md#missing-fragment-negative-control",
        1,
    )
    if bad_fragment == design_text:
        raise PlanningError("self-test fixture drift: design distillation fragment")
    expect_markdown_failure(
        "missing relative Markdown fragment",
        DESIGN_DISTILLATION,
        bad_fragment,
        tracked_paths=tracked_paths,
    )
    expect_frozen_audit_failure(
        "frozen first audit mutation",
        first_audit_bytes + b"\nnegative-control",
    )
    expect_frozen_second_audit_failure(
        "frozen second audit mutation",
        second_audit_bytes + b"\nnegative-control",
    )
    expect_frozen_design_audit_failure(
        "frozen design audit mutation",
        design_audit_bytes + b"\nnegative-control",
    )
    expect_frozen_successor_audit_failure(
        "frozen successor audit mutation",
        successor_audit_bytes + b"\nnegative-control",
    )
    expect_frozen_third_audit_failure(
        "frozen third audit mutation",
        third_audit_bytes + b"\nnegative-control",
    )
    expect_frozen_design_receipt_bytes_failure(
        "byte-frozen design receipt mutation",
        design_receipt_bytes + b"\n",
    )
    external = design_receipt["external_source"]
    external_sha = external.get("sha256") if isinstance(external, dict) else None
    if not isinstance(external_sha, str):
        raise PlanningError("self-test fixture drift: design source receipt digest")
    receipt_mutation = design_receipt_bytes.replace(
        external_sha.encode("ascii"),
        ("0" * 64).encode("ascii"),
        1,
    )
    if receipt_mutation == design_receipt_bytes:
        raise PlanningError("self-test fixture drift: frozen design receipt")
    expect_frozen_design_receipt_failure("frozen design receipt mutation", receipt_mutation)
    markdown_receipt_mutation = design_text.replace(external_sha, "0" * 64, 1)
    if markdown_receipt_mutation == design_text:
        raise PlanningError("self-test fixture drift: Markdown design receipt")
    expect_design_source_receipt_failure(
        "Markdown design receipt mutation",
        markdown_receipt_mutation,
        design_receipt,
    )

    wrong_commit = json.loads(json.dumps(design_receipt))
    wrong_commit["audited_distillation_input"]["commit"] = "0" * 40
    expect_audited_distillation_receipt_failure(
        "audited distillation wrong commit",
        wrong_commit,
    )
    wrong_hash = json.loads(json.dumps(design_receipt))
    wrong_hash["audited_distillation_input"]["sha256"] = "0" * 64
    expect_audited_distillation_receipt_failure(
        "audited distillation wrong SHA-256",
        wrong_hash,
    )
    wrong_count = json.loads(json.dumps(design_receipt))
    audited_input = wrong_count["audited_distillation_input"]
    audited_input["decoded_utf16_code_units"] += 1
    expect_audited_distillation_receipt_failure(
        "audited distillation wrong UTF-16 count",
        wrong_count,
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true", help="run powered in-memory failures")
    args = parser.parse_args()

    check_planning_inventory(PLANNING_FILES)
    frozen_eol_policy = git_effective_eol_policy(FROZEN_TEXT_FILES)
    check_frozen_text_eol_policy(frozen_eol_policy)
    capability_text = CAPABILITY.read_text(encoding="utf-8")
    nodes = parse_capability(capability_text)
    conditional_edges = parse_conditional_edges(capability_text, nodes)
    predicate_entailments = parse_predicate_entailments(capability_text, conditional_edges)
    tracked_paths = git_tracked_or_staged_paths()
    first_audit_bytes = FIRST_AUDIT.read_bytes()
    check_frozen_first_audit(first_audit_bytes)
    second_audit_bytes = SECOND_AUDIT.read_bytes()
    check_frozen_second_audit(second_audit_bytes)
    design_audit_bytes = DESIGN_AUDIT.read_bytes()
    check_frozen_design_audit(design_audit_bytes)
    successor_audit_bytes = SUCCESSOR_AUDIT.read_bytes()
    check_frozen_successor_audit(successor_audit_bytes)
    third_audit_bytes = THIRD_AUDIT.read_bytes()
    check_frozen_third_audit(third_audit_bytes)
    design_receipt_bytes = DESIGN_RECEIPT.read_bytes()
    check_frozen_design_receipt_bytes(design_receipt_bytes)
    design_receipt = parse_frozen_design_receipt(design_receipt_bytes)
    check_audited_distillation_receipt(design_receipt)
    design_text = DESIGN_DISTILLATION.read_text(encoding="utf-8")
    check_design_source_receipt(design_text, design_receipt)
    for path in PLANNING_FILES:
        check_markdown(
            path,
            path.read_text(encoding="utf-8"),
            tracked_paths=tracked_paths,
        )
    if args.self_test:
        run_self_tests(
            capability_text,
            first_audit_bytes,
            second_audit_bytes,
            design_audit_bytes,
            successor_audit_bytes,
            third_audit_bytes,
            design_receipt_bytes,
            design_text,
            design_receipt,
            tracked_paths,
            frozen_eol_policy,
        )

    base_edge_count = sum(len(node.dependencies) for node in nodes.values())
    edge_count = base_edge_count + len(conditional_edges)
    suffix = " with negative controls" if args.self_test else ""
    print(
        f"planning-doc check PASS: {len(nodes)} capability nodes, "
        f"{edge_count} declared base-plus-conditional edges, "
        f"{len(predicate_entailments)} predicate entailments, "
        f"{len(PLANNING_FILES)} Markdown files and 1 frozen source receipt{suffix}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
