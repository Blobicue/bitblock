# RELEASE-R1@framework: candidate-build reproducibility framework v1

This record resolves `PS-087:RELEASE-R1@framework` as **BOUNDED** (keeping `PS-086:RELEASE-R1@base=READY`,
`PS-088:RELEASE-R1@candidate=LOCKED` pending `PROFILE-D1`, and `PS-089/PS-090:RELEASE-1=LOCKED`
pending `PROFILE-1` and `CRYPTO-1`). It pins the reusable dependency-lock, toolchain, SBOM,
proof-capsule, and artifact-integrity framework strictly from external pinned Rust/tool/proof
inputs without claiming candidate profile admission or signed release authority.

The machine-readable framework receipt is retained in
[`release-r1-framework-manifest-v1.json`](release-r1-framework-manifest-v1.json).

## Provenance and verifier

- **Merged `origin/main` subject:** commit `58cefcb9e9b4ae24cb90861571489560266d6b23`
  (tree `2166e50bcce90d14ea80abec8b2190dd9ba6b93c`, PR #14, CI runs `44978821303` & `44978824816`,
  10/10 jobs green).
- **Executable verifier:** `tools/release_repro_framework.py` (stdlib-only Python bare validator
  executed automatically by `tools/bbverify.py full` and CI `python vector tools (ubuntu-latest)`;
  supports `--emit-manifest` for deterministic JSON attestation).
- **Framework manifest digest (`framework_manifest_sha256`):**
  `3177a72ddef44016a9247f7b634a04083f813899f4ba9d2165aa75192cf86a56`.

## Pinned external build inputs

| Input surface | Pinning rule | Verified value at `58cefcb` |
| --- | --- | --- |
| Rust toolchain | `rust-toolchain.toml` exact channel + minimal profile | `1.96.0` (`profile = "minimal"`, SHA-256 `0d7b2f6589d3a2d9…`) |
| Direct external crates | Exact `=x.y.z` version pins across all 4 Cargo manifests | 10 pinned entries (`sha2 =0.10.9`, `winit =0.30.13`, `softbuffer =0.4.8`, `windows-sys =0.61.2`, `libc =0.2.189`) |
| Complete dependency SBOM | Canonical sorted `(name, version, source, checksum)` JSON derived from `Cargo.lock` | 212 package records; `Cargo.lock` SHA-256 `6ede64eba06c0ca1…`; SBOM SHA-256 `ec8e25d4d782dd89…` |
| Lean 4 proof toolchain & manifest | `proof/lean-toolchain` + `proof/ladderlang-transition-v1/source-manifest.tsv` | `leanprover/lean4:v4.33.0`; both LadderLang source files SHA-256 verified |
| Committed WebAssembly artifact | `crates/wasm/www/bitblock_wasm.wasm` + embedded `WASM_B64` parity | SHA-256 `bf1bf37bd8f1a3ea…` exact match across `play.html` and `bitblock-lab.html` |

## Reproducible build and vulnerability response procedure

1. **Deterministic native & WASM build commands:**
   - Default native release binary: `cargo build --release --locked --bin bitblock`
   - Research-enabled verification build: `cargo test --workspace --locked --features protocol-research`
   - WebAssembly `+simd128` release artifact:
     `RUSTFLAGS="-C target-feature=+simd128" CARGO_PROFILE_RELEASE_PANIC=abort cargo build --release --locked --target wasm32-unknown-unknown -p bitblock-wasm`
     followed by `node tools/wasm_parity_check.mjs <path> --check-embedded`.
2. **Powered negative controls (5/5 passing in `tools/release_repro_framework.py`):** Mutating the
   Rust toolchain channel, `Cargo.lock` SBOM digest, feature profile, Lean source manifest digest,
   or committed binary/WASM digest each strictly changes `framework_manifest_sha256` and rejects
   mismatched candidate identity.
3. **Vulnerability & compromised-toolchain response:** Any disclosed critical CVE in a pinned crate
   (`sha2`, `winit`, `softbuffer`, `windows-sys`, `libc`) or compiler/toolchain compromise requires:
   (a) halting candidate promotion immediately, (b) rotating the exact `=` pin and `Cargo.lock` or
   `rust-toolchain.toml` in a dedicated reviewed PR, (c) re-running the complete 5-job CI gauntlet
   (Windows, Linux, `wasm32` V8 parity, Python vectors, generated index, Lean `--trust=0`), and
   (d) emitting a fresh candidate manifest digest (never mutating a prior artifact in place).

## Non-claims

This reusable framework is labeled **`CANDIDATE-FRAMEWORK-ONLY`**. It does **not** constitute a
profile-bound `RELEASE-R1@candidate` attestation (locked until `PROFILE-D1`), a signed admitted
`RELEASE-1` artifact (locked until `PROFILE-1` and `CRYPTO-1`), cross-OS bit-identical native
executables (Windows PE vs Linux ELF remain explicitly platform-scoped), or a proof that the Rust
compiler or upstream dependencies are free of vulnerabilities.
