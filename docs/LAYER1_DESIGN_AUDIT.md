# Audit of the Bitblock Layer 1 design paper

**Audit checkpoint 0.3 — second loop — 2026-08-26**

This is the audit trail for `docs/LAYER1_DESIGN_PAPER.md`. It records gaps found by
cross-checking the paper against the current Rust tree, Lean/LadderLang capsules,
`docs/SEMANTICS.md`, `docs/HANDOFF.md`, and the distributed-protocol obligations in
`docs/PROTOCOL.md`.

The audit distinguishes two different outcomes:

- **Patched in paper** means the design now names the boundary, required rule, or open
  decision. It does not mean the code implements it.
- **Still open in repository** means the paper correctly identifies work that remains before
  a corresponding security claim could be made.

## Method and evidence boundary

The second loop attacked paper/audit input
`dd19c5fcdbb318f091cd58aec469824a7b5df3e6` against the unchanged source/proof snapshot
`6545d8e7d60e75607445fda564bbf11610a62658`. The patched checkpoint-0.3 paper is
`034b7b2b5f129625f9c6dcec3e14038b7c4aee5f`. Those pins make this audit stale as soon as
the relevant source/proof files or paper change.

The shared working tree acquired intermediate commit
`018ca52f43309c8ca6b30831e28c815bc8a7f3f5` during the loop. That commit banked the
then-uncommitted paper edits; it is not a separate audit result. The result identified above
is the cumulative `dd19c5f..034b7b2` paper state and supersedes that intermediate snapshot.

Three independent read-only lenses checked protocol/security completeness, internal formal
composition, and repository truth. The inspected set included `src/lib.rs`, `src/main.rs`,
`src/history.rs`, `src/coordinate_episode.rs`, `src/tick.rs`, `src/interaction.rs`,
`src/replay_evidence.rs`, `proof/BitblockProof/MintBurn.lean`,
`tools/coordinate_episode_vectors.py`, `docs/SEMANTICS.md`, `docs/HANDOFF.md`, and the
hash-suite, replay-evidence, and open-obligation sections of `docs/PROTOCOL.md`.

After edits, the audit ran `git diff --check` and read-only PowerShell checks for balanced
Markdown fences, duplicate headings, trailing whitespace/tabs, and consistent pipe counts
inside contiguous tables. No Rust, Lean, network, storage-fault, cryptographic, or
physical-display suite was run by this documentation audit.

The authoritative current behavior remains in the Rust implementation and its explicit
tests. This audit does not promote prose into implementation evidence.

## Findings and patches

| ID | Gap found | Why it mattered | Patch location | Status |
| --- | --- | --- | --- | --- |
| F-01 | The state tuple covered application fields but omitted consensus, availability, governance, and candidate/finalized history. | A Layer 1 is a replicated protocol state machine, not only a world bitmap. | Paper §2 | Patched in paper |
| F-02 | The paper said the current executable had only the world plus local history. | The executable also has camera/presentation state and accepted-action/tick surfaces; the sentence could be read as a false implementation claim. | Paper §2 | Patched in paper |
| F-03 | Genesis was mentioned but not defined as a trust anchor. | Without chain identity, initial authority, parameters, and an immutable genesis package, replay domains and validator bootstrapping are ambiguous. | Paper §2 and §4.5 | Patched in paper |
| F-04 | Token uniqueness was described but no event-position derivation or cross-chain binding was given. | Coordinate reuse, replays, and alternate worlds could alias an asset identity. | Paper §2.1 and §3 | Patched in paper |
| F-05 | There was no transaction lifecycle, stale-parent behavior, or pre-finality reorganization rule. | A mempool action is not finalized ownership, and a fork can otherwise create phantom mint/burn state. | Paper §4.3 and §6.1 | Patched in paper |
| F-06 | The relationship between the 60 Hz local tick and consensus height was underspecified. | Wall-clock cadence cannot be treated as distributed finality or consensus time. | Paper §2 and §4 | Patched in paper |
| F-07 | Fault assumptions were named but not required to be quantitative. | “Honest majority” is not testable without a network model, quorum rule, and validator-set transition rule. | Paper §6 | Patched in paper |
| F-08 | Node synchronization, peer isolation, snapshot trust, and node roles were only implied. | Hashes do not make block bodies available or protect a new node from an eclipsed peer. | Paper §8 | Patched in paper |
| F-09 | State, execution, finality, and availability proofs were not separated by type. | A valid world digest is not a membership witness or a finality certificate. | Paper §5 | Patched in paper |
| F-10 | Future visual programs and randomness had no explicit authority boundary. | Executable blocks, random choices, and renderer inputs can become consensus inputs accidentally. | Paper §4.6 and §9 | Patched in paper |
| F-11 | Block-level resource limits, state growth, fee accounting, and asset-domain separation were thin. | Infinite-coordinate intent creates allocation, history, and spam risks even without monetary value. | Paper §4.1 and §10 | Patched in paper |
| F-12 | Release and dependency substitution were not part of the security model. | A proof about reviewed source does not cover a different compiler, dependency, or binary. | Paper §9 and Gate G16 | Patched in paper; gate pointer corrected after the pinned loop |
| F-13 | Finality, validator-set changes, censorship, MEV/order manipulation, and long-range attacks were not explicit enough. | A proposer can reorder or withhold without forging a user signature; validator changes can invalidate old assumptions. | Paper §6 and §7 | Patched in paper |
| F-14 | Key custody and Sybil identity limits were listed but not treated as an operational lifecycle. | Public-key control is not human uniqueness, and lost/compromised keys need protocol behavior. | Paper §3 and §7 | Patched in paper |
| F-15 | The paper did not explicitly carry forward the repository's current open obligations. | Future agents could mistake a complete architecture map for completed Layer 1 evidence. | Paper §14 | Patched in paper |
| F-16 | The first revision contained a duplicate state bullet and placed explanatory text inside the liveness code fence. | The paper itself had a small semantic/formatting defect that could obscure the protocol definition. | Paper §2 and §6 | Patched in paper |
| F-17 | Consensus state and local camera/presentation state were initially written as one transition tuple. | A renderer or camera update must not become an accidental consensus input. | Paper §2 and §9 | Patched in paper |
| F-18 | Token derivation initially used `finalized_height` even though execution precedes finality. | Provisional events need an included height; finality is a separate status. | Paper §2.1 | Patched in paper |
| F-19 | Transaction and root examples did not bind every cross-chain/world identity or expose an explicit transaction-root lane. | Cross-domain replay and light-client ambiguity need domain-separated identifiers. | Paper §3, §4.4, and §5 | Patched in paper |
| F-20 | The repository's sequential/elapsed-time and LadderLang-specific open obligations were only implicit in the residual list. | A future agent could mistake local serial hashing or a representation bridge for Proof of History or executable semantics. | Paper §14 and audit residuals | Patched in paper |
| F-21 | The paper named an asset root but did not place ownership/balance/fee state in the protocol-state model. | A root lane without a state component leaves the economic transition under-specified. | Paper §2 and §10 | Patched in paper |
| F-22 | Actor identity and validator identity were not explicitly separated. | A user signature must not silently become a consensus vote or validator reward right. | Paper §3 | Patched in paper |
| F-23 | `toggle` and batch failure atomicity lacked a distributed precondition/commit rule. | Race outcomes and partial writes can diverge nodes or create unbacked assets. | Paper §2.1, §4.1, and §4.2 | Patched in paper |
| F-24 | The paper did not define Layer 1 as a bounded claim about a replicated base state machine. | Without a term boundary, a game server, bridge, local hash, or market token could be mislabeled as an L1. | Paper security-claim boundary | Patched in paper |

## Second-loop findings and patches

| ID | Gap found | Why it mattered | Patch location | Status |
| --- | --- | --- | --- | --- |
| F-25 | Deterministic execution state included candidates, votes, availability/sync metadata, and other node-local facts. | Honest nodes may legitimately disagree on those facts, and user transactions cannot derive them. | Paper §2 | Patched in paper |
| F-26 | Genesis and actor identity examples could become self-referential. | A genesis authorization map cannot be keyed by an ID that requires the not-yet-derived genesis ID. | Paper §2 and §3 | Patched in paper |
| F-27 | The signed action contained its own signature, did not bind selectable suite IDs, and used ambiguous concatenation sketches. | This creates a circular signing definition plus malleability, downgrade, and cross-decoder risk. | Paper §3 | Patched in paper |
| F-28 | The block carried the certificate it appeared to certify, while its root omitted body, receipts, DA, proposer, version, and other fields. | A certificate must authenticate a non-circular identifier that binds every authority-relevant header field. | Paper §4.4 | Patched in paper |
| F-29 | Auxiliary inputs, block context, and the DA commitment were not tied to the same exact body/receipt/witness bytes. | Two byte carriers could share an identifier, or availability evidence could attest data unrelated to execution. | Paper §4.4 and §6.1 | Patched in paper |
| F-30 | The state-root sketch omitted episode, authorization, full asset, governance, and validator-schedule state. | Consensus behavior could depend on state that no authenticated root committed. | Paper §2 and §5 | Patched in paper |
| F-31 | Receipts lacked a committed ordered lane, inclusion path, exact effects, system-transition subjects, and distinct burn fields. | A client could not prove the finalized/confirmed result of a mint, burn, fee, reward, rent, or upgrade. | Paper §4.4 and §5 | Patched in paper |
| F-32 | Rejection, admitted failure, no-op, transaction atomicity, block invalidity, fee, and nonce effects contradicted one another. | “No mutation” was incompatible with failed fees/nonces, and partial domain writes could escape. | Paper §4.1 and §4.3 | Patched in paper |
| F-33 | The paper described a complete accepted-action history that the live reducer does not retain. | Non-edit controls are aggregated; misses, overlap rejections, camera-only actions, and idle ticks occupy different lanes. | Paper abstract, §1, §2, §4, and §14 | Patched in paper; repository gate open |
| F-34 | Active-supply equality did not prove coordinate/token/episode correspondence or the converse from live episode to active map. | Phantom live episodes, wrong-coordinate mappings, or consumed records with no burn provenance could pass cardinality. | Paper §2.1 | Patched in paper |
| F-35 | Mint/event identity was ambiguous across forks and did not bind auxiliary/system context or per-effect position. | Sibling candidates and multi-effect system transitions could alias event identifiers. | Paper §2.1 and §4.4 | Patched in paper; final stability choice open |
| F-36 | Same-height safety, absolute finality language, and generic `FinalityEvidence` conflated deterministic finality with probabilistic confirmation. | Normal work forks could be misclassified as safety faults or safe-halt events. | Paper §4.4, §6, §8, and gates | Patched in paper |
| F-37 | Votes/certificates did not bind phase/round/epoch or derive unique signer weight, keys, suites, and active set from the accepted parent. | A candidate could select its own authority, duplicate weight, replay votes, or downgrade credentials. | Paper §3, §4.4, and §6.1 | Patched in paper |
| F-38 | DA sampling could be read as a substitute for transition verification. | Availability confidence does not prove execution validity or the post-state root. | Paper §4.4 and §6.1 | Patched in paper |
| F-39 | Transaction inclusion liveness had no fee, capacity, dissemination, resubmission, expiry, or synchrony envelope. | A syntactically valid low-priority transaction can remain excluded without violating chain progress. | Paper §6 | Patched in paper |
| F-40 | The threat list did not define static/adaptive corruption, network power, compromise scope, economic budget, or time horizon. | Threat names without an adversary model do not produce falsifiable security claims. | Paper §7 | Patched in paper |
| F-41 | Key custody omitted entropy, key validation, role separation, aggregate-verification controls, signer restore, and TCB exclusions. | A sound primitive can still fail at generation, parsing, operation, failover, or hardware boundaries. | Paper §3, §7, and §8.3 | Patched in paper |
| F-42 | Upgrades lacked a deterministic migration function, old/new root binding, bounded replay, and invariant-preservation rule. | Serialization or binary drift could silently rewrite supply, bedrock, ownership, or authority. | Paper §4.5 and Gate G17 | Patched in paper |
| F-43 | Node roles conflated verification with voting, and pruning conflicted with mandatory full replay. | A pruned network cannot promise genesis replay without a separate archival guarantee. | Paper §8.1 | Patched in paper |
| F-44 | Production monitoring, privilege separation, anti-double-sign restore, incident handling, and safe halt existed only as an open label. | Operational faults can violate authority even when the state transition is correct. | Paper §8.2–§8.3 and Gate G18 | Patched in paper |
| F-45 | Governance reversibility and unspecified rent expiry contradicted irreversible burn history and atomic supply. | Governance must compensate rather than erase, and rent must state its exact token/world effect. | Paper §2.2 and §10 | Patched in paper |
| F-46 | Feature-gated coordinate evidence and the Lean capsule were described too broadly. | Research-only in-memory evidence and per-edit finite-support theorems are not default runtime assets or Rust refinement. | Paper §1, §9, §11, and gates | Patched in paper |
| F-47 | The ternary-hash section omitted existing negative evidence while treating the intended balanced-trit construction as merely unidentified. | The audited Aethor TSH/PDH collision/framing defects must remain disqualified without prejudging a different missing algorithm. | Paper §5 and §14 | Patched in paper |
| F-48 | The current full-world tag and local replay output were too easy to read as witnesses or durable run receipts. | A sorted digest has no sparse proof/decoder, and stdout or in-memory self-verification has no supplied-carrier custody. | Paper §5 and §14 | Patched in paper; repository gates open |
| F-49 | Replay used one external protocol version and omitted committed context, system effects, signatures, receipts, and consensus evidence. | A chain crossing upgrades cannot be independently validated from ordered user actions alone. | Paper §2, §4.4, and §9 | Patched in paper |
| F-50 | Verification gates omitted codec negatives, action completeness, receipt linkage, DA-before-vote, migrations, operations, and explicit failure conditions. | A topic list is not a coverage matrix, and a test without a loss condition can be vacuous. | Paper §12 | Patched in paper |
| F-51 | “Complete design coverage map” and “small state space” overstated a finite documentation audit and a huge coordinate state space. | Coverage completeness was unproved; only the semantic core is small. | Paper §14 and conclusion | Patched in paper |
| F-52 | The first audit said “current branch” without immutable source, input-paper, file-set, or result pins. | Later edits could silently stale its findings while preserving confident language. | This audit method section | Patched in audit |

## Design surfaces now made explicit

After both loops, the paper explicitly names the following design surfaces. “Named” means
the contract or open obligation is visible; it does not mean implementation evidence exists:

1. deterministic execution state, node-local consensus views, and non-self-referential genesis;
2. exact occupancy/token/episode bijection and fork/system-aware event identity;
3. canonical typed framing, non-circular signatures, suite binding, stable identity, and key lifecycle;
4. transaction admission, failure/no-op/fee/nonce dispositions, atomicity, lifecycle, and action/result completeness;
5. body/context/header/receipt/effect/availability commitments and an acyclic identifier graph;
6. application/execution/authorization/governance/validator roots plus typed proof and receipt-inclusion paths;
7. deterministic finality versus probabilistic confirmation, prefix safety, validator-set provenance, and vote domains;
8. execution validity versus data availability, eligible-inclusion liveness, ordering fairness, and censorship;
9. quantitative adversary assumptions, cryptographic TCB, credential roles, and anti-double-sign recovery;
10. full-verifier, consensus-validator, archive, light-client, and non-authority RPC/indexer roles;
11. peer discovery, synchronization, genesis/checkpoint/weak-subjectivity bootstrap, pruning, and crash-safe publication;
12. privilege separation, monitoring, safe halt, incident response, backup, failover, and release integrity;
13. deterministic upgrades/migrations, governance authority, crypto migration, and historical verification;
14. fees, balances, rewards, rent, state growth, supply, and asset-domain separation;
15. future programmability, randomness, privacy, and bridge/oracle boundaries;
16. staged implementation checkpoints and failure-specific verification gates.

## Repository obligations that remain open

The patches intentionally do not pretend to implement the following:

- Rust-to-Lean refinement for the chunked world;
- independent parity for every canonical byte domain and future key codec;
- durable journal receipts, power-loss evidence, external custody, and rollback detection;
- an independently retained history head and explicit latest-branch authority;
- one complete live action/result record spanning non-edit controls, edit attempts, rejections,
  camera-only actions, and idle ticks;
- promotion of feature-gated in-memory coordinate episodes into durable asset evidence and safe pruning;
- an authenticated sparse world map, durable decoder, and membership/non-membership witnesses;
- a durable supplied-carrier replay verifier and independent executor rather than stdout or local self-verification;
- independent sequentiality/VDF analysis and external anchors before any elapsed-time or
  Proof-of-History claim;
- executable edit semantics and refinement for the current LadderLang bridge;
- signed actions, ownership, transfers, account/UTXO semantics, and a chosen fee policy;
- a network transport, synchronization protocol, validator messages, profile-specific
  consensus evidence, and data-before-vote enforcement;
- a selected consensus mechanism and adversarial multi-node evidence;
- explicit transaction lifecycle, proof-witness, and finality/confirmation-status client behavior;
- the intended balanced-trit hash implementation, contract, vectors, and security analysis;
- deterministic upgrade/migration evidence and production privilege/monitoring/safe-halt drills;
- a deterministic VM/refinement proof if visual blocks become executable;
- physical display/scanout evidence, which remains outside Layer 1 state authority.

These are implementation and evidence obligations, not prose omissions. They should be
closed one at a time, with the exact assumption and evidence recorded before a public claim
is widened.

## Residual paper risks

The paper is still a proposal and has deliberate open choices. In particular, it does not
select a signature scheme, authenticated-map structure, consensus mechanism, asset model,
privacy design, or network synchrony assumption. Those choices cannot be safely inferred
from the current SHA-256 local history or from the existence of a multiplayer client.

The phrase “Layer 1 candidate” remains a future milestone, not a current status. At the
pinned source snapshot, the branch remains a deterministic local Bitblock world with bounded
provenance/proof surfaces. The three audit lenses and their refreshes found no remaining
material paper contradiction after the listed patches, but that finite result is not proof
that the inventory is complete.

The table's original F-12 row incorrectly named G14; G16 is the reproducible-release gate. The
post-checkpoint bedrock qualification and this pointer correction were found against planning input
`162406819133c1fd57d0fd26cbe5fa0b495766b8` and are mapped in
[`FUTURE_PLAN_AUDIT.md`](FUTURE_PLAN_AUDIT.md). This erratum does not change the historical claim
scope or pretend that checkpoint 0.3 received a third complete paper audit.
