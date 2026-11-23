# CRYPTO-1: cryptographic key lifecycle, custody envelope, and anti-downgrade receipt v1

This receipt resolves `PS-070:CRYPTO-1@base=BOUNDED` (keeping third-party external cryptographic
audit and physical HSM custody honestly open) and unlocks **`PS-028:IDENTITY-1@base=READY`**.

## Provenance

- **Merged `origin/main` subject (PR #48):** commit `c19542aab190e185cea11bf1cd54333bc40bd86d`
  (tree `b55b78a0df83bf734966ca460be3ddf320766aad`, parent `ca26b37bf1e4d5d2a4bb5a99a5e6d0cec75f90ec`,
  CI runs `45096374508` & `45096378966`, 10/10 jobs green).
- **Executable module:** [`crates/research/src/crypto_1.rs`](../crates/research/src/crypto_1.rs)
  (`wasm32`-clean, strictly behind `--features protocol-research`; zero new Cargo dependencies,
  `Cargo.lock` 212-package SBOM untouched).
- **Independent Python reference suite:** [`tools/crypto_1_vectors.py`](../tools/crypto_1_vectors.py)
  (`138/138` protocol schemas independently vectored).
- **CI & mutation evidence:** [`tests/wave10_crypto_1.rs`](../tests/wave10_crypto_1.rs) and
  `python3 tools/bbmutate.py crypto_1` (`16/16` canonical big-endian sites KILLED, 100%).
- **Release probe & audit JSON:** [`examples/crypto_1_lifecycle_probe.rs`](../examples/crypto_1_lifecycle_probe.rs)
  and [`crypto-1-audit-v1.json`](crypto-1-audit-v1.json).

## Verified golden tags & normative invariants

- **Key lifecycle record (`KeyLifecycleRecordV1`, `bitblock/crypto-1/key-lifecycle-record/v1`):**
  rotated record tag `#bb.receipt.sha256.3dyl5u2pyn7r4svpu5p7sxnny7qgv3hqui3edgasuwfiiwjzvkcq`
  (`Active -> Rotated | Revoked | Compromised` state machine with irreversible compromise
  quarantine, strict 9×9 signing-domain separation: `9` diagonal OK / `72` cross-domain rejected).
- **Authenticated custody backup/restore (`CustodyBackupEnvelopeV1`, `bitblock/crypto-1/custody-backup-envelope/v1`):**
  tag `#bb.receipt.sha256.lc3kdko2lsupgbqwysbcsoyadi3z62p4pde3ch5ioldg3hqriumq`
  (`PBKDF2-HMAC-SHA256` -> `HKDF-SHA256` -> `SHA-256-CTR` + `HMAC-SHA256` Encrypt-then-MAC with
  constant-time MAC check before decryption and canonical public-key binding check; exhaustive
  `136/136`-byte 1-bit tamper sweep caught fail-closed; restore latency `0.497 ms`).
- **Anti-downgrade migration policy (`SuiteMigrationPolicyV1`, `bitblock/crypto-1/suite-migration-policy/v1`):**
  tag `#bb.policy.sha256.3r2uumuzxwfdhct2e6xniiatblaj555mjjgxpy4dtdy4tvmup6oq`
  (forbids suite-ID downgrades, mandates dual-sign overlap `>= 1,024` blocks, and forbids
  post-sunset legacy fallback).
- **Bounded batch verification with blame isolation (merged `c19542a`):** `2,048`-signature
  lifecycle + strict RFC 8032 batch verify runs in `67.354 ms` p50 (`68.540 ms` max; `5.9x` below
  `BOUNDS-1`'s `400 ms` budget), fault injection at index `1337` pins exact
  `first_invalid_index = 1337` in `43.506 ms`, and over-limit batches fail closed before verifying.
- **Visual verification:** `crypto_1_lifecycle_chart.png` rendered on merged `c19542a` was
  directly inspected via `read_file`.

## Non-claims

`BOUNDED` is mandatory: third-party external cryptographic audit and physical HSM custody remain
open, and a complete key lifecycle does not establish honest actors, human uniqueness, consensus
safety, or correct execution.
