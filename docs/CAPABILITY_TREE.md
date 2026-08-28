# Bitblock capability tree

This is Bitblock's project-development “skill tree.” It records what each future capability
depends on, what unlocks it, how it is accepted, and what it still does not prove. It is not a
player progression system; the current Bitblock observer is not an entity and has no skills.

The tree is the scheduling source of truth. [`PROJECT_INTENT.md`](PROJECT_INTENT.md) owns the
motives, [`SEMANTICS.md`](SEMANTICS.md) owns implemented meaning, and
[`LAYER1_DESIGN_PAPER.md`](LAYER1_DESIGN_PAPER.md) owns the proposed protocol details. A node can
link to those documents, but prose alone never marks an implementation node complete.

## Status vocabulary

| Status | Meaning |
| --- | --- |
| **IMPLEMENTED** | The capability exists in the current source path; its exact limits still apply. |
| **BOUNDED** | A finite/model/research artifact exists, but the wider refinement or authority claim is open. |
| **READY** | Its declared prerequisites exist; it is eligible to become one future checkpoint. |
| **LOCKED** | At least one prerequisite artifact or decision is missing. |
| **OPEN DECISION** | Alternatives are recorded but no protocol choice has been made. |
| **NOT STARTED** | The node is named but no qualifying artifact exists. |

The following manifest is the authoritative phase-status surface. Every capability has exactly one
`base` tuple; a capability with a separately reportable research, decision, candidate, or authority
phase has additional tuples. Each node's `Status` field is only a checked, non-authoritative
projection of all the manifest rows owned by that node, in manifest order, using the exact token
`` `PS-NNN:NODE@phase=STATUS` ``. Projection prose is forbidden: explanations belong in the other
node fields, so a free-text promotion or demotion cannot disagree with the manifest while appearing
normative. A projection cannot create, remove, rename, duplicate, or promote a phase; the checker
requires byte-for-byte tuple equality with the authoritative rows. The checker row regex is ASCII
with exact single spaces and delimiters:

```regex
^\| (PS-[0-9]{3}) \| ([A-Z0-9]+(?:-[A-Z0-9]+)*) \| ([a-z0-9]+(?:-[a-z0-9]+)*) \| (IMPLEMENTED|BOUNDED|READY|LOCKED|OPEN-DECISION|NOT-STARTED) \|$
```

Phase-status IDs and `(node, phase)` pairs are unique. A phase named by the structured
phase-dependency manifest below MUST have a phase-status tuple, and every node named anywhere in the
tree MUST have its canonical `base` tuple. `OPEN-DECISION` is a status token, not permission for a
candidate to choose its own authority.

| Phase status | Node | Phase | Status |
| --- | --- | --- | --- |
| PS-001 | WORLD-0 | base | IMPLEMENTED |
| PS-002 | OBSERVE-0 | base | IMPLEMENTED |
| PS-003 | OBSERVE-0 | claim-scope | BOUNDED |
| PS-004 | TICK-0 | base | IMPLEMENTED |
| PS-005 | REPLAY-0 | base | IMPLEMENTED |
| PS-006 | REPLAY-0 | claim-scope | BOUNDED |
| PS-007 | PROOF-0 | base | BOUNDED |
| PS-008 | DOC-0 | base | IMPLEMENTED |
| PS-009 | ACCOUNT-1 | base | READY |
| PS-010 | LOCAL-BOUNDS-1 | base | LOCKED |
| PS-011 | STORAGE-1 | base | LOCKED |
| PS-012 | STORAGE-1 | research-journals | BOUNDED |
| PS-013 | LEDGER-1 | base | LOCKED |
| PS-014 | TRANSITION-SPEC-D1 | base | LOCKED |
| PS-015 | COMMIT-R1 | base | READY |
| PS-016 | VERKLE-R1 | base | READY |
| PS-017 | VERKLE-R1 | feasibility | READY |
| PS-018 | VERKLE-R1 | candidate-scope | LOCKED |
| PS-019 | VERKLE-R1 | prototype | LOCKED |
| PS-020 | COMMIT-1 | base | LOCKED |
| PS-021 | REFINE-1 | base | READY |
| PS-022 | REFINE-1 | world | READY |
| PS-023 | REFINE-1 | accounting | LOCKED |
| PS-024 | REFINE-1 | commitment | LOCKED |
| PS-025 | REFINE-1 | vm | LOCKED |
| PS-026 | IDENTITY-D1 | base | LOCKED |
| PS-027 | PROTOCOL-ID-1 | base | LOCKED |
| PS-028 | IDENTITY-1 | base | LOCKED |
| PS-029 | AUTH-COMMIT-1 | base | LOCKED |
| PS-030 | EXECUTION-ROOT-1 | base | LOCKED |
| PS-031 | GENESIS-ROOT-PARITY-1 | base | LOCKED |
| PS-032 | MULTI-1 | base | LOCKED |
| PS-033 | SEQUENCER-1 | base | LOCKED |
| PS-034 | SYNC-1 | base | LOCKED |
| PS-035 | ASSET-D1 | base | LOCKED |
| PS-036 | ASSET-D1 | asset-meaning | OPEN-DECISION |
| PS-037 | VALIDATOR-ECON-D1 | base | LOCKED |
| PS-038 | ASSET-1 | base | LOCKED |
| PS-039 | RESOURCE-1 | base | LOCKED |
| PS-040 | ECONOMIC-STATE-1 | base | LOCKED |
| PS-041 | VALIDATOR-ECON-1 | base | LOCKED |
| PS-042 | RULE-1 | base | LOCKED |
| PS-043 | RULE-1 | rule-choice | OPEN-DECISION |
| PS-044 | LANGUAGE-R1 | base | READY |
| PS-045 | LANGUAGE-R1 | grammar-choice | OPEN-DECISION |
| PS-046 | VM-D1 | base | LOCKED |
| PS-047 | VM-1 | base | LOCKED |
| PS-048 | PROGRAM-STATE-1 | base | LOCKED |
| PS-049 | EMERGENCE-R1 | base | LOCKED |
| PS-050 | PROGRAM-CONSENSUS | base | LOCKED |
| PS-051 | SCALE-1 | base | READY |
| PS-052 | SCALE-2 | base | LOCKED |
| PS-053 | CONSENSUS-D1 | base | LOCKED |
| PS-054 | CONSENSUS-D1 | mechanism-choice | OPEN-DECISION |
| PS-055 | VALIDATOR-SCHEDULE-1 | base | LOCKED |
| PS-056 | CONSENSUS-1 | base | LOCKED |
| PS-057 | CLIENT-1 | base | LOCKED |
| PS-058 | TESTNET-1 | base | LOCKED |
| PS-059 | L1-CANDIDATE | base | LOCKED |
| PS-060 | L1-PRODUCTION | base | LOCKED |
| PS-061 | VALUE-1 | base | LOCKED |
| PS-062 | COST-1 | base | READY |
| PS-063 | COST-1 | budget | OPEN-DECISION |
| PS-064 | CODEC-1 | base | READY |
| PS-065 | EXECUTOR-1 | base | LOCKED |
| PS-066 | THREAT-D0 | base | READY |
| PS-067 | THREAT-1 | base | LOCKED |
| PS-068 | CRYPTO-D1 | base | LOCKED |
| PS-069 | CRYPTO-D1 | primitive-selection | OPEN-DECISION |
| PS-070 | CRYPTO-1 | base | LOCKED |
| PS-071 | HASH-SOURCE-R1 | base | READY |
| PS-072 | HASH-SOURCE-R1 | candidate | NOT-STARTED |
| PS-073 | HASH-R1 | base | LOCKED |
| PS-074 | BOUNDS-1 | base | LOCKED |
| PS-075 | GOVERNANCE-D1 | base | LOCKED |
| PS-076 | GOVERNANCE-D1 | authority-choice | OPEN-DECISION |
| PS-077 | PROFILE-SCOPE-D0 | base | LOCKED |
| PS-078 | GENESIS-D1 | base | LOCKED |
| PS-079 | PROFILE-D1 | base | LOCKED |
| PS-080 | TRANSITION-SPEC-1 | base | LOCKED |
| PS-081 | GOVERNANCE-1 | base | LOCKED |
| PS-082 | UPGRADE-FRAMEWORK-1 | base | LOCKED |
| PS-083 | UPGRADE-1 | base | LOCKED |
| PS-084 | PROFILE-1 | base | LOCKED |
| PS-085 | SIGNER-OPS-1 | base | LOCKED |
| PS-086 | RELEASE-R1 | base | READY |
| PS-087 | RELEASE-R1 | framework | READY |
| PS-088 | RELEASE-R1 | candidate | LOCKED |
| PS-089 | RELEASE-1 | base | LOCKED |
| PS-090 | RELEASE-1 | admitted | LOCKED |
| PS-091 | CHAIN-LAUNCH-1 | base | LOCKED |
| PS-092 | SUCCESSOR-ACTIVATION-1 | base | LOCKED |
| PS-093 | SUCCESSOR-ACTIVATION-1 | activation | LOCKED |
| PS-094 | PROD-OPS-1 | base | LOCKED |
| PS-095 | SYNC-1 | replay-authority | LOCKED |

## Structured phase-dependency manifest

Unconditional `Depends on` rows are inherited by every phase of their owner. The following rows add
only concrete phase-to-phase prerequisites that must not be collapsed into base-node edges. The
checker row regex is ASCII with exact single spaces and delimiters:

```regex
^\| (PD-[0-9]{2}) \| ([A-Z0-9]+(?:-[A-Z0-9]+)*) \| ([a-z0-9]+(?:-[a-z0-9]+)*) \| ([A-Z0-9]+(?:-[A-Z0-9]+)*) \| ([a-z0-9]+(?:-[a-z0-9]+)*) \|$
```

Phase-dependency IDs and complete owner/prerequisite tuples are unique. Both endpoint phases MUST
exist in the phase-status manifest. A same-node edge is valid only between distinct phases; the
phase graph, including inherited base prerequisites, MUST remain acyclic. Conditional profile
selection belongs only to the CE manifest and MUST NOT be duplicated here.

| Phase dependency | Owner node | Owner phase | Prerequisite node | Prerequisite phase |
| --- | --- | --- | --- | --- |
| PD-01 | REFINE-1 | accounting | ACCOUNT-1 | base |
| PD-02 | REFINE-1 | commitment | COMMIT-1 | base |
| PD-03 | REFINE-1 | vm | VM-1 | base |
| PD-04 | RELEASE-R1 | candidate | RELEASE-R1 | framework |
| PD-05 | RELEASE-R1 | candidate | PROFILE-D1 | base |
| PD-06 | SYNC-1 | replay-authority | CONSENSUS-1 | base |
| PD-07 | TESTNET-1 | base | SYNC-1 | replay-authority |

“Ready” is not “recommended regardless of context.” Bitblock still works in one visible semantic
checkpoint at a time. Research that does not alter live authority can run beside that checkpoint,
but it must not be wired into the runtime until its own gate and prerequisites pass.

An activated checkpoint inherits the transitive closure of its prerequisites. Optional branches
stay optional until a released profile activates them: a base occupancy network need not implement
visual programs or a market asset, but a profile that executes visual programs must also satisfy
their parser, VM, resource, refinement, upgrade, and security nodes. A node with several phases
must report each phase separately; an early research artifact cannot lend **IMPLEMENTED** status to
its later authority or operations phase.

Activation also fixes domain presence. Every candidate profile binds one pre-ID
`DomainRegistryV1`, a canonically ordered list with unique `domain_tag` values.
`RequiredDomainV1(domain_tag, composition_layer, root_type, tree_domain, root_schema_id,
commitment_profile_id)` is the complete six-field active-domain identity; each registry entry is
exactly `ActiveDomainV1(required_domain)` or `InactiveDomainV1(domain_tag, composition_layer,
inactive_sentinel_schema_id)`. Neither variant contains the later network `profile_id`, and
`required_domain_set` is exactly the canonically sorted set of those projections. A composer
includes an active root or the typed inactive sentinel in every registry-fixed slot; an empty root,
zero bytes, a stale prior-profile root, omission, or an unknown domain is not an inactive encoding.
Proofs for inactive domains reject. `ProfileScopeV1` contains the registry before any derived ID;
genesis and candidate bytes project its protocol-hash `domain_registry_digest`, and the later
network `CommitmentSubjectV1` binds `profile_id`, `profile_scope_digest`, and that same registry
digest. `CommitmentSetV1`, the execution-root preimage, release metadata, and admission evidence
must resolve and reproduce those same bytes and identities.

Once a schema/profile instance has an artifact, expanding nodes—especially `COST-1`, `CODEC-1`,
`EXECUTOR-1`, `THREAT-1`, `BOUNDS-1`, `RELEASE-R1`, `RELEASE-1`, and the crypto/operations nodes—record it as
`node@profile-or-schema`, with exact artifact and commit identities. Before an instance exists, the
node must say which framework/design phase is READY and which authority instance remains LOCKED;
it must not invent a profile label merely to satisfy this convention. Passing a local-v1 corpus
does not give a later network, program, asset, or migration profile the same status.

## The tree at a glance

This compact view is a selected scheduling projection, not an exhaustive edge manifest. The
unconditional `Depends on` fields and the structured conditional-instance edge manifest below are
jointly authoritative. `A + B -> C` means the shown inputs are required, but other direct or
profile-conditional prerequisites may be omitted here for readability.

```text
FOUNDATION (current)
  WORLD-0  binary sparse world + immutable origin
  OBSERVE-0 camera observer + collision + pixel-to-screen depth
  TICK-0   deterministic fixed-tick reducer
  REPLAY-0 bounded edit/tape/headless replay surfaces
  PROOF-0  bounded Lean/LadderLang and canonical-tag evidence

LOCAL AUTHORITY TRUNK
  TICK-0 + REPLAY-0 -> ACCOUNT-1
  ACCOUNT-1 + COST-1 -> LOCAL-BOUNDS-1 -> STORAGE-1 -> LEDGER-1
  LEDGER-1 + ACCOUNT-1 -> TRANSITION-SPEC-D1
  WORLD-0 + PROOF-0 -> COMMIT-R1
  WORLD-0 + PROOF-0 -> VERKLE-R1 feasibility (separate research)
  LEDGER-1 + TRANSITION-SPEC-D1 + COMMIT-R1 + CODEC-1 -> COMMIT-1

PRE-GENESIS DESIGN AND IDENTITY
  PROOF-0 -> CODEC-1
  FOUNDATION -> THREAT-D0 (COST-1 supplies measurement method, not readiness)
  CODEC-1 + THREAT-D0 -> CRYPTO-D1
  ACCOUNT-1 + CODEC-1 + THREAT-D0 + CRYPTO-D1 -> IDENTITY-D1
  THREAT-D0 + CRYPTO-D1 -> VALIDATOR-ECON-D1 + CONSENSUS-D1 + GOVERNANCE-D1
  TRANSITION-SPEC-D1 + IDENTITY-D1 + CONSENSUS-D1 + GOVERNANCE-D1
      + COMMIT-R1 -> PROFILE-SCOPE-D0
  PROFILE-SCOPE-D0 + measured COST-1 -> THREAT-1 -> BOUNDS-1
  PROFILE-SCOPE-D0 + BOUNDS-1 -> GENESIS-D1 -> PROTOCOL-ID-1 -> PROFILE-D1
  PROFILE-D1 + BOUNDS-1 + selected design extensions -> TRANSITION-SPEC-1

IDENTITY AND SHARED ORDER
  CRYPTO-D1 + IDENTITY-D1 + PROTOCOL-ID-1 -> CRYPTO-1
  IDENTITY-D1 + PROTOCOL-ID-1 + CRYPTO-1 -> IDENTITY-1
  IDENTITY-1 + COMMIT-1 -> AUTH-COMMIT-1
  TRANSITION-SPEC-1 + GOVERNANCE-D1 -> GOVERNANCE-1
  TRANSITION-SPEC-1 + CONSENSUS-D1 + GOVERNANCE-1 -> VALIDATOR-SCHEDULE-1
  TRANSITION-SPEC-1 + PROFILE-D1 + BOUNDS-1 + activated producers -> EXECUTOR-1
  AUTH-COMMIT-1 + VALIDATOR-SCHEDULE-1 + EXECUTOR-1 + PROFILE-D1
      -> EXECUTION-ROOT-1 -> GENESIS-ROOT-PARITY-1 -> MULTI-1
  MULTI-1 + BOUNDS-1 + EXECUTOR-1 + EXECUTION-ROOT-1 -> SEQUENCER-1
  SEQUENCER-1 + STORAGE-1 + BOUNDS-1 -> SYNC-1
  SYNC-1 + CONSENSUS-D1 + THREAT-1 + CRYPTO-1
      + VALIDATOR-SCHEDULE-1 + EXECUTION-ROOT-1 -> CONSENSUS-1
  CONSENSUS-1 + EXECUTION-ROOT-1 + PROTOCOL-ID-1 -> CLIENT-1
  PROFILE-SCOPE-D0 + GENESIS-D1 + PROFILE-D1 + TRANSITION-SPEC-1
      + PROFILE-1 + GENESIS-ROOT-PARITY-1 + RELEASE-1 + launch authority
      -> CHAIN-LAUNCH-1
  CHAIN-LAUNCH-1 + successor PROFILE-1 + UPGRADE-1 + RELEASE-1
      -> SUCCESSOR-ACTIVATION-1 when in-place-upgrade is selected
  CLIENT-1 + CHAIN-LAUNCH-1 + cross-cutting release/security nodes -> TESTNET-1
  TESTNET-1 conditionally consumes SUCCESSOR-ACTIVATION-1 through CE-44
  TESTNET-1 -> L1-CANDIDATE -> L1-PRODUCTION -> optional VALUE-1

ASSET BRANCH
  LEDGER-1 -> ASSET-D1
  ASSET-D1 + IDENTITY-1 + COMMIT-1 + TRANSITION-SPEC-1 -> ASSET-1
  PROFILE-SCOPE-D0 + BOUNDS-1 -> RESOURCE-1 (asset design only for economic charging)
  VALIDATOR-ECON-D1 + ASSET-1 + RESOURCE-1 + TRANSITION-SPEC-1 -> ECONOMIC-STATE-1
  CONSENSUS-1 + ECONOMIC-STATE-1 + EXECUTION-ROOT-1 -> VALIDATOR-ECON-1

VISUAL LANGUAGE AND EMERGENCE BRANCH
  WORLD-0 + PROOF-0 -> LANGUAGE-R1 exact static spatial grammar research
  ACCOUNT-1 -> RULE-1 one finite deterministic block rule -> EMERGENCE-R1
  LANGUAGE-R1 + RULE-1 + TRANSITION-SPEC-D1 -> VM-D1
  VM-D1 + BOUNDS-1 -> VM-1
  VM-1 + TRANSITION-SPEC-1 + COMMIT-1 -> PROGRAM-STATE-1
  PROGRAM-STATE-1 + REFINE-1 + RESOURCE-1 + EXECUTION-ROOT-1 -> PROGRAM-CONSENSUS

SCALE BRANCH
  WORLD-0 + OBSERVE-0 -> SCALE-1 -> SCALE-2

CROSS-CUTTING BEFORE A PUBLIC L1 CLAIM
  COST-1 cheapness budgets and measurements
  CODEC-1 independent canonical-codec parity
  EXECUTOR-1 independent transition executor
  THREAT-1 quantitative adversary and consensus assumptions
  CRYPTO-D1 primitive/key-encoding selection
  CRYPTO-1 key/signature/hash lifecycle and migration
  HASH-SOURCE-R1 intended ternary-hash source discovery (optional research)
  HASH-R1 intended ternary-hash evaluation (optional research)
  LOCAL-BOUNDS-1 local record/storage/recovery limits
  BOUNDS-1 protocol work, allocation, state-growth, and admission limits
  THREAT-D0 pre-profile adversary/claim envelope
  GOVERNANCE-D1 upgrade/emergency authority decision
  GOVERNANCE-1 implemented governance transitions and state
  PROFILE-SCOPE-D0 pre-ID feature/operation/configuration scope
  GENESIS-D1 non-self-referential genesis configuration
  GENESIS-ROOT-PARITY-1 complete typed-root reconstruction of genesis
  PROFILE-D1 frozen candidate activation manifest
  TRANSITION-SPEC-1 resolved profile transition semantics
  PROFILE-1 admitted activated-feature closure
  UPGRADE-FRAMEWORK-1 version-lifecycle policy and migration verifier
  UPGRADE-1 deterministic version migration and historical verification
  SIGNER-OPS-1 anti-double-sign key custody and restore
  RELEASE-R1 candidate-build reproducibility
  RELEASE-1 admitted-profile signed release integrity
  CHAIN-LAUNCH-1 complete authorized launch closure
  SUCCESSOR-ACTIVATION-1 complete authorized old-to-new profile closure
  PROD-OPS-1 production monitoring, backup, incident, and safe-halt exercises
```

The view shows dependency, not elapsed time. `COMMIT-R1`, `VERKLE-R1`, `REFINE-1`, `LANGUAGE-R1`,
`SCALE-1`, `COST-1`, and `HASH-SOURCE-R1` are research lanes that can produce
evidence without changing live authority. `PROGRAM-CONSENSUS` does not require the asset branch,
and the asset branch does not require visual programming.

The canonical inventory decomposition is projected in `PlanningCheckerSummaryV1`. The retained base
checker derives it from this document; the conditional rows below are edges, not extra capabilities.

## Structured conditional-instance edge manifest

Unconditional base-node prerequisites remain in each node's `Depends on` field. Every conditional
or cross-instance prerequisite is one row below, with one prerequisite per row. The checker row
regex is ASCII with exact single spaces and delimiters:

```regex
^\| (CE-[0-9]{2}) \| ([A-Z0-9]+(?:-[A-Z0-9]+)*) \| ([a-z0-9]+(?:-[a-z0-9]+)*) \| (launch-profile|target-profile) \| ([a-z0-9]+(?:-[a-z0-9]+)*) \| ([A-Z0-9]+(?:-[A-Z0-9]+)*) \| ([a-z0-9]+(?:-[a-z0-9]+)*) \|$
```

`owner-role`, `evaluation-context`, `predicate-token`, and `dependency-role` are lowercase
hyphenated tokens. Edge IDs are unique. Owner and prerequisite bases must exist. Every closure
evaluation receives exactly one `PredicateEnvironmentV1(launch_profile, target_profile)`. For an
initial admission those two slots resolve to the same initial candidate and genesis-owned scope;
`CHAIN-LAUNCH-1` later authorizes that frozen result. For a successor, `launch_profile` continues to
resolve from the immutable authorized initial launch closure while `target_profile` resolves to the
successor candidate/admission being evaluated. A row reads its predicate only from its declared
context. Predicate entailment is applied independently within each context: no truth from the
target context can be copied into the launch context or vice versa.

A profile instantiates exactly the rows whose context-local predicates are true, resolves each role
to one exact artifact/version, and rejects an unresolved context or predicate. The role-aware graph
for every realizable two-context profile closure, plus the union of all structurally compatible role
edges, must be acyclic. Mutually exclusive predicates may not be used to excuse a cycle inside any
realizable closure.

`always` is the reserved protocol predicate that is true for every candidate. It is not a
candidate-controlled token, cannot be omitted or negated, and adds no predicate-entailment row.

Predicate entailments are canonical direct edges too. Transitive closure is derived from this
manifest; prose cannot add, reverse, or remove an entailment:

| Entailment | Antecedent predicate | Entailed predicate |
| --- | --- | --- |
| PE-01 | economic-charging | asset-active |
| PE-02 | stake-consensus | validator-economics |
| PE-03 | stake-consensus | asset-active |
| PE-04 | stake-consensus | economic-charging |
| PE-05 | validator-rewards | validator-economics |
| PE-06 | validator-rewards | asset-active |
| PE-07 | validator-rewards | economic-charging |
| PE-08 | post-genesis-program | program-active |
| PE-09 | post-genesis-program | migration-profile |
| PE-10 | in-place-upgrade | migration-profile |

Roles select artifact instances, not aliases for a base node. In particular, `new-admission` is the
candidate admission being evaluated, `old-admitted-profile` is a previously admitted immutable
`PROFILE-1@old-profile-id`, and `new-candidate-profile` is `PROFILE-D1@new-candidate-id`; the latter
two IDs must differ. Thus CE-29–CE-31 are accepted only when the dependency is already settled in
the role it names. One target-context `verkle-selected` predicate activates the entire CE-35–CE-42
feasibility, prototype-prerequisite, candidate-binding, and admission closure; there is no second
prototype predicate whose omission could weaken selection. CE-42 resolves only the
`VERKLE-R1@prototype` whose CE-40 candidate ID equals the candidate under
`PROFILE-1@new-admission`. CE-43 binds every candidate to its exact current
`PROFILE-SCOPE-D0@candidate-scope`: an initial candidate resolves that role to its genesis-owned
scope, while a successor resolves it to the successor's current scope instance without regenerating
or substituting immutable `GENESIS-D1`. For `in-place-upgrade`, CE-44 makes the testnet consume the
complete `SUCCESSOR-ACTIVATION-1@activation` artifact and CE-33 makes the later L1 candidate consume
its rehearsal evidence instead of accepting a raw migration result as the successor closure.
Collapsing roles to base names would manufacture a cycle or erase those instance distinctions and is
a checker error.

| Edge | Owner base | Owner role | Evaluation context | Predicate | Prerequisite base | Prerequisite role |
| --- | --- | --- | --- | --- | --- | --- |
| CE-01 | PROFILE-SCOPE-D0 | candidate-scope | target-profile | asset-active | ASSET-D1 | design |
| CE-02 | PROFILE-SCOPE-D0 | candidate-scope | target-profile | program-active | VM-D1 | design |
| CE-03 | PROFILE-SCOPE-D0 | candidate-scope | target-profile | validator-economics | VALIDATOR-ECON-D1 | design |
| CE-04 | CONSENSUS-D1 | candidate-design | target-profile | stake-consensus | VALIDATOR-ECON-D1 | design |
| CE-05 | BOUNDS-1 | candidate-bounds | target-profile | program-active | VM-D1 | design |
| CE-06 | GENESIS-D1 | genesis-bytes | launch-profile | asset-active | ASSET-D1 | design |
| CE-07 | GENESIS-D1 | genesis-bytes | launch-profile | program-active | VM-D1 | design |
| CE-08 | GENESIS-D1 | genesis-bytes | launch-profile | economic-charging | RESOURCE-1 | policy |
| CE-09 | GENESIS-D1 | genesis-bytes | launch-profile | validator-economics | VALIDATOR-ECON-D1 | design |
| CE-10 | TRANSITION-SPEC-1 | resolved-spec | target-profile | asset-active | ASSET-D1 | design |
| CE-11 | TRANSITION-SPEC-1 | resolved-spec | target-profile | program-active | VM-D1 | design |
| CE-12 | TRANSITION-SPEC-1 | resolved-spec | target-profile | economic-charging | RESOURCE-1 | policy |
| CE-13 | TRANSITION-SPEC-1 | resolved-spec | target-profile | validator-economics | VALIDATOR-ECON-D1 | design |
| CE-14 | RESOURCE-1 | policy | target-profile | economic-charging | ASSET-D1 | design |
| CE-15 | VALIDATOR-SCHEDULE-1 | implementation | target-profile | stake-consensus | ECONOMIC-STATE-1 | implementation |
| CE-16 | EXECUTOR-1 | profile-executor | target-profile | asset-active | ASSET-1 | implementation |
| CE-17 | EXECUTOR-1 | profile-executor | target-profile | program-active | PROGRAM-STATE-1 | implementation |
| CE-18 | EXECUTOR-1 | profile-executor | target-profile | validator-economics | ECONOMIC-STATE-1 | implementation |
| CE-19 | EXECUTION-ROOT-1 | root-composer | target-profile | asset-active | ASSET-1 | implementation |
| CE-20 | EXECUTION-ROOT-1 | root-composer | target-profile | program-active | PROGRAM-STATE-1 | implementation |
| CE-21 | EXECUTION-ROOT-1 | root-composer | target-profile | validator-economics | ECONOMIC-STATE-1 | implementation |
| CE-22 | GENESIS-ROOT-PARITY-1 | parity | launch-profile | asset-active | ASSET-1 | implementation |
| CE-23 | GENESIS-ROOT-PARITY-1 | parity | launch-profile | program-active | PROGRAM-STATE-1 | implementation |
| CE-24 | GENESIS-ROOT-PARITY-1 | parity | launch-profile | validator-economics | ECONOMIC-STATE-1 | implementation |
| CE-25 | PROFILE-1 | new-admission | target-profile | asset-active | ASSET-1 | implementation |
| CE-26 | PROFILE-1 | new-admission | target-profile | program-active | PROGRAM-CONSENSUS | authority |
| CE-27 | PROFILE-1 | new-admission | target-profile | economic-charging | RESOURCE-1 | policy |
| CE-28 | PROFILE-1 | new-admission | target-profile | validator-economics | VALIDATOR-ECON-1 | authority |
| CE-29 | PROFILE-1 | new-admission | target-profile | migration-profile | UPGRADE-1 | migration |
| CE-30 | UPGRADE-1 | migration | target-profile | migration-profile | PROFILE-1 | old-admitted-profile |
| CE-31 | UPGRADE-1 | migration | target-profile | migration-profile | PROFILE-D1 | new-candidate-profile |
| CE-32 | PROGRAM-CONSENSUS | authority | target-profile | post-genesis-program | UPGRADE-1 | migration |
| CE-33 | L1-CANDIDATE | candidate | target-profile | in-place-upgrade | SUCCESSOR-ACTIVATION-1 | rehearsal |
| CE-34 | VALUE-1 | activation | target-profile | validator-rewards | VALIDATOR-ECON-1 | authority |
| CE-35 | VERKLE-R1 | prototype | target-profile | verkle-selected | CRYPTO-D1 | selected-suite |
| CE-36 | VERKLE-R1 | prototype | target-profile | verkle-selected | CODEC-1 | prototype-codec |
| CE-37 | VERKLE-R1 | prototype | target-profile | verkle-selected | THREAT-1 | profile-model |
| CE-38 | VERKLE-R1 | prototype | target-profile | verkle-selected | BOUNDS-1 | prototype-bounds |
| CE-39 | VERKLE-R1 | prototype | target-profile | verkle-selected | UPGRADE-FRAMEWORK-1 | migration-framework |
| CE-40 | VERKLE-R1 | prototype | target-profile | verkle-selected | PROFILE-D1 | new-candidate-profile |
| CE-41 | PROFILE-SCOPE-D0 | candidate-scope | target-profile | verkle-selected | VERKLE-R1 | feasibility |
| CE-42 | PROFILE-1 | new-admission | target-profile | verkle-selected | VERKLE-R1 | prototype |
| CE-43 | PROFILE-D1 | new-candidate-profile | target-profile | always | PROFILE-SCOPE-D0 | candidate-scope |
| CE-44 | TESTNET-1 | testnet | target-profile | in-place-upgrade | SUCCESSOR-ACTIVATION-1 | activation |

## Foundation nodes

| Node | Status | Prerequisites | Motive | Current artifact | Gate already met | Non-claim |
| --- | --- | --- | --- | --- | --- | --- |
| `WORLD-0` | `PS-001:WORLD-0@base=IMPLEMENTED` | None; substrate root | Make one coordinate carry one binary fact. | Sparse 16³ chunks over signed `i64³`; absent is off; bedrock is permanent. | World invariant tests and current source semantics. | Not mathematically infinite or unbounded in cost. |
| `OBSERVE-0` | `PS-002:OBSERVE-0@base=IMPLEMENTED`, `PS-003:OBSERVE-0@claim-scope=BOUNDED` | `WORLD-0` | Let a person inspect and edit the substrate without an entity system. | Free camera, swept-point collision, crosshair, depth controls, fit, sidebar. | Current renderer/camera tests and bounded display carriers. | Not a body, user identity, physical-display proof, or scalable far-coordinate solution. |
| `TICK-0` | `PS-004:TICK-0@base=IMPLEMENTED` | `WORLD-0`, `OBSERVE-0` | Give every state transition one deterministic clock. | Fixed 60 Hz reducer and ordered tick outcomes; presentation is separate. | Deterministic same-build tape replay for declared inputs. | Not consensus time, wall-clock proof, or a guarantee that every tick rerenders. |
| `REPLAY-0` | `PS-005:REPLAY-0@base=IMPLEMENTED`, `PS-006:REPLAY-0@claim-scope=BOUNDED` | `TICK-0` | Give people and agents a shared action language. | Edit programs, canonical `InteractionTape`, JSON scenario helper, headless reducer. | Parser/codec/limit vectors and local replay paths. | Not a complete recorder of all accepted live OS input or an independently held run receipt. |
| `PROOF-0` | `PS-007:PROOF-0@base=BOUNDED` | `WORLD-0`; individual capsules declare more | Make semantic claims attackable instead of intuitive. | SHA-256 content tags, Lean world model, finite LadderLang transition capsule, research receipts. | The declared finite/model checks exist. | Not Rust refinement, ternary-hash integration, physical truth, ownership, consensus, or L1 security. |
| `DOC-0` | `PS-008:DOC-0@base=IMPLEMENTED` | None; source/document evidence is external | Preserve intent, dependencies, and claim boundaries for successor agents. | Project intent, capability tree, authenticated-state plan, finite plan audit, transcript and handoff links. | Retained planning-doc checker plus committed validation receipt. | Documentation is not implementation or independent audit evidence. |

## First trunk: complete and authenticate local state

### `ACCOUNT-1` — complete action/result accounting

- **Status:** `PS-009:ACCOUNT-1@base=READY`.
- **Depends on:** `TICK-0`, `REPLAY-0`.
- **Motive:** ensure every included action has one visible disposition before authority, programs,
  or money depend on the reducer.
- **Artifact:** one canonical tick record that reconciles submitted, admitted, executed, rejected,
  no-op, camera-only, edit-miss, observer-overlap, control, and idle lanes without inventing OS
  events that were never accepted.
- **Gate:** replay and live adapters produce the same ordered dispositions for the same accepted
  actions; counts and effect commitments reconcile exactly; malformed or over-limit records fail
  closed before mutation.
- **Non-claim:** a complete local action record is not durable custody, authentication, ownership,
  or consensus.

### `LOCAL-BOUNDS-1` — local record and recovery limits

- **Status:** `PS-010:LOCAL-BOUNDS-1@base=LOCKED`.
- **Depends on:** `ACCOUNT-1`, measured `COST-1` facts for the local storage profile.
- **Motive:** prevent local durability from depending on unnamed record, history, recovery, or disk
  limits while avoiding a dependency on the later network profile.
- **Artifact:** numeric limits for canonical tick/ledger record bytes, records per commit, staged
  writes, retained local history, restart scan work, backup/restore work, and local pruning batches,
  with checked preallocation arithmetic and a versioned local-storage profile.
- **Gate:** worst-shaped valid local records and recovery histories fit the predeclared envelope;
  over-limit carriers reject before allocation or mutation; the limits are fixed before fault tests.
- **Non-claim:** local bounds do not cover remote peers, consensus messages, programs, economic
  sufficiency, or production capacity.

### `STORAGE-1` — crash-safe local publication substrate

- **Status:** `PS-011:STORAGE-1@base=LOCKED`, `PS-012:STORAGE-1@research-journals=BOUNDED`.
- **Depends on:** `ACCOUNT-1`, `LOCAL-BOUNDS-1`.
- **Motive:** make later ledger heads and authenticated nodes recover together instead of treating
  in-memory success as durable state.
- **Artifact:** versioned append/record envelopes, exclusive-writer rule, atomic or recoverably
  ordered head publication, checksummed data, fsync policy, rollback detection, retained failure
  evidence, supplied-carrier verification, backup/restore hooks, and bounded pruning entry points.
- **Gate:** process-death, torn-write, corruption, stale-head, rollback, concurrent-writer, disk-full,
  malformed supplied-carrier, independent-decoder, pruning/GC, and restore assays recover one fully
  verified committed prefix or fail closed without inventing a new head or deleting a pinned one.
- **Non-claim:** one-machine durability is not independent custody, network availability,
  Byzantine agreement, or anti-administrator rollback resistance.

### `LEDGER-1` — durable local occupancy-episode ledger

- **Status:** `PS-013:LEDGER-1@base=LOCKED`.
- **Depends on:** `ACCOUNT-1`, `STORAGE-1`; the current feature-gated coordinate-episode prototype is
  bounded input evidence, not a prerequisite node.
- **Motive:** establish the exact local meaning of placement-as-mint and deletion-as-burn before
  adding participants or monetary language.
- **Artifact:** optional authoritative local ledger with crash-safe canonical records for one live
  episode per mutable occupied coordinate, consuming burns, fresh re-mints, bedrock excluded only
  from the mutable occupancy-episode map, no-op/rejection results, recovery, and bounded
  replay/pruning rules. `ASSET-D1` still decides whether origin has a separate locked genesis
  allocation.
- **Gate:** domain equality and bijection between mutable occupancy and the active episode map;
  globally fresh episode identifiers never reuse a live or burned identifier; delete/re-mint,
  origin, no-op/rejection, canonical-carrier, independently supplied replay, pruning-retention,
  restart, and torn-write vectors restore one committed head or fail closed; every consumed episode
  has one burn provenance.
- **Non-claim:** provenance and conservation do not create a transferable coin, market value,
  multiplayer authority, rollback resistance against an administrator, or consensus.

### `TRANSITION-SPEC-D1` — base occupancy/episode transition contract

- **Status:** `PS-014:TRANSITION-SPEC-D1@base=LOCKED`.
- **Depends on:** `ACCOUNT-1`, `LEDGER-1`, `CODEC-1`, `COMMIT-R1` profile definitions.
- **Motive:** freeze the substrate transition and the interface by which later profile extensions
  compose, without making base semantics depend on identity, fees, consensus, or optional features.
- **Artifact:** versioned specification for occupancy/episode genesis, local accepted operations,
  ordering, atomicity, no-op/rejection/admitted-failure, complete base effects/receipts, and one typed
  extension interface declaring operation tags, read/write sets, work formula, disposition slots,
  state-domain ownership, and composition conflicts. It does not fill nonce, fee, governance,
  validator, asset, or program policy slots.
- **Gate:** executable/formal base vectors give every base byte sequence and context one result or
  one rejection; extension-slot collision, undeclared state access, ambiguity, omitted-lane,
  partial-write, wrong-version, and powered semantic mutants fail. Profile-specific semantics remain
  locked until `TRANSITION-SPEC-1` resolves every active slot.
- **Non-claim:** a base contract is not a profile transition specification, implementation,
  refinement proof, root publication, or consensus rule.

### `COMMIT-R1` — authenticated-map research

- **Status:** `PS-015:COMMIT-R1@base=READY`.
- **Depends on:** `WORLD-0`, `PROOF-0` claim discipline.
- **Motive:** compare proof-carrying sparse-state structures before one becomes consensus-critical.
- **Artifact:** the side-by-side fixed-key binary Patricia and 192-level sparse Merkle prototypes,
  vectors, malformed-proof corpus, and measurements specified in
  [`AUTHENTICATED_STATE_PLAN.md`](AUTHENTICATED_STATE_PLAN.md).
- **Gate:** both candidates pass the same logical-map/root/proof/deletion/fork vectors and report
  bounded update, witness, multiproof, storage, and rebuild costs. A winner is selected only by a
  recorded decision with rejected alternatives and migration consequences. “Winner” means only the
  selected structure among these two admitted hash-structure candidates under their frozen assay;
  it neither rejects nor admits `VERKLE-R1` or any future family.
- **Non-claim:** a prototype root does not replace the current `world_tag`, authenticate a block,
  establish finality, make bytes available, or prove the runtime map is correct.

### `VERKLE-R1` — separate Verkle feasibility and prototype lane

- **Status:** `PS-016:VERKLE-R1@base=READY`, `PS-017:VERKLE-R1@feasibility=READY`, `PS-018:VERKLE-R1@candidate-scope=LOCKED`, `PS-019:VERKLE-R1@prototype=LOCKED`.
- **Depends on:** `WORLD-0`, `PROOF-0`.
- **Motive:** evaluate whether a vector-commitment map could materially improve witnesses or updates
  without silently importing trusted setup, field/curve, codec, cryptographic, or migration risk.
- **Artifact:** feasibility report fixing the candidate vector-commitment primitive, setup/custody
  model, field/curve and subgroup rules, coordinate-to-field mapping, commitment/opening codecs,
  update and multiproof model, verifier work, storage/availability obligations, implementation and
  audit maturity, pinned cost workloads, and migration/historical-verification design, plus the
  exact canonical self-excluding vector-commitment profile descriptor bytes and domain-separated
  identifier consumed by a later `DomainRegistryV1`. Neither descriptor nor identifier depends on
  candidate/genesis/profile identifiers or a produced root. The later prototype is a separately
  identified artifact in the same `verkle-selected` closure, after the candidate-profile dependency
  in CE-40. Feasibility reuses the cost-measurement method without inheriting the authenticated-map
  comparison lane or competing in its already bounded two-candidate decision.
- **Gate:** feasibility reports exact security assumptions and measured cost thresholds with rejected
  alternatives. Prototype admission additionally requires the selected crypto suite, independent
  canonical codecs and implementation, subgroup/noncanonical/malformed-proof controls, worst-case
  update/opening/batch/storage bounds, powered controls, and a deterministic old/new-root migration
  plus historical verifier under the exact candidate profile. Failure keeps the prototype locked
  and cannot reopen the two-candidate `COMMIT-R1` decision by rhetoric.
- **Non-claim:** feasibility, shorter-looking witnesses, or a matching prototype does not establish
  cryptographic security, production readiness, selection, or authority.

### `COMMIT-1` — authenticated local state

- **Status:** `PS-020:COMMIT-1@base=LOCKED`.
- **Depends on:** `LEDGER-1`, `TRANSITION-SPEC-D1`, `COMMIT-R1`, `STORAGE-1`, `CODEC-1` at every
  activated commitment schema; each domain also requires a frozen numeric commitment-limit profile.
- **Motive:** support independently verifiable occupancy, absence, active-episode, and provenance
  queries without replaying or hashing the complete live set for every query.
- **Artifact:** context-free separately typed world, active-token, and episode roots; canonical
  membership/non-membership proofs; full-rebuild parity; one immutable `CommitmentSetV1` binding
  the ledger head and all roots, with staged-node then recoverable-head publication beside the
  corresponding ledger head. `COMMIT-R1` validates only the 192-bit world map: `MintIdV1` value
  framing and the still-unfrozen `EventIdV1`/episode profile must each pass their own domain gates
  before those roots are included. Authorization, nonce, and asset roots are later typed
  instantiations, not prerequisites hidden inside this node.
- **Gate:** incremental root equals a full rebuild after every generated transition; same logical
  map has one root regardless of insertion order; insert/delete restores the old world root when
  logical occupancy is restored; every activated key/value domain passes the same applicable
  canonicalization/proof/parity gates; malformed, cross-domain, stale-version, and over-bound
  witnesses reject. Independent projection rebuilds prove active-token keys/values and episode
  status against the exact same ledger head/manifest after every transition and recovery; crash
  assays recover the previous or next complete commitment set, never a mixed tuple.
- **Non-claim:** an authenticated map proves a statement relative to a supplied root, not that the
  root is canonical, finalized, available, honestly executed, or economically meaningful.

### `REFINE-1` — Rust/model correspondence

- **Status:** `PS-021:REFINE-1@base=READY`, `PS-022:REFINE-1@world=READY`, `PS-023:REFINE-1@accounting=LOCKED`, `PS-024:REFINE-1@commitment=LOCKED`, `PS-025:REFINE-1@vm=LOCKED`.
- **Depends on:** `WORLD-0`.
- **Motive:** connect abstract world theorems to the actual chunked implementation.
- **Artifact:** independently checked refinement or exhaustive/symbolic correspondence for
  coordinate splitting, bit indexing, bedrock, edits, iteration, and eventually root updates. PD-01
  through PD-03 add the exact implemented surface only to the accounting, commitment, or VM
  refinement phase respectively.
- **Gate:** named abstraction function plus base/step proof or a precisely bounded exhaustive
  procedure with powered negative controls; no silent appeal from a finite sample to all `i64³`.
- **Non-claim:** a model theorem without the abstraction/refinement link does not certify Rust.

## Identity and multiplayer branch

### `IDENTITY-D1` — actor and authorization schema

- **Status:** `PS-026:IDENTITY-D1@base=LOCKED`.
- **Depends on:** `ACCOUNT-1`, `CODEC-1`, `THREAT-D0`, and primitive/key-encoding choices from
  `CRYPTO-D1`.
- **Motive:** define what a credential and permission mean before genesis identifiers bind them or
  signed actions use them.
- **Artifact:** canonical actor descriptor, credential roles, authorization subjects/actions,
  delegation, threshold policy, rotation/revocation/recovery state, nonce model, and initial
  descriptor-set encoding. Actor identity remains distinct from validator identity and camera state.
- **Gate:** independent codec plus invalid-key, duplicate-descriptor, ambiguous-threshold,
  cross-role, delegation-cycle, rotation/revocation/recovery, and nonce-initialization vectors. The
  schema can be encoded without knowing the derived `genesis_id`.
- **Non-claim:** a schema or descriptor does not authenticate an action, prove human uniqueness, or
  establish ownership or consensus authority.

### `PROTOCOL-ID-1` — genesis and non-circular identifier graph

- **Status:** `PS-027:PROTOCOL-ID-1@base=LOCKED`.
- **Depends on:** `GENESIS-D1`, `IDENTITY-D1`, `CODEC-1`, `COMMIT-R1` root/profile definitions.
- **Motive:** give signatures, transactions, receipts, roots, and later votes one chain/world
  context without objects hashing themselves or candidates selecting their own authority.
- **Artifact:** immutable pre-ID genesis payload from `GENESIS-D1` and derived `genesis_id`;
  chain/world/version domains;
  pre-genesis authorization descriptor encoding; acyclic transaction, body, context, proposal,
  block, receipt, effect, availability, and consensus-evidence identifier graph; one signature-free
  `AcceptedProposalCoreV1` separated from its bounded, typed `AcceptedProposalCredentialSetV1` and
  `AcceptedEvidenceSetV1`; atomic same-wrapper consensus-entailed availability dependencies;
  explicit system transition subjects; and the protocol-frozen `ProfileIdHashV1` trust anchor with
  named profile-owned and protocol-identity compromise boundaries.
- **Gate:** dependency-graph cycle check plus cross-domain, self-reference, field-omission,
  duplicate-field, alternate-body, wrong-parent, and wrong-version mutation corpus. Every
  authority-relevant byte reaches exactly the intended identifier before it can be signed. Two
  valid proposal signatures and split evidence wrappers for one unsigned core normalize without
  extra weight or parent multiplication; same-wrapper availability dependencies insert atomically;
  a genuine distinct-unsigned-core collision rejects and is classified only through the exact
  `AcceptedParentCollisionDispositionV1` projection below. Either class safe-halts and never selects
  one core or treats the event as an ordinary in-place successor.
- **Non-claim:** identifiers and genesis bytes do not establish trusted distribution, valid keys,
  honest execution, consensus, availability, a fair initial allocation, or the cryptographic
  soundness of `ProfileIdHashV1`.

## Exact accepted-parent invariant projections

These narrow manifests are byte-for-byte checker projections of the normative paper schemas. They
do not make arbitrary prose machine-readable and do not prove the cryptographic assumptions.

```text
AcceptedProposalCoreFieldManifestV1 = ExactOrderedFieldManifest(
    1 => accepted_proposal_core_version_u16_be,
    2 => accepted_parent_lookup_id,
    3 => authority_ref_kind_u8,
    4 => canonical_authority_ref_length_u32_be,
    5 => canonical_proposal_header_length_u64_be,
    6 => canonical_availability_payload_length_u64_be,
    7 => canonical_authority_ref_bytes,
    8 => canonical_proposal_header_bytes,
    9 => canonical_availability_payload_bytes
)

AcceptedProposalCoreV1 = FixedPreProfileCanonicalV1(FieldsExactly(AcceptedProposalCoreFieldManifestV1))

AcceptedProposalCoreExcludedFieldManifestV1 = ExactForbiddenFieldManifest(
    canonical_proposer_signature_length_u64_be,
    canonical_proposer_signature_bytes,
    typed_proposal_credential_id,
    proposal_credential_id,
    canonical_consensus_evidence_bytes,
    consensus_evidence_id,
    canonical_availability_assessment_bytes,
    availability_assessment_id
)

AcceptedParentCollisionDispositionV1 = CanonicalTwoRowMap(
    row_1 = (ByteDistinctAcceptedParentLookupPayloadPreimagesSameProfileIdHashV1OutputV1, ProtocolIdentityHashCompromiseV1),
    row_2 = (ByteEqualAcceptedParentLookupPayloadAndBlockIdDistinctValidAcceptedProposalCoresV1, ProfileOwnedAuthorityBindingCompromiseV1)
)
```

The first collision row requires byte-distinct reconstructed lookup-payload preimages under one
`ProfileIdHashV1` output. The second requires a byte-equal lookup payload, including `block_id`, but
distinct otherwise-valid signature-free cores. A core that does not reconstruct its claimed lookup
is invalid input, not a third collision row.

### `IDENTITY-1` — actor identity and authorization

- **Status:** `PS-028:IDENTITY-1@base=LOCKED`.
- **Depends on:** `IDENTITY-D1`, `PROTOCOL-ID-1`, `CRYPTO-1`.
- **Motive:** distinguish who requested an action from which camera submitted it.
- **Artifact:** non-circular canonical signing preimage; chain/genesis/world/version binding;
  canonical keys; nonces; permissions; delegation; rotation, revocation, and recovery; separate
  actor and validator credential domains.
- **Gate:** mutation, replay, cross-chain, cross-world, wrong-suite, duplicate-key, invalid-key,
  stale-nonce, rotation, and recovery vectors all have deterministic dispositions.
- **Non-claim:** a public key proves key control, not one unique human, legal ownership, or
  validator authority.

### `AUTH-COMMIT-1` — authenticated authorization and nonce state

- **Status:** `PS-029:AUTH-COMMIT-1@base=LOCKED`.
- **Depends on:** `IDENTITY-1`, implemented `COMMIT-1`, `CODEC-1` at the authorization/nonce schemas.
- **Motive:** let a verifier prove which descriptor and nonce state governed an admitted action
  without making identity a circular prerequisite of occupancy-root design.
- **Artifact:** separately typed `AuthorizationRootV1` and `NonceRootV1`, canonical values and
  proofs, genesis initialization, atomic updates for rotation/revocation/recovery/nonce effects,
  and composition into the execution-state root. Actor-key width and both value codecs receive
  their own domain-profile gates; the 192-bit world comparison is not inherited as evidence.
- **Gate:** full-rebuild/incremental parity plus absence, stale-nonce, rotation, recovery,
  cross-domain, and atomic-failure vectors; no occupancy or episode proof verifies as an auth proof.
- **Non-claim:** an authenticated authorization map does not prove the key holder is one human or
  that a submitted action was included, executed, or finalized.

### `EXECUTION-ROOT-1` — complete execution-state commitment

- **Status:** `PS-030:EXECUTION-ROOT-1@base=LOCKED`.
- **Depends on:** `TRANSITION-SPEC-1`, `COMMIT-1`, `AUTH-COMMIT-1`, `EXECUTOR-1`, `PROTOCOL-ID-1`,
  `PROFILE-D1`, `CODEC-1`, `GOVERNANCE-1`, `VALIDATOR-SCHEDULE-1`; activated domain producers are
  the target-profile conditional prerequisites in CE-19–CE-21.
- **Motive:** ensure no parameter, governance, validator-schedule, authorization, asset, or
  application state can affect execution while remaining outside the accepted root.
- **Artifact:** a typed application/execution-root composer over the exact profile domain registry:
  base world, episode, authorization, nonce, parameter/governance, and validator-schedule roots plus
  every activated optional asset, program, or economic root. It binds `required_domain_set`, the
  canonical inactive-domain sentinels, `AuthenticatedRootRefV1`, `CommitmentSetV1`, and header/root
  correspondence without reimplementing any domain producer.
- **Gate:** every domain passes independent codec, rebuild/update/proof/malformed/persistence parity;
  typed roots are non-interchangeable; full-state recomposition matches the executor after every
  transition/recovery; omission, an empty/zero/stale substitute for an inactive sentinel, an active
  root in an inactive slot, an unknown domain, chain/genesis/world/version/header mutation, and
  mixed-manifest publication reject.
- **Non-claim:** a complete execution root does not prove valid ordering, consensus status, data
  availability, or correct rendering.

### `GENESIS-ROOT-PARITY-1` — logical-genesis to typed-root parity

- **Status:** `PS-031:GENESIS-ROOT-PARITY-1@base=LOCKED`.
- **Depends on:** `GENESIS-D1`, `PROTOCOL-ID-1`, `PROFILE-D1`, `COMMIT-1`, `AUTH-COMMIT-1`,
  `GOVERNANCE-1`, `VALIDATOR-SCHEDULE-1`, `EXECUTOR-1`, `EXECUTION-ROOT-1`, `CODEC-1`; activated
  launch-domain implementations are the launch-profile conditional prerequisites in CE-22–CE-24.
- **Motive:** keep pre-ID genesis-byte reproducibility separate from the later claim that every
  implemented typed root reconstructs exactly those logical bytes.
- **Artifact:** independently decoded logical genesis state, per-domain full rebuilds, the composed
  execution root, and a parity receipt binding genesis, profile, domain-registry, root-schema, and
  implementation identifiers.
- **Gate:** independent decoders reconstruct one logical state; every active typed domain rebuilds
  exactly its declared genesis state and every inactive domain contributes its canonical sentinel;
  full rebuild, initialized incremental state, executor output, and composed root agree. Omitted,
  duplicated, cross-domain, stale-profile, synthetic-zero, and self-derived-ID mutations reject.
- **Non-claim:** genesis/root parity does not admit the profile, authenticate distribution, establish
  data availability, or prove consensus safety.

### `MULTI-1` — deterministic multi-actor harness

- **Status:** `PS-032:MULTI-1@base=LOCKED`.
- **Depends on:** `AUTH-COMMIT-1`, `TRANSITION-SPEC-1`, `EXECUTOR-1`, `EXECUTION-ROOT-1`,
  `GENESIS-ROOT-PARITY-1`.
- **Motive:** expose ordering, authorization, and same-coordinate conflicts at the cheapest scale.
- **Artifact:** two or more signed action streams folded by one in-process deterministic executor.
- **Gate:** permutations of races, stale anchors, duplicate nonces, delegated rights, no-ops,
  admitted failures, and re-mints produce one specified result or one specified rejection across
  independent runs.
- **Non-claim:** multiple actors in one process are not networking, decentralization, Sybil
  resistance, availability, or consensus.

### `SEQUENCER-1` — accountable centralized multiplayer

- **Status:** `PS-033:SEQUENCER-1@base=LOCKED`.
- **Depends on:** `MULTI-1`, `PROTOCOL-ID-1`, `CODEC-1`, `BOUNDS-1`, `EXECUTOR-1`,
  `EXECUTION-ROOT-1`, and durable `STORAGE-1` publication.
- **Motive:** let remote participants share one signed, replayable world before consensus is added.
- **Artifact:** bounded wire messages; authenticated admission; canonical batch order; append-only
  bodies/receipts; atomic state-root publication; independently replaying clients.
- **Gate:** malicious-input, restart, duplicate, race, withheld-body, receipt-inclusion, and client
  resynchronization tests; every accepted batch reproduces the published root in an independent
  executor.
- **Non-claim:** the sequencer can still censor, reorder, halt, equivocate, lose data, or present
  different histories. Accountability is not decentralization.

### `SYNC-1` — peer synchronization, availability, and recovery

- **Status:** `PS-034:SYNC-1@base=LOCKED`, `PS-095:SYNC-1@replay-authority=LOCKED`.
- **Depends on:** `SEQUENCER-1`, `STORAGE-1`, and the applicable `BOUNDS-1` limits.
- **Motive:** make required bodies, witnesses, and accepted state recoverable from more than one
  process.
- **Artifact:** the base phase supplies role-separated full verifier, archive, light client, and non-
  authority RPC paths; peer discovery; bounded parsing/backpressure; block/body/witness retrieval;
  checkpoint/genesis bootstrap; pruning and recovery policy. It transports and retains opaque bounded
  authority-certificate and occurrence bytes but does not admit or interpret a consensus-native
  certificate. After `CONSENSUS-1@base`, the separate replay-authority phase additionally carries ordered
  `ReplayEvidenceEventV1` values, an authorized `ReplayEvidenceArchiveCheckpointV1`, inclusion and
  prefix-completeness proofs, one replay-wide `ReplayEvidenceViewV1`, occurrence-bound same-wrapper
  witnesses, proof-free `NormalizedAcceptedAuthorityEntryCoreV1` values with recomputed entry IDs
  and sorted `(lookup_id, entry_id)` view root, full-core retention proofs,
  consumer-bound `HistoricalAcceptedParentAuthorityUseWitnessSetV1` values with exact accepted
  consensus-native certificate admissions, pinned raw wrapper-occurrence reconstruction,
  certificate-to-replay commitment proofs, and append-consistency proofs, and a separate later
  `ReplayParentAuthorityStatusProjectionSetV1`.
- **Gate:** the base phase passes multi-peer eclipse, partial-data, corrupt-snapshot, slow-peer,
  restart, pruning, and fresh-node recovery tests under declared retention assumptions without using
  replayed authority as live consensus input. The replay-authority phase independently reconstructs
  the already accepted certificate protocol owned by `CONSENSUS-1`; its controls reject an omitted
  pre-cutoff conflict, post-cutoff insertion, event reorder/duplication, mixed root/policy/view,
  circular or substituted normalized-entry commitments, retained-subset masquerade, synthetic cross-
  wrapper pair, missing purpose/consumer, non-admitted or late certificate, certificate-to-replay
  evidence mismatch, and every partial checkpoint-publication prefix. A replay checkpoint created
  after the consumer is a required positive control because archive order is not live authority.
- **Non-claim:** data retrieval does not prove correct execution, canonical ordering, finality, or
  certificate admission. Replay reconstruction audits accepted authority and never selects it live.

## Asset and economic branch

### `ASSET-D1` — choose what a block asset means

- **Status:** `PS-035:ASSET-D1@base=LOCKED`, `PS-036:ASSET-D1@asset-meaning=OPEN-DECISION`.
- **Depends on:** `LEDGER-1`; threat/resource analysis may force revision.
- **Motive:** prevent one word—“coin”—from hiding incompatible state machines.
- **Artifact:** a decision record choosing and specifying occupancy collectible, coordinate edit
  right, fungible unit, dual-domain design, or no monetary asset; it names rejected alternatives,
  conservation rules, bedrock treatment, transferability, and recovery behavior.
- **Gate:** the chosen meaning maps every mint, burn, re-mint, transfer, failed action, fork, rent,
  and governance event to one atomic outcome without contradictory supply equations.
- **Non-claim:** choosing semantics does not establish demand, price, legal status, fair
  distribution, decentralization, or security.

### `VALIDATOR-ECON-D1` — pre-genesis stake and incentive schema

- **Status:** `PS-037:VALIDATOR-ECON-D1@base=LOCKED`.
- **Depends on:** `ASSET-D1`, `THREAT-D0`, `CRYPTO-D1`, `CODEC-1`.
- **Motive:** define stake, weight, issuance, rewards, penalties, and exits before a consensus
  candidate or genesis payload can treat economic state as authority.
- **Artifact:** pre-genesis-encodable stake/weight source, bonding and unbonding, reward/issuance,
  slashable evidence and penalty, withdrawal/recovery, validator-entry/exit, concentration, and
  long-range assumptions; it declares the state and transition slots later implemented by
  `ECONOMIC-STATE-1` without choosing the consensus mechanism for non-stake profiles.
- **Gate:** supply and weight equations close over genesis, join, exit, reward, slash, inactivity,
  key change, fork, and recovery vectors; evidence lifetime, unbonding horizon, bootstrap trust, and
  stake-origin assumptions are explicit; a validator cannot select or fabricate its own initial
  weight.
- **Non-claim:** an incentive schema is not implemented economic state, adequate incentives,
  consensus safety, fair distribution, Sybil resistance, or proof that the asset has value.

### `ASSET-1` — ownership and transfer

- **Status:** `PS-038:ASSET-1@base=LOCKED`.
- **Depends on:** `ASSET-D1`, `IDENTITY-1`, `AUTH-COMMIT-1`, `COMMIT-1`,
  `TRANSITION-SPEC-1`, `PROFILE-D1`.
- **Motive:** distinguish occupancy, episode identity, and authority to transfer or edit.
- **Artifact:** chosen account/UTXO/object model; ownership/balance/permission roots; atomic
  transfer, mint, burn, recovery, and conflict rules; proof-carrying client receipts.
- **Gate:** conservation, authorization, replay, concurrent transfer, burn-after-transfer,
  reorganization, recovery, and root-correspondence vectors pass independently.
- **Non-claim:** a consistent asset ledger is not yet a cryptocurrency network or safe place for
  production value.

### `RESOURCE-1` — fees, rent, and economic resource policy

- **Status:** `PS-039:RESOURCE-1@base=LOCKED`.
- **Depends on:** `BOUNDS-1`, `PROFILE-SCOPE-D0`; target-profile economic charging adds the
  conditional design edge CE-14, while a non-economic profile may choose deterministic quotas.
- **Motive:** prevent the huge coordinate space and proof/history creation from becoming an
  unpaid obligation or denial-of-service surface.
- **Artifact:** deterministic pricing or quota policy for signatures, codecs, execution, proof
  bytes, writes, witnesses, retained history, and recovery; explicit payer, denomination,
  price-update, failure charging, state-rent/deposit/expiry, and partition semantics if used.
- **Gate:** charges or quotas cover the measured `BOUNDS-1` resource model under the declared
  assumptions; expiry preserves bedrock and atomic world/episode/supply invariants; prices and
  limits are never changed merely to pass a workload.
- **Non-claim:** fees do not guarantee liveness, fair inclusion, price stability, or sustainable
  validator incentives.

### `ECONOMIC-STATE-1` — deterministic stake, reward, and slash state

- **Status:** `PS-040:ECONOMIC-STATE-1@base=LOCKED`.
- **Depends on:** `VALIDATOR-ECON-D1`, `ASSET-1`, `RESOURCE-1`, `TRANSITION-SPEC-1`, `PROFILE-D1`,
  `CODEC-1`, `COMMIT-1`, `BOUNDS-1`.
- **Motive:** produce every economic state transition and commitment before execution-root or
  consensus consumers rely on stake, rewards, or slashing.
- **Artifact:** independently specified and implemented stake, reward/accrual, issuance, slash,
  unbonding, withdrawal, and validator-economic state; canonical codecs and separately typed roots;
  deterministic transition/receipt adapters for the resolved profile.
- **Gate:** full-rebuild/incremental/root-proof parity and independent execution agree across genesis,
  join/exit, reward, slash, inactivity, fork, restart, recovery, and supply/weight conservation
  vectors; malformed evidence, cross-domain roots, hidden issuance, and partial economic writes
  reject before publication.
- **Non-claim:** committed economic state does not show that incentives are sufficient, participants
  are independent, the asset has value, or consensus is safe or live.

### `VALIDATOR-ECON-1` — consensus incentives

- **Status:** `PS-041:VALIDATOR-ECON-1@base=LOCKED`.
- **Depends on:** `CONSENSUS-1`, `VALIDATOR-ECON-D1`, `ECONOMIC-STATE-1`, `EXECUTION-ROOT-1`,
  `THREAT-1`; only activated profiles that use economic validator incentives require this node.
- **Motive:** align validator behavior only after “validator” has an exact protocol meaning.
- **Artifact:** profile-bound incentive analysis and consensus adapter consuming the already committed
  economic roots and receipts; issuance/unbonding/slashing correspondence; attack assumptions and
  monitored loss conditions. It does not produce an execution-root domain.
- **Gate:** safety/liveness arguments and adversarial simulations include bribery, censorship,
  equivocation, griefing, concentration, long-range, and recovery assumptions; every modeled weight,
  reward, and penalty matches an `ECONOMIC-STATE-1` receipt under the accepted execution root.
- **Non-claim:** occupancy scarcity does not automatically secure consensus, and a block token
  must not silently become validator stake.

## Visual-programming branch

### `RULE-1` — one finite autonomous transformation rule

- **Status:** `PS-042:RULE-1@base=LOCKED`, `PS-043:RULE-1@rule-choice=OPEN-DECISION`.
- **Depends on:** `ACCOUNT-1`.
- **Motive:** test the emergence hypothesis with the smallest executable semantics.
- **Artifact:** one opt-in, finite, tick-scoped rule over exact coordinates with declared read/write
  neighborhood, ordering, boundary, conflict, quiescence, and resource behavior.
- **Gate:** base/step vectors, symmetry cases, powered mutants, headless/live parity, and exact
  `(initial state, accepted actions, rule version)` replay.
- **Non-claim:** one rule does not demonstrate universal computation, useful emergence, or a
  programming language.

### `LANGUAGE-R1` — exact spatial grammar research

- **Status:** `PS-044:LANGUAGE-R1@base=READY`, `PS-045:LANGUAGE-R1@grammar-choice=OPEN-DECISION`.
- **Depends on:** `WORLD-0`, `PROOF-0` claim discipline.
- **Motive:** let arrangements of bits denote symbols and instructions without visual ambiguity.
- **Artifact:** canonical coordinate normalization, origin/orientation, framing, namespaces,
  delimiters, symbol table, parse errors, versioning, and canonical source serialization.
- **Gate:** independent parsers agree over valid/invalid corpora; rotations, translations,
  reflections, overlaps, missing delimiters, and visually similar but coordinate-distinct patterns
  have explicit results.
- **Non-claim:** a pattern recognized by the renderer or a human is not canonical program data.

### `VM-D1` — frozen visual-machine semantics

- **Status:** `PS-046:VM-D1@base=LOCKED`.
- **Depends on:** `LANGUAGE-R1`, `RULE-1`, `ACCOUNT-1`, `TRANSITION-SPEC-D1`.
- **Motive:** define the instruction and work surface before either the bounds checker or interpreter
  is asked to validate it.
- **Artifact:** versioned instruction set, parse-to-instruction mapping, integer state semantics,
  read/write sets, ordering/conflicts, scheduler, traps, rollback/commit behavior, randomness input
  if any, and a pre-execution work-accounting formula.
- **Gate:** every opcode/program form has one finite step or deterministic trap in model vectors;
  ambiguous parses, uncharged work, hidden renderer/camera input, and undefined conflict order stop
  the draft before `BOUNDS-1@program` begins.
- **Non-claim:** frozen VM semantics do not implement an interpreter or establish safe resource
  bounds, usefulness, or consensus authority.

### `VM-1` — bounded deterministic interpreter

- **Status:** `PS-047:VM-1@base=LOCKED`.
- **Depends on:** `VM-D1`, `ACCOUNT-1`, `CODEC-1`, and `BOUNDS-1` for the activated language profile.
- **Motive:** turn exact spatial programs into reproducible state transitions.
- **Artifact:** bounded interpreter plus an independent reference executor implementing the frozen
  `VM-D1` semantics, with versioned gas/work, memory, trap, rollback, and commit enforcement.
- **Gate:** differential and adversarial programs agree per step/root; nontermination and resource
  exhaustion stop deterministically; renderer/camera mutations cannot affect execution.
- **Non-claim:** determinism alone does not make a language safe, useful, decentralized, or
  consensus-authoritative.

### `PROGRAM-STATE-1` — deterministic code and program-state commitments

- **Status:** `PS-048:PROGRAM-STATE-1@base=LOCKED`.
- **Depends on:** `VM-1`, `TRANSITION-SPEC-1`, `PROFILE-D1`, `CODEC-1`, `COMMIT-1`, `BOUNDS-1`.
- **Motive:** produce canonical code/program state and roots before the execution-root composer or
  shared-authority layer consumes them.
- **Artifact:** canonical code identity and byte-availability references, program instances and
  storage, separately typed code/program roots, deterministic activation/execution/trap receipts,
  and the resolved transition adapter for the exact language profile.
- **Gate:** independent codecs, parser/interpreter, full-rebuild/incremental roots, proofs, receipts,
  restart, and recovery agree; malformed/unavailable code, stale versions, hidden state, uncharged
  work, cross-domain roots, and partial program writes reject before root publication.
- **Non-claim:** committed program state does not grant shared authority, prove useful behavior,
  establish code availability, or make activation governance or consensus-safe.

### `EMERGENCE-R1` — bounded emergence assays

- **Status:** `PS-049:EMERGENCE-R1@base=LOCKED`.
- **Depends on:** `RULE-1`.
- **Motive:** turn the founding question—whether simple bit/tick rules produce higher-order
  structure—into observations that can fail rather than examples selected for looking interesting.
- **Artifact:** frozen rule versions, initial states, seeds or committed randomness if used, action
  tapes, explicit metrics for persistence/replication/compression/computation or another named
  behavior, null models, negative controls, and retained runs. A richer VM assay is hypothetical
  until it receives its own declared phase and structured dependency row.
- **Gate:** predeclared metrics distinguish the candidate behavior from its controls over the
  declared finite corpus and reproduce under an independent executor; qualitative images remain
  labeled observations rather than substituted for the metric.
- **Non-claim:** a complex-looking pattern does not by itself prove emergence, universal
  computation, intelligence, life, a fractal law, or behavior outside the tested rule/corpus.

### `PROGRAM-CONSENSUS` — programs as shared authority

- **Status:** `PS-050:PROGRAM-CONSENSUS@base=LOCKED`.
- **Depends on:** `PROGRAM-STATE-1`, `REFINE-1`, `PROTOCOL-ID-1`, `AUTH-COMMIT-1`, `EXECUTION-ROOT-1`,
  `BOUNDS-1`, `RESOURCE-1`, `EXECUTOR-1`, `SEQUENCER-1`, `SYNC-1`, `CONSENSUS-1`,
  `UPGRADE-FRAMEWORK-1`, and a `PROFILE-D1` draft that proposes programmability.
- **Motive:** allow spatial programs to affect shared state only when every verifier can reproduce
  and price them.
- **Artifact:** shared-authority activation/version rules, transaction context and root/receipt
  correspondence, migration policy, deterministic randomness source if any, availability contract,
  and refinement evidence consuming the `PROGRAM-STATE-1` roots.
- **Gate:** independent nodes reproduce program effects and resource charges; malformed code,
  nondeterminism, stale versions, and unavailable program bytes fail closed. Genesis activation
  binds the framework policy; target-profile post-genesis activation additionally realizes CE-32
  and requires a concrete old-admitted to new-candidate `UPGRADE-1` result in the profile admission
  matrix.
- **Non-claim:** visual programmability is not required for the base occupancy ledger or an L1.

## Scale branch

### `SCALE-1` — measure before redesign

- **Status:** `PS-051:SCALE-1@base=READY`.
- **Depends on:** `WORLD-0`, `OBSERVE-0`.
- **Motive:** identify the real limits behind “near-infinite” intent.
- **Artifact:** reproducible workloads separating allocated bits, chunks, visible contributions,
  collision candidates, zoom depth, frame dimensions, proof updates, and memory.
- **Gate:** measurements report distributions and hard failure points under pinned builds; every
  proposed index names which measured cost it reduces and what semantic parity it must preserve.
- **Non-claim:** a benchmark sample does not prove unlimited scale or future hardware behavior.

### `SCALE-2` — chunk-relative queries and hierarchical observation

- **Status:** `PS-052:SCALE-2@base=LOCKED`.
- **Depends on:** `SCALE-1` and its parity vectors.
- **Motive:** preserve exact interaction across far coordinates and large occupied worlds without
  scanning every on bit for every query.
- **Artifact:** chunk-relative camera/collision coordinates, spatial query index, hierarchical
  occupancy/LOD summaries, exact top-saturation composition, invalidation rules, and bounded caches.
- **Gate:** optimized and reference paths agree for frames, aim, collision, bounds, and edits over
  adversarial coordinates including negative chunk edges and signed extrema; measured budgets pass.
- **Non-claim:** rendering LOD must not become the authenticated or consensus state representation.

## Consensus and Layer 1 branch

### `CONSENSUS-D1` — pre-genesis consensus and network-role selection

- **Status:** `PS-053:CONSENSUS-D1@base=LOCKED`, `PS-054:CONSENSUS-D1@mechanism-choice=OPEN-DECISION`.
- **Depends on:** `THREAT-D0`, `CRYPTO-D1`, `CODEC-1`, `COST-1` design inputs; a target-profile
  stake-weighted candidate realizes CE-04 before it can be selected.
- **Motive:** freeze the validator/sequencer role schema and initial authority bytes before genesis
  identifies them, without pretending the protocol is implemented or tested.
- **Artifact:** selected candidate mechanism, quantitative design envelope, proposal/vote/certificate
  schemas, initial validator descriptor and weight rules, role/retention model, data-before-vote,
  validator-set transition shape, finality/confirmation vocabulary, and rejected alternatives.
- **Gate:** the selected design has one non-self-referential initial authority encoding and named
  safety/liveness assumptions, quorum rule, network powers, bootstrap trust, and falsifiers; a
  candidate cannot select its own authority.
- **Non-claim:** a frozen consensus design is not a safety proof, running network, finality evidence,
  or adequate incentive model.

### `VALIDATOR-SCHEDULE-1` — deterministic validator schedule state and root

- **Status:** `PS-055:VALIDATOR-SCHEDULE-1@base=LOCKED`.
- **Depends on:** `CONSENSUS-D1`, `GOVERNANCE-1`, `IDENTITY-1`, `AUTH-COMMIT-1`, `PROTOCOL-ID-1`,
  `TRANSITION-SPEC-1`, `PROFILE-D1`, `CODEC-1`, `COMMIT-1`, `BOUNDS-1`; stake-weighted consensus adds
  the target-profile conditional producer edge CE-15.
- **Motive:** make validator membership, weight, epoch, and activation changes ordinary deterministic
  committed state instead of a hidden output of the consensus implementation that later consumes it.
- **Artifact:** canonical validator descriptors and epochs, join/exit/key-change/weight/activation
  transitions, separately typed schedule root and receipts, historical lookup, governance adapter,
  and economic-root input mapping where stake weighting is active.
- **Gate:** independent codec/executor and full-rebuild/incremental/proof parity agree across genesis,
  epoch boundaries, simultaneous changes, stale/conflicting governance actions, restart, recovery,
  and stake-root changes; a proposal/vote cannot fabricate or retroactively select its own schedule.
- **Non-claim:** a committed validator schedule does not prove quorum honesty, decentralization,
  consensus safety/liveness, economic adequacy, signer custody, or data availability.

### `CONSENSUS-1` — select and specify one consensus profile

- **Status:** `PS-056:CONSENSUS-1@base=LOCKED`.
- **Depends on:** `CONSENSUS-D1`, `SYNC-1`, `PROTOCOL-ID-1`, `THREAT-1`, `CRYPTO-1`, `BOUNDS-1`,
  `VALIDATOR-SCHEDULE-1`, `EXECUTION-ROOT-1`.
- **Motive:** replace sequencer trust with an explicit replicated ordering and state-root rule.
- **Artifact:** implementation and evidence for the selected quantitative adversary/network model;
  proposal/vote/certificate domains; fork-choice or deterministic-finality rule; exact consumption of
  committed validator-schedule transitions; data-before-vote; confirmation/finality status; and safe-
  halt and recovery rules. This node also owns live
  `AcceptedConsensusNativeParentAuthorityCertificateV1` issuance/admission: certificate-independent
  consumer contexts, component-total accepted-wrapper receipt commitments, authorization,
  availability, exact consensus position/history, exactly one accepted admission per
  `(parent core, purpose, required position)`, and consumer dependency/ordering rules. It consumes
  live accepted records directly; no replay checkpoint or normalized archive view selects admission.
- **Gate:** formal/model safety argument appropriate to the profile plus a deterministic reference
  harness over adversarial proposal, vote, partition, missing-data, set-change, long-range,
  split-wrapper, arrival-permutation, duplicate, and post-acceptance status-upgrade traces. Certificate
  controls total-decode every committed wrapper, require exact component coverage including conflict
  material, reject duplicate/forked admissions, and distinguish envelope consensus-order chronology
  from release/activation signing dependency. Independent historical reconstruction belongs to
  `SYNC-1@replay-authority`; independent multi-process, restart, and
  operational fault evidence belongs to `TESTNET-1`; sampled/model liveness evidence stays bounded
  to its assumptions.
- **Non-claim:** consensus evidence does not imply correct execution or available bodies unless
  those checks are separately bound into the profile.

### `CLIENT-1` — proof-bound provisional and accepted client state

- **Status:** `PS-057:CLIENT-1@base=LOCKED`.
- **Depends on:** `CONSENSUS-1`, `EXECUTION-ROOT-1`, and receipt-inclusion commitments from
  `PROTOCOL-ID-1`.
- **Motive:** stop wallets and observers from presenting a sequencer acknowledgement, candidate
  receipt, confirmed result, and finalized result as the same fact.
- **Artifact:** typed submitted/admitted/included/confirmed-or-finalized/rejected/replaced states;
  receipt-to-header and state-proof verification; root status; nonce replacement; fork/reorg;
  mint/burn/ownership rollback; expiry and resubmission behavior; light-client trust inputs.
- **Gate:** pre-testnet model/reference vectors for competing candidates, delayed inclusion,
  duplicate/replaced transactions, wrong-root proofs, missing bodies, reorganization,
  validator-set changes, and stale checkpoints always expose the strongest verified status and
  never retain phantom ownership or supply. Independent-process behavior remains for `TESTNET-1`.
- **Non-claim:** a locally displayed “confirmed” label is not stronger than the exact consensus
  evidence and assumptions the client independently verified.

### `TESTNET-1` — replicated adversarial testnet

- **Status:** `PS-058:TESTNET-1@base=LOCKED`.
- **Depends on:** `CLIENT-1`, `PROFILE-1`, `EXECUTOR-1`, `EXECUTION-ROOT-1`,
  `GENESIS-ROOT-PARITY-1`,
  `UPGRADE-FRAMEWORK-1`, `SIGNER-OPS-1`, `RELEASE-1`, `CHAIN-LAUNCH-1`, `GOVERNANCE-1`, `CRYPTO-1`,
  `THREAT-1`, and the activated `BOUNDS-1` profile.
- **Motive:** expose implementation and operating failures across independent Rust processes.
- **Artifact:** the exact authorized `CHAIN-LAUNCH-1` package and closure bundle, validators, full
  verifiers, archive/recovery nodes, fault injector, retained bodies/witnesses/evidence, and public
  coverage results. Every process pins the same launch-manifest and closure-bundle identities. When
  `in-place-upgrade` is active, CE-44 additionally supplies the exact old-to-new successor bundle,
  target admission, authorization, and availability evidence exercised by every verifier.
- **Gate:** the admitted stage×gate-subclaim matrix names every owner/artifact/stop condition and has
  no unresolved conditional: `G5.core` and `G15.resource` pass for every profile, each economic
  subclaim passes when its predicate is true, and each active `G19.<feature>` passes while inactive
  feature subclaims carry an explicit exclusion. Production-only G18 remains owned by `PROD-OPS-1`.
  Genesis-root parity and every testnet-required row pass; a missing launch-closure or successor
  preimage, substituted launch/successor reference, ambient successor registry, or unresolved
  authority, root, availability, predicate, migration, admission, or release-binding fault halts the
  candidate before the first accepted record or successor activation.
- **Non-claim:** a testnet, hash agreement, or long fault-free run is not production security.

### `L1-CANDIDATE` — independently reviewable base state machine

- **Status:** `PS-059:L1-CANDIDATE@base=LOCKED`.
- **Depends on:** `TESTNET-1`, `PROFILE-1`, `UPGRADE-FRAMEWORK-1`, `RELEASE-1`, `PROD-OPS-1`, and the
  complete activated-profile coverage matrix; target-profile in-place upgrade rehearsal is the
  conditional edge CE-33.
- **Motive:** make the strongest possible claim reviewable before real value depends on it.
- **Artifact:** frozen schemas, reproducible builds, independent executor, the exact authorized
  `CHAIN-LAUNCH-1` package consumed by `TESTNET-1`, threat model, consensus proof/evidence, DA and
  recovery contracts, upgrade rules, operations manual, audits, and dispositioned findings. An
  in-place-upgrade profile also retains the exact CE-33 successor-activation rehearsal artifact.
- **Gate:** every public property maps to a version, assumption, artifact, result, and loss
  condition; the candidate evidence index reproduces the testnet launch manifest, bundle inventory,
  and authorization without omission or substitution; independent review finds no unresolved
  critical issue; migration and incident drills preserve stated invariants.
- **Non-claim:** candidate status is not an unqualified assertion of security or fitness for
  production value.

### `L1-PRODUCTION` — scoped production security claim

- **Status:** `PS-060:L1-PRODUCTION@base=LOCKED`.
- **Depends on:** `L1-CANDIDATE`, `PROD-OPS-1`, external review, and matched deployed artifacts.
  Real monetary value is a separate optional activation.
- **Motive:** state exactly what the deployed base layer guarantees and under which assumptions.
- **Artifact:** signed release/genesis/profile/chain-launch identifiers, the reviewed authorized
  launch-closure bundle, live validator and availability assumptions, incident channels, monitored
  bounds, archival commitments, and public exclusions.
- **Gate:** the deployed binary, configuration, and launch closure match the reviewed artifacts;
  ongoing monitoring, backup, recovery, key-compromise, and upgrade exercises meet the published
  contract.
- **Non-claim:** “Layer 1” never means universal safety, guaranteed value, one-human-one-account,
  privacy, fair ordering, physical-display truth, or protection beyond the named model.

### `VALUE-1` — optional production-value activation

- **Status:** `PS-061:VALUE-1@base=LOCKED`.
- **Depends on:** `L1-PRODUCTION`, `ASSET-1`, `RESOURCE-1`, independent economic/security/legal
  review, and an explicit human decision; target-profile validator rewards realize CE-34.
- **Motive:** keep launching secure replication separate from asking people to assign or risk real
  value.
- **Artifact:** finalized issuance/distribution and custody policy, user risk disclosures,
  jurisdiction-specific review, market/oracle/bridge exclusions or contracts, incident authority,
  and monitored economic assumptions.
- **Gate:** every value-facing claim maps to the deployed version and evidence; no unresolved
  critical security, custody, supply, bridge/oracle, governance, or operational finding remains.
- **Non-claim:** activation cannot prove demand, stable price, profit, legal classification,
  fairness, or freedom from loss.

## Cross-cutting security nodes

### `COST-1` — cheapness budgets and measurements

- **Status:** `PS-062:COST-1@base=READY`, `PS-063:COST-1@budget=OPEN-DECISION`.
- **Depends on:** `WORLD-0`, `OBSERVE-0`, `TICK-0`, `REPLAY-0`, `PROOF-0`; repeats for every
  activated feature.
- **Motive:** turn “as cheap as possible” into separate, attackable resource claims.
- **Artifact:** pinned measurements and proposed budgets for default binary/dependency size,
  touched-state memory, tick work, renderer/collision work, commitment updates, proof verification,
  bandwidth, database writes, retained history, recovery, and idle overhead.
- **Gate:** every reported number identifies build, workload, machine where relevant, deterministic
  counters where possible, distribution/failure point, and powered control; budgets are fixed before
  optimizing against them.
- **Non-claim:** one machine, workload, average, or sparse-allocation result does not prove universal
  cheapness or near-infinite scale.

### `CODEC-1` — independent canonical-codec parity

- **Status:** `PS-064:CODEC-1@base=READY`.
- **Depends on:** `PROOF-0` framing discipline and a frozen schema.
- **Motive:** ensure “the same bytes” does not mean only that one encoder can decode itself.
- **Artifact:** language-neutral positive/negative corpus plus an independent implementation for
  every activated key, value, transaction, block, receipt, proof, vote, and migration codec.
- **Gate:** exact byte/root parity; alternate bytes, unknown/duplicate fields, truncation, overlong
  lengths, overflow, noncanonical order, and cross-domain/version replay reject before allocation or
  mutation.
- **Non-claim:** same-code round trips or matching valid vectors alone are not independent parity.

### `EXECUTOR-1` — independent transition executor

- **Status:** `PS-065:EXECUTOR-1@base=LOCKED`.
- **Depends on:** `TRANSITION-SPEC-1`, `PROFILE-D1`, `BOUNDS-1`, `ACCOUNT-1`, `CODEC-1`, `COMMIT-1`,
  `PROTOCOL-ID-1`, `AUTH-COMMIT-1`, `GOVERNANCE-1`, `VALIDATOR-SCHEDULE-1`; activated state producers
  are the target-profile conditional prerequisites in CE-16–CE-18.
- **Motive:** detect a shared implementation bug that same-code replay would reproduce perfectly.
- **Artifact:** independently implemented executor for the exact resolved profile transition spec,
  with no shared authority-path code, producing ordered effects, receipts, per-domain updates, and
  the domain-registry inputs consumed by the root composer. Each later upgrade adds its own
  independently implemented migration extension under `UPGRADE-1`.
- **Gate:** each executor first passes the frozen specification vectors and powered semantic mutants;
  then differential corpora, generated sequences, failure/no-op paths, and fork contexts agree or
  fail at a named boundary per step for the pinned profile. Every activated extension is present,
  every inactive extension is rejected, and unresolved predicates cannot enter the executable.
- **Non-claim:** two wrappers, binaries, or processes around the same reducer are not independent.

### `THREAT-D0` — pre-profile adversary and claim envelope

- **Status:** `PS-066:THREAT-D0@base=READY`.
- **Depends on:** implemented `WORLD-0`, `OBSERVE-0`, `TICK-0`, `REPLAY-0`, plus bounded `PROOF-0`
  claim discipline.
- **Motive:** enumerate attacker powers, public properties, parameter slots, and loss conditions
  before crypto, consensus, or genesis choices narrow the candidate.
- **Artifact:** versioned threat taxonomy and candidate quantitative ranges for corruption, network,
  eclipse, key compromise, denial of service, censorship, supply-chain, bridge/oracle, bootstrap,
  and operational powers, including the measurements still required. `COST-1` supplies a measurement
  methodology and later facts; it is not a falsely completed prerequisite for drafting the envelope.
- **Gate:** every proposed property has an attacker action, assumption slot, loss condition, and
  candidate mitigation; unquantified entries remain visibly open and cannot support profile status.
- **Non-claim:** an envelope is not the profile's measured adversary model or a security proof.

### `THREAT-1` — quantitative adversary model

- **Status:** `PS-067:THREAT-1@base=LOCKED`.
- **Depends on:** `THREAT-D0`, `PROFILE-SCOPE-D0`, measured `COST-1` facts.
- **Motive:** make “secure” falsifiable against declared attacker powers and time horizons.
- **Artifact:** static/adaptive corruption, network scheduling/partition/eclipsing, validator weight,
  key compromise, client bootstrap, economic budget, denial-of-service capacity, censorship,
  bridge/oracle, supply-chain, and operational powers mapped to properties and mitigations.
- **Gate:** every public property names assumptions, loss condition, detection/recovery behavior,
  excluded attacks, and the gate/artifact that bears on it.
- **Non-claim:** a list of attack names without thresholds, powers, or failure conditions is not an
  adversary model.

### `CRYPTO-D1` — primitive and key-encoding selection

- **Status:** `PS-068:CRYPTO-D1@base=LOCKED`, `PS-069:CRYPTO-D1@primitive-selection=OPEN-DECISION`.
- **Depends on:** `CODEC-1`, `THREAT-D0`.
- **Motive:** freeze the primitive, suite identifier, key bytes, validation, and signing/hash domain
  assumptions needed to define identity without also pretending the operational lifecycle exists.
- **Artifact:** selected signature/hash candidates, canonical key encodings and validation,
  signature encoding, suite/domain identifiers, signing-preimage framing, baseline cryptographic
  assumptions, implementation choices, and external analysis plan.
- **Gate:** invalid/weak/noncanonical key, wrong-suite/domain, signature malleability, parser,
  cross-protocol replay, and primitive-level vectors plus an explicit rejected-alternative record.
- **Non-claim:** a sound primitive does not secure its codec, random generation, keys, operators,
  binary, aggregation, or migration.

### `CRYPTO-1` — cryptographic and key lifecycle

- **Status:** `PS-070:CRYPTO-1@base=LOCKED`.
- **Depends on:** `CRYPTO-D1`, `IDENTITY-D1`, `PROTOCOL-ID-1`.
- **Motive:** turn selected primitive/key bytes into operated, role-separated actor and validator
  credentials while keeping custody and migration requirements visible.
- **Artifact:** entropy/generation, custody, role separation, aggregation/batch checks,
  rotation/revocation/recovery, compromise response, downgrade rules, external review, and explicit
  old/new-profile requirements for `UPGRADE-1`.
- **Gate:** cross-role, aggregation, replay, compromise, backup/restore, rotation/revocation,
  migration-requirement, downgrade, and operational mutation vectors plus independent review.
- **Non-claim:** a complete key lifecycle does not establish honest actors, human uniqueness,
  consensus safety, or correct execution.

### `HASH-SOURCE-R1` — intended ternary-hash source discovery

- **Status:** `PS-071:HASH-SOURCE-R1@base=READY`, `PS-072:HASH-SOURCE-R1@candidate=NOT-STARTED`.
- **Depends on:** none beyond access to the exact artifact the user intends.
- **Motive:** distinguish the user's actual balanced-trit construction from similarly named but
  already disqualified Aethor TSH/PDH or IterHash designs.
- **Artifact:** exact path/repository/commit, provenance, author-intended name and use case, build or
  reading entry point, and an immutable source digest.
- **Gate:** the artifact can be pinned and its claimed contract can be quoted from its own source or
  accompanying specification; resemblance is not accepted as identity.
- **Non-claim:** locating source says nothing about correctness or cryptographic security.

### `HASH-R1` — intended ternary-hash evaluation

- **Status:** `PS-073:HASH-R1@base=LOCKED`.
- **Depends on:** `HASH-SOURCE-R1`, `CODEC-1`, `THREAT-1`, and a declared use case.
- **Motive:** evaluate the intended balanced-trit construction without replacing the implemented
  SHA-256 suite by resemblance or assumption.
- **Artifact:** pinned source and provenance, exact input/output/framing/domain contract, reference
  vectors, independent implementation, collision/preimage/length-extension or relevant-property
  analysis, cost comparison, and migration plan if it survives.
- **Gate:** the exact named construction—not examined Aethor TSH/PDH or IterHash substitutes—passes
  its declared security and interoperability criteria with powered collision/framing controls.
- **Non-claim:** ternary representation, deterministic output, or two matching implementations do
  not establish cryptographic security or authority.

### `BOUNDS-1` — protocol admission and state-growth bounds

- **Status:** `PS-074:BOUNDS-1@base=LOCKED`.
- **Depends on:** `PROFILE-SCOPE-D0`, `THREAT-1`, `LOCAL-BOUNDS-1`, `ACCOUNT-1`, `CODEC-1`, and the
  common proof shapes frozen by `COMMIT-R1`; target-profile program scope realizes the design edge
  CE-05, not a dependency on the later interpreter implementation.
- **Motive:** make every accepted remote input and retained obligation processable under a committed
  deterministic limit before fees or consensus rely on it.
- **Artifact:** checked maxima and pre-mutation charging for payloads, batches, signatures, keys,
  proofs, nodes, hashes, allocations, execution, program gas, state writes, retained roots/history,
  sync, recovery, and per-peer work.
- **Gate:** worst-shaped accepted inputs stay within the predeclared work/allocation/storage
  envelope; over-bound inputs reject before materialization; a consensus-accepted artifact that
  exceeds the already committed bound triggers the declared safe-halt path rather than a raised
  limit.
- **Non-claim:** a resource bound does not ensure economic coverage, fair inclusion, chain liveness,
  or adequate hardware decentralization.

### `GOVERNANCE-D1` — upgrade and emergency authority schema

- **Status:** `PS-075:GOVERNANCE-D1@base=LOCKED`, `PS-076:GOVERNANCE-D1@authority-choice=OPEN-DECISION`.
- **Depends on:** `IDENTITY-D1`, `THREAT-D0`, `CRYPTO-D1`.
- **Motive:** name who may activate a migration, halt for a proven authority/root fault, or authorize
  recovery without leaving those powers as an operator convention.
- **Artifact:** pre-genesis-encodable governance subjects and permissions, proposal/approval
  threshold, timelock,
  conflict-of-interest and validator-set rules, emergency scope, safe-halt trigger, restart
  authority, compensation versus forbidden history rewrite, appeal/expiry, and public evidence.
- **Gate:** unauthorized, replayed, conflicting, rushed, censored, compromised-key, stale-set,
  rollback, emergency-overreach, and restart vectors have deterministic outcomes under the threat
  model; no emergency action silently violates finalized/confirmed history or supply invariants.
- **Non-claim:** a governance rule does not make decisions wise, decentralized, legally legitimate,
  uncensorable, or immune to capture.

### `PROFILE-SCOPE-D0` — pre-ID feature and configuration scope

- **Status:** `PS-077:PROFILE-SCOPE-D0@base=LOCKED`.
- **Depends on:** `TRANSITION-SPEC-D1`, `IDENTITY-D1`, `CONSENSUS-D1`, `GOVERNANCE-D1`, `CRYPTO-D1`,
  `COMMIT-R1`, `CODEC-1`, `COST-1`; target-profile conditional design prerequisites are CE-01–CE-03
  and CE-41.
- **Motive:** give bounds and genesis an exact, non-self-referential target before any derived chain,
  genesis, or profile identifier exists.
- **Artifact:** canonical pre-ID scope containing activated operations/schemas, the ordered active-or-
  inactive domain registry and `required_domain_set`, roles, suite and consensus/governance choices,
  initial logical state descriptors, proposed numeric limits, optional features, and stage×gate-
  subclaim applicability with named predicate tokens—explicitly excluding all derived IDs.
- **Gate:** independent encoding and closure expansion produce one scope digest; every field owner is
  named; every inactive domain uses its canonical sentinel; no `genesis_id`, `profile_id`, candidate-
  selected authority, missing operation, unknown feature, omitted domain, or unregistered predicate
  token can enter the preimage.
- **Non-claim:** a scope digest does not validate its proposed limits, derive genesis, implement its
  features, or admit a network profile.

### `GENESIS-D1` — reproducible non-self-referential genesis configuration

- **Status:** `PS-078:GENESIS-D1@base=LOCKED`.
- **Depends on:** `PROFILE-SCOPE-D0`, `BOUNDS-1`, `CODEC-1`, `COMMIT-R1` profile definitions;
  launch-profile activated logical domains add CE-06–CE-09. Successor target predicates never add
  state to this immutable launch artifact.
- **Motive:** freeze every initial logical state and authority byte before deriving the identifier
  that later signatures, roots, and profiles bind.
- **Artifact:** canonical logical genesis payload containing the scope and domain-registry digests,
  chain/world namespace, initial base state and every active optional-domain descriptor, canonical
  inactive-domain entries, authorization descriptors, parameter/governance state, validator
  configuration, suites, and bootstrap/availability policy; it excludes its derived `genesis_id`,
  profile identifier, and all roots derived from those bytes.
- **Gate:** independent codecs reconstruct exactly one logical payload before any ID or root exists;
  every initial logical component and active/inactive domain entry is explicit, and mutation,
  omission, duplicate, alternate-order, unknown-domain, and self-reference tests reject. Root parity
  is deliberately deferred to `GENESIS-ROOT-PARITY-1` and is not evidence for this pre-ID gate.
- **Non-claim:** reproducible bytes do not prove trusted distribution, fair allocation, usable keys,
  available archives, or consensus safety.

### `PROFILE-D1` — ID-bound candidate activated-feature manifest

- **Status:** `PS-079:PROFILE-D1@base=LOCKED`.
- **Depends on:** `GENESIS-D1`, `PROTOCOL-ID-1`, `CODEC-1`, `BOUNDS-1`; target-profile CE-43 always
  binds the exact current candidate-scope instance, and the candidate names rather than implements
  optional features.
- **Motive:** give programs, migrations, economics, and other optional features one immutable target
  without pretending their implementation has already passed.
- **Artifact:** versioned canonical candidate manifest binding its lineage, current pre-ID scope and
  domain-registry digests, derived `genesis_id`, activated schemas/roles/capability nodes, validated
  parameters/limits/suites and
  root/domain schema plus commitment-profile identifiers,
  the ordered active-or-inactive domain registry and `required_domain_set`, transitive prerequisites,
  and a stage×gate-subclaim matrix. Candidate bytes exclude their own `profile_id`; that identifier
  is derived afterward from a domain-separated digest.
- **Gate:** independent codec/parser parity, non-circular identifier graph, no unknown/duplicate
  activation, canonical ordering, and deterministic closure expansion. An initial candidate's
  current-scope role equals the genesis-owned scope; a successor's role resolves to the exact current
  scope named by that successor while `GENESIS-D1` remains the immutable chain-genesis artifact.
  Every stage/gate subclaim has
  exactly one required, conditional-with-predicate, or excluded-with-reason entry: `G5.core` and
  `G15.resource` are always required; economic subclaims are conditional on their named economic
  predicate; each `G19.<feature>` is required exactly when that feature is active and otherwise is
  excluded with reason. Missing implementation evidence is reported but does not make drafting
  circular; unresolved conditionals can exist only in a candidate and cannot pass `PROFILE-1`.
- **Non-claim:** a frozen candidate manifest is not an admitted profile and does not implement or
  approve any feature it names.

### `TRANSITION-SPEC-1` — exact candidate-profile transition specification

- **Status:** `PS-080:TRANSITION-SPEC-1@base=LOCKED`.
- **Depends on:** `TRANSITION-SPEC-D1`, `PROFILE-D1`, `BOUNDS-1`, `IDENTITY-D1`, `CONSENSUS-D1`,
  `GOVERNANCE-D1`, `CODEC-1`; target-profile conditional design/policy inputs are CE-10–CE-13.
- **Motive:** resolve the base extension interface into one exact operation/state/work contract before
  either authoritative or independent execution is implemented.
- **Artifact:** profile-ID-bound total transition specification covering operation tags, preconditions,
  authorization/nonces, ordering/conflicts, read/write domains, work/charging, dispositions, atomic
  effects/receipts, governance and validator-schedule slots, active optional extensions, and explicit
  rejection of inactive or unknown extensions.
- **Gate:** independent executable/formal vectors give every bounded byte sequence and context one
  result or one rejection; the resolved operation/domain/predicate closure equals `PROFILE-D1`, every
  active slot has one owner, no inactive slot is reachable, and collision, omitted-effect, partial-
  write, wrong-version, uncharged-work, ambiguity, and powered semantic mutants fail.
- **Non-claim:** an exact transition specification is not either executor, a commitment/root,
  profile admission, networking, consensus, or proof of implementation correctness.

### `GOVERNANCE-1` — implemented governance and emergency transitions

- **Status:** `PS-081:GOVERNANCE-1@base=LOCKED`.
- **Depends on:** `GOVERNANCE-D1`, `IDENTITY-1`, `AUTH-COMMIT-1`, `PROTOCOL-ID-1`,
  `TRANSITION-SPEC-1`, `CODEC-1`, `BOUNDS-1`.
- **Motive:** turn pre-genesis authority bytes into deterministic, authenticated state transitions
  instead of operator convention.
- **Artifact:** governance and emergency transactions/system transitions, canonical receipts,
  parameter/governance-root updates, timelocks, conflict/expiry rules, safe-halt and restart paths.
- **Gate:** unauthorized, replayed, conflicting, rushed, censored, stale-set, compromised-key,
  rollback, emergency-overreach, and restart vectors match the frozen schema and preserve history,
  supply, and root invariants.
- **Non-claim:** correct governance execution does not make decisions wise, decentralized, fair, or
  immune to capture.

### `UPGRADE-FRAMEWORK-1` — version lifecycle and migration verifier

- **Status:** `PS-082:UPGRADE-FRAMEWORK-1@base=LOCKED`.
- **Depends on:** `PROTOCOL-ID-1`, `CODEC-1`, `COMMIT-1`, `EXECUTOR-1`, `CRYPTO-1`, `GOVERNANCE-1`,
  `PROFILE-D1`.
- **Motive:** make genesis activation and later migration distinct while every profile still has an
  explicit version-lifecycle policy.
- **Artifact:** either an immutable-version policy requiring a new genesis/chain for change, or an
  in-place-upgrade framework with activation authority, old/new profile roles, bounded migration
  interface, downgrade rule, historical verifier, interruption/restart, rollback/safe-halt policy.
  Preventive in-place migration owns still-sound-but-deprecated profile-selected object-hash,
  signature, proof, credential, codec, VM, and state-layout changes only while one exact last-safe
  parent and every live authority binding remain independently sound. An established live-role
  failure is `ProfileOwnedAuthorityBindingCompromiseV1`, not an ordinary migration input; compromise
  of the pre-profile identity hash remains separately excluded.
- **Gate:** genesis rejects fabricated old-profile inputs; the no-upgrade policy rejects changed
  versions as a different chain; the migration framework refuses a target without an admitted old
  profile and new candidate, and powered version/downgrade/history mutants fail. Preventive
  role-by-role suite controls may activate only a fully admitted successor while all old bindings
  remain sound. Collision/forgery, ambiguous-last-safe-parent, failed-role-authorized,
  hypothetical-precommit, post-compromise-precommit, and no-precommit controls all reach exact
  `CurrentEstablishedAuthorityCompromiseDispositionV1 =
  SafeHaltAndSeparatelyTrustedNewGenesisV1`. The reserved
  `PrecommittedIndependentRecoveryEnvelopeV1` is uninhabited in the current profile; a positive
  recovery vector is future-conditional on first defining and activating the complete
  `FutureIndependentRecoveryExtensionV1`. `ProtocolIdentityHashCompromiseV1` remains terminal.
- **Non-claim:** a framework does not prove any concrete migration or authorize an upgrade. Neither
  an in-place successor nor a recovery object selected through a failed live authority binding can
  repair that binding; no independent recovery envelope is currently specified.

### `UPGRADE-1` — deterministic version migration

- **Status:** `PS-083:UPGRADE-1@base=LOCKED`.
- **Depends on:** `UPGRADE-FRAMEWORK-1`, `EXECUTOR-1`, `GOVERNANCE-1`; target-profile CE-30 and CE-31
  bind the already admitted old-profile instance and distinct new candidate-profile instance
  without reversing their polarity.
- **Motive:** prevent a codec, profile-owned object hash, signature/proof suite, VM, or state-layout
  upgrade from silently rewriting authority, supply, bedrock, ownership, or historical verification.
- **Artifact:** `UPGRADE-1@old-profile-id->new-candidate-id`, a frozen migration specification,
  activation authority/timelock, deterministic
  bounded migration function, old/new roots over one logical state, dual-profile window if any,
  downgrade rejection, rollback/safe-halt rule, archival historical verifier, and an independent
  migration extension implemented after the migration specification is frozen.
- **Gate:** old/new-root vectors and full bounded migration replay agree independently; invariant,
  interruption, restart, unauthorized activation, downgrade, and historical-proof tests pass.
- **Non-claim:** governance approval or old-root equality alone does not prove migration correctness.
  This capability covers preventive migration while the old authority bindings remain sound; it does
  not recover an established `ProfileOwnedAuthorityBindingCompromiseV1` or `ProfileIdHashV1`
  compromise through the authority that failed.

### `PROFILE-1` — admitted activated-feature closure

- **Status:** `PS-084:PROFILE-1@base=LOCKED`.
- **Depends on:** `PROFILE-D1`, `TRANSITION-SPEC-1`, `BOUNDS-1`, `UPGRADE-FRAMEWORK-1`,
  `GENESIS-ROOT-PARITY-1`, `GOVERNANCE-1`, `EXECUTION-ROOT-1`; optional exact-version authority and
  implementation prerequisites are target-profile CE-25–CE-29 and CE-42.
- **Motive:** prevent a testnet or release from inheriting gates for the base profile while quietly
  activating an optional surface whose evidence is missing.
- **Artifact:** canonical `ProfileAdmissionManifestV1` plus its protocol-frozen
  `profile_admission_manifest_id`, containing a machine-readable mapping from every activated
  node/version and domain-registry entry to its artifact, commit, result, assumptions, loss
  condition, stage×gate-subclaim disposition, and resolved conditional-edge instance.
- **Gate:** the closure walker rejects every missing, unknown, wrong-version, non-passing, stale, or
  mutually incompatible prerequisite, inactive-domain encoding, and unjustified gate exclusion.
  Every conditional predicate is resolved in its declared launch or target context, its realizable
  role-aware closure and compatible union are acyclic, `G5.core` and `G15.resource` are required,
  economic subclaims follow their target predicates, and every `G19.<feature>` matches target
  activation. A `verkle-selected` admission activates all of CE-35–CE-42 and consumes the passing
  prototype whose CE-40 candidate ID equals the candidate being admitted. A migration
  admission consumes the concrete old-admitted to new-candidate `UPGRADE-1` evidence after verifying
  polarity; `TESTNET-1`, releases, and coverage matrices bind the exact admitted digest.
- **Non-claim:** a passing candidate-bound Verkle prototype is necessary when Verkle is selected but
  is not sufficient by itself for authority. Profile admission does not prove properties outside the
  activated closure or make its assumptions universally true.

### `SIGNER-OPS-1` — validator signer safety

- **Status:** `PS-085:SIGNER-OPS-1@base=LOCKED`.
- **Depends on:** `IDENTITY-1`, selected `CONSENSUS-1` credentials, and `CRYPTO-1`.
- **Motive:** keep restart, failover, and backup from creating equivocation even when the protocol
  rules are correct.
- **Artifact:** role-separated signer boundary, monotonic anti-double-sign state, key ceremony,
  backup/restore and failover procedures, compromise/revocation path, and auditable signing policy.
- **Gate:** crash, stale backup, concurrent replica, rollback, failover, compromise, and disaster
  recovery drills cannot emit conflicting authorized messages or silently regress signer state.
- **Non-claim:** secure signer operation does not establish validator honesty, quorum safety, or
  network availability.

### `RELEASE-R1` — candidate-build reproducibility

- **Status:** `PS-086:RELEASE-R1@base=READY`, `PS-087:RELEASE-R1@framework=READY`, `PS-088:RELEASE-R1@candidate=LOCKED`.
- **Depends on:** None.
- **Motive:** stop an unreviewed compiler, dependency, feature set, binary, or genesis configuration
  from inheriting candidate source-level evidence.
- **Artifact:** dependency lock/policy, pinned toolchain, reproducible independent builds, SBOM or
  equivalent manifest, feature/config/genesis/candidate-profile binding, candidate artifact digest,
  and vulnerability response. Candidate attestations are labeled candidate, not admitted-release
  authority. The reusable framework uses only pinned external Rust/tool/proof inputs. PD-04 and PD-05
  make the candidate phase consume that framework and the exact candidate profile without turning
  either prerequisite into a base edge.
- **Gate:** independent builders reproduce the declared artifact or an explicitly explained
  platform-scoped equivalent; mismatched source, dependency, feature, config, or binary rejects the
  candidate artifact identity.
- **Non-claim:** reproducibility proves candidate artifact correspondence, not admission, release
  authorization, signature custody, or absence of source/toolchain vulnerabilities.

### `RELEASE-1` — admitted-profile signed release

- **Status:** `PS-089:RELEASE-1@base=LOCKED`, `PS-090:RELEASE-1@admitted=LOCKED`.
- **Depends on:** `RELEASE-R1`, `PROFILE-1`, `CRYPTO-1`.
- **Motive:** ensure the binary distributed to testnet or production is the reproducible candidate
  whose exact profile closure was admitted, not merely a similarly configured build.
- **Artifact:** signed release metadata binding source, toolchain/dependency manifest, candidate
  artifact digest, profile-admission-manifest ID, genesis, features/config, platform artifacts,
  signer role/suite, and vulnerability/withdrawal status.
- **Gate:** independent builders reproduce the candidate artifact; the admitted profile closure and
  genesis/root-parity receipt match byte-for-byte; wrong candidate/profile/genesis/platform/suite,
  stale admission, unsigned metadata, unauthorized signer, and signature mutation reject the release
  identity before testnet deployment.
- **Non-claim:** a reproducible signed admitted release does not prove source correctness, toolchain
  integrity, operator honesty, availability, or consensus safety.

### `CHAIN-LAUNCH-1` — complete authorized launch closure

- **Status:** `PS-091:CHAIN-LAUNCH-1@base=LOCKED`.
- **Depends on:** `PROFILE-SCOPE-D0`, `GENESIS-D1`, `PROTOCOL-ID-1`, `PROFILE-D1`,
  `TRANSITION-SPEC-1`, `PROFILE-1`, `EXECUTION-ROOT-1`, `GENESIS-ROOT-PARITY-1`, `RELEASE-1`,
  `IDENTITY-D1`, `GOVERNANCE-D1`, `CRYPTO-D1`, `CRYPTO-1`, `CODEC-1`.
- **Motive:** turn the post-ID launch manifest into one independently retrievable and authorized
  trust anchor instead of leaving bootstrap to reconstruct its bound preimages from an undeclared
  local mapping.
- **Artifact:** `CHAIN-LAUNCH-1@chain-launch-manifest-id`, the canonical `LaunchClosureBundleV1`,
  its `launch_closure_bundle_id`, and `chain_launch_package`: a bounded, complete, uniquely typed
  carrier with one outer genesis package/payload and ID. The bundle is an embedded-only inventory
  for its internal artifacts plus exactly one `LaunchClosureExternalGenesisDependencyNodeV1` bound
  to the exact outer `GenesisPackageV1`. That combined-graph node owns dependency refs but not a
  second copy of the outer genesis bytes; initial `ProfileScopeV1` and `DomainRegistryV1`, initial
  `CandidateProfileV1` and profile ID,
  `TransitionSpecAtProfileV1` and ID, `ProfileAdmissionManifestV1` and ID,
  `GenesisRootParityRecordV1` and initial typed execution-root reference, `RELEASE-1` record and ID,
  `ChainLaunchManifestV1` and ID, and detached `ChainLaunchAuthorizationV1` with the exact launch
  policy, credentials, scheme, and verifier selected before the candidate. Content-address-only
  entries and retrieval locators are not members of this carrier.
- **Gate:** two independent implementations bounded-decode the package, reject unknown, duplicate,
  omitted, over-limit, unavailable, or type-substituted entries, and hash-check every embedded
  preimage. The seven explicit artifacts are the scope, registry, candidate, transition
  specification, admission manifest, parity record, and admitted release. If `S` is the carried
  supplementary-artifact list, `D` the internal dependency-node list, `X` the external genesis node,
  and `G` the exact outer genesis-package bytes, the checked equations include
  `internal_artifact_count_u32 = 7 + |S| = |D|`, `external_root_count_u32 = 1`, and
  `complete_artifact_count_u32 = 8 + |S| = |D| + 1`. Internal-to-internal,
  internal-to-external, and external-to-internal dependency-edge counts are checked separately and
  sum to the complete count; separate internal and external byte counts include `len(G)` exactly
  once. Every declared
  individual preimage/root length equals its exact slice. Bundle schema/counter/length fields,
  dependency-node/descriptor bytes, collection framing, `launch_closure_bundle_id`,
  `ChainLaunchManifestV1`, and launch authorization remain excluded from that byte total.
  Fixed-width counters and length prefixes are streaming-preflighted with checked arithmetic before
  allocation, copying, hashing, nested decode, profile-owned work, or materialization. The verifier
  then validates one combined DAG with actual dependency edges scope -> registry, genesis ->
  scope/registry, and candidate -> genesis before the later specification, admission, parity,
  release, and launch-manifest derivations. A valid prefix is registry rank 0, scope rank 1, external
  genesis rank 2, and candidate rank 3 or higher. Every repeated projection and typed ID matches its
  unique owner; the sole external node byte-equals the outer genesis package and every other external
  artifact rejects; the initial target and launch predicate
  contexts are identical; the detached authorization verifies only under the launch policy already
  committed in genesis. Missing-preimage, wrong-profile/specification/admission/parity/root/release,
  candidate-selected-authority, altered authorization, reversed or omitted genesis edge,
  missing genesis outgoing ref, rank-zero genesis, injected external artifact, outer/node mismatch,
  off-by-one edge/artifact count, arithmetic overflow, double-count, locator-only, and
  manifest/package self-reference mutants fail before bootstrap or the first accepted record.
- **Non-claim:** a complete authorized launch package does not prove archive availability after
  launch, validator honesty, consensus safety, transition correctness, operator custody, or the
  security of its cryptographic primitives.

### `SUCCESSOR-ACTIVATION-1` — authorized successor-profile closure

- **Status:** `PS-092:SUCCESSOR-ACTIVATION-1@base=LOCKED`, `PS-093:SUCCESSOR-ACTIVATION-1@activation=LOCKED`.
- **Depends on:** `CHAIN-LAUNCH-1`, `PROFILE-SCOPE-D0`, `PROFILE-D1`, `TRANSITION-SPEC-1`,
  `PROFILE-1`, `UPGRADE-FRAMEWORK-1`, `UPGRADE-1`, `RELEASE-1`, `SYNC-1`, `EXECUTOR-1`,
  `EXECUTION-ROOT-1`, `CONSENSUS-1`, `GOVERNANCE-1`, `CRYPTO-1`, `CODEC-1`, `BOUNDS-1`.
- **Motive:** let a fresh verifier validate an in-place old-to-new profile transition without an
  ambient artifact registry or a profile label standing in for the closure that label denotes.
- **Artifact:** `SUCCESSOR-ACTIVATION-1@old-profile-id->new-profile-id`, a bounded canonical
  `SuccessorActivationBundleV1` and fixed `successor_activation_bundle_id`, verified through three
  acyclic semantic stages that are neither serialized nor counted. A pre-bundle
  `VerifiedSuccessorPredecessorBaseContextV1` carries distinct `verified_release_parent_eligibility` and
  `verified_activation_parent_eligibility` results. They must byte-equal one shared predecessor
  lookup/core and governance source while retaining separate purpose, policy, predicate, status,
  and evidence. After the release core ID exists,
  `VerifiedSuccessorReleaseAuthorityContextV1` binds its certificate-independent
  `ParentAuthorityCertificateConsumerContextV1` variant and
  accepted release-purpose admission before the release is signed. Only after that admitted release
  completes the bundle and yields its ID may `VerifiedSuccessorActivationAuthorityContextV1` bind the
  distinct activation-purpose context/admission before activation is signed. The successor release
  sign document and activation sign document each carry the exact purpose-specific
  `HistoricalAcceptedParentAuthorityUseRefV1`, including the resolver-selected consensus position,
  exact evidence commitment with its complete
  `ConsensusNativeAcceptedRecordOccurrenceReceiptSetV1` and exact
  `AcceptedParentAuthorityWitnessReceiptCoverageV1`, including every decoded conflict component,
  certificate/authorization/admission IDs,
  and status, before their IDs and authorizations are derived. The
  internal-only bundle carries canonical bytes for the target scope and registry,
  candidate profile, resolved transition specification, exact target `ProfileAdmissionManifestV1`,
  admitted release, every implementation artifact transitively consumed by admission, and the
  concrete `UPGRADE-1` evidence. The migration context and detached activation authorization bind
  both the bundle ID and exact target admission ID. The old profile's envelope availability payload
  carries the complete bundle before migration execution or activation.
- **Gate:** two independent implementations preflight and bounded-decode the old-envelope carrier,
  verify its availability commitment, resolve the predecessor base context, walk the internal DAG
  through the release core, verify the release-purpose certificate and signed admitted release,
  complete every typed internal ID and the bundle ID, then verify the activation-purpose certificate
  and authorization. Any attempt to pre-resolve a certificate from the release-core or bundle ID it
  helps authorize rejects. The complete new-scope/registry -> candidate -> specification ->
  admission/release -> migration closure remains internal. If `S` is the supplementary internal
  sequence and `D` the internal dependency nodes, it checks `7 + |S| = |D|`, internal-only edge and
  byte totals, individual lengths, exclusions, and prefix-first preflight independently of launch's
  external-root grammar. Prior accepted core/evidence/credential, launch/genesis, old-profile/state,
  framing/counter, dependency-node, and predecessor-context bytes are not counted or duplicated;
  every external projection byte-equals the unique predecessor base context. The verifier checks the
  admitted release against the later release-authority context and the activation sign document
  against the post-bundle activation-authority context, including their distinct signed certificate-
  admission refs. The
  parent-profile resolver fixes certificate, consensus, availability, position, status,
  equivocation, and liveness rules; replay checkpoints cannot substitute. Purpose coercion,
  swapping, non-admission, late admission, or one-purpose-only evidence rejects.
  It then checks old/new role polarity and activation authorization under the already admitted governance/crypto
  policy and executes only after closure acceptance. Count/length/edge, external-ID injection,
  context/purpose mismatch, substitution of a non-accepted evidence-subset certificate candidate,
  competing accepted admissions at one required position, duplicate-prior-bytes, ambient-map,
  authorization, and availability mutants reject before migration state changes.
- **Non-claim:** a complete successor bundle does not authorize itself, prove the migration correct,
  make its artifacts durably available after activation, or establish consensus safety.

### `PROD-OPS-1` — production operations and incident safety

- **Status:** `PS-094:PROD-OPS-1@base=LOCKED`.
- **Depends on:** `TESTNET-1`, `STORAGE-1`, `SIGNER-OPS-1`, `RELEASE-1`, and the safe-halt/recovery
  authority selected by `GOVERNANCE-D1`.
- **Motive:** preserve the published claim through ordinary failures, attacks, and human operation.
- **Artifact:** privilege separation, RPC/firewall policy, monitoring and alert thresholds,
  availability/archive commitments, backup/failover, incident classification and communication,
  key-compromise response, safe halt, restart authority, and postmortem evidence retention.
- **Gate:** fault, intrusion, disk, network, data-loss, release-rollback, signer-compromise,
  monitoring-loss, safe-halt, and recovery exercises meet the named detection and recovery bounds
  without bypassing unresolved authority/root faults.
- **Non-claim:** an operations manual or one successful drill does not guarantee future uptime or
  incident-free operation.

## Current unlock frontier

The phase-status manifest is the sole scheduling authority. This table is its complete ordered
projection of every currently **READY** row; it grants only checkpoint eligibility, never
implementation, activation, or profile authority. Each entry must remain byte-equal to its cited
`PS` row.

| Frontier ordinal | Authoritative READY projection |
| ---: | --- |
| 1 | `PS-009:ACCOUNT-1@base=READY` |
| 2 | `PS-015:COMMIT-R1@base=READY` |
| 3 | `PS-016:VERKLE-R1@base=READY` |
| 4 | `PS-017:VERKLE-R1@feasibility=READY` |
| 5 | `PS-021:REFINE-1@base=READY` |
| 6 | `PS-022:REFINE-1@world=READY` |
| 7 | `PS-044:LANGUAGE-R1@base=READY` |
| 8 | `PS-051:SCALE-1@base=READY` |
| 9 | `PS-062:COST-1@base=READY` |
| 10 | `PS-064:CODEC-1@base=READY` |
| 11 | `PS-066:THREAT-D0@base=READY` |
| 12 | `PS-071:HASH-SOURCE-R1@base=READY` |
| 13 | `PS-086:RELEASE-R1@base=READY` |
| 14 | `PS-087:RELEASE-R1@framework=READY` |

## `PlanningCheckerSummaryV1`

This is the sole exact numeric projection of the retained planning checker. Every value is derived
from its canonical parsed source; prose elsewhere links here instead of repeating these numbers.

| Summary field | Canonical decimal value |
| --- | ---: |
| `capability_node_count_u32` | `72` |
| `foundation_node_count_u32` | `6` |
| `detailed_node_count_u32` | `66` |
| `phase_status_count_u32` | `95` |
| `ready_frontier_count_u32` | `14` |
| `base_dependency_edge_count_u32` | `312` |
| `phase_dependency_edge_count_u32` | `7` |
| `conditional_edge_count_u32` | `44` |
| `total_declared_edge_count_u32` | `363` |
| `predicate_entailment_count_u32` | `10` |
| `planning_markdown_count_u32` | `22` |
| `frozen_text_artifact_count_u32` | `12` |
| `frozen_markdown_audit_count_u32` | `11` |
| `frozen_json_receipt_count_u32` | `1` |

`LEDGER-1`, a token/ownership system, networking, and autonomous rules should not be combined in
one change. The strongest next trunk sequence is:

```text
ACCOUNT-1
  -> LOCAL-BOUNDS-1
  -> STORAGE-1
  -> LEDGER-1
  -> TRANSITION-SPEC-D1
  -> select COMMIT-R1 result
  -> COMMIT-1
  -> pre-genesis design closure
  -> PROFILE-SCOPE-D0
  -> THREAT-1 + BOUNDS-1
  -> GENESIS-D1
  -> PROTOCOL-ID-1
  -> PROFILE-D1
  -> TRANSITION-SPEC-1
  -> IDENTITY-1
  -> AUTH-COMMIT-1
  -> GOVERNANCE-1
  -> activated state producers + VALIDATOR-SCHEDULE-1
  -> EXECUTOR-1
  -> EXECUTION-ROOT-1
  -> GENESIS-ROOT-PARITY-1
  -> MULTI-1
  -> SEQUENCER-1
```

This sequence is deliberately non-normative shorthand. `IDENTITY-D1`, `CONSENSUS-D1`,
`GOVERNANCE-D1`, `THREAT-D0`, `CODEC-1`, `CRYPTO-D1`, `CRYPTO-1`, `COST-1`, `BOUNDS-1`,
`EXECUTOR-1`, and conditional branches join only at their exact `Depends on` fields and manifest
rows. Run the retained checker rather than inferring an omitted edge from this display.

The later public-network path cannot jump from genesis or profile admission directly to testnet.
After the initial profile, transition specification, admission, root parity, admitted release, and
launch-authority prerequisites pass, `CHAIN-LAUNCH-1` packages and authorizes their complete
embedded closure. `TESTNET-1`, `L1-CANDIDATE`, and `L1-PRODUCTION` retain that exact launch
identity and package rather than reconstructing or replacing it downstream.

An in-place successor is a separate closure, never an edit to the launch package. Before its
activation, `SUCCESSOR-ACTIVATION-1` binds and availability-covers the exact target admission,
release, implementation, and migration preimages under the old profile; CE-44 and CE-33 carry that
artifact through testnet and candidate review. An immutable-version profile does not realize those
conditional edges.

At that point Bitblock could be truthfully described as an authenticated, accountable multiplayer
ledger under a centralized ordering boundary. Consensus, currency, and Layer 1 would still remain
later nodes.

## How to update the tree

When a checkpoint changes a node:

1. link the exact commit and artifact;
2. record commands and exit codes actually run;
3. copy no broader status from an upstream node;
4. move only the node whose gate passed;
5. add newly discovered prerequisites rather than hiding them;
6. retain rejected alternatives and failed controls in the owning design/audit document;
7. update [`HANDOFF.md`](HANDOFF.md) and any changed current semantics in
   [`SEMANTICS.md`](SEMANTICS.md).

Run `python -B tools/check_planning_docs.py --self-test` after every tree edit. Its exact graph,
status, READY-frontier, inventory, and frozen-artifact counts are derived and checked against
`PlanningCheckerSummaryV1`; prose does not independently restate them. It also checks the
unconditional conservative DAG, context-bearing conditional-instance rows, direct predicate-
entailment manifest and context-local acyclic closure, compatible role-edge union, exact path-only
handoff order, frozen inventory/EOL policy, links/fences/tables in the planning set, and powered
in-memory mutations for those surfaces. Its controls must reject a phase-status promotion/demotion,
duplicate or omitted phase tuple, locked/substituted/omitted/duplicated/reordered frontier entry,
phase-edge mutation, a missing CE-35–CE-42 Verkle prerequisite, a launch row evaluated as target, a
target row evaluated as launch, collapse of the two contexts on a successor, a missing
`CHAIN-LAUNCH-1` or `SUCCESSOR-ACTIVATION-1` dependency, a wrong or omitted
launch/successor-closure reference, and handoff-order/count drift. It still does **not** decide which
predicates are true,
prove that the manifest is semantically complete, or expand and validate each realizable
profile-specific artifact closure. Before `PROFILE-1` can admit anything, that admission checker
must resolve both launch and target predicate contexts and their roles against exact artifacts,
reject incompatible combinations, and apply these already checked edges and entailments. Until that
powered admission checker exists, conditional closure admission remains **LOCKED**. The compact
diagram remains explanatory only.

A branch can unlock knowledge without unlocking authority. That distinction is the central rule of
this tree.
