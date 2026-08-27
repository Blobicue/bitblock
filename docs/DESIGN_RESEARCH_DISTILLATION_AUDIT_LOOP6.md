# Sixth bounded successor audit

**One read-only gap sweep followed by one coordinated fix sweep — 2026-08-27**

This successor audit freezes exact clean input
`1821f37d1f042115a4ffb3bd50292885e581d812`, Git tree
`9b8c08b10c611dd62ac8dbd7b4e46d1b0a02bf90`, on
`codex/bitblock-progress-2026-08-26`. The remote branch matched that commit and the isolated
worktree was clean before and after discovery.

The principal inspected blobs were:

```text
d00fa7e22e02e308a3aae2b968c346b56e3e8a3b  docs/LAYER1_DESIGN_PAPER.md
debf04b32c1404eeb8ca6e555132e39765f182a7  docs/AUTHENTICATED_STATE_PLAN.md
4da32d1adce5397efd051966438b9492d6f2cba9  docs/CAPABILITY_TREE.md
e37528b5b5da1b49b955b48ffc9a31983ad1d09c  docs/DESIGN_RESEARCH_DISTILLATION.md
06751ab282642424df5e72f34affc22538660a4b  docs/HANDOFF.md
519451b120b458735afaa59daa01aa23466e165e  docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP5.md
124e066b418a61279abf90c1b1b2c2cc0604ef2a  docs/LAYER1_DESIGN_AUDIT.md
c3169c10d8f993f5e7bc78c2af2cedd94d0cc15a  docs/TRANSCRIPT_AUDIT.md
995f037f67217f1e92cb926a3cfb094d3816868d  docs/TIMELINES.md
dff1d825d6bb2ea0446cc36a6204c7fc0465607d  tools/check_planning_docs.py
df441fea73fd82c9709f14fb442a466fa13d314a  .github/workflows/ci.yml
403a2b57d1f83b4b801afcce24ab7798d70bdb45  .gitattributes
1cab847b8832a3cd7867b10982d3cbad1bbcac7e  README.md
```

Independent read-only lenses attacked protocol/type/cryptographic closure and
state/availability/replay closure. A separate capability-graph/provenance pass tested structured
status and dependency claims, exact frozen-history identities, and powered-checker behavior. The
passes were reconciled by merging one independently reproduced parent-identity break, challenging
the remaining candidates against the explicit residual boundaries, and retaining four material
findings. No audited input was edited during discovery.

The narrow planning checker was the only executable checker run:

```text
python -B tools/check_planning_docs.py --self-test
planning-doc check PASS: 72 capability nodes, 94 phase-status tuples,
360 declared base-plus-phase-plus-conditional edges, 10 predicate entailments,
19 Markdown files and 1 frozen source receipt with negative controls
```

One additional read-only in-memory mutation changed a node-prose phase projection from
`LOCKED@prototype` to `READY@prototype`; the full capability parsers and Markdown check accepted it
while authoritative row `PS-019` remained `LOCKED`. No Rust, Lean, broad Python-vector, headless,
network, storage-fault, cryptographic-implementation, performance, or physical-display test was run.

This record preserves findings and break witnesses, not verbatim agent transcripts or agent
identity. It must be committed before the correction. The correction may pin this file's
first-addition Git blob but must not mutate this file.

## Frozen finding set and one-sweep correction map

| ID | Priority | Gap at `1821f37` | Concrete loss mechanism | One-sweep correction |
| --- | --- | --- | --- | --- |
| RG-01 | P1, falsely closed PG-03 | `accepted_parent_lookup_id` identifies a proposal through its typed `block_id` context, while `ConsensusEvidence` and `AvailabilityAssessmentV1` remain detached, policy-shaped, and outside `block_id`. Nevertheless the index requires exactly one byte-exact `AcceptedBlockRecord` for that lookup ID and rejects multiple records. | Two honest quorum subsets, confirmation snapshots, or availability attestations can authenticate the same proposal while producing different accepted-record bytes. Honest evidence growth or an attacker inserting a second independently valid wrapper can therefore poison the index, reject a legitimate parent, or force an undeclared and node-dependent wrapper-selection rule. | Make the lookup resolve one byte-exact accepted proposal/core under `block_id`; store verified consensus and availability evidence as a separately typed, possibly multiple set. Freeze the profile-owned monotone parent-eligibility/status rule, evidence identity and conflict handling; additional qualifying evidence cannot invalidate an eligible core. Reject a true distinct-core collision, not evidence multiplicity. Propagate this semantic identity through release and activation parent references, and power two-valid-wrapper, status-upgrade, nonqualifying-evidence, conflicting-evidence, and distinct-core-collision controls. |
| RG-02 | P1, new contradiction | `ProfileIdHashV1` permanently identifies genesis, candidate, admission, release, launch, lookup, and successor objects outside candidate-selectable profile cryptography, while the threat and upgrade surfaces promise a migration path for hash failure. | If `ProfileIdHashV1` itself loses collision or second-preimage security, an attacker can substitute differently encoded launch, release, lookup, or successor objects under an already authorized identity. A successor identified and authorized through that same failed function cannot non-circularly repair the trust anchor, so the stated generic hash-migration claim is false. | Scope in-place cryptographic migration to profile-owned suites and make compromise of `ProfileIdHashV1` a named terminal safe-halt/new-genesis boundary, unless a separately trusted non-self-selecting transition is precommitted before launch. Correct threat, upgrade, capability, and handoff claims and distinguish an ordinary profile-hash migration control from a protocol-identity-hash compromise control. |
| RG-03 | P1, residual PG-04 | `LaunchClosureBundleV1` declares `7 + Length(S) = Length(D)` and requires every dependency ID to resolve to one carried lower-ranked artifact, but the outer `genesis_package` is excluded from `D` and the byte totals. Candidate identity, scope/registry linkage, derivation, and genesis release authority still depend on that outer genesis object. | A complete walker must either omit real candidate-to-genesis and release-authority-to-genesis dependencies, add an uncounted/unranked node, or reject every otherwise valid launch carrier. Implementations can disagree about whether the externally carried genesis is inside the ranked closure, and an outer/bundle mismatch can escape the advertised complete-closure equations. | Add one typed bounded external-root convention for the outer `genesis_package`: bind its exact `genesis_id`, give it a fixed rank and count, state the only allowed incoming edge classes, and require byte equality with the outer carrier. Keep internal-artifact and external-root count/byte equations distinct and forbid every other external dependency. Power omitted-genesis-edge, external-root injection, rank-to-unranked-root, outer/root mismatch, and count mutations. |
| RG-04 | P1/P2, residual PG-07 | The phase-status manifest is authoritative, but its declared prose projections are not checked. The complete checker accepted a mutation from `LOCKED@prototype` to `READY@prototype` in node prose while `PS-019` remained `LOCKED`. Equivalent stale or promoted secondary status/open-decision prose can contradict the scheduling surface. | Agents primarily reading a node can schedule or describe authority work from a false projection while the exact manifest digest and all advertised phase-status negative controls stay green. The manifest prevents machine promotion but does not keep the repository's explanatory truth synchronized. | Give every secondary phase projection a canonical machine-readable `STATUS@phase` form that maps to exactly one authoritative tuple, or remove the duplicated status claim and reference the tuple ID. Require exact projection equality and reject missing, duplicate, unknown-phase, unknown-status, and prose-only promotion/demotion mutations without making prose an authority source. |

## Candidates rejected or classified after challenge

- A fixed `accepted_record_id` over the entire evidence-bearing wrapper would remove index aliasing
  only by making children name one evidence occurrence. It would still require every verifier to
  retrieve that exact honest quorum subset or confirmation snapshot. The minimum correction instead
  separates the unique proposal/core from non-unique evidence unless a future consensus profile
  proves wrapper uniqueness and availability.
- Consensus choice, finality thresholds, evidence codecs, and numeric confirmation depths remain
  `OPEN` or `LOCKED`. RG-01 does not choose them; it requires the generic carrier/index semantics not
  to contradict the multiplicity the paper already permits.
- Candidate-selectable suite migration cannot rescue a failed pre-profile identity hash. A
  multi-hash handoff could be designed before launch, but inventing one during this correction would
  create a new untested authority protocol. The honest minimum is the safe-halt/new-genesis boundary.
- Duplicating `ChainGenesisPayloadV1` inside the embedded bundle is unnecessary and creates a second
  byte owner. RG-03 instead makes the already outer-carried genesis the sole typed external root.
- Ignored semicolon tails in `Depends on` contain no backticked capability IDs at this input, and the
  exact base/phase/conditional digests still match. No second dependency-parser break survived.
- The original checkpoint-0.3 audit-result blob `ee2075547d7c7895c7d0ebaabfc64d1e09f5ba3a`,
  frozen corrected reading-copy blob `124e066b418a61279abf90c1b1b2c2cc0604ef2a`, and Loop 5 audit blob
  `519451b120b458735afaa59daa01aa23466e165e` remain distinguishable. No new provenance gap survived.
- Release authorization is non-circular at the planning level, successor admission/activation is
  availability-bound under the old envelope, and profile-owned shallow preflight limits precede
  nested materialization. These prior gaps did not reproduce.

## Areas checked with no additional material gap

- The launch carrier is consistently embedded-only; explicit and supplementary artifact equations,
  internal dependency-node/edge counts, preflight ordering, and byte-sum exclusions are otherwise
  coherent. RG-03 is confined to the deliberately outer genesis root.
- The successor activation bundle binds the target admission, release, migration evidence,
  authorization, activation height, and old-profile availability carrier. No ambient successor
  artifact map is required by the proposed verifier.
- Canonical `BlockContext`, total ordinary/migration execution, typed state/history separation,
  MPT normalization, sparse-Merkle defaults, logical pruning versus physical garbage collection,
  and the snapshot-retention non-claims survived the state/replay lens.
- The 72-node base graph, 94-row phase manifest, five phase dependencies, 44 conditional edges,
  predicate contexts, and phase DAG survived direct structural checks apart from RG-04's duplicated
  prose projection.
- The 19-document planning inventory, nine frozen text artifacts, contained relative links, powered
  checker path, CI command descriptions, correction commit, and remote branch pin matched the
  audited input.

## One-fix-sweep rule

The correction may change only the protocol/planning/checker/handoff surfaces needed to close
RG-01 through RG-04, connect this audit to the frozen inventory/read order, and power the new
controls. It may not activate a capability, implement a network or cryptographic primitive, edit
any previously frozen audit or this file, or silently add a fifth finding. Any later discovery
belongs to another pinned loop.

The correction remains documentation, checker, attribute-policy, and handoff work. Narrow checker
execution and whitespace checks are correction gates. Broad runtime/proof tests begin only after a
complete later read-only sweep finds neither a new material gap nor a falsely closed material
residual under its declared finite lenses.

## Residual boundary

Closing these findings cannot prove that the protocol is secure, that `ProfileIdHashV1` remains
cryptographically sound, that a chosen consensus profile gives monotone parent eligibility, that
launch artifacts are available in deployment, that any candidate profile is satisfiable, that the
implementation respects the proposed byte graph, that CI infrastructure is honest, or that Rust
refines the planning documents. The next complete sweep must pin the correction and may declare
diminishing returns only within its explicit finite coverage.
