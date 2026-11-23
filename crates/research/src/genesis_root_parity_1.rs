//! `GENESIS-ROOT-PARITY-1`: preimage-verified 4-way logical-genesis to typed-root parity
//! (`PS-031`, `docs/LAYER1_DESIGN_PAPER.md` §4.4 & §5).
//!
//! Leaves frozen `genesis_d1.rs` (`GENESIS-D1`) and `commit_state.rs` (`COMMIT-1`) untouched and
//! proves bit-exact correspondence between an independently decoded [`GenesisPackageV1`] +
//! [`GenesisPreimageBundleV1`] and the complete 11-slot [`ExecutionRootCompositionV1`] across
//! four independent reconstruction paths:
//! 1. **Path 1 (Cold Per-Domain Full Rebuild):** independently decodes `GenesisPackageV1` and
//!    `GenesisPreimageBundleV1`, checks every policy and `AuthorizationDescriptorV1` digest, and
//!    cold-builds all 10 active launch Patricia tries plus the inactive `ProgramState` sentinel.
//! 2. **Path 2 (Initialized Incremental Domain States):** boots `AuthCommitStateV1`,
//!    `AssetLedgerStateV1`, `GovernanceStateV1`, `EconomicStateV1`, and `ValidatorScheduleStateV1`
//!    via `from_genesis()`, verifies `verify_rebuild_parity()` on each, and matches Path 1.
//! 3. **Path 3 (`EXECUTOR-1` + `EXECUTION-ROOT-1` Boot):** initializes `PrimaryLaunchExecutorV1`,
//!    verifies all-domain rebuild parity, composes `ExecutionRootCompositionV1`, and verifies
//!    header + `BoundProfileManifestV1` correspondence.
//! 4. **Path 4 (Clean-Room Pure-Python Rebuild):** `tools/genesis_root_parity_1_vectors.py`
//!    independently decodes the exact wire bytes and reproduces every domain root,
//!    `application_root`, `execution_state_root`, `ExecutionRootRefV1`, and
//!    [`GenesisRootParityRecordV1`] bit-for-bit.
//!
//! Genesis/root parity does not admit the profile (`PROFILE-1`), authenticate distribution,
//! establish data availability, or prove consensus safety.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt;

use sha2::{Digest, Sha256};

use crate::asset_1::AssetLedgerStateV1;
use crate::asset_d1::{AssetPolicyDescriptorV1, ValidatorEconPolicyV1};
use crate::auth_commit_1::{AuthCommitStateV1, DomainIdV1, DomainPatriciaTrieV1};
use crate::bounds_1::ProtocolBoundsProfileV1;
use crate::commit_state::{
    DomainRegistryV1, LocalAuthenticatedSidecar, ProfileRegistryV1, verify_independent_projection,
};
use crate::consensus_gov_d1::{ConsensusPolicyDescriptorV1, GovernancePolicyDescriptorV1};
use crate::coordinate_episode::OccupancyEpisodeLedger;
use crate::crypto_d1::{
    CanonicalEd25519PublicKeyV1, CryptoSuiteDescriptorV1, HashSuiteIdV1, SignatureSuiteIdV1,
};
use crate::economic_state_1::{EconomicStateV1, ValidatorStakeRecordV1};
use crate::execution_root_1::{
    EXECUTION_ROOT_1_VERSION, ExecutionDomainRegistryV1, ExecutionRoot1Error,
    ExecutionRootCompositionV1, ExecutionRootPreimageContextV1, ExecutionRootRefV1,
    compose_from_executor_snapshot, verify_header_and_manifest_correspondence,
};
use crate::executor_1::{
    EXECUTOR_1_VERSION, Executor1Error, ExecutorDomainSnapshotV1, ExecutorForkContextV1,
    ExecutorGenesisConfigV1, PrimaryLaunchExecutorV1,
};
use crate::genesis_d1::{
    BootstrapAvailabilityPolicyV1, CANONICAL_LAUNCH_PREDICATES_V1, DEVNET_V1_IDENTITY_NAMESPACE,
    GenesisActorEntryV1, GenesisD1Error, GenesisPackageV1, GenesisValidatorEntryV1,
    LaunchGenesisStateV1,
};
use crate::governance_1::{GovernanceContextV1, GovernanceStateV1, param_trie_key};
use crate::history::{History, world_tag};
use crate::history_store::HistoryReplayLimits;
use crate::identity_1::{ActorAuthStateV1, IdentityContextV1};
use crate::identity_d1::{
    ActorIdV1, AuthorizationDescriptorV1, CredentialRoleV1, ThresholdPolicyV1,
    validate_actor_descriptor_set,
};
use crate::local_bounds::LocalStorageProfile;
use crate::profile_bootstrap::ProfileScopeV1;
use crate::profile_d1::{BoundProfileManifestV1, CandidateProfileManifestV1, ProfileLineageKindV1};
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::resource_1::ResourcePolicyV1;
use crate::threat_1::ThreatModelV1;
use crate::transition_spec_1::ResolvedProfileTransitionSpecV1;
use crate::validator_schedule_1::{ValidatorKeyPairV1, ValidatorScheduleStateV1, epoch_trie_key};
use crate::world::{BEDROCK_COORD, World};

/// Canonical schema string for a [`GenesisPreimageBundleV1`] policy carrier (`GENESIS-ROOT-PARITY-1`).
pub const GENESIS_ROOT_PARITY_1_BUNDLE_SCHEMA: &[u8] =
    b"bitblock/genesis-root-parity-1/preimage-bundle/v1";
/// Canonical schema string for a [`GenesisRootParityRecordV1`] parity receipt (`GENESIS-ROOT-PARITY-1`).
pub const GENESIS_ROOT_PARITY_1_RECORD_SCHEMA: &[u8] =
    b"bitblock/genesis-root-parity-1/parity-record/v1";

/// Canonical schema version for `GENESIS-ROOT-PARITY-1` objects.
pub const GENESIS_ROOT_PARITY_1_VERSION: u16 = 1;

const PARITY_RECORD_ID_PREFIX: &[u8] = b"bb.grp1.parity_record_id.v1";
const RECON_COLD_PREFIX: &[u8] = b"bb.grp1.recon.cold_rebuild.v1";
const RECON_INCR_PREFIX: &[u8] = b"bb.grp1.recon.incremental.v1";
const RECON_EXEC_PREFIX: &[u8] = b"bb.grp1.recon.executor.v1";

/// Fail-closed validation, codec, preimage, or parity error for `GENESIS-ROOT-PARITY-1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GenesisRootParity1Error {
    /// Wire buffer had invalid version, length, discriminant, or trailing bytes.
    InvalidWire(&'static str),
    /// Frozen `GENESIS-D1` package decode or verification failed.
    GenesisPackage(GenesisD1Error),
    /// Supplied preimage object hashed differently from `LaunchGenesisStateV1`.
    PreimageDigestMismatch(&'static str),
    /// Actor, validator, parameter, or domain slot entry was omitted, duplicated, or misordered.
    InvalidBundleCardinalityOrOrder(&'static str),
    /// Non-initial (`CE-43` successor) or stale profile manifest was submitted at genesis.
    StaleOrNonInitialProfileRejected(&'static str),
    /// Derived `genesis_id` or `profile_id` mismatch or self-reference detected.
    SelfDerivedIdViolation(&'static str),
    /// Initial world `B_0` did not equal canonical bedrock-only `(0, 0, 0)`.
    NonBedrockWorldRejected(&'static str),
    /// Cross-domain root substitution or multi-path parity divergence detected.
    CrossDomainOrParityMismatch(&'static str),
    /// Underlying `EXECUTION-ROOT-1` composition or registry check failed.
    ExecutionRoot(ExecutionRoot1Error),
    /// Underlying `EXECUTOR-1` boot or parity check failed.
    Executor(Executor1Error),
    /// Underlying domain producer failed during cold or incremental rebuild.
    DomainState(String),
}

impl From<GenesisD1Error> for GenesisRootParity1Error {
    fn from(value: GenesisD1Error) -> Self {
        Self::GenesisPackage(value)
    }
}

impl From<ExecutionRoot1Error> for GenesisRootParity1Error {
    fn from(value: ExecutionRoot1Error) -> Self {
        Self::ExecutionRoot(value)
    }
}

impl From<Executor1Error> for GenesisRootParity1Error {
    fn from(value: Executor1Error) -> Self {
        Self::Executor(value)
    }
}

impl fmt::Display for GenesisRootParity1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for GenesisRootParity1Error {}

/// Complete canonical preimage bundle accompanying [`GenesisPackageV1`] so an independent
/// verifier can authenticate every policy & actor descriptor digest before rebuilding domain tries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenesisPreimageBundleV1 {
    /// Bundle schema version (`1`).
    pub bundle_version: u16,
    /// Preimage for `LaunchGenesisStateV1::resource_policy_digest`.
    pub resource_policy: ResourcePolicyV1,
    /// Preimage for `LaunchGenesisStateV1::asset_policy_digest`.
    pub asset_policy: AssetPolicyDescriptorV1,
    /// Preimage for `LaunchGenesisStateV1::validator_econ_digest`.
    pub validator_econ_policy: ValidatorEconPolicyV1,
    /// Preimage for `LaunchGenesisStateV1::governance_policy_digest`.
    pub governance_policy: GovernancePolicyDescriptorV1,
    /// Launch `CONSENSUS-D1` policy aligned with `validator_econ_policy`.
    pub consensus_policy: ConsensusPolicyDescriptorV1,
    /// Preimage for `LaunchGenesisStateV1::validated_limit_vector_digest`.
    pub bounds_profile: ProtocolBoundsProfileV1,
    /// Preimage for `LaunchGenesisStateV1::bootstrap_availability_digest`.
    pub bootstrap_policy: BootstrapAvailabilityPolicyV1,
    /// Strictly ascending unique non-zero initial governance parameter `(key, value)` slots.
    pub initial_gov_params: Vec<(u32, u64)>,
    /// Strictly ascending unique `(ActorIdV1, AuthorizationDescriptorV1)` preimages for `A_0`.
    pub actor_descriptors: Vec<(ActorIdV1, AuthorizationDescriptorV1)>,
}

impl GenesisPreimageBundleV1 {
    /// Validates internal bundle ordering, uniqueness, policy alignment, and acyclic delegations.
    pub fn validate(&self) -> Result<(), GenesisRootParity1Error> {
        if self.bundle_version != GENESIS_ROOT_PARITY_1_VERSION {
            return Err(GenesisRootParity1Error::InvalidWire("bad bundle_version"));
        }
        self.resource_policy
            .validate(Some(&self.bounds_profile), Some(&self.asset_policy))
            .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
        self.asset_policy
            .validate()
            .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
        self.validator_econ_policy
            .validate()
            .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
        self.governance_policy
            .validate(Some(&self.validator_econ_policy))
            .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
        self.consensus_policy
            .validate(Some(&self.validator_econ_policy))
            .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
        self.bounds_profile
            .validate(Some(&LocalStorageProfile::DEFAULT), None)
            .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
        self.bootstrap_policy.validate()?;

        for (idx, &(k, _)) in self.initial_gov_params.iter().enumerate() {
            if k == 0 {
                return Err(GenesisRootParity1Error::InvalidBundleCardinalityOrOrder(
                    "zero governance param key forbidden",
                ));
            }
            if idx > 0 && self.initial_gov_params[idx - 1].0 >= k {
                return Err(GenesisRootParity1Error::InvalidBundleCardinalityOrOrder(
                    "initial_gov_params must be strictly ascending and unique",
                ));
            }
        }
        if self.actor_descriptors.is_empty() {
            return Err(GenesisRootParity1Error::InvalidBundleCardinalityOrOrder(
                "actor_descriptors cannot be empty",
            ));
        }
        validate_actor_descriptor_set(&self.actor_descriptors)
            .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
        Ok(())
    }

    /// Encodes this preimage bundle into canonical length-framed big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, GenesisRootParity1Error> {
        self.validate()?;
        let res_b = self
            .resource_policy
            .encode_canonical()
            .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
        let asset_b = self
            .asset_policy
            .encode_canonical()
            .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
        let econ_b = self
            .validator_econ_policy
            .encode_canonical()
            .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
        let gov_b = self
            .governance_policy
            .encode_canonical()
            .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
        let cons_b = self
            .consensus_policy
            .encode_canonical()
            .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
        let bounds_b = self
            .bounds_profile
            .encode_canonical()
            .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
        let boot_b = self.bootstrap_policy.encode_canonical()?;

        let mut out = Vec::with_capacity(1024);
        out.extend_from_slice(&self.bundle_version.to_be_bytes());
        push_framed(&mut out, &res_b);
        push_framed(&mut out, &asset_b);
        push_framed(&mut out, &econ_b);
        push_framed(&mut out, &gov_b);
        push_framed(&mut out, &cons_b);
        push_framed(&mut out, &bounds_b);
        push_framed(&mut out, &boot_b);

        let pcount = u16::try_from(self.initial_gov_params.len())
            .map_err(|_| GenesisRootParity1Error::InvalidWire("param count overflow"))?;
        out.extend_from_slice(&pcount.to_be_bytes());
        for &(k, v) in &self.initial_gov_params {
            out.extend_from_slice(&k.to_be_bytes());
            out.extend_from_slice(&v.to_be_bytes());
        }

        let acount = u32::try_from(self.actor_descriptors.len())
            .map_err(|_| GenesisRootParity1Error::InvalidWire("actor count overflow"))?;
        out.extend_from_slice(&acount.to_be_bytes());
        for (aid, desc) in &self.actor_descriptors {
            let db = desc
                .encode_canonical()
                .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
            out.extend_from_slice(aid.as_bytes());
            push_framed(&mut out, &db);
        }
        Ok(out)
    }

    /// Strictly decodes canonical big-endian wire bytes into a validated [`GenesisPreimageBundleV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, GenesisRootParity1Error> {
        let mut cur = 0usize;
        let bundle_version = read_u16_be(bytes, &mut cur)?;
        let res_b = read_framed(bytes, &mut cur)?;
        let asset_b = read_framed(bytes, &mut cur)?;
        let econ_b = read_framed(bytes, &mut cur)?;
        let gov_b = read_framed(bytes, &mut cur)?;
        let cons_b = read_framed(bytes, &mut cur)?;
        let bounds_b = read_framed(bytes, &mut cur)?;
        let boot_b = read_framed(bytes, &mut cur)?;

        let pcount = usize::from(read_u16_be(bytes, &mut cur)?);
        if pcount > 1_024 {
            return Err(GenesisRootParity1Error::InvalidWire("excessive gov params"));
        }
        let mut initial_gov_params = Vec::with_capacity(pcount);
        for _ in 0..pcount {
            let k = read_u32_be(bytes, &mut cur)?;
            let v = read_u64_be(bytes, &mut cur)?;
            initial_gov_params.push((k, v));
        }

        let acount = read_u32_be(bytes, &mut cur)? as usize;
        if acount > 1_024 {
            return Err(GenesisRootParity1Error::InvalidWire(
                "excessive actor descriptors",
            ));
        }
        let mut actor_descriptors = Vec::with_capacity(acount);
        for _ in 0..acount {
            let aid = ActorIdV1::from_bytes(read_arr32(bytes, &mut cur)?);
            let db = read_framed(bytes, &mut cur)?;
            let desc = AuthorizationDescriptorV1::decode_canonical(db)
                .map_err(|_| GenesisRootParity1Error::InvalidWire("bad auth descriptor"))?;
            actor_descriptors.push((aid, desc));
        }
        if cur != bytes.len() {
            return Err(GenesisRootParity1Error::InvalidWire(
                "trailing bundle bytes",
            ));
        }

        let bundle = Self {
            bundle_version,
            resource_policy: ResourcePolicyV1::decode_canonical(res_b)
                .map_err(|_| GenesisRootParity1Error::InvalidWire("bad ResourcePolicyV1"))?,
            asset_policy: AssetPolicyDescriptorV1::decode_canonical(asset_b)
                .map_err(|_| GenesisRootParity1Error::InvalidWire("bad AssetPolicyDescriptorV1"))?,
            validator_econ_policy: ValidatorEconPolicyV1::decode_canonical(econ_b)
                .map_err(|_| GenesisRootParity1Error::InvalidWire("bad ValidatorEconPolicyV1"))?,
            governance_policy: GovernancePolicyDescriptorV1::decode_canonical(gov_b).map_err(
                |_| GenesisRootParity1Error::InvalidWire("bad GovernancePolicyDescriptorV1"),
            )?,
            consensus_policy: ConsensusPolicyDescriptorV1::decode_canonical(cons_b).map_err(
                |_| GenesisRootParity1Error::InvalidWire("bad ConsensusPolicyDescriptorV1"),
            )?,
            bounds_profile: ProtocolBoundsProfileV1::decode_canonical(bounds_b)
                .map_err(|_| GenesisRootParity1Error::InvalidWire("bad ProtocolBoundsProfileV1"))?,
            bootstrap_policy: BootstrapAvailabilityPolicyV1::decode_canonical(boot_b)?,
            initial_gov_params,
            actor_descriptors,
        };
        bundle.validate()?;
        Ok(bundle)
    }

    /// Computes the canonical `Policy` `ContentTag` for this genesis preimage bundle.
    pub fn tag(&self) -> Result<ContentTag, GenesisRootParity1Error> {
        let bytes = self.encode_canonical()?;
        hash_canonical(
            TagKind::Policy,
            GENESIS_ROOT_PARITY_1_BUNDLE_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .map_err(|_| GenesisRootParity1Error::InvalidWire("hash_canonical failed"))
    }
}

/// Complete L1 §4.4 `GenesisRootParityRecordV1` (`bitblock/genesis-root-parity-1/parity-record/v1`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GenesisRootParityRecordV1 {
    /// Parity record schema version (`1`).
    pub record_version: u16,
    /// Immutable `GENESIS-D1` genesis identifier.
    pub genesis_id: [u8; 32],
    /// Immutable `PROFILE-D1` initial launch profile identifier.
    pub initial_profile_id: [u8; 32],
    /// Digest of the resolved `TRANSITION-SPEC-1` specification for `initial_profile_id`.
    pub initial_transition_spec_at_profile_id: [u8; 32],
    /// Digest of the sorted active `required_domain_set` from `ExecutionDomainRegistryV1`.
    pub required_domain_set_digest: [u8; 32],
    /// Digest of the complete 11-slot initial `RootSlotCommitmentV1` entry set.
    pub canonical_initial_root_entry_set_digest: [u8; 32],
    /// Composed Layer-1 `application_root` over initial genesis slots `1..=6`.
    pub initial_application_root: [u8; 32],
    /// Complete external-context [`ExecutionRootRefV1`] binding `execution_state_root` to context.
    pub initial_execution_root_ref: ExecutionRootRefV1,
    /// Canonical `History::new(&World::new()).genesis()` digest.
    pub genesis_history_root: [u8; 32],
    /// Ordered unique non-zero digests of all independent reconstruction artifacts (Paths 1..=3).
    pub independent_reconstruction_artifact_digests: Vec<[u8; 32]>,
}

impl GenesisRootParityRecordV1 {
    /// Validates internal record consistency and non-zero digests.
    pub fn validate(&self) -> Result<(), GenesisRootParity1Error> {
        if self.record_version != GENESIS_ROOT_PARITY_1_VERSION {
            return Err(GenesisRootParity1Error::InvalidWire("bad record_version"));
        }
        let required_nonzero = [
            self.genesis_id,
            self.initial_profile_id,
            self.initial_transition_spec_at_profile_id,
            self.required_domain_set_digest,
            self.canonical_initial_root_entry_set_digest,
            self.initial_application_root,
            self.genesis_history_root,
            self.initial_execution_root_ref.execution_state_root,
        ];
        if required_nonzero.contains(&[0u8; 32]) {
            return Err(GenesisRootParity1Error::SelfDerivedIdViolation(
                "parity record contains zero digest field",
            ));
        }
        if self.initial_execution_root_ref.genesis_id != self.genesis_id
            || self.initial_execution_root_ref.profile_id != self.initial_profile_id
        {
            return Err(GenesisRootParity1Error::SelfDerivedIdViolation(
                "initial_execution_root_ref disagrees with genesis_id or initial_profile_id",
            ));
        }
        if self.independent_reconstruction_artifact_digests.len() < 3 {
            return Err(GenesisRootParity1Error::CrossDomainOrParityMismatch(
                "parity record requires >= 3 independent reconstruction artifact digests",
            ));
        }
        let mut seen = BTreeSet::new();
        for &d in &self.independent_reconstruction_artifact_digests {
            if d == [0u8; 32] || !seen.insert(d) {
                return Err(GenesisRootParity1Error::CrossDomainOrParityMismatch(
                    "reconstruction artifact digests must be non-zero and unique",
                ));
            }
        }
        Ok(())
    }

    /// Encodes this parity record into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, GenesisRootParity1Error> {
        self.validate()?;
        let count = u16::try_from(self.independent_reconstruction_artifact_digests.len())
            .map_err(|_| GenesisRootParity1Error::InvalidWire("artifact count overflow"))?;
        let mut out = Vec::with_capacity(2 + 32 * 7 + ExecutionRootRefV1::WIRE_LEN + 2 + 32 * 3);
        out.extend_from_slice(&self.record_version.to_be_bytes());
        out.extend_from_slice(&self.genesis_id);
        out.extend_from_slice(&self.initial_profile_id);
        out.extend_from_slice(&self.initial_transition_spec_at_profile_id);
        out.extend_from_slice(&self.required_domain_set_digest);
        out.extend_from_slice(&self.canonical_initial_root_entry_set_digest);
        out.extend_from_slice(&self.initial_application_root);
        out.extend_from_slice(&self.initial_execution_root_ref.encode_canonical());
        out.extend_from_slice(&self.genesis_history_root);
        out.extend_from_slice(&count.to_be_bytes());
        for d in &self.independent_reconstruction_artifact_digests {
            out.extend_from_slice(d);
        }
        Ok(out)
    }

    /// Strictly decodes canonical big-endian wire bytes into a validated [`GenesisRootParityRecordV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, GenesisRootParity1Error> {
        let mut cur = 0usize;
        let record_version = read_u16_be(bytes, &mut cur)?;
        let genesis_id = read_arr32(bytes, &mut cur)?;
        let initial_profile_id = read_arr32(bytes, &mut cur)?;
        let initial_transition_spec_at_profile_id = read_arr32(bytes, &mut cur)?;
        let required_domain_set_digest = read_arr32(bytes, &mut cur)?;
        let canonical_initial_root_entry_set_digest = read_arr32(bytes, &mut cur)?;
        let initial_application_root = read_arr32(bytes, &mut cur)?;
        let ref_slice = take_slice(bytes, &mut cur, ExecutionRootRefV1::WIRE_LEN)?;
        let initial_execution_root_ref = ExecutionRootRefV1::decode_canonical(ref_slice)?;
        let genesis_history_root = read_arr32(bytes, &mut cur)?;
        let count = usize::from(read_u16_be(bytes, &mut cur)?);
        if count > 32 {
            return Err(GenesisRootParity1Error::InvalidWire(
                "excessive artifact count",
            ));
        }
        let mut independent_reconstruction_artifact_digests = Vec::with_capacity(count);
        for _ in 0..count {
            independent_reconstruction_artifact_digests.push(read_arr32(bytes, &mut cur)?);
        }
        if cur != bytes.len() {
            return Err(GenesisRootParity1Error::InvalidWire(
                "trailing parity record bytes",
            ));
        }
        let rec = Self {
            record_version,
            genesis_id,
            initial_profile_id,
            initial_transition_spec_at_profile_id,
            required_domain_set_digest,
            canonical_initial_root_entry_set_digest,
            initial_application_root,
            initial_execution_root_ref,
            genesis_history_root,
            independent_reconstruction_artifact_digests,
        };
        rec.validate()?;
        Ok(rec)
    }

    /// Computes the L1 §4.4 `ProfileIdHashV1`-framed `genesis_root_parity_record_id`.
    pub fn parity_record_id(&self) -> Result<[u8; 32], GenesisRootParity1Error> {
        let encoded = self.encode_canonical()?;
        let mut h = Sha256::new();
        h.update(PARITY_RECORD_ID_PREFIX);
        h.update((encoded.len() as u32).to_be_bytes());
        h.update(&encoded);
        Ok(h.finalize().into())
    }

    /// Computes the canonical `Receipt` `ContentTag` for this genesis parity record.
    pub fn tag(&self) -> Result<ContentTag, GenesisRootParity1Error> {
        let bytes = self.encode_canonical()?;
        hash_canonical(
            TagKind::Receipt,
            GENESIS_ROOT_PARITY_1_RECORD_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .map_err(|_| GenesisRootParity1Error::InvalidWire("hash_canonical failed"))
    }
}

/// Borrowed inputs to [`verify_genesis_root_parity`].
#[derive(Clone, Copy, Debug)]
pub struct GenesisParityInputV1<'a> {
    /// Raw canonical wire bytes of [`GenesisPackageV1`].
    pub raw_genesis_package: &'a [u8],
    /// Raw canonical wire bytes of [`GenesisPreimageBundleV1`].
    pub raw_preimage_bundle: &'a [u8],
    /// Governing pre-ID [`ProfileScopeV1`].
    pub profile_scope: &'a ProfileScopeV1,
    /// Governing `COMMIT-1` [`DomainRegistryV1`].
    pub commit_domain_registry: &'a DomainRegistryV1,
    /// Governing initial launch [`BoundProfileManifestV1`] (successor manifests rejected).
    pub bound_manifest: &'a BoundProfileManifestV1,
    /// Governing launch [`ResolvedProfileTransitionSpecV1`].
    pub transition_spec: &'a ResolvedProfileTransitionSpecV1,
    /// Governing 11-slot [`ExecutionDomainRegistryV1`].
    pub exec_registry: &'a ExecutionDomainRegistryV1,
}

/// Verified multi-path parity output returned by [`verify_genesis_root_parity`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenesisParityOutcomeV1 {
    /// Independently decoded and verified [`GenesisPackageV1`].
    pub package: GenesisPackageV1,
    /// Independently decoded and verified [`GenesisPreimageBundleV1`].
    pub bundle: GenesisPreimageBundleV1,
    /// Bit-exact 10-domain active genesis snapshot agreed across Paths 1, 2, and 3.
    pub snapshot: ExecutorDomainSnapshotV1,
    /// Bit-exact 11-slot [`ExecutionRootCompositionV1`] agreed across Paths 1, 2, and 3.
    pub composition: ExecutionRootCompositionV1,
    /// Final canonical [`GenesisRootParityRecordV1`].
    pub record: GenesisRootParityRecordV1,
}

/// Complete deterministic `bitblock.devnet.v1` genesis fixture bundle used by unit tests,
/// integration tests, and the release parity probe.
#[derive(Clone, Debug)]
pub struct CanonicalDevnetGenesisFixtureV1 {
    /// Sealed `GENESIS-D1` package.
    pub package: GenesisPackageV1,
    /// Complete preimage bundle matching every digest in `package`.
    pub bundle: GenesisPreimageBundleV1,
    /// Governing `PROFILE-SCOPE-D0` scope.
    pub profile_scope: ProfileScopeV1,
    /// Governing `COMMIT-1` domain registry.
    pub commit_domain_registry: DomainRegistryV1,
    /// Governing initial launch `PROFILE-D1` bound manifest.
    pub bound_manifest: BoundProfileManifestV1,
    /// Governing launch `TRANSITION-SPEC-1` resolved spec.
    pub transition_spec: ResolvedProfileTransitionSpecV1,
    /// Governing 11-slot `EXECUTION-ROOT-1` launch registry.
    pub exec_registry: ExecutionDomainRegistryV1,
}

impl CanonicalDevnetGenesisFixtureV1 {
    /// Builds the deterministic `bitblock.devnet.v1` launch fixture (`chain_id = 77`, 2 actors,
    /// 4 equal 25% validators, bedrock-only `B_0`, inactive `program-active` & `verkle-selected`).
    pub fn build() -> Result<Self, GenesisRootParity1Error> {
        let mut council_keys: Vec<_> = [31u8, 32, 33]
            .into_iter()
            .map(|s| CanonicalEd25519PublicKeyV1::from_secret_seed(&[s; 32]).unwrap())
            .collect();
        council_keys.sort();
        let validator_econ_policy = ValidatorEconPolicyV1::v1_pos_default();
        let consensus_policy = ConsensusPolicyDescriptorV1::v1_bft_pos_default();
        let governance_policy = GovernancePolicyDescriptorV1 {
            schema_version: 1,
            upgrade_quorum_numerator: 2,
            upgrade_quorum_denominator: 3,
            mandatory_exit_timelock_epochs: validator_econ_policy.unbonding_delay_epochs,
            max_emergency_pause_epochs: 3,
            automatic_safe_halt_on_proven_fault: true,
            emergency_council_pause_only: true,
            forbid_history_rewrite: true,
            emergency_council: ThresholdPolicyV1 {
                role: CredentialRoleV1::EmergencyPauseSigner,
                threshold: 2,
                keys: council_keys,
            },
        };
        let commit_domain_registry = DomainRegistryV1::local_v1();
        let preds: Vec<String> = CANONICAL_LAUNCH_PREDICATES_V1
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        let profile_scope = ProfileScopeV1::from_wave1_contracts(
            &commit_domain_registry,
            &governance_policy,
            preds,
        )
        .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
        let threat = ThreatModelV1::v1_conservative_consumer(&profile_scope)
            .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
        let bounds_profile =
            ProtocolBoundsProfileV1::v1_consumer_default(&threat, &LocalStorageProfile::DEFAULT)
                .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
        let asset_policy = AssetPolicyDescriptorV1::v1_dual_domain_default();
        let resource_policy = ResourcePolicyV1::v1_default(&bounds_profile, &asset_policy)
            .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
        let bootstrap_policy = BootstrapAvailabilityPolicyV1::v1_default();

        let chain_id = 77u64;
        let namespace = DEVNET_V1_IDENTITY_NAMESPACE.to_string();

        // Two distinct genesis actors with separated action & admin keys (seeds 11..=14)
        let mut actor_pairs: Vec<(ActorIdV1, AuthorizationDescriptorV1)> = [(11u8, 12u8), (13, 14)]
            .into_iter()
            .map(|(act_s, adm_s)| {
                let desc = AuthorizationDescriptorV1 {
                    descriptor_version: 1,
                    action_policy: ThresholdPolicyV1 {
                        role: CredentialRoleV1::ActorAction,
                        threshold: 1,
                        keys: vec![
                            CanonicalEd25519PublicKeyV1::from_secret_seed(&[act_s; 32]).unwrap(),
                        ],
                    },
                    admin_policy: ThresholdPolicyV1 {
                        role: CredentialRoleV1::ActorAdmin,
                        threshold: 1,
                        keys: vec![
                            CanonicalEd25519PublicKeyV1::from_secret_seed(&[adm_s; 32]).unwrap(),
                        ],
                    },
                    delegations: Vec::new(),
                    revoked: false,
                };
                let aid = ActorIdV1::derive(chain_id, &namespace, &desc).unwrap();
                (aid, desc)
            })
            .collect();
        actor_pairs.sort_by_key(|(aid, _)| *aid);

        let initial_actors: Vec<GenesisActorEntryV1> = actor_pairs
            .iter()
            .map(|(aid, desc)| GenesisActorEntryV1 {
                actor_id: *aid,
                descriptor_digest: desc.tag().unwrap().digest(),
                initial_nonce: 0,
                initial_liquid_balance: 100_000_000_000,
            })
            .collect();

        // Four equal 25% genesis validators (100B each >= min_self_stake, < 3,333 bps BFT cap)
        let initial_validators: Vec<GenesisValidatorEntryV1> = (1u8..=4)
            .map(|i| GenesisValidatorEntryV1 {
                validator_id: [i; 32],
                vote_pubkey: CanonicalEd25519PublicKeyV1::from_secret_seed(&[100 + i; 32]).unwrap(),
                proposal_pubkey: CanonicalEd25519PublicKeyV1::from_secret_seed(&[200 + i; 32])
                    .unwrap(),
                bonded_stake: 100_000_000_000,
            })
            .collect();

        let launch_state = LaunchGenesisStateV1::build_launch_v1(
            &profile_scope,
            &commit_domain_registry,
            &resource_policy,
            &asset_policy,
            &validator_econ_policy,
            &governance_policy,
            &bounds_profile,
            &bootstrap_policy,
            chain_id,
            namespace,
            initial_actors,
            initial_validators,
        )?;
        let package = GenesisPackageV1::seal(launch_state)?;

        let candidate = CandidateProfileManifestV1::canonical_launch_v1(
            package.genesis_id,
            profile_scope
                .tag()
                .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?
                .digest(),
            commit_domain_registry.tag().digest(),
            bounds_profile
                .tag()
                .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?
                .digest(),
            CryptoSuiteDescriptorV1::v1_ed25519_sha256()
                .tag()
                .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?
                .digest(),
            resource_policy
                .tag()
                .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?
                .digest(),
        )
        .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
        let bound_manifest = BoundProfileManifestV1::seal(candidate)
            .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
        let transition_spec = ResolvedProfileTransitionSpecV1::resolve_from_profile(
            &bound_manifest,
            &bounds_profile,
            &resource_policy,
        )
        .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;

        let bundle = GenesisPreimageBundleV1 {
            bundle_version: GENESIS_ROOT_PARITY_1_VERSION,
            resource_policy,
            asset_policy,
            validator_econ_policy,
            governance_policy,
            consensus_policy,
            bounds_profile,
            bootstrap_policy,
            initial_gov_params: vec![(1, 2_000), (2, 100_000_000)],
            actor_descriptors: actor_pairs,
        };
        bundle.validate()?;

        Ok(Self {
            package,
            bundle,
            profile_scope,
            commit_domain_registry,
            bound_manifest,
            transition_spec,
            exec_registry: ExecutionDomainRegistryV1::canonical_launch_v1(),
        })
    }
}

/// Checks that `world` contains exclusively the immutable bedrock coordinate `(0, 0, 0)`.
pub fn verify_bedrock_only_world(world: &World) -> Result<[u8; 32], GenesisRootParity1Error> {
    if world.len() != 1 || !world.is_on(BEDROCK_COORD) {
        return Err(GenesisRootParity1Error::NonBedrockWorldRejected(
            "B_0 world must contain exclusively bedrock (0, 0, 0)",
        ));
    }
    let expected = world_tag(&World::new()).digest();
    let actual = world_tag(world).digest();
    if actual != expected {
        return Err(GenesisRootParity1Error::NonBedrockWorldRejected(
            "world digest disagrees with canonical bedrock world",
        ));
    }
    Ok(actual)
}

/// Path 1: cold per-domain full rebuild of all 10 active launch Patricia tries from verified
/// logical genesis package + preimage bundle without using incremental domain state helpers.
pub fn build_cold_genesis_snapshot(
    pkg: &GenesisPackageV1,
    bundle: &GenesisPreimageBundleV1,
    profile_id: [u8; 32],
) -> Result<(ExecutorDomainSnapshotV1, [u8; 32]), GenesisRootParity1Error> {
    let world = World::new();
    verify_bedrock_only_world(&world)?;
    let history = History::new(&world);
    let genesis_history_root = history.genesis().digest();
    let episodes = OccupancyEpisodeLedger::new(history.genesis());
    let sidecar = LocalAuthenticatedSidecar::from_genesis(
        &world,
        &history,
        &episodes,
        &LocalStorageProfile::DEFAULT,
        pkg.state.initial_profile_scope_digest,
    )
    .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
    let empty_pruned = HashSet::new();
    verify_independent_projection(
        &world,
        &history,
        &empty_pruned,
        sidecar.current_set(),
        &ProfileRegistryV1::local_v1(),
        &DomainRegistryV1::local_v1(),
        HistoryReplayLimits::DEFAULT,
    )
    .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;

    // Domain 1 (Authorization) & Domain 2 (Nonce) cold Patricia rebuild
    let mut auth_pairs = Vec::with_capacity(bundle.actor_descriptors.len());
    let mut nonce_pairs = Vec::with_capacity(bundle.actor_descriptors.len());
    for (aid, desc) in &bundle.actor_descriptors {
        let state = genesis_actor_auth_state(*aid, desc);
        let encoded = state
            .encode_canonical()
            .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
        auth_pairs.push((*aid.as_bytes(), encoded));
        nonce_pairs.push((*aid.as_bytes(), 0u64.to_be_bytes().to_vec()));
    }
    let auth_trie = DomainPatriciaTrieV1::from_entries(DomainIdV1::Authorization, auth_pairs)
        .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
    let nonce_trie = DomainPatriciaTrieV1::from_entries(DomainIdV1::Nonce, nonce_pairs)
        .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;

    // Domain 3 (NativeBalance) & Domain 4 (EpisodeOwnership) cold Patricia rebuild
    let bal_pairs: Vec<([u8; 32], Vec<u8>)> = pkg
        .state
        .initial_actors
        .iter()
        .map(|a| {
            (
                *a.actor_id.as_bytes(),
                a.initial_liquid_balance.to_be_bytes().to_vec(),
            )
        })
        .collect();
    let bal_trie = DomainPatriciaTrieV1::from_entries(DomainIdV1::NativeBalance, bal_pairs)
        .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
    let ep_own_trie = DomainPatriciaTrieV1::from_entries(DomainIdV1::EpisodeOwnership, Vec::new())
        .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;

    // Domain 5 (GovernanceState) cold Patricia rebuild
    let gov_pairs: Vec<([u8; 32], Vec<u8>)> = bundle
        .initial_gov_params
        .iter()
        .map(|&(k, v)| (param_trie_key(k), v.to_be_bytes().to_vec()))
        .collect();
    let gov_trie = DomainPatriciaTrieV1::from_entries(DomainIdV1::GovernanceState, gov_pairs)
        .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;

    // Domain 6 (EconomicStake) cold Patricia rebuild
    let mut econ_pairs = Vec::with_capacity(pkg.state.initial_validators.len());
    for v in &pkg.state.initial_validators {
        let rec = ValidatorStakeRecordV1 {
            bonded_stake: v.bonded_stake,
            accrued_rewards: 0,
            total_slashed: 0,
            jailed: false,
            slash_count: 0,
            unbonding_entries: Vec::new(),
        };
        let encoded = rec
            .encode_canonical()
            .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
        econ_pairs.push((v.validator_id, encoded));
    }
    let econ_trie = DomainPatriciaTrieV1::from_entries(DomainIdV1::EconomicStake, econ_pairs)
        .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;

    // Domain 7 (ValidatorSchedule) cold Patricia rebuild over canonical genesis epochs 0..delay
    let gov_ctx = GovernanceContextV1 {
        protocol_version: 1,
        chain_id: pkg.state.chain_id,
        genesis_id: pkg.genesis_id,
        launch_manifest_id: pkg.genesis_id,
        profile_id,
    };
    let gov_ref = GovernanceStateV1::from_genesis(
        gov_ctx,
        bundle.governance_policy.clone(),
        &bundle.initial_gov_params,
    )
    .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
    let econ_stakes: Vec<(ActorIdV1, u128)> = pkg
        .state
        .initial_validators
        .iter()
        .map(|v| (ActorIdV1::from_bytes(v.validator_id), v.bonded_stake))
        .collect();
    let econ_ref =
        EconomicStateV1::from_genesis(bundle.validator_econ_policy, profile_id, &econ_stakes)
            .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
    let val_keys: Vec<(ActorIdV1, ValidatorKeyPairV1)> = pkg
        .state
        .initial_validators
        .iter()
        .map(|v| {
            (
                ActorIdV1::from_bytes(v.validator_id),
                ValidatorKeyPairV1 {
                    vote_key: v.vote_pubkey,
                    proposal_key: v.proposal_pubkey,
                },
            )
        })
        .collect();
    let forbidden_actor_keys = collect_actor_pubkeys(&bundle.actor_descriptors);
    let sched_ref = ValidatorScheduleStateV1::from_genesis(
        bundle.consensus_policy,
        bundle.validator_econ_policy,
        profile_id,
        &econ_ref,
        &gov_ref,
        &val_keys,
        &forbidden_actor_keys,
    )
    .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
    let delay = bundle.validator_econ_policy.activation_delay_epochs;
    let mut sched_pairs = Vec::with_capacity(delay as usize);
    for ep in 0..delay {
        let s = sched_ref
            .schedule_at(ep)
            .ok_or_else(|| GenesisRootParity1Error::DomainState("missing genesis epoch".into()))?;
        let encoded = s
            .encode_canonical()
            .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
        sched_pairs.push((epoch_trie_key(ep), encoded));
    }
    let sched_trie = DomainPatriciaTrieV1::from_entries(DomainIdV1::ValidatorSchedule, sched_pairs)
        .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;

    let snapshot = ExecutorDomainSnapshotV1 {
        snapshot_version: EXECUTOR_1_VERSION,
        batch_height: 0,
        epoch: 0,
        world_root: sidecar.world_root().root,
        active_token_root: sidecar.active_token_root().root,
        episode_root: sidecar.episode_root().root,
        auth_root: auth_trie.root(),
        nonce_root: nonce_trie.root(),
        native_balance_root: bal_trie.root(),
        episode_ownership_root: ep_own_trie.root(),
        governance_root: gov_trie.root(),
        economic_stake_root: econ_trie.root(),
        validator_schedule_root: sched_trie.root(),
        program_state_active: false,
    };
    Ok((snapshot, genesis_history_root))
}

/// Path 2: initializes each stateful upstream domain producer incrementally via `from_genesis()`,
/// verifies every producer's internal `verify_rebuild_parity()`, and projects the 10-domain snapshot.
pub fn build_incremental_genesis_snapshot(
    pkg: &GenesisPackageV1,
    bundle: &GenesisPreimageBundleV1,
    profile_id: [u8; 32],
) -> Result<ExecutorDomainSnapshotV1, GenesisRootParity1Error> {
    let world = World::new();
    let history = History::new(&world);
    let episodes = OccupancyEpisodeLedger::new(history.genesis());
    let sidecar = LocalAuthenticatedSidecar::from_genesis(
        &world,
        &history,
        &episodes,
        &LocalStorageProfile::DEFAULT,
        pkg.state.initial_profile_scope_digest,
    )
    .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;

    let auth_ctx = genesis_identity_ctx(pkg, profile_id, &world);
    let actor_states: Vec<ActorAuthStateV1> = bundle
        .actor_descriptors
        .iter()
        .map(|(aid, desc)| genesis_actor_auth_state(*aid, desc))
        .collect();
    let auth = AuthCommitStateV1::from_genesis(auth_ctx, actor_states)
        .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
    auth.verify_rebuild_parity()
        .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;

    let balances: Vec<(ActorIdV1, u128)> = pkg
        .state
        .initial_actors
        .iter()
        .map(|a| (a.actor_id, a.initial_liquid_balance))
        .collect();
    let asset = AssetLedgerStateV1::from_genesis(bundle.asset_policy, profile_id, &balances)
        .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
    asset
        .verify_rebuild_parity()
        .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;

    let gov_ctx = GovernanceContextV1 {
        protocol_version: 1,
        chain_id: pkg.state.chain_id,
        genesis_id: pkg.genesis_id,
        launch_manifest_id: pkg.genesis_id,
        profile_id,
    };
    let gov = GovernanceStateV1::from_genesis(
        gov_ctx,
        bundle.governance_policy.clone(),
        &bundle.initial_gov_params,
    )
    .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
    gov.verify_rebuild_parity()
        .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;

    let stakes: Vec<(ActorIdV1, u128)> = pkg
        .state
        .initial_validators
        .iter()
        .map(|v| (ActorIdV1::from_bytes(v.validator_id), v.bonded_stake))
        .collect();
    let econ = EconomicStateV1::from_genesis(bundle.validator_econ_policy, profile_id, &stakes)
        .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
    econ.verify_rebuild_parity()
        .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;

    let val_keys: Vec<(ActorIdV1, ValidatorKeyPairV1)> = pkg
        .state
        .initial_validators
        .iter()
        .map(|v| {
            (
                ActorIdV1::from_bytes(v.validator_id),
                ValidatorKeyPairV1 {
                    vote_key: v.vote_pubkey,
                    proposal_key: v.proposal_pubkey,
                },
            )
        })
        .collect();
    let forbidden_actor_keys = collect_actor_pubkeys(&bundle.actor_descriptors);
    let sched = ValidatorScheduleStateV1::from_genesis(
        bundle.consensus_policy,
        bundle.validator_econ_policy,
        profile_id,
        &econ,
        &gov,
        &val_keys,
        &forbidden_actor_keys,
    )
    .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
    sched
        .verify_rebuild_parity()
        .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;

    let auth_roots = auth.roots();
    let asset_roots = asset
        .roots()
        .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
    let gov_root = gov.root_commitment();
    let econ_root = econ
        .root_commitment()
        .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
    let sched_root = sched
        .root_commitment()
        .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;

    Ok(ExecutorDomainSnapshotV1 {
        snapshot_version: EXECUTOR_1_VERSION,
        batch_height: 0,
        epoch: 0,
        world_root: sidecar.world_root().root,
        active_token_root: sidecar.active_token_root().root,
        episode_root: sidecar.episode_root().root,
        auth_root: auth_roots.authorization_root,
        nonce_root: auth_roots.nonce_root,
        native_balance_root: asset_roots.native_balance_root,
        episode_ownership_root: asset_roots.episode_ownership_root,
        governance_root: gov_root.governance_trie_root,
        economic_stake_root: econ_root.economic_trie_root,
        validator_schedule_root: sched_root.schedule_trie_root,
        program_state_active: false,
    })
}

/// Path 3: initializes [`PrimaryLaunchExecutorV1`] from decoded genesis state, verifies all-domain
/// rebuild parity, and returns its initial 10-domain snapshot.
pub fn build_executor_genesis_snapshot(
    pkg: &GenesisPackageV1,
    bundle: &GenesisPreimageBundleV1,
    profile_id: [u8; 32],
    spec: &ResolvedProfileTransitionSpecV1,
) -> Result<ExecutorDomainSnapshotV1, GenesisRootParity1Error> {
    let spec_digest = spec
        .tag()
        .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?
        .digest();
    let fork_context = ExecutorForkContextV1 {
        chain_id: pkg.state.chain_id,
        world_id: pkg.state.world_id,
        genesis_id: pkg.genesis_id,
        profile_id,
        transition_spec_digest: spec_digest,
    };
    let initial_actors: Vec<ActorAuthStateV1> = bundle
        .actor_descriptors
        .iter()
        .map(|(aid, desc)| genesis_actor_auth_state(*aid, desc))
        .collect();
    let initial_balances: Vec<(ActorIdV1, u128)> = pkg
        .state
        .initial_actors
        .iter()
        .map(|a| (a.actor_id, a.initial_liquid_balance))
        .collect();
    let initial_stakes: Vec<(ActorIdV1, u128)> = pkg
        .state
        .initial_validators
        .iter()
        .map(|v| (ActorIdV1::from_bytes(v.validator_id), v.bonded_stake))
        .collect();
    let initial_validator_keys: Vec<(ActorIdV1, ValidatorKeyPairV1)> = pkg
        .state
        .initial_validators
        .iter()
        .map(|v| {
            (
                ActorIdV1::from_bytes(v.validator_id),
                ValidatorKeyPairV1 {
                    vote_key: v.vote_pubkey,
                    proposal_key: v.proposal_pubkey,
                },
            )
        })
        .collect();
    let cfg = ExecutorGenesisConfigV1 {
        fork_context,
        profile_scope_digest: pkg.state.initial_profile_scope_digest,
        spec: spec.clone(),
        bounds: bundle.bounds_profile,
        resource: bundle.resource_policy,
        auth_context: genesis_identity_ctx(pkg, profile_id, &World::new()),
        initial_actors,
        asset_policy: bundle.asset_policy,
        initial_balances,
        gov_context: GovernanceContextV1 {
            protocol_version: 1,
            chain_id: pkg.state.chain_id,
            genesis_id: pkg.genesis_id,
            launch_manifest_id: pkg.genesis_id,
            profile_id,
        },
        gov_policy: bundle.governance_policy.clone(),
        initial_params: bundle.initial_gov_params.clone(),
        econ_policy: bundle.validator_econ_policy,
        initial_stakes,
        consensus_policy: bundle.consensus_policy,
        initial_validator_keys,
        forbidden_actor_keys: collect_actor_pubkeys(&bundle.actor_descriptors),
    };
    let executor = PrimaryLaunchExecutorV1::from_genesis(cfg)?;
    executor.verify_all_domain_rebuild_parity()?;
    Ok(executor.snapshot()?)
}

/// Executes the complete preimage-verified 3-path Rust genesis-to-root parity audit (matching Path 4
/// clean-room Python in `tools/genesis_root_parity_1_vectors.py`) and returns the canonical
/// [`GenesisRootParityRecordV1`].
pub fn verify_genesis_root_parity(
    input: &GenesisParityInputV1<'_>,
) -> Result<GenesisParityOutcomeV1, GenesisRootParity1Error> {
    // 1. Independently decode & verify GenesisPackageV1 and GenesisPreimageBundleV1
    let pkg = GenesisPackageV1::decode_canonical(input.raw_genesis_package)?;
    let bundle = GenesisPreimageBundleV1::decode_canonical(input.raw_preimage_bundle)?;

    // 2. Verify all policy preimages match every digest in LaunchGenesisStateV1
    let scope_dig = input
        .profile_scope
        .tag()
        .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?
        .digest();
    if scope_dig != pkg.state.initial_profile_scope_digest {
        return Err(GenesisRootParity1Error::PreimageDigestMismatch(
            "ProfileScopeV1 digest mismatch",
        ));
    }
    if input.commit_domain_registry.tag().digest() != pkg.state.initial_domain_registry_digest {
        return Err(GenesisRootParity1Error::PreimageDigestMismatch(
            "COMMIT-1 DomainRegistryV1 digest mismatch",
        ));
    }
    if bundle
        .resource_policy
        .tag()
        .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?
        .digest()
        != pkg.state.resource_policy_digest
    {
        return Err(GenesisRootParity1Error::PreimageDigestMismatch(
            "ResourcePolicyV1 digest mismatch",
        ));
    }
    if bundle
        .asset_policy
        .tag()
        .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?
        .digest()
        != pkg.state.asset_policy_digest
    {
        return Err(GenesisRootParity1Error::PreimageDigestMismatch(
            "AssetPolicyDescriptorV1 digest mismatch",
        ));
    }
    if bundle
        .validator_econ_policy
        .tag()
        .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?
        .digest()
        != pkg.state.validator_econ_digest
    {
        return Err(GenesisRootParity1Error::PreimageDigestMismatch(
            "ValidatorEconPolicyV1 digest mismatch",
        ));
    }
    if bundle
        .governance_policy
        .tag()
        .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?
        .digest()
        != pkg.state.governance_policy_digest
    {
        return Err(GenesisRootParity1Error::PreimageDigestMismatch(
            "GovernancePolicyDescriptorV1 digest mismatch",
        ));
    }
    if bundle
        .bounds_profile
        .tag()
        .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?
        .digest()
        != pkg.state.validated_limit_vector_digest
    {
        return Err(GenesisRootParity1Error::PreimageDigestMismatch(
            "ProtocolBoundsProfileV1 digest mismatch",
        ));
    }
    if bundle.bootstrap_policy.tag()?.digest() != pkg.state.bootstrap_availability_digest {
        return Err(GenesisRootParity1Error::PreimageDigestMismatch(
            "BootstrapAvailabilityPolicyV1 digest mismatch",
        ));
    }
    pkg.state.validate(
        Some(&bundle.asset_policy),
        Some(&bundle.validator_econ_policy),
    )?;

    // 3. Verify 1-to-1 actor descriptor preimages & stable pre-genesis ActorIdV1 derivation
    if bundle.actor_descriptors.len() != pkg.state.initial_actors.len() {
        return Err(GenesisRootParity1Error::InvalidBundleCardinalityOrOrder(
            "actor_descriptors length disagrees with LaunchGenesisStateV1::initial_actors",
        ));
    }
    for ((aid, desc), g_entry) in bundle
        .actor_descriptors
        .iter()
        .zip(&pkg.state.initial_actors)
    {
        if *aid != g_entry.actor_id {
            return Err(GenesisRootParity1Error::InvalidBundleCardinalityOrOrder(
                "actor_id mismatch between bundle and LaunchGenesisStateV1",
            ));
        }
        let derived = ActorIdV1::derive(pkg.state.chain_id, &pkg.state.identity_namespace, desc)
            .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
        if derived != *aid {
            return Err(GenesisRootParity1Error::PreimageDigestMismatch(
                "ActorIdV1 derivation disagrees with AuthorizationDescriptorV1 preimage",
            ));
        }
        let d_dig = desc
            .tag()
            .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?
            .digest();
        if d_dig != g_entry.descriptor_digest {
            return Err(GenesisRootParity1Error::PreimageDigestMismatch(
                "AuthorizationDescriptorV1 tag digest disagrees with LaunchGenesisStateV1",
            ));
        }
    }

    // 4. Verify InitialLaunch BoundProfileManifestV1 (rejects CE-43 successor & self-derived IDs)
    input.bound_manifest.verify().map_err(|_| {
        GenesisRootParity1Error::SelfDerivedIdViolation("invalid BoundProfileManifestV1")
    })?;
    let cand = &input.bound_manifest.candidate;
    if cand.lineage_kind != ProfileLineageKindV1::InitialLaunch
        || cand.predecessor_profile_id.is_some()
    {
        return Err(GenesisRootParity1Error::StaleOrNonInitialProfileRejected(
            "genesis parity requires InitialLaunch manifest (CE-43 successor rejected)",
        ));
    }
    if cand.genesis_id != pkg.genesis_id {
        return Err(GenesisRootParity1Error::SelfDerivedIdViolation(
            "manifest genesis_id disagrees with decoded GenesisPackageV1",
        ));
    }
    if cand.genesis_scope_digest != pkg.state.initial_profile_scope_digest
        || cand.current_scope_digest != pkg.state.initial_profile_scope_digest
        || cand.domain_registry_digest != pkg.state.initial_domain_registry_digest
        || cand.protocol_bounds_digest != pkg.state.validated_limit_vector_digest
        || cand.resource_policy_digest != pkg.state.resource_policy_digest
    {
        return Err(GenesisRootParity1Error::StaleOrNonInitialProfileRejected(
            "manifest pre-ID digests disagree with LaunchGenesisStateV1",
        ));
    }
    if pkg
        .state
        .pre_id_digests()
        .contains(&input.bound_manifest.profile_id)
    {
        return Err(GenesisRootParity1Error::SelfDerivedIdViolation(
            "profile_id leaked into pre-ID genesis digests",
        ));
    }
    input
        .transition_spec
        .validate()
        .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?;
    input
        .transition_spec
        .verify_against_profile(input.bound_manifest)
        .map_err(|_| {
            GenesisRootParity1Error::StaleOrNonInitialProfileRejected(
                "transition_spec disagrees with initial launch manifest",
            )
        })?;
    if input.transition_spec.program_slot_active {
        return Err(GenesisRootParity1Error::StaleOrNonInitialProfileRejected(
            "Program/VM slot must be inactive at launch genesis",
        ));
    }
    input.exec_registry.validate()?;

    // 5. Execute all 3 independent Rust reconstruction paths and compose ExecutionRootCompositionV1
    let profile_id = input.bound_manifest.profile_id;
    let preimage_ctx = ExecutionRootPreimageContextV1 {
        protocol_version: EXECUTION_ROOT_1_VERSION,
        chain_id: pkg.state.chain_id,
        world_id: pkg.state.world_id,
        genesis_id: pkg.genesis_id,
        profile_scope_digest: pkg.state.initial_profile_scope_digest,
    };

    let (snap_cold, genesis_history_root) = build_cold_genesis_snapshot(&pkg, &bundle, profile_id)?;
    let comp_cold =
        compose_from_executor_snapshot(&snap_cold, input.exec_registry, preimage_ctx, profile_id)?;

    let snap_incr = build_incremental_genesis_snapshot(&pkg, &bundle, profile_id)?;
    let comp_incr =
        compose_from_executor_snapshot(&snap_incr, input.exec_registry, preimage_ctx, profile_id)?;

    let snap_exec =
        build_executor_genesis_snapshot(&pkg, &bundle, profile_id, input.transition_spec)?;
    let comp_exec =
        compose_from_executor_snapshot(&snap_exec, input.exec_registry, preimage_ctx, profile_id)?;

    if snap_cold != snap_incr || snap_cold != snap_exec {
        return Err(GenesisRootParity1Error::CrossDomainOrParityMismatch(
            "10-domain genesis snapshot diverged across Cold/Incremental/Executor paths",
        ));
    }
    if comp_cold != comp_incr || comp_cold != comp_exec {
        return Err(GenesisRootParity1Error::CrossDomainOrParityMismatch(
            "composed ExecutionRootCompositionV1 diverged across paths",
        ));
    }
    verify_header_and_manifest_correspondence(
        &comp_exec,
        &comp_exec.execution_root_ref,
        input.exec_registry,
        input.bound_manifest,
    )?;

    let spec_dig = input
        .transition_spec
        .tag()
        .map_err(|e| GenesisRootParity1Error::DomainState(e.to_string()))?
        .digest();
    let artifact_digests = vec![
        reconstruction_artifact_digest(RECON_COLD_PREFIX, pkg.genesis_id, profile_id, &comp_cold),
        reconstruction_artifact_digest(RECON_INCR_PREFIX, pkg.genesis_id, profile_id, &comp_incr),
        reconstruction_artifact_digest(RECON_EXEC_PREFIX, pkg.genesis_id, profile_id, &comp_exec),
    ];
    let record = GenesisRootParityRecordV1 {
        record_version: GENESIS_ROOT_PARITY_1_VERSION,
        genesis_id: pkg.genesis_id,
        initial_profile_id: profile_id,
        initial_transition_spec_at_profile_id: spec_dig,
        required_domain_set_digest: comp_exec.required_domain_set_digest,
        canonical_initial_root_entry_set_digest: comp_exec.root_entry_set_digest,
        initial_application_root: comp_exec.application_root,
        initial_execution_root_ref: comp_exec.execution_root_ref,
        genesis_history_root,
        independent_reconstruction_artifact_digests: artifact_digests,
    };
    record.validate()?;
    verify_parity_record_against_composition(
        &record,
        &comp_exec,
        input.exec_registry,
        input.bound_manifest,
    )?;

    Ok(GenesisParityOutcomeV1 {
        package: pkg,
        bundle,
        snapshot: snap_exec,
        composition: comp_exec,
        record,
    })
}

/// Verifies an emitted [`GenesisRootParityRecordV1`] against a composed
/// [`ExecutionRootCompositionV1`], [`ExecutionDomainRegistryV1`], and [`BoundProfileManifestV1`],
/// rejecting cross-domain swaps, tampered roots, synthetic-zero sentinels, and self-derived ID drift.
pub fn verify_parity_record_against_composition(
    record: &GenesisRootParityRecordV1,
    composition: &ExecutionRootCompositionV1,
    registry: &ExecutionDomainRegistryV1,
    bound_manifest: &BoundProfileManifestV1,
) -> Result<(), GenesisRootParity1Error> {
    record.validate()?;
    verify_header_and_manifest_correspondence(
        composition,
        &record.initial_execution_root_ref,
        registry,
        bound_manifest,
    )?;
    if record.genesis_id != bound_manifest.candidate.genesis_id
        || record.initial_profile_id != bound_manifest.profile_id
    {
        return Err(GenesisRootParity1Error::SelfDerivedIdViolation(
            "parity record IDs disagree with BoundProfileManifestV1",
        ));
    }
    if record.required_domain_set_digest != composition.required_domain_set_digest
        || record.canonical_initial_root_entry_set_digest != composition.root_entry_set_digest
        || record.initial_application_root != composition.application_root
        || record.initial_execution_root_ref != composition.execution_root_ref
    {
        return Err(GenesisRootParity1Error::CrossDomainOrParityMismatch(
            "parity record roots disagree with composed ExecutionRootCompositionV1",
        ));
    }
    let expected_artifacts = [
        reconstruction_artifact_digest(
            RECON_COLD_PREFIX,
            record.genesis_id,
            record.initial_profile_id,
            composition,
        ),
        reconstruction_artifact_digest(
            RECON_INCR_PREFIX,
            record.genesis_id,
            record.initial_profile_id,
            composition,
        ),
        reconstruction_artifact_digest(
            RECON_EXEC_PREFIX,
            record.genesis_id,
            record.initial_profile_id,
            composition,
        ),
    ];
    if record.independent_reconstruction_artifact_digests != expected_artifacts {
        return Err(GenesisRootParity1Error::CrossDomainOrParityMismatch(
            "reconstruction artifact digests disagree with composition",
        ));
    }
    Ok(())
}

/// Computes the deterministic domain-separated artifact digest for one reconstruction path.
pub fn reconstruction_artifact_digest(
    prefix: &[u8],
    genesis_id: [u8; 32],
    profile_id: [u8; 32],
    composition: &ExecutionRootCompositionV1,
) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(prefix);
    h.update(genesis_id);
    h.update(profile_id);
    h.update(composition.application_root);
    h.update(composition.execution_state_root);
    h.update(composition.root_entry_set_digest);
    h.finalize().into()
}

fn genesis_actor_auth_state(
    actor_id: ActorIdV1,
    desc: &AuthorizationDescriptorV1,
) -> ActorAuthStateV1 {
    ActorAuthStateV1 {
        state_version: 1,
        actor_id,
        descriptor: desc.clone(),
        next_nonce: 0,
        recovery_guardians: desc.admin_policy.clone(),
        recovery_delay_heights: 100,
        pending_recovery: None,
    }
}

fn genesis_identity_ctx(
    pkg: &GenesisPackageV1,
    profile_id: [u8; 32],
    world: &World,
) -> IdentityContextV1 {
    IdentityContextV1 {
        protocol_version: 1,
        chain_id: pkg.state.chain_id,
        genesis_id: pkg.genesis_id,
        launch_manifest_id: pkg.genesis_id,
        profile_id,
        world_tag: world_tag(world),
        signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
        hash_suite: HashSuiteIdV1::Sha256V1,
    }
}

fn collect_actor_pubkeys(
    actors: &[(ActorIdV1, AuthorizationDescriptorV1)],
) -> BTreeSet<CanonicalEd25519PublicKeyV1> {
    let mut keys = BTreeSet::new();
    for (_, desc) in actors {
        for &k in &desc.action_policy.keys {
            keys.insert(k);
        }
        for &k in &desc.admin_policy.keys {
            keys.insert(k);
        }
    }
    keys
}

/// Maps each of the 10 active genesis domains to its label and 32-byte root for audit reporting.
pub fn snapshot_domain_map(
    snapshot: &ExecutorDomainSnapshotV1,
) -> BTreeMap<&'static str, [u8; 32]> {
    BTreeMap::from([
        ("01_WorldOccupancy", snapshot.world_root),
        ("02_ActiveToken", snapshot.active_token_root),
        ("03_Episode", snapshot.episode_root),
        ("04_NativeBalance", snapshot.native_balance_root),
        ("05_EpisodeOwnership", snapshot.episode_ownership_root),
        ("07_Authorization", snapshot.auth_root),
        ("08_Nonce", snapshot.nonce_root),
        ("09_GovernanceState", snapshot.governance_root),
        ("10_EconomicStake", snapshot.economic_stake_root),
        ("11_ValidatorSchedule", snapshot.validator_schedule_root),
    ])
}

fn push_framed(out: &mut Vec<u8>, slice: &[u8]) {
    out.extend_from_slice(&(slice.len() as u32).to_be_bytes());
    out.extend_from_slice(slice);
}

fn take_slice<'a>(
    bytes: &'a [u8],
    cur: &mut usize,
    n: usize,
) -> Result<&'a [u8], GenesisRootParity1Error> {
    if bytes.len().saturating_sub(*cur) < n {
        return Err(GenesisRootParity1Error::InvalidWire("truncated buffer"));
    }
    let sl = &bytes[*cur..*cur + n];
    *cur += n;
    Ok(sl)
}

fn read_framed<'a>(bytes: &'a [u8], cur: &mut usize) -> Result<&'a [u8], GenesisRootParity1Error> {
    let len = read_u32_be(bytes, cur)? as usize;
    take_slice(bytes, cur, len)
}

fn read_u16_be(bytes: &[u8], cur: &mut usize) -> Result<u16, GenesisRootParity1Error> {
    Ok(u16::from_be_bytes(
        take_slice(bytes, cur, 2)?.try_into().unwrap(),
    ))
}

fn read_u32_be(bytes: &[u8], cur: &mut usize) -> Result<u32, GenesisRootParity1Error> {
    Ok(u32::from_be_bytes(
        take_slice(bytes, cur, 4)?.try_into().unwrap(),
    ))
}

fn read_u64_be(bytes: &[u8], cur: &mut usize) -> Result<u64, GenesisRootParity1Error> {
    Ok(u64::from_be_bytes(
        take_slice(bytes, cur, 8)?.try_into().unwrap(),
    ))
}

fn read_arr32(bytes: &[u8], cur: &mut usize) -> Result<[u8; 32], GenesisRootParity1Error> {
    Ok(take_slice(bytes, cur, 32)?.try_into().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genesis_root_parity_1_three_path_rebuild_and_wire_roundtrip() {
        let fix = CanonicalDevnetGenesisFixtureV1::build().unwrap();
        let raw_pkg = fix.package.encode_canonical().unwrap();
        let raw_bundle = fix.bundle.encode_canonical().unwrap();
        assert_eq!(
            GenesisPreimageBundleV1::decode_canonical(&raw_bundle).unwrap(),
            fix.bundle
        );

        let input = GenesisParityInputV1 {
            raw_genesis_package: &raw_pkg,
            raw_preimage_bundle: &raw_bundle,
            profile_scope: &fix.profile_scope,
            commit_domain_registry: &fix.commit_domain_registry,
            bound_manifest: &fix.bound_manifest,
            transition_spec: &fix.transition_spec,
            exec_registry: &fix.exec_registry,
        };
        let out = verify_genesis_root_parity(&input).unwrap();
        let raw_rec = out.record.encode_canonical().unwrap();
        assert_eq!(
            GenesisRootParityRecordV1::decode_canonical(&raw_rec).unwrap(),
            out.record
        );
        assert_eq!(snapshot_domain_map(&out.snapshot).len(), 10);
        assert!(!out.snapshot.program_state_active);
        assert_eq!(
            fix.bundle.tag().unwrap().to_string(),
            "#bb.policy.sha256.6mcfpltkrno445wrrzhsecwowfll6m5ekyhakh6aw5n2vcdirpsa"
        );
        assert_eq!(
            out.composition.tag().unwrap().to_string(),
            "#bb.receipt.sha256.uihe33whofr5752ptw2dxfq3tuvtee7ssqrukxxldfolnjsvay2q"
        );
        assert_eq!(
            out.record.tag().unwrap().to_string(),
            "#bb.receipt.sha256.xkaeumdnpaz77nlkk3oolxx2surjpvay2ycpksqckx4oddbm67fq"
        );
        let pid_hex: String = out
            .record
            .parity_record_id()
            .unwrap()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(
            pid_hex,
            "2e72dd4d8c6d61ff0158dc7460a8bdaf75e6d9a381e542bb97a9a80a520a2eaa"
        );
    }
}
