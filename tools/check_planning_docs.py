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
FOURTH_AUDIT = ROOT / "docs" / "DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP4.md"
FOURTH_AUDIT_GIT_BLOB = "350d1af6279d7f441f45fba2689481c0dbea79a5"
FIFTH_AUDIT = ROOT / "docs" / "DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP5.md"
FIFTH_AUDIT_GIT_BLOB = "519451b120b458735afaa59daa01aa23466e165e"
LAYER1_DESIGN_AUDIT = ROOT / "docs" / "LAYER1_DESIGN_AUDIT.md"
LAYER1_DESIGN_AUDIT_GIT_BLOB = "124e066b418a61279abf90c1b1b2c2cc0604ef2a"
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
    FOURTH_AUDIT,
    FIFTH_AUDIT,
    LAYER1_DESIGN_AUDIT,
    DESIGN_RECEIPT,
)
EXPECTED_FROZEN_TEXT_PATHS = frozenset(
    {
        "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT.md",
        "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP2.md",
        "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP3.md",
        "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP4.md",
        "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP5.md",
        "docs/FUTURE_PLAN_AUDIT.md",
        "docs/FUTURE_PLAN_AUDIT_LOOP2.md",
        "docs/LAYER1_DESIGN_AUDIT.md",
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
    FOURTH_AUDIT,
    FIFTH_AUDIT,
    CAPABILITY,
    ROOT / "docs" / "AUTHENTICATED_STATE_PLAN.md",
    ROOT / "docs" / "FUTURE_PLAN_AUDIT.md",
    SECOND_AUDIT,
    ROOT / "docs" / "SEMANTICS.md",
    ROOT / "docs" / "TRANSCRIPT_AUDIT.md",
    ROOT / "docs" / "HANDOFF.md",
    ROOT / "docs" / "LAYER1_DESIGN_PAPER.md",
    LAYER1_DESIGN_AUDIT,
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
        "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP4.md",
        "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP5.md",
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
    "3411c02e25882d9017056f1b21559dacca7bd853e6e80884bb316023161de129"
)
NODE_ID = r"[A-Z][A-Z0-9-]*"
PHASE_NODE_ID = r"[A-Z0-9]+(?:-[A-Z0-9]+)*"
DETAIL_HEADING = re.compile(rf"^### `({NODE_ID})`(?:\s|$)")
FIELD_BOUNDARY = re.compile(r"^#{1,3}\s")
FIELD = re.compile(r"^- \*\*(Status|Depends on|Motive|Artifact|Gate|Non-claim):\*\*\s*(.*)$")
DEPENDENCY = re.compile(rf"`({NODE_ID})`")
FOUNDATION_ROW = re.compile(rf"^\| `({NODE_ID})` \| \*\*([^|]+)\*\* \| ([^|]+) \|")
CONDITIONAL_ROW = re.compile(
    rf"^\| (CE-[0-9]{{2}}) \| ({NODE_ID}) \| ([a-z0-9]+(?:-[a-z0-9]+)*) \| "
    rf"(launch-profile|target-profile) \| "
    rf"([a-z0-9]+(?:-[a-z0-9]+)*) \| ({NODE_ID}) \| "
    rf"([a-z0-9]+(?:-[a-z0-9]+)*) \|$"
)
PHASE_STATUS_ROW = re.compile(
    rf"^\| (PS-[0-9]{{3}}) \| ({PHASE_NODE_ID}) \| ([a-z0-9]+(?:-[a-z0-9]+)*) \| "
    r"(IMPLEMENTED|BOUNDED|READY|LOCKED|OPEN-DECISION|NOT-STARTED) \|$"
)
PHASE_DEPENDENCY_ROW = re.compile(
    rf"^\| (PD-[0-9]{{2}}) \| ({PHASE_NODE_ID}) \| ([a-z0-9]+(?:-[a-z0-9]+)*) \| "
    rf"({PHASE_NODE_ID}) \| ([a-z0-9]+(?:-[a-z0-9]+)*) \|$"
)
PREDICATE_ENTAILMENT_ROW = re.compile(
    r"^\| (PE-[0-9]{2}) \| ([a-z0-9]+(?:-[a-z0-9]+)*) \| "
    r"([a-z0-9]+(?:-[a-z0-9]+)*) \|$"
)
LINK = re.compile(r"\[[^\]]*\]\(([^)]+)\)")
HEADING = re.compile(r"^#{1,6}\s+(.+?)\s*$")

EXPECTED_NODE_IDS = frozenset(
    """
    ACCOUNT-1 ASSET-1 ASSET-D1 AUTH-COMMIT-1 BOUNDS-1 CHAIN-LAUNCH-1 CLIENT-1 CODEC-1 COMMIT-1
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
    SUCCESSOR-ACTIVATION-1
    """.split()
)
EXPECTED_STATUS_DIGEST = "8195e31919187ed962fbf003eb6eab88b48e6eb6f9f0676442672846c4299354"
EXPECTED_BASE_DEPENDENCY_DIGEST = (
    "609e30bdad0010c89760faebe603a5b81c0ebe41e927a4b8827bb1429850e93e"
)
EXPECTED_PHASE_STATUS_IDS = frozenset(f"PS-{number:03d}" for number in range(1, 95))
EXPECTED_PHASE_STATUS_DIGEST = (
    "1e61f3dc86a408a80c54d6792a7f4f2a60194a63d673cb9c1d6231ed66bcb6cf"
)
EXPECTED_PHASE_DEPENDENCY_IDS = frozenset(f"PD-{number:02d}" for number in range(1, 6))
EXPECTED_PHASE_DEPENDENCY_DIGEST = (
    "0014820774d778fd3c970549bd4c4cbd3cdaa357ee7585b662452c3509544d21"
)
EXPECTED_CONDITIONAL_IDS = frozenset(f"CE-{number:02d}" for number in range(1, 45))
EXPECTED_CONDITIONAL_DIGEST = "eb15bafe8fac45ac591c468f0e98abb11489702abfa552a502919e9a35f9eb13"
EXPECTED_PREDICATE_CONTEXTS = frozenset({"launch-profile", "target-profile"})
EXPECTED_LAUNCH_CONTEXT_EDGE_IDS = frozenset(
    {"CE-06", "CE-07", "CE-08", "CE-09", "CE-22", "CE-23", "CE-24"}
)
EXPECTED_PREDICATE_ENTAILMENT_IDS = frozenset(f"PE-{number:02d}" for number in range(1, 11))
EXPECTED_PREDICATE_ENTAILMENT_DIGEST = "388678b7dad8bced1ac9bab7c81b2b022f26864fd8345c79afb4a60b1216c91c"
EXPECTED_CHAIN_LAUNCH_DEPENDENCIES = frozenset(
    {
        "PROFILE-SCOPE-D0",
        "GENESIS-D1",
        "PROTOCOL-ID-1",
        "PROFILE-D1",
        "TRANSITION-SPEC-1",
        "PROFILE-1",
        "EXECUTION-ROOT-1",
        "GENESIS-ROOT-PARITY-1",
        "RELEASE-1",
        "IDENTITY-D1",
        "GOVERNANCE-D1",
        "CRYPTO-D1",
        "CRYPTO-1",
        "CODEC-1",
    }
)
EXPECTED_SUCCESSOR_ACTIVATION_DEPENDENCIES = frozenset(
    {
        "CHAIN-LAUNCH-1",
        "PROFILE-SCOPE-D0",
        "PROFILE-D1",
        "TRANSITION-SPEC-1",
        "PROFILE-1",
        "UPGRADE-FRAMEWORK-1",
        "UPGRADE-1",
        "RELEASE-1",
        "SYNC-1",
        "EXECUTOR-1",
        "EXECUTION-ROOT-1",
        "GOVERNANCE-1",
        "CRYPTO-1",
        "CODEC-1",
        "BOUNDS-1",
    }
)
EXPECTED_CHAIN_LAUNCH_ARTIFACT_FRAGMENTS = (
    "`CHAIN-LAUNCH-1@chain-launch-manifest-id`",
    "`LaunchClosureBundleV1`",
    "`launch_closure_bundle_id`",
    "`chain_launch_package`",
    "genesis package/payload and ID",
    "`ProfileScopeV1`",
    "`DomainRegistryV1`",
    "`CandidateProfileV1`",
    "`TransitionSpecAtProfileV1`",
    "`ProfileAdmissionManifestV1`",
    "`GenesisRootParityRecordV1`",
    "initial typed execution-root reference",
    "`RELEASE-1` record and ID",
    "`ChainLaunchManifestV1` and ID",
    "`ChainLaunchAuthorizationV1`",
    "embedded-only inventory",
    "entries and retrieval locators are not members of this carrier",
)
EXPECTED_SUCCESSOR_ACTIVATION_ARTIFACT_FRAGMENTS = (
    "`SUCCESSOR-ACTIVATION-1@old-profile-id->new-profile-id`",
    "`SuccessorActivationBundleV1`",
    "`successor_activation_bundle_id`",
    "exact target `ProfileAdmissionManifestV1`",
    "concrete `UPGRADE-1` evidence",
    "old profile's envelope availability payload carries the complete bundle",
    "seven explicit preimages",
    "identical checked count, individual-length, byte-total, exclusion",
)
EXPECTED_TESTNET_LAUNCH_ARTIFACT_FRAGMENTS = (
    "exact authorized `CHAIN-LAUNCH-1` package and closure bundle",
    "same launch-manifest and closure-bundle identities",
)
EXPECTED_TESTNET_SUCCESSOR_ARTIFACT_FRAGMENTS = (
    "CE-44 additionally supplies the exact old-to-new successor bundle",
    "target admission, authorization, and availability evidence",
)
EXPECTED_LOOP5_PAPER_FRAGMENTS = {
    "PG-01": (
        "SuccessorActivationBundleV1 = Canonical(",
        "successor_activation_bundle_id = ProfileIdHashV1(",
        "SuccessorActivationSignDocV1 = Canonical(",
        "EmbeddedSuccessorActivationV1(",
    ),
    "PG-02": (
        "ProposalInnerShallowFrameV1 = FixedPreProfileShallowFrameV1(",
        "A profile whose codec cannot provide this deterministic bounded preflight is not admissible.",
        "Shallow-frame controls place a body, receipt, witness,",
    ),
    "PG-03": (
        "AcceptedParentLookupPayloadV1 = FixedPreProfileCanonicalV1(",
        "accepted_parent_lookup_id = ProfileIdHashV1(",
        "FixedPreProfileEncodeRefPayloadV1(ref) =",
        "The local accepted-parent index is keyed only by this typed lookup ID",
    ),
    "PG-04": (
        "LaunchClosureBundleV1 = Canonical(",
        "Length(E)                              = 7",
        "total_canonical_preimage_bytes_u64      =",
        "locator, content-address-only, optional-fetch, or ambient-database variant in this schema",
    ),
    "PG-05": (
        "AdmittedReleaseCoreV1 = Canonical(",
        "ReleaseSignDocV1 = Canonical(",
        "ReleaseAuthorizationV1 = Canonical(",
        "ResolveTrustedReleaseAuthorityV1(release_authority_context_ref)",
    ),
}


class PlanningError(ValueError):
    pass


@dataclass(frozen=True)
class Node:
    node_id: str
    dependencies: tuple[str, ...]
    status_class: str
    source: str


@dataclass(frozen=True)
class PhaseStatus:
    status_id: str
    node_id: str
    phase: str
    status: str


@dataclass(frozen=True)
class PhaseDependency:
    dependency_id: str
    owner_node: str
    owner_phase: str
    prerequisite_node: str
    prerequisite_phase: str


@dataclass(frozen=True)
class ConditionalEdge:
    edge_id: str
    owner_base: str
    owner_role: str
    evaluation_context: str
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


def _unconditional_dependency_clause(text: str) -> str:
    """Return only the normative base-dependency clause.

    A semicolon ends the unconditional clause. Later prose may explain phases,
    conditional edges, measurement inputs, or rejected alternatives, but it is
    not allowed to manufacture base graph edges.
    """

    return text.partition(";")[0].strip()


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
    detail_fields: dict[str, dict[str, str]] = {}

    for number, line in enumerate(lines, 1):
        match = FOUNDATION_ROW.match(line)
        if not match:
            continue
        node_id, _status, prerequisites = match.groups()
        if node_id in nodes:
            raise PlanningError(f"duplicate node {node_id} at line {number}")
        dependency_clause = _unconditional_dependency_clause(prerequisites)
        nodes[node_id] = Node(
            node_id,
            tuple(DEPENDENCY.findall(dependency_clause)),
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
        detail_fields[node_id] = fields
        dependency_text = fields["Depends on"]
        dependency_clause = _unconditional_dependency_clause(dependency_text)
        dependencies = tuple(DEPENDENCY.findall(dependency_clause))
        if not dependencies and "none" not in dependency_clause.lower():
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
        if len(node.dependencies) != len(set(node.dependencies)):
            raise PlanningError(f"{node.node_id}: duplicate unconditional dependency")
        unknown = sorted(set(node.dependencies) - nodes.keys())
        if unknown:
            raise PlanningError(f"{node.node_id}: undefined dependencies: {', '.join(unknown)}")

    chain_launch = nodes["CHAIN-LAUNCH-1"]
    actual_chain_launch_dependencies = frozenset(chain_launch.dependencies)
    if actual_chain_launch_dependencies != EXPECTED_CHAIN_LAUNCH_DEPENDENCIES:
        missing = sorted(EXPECTED_CHAIN_LAUNCH_DEPENDENCIES - actual_chain_launch_dependencies)
        extra = sorted(actual_chain_launch_dependencies - EXPECTED_CHAIN_LAUNCH_DEPENDENCIES)
        raise PlanningError(
            "CHAIN-LAUNCH-1 prerequisite closure changed: "
            f"missing={missing}, extra={extra}"
        )
    if "CHAIN-LAUNCH-1" not in nodes["TESTNET-1"].dependencies:
        raise PlanningError("TESTNET-1 does not directly consume CHAIN-LAUNCH-1")

    successor_activation = nodes["SUCCESSOR-ACTIVATION-1"]
    actual_successor_dependencies = frozenset(successor_activation.dependencies)
    if actual_successor_dependencies != EXPECTED_SUCCESSOR_ACTIVATION_DEPENDENCIES:
        missing = sorted(
            EXPECTED_SUCCESSOR_ACTIVATION_DEPENDENCIES - actual_successor_dependencies
        )
        extra = sorted(
            actual_successor_dependencies - EXPECTED_SUCCESSOR_ACTIVATION_DEPENDENCIES
        )
        raise PlanningError(
            "SUCCESSOR-ACTIVATION-1 prerequisite closure changed: "
            f"missing={missing}, extra={extra}"
        )

    chain_launch_artifact = detail_fields["CHAIN-LAUNCH-1"]["Artifact"]
    for required_fragment in EXPECTED_CHAIN_LAUNCH_ARTIFACT_FRAGMENTS:
        if required_fragment not in chain_launch_artifact:
            raise PlanningError(
                f"CHAIN-LAUNCH-1 artifact omits {required_fragment}"
            )
    testnet_artifact = detail_fields["TESTNET-1"]["Artifact"]
    for required_fragment in EXPECTED_TESTNET_LAUNCH_ARTIFACT_FRAGMENTS:
        if required_fragment not in testnet_artifact:
            raise PlanningError(
                f"TESTNET-1 artifact omits {required_fragment}"
            )
    successor_artifact = detail_fields["SUCCESSOR-ACTIVATION-1"]["Artifact"]
    for required_fragment in EXPECTED_SUCCESSOR_ACTIVATION_ARTIFACT_FRAGMENTS:
        if required_fragment not in successor_artifact:
            raise PlanningError(
                f"SUCCESSOR-ACTIVATION-1 artifact omits {required_fragment}"
            )
    for required_fragment in EXPECTED_TESTNET_SUCCESSOR_ARTIFACT_FRAGMENTS:
        if required_fragment not in testnet_artifact:
            raise PlanningError(
                f"TESTNET-1 artifact omits {required_fragment}"
            )

    base_dependency_manifest = "\n".join(
        f"{node_id}={','.join(nodes[node_id].dependencies)}" for node_id in sorted(nodes)
    )
    base_dependency_digest = hashlib.sha256(
        base_dependency_manifest.encode("utf-8")
    ).hexdigest()
    if base_dependency_digest != EXPECTED_BASE_DEPENDENCY_DIGEST:
        raise PlanningError(
            "unconditional base dependencies changed without updating their intentional guard: "
            f"expected {EXPECTED_BASE_DEPENDENCY_DIGEST}, got {base_dependency_digest}"
        )

    _assert_acyclic(
        {node_id: node.dependencies for node_id, node in nodes.items()},
        "base dependency",
    )
    return nodes


def parse_phase_statuses(
    text: str,
    nodes: dict[str, Node],
    *,
    enforce_manifest: bool = True,
) -> tuple[PhaseStatus, ...]:
    statuses: list[PhaseStatus] = []
    seen_ids: set[str] = set()
    seen_tuples: set[tuple[str, str]] = set()
    for number, line in enumerate(text.splitlines(), 1):
        if not line.startswith("| PS-"):
            continue
        match = PHASE_STATUS_ROW.match(line)
        if not match:
            raise PlanningError(f"phase-status row {number} is not canonical: {line}")
        status = PhaseStatus(*match.groups())
        if status.status_id in seen_ids:
            raise PlanningError(f"duplicate phase-status ID {status.status_id} at line {number}")
        key = (status.node_id, status.phase)
        if key in seen_tuples:
            raise PlanningError(
                f"duplicate phase-status tuple {status.node_id}@{status.phase} at line {number}"
            )
        if status.node_id not in nodes:
            raise PlanningError(
                f"{status.status_id}: undefined phase-status node {status.node_id}"
            )
        seen_ids.add(status.status_id)
        seen_tuples.add(key)
        statuses.append(status)

    if not statuses:
        raise PlanningError("no structured phase-status rows parsed")

    for node_id, node in nodes.items():
        base_rows = [
            status
            for status in statuses
            if status.node_id == node_id and status.phase == "base"
        ]
        if len(base_rows) != 1:
            raise PlanningError(
                f"{node_id}: expected exactly one base phase-status tuple, got {len(base_rows)}"
            )
        manifest_primary = base_rows[0].status.replace("-", " ")
        if manifest_primary != node.status_class:
            raise PlanningError(
                f"{node_id}: base phase status {base_rows[0].status} disagrees with prose primary "
                f"status {node.status_class}"
            )

    canonical = "\n".join(
        "|".join((row.status_id, row.node_id, row.phase, row.status))
        for row in statuses
    )
    if enforce_manifest:
        actual_ids = frozenset(seen_ids)
        if actual_ids != EXPECTED_PHASE_STATUS_IDS:
            missing = sorted(EXPECTED_PHASE_STATUS_IDS - actual_ids)
            extra = sorted(actual_ids - EXPECTED_PHASE_STATUS_IDS)
            raise PlanningError(
                "phase-status inventory changed without updating its intentional guard: "
                f"missing={missing}, extra={extra}"
            )
        digest = hashlib.sha256(canonical.encode("utf-8")).hexdigest()
        if digest != EXPECTED_PHASE_STATUS_DIGEST:
            raise PlanningError(
                "phase-status tuples changed without updating their intentional guard: "
                f"expected {EXPECTED_PHASE_STATUS_DIGEST}, got {digest}"
            )
    return tuple(statuses)


def parse_phase_dependencies(
    text: str,
    nodes: dict[str, Node],
    phase_statuses: tuple[PhaseStatus, ...],
    *,
    enforce_manifest: bool = True,
) -> tuple[PhaseDependency, ...]:
    dependencies: list[PhaseDependency] = []
    seen_ids: set[str] = set()
    seen_tuples: set[tuple[str, str, str, str]] = set()
    valid_phases = {(row.node_id, row.phase) for row in phase_statuses}
    for number, line in enumerate(text.splitlines(), 1):
        if not line.startswith("| PD-"):
            continue
        match = PHASE_DEPENDENCY_ROW.match(line)
        if not match:
            raise PlanningError(f"phase-dependency row {number} is not canonical: {line}")
        dependency = PhaseDependency(*match.groups())
        if dependency.dependency_id in seen_ids:
            raise PlanningError(
                f"duplicate phase-dependency ID {dependency.dependency_id} at line {number}"
            )
        key = (
            dependency.owner_node,
            dependency.owner_phase,
            dependency.prerequisite_node,
            dependency.prerequisite_phase,
        )
        if key in seen_tuples:
            raise PlanningError(
                f"duplicate phase-dependency tuple {'@'.join(key[:2])} -> "
                f"{'@'.join(key[2:])} at line {number}"
            )
        owner = (dependency.owner_node, dependency.owner_phase)
        prerequisite = (
            dependency.prerequisite_node,
            dependency.prerequisite_phase,
        )
        missing = [
            f"{node_id}@{phase}"
            for node_id, phase in (owner, prerequisite)
            if (node_id, phase) not in valid_phases
        ]
        if missing:
            raise PlanningError(
                f"{dependency.dependency_id}: undefined phase endpoints: {', '.join(missing)}"
            )
        if owner == prerequisite:
            raise PlanningError(
                f"{dependency.dependency_id}: phase cannot depend on itself"
            )
        seen_ids.add(dependency.dependency_id)
        seen_tuples.add(key)
        dependencies.append(dependency)

    if not dependencies:
        raise PlanningError("no structured phase-dependency rows parsed")

    canonical = "\n".join(
        "|".join(
            (
                row.dependency_id,
                row.owner_node,
                row.owner_phase,
                row.prerequisite_node,
                row.prerequisite_phase,
            )
        )
        for row in dependencies
    )
    if enforce_manifest:
        actual_ids = frozenset(seen_ids)
        if actual_ids != EXPECTED_PHASE_DEPENDENCY_IDS:
            missing = sorted(EXPECTED_PHASE_DEPENDENCY_IDS - actual_ids)
            extra = sorted(actual_ids - EXPECTED_PHASE_DEPENDENCY_IDS)
            raise PlanningError(
                "phase-dependency inventory changed without updating its intentional guard: "
                f"missing={missing}, extra={extra}"
            )
        digest = hashlib.sha256(canonical.encode("utf-8")).hexdigest()
        if digest != EXPECTED_PHASE_DEPENDENCY_DIGEST:
            raise PlanningError(
                "phase-dependency tuples changed without updating their intentional guard: "
                f"expected {EXPECTED_PHASE_DEPENDENCY_DIGEST}, got {digest}"
            )

    adjacency: dict[str, list[str]] = {
        f"{node_id}@{phase}": [
            f"{base_dependency}@base"
            for base_dependency in nodes[node_id].dependencies
        ]
        for node_id, phase in valid_phases
    }
    for dependency in dependencies:
        adjacency[f"{dependency.owner_node}@{dependency.owner_phase}"].append(
            f"{dependency.prerequisite_node}@{dependency.prerequisite_phase}"
        )
    _assert_acyclic(
        {phase: tuple(prerequisites) for phase, prerequisites in adjacency.items()},
        "phase dependency",
    )
    return tuple(dependencies)


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

    actual_contexts = frozenset(edge.evaluation_context for edge in edges)
    if actual_contexts != EXPECTED_PREDICATE_CONTEXTS:
        missing = sorted(EXPECTED_PREDICATE_CONTEXTS - actual_contexts)
        extra = sorted(actual_contexts - EXPECTED_PREDICATE_CONTEXTS)
        raise PlanningError(
            "conditional predicate-context inventory changed: "
            f"missing={missing}, extra={extra}"
        )
    actual_launch_context_edges = frozenset(
        edge.edge_id for edge in edges if edge.evaluation_context == "launch-profile"
    )
    if actual_launch_context_edges != EXPECTED_LAUNCH_CONTEXT_EDGE_IDS:
        missing = sorted(EXPECTED_LAUNCH_CONTEXT_EDGE_IDS - actual_launch_context_edges)
        extra = sorted(actual_launch_context_edges - EXPECTED_LAUNCH_CONTEXT_EDGE_IDS)
        raise PlanningError(
            "launch-profile conditional edge set changed: "
            f"missing={missing}, extra={extra}"
        )

    canonical = "\n".join(
        "|".join(
            (
                edge.edge_id,
                edge.owner_base,
                edge.owner_role,
                edge.evaluation_context,
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


def active_conditional_edges(
    conditional_edges: tuple[ConditionalEdge, ...],
    entailments: tuple[PredicateEntailment, ...],
    predicate_environment: dict[str, set[str]],
) -> tuple[ConditionalEdge, ...]:
    actual_contexts = frozenset(predicate_environment)
    if actual_contexts != EXPECTED_PREDICATE_CONTEXTS:
        missing = sorted(EXPECTED_PREDICATE_CONTEXTS - actual_contexts)
        extra = sorted(actual_contexts - EXPECTED_PREDICATE_CONTEXTS)
        raise PlanningError(
            "predicate environment changed: "
            f"missing={missing}, extra={extra}"
        )

    known_predicates = {edge.predicate for edge in conditional_edges}
    closures: dict[str, frozenset[str]] = {}
    for context in EXPECTED_PREDICATE_CONTEXTS:
        seeds = set(predicate_environment[context])
        unknown = sorted(seeds - known_predicates)
        if unknown:
            raise PlanningError(
                f"{context}: undefined predicate seeds: {', '.join(unknown)}"
            )
        seeds.add("always")
        closures[context] = expand_predicates(seeds, entailments)

    return tuple(
        edge
        for edge in conditional_edges
        if edge.predicate in closures[edge.evaluation_context]
    )


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
            # A literal pipe inside an inline-code table cell is content, not a
            # column delimiter (for example `node | phase | status`).
            width = re.sub(r"`[^`]*`", "``", line).count("|")
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


def check_frozen_fourth_audit(data: bytes) -> None:
    header = f"blob {len(data)}\0".encode("ascii")
    actual = hashlib.sha1(header + data).hexdigest()
    if actual != FOURTH_AUDIT_GIT_BLOB:
        raise PlanningError(
            "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP4.md changed after its frozen first "
            f"addition: expected Git blob {FOURTH_AUDIT_GIT_BLOB}, got {actual}"
        )


def check_frozen_fifth_audit(data: bytes) -> None:
    header = f"blob {len(data)}\0".encode("ascii")
    actual = hashlib.sha1(header + data).hexdigest()
    if actual != FIFTH_AUDIT_GIT_BLOB:
        raise PlanningError(
            "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP5.md changed after its frozen first "
            f"addition: expected Git blob {FIFTH_AUDIT_GIT_BLOB}, got {actual}"
        )


def check_frozen_layer1_design_audit(data: bytes) -> None:
    header = f"blob {len(data)}\0".encode("ascii")
    actual = hashlib.sha1(header + data).hexdigest()
    if actual != LAYER1_DESIGN_AUDIT_GIT_BLOB:
        raise PlanningError(
            "docs/LAYER1_DESIGN_AUDIT.md changed after its corrected checkpoint-0.3 freeze: "
            f"expected Git blob {LAYER1_DESIGN_AUDIT_GIT_BLOB}, got {actual}"
        )


def check_loop5_paper_fragments(text: str) -> None:
    for finding_id, fragments in EXPECTED_LOOP5_PAPER_FRAGMENTS.items():
        for fragment in fragments:
            if text.count(fragment) != 1:
                raise PlanningError(
                    f"LAYER1_DESIGN_PAPER.md must contain exactly one {finding_id} protocol "
                    f"fragment: {fragment}"
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


def expect_failure(
    label: str,
    text: str,
    *,
    expected_message: str | None = None,
) -> None:
    try:
        parse_capability(text)
    except PlanningError as error:
        if expected_message is not None and expected_message not in str(error):
            raise PlanningError(
                f"negative control failed by the wrong mechanism: {label}: {error}"
            ) from error
        return
    raise PlanningError(f"negative control did not fail: {label}")


def replace_detail_section_fragment(
    text: str,
    node_id: str,
    fragment: str,
    replacement: str,
) -> str:
    section = re.search(
        rf"^### `{re.escape(node_id)}`.*?(?=^### |^## )",
        text,
        flags=re.MULTILINE | re.DOTALL,
    )
    if section is None or fragment not in section.group(0):
        raise PlanningError(
            f"self-test fixture drift: {node_id} section fragment {fragment}"
        )
    mutated_section = section.group(0).replace(fragment, replacement, 1)
    return text[: section.start()] + mutated_section + text[section.end() :]


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


def expect_frozen_fourth_audit_failure(label: str, data: bytes) -> None:
    try:
        check_frozen_fourth_audit(data)
    except PlanningError:
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_frozen_fifth_audit_failure(label: str, data: bytes) -> None:
    try:
        check_frozen_fifth_audit(data)
    except PlanningError:
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_frozen_layer1_design_audit_failure(label: str, data: bytes) -> None:
    try:
        check_frozen_layer1_design_audit(data)
    except PlanningError:
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_loop5_paper_fragment_failure(label: str, text: str) -> None:
    try:
        check_loop5_paper_fragments(text)
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


def expect_phase_status_failure(
    label: str,
    text: str,
    nodes: dict[str, Node],
    *,
    enforce_manifest: bool = True,
    expected_message: str | None = None,
) -> None:
    try:
        parse_phase_statuses(text, nodes, enforce_manifest=enforce_manifest)
    except PlanningError as error:
        if expected_message is not None and expected_message not in str(error):
            raise PlanningError(
                f"negative control failed by the wrong mechanism: {label}: {error}"
            ) from error
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_phase_dependency_failure(
    label: str,
    text: str,
    nodes: dict[str, Node],
    *,
    enforce_manifest: bool = True,
    expected_message: str | None = None,
) -> None:
    phase_statuses = parse_phase_statuses(text, nodes)
    try:
        parse_phase_dependencies(
            text,
            nodes,
            phase_statuses,
            enforce_manifest=enforce_manifest,
        )
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
    fourth_audit_bytes: bytes,
    fifth_audit_bytes: bytes,
    layer1_design_audit_bytes: bytes,
    design_receipt_bytes: bytes,
    design_text: str,
    layer1_design_paper_text: str,
    design_receipt: dict[str, object],
    tracked_paths: frozenset[Path],
    frozen_eol_policy: dict[str, dict[str, str]],
) -> None:
    reduced_inventory = tuple(path for path in PLANNING_FILES if path != FIFTH_AUDIT)
    if len(reduced_inventory) != len(PLANNING_FILES) - 1:
        raise PlanningError("self-test fixture drift: planning authority inventory")
    expect_planning_inventory_failure(
        "missing planning authority document",
        reduced_inventory,
    )

    wrong_eol_policy = {
        path: dict(attributes) for path, attributes in frozen_eol_policy.items()
    }
    eol_probe_path = repository_relative_path(FIFTH_AUDIT)
    wrong_eol_policy[eol_probe_path]["eol"] = "crlf"
    expect_frozen_text_eol_policy_failure(
        "fresh-checkout EOL policy changed from LF",
        wrong_eol_policy,
    )
    missing_eol_policy = dict(frozen_eol_policy)
    missing_eol_policy.pop(repository_relative_path(LAYER1_DESIGN_AUDIT))
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

    manufactured_verkle_base_edge = text.replace(
        "- **Depends on:** `WORLD-0`, `PROOF-0`.",
        "- **Depends on:** `WORLD-0`, `PROOF-0`, `COMMIT-R1`.",
        1,
    )
    if manufactured_verkle_base_edge == text:
        raise PlanningError("self-test fixture drift: VERKLE-R1 base dependencies")
    expect_failure(
        "negated or adjacent prose cannot manufacture a base dependency",
        manufactured_verkle_base_edge,
        expected_message="unconditional base dependencies changed without updating their intentional guard:",
    )

    non_authoritative_verkle_prose = text.replace(
        "- **Depends on:** `WORLD-0`, `PROOF-0`.",
        "- **Depends on:** `WORLD-0`, `PROOF-0`; `COMMIT-R1` is named only as a rejected alternative.",
        1,
    )
    if non_authoritative_verkle_prose == text:
        raise PlanningError("self-test fixture drift: non-authoritative dependency prose")
    prose_nodes = parse_capability(non_authoritative_verkle_prose)
    if prose_nodes["VERKLE-R1"].dependencies != ("WORLD-0", "PROOF-0"):
        raise PlanningError("non-authoritative prose manufactured a VERKLE-R1 base dependency")

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

    missing_chain_codec = text.replace(
        "  `IDENTITY-D1`, `GOVERNANCE-D1`, `CRYPTO-D1`, `CRYPTO-1`, `CODEC-1`.",
        "  `IDENTITY-D1`, `GOVERNANCE-D1`, `CRYPTO-D1`, `CRYPTO-1`.",
        1,
    )
    if missing_chain_codec == text:
        raise PlanningError("self-test fixture drift: CHAIN-LAUNCH-1 dependency closure")
    expect_failure(
        "launch closure loses one typed prerequisite",
        missing_chain_codec,
        expected_message="CHAIN-LAUNCH-1 prerequisite closure changed:",
    )

    testnet_without_chain_launch = text.replace(
        "  `UPGRADE-FRAMEWORK-1`, `SIGNER-OPS-1`, `RELEASE-1`, `CHAIN-LAUNCH-1`, `GOVERNANCE-1`, `CRYPTO-1`,",
        "  `UPGRADE-FRAMEWORK-1`, `SIGNER-OPS-1`, `RELEASE-1`, `GOVERNANCE-1`, `CRYPTO-1`,",
        1,
    )
    if testnet_without_chain_launch == text:
        raise PlanningError("self-test fixture drift: TESTNET-1 launch dependency")
    expect_failure(
        "testnet bypasses authorized launch closure",
        testnet_without_chain_launch,
        expected_message="TESTNET-1 does not directly consume CHAIN-LAUNCH-1",
    )

    for index, fragment in enumerate(EXPECTED_CHAIN_LAUNCH_ARTIFACT_FRAGMENTS, 1):
        missing_launch_artifact = replace_detail_section_fragment(
            text,
            "CHAIN-LAUNCH-1",
            fragment,
            f"mutated-launch-artifact-{index}",
        )
        expect_failure(
            f"launch closure loses required artifact fragment {index}",
            missing_launch_artifact,
            expected_message=f"CHAIN-LAUNCH-1 artifact omits {fragment}",
        )

    for index, fragment in enumerate(EXPECTED_TESTNET_LAUNCH_ARTIFACT_FRAGMENTS, 1):
        wrong_testnet_launch_artifact = replace_detail_section_fragment(
            text,
            "TESTNET-1",
            fragment,
            f"mutated-testnet-launch-reference-{index}",
        )
        expect_failure(
            f"testnet loses exact launch-artifact reference {index}",
            wrong_testnet_launch_artifact,
            expected_message=f"TESTNET-1 artifact omits {fragment}",
        )

    successor_without_bounds = replace_detail_section_fragment(
        text,
        "SUCCESSOR-ACTIVATION-1",
        "`CRYPTO-1`, `CODEC-1`, `BOUNDS-1`.",
        "`CRYPTO-1`, `CODEC-1`.",
    )
    expect_failure(
        "successor closure loses one typed prerequisite",
        successor_without_bounds,
        expected_message="SUCCESSOR-ACTIVATION-1 prerequisite closure changed:",
    )

    for index, fragment in enumerate(EXPECTED_SUCCESSOR_ACTIVATION_ARTIFACT_FRAGMENTS, 1):
        missing_successor_artifact = replace_detail_section_fragment(
            text,
            "SUCCESSOR-ACTIVATION-1",
            fragment,
            f"mutated-successor-artifact-{index}",
        )
        expect_failure(
            f"successor closure loses required artifact fragment {index}",
            missing_successor_artifact,
            expected_message=f"SUCCESSOR-ACTIVATION-1 artifact omits {fragment}",
        )

    for index, fragment in enumerate(EXPECTED_TESTNET_SUCCESSOR_ARTIFACT_FRAGMENTS, 1):
        missing_testnet_successor = replace_detail_section_fragment(
            text,
            "TESTNET-1",
            fragment,
            f"mutated-testnet-successor-reference-{index}",
        )
        expect_failure(
            f"testnet loses exact successor-artifact reference {index}",
            missing_testnet_successor,
            expected_message=f"TESTNET-1 artifact omits {fragment}",
        )

    nodes = parse_capability(text)
    phase_statuses = parse_phase_statuses(text, nodes)
    parse_phase_dependencies(text, nodes, phase_statuses)

    for status_row, promoted_row, label in (
        (
            "| PS-019 | VERKLE-R1 | prototype | LOCKED |",
            "| PS-019 | VERKLE-R1 | prototype | READY |",
            "Verkle prototype phase promotion",
        ),
        (
            "| PS-088 | RELEASE-R1 | candidate | LOCKED |",
            "| PS-088 | RELEASE-R1 | candidate | READY |",
            "release candidate phase promotion",
        ),
    ):
        promoted = text.replace(status_row, promoted_row, 1)
        if promoted == text:
            raise PlanningError(f"self-test fixture drift: {label}")
        expect_phase_status_failure(
            label,
            promoted,
            nodes,
            expected_message="phase-status tuples changed without updating their intentional guard:",
        )

    missing_base_status = text.replace(
        "| PS-009 | ACCOUNT-1 | base | READY |\n",
        "",
        1,
    )
    if missing_base_status == text:
        raise PlanningError("self-test fixture drift: ACCOUNT-1 base phase status")
    expect_phase_status_failure(
        "missing canonical base phase status",
        missing_base_status,
        nodes,
        expected_message="ACCOUNT-1: expected exactly one base phase-status tuple",
    )

    duplicate_phase_status = text.replace(
        "| PS-019 | VERKLE-R1 | prototype | LOCKED |",
        "| PS-019 | VERKLE-R1 | prototype | LOCKED |\n"
        "| PS-095 | VERKLE-R1 | prototype | LOCKED |",
        1,
    )
    expect_phase_status_failure(
        "duplicate phase-status tuple",
        duplicate_phase_status,
        nodes,
        enforce_manifest=False,
        expected_message="duplicate phase-status tuple VERKLE-R1@prototype",
    )

    unknown_phase_status_node = text.replace(
        "| PS-019 | VERKLE-R1 | prototype | LOCKED |",
        "| PS-019 | VERKLE-R1 | prototype | LOCKED |\n"
        "| PS-095 | UNKNOWN-9 | base | LOCKED |",
        1,
    )
    expect_phase_status_failure(
        "undefined phase-status node",
        unknown_phase_status_node,
        nodes,
        enforce_manifest=False,
        expected_message="undefined phase-status node UNKNOWN-9",
    )

    malformed_phase_status = text.replace(
        "| PS-019 | VERKLE-R1 | prototype | LOCKED |",
        "| PS-019 | VERKLE-R1 | prototype | MAGIC |",
        1,
    )
    expect_phase_status_failure(
        "unknown phase-status token",
        malformed_phase_status,
        nodes,
        expected_message="phase-status row",
    )

    wrong_phase_dependency = text.replace(
        "| PD-05 | RELEASE-R1 | candidate | PROFILE-D1 | base |",
        "| PD-05 | RELEASE-R1 | candidate | COMMIT-1 | base |",
        1,
    )
    if wrong_phase_dependency == text:
        raise PlanningError("self-test fixture drift: PD-05")
    expect_phase_dependency_failure(
        "release candidate phase dependency changed",
        wrong_phase_dependency,
        nodes,
        expected_message="phase-dependency tuples changed without updating their intentional guard:",
    )

    missing_phase_dependency = text.replace(
        "| PD-05 | RELEASE-R1 | candidate | PROFILE-D1 | base |\n",
        "",
        1,
    )
    expect_phase_dependency_failure(
        "missing phase dependency",
        missing_phase_dependency,
        nodes,
        expected_message="phase-dependency inventory changed without updating its intentional guard:",
    )

    duplicate_phase_dependency = text.replace(
        "| PD-05 | RELEASE-R1 | candidate | PROFILE-D1 | base |",
        "| PD-05 | RELEASE-R1 | candidate | PROFILE-D1 | base |\n"
        "| PD-06 | RELEASE-R1 | candidate | PROFILE-D1 | base |",
        1,
    )
    expect_phase_dependency_failure(
        "duplicate phase-dependency tuple",
        duplicate_phase_dependency,
        nodes,
        enforce_manifest=False,
        expected_message="duplicate phase-dependency tuple RELEASE-R1@candidate -> PROFILE-D1@base",
    )

    undefined_phase_endpoint = text.replace(
        "| PD-05 | RELEASE-R1 | candidate | PROFILE-D1 | base |",
        "| PD-05 | RELEASE-R1 | candidate | PROFILE-D1 | base |\n"
        "| PD-06 | REFINE-1 | world | ACCOUNT-1 | missing-phase |",
        1,
    )
    expect_phase_dependency_failure(
        "undefined phase-dependency endpoint",
        undefined_phase_endpoint,
        nodes,
        enforce_manifest=False,
        expected_message="undefined phase endpoints: ACCOUNT-1@missing-phase",
    )

    phase_cycle = text.replace(
        "| PD-05 | RELEASE-R1 | candidate | PROFILE-D1 | base |",
        "| PD-05 | RELEASE-R1 | candidate | PROFILE-D1 | base |\n"
        "| PD-06 | RELEASE-R1 | framework | RELEASE-R1 | candidate |",
        1,
    )
    expect_phase_dependency_failure(
        "same-node phases form a real cycle",
        phase_cycle,
        nodes,
        enforce_manifest=False,
        expected_message="phase dependency cycle:",
    )

    conditional_edges = parse_conditional_edges(text, nodes)
    predicate_entailments = parse_predicate_entailments(text, conditional_edges)

    in_place_upgrade_active = frozenset(
        edge.edge_id
        for edge in active_conditional_edges(
            conditional_edges,
            predicate_entailments,
            {
                "launch-profile": set(),
                "target-profile": {"in-place-upgrade"},
            },
        )
    )
    required_upgrade_edges = frozenset({"CE-29", "CE-30", "CE-31", "CE-33", "CE-43", "CE-44"})
    if not required_upgrade_edges.issubset(in_place_upgrade_active):
        raise PlanningError(
            "in-place-upgrade closure lost successor activation edges: "
            f"required={sorted(required_upgrade_edges)}, got={sorted(in_place_upgrade_active)}"
        )

    ce44 = "| CE-44 | TESTNET-1 | testnet | target-profile | in-place-upgrade | SUCCESSOR-ACTIVATION-1 | activation |"
    missing_ce44 = text.replace(ce44 + "\n", "", 1)
    if missing_ce44 == text:
        raise PlanningError("self-test fixture drift: CE-44")
    expect_conditional_failure(
        "testnet loses successor activation edge",
        missing_ce44,
        nodes,
        expected_message="conditional edge inventory changed without updating its intentional guard:",
    )

    successor_active = frozenset(
        edge.edge_id
        for edge in active_conditional_edges(
            conditional_edges,
            predicate_entailments,
            {
                "launch-profile": set(),
                "target-profile": {"post-genesis-program"},
            },
        )
    )
    expected_successor_active = frozenset(
        {
            "CE-02", "CE-05", "CE-11", "CE-17", "CE-20", "CE-26",
            "CE-29", "CE-30", "CE-31", "CE-32", "CE-43",
        }
    )
    if successor_active != expected_successor_active:
        raise PlanningError(
            "successor predicate-context assay changed: "
            f"expected={sorted(expected_successor_active)}, got={sorted(successor_active)}"
        )

    collapsed_context_active = frozenset(
        edge.edge_id
        for edge in active_conditional_edges(
            conditional_edges,
            predicate_entailments,
            {
                "launch-profile": {"post-genesis-program"},
                "target-profile": {"post-genesis-program"},
            },
        )
    )
    if not {"CE-07", "CE-23"}.issubset(collapsed_context_active):
        raise PlanningError(
            "self-test fixture drift: collapsed successor contexts no longer falsely activate "
            "launch-only program edges"
        )

    initial_active = frozenset(
        edge.edge_id
        for edge in active_conditional_edges(
            conditional_edges,
            predicate_entailments,
            {
                "launch-profile": {"program-active"},
                "target-profile": {"program-active"},
            },
        )
    )
    expected_initial_active = frozenset(
        {
            "CE-02", "CE-05", "CE-07", "CE-11", "CE-17",
            "CE-20", "CE-23", "CE-26", "CE-43",
        }
    )
    if initial_active != expected_initial_active:
        raise PlanningError(
            "initial predicate-context assay changed: "
            f"expected={sorted(expected_initial_active)}, got={sorted(initial_active)}"
        )

    verkle_active = frozenset(
        edge.edge_id
        for edge in active_conditional_edges(
            conditional_edges,
            predicate_entailments,
            {
                "launch-profile": set(),
                "target-profile": {"verkle-selected"},
            },
        )
    )
    expected_verkle_active = frozenset(
        {*(f"CE-{number:02d}" for number in range(35, 43)), "CE-43"}
    )
    if verkle_active != expected_verkle_active:
        raise PlanningError(
            "selected-Verkle closure assay changed: "
            f"expected={sorted(expected_verkle_active)}, got={sorted(verkle_active)}"
        )

    missing_conditional = text.replace(
        "| CE-01 | PROFILE-SCOPE-D0 | candidate-scope | target-profile | asset-active | ASSET-D1 | design |\n",
        "",
        1,
    )
    if missing_conditional == text:
        raise PlanningError("self-test fixture drift: CE-01")
    expect_conditional_failure(
        "missing conditional edge",
        missing_conditional,
        nodes,
        expected_message="conditional edge inventory changed without updating its intentional guard:",
    )

    wrong_polarity = text.replace(
        "| CE-30 | UPGRADE-1 | migration | target-profile | migration-profile | PROFILE-1 | old-admitted-profile |",
        "| CE-30 | UPGRADE-1 | migration | target-profile | migration-profile | PROFILE-1 | new-candidate-profile |",
        1,
    )
    if wrong_polarity == text:
        raise PlanningError("self-test fixture drift: upgrade polarity")
    expect_conditional_failure(
        "changed old/new profile polarity",
        wrong_polarity,
        nodes,
        expected_message="conditional roles, predicates, or polarity changed without updating their guard:",
    )

    launch_edge = "| CE-07 | GENESIS-D1 | genesis-bytes | launch-profile | program-active | VM-D1 | design |"
    launch_edge_in_target_context = text.replace(
        launch_edge,
        "| CE-07 | GENESIS-D1 | genesis-bytes | target-profile | program-active | VM-D1 | design |",
        1,
    )
    if launch_edge_in_target_context == text:
        raise PlanningError("self-test fixture drift: CE-07 launch context")
    expect_conditional_failure(
        "launch-only edge moved into successor target context",
        launch_edge_in_target_context,
        nodes,
        expected_message="launch-profile conditional edge set changed:",
    )

    target_edge = "| CE-17 | EXECUTOR-1 | profile-executor | target-profile | program-active | PROGRAM-STATE-1 | implementation |"
    target_edge_in_launch_context = text.replace(
        target_edge,
        "| CE-17 | EXECUTOR-1 | profile-executor | launch-profile | program-active | PROGRAM-STATE-1 | implementation |",
        1,
    )
    if target_edge_in_launch_context == text:
        raise PlanningError("self-test fixture drift: CE-17 target context")
    expect_conditional_failure(
        "successor target edge moved into immutable launch context",
        target_edge_in_launch_context,
        nodes,
        expected_message="launch-profile conditional edge set changed:",
    )

    for number in range(35, 43):
        edge_id = f"CE-{number:02d}"
        edge_line = next(
            (line for line in text.splitlines() if line.startswith(f"| {edge_id} |")),
            None,
        )
        if edge_line is None:
            raise PlanningError(f"self-test fixture drift: {edge_id} Verkle closure")
        missing_verkle_edge = text.replace(edge_line + "\n", "", 1)
        expect_conditional_failure(
            f"selected-Verkle closure loses {edge_id}",
            missing_verkle_edge,
            nodes,
            expected_message="conditional edge inventory changed without updating its intentional guard:",
        )

    ce42 = "| CE-42 | PROFILE-1 | new-admission | target-profile | verkle-selected | VERKLE-R1 | prototype |"
    wrong_verkle_admission_predicate = text.replace(
        ce42,
        "| CE-42 | PROFILE-1 | new-admission | target-profile | program-active | VERKLE-R1 | prototype |",
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
        "| CE-42 | PROFILE-1 | old-admitted-profile | target-profile | verkle-selected | VERKLE-R1 | feasibility |",
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

    ce43 = "| CE-43 | PROFILE-D1 | new-candidate-profile | target-profile | always | PROFILE-SCOPE-D0 | candidate-scope |"
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
        "| CE-43 | PROFILE-D1 | new-candidate-profile | target-profile | migration-profile | PROFILE-SCOPE-D0 | candidate-scope |",
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
        "| CE-43 | PROFILE-D1 | candidate-scope | target-profile | always | PROFILE-SCOPE-D0 | base |",
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

    ce40 = "| CE-40 | VERKLE-R1 | prototype | target-profile | verkle-selected | PROFILE-D1 | new-candidate-profile |"
    conditional_cycle = text.replace(
        ce40,
        ce40 + "\n"
        "| CE-45 | PROFILE-D1 | new-candidate-profile | target-profile | migration-profile | UPGRADE-1 | migration |",
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

    pe10 = "| PE-10 | in-place-upgrade | migration-profile |"
    missing_entailment = text.replace(pe10 + "\n", "", 1)
    if missing_entailment == text:
        raise PlanningError("self-test fixture drift: PE-10")
    expect_predicate_entailment_failure(
        "missing predicate entailment",
        missing_entailment,
        nodes,
        expected_message="predicate entailment inventory changed without updating its intentional guard:",
    )

    reversed_entailment = text.replace(
        pe10,
        "| PE-10 | migration-profile | in-place-upgrade |",
        1,
    )
    if reversed_entailment == text:
        raise PlanningError("self-test fixture drift: predicate entailment polarity")
    expect_predicate_entailment_failure(
        "reversed predicate entailment",
        reversed_entailment,
        nodes,
        expected_message="predicate entailments changed without updating their guard:",
    )

    entailment_cycle = text.replace(
        pe10,
        pe10 + "\n| PE-11 | migration-profile | in-place-upgrade |",
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
    expect_frozen_fourth_audit_failure(
        "frozen fourth audit mutation",
        fourth_audit_bytes + b"\nnegative-control",
    )
    expect_frozen_fifth_audit_failure(
        "frozen fifth audit mutation",
        fifth_audit_bytes + b"\nnegative-control",
    )
    expect_frozen_layer1_design_audit_failure(
        "frozen corrected checkpoint-0.3 audit mutation",
        layer1_design_audit_bytes + b"\nnegative-control",
    )
    for finding_id, fragments in EXPECTED_LOOP5_PAPER_FRAGMENTS.items():
        for index, fragment in enumerate(fragments, 1):
            mutated_paper = layer1_design_paper_text.replace(
                fragment,
                f"mutated-{finding_id.lower()}-paper-fragment-{index}",
                1,
            )
            if mutated_paper == layer1_design_paper_text:
                raise PlanningError(
                    f"self-test fixture drift: {finding_id} paper fragment {index}"
                )
            expect_loop5_paper_fragment_failure(
                f"{finding_id} paper fragment mutation {index}",
                mutated_paper,
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
    phase_statuses = parse_phase_statuses(capability_text, nodes)
    phase_dependencies = parse_phase_dependencies(
        capability_text,
        nodes,
        phase_statuses,
    )
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
    fourth_audit_bytes = FOURTH_AUDIT.read_bytes()
    check_frozen_fourth_audit(fourth_audit_bytes)
    fifth_audit_bytes = FIFTH_AUDIT.read_bytes()
    check_frozen_fifth_audit(fifth_audit_bytes)
    layer1_design_audit_bytes = LAYER1_DESIGN_AUDIT.read_bytes()
    check_frozen_layer1_design_audit(layer1_design_audit_bytes)
    design_receipt_bytes = DESIGN_RECEIPT.read_bytes()
    check_frozen_design_receipt_bytes(design_receipt_bytes)
    design_receipt = parse_frozen_design_receipt(design_receipt_bytes)
    check_audited_distillation_receipt(design_receipt)
    design_text = DESIGN_DISTILLATION.read_text(encoding="utf-8")
    check_design_source_receipt(design_text, design_receipt)
    layer1_design_paper_text = (ROOT / "docs" / "LAYER1_DESIGN_PAPER.md").read_text(
        encoding="utf-8"
    )
    check_loop5_paper_fragments(layer1_design_paper_text)
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
            fourth_audit_bytes,
            fifth_audit_bytes,
            layer1_design_audit_bytes,
            design_receipt_bytes,
            design_text,
            layer1_design_paper_text,
            design_receipt,
            tracked_paths,
            frozen_eol_policy,
        )

    base_edge_count = sum(len(node.dependencies) for node in nodes.values())
    edge_count = base_edge_count + len(phase_dependencies) + len(conditional_edges)
    suffix = " with negative controls" if args.self_test else ""
    print(
        f"planning-doc check PASS: {len(nodes)} capability nodes, "
        f"{len(phase_statuses)} phase-status tuples, "
        f"{edge_count} declared base-plus-phase-plus-conditional edges, "
        f"{len(predicate_entailments)} predicate entailments, "
        f"{len(PLANNING_FILES)} Markdown files and 1 frozen source receipt{suffix}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
