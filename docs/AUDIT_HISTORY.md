# Audit and correction history

This text was the second half of the README's introduction until 2026-11-03. It was moved
here unchanged so the README opens with what Bitblock is and how to run it. Relative links
were adjusted for the new location; no wording was changed. It records the planning-audit
chronology for [`LAYER1_DESIGN_PAPER.md`](LAYER1_DESIGN_PAPER.md) and the
design-research distillation. None of it is runtime, cryptographic or security evidence.

References to `design.txt` and to local checkout paths are explained in
[`UNPUBLISHED_SOURCES.md`](UNPUBLISHED_SOURCES.md).

At the ninth audit's frozen input, that mutable paper still presented itself as historical
checkpoint 0.3 even though its contents had accumulated the later successor corrections. The Loop 9
descendant now preserves the pinned checkpoint and its audit as historical, labels the mutable paper
as the current proposed descendant, links the complete successor-review chain, and scopes its
checkpoint-obligations table to the historical artifact. This is a provenance repair, not a protocol,
runtime, or security activation.

The paper's historical checkpoint-0.3 gap review has a canonical corrected reading copy in
[`docs/LAYER1_DESIGN_AUDIT.md`](LAYER1_DESIGN_AUDIT.md). Its current Git blob
`124e066b418a61279abf90c1b1b2c2cc0604ef2a` is frozen from the fifth-successor correction onward;
the original audit-result blob at commit `9eace50` was
`ee2075547d7c7895c7d0ebaabfc64d1e09f5ba3a`, before later provenance and F-12 errata. The corrected
copy distinguishes that checkpoint's prose fixes from repository obligations that remained
unimplemented, but it is not the latest review of the subsequently changed paper and is not
claimed to have been retrospectively immutable. Current review chronology continues through the
frozen
[`docs/DESIGN_RESEARCH_DISTILLATION_AUDIT.md`](DESIGN_RESEARCH_DISTILLATION_AUDIT.md),
[`docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP2.md`](DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP2.md),
[`docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP3.md`](DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP3.md),
[`docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP4.md`](DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP4.md),
[`docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP5.md`](DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP5.md),
and
[`docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP6.md`](DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP6.md).

The current successor records are
[`docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP7.md`](DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP7.md),
[`docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP8.md`](DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP8.md),
[`docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP9.md`](DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP9.md),
and
[`docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP10.md`](DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP10.md)
with exact provenance projected in [`docs/HANDOFF.md`](HANDOFF.md).

The proposed authenticated-state checkpoint—including a candidate 192-bit canonical coordinate-key
encoding and the still-open comparison between a constrained binary Merkle Patricia trie and a
192-level sparse Merkle tree—is isolated in
[`docs/AUTHENTICATED_STATE_PLAN.md`](AUTHENTICATED_STATE_PLAN.md). No authenticated trie or
proof codec is implemented in the current runtime. Verkle is recorded there as a separate,
cryptography-gated feasibility lane; it is not selected and does not share the two hash candidates'
`NodeHash` assay.

The immutable first finite audit is [`docs/FUTURE_PLAN_AUDIT.md`](FUTURE_PLAN_AUDIT.md). The
latest second-loop audit, including the Verkle disposition and its own frozen input, is
[`docs/FUTURE_PLAN_AUDIT_LOOP2.md`](FUTURE_PLAN_AUDIT_LOOP2.md). They audit specifications and
dependencies, not implementation or security evidence.

The large state/Verkle/tick/proof research conversation from `design.txt` is source-receipted and
compressed in
[`docs/DESIGN_RESEARCH_DISTILLATION.md`](DESIGN_RESEARCH_DISTILLATION.md). That file is a
notes-only question map with corrected boundaries, rejected claims, and candidate research lanes;
it does not activate a capability-tree node or change the runtime.
Its first bounded successor review is frozen in
[`docs/DESIGN_RESEARCH_DISTILLATION_AUDIT.md`](DESIGN_RESEARCH_DISTILLATION_AUDIT.md), and the
external input identity/count method is pinned by
[`docs/design-source-receipt-v1.json`](design-source-receipt-v1.json). The audit corrects notes
and checker coverage only; it is not cryptographic or implementation validation.

The first correction is pinned at `ea9294ba08b5e4bd339f82894ecb761461f5af13`. A second complete
read-only sweep of that corrected state is frozen in
[`docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP2.md`](DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP2.md).
It records 11 successor type, dependency, provenance, checker, and CI gaps before their one
coordinated documentation/tooling correction at
`54926f52fa8ba9768e817ecb6139dcca41960e6a`. A third complete read-only sweep of that correction is
frozen in
[`docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP3.md`](DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP3.md).
It records eight launch-anchor, migration-envelope/body, validation-order, Verkle-admission,
checkout-EOL, authority-inventory, and successor-scope gaps before correction
`b23ddc95e5075c35d0dbf918b89f353fcdd40ddd`. A fourth complete read-only sweep of that exact
correction and Git tree `3de2d45aa20b08318f1a1af4d93308daccf1f55e` is frozen at commit
`8a5743595a0aa8ff13ede576f282b92540db5d9b` in
[`docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP4.md`](DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP4.md),
whose first-addition Git blob is `350d1af6279d7f441f45fba2689481c0dbea79a5`. It records eight
launch-closure, block-context availability, execution-input, outer-framing, conditional-predicate,
capability-owner, and review-provenance gaps before correction
`2cb1cc02320d180ac9ee34bd103bd44459f128fa`. A fifth complete sweep of that correction and Git tree
`60e9714c6d691f383fc75be03f6c9927c7665b84` is frozen at commit
`0d0122d9bf84664511db7588bb09d5c590a006f9` in
[`docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP5.md`](DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP5.md),
whose first-addition Git blob is `519451b120b458735afaa59daa01aa23466e165e`. It records eight
successor-activation-closure, pre-limit work, parent-lookup, launch-count, release-authorization,
capability-dependency/status, and corrected-audit-provenance gaps before correction
`1821f37d1f042115a4ffb3bd50292885e581d812`. A sixth complete sweep of that correction and Git tree
`9b8c08b10c611dd62ac8dbd7b4e46d1b0a02bf90` is frozen at commit
`80b58e0a40408e03f16e2d0c739c177e05c22903` in
[`docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP6.md`](DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP6.md),
whose first-addition Git blob is `5374b160424b78bd516ffc598e9bbd3b29392227`. Its frozen RG set
covers accepted-parent evidence multiplicity, protocol-identity-hash failure,
external-genesis-root, and phase-status-projection issues before the coordinated RG correction
that became the next audited input.

A seventh complete sweep of that correction is frozen in
[`docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP7.md`](DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP7.md).
The machine-checked `Loop7ProvenanceV1` projection in
[`docs/HANDOFF.md`](HANDOFF.md) is the sole current display of its exact input, first-addition
commit/tree/blob, and LG-set cardinality. It records proposal-credential,
live-authority-compromise, availability-dependency, replay, launch-DAG, successor-equation,
unlock-frontier, and handoff-projection issues before the coordinated descendant correction.

The descendant proposal makes the indexed accepted core signature-free, separates the bounded
no-double-weight `AcceptedProposalCredentialSetV1` from accepted evidence, pairs consensus-entailed
availability through `SameWrapperConsensusDependencyWitnessV1`, and replays one parent-ordered
`AcceptedCoreReplayEntryV1` sequence whose entries carry `NormalizedAcceptedEvidenceSnapshotV1` or
`RetainedAcceptedEligibilityWitnessV1` under an explicit `ReplayEvidenceCutoffV1`.
`PreventiveProfileOwnedCryptographicMigrationV1` is distinct from
safe-halt/recovery after a live
authority-binding compromise. Launch now uses the actual combined scope/registry/genesis/candidate
DAG with one counted outer genesis artifact at a nonzero checked rank, while successor activation
uses exact internal-only seven-plus-supplementary equations relative to an already verified semantic
predecessor context. These are proposed byte and verifier contracts, not implemented behavior.

An eighth complete sweep of that correction is frozen in
[`docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP8.md`](DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP8.md).
The machine-checked `Loop8ProvenanceV1` projection in
[`docs/HANDOFF.md`](HANDOFF.md) is the sole current display of its exact input,
first-addition commit/tree/blob, and EG-set cardinality. It records purpose-split successor
authority, replay-archive commitment, historical replay authority, unavailable recovery,
exact-invariant guard, and provenance/count-projection issues before the coordinated
descendant correction.

The corrected descendant retains the eighth proposal to derive each normalized-authority entry ID from one proof-free canonical core
and derives the view root from sorted unique `(lookup_id, entry_id)` pairs; retained projections
cannot substitute an uncommitted subset. Live consumers bind an exact consensus-native parent-
authority certificate only after its certificate-independent consumer context, evidence commitment,
exact component-total witness-to-accepted-wrapper receipt coverage (including conflicting evidence),
authorization, consensus admission, availability,
position, and status verify; the replay archive never selects live authority. Replay later
authenticates the full accepted-certificate bytes in the authority-use event and reconstructs every
range-derived receipt from pinned raw wrapper occurrences, their live consensus positions, and their
receipt IDs, even when its archive checkpoint is produced after the consumer. The later archive
ordinal proves custody order only; live authority comes from the pre-admission consensus universe.
Later evidence can
change a named status projection, but cannot alter the already accepted certificate. These remain
proposed schemas and controls.

The ninth sweep broke the occurrence-universe and chronology closure claims at its frozen input.
Component-total coverage of a candidate-selected receipt set did not prove that it contained every relevant accepted
occurrence before admission, and a content-derived receipt could not distinguish the original live
occurrence from byte-identical content archived later. The coordinated correction now derives and
authenticates a consensus-positioned evidence cutoff and complete accepted-occurrence universe before
admission. Its exact inclusive scoped range and lower/gap-free/upper exclusion witnesses derive the
receipt and status inputs rather than trusting a candidate subset. Each receipt binds its live
consensus position and exact bytes. Replay may archive identical bytes later only as recovery of that
already positioned receipt; it cannot invent an occurrence absent from the live universe root or
become live authority.

Planning now assigns live certificate issuance/admission to `CONSENSUS-1@base`. `SYNC-1@base`
only transports and retains bounded bytes; its separate post-consensus
`SYNC-1@replay-authority` phase reconstructs historical use for `TESTNET-1` and never feeds live
authority backward into consensus.

The coordinated checker correction now tests one compatible inherited-base/phase/conditional role
union, guards the complete named live-certificate/replay assignment chain and its hash contexts,
derives each current audit cardinality from the frozen finding table, scans every mutable planning
authority for malformed semantic projections, proves each first-addition commit reaches the explicit
checked lineage, and requires cycle controls to fail on the named cycle mechanism. These remain
planning controls, not implementation security properties or capability promotions.

The latest frozen successor sweep exposed further material gaps or falsely closed residuals within
its declared finite lenses. The protocol-side findings concern an unbound occurrence-history lower boundary,
historical signing chronology that current artifacts cannot prove, a consumer-context exclusion
contradiction. The checker-side findings concern quoted
domain bytes, post-blank assignment tails, helper assignments, wrapped or negated semantic
projections, and first-addition operational/merge failure. The cross-document findings concern
`COMMIT-R1` implementation wording, the zero-only convergence gate, stale chronology, and historical
checkpoint-review ownership. Their coordinated correction remains proposed protocol,
documentation, checkout-policy, and checker work, not implementation or security evidence.

The user's earlier Loop 10 stop was later lifted so correction and successor sweeps can resume; this
does not establish convergence.

The paper correction makes the admitted parent profile, occurrence-index policy, and immutable start
rule resolve one inclusive `ConsensusNativeAcceptedRecordOccurrenceHistoryStartV1`. The exact object
is carried by both cutoff and scope, and
`VerifiedConsensusNativeAcceptedRecordOccurrenceHistoryStartEqualityV1` must pass before range
verification, so an internally valid later-start suffix cannot hide an earlier conflict. Release and
activation artifacts now claim only certificate-before-verification/acceptance/use order; identical
artifacts with opposite alleged signing histories remain indistinguishable without a separately
authenticated authorization position. The recursive consumer-context guard excludes every current
certificate/consumer-chain role while allowing exactly the already independently verified prior
migration `activation_authorization_id` path. These are proposed verifier contracts, not runtime
behavior.

The checker correction preserves quoted domain bytes while removing layout whitespace, parses each
guarded assignment through its next top-level boundary rather than stopping at a blank line, and
rejects orphan post-blank variants, fields, or hash tails. The four generic envelope helpers are now
exact-guarded as one declared transitive dependency closure. Semantic projection checks normalize
intra-paragraph wrapping and recognize numeric or word counts plus explicit `zero`, `no`, and `none`
claims in forward and reverse forms. First-addition proof fails closed on Git operational errors,
checks every parent for prior path presence, and separately requires the addition commit to be an
ancestor of the explicit checked lineage. These controls guard the named finite mechanisms only.

None of these audits or corrections activates a runtime capability or constitutes Rust, Lean,
cryptographic, consensus, or Layer 1 evidence. Broad implementation tests are intentionally
deferred. The tenth sweep found material gaps and falsely closed residuals, so it did not satisfy
the declared diminishing-returns boundary. Its only executed repository checks were the narrow
planning-checker self-test and whitespace check; a later complete successor sweep must freeze zero
new material gaps and zero falsely closed material residuals within its declared finite lenses
before broad tests begin.
