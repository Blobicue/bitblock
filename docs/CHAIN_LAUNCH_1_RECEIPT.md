# CHAIN-LAUNCH-1@base: complete authorized launch-closure receipt v1

This receipt resolves `PS-091:CHAIN-LAUNCH-1@base=BOUNDED` and unlocks
**`PS-058:TESTNET-1@base=READY`** (base launch-closure checkpoint eligibility; per `CE-44`,
in-place successor activation remains gated on `UPGRADE-1 -> SUCCESSOR-ACTIVATION-1`), while
keeping `SUCCESSOR-ACTIVATION-1`, `PROD-OPS-1`, `L1-CANDIDATE`, and `L1-PRODUCTION` strictly
`LOCKED`.

## Provenance

- **Merged `origin/main` subject (PR #96):** commit `4cfeca3a10f65fd3fc838da73d0b874d83a33670`
  (tree `d9419fe4a34d38cb9f371ca1fd59eabc71e10841`, parent `dc10ad8e392fa067bd1cb223b012a2523ae80144`,
  CI runs `45261626964` & `45261641249`, 10/10 jobs green across Linux, Windows, `wasm32` V8
  parity, Python vectors, generated index, and Lean 4 `--trust=0`).
- **Executable module:** [`crates/research/src/chain_launch_1.rs`](../crates/research/src/chain_launch_1.rs)
  (`wasm32`-clean, strictly behind `--features protocol-research`, zero new Cargo dependencies,
  zero edits to frozen crates, modules, or existing Lean 4 proof modules).
- **Additive Lean 4 formal proof capsule:**
  [`proof/BitblockProof/LaunchClosureRefinement.lean`](../proof/BitblockProof/LaunchClosureRefinement.lean)
  (**31** machine-checked `--trust=0` theorems, `0` warnings, `[propext, Quot.sound]` axioms only,
  raising total refinement/safety/launch `--trust=0` theorems `198 -> 229`, `252` total `theorem`
  declarations across all `10` `BitblockProof/*.lean` modules; linked via `proves` edge in
  `index/edges.jsonl`).
- **Clean-room pure-Python verifier:**
  [`tools/chain_launch_1_vectors.py`](../tools/chain_launch_1_vectors.py) (`190/190` protocol
  schemas independently vectored; `0` without vectors).
- **Machine-readable manifest:**
  [`docs/chain-launch-1-manifest-v1.json`](chain-launch-1-manifest-v1.json).
- **CI & mutation evidence:**
  [`tests/wave30_chain_launch_1.rs`](../tests/wave30_chain_launch_1.rs) and
  `python3 tools/bbmutate.py chain_launch_1` (`34/34` canonical big-endian sites KILLED, 100%).
- **Release probe:**
  [`examples/chain_launch_1_package_probe.rs`](../examples/chain_launch_1_package_probe.rs).

## Verified golden tags & checked equations

Both Rust (`LaunchClosureBundleV1`, `ChainLaunchManifestV1`, `ChainLaunchAuthorizationV1`,
`ChainLaunchPackageV1`, `preflight_launch_closure_bundle_bytes`, `verify_chain_launch_package`)
and clean-room pure Python (`tools/chain_launch_1_vectors.py`, including independent streaming
preflight and strict RFC 8032 Ed25519 verification under `SigningDomainTagV1::GovernanceAction`)
match bit-for-bit across all five golden artifacts for `bitblock.devnet.v1`:

- **`LaunchClosureBundleV1` (`bitblock/launch-closure-bundle/v1`, `|S| = 3`, `7,305` B wire):**
  `#bb.receipt.sha256.2cyxfi3goddwndgyfzvjy7y3eofikotdyry4wy7s7wzkubuzjm5a`
- **`ChainLaunchManifestV1` (`bitblock/chain-launch-manifest/v1`, `|S| = 3`, `459` B wire):**
  `#bb.policy.sha256.nxam3harptfty3tsbfqz6dtwnt44cgm7zdrd46334v4grcuzuspa`
- **`ChainLaunchAuthorizationV1` (`bitblock/chain-launch-authorization/v1`, `|S| = 3`, `361` B wire):**
  `#bb.receipt.sha256.daxw2r45k4zxrw3ypg6iupja7mcfdnyekgxrgsuhhi3gn75s3lua`
- **`ChainLaunchPackageV1` (`bitblock/chain-launch-1/package/v1`, `|S| = 3`, `9,441` B wire):**
  `#bb.receipt.sha256.2xyk6dukblpqmqfkkovaez7obpgck5j6yki5tqqa3xjcvg5ktgna`
- **`ChainLaunchPackageV1` minimal explicit-only control (`|S| = 0`):**
  `#bb.receipt.sha256.apvfatsiegwvk52s4s25y5ffnhss63flbd5ydehwukrh75yuvcoa`

### Exact `LAYER1_DESIGN_PAPER.md` §2 equations realized & formally proved

- **Canonical `|S| = 3` (`ExecutionRootCompositionV1`, `CandidateBuildInputsV1`, `CandidateReleaseReceiptV1`):**
  - `Length(E) = 7`, `supplementary_artifact_count_u32 = 3`, `internal_artifact_count_u32 = 10`,
    `external_root_count_u32 = 1`, `complete_artifact_count_u32 = 11`
  - `i2i = 22`, `i2e = 3`, `e2i = 2`, `complete_dependency_edge_count_u32 = 27`
  - `internal_canonical_preimage_bytes_u64 = 5,551`, `external_root_canonical_preimage_bytes_u64 = 1,181`
    (`Length(G)` counted once), `total_canonical_preimage_bytes_u64 = 6,732`
- **Minimal `|S| = 0` explicit-only control:**
  - `Length(E) = 7`, `supplementary_artifact_count_u32 = 0`, `internal_artifact_count_u32 = 7`,
    `external_root_count_u32 = 1`, `complete_artifact_count_u32 = 8`
  - `i2i = 13`, `i2e = 3`, `e2i = 2`, `complete_dependency_edge_count_u32 = 18`
  - `internal_canonical_preimage_bytes_u64 = 3,751`, `external_root_canonical_preimage_bytes_u64 = 1,181`,
    `total_canonical_preimage_bytes_u64 = 4,932`
- **Combined DAG strict topological ranks:** `DomainRegistryV1 = 0`, `ProfileScopeV1 = 1`,
  `ExternalGenesis X = 2` (outgoing `GenesisInitialScopeV1` & `GenesisInitialDomainRegistryV1`;
  never rank `0`), `CandidateProfileV1 = 3`, `TransitionSpecAtProfileV1 = 4` (zero direct external
  edges), `ProfileAdmissionManifestV1 = 5` (zero direct external edges), `ExecutionRootCompositionV1`
  & `CandidateBuildInputsV1 = 5`, `CandidateReleaseReceiptV1` & `GenesisRootParityRecordV1 = 6`,
  `AdmittedReleaseRecordV1 = 7`.
- **19/19 powered launch-carrier mutants (`tests/wave30_chain_launch_1.rs`):** off-by-one counts,
  second external root, independent `i2i`/`i2e`/`e2i`/total edge mutations, `u64::MAX` overflow,
  double-counted embedded registry bytes, truncated & trailing slices, rank-zero genesis, wrong
  external schema, omitted outgoing genesis ref, locator-only variant tag, outer genesis length
  mismatch, reversed `scope -> genesis` & `registry -> genesis` edges, forbidden direct
  `TransitionSpec`/`ProfileAdmission -> genesis` edges, omitted mandatory `Candidate -> genesis`
  edge, candidate-selected launch policy, 2-of-5 shortfall, cross-domain signature, 1-bit signature
  flip, and manifest ID / parent tampering all reject fail-closed.
- **Merged-head release timings (`4cfeca3`):** zero-allocation streaming preflight `0.43 us`,
  full `|S| = 3` verification (preflight + DAG + 4-way preimage decode + `RELEASE-1` 3-of-5 +
  `CHAIN-LAUNCH-1` 3-of-5 Ed25519) `556.00 us`, mutant preflight reject `0.03 us`.
- **Visual verification:** `chain_launch_1_closure_chart.png` rendered on merged `4cfeca3` was
  directly inspected via `read_file`.

## Non-claims

`BOUNDED` is mandatory and all deterministic devnet `3-of-5` launch signing seeds are explicitly
labeled **`DEVNET-TEST-KEYS-NOT-PRODUCTION-CUSTODY`**. A verified launch-closure package and Lean 4
abstract refinement model do not prove physical key custody, trusted out-of-band distribution,
uncompromised host/compiler toolchains, live network availability, or multi-process testnet
liveness (`TESTNET-1`). Per `CE-44`, unlocking `PS-058:TESTNET-1@base=READY` grants base
launch-closure checkpoint eligibility only; in-place successor activation remains gated on
`UPGRADE-1 -> SUCCESSOR-ACTIVATION-1`.
