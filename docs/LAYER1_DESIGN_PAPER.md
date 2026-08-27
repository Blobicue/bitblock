# Bitblock: From Binary Occupancy to a Verifiable Layer 1

The checkpoint-0.3 audit remains pinned to paper commit `034b7b2b5f129625f9c6dcec3e14038b7c4aee5f`.
The first post-checkpoint corrections—conditional bedrock economics and scheduling-authority
cross-links—were made after frozen planning input
`162406819133c1fd57d0fd26cbe5fa0b495766b8` and are scoped in
[`FUTURE_PLAN_AUDIT.md`](FUTURE_PLAN_AUDIT.md). They are not a silent claim that the whole paper was
re-audited.

The later pre-ID byte pipeline, profile-derived root composition, gate-applicability, and Verkle
feasibility clarifications are a successor planning correction scoped by
[`FUTURE_PLAN_AUDIT_LOOP2.md`](FUTURE_PLAN_AUDIT_LOOP2.md) and recorded in
[`TRANSCRIPT_AUDIT.md`](TRANSCRIPT_AUDIT.md). They preserve the original pinned audit result and do
not claim that checkpoint 0.3 received another complete paper audit.

**Design paper — checkpoint 0.3 — 2026-08-26**

This paper makes the Bitblock idea explicit as a protocol design. It starts from the
current Rust implementation and describes the additional machinery required before any
cryptocurrency or Layer 1 claim would be honest.

It is a design document, not a claim that the current executable is a blockchain. The
current project deliberately stops at a deterministic local coordinate world, local
history evidence, and bounded formal models.

## Reading the status labels

- **Implemented** — present in the current source tree.
- **Bounded** — covered by a finite test, vector, or abstract proof whose scope is named.
- **Proposed** — a protocol shape that could be implemented later.
- **Open** — a decision or proof obligation that is not settled.
- **Not claimed** — a property the current system explicitly refuses to imply.

Normative words such as **MUST**, **SHOULD**, and **MAY** describe the proposed future
protocol, not the current executable.

## Security-claim boundary

“Full Layer 1 security coverage” is not an unconditional promise. It means that every
claimed property is tied to a protocol version, an adversary model, explicit cryptographic
and network assumptions, and evidence that reaches the named boundary. A protocol cannot
prove safety against an unspecified validator set, an unspecified network, an unknown
implementation, or an unavailable history.

This paper therefore treats security as a coverage matrix, not a badge. A passing local
test can close a local invariant while leaving consensus, custody, or availability open.
The final public claim MUST state both what is covered and what remains assumed or excluded.

For this paper, a **Layer 1** means the base replicated state machine that accepts
transactions, commits canonical state roots, and provides a finality/availability path to
independent nodes. It does not mean the renderer, a game server with an internal database,
a local hash chain, a bridge, or a token that has merely been given a market name.

## Abstract

Bitblock begins with one simple rule: a coordinate is a bit. If a block exists at a
coordinate, the bit is `1`; if it does not, the bit is `0`. The intended protocol is an
ordered fold over canonical transactions. The current fixed tick is a smaller local reducer:
it consumes aggregate live-input state plus ordered edit attempts, while supplied
`InteractionTape` values provide bounded replay input. It is not yet a complete live action
recorder. Rendering is a projection of state, not a second simulation authority.

That model has a useful accounting interpretation. A successful transition from `off` to
`on` can open one issuance episode, and a transition from `on` to `off` can close it. This
gives exact local conservation and provenance, but it is not yet a cryptocurrency. A
cryptocurrency additionally needs authenticated participants, authorization, transfer
semantics, replay and double-spend protection, an agreed ordering, finality, data
availability, resource resistance, and independent verification.

The proposed architecture therefore separates five things that are easy to accidentally
collapse:

1. **World state** — which coordinates are occupied.
2. **Asset state** — which occupancy episodes have live token records.
3. **Authority** — which identity may submit or authorize an action.
4. **Replication** — which machines agree on one ordered history.
5. **Observation** — how a camera and renderer display the committed state.

Only the first four are relevant to a Layer 1 state machine. Pixels, camera floats, and
window timing are not consensus evidence.

## 1. Current foundation

The starting point is intentionally small:

| Surface | Current truth | Status |
| --- | --- | --- |
| Coordinate domain | Signed 64-bit `(x, y, z)` addresses; absent means off | Implemented |
| Storage | Lazy 16×16×16 sparse chunks; empty non-origin chunks can be released | Implemented |
| Spatial origin anchor | `(0, 0, 0)` is permanent bedrock and cannot be removed | Implemented |
| Observer | Camera state beside bedrock; no entity, gravity, terrain, inventory, or autonomous rules | Implemented |
| Tick | Fixed 60 Hz reducer consumes aggregate live-input state and ordered edit attempts | Implemented |
| World/history identity | Canonical SHA-256 content tags and parent-linked local receipts | Implemented |
| Interaction replay | Supplied bounded canonical `InteractionTape` values and headless replay reports; no complete live recorder | Implemented, bounded |
| Mint/burn evidence | `CoordinateEpisodeEvidence` records local issuance episodes and consuming burns | Implemented, feature-gated, bounded/in-memory |
| Abstract proof | Lean finite-support models cover bedrock/edit replay and per-edit supply delta/no-op theorems; not token identity or Rust refinement | Bounded |
| Ownership | No owners, accounts, signatures, or transfer records | Not claimed |
| Networking | No replicated nodes or wire consensus protocol | Not claimed |
| Persistence | Research storage exists, but the default live history is not durable custody | Not claimed |
| Layer 1 security | No finality, validator set, fork choice, or availability protocol | Not claimed |

The existing SHA-256 tags identify canonical bytes under named schemas. They do not prove
that the bytes are true, current, owned, persisted, or agreed by other machines. The
research-only coordinate-episode surface records and locally verifies bounded issuance
provenance; it has no balances, owners, asset transfers, durable custody, or monetary value.
The immutable origin is a spatial invariant. The current local history genesis commits an
initial world digest, but it is not a chain identity, validator trust anchor, or future
protocol genesis package.

## 2. The semantic state machine

Let the coordinate domain be

```text
C = { -2^63, ..., 2^63 - 1 }^3
```

for the current version, and let `o = (0, 0, 0)` be the immutable origin. A world state
contains an occupied set:

```text
B_h : C -> {0, 1}
B_h(o) = 1
```

The absence of a stored bit is semantically `0`, not “unloaded” or “unknown”. Storage is
an implementation of this function; it is not the definition of the world.

A future deterministic execution state can be written as:

```text
ApplicationState_h = (B_h, M_h, E_h, X_h)
ProtocolState_h    = (A_h, N_h, P_h, G_h, Q_h)
ExecutionState_h   = (ApplicationState_h, ProtocolState_h)

ConsensusHead_(i,h) = (height, block_id, execution_state_root, history_root,
                       consensus_status, availability_status,
                       availability_evidence_ref)

NodeLocalView_i = (UI_i, mempool_i, candidate_branches_i, current_votes_i,
                   timeouts_i, peers_i, sync_i, caches_i)
```

where:

- `B_h` is the occupied-coordinate function;
- `M_h` is the active mint map from mutable occupied coordinates to token identifiers;
- `E_h` is canonical mint/burn episode and event state, including live/consumed status and
  provenance needed by the chosen pruning policy;
- `ApplicationState_h` MAY also contain `X_h`, the selected ownership, transfer, balance,
  fee, or stake state. `X_h` is empty until an asset/economic model is chosen, but any later
  asset domain MUST have its own canonical state and root;
- `A_h` is committed authorization, identity, delegation, rotation, and revocation state;
- `N_h` is committed per-identity replay state such as nonces;
- `P_h` is committed protocol-version, codec, resource, and economic parameter state;
- `G_h` is committed governance, upgrade, and emergency-policy state;
- `Q_h` is the validator/sequencer schedule authorized by the parent accepted under the
  selected consensus rule, including
  active and delayed-next authority sets where the selected consensus needs them;
- `ConsensusHead_(i,h)` identifies node `i`'s rule-verified ledger prefix, exact
  finality/confirmation status, and typed availability status/evidence reference. A bare
  availability claim cannot populate these fields; the status is `Unverified` until the
  selected possession, certificate, or sampling rule verifies. A probabilistic tip may be
  node-relative; a public safety claim covers only the prefix satisfying its declared
  confirmation/common-prefix rule; and
- `NodeLocalView_i` contains facts that may legitimately differ between honest nodes and is
  never covered by `execution_state_root`.

Candidate branches, mempool contents, current-round votes, timeouts, peer state, retrieval
progress, caches, camera state, and rendering state are not deterministic results of user
transactions. They MUST NOT be placed inside `ExecutionState_h`. Finality evidence is
detached consensus evidence over a proposed block identifier; actual byte availability is a network
property evaluated under the selected rule, not a Boolean manufactured by `Execute`.

The current executable has `B_h`, local camera/presentation state, ordered edit-attempt
history, aggregate tick outcomes, and local content tags. Its supplied interaction tapes are
replay inputs rather than proof that every live input was recorded. It does not implement
the proposed authorization, nonce, governance, validator, consensus, or availability
protocols. Resize, sidebar-toggle, and clear-motion actions also mutate local state when
accepted by the application, before `fixed_tick`; that local orchestration is not consensus
execution.

Protocol bootstrap uses distinct canonical producers. `TRANSITION-SPEC-D1` first freezes only the
base operation, result, and system-transition meanings plus the extension interface that scope
production is allowed to consume. `PROFILE-SCOPE-D0` then selects already designed schemas,
suites, roles, root domains, and optional features; `GENESIS-D1` supplies the concrete initial
logical state and the measured limits that passed for that scope; and `PROFILE-D1` emits the
candidate profile bytes. Only after that candidate has an identifier does `TRANSITION-SPEC-1`
resolve every active extension slot into the exact versioned `TRANSITION-SPEC-1@profile` artifact
and powered corpus consumed by `EXECUTOR-1@profile`. These are proposed future byte contracts and
producer phases, not implemented codecs.

To avoid self-reference, the exact pre-ID pipeline is:

```text
RequiredDomainV1 = Canonical(
    domain_tag,
    composition_layer,
    root_type,
    tree_domain,
    root_schema_id,
    commitment_profile_id
)

DomainRegistryEntryV1 =
    ActiveDomainV1(RequiredDomainV1)
  | InactiveDomainV1(domain_tag, composition_layer, inactive_sentinel_schema_id)

DomainRegistryV1 = CanonicalSortedUniqueByDomainTag([
    DomainRegistryEntryV1...
])

domain_registry_digest = ProfileIdHashV1(
    Domain("bitblock/domain-registry/v1")
 || LengthFrame(Canonical(DomainRegistryV1))
)

ProfileScopeV1 = Canonical(
    profile_scope_schema_version,
    base_transition_spec_id,
    activated_operation_and_system_transition_schema_ids,
    activated_state_schema_ids,
    DomainRegistryV1,
    identity_and_authorization_schema_ids,
    cryptographic_suite_ids,
    consensus_governance_and_network_role_policy_ids,
    optional_feature_ids,
    proposed_limit_vector
)

profile_scope_digest = H(profile_scope_domain || ProfileScopeV1)

GenesisPayloadV1 = Canonical(
    genesis_schema_version,
    profile_scope_digest,
    domain_registry_digest,
    chain_id,
    world_id,
    identity_namespace,
    B_0, M_0, E_0, X_0,
    A_0, N_0, P_0, G_0, Q_0,
    validated_limit_vector,
    bootstrap_and_availability_policy
)

genesis_id      = H(genesis_domain || GenesisPayloadV1)
genesis_package = (GenesisPayloadV1, genesis_id)

CandidateProfileV1 = Canonical(
    candidate_profile_schema_version,
    profile_scope_digest,
    domain_registry_digest,
    genesis_id,
    protocol_version,
    canonical_scope_and_genesis_projection,
    declared_capability_closure,
    canonical_stage_gate_matrix,
    required_artifact_and_loss_condition_slots,
    candidate_release_configuration
)

profile_id = H(candidate_profile_domain || CandidateProfileV1)
```

`ProfileScopeV1` owns feature, schema, suite, role, and active-or-inactive domain choices. The
domain registry is pre-ID: neither entry variant contains `profile_id`. Its independent
`domain_registry_digest` is projected into both later objects and MUST reproduce from the exact
registry inside the resolved scope. `GenesisPayloadV1` owns concrete initial state and validated
limits and references the scope by its digest. A proposed limit is not silently changed during
validation: `validated_limit_vector` MUST byte-equal the corresponding scope projection, or the
scope is regenerated and receives a new digest before genesis is derived. `CandidateProfileV1`
owns admission metadata and references both earlier objects. Repeating a semantic value is normally
forbidden; when a projection is deliberately repeated for independent inspection, it MUST be the
specified canonical projection and byte-equal its owner. Missing, additional, or unequal projected
values reject the candidate.

`GenesisPayloadV1` excludes `genesis_id`, and `CandidateProfileV1` excludes `profile_id`. The
resulting `profile_id` identifies exact candidate bytes but cannot authorize a network merely by
existing. `PROFILE-1` admission must separately bind that identifier to the complete passing
capability/evidence closure. `RELEASE-R1` may separately bind a reproducible candidate build to the
same identifier; `RELEASE-1` can bind that candidate artifact to the admitted profile only after
admission. Candidate builds and admitted releases therefore remain different states even when they
use the same candidate `profile_id`.

After `profile_id` is derived, the profile-specific transition producer emits:

```text
TransitionSpecAtProfileV1 = Canonical(
    transition_spec_schema_version,
    profile_id,
    base_transition_spec_id,
    canonical_resolved_active_extension_spec_refs,
    canonical_validated_bound_projection,
    complete_operation_result_and_system_transition_table,
    powered_transition_corpus_digest
)

transition_spec_at_profile_id =
    H(transition_spec_at_profile_domain || TransitionSpecAtProfileV1)
```

That artifact is an admission prerequisite, not an input to `CandidateProfileV1`; putting its
digest back into the candidate preimage would create a candidate/specification identifier cycle.
Every bound or active-schema value it repeats MUST be the canonical byte-equal projection of its
scope/genesis/candidate owner. `PROFILE-1` verifies that the produced specification is the
deterministic resolution of the candidate's declared scope and closure.

Genesis has two separate completion gates. **Logical-byte completion** under `GENESIS-D1` means
independent encoders reconstruct the same scope-bound concrete initial state and validated limits.
It does not require or certify an implementation root. **Genesis-root parity** is later work under
`COMMIT-1`, `GENESIS-ROOT-PARITY-1`, and `EXECUTION-ROOT-1`: independent implementations rebuild
every profile-required initial domain from those logical bytes and agree with the published initial
root tuple. A root mismatch does not rewrite genesis bytes or select whichever implementation
looked canonical; it keeps the profile locked.

For the first proposed network profile, one chain contains exactly one `world_id`; a future
multi-world version would replace the single world with a committed `worlds_root`. Initially
`support(B_0) = {o}`, `M_0` and `E_0` are empty, and every allocation, authority, parameter,
validator, and governance record is explicit. The package MUST be immutable and reproducible. For
this paper's provisional non-economic first-profile sketch, origin bedrock is an application
invariant rather than an occupancy-episode token, observer property, or validator trust anchor.
That is not a final asset ruling: `ASSET-D1` may instead define a separate permanently locked
genesis allocation outside mutable occupancy episodes, and the selected genesis profile must make
the choice explicit.

The authoritative execution transition is a pure function:

```text
ExecutionState_(h+1) = Execute(
    ExecutionState_h,
    BlockContext_(h+1),
    OrderedTransactions_(h+1)
)
```

`ExecutionState_h` is the post-state of ledger height `h` accepted at the profile's named
consensus status; the candidate successor has
height `h+1`. `BlockContext_(h+1)` contains only committed inputs required by the selected version, such as
height, parent identifier, and any authenticated protocol randomness. Execution MUST depend
only on canonical parent state, committed context, and ordered transaction bytes. Proposal,
validation, voting, certification, and publication are separate transitions. Wall-clock
timing MAY schedule local work and consensus timeouts, but MUST NOT change deterministic
execution of a given candidate body.

Consensus height is independent of the current 60 Hz simulation tick. A version MAY commit
a deterministic number of application sub-ticks per block, or omit application sub-ticks
and execute only ordered transactions. Any mapping is explicit in `P_h`; a validator MUST
NOT infer consensus height or finality from elapsed time, a local window, or a frame render.

### 2.1 Occupancy rules

For a mutable coordinate `c`:

```text
off -> on   = open one mint episode
on  -> off   = consume one live mint episode
on  -> on   = idempotent no-op
off -> off  = idempotent no-op
origin edit = protected no-op
```

If the same coordinate is deleted and later placed again, the later placement MUST receive
a new token identifier. The protocol must choose whether identity is transaction-stable or
inclusion-stable across provisional forks. One non-circular inclusion-stable candidate is:

```text
UserEventPosition = Canonical(
    user_event_position_domain,
    candidate_execution_context_id,
    transaction_index,
    action_index,
    effect_index
)

SystemEventPosition = Canonical(
    system_event_position_domain,
    candidate_execution_context_id,
    system_phase,
    system_rule_id,
    system_transition_ordinal,
    effect_index
)

event_position = H(Canonical(UserEventPosition | SystemEventPosition))

mint_id = H(
    Canonical(mint_domain, event_position, coordinate)
)

burn_event_id = H(Canonical(
    burn_domain,
    event_position,
    coordinate,
    consumed_mint_id
))
```

`candidate_execution_context_id` is defined in §4.4 from the parent, exact body, committed
block context, and execution policy before execution begins. Both tagged position variants
are therefore known before post-state and `block_id` computation and create no hash cycle.
The system namespace covers rent, reward, migration, or governance
transitions that are not user transaction actions; its ordinals and phases are deterministic
protocol rules. Re-inclusion under another parent creates another user event position. A transaction-stable
alternative would deliberately preserve identity across such re-inclusion and must define
the collision and duplicate-inclusion rules. The final choice MUST include an unambiguous
event identity or equivalent canonical nonce; a coordinate alone is never a token ID.
Consensus status remains separate. The historical mint and its consuming burn remain in
the ledger prefix accepted under the selected rule.

Cardinality alone is too weak: it could pass while tokens are attached to the wrong
coordinates. Let `E_h[id]` be canonical episode state. The core correspondence is:

```text
dom(M_h) = { c in C : c != o and B_h(c) = 1 }
M_h is injective

for every c in dom(M_h):
    E_h[M_h(c)].coordinate = c
    E_h[M_h(c)].status = live

range(M_h) = {
    mint_id : E_h[mint_id] is a live MintEpisode
}

no burned mint_id occurs in range(M_h)
each MintEpisode has at most one consuming BurnEvent
each BurnEvent names exactly one existing MintEpisode
E_h[mint_id].status = consumed iff exactly one BurnEvent has
    consumed_mint_id = mint_id
MintEpisode IDs and BurnEvent IDs use distinct typed domains
```

Therefore `|M_h| = |{c != o : B_h(c) = 1}|` is a corollary. Each successful burn consumes
the exact live mint ID at that coordinate and emits a distinct burn-event ID; a re-placement
emits a fresh mint ID. A transfer preserves the mint ID, coordinate episode, and active
supply while changing only the selected authorization/ownership state.

This is **active supply**, not cumulative issuance. A user can produce arbitrarily many
historical mint and burn events by repeatedly placing and deleting a coordinate unless a
separate issuance, fee, or rate policy is introduced.

World and asset transitions MUST commit atomically. A receipt cannot report a successful
mint while the coordinate write failed, or a successful burn while the token-spend update
failed. A failed transaction may charge a declared fee, but it must not leave a partial
application state.

### 2.2 What “one block is one coin” means

A coordinate-specific occupancy token is naturally a unique UTXO-like or non-fungible
asset. It is not automatically a fungible cryptocurrency unit. The protocol MUST choose
one of these meanings:

1. **Occupancy token:** the token represents the right to claim one occupied coordinate.
2. **Edit right:** the token represents authority to change a coordinate, while the world
   remains world-owned.
3. **Fungible resource:** blocks consume or represent a separate interchangeable balance.
4. **Dual model:** occupancy tokens and a separate fungible fee/resource asset coexist.

Until that choice is made, “mint” and “burn” should be read as provenance terms, not as a
promise of financial value.

Transfers, if added, preserve active supply while changing ownership metadata. They MUST
not silently rewrite the coordinate episode. A burn becomes irreversible only under the
selected deterministic-finality rule; a probabilistic profile carries its declared residual
reversal risk. Governance MAY authorize a later, explicit compensating mint or
recovery transition, but it cannot erase the burn. A governance hard fork creates a newly
versioned history and must be disclosed as such.

## 3. Identity, authorization, and ownership

Multiplayer is useful because it creates competing action sources, but multiplayer alone
does not create secure identity. A username, session ID, or random UUID can be copied. A
future protocol needs a committed authorization model. A stable-actor candidate is:

```text
actor_id = H(Canonical(
    actor_identity_domain,
    chain_id,
    identity_namespace,
    initial_authorization_descriptor
))

A_h[actor_id] = current versioned authorization descriptor
```

`identity_namespace` is selected inside `genesis_payload` before `genesis_id` is derived, so
genesis authorization records do not create an identifier cycle. Transactions still bind
`genesis_id`, preventing cross-genesis replay. The descriptor can name one key, a threshold set, delegated capabilities, and an explicit
rotation/recovery policy. A consensus-accepted authorization transition changes the descriptor while
preserving `actor_id`. The simpler alternative is a public-key-hash address, in which case
“rotation” is an authorized ownership transfer to a new address rather than preservation of
identity. A network version MUST select one model; two valid keys cannot silently alias one
actor unless the committed multisignature or delegation policy says they do.

Throughout this paper, `Canonical(...)` and every remaining `a || b` sketch mean a typed,
unambiguously length-framed tuple encoding, never raw concatenation. The protocol MUST pin
integer width and endianness, enum and optional-field tags, list lengths and ordering,
normalization rules, and rejection of unknown, duplicate, non-minimal, or trailing fields.

A proposed signed transaction separates the signed payload from its signature:

```text
TransactionSignDoc =
    protocol_version
 || chain_id
 || genesis_id
 || profile_id
 || world_id
 || actor_id
 || authorizing_key_id
 || signature_suite_id
 || signing_hash_suite_id
 || actor_nonce
 || valid_after
 || valid_until
 || optional_anchor_block_id
 || optional_expected_state_root
 || fee_and_resource_offer
 || ordered_operations

signing_digest    = H[signing_hash_suite_id](
    action_signature_domain || Canonical(TransactionSignDoc)
)
signature         = Sign[signature_suite_id](actor_secret_key, signing_digest)
SignedTransaction = Canonical(
    TransactionSignDoc,
    signature
)

tx_id = H(transaction_id_domain || SignedTransaction)
```

The signature algorithm, key encoding, and hash suite are **Open** decisions. They MUST be
versioned and pinned before a network protocol is declared stable. The signed domain MUST include
the chain, genesis, and admitted profile identifiers so an action cannot be replayed under another
world or candidate configuration. `profile_id` is added only after candidate derivation and
admission; it is context for signatures, headers, and releases, not a field inside the initial
application- or execution-root preimages.
The transaction identifier hashes the exact canonical signed envelope. The chosen signature
scheme MUST either be non-malleable or define one canonical signature encoding. No alternate
text spelling, omitted signature, or decoded-and-reencoded variant may create another
identifier for the same submitted transaction. SHA-256 content tags are not a substitute
for signatures.

The authorization rules MUST state:

- who may mint at a coordinate;
- who may burn or transfer the resulting token;
- whether ownership follows the token or remains with the world;
- whether an owner may delegate edit rights;
- how key rotation and recovery work;
- what happens when a key is lost or compromised.

Key custody is part of the security boundary. The protocol needs a documented key lifecycle:
generation, secure storage, backup, rotation, revocation, recovery, and compromise response.
If identity is permissionless, the Sybil cost or admission rule must also be explicit. A
public key proves control of a key under the signature assumption; it does not prove a
real-world name or unique human.

The cryptographic operations boundary MUST additionally specify:

- CSPRNG and entropy assumptions for key generation;
- public-key, subgroup, low-order-point, and proof-of-possession checks where applicable;
- canonical signature verification and a safe individual-verification fallback if batch or
  aggregate verification fails;
- separate signing domains and preferably separate keys for actor transactions, proposals,
  votes, validator transport, rewards/withdrawals, governance/emergency authority, and
  release signing;
- durable anti-double-sign state that survives validator restart, backup restore, signer
  failover, and clock rollback; and
- the trusted computing base, remote-signer or HSM boundary if used, and any physical or
  side-channel attacks explicitly excluded from the claim.

Client presentation is not proof of signing intent. A wallet SHOULD render the exact decoded
semantic operations, chain/genesis/profile/world, fee cap, validity window, and authorization being
signed; the signer still authorizes only the canonical bytes above.

Actor/observer keys and validator keys are separate authority namespaces. An observer
signature authorizes only the action rights granted to that actor; it does not grant a vote,
proposal right, or validator reward unless a separate validator-state transition says so.

The observer can remain only camera state. It does not need to become an entity merely to
carry an identity or sign actions. However, a local camera position is not automatically a
consensus fact. If “the observer cannot enter a block” is an economic validity rule, the
protocol MUST either make the relevant pose a canonical state component or treat the
submitted coordinate and permission as the authoritative action. A client-only raycast
cannot authorize a transaction.

## 4. Transactions, ticks, and receipts

The current 60 Hz tick is a local simulation clock. Future consensus height is independent.
If a network version retains application ticks, `BlockContext_h` MUST supply their exact
logical ordinals and deterministic per-block count. No validator is required to execute at
60 wall-clock ticks per second.

### 4.1 Transaction validity

Before an action can affect state, a validator MUST check at least:

1. canonical decoding and protocol version;
2. signature and identity binding;
3. chain, genesis, admitted profile, world, and any selected anchor/state precondition;
4. nonce freshness and replay protection;
5. coordinate range and operation limits;
6. bedrock protection;
7. current occupancy and token-reference correctness;
8. ownership or delegated authorization;
9. fee/resource budget and per-tick work limits;
10. any explicitly consensus-relevant observer constraint.

The validator MUST also apply block-level limits before execution: maximum transaction
count and byte size, maximum coordinate/work budget, maximum state growth, and checked
integer arithmetic. Resource accounting must be deterministic; allocation failure,
overflow, and an exhausted budget are validity results, not process crashes.

`valid_after` and `valid_until` are interpreted against a declared logical height or epoch,
not an untrusted local wall clock. Signature verification, proof verification, and state
lookups also consume bounded work; otherwise a transaction can be syntactically valid yet
still exhaust a validator before the state transition begins.

Validity, admission, execution failure, and semantic no-op are different outcomes:

| Outcome | Consensus effect |
| --- | --- |
| `RejectedBeforeAdmission` | Malformed, bad-signature, wrong-domain, expired, or pre-admission-unaffordable bytes are excluded. They cause no consensus-state mutation; a local rejection response need not be permanently committed. |
| `DroppedAfterAdmission` | A node-local mempool may evict or expire an admitted but unincluded transaction under declared queue rules. This causes no consensus-state mutation and needs a local disposition, not a canonical receipt. |
| `BlockInvalid` | A proposed block that includes a transaction forbidden by block validation, has a wrong root, exceeds limits, or cannot execute deterministically is rejected wholesale. No candidate post-root becomes canonical. |
| `IncludedSuccess` | Ordered operations commit atomically with the declared nonce and fee disposition. |
| `IncludedNoOp` | An allowed idempotent `on -> on` or `off -> off` records an explicit success-no-op and applies the versioned nonce/fee rule, with no occupancy or token change. |
| `IncludedFailure` | A transaction admitted under the selected policy can record failure. It performs no partial execution-domain writes; only the explicitly selected nonce/fee effects may commit atomically. |

The proposed initial transaction model is transaction-local atomicity. Operations evaluate
in order against a staged transaction state; allowed no-ops may coexist with successes, but
any failing operation discards every staged execution-domain write for that transaction
except the explicitly versioned nonce/fee disposition of `IncludedFailure`. Blocks execute
transactions sequentially, so each receipt's actual pre-state
is the previous transaction's post-state. Candidate execution and durable publication are a
separate atomicity boundary: a node publishes either the fully validated candidate body and
roots or no candidate root, never a partial batch.

Every included transaction MUST have exactly one canonical receipt. Every submitted item
MUST receive a local disposition, but the protocol need not make rejected network spam part
of permanent consensus history. The submission, admission, execution, rejection, and
finalization lanes must reconcile without silently losing an included action.

### 4.2 Ordering and conflicts

Two observers may target the same coordinate in one logical tick. Arrival order on one
machine is not a safe distributed rule. A batch accepted at the selected consensus status
MUST carry one canonical order,
chosen by a sequencer or consensus mechanism. Under that order:

- the first valid `on` may open the coordinate;
- later `on` actions become explicit no-ops or rejections;
- an `off` must name a live token and satisfy authorization;
- a later `on` after a valid burn creates a fresh token identifier.

The tie-break rule and its error dispositions MUST be part of the state-machine version,
not an undocumented property of a particular network implementation.

If `toggle` remains a network operation, its receipt SHOULD include the observed pre-state
and post-state, and the transaction MAY carry an expected-state/token precondition. A
client-local assumption about what `toggle` will hit is not an authorization proof. A safer
wire format may lower `toggle` into explicit `on` or `off` after canonical ordering.

### 4.3 Transaction lifecycle

The protocol MUST distinguish these states:

```text
submitted -> rejected-before-admission / locally-expired
         \-> admitted -> dropped-or-expired-before-inclusion
                      \-> included-and-executed -> candidate
                                                 \-> finalized-or-threshold-confirmed
                                                 \-> orphaned-or-reorged under the selected rule
```

Mempool contents are node-local and MUST NOT be spendable or treated as ownership proof. A
transaction that references a stale anchor, expired validity window, or consumed nonce must
have a deterministic replacement or rejection rule. Exact-parent binding and windowed
anchor/precondition binding are different designs: a version MUST select one. If exact-parent
binding is selected, every new parent requires explicit re-signing or replacement; if a
windowed anchor is selected, the admissible ancestor window and expected-state failures are
consensus rules.

An unfinalized mint is provisional. Wallets and clients MUST distinguish candidate,
executed, and finalized or threshold-confirmed receipts. If a pre-finality fork removes a transaction, its mint
or burn is not part of the canonical supply. The same signed transaction MAY be re-included
if it remains valid under the surviving nonce, anchor, and validity-window rules; otherwise
the actor must authorize a replacement.

### 4.4 Proposed block, identifier, consensus evidence, and receipts

The signed object must not contain its own signature or certificate. A consensus-neutral
envelope separates the ordered body, deterministic execution commitments, proposal
signature, and detached consensus evidence:

`CommitOrdered(domain, items)` below means a domain-separated authenticated sequence
commitment that binds the sequence type, exact length, every zero-based index, and each
canonical leaf byte string. Its inclusion proof also binds domain, length, index, and leaf;
it is not an undifferentiated Merkle root over raw concatenation.

`CommitAvailability(policy_id, payload)` means the selected binding, domain-separated
commitment to the exact canonical `AvailabilityPayload` bytes. Any erasure-coded shares,
sampling positions, or recovery proofs are derived from and verify against that same payload
and policy; a commitment to unrelated bytes cannot satisfy proposal validity.

`system_transition_schedule` contains only pre-execution rule invocations and deterministic
ordinals. It excludes event IDs, effects, post-state values, and anything derived from
`candidate_execution_context_id`, preventing a context/identifier cycle.

```text
Body = Canonical(
    ordered_signed_transactions,
    deterministic_auxiliary_inputs
)

transactions_root = CommitOrdered(
    transaction_sequence_domain,
    ordered_signed_transactions
)
body_root         = H(body_domain || Body)

BlockContext = Canonical(
    protocol_version,
    chain_id,
    genesis_id,
    profile_id,
    world_id,
    height,
    parent_block_id,
    epoch,
    round_or_view,
    optional_committed_randomness,
    application_subtick_schedule,
    system_transition_schedule
)

block_context_root = H(block_context_domain || BlockContext)

candidate_execution_context_id = H(Canonical(
    candidate_execution_context_domain,
    protocol_version,
    chain_id,
    genesis_id,
    profile_id,
    world_id,
    height,
    parent_block_id,
    body_root,
    block_context_root,
    execution_parameter_root,
    governance_authority_root,
    active_validator_root,
    consensus_policy_id,
    proposer_identity
))

receipts_root     = CommitOrdered(
    execution_receipt_sequence_domain,
    all_transaction_and_system_receipts_in_execution_order
)

AvailabilityPayload = Canonical(
    Body,
    all_transaction_and_system_receipts_in_execution_order,
    consensus_required_execution_witnesses
)

data_availability_commitment = CommitAvailability(
    data_availability_policy_id,
    AvailabilityPayload
)

post_history_root = H(Canonical(
    history_domain,
    parent_history_root,
    height,
    body_root,
    receipts_root,
    post_execution_state_root
))

ProposalHeader = Canonical(
    protocol_version,
    chain_id,
    genesis_id,
    profile_id,
    world_id,
    height,
    epoch,
    round_or_view,
    parent_block_id,
    parent_history_root,
    pre_execution_state_root,
    block_context_root,
    candidate_execution_context_id,
    body_root,
    transactions_root,
    receipts_root,
    post_execution_state_root,
    post_history_root,
    execution_parameter_root,
    execution_policy_id,
    governance_authority_root,
    active_validator_root,
    optional_next_validator_root,
    consensus_policy_id,
    data_availability_policy_id,
    data_availability_commitment,
    proposer_identity,
    proposer_key_id,
    proposal_signature_suite_id,
    proposal_signing_hash_suite_id
)

block_id = H(block_header_domain || ProposalHeader)
proposer_signature = Sign[proposal_signature_suite_id](
    proposer_secret_key_for(proposer_key_id),
    H[proposal_signing_hash_suite_id](proposal_signature_domain || block_id)
)
Proposal           = Canonical(ProposalHeader, AvailabilityPayload, proposer_signature)
ConsensusEvidence =
    DeterministicFinalityEvidence(block_id, ...)
  | ProbabilisticConfirmationEvidence(block_id, ...)
  | CentralAuthorityEvidence(block_id, ...)

AcceptedBlockRecord = Canonical(Proposal, ConsensusEvidence)
```

`ConsensusEvidence` may be a quorum certificate, declared work/ancestry and confirmation
context, or a trusted sequencer signature according to the selected consensus profile. It is never inside the
`block_id` it authenticates. Consensus-specific header fields must replace, not informally
reinterpret, generic fields such as `round_or_view`.

The header's `profile_id` MUST resolve to one admitted `PROFILE-1` record whose candidate bytes,
scope/genesis projections, and capability closure all verify. A reproducible `RELEASE-R1` candidate
build does not satisfy that lookup, while artifact deployment separately requires a matching
`RELEASE-1` record. Because `profile_id` is outside the execution-root preimage but inside
`block_id` and proposal signatures, it contextualizes the root without making the candidate
identifier depend on a root that already contains itself.

`execution_parameter_root`, `governance_authority_root`, and `active_validator_root` are the
parent-authorized values governing this proposal. `optional_next_validator_root` and the
post-execution root commit successor changes. No header label may ambiguously mean both a
pre-transition authority and a post-transition result. `execution_policy_id` in each receipt
names the exact versioned execution, no-op, fee, and failure rules selected by
`execution_parameter_root`. `consensus_policy_id` and `data_availability_policy_id` likewise
name uniquely versioned policies authorized by the parent parameter/governance state;
`optional_next_validator_root` MUST equal the delayed-next schedule committed in the
post-execution `Q` state.

`proposer_identity`, key ID, and signature/hash suite IDs MUST resolve to exactly one active
proposal credential in the parent-authorized validator record. A proposer cannot select an
alternate key or downgrade suite merely by changing header fields.

```text
execution_policy_id = H(Canonical(
    execution_policy_domain, protocol_version, execution_parameter_root
))
consensus_policy_id = H(Canonical(
    consensus_policy_domain, protocol_version,
    execution_parameter_root, governance_authority_root
))
data_availability_policy_id = H(Canonical(
    availability_policy_domain, protocol_version, execution_parameter_root
))
```

`AcceptedBlockRecord` is a neutral carrier name. A deterministic-finality profile may call
it finalized only after its finality rule passes. A probabilistic profile must retain its
confidence/confirmation status and cannot turn work ancestry into absolute finality by using
this envelope.

A valid proposal requires all of the following to recompute exactly: parent/height/epoch;
proposer eligibility and signature; canonical block context, candidate execution context,
availability payload, body, body root, and transaction root;
transaction and block resource limits; sequential deterministic execution from the
pre-state; ordered receipts and receipt root; post-state and history roots; execution-
parameter, governance-authority, and active-validator roots authorized by the parent
accepted under the selected consensus rule;
parent; complete receipt/effect/witness bytes; and a data-availability commitment recomputed
under the declared policy. Acceptance at the claimed consensus status additionally requires
valid profile-specific evidence—deterministic finality or the declared probabilistic
confirmation threshold. A mismatch
in any required field invalidates the proposal rather than producing a partial successor.

Each included transaction and deterministic system transition has a canonical,
position-specific receipt:

```text
effects_root_j = CommitOrdered(
    execution_effect_sequence_domain,
    effects_in_subject_order_for_receipt_j
)

ReceiptSubject =
    UserTransaction(tx_id, transaction_index)
  | SystemTransition(system_phase, system_rule_id, system_transition_ordinal)

ReceiptCore = Canonical(
    ReceiptSubject,
    included_height,
    result,
    pre_subject_state_root,
    post_subject_state_root,
    nonce_disposition,
    fee_disposition,
    effects_root_j,
    execution_policy_id
)

Receipt = Canonical(ReceiptCore, effects_in_subject_order)

OccupancyEffect = Canonical(
    occupancy_effect_tag,
    effect_index,
    optional_source_action_index,
    event_position,
    status,
    coordinate,
    pre_occupancy,
    post_occupancy,
    optional_mint_id,
    optional_consumed_mint_id,
    optional_burn_event_id
)

Effect =
    OccupancyEffect
  | OwnershipOrTransferEffect(...)
  | BalanceFeeRewardOrRentEffect(...)
  | AuthorizationEffect(...)
  | GovernanceOrUpgradeEffect(...)
  | ValidatorScheduleEffect(...)
  | other versioned tagged execution-domain effect
```

`receipts_root` commits the full ordered receipts, so it transitively binds each effect list
and its per-receipt `effects_root_j`. `block_id` is deliberately not inside a receipt
whose root helps form that identifier. A
receipt-inclusion proof links the receipt and its canonical subject position to `receipts_root`; an
authenticated header links that root to `block_id`; detached consensus evidence establishes
its consensus status. Execution, inclusion, finality, and availability remain four separate
claims. A receipt hash alone proves only the receipt bytes and schema.
For a `SystemTransition` receipt, `nonce_disposition` and `fee_disposition` are the canonical
`NotApplicable` variants; rewards, rent, migrations, and other system changes appear as
tagged effects instead.

### 4.5 Genesis, versioning, and upgrades

`ProfileScopeV1` MUST contain the selected transition/schema versions, the canonical pre-ID domain
registry, cryptographic suites, consensus/governance/role choices, optional feature set, and
proposed limit vector. `GenesisPayloadV1` MUST reference that scope and domain-registry digest and
contain, at minimum:

- `chain_id` and the first profile's single `world_id`;
- the origin coordinate and complete initial mutable world/episode state;
- initial authorization, parameter, governance, and validator-schedule state;
- the validated resource, fee-if-activated, and size limits; and
- bootstrap and availability policy.

The candidate profile then references `profile_scope_digest`, `domain_registry_digest`, and
`genesis_id` and adds only the admission metadata defined in §2. Any denormalized suite, role,
feature, limit, state-domain, or root-schema value must be the exact canonical projection of its
owner. `genesis_id` and then `profile_id` are derived in that order. `TRANSITION-SPEC-1@profile` is then produced separately by
resolving the candidate's active extension slots against the base contract and validated bounds;
the candidate does not pre-commit a self-dependent resolved-specification digest. Every block,
proof, and signed action MUST identify the version and admitted `profile_id` context that
interpreted it. Unknown versions, unadmitted profiles, missing genesis data, unequal projections,
and incompatible parameter roots MUST fail closed.

Completing these logical bytes is not genesis-root evidence. `GENESIS-ROOT-PARITY-1` later rebuilds
the profile-required initial root tuple independently from `GenesisPayloadV1`; only after that gate
may the candidate publish the roots used by block-zero/header verification.

An upgrade is a deterministic, consensus-accepted state transition, not a changed constant in one
binary. Its record MUST bind the old and new protocol versions, activation height, migration
function or code hash, old root, expected new root, parameter deltas, and authorization
proof. Every validator replays the same bounded migration. Bedrock, supply, ownership, and
authorization changes must appear as explicit deltas rather than emerge from serialization
drift. A cryptographic migration must prevent downgrade, define any dual-validation window,
and preserve verification of historical blocks. Emergency and rollback policy cannot
silently rewrite history that the selected rule treats as irreversible.

Cross-world or cross-chain transfers MUST be explicit bridge transactions with their own
proof and trust boundary; matching coordinate names do not imply shared state.

### 4.6 Programmability boundary

The current edit language is data entry, not executable code: blocks are memory and ticks
are the execution clock, but no block transforms another block autonomously. If the future
visual language adds programs, its instruction set becomes another consensus surface. It
will need a deterministic interpreter, a versioned grammar, bounded execution/gas, memory
limits, failure semantics, and a refinement proof or independent reference executor.

Program execution MUST remain separate from rendering and camera state. A visual pattern
can be a program input or a projection, but a pixel pattern is not authority. Randomness,
if a later rule needs it, MUST be supplied by an explicitly committed and bias-resistant
protocol source; wall-clock timing or a renderer seed is not consensus randomness.

## 5. Authenticated state commitments

The current world tag is a canonical content identity. A Layer 1 needs a commitment that
also supports independently verifiable membership and non-membership claims.

A proposed root hierarchy is derived from the pre-ID registry rather than from a hard-coded list of
optional roots. `CommitmentSetV1` holds roots only for active `RequiredDomainV1` values; composition
walks every registry slot and supplies a typed sentinel for each inactive one:

```text
required_domain_set = CanonicalSorted([
    entry.RequiredDomainV1
    for each ActiveDomainV1 entry in DomainRegistryV1
])

application_domain_entries = ComposeRegistrySlots([
    RootEntryV1(RequiredDomainV1, root_bytes)
  | InactiveDomainSentinelV1(domain_tag, composition_layer,
                             inactive_sentinel_schema_id)
    for each application-layer entry in DomainRegistryV1
], CommitmentSetV1.root_entries)

application_root = H(Canonical(
    application_state_domain,
    protocol_version,
    chain_id,
    genesis_id,
    world_id,
    profile_scope_digest,
    domain_registry_digest,
    application_domain_entries
))

protocol_domain_entries = ComposeRegistrySlots([
    RootEntryV1(RequiredDomainV1, root_bytes)
  | InactiveDomainSentinelV1(domain_tag, composition_layer,
                             inactive_sentinel_schema_id)
    for each protocol-layer entry in DomainRegistryV1
], CommitmentSetV1.root_entries)

execution_state_root = H(Canonical(
    execution_state_domain,
    protocol_version,
    chain_id,
    genesis_id,
    world_id,
    profile_scope_digest,
    domain_registry_digest,
    application_root,
    protocol_domain_entries
))
```

The base occupancy profile requires world, active-token, episode, authorization, nonce, parameter,
governance, and `VALIDATOR-SCHEDULE-1` domains. `active_token_root` commits `M_h`;
`episode_root` commits `E_h` under the selected history and pruning policy; authorization commits
current descriptors, delegations, rotation, revocation, and recovery; the validator-schedule domain
commits active and delayed-next schedules but not current-round votes. If a profile activates
program state, `PROGRAM-STATE-1` owns its typed domain and root. If it activates an asset or wider
economic state, the selected asset decision plus `ECONOMIC-STATE-1` own those typed domains;
validator incentive semantics are first specified separately by `VALIDATOR-ECON-D1`.

An inactive asset, program, fee, reward, stake, or other optional domain occupies its registry slot
with `InactiveDomainSentinelV1`; it has no `RequiredDomainV1`, root bytes, or valid proof. A domain
may instead be active with an empty root only if its owning decision defines one typed empty state
and canonical empty value. The composer never hashes an undefined placeholder. Entry tags are
unique and registry ordered. A missing required tag, duplicate tag, unknown tag, wrong layer/type/
tree-domain/root-schema/commitment-profile, active/inactive flip, root supplied for an inactive slot,
sentinel supplied for an active slot, or different ordering rejects. The required set, registry
digest, and every repeated scope/genesis/profile projection must match exactly.

The `commitment_profile_id` inside each `RequiredDomainV1` is the self-excluding identifier for that
domain's selected authenticated-map construction and proof policy under `COMMIT-1`; it is not the
candidate network `profile_id`. A network `CommitmentSubjectV1` binds the later `profile_id`,
`profile_scope_digest`, and `domain_registry_digest` externally to the exact active root set.

`profile_id` is deliberately not in `application_root`, `execution_state_root`, or any initial map
root preimage. Including it there would make root derivation depend on the candidate identifier
while the candidate's own projections describe the root domains and schemas. Scope/genesis
identifiers, protocol version, the authenticated parameter roots, and the typed domain entries
carry execution semantics. Once the candidate is admitted, the
`ProposalHeader`, transaction signing document, release metadata, and `AuthenticatedRootRefV1`
carry `profile_id` as external context and MUST project the same scope/genesis/version/root choices.
This avoids a profile/root identifier cycle without allowing two profiles to reinterpret one root.

The `ProposalHeader` in §4.4 then binds the execution root to its admitted profile, parent,
transaction/receipt/history commitments, consensus policy, active validator set,
governance/parameter roots, and data-availability commitment. `block_id` is the header identifier;
`ConsensusEvidence` is detached.

Application roots, execution-state roots, block identifiers, and consensus evidence must not
be interchangeable. A valid world root is not a valid finality certificate, and a valid vote
certificate is not a proof that a coordinate is occupied.

The current `COMMIT-R1` comparison keeps one controlled choice: a constrained fixed-key binary
Patricia trie versus a 192-level sparse Merkle tree. Either must define canonical child ordering,
empty-subtree values, coordinate encoding, and both occupied and absent proofs. No structure is
selected yet.

### 5.1 Separate Verkle feasibility lane

The user's Verkle question opens `VERKLE-R1`, a separate feasibility lane rather than a third arm
silently added to the controlled `COMMIT-R1` assay. The original
[John Kuszmaul, “Verkle Trees” paper](https://math.mit.edu/research/highschool/primes/materials/2018/Kuszmaul.pdf)
develops the use of vector commitments in a high-branching authenticated tree to reduce proof
bandwidth. The
[Ethereum Foundation's concrete structure overview](https://blog.ethereum.org/2021/12/02/verkle-tree-structure)
illustrates a 256-way design using vector commitments and a stem/suffix layout. Those sources
motivate a hypothesis for Bitblock—smaller aggregated witnesses for many coordinate or state
queries—not a selected construction or inherited security result.

`VERKLE-R1` must first pin the vector-commitment scheme, curve/field and transcript rules, opening
and batch-verification equations, key/value layout, absence and deletion semantics, canonical
encoding, and numeric limits. It must measure witness bytes and verification work against the same
logical maps and workloads as the binary candidates. The cost side must separately report update
and witness-generation work, group/field operations, storage churn, cache behavior, and worst-case
accepted inputs; a small witness does not imply cheap mutation or verification.

The lane must also account for the larger cryptographic and trusted-computing surface: generator
derivation or scheme-specific setup/custody assumptions, implementation and side-channel risk,
independent-library diversity, external review, historical-proof retention, and migration from any
already admitted root. A Verkle prototype cannot reuse Ethereum's key layout, curve choices,
security analysis, or deployment experience as Bitblock evidence by citation. It remains
research-only until its own canonical vectors, powered malformed-opening controls, independent
implementation, refinement story, cost measurements, and reversal/migration plan pass. Failure to
beat the selected simpler baseline by a predeclared material threshold closes the lane without
changing live authority.

This is also where the practical-infinity idea meets cryptography. Lazy chunks are good
storage engineering, but a commitment cannot leave “unloaded” ambiguous. Every coordinate
must have a deterministic default value, and every touched region must have a bounded proof
path. The current signed-64-bit domain is large and finite; a later wider or mathematically
unbounded coordinate model would require a versioned commitment format.

The current `world_tag` is a full sorted-coordinate hash. It has no durable world decoder and
no membership or non-membership witness, so proving one coordinate currently requires more
than presenting the tag. An authenticated map is a new data structure and migration, not a
new name for that local digest.

The user's intended balanced-trit hashing implementation and contract have not been located
in this repository. Separately, the examined Aethor TSH-512/PDH-512 constructions are already
disqualified from authority here: the recorded audit found a concrete padding collision,
unframed node composition, and an unused PDH depth argument. IterHash is diagnostic only and
makes no security claim. Those negative results do not evaluate an unidentified balanced-trit
construction. Any future ternary candidate must pin its source, alphabet, output size,
security claim, canonical framing, domains, independent implementation, adversarial vectors,
and cryptanalysis before a versioned migration is considered. Agreement with itself cannot
promote it into authority.

Every proof object SHOULD state its type and scope:

- **membership** — a coordinate/token/nonce/authorization or receipt is included under a
  named root and position;
- **non-membership** — an absent key has a scheme-specific non-membership witness;
- **execution** — an ordered block body and committed `BlockContext`, starting at the parent
  execution root, deterministically yield the complete receipt and state-root sequence;
- **receipt inclusion** — a receipt path reaches the header's `receipts_root`, and the
  authenticated header reaches `block_id`;
- **consensus status** — profile-specific evidence covers one `block_id` under the authority
  selected by its parent accepted under that consensus rule;
- **availability** — the bytes needed to replay an accepted block can be retrieved under
  the stated guarantee.

The verifier MUST reject a proof presented under the wrong root kind, schema, chain, genesis,
admitted profile, world, height, or protocol version. A compact digest without the corresponding witness is
only an identity, not a membership or availability proof.

## 6. Replication, consensus, and finality

Local parent-linked history explains how one process derived a state. Consensus answers a
different question: which history independent processes accept. A versioned claim MUST
separate at least these properties:

In the remainder of this paper, “finalized” means irreversible under a selected
deterministic-finality rule. A probabilistic profile must substitute “reached the declared
confirmation threshold,” carry the residual reversal probability and horizon, and must not
reuse absolute-finality wording.

- **execution validity** — every accepted successor is the deterministic result of the
  declared parent, context, body, and version;
- **prefix safety** — any two deterministically finalized histories accepted under the claim
  are prefix-comparable, not merely non-conflicting at one height; a probabilistic profile
  states the corresponding common-prefix probability and horizon;
- **finality** — the exact rule after which a block is irreversible under the stated fault
  assumptions, or the confirmation/confidence rule if finality is probabilistic;
- **availability** — consensus-required bodies and witnesses are retrievable under the
  declared retention, coding, sampling, and network assumptions;
- **chain liveness** — new valid blocks continue to finalize or reach the declared
  confirmation threshold after the stated synchrony condition while faults remain within
  the bound;
- **transaction inclusion/fairness** — the narrower conditions, if any, under which an
  eligible transaction is included;
- **accountability** — conflicting signed evidence can be attributed where the selected
  construction supports it; and
- **cross-implementation agreement** — conforming executors obtain the same receipts and
  roots for the same accepted inputs.

An **eligible transaction** is canonical, continuously available to the protocol's required
honest network fraction, valid or correctly resubmitted against canonical state, within its
validity window, paying the declared admission price, and submitted while sustained offered
load is within the measured capacity envelope after synchrony. Outside that envelope, the
protocol may claim only its explicit queue, expiry, and priority behavior—not eventual
inclusion. “Unless censored” cannot make liveness unfalsifiable.

All assumptions must be quantitative enough to test. They include the network model, the
maximum Byzantine and offline weight, quorum intersection, corruption timing, capacity,
and cryptographic hardness. “A majority is honest” is incomplete until the protocol says
which majority, over which parent-authorized set, for how long, and how membership changes.

A consensus protocol MUST specify:

- validator or sequencer membership and admission;
- the fault model and maximum faulty fraction;
- proposal and vote messages;
- round/view-change, timeout, and pacemaker behavior;
- quorum and certificate encoding;
- fork choice and equivocation handling;
- finality and recovery after partitions;
- validator key rotation and revocation;
- resource and fee policy for spam resistance;
- how a new node obtains and verifies the canonical history.

### 6.1 Finality, forks, and validator-set changes

For a certificate-based construction, each vote signs an unambiguous domain:

```text
VoteSignDoc = Canonical(
    vote_domain,
    protocol_version,
    chain_id,
    genesis_id,
    epoch,
    active_validator_root,
    height,
    round_or_view,
    vote_phase,
    block_id
)
```

Certificate verification MUST reject duplicate signers or duplicated voting weight, derive
power from the active set authorized by the parent finalized under that certificate profile,
and use exact checked threshold
arithmetic. Aggregation must retain signer attribution and apply the selected scheme's
public-key validation and proof-of-possession defenses. A candidate cannot choose the set
that certifies itself. A block may commit an `optional_next_validator_root`, but delayed
activation and a specified cross-epoch handoff authorize it only from a prior finalized
state.

Each validator vote key and signature/hash suite MUST resolve uniquely from the
parent-authorized active-validator record and `consensus_policy_id`; a vote message cannot
select an alternate key or downgrade suite.

These bindings are mandatory for a quorum-certificate design even though the consensus
choice remains open. A probabilistic work-based design instead needs a precise work/fork and
confirmation rule and must not label confidence evidence a certificate. A node MUST NOT
treat a state root as final merely because it is newest or locally observed unless that is
the declared consensus rule.

A validator MUST NOT vote for execution validity or finality until it has obtained all
consensus-required execution bytes, validated resource limits, and recomputed the transition,
unless the selected protocol separately specifies a sound validity-proof system bound to the
same `block_id`, body, receipts, and post-state root. Data-availability sampling or a DA
certificate never substitutes for execution validity.

A separately typed availability attestation may use sampling instead of full possession only
under a declared construction. It must state the sampling threshold, erasure-code and
honest-share assumptions, recovery threshold, withholding horizon, commitment-to-payload
binding, and exact weaker guarantee. A combined validity/finality vote must satisfy both the
execution rule and the DA rule. A public claim must say whether finality entails availability
or requires separate evidence.

In a deterministic-finality profile, a branch can be replaced before finality; after
finality, a conflicting root is a safety violation rather than an ordinary reorganization.
In a probabilistic profile, reorganization remains possible with the bounded probability and
tail described by its common-prefix/confirmation claim. Every profile MUST specify orphan
handling, transaction re-inclusion, nonce behavior, receipt status, and visibility of
provisional mint/burn records. Validator-set changes need an activation rule, overlap or
handoff proof, key revocation behavior, and protection against long-range or stale-checkpoint
attacks.

### 6.2 Ordering fairness and censorship

Consensus ordering is an authority surface. A proposer or sequencer can reorder, delay, or
exclude transactions even when it cannot forge signatures. The protocol SHOULD define
eligible-transaction conditions, inclusion deadlines, fee/priority rules, duplicate
suppression, maximum queue age, and an evidence path for censorship or equivocation. If no
fairness guarantee is intended, the liveness claim must say so explicitly.

Proof of History-like sequential clocking is not consensus or finality by itself. Proof of
Stake is a Sybil/economic weighting mechanism commonly combined with a finality protocol;
BFT is a fault-tolerant agreement family; Proof of Work supplies a work-weighted fork rule;
and a trusted central sequencer is an explicit single-authority construction. The current
SHA-256 history chain is none of these. A cheap Rust implementation can begin with one
authority and then a deterministic multi-node testnet, but staging is not decentralized
security evidence.

If a future design uses stake, it must define deposits, validator selection, unbonding,
slashing, evidence validity, recovery, and the attack cost. If it uses work, it must define
difficulty, chain-work comparison, issuance, and energy/security assumptions. If it uses
permissioning, the admission authority and compromise response are part of the trust model.
The choice remains open until the desired decentralization, cost, and fault model are
measured rather than assumed.

## 7. Threat model and required defenses

The protocol MUST name its adversary before selecting a consensus mechanism. Every versioned
safety, liveness, availability, custody, or economic claim cites the applicable parts of:

```text
AdversaryModel = (
    security_parameter_and_computational_bounds,
    static_or_adaptive_corruption_and_corruption_latency,
    maximum_byzantine_weight_and_maximum_offline_weight,
    network_power_to_observe_drop_delay_reorder_duplicate_partition_or_eclipse,
    key_client_rpc_storage_release_and_governance_compromise_powers,
    economic_or_bribery_budget_and_security_time_horizon,
    physical_and_side_channel_assumptions_or_exclusions
)
```

The initial threat inventory is:

| Threat | Required defense or explicit assumption |
| --- | --- |
| Forged action | Signature verification and canonical signed bytes |
| Ambiguous or malleable encoding | Typed length framing, strict decoding, canonical signatures, and negative vectors |
| Replay | Per-identity nonces, chain/genesis/profile/world binding, selected anchor rules, and bounded retention |
| Double spend / duplicate occupancy | Single canonical state transition and live-token checks |
| Same-coordinate race | Consensus-defined ordering and deterministic conflict results |
| Validator equivocation | Signed votes, conflicting-certificate detection, and penalty/expulsion policy if applicable |
| Fork or rollback | Declared consensus/finality rule and evidence, checkpoint policy, and recovery rules |
| Data withholding | Full transaction availability, archival policy, and retrievability proofs or explicit trust boundary |
| Sybil participation | Admission, stake, proof-of-work, permissioning, or another declared cost |
| Spam and unbounded coordinates | Fees, quotas, work budgets, chunk limits, and allocation-failure handling |
| Malicious implementation | Independent executor, differential replay, fuzzing, and formal refinement where practical |
| Hash or signature failure | Versioned cryptographic suite, conservative assumptions, and migration path |
| Key loss or compromise | Rotation, recovery, revocation, and incident procedures |
| Validator restore or signer failover | Durable anti-double-sign state, restore monotonicity, and slashing/equivocation protection |
| Clock manipulation | Logical height for validity; wall-clock only for scheduling/timeouts |
| Client/render mismatch | Keep pixels non-authoritative; validate submitted semantic actions |
| State corruption | Checksummed storage, parent-root validation, snapshots, and restore tests |
| Malformed peer or parser exhaustion | Strict codecs, bounded allocation/work, backpressure, fuzzing, and fail-closed disconnect |
| RPC/indexer deception | Return proofs with queries, verify roots locally, and treat unproved API responses as hints |
| Mempool replacement or branch confusion | Explicit lifecycle, expiry, replacement, and finalized-versus-provisional status |
| MEV/order manipulation | Declared ordering/fairness policy, inclusion evidence, and bounded queue rules |
| Eclipse or peer isolation | Peer diversity, authenticated discovery, anti-eclipse checks, and multi-source sync |
| Long-range or stale checkpoint attack | Finality anchors, validator-set history, checkpoint provenance, and recovery policy |
| Randomness bias | No consensus randomness by default; use a committed protocol beacon if required |
| State-growth/rent exhaustion | Deterministic size, storage, and lifetime accounting; pruning and rent policy |
| Supply-chain or binary substitution | Reproducible builds, pinned toolchains/dependencies, signed releases, and independent executors |
| Privacy leakage | Explicit public-state model, metadata analysis, and cryptographic privacy scheme if required |
| Bridge/oracle compromise | Keep external claims outside base consensus or specify authenticated adapters and rollback rules |
| Governance capture | Explicit upgrade authority, timelocks, emergency rules, and audit trail |
| Upgrade or migration divergence | Committed bounded migration, old/new root checks, downgrade prevention, and cross-version replay |
| Operator/admin compromise | Least privilege, authenticated admin/RPC, pinned genesis/config, secret-safe logs, and incident isolation |

No defense is free. Each row needs a named assumption, version, artifact, test or proof
target, status, and halt condition. The threat inventory is not itself a proof or an
exhaustive production matrix; new findings append rows rather than being forced into an
existing label.

## 8. Availability, storage, and recovery

An L1 cannot be only a chain of hashes. A new node must be able to recover the bytes needed
to recompute the state. The future protocol MUST define:

- canonical block and transaction storage;
- snapshots/checkpoints and replay limits;
- archival retention and pruning rules;
- peer discovery and authenticated transport;
- data-availability commitments and sampling, if used;
- crash recovery and atomic publication of state roots;
- detection of a missing or conflicting segment;
- recovery after a node loses local storage;
- a reproducible genesis package containing protocol parameters and initial state.

The current research journal and typed coordinate-history layer are useful experiments, but
they are not external custody or rollback resistance. A local file, even if hash-linked,
does not establish availability to an independent observer.

### 8.1 Network and synchronization surface

The future network protocol must distinguish at least these roles:

- **full verifying nodes** — retrieve and execute blocks and verify roots/finality but need
  not propose or vote;
- **consensus validators/proposers** — perform the selected proposal/vote role and satisfy
  the selected data-possession rule before signing;
- **archive/recovery nodes** — retain historical blocks, receipts, proofs, and snapshots;
- **light clients** — verify headers, consensus evidence/status, and state/receipt witnesses without
  trusting a server's claimed coordinate state; and
- **optional RPC/indexer nodes** — accelerate queries but remain non-authoritative.

Peer discovery, handshake, capability negotiation, request authentication, message size
limits, deduplication, backpressure, retry, and disconnect behavior are part of the attack
surface. A node MUST be able to reject a peer that supplies a valid-looking hash with no
replayable body, and it SHOULD obtain critical headers/data from more than one independent
peer to reduce eclipse risk.

RPC and indexer responses are query surfaces, not authority surfaces. A client that asks
whether a coordinate is occupied or who owns a token SHOULD receive the relevant root and
witness, then verify it locally. Pagination, historical queries, and event indexes need a
consistency rule so an API cannot mix records from different verified consensus-status roots without saying
so.

Bootstrap MUST name its trust anchor: a pinned genesis package, a verified checkpoint, or a
declared weak-subjectivity source. A snapshot is admissible only after its state root,
validator-set root, consensus evidence/status proof, and parameter/version root are checked. A new node must
also obtain the bodies or witnesses required by the declared availability rule.

The protocol must choose its recovery guarantee. **Genesis replay** requires durable archival
availability of every required historical body and every referenced protocol implementation.
**Checkpoint or weak-subjectivity bootstrap** trades that requirement for a named trust/freshness
assumption. A pruned network cannot promise mandatory full replay unless separate archive
nodes actually provide it. If both paths are claimed, both must be tested independently.

### 8.2 Persistence and publication atomicity

Publishing a block, its state root, and its availability record must be crash-safe. The
protocol/storage boundary needs an ordering such as:

```text
receive -> validate -> durably stage body -> execute -> durably stage roots
         -> publish candidate -> verify selected consensus status -> publish accepted index
```

The exact order is an **Open** implementation choice, but a crash between any two steps
must leave a recoverable prefix rather than a root that claims bytes the node cannot
reconstruct. Power-loss, torn-write, corruption, restore, pruning, and recovery tests are
required before a custody or durability claim.

Validator recovery has an additional monotonicity requirement: restoring a backup or moving
to a failover signer MUST NOT regress vote, nonce, slashing-protection, accepted-head, or
upgrade state and thereby authorize a double-sign or stale history. Recovery tooling must
verify exact roots and authority epochs before proposal/vote service resumes.

### 8.3 Operational security, monitoring, and safe halt

Production operation is part of the security envelope. Nodes SHOULD separate consensus,
execution, networking, RPC/indexing, administration, and signing privileges; authenticate
admin/RPC surfaces; pin genesis and configuration; keep secrets out of logs; and verify
signed, reproducible updates with a downgrade-safe rollback procedure.

Operators must monitor finalized or threshold-confirmed head continuity, conflicting
deterministic-finality evidence, probabilistic fork-choice anomalies, validator
participation and double-sign warnings, active/next validator roots, availability retrieval,
peer diversity/eclipsing, execution-root agreement, storage and recovery health, supply
correspondence, resource saturation, and pending upgrades. Alerts must carry the exact
height, roots, version, and signed evidence needed for independent diagnosis. Metrics are
diagnostic projections, never consensus authority.

A node MUST enter a declared safe-halt mode—stop proposing, voting, or publishing new
accepted indexes—when evidence violates the selected profile's safety rule, it observes an
unknown active version, consensus-required bytes remain unavailable beyond the permitted
horizon, a state-root mismatch is irrecoverable, or anti-double-sign state has regressed.
Ordinary competing branches in a probabilistic protocol go through its fork-choice and
confirmation rules; their mere existence is not a safe-halt trigger. Safe halt does not automatically
roll back or rewrite finalized history. The incident procedure must define evidence capture,
key isolation/revocation, peer quarantine, resynchronization, coordinated upgrade handling,
restart criteria, and a public post-incident account for any widened trust assumption.

## 9. Determinism and formal refinement

The single most important inherited property is replayability across every activated
protocol version. Independent chain validation consumes the complete genesis package, each
canonical `AcceptedBlockRecord`—including availability payload, full receipts/effects,
proposal signature, and consensus evidence—and an implementation/specification for every
version referenced by that history:

```text
(genesis_package,
 canonical AcceptedBlockRecord sequence,
 implementation/specification for every referenced protocol and consensus version)
    -> exactly one verification disposition,
       execution-state-root sequence,
       receipt-root sequence,
       and finality-or-confirmation status sequence
```

Version activation, validator scheduling, committed randomness, rewards, governance, and
migrations come from accepted header/state context. Ordered user actions alone are not a
complete replay input once any of those mechanisms exists. An execution-only replay may
start from a supplied trusted parent and omit consensus evidence, but its claim MUST then say
that parent selection and finality verification were out of scope.

To make that a network property rather than a local intention:

1. Canonical bytes MUST have one decoder and one serialization.
2. All consensus-relevant integers, floats, limits, and error dispositions MUST be
   specified. Consensus should avoid unrestricted floating-point camera state.
3. The Rust executor SHOULD be compared with an independent reference implementation.
4. Property-based tests MUST cover idempotence, bedrock preservation, re-mint identity,
   nonce monotonicity, and conflict ordering.
5. The Lean model SHOULD grow from abstract occupancy/accounting into a refinement theorem
   for the chosen canonical state transition.
6. A node must fail closed on an unknown protocol version, malformed proof, missing parent,
   or resource-bound violation.

The authoritative execution path must be auditable separately from non-authority paths:

- client input normalization may propose an action but cannot decide validity;
- renderers, sidebars, metrics, and camera floats are projections and cannot mint, burn, or
  finalize state;
- learned or heuristic components, if ever used for tooling, cannot influence consensus
  validity or certificate creation;
- randomness and wall-clock readings cannot enter execution unless they are explicit,
  committed protocol inputs with a defined bias/failure model.

The release itself is part of the proof boundary. A production candidate SHOULD publish
reproducible build instructions, pinned compiler/toolchain/dependency versions, source and
binary digests, signed release metadata, an independent build, and a procedure for handling
a compromised dependency or compiler. A consensus proof over one local binary does not
cover a substituted binary.

Release production also has two phases. `RELEASE-R1` may exercise the reusable build/reproduction
framework and emit a clearly labeled candidate artifact bound to `CandidateProfileV1` and its
`profile_id`. That artifact is not an admitted network release. Only after `PROFILE-1` binds the
same `profile_id` to a complete passing evidence closure may `RELEASE-1` publish profile-authority
metadata that also binds the admitted-manifest digest, exact source, features, genesis, binary, and
configuration. A candidate build must never inherit “admitted” from reproducibility alone, and an
admitted record must reject any semantic projection or binary that differs from its candidate.

The current Lean capsules prove only their declared abstract finite-support models: bedrock
and edit/replay properties plus per-edit supply increments, decrements, and no-op
preservation. They do not prove token identifiers, consuming-burn provenance, a global
episode-ledger theorem, Rust refinement, exact camera floats over the full coordinate domain,
or any network implementation.

## 10. Economic semantics

Cryptographic validity and monetary value are separate. Even a perfectly replicated token
ledger does not create a market.

The protocol must decide:

- whether tokens are transferable;
- whether they are fungible, unique, or both via separate asset classes;
- whether minting is unlimited, capped, scheduled, or resource-priced;
- whether burning is voluntary or required, and whether a later explicit compensating
  mint/recovery transition is permitted without rewriting the finalized burn;
- who pays transaction and storage costs;
- whether validators receive rewards;
- whether failed actions pay fees;
- whether an occupied coordinate is scarce enough to have intended value.

The default Bitblock rule has a particularly important consequence: place/delete loops
preserve active supply but can grow history without bound. A production protocol therefore
needs a resource policy even if it never assigns financial value to a block.

Fees and balances require their own state semantics. The protocol must choose an account,
UTXO, or capability model; define fee-payer authorization; specify whether fees are burned,
paid to a proposer, or split; and make failed-transaction charging deterministic. A block
token cannot silently double as a validator stake, a fee balance, and a user-owned asset.
Those assets need distinct domains, roots, and transfer rules.

If validator rewards or stake are introduced, the issuance schedule, reward timing,
unbonding delay, slashing evidence, and maximum monetary supply become consensus state.
Their economics must be analyzed against censorship, bribery, collusion, and griefing rather
than inferred from occupancy scarcity. Resource pricing must cover worst-case signature and
proof verification, execution, bandwidth, state writes, persistent history, witness
generation, and recovery—not only block count or payload bytes.

State rent, leases, storage deposits, or pruning credits may be needed to stop the large
coordinate space from becoming an unbounded obligation for every validator. Any such rule is
a state transition and MUST define its payer, price-update rule, grace period, expiry height,
warning/evidence path, automatic action, token/ownership/supply effect, reactivation right,
and behavior during partitions. Expiry cannot silently delete a coordinate or token outside
the same atomic accounting rules as an ordinary burn or transfer.

## 11. Layered implementation plan

The following checkpoints preserve the current cheap Rust core and add one security surface
at a time. They are conceptual security milestones, not the current scheduling authority. Exact
dependencies and status live only in [`CAPABILITY_TREE.md`](CAPABILITY_TREE.md): checkpoint A maps
primarily to `ACCOUNT-1`, `LOCAL-BOUNDS-1`, `STORAGE-1`, and `LEDGER-1`; B to
`TRANSITION-SPEC-D1`, identity/authorization, `EXECUTOR-1`, and `MULTI-1`; C to the commitment,
execution-root, sequencer, and synchronization nodes; D to the consensus/profile/testnet nodes; E
to `L1-CANDIDATE`; and F to `L1-PRODUCTION`. A later tree correction therefore does not require
silently rewriting this historical milestone summary.

### Checkpoint A — local occupancy ledger

Promote and extend the existing feature-gated, in-memory coordinate-episode research path
into an optional authoritative local ledger wrapper. This is a change of status and
durability, not a claim that no prototype exists.

Required properties:

- one live mint record per mutable occupied coordinate;
- one consuming burn record per deletion;
- no mutable occupancy-episode token for bedrock; any separate locked genesis allocation remains an
  explicit `ASSET-D1` choice;
- fresh token identity after re-placement;
- explicit no-op/rejection records and a reconciled submitted/admitted/executed/result lane;
- exact active-map correspondence, injective token identities, and parent-root checks;
- deterministic canonical bytes and a bounded replay limit.

This checkpoint proves provenance and conservation. It is not multiplayer or currency.

### Checkpoint B — two-observer deterministic harness

Keep observers as signed action streams, not entities. Run two or more identities through the
same in-process tick reducer and test:

- same-coordinate races;
- ownership and delegated edit rights;
- nonce/replay failures;
- deterministic ordering;
- independent replay from the same genesis.

This is the cheapest place to find semantic ambiguity.

### Checkpoint C — authenticated centralized multiplayer

Introduce a server/sequencer that validates signed actions, chooses canonical batch order,
stores the append-only log, and returns receipts with state roots. Clients can independently
replay every accepted batch. The server is still a trust and availability boundary; the
system should be described as a cryptographically accountable game ledger.

### Checkpoint D — replicated testnet

Run independent Rust processes with a declared consensus prototype. Test partitions,
duplicate proposals, equivocation, restart, missing data, slow validators, and conflicting
roots. Do not call this secure until safety and liveness assumptions are measured against
adversarial scenarios.

### Checkpoint E — Layer 1 candidate

Freeze the transaction, block, root, proof, and consensus schemas. Publish an independent
executor, genesis package, threat model, validator assumptions, upgrade rules, and recovery
procedure. Require external review before production value is introduced.

### Checkpoint F — production security claim

Only after the previous gates can the project make a scoped Layer 1 claim. The claim must
name the exact version, validator assumptions, finality rule, availability guarantee,
cryptographic assumptions, and excluded properties. “Layer 1” must never mean “the hash
looked convincing”.

## 12. Verification gates

Each gate answers one question and must not be promoted into a broader claim. “Bounded” below
describes the current evidence class; this documentation pass did not rerun it.

| Gate | Claim boundary | Required passing evidence | Current status / stop condition |
| --- | --- | --- | --- |
| G0 | Current local occupancy invariants | Pinned Rust tests and exact abstract Lean theorems | **Bounded**; no Rust-refinement claim |
| G1 | Feature-gated coordinate-episode correspondence | Map-domain/injectivity, mint/burn, re-mint, and origin vectors | **Partial bounded/local; OPEN**—the independent vector does not replay history, and no durable asset claim exists |
| G2 | Complete action/result accounting | Reconcile live submission, tape, tick outcome, edit history, rejection, and idle/control lanes | **Open**; any silently lost included action stops admission |
| G3 | Canonical codec, genesis, and domain separation | Cross-implementation corpus, prefix/suffix/duplicate/unknown-field negatives, collision controls | **Open**; any alternate accepted bytes or ID collision stops the version |
| G4 | Actor authentication and replay safety | Signature, key-validation, nonce, chain/genesis/profile/world, anchor, rotation/recovery, and mutation controls | **Open**; any unauthorized or cross-domain replay stops admission |
| G5 | Transaction/interleaving semantics and conservation | **G5.core:** multi-actor schedules; failure/no-op/nonce atomicity; occupancy/episode conservation. **G5.asset:** transfer, fee, and wider asset/supply conservation only when activated | **Open; G5.core is mandatory**—any executor-dependent disposition or partial write stops every candidate. G5.asset is conditional, not a reason to exclude the core |
| G6 | Authenticated state and receipt proofs | Scheme-specific membership/non-membership plus receipt path through an authenticated header accepted at the selected consensus status | **Open**; a bare digest or unbound witness does not pass |
| G7 | Prefix safety and finality/confirmation | Formal argument plus partition/equivocation/fork adversarial harness under the named fault model | **Open**; evidence violating the selected profile's safety rule triggers safe halt, while ordinary probabilistic forks use fork choice |
| G8 | Chain liveness and eligible inclusion | Protocol/model argument for the scoped eventual claim plus bounded fault-injection measurements with synchrony, capacity, fee, and dissemination preconditions | **Open/bounded when run**; samples alone never prove an unstated universal |
| G9 | Availability, durability, and recovery | DA-before-vote controls, fresh-node recovery, pruning/archive, crash, power-loss, and restore tests | **Open**; unavailable required bytes or regressed signer state stops service |
| G10 | Implementation/specification refinement | Independent executor across every activated version plus formal or differential evidence | **Open**; same-code self-replay is insufficient |
| G11 | Cryptographic/key-operations boundary | Published TCB and assumptions, ceremonies, compromise drills, independent audit, dispositioned findings | **Open**; merely receiving an audit is not a pass |
| G12 | Validator-set transitions and partitions | Parent-authorized active/next roots, certificate vectors, handoff/quorum proof, long-range recovery | **Open**; candidate-selected authority never passes |
| G13 | Peer sync and bootstrap | Multi-peer body/witness retrieval, parser/backpressure controls, checkpoint/genesis trust tests, eclipse tests | **Open**; one unproved peer response is not authority |
| G14 | Provisional versus accepted client state | Reorg, receipt-inclusion, nonce/replacement, wallet proof, and finality/confirmation-status tests | **Open**; a candidate receipt cannot be presented beyond its verified status |
| G15 | Resource and economic bounds | **G15.resource:** worst-case codec/signature/proof/execution/storage/history/witness/recovery measurements. **G15.economic:** fee/rent/denomination invariants only when activated | **Open; G15.resource is mandatory**—an over-bound remote input rejects/disconnects. Safe halt is reserved for a consensus-accepted artifact or local invariant that cannot be processed within the already committed bound; limits are never silently raised |
| G16 | Reviewed source versus shipped binary | Pinned toolchain/dependencies, reproducible independent builds, signed release metadata | **Open**; unmatched binary has no inherited proof claim |
| G17 | Upgrade/governance migration | Old/new root vectors, bounded migration replay, authorization/timelock, downgrade and historical-verification tests | **Open**; divergent migration roots stop activation |
| G18 | Production operations and incident response | Privilege/RPC tests, monitoring controls, backup/failover drill, safe-halt and key-compromise exercise | **Open**; recovery cannot resume through an unresolved authority/root fault |
| G19 | Future programs and randomness | Versioned VM/gas/refinement tests and committed-randomness adversarial analysis | **Open** until either feature exists |

A production coverage matrix must map each public property and threat row to the exact
protocol version, adversary assumption, gate, artifact, result, and failure condition. A
green G0 or G1 does not imply G7. A matching hash does not imply liveness or availability.
A passing network harness does not prove physical display output or economic value.

For every base occupancy profile, `G5.core` and `G15.resource` are required. Only the asset/economic
subparts of G5/G15 and the program/randomness features in G19 are conditional on activation; their
absence cannot be used to exclude core transition or resource evidence. Stage ownership still
matters—G18 is production work rather than a pre-testnet pass—but a later stage cannot silently
remove a base gate that its profile already requires.

## 13. Open decisions

These are deliberately not silently decided by this paper:

1. Is a block an occupancy token, an edit right, a fungible resource, or a dual asset?
2. Does ownership attach to coordinates, token episodes, or permissions?
3. Does identity use a stable actor descriptor or public-key-hash addresses, and what exact
   threshold, delegation, rotation, revocation, and recovery rules apply?
4. Which signature, hash, aggregation, and canonical-key suites are accepted for the first network version?
5. Which authenticated map supports sparse coordinates and scheme-specific non-membership proofs?
6. Which consensus/finality model matches the desired trust, availability, and cost assumptions?
7. What adversary, synchrony, capacity, validator-fault, and quorum-intersection assumptions apply?
8. Which validator-set activation, checkpoint, handoff, and long-range-attack rules are required?
9. Is the coordinate domain permanently signed-64-bit, and will a chain always contain one world?
10. Are observer reach/collision rules consensus state or local interaction rules?
11. Does the protocol retain deterministic application sub-ticks, and how do they map to block context?
12. Is one transaction one operation or a bounded vector, and what are its atomicity, no-op,
    admitted-failure, nonce, and fee dispositions?
13. Does a transaction require an exact parent, a windowed anchor, an expected-state precondition,
    or no state anchor, and how are stale transactions replaced?
14. Does the network retain `toggle`, require an expected-state precondition, or lower it to `on`/`off`?
15. Is mint identity transaction-stable or inclusion-stable across provisional forks?
16. Which transaction lifecycle, re-inclusion, and reorganization receipt rules are canonical?
17. Which live inputs and local rejections must be retained so submitted, admitted, executed,
    finalized-or-confirmed, rejected, camera-only, and idle lanes reconcile?
18. What exact fee, storage-rent, state-growth, and failed-action charging rules prevent denial of service?
19. What data-availability, archival, pruning, genesis-replay, checkpoint, or weak-subjectivity guarantee applies?
20. Which full-verifier, consensus-validator, archive, light-client, and RPC/indexer roles and retention windows exist?
21. What upgrade, migration, key-recovery, safe-halt, emergency, and governance rules are acceptable?
22. Is privacy required, and if so, which state is public versus concealed?
23. What exact versioned claim will “Layer 1 security” be allowed to mean for Bitblock?
24. Will a future visual program be consensus code, user data, or a separate non-authoritative projection?
25. What randomness source, if any, is admissible to a deterministic future program?
26. What reproducible-build, dependency, key-ceremony, audit, monitoring, backup, and incident-response standard is required?
27. Which external bridges or oracles, if any, are allowed to make claims outside the coordinate world?
28. Which actions are actor-authorized, validator-authorized, governance-authorized, or emergency-authorized,
    and how are those namespaces enforced?
29. Does `VERKLE-R1` beat the selected simpler hash-tree baseline by a predeclared end-to-end
    witness, update, verification, dependency/TCB, setup-generator, availability, and migration
    threshold strongly enough to justify a later profile candidate, or should the lane close?

Each decision should be recorded with its assumptions, rejected alternatives, falsifier, and
the smallest test that could disprove the chosen design. Until then, the current system
should retain its narrower and truthful description.

## 14. Pinned checkpoint-0.3 audit result: obligations still open in the repository

The checkpoint-0.3 paper at commit `034b7b2b5f129625f9c6dcec3e14038b7c4aee5f` was checked
against source/proof snapshot `6545d8e7d60e75607445fda564bbf11610a62658`, the Lean/LadderLang capsules,
`docs/SEMANTICS.md`, `docs/HANDOFF.md`, and the open-obligation and hash-suite sections of
`docs/PROTOCOL.md`. The successor corrections named at the top of this paper were scoped planning
consistency sweeps, not another complete paper audit. The following gaps remain intentionally
open. They are not covered by the existence of this paper:

| Repository boundary | What is still missing before a Layer 1 claim |
| --- | --- |
| Rust world versus Lean world | A refinement proof or independently checked correspondence for the chunked implementation |
| Canonical byte domains | Independent corpus parity for every authority-relevant schema and key/signature codec |
| Local journal/storage | Durable journal receipts, process-death/power-loss evidence, external custody, and rollback detection |
| History branches | Independently retained heads and a canonical fork/latest-history authority |
| Tick sidecar | Independent replay/refinement and shared durable storage/pruning for consensus-accepted outcomes |
| Action/result completeness | One canonical reconciliation of live submission, aggregate non-edit controls, ordered edit attempts, misses/rejections, camera-only actions, idle ticks, admission, execution, and finalization |
| Coordinate episodes | Promotion of the feature-gated, bounded in-memory sidecar into durable, independently verifiable asset evidence |
| World commitment | An authenticated sparse map, durable decoder, and membership/non-membership witnesses instead of the current full sorted-coordinate tag |
| Local replay evidence | A durable supplied-carrier verifier and independent executor; ordinary stdout and locally self-verified in-memory evidence are not run receipts |
| Sequential/elapsed-time evidence | No VDF/sequentiality analysis or external anchor; serial SHA-256 history is not Proof of History |
| LadderLang bridge | The current bridge is representation-focused and does not prove live edit execution or network authority |
| Network | Peer protocol, sync, validator messages, consensus/finality evidence, data-before-vote rule, and adversarial multi-node evidence |
| Cryptography | Pinned signature/hash suite, key lifecycle, proof witnesses, and migration controls |
| Economics | A chosen asset/account model, fees/rent, validator incentives, and supply policy |
| Ternary hashing | The intended balanced-trit implementation, exact contract, independent vectors, and analysis; examined Aethor TSH/PDH and IterHash remain non-authoritative |
| Operations | Privilege separation, monitoring, safe-halt, backup/failover, key compromise, and incident evidence |
| Upgrades | Deterministic migration format, old/new-root replay, authorization, downgrade, and historical-verification evidence |
| Programmability | A versioned deterministic VM and refinement/resource proof if blocks become executable |
| Presentation | Physical display evidence remains outside the Layer 1 state-machine claim |

The current branch therefore has a broad, versioned **design inventory of the presently
named surfaces**, not a proof of completeness and not security coverage. A future
implementation checkpoint should remove one row only when its evidence is independently
reproducible and its assumptions are written into the public claim.

## Conclusion

Bitblock already has the right seed for a verifiable system: a small semantic core,
an ordered tick boundary, sparse storage, canonical bytes, parent-linked history, and
bounded proofs. What it lacks is not more visual complexity. It lacks the authority and
replication layers that let independent participants agree on who did what, in what order,
with what finality and availability.

The strongest path is therefore incremental:

```text
binary occupancy
  -> deterministic local ledger
  -> signed multi-observer actions
  -> authenticated centralized history
  -> replicated consensus testnet
  -> independently audited Layer 1 candidate
```

At every arrow, the project should state exactly what has become provable and exactly what
remains outside the claim. That discipline is part of the security model.
