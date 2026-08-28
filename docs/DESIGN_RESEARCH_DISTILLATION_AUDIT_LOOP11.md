# Eleventh bounded successor audit

**Complete read-only successor sweep after the Loop 10 correction — 2026-08-27**

This audit freezes exact tracked input
`3d7164ce3e6a81eec724aff18e0eeaf133e48a10`, Git tree
`b71509263f77d31e00b735baaa83f71b3d931846`, on
`codex/bitblock-progress-2026-08-26`. The remote branch matched that commit before this record was
written, and the tracked worktree had no staged or unstaged difference. The commit/tree above, not
ambient files, another checkout, or later prose, is the audited authority.

The principal inspected blobs were:

```text
5b1a4f05c2e305a3b6d98f5d577f1e4147f6f821  docs/LAYER1_DESIGN_PAPER.md
edb2c976b073abcb216a1ab04c14149910c83c5a  docs/AUTHENTICATED_STATE_PLAN.md
f3473bf354e22955cedd467a59a7c0ee39bc2ff8  docs/CAPABILITY_TREE.md
c4c5ac0b4615c737ef28b9f938c6ccccccc808b4  docs/DESIGN_RESEARCH_DISTILLATION.md
de27ce9c7274e3e2d7d8e9f1a8976cd36531e8e3  docs/HANDOFF.md
38a1ba45c33cbccc2ed616d652398863c6c8878b  docs/TRANSCRIPT_AUDIT.md
d5fa53207a61f276ba515053808d5587663d3b0b  docs/PROJECT_INTENT.md
034d70e9373277690315fe09cb8950193f7bba5e  docs/PROTOCOL.md
995f037f67217f1e92cb926a3cfb094d3816868d  docs/TIMELINES.md
166e1a5a4817a5fcf55e711ed6352ffef56b4669  README.md
7fe5d0b1e7e534ebebbb43dc3c18cbd72c81d0a2  tools/check_planning_docs.py
4b9bee50f5d3d156d751cd883359787883305987  tools/mirror_timeline1.py
df441fea73fd82c9709f14fb442a466fa13d314a  .github/workflows/ci.yml
cc1b70350a6e4c13117c8ad6b59aa97e6e6ddd51  .gitattributes
ad9a3ca03b7004e9229f174478e19f06feeb4fc4  src/tick.rs
```

Independent read-only lenses covered protocol/type/authority closure, checker/provenance/data-
integrity closure, and cross-document truth/coordination/convergence. The protocol lens replayed a
late but mutually equal history-start rule, compared the corrected paper with its derivative plans,
followed the occurrence-universe owner across a profile transition, and challenged the pending
`ACCOUNT-1` promotion gate. The checker lens used process-local mutations for lexical token splits,
Markdown comments and inline presentation, unsupported count/object syntax, and staged/index
visibility. The truth lens compared current paper identity, workflow execution, verification
inventories, finding namespaces, checkout topology, and the stated work order. Fourteen unique
material gaps survived hold frames; overlapping ACCOUNT/coordination observations were consolidated
into one finding. No audited input was edited during discovery.

Here, unique means deduplicated within Loop 11, not wholly novel across every prior loop. Relative to
Loop 10, UG-09 is a false-closure residual of Loop10/TG-10; UG-10 is an additional current-tense
surface in Loop10/TG-12's provenance class; and UG-03 plus UG-11 through UG-14 are distinct
mechanisms not reported in Loop 10. The remaining IDs are retained as Loop 11 witnesses without a
universal novelty claim against every earlier audit.

The input's narrow planning checks were replayed immediately before this audit file was created:

```text
python -B tools/check_planning_docs.py
planning-doc check PASS: 72 capability nodes, 95 phase-status tuples,
363 declared base-plus-phase-plus-conditional edges, 10 predicate entailments,
24 Markdown files and 1 frozen source receipt

python -B tools/check_planning_docs.py --self-test
planning-doc check PASS: 72 capability nodes, 95 phase-status tuples,
363 declared base-plus-phase-plus-conditional edges, 10 predicate entailments,
24 Markdown files and 1 frozen source receipt with negative controls

git diff --check
PASS (exit 0, no tracked difference and no output; this preceded the untracked audit file)
```

Those passes are evidence about the current checker, not a rebuttal to the powered counterexamples
below. No broad suite was started locally during this sweep. However, the repository workflow runs
on every push: GitHub Actions run `33132645791` executed Rust, standalone Python, scenario/registry,
and Lean jobs successfully for the nonzero Loop 10 audit commit `1f7ec0f`, and run `33139744411`
did the same for this exact audited input. Those existing remote runs are execution facts, not an
attributed convergence/release pass, and they do not cover all-feature, network, storage-fault,
cryptographic-implementation, performance, or physical-display claims.

One external runtime candidate was inspected read-only only to challenge the future `ACCOUNT-1`
gate. Remote `main` and the clean `C:/Users/Owen/bitblock-t2` checkout both identified
`5937056ef59d4659d68c6da0ab75b19469d2773d`; run `33137682969` was green for that SHA. The root audit
also reproduced its Python decoder accepting action lanes `(4294967295, 1)` with total
`4294967296`. That read-only observation does not make the candidate part of the audited tree or
merged here. Its promotion disposition remains the peer audit's independently reported HOLD, not a
phase status in this branch.

The root audit reproduced the decoder result from the clean external checkout with:

```text
cd C:/Users/Owen/bitblock-t2
python -B -c "import runpy; m=runpy.run_path(r'tools/tick_outcome_vectors.py'); p=bytearray(m['encode'](m['genesis_idle']())); p[147:151]=(0xffffffff).to_bytes(4,'big'); p[151:155]=(1).to_bytes(4,'big'); d=m['decode'](bytes(p)); print(d['lanes'][:2],sum(d['lanes']))"
(4294967295, 1) 4294967296
```

The separate schema checkpoint `6450007` and its 58/58 witness report are peer-reported coordination
facts from another Git root; this sweep did not inspect that checkout and grants it no authority.

This record preserves findings and break witnesses, not verbatim agent transcripts or agent
identity. It must be committed before any correction and remain immutable afterward.

## Frozen finding set

| ID | Priority | Gap at `3d7164c` | Concrete loss mechanism | Candidate correction |
| --- | --- | --- | --- | --- |
| UG-01 | P1, history-start correction propagation | The paper now derives `ConsensusNativeAcceptedRecordOccurrenceHistoryStartV1` from the admitted parent profile and requires resolver/cutoff/scope byte equality. `AUTHENTICATED_STATE_PLAN.md` lines 1009–1034 and the notes-only `DESIGN_RESEARCH_DISTILLATION.md` mirror at lines 376–405 still describe a resolver without that result, a cutoff proof without the start, and a raw inclusive scope start; the `CONSENSUS-1` gate does not name equality or the later-start control. | A reader relying on the stale authenticated-state plan body—or the explicitly non-authoritative distillation mirror—can choose start 8, prove a genuinely complete range `[8,9]`, and omit an authentic conflict at position 2 even though the normative paper would reject it. The stale prose still says the range cannot be candidate-selected. | Propagate the exact history-start type, admitted resolver rule, cutoff/scope equality result, authenticated proof binding, and late-valid-suffix rejection into the authenticated-state plan projection, notes-only distillation mirror, and owning `CONSENSUS-1` gate. Add a checker guard for the complete projection rather than a summary claim that the correction was incorporated. |
| UG-02 | P1, admitted lower-bound safety | Even in the paper, the corrected equality proves only that resolver, cutoff, and scope carry the same `history_start_rule_id` and position. No admission invariant requires that immutable rule to start at profile activation, the earliest occurrence that could affect a certificate, or an authenticated lossless predecessor closure. | Within the proposal, a candidate can satisfy every currently stated profile-admission condition while fixing its rule at start 8 although relevant conflict evidence exists at 2. All three values equal 8 and the range proof is complete. No loss occurs if admission instead proves that no relevant occurrence can precede activation or authenticates a lossless predecessor closure; neither condition is explicit now. | Define the safe lower-bound invariant and its admission evidence: start at the earliest relevant occurrence for the profile, or bind an exact migrated/accumulated predecessor closure that proves lossless equivalence. Power a negative in which the admitted rule itself advances the start; equality alone must not save it. |
| UG-03 | P1, `ACCOUNT-1` promotion and coordination closure | This tree truthfully leaves `PS-009:ACCOUNT-1@base=READY`, but its gate says only that counts/effects reconcile and malformed/over-limit records reject. `HANDOFF.md` tells a successor to begin the node without naming the parallel runtime/schema lanes or the external candidate's reported HOLD obligations: overflow, decoder limits, duplicated opcode/lane mapping, and an asymmetric oracle. | Same-code or symmetric `(1,1,1)` controls can survive a permutation of lanes 7/8/9. The root audit reproduced the external candidate's unchecked-total/Python-decoder over-limit case; the remaining obligation set and HOLD disposition are peer-audit evidence. A successor could duplicate work or promote that candidate while believing generic reconciliation satisfied the gate. | Keep this tree's authoritative phase status READY. Record the external candidate's promotion disposition as peer-reported HOLD until independently closed; add a clearly non-authoritative coordination ledger with exact repository roots/SHAs and no merge or ancestry implication. Extend the gate with overflow-safe totals; independent per-lane/total rejection; structural `canonical_code`/lane binding; v3/469 truth; and a Rust/cross-language KAT for `(ToggleSidebar, Resize, ClearMotion)=(2,1,1)` pinned to `#bb.tick-outcome.sha256.ckeafgihrlyc57iiyef4mjvkqrxsyfbiucrrb4zndfvp57h45xhq`, watched red under a subject lane permutation and then restored. |
| UG-04 | P1, profile-transition authority owner | Occurrence receipt IDs, ordered-map roots, index policy, and history-start policy are parent-profile-owned. A migration envelope stays old-profile-owned through acceptance, which activates the new profile, but no exact rule says whether the first new-profile child continues the old universe, re-encodes it under the new profile, imports a bridge proof, or uses a bounded dual-profile handoff. The generic `UPGRADE-1` old/new execution-root language does not name this consensus-native universe. | Migration block `M` receives `r_old/R_old`; after activation the first new-profile child needs authority over `M`. Old continuation retains old authority after activation, while a new root has no specified receipt for `M`. Conforming implementations can reject, select different roots, or lose the evidence needed for the certificate. | Give the consensus-native universe an explicit typed state/root owner and one transition relation. Bind old/new policy, start, root, receipt re-encoding or preservation, bridge/equivalence proof, activation ordering, and first-child certificate behavior into `UPGRADE-1`, the successor bundle, and powered cross-profile controls. |
| UG-05 | P1, exact-schema lexical fusion | `_compact_schema_whitespace` deletes every unquoted whitespace character before validating lexical tokens. `EnvelopeOwnerProfileContextV1(profile_transition_context)` and malformed `EnvelopeOwnerProfileContextV1(profile_ transition_context)` therefore parse identically and pass the exact-assignment/helper/invariant/Markdown checks. A bounded split matrix admitted 536 of 636 one-space identifier splits as equal. | A malformed or byte-distinct schema token can inherit the expected digest and transitive-helper result even though no conforming lexer would treat it as the same identifier. | Lex unquoted schema syntax first. Permit layout whitespace only between complete valid tokens, preserve quoted bytes, reject whitespace inside identifiers/numeric literals/operators, and power token-split matrices across every guarded assignment. |
| UG-06 | P1, Markdown authority-context blindness | Structured parsers consume raw rows inside HTML comments, while semantic scans do not normalize ordinary inline Markdown presentation. Commenting out the complete 95-row phase-status table or the `Loop10ProvenanceV1` table still passes; visible `Loop **10** found 13 material findings.` and related emphasis/entity forms also pass. | The checker can report a rendered authority that does not exist, or ignore a rendered false claim that a reader sees. Direct deletion/unformatted corruption still rejects, so the powered self-test misses the rendering context. | Parse Markdown once into explicit contexts. Authority rows must be visible, outside comments/code, and in the one named table; semantic prose should normalize inline presentation/entities to rendered text before finite claim parsing. Power hidden-table, formatted-marker, and formatted-count controls. |
| UG-07 | P1, unsupported semantic values fail open | The semantic scanner recognizes ASCII decimal counts, but spelled-out count words only through twenty-two, and Git-object strings only at lengths 7–40. A recognized claim head followed by `a baker's dozen`, `thirty`, full-width digits, a six-hex abbreviation, or a 41-hex value is ignored rather than rejected. | `Loop 10 found a baker's dozen material findings.` and `Loop 10 addition commit 1f7ec0` coexist with the canonical projection while the checker passes. The stated finite fail-closed grammar is therefore fail-open exactly when the value syntax is unsupported. | Recognize the claim head/field first. Require its value to parse under one finite canonical grammar or reject the clause; never use value recognition as the condition for noticing a claim. Power below/above-length object values, unsupported number words, Unicode digits, and malformed separators. |
| UG-08 | P1 local admission, frozen receipt/index split | `design-source-receipt-v1.json` is frozen by worktree bytes but is absent from the planning-path/read-order index requirement. A staged deletion can be masked locally by an exact untracked worktree copy if the two mutable Markdown links are changed to non-links; frozen-byte, read-order, Markdown, and staged whitespace checks can all remain green. | A local commit gate can accept deletion of the sole source receipt even though a later clean CI checkout will fail. This is not a durable CI bypass, but it is a false-green at the admission point where an agent decides to commit. | Require every frozen artifact to exist in the Git index and bind the checked bytes to the staged/index object. Refuse staged/worktree divergence for frozen inputs and power staged-delete-plus-untracked-copy controls. |
| UG-09 | P1, convergence execution/attribution contradiction | README, handoff, and transcript say broad tests begin only after a frozen zero-gap sweep, but `.github/workflows/ci.yml` runs default Rust, standalone Python/scenario/registry, and Lean on every push. Both the nonzero audit commit and its correction already ran those jobs successfully. | The workflow executes the supposedly gated suites before convergence; later prose can either deny real execution or accidentally treat routine CI as the special convergence/release pass. | State the enforceable boundary: automatic CI always runs and is recorded, while attribution of a complete manual broad/release evidence pass remains gated on frozen convergence. Name the additional suites absent from routine CI and pin every attributed receipt to commit/run/commands. |
| UG-10 | P1, current-paper identity contradiction | `TRANSCRIPT_AUDIT.md` labels as current truth that `LAYER1_DESIGN_PAPER.md` checkpoint 0.3 defines the inventory, while the paper explicitly says the mutable file is not checkpoint 0.3 and pins the historical artifact at `034b7b2`. The transcript later acknowledges that distinction. | Two current authorities answer whether the mutable paper is checkpoint 0.3 oppositely, allowing a reader to apply a historical audit to the current descendant. | Change the earlier transcript statement to historical wording or say that the current descendant retains the named shape. Preserve the pinned checkpoint/audit and the later successor-review boundary. |
| UG-11 | P1, cross-checkout write target and stale topology | `TIMELINES.md` describes mirroring into this repository, but `tools/mirror_timeline1.py` hard-codes `REPO=C:/Users/Owen/bitblock-t2` while the audited root is `C:/Users/Owen/bitblock_loop6`; running it here writes and commits in another shared checkout. Current handoff/timeline/tool prose also calls `C:/Users/Owen/bitblock` unversioned although coordination now reports it as a separate unrelated Git root. | A successor following the documented command can modify another agent's checkout, copy the wrong source state, or infer ancestry/merge authority between unrelated roots. The tool's internal `git add -A` magnifies the ownership error. | Quarantine the stale command until it accepts explicit validated source/repository/worktree roots, requires the target root to match the invoking repository or an explicit opt-in, rejects unexpected Git metadata, and previews exact paths. Update current topology prose and add a non-ancestry/non-authority coordination record; retain historical transcript statements as historical. |
| UG-12 | P2, verification inventory drift | `PROTOCOL.md` calls 314 default tests and sixteen Python vector programs the final pass; an earlier nearby paragraph says 316. Exact-input CI run `33139744411` reports 318 library, 42 application, and one integration test (361 total) and runs 22 standalone vector scripts. None of the old counts names a commit/run/list that would make it historical. | A successor cannot reproduce the advertised final evidence inventory and may omit six live vector programs or dozens of tests while reporting parity with an unnamed old pass. | Replace floating current/final counts with a generated manifest or pin each historical receipt to commit, run, commands, feature set, and exact tool list. Keep passing status separate from count drift and independently inventory all-feature/manual-only suites. |
| UG-13 | P2, frozen finding-ID namespace collision | Frozen Loop 3 and Loop 10 both define `TG-01...`; mutable handoff prose refers to both sets as unqualified `TG-*`. `TG-04` therefore names two immutable findings with unrelated mechanisms. | A correction, checker fixture, or agent instruction can cite the wrong frozen obligation while remaining textually plausible. Full file-path context holds, but short operational references do not. | Never edit the frozen audits. Qualify every mutable reference as `Loop3/TG-04` or `Loop10/TG-04`, and add a guard that either enforces globally unique prefixes for future audits or requires loop-qualified IDs wherever a collision exists. |
| UG-14 | P2, reversed deferred subject | The authenticated-state plan front matter and R0 sequence defer Verkle while leaving the two hash-tree `COMMIT-R1` comparison open/READY. Open decision 13 instead says “The first hash-tree profile is already explicitly deferred.” | Literal reading blocks the READY baseline assay or reverses the intended work order, even though surrounding prose makes the likely intended subject clear. | Say that Verkle is deferred from the first hash-tree profile/comparison. Keep the first fixed-key Patricia versus sparse-Merkle assay open and preserve all selection gates. |

## Candidates rejected or classified after challenge

- Current Loop 7–10 commit/tree/blob/count/ancestry values, the Loop 10 frozen blob, and the checked
  remote ref reproduced. Git operational errors and multi-parent prior-path collisions still fail
  closed under the corrected first-addition procedure.
- Quoted space, tab, and newline mutations reject. UG-05 is specifically unquoted lexical fusion.
- The current combined dependency graph remains acyclic, and removing an existing path from the
  declared planning inventory rejects. UG-08 is the narrower worktree/index split for the frozen
  JSON receipt.
- Transaction suite selection, identity/cryptography, consensus mechanism choice, concrete
  authenticated-map construction, numeric network bounds, and asset/economic choices remain
  explicitly LOCKED or OPEN; they were not counted as falsely closed.
- The occurrence universe is intentionally consensus-native rather than part of the application
  execution state. That is not itself a gap. UG-04 is only the absent exact owner/handoff at a
  profile transition.
- Generic migration prose can eventually carry the UG-04 correction, but it does not presently name
  the universe, root, receipt transformation, or first-child rule.
- Full path context disambiguates the two `TG` namespaces; frozen files remain reproducible. UG-13
  concerns short mutable operational references, not corruption of either audit.
- Existing remote CI runs were green. UG-09 and UG-12 concern claim scope and reproducible evidence
  inventory, not a claim that those jobs failed.
- The external runtime candidate's defects do not make this audited v2 tree claim `ACCOUNT-1` is
  implemented. READY still means only eligible for a future checkpoint.

## Areas checked with no additional material gap

- Corrected paper-level history-start equality, release/activation verifier-use chronology,
  consumer-context prior-authorization exception, and current safe-halt boundary, apart from UG-02
  and the profile-transition handoff in UG-04.
- Current proposal/evidence ID separation, complete occurrence-range shape above the lower-bound
  rule, replay/live-authority direction, compatible role-edge union, and successor internal DAG.
- Frozen audit/source-receipt bytes, Loop 7–10 provenance values, current single-parent addition
  ancestry, planning-summary derivation, and current direct Git operational controls.
- Current implementation/proposal labels, Layer 1 non-activation boundary, and explicit OPEN/LOCKED
  crypto, consensus, networking, state, economic, and operations decisions.
- No drift was found between the frozen receipt, its projected statistics, and the pinned historical
  distillation. The external raw source remains unavailable and was not reauthenticated.

## Stop boundary

This nonzero audit is the immutable input-side record for Loop 11. No UG-01 through UG-14 correction
is applied here, and no Loop 12 audit starts before one coordinated descendant correction is itself
committed. Diminishing returns was not reached.

Routine CI has already executed broad subsets for the audited input, so “tests did not run” is not a
truthful gate. What remains closed is attribution of a complete convergence/release evidence pass,
plus every suite and authority claim not covered by routine CI. No automatic or local result proves
physical display, network safety/liveness, storage-fault recovery, cryptographic implementation,
remote custody, or Layer 1 security.

## Residual boundary

Even a future correction of all fourteen findings could not prove protocol security,
cryptographic soundness, consensus safety/liveness, candidate satisfiability, implementation
conformance, honest CI, complete archival evidence, remote custody, or Rust/Lean refinement. It
would only align the named history-start, profile handoff, ACCOUNT coordination, parser, Markdown,
index, convergence, provenance, topology, and evidence-inventory claims well enough for the next
bounded attack.
