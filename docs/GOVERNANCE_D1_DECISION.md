# GOVERNANCE-D1: upgrade, safe-halt, and emergency authority decision v1

This decision record resolves `PS-075:GOVERNANCE-D1@base=BOUNDED` and
`PS-076:GOVERNANCE-D1@authority-choice=BOUNDED`.

## Provenance

- **Merged `origin/main` subject (PR #26):** commit `ac503dce78ffb3951ea489ddb1d2cc35f4a1f178`
  (tree `950e59927c847833892b9b2debfe64b276227ae3`, CI runs `45009961375` & `45009964425`).
- **Executable contract:** [`crates/research/src/consensus_gov_d1.rs`](../crates/research/src/consensus_gov_d1.rs).
- **Independent Python vectors:** [`tools/pre_genesis_d1_vectors.py`](../tools/pre_genesis_d1_vectors.py).
- **CI integration & mutation evidence:** [`tests/pre_genesis_d1_contracts.rs`](../tests/pre_genesis_d1_contracts.rs).

## Selected authority model (`GovernancePolicyDescriptorV1`)

- **Fixture canonical tag:** `#bb.policy.sha256.advjjkb4urtxkycounxi54i7x4x7uvefeqcatwnrzatpys7n2lwa`
- **Ordinary upgrades & parameter changes:** require strict `> 2/3` bonded-stake supermajority
  (`3 * approved_stake > 2 * total_weight`) AND a **mandatory public exit timelock**
  (`mandatory_exit_timelock_epochs >= unbonding_delay_epochs`, `21` epochs default) so any
  dissenting user or validator can fully unbond and exit before activation.
- **Automatic deterministic safe-halt:** proven equivocation, `AcceptedParentCollisionDispositionV1`,
  or committed `BOUNDS-1` violations trigger automatic deterministic safe-halt (`automatic_safe_halt_on_proven_fault = true`).
- **Strictly pause-only emergency council:** a role-separated `m-of-n` `ThresholdPolicyV1` bound to
  `CredentialRoleV1::EmergencyPauseSigner` may trigger an auto-expiring pause bounded by
  `1..=max_emergency_pause_epochs` (`<= mandatory_exit_timelock_epochs`) with signed public evidence
  digest. It has **zero state-mutation power** (`EmergencyStateMutationAttempt` rejected fail-closed).
- **Hard ban on rewriting finalized history:** `forbid_history_rewrite = true`. Finalized burns or
  transitions are never erased; recovery is strictly a forward timelocked compensating transition
  or a separately disclosed new genesis (`SafeHaltAndSeparatelyTrustedNewGenesisV1`).

## Rejected alternatives

- **Pure stake governance with no emergency pause path:** rejected because zero pause valve leaves
  a live implementation bug exposed across the full exit timelock before an upgrade activates.
- **Bootstrap multisig holding full upgrade authority:** rejected to prevent unilateral council
  parameter/rule changes without `> 2/3` bonded-stake consent and exit timelock.
- **Zero on-chain governance (hard-fork-only):** rejected because deterministic in-place upgrade
  scheduling (`UPGRADE-1`) and bounded safe-halt rules benefit from explicit committed policy.

## Non-claims

A governance schema does not make decisions wise, uncensorable, legally legitimate, or immune to
stake capture, and does not implement runtime `GOVERNANCE-1` or `UPGRADE-1` (both remain `LOCKED`).
