# SIGNER-OPS-1: validator signer safety & operational drills receipt v1

This receipt resolves `PS-085:SIGNER-OPS-1@base=BOUNDED`, satisfying the validator signer safety
prerequisite ahead of `TESTNET-1` and `PROD-OPS-1` while preserving strict role separation,
monotonic anti-double-sign persistence, and explicit `BOUNDED` non-claims.

## Provenance

- **Merged `origin/main` subject (PR #88):** commit `8189ef19be0c64967f8f2eb580bbaacf26016566`
  (tree `d86acbf025aeab3f1265ea23d41c2fdb0ca29ade`, parent `ecccd596da815f8af4edc8ab18e27d118e9434c2`,
  CI runs `45192256063` & `45192271998`, 10/10 jobs green across Linux, Windows, Python vectors,
  generated index, and Lean 4 `--trust=0`).
- **Executable module:** [`crates/research/src/signer_ops_1.rs`](../crates/research/src/signer_ops_1.rs)
  (`wasm32`-clean core + `#[cfg(not(target_arch = "wasm32"))] pub mod signer_io` native OS
  exclusive lock `flock`/`share_mode(0)`, atomic `fsync`+rename watermark persistence, and
  `127.0.0.1` loopback-TCP `BBSIG1` remote-signer RPC; strictly behind `--features protocol-research`
  with zero new Cargo dependencies and zero edits to frozen crates/modules).
- **Clean-room pure-Python verifier:**
  [`tools/signer_ops_1_vectors.py`](../tools/signer_ops_1_vectors.py) (`178/178` protocol schemas
  independently vectored).
- **CI & mutation evidence:**
  [`tests/wave26_signer_ops_1.rs`](../tests/wave26_signer_ops_1.rs) and
  `python3 tools/bbmutate.py signer_ops_1` (`14/14` canonical big-endian sites KILLED, 100%).
- **Release probe:**
  [`examples/signer_ops_1_drill_probe.rs`](../examples/signer_ops_1_drill_probe.rs).

## Verified golden tags & normative invariants

Both Rust (`ValidatorKeyCeremonyBundleV1`, `SignerPolicyManifestV1`, `SignerAuditReceiptV1`,
`ValidatorSignerCoreV1`, `PersistentValidatorSignerV1`) and clean-room pure Python
(`tools/signer_ops_1_vectors.py`) match bit-for-bit across both golden artifacts:

- **`SignerPolicyManifestV1` (`bitblock/signer-ops-1/signing-policy/v1`, `257` bytes):**
  `#bb.policy.sha256.sbnuiur54vs5jeztkb724vb66dknfn2s6kqkgbgmqsgu4g35jqka` (binds `genesis_id`,
  `profile_id`, `validator_id`, distinct `proposal_key != vote_key`, key-ceremony transcript
  digest, `CRYPTO-1` `CustodyBackupEnvelopeV1` pair digest, `failover_epoch >= 1`,
  `min_watermark_floor`, `strict_role_separation = true`, and `allow_stale_backup_resume = false`).
- **`SignerAuditReceiptV1` (`bitblock/signer-ops-1/audit-receipt/v1`, `267` bytes):**
  `#bb.receipt.sha256.zsx5ynhdhu23fbzjq6tuhzfe7unacilht67ub6sgmd4rnzr2v6eq` (hash-chained audit
  receipt locking `decision == SignedApproved` <-> `signature_emitted == true`, monotonic
  `state_counter`, `failover_epoch`, role/domain codes, and `(epoch, height, round, phase)`
  watermark).
- **Complete 7/7 `CAPABILITY_TREE.md` operational drills (`tests/wave26_signer_ops_1.rs`):**
  - `D1` crash & corrupt watermark (atomic `write(.tmp) -> sync_all() -> rename()` preserves
    height 5 across restart; corrupted `watermark.bin` fails closed on open),
  - `D2` stale backup restore (`CustodyBackupEnvelopeV1` restore refused unless both
    `failover_epoch` and operator-attested watermark floor strictly advance),
  - `D3` concurrent replica OS exclusive lock (second open fails immediately via `flock` on Unix
    and `share_mode(0)` on Windows),
  - `D4` equivocation & rollback refusal (distinct `block_id` at same watermark and lower watermark
    both emit zero conflicting signatures and append chained refusal receipts),
  - `D5` active-to-standby failover (`failover_epoch` fencing rejects stale primary over
    `127.0.0.1` loopback TCP `BBSIG1` RPC; signed `ConsensusVoteV1` verified by `CONSENSUS-1`),
  - `D6` compromise & revocation (`CRYPTO-1` `report_compromise` halts signing immediately until
    clean rotation to a fresh `ValidatorVote` key),
  - `D7` clean-host disaster recovery + strict cross-role/cross-domain refusal (`ActorTransaction`,
    `GovernanceProposal`, `ReleaseArtifact`, mismatched `ValidatorProposal`/`ConsensusVote`).
- **Merged-head release timings (`8189ef1`):** atomic `fsync`+rename disk watermark sign
  `3361.56 us`, `127.0.0.1` loopback-TCP `BBSIG1` remote sign + atomic disk `3921.48 us`,
  equivocation/rollback refusal + chained audit `2.53 us`.
- **Visual verification:** `signer_ops_1_drills_chart.png` rendered on merged `8189ef1` was
  directly inspected via `read_file`.

## Non-claims

`BOUNDED` is mandatory: secure signer operation does not establish validator honesty, quorum
safety, hardware HSM tamper resistance, or network availability.
