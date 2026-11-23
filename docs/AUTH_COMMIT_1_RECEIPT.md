# AUTH-COMMIT-1: authenticated authorization and nonce state receipt v1

This receipt resolves `PS-029:AUTH-COMMIT-1@base=BOUNDED` (keeping execution composition downstream
in `EXECUTOR-1` and `EXECUTION-ROOT-1`) and unlocks **`PS-038:ASSET-1@base=READY`** and
**`PS-081:GOVERNANCE-1@base=READY`** (while keeping `ECONOMIC-STATE-1`, `VALIDATOR-SCHEDULE-1`,
`EXECUTOR-1`, and `EXECUTION-ROOT-1` honestly `LOCKED`).

## Provenance

- **Merged `origin/main` subject (PR #56):** commit `6bac6d5a9c226dbefbbcd17da8770d2b793155a4`
  (tree `2999304e7aaa83c2a00ff69fede9222d4d16b1a0`, parent `e89adbe7010dfd267f0507ed49a09d1d88ab1be8`,
  CI runs `45108142656` & `45108147347`, 10/10 jobs green).
- **Executable module:** [`crates/research/src/auth_commit_1.rs`](../crates/research/src/auth_commit_1.rs)
  (`wasm32`-clean, strictly behind `--features protocol-research`; zero changes to `crates/core/**`,
  `crates/research/src/commit_state.rs` (`COMMIT-1` / Lean 4 `REFINE-1@commitment`), default binary,
  `world_tag`, `crates/wasm/www/*`, or `Cargo.lock`).
- **Independent Python reference suite:** [`tools/auth_commit_1_vectors.py`](../tools/auth_commit_1_vectors.py)
  (`146/146` protocol schemas independently vectored; exports reusable `DomainPatriciaPy` for
  downstream Layer 1 state-producer suites).
- **CI & mutation evidence:** [`tests/wave13_auth_commit_1.rs`](../tests/wave13_auth_commit_1.rs)
  and `python3 tools/bbmutate.py auth_commit_1` (`9/9` canonical big-endian sites KILLED, 100%).
- **Release probe:** [`examples/auth_commit_1_probe.rs`](../examples/auth_commit_1_probe.rs).

## Verified golden tags & normative invariants

- **Shared 256-bit domain-separated Candidate A binary Merkle Patricia Trie (`DomainPatriciaTrieV1`
  / `DomainPatriciaProofV1`, `bitblock/auth-commit-1/domain-patricia-proof/v1`):**
  - `DomainIdV1` separates all 7 Layer 1 state domains (`Authorization = 1`, `Nonce = 2`,
    `NativeBalance = 3`, `EpisodeOwnership = 4`, `GovernanceState = 5`, `EconomicStake = 6`,
    `ValidatorSchedule = 7`) inside every empty, leaf, extension, and branch node hash, causing
    `7/7` cross-domain proof attempts (including against `COMMIT-1` occupancy/episode roots) to
    fail closed.
  - Canonical path-copy normalization matches cold `from_entries` full rebuild with `0` root
    mismatches across 256 actors + 200 updates (`6.0 us` incremental upsert vs `565.9 us` 256-actor
    cold rebuild, `0.9 us` prove, `5.3 us` verify).
  - Inclusion and non-membership (`Empty`, `LeafMismatch`, `DivergentExtension`) proofs enforce
    `BOUNDS-1`'s `<= 256` depth and `<= 12 KiB` encoded ceiling (`389 B` inclusion / `308 B` absence
    on 256 actors).
- **Separately typed authenticated roots (`AuthCommitRootsV1`,
  `bitblock/auth-commit-1/auth-nonce-roots/v1`):**
  - Genesis paired roots tag:
    `#bb.receipt.sha256.kbi2xxuzs6zay5pjvotpyk3edmn7dchwlh22bmcbpwfde3cyc55a`.
  - Post-tx paired roots tag:
    `#bb.receipt.sha256.5mq4zzp7sm6ghxnrhiww54uu2kmj56qgkftkjkoo2eosjf5hilja`.
  - Authorization inclusion proof tag:
    `#bb.receipt.sha256.nu2trkasizgtdp4mnyoom77tr2nqgylmuazaszn4ra3lwlrwvjiq`.
  - Nonce non-membership proof tag:
    `#bb.receipt.sha256.daucn3zib6opfx5dmpny25vqeift7rvitvxgsylagmhq3utgwegq`.
- **Atomic staged rollback:** stale nonces, invalid signatures, revocations, and early recovery
  completions roll back completely, leaving both `AuthorizationRootV1` and `NonceRootV1`
  byte-unchanged.
- **Visual verification:** `auth_commit_1_patricia_chart.png` rendered on merged `6bac6d5` was
  directly inspected via `read_file`.

## Non-claims

`BOUNDED` is mandatory: an authenticated authorization map does not prove the key holder is one
human or that a submitted action was included, executed, or finalized by consensus.
