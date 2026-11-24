//! `SUCCESSOR-ACTIVATION-1@base` (`PS-092`) & `SUCCESSOR-ACTIVATION-1@activation` (`PS-093`):
//! internal-only [`SuccessorActivationBundleV1`], zero-allocation streaming preflight, three
//! non-serialized acyclic semantic verifier stages
//! ([`VerifiedSuccessorPredecessorBaseContextV1`] ->
//! [`VerifiedSuccessorReleaseAuthorityContextV1`] ->
//! [`VerifiedSuccessorActivationAuthorityContextV1`]), purpose-disjoint live `CONSENSUS-1`
//! parent-authority certificates, `3-of-5` Ed25519 [`ActivationAuthorizationV1`], and
//! [`DedicatedMigrationBodyV1`] availability carrier [`SuccessorActivationCarrierV1`].
//!
//! Strictly behind `--features protocol-research`, 100% `wasm32`-clean, zero new Cargo
//! dependencies, and zero edits to frozen crates or modules. Formally accompanied by
//! `proof/BitblockProof/SuccessorActivationRefinement.lean` (`lake env lean --trust=0`).
//!
//! # Normative Boundaries (`CAPABILITY_TREE.md` / `LAYER1_DESIGN_PAPER.md` §2 lines 2119–2245, 2885–3035)
//! - **Internal-only bundle (`external_root_count = 0`):** [`SuccessorActivationBundleV1`] carries
//!   seven explicit internal preimages (`ProfileScopeV1`, `DomainRegistryV1`,
//!   `BoundProfileManifestV1`, `ResolvedProfileTransitionSpecV1`, `ProfileAdmissionManifestV1`,
//!   `AdmittedReleaseRecordV1`, `SuccessorUpgradeEvidenceManifestV1`) plus optional bounded
//!   supplementary internal preimages (`|S| = 3` canonical or `|S| = 0` minimal). External/root
//!   dependency variants, locator tags, and duplicated prior launch/genesis/predecessor bytes all
//!   reject fail-closed before allocation or migration execution.
//! - **Three non-serialized acyclic semantic verifier stages:** Predecessor base eligibility
//!   (carrying distinct release & activation eligibility over one shared parent core and
//!   governance root), release-purpose live certificate admission (verified ONLY after
//!   `admitted_release_core_id` exists via frozen `release_1::verify_admitted_release_record`),
//!   and activation-purpose live certificate admission + `3-of-5` Ed25519 activation authorization
//!   (verified ONLY after `successor_activation_bundle_id` completes). None of the three contexts
//!   is serialized into or counted by [`SuccessorActivationBundleV1`].
//! - **Dedicated migration body:** `InPlaceMigrationTransitionV1` mandates
//!   [`DedicatedMigrationBodyV1`] with zero signed transactions, zero application subticks, and
//!   exact single [`AuthorizedMigrationSystemTransitionV1`].
//! - **Non-claim:** Deterministic devnet `3-of-5` activation keys (seeds `61..=65`) are explicitly
//!   labeled `DEVNET-TEST-KEYS-NOT-PRODUCTION-CUSTODY`. A verified successor bundle does not
//!   authorize itself, prove migration correctness, guarantee post-activation archival retention,
//!   or establish consensus safety.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use sha2::{Digest, Sha256};

use crate::chain_launch_1::{CanonicalDevnetLaunchBuildV1, build_canonical_devnet_chain_launch};
use crate::commit_state::DomainRegistryV1;
use crate::consensus_1::{
    AcceptedConsensusNativeParentAuthorityCertificateV1, CONSENSUS_1_VERSION,
    ConsensusClusterHarnessV1, ConsensusCommitCertificateV1,
    ConsensusNativeAcceptedRecordOccurrenceReceiptV1, ConsensusProposalV1, ConsensusVotePhaseV1,
    ConsensusVoteV1, LiveParentAuthorityRegistryV1, ParentAuthorityCertificateConsumerContextV1,
    ParentAuthorityPurposeV1, ParentAuthorityStatusV1, QuorumCertificateV1,
    VerifiedConsensusNativeAcceptedRecordOccurrenceHistoryStartEqualityV1,
    build_consensus_proposal_core, execution_root_ref_digest,
};
use crate::crypto_d1::{
    CanonicalEd25519PublicKeyV1, CanonicalEd25519SignatureV1, HashSuiteIdV1, SignatureSuiteIdV1,
    SigningDomainTagV1, SigningPreimageV1, sign_preimage_deterministic, verify_preimage_strict,
};
use crate::economic_state_1::EconomicStateV1;
use crate::execution_root_1::ExecutionRootRefV1;
use crate::executor_1::ExecutorForkContextV1;
use crate::genesis_d1::GenesisPackageV1;
use crate::genesis_root_parity_1::GenesisRootParityRecordV1;
use crate::governance_1::{GovernanceContextV1, GovernanceStateV1};
use crate::identity_d1::ActorIdV1;
use crate::profile_1::ProfileAdmissionManifestV1;
use crate::profile_bootstrap::ProfileScopeV1;
use crate::profile_d1::BoundProfileManifestV1;
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::protocol_id_1::{
    AcceptedProposalWrapperV1, ProposalCredentialEntryV1, SameWrapperAvailabilityDependencyV1,
    normalize_proposal_wrappers,
};
use crate::release_1::{
    AdmittedReleaseCoreV1, AdmittedReleaseRecordV1, ReleaseAuthorityContextRefV1,
    ReleaseAuthorizationSignDocEnvelopeV1, ReleaseAuthorizationV1, ReleaseHistoricalAuthorityRefV1,
    ReleaseParentPurposeCodeV1, ReleaseSignDocV1, ReleaseSignerSignatureV1,
    SuccessorReleaseCertContextV1, TrustedReleaseAuthorityPolicyV1,
    VulnerabilityWithdrawalStatusV1, verify_admitted_release_record,
};
use crate::release_r1::{
    CandidateBuildInputsV1, CandidateReleaseReceiptV1, RELEASE_R1_BUILD_INPUTS_SCHEMA,
    RELEASE_R1_CANDIDATE_RECEIPT_SCHEMA, build_canonical_candidate_fixture,
};
use crate::transition_spec_1::ResolvedProfileTransitionSpecV1;
use crate::upgrade_1::{
    CanonicalUpgrade1OutcomeV1, MigrationSpecV1, SuccessorUpgradeEvidenceManifestV1,
    UPGRADE_1_MIGRATION_SPEC_SCHEMA, admit_canonical_devnet_successor_profile,
    build_canonical_devnet_upgrade_1,
};
use crate::validator_schedule_1::{
    EpochValidatorScheduleV1, ValidatorKeyPairV1, ValidatorScheduleStateV1,
};

/// Canonical schema version for `SUCCESSOR-ACTIVATION-1` objects.
pub const SUCCESSOR_ACTIVATION_1_VERSION: u16 = 1;

/// Fixed count of mandatory explicit internal preimages in [`SuccessorActivationBundleV1`] (`7`).
pub const EXPLICIT_SUCCESSOR_ARTIFACT_COUNT: u32 = 7;

/// Fixed count of external roots in [`SuccessorActivationBundleV1`] (strictly `0`: internal-only).
pub const SUCCESSOR_EXTERNAL_ROOT_COUNT: u32 = 0;

/// Canonical embedded-only carrier discriminant (`1` is the sole legal embedded mode).
pub const SUCCESSOR_EMBEDDED_CARRIER_MODE_V1: u8 = 1;

/// Canonical internal-only dependency-ref wire discriminant (`1` is the sole legal variant).
pub const INTERNAL_DEP_REF_TAG_V1: u8 = 1;

/// Canonical schema string for internal-only [`SuccessorActivationBundleV1`].
pub const SUCCESSOR_ACTIVATION_1_BUNDLE_SCHEMA: &[u8] = b"bitblock/successor-activation-bundle/v1";

/// Canonical schema string for [`SuccessorActivationSignDocV1`].
pub const SUCCESSOR_ACTIVATION_1_SIGN_DOC_SCHEMA: &[u8] =
    b"bitblock/successor-activation-sign-doc/v1";

/// Canonical schema string for [`ActivationAuthorizationV1`].
pub const SUCCESSOR_ACTIVATION_1_AUTH_SCHEMA: &[u8] =
    b"bitblock/successor-activation-authorization/v1";

/// Canonical schema string for [`SuccessorActivationCarrierV1`].
pub const SUCCESSOR_ACTIVATION_1_CARRIER_SCHEMA: &[u8] =
    b"bitblock/successor-activation-1/carrier/v1";

/// Pinned merged `origin/main` PR #98 `UPGRADE-1` commit SHA-1 (`246ef87a...`).
pub const UPGRADE_1_MERGED_COMMIT_SHA1: [u8; 20] = [
    0x24, 0x6e, 0xf8, 0x7a, 0x5d, 0x2e, 0x5f, 0x7b, 0x68, 0x2f, 0xda, 0x23, 0xcf, 0x88, 0xff, 0xec,
    0x3c, 0x50, 0xb9, 0xc6,
];

const SA1_ACTIVATION_POLICY_PREFIX: &[u8] = b"bb.sa1.trusted_activation_authority_policy.v1";
const SA1_AVAIL_COMMIT_PREFIX: &[u8] = b"bb.sa1.availability_commitment.v1";
const SA1_GOV_AUTH_ROOT_PREFIX: &[u8] = b"bb.sa1.predecessor_gov_authority_root.v1";
const SA1_RELEASE_CERT_POLICY_PREFIX: &[u8] = b"bb.sa1.release_cert_policy.v1";
const SA1_ACT_CERT_POLICY_PREFIX: &[u8] = b"bb.sa1.activation_cert_policy.v1";
const SA1_RELEASE_ELIG_PRED_PREFIX: &[u8] = b"bb.sa1.release_elig_predicate.v1";
const SA1_ACT_ELIG_PRED_PREFIX: &[u8] = b"bb.sa1.activation_elig_predicate.v1";

/// Fail-closed preflight, equation, DAG, authority-stage, authorization, or body error for
/// `SUCCESSOR-ACTIVATION-1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SuccessorActivation1Error {
    /// Streaming preflight rejected truncation, trailing bytes, locator/external tag, or over-cap frame.
    PreflightRejected(&'static str),
    /// One of the internal-only successor artifact, edge, or byte-sum equations failed.
    EquationViolation(&'static str),
    /// Internal DAG violated rank monotonicity, bijection, external-ID exclusion, or prior-byte dedup.
    DagViolation(&'static str),
    /// Stage 1/2/3 predecessor, release, or activation authority check failed (purpose/replay/core/cert).
    AuthorityStageRejected(&'static str),
    /// Decoded preimage or sign-document projection disagreed with its canonical owner.
    BindingMismatch(&'static str),
    /// Detached `3-of-5` Ed25519 activation authorization failed verification.
    InvalidActivationAuthorization(&'static str),
    /// Dedicated migration body or availability commitment violated exclusivity rules.
    DedicatedMigrationBodyViolation(&'static str),
}

impl fmt::Display for SuccessorActivation1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for SuccessorActivation1Error {}

/// Protocol-frozen streaming preflight bounds enforced before allocation or nested decode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SuccessorPreflightCapsV1 {
    /// Maximum wire byte length of an entire [`SuccessorActivationBundleV1`] frame.
    pub max_bundle_frame_bytes: u64,
    /// Maximum summed internal canonical preimage bytes (`internal_canonical_preimage_bytes_u64`).
    pub max_internal_canonical_preimage_bytes: u64,
    /// Maximum byte length of any single embedded internal artifact preimage.
    pub max_single_preimage_bytes: u64,
    /// Maximum number of supplementary internal artifacts (`|S_s|`).
    pub max_supplementary_artifacts: u32,
    /// Maximum total internal dependency edges (`internal_dependency_edge_count_u32`).
    pub max_internal_dependency_edges: u32,
}

impl SuccessorPreflightCapsV1 {
    /// Protocol-frozen v1 conservative successor preflight caps.
    pub const V1_DEFAULT: Self = Self {
        max_bundle_frame_bytes: 1_048_576,
        max_internal_canonical_preimage_bytes: 1_048_576,
        max_single_preimage_bytes: 262_144,
        max_supplementary_artifacts: 64,
        max_internal_dependency_edges: 256,
    };
}

/// One internal artifact node in the internal-only successor dependency DAG.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuccessorActivationInternalDependencyNodeV1 {
    /// Checked topological rank (strictly greater than every internal dependency target's rank).
    pub topological_rank_u32: u32,
    /// Unique typed internal artifact identifier matching one explicit or supplementary preimage.
    pub typed_internal_artifact_id: [u8; 32],
    /// Strictly sorted & unique internal-only dependency target IDs (zero external variants exist).
    pub internal_dependency_refs: Vec<[u8; 32]>,
}

/// One embedded supplementary internal artifact (`SupplementarySuccessorActivationArtifactV1`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SupplementarySuccessorActivationArtifactV1 {
    /// Canonical schema string identifying the internal artifact codec.
    pub typed_artifact_schema_id: Vec<u8>,
    /// Recomputed typed internal artifact identifier.
    pub typed_artifact_id: [u8; 32],
    /// Declared canonical preimage length (`== canonical_artifact_preimage_bytes.len()`).
    pub canonical_artifact_preimage_length_u64: u64,
    /// Exact embedded canonical preimage bytes (embedded-only; no locator variant exists).
    pub canonical_artifact_preimage_bytes: Vec<u8>,
}

/// Zero-allocation summary returned by [`preflight_successor_activation_bundle_bytes`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreflightSuccessorBundleSummaryV1 {
    /// Verified `internal_artifact_count_u32` (`7 + |S_s| = |D_s|`).
    pub internal_artifact_count_u32: u32,
    /// Verified `supplementary_internal_artifact_count_u32` (`|S_s|`).
    pub supplementary_internal_artifact_count_u32: u32,
    /// Verified `internal_dependency_edge_count_u32`.
    pub internal_dependency_edge_count_u32: u32,
    /// Verified `internal_canonical_preimage_bytes_u64`.
    pub internal_canonical_preimage_bytes_u64: u64,
    /// Verified explicit internal preimage lengths `[E0..E6]`.
    pub explicit_internal_preimage_lengths_u64: [u64; 7],
    /// Declared `predecessor_admitted_profile_id` from fixed prefix.
    pub predecessor_admitted_profile_id: [u8; 32],
    /// Declared `successor_admitted_profile_id` from fixed prefix.
    pub successor_admitted_profile_id: [u8; 32],
}

/// Complete internal-only embedded successor closure bundle
/// (`bitblock/successor-activation-bundle/v1`, `external_root_count = 0`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuccessorActivationBundleV1 {
    /// Schema version (`1`).
    pub schema_version: u16,
    /// Already-admitted predecessor profile ID (`old_profile_id`).
    pub predecessor_admitted_profile_id: [u8; 32],
    /// Target successor admitted profile ID (`new_profile_id != old_profile_id`).
    pub successor_admitted_profile_id: [u8; 32],
    /// Target `ProfileAdmissionManifestV1` ID (`CE-29`/`CE-30`/`CE-31`).
    pub successor_profile_admission_manifest_id: [u8; 32],
    /// Target `ResolvedProfileTransitionSpecV1` ID.
    pub successor_transition_spec_at_profile_id: [u8; 32],
    /// Target `AdmittedReleaseRecordV1` ID (verified under Stage 2 `ReleaseAuthority`).
    pub successor_release_record_id: [u8; 32],
    /// Concrete `UPGRADE-1` `MigrationSpecV1` ID.
    pub migration_spec_id: [u8; 32],
    /// Concrete `UPGRADE-1` `SuccessorUpgradeEvidenceManifestV1` ID.
    pub successor_upgrade_evidence_manifest_id: [u8; 32],
    /// Declared `internal_artifact_count_u32` (`7 + |S_s| = |D_s|`).
    pub internal_artifact_count_u32: u32,
    /// Declared `supplementary_internal_artifact_count_u32` (`|S_s|`).
    pub supplementary_internal_artifact_count_u32: u32,
    /// Declared `internal_dependency_edge_count_u32`.
    pub internal_dependency_edge_count_u32: u32,
    /// Declared `internal_canonical_preimage_bytes_u64` (`sum(E_s) + sum(S_s)`).
    pub internal_canonical_preimage_bytes_u64: u64,
    /// Exact declared byte lengths of the 7 explicit internal preimages `[E0..E6]`.
    pub explicit_internal_preimage_lengths_u64: [u64; 7],
    /// Explicit internal preimage 0: canonical `ProfileScopeV1` bytes.
    pub successor_profile_scope_bytes: Vec<u8>,
    /// Explicit internal preimage 1: canonical `DomainRegistryV1` bytes.
    pub successor_domain_registry_bytes: Vec<u8>,
    /// Explicit internal preimage 2: canonical `BoundProfileManifestV1` bytes.
    pub successor_candidate_profile_bytes: Vec<u8>,
    /// Explicit internal preimage 3: canonical `ResolvedProfileTransitionSpecV1` bytes.
    pub successor_transition_spec_bytes: Vec<u8>,
    /// Explicit internal preimage 4: canonical `ProfileAdmissionManifestV1` bytes.
    pub successor_profile_admission_manifest_bytes: Vec<u8>,
    /// Explicit internal preimage 5: canonical `AdmittedReleaseRecordV1` bytes.
    pub successor_admitted_release_record_bytes: Vec<u8>,
    /// Explicit internal preimage 6: canonical `SuccessorUpgradeEvidenceManifestV1` bytes.
    pub successor_upgrade_evidence_manifest_bytes: Vec<u8>,
    /// Strictly sorted unique internal-only dependency nodes `D_s` (`|D_s| = 7 + |S_s|`).
    pub internal_dependency_nodes: Vec<SuccessorActivationInternalDependencyNodeV1>,
    /// Strictly sorted unique embedded supplementary internal artifacts `S_s`.
    pub supplementary_internal_artifacts: Vec<SupplementarySuccessorActivationArtifactV1>,
}

/// Purpose-specific historical accepted parent-authority use reference embedded inside
/// [`SuccessorActivationSignDocV1`] (`HistoricalAcceptedParentAuthorityUseRefV1`, `148` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HistoricalAcceptedParentAuthorityUseRefV1 {
    /// Admitted live consensus-native parent certificate digest (`cert.tag().digest()`).
    pub parent_cert_id: [u8; 32],
    /// Admitted live certificate canonical admission ID (`cert.admission_id()`).
    pub parent_cert_admission_id: [u8; 32],
    /// Exact evidence commitment inside the live certificate.
    pub evidence_commitment_id: [u8; 32],
    /// Exact occurrence-universe root covering all decoded occurrences and conflict witnesses.
    pub occurrence_universe_root: [u8; 32],
    /// Resolver-selected required consensus position.
    pub required_position: u64,
    /// Attained live consensus-native status (`AcceptedLive` or `UpgradedFinalized`).
    pub attained_status: ParentAuthorityStatusV1,
    /// Purpose tag (MUST be [`ParentAuthorityPurposeV1::ActivationAuthority`] in activation docs).
    pub purpose: ParentAuthorityPurposeV1,
    /// Count of decoded conflict-witness components preserved inside total occurrence coverage.
    pub decoded_conflict_component_count: u16,
    /// Count of total occurrence receipts covered by `occurrence_universe_root`.
    pub covered_occurrence_receipt_count: u32,
}

impl HistoricalAcceptedParentAuthorityUseRefV1 {
    /// Fixed canonical big-endian wire length (`144` bytes).
    pub const WIRE_LEN: usize = 32 * 4 + 8 + 1 + 1 + 2 + 4;

    /// Derives the canonical historical ref directly from an admitted live `CONSENSUS-1` cert.
    pub fn from_live_cert(
        cert: &AcceptedConsensusNativeParentAuthorityCertificateV1,
    ) -> Result<Self, SuccessorActivation1Error> {
        let parent_cert_id = cert
            .tag()
            .map_err(|_| {
                SuccessorActivation1Error::AuthorityStageRejected("invalid live cert tag")
            })?
            .digest();
        let parent_cert_admission_id = cert.admission_id().map_err(|_| {
            SuccessorActivation1Error::AuthorityStageRejected("invalid live cert admission_id")
        })?;
        let conflicts = cert
            .occurrence_receipts
            .iter()
            .filter(|r| r.has_conflict_material)
            .count();
        let decoded_conflict_component_count = u16::try_from(conflicts).map_err(|_| {
            SuccessorActivation1Error::AuthorityStageRejected("conflict count overflow")
        })?;
        let covered_occurrence_receipt_count = u32::try_from(cert.occurrence_receipts.len())
            .map_err(|_| {
                SuccessorActivation1Error::AuthorityStageRejected("occurrence count overflow")
            })?;
        Ok(Self {
            parent_cert_id,
            parent_cert_admission_id,
            evidence_commitment_id: cert.evidence_commitment_id,
            occurrence_universe_root: cert.occurrence_universe_root,
            required_position: cert.consumer_context.required_position,
            attained_status: cert.attained_status,
            purpose: cert.consumer_context.purpose,
            decoded_conflict_component_count,
            covered_occurrence_receipt_count,
        })
    }

    /// Encodes into exact 144-byte canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> [u8; Self::WIRE_LEN] {
        let mut b = Vec::with_capacity(Self::WIRE_LEN);
        b.extend_from_slice(&self.parent_cert_id);
        b.extend_from_slice(&self.parent_cert_admission_id);
        b.extend_from_slice(&self.evidence_commitment_id);
        b.extend_from_slice(&self.occurrence_universe_root);
        b.extend_from_slice(&self.required_position.to_be_bytes());
        b.push(self.attained_status.code());
        b.push(self.purpose.code());
        b.extend_from_slice(&self.decoded_conflict_component_count.to_be_bytes());
        b.extend_from_slice(&self.covered_occurrence_receipt_count.to_be_bytes());
        b.try_into().expect("144")
    }

    /// Decodes exact 144-byte canonical big-endian wire bytes fail-closed.
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, SuccessorActivation1Error> {
        if raw.len() != Self::WIRE_LEN {
            return Err(SuccessorActivation1Error::PreflightRejected(
                "bad HistoricalAcceptedParentAuthorityUseRefV1 len",
            ));
        }
        let mut p = 0usize;
        let parent_cert_id = take_32(raw, &mut p)?;
        let parent_cert_admission_id = take_32(raw, &mut p)?;
        let evidence_commitment_id = take_32(raw, &mut p)?;
        let occurrence_universe_root = take_32(raw, &mut p)?;
        let required_position = take_u64(raw, &mut p)?;
        let attained_status = ParentAuthorityStatusV1::from_code(take_u8(raw, &mut p)?)
            .map_err(|_| SuccessorActivation1Error::PreflightRejected("bad attained_status"))?;
        let purpose = ParentAuthorityPurposeV1::from_code(take_u8(raw, &mut p)?)
            .map_err(|_| SuccessorActivation1Error::PreflightRejected("bad purpose"))?;
        let decoded_conflict_component_count = take_u16(raw, &mut p)?;
        let covered_occurrence_receipt_count = take_u32(raw, &mut p)?;
        Ok(Self {
            parent_cert_id,
            parent_cert_admission_id,
            evidence_commitment_id,
            occurrence_universe_root,
            required_position,
            attained_status,
            purpose,
            decoded_conflict_component_count,
            covered_occurrence_receipt_count,
        })
    }
}

/// Canonical successor activation signing document
/// (`bitblock/successor-activation-sign-doc/v1`, `799` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SuccessorActivationSignDocV1 {
    /// Schema version (`1`).
    pub schema_version: u16,
    /// Immutable launch manifest identity (`chain_launch_manifest_id`).
    pub chain_launch_manifest_id: [u8; 32],
    /// Predecessor `accepted_parent_lookup_id`.
    pub predecessor_accepted_parent_lookup_id: [u8; 32],
    /// Predecessor purpose tag (MUST be `ActivationAuthority`).
    pub predecessor_authority_purpose: ParentAuthorityPurposeV1,
    /// Predecessor governance authority root bound by the parent core.
    pub predecessor_governance_authority_root: [u8; 32],
    /// Exact activation-purpose historical certificate admission reference.
    pub historical_predecessor_activation_authority_use_ref:
        HistoricalAcceptedParentAuthorityUseRefV1,
    /// Active predecessor profile ID (`old_profile_id`).
    pub old_active_profile_id: [u8; 32],
    /// Target successor admitted profile ID (`new_profile_id`).
    pub new_admitted_profile_id: [u8; 32],
    /// Exact target `ProfileAdmissionManifestV1` ID.
    pub successor_profile_admission_manifest_id: [u8; 32],
    /// Exact `UPGRADE-1` `MigrationSpecV1` ID.
    pub migration_spec_id: [u8; 32],
    /// Exact completed internal-only `successor_activation_bundle_id`.
    pub successor_activation_bundle_id: [u8; 32],
    /// Scheduled activation ledger height.
    pub activation_height: u64,
    /// Typed pre-migration execution root reference under the predecessor profile.
    pub activation_old_execution_root_ref: ExecutionRootRefV1,
    /// Typed expected post-migration execution root reference under the successor profile.
    pub activation_expected_new_execution_root_ref: ExecutionRootRefV1,
    /// Byte-equal projection of the predecessor-resolved activation authority policy ID.
    pub activation_authority_policy_id: [u8; 32],
}

impl SuccessorActivationSignDocV1 {
    /// Fixed canonical big-endian wire byte length (`799` bytes).
    pub const WIRE_LEN: usize = 2
        + 32
        + 32
        + 1
        + 32
        + HistoricalAcceptedParentAuthorityUseRefV1::WIRE_LEN
        + 32 * 5
        + 8
        + ExecutionRootRefV1::WIRE_LEN * 2
        + 32;

    /// Encodes into exact 799-byte canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], SuccessorActivation1Error> {
        if self.schema_version != SUCCESSOR_ACTIVATION_1_VERSION
            || self.old_active_profile_id == self.new_admitted_profile_id
            || self.activation_height == 0
        {
            return Err(SuccessorActivation1Error::BindingMismatch(
                "invalid SuccessorActivationSignDocV1 fields",
            ));
        }
        let mut b = Vec::with_capacity(Self::WIRE_LEN);
        b.extend_from_slice(&self.schema_version.to_be_bytes());
        b.extend_from_slice(&self.chain_launch_manifest_id);
        b.extend_from_slice(&self.predecessor_accepted_parent_lookup_id);
        b.push(self.predecessor_authority_purpose.code());
        b.extend_from_slice(&self.predecessor_governance_authority_root);
        b.extend_from_slice(
            &self
                .historical_predecessor_activation_authority_use_ref
                .encode_canonical(),
        );
        b.extend_from_slice(&self.old_active_profile_id);
        b.extend_from_slice(&self.new_admitted_profile_id);
        b.extend_from_slice(&self.successor_profile_admission_manifest_id);
        b.extend_from_slice(&self.migration_spec_id);
        b.extend_from_slice(&self.successor_activation_bundle_id);
        b.extend_from_slice(&self.activation_height.to_be_bytes());
        b.extend_from_slice(&self.activation_old_execution_root_ref.encode_canonical());
        b.extend_from_slice(
            &self
                .activation_expected_new_execution_root_ref
                .encode_canonical(),
        );
        b.extend_from_slice(&self.activation_authority_policy_id);
        b.try_into()
            .map_err(|_| SuccessorActivation1Error::PreflightRejected("sign doc len mismatch"))
    }

    /// Decodes exact 799-byte canonical big-endian wire bytes fail-closed.
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, SuccessorActivation1Error> {
        if raw.len() != Self::WIRE_LEN {
            return Err(SuccessorActivation1Error::PreflightRejected(
                "invalid SuccessorActivationSignDocV1 len",
            ));
        }
        let mut p = 0usize;
        let schema_version = take_u16(raw, &mut p)?;
        let chain_launch_manifest_id = take_32(raw, &mut p)?;
        let predecessor_accepted_parent_lookup_id = take_32(raw, &mut p)?;
        let predecessor_authority_purpose =
            ParentAuthorityPurposeV1::from_code(take_u8(raw, &mut p)?).map_err(|_| {
                SuccessorActivation1Error::PreflightRejected("bad sign doc purpose")
            })?;
        let predecessor_governance_authority_root = take_32(raw, &mut p)?;
        let hist_end = p + HistoricalAcceptedParentAuthorityUseRefV1::WIRE_LEN;
        let historical_predecessor_activation_authority_use_ref =
            HistoricalAcceptedParentAuthorityUseRefV1::decode_canonical(&raw[p..hist_end])?;
        p = hist_end;
        let old_active_profile_id = take_32(raw, &mut p)?;
        let new_admitted_profile_id = take_32(raw, &mut p)?;
        let successor_profile_admission_manifest_id = take_32(raw, &mut p)?;
        let migration_spec_id = take_32(raw, &mut p)?;
        let successor_activation_bundle_id = take_32(raw, &mut p)?;
        let activation_height = take_u64(raw, &mut p)?;
        let old_ref_end = p + ExecutionRootRefV1::WIRE_LEN;
        let activation_old_execution_root_ref =
            ExecutionRootRefV1::decode_canonical(&raw[p..old_ref_end]).map_err(|_| {
                SuccessorActivation1Error::PreflightRejected("bad old execution root ref")
            })?;
        p = old_ref_end;
        let new_ref_end = p + ExecutionRootRefV1::WIRE_LEN;
        let activation_expected_new_execution_root_ref =
            ExecutionRootRefV1::decode_canonical(&raw[p..new_ref_end]).map_err(|_| {
                SuccessorActivation1Error::PreflightRejected("bad new execution root ref")
            })?;
        p = new_ref_end;
        let activation_authority_policy_id = take_32(raw, &mut p)?;
        Ok(Self {
            schema_version,
            chain_launch_manifest_id,
            predecessor_accepted_parent_lookup_id,
            predecessor_authority_purpose,
            predecessor_governance_authority_root,
            historical_predecessor_activation_authority_use_ref,
            old_active_profile_id,
            new_admitted_profile_id,
            successor_profile_admission_manifest_id,
            migration_spec_id,
            successor_activation_bundle_id,
            activation_height,
            activation_old_execution_root_ref,
            activation_expected_new_execution_root_ref,
            activation_authority_policy_id,
        })
    }

    /// Computes the canonical `Policy` `ContentTag` whose digest IS `successor_activation_sign_doc_id`.
    pub fn tag(&self) -> Result<ContentTag, SuccessorActivation1Error> {
        let enc = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            SUCCESSOR_ACTIVATION_1_SIGN_DOC_SCHEMA,
            enc.len() as u64,
            |w| w(&enc),
        )
        .expect("static schema and fixed length"))
    }
}

/// Predecessor-committed `3-of-5` Ed25519 activation authority policy
/// (`DEVNET-TEST-KEYS-NOT-PRODUCTION-CUSTODY`, seeds `61..=65`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustedActivationAuthorityPolicyV1 {
    /// Required signature threshold (`3`).
    pub threshold_m: u8,
    /// Total authorized keyset size (`5`).
    pub total_n: u8,
    /// Strictly sorted unique Ed25519 public keys bound by predecessor governance state.
    pub authorized_pubkeys: Vec<CanonicalEd25519PublicKeyV1>,
}

impl TrustedActivationAuthorityPolicyV1 {
    /// Deterministic `3-of-5` devnet test activation authority (`seeds 61..=65`).
    /// Explicitly non-production: `DEVNET-TEST-KEYS-NOT-PRODUCTION-CUSTODY`.
    pub fn devnet_three_of_five() -> (Self, Vec<(CanonicalEd25519PublicKeyV1, [u8; 32])>) {
        let mut pairs: Vec<(CanonicalEd25519PublicKeyV1, [u8; 32])> = (61u8..=65)
            .map(|s| {
                let seed = [s; 32];
                let pk = CanonicalEd25519PublicKeyV1::from_secret_seed(&seed)
                    .expect("deterministic devnet activation key");
                (pk, seed)
            })
            .collect();
        pairs.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
        let authorized_pubkeys = pairs.iter().map(|(pk, _)| *pk).collect();
        (
            Self {
                threshold_m: 3,
                total_n: 5,
                authorized_pubkeys,
            },
            pairs,
        )
    }

    /// Computes the canonical `activation_authority_policy_id` digest.
    pub fn policy_id(&self) -> Result<[u8; 32], SuccessorActivation1Error> {
        if self.threshold_m == 0
            || usize::from(self.total_n) != self.authorized_pubkeys.len()
            || self.threshold_m > self.total_n
        {
            return Err(SuccessorActivation1Error::InvalidActivationAuthorization(
                "invalid activation threshold parameters",
            ));
        }
        let mut h = Sha256::new();
        h.update(SA1_ACTIVATION_POLICY_PREFIX);
        h.update([self.threshold_m, self.total_n]);
        for (i, pk) in self.authorized_pubkeys.iter().enumerate() {
            if i > 0 && self.authorized_pubkeys[i - 1].as_bytes() >= pk.as_bytes() {
                return Err(SuccessorActivation1Error::InvalidActivationAuthorization(
                    "activation pubkeys must be strictly sorted and unique",
                ));
            }
            h.update(pk.as_bytes());
        }
        Ok(h.finalize().into())
    }
}

/// One authorized activation signer's Ed25519 signature inside [`ActivationAuthorizationV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ActivationSignerSignatureV1 {
    /// Zero-based index into `TrustedActivationAuthorityPolicyV1::authorized_pubkeys`.
    pub signer_index: u8,
    /// Canonical Ed25519 public key matching `authorized_pubkeys[signer_index]`.
    pub public_key: CanonicalEd25519PublicKeyV1,
    /// Strict RFC 8032 Ed25519 signature over `SuccessorActivationSignDocV1` (`GovernanceAction`).
    pub signature: CanonicalEd25519SignatureV1,
}

impl ActivationSignerSignatureV1 {
    /// Fixed canonical wire byte length (`1 + 32 + 64 = 97` bytes).
    pub const WIRE_LEN: usize = 97;
}

/// Detached `3-of-5` Ed25519 successor activation authorization
/// (`bitblock/successor-activation-authorization/v1`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivationAuthorizationV1 {
    /// Schema version (`1`).
    pub schema_version: u16,
    /// Complete [`SuccessorActivationSignDocV1`] signed by all signers.
    pub sign_doc: SuccessorActivationSignDocV1,
    /// Recomputed `successor_activation_sign_doc_id`.
    pub successor_activation_sign_doc_id: [u8; 32],
    /// Predecessor-resolved `activation_authority_policy_id`.
    pub activation_authority_policy_id: [u8; 32],
    /// Signature suite (`Ed25519StrictRfc8032V1`).
    pub signature_suite: SignatureSuiteIdV1,
    /// Credential-set / policy identifier (`== activation_authority_policy_id`).
    pub activation_authority_credential_set_id: [u8; 32],
    /// Strictly ascending unique `signer_index` Ed25519 signatures.
    pub signatures: Vec<ActivationSignerSignatureV1>,
}

impl ActivationAuthorizationV1 {
    /// Constructs the domain-separated `CRYPTO-D1` [`SigningPreimageV1`] under
    /// `SigningDomainTagV1::GovernanceAction` using the **old active** predecessor profile context.
    pub fn signing_preimage(
        chain_id: u64,
        genesis_id: [u8; 32],
        chain_launch_manifest_id: [u8; 32],
        old_active_profile_id: [u8; 32],
        sign_doc_id: [u8; 32],
    ) -> SigningPreimageV1 {
        SigningPreimageV1 {
            protocol_version: SUCCESSOR_ACTIVATION_1_VERSION,
            chain_id,
            genesis_id,
            launch_manifest_id: chain_launch_manifest_id,
            profile_id: old_active_profile_id,
            domain_tag: SigningDomainTagV1::GovernanceAction,
            signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
            hash_suite: HashSuiteIdV1::Sha256V1,
            payload_digest: sign_doc_id,
        }
    }

    /// Encodes into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, SuccessorActivation1Error> {
        if self.schema_version != SUCCESSOR_ACTIVATION_1_VERSION {
            return Err(SuccessorActivation1Error::PreflightRejected(
                "auth schema_version must be 1",
            ));
        }
        let sd_b = self.sign_doc.encode_canonical()?;
        let n_sigs = u16::try_from(self.signatures.len()).map_err(|_| {
            SuccessorActivation1Error::PreflightRejected("too many activation signatures")
        })?;
        let mut out = Vec::with_capacity(
            2 + sd_b.len()
                + 32
                + 32
                + 2
                + 32
                + 2
                + self.signatures.len() * ActivationSignerSignatureV1::WIRE_LEN,
        );
        out.extend_from_slice(&self.schema_version.to_be_bytes());
        out.extend_from_slice(&sd_b);
        out.extend_from_slice(&self.successor_activation_sign_doc_id);
        out.extend_from_slice(&self.activation_authority_policy_id);
        out.extend_from_slice(&self.signature_suite.code().to_be_bytes());
        out.extend_from_slice(&self.activation_authority_credential_set_id);
        out.extend_from_slice(&n_sigs.to_be_bytes());
        for s in &self.signatures {
            out.push(s.signer_index);
            out.extend_from_slice(s.public_key.as_bytes());
            out.extend_from_slice(s.signature.as_bytes());
        }
        Ok(out)
    }

    /// Decodes canonical big-endian wire bytes fail-closed.
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, SuccessorActivation1Error> {
        let mut p = 0usize;
        let schema_version = take_u16(raw, &mut p)?;
        let sd_end = p + SuccessorActivationSignDocV1::WIRE_LEN;
        if sd_end > raw.len() {
            return Err(SuccessorActivation1Error::PreflightRejected(
                "truncated ActivationAuthorizationV1 sign doc",
            ));
        }
        let sign_doc = SuccessorActivationSignDocV1::decode_canonical(&raw[p..sd_end])?;
        p = sd_end;
        let successor_activation_sign_doc_id = take_32(raw, &mut p)?;
        let activation_authority_policy_id = take_32(raw, &mut p)?;
        let signature_suite =
            SignatureSuiteIdV1::from_code(take_u16(raw, &mut p)?).map_err(|_| {
                SuccessorActivation1Error::PreflightRejected(
                    "unsupported activation signature suite",
                )
            })?;
        let activation_authority_credential_set_id = take_32(raw, &mut p)?;
        let n_sigs = usize::from(take_u16(raw, &mut p)?);
        if raw.len() != p + n_sigs * ActivationSignerSignatureV1::WIRE_LEN {
            return Err(SuccessorActivation1Error::PreflightRejected(
                "ActivationAuthorizationV1 trailing or truncated signature bytes",
            ));
        }
        let mut signatures = Vec::with_capacity(n_sigs.min(16));
        for _ in 0..n_sigs {
            let signer_index = take_u8(raw, &mut p)?;
            let pk_bytes = take_32(raw, &mut p)?;
            let sig_bytes = take_64(raw, &mut p)?;
            let public_key = CanonicalEd25519PublicKeyV1::from_bytes(pk_bytes).map_err(|_| {
                SuccessorActivation1Error::InvalidActivationAuthorization("bad activation pubkey")
            })?;
            let signature = CanonicalEd25519SignatureV1::from_bytes(sig_bytes).map_err(|_| {
                SuccessorActivation1Error::InvalidActivationAuthorization("bad activation sig")
            })?;
            signatures.push(ActivationSignerSignatureV1 {
                signer_index,
                public_key,
                signature,
            });
        }
        Ok(Self {
            schema_version,
            sign_doc,
            successor_activation_sign_doc_id,
            activation_authority_policy_id,
            signature_suite,
            activation_authority_credential_set_id,
            signatures,
        })
    }

    /// Computes the canonical `Receipt` `ContentTag` whose digest IS `activation_authorization_id`.
    pub fn tag(&self) -> Result<ContentTag, SuccessorActivation1Error> {
        let enc = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            SUCCESSOR_ACTIVATION_1_AUTH_SCHEMA,
            enc.len() as u64,
            |w| w(&enc),
        )
        .expect("static schema and bounded length"))
    }
}

/// Exact single system transition mandated inside [`DedicatedMigrationBodyV1`] (`136` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AuthorizedMigrationSystemTransitionV1 {
    /// Target `ProfileAdmissionManifestV1` ID.
    pub successor_profile_admission_manifest_id: [u8; 32],
    /// Concrete `UPGRADE-1` `MigrationSpecV1` ID.
    pub migration_spec_id: [u8; 32],
    /// Completed internal-only `successor_activation_bundle_id`.
    pub successor_activation_bundle_id: [u8; 32],
    /// Verified detached `activation_authorization_id`.
    pub activation_authorization_id: [u8; 32],
    /// Scheduled activation ledger height.
    pub activation_height: u64,
}

impl AuthorizedMigrationSystemTransitionV1 {
    /// Fixed canonical big-endian wire length (`136` bytes).
    pub const WIRE_LEN: usize = 32 * 4 + 8;

    /// Encodes into exact 136-byte canonical wire form.
    pub fn encode_canonical(&self) -> [u8; Self::WIRE_LEN] {
        let mut b = Vec::with_capacity(Self::WIRE_LEN);
        b.extend_from_slice(&self.successor_profile_admission_manifest_id);
        b.extend_from_slice(&self.migration_spec_id);
        b.extend_from_slice(&self.successor_activation_bundle_id);
        b.extend_from_slice(&self.activation_authorization_id);
        b.extend_from_slice(&self.activation_height.to_be_bytes());
        b.try_into().expect("136")
    }

    /// Decodes exact 136-byte canonical wire form fail-closed.
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, SuccessorActivation1Error> {
        if raw.len() != Self::WIRE_LEN {
            return Err(SuccessorActivation1Error::DedicatedMigrationBodyViolation(
                "bad AuthorizedMigrationSystemTransitionV1 len",
            ));
        }
        let mut p = 0usize;
        Ok(Self {
            successor_profile_admission_manifest_id: take_32(raw, &mut p)?,
            migration_spec_id: take_32(raw, &mut p)?,
            successor_activation_bundle_id: take_32(raw, &mut p)?,
            activation_authorization_id: take_32(raw, &mut p)?,
            activation_height: take_u64(raw, &mut p)?,
        })
    }
}

/// Dedicated migration block body (`DedicatedMigrationBodyV1`, `LAYER1_DESIGN_PAPER.md` §2).
/// Must carry zero signed transactions, zero application subticks, and exactly one
/// [`AuthorizedMigrationSystemTransitionV1`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DedicatedMigrationBodyV1 {
    /// Must strictly be `0` (`empty_signed_transaction_sequence`).
    pub signed_transaction_count: u32,
    /// Must strictly be `0` (`application_subtick_schedule` empty).
    pub application_subtick_count: u32,
    /// Must strictly have length `1` (`AuthorizedMigrationSystemTransitionV1`).
    pub system_transitions: Vec<AuthorizedMigrationSystemTransitionV1>,
    /// Deterministic migration parameter/state delta digest matching `UPGRADE-1`.
    pub deterministic_migration_inputs_digest: [u8; 32],
}

impl DedicatedMigrationBodyV1 {
    /// Encodes into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, SuccessorActivation1Error> {
        let n_sys = u16::try_from(self.system_transitions.len()).map_err(|_| {
            SuccessorActivation1Error::DedicatedMigrationBodyViolation("system transition overflow")
        })?;
        let mut b = Vec::with_capacity(
            4 + 4
                + 2
                + self.system_transitions.len() * AuthorizedMigrationSystemTransitionV1::WIRE_LEN
                + 32,
        );
        b.extend_from_slice(&self.signed_transaction_count.to_be_bytes());
        b.extend_from_slice(&self.application_subtick_count.to_be_bytes());
        b.extend_from_slice(&n_sys.to_be_bytes());
        for t in &self.system_transitions {
            b.extend_from_slice(&t.encode_canonical());
        }
        b.extend_from_slice(&self.deterministic_migration_inputs_digest);
        Ok(b)
    }

    /// Decodes canonical big-endian wire bytes fail-closed.
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, SuccessorActivation1Error> {
        let mut p = 0usize;
        let signed_transaction_count = take_u32(raw, &mut p)?;
        let application_subtick_count = take_u32(raw, &mut p)?;
        let n_sys = usize::from(take_u16(raw, &mut p)?);
        let expected_len = p + n_sys * AuthorizedMigrationSystemTransitionV1::WIRE_LEN + 32;
        if raw.len() != expected_len {
            return Err(SuccessorActivation1Error::DedicatedMigrationBodyViolation(
                "DedicatedMigrationBodyV1 byte length mismatch",
            ));
        }
        let mut system_transitions = Vec::with_capacity(n_sys.min(4));
        for _ in 0..n_sys {
            let end = p + AuthorizedMigrationSystemTransitionV1::WIRE_LEN;
            system_transitions.push(AuthorizedMigrationSystemTransitionV1::decode_canonical(
                &raw[p..end],
            )?);
            p = end;
        }
        let deterministic_migration_inputs_digest = take_32(raw, &mut p)?;
        Ok(Self {
            signed_transaction_count,
            application_subtick_count,
            system_transitions,
            deterministic_migration_inputs_digest,
        })
    }
}

/// Old-profile envelope availability carrier (`bitblock/successor-activation-1/carrier/v1`).
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum SuccessorActivationCarrierV1 {
    /// Ordinary non-migration block carrier (wire tag `0`).
    NoSuccessorActivationV1,
    /// In-place migration block availability carrier (wire tag `1`).
    EmbeddedSuccessorActivationV1 {
        /// Complete internal-only [`SuccessorActivationBundleV1`].
        bundle: SuccessorActivationBundleV1,
        /// Recomputed `successor_activation_bundle_id`.
        successor_activation_bundle_id: [u8; 32],
        /// Complete detached `3-of-5` [`ActivationAuthorizationV1`].
        authorization: ActivationAuthorizationV1,
        /// Recomputed `activation_authorization_id`.
        activation_authorization_id: [u8; 32],
        /// Exclusive [`DedicatedMigrationBodyV1`] executed only after closure acceptance.
        dedicated_migration_body: DedicatedMigrationBodyV1,
        /// Old-profile envelope availability commitment binding bundle + authorization + body.
        availability_commitment: [u8; 32],
    },
}

impl SuccessorActivationCarrierV1 {
    /// Computes the old-profile envelope availability commitment over `(old_profile_id, bundle_bytes, auth_bytes, body_bytes)`.
    pub fn compute_availability_commitment(
        old_profile_id: [u8; 32],
        bundle_bytes: &[u8],
        auth_bytes: &[u8],
        body_bytes: &[u8],
    ) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(SA1_AVAIL_COMMIT_PREFIX);
        h.update(old_profile_id);
        h.update((bundle_bytes.len() as u64).to_be_bytes());
        h.update(bundle_bytes);
        h.update((auth_bytes.len() as u64).to_be_bytes());
        h.update(auth_bytes);
        h.update((body_bytes.len() as u64).to_be_bytes());
        h.update(body_bytes);
        h.finalize().into()
    }

    /// Encodes into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, SuccessorActivation1Error> {
        match self {
            Self::NoSuccessorActivationV1 => {
                let mut out = Vec::with_capacity(3);
                out.extend_from_slice(&SUCCESSOR_ACTIVATION_1_VERSION.to_be_bytes());
                out.push(0);
                Ok(out)
            }
            Self::EmbeddedSuccessorActivationV1 {
                bundle,
                successor_activation_bundle_id,
                authorization,
                activation_authorization_id,
                dedicated_migration_body,
                availability_commitment,
            } => {
                let bundle_b = bundle.encode_canonical()?;
                let auth_b = authorization.encode_canonical()?;
                let body_b = dedicated_migration_body.encode_canonical()?;
                let mut out = Vec::with_capacity(
                    3 + 4 + bundle_b.len() + 32 + 4 + auth_b.len() + 32 + 4 + body_b.len() + 32,
                );
                out.extend_from_slice(&SUCCESSOR_ACTIVATION_1_VERSION.to_be_bytes());
                out.push(1);
                out.extend_from_slice(&(bundle_b.len() as u32).to_be_bytes());
                out.extend_from_slice(&bundle_b);
                out.extend_from_slice(successor_activation_bundle_id);
                out.extend_from_slice(&(auth_b.len() as u32).to_be_bytes());
                out.extend_from_slice(&auth_b);
                out.extend_from_slice(activation_authorization_id);
                out.extend_from_slice(&(body_b.len() as u32).to_be_bytes());
                out.extend_from_slice(&body_b);
                out.extend_from_slice(availability_commitment);
                Ok(out)
            }
        }
    }

    /// Decodes canonical big-endian wire bytes fail-closed (preflighting bundle before allocation).
    pub fn decode_canonical(
        raw: &[u8],
        caps: SuccessorPreflightCapsV1,
    ) -> Result<Self, SuccessorActivation1Error> {
        let mut p = 0usize;
        let ver = take_u16(raw, &mut p)?;
        if ver != SUCCESSOR_ACTIVATION_1_VERSION {
            return Err(SuccessorActivation1Error::PreflightRejected(
                "bad carrier schema_version",
            ));
        }
        let tag = take_u8(raw, &mut p)?;
        match tag {
            0 => {
                if p != raw.len() {
                    return Err(SuccessorActivation1Error::PreflightRejected(
                        "trailing bytes in NoSuccessorActivationV1",
                    ));
                }
                Ok(Self::NoSuccessorActivationV1)
            }
            1 => {
                let b_len = take_u32(raw, &mut p)? as usize;
                let b_slice = take_slice(raw, &mut p, b_len)?;
                let _ = preflight_successor_activation_bundle_bytes(b_slice, caps)?;
                let bundle = SuccessorActivationBundleV1::decode_canonical(b_slice, caps)?;
                let successor_activation_bundle_id = take_32(raw, &mut p)?;
                let a_len = take_u32(raw, &mut p)? as usize;
                let a_slice = take_slice(raw, &mut p, a_len)?;
                let authorization = ActivationAuthorizationV1::decode_canonical(a_slice)?;
                let activation_authorization_id = take_32(raw, &mut p)?;
                let d_len = take_u32(raw, &mut p)? as usize;
                let d_slice = take_slice(raw, &mut p, d_len)?;
                let dedicated_migration_body = DedicatedMigrationBodyV1::decode_canonical(d_slice)?;
                let availability_commitment = take_32(raw, &mut p)?;
                if p != raw.len() {
                    return Err(SuccessorActivation1Error::PreflightRejected(
                        "trailing bytes in EmbeddedSuccessorActivationV1",
                    ));
                }
                Ok(Self::EmbeddedSuccessorActivationV1 {
                    bundle,
                    successor_activation_bundle_id,
                    authorization,
                    activation_authorization_id,
                    dedicated_migration_body,
                    availability_commitment,
                })
            }
            _ => Err(SuccessorActivation1Error::PreflightRejected(
                "unknown SuccessorActivationCarrierV1 discriminant",
            )),
        }
    }

    /// Computes the canonical `Receipt` `ContentTag` for this carrier.
    pub fn tag(&self) -> Result<ContentTag, SuccessorActivation1Error> {
        let enc = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            SUCCESSOR_ACTIVATION_1_CARRIER_SCHEMA,
            enc.len() as u64,
            |w| w(&enc),
        )
        .expect("static schema and bounded length"))
    }
}

/// Purpose-specific verified parent-authority eligibility result (non-serialized semantic value).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerifiedParentEligibilityV1 {
    /// Shared predecessor `accepted_parent_lookup_id`.
    pub accepted_parent_lookup_id: [u8; 32],
    /// Shared unique signature-free `PROTOCOL-ID-1` parent proposal core ID.
    pub unique_accepted_proposal_core_block_id: [u8; 32],
    /// Shared predecessor governance authority root bound by the parent core.
    pub governance_authority_root: [u8; 32],
    /// Purpose-specific tag (`ReleaseAuthority` vs `ActivationAuthority`, NEVER interchangeable).
    pub purpose: ParentAuthorityPurposeV1,
    /// Purpose-specific certificate/authority policy digest.
    pub policy_id: [u8; 32],
    /// Purpose-specific eligibility predicate digest.
    pub eligibility_predicate_id: [u8; 32],
    /// Attained consensus-native status (`AcceptedLive` or `UpgradedFinalized`).
    pub status: ParentAuthorityStatusV1,
    /// Required consensus position for this purpose.
    pub required_position: u64,
    /// Authorized evidence cutoff position (`>= earliest_unbounded_valid_position`).
    pub evidence_cutoff_position: u64,
    /// Exact evidence commitment covering the complete occurrence universe at cutoff.
    pub evidence_commitment_id: [u8; 32],
    /// Exact occurrence-universe root covering every decoded occurrence and conflict witness.
    pub occurrence_universe_root: [u8; 32],
    /// Count of decoded occurrences covered by `occurrence_universe_root`.
    pub total_occurrence_count: u32,
}

/// **Stage 1 (Non-serialized semantic verifier context):** Pre-bundle predecessor base context
/// carrying distinct `verified_release_parent_eligibility` and
/// `verified_activation_parent_eligibility` over ONE shared predecessor core & governance root.
#[derive(Clone, Debug)]
pub struct VerifiedSuccessorPredecessorBaseContextV1 {
    /// Immutable `chain_launch_manifest_id`.
    pub chain_launch_manifest_id: [u8; 32],
    /// Immutable chain `GenesisPackageV1`.
    pub genesis_package: GenesisPackageV1,
    /// Immutable `GenesisRootParityRecordV1`.
    pub launch_parity_record: GenesisRootParityRecordV1,
    /// Predecessor active profile ID (`old_profile_id`).
    pub old_profile_id: [u8; 32],
    /// Predecessor `ProfileScopeV1` digest (for duplicate prior-byte detection).
    pub old_profile_scope_id: [u8; 32],
    /// Predecessor `ResolvedProfileTransitionSpecV1` digest (for duplicate prior-byte detection).
    pub old_transition_spec_id: [u8; 32],
    /// Predecessor active `ExecutorForkContextV1`.
    pub fork_context: ExecutorForkContextV1,
    /// Active predecessor epoch validator schedule verifying live `CONSENSUS-1` parent certs.
    pub active_schedule: EpochValidatorScheduleV1,
    /// Predecessor typed execution-root reference at migration entry.
    pub old_execution_root_ref: ExecutionRootRefV1,
    /// Predecessor-resolved `3-of-5` release authority policy (`RELEASE-1`).
    pub trusted_release_policy: TrustedReleaseAuthorityPolicyV1,
    /// Predecessor-resolved `3-of-5` activation authority policy (`SUCCESSOR-ACTIVATION-1`).
    pub trusted_activation_policy: TrustedActivationAuthorityPolicyV1,
    /// Distinct Stage 1 eligibility for `ReleaseAuthority` purpose.
    pub verified_release_parent_eligibility: VerifiedParentEligibilityV1,
    /// Distinct Stage 1 eligibility for `ActivationAuthority` purpose.
    pub verified_activation_parent_eligibility: VerifiedParentEligibilityV1,
}

impl VerifiedSuccessorPredecessorBaseContextV1 {
    /// Validates Stage 1 structural integrity: shared parent core & governance root, strictly
    /// distinct purposes (`ReleaseAuthority` vs `ActivationAuthority`), distinct policy/predicate
    /// bindings, and non-tentative live attainment.
    pub fn validate(&self) -> Result<(), SuccessorActivation1Error> {
        let rel = &self.verified_release_parent_eligibility;
        let act = &self.verified_activation_parent_eligibility;
        if rel.purpose != ParentAuthorityPurposeV1::ReleaseAuthority {
            return Err(SuccessorActivation1Error::AuthorityStageRejected(
                "verified_release_parent_eligibility must carry ReleaseAuthority purpose",
            ));
        }
        if act.purpose != ParentAuthorityPurposeV1::ActivationAuthority {
            return Err(SuccessorActivation1Error::AuthorityStageRejected(
                "verified_activation_parent_eligibility must carry ActivationAuthority purpose",
            ));
        }
        if rel.accepted_parent_lookup_id != act.accepted_parent_lookup_id
            || rel.unique_accepted_proposal_core_block_id
                != act.unique_accepted_proposal_core_block_id
            || rel.governance_authority_root != act.governance_authority_root
        {
            return Err(SuccessorActivation1Error::AuthorityStageRejected(
                "release and activation eligibilities disagree on shared predecessor core or governance root",
            ));
        }
        if rel.accepted_parent_lookup_id == [0u8; 32]
            || rel.unique_accepted_proposal_core_block_id == [0u8; 32]
            || rel.governance_authority_root == [0u8; 32]
        {
            return Err(SuccessorActivation1Error::AuthorityStageRejected(
                "zero predecessor lookup, core, or governance root",
            ));
        }
        if rel.policy_id == act.policy_id
            || rel.eligibility_predicate_id == act.eligibility_predicate_id
        {
            return Err(SuccessorActivation1Error::AuthorityStageRejected(
                "release and activation eligibilities must retain separate policies and predicates",
            ));
        }
        if rel.status == ParentAuthorityStatusV1::Tentative
            || act.status == ParentAuthorityStatusV1::Tentative
        {
            return Err(SuccessorActivation1Error::AuthorityStageRejected(
                "tentative parent authority status cannot verify live successor stages",
            ));
        }
        Ok(())
    }
}

/// **Stage 2 (Non-serialized semantic verifier context):** Release-authority context constructed
/// ONLY after `admitted_release_core_id` exists in the internal DAG, verifying the live
/// `ReleaseAuthority` `CONSENSUS-1` certificate before accepting `AdmittedReleaseRecordV1`.
#[derive(Clone, Debug)]
pub struct VerifiedSuccessorReleaseAuthorityContextV1 {
    /// Bound authorization-free `admitted_release_core_id` (`consumer_object_digest`).
    pub admitted_release_core_id: [u8; 32],
    /// Verified certificate-independent consumer context (`ReleaseAuthority`).
    pub consumer_context: ParentAuthorityCertificateConsumerContextV1,
    /// Verified live `CONSENSUS-1` `ReleaseAuthority` certificate digest.
    pub release_cert_id: [u8; 32],
    /// Verified live `CONSENSUS-1` `ReleaseAuthority` admission ID.
    pub release_cert_admission_id: [u8; 32],
    /// Verified `RELEASE-1` `admitted_release_record_id`.
    pub admitted_release_record_id: [u8; 32],
}

/// **Stage 3 (Non-serialized semantic verifier context):** Activation-authority context constructed
/// ONLY AFTER the complete internal-only bundle yields `successor_activation_bundle_id`, verifying
/// the distinct live `ActivationAuthority` `CONSENSUS-1` certificate before accepting
/// [`SuccessorActivationSignDocV1`] and [`ActivationAuthorizationV1`].
#[derive(Clone, Debug)]
pub struct VerifiedSuccessorActivationAuthorityContextV1 {
    /// Bound completed `successor_activation_bundle_id` (`consumer_object_digest`).
    pub successor_activation_bundle_id: [u8; 32],
    /// Verified certificate-independent consumer context (`ActivationAuthority`).
    pub consumer_context: ParentAuthorityCertificateConsumerContextV1,
    /// Verified live `CONSENSUS-1` `ActivationAuthority` certificate digest.
    pub activation_cert_id: [u8; 32],
    /// Verified live `CONSENSUS-1` `ActivationAuthority` admission ID.
    pub activation_cert_admission_id: [u8; 32],
    /// Verified historical activation-authority use reference bound inside the sign doc.
    pub historical_activation_use_ref: HistoricalAcceptedParentAuthorityUseRefV1,
    /// Verified `successor_activation_sign_doc_id`.
    pub successor_activation_sign_doc_id: [u8; 32],
    /// Verified `activation_authorization_id`.
    pub activation_authorization_id: [u8; 32],
}

/// Complete verified outcome of [`verify_successor_activation_carrier`].
#[derive(Clone, Debug)]
pub struct VerifiedSuccessorActivationOutcomeV1 {
    /// Zero-allocation streaming preflight summary.
    pub preflight: PreflightSuccessorBundleSummaryV1,
    /// Verified Stage 2 release-authority context.
    pub release_stage: VerifiedSuccessorReleaseAuthorityContextV1,
    /// Verified Stage 3 activation-authority context.
    pub activation_stage: VerifiedSuccessorActivationAuthorityContextV1,
    /// Verified `successor_profile_admission_manifest_id`.
    pub successor_profile_admission_manifest_id: [u8; 32],
    /// Verified `successor_upgrade_evidence_manifest_id`.
    pub successor_upgrade_evidence_manifest_id: [u8; 32],
    /// Verified `activation_height`.
    pub activation_height: u64,
    /// Verified post-migration expected execution-root reference.
    pub expected_new_execution_root_ref: ExecutionRootRefV1,
}

impl SuccessorActivationBundleV1 {
    /// Encodes into prefix-first canonical big-endian wire bytes (embedded-only, zero external roots).
    pub fn encode_canonical(&self) -> Result<Vec<u8>, SuccessorActivation1Error> {
        if self.schema_version != SUCCESSOR_ACTIVATION_1_VERSION {
            return Err(SuccessorActivation1Error::PreflightRejected(
                "bundle schema_version must be 1",
            ));
        }
        let mut out = Vec::with_capacity(8192);
        out.extend_from_slice(&self.schema_version.to_be_bytes());
        out.extend_from_slice(&self.predecessor_admitted_profile_id);
        out.extend_from_slice(&self.successor_admitted_profile_id);
        out.extend_from_slice(&self.successor_profile_admission_manifest_id);
        out.extend_from_slice(&self.successor_transition_spec_at_profile_id);
        out.extend_from_slice(&self.successor_release_record_id);
        out.extend_from_slice(&self.migration_spec_id);
        out.extend_from_slice(&self.successor_upgrade_evidence_manifest_id);
        out.extend_from_slice(&self.internal_artifact_count_u32.to_be_bytes());
        out.extend_from_slice(&self.supplementary_internal_artifact_count_u32.to_be_bytes());
        out.extend_from_slice(&self.internal_dependency_edge_count_u32.to_be_bytes());
        out.extend_from_slice(&self.internal_canonical_preimage_bytes_u64.to_be_bytes());
        for &len in &self.explicit_internal_preimage_lengths_u64 {
            out.extend_from_slice(&len.to_be_bytes());
        }
        let explicit: [&[u8]; 7] = [
            &self.successor_profile_scope_bytes,
            &self.successor_domain_registry_bytes,
            &self.successor_candidate_profile_bytes,
            &self.successor_transition_spec_bytes,
            &self.successor_profile_admission_manifest_bytes,
            &self.successor_admitted_release_record_bytes,
            &self.successor_upgrade_evidence_manifest_bytes,
        ];
        for bytes in explicit {
            out.push(SUCCESSOR_EMBEDDED_CARRIER_MODE_V1);
            out.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
            out.extend_from_slice(bytes);
        }
        let n_nodes = u32::try_from(self.internal_dependency_nodes.len())
            .map_err(|_| SuccessorActivation1Error::PreflightRejected("node count overflow"))?;
        out.extend_from_slice(&n_nodes.to_be_bytes());
        for node in &self.internal_dependency_nodes {
            out.extend_from_slice(&node.topological_rank_u32.to_be_bytes());
            out.extend_from_slice(&node.typed_internal_artifact_id);
            let n_deps = u16::try_from(node.internal_dependency_refs.len())
                .map_err(|_| SuccessorActivation1Error::PreflightRejected("dep count overflow"))?;
            out.extend_from_slice(&n_deps.to_be_bytes());
            for dep_id in &node.internal_dependency_refs {
                out.push(INTERNAL_DEP_REF_TAG_V1);
                out.extend_from_slice(dep_id);
            }
        }
        let n_supp = u32::try_from(self.supplementary_internal_artifacts.len())
            .map_err(|_| SuccessorActivation1Error::PreflightRejected("supp count overflow"))?;
        out.extend_from_slice(&n_supp.to_be_bytes());
        for supp in &self.supplementary_internal_artifacts {
            out.push(SUCCESSOR_EMBEDDED_CARRIER_MODE_V1);
            let slen = u16::try_from(supp.typed_artifact_schema_id.len()).map_err(|_| {
                SuccessorActivation1Error::PreflightRejected("schema_id len overflow")
            })?;
            out.extend_from_slice(&slen.to_be_bytes());
            out.extend_from_slice(&supp.typed_artifact_schema_id);
            out.extend_from_slice(&supp.typed_artifact_id);
            out.extend_from_slice(&supp.canonical_artifact_preimage_length_u64.to_be_bytes());
            out.extend_from_slice(&supp.canonical_artifact_preimage_bytes);
        }
        Ok(out)
    }

    /// Decodes a canonical [`SuccessorActivationBundleV1`] AFTER zero-allocation preflight succeeds.
    pub fn decode_canonical(
        raw: &[u8],
        caps: SuccessorPreflightCapsV1,
    ) -> Result<Self, SuccessorActivation1Error> {
        let _ = preflight_successor_activation_bundle_bytes(raw, caps)?;
        let mut p = 0usize;
        let schema_version = take_u16(raw, &mut p)?;
        let predecessor_admitted_profile_id = take_32(raw, &mut p)?;
        let successor_admitted_profile_id = take_32(raw, &mut p)?;
        let successor_profile_admission_manifest_id = take_32(raw, &mut p)?;
        let successor_transition_spec_at_profile_id = take_32(raw, &mut p)?;
        let successor_release_record_id = take_32(raw, &mut p)?;
        let migration_spec_id = take_32(raw, &mut p)?;
        let successor_upgrade_evidence_manifest_id = take_32(raw, &mut p)?;
        let internal_artifact_count_u32 = take_u32(raw, &mut p)?;
        let supplementary_internal_artifact_count_u32 = take_u32(raw, &mut p)?;
        let internal_dependency_edge_count_u32 = take_u32(raw, &mut p)?;
        let internal_canonical_preimage_bytes_u64 = take_u64(raw, &mut p)?;
        let mut explicit_internal_preimage_lengths_u64 = [0u64; 7];
        for slot in &mut explicit_internal_preimage_lengths_u64 {
            *slot = take_u64(raw, &mut p)?;
        }
        let mut expl_vecs = Vec::with_capacity(7);
        for _ in 0..7 {
            let _mode = take_u8(raw, &mut p)?;
            let elen = take_u64(raw, &mut p)? as usize;
            expl_vecs.push(take_slice(raw, &mut p, elen)?.to_vec());
        }
        let n_nodes = take_u32(raw, &mut p)? as usize;
        let mut internal_dependency_nodes = Vec::with_capacity(n_nodes);
        for _ in 0..n_nodes {
            let topological_rank_u32 = take_u32(raw, &mut p)?;
            let typed_internal_artifact_id = take_32(raw, &mut p)?;
            let n_deps = usize::from(take_u16(raw, &mut p)?);
            let mut internal_dependency_refs = Vec::with_capacity(n_deps);
            for _ in 0..n_deps {
                let _tag = take_u8(raw, &mut p)?;
                internal_dependency_refs.push(take_32(raw, &mut p)?);
            }
            internal_dependency_nodes.push(SuccessorActivationInternalDependencyNodeV1 {
                topological_rank_u32,
                typed_internal_artifact_id,
                internal_dependency_refs,
            });
        }
        let n_supp = take_u32(raw, &mut p)? as usize;
        let mut supplementary_internal_artifacts = Vec::with_capacity(n_supp);
        for _ in 0..n_supp {
            let _mode = take_u8(raw, &mut p)?;
            let slen = usize::from(take_u16(raw, &mut p)?);
            let typed_artifact_schema_id = take_slice(raw, &mut p, slen)?.to_vec();
            let typed_artifact_id = take_32(raw, &mut p)?;
            let canonical_artifact_preimage_length_u64 = take_u64(raw, &mut p)?;
            let pre_bytes =
                take_slice(raw, &mut p, canonical_artifact_preimage_length_u64 as usize)?.to_vec();
            supplementary_internal_artifacts.push(SupplementarySuccessorActivationArtifactV1 {
                typed_artifact_schema_id,
                typed_artifact_id,
                canonical_artifact_preimage_length_u64,
                canonical_artifact_preimage_bytes: pre_bytes,
            });
        }
        let mut it = expl_vecs.into_iter();
        Ok(Self {
            schema_version,
            predecessor_admitted_profile_id,
            successor_admitted_profile_id,
            successor_profile_admission_manifest_id,
            successor_transition_spec_at_profile_id,
            successor_release_record_id,
            migration_spec_id,
            successor_upgrade_evidence_manifest_id,
            internal_artifact_count_u32,
            supplementary_internal_artifact_count_u32,
            internal_dependency_edge_count_u32,
            internal_canonical_preimage_bytes_u64,
            explicit_internal_preimage_lengths_u64,
            successor_profile_scope_bytes: it.next().expect("0"),
            successor_domain_registry_bytes: it.next().expect("1"),
            successor_candidate_profile_bytes: it.next().expect("2"),
            successor_transition_spec_bytes: it.next().expect("3"),
            successor_profile_admission_manifest_bytes: it.next().expect("4"),
            successor_admitted_release_record_bytes: it.next().expect("5"),
            successor_upgrade_evidence_manifest_bytes: it.next().expect("6"),
            internal_dependency_nodes,
            supplementary_internal_artifacts,
        })
    }

    /// Computes the canonical `Receipt` `ContentTag` whose digest IS `successor_activation_bundle_id`.
    pub fn tag(&self) -> Result<ContentTag, SuccessorActivation1Error> {
        let enc = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            SUCCESSOR_ACTIVATION_1_BUNDLE_SCHEMA,
            enc.len() as u64,
            |w| w(&enc),
        )
        .expect("static schema and bounded length"))
    }
}

/// Zero-allocation streaming prefix-first preflight of raw [`SuccessorActivationBundleV1`] wire
/// bytes. Checks EVERY count, internal-only equation (`7 + |S_s| = |D_s|`), checked byte sum,
/// carrier mode (`1` embedded-only), internal-only dep-ref tag (`1`, rejecting external-ID
/// injection), sort uniqueness, cap bound, and exact frame boundary BEFORE any heap allocation,
/// copying, hashing, or nested decode.
pub fn preflight_successor_activation_bundle_bytes(
    raw: &[u8],
    caps: SuccessorPreflightCapsV1,
) -> Result<PreflightSuccessorBundleSummaryV1, SuccessorActivation1Error> {
    if (raw.len() as u64) > caps.max_bundle_frame_bytes {
        return Err(SuccessorActivation1Error::PreflightRejected(
            "successor bundle exceeds max_bundle_frame_bytes",
        ));
    }
    let mut p = 0usize;
    let ver = take_u16(raw, &mut p)?;
    if ver != SUCCESSOR_ACTIVATION_1_VERSION {
        return Err(SuccessorActivation1Error::PreflightRejected(
            "unsupported successor bundle schema_version",
        ));
    }
    let predecessor_admitted_profile_id = take_32(raw, &mut p)?;
    let successor_admitted_profile_id = take_32(raw, &mut p)?;
    let _adm_id = take_32(raw, &mut p)?;
    let _spec_id = take_32(raw, &mut p)?;
    let _rel_id = take_32(raw, &mut p)?;
    let _mig_id = take_32(raw, &mut p)?;
    let _ev_id = take_32(raw, &mut p)?;
    if predecessor_admitted_profile_id == [0u8; 32]
        || successor_admitted_profile_id == [0u8; 32]
        || predecessor_admitted_profile_id == successor_admitted_profile_id
    {
        return Err(SuccessorActivation1Error::BindingMismatch(
            "successor bundle requires distinct non-zero old and new profile IDs",
        ));
    }

    let internal_artifact_count_u32 = take_u32(raw, &mut p)?;
    let supplementary_internal_artifact_count_u32 = take_u32(raw, &mut p)?;
    let internal_dependency_edge_count_u32 = take_u32(raw, &mut p)?;
    let internal_canonical_preimage_bytes_u64 = take_u64(raw, &mut p)?;

    if supplementary_internal_artifact_count_u32 > caps.max_supplementary_artifacts {
        return Err(SuccessorActivation1Error::PreflightRejected(
            "supplementary_internal_artifact_count exceeds cap",
        ));
    }
    if internal_dependency_edge_count_u32 > caps.max_internal_dependency_edges {
        return Err(SuccessorActivation1Error::PreflightRejected(
            "internal_dependency_edge_count exceeds cap",
        ));
    }
    if internal_canonical_preimage_bytes_u64 > caps.max_internal_canonical_preimage_bytes {
        return Err(SuccessorActivation1Error::PreflightRejected(
            "internal_canonical_preimage_bytes exceeds cap",
        ));
    }
    let expected_internal = EXPLICIT_SUCCESSOR_ARTIFACT_COUNT
        .checked_add(supplementary_internal_artifact_count_u32)
        .ok_or(SuccessorActivation1Error::EquationViolation(
            "7 + |S_s| overflow",
        ))?;
    if internal_artifact_count_u32 != expected_internal {
        return Err(SuccessorActivation1Error::EquationViolation(
            "internal_artifact_count_u32 != 7 + supplementary_internal_artifact_count_u32",
        ));
    }

    let mut explicit_lengths = [0u64; 7];
    let mut sum_bytes = 0u64;
    for slot in &mut explicit_lengths {
        let l = take_u64(raw, &mut p)?;
        if l == 0 || l > caps.max_single_preimage_bytes {
            return Err(SuccessorActivation1Error::PreflightRejected(
                "explicit internal preimage length zero or over cap",
            ));
        }
        *slot = l;
        sum_bytes =
            sum_bytes
                .checked_add(l)
                .ok_or(SuccessorActivation1Error::EquationViolation(
                    "explicit byte sum overflow",
                ))?;
    }

    // Stream 7 explicit internal preimages (embedded-only carrier mode = 1, zero allocation)
    for &decl_len in &explicit_lengths {
        let mode = take_u8(raw, &mut p)?;
        if mode != SUCCESSOR_EMBEDDED_CARRIER_MODE_V1 {
            return Err(SuccessorActivation1Error::PreflightRejected(
                "locator or non-embedded carrier mode forbidden in explicit internal slot",
            ));
        }
        let wire_len = take_u64(raw, &mut p)?;
        if wire_len != decl_len {
            return Err(SuccessorActivation1Error::EquationViolation(
                "explicit_internal_preimage_lengths_u64 disagrees with carried slice length",
            ));
        }
        let wire_usize = usize::try_from(wire_len).map_err(|_| {
            SuccessorActivation1Error::PreflightRejected("explicit preimage usize overflow")
        })?;
        let _ = take_slice(raw, &mut p, wire_usize)?;
    }

    // Stream internal-only dependency nodes D_s (zero external variants permitted!)
    let n_nodes = take_u32(raw, &mut p)?;
    if n_nodes != internal_artifact_count_u32 {
        return Err(SuccessorActivation1Error::EquationViolation(
            "|D_s| != internal_artifact_count_u32",
        ));
    }
    let mut counted_edges = 0u32;
    let mut prev_node_id: Option<[u8; 32]> = None;
    for _ in 0..n_nodes {
        let _rank = take_u32(raw, &mut p)?;
        let node_id = take_32(raw, &mut p)?;
        if let Some(prev) = prev_node_id
            && prev >= node_id
        {
            return Err(SuccessorActivation1Error::DagViolation(
                "internal_dependency_nodes not strictly sorted by unique typed_internal_artifact_id",
            ));
        }
        prev_node_id = Some(node_id);
        let n_deps = u32::from(take_u16(raw, &mut p)?);
        counted_edges = counted_edges.checked_add(n_deps).ok_or(
            SuccessorActivation1Error::EquationViolation("internal edge count overflow"),
        )?;
        if counted_edges > caps.max_internal_dependency_edges {
            return Err(SuccessorActivation1Error::PreflightRejected(
                "streamed internal dependency edges exceed cap",
            ));
        }
        let mut prev_dep: Option<[u8; 32]> = None;
        for _ in 0..n_deps {
            let ref_tag = take_u8(raw, &mut p)?;
            if ref_tag != INTERNAL_DEP_REF_TAG_V1 {
                return Err(SuccessorActivation1Error::DagViolation(
                    "external/root/launch/genesis dependency variant forbidden in internal-only successor bundle",
                ));
            }
            let dep_id = take_32(raw, &mut p)?;
            if let Some(pd) = prev_dep
                && pd >= dep_id
            {
                return Err(SuccessorActivation1Error::DagViolation(
                    "internal_dependency_refs not strictly sorted and unique",
                ));
            }
            prev_dep = Some(dep_id);
        }
    }
    if counted_edges != internal_dependency_edge_count_u32 {
        return Err(SuccessorActivation1Error::EquationViolation(
            "internal_dependency_edge_count_u32 != Length(InternalRefs(D_s))",
        ));
    }

    // Stream supplementary internal artifacts S_s (embedded-only)
    let n_supp = take_u32(raw, &mut p)?;
    if n_supp != supplementary_internal_artifact_count_u32 {
        return Err(SuccessorActivation1Error::EquationViolation(
            "|S_s| != supplementary_internal_artifact_count_u32",
        ));
    }
    let mut prev_supp_id: Option<[u8; 32]> = None;
    for _ in 0..n_supp {
        let mode = take_u8(raw, &mut p)?;
        if mode != SUCCESSOR_EMBEDDED_CARRIER_MODE_V1 {
            return Err(SuccessorActivation1Error::PreflightRejected(
                "locator or non-embedded carrier mode forbidden in supplementary internal slot",
            ));
        }
        let slen = usize::from(take_u16(raw, &mut p)?);
        if slen == 0 {
            return Err(SuccessorActivation1Error::PreflightRejected(
                "empty supplementary schema_id",
            ));
        }
        let _ = take_slice(raw, &mut p, slen)?;
        let sid = take_32(raw, &mut p)?;
        if let Some(ps) = prev_supp_id
            && ps >= sid
        {
            return Err(SuccessorActivation1Error::DagViolation(
                "supplementary_internal_artifacts not strictly sorted by unique typed_artifact_id",
            ));
        }
        prev_supp_id = Some(sid);
        let pre_len = take_u64(raw, &mut p)?;
        if pre_len == 0 || pre_len > caps.max_single_preimage_bytes {
            return Err(SuccessorActivation1Error::PreflightRejected(
                "supplementary preimage length zero or over cap",
            ));
        }
        sum_bytes =
            sum_bytes
                .checked_add(pre_len)
                .ok_or(SuccessorActivation1Error::EquationViolation(
                    "internal canonical preimage sum overflow",
                ))?;
        let pre_usize = usize::try_from(pre_len).map_err(|_| {
            SuccessorActivation1Error::PreflightRejected("supplementary usize overflow")
        })?;
        let _ = take_slice(raw, &mut p, pre_usize)?;
    }

    if sum_bytes != internal_canonical_preimage_bytes_u64 {
        return Err(SuccessorActivation1Error::EquationViolation(
            "internal_canonical_preimage_bytes_u64 != sum(E_s) + sum(S_s)",
        ));
    }
    if p != raw.len() {
        return Err(SuccessorActivation1Error::PreflightRejected(
            "trailing bytes after SuccessorActivationBundleV1 frame",
        ));
    }

    Ok(PreflightSuccessorBundleSummaryV1 {
        internal_artifact_count_u32,
        supplementary_internal_artifact_count_u32,
        internal_dependency_edge_count_u32,
        internal_canonical_preimage_bytes_u64,
        explicit_internal_preimage_lengths_u64: explicit_lengths,
        predecessor_admitted_profile_id,
        successor_admitted_profile_id,
    })
}

/// Verifies a live `CONSENSUS-1` parent-authority certificate against a Stage 1 eligibility record,
/// active validator schedule, exact consumer context, and live admission registry uniqueness.
fn verify_purpose_live_cert(
    base: &VerifiedSuccessorPredecessorBaseContextV1,
    elig: &VerifiedParentEligibilityV1,
    cert: &AcceptedConsensusNativeParentAuthorityCertificateV1,
    expected_purpose: ParentAuthorityPurposeV1,
    expected_consumer_object_digest: [u8; 32],
    live_registry: &mut LiveParentAuthorityRegistryV1,
) -> Result<([u8; 32], [u8; 32]), SuccessorActivation1Error> {
    if cert.from_replay_archive {
        return Err(SuccessorActivation1Error::AuthorityStageRejected(
            "replay-archive certificate forbidden in live successor authority stage",
        ));
    }
    if cert.consumer_context.purpose != expected_purpose || elig.purpose != expected_purpose {
        return Err(SuccessorActivation1Error::AuthorityStageRejected(
            "parent-authority certificate purpose mismatch (purpose swap/coercion rejected)",
        ));
    }
    if cert.parent_profile_id != base.old_profile_id
        || cert.accepted_parent_lookup_id != elig.accepted_parent_lookup_id
        || cert.unique_accepted_proposal_core_block_id
            != elig.unique_accepted_proposal_core_block_id
    {
        return Err(SuccessorActivation1Error::AuthorityStageRejected(
            "live parent-authority certificate disagrees with Stage 1 predecessor parent core",
        ));
    }
    if cert.consumer_context.consumer_chain_id != base.genesis_package.state.chain_id
        || cert.consumer_context.consumer_profile_id != base.old_profile_id
        || cert.consumer_context.consumer_object_digest != expected_consumer_object_digest
        || cert.consumer_context.required_position != elig.required_position
    {
        return Err(SuccessorActivation1Error::AuthorityStageRejected(
            "certificate consumer context disagrees with bound object digest or required position",
        ));
    }
    if cert.evidence_cutoff_position != elig.evidence_cutoff_position
        || cert.evidence_commitment_id != elig.evidence_commitment_id
        || cert.occurrence_universe_root != elig.occurrence_universe_root
        || cert.occurrence_receipts.len() as u32 != elig.total_occurrence_count
        || cert.certificate_policy_id != elig.policy_id
        || cert.attained_status != elig.status
    {
        return Err(SuccessorActivation1Error::AuthorityStageRejected(
            "certificate evidence commitment, occurrence universe, policy, or status disagrees with Stage 1 eligibility",
        ));
    }
    cert.verify_live_admission(&base.fork_context, &base.active_schedule)
        .map_err(|_| {
            SuccessorActivation1Error::AuthorityStageRejected(
                "CONSENSUS-1 verify_live_admission rejected parent-authority certificate",
            )
        })?;
    let _slot = live_registry
        .admit_live(cert.clone(), &base.fork_context, &base.active_schedule)
        .map_err(|_| {
            SuccessorActivation1Error::AuthorityStageRejected(
                "competing or duplicate live parent-authority admission at required position",
            )
        })?;
    let admission_id = cert.admission_id().map_err(|_| {
        SuccessorActivation1Error::AuthorityStageRejected("cert admission_id failed")
    })?;
    let cert_id = cert
        .tag()
        .map_err(|_| SuccessorActivation1Error::AuthorityStageRejected("cert tag failed"))?
        .digest();
    Ok((cert_id, admission_id))
}

/// Verifies a complete [`SuccessorActivationCarrierV1`] end-to-end across all three non-serialized
/// acyclic verifier stages (`Stage 1 PredecessorBase` -> `Stage 2 ReleaseAuthority` -> internal
/// bundle closure -> `Stage 3 ActivationAuthority` -> `DedicatedMigrationBodyV1`).
///
/// Rejects fail-closed BEFORE any migration state change on every count, length, edge, external-ID,
/// duplicate-prior-byte, purpose-swap, replay-archive, partial-occurrence, competing-admission,
/// wrong-release, wrong-authorization, availability-commitment, or migration-body mutant.
#[allow(clippy::too_many_arguments)]
pub fn verify_successor_activation_carrier(
    carrier: &SuccessorActivationCarrierV1,
    base: &VerifiedSuccessorPredecessorBaseContextV1,
    release_parent_cert: &AcceptedConsensusNativeParentAuthorityCertificateV1,
    activation_parent_cert: &AcceptedConsensusNativeParentAuthorityCertificateV1,
    caps: SuccessorPreflightCapsV1,
) -> Result<VerifiedSuccessorActivationOutcomeV1, SuccessorActivation1Error> {
    // Stage 1: Validate non-serialized predecessor base context.
    base.validate()?;

    let (
        bundle,
        declared_bundle_id,
        authorization,
        declared_auth_id,
        dedicated_body,
        declared_avail_commit,
    ) = match carrier {
        SuccessorActivationCarrierV1::NoSuccessorActivationV1 => {
            return Err(SuccessorActivation1Error::DedicatedMigrationBodyViolation(
                "migration verification requires EmbeddedSuccessorActivationV1 carrier",
            ));
        }
        SuccessorActivationCarrierV1::EmbeddedSuccessorActivationV1 {
            bundle,
            successor_activation_bundle_id,
            authorization,
            activation_authorization_id,
            dedicated_migration_body,
            availability_commitment,
        } => (
            bundle,
            *successor_activation_bundle_id,
            authorization,
            *activation_authorization_id,
            dedicated_migration_body,
            *availability_commitment,
        ),
    };

    // 1. Prefix-first zero-allocation preflight on the carried bundle bytes + availability commitment.
    let bundle_bytes = bundle.encode_canonical()?;
    let preflight = preflight_successor_activation_bundle_bytes(&bundle_bytes, caps)?;
    let auth_bytes = authorization.encode_canonical()?;
    let body_bytes = dedicated_body.encode_canonical()?;
    let expected_avail = SuccessorActivationCarrierV1::compute_availability_commitment(
        base.old_profile_id,
        &bundle_bytes,
        &auth_bytes,
        &body_bytes,
    );
    if expected_avail != declared_avail_commit {
        return Err(SuccessorActivation1Error::DedicatedMigrationBodyViolation(
            "old-profile availability_commitment mismatch",
        ));
    }

    // 2. Decode all 7 explicit internal preimages and verify projections against Stage 1 base context.
    let succ_scope = ProfileScopeV1::decode_canonical(&bundle.successor_profile_scope_bytes)
        .map_err(|_| {
            SuccessorActivation1Error::BindingMismatch("decode successor ProfileScopeV1")
        })?;
    let succ_reg = DomainRegistryV1::decode_canonical(&bundle.successor_domain_registry_bytes)
        .map_err(|_| {
            SuccessorActivation1Error::BindingMismatch("decode successor DomainRegistryV1")
        })?;
    let succ_bound =
        BoundProfileManifestV1::decode_canonical(&bundle.successor_candidate_profile_bytes)
            .map_err(|_| {
                SuccessorActivation1Error::BindingMismatch(
                    "decode successor BoundProfileManifestV1",
                )
            })?;
    let succ_spec =
        ResolvedProfileTransitionSpecV1::decode_canonical(&bundle.successor_transition_spec_bytes)
            .map_err(|_| {
                SuccessorActivation1Error::BindingMismatch(
                    "decode successor ResolvedProfileTransitionSpecV1",
                )
            })?;
    let succ_admission = ProfileAdmissionManifestV1::decode_canonical(
        &bundle.successor_profile_admission_manifest_bytes,
    )
    .map_err(|_| {
        SuccessorActivation1Error::BindingMismatch("decode successor ProfileAdmissionManifestV1")
    })?;
    let succ_release =
        AdmittedReleaseRecordV1::decode_canonical(&bundle.successor_admitted_release_record_bytes)
            .map_err(|_| {
                SuccessorActivation1Error::BindingMismatch(
                    "decode successor AdmittedReleaseRecordV1",
                )
            })?;
    let succ_evidence = SuccessorUpgradeEvidenceManifestV1::decode_canonical(
        &bundle.successor_upgrade_evidence_manifest_bytes,
    )
    .map_err(|_| {
        SuccessorActivation1Error::BindingMismatch(
            "decode successor SuccessorUpgradeEvidenceManifestV1",
        )
    })?;

    let scope_id = succ_scope
        .tag()
        .map_err(|_| SuccessorActivation1Error::BindingMismatch("succ scope tag"))?
        .digest();
    let reg_id = succ_reg.tag().digest();
    let new_profile_id = succ_bound.profile_id;
    let spec_id = succ_spec
        .tag()
        .map_err(|_| SuccessorActivation1Error::BindingMismatch("succ spec tag"))?
        .digest();
    let admission_id = succ_admission
        .tag()
        .map_err(|_| SuccessorActivation1Error::BindingMismatch("succ admission tag"))?
        .digest();
    let release_core_id = succ_release
        .core
        .tag()
        .map_err(|_| SuccessorActivation1Error::BindingMismatch("succ release core tag"))?
        .digest();
    let evidence_id = succ_evidence
        .tag()
        .map_err(|_| SuccessorActivation1Error::BindingMismatch("succ evidence tag"))?
        .digest();

    // Check predecessor projections & old->new polarity before Stage 2.
    if bundle.predecessor_admitted_profile_id != base.old_profile_id
        || bundle.successor_admitted_profile_id != new_profile_id
        || bundle.predecessor_admitted_profile_id == bundle.successor_admitted_profile_id
        || bundle.successor_profile_admission_manifest_id != admission_id
        || bundle.successor_transition_spec_at_profile_id != spec_id
        || bundle.successor_upgrade_evidence_manifest_id != evidence_id
        || bundle.migration_spec_id != succ_evidence.migration_spec_id
        || succ_bound.candidate.genesis_id != base.genesis_package.genesis_id
        || succ_bound.candidate.predecessor_profile_id != Some(base.old_profile_id)
        || succ_bound.candidate.current_scope_digest != scope_id
        || succ_bound.candidate.domain_registry_digest != reg_id
        || succ_admission.admitted_profile_id != new_profile_id
        || succ_admission.admitted_transition_spec_at_profile_id != spec_id
        || succ_admission.admitted_profile_scope_digest != scope_id
        || succ_admission.admitted_domain_registry_digest != reg_id
        || succ_evidence.predecessor_admitted_profile_id != base.old_profile_id
        || succ_evidence.successor_candidate_profile_id != new_profile_id
        || succ_evidence.old_execution_root_ref != base.old_execution_root_ref
    {
        return Err(SuccessorActivation1Error::BindingMismatch(
            "internal successor preimage disagrees with Stage 1 predecessor context or polarity",
        ));
    }

    // Check supplementary internal artifacts S_s: hash-check every preimage and FORBID duplicating
    // any prior launch/genesis/predecessor artifact bytes or IDs.
    let forbidden_prior_ids: BTreeSet<[u8; 32]> = [
        base.genesis_package.genesis_id,
        base.chain_launch_manifest_id,
        base.old_profile_id,
        base.old_profile_scope_id,
        base.old_transition_spec_id,
        base.verified_release_parent_eligibility
            .accepted_parent_lookup_id,
        base.verified_release_parent_eligibility
            .unique_accepted_proposal_core_block_id,
    ]
    .into_iter()
    .collect();

    let mut supp_ids = BTreeSet::new();
    let mut supp_inputs: Option<CandidateBuildInputsV1> = None;
    let mut supp_receipt: Option<CandidateReleaseReceiptV1> = None;
    let mut supp_mig_spec: Option<MigrationSpecV1> = None;
    for s in &bundle.supplementary_internal_artifacts {
        if forbidden_prior_ids.contains(&s.typed_artifact_id) {
            return Err(SuccessorActivation1Error::DagViolation(
                "prior launch/genesis/predecessor artifact duplicated inside supplementary S_s",
            ));
        }
        let kind = if s.typed_artifact_schema_id.as_slice() == RELEASE_R1_CANDIDATE_RECEIPT_SCHEMA {
            TagKind::Receipt
        } else {
            TagKind::Policy
        };
        let recomputed = hash_canonical(
            kind,
            &s.typed_artifact_schema_id,
            s.canonical_artifact_preimage_bytes.len() as u64,
            |w| w(&s.canonical_artifact_preimage_bytes),
        )
        .map_err(|_| SuccessorActivation1Error::BindingMismatch("supplementary schema invalid"))?
        .digest();
        if recomputed != s.typed_artifact_id {
            return Err(SuccessorActivation1Error::BindingMismatch(
                "supplementary internal artifact digest mismatch",
            ));
        }
        if !supp_ids.insert(s.typed_artifact_id) {
            return Err(SuccessorActivation1Error::DagViolation(
                "duplicate supplementary internal artifact ID",
            ));
        }
        if s.typed_artifact_schema_id.as_slice() == RELEASE_R1_BUILD_INPUTS_SCHEMA {
            supp_inputs = Some(
                CandidateBuildInputsV1::decode_canonical(&s.canonical_artifact_preimage_bytes)
                    .map_err(|_| {
                        SuccessorActivation1Error::BindingMismatch(
                            "decode supp CandidateBuildInputsV1",
                        )
                    })?,
            );
        } else if s.typed_artifact_schema_id.as_slice() == RELEASE_R1_CANDIDATE_RECEIPT_SCHEMA {
            supp_receipt = Some(
                CandidateReleaseReceiptV1::decode_canonical(&s.canonical_artifact_preimage_bytes)
                    .map_err(|_| {
                    SuccessorActivation1Error::BindingMismatch(
                        "decode supp CandidateReleaseReceiptV1",
                    )
                })?,
            );
        } else if s.typed_artifact_schema_id.as_slice() == UPGRADE_1_MIGRATION_SPEC_SCHEMA {
            let ms = MigrationSpecV1::decode_canonical(&s.canonical_artifact_preimage_bytes)
                .map_err(|_| {
                    SuccessorActivation1Error::BindingMismatch("decode supp MigrationSpecV1")
                })?;
            if ms
                .tag()
                .map_err(|_| SuccessorActivation1Error::BindingMismatch("mig spec tag"))?
                .digest()
                != bundle.migration_spec_id
            {
                return Err(SuccessorActivation1Error::BindingMismatch(
                    "supplementary MigrationSpecV1 disagrees with bundle.migration_spec_id",
                ));
            }
            supp_mig_spec = Some(ms);
        }
    }

    // Resolve candidate inputs/receipt deterministically (from S_s when carried, or canonical derivation for |S|=0).
    let (cand_inputs, cand_receipt) = match (supp_inputs, supp_receipt) {
        (Some(ci), Some(cr)) => (ci, cr),
        (None, None) => build_canonical_candidate_fixture(&succ_bound).map_err(|_| {
            SuccessorActivation1Error::BindingMismatch("canonical v2 candidate fixture")
        })?,
        _ => {
            return Err(SuccessorActivation1Error::BindingMismatch(
                "partial candidate release inputs/receipt in supplementary S_s",
            ));
        }
    };
    let _ = supp_mig_spec;

    // Stage 2: Verify live CONSENSUS-1 ReleaseAuthority certificate bound to `release_core_id`,
    // then verify `AdmittedReleaseRecordV1` through FROZEN `release_1::verify_admitted_release_record`.
    let mut live_registry = LiveParentAuthorityRegistryV1::default();
    let (release_cert_id, release_cert_admission_id) = verify_purpose_live_cert(
        base,
        &base.verified_release_parent_eligibility,
        release_parent_cert,
        ParentAuthorityPurposeV1::ReleaseAuthority,
        release_core_id,
        &mut live_registry,
    )?;

    // Ensure release core context ref & historical ref match Stage 1 release eligibility.
    match (
        succ_release.core.release_authority_context_ref,
        succ_release
            .authorization
            .sign_doc
            .release_historical_authority_ref,
    ) {
        (
            ReleaseAuthorityContextRefV1::AcceptedParentReleaseAuthorityV1 {
                predecessor_accepted_parent_lookup_id,
                purpose_code: ReleaseParentPurposeCodeV1::SuccessorReleaseAuthority,
                predecessor_governance_authority_root,
            },
            ReleaseHistoricalAuthorityRefV1::AcceptedParentReleaseHistoricalAuthorityRefV1 {
                parent_cert_id,
                evidence_commitment,
                occurrence_universe_root,
                height,
                purpose_code: ReleaseParentPurposeCodeV1::SuccessorReleaseAuthority,
                ..
            },
        ) => {
            let rel_elig = &base.verified_release_parent_eligibility;
            if predecessor_accepted_parent_lookup_id != rel_elig.accepted_parent_lookup_id
                || predecessor_governance_authority_root != rel_elig.governance_authority_root
                || parent_cert_id != release_cert_id
                || evidence_commitment != rel_elig.evidence_commitment_id
                || occurrence_universe_root != rel_elig.occurrence_universe_root
                || height != rel_elig.required_position
            {
                return Err(SuccessorActivation1Error::AuthorityStageRejected(
                    "AdmittedReleaseRecordV1 authority refs disagree with Stage 2 ReleaseAuthority cert",
                ));
            }
        }
        _ => {
            return Err(SuccessorActivation1Error::AuthorityStageRejected(
                "successor AdmittedReleaseRecordV1 must use AcceptedParentReleaseAuthorityV1 + SuccessorReleaseAuthority",
            ));
        }
    }

    let succ_cert_ctx = SuccessorReleaseCertContextV1 {
        live_parent_cert: release_parent_cert,
        from_replay_archive: false,
    };
    let release_id = verify_admitted_release_record(
        &succ_release,
        &base.genesis_package,
        &succ_admission,
        &base.launch_parity_record,
        &cand_inputs,
        &cand_receipt,
        &base.trusted_release_policy,
        Some(&succ_cert_ctx),
    )
    .map_err(|_| {
        SuccessorActivation1Error::AuthorityStageRejected(
            "frozen RELEASE-1 verify_admitted_release_record rejected successor release",
        )
    })?;
    if release_id != bundle.successor_release_record_id {
        return Err(SuccessorActivation1Error::BindingMismatch(
            "successor_release_record_id mismatch",
        ));
    }
    let release_stage = VerifiedSuccessorReleaseAuthorityContextV1 {
        admitted_release_core_id: release_core_id,
        consumer_context: release_parent_cert.consumer_context,
        release_cert_id,
        release_cert_admission_id,
        admitted_release_record_id: release_id,
    };

    // 3. Walk the internal-only DAG D_s: exact bijection with carried artifacts & strict rank monotonicity.
    let mut expected_internal_ids = BTreeSet::new();
    for id in [
        scope_id,
        reg_id,
        new_profile_id,
        spec_id,
        admission_id,
        release_id,
        evidence_id,
    ] {
        if !expected_internal_ids.insert(id) {
            return Err(SuccessorActivation1Error::DagViolation(
                "duplicate explicit internal artifact ID",
            ));
        }
    }
    for sid in &supp_ids {
        if !expected_internal_ids.insert(*sid) {
            return Err(SuccessorActivation1Error::DagViolation(
                "supplementary internal artifact collides with explicit internal artifact",
            ));
        }
    }
    let mut ranks: BTreeMap<[u8; 32], u32> = BTreeMap::new();
    for node in &bundle.internal_dependency_nodes {
        if !expected_internal_ids.contains(&node.typed_internal_artifact_id) {
            return Err(SuccessorActivation1Error::DagViolation(
                "internal DAG node names uncarried or external artifact ID",
            ));
        }
        if ranks
            .insert(node.typed_internal_artifact_id, node.topological_rank_u32)
            .is_some()
        {
            return Err(SuccessorActivation1Error::DagViolation(
                "duplicate internal DAG node",
            ));
        }
    }
    if ranks.len() != expected_internal_ids.len() {
        return Err(SuccessorActivation1Error::DagViolation(
            "carried internal artifact missing from D_s",
        ));
    }

    let mut has_scope_reg = false;
    let mut has_adm_ev = false;
    let mut has_rel_adm = false;
    for node in &bundle.internal_dependency_nodes {
        let src_id = node.typed_internal_artifact_id;
        let src_rank = node.topological_rank_u32;
        for &dst_id in &node.internal_dependency_refs {
            if forbidden_prior_ids.contains(&dst_id) {
                return Err(SuccessorActivation1Error::DagViolation(
                    "external launch/genesis/predecessor ID injected into internal DAG edge",
                ));
            }
            let dst_rank = *ranks
                .get(&dst_id)
                .ok_or(SuccessorActivation1Error::DagViolation(
                    "internal DAG edge targets unknown node outside D_s",
                ))?;
            if dst_rank >= src_rank {
                return Err(SuccessorActivation1Error::DagViolation(
                    "internal DAG edge violates strict topological rank monotonicity",
                ));
            }
            if src_id == scope_id && dst_id == reg_id {
                has_scope_reg = true;
            }
            if src_id == admission_id && dst_id == evidence_id {
                has_adm_ev = true;
            }
            if src_id == release_id && dst_id == admission_id {
                has_rel_adm = true;
            }
        }
    }
    if !(has_scope_reg && has_adm_ev && has_rel_adm) {
        return Err(SuccessorActivation1Error::DagViolation(
            "missing mandatory internal scope->reg, admission->evidence, or release->admission edge",
        ));
    }

    // Complete bundle ID ONLY AFTER all internal preimages, Stage 2 release, and DAG checks pass!
    let recomputed_bundle_id = bundle.tag()?.digest();
    if recomputed_bundle_id != declared_bundle_id {
        return Err(SuccessorActivation1Error::BindingMismatch(
            "successor_activation_bundle_id mismatch",
        ));
    }
    // Forbid self-reference of bundle ID in any internal node.
    if expected_internal_ids.contains(&recomputed_bundle_id) {
        return Err(SuccessorActivation1Error::DagViolation(
            "bundle self-reference detected",
        ));
    }

    // Stage 3: Verify distinct live CONSENSUS-1 ActivationAuthority certificate bound ONLY
    // post-bundle to `recomputed_bundle_id` (cannot substitute Stage 2 ReleaseAuthority cert!).
    let (activation_cert_id, activation_cert_admission_id) = verify_purpose_live_cert(
        base,
        &base.verified_activation_parent_eligibility,
        activation_parent_cert,
        ParentAuthorityPurposeV1::ActivationAuthority,
        recomputed_bundle_id,
        &mut live_registry,
    )?;
    if activation_cert_id == release_cert_id {
        return Err(SuccessorActivation1Error::AuthorityStageRejected(
            "release and activation certificates must be distinct",
        ));
    }

    // 4. Verify SuccessorActivationSignDocV1 & 3-of-5 Ed25519 ActivationAuthorizationV1 under
    //    predecessor-resolved TrustedActivationAuthorityPolicyV1 (NEVER candidate-selected!).
    let expected_act_policy_id = base.trusted_activation_policy.policy_id()?;
    let expected_hist_ref =
        HistoricalAcceptedParentAuthorityUseRefV1::from_live_cert(activation_parent_cert)?;
    let sd = &authorization.sign_doc;
    let act_elig = &base.verified_activation_parent_eligibility;
    if sd.chain_launch_manifest_id != base.chain_launch_manifest_id
        || sd.predecessor_accepted_parent_lookup_id != act_elig.accepted_parent_lookup_id
        || sd.predecessor_authority_purpose != ParentAuthorityPurposeV1::ActivationAuthority
        || sd.predecessor_governance_authority_root != act_elig.governance_authority_root
        || sd.historical_predecessor_activation_authority_use_ref != expected_hist_ref
        || sd.old_active_profile_id != base.old_profile_id
        || sd.new_admitted_profile_id != new_profile_id
        || sd.successor_profile_admission_manifest_id != admission_id
        || sd.migration_spec_id != bundle.migration_spec_id
        || sd.successor_activation_bundle_id != recomputed_bundle_id
        || sd.activation_old_execution_root_ref != base.old_execution_root_ref
        || sd.activation_expected_new_execution_root_ref
            != succ_evidence.expected_new_execution_root_ref
        || sd.activation_authority_policy_id != expected_act_policy_id
    {
        return Err(SuccessorActivation1Error::BindingMismatch(
            "SuccessorActivationSignDocV1 disagrees with Stage 3 authority or completed bundle",
        ));
    }
    let recomputed_sd_id = sd.tag()?.digest();
    if authorization.successor_activation_sign_doc_id != recomputed_sd_id
        || authorization.activation_authority_policy_id != expected_act_policy_id
        || authorization.activation_authority_credential_set_id != expected_act_policy_id
        || authorization.signature_suite != SignatureSuiteIdV1::Ed25519StrictRfc8032V1
    {
        return Err(SuccessorActivation1Error::InvalidActivationAuthorization(
            "ActivationAuthorizationV1 sign-doc ID, policy ID, or suite mismatch",
        ));
    }
    if authorization.signatures.len() < usize::from(base.trusted_activation_policy.threshold_m) {
        return Err(SuccessorActivation1Error::InvalidActivationAuthorization(
            "insufficient activation threshold signatures (< 3-of-5)",
        ));
    }
    let preimage = ActivationAuthorizationV1::signing_preimage(
        base.genesis_package.state.chain_id,
        base.genesis_package.genesis_id,
        base.chain_launch_manifest_id,
        base.old_profile_id,
        recomputed_sd_id,
    );
    let mut seen_signer = BTreeSet::new();
    for (pos, entry) in authorization.signatures.iter().enumerate() {
        let idx = usize::from(entry.signer_index);
        if idx >= base.trusted_activation_policy.authorized_pubkeys.len() {
            return Err(SuccessorActivation1Error::InvalidActivationAuthorization(
                "activation signer_index out of authorized keyset range",
            ));
        }
        if !seen_signer.insert(entry.signer_index) {
            return Err(SuccessorActivation1Error::InvalidActivationAuthorization(
                "duplicate activation signer_index",
            ));
        }
        if pos > 0 && authorization.signatures[pos - 1].signer_index >= entry.signer_index {
            return Err(SuccessorActivation1Error::InvalidActivationAuthorization(
                "activation signatures must be strictly ascending by signer_index",
            ));
        }
        if entry.public_key != base.trusted_activation_policy.authorized_pubkeys[idx] {
            return Err(SuccessorActivation1Error::InvalidActivationAuthorization(
                "unauthorized activation signer public key",
            ));
        }
        verify_preimage_strict(&entry.public_key, &preimage, &entry.signature).map_err(|_| {
            SuccessorActivation1Error::InvalidActivationAuthorization(
                "activation Ed25519 signature verification failed",
            )
        })?;
    }
    let recomputed_auth_id = authorization.tag()?.digest();
    if recomputed_auth_id != declared_auth_id {
        return Err(SuccessorActivation1Error::InvalidActivationAuthorization(
            "activation_authorization_id mismatch",
        ));
    }

    // 5. Enforce DedicatedMigrationBodyV1 exclusivity (0 txs, 0 app subticks, 1 matching migration transition).
    if dedicated_body.signed_transaction_count != 0
        || dedicated_body.application_subtick_count != 0
        || dedicated_body.system_transitions.len() != 1
    {
        return Err(SuccessorActivation1Error::DedicatedMigrationBodyViolation(
            "DedicatedMigrationBodyV1 requires 0 txs, 0 app subticks, and exactly 1 migration transition",
        ));
    }
    let sys_tr = dedicated_body.system_transitions[0];
    if sys_tr.successor_profile_admission_manifest_id != admission_id
        || sys_tr.migration_spec_id != bundle.migration_spec_id
        || sys_tr.successor_activation_bundle_id != recomputed_bundle_id
        || sys_tr.activation_authorization_id != recomputed_auth_id
        || sys_tr.activation_height != sd.activation_height
        || dedicated_body.deterministic_migration_inputs_digest
            != succ_evidence.canonical_parameter_and_state_delta_digest
    {
        return Err(SuccessorActivation1Error::DedicatedMigrationBodyViolation(
            "AuthorizedMigrationSystemTransitionV1 or migration inputs disagree with verified closure",
        ));
    }

    let activation_stage = VerifiedSuccessorActivationAuthorityContextV1 {
        successor_activation_bundle_id: recomputed_bundle_id,
        consumer_context: activation_parent_cert.consumer_context,
        activation_cert_id,
        activation_cert_admission_id,
        historical_activation_use_ref: expected_hist_ref,
        successor_activation_sign_doc_id: recomputed_sd_id,
        activation_authorization_id: recomputed_auth_id,
    };

    Ok(VerifiedSuccessorActivationOutcomeV1 {
        preflight,
        release_stage,
        activation_stage,
        successor_profile_admission_manifest_id: admission_id,
        successor_upgrade_evidence_manifest_id: evidence_id,
        activation_height: sd.activation_height,
        expected_new_execution_root_ref: succ_evidence.expected_new_execution_root_ref,
    })
}

/// Complete deterministic `bitblock.devnet.v1` successor-activation build result (for tests & probes).
#[derive(Clone, Debug)]
pub struct CanonicalDevnetSuccessorActivationBuildV1 {
    /// Upstream verified `CHAIN-LAUNCH-1` launch build.
    pub launch_build: CanonicalDevnetLaunchBuildV1,
    /// Upstream verified `UPGRADE-1` preventive migration outcome.
    pub upgrade_outcome: CanonicalUpgrade1OutcomeV1,
    /// Upstream finalized height-1 `CONSENSUS-1` commit certificate over the predecessor profile.
    pub predecessor_commit_cert: ConsensusCommitCertificateV1,
    /// Non-serialized Stage 1 predecessor base context.
    pub base_context: VerifiedSuccessorPredecessorBaseContextV1,
    /// Live `CONSENSUS-1` `ReleaseAuthority` parent-authority certificate (Stage 2).
    pub release_parent_cert: AcceptedConsensusNativeParentAuthorityCertificateV1,
    /// Live `CONSENSUS-1` `ActivationAuthority` parent-authority certificate (Stage 3).
    pub activation_parent_cert: AcceptedConsensusNativeParentAuthorityCertificateV1,
    /// Complete internal-only [`SuccessorActivationBundleV1`].
    pub bundle: SuccessorActivationBundleV1,
    /// Complete detached `3-of-5` [`ActivationAuthorizationV1`].
    pub authorization: ActivationAuthorizationV1,
    /// Complete [`SuccessorActivationCarrierV1`] (`EmbeddedSuccessorActivationV1`).
    pub carrier: SuccessorActivationCarrierV1,
}

/// Builds and end-to-end verifies the canonical `bitblock.devnet.v1` preventive `v1 -> v2`
/// `SUCCESSOR-ACTIVATION-1` closure for either:
/// - `include_supplementary = true` (`|S_s| = 3`: `10` internal artifacts, `23` internal edges), or
/// - `include_supplementary = false` (`|S_s| = 0`: `7` internal artifacts, `15` internal edges).
pub fn build_canonical_devnet_successor_activation(
    include_supplementary: bool,
) -> Result<CanonicalDevnetSuccessorActivationBuildV1, SuccessorActivation1Error> {
    let launch_build = build_canonical_devnet_chain_launch(true)
        .map_err(|_| SuccessorActivation1Error::BindingMismatch("chain launch build failed"))?;
    let upgrade_outcome = build_canonical_devnet_upgrade_1()
        .map_err(|_| SuccessorActivation1Error::BindingMismatch("upgrade_1 build failed"))?;
    let (_, succ_admission) =
        admit_canonical_devnet_successor_profile(&upgrade_outcome, UPGRADE_1_MERGED_COMMIT_SHA1)
            .map_err(|_| {
                SuccessorActivation1Error::BindingMismatch("frozen PROFILE-1 v2 admission failed")
            })?;
    let (cand_inputs, cand_receipt) = build_canonical_candidate_fixture(
        &upgrade_outcome.successor_candidate.successor_bound_manifest,
    )
    .map_err(|_| SuccessorActivation1Error::BindingMismatch("v2 candidate fixture failed"))?;

    // 1. Derive epoch-0 validator schedule & finalized height-1 HotStuff 2-phase BFT commit cert
    //    directly over the canonical launch `CanonicalDevnetGenesisFixtureV1` (exact same
    //    `genesis_id`, `old_profile_id`, and `chain_launch_manifest_id` as `CHAIN-LAUNCH-1` & `UPGRADE-1`).
    let fx = &upgrade_outcome.launch_fixture;
    let old_pid = fx.bound_manifest.profile_id;
    let gid = fx.package.genesis_id;
    let old_spec_dig = fx
        .transition_spec
        .tag()
        .map_err(|_| SuccessorActivation1Error::AuthorityStageRejected("old spec tag"))?
        .digest();
    let fork = ExecutorForkContextV1 {
        chain_id: fx.package.state.chain_id,
        world_id: fx.package.state.world_id,
        genesis_id: gid,
        profile_id: old_pid,
        transition_spec_digest: old_spec_dig,
    };
    let mut forbidden_actor_keys = BTreeSet::new();
    for (_, desc) in &fx.bundle.actor_descriptors {
        forbidden_actor_keys.extend(desc.action_policy.keys.iter().copied());
        forbidden_actor_keys.extend(desc.admin_policy.keys.iter().copied());
    }
    let vals = &fx.package.state.initial_validators;
    let val_stakes: Vec<_> = vals
        .iter()
        .map(|v| (ActorIdV1::from_bytes(v.validator_id), v.bonded_stake))
        .collect();
    let val_keys: Vec<_> = vals
        .iter()
        .map(|v| {
            (
                ActorIdV1::from_bytes(v.validator_id),
                ValidatorKeyPairV1 {
                    proposal_key: v.proposal_pubkey,
                    vote_key: v.vote_pubkey,
                },
            )
        })
        .collect();
    let econ_state =
        EconomicStateV1::from_genesis(fx.bundle.validator_econ_policy, old_pid, &val_stakes)
            .map_err(|_| SuccessorActivation1Error::AuthorityStageRejected("econ genesis"))?;
    let gov_ctx = GovernanceContextV1 {
        profile_id: old_pid,
        launch_manifest_id: gid,
        genesis_id: gid,
        chain_id: fx.package.state.chain_id,
        protocol_version: 1,
    };
    let gov_state = GovernanceStateV1::from_genesis(
        gov_ctx,
        fx.bundle.governance_policy.clone(),
        &fx.bundle.initial_gov_params,
    )
    .map_err(|_| SuccessorActivation1Error::AuthorityStageRejected("gov genesis"))?;
    let sched_state = ValidatorScheduleStateV1::from_genesis(
        fx.bundle.consensus_policy,
        fx.bundle.validator_econ_policy,
        old_pid,
        &econ_state,
        &gov_state,
        &val_keys,
        &forbidden_actor_keys,
    )
    .map_err(|_| SuccessorActivation1Error::AuthorityStageRejected("sched genesis"))?;
    let ep0_schedule = sched_state
        .schedule_at(0)
        .ok_or(SuccessorActivation1Error::AuthorityStageRejected(
            "missing ep0 schedule",
        ))?
        .clone();
    let ep0_sched_digest = ep0_schedule
        .tag()
        .map_err(|_| SuccessorActivation1Error::AuthorityStageRejected("ep0 sched tag"))?
        .digest();

    let receipts_merkle_root = upgrade_outcome.launch_parity_record.genesis_history_root;
    let old_exec_ref = upgrade_outcome.evidence_manifest.old_execution_root_ref;
    let body_digest = launch_build.package.launch_closure_bundle_id;
    let parent_core = build_consensus_proposal_core(
        gid,
        old_pid,
        [0u8; 32],
        1,
        0,
        0,
        receipts_merkle_root,
        &old_exec_ref,
        body_digest,
    )
    .map_err(|_| SuccessorActivation1Error::AuthorityStageRejected("parent proposal core"))?;
    let parent_core_id = parent_core
        .tag()
        .map_err(|_| SuccessorActivation1Error::AuthorityStageRejected("parent core tag"))?
        .digest();
    let parent_lookup_id = parent_core.accepted_parent_lookup_id;

    let leader_id = ActorIdV1::from_bytes([2u8; 32]);
    let leader_pseed = ConsensusClusterHarnessV1::validator_proposal_seed(2);
    let leader_ppk = CanonicalEd25519PublicKeyV1::from_secret_seed(&leader_pseed)
        .map_err(|_| SuccessorActivation1Error::AuthorityStageRejected("leader pk"))?;
    let proposal = ConsensusProposalV1 {
        proposal_version: CONSENSUS_1_VERSION,
        height: 1,
        epoch: 0,
        view: 0,
        parent_block_id: [0u8; 32],
        fork_context: fork,
        schedule_epoch_digest: ep0_sched_digest,
        body_digest,
        receipts_merkle_root,
        proposal_core_digest: parent_core_id,
        execution_root_ref: old_exec_ref,
        justify_qc_digest: [0u8; 32],
        justify_qc_view: 0,
        proposer_id: leader_id,
        proposal_pubkey: *leader_ppk.as_bytes(),
        signature: [0u8; 64],
    }
    .sign_with_seed(&leader_pseed)
    .map_err(|_| SuccessorActivation1Error::AuthorityStageRejected("sign h1 proposal"))?;
    let block_id = proposal.block_id();
    let exec_ref_dig = execution_root_ref_digest(&old_exec_ref);
    let mk_qc =
        |phase: ConsensusVotePhaseV1| -> Result<QuorumCertificateV1, SuccessorActivation1Error> {
            let mut votes = Vec::with_capacity(4);
            for i in 1u8..=4 {
                let vseed = ConsensusClusterHarnessV1::validator_vote_seed(i);
                let vpk = CanonicalEd25519PublicKeyV1::from_secret_seed(&vseed)
                    .map_err(|_| SuccessorActivation1Error::AuthorityStageRejected("vote pk"))?;
                let vote = ConsensusVoteV1 {
                    vote_version: CONSENSUS_1_VERSION,
                    height: 1,
                    epoch: 0,
                    view: 0,
                    phase,
                    block_id,
                    schedule_epoch_digest: ep0_sched_digest,
                    execution_root_ref_digest: exec_ref_dig,
                    has_full_body: true,
                    voter_id: ActorIdV1::from_bytes([i; 32]),
                    vote_pubkey: *vpk.as_bytes(),
                    signature: [0u8; 64],
                }
                .sign_with_seed(&vseed, &fork)
                .map_err(|_| SuccessorActivation1Error::AuthorityStageRejected("sign h1 vote"))?;
                votes.push(vote);
            }
            QuorumCertificateV1::assemble(&fork, &ep0_schedule, votes)
                .map_err(|_| SuccessorActivation1Error::AuthorityStageRejected("assemble h1 qc"))
        };
    let prepare_qc = mk_qc(ConsensusVotePhaseV1::Prepare)?;
    let commit_qc = mk_qc(ConsensusVotePhaseV1::Commit)?;
    let predecessor_commit_cert = ConsensusCommitCertificateV1 {
        cert_version: CONSENSUS_1_VERSION,
        proposal,
        prepare_qc,
        commit_qc,
    };
    predecessor_commit_cert
        .verify(&ep0_schedule)
        .map_err(|_| SuccessorActivation1Error::AuthorityStageRejected("verify h1 commit cert"))?;

    // Derive canonical arrival-permutation-invariant split-wrapper occurrence receipts over parent_core.
    let wrap_pre = SigningPreimageV1 {
        protocol_version: 1,
        chain_id: fork.chain_id,
        genesis_id: fork.genesis_id,
        launch_manifest_id: fork.genesis_id,
        profile_id: fork.profile_id,
        domain_tag: SigningDomainTagV1::BlockProposal,
        signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
        hash_suite: HashSuiteIdV1::Sha256V1,
        payload_digest: parent_core_id,
    };
    let mk_wrap = |v_idx: u8,
                   ev_b: u8,
                   dep_b: u8|
     -> Result<AcceptedProposalWrapperV1, SuccessorActivation1Error> {
        let pseed = ConsensusClusterHarnessV1::validator_proposal_seed(v_idx);
        let pk = CanonicalEd25519PublicKeyV1::from_secret_seed(&pseed)
            .map_err(|_| SuccessorActivation1Error::AuthorityStageRejected("wrap pk"))?;
        let sig = sign_preimage_deterministic(&pseed, &wrap_pre)
            .map_err(|_| SuccessorActivation1Error::AuthorityStageRejected("wrap sig"))?;
        Ok(AcceptedProposalWrapperV1 {
            core: parent_core.clone(),
            credentials: vec![ProposalCredentialEntryV1 {
                proposer_pubkey: pk,
                signature: sig,
            }],
            evidence_digests: vec![[ev_b; 32]],
            availability_dependencies: vec![SameWrapperAvailabilityDependencyV1 {
                dependency_id: [dep_b; 32],
                expected_bytes: 1024,
            }],
        })
    };
    let w1 = mk_wrap(1, 0xA1, 0xB1)?;
    let w2 = mk_wrap(2, 0xA2, 0xB2)?;
    let norm_12 = normalize_proposal_wrappers(&w1, &w2, fork.chain_id, gid, gid, old_pid)
        .map_err(|_| SuccessorActivation1Error::AuthorityStageRejected("norm 12"))?;
    let norm_21 = normalize_proposal_wrappers(&w2, &w1, fork.chain_id, gid, gid, old_pid)
        .map_err(|_| SuccessorActivation1Error::AuthorityStageRejected("norm 21"))?;
    if norm_12
        .tag()
        .map_err(|_| SuccessorActivation1Error::AuthorityStageRejected("norm tag"))?
        != norm_21
            .tag()
            .map_err(|_| SuccessorActivation1Error::AuthorityStageRejected("norm tag"))?
    {
        return Err(SuccessorActivation1Error::AuthorityStageRejected(
            "split wrapper normalization diverged",
        ));
    }
    let r1 =
        ConsensusNativeAcceptedRecordOccurrenceReceiptV1::from_wrapper(1, &w1, [0xC1; 32], false)
            .map_err(|_| SuccessorActivation1Error::AuthorityStageRejected("occ rcpt 1"))?;
    let r2 =
        ConsensusNativeAcceptedRecordOccurrenceReceiptV1::from_wrapper(2, &w2, [0xC2; 32], false)
            .map_err(|_| SuccessorActivation1Error::AuthorityStageRejected("occ rcpt 2"))?;
    let occ_rcpts = vec![r1, r2];
    let history_start =
        VerifiedConsensusNativeAcceptedRecordOccurrenceHistoryStartEqualityV1::exact(
            fork.genesis_id,
        );
    let cqc_dig = predecessor_commit_cert
        .commit_qc
        .qc_digest()
        .map_err(|_| SuccessorActivation1Error::AuthorityStageRejected("commit qc dig"))?;
    let val_signer_seeds: Vec<(ActorIdV1, [u8; 32])> = (1u8..=3)
        .map(|i| {
            (
                ActorIdV1::from_bytes([i; 32]),
                ConsensusClusterHarnessV1::validator_vote_seed(i),
            )
        })
        .collect();

    let predecessor_gov_root: [u8; 32] = {
        let mut h = Sha256::new();
        h.update(SA1_GOV_AUTH_ROOT_PREFIX);
        h.update(fork.genesis_id);
        h.update(fork.profile_id);
        h.update(parent_core_id);
        h.finalize().into()
    };
    let rel_cert_policy_id: [u8; 32] = Sha256::digest(SA1_RELEASE_CERT_POLICY_PREFIX).into();
    let act_cert_policy_id: [u8; 32] = Sha256::digest(SA1_ACT_CERT_POLICY_PREFIX).into();
    let rel_elig_pred_id: [u8; 32] = Sha256::digest(SA1_RELEASE_ELIG_PRED_PREFIX).into();
    let act_elig_pred_id: [u8; 32] = Sha256::digest(SA1_ACT_ELIG_PRED_PREFIX).into();

    // 2. Build authorization-free successor AdmittedReleaseCoreV1 FIRST (acyclic Stage 2).
    let (trusted_release_policy, release_keys) =
        TrustedReleaseAuthorityPolicyV1::devnet_three_of_five();
    let release_policy_id = trusted_release_policy
        .policy_id()
        .map_err(|_| SuccessorActivation1Error::AuthorityStageRejected("release policy id"))?;
    let (trusted_activation_policy, activation_keys) =
        TrustedActivationAuthorityPolicyV1::devnet_three_of_five();
    let activation_policy_id = trusted_activation_policy.policy_id()?;

    let succ_bound = &upgrade_outcome.successor_candidate.successor_bound_manifest;
    let succ_scope = &upgrade_outcome.successor_candidate.successor_scope;
    let succ_spec = &upgrade_outcome
        .successor_candidate
        .successor_transition_spec;
    let succ_reg = &upgrade_outcome.launch_fixture.commit_domain_registry;
    let new_profile_id = succ_bound.profile_id;
    let scope_id = succ_scope.tag().expect("scope tag").digest();
    let reg_id = succ_reg.tag().digest();
    let spec_id = succ_spec.tag().expect("spec tag").digest();
    let admission_id = succ_admission.tag().expect("adm tag").digest();
    let parity_id = upgrade_outcome
        .launch_parity_record
        .parity_record_id()
        .expect("parity id");
    let inputs_id = cand_inputs.tag().expect("inputs tag").digest();
    let rcpt_id = cand_receipt.tag().expect("rcpt tag").digest();
    let mig_spec_id = upgrade_outcome
        .migration_spec
        .tag()
        .expect("mig spec tag")
        .digest();
    let evidence_id = upgrade_outcome
        .evidence_manifest
        .tag()
        .expect("evidence tag")
        .digest();

    let rel_ctx_ref = ReleaseAuthorityContextRefV1::AcceptedParentReleaseAuthorityV1 {
        predecessor_accepted_parent_lookup_id: parent_lookup_id,
        purpose_code: ReleaseParentPurposeCodeV1::SuccessorReleaseAuthority,
        predecessor_governance_authority_root: predecessor_gov_root,
    };
    let rel_core = AdmittedReleaseCoreV1 {
        schema_version: 1,
        admitted_profile_id: new_profile_id,
        admitted_transition_spec_at_profile_id: spec_id,
        profile_admission_manifest_id: admission_id,
        genesis_id: fork.genesis_id,
        genesis_root_parity_record_id: parity_id,
        candidate_build_inputs_digest: inputs_id,
        candidate_release_receipt_digest: rcpt_id,
        framework_manifest_sha256: cand_receipt.framework_manifest_sha256,
        native_artifact_sha256: cand_receipt.native_artifact_sha256,
        native_artifact_len: cand_receipt.native_artifact_len,
        wasm_artifact_sha256: cand_receipt.wasm_artifact_sha256,
        wasm_artifact_len: cand_receipt.wasm_artifact_len,
        platform_target: cand_inputs.platform_target,
        release_authority_context_ref: rel_ctx_ref,
        release_authority_policy_id: release_policy_id,
        vulnerability_status: VulnerabilityWithdrawalStatusV1::Active,
    };
    let rel_core_id = rel_core
        .tag()
        .map_err(|_| SuccessorActivation1Error::BindingMismatch("rel core tag"))?
        .digest();

    // 3. Issue live CONSENSUS-1 ReleaseAuthority cert bound to `rel_core_id`, then sign ReleaseSignDocV1.
    let release_consumer_ctx = ParentAuthorityCertificateConsumerContextV1 {
        purpose: ParentAuthorityPurposeV1::ReleaseAuthority,
        consumer_chain_id: fork.chain_id,
        consumer_profile_id: fork.profile_id,
        consumer_object_digest: rel_core_id,
        required_position: 1,
        envelope_order_position: 1,
        release_signing_dependency_digest: mig_spec_id,
    };
    let release_parent_cert = AcceptedConsensusNativeParentAuthorityCertificateV1::issue_live(
        &fork,
        &ep0_schedule,
        fork.profile_id,
        parent_lookup_id,
        parent_core_id,
        release_consumer_ctx,
        history_start,
        2,
        1,
        1,
        occ_rcpts.clone(),
        ParentAuthorityStatusV1::AcceptedLive,
        rel_cert_policy_id,
        cqc_dig,
        &val_signer_seeds,
    )
    .map_err(|_| SuccessorActivation1Error::AuthorityStageRejected("issue live release cert"))?;
    let release_cert_id = release_parent_cert
        .tag()
        .map_err(|_| SuccessorActivation1Error::AuthorityStageRejected("rel cert tag"))?
        .digest();

    let rel_hist_ref =
        ReleaseHistoricalAuthorityRefV1::AcceptedParentReleaseHistoricalAuthorityRefV1 {
            parent_cert_id: release_cert_id,
            evidence_commitment: release_parent_cert.evidence_commitment_id,
            occurrence_universe_root: release_parent_cert.occurrence_universe_root,
            height: 1,
            epoch: 0,
            purpose_code: ReleaseParentPurposeCodeV1::SuccessorReleaseAuthority,
        };
    let rel_sign_doc = ReleaseSignDocV1 {
        schema_version: 1,
        admitted_release_core_id: rel_core_id,
        release_authority_context_ref: rel_ctx_ref,
        release_historical_authority_ref: rel_hist_ref,
        release_authority_policy_id: release_policy_id,
    };
    let rel_sd_id = rel_sign_doc
        .tag()
        .map_err(|_| SuccessorActivation1Error::BindingMismatch("rel sign doc tag"))?
        .digest();
    let rel_env = ReleaseAuthorizationSignDocEnvelopeV1 {
        sign_doc: rel_sign_doc,
    };
    let rel_pre = rel_env
        .signing_preimage(fork.chain_id, fork.genesis_id, new_profile_id)
        .map_err(|_| SuccessorActivation1Error::BindingMismatch("rel preimage"))?;
    let mut rel_sigs = Vec::with_capacity(3);
    for idx in [0usize, 1, 2] {
        let (pk, seed) = release_keys[idx];
        let signature = sign_preimage_deterministic(&seed, &rel_pre)
            .map_err(|_| SuccessorActivation1Error::AuthorityStageRejected("rel sign"))?;
        rel_sigs.push(ReleaseSignerSignatureV1 {
            signer_index: idx as u8,
            public_key: pk,
            signature,
        });
    }
    let succ_release = AdmittedReleaseRecordV1 {
        schema_version: 1,
        core: rel_core,
        admitted_release_core_id: rel_core_id,
        authorization: ReleaseAuthorizationV1 {
            auth_schema_version: 1,
            sign_doc: rel_sign_doc,
            release_sign_doc_id: rel_sd_id,
            signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
            release_authority_credential_set_id: release_policy_id,
            signatures: rel_sigs,
        },
    };
    let release_id = succ_release
        .tag()
        .map_err(|_| SuccessorActivation1Error::BindingMismatch("succ release tag"))?
        .digest();

    // 4. Construct internal-only SuccessorActivationBundleV1 (|S|=3 canonical or |S|=0 minimal).
    let scope_b = succ_scope.encode_canonical().expect("scope enc");
    let reg_b = succ_reg.encode_canonical();
    let cand_b = succ_bound.encode_canonical().expect("cand enc");
    let spec_b = succ_spec.encode_canonical().expect("spec enc");
    let adm_b = succ_admission.encode_canonical().expect("adm enc");
    let rel_b = succ_release.encode_canonical().expect("rel enc");
    let ev_b = upgrade_outcome
        .evidence_manifest
        .encode_canonical()
        .expect("ev enc");

    let mut supp_artifacts = Vec::new();
    let mut extra_nodes = Vec::new();
    let mut ev_deps = vec![new_profile_id, spec_id];
    let mut rel_deps = vec![new_profile_id, spec_id, admission_id];

    if include_supplementary {
        let ms_b = upgrade_outcome
            .migration_spec
            .encode_canonical()
            .expect("ms enc")
            .to_vec();
        let in_b = cand_inputs.encode_canonical().expect("in enc").to_vec();
        let rc_b = cand_receipt.encode_canonical().expect("rc enc").to_vec();
        supp_artifacts.push(SupplementarySuccessorActivationArtifactV1 {
            typed_artifact_schema_id: UPGRADE_1_MIGRATION_SPEC_SCHEMA.to_vec(),
            typed_artifact_id: mig_spec_id,
            canonical_artifact_preimage_length_u64: ms_b.len() as u64,
            canonical_artifact_preimage_bytes: ms_b,
        });
        supp_artifacts.push(SupplementarySuccessorActivationArtifactV1 {
            typed_artifact_schema_id: RELEASE_R1_BUILD_INPUTS_SCHEMA.to_vec(),
            typed_artifact_id: inputs_id,
            canonical_artifact_preimage_length_u64: in_b.len() as u64,
            canonical_artifact_preimage_bytes: in_b,
        });
        supp_artifacts.push(SupplementarySuccessorActivationArtifactV1 {
            typed_artifact_schema_id: RELEASE_R1_CANDIDATE_RECEIPT_SCHEMA.to_vec(),
            typed_artifact_id: rcpt_id,
            canonical_artifact_preimage_length_u64: rc_b.len() as u64,
            canonical_artifact_preimage_bytes: rc_b,
        });
        supp_artifacts.sort_by_key(|s| s.typed_artifact_id);
        extra_nodes.push(mk_internal_node(
            4,
            mig_spec_id,
            vec![new_profile_id, spec_id],
        ));
        extra_nodes.push(mk_internal_node(4, inputs_id, vec![new_profile_id]));
        extra_nodes.push(mk_internal_node(
            5,
            rcpt_id,
            vec![new_profile_id, inputs_id],
        ));
        ev_deps.push(mig_spec_id);
        rel_deps.push(inputs_id);
        rel_deps.push(rcpt_id);
    }

    let mut nodes = vec![
        mk_internal_node(0, reg_id, vec![]),
        mk_internal_node(1, scope_id, vec![reg_id]),
        mk_internal_node(2, new_profile_id, vec![reg_id, scope_id]),
        mk_internal_node(3, spec_id, vec![scope_id, new_profile_id]),
        mk_internal_node(5, evidence_id, ev_deps),
        mk_internal_node(
            6,
            admission_id,
            vec![reg_id, scope_id, new_profile_id, spec_id, evidence_id],
        ),
        mk_internal_node(7, release_id, rel_deps),
    ];
    nodes.extend(extra_nodes);
    nodes.sort_by_key(|n| n.typed_internal_artifact_id);

    let explicit_lengths = [
        scope_b.len() as u64,
        reg_b.len() as u64,
        cand_b.len() as u64,
        spec_b.len() as u64,
        adm_b.len() as u64,
        rel_b.len() as u64,
        ev_b.len() as u64,
    ];
    let internal_bytes: u64 = explicit_lengths.iter().sum::<u64>()
        + supp_artifacts
            .iter()
            .map(|s| s.canonical_artifact_preimage_length_u64)
            .sum::<u64>();
    let edge_count: u32 = nodes
        .iter()
        .map(|n| n.internal_dependency_refs.len() as u32)
        .sum();
    let supp_count = supp_artifacts.len() as u32;

    let bundle = SuccessorActivationBundleV1 {
        schema_version: SUCCESSOR_ACTIVATION_1_VERSION,
        predecessor_admitted_profile_id: fork.profile_id,
        successor_admitted_profile_id: new_profile_id,
        successor_profile_admission_manifest_id: admission_id,
        successor_transition_spec_at_profile_id: spec_id,
        successor_release_record_id: release_id,
        migration_spec_id: mig_spec_id,
        successor_upgrade_evidence_manifest_id: evidence_id,
        internal_artifact_count_u32: EXPLICIT_SUCCESSOR_ARTIFACT_COUNT + supp_count,
        supplementary_internal_artifact_count_u32: supp_count,
        internal_dependency_edge_count_u32: edge_count,
        internal_canonical_preimage_bytes_u64: internal_bytes,
        explicit_internal_preimage_lengths_u64: explicit_lengths,
        successor_profile_scope_bytes: scope_b,
        successor_domain_registry_bytes: reg_b,
        successor_candidate_profile_bytes: cand_b,
        successor_transition_spec_bytes: spec_b,
        successor_profile_admission_manifest_bytes: adm_b,
        successor_admitted_release_record_bytes: rel_b,
        successor_upgrade_evidence_manifest_bytes: ev_b,
        internal_dependency_nodes: nodes,
        supplementary_internal_artifacts: supp_artifacts,
    };
    let bundle_id = bundle.tag()?.digest();

    // 5. ONLY AFTER `bundle_id` exists, issue distinct live CONSENSUS-1 ActivationAuthority cert.
    let activation_consumer_ctx = ParentAuthorityCertificateConsumerContextV1 {
        purpose: ParentAuthorityPurposeV1::ActivationAuthority,
        consumer_chain_id: fork.chain_id,
        consumer_profile_id: fork.profile_id,
        consumer_object_digest: bundle_id,
        required_position: 1,
        envelope_order_position: 1,
        release_signing_dependency_digest: release_id,
    };
    let activation_parent_cert = AcceptedConsensusNativeParentAuthorityCertificateV1::issue_live(
        &fork,
        &ep0_schedule,
        fork.profile_id,
        parent_lookup_id,
        parent_core_id,
        activation_consumer_ctx,
        history_start,
        2,
        1,
        1,
        occ_rcpts,
        ParentAuthorityStatusV1::AcceptedLive,
        act_cert_policy_id,
        cqc_dig,
        &val_signer_seeds,
    )
    .map_err(|_| SuccessorActivation1Error::AuthorityStageRejected("issue live activation cert"))?;

    // 6. Construct SuccessorActivationSignDocV1 & sign 3-of-5 Ed25519 ActivationAuthorizationV1.
    let hist_act_ref =
        HistoricalAcceptedParentAuthorityUseRefV1::from_live_cert(&activation_parent_cert)?;
    let chain_launch_manifest_id = launch_build.package.chain_launch_manifest_id;
    let activation_height = upgrade_outcome.migration_spec.activation_height;
    let act_sign_doc = SuccessorActivationSignDocV1 {
        schema_version: SUCCESSOR_ACTIVATION_1_VERSION,
        chain_launch_manifest_id,
        predecessor_accepted_parent_lookup_id: parent_lookup_id,
        predecessor_authority_purpose: ParentAuthorityPurposeV1::ActivationAuthority,
        predecessor_governance_authority_root: predecessor_gov_root,
        historical_predecessor_activation_authority_use_ref: hist_act_ref,
        old_active_profile_id: fork.profile_id,
        new_admitted_profile_id: new_profile_id,
        successor_profile_admission_manifest_id: admission_id,
        migration_spec_id: mig_spec_id,
        successor_activation_bundle_id: bundle_id,
        activation_height,
        activation_old_execution_root_ref: upgrade_outcome.evidence_manifest.old_execution_root_ref,
        activation_expected_new_execution_root_ref: upgrade_outcome
            .evidence_manifest
            .expected_new_execution_root_ref,
        activation_authority_policy_id: activation_policy_id,
    };
    let act_sd_id = act_sign_doc.tag()?.digest();
    let act_pre = ActivationAuthorizationV1::signing_preimage(
        fork.chain_id,
        fork.genesis_id,
        chain_launch_manifest_id,
        fork.profile_id,
        act_sd_id,
    );
    let mut act_sigs = Vec::with_capacity(3);
    for idx in [0usize, 1, 2] {
        let (pk, seed) = activation_keys[idx];
        let signature = sign_preimage_deterministic(&seed, &act_pre).map_err(|_| {
            SuccessorActivation1Error::InvalidActivationAuthorization("activation sign failed")
        })?;
        act_sigs.push(ActivationSignerSignatureV1 {
            signer_index: idx as u8,
            public_key: pk,
            signature,
        });
    }
    let authorization = ActivationAuthorizationV1 {
        schema_version: SUCCESSOR_ACTIVATION_1_VERSION,
        sign_doc: act_sign_doc,
        successor_activation_sign_doc_id: act_sd_id,
        activation_authority_policy_id: activation_policy_id,
        signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
        activation_authority_credential_set_id: activation_policy_id,
        signatures: act_sigs,
    };
    let auth_id = authorization.tag()?.digest();

    // 7. Wrap DedicatedMigrationBodyV1 + availability commitment into SuccessorActivationCarrierV1.
    let dedicated_body = DedicatedMigrationBodyV1 {
        signed_transaction_count: 0,
        application_subtick_count: 0,
        system_transitions: vec![AuthorizedMigrationSystemTransitionV1 {
            successor_profile_admission_manifest_id: admission_id,
            migration_spec_id: mig_spec_id,
            successor_activation_bundle_id: bundle_id,
            activation_authorization_id: auth_id,
            activation_height,
        }],
        deterministic_migration_inputs_digest: upgrade_outcome
            .evidence_manifest
            .canonical_parameter_and_state_delta_digest,
    };
    let bundle_b = bundle.encode_canonical()?;
    let auth_b = authorization.encode_canonical()?;
    let body_b = dedicated_body.encode_canonical()?;
    let avail_commit = SuccessorActivationCarrierV1::compute_availability_commitment(
        fork.profile_id,
        &bundle_b,
        &auth_b,
        &body_b,
    );
    let carrier = SuccessorActivationCarrierV1::EmbeddedSuccessorActivationV1 {
        bundle: bundle.clone(),
        successor_activation_bundle_id: bundle_id,
        authorization: authorization.clone(),
        activation_authorization_id: auth_id,
        dedicated_migration_body: dedicated_body,
        availability_commitment: avail_commit,
    };

    let base_context = VerifiedSuccessorPredecessorBaseContextV1 {
        chain_launch_manifest_id,
        genesis_package: upgrade_outcome.launch_fixture.package.clone(),
        launch_parity_record: upgrade_outcome.launch_parity_record.clone(),
        old_profile_id: fork.profile_id,
        old_profile_scope_id: upgrade_outcome
            .launch_fixture
            .profile_scope
            .tag()
            .expect("old scope tag")
            .digest(),
        old_transition_spec_id: upgrade_outcome
            .launch_fixture
            .transition_spec
            .tag()
            .expect("old spec tag")
            .digest(),
        fork_context: fork,
        active_schedule: ep0_schedule,
        old_execution_root_ref: upgrade_outcome.evidence_manifest.old_execution_root_ref,
        trusted_release_policy,
        trusted_activation_policy,
        verified_release_parent_eligibility: VerifiedParentEligibilityV1 {
            accepted_parent_lookup_id: parent_lookup_id,
            unique_accepted_proposal_core_block_id: parent_core_id,
            governance_authority_root: predecessor_gov_root,
            purpose: ParentAuthorityPurposeV1::ReleaseAuthority,
            policy_id: rel_cert_policy_id,
            eligibility_predicate_id: rel_elig_pred_id,
            status: ParentAuthorityStatusV1::AcceptedLive,
            required_position: 1,
            evidence_cutoff_position: 2,
            evidence_commitment_id: release_parent_cert.evidence_commitment_id,
            occurrence_universe_root: release_parent_cert.occurrence_universe_root,
            total_occurrence_count: release_parent_cert.occurrence_receipts.len() as u32,
        },
        verified_activation_parent_eligibility: VerifiedParentEligibilityV1 {
            accepted_parent_lookup_id: parent_lookup_id,
            unique_accepted_proposal_core_block_id: parent_core_id,
            governance_authority_root: predecessor_gov_root,
            purpose: ParentAuthorityPurposeV1::ActivationAuthority,
            policy_id: act_cert_policy_id,
            eligibility_predicate_id: act_elig_pred_id,
            status: ParentAuthorityStatusV1::AcceptedLive,
            required_position: 1,
            evidence_cutoff_position: 2,
            evidence_commitment_id: activation_parent_cert.evidence_commitment_id,
            occurrence_universe_root: activation_parent_cert.occurrence_universe_root,
            total_occurrence_count: activation_parent_cert.occurrence_receipts.len() as u32,
        },
    };

    // Self-verify full 3-stage pipeline before returning.
    let _ = verify_successor_activation_carrier(
        &carrier,
        &base_context,
        &release_parent_cert,
        &activation_parent_cert,
        SuccessorPreflightCapsV1::V1_DEFAULT,
    )?;

    Ok(CanonicalDevnetSuccessorActivationBuildV1 {
        launch_build,
        upgrade_outcome,
        predecessor_commit_cert,
        base_context,
        release_parent_cert,
        activation_parent_cert,
        bundle,
        authorization,
        carrier,
    })
}

fn mk_internal_node(
    rank: u32,
    id: [u8; 32],
    mut deps: Vec<[u8; 32]>,
) -> SuccessorActivationInternalDependencyNodeV1 {
    deps.sort();
    deps.dedup();
    SuccessorActivationInternalDependencyNodeV1 {
        topological_rank_u32: rank,
        typed_internal_artifact_id: id,
        internal_dependency_refs: deps,
    }
}

fn take_u8(raw: &[u8], p: &mut usize) -> Result<u8, SuccessorActivation1Error> {
    if *p >= raw.len() {
        return Err(SuccessorActivation1Error::PreflightRejected(
            "unexpected EOF u8",
        ));
    }
    let v = raw[*p];
    *p += 1;
    Ok(v)
}

fn take_u16(raw: &[u8], p: &mut usize) -> Result<u16, SuccessorActivation1Error> {
    let s = take_slice(raw, p, 2)?;
    Ok(u16::from_be_bytes([s[0], s[1]]))
}

fn take_u32(raw: &[u8], p: &mut usize) -> Result<u32, SuccessorActivation1Error> {
    let s = take_slice(raw, p, 4)?;
    Ok(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

fn take_u64(raw: &[u8], p: &mut usize) -> Result<u64, SuccessorActivation1Error> {
    let s = take_slice(raw, p, 8)?;
    Ok(u64::from_be_bytes([
        s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7],
    ]))
}

fn take_32(raw: &[u8], p: &mut usize) -> Result<[u8; 32], SuccessorActivation1Error> {
    let s = take_slice(raw, p, 32)?;
    let mut out = [0u8; 32];
    out.copy_from_slice(s);
    Ok(out)
}

fn take_64(raw: &[u8], p: &mut usize) -> Result<[u8; 64], SuccessorActivation1Error> {
    let s = take_slice(raw, p, 64)?;
    let mut out = [0u8; 64];
    out.copy_from_slice(s);
    Ok(out)
}

fn take_slice<'a>(
    raw: &'a [u8],
    p: &mut usize,
    len: usize,
) -> Result<&'a [u8], SuccessorActivation1Error> {
    let end = p
        .checked_add(len)
        .ok_or(SuccessorActivation1Error::PreflightRejected(
            "slice cursor overflow",
        ))?;
    if end > raw.len() {
        return Err(SuccessorActivation1Error::PreflightRejected(
            "truncated wire slice",
        ));
    }
    let out = &raw[*p..end];
    *p = end;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successor_activation_1_golden_tags_and_roundtrip() {
        let s3 = build_canonical_devnet_successor_activation(true).expect("s3 build");
        let s0 = build_canonical_devnet_successor_activation(false).expect("s0 build");
        assert_eq!(s3.bundle.internal_artifact_count_u32, 10);
        assert_eq!(s3.bundle.supplementary_internal_artifact_count_u32, 3);
        assert_eq!(s3.bundle.internal_dependency_edge_count_u32, 23);
        assert_eq!(s0.bundle.internal_artifact_count_u32, 7);
        assert_eq!(s0.bundle.supplementary_internal_artifact_count_u32, 0);
        assert_eq!(s0.bundle.internal_dependency_edge_count_u32, 15);

        let b_enc = s3.bundle.encode_canonical().expect("b enc");
        let b_dec = SuccessorActivationBundleV1::decode_canonical(
            &b_enc,
            SuccessorPreflightCapsV1::V1_DEFAULT,
        )
        .expect("b dec");
        assert_eq!(b_dec, s3.bundle);

        let c_enc = s3.carrier.encode_canonical().expect("c enc");
        let c_dec = SuccessorActivationCarrierV1::decode_canonical(
            &c_enc,
            SuccessorPreflightCapsV1::V1_DEFAULT,
        )
        .expect("c dec");
        assert_eq!(c_dec, s3.carrier);

        let noop = SuccessorActivationCarrierV1::NoSuccessorActivationV1;
        let noop_enc = noop.encode_canonical().expect("noop enc");
        assert_eq!(
            SuccessorActivationCarrierV1::decode_canonical(
                &noop_enc,
                SuccessorPreflightCapsV1::V1_DEFAULT
            )
            .expect("noop dec"),
            noop
        );

        let b_tag = s3.bundle.tag().expect("b tag").to_string();
        let sd_tag = s3.authorization.sign_doc.tag().expect("sd tag").to_string();
        let auth_tag = s3.authorization.tag().expect("auth tag").to_string();
        let car_tag = s3.carrier.tag().expect("car tag").to_string();
        let s0_car_tag = s0.carrier.tag().expect("s0 car tag").to_string();
        assert_eq!(
            b_tag,
            "#bb.receipt.sha256.cxa644n2sko6qt6o7duozk233o44gcnnyewn5fmi675sa3xxdowa"
        );
        assert_eq!(
            sd_tag,
            "#bb.policy.sha256.6iyowm3ua37gjffzadytwdxkfuzpczstvicug62kmoevvyjwfamq"
        );
        assert_eq!(
            auth_tag,
            "#bb.receipt.sha256.75nq5siuuiml4z32d2rspnijjz5vub3rt7nejvmpeks3zdzvfnla"
        );
        assert_eq!(
            car_tag,
            "#bb.receipt.sha256.rsp5f3zibybn6kywjbeqjld2i6ccuta6daczhzkwyqacfc22klwq"
        );
        assert_eq!(
            s0_car_tag,
            "#bb.receipt.sha256.7jwabo3fyxhattv2hjveqsahguti3sdkc3eyivy6352cnf7hrnda"
        );
    }
}
