# CLIENT-1: proof-bound provisional and accepted client state receipt v1

This receipt resolves `PS-057:CLIENT-1@base=BOUNDED`, completing the 3-node Consensus,
Replay-Authority & Client Finality Trunk (`CONSENSUS-1`, `SYNC-1@replay-authority`, and `CLIENT-1`)
while strictly enforcing that provisional `PrepareQC` branches never masquerade as finalized truth
and that pre-finality reorgs retain zero phantom ownership or supply.

## Provenance

- **Merged `origin/main` subject (PR #84):** commit `1b7f793f0c655d7421f7318ea3b0cd7225a77075`
  (tree `418832fc93f3c544d23a74e1e415f62f572080e4`, parent `4050481ef06df0948aee033b302ded42b30cd983`,
  CI runs `45166833606` & `45166844430`, 10/10 jobs green across Linux, Windows, Python vectors,
  generated index, and Lean 4 `--trust=0`).
- **Executable module:** [`crates/research/src/client_1.rs`](../crates/research/src/client_1.rs)
  (`wasm32`-clean dedicated module strictly behind `--features protocol-research`, with zero edits
  to `crates/core/**`, `commit_state.rs`, `genesis_d1.rs`, `storage.rs`, `journal.rs`, `sync_1.rs`,
  default binary, `crates/wasm/**`, `Cargo.lock` (`212` packages), or Lean 4 proofs).
- **Clean-room pure-Python verifier:**
  [`tools/client_1_vectors.py`](../tools/client_1_vectors.py) (`174/174` protocol schemas
  independently vectored).
- **CI & mutation evidence:**
  [`tests/wave24_client_1.rs`](../tests/wave24_client_1.rs) and
  `python3 tools/bbmutate.py client_1` (`20/20` canonical big-endian sites KILLED, 100%).
- **Release probe:**
  [`examples/client_1_lifecycle_probe.rs`](../examples/client_1_lifecycle_probe.rs).

## Verified golden tags & normative invariants

Both Rust (`ClientTxStatusV1`, `ClientRootStatusV1`, `ClientTrustAnchorV1`,
`ClientVerifiedStateViewV1`, `ClientTxLifecycleReceiptV1`, `ClientStateTrackerV1`) and clean-room
pure Python (`tools/client_1_vectors.py`) match bit-for-bit across both golden artifacts:

- **`ClientVerifiedStateViewV1` (`bitblock/client-1/verified-state-view/v1`, `450` bytes):**
  `#bb.receipt.sha256.532kser2hxodz65nosyv557zpcqdnlhly2orxsyvnmzu45xr5l5q` (binds `genesis_id`,
  `profile_id`, `finalized_root_status = FinalizedCommitQc`, finalized height/epoch/block/execution
  root/receipts root, `weak_subjectivity_checkpoint_epoch`, finalized liquid balances and locked
  deposits strictly isolated from provisional candidates, tracked/finalized/rejected/replaced tx
  counters, and `finalized_state_summary_digest`).
- **`ClientTxLifecycleReceiptV1` (`bitblock/client-1/tx-lifecycle-receipt/v1`, `392` bytes):**
  `#bb.receipt.sha256.vqjs76elssntzogtqd4ir6xtydoj7ctzmi7odas7pjq3562ayjvq` (binds `tx_digest`,
  `principal_actor_id`, `nonce`, 6-state `ClientTxStatusV1`, 4-state `ClientRootStatusV1`,
  `observed_height`, `observed_block_id`, `observed_execution_root_ref`, `replaced_by_tx_digest`,
  and fail-closed `phantom_ownership_retained = false` / `phantom_supply_retained = false`).
- **Complete 6-state lifecycle & 8/8 `CAPABILITY_TREE.md` gate matrix (`tests/wave24_client_1.rs`):**
  - 6-state typed lifecycle (`Submitted -> Admitted -> IncludedProvisional -> Finalized | Rejected |
    Replaced`) and 4-state root tracker (`Unverified | GenesisAnchor | ProvisionalCandidate |
    FinalizedCommitQc`).
  - `G1` competing candidates (`PrepareQC`-only candidate never upgrades to `Finalized`), `G2`
    delayed inclusion, `G3` nonce replacement (higher-tip same-nonce replacement marks prior tx
    `Replaced`), `G4` wrong-root receipt Merkle & bounded `<= 12 KiB` `DomainPatriciaProofV1`
    rejection, `G5` missing-body refusal, `G6` pre-finality reorg rollback (displaced provisional
    mint/burn rolls back with zero phantom ownership or phantom supply), `G7` expiry and clean
    resubmission, and `G8` authenticated epoch validator-set transition + `> 28`-epoch stale
    weak-subjectivity checkpoint rejection.
- **Merged-head release timings (`1b7f793`):** `CommitQC` finality + receipt & Patricia state-proof
  verification `450.19 us`, pre-finality reorg mint/burn/ownership atomic rollback `545.00 us`,
  canonical `ClientVerifiedStateViewV1` & `ClientTxLifecycleReceiptV1` encode `2.90 us`.
- **Visual verification:** `client_1_finality_reorg_chart.png` rendered on merged `1b7f793` was
  directly inspected via `read_file`.

## Non-claims

`BOUNDED` is mandatory: a locally displayed “confirmed” label is not stronger than the exact
consensus evidence and assumptions the client independently verified, and independent-process
behavior remains for `TESTNET-1`.
