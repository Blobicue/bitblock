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

“Ready” is not “recommended regardless of context.” Bitblock still works in one visible semantic
checkpoint at a time. Research that does not alter live authority can run beside that checkpoint,
but it must not be wired into the runtime until its own gate and prerequisites pass.

An activated checkpoint inherits the transitive closure of its prerequisites. Optional branches
stay optional until a released profile activates them: a base occupancy network need not implement
visual programs or a market asset, but a profile that executes visual programs must also satisfy
their parser, VM, resource, refinement, upgrade, and security nodes. A node with several phases
must report each phase separately; an early research artifact cannot lend **IMPLEMENTED** status to
its later authority or operations phase.

Nodes that expand with schemas or profiles—especially `COST-1`, `CODEC-1`, `EXECUTOR-1`,
`BOUNDS-1`, `RELEASE-1`, and the crypto/operations nodes—must record status as
`node@profile-or-schema`, with exact artifact and commit identities. Passing a local-v1 corpus does
not give a later network, program, asset, or migration profile the same status.

## The tree at a glance

`A + B -> C` means that both named nodes are direct prerequisites. The detailed node contracts
below are normative when this compact view omits a later profile-specific prerequisite.

```text
FOUNDATION (current)
  WORLD-0  binary sparse world + immutable origin
  OBSERVE-0 camera observer + collision + pixel-to-screen depth
  TICK-0   deterministic fixed-tick reducer
  REPLAY-0 bounded edit/tape/headless replay surfaces
  PROOF-0  bounded Lean/LadderLang and canonical-tag evidence

LOCAL AUTHORITY TRUNK
  TICK-0 + REPLAY-0 -> ACCOUNT-1 -> STORAGE-1 -> LEDGER-1
  WORLD-0 + PROOF-0 -> COMMIT-R1
  LEDGER-1 + COMMIT-R1 -> COMMIT-1

IDENTITY AND SHARED ORDER
  PROOF-0 -> CODEC-1
  FOUNDATION -> COST-1 -> THREAT-1
  CODEC-1 + THREAT-1 -> CRYPTO-D1
  ACCOUNT-1 + CODEC-1 + THREAT-1 + CRYPTO-D1 -> IDENTITY-D1
  COMMIT-1 + IDENTITY-D1 + CODEC-1 -> PROTOCOL-ID-1
  CRYPTO-D1 + IDENTITY-D1 + PROTOCOL-ID-1 -> CRYPTO-1
  IDENTITY-D1 + PROTOCOL-ID-1 + CRYPTO-1 -> IDENTITY-1
  IDENTITY-1 + COMMIT-1 -> AUTH-COMMIT-1 -> MULTI-1
  MULTI-1 + PROTOCOL-ID-1 + CODEC-1 + BOUNDS-1 + COMMIT-1 + EXECUTOR-1 -> SEQUENCER-1
  SEQUENCER-1 + STORAGE-1 + BOUNDS-1 -> SYNC-1
  SYNC-1 + PROTOCOL-ID-1 + THREAT-1 + CRYPTO-1 + BOUNDS-1 -> CONSENSUS-1
  CONSENSUS-1 + COMMIT-1 + AUTH-COMMIT-1 + PROTOCOL-ID-1 -> CLIENT-1
  CLIENT-1 + PROFILE-1 + cross-cutting release/security nodes -> TESTNET-1
  TESTNET-1 -> L1-CANDIDATE -> L1-PRODUCTION -> optional VALUE-1

ASSET BRANCH
  LEDGER-1 -> ASSET-D1
  ASSET-D1 + IDENTITY-1 + COMMIT-1 -> ASSET-1
  ASSET-D1 + BOUNDS-1 + selected network role model -> RESOURCE-1
  CONSENSUS-1 -> VALIDATOR-ECON-1

VISUAL LANGUAGE AND EMERGENCE BRANCH
  WORLD-0 + PROOF-0 -> LANGUAGE-R1 exact static spatial grammar research
  ACCOUNT-1 -> RULE-1 one finite deterministic block rule -> EMERGENCE-R1
  LANGUAGE-R1 + RULE-1 + ACCOUNT-1 + REFINE-1 + BOUNDS-1 -> VM-1
  VM-1 + COMMIT-1 + SEQUENCER-1 + selected consensus + PROFILE-D1 -> PROGRAM-CONSENSUS

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
  BOUNDS-1 protocol work, allocation, state-growth, and admission limits
  GOVERNANCE-D1 upgrade/emergency authority decision
  PROFILE-D1 frozen candidate activation manifest
  PROFILE-1 admitted activated-feature closure
  UPGRADE-1 deterministic version migration and historical verification
  SIGNER-OPS-1 anti-double-sign key custody and restore
  RELEASE-1 reviewed-source-to-shipped-binary integrity
  PROD-OPS-1 production monitoring, backup, incident, and safe-halt exercises
```

The view shows dependency, not elapsed time. `COMMIT-R1`, `REFINE-1`, `LANGUAGE-R1`, `SCALE-1`,
`COST-1`, and `HASH-SOURCE-R1` are research lanes that can produce
evidence without changing live authority. `PROGRAM-CONSENSUS` does not require the asset branch,
and the asset branch does not require visual programming.

## Foundation nodes

| Node | Status | Prerequisites | Motive | Current artifact | Gate already met | Non-claim |
| --- | --- | --- | --- | --- | --- | --- |
| `WORLD-0` | **IMPLEMENTED** | None; substrate root | Make one coordinate carry one binary fact. | Sparse 16³ chunks over signed `i64³`; absent is off; bedrock is permanent. | World invariant tests and current source semantics. | Not mathematically infinite or unbounded in cost. |
| `OBSERVE-0` | **IMPLEMENTED, bounded** | `WORLD-0` | Let a person inspect and edit the substrate without an entity system. | Free camera, swept-point collision, crosshair, depth controls, fit, sidebar. | Current renderer/camera tests and bounded display carriers. | Not a body, user identity, physical-display proof, or scalable far-coordinate solution. |
| `TICK-0` | **IMPLEMENTED** | `WORLD-0`, `OBSERVE-0` | Give every state transition one deterministic clock. | Fixed 60 Hz reducer and ordered tick outcomes; presentation is separate. | Deterministic same-build tape replay for declared inputs. | Not consensus time, wall-clock proof, or a guarantee that every tick rerenders. |
| `REPLAY-0` | **IMPLEMENTED, bounded** | `TICK-0` | Give people and agents a shared action language. | Edit programs, canonical `InteractionTape`, JSON scenario helper, headless reducer. | Parser/codec/limit vectors and local replay paths. | Not a complete recorder of all accepted live OS input or an independently held run receipt. |
| `PROOF-0` | **BOUNDED** | `WORLD-0`; individual capsules declare more | Make semantic claims attackable instead of intuitive. | SHA-256 content tags, Lean world model, finite LadderLang transition capsule, research receipts. | The declared finite/model checks exist. | Not Rust refinement, ternary-hash integration, physical truth, ownership, consensus, or L1 security. |
| `DOC-0` | **IMPLEMENTED by this checkpoint** | Current source and all documents it describes | Preserve intent, dependencies, and claim boundaries for successor agents. | Project intent, capability tree, authenticated-state plan, transcript and handoff links. | Markdown/source consistency checks for the committed checkpoint. | Documentation is not implementation or independent audit evidence. |

## First trunk: complete and authenticate local state

### `ACCOUNT-1` — complete action/result accounting

- **Status:** **READY; preferred next semantic implementation checkpoint**.
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

### `STORAGE-1` — crash-safe local publication substrate

- **Status:** **LOCKED** on `ACCOUNT-1`; bounded research journals exist but are not this authority.
- **Depends on:** complete canonical records and fixed storage/resource bounds.
- **Motive:** make later ledger heads and authenticated nodes recover together instead of treating
  in-memory success as durable state.
- **Artifact:** versioned append/record envelopes, exclusive-writer rule, atomic or recoverably
  ordered head publication, checksummed data, fsync policy, rollback detection, retained failure
  evidence, supplied-carrier verification, backup/restore hooks, and bounded pruning entry points.
- **Gate:** process-death, torn-write, corruption, stale-head, rollback, concurrent-writer, disk-full,
  and restore assays recover one fully verified committed prefix or fail closed without inventing a
  new head.
- **Non-claim:** one-machine durability is not independent custody, network availability,
  Byzantine agreement, or anti-administrator rollback resistance.

### `LEDGER-1` — durable local occupancy-episode ledger

- **Status:** **LOCKED** on `ACCOUNT-1` and `STORAGE-1`.
- **Depends on:** the complete result lane, crash-safe publication substrate, and current
  feature-gated coordinate-episode prototype.
- **Motive:** establish the exact local meaning of placement-as-mint and deletion-as-burn before
  adding participants or monetary language.
- **Artifact:** optional authoritative local ledger with crash-safe canonical records for one live
  episode per mutable occupied coordinate, consuming burns, fresh re-mints, bedrock excluded only
  from the mutable occupancy-episode map, no-op/rejection results, recovery, and bounded
  replay/pruning rules. `ASSET-D1` still decides whether origin has a separate locked genesis
  allocation.
- **Gate:** domain equality and bijection between mutable occupancy and the active episode map;
  injective live identifiers; every consumed episode has one burn provenance; restart and torn-write
  tests restore one committed head or fail closed.
- **Non-claim:** provenance and conservation do not create a transferable coin, market value,
  multiplayer authority, rollback resistance against an administrator, or consensus.

### `COMMIT-R1` — authenticated-map research

- **Status:** **READY as research only**.
- **Depends on:** `WORLD-0`, `PROOF-0` claim discipline.
- **Motive:** compare proof-carrying sparse-state structures before one becomes consensus-critical.
- **Artifact:** the side-by-side fixed-key binary Patricia and 192-level sparse Merkle prototypes,
  vectors, malformed-proof corpus, and measurements specified in
  [`AUTHENTICATED_STATE_PLAN.md`](AUTHENTICATED_STATE_PLAN.md).
- **Gate:** both candidates pass the same logical-map/root/proof/deletion/fork vectors and report
  bounded update, witness, multiproof, storage, and rebuild costs. A winner is selected only by a
  recorded decision with rejected alternatives and migration consequences.
- **Non-claim:** a prototype root does not replace the current `world_tag`, authenticate a block,
  establish finality, make bytes available, or prove the runtime map is correct.

### `COMMIT-1` — authenticated local state

- **Status:** **LOCKED**.
- **Depends on:** `LEDGER-1`, `COMMIT-R1`, `STORAGE-1`, selected coordinate commitment profile,
  and separately frozen key/value profiles for every additional root this node activates.
- **Motive:** support independently verifiable occupancy, absence, active-episode, and provenance
  queries without replaying or hashing the complete live set for every query.
- **Artifact:** context-free separately typed world, active-token, and episode roots; canonical
  membership/non-membership proofs; full-rebuild parity; atomic publication beside the
  corresponding ledger head. `COMMIT-R1` validates only the 192-bit world map: `MintIdV1` value
  framing and the still-unfrozen `EventIdV1`/episode profile must each pass their own domain gates
  before those roots are included. Authorization, nonce, and asset roots are later typed
  instantiations, not prerequisites hidden inside this node.
- **Gate:** incremental root equals a full rebuild after every generated transition; same logical
  map has one root regardless of insertion order; insert/delete restores the old world root when
  logical occupancy is restored; every activated key/value domain passes the same applicable
  canonicalization/proof/parity gates; malformed, cross-domain, stale-version, and over-bound
  witnesses reject.
- **Non-claim:** an authenticated map proves a statement relative to a supplied root, not that the
  root is canonical, finalized, available, honestly executed, or economically meaningful.

### `REFINE-1` — Rust/model correspondence

- **Status:** **READY as a bounded proof project**.
- **Depends on:** `WORLD-0`; expand after `ACCOUNT-1` and `COMMIT-1` for those surfaces.
- **Motive:** connect abstract world theorems to the actual chunked implementation.
- **Artifact:** independently checked refinement or exhaustive/symbolic correspondence for
  coordinate splitting, bit indexing, bedrock, edits, iteration, and eventually root updates.
- **Gate:** named abstraction function plus base/step proof or a precisely bounded exhaustive
  procedure with powered negative controls; no silent appeal from a finite sample to all `i64³`.
- **Non-claim:** a model theorem without the abstraction/refinement link does not certify Rust.

## Identity and multiplayer branch

### `IDENTITY-D1` — actor and authorization schema

- **Status:** **LOCKED**.
- **Depends on:** `ACCOUNT-1`, `CODEC-1`, `THREAT-1`, and primitive/key-encoding choices from
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

- **Status:** **LOCKED**.
- **Depends on:** `COMMIT-1`, `IDENTITY-D1`, `CODEC-1`.
- **Motive:** give signatures, transactions, receipts, roots, and later votes one chain/world
  context without objects hashing themselves or candidates selecting their own authority.
- **Artifact:** immutable genesis-core and derived `genesis_id`; chain/world/version domains;
  pre-genesis authorization descriptor encoding; acyclic transaction, body, context, proposal,
  block, receipt, effect, availability, and consensus-evidence identifier graph; explicit system
  transition subjects.
- **Gate:** dependency-graph cycle check plus cross-domain, self-reference, field-omission,
  duplicate-field, alternate-body, wrong-parent, and wrong-version mutation corpus. Every
  authority-relevant byte reaches exactly the intended identifier before it can be signed.
- **Non-claim:** identifiers and genesis bytes do not establish trusted distribution, valid keys,
  honest execution, consensus, availability, or a fair initial allocation.

### `IDENTITY-1` — actor identity and authorization

- **Status:** **LOCKED**.
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

- **Status:** **LOCKED**.
- **Depends on:** `IDENTITY-1`, implemented `COMMIT-1`; `COMMIT-R1` selection alone is insufficient.
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

### `MULTI-1` — deterministic multi-actor harness

- **Status:** **LOCKED**.
- **Depends on:** `AUTH-COMMIT-1`.
- **Motive:** expose ordering, authorization, and same-coordinate conflicts at the cheapest scale.
- **Artifact:** two or more signed action streams folded by one in-process deterministic executor.
- **Gate:** permutations of races, stale anchors, duplicate nonces, delegated rights, no-ops,
  admitted failures, and re-mints produce one specified result or one specified rejection across
  independent runs.
- **Non-claim:** multiple actors in one process are not networking, decentralization, Sybil
  resistance, availability, or consensus.

### `SEQUENCER-1` — accountable centralized multiplayer

- **Status:** **LOCKED**.
- **Depends on:** `MULTI-1`, `PROTOCOL-ID-1`, `CODEC-1`, `BOUNDS-1`, `EXECUTOR-1`, and durable
  `COMMIT-1` publication.
- **Motive:** let remote participants share one signed, replayable world before consensus is added.
- **Artifact:** bounded wire messages; authenticated admission; canonical batch order; append-only
  bodies/receipts; atomic state-root publication; independently replaying clients.
- **Gate:** malicious-input, restart, duplicate, race, withheld-body, receipt-inclusion, and client
  resynchronization tests; every accepted batch reproduces the published root in an independent
  executor.
- **Non-claim:** the sequencer can still censor, reorder, halt, equivocate, lose data, or present
  different histories. Accountability is not decentralization.

### `SYNC-1` — peer synchronization, availability, and recovery

- **Status:** **LOCKED**.
- **Depends on:** `SEQUENCER-1`, `STORAGE-1`, and the applicable `BOUNDS-1` limits.
- **Motive:** make required bodies, witnesses, and accepted state recoverable from more than one
  process.
- **Artifact:** role-separated full verifier, archive, light client, and non-authority RPC paths;
  peer discovery; bounded parsing/backpressure; block/body/witness retrieval; checkpoint/genesis
  bootstrap; pruning and recovery policy.
- **Gate:** multi-peer eclipse, partial-data, corrupt-snapshot, slow-peer, restart, pruning, and
  fresh-node recovery tests under declared retention assumptions.
- **Non-claim:** data retrieval does not prove correct execution, canonical ordering, or finality.

## Asset and economic branch

### `ASSET-D1` — choose what a block asset means

- **Status:** **OPEN DECISION**, eligible after `LEDGER-1` exposes the real episode model.
- **Depends on:** `LEDGER-1`; threat/resource analysis may force revision.
- **Motive:** prevent one word—“coin”—from hiding incompatible state machines.
- **Artifact:** a decision record choosing and specifying occupancy collectible, coordinate edit
  right, fungible unit, dual-domain design, or no monetary asset; it names rejected alternatives,
  conservation rules, bedrock treatment, transferability, and recovery behavior.
- **Gate:** the chosen meaning maps every mint, burn, re-mint, transfer, failed action, fork, rent,
  and governance event to one atomic outcome without contradictory supply equations.
- **Non-claim:** choosing semantics does not establish demand, price, legal status, fair
  distribution, decentralization, or security.

### `ASSET-1` — ownership and transfer

- **Status:** **LOCKED**.
- **Depends on:** `ASSET-D1`, `IDENTITY-1`, `AUTH-COMMIT-1`, `COMMIT-1`.
- **Motive:** distinguish occupancy, episode identity, and authority to transfer or edit.
- **Artifact:** chosen account/UTXO/object model; ownership/balance/permission roots; atomic
  transfer, mint, burn, recovery, and conflict rules; proof-carrying client receipts.
- **Gate:** conservation, authorization, replay, concurrent transfer, burn-after-transfer,
  reorganization, recovery, and root-correspondence vectors pass independently.
- **Non-claim:** a consistent asset ledger is not yet a cryptocurrency network or safe place for
  production value.

### `RESOURCE-1` — fees, rent, and economic resource policy

- **Status:** **LOCKED**.
- **Depends on:** `BOUNDS-1`, `ASSET-D1`, and the selected network role model.
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

### `VALIDATOR-ECON-1` — consensus incentives

- **Status:** **LOCKED**.
- **Depends on:** `CONSENSUS-1`; only activated profiles that use economic validator incentives
  require this node.
- **Motive:** align validator behavior only after “validator” has an exact protocol meaning.
- **Artifact:** separate stake/reward/slashing domains, issuance schedule, unbonding, evidence,
  and economic attack analysis.
- **Gate:** safety/liveness arguments and adversarial simulations include bribery, censorship,
  equivocation, griefing, concentration, long-range, and recovery assumptions.
- **Non-claim:** occupancy scarcity does not automatically secure consensus, and a block token
  must not silently become validator stake.

## Visual-programming branch

### `RULE-1` — one finite autonomous transformation rule

- **Status:** **LOCKED**; rule choice is **OPEN**.
- **Depends on:** `ACCOUNT-1`.
- **Motive:** test the emergence hypothesis with the smallest executable semantics.
- **Artifact:** one opt-in, finite, tick-scoped rule over exact coordinates with declared read/write
  neighborhood, ordering, boundary, conflict, quiescence, and resource behavior.
- **Gate:** base/step vectors, symmetry cases, powered mutants, headless/live parity, and exact
  `(initial state, accepted actions, rule version)` replay.
- **Non-claim:** one rule does not demonstrate universal computation, useful emergence, or a
  programming language.

### `LANGUAGE-R1` — exact spatial grammar research

- **Status:** **READY as non-authoritative research**; grammar remains **OPEN**.
- **Depends on:** `WORLD-0`, `PROOF-0` claim discipline.
- **Motive:** let arrangements of bits denote symbols and instructions without visual ambiguity.
- **Artifact:** canonical coordinate normalization, origin/orientation, framing, namespaces,
  delimiters, symbol table, parse errors, versioning, and canonical source serialization.
- **Gate:** independent parsers agree over valid/invalid corpora; rotations, translations,
  reflections, overlaps, missing delimiters, and visually similar but coordinate-distinct patterns
  have explicit results.
- **Non-claim:** a pattern recognized by the renderer or a human is not canonical program data.

### `VM-1` — bounded deterministic interpreter

- **Status:** **LOCKED**.
- **Depends on:** `LANGUAGE-R1`, `RULE-1`, `ACCOUNT-1`, `REFINE-1`, and `BOUNDS-1` for the activated
  language profile.
- **Motive:** turn exact spatial programs into reproducible state transitions.
- **Artifact:** versioned instruction set, integer-only authority semantics where possible,
  deterministic scheduler, gas/work bounds, memory limits, traps, rollback/commit rules, and an
  independent reference executor.
- **Gate:** differential and adversarial programs agree per step/root; nontermination and resource
  exhaustion stop deterministically; renderer/camera mutations cannot affect execution.
- **Non-claim:** determinism alone does not make a language safe, useful, decentralized, or
  consensus-authoritative.

### `EMERGENCE-R1` — bounded emergence assays

- **Status:** **LOCKED**.
- **Depends on:** `RULE-1`; richer assays may later depend on `VM-1`.
- **Motive:** turn the founding question—whether simple bit/tick rules produce higher-order
  structure—into observations that can fail rather than examples selected for looking interesting.
- **Artifact:** frozen rule versions, initial states, seeds or committed randomness if used, action
  tapes, explicit metrics for persistence/replication/compression/computation or another named
  behavior, null models, negative controls, and retained runs.
- **Gate:** predeclared metrics distinguish the candidate behavior from its controls over the
  declared finite corpus and reproduce under an independent executor; qualitative images remain
  labeled observations rather than substituted for the metric.
- **Non-claim:** a complex-looking pattern does not by itself prove emergence, universal
  computation, intelligence, life, a fractal law, or behavior outside the tested rule/corpus.

### `PROGRAM-CONSENSUS` — programs as shared authority

- **Status:** **LOCKED**.
- **Depends on:** `VM-1`, `PROTOCOL-ID-1`, `AUTH-COMMIT-1`, `COMMIT-1`, `BOUNDS-1`, `EXECUTOR-1`,
  `SEQUENCER-1`, `SYNC-1`, `CONSENSUS-1`, `UPGRADE-1`, and a `PROFILE-D1` draft that proposes
  programmability.
- **Motive:** allow spatial programs to affect shared state only when every verifier can reproduce
  and price them.
- **Artifact:** program/code roots, activation/version rules, transaction context, receipts,
  migration policy, deterministic randomness source if any, and refinement evidence.
- **Gate:** independent nodes reproduce program effects and resource charges across upgrades;
  malformed code, nondeterminism, stale versions, and unavailable program bytes fail closed.
- **Non-claim:** visual programmability is not required for the base occupancy ledger or an L1.

## Scale branch

### `SCALE-1` — measure before redesign

- **Status:** **READY as read-only research**.
- **Depends on:** `WORLD-0`, `OBSERVE-0`.
- **Motive:** identify the real limits behind “near-infinite” intent.
- **Artifact:** reproducible workloads separating allocated bits, chunks, visible contributions,
  collision candidates, zoom depth, frame dimensions, proof updates, and memory.
- **Gate:** measurements report distributions and hard failure points under pinned builds; every
  proposed index names which measured cost it reduces and what semantic parity it must preserve.
- **Non-claim:** a benchmark sample does not prove unlimited scale or future hardware behavior.

### `SCALE-2` — chunk-relative queries and hierarchical observation

- **Status:** **LOCKED**.
- **Depends on:** `SCALE-1` and its parity vectors.
- **Motive:** preserve exact interaction across far coordinates and large occupied worlds without
  scanning every on bit for every query.
- **Artifact:** chunk-relative camera/collision coordinates, spatial query index, hierarchical
  occupancy/LOD summaries, exact top-saturation composition, invalidation rules, and bounded caches.
- **Gate:** optimized and reference paths agree for frames, aim, collision, bounds, and edits over
  adversarial coordinates including negative chunk edges and signed extrema; measured budgets pass.
- **Non-claim:** rendering LOD must not become the authenticated or consensus state representation.

## Consensus and Layer 1 branch

### `CONSENSUS-1` — select and specify one consensus profile

- **Status:** **LOCKED**; mechanism, fault threshold, synchrony model, and finality type are
  **OPEN**.
- **Depends on:** `SYNC-1`, `PROTOCOL-ID-1`, `THREAT-1`, `CRYPTO-1`, `BOUNDS-1`.
- **Motive:** replace sequencer trust with an explicit replicated ordering and state-root rule.
- **Artifact:** quantitative adversary and network model; proposal/vote/certificate domains;
  fork-choice or deterministic-finality rule; validator-set transition; data-before-vote;
  confirmation/finality status; safe-halt and recovery rules.
- **Gate:** formal/model safety argument appropriate to the profile plus a deterministic reference
  harness over adversarial proposal, vote, partition, missing-data, set-change, and long-range
  traces. Independent multi-process, restart, and operational fault evidence belongs to
  `TESTNET-1`; sampled/model liveness evidence stays bounded to its assumptions.
- **Non-claim:** consensus evidence does not imply correct execution or available bodies unless
  those checks are separately bound into the profile.

### `CLIENT-1` — proof-bound provisional and accepted client state

- **Status:** **LOCKED**.
- **Depends on:** `CONSENSUS-1`, `COMMIT-1`, `AUTH-COMMIT-1`, and receipt-inclusion commitments from
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

- **Status:** **LOCKED**.
- **Depends on:** `CLIENT-1`, `PROFILE-1`, `EXECUTOR-1`, `UPGRADE-1`, `SIGNER-OPS-1`, `RELEASE-1`,
  `CRYPTO-1`, `THREAT-1`, and the activated `BOUNDS-1` profile.
- **Motive:** expose implementation and operating failures across independent Rust processes.
- **Artifact:** version-pinned genesis, validators, full verifiers, archive/recovery nodes, fault
  injector, retained bodies/witnesses/evidence, and public coverage results.
- **Gate:** every design-paper gate G0–G19 relevant to the activated profile has an artifact,
  result, assumption, and stop condition; unresolved authority/root/availability faults halt the
  candidate rather than being documented away.
- **Non-claim:** a testnet, hash agreement, or long fault-free run is not production security.

### `L1-CANDIDATE` — independently reviewable base state machine

- **Status:** **LOCKED**.
- **Depends on:** `TESTNET-1`, `PROFILE-1`, `UPGRADE-1`, `RELEASE-1`, `PROD-OPS-1`, and the complete
  activated-profile coverage matrix.
- **Motive:** make the strongest possible claim reviewable before real value depends on it.
- **Artifact:** frozen schemas, reproducible builds, independent executor, genesis package,
  threat model, consensus proof/evidence, DA and recovery contracts, upgrade rules, operations
  manual, audits, and dispositioned findings.
- **Gate:** every public property maps to a version, assumption, artifact, result, and loss
  condition; independent review finds no unresolved critical issue; migration and incident drills
  preserve stated invariants.
- **Non-claim:** candidate status is not an unqualified assertion of security or fitness for
  production value.

### `L1-PRODUCTION` — scoped production security claim

- **Status:** **LOCKED**.
- **Depends on:** `L1-CANDIDATE`, `PROD-OPS-1`, external review, and matched deployed artifacts.
  Real monetary value is a separate optional activation.
- **Motive:** state exactly what the deployed base layer guarantees and under which assumptions.
- **Artifact:** signed release/genesis/profile identifiers, live validator and availability
  assumptions, incident channels, monitored bounds, archival commitments, and public exclusions.
- **Gate:** the deployed binary and configuration match the reviewed artifacts; ongoing monitoring,
  backup, recovery, key-compromise, and upgrade exercises meet the published contract.
- **Non-claim:** “Layer 1” never means universal safety, guaranteed value, one-human-one-account,
  privacy, fair ordering, physical-display truth, or protection beyond the named model.

### `VALUE-1` — optional production-value activation

- **Status:** **LOCKED** and optional; Bitblock can remain a research/game network indefinitely.
- **Depends on:** `L1-PRODUCTION`, `ASSET-1`, `RESOURCE-1`, `VALIDATOR-ECON-1` if validators receive
  economic rewards, independent economic/security/legal review, and an explicit human decision.
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

- **Status:** **READY as measurement work**; no “cheap” budget is currently frozen.
- **Depends on:** the current foundation; repeats for every activated feature.
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

- **Status:** **READY for current schemas; expands before each authority-relevant schema activates**.
- **Depends on:** `PROOF-0` framing discipline and a frozen schema.
- **Motive:** ensure “the same bytes” does not mean only that one encoder can decode itself.
- **Artifact:** language-neutral positive/negative corpus plus an independent implementation for
  every activated key, value, transaction, block, receipt, proof, vote, and migration codec.
- **Gate:** exact byte/root parity; alternate bytes, unknown/duplicate fields, truncation, overlong
  lengths, overflow, noncanonical order, and cross-domain/version replay reject before allocation or
  mutation.
- **Non-claim:** same-code round trips or matching valid vectors alone are not independent parity.

### `EXECUTOR-1` — independent transition executor

- **Status:** **LOCKED** for the first authority profile.
- **Depends on:** `ACCOUNT-1`, `CODEC-1`, `COMMIT-1`, `PROTOCOL-ID-1`, and the frozen base transition
  specification for that profile.
- **Motive:** detect a shared implementation bug that same-code replay would reproduce perfectly.
- **Artifact:** independently implemented base state-transition executor with no shared
  authority-path code, producing ordered effects, receipts, and roots. Each later upgrade must add
  its own independently implemented migration extension under `UPGRADE-1`.
- **Gate:** differential corpora, generated sequences, failure/no-op paths, fork contexts, and
  powered semantic mutants agree or fail at a named boundary per step for the pinned profile.
- **Non-claim:** two wrappers, binaries, or processes around the same reducer are not independent.

### `THREAT-1` — quantitative adversary model

- **Status:** **READY as design work; required before consensus selection**.
- **Depends on:** the exact claim/profile being considered and measured `COST-1` facts. It defines
  capacity/admission requirements that `BOUNDS-1` must later realize rather than depending on that
  later node.
- **Motive:** make “secure” falsifiable against declared attacker powers and time horizons.
- **Artifact:** static/adaptive corruption, network scheduling/partition/eclipsing, validator weight,
  key compromise, client bootstrap, economic budget, denial-of-service capacity, censorship,
  bridge/oracle, supply-chain, and operational powers mapped to properties and mitigations.
- **Gate:** every public property names assumptions, loss condition, detection/recovery behavior,
  excluded attacks, and the gate/artifact that bears on it.
- **Non-claim:** a list of attack names without thresholds, powers, or failure conditions is not an
  adversary model.

### `CRYPTO-D1` — primitive and key-encoding selection

- **Status:** **OPEN DECISION** for network signatures and production commitment suites.
- **Depends on:** `CODEC-1`, `THREAT-1`.
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

- **Status:** **LOCKED**.
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

- **Status:** **READY as identification work**; no candidate exists in this repository.
- **Depends on:** none beyond access to the exact artifact the user intends.
- **Motive:** distinguish the user's actual balanced-trit construction from similarly named but
  already disqualified Aethor TSH/PDH or IterHash designs.
- **Artifact:** exact path/repository/commit, provenance, author-intended name and use case, build or
  reading entry point, and an immutable source digest.
- **Gate:** the artifact can be pinned and its claimed contract can be quoted from its own source or
  accompanying specification; resemblance is not accepted as identity.
- **Non-claim:** locating source says nothing about correctness or cryptographic security.

### `HASH-R1` — intended ternary-hash evaluation

- **Status:** **LOCKED**.
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

- **Status:** **LOCKED** for the first authority profile; it expands for later profiles.
- **Depends on:** `COST-1`, `ACCOUNT-1`, `CODEC-1`, the common proof shapes frozen by `COMMIT-R1`,
  and the exact accepted-input operations proposed for that profile—not a future sequencer or
  consensus implementation.
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

### `GOVERNANCE-D1` — upgrade and emergency authority

- **Status:** **OPEN DECISION**.
- **Depends on:** `IDENTITY-D1`, `PROTOCOL-ID-1`, `AUTH-COMMIT-1`, `THREAT-1`.
- **Motive:** name who may activate a migration, halt for a proven authority/root fault, or authorize
  recovery without leaving those powers as an operator convention.
- **Artifact:** governance subjects and permissions, proposal/approval threshold, timelock,
  conflict-of-interest and validator-set rules, emergency scope, safe-halt trigger, restart
  authority, compensation versus forbidden history rewrite, appeal/expiry, and public evidence.
- **Gate:** unauthorized, replayed, conflicting, rushed, censored, compromised-key, stale-set,
  rollback, emergency-overreach, and restart vectors have deterministic outcomes under the threat
  model; no emergency action silently violates finalized/confirmed history or supply invariants.
- **Non-claim:** a governance rule does not make decisions wise, decentralized, legally legitimate,
  uncensorable, or immune to capture.

### `PROFILE-D1` — candidate activated-feature manifest

- **Status:** **LOCKED** for the first network-profile draft.
- **Depends on:** `PROTOCOL-ID-1`, `CODEC-1`, `BOUNDS-1`, and the manifest schema; it names rather
  than implements optional features.
- **Motive:** give programs, migrations, economics, and other optional features one immutable target
  without pretending their implementation has already passed.
- **Artifact:** versioned canonical candidate manifest of activated schemas, roles, optional
  capability nodes, parameters, roots, limits, suites, genesis/profile identifiers, and the
  transitive prerequisite set required for each activation.
- **Gate:** independent codec/parser parity, no unknown or duplicate activation, canonical ordering,
  and deterministic closure expansion. Missing implementation evidence is reported but does not
  make drafting circular.
- **Non-claim:** a frozen candidate manifest is not an admitted profile and does not implement or
  approve any feature it names.

### `UPGRADE-1` — deterministic version migration

- **Status:** **LOCKED**.
- **Depends on:** `PROTOCOL-ID-1`, `CODEC-1`, `COMMIT-1`, base `EXECUTOR-1`, `CRYPTO-1`,
  `GOVERNANCE-D1`, and the old/new `PROFILE-D1` candidate manifests.
- **Motive:** prevent a codec, hash, VM, or state-layout upgrade from silently rewriting authority,
  supply, bedrock, ownership, or historical verification.
- **Artifact:** frozen migration specification, activation authority/timelock, deterministic
  bounded migration function, old/new roots over one logical state, dual-profile window if any,
  downgrade rejection, rollback/safe-halt rule, archival historical verifier, and an independent
  migration extension implemented after the migration specification is frozen.
- **Gate:** old/new-root vectors and full bounded migration replay agree independently; invariant,
  interruption, restart, unauthorized activation, downgrade, and historical-proof tests pass.
- **Non-claim:** governance approval or old-root equality alone does not prove migration correctness,
  especially if the old hash or implementation is compromised.

### `PROFILE-1` — admitted activated-feature closure

- **Status:** **LOCKED** for the first testnet profile.
- **Depends on:** `PROFILE-D1` and every exact-version capability node named by its expanded
  prerequisite set, including `UPGRADE-1` for a migration profile and `PROGRAM-CONSENSUS`,
  `ASSET-1`, `RESOURCE-1`, or `VALIDATOR-ECON-1` only when activated.
- **Motive:** prevent a testnet or release from inheriting gates for the base profile while quietly
  activating an optional surface whose evidence is missing.
- **Artifact:** immutable admitted-manifest digest plus a machine-readable mapping from every
  activated node/version to its artifact, commit, result, assumptions, and loss condition.
- **Gate:** the closure walker rejects every missing, wrong-version, non-passing, stale, or mutually
  incompatible prerequisite; `TESTNET-1`, releases, and coverage matrices bind this exact admitted
  digest.
- **Non-claim:** profile admission does not prove properties outside the activated closure or make
  its assumptions universally true.

### `SIGNER-OPS-1` — validator signer safety

- **Status:** **LOCKED**.
- **Depends on:** `IDENTITY-1`, selected `CONSENSUS-1` credentials, and `CRYPTO-1`.
- **Motive:** keep restart, failover, and backup from creating equivocation even when the protocol
  rules are correct.
- **Artifact:** role-separated signer boundary, monotonic anti-double-sign state, key ceremony,
  backup/restore and failover procedures, compromise/revocation path, and auditable signing policy.
- **Gate:** crash, stale backup, concurrent replica, rollback, failover, compromise, and disaster
  recovery drills cannot emit conflicting authorized messages or silently regress signer state.
- **Non-claim:** secure signer operation does not establish validator honesty, quorum safety, or
  network availability.

### `RELEASE-1` — reviewed source to shipped binary

- **Status:** **READY to establish; required before adversarial testnet claims inherit review**.
- **Depends on:** pinned Rust/tool/proof dependencies and activated build profiles.
- **Motive:** stop an unreviewed compiler, dependency, feature set, binary, or genesis configuration
  from inheriting source-level evidence.
- **Artifact:** dependency lock/policy, pinned toolchain, reproducible independent builds, SBOM or
  equivalent manifest, signed release metadata, feature/config/genesis/profile binding, and
  vulnerability response.
- **Gate:** independent builders reproduce the declared artifact or an explicitly explained
  platform-scoped equivalent; mismatched source, dependency, feature, config, or binary rejects the
  release identity.
- **Non-claim:** reproducibility proves artifact correspondence, not absence of source or toolchain
  vulnerabilities.

### `PROD-OPS-1` — production operations and incident safety

- **Status:** **LOCKED**.
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

The current branch has three kinds of eligible next work, but only one should alter semantics in a
single checkpoint:

1. **Semantic trunk:** `ACCOUNT-1` is the first implementation prerequisite shared by the ledger,
   multiplayer, and programming branches.
2. **Research without authority:** `COMMIT-R1`, `REFINE-1`, `LANGUAGE-R1`, `SCALE-1`, `COST-1`,
   `CODEC-1`, and `HASH-SOURCE-R1` can produce comparisons and vectors without
   changing the live state representation.
3. **Visual decision:** the exact one-pixel-startup choice remains independent and can be handled as
   a small observer checkpoint, provided its source, vectors, and docs change together.

`LEDGER-1`, a token/ownership system, networking, and autonomous rules should not be combined in
one change. The strongest next trunk sequence is:

```text
ACCOUNT-1
  -> STORAGE-1
  -> LEDGER-1
  -> select COMMIT-R1 result
  -> COMMIT-1
  -> IDENTITY-D1
  -> PROTOCOL-ID-1
  -> IDENTITY-1
  -> AUTH-COMMIT-1
  -> MULTI-1
  -> SEQUENCER-1
```

`CODEC-1`, `CRYPTO-D1`, `CRYPTO-1`, `COST-1`, and `BOUNDS-1` join the trunk at their named
prerequisites; they are omitted from the linear display only to keep the main semantic sequence
readable.

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

A branch can unlock knowledge without unlocking authority. That distinction is the central rule of
this tree.
