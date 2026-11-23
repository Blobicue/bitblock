# RELEASE-R1@candidate & @base: profile-bound candidate-build reproducibility receipt v1

This receipt resolves both `PS-088:RELEASE-R1@candidate=BOUNDED` (`PD-04` + `PD-05`) and
`PS-086:RELEASE-R1@base=BOUNDED`, completing the pre-admission Launch-Qualification Trunk
(`VALIDATOR-ECON-1`, `SIGNER-OPS-1`, `RELEASE-R1`) while strictly enforcing
`admitted_release_authority = false` and `platform_scoped_only = true` so candidate
reproducibility can never impersonate an admitted signed `RELEASE-1` artifact.

The structured machine-readable manifest is retained in
[`release-r1-candidate-manifest-v1.json`](release-r1-candidate-manifest-v1.json).

## Provenance

- **Merged `origin/main` subject (PR #90):** commit `b3f3ba2253339d906f637440c6608537096fac45`
  (tree `e92e51cdc62a697e1b6c5f3b0b589f88c54fc5d1`, parent `6d029f273d9defa839a7080c9ae5bf70170e1127`,
  CI runs `45197131990` & `45197144949`, 10/10 jobs green across Linux, Windows, `wasm32` V8
  parity, Python vectors, generated index, and Lean 4 `--trust=0`).
- **Executable module:** [`crates/research/src/release_r1.rs`](../crates/research/src/release_r1.rs)
  (`wasm32`-clean, strictly behind `--features protocol-research`, zero new Cargo dependencies,
  zero edits to frozen crates/modules).
- **Clean-room pure-Python verifier:**
  [`tools/release_r1_vectors.py`](../tools/release_r1_vectors.py) (`180/180` protocol schemas
  independently vectored; `0` without vectors) alongside
  [`tools/release_repro_framework.py`](../tools/release_repro_framework.py) (`PD-04`).
- **CI & mutation evidence:**
  [`tests/wave27_release_r1.rs`](../tests/wave27_release_r1.rs) and
  `python3 tools/bbmutate.py release_r1` (`7/7` canonical big-endian sites KILLED, 100%; `39/39`
  new sites KILLED across Waves 25–27).
- **Release probe:**
  [`examples/release_r1_repro_probe.rs`](../examples/release_r1_repro_probe.rs).

## Verified golden tags & PD-04 / PD-05 bindings

Both Rust (`CandidateBuildInputsV1`, `BuilderAttestationV1`, `CandidateReleaseReceiptV1`,
`verify_candidate_reproducibility`) and clean-room pure Python (`tools/release_r1_vectors.py`)
match bit-for-bit across both golden artifacts:

- **`CandidateBuildInputsV1` (`bitblock/release-r1/candidate-build-inputs/v1`, `363` bytes):**
  `#bb.policy.sha256.2wlogxgqgwzblmcwtpgvmai2h3s5vagoagxgywtjqmzcwxioryaq` (binds `PD-05`
  `PROFILE-D1` `profile_id`, `GENESIS-D1` `genesis_id`, pre-ID `domain_registry_digest`, `PD-04`
  `RELEASE-R1@framework` `framework_manifest_sha256`
  `feec1255b6dc45abda2b17ee97e4bd5ecc448d6795f38de5d7897d0478c38d08`, `rust-toolchain.toml` `1.96.0`
  minimal SHA-256 `0d7b2f6589d3a2d9ea22b782a107ddafc0da08bdca08e3524b0cf4db12c12da9`,
  `proof/lean-toolchain` `leanprover/lean4:v4.33.0` SHA-256
  `302cd63c54178885b89e669f33b38f12f4dd7ae7e5cac537b3203e3768d8fb2b`, `Cargo.lock` (`220` packages,
  `11` exact `=` direct pins) SHA-256
  `4d5b8b9d9932e863c468df0d0db5381a9400b29c358e77a10da999a83a88657e`, deterministic SBOM SHA-256
  `aaaf322ba8019d810e0853e74798d6ca494a3838eb5b27a7136930c3b8a8a23b`, feature profile digest,
  deterministic build-config digest, and explicit `CandidatePlatformTargetV1`).
- **`CandidateReleaseReceiptV1` (`bitblock/release-r1/candidate-build-receipt/v1`, `217` bytes):**
  `#bb.receipt.sha256.zyk4zg44x3q46h36xnnpghcklfndgulmx7pmuspm3d546vcblnfa` (commits
  `candidate_build_inputs_digest`, `profile_id`, `genesis_id`, `framework_manifest_sha256`,
  dual-builder native & `wasm32` hashes/lengths, `independent_builder_count >= 2`,
  `builders_bit_identical = true`, `platform_scoped_only = true`, and
  `admitted_release_authority = false`).
- **8/8 mutation, divergence, and non-admission negative controls (`tests/wave27_release_r1.rs`):**
  source-tree mutation, feature/config mutation, `PROFILE-D1` binding mismatch, `PD-04` framework
  mismatch, `< 2` or duplicate `builder_id`, 1-bit builder artifact divergence, wire-tampered
  `admitted_release_authority = true`, and wire-tampered `platform_scoped_only = false` each
  strictly change candidate identity or reject fail-closed.

## Real isolated dual-target clean-build bit-exact reproducibility (`b3f3ba2`)

Executed on merged `origin/main` commit `b3f3ba2253339d906f637440c6608537096fac45` across two
completely isolated clean Cargo target directories (`target/repro_a` and `target/repro_b`, both
deleted cleanly after measurement) using `--locked` and explicit `--remap-path-prefix`:

| Target artifact | Clean build A == Clean build B (`SHA-256`) | Exact byte length | Parity verdict |
| --- | --- | ---: | --- |
| Native `linux-aarch64-elf` (`--release --locked --bin bitblock`) | `292b8f420f41d181ba818971c49ac0cb640c0ca853339c4b592f3bd410b8249d` | `3,219,304` | **BIT-EXACT PASS** (platform-scoped) |
| WebAssembly `wasm32-unknown-unknown` (`+simd128`, `panic=abort`, `-p bitblock-wasm`) | `b24d8f37cd13d2c7e64a3a9f9edd050e15d4fc1613c973adf92a46469e0c615a` | `1,831,176` | **BIT-EXACT PASS** + Node 22 V8 parity (`abi=17`, `39/39` constants, `37` check groups in `644 ms`) |

- **Merged-head release probe timings (`b3f3ba2`):** `PD-05` `PROFILE-D1` + dual-builder
  reproducibility verify `13.94 us`, canonical inputs & candidate receipt encode/decode/tag
  `3.18 us`, builder divergence & `admitted_release_authority = true` fail-closed rejection
  `11.95 us`.
- **Visual verification:** `release_r1_repro_chart.png` rendered on merged `b3f3ba2` was directly
  inspected via `read_file`.

## Non-claims

This attestation is strictly **`CANDIDATE-NOT-ADMITTED` (`BOUNDED`)**. Reproducibility proves
candidate artifact correspondence on the declared platform scope only; it does **not** constitute
`PROFILE-1` conditional-closure admission, signed `RELEASE-1` authorization, key custody, cross-OS
native binary identity (Linux ELF vs Windows PE remain explicitly platform-scoped), or a proof
that the Rust compiler, Lean 4 toolchain, or upstream dependencies are free of vulnerabilities.
