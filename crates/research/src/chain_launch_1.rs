//! `CHAIN-LAUNCH-1@base` (`PS-091`): embedded-only [`LaunchClosureBundleV1`], zero-allocation
//! streaming fixed-width preflight, combined launch-closure DAG rank enforcement,
//! [`ChainLaunchManifestV1`], genesis-committed `3-of-5` Ed25519 [`ChainLaunchAuthorizationV1`],
//! and complete [`ChainLaunchPackageV1`].
//!
//! Strictly behind `--features protocol-research`, 100% `wasm32`-clean, zero new Cargo
//! dependencies, and zero edits to frozen crates or modules. Formally accompanied by
//! `proof/BitblockProof/LaunchClosureRefinement.lean` (`lake env lean --trust=0`).
//!
//! # Normative Boundaries (`CAPABILITY_TREE.md` / `LAYER1_DESIGN_PAPER.md` §2 lines 865–1197)
//! - **Embedded-only closure:** [`LaunchClosureBundleV1`] carries all seven explicit preimages
//!   (`ProfileScopeV1`, `DomainRegistryV1`, `BoundProfileManifestV1`,
//!   `ResolvedProfileTransitionSpecV1`, `ProfileAdmissionManifestV1`, `GenesisRootParityRecordV1`,
//!   `AdmittedReleaseRecordV1`) plus optional bounded supplementary preimages (`|S| = 3` canonical
//!   or `|S| = 0` minimal). Locator-only tags or ambient lookups reject fail-closed.
//! - **Zero-allocation streaming preflight:** [`preflight_launch_closure_bundle_bytes`] checks all
//!   count, length, byte-sum, cap, and framing equations using checked arithmetic BEFORE any
//!   `Vec` allocation, copying, hashing, or nested decode.
//! - **Combined DAG & non-zero genesis rank:** External root `X` (`GenesisPackageV1`) has exactly
//!   two outgoing refs (`ProfileScopeV1`, `DomainRegistryV1`) and therefore strictly `rank(X) >= 2`
//!   (never rank `0`). `TransitionSpecAtProfileV1` and `ProfileAdmissionManifestV1` never hold
//!   direct edges to `X`.
//! - **Detached genesis-committed launch authority:** `ChainLaunchManifestV1` excludes its own ID;
//!   `ChainLaunchAuthorizationV1` signs `chain_launch_manifest_id` under
//!   `SigningDomainTagV1::GovernanceAction` using the `G_0`-committed `3-of-5` policy
//!   (`DEVNET-TEST-KEYS-NOT-PRODUCTION-CUSTODY`).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use sha2::{Digest, Sha256};

use crate::commit_state::DomainRegistryV1;
use crate::crypto_d1::{
    CanonicalEd25519PublicKeyV1, CanonicalEd25519SignatureV1, HashSuiteIdV1, SignatureSuiteIdV1,
    SigningDomainTagV1, SigningPreimageV1, sign_preimage_deterministic, verify_preimage_strict,
};
use crate::execution_root_1::{
    EXECUTION_ROOT_1_COMPOSITION_SCHEMA, ExecutionRootCompositionV1, ExecutionRootRefV1,
};
use crate::genesis_d1::{GENESIS_D1_PACKAGE_SCHEMA, GenesisPackageV1};
use crate::genesis_root_parity_1::{
    CanonicalDevnetGenesisFixtureV1, GenesisParityInputV1, GenesisRootParityRecordV1,
    verify_genesis_root_parity,
};
use crate::profile_1::{ProfileAdmissionManifestV1, build_canonical_devnet_profile_admission};
use crate::profile_bootstrap::ProfileScopeV1;
use crate::profile_d1::BoundProfileManifestV1;
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::release_1::{
    AdmittedReleaseRecordV1, TrustedReleaseAuthorityPolicyV1,
    build_canonical_devnet_admitted_release, verify_admitted_release_record,
};
use crate::release_r1::{
    CandidateBuildInputsV1, CandidateReleaseReceiptV1, RELEASE_R1_BUILD_INPUTS_SCHEMA,
    RELEASE_R1_CANDIDATE_RECEIPT_SCHEMA, build_canonical_candidate_fixture,
};
use crate::transition_spec_1::ResolvedProfileTransitionSpecV1;

/// Canonical schema version for `CHAIN-LAUNCH-1` objects.
pub const CHAIN_LAUNCH_1_VERSION: u16 = 1;

/// Fixed count of mandatory explicit preimages in [`LaunchClosureBundleV1`] (`Length(E) = 7`).
pub const EXPLICIT_LAUNCH_ARTIFACT_COUNT: u32 = 7;

/// Fixed count of external genesis roots (`external_root_count_u32 = 1`).
pub const EXTERNAL_GENESIS_ROOT_COUNT: u32 = 1;

/// Fixed count of outgoing edges from the external genesis node (`Length(GenesisInternalRefs(X)) = 2`).
pub const EXTERNAL_GENESIS_OUTGOING_REF_COUNT: u32 = 2;

/// Canonical schema string for [`LaunchClosureBundleV1`].
pub const CHAIN_LAUNCH_1_BUNDLE_SCHEMA: &[u8] = b"bitblock/launch-closure-bundle/v1";

/// Canonical schema string for [`ChainLaunchManifestV1`].
pub const CHAIN_LAUNCH_1_MANIFEST_SCHEMA: &[u8] = b"bitblock/chain-launch-manifest/v1";

/// Canonical schema string for [`ChainLaunchAuthorizationV1`].
pub const CHAIN_LAUNCH_1_AUTH_SCHEMA: &[u8] = b"bitblock/chain-launch-authorization/v1";

/// Canonical schema string for complete [`ChainLaunchPackageV1`].
pub const CHAIN_LAUNCH_1_PACKAGE_SCHEMA: &[u8] = b"bitblock/chain-launch-1/package/v1";

const CHL1_POLICY_PREFIX: &[u8] = b"bb.chl1.trusted_launch_authority_policy.v1";
const CHL1_GENESIS_PARENT_PREFIX: &[u8] = b"bb.chl1.genesis_parent_block_id.v1";

/// Embedded preimage carrier discriminant (`1` is the SOLE legal mode; any locator rejects).
pub const EMBEDDED_PREIMAGE_CARRIER_MODE_V1: u8 = 1;

/// Fail-closed preflight, equation, DAG, binding, or authorization error for `CHAIN-LAUNCH-1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChainLaunch1Error {
    /// Streaming preflight rejected truncation, trailing bytes, unknown/locator tag, or over-cap frame.
    PreflightRejected(&'static str),
    /// One of the `LAYER1_DESIGN_PAPER.md` §2 artifact, edge, or byte-sum equations failed.
    EquationViolation(&'static str),
    /// Combined launch-closure DAG violated rank monotonicity, edge class, or node bijection.
    DagViolation(&'static str),
    /// Decoded preimage or manifest field disagreed with its recomputed canonical owner.
    BindingMismatch(&'static str),
    /// Detached `3-of-5` genesis-committed launch authorization failed verification.
    InvalidLaunchAuthorization(&'static str),
}

impl fmt::Display for ChainLaunch1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ChainLaunch1Error {}

/// Protocol-frozen streaming preflight bounds enforced before any allocation or nested decode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LaunchPreflightCapsV1 {
    /// Maximum wire byte length of an entire [`LaunchClosureBundleV1`] frame.
    pub max_bundle_frame_bytes: u64,
    /// Maximum summed canonical preimage bytes (`total_canonical_preimage_bytes_u64`).
    pub max_total_canonical_preimage_bytes: u64,
    /// Maximum byte length of any single embedded artifact preimage.
    pub max_single_preimage_bytes: u64,
    /// Maximum number of supplementary embedded artifacts (`|S|`).
    pub max_supplementary_artifacts: u32,
    /// Maximum total combined dependency edges (`complete_dependency_edge_count_u32`).
    pub max_total_dependency_edges: u32,
}

impl LaunchPreflightCapsV1 {
    /// Protocol-frozen v1 conservative launch preflight caps.
    pub const V1_DEFAULT: Self = Self {
        max_bundle_frame_bytes: 1_048_576,
        max_total_canonical_preimage_bytes: 1_048_576,
        max_single_preimage_bytes: 262_144,
        max_supplementary_artifacts: 64,
        max_total_dependency_edges: 256,
    };
}

/// Permitted internal-to-external dependency edge classes targeting `X` (`GenesisPackageV1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ExternalGenesisIncomingDependencyClassV1 {
    /// Mandatory initial-candidate genesis identity edge from `CandidateProfileV1`.
    InitialCandidateGenesisIdentityV1 = 1,
    /// Genesis-state derivation edge from `GenesisRootParityRecordV1`.
    GenesisStateDerivationV1 = 2,
    /// Genesis release-authority edge from `AdmittedReleaseRecordV1`.
    GenesisReleaseAuthorityV1 = 3,
}

impl ExternalGenesisIncomingDependencyClassV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte incoming genesis edge class.
    pub const fn from_code(code: u8) -> Result<Self, ChainLaunch1Error> {
        match code {
            1 => Ok(Self::InitialCandidateGenesisIdentityV1),
            2 => Ok(Self::GenesisStateDerivationV1),
            3 => Ok(Self::GenesisReleaseAuthorityV1),
            _ => Err(ChainLaunch1Error::PreflightRejected(
                "unknown ExternalGenesisIncomingDependencyClassV1",
            )),
        }
    }
}

/// Permitted outgoing dependency edge classes from `X` (`GenesisPackageV1`) to internal nodes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ExternalGenesisOutgoingDependencyClassV1 {
    /// Outgoing edge from `X` to the carried `ProfileScopeV1`.
    GenesisInitialScopeV1 = 1,
    /// Outgoing edge from `X` to the carried `DomainRegistryV1`.
    GenesisInitialDomainRegistryV1 = 2,
}

impl ExternalGenesisOutgoingDependencyClassV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte outgoing genesis edge class.
    pub const fn from_code(code: u8) -> Result<Self, ChainLaunch1Error> {
        match code {
            1 => Ok(Self::GenesisInitialScopeV1),
            2 => Ok(Self::GenesisInitialDomainRegistryV1),
            _ => Err(ChainLaunch1Error::PreflightRejected(
                "unknown ExternalGenesisOutgoingDependencyClassV1",
            )),
        }
    }
}

/// Typed dependency reference carried inside [`LaunchClosureInternalDependencyNodeV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LaunchClosureDependencyRefV1 {
    /// Dependency on another internal embedded artifact (wire tag `1`).
    InternalLaunchArtifactV1([u8; 32]),
    /// Typed dependency on the sole external genesis root `X` (wire tag `2`).
    ExternalGenesisRootV1 {
        /// Referenced `genesis_id`.
        genesis_id: [u8; 32],
        /// Semantic class restricting which referrer may emit this edge.
        edge_class: ExternalGenesisIncomingDependencyClassV1,
    },
}

impl LaunchClosureDependencyRefV1 {
    /// Sort/uniqueness key `(tag, id, class_code)` matching canonical wire order.
    fn sort_key(&self) -> (u8, [u8; 32], u8) {
        match *self {
            Self::InternalLaunchArtifactV1(id) => (1, id, 0),
            Self::ExternalGenesisRootV1 {
                genesis_id,
                edge_class,
            } => (2, genesis_id, edge_class.code()),
        }
    }
}

/// One outgoing dependency edge from the external genesis root `X` to an internal artifact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ExternalGenesisToInternalDependencyRefV1 {
    /// Typed digest of the internal target (`ProfileScopeV1` or `DomainRegistryV1`).
    pub typed_internal_artifact_id: [u8; 32],
    /// Outgoing dependency class.
    pub outgoing_class: ExternalGenesisOutgoingDependencyClassV1,
}

/// External genesis root node `X` representing the outer `genesis_package` inside the DAG.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchClosureExternalGenesisDependencyNodeV1 {
    /// Must equal `GENESIS_D1_PACKAGE_SCHEMA`.
    pub external_root_schema_id: Vec<u8>,
    /// Strictly positive checked topological rank (`>= 2` in valid closures; NEVER `0`).
    pub external_root_topological_rank_u32: u32,
    /// Outer `genesis_package.genesis_id`.
    pub genesis_id: [u8; 32],
    /// Exact canonical byte length of the outer `genesis_package` (`Length(G)`).
    pub canonical_outer_genesis_package_length_u64: u64,
    /// Strictly sorted & unique outgoing refs (`Length == 2`: scope + domain registry).
    pub outgoing_refs: Vec<ExternalGenesisToInternalDependencyRefV1>,
}

/// One internal artifact node inside the combined launch-closure dependency DAG.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchClosureInternalDependencyNodeV1 {
    /// Checked topological rank (strictly greater than every dependency target's rank).
    pub topological_rank_u32: u32,
    /// Unique typed artifact identifier matching one explicit or supplementary preimage.
    pub typed_artifact_id: [u8; 32],
    /// Strictly sorted & unique outgoing dependency refs from this internal artifact.
    pub dependency_refs: Vec<LaunchClosureDependencyRefV1>,
}

/// One embedded supplementary artifact (`SupplementaryLaunchArtifactV1`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SupplementaryLaunchArtifactV1 {
    /// Canonical schema string identifying the artifact codec.
    pub typed_artifact_schema_id: Vec<u8>,
    /// Recomputed typed artifact identifier.
    pub typed_artifact_id: [u8; 32],
    /// Declared canonical preimage length (`== canonical_artifact_preimage_bytes.len()`).
    pub canonical_artifact_preimage_length_u64: u64,
    /// Exact embedded canonical preimage bytes (no locator variant exists).
    pub canonical_artifact_preimage_bytes: Vec<u8>,
}

/// Verified zero-allocation summary produced by [`preflight_launch_closure_bundle_bytes`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreflightBundleSummaryV1 {
    /// Verified `internal_artifact_count_u32` (`7 + |S|`).
    pub internal_artifact_count_u32: u32,
    /// Verified `external_root_count_u32` (`1`).
    pub external_root_count_u32: u32,
    /// Verified `complete_artifact_count_u32` (`8 + |S|`).
    pub complete_artifact_count_u32: u32,
    /// Verified `supplementary_artifact_count_u32` (`|S|`).
    pub supplementary_artifact_count_u32: u32,
    /// Verified `internal_to_internal_dependency_edge_count_u32`.
    pub i2i_edges: u32,
    /// Verified `internal_to_external_dependency_edge_count_u32`.
    pub i2e_edges: u32,
    /// Verified `external_to_internal_dependency_edge_count_u32` (`2`).
    pub e2i_edges: u32,
    /// Verified `complete_dependency_edge_count_u32`.
    pub complete_edges: u32,
    /// Verified `internal_canonical_preimage_bytes_u64`.
    pub internal_preimage_bytes: u64,
    /// Verified `external_root_canonical_preimage_bytes_u64` (`Length(G)`).
    pub external_preimage_bytes: u64,
    /// Verified `total_canonical_preimage_bytes_u64`.
    pub total_preimage_bytes: u64,
    /// Verified `external_root_topological_rank_u32` (`> 0`).
    pub external_root_rank: u32,
    /// External `genesis_id` parsed from `X`.
    pub genesis_id: [u8; 32],
    /// Verified explicit preimage byte lengths `[E0..E6]`.
    pub explicit_preimage_lengths_u64: [u64; 7],
}

/// Complete embedded-only launch-closure retrieval bundle (`bitblock/launch-closure-bundle/v1`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchClosureBundleV1 {
    /// Schema version (`1`).
    pub schema_version: u16,
    /// `7 + supplementary_artifact_count_u32`.
    pub internal_artifact_count_u32: u32,
    /// Always `1`.
    pub external_root_count_u32: u32,
    /// `8 + supplementary_artifact_count_u32`.
    pub complete_artifact_count_u32: u32,
    /// `supplementary_artifacts.len()`.
    pub supplementary_artifact_count_u32: u32,
    /// Total internal-to-internal dependency refs across `D`.
    pub internal_to_internal_dependency_edge_count_u32: u32,
    /// Total internal-to-external dependency refs across `D`.
    pub internal_to_external_dependency_edge_count_u32: u32,
    /// Total outgoing refs from `X` (`2`).
    pub external_to_internal_dependency_edge_count_u32: u32,
    /// Sum of `i2i + i2e + e2i`.
    pub complete_dependency_edge_count_u32: u32,
    /// Checked sum of `E[0..7]` and `S[0..|S|]` preimage lengths.
    pub internal_canonical_preimage_bytes_u64: u64,
    /// Exact byte length of outer `genesis_package` (`Length(G)`).
    pub external_root_canonical_preimage_bytes_u64: u64,
    /// `internal_canonical_preimage_bytes_u64 + external_root_canonical_preimage_bytes_u64`.
    pub total_canonical_preimage_bytes_u64: u64,
    /// Sole external genesis dependency node `X`.
    pub external_genesis_node: LaunchClosureExternalGenesisDependencyNodeV1,
    /// Exact declared byte lengths of the 7 explicit preimages placed before variable slices.
    pub explicit_preimage_lengths_u64: [u64; 7],
    /// Explicit preimage `E[0]`: canonical `ProfileScopeV1` bytes.
    pub profile_scope_bytes: Vec<u8>,
    /// Explicit preimage `E[1]`: canonical `DomainRegistryV1` bytes.
    pub domain_registry_bytes: Vec<u8>,
    /// Explicit preimage `E[2]`: canonical `BoundProfileManifestV1` bytes.
    pub candidate_profile_bytes: Vec<u8>,
    /// Explicit preimage `E[3]`: canonical `ResolvedProfileTransitionSpecV1` bytes.
    pub transition_spec_bytes: Vec<u8>,
    /// Explicit preimage `E[4]`: canonical `ProfileAdmissionManifestV1` bytes.
    pub profile_admission_bytes: Vec<u8>,
    /// Explicit preimage `E[5]`: canonical `GenesisRootParityRecordV1` bytes.
    pub genesis_root_parity_bytes: Vec<u8>,
    /// Explicit preimage `E[6]`: canonical `AdmittedReleaseRecordV1` bytes.
    pub admitted_release_bytes: Vec<u8>,
    /// Strictly sorted & unique internal DAG nodes `D`.
    pub internal_dependency_nodes: Vec<LaunchClosureInternalDependencyNodeV1>,
    /// Strictly sorted & unique embedded supplementary artifacts `S`.
    pub supplementary_artifacts: Vec<SupplementaryLaunchArtifactV1>,
}

/// Zero-allocation streaming preflight over raw [`LaunchClosureBundleV1`] bytes.
///
/// Enforces every count, length, checked byte sum, framing, cap, carrier-mode, and structural
/// ordering equation BEFORE any `Vec` allocation, copying, hashing, or nested decode.
pub fn preflight_launch_closure_bundle_bytes(
    raw: &[u8],
    outer_genesis_len: u64,
    caps: LaunchPreflightCapsV1,
) -> Result<PreflightBundleSummaryV1, ChainLaunch1Error> {
    if (raw.len() as u64) > caps.max_bundle_frame_bytes {
        return Err(ChainLaunch1Error::PreflightRejected(
            "bundle frame exceeds max_bundle_frame_bytes",
        ));
    }
    let mut cur = 0usize;
    let schema_version = scan_u16(raw, &mut cur)?;
    if schema_version != CHAIN_LAUNCH_1_VERSION {
        return Err(ChainLaunch1Error::PreflightRejected(
            "unsupported LaunchClosureBundleV1 schema_version",
        ));
    }
    let internal_count = scan_u32(raw, &mut cur)?;
    let external_count = scan_u32(raw, &mut cur)?;
    let complete_count = scan_u32(raw, &mut cur)?;
    let supp_count = scan_u32(raw, &mut cur)?;
    let i2i_edges = scan_u32(raw, &mut cur)?;
    let i2e_edges = scan_u32(raw, &mut cur)?;
    let e2i_edges = scan_u32(raw, &mut cur)?;
    let complete_edges = scan_u32(raw, &mut cur)?;
    let internal_bytes = scan_u64(raw, &mut cur)?;
    let external_bytes = scan_u64(raw, &mut cur)?;
    let total_bytes = scan_u64(raw, &mut cur)?;

    if external_count != EXTERNAL_GENESIS_ROOT_COUNT {
        return Err(ChainLaunch1Error::EquationViolation(
            "external_root_count_u32 must equal 1",
        ));
    }
    if supp_count > caps.max_supplementary_artifacts {
        return Err(ChainLaunch1Error::PreflightRejected(
            "supplementary_artifact_count_u32 exceeds cap",
        ));
    }
    if EXPLICIT_LAUNCH_ARTIFACT_COUNT.checked_add(supp_count) != Some(internal_count) {
        return Err(ChainLaunch1Error::EquationViolation(
            "internal_artifact_count_u32 != 7 + supplementary_artifact_count_u32",
        ));
    }
    if internal_count.checked_add(external_count) != Some(complete_count) {
        return Err(ChainLaunch1Error::EquationViolation(
            "complete_artifact_count_u32 != internal_artifact_count_u32 + external_root_count_u32",
        ));
    }
    if e2i_edges != EXTERNAL_GENESIS_OUTGOING_REF_COUNT {
        return Err(ChainLaunch1Error::EquationViolation(
            "external_to_internal_dependency_edge_count_u32 must equal 2",
        ));
    }
    let expected_edges = i2i_edges
        .checked_add(i2e_edges)
        .and_then(|s| s.checked_add(e2i_edges))
        .ok_or(ChainLaunch1Error::EquationViolation(
            "dependency edge total overflow",
        ))?;
    if expected_edges != complete_edges {
        return Err(ChainLaunch1Error::EquationViolation(
            "complete_dependency_edge_count_u32 != i2i + i2e + e2i",
        ));
    }
    if complete_edges > caps.max_total_dependency_edges {
        return Err(ChainLaunch1Error::PreflightRejected(
            "complete_dependency_edge_count_u32 exceeds cap",
        ));
    }
    if external_bytes != outer_genesis_len {
        return Err(ChainLaunch1Error::EquationViolation(
            "external_root_canonical_preimage_bytes_u64 != Length(G)",
        ));
    }
    let expected_total_bytes =
        internal_bytes
            .checked_add(external_bytes)
            .ok_or(ChainLaunch1Error::EquationViolation(
                "total_canonical_preimage_bytes_u64 checked sum overflow",
            ))?;
    if expected_total_bytes != total_bytes {
        return Err(ChainLaunch1Error::EquationViolation(
            "total_canonical_preimage_bytes_u64 != internal + external",
        ));
    }
    if total_bytes > caps.max_total_canonical_preimage_bytes {
        return Err(ChainLaunch1Error::PreflightRejected(
            "total_canonical_preimage_bytes_u64 exceeds cap",
        ));
    }

    // Scan LaunchClosureExternalGenesisDependencyNodeV1 (zero allocation).
    let ext_schema_len = usize::from(scan_u16(raw, &mut cur)?);
    let ext_schema = scan_slice(raw, &mut cur, ext_schema_len)?;
    if ext_schema != GENESIS_D1_PACKAGE_SCHEMA {
        return Err(ChainLaunch1Error::EquationViolation(
            "external_root_schema_id must equal GenesisPackageV1 schema",
        ));
    }
    let external_root_rank = scan_u32(raw, &mut cur)?;
    if external_root_rank == 0 {
        return Err(ChainLaunch1Error::DagViolation(
            "external_root_topological_rank_u32 cannot be 0",
        ));
    }
    let genesis_id = scan_arr32(raw, &mut cur)?;
    let ext_node_len = scan_u64(raw, &mut cur)?;
    if ext_node_len != outer_genesis_len {
        return Err(ChainLaunch1Error::EquationViolation(
            "external genesis node canonical_outer_genesis_package_length_u64 != Length(G)",
        ));
    }
    let ext_out_count = u32::from(scan_u16(raw, &mut cur)?);
    if ext_out_count != EXTERNAL_GENESIS_OUTGOING_REF_COUNT {
        return Err(ChainLaunch1Error::EquationViolation(
            "Length(GenesisInternalRefs(X)) must equal 2",
        ));
    }
    let mut prev_ext_target: Option<[u8; 32]> = None;
    let mut seen_out_mask = 0u8;
    for _ in 0..ext_out_count {
        let tid = scan_arr32(raw, &mut cur)?;
        if let Some(prev) = prev_ext_target
            && prev >= tid
        {
            return Err(ChainLaunch1Error::PreflightRejected(
                "external genesis outgoing refs must be strictly sorted and unique",
            ));
        }
        prev_ext_target = Some(tid);
        let cls = ExternalGenesisOutgoingDependencyClassV1::from_code(scan_u8(raw, &mut cur)?)?;
        let bit = 1u8 << cls.code();
        if (seen_out_mask & bit) != 0 {
            return Err(ChainLaunch1Error::DagViolation(
                "duplicate external genesis outgoing dependency class",
            ));
        }
        seen_out_mask |= bit;
    }

    // Scan explicit_preimage_lengths_u64[7] placed BEFORE variable slices.
    let mut explicit_lens = [0u64; 7];
    let mut explicit_sum = 0u64;
    for slot in &mut explicit_lens {
        let len = scan_u64(raw, &mut cur)?;
        if len == 0 || len > caps.max_single_preimage_bytes {
            return Err(ChainLaunch1Error::PreflightRejected(
                "explicit preimage length is zero or exceeds cap",
            ));
        }
        explicit_sum =
            explicit_sum
                .checked_add(len)
                .ok_or(ChainLaunch1Error::EquationViolation(
                    "explicit preimage length sum overflow",
                ))?;
        *slot = len;
    }
    for &len in &explicit_lens {
        let mode = scan_u8(raw, &mut cur)?;
        if mode != EMBEDDED_PREIMAGE_CARRIER_MODE_V1 {
            return Err(ChainLaunch1Error::PreflightRejected(
                "non-embedded or locator-only explicit preimage variant rejected",
            ));
        }
        let n = usize::try_from(len)
            .map_err(|_| ChainLaunch1Error::PreflightRejected("explicit len usize overflow"))?;
        let _ = scan_slice(raw, &mut cur, n)?;
    }

    // Scan internal dependency nodes D (zero allocation).
    let node_count = scan_u32(raw, &mut cur)?;
    if node_count != internal_count {
        return Err(ChainLaunch1Error::EquationViolation(
            "Length(D) != internal_artifact_count_u32",
        ));
    }
    let mut seen_i2i = 0u32;
    let mut seen_i2e = 0u32;
    let mut prev_node_id: Option<[u8; 32]> = None;
    for _ in 0..node_count {
        let _rank = scan_u32(raw, &mut cur)?;
        let nid = scan_arr32(raw, &mut cur)?;
        if let Some(prev) = prev_node_id
            && prev >= nid
        {
            return Err(ChainLaunch1Error::PreflightRejected(
                "internal dependency nodes must be strictly sorted and unique by ID",
            ));
        }
        prev_node_id = Some(nid);
        let rc = scan_u16(raw, &mut cur)?;
        let mut prev_ref: Option<(u8, [u8; 32], u8)> = None;
        for _ in 0..rc {
            let tag = scan_u8(raw, &mut cur)?;
            let key = match tag {
                1 => {
                    let target = scan_arr32(raw, &mut cur)?;
                    seen_i2i =
                        seen_i2i
                            .checked_add(1)
                            .ok_or(ChainLaunch1Error::EquationViolation(
                                "i2i edge counter overflow",
                            ))?;
                    (1u8, target, 0u8)
                }
                2 => {
                    let gid = scan_arr32(raw, &mut cur)?;
                    let cls = ExternalGenesisIncomingDependencyClassV1::from_code(scan_u8(
                        raw, &mut cur,
                    )?)?;
                    seen_i2e =
                        seen_i2e
                            .checked_add(1)
                            .ok_or(ChainLaunch1Error::EquationViolation(
                                "i2e edge counter overflow",
                            ))?;
                    (2u8, gid, cls.code())
                }
                _ => {
                    return Err(ChainLaunch1Error::PreflightRejected(
                        "unknown LaunchClosureDependencyRefV1 variant tag",
                    ));
                }
            };
            if let Some(prev) = prev_ref
                && prev >= key
            {
                return Err(ChainLaunch1Error::PreflightRejected(
                    "dependency_refs must be strictly sorted and unique",
                ));
            }
            prev_ref = Some(key);
        }
    }
    if seen_i2i != i2i_edges {
        return Err(ChainLaunch1Error::EquationViolation(
            "internal_to_internal_dependency_edge_count_u32 != Length(InternalRefs(D))",
        ));
    }
    if seen_i2e != i2e_edges {
        return Err(ChainLaunch1Error::EquationViolation(
            "internal_to_external_dependency_edge_count_u32 != Length(ExternalGenesisRefs(D))",
        ));
    }

    // Scan supplementary artifacts S (zero allocation).
    let wire_supp_count = scan_u32(raw, &mut cur)?;
    if wire_supp_count != supp_count {
        return Err(ChainLaunch1Error::EquationViolation(
            "supplementary_artifact_count_u32 != Length(S)",
        ));
    }
    let mut supp_sum = 0u64;
    let mut prev_supp_id: Option<[u8; 32]> = None;
    for _ in 0..wire_supp_count {
        let mode = scan_u8(raw, &mut cur)?;
        if mode != EMBEDDED_PREIMAGE_CARRIER_MODE_V1 {
            return Err(ChainLaunch1Error::PreflightRejected(
                "non-embedded or locator-only supplementary artifact variant rejected",
            ));
        }
        let slen = usize::from(scan_u16(raw, &mut cur)?);
        if slen == 0 || slen > 128 {
            return Err(ChainLaunch1Error::PreflightRejected(
                "invalid supplementary schema ID length",
            ));
        }
        let _ = scan_slice(raw, &mut cur, slen)?;
        let sid = scan_arr32(raw, &mut cur)?;
        if let Some(prev) = prev_supp_id
            && prev >= sid
        {
            return Err(ChainLaunch1Error::PreflightRejected(
                "supplementary artifacts must be strictly sorted and unique by ID",
            ));
        }
        prev_supp_id = Some(sid);
        let plen = scan_u64(raw, &mut cur)?;
        if plen == 0 || plen > caps.max_single_preimage_bytes {
            return Err(ChainLaunch1Error::PreflightRejected(
                "supplementary preimage length is zero or exceeds cap",
            ));
        }
        supp_sum = supp_sum
            .checked_add(plen)
            .ok_or(ChainLaunch1Error::EquationViolation(
                "supplementary preimage length sum overflow",
            ))?;
        let n = usize::try_from(plen)
            .map_err(|_| ChainLaunch1Error::PreflightRejected("supp len usize overflow"))?;
        let _ = scan_slice(raw, &mut cur, n)?;
    }

    let computed_internal =
        explicit_sum
            .checked_add(supp_sum)
            .ok_or(ChainLaunch1Error::EquationViolation(
                "internal_canonical_preimage_bytes_u64 overflow",
            ))?;
    if computed_internal != internal_bytes {
        return Err(ChainLaunch1Error::EquationViolation(
            "internal_canonical_preimage_bytes_u64 != Sum(E) + Sum(S)",
        ));
    }
    if cur != raw.len() {
        return Err(ChainLaunch1Error::PreflightRejected(
            "trailing bytes after LaunchClosureBundleV1",
        ));
    }

    Ok(PreflightBundleSummaryV1 {
        internal_artifact_count_u32: internal_count,
        external_root_count_u32: external_count,
        complete_artifact_count_u32: complete_count,
        supplementary_artifact_count_u32: supp_count,
        i2i_edges,
        i2e_edges,
        e2i_edges,
        complete_edges,
        internal_preimage_bytes: internal_bytes,
        external_preimage_bytes: external_bytes,
        total_preimage_bytes: total_bytes,
        external_root_rank,
        genesis_id,
        explicit_preimage_lengths_u64: explicit_lens,
    })
}

impl LaunchClosureBundleV1 {
    /// Encodes the bundle into exact canonical big-endian wire bytes (with fixed-width header &
    /// explicit lengths placed before variable slices).
    pub fn encode_canonical(&self) -> Result<Vec<u8>, ChainLaunch1Error> {
        let mut out = Vec::with_capacity(4096);
        out.extend_from_slice(&self.schema_version.to_be_bytes());
        out.extend_from_slice(&self.internal_artifact_count_u32.to_be_bytes());
        out.extend_from_slice(&self.external_root_count_u32.to_be_bytes());
        out.extend_from_slice(&self.complete_artifact_count_u32.to_be_bytes());
        out.extend_from_slice(&self.supplementary_artifact_count_u32.to_be_bytes());
        out.extend_from_slice(
            &self
                .internal_to_internal_dependency_edge_count_u32
                .to_be_bytes(),
        );
        out.extend_from_slice(
            &self
                .internal_to_external_dependency_edge_count_u32
                .to_be_bytes(),
        );
        out.extend_from_slice(
            &self
                .external_to_internal_dependency_edge_count_u32
                .to_be_bytes(),
        );
        out.extend_from_slice(&self.complete_dependency_edge_count_u32.to_be_bytes());
        out.extend_from_slice(&self.internal_canonical_preimage_bytes_u64.to_be_bytes());
        out.extend_from_slice(
            &self
                .external_root_canonical_preimage_bytes_u64
                .to_be_bytes(),
        );
        out.extend_from_slice(&self.total_canonical_preimage_bytes_u64.to_be_bytes());

        // External genesis dependency node X
        let x = &self.external_genesis_node;
        let x_slen = u16::try_from(x.external_root_schema_id.len())
            .map_err(|_| ChainLaunch1Error::PreflightRejected("external schema len overflow"))?;
        out.extend_from_slice(&x_slen.to_be_bytes());
        out.extend_from_slice(&x.external_root_schema_id);
        out.extend_from_slice(&x.external_root_topological_rank_u32.to_be_bytes());
        out.extend_from_slice(&x.genesis_id);
        out.extend_from_slice(&x.canonical_outer_genesis_package_length_u64.to_be_bytes());
        let x_rc = u16::try_from(x.outgoing_refs.len())
            .map_err(|_| ChainLaunch1Error::PreflightRejected("external ref count overflow"))?;
        out.extend_from_slice(&x_rc.to_be_bytes());
        for r in &x.outgoing_refs {
            out.extend_from_slice(&r.typed_internal_artifact_id);
            out.push(r.outgoing_class.code());
        }

        // explicit_preimage_lengths_u64[7]
        for &l in &self.explicit_preimage_lengths_u64 {
            out.extend_from_slice(&l.to_be_bytes());
        }
        // 7 explicit preimages
        let explicit_slices: [&[u8]; 7] = [
            &self.profile_scope_bytes,
            &self.domain_registry_bytes,
            &self.candidate_profile_bytes,
            &self.transition_spec_bytes,
            &self.profile_admission_bytes,
            &self.genesis_root_parity_bytes,
            &self.admitted_release_bytes,
        ];
        for (i, slice) in explicit_slices.iter().enumerate() {
            if (slice.len() as u64) != self.explicit_preimage_lengths_u64[i] {
                return Err(ChainLaunch1Error::EquationViolation(
                    "explicit_preimage_lengths_u64[i] != Length(E[i])",
                ));
            }
            out.push(EMBEDDED_PREIMAGE_CARRIER_MODE_V1);
            out.extend_from_slice(slice);
        }

        // Internal dependency nodes D
        let n_nodes = u32::try_from(self.internal_dependency_nodes.len())
            .map_err(|_| ChainLaunch1Error::PreflightRejected("node count overflow"))?;
        out.extend_from_slice(&n_nodes.to_be_bytes());
        for node in &self.internal_dependency_nodes {
            out.extend_from_slice(&node.topological_rank_u32.to_be_bytes());
            out.extend_from_slice(&node.typed_artifact_id);
            let n_refs = u16::try_from(node.dependency_refs.len())
                .map_err(|_| ChainLaunch1Error::PreflightRejected("ref count overflow"))?;
            out.extend_from_slice(&n_refs.to_be_bytes());
            for r in &node.dependency_refs {
                match *r {
                    LaunchClosureDependencyRefV1::InternalLaunchArtifactV1(id) => {
                        out.push(1);
                        out.extend_from_slice(&id);
                    }
                    LaunchClosureDependencyRefV1::ExternalGenesisRootV1 {
                        genesis_id,
                        edge_class,
                    } => {
                        out.push(2);
                        out.extend_from_slice(&genesis_id);
                        out.push(edge_class.code());
                    }
                }
            }
        }

        // Supplementary artifacts S
        let n_supp = u32::try_from(self.supplementary_artifacts.len())
            .map_err(|_| ChainLaunch1Error::PreflightRejected("supp count overflow"))?;
        out.extend_from_slice(&n_supp.to_be_bytes());
        for s in &self.supplementary_artifacts {
            if (s.canonical_artifact_preimage_bytes.len() as u64)
                != s.canonical_artifact_preimage_length_u64
            {
                return Err(ChainLaunch1Error::EquationViolation(
                    "s.canonical_artifact_preimage_length_u64 != Length(s.bytes)",
                ));
            }
            out.push(EMBEDDED_PREIMAGE_CARRIER_MODE_V1);
            let slen = u16::try_from(s.typed_artifact_schema_id.len())
                .map_err(|_| ChainLaunch1Error::PreflightRejected("supp schema len overflow"))?;
            out.extend_from_slice(&slen.to_be_bytes());
            out.extend_from_slice(&s.typed_artifact_schema_id);
            out.extend_from_slice(&s.typed_artifact_id);
            out.extend_from_slice(&s.canonical_artifact_preimage_length_u64.to_be_bytes());
            out.extend_from_slice(&s.canonical_artifact_preimage_bytes);
        }
        Ok(out)
    }

    /// Runs zero-allocation preflight first, then materializes [`LaunchClosureBundleV1`].
    pub fn decode_canonical_with_preflight(
        raw: &[u8],
        outer_genesis_len: u64,
        caps: LaunchPreflightCapsV1,
    ) -> Result<(PreflightBundleSummaryV1, Self), ChainLaunch1Error> {
        let summary = preflight_launch_closure_bundle_bytes(raw, outer_genesis_len, caps)?;
        let mut cur = 0usize;
        let schema_version = scan_u16(raw, &mut cur)?;
        let internal_artifact_count_u32 = scan_u32(raw, &mut cur)?;
        let external_root_count_u32 = scan_u32(raw, &mut cur)?;
        let complete_artifact_count_u32 = scan_u32(raw, &mut cur)?;
        let supplementary_artifact_count_u32 = scan_u32(raw, &mut cur)?;
        let i2i = scan_u32(raw, &mut cur)?;
        let i2e = scan_u32(raw, &mut cur)?;
        let e2i = scan_u32(raw, &mut cur)?;
        let complete_edges = scan_u32(raw, &mut cur)?;
        let internal_bytes = scan_u64(raw, &mut cur)?;
        let external_bytes = scan_u64(raw, &mut cur)?;
        let total_bytes = scan_u64(raw, &mut cur)?;

        let ext_slen = usize::from(scan_u16(raw, &mut cur)?);
        let external_root_schema_id = scan_slice(raw, &mut cur, ext_slen)?.to_vec();
        let external_root_topological_rank_u32 = scan_u32(raw, &mut cur)?;
        let genesis_id = scan_arr32(raw, &mut cur)?;
        let canonical_outer_genesis_package_length_u64 = scan_u64(raw, &mut cur)?;
        let ext_rc = usize::from(scan_u16(raw, &mut cur)?);
        let mut outgoing_refs = Vec::with_capacity(ext_rc);
        for _ in 0..ext_rc {
            let tid = scan_arr32(raw, &mut cur)?;
            let cls = ExternalGenesisOutgoingDependencyClassV1::from_code(scan_u8(raw, &mut cur)?)?;
            outgoing_refs.push(ExternalGenesisToInternalDependencyRefV1 {
                typed_internal_artifact_id: tid,
                outgoing_class: cls,
            });
        }
        let external_genesis_node = LaunchClosureExternalGenesisDependencyNodeV1 {
            external_root_schema_id,
            external_root_topological_rank_u32,
            genesis_id,
            canonical_outer_genesis_package_length_u64,
            outgoing_refs,
        };

        let mut explicit_preimage_lengths_u64 = [0u64; 7];
        for slot in &mut explicit_preimage_lengths_u64 {
            *slot = scan_u64(raw, &mut cur)?;
        }
        let mut explicit_vecs = Vec::with_capacity(7);
        for &l in &explicit_preimage_lengths_u64 {
            let _mode = scan_u8(raw, &mut cur)?;
            explicit_vecs.push(scan_slice(raw, &mut cur, l as usize)?.to_vec());
        }
        let mut ex = explicit_vecs.into_iter();
        let profile_scope_bytes = ex.next().expect("E0");
        let domain_registry_bytes = ex.next().expect("E1");
        let candidate_profile_bytes = ex.next().expect("E2");
        let transition_spec_bytes = ex.next().expect("E3");
        let profile_admission_bytes = ex.next().expect("E4");
        let genesis_root_parity_bytes = ex.next().expect("E5");
        let admitted_release_bytes = ex.next().expect("E6");

        let n_nodes = scan_u32(raw, &mut cur)? as usize;
        let mut internal_dependency_nodes = Vec::with_capacity(n_nodes);
        for _ in 0..n_nodes {
            let topological_rank_u32 = scan_u32(raw, &mut cur)?;
            let typed_artifact_id = scan_arr32(raw, &mut cur)?;
            let n_refs = usize::from(scan_u16(raw, &mut cur)?);
            let mut dependency_refs = Vec::with_capacity(n_refs);
            for _ in 0..n_refs {
                match scan_u8(raw, &mut cur)? {
                    1 => dependency_refs.push(
                        LaunchClosureDependencyRefV1::InternalLaunchArtifactV1(scan_arr32(
                            raw, &mut cur,
                        )?),
                    ),
                    2 => {
                        let gid = scan_arr32(raw, &mut cur)?;
                        let edge_class = ExternalGenesisIncomingDependencyClassV1::from_code(
                            scan_u8(raw, &mut cur)?,
                        )?;
                        dependency_refs.push(LaunchClosureDependencyRefV1::ExternalGenesisRootV1 {
                            genesis_id: gid,
                            edge_class,
                        });
                    }
                    _ => unreachable!("preflight already checked dep ref tag"),
                }
            }
            internal_dependency_nodes.push(LaunchClosureInternalDependencyNodeV1 {
                topological_rank_u32,
                typed_artifact_id,
                dependency_refs,
            });
        }

        let n_supp = scan_u32(raw, &mut cur)? as usize;
        let mut supplementary_artifacts = Vec::with_capacity(n_supp);
        for _ in 0..n_supp {
            let _mode = scan_u8(raw, &mut cur)?;
            let slen = usize::from(scan_u16(raw, &mut cur)?);
            let typed_artifact_schema_id = scan_slice(raw, &mut cur, slen)?.to_vec();
            let typed_artifact_id = scan_arr32(raw, &mut cur)?;
            let plen = scan_u64(raw, &mut cur)?;
            let canonical_artifact_preimage_bytes =
                scan_slice(raw, &mut cur, plen as usize)?.to_vec();
            supplementary_artifacts.push(SupplementaryLaunchArtifactV1 {
                typed_artifact_schema_id,
                typed_artifact_id,
                canonical_artifact_preimage_length_u64: plen,
                canonical_artifact_preimage_bytes,
            });
        }

        Ok((
            summary,
            Self {
                schema_version,
                internal_artifact_count_u32,
                external_root_count_u32,
                complete_artifact_count_u32,
                supplementary_artifact_count_u32,
                internal_to_internal_dependency_edge_count_u32: i2i,
                internal_to_external_dependency_edge_count_u32: i2e,
                external_to_internal_dependency_edge_count_u32: e2i,
                complete_dependency_edge_count_u32: complete_edges,
                internal_canonical_preimage_bytes_u64: internal_bytes,
                external_root_canonical_preimage_bytes_u64: external_bytes,
                total_canonical_preimage_bytes_u64: total_bytes,
                external_genesis_node,
                explicit_preimage_lengths_u64,
                profile_scope_bytes,
                domain_registry_bytes,
                candidate_profile_bytes,
                transition_spec_bytes,
                profile_admission_bytes,
                genesis_root_parity_bytes,
                admitted_release_bytes,
                internal_dependency_nodes,
                supplementary_artifacts,
            },
        ))
    }

    /// Computes the canonical `Receipt` `ContentTag` whose digest IS `launch_closure_bundle_id`.
    pub fn tag(&self) -> Result<ContentTag, ChainLaunch1Error> {
        let enc = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            CHAIN_LAUNCH_1_BUNDLE_SCHEMA,
            enc.len() as u64,
            |w| w(&enc),
        )
        .expect("static schema and bounded length"))
    }
}

/// Immutable network launch trust anchor (`bitblock/chain-launch-manifest/v1`).
///
/// Strictly excludes its own derived `chain_launch_manifest_id` and all future block identifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChainLaunchManifestV1 {
    /// Schema version (`1`).
    pub schema_version: u16,
    /// Immutable launch `genesis_id`.
    pub genesis_id: [u8; 32],
    /// Initial admitted `profile_id`.
    pub initial_profile_id: [u8; 32],
    /// Initial `transition_spec_at_profile_id`.
    pub initial_transition_spec_at_profile_id: [u8; 32],
    /// Initial `PROFILE-1` `profile_admission_manifest_id`.
    pub initial_profile_admission_manifest_id: [u8; 32],
    /// Initial `GENESIS-ROOT-PARITY-1` `genesis_root_parity_record_id`.
    pub genesis_root_parity_record_id: [u8; 32],
    /// Typed initial `ExecutionRootRefV1` carrying initial admitted profile context.
    pub initial_execution_root_ref: ExecutionRootRefV1,
    /// Initial `RELEASE-1` `admitted_release_record_id`.
    pub initial_release_record_id: [u8; 32],
    /// Recomputed `launch_closure_bundle_id`.
    pub launch_closure_bundle_id: [u8; 32],
    /// Byte-exact projection of the genesis-committed launch authority policy (`G_0`).
    pub launch_authority_policy_id: [u8; 32],
    /// Immutable genesis parent block anchor derived from `genesis_id`.
    pub genesis_parent_block_id: [u8; 32],
    /// Exact `genesis_history_root` matching `GenesisRootParityRecordV1`.
    pub genesis_history_root: [u8; 32],
    /// Height of the first post-genesis ledger block (`1`).
    pub first_ledger_height: u64,
}

impl ChainLaunchManifestV1 {
    /// Fixed canonical big-endian wire byte length (`2 + 32*10 + ExecutionRootRefV1::WIRE_LEN + 8`).
    pub const WIRE_LEN: usize = 2 + 32 * 10 + ExecutionRootRefV1::WIRE_LEN + 8;

    /// Computes the deterministic `genesis_parent_block_id` derived from `genesis_id`.
    pub fn derive_genesis_parent_block_id(genesis_id: [u8; 32]) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(CHL1_GENESIS_PARENT_PREFIX);
        h.update(genesis_id);
        h.finalize().into()
    }

    /// Encodes into exact fixed-width canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], ChainLaunch1Error> {
        if self.schema_version != CHAIN_LAUNCH_1_VERSION {
            return Err(ChainLaunch1Error::PreflightRejected(
                "ChainLaunchManifestV1 schema_version must be 1",
            ));
        }
        if self.first_ledger_height != 1 {
            return Err(ChainLaunch1Error::BindingMismatch(
                "first_ledger_height must equal 1",
            ));
        }
        let mut out = [0u8; Self::WIRE_LEN];
        let mut c = 0usize;
        out[c..c + 2].copy_from_slice(&self.schema_version.to_be_bytes());
        c += 2;
        for f in [
            self.genesis_id,
            self.initial_profile_id,
            self.initial_transition_spec_at_profile_id,
            self.initial_profile_admission_manifest_id,
            self.genesis_root_parity_record_id,
        ] {
            out[c..c + 32].copy_from_slice(&f);
            c += 32;
        }
        let ref_b = self.initial_execution_root_ref.encode_canonical();
        out[c..c + ExecutionRootRefV1::WIRE_LEN].copy_from_slice(&ref_b);
        c += ExecutionRootRefV1::WIRE_LEN;
        for f in [
            self.initial_release_record_id,
            self.launch_closure_bundle_id,
            self.launch_authority_policy_id,
            self.genesis_parent_block_id,
            self.genesis_history_root,
        ] {
            out[c..c + 32].copy_from_slice(&f);
            c += 32;
        }
        out[c..c + 8].copy_from_slice(&self.first_ledger_height.to_be_bytes());
        c += 8;
        debug_assert_eq!(c, Self::WIRE_LEN);
        Ok(out)
    }

    /// Decodes exact fixed-width canonical big-endian bytes into [`ChainLaunchManifestV1`].
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, ChainLaunch1Error> {
        if raw.len() != Self::WIRE_LEN {
            return Err(ChainLaunch1Error::PreflightRejected(
                "bad ChainLaunchManifestV1 wire length",
            ));
        }
        let mut c = 0usize;
        let schema_version = scan_u16(raw, &mut c)?;
        if schema_version != CHAIN_LAUNCH_1_VERSION {
            return Err(ChainLaunch1Error::PreflightRejected(
                "bad ChainLaunchManifestV1 schema_version",
            ));
        }
        let genesis_id = scan_arr32(raw, &mut c)?;
        let initial_profile_id = scan_arr32(raw, &mut c)?;
        let initial_transition_spec_at_profile_id = scan_arr32(raw, &mut c)?;
        let initial_profile_admission_manifest_id = scan_arr32(raw, &mut c)?;
        let genesis_root_parity_record_id = scan_arr32(raw, &mut c)?;
        let ref_slice = scan_slice(raw, &mut c, ExecutionRootRefV1::WIRE_LEN)?;
        let initial_execution_root_ref = ExecutionRootRefV1::decode_canonical(ref_slice)
            .map_err(|_| ChainLaunch1Error::BindingMismatch("bad ExecutionRootRefV1"))?;
        let initial_release_record_id = scan_arr32(raw, &mut c)?;
        let launch_closure_bundle_id = scan_arr32(raw, &mut c)?;
        let launch_authority_policy_id = scan_arr32(raw, &mut c)?;
        let genesis_parent_block_id = scan_arr32(raw, &mut c)?;
        let genesis_history_root = scan_arr32(raw, &mut c)?;
        let first_ledger_height = scan_u64(raw, &mut c)?;
        if first_ledger_height != 1 {
            return Err(ChainLaunch1Error::BindingMismatch(
                "first_ledger_height must equal 1",
            ));
        }
        Ok(Self {
            schema_version,
            genesis_id,
            initial_profile_id,
            initial_transition_spec_at_profile_id,
            initial_profile_admission_manifest_id,
            genesis_root_parity_record_id,
            initial_execution_root_ref,
            initial_release_record_id,
            launch_closure_bundle_id,
            launch_authority_policy_id,
            genesis_parent_block_id,
            genesis_history_root,
            first_ledger_height,
        })
    }

    /// Computes the canonical `Policy` `ContentTag` whose digest IS `chain_launch_manifest_id`.
    pub fn tag(&self) -> Result<ContentTag, ChainLaunch1Error> {
        let enc = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            CHAIN_LAUNCH_1_MANIFEST_SCHEMA,
            enc.len() as u64,
            |w| w(&enc),
        )
        .expect("static schema and fixed length"))
    }
}

/// Genesis-committed `3-of-5` Ed25519 launch authority policy (`G_0`).
///
/// Deterministic devnet seeds (`51..=55`) are explicitly labeled
/// `DEVNET-TEST-KEYS-NOT-PRODUCTION-CUSTODY`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrustedLaunchAuthorityPolicyV1 {
    /// Required signature threshold (`3`).
    pub threshold_m: u8,
    /// Strictly ascending unique authorized Ed25519 public keys (`5`).
    pub authorized_pubkeys: [CanonicalEd25519PublicKeyV1; 5],
}

impl TrustedLaunchAuthorityPolicyV1 {
    /// Constructs the deterministic `3-of-5` devnet test launch-authority policy (`51..=55`).
    ///
    /// **DEVNET-TEST-KEYS-NOT-PRODUCTION-CUSTODY**: for deterministic CI and clean-room Python
    /// vector parity only.
    pub fn devnet_three_of_five() -> (Self, [(CanonicalEd25519PublicKeyV1, [u8; 32]); 5]) {
        let mut pairs: Vec<(CanonicalEd25519PublicKeyV1, [u8; 32])> = [51u8, 52, 53, 54, 55]
            .into_iter()
            .map(|b| {
                let seed = [b; 32];
                let pk = CanonicalEd25519PublicKeyV1::from_secret_seed(&seed)
                    .expect("deterministic devnet launch seed valid");
                (pk, seed)
            })
            .collect();
        pairs.sort_by_key(|(pk, _)| *pk.as_bytes());
        let pks: [CanonicalEd25519PublicKeyV1; 5] =
            [pairs[0].0, pairs[1].0, pairs[2].0, pairs[3].0, pairs[4].0];
        let arr: [(CanonicalEd25519PublicKeyV1, [u8; 32]); 5] =
            [pairs[0], pairs[1], pairs[2], pairs[3], pairs[4]];
        (
            Self {
                threshold_m: 3,
                authorized_pubkeys: pks,
            },
            arr,
        )
    }

    /// Validates key ordering/uniqueness and computes the canonical `launch_authority_policy_id`.
    pub fn policy_id(&self) -> Result<[u8; 32], ChainLaunch1Error> {
        if self.threshold_m != 3 {
            return Err(ChainLaunch1Error::InvalidLaunchAuthorization(
                "launch authority policy requires threshold_m == 3",
            ));
        }
        for i in 1..5 {
            if self.authorized_pubkeys[i - 1].as_bytes() >= self.authorized_pubkeys[i].as_bytes() {
                return Err(ChainLaunch1Error::InvalidLaunchAuthorization(
                    "launch authority pubkeys must be strictly ascending and unique",
                ));
            }
        }
        let mut h = Sha256::new();
        h.update(CHL1_POLICY_PREFIX);
        h.update([self.threshold_m, 5u8]);
        for pk in &self.authorized_pubkeys {
            h.update(pk.as_bytes());
        }
        Ok(h.finalize().into())
    }
}

/// One authorized launch-signer Ed25519 signature inside [`ChainLaunchAuthorizationV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LaunchSignerSignatureV1 {
    /// Zero-based index into `TrustedLaunchAuthorityPolicyV1::authorized_pubkeys`.
    pub signer_index: u8,
    /// Canonical Ed25519 public key matching `authorized_pubkeys[signer_index]`.
    pub public_key: CanonicalEd25519PublicKeyV1,
    /// Strict RFC 8032 Ed25519 signature over `chain_launch_manifest_id`.
    pub signature: CanonicalEd25519SignatureV1,
}

/// Detached genesis-committed `3-of-5` launch authorization (`bitblock/chain-launch-authorization/v1`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainLaunchAuthorizationV1 {
    /// Schema version (`1`).
    pub schema_version: u16,
    /// Recomputed `chain_launch_manifest_id` authorized by this envelope.
    pub chain_launch_manifest_id: [u8; 32],
    /// Genesis-committed `launch_authority_policy_id`.
    pub launch_authority_policy_id: [u8; 32],
    /// Signature suite (`Ed25519StrictRfc8032V1`).
    pub signature_suite: SignatureSuiteIdV1,
    /// Strictly ascending unique `signer_index` signatures (`>= 3`).
    pub signatures: Vec<LaunchSignerSignatureV1>,
}

impl ChainLaunchAuthorizationV1 {
    /// Computes the exact domain-separated payload digest signed under `GovernanceAction`.
    pub fn auth_payload_digest(
        chain_launch_manifest_id: [u8; 32],
        launch_authority_policy_id: [u8; 32],
        signature_suite: SignatureSuiteIdV1,
    ) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(CHAIN_LAUNCH_1_AUTH_SCHEMA);
        h.update((66u32).to_be_bytes());
        h.update(chain_launch_manifest_id);
        h.update(launch_authority_policy_id);
        h.update(signature_suite.code().to_be_bytes());
        h.finalize().into()
    }

    /// Builds the exact `CRYPTO-D1` [`SigningPreimageV1`] under `SigningDomainTagV1::GovernanceAction`.
    pub fn signing_preimage(
        chain_id: u64,
        genesis_id: [u8; 32],
        chain_launch_manifest_id: [u8; 32],
        initial_profile_id: [u8; 32],
        launch_authority_policy_id: [u8; 32],
        signature_suite: SignatureSuiteIdV1,
    ) -> SigningPreimageV1 {
        SigningPreimageV1 {
            protocol_version: CHAIN_LAUNCH_1_VERSION,
            chain_id,
            genesis_id,
            launch_manifest_id: chain_launch_manifest_id,
            profile_id: initial_profile_id,
            domain_tag: SigningDomainTagV1::GovernanceAction,
            signature_suite,
            hash_suite: HashSuiteIdV1::Sha256V1,
            payload_digest: Self::auth_payload_digest(
                chain_launch_manifest_id,
                launch_authority_policy_id,
                signature_suite,
            ),
        }
    }

    /// Encodes the detached authorization into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, ChainLaunch1Error> {
        if self.schema_version != CHAIN_LAUNCH_1_VERSION {
            return Err(ChainLaunch1Error::PreflightRejected(
                "ChainLaunchAuthorizationV1 schema_version must be 1",
            ));
        }
        let n = u16::try_from(self.signatures.len())
            .map_err(|_| ChainLaunch1Error::PreflightRejected("too many launch signatures"))?;
        let mut out = Vec::with_capacity(2 + 32 + 32 + 2 + 2 + self.signatures.len() * 97);
        out.extend_from_slice(&self.schema_version.to_be_bytes());
        out.extend_from_slice(&self.chain_launch_manifest_id);
        out.extend_from_slice(&self.launch_authority_policy_id);
        out.extend_from_slice(&self.signature_suite.code().to_be_bytes());
        out.extend_from_slice(&n.to_be_bytes());
        for s in &self.signatures {
            out.push(s.signer_index);
            out.extend_from_slice(s.public_key.as_bytes());
            out.extend_from_slice(s.signature.as_bytes());
        }
        Ok(out)
    }

    /// Strictly decodes canonical big-endian wire bytes into [`ChainLaunchAuthorizationV1`].
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, ChainLaunch1Error> {
        let mut c = 0usize;
        let schema_version = scan_u16(raw, &mut c)?;
        if schema_version != CHAIN_LAUNCH_1_VERSION {
            return Err(ChainLaunch1Error::PreflightRejected(
                "bad ChainLaunchAuthorizationV1 schema_version",
            ));
        }
        let chain_launch_manifest_id = scan_arr32(raw, &mut c)?;
        let launch_authority_policy_id = scan_arr32(raw, &mut c)?;
        let signature_suite = SignatureSuiteIdV1::from_code(scan_u16(raw, &mut c)?)
            .map_err(|_| ChainLaunch1Error::InvalidLaunchAuthorization("bad signature suite"))?;
        let n = usize::from(scan_u16(raw, &mut c)?);
        if raw.len() != c + n * 97 {
            return Err(ChainLaunch1Error::PreflightRejected(
                "ChainLaunchAuthorizationV1 signature byte length mismatch",
            ));
        }
        let mut signatures = Vec::with_capacity(n.min(16));
        for _ in 0..n {
            let signer_index = scan_u8(raw, &mut c)?;
            let pk_b = scan_arr32(raw, &mut c)?;
            let sig_b: [u8; 64] = scan_slice(raw, &mut c, 64)?.try_into().expect("64");
            let public_key = CanonicalEd25519PublicKeyV1::from_bytes(pk_b)
                .map_err(|_| ChainLaunch1Error::InvalidLaunchAuthorization("bad launch pubkey"))?;
            let signature = CanonicalEd25519SignatureV1::from_bytes(sig_b)
                .map_err(|_| ChainLaunch1Error::InvalidLaunchAuthorization("bad launch sig"))?;
            signatures.push(LaunchSignerSignatureV1 {
                signer_index,
                public_key,
                signature,
            });
        }
        Ok(Self {
            schema_version,
            chain_launch_manifest_id,
            launch_authority_policy_id,
            signature_suite,
            signatures,
        })
    }

    /// Computes the canonical `Receipt` `ContentTag` for this detached launch authorization.
    pub fn tag(&self) -> Result<ContentTag, ChainLaunch1Error> {
        let enc = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            CHAIN_LAUNCH_1_AUTH_SCHEMA,
            enc.len() as u64,
            |w| w(&enc),
        )
        .expect("static schema and bounded length"))
    }
}

/// Complete outer launch package (`bitblock/chain-launch-1/package/v1`).
///
/// Carries `genesis_package` ONCE outside `LaunchClosureBundleV1` alongside its bound manifest,
/// identifiers, and detached genesis-committed launch authorization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainLaunchPackageV1 {
    /// Schema version (`1`).
    pub schema_version: u16,
    /// Exact outer canonical `GenesisPackageV1` bytes (`G`, carried once outside the bundle).
    pub genesis_package_bytes: Vec<u8>,
    /// Exact canonical [`LaunchClosureBundleV1`] bytes.
    pub launch_closure_bundle_bytes: Vec<u8>,
    /// Recomputed `launch_closure_bundle_id`.
    pub launch_closure_bundle_id: [u8; 32],
    /// Immutable [`ChainLaunchManifestV1`].
    pub manifest: ChainLaunchManifestV1,
    /// Recomputed `chain_launch_manifest_id`.
    pub chain_launch_manifest_id: [u8; 32],
    /// Detached genesis-committed `3-of-5` [`ChainLaunchAuthorizationV1`].
    pub authorization: ChainLaunchAuthorizationV1,
}

impl ChainLaunchPackageV1 {
    /// Encodes the complete package into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, ChainLaunch1Error> {
        if self.schema_version != CHAIN_LAUNCH_1_VERSION {
            return Err(ChainLaunch1Error::PreflightRejected(
                "ChainLaunchPackageV1 schema_version must be 1",
            ));
        }
        let manifest_b = self.manifest.encode_canonical()?;
        let auth_b = self.authorization.encode_canonical()?;
        let g_len = u64::try_from(self.genesis_package_bytes.len())
            .map_err(|_| ChainLaunch1Error::PreflightRejected("genesis len overflow"))?;
        let b_len = u64::try_from(self.launch_closure_bundle_bytes.len())
            .map_err(|_| ChainLaunch1Error::PreflightRejected("bundle len overflow"))?;
        let a_len = u32::try_from(auth_b.len())
            .map_err(|_| ChainLaunch1Error::PreflightRejected("auth len overflow"))?;

        let mut out = Vec::with_capacity(
            2 + 8
                + self.genesis_package_bytes.len()
                + 8
                + self.launch_closure_bundle_bytes.len()
                + 32
                + manifest_b.len()
                + 32
                + 4
                + auth_b.len(),
        );
        out.extend_from_slice(&self.schema_version.to_be_bytes());
        out.extend_from_slice(&g_len.to_be_bytes());
        out.extend_from_slice(&self.genesis_package_bytes);
        out.extend_from_slice(&b_len.to_be_bytes());
        out.extend_from_slice(&self.launch_closure_bundle_bytes);
        out.extend_from_slice(&self.launch_closure_bundle_id);
        out.extend_from_slice(&manifest_b);
        out.extend_from_slice(&self.chain_launch_manifest_id);
        out.extend_from_slice(&a_len.to_be_bytes());
        out.extend_from_slice(&auth_b);
        Ok(out)
    }

    /// Decodes a canonical [`ChainLaunchPackageV1`] after enforcing `caps.max_bundle_frame_bytes`.
    pub fn decode_canonical(
        raw: &[u8],
        caps: LaunchPreflightCapsV1,
    ) -> Result<Self, ChainLaunch1Error> {
        if (raw.len() as u64) > caps.max_bundle_frame_bytes.saturating_mul(2) {
            return Err(ChainLaunch1Error::PreflightRejected(
                "ChainLaunchPackageV1 exceeds outer cap",
            ));
        }
        let mut c = 0usize;
        let schema_version = scan_u16(raw, &mut c)?;
        if schema_version != CHAIN_LAUNCH_1_VERSION {
            return Err(ChainLaunch1Error::PreflightRejected(
                "bad ChainLaunchPackageV1 schema_version",
            ));
        }
        let g_len = scan_u64(raw, &mut c)?;
        if g_len == 0 || g_len > caps.max_single_preimage_bytes {
            return Err(ChainLaunch1Error::PreflightRejected(
                "outer genesis length zero or over cap",
            ));
        }
        let genesis_package_bytes = scan_slice(raw, &mut c, g_len as usize)?.to_vec();
        let b_len = scan_u64(raw, &mut c)?;
        if b_len == 0 || b_len > caps.max_bundle_frame_bytes {
            return Err(ChainLaunch1Error::PreflightRejected(
                "bundle length zero or over cap",
            ));
        }
        let launch_closure_bundle_bytes = scan_slice(raw, &mut c, b_len as usize)?.to_vec();
        let launch_closure_bundle_id = scan_arr32(raw, &mut c)?;
        let manifest_s = scan_slice(raw, &mut c, ChainLaunchManifestV1::WIRE_LEN)?;
        let manifest = ChainLaunchManifestV1::decode_canonical(manifest_s)?;
        let chain_launch_manifest_id = scan_arr32(raw, &mut c)?;
        let a_len = scan_u32(raw, &mut c)? as usize;
        let auth_s = scan_slice(raw, &mut c, a_len)?;
        let authorization = ChainLaunchAuthorizationV1::decode_canonical(auth_s)?;
        if c != raw.len() {
            return Err(ChainLaunch1Error::PreflightRejected(
                "trailing bytes after ChainLaunchPackageV1",
            ));
        }
        Ok(Self {
            schema_version,
            genesis_package_bytes,
            launch_closure_bundle_bytes,
            launch_closure_bundle_id,
            manifest,
            chain_launch_manifest_id,
            authorization,
        })
    }

    /// Computes the canonical `Receipt` `ContentTag` for the complete [`ChainLaunchPackageV1`].
    pub fn tag(&self) -> Result<ContentTag, ChainLaunch1Error> {
        let enc = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            CHAIN_LAUNCH_1_PACKAGE_SCHEMA,
            enc.len() as u64,
            |w| w(&enc),
        )
        .expect("static schema and bounded length"))
    }
}

/// Verified outcome returned by [`verify_chain_launch_package`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedChainLaunchOutcomeV1 {
    /// Zero-allocation preflight summary.
    pub preflight: PreflightBundleSummaryV1,
    /// Verified `launch_closure_bundle_id`.
    pub launch_closure_bundle_id: [u8; 32],
    /// Verified `chain_launch_manifest_id`.
    pub chain_launch_manifest_id: [u8; 32],
    /// Verified outer `genesis_id`.
    pub genesis_id: [u8; 32],
    /// Verified initial admitted `profile_id`.
    pub initial_profile_id: [u8; 32],
}

/// Verifies a complete [`ChainLaunchPackageV1`] end-to-end in exact `LAYER1_DESIGN_PAPER.md` order:
/// 1. Zero-allocation streaming preflight on `launch_closure_bundle_bytes`;
/// 2. Outer `GenesisPackageV1` bounded decode & verification (carried once, never duplicated);
/// 3. Recompute all 7 explicit preimages + supplementary preimages & byte-exact equality checks;
/// 4. Enforce combined DAG node bijection, strict rank monotonicity, external genesis outgoing
///    edges (`ProfileScopeV1` & `DomainRegistryV1`), permitted incoming edge classes, and zero
///    direct external edges from `ProfileScopeV1`, `DomainRegistryV1`, `TransitionSpecAtProfileV1`,
///    or `ProfileAdmissionManifestV1`;
/// 5. Verify `LaunchClosureBundleV1` ID, `ChainLaunchManifestV1` field bindings & non-self-reference;
/// 6. Verify detached `3-of-5` genesis-committed Ed25519 [`ChainLaunchAuthorizationV1`].
pub fn verify_chain_launch_package(
    pkg: &ChainLaunchPackageV1,
    genesis_release_policy: &TrustedReleaseAuthorityPolicyV1,
    genesis_launch_policy: &TrustedLaunchAuthorityPolicyV1,
    caps: LaunchPreflightCapsV1,
) -> Result<VerifiedChainLaunchOutcomeV1, ChainLaunch1Error> {
    let outer_genesis_len = pkg.genesis_package_bytes.len() as u64;
    let (preflight, bundle) = LaunchClosureBundleV1::decode_canonical_with_preflight(
        &pkg.launch_closure_bundle_bytes,
        outer_genesis_len,
        caps,
    )?;

    // Recompute launch_closure_bundle_id from raw bundle bytes.
    let bundle_id = hash_canonical(
        TagKind::Receipt,
        CHAIN_LAUNCH_1_BUNDLE_SCHEMA,
        pkg.launch_closure_bundle_bytes.len() as u64,
        |w| w(&pkg.launch_closure_bundle_bytes),
    )
    .expect("static schema")
    .digest();
    if bundle_id != pkg.launch_closure_bundle_id {
        return Err(ChainLaunch1Error::BindingMismatch(
            "launch_closure_bundle_id mismatch",
        ));
    }

    // 2. Outer GenesisPackageV1 decode & verify.
    let genesis_pkg = GenesisPackageV1::decode_canonical(&pkg.genesis_package_bytes)
        .map_err(|_| ChainLaunch1Error::BindingMismatch("outer GenesisPackageV1 decode failed"))?;
    genesis_pkg
        .verify()
        .map_err(|_| ChainLaunch1Error::BindingMismatch("outer GenesisPackageV1 verify failed"))?;
    if bundle.external_genesis_node.genesis_id != genesis_pkg.genesis_id {
        return Err(ChainLaunch1Error::BindingMismatch(
            "external genesis node genesis_id != outer GenesisPackageV1.genesis_id",
        ));
    }

    // 3. Decode & verify all 7 explicit preimages in order.
    let domain_reg = DomainRegistryV1::decode_canonical(&bundle.domain_registry_bytes)
        .map_err(|_| ChainLaunch1Error::BindingMismatch("E1 DomainRegistryV1 decode failed"))?;
    let domain_reg_id = domain_reg.tag().digest();

    let scope = ProfileScopeV1::decode_canonical(&bundle.profile_scope_bytes)
        .map_err(|_| ChainLaunch1Error::BindingMismatch("E0 ProfileScopeV1 decode failed"))?;
    scope
        .validate()
        .map_err(|_| ChainLaunch1Error::BindingMismatch("E0 ProfileScopeV1 validate failed"))?;
    if scope.domain_registry_digest != domain_reg_id {
        return Err(ChainLaunch1Error::BindingMismatch(
            "ProfileScopeV1 domain_registry_digest != separate DomainRegistryV1 digest",
        ));
    }
    let scope_id = scope
        .tag()
        .map_err(|_| ChainLaunch1Error::BindingMismatch("E0 ProfileScopeV1 tag failed"))?
        .digest();
    if genesis_pkg.state.initial_profile_scope_digest != scope_id
        || genesis_pkg.state.initial_domain_registry_digest != domain_reg_id
    {
        return Err(ChainLaunch1Error::BindingMismatch(
            "outer GenesisPackageV1 disagrees with carried ProfileScopeV1 or DomainRegistryV1",
        ));
    }

    let bound_profile =
        BoundProfileManifestV1::decode_canonical(&bundle.candidate_profile_bytes)
            .map_err(|_| ChainLaunch1Error::BindingMismatch("E2 BoundProfileManifestV1 decode"))?;
    bound_profile
        .verify()
        .map_err(|_| ChainLaunch1Error::BindingMismatch("E2 BoundProfileManifestV1 verify"))?;
    if bound_profile.candidate.genesis_id != genesis_pkg.genesis_id
        || bound_profile.candidate.genesis_scope_digest != scope_id
        || bound_profile.candidate.domain_registry_digest != domain_reg_id
    {
        return Err(ChainLaunch1Error::BindingMismatch(
            "CandidateProfileV1 disagrees with genesis/scope/domain_registry",
        ));
    }
    let profile_id = bound_profile.profile_id;

    let trans_spec = ResolvedProfileTransitionSpecV1::decode_canonical(
        &bundle.transition_spec_bytes,
    )
    .map_err(|_| ChainLaunch1Error::BindingMismatch("E3 ResolvedProfileTransitionSpecV1 decode"))?;
    trans_spec
        .validate()
        .map_err(|_| ChainLaunch1Error::BindingMismatch("E3 transition spec validate"))?;
    if trans_spec.profile_id != profile_id {
        return Err(ChainLaunch1Error::BindingMismatch(
            "TransitionSpecAtProfileV1 profile_id mismatch",
        ));
    }
    let trans_spec_id = trans_spec
        .tag()
        .map_err(|_| ChainLaunch1Error::BindingMismatch("E3 transition spec tag"))?
        .digest();

    let admission = ProfileAdmissionManifestV1::decode_canonical(&bundle.profile_admission_bytes)
        .map_err(|_| {
        ChainLaunch1Error::BindingMismatch("E4 ProfileAdmissionManifestV1 decode")
    })?;
    admission
        .validate()
        .map_err(|_| ChainLaunch1Error::BindingMismatch("E4 admission manifest validate"))?;
    if admission.admitted_profile_id != profile_id
        || admission.admitted_transition_spec_at_profile_id != trans_spec_id
    {
        return Err(ChainLaunch1Error::BindingMismatch(
            "ProfileAdmissionManifestV1 disagrees with profile_id or transition_spec_id",
        ));
    }
    let admission_id = admission
        .tag()
        .map_err(|_| ChainLaunch1Error::BindingMismatch("E4 admission tag"))?
        .digest();

    let parity = GenesisRootParityRecordV1::decode_canonical(&bundle.genesis_root_parity_bytes)
        .map_err(|_| ChainLaunch1Error::BindingMismatch("E5 GenesisRootParityRecordV1 decode"))?;
    if parity.genesis_id != genesis_pkg.genesis_id
        || parity.initial_profile_id != profile_id
        || parity.initial_transition_spec_at_profile_id != trans_spec_id
    {
        return Err(ChainLaunch1Error::BindingMismatch(
            "GenesisRootParityRecordV1 disagrees with genesis/profile/spec",
        ));
    }
    let parity_id = parity
        .parity_record_id()
        .map_err(|_| ChainLaunch1Error::BindingMismatch("E5 parity_record_id failed"))?;

    let release = AdmittedReleaseRecordV1::decode_canonical(&bundle.admitted_release_bytes)
        .map_err(|_| ChainLaunch1Error::BindingMismatch("E6 AdmittedReleaseRecordV1 decode"))?;
    if release.core.genesis_id != genesis_pkg.genesis_id
        || release.core.admitted_profile_id != profile_id
        || release.core.admitted_transition_spec_at_profile_id != trans_spec_id
        || release.core.profile_admission_manifest_id != admission_id
        || release.core.genesis_root_parity_record_id != parity_id
    {
        return Err(ChainLaunch1Error::BindingMismatch(
            "AdmittedReleaseRecordV1 disagrees with genesis/profile/spec/admission/parity",
        ));
    }
    let release_id = release
        .tag()
        .map_err(|_| ChainLaunch1Error::BindingMismatch("E6 release tag failed"))?
        .digest();

    // Decode & authenticate supplementary artifacts S (if any) + check full RELEASE-1 verification when S carries R1 artifacts.
    let mut supp_ids = BTreeSet::new();
    let mut supp_inputs: Option<CandidateBuildInputsV1> = None;
    let mut supp_receipt: Option<CandidateReleaseReceiptV1> = None;
    for s in &bundle.supplementary_artifacts {
        if s.typed_artifact_schema_id == GENESIS_D1_PACKAGE_SCHEMA {
            return Err(ChainLaunch1Error::DagViolation(
                "in-bundle duplicate GenesisPackageV1 forbidden",
            ));
        }
        let recomputed = if s.typed_artifact_schema_id == EXECUTION_ROOT_1_COMPOSITION_SCHEMA {
            let comp =
                ExecutionRootCompositionV1::decode_canonical(&s.canonical_artifact_preimage_bytes)
                    .map_err(|_| {
                        ChainLaunch1Error::BindingMismatch(
                            "supplementary ExecutionRootCompositionV1",
                        )
                    })?;
            if comp.application_root != parity.initial_application_root
                || comp.execution_state_root
                    != parity.initial_execution_root_ref.execution_state_root
            {
                return Err(ChainLaunch1Error::BindingMismatch(
                    "supplementary composition roots disagree with parity record",
                ));
            }
            comp.tag()
                .map_err(|_| ChainLaunch1Error::BindingMismatch("supp composition tag"))?
                .digest()
        } else if s.typed_artifact_schema_id == RELEASE_R1_BUILD_INPUTS_SCHEMA {
            let ci = CandidateBuildInputsV1::decode_canonical(&s.canonical_artifact_preimage_bytes)
                .map_err(|_| {
                    ChainLaunch1Error::BindingMismatch("supplementary CandidateBuildInputsV1")
                })?;
            ci.verify_bound_profile(&bound_profile).map_err(|_| {
                ChainLaunch1Error::BindingMismatch("supp candidate inputs bound_profile mismatch")
            })?;
            let id = ci
                .tag()
                .map_err(|_| ChainLaunch1Error::BindingMismatch("supp inputs tag"))?
                .digest();
            supp_inputs = Some(ci);
            id
        } else if s.typed_artifact_schema_id == RELEASE_R1_CANDIDATE_RECEIPT_SCHEMA {
            let cr =
                CandidateReleaseReceiptV1::decode_canonical(&s.canonical_artifact_preimage_bytes)
                    .map_err(|_| {
                    ChainLaunch1Error::BindingMismatch("supplementary CandidateReleaseReceiptV1")
                })?;
            let id = cr
                .tag()
                .map_err(|_| ChainLaunch1Error::BindingMismatch("supp receipt tag"))?
                .digest();
            supp_receipt = Some(cr);
            id
        } else {
            return Err(ChainLaunch1Error::DagViolation(
                "unknown supplementary artifact schema",
            ));
        };
        if recomputed != s.typed_artifact_id {
            return Err(ChainLaunch1Error::BindingMismatch(
                "supplementary artifact typed_artifact_id mismatch",
            ));
        }
        if !supp_ids.insert(s.typed_artifact_id) {
            return Err(ChainLaunch1Error::DagViolation(
                "duplicate supplementary artifact ID",
            ));
        }
    }
    if let (Some(ci), Some(cr)) = (&supp_inputs, &supp_receipt) {
        let verified_rel_id = verify_admitted_release_record(
            &release,
            &genesis_pkg,
            &admission,
            &parity,
            ci,
            cr,
            genesis_release_policy,
            None,
        )
        .map_err(|_| ChainLaunch1Error::BindingMismatch("RELEASE-1 verification failed"))?;
        if verified_rel_id != release_id {
            return Err(ChainLaunch1Error::BindingMismatch(
                "verified RELEASE-1 record ID mismatch",
            ));
        }
    }

    // 4. Combined DAG verification (node bijection, external genesis outgoing refs, strict ranks, permitted classes).
    let explicit_ids = [
        scope_id,
        domain_reg_id,
        profile_id,
        trans_spec_id,
        admission_id,
        parity_id,
        release_id,
    ];
    let mut expected_internal_ids = BTreeSet::new();
    for id in explicit_ids {
        if !expected_internal_ids.insert(id) {
            return Err(ChainLaunch1Error::DagViolation(
                "duplicate explicit artifact ID",
            ));
        }
    }
    for &sid in &supp_ids {
        if !expected_internal_ids.insert(sid) {
            return Err(ChainLaunch1Error::DagViolation(
                "supplementary artifact duplicates an explicit artifact ID",
            ));
        }
    }
    if expected_internal_ids.contains(&genesis_pkg.genesis_id) {
        return Err(ChainLaunch1Error::DagViolation(
            "internal artifact collides with external genesis_id",
        ));
    }

    let mut ranks: BTreeMap<[u8; 32], u32> = BTreeMap::new();
    for node in &bundle.internal_dependency_nodes {
        if !expected_internal_ids.contains(&node.typed_artifact_id) {
            return Err(ChainLaunch1Error::DagViolation(
                "internal DAG node does not match any carried artifact",
            ));
        }
        if ranks
            .insert(node.typed_artifact_id, node.topological_rank_u32)
            .is_some()
        {
            return Err(ChainLaunch1Error::DagViolation(
                "duplicate internal DAG node ID",
            ));
        }
    }
    if ranks.len() != expected_internal_ids.len() {
        return Err(ChainLaunch1Error::DagViolation(
            "carried artifact missing from internal DAG nodes",
        ));
    }

    // Check external genesis node X outgoing refs & strict rank monotonicity.
    let x_rank = bundle
        .external_genesis_node
        .external_root_topological_rank_u32;
    let mut seen_x_scope = false;
    let mut seen_x_reg = false;
    for r in &bundle.external_genesis_node.outgoing_refs {
        let target_rank = *ranks
            .get(&r.typed_internal_artifact_id)
            .ok_or(ChainLaunch1Error::DagViolation("X target not in D"))?;
        if target_rank >= x_rank {
            return Err(ChainLaunch1Error::DagViolation(
                "X outgoing ref violates strict rank monotonicity",
            ));
        }
        match r.outgoing_class {
            ExternalGenesisOutgoingDependencyClassV1::GenesisInitialScopeV1 => {
                if r.typed_internal_artifact_id != scope_id {
                    return Err(ChainLaunch1Error::DagViolation(
                        "GenesisInitialScopeV1 target != ProfileScopeV1",
                    ));
                }
                seen_x_scope = true;
            }
            ExternalGenesisOutgoingDependencyClassV1::GenesisInitialDomainRegistryV1 => {
                if r.typed_internal_artifact_id != domain_reg_id {
                    return Err(ChainLaunch1Error::DagViolation(
                        "GenesisInitialDomainRegistryV1 target != DomainRegistryV1",
                    ));
                }
                seen_x_reg = true;
            }
        }
    }
    if !(seen_x_scope && seen_x_reg) {
        return Err(ChainLaunch1Error::DagViolation(
            "X must carry both GenesisInitialScopeV1 and GenesisInitialDomainRegistryV1",
        ));
    }

    // Check every internal node's dependency refs for strict rank monotonicity & edge-class legality.
    let mut has_candidate_to_x = false;
    let mut has_parity_to_x = false;
    let mut has_release_to_x = false;
    let mut has_scope_to_reg = false;
    for node in &bundle.internal_dependency_nodes {
        let nid = node.typed_artifact_id;
        let src_rank = node.topological_rank_u32;
        for dep in &node.dependency_refs {
            match *dep {
                LaunchClosureDependencyRefV1::InternalLaunchArtifactV1(tid) => {
                    let dst_rank = *ranks.get(&tid).ok_or(ChainLaunch1Error::DagViolation(
                        "internal dependency targets unknown or unranked node",
                    ))?;
                    if dst_rank >= src_rank {
                        return Err(ChainLaunch1Error::DagViolation(
                            "internal edge violates strict topological rank order",
                        ));
                    }
                    if nid == scope_id && tid == domain_reg_id {
                        has_scope_to_reg = true;
                    }
                }
                LaunchClosureDependencyRefV1::ExternalGenesisRootV1 {
                    genesis_id,
                    edge_class,
                } => {
                    if genesis_id != genesis_pkg.genesis_id {
                        return Err(ChainLaunch1Error::DagViolation(
                            "external edge references unknown genesis_id",
                        ));
                    }
                    if x_rank >= src_rank {
                        return Err(ChainLaunch1Error::DagViolation(
                            "internal-to-external edge violates strict rank monotonicity",
                        ));
                    }
                    if nid == scope_id || nid == domain_reg_id {
                        return Err(ChainLaunch1Error::DagViolation(
                            "reversed scope/registry -> genesis edge forbidden",
                        ));
                    }
                    if nid == trans_spec_id || nid == admission_id {
                        return Err(ChainLaunch1Error::DagViolation(
                            "TransitionSpecAtProfileV1 and ProfileAdmissionManifestV1 cannot emit direct genesis edges",
                        ));
                    }
                    match edge_class {
                        ExternalGenesisIncomingDependencyClassV1::InitialCandidateGenesisIdentityV1 => {
                            if nid != profile_id {
                                return Err(ChainLaunch1Error::DagViolation(
                                    "InitialCandidateGenesisIdentityV1 permitted only on CandidateProfileV1",
                                ));
                            }
                            has_candidate_to_x = true;
                        }
                        ExternalGenesisIncomingDependencyClassV1::GenesisStateDerivationV1 => {
                            if nid != parity_id {
                                return Err(ChainLaunch1Error::DagViolation(
                                    "GenesisStateDerivationV1 permitted only on GenesisRootParityRecordV1",
                                ));
                            }
                            has_parity_to_x = true;
                        }
                        ExternalGenesisIncomingDependencyClassV1::GenesisReleaseAuthorityV1 => {
                            if nid != release_id {
                                return Err(ChainLaunch1Error::DagViolation(
                                    "GenesisReleaseAuthorityV1 permitted only on AdmittedReleaseRecordV1",
                                ));
                            }
                            has_release_to_x = true;
                        }
                    }
                }
            }
        }
    }
    if !has_scope_to_reg {
        return Err(ChainLaunch1Error::DagViolation(
            "ProfileScopeV1 must depend on DomainRegistryV1",
        ));
    }
    if !(has_candidate_to_x && has_parity_to_x && has_release_to_x) {
        return Err(ChainLaunch1Error::DagViolation(
            "missing mandatory internal-to-external genesis edge",
        ));
    }

    // 5. Verify ChainLaunchManifestV1 bindings, G_0 launch authority projection, & non-self-reference.
    let expected_launch_policy_id = genesis_launch_policy.policy_id()?;
    let m = &pkg.manifest;
    let expected_parent =
        ChainLaunchManifestV1::derive_genesis_parent_block_id(genesis_pkg.genesis_id);
    if m.genesis_id != genesis_pkg.genesis_id
        || m.initial_profile_id != profile_id
        || m.initial_transition_spec_at_profile_id != trans_spec_id
        || m.initial_profile_admission_manifest_id != admission_id
        || m.genesis_root_parity_record_id != parity_id
        || m.initial_execution_root_ref != parity.initial_execution_root_ref
        || m.initial_release_record_id != release_id
        || m.launch_closure_bundle_id != bundle_id
        || m.launch_authority_policy_id != expected_launch_policy_id
        || m.genesis_parent_block_id != expected_parent
        || m.genesis_history_root != parity.genesis_history_root
        || m.first_ledger_height != 1
    {
        return Err(ChainLaunch1Error::BindingMismatch(
            "ChainLaunchManifestV1 field disagrees with verified closure bundle or G_0 policy",
        ));
    }
    let manifest_id = m.tag()?.digest();
    if manifest_id != pkg.chain_launch_manifest_id {
        return Err(ChainLaunch1Error::BindingMismatch(
            "chain_launch_manifest_id mismatch",
        ));
    }
    // Manifest must never self-reference its own identifier in any field.
    if [
        m.genesis_id,
        m.initial_profile_id,
        m.initial_transition_spec_at_profile_id,
        m.initial_profile_admission_manifest_id,
        m.genesis_root_parity_record_id,
        m.initial_release_record_id,
        m.launch_closure_bundle_id,
        m.launch_authority_policy_id,
        m.genesis_parent_block_id,
        m.genesis_history_root,
    ]
    .contains(&manifest_id)
    {
        return Err(ChainLaunch1Error::DagViolation(
            "ChainLaunchManifestV1 self-reference detected",
        ));
    }

    // 6. Verify detached 3-of-5 genesis-committed ChainLaunchAuthorizationV1.
    let auth = &pkg.authorization;
    if auth.chain_launch_manifest_id != manifest_id
        || auth.launch_authority_policy_id != expected_launch_policy_id
        || auth.signature_suite != SignatureSuiteIdV1::Ed25519StrictRfc8032V1
    {
        return Err(ChainLaunch1Error::InvalidLaunchAuthorization(
            "ChainLaunchAuthorizationV1 manifest, policy, or suite mismatch",
        ));
    }
    if auth.signatures.len() < usize::from(genesis_launch_policy.threshold_m) {
        return Err(ChainLaunch1Error::InvalidLaunchAuthorization(
            "insufficient launch authority signatures (< 3-of-5)",
        ));
    }
    let preimage = ChainLaunchAuthorizationV1::signing_preimage(
        genesis_pkg.state.chain_id,
        genesis_pkg.genesis_id,
        manifest_id,
        profile_id,
        expected_launch_policy_id,
        auth.signature_suite,
    );
    let mut seen_idx = BTreeSet::new();
    for (pos, entry) in auth.signatures.iter().enumerate() {
        let idx = usize::from(entry.signer_index);
        if idx >= genesis_launch_policy.authorized_pubkeys.len() {
            return Err(ChainLaunch1Error::InvalidLaunchAuthorization(
                "launch signer_index out of range",
            ));
        }
        if !seen_idx.insert(entry.signer_index) {
            return Err(ChainLaunch1Error::InvalidLaunchAuthorization(
                "duplicate launch signer_index",
            ));
        }
        if pos > 0 && auth.signatures[pos - 1].signer_index >= entry.signer_index {
            return Err(ChainLaunch1Error::InvalidLaunchAuthorization(
                "launch signatures must be strictly ascending by signer_index",
            ));
        }
        if entry.public_key != genesis_launch_policy.authorized_pubkeys[idx] {
            return Err(ChainLaunch1Error::InvalidLaunchAuthorization(
                "unauthorized launch signer public key",
            ));
        }
        verify_preimage_strict(&entry.public_key, &preimage, &entry.signature).map_err(|_| {
            ChainLaunch1Error::InvalidLaunchAuthorization(
                "launch Ed25519 signature verification failed",
            )
        })?;
    }

    Ok(VerifiedChainLaunchOutcomeV1 {
        preflight,
        launch_closure_bundle_id: bundle_id,
        chain_launch_manifest_id: manifest_id,
        genesis_id: genesis_pkg.genesis_id,
        initial_profile_id: profile_id,
    })
}

/// Complete deterministic `bitblock.devnet.v1` launch-package build result (for tests & probes).
#[derive(Clone, Debug)]
pub struct CanonicalDevnetLaunchBuildV1 {
    /// Genesis-committed `3-of-5` release authority policy (`RELEASE-1`).
    pub release_policy: TrustedReleaseAuthorityPolicyV1,
    /// Genesis-committed `3-of-5` launch authority policy (`CHAIN-LAUNCH-1`).
    pub launch_policy: TrustedLaunchAuthorityPolicyV1,
    /// Constructed [`LaunchClosureBundleV1`].
    pub bundle: LaunchClosureBundleV1,
    /// Constructed and verified [`ChainLaunchPackageV1`].
    pub package: ChainLaunchPackageV1,
}

/// Builds and verifies the canonical `bitblock.devnet.v1` [`ChainLaunchPackageV1`] for either:
/// - `include_supplementary = true` (`|S| = 3`: `10` internal / `11` complete artifacts), or
/// - `include_supplementary = false` (`|S| = 0`: `7` internal / `8` complete artifacts).
pub fn build_canonical_devnet_chain_launch(
    include_supplementary: bool,
) -> Result<CanonicalDevnetLaunchBuildV1, ChainLaunch1Error> {
    let fx = CanonicalDevnetGenesisFixtureV1::build()
        .map_err(|_| ChainLaunch1Error::BindingMismatch("devnet genesis fixture build failed"))?;
    let raw_genesis = fx
        .package
        .encode_canonical()
        .map_err(|_| ChainLaunch1Error::BindingMismatch("genesis encode failed"))?;
    let raw_preimage = fx
        .bundle
        .encode_canonical()
        .map_err(|_| ChainLaunch1Error::BindingMismatch("preimage bundle encode failed"))?;
    let parity = verify_genesis_root_parity(&GenesisParityInputV1 {
        raw_genesis_package: &raw_genesis,
        raw_preimage_bundle: &raw_preimage,
        profile_scope: &fx.profile_scope,
        commit_domain_registry: &fx.commit_domain_registry,
        bound_manifest: &fx.bound_manifest,
        transition_spec: &fx.transition_spec,
        exec_registry: &fx.exec_registry,
    })
    .map_err(|_| ChainLaunch1Error::BindingMismatch("genesis parity failed"))?;
    let (_closure, admission) = build_canonical_devnet_profile_admission(
        &fx.package,
        &fx.bound_manifest,
        &fx.transition_spec,
        &parity.record,
    )
    .map_err(|_| ChainLaunch1Error::BindingMismatch("profile admission failed"))?;
    let (cand_inputs, cand_receipt) = build_canonical_candidate_fixture(&fx.bound_manifest)
        .map_err(|_| ChainLaunch1Error::BindingMismatch("candidate fixture failed"))?;
    let (release_policy, release_record) = build_canonical_devnet_admitted_release(
        &fx.package,
        &admission,
        &parity.record,
        &cand_inputs,
        &cand_receipt,
    )
    .map_err(|_| ChainLaunch1Error::BindingMismatch("admitted release build failed"))?;

    let scope_b = fx
        .profile_scope
        .encode_canonical()
        .map_err(|_| ChainLaunch1Error::BindingMismatch("scope encode"))?;
    let reg_b = fx.commit_domain_registry.encode_canonical();
    let cand_b = fx
        .bound_manifest
        .encode_canonical()
        .map_err(|_| ChainLaunch1Error::BindingMismatch("bound encode"))?;
    let spec_b = fx
        .transition_spec
        .encode_canonical()
        .map_err(|_| ChainLaunch1Error::BindingMismatch("spec encode"))?;
    let adm_b = admission
        .encode_canonical()
        .map_err(|_| ChainLaunch1Error::BindingMismatch("admission encode"))?;
    let par_b = parity
        .record
        .encode_canonical()
        .map_err(|_| ChainLaunch1Error::BindingMismatch("parity encode"))?;
    let rel_b = release_record
        .encode_canonical()
        .map_err(|_| ChainLaunch1Error::BindingMismatch("release encode"))?;

    let scope_id = fx.profile_scope.tag().expect("scope tag").digest();
    let reg_id = fx.commit_domain_registry.tag().digest();
    let profile_id = fx.bound_manifest.profile_id;
    let spec_id = fx.transition_spec.tag().expect("spec tag").digest();
    let adm_id = admission.tag().expect("adm tag").digest();
    let par_id = parity.record.parity_record_id().expect("par id");
    let rel_id = release_record.tag().expect("rel tag").digest();
    let gid = fx.package.genesis_id;

    let mut supp_artifacts = Vec::new();
    let mut extra_nodes = Vec::new();
    let mut parity_refs = vec![
        LaunchClosureDependencyRefV1::InternalLaunchArtifactV1(profile_id),
        LaunchClosureDependencyRefV1::InternalLaunchArtifactV1(spec_id),
        LaunchClosureDependencyRefV1::ExternalGenesisRootV1 {
            genesis_id: gid,
            edge_class: ExternalGenesisIncomingDependencyClassV1::GenesisStateDerivationV1,
        },
    ];
    let mut release_refs = vec![
        LaunchClosureDependencyRefV1::InternalLaunchArtifactV1(profile_id),
        LaunchClosureDependencyRefV1::InternalLaunchArtifactV1(spec_id),
        LaunchClosureDependencyRefV1::InternalLaunchArtifactV1(adm_id),
        LaunchClosureDependencyRefV1::InternalLaunchArtifactV1(par_id),
        LaunchClosureDependencyRefV1::ExternalGenesisRootV1 {
            genesis_id: gid,
            edge_class: ExternalGenesisIncomingDependencyClassV1::GenesisReleaseAuthorityV1,
        },
    ];

    if include_supplementary {
        let comp_b = parity
            .composition
            .encode_canonical()
            .map_err(|_| ChainLaunch1Error::BindingMismatch("comp encode"))?;
        let comp_id = parity.composition.tag().expect("comp tag").digest();
        let in_b = cand_inputs
            .encode_canonical()
            .map_err(|_| ChainLaunch1Error::BindingMismatch("inputs encode"))?
            .to_vec();
        let in_id = cand_inputs.tag().expect("inputs tag").digest();
        let rc_b = cand_receipt
            .encode_canonical()
            .map_err(|_| ChainLaunch1Error::BindingMismatch("rcpt encode"))?
            .to_vec();
        let rc_id = cand_receipt.tag().expect("rcpt tag").digest();

        supp_artifacts.push(SupplementaryLaunchArtifactV1 {
            typed_artifact_schema_id: EXECUTION_ROOT_1_COMPOSITION_SCHEMA.to_vec(),
            typed_artifact_id: comp_id,
            canonical_artifact_preimage_length_u64: comp_b.len() as u64,
            canonical_artifact_preimage_bytes: comp_b,
        });
        supp_artifacts.push(SupplementaryLaunchArtifactV1 {
            typed_artifact_schema_id: RELEASE_R1_BUILD_INPUTS_SCHEMA.to_vec(),
            typed_artifact_id: in_id,
            canonical_artifact_preimage_length_u64: in_b.len() as u64,
            canonical_artifact_preimage_bytes: in_b,
        });
        supp_artifacts.push(SupplementaryLaunchArtifactV1 {
            typed_artifact_schema_id: RELEASE_R1_CANDIDATE_RECEIPT_SCHEMA.to_vec(),
            typed_artifact_id: rc_id,
            canonical_artifact_preimage_length_u64: rc_b.len() as u64,
            canonical_artifact_preimage_bytes: rc_b,
        });
        supp_artifacts.sort_by_key(|s| s.typed_artifact_id);

        extra_nodes.push(make_node(
            5,
            comp_id,
            vec![
                LaunchClosureDependencyRefV1::InternalLaunchArtifactV1(reg_id),
                LaunchClosureDependencyRefV1::InternalLaunchArtifactV1(profile_id),
            ],
        ));
        extra_nodes.push(make_node(
            5,
            in_id,
            vec![
                LaunchClosureDependencyRefV1::InternalLaunchArtifactV1(reg_id),
                LaunchClosureDependencyRefV1::InternalLaunchArtifactV1(profile_id),
            ],
        ));
        extra_nodes.push(make_node(
            6,
            rc_id,
            vec![
                LaunchClosureDependencyRefV1::InternalLaunchArtifactV1(in_id),
                LaunchClosureDependencyRefV1::InternalLaunchArtifactV1(profile_id),
            ],
        ));
        parity_refs.push(LaunchClosureDependencyRefV1::InternalLaunchArtifactV1(
            comp_id,
        ));
        release_refs.push(LaunchClosureDependencyRefV1::InternalLaunchArtifactV1(
            in_id,
        ));
        release_refs.push(LaunchClosureDependencyRefV1::InternalLaunchArtifactV1(
            rc_id,
        ));
    }

    let mut nodes = vec![
        make_node(0, reg_id, vec![]),
        make_node(
            1,
            scope_id,
            vec![LaunchClosureDependencyRefV1::InternalLaunchArtifactV1(
                reg_id,
            )],
        ),
        make_node(
            3,
            profile_id,
            vec![
                LaunchClosureDependencyRefV1::InternalLaunchArtifactV1(reg_id),
                LaunchClosureDependencyRefV1::InternalLaunchArtifactV1(scope_id),
                LaunchClosureDependencyRefV1::ExternalGenesisRootV1 {
                    genesis_id: gid,
                    edge_class:
                        ExternalGenesisIncomingDependencyClassV1::InitialCandidateGenesisIdentityV1,
                },
            ],
        ),
        make_node(
            4,
            spec_id,
            vec![
                LaunchClosureDependencyRefV1::InternalLaunchArtifactV1(scope_id),
                LaunchClosureDependencyRefV1::InternalLaunchArtifactV1(profile_id),
            ],
        ),
        make_node(
            5,
            adm_id,
            vec![
                LaunchClosureDependencyRefV1::InternalLaunchArtifactV1(profile_id),
                LaunchClosureDependencyRefV1::InternalLaunchArtifactV1(spec_id),
            ],
        ),
        make_node(6, par_id, parity_refs),
        make_node(7, rel_id, release_refs),
    ];
    nodes.extend(extra_nodes);
    nodes.sort_by_key(|n| n.typed_artifact_id);

    let mut x_outgoing = vec![
        ExternalGenesisToInternalDependencyRefV1 {
            typed_internal_artifact_id: scope_id,
            outgoing_class: ExternalGenesisOutgoingDependencyClassV1::GenesisInitialScopeV1,
        },
        ExternalGenesisToInternalDependencyRefV1 {
            typed_internal_artifact_id: reg_id,
            outgoing_class:
                ExternalGenesisOutgoingDependencyClassV1::GenesisInitialDomainRegistryV1,
        },
    ];
    x_outgoing.sort_by_key(|r| r.typed_internal_artifact_id);

    let mut i2i = 0u32;
    let mut i2e = 0u32;
    for n in &nodes {
        for r in &n.dependency_refs {
            match r {
                LaunchClosureDependencyRefV1::InternalLaunchArtifactV1(_) => i2i += 1,
                LaunchClosureDependencyRefV1::ExternalGenesisRootV1 { .. } => i2e += 1,
            }
        }
    }
    let e2i = EXTERNAL_GENESIS_OUTGOING_REF_COUNT;
    let supp_count = supp_artifacts.len() as u32;
    let internal_count = EXPLICIT_LAUNCH_ARTIFACT_COUNT + supp_count;
    let complete_count = internal_count + EXTERNAL_GENESIS_ROOT_COUNT;

    let explicit_lens = [
        scope_b.len() as u64,
        reg_b.len() as u64,
        cand_b.len() as u64,
        spec_b.len() as u64,
        adm_b.len() as u64,
        par_b.len() as u64,
        rel_b.len() as u64,
    ];
    let internal_bytes: u64 = explicit_lens.iter().sum::<u64>()
        + supp_artifacts
            .iter()
            .map(|s| s.canonical_artifact_preimage_length_u64)
            .sum::<u64>();
    let external_bytes = raw_genesis.len() as u64;

    let bundle = LaunchClosureBundleV1 {
        schema_version: CHAIN_LAUNCH_1_VERSION,
        internal_artifact_count_u32: internal_count,
        external_root_count_u32: EXTERNAL_GENESIS_ROOT_COUNT,
        complete_artifact_count_u32: complete_count,
        supplementary_artifact_count_u32: supp_count,
        internal_to_internal_dependency_edge_count_u32: i2i,
        internal_to_external_dependency_edge_count_u32: i2e,
        external_to_internal_dependency_edge_count_u32: e2i,
        complete_dependency_edge_count_u32: i2i + i2e + e2i,
        internal_canonical_preimage_bytes_u64: internal_bytes,
        external_root_canonical_preimage_bytes_u64: external_bytes,
        total_canonical_preimage_bytes_u64: internal_bytes + external_bytes,
        external_genesis_node: LaunchClosureExternalGenesisDependencyNodeV1 {
            external_root_schema_id: GENESIS_D1_PACKAGE_SCHEMA.to_vec(),
            external_root_topological_rank_u32: 2,
            genesis_id: gid,
            canonical_outer_genesis_package_length_u64: external_bytes,
            outgoing_refs: x_outgoing,
        },
        explicit_preimage_lengths_u64: explicit_lens,
        profile_scope_bytes: scope_b,
        domain_registry_bytes: reg_b,
        candidate_profile_bytes: cand_b,
        transition_spec_bytes: spec_b,
        profile_admission_bytes: adm_b,
        genesis_root_parity_bytes: par_b,
        admitted_release_bytes: rel_b,
        internal_dependency_nodes: nodes,
        supplementary_artifacts: supp_artifacts,
    };
    let bundle_bytes = bundle.encode_canonical()?;
    let bundle_id = bundle.tag()?.digest();

    let (launch_policy, launch_key_pairs) = TrustedLaunchAuthorityPolicyV1::devnet_three_of_five();
    let launch_policy_id = launch_policy.policy_id()?;
    let manifest = ChainLaunchManifestV1 {
        schema_version: CHAIN_LAUNCH_1_VERSION,
        genesis_id: gid,
        initial_profile_id: profile_id,
        initial_transition_spec_at_profile_id: spec_id,
        initial_profile_admission_manifest_id: adm_id,
        genesis_root_parity_record_id: par_id,
        initial_execution_root_ref: parity.record.initial_execution_root_ref,
        initial_release_record_id: rel_id,
        launch_closure_bundle_id: bundle_id,
        launch_authority_policy_id: launch_policy_id,
        genesis_parent_block_id: ChainLaunchManifestV1::derive_genesis_parent_block_id(gid),
        genesis_history_root: parity.record.genesis_history_root,
        first_ledger_height: 1,
    };
    let manifest_id = manifest.tag()?.digest();

    let preimage = ChainLaunchAuthorizationV1::signing_preimage(
        fx.package.state.chain_id,
        gid,
        manifest_id,
        profile_id,
        launch_policy_id,
        SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
    );
    let mut signatures = Vec::with_capacity(3);
    for (idx, (pk, seed)) in launch_key_pairs.iter().take(3).enumerate() {
        let signature = sign_preimage_deterministic(seed, &preimage).map_err(|_| {
            ChainLaunch1Error::InvalidLaunchAuthorization("launch deterministic signing failed")
        })?;
        signatures.push(LaunchSignerSignatureV1 {
            signer_index: idx as u8,
            public_key: *pk,
            signature,
        });
    }
    let authorization = ChainLaunchAuthorizationV1 {
        schema_version: CHAIN_LAUNCH_1_VERSION,
        chain_launch_manifest_id: manifest_id,
        launch_authority_policy_id: launch_policy_id,
        signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
        signatures,
    };
    let package = ChainLaunchPackageV1 {
        schema_version: CHAIN_LAUNCH_1_VERSION,
        genesis_package_bytes: raw_genesis,
        launch_closure_bundle_bytes: bundle_bytes,
        launch_closure_bundle_id: bundle_id,
        manifest,
        chain_launch_manifest_id: manifest_id,
        authorization,
    };
    verify_chain_launch_package(
        &package,
        &release_policy,
        &launch_policy,
        LaunchPreflightCapsV1::V1_DEFAULT,
    )?;
    Ok(CanonicalDevnetLaunchBuildV1 {
        release_policy,
        launch_policy,
        bundle,
        package,
    })
}

fn make_node(
    topological_rank_u32: u32,
    typed_artifact_id: [u8; 32],
    mut dependency_refs: Vec<LaunchClosureDependencyRefV1>,
) -> LaunchClosureInternalDependencyNodeV1 {
    dependency_refs.sort_by_key(LaunchClosureDependencyRefV1::sort_key);
    LaunchClosureInternalDependencyNodeV1 {
        topological_rank_u32,
        typed_artifact_id,
        dependency_refs,
    }
}

fn scan_slice<'a>(
    raw: &'a [u8],
    cur: &mut usize,
    len: usize,
) -> Result<&'a [u8], ChainLaunch1Error> {
    let end = cur
        .checked_add(len)
        .ok_or(ChainLaunch1Error::PreflightRejected(
            "slice offset overflow",
        ))?;
    if end > raw.len() {
        return Err(ChainLaunch1Error::PreflightRejected(
            "truncated bundle frame",
        ));
    }
    let out = &raw[*cur..end];
    *cur = end;
    Ok(out)
}

fn scan_u8(raw: &[u8], cur: &mut usize) -> Result<u8, ChainLaunch1Error> {
    Ok(scan_slice(raw, cur, 1)?[0])
}

fn scan_u16(raw: &[u8], cur: &mut usize) -> Result<u16, ChainLaunch1Error> {
    let s = scan_slice(raw, cur, 2)?;
    Ok(u16::from_be_bytes([s[0], s[1]]))
}

fn scan_u32(raw: &[u8], cur: &mut usize) -> Result<u32, ChainLaunch1Error> {
    let s = scan_slice(raw, cur, 4)?;
    Ok(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

fn scan_u64(raw: &[u8], cur: &mut usize) -> Result<u64, ChainLaunch1Error> {
    let s = scan_slice(raw, cur, 8)?;
    Ok(u64::from_be_bytes(s.try_into().expect("8")))
}

fn scan_arr32(raw: &[u8], cur: &mut usize) -> Result<[u8; 32], ChainLaunch1Error> {
    Ok(scan_slice(raw, cur, 32)?.try_into().expect("32"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_chain_launch_s3_and_s0_codecs_roundtrip_and_golden_tags() {
        let s3 = build_canonical_devnet_chain_launch(true).expect("S=3 launch build");
        assert_eq!(s3.bundle.internal_artifact_count_u32, 10);
        assert_eq!(s3.bundle.complete_artifact_count_u32, 11);
        assert_eq!(s3.bundle.complete_dependency_edge_count_u32, 27);
        let wire_pkg = s3.package.encode_canonical().expect("pkg encode");
        let rt_pkg =
            ChainLaunchPackageV1::decode_canonical(&wire_pkg, LaunchPreflightCapsV1::V1_DEFAULT)
                .expect("pkg decode");
        assert_eq!(rt_pkg, s3.package);
        let outcome = verify_chain_launch_package(
            &rt_pkg,
            &s3.release_policy,
            &s3.launch_policy,
            LaunchPreflightCapsV1::V1_DEFAULT,
        )
        .expect("verify S=3");
        assert_eq!(outcome.preflight.complete_artifact_count_u32, 11);
        assert_eq!(
            s3.bundle.tag().expect("bundle tag").to_string(),
            "#bb.receipt.sha256.2cyxfi3goddwndgyfzvjy7y3eofikotdyry4wy7s7wzkubuzjm5a"
        );
        assert_eq!(
            s3.package.manifest.tag().expect("manifest tag").to_string(),
            "#bb.policy.sha256.nxam3harptfty3tsbfqz6dtwnt44cgm7zdrd46334v4grcuzuspa"
        );
        assert_eq!(
            s3.package
                .authorization
                .tag()
                .expect("auth tag")
                .to_string(),
            "#bb.receipt.sha256.daxw2r45k4zxrw3ypg6iupja7mcfdnyekgxrgsuhhi3gn75s3lua"
        );
        assert_eq!(
            s3.package.tag().expect("package tag").to_string(),
            "#bb.receipt.sha256.2xyk6dukblpqmqfkkovaez7obpgck5j6yki5tqqa3xjcvg5ktgna"
        );

        let s0 = build_canonical_devnet_chain_launch(false).expect("S=0 launch build");
        assert_eq!(s0.bundle.internal_artifact_count_u32, 7);
        assert_eq!(s0.bundle.complete_artifact_count_u32, 8);
        assert_eq!(s0.bundle.complete_dependency_edge_count_u32, 18);
        assert_eq!(
            s0.package.tag().expect("s0 pkg tag").to_string(),
            "#bb.receipt.sha256.apvfatsiegwvk52s4s25y5ffnhss63flbd5ydehwukrh75yuvcoa"
        );
    }
}
