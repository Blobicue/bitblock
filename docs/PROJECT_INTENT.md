# Bitblock project intent

This document owns the **why** of Bitblock and the long-range direction currently known from the
design conversation. It does not override the Rust implementation. Current behavior is defined by
the source and [`SEMANTICS.md`](SEMANTICS.md); dependency order and completion gates live in
[`CAPABILITY_TREE.md`](CAPABILITY_TREE.md); the security proposal lives in
[`LAYER1_DESIGN_PAPER.md`](LAYER1_DESIGN_PAPER.md).

This is a versioned record of the motives named so far, not a claim that the future has been fully
discovered. New ideas should be added without rewriting an earlier proposal into an implemented
fact.

## North star

Bitblock asks how much structure can emerge from an unusually small substrate:

```text
coordinate occupied = 1
coordinate absent   = 0
tick                 = the only simulation commit boundary
```

The immediate object is not a Minecraft clone. The cube is a readable spatial representation for
one bit. The experiment is whether direct observation, deterministic edits, spatial arrangement,
and eventually bounded transformation rules can produce symbols, logic, programs, economies, or
other higher-order behavior without hiding a more complicated primitive underneath them.

Emergence is the question, not an assumed result. Until autonomous rules or an interpreter exist,
the current world demonstrates user-authored structure and deterministic replay—not emergent
computation.

## Motives carried forward

### 1. Make state physically legible

One address has one binary meaning. A block is not decoration placed over a separate database row:
its presence is the on state, and its absence is the off state. The permanent origin block makes
the otherwise sparse space orientable.

The observer begins as a camera, not as an entity. Movement, collision, the crosshair, zoom, and
the metrics sidebar exist so a person can inspect and edit the substrate directly. There is no
need for gravity, inventory, health, terrain, mobs, or a general entity system at this stage.

### 2. Make scale perceptible

The same existence should remain observable from a single block filling the viewport down to the
whole occupied world saturating one pixel. Scroll changes observation depth while camera movement
continues to work. Sparse allocation avoids preallocating the coordinate domain.

This is an experiential goal, not a promise of infinite resources. The signed-64-bit coordinate
domain is enormous but finite; memory, traversal, rasterization, proofs, storage, and bandwidth
all require measured bounds.

### 3. Make time one deterministic fold

The tick is the semantic clock. Accepted actions are ordered inside a tick; the reducer commits the
next simulation state; rendering is derived from that state. Wall-clock scheduling decides when
folds are attempted, not what a fold means. Presentation remains a separate observation boundary.

This gives human interaction and agent-authored tapes a common language. A person can act through
the window and an agent can supply a canonical headless sequence, while both target the same
reducer semantics. Complete live-input recording is still an open prerequisite for stronger replay
claims.

### 4. Keep the substrate cheap and inspectable

Rust, sparse chunks, bounded codecs, feature-gated research surfaces, and the absence of unused
game systems all serve the same motive: spend resources on occupied state and explicit evidence.
Optimization must preserve semantic parity and be measured; “sparse” and “near-infinite” are not
synonyms.

### 5. Let patterns become language only through an explicit rule

A spatial arrangement can already be interpreted by a person as a symbol or diagram. A future
visual programming language would make that interpretation canonical:

```text
bits -> pattern -> parsed symbol -> deterministic instruction -> next tick state
```

That future requires a grammar, orientation and namespace rules, a deterministic interpreter,
bounded work, failure semantics, and an independent executor or refinement argument. Rendering
cannot decide program meaning, and a visually similar pixel pattern cannot substitute for exact
coordinate data.

### 6. Make shared state accountable before calling it multiplayer authority

Multiple observers can submit actions without becoming world entities. Multiplayer is valuable
because it introduces distinct actors, conflicting edits, and a need for one shared order. It does
not automatically create identity or ownership. Those require canonical actor identifiers,
signatures, authorization, replay protection, and recovery rules.

A centralized signed sequencer can be useful before consensus: it can produce an accountable log
that every client replays. It remains a trust and availability boundary until independent nodes
agree under a declared consensus profile.

### 7. Explore occupancy episodes as assets without assuming value

The proposed economic reading is:

```text
off -> on  at mutable coordinate c = mint a fresh occupancy episode
on  -> off at mutable coordinate c = burn that live episode
```

Re-placing at the same coordinate creates a new episode identity. The current first-profile
proposal treats bedrock as world structure and gives it no post-genesis occupancy episode; the
asset-design checkpoint must still decide whether origin has no economic representation or a
separate permanently locked genesis allocation. The number of live mutable occupancy episodes can
equal mutable occupied coordinates when a bijection is enforced.

That invariant can be established locally and early. It is not yet a cryptocurrency. A currency
also needs, at minimum, ownership or balance semantics, transfer and authorization rules, shared
ordering, authenticated state, data availability, finality or confirmation, supply and fee policy,
key custody, and adversarial operation. Market value and human uniqueness are outside what a hash
or multiplayer socket can prove.

The project must also decide whether an occupancy episode is a collectible asset, an edit right,
a fungible unit, or something else. Those meanings are not interchangeable. A block token must not
silently become validator stake, a fee balance, and a user-owned coin at once.

### 8. Make proof claims compositional

Bitblock's proof motive is not “hash everything and call it proven.” The project separates:

1. **Canonical identity** — a digest names exact bytes under a schema.
2. **Membership or absence** — a witness relates a key/value claim to an authenticated root.
3. **Transition validity** — an executor proves or checks that one state follows another.
4. **Authorization** — a valid actor was allowed to request the transition.
5. **Consensus status** — independent participants accepted an ordered history under a fault model.
6. **Availability and custody** — the required bytes can be obtained and retained.
7. **Observation** — a renderer, OS, or sensor reported a projection of some committed state.

No item inherits the guarantees of the next. The Lean and LadderLang capsules remain bounded to
their declared models. The user's intended balanced-trit hashing algorithm remains unidentified in
this repository; the current SHA-256 content tags remain the implemented byte-identity mechanism.

### 9. Treat Layer 1 as a final scoped claim, not a starting label

The Layer 1 direction is an optional but serious extension of the original experiment: can the
same minimal coordinate state become a replicated base state machine whose history, authority,
availability, and security assumptions are explicit?

The answer cannot be established by adding a token name, a Merkle root, or a second client. It
requires the staged security work in the design paper and capability tree. Production value should
not be introduced before the exact claim, adversary model, operational controls, and independent
evidence are ready.

## Architectural separations that must survive growth

| Surface | Meaning | Must not silently become |
| --- | --- | --- |
| World | Occupied coordinates | Ownership, history, or pixels |
| Observer | Local camera and interaction state | A consensus entity or human identity |
| Tick | Deterministic local commit boundary | Wall-clock truth or network finality |
| Rendered frame | Projection of committed state | State authority or physical-display proof |
| Content tag | Identity of canonical bytes | Truth, validity, custody, or consensus |
| Authenticated root | Commitment to a logical map | Agreement that the root is canonical |
| Episode ledger | Mint/burn provenance | Currency value or transferable ownership |
| Actor identity | Cryptographic authorization descriptor | One unique human |
| Sequencer | Ordered signed actions | Decentralized consensus |
| Consensus evidence | Acceptance under one declared profile | Availability, execution validity, or eternal safety |
| Visual program | Exact coordinate data under a grammar | Whatever a camera happens to display |

## Deliberate present non-goals

These are not rejected forever. They are excluded from the current substrate checkpoint so they
do not create accidental semantics:

- entities, avatars, bodies, gravity, terrain generation, inventory, combat, or crafting;
- autonomous block rules before one finite deterministic rule is specified and replayed;
- player-facing progression or an in-game skill system;
- token sale, exchange listing, monetary promises, or a claim of cryptocurrency status;
- camera, UI, frame, OS timing, or physical display data inside consensus state;
- an “infinite world” or “unlimited rendering” performance claim;
- treating the current edit language as a VM;
- treating a local hash chain as Proof of History, finality, or a Layer 1;
- replacing the current hash suite with an unidentified ternary construction;
- selecting consensus, signatures, ownership, fees, privacy, governance, or authenticated-map
  designs without a written comparison and failure gate.

## Why the capability tree comes before the next feature

The proposed futures share prerequisites but do not have the same meaning. For example:

- visual programs need deterministic action accounting, but do not need currency;
- a local mint/burn ledger needs complete transition accounting, but not multiplayer;
- ownership needs identity and authorization;
- membership proofs need an authenticated root, but not consensus;
- a light client needs both a state proof and evidence that the referenced root has the claimed
  consensus status;
- validator rewards cannot be designed safely before the consensus role and resource model exist.

The dependency graph therefore prevents an attractive later idea from being smuggled into an
earlier primitive. It also gives each checkpoint a visible stopping condition. The tree is a
project capability graph, not an in-game character skill tree; Bitblock still has no player entity
or progression system.

## Checkpoint discipline

Every capability node should carry five fields:

1. **Motive** — why this capability exists.
2. **Prerequisites** — which semantic decisions and artifacts it relies on.
3. **Artifact** — what code, specification, vectors, or evidence the checkpoint produces.
4. **Gate** — the exact observation that would accept or stop it.
5. **Non-claim** — the tempting broader conclusion the checkpoint does not license.

One checkpoint should change one semantic layer at a time. A failed gate remains recorded. An open
choice remains open until its assumptions, rejected alternatives, and smallest falsifier are
written down.

## Durable document ownership

- [`SEMANTICS.md`](SEMANTICS.md) owns current state meaning.
- [`TRANSCRIPT_AUDIT.md`](TRANSCRIPT_AUDIT.md) owns chronology, original intent, and corrections.
- [`CAPABILITY_TREE.md`](CAPABILITY_TREE.md) owns dependency order, status, and next gates.
- [`AUTHENTICATED_STATE_PLAN.md`](AUTHENTICATED_STATE_PLAN.md) owns the MPT/SMT research checkpoint.
- [`FUTURE_PLAN_AUDIT.md`](FUTURE_PLAN_AUDIT.md) owns the finite audit and correction map for the
  capability/authenticated-state planning checkpoint at its pinned input.
- [`LAYER1_DESIGN_PAPER.md`](LAYER1_DESIGN_PAPER.md) owns the proposed replicated protocol.
- [`LAYER1_DESIGN_AUDIT.md`](LAYER1_DESIGN_AUDIT.md) owns the pinned audit of that paper.
- [`PROTOCOL.md`](PROTOCOL.md) owns implemented canonical-byte and research-carrier contracts.
- [`HANDOFF.md`](HANDOFF.md) owns the short operational state for the next agent.
