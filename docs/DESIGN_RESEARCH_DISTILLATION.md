# Distilled design research notes

**Checkpoint:** source-receipted research distillation, 2026-08-27
**Authority:** notes only; not a protocol specification, capability-tree decision, implementation
claim, cryptographic result, or Layer 1 admission artifact
**Audit:** [`DESIGN_RESEARCH_DISTILLATION_AUDIT.md`](DESIGN_RESEARCH_DISTILLATION_AUDIT.md), frozen
against input commit `694650f4028f5c1fb715384597326e0b0e31f0e1`
**First correction:** `ea9294ba08b5e4bd339f82894ecb761461f5af13`
**Successor audit:**
[`DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP2.md`](DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP2.md), frozen
against that first correction
**Second correction:** `54926f52fa8ba9768e817ecb6139dcca41960e6a`
**Third successor audit:**
[`DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP3.md`](DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP3.md), frozen
against that second correction; its descendant remains a documentation/checker correction

This document replaces repeated conversational explanation with a compact map of the useful ideas
in `C:\Users\Owen\Downloads\design.txt`. The downloaded source remains untouched. It is a mixed
research conversation containing short user questions, generated answers, search-result residue,
analogies, proposed code, and claims of very different quality. It is useful as idea provenance,
not as authority.

## Source receipt and extraction boundary

```text
source_path       C:\Users\Owen\Downloads\design.txt
source_bytes      71583
source_characters 71080
source_words      10673       # whitespace-delimited, not a model-token count
source_lines      125
source_sha256     eeda72adcf153f027c38ac8be6669f668c33e4b11d03b19d34fef2f9f228352f
source_mtime      2026-08-27T02:13:54.4299684-07:00
```

The repository does not archive those raw bytes. This receipt can identify an independently
supplied copy but cannot recover omitted text; continued access to the external file is therefore
an open provenance dependency.

[`design-source-receipt-v1.json`](design-source-receipt-v1.json) records the canonical count
methods, external-source identity, and exact pre-audit distillation input. The planning checker
pins that manifest and requires this Markdown receipt to agree with it.

The concatenated source does not retain a trustworthy machine-readable author boundary. The
question spine below is therefore paraphrased rather than presented as verbatim user quotation.
No claim is retained merely because the source repeated it or attached a search card.

The distillation keeps:

- user motives and questions;
- definitions needed to distinguish objects;
- relationships that can become deterministic contracts;
- falsifiable research candidates and their loss conditions;
- primary-source-backed facts with their scope; and
- explicit open questions.

It removes or quarantines:

- praise, hype, and conclusions such as “holy grail,” “perfect,” or “database disappears”;
- eleven repeated invitations to continue and six `Show all` search-interface fragments;
- repeated wave, graph-paper, airplane-engine, and magical-compression analogies;
- unattributed product adoption, roadmap, performance, replication, and latency numbers;
- generic source cards and vendor comparisons that do not settle a Bitblock decision;
- code-shaped text that does not constrain the claimed computation; and
- restatements that add no new object, invariant, risk, gate, or question.

## Truth labels used here

| Label | Meaning |
| --- | --- |
| **CURRENT** | Verified behavior or boundary already present in the Bitblock repository. |
| **DESIGN RULE** | A relationship the future design must preserve if that lane is activated. |
| **CANDIDATE** | A research direction worth testing, not a selection. |
| **OPEN** | A choice or claim that has no admitted answer. |
| **REJECTED** | A formulation that is false, conflates objects, or exceeds its evidence. |
| **SOURCE NEEDED** | Plausible external claim that must not enter a decision without a pinned primary source and measurement context. |

## Preserved question spine

| Theme | Preserved question | Distilled disposition |
| --- | --- | --- |
| MPT cost | Which costs actually come from the logical trie, and which come from a client database, cache, pruning, or workload? | Measure proof, update, storage, I/O, codec, and ZK costs separately. |
| Layer boundary | Is an authenticated state structure intrinsically Layer 1? | No. It belongs to an execution environment; an L2 can choose a different internal commitment while its L1 verifies a separate rollup contract. |
| Verkle math | What does replacing sibling hashes with a vector commitment buy and cost? | Smaller aggregated openings are a candidate benefit; update, verification, dependency, side-channel, and migration costs remain. |
| Migration | Can a live chain change state commitments? | Yes, through a consensus-specified transition that binds the old representation to the new one; an overlay is one option. A new genesis avoids legacy conversion but not upgrade and historical-verification duties. |
| Parameters | What is the “static base,” and does it require trust? | Split public commitment parameters, genesis state, genesis root, and history genesis. They are not one object. |
| Ceremony | Can simulated participants, hardware, PoW, or a VDF remove setup trust? | Not by assertion. A transparent generator basis needs a security argument against efficient discovery of any nonzero linear relation among all basis points; an updatable ceremony addresses a different setup risk. |
| Mint/sink fairness | Can computational work make issuance and destruction provably fair? | Work may support Sybil resistance, ordering, or delay. Fairness requires explicit economic invariants and threat assumptions. |
| Tick history | Can state be treated as a deterministic sequence of ticks? | Yes. This is event sourcing and is already close to Bitblock’s local fixed-tick fold. |
| Statelessness | Can validators operate without a full local state database? | Some validators can, if complete touched-state witnesses and all execution bytes are available. The network is not thereby storage-free. |
| Fast bootstrap | Can old ticks be compressed so a new node avoids replay from genesis? | A verified snapshot or recursive validity proof can reduce verification work; neither reveals or preserves discarded state data. |
| Old-state retrieval | Can a user recover an old balance from an archived proof? | An old proof establishes an old claim only. Current usability needs a state-expiry/resurrection protocol that excludes intervening spends and replay. |
| Recursive proofs | Can one proof carry forward the validity of all earlier transitions? | Potentially, for one precisely encoded relation and base case. It does not establish canonicality or data availability. |
| Retrieval network | Can history and state be distributed across small nodes? | A content-addressed retrieval network is a candidate availability layer; exact replication, latency, repair, and incentive guarantees must be designed and measured. |
| Circuit/zkVM tooling | Should Bitblock use Halo2, Circom, Nova, a zkVM, or Rust guest code? | Tool choice is downstream of the exact relation, cost budget, recursion need, cryptographic profile, and independent-verifier requirement. |

## Core thesis retained

The useful idea is not “state is an illusion.” It is:

> Logical state at tick `n` is the deterministic projection of an authorized chain-launch package
> and an ordered admitted transition sequence. The launch package binds the pre-ID genesis bytes to
> one post-ID initial admitted profile, transition specification, typed root-parity record, and
> release authority. Reconstructing or serving that state additionally requires
> the sequence, or equivalent state material, to remain available. A database is one possible
> materialization and may be optional for some verifier roles.

Let:

```text
G_p       one admitted profile and its exact scope, required-domain registry, limits, policy IDs,
          inactive sentinels, codecs, and authority-context projections
P_(p,d)   immutable commitment and value-codec parameters for required typed domain d under G_p
T_t       exact transition specification or program selected by transition-specification ID t
S_0       declared complete logical genesis state for G_p
L_0       authorized post-ID launch manifest selecting the one initial G_p, T_t, and typed X_0
C_n       exact RequiredDomainV1 -> typed domain-root map after tick n
X_n       complete execution root composed from G_p and C_n
H_n       separately typed history accumulator after tick n
A_n       deterministically ordered proposed actions only
D_n       deterministic auxiliary and system inputs only
W_n       typed per-domain openings plus profile-required update material
E_n       canonically encoded effects and receipts
S_n       complete post-execution logical state, not only world occupancy
```

These are abstract relationships, not wire types:

```text
(S_n, E_n) = Step(T_(t_n), n, S_(n-1), A_n, D_n)
C_n        = CommitTypedDomains(G_(p_n), {P_(p_n,d)}, S_n)
X_n        = ComposeExecutionRoot(G_(p_n), C_n)
```

Every scope, genesis, candidate-profile, and other pre-admission identifier is derived with the
single protocol-frozen `ProfileIdHashV1`; a candidate's selected cryptographic suites cannot choose
or reinterpret the function that identifies the candidate. Profile-selected hashes apply only
where the already identified and admitted profile assigns them.

The raw `(genesis payload, genesis_id)` pair intentionally predates `profile_id` and cannot select
an initial interpreter by itself. The Layer 1 `ChainLaunchManifestV1` is derived afterward without
feeding back into `genesis_id`; bootstrap and the first accepted record verify that manifest and its
detached launch authorization.

Binding `D_n` makes replay inputs explicit; it does not prove an oracle value true, randomness
unbiased, or acquisition honest.

`G_p` must resolve every component rather than allow callers to mix compatible-looking IDs. `C_n`
must contain exactly the required world, episode, authorization, nonce, parameter/governance,
validator-schedule, and activated optional roots under their own typed profiles. The current
occupancy-only `world_tag` is not `X_n`.

This note deliberately defines no `TickCoreV1`, envelope codec, block identifier, or competing
history formula. The repository's sole proposed byte-contract owner is
[`LAYER1_DESIGN_PAPER.md` §4.4](LAYER1_DESIGN_PAPER.md#44-proposed-block-identifier-consensus-evidence-and-receipts).
That proposal already separates ordered actions from deterministic auxiliary inputs, binds effects
through receipts, binds the chain-launch identity and the deterministic envelope owner plus
parent-authorized execution/consensus/availability policies and authority context, commits the exact availability payload, derives a non-circular
block identifier, and keeps signatures and consensus evidence outside that identifier. It remains
proposed, unimplemented, and non-authoritative.

Its verifier rejects over-bound outer frames before nested materialization, resolves the accepted
parent and envelope owner before selecting any codec/hash/signature suite, checks every
parent-authorized limit/role/policy before execution, and only then decodes payloads, recomputes
commitments, executes, and checks successor state. This is an ordering requirement, not evidence
that the future implementation meets its budgets.

A payload commitment establishes byte identity, not retrieval. Consensus and availability evidence
must each bind the candidate identifier, selected policy, applicable authority/epoch context, and
the exact status or availability predicate being claimed. A probabilistic confirmation must not be
renamed finality. The future protocol must still decide whether one network unit means one local
simulation tick, one action, or one deterministic batch; the current 60 Hz local tick must not
silently become consensus time.

Ordinary witnessed transitions require `p_n = p_(n-1)` and a parent-authorized `T_(t_n)`. A
commitment-profile change is a different transition class:

```text
MigrationTransition(
    old_profile_id,
    new_admitted_profile_id,
    migration_spec_id,
    typed_old_execution_root_ref,
    typed_new_execution_root_ref,
    activation_authorization_ref,
    complete_bounded_correspondence_evidence
)
```

Touched-state openings cannot establish equality of untouched state across unrelated profiles. An
admitted migration would additionally need parent-authorized activation, deterministic complete
old/new correspondence, failure and restart semantics, retained old-profile verification context,
and available migration material. Root correspondence alone would not authorize the upgrade or
select its fork. Ordinary units carry one unchanged profile context; a migration unit carries the
explicit old/new context while the old parent-admitted profile solely owns its envelope codec,
object hashes, commitments, policies, and proposal credentials. The first migration form has an
empty signed-transaction sequence, an empty application-subtick schedule, and exactly one
authorized migration system transition and receipt. It uses the migration-specific commitment-set
transition owned by the Layer 1 paper and authenticated-state plan. No mixed old/new ordinary-action
interpretation is implied; such a form would require a separately versioned phase-tagged schema.

## Witnessed transition, stated precisely

A Verkle, Merkle-Patricia, or sparse-Merkle witness authenticates membership or non-membership
claims relative to one supplied typed domain root and may carry that profile's update material. It
is not directly a proof under composed execution root `X_n`, the action, the state delta, a proof
of authorization, or a proof that the transition function was executed correctly.

An abstract execution-witness bundle therefore needs the parent commitment-set/execution-root
context plus per-domain entries naming required domain, typed pre-domain root, membership or absence
proofs, and update material. The verifier first checks that the domain-root set exactly matches
`G_p` and recomposes `X_(n-1)`, then verifies each map proof under its own `P_(p,d)`. Inactive,
unknown, omitted, duplicated, and cross-domain entries reject. Access entries name domain, key, and
mode.

A stateless transition verifier would:

1. resolve one exact admitted chain/genesis/profile, required-domain registry, typed commitment
   profiles, codecs, bounds, policies, authority context, and transition program;
2. verify parent linkage and check the parent roots against a previously admitted canonical context
   or an explicitly trusted checkpoint;
3. retrieve the bounded action body `A_n`, disjoint deterministic input record `D_n`, typed witness,
   active code, receipts/effects, and availability payload; verify every byte commitment;
4. decode and bound `W_n`, verify the exact typed domain-root set against `X_(n-1)`, and authenticate
   supplied membership or absence openings under their own roots;
5. execute `T_(t_n)` with deterministic first-touch semantics: every key's first pre-state-dependent
   access, including every write target, requires a pre-root opening; later reads after an in-batch
   write resolve from the deterministic overlay;
6. record the actual typed read/write set and compare it with the committed access manifest;
   missing, duplicate, inactive-domain, unauthorized-extra, and non-canonical material rejects;
7. require final update evidence for every written key, derive each post-domain root, recompute
   `C_n` and `X_n`, and derive all authorization, nonce, conflict, failure, accounting, and resource
   outcomes;
8. recompute each ordered effects commitment, its receipt, the ordered receipts root, history root,
   body/context/header fields, and availability payload exactly as owned by the Layer 1 paper; and
9. only after execution validity is established, independently verify the candidate's typed
   consensus-status evidence and availability predicate under their declared policies and threat
   model.

The concrete owner represents availability as detached assessment/evidence bound to the block,
policy, authority/epoch context, and exact predicate. A profile may prove that its already-bound
consensus evidence entails that assessment, or may require separate evidence; a node-local
retrieval observation is not silently promoted into the accepted record.

A profile may choose explicitly chained per-action roots instead of one batch pre-root plus overlay,
but then it must bind and verify every intermediate root. The two update models cannot be mixed
implicitly.

This lets a verifier avoid a complete local state database. A full-state builder or another state
provider must still hold or reconstruct enough current data to generate `W_n`. Ethereum’s own
[statelessness explanation](https://ethereum.org/roadmap/verkle-trees) similarly describes a
witness as the state fragments required for execution plus a proof that they belong to the full
state.

## Five layers that must not collapse

| Layer | It can establish | It cannot establish alone |
| --- | --- | --- |
| Deterministic tick fold | Exact result of admitted inputs under one transition version | Which distributed proposal is canonical |
| Authenticated state witness | Touched values or absence under a supplied root | Correct execution, latest state, or data availability |
| Sequential clock or VDF | A sequence or delay property under exact assumptions | Fork choice, finality, or monetary fairness |
| Consensus/finality evidence | Which tick identifier has the declared network status | That required bodies, witnesses, or state remain retrievable |
| Availability/archive layer | Retrieval under a stated retention and fault model | Execution correctness or canonicality |

An optional recursive validity proof would be a sixth layer. It may compress verification of a
precise transition relation but cannot reveal arbitrary committed values, preserve transaction
bodies, choose a fork, or guarantee availability.

## State root, history root, and snapshot are different

An on-then-off sequence can return to an earlier world state. The state root may therefore repeat
while the history root and episode/receipt commitments continue to change. Forcing chronology into
the state root destroys the meaning of “same current state.” This separation is already explicit in
[`AUTHENTICATED_STATE_PLAN.md`](AUTHENTICATED_STATE_PLAN.md#world-roots-are-not-history-roots).

A sound commitment is binding under its scheme assumptions; it is not lossless compression. A
constant-size root cannot reveal an arbitrary number of balances or coordinates. If every holder
deletes both the state and a complete reconstructible history, the root becomes a commitment to
unavailable data.

A snapshot is an acceleration artifact rather than authority. A usable snapshot needs canonically
encoded state chunks for a claimed checkpoint plus a manifest binding at least its height,
protocol/profile versions, complete domain set, expected roots, and chunk identities. A restorer
independently rebuilds the roots, checks them against a suitably authenticated checkpoint, and then
replays subsequent transitions.

## Node and storage roles

| Role | Minimum retained or received material | Main non-claim |
| --- | --- | --- |
| Full-state builder/witness producer | Current materialized state, commitment update data, action body, active code/profile | Holding state does not authorize its proposed ordering. |
| Stateless validator | Trusted parent context, current root, complete body, touched-state witness, exact executor | It cannot answer arbitrary state queries from the root alone. |
| Light client | Header chain or checkpoint, consensus evidence, requested value/receipt witnesses | It does not execute every transition or guarantee server availability. |
| Archive/recovery node | Historical bodies, receipts, versions, proofs, snapshots according to policy | Retention does not make a fork canonical. |
| RPC/indexer | Query indexes and proof-producing access to state/history | Fast answers are not authority without locally checked roots and proofs. |

The network must explicitly fund, require, or otherwise justify every availability assumption.
“Someone will archive it” is not a protocol.

## Bootstrap choices

| Method | What the joining node obtains | Cost and trust boundary |
| --- | --- | --- |
| Genesis replay | Authorized chain-launch package plus every required historical transition and protocol version | Highest data and execution work; still needs a canonical-chain rule and complete archives. A raw genesis pair cannot select the initial profile. |
| Verified snapshot | Materialized state chunks and a manifest at an authenticated checkpoint that binds the chain-launch ID | Fast, but requires root recomputation plus an explicit checkpoint/finality trust model. |
| Recursive validity proof plus state retrieval | Launch-bound checkpoint, proof of a contiguous transition relation, final roots, and separately retrieved state | Compresses correctness checking; does not supply state values or availability. |

No method allows a node to “instantly know every balance” from a root. It can rapidly verify one
supplied balance witness, or verify that a root followed from a prior root, while full enumeration
still requires the data.

## Verkle lane retained and narrowed

**CANDIDATE:** A Verkle tree is parameterized by a position-binding vector commitment with sound,
profile-defined opening and multiproof verification. It is not synonymous with KZG and does not
inherently require a trusted setup. The Ethereum Foundation's concrete 2021 research design uses
Pedersen commitments and an inner-product argument; it is a historical research design, not
evidence of current deployment. Other profiles have different setup, proof, update, and
verification properties.

A Pedersen/IPA profile must freeze the complete generator derivation, group and discrete-log
assumptions, subgroup/cofactor checks, point/scalar encoding, and transcript domain separation. Its
security argument must rule out efficient discovery of any nonzero linear relation among the whole
basis, not merely an unknown pairwise discrete logarithm. Transparent derivation removes toxic
waste; it does not remove curve, hash-to-curve, implementation, or side-channel assumptions.

The value proposition is smaller aggregated state witnesses for wide trees. The cost surface must
still include commitment updates, witness generation and verification, group/field work, storage
churn, caches, dependencies, audited-code and side-channel surface, malformed-input handling, and
migration/reversal. A proof-size win alone does not select the construction.

The raw file's fixed whole-witness `1–2 KB` claim is **REJECTED**. Some commitment profiles can have
fixed-size opening-proof components, but a complete state witness also carries opened keys and
values plus workload- and profile-dependent material. The
[Ethereum Verkle overview](https://ethereum.org/roadmap/verkle-trees) gives a pedagogical estimate
of roughly 150 kB for 1,000 opened leaves under its stated assumptions, while the Stagnant
[EIP-6800 at the audited EIPs revision](https://github.com/ethereum/EIPs/blob/e400af2874d6728bfd369f1851c24f250c8825f7/EIPS/eip-6800.md)
gives an approximate average-per-account estimate. These are Ethereum design estimates, not
benchmarks or Bitblock measurements.

Starting Bitblock with a selected commitment at genesis would avoid conversion from a live legacy
tree. It would not remove:

- public-parameter and codec versioning;
- state and witness availability;
- full-state witness producers;
- independent verification;
- storage growth and garbage collection;
- cryptographic failure and dependency risk;
- historical-proof support; or
- a safe upgrade/reversal policy.

`VERKLE-R1` remains a separate feasibility lane. The planned first authenticated-state research
checkpoint is still the measured hash-tree assay in
[`AUTHENTICATED_STATE_PLAN.md`](AUTHENTICATED_STATE_PLAN.md).

## Sequential clock and Proof-of-History lane

Bitblock already has an ordered local tick fold and parent-linked content identities. A distributed
ledger can order blocks using parent identifiers plus its selected consensus rule without adding a
VDF. A sequential clock is justified only by a concrete requirement that ordinary height and
consensus ordering cannot meet.

Solana’s own [Tower BFT description](https://solana.com/uk/news/tower-bft--solana-s-high-performance-implementation-of-pbft)
describes Proof of History as a source of time before consensus, while Tower BFT supplies the
consensus role. A PoH-like sequence can make modifying an earlier entry require recomputing its
suffix under stated hardware assumptions. It does not by itself prevent equivocation, withholding,
alternative sequences, censorship, or forks, and it is not an external wall-clock timestamp.

**REJECTED:** PoH, a VDF, or proof of work does not make minting or sinking rules fair. It can
contribute delay, ordering, proposer selection, or Sybil-resistance assumptions. The policy and its
distributional consequences remain separate.

## Recursive validity and zk tooling lane

**CANDIDATE:** A recursive or incrementally verifiable proof could attest that a pinned transition
program carried one checkpoint root through a contiguous tick interval to a final state and history
root. At minimum the proven relation must bind:

- protocol, chain, genesis, profile, and transition-program identity;
- an explicit base case binding the checkpoint identifier and roots; checkpoint canonicality
  remains externally verified unless the proof relation explicitly verifies consensus;
- contiguous tick numbers and parent/post roots;
- ordered-action and deterministic-external-input commitments;
- authorization, nonce, conflict, failure, accounting, and resource rules;
- state-opening verification and exact commitment updates; and
- the prior proof’s complete public outputs.

The source’s Circom-shaped example is **REJECTED**: it copies an unconstrained supplied
`current_state_root` to the output and never proves that executing the action produced it. Code
syntax is not a transition relation.

Halo2, Circom, Nova, RISC Zero, SP1, and other circuit/zkVM systems belong in a later tool assay.
They are not interchangeable, and their names answer none of the profile, relation, recursion,
performance, trusted-computing-base, or independent-verifier questions. A zkVM may eventually let a
pinned Rust guest express the transition, but proving a program image ran does not prove that its
economic policy is fair or that its output is canonical and available.

Primary research starting points include the [Halo2 recursion notes](https://zcash.github.io/halo2/background/recursion.html)
and the [Nova paper](https://eprint.iacr.org/2021/370.pdf). They are research inputs, not Bitblock
selections.

## History expiry, state expiry, and retrieval

History expiry and state expiry are different designs.
[EIP-4444 at the audited EIPs revision](https://github.com/ethereum/EIPs/blob/e400af2874d6728bfd369f1851c24f250c8825f7/EIPS/eip-4444.md)
is a Stagnant Ethereum proposal about old headers, bodies, and receipts on the normal execution
peer-to-peer network. It does not make a dormant current balance disappear.

Given a separately authenticated canonical old root, a valid old membership proof establishes that
a value existed relative to that root. It cannot establish that the value remains unspent under the
current root. Any future state-expiry/resurrection design would need an exact epoch model, current
absence or nullifier evidence, nonce and version continuity, ownership/authorization, and
prevention of replay, double revival, and intervening consumption.

The [audited Portal specifications revision](https://github.com/ethereum/portal-network-specs/tree/1ac0152620fb6426dd2500f0d43459f49d4dc25d)
is useful as a reference for content-addressed, verifiable retrieval. The exact `10 GB` slices, 32
replicas, sub-second retrieval, automatic self-healing, and tit-for-tat guarantees in the raw file
are **REJECTED** as unsupported. Bitblock would need its own retention target, replication or
coding rule, repair process, adversarial model, incentive/funding mechanism, and measurable service
loss condition.

## Mint and sink fairness, decomposed

“Provably fair” is not one cryptographic predicate. A Bitblock asset design would have to separate
at least:

- deterministic issuance and sink effects from canonical inputs;
- no hidden mint, freeze, or confiscation authority;
- exact supply accounting, issuance schedule, and any claimed maximum-supply bound;
- genesis allocation and validator/proposer reward distribution;
- equivalent treatment rules for equivalent protocol roles;
- Sybil, stake, hardware, capital-concentration, and bribery assumptions;
- censorship, reorganization, withholding, and grinding exposure;
- storage-rent payer, price, grace, expiry, and ownership consequences; and
- governance and upgrade authority over every economic parameter.

Proof of work verifies a target-satisfying output whose expected production cost follows from the
hash-function and difficulty model; it does not prove measured hashes, energy, or expenditure. A
VDF can establish a sequential-computation claim under its assumptions. Neither establishes that
the selected issuance policy is normatively or distributionally fair.

## Rejected-claim ledger

| Raw formulation | Disposition | Compact correction |
| --- | --- | --- |
| A Verkle proof is the state delta or wave. | **REJECTED** | A witness authenticates opened pre-state; actions plus `Step` determine the delta. |
| Every tick is a complete mathematical snapshot. | **REJECTED** | A root commits to a snapshot but does not contain its values. |
| Tick 0 is the static base. | **REJECTED** | Commitment parameters, `S_0`, `X_0`, and `H_0` are separate typed objects. |
| The permanent database disappears. | **REJECTED** | Full local state may disappear for one verifier role; system-wide information retention remains. |
| Proof history is sufficient to replay state. | **REJECTED** | Full replay needs an authorized post-ID launch binding, complete ordered actions and deterministic inputs, and every applicable transition/codec version; stateless replay additionally needs witnesses. |
| A root lets a new node know every balance. | **REJECTED** | A root verifies supplied openings; it cannot enumerate committed data. |
| A valid state witness proves a valid transaction. | **REJECTED** | Authorization and deterministic execution must also be checked, or separately validity-proven. |
| PoH prevents forged forks and proves exact wall time. | **REJECTED** | It supplies a scoped sequence/delay claim; consensus and external assumptions remain. |
| Recursive proof means history and state can be deleted. | **REJECTED** | It compresses checking of one relation, not the data needed for recovery and queries. |
| Ancient proof resurrects an unspent balance. | **REJECTED** | It proves an old claim only; current applicability needs a dedicated expiry protocol. |
| Verkle always means KZG and trusted setup. | **REJECTED** | Verkle is parameterized by a vector commitment; profiles have different setup assumptions. |
| One fixed proof-size or speed number describes Verkle or a zkVM. | **REJECTED** | Bind every measurement to workload, scheme, version, encoding, hardware, and implementation. |
| Portal guarantees fixed replicas, latency, and immortal data. | **REJECTED** | Availability requires a Bitblock-specific protocol, incentives, measurements, and fault model. |
| PoW or a VDF proves monetary fairness. | **REJECTED** | PoW supports an expected-cost claim and a VDF a scoped delay claim; fairness is a separate bundle of economic properties. |

## Candidate capability notes, not applied to the tree

The raw conversation suggests three potentially useful future packages. Their labels are schematic,
not capability-tree nodes or dependency declarations; any promotion must be reconciled into
[`CAPABILITY_TREE.md`](CAPABILITY_TREE.md) with one exact artifact, gate, dependency closure, and
non-claim.

### `WITNESSED-TRANSITION-R1` candidate

- **Purpose:** test complete stateless re-execution of one bounded transition batch.
- **Possible authority prerequisites, not DAG edges:** an admitted profile; exact
  `TRANSITION-SPEC-1`, `COMMIT-1`, `EXECUTOR-1`, `EXECUTION-ROOT-1`, `PROTOCOL-ID-1`, `BOUNDS-1`,
  and `SYNC-1` artifacts; and independent codecs/verifiers. A world-only research prototype would
  not establish this full package.
- **Gate idea:** the exact typed domain set recomposes the parent execution root; every first-touch
  read and write target is authenticated under its domain pre-root; overlay reads and final update
  evidence are complete; independent execution reproduces effects, receipts, and execution root;
  one-over-limit, malformed, withheld, cross-domain, wrong-profile, and partial witnesses fail
  closed.
- **Non-claim:** witnessed execution is not succinct validity, consensus, or availability.

### `VALIDITY-PROOF-R1` candidate

- **Purpose:** measure an independently verified proof of the exact transition relation, optionally
  recursive over a contiguous interval.
- **Would depend on:** admitted witnessed-transition semantics, exact public inputs/base case,
  cryptographic profile, bounds, independent verifier, source-to-relation/refinement evidence, and
  upgrade policy.
- **Gate idea:** powered mutations to prior proof outputs, program/profile ID, tick continuity,
  action root, state openings, post-root, receipts, and base case all reject.
- **Non-claim:** a validity proof does not select a canonical fork or retain state/history bytes.

### `SEQUENTIAL-CLOCK-R1` candidate

- **Purpose:** determine whether Bitblock has a requirement for externally verifiable sequential
  work or delay beyond ordinary height, parent links, and consensus ordering.
- **Would depend on:** a written requirement, exact VDF/sequence construction, parameter and timing
  assumptions, resource bounds, threat model, and integration with—never substitution for—the
  chosen consensus protocol.
- **Gate idea:** alternative-sequence, withholding, precomputation, hardware-speedup, fork,
  calibration, restart, and liveness cases have explicit outcomes.
- **Non-claim:** sequential work is not consensus, exact wall time, randomness, or economic fairness.

## Dependency-ordered open questions

1. What exactly is one future authority tick: local simulation tick, action, or ordered batch?
2. Which actions, failures, no-ops, idle ticks, and deterministic external inputs enter its body?
3. What transition and codec versions make replay complete across upgrades?
4. Which account, authorization, ownership, and accounting model exists before balances are named?
5. Which hash-tree commitment passes the current common assay, and what measured requirement would
   justify later Verkle work?
6. Who holds full current state and produces complete witnesses?
7. What availability rule covers bodies, witnesses, program code, snapshots, and historical
   versions before any validator accepts a tick?
8. What consensus/finality rule selects the canonical tick chain?
9. Which launch-bound bootstrap contract is supported: authorized chain-launch replay, verified snapshot, recursive proof, or a
   precisely scoped combination?
10. What concrete latency, bandwidth, or verifier-cost requirement would justify recursive proving?
11. What property unavailable from height and consensus would justify a VDF or PoH-like clock?
12. Is state expiry required at all? If so, how are current applicability and no-double-revival
    proven?
13. Which economic properties make “fair mint and sink” a falsifiable claim?

Until those questions have artifacts and gates, the concise model is:

> Under a future admitted profile, a Bitblock unit accepted at its profile-declared status could
> commit the ordered transition body, deterministic auxiliary inputs, typed witness bytes, effects,
> receipts, complete post-execution root, history link, and exact availability payload. When those
> bytes are available and validate against their commitments, a stateless verifier could check
> execution without retaining full state. Full-state producers and archive providers would still
> need explicit retention duties; the declared consensus/finality rule under its threat assumptions
> would select canonical units. Optional Verkle, sequential-clock, and recursive-proof lanes must
> each justify their own cost and security surface.

## Source queue with scoped authority

Statuses and mutable-page descriptions below were retrieved on 2026-08-27. EIP status observations
are pinned to `ethereum/EIPs` commit `e400af2874d6728bfd369f1851c24f250c8825f7`; the Portal
reference is pinned to commit `1ac0152620fb6426dd2500f0d43459f49d4dc25d`. Live explanatory pages
must still be refreshed and pinned before supporting a protocol decision.

| Source | Scope here |
| --- | --- |
| [Ethereum MPT documentation](https://ethereum.org/developers/docs/data-structures-and-encoding/patricia-merkle-trie/) | Existing Ethereum trie mechanics and encoding, not client-I/O benchmarks. |
| [Kuszmaul, *Verkle Trees*](https://math.mit.edu/research/highschool/primes/materials/2018/Kuszmaul.pdf) | Original Verkle construction instantiated with the Catalano–Fiore vector commitment; its setup, proof, and benchmark results do not transfer to Pedersen/IPA or KZG. |
| [Ethereum Foundation 2021 Verkle note](https://blog.ethereum.org/2021/12/02/verkle-tree-structure) | One Pedersen/IPA research design, not current deployment. |
| [Bulletproofs paper](https://eprint.iacr.org/2017/1066.pdf) | Primary inner-product-argument source and assumptions, not a Bitblock generator profile. |
| [Ethereum Verkle overview](https://ethereum.org/roadmap/verkle-trees) | First-party explanatory witness model and estimates, not a benchmark or specification. |
| [EIP-6800](https://github.com/ethereum/EIPs/blob/e400af2874d6728bfd369f1851c24f250c8825f7/EIPS/eip-6800.md) and [EIP-7612](https://github.com/ethereum/EIPs/blob/e400af2874d6728bfd369f1851c24f250c8825f7/EIPS/eip-7612.md) | Stagnant Verkle state/overlay proposals at the audited revision, not current Ethereum selection. |
| [EIP-7864](https://github.com/ethereum/EIPs/blob/e400af2874d6728bfd369f1851c24f250c8825f7/EIPS/eip-7864.md) | Draft competing unified binary-tree proposal at the audited revision; its hash choice remained open. |
| [KZG paper](https://www.iacr.org/archive/asiacrypt2010/6477178/6477178.pdf) and [Ethereum KZG ceremony](https://ceremony.ethereum.org/) | KZG assumptions and the EIP-4844 ceremony; not the Pedersen/IPA Verkle setup. |
| [Solana whitepaper](https://solana.com/solana-whitepaper.pdf) and [Tower BFT note](https://solana.com/uk/news/tower-bft--solana-s-high-performance-implementation-of-pbft) | PoH sequence design and its separation from consensus. |
| [Boneh et al., *Verifiable Delay Functions*](https://eprint.iacr.org/2018/601.pdf) | VDF definition and assumptions, not a consensus or fairness result. |
| [Bitcoin whitepaper](https://bitcoin.org/bitcoin.pdf) | Hash-target and expected-work model, not evidence of measured energy or fairness. |
| [EIP-4444](https://github.com/ethereum/EIPs/blob/e400af2874d6728bfd369f1851c24f250c8825f7/EIPS/eip-4444.md) | Stagnant history-expiry proposal at the audited revision, not state expiry. |
| [Portal specifications](https://github.com/ethereum/portal-network-specs/tree/1ac0152620fb6426dd2500f0d43459f49d4dc25d) | Audited retrieval-protocol revision, not a Bitblock availability guarantee. |
| [Halo2 recursion notes](https://zcash.github.io/halo2/background/recursion.html) and [Nova paper](https://eprint.iacr.org/2021/370.pdf) | Distinct recursive-proof research inputs, not selections or benchmarks. |

Later evidence must match its claim type: construction security needs an exact theorem, model, and
assumptions; parameter/setup security needs a pinned derivation or ceremony transcript;
implementation security needs conformance evidence and independent review; performance or
deployment needs a dated measurement with workload, artifact, version, hardware, assumptions, and
loss condition. This queue is not evidence that Bitblock selected any listed technology.

## Compression receipt boundary

This note preserves the raw source fingerprint, question spine, useful architecture, corrections,
candidate lanes, and source queue. It intentionally does not preserve every phrase or false claim;
Git history will retain later revisions of this note, while the original downloaded source remains
an external byte-level provenance artifact on this machine. The checker validates this file's
Markdown structure, repository-contained tracked relative planning files/fragments, frozen audit
and receipt identity, receipt-field agreement, and the receipt's historical distillation
object/count claims against Git. It does not validate external URLs, recover the external raw
source, or establish the completeness or truth of the technical contents.
