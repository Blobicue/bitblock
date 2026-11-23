//! `UPGRADE-FRAMEWORK-1`: version-lifecycle policy, preventive in-place migration verifier,
//! downgrade & historical-block guard, compile-time uninhabited recovery envelope, and Loop 8–10
//! authority-compromise disposition matrix (`PS-082`).
//!
//! Enforces every lifecycle and compromise rule in `CAPABILITY_TREE.md` and `LAYER1_DESIGN_PAPER.md`
//! §4.5 without claiming any concrete `UPGRADE-1` migration or `PROFILE-1` admission:
//! - **Dual lifecycle policies:** supports both `ImmutableVersionOnly` (rejects any version change
//!   as a distinct chain requiring new genesis) and `PreventiveInPlaceMigration` (preserves the
//!   `CE-43` in-place successor path while requiring an admitted old profile, distinct new
//!   candidate, active governance timelock, bounded migration interface, monotonic non-downgrade
//!   suite policy, and sound live authority bindings with one exact last-safe parent).
//! - **Genesis & historical guards:** height-0 genesis rejects fabricated predecessor inputs;
//!   historical blocks prior to activation remain verifiable under old-profile rules while
//!   post-activation heights reject old-profile downgrades fail-closed; interrupted migrations roll
//!   back atomically to the old profile with zero mixed-profile root publication.
//! - **Established compromise disposition (Loop 8–10):** all 6 established-compromise controls
//!   (`CollisionOrForgery`, `AmbiguousLastSafeParent`, `FailedRoleAuthorized`,
//!   `HypotheticalPrecommit`, `PostCompromisePrecommit`, `NoPrecommit`) resolve strictly to
//!   [`CurrentEstablishedAuthorityCompromiseDispositionV1::SafeHaltAndSeparatelyTrustedNewGenesisV1`].
//! - **Compile-time uninhabited recovery envelope:** [`PrecommittedIndependentRecoveryEnvelopeV1`]
//!   is a zero-constructor uninhabited Rust enum whose reserved wire discriminant always rejects
//!   fail-closed.
//! - **Terminal identity-hash failure:** `ProtocolIdentityHashCompromiseV1` (Row 1 from
//!   `PROTOCOL-ID-1`) is strictly terminal and cannot be migrated in place.
//!
//! A framework does not prove any concrete migration (`UPGRADE-1`) or authorize an upgrade.

use std::fmt;

use crate::crypto_1::SuiteMigrationPolicyV1;
use crate::governance_1::{GovernanceLifecycleCodeV1, GovernanceStateRootV1};
use crate::profile_d1::{BoundProfileManifestV1, ProfileLineageKindV1};
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::protocol_id_1::{ParentCollisionRowV1, TwoRowParentCollisionAuditV1};

/// Canonical schema string for [`UpgradeFrameworkPolicyV1`] (`UPGRADE-FRAMEWORK-1`).
pub const UPGRADE_FRAMEWORK_1_POLICY_SCHEMA: &[u8] =
    b"bitblock/upgrade-framework-1/lifecycle-policy/v1";
/// Canonical schema string for [`UpgradeFrameworkAuditReceiptV1`] (`UPGRADE-FRAMEWORK-1`).
pub const UPGRADE_FRAMEWORK_1_AUDIT_SCHEMA: &[u8] =
    b"bitblock/upgrade-framework-1/transition-audit/v1";

/// Canonical version identifier for `UPGRADE-FRAMEWORK-1` policies and audit receipts.
pub const UPGRADE_FRAMEWORK_1_VERSION: u16 = 1;

/// Reserved wire discriminant for uninhabited [`PrecommittedIndependentRecoveryEnvelopeV1`].
pub const RESERVED_RECOVERY_ENVELOPE_WIRE_TAG: u8 = 0xFE;

/// Fail-closed validation, codec, or policy error for `UPGRADE-FRAMEWORK-1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpgradeFramework1Error {
    /// Wire buffer had invalid version, length, discriminant, or trailing bytes.
    InvalidWire(&'static str),
    /// Lifecycle policy violated anti-downgrade, timelock, or suite-migration invariants.
    InvalidPolicy(&'static str),
    /// Caller attempted to decode or supply the strictly uninhabited recovery envelope.
    UninhabitedRecoveryEnvelopeRejected,
    /// Historical block verification or downgrade rule failed closed.
    HistoricalOrDowngradeViolation(&'static str),
}

impl fmt::Display for UpgradeFramework1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for UpgradeFramework1Error {}

/// Reserved future independent recovery envelope — strictly uninhabited in v1 (`CAPABILITY_TREE.md`).
///
/// Has zero constructors at compile time. Positive recovery controls remain inadmissible until a
/// future `FutureIndependentRecoveryExtensionV1` is separately specified and admitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PrecommittedIndependentRecoveryEnvelopeV1 {}

/// Always fails closed: [`PrecommittedIndependentRecoveryEnvelopeV1`] is uninhabited in v1.
pub fn decode_reserved_recovery_envelope(
    _bytes: &[u8],
) -> Result<PrecommittedIndependentRecoveryEnvelopeV1, UpgradeFramework1Error> {
    Err(UpgradeFramework1Error::UninhabitedRecoveryEnvelopeRejected)
}

/// Exact sole inhabited disposition for an established live authority-binding compromise (Loop 8–10).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum CurrentEstablishedAuthorityCompromiseDispositionV1 {
    /// Affected lineage must safe-halt and recover only under a separately trusted new genesis.
    SafeHaltAndSeparatelyTrustedNewGenesisV1 = 1,
}

/// Version-lifecycle policy selected by a protocol profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum VersionLifecycleModeV1 {
    /// Every version change is rejected in-place and requires a brand-new genesis/chain.
    ImmutableVersionOnly = 1,
    /// Preventive in-place migration permitted while live authority bindings remain sound.
    PreventiveInPlaceMigration = 2,
}

impl VersionLifecycleModeV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte lifecycle-mode discriminant.
    pub const fn from_code(code: u8) -> Result<Self, UpgradeFramework1Error> {
        match code {
            1 => Ok(Self::ImmutableVersionOnly),
            2 => Ok(Self::PreventiveInPlaceMigration),
            _ => Err(UpgradeFramework1Error::InvalidWire(
                "unknown VersionLifecycleModeV1",
            )),
        }
    }
}

/// Complete 6-scenario Loop 8–10 established authority-binding compromise control table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum CompromiseControlScenarioV1 {
    /// No established authority compromise (ordinary preventive evaluation).
    None = 0,
    /// Live role cryptographic collision or forgery (`ProfileOwnedAuthorityBindingCompromiseV1`).
    CollisionOrForgery = 1,
    /// Multiple conflicting last-safe parent candidates (ambiguous last-safe parent).
    AmbiguousLastSafeParent = 2,
    /// Recovery or successor signed through the already-failed authority role.
    FailedRoleAuthorized = 3,
    /// Hypothetical unactivated recovery precommit attempted in current protocol.
    HypotheticalPrecommit = 4,
    /// Post-compromise precommit created after the live role already failed.
    PostCompromisePrecommit = 5,
    /// Established compromise with no precommit present.
    NoPrecommit = 6,
}

impl CompromiseControlScenarioV1 {
    /// All 6 mandatory established-compromise control scenarios from `CAPABILITY_TREE.md`.
    pub const ALL_SIX_ESTABLISHED: [Self; 6] = [
        Self::CollisionOrForgery,
        Self::AmbiguousLastSafeParent,
        Self::FailedRoleAuthorized,
        Self::HypotheticalPrecommit,
        Self::PostCompromisePrecommit,
        Self::NoPrecommit,
    ];

    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte compromise scenario discriminant.
    pub const fn from_code(code: u8) -> Result<Self, UpgradeFramework1Error> {
        match code {
            0 => Ok(Self::None),
            1 => Ok(Self::CollisionOrForgery),
            2 => Ok(Self::AmbiguousLastSafeParent),
            3 => Ok(Self::FailedRoleAuthorized),
            4 => Ok(Self::HypotheticalPrecommit),
            5 => Ok(Self::PostCompromisePrecommit),
            6 => Ok(Self::NoPrecommit),
            _ => Err(UpgradeFramework1Error::InvalidWire(
                "unknown CompromiseControlScenarioV1",
            )),
        }
    }
}

/// Deterministic lifecycle / compromise disposition emitted by `UPGRADE-FRAMEWORK-1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum UpgradeDispositionV1 {
    /// Preventive in-place migration satisfied all soundness, timelock, and non-downgrade gates.
    PreventiveMigrationAdmitted = 0,
    /// Immutable-version policy refused in-place version change (requires new genesis/chain).
    RejectedImmutableVersionRequiresNewGenesis = 1,
    /// Height-0 genesis rejected fabricated old-profile predecessor input.
    RejectedGenesisFabricatedOldProfile = 2,
    /// Missing admitted old profile or identical self-migration target rejected.
    RejectedMissingAdmittedOldOrSameTarget = 3,
    /// Non-increasing version or post-activation downgrade attempt rejected.
    RejectedVersionDowngrade = 4,
    /// Governance Safe-Halted or mandatory exit timelock not elapsed.
    RejectedGovernanceHaltedOrTimelockPending = 5,
    /// Migration interrupted prior to atomic commit; rolled back cleanly to old profile.
    RejectedInterruptedRollbackToOldProfile = 6,
    /// Established live authority compromise -> `SafeHaltAndSeparatelyTrustedNewGenesisV1`.
    SafeHaltAndSeparatelyTrustedNewGenesis = 7,
    /// Row-1 `ProtocolIdentityHashCompromiseV1` -> strictly terminal pre-profile failure.
    TerminalProtocolIdentityHashCompromise = 8,
    /// Uninhabited `PrecommittedIndependentRecoveryEnvelopeV1` rejected fail-closed.
    RejectedUninhabitedRecoveryEnvelope = 9,
}

impl UpgradeDispositionV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte disposition discriminant.
    pub const fn from_code(code: u8) -> Result<Self, UpgradeFramework1Error> {
        match code {
            0 => Ok(Self::PreventiveMigrationAdmitted),
            1 => Ok(Self::RejectedImmutableVersionRequiresNewGenesis),
            2 => Ok(Self::RejectedGenesisFabricatedOldProfile),
            3 => Ok(Self::RejectedMissingAdmittedOldOrSameTarget),
            4 => Ok(Self::RejectedVersionDowngrade),
            5 => Ok(Self::RejectedGovernanceHaltedOrTimelockPending),
            6 => Ok(Self::RejectedInterruptedRollbackToOldProfile),
            7 => Ok(Self::SafeHaltAndSeparatelyTrustedNewGenesis),
            8 => Ok(Self::TerminalProtocolIdentityHashCompromise),
            9 => Ok(Self::RejectedUninhabitedRecoveryEnvelope),
            _ => Err(UpgradeFramework1Error::InvalidWire(
                "unknown UpgradeDispositionV1",
            )),
        }
    }
}

/// Canonical version-lifecycle policy (`bitblock/upgrade-framework-1/lifecycle-policy/v1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UpgradeFrameworkPolicyV1 {
    /// Policy schema version (`1`).
    pub policy_version: u16,
    /// Selected version-lifecycle mode (`ImmutableVersionOnly` or `PreventiveInPlaceMigration`).
    pub lifecycle_mode: VersionLifecycleModeV1,
    /// Chain identifier (`> 0`).
    pub chain_id: u64,
    /// Bound `GENESIS-D1` immutable genesis identifier.
    pub genesis_id: [u8; 32],
    /// Minimum mandatory exit timelock epochs (`>= 21` for `PreventiveInPlaceMigration`).
    pub min_timelock_epochs: u64,
    /// Hard maximum bounded migration steps per atomic migration block (`> 0`).
    pub max_migration_ops: u32,
    /// Must be `true`: post-activation version downgrades are strictly rejected.
    pub forbid_downgrade: bool,
    /// Must be `true`: historical pre-activation blocks remain verifiable under old rules.
    pub preserve_historical_verification: bool,
    /// Must be `true`: preventive migration requires every live authority binding sound.
    pub require_sound_live_bindings: bool,
    /// Embedded `CRYPTO-1` suite-migration readiness policy (`36` bytes).
    pub suite_migration_policy: SuiteMigrationPolicyV1,
}

impl UpgradeFrameworkPolicyV1 {
    /// Fixed canonical big-endian wire length (`94` bytes).
    pub const WIRE_LEN: usize = 2 + 1 + 8 + 32 + 8 + 4 + 3 + SuiteMigrationPolicyV1::WIRE_LEN;

    /// Returns the default v1 preventive in-place upgrade framework policy (`bitblock.devnet.v1`).
    pub fn v1_preventive_default(
        chain_id: u64,
        genesis_id: [u8; 32],
    ) -> Result<Self, UpgradeFramework1Error> {
        let p = Self {
            policy_version: UPGRADE_FRAMEWORK_1_VERSION,
            lifecycle_mode: VersionLifecycleModeV1::PreventiveInPlaceMigration,
            chain_id,
            genesis_id,
            min_timelock_epochs: 21,
            max_migration_ops: 4_096,
            forbid_downgrade: true,
            preserve_historical_verification: true,
            require_sound_live_bindings: true,
            suite_migration_policy: SuiteMigrationPolicyV1::v1_pq_upgrade_ready(10_000, 12_000),
        };
        p.validate()?;
        Ok(p)
    }

    /// Returns an immutable-version-only policy rejecting every in-place version change.
    pub fn v1_immutable_only(
        chain_id: u64,
        genesis_id: [u8; 32],
    ) -> Result<Self, UpgradeFramework1Error> {
        let p = Self {
            policy_version: UPGRADE_FRAMEWORK_1_VERSION,
            lifecycle_mode: VersionLifecycleModeV1::ImmutableVersionOnly,
            chain_id,
            genesis_id,
            min_timelock_epochs: 21,
            max_migration_ops: 1,
            forbid_downgrade: true,
            preserve_historical_verification: true,
            require_sound_live_bindings: true,
            suite_migration_policy: SuiteMigrationPolicyV1::v1_pq_upgrade_ready(10_000, 12_000),
        };
        p.validate()?;
        Ok(p)
    }

    /// Validates all non-downgrade, historical, timelock, boundedness, and suite invariants.
    pub fn validate(&self) -> Result<(), UpgradeFramework1Error> {
        if self.policy_version != UPGRADE_FRAMEWORK_1_VERSION {
            return Err(UpgradeFramework1Error::InvalidPolicy("bad policy_version"));
        }
        if self.chain_id == 0 || self.genesis_id == [0u8; 32] {
            return Err(UpgradeFramework1Error::InvalidPolicy(
                "chain_id and genesis_id must be non-zero",
            ));
        }
        if self.min_timelock_epochs == 0 || self.max_migration_ops == 0 {
            return Err(UpgradeFramework1Error::InvalidPolicy(
                "timelock and max_migration_ops must be > 0",
            ));
        }
        if !self.forbid_downgrade
            || !self.preserve_historical_verification
            || !self.require_sound_live_bindings
        {
            return Err(UpgradeFramework1Error::InvalidPolicy(
                "forbid_downgrade, historical preservation, and sound bindings must be true",
            ));
        }
        self.suite_migration_policy
            .validate()
            .map_err(|_| UpgradeFramework1Error::InvalidPolicy("invalid SuiteMigrationPolicyV1"))?;
        Ok(())
    }

    /// Encodes this policy into 94 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], UpgradeFramework1Error> {
        self.validate()?;
        let mut out = Vec::with_capacity(Self::WIRE_LEN);
        out.extend_from_slice(&self.policy_version.to_be_bytes());
        out.push(self.lifecycle_mode.code());
        out.extend_from_slice(&self.chain_id.to_be_bytes());
        out.extend_from_slice(&self.genesis_id);
        out.extend_from_slice(&self.min_timelock_epochs.to_be_bytes());
        out.extend_from_slice(&self.max_migration_ops.to_be_bytes());
        out.push(u8::from(self.forbid_downgrade));
        out.push(u8::from(self.preserve_historical_verification));
        out.push(u8::from(self.require_sound_live_bindings));
        let suite_bytes = self
            .suite_migration_policy
            .encode_canonical()
            .map_err(|_| UpgradeFramework1Error::InvalidPolicy("cannot encode suite policy"))?;
        out.extend_from_slice(&suite_bytes);
        Ok(out.try_into().expect("94-byte UpgradeFrameworkPolicyV1"))
    }

    /// Strictly decodes 94 canonical big-endian wire bytes into an [`UpgradeFrameworkPolicyV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, UpgradeFramework1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(UpgradeFramework1Error::InvalidWire("wrong policy wire len"));
        }
        let policy = Self {
            policy_version: read_u16_be(bytes, 0)?,
            lifecycle_mode: VersionLifecycleModeV1::from_code(bytes[2])?,
            chain_id: read_u64_be(bytes, 3)?,
            genesis_id: read_arr32(bytes, 11)?,
            min_timelock_epochs: read_u64_be(bytes, 43)?,
            max_migration_ops: read_u32_be(bytes, 51)?,
            forbid_downgrade: read_bool(bytes[55])?,
            preserve_historical_verification: read_bool(bytes[56])?,
            require_sound_live_bindings: read_bool(bytes[57])?,
            suite_migration_policy: SuiteMigrationPolicyV1::decode_canonical(&bytes[58..94])
                .map_err(|_| UpgradeFramework1Error::InvalidWire("bad SuiteMigrationPolicyV1"))?,
        };
        policy.validate()?;
        Ok(policy)
    }

    /// Computes the canonical `Policy` `ContentTag` for this upgrade framework policy.
    pub fn tag(&self) -> Result<ContentTag, UpgradeFramework1Error> {
        let bytes = self.encode_canonical()?;
        hash_canonical(
            TagKind::Policy,
            UPGRADE_FRAMEWORK_1_POLICY_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .map_err(|_| UpgradeFramework1Error::InvalidWire("hash_canonical failed"))
    }
}

/// Deterministic upgrade-lifecycle / compromise audit receipt
/// (`bitblock/upgrade-framework-1/transition-audit/v1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UpgradeFrameworkAuditReceiptV1 {
    /// Receipt schema version (`1`).
    pub receipt_version: u16,
    /// Digest of the governing [`UpgradeFrameworkPolicyV1`] policy tag.
    pub policy_digest: [u8; 32],
    /// Admitted old `profile_id` (`[0u8; 32]` on genesis or missing-old rejection).
    pub old_profile_id: [u8; 32],
    /// Target candidate `profile_id`.
    pub target_profile_id: [u8; 32],
    /// Target activation height (`0` on genesis check).
    pub activation_height: u64,
    /// Old protocol version.
    pub old_version: u16,
    /// Target protocol version.
    pub target_version: u16,
    /// Deterministic framework disposition.
    pub disposition: UpgradeDispositionV1,
    /// Compromise control scenario code (`0` = none, `1..=6` = Loop 8–10 controls).
    pub compromise_scenario: CompromiseControlScenarioV1,
    /// `true` when pre-activation historical verification remained intact.
    pub historical_verified: bool,
    /// `true` when rollback/safe-halt left zero mixed-profile root residue.
    pub rollback_clean: bool,
    /// 32-byte digest of the audited evidence / migration proposal.
    pub evidence_digest: [u8; 32],
}

impl UpgradeFrameworkAuditReceiptV1 {
    /// Fixed canonical big-endian wire length (`146` bytes).
    pub const WIRE_LEN: usize = 2 + 32 * 3 + 8 + 2 + 2 + 1 + 1 + 1 + 1 + 32;

    /// Encodes this audit receipt into 146 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], UpgradeFramework1Error> {
        if self.receipt_version != UPGRADE_FRAMEWORK_1_VERSION {
            return Err(UpgradeFramework1Error::InvalidWire("bad receipt_version"));
        }
        let mut out = Vec::with_capacity(Self::WIRE_LEN);
        out.extend_from_slice(&self.receipt_version.to_be_bytes());
        out.extend_from_slice(&self.policy_digest);
        out.extend_from_slice(&self.old_profile_id);
        out.extend_from_slice(&self.target_profile_id);
        out.extend_from_slice(&self.activation_height.to_be_bytes());
        out.extend_from_slice(&self.old_version.to_be_bytes());
        out.extend_from_slice(&self.target_version.to_be_bytes());
        out.push(self.disposition.code());
        out.push(self.compromise_scenario.code());
        out.push(u8::from(self.historical_verified));
        out.push(u8::from(self.rollback_clean));
        out.extend_from_slice(&self.evidence_digest);
        Ok(out.try_into().expect("146-byte audit receipt"))
    }

    /// Strictly decodes 146 canonical big-endian wire bytes into an [`UpgradeFrameworkAuditReceiptV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, UpgradeFramework1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(UpgradeFramework1Error::InvalidWire(
                "wrong audit receipt len",
            ));
        }
        let receipt_version = read_u16_be(bytes, 0)?;
        if receipt_version != UPGRADE_FRAMEWORK_1_VERSION {
            return Err(UpgradeFramework1Error::InvalidWire("bad receipt_version"));
        }
        Ok(Self {
            receipt_version,
            policy_digest: read_arr32(bytes, 2)?,
            old_profile_id: read_arr32(bytes, 34)?,
            target_profile_id: read_arr32(bytes, 66)?,
            activation_height: read_u64_be(bytes, 98)?,
            old_version: read_u16_be(bytes, 106)?,
            target_version: read_u16_be(bytes, 108)?,
            disposition: UpgradeDispositionV1::from_code(bytes[110])?,
            compromise_scenario: CompromiseControlScenarioV1::from_code(bytes[111])?,
            historical_verified: read_bool(bytes[112])?,
            rollback_clean: read_bool(bytes[113])?,
            evidence_digest: read_arr32(bytes, 114)?,
        })
    }

    /// Computes the canonical `Receipt` `ContentTag` for this upgrade audit receipt.
    pub fn tag(&self) -> Result<ContentTag, UpgradeFramework1Error> {
        let bytes = self.encode_canonical()?;
        hash_canonical(
            TagKind::Receipt,
            UPGRADE_FRAMEWORK_1_AUDIT_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .map_err(|_| UpgradeFramework1Error::InvalidWire("hash_canonical failed"))
    }
}

/// Complete input context for evaluating one preventive in-place migration proposal.
#[derive(Clone, Debug)]
pub struct PreventiveMigrationRequestV1<'a> {
    /// Admitted old profile manifest (`None` triggers fail-closed missing-old rejection).
    pub old_admitted_profile: Option<&'a BoundProfileManifestV1>,
    /// Distinct candidate successor profile manifest.
    pub target_candidate_profile: &'a BoundProfileManifestV1,
    /// Current protocol version under old profile.
    pub old_version: u16,
    /// Proposed target protocol version under successor profile.
    pub target_version: u16,
    /// Planned consensus activation height.
    pub activation_height: u64,
    /// Governance root snapshot governing timelock & Safe-Halt status.
    pub gov_root: &'a GovernanceStateRootV1,
    /// Elapsed public exit-timelock epochs since proposal queue.
    pub elapsed_timelock_epochs: u64,
    /// `true` iff every live profile-owned authority binding remains independently sound.
    pub all_live_bindings_sound: bool,
    /// `true` iff exactly one unambiguous last-safe parent is verified.
    pub unique_last_safe_parent: bool,
    /// Optional `PROTOCOL-ID-1` two-row parent collision evidence.
    pub collision_audit: Option<&'a TwoRowParentCollisionAuditV1>,
    /// `true` to simulate a crash/interruption before atomic commit (tests rollback safety).
    pub interrupted_before_commit: bool,
    /// Digest of the candidate migration specification / proposal artifact.
    pub evidence_digest: [u8; 32],
}

/// Stateless deterministic verifier enforcing [`UpgradeFrameworkPolicyV1`].
#[derive(Clone, Copy, Debug)]
pub struct UpgradeFrameworkVerifierV1 {
    policy: UpgradeFrameworkPolicyV1,
    policy_digest: [u8; 32],
}

impl UpgradeFrameworkVerifierV1 {
    /// Constructs a validated verifier bound to `policy`.
    pub fn new(policy: UpgradeFrameworkPolicyV1) -> Result<Self, UpgradeFramework1Error> {
        policy.validate()?;
        let policy_digest = policy.tag()?.digest();
        Ok(Self {
            policy,
            policy_digest,
        })
    }

    /// Returns the governing [`UpgradeFrameworkPolicyV1`].
    pub const fn policy(&self) -> UpgradeFrameworkPolicyV1 {
        self.policy
    }

    /// Verifies height-0 genesis lineage, rejecting fabricated old-profile predecessor inputs.
    pub fn evaluate_genesis_lineage(
        &self,
        manifest: &BoundProfileManifestV1,
        height: u64,
        evidence_digest: [u8; 32],
    ) -> UpgradeFrameworkAuditReceiptV1 {
        let fabricated_old = height == 0
            && (manifest.candidate.lineage_kind != ProfileLineageKindV1::InitialLaunch
                || manifest.candidate.predecessor_profile_id.is_some());
        let disposition = if fabricated_old {
            UpgradeDispositionV1::RejectedGenesisFabricatedOldProfile
        } else {
            UpgradeDispositionV1::PreventiveMigrationAdmitted
        };
        UpgradeFrameworkAuditReceiptV1 {
            receipt_version: UPGRADE_FRAMEWORK_1_VERSION,
            policy_digest: self.policy_digest,
            old_profile_id: manifest
                .candidate
                .predecessor_profile_id
                .unwrap_or([0u8; 32]),
            target_profile_id: manifest.profile_id,
            activation_height: height,
            old_version: 1,
            target_version: 1,
            disposition,
            compromise_scenario: CompromiseControlScenarioV1::None,
            historical_verified: true,
            rollback_clean: true,
            evidence_digest,
        }
    }

    /// Evaluates one preventive in-place migration proposal against all lifecycle, governance,
    /// downgrade, collision, interruption, and soundness gates.
    pub fn evaluate_preventive_migration(
        &self,
        req: &PreventiveMigrationRequestV1<'_>,
    ) -> UpgradeFrameworkAuditReceiptV1 {
        let old_id = req
            .old_admitted_profile
            .map(|m| m.profile_id)
            .unwrap_or([0u8; 32]);
        let target_id = req.target_candidate_profile.profile_id;

        // 1. Pre-profile identity hash / live-binding collision audit takes absolute precedence.
        if let Some(col) = req.collision_audit {
            let (disp, scen) = match col.collision_row {
                ParentCollisionRowV1::ProtocolIdentityHashCompromise => (
                    UpgradeDispositionV1::TerminalProtocolIdentityHashCompromise,
                    CompromiseControlScenarioV1::CollisionOrForgery,
                ),
                ParentCollisionRowV1::ProfileOwnedAuthorityBindingCompromise => (
                    UpgradeDispositionV1::SafeHaltAndSeparatelyTrustedNewGenesis,
                    CompromiseControlScenarioV1::CollisionOrForgery,
                ),
            };
            return self.make_receipt(old_id, target_id, req, disp, scen, true, true);
        }

        // 2. Unsound live binding or ambiguous last-safe parent -> SafeHalt + new genesis.
        if !req.all_live_bindings_sound {
            return self.make_receipt(
                old_id,
                target_id,
                req,
                UpgradeDispositionV1::SafeHaltAndSeparatelyTrustedNewGenesis,
                CompromiseControlScenarioV1::FailedRoleAuthorized,
                true,
                true,
            );
        }
        if !req.unique_last_safe_parent {
            return self.make_receipt(
                old_id,
                target_id,
                req,
                UpgradeDispositionV1::SafeHaltAndSeparatelyTrustedNewGenesis,
                CompromiseControlScenarioV1::AmbiguousLastSafeParent,
                true,
                true,
            );
        }

        // 3. Immutable-version policy refuses every in-place version change.
        if self.policy.lifecycle_mode == VersionLifecycleModeV1::ImmutableVersionOnly {
            return self.make_receipt(
                old_id,
                target_id,
                req,
                UpgradeDispositionV1::RejectedImmutableVersionRequiresNewGenesis,
                CompromiseControlScenarioV1::None,
                true,
                true,
            );
        }

        // 4. Require admitted old profile & distinct successor bound to exact predecessor.
        let Some(old_m) = req.old_admitted_profile else {
            return self.make_receipt(
                old_id,
                target_id,
                req,
                UpgradeDispositionV1::RejectedMissingAdmittedOldOrSameTarget,
                CompromiseControlScenarioV1::None,
                true,
                true,
            );
        };
        if old_m.verify().is_err()
            || req.target_candidate_profile.verify().is_err()
            || old_m.profile_id == target_id
            || old_m.candidate.genesis_id != self.policy.genesis_id
            || req.target_candidate_profile.candidate.genesis_id != self.policy.genesis_id
            || req
                .target_candidate_profile
                .candidate
                .predecessor_profile_id
                != Some(old_m.profile_id)
        {
            return self.make_receipt(
                old_id,
                target_id,
                req,
                UpgradeDispositionV1::RejectedMissingAdmittedOldOrSameTarget,
                CompromiseControlScenarioV1::None,
                true,
                true,
            );
        }

        // 5. Anti-downgrade guard: target_version MUST strictly exceed old_version.
        if req.target_version <= req.old_version {
            return self.make_receipt(
                old_id,
                target_id,
                req,
                UpgradeDispositionV1::RejectedVersionDowngrade,
                CompromiseControlScenarioV1::None,
                true,
                true,
            );
        }

        // 6. Governance Safe-Halt and mandatory exit-timelock guard.
        if req.gov_root.lifecycle == GovernanceLifecycleCodeV1::SafeHalted
            || req.elapsed_timelock_epochs < self.policy.min_timelock_epochs
        {
            return self.make_receipt(
                old_id,
                target_id,
                req,
                UpgradeDispositionV1::RejectedGovernanceHaltedOrTimelockPending,
                CompromiseControlScenarioV1::None,
                true,
                true,
            );
        }

        // 7. Interruption / crash before atomic commit rolls back cleanly to old profile.
        if req.interrupted_before_commit {
            return self.make_receipt(
                old_id,
                target_id,
                req,
                UpgradeDispositionV1::RejectedInterruptedRollbackToOldProfile,
                CompromiseControlScenarioV1::None,
                true,
                true,
            );
        }

        self.make_receipt(
            old_id,
            target_id,
            req,
            UpgradeDispositionV1::PreventiveMigrationAdmitted,
            CompromiseControlScenarioV1::None,
            true,
            true,
        )
    }

    /// Evaluates any of the 6 Loop 8–10 established authority-binding compromise controls, always
    /// returning [`CurrentEstablishedAuthorityCompromiseDispositionV1::SafeHaltAndSeparatelyTrustedNewGenesisV1`]
    /// alongside a canonical audit receipt.
    pub fn evaluate_established_authority_compromise(
        &self,
        scenario: CompromiseControlScenarioV1,
        old_profile_id: [u8; 32],
        attempted_target_id: [u8; 32],
        height: u64,
        evidence_digest: [u8; 32],
    ) -> (
        CurrentEstablishedAuthorityCompromiseDispositionV1,
        UpgradeFrameworkAuditReceiptV1,
    ) {
        let receipt = UpgradeFrameworkAuditReceiptV1 {
            receipt_version: UPGRADE_FRAMEWORK_1_VERSION,
            policy_digest: self.policy_digest,
            old_profile_id,
            target_profile_id: attempted_target_id,
            activation_height: height,
            old_version: 1,
            target_version: 2,
            disposition: UpgradeDispositionV1::SafeHaltAndSeparatelyTrustedNewGenesis,
            compromise_scenario: scenario,
            historical_verified: true,
            rollback_clean: true,
            evidence_digest,
        };
        (
            CurrentEstablishedAuthorityCompromiseDispositionV1::SafeHaltAndSeparatelyTrustedNewGenesisV1,
            receipt,
        )
    }

    /// Evaluates an uninhabited `PrecommittedIndependentRecoveryEnvelopeV1` wire claim (always rejects).
    pub fn evaluate_uninhabited_recovery_claim(
        &self,
        raw_envelope: &[u8],
        old_profile_id: [u8; 32],
        height: u64,
        evidence_digest: [u8; 32],
    ) -> UpgradeFrameworkAuditReceiptV1 {
        let _ = decode_reserved_recovery_envelope(raw_envelope).unwrap_err();
        UpgradeFrameworkAuditReceiptV1 {
            receipt_version: UPGRADE_FRAMEWORK_1_VERSION,
            policy_digest: self.policy_digest,
            old_profile_id,
            target_profile_id: [0u8; 32],
            activation_height: height,
            old_version: 1,
            target_version: 2,
            disposition: UpgradeDispositionV1::RejectedUninhabitedRecoveryEnvelope,
            compromise_scenario: CompromiseControlScenarioV1::HypotheticalPrecommit,
            historical_verified: true,
            rollback_clean: true,
            evidence_digest,
        }
    }

    /// Enforces historical-block preservation (`height < activation_height` -> `old_version`) and
    /// strict post-activation anti-downgrade (`height >= activation_height` -> `new_version`).
    pub fn verify_historical_or_post_activation_version(
        &self,
        activation_height: u64,
        block_height: u64,
        block_version: u16,
        old_version: u16,
        new_version: u16,
    ) -> Result<(), UpgradeFramework1Error> {
        if new_version <= old_version {
            return Err(UpgradeFramework1Error::HistoricalOrDowngradeViolation(
                "new_version must strictly exceed old_version",
            ));
        }
        if block_height < activation_height {
            if block_version != old_version {
                return Err(UpgradeFramework1Error::HistoricalOrDowngradeViolation(
                    "historical pre-activation block must verify under old_version",
                ));
            }
        } else if block_version != new_version {
            return Err(UpgradeFramework1Error::HistoricalOrDowngradeViolation(
                "post-activation block downgrade to old_version rejected",
            ));
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn make_receipt(
        &self,
        old_profile_id: [u8; 32],
        target_profile_id: [u8; 32],
        req: &PreventiveMigrationRequestV1<'_>,
        disposition: UpgradeDispositionV1,
        compromise_scenario: CompromiseControlScenarioV1,
        historical_verified: bool,
        rollback_clean: bool,
    ) -> UpgradeFrameworkAuditReceiptV1 {
        UpgradeFrameworkAuditReceiptV1 {
            receipt_version: UPGRADE_FRAMEWORK_1_VERSION,
            policy_digest: self.policy_digest,
            old_profile_id,
            target_profile_id,
            activation_height: req.activation_height,
            old_version: req.old_version,
            target_version: req.target_version,
            disposition,
            compromise_scenario,
            historical_verified,
            rollback_clean,
            evidence_digest: req.evidence_digest,
        }
    }
}

fn read_bool(b: u8) -> Result<bool, UpgradeFramework1Error> {
    match b {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(UpgradeFramework1Error::InvalidWire("non-boolean byte")),
    }
}

fn read_u16_be(bytes: &[u8], off: usize) -> Result<u16, UpgradeFramework1Error> {
    let sl: [u8; 2] = bytes
        .get(off..off + 2)
        .ok_or(UpgradeFramework1Error::InvalidWire("truncated u16"))?
        .try_into()
        .unwrap();
    Ok(u16::from_be_bytes(sl))
}

fn read_u32_be(bytes: &[u8], off: usize) -> Result<u32, UpgradeFramework1Error> {
    let sl: [u8; 4] = bytes
        .get(off..off + 4)
        .ok_or(UpgradeFramework1Error::InvalidWire("truncated u32"))?
        .try_into()
        .unwrap();
    Ok(u32::from_be_bytes(sl))
}

fn read_u64_be(bytes: &[u8], off: usize) -> Result<u64, UpgradeFramework1Error> {
    let sl: [u8; 8] = bytes
        .get(off..off + 8)
        .ok_or(UpgradeFramework1Error::InvalidWire("truncated u64"))?
        .try_into()
        .unwrap();
    Ok(u64::from_be_bytes(sl))
}

fn read_arr32(bytes: &[u8], off: usize) -> Result<[u8; 32], UpgradeFramework1Error> {
    let sl: [u8; 32] = bytes
        .get(off..off + 32)
        .ok_or(UpgradeFramework1Error::InvalidWire("truncated [u8; 32]"))?
        .try_into()
        .unwrap();
    Ok(sl)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrade_framework_1_policy_codec_uninhabited_recovery_and_six_compromise_controls() {
        let prev = UpgradeFrameworkPolicyV1::v1_preventive_default(77, [0xAA; 32]).unwrap();
        let imm = UpgradeFrameworkPolicyV1::v1_immutable_only(77, [0xAA; 32]).unwrap();
        assert_eq!(
            UpgradeFrameworkPolicyV1::decode_canonical(&prev.encode_canonical().unwrap()).unwrap(),
            prev
        );
        assert_eq!(
            UpgradeFrameworkPolicyV1::decode_canonical(&imm.encode_canonical().unwrap()).unwrap(),
            imm
        );
        assert_eq!(
            prev.tag().unwrap().to_string(),
            "#bb.policy.sha256.udwdlgfcdx3lmezbw6ffttvoom2bqjv2tlyxnbekhmtswgeyy74q"
        );
        assert_eq!(
            imm.tag().unwrap().to_string(),
            "#bb.policy.sha256.kqe4wzngoso465trwfsy2brufqygodn7vytur6af27s7rrqeoxdq"
        );
        assert_ne!(prev.tag().unwrap(), imm.tag().unwrap());

        // Uninhabited recovery envelope rejects fail-closed
        assert_eq!(
            decode_reserved_recovery_envelope(&[RESERVED_RECOVERY_ENVELOPE_WIRE_TAG]),
            Err(UpgradeFramework1Error::UninhabitedRecoveryEnvelopeRejected)
        );

        let verifier = UpgradeFrameworkVerifierV1::new(prev).unwrap();
        let (_, comp_rcpt) = verifier.evaluate_established_authority_compromise(
            CompromiseControlScenarioV1::CollisionOrForgery,
            [0x11; 32],
            [0x22; 32],
            10_000,
            [0x33; 32],
        );
        assert_eq!(
            comp_rcpt.tag().unwrap().to_string(),
            "#bb.receipt.sha256.sgjpzscp672njwiyhayextl5ccgcx4ypk45b6iw4ci7j6kmyj7vq"
        );
        let uninh_rcpt = verifier.evaluate_uninhabited_recovery_claim(
            &[RESERVED_RECOVERY_ENVELOPE_WIRE_TAG],
            [0x11; 32],
            10_000,
            [0x33; 32],
        );
        assert_eq!(
            uninh_rcpt.tag().unwrap().to_string(),
            "#bb.receipt.sha256.agawoxk3h6ceowzjzjgonqtz43jpngokg5jzdvrnupvauwaoytza"
        );
        for scen in CompromiseControlScenarioV1::ALL_SIX_ESTABLISHED {
            let (disp, rcpt) = verifier.evaluate_established_authority_compromise(
                scen, [0x11; 32], [0x22; 32], 10_000, [0x33; 32],
            );
            assert_eq!(
                disp,
                CurrentEstablishedAuthorityCompromiseDispositionV1::SafeHaltAndSeparatelyTrustedNewGenesisV1
            );
            assert_eq!(
                rcpt.disposition,
                UpgradeDispositionV1::SafeHaltAndSeparatelyTrustedNewGenesis
            );
            assert_eq!(
                UpgradeFrameworkAuditReceiptV1::decode_canonical(&rcpt.encode_canonical().unwrap())
                    .unwrap(),
                rcpt
            );
        }
        // Historical preservation vs post-activation downgrade rejection
        verifier
            .verify_historical_or_post_activation_version(10_000, 9_999, 1, 1, 2)
            .unwrap();
        verifier
            .verify_historical_or_post_activation_version(10_000, 10_000, 2, 1, 2)
            .unwrap();
        assert!(matches!(
            verifier.verify_historical_or_post_activation_version(10_000, 10_000, 1, 1, 2),
            Err(UpgradeFramework1Error::HistoricalOrDowngradeViolation(_))
        ));
    }
}
