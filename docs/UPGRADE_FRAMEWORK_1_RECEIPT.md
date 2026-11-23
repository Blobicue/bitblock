# UPGRADE-FRAMEWORK-1: version lifecycle, preventive migration, and compromise matrix receipt v1

This receipt resolves `PS-082:UPGRADE-FRAMEWORK-1@base=BOUNDED` (while keeping `UPGRADE-1`,
`PROFILE-1`, `MULTI-1`, `SEQUENCER-1`, `CONSENSUS-1`, and `CHAIN-LAUNCH-1` honestly `LOCKED`).

## Provenance

- **Merged `origin/main` subject (PR #70):** commit `d283dedaac924065c80f560cb0d7b1c1f8fadb8b`
  (tree `9b4b7c118b99d90d4a6b6500b0376fdf38533244`, parent `593c5790287a7e0f6ab2d35e9f022851fa8c220f`,
  CI runs `45131829185` & `45131836216`, 10/10 jobs green).
- **Executable module:** [`crates/research/src/upgrade_framework_1.rs`](../crates/research/src/upgrade_framework_1.rs)
  (`wasm32`-clean, strictly behind `--features protocol-research`; zero changes to `crates/core/**`,
  `crates/research/src/commit_state.rs`, `crates/research/src/genesis_d1.rs`, default binary,
  `world_tag`, `crates/wasm/**`, `Cargo.lock`, or Lean 4 proofs).
- **Independent Python reference suite:** [`tools/upgrade_framework_1_vectors.py`](../tools/upgrade_framework_1_vectors.py)
  (`160/160` protocol schemas independently vectored).
- **CI & mutation evidence:** [`tests/wave17c_upgrade_framework_1.rs`](../tests/wave17c_upgrade_framework_1.rs)
  and `python3 tools/bbmutate.py upgrade_framework_1` (`8/8` canonical big-endian sites KILLED, 100%).
- **Release probe:** [`examples/upgrade_framework_1_probe.rs`](../examples/upgrade_framework_1_probe.rs).

## Verified golden tags & normative invariants

- **Canonical lifecycle policies (`UpgradeFrameworkPolicyV1`,
  `bitblock/upgrade-framework-1/lifecycle-policy/v1`):**
  - Preventive in-place migration policy (`VersionLifecycleModeV1::PreventiveInPlaceMigration`):
    `#bb.policy.sha256.udwdlgfcdx3lmezbw6ffttvoom2bqjv2tlyxnbekhmtswgeyy74q`.
  - Immutable-version policy (`VersionLifecycleModeV1::ImmutableVersionOnly`, refuses any version
    change as a separate chain requiring a new genesis):
    `#bb.policy.sha256.kqe4wzngoso465trwfsy2brufqygodn7vytur6af27s7rrqeoxdq`.
- **Canonical transition & compromise audit receipts (`UpgradeFrameworkAuditReceiptV1`,
  `bitblock/upgrade-framework-1/transition-audit/v1`):**
  - Preventive migration admitted (`old_version = 1 -> target_version = 2`):
    `#bb.receipt.sha256.efusvpoh6mfavk4gq7x3lg3gv2fe6vzrfnjw6xatkfllaehnmpeq`.
  - Established authority compromise (`CollisionOrForgery -> SafeHaltAndSeparatelyTrustedNewGenesisV1`):
    `#bb.receipt.sha256.sgjpzscp672njwiyhayextl5ccgcx4ypk45b6iw4ci7j6kmyj7vq`.
  - Terminal Row-1 `ProtocolIdentityHashCompromiseV1`:
    `#bb.receipt.sha256.blfxqa7vir3smboq2b2ecuh3i4mas3hvr7qhohrhjauuuabfy6tq`.
  - Uninhabited `PrecommittedIndependentRecoveryEnvelopeV1` wire claim rejection:
    `#bb.receipt.sha256.agawoxk3h6ceowzjzjgonqtz43jpngokg5jzdvrnupvauwaoytza`.
- **Merged-head release timings (`d283ded`, 200-run p50):** preventive `CE-43` migration check
  `24.14 us`, 6-control compromise disposition audit `0.03 us`, 10-height historical & downgrade
  preservation check `0.03 us`.
- **Complete 12-control version-lifecycle & Loop 8–10 compromise matrix:**
  - Genesis height `0` rejects fabricated old-profile (`InPlaceSuccessorProfileV1`) inputs.
  - Immutable-version mode refuses all in-place version bumps; preventive mode requires admitted old
    profile, distinct `CE-43` target candidate, strict version increase, active governance with
    matured timelock, unique last-safe parent, and sound live bindings, preserving pre-activation
    historical verification while rejecting post-activation downgrades and cleanly rolling back
    interrupted migrations.
  - All 6 established authority-compromise controls (`CollisionOrForgery`, `AmbiguousLastSafeParent`,
    `FailedRoleAuthorized`, `HypotheticalPrecommit`, `PostCompromisePrecommit`, `NoPrecommit`)
    resolve exclusively to `CurrentEstablishedAuthorityCompromiseDispositionV1 =
    SafeHaltAndSeparatelyTrustedNewGenesisV1`.
  - `PrecommittedIndependentRecoveryEnvelopeV1` is a zero-constructor compile-time uninhabited Rust
    enum whose reserved wire tag always fails closed; `ProtocolIdentityHashCompromiseV1` (Row 1) is
    strictly terminal.
- **Visual verification:** `upgrade_framework_1_matrix_chart.png` rendered on merged `d283ded` was
  directly inspected via `read_file`.

## Non-claims

`BOUNDED` is mandatory: a framework does not prove any concrete migration or authorize an upgrade.
Neither an in-place successor nor a recovery object selected through a failed live authority
binding can repair that binding; no independent recovery envelope is currently specified.
