# Second bounded successor audit

**One read-only gap sweep followed by one coordinated fix sweep — 2026-08-27**

This successor audit freezes exact clean input
`ea9294ba08b5e4bd339f82894ecb761461f5af13`, Git tree
`1ba4b7edcd3c0efa18139a3d00dc85449aa005f4`, on
`codex/bitblock-progress-2026-08-26`. The remote branch matched that commit before discovery began.

The principal inspected blobs were:

```text
0eca02051b41d136a3f7a12639e4211d14cec2b1  docs/LAYER1_DESIGN_PAPER.md
289d70f176f6a11caaa3414b021ae084f7410e64  docs/AUTHENTICATED_STATE_PLAN.md
e343cc69dda2f5119d7739733b3f3221a6eb10a5  docs/CAPABILITY_TREE.md
53dc9a94f985e9c795376e686d12a62648be3683  docs/HANDOFF.md
c4be9e0dc7041bfe79b3e8561308b6c2c4767ce6  docs/FUTURE_PLAN_AUDIT_LOOP2.md
c3005e7fc98971a7747ab125f1624d08aff31199  docs/design-source-receipt-v1.json
2640a832ad2b27beedd211b9f555a3f0753268f2  tools/check_planning_docs.py
3d5cf35c981e353e41875dc4a613a5e5a92a4785  .github/workflows/ci.yml
```

Three independent read-only lenses examined protocol/type closure, cryptographic/dependency
closure, and provenance/checker/handoff behavior. A fourth local pass reconciled their results,
inspected the actual CI workflow, and queried GitHub Actions for the continuation branch. At
`2026-08-27T10:48:52.0147708-07:00`, `gh run list` returned no runs for this branch. No audited file
was edited and no Rust, Lean, broad Python-vector, headless, network, storage-fault,
cryptographic-implementation, performance, or physical-display test was run before the finite set
below was closed.

This record preserves findings and break witnesses, not verbatim agent transcripts or agent
identity. It must be committed before the correction. The correction may pin this file's
first-addition Git blob but must not mutate this file.

## Frozen finding set and one-sweep correction map

| ID | Priority | Gap at `ea9294b` | Concrete loss mechanism | One-sweep correction |
| --- | --- | --- | --- | --- |
| SG-01 | P0 | `AUTHENTICATED_STATE_PLAN.md` freezes `ProfileIdHashV1` before candidate-selected cryptography, but the Layer 1 paper uses undefined bare `H` for `profile_scope_digest`, `genesis_id`, `profile_id`, and the profile-bound transition-specification ID even though the scope contains cryptographic-suite choices. | Two encoders can interpret `H` differently, or a candidate can select the function that identifies its own scope/profile, producing divergent or self-selected authority identifiers. | Make the already pre-frozen protocol identity hash the sole owner of every pre-admission identifier and use explicit domain/length framing. Candidate suites cannot alter it. |
| SG-02 | P0 | The paper and tree require an in-place old-profile to new-profile migration, but genesis is tied to one scope/registry, ordinary commitment-set succession requires identical non-counter context, and the header supplies one profile context for both pre- and post-roots. | A changed domain registry, codec, or commitment profile either implies a new genesis/chain, fails the ordinary successor rule, or leaves old and new roots untyped under one header profile. The declared migration transition is uninhabitable. | Separate immutable chain genesis from later profile activation, define an ordinary-versus-migration profile-transition context with old/new typed root references and authorization/specification identity, and add a migration-specific commitment-set edge while retaining strict same-context ordinary succession. |
| SG-03 | P1 | `ConsensusHead` carries availability status/evidence reference, and the prose permits separate attestations, but `AcceptedBlockRecord` contains only `Proposal` and `ConsensusEvidence`. | Nodes can attach incompatible status/reference pairs to identical accepted bytes. The payload commitment proves byte identity, not possession, sampling, recovery, or future retrieval. | Define detached, policy-bound availability assessment/evidence bytes that bind `block_id`, authority/epoch context, predicate/status, and evidence or a consensus-evidence entailment reference. Keep node-local observations explicitly non-authoritative. |
| SG-04 | P1 | A candidate scope may select a Verkle commitment-profile ID, but `PROFILE-SCOPE-D0` has no conditional dependency on `VERKLE-R1@feasibility`; the later prototype instead depends on the already-created candidate profile. | An unreviewed descriptor can enter profile/genesis authority, or strict descriptor resolution deadlocks between candidate creation and the prototype. | Add a `verkle-selected` conditional edge from candidate scope to the feasibility artifact that emits the exact self-excluding descriptor/ID. Retain the post-candidate prototype edges and declare predicate entailment. |
| SG-05 | P1 | Handoff and audit prose call `FUTURE_PLAN_AUDIT_LOOP2.md` frozen, but the checker guards only the first future-plan audit and the first design-distillation audit. | Changing LOOP2 from “16” to “999” findings still passes its available Markdown check, so a historical audit can drift under an advertised freeze. | Pin blob `c4be9e0dc7041bfe79b3e8561308b6c2c4767ce6` and add a powered mutation control. |
| SG-06 | P1 | The source receipt asserts historical commit/blob/SHA/count facts for the pre-audit distillation, but the checker only pins the current JSON semantics and compares the external-source fields with Markdown. | A false historical object hash or count passes the receipt-agreement path if its admission constant is changed with it; the current correct values are not mechanically derived from Git. | Resolve the recorded commit/path, recompute Git blob, SHA-256, bytes, UTF-16 code units, whitespace words, and lines under the declared method, and power wrong-commit/hash/count controls. Also distinguish byte-level from semantic receipt freezing. |
| SG-07 | P1 | The predecessor audit and handoff call `694650f..HEAD` and `git show HEAD` stable historical checks, while the correction commit `ea9294b` is not pinned in the handoff. | Later commits silently change which bytes the commands inspect and can misattribute unrelated success or failure to the DR-01–DR-15 correction. | Preserve the predecessor audit, but pin `ea9294ba08b5e4bd339f82894ecb761461f5af13` and the fully bounded range/object commands in successor/handoff surfaces. |
| SG-08 | P1 | Local-link checking accepts paths outside the repository and does not structurally inspect `SEMANTICS.md` or `PROTOCOL.md`, although the handoff prescribes both as authority documents. | A machine-local or untracked target can pass on one PC but fail in a clone, while broken links/fragments in current semantic/protocol authority remain green. | Require every local target to remain repository-contained and Git tracked/staged, add escape/untracked controls, and add the omitted authority documents plus the CI contract document to the structural inventory. |
| SG-09 | P1 | CI invokes the planning checker without `--self-test`, and push events cover only `main` and `timeline-2`; GitHub reported no run for the continuation branch. | Powered audit/receipt/fragment controls can die while automated CI stays green, and the branch presented for handoff has no remote run at all. | Run the powered planning checker explicitly, make branch-push coverage match the documented “every push” claim, and fetch enough Git history for the historical receipt check. |
| SG-10 | P1 | The generic Python CI loop invokes every `tools/*.py` except two helpers, but `simulate_sequence.py` requires a positional scenario. | Any pull request or covered push containing the current tools fails at argument parsing before the advertised vector suite completes; the continuation branch hid this because SG-09 prevented a run. | Exclude the scenario runner from bare invocation and add one explicit, expectation-checked example invocation. |
| SG-11 | P2 | The sole proposed byte-contract owner's proposal-validity paragraph contains the malformed sequence “roots authorized by the parent accepted under the selected consensus rule; parent; complete...”. | Parent acceptance, parent authorization, and byte-availability requirements are grammatically ambiguous in the central acceptance checklist. | Rewrite the paragraph as an explicit ordered checklist without changing the intended authority boundary. |

## Areas checked with no additional material gap

- DR-01 through DR-15 remain closed in the corrected distillation. Typed-domain recomposition,
  first-touch/overlay rules, external-input non-claims, and old/new commitment correspondence did
  not regress; SG-02 concerns representability in the concrete profile/header/root types rather
  than re-opening DR-04's witness finding.
- MPT/SMT canonicalization, Verkle scheme/setup separation, generator-relation wording, proof-size
  qualification, state/history/snapshot separation, and information-retention limits remained
  coherent.
- The pinned EIP bytes/statuses reproduced at their recorded revision. Portal and mutable live-page
  claims remain scoped as research inputs rather than Bitblock authority.
- Current Rust behavior and its narrow non-claims did not broaden in the audited documentation.
  Existing runtime, custody, networking, authenticated-map, consensus, asset, and Layer 1
  obligations remain implementation work, not newly discovered prose contradictions.
- The raw external `design.txt` remained outside Git and matched its recorded SHA-256 during the
  prior correction verification. This audit did not pretend that a digest can recover or
  authenticate those bytes.

## One-fix-sweep rule

The correction may change only the protocol/planning/checker/CI/handoff surfaces needed to close
SG-01 through SG-11, connect this audit to the read order, and power the new controls. It may not
activate a capability, implement a network or cryptographic primitive, edit any previously frozen
audit, or silently add a twelfth finding. Any later discovery belongs to another pinned loop.

The correction is a documentation, checker, and CI-definition change. Narrow checker execution and
whitespace checks are correction gates; broad runtime/proof tests begin only after a complete later
read-only sweep reaches the stated diminishing-return condition.

## Residual boundary

Closing these findings will make the intended types and test contracts explicit. It cannot prove
that the protocol is secure, that the proposed migration is implementable within an acceptable
budget, that availability persists, that cited cryptography is sound, that CI infrastructure is
honest, or that Rust refines the planning documents. A later sweep must pin the correction and may
declare diminishing returns only if it finds no new material contradiction.
