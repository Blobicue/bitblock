//! `RELEASE-1@base` (`PS-089`) & `RELEASE-1@admitted` (`PS-090`): admitted-profile signed release
//! record (`AdmittedReleaseCoreV1`, `ReleaseSignDocV1`, `ReleaseAuthorizationSignDocEnvelopeV1`,
//! `ReleaseAuthorizationV1`, `AdmittedReleaseRecordV1`), genesis-committed `3-of-5` Ed25519
//! threshold release authority (`SigningDomainTagV1::ReleaseArtifact`), vulnerability/withdrawal
//! enforcement, and fail-closed `AcceptedParentReleaseAuthorityV1` successor resolver guards.
//!
//! Strictly behind `--features protocol-research`, 100% `wasm32`-clean, zero new Cargo
//! dependencies, and zero edits to frozen crates or modules.
//!
//! # Normative Boundaries (`CAPABILITY_TREE.md` / `LAYER1_DESIGN_PAPER.md` §2 lines 795–845, 1000–1035)
//! - **Acyclic derivation:** [`AdmittedReleaseCoreV1`] contains ZERO authorization bytes and ZERO
//!   `chain_launch_manifest_id`. `admitted_release_core_id` is derived first, then
//!   [`ReleaseSignDocV1`] -> [`ReleaseAuthorizationSignDocEnvelopeV1`] -> detached threshold
//!   signatures -> [`AdmittedReleaseRecordV1`] (`admitted_release_record_id`).
//! - **Genesis-committed trust anchor (`ResolveTrustedReleaseAuthorityV1`):** The core's
//!   `release_authority_policy_id` is a byte-equal projection of the resolved authority, NEVER a
//!   selector for it. Initial launch uses `GenesisReleaseAuthorityV1`; successor releases require
//!   `AcceptedParentReleaseAuthorityV1` with `SuccessorReleaseAuthorityV1` purpose and a live
//!   consensus-native admitted parent certificate (replay-archive substitution rejects fail-closed).
//! - **Non-claim:** Deterministic devnet keys are explicitly labeled
//!   `DEVNET-TEST-KEYS-NOT-PRODUCTION-CUSTODY`. A signed admitted release does not prove source
//!   correctness, compiler/toolchain integrity, operator custody, availability, or consensus safety.

use std::collections::BTreeSet;
use std::fmt;

use sha2::{Digest, Sha256};

use crate::consensus_1::AcceptedConsensusNativeParentAuthorityCertificateV1;
use crate::crypto_d1::{
    CanonicalEd25519PublicKeyV1, CanonicalEd25519SignatureV1, HashSuiteIdV1, SignatureSuiteIdV1,
    SigningDomainTagV1, SigningPreimageV1, sign_preimage_deterministic, verify_preimage_strict,
};
use crate::genesis_d1::GenesisPackageV1;
use crate::genesis_root_parity_1::GenesisRootParityRecordV1;
use crate::profile_1::ProfileAdmissionManifestV1;
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::release_r1::{
    CandidateBuildInputsV1, CandidatePlatformTargetV1, CandidateReleaseReceiptV1,
};

/// Canonical schema version for `RELEASE-1` objects.
pub const RELEASE_1_VERSION: u16 = 1;

/// Canonical schema string for authorization-free [`AdmittedReleaseCoreV1`].
pub const RELEASE_1_CORE_SCHEMA: &[u8] = b"bitblock/admitted-release-core/v1";

/// Canonical schema string for [`ReleaseSignDocV1`].
pub const RELEASE_1_SIGN_DOC_SCHEMA: &[u8] = b"bitblock/release-sign-doc/v1";

/// Canonical schema string for detached signing envelope [`ReleaseAuthorizationSignDocEnvelopeV1`].
pub const RELEASE_1_AUTH_SIGN_DOC_SCHEMA: &[u8] = b"bitblock/release-authorization-sign-doc/v1";

/// Canonical schema string for complete [`AdmittedReleaseRecordV1`].
pub const RELEASE_1_RECORD_SCHEMA: &[u8] = b"bitblock/admitted-release-record/v1";

const REL1_POLICY_PREFIX: &[u8] = b"bb.rel1.trusted_release_authority_policy.v1";

/// Fail-closed validation, binding, withdrawal, or threshold-signature error for `RELEASE-1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Release1Error {
    /// Truncated, overlong, malformed, or non-canonical wire encoding.
    InvalidWire(&'static str),
    /// Candidate build inputs, receipt, profile admission, or genesis/parity binding mismatched.
    BindingMismatch(&'static str),
    /// Core `release_authority_policy_id` disagreed with the trusted genesis/parent resolver.
    AuthorityPolicyMismatch(&'static str),
    /// Threshold signature count, signer ordering, key membership, domain, or Ed25519 check failed.
    InvalidReleaseAuthorization(&'static str),
    /// Release was withdrawn for a security advisory or superseded and cannot be deployed.
    ReleaseNotDeployable(VulnerabilityWithdrawalStatusV1),
    /// Successor parent-authority context had wrong purpose, replay source, or missing live cert.
    SuccessorAuthorityRejected(&'static str),
}

impl fmt::Display for Release1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Release1Error {}

/// Explicit vulnerability and withdrawal lifecycle status carried by [`AdmittedReleaseCoreV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum VulnerabilityWithdrawalStatusV1 {
    /// Active, non-withdrawn release eligible for deployment verification.
    Active = 1,
    /// Explicitly withdrawn due to a security advisory (blocked from deployment fail-closed).
    WithdrawnSecurityAdvisory = 2,
    /// Explicitly superseded by a newer release record (blocked from fresh deployment fail-closed).
    Superseded = 3,
}

impl VulnerabilityWithdrawalStatusV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte vulnerability/withdrawal discriminant.
    pub const fn from_code(code: u8) -> Result<Self, Release1Error> {
        match code {
            1 => Ok(Self::Active),
            2 => Ok(Self::WithdrawnSecurityAdvisory),
            3 => Ok(Self::Superseded),
            _ => Err(Release1Error::InvalidWire(
                "unknown VulnerabilityWithdrawalStatusV1",
            )),
        }
    }
}

/// Parent-authority purpose codes aligned with `AcceptedParentAuthorityUseV1` (`LAYER1_DESIGN_PAPER.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ReleaseParentPurposeCodeV1 {
    /// Genesis initial launch (no parent consensus certificate).
    GenesisLaunch = 0,
    /// Block-envelope parent authority (`EnvelopeParentAuthorityV1`, forbidden for releases!).
    EnvelopeParentAuthority = 1,
    /// Sole legal successor release authority purpose (`SuccessorReleaseAuthorityV1`).
    SuccessorReleaseAuthority = 2,
    /// Successor activation authority (`SuccessorActivationAuthorityV1`, forbidden for releases!).
    SuccessorActivationAuthority = 3,
}

impl ReleaseParentPurposeCodeV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte parent-purpose discriminant.
    pub const fn from_code(code: u8) -> Result<Self, Release1Error> {
        match code {
            0 => Ok(Self::GenesisLaunch),
            1 => Ok(Self::EnvelopeParentAuthority),
            2 => Ok(Self::SuccessorReleaseAuthority),
            3 => Ok(Self::SuccessorActivationAuthority),
            _ => Err(Release1Error::InvalidWire(
                "unknown ReleaseParentPurposeCodeV1",
            )),
        }
    }
}

/// Trusted release-authority context reference (`GenesisReleaseAuthorityV1` | `AcceptedParentReleaseAuthorityV1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReleaseAuthorityContextRefV1 {
    /// Initial launch release authority already committed inside `G_0` (`genesis_id`).
    GenesisReleaseAuthorityV1 {
        /// Immutable launch `genesis_id`.
        genesis_id: [u8; 32],
        /// Pre-candidate `G_0` release-authority policy digest.
        genesis_release_authority_policy_digest: [u8; 32],
    },
    /// Successor release authority derived strictly from an accepted consensus-native parent cert.
    AcceptedParentReleaseAuthorityV1 {
        /// Predecessor `accepted_parent_lookup_id`.
        predecessor_accepted_parent_lookup_id: [u8; 32],
        /// Purpose tag (MUST be [`ReleaseParentPurposeCodeV1::SuccessorReleaseAuthority`]).
        purpose_code: ReleaseParentPurposeCodeV1,
        /// Predecessor governance authority root reached via the purpose-tagged parent ref.
        predecessor_governance_authority_root: [u8; 32],
    },
}

impl ReleaseAuthorityContextRefV1 {
    /// Fixed canonical big-endian wire byte length (`66` bytes).
    pub const WIRE_LEN: usize = 1 + 32 + 1 + 32;

    /// Encodes into exact 66-byte canonical wire form.
    pub fn encode_canonical(&self) -> [u8; Self::WIRE_LEN] {
        let mut out = [0u8; Self::WIRE_LEN];
        match *self {
            Self::GenesisReleaseAuthorityV1 {
                genesis_id,
                genesis_release_authority_policy_digest,
            } => {
                out[0] = 1;
                out[1..33].copy_from_slice(&genesis_id);
                out[33] = ReleaseParentPurposeCodeV1::GenesisLaunch.code();
                out[34..66].copy_from_slice(&genesis_release_authority_policy_digest);
            }
            Self::AcceptedParentReleaseAuthorityV1 {
                predecessor_accepted_parent_lookup_id,
                purpose_code,
                predecessor_governance_authority_root,
            } => {
                out[0] = 2;
                out[1..33].copy_from_slice(&predecessor_accepted_parent_lookup_id);
                out[33] = purpose_code.code();
                out[34..66].copy_from_slice(&predecessor_governance_authority_root);
            }
        }
        out
    }

    /// Decodes a 66-byte canonical [`ReleaseAuthorityContextRefV1`].
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, Release1Error> {
        if raw.len() != Self::WIRE_LEN {
            return Err(Release1Error::InvalidWire("bad context ref len"));
        }
        let a: [u8; 32] = raw[1..33].try_into().expect("32");
        let purpose = ReleaseParentPurposeCodeV1::from_code(raw[33])?;
        let b: [u8; 32] = raw[34..66].try_into().expect("32");
        match raw[0] {
            1 => {
                if purpose != ReleaseParentPurposeCodeV1::GenesisLaunch {
                    return Err(Release1Error::InvalidWire(
                        "GenesisReleaseAuthorityV1 requires GenesisLaunch purpose",
                    ));
                }
                Ok(Self::GenesisReleaseAuthorityV1 {
                    genesis_id: a,
                    genesis_release_authority_policy_digest: b,
                })
            }
            2 => Ok(Self::AcceptedParentReleaseAuthorityV1 {
                predecessor_accepted_parent_lookup_id: a,
                purpose_code: purpose,
                predecessor_governance_authority_root: b,
            }),
            _ => Err(Release1Error::InvalidWire("unknown context ref variant")),
        }
    }
}

/// Historical authority reference carried inside [`ReleaseSignDocV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReleaseHistoricalAuthorityRefV1 {
    /// Initial launch historical ref bound to `G_0`.
    GenesisReleaseHistoricalAuthorityRefV1 {
        /// Immutable launch `genesis_id`.
        genesis_id: [u8; 32],
        /// Committed `G_0` release-authority policy digest.
        genesis_policy_digest: [u8; 32],
    },
    /// Successor historical ref naming an already-accepted consensus-native parent certificate.
    AcceptedParentReleaseHistoricalAuthorityRefV1 {
        /// Admitted consensus-native parent certificate digest.
        parent_cert_id: [u8; 32],
        /// Evidence commitment inside the admitted parent certificate.
        evidence_commitment: [u8; 32],
        /// Authenticated occurrence-universe root at the required consensus position.
        occurrence_universe_root: [u8; 32],
        /// Consensus height at admission.
        height: u64,
        /// Consensus epoch at admission.
        epoch: u64,
        /// Purpose tag (MUST be [`ReleaseParentPurposeCodeV1::SuccessorReleaseAuthority`]).
        purpose_code: ReleaseParentPurposeCodeV1,
    },
}

impl ReleaseHistoricalAuthorityRefV1 {
    /// Fixed canonical big-endian wire byte length (`114` bytes).
    pub const WIRE_LEN: usize = 1 + 32 * 3 + 8 + 8 + 1;

    /// Encodes into exact 114-byte canonical big-endian wire form.
    pub fn encode_canonical(&self) -> [u8; Self::WIRE_LEN] {
        let mut out = [0u8; Self::WIRE_LEN];
        match *self {
            Self::GenesisReleaseHistoricalAuthorityRefV1 {
                genesis_id,
                genesis_policy_digest,
            } => {
                out[0] = 1;
                out[1..33].copy_from_slice(&genesis_id);
                out[33..65].copy_from_slice(&genesis_policy_digest);
                // out[65..114] stay zero (including purpose = GenesisLaunch = 0)
            }
            Self::AcceptedParentReleaseHistoricalAuthorityRefV1 {
                parent_cert_id,
                evidence_commitment,
                occurrence_universe_root,
                height,
                epoch,
                purpose_code,
            } => {
                out[0] = 2;
                out[1..33].copy_from_slice(&parent_cert_id);
                out[33..65].copy_from_slice(&evidence_commitment);
                out[65..97].copy_from_slice(&occurrence_universe_root);
                out[97..105].copy_from_slice(&height.to_be_bytes());
                out[105..113].copy_from_slice(&epoch.to_be_bytes());
                out[113] = purpose_code.code();
            }
        }
        out
    }

    /// Decodes a 114-byte canonical [`ReleaseHistoricalAuthorityRefV1`].
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, Release1Error> {
        if raw.len() != Self::WIRE_LEN {
            return Err(Release1Error::InvalidWire("bad historical ref len"));
        }
        let a: [u8; 32] = raw[1..33].try_into().expect("32");
        let b: [u8; 32] = raw[33..65].try_into().expect("32");
        let c: [u8; 32] = raw[65..97].try_into().expect("32");
        let height = u64::from_be_bytes(raw[97..105].try_into().expect("8"));
        let epoch = u64::from_be_bytes(raw[105..113].try_into().expect("8"));
        let purpose = ReleaseParentPurposeCodeV1::from_code(raw[113])?;
        match raw[0] {
            1 => {
                if c != [0u8; 32]
                    || height != 0
                    || epoch != 0
                    || purpose != ReleaseParentPurposeCodeV1::GenesisLaunch
                {
                    return Err(Release1Error::InvalidWire(
                        "non-zero padding in GenesisReleaseHistoricalAuthorityRefV1",
                    ));
                }
                Ok(Self::GenesisReleaseHistoricalAuthorityRefV1 {
                    genesis_id: a,
                    genesis_policy_digest: b,
                })
            }
            2 => Ok(Self::AcceptedParentReleaseHistoricalAuthorityRefV1 {
                parent_cert_id: a,
                evidence_commitment: b,
                occurrence_universe_root: c,
                height,
                epoch,
                purpose_code: purpose,
            }),
            _ => Err(Release1Error::InvalidWire("unknown historical ref variant")),
        }
    }
}

/// Genesis-committed `3-of-5` Ed25519 trusted release-authority policy
/// (`DEVNET-TEST-KEYS-NOT-PRODUCTION-CUSTODY`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustedReleaseAuthorityPolicyV1 {
    /// Minimum distinct valid signatures required (`3` for `3-of-5`).
    pub threshold_m: u8,
    /// Strictly sorted unique authorized Ed25519 release-signer public keys (`5` keys).
    pub authorized_pubkeys: Vec<CanonicalEd25519PublicKeyV1>,
}

impl TrustedReleaseAuthorityPolicyV1 {
    /// Deterministic devnet `3-of-5` test-key seeds (`DEVNET-TEST-KEYS-NOT-PRODUCTION-CUSTODY`).
    pub const DEVNET_TEST_SEEDS: [[u8; 32]; 5] =
        [[41u8; 32], [42u8; 32], [43u8; 32], [44u8; 32], [45u8; 32]];

    /// Constructs the deterministic devnet `3-of-5` release-authority policy and returns
    /// `(policy, sorted_key_seed_pairs)` for test/probe signing.
    pub fn devnet_three_of_five() -> (Self, Vec<(CanonicalEd25519PublicKeyV1, [u8; 32])>) {
        let mut pairs: Vec<(CanonicalEd25519PublicKeyV1, [u8; 32])> = Self::DEVNET_TEST_SEEDS
            .into_iter()
            .map(|seed| {
                (
                    CanonicalEd25519PublicKeyV1::from_secret_seed(&seed)
                        .expect("valid canonical devnet test seed"),
                    seed,
                )
            })
            .collect();
        pairs.sort_by_key(|(pk, _)| *pk);
        let policy = Self {
            threshold_m: 3,
            authorized_pubkeys: pairs.iter().map(|(pk, _)| *pk).collect(),
        };
        (policy, pairs)
    }

    /// Validates `1 <= threshold_m <= authorized_pubkeys.len()` and strict key uniqueness.
    pub fn validate(&self) -> Result<(), Release1Error> {
        if self.threshold_m == 0
            || usize::from(self.threshold_m) > self.authorized_pubkeys.len()
            || self.authorized_pubkeys.len() > 16
        {
            return Err(Release1Error::AuthorityPolicyMismatch(
                "invalid release authority threshold bounds",
            ));
        }
        for w in self.authorized_pubkeys.windows(2) {
            if w[0] >= w[1] {
                return Err(Release1Error::AuthorityPolicyMismatch(
                    "release authority pubkeys must be strictly sorted and unique",
                ));
            }
        }
        Ok(())
    }

    /// Computes the deterministic 32-byte `release_authority_policy_id` committed in `G_0`.
    pub fn policy_id(&self) -> Result<[u8; 32], Release1Error> {
        self.validate()?;
        let mut h = Sha256::new();
        h.update(REL1_POLICY_PREFIX);
        h.update([self.threshold_m, self.authorized_pubkeys.len() as u8]);
        for pk in &self.authorized_pubkeys {
            h.update(pk.as_bytes());
        }
        Ok(h.finalize().into())
    }
}

/// Authorization-free admitted release core (`bitblock/admitted-release-core/v1`, `438` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AdmittedReleaseCoreV1 {
    /// Schema version (`1`).
    pub schema_version: u16,
    /// Admitted `PROFILE-D1` `profile_id`.
    pub admitted_profile_id: [u8; 32],
    /// Admitted `TRANSITION-SPEC-1` `transition_spec_at_profile_id`.
    pub admitted_transition_spec_at_profile_id: [u8; 32],
    /// Admitted `PROFILE-1` `profile_admission_manifest_id`.
    pub profile_admission_manifest_id: [u8; 32],
    /// Immutable launch `GENESIS-D1` `genesis_id`.
    pub genesis_id: [u8; 32],
    /// Verified `GENESIS-ROOT-PARITY-1` `parity_record_id`.
    pub genesis_root_parity_record_id: [u8; 32],
    /// Bound `RELEASE-R1` `CandidateBuildInputsV1` digest.
    pub candidate_build_inputs_digest: [u8; 32],
    /// Bound `RELEASE-R1` `CandidateReleaseReceiptV1` digest.
    pub candidate_release_receipt_digest: [u8; 32],
    /// Bound `RELEASE-R1@framework` `framework_manifest_sha256`.
    pub framework_manifest_sha256: [u8; 32],
    /// Reproduced platform-scoped native release binary `SHA-256`.
    pub native_artifact_sha256: [u8; 32],
    /// Reproduced platform-scoped native release binary exact byte length.
    pub native_artifact_len: u64,
    /// Reproduced `wasm32` `+simd128` release binary `SHA-256`.
    pub wasm_artifact_sha256: [u8; 32],
    /// Reproduced `wasm32` `+simd128` release binary exact byte length.
    pub wasm_artifact_len: u64,
    /// Explicit platform target scope matching `CandidateBuildInputsV1`.
    pub platform_target: CandidatePlatformTargetV1,
    /// Trusted authority resolver context ref (never selected by candidate).
    pub release_authority_context_ref: ReleaseAuthorityContextRefV1,
    /// Required byte-equal projection of `ResolveTrustedReleaseAuthorityV1`.
    pub release_authority_policy_id: [u8; 32],
    /// Explicit vulnerability/withdrawal status (`Active` required for deployment).
    pub vulnerability_status: VulnerabilityWithdrawalStatusV1,
}

impl AdmittedReleaseCoreV1 {
    /// Fixed canonical big-endian wire length (`438` bytes).
    pub const WIRE_LEN: usize =
        2 + 32 * 10 + 8 + 8 + 1 + ReleaseAuthorityContextRefV1::WIRE_LEN + 32 + 1;

    /// Validates non-zero digests, non-zero artifact lengths, and schema version fail-closed.
    pub fn validate(&self) -> Result<(), Release1Error> {
        if self.schema_version != RELEASE_1_VERSION {
            return Err(Release1Error::InvalidWire("core schema_version must be 1"));
        }
        for d in [
            self.admitted_profile_id,
            self.admitted_transition_spec_at_profile_id,
            self.profile_admission_manifest_id,
            self.genesis_id,
            self.genesis_root_parity_record_id,
            self.candidate_build_inputs_digest,
            self.candidate_release_receipt_digest,
            self.framework_manifest_sha256,
            self.native_artifact_sha256,
            self.wasm_artifact_sha256,
            self.release_authority_policy_id,
        ] {
            if d == [0u8; 32] {
                return Err(Release1Error::BindingMismatch(
                    "zero digest forbidden in AdmittedReleaseCoreV1",
                ));
            }
        }
        if self.native_artifact_len == 0 || self.wasm_artifact_len == 0 {
            return Err(Release1Error::BindingMismatch(
                "zero artifact length forbidden in AdmittedReleaseCoreV1",
            ));
        }
        Ok(())
    }

    /// Encodes into exact 438-byte canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], Release1Error> {
        self.validate()?;
        let mut b = Vec::with_capacity(Self::WIRE_LEN);
        b.extend_from_slice(&self.schema_version.to_be_bytes());
        b.extend_from_slice(&self.admitted_profile_id);
        b.extend_from_slice(&self.admitted_transition_spec_at_profile_id);
        b.extend_from_slice(&self.profile_admission_manifest_id);
        b.extend_from_slice(&self.genesis_id);
        b.extend_from_slice(&self.genesis_root_parity_record_id);
        b.extend_from_slice(&self.candidate_build_inputs_digest);
        b.extend_from_slice(&self.candidate_release_receipt_digest);
        b.extend_from_slice(&self.framework_manifest_sha256);
        b.extend_from_slice(&self.native_artifact_sha256);
        b.extend_from_slice(&self.native_artifact_len.to_be_bytes());
        b.extend_from_slice(&self.wasm_artifact_sha256);
        b.extend_from_slice(&self.wasm_artifact_len.to_be_bytes());
        b.push(self.platform_target.code());
        b.extend_from_slice(&self.release_authority_context_ref.encode_canonical());
        b.extend_from_slice(&self.release_authority_policy_id);
        b.push(self.vulnerability_status.code());
        b.try_into()
            .map_err(|_| Release1Error::InvalidWire("core wire len mismatch"))
    }

    /// Decodes and validates a 438-byte canonical [`AdmittedReleaseCoreV1`].
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, Release1Error> {
        if raw.len() != Self::WIRE_LEN {
            return Err(Release1Error::InvalidWire("invalid core len"));
        }
        let mut c = 0usize;
        let schema_version = rd_u16(raw, &mut c);
        let admitted_profile_id = rd_32(raw, &mut c);
        let admitted_transition_spec_at_profile_id = rd_32(raw, &mut c);
        let profile_admission_manifest_id = rd_32(raw, &mut c);
        let genesis_id = rd_32(raw, &mut c);
        let genesis_root_parity_record_id = rd_32(raw, &mut c);
        let candidate_build_inputs_digest = rd_32(raw, &mut c);
        let candidate_release_receipt_digest = rd_32(raw, &mut c);
        let framework_manifest_sha256 = rd_32(raw, &mut c);
        let native_artifact_sha256 = rd_32(raw, &mut c);
        let native_artifact_len = rd_u64(raw, &mut c);
        let wasm_artifact_sha256 = rd_32(raw, &mut c);
        let wasm_artifact_len = rd_u64(raw, &mut c);
        let platform_target = CandidatePlatformTargetV1::from_code(raw[c])
            .map_err(|_| Release1Error::InvalidWire("bad platform_target"))?;
        c += 1;
        let release_authority_context_ref = ReleaseAuthorityContextRefV1::decode_canonical(
            &raw[c..c + ReleaseAuthorityContextRefV1::WIRE_LEN],
        )?;
        c += ReleaseAuthorityContextRefV1::WIRE_LEN;
        let release_authority_policy_id = rd_32(raw, &mut c);
        let vulnerability_status = VulnerabilityWithdrawalStatusV1::from_code(raw[c])?;
        let out = Self {
            schema_version,
            admitted_profile_id,
            admitted_transition_spec_at_profile_id,
            profile_admission_manifest_id,
            genesis_id,
            genesis_root_parity_record_id,
            candidate_build_inputs_digest,
            candidate_release_receipt_digest,
            framework_manifest_sha256,
            native_artifact_sha256,
            native_artifact_len,
            wasm_artifact_sha256,
            wasm_artifact_len,
            platform_target,
            release_authority_context_ref,
            release_authority_policy_id,
            vulnerability_status,
        };
        out.validate()?;
        Ok(out)
    }

    /// Computes the canonical `Policy` `ContentTag` whose digest IS `admitted_release_core_id`.
    pub fn tag(&self) -> Result<ContentTag, Release1Error> {
        let enc = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            RELEASE_1_CORE_SCHEMA,
            enc.len() as u64,
            |w| w(&enc),
        )
        .expect("static schema and fixed length"))
    }
}

/// Canonical release signing document (`bitblock/release-sign-doc/v1`, `246` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ReleaseSignDocV1 {
    /// Schema version (`1`).
    pub schema_version: u16,
    /// Exact derived `admitted_release_core_id`.
    pub admitted_release_core_id: [u8; 32],
    /// Exact trusted authority context reference.
    pub release_authority_context_ref: ReleaseAuthorityContextRefV1,
    /// Exact genesis or consensus-native historical authority reference.
    pub release_historical_authority_ref: ReleaseHistoricalAuthorityRefV1,
    /// Exact resolved `release_authority_policy_id`.
    pub release_authority_policy_id: [u8; 32],
}

impl ReleaseSignDocV1 {
    /// Fixed canonical big-endian wire byte length (`246` bytes).
    pub const WIRE_LEN: usize = 2
        + 32
        + ReleaseAuthorityContextRefV1::WIRE_LEN
        + ReleaseHistoricalAuthorityRefV1::WIRE_LEN
        + 32;

    /// Encodes into exact 246-byte canonical wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], Release1Error> {
        if self.schema_version != RELEASE_1_VERSION
            || self.admitted_release_core_id == [0u8; 32]
            || self.release_authority_policy_id == [0u8; 32]
        {
            return Err(Release1Error::InvalidWire(
                "invalid ReleaseSignDocV1 fields",
            ));
        }
        let mut b = Vec::with_capacity(Self::WIRE_LEN);
        b.extend_from_slice(&self.schema_version.to_be_bytes());
        b.extend_from_slice(&self.admitted_release_core_id);
        b.extend_from_slice(&self.release_authority_context_ref.encode_canonical());
        b.extend_from_slice(&self.release_historical_authority_ref.encode_canonical());
        b.extend_from_slice(&self.release_authority_policy_id);
        b.try_into()
            .map_err(|_| Release1Error::InvalidWire("sign doc len mismatch"))
    }

    /// Decodes a 246-byte canonical [`ReleaseSignDocV1`].
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, Release1Error> {
        if raw.len() != Self::WIRE_LEN {
            return Err(Release1Error::InvalidWire("invalid sign doc len"));
        }
        let mut c = 0usize;
        let schema_version = rd_u16(raw, &mut c);
        let admitted_release_core_id = rd_32(raw, &mut c);
        let release_authority_context_ref = ReleaseAuthorityContextRefV1::decode_canonical(
            &raw[c..c + ReleaseAuthorityContextRefV1::WIRE_LEN],
        )?;
        c += ReleaseAuthorityContextRefV1::WIRE_LEN;
        let release_historical_authority_ref = ReleaseHistoricalAuthorityRefV1::decode_canonical(
            &raw[c..c + ReleaseHistoricalAuthorityRefV1::WIRE_LEN],
        )?;
        c += ReleaseHistoricalAuthorityRefV1::WIRE_LEN;
        let release_authority_policy_id = rd_32(raw, &mut c);
        if schema_version != RELEASE_1_VERSION
            || admitted_release_core_id == [0u8; 32]
            || release_authority_policy_id == [0u8; 32]
        {
            return Err(Release1Error::InvalidWire("invalid ReleaseSignDocV1"));
        }
        Ok(Self {
            schema_version,
            admitted_release_core_id,
            release_authority_context_ref,
            release_historical_authority_ref,
            release_authority_policy_id,
        })
    }

    /// Computes the canonical `Policy` `ContentTag` whose digest IS `release_sign_doc_id`.
    pub fn tag(&self) -> Result<ContentTag, Release1Error> {
        let enc = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            RELEASE_1_SIGN_DOC_SCHEMA,
            enc.len() as u64,
            |w| w(&enc),
        )
        .expect("static schema and fixed length"))
    }
}

/// Exact detached authorization signing envelope (`bitblock/release-authorization-sign-doc/v1`,
/// `250` bytes: `LengthFrame(Canonical(ReleaseSignDocV1))`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ReleaseAuthorizationSignDocEnvelopeV1 {
    /// Enclosed [`ReleaseSignDocV1`].
    pub sign_doc: ReleaseSignDocV1,
}

impl ReleaseAuthorizationSignDocEnvelopeV1 {
    /// Fixed canonical big-endian wire length (`4 + 246 = 250` bytes).
    pub const WIRE_LEN: usize = 4 + ReleaseSignDocV1::WIRE_LEN;

    /// Encodes `u32_be(len) || Canonical(ReleaseSignDocV1)`.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], Release1Error> {
        let inner = self.sign_doc.encode_canonical()?;
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..4].copy_from_slice(&(inner.len() as u32).to_be_bytes());
        out[4..].copy_from_slice(&inner);
        Ok(out)
    }

    /// Computes the canonical `Policy` `ContentTag` hashed under `RELEASE_1_AUTH_SIGN_DOC_SCHEMA`.
    pub fn tag(&self) -> Result<ContentTag, Release1Error> {
        let enc = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            RELEASE_1_AUTH_SIGN_DOC_SCHEMA,
            enc.len() as u64,
            |w| w(&enc),
        )
        .expect("static schema and fixed length"))
    }

    /// Constructs the exact `CRYPTO-D1` [`SigningPreimageV1`] under `SigningDomainTagV1::ReleaseArtifact`
    /// with `launch_manifest_id = [0; 32]` (pre-launch acyclic derivation).
    pub fn signing_preimage(
        &self,
        chain_id: u64,
        genesis_id: [u8; 32],
        admitted_profile_id: [u8; 32],
    ) -> Result<SigningPreimageV1, Release1Error> {
        Ok(SigningPreimageV1 {
            protocol_version: RELEASE_1_VERSION,
            chain_id,
            genesis_id,
            launch_manifest_id: [0u8; 32],
            profile_id: admitted_profile_id,
            domain_tag: SigningDomainTagV1::ReleaseArtifact,
            signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
            hash_suite: HashSuiteIdV1::Sha256V1,
            payload_digest: self.tag()?.digest(),
        })
    }
}

/// One authorized release-signer's Ed25519 signature inside [`ReleaseAuthorizationV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ReleaseSignerSignatureV1 {
    /// Zero-based index into `TrustedReleaseAuthorityPolicyV1::authorized_pubkeys`.
    pub signer_index: u8,
    /// Canonical Ed25519 public key matching `authorized_pubkeys[signer_index]`.
    pub public_key: CanonicalEd25519PublicKeyV1,
    /// Strict RFC 8032 Ed25519 signature over `ReleaseAuthorizationSignDocEnvelopeV1`.
    pub signature: CanonicalEd25519SignatureV1,
}

impl ReleaseSignerSignatureV1 {
    /// Fixed canonical wire byte length (`1 + 32 + 64 = 97` bytes).
    pub const WIRE_LEN: usize = 97;
}

/// Detached `3-of-5` threshold release authorization (`ReleaseAuthorizationV1`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseAuthorizationV1 {
    /// Authorization schema version (`1`).
    pub auth_schema_version: u16,
    /// Exact [`ReleaseSignDocV1`] signed by all signers.
    pub sign_doc: ReleaseSignDocV1,
    /// Recomputed `release_sign_doc_id`.
    pub release_sign_doc_id: [u8; 32],
    /// Signature suite (`Ed25519StrictRfc8032V1`).
    pub signature_suite: SignatureSuiteIdV1,
    /// Credential-set / policy identifier (`TrustedReleaseAuthorityPolicyV1::policy_id`).
    pub release_authority_credential_set_id: [u8; 32],
    /// Strictly ascending unique `signer_index` threshold signatures.
    pub signatures: Vec<ReleaseSignerSignatureV1>,
}

/// Complete signed admitted release record (`bitblock/admitted-release-record/v1`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdmittedReleaseRecordV1 {
    /// Record schema version (`1`).
    pub schema_version: u16,
    /// Authorization-free [`AdmittedReleaseCoreV1`].
    pub core: AdmittedReleaseCoreV1,
    /// Recomputed `admitted_release_core_id`.
    pub admitted_release_core_id: [u8; 32],
    /// Detached threshold [`ReleaseAuthorizationV1`].
    pub authorization: ReleaseAuthorizationV1,
}

impl AdmittedReleaseRecordV1 {
    /// Encodes the complete record into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, Release1Error> {
        if self.schema_version != RELEASE_1_VERSION
            || self.authorization.auth_schema_version != RELEASE_1_VERSION
        {
            return Err(Release1Error::InvalidWire(
                "record schema_version must be 1",
            ));
        }
        let core_b = self.core.encode_canonical()?;
        let sd_b = self.authorization.sign_doc.encode_canonical()?;
        let n_sigs = u16::try_from(self.authorization.signatures.len())
            .map_err(|_| Release1Error::InvalidWire("too many release signatures"))?;
        let mut out = Vec::with_capacity(
            2 + core_b.len()
                + 32
                + 2
                + sd_b.len()
                + 32
                + 2
                + 32
                + 2
                + self.authorization.signatures.len() * ReleaseSignerSignatureV1::WIRE_LEN,
        );
        out.extend_from_slice(&self.schema_version.to_be_bytes());
        out.extend_from_slice(&core_b);
        out.extend_from_slice(&self.admitted_release_core_id);
        out.extend_from_slice(&self.authorization.auth_schema_version.to_be_bytes());
        out.extend_from_slice(&sd_b);
        out.extend_from_slice(&self.authorization.release_sign_doc_id);
        out.extend_from_slice(&self.authorization.signature_suite.code().to_be_bytes());
        out.extend_from_slice(&self.authorization.release_authority_credential_set_id);
        out.extend_from_slice(&n_sigs.to_be_bytes());
        for s in &self.authorization.signatures {
            out.push(s.signer_index);
            out.extend_from_slice(s.public_key.as_bytes());
            out.extend_from_slice(s.signature.as_bytes());
        }
        Ok(out)
    }

    /// Decodes a canonical [`AdmittedReleaseRecordV1`] without skipping signature/core structural checks.
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, Release1Error> {
        const MIN_LEN: usize = 2
            + AdmittedReleaseCoreV1::WIRE_LEN
            + 32
            + 2
            + ReleaseSignDocV1::WIRE_LEN
            + 32
            + 2
            + 32
            + 2;
        if raw.len() < MIN_LEN {
            return Err(Release1Error::InvalidWire(
                "truncated AdmittedReleaseRecordV1",
            ));
        }
        let mut c = 0usize;
        let schema_version = rd_u16(raw, &mut c);
        let core =
            AdmittedReleaseCoreV1::decode_canonical(&raw[c..c + AdmittedReleaseCoreV1::WIRE_LEN])?;
        c += AdmittedReleaseCoreV1::WIRE_LEN;
        let admitted_release_core_id = rd_32(raw, &mut c);
        let auth_schema_version = rd_u16(raw, &mut c);
        let sign_doc = ReleaseSignDocV1::decode_canonical(&raw[c..c + ReleaseSignDocV1::WIRE_LEN])?;
        c += ReleaseSignDocV1::WIRE_LEN;
        let release_sign_doc_id = rd_32(raw, &mut c);
        let signature_suite = SignatureSuiteIdV1::from_code(rd_u16(raw, &mut c))
            .map_err(|_| Release1Error::InvalidWire("unsupported release signature suite"))?;
        let release_authority_credential_set_id = rd_32(raw, &mut c);
        let n_sigs = usize::from(rd_u16(raw, &mut c));
        if raw.len() != c + n_sigs * ReleaseSignerSignatureV1::WIRE_LEN {
            return Err(Release1Error::InvalidWire(
                "AdmittedReleaseRecordV1 signature byte length mismatch",
            ));
        }
        let mut signatures = Vec::with_capacity(n_sigs.min(16));
        for _ in 0..n_sigs {
            let signer_index = raw[c];
            c += 1;
            let pk_bytes = rd_32(raw, &mut c);
            let mut sig_bytes = [0u8; 64];
            sig_bytes.copy_from_slice(&raw[c..c + 64]);
            c += 64;
            let public_key = CanonicalEd25519PublicKeyV1::from_bytes(pk_bytes)
                .map_err(|_| Release1Error::InvalidReleaseAuthorization("bad Ed25519 pubkey"))?;
            let signature = CanonicalEd25519SignatureV1::from_bytes(sig_bytes).map_err(|_| {
                Release1Error::InvalidReleaseAuthorization("bad Ed25519 signature scalar/point")
            })?;
            signatures.push(ReleaseSignerSignatureV1 {
                signer_index,
                public_key,
                signature,
            });
        }
        Ok(Self {
            schema_version,
            core,
            admitted_release_core_id,
            authorization: ReleaseAuthorizationV1 {
                auth_schema_version,
                sign_doc,
                release_sign_doc_id,
                signature_suite,
                release_authority_credential_set_id,
                signatures,
            },
        })
    }

    /// Computes the canonical `Receipt` `ContentTag` whose digest IS `admitted_release_record_id`.
    pub fn tag(&self) -> Result<ContentTag, Release1Error> {
        let enc = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            RELEASE_1_RECORD_SCHEMA,
            enc.len() as u64,
            |w| w(&enc),
        )
        .expect("static schema and bounded length"))
    }
}

/// Optional live consensus-native parent-authority certificate supplied ONLY for successor releases.
#[derive(Clone, Debug)]
pub struct SuccessorReleaseCertContextV1<'a> {
    /// Live admitted parent-authority certificate owned by `CONSENSUS-1`.
    pub live_parent_cert: &'a AcceptedConsensusNativeParentAuthorityCertificateV1,
    /// Must strictly be `false`: replay-archive certificates are forbidden for release authority.
    pub from_replay_archive: bool,
}

/// Resolves the trusted release authority policy and validates `release_authority_context_ref`
/// + `release_historical_authority_ref` (`ResolveTrustedReleaseAuthorityV1`).
pub fn resolve_trusted_release_authority(
    genesis_package: &GenesisPackageV1,
    genesis_release_policy: &TrustedReleaseAuthorityPolicyV1,
    ctx_ref: &ReleaseAuthorityContextRefV1,
    hist_ref: &ReleaseHistoricalAuthorityRefV1,
    successor_cert: Option<&SuccessorReleaseCertContextV1<'_>>,
) -> Result<[u8; 32], Release1Error> {
    let expected_policy_id = genesis_release_policy.policy_id()?;
    match (ctx_ref, hist_ref) {
        (
            ReleaseAuthorityContextRefV1::GenesisReleaseAuthorityV1 {
                genesis_id,
                genesis_release_authority_policy_digest,
            },
            ReleaseHistoricalAuthorityRefV1::GenesisReleaseHistoricalAuthorityRefV1 {
                genesis_id: h_gid,
                genesis_policy_digest: h_pol,
            },
        ) => {
            if *genesis_id != genesis_package.genesis_id || *h_gid != genesis_package.genesis_id {
                return Err(Release1Error::BindingMismatch(
                    "GenesisReleaseAuthorityV1 genesis_id mismatch",
                ));
            }
            if *genesis_release_authority_policy_digest != expected_policy_id
                || *h_pol != expected_policy_id
            {
                return Err(Release1Error::AuthorityPolicyMismatch(
                    "genesis release policy digest disagrees with committed G_0 authority",
                ));
            }
            Ok(expected_policy_id)
        }
        (
            ReleaseAuthorityContextRefV1::AcceptedParentReleaseAuthorityV1 {
                purpose_code,
                predecessor_accepted_parent_lookup_id,
                predecessor_governance_authority_root,
            },
            ReleaseHistoricalAuthorityRefV1::AcceptedParentReleaseHistoricalAuthorityRefV1 {
                parent_cert_id,
                evidence_commitment,
                occurrence_universe_root,
                purpose_code: h_purpose,
                ..
            },
        ) => {
            if *purpose_code != ReleaseParentPurposeCodeV1::SuccessorReleaseAuthority
                || *h_purpose != ReleaseParentPurposeCodeV1::SuccessorReleaseAuthority
            {
                return Err(Release1Error::SuccessorAuthorityRejected(
                    "successor release requires SuccessorReleaseAuthorityV1 purpose (purpose swap rejected)",
                ));
            }
            if *predecessor_accepted_parent_lookup_id == [0u8; 32]
                || *predecessor_governance_authority_root == [0u8; 32]
            {
                return Err(Release1Error::SuccessorAuthorityRejected(
                    "zero predecessor lookup or governance root",
                ));
            }
            let sc = successor_cert.ok_or(Release1Error::SuccessorAuthorityRejected(
                "missing live consensus-native AcceptedConsensusNativeParentAuthorityCertificateV1",
            ))?;
            if sc.from_replay_archive {
                return Err(Release1Error::SuccessorAuthorityRejected(
                    "replay-archive certificate forbidden in release authority resolution",
                ));
            }
            let cert_dig = sc
                .live_parent_cert
                .tag()
                .map_err(|_| Release1Error::SuccessorAuthorityRejected("invalid parent cert"))?
                .digest();
            if cert_dig != *parent_cert_id
                || sc.live_parent_cert.evidence_commitment_id != *evidence_commitment
                || sc.live_parent_cert.occurrence_universe_root != *occurrence_universe_root
            {
                return Err(Release1Error::SuccessorAuthorityRejected(
                    "historical ref disagrees with live admitted parent certificate",
                ));
            }
            Ok(expected_policy_id)
        }
        _ => Err(Release1Error::SuccessorAuthorityRejected(
            "mismatched release authority context ref and historical ref variants",
        )),
    }
}

/// Verifies an [`AdmittedReleaseRecordV1`] end-to-end against `PROFILE-1`, `GENESIS-D1`,
/// `GENESIS-ROOT-PARITY-1`, `RELEASE-R1@candidate`, trusted `3-of-5` Ed25519 release authority,
/// and `VulnerabilityWithdrawalStatusV1::Active` deployability, returning `admitted_release_record_id`.
#[allow(clippy::too_many_arguments)]
pub fn verify_admitted_release_record(
    record: &AdmittedReleaseRecordV1,
    genesis_package: &GenesisPackageV1,
    admission_manifest: &ProfileAdmissionManifestV1,
    parity_record: &GenesisRootParityRecordV1,
    candidate_inputs: &CandidateBuildInputsV1,
    candidate_receipt: &CandidateReleaseReceiptV1,
    genesis_release_policy: &TrustedReleaseAuthorityPolicyV1,
    successor_cert: Option<&SuccessorReleaseCertContextV1<'_>>,
) -> Result<[u8; 32], Release1Error> {
    record.core.validate()?;
    if record.core.vulnerability_status != VulnerabilityWithdrawalStatusV1::Active {
        return Err(Release1Error::ReleaseNotDeployable(
            record.core.vulnerability_status,
        ));
    }

    // 1. Cross-check all upstream owners byte-for-byte.
    let admission_id = admission_manifest
        .tag()
        .map_err(|_| Release1Error::BindingMismatch("invalid ProfileAdmissionManifestV1"))?
        .digest();
    let inputs_id = candidate_inputs
        .tag()
        .map_err(|_| Release1Error::BindingMismatch("invalid CandidateBuildInputsV1"))?
        .digest();
    let rcpt_id = candidate_receipt
        .tag()
        .map_err(|_| Release1Error::BindingMismatch("invalid CandidateReleaseReceiptV1"))?
        .digest();
    let parity_id = parity_record
        .parity_record_id()
        .map_err(|_| Release1Error::BindingMismatch("invalid GenesisRootParityRecordV1"))?;

    if record.core.admitted_profile_id != admission_manifest.admitted_profile_id
        || record.core.admitted_transition_spec_at_profile_id
            != admission_manifest.admitted_transition_spec_at_profile_id
        || record.core.profile_admission_manifest_id != admission_id
        || record.core.genesis_id != genesis_package.genesis_id
        || record.core.genesis_root_parity_record_id != parity_id
        || record.core.candidate_build_inputs_digest != inputs_id
        || record.core.candidate_release_receipt_digest != rcpt_id
        || record.core.framework_manifest_sha256 != candidate_receipt.framework_manifest_sha256
        || record.core.native_artifact_sha256 != candidate_receipt.native_artifact_sha256
        || record.core.native_artifact_len != candidate_receipt.native_artifact_len
        || record.core.wasm_artifact_sha256 != candidate_receipt.wasm_artifact_sha256
        || record.core.wasm_artifact_len != candidate_receipt.wasm_artifact_len
        || record.core.platform_target != candidate_inputs.platform_target
        || candidate_receipt.profile_id != admission_manifest.admitted_profile_id
        || candidate_receipt.genesis_id != genesis_package.genesis_id
    {
        return Err(Release1Error::BindingMismatch(
            "AdmittedReleaseCoreV1 projection disagrees with PROFILE-1 / GENESIS-ROOT-PARITY-1 / RELEASE-R1",
        ));
    }

    // 2. Recompute core_id, sign_doc_id, and resolve trusted release policy from G_0 / parent cert.
    let expected_core_id = record.core.tag()?.digest();
    if record.admitted_release_core_id != expected_core_id {
        return Err(Release1Error::BindingMismatch(
            "admitted_release_core_id mismatch",
        ));
    }
    let sd = &record.authorization.sign_doc;
    if sd.admitted_release_core_id != expected_core_id
        || sd.release_authority_context_ref != record.core.release_authority_context_ref
        || sd.release_authority_policy_id != record.core.release_authority_policy_id
    {
        return Err(Release1Error::BindingMismatch(
            "ReleaseSignDocV1 disagrees with AdmittedReleaseCoreV1",
        ));
    }
    let expected_sd_id = sd.tag()?.digest();
    if record.authorization.release_sign_doc_id != expected_sd_id {
        return Err(Release1Error::BindingMismatch(
            "release_sign_doc_id mismatch",
        ));
    }

    let resolved_policy_id = resolve_trusted_release_authority(
        genesis_package,
        genesis_release_policy,
        &record.core.release_authority_context_ref,
        &sd.release_historical_authority_ref,
        successor_cert,
    )?;
    if record.core.release_authority_policy_id != resolved_policy_id
        || record.authorization.release_authority_credential_set_id != resolved_policy_id
    {
        return Err(Release1Error::AuthorityPolicyMismatch(
            "core or authorization policy ID disagrees with ResolveTrustedReleaseAuthorityV1",
        ));
    }

    // 3. Verify >= threshold_m distinct authorized Ed25519 signatures over SigningDomainTagV1::ReleaseArtifact.
    if record.authorization.signatures.len() < usize::from(genesis_release_policy.threshold_m) {
        return Err(Release1Error::InvalidReleaseAuthorization(
            "insufficient threshold signatures (< 3-of-5)",
        ));
    }
    let env = ReleaseAuthorizationSignDocEnvelopeV1 { sign_doc: *sd };
    let preimage = env.signing_preimage(
        genesis_package.state.chain_id,
        genesis_package.genesis_id,
        record.core.admitted_profile_id,
    )?;
    let mut seen_idx = BTreeSet::new();
    for (pos, sig_entry) in record.authorization.signatures.iter().enumerate() {
        let idx = usize::from(sig_entry.signer_index);
        if idx >= genesis_release_policy.authorized_pubkeys.len() {
            return Err(Release1Error::InvalidReleaseAuthorization(
                "signer_index out of authorized keyset range",
            ));
        }
        if !seen_idx.insert(sig_entry.signer_index) {
            return Err(Release1Error::InvalidReleaseAuthorization(
                "duplicate signer_index in release authorization",
            ));
        }
        if pos > 0
            && record.authorization.signatures[pos - 1].signer_index >= sig_entry.signer_index
        {
            return Err(Release1Error::InvalidReleaseAuthorization(
                "release signatures must be strictly ascending by signer_index",
            ));
        }
        if sig_entry.public_key != genesis_release_policy.authorized_pubkeys[idx] {
            return Err(Release1Error::InvalidReleaseAuthorization(
                "unauthorized signer public key",
            ));
        }
        verify_preimage_strict(&sig_entry.public_key, &preimage, &sig_entry.signature).map_err(
            |_| Release1Error::InvalidReleaseAuthorization("Ed25519 signature verification failed"),
        )?;
    }

    // 4. Derive admitted_release_record_id ONLY after authorization passes.
    Ok(record.tag()?.digest())
}

/// Builds and signs the canonical `bitblock.devnet.v1` initial launch [`AdmittedReleaseRecordV1`]
/// using the deterministic `3-of-5` devnet test keys (`DEVNET-TEST-KEYS-NOT-PRODUCTION-CUSTODY`).
pub fn build_canonical_devnet_admitted_release(
    genesis_package: &GenesisPackageV1,
    admission_manifest: &ProfileAdmissionManifestV1,
    parity_record: &GenesisRootParityRecordV1,
    candidate_inputs: &CandidateBuildInputsV1,
    candidate_receipt: &CandidateReleaseReceiptV1,
) -> Result<(TrustedReleaseAuthorityPolicyV1, AdmittedReleaseRecordV1), Release1Error> {
    let (policy, key_pairs) = TrustedReleaseAuthorityPolicyV1::devnet_three_of_five();
    let policy_id = policy.policy_id()?;
    let admission_id = admission_manifest
        .tag()
        .map_err(|_| Release1Error::BindingMismatch("invalid admission manifest"))?
        .digest();
    let parity_id = parity_record
        .parity_record_id()
        .map_err(|_| Release1Error::BindingMismatch("invalid parity record"))?;
    let inputs_id = candidate_inputs
        .tag()
        .map_err(|_| Release1Error::BindingMismatch("invalid candidate inputs"))?
        .digest();
    let rcpt_id = candidate_receipt
        .tag()
        .map_err(|_| Release1Error::BindingMismatch("invalid candidate receipt"))?
        .digest();

    let ctx_ref = ReleaseAuthorityContextRefV1::GenesisReleaseAuthorityV1 {
        genesis_id: genesis_package.genesis_id,
        genesis_release_authority_policy_digest: policy_id,
    };
    let hist_ref = ReleaseHistoricalAuthorityRefV1::GenesisReleaseHistoricalAuthorityRefV1 {
        genesis_id: genesis_package.genesis_id,
        genesis_policy_digest: policy_id,
    };
    let core = AdmittedReleaseCoreV1 {
        schema_version: RELEASE_1_VERSION,
        admitted_profile_id: admission_manifest.admitted_profile_id,
        admitted_transition_spec_at_profile_id: admission_manifest
            .admitted_transition_spec_at_profile_id,
        profile_admission_manifest_id: admission_id,
        genesis_id: genesis_package.genesis_id,
        genesis_root_parity_record_id: parity_id,
        candidate_build_inputs_digest: inputs_id,
        candidate_release_receipt_digest: rcpt_id,
        framework_manifest_sha256: candidate_receipt.framework_manifest_sha256,
        native_artifact_sha256: candidate_receipt.native_artifact_sha256,
        native_artifact_len: candidate_receipt.native_artifact_len,
        wasm_artifact_sha256: candidate_receipt.wasm_artifact_sha256,
        wasm_artifact_len: candidate_receipt.wasm_artifact_len,
        platform_target: candidate_inputs.platform_target,
        release_authority_context_ref: ctx_ref,
        release_authority_policy_id: policy_id,
        vulnerability_status: VulnerabilityWithdrawalStatusV1::Active,
    };
    let core_id = core.tag()?.digest();
    let sign_doc = ReleaseSignDocV1 {
        schema_version: RELEASE_1_VERSION,
        admitted_release_core_id: core_id,
        release_authority_context_ref: ctx_ref,
        release_historical_authority_ref: hist_ref,
        release_authority_policy_id: policy_id,
    };
    let sign_doc_id = sign_doc.tag()?.digest();
    let env = ReleaseAuthorizationSignDocEnvelopeV1 { sign_doc };
    let preimage = env.signing_preimage(
        genesis_package.state.chain_id,
        genesis_package.genesis_id,
        core.admitted_profile_id,
    )?;

    let mut signatures = Vec::with_capacity(3);
    for (idx, (pk, seed)) in key_pairs.iter().take(3).enumerate() {
        let sig = sign_preimage_deterministic(seed, &preimage).map_err(|_| {
            Release1Error::InvalidReleaseAuthorization("deterministic signing failed")
        })?;
        signatures.push(ReleaseSignerSignatureV1 {
            signer_index: idx as u8,
            public_key: *pk,
            signature: sig,
        });
    }

    let record = AdmittedReleaseRecordV1 {
        schema_version: RELEASE_1_VERSION,
        core,
        admitted_release_core_id: core_id,
        authorization: ReleaseAuthorizationV1 {
            auth_schema_version: RELEASE_1_VERSION,
            sign_doc,
            release_sign_doc_id: sign_doc_id,
            signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
            release_authority_credential_set_id: policy_id,
            signatures,
        },
    };
    verify_admitted_release_record(
        &record,
        genesis_package,
        admission_manifest,
        parity_record,
        candidate_inputs,
        candidate_receipt,
        &policy,
        None,
    )?;
    Ok((policy, record))
}

fn rd_u16(b: &[u8], i: &mut usize) -> u16 {
    let v = u16::from_be_bytes([b[*i], b[*i + 1]]);
    *i += 2;
    v
}

fn rd_u64(b: &[u8], i: &mut usize) -> u64 {
    let v = u64::from_be_bytes(b[*i..*i + 8].try_into().expect("8"));
    *i += 8;
    v
}

fn rd_32(b: &[u8], i: &mut usize) -> [u8; 32] {
    let out: [u8; 32] = b[*i..*i + 32].try_into().expect("32");
    *i += 32;
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genesis_root_parity_1::{
        CanonicalDevnetGenesisFixtureV1, GenesisParityInputV1, verify_genesis_root_parity,
    };
    use crate::profile_1::build_canonical_devnet_profile_admission;
    use crate::release_r1::build_canonical_candidate_fixture;

    #[test]
    fn release_1_admitted_record_3_of_5_roundtrip_and_golden_tags() {
        let fx = CanonicalDevnetGenesisFixtureV1::build().unwrap();
        let (pkg_b, bnd_b) = (
            fx.package.encode_canonical().unwrap(),
            fx.bundle.encode_canonical().unwrap(),
        );
        let p_in = GenesisParityInputV1 {
            raw_genesis_package: &pkg_b,
            raw_preimage_bundle: &bnd_b,
            profile_scope: &fx.profile_scope,
            commit_domain_registry: &fx.commit_domain_registry,
            bound_manifest: &fx.bound_manifest,
            transition_spec: &fx.transition_spec,
            exec_registry: &fx.exec_registry,
        };
        let parity = verify_genesis_root_parity(&p_in).unwrap();
        let (_, manifest) = build_canonical_devnet_profile_admission(
            &fx.package,
            &fx.bound_manifest,
            &fx.transition_spec,
            &parity.record,
        )
        .unwrap();
        let (cand_in, cand_rcpt) = build_canonical_candidate_fixture(&fx.bound_manifest).unwrap();

        let (policy, record) = build_canonical_devnet_admitted_release(
            &fx.package,
            &manifest,
            &parity.record,
            &cand_in,
            &cand_rcpt,
        )
        .unwrap();
        let enc = record.encode_canonical().unwrap();
        let dec = AdmittedReleaseRecordV1::decode_canonical(&enc).unwrap();
        assert_eq!(dec, record);

        let rec_id = verify_admitted_release_record(
            &dec,
            &fx.package,
            &manifest,
            &parity.record,
            &cand_in,
            &cand_rcpt,
            &policy,
            None,
        )
        .unwrap();
        assert_eq!(rec_id, record.tag().unwrap().digest());

        let env = ReleaseAuthorizationSignDocEnvelopeV1 {
            sign_doc: record.authorization.sign_doc,
        };
        assert_eq!(
            record.core.tag().unwrap().to_string(),
            "#bb.policy.sha256.ounjvzefp2yesle4hlbmlo5m6elwvl4uk76v4ajudelek2bgjrga"
        );
        assert_eq!(
            record.authorization.sign_doc.tag().unwrap().to_string(),
            "#bb.policy.sha256.ehdsiakhtw2nbziey26vwmklsii2kjxbsty3c4prpavmcj77noea"
        );
        assert_eq!(
            env.tag().unwrap().to_string(),
            "#bb.policy.sha256.nbu5622ibijedtdqniy6taoo7xsauu7kuf66xvpwgquqmx2j5yea"
        );
        assert_eq!(
            record.tag().unwrap().to_string(),
            "#bb.receipt.sha256.qui5yzjsvwkfvbh7ihzva3swlvfgujvvxw3x6hxbmutto3udqlba"
        );
    }
}
