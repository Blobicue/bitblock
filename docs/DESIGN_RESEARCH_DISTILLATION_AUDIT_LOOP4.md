# Fourth bounded successor audit

**One read-only gap sweep followed by one coordinated fix sweep — 2026-08-27**

This successor audit freezes exact clean input
`b23ddc95e5075c35d0dbf918b89f353fcdd40ddd`, Git tree
`3de2d45aa20b08318f1a1af4d93308daccf1f55e`, on
`codex/bitblock-progress-2026-08-26`. The remote branch matched that commit and the worktree was
clean before discovery began.

The principal inspected blobs were:

```text
1e53bc81c3510302445cf7514e0ed61438c6b36b  docs/LAYER1_DESIGN_PAPER.md
7c7490fddfa79efbd818be2dfa2375948f1d9f43  docs/AUTHENTICATED_STATE_PLAN.md
dba15354f0da113d13441340862303ee67d4f8cb  docs/CAPABILITY_TREE.md
cd42b19a398fe6c8155870b8f07cebd549ba36ae  docs/DESIGN_RESEARCH_DISTILLATION.md
f66f53517a64cbab34d8d20321f3445f60c76368  docs/HANDOFF.md
fbee935d27617f47f913617ca2d8e19aba618c6e  docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP3.md
124e066b418a61279abf90c1b1b2c2cc0604ef2a  docs/LAYER1_DESIGN_AUDIT.md
ac8f23037fa3716295ed7d9490b1a77bab7ede57  docs/TRANSCRIPT_AUDIT.md
995f037f67217f1e92cb926a3cfb094d3816868d  docs/TIMELINES.md
7213d159e810707ab4ad437b9e457ee6716b0266  tools/check_planning_docs.py
df441fea73fd82c9709f14fb442a466fa13d314a  .github/workflows/ci.yml
94774e7e94802e7f1d9364c2f41c1cd249dc4c7d  .gitattributes
2fafd6bd920c3a64988cb575898e31f753b9a436  README.md
```

Three independent read-only lenses attacked protocol/type inhabitation, authenticated-state and
operational replay, and capability-instance/provenance closure. A fourth local pass reproduced the
structured-predicate break, reconciled overlaps, separated two falsely closed historical
obligations from genuinely new mechanisms, and challenged additional candidates. No audited input
was edited and no Rust, Lean, broad Python-vector, headless, network, storage-fault,
cryptographic-implementation, performance, or physical-display test was run before this finite set
was frozen.

This record preserves findings and break witnesses, not verbatim agent transcripts or agent
identity. It must be committed before the correction. The correction may pin this file's
first-addition Git blob but must not mutate this file.

## Frozen finding set and one-sweep correction map

| ID | Priority | Gap at `b23ddc9` | Concrete loss mechanism | One-sweep correction |
| --- | --- | --- | --- | --- |
| QG-01 | P0, new | `ChainLaunchManifestV1` binds the initial profile, transition specification, admission, parity, and release references, but the explicit `chain_launch_package` carries only genesis, parity, the manifest/ID, and authorization. Bootstrap and replay nevertheless treat that package as the launch input. | A fresh verifier has hashes for but no declared carrier or mandatory retrieval closure containing the exact `ProfileScopeV1`, `DomainRegistryV1`, `CandidateProfileV1`, `TransitionSpecAtProfileV1`, `PROFILE-1` admission manifest, and `RELEASE-1` record. It cannot independently resolve the first codec/policy closure or verify the manifest references without an undeclared local mapping. | Define a bounded canonical `LaunchClosureBundleV1` that embeds or content-addresses every required preimage, with typed IDs, complete unique inventory, mandatory retrieval, and hash verification. Make it an explicit launch/bootstrap/replay input and verify the acyclic genesis→scope/registry→candidate→specification→admission/parity/release→manifest chain before resolving the first envelope. |
| QG-02 | P0, reopened F-29 | `BlockContext` supplies committed randomness plus application/system schedules and is hashed into `block_context_root`, but neither `AvailabilityPayload` nor another required carrier supplies its canonical preimage. The paper still calls the `AcceptedBlockRecord` sequence a complete replay input. | A proposer can make the declared body/receipt/witness payload available while withholding the exact context bytes required to recompute `block_context_root`, enforce the migration schedule, or execute/replay the transition. Availability evidence can therefore attest an incomplete replay carrier. | Carry canonical `BlockContext` in `AvailabilityPayload` or an equally exact separately committed and availability-covered carrier; bound-decode it, require every repeated projection to match the verified header/parent, recompute its root before execution, and add withheld/substituted-context controls. |
| QG-03 | P0, reopened DR-07 | The authoritative transition is still written as `Execute(state, BlockContext, OrderedTransactions)`, while ordinary and migration `Body` variants separately contain deterministic auxiliary or migration inputs. | Implementations can ignore the committed auxiliary inputs, source them again from `BlockContext`, or consume them from `Body`, producing divergent validity, receipts, or roots from the same accepted bytes. The migration variant has the same omission. | Define execution over the exact canonical `(state, BlockContext, Body)` with total ordinary/migration branches, one owner for every deterministic input, and no duplicate source. Align the abstract relation, validation checklist, receipts, and replay statement. |
| QG-04 | P1, unclosed TG-04 surface | Validation requires a profile-independent bounded outer frame and parent-or-launch reference before the profile codec can be selected, but only profile-owned inner `Proposal` and `AcceptedBlockRecord` canonical objects are typed. The first-record branch also grammatically asks a launch manifest to satisfy an accepted parent's consensus status. | A receiver cannot know how to locate and cap the authority reference or inner byte ranges without first choosing the very profile-controlled parser that the outer frame is meant to precede. Implementations can invent incompatible framing, allocate before limits, or reject/over-trust the first record. | Define fixed pre-profile `ProposalOuterV1` and `AcceptedRecordOuterV1` framing with version/tag, bounded lengths, parent-or-launch lookup reference, and opaque inner bytes whose projections must match. Split ordinary-parent status validation from first-record launch authorization explicitly. |
| QG-05 | P1, new | `verkle-selected` activates CE-41 and CE-42, but the prototype's CE-35–CE-40 crypto/codec/threat/bounds/migration/candidate prerequisites are guarded by `verkle-prototype`; PE-11 points only `verkle-prototype -> verkle-selected`. | The measured closure from seed `verkle-selected` contains only that predicate, so an admission can name `VERKLE-R1@prototype` without structurally activating the prototype's own declared prerequisite closure. | Redesign or unify the predicates so a selected Verkle admission activates CE-35 through CE-42 without forcing unrelated research authority. Power the exact active-edge set, candidate binding, polarity, and a missing-prototype-prerequisite mutant. |
| QG-06 | P1, new | Conditional predicate truth has no explicit profile-instance context. Evaluating a program-active successor profile activates CE-07 on immutable launch `GENESIS-D1@genesis-bytes` and CE-23 on historical `GENESIS-ROOT-PARITY-1@parity`. | A successor that adds a feature can be forced to rewrite launch genesis/parity, or the graph can incorrectly claim the launch artifacts already contained successor-only state. This contradicts the retained-genesis migration model. | Add a structured predicate-evaluation context/role. Scope CE-06–CE-09 and CE-22–CE-24 to the initial launch profile and target-dependent edges to the target candidate. Power a base-only launch followed by a program-active successor: migration/new-root evidence activates without changing launch closure. |
| QG-07 | P1, new | The paper now requires an authorized post-ID launch closure, but the capability tree names no owner for producing/admitting it: `PROTOCOL-ID-1` lists the pre-ID genesis and generic identifier graph, `TESTNET-1` lists only version-pinned genesis, and `L1-CANDIDATE` still lists a genesis package. | The checked capability DAG can schedule testnet and candidate completion with every declared dependency satisfied while never producing or verifying the launch object that TG-01 made mandatory. A paper-only requirement is not a graph gate. | Add a `CHAIN-LAUNCH-1` node or equivalent structured owner depending on the exact genesis/profile/specification/admission/parity/release/authority prerequisites; make `TESTNET-1` and downstream artifacts consume its authorized closure, and power omission/wrong-reference gates. |
| QG-08 | P2, new | README calls `LAYER1_DESIGN_AUDIT.md` the paper's “latest gap review,” but that audit is pinned to checkpoint-0.3 and explicitly becomes stale after paper commit `034b7b2`; three later successor audits inspected and changed the paper. | A successor can treat the historical F-01–F-52 disposition as current closure and skip the actual successor audit chronology. | Relabel it the pinned checkpoint-0.3 audit and point current review chronology to the immutable successor audits. Update the handoff/read order without rewriting historical audits. |

## Candidates rejected or classified after challenge

- `ChainLaunchAuthorizationV1` may carry a policy-specific threshold proof or certificate rather
  than one universal signature document. The selected `G_0` policy must eventually freeze its
  scheme, credentials, statement, codec, and verification, but forcing one signature shape here did
  not produce a distinct ambiguity beyond the still-locked codec/cryptographic implementation.
- `post_history_root` need not be a second full block identifier: `block_id` already commits the
  proposal context and the documents make the two types distinct.
- Multiple valid availability-evidence references can remain node-relative while the exact
  policy-bound predicate/status remains fixed; no canonical single-reference claim was found.
- A standalone authenticated root is not authority. Its network consumer already verifies an
  accepted header context before local proof verification, so omitting launch identity from the
  context-free map root did not by itself enable cross-launch authority.
- Concrete cryptographic suites, numerical limits, persistence engines, archive guarantees,
  Verkle implementation, asset meaning, and Rust refinement remain visibly locked implementation
  or decision work. They are not silently counted as completed design evidence.

## Areas checked with no additional material gap

- QG-01 is distinct from TG-01: the post-ID manifest now binds one launch; the remaining failure is
  missing availability of the bound preimages.
- The ordinary/migration envelope owner, dedicated migration-body cardinality, proposal/evidence
  selector resolution, and bounds-before-work ordering added after Loop3 otherwise remain intact.
- The exact 17-document authority inventory, six frozen-file LF policies/blobs, contained relative
  links, powered checker path, CI command descriptions, and remote correction pin matched the
  audited input.
- MPT/SMT versus Verkle separation, logical-state versus availability, local tick versus consensus
  time, and proof/security non-claims did not broaden.

## One-fix-sweep rule

The correction may change only the protocol/planning/checker/handoff surfaces needed to close
QG-01 through QG-08, connect this audit to the frozen inventory/read order, and power the new
structured controls. It may not activate a capability, implement a network or cryptographic
primitive, edit any previously frozen audit or this file, or silently add a ninth finding. Any
later discovery belongs to another pinned loop.

The correction remains documentation, checker, attribute-policy, and handoff work. Narrow checker
execution and whitespace checks are correction gates. Broad runtime/proof tests begin only after a
complete later read-only sweep finds neither a new material gap nor a falsely closed material
residual under its declared finite lenses.

## Residual boundary

Closing these findings cannot prove that the protocol is secure, that launch/profile artifacts are
available in deployment, that a candidate profile is satisfiable, that cryptographic schemes are
sound, that the implementation respects the proposed byte graph, that CI infrastructure is honest,
or that Rust refines the planning documents. The next complete sweep must pin the correction and
may declare diminishing returns only within its explicit finite coverage.
