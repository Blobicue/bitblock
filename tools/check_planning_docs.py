#!/usr/bin/env python3
"""Validate Bitblock planning documents without changing repository state."""

from __future__ import annotations

import argparse
import re
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import unquote


ROOT = Path(__file__).resolve().parents[1]
CAPABILITY = ROOT / "docs" / "CAPABILITY_TREE.md"
PLANNING_FILES = (
    ROOT / "README.md",
    ROOT / "docs" / "PROJECT_INTENT.md",
    CAPABILITY,
    ROOT / "docs" / "AUTHENTICATED_STATE_PLAN.md",
    ROOT / "docs" / "FUTURE_PLAN_AUDIT.md",
    ROOT / "docs" / "TRANSCRIPT_AUDIT.md",
    ROOT / "docs" / "HANDOFF.md",
    ROOT / "docs" / "LAYER1_DESIGN_PAPER.md",
    ROOT / "docs" / "LAYER1_DESIGN_AUDIT.md",
)
NODE_ID = r"[A-Z][A-Z0-9-]*"
DETAIL_HEADING = re.compile(rf"^### `({NODE_ID})`(?:\s|$)")
FIELD = re.compile(r"^- \*\*(Status|Depends on|Motive|Artifact|Gate|Non-claim):\*\*\s*(.*)$")
DEPENDENCY = re.compile(rf"`({NODE_ID})`")
FOUNDATION_ROW = re.compile(rf"^\| `({NODE_ID})` \| \*\*([^|]+)\*\* \| ([^|]+) \|")
LINK = re.compile(r"\[[^\]]*\]\(([^)]+)\)")


class PlanningError(ValueError):
    pass


@dataclass(frozen=True)
class Node:
    node_id: str
    dependencies: tuple[str, ...]
    source: str


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
    return {name: " ".join(parts).strip() for name, parts in fields.items()}


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
        nodes[node_id] = Node(node_id, tuple(DEPENDENCY.findall(prerequisites)), f"line {number}")

    headings: list[tuple[int, str]] = []
    for index, line in enumerate(lines):
        match = DETAIL_HEADING.match(line)
        if match:
            headings.append((index, match.group(1)))

    for position, (start, node_id) in enumerate(headings):
        if node_id in nodes:
            raise PlanningError(f"duplicate node {node_id} at line {start + 1}")
        end = headings[position + 1][0] if position + 1 < len(headings) else len(lines)
        fields = _field_blocks(lines, start + 1, end, node_id)
        dependency_text = fields["Depends on"]
        dependencies = tuple(DEPENDENCY.findall(dependency_text))
        if not dependencies and "none" not in dependency_text.lower():
            raise PlanningError(f"{node_id}: dependency field names no node and does not say None")
        nodes[node_id] = Node(node_id, dependencies, f"line {start + 1}")

    if not nodes:
        raise PlanningError("no capability nodes parsed")

    for node in nodes.values():
        unknown = sorted(set(node.dependencies) - nodes.keys())
        if unknown:
            raise PlanningError(f"{node.node_id}: undefined dependencies: {', '.join(unknown)}")

    state: dict[str, int] = {}
    stack: list[str] = []

    def visit(node_id: str) -> None:
        state[node_id] = 1
        stack.append(node_id)
        for dependency in nodes[node_id].dependencies:
            if state.get(dependency) == 1:
                begin = stack.index(dependency)
                cycle = stack[begin:] + [dependency]
                raise PlanningError("dependency cycle: " + " -> ".join(cycle))
            if state.get(dependency, 0) == 0:
                visit(dependency)
        stack.pop()
        state[node_id] = 2

    for node_id in nodes:
        if state.get(node_id, 0) == 0:
            visit(node_id)
    return nodes


def check_markdown(path: Path, text: str) -> None:
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
        target = unquote(match.group(1).split("#", 1)[0].strip())
        if not target or target.startswith(("http://", "https://", "mailto:", "/")):
            continue
        resolved = (path.parent / target).resolve()
        if not resolved.exists():
            raise PlanningError(f"{path.relative_to(ROOT)}: missing relative link target {target}")


def expect_failure(label: str, text: str) -> None:
    try:
        parse_capability(text)
    except PlanningError:
        return
    raise PlanningError(f"negative control did not fail: {label}")


def run_self_tests(text: str) -> None:
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


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true", help="run powered in-memory failures")
    args = parser.parse_args()

    capability_text = CAPABILITY.read_text(encoding="utf-8")
    nodes = parse_capability(capability_text)
    for path in PLANNING_FILES:
        check_markdown(path, path.read_text(encoding="utf-8"))
    if args.self_test:
        run_self_tests(capability_text)

    edge_count = sum(len(node.dependencies) for node in nodes.values())
    suffix = " with negative controls" if args.self_test else ""
    print(
        f"planning-doc check PASS: {len(nodes)} capability nodes, "
        f"{edge_count} conservative edges, {len(PLANNING_FILES)} files{suffix}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
