# ECONOMIC-STATE-1: deterministic stake, reward, unbonding, and slash receipt v1

This receipt resolves `PS-040:ECONOMIC-STATE-1@base=BOUNDED` (satisfying the `CE-15`/`CE-18`
economic state-producer edge ahead of `VALIDATOR-SCHEDULE-1` and `EXECUTOR-1`, while keeping
`VALIDATOR-ECON-1`, `EXECUTOR-1`, `EXECUTION-ROOT-1`, and `CONSENSUS-1` honestly `LOCKED`).

## Provenance

- **Merged `origin/main` subject (PR #62):** commit `685093b438f152942259e94965563f49a964867a`
  (tree `cd8475b1cf5c5faa31eed9c201f68fa6801e6da2`, parent `02179aeb22eeb56c6fd75289ae6e7fd5c1f10ade`,
  CI runs `45116181173` & `45116327471`, 10/10 jobs green).
- **Executable module:** [`crates/research/src/economic_state_1.rs`](../crates/research/src/economic_state_1.rs)
  (`wasm32`-clean, strictly behind `--features protocol-research`; zero changes to `crates/core/**`,
  `crates/research/src/commit_state.rs`, default binary, `world_tag`, `crates/wasm/www/*`, or
  `Cargo.lock`).
- **Independent Python reference suite:** [`tools/economic_state_1_vectors.py`](../tools/economic_state_1_vectors.py)
  (`152/152` protocol schemas independently vectored; reuses `DomainPatriciaPy`).
- **CI & mutation evidence:** [`tests/wave15a_economic_state_1.rs`](../tests/wave15a_economic_state_1.rs)
  and `python3 tools/bbmutate.py economic_state_1` (`24/24` canonical big-endian sites KILLED, 100%).
- **Release probe:** [`examples/economic_state_1_probe.rs`](../examples/economic_state_1_probe.rs).

## Verified golden tags & normative invariants

- **Authenticated economic state root (`EconomicStateRootV1`,
  `bitblock/economic-state-1/economic-root/v1`, `DomainIdV1::EconomicStake = 6`):**
  - Post-equivocation-slash root tag (`50%` burned across active + unbonding):
    `#bb.receipt.sha256.x4sivbvgyjbvy7od5mxdfoiptaywamcdg42754yayqdqnjpaes7q`.
  - Post-matured-withdrawal root tag (`e = 23`, post-slash `10B` released):
    `#bb.receipt.sha256.kc7wgdidj5xxkz7v345cskrfi6x6vpfs6fbije4pdhv3m5wysreq`.
- **Proof-carrying economic transition receipts (`EconomicTransitionReceiptV1`,
  `bitblock/economic-state-1/economic-receipt/v1`):**
  - Integer-floor reward distribution receipt (`budget = 999_999_999`, `issued = 999_999_996`,
    `dust_unissued = 3`):
    `#bb.receipt.sha256.r2jxjtxll4lptlqmd7d5gkbvlzphwe2m2rkb5rlr364sjqq43ndq`.
  - Equivocation `50%` slash-to-burn receipt (`50B` burned across active + unbonding):
    `#bb.receipt.sha256.kzjsjcb6un22a2u45uyci43gky5bh7o7r3h6xpxzv5aisfx5o7xa`.
- **Normative staking, reward, unbonding, and slashing gates (`7/7` verified):**
  - Closed supply conservation (`genesis_bonded_pool + cumulative_added_bonds +
    cumulative_issued_rewards == total_active_bonded + total_unbonding + total_accrued_rewards +
    total_withdrawn + total_slashed_burned`) verified on every transition and decode.
  - 100% slash-to-burn across BOTH active bonded stake and pending 21-epoch unbonding entries
    defeats slash-and-flee races; equivocation permanently jails the validator (`RejectedValidatorJailed`).
  - 21-epoch unbonding timelock rejects early withdrawals (`RejectedUnbondingNotMatured`);
    integer-floor pro-rata rewards enforce `max_epoch_reward_issuance` with zero hidden issuance.
  - Domain 6 inclusion proofs and cold full-rebuild parity verified; cross-domain proofs reject.
- **Visual verification:** `economic_state_1_staking_chart.png` rendered on merged `685093b` was
  directly inspected via `read_file`.

## Non-claims

`BOUNDED` is mandatory: committed economic state does not show that incentives are sufficient,
participants are independent, the asset has value, or consensus is safe or live.
