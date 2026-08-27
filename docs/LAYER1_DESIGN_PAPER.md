# Bitblock: From Binary Occupancy to a Verifiable Layer 1

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
| Abstract proof | Lean models cover bedrock/edit replay and per-edit finite-trace supply deltas; not token identity or Rust refinement | Bounded |
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
B_t : C -> {0, 1}
B_t(o) = 1
```

The absence of a stored bit is semantically `0`, not “unloaded” or “unknown”. Storage is
an implementation of this function; it is not the definition of the world.

A future deterministic execution state can be written as:

```text
ApplicationState_h = (B_h, M_h, X_h)
ProtocolState_h    = (A_h, N_h, P_h, G_h, Q_h)
ExecutionState_h   = (ApplicationState_h, ProtocolState_h)

FinalizedHead_h = (height, block_id, execution_state_root, history_root,
                   availability_claim)

NodeLocalView_i = (UI_i, mempool_i, candidate_branches_i, current_votes_i,
                   timeouts_i, peers_i, sync_i, caches_i)
```

where:

- `B_h` is the occupied-coordinate function;
- `M_h` is the active mint map from mutable occupied coordinates to token identifiers;
- `ApplicationState_h` MAY also contain `X_h`, the selected ownership, transfer, balance,
  fee, or stake state. `X_h` is empty until an asset/economic model is chosen, but any later
  asset domain MUST have its own canonical state and root;
- `A_h` is committed authorization, identity, delegation, rotation, and revocation state;
- `N_h` is committed per-identity replay state such as nonces;
- `P_h` is committed protocol-version, codec, resource, and economic parameter state;
- `G_h` is committed governance, upgrade, and emergency-policy state;
- `Q_h` is the validator/sequencer schedule authorized by the finalized parent, including
  active and delayed-next authority sets where the selected consensus needs them;
- `FinalizedHead_h` identifies the accepted ledger prefix and its declared availability
  status; and
- `NodeLocalView_i` contains facts that may legitimately differ between honest nodes and is
  never covered by `execution_state_root`.

Candidate branches, mempool contents, current-round votes, timeouts, peer state, retrieval
progress, caches, camera state, and rendering state are not deterministic results of user
transactions. They MUST NOT be placed inside `ExecutionState_h`. Finality evidence is
detached evidence over a proposed block identifier; actual byte availability is a network
property evaluated under the selected rule, not a Boolean manufactured by `Execute`.

The current executable has `B_h`, local camera/presentation state, ordered edit-attempt
history, aggregate tick outcomes, and local content tags. Its supplied interaction tapes are
replay inputs rather than proof that every live input was recorded. It does not implement
the proposed authorization, nonce, governance, validator, consensus, or availability
protocols.

Protocol genesis MUST make every initial execution component and authority root explicit.
To avoid a self-referential identifier:

```text
genesis_payload = Canonical(
    chain_id, world_id, B_0, M_0, X_0, A_0, N_0, P_0, G_0, Q_0,
    cryptographic_suites, bootstrap_and_availability_policy
)
genesis_id      = H(genesis_domain || genesis_payload)
genesis_package = (genesis_payload, genesis_id)
```

`genesis_payload` excludes `genesis_id`. For the first proposed network profile, one chain
contains exactly one `world_id`; a future multi-world version would replace the single world
with a committed `worlds_root`. Initially `B_0 = {o}`, `M_0` is empty, and every allocation,
authority, parameter, validator, and governance record is explicit. The package MUST be
immutable and reproducible. Origin bedrock is an application invariant, not an initial coin
allocation, observer property, or validator trust anchor.

The authoritative execution transition is a pure function:

```text
ExecutionState_(h+1) = Execute(
    ExecutionState_h,
    BlockContext_h,
    OrderedTransactions_h
)
```

`BlockContext_h` contains only committed inputs required by the selected version, such as
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
event_position = H(Canonical(
    event_position_domain,
    chain_id,
    genesis_id,
    world_id,
    parent_block_id,
    transactions_root,
    transaction_index,
    action_index
))

mint_id = H(
    Canonical(mint_domain, event_position, coordinate)
)
```

This candidate is known before post-state and `block_id` computation, so it creates no hash
cycle. Re-inclusion under another parent creates another event position. A transaction-stable
alternative would deliberately preserve identity across such re-inclusion and must define
the collision and duplicate-inclusion rules. The final choice MUST include an unambiguous
event identity or equivalent canonical nonce; a coordinate alone is never a token ID.
Finality remains a separate status. The historical mint and its consuming burn remain in
the finalized log.

Cardinality alone is too weak: it could pass while tokens are attached to the wrong
coordinates. Let `Episode[id]` be canonical episode state. The core correspondence is:

```text
dom(M_h) = { c in C : c != o and B_h(c) = 1 }
M_h is injective

for every c in dom(M_h):
    Episode[M_h(c)].coordinate = c
    Episode[M_h(c)].status = live

no burned mint_id occurs in range(M_h)
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
not silently rewrite the coordinate episode. A burn is an irreversible state transition in
the finalized history. Governance MAY authorize a later, explicit compensating mint or
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
    genesis_id,
    initial_authorization_descriptor
))

A_h[actor_id] = current versioned authorization descriptor
```

The descriptor can name one key, a threshold set, delegated capabilities, and an explicit
rotation/recovery policy. A finalized authorization transition changes the descriptor while
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
 || world_id
 || actor_id
 || authorizing_key_id
 || actor_nonce
 || valid_after
 || valid_until
 || optional_anchor_block_id
 || optional_expected_state_root
 || fee_and_resource_offer
 || ordered_operations

signing_digest    = H(action_signature_domain || Canonical(TransactionSignDoc))
signature         = Sign(actor_secret_key, signing_digest)
SignedTransaction = Canonical(
    TransactionSignDoc,
    signature_suite_id,
    signature
)

tx_id = H(transaction_id_domain || SignedTransaction)
```

The signature algorithm, key encoding, and hash suite are **Open** decisions. They MUST be
versioned and pinned before a network protocol is declared stable. The signed domain MUST
include the chain and genesis identifiers so an action cannot be replayed on another world.
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
semantic operations, chain/genesis/world, fee cap, validity window, and authorization being
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
3. chain, genesis, world, and any selected anchor/state precondition;
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
| `BlockInvalid` | A proposed block that includes a transaction forbidden by block validation, has a wrong root, exceeds limits, or cannot execute deterministically is rejected wholesale. No candidate post-root becomes canonical. |
| `IncludedSuccess` | Ordered operations commit atomically with the declared nonce and fee disposition. |
| `IncludedNoOp` | An allowed idempotent `on -> on` or `off -> off` records an explicit success-no-op and applies the versioned nonce/fee rule, with no occupancy or token change. |
| `IncludedFailure` | A transaction admitted under the selected policy can record failure. It performs no partial occupancy, token, or ownership writes; only the explicitly selected nonce/fee effects may commit atomically. |

The proposed initial transaction model is transaction-local atomicity: all ordered
application operations in one transaction succeed, no-op as specified, or leave the
application domains unchanged. Blocks execute transactions sequentially, so each receipt's
actual pre-state is the previous transaction's post-state. Candidate execution and durable
publication are a separate atomicity boundary: a node publishes either the fully validated
candidate body and roots or no candidate root, never a partial batch.

Every included transaction MUST have exactly one canonical receipt. Every submitted item
MUST receive a local disposition, but the protocol need not make rejected network spam part
of permanent consensus history. The submission, admission, execution, rejection, and
finalization lanes must reconcile without silently losing an included action.

### 4.2 Ordering and conflicts

Two observers may target the same coordinate in one logical tick. Arrival order on one
machine is not a safe distributed rule. A finalized batch MUST carry one canonical order,
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
submitted -> admitted -> executed -> finalized
                    \-> rejected / expired
executed  -> orphaned  (only before finality)
```

Mempool contents are node-local and MUST NOT be spendable or treated as ownership proof. A
transaction that references a stale anchor, expired validity window, or consumed nonce must
have a deterministic replacement or rejection rule. Exact-parent binding and windowed
anchor/precondition binding are different designs: a version MUST select one. If exact-parent
binding is selected, every new parent requires explicit re-signing or replacement; if a
windowed anchor is selected, the admissible ancestor window and expected-state failures are
consensus rules.

An unfinalized mint is provisional. Wallets and clients MUST distinguish candidate,
executed, and finalized receipts. If a pre-finality fork removes a transaction, its mint
or burn is not part of the canonical supply; re-inclusion requires a new, valid transaction
under the surviving parent and nonce rules.

### 4.4 Proposed block, identifier, finality evidence, and receipts

The signed object must not contain its own signature or certificate. A consensus-neutral
envelope separates the ordered body, deterministic execution commitments, proposal
signature, and detached finality evidence:

```text
Body = Canonical(
    ordered_signed_transactions,
    deterministic_auxiliary_inputs
)

transactions_root = CommitOrdered(ordered_signed_transactions)
receipts_root     = CommitOrdered(receipts_by_transaction_index)

post_history_root = H(Canonical(
    history_domain,
    parent_history_root,
    height,
    transactions_root,
    receipts_root,
    post_execution_state_root
))

ProposalHeader = Canonical(
    protocol_version,
    chain_id,
    genesis_id,
    world_id,
    height,
    epoch,
    round_or_view,
    parent_block_id,
    parent_history_root,
    pre_execution_state_root,
    transactions_root,
    receipts_root,
    post_execution_state_root,
    post_history_root,
    parameter_root,
    governance_root,
    active_validator_root,
    optional_next_validator_root,
    consensus_policy_id,
    data_availability_commitment,
    proposer_identity
)

block_id           = H(block_header_domain || ProposalHeader)
proposer_signature = Sign(proposer_key, H(proposal_signature_domain || block_id))
Proposal           = Canonical(ProposalHeader, Body, receipts, proposer_signature)
FinalityEvidence   = consensus-specific evidence over block_id
FinalizedBlock     = Canonical(Proposal, FinalityEvidence)
```

`FinalityEvidence` may be a quorum certificate, a declared work/ancestry proof, or a trusted
sequencer signature according to the selected consensus profile. It is never inside the
`block_id` it authenticates. Consensus-specific header fields must replace, not informally
reinterpret, generic fields such as `round_or_view`.

A valid proposal requires all of the following to recompute exactly: parent/height/epoch;
proposer eligibility and signature; canonical body and transaction root; transaction and
block resource limits; sequential deterministic execution from the pre-state; ordered
receipts and receipt root; post-state and history roots; parameter, governance, and active
validator roots authorized by the finalized parent; and the declared data-availability
commitment. Finalization additionally requires valid consensus-specific evidence. A mismatch
in any required field invalidates the proposal rather than producing a partial successor.

Each included transaction receipt is canonical and position-specific:

```text
Receipt = Canonical(
    tx_id,
    included_height,
    transaction_index,
    result,
    pre_transaction_state_root,
    post_transaction_state_root,
    nonce_disposition,
    fee_disposition,
    ordered_effects_root,
    execution_policy_id
)

Effect = Canonical(
    action_index,
    status,
    coordinate,
    pre_occupancy,
    post_occupancy,
    optional_mint_id,
    optional_consumed_mint_id,
    optional_burn_event_id
)
```

`block_id` is deliberately not inside a receipt whose root helps form that identifier. A
receipt-inclusion proof links the receipt and its transaction index to `receipts_root`; an
authenticated header links that root to `block_id`; detached finality evidence establishes
its consensus status. Execution, inclusion, finality, and availability remain four separate
claims. A receipt hash alone proves only the receipt bytes and schema.

### 4.5 Genesis, versioning, and upgrades

The `genesis_payload` MUST contain, at minimum:

- `chain_id` and the first profile's single `world_id`;
- protocol and state-machine versions;
- the origin coordinate and initial mutable world;
- initial authority/validator configuration;
- resource, fee, and size limits;
- cryptographic suite identifiers;
- upgrade and emergency-policy roots.

`genesis_id` is then derived from that payload as defined in §2. Every block and proof MUST
identify the version that interpreted it. Unknown versions, missing genesis data, and
incompatible parameter roots MUST fail closed.

An upgrade is a deterministic, finalized state transition, not a changed constant in one
binary. Its record MUST bind the old and new protocol versions, activation height, migration
function or code hash, old root, expected new root, parameter deltas, and authorization
proof. Every validator replays the same bounded migration. Bedrock, supply, ownership, and
authorization changes must appear as explicit deltas rather than emerge from serialization
drift. A cryptographic migration must prevent downgrade, define any dual-validation window,
and preserve verification of historical blocks. Emergency and rollback policy cannot
silently rewrite finalized history.

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

A proposed first-profile root hierarchy commits every deterministic execution domain:

```text
application_root = H(Canonical(
    application_state_domain,
    protocol_version,
    chain_id,
    genesis_id,
    world_id,
    world_root,
    active_token_root,
    asset_state_root
))

execution_state_root = H(Canonical(
    execution_state_domain,
    protocol_version,
    chain_id,
    genesis_id,
    world_id,
    application_root,
    authorization_root,
    nonce_root,
    parameter_root,
    governance_root,
    validator_schedule_root
))
```

`asset_state_root` covers all selected ownership, transfer, balance, fee, reward, and stake
state, not only an ownership projection. `authorization_root` covers current key descriptors,
delegations, rotation, revocation, and recovery state. `validator_schedule_root` covers
active and delayed-next schedules but not current-round votes. The `ProposalHeader` in §4.4
then binds the execution root to its parent, transaction/receipt/history commitments,
consensus policy, active validator set, governance/parameter roots, and data-availability
commitment. `block_id` is the header identifier; `FinalityEvidence` is detached.

Application roots, execution-state roots, block identifiers, and finality evidence must not
be interchangeable. A valid world root is not a valid finality certificate, and a valid vote
certificate is not a proof that a coordinate is occupied.

The `world_root` could be an authenticated sparse map, sparse Merkle tree, Verkle-style
map, or another structure that handles mostly-absent coordinates efficiently. The choice
is **Open**. It must define canonical child ordering, empty-subtree values, coordinate
encoding, and proofs for both “occupied” and “absent”.

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
- **finality** — consensus-specific evidence covers one `block_id` under the authority set
  selected by its finalized parent;
- **availability** — the bytes needed to replay a finalized block can be retrieved under
  the stated guarantee.

The verifier MUST reject a proof presented under the wrong root kind, schema, chain, genesis,
height, or protocol version. A compact digest without the corresponding witness is only an
identity, not a membership or availability proof.

## 6. Replication, consensus, and finality

Local parent-linked history explains how one process derived a state. Consensus answers a
different question: which history independent processes accept. A versioned claim MUST
separate at least these properties:

- **execution validity** — every accepted successor is the deterministic result of the
  declared parent, context, body, and version;
- **prefix safety** — any two finalized histories accepted under the claim are
  prefix-comparable, not merely non-conflicting at one height;
- **finality** — the exact rule after which a block is irreversible under the stated fault
  assumptions, or the confirmation/confidence rule if finality is probabilistic;
- **availability** — consensus-required bodies and witnesses are retrievable under the
  declared retention, coding, sampling, and network assumptions;
- **chain liveness** — new valid blocks continue to finalize after the stated synchrony
  condition while faults remain within the bound;
- **transaction inclusion/fairness** — the narrower conditions, if any, under which an
  eligible transaction is included;
- **accountability** — conflicting signed evidence can be attributed where the selected
  construction supports it; and
- **cross-implementation agreement** — conforming executors obtain the same receipts and
  roots for the same finalized inputs.

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
power from the active set authorized by the finalized parent, and use exact checked threshold
arithmetic. Aggregation must retain signer attribution and apply the selected scheme's
public-key validation and proof-of-possession defenses. A candidate cannot choose the set
that certifies itself. A block may commit an `optional_next_validator_root`, but delayed
activation and a specified cross-epoch handoff authorize it only from a prior finalized
state.

These bindings are mandatory for a quorum-certificate design even though the consensus
choice remains open. A probabilistic work-based design instead needs a precise work/fork and
confirmation rule and must not label confidence evidence a certificate. A node MUST NOT
treat a state root as final merely because it is newest or locally observed unless that is
the declared consensus rule.

A validator MUST NOT vote for execution/finality until it has obtained all
consensus-required block bytes, validated resource limits, and recomputed the transition.
If the selected protocol substitutes an explicit data-availability certificate or sampling
construction, it must state the sampling threshold, erasure-code and honest-share assumptions,
recovery threshold, withholding horizon, and exact weaker guarantee. A public claim must say
whether its finality entails availability or requires separate evidence.

Before finality, a branch can be replaced. The protocol MUST specify orphan handling,
transaction re-inclusion, nonce behavior, receipt status, and whether provisional mint/burn
records are visible to clients. After finality, a conflicting root is a safety violation,
not an ordinary reorganization. Validator-set changes need an activation rule, overlap or
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
| Replay | Per-identity nonces, chain/genesis/world binding, selected anchor rules, and bounded retention |
| Double spend / duplicate occupancy | Single canonical state transition and live-token checks |
| Same-coordinate race | Consensus-defined ordering and deterministic conflict results |
| Validator equivocation | Signed votes, conflicting-certificate detection, and penalty/expulsion policy if applicable |
| Fork or rollback | Declared finality evidence, checkpoint policy, and recovery rules |
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
- **light clients** — verify headers, finality evidence, and state/receipt witnesses without
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
consistency rule so an API cannot mix records from different finalized roots without saying
so.

Bootstrap MUST name its trust anchor: a pinned genesis package, a verified checkpoint, or a
declared weak-subjectivity source. A snapshot is admissible only after its state root,
validator-set root, finality proof, and parameter/version root are checked. A new node must
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
         -> publish candidate -> obtain finality -> publish finalized index
```

The exact order is an **Open** implementation choice, but a crash between any two steps
must leave a recoverable prefix rather than a root that claims bytes the node cannot
reconstruct. Power-loss, torn-write, corruption, restore, pruning, and recovery tests are
required before a custody or durability claim.

Validator recovery has an additional monotonicity requirement: restoring a backup or moving
to a failover signer MUST NOT regress vote, nonce, slashing-protection, finalized-head, or
upgrade state and thereby authorize a double-sign or stale history. Recovery tooling must
verify exact roots and authority epochs before proposal/vote service resumes.

### 8.3 Operational security, monitoring, and safe halt

Production operation is part of the security envelope. Nodes SHOULD separate consensus,
execution, networking, RPC/indexing, administration, and signing privileges; authenticate
admin/RPC surfaces; pin genesis and configuration; keep secrets out of logs; and verify
signed, reproducible updates with a downgrade-safe rollback procedure.

Operators must monitor finalized-head continuity, conflicting finality evidence, validator
participation and double-sign warnings, active/next validator roots, availability retrieval,
peer diversity/eclipsing, execution-root agreement, storage and recovery health, supply
correspondence, resource saturation, and pending upgrades. Alerts must carry the exact
height, roots, version, and signed evidence needed for independent diagnosis. Metrics are
diagnostic projections, never consensus authority.

A node MUST enter a declared safe-halt mode—stop proposing, voting, or publishing new
finalized indexes—when it observes conflicting valid finality evidence, an unknown active
version, unavailable consensus-required bytes beyond the permitted horizon, an irrecoverable
state-root mismatch, or regressed anti-double-sign state. Safe halt does not automatically
roll back or rewrite finalized history. The incident procedure must define evidence capture,
key isolation/revocation, peer quarantine, resynchronization, coordinated upgrade handling,
restart criteria, and a public post-incident account for any widened trust assumption.

## 9. Determinism and formal refinement

The single most important inherited property is replayability across every activated
protocol version. The replay input is the complete genesis package, finalized headers and
bodies, and an implementation/specification for every version referenced by that history:

```text
(genesis_payload,
 finalized ProposalHeader + Body sequence,
 implementation/specification for every referenced protocol_version)
    -> exactly one execution-state-root and receipt-root sequence
```

Version activation, validator scheduling, committed randomness, rewards, governance, and
migrations come from finalized header/state context. Ordered user actions alone are not a
complete replay input once any of those mechanisms exists.

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

The current Lean capsules prove only their declared abstract or finite models: bedrock and
edit/replay properties plus per-edit finite-trace supply increments, decrements, and no-op
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
at a time.

### Checkpoint A — local occupancy ledger

Promote and extend the existing feature-gated, in-memory coordinate-episode research path
into an optional authoritative local ledger wrapper. This is a change of status and
durability, not a claim that no prototype exists.

Required properties:

- one live mint record per mutable occupied coordinate;
- one consuming burn record per deletion;
- no token for bedrock;
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

Each gate answers one question and must not be promoted into a broader claim:

| Gate | Question | Passing evidence |
| --- | --- | --- |
| G0 | Does the local world obey its invariants? | Rust tests plus bounded Lean model |
| G1 | Does the optional ledger conserve active occupancy? | Independent vectors and mint/burn replay |
| G2 | Do multiple identities produce one deterministic result? | Two-or-more executor agreement per state root |
| G3 | Are actions authenticated and non-replayable? | Signature, nonce, parent-root, and mutation controls |
| G4 | Can an independent node verify membership and non-membership? | Authenticated state proofs and negative tests |
| G5 | Does the replicated protocol preserve safety? | Partition/equivocation/fork adversarial harness |
| G6 | Does it make progress under the stated assumptions? | Bounded liveness measurements with fault injection |
| G7 | Is history and data recoverable? | Fresh-node bootstrap, pruning, crash, and restore tests |
| G8 | Does the implementation refine the specification? | Independent executor plus formal or differential evidence |
| G9 | Are the cryptographic and operational assumptions reviewed? | External audit and published threat-model response |
| G10 | Does finality survive validator changes and partitions? | Certificate/quorum proofs, fork tests, and recovery vectors |
| G11 | Can a node sync without trusting one peer? | Multi-peer body retrieval, snapshot witnesses, and eclipse controls |
| G12 | Are provisional and finalized assets separated? | Reorg/receipt/nonce tests and wallet admission rules |
| G13 | Is execution resource-bounded over time? | State-growth, storage-rent, pruning, and fee-budget measurements |
| G14 | Does the shipped binary match the reviewed source? | Reproducible independent builds and signed release metadata |
| G15 | Are future programs/randomness separately secured? | Versioned VM/gas tests and committed-randomness adversarial tests |

A green G0 or G1 does not imply G5. A matching hash does not imply G6 or G7. A passing
network harness does not prove physical display output or economic value.

## 13. Open decisions

These are deliberately not silently decided by this paper:

1. Is a block an occupancy token, an edit right, a fungible resource, or a dual asset?
2. Does ownership attach to coordinates, token episodes, or permissions?
3. Which signature and hash suites are accepted for the first network version?
4. Which authenticated map supports sparse coordinates and non-membership proofs?
5. Which consensus model matches the desired trust and cost assumptions?
6. Is the coordinate domain permanently signed-64-bit or versioned for expansion?
7. Are observer reach/collision rules consensus state or local interaction rules?
8. What limits or fees prevent place/delete history spam?
9. What data-availability and archival guarantee is required?
10. What upgrade, key-recovery, emergency-stop, and governance rules are acceptable?
11. Is privacy required, and if so, which state is public versus concealed?
12. What exact claim will “Layer 1 security” be allowed to mean for Bitblock?
13. Is `parent_state_root` required on every transaction, and how are stale pending actions replaced?
14. Which transaction lifecycle and reorganization receipts are canonical?
15. What network synchrony, validator fault threshold, and quorum-intersection assumptions apply?
16. Which validator-set transition, checkpoint, and long-range-attack rules are required?
17. Which node types, archival window, snapshot trust anchor, and data-availability guarantee are required?
18. Which state-commitment structure supplies efficient sparse-coordinate membership and non-membership proofs?
19. What exact fee, storage-rent, state-growth, and failed-action charging rules prevent denial of service?
20. Will a future visual program be consensus code, user data, or a separate non-authoritative projection?
21. What randomness source, if any, is admissible to a deterministic future program?
22. What reproducible-build, dependency, key-ceremony, audit, and incident-response standard is required?
23. Which external bridges or oracles, if any, are allowed to make claims outside the coordinate world?
24. Does the network retain `toggle`, require an expected-state precondition, or lower it to `on`/`off`?
25. Which actions are actor-authorized, which are validator-authorized, and how are those namespaces enforced?

Each decision should be recorded with its assumptions, rejected alternatives, falsifier, and
the smallest test that could disprove the chosen design. Until then, the current system
should retain its narrower and truthful description.

## 14. Audit result: obligations still open in the repository

This revision was checked against the current Rust modules, the Lean/LadderLang capsules,
`docs/SEMANTICS.md`, `docs/HANDOFF.md`, and the open-obligation section of
`docs/PROTOCOL.md`. The following gaps remain intentionally open. They are not covered by
the existence of this paper:

| Repository boundary | What is still missing before a Layer 1 claim |
| --- | --- |
| Rust world versus Lean world | A refinement proof or independently checked correspondence for the chunked implementation |
| Canonical byte domains | Independent corpus parity for every authority-relevant schema and key/signature codec |
| Local journal/storage | Durable journal receipts, process-death/power-loss evidence, external custody, and rollback detection |
| History branches | Independently retained heads and a canonical fork/latest-history authority |
| Tick sidecar | Independent replay/refinement and shared durable storage/pruning for finalized outcomes |
| Coordinate episodes | Durable, independently verifiable asset evidence rather than an in-memory local sidecar |
| Sequential/elapsed-time evidence | No VDF/sequentiality analysis or external anchor; serial SHA-256 history is not Proof of History |
| LadderLang bridge | The current bridge is representation-focused and does not prove live edit execution or network authority |
| Network | Peer protocol, sync, validator messages, finality certificates, and adversarial multi-node evidence |
| Cryptography | Pinned signature/hash suite, key lifecycle, proof witnesses, and migration controls |
| Economics | A chosen asset/account model, fees/rent, validator incentives, and supply policy |
| Ternary hashing | The user's implementation, exact contract, independent vectors, and security analysis |
| Programmability | A versioned deterministic VM and refinement/resource proof if blocks become executable |
| Presentation | Physical display evidence remains outside the Layer 1 state-machine claim |

The current branch therefore has a complete **design coverage map**, not complete security
coverage. A future implementation checkpoint should remove one row only when its evidence
is independently reproducible and its assumptions are written into the public claim.

## Conclusion

Bitblock already has the right seed for a verifiable system: a small state space semantics,
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
