# Seventh bounded successor audit

**One read-only gap sweep followed by one coordinated fix sweep — 2026-08-27**

This successor audit freezes exact tracked input
`62afdd24a8525704f5949584859bd7cbad300d4d`, Git tree
`ee346fb3aa9bc54591f851dcb482fd92be9da84a`, on
`codex/bitblock-progress-2026-08-26`. The remote branch matched that commit before this record was
written, and the tracked worktree had no staged or unstaged difference. Two generated `.lake`
directories appeared transiently during discovery from an unidentified concurrent process and were
not read, attributed, deleted, staged, or used as evidence; they were absent at the freeze check.
The commit/tree above, not ambient generated state, is the audited input.

The principal inspected blobs were:

```text
1a9e63adb2b383a16ac7f9081e40ef515899f2a1  docs/LAYER1_DESIGN_PAPER.md
5516254d01f14b3c27f4df816f98c27261b825c5  docs/AUTHENTICATED_STATE_PLAN.md
e397d3ace4288367c3a9cff0f5d91bb4df8570d5  docs/CAPABILITY_TREE.md
0be433a1b1f8a1daccab91eeab211f68dfa7da34  docs/DESIGN_RESEARCH_DISTILLATION.md
255a4608139f51e9d538a4a5d3256d83f05d7de5  docs/HANDOFF.md
5374b160424b78bd516ffc598e9bbd3b29392227  docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP6.md
124e066b418a61279abf90c1b1b2c2cc0604ef2a  docs/LAYER1_DESIGN_AUDIT.md
0d89c3984a0321292794d62ef695cc1d09399cf4  docs/TRANSCRIPT_AUDIT.md
995f037f67217f1e92cb926a3cfb094d3816868d  docs/TIMELINES.md
70d682730ec8b7b5a152ad76113337fb7df73446  tools/check_planning_docs.py
df441fea73fd82c9709f14fb442a466fa13d314a  .github/workflows/ci.yml
3463ea62d7eee61a5afbe6adb0f25af708d3349d  .gitattributes
755f49a795f93d6344b1d71ee5000f8c9bea0498  README.md
```

Independent read-only lenses attacked protocol/type/cryptographic closure,
state/availability/replay closure, and capability-graph/provenance closure. Follow-up challenges
reproduced the proposer-signature identity break and the active-profile authority-hash compromise,
constructed a concrete valid launch dependency ranking that the current external-root grammar cannot
express, and separated successor internal preimages from its already verified predecessor context.
The passes were reconciled into eight material findings. No audited input was edited during
discovery.

The narrow planning checker was the only repository checker run:

```text
python -B tools/check_planning_docs.py --self-test
planning-doc check PASS: 72 capability nodes, 94 phase-status tuples,
360 declared base-plus-phase-plus-conditional edges, 10 predicate entailments,
20 Markdown files and 1 frozen source receipt with negative controls
```

Read-only in-memory mutations then demonstrated two checker false closures. Replacing eligible
`VERKLE-R1@feasibility` with locked `VERKLE-R1@prototype` in the current unlock frontier passed while
authoritative row `PS-019` remained `LOCKED`. Separately, removing the Loop 6 audit from the handoff
read order or changing its displayed 20-document count to 19 passed the complete checker. No Rust,
Lean, broad Python-vector, headless, network, storage-fault, cryptographic-implementation,
performance, or physical-display test was run.

This record preserves findings and break witnesses, not verbatim agent transcripts or agent
identity. It must be committed before the correction. The correction may pin this file's
first-addition Git blob but must not mutate this file.

## Frozen finding set and one-sweep correction map

| ID | Priority | Gap at `62afdd2` | Concrete loss mechanism | One-sweep correction |
| --- | --- | --- | --- | --- |
| LG-01 | P1, falsely closed RG-01 | `accepted_parent_lookup_id` binds `block_id`, and `block_id` hashes the proposal header before signing. `AcceptedProposalCoreV1` nevertheless contains the complete proposal outer bytes, including `canonical_proposer_signature_bytes`, and rejects two core byte strings under one lookup. No admitted-suite rule requires a unique valid signature relation. | A randomized, threshold, or malicious authorized signer can produce two byte-distinct canonical signatures that both verify for the same `block_id`. The lookup stays equal while the alleged unique core changes, so honest signature multiplicity recreates the parent-index poisoning that Loop 6 removed for consensus and availability evidence; no hash collision is required. | Make the indexed core the exact signature-free proposal content: authority reference, header, and availability payload under the recomputed lookup. Store verified proposal credentials in a separately typed, bounded, sorted/deduplicated set; require at least one qualifying witness but assign no extra weight to multiplicity. Reject different unsigned content under one lookup, not multiple valid credentials. Power two-valid-signature, order, duplicate, invalid/wrong-domain, evidence-ID-collision, retention, no-double-weight, and genuine unsigned-core-collision controls. |
| LG-02 | P1, falsely closed RG-02 | `ProfileOwnedCryptographicMigrationV1` still promises parent-authorized in-place replacement when an active profile object-hash or signature suite fails. The old object hash binds `block_id`, parent selection, votes, the migration envelope, and the authority context that selects activation governance. Only `ProfileIdHashV1` compromise is currently terminal. | Distinct old-profile headers can collide to one `block_id`, receive the same proposer/consensus evidence, and produce one sound protocol-level lookup ID. Partitioned nodes can each retain one locally unique core, derive different governance state, and authorize different migrations; later seeing both only rejects locally. A transition identified and authorized through that compromised old binding cannot prove which parent or migration is canonical. Equivalent forgery can affect other live authority-binding roles. | Split preventive migration of a still-sound-but-deprecated suite from `ProfileOwnedAuthorityBindingCompromiseV1`. Ordinary migration is allowed only while an exact last-safe parent and every parent, consensus, availability, replay, activation, and historical-verification binding remain independently sound. An established collision/forgery in a live authority role safe-halts; absent a recovery envelope independently anchored and precommitted before compromise, restart requires new genesis. Make genuine distinct-core collision enter that disposition and power role-by-role hash, proposal/vote signature, governance/release credential, proof/state, precommit, and no-precommit controls. |
| LG-03 | P1, residual RG-01 | `ConsensusEntailedAvailabilityV1` names a `consensus_evidence_id`, while each `AcceptedRecordOuterV1` independently carries one consensus-evidence segment and one availability-assessment segment. The evidence-set rules neither require the entailed reference to equal that wrapper's evidence ID nor define an order-independent bounded pending/dependency closure. | An assessment can arrive before its named evidence, or arrive beside a different wrapper's evidence. One node can reject it permanently, another can retain it pending, and another can validate it from a later cross-wrapper entry, yielding different accepted evidence sets and authority status from the same finite wrappers. | For the consensus-entailed variant, require exact equality to the consensus evidence carried in the same wrapper, verify that evidence first, and insert the valid pair atomically; detached evidence remains separate. Preserve every retained assessment's dependency witness. Power cross-wrapper mismatch, reversed wrapper order, assessment-before-evidence, invalid evidence, dependency eviction, duplicate, and atomic no-partial-insertion controls. |
| LG-04 | P1, residual RG-01 | Independent replay still consumes a canonical one-per-height `AcceptedRecordOuterV1` sequence even though accepted-record wrappers are now explicitly plural ingestion carriers for one core and their evidence may accumulate or upgrade status. | Choosing one wrapper may omit qualifying evidence; replaying all wrappers creates several apparent ledger records at one height; choosing by arrival order makes finality/availability output node-local. Later evidence also leaves the status observation time undefined. | Define replay over the parent-ordered unique proposal-core sequence plus, for every core, a canonical bounded normalized evidence snapshot or retained eligibility witness at an explicit replay cutoff. Proposal-credential, consensus, and availability evidence remain typed and dependency-closed; wrappers are ingestion/archive carriers only. State whether statuses are at acceptance or the named replay cutoff and power split-wrapper, permutation, duplicate, status-upgrade, omitted-evidence, and one-core-per-height controls. |
| LG-05 | P1, falsely closed RG-03 | The external genesis descriptor has fixed rank zero and no dependency node, while `ChainGenesisPayloadV1` references the initial scope and registry digests. Current permitted edges instead claim scope/registry linkage *to* genesis, although those preimages contain no `genesis_id`. | With dependency targets required to have lower rank than referrers, the real graph needs `DomainRegistry` rank 0, `ProfileScope` rank 1, external `GenesisPackage` rank 2, and `CandidateProfile` rank 3 or higher. Fixed-rank-zero genesis cannot point to scope/registry, so a complete actual-preimage walker must omit real edges, reverse them, or reject every valid carrier. | Keep genesis bytes externally owned and counted once, but give the external genesis artifact its own dependency refs and nonzero checked rank in the combined graph. Encode actual `genesis -> scope/registry` and internal `candidate -> genesis` edges, distinguish internal-to-internal, internal-to-external, and external-to-internal edge counts, and validate one combined DAG without duplicating genesis bytes. Power the concrete valid ranking plus reversed-edge, missing-genesis-outgoing-ref, rank-zero, outer/root mismatch, extra external root, and count mutations. |
| LG-06 | P1, direct contradiction | `SuccessorActivationBundleV1` still exposes old `complete_*` count/byte fields and an untyped dependency-ID list, while the paper and capability tree say its equations are identical to launch. Loop 6 changed launch to an eight-plus-supplementary graph with an external genesis root, but successor prose separately says it inherits only the internal seven-plus-supplementary equations. | Implementations can count successor artifacts as `7 + S` or `8 + S`, include or exclude launch/genesis bytes, and accept external IDs in an allegedly internal dependency graph. The schema, equations, and claimed verifier input cannot all be true. | Make successor closure explicitly internal-only relative to a typed `VerifiedSuccessorPredecessorContextV1` semantic input that is verified before bundle processing and is neither serialized nor counted in the bundle. Rename/narrow dependency nodes and fields to internal artifacts/edges/bytes, state exact `7 + Length(S) = Length(D)` equations and exclusions, and require every external projection to match the unique predecessor context. Power count/length/edge, external-ID injection, context mismatch, purpose-tag, different-evidence-same-parent, duplicate prior bytes, and ambient-registry controls. |
| LG-07 | P1/P2, falsely closed RG-04 | The capability tree's “Current unlock frontier” is scheduling guidance but remains unparsed prose. The checker accepted a locked prototype in the eligible research list while the authoritative status manifest stayed unchanged. | An agent can schedule authority-bearing or locked work from a false projection even though the exact 94-row status digest and all node `Status` projections pass. This is the same operational loss class RG-04 intended to close. | Replace the prose list with a structured frontier projection whose entries reference exact `PS-NNN:NODE@phase=READY` rows, declare its exact set/order and role, and reject any locked/unknown/duplicate/missing/substituted/reordered entry. Keep narrative sequence explicitly non-authoritative or derive it from structured edges. |
| LG-08 | P2, new checker/provenance omission | The handoff's numbered 20-document read order and displayed 20-document/ten-frozen-artifact counts are not parsed against the checker's authoritative inventories. Removing an audit from the order or changing 20 to 19 leaves the powered checker green. | A future correction can freeze a new audit yet omit it from the only human/agent reading sequence, or display stale counts that falsely imply complete coverage. The underlying inventory remains correct while the handoff path lies. | Parse one canonical handoff read-order block and require continuous numbering, uniqueness, exact normalized document set and intended order. Derive or validate all displayed planning-document and frozen-artifact counts against canonical inventories. Power omission, insertion, duplicate, numbering gap, swap/order, wrong count, and untracked/out-of-repository path controls. |

## Candidates rejected or classified after challenge

- Canonical signature encoding gives one encoding for one signature object; it does not establish one
  valid signature object for a key/message pair. Deterministic honest signing likewise does not
  constrain a secret-key holder when the verifier accepts several valid signatures. LG-01 therefore
  cannot be closed by reusing the transaction-malleability prose.
- A protocol-level lookup over a compromised profile-owned `block_id` remains byte-stable but does
  not restore the unique preimage or authority it indexes. `ProfileIdHashV1` being sound is necessary
  and insufficient for LG-02. Preventive migration before an exploit remains distinct and allowed.
- Probabilistic reorganization does not by itself contradict same-core monotone evidence. Attained
  evidence status and current fork choice can remain separately typed; no additional finding
  survived there.
- The launch package should not duplicate `GenesisPackageV1` bytes. LG-05 changes the external
  artifact's graph role and edge direction, not its sole physical byte owner.
- The successor predecessor is a verified semantic authority input, not a second embedded copy of
  prior history. Serializing an evidence-bearing predecessor wrapper would recreate wrapper-choice
  ambiguity. LG-06 instead freezes the internal closure boundary and every projection from that
  external input.
- Multiple release or activation authorizations do not recreate parent lookup aliasing because the
  launch or migration context selects their exact record IDs. No separate authorization-set finding
  survived this pass.
- Consensus choice, concrete signature/evidence codecs, numeric retention limits, cryptographic
  suite selection, and recovery authority remain `OPEN` or `LOCKED`. The correction specifies loss
  conditions and carrier semantics without choosing those values.
- The original checkpoint-0.3 audit-result blob, corrected reading-copy blob, every predecessor
  successor-audit blob, and the frozen source receipt remained distinguishable and mechanically
  checked. LG-08 concerns the unguarded handoff projection, not a wrong canonical inventory.

## Areas checked with no additional material gap

- The 72-node base graph, 94-row phase manifest, five phase dependencies, 44 conditional edges, ten
  predicate entailments, role-aware predicate contexts, and their acyclic closures survived direct
  structural checks apart from the unparsed current-frontier projection.
- The 20-document planning inventory, ten frozen text artifacts, effective LF attributes, contained
  relative links, historical Git identities/counts, powered checker path, CI command description,
  Loop 6 correction commit, and remote branch pin matched the audited tree.
- Fixed pre-profile outer/shallow framing, authority-before-profile resolution, total ordinary versus
  migration execution, typed state/history roots, and purpose-tagged parent authority survived except
  where the new credential/evidence and compromise findings explicitly cut across them.
- MPT normalization, sparse-Merkle defaults, logical pruning versus physical garbage collection,
  Verkle's research-only boundary, and the stated snapshot/archive non-claims did not produce another
  material contradiction in this finite pass.
- Launch and successor bytes remain proposed schemas only. No runtime implementation, network,
  storage, cryptography, proof refinement, performance, or visual behavior was inferred from the
  planning checker.

## One-fix-sweep rule

The correction may change only the protocol/planning/checker/handoff surfaces needed to close
LG-01 through LG-08, connect this audit to the frozen inventory/read order, and power the new
controls. It may not activate a capability, implement a network or cryptographic primitive, edit
any previously frozen audit or this file, or silently add a ninth finding. Any later discovery
belongs to another pinned loop.

The correction remains documentation, checker, attribute-policy, and handoff work. Narrow checker
execution and whitespace checks are correction gates. Broad runtime/proof tests begin only after a
complete later read-only sweep finds neither a new material gap nor a falsely closed material
residual under its declared finite lenses, and that zero-gap sweep is itself frozen.

## Residual boundary

Closing these findings cannot prove protocol security, cryptographic uniqueness or soundness,
availability in deployment, consensus safety/liveness, candidate satisfiability, implementation
conformance, honest CI, complete archival evidence, or Rust/Lean refinement. It can only make the
current proposed carrier, dependency, authority-loss, replay, status-projection, and handoff claims
internally explicit enough for the next bounded attack.
