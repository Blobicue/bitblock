//! `RELEASE-R1@candidate` (`PS-088`) & `RELEASE-R1@base` (`PS-086`): profile-bound
//! candidate-build reproducibility carrier, dual-builder bit-exact attestation verifier, and
//! non-admission boundary enforcement.
//!
//! Strictly behind `--features protocol-research` and 100% `wasm32`-clean with zero additional
//! Cargo dependencies.
//!
//! # Normative Boundaries (`CAPABILITY_TREE.md` / `LAYER1_DESIGN_PAPER.md`)
//! - **Candidate ≠ Admitted Release:** `RELEASE-R1` binds reproducible candidate-build inputs
//!   and dual-builder digests to a concrete `PROFILE-D1` candidate `profile_id` and `GENESIS-D1`
//!   `genesis_id`, while strictly enforcing `admitted_release_authority = false` fail-closed.
//!   Signing an admitted release belongs exclusively to `RELEASE-1` after `PROFILE-1` passes.
//! - **Explicit platform scoping:** Native ELF/PE binaries are explicitly platform-scoped
//!   (`platform_scoped_only = true`); WebAssembly `+simd128` artifacts are target-independent.
//! - **Fail-closed mutation sensitivity:** Any mutation to `source_tree_digest`,
//!   `rust_toolchain_sha256`, `lean_toolchain_sha256`, `cargo_lock_sha256`, `sbom_sha256`,
//!   `feature_profile_digest`, `build_config_digest`, `profile_id`, `genesis_id`, or either
//!   builder artifact digest strictly changes the candidate identity or rejects verification.

use std::fmt;

use sha2::{Digest, Sha256};

use crate::profile_d1::BoundProfileManifestV1;
use crate::protocol::{ContentTag, TagKind, hash_canonical};

/// Canonical schema version for `RELEASE-R1` candidate-build carriers.
pub const RELEASE_R1_VERSION: u16 = 1;

/// Canonical schema string for [`CandidateBuildInputsV1`].
pub const RELEASE_R1_BUILD_INPUTS_SCHEMA: &[u8] = b"bitblock/release-r1/candidate-build-inputs/v1";

/// Canonical schema string for [`CandidateReleaseReceiptV1`].
pub const RELEASE_R1_CANDIDATE_RECEIPT_SCHEMA: &[u8] =
    b"bitblock/release-r1/candidate-build-receipt/v1";

const RELR1_FEATURE_PREFIX: &[u8] = b"bb.relr1.feature_profile.v1";
const RELR1_CONFIG_PREFIX: &[u8] = b"bb.relr1.build_config.v1";
const RELR1_SOURCE_PREFIX: &[u8] = b"bb.relr1.source_tree.v1";

/// Failure modes for `RELEASE-R1` candidate-build reproducibility verification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReleaseR1Error {
    /// Malformed or non-canonical wire encoding.
    InvalidWire(&'static str),
    /// Candidate build inputs disagreed with the governing `PROFILE-D1` [`BoundProfileManifestV1`].
    ProfileBindingMismatch(&'static str),
    /// Insufficient independent builders (`< 2`) or builder artifacts diverged.
    ReproducibilityDivergence(&'static str),
    /// Attempted to claim admitted `RELEASE-1` authority inside a `RELEASE-R1` candidate receipt.
    AdmittedReleaseAuthorityForbidden,
    /// Candidate build failed to label native artifacts as explicitly platform-scoped.
    PlatformScopingRequired,
    /// Zero-length or all-zero required cryptographic digest or artifact.
    EmptyDigestOrArtifact(&'static str),
}

impl fmt::Display for ReleaseR1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ReleaseR1Error {}

/// Explicitly platform-scoped target discriminant for candidate reproducibility receipts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum CandidatePlatformTargetV1 {
    /// Linux AArch64 ELF native binary + `wasm32-unknown-unknown` `+simd128` bundle.
    LinuxAarch64Elf = 1,
    /// Linux x86_64 ELF native binary + `wasm32-unknown-unknown` `+simd128` bundle.
    LinuxX86_64Elf = 2,
    /// Windows x86_64 PE native binary + `wasm32-unknown-unknown` `+simd128` bundle.
    WindowsX86_64Pe = 3,
    /// Standalone target-independent `wasm32-unknown-unknown` `+simd128` artifact only.
    Wasm32Simd128 = 4,
}

impl CandidatePlatformTargetV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte platform target discriminant.
    pub const fn from_code(code: u8) -> Result<Self, ReleaseR1Error> {
        match code {
            1 => Ok(Self::LinuxAarch64Elf),
            2 => Ok(Self::LinuxX86_64Elf),
            3 => Ok(Self::WindowsX86_64Pe),
            4 => Ok(Self::Wasm32Simd128),
            _ => Err(ReleaseR1Error::InvalidWire(
                "unknown CandidatePlatformTargetV1 code",
            )),
        }
    }
}

/// Computes the domain-separated feature-profile digest (`bb.relr1.feature_profile.v1`).
pub fn digest_feature_profile(label: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(RELR1_FEATURE_PREFIX);
    h.update(label.as_bytes());
    h.finalize().into()
}

/// Computes the domain-separated build-configuration digest (`bb.relr1.build_config.v1`).
pub fn digest_build_config(label: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(RELR1_CONFIG_PREFIX);
    h.update(label.as_bytes());
    h.finalize().into()
}

/// Computes the domain-separated source-tree digest (`bb.relr1.source_tree.v1`).
pub fn digest_source_label(label: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(RELR1_SOURCE_PREFIX);
    h.update(label.as_bytes());
    h.finalize().into()
}

/// Canonical profile-bound candidate-build input descriptor
/// (`bitblock/release-r1/candidate-build-inputs/v1`, `363` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CandidateBuildInputsV1 {
    /// Schema version (`1`).
    pub schema_version: u16,
    /// Governing `PROFILE-D1` candidate `profile_id` (`PD-05`).
    pub profile_id: [u8; 32],
    /// Immutable `GENESIS-D1` `genesis_id`.
    pub genesis_id: [u8; 32],
    /// Pre-ID `DomainRegistryV1` digest projected from `PROFILE-D1`.
    pub domain_registry_digest: [u8; 32],
    /// `RELEASE-R1@framework` `framework_manifest_sha256` (`PD-04`).
    pub framework_manifest_sha256: [u8; 32],
    /// Source-tree / Git-tree digest of the candidate checkout.
    pub source_tree_digest: [u8; 32],
    /// `SHA-256` of `rust-toolchain.toml`.
    pub rust_toolchain_sha256: [u8; 32],
    /// `SHA-256` of `proof/lean-toolchain`.
    pub lean_toolchain_sha256: [u8; 32],
    /// `SHA-256` of `Cargo.lock`.
    pub cargo_lock_sha256: [u8; 32],
    /// Deterministic sorted SBOM `SHA-256` derived from `Cargo.lock`.
    pub sbom_sha256: [u8; 32],
    /// Exact `[[package]]` count in `Cargo.lock`.
    pub cargo_lock_package_count: u32,
    /// Exact direct `=` external crate pin count across workspace manifests.
    pub direct_pin_count: u32,
    /// Domain-separated feature-profile digest.
    pub feature_profile_digest: [u8; 32],
    /// Domain-separated deterministic build-flags & config digest.
    pub build_config_digest: [u8; 32],
    /// Explicitly declared platform scope.
    pub platform_target: CandidatePlatformTargetV1,
}

impl CandidateBuildInputsV1 {
    /// Fixed canonical big-endian wire length (`363` bytes).
    pub const WIRE_LEN: usize = 2 + 32 * 11 + 4 + 4 + 1;

    /// Validates non-zero digests, package/pin counts, and schema version fail-closed.
    pub fn validate(&self) -> Result<(), ReleaseR1Error> {
        if self.schema_version != RELEASE_R1_VERSION {
            return Err(ReleaseR1Error::InvalidWire("schema_version must be 1"));
        }
        let required = [
            (self.profile_id, "profile_id"),
            (self.genesis_id, "genesis_id"),
            (self.domain_registry_digest, "domain_registry_digest"),
            (self.framework_manifest_sha256, "framework_manifest_sha256"),
            (self.source_tree_digest, "source_tree_digest"),
            (self.rust_toolchain_sha256, "rust_toolchain_sha256"),
            (self.lean_toolchain_sha256, "lean_toolchain_sha256"),
            (self.cargo_lock_sha256, "cargo_lock_sha256"),
            (self.sbom_sha256, "sbom_sha256"),
            (self.feature_profile_digest, "feature_profile_digest"),
            (self.build_config_digest, "build_config_digest"),
        ];
        for (dig, name) in required {
            if dig == [0u8; 32] {
                return Err(ReleaseR1Error::EmptyDigestOrArtifact(name));
            }
        }
        if self.cargo_lock_package_count == 0 || self.direct_pin_count == 0 {
            return Err(ReleaseR1Error::EmptyDigestOrArtifact(
                "cargo_lock_package_count and direct_pin_count must be > 0",
            ));
        }
        Ok(())
    }

    /// Verifies exact `PD-05` binding against a verified `PROFILE-D1` [`BoundProfileManifestV1`].
    pub fn verify_bound_profile(
        &self,
        bound: &BoundProfileManifestV1,
    ) -> Result<(), ReleaseR1Error> {
        self.validate()?;
        bound.verify().map_err(|_| {
            ReleaseR1Error::ProfileBindingMismatch("BoundProfileManifestV1 failed verify")
        })?;
        if self.profile_id != bound.profile_id {
            return Err(ReleaseR1Error::ProfileBindingMismatch(
                "CandidateBuildInputsV1 profile_id disagrees with PROFILE-D1",
            ));
        }
        if self.genesis_id != bound.candidate.genesis_id {
            return Err(ReleaseR1Error::ProfileBindingMismatch(
                "CandidateBuildInputsV1 genesis_id disagrees with PROFILE-D1",
            ));
        }
        if self.domain_registry_digest != bound.candidate.domain_registry_digest {
            return Err(ReleaseR1Error::ProfileBindingMismatch(
                "CandidateBuildInputsV1 domain_registry_digest disagrees with PROFILE-D1",
            ));
        }
        Ok(())
    }

    /// Encodes into exact 363-byte canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], ReleaseR1Error> {
        self.validate()?;
        let mut buf = Vec::with_capacity(Self::WIRE_LEN);
        buf.extend_from_slice(&self.schema_version.to_be_bytes());
        buf.extend_from_slice(&self.profile_id);
        buf.extend_from_slice(&self.genesis_id);
        buf.extend_from_slice(&self.domain_registry_digest);
        buf.extend_from_slice(&self.framework_manifest_sha256);
        buf.extend_from_slice(&self.source_tree_digest);
        buf.extend_from_slice(&self.rust_toolchain_sha256);
        buf.extend_from_slice(&self.lean_toolchain_sha256);
        buf.extend_from_slice(&self.cargo_lock_sha256);
        buf.extend_from_slice(&self.sbom_sha256);
        buf.extend_from_slice(&self.cargo_lock_package_count.to_be_bytes());
        buf.extend_from_slice(&self.direct_pin_count.to_be_bytes());
        buf.extend_from_slice(&self.feature_profile_digest);
        buf.extend_from_slice(&self.build_config_digest);
        buf.push(self.platform_target.code());
        buf.try_into()
            .map_err(|_| ReleaseR1Error::InvalidWire("inputs length mismatch"))
    }

    /// Decodes and validates a 363-byte canonical [`CandidateBuildInputsV1`].
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, ReleaseR1Error> {
        if raw.len() != Self::WIRE_LEN {
            return Err(ReleaseR1Error::InvalidWire(
                "invalid CandidateBuildInputsV1 length",
            ));
        }
        let mut idx = 0usize;
        let schema_version = take_u16(raw, &mut idx);
        let profile_id = take_32(raw, &mut idx);
        let genesis_id = take_32(raw, &mut idx);
        let domain_registry_digest = take_32(raw, &mut idx);
        let framework_manifest_sha256 = take_32(raw, &mut idx);
        let source_tree_digest = take_32(raw, &mut idx);
        let rust_toolchain_sha256 = take_32(raw, &mut idx);
        let lean_toolchain_sha256 = take_32(raw, &mut idx);
        let cargo_lock_sha256 = take_32(raw, &mut idx);
        let sbom_sha256 = take_32(raw, &mut idx);
        let cargo_lock_package_count = take_u32(raw, &mut idx);
        let direct_pin_count = take_u32(raw, &mut idx);
        let feature_profile_digest = take_32(raw, &mut idx);
        let build_config_digest = take_32(raw, &mut idx);
        let platform_target = CandidatePlatformTargetV1::from_code(raw[idx])?;
        let out = Self {
            schema_version,
            profile_id,
            genesis_id,
            domain_registry_digest,
            framework_manifest_sha256,
            source_tree_digest,
            rust_toolchain_sha256,
            lean_toolchain_sha256,
            cargo_lock_sha256,
            sbom_sha256,
            cargo_lock_package_count,
            direct_pin_count,
            feature_profile_digest,
            build_config_digest,
            platform_target,
        };
        out.validate()?;
        Ok(out)
    }

    /// Computes the canonical `ContentTag` over `RELEASE_R1_BUILD_INPUTS_SCHEMA`.
    pub fn tag(&self) -> Result<ContentTag, ReleaseR1Error> {
        let enc = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            RELEASE_R1_BUILD_INPUTS_SCHEMA,
            enc.len() as u64,
            |w| w(&enc),
        )
        .expect("static schema and fixed length"))
    }
}

/// One independent builder's artifact digest attestation for [`verify_candidate_reproducibility`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BuilderAttestationV1 {
    /// Distinct builder identifier (e.g. `1 = isolated-target-A`, `2 = isolated-target-B`).
    pub builder_id: u32,
    /// `SHA-256` of the reproduced native release binary on `platform_target`.
    pub native_artifact_sha256: [u8; 32],
    /// Exact byte length of the reproduced native release binary.
    pub native_artifact_len: u64,
    /// `SHA-256` of the reproduced `wasm32-unknown-unknown` `+simd128` release binary.
    pub wasm_artifact_sha256: [u8; 32],
    /// Exact byte length of the reproduced `wasm32-unknown-unknown` `+simd128` release binary.
    pub wasm_artifact_len: u64,
}

/// Canonical candidate-build reproducibility receipt
/// (`bitblock/release-r1/candidate-build-receipt/v1`, `217` bytes) explicitly separated from
/// admitted `RELEASE-1` signed release authority (`admitted_release_authority = false`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CandidateReleaseReceiptV1 {
    /// Receipt schema version (`1`).
    pub receipt_version: u16,
    /// Canonical `ContentTag` digest of the bound [`CandidateBuildInputsV1`].
    pub candidate_build_inputs_digest: [u8; 32],
    /// Governing `PROFILE-D1` candidate `profile_id`.
    pub profile_id: [u8; 32],
    /// Immutable `GENESIS-D1` `genesis_id`.
    pub genesis_id: [u8; 32],
    /// `RELEASE-R1@framework` `framework_manifest_sha256`.
    pub framework_manifest_sha256: [u8; 32],
    /// Reproduced native release artifact `SHA-256` (platform-scoped).
    pub native_artifact_sha256: [u8; 32],
    /// Reproduced native release artifact byte length.
    pub native_artifact_len: u64,
    /// Reproduced `wasm32` `+simd128` release artifact `SHA-256`.
    pub wasm_artifact_sha256: [u8; 32],
    /// Reproduced `wasm32` `+simd128` release artifact byte length.
    pub wasm_artifact_len: u64,
    /// Count of distinct independent builders (`>= 2`) agreeing bit-for-bit.
    pub independent_builder_count: u32,
    /// Must strictly be `true` (diverged builders fail closed before receipt emission).
    pub builders_bit_identical: bool,
    /// Must strictly be `true` (native ELF/PE hashes remain explicitly platform-scoped).
    pub platform_scoped_only: bool,
    /// Must strictly be `false` (`RELEASE-R1` candidate build is NEVER admitted `RELEASE-1`).
    pub admitted_release_authority: bool,
}

impl CandidateReleaseReceiptV1 {
    /// Fixed canonical big-endian wire length (`217` bytes).
    pub const WIRE_LEN: usize = 2 + 32 * 6 + 8 + 8 + 4 + 1 + 1 + 1;

    /// Validates non-admission, platform scoping, builder count, and bit-exact agreement fail-closed.
    pub fn validate(&self) -> Result<(), ReleaseR1Error> {
        if self.receipt_version != RELEASE_R1_VERSION {
            return Err(ReleaseR1Error::InvalidWire("receipt_version must be 1"));
        }
        if self.admitted_release_authority {
            return Err(ReleaseR1Error::AdmittedReleaseAuthorityForbidden);
        }
        if !self.platform_scoped_only {
            return Err(ReleaseR1Error::PlatformScopingRequired);
        }
        if self.independent_builder_count < 2 || !self.builders_bit_identical {
            return Err(ReleaseR1Error::ReproducibilityDivergence(
                "requires >= 2 independent bit-identical builders",
            ));
        }
        if self.native_artifact_len == 0
            || self.wasm_artifact_len == 0
            || self.candidate_build_inputs_digest == [0u8; 32]
            || self.profile_id == [0u8; 32]
            || self.genesis_id == [0u8; 32]
            || self.framework_manifest_sha256 == [0u8; 32]
            || self.native_artifact_sha256 == [0u8; 32]
            || self.wasm_artifact_sha256 == [0u8; 32]
        {
            return Err(ReleaseR1Error::EmptyDigestOrArtifact(
                "receipt requires non-zero artifact digests and byte lengths",
            ));
        }
        Ok(())
    }

    /// Encodes into exact 217-byte canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], ReleaseR1Error> {
        self.validate()?;
        let mut buf = Vec::with_capacity(Self::WIRE_LEN);
        buf.extend_from_slice(&self.receipt_version.to_be_bytes());
        buf.extend_from_slice(&self.candidate_build_inputs_digest);
        buf.extend_from_slice(&self.profile_id);
        buf.extend_from_slice(&self.genesis_id);
        buf.extend_from_slice(&self.framework_manifest_sha256);
        buf.extend_from_slice(&self.native_artifact_sha256);
        buf.extend_from_slice(&self.native_artifact_len.to_be_bytes());
        buf.extend_from_slice(&self.wasm_artifact_sha256);
        buf.extend_from_slice(&self.wasm_artifact_len.to_be_bytes());
        buf.extend_from_slice(&self.independent_builder_count.to_be_bytes());
        buf.push(u8::from(self.builders_bit_identical));
        buf.push(u8::from(self.platform_scoped_only));
        buf.push(u8::from(self.admitted_release_authority));
        buf.try_into()
            .map_err(|_| ReleaseR1Error::InvalidWire("receipt length mismatch"))
    }

    /// Decodes and validates a 217-byte canonical [`CandidateReleaseReceiptV1`].
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, ReleaseR1Error> {
        if raw.len() != Self::WIRE_LEN {
            return Err(ReleaseR1Error::InvalidWire(
                "invalid CandidateReleaseReceiptV1 length",
            ));
        }
        let mut idx = 0usize;
        let receipt_version = take_u16(raw, &mut idx);
        let candidate_build_inputs_digest = take_32(raw, &mut idx);
        let profile_id = take_32(raw, &mut idx);
        let genesis_id = take_32(raw, &mut idx);
        let framework_manifest_sha256 = take_32(raw, &mut idx);
        let native_artifact_sha256 = take_32(raw, &mut idx);
        let native_artifact_len = take_u64(raw, &mut idx);
        let wasm_artifact_sha256 = take_32(raw, &mut idx);
        let wasm_artifact_len = take_u64(raw, &mut idx);
        let independent_builder_count = take_u32(raw, &mut idx);
        let builders_bit_identical = take_flag(raw, &mut idx)?;
        let platform_scoped_only = take_flag(raw, &mut idx)?;
        let admitted_release_authority = take_flag(raw, &mut idx)?;
        let out = Self {
            receipt_version,
            candidate_build_inputs_digest,
            profile_id,
            genesis_id,
            framework_manifest_sha256,
            native_artifact_sha256,
            native_artifact_len,
            wasm_artifact_sha256,
            wasm_artifact_len,
            independent_builder_count,
            builders_bit_identical,
            platform_scoped_only,
            admitted_release_authority,
        };
        out.validate()?;
        Ok(out)
    }

    /// Computes the canonical `ContentTag` over `RELEASE_R1_CANDIDATE_RECEIPT_SCHEMA`.
    pub fn tag(&self) -> Result<ContentTag, ReleaseR1Error> {
        let enc = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            RELEASE_R1_CANDIDATE_RECEIPT_SCHEMA,
            enc.len() as u64,
            |w| w(&enc),
        )
        .expect("static schema and fixed length"))
    }
}

/// Verifies `PD-04` + `PD-05` profile-bound candidate build inputs and `>= 2` distinct independent
/// builder attestations, emitting a verified [`CandidateReleaseReceiptV1`] iff every builder
/// agrees bit-for-bit on both native and `wasm32` artifacts.
pub fn verify_candidate_reproducibility(
    bound_profile: &BoundProfileManifestV1,
    inputs: &CandidateBuildInputsV1,
    expected_framework_manifest_sha256: [u8; 32],
    builders: &[BuilderAttestationV1],
) -> Result<CandidateReleaseReceiptV1, ReleaseR1Error> {
    inputs.verify_bound_profile(bound_profile)?;
    if inputs.framework_manifest_sha256 != expected_framework_manifest_sha256 {
        return Err(ReleaseR1Error::ProfileBindingMismatch(
            "framework_manifest_sha256 disagrees with RELEASE-R1@framework",
        ));
    }
    if builders.len() < 2 {
        return Err(ReleaseR1Error::ReproducibilityDivergence(
            "at least 2 independent builders required",
        ));
    }
    let first = builders[0];
    if first.native_artifact_len == 0
        || first.wasm_artifact_len == 0
        || first.native_artifact_sha256 == [0u8; 32]
        || first.wasm_artifact_sha256 == [0u8; 32]
    {
        return Err(ReleaseR1Error::EmptyDigestOrArtifact(
            "builder attestation has empty digest or zero length",
        ));
    }
    let mut seen_ids = std::collections::BTreeSet::new();
    for b in builders {
        if !seen_ids.insert(b.builder_id) {
            return Err(ReleaseR1Error::ReproducibilityDivergence(
                "duplicate builder_id in reproducibility attestation",
            ));
        }
        if b.native_artifact_sha256 != first.native_artifact_sha256
            || b.native_artifact_len != first.native_artifact_len
            || b.wasm_artifact_sha256 != first.wasm_artifact_sha256
            || b.wasm_artifact_len != first.wasm_artifact_len
        {
            return Err(ReleaseR1Error::ReproducibilityDivergence(
                "independent builders produced divergent artifact bytes",
            ));
        }
    }
    let rcpt = CandidateReleaseReceiptV1 {
        receipt_version: RELEASE_R1_VERSION,
        candidate_build_inputs_digest: inputs.tag()?.digest(),
        profile_id: inputs.profile_id,
        genesis_id: inputs.genesis_id,
        framework_manifest_sha256: inputs.framework_manifest_sha256,
        native_artifact_sha256: first.native_artifact_sha256,
        native_artifact_len: first.native_artifact_len,
        wasm_artifact_sha256: first.wasm_artifact_sha256,
        wasm_artifact_len: first.wasm_artifact_len,
        independent_builder_count: builders.len() as u32,
        builders_bit_identical: true,
        platform_scoped_only: true,
        admitted_release_authority: false,
    };
    rcpt.validate()?;
    Ok(rcpt)
}

/// Pinned live `RELEASE-R1@framework` SHA-256 digests (`Cargo.lock` 220 pkgs / 11 direct pins).
pub const PINNED_FRAMEWORK_MANIFEST_SHA256: [u8; 32] = [
    0xfe, 0xec, 0x12, 0x55, 0xb6, 0xdc, 0x45, 0xab, 0xda, 0x2b, 0x17, 0xee, 0x97, 0xe4, 0xbd, 0x5e,
    0xcc, 0x44, 0x8d, 0x67, 0x95, 0xf3, 0x8d, 0xe5, 0xd7, 0x89, 0x7d, 0x04, 0x78, 0xc3, 0x8d, 0x08,
];
/// Pinned `rust-toolchain.toml` (`1.96.0` minimal) SHA-256.
pub const PINNED_RUST_TOOLCHAIN_SHA256: [u8; 32] = [
    0x0d, 0x7b, 0x2f, 0x65, 0x89, 0xd3, 0xa2, 0xd9, 0xea, 0x22, 0xb7, 0x82, 0xa1, 0x07, 0xdd, 0xaf,
    0xc0, 0xda, 0x08, 0xbd, 0xca, 0x08, 0xe3, 0x52, 0x4b, 0x0c, 0xf4, 0xdb, 0x12, 0xc1, 0x2d, 0xa9,
];
/// Pinned `proof/lean-toolchain` (`v4.33.0`) SHA-256.
pub const PINNED_LEAN_TOOLCHAIN_SHA256: [u8; 32] = [
    0x30, 0x2c, 0xd6, 0x3c, 0x54, 0x17, 0x88, 0x85, 0xb8, 0x9e, 0x66, 0x9f, 0x33, 0xb3, 0x8f, 0x12,
    0xf4, 0xdd, 0x7a, 0xe7, 0xe5, 0xca, 0xc5, 0x37, 0xb3, 0x20, 0x3e, 0x37, 0x68, 0xd8, 0xfb, 0x2b,
];
/// Pinned `Cargo.lock` (`220` packages) SHA-256.
pub const PINNED_CARGO_LOCK_SHA256: [u8; 32] = [
    0x4d, 0x5b, 0x8b, 0x9d, 0x99, 0x32, 0xe8, 0x63, 0xc4, 0x68, 0xdf, 0x0d, 0x0d, 0xb5, 0x38, 0x1a,
    0x94, 0x00, 0xb2, 0x9c, 0x35, 0x8e, 0x77, 0xa1, 0x0d, 0xa9, 0x99, 0xa8, 0x3a, 0x88, 0x65, 0x7e,
];
/// Pinned deterministic sorted `Cargo.lock` SBOM SHA-256.
pub const PINNED_SBOM_SHA256: [u8; 32] = [
    0xaa, 0xaf, 0x32, 0x2b, 0xa8, 0x01, 0x9d, 0x81, 0x0e, 0x08, 0x53, 0xe7, 0x47, 0x98, 0xd6, 0xca,
    0x49, 0x4a, 0x38, 0x38, 0xeb, 0x5b, 0x27, 0xa7, 0x13, 0x69, 0x30, 0xc3, 0xb8, 0xa8, 0xa2, 0x3b,
];
/// Pinned committed `crates/wasm/www/bitblock_wasm.wasm` (`+simd128`) SHA-256.
pub const PINNED_COMMITTED_WASM_SHA256: [u8; 32] = [
    0x9a, 0x96, 0x05, 0x28, 0xc1, 0x24, 0x64, 0xb3, 0xc5, 0xe9, 0xf4, 0x5e, 0x09, 0x1c, 0x5f, 0x6e,
    0x38, 0x74, 0x8b, 0x98, 0x78, 0x60, 0x37, 0xc4, 0x37, 0x7d, 0xff, 0xd0, 0x9a, 0xf5, 0x3d, 0x2e,
];

/// Builds the canonical `RELEASE-R1@candidate` input descriptor and verified dual-builder receipt
/// bound to `bound_profile` (`PROFILE-D1`) and the live `RELEASE-R1@framework` pins.
pub fn build_canonical_candidate_fixture(
    bound_profile: &BoundProfileManifestV1,
) -> Result<(CandidateBuildInputsV1, CandidateReleaseReceiptV1), ReleaseR1Error> {
    let inputs = CandidateBuildInputsV1 {
        schema_version: RELEASE_R1_VERSION,
        profile_id: bound_profile.profile_id,
        genesis_id: bound_profile.candidate.genesis_id,
        domain_registry_digest: bound_profile.candidate.domain_registry_digest,
        framework_manifest_sha256: PINNED_FRAMEWORK_MANIFEST_SHA256,
        source_tree_digest: digest_source_label("bitblock-launch-candidate-v1-tree"),
        rust_toolchain_sha256: PINNED_RUST_TOOLCHAIN_SHA256,
        lean_toolchain_sha256: PINNED_LEAN_TOOLCHAIN_SHA256,
        cargo_lock_sha256: PINNED_CARGO_LOCK_SHA256,
        sbom_sha256: PINNED_SBOM_SHA256,
        cargo_lock_package_count: 220,
        direct_pin_count: 11,
        feature_profile_digest: digest_feature_profile("default+protocol-research"),
        build_config_digest: digest_build_config(
            "release+locked+remap-path-prefix+wasm32-simd128-panic-abort",
        ),
        platform_target: CandidatePlatformTargetV1::LinuxAarch64Elf,
    };
    let nat_dig: [u8; 32] = Sha256::digest(b"bb.relr1.canonical_native_elf_fixture.v1").into();
    let builders = [
        BuilderAttestationV1 {
            builder_id: 1,
            native_artifact_sha256: nat_dig,
            native_artifact_len: 4_194_304,
            wasm_artifact_sha256: PINNED_COMMITTED_WASM_SHA256,
            wasm_artifact_len: 1_831_314,
        },
        BuilderAttestationV1 {
            builder_id: 2,
            native_artifact_sha256: nat_dig,
            native_artifact_len: 4_194_304,
            wasm_artifact_sha256: PINNED_COMMITTED_WASM_SHA256,
            wasm_artifact_len: 1_831_314,
        },
    ];
    let rcpt = verify_candidate_reproducibility(
        bound_profile,
        &inputs,
        PINNED_FRAMEWORK_MANIFEST_SHA256,
        &builders,
    )?;
    Ok((inputs, rcpt))
}

fn take_u16(b: &[u8], i: &mut usize) -> u16 {
    let v = u16::from_be_bytes([b[*i], b[*i + 1]]);
    *i += 2;
    v
}

fn take_u32(b: &[u8], i: &mut usize) -> u32 {
    let v = u32::from_be_bytes([b[*i], b[*i + 1], b[*i + 2], b[*i + 3]]);
    *i += 4;
    v
}

fn take_u64(b: &[u8], i: &mut usize) -> u64 {
    let v = u64::from_be_bytes(b[*i..*i + 8].try_into().expect("8"));
    *i += 8;
    v
}

fn take_32(b: &[u8], i: &mut usize) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(&b[*i..*i + 32]);
    *i += 32;
    out
}

fn take_flag(b: &[u8], i: &mut usize) -> Result<bool, ReleaseR1Error> {
    let v = b[*i];
    *i += 1;
    match v {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(ReleaseR1Error::InvalidWire("non-boolean flag byte")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consensus_1::ConsensusClusterHarnessV1;

    #[test]
    fn release_r1_candidate_inputs_receipt_and_golden_tags() {
        let cluster = ConsensusClusterHarnessV1::bootstrap_four_validators().unwrap();
        let (inputs, rcpt) =
            build_canonical_candidate_fixture(&cluster.fixture.bound_manifest).unwrap();
        assert!(!rcpt.admitted_release_authority);
        assert!(rcpt.builders_bit_identical);
        assert!(rcpt.platform_scoped_only);
        assert_eq!(rcpt.independent_builder_count, 2);

        assert_eq!(
            CandidateBuildInputsV1::decode_canonical(&inputs.encode_canonical().unwrap()).unwrap(),
            inputs
        );
        assert_eq!(
            CandidateReleaseReceiptV1::decode_canonical(&rcpt.encode_canonical().unwrap()).unwrap(),
            rcpt
        );

        let inputs_tag = inputs.tag().unwrap().to_string();
        let rcpt_tag = rcpt.tag().unwrap().to_string();
        assert_eq!(
            inputs_tag,
            "#bb.policy.sha256.2wlogxgqgwzblmcwtpgvmai2h3s5vagoagxgywtjqmzcwxioryaq"
        );
        assert_eq!(
            rcpt_tag,
            "#bb.receipt.sha256.zyk4zg44x3q46h36xnnpghcklfndgulmx7pmuspm3d546vcblnfa"
        );
    }
}
