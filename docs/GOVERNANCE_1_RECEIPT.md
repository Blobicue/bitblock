# GOVERNANCE-1: authenticated timelocked governance and safe-halt receipt v1

This receipt resolves `PS-081:GOVERNANCE-1@base=BOUNDED` (keeping execution composition downstream
in `EXECUTOR-1` and `UPGRADE-FRAMEWORK-1`) and unlocks **`PS-055:VALIDATOR-SCHEDULE-1@base=READY`**
(while keeping `EXECUTOR-1`, `EXECUTION-ROOT-1`, `UPGRADE-FRAMEWORK-1`, and `CONSENSUS-1` honestly
`LOCKED`).

## Provenance

- **Merged `origin/main` subject (PR #60):** commit `35b26afd0f0abe8416a22edd009ce25f8e077be1`
  (tree `909ac87777587f9597d76016afb91f717cba1a70`, parent `5fe89211a19becbf1b4525a084909a1bff7bead7`,
  CI runs `45113664278` & `45113668408`, 10/10 jobs green).
- **Executable module:** [`crates/research/src/governance_1.rs`](../crates/research/src/governance_1.rs)
  (`wasm32`-clean, strictly behind `--features protocol-research`; zero changes to `crates/core/**`,
  `crates/research/src/commit_state.rs`, default binary, `world_tag`, `crates/wasm/www/*`, or
  `Cargo.lock`).
- **Independent Python reference suite:** [`tools/governance_1_vectors.py`](../tools/governance_1_vectors.py)
  (`150/150` protocol schemas independently vectored; reuses `DomainPatriciaPy`).
- **CI & mutation evidence:** [`tests/wave14c_governance_1.rs`](../tests/wave14c_governance_1.rs)
  and `python3 tools/bbmutate.py governance_1` (`24/24` canonical big-endian sites KILLED, 100%).
- **Release probe:** [`examples/governance_1_timelock_probe.rs`](../examples/governance_1_timelock_probe.rs).

## Verified golden tags & normative invariants

- **Authenticated governance root (`GovernanceStateRootV1`,
  `bitblock/governance-1/governance-root/v1`, `DomainIdV1::GovernanceState = 5`):**
  - Queued proposal root tag (`e = 1 -> activation 22`):
    `#bb.receipt.sha256.ro25qjtz5x6jglbdtk4pzqtlm5tqwfkxfpod53hn2amne7otuhxq`.
  - Activated parameter root tag (`e = 22`):
    `#bb.receipt.sha256.u5vnfvqypcrivjeu7d7jrsgfeaottqchqap6dxosvlksqdry5f2q`.
  - Emergency paused root tag (`e = 25..28`, cooldown until `31`):
    `#bb.receipt.sha256.6zrhhqdbeao6dhzcbotcfxyfiolbmrbj2wvoc4axkk2owomfpb6q`.
  - Automatic Safe-Halt root tag (`e = 35`, proven `BOUNDS-1` `SafeHaltEvidenceV1`):
    `#bb.receipt.sha256.hn4fvvlm3baxpa6ycedxmisfz4zx32j55r2sna5enjzzxfqx4tnq`.
- **Proof-carrying governance transition receipt (`GovernanceTransitionReceiptV1`,
  `bitblock/governance-1/governance-receipt/v1`):**
  - Queued proposal receipt tag:
    `#bb.receipt.sha256.lwhmyl2rjlhlpflygohdh2rpiccdeksimz75mjdo23jdqtktp5ha`.
- **All 10 `CAPABILITY_TREE.md` gate vectors verified fail-closed:**
  - Mandatory 21-epoch exit timelock for all `> 2/3` bonded-stake parameter and restart proposals;
    unauthorized (`<= 2/3`), replayed, conflicting-param-key, rushed, expired, stale-validator-set,
    and compromised-key proposals fail closed.
  - Pause-only Emergency Council bounded to `1..=3` epochs, auto-expiring at `pause_until_epoch`,
    enforcing a mandatory 3-epoch anti-DoS cooldown preventing back-to-back pauses, with zero
    state-mutation, history-rewrite, or Safe-Halt-unhalt authority.
  - Proven `BOUNDS-1` Safe-Halt clears only via `> 2/3` timelocked stake-weighted restart without
    rewriting finalized history; inclusion proofs (`152 B <= 12 KiB`) and cold rebuild parity hold.
- **Visual verification:** `governance_1_timelock_chart.png` rendered on merged `35b26af` was
  directly inspected via `read_file`.

## Non-claims

`BOUNDED` is mandatory: correct governance execution does not make decisions wise, decentralized,
fair, or immune to capture.
