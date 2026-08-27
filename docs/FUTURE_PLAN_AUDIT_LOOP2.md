# Second-loop audit of the Bitblock future plan

**One read-only sweep followed by one coordinated fix sweep — 2026-08-27**

This successor audit freezes exact input
`2a9e66731e3ca9e0e433b49f185d2a1cb7096a02`. The worktree was clean at that commit. Three
independent read-only lenses examined authenticated-state security, capability/activation logic,
and motive/provenance consistency; a fourth local pass reconciled their reports and the user's
mid-sweep Verkle question. No file was edited until the 16 findings below were consolidated and
closed to further discovery.

The lens reports were transient agent outputs rather than separately committed artifacts. Their
finite finding sets, exact anchors, corrections, no-gap areas, target commit, and clean/no-edit
attestations are retained here as the audit record; this file does not pretend to preserve their
verbatim transcripts or independently authenticate who produced them. The result is the first
descendant commit of the frozen input that adds this file. A Git commit cannot contain its own final
identifier.

The first-loop audit remains immutable at Git blob
`8a02f4b7b00b440e14a35a098b87a7493fd887b4`, first added by
`2a9e66731e3ca9e0e433b49f185d2a1cb7096a02`. This loop does not append findings to that historical
set. A retained checker guard rejects a changed current copy of the first-loop audit.

## Evidence boundary

The sweep inspected `README.md`, `docs/PROJECT_INTENT.md`, `docs/CAPABILITY_TREE.md`,
`docs/AUTHENTICATED_STATE_PLAN.md`, `docs/FUTURE_PLAN_AUDIT.md`, `docs/TRANSCRIPT_AUDIT.md`,
`docs/HANDOFF.md`, `docs/LAYER1_DESIGN_PAPER.md`, `docs/LAYER1_DESIGN_AUDIT.md`, and
`tools/check_planning_docs.py`. Source/proof files remained unchanged from
`6545d8e7d60e75607445fda564bbf11610a62658` through the frozen input. The sweep ran only read-only
Git/document checks; it did not run Rust, Lean, network, storage-fault, cryptographic, or
physical-display suites.

“Patched” below means the plan now names a deterministic contract, dependency, gate, or explicit
open boundary. It never means that the runtime implements the planned capability.

## Frozen finding set and one-sweep patch map

| ID | Priority | Gap at `2a9e667` | One-sweep correction |
| --- | --- | --- | --- |
| FP2-01 | P0 | `CommitmentSetV1` had neither a canonical identifier nor an acyclic ledger/root/manifest/pointer graph; its root set, ordering, position, and predecessor rules were ambiguous. | Define canonical `RootEntryV1`, a tagged position, exact domain-set equality, domain-separated set ID, predecessor/genesis rules, one-way identifier graph, and compare-and-swap recoverable publication. |
| FP2-02 | P0 | `commitment_profile_id` could become self-referential, while callers supplied raw proof limits that authority implementations could resolve differently. | Derive the profile ID from a self-excluding descriptor; bind an immutable proof-policy/limit digest to it; allow local policy only to tighten the authoritative maximum. |
| FP2-03 | P1 | Patricia consumed-depth/empty-root semantics and sparse-tree bitmap sibling depth still admitted off-by-one interpretations. | Freeze exact MPT empty/leaf/extension/branch depth formulas and state that bitmap index `d` carries the sibling subtree after decision bit `d`, at consumed depth `d + 1`; add boundary mutants. |
| FP2-04 | P0 | The GC generation fix protected new writes but not a newly pinned root that reused old unpinned nodes; provisional pins had no crash retirement rule. | Require root pinning before visibility plus a pin-epoch barrier/restart rule, deterministic promotion/retirement of provisional pins, and mark/pin/sweep/crash schedules. |
| FP2-05 | P1 | Cross-map refinement constrained active-token values but did not require the world root to equal ledger occupancy or the episode root to contain every and only retained episode. | Require exact same-head reconstruction of world including origin, mutable active episodes, and the retention-policy episode projection. |
| FP2-06 | P0 | Scope, genesis, and profile bytes had competing owners and duplicated values without a projection rule; the genesis gate depended on later root implementations and recreated a phase cycle. | Make the paper own one scope-to-genesis-to-candidate byte pipeline, require equality of every repeated projection, keep `GENESIS-D1` at logical bytes, and add downstream genesis-root parity. |
| FP2-07 | P0 | `TRANSITION-SPEC-D1` promised profile-specific nonce, fee, optional-domain, and root behavior before those designs existed; `EXECUTOR-1` could pass without an exact candidate or bounds. | Narrow the early base specification, add an exact composed `TRANSITION-SPEC-1@profile`, bind executor instances to it, the candidate, bounds, and activated extensions, and make `COMMIT-1` depend on the base contract. |
| FP2-08 | P0 | `EXECUTION-ROOT-1` named no enforceable producers for optional program, economic, or validator-schedule domains; using the downstream authority nodes as producers creates cycles. | Add upstream deterministic state/design producers, then make the composed root consume them while consensus, program-consensus, and incentive-evidence nodes consume the root afterward. Inactive domains use the profile's canonical required-domain set rather than an undefined root. |
| FP2-09 | P1 | A stake-weighted consensus choice had no pre-genesis economic schema; the only validator-economics node was downstream of implemented consensus. | Add a pre-genesis validator-economic design node and require it conditionally for stake-weighted consensus, scope, genesis, state production, and later evidence. |
| FP2-10 | P1 | Gate applicability was only whole-row granularity: G5 mixed mandatory interleaving/atomicity with optional assets, and G15 mixed mandatory resource bounds with optional fees/rent. | Use stage-by-gate-subclaim predicates; keep G5 core and G15 resource claims mandatory, make only economic/program clauses conditional, and reject unresolved predicates. |
| FP2-11 | P1 | The release node used a candidate manifest where the testnet contract required a signed release bound to the admitted profile digest. | Split candidate reproducible-build evidence from the admitted-profile release identity and require the latter at testnet. |
| FP2-12 | P1 | Conditional and `node@profile` dependencies existed only in prose; the checker flattened some to unconditional base edges and omitted unnamed edges, so it could not test activation polarity. | Add a structured conditional-instance edge manifest, validate its bases/roles/predicates and conservative union, and require each future candidate closure to resolve and check its own instance graph. |
| FP2-13 | P1 | The planning checker let node inventory disappear, accepted arbitrary status text, parsed the last node through later sections, had no guard for the frozen first audit, and advertised unpowered Markdown paths. | Pin the expected inventory/status class and first-audit blob, stop fields at the next peer/higher heading, add powered inventory/status/Markdown/freeze/conditional-edge controls, and narrow claims to what is actually checked. |
| FP2-14 | P2 | The first receipt used a state-dependent `git diff --check`, and process/quotation prose could be read as stronger provenance than was retained. | Preserve the first audit unchanged; record stable historical diff/show checks here, state the exact retained/transient provenance boundary, and label transcript paraphrases as paraphrases. |
| FP2-15 | P2 | `THREAT-D0` was READY through an uncompleted `COST-1` measurement prerequisite even though it needed only the existing foundation and cost-method vocabulary. | Name the implemented foundation directly; retain cost measurement as input to later quantitative `THREAT-1`, not as fictitious completed evidence. |
| FP2-16 | P1 | The paper allowed a Verkle-style map but the two-candidate research plan neither admitted nor rejected Verkle, so “winner” could be misread as global. | Keep the cheap MPT/SMT hash baseline, add a separate Verkle feasibility/prototype lane, and require exact vector-commitment assumptions, 192-bit embedding, codecs, adversarial proofs, end-to-end cost/TCB, availability, and migration evidence before admission. |

## Verkle disposition

The user asked “Verkle Trees?” during the read-only sweep. The answer is **yes as a separately
gated research lane, no as an automatic replacement**. The original proposal uses vector
commitments to trade additional construction/update computation for smaller proofs, with branching
factor controlling that tradeoff ([Kuszmaul, *Verkle Trees*](https://math.mit.edu/research/highschool/primes/materials/2018/Kuszmaul.pdf)). A concrete Ethereum Foundation design illustrates how a width-256 tree introduces
elliptic-curve Pedersen/inner-product commitments, field encoding, and a different node layout
([Ballet and Feist, *Verkle tree structure*](https://blog.ethereum.org/2021/12/02/verkle-tree-structure)).
Those sources motivate a candidate; they do not establish that their key layout, cryptographic
scheme, parameters, performance, or security contract is right for Bitblock.

The first Bitblock checkpoint remains a comparison of two hash-based structures under one common
`NodeHash` contract. A Verkle parent is a vector commitment and cannot honestly share that
one-variable assay. Its feasibility lane must first freeze the commitment scheme, curve/field,
point/scalar/subgroup rules, generator or setup provenance, Fiat–Shamir transcript, absence and
deletion semantics, 192-bit key embedding, proof/update limits, independent implementation, Rust
dependency and binary cost, and migration/reversal policy. Until it beats a predeclared
end-to-end threshold on an actual Bitblock workload, it is neither selected nor allowed to delay
the simpler first authenticated-root implementation.

## Areas checked with no material second-loop gap

- The exact 62-node declared base graph at the frozen input parsed with 231 dependency edges and no
  named-node cycle. That result did not cover conditional instance graphs; FP2-12 closes the claim
  boundary rather than reinterpreting the old pass.
- MSB-first coordinate traversal, child direction, default sparse-tree recurrence, Patricia
  longest-prefix insertion/deletion normalization, caller-selected query/claim meaning, typed root
  separation, logical pruning versus physical GC, and availability/finality non-claims remained
  coherent apart from the exact gaps listed above.
- Bedrock economics remains an explicit asset decision; local-storage bounds, upgrade old/new
  polarity, VM-design-before-bounds ordering, G18 production ownership, and the F-12/G16 erratum
  stayed repaired.
- Every previously recorded user motive remained present, and the Verkle question is now recorded
  without promoting proof-size intuition into a protocol choice.
- No trie, Verkle commitment, transferable asset, network identity, consensus, cryptocurrency, or
  Layer 1 authority is implemented or claimed.

## Validation receipt

The coordinated patch is admitted only if these commands exit zero:

```powershell
python -B tools/check_planning_docs.py --self-test
git diff --cached --check
```

After the result commit exists, the stable historical whitespace checks are:

```powershell
git diff --check 2a9e66731e3ca9e0e433b49f185d2a1cb7096a02..HEAD
git show --check --format=fuller HEAD
```

For the first-loop historical patch, both
`git diff --check 162406819133c1fd57d0fd26cbe5fa0b495766b8..2a9e66731e3ca9e0e433b49f185d2a1cb7096a02`
and `git show --check --format=fuller 2a9e66731e3ca9e0e433b49f185d2a1cb7096a02` exited zero in
this read-only sweep. This is documentation/whitespace evidence only.

## Residual boundary

This loop closes the 16 listed planning contradictions. It does not prove the plan complete, the
cryptography secure, or any future implementation correct. The implementation frontier remains
`ACCOUNT-1`; MPT/SMT work remains `COMMIT-R1`; Verkle begins only at feasibility; and every authority
profile remains locked behind its exact artifact, instance closure, gate result, and loss condition.
A later audit must pin a new input and create a new successor artifact rather than mutate either
frozen audit.
