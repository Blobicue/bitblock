# SYNC-1@replay-authority: independent historical certificate reconstruction receipt v1

This receipt resolves `PS-095:SYNC-1@replay-authority=BOUNDED` (`PD-06`), completing both phases of
`SYNC-1` (`PS-034:SYNC-1@base=BOUNDED` and `PS-095:SYNC-1@replay-authority=BOUNDED`) while strictly
preserving the compile-time non-authority invariant that replay reconstruction audits already-accepted
`CONSENSUS-1` authority and never selects live consensus authority.

## Provenance

- **Merged `origin/main` subject (PR #82):** commit `96f2118340d2f25d713b2176b563f8817d48cd5d`
  (tree `3d6853fad1a3416cf0ca7fe99fc9805cfd1495bf`, parent `f9f4ec62eb4dfda9916cc7228c68f000f83ff8ec`,
  CI runs `45162574016` & `45162582728`, 10/10 jobs green across Linux, Windows, Python vectors,
  generated index, and Lean 4 `--trust=0`).
- **Executable module:** [`crates/research/src/sync_replay_authority_1.rs`](../crates/research/src/sync_replay_authority_1.rs)
  (`wasm32`-clean dedicated acyclic module strictly behind `--features protocol-research`,
  preserving strict Rust DAG acyclicity `sync_1 -> consensus_1 -> sync_replay_authority_1` with zero
  edits to `sync_1.rs`, `crates/core/**`, `commit_state.rs`, `genesis_d1.rs`, `storage.rs`,
  `journal.rs`, default binary, `crates/wasm/**`, `Cargo.lock` (`212` packages), or Lean 4 proofs).
- **Clean-room pure-Python verifier:**
  [`tools/sync_replay_authority_1_vectors.py`](../tools/sync_replay_authority_1_vectors.py)
  (`172/172` protocol schemas independently vectored).
- **CI & mutation evidence:**
  [`tests/wave23_sync_replay_authority_1.rs`](../tests/wave23_sync_replay_authority_1.rs) and
  `python3 tools/bbmutate.py sync_replay_authority_1` (`61/61` canonical big-endian sites KILLED,
  100%).
- **Release probe:**
  [`examples/sync_replay_authority_1_probe.rs`](../examples/sync_replay_authority_1_probe.rs).

## Verified golden tags & normative invariants

Both Rust (`ReplayEvidenceEventRecordV1`, `NormalizedAcceptedAuthorityEntryCoreV1`,
`ReplayEvidenceArchiveCheckpointV1`, `ReplayEvidenceViewV1`,
`SameWrapperAcceptedRecordOccurrenceWitnessV1`, `ReplayEvidenceRetentionProjectionProofV1`,
`ConsensusToReplayParentAuthorityCommitmentProofV1`,
`HistoricalAcceptedParentAuthorityUseWitnessV1`, `ReplayEvidenceCheckpointConsistencyProofV1`,
`ReplayParentAuthorityStatusProjectionV1`, `ReplayAuthorityAuditBundleV1`) and clean-room pure
Python (`tools/sync_replay_authority_1_vectors.py`) match bit-for-bit across all 3 golden artifacts:

- **Authorized post-consumer `ReplayEvidenceArchiveCheckpointV1` `C1`
  (`bitblock/sync-1/replay-archive-checkpoint/v1`, `363` bytes):**
  `#bb.receipt.sha256.2mdit6c2t47g4aoihd6qnz7r4iqy7qnyxq5zxl7lnbkvxh7jofna` (sealed at ledger height
  `5 > consumer_ledger_height 1`, signed by pre-existing checkpoint authority under
  `SigningDomainTagV1::ReleaseArtifact`, `publication_complete = true`, verifying carried policy and
  root digests as byte-equal assertions rather than self-selectors).
- **Post-consumer `ReplayAuthorityAuditBundleV1` at `C1`
  (`bitblock/sync-1/replay-authority-audit/v1`):**
  `#bb.receipt.sha256.k3g23z6c5xob4cyxdictjgeaqhckkvjqfdzahfjqcko6faxfsf5a` (`selects_live_consensus_authority() == false`,
  passes the required post-consumer checkpoint positive control proving archive order is not live
  authority).
- **Append-consistent `ReplayAuthorityAuditBundleV1` at `C2` (ledger height `8`, extending `C1`):**
  `#bb.receipt.sha256.y22xa32n3vfzindqhtg6sltlso5junn2lg2nzeon7orvqubznooa` (carries verified
  `ReplayEvidenceCheckpointConsistencyProofV1` proving monotonic prefix extension `C1 -> C2`).
- **Complete 12/12 `CAPABILITY_TREE.md` negative-control matrix
  (`tests/wave23_sync_replay_authority_1.rs`):** rejects `NC1` omitted pre-cutoff conflict, `NC2`
  post-cutoff insertion, `NC3` event reorder/gap, `NC4` event duplication, `NC5` mixed
  root/policy/view, `NC6` circular or substituted normalized-entry commitment, `NC7` retained-subset
  masquerade as full normalized entry core, `NC8` synthetic cross-wrapper pair, `NC9` missing
  purpose/consumer binding, `NC10` non-admitted/tentative/late certificate, `NC11`
  certificate-to-replay evidence mismatch, and `NC12` every partial checkpoint-publication prefix
  (both incomplete flag and truncated event prefix), plus unauthorized checkpoint signers.
- **Merged-head release timings (`96f2118`):** `ReplayEvidenceViewV1` prefix completeness +
  normalized view root `34.75 us`, `C1 -> C2` checkpoint authorization & append-consistency
  `79.88 us`, full post-consumer replay-authority audit `361.29 us`.
- **Visual verification:** `sync_replay_authority_1_audit_chart.png` rendered on merged `96f2118` was
  directly inspected via `read_file`.

## Non-claims

`BOUNDED` is mandatory: data retrieval does not prove correct execution, canonical ordering,
finality, or certificate admission. Replay reconstruction audits already-accepted `CONSENSUS-1`
authority and never selects live consensus authority.
