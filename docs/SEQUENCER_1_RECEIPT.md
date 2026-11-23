# SEQUENCER-1: accountable centralized multiplayer engine receipt v1

This receipt resolves `PS-033:SEQUENCER-1@base=BOUNDED` and unlocks
**`PS-034:SYNC-1@base=READY`** (while keeping `PS-095:SYNC-1@replay-authority=LOCKED`,
`CONSENSUS-1`, `CLIENT-1`, `VALIDATOR-ECON-1`, `PROFILE-1`, `SIGNER-OPS-1`, `UPGRADE-1`, and
`CHAIN-LAUNCH-1` honestly `LOCKED`).

## Provenance

- **Merged `origin/main` subject (PR #76):** commit `54464675750c06302a987df6467844c68a632076`
  (tree `91b3957359a9f1f44debb13182efeab98ef057bd`, parent `0435314d676f7ba75873f9a26002da29191823c6`,
  CI runs `45146261970` & `45146272686`, 10/10 jobs green across Linux & Windows).
- **Executable module:** [`crates/research/src/sequencer_1.rs`](../crates/research/src/sequencer_1.rs)
  (`wasm32`-clean portable core + native `STORAGE-1` filesystem `Journal` and `std::net` loopback
  TCP adapters strictly behind `--features protocol-research`; zero new Cargo dependencies with
  `Cargo.lock` frozen at `212` packages; zero edits to `crates/core/**`, `storage.rs`, `journal.rs`,
  `commit_state.rs`, `genesis_d1.rs`, default binary, `crates/wasm/**`, or Lean 4 proofs).
- **Clean-room pure-Python verifier:** [`tools/sequencer_1_vectors.py`](../tools/sequencer_1_vectors.py)
  (`166/166` protocol schemas independently vectored).
- **CI & mutation evidence:** [`tests/wave20_sequencer_1.rs`](../tests/wave20_sequencer_1.rs) and
  `python3 tools/bbmutate.py sequencer_1` (`21/21` canonical big-endian sites KILLED, 100%).
- **Release probe:** [`examples/sequencer_1_replay_probe.rs`](../examples/sequencer_1_replay_probe.rs).

## Verified golden tags & normative invariants

Both Rust (`SequencerEngineV1`, `SequencerClientVerifierV1`, `SequencerLogV1`,
`DurableSequencerStoreV1`, `sequencer_1::net`) and clean-room pure Python
(`tools/sequencer_1_vectors.py`) agree bit-for-bit across all 4 golden artifacts:

- **Signed `SequencerBatchHeaderV1` (`bitblock/sequencer-1/signed-batch-header/v1`, `532` bytes):**
  `#bb.receipt.sha256.zq4zzmtlddpcvphk24t3koybltetooeb5qo3b6yvjhbncrgdsyza` (strict RFC 8032
  Ed25519 under `SigningDomainTagV1::BlockProposal`, bound to `PROTOCOL-ID-1`
  `CanonicalAcceptedProposalCoreV1`, `receipts_merkle_root`, and 11-slot `ExecutionRootRefV1`).
- **Atomic `SequencerBatchPublicationV1` (`bitblock/sequencer-1/batch-publication/v1`):**
  `#bb.receipt.sha256.htcbximoqnut4khf6do3lev5r4znvuvd2ppjgaeicgjiml254eia`.
- **Receipt Merkle inclusion proof (`ReceiptMerkleProofV1` for canonical outcome 0):** SHA-256
  `2a1fbc6c45cbe4b7d7e7428eeb4a28060f382f096c3efbc7dcba6b2f25f77f81`.
- **Bounded `BBSEQ1` wire frame (`SequencerWireFrameV1`):** SHA-256
  `7d73767e55d0747d42b7d92a742b8c67316964567e125290e65c5330242048c0`.
- **Durable `STORAGE-1` body-before-root publication & torn-tail recovery:** writes and `sync_all`s
  the complete body+receipt `StorageRecord` strictly before appending the parent-linked signed
  header `StorageRecord` via frozen `journal::Journal`; simulated crash mid-write injects a
  corrupt suffix that recovers cleanly via `OpenJournal::NeedsRecovery` + `discard_unverified_tail`
  (`TailDiscardReceipt`) without losing committed prefix or publishing orphan headers.
- **Real `127.0.0.1` loopback TCP resynchronization & independent replay:** fresh client pulls
  multi-batch publications over `std::net` loopback TCP under pre-allocation `BOUNDS-1` framing,
  independently replays every batch through `MultiActorHarnessV1` / `PrimaryLaunchExecutorV1` +
  `EXECUTION-ROOT-1`, confirms exact `ExecutionRootRefV1` agreement, and verifies `O(log n)`
  `ReceiptMerkleProofV1` inclusion for every step outcome.
- **Complete 8/8 malicious-input & accountability gate matrix:** rejects `> 2 MiB` wire frames
  before allocation (`G1`), malformed/truncated/trailing wire frames (`G2`), invalid Ed25519
  sequencer signatures (`G3`), withheld-body header announcements (`G4`), tampered execution roots
  (`G5`), tampered receipt Merkle proofs (`G6`), same-height sequencer equivocation (`G7`), and
  broken `parent_header_digest` chain links (`G8`), plus duplicate-envelope deduplication.
- **Merged-head release timings (`5446467`, 50-run p50):** batch fold + sign + body-before-root log
  `708.53 us`, independent client full replay + root verification `657.63 us`, `O(log n)` receipt
  Merkle proof verification `1.09 us`.
- **Visual verification:** `sequencer_1_accountability_chart.png` rendered on merged `5446467` was
  directly inspected via `read_file`.

## Non-claims

`BOUNDED` is mandatory: the sequencer can still censor, reorder, halt, equivocate, lose data, or
present different histories. Accountability is not decentralization.
