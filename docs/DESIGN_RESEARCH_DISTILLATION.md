# Distilled design research notes

**Checkpoint:** source-receipted research distillation, 2026-08-27
**Authority:** notes only; not a protocol specification, capability-tree decision, implementation
claim, cryptographic result, or Layer 1 admission artifact

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
| Ceremony | Can simulated participants, hardware, PoW, or a VDF remove setup trust? | Not by assertion. Transparently derived generators with no known inter-generator discrete-log relations and updatable ceremonies address different setup risks. |
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

> Logical state at tick `n` is the deterministic projection of an admitted genesis state and an
> ordered admitted transition sequence. Reconstructing or serving that state additionally requires
> the sequence, or equivalent state material, to remain available. A database is one possible
> materialization and may be optional for some verifier roles.

Let:

```text
P_v      immutable codec and commitment parameters for admitted profile version v
T_v      exact transition specification or program for protocol version v
S_0      declared logical genesis state for the admitted chain profile
X_0      CommitExecution(P_(v_0), S_0)
H_0      domain-separated history genesis
A_n      deterministically ordered proposed actions and deterministic external inputs for tick n
W_n      authenticated membership/non-membership openings and profile-required update material
E_n      canonically encoded effects and receipts
S_n      complete post-execution logical state, not only world occupancy
X_n      complete post-execution root over every activated typed state domain
H_n      history accumulator after tick n
```

The pure transition relation is:

```text
(S_n, E_n) = Step(T_(v_n), n, S_(n-1), A_n)
X_n        = CommitExecution(P_(v_n), S_n)
```

`X_n` must compose the admitted world, authorization, nonce, accounting, asset, governance, and
other activated domain roots without silently treating the current occupancy-only `world_tag` as
the full execution state. A commitment-profile upgrade selects a new immutable `P_v`; a transition
upgrade selects a separately identified `T_v`.

A proposed network transition record can be derived without a circular consensus signature:

```text
TickCoreV1 = Canonical(
    schema_version,
    chain_id,
    genesis_id,
    commitment_profile_id,
    transition_spec_id,
    tick_or_height,
    parent_tick_id,
    parent_history_root,
    protocol_version,
    actions_root,
    deterministic_inputs_root,
    pre_execution_root,
    access_manifest_root,
    state_witness_commitment,
    receipts_root,
    post_execution_root,
    payload_manifest_root
)

tick_id_n = H(Domain("bitblock/tick-id/v1") || LengthFrame(TickCoreV1))
H_n       = H(Domain("bitblock/history/v1") || parent_history_root || tick_id_n)

TickEnvelopeV1 = Canonical(
    TickCoreV1,
    tick_id_n,
    H_n,
    consensus_or_finality_evidence,
    availability_evidence
)
```

Every object hashed into `TickCoreV1` excludes `tick_id_n`, `H_n`, and certificates over either
value. Consensus and availability evidence live in the external envelope, so the identity is not
self-referential. `payload_manifest_root` binds named bytes; it does not prove those bytes are
retrievable. Exact schemas remain **OPEN**. In particular, the future protocol must decide whether
one network tick means one local simulation tick, one action, or one deterministically ordered
batch. The current 60 Hz local tick must not silently become consensus time.

## Witnessed transition, stated precisely

A Verkle, Merkle-Patricia, or sparse-Merkle witness authenticates membership or non-membership
claims relative to one supplied root and may carry profile-required update material. It is not the
action, the state delta, a proof of authorization, or a proof that the transition function was
executed correctly.

A stateless transition verifier would:

1. resolve the exact admitted chain, genesis, commitment profile, codec, and transition program;
2. verify parent linkage and check the parent roots against a previously admitted canonical context
   or an explicitly trusted checkpoint;
3. retrieve the bounded action, deterministic-input, witness, code, and manifest bytes and verify
   each against its committed root;
4. decode and bound `W_n`, authenticate every supplied membership or non-membership opening under
   `X_(n-1)`, and enforce the profile's structural and canonical-encoding rules;
5. execute `T_(v_n)`, require one unambiguous authenticated result at every state read, and record
   the actual read/write set plus authorization, nonce, conflict, failure, accounting, resource,
   and receipt outcomes;
6. compare the actual read/write set with the committed access manifest and enforce the declared
   duplicate and unused-material policy;
7. apply every write through the selected commitment profile and derive `X_n` and `E_n`;
8. compare the computed input, witness, receipt, payload-manifest, and execution-root commitments
   with `TickCoreV1`, then derive `tick_id_n` and `H_n`; and
9. after execution validity is established, independently verify the candidate tick's
   consensus/finality evidence and evaluate its availability evidence under the declared fault
   model.

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
| Genesis replay | Genesis plus every required historical transition and protocol version | Highest data and execution work; still needs a canonical-chain rule and complete archives. |
| Verified snapshot | Materialized state chunks and a manifest at an authenticated checkpoint | Fast, but requires root recomputation plus an explicit checkpoint/finality trust model. |
| Recursive validity proof plus state retrieval | Checkpoint, proof of a contiguous transition relation, final roots, and separately retrieved state | Compresses correctness checking; does not supply state values or availability. |

No method allows a node to “instantly know every balance” from a root. It can rapidly verify one
supplied balance witness, or verify that a root followed from a prior root, while full enumeration
still requires the data.

## Verkle lane retained and narrowed

**CANDIDATE:** A Verkle tree is parameterized by a binding vector-commitment scheme with efficient
opening proofs. It is not synonymous with KZG and does not inherently require a trusted setup. The
Ethereum Foundation's concrete 2021 research design uses Pedersen commitments and an inner-product
argument; it is a historical research design, not evidence of current deployment. Other profiles
have different setup, proof, update, and verification properties.

The value proposition is smaller aggregated state witnesses for wide trees. The cost surface must
still include commitment updates, witness generation and verification, group/field work, storage
churn, caches, dependencies, audited-code and side-channel surface, malformed-input handling, and
migration/reversal. A proof-size win alone does not select the construction.

The raw file's fixed whole-witness `1–2 KB` claim is **REJECTED**. Some commitment profiles can have
fixed-size opening-proof components, but a complete state witness also carries opened keys and
values plus workload- and profile-dependent material. The
[Ethereum Verkle overview](https://ethereum.org/roadmap/verkle-trees) gives a pedagogical estimate
of roughly 150 kB for 1,000 opened leaves under its stated assumptions, while the Stagnant
[EIP-6800](https://eips.ethereum.org/EIPS/eip-6800) gives an approximate average-per-account
estimate. These are Ethereum design estimates, not benchmarks or Bitblock measurements.

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

History expiry and state expiry are different designs. [EIP-4444](https://eips.ethereum.org/EIPS/eip-4444)
is a Stagnant Ethereum proposal about old headers, bodies, and receipts on the normal execution
peer-to-peer network. It does not make a dormant current balance disappear.

Given a separately authenticated canonical old root, a valid old membership proof establishes that
a value existed relative to that root. It cannot establish that the value remains unspent under the
current root. Any future state-expiry/resurrection design would need an exact epoch model, current
absence or nullifier evidence, nonce and version continuity, ownership/authorization, and
prevention of replay, double revival, and intervening consumption.

The [Portal Network specifications](https://github.com/ethereum/portal-network-specs) are useful as
a reference for content-addressed, verifiable retrieval. The exact `10 GB` slices, 32 replicas,
sub-second retrieval, automatic self-healing, and tit-for-tat guarantees in the raw file are
**REJECTED** as unsupported. Bitblock would need its own retention target, replication or coding
rule, repair process, adversarial model, incentive/funding mechanism, and measurable service loss
condition.

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

Proof of work can establish expended work under a hash/difficulty rule. A VDF can establish a
sequential-computation claim under its assumptions. Neither establishes that the selected issuance
policy is normatively or distributionally fair.

## Rejected-claim ledger

| Raw formulation | Disposition | Compact correction |
| --- | --- | --- |
| A Verkle proof is the state delta or wave. | **REJECTED** | A witness authenticates opened pre-state; actions plus `Step` determine the delta. |
| Every tick is a complete mathematical snapshot. | **REJECTED** | A root commits to a snapshot but does not contain its values. |
| Tick 0 is the static base. | **REJECTED** | Commitment parameters, `S_0`, `X_0`, and `H_0` are separate typed objects. |
| The permanent database disappears. | **REJECTED** | Full local state may disappear for one verifier role; system-wide information retention remains. |
| Proof history is sufficient to replay state. | **REJECTED** | Full replay needs admitted genesis, complete ordered actions and deterministic inputs, and every applicable transition/codec version; stateless replay additionally needs witnesses. |
| A root lets a new node know every balance. | **REJECTED** | A root verifies supplied openings; it cannot enumerate committed data. |
| A valid state witness proves a valid transaction. | **REJECTED** | Authorization and deterministic execution must also be checked, or separately validity-proven. |
| PoH prevents forged forks and proves exact wall time. | **REJECTED** | It supplies a scoped sequence/delay claim; consensus and external assumptions remain. |
| Recursive proof means history and state can be deleted. | **REJECTED** | It compresses checking of one relation, not the data needed for recovery and queries. |
| Ancient proof resurrects an unspent balance. | **REJECTED** | It proves an old claim only; current applicability needs a dedicated expiry protocol. |
| Verkle always means KZG and trusted setup. | **REJECTED** | Verkle is parameterized by a vector commitment; profiles have different setup assumptions. |
| One fixed proof-size or speed number describes Verkle or a zkVM. | **REJECTED** | Bind every measurement to workload, scheme, version, encoding, hardware, and implementation. |
| Portal guarantees fixed replicas, latency, and immortal data. | **REJECTED** | Availability requires a Bitblock-specific protocol, incentives, measurements, and fault model. |
| PoW or a VDF proves monetary fairness. | **REJECTED** | It can support work/delay/ordering claims; fairness is a bundle of explicit economic properties. |

## Candidate capability notes, not applied to the tree

The raw conversation suggests three potentially useful future nodes. They are recorded here only;
they do not yet exist in [`CAPABILITY_TREE.md`](CAPABILITY_TREE.md).

### `WITNESSED-TRANSITION-R1` candidate

- **Purpose:** test complete stateless re-execution of one bounded transition batch.
- **Would depend on:** `ACCOUNT-1`, `TRANSITION-SPEC-1`, a selected `COMMIT-1` profile, `CODEC-1`,
  `BOUNDS-1`, independent `EXECUTOR-1`, `EXECUTION-ROOT-1`, `PROTOCOL-ID-1`, and the applicable
  synchronization/availability contract.
- **Gate idea:** every required pre-state read is authenticated; missing, duplicate, or
  non-canonical material rejects; independent execution reproduces receipts and execution root;
  one-over-limit, malformed, withheld, wrong-profile, and partial-domain witnesses fail closed.
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
9. Which bootstrap contract is supported: genesis replay, verified snapshot, recursive proof, or a
   precisely scoped combination?
10. What concrete latency, bandwidth, or verifier-cost requirement would justify recursive proving?
11. What property unavailable from height and consensus would justify a VDF or PoH-like clock?
12. Is state expiry required at all? If so, how are current applicability and no-double-revival
    proven?
13. Which economic properties make “fair mint and sink” a falsifiable claim?

Until those questions have artifacts and gates, the concise model is:

> Under a future admitted profile, a finalized Bitblock tick could commit the ordered transition
> body, witness bytes, receipts, complete post-execution root, history link, and payload manifest.
> When those bytes are available and validate against their commitments, a stateless verifier could
> check execution without retaining full state. Full-state producers and archive providers would
> still need explicit retention duties; consensus alone would select canonical ticks. Optional
> Verkle, sequential-clock, and recursive-proof lanes must each justify their own cost and security
> surface.

## Source queue with scoped authority

Statuses and mutable-page descriptions below were retrieved on 2026-08-27. They must be refreshed
and pinned to immutable artifacts before supporting a protocol decision.

| Source | Scope here |
| --- | --- |
| [Ethereum MPT documentation](https://ethereum.org/developers/docs/data-structures-and-encoding/patricia-merkle-trie/) | Existing Ethereum trie mechanics and encoding, not client-I/O benchmarks. |
| [Kuszmaul, *Verkle Trees*](https://math.mit.edu/research/highschool/primes/materials/2018/Kuszmaul.pdf) | Original construction and asymptotic argument, not a Bitblock profile. |
| [Ethereum Foundation 2021 Verkle note](https://blog.ethereum.org/2021/12/02/verkle-tree-structure) | One Pedersen/IPA research design, not current deployment. |
| [Ethereum Verkle overview](https://ethereum.org/roadmap/verkle-trees) | First-party explanatory witness model and estimates, not a benchmark or specification. |
| [EIP-6800](https://eips.ethereum.org/EIPS/eip-6800) and [EIP-7612](https://eips.ethereum.org/EIPS/eip-7612) | Stagnant Verkle state/overlay proposals, not current Ethereum selection. |
| [EIP-7864](https://eips.ethereum.org/EIPS/eip-7864) | Draft competing unified binary-tree proposal; its hash choice remains open. |
| [KZG paper](https://www.iacr.org/archive/asiacrypt2010/6477178/6477178.pdf) and [Ethereum KZG ceremony](https://ceremony.ethereum.org/) | KZG assumptions and the EIP-4844 ceremony; not the Pedersen/IPA Verkle setup. |
| [Solana whitepaper](https://solana.com/solana-whitepaper.pdf) and [Tower BFT note](https://solana.com/uk/news/tower-bft--solana-s-high-performance-implementation-of-pbft) | PoH sequence design and its separation from consensus. |
| [Boneh et al., *Verifiable Delay Functions*](https://eprint.iacr.org/2018/601.pdf) | VDF definition and assumptions, not a consensus or fairness result. |
| [EIP-4444](https://eips.ethereum.org/EIPS/eip-4444) | Stagnant history-expiry proposal, not state expiry. |
| [Portal specifications](https://github.com/ethereum/portal-network-specs) | Retrieval-protocol reference, not a Bitblock availability guarantee. |
| [Halo2 recursion notes](https://zcash.github.io/halo2/background/recursion.html) and [Nova paper](https://eprint.iacr.org/2021/370.pdf) | Distinct recursive-proof research inputs, not selections or benchmarks. |

Every later adoption, performance, security, or deployment claim needs a dated primary source or a
Bitblock-owned measurement with exact workload, artifact, version, hardware, assumptions, and loss
condition. This queue is not evidence that Bitblock selected any listed technology.

## Compression receipt boundary

This note preserves the raw source fingerprint, question spine, useful architecture, corrections,
candidate lanes, and source queue. It intentionally does not preserve every phrase or false claim;
Git history will retain later revisions of this note, while the original downloaded source remains
an external byte-level provenance artifact on this machine. The checker validates this file's
Markdown structure and relative planning links, not external URLs or the completeness or truth of
its technical contents.
