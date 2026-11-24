# UPGRADE-1@base: bounded preventive v1->v2 migration receipt v1

This receipt resolves `PS-083:UPGRADE-1@base=BOUNDED` and unlocks
**`PS-092:SUCCESSOR-ACTIVATION-1@base=READY`** and
**`PS-093:SUCCESSOR-ACTIVATION-1@activation=READY`**, while keeping
`PROD-OPS-1`, `L1-CANDIDATE`, and `L1-PRODUCTION` strictly `LOCKED`.

## Provenance

- **Merged `origin/main` subject (PR #98):** commit `246ef87a5d2e5f7b682fda23cf88ffec3c50b9c6`
  (tree `ed796bf82839049dc35356965a434b1a0dcf5f1a`, parent `ba5eab8db29f2f2af609641d8ce455f6367df8a0`,
  CI runs `45294441732` & `45294451747`, 10/10 jobs green across Linux, Windows, `wasm32` V8
  parity, Python vectors, generated index, and Lean 4 `--trust=0`).
- **Executable module:** [`crates/research/src/upgrade_1.rs`](../crates/research/src/upgrade_1.rs)
  (`wasm32`-clean, strictly behind `--features protocol-research`, zero new Cargo dependencies,
  zero edits to frozen crates or modules).
- **Clean-room pure-Python verifier:**
  [`tools/upgrade_1_vectors.py`](../tools/upgrade_1_vectors.py) (`194/194` protocol schemas
  independently vectored; `0` without vectors).
- **CI & mutation evidence:**
  [`tests/wave31_upgrade_1.rs`](../tests/wave31_upgrade_1.rs) and
  `python3 tools/bbmutate.py upgrade_1` (`11/11` canonical big-endian sites KILLED, 100%).
- **Release probe:**
  [`examples/upgrade_1_migration_probe.rs`](../examples/upgrade_1_migration_probe.rs).

## Verified golden tags & preventive migration equations

Both Rust (`MigrationSpecV1`, `OccurrenceUniverseHandoffV1`, `BoundedMigrationReplayReceiptV1`,
`SuccessorUpgradeEvidenceManifestV1`, `execute_bounded_migration`, `verify_rollback_or_safe_halt`,
`verify_archival_historical_proof`, and `build_canonical_devnet_upgrade_1`) and clean-room pure
Python (`tools/upgrade_1_vectors.py`) match bit-for-bit across all four golden artifacts for the
preventive `v1 -> v2` `bitblock.devnet.v1` migration:

- **`MigrationSpecV1` (`bitblock/upgrade-1/migration-spec/v1`, `236` B wire):**
  `#bb.policy.sha256.i77xbr5xhfaowu6akytl6qk5okfn3urcpc3ckl2bx5iua6jwycua`
- **`OccurrenceUniverseHandoffV1` (`bitblock/upgrade-1/occurrence-universe-handoff/v1`, `187` B wire):**
  `#bb.policy.sha256.5u2vcytthbdbsd3hgjzum7ksovpe5top4t6isjquu5ddukefr2ba`
- **`BoundedMigrationReplayReceiptV1` (`bitblock/upgrade-1/bounded-migration-replay/v1`, `527` B wire):**
  `#bb.receipt.sha256.xt7rn7i6agoxhzv5tiohrwrh36lwb6vyoqa67vsylex3mvel36sq`
- **`SuccessorUpgradeEvidenceManifestV1` (`bitblock/successor-upgrade-evidence-manifest/v1`, `651` B wire):**
  `#bb.receipt.sha256.rrd55p74b5zijkyd3v6riq2jepyseru2ruodlohjsrpoyckel5ta`

### Verified preventive invariants & adversarial controls

- **Exact preventive `v1 -> v2` state delta & conservation:**
  - Successor `ProfileScopeV1` raises bounded execution limits (`max_tx_bytes = 16,384`,
    `max_block_txs = 1,536`, `max_block_ops = 65,536`, `max_proof_bytes = 16,384`,
    `max_state_writes_per_block = 65,536`), producing distinct `new_profile_id != old_profile_id`
    and distinct `new_scope_digest != old_scope_digest`.
  - Bounded migration writes exactly one governance parameter entry (`param_key = 9,001`,
    `param_value = 2`) into `DomainIdV1::GovernanceState` while conserving all nine other active
    domains bit-identically, `conserved_total_native_supply = 200,000,000,000`,
    `conserved_bonded_stake = 400,000,000,000`, and slot 6 (`ProgramState`) as
    `InactiveDomainSentinelV1`.
- **Live frozen `PROFILE-1` `SuccessorCandidate` admission (`CE-29`/`CE-30`/`CE-31`):**
  - `tests/wave31_upgrade_1.rs` drives `SuccessorUpgradeEvidenceManifestV1` directly into frozen
    `profile_1::admit_candidate_profile` with `ProfileLineageKindV1::SuccessorCandidate`,
    preserving polarity (`old_admitted_profile_id != new_candidate_profile_id`) with zero edits to
    `profile_1.rs`.
- **10/10 powered fail-closed controls (`tests/wave31_upgrade_1.rs`):** reversed/equal polarity,
  downgrade (`target_version <= old_version`), native supply drift, bonded stake drift, active
  `ProgramState` sentinel violation, occurrence-universe history-start mismatch, receipt
  preservation / child-certificate handoff tampering, rollback/safe-halt recovery on interrupted
  post-root mismatch, archival historical proof mismatch, and evidence manifest digest tampering all
  reject fail-closed.
- **Merged-head release timings (`246ef87`):** bounded `v1 -> v2` migration execution `64.93 us`,
  full verification + archival proof `9.33 us`, rollback/safe-halt recovery verification `27.81 us`.
- **Visual verification:** `upgrade_1_migration_chart.png` rendered on merged `246ef87` was
  directly inspected via `read_file`.

## Non-claims

`BOUNDED` is mandatory. This capability covers deterministic preventive migration while the
predecessor authority bindings remain sound. Neither governance approval nor old-root equality
proves arbitrary migration correctness, and `UPGRADE-1` does not recover an established
`ProfileOwnedAuthorityBindingCompromiseV1` or `ProfileIdHashV1` compromise through the failed
authority. Live consensus-native successor activation remains owned by `SUCCESSOR-ACTIVATION-1`.
