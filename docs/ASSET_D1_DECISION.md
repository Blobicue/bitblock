# ASSET-D1 & VALIDATOR-ECON-D1: dual-domain asset and bonded PoS economics decision v1

This decision record resolves `PS-035:ASSET-D1@base=BOUNDED`,
`PS-036:ASSET-D1@asset-meaning=BOUNDED`, and `PS-037:VALIDATOR-ECON-D1@base=BOUNDED` (satisfying
pre-genesis design edge `CE-04` for stake-weighted consensus).

## Provenance

- **Merged `origin/main` subject (PR #26):** commit `ac503dce78ffb3951ea489ddb1d2cc35f4a1f178`
  (tree `950e59927c847833892b9b2debfe64b276227ae3`, CI runs `45009961375` & `45009964425`).
- **Executable contract:** [`crates/research/src/asset_d1.rs`](../crates/research/src/asset_d1.rs).
- **Independent Python vectors:** [`tools/pre_genesis_d1_vectors.py`](../tools/pre_genesis_d1_vectors.py).
- **CI integration & mutation evidence:** [`tests/pre_genesis_d1_contracts.rs`](../tests/pre_genesis_d1_contracts.rs)
  and `python3 tools/bbmutate.py asset_d1` (`13/13` sites KILLED).

## Selected design

### 1. `ASSET-D1` — Dual-domain model (`AssetPolicyDescriptorV1`)

- **Canonical default policy tag:** `#bb.policy.sha256.nxy3syo3wcq5ch7ayryyut25sbyustu4snj5vphox52fsnnlo4ca`
- **Domain 1 (Transferable non-fungible occupancy-episode tokens):** every live mutable occupied
  coordinate `(coord, mint_episode_id)` maps to one `owner: ActorIdV1` controlling transfer and
  burn. Transfers change owner metadata without rewriting `mint_episode_id` or world occupancy.
- **Domain 2 (Strictly separate fungible native unit):** pays burned execution/history fees
  (`base_fee_burn`, charged even on admitted failed transactions), refundable per-block storage
  deposits (`deposit_per_occupied_block`, locked on mint and refunded on burn), and bonded
  validator stake. A block token never doubles as fee balance or validator stake.
- **Bedrock `(0, 0, 0)`:** pure spatial structural invariant with **no economic token** and zero
  deposit lock (`BedrockHasNoEconomicToken` fail-closed).
- **No forced state rent:** `state_rent_enabled = false`; anti-spam relies on burned fees plus
  refundable locked deposits rather than forced expiry auto-burns.
- **Closed conservation invariant:** `AssetConservationLedgerV1` proves
  `sum(balances) + sum(locked_deposits) + total_burned_fees == total_issued_supply` across genesis,
  mint, transfer, burn, admitted failure, bedrock rejection, and explicit forward compensating
  mints (never rewriting finalized burns).

### 2. `VALIDATOR-ECON-D1` — Bonded PoS incentive schema (`ValidatorEconPolicyV1`)

- **Canonical default policy tag:** `#bb.policy.sha256.uzhiakc4bqij66hyg6h75kxsrlxqfaunxd7yzx36kdlxvii2gpaa`
- Fixes `min_self_stake > 0`, `2 <= activation_delay_epochs <= unbonding_delay_epochs`,
  `1 <= weak_subjectivity_horizon_epochs <= unbonding_delay_epochs`, `equivocation_slash_bps`
  (`5,000` default = 50% slash on both bonded and unbonding stake), `inactivity_slash_bps`,
  `max_epoch_reward_issuance`, and `< 1/3` single-validator concentration ceiling
  (`max_single_validator_bps <= 3,333`).
- `ValidatorStakeBookV1` verifies `bonded + unbonding + slashed + withdrawn == initial + rewards`
  and forbids premature unbonding claims or self-selected genesis weights.

## Rejected alternatives

- **Pure occupancy collectible (no fungible unit):** rejected because bonded PoS consensus and
  place/delete churn pricing require a clean fungible fee/deposit/stake unit.
- **1:1 fungible lock only / no separate episode token:** rejected to preserve distinct coordinate
  episode provenance and transferable block ownership.
- **Ongoing state rent / forced block expiry:** rejected to avoid silent or forced destruction of
  user-built spatial structures during outages or partitions.
- **Bedrock as a genesis token:** rejected to keep `(0, 0, 0)` cleanly aligned with `COMMIT-1`
  (`ActiveTokenRootV1` excludes bedrock).

## Non-claims

Choosing asset and PoS conservation semantics does not create market value, legal status, fair
distribution, Sybil resistance, or implemented runtime `ASSET-1` / `ECONOMIC-STATE-1` /
`VALIDATOR-ECON-1` state (all remain `LOCKED`).
