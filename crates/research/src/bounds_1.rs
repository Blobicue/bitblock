//! `BOUNDS-1`: checked protocol admission, per-peer DoS, state-growth, local-VM (`CE-05`), and
//! deterministic safe-halt bounds over `PROFILE-SCOPE-D0`, `THREAT-1`, `LOCAL-BOUNDS-1`, and `VM-D1`.
//!
//! Enforces tight consumer-validator ceilings aligned with `THREAT-1`'s `2_000 ms` block slot:
//! - Worst-case block verification budget: `<= 400 ms` (`20%` of `2 s` slot).
//! - Block payload `<= 2 MiB`, tx `<= 16 KiB`, `<= 1,024` txs/block, `<= 2,048` Ed25519 sigs/block,
//!   `<= 16,384` ops/block, `<= 4,096` new state writes/block (bounding worst-case annual disk growth),
//!   `<= 12 KiB` / `<= 256`-depth proofs, `<= 1,048,576` hashes/block, `<= 32 MiB` pre-alloc ceiling.
//! - Per-peer DoS limits: `<= 4 MiB` inflight bytes and `<= 64` requests/sec per peer.
//! - Separate `LocalVmBoundsV1` sub-profile (`CE-05`) consuming `VmSpecD1::pre_execution_work_upper_bound`.
//! - **Fail-closed safe-halt:** any consensus-accepted artifact exceeding committed bounds produces
//!   canonical [`SafeHaltEvidenceV1`] instead of silently widening a limit (`tighten_only` forbids
//!   widening).
//!
//! Resource bounds do not establish economic fee sufficiency, fair inclusion, or liveness.

use std::fmt;

use crate::language_r1::{MAX_SPATIAL_INSTRUCTIONS, ParsedSpatialProgramV1};
use crate::local_bounds::LocalStorageProfile;
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::threat_1::ThreatModelV1;
use crate::vm_d1::VmSpecD1;

/// Canonical schema string for a [`ProtocolBoundsProfileV1`] policy object (`BOUNDS-1`).
pub const BOUNDS_1_PROTOCOL_PROFILE_SCHEMA: &[u8] = b"bitblock/bounds-1/protocol-profile/v1";
/// Canonical schema string for a [`LocalVmBoundsV1`] `CE-05` program-scope bounds profile.
pub const BOUNDS_1_LOCAL_VM_PROFILE_SCHEMA: &[u8] = b"bitblock/bounds-1/local-vm-bounds/v1";
/// Canonical schema string for a deterministic [`SafeHaltEvidenceV1`] over-bound artifact receipt.
pub const BOUNDS_1_SAFE_HALT_EVIDENCE_SCHEMA: &[u8] = b"bitblock/bounds-1/safe-halt-evidence/v1";

/// Maximum committed bytes per network block (`2 MiB`).
pub const DEFAULT_MAX_BLOCK_BYTES: u32 = 2 * 1024 * 1024;
/// Maximum committed bytes per transaction (`16 KiB`).
pub const DEFAULT_MAX_TX_BYTES: u32 = 16 * 1024;
/// Maximum admitted transactions per block.
pub const DEFAULT_MAX_BLOCK_TXS: u32 = 1_024;
/// Maximum strict Ed25519 signature verifications per block.
pub const DEFAULT_MAX_BLOCK_SIGNATURES: u32 = 2_048;
/// Maximum occupancy operations per block.
pub const DEFAULT_MAX_BLOCK_OPS: u32 = 16_384;
/// Maximum committed new state writes per block (tightened below draft `65,536` for disk bounds).
pub const DEFAULT_MAX_STATE_WRITES_PER_BLOCK: u32 = 4_096;
/// Maximum encoded authenticated proof bytes (`12 KiB`).
pub const DEFAULT_MAX_PROOF_BYTES: u32 = 12 * 1024;
/// Maximum authenticated Patricia trie proof depth (`256` bits covering 192-bit & 256-bit keys).
pub const DEFAULT_MAX_PROOF_DEPTH: u16 = 256;
/// Maximum cryptographic hash invocations budgeted per block.
pub const DEFAULT_MAX_HASHES_PER_BLOCK: u32 = 1_048_576;
/// Maximum pre-allocation byte budget per block (`32 MiB`).
pub const DEFAULT_MAX_ALLOC_BYTES_PER_BLOCK: u32 = 32 * 1024 * 1024;
/// Target worst-case block verification latency budget (`400 ms` = 20% of `2_000 ms` block slot).
pub const DEFAULT_MAX_VERIFY_BUDGET_MS: u32 = 400;
/// Maximum concurrent inflight bytes buffered from one remote peer (`4 MiB`).
pub const DEFAULT_MAX_PEER_INFLIGHT_BYTES: u32 = 4 * 1024 * 1024;
/// Maximum remote RPC / sync requests accepted per second per peer.
pub const DEFAULT_MAX_PEER_REQUESTS_PER_SEC: u16 = 64;

/// Errors returned by `BOUNDS-1` profile validation, codecs, and tightening checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Bounds1Error {
    /// Profile parameters were zero, inconsistent with `LOCAL-BOUNDS-1`, or out of bounds.
    InvalidProfile(&'static str),
    /// Canonical wire buffer had wrong length or invalid field encoding.
    InvalidWire(&'static str),
    /// Candidate profile attempted to widen a committed limit (strictly forbidden).
    WideningForbidden(&'static str),
    /// Pre-allocation streaming preflight rejected an over-bound or overflowing input.
    PreflightViolation(BoundsViolationV1),
}

impl fmt::Display for Bounds1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Bounds1Error {}

/// Typed violation returned before allocation when an input exceeds committed `BOUNDS-1` limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BoundsViolationV1 {
    /// Canonical violation class code (`1..=14`).
    pub code: u8,
    /// Observed requested quantity (`u64::MAX` on checked arithmetic overflow).
    pub observed: u64,
    /// Committed limit that was exceeded.
    pub limit: u64,
}

/// Violation code constants for deterministic wire & safe-halt classification.
pub const VIOLATION_BLOCK_BYTES: u8 = 1;
/// Transaction byte length exceeded `max_tx_bytes`.
pub const VIOLATION_TX_BYTES: u8 = 2;
/// Block transaction count exceeded `max_block_txs`.
pub const VIOLATION_BLOCK_TXS: u8 = 3;
/// Block Ed25519 signature count exceeded `max_block_signatures`.
pub const VIOLATION_BLOCK_SIGNATURES: u8 = 4;
/// Block occupancy operation count exceeded `max_block_ops`.
pub const VIOLATION_BLOCK_OPS: u8 = 5;
/// Block new state write count exceeded `max_state_writes_per_block`.
pub const VIOLATION_STATE_WRITES: u8 = 6;
/// Proof byte length exceeded `max_proof_bytes`.
pub const VIOLATION_PROOF_BYTES: u8 = 7;
/// Proof depth exceeded `max_proof_depth`.
pub const VIOLATION_PROOF_DEPTH: u8 = 8;
/// Estimated hash invocations exceeded `max_hashes_per_block`.
pub const VIOLATION_HASH_BUDGET: u8 = 9;
/// Estimated allocation bytes exceeded `max_alloc_bytes_per_block`.
pub const VIOLATION_ALLOC_BUDGET: u8 = 10;
/// Peer inflight bytes or request rate exceeded per-peer DoS cap.
pub const VIOLATION_PEER_DOS: u8 = 11;
/// Local VM program count, instruction count, gas, or output writes exceeded `LocalVmBoundsV1`.
pub const VIOLATION_LOCAL_VM: u8 = 12;
/// Checked arithmetic overflow occurred during streaming preflight before allocation.
pub const VIOLATION_ARITHMETIC_OVERFLOW: u8 = 13;

/// Pre-allocation streaming header summary inspected before materializing a block payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlockEnvelopePreflightHeaderV1 {
    /// Total outer encoded block byte length.
    pub block_bytes: u64,
    /// Number of transactions declared in the block.
    pub tx_count: u64,
    /// Maximum single transaction byte length declared in the block.
    pub max_tx_bytes_observed: u64,
    /// Total strict Ed25519 signature checks required by the block.
    pub signature_count: u64,
    /// Total occupancy operations declared in the block.
    pub op_count: u64,
    /// Total new committed state writes declared in the block.
    pub state_writes: u64,
    /// Maximum proof byte length carried by the block.
    pub max_proof_bytes_observed: u64,
    /// Maximum proof depth carried by the block.
    pub max_proof_depth_observed: u64,
    /// Pre-calculated cryptographic hash invocations required to verify the block.
    pub estimated_hashes: u64,
    /// Pre-calculated peak heap allocation bytes required to materialize the block.
    pub estimated_alloc_bytes: u64,
}

/// Canonical committed protocol admission, DoS, and state-growth bounds (`BOUNDS-1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProtocolBoundsProfileV1 {
    /// Schema version (`1`).
    pub schema_version: u16,
    /// Digest of the governing [`ThreatModelV1`].
    pub threat_model_digest: [u8; 32],
    /// Digest of the governing [`LocalStorageProfile`].
    pub local_bounds_digest: [u8; 32],
    /// Maximum encoded block bytes (`<= LocalStorageProfile::max_record_bytes`).
    pub max_block_bytes: u32,
    /// Maximum encoded transaction bytes (`<= max_block_bytes`).
    pub max_tx_bytes: u32,
    /// Maximum transactions admitted per block.
    pub max_block_txs: u32,
    /// Maximum Ed25519 signature verifications per block.
    pub max_block_signatures: u32,
    /// Maximum occupancy operations per block.
    pub max_block_ops: u32,
    /// Maximum committed new state writes per block.
    pub max_state_writes_per_block: u32,
    /// Maximum proof bytes per authenticated proof.
    pub max_proof_bytes: u32,
    /// Maximum proof bit depth (`<= 256`).
    pub max_proof_depth: u16,
    /// Maximum hash invocations per block.
    pub max_hashes_per_block: u32,
    /// Maximum peak pre-allocation bytes per block (`<= LocalStorageProfile::max_staged_write_bytes`).
    pub max_alloc_bytes_per_block: u32,
    /// Target worst-case block verification latency budget in milliseconds.
    pub max_verify_budget_ms: u32,
    /// Maximum inflight bytes buffered per remote peer.
    pub max_peer_inflight_bytes: u32,
    /// Maximum requests per second accepted per remote peer.
    pub max_peer_requests_per_sec: u16,
}

impl ProtocolBoundsProfileV1 {
    /// Fixed canonical big-endian wire length (`114` bytes).
    pub const WIRE_LEN: usize = 114;

    /// Constructs the tightened v1 consumer-validator protocol bounds profile bound to `threat`
    /// and `local`.
    pub fn v1_consumer_default(
        threat: &ThreatModelV1,
        local: &LocalStorageProfile,
    ) -> Result<Self, Bounds1Error> {
        let threat_digest = threat
            .tag()
            .map_err(|_| Bounds1Error::InvalidProfile("invalid ThreatModelV1"))?
            .digest();
        let profile = Self {
            schema_version: 1,
            threat_model_digest: threat_digest,
            local_bounds_digest: local.tag().digest(),
            max_block_bytes: DEFAULT_MAX_BLOCK_BYTES,
            max_tx_bytes: DEFAULT_MAX_TX_BYTES,
            max_block_txs: DEFAULT_MAX_BLOCK_TXS,
            max_block_signatures: DEFAULT_MAX_BLOCK_SIGNATURES,
            max_block_ops: DEFAULT_MAX_BLOCK_OPS,
            max_state_writes_per_block: DEFAULT_MAX_STATE_WRITES_PER_BLOCK,
            max_proof_bytes: DEFAULT_MAX_PROOF_BYTES,
            max_proof_depth: DEFAULT_MAX_PROOF_DEPTH,
            max_hashes_per_block: DEFAULT_MAX_HASHES_PER_BLOCK,
            max_alloc_bytes_per_block: DEFAULT_MAX_ALLOC_BYTES_PER_BLOCK,
            max_verify_budget_ms: DEFAULT_MAX_VERIFY_BUDGET_MS,
            max_peer_inflight_bytes: DEFAULT_MAX_PEER_INFLIGHT_BYTES,
            max_peer_requests_per_sec: DEFAULT_MAX_PEER_REQUESTS_PER_SEC,
        };
        profile.validate(Some(local), Some(threat))?;
        Ok(profile)
    }

    /// Validates internal positivity, `LOCAL-BOUNDS-1` containment, and `THREAT-1` slot budget.
    pub fn validate(
        &self,
        local: Option<&LocalStorageProfile>,
        threat: Option<&ThreatModelV1>,
    ) -> Result<(), Bounds1Error> {
        if self.schema_version != 1 {
            return Err(Bounds1Error::InvalidProfile("schema_version must be 1"));
        }
        if self.threat_model_digest == [0u8; 32] || self.local_bounds_digest == [0u8; 32] {
            return Err(Bounds1Error::InvalidProfile(
                "threat_model_digest and local_bounds_digest must be non-zero",
            ));
        }
        let non_zero_u32 = [
            self.max_block_bytes,
            self.max_tx_bytes,
            self.max_block_txs,
            self.max_block_signatures,
            self.max_block_ops,
            self.max_state_writes_per_block,
            self.max_proof_bytes,
            self.max_hashes_per_block,
            self.max_alloc_bytes_per_block,
            self.max_verify_budget_ms,
            self.max_peer_inflight_bytes,
        ];
        if non_zero_u32.contains(&0)
            || self.max_proof_depth == 0
            || self.max_proof_depth > 256
            || self.max_peer_requests_per_sec == 0
        {
            return Err(Bounds1Error::InvalidProfile(
                "all numeric bounds must be > 0 and max_proof_depth <= 256",
            ));
        }
        if self.max_tx_bytes > self.max_block_bytes
            || self.max_block_bytes > self.max_alloc_bytes_per_block
        {
            return Err(Bounds1Error::InvalidProfile(
                "must satisfy max_tx_bytes <= max_block_bytes <= max_alloc_bytes_per_block",
            ));
        }
        if let Some(lp) = local
            && (u64::from(self.max_block_bytes) > lp.max_record_bytes
                || u64::from(self.max_alloc_bytes_per_block) > lp.max_staged_write_bytes)
        {
            return Err(Bounds1Error::InvalidProfile(
                "protocol bounds must stay within LocalStorageProfile limits",
            ));
        }
        if let Some(tm) = threat
            && self.max_verify_budget_ms.saturating_mul(2) > tm.target_block_time_ms
        {
            return Err(Bounds1Error::InvalidProfile(
                "max_verify_budget_ms must stay <= 50% of target_block_time_ms",
            ));
        }
        Ok(())
    }

    /// Returns `candidate` strictly iff every bound in `candidate` is `<=` `self` (no widening!).
    pub fn tighten_only(&self, candidate: Self) -> Result<Self, Bounds1Error> {
        candidate.validate(None, None)?;
        if candidate.threat_model_digest != self.threat_model_digest
            || candidate.local_bounds_digest != self.local_bounds_digest
        {
            return Err(Bounds1Error::WideningForbidden(
                "tighten_only cannot switch bound threat/local digests",
            ));
        }
        let widened = candidate.max_block_bytes > self.max_block_bytes
            || candidate.max_tx_bytes > self.max_tx_bytes
            || candidate.max_block_txs > self.max_block_txs
            || candidate.max_block_signatures > self.max_block_signatures
            || candidate.max_block_ops > self.max_block_ops
            || candidate.max_state_writes_per_block > self.max_state_writes_per_block
            || candidate.max_proof_bytes > self.max_proof_bytes
            || candidate.max_proof_depth > self.max_proof_depth
            || candidate.max_hashes_per_block > self.max_hashes_per_block
            || candidate.max_alloc_bytes_per_block > self.max_alloc_bytes_per_block
            || candidate.max_verify_budget_ms > self.max_verify_budget_ms
            || candidate.max_peer_inflight_bytes > self.max_peer_inflight_bytes
            || candidate.max_peer_requests_per_sec > self.max_peer_requests_per_sec;
        if widened {
            return Err(Bounds1Error::WideningForbidden(
                "committed BOUNDS-1 limits cannot be silently widened",
            ));
        }
        Ok(candidate)
    }

    /// Zero-allocation checked streaming preflight before materializing any block payload.
    pub fn preflight_block_envelope(
        &self,
        header: &BlockEnvelopePreflightHeaderV1,
    ) -> Result<(), BoundsViolationV1> {
        // Checked aggregate memory safety check (block bytes + estimated alloc must not wrap u64)
        let Some(total_mem) = header.block_bytes.checked_add(header.estimated_alloc_bytes) else {
            return Err(BoundsViolationV1 {
                code: VIOLATION_ARITHMETIC_OVERFLOW,
                observed: u64::MAX,
                limit: u64::from(self.max_alloc_bytes_per_block),
            });
        };
        let checks = [
            (
                VIOLATION_BLOCK_BYTES,
                header.block_bytes,
                u64::from(self.max_block_bytes),
            ),
            (
                VIOLATION_TX_BYTES,
                header.max_tx_bytes_observed,
                u64::from(self.max_tx_bytes),
            ),
            (
                VIOLATION_BLOCK_TXS,
                header.tx_count,
                u64::from(self.max_block_txs),
            ),
            (
                VIOLATION_BLOCK_SIGNATURES,
                header.signature_count,
                u64::from(self.max_block_signatures),
            ),
            (
                VIOLATION_BLOCK_OPS,
                header.op_count,
                u64::from(self.max_block_ops),
            ),
            (
                VIOLATION_STATE_WRITES,
                header.state_writes,
                u64::from(self.max_state_writes_per_block),
            ),
            (
                VIOLATION_PROOF_BYTES,
                header.max_proof_bytes_observed,
                u64::from(self.max_proof_bytes),
            ),
            (
                VIOLATION_PROOF_DEPTH,
                header.max_proof_depth_observed,
                u64::from(self.max_proof_depth),
            ),
            (
                VIOLATION_HASH_BUDGET,
                header.estimated_hashes,
                u64::from(self.max_hashes_per_block),
            ),
            (
                VIOLATION_ALLOC_BUDGET,
                total_mem,
                u64::from(self.max_alloc_bytes_per_block),
            ),
        ];
        for (code, observed, limit) in checks {
            if observed > limit {
                return Err(BoundsViolationV1 {
                    code,
                    observed,
                    limit,
                });
            }
        }
        Ok(())
    }

    /// Enforces committed bounds on a consensus-accepted artifact: any violation triggers
    /// deterministic safe-halt with canonical [`SafeHaltEvidenceV1`] instead of raising limits.
    pub fn enforce_consensus_accepted_bounds(
        &self,
        artifact_digest: [u8; 32],
        header: &BlockEnvelopePreflightHeaderV1,
    ) -> Result<(), SafeHaltEvidenceV1> {
        match self.preflight_block_envelope(header) {
            Ok(()) => Ok(()),
            Err(v) => Err(SafeHaltEvidenceV1 {
                schema_version: 1,
                bounds_profile_digest: self.tag().expect("valid profile").digest(),
                artifact_digest,
                violation_code: v.code,
                observed_value: v.observed,
                committed_limit: v.limit,
            }),
        }
    }

    /// Checks per-peer DoS inflight-byte and request-rate budgets before servicing a remote peer.
    pub fn check_peer_budget(
        &self,
        inflight_bytes: u64,
        requests_per_sec: u64,
    ) -> Result<(), BoundsViolationV1> {
        if inflight_bytes > u64::from(self.max_peer_inflight_bytes) {
            return Err(BoundsViolationV1 {
                code: VIOLATION_PEER_DOS,
                observed: inflight_bytes,
                limit: u64::from(self.max_peer_inflight_bytes),
            });
        }
        if requests_per_sec > u64::from(self.max_peer_requests_per_sec) {
            return Err(BoundsViolationV1 {
                code: VIOLATION_PEER_DOS,
                observed: requests_per_sec,
                limit: u64::from(self.max_peer_requests_per_sec),
            });
        }
        Ok(())
    }

    /// Projects worst-case sustained annual committed state-write storage growth in bytes assuming
    /// `bytes_per_state_write` persisted bytes per write at `target_block_time_ms` block cadence.
    pub fn projected_worst_case_annual_state_bytes(
        &self,
        target_block_time_ms: u32,
        bytes_per_state_write: u64,
    ) -> Option<u64> {
        if target_block_time_ms == 0 {
            return None;
        }
        let blocks_per_year = (365u64 * 24 * 3600 * 1000) / u64::from(target_block_time_ms);
        blocks_per_year
            .checked_mul(u64::from(self.max_state_writes_per_block))?
            .checked_mul(bytes_per_state_write)
    }

    /// Encodes the profile into 114 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], Bounds1Error> {
        self.validate(None, None)?;
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.schema_version.to_be_bytes());
        out[2..34].copy_from_slice(&self.threat_model_digest);
        out[34..66].copy_from_slice(&self.local_bounds_digest);
        out[66..70].copy_from_slice(&self.max_block_bytes.to_be_bytes());
        out[70..74].copy_from_slice(&self.max_tx_bytes.to_be_bytes());
        out[74..78].copy_from_slice(&self.max_block_txs.to_be_bytes());
        out[78..82].copy_from_slice(&self.max_block_signatures.to_be_bytes());
        out[82..86].copy_from_slice(&self.max_block_ops.to_be_bytes());
        out[86..90].copy_from_slice(&self.max_state_writes_per_block.to_be_bytes());
        out[90..94].copy_from_slice(&self.max_proof_bytes.to_be_bytes());
        out[94..96].copy_from_slice(&self.max_proof_depth.to_be_bytes());
        out[96..100].copy_from_slice(&self.max_hashes_per_block.to_be_bytes());
        out[100..104].copy_from_slice(&self.max_alloc_bytes_per_block.to_be_bytes());
        out[104..108].copy_from_slice(&self.max_verify_budget_ms.to_be_bytes());
        out[108..112].copy_from_slice(&self.max_peer_inflight_bytes.to_be_bytes());
        out[112..114].copy_from_slice(&self.max_peer_requests_per_sec.to_be_bytes());
        Ok(out)
    }

    /// Decodes and validates a 114-byte canonical `ProtocolBoundsProfileV1`.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Bounds1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(Bounds1Error::InvalidWire(
                "wrong ProtocolBoundsProfileV1 wire length",
            ));
        }
        let r32 = |off: usize| -> [u8; 32] { bytes[off..off + 32].try_into().expect("32") };
        let ru32 =
            |off: usize| -> u32 { u32::from_be_bytes(bytes[off..off + 4].try_into().expect("4")) };
        let profile = Self {
            schema_version: u16::from_be_bytes([bytes[0], bytes[1]]),
            threat_model_digest: r32(2),
            local_bounds_digest: r32(34),
            max_block_bytes: ru32(66),
            max_tx_bytes: ru32(70),
            max_block_txs: ru32(74),
            max_block_signatures: ru32(78),
            max_block_ops: ru32(82),
            max_state_writes_per_block: ru32(86),
            max_proof_bytes: ru32(90),
            max_proof_depth: u16::from_be_bytes([bytes[94], bytes[95]]),
            max_hashes_per_block: ru32(96),
            max_alloc_bytes_per_block: ru32(100),
            max_verify_budget_ms: ru32(104),
            max_peer_inflight_bytes: ru32(108),
            max_peer_requests_per_sec: u16::from_be_bytes([bytes[112], bytes[113]]),
        };
        profile.validate(None, None)?;
        Ok(profile)
    }

    /// Computes the canonical `Policy` `ContentTag` for this protocol bounds profile.
    pub fn tag(&self) -> Result<ContentTag, Bounds1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            BOUNDS_1_PROTOCOL_PROFILE_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

/// Separate local-VM bounds sub-profile satisfying `CE-05` (`BOUNDS-1` -> `VM-D1`) and unlocking
/// `VM-1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LocalVmBoundsV1 {
    /// Schema version (`1`).
    pub schema_version: u16,
    /// Digest of the governing [`VmSpecD1`].
    pub vm_spec_digest: [u8; 32],
    /// Maximum instructions per spatial program (`<= MAX_SPATIAL_INSTRUCTIONS`).
    pub max_instructions_per_program: u16,
    /// Maximum scheduled spatial programs per local VM tick.
    pub max_programs_per_tick: u16,
    /// Maximum gas budget per individual program invocation.
    pub max_gas_per_program: u64,
    /// Maximum aggregate gas budget across one local VM tick.
    pub max_gas_per_tick: u64,
    /// Maximum integer stack depth (`<= VmSpecD1::max_stack_depth`).
    pub max_stack_depth: u16,
    /// Maximum emitted explicit `On`/`Off` output voxel writes per tick.
    pub max_output_writes_per_tick: u16,
}

impl LocalVmBoundsV1 {
    /// Fixed canonical big-endian wire length (`58` bytes).
    pub const WIRE_LEN: usize = 58;

    /// Constructs the default v1 local-VM bounds profile bound to `spec`.
    pub fn v1_default(spec: &VmSpecD1) -> Result<Self, Bounds1Error> {
        let spec_digest = spec
            .tag()
            .map_err(|_| Bounds1Error::InvalidProfile("invalid VmSpecD1"))?
            .digest();
        let bounds = Self {
            schema_version: 1,
            vm_spec_digest: spec_digest,
            max_instructions_per_program: MAX_SPATIAL_INSTRUCTIONS as u16,
            max_programs_per_tick: 64,
            max_gas_per_program: 2_048,
            max_gas_per_tick: 65_536,
            max_stack_depth: spec.max_stack_depth,
            max_output_writes_per_tick: 1_024,
        };
        bounds.validate(Some(spec))?;
        Ok(bounds)
    }

    /// Validates all local-VM bounds invariants against `spec`.
    pub fn validate(&self, spec: Option<&VmSpecD1>) -> Result<(), Bounds1Error> {
        if self.schema_version != 1 || self.vm_spec_digest == [0u8; 32] {
            return Err(Bounds1Error::InvalidProfile(
                "invalid LocalVmBoundsV1 version or zero vm_spec_digest",
            ));
        }
        if self.max_instructions_per_program == 0
            || usize::from(self.max_instructions_per_program) > MAX_SPATIAL_INSTRUCTIONS
            || self.max_programs_per_tick == 0
            || self.max_gas_per_program == 0
            || self.max_gas_per_tick < self.max_gas_per_program
            || self.max_stack_depth == 0
            || self.max_output_writes_per_tick == 0
        {
            return Err(Bounds1Error::InvalidProfile(
                "invalid LocalVmBoundsV1 numeric ceiling",
            ));
        }
        if let Some(s) = spec
            && self.max_stack_depth > s.max_stack_depth
        {
            return Err(Bounds1Error::InvalidProfile(
                "LocalVmBoundsV1 max_stack_depth cannot exceed VmSpecD1",
            ));
        }
        Ok(())
    }

    /// Preflights a batch of parsed `LANGUAGE-R1` programs and output bit widths against `spec`
    /// and `self` before executing or allocating output writes.
    pub fn preflight_vm_batch(
        &self,
        spec: &VmSpecD1,
        programs: &[ParsedSpatialProgramV1],
        output_widths: &[u8],
    ) -> Result<u64, BoundsViolationV1> {
        if programs.len() > usize::from(self.max_programs_per_tick) {
            return Err(BoundsViolationV1 {
                code: VIOLATION_LOCAL_VM,
                observed: programs.len() as u64,
                limit: u64::from(self.max_programs_per_tick),
            });
        }
        let mut total_gas = 0u64;
        let mut total_writes = 0u64;
        for (prog, &width) in programs.iter().zip(output_widths.iter()) {
            if prog.instructions.len() > usize::from(self.max_instructions_per_program) {
                return Err(BoundsViolationV1 {
                    code: VIOLATION_LOCAL_VM,
                    observed: prog.instructions.len() as u64,
                    limit: u64::from(self.max_instructions_per_program),
                });
            }
            let Ok(upper_gas) = spec.pre_execution_work_upper_bound(prog, width) else {
                return Err(BoundsViolationV1 {
                    code: VIOLATION_LOCAL_VM,
                    observed: u64::from(width),
                    limit: u64::from(spec.max_output_bits),
                });
            };
            if upper_gas > self.max_gas_per_program {
                return Err(BoundsViolationV1 {
                    code: VIOLATION_LOCAL_VM,
                    observed: upper_gas,
                    limit: self.max_gas_per_program,
                });
            }
            total_gas = total_gas.saturating_add(upper_gas);
            total_writes = total_writes.saturating_add(u64::from(width));
        }
        if total_gas > self.max_gas_per_tick {
            return Err(BoundsViolationV1 {
                code: VIOLATION_LOCAL_VM,
                observed: total_gas,
                limit: self.max_gas_per_tick,
            });
        }
        if total_writes > u64::from(self.max_output_writes_per_tick) {
            return Err(BoundsViolationV1 {
                code: VIOLATION_LOCAL_VM,
                observed: total_writes,
                limit: u64::from(self.max_output_writes_per_tick),
            });
        }
        Ok(total_gas)
    }

    /// Encodes `LocalVmBoundsV1` into 58 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], Bounds1Error> {
        self.validate(None)?;
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.schema_version.to_be_bytes());
        out[2..34].copy_from_slice(&self.vm_spec_digest);
        out[34..36].copy_from_slice(&self.max_instructions_per_program.to_be_bytes());
        out[36..38].copy_from_slice(&self.max_programs_per_tick.to_be_bytes());
        out[38..46].copy_from_slice(&self.max_gas_per_program.to_be_bytes());
        out[46..54].copy_from_slice(&self.max_gas_per_tick.to_be_bytes());
        out[54..56].copy_from_slice(&self.max_stack_depth.to_be_bytes());
        out[56..58].copy_from_slice(&self.max_output_writes_per_tick.to_be_bytes());
        Ok(out)
    }

    /// Decodes and validates a 58-byte canonical `LocalVmBoundsV1`.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Bounds1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(Bounds1Error::InvalidWire(
                "wrong LocalVmBoundsV1 wire length",
            ));
        }
        let mut digest = [0u8; 32];
        digest.copy_from_slice(&bytes[2..34]);
        let bounds = Self {
            schema_version: u16::from_be_bytes([bytes[0], bytes[1]]),
            vm_spec_digest: digest,
            max_instructions_per_program: u16::from_be_bytes([bytes[34], bytes[35]]),
            max_programs_per_tick: u16::from_be_bytes([bytes[36], bytes[37]]),
            max_gas_per_program: u64::from_be_bytes(bytes[38..46].try_into().expect("8")),
            max_gas_per_tick: u64::from_be_bytes(bytes[46..54].try_into().expect("8")),
            max_stack_depth: u16::from_be_bytes([bytes[54], bytes[55]]),
            max_output_writes_per_tick: u16::from_be_bytes([bytes[56], bytes[57]]),
        };
        bounds.validate(None)?;
        Ok(bounds)
    }

    /// Computes the canonical `Policy` `ContentTag` for this local-VM bounds sub-profile.
    pub fn tag(&self) -> Result<ContentTag, Bounds1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            BOUNDS_1_LOCAL_VM_PROFILE_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

/// Deterministic safe-halt evidence emitted when a consensus-accepted artifact exceeds committed
/// `BOUNDS-1` limits (`bitblock/bounds-1/safe-halt-evidence/v1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SafeHaltEvidenceV1 {
    /// Schema version (`1`).
    pub schema_version: u16,
    /// Digest of the committed [`ProtocolBoundsProfileV1`].
    pub bounds_profile_digest: [u8; 32],
    /// Digest of the consensus-accepted over-bound artifact.
    pub artifact_digest: [u8; 32],
    /// Canonical violation class code (`1..=13`).
    pub violation_code: u8,
    /// Observed value that breached the committed bound.
    pub observed_value: u64,
    /// Committed bound from `bounds_profile_digest`.
    pub committed_limit: u64,
}

impl SafeHaltEvidenceV1 {
    /// Fixed canonical big-endian wire length (`83` bytes).
    pub const WIRE_LEN: usize = 83;

    /// Encodes the safe-halt evidence into 83 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> [u8; Self::WIRE_LEN] {
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.schema_version.to_be_bytes());
        out[2..34].copy_from_slice(&self.bounds_profile_digest);
        out[34..66].copy_from_slice(&self.artifact_digest);
        out[66] = self.violation_code;
        out[67..75].copy_from_slice(&self.observed_value.to_be_bytes());
        out[75..83].copy_from_slice(&self.committed_limit.to_be_bytes());
        out
    }

    /// Computes the canonical `Receipt` `ContentTag` for this safe-halt evidence record.
    pub fn tag(&self) -> ContentTag {
        let bytes = self.encode_canonical();
        hash_canonical(
            TagKind::Receipt,
            BOUNDS_1_SAFE_HALT_EVIDENCE_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_1_profiles_preflight_tighten_and_safe_halt() {
        let threat = ThreatModelV1 {
            schema_version: 1,
            profile_scope_digest: [9u8; 32],
            max_corrupt_weight_bps: 3_333,
            target_block_time_ms: 2_000,
            gst_max_delay_ms: 10_000,
            adaptive_corruption_delay_epochs: 1,
            unbonding_delay_epochs: 21,
            rolling_retention_epochs: 28,
            min_honest_peers: 8,
            release_builder_threshold_m: 3,
            release_builder_total_n: 5,
            sequencer_censorship_guaranteed: false,
            consensus_max_inclusion_window_factor: 3,
            suite_classical_security_bits: 128,
            external_bridges_admitted: false,
        };
        let profile =
            ProtocolBoundsProfileV1::v1_consumer_default(&threat, &LocalStorageProfile::DEFAULT)
                .unwrap();
        let raw = profile.encode_canonical().unwrap();
        assert_eq!(
            ProtocolBoundsProfileV1::decode_canonical(&raw).unwrap(),
            profile
        );

        let vm_spec = VmSpecD1::v1_default();
        let vm_bounds = LocalVmBoundsV1::v1_default(&vm_spec).unwrap();
        let raw_vm = vm_bounds.encode_canonical().unwrap();
        assert_eq!(
            LocalVmBoundsV1::decode_canonical(&raw_vm).unwrap(),
            vm_bounds
        );

        let over = BlockEnvelopePreflightHeaderV1 {
            block_bytes: u64::from(DEFAULT_MAX_BLOCK_BYTES) + 1,
            tx_count: 1,
            max_tx_bytes_observed: 256,
            signature_count: 1,
            op_count: 1,
            state_writes: 1,
            max_proof_bytes_observed: 256,
            max_proof_depth_observed: 32,
            estimated_hashes: 100,
            estimated_alloc_bytes: 4096,
        };
        let halt = profile
            .enforce_consensus_accepted_bounds([0xAB; 32], &over)
            .unwrap_err();
        assert_eq!(halt.violation_code, VIOLATION_BLOCK_BYTES);
        assert_eq!(halt.observed_value, u64::from(DEFAULT_MAX_BLOCK_BYTES) + 1);
    }
}
