# CONSENSUS-1@base: HotStuff 2-phase BFT, live parent-authority certificates & Lean 4 safety receipt v1

This receipt resolves `PS-056:CONSENSUS-1@base=BOUNDED` and unlocks
**`PS-095:SYNC-1@replay-authority=READY`** (`PD-06`), **`PS-057:CLIENT-1@base=READY`**,
**`PS-041:VALIDATOR-ECON-1@base=READY`**, and **`PS-085:SIGNER-OPS-1@base=READY`** while keeping
`PROFILE-1`, `RELEASE-1`, `UPGRADE-1`, `CHAIN-LAUNCH-1`, and `TESTNET-1` honestly `LOCKED`.

## Provenance

- **Merged `origin/main` subject (PR #80):** commit `4ccbe564e218d21b93f55727233284b1a62569e4`
  (tree `f7faf329d7efa59f3b0be1f69d147f0588aec942`, parent `2a6ea196d9c405e3e985611ec850c9b53b5d5f24`,
  CI runs `45157924386` & `45157933523`, 10/10 jobs green across Linux, Windows, Python vectors,
  generated index, and Lean 4 `--trust=0`).
- **Machine-checked Lean 4 safety proof:**
  [`proof/BitblockProof/ConsensusSafety.lean`](../proof/BitblockProof/ConsensusSafety.lean)
  (`26` new `--trust=0` theorems bringing the proof capsule from `172` to `198` theorems across `9`
  modules, `0` warnings, `0` `sorry`/`admit`/`axiom`/`unsafe`/`native_decide`, axiom footprint
  `[propext, Quot.sound]` only, zero edits to the 8 existing Lean 4 proof modules).
- **Executable module:** [`crates/research/src/consensus_1.rs`](../crates/research/src/consensus_1.rs)
  (`wasm32`-clean portable BFT & live certificate core + native `std::net` loopback TCP 4/5-validator
  adapter `consensus_net` strictly behind `--features protocol-research`; zero new Cargo
  dependencies with `Cargo.lock` frozen at `212` packages; zero edits to `crates/core/**`,
  `commit_state.rs`, `genesis_d1.rs`, `storage.rs`, `journal.rs`, `sync_1.rs`, default binary, or
  `crates/wasm/**`).
- **Clean-room pure-Python verifier:** [`tools/consensus_1_vectors.py`](../tools/consensus_1_vectors.py)
  (`170/170` protocol schemas independently vectored).
- **CI & mutation evidence:** [`tests/wave22_consensus_1.rs`](../tests/wave22_consensus_1.rs) and
  `python3 tools/bbmutate.py consensus_1` (`52/52` canonical big-endian sites KILLED, 100%).
- **Release probe:** [`examples/consensus_1_bft_probe.rs`](../examples/consensus_1_bft_probe.rs).

## Verified golden tags & normative invariants

Both Rust (`HotStuffValidatorNodeV1`, `ConsensusClusterHarnessV1`, `QuorumCertificateV1`,
`ConsensusCommitCertificateV1`, `AcceptedConsensusNativeParentAuthorityCertificateV1`,
`LiveParentAuthorityRegistryV1`, `consensus_net`) and clean-room pure Python
(`tools/consensus_1_vectors.py`) match bit-for-bit across both new protocol schemas:

- **Finalized `ConsensusCommitCertificateV1` (`bitblock/consensus-1/bft-commit-certificate/v1`):**
  `#bb.receipt.sha256.hubj5ifmhrm4xxyu4gw5yzexokxryskow6yvge6uflam4xvyawna` (height `1`, epoch `0`,
  view `0`, designated leader `[2; 32]` signed with `proposal_key` under
  `SigningDomainTagV1::BlockProposal`, 4-validator `PrepareQC` + `CommitQC` signed with `vote_key`
  under `SigningDomainTagV1::ConsensusVote`, strict `3 * signed_weight > 2 * total_weight`, full
  data-before-vote execution bound to 11-slot `ExecutionRootRefV1`).
- **Live `AcceptedConsensusNativeParentAuthorityCertificateV1`
  (`bitblock/consensus-1/parent-authority-certificate/v1`):**
  `#bb.receipt.sha256.ovdkxdeii3nlsw6xmpidj6xkqun3mmmmnir36mwqikie3levp4rq` (certificate-independent
  `ReleaseAuthority` consumer context, exact three-anchor
  `VerifiedConsensusNativeAcceptedRecordOccurrenceHistoryStartEqualityV1`, split-wrapper
  arrival-permutation-invariant occurrence receipts, lossless predecessor-closure lower bounds,
  `from_replay_archive = false`, and strict `> 2/3` issuing validator quorum).
- **Lean 4 `--trust=0` formal safety theorems (`ConsensusSafety.lean`):** proves weighted strict
  `> 2/3` quorum intersection under `3 * F < W` (`quorum_intersection_has_honest_validator`),
  data-before-vote extraction, same-view QC uniqueness (`qc_unique_same_view_phase`), cross-view
  2-phase HotStuff lock persistence by strong induction (`no_prepare_qc_conflicting_after_commit`),
  full finality safety across arbitrary views (`hotstuff_two_phase_finality_safety`), fail-closed
  unique live certificate admission (`admitLiveCert_sound_and_preserves_unique`), and powered
  negative controls proving tightness of strict `> 2/3` and `< 1/3` bounds.
- **Complete 12/12 adversarial & certificate control matrix (`tests/wave22_consensus_1.rs`):**
  loopback TCP 4-validator & `4 -> 5` epoch validator set-change roundtrips (`VALIDATOR-SCHEDULE-1`),
  non-leader / cross-role key reuse refusal, data-before-vote refusal on missing/tampered body,
  sub-quorum `2/4` rejection, slashable vote equivocation evidence, 3-of-4 partition + heal,
  HotStuff lock preservation, conflicting-finality automatic `ConsensusSafeHaltEvidenceV1`,
  split-wrapper arrival-permutation invariance, duplicate slot rejection, monotonic post-acceptance
  status upgrade (`AcceptedLive -> UpgradedFinalized`), history-start mismatch refusal,
  late-valid-suffix & candidate-advanced-rule rejections, and strict replay-archive refusal in live
  admission.
- **Merged-head release timings (`4ccbe56`):** 4-validator 2-phase finality round `9351.51 us`,
  `ConsensusCommitCertificateV1` verification `333.72 us`, live parent-authority certificate
  verification/admission `107.00 us`.
- **Visual verification:** `consensus_1_bft_safety_chart.png` rendered on merged `4ccbe56` was
  directly inspected via `read_file`.

## Non-claims

`BOUNDED` is mandatory: consensus evidence does not imply correct execution or available bodies
unless those checks are separately bound into the profile. In-process and loopback simulation plus
an abstract Lean 4 model do not prove live multi-host Byzantine liveness, independent multi-process
deployment safety, signer custody, or Sybil resistance (`SYNC-1@replay-authority`, `CLIENT-1`,
`SIGNER-OPS-1`, `VALIDATOR-ECON-1`, `PROFILE-1`, and `TESTNET-1` remain separate gates).
