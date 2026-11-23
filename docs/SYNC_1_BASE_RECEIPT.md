# SYNC-1@base: 4-role peer synchronization, availability & recovery receipt v1

This receipt resolves `PS-034:SYNC-1@base=BOUNDED` and unlocks
**`PS-056:CONSENSUS-1@base=READY`** while strictly keeping **`PS-095:SYNC-1@replay-authority=LOCKED`**
on `PD-06` (`CONSENSUS-1@base`), along with `CLIENT-1`, `VALIDATOR-ECON-1`, `PROFILE-1`,
`SIGNER-OPS-1`, `UPGRADE-1`, and `CHAIN-LAUNCH-1` honestly `LOCKED`.

## Provenance

- **Merged `origin/main` subject (PR #78):** commit `b1485f57507d202359ea73acd485b19b23d353d8`
  (tree `ab3c151baf9502b830de0123a891857c0dfc27ef`, parent `5f077cfbf62d24e1fa977b26ef32efae934910cc`,
  CI runs `45150460787` & `45150662852`, 10/10 jobs green across Linux & Windows).
- **Executable module:** [`crates/research/src/sync_1.rs`](../crates/research/src/sync_1.rs)
  (`wasm32`-clean portable core + native `std::net` loopback TCP multi-peer bootstrap adapter
  `sync_net` strictly behind `--features protocol-research`; zero new Cargo dependencies with
  `Cargo.lock` frozen at `212` packages; zero edits to `crates/core/**`, `storage.rs`, `journal.rs`,
  `commit_state.rs`, `genesis_d1.rs`, default binary, `crates/wasm/**`, or Lean 4 proofs).
- **Clean-room pure-Python verifier:** [`tools/sync_1_vectors.py`](../tools/sync_1_vectors.py)
  (`168/168` protocol schemas independently vectored).
- **CI & mutation evidence:** [`tests/wave21_sync_1.rs`](../tests/wave21_sync_1.rs) and
  `python3 tools/bbmutate.py sync_1` (`21/21` canonical big-endian sites KILLED, 100%).
- **Release probe:** [`examples/sync_1_cluster_probe.rs`](../examples/sync_1_cluster_probe.rs).

## Verified golden tags & normative invariants

Both Rust (`FullVerifierNodeV1`, `ArchiveNodeV1`, `LightClientNodeV1`, `NonAuthorityRpcNodeV1`,
`PeerTableV1`, `OpaqueAuthorityBlobV1`, `SyncClusterHarnessV1`, `sync_net`) and clean-room pure
Python (`tools/sync_1_vectors.py`) match bit-for-bit across all 3 golden artifacts:

- **Chunked checksummed `SyncCheckpointManifestV1` (`bitblock/sync-1/checkpoint-manifest/v1`):**
  `#bb.receipt.sha256.m267jyqvgn3t7iqja3cu4jnb3qxnlpqjxr3lrlh4dq77nnura6qa` (`1,024`-byte chunks
  bound to `BootstrapAvailabilityPolicyV1`, `header_digest`, 11-slot `ExecutionRootRefV1`, and
  `opaque_blobs_digest`).
- **Deterministic `SyncRecoveryReceiptV1` (`bitblock/sync-1/sync-recovery-receipt/v1`, `380` bytes):**
  `#bb.receipt.sha256.p4ock6iohrsau6v6sjkehw3tt36kw2zj2gdrjhqjgt25j2aly5aq` (`FullVerifier`,
  `8` discovered / `7` banned / `1` honest peer, `retained_epoch_window = 28`,
  `live_authority_admitted = false` enforced fail-closed on encode and decode).
- **Opaque authority blob set digest (`OpaqueAuthorityBlobV1`):**
  `85f14c56e34047bdab721baaf4dfa88cbc6824733b1d6b3cba5cba323bf3fc75` (retains bounded
  `<= 64 KiB` opaque certificate & occurrence bytes verbatim with compile-time invariant
  `interpreted_as_live_authority() == false`, preserving `PS-095:SYNC-1@replay-authority=LOCKED`).
- **Strict 4-role separation:** `FullVerifier` independently executes & prunes strictly below the
  `>= 28`-epoch window without dropping tip or pinned anchors; `Archive` retains complete unpruned
  history + opaque blobs and rejects pruning (`ArchivePruningForbidden`); `LightClient` stores
  `0` batch bodies and verifies signed headers plus bounded `<= 12 KiB` `ReceiptMerkleProofV1` and
  `DomainPatriciaProofV1` witnesses; `NonAuthorityRpc` enforces read-only rate-limited queries
  (`<= 4 MiB` inflight, `<= 64 req/s`, `can_mutate_ledger() == false`,
  `can_admit_live_authority() == false`).
- **Complete 7/7 fault-injection gate matrix:** verifies `F1` 8-peer eclipse (7/8 malicious peers
  banned with honest recovery; 8/8 total eclipse safe-halts via `TotalEclipseNoHonestPeers` with
  zero corrupted state), `F2` partial-data/withheld-body peer ban, `F3` 1-bit corrupt checkpoint
  chunk rejection before install, `F4` slow-peer/backpressure DoS throttling (`> 4 MiB` / `> 64 req/s`),
  `F5` `STORAGE-1` filesystem `Journal` crash-restart torn-tail recovery, `F6` `>= 28`-epoch
  retention pruning + archive refusal, and `F7` fresh-node bootstrap over real `127.0.0.1`
  loopback TCP with untouched opaque blob preservation.
- **Merged-head release timings (`b1485f5`, 50-run p50):** FullVerifier 7/8-eclipse recovery
  `946.83 us`, LightClient header + `<= 12 KiB` proof verification `38.02 us`, checkpoint chunk
  SHA-256 verification + reassembly `8.66 us`.
- **Visual verification:** `sync_1_fault_resilience_chart.png` rendered on merged `b1485f5` was
  directly inspected via `read_file`.

## Non-claims

`BOUNDED` is mandatory: data retrieval does not prove correct execution, canonical ordering,
finality, or certificate admission. Replay reconstruction audits accepted authority and never
selects it live; `PS-095:SYNC-1@replay-authority` remains strictly `LOCKED` until `CONSENSUS-1@base`.
