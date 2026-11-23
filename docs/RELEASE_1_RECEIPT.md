# RELEASE-1@base & @admitted: admitted-profile signed release receipt v1

This receipt resolves both `PS-089:RELEASE-1@base=BOUNDED` and `PS-090:RELEASE-1@admitted=BOUNDED`,
unlocking **`PS-091:CHAIN-LAUNCH-1@base=READY`** while keeping `TESTNET-1`,
`SUCCESSOR-ACTIVATION-1`, and `PROD-OPS-1` strictly `LOCKED` on their remaining prerequisites.

## Provenance

- **Merged `origin/main` subject (PR #94):** commit `5f682e1f6a046b5c6ec8f16e4c811ecf1000dc1d`
  (tree `edad5b0205ed989bda30146e0dc23bf15bfef967`, parent `39766e6d2988242e407a6bc1794daebb384ed7a9`,
  CI runs `45255049300` & `45255065215`, 10/10 jobs green across Linux, Windows, `wasm32` V8
  parity, Python vectors, generated index, and Lean 4 `--trust=0`).
- **Executable module:** [`crates/research/src/release_1.rs`](../crates/research/src/release_1.rs)
  (`wasm32`-clean, strictly behind `--features protocol-research`, zero new Cargo dependencies,
  zero edits to frozen crates or modules).
- **Clean-room pure-Python verifier:**
  [`tools/release_1_vectors.py`](../tools/release_1_vectors.py) (`186/186` protocol schemas
  independently vectored; `0` without vectors).
- **CI & mutation evidence:**
  [`tests/wave29_release_1.rs`](../tests/wave29_release_1.rs) and
  `python3 tools/bbmutate.py release_1` (`11/11` canonical big-endian sites KILLED, 100%).
- **Release probe:**
  [`examples/release_1_signing_probe.rs`](../examples/release_1_signing_probe.rs).

## Verified golden tags & normative invariants

Both Rust (`AdmittedReleaseCoreV1`, `ReleaseSignDocV1`, `ReleaseAuthorizationSignDocEnvelopeV1`,
`ReleaseAuthorizationV1`, `AdmittedReleaseRecordV1`, `TrustedReleaseAuthorityPolicyV1`,
`resolve_trusted_release_authority`, `verify_admitted_release_record`) and clean-room pure Python
(`tools/release_1_vectors.py`, including pure-Python strict RFC 8032 Ed25519 verification) match
bit-for-bit across all four golden artifacts for `bitblock.devnet.v1`:

- **`AdmittedReleaseCoreV1` (`bitblock/admitted-release-core/v1`, `438` bytes):**
  `#bb.policy.sha256.ounjvzefp2yesle4hlbmlo5m6elwvl4uk76v4ajudelek2bgjrga` (strictly
  authorization-free and excludes `chain_launch_manifest_id`; binds `PROFILE-1`
  `profile_admission_manifest_id`, `PROFILE-D1` `profile_id`, `TRANSITION-SPEC-1`
  `transition_spec_at_profile_id`, `GENESIS-D1` `genesis_id`, `GENESIS-ROOT-PARITY-1`
  `parity_record_id`, `RELEASE-R1` inputs & receipt digests, platform-scoped native ELF &
  `wasm32` `+simd128` artifact digests/lengths, `GenesisReleaseAuthorityV1` context ref,
  projected `release_authority_policy_id`, and `VulnerabilityWithdrawalStatusV1::Active`).
- **`ReleaseSignDocV1` (`bitblock/release-sign-doc/v1`, `246` bytes):**
  `#bb.policy.sha256.ehdsiakhtw2nbziey26vwmklsii2kjxbsty3c4prpavmcj77noea` (binds derived
  `admitted_release_core_id`, `release_authority_context_ref`,
  `GenesisReleaseHistoricalAuthorityRefV1`, and `release_authority_policy_id`).
- **`ReleaseAuthorizationSignDocEnvelopeV1` (`bitblock/release-authorization-sign-doc/v1`, `250` bytes):**
  `#bb.policy.sha256.nbu5622ibijedtdqniy6taoo7xsauu7kuf66xvpwgquqmx2j5yea` (exact detached
  `LengthFrame(Canonical(ReleaseSignDocV1))` signed under `CRYPTO-D1`
  `SigningDomainTagV1::ReleaseArtifact` with `launch_manifest_id = [0; 32]`).
- **`AdmittedReleaseRecordV1` (`bitblock/admitted-release-record/v1`, `1,079` bytes):**
  `#bb.receipt.sha256.qui5yzjsvwkfvbh7ihzva3swlvfgujvvxw3x6hxbmutto3udqlba` (`admitted_release_record_id`
  derived strictly after `3-of-5` Ed25519 threshold authorization passes under
  `ResolveTrustedReleaseAuthorityV1`).
- **10/10 forgery, withdrawal & successor-authority controls (`tests/wave29_release_1.rs`):**
  wrong `RELEASE-R1` artifact digest, stale/mismatched `PROFILE-1` admission manifest, wrong
  platform target, `< 3` signature shortfall & duplicate signer replay, unauthorized 6th key,
  1-bit scalar `S` & wire `R` signature flips, cross-domain `BlockProposal` signature,
  `WithdrawnSecurityAdvisory` / `Superseded` release block, `G_0` authority-policy mismatch, and
  `AcceptedParentReleaseAuthorityV1` purpose swap (`SuccessorActivationAuthority`),
  replay-archive certificate substitution, and missing live consensus-native parent certificate
  all fail closed.
- **Merged-head release timings (`5f682e1`):** 3-of-5 Ed25519 sign + self-verify `242.27 us`,
  wire decode + upstream check + 3-of-5 verify `122.38 us`, withdrawn & 2-of-5 shortfall reject
  `6.03 us`.
- **Visual verification:** `release_1_signed_release_chart.png` rendered on merged `5f682e1` was
  directly inspected via `read_file`.

## Non-claims

`BOUNDED` is mandatory and all deterministic devnet `3-of-5` signing seeds are explicitly labeled
**`DEVNET-TEST-KEYS-NOT-PRODUCTION-CUSTODY`**. A reproducible signed admitted release does not
prove source correctness, compiler/toolchain integrity, operator custody, availability, or
consensus safety.
