# VALIDATOR-ECON-1: consensus incentives adapter & adversarial assay receipt v1

This receipt resolves `PS-041:VALIDATOR-ECON-1@base=BOUNDED`, satisfying the launch-profile `CE-28`
`validator-economics` edge and unlocking `PS-084:PROFILE-1@base=READY` strictly as checkpoint
eligibility (conditional closure admission itself, `RELEASE-1`, `CHAIN-LAUNCH-1`, and `TESTNET-1`
remain `LOCKED` until a future wave delivers the powered admission checker).

## Provenance

- **Merged `origin/main` subject (PR #86):** commit `7ac86ef897b86f28de8a7abae111ea68994443c0`
  (tree `ed8dc07c2d1f69006002dc71c0dc38e41eeef061`, parent `dc2661d79f05033392cb784427808ee88bc0b531`,
  CI runs `45186434172` & `45186445394`, 10/10 jobs green across Linux, Windows, Python vectors,
  generated index, and Lean 4 `--trust=0`).
- **Executable module:** [`crates/research/src/validator_econ_1.rs`](../crates/research/src/validator_econ_1.rs)
  (`wasm32`-clean dedicated module strictly behind `--features protocol-research`, producing zero
  new execution-root domains and with zero edits to `crates/core/**`, `commit_state.rs`,
  `genesis_d1.rs`, `storage.rs`, `journal.rs`, `sync_1.rs`, `consensus_1.rs`, `client_1.rs`,
  default binary, `crates/wasm/**`, `Cargo.lock` (`220` packages), or Lean 4 proofs).
- **Clean-room pure-Python verifier:**
  [`tools/validator_econ_1_vectors.py`](../tools/validator_econ_1_vectors.py) (`176/176` protocol
  schemas independently vectored).
- **CI & mutation evidence:**
  [`tests/wave25_validator_econ_1.rs`](../tests/wave25_validator_econ_1.rs) and
  `python3 tools/bbmutate.py validator_econ_1` (`18/18` canonical big-endian sites KILLED, 100%).
- **Release probe:**
  [`examples/validator_econ_1_probe.rs`](../examples/validator_econ_1_probe.rs).

## Verified golden tags & normative invariants

Both Rust (`ValidatorEconAdapterV1`, `ValidatorEconAdapterReceiptV1`,
`ValidatorEconAdversarialAssayV1`, `run_canonical_validator_econ_adversarial_assay`) and clean-room
pure Python (`tools/validator_econ_1_vectors.py`) match bit-for-bit across both golden artifacts:

- **`ValidatorEconAdapterReceiptV1` (`bitblock/validator-econ-1/consensus-adapter-receipt/v1`,
  `443` bytes):** `#bb.receipt.sha256.kwwsv73gxzeuug576j55yktxgcdn6zo5zuv4pguns5ewzq4nukda` (binds
  accepted `ExecutionRootRefV1`, finalized `CONSENSUS-1` block/height/epoch, pre/post
  `EconomicStateRootV1` digests, exact `EconomicTransitionReceiptV1` digest for Epoch 1 pro-rata
  rewards `999_999_996` / `dust_unissued = 3`, `produces_execution_root_domain = false`, and
  `block_token_used_as_stake = false`).
- **`ValidatorEconAdversarialAssayV1` (`bitblock/validator-econ-1/adversarial-assay/v1`,
  `196` bytes):** `#bb.receipt.sha256.diw6cjminiwaapvdpjqh36cu7tthymzfzg62wrdeviizgnlftjta` (binds
  `ValidatorEconPolicyV1` & `THREAT-1` digests, `min_nakamoto_halt = 2`, `min_nakamoto_safety = 3`,
  `equivocation_slash_cost_bps = 5_000`, `min_safety_attack_burn_bps = 1_667`,
  `passed_attack_mask = 0x7F`, `monitored_loss_alert_count = 2`,
  `block_token_stake_aliasing_rejected = true`, and the 7-scenario transcript digest).
- **Complete 7/7 adversarial incentive vectors & negative controls
  (`tests/wave25_validator_econ_1.rs`):**
  - `A1` bribery (`>= 1,667 bps` safety-attack burn floor + `BriberyExceedsSlashFloor` alert),
    `A2` censorship (`25%` single validator cannot stall; `> 1/3` cartel triggers
    `CensorshipCartelAboveOneThird` alert), `A3` equivocation slash-and-flee defeat (`50B` burned
    across `80B` active + `20B` unbonding + permanent jailing matching `ECONOMIC-STATE-1` receipt),
    `A4` griefing (`1%` inactivity slash + admitted-failure `base_fee_burn`), `A5` concentration
    (`CE-15` `2,500 bps` cap enforcing Nakamoto halt `>= 2` and safety `>= 3`), `A6` long-range
    (`> 21`-epoch stale unbonded key rejected), and `A7` recovery (jailed re-bond refused; `v2`
    top-up + `v5` `100B` bond restores `4` active validators with exact supply conservation).
  - Rejects occupancy block-token-as-stake aliasing, `produces_execution_root_domain = true`,
    `block_token_used_as_stake = true`, wrong `ExecutionRootRefV1`, non-conflicting equivocation
    votes, `> 3,333 bps` caps, and consensus schedule weight drift.
- **Merged-head release timings (`7ac86ef`):** weight audit + epoch reward correspondence
  `1237.73 us`, verified `VoteEquivocationEvidenceV1` 50% active+unbonding slash correspondence
  `754.24 us`, complete 7-vector adversarial incentive assay `841.01 us`.
- **Visual verification:** `validator_econ_1_incentives_chart.png` rendered on merged `7ac86ef` was
  directly inspected via `read_file`.

## Non-claims

`BOUNDED` is mandatory: occupancy scarcity does not automatically secure consensus, a block token
must not silently become validator stake, and bounded adversarial simulation does not prove that
real-world participants are independent or that external bribery markets remain below the modeled
slashing floor.
