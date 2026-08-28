# Tenth bounded successor audit

**Final complete read-only gap sweep under the user-set Loop 10 cap — 2026-08-27**

This audit freezes exact tracked input
`c6474e97a65fccee6c02ff07ad7a46bb516b7f9b`, Git tree
`29c72d3a26a72213e399b8846f8b6c24cdaeeed8`, on
`codex/bitblock-progress-2026-08-26`. The remote branch matched that commit before this record was
written, and the tracked worktree had no staged or unstaged difference. The commit/tree above, not
ambient files or later prose, is the audited input.

The principal inspected blobs were:

```text
02c60a636a8bac14d20993b1da3f3818d826ca9d  docs/LAYER1_DESIGN_PAPER.md
120e50bc8add7bd893ea8a58ff84acc9daac0a2f  docs/AUTHENTICATED_STATE_PLAN.md
fecfebf7401b01d09ec8d624eae1fe895cb71e64  docs/CAPABILITY_TREE.md
0501d8fdfa0d992802fc0fbd86a8fe53ad42e5c3  docs/DESIGN_RESEARCH_DISTILLATION.md
f7dbe7ee30bc3f2b9eafbcd8037ff39162cca456  docs/HANDOFF.md
9aeec0b750a30ece8a0b1df4b75c2f9209e27a2d  docs/TRANSCRIPT_AUDIT.md
fd1396062fdc6b9371f7ec4c8ab33cb71c01d851  docs/PROJECT_INTENT.md
8ff741967136ad4efd67e2f0df5e539059d7926d  tools/check_planning_docs.py
1614078e92cf5664fb16d433c574ffdf3f34029f  .gitattributes
59ec05ad38cd6a5fd2efe4ff46cd39f31c59e39e  README.md
df441fea73fd82c9709f14fb442a466fa13d314a  .github/workflows/ci.yml
```

Independent read-only lenses attacked protocol/type/authority closure, checker/graph/provenance
closure, and cross-document truth/convergence. The protocol lens constructed an authenticated
later-start occurrence suffix that omits an earlier conflict, two signing histories with identical
release/activation artifacts, and a valid migration context that violates the paper's universal
authorization-ID exclusion. The checker lens mutated quoted domain bytes, appended an authority
variant after a blank line, collapsed a transitive envelope hash domain, wrapped or negated semantic
projections, injected a Git operational failure, and analyzed the merge-topology residual. The truth lens compared every
mutable planning authority's implementation, chronology, convergence, and review-provenance claims.
The passes reconciled into twelve unique material gaps or falsely closed residuals. No audited input
was edited during discovery.

The narrow planning checker and clean-input tracked-diff check were the only repository checks run
before this audit record was created:

```text
python -B tools/check_planning_docs.py --self-test
planning-doc check PASS: 72 capability nodes, 95 phase-status tuples,
363 declared base-plus-phase-plus-conditional edges, 10 predicate entailments,
23 Markdown files and 1 frozen source receipt with negative controls

git diff --check
PASS (exit 0, no tracked difference and no output; this preceded the untracked audit file)
```

No Rust, Lean, broad Python-vector, headless, release, network, storage-fault,
cryptographic-implementation, performance, or physical-display test was run. The nonzero result
does not satisfy the frozen zero-gap/zero-false-closure gate, so those tests remain locked.

This record preserves findings and break witnesses, not verbatim agent transcripts or agent
identity. It must be committed before any later correction and must remain immutable afterward.

## Frozen finding set

| ID | Priority | Gap at `c6474e9` | Concrete loss mechanism | Candidate correction |
| --- | --- | --- | --- | --- |
| TG-01 | P1, occurrence-range lower-bound authority | `ConsensusNativeAcceptedRecordOccurrenceUniverseScopeV1` carries raw `consensus_history_start_position_inclusive`, but no resolver result, cutoff field, profile projection, or exact equality binds its value. The field occurs nowhere else in the audited tree. | A candidate chooses a later start and proves a valid gap-free/lower/upper suffix under the authentic root and cutoff. An earlier same-lookup/core conflict wrapper falls outside the candidate-selected scope, so scope-relative completeness passes while live conflict completeness fails. | Define one profile/resolver-derived history-start value and policy; bind it into the cutoff/scope; require byte equality before range verification; power a later-start plus internally valid suffix-proof rejection. Replace or define the otherwise lone `RequiredParentAuthorityCertificatePositionV1` reference with an exact semantic type/equality. |
| TG-02 | P2, falsely closed release/activation signing chronology | Release and activation sign documents explicitly have no consensus position, yet the paper calls certificate-before-sign/derive enforceable and promises rejection when a document was signed before the complete certificate existed. | A signer precomputes and authorizes the exact future certificate-dependent bytes just before ledger admission; the byte-identical certificate is then admitted. This history is indistinguishable from admission followed by signing. Staged verification proves use order, not historical signing time. | Narrow the invariant and negative control to acceptance/verification/use dependency, or add an authenticated authorization position/challenge/nonce plus a strict predecessor proof if historical signing chronology is required. |
| TG-03 | P2, consumer-context exclusion contradiction | `EnvelopeChildParentAuthorityConsumerContextV1` embeds `child_profile_transition_context`; a valid migration transition embeds `activation_authorization_id`; the paper nevertheless says the consumer context excludes every authorization ID and “their authorizations.” | An honest migration consumer necessarily carries the already completed activation authorization transitively, so the exact schema and universal exclusion cannot both hold. This is a scope contradiction, not a dependency cycle. | Scope the exclusion to the current certificate/consumer derivation chain and explicitly permit the prior activation authorization in migration context, or redesign the context; power a transitive allowed/forbidden-field check. |
| TG-04 | P1, quoted bytes erased by exact-assignment normalization | `_parse_exact_schema_assignment` removes all whitespace from arguments, including whitespace inside quoted domain literals. | Changing guarded `Domain("bitblock/replay-evidence-event/v1")` to byte-distinct `Domain("bitblock/replay-evidence-event /v1")` yields the same parsed tuple and passes every current paper/exact/invariant/projection/Markdown guard. | Make normalization quote-aware and preserve literal bytes exactly; power embedded space, tab, and newline mutations for every quoted domain assignment. |
| TG-05 | P1, exact-assignment blank-line boundary bypass | Exact assignment parsing stops at the first blank line. | A post-blank continuation `| UnauthenticatedParentAuthorityV1` adjacent to guarded `AcceptedParentAuthorityUseV1` remains outside the parsed assignment and passes the current paper/exact/Markdown guards. | Parse through the next top-level assignment or closing fence, reject orphan continuation/field/hash lines, and power post-blank variant, field, and hash-tail controls. |
| TG-06 | P1, transitive envelope primitives outside exact closure | The exact manifest guards certificate/replay call sites but not `EnvelopeOwnerProfileContextV1`, `EnvelopeResolutionV1`, `EnvelopeHashV1`, or `EnvelopeCommitOrderedV1`. | Replacing `Domain(object_domain)` in `EnvelopeHashV1` with one constant collapsed domain passes all current guards while every guarded certificate ID call site remains textually exact. | Exact-guard the four helper assignments and require transitive closure over defined helpers, with explicit exemptions only for declared abstract primitives. Power owner old/new, resolver, object-domain/profile-context/payload framing, and ordered-sequence mutations. |
| TG-07 | P1, wrapped and zero-negated semantic projections | Semantic provenance/count patterns do not normalize intra-paragraph line wrapping, and the count vocabulary omits `zero`, `no`, and `none`. | Claims such as `Loop 8` followed by a newline and a wrong addition commit, `capability` followed by a newline and `node count is 73`, `Loop 9 found no material gaps`, or `the combined graph has no edges` contradict canonical projections while the sole-projection guard passes. | Normalize intra-paragraph whitespace before semantic matching; recognize explicit zero/no/none negations; power wrapped forward/reverse claims and negations across every mutable authority. |
| TG-08 | P1, first-addition operational fail-open; P2, merge topology residual | The first-addition check treats every nonzero `git cat-file -e parent:path` result as proof of absence and inspects only the first parent. | An injected Git operational exit 129 is accepted as absence. A merge can name the audited input as first parent while a second parent already contains the audit, falsely satisfying “first addition.” | Use a checked tree query that distinguishes absence from command failure; require exactly one parent equal to the audited input or prove absence on every parent; power operational-error and multi-parent fixtures. |
| TG-09 | P1, proposed commitment node described as implemented | The authenticated-state plan correctly says the current source has no authenticated trie and the capability tree marks `COMMIT-R1@base` only `READY`, but the same plan says “`COMMIT-R1` implements and compares only `WorldRootV1`.” | A later agent can treat an unstarted research comparison and proposed root as implemented input, collapsing `READY` into implementation. | Use future/scope language such as “will implement and compare” or “is scoped to implementing and comparing,” while retaining every other domain's locked boundary. |
| TG-10 | P1, convergence gate contradiction | One transcript section requires a complete zero-new-gap sweep before broad tests, while another says broad tests begin after the next sweep whether it finds another material gap or records zero convergence. | The nonzero branch explicitly licenses the test phase that the current README, handoff, and later transcript keep gated. | State that a nonzero sweep is frozen and corrected while broad tests remain gated; only a frozen zero-new-gap and zero-false-closure result opens them. |
| TG-11 | P2, stale current chronology header | The transcript header says documentation chronology is checked only through frozen Loop 8, while later sections and every current authority record the Loop 9 correction at the audited input. | A reader following the header can ignore the current review chain or correctly conclude that the document has declared itself stale. | Advance the header through frozen Loop 9 and the current Loop 9 descendant without implying any broad rerun. |
| TG-12 | P2, current-paper review provenance false closure | `PROJECT_INTENT.md` says `LAYER1_DESIGN_AUDIT.md` owns the pinned audit of the current paper. That frozen audit identifies itself as checkpoint 0.3 and explicitly becomes stale after paper changes; current authorities classify it as a historical corrected reading. | A standalone ownership reader can treat a historical checkpoint audit as review of the current descendant and skip the successor audit chain. | Scope the ownership row to historical checkpoint 0.3 and name the frozen successor audits as the current descendant's review chronology. |

## Candidates rejected or classified after challenge

- The current compatible inherited-base/phase/conditional graph is acyclic. No present dependency
  cycle was found.
- Current Loop 7/8/9 provenance values, contiguous finding cardinalities, ancestry, inventory, read
  order, global summary-field uniqueness, EOL policy, and frozen blobs reproduced. TG-08 is a
  fail-open guard and merge-topology residual, not current provenance drift; the current audit
  additions are single-parent ancestors of `HEAD`.
- The current exact manifest covers the direct live-certificate/replay assignments added through
  Loop 9. TG-04 through TG-06 concern parser equivalence, assignment boundaries, and transitive
  helpers outside that direct manifest.
- Staged successor release-to-bundle-to-activation dependencies remain acyclic and purpose-separated.
  TG-02 concerns historical signing time, not verifier execution order.
- Live consensus-native authority and replay custody remain separated. No additional material gap
  was found in receipt totality above the unbound lower boundary, certificate/admission reconstruction,
  the replay bridge, or compromise safe-halt handling.
- Concrete consensus, signature, commitment, and evidence codecs; authenticated-root construction;
  numeric retention limits; deployment availability; operator custody; and future recovery remain
  explicitly `OPEN` or `LOCKED`. They were not counted as falsely closed.

## Areas checked with no additional material gap

- Cutoff/root/receipt totality above the lower-bound issue and exact replay reconstruction of already
  consensus-positioned occurrences.
- Successor release/activation purpose separation, internal dependency DAG, replay/live-authority
  boundary, and current compromise disposition.
- Combined dependency union, current audit cardinality/provenance values, current single-parent
  addition ancestry, canonical planning-summary projections, mutable-authority inventory, path-only
  read order, LF policy, and frozen artifacts.
- Implementation-versus-proposal, chronology, convergence/test-gate, work-order, and review-
  provenance claims across the mutable planning authorities.

## Stop boundary

The user explicitly capped this process at Loop 10 after finishing and pushing the Loop 9
correction. This nonzero audit is therefore the terminal loop for the current task. No Loop 11 is
started, and no TG-01 through TG-12 correction is applied here. A later separately authorized task
may use the candidate corrections above, but it must preserve this file as the immutable input-side
record before changing any audited surface.

Because this complete sweep found material gaps and falsely closed residuals, diminishing returns
was not reached. Broad Rust, Python-vector, headless, release, Lean, network, storage-fault,
cryptographic-implementation, performance, and physical-display tests remain gated and were not run.

## Residual boundary

Even a future correction of these findings could not prove protocol security, cryptographic
soundness, consensus safety/liveness, candidate satisfiability, implementation conformance, honest
CI, complete archival evidence, remote custody, or Rust/Lean refinement. It would only make the
named lower-bound, chronology, context, parser, helper-closure, projection, provenance, and document-
truth claims explicit enough for a later bounded attack.
