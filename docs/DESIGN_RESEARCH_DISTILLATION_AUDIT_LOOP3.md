# Third bounded successor audit

**One read-only gap sweep followed by one coordinated fix sweep — 2026-08-27**

This successor audit freezes exact clean input
`54926f52fa8ba9768e817ecb6139dcca41960e6a`, Git tree
`535734b31ff630a06d9efbb1b947cb4b17a1dca8`, on
`codex/bitblock-progress-2026-08-26`. The remote branch matched that commit before discovery began.

The principal inspected blobs were:

```text
1f3fac587dcb985ba9ab5d5cbe9847d01b803ed3  docs/LAYER1_DESIGN_PAPER.md
82777de700db9ca23cfdc1289c057fb928883bfa  docs/AUTHENTICATED_STATE_PLAN.md
2dd6803049d447440810bfe34aa623d9ab66796b  docs/CAPABILITY_TREE.md
71cd3fda62d38f1fffe605ebea97542a356b920d  docs/DESIGN_RESEARCH_DISTILLATION.md
4e9b6a50e249ae2f21f2f9bae7fb262f8b74007b  docs/HANDOFF.md
e8bd912f171d64c345b6fa71413bd9503e12f5f3  docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP2.md
c3005e7fc98971a7747ab125f1624d08aff31199  docs/design-source-receipt-v1.json
70a91c1a202df0f07573edc1a2aed28949b1f138  tools/check_planning_docs.py
df441fea73fd82c9709f14fb442a466fa13d314a  .github/workflows/ci.yml
5166b367f6d00173bdc1a030b57e658366f21536  README.md
```

Three independent read-only lenses attacked protocol/type inhabitation, capability-instance
closure, and provenance/checker/CI behavior. A fourth local pass reconciled overlaps, challenged
four additional candidates, and rejected two that did not break the model. No audited input was
edited and no Rust, Lean, broad Python-vector, headless, network, storage-fault,
cryptographic-implementation, performance, or physical-display test was run before this finite set
was frozen.

This record preserves findings and break witnesses, not verbatim agent transcripts or agent
identity. It must be committed before the correction. The correction may pin this file's
first-addition Git blob but must not mutate this file.

## Frozen finding set and one-sweep correction map

| ID | Priority | Gap at `54926f5` | Concrete loss mechanism | One-sweep correction |
| --- | --- | --- | --- | --- |
| TG-01 | P0 | `genesis_package` contains only `ChainGenesisPayloadV1` and `genesis_id`, while the initial `profile_id`, profile-bound transition specification, `PROFILE-1` admission, root-parity record, and release identity are derived later. Bootstrap and genesis replay nevertheless accept a pinned genesis package without a post-ID launch binding. | Two initial candidates can share the same genesis-owned scope/registry while differing in later candidate fields and admission/spec/root records. The first history can self-select its interpretation, or verifiers must import an unlisted local choice. | Keep `genesis_id` non-circular, then define a canonical post-ID launch manifest binding the one initial profile, transition specification, admitted-manifest digest, typed genesis root/parity record, and release/authorization references. Bootstrap and genesis replay pin and verify both genesis and launch objects, and the first record must match them. |
| TG-02 | P0 | A migration carries old and new profile contexts but no unique envelope codec/hash owner. The header repeats one top-level `protocol_version`, the prose calls it a parent-selected envelope version distinct from old/new execution versions, and bare envelope `H` calls remain profile-resolved. | When old and new profiles select different versions or suites, conforming implementations can encode/hash the body, context, history, and block identifier under different profiles, or find the claimed projection unsatisfiable. | Define one deterministic envelope owner: the active profile for ordinary blocks and the old parent-admitted profile for migration blocks. Resolve the top-level version, canonical codec, object-hash suite, proposal suites, and every envelope-level identifier through that owner; reserve old/new contexts for typed execution roots. |
| TG-03 | P0 | `Body` always permits ordered signed transactions, each transaction names one profile, and the migration checklist never says whether those transactions are forbidden, old-profile, new-profile, or phase-tagged. The distillation separately promises no ambiguously interpreted ordinary action. | The same migration proposal can be rejected, execute an action before migration under old rules, or execute it after migration under new rules, yielding different validity or roots. | Make the first migration form dedicated and total: its signed-transaction sequence is empty and its schedule contains exactly the authorized migration system transition. A later mixed form would need a separately versioned phase-tagged schema and total ordering. |
| TG-04 | P1 | The numbered proposal checklist recomputes body/payload commitments before enforcing limits, obtains nested receipts/witnesses only afterward, and defers parent-authorized policy/root equality until after execution. | An oversized nested payload can force decode/hash/allocation before its cap, or an unauthorized policy/root can be consumed before rejection, exposing deterministic denial of service and authority-before-validation. | Reorder the verifier: bounded outer framing first; accepted-parent and envelope-owner resolution next; parent-authorized limits, roles, policies, suites, and signature checks before nested materialization or execution; then bounded commitments/payload decode, execution, and post-state/successor checks. |
| TG-05 | P1 | `verkle-selected` makes candidate scope consume only `VERKLE-R1@feasibility`. The prototype dependencies are activated by `verkle-prototype`, and no `PROFILE-1@new-admission` edge requires the candidate-bound prototype. | A profile can name the feasibility descriptor and reach its declared admission closure without implementation, adversarial proof, cost, availability, or migration evidence, despite the authenticated-state plan saying those gates precede admission. | Add a `verkle-selected` edge from `PROFILE-1@new-admission` to the exact candidate-bound `VERKLE-R1@prototype`, update the profile gate/non-claim, and power its polarity/presence. A prototype is necessary, not sufficient, for authority. |
| TG-06 | P1 | Frozen audit/receipt guards hash raw checkout bytes, the repository has no EOL policy, and Windows uses `core.autocrlf=true`. Git's filtered checkout bytes produce different raw hashes from the correct stored blobs. | A fresh Windows clone can fail every frozen-byte guard while Ubuntu CI passes and the repository objects are unchanged, making the handoff non-reproducible across supported hosts. | Pin LF checkout policy for every frozen text artifact or hash its Git-clean canonical form, guard that policy mechanically, and add a powered checkout-EOL control without weakening content-mutation detection. |
| TG-07 | P1 | The planning checker iterates its current `PLANNING_FILES` tuple but pins neither the exact authority-path set nor its digest. | Removing `docs/PROTOCOL.md` from the tuple makes the complete powered self-test pass while reporting only 15 files, silently dropping a handoff-prescribed authority surface. | Pin the exact normalized repository-relative inventory and add a one-file-removal negative control before iterating it. New or removed authority files require an explicit guard update. |
| TG-08 | P1, unclosed SG-02 surface | `PROFILE-D1` reaches only the immutable launch scope through `GENESIS-D1`; it has no direct current-candidate `PROFILE-SCOPE-D0` dependency even though an in-place successor may retain genesis while selecting a different scope/registry. | The graph can schedule a successor candidate with only old launch-scope evidence, or force regeneration of genesis to obtain the new scope that the concrete candidate bytes reference. | Add an always-active, role-aware candidate-to-current-scope edge, or an equivalently checked direct dependency. Initial candidates may resolve it to the genesis-owned scope; successors must resolve it to their distinct current scope while retaining immutable `GENESIS-D1`. Power the role distinction. |

## Candidates rejected after challenge

- `ConsensusHead` carries raw root bytes, but it also carries `block_id`; that identifier commits a
  header containing `profile_transition_context` and typed `ExecutionRootRefV1` values. The head is
  a node-view projection, not a separate signed authority carrier, so profile typing is transitively
  recoverable. A later standalone head certificate would need its own explicit type.
- The summary phrases “target candidate” and replay “before admitting” do not reverse the controlling
  lifecycle. The target is the admitted profile's underlying candidate object; offline bounded
  migration replay supplies evidence before `PROFILE-1` admission, and consensus activation occurs
  only afterward.
- Requiring the historical launch root-parity artifact of every same-chain successor is not a claim
  that genesis parity is rerun for the new profile. `UPGRADE-1` separately supplies successor
  old/new-root evidence.
- Default-only CI Rust commands are accurately disclosed in `TIMELINES.md`; all-feature execution is
  part of the later broad local test phase, not a hidden claim that the current CI already runs it.

## Areas checked with no additional material gap

- SG-01's pre-admission identity-hash chain, SG-03's canonical availability assessment carrier, and
  SG-05 through SG-11's frozen provenance/checker/CI corrections remain intact.
- The current conditional-edge and predicate-entailment manifests are canonical, acyclic under their
  declared union, and powered. Deciding predicate truth and validating complete realizable profile
  closures remains explicitly locked implementation work.
- MPT/SMT canonicalization, Verkle scheme/setup separation, state/history/snapshot separation, and
  cryptographic/security non-claims did not broaden.
- Every current authority path is tracked and structurally passes today; TG-07 concerns silent future
  removal from that inventory, not a presently missing file.

## One-fix-sweep rule

The correction may change only the protocol/planning/checker/handoff surfaces needed to close
TG-01 through TG-08, connect this audit to the read order, and power the new controls. It may not
activate a capability, implement a network or cryptographic primitive, edit any previously frozen
audit, or silently add a ninth finding. Any later discovery belongs to another pinned loop.

The correction is a documentation, checker, attribute-policy, and handoff change. Narrow checker
execution and whitespace checks are correction gates; broad runtime/proof tests begin only after a
complete later read-only sweep reaches the stated diminishing-return condition.

## Residual boundary

Closing these findings cannot prove that the protocol is secure, that a migration or Verkle profile
is implementable within an acceptable budget, that availability persists, that cited cryptography
is sound, that CI infrastructure is honest, or that Rust refines the planning documents. The next
complete sweep must pin the correction and may declare diminishing returns only if it finds no new
material contradiction.
