# `COMMIT-1` authenticated local state implementation receipt

This record pins the `COMMIT-1` (`PS-020:COMMIT-1@base`) implementation and verification receipt on
`origin/main` commit `cb2b7751bfb7c9763120f58568ebe34d6cb7d9a0` (tree
`63cdc7c1f1a3f18a0e69a530283a9945a95593d2`, PR #16 CI runs `44982516645` and `44982520445`, 10/10
jobs green across Windows/Linux Rust, `wasm32` `+simd128` V8 parity, Python vector tools,
generated index, and Lean 4 `--trust=0` proof capsules).

## Scope and architecture

`COMMIT-1` instantiates the Candidate A fixed-key binary Merkle Patricia Trie selected in
[`COMMIT_R1_DECISION.md`](COMMIT_R1_DECISION.md) as a separately typed research-gated sidecar
(`crates/research/src/commit_state.rs`, re-exported via `bitblock::commit_state`) without touching
`crates/core/src/world.rs`, `crates/core/src/render.rs`, or legacy `world_tag` (`TagKind::World`).

### Activated local domains and schemas (`9` new schemas, `87/87` protocol schemas vectored)

| Schema | Outer kind | Purpose & golden tag (Rust == independent Python) |
| --- | --- | --- |
| `bitblock/commit-1/proof-policy/v1` | `Policy` | `ProofPolicyDescriptorV1` (`WORLD_DEFAULT`: `#bb.policy.sha256.vkvqipvpzty76ylzflzkwdyuwq3u2ikjwumbtu52e45u5a6jbo3q`, `ACTIVE_TOKEN_DEFAULT`: `#bb.policy.sha256.3rof4k6xgp5dfl5mc3fj63cuetrjetcxrucd6qhhvry3xvertw5q`, `EPISODE_DEFAULT`: `#bb.policy.sha256.qdewuvwl266u2dkfpowshnv3n3ihd52i2j366rivbph5ef4zh5ea`) |
| `bitblock/commit-1/operational-limits/v1` | `Policy` | `CommitmentOperationalLimitsDescriptorV1` (`LOCAL_DEFAULT`: `#bb.policy.sha256.u72lxb77ydfzak3eamjl4v53km4yb7nfkobb5dtmmzvoqicxsaua`) |
| `bitblock/commit-1/commitment-profile/v1` | `Policy` | `CommitmentProfileDescriptorV1` (`world_v1`: `#bb.policy.sha256.pta7j7uqpw2rtx2wmez3zobxicou5updtavrhc6kxvewbaexta6a`, `active_token_v1`: `#bb.policy.sha256.m42tb7fel3fz6ktrir6huv6xq3uy67egs74xd22n2vlfsim4dbka`, `episode_v1`: `#bb.policy.sha256.3qk2vnttiwowmjhklrckm2p2dqwv6eiqozwex2wqnvk4k2uh7x2q`) |
| `bitblock/commit-1/domain-registry/v1` | `Policy` | `DomainRegistryV1::local_v1()` activating World, ActiveToken, and Episode while recording Authorization, Nonce, and AssetState as typed inactive sentinels (`#bb.policy.sha256.g7mygkuhz6daeuzqztg5bvftvhl7kutqmrvx7hsj53s22fzhpzlq`) |
| `bitblock/commit-1/world-root/v1` | `World` | `WorldRootV1`: 192-bit `CoordinateKeyV1` -> `OnV1` (`[0x01]`), including permanent origin bedrock (bedrock-only: `#bb.world.sha256.tlnd3xyl3cp3aa6wsli3jd2uwx35srbomu4uxoarmlpgpffmxdyq`) |
| `bitblock/commit-1/active-token-root/v1` | `Receipt` | `ActiveTokenRootV1`: 192-bit `CoordinateKeyV1` -> 32-byte `MintIdV1`, strictly excluding origin bedrock (`2`-coord vector: `#bb.receipt.sha256.6uqfwbjqfpfllv3wsoxlgy5adzqhc2ltwlfkbnb347p5wygr56eq`) |
| `bitblock/commit-1/episode-root/v1` | `Receipt` | `EpisodeRootV1`: 256-bit `EventIdV1` -> canonical 57B active / 89B closed `EpisodeRecordV1` (`3`-entry vector including depth-256 twins: `#bb.receipt.sha256.cmyepgr23n5evmmnakckviwf7j6xvqcgquv4lh7zmo6fnzbwq4da`) |
| `bitblock/commit-1/patricia-proof/v1` | `Receipt` | `TypedPatriciaProofV1` + `ExpectedProofContextV1` (`verify_lookup` / `verify_claim`, bounded policy enforcement, fail-closed `tighten` refusing widening) |
| `bitblock/commit-1/commitment-set/v1` | `Receipt` | Immutable `CommitmentSetV1` binding `LocalTransitionV1` subject, `ledger_head_id`, `required_domain_set`, all active `root_entries`, and predecessor lineage (genesis vector: `#bb.receipt.sha256.mrlbnlwnwjezmgrj5nqpngbcvng2oge4tvs2kvuzfi4beumci2cq`) |

### Publication, crash recovery, and GC invariants

- **Incremental vs. full-rebuild & independent projection parity:** `LocalAuthenticatedSidecar`
  advances all three tries incrementally per committed receipt and logical episode prune
  (`advance_for_logical_episode_prune`), verifying exact equality with cold rebuilds on every
  transition. `verify_independent_projection` replays `History` from scratch via
  `verify_in_memory_history_with_limits` and checks `OccupancyEpisodeLedger::verify_world_bijection`
  plus exact root and `ledger_head_id` equality.
- **Content-addressed storage & crash recovery:** `MemoryCommitmentStore` (portable across native
  and `wasm32`) and `DurableCommitmentStore` / `DurableAuthenticatedLedger` (native filesystem with
  Unix `flock(LOCK_EX | LOCK_NB)` / Windows `share_mode(0)`) key every node by
  `(commitment_profile_id, domain_tag, node_hash)`, canonically decode and rehash every node read,
  and enforce the staged publication sequence: node staging -> complete reachability closure ->
  checksummed `ProvisionalPinRecordV1` -> immutable `CommitmentSetV1` manifest -> atomic checksummed
  `HeadPointerRecordV1` CAS -> pin promotion.
- **Write-barrier GC:** `begin_gc_mark` captures `(pin_epoch, canonical_pin_set)` and traces
  reachable nodes within `CommitmentOperationalLimitsDescriptorV1` bounds; any pin/publish
  increments `pin_epoch` so `finish_gc_sweep` aborts safely (`GcSweepOutcome::AbortedEpochChanged`)
  with zero nodes reclaimed if pins moved during mark.

## Verification commands and receipts

- `python3 -B tools/commit_state_vectors.py`: PASS (`9` schemas, `19` golden cross-parity assertions
  matching Rust bit-for-bit, `4` negative controls).
- `cargo test --features protocol-research --test commit_1_authenticated_state`: PASS (`5/5`
  integration tests covering insertion-order independence, depth-192 & depth-256 zero-bit-suffix
  boundaries, adversarial proof/domain/policy/codec matrix, logical pruning, GC write-barrier race,
  failed CAS, exclusive locking, on-disk corruption fail-closed, and all 5 filesystem `CrashPoint`s).
- `python3 tools/bbmutate.py commit_state`: PASS (`61/61` big-endian mutation sites KILLED, `0`
  survived).
- `python3 -B tools/bbverify.py full --force`: PASS (`18/18` checks, `709/709` Linux tests, `41`
  bare Python validators / `39` independent vector suites).
- `cargo clippy --target wasm32-unknown-unknown -p bitblock-research --lib -- -D warnings` &
  `node tools/wasm_parity_check.mjs`: PASS (`abi=17`, `39/39` constants).
- GitHub Actions PR #16 (`44982516645` & `44982520445`): 10/10 jobs green on Windows (`rust (windows-latest)`)
  and Linux (`rust (ubuntu-latest)`, `python vector tools`, `generated index`, `lean proof capsule`).

## Explicit non-claims

- `COMMIT-1` proves key/value statements relative to a caller-supplied typed root; it does **not**
  establish canonical-chain selection, consensus finality, data availability, honest execution,
  actor authorization, or economic ownership.
- Legacy `world_tag` (`bitblock/world/chunk-merkle-sha256/v2`) remains unchanged for default builds
  and historical receipt verification.
- Physical sudden-power hardware durability testing and machine-checked Rust-to-model refinement
  of `COMMIT-1` remain separate boundaries (`STORAGE-1@research-journals=BOUNDED` and newly
  unlocked `REFINE-1@commitment=READY`).
