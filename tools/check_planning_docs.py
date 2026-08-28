#!/usr/bin/env python3
"""Validate Bitblock planning documents without changing repository state."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
from collections.abc import Callable
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
SIXTH_AUDIT = ROOT / "docs" / "DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP6.md"
SIXTH_AUDIT_GIT_BLOB = "5374b160424b78bd516ffc598e9bbd3b29392227"
SEVENTH_AUDIT = ROOT / "docs" / "DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP7.md"
SEVENTH_AUDIT_GIT_BLOB = "dfff19eddc3ab2d4d36f8b7f4e38c296c26f10c2"
SEVENTH_AUDIT_INPUT_COMMIT = "62afdd24a8525704f5949584859bd7cbad300d4d"
SEVENTH_AUDIT_INPUT_TREE = "ee346fb3aa9bc54591f851dcb482fd92be9da84a"
SEVENTH_AUDIT_ADDITION_COMMIT = "78d5494f930c7412c2d60162b44c0b72050f9f22"
SEVENTH_AUDIT_ADDITION_TREE = "07aa851e422a5da9453d4409f91b62ad9442a7d5"
EIGHTH_AUDIT = ROOT / "docs" / "DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP8.md"
EIGHTH_AUDIT_GIT_BLOB = "b247d7585a9e4be06558b0076511fa1064252049"
EIGHTH_AUDIT_INPUT_COMMIT = "e3650eb5d4ea390ac0023cb67325bae3c5351c60"
EIGHTH_AUDIT_INPUT_TREE = "91378e4fe123a96298d73905ea2b1b3974137791"
EIGHTH_AUDIT_ADDITION_COMMIT = "b9d441097c6c4dce013a3dad71cefb7ec2d5f0bf"
EIGHTH_AUDIT_ADDITION_TREE = "916f7c22304d969d809f2f01f5032602a1a81b3b"
NINTH_AUDIT = ROOT / "docs" / "DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP9.md"
NINTH_AUDIT_GIT_BLOB = "0fa04587f3715c22f3b39017583e32000d8ba871"
NINTH_AUDIT_INPUT_COMMIT = "e6baa331f89d8b367d97ae218ad8506c103eb82c"
NINTH_AUDIT_INPUT_TREE = "044f210a7ae854a9416573a8f326d414da3ad4b5"
NINTH_AUDIT_ADDITION_COMMIT = "bf62cb4848eeb3481eb413112cd33c06a63f1451"
NINTH_AUDIT_ADDITION_TREE = "8f4c14e4926a5d5bcf77147cbfe724e55e234492"
TENTH_AUDIT = ROOT / "docs" / "DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP10.md"
TENTH_AUDIT_GIT_BLOB = "eb730338c6ec00c0285cc3ee7d8245f2c1ea2ebe"
TENTH_AUDIT_INPUT_COMMIT = "c6474e97a65fccee6c02ff07ad7a46bb516b7f9b"
TENTH_AUDIT_INPUT_TREE = "29c72d3a26a72213e399b8846f8b6c24cdaeeed8"
TENTH_AUDIT_ADDITION_COMMIT = "1f7ec0f2ca094414264f5172a4f07fd18a8dc3f3"
TENTH_AUDIT_ADDITION_TREE = "462120e5e33b1e45932e3891ce7e5c43b9a35825"
CHECKED_GIT_REF = "HEAD"
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
    SIXTH_AUDIT,
    SEVENTH_AUDIT,
    EIGHTH_AUDIT,
    NINTH_AUDIT,
    TENTH_AUDIT,
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
        "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP6.md",
        "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP7.md",
        "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP8.md",
        "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP9.md",
        "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP10.md",
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
    SIXTH_AUDIT,
    SEVENTH_AUDIT,
    EIGHTH_AUDIT,
    NINTH_AUDIT,
    TENTH_AUDIT,
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
        "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP6.md",
        "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP7.md",
        "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP8.md",
        "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP9.md",
        "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP10.md",
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
    "4db6e56035837605738015f5d1a2dbe5b1e5ddaa0b323baabd8a80a577214803"
)
EXPECTED_HANDOFF_READ_ORDER = (
    "docs/HANDOFF.md",
    "docs/PROJECT_INTENT.md",
    "docs/CAPABILITY_TREE.md",
    "docs/DESIGN_RESEARCH_DISTILLATION.md",
    "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT.md",
    "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP2.md",
    "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP3.md",
    "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP4.md",
    "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP5.md",
    "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP6.md",
    "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP7.md",
    "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP8.md",
    "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP9.md",
    "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP10.md",
    "docs/FUTURE_PLAN_AUDIT_LOOP2.md",
    "docs/FUTURE_PLAN_AUDIT.md",
    "docs/SEMANTICS.md",
    "docs/TRANSCRIPT_AUDIT.md",
    "docs/AUTHENTICATED_STATE_PLAN.md",
    "docs/LAYER1_DESIGN_PAPER.md",
    "docs/LAYER1_DESIGN_AUDIT.md",
    "README.md",
    "docs/PROTOCOL.md",
    "docs/TIMELINES.md",
)
NODE_ID = r"[A-Z][A-Z0-9-]*"
PHASE_NODE_ID = r"[A-Z0-9]+(?:-[A-Z0-9]+)*"
DETAIL_HEADING = re.compile(rf"^### `({NODE_ID})`(?:\s|$)")
FIELD_BOUNDARY = re.compile(r"^#{1,3}\s")
FIELD = re.compile(r"^- \*\*(Status|Depends on|Motive|Artifact|Gate|Non-claim):\*\*\s*(.*)$")
DEPENDENCY = re.compile(rf"`({NODE_ID})`")
FOUNDATION_ROW = re.compile(rf"^\| `({NODE_ID})` \| ([^|]+) \| ([^|]+) \|")
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
STATUS_PROJECTION_TOKEN = re.compile(
    rf"`(PS-[0-9]{{3}}):({PHASE_NODE_ID})@([a-z0-9]+(?:-[a-z0-9]+)*)="
    r"(IMPLEMENTED|BOUNDED|READY|LOCKED|OPEN-DECISION|NOT-STARTED)`"
)
READY_FRONTIER_HEADER = "| Frontier ordinal | Authoritative READY projection |"
READY_FRONTIER_SEPARATOR = "| ---: | --- |"
READY_FRONTIER_ROW = re.compile(
    rf"^\| ([0-9]+) \| `(PS-[0-9]{{3}}):({PHASE_NODE_ID})@"
    r"([a-z0-9]+(?:-[a-z0-9]+)*)="
    r"(IMPLEMENTED|BOUNDED|READY|LOCKED|OPEN-DECISION|NOT-STARTED)` \|$"
)
HANDOFF_READ_ORDER_ROW = re.compile(r"^([0-9]+)\. `([^`]+)`$")
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
    "ab4fcdaa8167a306e34d3b8de89b518ef018c733cfbcddf0df00112ab842a148"
)
EXPECTED_PHASE_STATUS_IDS = frozenset(f"PS-{number:03d}" for number in range(1, 96))
EXPECTED_PHASE_STATUS_DIGEST = (
    "8f368d8caa0da3ba290102c2c564d85fe9ef5d457e7eb2ab7fda9b457ac99215"
)
EXPECTED_PHASE_DEPENDENCY_IDS = frozenset(f"PD-{number:02d}" for number in range(1, 8))
EXPECTED_PHASE_DEPENDENCY_DIGEST = (
    "20fb7a3fa1de1a30a5a1034f73bd204b7d522e971a4bb978a5e1ed0277e3ce3f"
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
        "CONSENSUS-1",
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
    "`LaunchClosureExternalGenesisDependencyNodeV1`",
    "combined-graph node owns dependency refs",
    "second copy of the outer genesis bytes",
    "entries and retrieval locators are not members of this carrier",
)
EXPECTED_SUCCESSOR_ACTIVATION_ARTIFACT_FRAGMENTS = (
    "`SUCCESSOR-ACTIVATION-1@old-profile-id->new-profile-id`",
    "`SuccessorActivationBundleV1`",
    "`successor_activation_bundle_id`",
    "exact target `ProfileAdmissionManifestV1`",
    "concrete `UPGRADE-1` evidence",
    "carries the complete bundle before migration execution or activation",
    "`VerifiedSuccessorPredecessorBaseContextV1`",
    "`VerifiedSuccessorReleaseAuthorityContextV1`",
    "`VerifiedSuccessorActivationAuthorityContextV1`",
    "`ParentAuthorityCertificateConsumerContextV1`",
    "`ConsensusNativeAcceptedRecordOccurrenceReceiptSetV1`",
    "`AcceptedParentAuthorityWitnessReceiptCoverageV1`",
    "internal-only bundle",
    "that are neither serialized nor",
)
EXPECTED_CONSENSUS_AUTHORITY_ARTIFACT_FRAGMENTS = (
    "owns live `AcceptedConsensusNativeParentAuthorityCertificateV1` issuance/admission",
    "exactly one accepted admission per `(parent core, purpose, required position)`",
    "consumes live accepted records directly; no replay checkpoint or normalized archive view selects admission",
)
EXPECTED_SYNC_AUTHORITY_ARTIFACT_FRAGMENTS = (
    "base phase supplies role-separated full verifier, archive, light client, and non- authority RPC paths",
    "transports and retains opaque bounded authority-certificate and occurrence bytes but does not admit or interpret a consensus-native certificate",
    "After `CONSENSUS-1@base`, the separate replay-authority phase additionally carries ordered `ReplayEvidenceEventV1` values",
    "consumer-bound `HistoricalAcceptedParentAuthorityUseWitnessSetV1` values with exact accepted consensus-native certificate admissions",
)
EXPECTED_SYNC_AUTHORITY_GATE_FRAGMENTS = (
    "base phase passes multi-peer eclipse, partial-data, corrupt-snapshot, slow-peer, restart, pruning, and fresh-node recovery tests under declared retention assumptions without using replayed authority as live consensus input",
    "replay-authority phase independently reconstructs the already accepted certificate protocol owned by `CONSENSUS-1`",
    "A replay checkpoint created after the consumer is a required positive control because archive order is not live authority",
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
EXPECTED_LOOP6_PAPER_FRAGMENTS = {
    "RG-01": (
        "AcceptedProposalCoreV1 = FixedPreProfileCanonicalV1(",
        "AcceptedEvidenceSetV1 = CanonicalSortedUniqueByTypedEvidenceId([",
        "AcceptedParentAuthorityRefV1 = Canonical(",
        "ParentAuthorityEligibilityV1(core, use, C, E) = Eligible",
        "Powered evidence/parent-index controls MUST include:",
    ),
    "RG-02": (
        "This creates three explicitly different conditions. `PreventiveProfileOwnedCryptographicMigrationV1`",
        "`ProtocolIdentityHashCompromiseV1` is therefore terminal safe halt",
        "A demonstrated collision, practical second-preimage break",
        "safe-halts and requires a new genesis under a",
    ),
    "RG-03": (
        "ExternalGenesisIncomingDependencyClassV1 =",
        "LaunchClosureExternalGenesisDependencyNodeV1 = Canonical(",
        "external_root_count_u32                 = 1",
        "Any second external root, other external",
        "mismatch the outer/root bytes or ID",
    ),
}
EXPECTED_LOOP7_PAPER_FRAGMENTS = {
    "LG-01": (
        "TypedProposalCredentialIdV1 = ProposalCredentialIdV1(",
        "AcceptedProposalCredentialSetV1 = CanonicalSortedUniqueByTypedProposalCredentialId([",
        "additional valid credentials have exactly zero proposal, vote, eligibility, or fork-choice weight",
    ),
    "LG-02": (
        "This creates three explicitly different conditions. `PreventiveProfileOwnedCryptographicMigrationV1`",
        "`ProfileOwnedAuthorityBindingCompromiseV1` is instead an established collision",
        "PrecommittedIndependentRecoveryEnvelopeV1 = ReservedFutureUninhabitedTypeV1",
    ),
    "LG-03": (
        "VerifiedConsensusEntailedAvailabilityDeltaV1 = Canonical(",
        "    SameWrapperConsensusDependencyWitnessV1(",
        "consensus entry and availability-assessment entry atomically or inserts neither",
    ),
    "LG-04": (
        "ReplayEvidenceCutoffV1 = Canonical(",
        "NormalizedAcceptedEvidenceSnapshotV1 = Canonical(",
        "RetainedAcceptedEligibilityWitnessV1 = Canonical(",
        "parent-ordered unique AcceptedCoreReplayEntryV1 sequence",
    ),
    "LG-05": (
        "LaunchClosureExternalGenesisDependencyNodeV1 = Canonical(",
        "  | GenesisInitialDomainRegistryDependencyV1",
        "internal_to_internal_dependency_edge_count_u32 = Length(InternalRefs(D))",
        "internal_to_external_dependency_edge_count_u32 = Length(ExternalGenesisRefs(D))",
        "external_to_internal_dependency_edge_count_u32 = Length(GenesisInternalRefs(X))",
    ),
    "LG-06": (
        "VerifiedSuccessorPredecessorBaseContextV1 = SemanticVerifierResult(",
        "VerifiedSuccessorReleaseAuthorityContextV1 = SemanticVerifierResult(",
        "VerifiedSuccessorActivationAuthorityContextV1 = SemanticVerifierResult(",
        "SuccessorActivationInternalDependencyRefV1 =",
        "SuccessorActivationInternalDependencyNodeV1 = Canonical(",
        "internal_artifact_count_u32                       = 7 + Length(S_s) = Length(D_s)",
    ),
}
EXPECTED_LOOP8_PAPER_FRAGMENTS = {
    "EG-02": (
        "ReplayEvidenceViewV1",
        "ReplayEvidenceArchiveCheckpointV1",
        "ReplayEvidenceEventInclusionProofV1",
        "ReplayEvidencePrefixCompletenessProofV1",
        "SameWrapperAcceptedRecordOccurrenceWitnessV1",
        "NormalizedAcceptedAuthorityEntryCoreV1 = Canonical(",
        "normalized_authority_entry_id = ReplayEvidenceArchiveHashV1(",
        "normalized_authority_view_root = ReplayEvidenceArchiveHashV1(",
        "ReplayEvidenceCheckpointConsistencyProofV1 = Canonical(",
    ),
    "EG-03": (
        "ReplayParentAuthorityStatusProjectionV1",
        "it never selects live authority",
    ),
    "EG-04": (
        "CurrentEstablishedAuthorityCompromiseDispositionV1 =",
        "PrecommittedIndependentRecoveryEnvelopeV1 = ReservedFutureUninhabitedTypeV1",
    ),
    "EG-05": (
        "AcceptedProposalCoreFieldManifestV1 = ExactOrderedFieldManifest(",
        "AcceptedProposalCoreExcludedFieldManifestV1 = ExactForbiddenFieldManifest(",
        "AcceptedParentCollisionDispositionV1 = CanonicalTwoRowMap(",
    ),
}

EXPECTED_LOOP8_EXACT_SCHEMA_VARIANTS = {
    "VerifiedSuccessorPredecessorBaseContextV1": (
        (
            "SemanticVerifierResult",
            (
                "shared_predecessor_accepted_parent_lookup_id",
                "shared_predecessor_core_block_id",
                "release_parent_authority_ref=AcceptedParentAuthorityRefV1(SuccessorReleaseAuthorityV1)",
                "verified_release_parent_eligibility=VerifiedEligibleAcceptedParentRefV1(SuccessorReleaseAuthorityV1)",
                "release_parent_authority_status",
                "activation_parent_authority_ref=AcceptedParentAuthorityRefV1(SuccessorActivationAuthorityV1)",
                "verified_activation_parent_eligibility=VerifiedEligibleAcceptedParentRefV1(SuccessorActivationAuthorityV1)",
                "activation_parent_authority_status",
                "predecessor_admitted_profile_id",
                "old_active_profile_context",
                "genesis_id",
                "chain_launch_manifest_id",
                "predecessor_governance_authority_root",
                "activation_old_execution_root_ref",
            ),
        ),
    ),
    "ParentAuthorityCertificateConsumerContextV1": (
        (
            "EnvelopeChildParentAuthorityConsumerContextV1",
            (
                "chain_launch_manifest_id",
                "child_profile_transition_context",
                "child_world_id",
                "child_height",
                "child_epoch",
                "child_round_or_view",
                "child_parent_block_id",
                "child_consensus_policy_id",
            ),
        ),
        (
            "SuccessorReleaseParentAuthorityConsumerContextV1",
            (
                "admitted_release_core_id",
                "release_authority_context_ref",
                "release_authority_policy_id",
            ),
        ),
        (
            "SuccessorActivationParentAuthorityConsumerContextV1",
            (
                "chain_launch_manifest_id",
                "predecessor_accepted_parent_authority_ref",
                "old_active_profile_context",
                "new_admitted_profile_context",
                "successor_profile_admission_manifest_id",
                "migration_spec_id",
                "successor_activation_bundle_id",
                "activation_height",
                "activation_authority_policy_id",
            ),
        ),
    ),
    "ConsensusNativeAcceptedRecordOccurrenceReceiptV1": (
        (
            "Canonical",
            (
                "accepted_record_occurrence_receipt_schema_version",
                "accepted_parent_lookup_id",
                "unique_accepted_proposal_core_block_id",
                "canonical_accepted_record_outer_length_u64",
                "canonical_accepted_record_outer_bytes",
                "typed_proposal_credential_id",
                "ConsensusEvidenceIdV1",
                "AvailabilityAssessmentIdV1",
            ),
        ),
    ),
    "AcceptedParentAuthorityReceiptComponentProjectionV1": (
        (
            "Canonical",
            (
                "consensus_native_accepted_record_occurrence_receipt_id",
                "proposal_credential_component=ProposalCredentialCommittedWitnessIdV1(TypedProposalCredentialIdV1)",
                "consensus_evidence_component=AcceptedEvidenceCommittedWitnessIdV1(ConsensusEvidenceIdV1)",
                "availability_assessment_component=AcceptedEvidenceCommittedWitnessIdV1(AvailabilityAssessmentIdV1)",
                "availability_dependency_component=EvidenceDependencyCommittedWitnessIdV1(AcceptedEvidenceDependencyWitnessIdV1)",
            ),
        ),
    ),
    "AcceptedParentAuthorityWitnessReceiptCoverageV1": (
        (
            "Canonical",
            (
                "CanonicalSortedUniqueByConsensusNativeAcceptedRecordOccurrenceReceiptId([AcceptedParentAuthorityReceiptComponentProjectionV1...])",
            ),
        ),
    ),
    "AcceptedParentAuthorityEvidenceCommitmentV1": (
        (
            "Canonical",
            (
                "canonical_qualifying_proposal_credential_witness_set",
                "canonical_receipt_component_total_dependency_closed_evidence_set",
                "canonical_dependency_closure_witness_manifest",
                "ConsensusNativeAcceptedRecordOccurrenceReceiptSetV1",
                "AcceptedParentAuthorityWitnessReceiptCoverageV1",
            ),
        ),
    ),
    "ConsensusNativeParentAuthorityCertificateAdmissionCoreV1": (
        (
            "Canonical",
            (
                "consensus_native_parent_authority_certificate_id",
                "consensus_native_parent_authority_certificate_authorization_id",
                "parent_authority_certificate_consumer_context_id",
                "accepted_parent_authority_evidence_commitment_id",
                "ConsensusNativeParentAuthorityPositionV1",
                "certificate_consensus_policy_id",
                "certificate_availability_policy_id",
                "required_certificate_acceptance_status",
            ),
        ),
    ),
    "ConsensusNativeParentAuthorityCertificateAdmissionV1": (
        (
            "Canonical",
            (
                "AuthorizedConsensusNativeParentAuthorityCertificateV1",
                "ConsensusNativeParentAuthorityCertificateAdmissionCoreV1",
                "consensus_native_parent_authority_certificate_admission_core_id",
                "canonical_certificate_consensus_evidence_length_u64",
                "canonical_certificate_consensus_evidence_bytes",
                "certificate_consensus_evidence_id",
                "canonical_certificate_availability_assessment_length_u64",
                "canonical_certificate_availability_assessment_bytes",
                "certificate_availability_assessment_id",
                "attained_certificate_acceptance_status",
            ),
        ),
    ),
    "consensus_native_parent_authority_certificate_admission_id": (
        (
            "EnvelopeHashV1",
            (
                "OrdinaryProfileTransitionV1(parent_profile_context)",
                "parent_authority_certificate_admission_domain",
                "ConsensusNativeParentAuthorityCertificateAdmissionV1",
            ),
        ),
    ),
    "AcceptedConsensusNativeParentAuthorityCertificateV1": (
        (
            "Canonical",
            (
                "ConsensusNativeParentAuthorityCertificateAdmissionV1",
                "consensus_native_parent_authority_certificate_admission_id",
            ),
        ),
    ),
    "ConsensusNativeParentAuthorityCertificateRefV1": (
        (
            "Canonical",
            (
                "consensus_native_parent_authority_certificate_id",
                "consensus_native_parent_authority_certificate_authorization_id",
                "consensus_native_parent_authority_certificate_admission_id",
                "parent_authority_certificate_consumer_context_id",
                "accepted_parent_authority_evidence_commitment_id",
                "ConsensusNativeParentAuthorityPositionV1",
            ),
        ),
    ),
    "HistoricalAcceptedParentAuthorityUseRefV1": (
        (
            "Canonical",
            (
                "AcceptedParentAuthorityRefV1",
                "unique_accepted_proposal_core_block_id",
                "ConsensusNativeParentAuthorityCertificateRefV1",
                "attained_parent_authority_status",
            ),
        ),
    ),
    "AcceptedParentAuthorityConsumerRefV1": (
        (
            "EnvelopeChildAuthorityConsumerV1",
            (
                "child_accepted_parent_lookup_id",
                "child_block_id",
            ),
        ),
        (
            "SuccessorReleaseAuthorityConsumerV1",
            ("release_sign_doc_id",),
        ),
        (
            "SuccessorActivationAuthorityConsumerV1",
            ("successor_activation_sign_doc_id",),
        ),
    ),
    "AcceptedConsensusNativeParentAuthorityCertificateOccurrenceV1": (
        (
            "Canonical",
            (
                "accepted_parent_authority_certificate_occurrence_schema_version",
                "consensus_native_parent_authority_certificate_id",
                "consensus_native_parent_authority_certificate_authorization_id",
                "consensus_native_parent_authority_certificate_admission_id",
                "accepted_parent_authority_evidence_commitment_id",
                "canonical_accepted_parent_authority_certificate_length_u64",
                "canonical_accepted_parent_authority_certificate_bytes",
            ),
        ),
    ),
    "ReplayEvidenceEventV1": (
        (
            "AcceptedRecordOuterOccurrenceEventV1",
            (
                "AcceptedRecordOuterOccurrenceV1",
                "accepted_record_outer_occurrence_id",
            ),
        ),
        (
            "AcceptedParentAuthorityUseOccurrenceEventV1",
            (
                "AcceptedParentAuthorityUseV1",
                "AcceptedParentAuthorityConsumerRefV1",
                "HistoricalAcceptedParentAuthorityUseRefV1",
                "AcceptedConsensusNativeParentAuthorityCertificateOccurrenceV1",
                "accepted_parent_authority_certificate_occurrence_id",
                "canonical_authority_consumer_preimage_length_u64",
                "canonical_authority_consumer_preimage_bytes",
            ),
        ),
    ),
    "CertificateEvidenceArchiveOccurrenceWitnessV1": (
        (
            "Canonical",
            (
                "CertificateEvidenceReplayCheckpointRefV1",
                "replay_evidence_event_id",
                "accepted_record_outer_occurrence_id",
                "accepted_parent_lookup_id",
                "unique_accepted_proposal_core_block_id",
                "consensus_native_accepted_record_occurrence_receipt_id",
                "typed_proposal_credential_id",
                "ConsensusEvidenceIdV1",
                "AvailabilityAssessmentIdV1",
                "ReplayEvidenceEventInclusionProofV1",
            ),
        ),
    ),
    "CertificateEvidenceArchiveOccurrenceWitnessSetV1": (
        (
            "CanonicalSortedUniqueByAcceptedRecordOuterOccurrenceId",
            ("[CertificateEvidenceArchiveOccurrenceWitnessV1...]",),
        ),
    ),
    "ConsensusToReplayParentAuthorityCommitmentProofV1": (
        (
            "Canonical",
            (
                "consensus_native_parent_authority_certificate_admission_id",
                "accepted_parent_authority_evidence_commitment_id",
                "CertificateEvidenceReplayCheckpointRefV1",
                "AcceptedParentAuthorityRefV1",
                "CertificateEvidenceArchiveOccurrenceWitnessSetV1",
                "canonical_certificate_commitment_to_occurrence_reconstruction_map",
                "canonical_certificate_dependency_closure_recomputation_trace",
                "recomputed_certificate_parent_authority_status",
            ),
        ),
    ),
    "HistoricalAcceptedParentAuthorityUseWitnessV1": (
        (
            "Canonical",
            (
                "HistoricalAcceptedParentAuthorityUseRefV1",
                "AcceptedParentAuthorityConsumerRefV1",
                "AcceptedConsensusNativeParentAuthorityCertificateV1",
                "accepted_parent_authority_certificate_occurrence_id",
                "replay_evidence_view_id",
                "certificate_evidence_replay_checkpoint=ReplayEvidenceArchiveCheckpointV1",
                "ReplayEvidenceCheckpointConsistencyProofV1",
                "ConsensusToReplayParentAuthorityCommitmentProofV1",
                "authority_consumer_event_ordinal_u64",
                "certificate_evidence_prefix_completeness_proof=ReplayEvidencePrefixCompletenessProofV1",
                "authority_consumer_event_inclusion_proof=ReplayEvidenceEventInclusionProofV1",
                "canonical_authority_consumer_preimage_length_u64",
                "canonical_authority_consumer_preimage_bytes",
                "canonical_qualifying_proposal_credential_preimages",
                "canonical_dependency_closed_evidence_preimages",
            ),
        ),
    ),
}

# Loop 9 freezes the complete parsed assignment surface spanning live
# consensus-native occurrence/certificate admission through replay
# checkpoint/view/completeness reconstruction.  Each digest is over compact
# JSON of ``_parse_exact_schema_assignment``'s ordered variant/field result;
# it therefore binds variant order, field order, domain strings, and context
# arguments while keeping this already-large checker manifest reviewable.
EXPECTED_LOOP9_EXACT_ASSIGNMENT_SHA256 = {
    "AcceptedParentAuthorityUseV1": "b017ba1a930b313587e1490f9f3b78e366a1eefc6ba99eb69f0cda788775a7fd",
    "AcceptedParentAuthorityRefV1": "f5837a033dc9a840a2240481a880733749eb5433283166a46169a0d49439d90e",
    "ParentAuthorityCertificateConsumerContextV1": "04cd61c45dc7cca94ac051eb27a10e6ff9132ae66149480511795ff2fc601960",
    "ParentAuthorityCertificateConsumerContextCurrentChainForbiddenFieldManifestV1": "16357d39c54226affc873fb085c6e7aef717617ded6d3269310e3050bd570d8e",
    "EnvelopeMigrationConsumerContextPriorAuthorizationAllowedPathManifestV1": "9c650bb2dbf82c65e9e7a13e8454f2d8a13e430ff4d8064dc542d405b203476c",
    "VerifiedParentAuthorityCertificateConsumerContextFieldGuardV1": "c09a4a460fbf3bdbdf12e97ef0180dd98ff651fc2801cc6178061082f59d0497",
    "parent_authority_certificate_consumer_context_id": "4b143ecd090383530c005f1cae579b30c9161bee9df2879bcefa09e866cb73cd",
    "EnvelopeOwnerProfileContextV1(profile_transition_context)": "5e258adb3ad7f617f95a12027ad1485a08b8d481a7c69e053fca166dcf0b26a4",
    "EnvelopeResolutionV1(profile_transition_context)": "7c1dfcc9a943043b2a9cee74455b801b14420e6b02ba7f42e0956e2e6d2d7995",
    "EnvelopeHashV1(profile_transition_context, object_domain, payload)": "ab03013956bd120392e3213f4b835378cbba8e1eb000eb43403bd391b19aa90c",
    "EnvelopeCommitOrderedV1(profile_transition_context, sequence_domain, sequence)": "bd1aebdc8b2c255da19a602506516e22d061b6ed59a3407fc73916a28031f50e",
    "ConsensusNativeParentAuthorityPositionV1": "1dbb4167f75483c23958d0aedee2aecbc5ae136f3b7d7450d69ca3decea0a62a",
    "ConsensusNativeParentAuthorityEvidenceCutoffPositionV1": "3b5f50dc732f922e36e16f203d373c698bb5a268f418cc1353ad5737652effc5",
    "ConsensusNativeAcceptedRecordOccurrencePositionV1": "4f03afe01a52b1e73d269af2f42a1ccc6bfaa789f7cb26cd67166ad42792119c",
    "ConsensusNativeAcceptedRecordOccurrenceHistoryStartPositionV1": "79b4c15104e650a65f41e52e0b88a9c1fa1668f7524ad648b6064471ad896d62",
    "ConsensusNativeAcceptedRecordOccurrenceHistoryStartV1": "a956412757781c0dfce09a41d375b86dca5359eade5aa07cf2d633bce09b2534",
    "VerifiedConsensusNativeAcceptedRecordOccurrenceHistoryStartEqualityV1": "2217aeb6969803e5e1e7ef231ef6c0e7659dad885d09a737d75fa9d60347c512",
    "ConsensusNativeAcceptedRecordOccurrenceIndexKeyV1": "022ad5bb7d9e62d6830d403a747145e5b887d9fea838f7b4bcfa42b1c5456c83",
    "ConsensusNativeAcceptedRecordOccurrenceReceiptV1": "1dcaff1b2403caae442f523b22b52b3bcb7f54ac720f5bb5b0261b6f8be67b11",
    "consensus_native_accepted_record_occurrence_receipt_id": "1fdcaf77b471f81160bb23e2c55e66a5ea75c3c77e3b7ee1fc5a558df1cd1689",
    "ConsensusNativeAcceptedRecordOccurrenceReceiptSetV1": "2701756b147a9da14125e459b7cadd5e15ebc55f41927ff0882da1f2a48cdaa7",
    "consensus_native_accepted_record_occurrence_receipt_set_id": "9afc9d2514f13ea9b4c13a367d6baf0e8389ca457d80905dfc0ec91e9c4d32c3",
    "ConsensusNativeAcceptedRecordOccurrenceUniverseEntryV1": "137396aa74b1aa9d05ce0e72ea41ba1bdc0d468cd13a9c4e605e6c1f4d731237",
    "consensus_native_accepted_record_occurrence_universe_root": "c6b1bbf73f1fbfa18b858e3cbd9c3d1af5bb70737d8e7e09e9a611021f83cb4f",
    "ConsensusNativeParentAuthorityEvidenceCutoffV1": "e10d0a72f983b142a211564aeb91ad59b4b9ee85571ec3917736a94516add009",
    "consensus_native_parent_authority_evidence_cutoff_id": "67779a0be6f4075f4f76dd234e57e78728caadc12d92b027b30feaf07dbc26e0",
    "ConsensusNativeParentAuthorityEvidenceCutoffConsensusProofV1": "9ba382c376cf78b9e58d9297ca209f763363c633cbb66ae44a71622374467d57",
    "ConsensusNativeAcceptedRecordOccurrenceUniverseScopeV1": "6a2b9ccee0082416c0e8d8b9d75e42f04b63c044809d39309c28bd79cd2bdb7c",
    "ConsensusNativeAcceptedRecordOccurrenceUniverseRangeCompletenessProofV1": "11d0c7b75afc0249f9e69e8125fa21bea6778f34074b361cca9cf721290fefb9",
    "CompleteConsensusNativeAcceptedRecordOccurrenceUniverseV1": "883e92729dde0369f90aee2caf45d0e79a55f05118b297564fdfb156a0cd41f4",
    "AcceptedParentAuthorityReceiptComponentProjectionV1": "132315b5f1bac44aaedd26bae68a8034da6cf54f9dea094e2d7fd6c74ffc54e7",
    "AcceptedParentAuthorityWitnessReceiptCoverageV1": "731ce5e41701dce1bf55dbb7bd98c863c2ca00ba9587f6d77337c8d87870a518",
    "AcceptedParentAuthorityEvidenceCommitmentV1": "be3836894333169e115978d3505b88cada7d960c66047027208392359096f78e",
    "accepted_parent_authority_evidence_commitment_id": "8d1f29ac7bbdc9f80d9d444f7e5025cc9183256f7aa0c2690bf93fe7064d3479",
    "ConsensusNativeParentAuthorityCertificateCoreV1": "ccc93bfdb45c7475976b6ad455b9d3402262e6394a9648c5523d143a3ad40100",
    "consensus_native_parent_authority_certificate_id": "9191f98293e1923cc3667771ce36e8eefe4972a165b35ab7bbd1b814c8ee7df6",
    "ConsensusNativeParentAuthorityCertificateSignDocV1": "9b41c1b7b965749262bdd89c7e32f3a0e987b5a9ea2eeea7748739aabc938728",
    "ConsensusNativeParentAuthorityCertificateAuthorizationV1": "8202df8c4d3f7eae69dbd664ee1cc1285f94aa720314c8f01fd2212a12f06826",
    "consensus_native_parent_authority_certificate_authorization_id": "e20f5ee825e060ab83c74ee1f669251f7ae44ce94140de2790211b7d773cd44d",
    "AuthorizedConsensusNativeParentAuthorityCertificateV1": "b7eb434c24cc9af7bc0151be2649b6dcdcd3557943a58e7c718115e283583286",
    "ConsensusNativeParentAuthorityCertificateAdmissionCoreV1": "ad353b1e046aeb98c3ccb308fdbf9da016cf9587d24d336dd1c919d04984edc0",
    "consensus_native_parent_authority_certificate_admission_core_id": "9794e98919ae19e8645715ee09810fd9924619722600164c6a45b04fcfc4fcf9",
    "ConsensusNativeParentAuthorityCertificateAdmissionV1": "6df2ec1859ace912960e2519b1611f212b88d09524fb304b009ac3f0d22f9b6e",
    "consensus_native_parent_authority_certificate_admission_id": "1b99fa39a9f06d361097c1a9cf85e1e29d7f8e2845b6866e6e5abc6984934fd5",
    "AcceptedConsensusNativeParentAuthorityCertificateV1": "88a703f9930ccd5108eaa7362a24d66a33048bcec4bb064d0d624637a697b8e9",
    "ConsensusNativeParentAuthorityCertificateRefV1": "e5f6cb605766a9d56a631b449ead85062ab90bde54a80f9ae993b2d9cb70a7dc",
    "CertificateEvidenceReplayCheckpointRefV1": "be45e8083985dcc53631c4dac26c2b547a13d5a09f46ecf1dbb2c23fceded6b5",
    "HistoricalAcceptedParentAuthorityUseRefV1": "1079b7564c734f2167f55de4ad672ef962ac96cfb61fbafda38e360c6895c593",
    "ReplayEvidenceArchiveResolutionV1": "b0ead3701d205cd4a564dcdd8f37527d4f3ded9de29851e48a26d598a2ec52ce",
    "ReplayEvidenceArchiveHashV1(framed_input)": "a940ab4dd84a6021cb38a1bdab7adaa68a7eac0aebb7ce34a9cee32a0b2fa50e",
    "AcceptedRecordOuterOccurrenceV1": "d5df82b6ae2b792b4118c8361fc255059b74aac933e02a182af09b5b02618570",
    "accepted_record_outer_occurrence_id": "f83e80378d6b68d12e1a8fab46a58f53150a44817d5bd9a08174f66906cf7837",
    "AcceptedParentAuthorityConsumerRefV1": "e47e2b9e3539b967f0c1570a5e215669142f455c48644ff0cbdb3c3e80807ab6",
    "AcceptedConsensusNativeParentAuthorityCertificateOccurrenceV1": "2ee50de59bf5c26c34bdfeb4ec8701bc8a139e9888264a707cc4ed5a7dc469b4",
    "accepted_parent_authority_certificate_occurrence_id": "536b5983560549ead80a222015e156b7d4606680e6877f5ece1dc04c7c6939a7",
    "ReplayEvidenceEventV1": "f207cea22f1cc0be6578228f49ff768593be16c107e211dee1f8e0a1ae59fab2",
    "ReplayEvidenceEventRecordV1": "8dfa117554bea34acf4dbf7a9d6c0d82c162fc90900a4c515e95c7f46a443e7d",
    "replay_evidence_event_id": "524ca4fccb503ddb6d3b5751eebb6ba175c92512bda3d8d21491237e18e29767",
    "ReplayParentAuthorityStatusProjectionCoreV1": "1746d96494ebaa023f6ba83c373dd0830cf75c379e15edb7c9ce5ad4f3f42ec2",
    "ReplayParentAuthorityStatusProjectionCoreSetV1": "3f12ce78d627fcef076f08f9a4b28a9ca841863f0c1bbec13077e111b8193ef4",
    "NormalizedAcceptedAuthorityEntryCoreV1": "b0919ed66902dee9204986eb377939a93df3aacbe89065661ec0fbb6cb9dcee1",
    "normalized_authority_entry_id": "f61d78613e52da307d588d21117ab43eff4894740bf60b7a1f18b6eda12ad738",
    "NormalizedAuthorityViewEntryV1": "2c7b4b2a36bb396a59360f6c013d0152a0eeba1f7d4542782ad5910b63f92495",
    "NormalizedAuthorityViewEntrySetV1": "cf134fc6e2c9ca167f19d985b80f041e89d1c29566d7a3eceb0e9953397ab19c",
    "normalized_authority_view_root": "a3b931f316ef811d022410ac948e81bf60b53c86c98f8934cb0db5ea4376a3b4",
    "ReplayEvidenceArchiveCheckpointCoreV1": "36c5921855edf750f36a1e67dada271e449fb16f7f830b51429b1c781c48d9ab",
    "replay_evidence_archive_checkpoint_id": "cb6b5a374117588a0600e6558e7ab4013981f64d7cc2e15f8265902ea99bc99d",
    "ReplayEvidenceArchiveCheckpointSignDocV1": "bc08977844534726a84c4843f66235fece8be41e0d8397a239c63cc4d1fc152f",
    "ReplayEvidenceArchiveCheckpointAuthorizationV1": "18b3368f08ddcf26c60d3d3d8d4e657ca7763b6a2ca7aa3114ad476a165a14d7",
    "replay_evidence_archive_checkpoint_authorization_id": "63b094d0f11643ee579c10341073e56a40e9e48eb2cbba83e5bc89c3225bc18f",
    "ReplayEvidenceArchiveCheckpointV1": "47c1d0760f0f5ff9fd5ace6443943dda97671c3ee99784909af3e0d4cb23d9ec",
    "ReplayEvidenceCheckpointConsistencyProofV1": "e0e5df4b9cbcc8512c90a6b0a89e6a0d79b8df2bf9d00e31266b33bbfb94fbac",
    "ReplayEvidenceEventInclusionProofV1": "48d7b4231382e9e81b18e07f38f72aaafb19ff9da56420408fa3993ca1f5d3f6",
    "ReplayEvidencePrefixCompletenessProofV1": "3e8025b0dcc31e17bfc5f9696838ba3cb30f8a7c01a58fb98954be61fd650841",
    "ReplayEvidenceCutoffV1": "bf9463783db613b82a7fb8dc4a213cb4c482f621934ded42cca23f5f19c43b30",
    "ReplayEvidenceViewCoreV1": "92fd20e3c6724f78986975d2b7b8d19bda6247e1f41cfc6f2c321f21e33281b6",
    "replay_evidence_view_id": "d3dafb290ff623ec04ef8b6637fb601582ecafc1c9c5ca391819f5a4437bcc70",
    "ReplayEvidenceViewV1": "a802537905faf7ec1f3e309519865891e2aa71a80a4813c874f4f2518f6b1e06",
    "SameWrapperAcceptedRecordOccurrenceWitnessV1": "33bd0661173b165193b314040652d87630f5e2e723a8cb83100213575a7040e9",
    "ReplayEvidenceNormalizedEntryProofV1": "0bd3eb4e9b41eb9f7329f24d921beff854171d9268be072d299d9a1e6359655b",
    "CertificateEvidenceArchiveOccurrenceWitnessV1": "8306448e64822a32d1c12fd95dcbc04274fe22e351d75f95c14b53d500fe33e5",
    "CertificateEvidenceArchiveOccurrenceWitnessSetV1": "fd726cae6b603ee5540fc93626507447fa6781a0d0528439faa84885fbc5e040",
    "ConsensusToReplayParentAuthorityCommitmentProofV1": "82fb5dd8b497591ae54d2a5b6ad4960f079a40f0a0caa24657418a134c2e5f79",
    "ReplayParentAuthorityStatusProjectionV1": "7d4001fdaadf3fe01de22119d32bde8d7633b3f28dcde34cb48b3f0d53bfe074",
    "ReplayParentAuthorityStatusProjectionSetV1": "30d122d592f5721980de0a4d130ba3551990e77b0a31f47a25d0656c66c9f80b",
    "NormalizedAcceptedEvidenceSnapshotV1": "62e7738e282b34f0d2a38c507332b4fd17943bd8ce74a3d2b17f64f716b94e73",
    "RetainedAcceptedEligibilityProjectionV1": "a7617047934f7435744671be3bed7aec082b7b1391b4f2ce006de0efe7df9bb5",
    "ReplayEvidenceRetentionProjectionProofV1": "a78f391f8cba45883ad243d56c6750b86c45072e89b4a606098c4f6cc929b6ff",
    "RetainedAcceptedEligibilityWitnessV1": "c82ab7b7ef0949fbf1a3cddd2d21c52ebc81a8008a5e6c9b7ca3cc1762bc2a56",
    "ReplayAcceptedAuthorityInputV1": "3f33efca53074dda11882bf2d27b25a24b1f2c2d640a12e8d9e0960563c70964",
    "HistoricalAcceptedParentAuthorityUseWitnessV1": "cffd714de1c56b00acfb592224f7d75259dc4d146bb3b5cc521a29e5acb32b1a",
    "HistoricalAcceptedParentAuthorityUseWitnessSetV1": "18b8b842930f29693fb1af5c4fdc89519864c71617dfa9f13f0712650dec361b",
    "AcceptedCoreReplayEntryV1": "f5f2804bb69296f40a58f6cc0fdabb4cf6ddf97a9d775302117b1c67015abe89",
}

EXPECTED_LOOP9_DOMAIN_CONTEXT_ASSIGNMENTS = frozenset(
    {
        name
        for name in EXPECTED_LOOP9_EXACT_ASSIGNMENT_SHA256
        if name[0].islower()
    }
)

EXPECTED_LOOP9_SPECIAL_CONTEXT_MUTATIONS = {
    "ReplayEvidenceArchiveResolutionV1": (
        (
            "replay_evidence_archive_policy_id",
            "wrong_replay_evidence_archive_policy_id",
        ),
    ),
    "ReplayEvidenceArchiveHashV1(framed_input)": (
        (
            "ReplayEvidenceArchiveResolutionV1.hash_suite_id",
            "ReplayEvidenceArchiveResolutionV1.wrong_hash_suite_id",
        ),
    ),
}

EXPECTED_LOOP10_ENVELOPE_HELPER_ASSIGNMENTS = (
    "EnvelopeOwnerProfileContextV1(profile_transition_context)",
    "EnvelopeResolutionV1(profile_transition_context)",
    "EnvelopeHashV1(profile_transition_context, object_domain, payload)",
    "EnvelopeCommitOrderedV1(profile_transition_context, sequence_domain, sequence)",
)
EXPECTED_LOOP10_ENVELOPE_HELPER_DEPENDENCIES = {
    "EnvelopeOwnerProfileContextV1(profile_transition_context)": frozenset(),
    "EnvelopeResolutionV1(profile_transition_context)": frozenset(
        {"EnvelopeOwnerProfileContextV1(profile_transition_context)"}
    ),
    "EnvelopeHashV1(profile_transition_context, object_domain, payload)": frozenset(
        {
            "EnvelopeOwnerProfileContextV1(profile_transition_context)",
            "EnvelopeResolutionV1(profile_transition_context)",
        }
    ),
    "EnvelopeCommitOrderedV1(profile_transition_context, sequence_domain, sequence)": frozenset(
        {"EnvelopeHashV1(profile_transition_context, object_domain, payload)"}
    ),
}
EXPECTED_LOOP10_ABSTRACT_ENVELOPE_PRIMITIVES = frozenset(
    {
        "Canonical",
        "CanonicalOrderedSequence",
        "Domain",
        "H",
        "InPlaceMigrationTransitionV1",
        "LengthFrame",
        "OrdinaryProfileTransitionV1",
        "ResolveAdmittedEnvelopeProfile",
    }
)

EXPECTED_ACCEPTED_PROPOSAL_CORE_FIELDS = (
    "accepted_proposal_core_version_u16_be",
    "accepted_parent_lookup_id",
    "authority_ref_kind_u8",
    "canonical_authority_ref_length_u32_be",
    "canonical_proposal_header_length_u64_be",
    "canonical_availability_payload_length_u64_be",
    "canonical_authority_ref_bytes",
    "canonical_proposal_header_bytes",
    "canonical_availability_payload_bytes",
)
EXPECTED_ACCEPTED_PROPOSAL_CORE_EXCLUDED_FIELDS = (
    "canonical_proposer_signature_length_u64_be",
    "canonical_proposer_signature_bytes",
    "typed_proposal_credential_id",
    "proposal_credential_id",
    "canonical_consensus_evidence_bytes",
    "consensus_evidence_id",
    "canonical_availability_assessment_bytes",
    "availability_assessment_id",
)
EXPECTED_ACCEPTED_PARENT_COLLISION_ROWS = (
    (
        "row_1",
        "ByteDistinctAcceptedParentLookupPayloadPreimagesSameProfileIdHashV1OutputV1",
        "ProtocolIdentityHashCompromiseV1",
    ),
    (
        "row_2",
        "ByteEqualAcceptedParentLookupPayloadAndBlockIdDistinctValidAcceptedProposalCoresV1",
        "ProfileOwnedAuthorityBindingCompromiseV1",
    ),
)
EXACT_ACCEPTED_PARENT_SCHEMA_NAMES = (
    "AcceptedProposalCoreFieldManifestV1",
    "AcceptedProposalCoreV1",
    "AcceptedProposalCoreExcludedFieldManifestV1",
    "AcceptedParentCollisionDispositionV1",
)
EXACT_ACCEPTED_PROPOSAL_CORE_DECLARATION = (
    "AcceptedProposalCoreV1 = FixedPreProfileCanonicalV1("
    "FieldsExactly(AcceptedProposalCoreFieldManifestV1))"
)
PROVENANCE_TABLE_HEADER = "| Provenance field | Canonical value |"
PROVENANCE_TABLE_SEPARATOR = "| --- | --- |"
PROVENANCE_TABLE_ROW = re.compile(r"^\| `([a-z0-9_]+)` \| `([^`]+)` \|$")
SUMMARY_TABLE_HEADER = "| Summary field | Canonical decimal value |"
SUMMARY_TABLE_SEPARATOR = "| --- | ---: |"
SUMMARY_TABLE_ROW = re.compile(r"^\| `([a-z0-9_]+)` \| `([0-9]+)` \|$")
PROVENANCE_FIELD_ORDER = (
    "audited_input_commit",
    "audited_input_tree",
    "addition_commit",
    "addition_tree",
    "audit_git_blob",
    "material_finding_count_u32",
)
PLANNING_SUMMARY_FIELD_ORDER = (
    "capability_node_count_u32",
    "foundation_node_count_u32",
    "detailed_node_count_u32",
    "phase_status_count_u32",
    "ready_frontier_count_u32",
    "base_dependency_edge_count_u32",
    "phase_dependency_edge_count_u32",
    "conditional_edge_count_u32",
    "total_declared_edge_count_u32",
    "predicate_entailment_count_u32",
    "planning_markdown_count_u32",
    "frozen_text_artifact_count_u32",
    "frozen_markdown_audit_count_u32",
    "frozen_json_receipt_count_u32",
)


class PlanningError(ValueError):
    pass


@dataclass(frozen=True)
class Node:
    node_id: str
    dependencies: tuple[str, ...]
    status_class: str
    status_projections: tuple["PhaseStatus", ...]
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


def _status_projections(
    text: str,
    node_id: str,
) -> tuple[tuple[PhaseStatus, ...], str]:
    """Parse a node's checked projection without treating its prose as authority."""

    canonical = text.strip()
    if canonical.endswith("."):
        canonical = canonical[:-1]
    tokens = canonical.split(", ") if canonical else []
    projections: list[PhaseStatus] = []
    seen_ids: set[str] = set()
    seen_phases: set[str] = set()
    for token in tokens:
        match = STATUS_PROJECTION_TOKEN.fullmatch(token)
        if match is None:
            raise PlanningError(
                f"{node_id}: status field is not a canonical projection token list: {text}"
            )
        projection = PhaseStatus(*match.groups())
        if projection.node_id != node_id:
            raise PlanningError(
                f"{node_id}: status projection {projection.status_id} names node "
                f"{projection.node_id}"
            )
        if projection.status_id in seen_ids:
            raise PlanningError(
                f"{node_id}: duplicate status projection ID {projection.status_id}"
            )
        if projection.phase in seen_phases:
            raise PlanningError(
                f"{node_id}: duplicate status projection phase {projection.phase}"
            )
        seen_ids.add(projection.status_id)
        seen_phases.add(projection.phase)
        projections.append(projection)

    base = [projection for projection in projections if projection.phase == "base"]
    if len(base) != 1:
        raise PlanningError(
            f"{node_id}: expected exactly one projected base phase, got {len(base)}"
        )
    return tuple(projections), base[0].status.replace("-", " ")


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


def _unique_manifest_lines(text: str, header: str, label: str) -> tuple[str, ...]:
    lines = text.splitlines()
    starts = [index for index, line in enumerate(lines) if line.strip() == header]
    if len(starts) != 1:
        raise PlanningError(f"{label}: expected exactly one {header}, got {len(starts)}")
    start = starts[0]
    depth = header.count("(") - header.count(")")
    end: int | None = None
    for index in range(start + 1, len(lines)):
        depth += lines[index].count("(") - lines[index].count(")")
        if depth == 0:
            end = index
            break
        if depth < 0:
            raise PlanningError(f"{label}: {header} closes past its opening parenthesis")
    if end is None:
        raise PlanningError(f"{label}: {header} has no exact closing parenthesis")
    return tuple(line.strip() for line in lines[start + 1 : end] if line.strip())


def _require_unique_schema_assignment(text: str, schema_name: str, label: str) -> None:
    assignments = re.findall(
        rf"(?m)^[ \t]*{re.escape(schema_name)}[ \t]*=",
        text,
    )
    if len(assignments) != 1:
        raise PlanningError(
            f"{label}: {schema_name} must have exactly one schema assignment, "
            f"got {len(assignments)}"
        )


def check_accepted_parent_exact_invariants(text: str, label: str) -> None:
    for schema_name in EXACT_ACCEPTED_PARENT_SCHEMA_NAMES:
        _require_unique_schema_assignment(text, schema_name, label)

    field_lines = _unique_manifest_lines(
        text,
        "AcceptedProposalCoreFieldManifestV1 = ExactOrderedFieldManifest(",
        label,
    )
    fields: list[str] = []
    ordinals: list[int] = []
    for line in field_lines:
        match = re.fullmatch(r"([0-9]+)\s*=>\s*([a-z0-9_]+),?", line)
        if match is None:
            raise PlanningError(f"{label}: malformed accepted-core field row: {line}")
        ordinal, field_name = match.groups()
        ordinals.append(int(ordinal))
        fields.append(field_name)
    expected_ordinals = list(range(1, len(EXPECTED_ACCEPTED_PROPOSAL_CORE_FIELDS) + 1))
    if ordinals != expected_ordinals or tuple(fields) != EXPECTED_ACCEPTED_PROPOSAL_CORE_FIELDS:
        raise PlanningError(
            f"{label}: AcceptedProposalCoreV1 exact ordered fields disagree: "
            f"expected={list(EXPECTED_ACCEPTED_PROPOSAL_CORE_FIELDS)}, got={fields}"
        )

    core_declarations = re.findall(
        r"AcceptedProposalCoreV1\s*=\s*FixedPreProfileCanonicalV1\(\s*"
        r"FieldsExactly\(AcceptedProposalCoreFieldManifestV1\)\s*\)",
        text,
    )
    if len(core_declarations) != 1:
        raise PlanningError(
            f"{label}: AcceptedProposalCoreV1 must be declared exactly once through "
            "FieldsExactly(AcceptedProposalCoreFieldManifestV1)"
        )

    excluded_lines = _unique_manifest_lines(
        text,
        "AcceptedProposalCoreExcludedFieldManifestV1 = ExactForbiddenFieldManifest(",
        label,
    )
    exclusions: list[str] = []
    for line in excluded_lines:
        match = re.fullmatch(r"([a-z0-9_]+),?", line)
        if match is None:
            raise PlanningError(f"{label}: malformed accepted-core exclusion row: {line}")
        exclusions.append(match.group(1))
    if tuple(exclusions) != EXPECTED_ACCEPTED_PROPOSAL_CORE_EXCLUDED_FIELDS:
        raise PlanningError(
            f"{label}: AcceptedProposalCoreV1 exact exclusions disagree: "
            f"expected={list(EXPECTED_ACCEPTED_PROPOSAL_CORE_EXCLUDED_FIELDS)}, got={exclusions}"
        )
    overlap = sorted(set(fields) & set(exclusions))
    if overlap:
        raise PlanningError(f"{label}: accepted-core fields also appear as exclusions: {overlap}")

    collision_lines = _unique_manifest_lines(
        text,
        "AcceptedParentCollisionDispositionV1 = CanonicalTwoRowMap(",
        label,
    )
    collision_body = " ".join(collision_lines)
    collision_pattern = re.compile(
        r"(row_[0-9]+)\s*=\s*\(\s*"
        r"([A-Za-z0-9]+)\s*,\s*([A-Za-z0-9]+)\s*\)\s*,?"
    )
    collision_rows = [match.groups() for match in collision_pattern.finditer(collision_body)]
    residual = collision_pattern.sub("", collision_body).strip()
    if residual:
        raise PlanningError(f"{label}: malformed collision-disposition content: {residual}")
    if tuple(collision_rows) != EXPECTED_ACCEPTED_PARENT_COLLISION_ROWS:
        raise PlanningError(
            f"{label}: AcceptedParentCollisionDispositionV1 disagrees: "
            f"expected={list(EXPECTED_ACCEPTED_PARENT_COLLISION_ROWS)}, got={collision_rows}"
        )


def _split_schema_top_level(text: str, delimiter: str, label: str) -> tuple[str, ...]:
    parts: list[str] = []
    start = 0
    depth = 0
    quote: str | None = None
    escaped = False
    for index, character in enumerate(text):
        if quote is not None:
            if escaped:
                escaped = False
            elif character == "\\":
                escaped = True
            elif character == quote:
                quote = None
            continue
        if character in {'"', "'"}:
            quote = character
        elif character in "([{" :
            depth += 1
        elif character in ")]}" :
            depth -= 1
            if depth < 0:
                raise PlanningError(f"{label}: schema expression closes past its opener")
        elif character == delimiter and depth == 0:
            part = text[start:index].strip()
            if not part:
                raise PlanningError(f"{label}: empty schema component")
            parts.append(part)
            start = index + 1
    if quote is not None or depth != 0:
        raise PlanningError(f"{label}: unbalanced schema expression")
    final = text[start:].strip()
    if not final:
        raise PlanningError(f"{label}: empty final schema component")
    parts.append(final)
    return tuple(parts)


_SIMPLE_TOP_LEVEL_SCHEMA_BOUNDARY = re.compile(
    r"^(?:[A-Za-z_][A-Za-z0-9_]*(?:\([^\r\n)]*\))?[ \t]*=(?!>)|"
    r"#{1,6}[ \t]+|```)",
    flags=re.MULTILINE,
)
_LINE_START = re.compile(r"^", flags=re.MULTILINE)
_LINE_IDENTIFIER = re.compile(
    r"^[ \t]*([A-Za-z_][A-Za-z0-9_]*)",
    flags=re.MULTILINE,
)


def _balanced_parenthesis_end(text: str, start: int) -> int | None:
    if start >= len(text) or text[start] != "(":
        return None
    depth = 0
    quote: str | None = None
    escaped = False
    for index in range(start, len(text)):
        character = text[index]
        if quote is not None:
            if escaped:
                escaped = False
            elif character == "\\":
                escaped = True
            elif character == quote:
                quote = None
            continue
        if character in {'"', "'"}:
            quote = character
            continue
        if character == "(":
            depth += 1
            continue
        if character != ")":
            continue
        depth -= 1
        if depth < 0:
            return None
        if depth == 0:
            return index + 1
    return None


def _is_parenthesized_top_level_schema_boundary(text: str, start: int) -> bool:
    """Recognize a complete tuple-LHS ``=``/``->`` statement, not a stray ``(`` tail."""

    cursor = _balanced_parenthesis_end(text, start)
    if cursor is None:
        return False
    while cursor < len(text) and text[cursor].isspace():
        cursor += 1
    return (
        (text.startswith("=", cursor) and not text.startswith("=>", cursor))
        or text.startswith("->", cursor)
    )


def _next_top_level_schema_boundary(text: str, search_start: int) -> int:
    for line_start in _LINE_START.finditer(text, search_start):
        start = line_start.start()
        if _SIMPLE_TOP_LEVEL_SCHEMA_BOUNDARY.match(text, start) is not None:
            return start
        if _is_parenthesized_top_level_schema_boundary(text, start):
            return start
    return len(text)


def _schema_assignment_bounds(text: str, schema_name: str) -> tuple[int, int]:
    assignment_pattern = re.compile(
        rf"^[ \t]*{re.escape(schema_name)}[ \t]*=",
        flags=re.MULTILINE,
    )
    assignments = list(assignment_pattern.finditer(text))
    if len(assignments) != 1:
        raise PlanningError(
            f"LAYER1_DESIGN_PAPER.md: {schema_name} must have exactly one assignment, "
            f"got {len(assignments)}"
        )
    start = assignments[0].start()
    return start, _next_top_level_schema_boundary(text, assignments[0].end())


def _compact_schema_whitespace(text: str, label: str) -> str:
    """Remove layout whitespace while preserving every quoted literal character."""

    compact: list[str] = []
    quote: str | None = None
    escaped = False
    for character in text:
        if quote is not None:
            compact.append(character)
            if escaped:
                escaped = False
            elif character == "\\":
                escaped = True
            elif character == quote:
                quote = None
            continue
        if character in {'"', "'"}:
            quote = character
            compact.append(character)
        elif not character.isspace():
            compact.append(character)
    if quote is not None:
        raise PlanningError(f"{label}: unterminated quoted literal")
    return "".join(compact)


def _parse_exact_schema_assignment(
    text: str,
    schema_name: str,
) -> tuple[tuple[str, tuple[str, ...]], ...]:
    start, end = _schema_assignment_bounds(text, schema_name)
    block = text[start:end]
    if "=" not in block:
        raise PlanningError(f"LAYER1_DESIGN_PAPER.md: {schema_name} assignment has no RHS")
    rhs = block.split("=", 1)[1].strip()
    alternatives = _split_schema_top_level(rhs, "|", schema_name)
    parsed: list[tuple[str, tuple[str, ...]]] = []
    for alternative in alternatives:
        bare_type = re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", alternative)
        if bare_type is not None:
            parsed.append(("BareTypeRef", (bare_type.group(0),)))
            continue
        match_expression = re.fullmatch(
            r"match\s+([A-Za-z_][A-Za-z0-9_.]*)\s*\{(.*)\}",
            alternative,
            flags=re.DOTALL,
        )
        if match_expression is not None:
            match_subject, match_body = match_expression.groups()
            parsed.append(
                (
                    "MatchExpression",
                    (
                        match_subject,
                        _compact_schema_whitespace(
                            match_body,
                            f"{schema_name}.MatchExpression",
                        ),
                    ),
                )
            )
            continue
        call = re.fullmatch(
            r"([A-Za-z_][A-Za-z0-9_.]*(?:\[[^\[\]]+\])?)"
            r"\s*\((.*)\)",
            alternative,
            flags=re.DOTALL,
        )
        if call is None:
            raise PlanningError(
                f"LAYER1_DESIGN_PAPER.md: {schema_name} has malformed variant: "
                f"{alternative.strip()}"
            )
        variant_name, body = call.groups()
        variant_name = _compact_schema_whitespace(
            variant_name,
            f"{schema_name}.variant",
        )
        arguments = (
            _split_schema_top_level(body, ",", f"{schema_name}.{variant_name}")
            if body.strip()
            else ()
        )
        parsed.append(
            (
                variant_name,
                tuple(
                    _compact_schema_whitespace(
                        argument,
                        f"{schema_name}.{variant_name}",
                    )
                    for argument in arguments
                ),
            )
        )
    return tuple(parsed)


def check_loop8_exact_schemas(
    text: str,
    schema_names: tuple[str, ...] | None = None,
) -> None:
    selected = (
        tuple(EXPECTED_LOOP8_EXACT_SCHEMA_VARIANTS)
        if schema_names is None
        else schema_names
    )
    unknown = sorted(set(selected) - EXPECTED_LOOP8_EXACT_SCHEMA_VARIANTS.keys())
    if unknown:
        raise PlanningError(f"unknown Loop 8 exact schemas requested: {unknown}")
    for schema_name in selected:
        if schema_name in EXPECTED_LOOP9_EXACT_ASSIGNMENT_SHA256:
            check_loop9_exact_assignments(text, (schema_name,))
            continue
        expected = EXPECTED_LOOP8_EXACT_SCHEMA_VARIANTS[schema_name]
        actual = _parse_exact_schema_assignment(text, schema_name)
        if actual != expected:
            raise PlanningError(
                f"LAYER1_DESIGN_PAPER.md: {schema_name} exact ordered variants/fields "
                f"disagree: expected={expected}, got={actual}"
            )


def _exact_assignment_sha256(
    parsed: tuple[tuple[str, tuple[str, ...]], ...],
) -> str:
    canonical = json.dumps(parsed, separators=(",", ":"), ensure_ascii=True)
    return hashlib.sha256(canonical.encode("utf-8")).hexdigest()


def check_loop9_exact_assignments(
    text: str,
    schema_names: tuple[str, ...] | None = None,
) -> None:
    selected = (
        tuple(EXPECTED_LOOP9_EXACT_ASSIGNMENT_SHA256)
        if schema_names is None
        else schema_names
    )
    unknown = sorted(set(selected) - EXPECTED_LOOP9_EXACT_ASSIGNMENT_SHA256.keys())
    if unknown:
        raise PlanningError(f"unknown Loop 9 exact assignments requested: {unknown}")
    for schema_name in selected:
        actual = _parse_exact_schema_assignment(text, schema_name)
        digest = _exact_assignment_sha256(actual)
        expected = EXPECTED_LOOP9_EXACT_ASSIGNMENT_SHA256[schema_name]
        if digest != expected:
            raise PlanningError(
                f"LAYER1_DESIGN_PAPER.md: {schema_name} exact ordered variants/fields "
                "disagree (including Loop 9 domain/context): "
                f"expected_sha256={expected}, got_sha256={digest}, got={actual}"
            )


def _top_level_schema_assignment_bases(text: str) -> frozenset[str]:
    bases: set[str] = set()
    for match in _LINE_IDENTIFIER.finditer(text):
        cursor = match.end()
        while cursor < len(text) and text[cursor] in " \t":
            cursor += 1
        if cursor < len(text) and text[cursor] == "(":
            parenthesis_end = _balanced_parenthesis_end(text, cursor)
            if parenthesis_end is None:
                continue
            cursor = parenthesis_end
        while cursor < len(text) and text[cursor].isspace():
            cursor += 1
        if text.startswith("=", cursor) and not text.startswith("=>", cursor):
            bases.add(match.group(1))
    return frozenset(bases)


def _parsed_schema_call_symbols(
    parsed: tuple[tuple[str, tuple[str, ...]], ...],
) -> frozenset[str]:
    chunks: list[str] = []
    symbols: set[str] = set()
    for variant_name, arguments in parsed:
        if variant_name not in {"BareTypeRef", "MatchExpression"}:
            chunks.append(variant_name)
            outer_call = re.match(r"([A-Za-z_][A-Za-z0-9_]*)", variant_name)
            if outer_call is not None:
                symbols.add(outer_call.group(1))
        chunks.extend(arguments)
    symbols.update(
        match.group(1)
        for chunk in chunks
        for match in re.finditer(
            r"(?<![A-Za-z0-9_.])([A-Za-z_][A-Za-z0-9_]*)\s*(?=[\[(])",
            chunk,
        )
    )
    return frozenset(symbols)


def _check_loop10_envelope_helper_transitive_closure(text: str) -> None:
    expected_helpers = frozenset(EXPECTED_LOOP10_ENVELOPE_HELPER_ASSIGNMENTS)
    if set(EXPECTED_LOOP10_ENVELOPE_HELPER_DEPENDENCIES) != set(expected_helpers):
        raise PlanningError("Loop 10 envelope helper dependency manifest is incomplete")
    missing_exact_helpers = sorted(
        expected_helpers - EXPECTED_LOOP9_EXACT_ASSIGNMENT_SHA256.keys()
    )
    if missing_exact_helpers:
        raise PlanningError(
            "Loop 10 envelope helpers are outside the exact assignment manifest: "
            f"{missing_exact_helpers}"
        )

    helper_by_base = {
        helper.split("(", 1)[0]: helper
        for helper in EXPECTED_LOOP10_ENVELOPE_HELPER_ASSIGNMENTS
    }
    if len(helper_by_base) != len(EXPECTED_LOOP10_ENVELOPE_HELPER_ASSIGNMENTS):
        raise PlanningError("Loop 10 envelope helper base names are ambiguous")
    defined_bases = _top_level_schema_assignment_bases(text)
    actual_dependencies: dict[str, frozenset[str]] = {}
    for helper in EXPECTED_LOOP10_ENVELOPE_HELPER_ASSIGNMENTS:
        parsed = _parse_exact_schema_assignment(text, helper)
        symbols = _parsed_schema_call_symbols(parsed)
        helper_dependencies = frozenset(
            helper_by_base[symbol] for symbol in symbols if symbol in helper_by_base
        )
        undeclared_defined_helpers = sorted(
            symbol
            for symbol in symbols
            if symbol in defined_bases and symbol not in helper_by_base
        )
        if undeclared_defined_helpers:
            raise PlanningError(
                f"{helper}: defined helper calls escape the Loop 10 exact transitive "
                f"closure: {undeclared_defined_helpers}"
            )
        undeclared_abstract_primitives = sorted(
            symbol
            for symbol in symbols
            if symbol not in defined_bases
            and symbol not in helper_by_base
            and symbol not in EXPECTED_LOOP10_ABSTRACT_ENVELOPE_PRIMITIVES
        )
        if undeclared_abstract_primitives:
            raise PlanningError(
                f"{helper}: undeclared abstract envelope primitives: "
                f"{undeclared_abstract_primitives}"
            )
        expected_dependencies = EXPECTED_LOOP10_ENVELOPE_HELPER_DEPENDENCIES[helper]
        if helper_dependencies != expected_dependencies:
            raise PlanningError(
                f"{helper}: transitive helper dependencies disagree: "
                f"expected={sorted(expected_dependencies)}, "
                f"got={sorted(helper_dependencies)}"
            )
        actual_dependencies[helper] = helper_dependencies

    root = "EnvelopeCommitOrderedV1(profile_transition_context, sequence_domain, sequence)"
    reachable: set[str] = set()
    stack = [root]
    while stack:
        helper = stack.pop()
        if helper in reachable:
            continue
        reachable.add(helper)
        stack.extend(actual_dependencies[helper])
    if reachable != set(expected_helpers):
        raise PlanningError(
            "Loop 10 ordered-envelope helper transitive closure is incomplete: "
            f"expected={sorted(expected_helpers)}, got={sorted(reachable)}"
        )


def check_loop10_envelope_helper_closure(text: str) -> None:
    check_loop9_exact_assignments(text, EXPECTED_LOOP10_ENVELOPE_HELPER_ASSIGNMENTS)
    _check_loop10_envelope_helper_transitive_closure(text)


def _parse_named_two_column_table(
    text: str,
    heading: str,
    header: str,
    separator: str,
    row_pattern: re.Pattern[str],
    label: str,
) -> tuple[tuple[str, str], ...]:
    lines = text.splitlines()
    headings = [index for index, line in enumerate(lines) if line == heading]
    if len(headings) != 1:
        raise PlanningError(f"{label}: expected exactly one {heading} heading")
    start = headings[0]
    end = next(
        (
            index
            for index in range(start + 1, len(lines))
            if lines[index].startswith("#")
        ),
        len(lines),
    )
    headers = [index for index in range(start + 1, end) if lines[index] == header]
    if len(headers) != 1:
        raise PlanningError(f"{label}: expected exactly one canonical table header")
    table = headers[0]
    if table + 1 >= end or lines[table + 1] != separator:
        raise PlanningError(f"{label}: table separator is not canonical")
    rows: list[tuple[str, str]] = []
    index = table + 2
    while index < end and lines[index].startswith("|"):
        match = row_pattern.fullmatch(lines[index])
        if match is None:
            raise PlanningError(f"{label}: malformed projection row: {lines[index]}")
        rows.append(match.groups())
        index += 1
    if not rows:
        raise PlanningError(f"{label}: projection table is empty")
    if any(line.startswith("|") for line in lines[index:end]):
        raise PlanningError(f"{label}: contains a second or non-contiguous table")
    fields = [field for field, _value in rows]
    duplicates = sorted(field for field in set(fields) if fields.count(field) > 1)
    if duplicates:
        raise PlanningError(f"{label}: duplicate projection fields: {duplicates}")
    return tuple(rows)


def _without_heading_section(text: str, heading: str) -> str:
    lines = text.splitlines()
    starts = [index for index, line in enumerate(lines) if line == heading]
    if len(starts) != 1:
        raise PlanningError(f"expected exactly one {heading} heading while removing projection")
    start = starts[0]
    end = next(
        (index for index in range(start + 1, len(lines)) if lines[index].startswith("#")),
        len(lines),
    )
    return "\n".join(lines[:start] + lines[end:])


def parse_capability(
    text: str,
    *,
    enforce_manifest: bool = True,
) -> dict[str, Node]:
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
        status_projections, status_class = _status_projections(_status, node_id)
        nodes[node_id] = Node(
            node_id,
            tuple(DEPENDENCY.findall(dependency_clause)),
            status_class,
            status_projections,
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
        status_projections, status_class = _status_projections(fields["Status"], node_id)
        nodes[node_id] = Node(
            node_id,
            dependencies,
            status_class,
            status_projections,
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
    consensus_artifact = detail_fields["CONSENSUS-1"]["Artifact"]
    for required_fragment in EXPECTED_CONSENSUS_AUTHORITY_ARTIFACT_FRAGMENTS:
        if required_fragment not in consensus_artifact:
            raise PlanningError(
                f"CONSENSUS-1 authority-ownership artifact omits {required_fragment}"
            )
    sync_artifact = detail_fields["SYNC-1"]["Artifact"]
    for required_fragment in EXPECTED_SYNC_AUTHORITY_ARTIFACT_FRAGMENTS:
        if required_fragment not in sync_artifact:
            raise PlanningError(
                f"SYNC-1 authority-ownership artifact omits {required_fragment}"
            )
    sync_gate = detail_fields["SYNC-1"]["Gate"]
    for required_fragment in EXPECTED_SYNC_AUTHORITY_GATE_FRAGMENTS:
        if required_fragment not in sync_gate:
            raise PlanningError(
                f"SYNC-1 authority-ownership gate omits {required_fragment}"
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
    if enforce_manifest and base_dependency_digest != EXPECTED_BASE_DEPENDENCY_DIGEST:
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
        node_rows = tuple(status for status in statuses if status.node_id == node_id)
        base_rows = [status for status in node_rows if status.phase == "base"]
        if len(base_rows) != 1:
            raise PlanningError(
                f"{node_id}: expected exactly one base phase-status tuple, got {len(base_rows)}"
            )
        if node.status_projections != node_rows:
            projected = [
                f"{row.status_id}:{row.node_id}@{row.phase}={row.status}"
                for row in node.status_projections
            ]
            authoritative = [
                f"{row.status_id}:{row.node_id}@{row.phase}={row.status}"
                for row in node_rows
            ]
            raise PlanningError(
                f"{node_id}: checked status projections disagree with authoritative phase-status "
                f"rows: projected={projected}, authoritative={authoritative}"
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


def parse_ready_frontier(
    text: str,
    phase_statuses: tuple[PhaseStatus, ...],
) -> tuple[PhaseStatus, ...]:
    """Check the secondary scheduling projection against authoritative PS rows."""

    lines = text.splitlines()
    heading_indexes = [
        index for index, line in enumerate(lines) if line == "## Current unlock frontier"
    ]
    if len(heading_indexes) != 1:
        raise PlanningError(
            "capability tree must contain exactly one Current unlock frontier section"
        )
    start = heading_indexes[0]
    end = next(
        (
            index
            for index in range(start + 1, len(lines))
            if lines[index].startswith("## ")
        ),
        len(lines),
    )
    header_indexes = [
        index
        for index in range(start + 1, end)
        if lines[index] == READY_FRONTIER_HEADER
    ]
    if len(header_indexes) != 1:
        raise PlanningError(
            "Current unlock frontier must contain exactly one canonical READY table"
        )
    header = header_indexes[0]
    if header + 1 >= end or lines[header + 1] != READY_FRONTIER_SEPARATOR:
        raise PlanningError("Current unlock frontier READY table separator is not canonical")

    projections: list[PhaseStatus] = []
    ordinals: list[int] = []
    seen_ids: set[str] = set()
    seen_phases: set[tuple[str, str]] = set()
    index = header + 2
    while index < end and lines[index].startswith("|"):
        match = READY_FRONTIER_ROW.fullmatch(lines[index])
        if match is None:
            raise PlanningError(
                f"Current unlock frontier row {index + 1} is not canonical: {lines[index]}"
            )
        ordinal_text, status_id, node_id, phase, status = match.groups()
        projection = PhaseStatus(status_id, node_id, phase, status)
        if projection.status_id in seen_ids:
            raise PlanningError(
                f"Current unlock frontier duplicates status ID {projection.status_id}"
            )
        phase_key = (projection.node_id, projection.phase)
        if phase_key in seen_phases:
            raise PlanningError(
                "Current unlock frontier duplicates phase "
                f"{projection.node_id}@{projection.phase}"
            )
        if projection.status != "READY":
            raise PlanningError(
                f"Current unlock frontier entry {projection.status_id} is not READY"
            )
        seen_ids.add(projection.status_id)
        seen_phases.add(phase_key)
        ordinals.append(int(ordinal_text))
        projections.append(projection)
        index += 1

    if not projections:
        raise PlanningError("Current unlock frontier has no structured READY entries")
    expected_ordinals = list(range(1, len(projections) + 1))
    if ordinals != expected_ordinals:
        raise PlanningError(
            "Current unlock frontier ordinals are not continuous: "
            f"expected={expected_ordinals}, got={ordinals}"
        )
    if any(line.startswith("|") for line in lines[index:end]):
        raise PlanningError(
            "Current unlock frontier contains a second or non-contiguous table"
        )

    expected = tuple(row for row in phase_statuses if row.status == "READY")
    if tuple(projections) != expected:
        projected = [
            f"{row.status_id}:{row.node_id}@{row.phase}={row.status}"
            for row in projections
        ]
        authoritative = [
            f"{row.status_id}:{row.node_id}@{row.phase}={row.status}"
            for row in expected
        ]
        raise PlanningError(
            "Current unlock frontier disagrees with the ordered authoritative READY rows: "
            f"projected={projected}, authoritative={authoritative}"
        )
    return tuple(projections)


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


def check_compatible_dependency_union(
    nodes: dict[str, Node],
    phase_statuses: tuple[PhaseStatus, ...],
    phase_dependencies: tuple[PhaseDependency, ...],
    conditional_edges: tuple[ConditionalEdge, ...],
) -> None:
    """Reject cycles spanning inherited base, phase, and compatible role edges.

    Phase names and conditional roles name the same graph instance exactly when
    their ``node@phase-or-role`` strings are equal.  All instances inherit the
    owning capability node's unconditional base prerequisites.  Checking this
    union is necessary because the PD-only and CE-only projections can each be
    acyclic while their compatible union contains a cross-class cycle.
    """

    instances: set[tuple[str, str]] = {
        (status.node_id, status.phase) for status in phase_statuses
    }
    instances.update(
        (dependency.owner_node, dependency.owner_phase)
        for dependency in phase_dependencies
    )
    instances.update(
        (dependency.prerequisite_node, dependency.prerequisite_phase)
        for dependency in phase_dependencies
    )
    instances.update(
        (edge.owner_base, edge.owner_role) for edge in conditional_edges
    )
    instances.update(
        (edge.dependency_base, edge.dependency_role) for edge in conditional_edges
    )
    instances.update((node_id, "base") for node_id in nodes)

    adjacency: dict[str, list[str]] = {
        f"{node_id}@{role}": [
            f"{dependency}@base" for dependency in nodes[node_id].dependencies
        ]
        for node_id, role in sorted(instances)
    }
    for dependency in phase_dependencies:
        adjacency[f"{dependency.owner_node}@{dependency.owner_phase}"].append(
            f"{dependency.prerequisite_node}@{dependency.prerequisite_phase}"
        )
    for edge in conditional_edges:
        adjacency[f"{edge.owner_base}@{edge.owner_role}"].append(
            f"{edge.dependency_base}@{edge.dependency_role}"
        )

    _assert_acyclic(
        {instance: tuple(prerequisites) for instance, prerequisites in adjacency.items()},
        "compatible base-phase-role dependency union",
    )


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


def check_handoff_read_order_and_counts(
    text: str,
    tracked_paths: frozenset[Path],
) -> tuple[str, ...]:
    """Validate the human reading path as an exact projection of planning authority."""

    lines = text.splitlines()
    markers = [index for index, line in enumerate(lines) if line == "Read in this order:"]
    if len(markers) != 1:
        raise PlanningError("HANDOFF must contain exactly one canonical read-order marker")
    marker = markers[0]
    if marker + 1 >= len(lines) or lines[marker + 1] != "":
        raise PlanningError("HANDOFF read-order marker must be followed by one blank line")

    rows: list[tuple[int, str]] = []
    index = marker + 2
    while index < len(lines) and lines[index] != "":
        match = HANDOFF_READ_ORDER_ROW.fullmatch(lines[index])
        if match is None:
            raise PlanningError(
                f"HANDOFF read-order row {index + 1} is not canonical: {lines[index]}"
            )
        ordinal_text, raw_path = match.groups()
        rows.append((int(ordinal_text), raw_path))
        index += 1
    if not rows:
        raise PlanningError("HANDOFF read-order block is empty")

    ordinals = [ordinal for ordinal, _path in rows]
    expected_ordinals = list(range(1, len(rows) + 1))
    if ordinals != expected_ordinals:
        raise PlanningError(
            "HANDOFF read-order numbering is not continuous: "
            f"expected={expected_ordinals}, got={ordinals}"
        )

    normalized: list[str] = []
    for _ordinal, raw_path in rows:
        candidate = Path(raw_path)
        if candidate.is_absolute():
            raise PlanningError(f"HANDOFF read-order path must be repository-relative: {raw_path}")
        resolved = (ROOT / candidate).resolve()
        canonical = repository_relative_path(resolved)
        if canonical != raw_path:
            raise PlanningError(
                f"HANDOFF read-order path is not normalized: {raw_path} -> {canonical}"
            )
        if resolved not in tracked_paths:
            raise PlanningError(f"HANDOFF read-order path is not tracked or staged: {raw_path}")
        normalized.append(canonical)

    duplicates = sorted(path for path in set(normalized) if normalized.count(path) > 1)
    if duplicates:
        raise PlanningError(
            "HANDOFF read-order contains duplicate paths: " + ", ".join(duplicates)
        )
    if tuple(normalized) != EXPECTED_HANDOFF_READ_ORDER:
        raise PlanningError(
            "HANDOFF read-order disagrees with the exact canonical planning order: "
            f"expected={list(EXPECTED_HANDOFF_READ_ORDER)}, got={normalized}"
        )
    if frozenset(normalized) != EXPECTED_PLANNING_PATHS:
        raise PlanningError("HANDOFF read-order set disagrees with the planning inventory")
    return tuple(normalized)


def derive_planning_checker_summary(
    capability_text: str,
    nodes: dict[str, Node],
    phase_statuses: tuple[PhaseStatus, ...],
    ready_frontier: tuple[PhaseStatus, ...],
    phase_dependencies: tuple[PhaseDependency, ...],
    conditional_edges: tuple[ConditionalEdge, ...],
    predicate_entailments: tuple[PredicateEntailment, ...],
) -> dict[str, int]:
    foundation_count = sum(
        FOUNDATION_ROW.match(line) is not None for line in capability_text.splitlines()
    )
    detailed_count = sum(
        DETAIL_HEADING.match(line) is not None for line in capability_text.splitlines()
    )
    base_edge_count = sum(len(node.dependencies) for node in nodes.values())
    frozen_markdown_count = sum(path.suffix.lower() == ".md" for path in FROZEN_TEXT_FILES)
    frozen_json_count = sum(path.suffix.lower() == ".json" for path in FROZEN_TEXT_FILES)
    return {
        "capability_node_count_u32": len(nodes),
        "foundation_node_count_u32": foundation_count,
        "detailed_node_count_u32": detailed_count,
        "phase_status_count_u32": len(phase_statuses),
        "ready_frontier_count_u32": len(ready_frontier),
        "base_dependency_edge_count_u32": base_edge_count,
        "phase_dependency_edge_count_u32": len(phase_dependencies),
        "conditional_edge_count_u32": len(conditional_edges),
        "total_declared_edge_count_u32": (
            base_edge_count + len(phase_dependencies) + len(conditional_edges)
        ),
        "predicate_entailment_count_u32": len(predicate_entailments),
        "planning_markdown_count_u32": len(PLANNING_FILES),
        "frozen_text_artifact_count_u32": len(FROZEN_TEXT_FILES),
        "frozen_markdown_audit_count_u32": frozen_markdown_count,
        "frozen_json_receipt_count_u32": frozen_json_count,
    }


def check_planning_checker_summary_projection(
    capability_text: str,
    expected: dict[str, int],
) -> tuple[tuple[str, str], ...]:
    rows = _parse_named_two_column_table(
        capability_text,
        "## `PlanningCheckerSummaryV1`",
        SUMMARY_TABLE_HEADER,
        SUMMARY_TABLE_SEPARATOR,
        SUMMARY_TABLE_ROW,
        "PlanningCheckerSummaryV1",
    )
    fields = tuple(field for field, _value in rows)
    if fields != PLANNING_SUMMARY_FIELD_ORDER:
        raise PlanningError(
            "PlanningCheckerSummaryV1 field order disagrees: "
            f"expected={list(PLANNING_SUMMARY_FIELD_ORDER)}, got={list(fields)}"
        )
    projected = {field: int(value) for field, value in rows}
    if projected != expected:
        raise PlanningError(
            "PlanningCheckerSummaryV1 values disagree with parsed authority: "
            f"expected={expected}, got={projected}"
        )
    return rows


def check_audit_provenance_projection(
    handoff_text: str,
    schema_name: str,
    expected: dict[str, str],
) -> tuple[tuple[str, str], ...]:
    audit_count_sources = {
        "Loop7ProvenanceV1": (SEVENTH_AUDIT, "LG", "Loop 7 audit"),
        "Loop8ProvenanceV1": (EIGHTH_AUDIT, "EG", "Loop 8 audit"),
        "Loop9ProvenanceV1": (NINTH_AUDIT, "NG", "Loop 9 audit"),
        "Loop10ProvenanceV1": (TENTH_AUDIT, "TG", "Loop 10 audit"),
    }
    if schema_name not in audit_count_sources:
        raise PlanningError(f"unknown audit provenance schema {schema_name}")
    audit_path, finding_prefix, audit_label = audit_count_sources[schema_name]
    derived_count = len(
        _parse_frozen_audit_finding_ids(
            audit_path.read_bytes(),
            prefix=finding_prefix,
            label=audit_label,
        )
    )
    expected_count = expected.get("material_finding_count_u32")
    if expected_count != str(derived_count):
        raise PlanningError(
            f"{schema_name} audit-derived finding count mismatch: frozen {finding_prefix} "
            f"table derives {derived_count}, caller supplied {expected_count}"
        )
    rows = _parse_named_two_column_table(
        handoff_text,
        f"### {schema_name}",
        PROVENANCE_TABLE_HEADER,
        PROVENANCE_TABLE_SEPARATOR,
        PROVENANCE_TABLE_ROW,
        schema_name,
    )
    fields = tuple(field for field, _value in rows)
    if fields != PROVENANCE_FIELD_ORDER:
        raise PlanningError(
            f"{schema_name} field order disagrees: "
            f"expected={list(PROVENANCE_FIELD_ORDER)}, got={list(fields)}"
        )
    projected = dict(rows)
    if projected != expected:
        raise PlanningError(
            f"{schema_name} values disagree with Git/frozen authority: "
            f"expected={expected}, got={projected}"
        )
    return rows


_COUNT_WORD_BY_VALUE = {
    0: "zero",
    1: "one",
    2: "two",
    3: "three",
    4: "four",
    5: "five",
    6: "six",
    7: "seven",
    8: "eight",
    9: "nine",
    10: "ten",
    11: "eleven",
    12: "twelve",
    13: "thirteen",
    14: "fourteen",
    15: "fifteen",
    16: "sixteen",
    17: "seventeen",
    18: "eighteen",
    19: "nineteen",
    20: "twenty",
    21: "twenty-one",
    22: "twenty-two",
}


_ANY_COUNT_TOKEN = "(?:[0-9]+|" + "|".join(
    re.escape(word)
    for word in sorted(_COUNT_WORD_BY_VALUE.values(), key=len, reverse=True)
) + r"|no|none(?:[\s_-]+of(?:[\s_-]+the)?)?)"

_AUDIT_ACTIVITY_NOUN = r"(?:audits?|reviews?|sweeps?|pass(?:es)?|(?:re-?)?audits?)"
_RE_AUDIT_NOUN = r"(?:re-?audits?)"
_STRONG_SUCCESSOR_QUALIFIER = (
    r"(?:successor|separate|later|next|subsequent|another)"
)
_DISTINCT_SUCCESSOR_SUBJECT = (
    rf"(?:(?:{_STRONG_SUCCESSOR_QUALIFIER}\s+)+{_AUDIT_ACTIVITY_NOUN}"
    rf"|(?:(?:independent|fresh)\s+)*{_RE_AUDIT_NOUN})"
)
_AUDIT_RESULT_VERB = (
    r"(?:has|have|had|found|contains?|records?|recorded|reports?|reported|"
    r"shows?|showed|yields?|yielded|produces?|produced|returns?|returned|"
    r"identifies?|identified|exposes?|exposed|"
    r"(?:is|are|was|were)\s+(?:claimed|reported)\s+to\s+have)"
)
_INDIRECT_CLAIM_TOKEN = re.compile(
    r"\b(?:that|which|who|whose|whereas|although|but|because|while|since|as|"
    r"if|unless|though|despite)\b",
    re.IGNORECASE,
)
_DIRECT_SUCCESSOR_PAYLOAD_SUFFIX = re.compile(
    r"\s*(?:(?:after|following)\s+(?:the\s+)?"
    r"(?:correction|corrections|review|audit)"
    r"|in\s+(?:the\s+)?audit\s+(?:records?|reports?|files?))?\s*$",
    re.IGNORECASE,
)


def _payload_is_single_direct_finding_claim(
    payload: str,
    finding_claim_pattern: re.Pattern[str],
) -> bool:
    if _INDIRECT_CLAIM_TOKEN.search(payload) is not None:
        return False
    finding_claims = tuple(finding_claim_pattern.finditer(payload))
    if len(finding_claims) != 1:
        return False
    first_non_whitespace = len(payload) - len(payload.lstrip())
    if finding_claims[0].start() != first_non_whitespace:
        return False
    if _DIRECT_SUCCESSOR_PAYLOAD_SUFFIX.fullmatch(
        payload[finding_claims[0].end() :]
    ) is None:
        return False
    payload_counts = tuple(
        re.finditer(
            rf"(?<![A-Za-z0-9])(?:{_ANY_COUNT_TOKEN})(?![A-Za-z0-9])",
            payload,
            re.IGNORECASE,
        )
    )
    payload_finding_heads = tuple(
        re.finditer(
            r"(?<![A-Za-z0-9])(?:findings?|gaps?)(?![A-Za-z0-9])",
            payload,
            re.IGNORECASE,
        )
    )
    if len(payload_counts) != 1 or len(payload_finding_heads) != 1:
        return False
    secondary_result = re.search(
        rf"\b{_AUDIT_RESULT_VERB}\b",
        payload,
        re.IGNORECASE,
    )
    if (
        secondary_result is not None
        and secondary_result.start() < finding_claims[0].start()
    ):
        return False
    nested_audit_result = re.search(
        rf"\b(?:the\s+)?{_AUDIT_ACTIVITY_NOUN}\s+{_AUDIT_RESULT_VERB}\b",
        payload,
        re.IGNORECASE,
    )
    return (
        nested_audit_result is None
        or nested_audit_result.start() >= finding_claims[0].start()
    )


def _is_explicit_distinct_successor_finding_claim(
    clause: str,
    marker_pattern: str,
    finding_claim_pattern: re.Pattern[str],
) -> bool:
    marker = (
        rf"(?:{marker_pattern})(?:['’]s)?"
        r"(?:\s+(?:audit|sweep|review|pass|results?|finding\s+set|"
        r"duplicate|corrections?)){0,2}"
    )
    determiner = r"(?:(?:a|an|the)\s+)?"
    direct_subject_patterns = (
        re.compile(
            rf"^\s*{determiner}{_DISTINCT_SUCCESSOR_SUBJECT}\s+"
            rf"(?:after|following|for|of|about|by)\s+{determiner}{marker}\s+"
            rf"{_AUDIT_RESULT_VERB}\b(?P<claim_payload>[^;.!?]*)\s*$",
            re.IGNORECASE,
        ),
        re.compile(
            rf"^\s*{determiner}{_AUDIT_ACTIVITY_NOUN}\s+"
            rf"(?:after|following)\s+{determiner}{marker}\s+"
            rf"{_AUDIT_RESULT_VERB}\b(?P<claim_payload>[^;.!?]*)\s*$",
            re.IGNORECASE,
        ),
        re.compile(
            rf"^\s*{determiner}{_DISTINCT_SUCCESSOR_SUBJECT}\s+"
            rf"{_AUDIT_RESULT_VERB}\b(?P<claim_payload>[^;.!?]*?)\s+"
            rf"(?:after|following)\s+{determiner}{marker}\s*$",
            re.IGNORECASE,
        ),
        re.compile(
            rf"^\s*(?:after|following)\s+(?:"
            rf"correcting\s+{determiner}{marker}"
            rf"|{determiner}{marker}\s+corrections?"
            rf")\s*,\s*{determiner}{_DISTINCT_SUCCESSOR_SUBJECT}\s+"
            rf"{_AUDIT_RESULT_VERB}\b(?P<claim_payload>[^;.!?]*)\s*$",
            re.IGNORECASE,
        ),
    )
    marker_token = re.compile(marker_pattern, re.IGNORECASE)
    if len(tuple(marker_token.finditer(clause))) != 1:
        return False
    for subject_pattern in direct_subject_patterns:
        match = subject_pattern.fullmatch(clause)
        if match is None:
            continue
        payload = match.group("claim_payload")
        return _payload_is_single_direct_finding_claim(
            payload,
            finding_claim_pattern,
        )
    return False


def _is_explicit_markerless_successor_finding_claim(
    clause: str,
    finding_claim_pattern: re.Pattern[str],
) -> bool:
    determiner = r"(?:(?:a|an|the)\s+)?"
    subject_pattern = re.compile(
        rf"^\s*{determiner}{_DISTINCT_SUCCESSOR_SUBJECT}\s+"
        rf"{_AUDIT_RESULT_VERB}\b(?P<claim_payload>[^;.!?]*)\s*$",
        re.IGNORECASE,
    )
    match = subject_pattern.fullmatch(clause)
    if match is None:
        return False
    payload = match.group("claim_payload")
    return _payload_is_single_direct_finding_claim(
        payload,
        finding_claim_pattern,
    )


def _summary_prose_count_patterns(
) -> tuple[tuple[str, re.Pattern[str]], ...]:
    count = _ANY_COUNT_TOKEN
    forward = (
        (
            "capability_node_count_u32",
            re.compile(
                rf"\b{count}\s+capability\s+nodes?\b",
                re.IGNORECASE,
            ),
        ),
        (
            "foundation_node_count_u32",
            re.compile(
                rf"\b{count}\s+foundation\s+(?:nodes?|rows?)\b",
                re.IGNORECASE,
            ),
        ),
        (
            "detailed_node_count_u32",
            re.compile(
                rf"\b{count}\s+detailed\s+nodes?\b",
                re.IGNORECASE,
            ),
        ),
        (
            "phase_status_count_u32",
            re.compile(
                rf"\b{count}(?:-row|\s+structured)?\s+"
                r"phase-status(?:\s+(?:rows?|tuples?))?\b",
                re.IGNORECASE,
            ),
        ),
        (
            "ready_frontier_count_u32",
            re.compile(
                rf"\b{count}\s+(?:ordered\s+)?"
                r"READY(?:[- ]frontier)?\s+(?:rows?|tuples?)\b",
                re.IGNORECASE,
            ),
        ),
        (
            "base_dependency_edge_count_u32",
            re.compile(
                rf"\b{count}\s+(?:unconditional\s+)?"
                r"base(?:[- ]dependency)?\s+edges?\b",
                re.IGNORECASE,
            ),
        ),
        (
            "phase_dependency_edge_count_u32",
            re.compile(
                rf"\b{count}\s+phase"
                r"(?:[- ]dependency\s+edges?|\s+dependencies)\b",
                re.IGNORECASE,
            ),
        ),
        (
            "conditional_edge_count_u32",
            re.compile(
                rf"\b{count}\s+(?:context-bearing\s+)?"
                r"conditional(?:-instance|\s+instance)?\s+edges?\b",
                re.IGNORECASE,
            ),
        ),
        (
            "total_declared_edge_count_u32",
            re.compile(
                rf"(?:\b{count}\s+"
                r"(?:(?:total\s+)?declared(?:\s+base-plus-phase-plus-conditional)?|combined)"
                r"\s+edges?\b|"
                r"\b(?:combined|declared|dependency)\s+graph\s+"
                rf"(?:has|contains)\s+\b{count}\b(?:\s+edges?)?)",
                re.IGNORECASE,
            ),
        ),
        (
            "predicate_entailment_count_u32",
            re.compile(
                rf"\b{count}\s+predicate\s+entailments?\b",
                re.IGNORECASE,
            ),
        ),
        (
            "planning_markdown_count_u32",
            re.compile(
                rf"\b{count}"
                r"(?:(?:-document|\s+documents?)\s+(?:planning\s+)?(?:authority\s+)?inventory"
                r"|\s+planning\s+Markdown\s+(?:files?|documents?))\b",
                re.IGNORECASE,
            ),
        ),
        (
            "frozen_text_artifact_count_u32",
            re.compile(
                rf"\b{count}\s+(?:byte-)?"
                r"frozen(?:\s+text)?\s+artifacts?\b",
                re.IGNORECASE,
            ),
        ),
        (
            "frozen_markdown_audit_count_u32",
            re.compile(
                rf"\b{count}\s+"
                r"(?:frozen\s+)?Markdown\s+audits?\b",
                re.IGNORECASE,
            ),
        ),
        (
            "frozen_json_receipt_count_u32",
            re.compile(
                rf"\b{count}\s+"
                r"(?:frozen\s+)?JSON\s+source\s+receipts?\b",
                re.IGNORECASE,
            ),
        ),
    )
    reverse_nouns = {
        "capability_node_count_u32": r"capability[- ]nodes?",
        "foundation_node_count_u32": r"foundation[- ](?:nodes?|rows?)",
        "detailed_node_count_u32": r"detailed[- ]nodes?",
        "phase_status_count_u32": (
            r"phase[- ]status(?:[- ](?:rows?|tuples?))?"
        ),
        "ready_frontier_count_u32": (
            r"(?:ordered[- ])?READY(?:[- ]frontier)?(?:[- ](?:rows?|tuples?))?"
        ),
        "base_dependency_edge_count_u32": (
            r"(?:unconditional[- ])?base(?:[- ]dependency)?[- ]edges?"
        ),
        "phase_dependency_edge_count_u32": (
            r"phase[- ]dependenc(?:y|ies)(?:[- ]edges?)?"
        ),
        "conditional_edge_count_u32": (
            r"(?:context-bearing[- ])?conditional(?:[- ]instance)?[- ]edges?"
        ),
        "total_declared_edge_count_u32": (
            r"(?:(?:total[- ])?declared(?:[- ]base-plus-phase-plus-conditional)?|combined)"
            r"[- ]edges?"
        ),
        "predicate_entailment_count_u32": r"predicate[- ]entailments?",
        "planning_markdown_count_u32": (
            r"(?:(?:planning[- ])?(?:authority[- ])?inventory|"
            r"planning[- ]Markdown[- ](?:files?|documents?))"
        ),
        "frozen_text_artifact_count_u32": (
            r"(?:byte[- ])?frozen(?:[- ]text)?[- ]artifacts?"
        ),
        "frozen_markdown_audit_count_u32": (
            r"(?:frozen[- ])?Markdown[- ]audits?"
        ),
        "frozen_json_receipt_count_u32": (
            r"(?:frozen[- ])?JSON[- ]source[- ]receipts?"
        ),
    }
    if set(reverse_nouns) != set(PLANNING_SUMMARY_FIELD_ORDER):
        raise PlanningError("reverse planning-count semantic manifest is incomplete")
    return tuple(
        (
            field,
            re.compile(
                rf"(?:{pattern.pattern}|\b(?:{reverse_nouns[field]})"
                rf"(?:\s+count)?\s*(?:is|=|:)\s*\b{count}\b)",
                re.IGNORECASE,
            ),
        )
        for field, pattern in forward
    )


def check_no_duplicate_exact_projections(
    texts: dict[str, str],
    planning_summary: dict[str, int],
) -> None:
    expected_mutable_paths = EXPECTED_PLANNING_PATHS - EXPECTED_FROZEN_TEXT_PATHS
    actual_mutable_paths = frozenset(texts)
    if actual_mutable_paths != expected_mutable_paths:
        missing = sorted(expected_mutable_paths - actual_mutable_paths)
        extra = sorted(actual_mutable_paths - expected_mutable_paths)
        raise PlanningError(
            "sole-projection scan must cover every non-frozen mutable planning authority: "
            f"missing={missing}, extra={extra}"
        )

    for field in PLANNING_SUMMARY_FIELD_ORDER:
        field_token = re.compile(
            rf"(?<![A-Za-z0-9_]){re.escape(field)}(?![A-Za-z0-9_])"
        )
        occurrences = [
            (path, match.group(0))
            for path, text in texts.items()
            for match in field_token.finditer(text)
        ]
        if len(occurrences) != 1:
            raise PlanningError(
                f"exact PlanningCheckerSummaryV1 field {field} must occur exactly once "
                "in its canonical CAPABILITY_TREE table cell; "
                f"occurrences={occurrences}"
            )

    audit_markers = {
        "Loop7ProvenanceV1": (
            (
                "design_research_distillation_audit_loop7.md",
                "loop 7",
                "seventh audit",
                "loop7provenancev1",
            ),
            len(
                _parse_frozen_audit_finding_ids(
                    SEVENTH_AUDIT.read_bytes(), prefix="LG", label="Loop 7 audit"
                )
            ),
            (
                SEVENTH_AUDIT_INPUT_COMMIT,
                SEVENTH_AUDIT_INPUT_TREE,
                SEVENTH_AUDIT_ADDITION_COMMIT,
                SEVENTH_AUDIT_ADDITION_TREE,
                SEVENTH_AUDIT_GIT_BLOB,
            ),
        ),
        "Loop8ProvenanceV1": (
            (
                "design_research_distillation_audit_loop8.md",
                "loop 8",
                "eighth audit",
                "loop8provenancev1",
            ),
            len(
                _parse_frozen_audit_finding_ids(
                    EIGHTH_AUDIT.read_bytes(), prefix="EG", label="Loop 8 audit"
                )
            ),
            (
                EIGHTH_AUDIT_INPUT_COMMIT,
                EIGHTH_AUDIT_INPUT_TREE,
                EIGHTH_AUDIT_ADDITION_COMMIT,
                EIGHTH_AUDIT_ADDITION_TREE,
                EIGHTH_AUDIT_GIT_BLOB,
            ),
        ),
        "Loop9ProvenanceV1": (
            (
                "design_research_distillation_audit_loop9.md",
                "loop 9",
                "ninth audit",
                "loop9provenancev1",
            ),
            len(
                _parse_frozen_audit_finding_ids(
                    NINTH_AUDIT.read_bytes(), prefix="NG", label="Loop 9 audit"
                )
            ),
            (
                NINTH_AUDIT_INPUT_COMMIT,
                NINTH_AUDIT_INPUT_TREE,
                NINTH_AUDIT_ADDITION_COMMIT,
                NINTH_AUDIT_ADDITION_TREE,
                NINTH_AUDIT_GIT_BLOB,
            ),
        ),
        "Loop10ProvenanceV1": (
            (
                "design_research_distillation_audit_loop10.md",
                "loop 10",
                "tenth audit",
                "loop10provenancev1",
            ),
            len(
                _parse_frozen_audit_finding_ids(
                    TENTH_AUDIT.read_bytes(), prefix="TG", label="Loop 10 audit"
                )
            ),
            (
                TENTH_AUDIT_INPUT_COMMIT,
                TENTH_AUDIT_INPUT_TREE,
                TENTH_AUDIT_ADDITION_COMMIT,
                TENTH_AUDIT_ADDITION_TREE,
                TENTH_AUDIT_GIT_BLOB,
            ),
        ),
    }
    for schema_name, (_markers, _expected_count, object_ids) in audit_markers.items():
        for object_id in object_ids:
            recognized_forms = tuple(
                object_id[:length] for length in range(len(object_id), 6, -1)
            )
            object_token = re.compile(
                rf"(?<![0-9a-f])(?:{'|'.join(map(re.escape, recognized_forms))})"
                r"(?![0-9a-f])",
                re.IGNORECASE,
            )
            occurrences = [
                (path, match.group(0))
                for path, text in texts.items()
                for match in object_token.finditer(text)
            ]
            if len(occurrences) != 1:
                raise PlanningError(
                    f"exact Git object for {schema_name} must occur exactly once in its "
                    "canonical HANDOFF table cell; "
                    f"object={object_id}, occurrences={occurrences}"
                )

    for path, text in texts.items():
        for raw_paragraph in re.split(r"\n\s*\n", text):
            paragraph = re.sub(r"[ \t\r\n]+", " ", raw_paragraph).strip()
            lowered = paragraph.lower()
            for schema_name, (markers, _expected_count, _object_ids) in audit_markers.items():
                if not any(marker in lowered for marker in markers):
                    continue
                marker_pattern = (
                    r"(?<![A-Za-z0-9_])(?:"
                    + "|".join(map(re.escape, markers))
                    + r")(?![A-Za-z0-9_])"
                )
                marker_token = re.compile(marker_pattern, re.IGNORECASE)
                count_pattern = re.compile(
                    rf"(?<![A-Za-z0-9])(?:{_ANY_COUNT_TOKEN})(?![A-Za-z0-9])",
                    re.IGNORECASE,
                )
                finding_noun = r"(?:material[\s_-]+)?(?:findings?|gaps?)"
                finding_label = rf"{finding_noun}(?:\s+(?:count|total))?"
                finding_count_field = (
                    r"(?:material[ _-]+)?(?:finding|gap)[ _-]+count"
                    r"(?:[ _-]+u32)?"
                )
                count_finding_barrier = (
                    r"(?:and|or|but|with|without|from|for|in|of|by|after|before|"
                    r"because|while|whereas|that|which|who|whose|"
                    r"has|have|had|is|are|was|were|found|contains?|records?|"
                    r"reports?|shows?|showed|yields?|yielded|produces?|produced|"
                    r"returns?|returned|identifies?|identified|exposes?|exposed|"
                    r"audits?|reviews?|reviewers?|sweeps?|pass(?:es)?|(?:re-?)?audits?|"
                    r"entries?|items?|members?|rows?|nodes?|schemas?|parameters?|fields?|"
                    r"files?|commits?|trees?|proofs?|tests?)"
                )
                count_then_finding = (
                    rf"(?<![A-Za-z0-9_])(?:{_ANY_COUNT_TOKEN})"
                    rf"(?:[\s_-]+(?!(?:findings?|gaps?)(?![A-Za-z0-9_]))"
                    rf"(?!(?:{_ANY_COUNT_TOKEN})(?![A-Za-z0-9_]))"
                    rf"(?!(?:{count_finding_barrier})(?![A-Za-z0-9]))"
                    rf"[A-Za-z][A-Za-z0-9_-]*)*[\s_-]+"
                    rf"(?:findings?|gaps?)(?![A-Za-z0-9_])"
                )
                finding_then_count = (
                    rf"(?<![A-Za-z0-9_]){finding_label}\s*[\])]?\s*"
                    rf"(?:is|are|was|were|=|:)\s*"
                    rf"(?<![A-Za-z0-9_])(?:{_ANY_COUNT_TOKEN})(?![A-Za-z0-9_])"
                )
                field_then_count = (
                    rf"(?<![A-Za-z0-9_]){finding_count_field}(?![A-Za-z0-9_])\s*"
                    rf"(?:is|are|was|were|=|:)\s*"
                    rf"(?<![A-Za-z0-9_])(?:{_ANY_COUNT_TOKEN})(?![A-Za-z0-9_])"
                )
                marker_suffix = (
                    rf"{marker_pattern}(?:['’]s)?"
                    r"(?:\s+(?:audit|sweep|review|pass|results?|finding\s+set|"
                    r"duplicate|corrections?)){0,2}"
                )
                reverse_marker_count = (
                    rf"(?<![A-Za-z0-9_])(?:{finding_label}|{finding_count_field})\s+"
                    rf"(?:for|in|of)\s+{marker_suffix}\s*"
                    rf"(?:is|are|was|were|=|:)\s*"
                    rf"(?<![A-Za-z0-9_])(?:{_ANY_COUNT_TOKEN})(?![A-Za-z0-9_])"
                )
                finding_claim_pattern = re.compile(
                    rf"(?:{count_then_finding}|{finding_then_count}|"
                    rf"{field_then_count}|{reverse_marker_count})",
                    re.IGNORECASE,
                )
                direct_successor_modifier = (
                    r"(?:new|newly|discovered|material|open|unresolved|remaining|"
                    r"residual|additional|further|finite|exact|distinct|unique|"
                    r"reported|recorded|falsely|closed|false-closure|false-closed|"
                    r"(?:P|L|v)[0-9]+)"
                )
                direct_successor_count_then_finding = (
                    rf"(?<![A-Za-z0-9_])(?:{_ANY_COUNT_TOKEN})"
                    rf"(?:[\s_-]+{direct_successor_modifier})*[\s_-]+"
                    rf"(?:findings?|gaps?)(?![A-Za-z0-9_])"
                )
                direct_successor_finding_claim_pattern = re.compile(
                    rf"(?:{direct_successor_count_then_finding}|"
                    rf"{finding_then_count}|{field_then_count})",
                    re.IGNORECASE,
                )
                unsupported_claim = None
                for raw_sentence in re.split(r"(?<=[.!?])\s+", paragraph):
                    sentence = re.sub(r"[.!?]+\s*$", "", raw_sentence).strip()
                    sentence_has_marker = marker_token.search(sentence) is not None
                    inherits_marker = False
                    for clause in sentence.split(";"):
                        marker_matches = tuple(marker_token.finditer(clause))
                        claim_match = finding_claim_pattern.search(clause)
                        external_count_exists = any(
                            not any(
                                count_match.start() < marker_match.end()
                                and marker_match.start() < count_match.end()
                                for marker_match in marker_matches
                            )
                            for count_match in count_pattern.finditer(clause)
                        )
                        if marker_matches:
                            inherits_marker = True
                        if claim_match is None:
                            continue
                        if marker_matches and not external_count_exists:
                            continue
                        if marker_matches:
                            supported = _is_explicit_distinct_successor_finding_claim(
                                clause,
                                marker_pattern,
                                direct_successor_finding_claim_pattern,
                            )
                        elif sentence_has_marker:
                            supported = (
                                inherits_marker
                                and _is_explicit_markerless_successor_finding_claim(
                                    clause,
                                    direct_successor_finding_claim_pattern,
                                )
                            )
                        else:
                            continue
                        if not supported:
                            unsupported_claim = clause
                            break
                    if unsupported_claim is not None:
                        break
                if unsupported_claim is not None:
                    raise PlanningError(
                        f"{path}: exact finding count for {schema_name} must live only in its "
                        "canonical HANDOFF projection"
                    )

                object_field_separator = r"[ _-]+"
                object_field_pattern = (
                    rf"(?:audited{object_field_separator}input{object_field_separator}"
                    rf"(?:commit|tree)|addition{object_field_separator}(?:commit|tree)|"
                    rf"first{object_field_separator}addition{object_field_separator}commit|"
                    rf"audit(?:{object_field_separator}git)?{object_field_separator}blob)"
                )
                forward_full_claim_pattern = re.compile(
                    rf"{marker_pattern}[^.\n]{{0,100}}{object_field_pattern}"
                    r"[^.\n]{0,60}(?<![0-9a-f])([0-9a-f]{40})(?![0-9a-f])",
                    flags=re.IGNORECASE,
                )
                reverse_full_claim_pattern = re.compile(
                    rf"{object_field_pattern}[^.\n]{{0,60}}"
                    r"(?<![0-9a-f])([0-9a-f]{40})(?![0-9a-f])"
                    rf"[^.\n]{{0,100}}{marker_pattern}",
                    flags=re.IGNORECASE,
                )
                wrong_claim = forward_full_claim_pattern.search(paragraph)
                if wrong_claim is None:
                    wrong_claim = reverse_full_claim_pattern.search(paragraph)
                if wrong_claim is not None:
                    raise PlanningError(
                        f"{path}: non-canonical Git object claim for {schema_name}: "
                        f"{wrong_claim.group(1)}"
                    )

                forward_abbreviated_claim_pattern = re.compile(
                    rf"{marker_pattern}[^.\n]{{0,100}}{object_field_pattern}"
                    r"[^.\n]{0,60}(?<![0-9a-f])([0-9a-f]{7,39})(?![0-9a-f])",
                    flags=re.IGNORECASE,
                )
                reverse_abbreviated_claim_pattern = re.compile(
                    rf"{object_field_pattern}[^.\n]{{0,60}}"
                    r"(?<![0-9a-f])([0-9a-f]{7,39})(?![0-9a-f])"
                    rf"[^.\n]{{0,100}}{marker_pattern}",
                    flags=re.IGNORECASE,
                )
                abbreviated_claim = forward_abbreviated_claim_pattern.search(paragraph)
                if abbreviated_claim is None:
                    abbreviated_claim = reverse_abbreviated_claim_pattern.search(paragraph)
                if abbreviated_claim is not None:
                    raise PlanningError(
                        f"{path}: abbreviated Git object claim for {schema_name} is disallowed: "
                        f"{abbreviated_claim.group(1)}"
                    )

    for path, text in texts.items():
        for raw_paragraph in re.split(r"\n\s*\n", text):
            paragraph = re.sub(r"[ \t\r\n]+", " ", raw_paragraph).strip()
            for field, pattern in _summary_prose_count_patterns():
                match = pattern.search(paragraph)
                if match is not None:
                    raise PlanningError(
                        f"{path}: exact planning count must project only through "
                        f"PlanningCheckerSummaryV1: field={field}, text={match.group(0)}"
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


def check_frozen_sixth_audit(data: bytes) -> None:
    header = f"blob {len(data)}\0".encode("ascii")
    actual = hashlib.sha1(header + data).hexdigest()
    if actual != SIXTH_AUDIT_GIT_BLOB:
        raise PlanningError(
            "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP6.md changed after its frozen first "
            f"addition: expected Git blob {SIXTH_AUDIT_GIT_BLOB}, got {actual}"
        )


def check_frozen_seventh_audit(data: bytes) -> None:
    header = f"blob {len(data)}\0".encode("ascii")
    actual = hashlib.sha1(header + data).hexdigest()
    if actual != SEVENTH_AUDIT_GIT_BLOB:
        raise PlanningError(
            "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP7.md changed after its frozen first "
            f"addition: expected Git blob {SEVENTH_AUDIT_GIT_BLOB}, got {actual}"
        )


def check_frozen_eighth_audit(data: bytes) -> None:
    header = f"blob {len(data)}\0".encode("ascii")
    actual = hashlib.sha1(header + data).hexdigest()
    if actual != EIGHTH_AUDIT_GIT_BLOB:
        raise PlanningError(
            "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP8.md changed after its frozen first "
            f"addition: expected Git blob {EIGHTH_AUDIT_GIT_BLOB}, got {actual}"
        )


def check_frozen_ninth_audit(data: bytes) -> None:
    header = f"blob {len(data)}\0".encode("ascii")
    actual = hashlib.sha1(header + data).hexdigest()
    if actual != NINTH_AUDIT_GIT_BLOB:
        raise PlanningError(
            "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP9.md changed after its frozen first "
            f"addition: expected Git blob {NINTH_AUDIT_GIT_BLOB}, got {actual}"
        )


def check_frozen_tenth_audit(data: bytes) -> None:
    header = f"blob {len(data)}\0".encode("ascii")
    actual = hashlib.sha1(header + data).hexdigest()
    if actual != TENTH_AUDIT_GIT_BLOB:
        raise PlanningError(
            "docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP10.md changed after its frozen "
            f"first addition: expected Git blob {TENTH_AUDIT_GIT_BLOB}, got {actual}"
        )


def _parse_frozen_audit_finding_ids(
    data: bytes,
    *,
    prefix: str,
    label: str,
) -> tuple[str, ...]:
    """Derive finding cardinality from one exact contiguous frozen audit table."""

    try:
        lines = data.decode("utf-8").splitlines()
    except UnicodeDecodeError as error:
        raise PlanningError(f"{label} is not valid UTF-8") from error
    headers = [
        index
        for index, line in enumerate(lines)
        if line == "| ID | Priority | Gap at `e6baa331` | Concrete loss mechanism | One-sweep correction |"
        or (
            line.startswith("| ID | Priority | Gap at `")
            and (
                line.endswith("| Concrete loss mechanism | One-sweep correction |")
                or line.endswith("| Concrete loss mechanism | Candidate correction |")
            )
        )
    ]
    if len(headers) != 1:
        raise PlanningError(
            f"{label} must contain exactly one frozen finding table, got {len(headers)}"
        )
    header_index = headers[0]
    if header_index + 1 >= len(lines) or not re.fullmatch(
        r"\|(?: --- \|){5}", lines[header_index + 1]
    ):
        raise PlanningError(f"{label} frozen finding table separator is not canonical")

    finding_ids: list[str] = []
    index = header_index + 2
    row_pattern = re.compile(r"^\| ([A-Z]{2}-[0-9]{2}) \|")
    while index < len(lines) and lines[index].startswith("|"):
        match = row_pattern.match(lines[index])
        if match is None:
            raise PlanningError(
                f"{label} frozen finding row {index + 1} is not canonical: {lines[index]}"
            )
        finding_ids.append(match.group(1))
        index += 1
    if not finding_ids:
        raise PlanningError(f"{label} frozen finding table is empty")
    expected = tuple(
        f"{prefix}-{ordinal:02d}" for ordinal in range(1, len(finding_ids) + 1)
    )
    if tuple(finding_ids) != expected:
        raise PlanningError(
            f"{label} finding IDs must be unique and contiguous from {prefix}-01: "
            f"expected={list(expected)}, got={finding_ids}"
        )
    return tuple(finding_ids)


def _git_rev_parse(revision: str) -> str:
    try:
        result = subprocess.run(
            ["git", "rev-parse", "--verify", revision],
            cwd=ROOT,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
    except (OSError, subprocess.CalledProcessError) as error:
        detail = getattr(error, "stderr", "").strip()
        raise PlanningError(
            f"cannot resolve historical Git object {revision}: {detail or error}"
        ) from error
    return result.stdout.strip()


def _git_is_ancestor(ancestor: str, checked_ref: str) -> bool:
    try:
        result = subprocess.run(
            ["git", "merge-base", "--is-ancestor", ancestor, checked_ref],
            cwd=ROOT,
            check=False,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            text=True,
        )
    except OSError as error:
        raise PlanningError(
            f"cannot test Git ancestry {ancestor} -> {checked_ref}: {error}"
        ) from error
    if result.returncode not in (0, 1):
        raise PlanningError(
            f"cannot test Git ancestry {ancestor} -> {checked_ref}: "
            f"{result.stderr.strip() or f'exit {result.returncode}'}"
        )
    return result.returncode == 0


def _require_audit_addition_reachable(
    *,
    label: str,
    addition_commit: str,
    checked_ref: str,
    ancestor_probe: Callable[[str, str], bool],
) -> None:
    if not ancestor_probe(addition_commit, checked_ref):
        raise PlanningError(
            f"{label} first-addition commit {addition_commit} is not an ancestor of "
            f"the explicitly checked ref {checked_ref}"
        )


def _git_commit_parents(commit: str) -> tuple[str, ...]:
    try:
        result = subprocess.run(
            ["git", "show", "-s", "--format=%P", commit],
            cwd=ROOT,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
    except (OSError, subprocess.CalledProcessError) as error:
        detail = getattr(error, "stderr", "").strip()
        raise PlanningError(
            f"cannot resolve Git parents for {commit}: {detail or error}"
        ) from error
    parents = tuple(result.stdout.strip().split())
    if not parents or any(re.fullmatch(r"[0-9a-f]{40}", parent) is None for parent in parents):
        raise PlanningError(
            f"Git parent list for {commit} is empty or malformed: {parents}"
        )
    if len(set(parents)) != len(parents):
        raise PlanningError(f"Git parent list for {commit} contains duplicates: {parents}")
    return parents


def _git_tree_path_exists(
    parent: str,
    relative_path: str,
    *,
    run_command: Callable[..., subprocess.CompletedProcess[bytes]] = subprocess.run,
) -> bool:
    try:
        result = run_command(
            ["git", "ls-tree", "-z", "--name-only", parent, "--", relative_path],
            cwd=ROOT,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except (OSError, subprocess.CalledProcessError) as error:
        raw_detail = getattr(error, "stderr", b"")
        detail = (
            raw_detail.decode("utf-8", errors="replace").strip()
            if isinstance(raw_detail, bytes)
            else str(raw_detail).strip()
        )
        raise PlanningError(
            f"cannot inspect Git parent tree {parent}:{relative_path}: {detail or error}"
        ) from error
    if result.stdout == b"":
        return False
    expected = relative_path.encode("utf-8") + b"\0"
    if result.stdout != expected:
        raise PlanningError(
            f"git ls-tree returned malformed path evidence for {parent}:{relative_path}: "
            f"{result.stdout!r}"
        )
    return True


def _check_audit_addition_identity(
    *,
    label: str,
    path: Path,
    input_commit: str,
    input_tree: str,
    addition_commit: str,
    addition_tree: str,
    addition_blob: str,
    finding_prefix: str,
    checked_ref: str,
    ancestor_probe: Callable[[str, str], bool],
    parent_resolver: Callable[[str], tuple[str, ...]],
    parent_path_probe: Callable[[str, str], bool],
) -> dict[str, str]:
    resolved_input = _git_rev_parse(f"{input_commit}^{{commit}}")
    if resolved_input != input_commit:
        raise PlanningError(
            f"{label} audited input commit mismatch: expected {input_commit}, got {resolved_input}"
        )
    resolved_input_tree = _git_rev_parse(f"{input_commit}^{{tree}}")
    if resolved_input_tree != input_tree:
        raise PlanningError(
            f"{label} audited input tree mismatch: expected {input_tree}, got {resolved_input_tree}"
        )
    commit = _git_rev_parse(f"{addition_commit}^{{commit}}")
    if commit != addition_commit:
        raise PlanningError(
            f"{label} addition commit mismatch: expected {addition_commit}, got {commit}"
        )
    parents = parent_resolver(addition_commit)
    if (
        not parents
        or any(re.fullmatch(r"[0-9a-f]{40}", parent) is None for parent in parents)
        or len(set(parents)) != len(parents)
    ):
        raise PlanningError(f"{label} exact parent set is empty or malformed: {parents}")
    if input_commit not in parents:
        raise PlanningError(
            f"{label} addition parent mismatch: audited input {input_commit} is not in "
            f"the exact parent set {parents}"
        )
    tree = _git_rev_parse(f"{addition_commit}^{{tree}}")
    if tree != addition_tree:
        raise PlanningError(
            f"{label} addition tree mismatch: expected {addition_tree}, got {tree}"
        )
    relative_path = repository_relative_path(path)
    blob = _git_rev_parse(f"{addition_commit}:{relative_path}")
    if blob != addition_blob:
        raise PlanningError(
            f"{label} addition blob mismatch: expected {addition_blob}, got {blob}"
        )
    for parent in parents:
        if parent_path_probe(parent, relative_path):
            raise PlanningError(
                f"{label} existed on parent {parent} before its declared first-addition commit"
            )
    _require_audit_addition_reachable(
        label=label,
        addition_commit=addition_commit,
        checked_ref=checked_ref,
        ancestor_probe=ancestor_probe,
    )
    finding_ids = _parse_frozen_audit_finding_ids(
        path.read_bytes(),
        prefix=finding_prefix,
        label=label,
    )
    return {
        "audited_input_commit": input_commit,
        "audited_input_tree": input_tree,
        "addition_commit": addition_commit,
        "addition_tree": addition_tree,
        "audit_git_blob": addition_blob,
        "material_finding_count_u32": str(len(finding_ids)),
    }


def check_seventh_audit_addition_identity(
    *,
    checked_ref: str = CHECKED_GIT_REF,
    ancestor_probe: Callable[[str, str], bool] = _git_is_ancestor,
    parent_resolver: Callable[[str], tuple[str, ...]] = _git_commit_parents,
    parent_path_probe: Callable[[str, str], bool] = _git_tree_path_exists,
) -> dict[str, str]:
    return _check_audit_addition_identity(
        label="Loop 7 audit",
        path=SEVENTH_AUDIT,
        input_commit=SEVENTH_AUDIT_INPUT_COMMIT,
        input_tree=SEVENTH_AUDIT_INPUT_TREE,
        addition_commit=SEVENTH_AUDIT_ADDITION_COMMIT,
        addition_tree=SEVENTH_AUDIT_ADDITION_TREE,
        addition_blob=SEVENTH_AUDIT_GIT_BLOB,
        finding_prefix="LG",
        checked_ref=checked_ref,
        ancestor_probe=ancestor_probe,
        parent_resolver=parent_resolver,
        parent_path_probe=parent_path_probe,
    )


def check_eighth_audit_addition_identity(
    *,
    checked_ref: str = CHECKED_GIT_REF,
    ancestor_probe: Callable[[str, str], bool] = _git_is_ancestor,
    parent_resolver: Callable[[str], tuple[str, ...]] = _git_commit_parents,
    parent_path_probe: Callable[[str, str], bool] = _git_tree_path_exists,
) -> dict[str, str]:
    return _check_audit_addition_identity(
        label="Loop 8 audit",
        path=EIGHTH_AUDIT,
        input_commit=EIGHTH_AUDIT_INPUT_COMMIT,
        input_tree=EIGHTH_AUDIT_INPUT_TREE,
        addition_commit=EIGHTH_AUDIT_ADDITION_COMMIT,
        addition_tree=EIGHTH_AUDIT_ADDITION_TREE,
        addition_blob=EIGHTH_AUDIT_GIT_BLOB,
        finding_prefix="EG",
        checked_ref=checked_ref,
        ancestor_probe=ancestor_probe,
        parent_resolver=parent_resolver,
        parent_path_probe=parent_path_probe,
    )


def check_ninth_audit_addition_identity(
    *,
    checked_ref: str = CHECKED_GIT_REF,
    ancestor_probe: Callable[[str, str], bool] = _git_is_ancestor,
    parent_resolver: Callable[[str], tuple[str, ...]] = _git_commit_parents,
    parent_path_probe: Callable[[str, str], bool] = _git_tree_path_exists,
) -> dict[str, str]:
    return _check_audit_addition_identity(
        label="Loop 9 audit",
        path=NINTH_AUDIT,
        input_commit=NINTH_AUDIT_INPUT_COMMIT,
        input_tree=NINTH_AUDIT_INPUT_TREE,
        addition_commit=NINTH_AUDIT_ADDITION_COMMIT,
        addition_tree=NINTH_AUDIT_ADDITION_TREE,
        addition_blob=NINTH_AUDIT_GIT_BLOB,
        finding_prefix="NG",
        checked_ref=checked_ref,
        ancestor_probe=ancestor_probe,
        parent_resolver=parent_resolver,
        parent_path_probe=parent_path_probe,
    )


def check_tenth_audit_addition_identity(
    *,
    checked_ref: str = CHECKED_GIT_REF,
    ancestor_probe: Callable[[str, str], bool] = _git_is_ancestor,
    parent_resolver: Callable[[str], tuple[str, ...]] = _git_commit_parents,
    parent_path_probe: Callable[[str, str], bool] = _git_tree_path_exists,
) -> dict[str, str]:
    return _check_audit_addition_identity(
        label="Loop 10 audit",
        path=TENTH_AUDIT,
        input_commit=TENTH_AUDIT_INPUT_COMMIT,
        input_tree=TENTH_AUDIT_INPUT_TREE,
        addition_commit=TENTH_AUDIT_ADDITION_COMMIT,
        addition_tree=TENTH_AUDIT_ADDITION_TREE,
        addition_blob=TENTH_AUDIT_GIT_BLOB,
        finding_prefix="TG",
        checked_ref=checked_ref,
        ancestor_probe=ancestor_probe,
        parent_resolver=parent_resolver,
        parent_path_probe=parent_path_probe,
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


def check_loop6_paper_fragments(text: str) -> None:
    for finding_id, fragments in EXPECTED_LOOP6_PAPER_FRAGMENTS.items():
        for fragment in fragments:
            if text.count(fragment) != 1:
                raise PlanningError(
                    f"LAYER1_DESIGN_PAPER.md must contain exactly one {finding_id} protocol "
                    f"fragment: {fragment}"
                )


def check_loop7_paper_fragments(text: str) -> None:
    for finding_id, fragments in EXPECTED_LOOP7_PAPER_FRAGMENTS.items():
        for fragment in fragments:
            if text.count(fragment) != 1:
                raise PlanningError(
                    f"LAYER1_DESIGN_PAPER.md must contain exactly one {finding_id} protocol "
                    f"fragment: {fragment}"
                )


def check_loop8_paper_fragments(text: str) -> None:
    for finding_id, fragments in EXPECTED_LOOP8_PAPER_FRAGMENTS.items():
        for fragment in fragments:
            if text.count(fragment) < 1:
                raise PlanningError(
                    f"LAYER1_DESIGN_PAPER.md must contain at least one {finding_id} protocol "
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
    enforce_manifest: bool = True,
    expected_message: str | None = None,
) -> None:
    try:
        parse_capability(text, enforce_manifest=enforce_manifest)
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


def expect_frozen_sixth_audit_failure(label: str, data: bytes) -> None:
    try:
        check_frozen_sixth_audit(data)
    except PlanningError:
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_frozen_seventh_audit_failure(label: str, data: bytes) -> None:
    try:
        check_frozen_seventh_audit(data)
    except PlanningError:
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_frozen_eighth_audit_failure(label: str, data: bytes) -> None:
    try:
        check_frozen_eighth_audit(data)
    except PlanningError:
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_frozen_ninth_audit_failure(label: str, data: bytes) -> None:
    try:
        check_frozen_ninth_audit(data)
    except PlanningError:
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_frozen_tenth_audit_failure(label: str, data: bytes) -> None:
    try:
        check_frozen_tenth_audit(data)
    except PlanningError:
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_audit_finding_id_failure(
    label: str,
    data: bytes,
    *,
    prefix: str,
    expected_message: str,
) -> None:
    try:
        _parse_frozen_audit_finding_ids(data, prefix=prefix, label=label)
    except PlanningError as error:
        if expected_message not in str(error):
            raise PlanningError(
                f"negative control failed by the wrong mechanism: {label}: {error}"
            ) from error
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


def expect_loop6_paper_fragment_failure(label: str, text: str) -> None:
    try:
        check_loop6_paper_fragments(text)
    except PlanningError:
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_loop7_paper_fragment_failure(label: str, text: str) -> None:
    try:
        check_loop7_paper_fragments(text)
    except PlanningError:
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_loop8_paper_fragment_failure(label: str, text: str) -> None:
    try:
        check_loop8_paper_fragments(text)
    except PlanningError:
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_loop8_exact_schema_failure(
    label: str,
    text: str,
    schema_name: str,
    *,
    expected_message: str,
) -> None:
    try:
        check_loop8_exact_schemas(text, (schema_name,))
    except PlanningError as error:
        if expected_message not in str(error):
            raise PlanningError(
                f"negative control failed by the wrong mechanism: {label}: {error}"
            ) from error
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_loop9_exact_assignment_failure(
    label: str,
    text: str,
    schema_name: str,
    *,
    expected_message: str,
) -> None:
    try:
        check_loop9_exact_assignments(text, (schema_name,))
    except PlanningError as error:
        if expected_message not in str(error):
            raise PlanningError(
                f"negative control failed by the wrong mechanism: {label}: {error}"
            ) from error
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_compatible_union_failure(
    label: str,
    nodes: dict[str, Node],
    phase_statuses: tuple[PhaseStatus, ...],
    phase_dependencies: tuple[PhaseDependency, ...],
    conditional_edges: tuple[ConditionalEdge, ...],
    *,
    expected_message: str,
) -> None:
    try:
        check_compatible_dependency_union(
            nodes,
            phase_statuses,
            phase_dependencies,
            conditional_edges,
        )
    except PlanningError as error:
        if expected_message not in str(error):
            raise PlanningError(
                f"negative control failed by the wrong mechanism: {label}: {error}"
            ) from error
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_exact_invariant_failure(
    label: str,
    text: str,
    source_label: str,
    *,
    expected_message: str | None = None,
) -> None:
    try:
        check_accepted_parent_exact_invariants(text, source_label)
    except PlanningError as error:
        if expected_message is not None and expected_message not in str(error):
            raise PlanningError(
                f"negative control failed by the wrong mechanism: {label}: {error}"
            ) from error
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_summary_projection_failure(
    label: str,
    text: str,
    expected: dict[str, int],
) -> None:
    try:
        check_planning_checker_summary_projection(text, expected)
    except PlanningError:
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_provenance_projection_failure(
    label: str,
    handoff_text: str,
    schema_name: str,
    expected: dict[str, str],
    *,
    expected_message: str | None = None,
) -> None:
    try:
        check_audit_provenance_projection(handoff_text, schema_name, expected)
    except PlanningError as error:
        if expected_message is not None and expected_message not in str(error):
            raise PlanningError(
                f"negative control failed by the wrong mechanism: {label}: {error}"
            ) from error
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_duplicate_projection_failure(
    label: str,
    texts: dict[str, str],
    planning_summary: dict[str, int],
    *,
    expected_message: str | None = None,
) -> None:
    try:
        check_no_duplicate_exact_projections(texts, planning_summary)
    except PlanningError as error:
        if expected_message is not None and expected_message not in str(error):
            raise PlanningError(
                f"negative control failed by the wrong mechanism: {label}: {error}"
            ) from error
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_ready_frontier_failure(
    label: str,
    text: str,
    *,
    expected_message: str | None = None,
) -> None:
    try:
        nodes = parse_capability(text)
        phase_statuses = parse_phase_statuses(text, nodes)
        parse_ready_frontier(text, phase_statuses)
    except PlanningError as error:
        if expected_message is not None and expected_message not in str(error):
            raise PlanningError(
                f"negative control failed by the wrong mechanism: {label}: {error}"
            ) from error
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_handoff_failure(
    label: str,
    text: str,
    tracked_paths: frozenset[Path],
    *,
    expected_message: str | None = None,
) -> None:
    try:
        check_handoff_read_order_and_counts(text, tracked_paths)
    except PlanningError as error:
        if expected_message is not None and expected_message not in str(error):
            raise PlanningError(
                f"negative control failed by the wrong mechanism: {label}: {error}"
            ) from error
        return
    raise PlanningError(f"negative control did not fail: {label}")


def expect_status_projection_failure(
    label: str,
    text: str,
    *,
    expected_message: str | None = None,
) -> None:
    try:
        nodes = parse_capability(text)
        parse_phase_statuses(text, nodes)
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
    handoff_text: str,
    first_audit_bytes: bytes,
    second_audit_bytes: bytes,
    design_audit_bytes: bytes,
    successor_audit_bytes: bytes,
    third_audit_bytes: bytes,
    fourth_audit_bytes: bytes,
    fifth_audit_bytes: bytes,
    sixth_audit_bytes: bytes,
    seventh_audit_bytes: bytes,
    eighth_audit_bytes: bytes,
    ninth_audit_bytes: bytes,
    tenth_audit_bytes: bytes,
    layer1_design_audit_bytes: bytes,
    design_receipt_bytes: bytes,
    design_text: str,
    layer1_design_paper_text: str,
    design_receipt: dict[str, object],
    tracked_paths: frozenset[Path],
    frozen_eol_policy: dict[str, dict[str, str]],
    planning_summary: dict[str, int],
    loop7_provenance: dict[str, str],
    loop8_provenance: dict[str, str],
    loop9_provenance: dict[str, str],
    loop10_provenance: dict[str, str],
    projection_texts: dict[str, str],
) -> None:
    def mutate_manifest_fragment(
        source: str,
        header: str,
        old: str,
        new: str,
        label: str,
    ) -> str:
        lines = source.splitlines(keepends=True)
        starts = [index for index, line in enumerate(lines) if line.strip() == header]
        if len(starts) != 1:
            raise PlanningError(f"self-test fixture drift: {label} manifest header")
        start = starts[0]
        depth = header.count("(") - header.count(")")
        end: int | None = None
        for index in range(start + 1, len(lines)):
            depth += lines[index].count("(") - lines[index].count(")")
            if depth == 0:
                end = index
                break
        if end is None:
            raise PlanningError(f"self-test fixture drift: {label} manifest close")
        block = "".join(lines[start : end + 1])
        if block.count(old) != 1:
            raise PlanningError(
                f"self-test fixture drift: {label} manifest fragment count {block.count(old)}"
            )
        mutated_block = block.replace(old, new, 1)
        return "".join(lines[:start]) + mutated_block + "".join(lines[end + 1 :])

    def replace_manifest_rows(
        source: str,
        header: str,
        rendered_rows: tuple[str, ...],
        label: str,
    ) -> str:
        lines = source.splitlines(keepends=True)
        starts = [index for index, line in enumerate(lines) if line.strip() == header]
        if len(starts) != 1:
            raise PlanningError(f"self-test fixture drift: {label} manifest header")
        start = starts[0]
        depth = header.count("(") - header.count(")")
        end: int | None = None
        for index in range(start + 1, len(lines)):
            depth += lines[index].count("(") - lines[index].count(")")
            if depth == 0:
                end = index
                break
        if end is None:
            raise PlanningError(f"self-test fixture drift: {label} manifest close")
        newline = "\r\n" if lines[start].endswith("\r\n") else "\n"
        indent_match = re.match(r"[ \t]*", lines[start + 1])
        indent = indent_match.group(0) if indent_match is not None else "    "
        replacement = [f"{indent}{row}{newline}" for row in rendered_rows]
        mutated = "".join(lines[: start + 1] + replacement + lines[end:])
        if mutated == source:
            raise PlanningError(f"self-test fixture drift: {label} unchanged manifest")
        return mutated

    def replace_core_field_manifest(
        source: str,
        fields: tuple[str, ...],
        label: str,
    ) -> str:
        rows = tuple(
            f"{index} => {field}{',' if index < len(fields) else ''}"
            for index, field in enumerate(fields, 1)
        )
        return replace_manifest_rows(
            source,
            "AcceptedProposalCoreFieldManifestV1 = ExactOrderedFieldManifest(",
            rows,
            label,
        )

    def replace_collision_manifest(
        source: str,
        rows: tuple[tuple[str, str, str], ...],
        label: str,
    ) -> str:
        rendered = tuple(
            f"{row_name} = ({collision_class}, {disposition})"
            f"{',' if index < len(rows) else ''}"
            for index, (row_name, collision_class, disposition) in enumerate(rows, 1)
        )
        return replace_manifest_rows(
            source,
            "AcceptedParentCollisionDispositionV1 = CanonicalTwoRowMap(",
            rendered,
            label,
        )

    def render_loop8_schema_assignment(
        schema_name: str,
        variants: tuple[tuple[str, tuple[str, ...]], ...],
    ) -> str:
        rendered_variants = []
        for variant_name, arguments in variants:
            if variant_name == "BareTypeRef":
                if len(arguments) != 1:
                    raise PlanningError(
                        f"self-test fixture drift: {schema_name} malformed bare type variant"
                    )
                rendered_variants.append(arguments[0])
                continue
            if variant_name == "MatchExpression":
                if len(arguments) != 2:
                    raise PlanningError(
                        f"self-test fixture drift: {schema_name} malformed match expression"
                    )
                rendered_variants.append(
                    f"match {arguments[0]} {{{arguments[1]}}}"
                )
                continue
            rendered_arguments = ",\n        ".join(arguments)
            rendered_variants.append(
                f"{variant_name}(\n        {rendered_arguments}\n    )"
                if arguments
                else f"{variant_name}()"
            )
        return f"{schema_name} =\n    " + "\n  | ".join(rendered_variants)

    def replace_loop8_schema_assignment(
        source: str,
        schema_name: str,
        variants: tuple[tuple[str, tuple[str, ...]], ...],
        label: str,
    ) -> str:
        try:
            start, end = _schema_assignment_bounds(source, schema_name)
        except PlanningError as error:
            raise PlanningError(f"self-test fixture drift: {label}: {error}") from error
        replacement = render_loop8_schema_assignment(schema_name, variants)
        if end < len(source):
            replacement += "\n\n"
        mutated = source[:start] + replacement + source[end:]
        if mutated == source:
            raise PlanningError(f"self-test fixture drift: {label} unchanged assignment")
        return mutated

    def replace_exact_assignment_fragment(
        source: str,
        schema_name: str,
        original: str,
        replacement: str,
        label: str,
    ) -> str:
        start, end = _schema_assignment_bounds(source, schema_name)
        assignment = source[start:end]
        if assignment.count(original) != 1:
            raise PlanningError(
                f"self-test fixture drift: {label} fragment count "
                f"{assignment.count(original)}"
            )
        mutated_assignment = assignment.replace(original, replacement, 1)
        return source[:start] + mutated_assignment + source[end:]

    reduced_inventory = tuple(path for path in PLANNING_FILES if path != EIGHTH_AUDIT)
    if len(reduced_inventory) != len(PLANNING_FILES) - 1:
        raise PlanningError("self-test fixture drift: planning authority inventory")
    expect_planning_inventory_failure(
        "missing planning authority document",
        reduced_inventory,
    )

    wrong_eol_policy = {
        path: dict(attributes) for path, attributes in frozen_eol_policy.items()
    }
    eol_probe_path = repository_relative_path(EIGHTH_AUDIT)
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

    expect_frozen_eighth_audit_failure(
        "frozen eighth audit mutation",
        eighth_audit_bytes + b"\nnegative-control",
    )
    expect_frozen_ninth_audit_failure(
        "frozen ninth audit mutation",
        ninth_audit_bytes + b"\nnegative-control",
    )
    expect_frozen_tenth_audit_failure(
        "frozen tenth audit mutation",
        tenth_audit_bytes + b"\nnegative-control",
    )

    for audit_label, audit_bytes, prefix, omitted_id in (
        ("Loop 7 audit", seventh_audit_bytes, "LG", "LG-04"),
        ("Loop 8 audit", eighth_audit_bytes, "EG", "EG-03"),
        ("Loop 9 audit", ninth_audit_bytes, "NG", "NG-05"),
        ("Loop 10 audit", tenth_audit_bytes, "TG", "TG-06"),
    ):
        rows = audit_bytes.splitlines(keepends=True)
        omitted = [row for row in rows if not row.startswith(f"| {omitted_id} |".encode())]
        if len(omitted) != len(rows) - 1:
            raise PlanningError(
                f"self-test fixture drift: {audit_label} finding row {omitted_id}"
            )
        expect_audit_finding_id_failure(
            f"{audit_label} non-contiguous finding table",
            b"".join(omitted),
            prefix=prefix,
            expected_message="finding IDs must be unique and contiguous",
        )

    observed_ancestry_operands: list[tuple[str, str]] = []

    def asserting_unrelated_lineage_probe(addition: str, checked_ref: str) -> bool:
        operands = (addition, checked_ref)
        expected_operands = (NINTH_AUDIT_ADDITION_COMMIT, CHECKED_GIT_REF)
        if operands != expected_operands:
            raise PlanningError(
                "self-test fixture drift: ancestry probe operands "
                f"expected={expected_operands}, got={operands}"
            )
        observed_ancestry_operands.append(operands)
        return False

    try:
        _require_audit_addition_reachable(
            label="Loop 9 audit",
            addition_commit=NINTH_AUDIT_ADDITION_COMMIT,
            checked_ref=CHECKED_GIT_REF,
            ancestor_probe=asserting_unrelated_lineage_probe,
        )
    except PlanningError as error:
        expected_ancestry_mechanism = "is not an ancestor of the explicitly checked ref HEAD"
        if expected_ancestry_mechanism not in str(error):
            raise PlanningError(
                "negative control failed by the wrong mechanism: unrelated Loop 9 "
                f"first-addition lineage: {error}"
            ) from error
    else:
        raise PlanningError(
            "negative control did not fail: unrelated Loop 9 first-addition lineage"
        )
    if observed_ancestry_operands != [
        (NINTH_AUDIT_ADDITION_COMMIT, CHECKED_GIT_REF)
    ]:
        raise PlanningError(
            "self-test fixture drift: ancestry probe was not called exactly once"
        )

    try:
        _require_audit_addition_reachable(
            label="Loop 9 audit real Git reverse-lineage control",
            addition_commit=NINTH_AUDIT_ADDITION_COMMIT,
            checked_ref=NINTH_AUDIT_INPUT_COMMIT,
            ancestor_probe=_git_is_ancestor,
        )
    except PlanningError as error:
        if "is not an ancestor of the explicitly checked ref" not in str(error):
            raise PlanningError(
                "negative control failed by the wrong mechanism: real Git reverse-lineage "
                f"control: {error}"
            ) from error
    else:
        raise PlanningError(
            "negative control did not fail: real Git reverse-lineage control"
        )

    operational_commands: list[tuple[str, ...]] = []

    def simulated_git_operational_error(
        command: list[str],
        **_kwargs: object,
    ) -> subprocess.CompletedProcess[bytes]:
        operational_commands.append(tuple(command))
        raise subprocess.CalledProcessError(
            129,
            command,
            stderr=b"fatal: simulated object database failure",
        )

    def operational_error_parent_path_probe(parent: str, relative_path: str) -> bool:
        return _git_tree_path_exists(
            parent,
            relative_path,
            run_command=simulated_git_operational_error,
        )

    try:
        check_ninth_audit_addition_identity(
            parent_path_probe=operational_error_parent_path_probe,
        )
    except PlanningError as error:
        expected_operational_mechanism = "cannot inspect Git parent tree"
        if (
            expected_operational_mechanism not in str(error)
            or "simulated object database failure" not in str(error)
        ):
            raise PlanningError(
                "negative control failed by the wrong mechanism: Git operational "
                f"first-addition probe: {error}"
            ) from error
    else:
        raise PlanningError(
            "negative control did not fail: Git operational first-addition probe"
        )
    expected_operational_command = (
        "git",
        "ls-tree",
        "-z",
        "--name-only",
        NINTH_AUDIT_INPUT_COMMIT,
        "--",
        repository_relative_path(NINTH_AUDIT),
    )
    if operational_commands != [expected_operational_command]:
        raise PlanningError(
            "self-test fixture drift: operational parent-path command "
            f"expected={[expected_operational_command]}, got={operational_commands}"
        )

    simulated_second_parent = "f" * 40
    observed_merge_parent_paths: list[tuple[str, str]] = []

    def simulated_merge_parents(commit: str) -> tuple[str, ...]:
        if commit != NINTH_AUDIT_ADDITION_COMMIT:
            raise PlanningError(
                "self-test fixture drift: merge parent resolver commit "
                f"expected={NINTH_AUDIT_ADDITION_COMMIT}, got={commit}"
            )
        return NINTH_AUDIT_INPUT_COMMIT, simulated_second_parent

    def second_parent_contains_audit(parent: str, relative_path: str) -> bool:
        observed_merge_parent_paths.append((parent, relative_path))
        return parent == simulated_second_parent

    try:
        check_ninth_audit_addition_identity(
            ancestor_probe=lambda _addition, _checked_ref: True,
            parent_resolver=simulated_merge_parents,
            parent_path_probe=second_parent_contains_audit,
        )
    except PlanningError as error:
        expected_merge_mechanism = (
            f"existed on parent {simulated_second_parent} before its declared "
            "first-addition commit"
        )
        if expected_merge_mechanism not in str(error):
            raise PlanningError(
                "negative control failed by the wrong mechanism: second-parent "
                f"first-addition collision: {error}"
            ) from error
    else:
        raise PlanningError(
            "negative control did not fail: second-parent first-addition collision"
        )
    expected_merge_parent_paths = [
        (NINTH_AUDIT_INPUT_COMMIT, repository_relative_path(NINTH_AUDIT)),
        (simulated_second_parent, repository_relative_path(NINTH_AUDIT)),
    ]
    if observed_merge_parent_paths != expected_merge_parent_paths:
        raise PlanningError(
            "self-test fixture drift: merge topology did not inspect every parent: "
            f"expected={expected_merge_parent_paths}, got={observed_merge_parent_paths}"
        )

    observed_absent_parent_paths: list[tuple[str, str]] = []

    def every_merge_parent_absent(parent: str, relative_path: str) -> bool:
        observed_absent_parent_paths.append((parent, relative_path))
        return False

    multi_parent_provenance = check_ninth_audit_addition_identity(
        ancestor_probe=lambda _addition, _checked_ref: True,
        parent_resolver=simulated_merge_parents,
        parent_path_probe=every_merge_parent_absent,
    )
    if observed_absent_parent_paths != expected_merge_parent_paths:
        raise PlanningError(
            "self-test fixture drift: all-parent absence control did not inspect every parent"
        )
    if multi_parent_provenance["material_finding_count_u32"] != "9":
        raise PlanningError(
            "self-test fixture drift: all-parent absence control changed frozen provenance"
        )

    exact_invariant_sources = (
        ("LAYER1_DESIGN_PAPER.md", layer1_design_paper_text),
        ("CAPABILITY_TREE.md", text),
    )
    first_collision_class = (
        "ByteDistinctAcceptedParentLookupPayloadPreimagesSameProfileIdHashV1OutputV1"
    )
    second_collision_class = (
        "ByteEqualAcceptedParentLookupPayloadAndBlockIdDistinctValidAcceptedProposalCoresV1"
    )
    for source_label, source_text in exact_invariant_sources:
        for omitted_index, omitted_field in enumerate(
            EXPECTED_ACCEPTED_PROPOSAL_CORE_FIELDS
        ):
            remaining_fields = (
                EXPECTED_ACCEPTED_PROPOSAL_CORE_FIELDS[:omitted_index]
                + EXPECTED_ACCEPTED_PROPOSAL_CORE_FIELDS[omitted_index + 1 :]
            )
            missing_field = replace_core_field_manifest(
                source_text,
                remaining_fields,
                f"{source_label} omit core field {omitted_field}",
            )
            expect_exact_invariant_failure(
                f"{source_label} accepted-core field omission {omitted_field}",
                missing_field,
                source_label,
                expected_message="exact ordered fields disagree",
            )

        for insertion_index in range(
            len(EXPECTED_ACCEPTED_PROPOSAL_CORE_FIELDS) + 1
        ):
            added_field = f"negative_control_extra_core_field_{insertion_index + 1}"
            expanded_fields = list(EXPECTED_ACCEPTED_PROPOSAL_CORE_FIELDS)
            expanded_fields.insert(insertion_index, added_field)
            added_core_field = replace_core_field_manifest(
                source_text,
                tuple(expanded_fields),
                f"{source_label} add core field at position {insertion_index + 1}",
            )
            expect_exact_invariant_failure(
                f"{source_label} accepted-core field addition at position "
                f"{insertion_index + 1}",
                added_core_field,
                source_label,
                expected_message="exact ordered fields disagree",
            )

        for left_index in range(len(EXPECTED_ACCEPTED_PROPOSAL_CORE_FIELDS)):
            for right_index in range(
                left_index + 1,
                len(EXPECTED_ACCEPTED_PROPOSAL_CORE_FIELDS),
            ):
                reordered_field_list = list(EXPECTED_ACCEPTED_PROPOSAL_CORE_FIELDS)
                reordered_field_list[left_index], reordered_field_list[right_index] = (
                    reordered_field_list[right_index],
                    reordered_field_list[left_index],
                )
                reordered_fields = replace_core_field_manifest(
                    source_text,
                    tuple(reordered_field_list),
                    f"{source_label} swap core fields {left_index + 1}/{right_index + 1}",
                )
                expect_exact_invariant_failure(
                    f"{source_label} accepted-core field reorder "
                    f"{left_index + 1}/{right_index + 1}",
                    reordered_fields,
                    source_label,
                    expected_message="exact ordered fields disagree",
                )

        for excluded_field in EXPECTED_ACCEPTED_PROPOSAL_CORE_EXCLUDED_FIELDS:
            forbidden_in_core = replace_core_field_manifest(
                source_text,
                EXPECTED_ACCEPTED_PROPOSAL_CORE_FIELDS + (excluded_field,),
                f"{source_label} insert excluded field {excluded_field}",
            )
            expect_exact_invariant_failure(
                f"{source_label} excluded field inserted into accepted core: "
                f"{excluded_field}",
                forbidden_in_core,
                source_label,
                expected_message="exact ordered fields disagree",
            )

        for schema_name in EXACT_ACCEPTED_PARENT_SCHEMA_NAMES:
            duplicate_assignment = (
                source_text
                + f"\n{schema_name} = ConflictingNegativeControlSchemaV1()\n"
            )
            expect_exact_invariant_failure(
                f"{source_label} duplicate schema assignment {schema_name}",
                duplicate_assignment,
                source_label,
                expected_message=f"{schema_name} must have exactly one schema assignment",
            )

        signature_in_declaration = source_text.replace(
            "FieldsExactly(AcceptedProposalCoreFieldManifestV1)",
            "FieldsExactly(AcceptedProposalCoreFieldManifestV1), "
            "canonical_proposer_signature_bytes",
            1,
        )
        if signature_in_declaration == source_text:
            raise PlanningError(
                f"self-test fixture drift: {source_label} direct signature insertion"
            )
        expect_exact_invariant_failure(
            f"{source_label} proposer signature restored in accepted-core declaration",
            signature_in_declaration,
            source_label,
        )

        omitted_collision_class = mutate_manifest_fragment(
            source_text,
            "AcceptedParentCollisionDispositionV1 = CanonicalTwoRowMap(",
            first_collision_class,
            "OmittedAcceptedParentCollisionClassV1",
            f"{source_label} collision omission",
        )
        expect_exact_invariant_failure(
            f"{source_label} collision class omission",
            omitted_collision_class,
            source_label,
        )

        merged_collision_classes = mutate_manifest_fragment(
            source_text,
            "AcceptedParentCollisionDispositionV1 = CanonicalTwoRowMap(",
            second_collision_class,
            first_collision_class,
            f"{source_label} collision merge",
        )
        expect_exact_invariant_failure(
            f"{source_label} collision classes merged",
            merged_collision_classes,
            source_label,
        )

        swapped_first_disposition = mutate_manifest_fragment(
            source_text,
            "AcceptedParentCollisionDispositionV1 = CanonicalTwoRowMap(",
            "ProtocolIdentityHashCompromiseV1",
            "ProfileOwnedAuthorityBindingCompromiseV1",
            f"{source_label} first collision disposition swap",
        )
        expect_exact_invariant_failure(
            f"{source_label} first collision disposition swapped",
            swapped_first_disposition,
            source_label,
        )

        swapped_second_disposition = mutate_manifest_fragment(
            source_text,
            "AcceptedParentCollisionDispositionV1 = CanonicalTwoRowMap(",
            "ProfileOwnedAuthorityBindingCompromiseV1",
            "ProtocolIdentityHashCompromiseV1",
            f"{source_label} second collision disposition swap",
        )
        expect_exact_invariant_failure(
            f"{source_label} second collision disposition swapped",
            swapped_second_disposition,
            source_label,
        )

        collision_header = "AcceptedParentCollisionDispositionV1 = CanonicalTwoRowMap("
        temporary_disposition = "TemporaryCollisionDispositionV1"
        double_swapped = mutate_manifest_fragment(
            source_text,
            collision_header,
            "ProtocolIdentityHashCompromiseV1",
            temporary_disposition,
            f"{source_label} collision double-swap stage one",
        )
        double_swapped = mutate_manifest_fragment(
            double_swapped,
            collision_header,
            "ProfileOwnedAuthorityBindingCompromiseV1",
            "ProtocolIdentityHashCompromiseV1",
            f"{source_label} collision double-swap stage two",
        )
        double_swapped = mutate_manifest_fragment(
            double_swapped,
            collision_header,
            temporary_disposition,
            "ProfileOwnedAuthorityBindingCompromiseV1",
            f"{source_label} collision double-swap stage three",
        )
        expect_exact_invariant_failure(
            f"{source_label} both collision dispositions swapped",
            double_swapped,
            source_label,
        )

        physical_row_reorder = replace_collision_manifest(
            source_text,
            tuple(reversed(EXPECTED_ACCEPTED_PARENT_COLLISION_ROWS)),
            f"{source_label} physical collision-row reorder",
        )
        expect_exact_invariant_failure(
            f"{source_label} physical collision-row reorder",
            physical_row_reorder,
            source_label,
            expected_message="AcceptedParentCollisionDispositionV1 disagrees",
        )

        first_collision_row, second_collision_row = EXPECTED_ACCEPTED_PARENT_COLLISION_ROWS
        label_only_row_reorder = replace_collision_manifest(
            source_text,
            (
                (second_collision_row[0], first_collision_row[1], first_collision_row[2]),
                (first_collision_row[0], second_collision_row[1], second_collision_row[2]),
            ),
            f"{source_label} collision-row label reorder",
        )
        expect_exact_invariant_failure(
            f"{source_label} collision-row label reorder",
            label_only_row_reorder,
            source_label,
            expected_message="AcceptedParentCollisionDispositionV1 disagrees",
        )

        payload_only_row_reorder = replace_collision_manifest(
            source_text,
            (
                (first_collision_row[0], second_collision_row[1], second_collision_row[2]),
                (second_collision_row[0], first_collision_row[1], first_collision_row[2]),
            ),
            f"{source_label} collision-row payload reorder",
        )
        expect_exact_invariant_failure(
            f"{source_label} collision-row payload reorder",
            payload_only_row_reorder,
            source_label,
            expected_message="AcceptedParentCollisionDispositionV1 disagrees",
        )

    for finding_id, fragments in EXPECTED_LOOP8_PAPER_FRAGMENTS.items():
        for index, fragment in enumerate(fragments, 1):
            mutated_paper = layer1_design_paper_text.replace(
                fragment,
                f"mutated-{finding_id.lower()}-paper-fragment-{index}",
            )
            if mutated_paper == layer1_design_paper_text:
                raise PlanningError(
                    f"self-test fixture drift: {finding_id} paper fragment {index}"
                )
            expect_loop8_paper_fragment_failure(
                f"{finding_id} paper fragment mutation {index}",
                mutated_paper,
            )

    exact_schema_failure_message = "exact ordered variants/fields disagree"
    for schema_name, expected_variants in EXPECTED_LOOP8_EXACT_SCHEMA_VARIANTS.items():
        duplicate_assignment = (
            layer1_design_paper_text
            + f"\n\n{schema_name} = ContradictoryLoop8NegativeControlV1()\n"
        )
        expect_loop8_exact_schema_failure(
            f"{schema_name} duplicate contradictory assignment",
            duplicate_assignment,
            schema_name,
            expected_message="must have exactly one assignment",
        )

        if len(expected_variants) > 1:
            for omitted_variant_index, (variant_name, _arguments) in enumerate(
                expected_variants
            ):
                omitted_variant = (
                    expected_variants[:omitted_variant_index]
                    + expected_variants[omitted_variant_index + 1 :]
                )
                mutated_paper = replace_loop8_schema_assignment(
                    layer1_design_paper_text,
                    schema_name,
                    omitted_variant,
                    f"{schema_name} omit variant {variant_name}",
                )
                expect_loop8_exact_schema_failure(
                    f"{schema_name} variant omission {variant_name}",
                    mutated_paper,
                    schema_name,
                    expected_message=exact_schema_failure_message,
                )

            for left_variant in range(len(expected_variants)):
                for right_variant in range(left_variant + 1, len(expected_variants)):
                    reordered_variant_list = list(expected_variants)
                    reordered_variant_list[left_variant], reordered_variant_list[right_variant] = (
                        reordered_variant_list[right_variant],
                        reordered_variant_list[left_variant],
                    )
                    mutated_paper = replace_loop8_schema_assignment(
                        layer1_design_paper_text,
                        schema_name,
                        tuple(reordered_variant_list),
                        f"{schema_name} reorder variants {left_variant + 1}/{right_variant + 1}",
                    )
                    expect_loop8_exact_schema_failure(
                        f"{schema_name} variant reorder "
                        f"{left_variant + 1}/{right_variant + 1}",
                        mutated_paper,
                        schema_name,
                        expected_message=exact_schema_failure_message,
                    )

        for variant_insertion_index in range(len(expected_variants) + 1):
            added_variant_list = list(expected_variants)
            added_variant_list.insert(
                variant_insertion_index,
                (
                    "UnexpectedLoop8NegativeControlVariantV1",
                    ("unexpected_loop8_negative_control_field",),
                ),
            )
            mutated_paper = replace_loop8_schema_assignment(
                layer1_design_paper_text,
                schema_name,
                tuple(added_variant_list),
                f"{schema_name} add variant at {variant_insertion_index + 1}",
            )
            expect_loop8_exact_schema_failure(
                f"{schema_name} variant addition at {variant_insertion_index + 1}",
                mutated_paper,
                schema_name,
                expected_message=exact_schema_failure_message,
            )

        for variant_index, (variant_name, arguments) in enumerate(expected_variants):
            for omitted_argument_index, omitted_argument in enumerate(arguments):
                remaining_arguments = (
                    arguments[:omitted_argument_index]
                    + arguments[omitted_argument_index + 1 :]
                )
                mutated_variants = list(expected_variants)
                mutated_variants[variant_index] = (variant_name, remaining_arguments)
                mutated_paper = replace_loop8_schema_assignment(
                    layer1_design_paper_text,
                    schema_name,
                    tuple(mutated_variants),
                    f"{schema_name}.{variant_name} omit {omitted_argument}",
                )
                expect_loop8_exact_schema_failure(
                    f"{schema_name}.{variant_name} field omission {omitted_argument}",
                    mutated_paper,
                    schema_name,
                    expected_message=exact_schema_failure_message,
                )

            for argument_insertion_index in range(len(arguments) + 1):
                added_arguments = list(arguments)
                added_arguments.insert(
                    argument_insertion_index,
                    f"unexpected_loop8_field_{variant_index + 1}_{argument_insertion_index + 1}",
                )
                mutated_variants = list(expected_variants)
                mutated_variants[variant_index] = (variant_name, tuple(added_arguments))
                mutated_paper = replace_loop8_schema_assignment(
                    layer1_design_paper_text,
                    schema_name,
                    tuple(mutated_variants),
                    f"{schema_name}.{variant_name} add field at "
                    f"{argument_insertion_index + 1}",
                )
                expect_loop8_exact_schema_failure(
                    f"{schema_name}.{variant_name} field addition at "
                    f"{argument_insertion_index + 1}",
                    mutated_paper,
                    schema_name,
                    expected_message=exact_schema_failure_message,
                )

            for left_argument in range(len(arguments)):
                for right_argument in range(left_argument + 1, len(arguments)):
                    reordered_arguments = list(arguments)
                    reordered_arguments[left_argument], reordered_arguments[right_argument] = (
                        reordered_arguments[right_argument],
                        reordered_arguments[left_argument],
                    )
                    mutated_variants = list(expected_variants)
                    mutated_variants[variant_index] = (
                        variant_name,
                        tuple(reordered_arguments),
                    )
                    mutated_paper = replace_loop8_schema_assignment(
                        layer1_design_paper_text,
                        schema_name,
                        tuple(mutated_variants),
                        f"{schema_name}.{variant_name} reorder fields "
                        f"{left_argument + 1}/{right_argument + 1}",
                    )
                    expect_loop8_exact_schema_failure(
                        f"{schema_name}.{variant_name} field reorder "
                        f"{left_argument + 1}/{right_argument + 1}",
                        mutated_paper,
                        schema_name,
                        expected_message=exact_schema_failure_message,
                    )

    loop9_exact_failure_message = "exact ordered variants/fields disagree"
    for schema_name in EXPECTED_LOOP9_EXACT_ASSIGNMENT_SHA256:
        expected_variants = _parse_exact_schema_assignment(
            layer1_design_paper_text,
            schema_name,
        )
        baseline_assignment = render_loop8_schema_assignment(
            schema_name,
            expected_variants,
        )
        check_loop9_exact_assignments(baseline_assignment, (schema_name,))
        duplicate_assignment = (
            baseline_assignment
            + f"\n\n{schema_name} = ContradictoryLoop9NegativeControlV1()\n"
        )
        expect_loop9_exact_assignment_failure(
            f"{schema_name} duplicate Loop 9 assignment",
            duplicate_assignment,
            schema_name,
            expected_message="must have exactly one assignment",
        )

        if len(expected_variants) > 1:
            for omitted_variant_index, (variant_name, _arguments) in enumerate(
                expected_variants
            ):
                omitted_variants = (
                    expected_variants[:omitted_variant_index]
                    + expected_variants[omitted_variant_index + 1 :]
                )
                mutated_paper = replace_loop8_schema_assignment(
                    baseline_assignment,
                    schema_name,
                    omitted_variants,
                    f"{schema_name} omit Loop 9 variant {variant_name}",
                )
                expect_loop9_exact_assignment_failure(
                    f"{schema_name} Loop 9 variant omission {variant_name}",
                    mutated_paper,
                    schema_name,
                    expected_message=loop9_exact_failure_message,
                )

            for left_variant in range(len(expected_variants)):
                for right_variant in range(left_variant + 1, len(expected_variants)):
                    reordered_variants = list(expected_variants)
                    reordered_variants[left_variant], reordered_variants[right_variant] = (
                        reordered_variants[right_variant],
                        reordered_variants[left_variant],
                    )
                    mutated_paper = replace_loop8_schema_assignment(
                        baseline_assignment,
                        schema_name,
                        tuple(reordered_variants),
                        f"{schema_name} reorder Loop 9 variants "
                        f"{left_variant + 1}/{right_variant + 1}",
                    )
                    expect_loop9_exact_assignment_failure(
                        f"{schema_name} Loop 9 variant reorder "
                        f"{left_variant + 1}/{right_variant + 1}",
                        mutated_paper,
                        schema_name,
                        expected_message=loop9_exact_failure_message,
                    )

        for variant_insertion_index in range(len(expected_variants) + 1):
            added_variants = list(expected_variants)
            added_variants.insert(
                variant_insertion_index,
                (
                    "UnexpectedLoop9NegativeControlVariantV1",
                    ("unexpected_loop9_negative_control_field",),
                ),
            )
            mutated_paper = replace_loop8_schema_assignment(
                baseline_assignment,
                schema_name,
                tuple(added_variants),
                f"{schema_name} add Loop 9 variant at {variant_insertion_index + 1}",
            )
            expect_loop9_exact_assignment_failure(
                f"{schema_name} Loop 9 variant addition at {variant_insertion_index + 1}",
                mutated_paper,
                schema_name,
                expected_message=loop9_exact_failure_message,
            )

        for variant_index, (variant_name, arguments) in enumerate(expected_variants):
            if variant_name == "BareTypeRef":
                if len(arguments) != 1:
                    raise PlanningError(
                        f"self-test fixture drift: {schema_name} malformed Loop 9 bare type"
                    )
                mutated_variants = list(expected_variants)
                mutated_variants[variant_index] = (
                    "BareTypeRef",
                    ("UnexpectedLoop9NegativeControlTypeV1",),
                )
                mutated_paper = replace_loop8_schema_assignment(
                    baseline_assignment,
                    schema_name,
                    tuple(mutated_variants),
                    f"{schema_name} substitute Loop 9 bare type",
                )
                expect_loop9_exact_assignment_failure(
                    f"{schema_name} Loop 9 bare type substitution",
                    mutated_paper,
                    schema_name,
                    expected_message=loop9_exact_failure_message,
                )
                continue
            if variant_name == "MatchExpression":
                if len(arguments) != 2 or "=>" not in arguments[1]:
                    raise PlanningError(
                        f"self-test fixture drift: {schema_name} malformed Loop 10 match expression"
                    )
                mutated_variants = list(expected_variants)
                mutated_variants[variant_index] = (
                    "MatchExpression",
                    (
                        arguments[0],
                        arguments[1].replace(
                            "=>",
                            "=>UnexpectedLoop10NegativeControlV1+",
                            1,
                        ),
                    ),
                )
                mutated_paper = replace_loop8_schema_assignment(
                    baseline_assignment,
                    schema_name,
                    tuple(mutated_variants),
                    f"{schema_name} mutate Loop 10 match expression",
                )
                expect_loop9_exact_assignment_failure(
                    f"{schema_name} Loop 10 match-expression mutation",
                    mutated_paper,
                    schema_name,
                    expected_message=loop9_exact_failure_message,
                )
                continue

            for omitted_argument_index, omitted_argument in enumerate(arguments):
                remaining_arguments = (
                    arguments[:omitted_argument_index]
                    + arguments[omitted_argument_index + 1 :]
                )
                mutated_variants = list(expected_variants)
                mutated_variants[variant_index] = (variant_name, remaining_arguments)
                mutated_paper = replace_loop8_schema_assignment(
                    baseline_assignment,
                    schema_name,
                    tuple(mutated_variants),
                    f"{schema_name}.{variant_name} omit Loop 9 field {omitted_argument}",
                )
                expect_loop9_exact_assignment_failure(
                    f"{schema_name}.{variant_name} Loop 9 field omission "
                    f"{omitted_argument}",
                    mutated_paper,
                    schema_name,
                    expected_message=loop9_exact_failure_message,
                )

            for argument_insertion_index in range(len(arguments) + 1):
                added_arguments = list(arguments)
                added_arguments.insert(
                    argument_insertion_index,
                    f"unexpected_loop9_field_{variant_index + 1}_"
                    f"{argument_insertion_index + 1}",
                )
                mutated_variants = list(expected_variants)
                mutated_variants[variant_index] = (variant_name, tuple(added_arguments))
                mutated_paper = replace_loop8_schema_assignment(
                    baseline_assignment,
                    schema_name,
                    tuple(mutated_variants),
                    f"{schema_name}.{variant_name} add Loop 9 field at "
                    f"{argument_insertion_index + 1}",
                )
                expect_loop9_exact_assignment_failure(
                    f"{schema_name}.{variant_name} Loop 9 field addition at "
                    f"{argument_insertion_index + 1}",
                    mutated_paper,
                    schema_name,
                    expected_message=loop9_exact_failure_message,
                )

            for left_argument in range(len(arguments)):
                for right_argument in range(left_argument + 1, len(arguments)):
                    reordered_arguments = list(arguments)
                    reordered_arguments[left_argument], reordered_arguments[right_argument] = (
                        reordered_arguments[right_argument],
                        reordered_arguments[left_argument],
                    )
                    mutated_variants = list(expected_variants)
                    mutated_variants[variant_index] = (
                        variant_name,
                        tuple(reordered_arguments),
                    )
                    mutated_paper = replace_loop8_schema_assignment(
                        baseline_assignment,
                        schema_name,
                        tuple(mutated_variants),
                        f"{schema_name}.{variant_name} reorder Loop 9 fields "
                        f"{left_argument + 1}/{right_argument + 1}",
                    )
                    expect_loop9_exact_assignment_failure(
                        f"{schema_name}.{variant_name} Loop 9 field reorder "
                        f"{left_argument + 1}/{right_argument + 1}",
                        mutated_paper,
                        schema_name,
                        expected_message=loop9_exact_failure_message,
                    )

        if schema_name in EXPECTED_LOOP9_DOMAIN_CONTEXT_ASSIGNMENTS:
            mutable_variants = [
                (variant_name, list(arguments))
                for variant_name, arguments in expected_variants
            ]
            domain_location = next(
                (
                    (variant_index, argument_index)
                    for variant_index, (_variant_name, arguments) in enumerate(
                        mutable_variants
                    )
                    for argument_index, argument in enumerate(arguments)
                    if "domain" in argument.lower()
                ),
                None,
            )
            if domain_location is None:
                raise PlanningError(
                    f"self-test fixture drift: {schema_name} has no domain-bearing argument"
                )
            variant_index, argument_index = domain_location
            mutable_variants[variant_index][1][argument_index] += "_wrong_domain"
            domain_mutation = tuple(
                (variant_name, tuple(arguments))
                for variant_name, arguments in mutable_variants
            )
            mutated_paper = replace_loop8_schema_assignment(
                baseline_assignment,
                schema_name,
                domain_mutation,
                f"{schema_name} domain substitution",
            )
            expect_loop9_exact_assignment_failure(
                f"{schema_name} domain substitution",
                mutated_paper,
                schema_name,
                expected_message=loop9_exact_failure_message,
            )

            context_location = next(
                (
                    (variant_index, argument_index)
                    for variant_index, (_variant_name, arguments) in enumerate(
                        expected_variants
                    )
                    for argument_index, argument in enumerate(arguments)
                    if "OrdinaryProfileTransitionV1(parent_profile_context)" in argument
                ),
                None,
            )
            if context_location is not None:
                context_variants = [
                    (variant_name, list(arguments))
                    for variant_name, arguments in expected_variants
                ]
                variant_index, argument_index = context_location
                context_variants[variant_index][1][argument_index] = (
                    context_variants[variant_index][1][argument_index].replace(
                        "OrdinaryProfileTransitionV1(parent_profile_context)",
                        "OrdinaryProfileTransitionV1(wrong_parent_profile_context)",
                        1,
                    )
                )
                context_mutation = tuple(
                    (variant_name, tuple(arguments))
                    for variant_name, arguments in context_variants
                )
                mutated_paper = replace_loop8_schema_assignment(
                    baseline_assignment,
                    schema_name,
                    context_mutation,
                    f"{schema_name} context substitution",
                )
                expect_loop9_exact_assignment_failure(
                    f"{schema_name} context substitution",
                    mutated_paper,
                    schema_name,
                    expected_message=loop9_exact_failure_message,
                )

        for original, replacement in EXPECTED_LOOP9_SPECIAL_CONTEXT_MUTATIONS.get(
            schema_name,
            (),
        ):
            if baseline_assignment.count(original) != 1:
                raise PlanningError(
                    f"self-test fixture drift: {schema_name} special context "
                    f"fragment {original} count={baseline_assignment.count(original)}"
                )
            mutated_paper = baseline_assignment.replace(original, replacement, 1)
            expect_loop9_exact_assignment_failure(
                f"{schema_name} special policy/hash context substitution {original}",
                mutated_paper,
                schema_name,
                expected_message=loop9_exact_failure_message,
            )

        assignment_start, assignment_end = _schema_assignment_bounds(
            layer1_design_paper_text,
            schema_name,
        )
        source_assignment = layer1_design_paper_text[assignment_start:assignment_end]
        quoted_domains = tuple(
            re.finditer(r'Domain\("([^"\r\n]*)"\)', source_assignment)
        )
        for domain_index, domain_match in enumerate(quoted_domains, 1):
            for whitespace_label, whitespace in (
                ("space", " "),
                ("tab", "\t"),
                ("newline", "\n"),
            ):
                insertion = domain_match.end(1)
                mutated_assignment = (
                    source_assignment[:insertion]
                    + whitespace
                    + source_assignment[insertion:]
                )
                baseline_parsed = _parse_exact_schema_assignment(
                    source_assignment,
                    schema_name,
                )
                mutated_parsed = _parse_exact_schema_assignment(
                    mutated_assignment,
                    schema_name,
                )
                if mutated_parsed == baseline_parsed:
                    raise PlanningError(
                        "negative control reached whitespace-erasing parser equivalence: "
                        f"{schema_name} quoted domain {domain_index} {whitespace_label}"
                    )
                expect_loop9_exact_assignment_failure(
                    f"{schema_name} quoted domain {domain_index} embedded {whitespace_label}",
                    mutated_assignment,
                    schema_name,
                    expected_message=loop9_exact_failure_message,
                )

    post_blank_tail_controls = (
        (
            "post-blank variant continuation",
            "AcceptedParentAuthorityUseV1",
            "\n\n  | UnauthenticatedParentAuthorityV1",
            loop9_exact_failure_message,
        ),
        (
            "post-blank parenthesized bare-variant continuation",
            "AcceptedParentAuthorityUseV1",
            "\n\n(unexpected_post_blank_field)",
            loop9_exact_failure_message,
        ),
        (
            "post-blank orphan field",
            "ConsensusNativeAcceptedRecordOccurrenceReceiptV1",
            "\n\n    unexpected_post_blank_field",
            "has malformed variant",
        ),
        (
            "post-blank hash tail",
            "consensus_native_accepted_record_occurrence_receipt_id",
            '\n\n || Domain("bitblock/unbound-post-blank-tail/v1")',
            "empty schema component",
        ),
    )
    for label, schema_name, tail, expected_message in post_blank_tail_controls:
        baseline_assignment = render_loop8_schema_assignment(
            schema_name,
            _parse_exact_schema_assignment(layer1_design_paper_text, schema_name),
        )
        expect_loop9_exact_assignment_failure(
            label,
            baseline_assignment + tail,
            schema_name,
            expected_message=expected_message,
        )

    envelope_helper_mutations = (
        (
            "EnvelopeOwnerProfileContextV1(profile_transition_context)",
            ") => old",
            ") => new",
            "envelope owner old/new polarity",
        ),
        (
            "EnvelopeResolutionV1(profile_transition_context)",
            "ResolveAdmittedEnvelopeProfile",
            "ResolveUnadmittedEnvelopeProfile",
            "envelope profile resolver",
        ),
        (
            "EnvelopeHashV1(profile_transition_context, object_domain, payload)",
            "Domain(object_domain)",
            'Domain("bitblock/collapsed-envelope-domain/v1")',
            "envelope object-domain collapse",
        ),
        (
            "EnvelopeHashV1(profile_transition_context, object_domain, payload)",
            "EnvelopeOwnerProfileContextV1(profile_transition_context)",
            "profile_transition_context",
            "envelope profile-context framing",
        ),
        (
            "EnvelopeHashV1(profile_transition_context, object_domain, payload)",
            "LengthFrame(Canonical[\n"
            "            EnvelopeResolutionV1(profile_transition_context).canonical_codec_id\n"
            "        ](payload))",
            "Canonical[\n"
            "            EnvelopeResolutionV1(profile_transition_context).canonical_codec_id\n"
            "        ](payload)",
            "envelope payload length framing",
        ),
        (
            "EnvelopeCommitOrderedV1(profile_transition_context, sequence_domain, sequence)",
            "CanonicalOrderedSequence(sequence)",
            "UnorderedSequence(sequence)",
            "ordered envelope sequence commitment",
        ),
    )
    for schema_name, original, replacement, label in envelope_helper_mutations:
        mutated_paper = replace_exact_assignment_fragment(
            layer1_design_paper_text,
            schema_name,
            original,
            replacement,
            label,
        )
        expect_loop9_exact_assignment_failure(
            label,
            mutated_paper,
            schema_name,
            expected_message=loop9_exact_failure_message,
        )

    escaped_helper = replace_exact_assignment_fragment(
        layer1_design_paper_text,
        "EnvelopeHashV1(profile_transition_context, object_domain, payload)",
        "Domain(object_domain)",
        "CollapsedEnvelopeDomainV1(object_domain)",
        "defined helper outside transitive closure",
    )
    escaped_helper += (
        "\n\nCollapsedEnvelopeDomainV1(object_domain) = Domain(object_domain)\n"
    )
    try:
        _check_loop10_envelope_helper_transitive_closure(escaped_helper)
    except PlanningError as error:
        if "escape the Loop 10 exact transitive closure" not in str(error):
            raise PlanningError(
                "negative control failed by the wrong mechanism: defined helper outside "
                f"transitive closure: {error}"
            ) from error
    else:
        raise PlanningError(
            "negative control did not fail: defined helper outside transitive closure"
        )

    undeclared_primitive = replace_exact_assignment_fragment(
        layer1_design_paper_text,
        "EnvelopeCommitOrderedV1(profile_transition_context, sequence_domain, sequence)",
        "CanonicalOrderedSequence(sequence)",
        "UndeclaredOrderedSequence(sequence)",
        "undeclared abstract envelope primitive",
    )
    try:
        _check_loop10_envelope_helper_transitive_closure(undeclared_primitive)
    except PlanningError as error:
        if "undeclared abstract envelope primitives" not in str(error):
            raise PlanningError(
                "negative control failed by the wrong mechanism: undeclared abstract "
                f"envelope primitive: {error}"
            ) from error
    else:
        raise PlanningError(
            "negative control did not fail: undeclared abstract envelope primitive"
        )

    shadowed_abstract_primitive_declarations = (
        (
            "space indentation",
            '  Domain(object_domain) = Domain("bitblock/collapsed-envelope-domain/v1")',
        ),
        (
            "tab indentation",
            '\tDomain(object_domain) = Domain("bitblock/collapsed-envelope-domain/v1")',
        ),
        (
            "space before parameter list",
            '  Domain (object_domain) = Domain("bitblock/collapsed-envelope-domain/v1")',
        ),
        (
            "multiline parameter list",
            '  Domain(\n object_domain\n) = Domain("bitblock/collapsed-envelope-domain/v1")',
        ),
        (
            "newline before equals",
            '  Domain(object_domain)\n = Domain("bitblock/collapsed-envelope-domain/v1")',
        ),
    )
    for layout_label, declaration in shadowed_abstract_primitive_declarations:
        shadowed_abstract_primitive = layer1_design_paper_text + f"\n\n{declaration}\n"
        try:
            _check_loop10_envelope_helper_transitive_closure(
                shadowed_abstract_primitive
            )
        except PlanningError as error:
            if "defined helper calls escape the Loop 10 exact transitive closure" not in str(
                error
            ):
                raise PlanningError(
                    "negative control failed by the wrong mechanism: indented abstract "
                    f"primitive shadow ({layout_label}): {error}"
                ) from error
        else:
            raise PlanningError(
                "negative control did not fail: indented abstract primitive shadow "
                f"({layout_label})"
            )

    match_arm_layout_equivalent = replace_exact_assignment_fragment(
        layer1_design_paper_text,
        "EnvelopeOwnerProfileContextV1(profile_transition_context)",
        "        OrdinaryProfileTransitionV1(active) => active,",
        "OrdinaryProfileTransitionV1(active) => active,",
        "column-zero match-arm layout equivalence",
    )
    check_loop9_exact_assignments(
        match_arm_layout_equivalent,
        ("EnvelopeOwnerProfileContextV1(profile_transition_context)",),
    )

    predecessor_schema = "VerifiedSuccessorPredecessorBaseContextV1"
    predecessor_variants = EXPECTED_LOOP8_EXACT_SCHEMA_VARIANTS[predecessor_schema]
    predecessor_variant_name, predecessor_arguments = predecessor_variants[0]
    collapsed_release_arguments = tuple(
        (
            "verified_release_parent_eligibility="
            "VerifiedEligibleAcceptedParentRefV1(SuccessorActivationAuthorityV1)"
            if argument.startswith("verified_release_parent_eligibility=")
            else argument
        )
        for argument in predecessor_arguments
    )
    collapsed_release_variant = (
        (predecessor_variant_name, collapsed_release_arguments),
    )
    collapsed_release_paper = replace_loop8_schema_assignment(
        layer1_design_paper_text,
        predecessor_schema,
        collapsed_release_variant,
        "release eligibility collapsed to activation purpose",
    )
    expect_loop8_exact_schema_failure(
        "release eligibility collapsed to activation purpose",
        collapsed_release_paper,
        predecessor_schema,
        expected_message=exact_schema_failure_message,
    )

    for field, value in planning_summary.items():
        canonical_row = f"| `{field}` | `{value}` |"
        mutated_row = f"| `{field}` | `{value + 1}` |"
        mutated_summary = text.replace(canonical_row, mutated_row, 1)
        if mutated_summary == text:
            raise PlanningError(f"self-test fixture drift: summary field {field}")
        expect_summary_projection_failure(
            f"PlanningCheckerSummaryV1 wrong {field}",
            mutated_summary,
            planning_summary,
        )

    first_summary_field, second_summary_field = PLANNING_SUMMARY_FIELD_ORDER[:2]
    first_summary_row = (
        f"| `{first_summary_field}` | `{planning_summary[first_summary_field]}` |"
    )
    second_summary_row = (
        f"| `{second_summary_field}` | `{planning_summary[second_summary_field]}` |"
    )
    reordered_summary = text.replace(
        first_summary_row + "\n" + second_summary_row,
        second_summary_row + "\n" + first_summary_row,
        1,
    )
    if reordered_summary == text:
        raise PlanningError("self-test fixture drift: summary row reorder")
    expect_summary_projection_failure(
        "PlanningCheckerSummaryV1 row reorder",
        reordered_summary,
        planning_summary,
    )

    compact_projection_texts = {path: "" for path in projection_texts}
    compact_projection_texts["docs/HANDOFF.md"] = "\n\n".join(
        "\n\n".join(
            (
                f"### {schema_name}",
                "\n".join(
                    (
                        PROVENANCE_TABLE_HEADER,
                        PROVENANCE_TABLE_SEPARATOR,
                        *(
                            f"| `{field}` | `{provenance[field]}` |"
                            for field in PROVENANCE_FIELD_ORDER
                        ),
                    )
                ),
            )
        )
        for schema_name, provenance in (
            ("Loop7ProvenanceV1", loop7_provenance),
            ("Loop8ProvenanceV1", loop8_provenance),
            ("Loop9ProvenanceV1", loop9_provenance),
            ("Loop10ProvenanceV1", loop10_provenance),
        )
    )
    compact_projection_texts["docs/CAPABILITY_TREE.md"] = "\n".join(
        f"| `{field}` | `{planning_summary[field]}` |"
        for field in PLANNING_SUMMARY_FIELD_ORDER
    )
    check_no_duplicate_exact_projections(compact_projection_texts, planning_summary)

    for schema_name, provenance in (
        ("Loop7ProvenanceV1", loop7_provenance),
        ("Loop8ProvenanceV1", loop8_provenance),
        ("Loop9ProvenanceV1", loop9_provenance),
        ("Loop10ProvenanceV1", loop10_provenance),
    ):
        for field, value in provenance.items():
            mutated_value = (
                str(int(value) + 1)
                if field == "material_finding_count_u32"
                else "0" * len(value)
            )
            canonical_row = f"| `{field}` | `{value}` |"
            mutated_row = f"| `{field}` | `{mutated_value}` |"
            mutated_handoff = handoff_text.replace(canonical_row, mutated_row, 1)
            if mutated_handoff == handoff_text:
                raise PlanningError(
                    f"self-test fixture drift: {schema_name} provenance field {field}"
                )
            expect_provenance_projection_failure(
                f"{schema_name} wrong {field}",
                mutated_handoff,
                schema_name,
                provenance,
            )

        first_field, second_field = PROVENANCE_FIELD_ORDER[:2]
        first_row = f"| `{first_field}` | `{provenance[first_field]}` |"
        second_row = f"| `{second_field}` | `{provenance[second_field]}` |"
        reordered_handoff = handoff_text.replace(
            first_row + "\n" + second_row,
            second_row + "\n" + first_row,
            1,
        )
        if reordered_handoff == handoff_text:
            raise PlanningError(f"self-test fixture drift: {schema_name} row reorder")
        expect_provenance_projection_failure(
            f"{schema_name} row reorder",
            reordered_handoff,
            schema_name,
            provenance,
        )

        fake_provenance = dict(provenance)
        fake_count = str(int(provenance["material_finding_count_u32"]) + 1)
        fake_provenance["material_finding_count_u32"] = fake_count
        canonical_count_row = (
            "| `material_finding_count_u32` | `"
            + provenance["material_finding_count_u32"]
            + "` |"
        )
        fake_count_row = f"| `material_finding_count_u32` | `{fake_count}` |"
        matched_fake_handoff = handoff_text.replace(
            canonical_count_row,
            fake_count_row,
            1,
        )
        if matched_fake_handoff == handoff_text:
            raise PlanningError(
                f"self-test fixture drift: {schema_name} matched fake finding count"
            )
        expect_provenance_projection_failure(
            f"{schema_name} matched caller/HANDOFF wrong finding count",
            matched_fake_handoff,
            schema_name,
            fake_provenance,
            expected_message="audit-derived finding count mismatch",
        )

    for schema_name, provenance in (
        ("Loop7ProvenanceV1", loop7_provenance),
        ("Loop8ProvenanceV1", loop8_provenance),
        ("Loop9ProvenanceV1", loop9_provenance),
        ("Loop10ProvenanceV1", loop10_provenance),
    ):
        for field in PROVENANCE_FIELD_ORDER[:-1]:
            object_id = provenance[field]
            for token_kind, token in (
                ("full", object_id),
                ("short", object_id[:7]),
                ("eight-character", object_id[:8]),
            ):
                duplicate_object = dict(compact_projection_texts)
                duplicate_object["README.md"] += (
                    f"\n\nPinned exact object `{token}` without an audit label.\n"
                )
                expect_duplicate_projection_failure(
                    f"duplicate {schema_name} {token_kind} Git object {field}",
                    duplicate_object,
                    planning_summary,
                    expected_message=(
                        f"exact Git object for {schema_name} must occur exactly once"
                    ),
                )

    loop9_last_row = (
        "| `material_finding_count_u32` | `"
        + loop9_provenance["material_finding_count_u32"]
        + "` |"
    )
    trailing_provenance_handoff = compact_projection_texts["docs/HANDOFF.md"].replace(
        loop9_last_row,
        loop9_last_row
        + "\n\nTrailing exact object `"
        + loop9_provenance["addition_commit"]
        + "` remains inside this canonical provenance section.",
        1,
    )
    if trailing_provenance_handoff == compact_projection_texts["docs/HANDOFF.md"]:
        raise PlanningError("self-test fixture drift: trailing provenance-section prose")
    trailing_provenance = dict(compact_projection_texts)
    trailing_provenance["docs/HANDOFF.md"] = trailing_provenance_handoff
    expect_duplicate_projection_failure(
        "duplicate exact object in trailing canonical provenance-section prose",
        trailing_provenance,
        planning_summary,
        expected_message="exact Git object for Loop9ProvenanceV1 must occur exactly once",
    )

    duplicate_loop8_finding_count = dict(compact_projection_texts)
    duplicate_loop8_finding_count["docs/TRANSCRIPT_AUDIT.md"] += (
        "\n\nLoop 8 duplicate has "
        + loop8_provenance["material_finding_count_u32"]
        + " material findings.\n"
    )
    expect_duplicate_projection_failure(
        "duplicate Loop 8 exact finding count",
        duplicate_loop8_finding_count,
        planning_summary,
        expected_message="exact finding count for Loop8ProvenanceV1",
    )

    wrong_loop8_finding_count = dict(compact_projection_texts)
    wrong_loop8_finding_count["docs/PROTOCOL.md"] += (
        "\n\nLoop 8 is claimed to have 7 material findings.\n"
    )
    expect_duplicate_projection_failure(
        "wrong Loop 8 semantic finding count",
        wrong_loop8_finding_count,
        planning_summary,
        expected_message="exact finding count for Loop8ProvenanceV1",
    )

    wrong_loop8_object = dict(compact_projection_texts)
    wrong_loop8_object["docs/PROJECT_INTENT.md"] += (
        "\n\nLoop 8 addition commit `ffffffffffffffffffffffffffffffffffffffff`.\n"
    )
    expect_duplicate_projection_failure(
        "wrong Loop 8 full Git object claim",
        wrong_loop8_object,
        planning_summary,
        expected_message="non-canonical Git object claim for Loop8ProvenanceV1",
    )

    for path in compact_projection_texts:
        for field in PLANNING_SUMMARY_FIELD_ORDER:
            duplicate_summary_field = dict(compact_projection_texts)
            duplicate_summary_field[path] += (
                "\n\nPlanningCheckerSummaryV1 "
                f"{field} = {planning_summary[field] + 1000}.\n"
            )
            expect_duplicate_projection_failure(
                f"duplicate canonical summary field {field} in {path}",
                duplicate_summary_field,
                planning_summary,
                expected_message=(
                    f"exact PlanningCheckerSummaryV1 field {field} must occur exactly once"
                ),
            )

    summary_duplicate_phrases = {
        "capability_node_count_u32": (
            f"{planning_summary['capability_node_count_u32']} capability nodes"
        ),
        "foundation_node_count_u32": (
            f"{planning_summary['foundation_node_count_u32']} foundation nodes"
        ),
        "detailed_node_count_u32": (
            f"{planning_summary['detailed_node_count_u32']} detailed nodes"
        ),
        "phase_status_count_u32": (
            f"{planning_summary['phase_status_count_u32']} phase-status rows"
        ),
        "ready_frontier_count_u32": (
            f"{planning_summary['ready_frontier_count_u32']} ordered READY frontier rows"
        ),
        "base_dependency_edge_count_u32": (
            f"{planning_summary['base_dependency_edge_count_u32']} base dependency edges"
        ),
        "phase_dependency_edge_count_u32": (
            f"{planning_summary['phase_dependency_edge_count_u32']} phase dependencies"
        ),
        "conditional_edge_count_u32": (
            f"{planning_summary['conditional_edge_count_u32']} conditional edges"
        ),
        "total_declared_edge_count_u32": (
            f"{planning_summary['total_declared_edge_count_u32']} combined edges"
        ),
        "predicate_entailment_count_u32": (
            f"{planning_summary['predicate_entailment_count_u32']} predicate entailments"
        ),
        "planning_markdown_count_u32": (
            f"{planning_summary['planning_markdown_count_u32']}-document planning authority "
            "inventory"
        ),
        "frozen_text_artifact_count_u32": (
            f"{planning_summary['frozen_text_artifact_count_u32']} frozen text artifacts"
        ),
        "frozen_markdown_audit_count_u32": (
            f"{planning_summary['frozen_markdown_audit_count_u32']} frozen Markdown audits"
        ),
        "frozen_json_receipt_count_u32": (
            f"{planning_summary['frozen_json_receipt_count_u32']} frozen JSON source receipt"
        ),
    }
    if set(summary_duplicate_phrases) != set(PLANNING_SUMMARY_FIELD_ORDER):
        raise PlanningError("self-test fixture drift: planning-summary duplicate phrases")
    summary_reverse_phrases = {
        "capability_node_count_u32": "capability-node count is",
        "foundation_node_count_u32": "foundation-node count is",
        "detailed_node_count_u32": "detailed-node count is",
        "phase_status_count_u32": "phase-status row count is",
        "ready_frontier_count_u32": "READY frontier count is",
        "base_dependency_edge_count_u32": "base dependency edge count is",
        "phase_dependency_edge_count_u32": "phase dependency count is",
        "conditional_edge_count_u32": "conditional edge count is",
        "total_declared_edge_count_u32": "combined edge count is",
        "predicate_entailment_count_u32": "predicate entailment count is",
        "planning_markdown_count_u32": "planning authority inventory count is",
        "frozen_text_artifact_count_u32": "frozen text artifact count is",
        "frozen_markdown_audit_count_u32": "frozen Markdown audit count is",
        "frozen_json_receipt_count_u32": "frozen JSON source receipt count is",
    }
    if set(summary_reverse_phrases) != set(PLANNING_SUMMARY_FIELD_ORDER):
        raise PlanningError("self-test fixture drift: reverse planning-summary phrases")
    for path in compact_projection_texts:
        for field in PLANNING_SUMMARY_FIELD_ORDER:
            duplicate_summary_count = dict(compact_projection_texts)
            duplicate_summary_count[path] += (
                f"\n\nDuplicate projection: {summary_duplicate_phrases[field]}.\n"
            )
            expect_duplicate_projection_failure(
                f"duplicate {field} in {path}",
                duplicate_summary_count,
                planning_summary,
                expected_message=f"field={field}",
            )

            wrong_count_phrase = re.sub(
                rf"\b{planning_summary[field]}\b",
                str(planning_summary[field] + 1000),
                summary_duplicate_phrases[field],
                count=1,
            )
            if wrong_count_phrase == summary_duplicate_phrases[field]:
                raise PlanningError(
                    f"self-test fixture drift: wrong prose count for {field}"
                )
            wrong_summary_count = dict(compact_projection_texts)
            wrong_summary_count[path] += (
                f"\n\nWrong projection: {wrong_count_phrase}.\n"
            )
            expect_duplicate_projection_failure(
                f"wrong {field} in {path}",
                wrong_summary_count,
                planning_summary,
                expected_message=f"field={field}",
            )

            reverse_summary_count = dict(compact_projection_texts)
            reverse_summary_count[path] += (
                "\n\nReverse wrong projection: "
                f"{summary_reverse_phrases[field]} {planning_summary[field] + 1000}.\n"
            )
            expect_duplicate_projection_failure(
                f"reverse wrong {field} in {path}",
                reverse_summary_count,
                planning_summary,
                expected_message=f"field={field}",
            )

    for path in compact_projection_texts:
        wrong_loop9_count = dict(compact_projection_texts)
        wrong_loop9_count[path] += "\n\nLoop 9 has 999 material findings.\n"
        expect_duplicate_projection_failure(
            f"wrong Loop 9 finding count in {path}",
            wrong_loop9_count,
            planning_summary,
            expected_message="exact finding count for Loop9ProvenanceV1",
        )

        reverse_loop9_count = dict(compact_projection_texts)
        reverse_loop9_count[path] += (
            "\n\nThe material finding count for Loop 9 is 999.\n"
        )
        expect_duplicate_projection_failure(
            f"reverse wrong Loop 9 finding count in {path}",
            reverse_loop9_count,
            planning_summary,
            expected_message="exact finding count for Loop9ProvenanceV1",
        )

        wrong_loop9_git_object = dict(compact_projection_texts)
        wrong_loop9_git_object[path] += (
            "\n\nLoop 9 addition commit "
            "`eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee`.\n"
        )
        expect_duplicate_projection_failure(
            f"wrong Loop 9 Git object in {path}",
            wrong_loop9_git_object,
            planning_summary,
            expected_message="non-canonical Git object claim for Loop9ProvenanceV1",
        )

        reverse_loop9_git_object = dict(compact_projection_texts)
        reverse_loop9_git_object[path] += (
            "\n\nAddition commit `dddddddddddddddddddddddddddddddddddddddd` "
            "is claimed for Loop 9.\n"
        )
        expect_duplicate_projection_failure(
            f"reverse wrong Loop 9 Git object in {path}",
            reverse_loop9_git_object,
            planning_summary,
            expected_message="non-canonical Git object claim for Loop9ProvenanceV1",
        )

        abbreviated_loop9_git_object = dict(compact_projection_texts)
        abbreviated_loop9_git_object[path] += (
            f"\n\nLoop 9 addition commit `{NINTH_AUDIT_ADDITION_COMMIT[:8]}`.\n"
        )
        expect_duplicate_projection_failure(
            f"abbreviated Loop 9 Git object in {path}",
            abbreviated_loop9_git_object,
            planning_summary,
            expected_message="exact Git object for Loop9ProvenanceV1 must occur exactly once",
        )

        reverse_abbreviated_loop9_git_object = dict(compact_projection_texts)
        reverse_abbreviated_loop9_git_object[path] += (
            "\n\nAddition commit `deadbeef` is claimed for Loop 9.\n"
        )
        expect_duplicate_projection_failure(
            f"reverse abbreviated Loop 9 Git object in {path}",
            reverse_abbreviated_loop9_git_object,
            planning_summary,
            expected_message="abbreviated Git object claim for Loop9ProvenanceV1",
        )

        semantic_projection_controls = (
            (
                "wrapped forward Git object",
                "Loop 8\naddition commit `ffffffffffffffffffffffffffffffffffffffff`.",
                "non-canonical Git object claim for Loop8ProvenanceV1",
            ),
            (
                "wrapped reverse Git object",
                "Addition commit `ffffffffffffffffffffffffffffffffffffffff` belongs to\nLoop 8.",
                "non-canonical Git object claim for Loop8ProvenanceV1",
            ),
            (
                "wrapped planning count",
                "The capability\nnode count is 999.",
                "field=capability_node_count_u32",
            ),
            (
                "zero planning count",
                "There are zero capability nodes.",
                "field=capability_node_count_u32",
            ),
            (
                "no planning count",
                "The combined graph has no edges.",
                "field=total_declared_edge_count_u32",
            ),
            (
                "none planning count",
                "Capability nodes: none.",
                "field=capability_node_count_u32",
            ),
            (
                "no audit finding count",
                "Loop 9 found no material gaps.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "Loop 7 filename marker finding count",
                "design_research_distillation_audit_loop7.md found no material gaps.",
                "exact finding count for Loop7ProvenanceV1",
            ),
            (
                "Loop 8 filename marker finding count",
                "design_research_distillation_audit_loop8.md found no material gaps.",
                "exact finding count for Loop8ProvenanceV1",
            ),
            (
                "Loop 9 filename marker finding count",
                "design_research_distillation_audit_loop9.md found no material gaps.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "Loop 10 filename marker finding count",
                "design_research_distillation_audit_loop10.md found no material gaps.",
                "exact finding count for Loop10ProvenanceV1",
            ),
            (
                "marker-first none audit finding count",
                "Loop 9 material findings: none.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "marker-first zero audit finding count",
                "Loop 9 findings: zero.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "noun-first none audit finding count",
                "Material findings for Loop 9: none.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "colon no audit finding count",
                "Loop 9: no findings.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "show-verb no audit finding count",
                "Loop 9 shows no findings.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "intervening-total none audit finding count",
                "Loop 9 material findings total: none.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "dash no audit finding count",
                "Loop 9 — no findings.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "with-none audit finding count",
                "Loop 9 with none of the material findings.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "dash noun-first none audit finding count",
                "Loop 9 — material findings: none.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "parenthesized noun-first none audit finding count",
                "Loop 9 (material findings): none.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "colon noun-first none audit finding count",
                "Loop 9: material findings: none.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "comma noun-first none audit finding count",
                "Loop 9, material findings: none.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "semicolon noun-first none audit finding count",
                "Loop 9; material findings: none.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "bracketed noun-first none audit finding count",
                "Loop 9 [material findings]: none.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "dash-parenthesized noun-first none audit finding count",
                "Loop 9 — (material findings): none.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "subordinate actual audit subject count",
                "After Loop 9 found no material gaps, its correction began.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "audit-of actual subject count",
                "The audit of Loop 9 found no material gaps.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "review-of actual subject count",
                "The review of Loop 9 found no material gaps.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "audit-for actual subject count",
                "The audit for Loop 9 reported zero material findings.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "review-about actual subject count",
                "The review about Loop 9 shows no findings.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "adverb between actual audit marker and result",
                "Loop 9 actually found no material gaps.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "parenthetical between actual review and result",
                "The Loop 9 review, after correction, found no material gaps.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "parenthetical after audit-of actual subject",
                "The audit of Loop 9, after correction, found no material gaps.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "adverb after review-about actual subject",
                "The review about Loop 9 conclusively showed no findings.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "successor noun inside contrastive aside does not own result",
                "Loop 9, unlike a separate successor sweep, found no material gaps.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "independent audit is not intrinsically a successor",
                "The independent audit of Loop 9 found no material gaps.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "successor phrase is object of an earlier verb",
                "We discussed a separate successor sweep after Loop 9 found no material gaps.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "successor appositive does not replace actual subject",
                "Loop 9, a separate successor audit, found no material gaps.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "compound subject retains actual audit",
                "Both Loop 9 and a separate successor sweep found no material gaps.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "actual claim before successor conjunct",
                "Loop 9 found no material gaps, but a separate successor sweep found no new material gaps.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "corrected exact cardinality in active voice",
                "Loop 9's correction closed all nine material findings.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "corrected exact cardinality in passive voice",
                "All nine material findings from Loop 9 were corrected.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "result noun owns zero finding claim",
                "The result of Loop 9 was no material findings.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "unbounded filler before actual result verb",
                "Loop 9 " + ("carefully " * 12) + "found no material gaps.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "unbounded filler before finding phrase",
                "Loop 9 found " + ("after independent review " * 4) + "no material gaps.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "P1 modifier inside actual finding phrase",
                "Loop 9 found no P1 material gaps.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "L1 modifier inside actual finding phrase",
                "Loop 9 found no L1 material gaps.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "version modifier inside actual finding phrase",
                "Loop 9 found no v2 material gaps.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "reported modifier inside actual finding phrase",
                "Loop 9 found no reported gaps.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "recorded modifier inside actual finding phrase",
                "Loop 9 found no recorded gaps.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "underscore-separated actual finding phrase",
                "Loop 9: no_gaps.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "underscore-separated modified actual finding phrase",
                "Loop 9: no_new_gaps.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "underscore-separated compound actual finding phrase",
                "Loop 9: none_of_the_gaps.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "hyphen-separated compound actual finding phrase",
                "Loop 9: none-of-the-gaps.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "actual claim precedes semicolon successor",
                "Loop 9 found no material gaps; a successor sweep found no new material gaps.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "finding claim precedes semicolon marker identity",
                "The audit found no material gaps; that audit was Loop 9.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "successor payload contains because-attributed frozen result",
                "The successor sweep after Loop 9 found no gaps because the predecessor result was zero findings.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "successor payload contains while-attributed frozen result",
                "The successor sweep after Loop 9 found no gaps while the prior audit had zero findings.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "successor payload contains second conjoined result",
                "The successor sweep after Loop 9 found no gaps and the predecessor had zero findings.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "successor payload delegates sole claim to predecessor",
                "The successor review after Loop 9 found the predecessor had no material gaps.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "successor payload delegates reverse claim to predecessor",
                "The successor review after Loop 9 found the predecessor's material gaps were zero.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "successor payload carries overlapping projections",
                "The successor sweep after Loop 9 found no findings count is zero.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "predecessor is not a direct successor modifier",
                "The successor review after Loop 9 found no predecessor gaps.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "successor payload delegates through embedded passive",
                "The successor sweep after Loop 9 found no gaps were reported by the predecessor.",
                "exact finding count for Loop9ProvenanceV1",
            ),
            (
                "compound count word in actual audit claim",
                "Loop 9 found twenty-one material gaps.",
                "exact finding count for Loop9ProvenanceV1",
            ),
        )
        for fixture_label, fixture_text, expected_message in semantic_projection_controls:
            semantic_projection = dict(compact_projection_texts)
            semantic_projection[path] += f"\n\n{fixture_text}\n"
            expect_duplicate_projection_failure(
                f"{fixture_label} in {path}",
                semantic_projection,
                planning_summary,
                expected_message=expected_message,
            )

        for fixture_label, fixture_text in (
            (
                "separate successor sweep convergence",
                "After correcting Loop 10, a separate successor sweep found no new material gaps.",
            ),
            (
                "successor review convergence before correction marker",
                "The successor review recorded no newly discovered gaps after the Loop 10 corrections.",
            ),
            (
                "successor sweep subject before marker",
                "The separate successor sweep after Loop 10 found no new material gaps.",
            ),
            (
                "successor review for marker",
                "The successor review for Loop 10 found no new material gaps after correction.",
            ),
            (
                "successor sweep after determiner marker",
                "The successor sweep after the Loop 10 audit found no new material gaps.",
            ),
            (
                "successor review for determiner marker",
                "The successor review for the Loop 10 audit found no new material gaps.",
            ),
            (
                "successor review of determiner marker",
                "The successor review of the Loop 10 audit found no new material gaps.",
            ),
            (
                "plural successor reviews of marker",
                "The successor reviews of Loop 10 found no new material gaps.",
            ),
            (
                "generic audits after marker",
                "The audits after Loop 10 found no new material gaps.",
            ),
            (
                "singular successor pass of marker",
                "The successor pass of Loop 10 found no new material gaps.",
            ),
            (
                "independent re-audit of marker",
                "The independent re-audit of Loop 10 found no new material gaps.",
            ),
            (
                "bare re-audit of marker",
                "The re-audit of Loop 10 found no new material gaps.",
            ),
            (
                "generic review after marker",
                "The review after Loop 10 found no new material gaps.",
            ),
            (
                "successor review following marker",
                "The successor review following Loop 10 found no new material gaps.",
            ),
            (
                "plural successor audits use passive result",
                "The successor audits of Loop 10 are reported to have no new material gaps.",
            ),
            (
                "plural generic audits after marker use passive result",
                "The audits after Loop 10 were claimed to have no new material gaps.",
            ),
            (
                "unrelated reviewer count is not a finding count",
                "The Loop 9 audit had two reviewers and unresolved material gaps.",
            ),
            (
                "underscore-separated reviewer count is not a finding count",
                "The Loop 9 audit had two_reviewers_and_unresolved_material_gaps.",
            ),
            (
                "underscore-separated with-clause count is not a finding count",
                "The Loop 9 audit had two_reviewers_with_unresolved_material_gaps.",
            ),
            (
                "longer loop number is not the Loop 9 marker",
                "Loop 90 found no material gaps.",
            ),
            (
                "audit records noun follows successor result",
                "The successor audit of Loop 10 found no gaps in audit records.",
            ),
            (
                "audit reports noun follows successor result",
                "The successor audit of Loop 10 found no gaps in audit reports.",
            ),
            (
                "compound count word remains one successor count",
                "The successor audit after Loop 10 found twenty-one material gaps.",
            ),
            (
                "declared alphanumeric successor modifier",
                "The successor audit after Loop 10 found no P1 material gaps.",
            ),
            (
                "declared reported successor modifier",
                "The successor audit after Loop 10 found no reported gaps.",
            ),
            (
                "declared recorded successor modifier",
                "The successor audit after Loop 10 found no recorded gaps.",
            ),
            (
                "underscore-separated successor finding phrase",
                "The successor audit after Loop 10 found no_new_gaps.",
            ),
            (
                "underscore-separated compound successor finding phrase",
                "The successor audit after Loop 10 found none_of_the_gaps.",
            ),
            (
                "hyphen-separated compound successor finding phrase",
                "The successor audit after Loop 10 found none-of-the-gaps.",
            ),
            (
                "marker corrections introduce successor subject",
                "After the Loop 10 corrections, a separate successor sweep found no new material gaps.",
            ),
            (
                "reported marker clause then successor clause",
                "Loop 10 reported its correction; a successor sweep found no new material gaps.",
            ),
            (
                "corrected marker clause then successor clause",
                "Loop 10 was corrected; a successor sweep found no new material gaps.",
            ),
        ):
            truthful_convergence = dict(compact_projection_texts)
            truthful_convergence[path] += f"\n\n{fixture_text}\n"
            try:
                check_no_duplicate_exact_projections(
                    truthful_convergence,
                    planning_summary,
                )
            except PlanningError as error:
                raise PlanningError(
                    "positive control rejected truthful successor convergence prose: "
                    f"{fixture_label} in {path}: {error}"
                ) from error

        for fixture_label, fixture_text in (
            (
                "forward canonical finding-count field",
                "Loop8ProvenanceV1 material_finding_count_u32 = 7",
            ),
            (
                "reverse canonical finding-count field",
                "material_finding_count_u32 for Loop 8 is 7",
            ),
        ):
            wrong_canonical_finding_count = dict(compact_projection_texts)
            wrong_canonical_finding_count[path] += f"\n\n{fixture_text}.\n"
            expect_duplicate_projection_failure(
                f"{fixture_label} in {path}",
                wrong_canonical_finding_count,
                planning_summary,
                expected_message="exact finding count for Loop8ProvenanceV1",
            )

        provenance_object_aliases = tuple(
            (field, field.replace("_", separator))
            for field in PROVENANCE_FIELD_ORDER[:-1]
            for separator in (" ", "-", "_")
        )
        for field, field_alias in provenance_object_aliases:
            for token_kind, token, expected_message in (
                (
                    "full",
                    "f" * 40,
                    "non-canonical Git object claim for Loop8ProvenanceV1",
                ),
                (
                    "abbreviated",
                    "deadbeef",
                    "abbreviated Git object claim for Loop8ProvenanceV1",
                ),
            ):
                schema_field_claim = dict(compact_projection_texts)
                schema_field_claim[path] += (
                    f"\n\nLoop8ProvenanceV1 {field_alias} = {token}.\n"
                )
                expect_duplicate_projection_failure(
                    f"{token_kind} schema marker {field}/{field_alias} in {path}",
                    schema_field_claim,
                    planning_summary,
                    expected_message=expected_message,
                )

        for field_alias in ("addition_commit", "audit_git_blob"):
            for token_kind, token, expected_message in (
                (
                    "full",
                    "f" * 40,
                    "non-canonical Git object claim for Loop8ProvenanceV1",
                ),
                (
                    "abbreviated",
                    "deadbeef",
                    "abbreviated Git object claim for Loop8ProvenanceV1",
                ),
            ):
                loop_marker_field_claim = dict(compact_projection_texts)
                loop_marker_field_claim[path] += (
                    f"\n\nLoop 8 {field_alias} = {token}.\n"
                )
                expect_duplicate_projection_failure(
                    f"{token_kind} Loop 8 marker {field_alias} in {path}",
                    loop_marker_field_claim,
                    planning_summary,
                    expected_message=expected_message,
                )

    benign_protocol_counts = dict(compact_projection_texts)
    benign_protocol_counts["docs/LAYER1_DESIGN_PAPER.md"] += (
        "\n\nA protocol example may mention 72 validators, 94 signature slots, 311 messages, "
        "5 authority witnesses, 44 proof limbs, 360 bytes, 10 predicates, 14 candidates, "
        "22 codecs, 12 commitments, 11 keys, and 1 genesis object without projecting any "
        "planning count.\n"
    )
    check_no_duplicate_exact_projections(benign_protocol_counts, planning_summary)

    handoff_omission = re.sub(
        r"^24\. `docs/TIMELINES\.md`(?:\n|$)",
        "",
        handoff_text,
        count=1,
        flags=re.MULTILINE,
    )
    if handoff_omission == handoff_text:
        raise PlanningError("self-test fixture drift: HANDOFF read-order omission")
    expect_handoff_failure(
        "HANDOFF read-order omission",
        handoff_omission,
        tracked_paths,
        expected_message="disagrees with the exact canonical planning order",
    )

    handoff_insertion = re.sub(
        r"^(24\. `docs/TIMELINES\.md`)$",
        r"\1\n25. `Cargo.toml`",
        handoff_text,
        count=1,
        flags=re.MULTILINE,
    )
    if handoff_insertion == handoff_text:
        raise PlanningError("self-test fixture drift: HANDOFF read-order insertion")
    expect_handoff_failure(
        "HANDOFF read-order insertion",
        handoff_insertion,
        tracked_paths,
        expected_message="disagrees with the exact canonical planning order",
    )

    handoff_duplicate = handoff_text.replace(
        "24. `docs/TIMELINES.md`",
        "24. `docs/PROTOCOL.md`",
        1,
    )
    if handoff_duplicate == handoff_text:
        raise PlanningError("self-test fixture drift: HANDOFF read-order duplicate")
    expect_handoff_failure(
        "HANDOFF read-order duplicate",
        handoff_duplicate,
        tracked_paths,
        expected_message="contains duplicate paths",
    )

    handoff_gap = handoff_text.replace(
        "2. `docs/PROJECT_INTENT.md`",
        "3. `docs/PROJECT_INTENT.md`",
        1,
    )
    if handoff_gap == handoff_text:
        raise PlanningError("self-test fixture drift: HANDOFF numbering gap")
    expect_handoff_failure(
        "HANDOFF numbering gap",
        handoff_gap,
        tracked_paths,
        expected_message="numbering is not continuous",
    )

    handoff_swap = handoff_text.replace(
        "1. `docs/HANDOFF.md`",
        "1. `docs/PROJECT_INTENT.md`",
        1,
    ).replace(
        "2. `docs/PROJECT_INTENT.md`",
        "2. `docs/HANDOFF.md`",
        1,
    )
    if handoff_swap == handoff_text:
        raise PlanningError("self-test fixture drift: HANDOFF read-order swap")
    expect_handoff_failure(
        "HANDOFF read-order swap",
        handoff_swap,
        tracked_paths,
        expected_message="disagrees with the exact canonical planning order",
    )

    handoff_description = handoff_text.replace(
        "24. `docs/TIMELINES.md`",
        "24. `docs/TIMELINES.md` (prose must not impersonate structured metadata)",
        1,
    )
    if handoff_description == handoff_text:
        raise PlanningError("self-test fixture drift: HANDOFF path-only row")
    expect_handoff_failure(
        "HANDOFF read-order trailing description",
        handoff_description,
        tracked_paths,
        expected_message="is not canonical",
    )

    handoff_outside = handoff_text.replace(
        "24. `docs/TIMELINES.md`",
        "24. `../outside-read-order-negative-control.md`",
        1,
    )
    if handoff_outside == handoff_text:
        raise PlanningError("self-test fixture drift: HANDOFF outside path")
    expect_handoff_failure(
        "HANDOFF outside-repository read-order path",
        handoff_outside,
        tracked_paths,
        expected_message="path escapes repository",
    )

    handoff_untracked = handoff_text.replace(
        "24. `docs/TIMELINES.md`",
        "24. `docs/untracked-read-order-negative-control.md`",
        1,
    )
    if handoff_untracked == handoff_text:
        raise PlanningError("self-test fixture drift: HANDOFF untracked path")
    expect_handoff_failure(
        "HANDOFF untracked read-order path",
        handoff_untracked,
        tracked_paths,
        expected_message="is not tracked or staged",
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
        "| `OBSERVE-0` | `PS-002:OBSERVE-0@base=IMPLEMENTED`, "
        "`PS-003:OBSERVE-0@claim-scope=BOUNDED` | `WORLD-0` |",
        "| `OBSERVE-0` | `PS-002:OBSERVE-0@base=IMPLEMENTED`, "
        "`PS-003:OBSERVE-0@claim-scope=BOUNDED` | `TICK-0` |",
        1,
    )
    if foundation_cycle == text:
        raise PlanningError("self-test fixture drift: OBSERVE-0 foundation row")
    expect_failure(
        "foundation dependency cycle",
        foundation_cycle,
        enforce_manifest=False,
        expected_message="base dependency cycle:",
    )

    missing_field = text.replace("- **Gate:**", "- **Removed gate:**", 1)
    expect_failure("missing required field", missing_field)

    duplicate = text + "\n### `ACCOUNT-1` — duplicate negative control\n"
    expect_failure("duplicate node", duplicate)

    bad_status = text.replace(
        "- **Status:** `PS-009:ACCOUNT-1@base=READY`.",
        "- **Status:** **MAGICALLY SECURE**.",
        1,
    )
    if bad_status == text:
        raise PlanningError("self-test fixture drift: ACCOUNT-1 status")
    expect_failure("invalid status vocabulary", bad_status)

    false_promotion = text.replace(
        "- **Status:** `PS-009:ACCOUNT-1@base=READY`.",
        "- **Status:** `PS-009:ACCOUNT-1@base=IMPLEMENTED`.",
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

    frontier_locked = text.replace(
        "| 4 | `PS-017:VERKLE-R1@feasibility=READY` |",
        "| 4 | `PS-019:VERKLE-R1@prototype=LOCKED` |",
        1,
    )
    if frontier_locked == text:
        raise PlanningError("self-test fixture drift: locked READY-frontier substitution")
    expect_ready_frontier_failure(
        "locked phase projected into READY frontier",
        frontier_locked,
        expected_message="is not READY",
    )

    frontier_substitution = text.replace(
        "| 1 | `PS-009:ACCOUNT-1@base=READY` |",
        "| 1 | `PS-010:LOCAL-BOUNDS-1@base=READY` |",
        1,
    )
    if frontier_substitution == text:
        raise PlanningError("self-test fixture drift: READY-frontier tuple substitution")
    expect_ready_frontier_failure(
        "unknown tuple substituted into READY frontier",
        frontier_substitution,
        expected_message="disagrees with the ordered authoritative READY rows",
    )

    frontier_omission = text.replace(
        "| 14 | `PS-087:RELEASE-R1@framework=READY` |\n",
        "",
        1,
    )
    if frontier_omission == text:
        raise PlanningError("self-test fixture drift: READY-frontier omission")
    expect_ready_frontier_failure(
        "READY-frontier omission",
        frontier_omission,
        expected_message="disagrees with the ordered authoritative READY rows",
    )

    frontier_duplicate = text.replace(
        "| 14 | `PS-087:RELEASE-R1@framework=READY` |",
        "| 14 | `PS-087:RELEASE-R1@framework=READY` |\n"
        "| 15 | `PS-009:ACCOUNT-1@base=READY` |",
        1,
    )
    if frontier_duplicate == text:
        raise PlanningError("self-test fixture drift: READY-frontier duplicate")
    expect_ready_frontier_failure(
        "READY-frontier duplicate",
        frontier_duplicate,
        expected_message="duplicates status ID",
    )

    frontier_reorder = text.replace(
        "| 1 | `PS-009:ACCOUNT-1@base=READY` |",
        "| 1 | `PS-015:COMMIT-R1@base=READY` |",
        1,
    ).replace(
        "| 2 | `PS-015:COMMIT-R1@base=READY` |",
        "| 2 | `PS-009:ACCOUNT-1@base=READY` |",
        1,
    )
    if frontier_reorder == text:
        raise PlanningError("self-test fixture drift: READY-frontier reorder")
    expect_ready_frontier_failure(
        "READY-frontier reorder",
        frontier_reorder,
        expected_message="disagrees with the ordered authoritative READY rows",
    )

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

    authority_ownership_mutations = (
        (
            "CONSENSUS-1",
            "`AcceptedConsensusNativeParentAuthorityCertificateV1`",
            "`OmittedLiveParentAuthorityCertificateV1`",
            "CONSENSUS-1 authority-ownership artifact omits",
        ),
        (
            "CONSENSUS-1",
            "exactly one accepted admission per",
            "an unspecified number of admissions per",
            "CONSENSUS-1 authority-ownership artifact omits",
        ),
        (
            "CONSENSUS-1",
            "no replay checkpoint or normalized archive view selects admission",
            "a replay checkpoint may select admission",
            "CONSENSUS-1 authority-ownership artifact omits",
        ),
        (
            "SYNC-1",
            "opaque bounded",
            "authority-interpreted",
            "SYNC-1 authority-ownership artifact omits",
        ),
        (
            "SYNC-1",
            "After `CONSENSUS-1@base`",
            "Before `CONSENSUS-1@base`",
            "SYNC-1 authority-ownership artifact omits",
        ),
        (
            "SYNC-1",
            "replayed authority as live consensus input",
            "replayed authority only as live consensus input",
            "SYNC-1 authority-ownership gate omits",
        ),
        (
            "SYNC-1",
            "already accepted certificate protocol owned by `CONSENSUS-1`",
            "certificate protocol selected by replay",
            "SYNC-1 authority-ownership gate omits",
        ),
        (
            "SYNC-1",
            "archive order is not live authority",
            "archive order is live authority",
            "SYNC-1 authority-ownership gate omits",
        ),
    )
    for index, (node_id, fragment, replacement, expected_message) in enumerate(
        authority_ownership_mutations,
        1,
    ):
        mutated_ownership = replace_detail_section_fragment(
            text,
            node_id,
            fragment,
            replacement,
        )
        expect_failure(
            f"authority ownership split mutation {index}",
            mutated_ownership,
            expected_message=expected_message,
        )

    nodes = parse_capability(text)
    phase_statuses = parse_phase_statuses(text, nodes)
    phase_dependencies = parse_phase_dependencies(text, nodes, phase_statuses)

    prototype_projection = "`PS-019:VERKLE-R1@prototype=LOCKED`"
    missing_projection = text.replace(f", {prototype_projection}", "", 1)
    if missing_projection == text:
        raise PlanningError("self-test fixture drift: missing status projection")
    expect_status_projection_failure(
        "missing secondary status projection",
        missing_projection,
        expected_message="checked status projections disagree with authoritative phase-status rows:",
    )

    duplicate_projection = text.replace(
        prototype_projection,
        f"{prototype_projection}, {prototype_projection}",
        1,
    )
    expect_status_projection_failure(
        "duplicate secondary status projection",
        duplicate_projection,
        expected_message="duplicate status projection ID PS-019",
    )

    unknown_projection_id = text.replace(
        prototype_projection,
        "`PS-999:VERKLE-R1@prototype=LOCKED`",
        1,
    )
    expect_status_projection_failure(
        "unknown status projection ID",
        unknown_projection_id,
        expected_message="checked status projections disagree with authoritative phase-status rows:",
    )

    unknown_projection_phase = text.replace(
        prototype_projection,
        "`PS-019:VERKLE-R1@unknown-phase=LOCKED`",
        1,
    )
    expect_status_projection_failure(
        "unknown status projection phase",
        unknown_projection_phase,
        expected_message="checked status projections disagree with authoritative phase-status rows:",
    )

    unknown_projection_status = text.replace(
        prototype_projection,
        "`PS-019:VERKLE-R1@prototype=MAGIC`",
        1,
    )
    expect_status_projection_failure(
        "unknown status projection token",
        unknown_projection_status,
        expected_message="status field is not a canonical projection token list:",
    )

    prose_only_promotion = text.replace(
        prototype_projection,
        f"{prototype_projection}; **READY@prototype**",
        1,
    )
    expect_status_projection_failure(
        "prose-only secondary status promotion",
        prose_only_promotion,
        expected_message="status field is not a canonical projection token list:",
    )

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
            expected_message="checked status projections disagree with authoritative phase-status rows:",
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
        "| PD-96 | RELEASE-R1 | candidate | PROFILE-D1 | base |",
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
        "| PD-96 | REFINE-1 | world | ACCOUNT-1 | missing-phase |",
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
        "| PD-96 | RELEASE-R1 | framework | RELEASE-R1 | candidate |",
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
    check_compatible_dependency_union(
        nodes,
        phase_statuses,
        phase_dependencies,
        conditional_edges,
    )

    cross_class_cycle = text.replace(
        "| PD-07 | TESTNET-1 | base | SYNC-1 | replay-authority |",
        "| PD-07 | TESTNET-1 | base | SYNC-1 | replay-authority |\n"
        "| PD-96 | VERKLE-R1 | prototype | SUCCESSOR-ACTIVATION-1 | activation |",
        1,
    )
    cross_class_cycle = cross_class_cycle.replace(
        "| CE-44 | TESTNET-1 | testnet | target-profile | in-place-upgrade | "
        "SUCCESSOR-ACTIVATION-1 | activation |",
        "| CE-44 | TESTNET-1 | testnet | target-profile | in-place-upgrade | "
        "SUCCESSOR-ACTIVATION-1 | activation |\n"
        "| CE-45 | SUCCESSOR-ACTIVATION-1 | activation | target-profile | "
        "verkle-selected | VERKLE-R1 | prototype |",
        1,
    )
    if cross_class_cycle == text:
        raise PlanningError("self-test fixture drift: compatible cross-class cycle")
    cross_phase_statuses = parse_phase_statuses(cross_class_cycle, nodes)
    cross_phase_dependencies = parse_phase_dependencies(
        cross_class_cycle,
        nodes,
        cross_phase_statuses,
        enforce_manifest=False,
    )
    cross_conditional_edges = parse_conditional_edges(
        cross_class_cycle,
        nodes,
        enforce_manifest=False,
    )
    expect_compatible_union_failure(
        "matched PD/CE cross-class cycle",
        nodes,
        cross_phase_statuses,
        cross_phase_dependencies,
        cross_conditional_edges,
        expected_message="compatible base-phase-role dependency union cycle:",
    )

    three_class_nodes = {
        "DOC-0": Node("DOC-0", (), "IMPLEMENTED", (), "negative control"),
        "OBSERVE-0": Node(
            "OBSERVE-0",
            ("WORLD-0",),
            "BOUNDED",
            (),
            "negative control",
        ),
        "WORLD-0": Node("WORLD-0", (), "IMPLEMENTED", (), "negative control"),
    }
    three_class_phase_statuses = (
        PhaseStatus("PS-NG04-1", "DOC-0", "base", "IMPLEMENTED"),
        PhaseStatus("PS-NG04-2", "OBSERVE-0", "claim-scope", "BOUNDED"),
        PhaseStatus("PS-NG04-3", "WORLD-0", "base", "IMPLEMENTED"),
    )
    three_class_phase_dependency = (
        PhaseDependency(
            "PD-NG04",
            "DOC-0",
            "base",
            "OBSERVE-0",
            "claim-scope",
        ),
    )
    three_class_conditional_edge = (
        ConditionalEdge(
            "CE-NG04",
            "WORLD-0",
            "base",
            "ng04-negative-control",
            "ng04-negative-control",
            "DOC-0",
            "base",
        ),
    )
    check_compatible_dependency_union(
        three_class_nodes,
        three_class_phase_statuses,
        (),
        (),
    )
    check_compatible_dependency_union(
        three_class_nodes,
        three_class_phase_statuses,
        three_class_phase_dependency,
        (),
    )
    check_compatible_dependency_union(
        three_class_nodes,
        three_class_phase_statuses,
        (),
        three_class_conditional_edge,
    )
    expect_compatible_union_failure(
        "matched inherited-base/PD/CE three-class cycle",
        three_class_nodes,
        three_class_phase_statuses,
        three_class_phase_dependency,
        three_class_conditional_edge,
        expected_message=(
            "compatible base-phase-role dependency union cycle: "
            "DOC-0@base -> OBSERVE-0@claim-scope -> WORLD-0@base -> DOC-0@base"
        ),
    )
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
    expect_frozen_sixth_audit_failure(
        "frozen sixth audit mutation",
        sixth_audit_bytes + b"\nnegative-control",
    )
    expect_frozen_seventh_audit_failure(
        "frozen seventh audit mutation",
        seventh_audit_bytes + b"\nnegative-control",
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
    for finding_id, fragments in EXPECTED_LOOP6_PAPER_FRAGMENTS.items():
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
            expect_loop6_paper_fragment_failure(
                f"{finding_id} paper fragment mutation {index}",
                mutated_paper,
            )
    for finding_id, fragments in EXPECTED_LOOP7_PAPER_FRAGMENTS.items():
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
            expect_loop7_paper_fragment_failure(
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
    ready_frontier = parse_ready_frontier(capability_text, phase_statuses)
    phase_dependencies = parse_phase_dependencies(
        capability_text,
        nodes,
        phase_statuses,
    )
    conditional_edges = parse_conditional_edges(capability_text, nodes)
    check_compatible_dependency_union(
        nodes,
        phase_statuses,
        phase_dependencies,
        conditional_edges,
    )
    predicate_entailments = parse_predicate_entailments(capability_text, conditional_edges)
    planning_summary = derive_planning_checker_summary(
        capability_text,
        nodes,
        phase_statuses,
        ready_frontier,
        phase_dependencies,
        conditional_edges,
        predicate_entailments,
    )
    check_planning_checker_summary_projection(capability_text, planning_summary)
    check_accepted_parent_exact_invariants(capability_text, "CAPABILITY_TREE.md")
    tracked_paths = git_tracked_or_staged_paths()
    handoff_text = (ROOT / "docs" / "HANDOFF.md").read_text(encoding="utf-8")
    check_handoff_read_order_and_counts(handoff_text, tracked_paths)
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
    sixth_audit_bytes = SIXTH_AUDIT.read_bytes()
    check_frozen_sixth_audit(sixth_audit_bytes)
    seventh_audit_bytes = SEVENTH_AUDIT.read_bytes()
    check_frozen_seventh_audit(seventh_audit_bytes)
    loop7_provenance = check_seventh_audit_addition_identity()
    eighth_audit_bytes = EIGHTH_AUDIT.read_bytes()
    check_frozen_eighth_audit(eighth_audit_bytes)
    loop8_provenance = check_eighth_audit_addition_identity()
    ninth_audit_bytes = NINTH_AUDIT.read_bytes()
    check_frozen_ninth_audit(ninth_audit_bytes)
    loop9_provenance = check_ninth_audit_addition_identity()
    tenth_audit_bytes = TENTH_AUDIT.read_bytes()
    check_frozen_tenth_audit(tenth_audit_bytes)
    loop10_provenance = check_tenth_audit_addition_identity()
    check_audit_provenance_projection(handoff_text, "Loop7ProvenanceV1", loop7_provenance)
    check_audit_provenance_projection(handoff_text, "Loop8ProvenanceV1", loop8_provenance)
    check_audit_provenance_projection(handoff_text, "Loop9ProvenanceV1", loop9_provenance)
    check_audit_provenance_projection(handoff_text, "Loop10ProvenanceV1", loop10_provenance)
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
    check_loop6_paper_fragments(layer1_design_paper_text)
    check_loop7_paper_fragments(layer1_design_paper_text)
    check_loop8_paper_fragments(layer1_design_paper_text)
    check_loop8_exact_schemas(layer1_design_paper_text)
    check_loop9_exact_assignments(layer1_design_paper_text)
    check_loop10_envelope_helper_closure(layer1_design_paper_text)
    check_accepted_parent_exact_invariants(layer1_design_paper_text, "LAYER1_DESIGN_PAPER.md")
    frozen_paths = frozenset(path.resolve() for path in FROZEN_TEXT_FILES)
    projection_texts = {
        repository_relative_path(path): path.read_text(encoding="utf-8")
        for path in PLANNING_FILES
        if path.resolve() not in frozen_paths
    }
    check_no_duplicate_exact_projections(projection_texts, planning_summary)
    for path in PLANNING_FILES:
        check_markdown(
            path,
            path.read_text(encoding="utf-8"),
            tracked_paths=tracked_paths,
        )
    if args.self_test:
        run_self_tests(
            capability_text,
            handoff_text,
            first_audit_bytes,
            second_audit_bytes,
            design_audit_bytes,
            successor_audit_bytes,
            third_audit_bytes,
            fourth_audit_bytes,
            fifth_audit_bytes,
            sixth_audit_bytes,
            seventh_audit_bytes,
            eighth_audit_bytes,
            ninth_audit_bytes,
            tenth_audit_bytes,
            layer1_design_audit_bytes,
            design_receipt_bytes,
            design_text,
            layer1_design_paper_text,
            design_receipt,
            tracked_paths,
            frozen_eol_policy,
            planning_summary,
            loop7_provenance,
            loop8_provenance,
            loop9_provenance,
            loop10_provenance,
            projection_texts,
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
