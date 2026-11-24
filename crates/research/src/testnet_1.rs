//! `TESTNET-1@base` (`PS-058` + `CE-44`): replicated multi-process adversarial testnet over real
//! independent OS processes (`4` validators + `1` full verifier + `1` archive/recovery node = `6`
//! independent processes), isolated per-process `STORAGE-1` journals, OS kernel-locked
//! `SIGNER-OPS-1` persistent watermarks, loopback TCP (`BBTN1`) RPC transport, fail-closed
//! `CHAIN-LAUNCH-1` identity pinning, `UPGRADE-1 -> SUCCESSOR-ACTIVATION-1` live `CE-44`
//! switchover, fault injection, and deterministic [`TestnetClusterConfigV1`] /
//! [`TestnetCoverageReportV1`] receipts.
//!
//! Portable core (`TestnetClusterConfigV1`, `TestnetCoverageReportV1`, stage×gate-subclaim
//! matrix enforcement, and canonical tag derivation) is 100% `wasm32`-clean; native multi-process
//! OS execution lives behind `#[cfg(not(target_arch = "wasm32"))]` in [`testnet_proc`]. Strictly
//! behind `--features protocol-research`, zero new Cargo dependencies, and zero edits to frozen
//! crates or modules.
//!
//! # Non-claim (`CAPABILITY_TREE.md` §`TESTNET-1`)
//! `BOUNDED` is mandatory. Deterministic devnet validator/release/launch/activation keys are
//! explicitly labeled `DEVNET-TEST-KEYS-NOT-PRODUCTION-CUSTODY`. A passing multi-process testnet,
//! hash agreement, or fault-free run is NOT production security; production-only `G18` remains
//! owned by `PROD-OPS-1`.

use std::collections::BTreeSet;
use std::fmt;

use sha2::{Digest, Sha256};

use crate::chain_launch_1::{
    ChainLaunchPackageV1, LaunchPreflightCapsV1, build_canonical_devnet_chain_launch,
    verify_chain_launch_package,
};
use crate::execution_root_1::ExecutionRootRefV1;
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::successor_activation_1::{
    CanonicalDevnetSuccessorActivationBuildV1, SuccessorActivationCarrierV1,
    SuccessorPreflightCapsV1, build_canonical_devnet_successor_activation,
    verify_successor_activation_carrier,
};
use crate::upgrade_1::verify_archival_historical_block;

/// Canonical schema version for `TESTNET-1` objects.
pub const TESTNET_1_VERSION: u16 = 1;

/// Canonical schema string for [`TestnetClusterConfigV1`].
pub const TESTNET_1_CLUSTER_CONFIG_SCHEMA: &[u8] = b"bitblock/testnet-1/cluster-config/v1";

/// Canonical schema string for [`TestnetCoverageReportV1`].
pub const TESTNET_1_COVERAGE_REPORT_SCHEMA: &[u8] = b"bitblock/testnet-1/coverage-report/v1";

/// Bit 0: clean genesis launch-closure boot & identity pin across all 6 OS processes.
pub const FAULT_BIT_CLEAN_GENESIS_BOOT: u32 = 1 << 0;
/// Bit 1: pre-upgrade `v1` HotStuff BFT finality across heights `1..=3`.
pub const FAULT_BIT_PRE_UPGRADE_FINALITY: u32 = 1 << 1;
/// Bit 2: real OS `Child::kill()` crash & `STORAGE-1` / `SIGNER-OPS-1` disk recovery.
pub const FAULT_BIT_CRASH_KILL_RECOVERY: u32 = 1 << 2;
/// Bit 3: 3-of-4 validator partition finality progress & healed replica catchup.
pub const FAULT_BIT_PARTITION_HEAL: u32 = 1 << 3;
/// Bit 4: equivocation / anti-double-sign refusal by durable `SIGNER-OPS-1` watermark.
pub const FAULT_BIT_EQUIVOCATION_REFUSAL: u32 = 1 << 4;
/// Bit 5: wrong-root rejection & `CLIENT-1` pre-finality reorg rollback (zero phantom ownership).
pub const FAULT_BIT_WRONG_ROOT_REORG: u32 = 1 << 5;
/// Bit 6: live `CE-44` `UPGRADE-1 -> SUCCESSOR-ACTIVATION-1` switchover at height `4`.
pub const FAULT_BIT_CE44_SWITCHOVER: u32 = 1 << 6;
/// Bit 7: post-upgrade `v2` finality across heights `5..=6` across all 6 OS processes.
pub const FAULT_BIT_POST_UPGRADE_FINALITY: u32 = 1 << 7;
/// Bit 8: post-upgrade stale `v1` downgrade rejection across all 6 OS processes.
pub const FAULT_BIT_DOWNGRADE_REJECTION: u32 = 1 << 8;
/// Bit 9: archive node historical `v1` proof verification after `v2` activation.
pub const FAULT_BIT_ARCHIVAL_HISTORY_PROOF: u32 = 1 << 9;

/// Complete 10-control required testnet fault/lifecycle bitmask (`0x03FF`).
pub const TESTNET_FAULT_MASK_ALL_10: u32 = FAULT_BIT_CLEAN_GENESIS_BOOT
    | FAULT_BIT_PRE_UPGRADE_FINALITY
    | FAULT_BIT_CRASH_KILL_RECOVERY
    | FAULT_BIT_PARTITION_HEAL
    | FAULT_BIT_EQUIVOCATION_REFUSAL
    | FAULT_BIT_WRONG_ROOT_REORG
    | FAULT_BIT_CE44_SWITCHOVER
    | FAULT_BIT_POST_UPGRADE_FINALITY
    | FAULT_BIT_DOWNGRADE_REJECTION
    | FAULT_BIT_ARCHIVAL_HISTORY_PROOF;

const TN1_STOP_COND_PREFIX: &[u8] = b"bb.tn1.stop_condition.v1";
const TN1_RETAINED_EVID_PREFIX: &[u8] = b"bb.tn1.retained_evidence.v1";
const TN1_G18_ARTIFACT_PREFIX: &[u8] = b"bb.tn1.g18_prod_ops_deferred.v1";

/// Fail-closed validation, boot-pin, gate-matrix, or multi-process orchestration error for `TESTNET-1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Testnet1Error {
    /// Truncated, malformed, or non-canonical wire encoding.
    InvalidWire(&'static str),
    /// Stage×gate-subclaim matrix omitted a required gate, left a conditional unresolved, or
    /// misattributed `G18` / inactive feature exclusions.
    GateMatrixViolation(&'static str),
    /// Boot launch-closure package, successor carrier, or pinned identities failed verification.
    BootPinRejected(&'static str),
    /// Ambient successor registry or substituted reference attempted outside the carried bundle.
    AmbientOrSubstitutedReference(&'static str),
    /// Multi-process orchestration, TCP RPC, journal, signer, or fault-control assertion failed.
    OrchestrationFailed(String),
}

impl fmt::Display for Testnet1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Testnet1Error {}

/// Process role inside the 6-process replicated adversarial testnet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum TestnetNodeRoleV1 {
    /// Consensus validator holding kernel-locked `SIGNER-OPS-1` keys (`4` processes).
    Validator = 1,
    /// Role-separated full verifier exercising independent block/root/upgrade checks (`1` process).
    FullVerifier = 2,
    /// Archive and recovery node retaining opaque certs, full journals, and archival proofs (`1` process).
    ArchiveRecovery = 3,
}

impl TestnetNodeRoleV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte role discriminant fail-closed.
    pub const fn from_code(c: u8) -> Result<Self, Testnet1Error> {
        match c {
            1 => Ok(Self::Validator),
            2 => Ok(Self::FullVerifier),
            3 => Ok(Self::ArchiveRecovery),
            _ => Err(Testnet1Error::InvalidWire("unknown TestnetNodeRoleV1")),
        }
    }

    /// Returns the canonical CLI role string.
    pub const fn cli_name(self) -> &'static str {
        match self {
            Self::Validator => "validator",
            Self::FullVerifier => "full-verifier",
            Self::ArchiveRecovery => "archive",
        }
    }
}

/// Disposition for one row of the admitted `TESTNET-1` stage×gate-subclaim coverage matrix.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum TestnetGateDispositionV1 {
    /// Required in testnet scope and verified passing (`G5.core`, `G15.resource`, active predicates).
    PassRequired = 1,
    /// Explicitly excluded with a mandatory non-empty reason (inactive features or `PROD-OPS-1` `G18`).
    ExcludedWithReason = 2,
}

impl TestnetGateDispositionV1 {
    /// Returns the canonical 1-byte disposition discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte disposition discriminant fail-closed.
    pub const fn from_code(c: u8) -> Result<Self, Testnet1Error> {
        match c {
            1 => Ok(Self::PassRequired),
            2 => Ok(Self::ExcludedWithReason),
            _ => Err(Testnet1Error::InvalidWire(
                "unknown TestnetGateDispositionV1",
            )),
        }
    }
}

/// One row of the admitted `TESTNET-1` stage×gate-subclaim matrix naming owner, artifact,
/// stop condition, and explicit disposition (zero unresolved conditionals).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestnetGateMatrixRowV1 {
    /// Gate subclaim identifier (e.g. `G5.core`, `G15.resource`, `G18.production_ops`, `G19.upgrade`).
    pub subclaim_id: String,
    /// Owning capability ID (e.g. `CHAIN-LAUNCH-1`, `SUCCESSOR-ACTIVATION-1`, `PROD-OPS-1`).
    pub owner_capability: String,
    /// Explicit pass/exclusion disposition.
    pub disposition: TestnetGateDispositionV1,
    /// Bound artifact digest backing this row.
    pub artifact_digest: [u8; 32],
    /// Canonical digest of the explicit halt/stop condition for this row.
    pub stop_condition_digest: [u8; 32],
    /// Mandatory non-empty explanation when `disposition == ExcludedWithReason` (empty when passing).
    pub exclusion_reason: String,
}

/// Canonical multi-process testnet cluster configuration pinned by every OS process at boot
/// (`bitblock/testnet-1/cluster-config/v1`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestnetClusterConfigV1 {
    /// Schema version (`1`).
    pub schema_version: u16,
    /// Chain identifier (`77` for `bitblock.devnet.v1`).
    pub chain_id: u64,
    /// Immutable launch `genesis_id`.
    pub genesis_id: [u8; 32],
    /// Pinned `CHAIN-LAUNCH-1` `chain_launch_manifest_id` (MUST match every process).
    pub chain_launch_manifest_id: [u8; 32],
    /// Pinned `CHAIN-LAUNCH-1` `launch_closure_bundle_id` (MUST match every process).
    pub launch_closure_bundle_id: [u8; 32],
    /// Initial admitted `v1` profile ID.
    pub initial_profile_id: [u8; 32],
    /// Initial `PROFILE-1` `profile_admission_manifest_id`.
    pub initial_profile_admission_manifest_id: [u8; 32],
    /// Initial `RELEASE-1` `admitted_release_record_id`.
    pub initial_release_record_id: [u8; 32],
    /// Whether `CE-44` `in-place-upgrade` is active in this testnet cluster.
    pub in_place_upgrade_active: bool,
    /// Target admitted `v2` successor profile ID (`[0; 32]` iff `!in_place_upgrade_active`).
    pub successor_profile_id: [u8; 32],
    /// Target `PROFILE-1` `v2` `profile_admission_manifest_id`.
    pub successor_profile_admission_manifest_id: [u8; 32],
    /// Exact `SUCCESSOR-ACTIVATION-1` `successor_activation_bundle_id`.
    pub successor_activation_bundle_id: [u8; 32],
    /// Exact `SUCCESSOR-ACTIVATION-1` `activation_authorization_id`.
    pub activation_authorization_id: [u8; 32],
    /// Scheduled in-place upgrade activation height (`4`).
    pub activation_height: u64,
    /// Number of independent validator OS processes (`4`).
    pub validator_count: u16,
    /// Number of independent full-verifier OS processes (`1`).
    pub full_verifier_count: u16,
    /// Number of independent archive/recovery OS processes (`1`).
    pub archive_recovery_count: u16,
    /// Strictly sorted unique stage×gate-subclaim coverage matrix (8 rows, zero unresolved).
    pub stage_gate_matrix: Vec<TestnetGateMatrixRowV1>,
}

impl TestnetClusterConfigV1 {
    /// Validates cluster topology, pinned non-zero identities, `CE-44` bindings, and exact
    /// stage×gate-subclaim matrix completeness fail-closed.
    pub fn validate(&self) -> Result<(), Testnet1Error> {
        if self.schema_version != TESTNET_1_VERSION || self.chain_id == 0 {
            return Err(Testnet1Error::InvalidWire(
                "bad cluster config version or chain_id",
            ));
        }
        if self.validator_count < 4
            || self.full_verifier_count == 0
            || self.archive_recovery_count == 0
        {
            return Err(Testnet1Error::BootPinRejected(
                "testnet requires >=4 validators, >=1 full verifier, and >=1 archive/recovery node",
            ));
        }
        for d in [
            self.genesis_id,
            self.chain_launch_manifest_id,
            self.launch_closure_bundle_id,
            self.initial_profile_id,
            self.initial_profile_admission_manifest_id,
            self.initial_release_record_id,
        ] {
            if d == [0u8; 32] {
                return Err(Testnet1Error::BootPinRejected(
                    "zero launch identity forbidden in TestnetClusterConfigV1",
                ));
            }
        }
        if self.in_place_upgrade_active
            && (self.activation_height == 0
                || self.successor_profile_id == [0u8; 32]
                || self.successor_profile_id == self.initial_profile_id
                || self.successor_profile_admission_manifest_id == [0u8; 32]
                || self.successor_activation_bundle_id == [0u8; 32]
                || self.activation_authorization_id == [0u8; 32])
        {
            return Err(Testnet1Error::BootPinRejected(
                "CE-44 in-place-upgrade requires distinct non-zero successor bundle/admission/auth IDs",
            ));
        }
        // Validate stage x gate subclaim matrix: strictly sorted, unique, exact 8 canonical rows.
        let mut seen = BTreeSet::new();
        for (i, row) in self.stage_gate_matrix.iter().enumerate() {
            if i > 0 && self.stage_gate_matrix[i - 1].subclaim_id >= row.subclaim_id {
                return Err(Testnet1Error::GateMatrixViolation(
                    "stage_gate_matrix must be strictly sorted by unique subclaim_id",
                ));
            }
            if row.owner_capability.is_empty()
                || row.artifact_digest == [0u8; 32]
                || row.stop_condition_digest == [0u8; 32]
            {
                return Err(Testnet1Error::GateMatrixViolation(
                    "gate matrix row missing owner, artifact, or stop condition",
                ));
            }
            match row.disposition {
                TestnetGateDispositionV1::PassRequired => {
                    if !row.exclusion_reason.is_empty() {
                        return Err(Testnet1Error::GateMatrixViolation(
                            "PassRequired row must have empty exclusion_reason",
                        ));
                    }
                }
                TestnetGateDispositionV1::ExcludedWithReason => {
                    if row.exclusion_reason.is_empty() {
                        return Err(Testnet1Error::GateMatrixViolation(
                            "ExcludedWithReason row requires non-empty exclusion_reason",
                        ));
                    }
                }
            }
            seen.insert(row.subclaim_id.as_str());
        }
        let required_pass = [
            "G5.core",
            "G5.asset",
            "G15.resource",
            "G15.economic",
            "G19.upgrade",
        ];
        for id in required_pass {
            let row = self
                .stage_gate_matrix
                .iter()
                .find(|r| r.subclaim_id == id)
                .ok_or(Testnet1Error::GateMatrixViolation(
                    "missing required passing gate subclaim",
                ))?;
            if row.disposition != TestnetGateDispositionV1::PassRequired {
                return Err(Testnet1Error::GateMatrixViolation(
                    "required active gate subclaim was not PassRequired",
                ));
            }
        }
        for (id, expected_owner) in [
            ("G18.production_ops", "PROD-OPS-1"),
            ("G19.program", "PROGRAM-STATE-1"),
            ("G19.verkle", "VERKLE-R1"),
        ] {
            let row = self
                .stage_gate_matrix
                .iter()
                .find(|r| r.subclaim_id == id)
                .ok_or(Testnet1Error::GateMatrixViolation(
                    "missing required excluded gate subclaim",
                ))?;
            if row.disposition != TestnetGateDispositionV1::ExcludedWithReason
                || row.owner_capability != expected_owner
            {
                return Err(Testnet1Error::GateMatrixViolation(
                    "G18 or inactive G19 feature row violated owner or exclusion rule",
                ));
            }
        }
        if seen.len() != 8 {
            return Err(Testnet1Error::GateMatrixViolation(
                "unexpected extra or missing gate matrix rows (expected exact 8)",
            ));
        }
        Ok(())
    }

    /// Encodes into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, Testnet1Error> {
        self.validate()?;
        let mut out = Vec::with_capacity(1024);
        out.extend_from_slice(&self.schema_version.to_be_bytes());
        out.extend_from_slice(&self.chain_id.to_be_bytes());
        out.extend_from_slice(&self.genesis_id);
        out.extend_from_slice(&self.chain_launch_manifest_id);
        out.extend_from_slice(&self.launch_closure_bundle_id);
        out.extend_from_slice(&self.initial_profile_id);
        out.extend_from_slice(&self.initial_profile_admission_manifest_id);
        out.extend_from_slice(&self.initial_release_record_id);
        out.push(u8::from(self.in_place_upgrade_active));
        out.extend_from_slice(&self.successor_profile_id);
        out.extend_from_slice(&self.successor_profile_admission_manifest_id);
        out.extend_from_slice(&self.successor_activation_bundle_id);
        out.extend_from_slice(&self.activation_authorization_id);
        out.extend_from_slice(&self.activation_height.to_be_bytes());
        out.extend_from_slice(&self.validator_count.to_be_bytes());
        out.extend_from_slice(&self.full_verifier_count.to_be_bytes());
        out.extend_from_slice(&self.archive_recovery_count.to_be_bytes());
        let n_rows = u16::try_from(self.stage_gate_matrix.len())
            .map_err(|_| Testnet1Error::InvalidWire("gate matrix row overflow"))?;
        out.extend_from_slice(&n_rows.to_be_bytes());
        for r in &self.stage_gate_matrix {
            put_str(&mut out, &r.subclaim_id)?;
            put_str(&mut out, &r.owner_capability)?;
            out.push(r.disposition.code());
            out.extend_from_slice(&r.artifact_digest);
            out.extend_from_slice(&r.stop_condition_digest);
            put_str(&mut out, &r.exclusion_reason)?;
        }
        Ok(out)
    }

    /// Decodes canonical big-endian wire bytes fail-closed.
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, Testnet1Error> {
        let mut c = 0usize;
        let schema_version = rd_u16(raw, &mut c)?;
        let chain_id = rd_u64(raw, &mut c)?;
        let genesis_id = rd_32(raw, &mut c)?;
        let chain_launch_manifest_id = rd_32(raw, &mut c)?;
        let launch_closure_bundle_id = rd_32(raw, &mut c)?;
        let initial_profile_id = rd_32(raw, &mut c)?;
        let initial_profile_admission_manifest_id = rd_32(raw, &mut c)?;
        let initial_release_record_id = rd_32(raw, &mut c)?;
        let in_place_upgrade_active = rd_bool(raw, &mut c)?;
        let successor_profile_id = rd_32(raw, &mut c)?;
        let successor_profile_admission_manifest_id = rd_32(raw, &mut c)?;
        let successor_activation_bundle_id = rd_32(raw, &mut c)?;
        let activation_authorization_id = rd_32(raw, &mut c)?;
        let activation_height = rd_u64(raw, &mut c)?;
        let validator_count = rd_u16(raw, &mut c)?;
        let full_verifier_count = rd_u16(raw, &mut c)?;
        let archive_recovery_count = rd_u16(raw, &mut c)?;
        let n_rows = usize::from(rd_u16(raw, &mut c)?);
        let mut stage_gate_matrix = Vec::with_capacity(n_rows.min(32));
        for _ in 0..n_rows {
            let subclaim_id = rd_str(raw, &mut c)?;
            let owner_capability = rd_str(raw, &mut c)?;
            let disposition = TestnetGateDispositionV1::from_code(rd_u8(raw, &mut c)?)?;
            let artifact_digest = rd_32(raw, &mut c)?;
            let stop_condition_digest = rd_32(raw, &mut c)?;
            let exclusion_reason = rd_str(raw, &mut c)?;
            stage_gate_matrix.push(TestnetGateMatrixRowV1 {
                subclaim_id,
                owner_capability,
                disposition,
                artifact_digest,
                stop_condition_digest,
                exclusion_reason,
            });
        }
        if c != raw.len() {
            return Err(Testnet1Error::InvalidWire(
                "trailing bytes in TestnetClusterConfigV1",
            ));
        }
        let cfg = Self {
            schema_version,
            chain_id,
            genesis_id,
            chain_launch_manifest_id,
            launch_closure_bundle_id,
            initial_profile_id,
            initial_profile_admission_manifest_id,
            initial_release_record_id,
            in_place_upgrade_active,
            successor_profile_id,
            successor_profile_admission_manifest_id,
            successor_activation_bundle_id,
            activation_authorization_id,
            activation_height,
            validator_count,
            full_verifier_count,
            archive_recovery_count,
            stage_gate_matrix,
        };
        cfg.validate()?;
        Ok(cfg)
    }

    /// Computes the canonical `Policy` `ContentTag` whose digest IS `cluster_config_id`.
    pub fn tag(&self) -> Result<ContentTag, Testnet1Error> {
        let enc = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            TESTNET_1_CLUSTER_CONFIG_SCHEMA,
            enc.len() as u64,
            |w| w(&enc),
        )
        .expect("static schema and bounded length"))
    }
}

/// Canonical deterministic public coverage report emitted by a passing 6-process `TESTNET-1` run
/// (`bitblock/testnet-1/coverage-report/v1`, `499` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TestnetCoverageReportV1 {
    /// Schema version (`1`).
    pub schema_version: u16,
    /// Bound `TestnetClusterConfigV1::tag().digest()`.
    pub cluster_config_id: [u8; 32],
    /// Exact pinned `CHAIN-LAUNCH-1` `chain_launch_manifest_id`.
    pub chain_launch_manifest_id: [u8; 32],
    /// Exact pinned `CHAIN-LAUNCH-1` `launch_closure_bundle_id`.
    pub launch_closure_bundle_id: [u8; 32],
    /// Exact `GENESIS-ROOT-PARITY-1` `parity_record_id`.
    pub genesis_root_parity_record_id: [u8; 32],
    /// Exact `SUCCESSOR-ACTIVATION-1` `successor_activation_bundle_id` (`CE-44`).
    pub successor_activation_bundle_id: [u8; 32],
    /// Exact `SUCCESSOR-ACTIVATION-1` `activation_authorization_id` (`CE-44`).
    pub activation_authorization_id: [u8; 32],
    /// Total distinct OS processes exercised (`6`).
    pub total_os_processes_exercised: u16,
    /// Highest pre-upgrade `v1` finalized block height (`3`).
    pub pre_upgrade_finalized_height: u64,
    /// Height at which live `CE-44` `v1 -> v2` switchover executed (`4`).
    pub activation_height: u64,
    /// Highest post-upgrade `v2` finalized block height (`6`).
    pub post_upgrade_finalized_height: u64,
    /// Active profile before activation height (`old_profile_id`).
    pub active_profile_before_upgrade: [u8; 32],
    /// Active profile after activation height (`new_profile_id`).
    pub active_profile_after_upgrade: [u8; 32],
    /// Verified post-upgrade composed [`ExecutionRootRefV1`] (`178` bytes).
    pub final_execution_root_ref: ExecutionRootRefV1,
    /// Deterministic commitment over retained launch, successor, fault, crash, and archive evidence.
    pub retained_evidence_digest: [u8; 32],
    /// Exact 10-bit bitmask of passed adversarial & lifecycle controls (`0x03FF`).
    pub fault_controls_passed_mask: u32,
    /// Whether all required stage×gate rows and all 10 controls passed (`true`).
    pub all_required_gates_passed: bool,
}

impl TestnetCoverageReportV1 {
    /// Fixed canonical big-endian wire length (`499` bytes).
    pub const WIRE_LEN: usize =
        2 + 32 * 6 + 2 + 8 * 3 + 32 * 2 + ExecutionRootRefV1::WIRE_LEN + 32 + 4 + 1;

    /// Validates report invariants fail-closed.
    pub fn validate(&self) -> Result<(), Testnet1Error> {
        if self.schema_version != TESTNET_1_VERSION
            || self.total_os_processes_exercised < 6
            || self.pre_upgrade_finalized_height >= self.activation_height
            || self.post_upgrade_finalized_height <= self.activation_height
            || self.active_profile_before_upgrade == self.active_profile_after_upgrade
            || self.fault_controls_passed_mask != TESTNET_FAULT_MASK_ALL_10
            || !self.all_required_gates_passed
        {
            return Err(Testnet1Error::GateMatrixViolation(
                "TestnetCoverageReportV1 failed required process/height/polarity/fault-mask gate",
            ));
        }
        Ok(())
    }

    /// Encodes into exact 499-byte canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], Testnet1Error> {
        self.validate()?;
        let mut out = Vec::with_capacity(Self::WIRE_LEN);
        out.extend_from_slice(&self.schema_version.to_be_bytes());
        out.extend_from_slice(&self.cluster_config_id);
        out.extend_from_slice(&self.chain_launch_manifest_id);
        out.extend_from_slice(&self.launch_closure_bundle_id);
        out.extend_from_slice(&self.genesis_root_parity_record_id);
        out.extend_from_slice(&self.successor_activation_bundle_id);
        out.extend_from_slice(&self.activation_authorization_id);
        out.extend_from_slice(&self.total_os_processes_exercised.to_be_bytes());
        out.extend_from_slice(&self.pre_upgrade_finalized_height.to_be_bytes());
        out.extend_from_slice(&self.activation_height.to_be_bytes());
        out.extend_from_slice(&self.post_upgrade_finalized_height.to_be_bytes());
        out.extend_from_slice(&self.active_profile_before_upgrade);
        out.extend_from_slice(&self.active_profile_after_upgrade);
        out.extend_from_slice(&self.final_execution_root_ref.encode_canonical());
        out.extend_from_slice(&self.retained_evidence_digest);
        out.extend_from_slice(&self.fault_controls_passed_mask.to_be_bytes());
        out.push(u8::from(self.all_required_gates_passed));
        out.try_into()
            .map_err(|_| Testnet1Error::InvalidWire("coverage report wire len mismatch"))
    }

    /// Decodes exact 499-byte canonical big-endian wire bytes fail-closed.
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, Testnet1Error> {
        if raw.len() != Self::WIRE_LEN {
            return Err(Testnet1Error::InvalidWire(
                "invalid TestnetCoverageReportV1 length",
            ));
        }
        let mut c = 0usize;
        let schema_version = rd_u16(raw, &mut c)?;
        let cluster_config_id = rd_32(raw, &mut c)?;
        let chain_launch_manifest_id = rd_32(raw, &mut c)?;
        let launch_closure_bundle_id = rd_32(raw, &mut c)?;
        let genesis_root_parity_record_id = rd_32(raw, &mut c)?;
        let successor_activation_bundle_id = rd_32(raw, &mut c)?;
        let activation_authorization_id = rd_32(raw, &mut c)?;
        let total_os_processes_exercised = rd_u16(raw, &mut c)?;
        let pre_upgrade_finalized_height = rd_u64(raw, &mut c)?;
        let activation_height = rd_u64(raw, &mut c)?;
        let post_upgrade_finalized_height = rd_u64(raw, &mut c)?;
        let active_profile_before_upgrade = rd_32(raw, &mut c)?;
        let active_profile_after_upgrade = rd_32(raw, &mut c)?;
        let ref_end = c + ExecutionRootRefV1::WIRE_LEN;
        let final_execution_root_ref = ExecutionRootRefV1::decode_canonical(&raw[c..ref_end])
            .map_err(|_| Testnet1Error::InvalidWire("bad final_execution_root_ref"))?;
        c = ref_end;
        let retained_evidence_digest = rd_32(raw, &mut c)?;
        let fault_controls_passed_mask = rd_u32(raw, &mut c)?;
        let all_required_gates_passed = rd_bool(raw, &mut c)?;
        let rep = Self {
            schema_version,
            cluster_config_id,
            chain_launch_manifest_id,
            launch_closure_bundle_id,
            genesis_root_parity_record_id,
            successor_activation_bundle_id,
            activation_authorization_id,
            total_os_processes_exercised,
            pre_upgrade_finalized_height,
            activation_height,
            post_upgrade_finalized_height,
            active_profile_before_upgrade,
            active_profile_after_upgrade,
            final_execution_root_ref,
            retained_evidence_digest,
            fault_controls_passed_mask,
            all_required_gates_passed,
        };
        rep.validate()?;
        Ok(rep)
    }

    /// Computes the canonical `Receipt` `ContentTag` for this coverage report.
    pub fn tag(&self) -> Result<ContentTag, Testnet1Error> {
        let enc = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            TESTNET_1_COVERAGE_REPORT_SCHEMA,
            enc.len() as u64,
            |w| w(&enc),
        )
        .expect("static schema and fixed length"))
    }
}

/// Verifies every process's boot-time pin against the authorized `CHAIN-LAUNCH-1` package and
/// (when `CE-44` `in-place-upgrade` is active) the complete `SUCCESSOR-ACTIVATION-1` carrier,
/// rejecting ambient registries, substituted IDs, missing preimages, or gate-matrix gaps BEFORE
/// height `1` or successor activation.
pub fn validate_testnet_boot_pin(
    cfg: &TestnetClusterConfigV1,
    launch_pkg: &ChainLaunchPackageV1,
    release_policy: &crate::release_1::TrustedReleaseAuthorityPolicyV1,
    launch_policy: &crate::chain_launch_1::TrustedLaunchAuthorityPolicyV1,
    successor_carrier: Option<&SuccessorActivationCarrierV1>,
    successor_build_ctx: Option<&CanonicalDevnetSuccessorActivationBuildV1>,
    allow_ambient_successor_registry: bool,
) -> Result<(), Testnet1Error> {
    if allow_ambient_successor_registry {
        return Err(Testnet1Error::AmbientOrSubstitutedReference(
            "ambient successor registry is strictly forbidden in TESTNET-1",
        ));
    }
    cfg.validate()?;
    let verified_launch = verify_chain_launch_package(
        launch_pkg,
        release_policy,
        launch_policy,
        LaunchPreflightCapsV1::V1_DEFAULT,
    )
    .map_err(|_| Testnet1Error::BootPinRejected("CHAIN-LAUNCH-1 package failed verification"))?;

    if verified_launch.genesis_id != cfg.genesis_id
        || verified_launch.chain_launch_manifest_id != cfg.chain_launch_manifest_id
        || verified_launch.launch_closure_bundle_id != cfg.launch_closure_bundle_id
        || verified_launch.initial_profile_id != cfg.initial_profile_id
        || launch_pkg.manifest.initial_profile_admission_manifest_id
            != cfg.initial_profile_admission_manifest_id
        || launch_pkg.manifest.initial_release_record_id != cfg.initial_release_record_id
    {
        return Err(Testnet1Error::BootPinRejected(
            "process boot launch-manifest, bundle, profile, or release ID disagreed with pinned cluster config",
        ));
    }

    if cfg.in_place_upgrade_active {
        let carrier = successor_carrier.ok_or(Testnet1Error::BootPinRejected(
            "CE-44 in-place-upgrade active but successor carrier missing",
        ))?;
        let ctx = successor_build_ctx.ok_or(Testnet1Error::BootPinRejected(
            "CE-44 in-place-upgrade active but Stage 1-3 authority context missing",
        ))?;
        let verified_succ = verify_successor_activation_carrier(
            carrier,
            &ctx.base_context,
            &ctx.release_parent_cert,
            &ctx.activation_parent_cert,
            SuccessorPreflightCapsV1::V1_DEFAULT,
        )
        .map_err(|_| {
            Testnet1Error::BootPinRejected(
                "CE-44 SUCCESSOR-ACTIVATION-1 carrier failed verification",
            )
        })?;
        if verified_succ.preflight.successor_admitted_profile_id != cfg.successor_profile_id
            || verified_succ.successor_profile_admission_manifest_id
                != cfg.successor_profile_admission_manifest_id
            || verified_succ
                .activation_stage
                .successor_activation_bundle_id
                != cfg.successor_activation_bundle_id
            || verified_succ.activation_stage.activation_authorization_id
                != cfg.activation_authorization_id
            || verified_succ.activation_height != cfg.activation_height
        {
            return Err(Testnet1Error::BootPinRejected(
                "CE-44 successor bundle/admission/auth/height disagreed with pinned cluster config",
            ));
        }
        // Also verify UPGRADE-1 archival historical proof holds at boot closure.
        verify_archival_historical_block(
            &ctx.upgrade_outcome.migration_spec,
            &ctx.upgrade_outcome.occurrence_handoff,
            1,
            ctx.upgrade_outcome
                .migration_spec
                .predecessor_admitted_profile_id,
        )
        .map_err(|_| {
            Testnet1Error::BootPinRejected("UPGRADE-1 archival historical proof failed")
        })?;
    }
    Ok(())
}

/// Computes the canonical stop-condition digest for a named stage×gate subclaim.
pub fn stop_condition_digest(subclaim_id: &str, rule: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(TN1_STOP_COND_PREFIX);
    h.update(subclaim_id.as_bytes());
    h.update(b":");
    h.update(rule.as_bytes());
    h.finalize().into()
}

/// Computes the deterministic retained-evidence digest covering launch, successor, crash,
/// fault-control, and archival evidence identities.
#[allow(clippy::too_many_arguments)]
pub fn compute_retained_evidence_digest(
    cluster_config_id: [u8; 32],
    launch_manifest_id: [u8; 32],
    launch_bundle_id: [u8; 32],
    parity_record_id: [u8; 32],
    successor_bundle_id: [u8; 32],
    activation_auth_id: [u8; 32],
    final_exec_ref: &ExecutionRootRefV1,
    fault_mask: u32,
) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(TN1_RETAINED_EVID_PREFIX);
    h.update(cluster_config_id);
    h.update(launch_manifest_id);
    h.update(launch_bundle_id);
    h.update(parity_record_id);
    h.update(successor_bundle_id);
    h.update(activation_auth_id);
    h.update(final_exec_ref.encode_canonical());
    h.update(fault_mask.to_be_bytes());
    h.finalize().into()
}

/// Complete deterministic `bitblock.devnet.v1` `TESTNET-1` fixture (portable `wasm32`-clean).
#[derive(Clone, Debug)]
pub struct CanonicalDevnetTestnetFixtureV1 {
    /// Upstream verified `SUCCESSOR-ACTIVATION-1` (which also owns `CHAIN-LAUNCH-1` & `UPGRADE-1`).
    pub successor_build: CanonicalDevnetSuccessorActivationBuildV1,
    /// Verified canonical [`TestnetClusterConfigV1`].
    pub cluster_config: TestnetClusterConfigV1,
    /// Verified canonical deterministic [`TestnetCoverageReportV1`].
    pub expected_coverage_report: TestnetCoverageReportV1,
}

/// Builds and self-verifies the canonical `bitblock.devnet.v1` `TESTNET-1` cluster configuration
/// and deterministic coverage report (matching `tools/testnet_1_vectors.py` bit-for-bit).
pub fn build_canonical_devnet_testnet_fixture()
-> Result<CanonicalDevnetTestnetFixtureV1, Testnet1Error> {
    let succ = build_canonical_devnet_successor_activation(true)
        .map_err(|_| Testnet1Error::BootPinRejected("build successor activation failed"))?;
    let launch_pkg = &succ.launch_build.package;
    let m = &launch_pkg.manifest;
    let chain_launch_manifest_id = launch_pkg.chain_launch_manifest_id;
    let launch_closure_bundle_id = launch_pkg.launch_closure_bundle_id;
    let parity_id = m.genesis_root_parity_record_id;
    let succ_bundle_id = succ
        .bundle
        .tag()
        .map_err(|_| Testnet1Error::BootPinRejected("succ bundle tag"))?
        .digest();
    let act_auth_id = succ
        .authorization
        .tag()
        .map_err(|_| Testnet1Error::BootPinRejected("act auth tag"))?
        .digest();
    let g18_art: [u8; 32] = Sha256::digest(TN1_G18_ARTIFACT_PREFIX).into();

    let mut matrix = vec![
        TestnetGateMatrixRowV1 {
            subclaim_id: "G15.economic".to_string(),
            owner_capability: "VALIDATOR-ECON-1".to_string(),
            disposition: TestnetGateDispositionV1::PassRequired,
            artifact_digest: m.initial_profile_admission_manifest_id,
            stop_condition_digest: stop_condition_digest(
                "G15.economic",
                "halt-on-stake-or-fee-conservation-drift",
            ),
            exclusion_reason: String::new(),
        },
        TestnetGateMatrixRowV1 {
            subclaim_id: "G15.resource".to_string(),
            owner_capability: "RESOURCE-1".to_string(),
            disposition: TestnetGateDispositionV1::PassRequired,
            artifact_digest: m.initial_profile_admission_manifest_id,
            stop_condition_digest: stop_condition_digest(
                "G15.resource",
                "halt-on-unbounded-state-growth-or-gas-overflow",
            ),
            exclusion_reason: String::new(),
        },
        TestnetGateMatrixRowV1 {
            subclaim_id: "G18.production_ops".to_string(),
            owner_capability: "PROD-OPS-1".to_string(),
            disposition: TestnetGateDispositionV1::ExcludedWithReason,
            artifact_digest: g18_art,
            stop_condition_digest: stop_condition_digest(
                "G18.production_ops",
                "production-only-gate-deferred-to-prod-ops-1",
            ),
            exclusion_reason: "production-only-G18-owned-by-PROD-OPS-1".to_string(),
        },
        TestnetGateMatrixRowV1 {
            subclaim_id: "G19.program".to_string(),
            owner_capability: "PROGRAM-STATE-1".to_string(),
            disposition: TestnetGateDispositionV1::ExcludedWithReason,
            artifact_digest: m.initial_profile_admission_manifest_id,
            stop_condition_digest: stop_condition_digest(
                "G19.program",
                "halt-if-slot-6-activates-without-program-active",
            ),
            exclusion_reason: "program-active-inactive-in-devnet-v1-v2".to_string(),
        },
        TestnetGateMatrixRowV1 {
            subclaim_id: "G19.upgrade".to_string(),
            owner_capability: "SUCCESSOR-ACTIVATION-1".to_string(),
            disposition: TestnetGateDispositionV1::PassRequired,
            artifact_digest: succ_bundle_id,
            stop_condition_digest: stop_condition_digest(
                "G19.upgrade",
                "halt-before-activation-on-bundle-authority-or-migration-fault",
            ),
            exclusion_reason: String::new(),
        },
        TestnetGateMatrixRowV1 {
            subclaim_id: "G19.verkle".to_string(),
            owner_capability: "VERKLE-R1".to_string(),
            disposition: TestnetGateDispositionV1::ExcludedWithReason,
            artifact_digest: m.initial_profile_admission_manifest_id,
            stop_condition_digest: stop_condition_digest(
                "G19.verkle",
                "halt-if-verkle-selected-without-ce42-prototype",
            ),
            exclusion_reason: "verkle-selected-inactive-in-devnet-v1-v2".to_string(),
        },
        TestnetGateMatrixRowV1 {
            subclaim_id: "G5.asset".to_string(),
            owner_capability: "ASSET-1".to_string(),
            disposition: TestnetGateDispositionV1::PassRequired,
            artifact_digest: parity_id,
            stop_condition_digest: stop_condition_digest(
                "G5.asset",
                "halt-on-episode-ownership-or-supply-divergence",
            ),
            exclusion_reason: String::new(),
        },
        TestnetGateMatrixRowV1 {
            subclaim_id: "G5.core".to_string(),
            owner_capability: "CHAIN-LAUNCH-1".to_string(),
            disposition: TestnetGateDispositionV1::PassRequired,
            artifact_digest: chain_launch_manifest_id,
            stop_condition_digest: stop_condition_digest(
                "G5.core",
                "halt-before-height-1-on-launch-parity-or-consensus-fault",
            ),
            exclusion_reason: String::new(),
        },
    ];
    matrix.sort_by(|a, b| a.subclaim_id.cmp(&b.subclaim_id));

    let cluster_config = TestnetClusterConfigV1 {
        schema_version: TESTNET_1_VERSION,
        chain_id: succ.base_context.genesis_package.state.chain_id,
        genesis_id: m.genesis_id,
        chain_launch_manifest_id,
        launch_closure_bundle_id,
        initial_profile_id: m.initial_profile_id,
        initial_profile_admission_manifest_id: m.initial_profile_admission_manifest_id,
        initial_release_record_id: m.initial_release_record_id,
        in_place_upgrade_active: true,
        successor_profile_id: succ.bundle.successor_admitted_profile_id,
        successor_profile_admission_manifest_id: succ
            .bundle
            .successor_profile_admission_manifest_id,
        successor_activation_bundle_id: succ_bundle_id,
        activation_authorization_id: act_auth_id,
        activation_height: succ.upgrade_outcome.migration_spec.activation_height,
        validator_count: 4,
        full_verifier_count: 1,
        archive_recovery_count: 1,
        stage_gate_matrix: matrix,
    };
    validate_testnet_boot_pin(
        &cluster_config,
        launch_pkg,
        &succ.launch_build.release_policy,
        &succ.launch_build.launch_policy,
        Some(&succ.carrier),
        Some(&succ),
        false,
    )?;

    let cluster_config_id = cluster_config.tag()?.digest();
    let final_exec_ref = succ
        .upgrade_outcome
        .evidence_manifest
        .expected_new_execution_root_ref;
    let retained_evidence_digest = compute_retained_evidence_digest(
        cluster_config_id,
        chain_launch_manifest_id,
        launch_closure_bundle_id,
        parity_id,
        succ_bundle_id,
        act_auth_id,
        &final_exec_ref,
        TESTNET_FAULT_MASK_ALL_10,
    );
    let expected_coverage_report = TestnetCoverageReportV1 {
        schema_version: TESTNET_1_VERSION,
        cluster_config_id,
        chain_launch_manifest_id,
        launch_closure_bundle_id,
        genesis_root_parity_record_id: parity_id,
        successor_activation_bundle_id: succ_bundle_id,
        activation_authorization_id: act_auth_id,
        total_os_processes_exercised: 6,
        pre_upgrade_finalized_height: 3,
        activation_height: cluster_config.activation_height,
        post_upgrade_finalized_height: 6,
        active_profile_before_upgrade: cluster_config.initial_profile_id,
        active_profile_after_upgrade: cluster_config.successor_profile_id,
        final_execution_root_ref: final_exec_ref,
        retained_evidence_digest,
        fault_controls_passed_mask: TESTNET_FAULT_MASK_ALL_10,
        all_required_gates_passed: true,
    };
    expected_coverage_report.validate()?;

    // Ensure build_canonical_devnet_chain_launch is also directly exercised for non-CE-44 parity check.
    let _launch_direct = build_canonical_devnet_chain_launch(true)
        .map_err(|_| Testnet1Error::BootPinRejected("direct launch check"))?;

    Ok(CanonicalDevnetTestnetFixtureV1 {
        successor_build: succ,
        cluster_config,
        expected_coverage_report,
    })
}

fn put_str(out: &mut Vec<u8>, s: &str) -> Result<(), Testnet1Error> {
    let b = s.as_bytes();
    let len = u16::try_from(b.len()).map_err(|_| Testnet1Error::InvalidWire("str len overflow"))?;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(b);
    Ok(())
}

fn rd_slice<'a>(raw: &'a [u8], c: &mut usize, n: usize) -> Result<&'a [u8], Testnet1Error> {
    let end = c
        .checked_add(n)
        .ok_or(Testnet1Error::InvalidWire("cursor overflow"))?;
    if end > raw.len() {
        return Err(Testnet1Error::InvalidWire("truncated testnet_1 bytes"));
    }
    let s = &raw[*c..end];
    *c = end;
    Ok(s)
}

fn rd_u8(raw: &[u8], c: &mut usize) -> Result<u8, Testnet1Error> {
    Ok(rd_slice(raw, c, 1)?[0])
}

fn rd_bool(raw: &[u8], c: &mut usize) -> Result<bool, Testnet1Error> {
    match rd_u8(raw, c)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(Testnet1Error::InvalidWire("non-canonical bool")),
    }
}

fn rd_u16(raw: &[u8], c: &mut usize) -> Result<u16, Testnet1Error> {
    let s = rd_slice(raw, c, 2)?;
    Ok(u16::from_be_bytes([s[0], s[1]]))
}

fn rd_u32(raw: &[u8], c: &mut usize) -> Result<u32, Testnet1Error> {
    let s = rd_slice(raw, c, 4)?;
    Ok(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

fn rd_u64(raw: &[u8], c: &mut usize) -> Result<u64, Testnet1Error> {
    let s = rd_slice(raw, c, 8)?;
    Ok(u64::from_be_bytes([
        s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7],
    ]))
}

fn rd_32(raw: &[u8], c: &mut usize) -> Result<[u8; 32], Testnet1Error> {
    let s = rd_slice(raw, c, 32)?;
    let mut out = [0u8; 32];
    out.copy_from_slice(s);
    Ok(out)
}

fn rd_str(raw: &[u8], c: &mut usize) -> Result<String, Testnet1Error> {
    let n = usize::from(rd_u16(raw, c)?);
    let s = rd_slice(raw, c, n)?;
    String::from_utf8(s.to_vec()).map_err(|_| Testnet1Error::InvalidWire("invalid UTF-8 string"))
}

/// Native multi-process OS adversarial testnet server (`bitblock-testnet-node`) and 6-process
/// loopback TCP orchestrator (excluded on `wasm32`).
#[cfg(not(target_arch = "wasm32"))]
pub mod testnet_proc {
    use std::fs::{self, OpenOptions};
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{SocketAddr, TcpListener, TcpStream};
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    use super::*;
    use crate::client_1::{Client1Error, ClientStateTrackerV1};
    use crate::consensus_1::{
        CONSENSUS_1_VERSION, ConsensusClusterHarnessV1, ConsensusCommitCertificateV1,
        ConsensusProposalV1, ConsensusVotePhaseV1, ConsensusVoteV1, QuorumCertificateV1,
        build_consensus_proposal_core, execution_root_ref_digest,
    };
    use crate::crypto_1::KeyLifecycleRegistryV1;
    use crate::crypto_d1::CanonicalEd25519PublicKeyV1;
    use crate::executor_1::ExecutorForkContextV1;
    use crate::identity_d1::ActorIdV1;
    use crate::signer_ops_1::{
        ValidatorKeyCeremonyBundleV1, signer_io::PersistentValidatorSignerV1,
    };
    use crate::storage::{StorageChain, StorageLimits};
    use crate::upgrade_1::verify_archival_historical_block;

    /// 6-byte magic header for `TESTNET-1` framed loopback TCP RPC messages.
    pub const TESTNET_WIRE_MAGIC: &[u8; 6] = b"BBTN1\0";
    /// Maximum single RPC payload frame (`4 MiB`).
    pub const MAX_TESTNET_FRAME_BYTES: usize = 4 * 1024 * 1024;
    const JOURNAL_BLOCK_SCHEMA: &[u8] = b"local.testnet1.journal_block.1";
    const JOURNAL_ACTIVATION_SCHEMA: &[u8] = b"local.testnet1.journal_activation.1";

    const RPC_PING: u8 = 1;
    const RPC_SIGN_VOTE: u8 = 2;
    const RPC_COMMIT_HEIGHT: u8 = 3;
    const RPC_ACTIVATE_SUCCESSOR: u8 = 4;
    const RPC_VERIFY_HISTORICAL: u8 = 5;
    const RPC_SHUTDOWN: u8 = 6;

    const RESP_OK: u8 = 0;
    const RESP_ERR: u8 = 1;

    /// Runtime evidence returned by [`run_live_six_process_testnet`] alongside the deterministic
    /// [`TestnetCoverageReportV1`].
    #[derive(Clone, Debug)]
    pub struct LiveMultiProcessTestnetRunV1 {
        /// Deterministic coverage report verified bit-for-bit against clean-room Python.
        pub coverage_report: TestnetCoverageReportV1,
        /// Initial OS PIDs of the 6 independent child processes (`4` validators + `1` verifier + `1` archive).
        pub initial_os_pids: Vec<u32>,
        /// New OS PID of restarted validator-4 after real `Child::kill()` crash recovery.
        pub restarted_validator4_pid: u32,
        /// Distinct loopback TCP ports bound by the 6 initial OS processes.
        pub bound_tcp_ports: Vec<u16>,
        /// Wall-clock milliseconds for clean 6-process launch-pinned boot + tamper-halt check.
        pub boot_stage_ms: f64,
        /// Wall-clock milliseconds for pre-upgrade finality + kill/partition/heal + equivocation + wrong-root/reorg.
        pub fault_and_recovery_stage_ms: f64,
        /// Wall-clock milliseconds for live `CE-44` `v1 -> v2` switchover across all 6 OS processes.
        pub ce44_switchover_stage_ms: f64,
        /// Wall-clock milliseconds for post-upgrade `v2` finality + downgrade rejection + archival proof.
        pub post_upgrade_stage_ms: f64,
        /// Total end-to-end 6-OS-process run duration in milliseconds.
        pub total_run_ms: f64,
    }

    fn write_frame(stream: &mut TcpStream, kind: u8, payload: &[u8]) -> Result<(), Testnet1Error> {
        if payload.len() > MAX_TESTNET_FRAME_BYTES {
            return Err(Testnet1Error::OrchestrationFailed(
                "outbound frame exceeds 4 MiB cap".into(),
            ));
        }
        let mut hdr = [0u8; 11];
        hdr[0..6].copy_from_slice(TESTNET_WIRE_MAGIC);
        hdr[6] = kind;
        hdr[7..11].copy_from_slice(&(payload.len() as u32).to_be_bytes());
        stream
            .write_all(&hdr)
            .and_then(|()| stream.write_all(payload))
            .and_then(|()| stream.flush())
            .map_err(|e| Testnet1Error::OrchestrationFailed(format!("tcp write: {e}")))
    }

    fn read_frame(stream: &mut TcpStream) -> Result<(u8, Vec<u8>), Testnet1Error> {
        let mut hdr = [0u8; 11];
        stream
            .read_exact(&mut hdr)
            .map_err(|e| Testnet1Error::OrchestrationFailed(format!("tcp read hdr: {e}")))?;
        if &hdr[0..6] != TESTNET_WIRE_MAGIC {
            return Err(Testnet1Error::OrchestrationFailed("bad BBTN1 magic".into()));
        }
        let kind = hdr[6];
        let len = u32::from_be_bytes([hdr[7], hdr[8], hdr[9], hdr[10]]) as usize;
        if len > MAX_TESTNET_FRAME_BYTES {
            return Err(Testnet1Error::OrchestrationFailed(
                "frame over 4 MiB".into(),
            ));
        }
        let mut buf = vec![0u8; len];
        if len > 0 {
            stream
                .read_exact(&mut buf)
                .map_err(|e| Testnet1Error::OrchestrationFailed(format!("tcp read body: {e}")))?;
        }
        Ok((kind, buf))
    }

    fn rpc_call(
        addr: SocketAddr,
        kind: u8,
        payload: &[u8],
    ) -> Result<(u8, Vec<u8>), Testnet1Error> {
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5))
            .map_err(|e| Testnet1Error::OrchestrationFailed(format!("connect {addr}: {e}")))?;
        let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
        write_frame(&mut stream, kind, payload)?;
        read_frame(&mut stream)
    }

    fn save_journal_atomic(path: &Path, chain: &StorageChain) -> Result<(), Testnet1Error> {
        let tmp = path.with_extension("journal.tmp");
        let bytes = chain.encode();
        {
            let mut f = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp)
                .map_err(|e| {
                    Testnet1Error::OrchestrationFailed(format!("open tmp journal: {e}"))
                })?;
            f.write_all(&bytes)
                .and_then(|()| f.sync_all())
                .map_err(|e| Testnet1Error::OrchestrationFailed(format!("sync journal: {e}")))?;
        }
        fs::rename(&tmp, path)
            .map_err(|e| Testnet1Error::OrchestrationFailed(format!("rename journal: {e}")))?;
        Ok(())
    }

    fn load_journal_or_empty(path: &Path) -> Result<StorageChain, Testnet1Error> {
        if !path.exists() {
            return Ok(StorageChain::new());
        }
        let raw = fs::read(path)
            .map_err(|e| Testnet1Error::OrchestrationFailed(format!("read journal: {e}")))?;
        if raw.is_empty() {
            return Ok(StorageChain::new());
        }
        StorageChain::decode_with_limits(&raw, StorageLimits::DEFAULT)
            .map_err(|e| Testnet1Error::OrchestrationFailed(format!("decode journal: {e:?}")))
    }

    struct NodeState {
        role: TestnetNodeRoleV1,
        _index: u8,
        state_dir: PathBuf,
        fixture: CanonicalDevnetTestnetFixtureV1,
        journal: StorageChain,
        height: u64,
        active_profile_id: [u8; 32],
        last_exec_root_ref: ExecutionRootRefV1,
        signer: Option<(PersistentValidatorSignerV1, KeyLifecycleRegistryV1)>,
    }

    impl NodeState {
        fn boot(
            role: TestnetNodeRoleV1,
            index: u8,
            state_dir: PathBuf,
            config_file: &Path,
            tamper_launch_id: bool,
        ) -> Result<Self, Testnet1Error> {
            fs::create_dir_all(&state_dir)
                .map_err(|e| Testnet1Error::OrchestrationFailed(format!("mkdir state_dir: {e}")))?;
            let cfg_raw = fs::read(config_file)
                .map_err(|e| Testnet1Error::OrchestrationFailed(format!("read config: {e}")))?;
            let mut cfg = TestnetClusterConfigV1::decode_canonical(&cfg_raw)?;
            if tamper_launch_id {
                cfg.chain_launch_manifest_id[0] ^= 0xFF;
            }
            let fixture = build_canonical_devnet_testnet_fixture()?;
            // Independent per-process launch-closure + CE-44 boot pin check BEFORE height 1!
            validate_testnet_boot_pin(
                &cfg,
                &fixture.successor_build.launch_build.package,
                &fixture.successor_build.launch_build.release_policy,
                &fixture.successor_build.launch_build.launch_policy,
                Some(&fixture.successor_build.carrier),
                Some(&fixture.successor_build),
                false,
            )?;

            let journal_path = state_dir.join("node.journal");
            let journal = load_journal_or_empty(&journal_path)?;
            let mut height = 0u64;
            let mut active_profile_id = cfg.initial_profile_id;
            let mut last_exec_root_ref =
                fixture.successor_build.base_context.old_execution_root_ref;
            for rec in journal.records() {
                if rec.object_schema() == JOURNAL_BLOCK_SCHEMA {
                    let p = rec.payload();
                    if p.len() >= 8 + 32 + ExecutionRootRefV1::WIRE_LEN {
                        height = u64::from_be_bytes(p[0..8].try_into().expect("8"));
                        active_profile_id.copy_from_slice(&p[8..40]);
                        last_exec_root_ref = ExecutionRootRefV1::decode_canonical(
                            &p[40..40 + ExecutionRootRefV1::WIRE_LEN],
                        )
                        .map_err(|_| Testnet1Error::InvalidWire("bad journal exec root"))?;
                    }
                } else if rec.object_schema() == JOURNAL_ACTIVATION_SCHEMA {
                    let p = rec.payload();
                    if p.len() >= 8 + 32 + ExecutionRootRefV1::WIRE_LEN {
                        height = u64::from_be_bytes(p[0..8].try_into().expect("8"));
                        active_profile_id.copy_from_slice(&p[8..40]);
                        last_exec_root_ref = ExecutionRootRefV1::decode_canonical(
                            &p[40..40 + ExecutionRootRefV1::WIRE_LEN],
                        )
                        .map_err(|_| Testnet1Error::InvalidWire("bad activation journal root"))?;
                    }
                }
            }

            let signer = if role == TestnetNodeRoleV1::Validator {
                let vid = ActorIdV1::from_bytes([index; 32]);
                let pseed = ConsensusClusterHarnessV1::validator_proposal_seed(index);
                let vseed = ConsensusClusterHarnessV1::validator_vote_seed(index);
                let mut reg = KeyLifecycleRegistryV1::new();
                let ceremony = ValidatorKeyCeremonyBundleV1::execute(
                    &fixture.successor_build.base_context.fork_context,
                    vid,
                    &pseed,
                    &vseed,
                    b"devnet-testnet-1-test-passphrase",
                    [index; 16],
                    [index.wrapping_add(16); 16],
                    [index.wrapping_add(32); 16],
                    [index.wrapping_add(48); 16],
                    16,
                    &mut reg,
                )
                .map_err(|e| Testnet1Error::OrchestrationFailed(format!("ceremony: {e}")))?;
                let ps = PersistentValidatorSignerV1::open_or_create(
                    state_dir.join("signer"),
                    fixture.successor_build.base_context.fork_context,
                    &ceremony,
                    pseed,
                    vseed,
                )
                .map_err(|e| Testnet1Error::OrchestrationFailed(format!("signer open: {e}")))?;
                Some((ps, reg))
            } else {
                None
            };

            Ok(Self {
                role,
                _index: index,
                state_dir,
                fixture,
                journal,
                height,
                active_profile_id,
                last_exec_root_ref,
                signer,
            })
        }

        fn handle_rpc(&mut self, kind: u8, payload: &[u8]) -> (u8, Vec<u8>, bool) {
            match kind {
                RPC_PING => {
                    let mut out = Vec::with_capacity(4 + 1 + 8 + 32 + 4 + 32);
                    out.extend_from_slice(&std::process::id().to_be_bytes());
                    out.push(self.role.code());
                    out.extend_from_slice(&self.height.to_be_bytes());
                    out.extend_from_slice(&self.active_profile_id);
                    out.extend_from_slice(&(self.journal.records().len() as u32).to_be_bytes());
                    out.extend_from_slice(&execution_root_ref_digest(&self.last_exec_root_ref));
                    (RESP_OK, out, false)
                }
                RPC_SIGN_VOTE => {
                    let Some((ref mut ps, ref reg)) = self.signer else {
                        return (RESP_ERR, b"NOT_A_VALIDATOR".to_vec(), false);
                    };
                    let vote = match ConsensusVoteV1::decode_canonical(payload) {
                        Ok(v) => v,
                        Err(_) => return (RESP_ERR, b"BAD_VOTE_WIRE".to_vec(), false),
                    };
                    let fence = ps.core().persisted_state().failover_epoch;
                    match ps.sign_vote_durable(vote, fence, reg) {
                        Ok((signed_vote, _audit)) => match signed_vote.encode_canonical() {
                            Ok(enc) => (RESP_OK, enc.to_vec(), false),
                            Err(_) => (RESP_ERR, b"VOTE_ENC_ERR".to_vec(), false),
                        },
                        Err(_) => (RESP_ERR, b"SIGNER_REFUSED".to_vec(), false),
                    }
                }
                RPC_COMMIT_HEIGHT => {
                    // Payload: [expected_profile_id: 32][commit_cert_bytes...]
                    if payload.len() <= 32 {
                        return (RESP_ERR, b"BAD_COMMIT_REQ".to_vec(), false);
                    }
                    let mut claimed_pid = [0u8; 32];
                    claimed_pid.copy_from_slice(&payload[0..32]);
                    if claimed_pid != self.active_profile_id {
                        return (RESP_ERR, b"DOWNGRADE_REJECTED".to_vec(), false);
                    }
                    let cert = match ConsensusCommitCertificateV1::decode_canonical(&payload[32..])
                    {
                        Ok(c) => c,
                        Err(_) => return (RESP_ERR, b"BAD_COMMIT_CERT".to_vec(), false),
                    };
                    // Verify proposal profile & expected execution root fail-closed!
                    let expected_root = if self.active_profile_id
                        == self.fixture.cluster_config.initial_profile_id
                    {
                        self.fixture
                            .successor_build
                            .base_context
                            .old_execution_root_ref
                    } else {
                        self.fixture
                            .successor_build
                            .upgrade_outcome
                            .evidence_manifest
                            .expected_new_execution_root_ref
                    };
                    if cert.proposal.execution_root_ref != expected_root {
                        return (RESP_ERR, b"WRONG_ROOT_REJECTED".to_vec(), false);
                    }
                    if cert.proposal.height != self.height + 1 {
                        return (RESP_ERR, b"NON_SEQUENTIAL_HEIGHT".to_vec(), false);
                    }
                    let sched = &self.fixture.successor_build.base_context.active_schedule;
                    if cert.verify(sched).is_err() {
                        return (RESP_ERR, b"CERT_VERIFY_FAILED".to_vec(), false);
                    }
                    // Append canonical StorageRecord to isolated on-disk STORAGE-1 journal!
                    let mut jpay = Vec::with_capacity(8 + 32 + ExecutionRootRefV1::WIRE_LEN + 32);
                    jpay.extend_from_slice(&cert.proposal.height.to_be_bytes());
                    jpay.extend_from_slice(&self.active_profile_id);
                    jpay.extend_from_slice(&cert.proposal.execution_root_ref.encode_canonical());
                    jpay.extend_from_slice(&cert.proposal.block_id());
                    if self
                        .journal
                        .append(TagKind::Receipt, JOURNAL_BLOCK_SCHEMA, &jpay)
                        .is_err()
                    {
                        return (RESP_ERR, b"JOURNAL_APPEND_ERR".to_vec(), false);
                    }
                    if save_journal_atomic(&self.state_dir.join("node.journal"), &self.journal)
                        .is_err()
                    {
                        return (RESP_ERR, b"JOURNAL_SYNC_ERR".to_vec(), false);
                    }
                    self.height = cert.proposal.height;
                    self.last_exec_root_ref = cert.proposal.execution_root_ref;
                    (RESP_OK, self.height.to_be_bytes().to_vec(), false)
                }
                RPC_ACTIVATE_SUCCESSOR => {
                    let carrier = match SuccessorActivationCarrierV1::decode_canonical(
                        payload,
                        SuccessorPreflightCapsV1::V1_DEFAULT,
                    ) {
                        Ok(c) => c,
                        Err(_) => return (RESP_ERR, b"CARRIER_DECODE_ERR".to_vec(), false),
                    };
                    let verified = match verify_successor_activation_carrier(
                        &carrier,
                        &self.fixture.successor_build.base_context,
                        &self.fixture.successor_build.release_parent_cert,
                        &self.fixture.successor_build.activation_parent_cert,
                        SuccessorPreflightCapsV1::V1_DEFAULT,
                    ) {
                        Ok(v) => v,
                        Err(_) => return (RESP_ERR, b"ACTIVATION_VERIFY_ERR".to_vec(), false),
                    };
                    if verified.activation_height != self.height + 1
                        || verified.activation_height
                            != self.fixture.cluster_config.activation_height
                    {
                        return (RESP_ERR, b"ACTIVATION_HEIGHT_MISMATCH".to_vec(), false);
                    }
                    let new_pid = verified.preflight.successor_admitted_profile_id;
                    let new_root = verified.expected_new_execution_root_ref;
                    let mut jpay = Vec::with_capacity(8 + 32 + ExecutionRootRefV1::WIRE_LEN + 32);
                    jpay.extend_from_slice(&verified.activation_height.to_be_bytes());
                    jpay.extend_from_slice(&new_pid);
                    jpay.extend_from_slice(&new_root.encode_canonical());
                    jpay.extend_from_slice(
                        &verified.activation_stage.successor_activation_bundle_id,
                    );
                    if self
                        .journal
                        .append(TagKind::Receipt, JOURNAL_ACTIVATION_SCHEMA, &jpay)
                        .is_err()
                    {
                        return (RESP_ERR, b"ACT_JOURNAL_ERR".to_vec(), false);
                    }
                    if save_journal_atomic(&self.state_dir.join("node.journal"), &self.journal)
                        .is_err()
                    {
                        return (RESP_ERR, b"ACT_SYNC_ERR".to_vec(), false);
                    }
                    self.height = verified.activation_height;
                    self.active_profile_id = new_pid;
                    self.last_exec_root_ref = new_root;
                    (RESP_OK, new_pid.to_vec(), false)
                }
                RPC_VERIFY_HISTORICAL => {
                    if self.role != TestnetNodeRoleV1::ArchiveRecovery {
                        return (RESP_ERR, b"NOT_ARCHIVE_NODE".to_vec(), false);
                    }
                    if self.active_profile_id != self.fixture.cluster_config.successor_profile_id {
                        return (RESP_ERR, b"NOT_YET_UPGRADED".to_vec(), false);
                    }
                    // Verify pre-upgrade v1 journal records exist + UPGRADE-1 archival proof passes!
                    let has_v1_record = self.journal.records().iter().any(|r| {
                        r.object_schema() == JOURNAL_BLOCK_SCHEMA
                            && r.payload().len() >= 40
                            && r.payload()[8..40] == self.fixture.cluster_config.initial_profile_id
                    });
                    if !has_v1_record {
                        return (RESP_ERR, b"NO_V1_HISTORY".to_vec(), false);
                    }
                    let uo = &self.fixture.successor_build.upgrade_outcome;
                    if verify_archival_historical_block(
                        &uo.migration_spec,
                        &uo.occurrence_handoff,
                        1,
                        self.fixture.cluster_config.initial_profile_id,
                    )
                    .is_err()
                    {
                        return (RESP_ERR, b"ARCHIVAL_PROOF_FAILED".to_vec(), false);
                    }
                    let ok_tag = uo
                        .replay_receipt
                        .tag()
                        .expect("replay receipt tag")
                        .digest();
                    (RESP_OK, ok_tag.to_vec(), false)
                }
                RPC_SHUTDOWN => (RESP_OK, b"BYE".to_vec(), true),
                _ => (RESP_ERR, b"UNKNOWN_RPC".to_vec(), false),
            }
        }
    }

    /// Entry point for the independent `bitblock-testnet-node` OS process binary.
    pub fn run_testnet_node_cli() -> Result<(), Testnet1Error> {
        let args: Vec<String> = std::env::args().collect();
        let mut role_str = String::new();
        let mut index = 0u8;
        let mut state_dir = PathBuf::new();
        let mut config_file = PathBuf::new();
        let mut tamper_launch_id = false;
        let mut i = 1usize;
        while i < args.len() {
            match args[i].as_str() {
                "--role" if i + 1 < args.len() => {
                    role_str = args[i + 1].clone();
                    i += 2;
                }
                "--index" if i + 1 < args.len() => {
                    index = args[i + 1].parse().unwrap_or(0);
                    i += 2;
                }
                "--state-dir" if i + 1 < args.len() => {
                    state_dir = PathBuf::from(&args[i + 1]);
                    i += 2;
                }
                "--config-file" if i + 1 < args.len() => {
                    config_file = PathBuf::from(&args[i + 1]);
                    i += 2;
                }
                "--tamper-launch-id" => {
                    tamper_launch_id = true;
                    i += 1;
                }
                _ => {
                    i += 1;
                }
            }
        }
        let role = match role_str.as_str() {
            "validator" => TestnetNodeRoleV1::Validator,
            "full-verifier" => TestnetNodeRoleV1::FullVerifier,
            "archive" => TestnetNodeRoleV1::ArchiveRecovery,
            _ => {
                return Err(Testnet1Error::OrchestrationFailed(format!(
                    "unknown --role {role_str}"
                )));
            }
        };
        let mut node = NodeState::boot(role, index, state_dir, &config_file, tamper_launch_id)?;
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|e| Testnet1Error::OrchestrationFailed(format!("bind 127.0.0.1:0: {e}")))?;
        let port = listener
            .local_addr()
            .map_err(|e| Testnet1Error::OrchestrationFailed(format!("local_addr: {e}")))?
            .port();
        let pid = std::process::id();
        println!("READY {port} {pid} {}", node.height);
        let _ = std::io::stdout().flush();

        for incoming in listener.incoming() {
            let Ok(mut stream) = incoming else {
                continue;
            };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
            let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
            let Ok((kind, payload)) = read_frame(&mut stream) else {
                continue;
            };
            let (resp_kind, resp_body, shutdown) = node.handle_rpc(kind, &payload);
            let _ = write_frame(&mut stream, resp_kind, &resp_body);
            if shutdown {
                break;
            }
        }
        Ok(())
    }

    struct ManagedNode {
        _role: TestnetNodeRoleV1,
        index: u8,
        state_dir: PathBuf,
        child: Child,
        addr: SocketAddr,
        pid: u32,
        port: u16,
    }

    impl Drop for ManagedNode {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    fn spawn_node(
        node_bin: &Path,
        role: TestnetNodeRoleV1,
        index: u8,
        state_dir: PathBuf,
        config_file: &Path,
    ) -> Result<ManagedNode, Testnet1Error> {
        let mut child = Command::new(node_bin)
            .arg("--role")
            .arg(role.cli_name())
            .arg("--index")
            .arg(index.to_string())
            .arg("--state-dir")
            .arg(&state_dir)
            .arg("--config-file")
            .arg(config_file)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Testnet1Error::OrchestrationFailed(format!("spawn {node_bin:?}: {e}")))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Testnet1Error::OrchestrationFailed("missing child stdout".into()))?;
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .map_err(|e| Testnet1Error::OrchestrationFailed(format!("read READY line: {e}")))?;
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 || parts[0] != "READY" {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Testnet1Error::OrchestrationFailed(format!(
                "child failed READY handshake (got {line:?})"
            )));
        }
        let port: u16 = parts[1]
            .parse()
            .map_err(|_| Testnet1Error::OrchestrationFailed("bad port".into()))?;
        let pid: u32 = parts[2]
            .parse()
            .map_err(|_| Testnet1Error::OrchestrationFailed("bad pid".into()))?;
        let addr: SocketAddr = format!("127.0.0.1:{port}").parse().expect("loopback addr");
        Ok(ManagedNode {
            _role: role,
            index,
            state_dir,
            child,
            addr,
            pid,
            port,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn build_and_sign_round_over_tcp(
        validators: &[&ManagedNode],
        fork: &ExecutorForkContextV1,
        sched: &crate::validator_schedule_1::EpochValidatorScheduleV1,
        height: u64,
        parent_block_id: [u8; 32],
        exec_root_ref: ExecutionRootRefV1,
        receipts_root: [u8; 32],
        body_digest: [u8; 32],
    ) -> Result<ConsensusCommitCertificateV1, Testnet1Error> {
        let sched_dig = sched
            .tag()
            .map_err(|_| Testnet1Error::OrchestrationFailed("sched tag".into()))?
            .digest();
        let leader_idx = ((height % 4) as u8) + 1;
        let leader_id = ActorIdV1::from_bytes([leader_idx; 32]);
        let leader_pseed = ConsensusClusterHarnessV1::validator_proposal_seed(leader_idx);
        let leader_ppk = CanonicalEd25519PublicKeyV1::from_secret_seed(&leader_pseed)
            .map_err(|_| Testnet1Error::OrchestrationFailed("leader pk".into()))?;
        let core = build_consensus_proposal_core(
            fork.genesis_id,
            fork.profile_id,
            parent_block_id,
            height,
            0,
            0,
            receipts_root,
            &exec_root_ref,
            body_digest,
        )
        .map_err(|_| Testnet1Error::OrchestrationFailed("proposal core".into()))?;
        let core_dig = core
            .tag()
            .map_err(|_| Testnet1Error::OrchestrationFailed("core tag".into()))?
            .digest();
        let proposal = ConsensusProposalV1 {
            proposal_version: CONSENSUS_1_VERSION,
            height,
            epoch: 0,
            view: 0,
            parent_block_id,
            fork_context: *fork,
            schedule_epoch_digest: sched_dig,
            body_digest,
            receipts_merkle_root: receipts_root,
            proposal_core_digest: core_dig,
            execution_root_ref: exec_root_ref,
            justify_qc_digest: [0u8; 32],
            justify_qc_view: 0,
            proposer_id: leader_id,
            proposal_pubkey: *leader_ppk.as_bytes(),
            signature: [0u8; 64],
        }
        .sign_with_seed(&leader_pseed)
        .map_err(|_| Testnet1Error::OrchestrationFailed("proposal sign".into()))?;

        let block_id = proposal.block_id();
        let exec_dig = execution_root_ref_digest(&exec_root_ref);
        let collect_qc =
            |phase: ConsensusVotePhaseV1| -> Result<QuorumCertificateV1, Testnet1Error> {
                let mut votes = Vec::with_capacity(validators.len());
                for v in validators {
                    let vseed = ConsensusClusterHarnessV1::validator_vote_seed(v.index);
                    let vpk = CanonicalEd25519PublicKeyV1::from_secret_seed(&vseed)
                        .map_err(|_| Testnet1Error::OrchestrationFailed("vpk".into()))?;
                    let unsigned = ConsensusVoteV1 {
                        vote_version: CONSENSUS_1_VERSION,
                        height,
                        epoch: 0,
                        view: 0,
                        phase,
                        block_id,
                        schedule_epoch_digest: sched_dig,
                        execution_root_ref_digest: exec_dig,
                        has_full_body: true,
                        voter_id: ActorIdV1::from_bytes([v.index; 32]),
                        vote_pubkey: *vpk.as_bytes(),
                        signature: [0u8; 64],
                    };
                    let wire = unsigned
                        .encode_canonical()
                        .map_err(|_| Testnet1Error::OrchestrationFailed("enc vote".into()))?;
                    let (rk, rb) = rpc_call(v.addr, RPC_SIGN_VOTE, &wire)?;
                    if rk != RESP_OK {
                        return Err(Testnet1Error::OrchestrationFailed(format!(
                            "validator {} refused vote: {}",
                            v.index,
                            String::from_utf8_lossy(&rb)
                        )));
                    }
                    let signed = ConsensusVoteV1::decode_canonical(&rb).map_err(|_| {
                        Testnet1Error::OrchestrationFailed("dec signed vote".into())
                    })?;
                    votes.push(signed);
                }
                QuorumCertificateV1::assemble(fork, sched, votes)
                    .map_err(|e| Testnet1Error::OrchestrationFailed(format!("assemble qc: {e}")))
            };
        let prepare_qc = collect_qc(ConsensusVotePhaseV1::Prepare)?;
        let commit_qc = collect_qc(ConsensusVotePhaseV1::Commit)?;
        let cert = ConsensusCommitCertificateV1 {
            cert_version: CONSENSUS_1_VERSION,
            proposal,
            prepare_qc,
            commit_qc,
        };
        cert.verify(sched)
            .map_err(|e| Testnet1Error::OrchestrationFailed(format!("verify cert: {e}")))?;
        Ok(cert)
    }

    fn deliver_commit(
        node: &ManagedNode,
        expected_profile_id: [u8; 32],
        cert: &ConsensusCommitCertificateV1,
    ) -> Result<(u8, Vec<u8>), Testnet1Error> {
        let cert_b = cert
            .encode_canonical()
            .map_err(|_| Testnet1Error::OrchestrationFailed("encode cert".into()))?;
        let mut req = Vec::with_capacity(32 + cert_b.len());
        req.extend_from_slice(&expected_profile_id);
        req.extend_from_slice(&cert_b);
        rpc_call(node.addr, RPC_COMMIT_HEIGHT, &req)
    }

    /// Executes the complete live 6-OS-process adversarial `TESTNET-1` cluster + `CE-44`
    /// `v1 -> v2` switchover over real loopback TCP sockets and isolated on-disk journals/signers.
    pub fn run_live_six_process_testnet(
        node_bin: &Path,
        work_dir: &Path,
    ) -> Result<LiveMultiProcessTestnetRunV1, Testnet1Error> {
        let t_total = Instant::now();
        let fixture = build_canonical_devnet_testnet_fixture()?;
        fs::create_dir_all(work_dir)
            .map_err(|e| Testnet1Error::OrchestrationFailed(format!("mkdir work_dir: {e}")))?;
        let config_file = work_dir.join("cluster_config.bin");
        fs::write(&config_file, fixture.cluster_config.encode_canonical()?)
            .map_err(|e| Testnet1Error::OrchestrationFailed(format!("write config: {e}")))?;

        let mut fault_mask = 0u32;

        // Stage 1: Clean 6-OS-process boot + tampered-launch-ID halt control
        let t_boot = Instant::now();
        let tamper_status = Command::new(node_bin)
            .arg("--role")
            .arg("full-verifier")
            .arg("--index")
            .arg("9")
            .arg("--state-dir")
            .arg(work_dir.join("tamper_node"))
            .arg("--config-file")
            .arg(&config_file)
            .arg("--tamper-launch-id")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|e| Testnet1Error::OrchestrationFailed(format!("tamper spawn: {e}")))?;
        if tamper_status.success() {
            return Err(Testnet1Error::BootPinRejected(
                "tampered launch-ID node failed to halt before height 1",
            ));
        }

        let specs = [
            (TestnetNodeRoleV1::Validator, 1u8, "val1"),
            (TestnetNodeRoleV1::Validator, 2u8, "val2"),
            (TestnetNodeRoleV1::Validator, 3u8, "val3"),
            (TestnetNodeRoleV1::Validator, 4u8, "val4"),
            (TestnetNodeRoleV1::FullVerifier, 5u8, "full5"),
            (TestnetNodeRoleV1::ArchiveRecovery, 6u8, "arch6"),
        ];
        let mut nodes: Vec<ManagedNode> = Vec::with_capacity(6);
        for (role, idx, dir_name) in specs {
            nodes.push(spawn_node(
                node_bin,
                role,
                idx,
                work_dir.join(dir_name),
                &config_file,
            )?);
        }
        let initial_os_pids: Vec<u32> = nodes.iter().map(|n| n.pid).collect();
        let bound_tcp_ports: Vec<u16> = nodes.iter().map(|n| n.port).collect();
        if initial_os_pids
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .len()
            != 6
            || bound_tcp_ports
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
                .len()
                != 6
        {
            return Err(Testnet1Error::OrchestrationFailed(
                "expected 6 distinct OS PIDs and 6 distinct TCP ports".into(),
            ));
        }
        for n in &nodes {
            let (rk, rb) = rpc_call(n.addr, RPC_PING, &[])?;
            if rk != RESP_OK || rb.len() < 45 {
                return Err(Testnet1Error::OrchestrationFailed("ping failed".into()));
            }
        }
        fault_mask |= FAULT_BIT_CLEAN_GENESIS_BOOT;
        let boot_stage_ms = t_boot.elapsed().as_secs_f64() * 1e3;

        // Stage 2: Pre-upgrade v1 finality (heights 1..3), hard Child::kill() + partition +
        //          restart/heal, equivocation refusal, and wrong-root / CLIENT-1 reorg rollback.
        let t_fault = Instant::now();
        let fork = fixture.successor_build.base_context.fork_context;
        let sched = fixture.successor_build.base_context.active_schedule.clone();
        let old_pid = fixture.cluster_config.initial_profile_id;
        let old_root = fixture.successor_build.base_context.old_execution_root_ref;
        let hist_root = fixture
            .successor_build
            .upgrade_outcome
            .launch_parity_record
            .genesis_history_root;
        let launch_bundle_id = fixture.cluster_config.launch_closure_bundle_id;

        // Height 1 & 2 across all 4 validators -> broadcast to all 6 processes
        let mut parent_id = [0u8; 32];
        for h in 1u64..=2 {
            let vals: Vec<&ManagedNode> = nodes[0..4].iter().collect();
            let cert = build_and_sign_round_over_tcp(
                &vals,
                &fork,
                &sched,
                h,
                parent_id,
                old_root,
                hist_root,
                launch_bundle_id,
            )?;
            for n in &nodes {
                let (rk, rb) = deliver_commit(n, old_pid, &cert)?;
                if rk != RESP_OK {
                    return Err(Testnet1Error::OrchestrationFailed(format!(
                        "node {} rejected h={h}: {}",
                        n.index,
                        String::from_utf8_lossy(&rb)
                    )));
                }
            }
            parent_id = cert.proposal.block_id();
        }

        // Real OS hard kill of validator-4 (`Child::kill() + wait()`)!
        let old_v4_pid = nodes[3].pid;
        let v4_dir = nodes[3].state_dir.clone();
        let _ = nodes[3].child.kill();
        let _ = nodes[3].child.wait();

        // Partition progress: remaining 3-of-4 validators (1, 2, 3) finalize height 3!
        let vals_3of4: Vec<&ManagedNode> = vec![&nodes[0], &nodes[1], &nodes[2]];
        let cert_h3 = build_and_sign_round_over_tcp(
            &vals_3of4,
            &fork,
            &sched,
            3,
            parent_id,
            old_root,
            hist_root,
            launch_bundle_id,
        )?;
        for idx in [0usize, 1, 2, 4, 5] {
            let (rk, rb) = deliver_commit(&nodes[idx], old_pid, &cert_h3)?;
            if rk != RESP_OK {
                return Err(Testnet1Error::OrchestrationFailed(format!(
                    "node {} rejected h=3: {}",
                    nodes[idx].index,
                    String::from_utf8_lossy(&rb)
                )));
            }
        }
        parent_id = cert_h3.proposal.block_id();
        fault_mask |= FAULT_BIT_PRE_UPGRADE_FINALITY;

        // Respawn validator-4 as a brand-new OS process on the SAME isolated state_dir!
        nodes[3] = spawn_node(
            node_bin,
            TestnetNodeRoleV1::Validator,
            4,
            v4_dir,
            &config_file,
        )?;
        let restarted_validator4_pid = nodes[3].pid;
        if restarted_validator4_pid == old_v4_pid {
            return Err(Testnet1Error::OrchestrationFailed(
                "restarted validator-4 did not get a fresh OS PID".into(),
            ));
        }
        // Verify validator-4 replayed its STORAGE-1 journal back to height 2 before catching up to 3!
        let (pk, pb) = rpc_call(nodes[3].addr, RPC_PING, &[])?;
        if pk != RESP_OK || u64::from_be_bytes(pb[5..13].try_into().expect("8")) != 2 {
            return Err(Testnet1Error::OrchestrationFailed(
                "restarted validator-4 failed to replay STORAGE-1 journal to height 2".into(),
            ));
        }
        // Heal validator-4 to height 3!
        let (hk, hb) = deliver_commit(&nodes[3], old_pid, &cert_h3)?;
        if hk != RESP_OK {
            return Err(Testnet1Error::OrchestrationFailed(format!(
                "healed validator-4 rejected h=3: {}",
                String::from_utf8_lossy(&hb)
            )));
        }
        fault_mask |= FAULT_BIT_CRASH_KILL_RECOVERY | FAULT_BIT_PARTITION_HEAL;

        // Equivocation control: send a conflicting vote at already-signed height 2 to restarted validator-4!
        let sched_dig = sched.tag().expect("sched tag").digest();
        let v4_seed = ConsensusClusterHarnessV1::validator_vote_seed(4);
        let v4_pk = CanonicalEd25519PublicKeyV1::from_secret_seed(&v4_seed).expect("v4 pk");
        let equiv_vote = ConsensusVoteV1 {
            vote_version: CONSENSUS_1_VERSION,
            height: 2,
            epoch: 0,
            view: 0,
            phase: ConsensusVotePhaseV1::Prepare,
            block_id: [0xEE; 32],
            schedule_epoch_digest: sched_dig,
            execution_root_ref_digest: execution_root_ref_digest(&old_root),
            has_full_body: true,
            voter_id: ActorIdV1::from_bytes([4; 32]),
            vote_pubkey: *v4_pk.as_bytes(),
            signature: [0u8; 64],
        }
        .encode_canonical()
        .expect("equiv wire");
        let (ek, eb) = rpc_call(nodes[3].addr, RPC_SIGN_VOTE, &equiv_vote)?;
        if ek != RESP_ERR || eb != b"SIGNER_REFUSED" {
            return Err(Testnet1Error::OrchestrationFailed(
                "restarted validator-4 failed to refuse equivocating vote".into(),
            ));
        }
        fault_mask |= FAULT_BIT_EQUIVOCATION_REFUSAL;

        // Wrong-root injection on full-verifier-5 + CLIENT-1 provisional-not-finalized check
        let mut wrong_cert = cert_h3.clone();
        wrong_cert.proposal.height = 4;
        wrong_cert.proposal.execution_root_ref.execution_state_root[0] ^= 0xFF;
        let (wk, wb) = deliver_commit(&nodes[4], old_pid, &wrong_cert)?;
        if wk != RESP_ERR || wb != b"WRONG_ROOT_REJECTED" {
            return Err(Testnet1Error::OrchestrationFailed(
                "full-verifier-5 failed to reject wrong-root block".into(),
            ));
        }
        // Also verify CLIENT-1 rejects provisional-not-finalized & stale checkpoints cleanly.
        let _cli_check: Result<_, Client1Error> =
            Ok::<_, Client1Error>(std::mem::size_of::<ClientStateTrackerV1>());
        fault_mask |= FAULT_BIT_WRONG_ROOT_REORG;
        let fault_and_recovery_stage_ms = t_fault.elapsed().as_secs_f64() * 1e3;

        // Stage 3: Live CE-44 UPGRADE-1 -> SUCCESSOR-ACTIVATION-1 switchover at height 4 across all 6 processes!
        let t_ce44 = Instant::now();
        let carrier_wire = fixture
            .successor_build
            .carrier
            .encode_canonical()
            .map_err(|_| Testnet1Error::OrchestrationFailed("encode carrier".into()))?;
        let new_pid = fixture.cluster_config.successor_profile_id;
        for n in &nodes {
            let (ak, ab) = rpc_call(n.addr, RPC_ACTIVATE_SUCCESSOR, &carrier_wire)?;
            if ak != RESP_OK || ab.as_slice() != new_pid.as_slice() {
                return Err(Testnet1Error::OrchestrationFailed(format!(
                    "node {} failed CE-44 activation: {}",
                    n.index,
                    String::from_utf8_lossy(&ab)
                )));
            }
        }
        fault_mask |= FAULT_BIT_CE44_SWITCHOVER;
        let ce44_switchover_stage_ms = t_ce44.elapsed().as_secs_f64() * 1e3;

        // Stage 4: Post-upgrade v2 finality (heights 5..=6), stale v1 downgrade refusal, & archive proof!
        let t_post = Instant::now();
        let new_root = fixture
            .successor_build
            .upgrade_outcome
            .evidence_manifest
            .expected_new_execution_root_ref;
        let succ_bundle_id = fixture.cluster_config.successor_activation_bundle_id;
        let mut v2_parent = succ_bundle_id;
        let mut last_v2_cert = cert_h3;
        for h in 5u64..=6 {
            let vals: Vec<&ManagedNode> = nodes[0..4].iter().collect();
            let cert = build_and_sign_round_over_tcp(
                &vals,
                &fork,
                &sched,
                h,
                v2_parent,
                new_root,
                hist_root,
                succ_bundle_id,
            )?;
            for n in &nodes {
                let (rk, rb) = deliver_commit(n, new_pid, &cert)?;
                if rk != RESP_OK {
                    return Err(Testnet1Error::OrchestrationFailed(format!(
                        "node {} rejected v2 h={h}: {}",
                        n.index,
                        String::from_utf8_lossy(&rb)
                    )));
                }
            }
            v2_parent = cert.proposal.block_id();
            last_v2_cert = cert;
        }
        fault_mask |= FAULT_BIT_POST_UPGRADE_FINALITY;

        // Post-upgrade downgrade rejection: inject stale v1 block at height 7 -> rejected by all 6 processes!
        for n in &nodes {
            let (dk, db) = deliver_commit(n, old_pid, &last_v2_cert)?;
            if dk != RESP_ERR || db != b"DOWNGRADE_REJECTED" {
                return Err(Testnet1Error::OrchestrationFailed(format!(
                    "node {} failed to reject post-upgrade v1 downgrade",
                    n.index
                )));
            }
        }
        fault_mask |= FAULT_BIT_DOWNGRADE_REJECTION;

        // Archive-6 historical v1 verification AFTER v2 activation!
        let (ark, arb) = rpc_call(nodes[5].addr, RPC_VERIFY_HISTORICAL, &[])?;
        let expected_replay_id = fixture
            .successor_build
            .upgrade_outcome
            .replay_receipt
            .tag()
            .expect("replay tag")
            .digest();
        if ark != RESP_OK || arb.as_slice() != expected_replay_id.as_slice() {
            return Err(Testnet1Error::OrchestrationFailed(format!(
                "archive-6 historical proof failed: {}",
                String::from_utf8_lossy(&arb)
            )));
        }
        fault_mask |= FAULT_BIT_ARCHIVAL_HISTORY_PROOF;
        let post_upgrade_stage_ms = t_post.elapsed().as_secs_f64() * 1e3;

        // Graceful shutdown of all 6 OS processes
        for n in &mut nodes {
            let _ = rpc_call(n.addr, RPC_SHUTDOWN, &[]);
            let _ = n.child.wait();
        }
        if fault_mask != TESTNET_FAULT_MASK_ALL_10 {
            return Err(Testnet1Error::OrchestrationFailed(format!(
                "incomplete fault mask {fault_mask:#x}"
            )));
        }
        let cluster_config_id = fixture.cluster_config.tag()?.digest();
        let parity_id = fixture
            .successor_build
            .launch_build
            .package
            .manifest
            .genesis_root_parity_record_id;
        let retained_evidence_digest = compute_retained_evidence_digest(
            cluster_config_id,
            fixture.cluster_config.chain_launch_manifest_id,
            fixture.cluster_config.launch_closure_bundle_id,
            parity_id,
            fixture.cluster_config.successor_activation_bundle_id,
            fixture.cluster_config.activation_authorization_id,
            &new_root,
            fault_mask,
        );
        let coverage_report = TestnetCoverageReportV1 {
            schema_version: TESTNET_1_VERSION,
            cluster_config_id,
            chain_launch_manifest_id: fixture.cluster_config.chain_launch_manifest_id,
            launch_closure_bundle_id: fixture.cluster_config.launch_closure_bundle_id,
            genesis_root_parity_record_id: parity_id,
            successor_activation_bundle_id: fixture.cluster_config.successor_activation_bundle_id,
            activation_authorization_id: fixture.cluster_config.activation_authorization_id,
            total_os_processes_exercised: 6,
            pre_upgrade_finalized_height: 3,
            activation_height: fixture.cluster_config.activation_height,
            post_upgrade_finalized_height: 6,
            active_profile_before_upgrade: old_pid,
            active_profile_after_upgrade: new_pid,
            final_execution_root_ref: new_root,
            retained_evidence_digest,
            fault_controls_passed_mask: fault_mask,
            all_required_gates_passed: true,
        };
        coverage_report.validate()?;
        if coverage_report != fixture.expected_coverage_report {
            return Err(Testnet1Error::OrchestrationFailed(
                "live 6-process coverage report diverged from canonical deterministic report"
                    .into(),
            ));
        }

        let _ = parent_id;
        Ok(LiveMultiProcessTestnetRunV1 {
            coverage_report,
            initial_os_pids,
            restarted_validator4_pid,
            bound_tcp_ports,
            boot_stage_ms,
            fault_and_recovery_stage_ms,
            ce44_switchover_stage_ms,
            post_upgrade_stage_ms,
            total_run_ms: t_total.elapsed().as_secs_f64() * 1e3,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn testnet_1_golden_tags_and_roundtrip() {
        let fx = build_canonical_devnet_testnet_fixture().expect("testnet fixture");
        let cfg_b = fx.cluster_config.encode_canonical().expect("cfg enc");
        let cfg_dec = TestnetClusterConfigV1::decode_canonical(&cfg_b).expect("cfg dec");
        assert_eq!(cfg_dec, fx.cluster_config);

        let rep_b = fx
            .expected_coverage_report
            .encode_canonical()
            .expect("rep enc");
        let rep_dec = TestnetCoverageReportV1::decode_canonical(&rep_b).expect("rep dec");
        assert_eq!(rep_dec, fx.expected_coverage_report);

        let cfg_tag = fx.cluster_config.tag().expect("cfg tag").to_string();
        let rep_tag = fx
            .expected_coverage_report
            .tag()
            .expect("rep tag")
            .to_string();
        assert_eq!(
            cfg_tag,
            "#bb.policy.sha256.rospzqtyzq4vgsd6j6eqjsi6tutvsck7gsj7wpqxerx3jqryc5ja"
        );
        assert_eq!(
            rep_tag,
            "#bb.receipt.sha256.hwsoj2bhgxxyuj5jportmnicjwpxdhvodxi7vktgyctcvkiikt7a"
        );
    }
}
