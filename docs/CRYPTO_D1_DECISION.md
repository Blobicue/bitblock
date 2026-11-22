# CRYPTO-D1: cryptographic primitive and canonical key-encoding decision v1

This decision record resolves `PS-068:CRYPTO-D1@base=BOUNDED` and
`PS-069:CRYPTO-D1@primitive-selection=BOUNDED`. It pins the pre-genesis signature, hash,
canonical key/signature encoding, and signing-domain contract selected by explicit human decision
without claiming operational key custody (`CRYPTO-1` / `SIGNER-OPS-1`) or consensus finality.

## Provenance

- **Merged `origin/main` code & vector subject (PR #26):** commit
  `ac503dce78ffb3951ea489ddb1d2cc35f4a1f178` (tree `950e59927c847833892b9b2debfe64b276227ae3`,
  CI runs `45009961375` & `45009964425`, 10/10 jobs green).
- **Executable contract:** [`crates/research/src/crypto_d1.rs`](../crates/research/src/crypto_d1.rs)
  (`wasm32`-clean, `--features protocol-research`, exact-pinned `ed25519-dalek = "=2.2.0"` with
  `default-features = false`, `features = ["fast", "zeroize"]`).
- **Independent Python verifier:** [`tools/crypto_d1_vectors.py`](../tools/crypto_d1_vectors.py)
  (pure-Python stdlib RFC 8032 strict Edwards25519 verifier, zero Rust imports).
- **CI integration & mutation evidence:** [`tests/pre_genesis_d1_contracts.rs`](../tests/pre_genesis_d1_contracts.rs)
  and `python3 tools/bbmutate.py crypto_d1` (`10/10` canonical big-endian sites KILLED).

## Selected v1 suite (`CryptoSuiteDescriptorV1`)

- **Canonical suite tag:** `#bb.policy.sha256.jczjgonfconrys2yy7cgehmsk6mh6757m4r4piyg5ynglq2uanlq`
- **Signature suite (`SignatureSuiteIdV1::Ed25519StrictRfc8032V1 = 1`):** strict RFC 8032 Ed25519
  for both actors and validators (`32`-byte canonical public keys, `64`-byte canonical `R || S`
  signatures), strictly rejecting:
  1. all 8 small-order torsion points (identity, order-2, order-4, order-8) on both `A` and `R`;
  2. any non-canonical compressed coordinate (`y >= p = 2^255 - 19` or negative-zero `x = 0`);
  3. any malleated signature scalar (`S >= L`).
- **Hash suite (`HashSuiteIdV1::Sha256V1 = 1`):** SHA-256 across all v1 signing digests and
  protocol identifiers, matching `PROOF-0` and `COMMIT-1` with explicit `signature_suite_id` and
  `hash_suite_id` framing for future PQ or `HASH-R1` migration under `UPGRADE-1`.
- **Role-separated signing domains (`SigningDomainTagV1`):** 9 distinct wire codes
  (`ActorTransaction`, `ActorAdminRotation`, `BlockProposal`, `ConsensusVote`, `ValidatorTransport`,
  `RewardWithdrawal`, `GovernanceAction`, `EmergencyPause`, `ReleaseArtifact`) bound inside
  `SigningPreimageV1` (`#bb.receipt.sha256.blgh6lg27ohjxiv2royj2wbkii7zny2innes6coaeqpy2ckcwoeq`
  for the golden fixture) alongside `protocol_version`, `chain_id`, `genesis_id`,
  `launch_manifest_id`, `profile_id`, suite IDs, and `payload_digest`.
- **Batch verification fallback:** `verify_batch_with_strict_fallback` always isolates the exact
  failing index via individual strict verification.

## Rejected alternatives

1. **Ed25519 + BLS12-381 split suite:** rejected for v1 to keep the cryptographic TCB small,
   avoid pairing-curve Proof-of-Possession / subgroup complexity before multi-validator BFT exists,
   and keep explicit signer attribution cheap at `<= 128` validators.
2. **Post-quantum hybrid (`Ed25519 + ML-DSA-65`) from v1:** deferred to `UPGRADE-1` via versioned
   suite IDs to avoid large signatures and unproven hybrid TCB overhead before launch.
3. **`secp256k1` (ECDSA / BIP-340):** rejected in favor of deterministic strict RFC 8032 Ed25519
   with minimal pure-Rust `wasm32` footprint.

## Assumptions, falsifiers, and non-claims

- **Assumptions:** discrete-log hardness on Curve25519 and SHA-256 collision/preimage resistance.
- **Smallest falsifiers:** any accepted weak/small-order torsion point, accepted `y >= p` point,
  accepted `S + L` malleated signature, or cross-domain/cross-chain signature replay.
- **Non-claim:** selecting a sound primitive does not secure CSPRNG entropy, key custody, signer
  anti-double-sign durability, aggregation, or consensus safety (`CRYPTO-1` and `SIGNER-OPS-1`
  remain `LOCKED`).
