# Audit of the Bitblock Layer 1 design paper

**Audit checkpoint — 2026-08-26**

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

The audit used source/document inspection and consistency checks. It checked the current
branch status, searched the implementation and protocol docs for state/identity/consensus/
availability boundaries, reviewed the paper's headings and fenced blocks, and ran
`git diff --check` after the paper edits. No Rust, Lean, network, or physical-display suite
was run by this documentation audit.

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
| F-12 | Release and dependency substitution were not part of the security model. | A proof about reviewed source does not cover a different compiler, dependency, or binary. | Paper §9 and Gate G14 | Patched in paper |
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

## Security surfaces now covered by the paper

After these patches, the paper explicitly names the following surfaces:

1. world/application semantics and immutable genesis;
2. active token accounting and coordinate-episode identity;
3. signatures, key lifecycle, authorization, delegation, and Sybil assumptions;
4. canonical transactions, nonces, validity windows, resource limits, and lifecycle;
5. ordering, conflicts, provisional state, forks, receipts, and finality;
6. application, consensus, validator, history, parameter, and availability roots;
7. membership/non-membership, execution, finality, and availability proof types;
8. validator assumptions, quorum, fork choice, set changes, censorship, and incentives;
9. peer discovery, synchronization, snapshots, archival data, and crash-safe publication;
10. deterministic execution, randomness boundaries, formal refinement, and reproducible builds;
11. fees, balances, storage rent, state growth, supply, and asset-domain separation;
12. future programmability, privacy, bridge/oracle, governance, and emergency boundaries;
13. actor/validator authority separation, batch atomicity, and explicit toggle preconditions;
14. staged implementation checkpoints and evidence-specific verification gates.

## Repository obligations that remain open

The patches intentionally do not pretend to implement the following:

- Rust-to-Lean refinement for the chunked world;
- independent parity for every canonical byte domain and future key codec;
- durable journal receipts, power-loss evidence, external custody, and rollback detection;
- an independently retained history head and explicit latest-branch authority;
- durable coordinate-episode/asset evidence and safe pruning;
- independent sequentiality/VDF analysis and external anchors before any elapsed-time or
  Proof-of-History claim;
- executable edit semantics and refinement for the current LadderLang bridge;
- signed actions, ownership, transfers, account/UTXO semantics, and a chosen fee policy;
- a network transport, synchronization protocol, validator messages, and finality proof;
- an authenticated sparse state map with membership and non-membership witnesses;
- a selected consensus mechanism and adversarial multi-node evidence;
- explicit transaction lifecycle, proof-witness, and finalized-root client behavior;
- the user's ternary-hash implementation, contract, vectors, and security analysis;
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

The phrase “Layer 1 candidate” remains a future milestone, not a current status. The current
branch remains a deterministic local Bitblock world with bounded provenance/proof surfaces.
