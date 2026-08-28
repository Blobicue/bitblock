# Ninth bounded successor audit

**One complete read-only gap sweep before any correction — 2026-08-27**

This audit freezes exact tracked input
`e6baa331f89d8b367d97ae218ad8506c103eb82c`, Git tree
`044f210a7ae854a9416573a8f326d414da3ad4b5`, on
`codex/bitblock-progress-2026-08-26`. The remote branch matched that commit before this record was
written, and the tracked worktree had no staged or unstaged difference. The commit/tree above, not
ambient files or later prose, is the audited input.

The principal inspected blobs were:

```text
6ade1cc2bf966f8171e5eb872598b254f37cd08b  docs/LAYER1_DESIGN_PAPER.md
93b5c13bcd4805cc93bdc118ae0ea2e64bdd7992  docs/AUTHENTICATED_STATE_PLAN.md
eec3c80e1e5ea6659e2bd585954415eae1673bef  docs/CAPABILITY_TREE.md
ef2d48d2b6a81d49295108500279cf5cd31cd631  docs/DESIGN_RESEARCH_DISTILLATION.md
11de97c361c8bd00e17a91fa643d17b2df814f49  docs/HANDOFF.md
b247d7585a9e4be06558b0076511fa1064252049  docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP8.md
124e066b418a61279abf90c1b1b2c2cc0604ef2a  docs/LAYER1_DESIGN_AUDIT.md
1875d32017cfefe0d04446b450a58762e02bf818  docs/TRANSCRIPT_AUDIT.md
995f037f67217f1e92cb926a3cfb094d3816868d  docs/TIMELINES.md
455caa0409f28e52f18dbaa5206f4db9f314eda0  tools/check_planning_docs.py
df441fea73fd82c9709f14fb442a466fa13d314a  .github/workflows/ci.yml
973a1f14e72a94e29d9fe399080f88dbbabae3fb  .gitattributes
b6224217ac07583641c344606100bad345a20a82  README.md
```

Independent read-only lenses attacked protocol/type/authority closure, capability-graph/checker
closure, and cross-document provenance/handoff truth. The protocol lens constructed a good wrapper
and a separate pre-admission conflict wrapper for one core, then showed that selecting only the good
receipt satisfies every candidate-local component equality. It also reproduced that byte-identical
outer bytes archived later are indistinguishable from the originally observed occurrence. The graph
lens built a cross-class phase/conditional two-cycle that passed the two separate acyclicity checks,
mutated unparsed certificate/archive fields, falsified audit counts and projection claims, removed
ancestry from first-addition provenance, and showed that the named base-cycle control actually failed
on a digest guard. The truth lens compared the current paper blob and review chronology with its
historical checkpoint-0.3 label. The passes reconciled into seven new material gaps and two falsely
closed material residuals. No audited input was edited during discovery.

The narrow planning checker and whitespace check were the only repository checks run:

```text
python -B tools/check_planning_docs.py --self-test
planning-doc check PASS: 72 capability nodes, 95 phase-status tuples,
363 declared base-plus-phase-plus-conditional edges, 10 predicate entailments,
22 Markdown files and 1 frozen source receipt with negative controls

git diff --check
PASS (exit 0, no output)
```

No Rust, Lean, broad Python-vector, headless, release, network, storage-fault,
cryptographic-implementation, performance, or physical-display test was run. Those tests remain
gated on a later complete zero-gap sweep.

This record preserves findings and break witnesses, not verbatim agent transcripts or agent
identity. It must be committed before the correction. The correction may pin this file's
first-addition Git blob but must not mutate this file.

## Frozen finding set and one-sweep correction map

| ID | Priority | Gap at `e6baa331` | Concrete loss mechanism | One-sweep correction |
| --- | --- | --- | --- | --- |
| NG-01 | P1, live certificate occurrence-universe completeness | `AcceptedParentAuthorityWitnessReceiptCoverageV1` is component-total only over the candidate-selected `ConsensusNativeAcceptedRecordOccurrenceReceiptSetV1`. No consensus-position-bound complete accepted-occurrence/evidence universe or exclusion proof establishes that the selected receipts are all relevant pre-admission occurrences. | For one unique core, a good wrapper and a separate already accepted wrapper carrying valid conflict evidence can exist before admission. A candidate commits only the good receipt. Every receipt/component equality passes because the conflict receipt is absent wholesale, contradicting the claim that conflict material cannot be omitted. | Freeze a consensus-native certificate evidence cutoff and a complete accepted-occurrence universe/root under that cutoff, with authorization/consensus position and exact inclusion plus completeness/exclusion proof. Derive the receipt set and component-total status input from that universe, not candidate choice. Power omitted whole wrapper, omitted conflict, boundary, root, position, and completeness mutations. |
| NG-02 | P2, falsely closed occurrence chronology | Consensus-native receipt IDs and replay occurrence IDs are content-derived; replay event ordinal is archival observation order and the checkpoint may postdate the consumer. The specification nevertheless claims that a matching same-wrapper occurrence first created after admission rejects. | A later event containing byte-identical `AcceptedRecordOuterV1` bytes has the same receipt content and satisfies the replay bridge. No authenticated field distinguishes original live observation from later archival reconstruction, so the required temporal negative control is uninhabited. | Bind each qualifying live occurrence to the consensus-native cutoff/universe and its accepted position before certificate admission. Replay may archive those committed bytes later, but must reconstruct the already positioned occurrence ID. Alternatively, if content recovery is intentionally sufficient, delete the temporal claim/control everywhere; do not claim both. |
| NG-03 | P2, current-paper provenance | The mutable current `LAYER1_DESIGN_PAPER.md` still labels itself “Design paper — checkpoint 0.3”, names only the early future-plan corrections at its top, and describes the successor corrections named there as the scoped planning sweeps. Its current blob is thousands of changed lines beyond the pinned checkpoint-0.3 artifact, while current review chronology runs through frozen design successor Loop 8. | A standalone paper reader can attribute current Loop 1–8 descendant schemas to the old checkpoint/audit, miss the actual review chain, or treat historical checkpoint obligations as the review status of the current descendant. | Preserve `034b7b2`/the pinned audit as historical checkpoint 0.3; relabel the mutable paper as the current descendant proposed reading; link the complete current successor-audit chronology; and state that §14's checkpoint obligations table belongs only to the pinned historical checkpoint. |
| NG-04 | P1, combined dependency-cycle false closure | The checker validates base+phase dependencies and base+conditional role edges in separate graphs. It never checks their compatible node@phase/role union. | A phase edge `VERKLE-R1@prototype -> SUCCESSOR-ACTIVATION-1@activation` and conditional reverse edge each pass their separate acyclicity check while their union is a two-cycle. The current real union is acyclic, but the guard can admit a future cross-class cycle. | Construct one canonical compatible node@phase/role union containing inherited base, PD, and CE edges; reject cycles there; and power a matched cross-class cycle mutation. Keep the separate structural parsers, but do not treat their separate DAG results as union safety. |
| NG-05 | P1, incomplete exact authority/archive schema guard | Loop 8 exact parsing covers only an ad hoc subset while remaining certificate/checkpoint/view/completeness and ID-derivation invariants are marker-only. | Removing checkpoint authorization, ordered event records, prefix proof, previous-checkpoint linkage, evidence commitment from the authorized certificate, attained status, or authorization bytes—or changing a receipt hash context—leaves marker strings elsewhere and passes the full current paper checks. | Extend exact ordered assignment/variant and domain-separated ID guards across the complete live-certificate and replay checkpoint/view/bridge chain. Power every field add/remove/reorder, duplicate assignment, and hash-domain/context substitution on the subject. |
| NG-06 | P1, checker-authored audit cardinality | Loop 7/8 finding counts are constants supplied to `_check_audit_addition_identity`; the checker returns the caller's value without parsing finding IDs from the frozen audit blob. | Supplying Loop 8 count seven and changing the HANDOFF projection to seven passes, even though the frozen table contains exactly EG-01 through EG-06. The checker can agree with its own false premise. | Parse exact unique contiguous finding IDs from each frozen audit table, derive cardinality from the blob, compare its expected prefix/range, and use that derived value in provenance. Power a matched wrong-count projection. |
| NG-07 | P1, incomplete/value-specific sole-projection guard | The duplicate-projection scan searches only already-correct expected numbers/object IDs and omits four mutable planning authorities. | Wrong claims such as 73 nodes, 999 edges, seven Loop 8 findings, wrong 40-hex Loop 8 IDs, or an eight-character canonical abbreviation pass. Correct duplicates fail. `PROJECT_INTENT.md`, `SEMANTICS.md`, `PROTOCOL.md`, and `TIMELINES.md` are not scanned at all. | Scan every non-frozen mutable planning authority. Outside canonical structured tables, reject or parse recognized summary/provenance claims by semantic shape rather than only matching the expected literal. Power wrong numbers, wrong full IDs, disallowed abbreviations, and claims in every mutable file. |
| NG-08 | P2, first-addition reachability | First-addition provenance verifies object existence, parent/tree/blob, and parent absence but does not prove that the addition commit is an ancestor of the checked current lineage. | An unreachable local/stale commit can satisfy the canonical first-addition table. Current Loop 7/8 addition commits are actual HEAD ancestors, so this is missing proof rather than current drift. | Define the checked current ref and require `git merge-base --is-ancestor <addition> <checked-ref>`. Add an injectable/unrelated-commit negative control while preserving exact parent/tree/blob/absence checks. |
| NG-09 | P2, falsely powered base-cycle control | The self-test labeled “foundation dependency cycle” calls a generic failure path while the intentional base-dependency digest guard runs first. | The mutated subject fails with “unconditional base dependencies changed”, not “base dependency cycle”. Removing or breaking the actual acyclicity check could leave the named control green. | Exercise cycle checking with manifest/digest enforcement disabled or with the mutation's expected digest injected, and require the exact `base dependency cycle:` mechanism. Use the same mechanism-specific discipline for the new combined-graph cycle control. |

## Candidates rejected or classified after challenge

- The current real combined graph is acyclic. NG-04 is a checker false closure, not evidence of a
  present capability cycle.
- Candidate-specific certificate subsets may remain useful as proposals, but final admission cannot
  claim component/conflict completeness until a consensus-positioned universe proves the exclusion
  of other relevant occurrences. NG-01 does not require the replay archive to select live authority.
- Replay checkpoints may validly postdate consumers. NG-02 requires them to reconstruct an earlier
  consensus-positioned occurrence, not to become a live clock.
- Byte-identical content recovery is coherent if selected explicitly, but it cannot support the
  simultaneous claim that later identical occurrences are rejected by chronology.
- Current Loop 7/8 provenance values, graph counts/digests, EOL policy, path-only read order, and
  frozen objects all reproduced correctly. NG-06 through NG-09 concern guards that accept their own
  negation or omit a proof.
- Concrete consensus, signature, commitment, and evidence codecs; numeric retention limits;
  deployment availability; operator custody; and recovery authority remain `OPEN` or `LOCKED`.
  None was promoted by this audit.

## Areas checked with no additional material gap

- A finite type/reference pass over the current protocol definitions found no multi-node type SCC;
  certificate core, authorization, admission, accepted reference, and consumer preimages close.
- Live consensus-native authority and replay custody remain separated: replay reconstructs accepted
  authority and does not select it. NG-01 requires consensus-native completeness before admission.
- The staged successor order—base context, release core/certificate, admitted release, bundle ID,
  activation certificate—remains acyclic.
- Current established authority-binding compromise correctly has no inhabited recovery shortcut;
  safe halt plus separately trusted new genesis remains the only current disposition.
- The current graph summary is internally consistent: base, phase, and conditional manifests parse;
  their actual compatible union is acyclic; status/frontier/count/digest projections agree.
- Loop 7/8 addition commits are ancestors of current HEAD, their Git identities reproduce, frozen
  files retain the required EOL policy, and the current tracked worktree and remote branch agree.
- Mutable projections contain no other current semantic contradiction or false implementation,
  security, test, runtime, network, crypto, storage, performance, or display claim under the declared
  finite lenses.

## One-fix-sweep rule

The correction may change only the protocol/planning/checker/handoff surfaces needed to close
NG-01 through NG-09, connect this audit to the frozen inventory/read order, and power the new
controls. It may not activate a capability, implement a network or cryptographic primitive, edit
any previously frozen audit or this file, or silently add a tenth finding. Any later discovery
belongs to another pinned loop.

The correction remains documentation, checker, attribute-policy, and handoff work. Narrow checker
execution and whitespace checks are correction gates. Broad runtime/proof tests begin only after a
complete later read-only sweep finds neither a new material gap nor a falsely closed material
residual under its declared finite lenses, and that zero-gap sweep is itself frozen.

## Residual boundary

Closing these findings cannot prove protocol security, cryptographic uniqueness or soundness,
availability in deployment, consensus safety/liveness, candidate satisfiability, implementation
conformance, honest CI, complete archival evidence, or Rust/Lean refinement. It can only make the
current proposed occurrence-universe, chronology, provenance, combined-graph, exact-schema, and
checker claims explicit enough for the next bounded attack.
