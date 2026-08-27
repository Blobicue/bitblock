# Bitblock: From Binary Occupancy to a Verifiable Layer 1

**Design paper — checkpoint 0.1 — 2026-08-26**

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

A future ledger state can be written as:

```text
S_t = (B_t, M_t, A_t, N_t, P_t, R_t)
```

where:

- `M_t` is the active mint map from mutable occupied coordinates to token identifiers;
- `A_t` is the authorization/identity state;
- `N_t` is per-identity replay state such as nonces;
- `P_t` is the protocol parameter/version state;
- `R_t` is the set of authenticated roots and parent links.

The current executable has only the first component plus local history. The other
components are the proposed extension.

The semantic transition is a pure function:

```text
S_(t+1) = Apply(S_t, OrderedActions_t)
```

It MUST depend only on the canonical parent state, protocol parameters, and ordered
actions. Wall-clock timing MAY determine when a local tick is attempted, but MUST NOT
change the result of a given finalized action sequence.

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
a new token identifier. The historical mint and its consuming burn remain in the log.

The core conservation invariant is:

```text
|M_t| = |{ c in C : c != o and B_t(c) = 1 }|
```

This is **active supply**, not cumulative issuance. A user can produce infinitely many
historical mint events by repeatedly placing and deleting a coordinate unless a separate
issuance, fee, or rate policy is introduced.

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
 || world_id
 || parent_state_root
 || logical_tick
 || actor_public_key
 || actor_nonce
 || operation
 || coordinate
 || optional_token_id
 || optional_fee
 || signature
```

The signature algorithm, key encoding, and hash suite are **Open** decisions. They MUST be
versioned and pinned before a network protocol is declared stable. SHA-256 content tags
are not a substitute for signatures.

The authorization rules MUST state:

- who may mint at a coordinate;
- who may burn or transfer the resulting token;
- whether ownership follows the token or remains with the world;
- whether an owner may delegate edit rights;
- how key rotation and recovery work;
- what happens when a key is lost or compromised.

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

### 4.3 Proposed block and receipt envelope

A future finalized batch can contain:

```text
Block =
    protocol_version
 || chain_id
 || height
 || parent_block_root
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

## 5. Authenticated state commitments

The current world tag is a canonical content identity. A Layer 1 needs a commitment that
also supports independently verifiable membership and non-membership claims.

A proposed state root SHOULD commit to at least:

```text
state_root = H(
    domain
 || protocol_version
 || world_root
 || active_token_root
 || ownership_root
 || nonce_root
 || parameter_root
)
```

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

A consensus protocol MUST specify:

- validator or sequencer membership and admission;
- the fault model and maximum faulty fraction;
- proposal and vote messages;
- quorum and certificate encoding;
- fork choice and equivocation handling;
- finality and recovery after partitions;
- validator key rotation and revocation;
- resource and fee policy for spam resistance;
- how a new node obtains and verifies the canonical history.

Proof of History, Proof of Stake, BFT, Proof of Work, and a trusted central sequencer are
different constructions. The current SHA-256 history chain is none of them. A cheap Rust
implementation can begin with one authority, then a deterministic multi-node testnet;
that is a staging choice, not evidence of decentralized security.

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

## 9. Determinism and formal refinement

The single most important inherited property is replayability:

```text
(genesis_state, canonical_action_log, protocol_version)
    -> exactly one state-root sequence
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

Each decision should be recorded with its assumptions, rejected alternatives, falsifier, and
the smallest test that could disprove the chosen design. Until then, the current system
should retain its narrower and truthful description.

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
