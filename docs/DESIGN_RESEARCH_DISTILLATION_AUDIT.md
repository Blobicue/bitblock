# First bounded audit of the design-research distillation

**One read-only gap sweep followed by one coordinated fix sweep — 2026-08-27**

This audit freezes exact clean input
`694650f4028f5c1fb715384597326e0b0e31f0e1`, Git tree
`8c094221fd7004fc2ec323a8b4e13249d4cfae50`. At that input:

```text
distillation_path        docs/DESIGN_RESEARCH_DISTILLATION.md
distillation_git_blob    97f2d855113479435030d55f02459f01ee9914a0
distillation_sha256      fa408ef8532a8b144c183e336b5746c205ec8598962daf2005474bc52c725191
external_source_path     C:\Users\Owen\Downloads\design.txt
external_source_sha256   eeda72adcf153f027c38ac8be6669f668c33e4b11d03b19d34fef2f9f228352f
```

Three independent read-only lenses examined protocol/type closure, cryptographic/source accuracy,
and provenance/handoff/checker behavior. A fourth local pass reconciled their overlaps against
`CAPABILITY_TREE.md`, `AUTHENTICATED_STATE_PLAN.md`, and `LAYER1_DESIGN_PAPER.md`. No audited file
was edited until the 15 findings below were consolidated and discovery was closed.

The lens reports were transient agent outputs. This file retains their finite findings, break
witnesses, correction map, no-gap areas, exact input, and no-edit boundary; it does not claim to
preserve verbatim agent transcripts or independently authenticate agent identity. The audit record
is committed first. The one fix sweep is a later descendant commit that changes the audited
documents and pins this file's first-addition Git blob. Neither commit can contain its own final
identifier.

## Evidence boundary

The sweep read:

- `docs/DESIGN_RESEARCH_DISTILLATION.md` and the external raw source receipt;
- `docs/CAPABILITY_TREE.md`, `docs/AUTHENTICATED_STATE_PLAN.md`, and the proposed identifier graph
  in `docs/LAYER1_DESIGN_PAPER.md`;
- `README.md`, `docs/HANDOFF.md`, and `docs/TRANSCRIPT_AUDIT.md`; and
- `tools/check_planning_docs.py` plus its powered controls.

It ran only read-only Git, filesystem, document, hash, and primary-source checks. It did not run
Rust, Lean, network, storage-fault, cryptographic implementation, performance, or physical-display
suites. “Patched” below will mean that prose or tooling names the missing type, dependency, check,
or non-claim. It will not mean that Bitblock implements a network, proof system, authenticated
execution root, cryptocurrency, consensus, or Layer 1.

## Frozen finding set and one-sweep correction map

| ID | Priority | Gap at `694650f` | One-sweep correction |
| --- | --- | --- | --- |
| DR-01 | P0 | Lines 119–159 introduced `TickCoreV1`, `TickEnvelopeV1`, `tick_id_n`, and a history formula that conflict with the existing proposed `ProposalHeader`, `block_id`, `post_history_root`, availability payload, and accepted-record graph in the Layer 1 paper. Two agents could implement incompatible byte contracts while following the repository. | Delete the competing `V1` wire sketch. Keep only an abstract typed relation and make the Layer 1 paper the sole proposed byte-contract owner; it remains unimplemented and non-authoritative. |
| DR-02 | P0 | Lines 94–139 had no exact admitted profile/domain-registry closure. One `P_v` could not identify the required domain set, inactive sentinels, codecs, limits, and per-domain commitment profiles composing `X_n`. | Add separate admitted profile, domain-registry, transition-specification, and per-domain commitment parameters. Compose `X_n` from the exact typed domain-root map projected from one admitted profile. |
| DR-03 | P0 | Lines 145–156 allowed generic consensus/finality and availability evidence without binding the selected policy, authority/validator context, epoch, or status predicate. Identical tick bytes could receive incompatible interpretations. | Defer concrete fields to the existing paper, require the candidate context to bind parent-authorized execution/consensus/availability policies and authority context, and describe evidence as typed, policy-bound predicates. Use profile-declared status rather than universal finality. |
| DR-04 | P0 | Lines 110–184 reused a touched-state witness across a commitment-profile change. Untouched state has no new-profile commitment, so a proposer could substitute it while preserving every touched opening. | Require profile equality on ordinary witnessed transitions. Route a profile change through a separately authorized migration transition with old/new roots, complete bounded correspondence evidence, crash/restart semantics, and historical-verifier retention. |
| DR-05 | P1 | Lines 100–176 verified typed map openings directly under composed execution root `X_(n-1)`. A world proof is valid under `WorldRootV1`, not under the composer over world, authorization, nonce, governance, and optional domains. | Define a typed execution-witness bundle: first verify the exact required domain-root set and recompute the execution root, then verify every map opening under its typed domain root and profile. Inactive or cross-domain proofs reject. |
| DR-06 | P1 | Lines 175–182 and 403–405 omitted batch-overlay and blind-write rules. A read after an in-batch write must not demand a contradictory pre-root opening, while every first write still needs old membership/absence evidence for a sound root update. | Freeze first-touch semantics: every first pre-state-dependent access and write target opens against the batch pre-root; later reads use a deterministic overlay; final update evidence covers every written key. Keep chained per-action roots as an explicit alternative, not an implicit mixture. |
| DR-07 | P1 | `A_n` included deterministic external inputs while the sketch separately committed `actions_root` and `deterministic_inputs_root`; `Step` received only `A_n`. Implementations could double-commit or omit the same input. | Split ordered actions `A_n` from deterministic auxiliary/system inputs `D_n`, use disjoint commitments, and pass both to the transition relation. Binding `D_n` proves reproducibility, not oracle truth or unbiased acquisition. |
| DR-08 | P1 | `E_n` modeled effects and receipts, but the sketch committed only `receipts_root` and did not require each receipt to bind its exact ordered effects. A payload could carry unrelated effect bytes. | Reuse the paper's existing `effects_root_j -> ReceiptCore_j -> receipts_root -> AvailabilityPayload` closure and require independent recomputation. Do not restate a second codec. |
| DR-09 | P1 | Line 72 excluded only known inter-generator discrete logs. A known wider linear relation among a Pedersen basis can also break position binding. | Require a position-binding vector commitment with sound profile-defined openings/multiproofs and a generator-basis argument excluding efficient discovery of any nonzero linear relation under named derivation/group assumptions. Retain subgroup, encoding, transcript, implementation, and side-channel risks. |
| DR-10 | P2 | The source queue called Kuszmaul's paper the original construction without saying its Catalano–Fiore instantiation does not establish Pedersen/IPA or KZG setup/security claims. | Scope that paper to its construction and add the primary IPA source. A source for one vector commitment does not transfer security or performance to another profile. |
| DR-11 | P1 | Lines 369–371 said proof of work establishes expended work. A target-satisfying nonce can occur on the first attempt; verification supports an expected-cost model, not measured hashes, energy, or expenditure. | State the expected-production-cost claim under the hash/difficulty model and explicitly reject measured-work or energy inference. |
| DR-12 | P1 | Mutable EIP and Portal status/specification claims had a retrieval date but no immutable source revision. The future page could change while the historical assertion stayed the same. | Pin the EIPs and Portal repository commits used by this sweep, link mutable status claims to immutable blobs, and distinguish construction, setup, implementation, and measurement evidence. A pin proves only what a source said at that revision. |
| DR-13 | P1 | `HANDOFF.md` called the pre-distillation future-plan audit the “latest” audit and could make its 16-finding closure or no-suite boundary look like review evidence for the later 491-line distillation. | Pin `694650f` as the distillation checkpoint, insert this successor audit into read order, and state exactly which checks and omitted suites belong to which checkpoint. |
| DR-14 | P1 | The only external raw-source receipt was prose inside a mutable Markdown file. A 64-zero digest still passed every checker control; no canonical method described byte, character, word, or line counts. | Add a canonical, immutable receipt manifest for the external input and audited distillation blob, compare the Markdown receipt with it, and power a receipt-mutation negative control. The raw bytes remain external and unrecoverable from the hash alone. |
| DR-15 | P1 | Relative-link checking discarded fragments. Replacing `#world-roots-are-not-history-roots` with a nonexistent heading still passed. | Resolve local fragments against a deterministic Markdown heading-slug map and add a wrong-fragment negative control. This validates navigation, not semantic agreement. |

## Source receipts used by the audit

Mutable status observations were checked on 2026-08-27 against `ethereum/EIPs` commit
`e400af2874d6728bfd369f1851c24f250c8825f7`:

| File | Observed status | UTF-8 text SHA-256 |
| --- | --- | --- |
| `EIPS/eip-6800.md` | Stagnant | `41dc40c62e4e2034d95a5762d0ed40339356bba4757efb123ff124279767aab9` |
| `EIPS/eip-7612.md` | Stagnant | `8afb0c5802da3a1b5d2d3474804aa9b0ac2ea7c0834d9255a0f8238b4735384d` |
| `EIPS/eip-7864.md` | Draft | `5c94205af77e617525595865ef62aa35c903b917fc0a0321fe2c148cc2439e5d` |
| `EIPS/eip-4444.md` | Stagnant | `a6fd896b9cec7f9b30efb2a6bb664158f7f89e768defd8d8343b0979f641c0e3` |

The Portal specification reference resolved to repository commit
`1ac0152620fb6426dd2500f0d43459f49d4dc25d`. These source receipts preserve what was inspected;
they do not elevate a proposal, webpage, repository, or paper into Bitblock authority.

## Areas checked with no additional material gap

- The raw source still matched its recorded byte count, decoded-character count, whitespace-word
  count, line count, modification time, and SHA-256 at the frozen input. The source still lacks a
  trustworthy machine-readable speaker boundary and remains external to Git.
- State-root/history-root/snapshot separation, information-retention limits, bootstrap choices,
  node-role separation, old-proof/current-state separation, and the rejection of fixed whole-witness
  sizes remained coherent.
- Verkle remained a separate feasibility lane rather than a third arm silently inserted into the
  hash-tree assay. KZG, Pedersen/IPA, and transparent/setup assumptions were not treated as
  interchangeable apart from DR-09 and DR-10's narrower generator/source corrections.
- Recursive validity remained separate from consensus and availability. The Circom-shaped passthrough
  example remained correctly rejected, and no proving framework was selected.
- The three candidate labels remained notes rather than capability-tree nodes. No Rust simulation,
  authenticated map, vector commitment, proof system, network, asset, consensus, or Layer 1 feature
  was implemented or claimed.

## One-fix-sweep rule

The correction pass may change only the files needed to close DR-01 through DR-15, connect this audit
to handoff/index surfaces, and power its controls. It may not discover and silently add a sixteenth
finding, activate a candidate capability, change the runtime, or rewrite either frozen future-plan
audit. A newly noticed issue after this freeze remains a residual for another explicitly pinned
loop.

## Validation contract

The correction is admitted only if these commands exit zero:

```powershell
python -B tools/check_planning_docs.py --self-test
git diff --cached --check
```

After the correction commit exists, stable historical whitespace checks are:

```powershell
git diff --check 694650f4028f5c1fb715384597326e0b0e31f0e1..HEAD
git show --check --format=fuller HEAD
```

Rust and Lean suites remain unrun because this loop changes research notes, provenance data,
handoff surfaces, and their checker only. That boundary is not runtime or proof validation.

## Residual boundary

This loop can close the 15 listed contradictions. It cannot prove that no other gap exists, that
the source paraphrases are complete, that a cited construction is secure, that an eventual
implementation refines the design, or that any distributed security property holds. The capability
frontier remains unchanged. A later loop must pin a new input and add a successor audit rather than
mutate this record.
