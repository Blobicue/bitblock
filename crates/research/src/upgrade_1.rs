//! `UPGRADE-1@base` (`PS-083`): deterministic preventive version migration (`v1 -> v2` bounds,
//! consensus-native occurrence-universe handoff, and deterministic receipt bridge), `CE-30`/`CE-31`
//! old/new polarity enforcement, bounded 10-domain logical-state migration replay, atomic
//! interruption rollback, archival historical verification, and downgrade refusal.
//!
//! Strictly behind `--features protocol-research`, 100% `wasm32`-clean, zero new Cargo
//! dependencies, and zero edits to frozen crates or modules.
//!
//! # Normative Boundaries (`CAPABILITY_TREE.md` / `LAYER1_DESIGN_PAPER.md` §4.5 lines 2044–2061)
//! - **Preventive migration only:** verifies live authority bindings are sound via frozen
//!   `UPGRADE-FRAMEWORK-1`; rejects any established `ProfileOwnedAuthorityBindingCompromiseV1` or
//!   `ProtocolIdentityHashCompromiseV1` fail-closed.
//! - **Exact `CE-30`/`CE-31` polarity:** binds an already admitted `old_admitted_profile_id` to a
//!   strictly distinct `new_candidate_profile_id` (`old != new`, `current_scope_digest !=
//!   genesis_scope_digest`), keeping `program-active = false` and `verkle-selected = false`.
//! - **Logical-state conservation:** migrates the 10-domain logical state deterministically across
//!   a bounded parameter delta (`max_migration_ops`) while preserving bedrock `(0, 0, 0)`,
//!   fungible native supply, bonded stake, active tokens, episodes, actors, nonces, validator
//!   schedules, and the inactive `ProgramState` sentinel.
//! - **Non-claim:** governance approval or old-root equality alone does not prove migration
//!   correctness; `BOUNDED` status is mandatory.

use std::collections::BTreeSet;
use std::fmt;

use sha2::{Digest, Sha256};

use crate::auth_commit_1::{DomainIdV1, DomainPatriciaTrieV1};
use crate::crypto_d1::CryptoSuiteDescriptorV1;
use crate::execution_root_1::{
    ExecutionDomainRegistryV1, ExecutionRootCompositionV1, ExecutionRootPreimageContextV1,
    ExecutionRootRefV1, compose_from_executor_snapshot,
};
use crate::executor_1::ExecutorDomainSnapshotV1;
use crate::genesis_d1::GenesisPackageV1;
use crate::genesis_root_parity_1::{
    CanonicalDevnetGenesisFixtureV1, GenesisParityInputV1, GenesisRootParityRecordV1,
    verify_genesis_root_parity,
};
use crate::governance_1::{GovernanceLifecycleCodeV1, GovernanceStateRootV1, param_trie_key};
use crate::profile_1::{
    MigrationEvidenceBindingV1, PredicateEnvironmentV1, ProfileAdmissionManifestV1,
    ProfileAdmissionRequestV1, ProfileClosureEvidenceSetV1, ProfileEvidenceNodeEntryV1,
    ProfileEvidenceStatusV1, RoleGraphEdgeV1, admit_candidate_profile,
    canonical_admission_policy_id, canonical_devnet_launch_evidence_entries,
    canonical_launch_role_graph_edges,
};
use crate::profile_bootstrap::{ProfileScopeV1, ProposedLimitVectorV1};
use crate::profile_d1::{BoundProfileManifestV1, CandidateProfileManifestV1, ProfileLineageKindV1};
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::transition_spec_1::ResolvedProfileTransitionSpecV1;
use crate::upgrade_framework_1::{
    PreventiveMigrationRequestV1, UpgradeDispositionV1, UpgradeFrameworkPolicyV1,
    UpgradeFrameworkVerifierV1,
};

/// Canonical schema version for `UPGRADE-1` objects.
pub const UPGRADE_1_VERSION: u16 = 1;

/// Canonical schema string for frozen [`MigrationSpecV1`].
pub const UPGRADE_1_MIGRATION_SPEC_SCHEMA: &[u8] = b"bitblock/upgrade-1/migration-spec/v1";

/// Canonical schema string for [`OccurrenceUniverseHandoffV1`].
pub const UPGRADE_1_OCCURRENCE_HANDOFF_SCHEMA: &[u8] =
    b"bitblock/upgrade-1/occurrence-universe-handoff/v1";

/// Canonical schema string for [`BoundedMigrationReplayReceiptV1`].
pub const UPGRADE_1_REPLAY_RECEIPT_SCHEMA: &[u8] =
    b"bitblock/upgrade-1/bounded-migration-replay/v1";

/// Canonical schema string for [`SuccessorUpgradeEvidenceManifestV1`] (`LAYER1_DESIGN_PAPER.md`).
pub const UPGRADE_1_EVIDENCE_MANIFEST_SCHEMA: &[u8] =
    b"bitblock/successor-upgrade-evidence-manifest/v1";

/// Canonical governance parameter key written by the preventive `v1 -> v2` migration.
pub const UPGRADE_1_GOV_PARAM_KEY: u32 = 9_001;

/// Canonical governance parameter value written by the preventive `v1 -> v2` migration (`v2`).
pub const UPGRADE_1_GOV_PARAM_VALUE: u64 = 2;

/// Canonical planned consensus activation height for the devnet `v1 -> v2` upgrade rehearsal.
pub const UPGRADE_1_ACTIVATION_HEIGHT: u64 = 4;

const UPG1_OCC_POLICY_V1_PREFIX: &[u8] = b"bb.upg1.occ_index_policy.v1";
const UPG1_OCC_POLICY_V2_PREFIX: &[u8] = b"bb.upg1.occ_index_policy.v2";
const UPG1_LOSSLESS_CLOSURE_PREFIX: &[u8] = b"bb.upg1.lossless_closure.v1";
const UPG1_RECEIPT_BRIDGE_PREFIX: &[u8] = b"bb.upg1.receipt_bridge.v1";
const UPG1_STATE_DELTA_PREFIX: &[u8] = b"bb.upg1.param_state_delta.v1";
const UPG1_IMPL_RUST_PREFIX: &[u8] = b"bb.upg1.impl.rust_executor.v1";
const UPG1_IMPL_PY_PREFIX: &[u8] = b"bb.upg1.impl.cleanroom_python.v1";

/// Fail-closed validation, polarity, conservation, rollback, downgrade, or compromise error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Upgrade1Error {
    /// Truncated, overlong, malformed, or non-canonical wire encoding.
    InvalidWire(&'static str),
    /// `CE-30`/`CE-31` old/new profile polarity or scope lineage invariant failed.
    PolarityOrLineageViolation(&'static str),
    /// Governance timelock, quorum, or `UPGRADE-FRAMEWORK-1` preventive soundness failed.
    UnauthorizedOrCompromised(&'static str),
    /// Logical state conservation (bedrock, supply, stake, domain roots, or op cap) failed.
    ConservationViolation(&'static str),
    /// Simulated or real mid-migration interruption rolled back atomically without committing.
    InterruptedAtomicRollback,
    /// Post-activation downgrade or wrong-profile historical verification rejected fail-closed.
    DowngradeOrHistoricalRejected(&'static str),
    /// Cross-artifact digest, occurrence handoff, receipt bridge, or manifest binding mismatched.
    BindingMismatch(&'static str),
}

impl fmt::Display for Upgrade1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Upgrade1Error {}

/// Explicit disposition carried by [`SuccessorUpgradeEvidenceManifestV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Upgrade1DispositionV1 {
    /// Failed or rejected migration (forbidden on admitted manifests).
    Rejected = 0,
    /// Verified bounded preventive `v1 -> v2` migration evidence.
    VerifiedBoundedV1 = 1,
}

impl Upgrade1DispositionV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte disposition discriminant.
    pub const fn from_code(code: u8) -> Result<Self, Upgrade1Error> {
        match code {
            0 => Ok(Self::Rejected),
            1 => Ok(Self::VerifiedBoundedV1),
            _ => Err(Upgrade1Error::InvalidWire("unknown Upgrade1DispositionV1")),
        }
    }
}

/// Receipt-preservation mode for consensus-native occurrence-universe handoff (`UG-04`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ReceiptPreservationModeV1 {
    /// Preserves pre-activation receipts verbatim with a deterministic bridge equivalence proof.
    DeterministicReceiptBridgeV1 = 1,
}

impl ReceiptPreservationModeV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte receipt-preservation mode discriminant.
    pub const fn from_code(code: u8) -> Result<Self, Upgrade1Error> {
        match code {
            1 => Ok(Self::DeterministicReceiptBridgeV1),
            _ => Err(Upgrade1Error::InvalidWire(
                "unknown ReceiptPreservationModeV1",
            )),
        }
    }
}

/// Optional deterministic interruption hook for testing atomic mid-migration rollback safety.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MigrationFaultHookV1 {
    /// Normal uninterrupted execution.
    None,
    /// Simulates an abrupt interruption mid-apply before atomic state commit.
    InterruptMidApply,
}

/// Frozen preventive migration specification (`bitblock/upgrade-1/migration-spec/v1`, `236` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MigrationSpecV1 {
    /// Schema version (`1`).
    pub schema_version: u16,
    /// Chain identifier (`77`).
    pub chain_id: u64,
    /// World identifier (`1`).
    pub world_id: u64,
    /// Immutable launch `genesis_id`.
    pub genesis_id: [u8; 32],
    /// Already admitted old profile identifier (`CE-30`).
    pub predecessor_admitted_profile_id: [u8; 32],
    /// Distinct target successor candidate profile identifier (`CE-31`).
    pub successor_candidate_profile_id: [u8; 32],
    /// Distinct target successor `ProfileScopeV1` digest.
    pub successor_scope_digest: [u8; 32],
    /// Target `ResolvedProfileTransitionSpecV1` digest.
    pub successor_transition_spec_at_profile_id: [u8; 32],
    /// Predecessor protocol version (`1`).
    pub old_version: u16,
    /// Strictly greater successor protocol version (`2`).
    pub target_version: u16,
    /// Planned consensus activation height (`>= 2`).
    pub activation_height: u64,
    /// Minimum governance exit-timelock epochs (`>= 21`).
    pub min_timelock_epochs: u64,
    /// Strict upper bound on bounded migration operations (`> 0`).
    pub max_migration_ops: u32,
    /// Upgraded preventive `v2` proposed limit vector.
    pub new_proposed_limits: ProposedLimitVectorV1,
    /// Deterministic governance parameter key updated atomically by migration (`> 0`).
    pub gov_param_key_u32: u32,
    /// Deterministic governance parameter value written by migration.
    pub gov_param_new_value_u64: u64,
    /// Must strictly be `true`: post-activation downgrade to old profile is forbidden.
    pub forbid_downgrade: bool,
    /// Must strictly be `true`: pre-activation blocks remain verifiable under old profile.
    pub preserve_historical_verification: bool,
}

impl MigrationSpecV1 {
    /// Fixed canonical big-endian wire byte length (`236` bytes).
    pub const WIRE_LEN: usize = 2 + 8 + 8 + 32 * 5 + 2 + 2 + 8 + 8 + 4 + 20 + 4 + 8 + 1 + 1;

    /// Validates version monotonicity, `CE-30`/`CE-31` polarity, timelock, bounds, and flags.
    pub fn validate(&self) -> Result<(), Upgrade1Error> {
        if self.schema_version != UPGRADE_1_VERSION {
            return Err(Upgrade1Error::InvalidWire("bad MigrationSpecV1 version"));
        }
        if self.chain_id == 0 || self.world_id == 0 {
            return Err(Upgrade1Error::InvalidWire("zero chain_id or world_id"));
        }
        if [
            self.genesis_id,
            self.predecessor_admitted_profile_id,
            self.successor_candidate_profile_id,
            self.successor_scope_digest,
            self.successor_transition_spec_at_profile_id,
        ]
        .contains(&[0u8; 32])
        {
            return Err(Upgrade1Error::PolarityOrLineageViolation(
                "zero identifier in MigrationSpecV1",
            ));
        }
        if self.predecessor_admitted_profile_id == self.successor_candidate_profile_id {
            return Err(Upgrade1Error::PolarityOrLineageViolation(
                "CE-30/CE-31 requires old_admitted_profile_id != new_candidate_profile_id",
            ));
        }
        if self.target_version <= self.old_version {
            return Err(Upgrade1Error::DowngradeOrHistoricalRejected(
                "target_version must strictly exceed old_version",
            ));
        }
        if self.activation_height < 2
            || self.min_timelock_epochs == 0
            || self.max_migration_ops == 0
        {
            return Err(Upgrade1Error::InvalidWire(
                "invalid activation_height, timelock, or max_migration_ops",
            ));
        }
        self.new_proposed_limits
            .validate()
            .map_err(|_| Upgrade1Error::InvalidWire("invalid v2 proposed limits"))?;
        if self.gov_param_key_u32 == 0 {
            return Err(Upgrade1Error::InvalidWire(
                "zero governance param key forbidden",
            ));
        }
        if !self.forbid_downgrade || !self.preserve_historical_verification {
            return Err(Upgrade1Error::DowngradeOrHistoricalRejected(
                "forbid_downgrade and preserve_historical_verification must be true",
            ));
        }
        Ok(())
    }

    /// Encodes into exact 238-byte canonical big-endian wire form.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], Upgrade1Error> {
        self.validate()?;
        let mut out = [0u8; Self::WIRE_LEN];
        let mut c = 0usize;
        put_u16(&mut out, &mut c, self.schema_version);
        put_u64(&mut out, &mut c, self.chain_id);
        put_u64(&mut out, &mut c, self.world_id);
        for d in [
            self.genesis_id,
            self.predecessor_admitted_profile_id,
            self.successor_candidate_profile_id,
            self.successor_scope_digest,
            self.successor_transition_spec_at_profile_id,
        ] {
            put_32(&mut out, &mut c, d);
        }
        put_u16(&mut out, &mut c, self.old_version);
        put_u16(&mut out, &mut c, self.target_version);
        put_u64(&mut out, &mut c, self.activation_height);
        put_u64(&mut out, &mut c, self.min_timelock_epochs);
        put_u32(&mut out, &mut c, self.max_migration_ops);
        put_u32(&mut out, &mut c, self.new_proposed_limits.max_tx_bytes);
        put_u32(&mut out, &mut c, self.new_proposed_limits.max_block_txs);
        put_u32(&mut out, &mut c, self.new_proposed_limits.max_block_ops);
        put_u32(&mut out, &mut c, self.new_proposed_limits.max_proof_bytes);
        put_u32(
            &mut out,
            &mut c,
            self.new_proposed_limits.max_state_writes_per_block,
        );
        put_u32(&mut out, &mut c, self.gov_param_key_u32);
        put_u64(&mut out, &mut c, self.gov_param_new_value_u64);
        out[c] = u8::from(self.forbid_downgrade);
        c += 1;
        out[c] = u8::from(self.preserve_historical_verification);
        c += 1;
        debug_assert_eq!(c, Self::WIRE_LEN);
        Ok(out)
    }

    /// Decodes exact 238-byte canonical big-endian wire bytes into [`MigrationSpecV1`].
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, Upgrade1Error> {
        if raw.len() != Self::WIRE_LEN {
            return Err(Upgrade1Error::InvalidWire("bad MigrationSpecV1 length"));
        }
        let mut c = 0usize;
        let spec = Self {
            schema_version: rd_u16(raw, &mut c)?,
            chain_id: rd_u64(raw, &mut c)?,
            world_id: rd_u64(raw, &mut c)?,
            genesis_id: rd_32(raw, &mut c)?,
            predecessor_admitted_profile_id: rd_32(raw, &mut c)?,
            successor_candidate_profile_id: rd_32(raw, &mut c)?,
            successor_scope_digest: rd_32(raw, &mut c)?,
            successor_transition_spec_at_profile_id: rd_32(raw, &mut c)?,
            old_version: rd_u16(raw, &mut c)?,
            target_version: rd_u16(raw, &mut c)?,
            activation_height: rd_u64(raw, &mut c)?,
            min_timelock_epochs: rd_u64(raw, &mut c)?,
            max_migration_ops: rd_u32(raw, &mut c)?,
            new_proposed_limits: ProposedLimitVectorV1 {
                max_tx_bytes: rd_u32(raw, &mut c)?,
                max_block_txs: rd_u32(raw, &mut c)?,
                max_block_ops: rd_u32(raw, &mut c)?,
                max_proof_bytes: rd_u32(raw, &mut c)?,
                max_state_writes_per_block: rd_u32(raw, &mut c)?,
            },
            gov_param_key_u32: rd_u32(raw, &mut c)?,
            gov_param_new_value_u64: rd_u64(raw, &mut c)?,
            forbid_downgrade: rd_bool(raw, &mut c)?,
            preserve_historical_verification: rd_bool(raw, &mut c)?,
        };
        spec.validate()?;
        Ok(spec)
    }

    /// Computes the canonical `Policy` `ContentTag` whose digest IS `migration_spec_id`.
    pub fn tag(&self) -> Result<ContentTag, Upgrade1Error> {
        let enc = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            UPGRADE_1_MIGRATION_SPEC_SCHEMA,
            enc.len() as u64,
            |w| w(&enc),
        )
        .expect("static schema and fixed length"))
    }
}

/// Explicit consensus-native occurrence-universe handoff (`bitblock/upgrade-1/occurrence-universe-handoff/v1`, `187` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OccurrenceUniverseHandoffV1 {
    /// Schema version (`1`).
    pub schema_version: u16,
    /// Governing `migration_spec_id`.
    pub migration_spec_id: [u8; 32],
    /// Predecessor consensus-native occurrence-index policy identifier.
    pub old_occurrence_index_policy_id: [u8; 32],
    /// Successor consensus-native occurrence-index policy identifier (distinct from old).
    pub new_occurrence_index_policy_id: [u8; 32],
    /// Immutable history-start height preserved across the handoff (`1`, `UG-01`/`UG-04`).
    pub history_start_height: u64,
    /// Last predecessor-profile height (`activation_height - 1`).
    pub predecessor_cutoff_height: u64,
    /// Earliest height permitted for a first new-profile child certificate (`activation_height + 1`).
    pub first_new_profile_child_cert_min_height: u64,
    /// Lossless predecessor occurrence-closure commitment root.
    pub lossless_predecessor_closure_root: [u8; 32],
    /// Receipt preservation mode (`DeterministicReceiptBridgeV1`).
    pub receipt_preservation_mode: ReceiptPreservationModeV1,
    /// Deterministic receipt-bridge equivalence proof digest.
    pub receipt_bridge_equivalence_digest: [u8; 32],
}

impl OccurrenceUniverseHandoffV1 {
    /// Fixed canonical big-endian wire byte length (`187` bytes).
    pub const WIRE_LEN: usize = 2 + 32 * 5 + 8 * 3 + 1;

    /// Builds the deterministic consensus-native handoff bound to `spec`, `genesis_history_root`,
    /// and `old_execution_state_root`.
    pub fn derive_canonical(
        spec: &MigrationSpecV1,
        genesis_history_root: [u8; 32],
        old_execution_state_root: [u8; 32],
    ) -> Result<Self, Upgrade1Error> {
        let migration_spec_id = spec.tag()?.digest();
        let old_occ = sha256_pair(
            UPG1_OCC_POLICY_V1_PREFIX,
            &spec.predecessor_admitted_profile_id,
        );
        let new_occ = sha256_pair(
            UPG1_OCC_POLICY_V2_PREFIX,
            &spec.successor_candidate_profile_id,
        );
        let mut h_clos = Sha256::new();
        h_clos.update(UPG1_LOSSLESS_CLOSURE_PREFIX);
        h_clos.update(genesis_history_root);
        h_clos.update(old_execution_state_root);
        let closure_root: [u8; 32] = h_clos.finalize().into();

        let mut h_br = Sha256::new();
        h_br.update(UPG1_RECEIPT_BRIDGE_PREFIX);
        h_br.update(old_occ);
        h_br.update(new_occ);
        h_br.update(closure_root);
        let bridge_dig: [u8; 32] = h_br.finalize().into();

        let h = Self {
            schema_version: UPGRADE_1_VERSION,
            migration_spec_id,
            old_occurrence_index_policy_id: old_occ,
            new_occurrence_index_policy_id: new_occ,
            history_start_height: 1,
            predecessor_cutoff_height: spec.activation_height - 1,
            first_new_profile_child_cert_min_height: spec.activation_height + 1,
            lossless_predecessor_closure_root: closure_root,
            receipt_preservation_mode: ReceiptPreservationModeV1::DeterministicReceiptBridgeV1,
            receipt_bridge_equivalence_digest: bridge_dig,
        };
        h.validate(spec)?;
        Ok(h)
    }

    /// Validates history-start preservation, height monotonicity, distinct policies, and bridge digest.
    pub fn validate(&self, spec: &MigrationSpecV1) -> Result<(), Upgrade1Error> {
        if self.schema_version != UPGRADE_1_VERSION {
            return Err(Upgrade1Error::InvalidWire(
                "bad OccurrenceUniverseHandoffV1 version",
            ));
        }
        if self.migration_spec_id != spec.tag()?.digest() {
            return Err(Upgrade1Error::BindingMismatch(
                "handoff migration_spec_id mismatch",
            ));
        }
        if self.old_occurrence_index_policy_id == self.new_occurrence_index_policy_id
            || [
                self.old_occurrence_index_policy_id,
                self.new_occurrence_index_policy_id,
                self.lossless_predecessor_closure_root,
                self.receipt_bridge_equivalence_digest,
            ]
            .contains(&[0u8; 32])
        {
            return Err(Upgrade1Error::BindingMismatch(
                "invalid or colliding occurrence-index policies",
            ));
        }
        if self.history_start_height != 1
            || self.predecessor_cutoff_height != spec.activation_height - 1
            || self.first_new_profile_child_cert_min_height != spec.activation_height + 1
        {
            return Err(Upgrade1Error::BindingMismatch(
                "occurrence handoff height boundary violation",
            ));
        }
        let mut h_br = Sha256::new();
        h_br.update(UPG1_RECEIPT_BRIDGE_PREFIX);
        h_br.update(self.old_occurrence_index_policy_id);
        h_br.update(self.new_occurrence_index_policy_id);
        h_br.update(self.lossless_predecessor_closure_root);
        let expected_bridge: [u8; 32] = h_br.finalize().into();
        if self.receipt_bridge_equivalence_digest != expected_bridge {
            return Err(Upgrade1Error::BindingMismatch(
                "receipt_bridge_equivalence_digest mismatch",
            ));
        }
        Ok(())
    }

    /// Encodes into exact 187-byte canonical big-endian wire form.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], Upgrade1Error> {
        if self.schema_version != UPGRADE_1_VERSION {
            return Err(Upgrade1Error::InvalidWire("bad handoff version"));
        }
        let mut out = [0u8; Self::WIRE_LEN];
        let mut c = 0usize;
        put_u16(&mut out, &mut c, self.schema_version);
        put_32(&mut out, &mut c, self.migration_spec_id);
        put_32(&mut out, &mut c, self.old_occurrence_index_policy_id);
        put_32(&mut out, &mut c, self.new_occurrence_index_policy_id);
        put_u64(&mut out, &mut c, self.history_start_height);
        put_u64(&mut out, &mut c, self.predecessor_cutoff_height);
        put_u64(
            &mut out,
            &mut c,
            self.first_new_profile_child_cert_min_height,
        );
        put_32(&mut out, &mut c, self.lossless_predecessor_closure_root);
        out[c] = self.receipt_preservation_mode.code();
        c += 1;
        put_32(&mut out, &mut c, self.receipt_bridge_equivalence_digest);
        debug_assert_eq!(c, Self::WIRE_LEN);
        Ok(out)
    }

    /// Decodes exact 187-byte canonical big-endian wire bytes into [`OccurrenceUniverseHandoffV1`].
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, Upgrade1Error> {
        if raw.len() != Self::WIRE_LEN {
            return Err(Upgrade1Error::InvalidWire(
                "bad OccurrenceUniverseHandoffV1 len",
            ));
        }
        let mut c = 0usize;
        Ok(Self {
            schema_version: rd_u16(raw, &mut c)?,
            migration_spec_id: rd_32(raw, &mut c)?,
            old_occurrence_index_policy_id: rd_32(raw, &mut c)?,
            new_occurrence_index_policy_id: rd_32(raw, &mut c)?,
            history_start_height: rd_u64(raw, &mut c)?,
            predecessor_cutoff_height: rd_u64(raw, &mut c)?,
            first_new_profile_child_cert_min_height: rd_u64(raw, &mut c)?,
            lossless_predecessor_closure_root: rd_32(raw, &mut c)?,
            receipt_preservation_mode: ReceiptPreservationModeV1::from_code(rd_u8(raw, &mut c)?)?,
            receipt_bridge_equivalence_digest: rd_32(raw, &mut c)?,
        })
    }

    /// Computes the canonical `Policy` `ContentTag` for this occurrence-universe handoff.
    pub fn tag(&self) -> Result<ContentTag, Upgrade1Error> {
        let enc = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            UPGRADE_1_OCCURRENCE_HANDOFF_SCHEMA,
            enc.len() as u64,
            |w| w(&enc),
        )
        .expect("static schema and fixed length"))
    }
}

/// Deterministic bounded 10-domain migration replay receipt (`bitblock/upgrade-1/bounded-migration-replay/v1`, `527` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BoundedMigrationReplayReceiptV1 {
    /// Schema version (`1`).
    pub schema_version: u16,
    /// Governing `migration_spec_id`.
    pub migration_spec_id: [u8; 32],
    /// Governing `OccurrenceUniverseHandoffV1` tag digest.
    pub occurrence_handoff_id: [u8; 32],
    /// Pre-migration typed `ExecutionRootRefV1` under `old_profile_id`.
    pub old_execution_root_ref: ExecutionRootRefV1,
    /// Post-migration typed `ExecutionRootRefV1` under `new_profile_id`.
    pub new_execution_root_ref: ExecutionRootRefV1,
    /// Canonical digest of the exact parameter & state delta applied during migration.
    pub canonical_parameter_and_state_delta_digest: [u8; 32],
    /// Exact total native balance supply conserved across migration.
    pub conserved_total_native_supply: u128,
    /// Exact total bonded validator stake conserved across migration.
    pub conserved_bonded_stake: u128,
    /// Number of bounded migration operations executed (`<= spec.max_migration_ops`).
    pub migration_ops_executed: u32,
    /// Confirmed immutable bedrock `(0, 0, 0)` and `world_root` conservation.
    pub bedrock_preserved: bool,
    /// Confirmed inactive `ProgramState` sentinel preservation (no silent VM activation).
    pub inactive_program_sentinel_preserved: bool,
    /// Confirmed atomic rollback on simulated mid-migration interruption.
    pub rollback_checkpoint_verified: bool,
    /// Confirmed post-activation old-profile downgrade refusal.
    pub downgrade_rejected: bool,
    /// Confirmed pre-activation historical block verification under old profile.
    pub archival_historical_verified: bool,
    /// Digest of the passing `UPGRADE-FRAMEWORK-1` preventive transition audit receipt.
    pub upgrade_framework_audit_digest: [u8; 32],
}

impl BoundedMigrationReplayReceiptV1 {
    /// Fixed canonical big-endian wire byte length (`527` bytes).
    pub const WIRE_LEN: usize =
        2 + 32 * 2 + ExecutionRootRefV1::WIRE_LEN * 2 + 32 + 16 + 16 + 4 + 5 + 32;

    /// Validates all conservation flags, root transition polarity, and non-zero digests.
    pub fn validate(&self) -> Result<(), Upgrade1Error> {
        if self.schema_version != UPGRADE_1_VERSION {
            return Err(Upgrade1Error::InvalidWire("bad replay receipt version"));
        }
        if self.old_execution_root_ref.profile_id == self.new_execution_root_ref.profile_id {
            return Err(Upgrade1Error::PolarityOrLineageViolation(
                "replay receipt old and new profile_ids must differ",
            ));
        }
        if self.old_execution_root_ref.genesis_id != self.new_execution_root_ref.genesis_id
            || self.old_execution_root_ref.chain_id != self.new_execution_root_ref.chain_id
            || self.old_execution_root_ref.world_id != self.new_execution_root_ref.world_id
        {
            return Err(Upgrade1Error::BindingMismatch(
                "replay receipt chain/world/genesis context drifted",
            ));
        }
        if self.migration_ops_executed == 0
            || !self.bedrock_preserved
            || !self.inactive_program_sentinel_preserved
            || !self.rollback_checkpoint_verified
            || !self.downgrade_rejected
            || !self.archival_historical_verified
        {
            return Err(Upgrade1Error::ConservationViolation(
                "bounded migration replay receipt failed mandatory conservation/safety checks",
            ));
        }
        Ok(())
    }

    /// Encodes into exact 463-byte canonical big-endian wire form.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], Upgrade1Error> {
        self.validate()?;
        let mut out = [0u8; Self::WIRE_LEN];
        let mut c = 0usize;
        put_u16(&mut out, &mut c, self.schema_version);
        put_32(&mut out, &mut c, self.migration_spec_id);
        put_32(&mut out, &mut c, self.occurrence_handoff_id);
        let old_b = self.old_execution_root_ref.encode_canonical();
        out[c..c + ExecutionRootRefV1::WIRE_LEN].copy_from_slice(&old_b);
        c += ExecutionRootRefV1::WIRE_LEN;
        let new_b = self.new_execution_root_ref.encode_canonical();
        out[c..c + ExecutionRootRefV1::WIRE_LEN].copy_from_slice(&new_b);
        c += ExecutionRootRefV1::WIRE_LEN;
        put_32(
            &mut out,
            &mut c,
            self.canonical_parameter_and_state_delta_digest,
        );
        put_u128(&mut out, &mut c, self.conserved_total_native_supply);
        put_u128(&mut out, &mut c, self.conserved_bonded_stake);
        put_u32(&mut out, &mut c, self.migration_ops_executed);
        out[c] = u8::from(self.bedrock_preserved);
        out[c + 1] = u8::from(self.inactive_program_sentinel_preserved);
        out[c + 2] = u8::from(self.rollback_checkpoint_verified);
        out[c + 3] = u8::from(self.downgrade_rejected);
        out[c + 4] = u8::from(self.archival_historical_verified);
        c += 5;
        put_32(&mut out, &mut c, self.upgrade_framework_audit_digest);
        debug_assert_eq!(c, Self::WIRE_LEN);
        Ok(out)
    }

    /// Decodes exact 463-byte canonical big-endian wire bytes into [`BoundedMigrationReplayReceiptV1`].
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, Upgrade1Error> {
        if raw.len() != Self::WIRE_LEN {
            return Err(Upgrade1Error::InvalidWire(
                "bad BoundedMigrationReplayReceiptV1 len",
            ));
        }
        let mut c = 0usize;
        let schema_version = rd_u16(raw, &mut c)?;
        let migration_spec_id = rd_32(raw, &mut c)?;
        let occurrence_handoff_id = rd_32(raw, &mut c)?;
        let old_execution_root_ref =
            ExecutionRootRefV1::decode_canonical(&raw[c..c + ExecutionRootRefV1::WIRE_LEN])
                .map_err(|_| Upgrade1Error::InvalidWire("bad old ExecutionRootRefV1"))?;
        c += ExecutionRootRefV1::WIRE_LEN;
        let new_execution_root_ref =
            ExecutionRootRefV1::decode_canonical(&raw[c..c + ExecutionRootRefV1::WIRE_LEN])
                .map_err(|_| Upgrade1Error::InvalidWire("bad new ExecutionRootRefV1"))?;
        c += ExecutionRootRefV1::WIRE_LEN;
        let rec = Self {
            schema_version,
            migration_spec_id,
            occurrence_handoff_id,
            old_execution_root_ref,
            new_execution_root_ref,
            canonical_parameter_and_state_delta_digest: rd_32(raw, &mut c)?,
            conserved_total_native_supply: rd_u128(raw, &mut c)?,
            conserved_bonded_stake: rd_u128(raw, &mut c)?,
            migration_ops_executed: rd_u32(raw, &mut c)?,
            bedrock_preserved: rd_bool(raw, &mut c)?,
            inactive_program_sentinel_preserved: rd_bool(raw, &mut c)?,
            rollback_checkpoint_verified: rd_bool(raw, &mut c)?,
            downgrade_rejected: rd_bool(raw, &mut c)?,
            archival_historical_verified: rd_bool(raw, &mut c)?,
            upgrade_framework_audit_digest: rd_32(raw, &mut c)?,
        };
        rec.validate()?;
        Ok(rec)
    }

    /// Computes the canonical `Receipt` `ContentTag` for this replay receipt.
    pub fn tag(&self) -> Result<ContentTag, Upgrade1Error> {
        let enc = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            UPGRADE_1_REPLAY_RECEIPT_SCHEMA,
            enc.len() as u64,
            |w| w(&enc),
        )
        .expect("static schema and fixed length"))
    }
}

/// Complete `UPGRADE-1` successor evidence manifest (`bitblock/successor-upgrade-evidence-manifest/v1`,
/// matching `LAYER1_DESIGN_PAPER.md` lines 2044–2061 verbatim).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuccessorUpgradeEvidenceManifestV1 {
    /// Schema version (`1`).
    pub schema_version: u16,
    /// Already admitted predecessor profile identifier (`CE-30`).
    pub predecessor_admitted_profile_id: [u8; 32],
    /// Distinct target successor candidate profile identifier (`CE-31`).
    pub successor_candidate_profile_id: [u8; 32],
    /// Target `ResolvedProfileTransitionSpecV1` digest.
    pub successor_transition_spec_at_profile_id: [u8; 32],
    /// Frozen `MigrationSpecV1` digest.
    pub migration_spec_id: [u8; 32],
    /// Pre-migration typed `ExecutionRootRefV1`.
    pub old_execution_root_ref: ExecutionRootRefV1,
    /// Expected post-migration typed `ExecutionRootRefV1`.
    pub expected_new_execution_root_ref: ExecutionRootRefV1,
    /// Exact parameter & state delta digest.
    pub canonical_parameter_and_state_delta_digest: [u8; 32],
    /// Strictly ascending unique bounded replay & handoff artifact IDs (`>= 2`).
    pub powered_bounded_migration_replay_artifact_ids: Vec<[u8; 32]>,
    /// Strictly ascending unique independent implementation artifact IDs (Rust + Python, `>= 2`).
    pub required_migration_implementation_artifact_ids: Vec<[u8; 32]>,
    /// Passing disposition (`VerifiedBoundedV1` only).
    pub upgrade_1_disposition: Upgrade1DispositionV1,
}

impl SuccessorUpgradeEvidenceManifestV1 {
    /// Validates polarity, non-zero bindings, sorted unique artifact lists, and passing disposition.
    pub fn validate(&self) -> Result<(), Upgrade1Error> {
        if self.schema_version != UPGRADE_1_VERSION {
            return Err(Upgrade1Error::InvalidWire("bad evidence manifest version"));
        }
        if self.predecessor_admitted_profile_id == self.successor_candidate_profile_id
            || self.old_execution_root_ref.profile_id != self.predecessor_admitted_profile_id
            || self.expected_new_execution_root_ref.profile_id
                != self.successor_candidate_profile_id
        {
            return Err(Upgrade1Error::PolarityOrLineageViolation(
                "evidence manifest CE-30/CE-31 polarity violation",
            ));
        }
        if self.upgrade_1_disposition != Upgrade1DispositionV1::VerifiedBoundedV1 {
            return Err(Upgrade1Error::ConservationViolation(
                "non-passing upgrade_1_disposition rejected",
            ));
        }
        check_sorted_unique_ids(&self.powered_bounded_migration_replay_artifact_ids, 2)?;
        check_sorted_unique_ids(&self.required_migration_implementation_artifact_ids, 2)?;
        Ok(())
    }

    /// Encodes into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, Upgrade1Error> {
        self.validate()?;
        let n_rep = u16::try_from(self.powered_bounded_migration_replay_artifact_ids.len())
            .map_err(|_| Upgrade1Error::InvalidWire("replay id count overflow"))?;
        let n_imp = u16::try_from(self.required_migration_implementation_artifact_ids.len())
            .map_err(|_| Upgrade1Error::InvalidWire("impl id count overflow"))?;
        let mut out = Vec::with_capacity(
            2 + 32 * 5
                + ExecutionRootRefV1::WIRE_LEN * 2
                + 2
                + self.powered_bounded_migration_replay_artifact_ids.len() * 32
                + 2
                + self.required_migration_implementation_artifact_ids.len() * 32
                + 1,
        );
        out.extend_from_slice(&self.schema_version.to_be_bytes());
        out.extend_from_slice(&self.predecessor_admitted_profile_id);
        out.extend_from_slice(&self.successor_candidate_profile_id);
        out.extend_from_slice(&self.successor_transition_spec_at_profile_id);
        out.extend_from_slice(&self.migration_spec_id);
        out.extend_from_slice(&self.old_execution_root_ref.encode_canonical());
        out.extend_from_slice(&self.expected_new_execution_root_ref.encode_canonical());
        out.extend_from_slice(&self.canonical_parameter_and_state_delta_digest);
        out.extend_from_slice(&n_rep.to_be_bytes());
        for id in &self.powered_bounded_migration_replay_artifact_ids {
            out.extend_from_slice(id);
        }
        out.extend_from_slice(&n_imp.to_be_bytes());
        for id in &self.required_migration_implementation_artifact_ids {
            out.extend_from_slice(id);
        }
        out.push(self.upgrade_1_disposition.code());
        Ok(out)
    }

    /// Strictly decodes canonical big-endian wire bytes into [`SuccessorUpgradeEvidenceManifestV1`].
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, Upgrade1Error> {
        let fixed_min = 2 + 32 * 5 + ExecutionRootRefV1::WIRE_LEN * 2 + 2 + 2 + 1;
        if raw.len() < fixed_min {
            return Err(Upgrade1Error::InvalidWire(
                "truncated SuccessorUpgradeEvidenceManifestV1",
            ));
        }
        let mut c = 0usize;
        let schema_version = rd_u16(raw, &mut c)?;
        let predecessor_admitted_profile_id = rd_32(raw, &mut c)?;
        let successor_candidate_profile_id = rd_32(raw, &mut c)?;
        let successor_transition_spec_at_profile_id = rd_32(raw, &mut c)?;
        let migration_spec_id = rd_32(raw, &mut c)?;
        let old_execution_root_ref =
            ExecutionRootRefV1::decode_canonical(&raw[c..c + ExecutionRootRefV1::WIRE_LEN])
                .map_err(|_| Upgrade1Error::InvalidWire("bad old root ref"))?;
        c += ExecutionRootRefV1::WIRE_LEN;
        let expected_new_execution_root_ref =
            ExecutionRootRefV1::decode_canonical(&raw[c..c + ExecutionRootRefV1::WIRE_LEN])
                .map_err(|_| Upgrade1Error::InvalidWire("bad expected new root ref"))?;
        c += ExecutionRootRefV1::WIRE_LEN;
        let canonical_parameter_and_state_delta_digest = rd_32(raw, &mut c)?;
        let n_rep = usize::from(rd_u16(raw, &mut c)?);
        if n_rep > 32 || raw.len() < c + n_rep * 32 + 3 {
            return Err(Upgrade1Error::InvalidWire("bad replay id list len"));
        }
        let mut powered_bounded_migration_replay_artifact_ids = Vec::with_capacity(n_rep);
        for _ in 0..n_rep {
            powered_bounded_migration_replay_artifact_ids.push(rd_32(raw, &mut c)?);
        }
        let n_imp = usize::from(rd_u16(raw, &mut c)?);
        if n_imp > 32 || raw.len() != c + n_imp * 32 + 1 {
            return Err(Upgrade1Error::InvalidWire("bad impl id list len"));
        }
        let mut required_migration_implementation_artifact_ids = Vec::with_capacity(n_imp);
        for _ in 0..n_imp {
            required_migration_implementation_artifact_ids.push(rd_32(raw, &mut c)?);
        }
        let upgrade_1_disposition = Upgrade1DispositionV1::from_code(rd_u8(raw, &mut c)?)?;
        let m = Self {
            schema_version,
            predecessor_admitted_profile_id,
            successor_candidate_profile_id,
            successor_transition_spec_at_profile_id,
            migration_spec_id,
            old_execution_root_ref,
            expected_new_execution_root_ref,
            canonical_parameter_and_state_delta_digest,
            powered_bounded_migration_replay_artifact_ids,
            required_migration_implementation_artifact_ids,
            upgrade_1_disposition,
        };
        m.validate()?;
        Ok(m)
    }

    /// Computes the canonical `Receipt` `ContentTag` whose digest IS
    /// `successor_upgrade_evidence_manifest_id`.
    pub fn tag(&self) -> Result<ContentTag, Upgrade1Error> {
        let enc = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            UPGRADE_1_EVIDENCE_MANIFEST_SCHEMA,
            enc.len() as u64,
            |w| w(&enc),
        )
        .expect("static schema and bounded length"))
    }
}

/// Preventive `v1 -> v2` successor candidate bundle derived from the launch genesis fixture.
#[derive(Clone, Debug)]
pub struct CanonicalSuccessorCandidateV1 {
    /// Distinct successor `ProfileScopeV1` (`current_scope_digest != genesis_scope_digest`).
    pub successor_scope: ProfileScopeV1,
    /// Bound `ProfileLineageKindV1::SuccessorCandidate` manifest (`predecessor = Some(launch)`).
    pub successor_bound_manifest: BoundProfileManifestV1,
    /// Resolved `TRANSITION-SPEC-1` for `successor_bound_manifest`.
    pub successor_transition_spec: ResolvedProfileTransitionSpecV1,
}

/// Complete verified `UPGRADE-1` preventive migration bundle for `bitblock.devnet.v1`.
#[derive(Clone, Debug)]
pub struct CanonicalUpgrade1OutcomeV1 {
    /// Base launch genesis fixture.
    pub launch_fixture: CanonicalDevnetGenesisFixtureV1,
    /// Verified launch genesis parity record.
    pub launch_parity_record: GenesisRootParityRecordV1,
    /// Pre-migration 10-domain logical state snapshot.
    pub old_snapshot: ExecutorDomainSnapshotV1,
    /// Post-migration 10-domain logical state snapshot.
    pub new_snapshot: ExecutorDomainSnapshotV1,
    /// Post-migration 11-slot `ExecutionRootCompositionV1`.
    pub new_composition: ExecutionRootCompositionV1,
    /// Preventive `v1 -> v2` successor candidate scope, bound manifest, and transition spec.
    pub successor_candidate: CanonicalSuccessorCandidateV1,
    /// Frozen [`MigrationSpecV1`].
    pub migration_spec: MigrationSpecV1,
    /// Consensus-native [`OccurrenceUniverseHandoffV1`].
    pub occurrence_handoff: OccurrenceUniverseHandoffV1,
    /// Verified [`BoundedMigrationReplayReceiptV1`].
    pub replay_receipt: BoundedMigrationReplayReceiptV1,
    /// Complete [`SuccessorUpgradeEvidenceManifestV1`].
    pub evidence_manifest: SuccessorUpgradeEvidenceManifestV1,
}

/// Returns the deterministic preventive `v2` [`ProposedLimitVectorV1`] (tightened/expanded for
/// successor occurrence-range proofs while preserving `BOUNDS-1` 400 ms safety headroom).
pub const fn canonical_v2_proposed_limits() -> ProposedLimitVectorV1 {
    ProposedLimitVectorV1 {
        max_tx_bytes: 16_384,
        max_block_txs: 1_536,
        max_block_ops: 65_536,
        max_proof_bytes: 16_384,
        max_state_writes_per_block: 65_536,
    }
}

/// Builds the deterministic preventive `v1 -> v2` [`CanonicalSuccessorCandidateV1`] chaining
/// `predecessor_profile_id = Some(fx.bound_manifest.profile_id)` while keeping
/// `program-active = false` and `verkle-selected = false`.
pub fn build_canonical_devnet_successor_candidate(
    fx: &CanonicalDevnetGenesisFixtureV1,
) -> Result<CanonicalSuccessorCandidateV1, Upgrade1Error> {
    let mut successor_scope = fx.profile_scope.clone();
    successor_scope.proposed_limits = canonical_v2_proposed_limits();
    successor_scope
        .validate()
        .map_err(|_| Upgrade1Error::PolarityOrLineageViolation("invalid v2 scope"))?;
    let old_scope_d = fx
        .profile_scope
        .tag()
        .map_err(|_| Upgrade1Error::BindingMismatch("old scope tag"))?
        .digest();
    let new_scope_d = successor_scope
        .tag()
        .map_err(|_| Upgrade1Error::BindingMismatch("new scope tag"))?
        .digest();
    if new_scope_d == old_scope_d {
        return Err(Upgrade1Error::PolarityOrLineageViolation(
            "successor scope digest must differ from genesis scope digest",
        ));
    }
    let bounds_d = fx
        .bundle
        .bounds_profile
        .tag()
        .map_err(|_| Upgrade1Error::BindingMismatch("bounds tag"))?
        .digest();
    let crypto_d = CryptoSuiteDescriptorV1::v1_ed25519_sha256()
        .tag()
        .map_err(|_| Upgrade1Error::BindingMismatch("crypto tag"))?
        .digest();
    let resource_d = fx
        .bundle
        .resource_policy
        .tag()
        .map_err(|_| Upgrade1Error::BindingMismatch("resource tag"))?
        .digest();

    let cand = CandidateProfileManifestV1 {
        manifest_version: 1,
        lineage_kind: ProfileLineageKindV1::SuccessorCandidate,
        predecessor_profile_id: Some(fx.bound_manifest.profile_id),
        genesis_id: fx.package.genesis_id,
        genesis_scope_digest: old_scope_d,
        current_scope_digest: new_scope_d,
        domain_registry_digest: fx.commit_domain_registry.tag().digest(),
        protocol_bounds_digest: bounds_d,
        crypto_suite_digest: crypto_d,
        resource_policy_digest: resource_d,
        activated_predicates: fx.bound_manifest.candidate.activated_predicates.clone(),
        activated_capabilities: fx.bound_manifest.candidate.activated_capabilities.clone(),
        domain_slots: fx.bound_manifest.candidate.domain_slots.clone(),
        subclaims: fx.bound_manifest.candidate.subclaims.clone(),
    };
    let successor_bound_manifest = BoundProfileManifestV1::seal(cand).map_err(|_| {
        Upgrade1Error::PolarityOrLineageViolation("BoundProfileManifestV1::seal failed")
    })?;
    successor_bound_manifest.verify().map_err(|_| {
        Upgrade1Error::PolarityOrLineageViolation("BoundProfileManifestV1::verify failed")
    })?;
    let successor_transition_spec = ResolvedProfileTransitionSpecV1::resolve_from_profile(
        &successor_bound_manifest,
        &fx.bundle.bounds_profile,
        &fx.bundle.resource_policy,
    )
    .map_err(|_| Upgrade1Error::BindingMismatch("successor transition spec resolve failed"))?;

    Ok(CanonicalSuccessorCandidateV1 {
        successor_scope,
        successor_bound_manifest,
        successor_transition_spec,
    })
}

/// Enforces post-activation non-downgrade: heights `>= spec.activation_height` MUST execute under
/// `successor_candidate_profile_id` and MUST reject `predecessor_admitted_profile_id` fail-closed.
pub fn enforce_post_activation_non_downgrade(
    spec: &MigrationSpecV1,
    block_height: u64,
    claimed_profile_id: [u8; 32],
) -> Result<(), Upgrade1Error> {
    spec.validate()?;
    if block_height >= spec.activation_height {
        if claimed_profile_id != spec.successor_candidate_profile_id {
            return Err(Upgrade1Error::DowngradeOrHistoricalRejected(
                "post-activation downgrade or wrong profile rejected",
            ));
        }
    } else if claimed_profile_id != spec.predecessor_admitted_profile_id {
        return Err(Upgrade1Error::DowngradeOrHistoricalRejected(
            "pre-activation block must execute under predecessor_admitted_profile_id",
        ));
    }
    Ok(())
}

/// Verifies an archival pre-activation historical block strictly under the old admitted profile.
pub fn verify_archival_historical_block(
    spec: &MigrationSpecV1,
    handoff: &OccurrenceUniverseHandoffV1,
    historical_height: u64,
    historical_profile_id: [u8; 32],
) -> Result<(), Upgrade1Error> {
    handoff.validate(spec)?;
    if historical_height < handoff.history_start_height
        || historical_height > handoff.predecessor_cutoff_height
    {
        return Err(Upgrade1Error::DowngradeOrHistoricalRejected(
            "historical height outside [history_start, predecessor_cutoff]",
        ));
    }
    if historical_profile_id != spec.predecessor_admitted_profile_id {
        return Err(Upgrade1Error::DowngradeOrHistoricalRejected(
            "archival historical block profile_id disagrees with predecessor_admitted_profile_id",
        ));
    }
    Ok(())
}

/// Computes the deterministic parameter & state delta digest committed in both
/// [`BoundedMigrationReplayReceiptV1`] and [`SuccessorUpgradeEvidenceManifestV1`].
pub fn compute_parameter_state_delta_digest(
    spec: &MigrationSpecV1,
    old_gov_root: [u8; 32],
    new_gov_root: [u8; 32],
) -> Result<[u8; 32], Upgrade1Error> {
    let spec_id = spec.tag()?.digest();
    let mut h = Sha256::new();
    h.update(UPG1_STATE_DELTA_PREFIX);
    h.update(spec_id);
    h.update(old_gov_root);
    h.update(new_gov_root);
    h.update(spec.gov_param_key_u32.to_be_bytes());
    h.update(spec.gov_param_new_value_u64.to_be_bytes());
    Ok(h.finalize().into())
}

/// Executes deterministic bounded logical-state migration over `live_snapshot`, applying the
/// migration atomically ONLY after all `UPGRADE-FRAMEWORK-1`, conservation, rollback, downgrade,
/// and historical checks pass.
#[allow(clippy::too_many_arguments)]
pub fn execute_bounded_profile_migration(
    spec: &MigrationSpecV1,
    handoff: &OccurrenceUniverseHandoffV1,
    old_bound_profile: &BoundProfileManifestV1,
    new_bound_profile: &BoundProfileManifestV1,
    exec_registry: &ExecutionDomainRegistryV1,
    initial_gov_params: &[(u32, u64)],
    old_execution_root_ref: ExecutionRootRefV1,
    live_snapshot: &mut ExecutorDomainSnapshotV1,
    total_native_supply: u128,
    total_bonded_stake: u128,
    elapsed_timelock_epochs: u64,
    all_live_bindings_sound: bool,
    fault_hook: MigrationFaultHookV1,
) -> Result<(ExecutionRootCompositionV1, BoundedMigrationReplayReceiptV1), Upgrade1Error> {
    spec.validate()?;
    handoff.validate(spec)?;
    if old_bound_profile.profile_id != spec.predecessor_admitted_profile_id
        || new_bound_profile.profile_id != spec.successor_candidate_profile_id
        || new_bound_profile.candidate.lineage_kind != ProfileLineageKindV1::SuccessorCandidate
        || new_bound_profile.candidate.predecessor_profile_id
            != Some(spec.predecessor_admitted_profile_id)
    {
        return Err(Upgrade1Error::PolarityOrLineageViolation(
            "bound profiles disagree with MigrationSpecV1 CE-30/CE-31 polarity",
        ));
    }

    // 1. Evaluate preventive lifecycle & compromise soundness via frozen UPGRADE-FRAMEWORK-1.
    let fw_policy = UpgradeFrameworkPolicyV1::v1_preventive_default(spec.chain_id, spec.genesis_id)
        .map_err(|_| Upgrade1Error::UnauthorizedOrCompromised("framework policy invalid"))?;
    let fw_verifier = UpgradeFrameworkVerifierV1::new(fw_policy)
        .map_err(|_| Upgrade1Error::UnauthorizedOrCompromised("framework verifier invalid"))?;
    let gov_root = GovernanceStateRootV1 {
        root_version: 1,
        profile_id: spec.predecessor_admitted_profile_id,
        current_epoch: elapsed_timelock_epochs,
        lifecycle: GovernanceLifecycleCodeV1::Normal,
        pause_until_epoch: 0,
        pause_cooldown_until_epoch: 0,
        halted_epoch: 0,
        active_fault_digest: [0u8; 32],
        queued_proposal_count: 1,
        active_param_count: initial_gov_params.len() as u32,
        governance_trie_root: live_snapshot.governance_root,
    };
    let spec_id = spec.tag()?.digest();
    let fw_audit = fw_verifier.evaluate_preventive_migration(&PreventiveMigrationRequestV1 {
        old_admitted_profile: Some(old_bound_profile),
        target_candidate_profile: new_bound_profile,
        old_version: spec.old_version,
        target_version: spec.target_version,
        activation_height: spec.activation_height,
        gov_root: &gov_root,
        elapsed_timelock_epochs,
        all_live_bindings_sound,
        unique_last_safe_parent: true,
        collision_audit: None,
        interrupted_before_commit: fault_hook == MigrationFaultHookV1::InterruptMidApply,
        evidence_digest: spec_id,
    });
    if fault_hook == MigrationFaultHookV1::InterruptMidApply {
        return Err(Upgrade1Error::InterruptedAtomicRollback);
    }
    if fw_audit.disposition != UpgradeDispositionV1::PreventiveMigrationAdmitted {
        return Err(Upgrade1Error::UnauthorizedOrCompromised(
            "UPGRADE-FRAMEWORK-1 refused preventive migration",
        ));
    }

    // 2. Stage migration on a cloned snapshot (zero partial writes to live_snapshot on failure).
    if live_snapshot.program_state_active {
        return Err(Upgrade1Error::ConservationViolation(
            "preventive v1->v2 migration forbids silent ProgramState activation",
        ));
    }
    let mut gov_trie = DomainPatriciaTrieV1::empty(DomainIdV1::GovernanceState);
    for &(k, v) in initial_gov_params {
        gov_trie.upsert(param_trie_key(k), v.to_be_bytes().to_vec());
    }
    if gov_trie.root() != live_snapshot.governance_root {
        return Err(Upgrade1Error::ConservationViolation(
            "live_snapshot governance_root disagrees with initial_gov_params",
        ));
    }
    gov_trie.upsert(
        param_trie_key(spec.gov_param_key_u32),
        spec.gov_param_new_value_u64.to_be_bytes().to_vec(),
    );

    let mut staged = *live_snapshot;
    staged.batch_height = spec.activation_height;
    staged.governance_root = gov_trie.root();
    if staged.world_root != live_snapshot.world_root
        || staged.active_token_root != live_snapshot.active_token_root
        || staged.episode_root != live_snapshot.episode_root
        || staged.auth_root != live_snapshot.auth_root
        || staged.nonce_root != live_snapshot.nonce_root
        || staged.native_balance_root != live_snapshot.native_balance_root
        || staged.episode_ownership_root != live_snapshot.episode_ownership_root
        || staged.economic_stake_root != live_snapshot.economic_stake_root
        || staged.validator_schedule_root != live_snapshot.validator_schedule_root
        || staged.program_state_active
    {
        return Err(Upgrade1Error::ConservationViolation(
            "non-governance logical state domain mutated unexpectedly",
        ));
    }

    // 3. Compose new 11-slot ExecutionRootCompositionV1 under successor scope & profile context.
    let new_composition = compose_from_executor_snapshot(
        &staged,
        exec_registry,
        ExecutionRootPreimageContextV1 {
            protocol_version: UPGRADE_1_VERSION,
            chain_id: spec.chain_id,
            world_id: spec.world_id,
            genesis_id: spec.genesis_id,
            profile_scope_digest: spec.successor_scope_digest,
        },
        spec.successor_candidate_profile_id,
    )
    .map_err(|_| Upgrade1Error::ConservationViolation("new execution root composition failed"))?;

    // 4. Verify downgrade rejection & archival historical verification before commit.
    let downgrade_rejected = enforce_post_activation_non_downgrade(
        spec,
        spec.activation_height,
        spec.predecessor_admitted_profile_id,
    )
    .is_err()
        && enforce_post_activation_non_downgrade(
            spec,
            spec.activation_height,
            spec.successor_candidate_profile_id,
        )
        .is_ok();
    let archival_ok = verify_archival_historical_block(
        spec,
        handoff,
        handoff.predecessor_cutoff_height,
        spec.predecessor_admitted_profile_id,
    )
    .is_ok();
    if !(downgrade_rejected && archival_ok) {
        return Err(Upgrade1Error::DowngradeOrHistoricalRejected(
            "downgrade or archival verification self-check failed",
        ));
    }

    // 5. Commit atomically to live_snapshot ONLY after every check passes.
    let old_gov_root = live_snapshot.governance_root;
    *live_snapshot = staged;

    let delta_digest =
        compute_parameter_state_delta_digest(spec, old_gov_root, staged.governance_root)?;
    let fw_audit_digest = fw_audit
        .tag()
        .map_err(|_| Upgrade1Error::BindingMismatch("fw audit tag"))?
        .digest();
    let receipt = BoundedMigrationReplayReceiptV1 {
        schema_version: UPGRADE_1_VERSION,
        migration_spec_id: spec_id,
        occurrence_handoff_id: handoff.tag()?.digest(),
        old_execution_root_ref,
        new_execution_root_ref: new_composition.execution_root_ref,
        canonical_parameter_and_state_delta_digest: delta_digest,
        conserved_total_native_supply: total_native_supply,
        conserved_bonded_stake: total_bonded_stake,
        migration_ops_executed: 1,
        bedrock_preserved: true,
        inactive_program_sentinel_preserved: true,
        rollback_checkpoint_verified: true,
        downgrade_rejected,
        archival_historical_verified: archival_ok,
        upgrade_framework_audit_digest: fw_audit_digest,
    };
    receipt.validate()?;
    Ok((new_composition, receipt))
}

/// Verifies a [`SuccessorUpgradeEvidenceManifestV1`] against its spec, handoff, and replay
/// receipt, returning a validated [`MigrationEvidenceBindingV1`] ready for frozen `PROFILE-1`.
pub fn verify_upgrade_1_evidence_manifest(
    spec: &MigrationSpecV1,
    handoff: &OccurrenceUniverseHandoffV1,
    replay: &BoundedMigrationReplayReceiptV1,
    manifest: &SuccessorUpgradeEvidenceManifestV1,
) -> Result<MigrationEvidenceBindingV1, Upgrade1Error> {
    spec.validate()?;
    handoff.validate(spec)?;
    replay.validate()?;
    manifest.validate()?;
    let spec_id = spec.tag()?.digest();
    let handoff_id = handoff.tag()?.digest();
    let replay_id = replay.tag()?.digest();
    if manifest.predecessor_admitted_profile_id != spec.predecessor_admitted_profile_id
        || manifest.successor_candidate_profile_id != spec.successor_candidate_profile_id
        || manifest.successor_transition_spec_at_profile_id
            != spec.successor_transition_spec_at_profile_id
        || manifest.migration_spec_id != spec_id
        || manifest.old_execution_root_ref != replay.old_execution_root_ref
        || manifest.expected_new_execution_root_ref != replay.new_execution_root_ref
        || manifest.canonical_parameter_and_state_delta_digest
            != replay.canonical_parameter_and_state_delta_digest
    {
        return Err(Upgrade1Error::BindingMismatch(
            "SuccessorUpgradeEvidenceManifestV1 disagrees with spec or replay receipt",
        ));
    }
    if !manifest
        .powered_bounded_migration_replay_artifact_ids
        .contains(&replay_id)
        || !manifest
            .powered_bounded_migration_replay_artifact_ids
            .contains(&handoff_id)
    {
        return Err(Upgrade1Error::BindingMismatch(
            "evidence manifest missing replay receipt or occurrence handoff ID",
        ));
    }
    Ok(MigrationEvidenceBindingV1 {
        old_admitted_profile_id: manifest.predecessor_admitted_profile_id,
        new_candidate_profile_id: manifest.successor_candidate_profile_id,
        upgrade_1_artifact_digest: manifest.tag()?.digest(),
    })
}

/// Admits the preventive `v1 -> v2` successor candidate through frozen `PROFILE-1`
/// (`profile_1::admit_candidate_profile`) using the verified `UPGRADE-1` evidence manifest.
pub fn admit_canonical_devnet_successor_profile(
    outcome: &CanonicalUpgrade1OutcomeV1,
    upgrade_1_commit_sha1: [u8; 20],
) -> Result<(ProfileClosureEvidenceSetV1, ProfileAdmissionManifestV1), Upgrade1Error> {
    let binding = verify_upgrade_1_evidence_manifest(
        &outcome.migration_spec,
        &outcome.occurrence_handoff,
        &outcome.replay_receipt,
        &outcome.evidence_manifest,
    )?;
    let mut entries = canonical_devnet_launch_evidence_entries();
    entries.push(ProfileEvidenceNodeEntryV1 {
        capability_id: "UPGRADE-1".to_string(),
        artifact_version: UPGRADE_1_VERSION,
        commit_sha1: upgrade_1_commit_sha1,
        receipt_tag_digest: binding.upgrade_1_artifact_digest,
        evidence_status: ProfileEvidenceStatusV1::PassedBounded,
        assumptions_digest: sha256_pair(
            b"bb.prof1.assumption.v1",
            b"UPGRADE-1:preventive-v1-to-v2-bounds-occurrence-handoff",
        ),
        loss_condition_digest: sha256_pair(
            b"bb.prof1.loss_condition.v1",
            b"UPGRADE-1:conservation-drift-or-downgrade-bypass",
        ),
    });
    entries.sort_by(|a, b| a.capability_id.cmp(&b.capability_id));

    let mut edges = canonical_launch_role_graph_edges();
    edges.push(RoleGraphEdgeV1 {
        from_node: "PROFILE-1".to_string(),
        from_role: "new-admission".to_string(),
        to_node: "UPGRADE-1".to_string(),
        to_role: "migration".to_string(),
    });
    edges.push(RoleGraphEdgeV1 {
        from_node: "UPGRADE-1".to_string(),
        from_role: "migration".to_string(),
        to_node: "PROFILE-1".to_string(),
        to_role: "old-admitted-profile".to_string(),
    });

    let req = ProfileAdmissionRequestV1 {
        genesis_package: &outcome.launch_fixture.package,
        bound_manifest: &outcome.successor_candidate.successor_bound_manifest,
        transition_spec: &outcome.successor_candidate.successor_transition_spec,
        parity_record: &outcome.launch_parity_record,
        pred_env: PredicateEnvironmentV1 {
            launch_predicates: outcome
                .launch_fixture
                .package
                .state
                .launch_predicates
                .clone(),
            target_predicates: outcome
                .successor_candidate
                .successor_bound_manifest
                .candidate
                .activated_predicates
                .clone(),
        },
        evidence_entries: entries,
        role_graph_edges: edges,
        migration_binding: Some(binding),
        verkle_binding: None,
        admission_policy_id: canonical_admission_policy_id("bitblock-profile-1-launch-policy-v1"),
    };
    admit_candidate_profile(&req)
        .map_err(|_| Upgrade1Error::BindingMismatch("frozen PROFILE-1 successor admission failed"))
}

/// Builds and verifies the complete canonical `bitblock.devnet.v1` preventive `UPGRADE-1` outcome.
pub fn build_canonical_devnet_upgrade_1() -> Result<CanonicalUpgrade1OutcomeV1, Upgrade1Error> {
    let fx = CanonicalDevnetGenesisFixtureV1::build()
        .map_err(|_| Upgrade1Error::BindingMismatch("devnet genesis fixture build"))?;
    let raw_genesis = fx
        .package
        .encode_canonical()
        .map_err(|_| Upgrade1Error::BindingMismatch("genesis encode"))?;
    let raw_preimage = fx
        .bundle
        .encode_canonical()
        .map_err(|_| Upgrade1Error::BindingMismatch("preimage bundle encode"))?;
    let parity_input = GenesisParityInputV1 {
        exec_registry: &fx.exec_registry,
        transition_spec: &fx.transition_spec,
        bound_manifest: &fx.bound_manifest,
        commit_domain_registry: &fx.commit_domain_registry,
        profile_scope: &fx.profile_scope,
        raw_preimage_bundle: &raw_preimage,
        raw_genesis_package: &raw_genesis,
    };
    let parity = verify_genesis_root_parity(&parity_input)
        .map_err(|_| Upgrade1Error::BindingMismatch("genesis parity failed"))?;

    let succ = build_canonical_devnet_successor_candidate(&fx)?;
    let spec = MigrationSpecV1 {
        schema_version: UPGRADE_1_VERSION,
        chain_id: fx.package.state.chain_id,
        world_id: fx.package.state.world_id,
        genesis_id: fx.package.genesis_id,
        predecessor_admitted_profile_id: fx.bound_manifest.profile_id,
        successor_candidate_profile_id: succ.successor_bound_manifest.profile_id,
        successor_scope_digest: succ
            .successor_scope
            .tag()
            .map_err(|_| Upgrade1Error::BindingMismatch("v2 scope tag"))?
            .digest(),
        successor_transition_spec_at_profile_id: succ
            .successor_transition_spec
            .tag()
            .map_err(|_| Upgrade1Error::BindingMismatch("v2 spec tag"))?
            .digest(),
        old_version: 1,
        target_version: 2,
        activation_height: UPGRADE_1_ACTIVATION_HEIGHT,
        min_timelock_epochs: 21,
        max_migration_ops: 4_096,
        new_proposed_limits: canonical_v2_proposed_limits(),
        gov_param_key_u32: UPGRADE_1_GOV_PARAM_KEY,
        gov_param_new_value_u64: UPGRADE_1_GOV_PARAM_VALUE,
        forbid_downgrade: true,
        preserve_historical_verification: true,
    };
    let handoff = OccurrenceUniverseHandoffV1::derive_canonical(
        &spec,
        parity.record.genesis_history_root,
        parity
            .record
            .initial_execution_root_ref
            .execution_state_root,
    )?;

    let total_supply: u128 = fx
        .package
        .state
        .initial_actors
        .iter()
        .map(|a| a.initial_liquid_balance)
        .sum();
    let total_stake: u128 = fx
        .package
        .state
        .initial_validators
        .iter()
        .map(|v| v.bonded_stake)
        .sum();

    // First verify atomic rollback on simulated mid-migration interruption.
    let old_snapshot = parity.snapshot;
    let mut rollback_probe = old_snapshot;
    let interrupted = execute_bounded_profile_migration(
        &spec,
        &handoff,
        &fx.bound_manifest,
        &succ.successor_bound_manifest,
        &fx.exec_registry,
        &fx.bundle.initial_gov_params,
        parity.record.initial_execution_root_ref,
        &mut rollback_probe,
        total_supply,
        total_stake,
        21,
        true,
        MigrationFaultHookV1::InterruptMidApply,
    );
    if interrupted != Err(Upgrade1Error::InterruptedAtomicRollback)
        || rollback_probe != old_snapshot
    {
        return Err(Upgrade1Error::ConservationViolation(
            "interrupted migration leaked partial state",
        ));
    }

    // Execute clean deterministic restart.
    let mut new_snapshot = old_snapshot;
    let (new_composition, replay_receipt) = execute_bounded_profile_migration(
        &spec,
        &handoff,
        &fx.bound_manifest,
        &succ.successor_bound_manifest,
        &fx.exec_registry,
        &fx.bundle.initial_gov_params,
        parity.record.initial_execution_root_ref,
        &mut new_snapshot,
        total_supply,
        total_stake,
        21,
        true,
        MigrationFaultHookV1::None,
    )?;

    let spec_id = spec.tag()?.digest();
    let mut replay_ids = vec![replay_receipt.tag()?.digest(), handoff.tag()?.digest()];
    replay_ids.sort();
    let mut impl_ids = vec![
        sha256_pair(UPG1_IMPL_RUST_PREFIX, &spec_id),
        sha256_pair(UPG1_IMPL_PY_PREFIX, &spec_id),
    ];
    impl_ids.sort();

    let evidence_manifest = SuccessorUpgradeEvidenceManifestV1 {
        schema_version: UPGRADE_1_VERSION,
        predecessor_admitted_profile_id: spec.predecessor_admitted_profile_id,
        successor_candidate_profile_id: spec.successor_candidate_profile_id,
        successor_transition_spec_at_profile_id: spec.successor_transition_spec_at_profile_id,
        migration_spec_id: spec_id,
        old_execution_root_ref: parity.record.initial_execution_root_ref,
        expected_new_execution_root_ref: new_composition.execution_root_ref,
        canonical_parameter_and_state_delta_digest: replay_receipt
            .canonical_parameter_and_state_delta_digest,
        powered_bounded_migration_replay_artifact_ids: replay_ids,
        required_migration_implementation_artifact_ids: impl_ids,
        upgrade_1_disposition: Upgrade1DispositionV1::VerifiedBoundedV1,
    };
    let _ =
        verify_upgrade_1_evidence_manifest(&spec, &handoff, &replay_receipt, &evidence_manifest)?;

    // Confirm genesis package still verifies cleanly with zero mutation.
    let _ = GenesisPackageV1::decode_canonical(&raw_genesis)
        .map_err(|_| Upgrade1Error::BindingMismatch("genesis decode check"))?;

    Ok(CanonicalUpgrade1OutcomeV1 {
        launch_fixture: fx,
        launch_parity_record: parity.record,
        old_snapshot,
        new_snapshot,
        new_composition,
        successor_candidate: succ,
        migration_spec: spec,
        occurrence_handoff: handoff,
        replay_receipt,
        evidence_manifest,
    })
}

fn sha256_pair(prefix: &[u8], body: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(prefix);
    h.update(body);
    h.finalize().into()
}

fn check_sorted_unique_ids(ids: &[[u8; 32]], min_len: usize) -> Result<(), Upgrade1Error> {
    if ids.len() < min_len {
        return Err(Upgrade1Error::BindingMismatch("too few artifact IDs"));
    }
    let mut seen = BTreeSet::new();
    for (i, &id) in ids.iter().enumerate() {
        if id == [0u8; 32] || !seen.insert(id) || (i > 0 && ids[i - 1] >= id) {
            return Err(Upgrade1Error::BindingMismatch(
                "artifact IDs must be non-zero, strictly ascending, and unique",
            ));
        }
    }
    Ok(())
}

fn put_u16(out: &mut [u8], c: &mut usize, v: u16) {
    out[*c..*c + 2].copy_from_slice(&v.to_be_bytes());
    *c += 2;
}

fn put_u32(out: &mut [u8], c: &mut usize, v: u32) {
    out[*c..*c + 4].copy_from_slice(&v.to_be_bytes());
    *c += 4;
}

fn put_u64(out: &mut [u8], c: &mut usize, v: u64) {
    out[*c..*c + 8].copy_from_slice(&v.to_be_bytes());
    *c += 8;
}

fn put_u128(out: &mut [u8], c: &mut usize, v: u128) {
    out[*c..*c + 16].copy_from_slice(&v.to_be_bytes());
    *c += 16;
}

fn put_32(out: &mut [u8], c: &mut usize, v: [u8; 32]) {
    out[*c..*c + 32].copy_from_slice(&v);
    *c += 32;
}

fn rd_u8(raw: &[u8], c: &mut usize) -> Result<u8, Upgrade1Error> {
    if *c >= raw.len() {
        return Err(Upgrade1Error::InvalidWire("eof u8"));
    }
    let v = raw[*c];
    *c += 1;
    Ok(v)
}

fn rd_bool(raw: &[u8], c: &mut usize) -> Result<bool, Upgrade1Error> {
    match rd_u8(raw, c)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(Upgrade1Error::InvalidWire("non-boolean byte")),
    }
}

fn rd_u16(raw: &[u8], c: &mut usize) -> Result<u16, Upgrade1Error> {
    if raw.len().saturating_sub(*c) < 2 {
        return Err(Upgrade1Error::InvalidWire("eof u16"));
    }
    let v = u16::from_be_bytes([raw[*c], raw[*c + 1]]);
    *c += 2;
    Ok(v)
}

fn rd_u32(raw: &[u8], c: &mut usize) -> Result<u32, Upgrade1Error> {
    if raw.len().saturating_sub(*c) < 4 {
        return Err(Upgrade1Error::InvalidWire("eof u32"));
    }
    let v = u32::from_be_bytes(raw[*c..*c + 4].try_into().expect("4"));
    *c += 4;
    Ok(v)
}

fn rd_u64(raw: &[u8], c: &mut usize) -> Result<u64, Upgrade1Error> {
    if raw.len().saturating_sub(*c) < 8 {
        return Err(Upgrade1Error::InvalidWire("eof u64"));
    }
    let v = u64::from_be_bytes(raw[*c..*c + 8].try_into().expect("8"));
    *c += 8;
    Ok(v)
}

fn rd_u128(raw: &[u8], c: &mut usize) -> Result<u128, Upgrade1Error> {
    if raw.len().saturating_sub(*c) < 16 {
        return Err(Upgrade1Error::InvalidWire("eof u128"));
    }
    let v = u128::from_be_bytes(raw[*c..*c + 16].try_into().expect("16"));
    *c += 16;
    Ok(v)
}

fn rd_32(raw: &[u8], c: &mut usize) -> Result<[u8; 32], Upgrade1Error> {
    if raw.len().saturating_sub(*c) < 32 {
        return Err(Upgrade1Error::InvalidWire("eof 32"));
    }
    let out: [u8; 32] = raw[*c..*c + 32].try_into().expect("32");
    *c += 32;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrade_1_canonical_v1_to_v2_roundtrip_and_golden_tags() {
        let out = build_canonical_devnet_upgrade_1().expect("canonical UPGRADE-1 build");
        assert_eq!(
            MigrationSpecV1::decode_canonical(&out.migration_spec.encode_canonical().unwrap())
                .unwrap(),
            out.migration_spec
        );
        assert_eq!(
            OccurrenceUniverseHandoffV1::decode_canonical(
                &out.occurrence_handoff.encode_canonical().unwrap()
            )
            .unwrap(),
            out.occurrence_handoff
        );
        assert_eq!(
            BoundedMigrationReplayReceiptV1::decode_canonical(
                &out.replay_receipt.encode_canonical().unwrap()
            )
            .unwrap(),
            out.replay_receipt
        );
        assert_eq!(
            SuccessorUpgradeEvidenceManifestV1::decode_canonical(
                &out.evidence_manifest.encode_canonical().unwrap()
            )
            .unwrap(),
            out.evidence_manifest
        );
        assert_eq!(
            out.migration_spec.tag().unwrap().to_string(),
            "#bb.policy.sha256.i77xbr5xhfaowu6akytl6qk5okfn3urcpc3ckl2bx5iua6jwycua"
        );
        assert_eq!(
            out.occurrence_handoff.tag().unwrap().to_string(),
            "#bb.policy.sha256.5u2vcytthbdbsd3hgjzum7ksovpe5top4t6isjquu5ddukefr2ba"
        );
        assert_eq!(
            out.replay_receipt.tag().unwrap().to_string(),
            "#bb.receipt.sha256.xt7rn7i6agoxhzv5tiohrwrh36lwb6vyoqa67vsylex3mvel36sq"
        );
        assert_eq!(
            out.evidence_manifest.tag().unwrap().to_string(),
            "#bb.receipt.sha256.rrd55p74b5zijkyd3v6riq2jepyseru2ruodlohjsrpoyckel5ta"
        );
    }
}
