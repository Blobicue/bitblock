# IDENTITY-1: actor identity, depth-1 delegation, timelocked recovery, and key-domain isolation receipt v1

This receipt resolves `PS-028:IDENTITY-1@base=BOUNDED` (keeping durable authenticated state and
nonce root commitment downstream in `AUTH-COMMIT-1`) and unlocks **`PS-029:AUTH-COMMIT-1@base=READY`**.

## Provenance

- **Merged `origin/main` subject (PR #52):** commit `92dced7be8b25f59f08ea12c6cd5a37c6592c8d2`
  (tree `91b6c5902c7d5ca119aa8678346dd2c27e4e90f0`, parent `bbfed5930f04407429ed5f2b51e7fc2ee77245e7`,
  CI runs `45101644050` & `45101649155`, 10/10 jobs green).
- **Executable module:** [`crates/research/src/identity_1.rs`](../crates/research/src/identity_1.rs)
  (`wasm32`-clean, strictly behind `--features protocol-research`; zero changes to `crates/core/**`,
  default binary, `world_tag`, `crates/wasm/www/*`, or `Cargo.lock`).
- **Independent Python reference suite:** [`tools/identity_1_vectors.py`](../tools/identity_1_vectors.py)
  (`142/142` protocol schemas independently vectored).
- **CI & mutation evidence:** [`tests/wave12a_identity_1.rs`](../tests/wave12a_identity_1.rs) and
  `python3 tools/bbmutate.py identity_1` (`13/13` canonical big-endian sites KILLED, 100%).
- **Release probe:** [`examples/identity_1_auth_probe.rs`](../examples/identity_1_auth_probe.rs).

## Verified golden tags & normative invariants

- **Actor authorization state (`ActorAuthStateV1`, `bitblock/identity-1/actor-auth-state/v1`):**
  canonical initial principal state tag
  `#bb.receipt.sha256.hthq4z3wjktuwvlwzg4uilsby2w4g2qia5tky55bk6meccqm26eq`.
- **Deterministic authorization receipts (`IdentityAuthReceiptV1`, `bitblock/identity-1/authorization-receipt/v1`):**
  - Direct principal admission (`h=10`, nonce `0 -> 1`):
    `#bb.receipt.sha256.63wc5rwka53hfx4nqtais7zz4lsxyfimvkt3mola6mqjqey4indq`.
  - Depth-1 delegated admission (`h=20`, delegate `B` acting on `On` for `A`, nonce `1 -> 2`):
    `#bb.receipt.sha256.q46n6n3ms6vjtbhy3marabz7d4jjhtvq6mjhuwyk7azceajbspmq`.
  - Timelocked recovery finalization (`h=250 >= unlock_height`, `ActorIdV1` preserved,
    `descriptor_version -> 1`, stale delegations cleared):
    `#bb.receipt.sha256.yo2g77yhasl2edcaxur5teoh4je2qz4chztcwsnmhqd7txduplha`.
- **Adversarial gate coverage (`8/8` matrix gates deterministic):** signature mutation, small-order
  torsion key, duplicate signer, wrong signing domain, cross-chain/genesis/version/world replay,
  validator key cross-domain use (`RejectedValidatorKeyDomainCrossUse`), stale replay & future-gap
  nonces, depth-2 transitive sub-delegation (`RejectedTransitiveDelegation`), scope denial, grant
  expiry, early recovery timelock (`RejectedRecoveryTimelockActive`), admin cancel, and permanent
  revocation all produce exact typed fail-closed dispositions without mutating state or nonces.
- **Merged-head release latency (`92dced7`, 200 runs):** direct authorization `50.929 us` p50,
  depth-1 delegated authorization `50.960 us` p50, recovery finalization `0.640 us` p50.
- **Visual verification:** `identity_1_auth_flow_chart.png` rendered on merged `92dced7` was
  directly inspected via `read_file`.

## Non-claims

`BOUNDED` is mandatory: a public key proves key control, not one unique human, legal ownership,
or validator authority; durable authenticated authorization/nonce root persistence remains
downstream in `AUTH-COMMIT-1`.
