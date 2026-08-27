# Eighth bounded successor audit

**One read-only gap sweep followed by one coordinated fix sweep — 2026-08-27**

This successor audit freezes exact tracked input
`e3650eb5d4ea390ac0023cb67325bae3c5351c60`, Git tree
`91378e4fe123a96298d73905ea2b1b3974137791`, on
`codex/bitblock-progress-2026-08-26`. The remote branch matched that commit before this record was
written, and the tracked worktree had no staged or unstaged difference. The commit/tree above, not
ambient files or later prose, is the audited input.

The principal inspected blobs were:

```text
b6ddcd641b50073dc42bd205ada9388314447935  docs/LAYER1_DESIGN_PAPER.md
767a74f23d1c530f811967748a2b140dae2dbfa9  docs/AUTHENTICATED_STATE_PLAN.md
8cbae366874d0d85dfb8fba48a64a1b8d723f97a  docs/CAPABILITY_TREE.md
1e000bf69ad0fa766cc42a7a513f7cde96a5b22b  docs/DESIGN_RESEARCH_DISTILLATION.md
159c2d15dca781a13b385972b2579d64318773c0  docs/HANDOFF.md
dfff19eddc3ab2d4d36f8b7f4e38c296c26f10c2  docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP7.md
124e066b418a61279abf90c1b1b2c2cc0604ef2a  docs/LAYER1_DESIGN_AUDIT.md
b4a43738fd7e119bdd4652fa363c3cfcb7520864  docs/TRANSCRIPT_AUDIT.md
995f037f67217f1e92cb926a3cfb094d3816868d  docs/TIMELINES.md
cfc5ebf6305ca5e3b7255fa18745e68b2fa022c9  tools/check_planning_docs.py
df441fea73fd82c9709f14fb442a466fa13d314a  .github/workflows/ci.yml
f531ffa978de3aeff38f8bc80a2cb3da53f967b2  .gitattributes
88aec6a7cbe96cf609730cbf11f350044ff9a1ad  README.md
```

Independent read-only lenses attacked protocol/type/cryptographic closure,
state/availability/replay closure, and capability-graph/checker/provenance closure. Follow-up
challenges independently reproduced the successor release-versus-activation purpose loss, the
uncommitted replay-evidence view, and the erasure of historical consumer time. Separate mutation
passes restored proposer-signature bytes to the indexed core, swapped both collision dispositions,
and falsified displayed provenance and graph counts while the relevant checker surfaces remained
green. The passes were reconciled into six material findings. No audited input was edited during
discovery.

The narrow planning checker and whitespace check were the only repository checks run:

```text
python -B tools/check_planning_docs.py --self-test
planning-doc check PASS: 72 capability nodes, 94 phase-status tuples,
360 declared base-plus-phase-plus-conditional edges, 10 predicate entailments,
21 Markdown files and 1 frozen source receipt with negative controls

git diff --check
PASS (exit 0, no output)
```

Read-only in-memory mutations demonstrated two powered false-closure classes. First, adding
`canonical_proposer_signature_bytes` back inside `AcceptedProposalCoreV1`, or swapping the two
normative parent-collision dispositions in both the paper and capability tree, passed the Loop 5,
Loop 6, Loop 7 fragment, graph, status, frontier, predicate, inventory, and Markdown checks.
Second, falsifying each displayed Loop 7 input/addition/tree/blob identity, graph summary count, or
the read-order finding-count label also passed while the underlying Git objects and parsed graph
remained correct. No Rust, Lean, broad Python-vector, headless, network, storage-fault,
cryptographic-implementation, performance, or physical-display test was run.

This record preserves findings and break witnesses, not verbatim agent transcripts or agent
identity. It must be committed before the correction. The correction may pin this file's
first-addition Git blob but must not mutate this file.

## Frozen finding set and one-sweep correction map

| ID | Priority | Gap at `e3650eb5` | Concrete loss mechanism | One-sweep correction |
| --- | --- | --- | --- | --- |
| EG-01 | P1, successor authority purpose | `VerifiedSuccessorPredecessorContextV1` carries only an eligible `SuccessorActivationAuthorityV1` parent result even though successor release verification separately requires `SuccessorReleaseAuthorityV1`. Parent eligibility is purpose-specific in `(core, use, C, E)`, and the bundle intentionally excludes the predecessor core and evidence after context resolution. | A verifier must either coerce activation eligibility into release eligibility, fetch excluded ambient evidence, or reject an otherwise valid release. Two implementations can therefore accept different successor bundles from the same bytes without violating the written purpose-specific eligibility rule. | Carry distinct purpose-tagged release and activation references plus both verified eligibility results in the predecessor context. Require them to resolve to the same unique parent core and governance source while preserving separate use, policy, predicate, status, and consuming-reference bindings. The embedded release reference must equal the release result; the activation signing context must equal the activation result. Power missing-release, purpose-coercion/swap, one-purpose-only, different-core/same-governance, policy/predicate, cutoff, and wrapper-selected-authority controls. |
| EG-02 | P1, replay archive commitment | `ReplayEvidenceCutoffV1` names an `authenticated_evidence_archive_root`, cutoff kind/height/event ordinal, and normalization policy, but no canonical evidence-event grammar, root transition, event ordering, authorization, inclusion proof, prefix/range-completeness proof, or replay-wide view derives the normalized snapshots from that root. The same-wrapper replay witness also lacks an outer-wrapper occurrence commitment. | A replay package can omit a conflicting pre-cutoff event, insert a post-cutoff event, reorder evidence, mix roots or policies between entries, or synthesize a consensus-evidence/availability-assessment pair from separate wrappers while still asserting the same root and ordinal. The root is a label, not yet a proof-bearing commitment. | Define canonical typed archive events and IDs, deterministic event order and bounds, a domain-separated authorized checkpoint transition, inclusion plus prefix/range-completeness witnesses, and one outer `ReplayEvidenceViewV1` shared by the complete replay package. Bind bootstrap/snapshot data to the checkpoint identity and authorization. Bind same-wrapper claims to a committed outer-wrapper occurrence containing the exact core, consensus evidence, and assessment. Power permutation, omitted conflict, post-cutoff insertion, off-by-one, root/policy substitution, mixed views, duplicate events, synthetic cross-wrapper pairs, and publication-crash controls. |
| EG-03 | P1, historical replay authority | Replay entries expose one untagged attained parent-authority status even though eligibility is purpose-specific for envelope, release, and activation use. A named replay cutoff can also include evidence that arrived only after a child, release, or activation consumed the parent. The retained witness does not bind the purpose, policy, predicate, consumer, or historical evidence view. | Later evidence can retroactively make an invalid historical action look authorized; envelope eligibility can be substituted for release or activation eligibility; and per-entry cutoffs can combine incompatible views. A current status projection and proof that an earlier transition was authorized are different claims but are represented as one. | Separate historical authorization from later status projection. For every consumed authority use, retain a purpose-scoped witness binding the unique parent lookup/core, exact `AcceptedParentAuthorityUseV1`, policy, predicate, shared replay view/cutoff/event, dependency-closed evidence, status, and exact consuming child/release/activation reference. Require coverage without omission or duplication for the replayed sequence. Use one replay-wide named projection for later status; late evidence may update that projection but never historical authorization. Power purpose substitution, missing/duplicate purpose, wrong policy/predicate/consumer, evidence-after-consumption, cross-cutoff union, and mixed-view controls. |
| EG-04 | P2, uninhabitable recovery control | The current specification explicitly defines no `PrecommittedIndependentRecoveryEnvelopeV1`, so established active-profile authority-binding compromise safe-halts and requires new genesis. It nevertheless requires a positive matched recovery-envelope control that may follow a frozen recovery path. | No schema, identifier, pre-launch commitment, independent authority/signing domain, last-safe-parent rule, transition, or failed-role-independence condition exists from which the required positive vector can be constructed. A checker can only fake the positive or silently choose recovery semantics that remain `OPEN`. | For the current profile, make every established-compromise control safe-halt to new genesis and classify positive independent recovery as future-conditional, not a present acceptance vector. A future positive path must first define its exact envelope schema/ID, independent genesis commitment and authority, signing domain, last-safe selection, transition, availability, failed-role independence, and powered tests. Preserve preventive migration while the binding is still sound. |
| EG-05 | P1, falsely closed exact Loop 7 invariants | Loop 7 checker fragments require marker presence/count but do not parse the exact signature-free `AcceptedProposalCoreV1` field tuple or the direction of the two parent-collision dispositions. Mutations restored signature bytes to the core or swapped both mappings in the paper and capability gate while all relevant checks passed. | Signature multiplicity can again split one lookup into several alleged cores; alternatively, different lookup-payload preimages can be misrouted to profile-owned compromise while same-lookup/same-block-ID unsigned-core divergence is misrouted to protocol identity-hash compromise. The precise LG-01/LG-02 losses reopen while the checker claims them pinned. | Parse and compare the exact ordered signature-free core field tuple, including explicit exclusions, and define one canonical two-row `AcceptedParentCollisionDispositionV1` projection shared by paper and capability tree. Power field addition/removal/reorder, signature insertion, mapping omission/merge/reorder, and each single or double class swap. This is a narrow exact-invariant guard, not a claim to parse arbitrary prose semantics. |
| EG-06 | P2, exact projection/provenance drift | README, HANDOFF, and TRANSCRIPT_AUDIT repeat Loop 7 input/addition/tree/blob/finding facts; HANDOFF and CAPABILITY_TREE repeat graph, status, edge, predicate, inventory, frozen-artifact, and finding-count summaries. Existing checks validate the canonical Git objects and parsed graph independently but do not equate every display to those sources. | A future agent can be handed false provenance, a false graph size, or an incorrect audit finding count while every authoritative underlying object and powered checker remains green. Duplicate exact facts become independent mutable claims. | Define structured `Loop7ProvenanceV1` and `PlanningCheckerSummaryV1` projections derived from Git and parser results. Remove duplicate literals where a stable link suffices; otherwise parse and compare every remaining projection. Keep canonical handoff read-order rows path-only so commentary cannot impersonate structured metadata. Power wrong input/addition/tree/blob/finding count, every displayed graph/count field, and duplicate-projection drift controls. |

## Candidates rejected or classified after challenge

- Release and activation may intentionally resolve the same parent core and governance source, but
  that equality does not erase their distinct eligibility purposes. EG-01 preserves shared identity
  while refusing a purpose coercion.
- An archive commitment can support replay acceleration without becoming consensus state. EG-02
  requires the replay claimant to prove one finite evidence view; it does not promote snapshots or
  local archive retention into authority.
- Same-wrapper live ingestion remains atomic under the Loop 7 grammar. EG-02 concerns proving that
  occurrence during independent replay, not changing the valid live-ingestion rule.
- Current-status replay and historical-authorization replay are both useful. EG-03 separates them
  instead of selecting one and silently discarding the other.
- Preventive migration of a still-sound but deprecated suite remains allowed. EG-04 applies only
  after an authority-binding role is established compromised; the missing recovery object cannot
  be invented by a test.
- The exact two-layer collision classification itself is coherent: different lookup-payload
  preimages under one profile-ID hash output are a protocol identity-hash compromise; equal
  lookup/block ID with distinct valid unsigned cores is a profile-owned authority-binding
  compromise; an invalid projection is simply invalid. EG-05 is a guard failure, not a third
  collision class.
- Concrete consensus, signature, commitment, and evidence codecs; numeric retention limits;
  durable storage/fsync policy; availability protocol; and recovery authority remain `OPEN` or
  `LOCKED`. No finding promoted one into a decided current mechanism.
- Arbitrary Markdown semantics remain outside the checker's claim. EG-06 is limited to exact facts
  already presented as provenance or parser-derived summaries.

## Areas checked with no additional material gap

- The signature-free core and separately typed credential-set design, bounded canonical
  credential ordering/deduplication, and no-double-weight rule are semantically coherent in the
  audited documents; EG-05 records that their guard was weaker than their prose.
- The first-record launch branch, profile-scoped parent branch, preventive-versus-established
  compromise split, and launch combined internal/external dependency DAG survived the bounded
  protocol and state attacks apart from EG-04's impossible current positive recovery vector.
- Successor internal-only `7 + Length(S)` bundle closure and its exclusion of predecessor bytes are
  coherent. EG-01 concerns the incomplete pre-resolved predecessor authority context, not the
  internal graph equations.
- The 72 capability nodes, 94 phase-status rows, 311 base edges, five phase dependencies, 44
  conditional edges, 360 combined edges, ten predicate entailments, and fourteen ordered READY
  frontier rows survived direct structural checks.
- The 21-document planning inventory, eleven frozen artifacts, frozen Loop 7 audit Git identity and
  first-addition history, effective LF attributes, and repository-contained paths were correct in
  the audited tree. EG-06 concerns mutable secondary displays of those correct facts.
- One-core-per-height normalization, plural ingestion wrappers, typed state/history roots, MPT
  normalization, sparse-Merkle defaults, logical pruning versus physical garbage collection,
  Verkle's research-only boundary, and the snapshot/archive non-authority boundary did not produce
  another distinct finding in this finite pass.
- Launch, successor, replay, archive, and recovery structures remain proposed schemas only. No
  runtime implementation, network behavior, cryptographic soundness, proof refinement,
  performance, storage durability, or visual behavior was inferred from the planning checker.

## One-fix-sweep rule

The correction may change only the protocol/planning/checker/handoff surfaces needed to close
EG-01 through EG-06, connect this audit to the frozen inventory/read order, and power the new
controls. It may not activate a capability, implement a network or cryptographic primitive, edit
any previously frozen audit or this file, or silently add a seventh finding. Any later discovery
belongs to another pinned loop.

The correction remains documentation, checker, attribute-policy, and handoff work. Narrow checker
execution and whitespace checks are correction gates. Broad runtime/proof tests begin only after a
complete later read-only sweep finds neither a new material gap nor a falsely closed material
residual under its declared finite lenses, and that zero-gap sweep is itself frozen.

## Residual boundary

Closing these findings cannot prove protocol security, cryptographic uniqueness or soundness,
availability in deployment, consensus safety/liveness, candidate satisfiability, implementation
conformance, honest CI, complete archival evidence, or Rust/Lean refinement. It can only make the
current proposed purpose-scoped authority, archive-view, historical replay, compromise,
exact-invariant, and provenance-projection claims internally explicit enough for the next bounded
attack.
