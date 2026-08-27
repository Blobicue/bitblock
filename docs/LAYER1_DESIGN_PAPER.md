# Bitblock: From Binary Occupancy to a Verifiable Layer 1

**Design paper — checkpoint 0.2 — 2026-08-26**

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
coordinate, the bit is `1`; if it does not, the bit is `0`. A fixed tick applies an ordered
set of accepted actions to that sparse world. Rendering is a projection of the committed
state, not a second simulation authority.

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
| Genesis anchor | `(0, 0, 0)` is permanent bedrock and cannot be removed | Implemented |
| Observer | Camera state beside bedrock; no entity, gravity, terrain, inventory, or autonomous rules | Implemented |
| Tick | Fixed 60 Hz reducer consumes accepted actions in exact order | Implemented |
| World/history identity | Canonical SHA-256 content tags and parent-linked local receipts | Implemented |
| Interaction replay | Bounded canonical `InteractionTape` and headless replay reports | Implemented |
| Mint/burn evidence | `CoordinateEpisodeEvidence` names local issuance episodes and consuming burns | Implemented |
| Abstract proof | Lean model covers bedrock, edits, replay, and finite mint/burn accounting | Bounded |
| Ownership | No owners, accounts, signatures, or transfer records | Not claimed |
| Networking | No replicated nodes or wire consensus protocol | Not claimed |
| Persistence | Research storage exists, but the default live history is not durable custody | Not claimed |
| Layer 1 security | No finality, validator set, fork choice, or availability protocol | Not claimed |

The existing SHA-256 tags identify canonical bytes under named schemas. They do not prove
that the bytes are true, current, owned, persisted, or agreed by other machines. The
existing mint/burn surface proves local issuance provenance only; it has no balances,
owners, asset transfers, or monetary value.

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

A future protocol state can be written as:

```text
Application_t = (B_t, M_t, X_t)
Protocol_t    = (A_t, N_t, P_t, H_t, V_t, D_t, G_t)
ConsensusState_t = (Application_t, Protocol_t)
LocalView_t     = UI_t
```

where:

- `B_t` is the occupied-coordinate function;
- `M_t` is the active mint map from mutable occupied coordinates to token identifiers;
- `Application_t` MAY also contain `X_t`, the selected ownership, transfer, balance, fee,
  or stake state. `X_t` is empty until an asset/economic model is chosen, but any later
  asset domain MUST have its own canonical state and root;
- `A_t` is the authorization/identity state;
- `N_t` is per-identity replay state such as nonces;
- `P_t` is the protocol parameter/version state;
- `H_t` is candidate and finalized block/history state;
- `V_t` is validator, quorum, vote, and finality state;
- `D_t` is data-availability and synchronization metadata;
- `G_t` is governance, upgrade, and emergency-policy state;
- `UI_t` is local camera, renderer, and presentation state and is not consensus state.

The current executable has `B_t`, camera/presentation state, accepted-action history, and
local content tags. It does not implement `A_t`, `N_t`, `V_t`, `D_t`, or `G_t` as a network
protocol. The other components are proposed extensions. `LocalView_t` may advance for a
local observer, but it is not part of `ConsensusState_t` unless a future protocol explicitly
promotes a field into authority.

The protocol genesis MUST make the initial tuple explicit:

```text
B_0 = { o }
M_0 = empty
X_0 = empty or explicitly declared initial allocations
H_0 = genesis block/receipt
V_0 = declared initial authority or validator set
P_0 = versioned limits and schemas
```

The genesis package MUST be immutable, reproducible, and identified by a domain-separated
`genesis_id`. The origin bedrock is an application invariant; it is not an initial coin
allocation and does not belong to an observer.

The authoritative semantic transition is a pure function:

```text
ConsensusState_(t+1) = Apply(ConsensusState_t, OrderedActions_t)
```

It MUST depend only on the canonical parent state, protocol parameters, and ordered
actions. Wall-clock timing MAY determine when a local tick is attempted, but MUST NOT
change the result of a given finalized action sequence.

The local 60 Hz simulation tick and a future consensus height are related only by an
explicit protocol rule. They MAY be one-for-one in an early centralized mode, but a
validator MUST NOT infer consensus finality from elapsed wall-clock time or from a window
render. Rendering and camera updates can remain local projections of a committed state.

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
a new token identifier. A proposed derivation is:

```text
mint_id = H(
    token_domain
 || chain_id
 || genesis_id
 || parent_state_root
 || included_height
 || tx_id
 || action_index
)
```

The derivation MUST include a unique event position or an equivalent canonical nonce; a
coordinate alone is not a token identifier. `included_height` identifies the candidate
batch that executed the event; finality is a separate consensus status. The historical mint
and its consuming burn remain in the log.

The core conservation invariant is:

```text
|M_t| = |{ c in C : c != o and B_t(c) = 1 }|
```

This is **active supply**, not cumulative issuance. A user can produce infinitely many
historical mint events by repeatedly placing and deleting a coordinate unless a separate
issuance, fee, or rate policy is introduced.

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
the finalized history; any rollback policy belongs to consensus/finality, not to the
occupancy rule.

## 3. Identity, authorization, and ownership

Multiplayer is useful because it creates competing action sources, but multiplayer alone
does not create secure identity. A username, session ID, or random UUID can be copied. A
future protocol SHOULD use a public-key identity:

```text
observer_id = H(identity_domain || public_key)
```

An action would be bound to that identity by a signature over its canonical bytes:

```text
Action =
    protocol_version
 || chain_id
 || genesis_id
 || world_id
 || parent_state_root
 || logical_tick
 || actor_public_key
 || actor_nonce
 || valid_after
 || valid_until
 || operation
 || coordinate
 || optional_token_id
 || optional_fee
 || signature
```

The signature algorithm, key encoding, and hash suite are **Open** decisions. They MUST be
versioned and pinned before a network protocol is declared stable. The signed domain MUST
include the chain and genesis identifiers so an action cannot be replayed on another world.
The transaction identifier MUST hash the exact canonical signed bytes, and the chosen
signature scheme MUST either be non-malleable or define one canonical signature encoding.
SHA-256 content tags are not a substitute for signatures.

For clarity, a proposed identifier is:

```text
tx_id = H(transaction_domain || canonical_signed_action_bytes)
```

No alternate text spelling, omitted signature, or decoded-and-reencoded variant may create
another identifier for the same submitted action.

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

The current 60 Hz tick is the simulation clock. In a networked protocol it MUST be treated
as a logical height or batch ordinal, not as a promise that every validator executes at
exactly 60 wall-clock ticks per second.

### 4.1 Transaction validity

Before an action can affect state, a validator MUST check at least:

1. canonical decoding and protocol version;
2. signature and identity binding;
3. parent state root and world identifier;
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

Execution of one finalized batch MUST be atomic at the committed-root boundary: validators
may stage intermediate writes privately, but they publish either the declared post-state
root or an explicit invalid/rejected result, never a partially applied batch.

Invalid actions MUST have an explicit result. They MUST NOT mint, burn, mutate state, or
silently disappear from an audit record. Whether invalid transactions are included as
rejections or excluded before finalization is an **Open** wire-protocol choice, but all
validators must obtain the same state result.

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

Mempool contents are not world state and MUST NOT be spendable or treated as ownership
proof. A transaction that references a stale parent, expired validity window, or consumed
nonce must have a deterministic replacement or rejection rule. If `parent_state_root` is
required in every transaction, the protocol must explicitly define how normal pending
transactions survive new blocks; otherwise the parent binding can become an accidental
availability or censorship mechanism.

An unfinalized mint is provisional. Wallets and clients MUST distinguish candidate,
executed, and finalized receipts. If a pre-finality fork removes a transaction, its mint
or burn is not part of the canonical supply; re-inclusion requires a new, valid transaction
under the surviving parent and nonce rules.

### 4.4 Proposed block and receipt envelope

A future finalized batch can contain:

```text
Block =
    protocol_version
 || chain_id
 || genesis_id
 || height
 || parent_block_root
 || transactions_root
 || ordered_transactions
 || proposer_identity
 || consensus_certificate
 || post_state_root
 || post_history_root
 || data_availability_commitment
```

Each transaction receipt SHOULD expose:

```text
Receipt =
    tx_id
 || parent_state_root
 || result
 || pre/post coordinate commitment
 || optional mint_id or consumed_burn_id
 || post_state_root
 || execution_policy_id
```

A receipt hash proves the receipt bytes match the named schema. It does not, by itself,
prove that an authorized validator signed it, that the block is final, or that the data is
available to other nodes. Those properties need separate evidence.

### 4.5 Genesis, versioning, and upgrades

The genesis package MUST contain, at minimum:

- `chain_id` and immutable `genesis_id`;
- a distinct `world_id` or an explicit rule that the chain contains exactly one world;
- protocol and state-machine versions;
- the origin coordinate and initial mutable world;
- initial authority/validator configuration;
- resource, fee, and size limits;
- cryptographic suite identifiers;
- upgrade and emergency-policy roots.

Every block and proof MUST identify the version that interpreted it. Unknown versions,
missing genesis data, and incompatible parameter roots MUST fail closed. An upgrade is a
state transition with an explicit activation height, authority, compatibility rule, and
rollback/emergency policy; changing a constant in a binary is not a protocol upgrade.
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

A proposed state root SHOULD commit to at least:

```text
state_root = H(
    domain
 || protocol_version
 || chain_id
 || genesis_id
 || world_root
 || active_token_root
 || ownership_root
 || nonce_root
 || parameter_root
)
```

The consensus state root SHOULD also bind the validator-set/finality state and the
canonical history head, either directly or through the block header:

```text
block_root = H(
    block_domain
 || chain_id
 || height
 || parent_block_root
 || state_root
 || history_root
 || validator_set_root
 || finality_epoch
)
```

Application roots and consensus roots must not be interchangeable. A valid world root is
not a valid finality certificate, and a valid vote certificate is not a proof that a
coordinate is occupied.

The `world_root` could be an authenticated sparse map, sparse Merkle tree, Verkle-style
map, or another structure that handles mostly-absent coordinates efficiently. The choice
is **Open**. It must define canonical child ordering, empty-subtree values, coordinate
encoding, and proofs for both “occupied” and “absent”.

This is also where the practical-infinity idea meets cryptography. Lazy chunks are good
storage engineering, but a commitment cannot leave “unloaded” ambiguous. Every coordinate
must have a deterministic default value, and every touched region must have a bounded proof
path. The current signed-64-bit domain is large and finite; a later wider or mathematically
unbounded coordinate model would require a versioned commitment format.

The current user-proposed ternary hash is not integrated. Before it can participate in
authority, its exact input alphabet, collision assumptions, output size, domain separation,
canonical encoding, and independent implementation must be identified and compared with
the existing SHA-256 suite. It MAY become a research or secondary commitment; it MUST NOT
become authority merely because it has an attractive name or representation.

Every proof object SHOULD state its type and scope:

- **membership** — a coordinate/token/nonce is included under a named root;
- **non-membership** — an absent coordinate has a verified default path;
- **execution** — a transaction and parent state deterministically yield a receipt;
- **finality** — a validator certificate covers one block root under one validator-set
  version;
- **availability** — the bytes needed to replay a finalized block can be retrieved under
  the stated guarantee.

The verifier MUST reject a proof presented under the wrong root kind, schema, chain, genesis,
height, or protocol version. A compact digest without the corresponding witness is only an
identity, not a membership or availability proof.

## 6. Replication, consensus, and finality

Local parent-linked history explains how one process derived a state. Consensus answers a
different question: which history independent processes accept.

The minimum Layer 1 safety target is:

```text
No two conflicting state roots are both finalized for the same height and chain_id.
```

The liveness target is conditional:

```text
Under the stated network and validator assumptions, a valid action eventually reaches a
finalized batch unless it is invalid, censored by an explicitly bounded fault set, or
rejected by a declared resource policy.
```

Those assumptions must be quantitative enough to test. They include the network model
(for example, authenticated channels and eventual synchrony), the maximum Byzantine or
offline validator fraction, the quorum intersection rule, and the cryptographic hardness
assumptions. “A majority is honest” is not a complete specification until the protocol says
which majority, over which set, and how membership changes.

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

A finality certificate SHOULD bind the block root, chain ID, height, validator-set root,
protocol version, and quorum evidence. A node MUST NOT treat a state root as final merely
because it is the longest, newest, or locally observed branch unless that is the declared
consensus rule.

Before finality, a branch can be replaced. The protocol MUST specify orphan handling,
transaction re-inclusion, nonce behavior, receipt status, and whether provisional mint/burn
records are visible to clients. After finality, a conflicting root is a safety violation,
not an ordinary reorganization. Validator-set changes need an activation rule, overlap or
handoff proof, key revocation behavior, and protection against long-range or stale-checkpoint
attacks.

### 6.2 Ordering fairness and censorship

Consensus ordering is an authority surface. A proposer or sequencer can reorder, delay, or
exclude transactions even when it cannot forge signatures. The protocol SHOULD define
inclusion deadlines, fee/priority rules, duplicate suppression, maximum queue age, and an
evidence path for censorship or equivocation. If no fairness guarantee is intended, the
availability and liveness claim must say so.

Proof of History, Proof of Stake, BFT, Proof of Work, and a trusted central sequencer are
different constructions. The current SHA-256 history chain is none of them. A cheap Rust
implementation can begin with one authority, then a deterministic multi-node testnet;
that is a staging choice, not evidence of decentralized security.

If a future design uses stake, it must define deposits, validator selection, unbonding,
slashing, evidence validity, recovery, and the attack cost. If it uses work, it must define
difficulty, chain-work comparison, issuance, and energy/security assumptions. If it uses
permissioning, the admission authority and compromise response are part of the trust model.
The choice remains open until the desired decentralization, cost, and fault model are
measured rather than assumed.

## 7. Threat model and required defenses

The protocol should name its adversary before selecting a consensus mechanism. The initial
threat inventory is:

| Threat | Required defense or explicit assumption |
| --- | --- |
| Forged action | Signature verification and canonical signed bytes |
| Replay | Per-identity nonces, parent-root binding, and bounded retention rules |
| Double spend / duplicate occupancy | Single canonical state transition and live-token checks |
| Same-coordinate race | Consensus-defined ordering and deterministic conflict results |
| Validator equivocation | Signed votes, conflicting-certificate detection, and penalty/expulsion policy if applicable |
| Fork or rollback | Finality certificate, checkpoint policy, and recovery rules |
| Data withholding | Full transaction availability, archival policy, and retrievability proofs or explicit trust boundary |
| Sybil participation | Admission, stake, proof-of-work, permissioning, or another declared cost |
| Spam and unbounded coordinates | Fees, quotas, work budgets, chunk limits, and allocation-failure handling |
| Malicious implementation | Independent executor, differential replay, fuzzing, and formal refinement where practical |
| Hash or signature failure | Versioned cryptographic suite, conservative assumptions, and migration path |
| Key loss or compromise | Rotation, recovery, revocation, and incident procedures |
| Clock manipulation | Logical height for validity; wall-clock only for scheduling/timeouts |
| Client/render mismatch | Keep pixels non-authoritative; validate submitted semantic actions |
| State corruption | Checksummed storage, parent-root validation, snapshots, and restore tests |
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

No defense is free. Each row needs a named assumption, a test or proof target, and a halt
condition if the assumption is violated.

## 8. Availability, storage, and recovery

An L1 cannot be only a chain of hashes. A new node must be able to recover the bytes needed
to recompute the state. The future protocol SHOULD define:

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

The future network protocol must distinguish at least three roles:

- **full validators** — receive candidate transactions, execute blocks, vote, and retain
  enough data to validate the declared window;
- **archive/recovery nodes** — retain historical blocks, receipts, proofs, and snapshots;
- **light clients** — verify headers, finality certificates, and state witnesses without
  trusting a server's claimed coordinate state.

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
be able to fall back to full replay when the snapshot is unavailable or disputed.

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

## 9. Determinism and formal refinement

The single most important inherited property is replayability:

```text
(genesis_state, canonical_action_log, protocol_version)
    -> exactly one state-root sequence
```

The replay input is the complete genesis package, not only a coordinate bitmap:

```text
(genesis_package, canonical_finalized_block_log, protocol_version)
    -> exactly one consensus-state-root sequence
```

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

The current Lean capsules prove their declared abstract or finite models. They do not yet
prove that the chunked Rust implementation refines the abstract world, that camera floats
are exact over the full coordinate domain, or that a network implementation exists.

## 10. Economic semantics

Cryptographic validity and monetary value are separate. Even a perfectly replicated token
ledger does not create a market.

The protocol must decide:

- whether tokens are transferable;
- whether they are fungible, unique, or both via separate asset classes;
- whether minting is unlimited, capped, scheduled, or resource-priced;
- whether burning is voluntary, required, or reversible under governance;
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
than inferred from occupancy scarcity. State rent, storage deposits, or pruning credits may
be needed to stop an “infinite” coordinate space from becoming an unbounded obligation for
every validator.

## 11. Layered implementation plan

The following checkpoints preserve the current cheap Rust core and add one security surface
at a time.

### Checkpoint A — local occupancy ledger

Add an optional ledger wrapper around the existing world/history path.

Required properties:

- one live mint record per mutable occupied coordinate;
- one consuming burn record per deletion;
- no token for bedrock;
- fresh token identity after re-placement;
- explicit no-op/rejection records;
- active-supply equation and parent-root checks;
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
