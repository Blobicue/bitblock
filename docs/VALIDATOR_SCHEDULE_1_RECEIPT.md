# VALIDATOR-SCHEDULE-1: deterministic delayed validator schedule receipt v1

This receipt resolves `PS-055:VALIDATOR-SCHEDULE-1@base=BOUNDED` (completing the Waves 13–15
Launch State-Producer Trunk across Domains 1–7) and unlocks **`PS-065:EXECUTOR-1@base=READY`**
(while keeping `EXECUTION-ROOT-1`, `GENESIS-ROOT-PARITY-1`, `UPGRADE-FRAMEWORK-1`, and
`CONSENSUS-1` honestly `LOCKED`).

## Provenance

- **Merged `origin/main` subject (PR #64):** commit `e61f06135ef526b417f14e2752bca1532b3296c5`
  (tree `12b1ed2015ffa549a3dada6d4c43a6750acb68b4`, parent `0c536d36437558f60eea7b6c440b9bebd895c499`,
  CI runs `45119003344` & `45119010081`, 10/10 jobs green).
- **Executable module:** [`crates/research/src/validator_schedule_1.rs`](../crates/research/src/validator_schedule_1.rs)
  (`wasm32`-clean, strictly behind `--features protocol-research`; zero changes to `crates/core/**`,
  `crates/research/src/commit_state.rs`, default binary, `world_tag`, `crates/wasm/www/*`, or
  `Cargo.lock`).
- **Independent Python reference suite:** [`tools/validator_schedule_1_vectors.py`](../tools/validator_schedule_1_vectors.py)
  (`154/154` protocol schemas independently vectored; reuses `DomainPatriciaPy`).
- **CI & mutation evidence:** [`tests/wave15c_validator_schedule_1.rs`](../tests/wave15c_validator_schedule_1.rs)
  and `python3 tools/bbmutate.py validator_schedule_1` (`15/15` canonical big-endian sites KILLED, 100%).
- **Release probe:** [`examples/validator_schedule_1_probe.rs`](../examples/validator_schedule_1_probe.rs).

## Verified golden tags & normative invariants

- **Authenticated validator schedule root (`ValidatorScheduleRootV1`,
  `bitblock/validator-schedule-1/schedule-root/v1`, `DomainIdV1::ValidatorSchedule = 7`):**
  - Genesis bootstrap root tag (`epochs 0..1`, `4` validators, `2,500 bps`):
    `#bb.receipt.sha256.3odndsylyhbqu3zinnag6r5zc7nveqfui2nmhcl6544elyfv56yq`.
  - Post-join delayed schedule root tag (`epoch 2` active, `5` validators, `2,000 bps`):
    `#bb.receipt.sha256.3ht2hivyqt4fijsvojhzyadzy3lzfajb3xlyy53jhboq4l7mbkta`.
- **Immutable per-epoch validator schedule (`EpochValidatorScheduleV1`,
  `bitblock/validator-schedule-1/epoch-schedule/v1`):**
  - Delayed `effective_epoch = 2` schedule tag (derived from `source_epoch = 0` economic &
    governance roots):
    `#bb.receipt.sha256.jpxuitjyubnexfutl57v2ypfehtlyynd2r27zflm7otqih6hxmca`.
- **Normative scheduling, `CE-15`, and safety gates (`7/7` verified):**
  - Strict `2`-epoch activation delay (`effective_epoch = source_epoch + 2`) applies to all stake
    changes, joins, exits, slashes, and validator key rotations; retroactive schedule rewrites
    reject fail-closed (`RejectedRetroactiveScheduleRewrite`).
  - `CE-15` economic coupling filters out jailed and `< min_self_stake` validators, enforces
    `4 <= N <= 128`, deterministically caps single-validator weight `<= 2,500 bps` for `N >= 5`,
    and hard-rejects any schedule with share `> 3,333 bps` (`< 1/3` BFT safety ceiling).
  - Safe-Halted governance blocks new epoch scheduling; actor/validator key-domain collisions
    reject fail-closed (`RejectedKeyDomainCollision`); historical epoch proofs (`694 B <= 12 KiB`)
    verify across epochs `0..4` with Domain 7 isolation and cold rebuild parity.
- **Visual verification:** `validator_schedule_1_epochs_chart.png` rendered on merged `e61f061`
  was directly inspected via `read_file`.

## Non-claims

`BOUNDED` is mandatory: a committed validator schedule does not prove quorum honesty,
decentralization, consensus safety/liveness, economic adequacy, signer custody, or data availability.
