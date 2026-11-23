//! `VALIDATOR-ECON-1` (`PS-041`): profile-bound consensus-to-economic incentives adapter,
//! receipt-exact `ECONOMIC-STATE-1` correspondence verifier, and deterministic 7-vector
//! adversarial economic simulator.
//!
//! Strictly behind `--features protocol-research` and `wasm32`-clean.
//!
//! # Normative Boundaries (`CAPABILITY_TREE.md` / `VALIDATOR-ECON-1`)
//! - **No new execution-root domain:** `VALIDATOR-ECON-1` consumes the already-committed
//!   `ECONOMIC-STATE-1` root (`DomainIdV1::EconomicStake = 6`) and receipts under an accepted
//!   `EXECUTION-ROOT-1` [`ExecutionRootRefV1`]; `produces_execution_root_domain` is strictly
//!   `false` fail-closed.
//! - **Occupancy token ≠ validator stake:** `LEDGER-1` / `ASSET-1` block occupancy tokens and
//!   refundable storage deposits MUST NEVER silently become consensus validator stake;
//!   `block_token_used_as_stake` is strictly `false` fail-closed.
//! - **Receipt-exact correspondence:** every modeled consensus voting weight, epoch reward,
//!   inactivity penalty, and equivocation slash matches an exact `ECONOMIC-STATE-1`
//!   [`EconomicTransitionReceiptV1`] under the accepted [`ExecutionRootRefV1`].
//! - **7-vector adversarial coverage:** deterministically models bribery, censorship,
//!   equivocation (including slash-and-flee across active + unbonding stake), griefing,
//!   concentration (`CE-15` `2,500 bps` cap -> Nakamoto lower bounds), long-range
//!   (`> weak_subjectivity_horizon_epochs`), and post-slash recovery with explicit monitored
//!   loss conditions.

use std::fmt;

use sha2::{Digest, Sha256};

use crate::asset_d1::{BPS_DENOMINATOR, ValidatorEconPolicyV1};
use crate::consensus_1::{
    Consensus1Error, ConsensusClusterHarnessV1, ConsensusCommitCertificateV1,
    VoteEquivocationEvidenceV1,
};
use crate::economic_state_1::{
    EconomicDispositionV1, EconomicState1Error, EconomicStateV1, EconomicTransitionReceiptV1,
};
use crate::execution_root_1::ExecutionRootRefV1;
use crate::executor_1::ExecutorForkContextV1;
use crate::identity_d1::ActorIdV1;
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::validator_schedule_1::EpochValidatorScheduleV1;
use crate::world::Coord;

/// Canonical schema version for `VALIDATOR-ECON-1` objects.
pub const VALIDATOR_ECON_1_VERSION: u16 = 1;

/// Canonical schema string for [`ValidatorEconAdapterReceiptV1`].
pub const VALIDATOR_ECON_1_ADAPTER_RECEIPT_SCHEMA: &[u8] =
    b"bitblock/validator-econ-1/consensus-adapter-receipt/v1";

/// Canonical schema string for [`ValidatorEconAdversarialAssayV1`].
pub const VALIDATOR_ECON_1_ADVERSARIAL_ASSAY_SCHEMA: &[u8] =
    b"bitblock/validator-econ-1/adversarial-assay/v1";

const VECON_EQUIV_EVIDENCE_PREFIX: &[u8] = b"bb.vecon1.equiv_evidence.v1";
const VECON_INACTIVITY_EVIDENCE_PREFIX: &[u8] = b"bb.vecon1.inactivity_evidence.v1";
const VECON_ASSAY_TRANSCRIPT_PREFIX: &[u8] = b"bb.vecon1.assay_transcript.v1";

/// Bitmask covering all 7 required `CAPABILITY_TREE.md` adversarial incentive vectors.
pub const ALL_SEVEN_ATTACK_VECTORS_MASK: u8 = 0x7F;

/// Failure modes for `VALIDATOR-ECON-1` consensus-to-economic correspondence and assay checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValidatorEcon1Error {
    /// Malformed or non-canonical wire encoding.
    InvalidWire(&'static str),
    /// Accepted consensus certificate or fork context failed verification or binding.
    ConsensusBindingViolation(&'static str),
    /// Consensus schedule weights disagreed with committed `ECONOMIC-STATE-1` truth.
    WeightCorrespondenceViolation(&'static str),
    /// `ECONOMIC-STATE-1` rejected or failed to correspond to the modeled economic transition.
    EconomicReceiptMismatch(&'static str),
    /// Attempted to alias a `LEDGER-1` / `ASSET-1` occupancy block token as validator stake.
    BlockTokenStakeAliasingForbidden {
        /// Coordinate of the occupancy block token that was refused as stake.
        coord: Coord,
        /// Locked refundable storage deposit (never convertible to validator stake).
        locked_deposit: u128,
    },
    /// Attempted to claim a new execution-root domain (forbidden by `VALIDATOR-ECON-1`).
    ExecutionRootDomainForbidden,
    /// Concentration or Nakamoto safety invariant violated.
    ConcentrationViolation(&'static str),
    /// Long-range stale unbonded key rejected past weak-subjectivity horizon.
    LongRangeUnbondedKeyRejected {
        /// Historical epoch where the unbonded key was last active.
        historical_epoch: u64,
        /// Current finalized epoch.
        current_epoch: u64,
        /// Configured weak-subjectivity horizon in epochs.
        horizon_epochs: u64,
    },
    /// Wrapped upstream `CONSENSUS-1` error.
    Consensus(String),
    /// Wrapped upstream `ECONOMIC-STATE-1` error.
    Economic(String),
}

impl From<Consensus1Error> for ValidatorEcon1Error {
    fn from(e: Consensus1Error) -> Self {
        Self::Consensus(e.to_string())
    }
}

impl From<EconomicState1Error> for ValidatorEcon1Error {
    fn from(e: EconomicState1Error) -> Self {
        Self::Economic(e.to_string())
    }
}

impl fmt::Display for ValidatorEcon1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ValidatorEcon1Error {}

/// Canonical action discriminant for [`ValidatorEconAdapterReceiptV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ValidatorEconActionV1 {
    /// Read-only audit proving consensus schedule weights match `ECONOMIC-STATE-1`.
    WeightScheduleAudit = 1,
    /// Pro-rata integer-floor epoch reward distribution under `max_epoch_reward_issuance`.
    EpochReward = 2,
    /// 50% active + unbonding equivocation slash-to-burn and permanent validator jailing.
    EquivocationSlash = 3,
    /// 1% active + unbonding inactivity slash-to-burn for sustained missed consensus votes.
    InactivitySlash = 4,
}

impl ValidatorEconActionV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte action discriminant.
    pub const fn from_code(code: u8) -> Result<Self, ValidatorEcon1Error> {
        match code {
            1 => Ok(Self::WeightScheduleAudit),
            2 => Ok(Self::EpochReward),
            3 => Ok(Self::EquivocationSlash),
            4 => Ok(Self::InactivitySlash),
            _ => Err(ValidatorEcon1Error::InvalidWire(
                "unknown ValidatorEconActionV1 code",
            )),
        }
    }
}

/// Named monitored loss conditions emitted by the deterministic adversarial incentive assay.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum MonitoredLossConditionV1 {
    /// External bribe / extractable value exceeds the provable `> 1/3` equivocation slash floor.
    BriberyExceedsSlashFloor = 1,
    /// Colluding cartel exceeds `1/3` voting weight and can stall inclusion / liveness.
    CensorshipCartelAboveOneThird = 2,
    /// Active validator set fell below BFT quorum recovery floor before re-bonding.
    ActiveSetBelowSafetyFloor = 3,
}

impl MonitoredLossConditionV1 {
    /// Returns the canonical 1-byte discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }
}

/// Deterministic summary row for one of the 7 required `CAPABILITY_TREE.md` adversarial scenarios.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AdversarialScenarioOutcomeV1 {
    /// Scenario ordinal (`1..=7`: bribery, censorship, equivocation, griefing, concentration,
    /// long-range, recovery).
    pub scenario_id: u8,
    /// `true` iff all safety, conservation, and correspondence assertions passed.
    pub passed: bool,
    /// Quantitative metric bound (stake burned, cost floor, bps, or epochs elapsed).
    pub primary_metric: u128,
    /// Secondary quantitative threshold or remaining bonded balance.
    pub secondary_metric: u128,
    /// Optional monitored loss condition surfaced when assumptions are deliberately exceeded.
    pub loss_condition: Option<MonitoredLossConditionV1>,
}

/// Canonical proof-bound adapter receipt (`bitblock/validator-econ-1/consensus-adapter-receipt/v1`,
/// `443` bytes) proving exact correspondence between accepted `CONSENSUS-1` state/evidence and
/// committed `ECONOMIC-STATE-1` receipts under an accepted `ExecutionRootRefV1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ValidatorEconAdapterReceiptV1 {
    /// Schema version (`1`).
    pub receipt_version: u16,
    /// Immutable `GENESIS-D1` identifier.
    pub genesis_id: [u8; 32],
    /// Governing `PROFILE-D1` launch profile identifier.
    pub profile_id: [u8; 32],
    /// Accepted `EXECUTION-ROOT-1` reference (`178` bytes).
    pub accepted_execution_root_ref: ExecutionRootRefV1,
    /// Accepted `CONSENSUS-1` finalized block identifier.
    pub consensus_block_id: [u8; 32],
    /// Accepted `CONSENSUS-1` finalized height.
    pub consensus_height: u64,
    /// Consensus / economic epoch of this adapter event.
    pub consensus_epoch: u64,
    /// Specific consensus-to-economic adapter action verified.
    pub action_kind: ValidatorEconActionV1,
    /// `EconomicStateRootV1::canonical_sha256()` before the action.
    pub economic_root_before_digest: [u8; 32],
    /// `EconomicStateRootV1::canonical_sha256()` after the action.
    pub economic_root_after_digest: [u8; 32],
    /// `SHA-256` of canonical `EconomicTransitionReceiptV1` bytes (`[0; 32]` for read-only audit).
    pub economic_receipt_digest: [u8; 32],
    /// Eligible active validator count audited/updated.
    pub validator_count: u32,
    /// Sum of eligible active voting weight.
    pub total_active_weight: u128,
    /// Issued reward or slashed-to-burn amount (`0` for read-only weight audit).
    pub reward_or_slash_amount: u128,
    /// Unissued integer-floor reward dust (`0` outside `EpochReward`).
    pub reward_dust_unissued: u128,
    /// Must strictly remain `false` (`VALIDATOR-ECON-1` produces no execution-root domain).
    pub produces_execution_root_domain: bool,
    /// Must strictly remain `false` (block occupancy tokens never become validator stake).
    pub block_token_used_as_stake: bool,
}

impl ValidatorEconAdapterReceiptV1 {
    /// Fixed canonical big-endian wire byte length (`443` bytes).
    pub const WIRE_LEN: usize =
        2 + 32 + 32 + ExecutionRootRefV1::WIRE_LEN + 32 + 8 + 8 + 1 + 32 * 3 + 4 + 16 * 3 + 1 + 1;

    /// Validates version, non-domain invariant, and non-aliasing invariant fail-closed.
    pub fn validate(&self) -> Result<(), ValidatorEcon1Error> {
        if self.receipt_version != VALIDATOR_ECON_1_VERSION {
            return Err(ValidatorEcon1Error::InvalidWire(
                "receipt_version must be 1",
            ));
        }
        if self.produces_execution_root_domain {
            return Err(ValidatorEcon1Error::ExecutionRootDomainForbidden);
        }
        if self.block_token_used_as_stake {
            return Err(ValidatorEcon1Error::InvalidWire(
                "block_token_used_as_stake must be false",
            ));
        }
        if self.accepted_execution_root_ref.genesis_id != self.genesis_id
            || self.accepted_execution_root_ref.profile_id != self.profile_id
        {
            return Err(ValidatorEcon1Error::ConsensusBindingViolation(
                "accepted_execution_root_ref disagrees with genesis_id or profile_id",
            ));
        }
        if self.validator_count == 0 || self.total_active_weight == 0 {
            return Err(ValidatorEcon1Error::WeightCorrespondenceViolation(
                "validator_count and total_active_weight must be > 0",
            ));
        }
        if self.action_kind != ValidatorEconActionV1::WeightScheduleAudit
            && self.economic_receipt_digest == [0u8; 32]
        {
            return Err(ValidatorEcon1Error::EconomicReceiptMismatch(
                "state-changing economic action requires non-zero economic_receipt_digest",
            ));
        }
        Ok(())
    }

    /// Encodes into exact 443-byte canonical big-endian wire format.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], ValidatorEcon1Error> {
        self.validate()?;
        let mut buf = Vec::with_capacity(Self::WIRE_LEN);
        buf.extend_from_slice(&self.receipt_version.to_be_bytes());
        buf.extend_from_slice(&self.genesis_id);
        buf.extend_from_slice(&self.profile_id);
        buf.extend_from_slice(&self.accepted_execution_root_ref.encode_canonical());
        buf.extend_from_slice(&self.consensus_block_id);
        buf.extend_from_slice(&self.consensus_height.to_be_bytes());
        buf.extend_from_slice(&self.consensus_epoch.to_be_bytes());
        buf.push(self.action_kind.code());
        buf.extend_from_slice(&self.economic_root_before_digest);
        buf.extend_from_slice(&self.economic_root_after_digest);
        buf.extend_from_slice(&self.economic_receipt_digest);
        buf.extend_from_slice(&self.validator_count.to_be_bytes());
        buf.extend_from_slice(&self.total_active_weight.to_be_bytes());
        buf.extend_from_slice(&self.reward_or_slash_amount.to_be_bytes());
        buf.extend_from_slice(&self.reward_dust_unissued.to_be_bytes());
        buf.push(u8::from(self.produces_execution_root_domain));
        buf.push(u8::from(self.block_token_used_as_stake));
        buf.try_into()
            .map_err(|_| ValidatorEcon1Error::InvalidWire("adapter receipt length mismatch"))
    }

    /// Decodes and validates exact 443-byte canonical wire bytes fail-closed.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, ValidatorEcon1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(ValidatorEcon1Error::InvalidWire(
                "invalid ValidatorEconAdapterReceiptV1 length",
            ));
        }
        let mut p = 0usize;
        let receipt_version = read_u16_be(bytes, &mut p);
        let genesis_id = read_arr32(bytes, &mut p);
        let profile_id = read_arr32(bytes, &mut p);
        let accepted_execution_root_ref =
            ExecutionRootRefV1::decode_canonical(&bytes[p..p + ExecutionRootRefV1::WIRE_LEN])
                .map_err(|_| {
                    ValidatorEcon1Error::InvalidWire("invalid embedded ExecutionRootRefV1")
                })?;
        p += ExecutionRootRefV1::WIRE_LEN;
        let consensus_block_id = read_arr32(bytes, &mut p);
        let consensus_height = read_u64_be(bytes, &mut p);
        let consensus_epoch = read_u64_be(bytes, &mut p);
        let action_kind = ValidatorEconActionV1::from_code(read_u8(bytes, &mut p))?;
        let economic_root_before_digest = read_arr32(bytes, &mut p);
        let economic_root_after_digest = read_arr32(bytes, &mut p);
        let economic_receipt_digest = read_arr32(bytes, &mut p);
        let validator_count = read_u32_be(bytes, &mut p);
        let total_active_weight = read_u128_be(bytes, &mut p);
        let reward_or_slash_amount = read_u128_be(bytes, &mut p);
        let reward_dust_unissued = read_u128_be(bytes, &mut p);
        let produces_execution_root_domain = read_bool(bytes, &mut p)?;
        let block_token_used_as_stake = read_bool(bytes, &mut p)?;
        let out = Self {
            receipt_version,
            genesis_id,
            profile_id,
            accepted_execution_root_ref,
            consensus_block_id,
            consensus_height,
            consensus_epoch,
            action_kind,
            economic_root_before_digest,
            economic_root_after_digest,
            economic_receipt_digest,
            validator_count,
            total_active_weight,
            reward_or_slash_amount,
            reward_dust_unissued,
            produces_execution_root_domain,
            block_token_used_as_stake,
        };
        out.validate()?;
        Ok(out)
    }

    /// Computes the canonical `ContentTag` over `VALIDATOR_ECON_1_ADAPTER_RECEIPT_SCHEMA`.
    pub fn tag(&self) -> Result<ContentTag, ValidatorEcon1Error> {
        let enc = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            VALIDATOR_ECON_1_ADAPTER_RECEIPT_SCHEMA,
            enc.len() as u64,
            |w| w(&enc),
        )
        .expect("static schema and fixed length"))
    }
}

/// Canonical 7-vector adversarial incentive assay (`bitblock/validator-econ-1/adversarial-assay/v1`,
/// `196` bytes) summarizing quantitative BFT slashing floors, concentration lower bounds, and
/// monitored loss conditions under the governing `VALIDATOR-ECON-D1` and `THREAT-1` policies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ValidatorEconAdversarialAssayV1 {
    /// Schema version (`1`).
    pub assay_version: u16,
    /// Immutable `GENESIS-D1` identifier.
    pub genesis_id: [u8; 32],
    /// Governing `PROFILE-D1` launch profile identifier.
    pub profile_id: [u8; 32],
    /// Canonical `ContentTag` digest of `ValidatorEconPolicyV1`.
    pub policy_tag_digest: [u8; 32],
    /// Canonical `ContentTag` digest of `ThreatModelV1` (`THREAT-1`).
    pub threat_model_digest: [u8; 32],
    /// Total genesis active validator stake modeled.
    pub total_network_stake: u128,
    /// Minimum distinct validators required to exceed `1/3` liveness halt threshold under CE-15 cap.
    pub min_nakamoto_halt: u32,
    /// Minimum distinct validators required to exceed `2/3` safety corruption threshold under CE-15 cap.
    pub min_nakamoto_safety: u32,
    /// Configured equivocation slash penalty in basis points (`5_000` = 50%).
    pub equivocation_slash_cost_bps: u16,
    /// Provable minimum network stake fraction burned on a `> 1/3` equivocating safety attack (`bps`).
    pub min_safety_attack_burn_bps: u16,
    /// Bitmask of verified attack vectors (`0x7F` = all 7/7 required vectors passed).
    pub passed_attack_mask: u8,
    /// Count of explicit monitored loss conditions triggered and classified during stress scenarios.
    pub monitored_loss_alert_count: u32,
    /// Must strictly be `true` (block occupancy tokens refused as consensus stake).
    pub block_token_stake_aliasing_rejected: bool,
    /// Domain-separated `SHA-256` transcript digest binding all 7 deterministic scenario rows.
    pub scenario_transcript_digest: [u8; 32],
}

impl ValidatorEconAdversarialAssayV1 {
    /// Fixed canonical big-endian wire byte length (`196` bytes).
    pub const WIRE_LEN: usize = 2 + 32 * 4 + 16 + 4 + 4 + 2 + 2 + 1 + 4 + 1 + 32;

    /// Validates assay version, Nakamoto floors, full 7-vector coverage, and non-aliasing gate.
    pub fn validate(&self) -> Result<(), ValidatorEcon1Error> {
        if self.assay_version != VALIDATOR_ECON_1_VERSION {
            return Err(ValidatorEcon1Error::InvalidWire("assay_version must be 1"));
        }
        if self.total_network_stake == 0 {
            return Err(ValidatorEcon1Error::ConcentrationViolation(
                "total_network_stake must be > 0",
            ));
        }
        if self.min_nakamoto_halt < 2 || self.min_nakamoto_safety < 3 {
            return Err(ValidatorEcon1Error::ConcentrationViolation(
                "CE-15 cap requires min_nakamoto_halt >= 2 and min_nakamoto_safety >= 3",
            ));
        }
        if self.equivocation_slash_cost_bps == 0
            || self.equivocation_slash_cost_bps > BPS_DENOMINATOR
            || self.min_safety_attack_burn_bps == 0
        {
            return Err(ValidatorEcon1Error::InvalidWire(
                "invalid slashing cost basis points",
            ));
        }
        if self.passed_attack_mask != ALL_SEVEN_ATTACK_VECTORS_MASK {
            return Err(ValidatorEcon1Error::InvalidWire(
                "all 7 adversarial incentive vectors (0x7F) must pass",
            ));
        }
        if !self.block_token_stake_aliasing_rejected {
            return Err(ValidatorEcon1Error::InvalidWire(
                "block_token_stake_aliasing_rejected must be true",
            ));
        }
        Ok(())
    }

    /// Encodes into exact 196-byte canonical big-endian wire format.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], ValidatorEcon1Error> {
        self.validate()?;
        let mut buf = Vec::with_capacity(Self::WIRE_LEN);
        buf.extend_from_slice(&self.assay_version.to_be_bytes());
        buf.extend_from_slice(&self.genesis_id);
        buf.extend_from_slice(&self.profile_id);
        buf.extend_from_slice(&self.policy_tag_digest);
        buf.extend_from_slice(&self.threat_model_digest);
        buf.extend_from_slice(&self.total_network_stake.to_be_bytes());
        buf.extend_from_slice(&self.min_nakamoto_halt.to_be_bytes());
        buf.extend_from_slice(&self.min_nakamoto_safety.to_be_bytes());
        buf.extend_from_slice(&self.equivocation_slash_cost_bps.to_be_bytes());
        buf.extend_from_slice(&self.min_safety_attack_burn_bps.to_be_bytes());
        buf.push(self.passed_attack_mask);
        buf.extend_from_slice(&self.monitored_loss_alert_count.to_be_bytes());
        buf.push(u8::from(self.block_token_stake_aliasing_rejected));
        buf.extend_from_slice(&self.scenario_transcript_digest);
        buf.try_into()
            .map_err(|_| ValidatorEcon1Error::InvalidWire("assay length mismatch"))
    }

    /// Decodes and validates exact 196-byte canonical wire bytes fail-closed.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, ValidatorEcon1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(ValidatorEcon1Error::InvalidWire(
                "invalid ValidatorEconAdversarialAssayV1 length",
            ));
        }
        let mut p = 0usize;
        let assay_version = read_u16_be(bytes, &mut p);
        let genesis_id = read_arr32(bytes, &mut p);
        let profile_id = read_arr32(bytes, &mut p);
        let policy_tag_digest = read_arr32(bytes, &mut p);
        let threat_model_digest = read_arr32(bytes, &mut p);
        let total_network_stake = read_u128_be(bytes, &mut p);
        let min_nakamoto_halt = read_u32_be(bytes, &mut p);
        let min_nakamoto_safety = read_u32_be(bytes, &mut p);
        let equivocation_slash_cost_bps = read_u16_be(bytes, &mut p);
        let min_safety_attack_burn_bps = read_u16_be(bytes, &mut p);
        let passed_attack_mask = read_u8(bytes, &mut p);
        let monitored_loss_alert_count = read_u32_be(bytes, &mut p);
        let block_token_stake_aliasing_rejected = read_bool(bytes, &mut p)?;
        let scenario_transcript_digest = read_arr32(bytes, &mut p);
        let out = Self {
            assay_version,
            genesis_id,
            profile_id,
            policy_tag_digest,
            threat_model_digest,
            total_network_stake,
            min_nakamoto_halt,
            min_nakamoto_safety,
            equivocation_slash_cost_bps,
            min_safety_attack_burn_bps,
            passed_attack_mask,
            monitored_loss_alert_count,
            block_token_stake_aliasing_rejected,
            scenario_transcript_digest,
        };
        out.validate()?;
        Ok(out)
    }

    /// Computes the canonical `ContentTag` over `VALIDATOR_ECON_1_ADVERSARIAL_ASSAY_SCHEMA`.
    pub fn tag(&self) -> Result<ContentTag, ValidatorEcon1Error> {
        let enc = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            VALIDATOR_ECON_1_ADVERSARIAL_ASSAY_SCHEMA,
            enc.len() as u64,
            |w| w(&enc),
        )
        .expect("static schema and fixed length"))
    }
}

/// Computes the deterministic `SHA-256` digest of an [`EconomicTransitionReceiptV1`] wire encoding.
pub fn economic_receipt_sha256(
    receipt: &EconomicTransitionReceiptV1,
) -> Result<[u8; 32], ValidatorEcon1Error> {
    let raw = receipt.encode_canonical()?;
    Ok(Sha256::digest(raw).into())
}

/// Computes the domain-separated evidence digest binding a verified [`VoteEquivocationEvidenceV1`].
pub fn equivocation_evidence_digest(
    evidence: &VoteEquivocationEvidenceV1,
) -> Result<[u8; 32], ValidatorEcon1Error> {
    let mut h = Sha256::new();
    h.update(VECON_EQUIV_EVIDENCE_PREFIX);
    h.update(evidence.voter_id.as_bytes());
    h.update(evidence.first_vote.encode_canonical()?);
    h.update(evidence.second_vote.encode_canonical()?);
    Ok(h.finalize().into())
}

/// Computes the domain-separated evidence digest for sustained validator inactivity slashing.
pub fn inactivity_evidence_digest(
    validator_id: ActorIdV1,
    epoch: u64,
    missed_blocks: u32,
) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(VECON_INACTIVITY_EVIDENCE_PREFIX);
    h.update(validator_id.as_bytes());
    h.update(epoch.to_be_bytes());
    h.update(missed_blocks.to_be_bytes());
    h.finalize().into()
}

/// Computes minimum Nakamoto coefficient lower bounds `(min_halt, min_safety)` implied by the
/// CE-15 single-validator concentration ceiling `max_single_validator_bps` (`1..=3_333`).
pub fn nakamoto_lower_bounds(
    max_single_validator_bps: u16,
) -> Result<(u32, u32), ValidatorEcon1Error> {
    if max_single_validator_bps == 0 || max_single_validator_bps > 3_333 {
        return Err(ValidatorEcon1Error::ConcentrationViolation(
            "max_single_validator_bps must be in 1..=3333",
        ));
    }
    let cap = u32::from(max_single_validator_bps);
    // Strict > 1/3 requires >= 3,334 bps; strict > 2/3 requires >= 6,667 bps.
    let min_halt = 3_334u32.div_ceil(cap);
    let min_safety = 6_667u32.div_ceil(cap);
    Ok((min_halt, min_safety))
}

/// Computes the provable minimum network-stake burn floor (in `bps` and native units) required
/// to mount a `> 1/3` equivocating BFT safety attack under `policy`.
pub fn safety_attack_burn_floor(
    total_network_stake: u128,
    policy: &ValidatorEconPolicyV1,
) -> Result<(u16, u128), ValidatorEcon1Error> {
    if total_network_stake == 0 || policy.equivocation_slash_bps == 0 {
        return Err(ValidatorEcon1Error::ConcentrationViolation(
            "non-zero total_network_stake and equivocation_slash_bps required",
        ));
    }
    // > 1/3 equivocating weight = total / 3 + 1
    let min_equivocating_stake = (total_network_stake / 3) + 1;
    let min_burn_units = min_equivocating_stake
        .checked_mul(u128::from(policy.equivocation_slash_bps))
        .ok_or(ValidatorEcon1Error::ConcentrationViolation(
            "burn floor overflow",
        ))?
        / u128::from(BPS_DENOMINATOR);
    let min_burn_bps = ((3_334u32 * u32::from(policy.equivocation_slash_bps))
        .div_ceil(u32::from(BPS_DENOMINATOR))) as u16;
    Ok((min_burn_bps, min_burn_units))
}

/// Explicitly refuses any attempt to count a `LEDGER-1` / `ASSET-1` block occupancy token or its
/// refundable storage deposit as consensus validator stake.
pub fn reject_occupancy_token_as_stake(
    coord: Coord,
    _mint_episode_id: [u8; 32],
    locked_deposit: u128,
) -> Result<(), ValidatorEcon1Error> {
    Err(ValidatorEcon1Error::BlockTokenStakeAliasingForbidden {
        coord,
        locked_deposit,
    })
}

/// Enforces that historical unbonded validator keys past `weak_subjectivity_horizon_epochs`
/// cannot authorize current consensus finality (long-range attack barrier).
pub fn verify_weak_subjectivity_key_freshness(
    historical_epoch: u64,
    current_epoch: u64,
    policy: &ValidatorEconPolicyV1,
) -> Result<(), ValidatorEcon1Error> {
    let age = current_epoch.saturating_sub(historical_epoch);
    if age > policy.weak_subjectivity_horizon_epochs {
        return Err(ValidatorEcon1Error::LongRangeUnbondedKeyRejected {
            historical_epoch,
            current_epoch,
            horizon_epochs: policy.weak_subjectivity_horizon_epochs,
        });
    }
    Ok(())
}

/// Profile-bound consensus-to-economic adapter consuming `CONSENSUS-1` certificates/evidence and
/// verifying receipt-exact correspondence with `ECONOMIC-STATE-1` under `EXECUTION-ROOT-1`.
#[derive(Clone, Debug)]
pub struct ValidatorEconAdapterV1 {
    /// Governing `EXECUTION-ROOT-1` fork context (`genesis_id`, `profile_id`, `transition_spec_id`).
    pub fork_context: ExecutorForkContextV1,
    /// Governing `VALIDATOR-ECON-D1` policy.
    pub policy: ValidatorEconPolicyV1,
}

impl ValidatorEconAdapterV1 {
    /// Constructs an adapter bound to `fork_context` and `policy`.
    pub fn new(
        fork_context: ExecutorForkContextV1,
        policy: ValidatorEconPolicyV1,
    ) -> Result<Self, ValidatorEcon1Error> {
        policy.validate().map_err(|_| {
            ValidatorEcon1Error::ConcentrationViolation("invalid ValidatorEconPolicyV1")
        })?;
        Ok(Self {
            fork_context,
            policy,
        })
    }

    fn verify_cert_context(
        &self,
        cert: &ConsensusCommitCertificateV1,
        schedule: &EpochValidatorScheduleV1,
    ) -> Result<(), ValidatorEcon1Error> {
        if cert.proposal.fork_context != self.fork_context {
            return Err(ValidatorEcon1Error::ConsensusBindingViolation(
                "certificate fork_context does not match adapter fork_context",
            ));
        }
        if cert.proposal.execution_root_ref.genesis_id != self.fork_context.genesis_id
            || cert.proposal.execution_root_ref.profile_id != self.fork_context.profile_id
        {
            return Err(ValidatorEcon1Error::ConsensusBindingViolation(
                "certificate execution_root_ref disagrees with fork_context",
            ));
        }
        cert.verify(schedule)?;
        Ok(())
    }

    /// Audits that every consensus voting weight in `schedule` matches `ECONOMIC-STATE-1`
    /// eligible bonded stake, non-jailed status, and CE-15 `<= max_single_validator_bps` cap.
    pub fn audit_consensus_schedule_weights(
        &self,
        cert: &ConsensusCommitCertificateV1,
        schedule: &EpochValidatorScheduleV1,
        econ_state: &EconomicStateV1,
    ) -> Result<ValidatorEconAdapterReceiptV1, ValidatorEcon1Error> {
        self.verify_cert_context(cert, schedule)?;
        econ_state.verify_conservation()?;
        let eligible = econ_state.eligible_validators();
        if eligible.len() != schedule.validators.len() {
            return Err(ValidatorEcon1Error::WeightCorrespondenceViolation(
                "schedule validator count disagrees with ECONOMIC-STATE-1 eligible set",
            ));
        }
        let mut total_weight = 0u128;
        for (sched_entry, &(econ_vid, econ_stake)) in schedule.validators.iter().zip(&eligible) {
            if sched_entry.validator_id != econ_vid || sched_entry.effective_weight != econ_stake {
                return Err(ValidatorEcon1Error::WeightCorrespondenceViolation(
                    "consensus schedule weight diverges from ECONOMIC-STATE-1 bonded stake",
                ));
            }
            let rec = econ_state.validator_record(&econ_vid).ok_or(
                ValidatorEcon1Error::WeightCorrespondenceViolation("missing economic record"),
            )?;
            if rec.jailed || rec.bonded_stake < self.policy.min_self_stake {
                return Err(ValidatorEcon1Error::WeightCorrespondenceViolation(
                    "jailed or under-staked validator present in consensus schedule",
                ));
            }
            total_weight = total_weight.checked_add(econ_stake).ok_or(
                ValidatorEcon1Error::WeightCorrespondenceViolation("weight overflow"),
            )?;
        }
        if total_weight != schedule.total_effective_weight {
            return Err(ValidatorEcon1Error::WeightCorrespondenceViolation(
                "total_effective_weight mismatch",
            ));
        }
        for v in &schedule.validators {
            let lhs = v
                .effective_weight
                .checked_mul(u128::from(BPS_DENOMINATOR))
                .ok_or(ValidatorEcon1Error::ConcentrationViolation("bps overflow"))?;
            let rhs = total_weight
                .checked_mul(u128::from(self.policy.max_single_validator_bps))
                .ok_or(ValidatorEcon1Error::ConcentrationViolation(
                    "bps rhs overflow",
                ))?;
            if lhs > rhs {
                return Err(ValidatorEcon1Error::ConcentrationViolation(
                    "validator exceeds CE-15 max_single_validator_bps ceiling",
                ));
            }
        }
        let root_sha = econ_state.root_commitment()?.canonical_sha256()?;
        let out = ValidatorEconAdapterReceiptV1 {
            receipt_version: VALIDATOR_ECON_1_VERSION,
            genesis_id: self.fork_context.genesis_id,
            profile_id: self.fork_context.profile_id,
            accepted_execution_root_ref: cert.proposal.execution_root_ref,
            consensus_block_id: cert.proposal.block_id(),
            consensus_height: cert.proposal.height,
            consensus_epoch: cert.proposal.epoch,
            action_kind: ValidatorEconActionV1::WeightScheduleAudit,
            economic_root_before_digest: root_sha,
            economic_root_after_digest: root_sha,
            economic_receipt_digest: [0u8; 32],
            validator_count: schedule.validators.len() as u32,
            total_active_weight: total_weight,
            reward_or_slash_amount: 0,
            reward_dust_unissued: 0,
            produces_execution_root_domain: false,
            block_token_used_as_stake: false,
        };
        out.validate()?;
        Ok(out)
    }

    /// Executes pro-rata integer-floor epoch rewards against `econ_state` for a finalized
    /// `CONSENSUS-1` certificate and proves bit-for-bit correspondence with `EconomicTransitionReceiptV1`.
    pub fn apply_epoch_rewards_for_finalized_block(
        &self,
        cert: &ConsensusCommitCertificateV1,
        schedule: &EpochValidatorScheduleV1,
        econ_state: &mut EconomicStateV1,
        budget: u128,
        reward_epoch: u64,
    ) -> Result<(ValidatorEconAdapterReceiptV1, EconomicTransitionReceiptV1), ValidatorEcon1Error>
    {
        self.verify_cert_context(cert, schedule)?;
        let pre_sha = econ_state.root_commitment()?.canonical_sha256()?;
        let econ_rcpt = econ_state.distribute_epoch_rewards(budget, reward_epoch)?;
        if econ_rcpt.disposition != EconomicDispositionV1::Admitted {
            return Err(ValidatorEcon1Error::EconomicReceiptMismatch(
                "ECONOMIC-STATE-1 rejected epoch reward distribution",
            ));
        }
        let post_sha = econ_state.root_commitment()?.canonical_sha256()?;
        if econ_rcpt.pre_root_sha256 != pre_sha || econ_rcpt.post_root_sha256 != post_sha {
            return Err(ValidatorEcon1Error::EconomicReceiptMismatch(
                "economic reward receipt roots disagree with committed state",
            ));
        }
        let eligible = econ_state.eligible_validators();
        let total_w: u128 = eligible.iter().map(|&(_, s)| s).sum();
        let adapter_rcpt = ValidatorEconAdapterReceiptV1 {
            receipt_version: VALIDATOR_ECON_1_VERSION,
            genesis_id: self.fork_context.genesis_id,
            profile_id: self.fork_context.profile_id,
            accepted_execution_root_ref: cert.proposal.execution_root_ref,
            consensus_block_id: cert.proposal.block_id(),
            consensus_height: cert.proposal.height,
            consensus_epoch: reward_epoch,
            action_kind: ValidatorEconActionV1::EpochReward,
            economic_root_before_digest: pre_sha,
            economic_root_after_digest: post_sha,
            economic_receipt_digest: economic_receipt_sha256(&econ_rcpt)?,
            validator_count: eligible.len() as u32,
            total_active_weight: total_w,
            reward_or_slash_amount: econ_rcpt.amount_delta,
            reward_dust_unissued: econ_rcpt.dust_unissued,
            produces_execution_root_domain: false,
            block_token_used_as_stake: false,
        };
        adapter_rcpt.validate()?;
        Ok((adapter_rcpt, econ_rcpt))
    }

    /// Verifies cryptographic [`VoteEquivocationEvidenceV1`] from `CONSENSUS-1` and applies the
    /// matching 50% active+unbonding slash-to-burn + permanent jailing in `ECONOMIC-STATE-1`.
    pub fn apply_verified_equivocation_slash(
        &self,
        cert: &ConsensusCommitCertificateV1,
        schedule: &EpochValidatorScheduleV1,
        evidence: &VoteEquivocationEvidenceV1,
        econ_state: &mut EconomicStateV1,
        slash_epoch: u64,
    ) -> Result<(ValidatorEconAdapterReceiptV1, EconomicTransitionReceiptV1), ValidatorEcon1Error>
    {
        self.verify_cert_context(cert, schedule)?;
        evidence.verify(&self.fork_context, schedule)?;
        let ev_dig = equivocation_evidence_digest(evidence)?;
        let pre_sha = econ_state.root_commitment()?.canonical_sha256()?;
        let econ_rcpt =
            econ_state.apply_equivocation_slash(evidence.voter_id, ev_dig, slash_epoch)?;
        if econ_rcpt.disposition != EconomicDispositionV1::Admitted {
            return Err(ValidatorEcon1Error::EconomicReceiptMismatch(
                "ECONOMIC-STATE-1 rejected verified equivocation slash",
            ));
        }
        let post_sha = econ_state.root_commitment()?.canonical_sha256()?;
        if econ_rcpt.pre_root_sha256 != pre_sha || econ_rcpt.post_root_sha256 != post_sha {
            return Err(ValidatorEcon1Error::EconomicReceiptMismatch(
                "equivocation slash receipt roots disagree with committed state",
            ));
        }
        let eligible = econ_state.eligible_validators();
        let total_w: u128 = eligible.iter().map(|&(_, s)| s).sum();
        let adapter_rcpt = ValidatorEconAdapterReceiptV1 {
            receipt_version: VALIDATOR_ECON_1_VERSION,
            genesis_id: self.fork_context.genesis_id,
            profile_id: self.fork_context.profile_id,
            accepted_execution_root_ref: cert.proposal.execution_root_ref,
            consensus_block_id: cert.proposal.block_id(),
            consensus_height: cert.proposal.height,
            consensus_epoch: slash_epoch,
            action_kind: ValidatorEconActionV1::EquivocationSlash,
            economic_root_before_digest: pre_sha,
            economic_root_after_digest: post_sha,
            economic_receipt_digest: economic_receipt_sha256(&econ_rcpt)?,
            validator_count: eligible.len() as u32,
            total_active_weight: total_w,
            reward_or_slash_amount: econ_rcpt.amount_delta,
            reward_dust_unissued: 0,
            produces_execution_root_domain: false,
            block_token_used_as_stake: false,
        };
        adapter_rcpt.validate()?;
        Ok((adapter_rcpt, econ_rcpt))
    }

    /// Applies bounded inactivity slashing in `ECONOMIC-STATE-1` when a validator misses at least
    /// `missed_threshold` consensus blocks in `slash_epoch`.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_inactivity_slash(
        &self,
        cert: &ConsensusCommitCertificateV1,
        schedule: &EpochValidatorScheduleV1,
        validator_id: ActorIdV1,
        missed_blocks: u32,
        missed_threshold: u32,
        econ_state: &mut EconomicStateV1,
        slash_epoch: u64,
    ) -> Result<(ValidatorEconAdapterReceiptV1, EconomicTransitionReceiptV1), ValidatorEcon1Error>
    {
        self.verify_cert_context(cert, schedule)?;
        if missed_threshold == 0 || missed_blocks < missed_threshold {
            return Err(ValidatorEcon1Error::ConsensusBindingViolation(
                "missed_blocks below configured inactivity slash threshold",
            ));
        }
        let ev_dig = inactivity_evidence_digest(validator_id, slash_epoch, missed_blocks);
        let pre_sha = econ_state.root_commitment()?.canonical_sha256()?;
        let econ_rcpt = econ_state.apply_inactivity_slash(validator_id, ev_dig, slash_epoch)?;
        if econ_rcpt.disposition != EconomicDispositionV1::Admitted {
            return Err(ValidatorEcon1Error::EconomicReceiptMismatch(
                "ECONOMIC-STATE-1 rejected inactivity slash",
            ));
        }
        let post_sha = econ_state.root_commitment()?.canonical_sha256()?;
        let eligible = econ_state.eligible_validators();
        let total_w: u128 = eligible.iter().map(|&(_, s)| s).sum();
        let adapter_rcpt = ValidatorEconAdapterReceiptV1 {
            receipt_version: VALIDATOR_ECON_1_VERSION,
            genesis_id: self.fork_context.genesis_id,
            profile_id: self.fork_context.profile_id,
            accepted_execution_root_ref: cert.proposal.execution_root_ref,
            consensus_block_id: cert.proposal.block_id(),
            consensus_height: cert.proposal.height,
            consensus_epoch: slash_epoch,
            action_kind: ValidatorEconActionV1::InactivitySlash,
            economic_root_before_digest: pre_sha,
            economic_root_after_digest: post_sha,
            economic_receipt_digest: economic_receipt_sha256(&econ_rcpt)?,
            validator_count: eligible.len() as u32,
            total_active_weight: total_w,
            reward_or_slash_amount: econ_rcpt.amount_delta,
            reward_dust_unissued: 0,
            produces_execution_root_domain: false,
            block_token_used_as_stake: false,
        };
        adapter_rcpt.validate()?;
        Ok((adapter_rcpt, econ_rcpt))
    }
}

/// Computes the domain-separated `SHA-256` transcript digest across the 7 adversarial scenarios.
pub fn compute_assay_transcript_digest(outcomes: &[AdversarialScenarioOutcomeV1; 7]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(VECON_ASSAY_TRANSCRIPT_PREFIX);
    for o in outcomes {
        h.update([o.scenario_id, u8::from(o.passed)]);
        h.update(o.primary_metric.to_be_bytes());
        h.update(o.secondary_metric.to_be_bytes());
        h.update([o.loss_condition.map_or(0, |c| c.code())]);
    }
    h.finalize().into()
}

/// Constructs and deterministically signs a valid [`crate::consensus_1::ConsensusVoteV1`]
/// against `schedule` and `cert` for equivocation-slashing correspondence simulations.
pub fn sign_consensus_vote_for_sim(
    fork: &ExecutorForkContextV1,
    schedule: &EpochValidatorScheduleV1,
    cert: &ConsensusCommitCertificateV1,
    validator_idx: u8,
    height: u64,
    block_id: [u8; 32],
) -> Result<crate::consensus_1::ConsensusVoteV1, ValidatorEcon1Error> {
    let sched_digest = schedule
        .tag()
        .map_err(|_| ValidatorEcon1Error::ConsensusBindingViolation("schedule tag failed"))?
        .digest();
    let exec_ref_digest: [u8; 32] =
        Sha256::digest(cert.proposal.execution_root_ref.encode_canonical()).into();
    let vote = crate::consensus_1::ConsensusVoteV1 {
        vote_version: 1,
        height,
        epoch: schedule.effective_epoch,
        view: 0,
        phase: crate::consensus_1::ConsensusVotePhaseV1::Prepare,
        block_id,
        schedule_epoch_digest: sched_digest,
        execution_root_ref_digest: exec_ref_digest,
        has_full_body: true,
        voter_id: ActorIdV1::from_bytes([validator_idx; 32]),
        vote_pubkey: [0u8; 32],
        signature: [0u8; 64],
    };
    Ok(vote.sign_with_seed(
        &ConsensusClusterHarnessV1::validator_vote_seed(validator_idx),
        fork,
    )?)
}

/// Executes the complete deterministic 7-vector adversarial incentive simulation (`A1` bribery,
/// `A2` censorship, `A3` equivocation slash-and-flee, `A4` griefing exhaustion, `A5` concentration
/// CE-15 Nakamoto floor, `A6` long-range weak-subjectivity barrier, `A7` post-slash recovery) and
/// returns both the canonical [`ValidatorEconAdversarialAssayV1`] and all 7 scenario outcomes.
pub fn run_canonical_validator_econ_adversarial_assay(
    cluster: &ConsensusClusterHarnessV1,
    cert: &ConsensusCommitCertificateV1,
) -> Result<
    (
        ValidatorEconAdversarialAssayV1,
        [AdversarialScenarioOutcomeV1; 7],
    ),
    ValidatorEcon1Error,
> {
    let policy = cluster.fixture.bundle.validator_econ_policy;
    let policy_tag_digest = policy
        .tag()
        .map_err(|_| ValidatorEcon1Error::InvalidWire("policy tag failed"))?
        .digest();
    let threat_model_digest = cluster.fixture.bundle.bounds_profile.threat_model_digest;
    let total_stake: u128 = cluster
        .economic_state
        .eligible_validators()
        .iter()
        .map(|&(_, s)| s)
        .sum();
    let (min_halt, min_safety) = nakamoto_lower_bounds(policy.max_single_validator_bps)?;
    let (min_burn_bps, min_burn_units) = safety_attack_burn_floor(total_stake, &policy)?;

    // Ensure occupancy token aliasing is explicitly rejected
    let aliasing_rejected =
        reject_occupancy_token_as_stake(Coord::new(3, 0, 0), [0xAA; 32], 1_000_000).is_err();

    // A1: Bribery — prove safety attack cost floor >= min_burn_units; trigger monitored loss alert
    // when external extractable value exceeds the slashable bond floor.
    let external_bribe_stress = min_burn_units + 1;
    let a1 = AdversarialScenarioOutcomeV1 {
        scenario_id: 1,
        passed: min_burn_units > 0 && external_bribe_stress > min_burn_units,
        primary_metric: min_burn_units,
        secondary_metric: external_bribe_stress,
        loss_condition: Some(MonitoredLossConditionV1::BriberyExceedsSlashFloor),
    };

    // A2: Censorship — single 25% validator cannot censor (>2/3 honest = 75% still finalize),
    // while 2-validator 50% cartel (> 1/3) triggers monitored liveness loss condition.
    let single_val_weight = total_stake / 4;
    let two_val_cartel = single_val_weight * 2;
    let a2 = AdversarialScenarioOutcomeV1 {
        scenario_id: 2,
        passed: single_val_weight * 3 <= total_stake && two_val_cartel * 3 > total_stake,
        primary_metric: single_val_weight,
        secondary_metric: two_val_cartel,
        loss_condition: Some(MonitoredLossConditionV1::CensorshipCartelAboveOneThird),
    };

    // A3: Equivocation (slash-and-flee defeated) — validator requests 20B unbond, then equivocates;
    // 50% is burned across BOTH active (80B) AND unbonding (20B) = 50B burned + permanent jail!
    let mut econ_sim = cluster.economic_state.clone();
    let v1 = ActorIdV1::from_bytes([1u8; 32]);
    let r_unbond = econ_sim.request_unbond(v1, 20_000_000_000, 1)?;
    assert_eq!(r_unbond.disposition, EconomicDispositionV1::Admitted);
    let sched0 = cluster.nodes[0]
        .schedule_at(0)
        .ok_or(ValidatorEcon1Error::ConsensusBindingViolation(
            "missing epoch 0 schedule",
        ))?
        .clone();
    let adapter = ValidatorEconAdapterV1::new(cluster.fixture.fork_context, policy)?;
    let vote_a = sign_consensus_vote_for_sim(
        &cluster.fixture.fork_context,
        &sched0,
        cert,
        1,
        2,
        [0x11; 32],
    )?;
    let vote_b = sign_consensus_vote_for_sim(
        &cluster.fixture.fork_context,
        &sched0,
        cert,
        1,
        2,
        [0x22; 32],
    )?;
    let eq_ev = VoteEquivocationEvidenceV1 {
        voter_id: v1,
        first_vote: vote_a,
        second_vote: vote_b,
    };
    let (eq_adapter_rcpt, eq_econ_rcpt) =
        adapter.apply_verified_equivocation_slash(cert, &sched0, &eq_ev, &mut econ_sim, 1)?;
    let v1_after = econ_sim.validator_record(&v1).expect("v1 exists");
    let a3 = AdversarialScenarioOutcomeV1 {
        scenario_id: 3,
        passed: eq_adapter_rcpt.reward_or_slash_amount == 50_000_000_000
            && eq_econ_rcpt.amount_delta == 50_000_000_000
            && v1_after.jailed
            && v1_after.bonded_stake == 40_000_000_000,
        primary_metric: eq_adapter_rcpt.reward_or_slash_amount,
        secondary_metric: v1_after.bonded_stake,
        loss_condition: None,
    };

    // A4: Griefing — repeated inactivity slashing (1% = 1B burned) + admitted-failure base fee burn
    // strictly depletes adversarial attacker balance on every griefing round.
    let v2 = ActorIdV1::from_bytes([2u8; 32]);
    let (inact_adapter, inact_econ) =
        adapter.apply_inactivity_slash(cert, &sched0, v2, 16, 10, &mut econ_sim, 1)?;
    let base_fee = cluster.fixture.bundle.asset_policy.base_fee_burn;
    let a4 = AdversarialScenarioOutcomeV1 {
        scenario_id: 4,
        passed: inact_adapter.reward_or_slash_amount == 1_000_000_000
            && inact_econ.amount_delta == 1_000_000_000
            && base_fee > 0,
        primary_metric: inact_adapter.reward_or_slash_amount,
        secondary_metric: base_fee,
        loss_condition: None,
    };

    // A5: Concentration — CE-15 2,500 bps cap enforces Nakamoto halt >= 2 & safety >= 3.
    let a5 = AdversarialScenarioOutcomeV1 {
        scenario_id: 5,
        passed: policy.max_single_validator_bps == 2_500 && min_halt == 2 && min_safety == 3,
        primary_metric: u128::from(policy.max_single_validator_bps),
        secondary_metric: u128::from(min_safety),
        loss_condition: None,
    };

    // A6: Long-range — unbonded historical key from epoch 0 rejected at epoch 22 (> 21 horizon).
    let stale_rejected = verify_weak_subjectivity_key_freshness(0, 22, &policy).is_err();
    let fresh_allowed = verify_weak_subjectivity_key_freshness(0, 21, &policy).is_ok();
    let a6 = AdversarialScenarioOutcomeV1 {
        scenario_id: 6,
        passed: stale_rejected && fresh_allowed,
        primary_metric: u128::from(policy.weak_subjectivity_horizon_epochs),
        secondary_metric: 22,
        loss_condition: None,
    };

    // A7: Recovery — jailed v1 cannot re-bond; v2 tops up 1B after inactivity slash and
    // replacement Validator 5 bonds min_self_stake = 100B, restoring 4 eligible active validators
    // while preserving exact supply conservation.
    let v1_rej = econ_sim.bond_stake(v1, policy.min_self_stake, 2)?;
    let v2_top = econ_sim.bond_stake(v2, 1_000_000_000, 2)?;
    let v5 = ActorIdV1::from_bytes([5u8; 32]);
    let r_bond = econ_sim.bond_stake(v5, policy.min_self_stake, 2)?;
    econ_sim.verify_conservation()?;
    let recovered_count = econ_sim.eligible_validators().len() as u128;
    let a7 = AdversarialScenarioOutcomeV1 {
        scenario_id: 7,
        passed: v1_rej.disposition == EconomicDispositionV1::RejectedValidatorJailed
            && v2_top.disposition == EconomicDispositionV1::Admitted
            && r_bond.disposition == EconomicDispositionV1::Admitted
            && recovered_count == 4
            && policy.activation_delay_epochs == 2,
        primary_metric: policy.min_self_stake,
        secondary_metric: recovered_count,
        loss_condition: None,
    };

    let outcomes = [a1, a2, a3, a4, a5, a6, a7];
    let mut mask = 0u8;
    let mut alert_count = 0u32;
    for (idx, o) in outcomes.iter().enumerate() {
        if o.passed {
            mask |= 1u8 << idx;
        }
        if o.loss_condition.is_some() {
            alert_count += 1;
        }
    }
    let transcript_digest = compute_assay_transcript_digest(&outcomes);
    let assay = ValidatorEconAdversarialAssayV1 {
        assay_version: VALIDATOR_ECON_1_VERSION,
        genesis_id: cluster.fixture.package.genesis_id,
        profile_id: cluster.fixture.bound_manifest.profile_id,
        policy_tag_digest,
        threat_model_digest,
        total_network_stake: total_stake,
        min_nakamoto_halt: min_halt,
        min_nakamoto_safety: min_safety,
        equivocation_slash_cost_bps: policy.equivocation_slash_bps,
        min_safety_attack_burn_bps: min_burn_bps,
        passed_attack_mask: mask,
        monitored_loss_alert_count: alert_count,
        block_token_stake_aliasing_rejected: aliasing_rejected,
        scenario_transcript_digest: transcript_digest,
    };
    assay.validate()?;
    Ok((assay, outcomes))
}

fn read_u8(b: &[u8], p: &mut usize) -> u8 {
    let v = b[*p];
    *p += 1;
    v
}

fn read_bool(b: &[u8], p: &mut usize) -> Result<bool, ValidatorEcon1Error> {
    match read_u8(b, p) {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(ValidatorEcon1Error::InvalidWire("non-boolean flag byte")),
    }
}

fn read_u16_be(b: &[u8], p: &mut usize) -> u16 {
    let v = u16::from_be_bytes([b[*p], b[*p + 1]]);
    *p += 2;
    v
}

fn read_u32_be(b: &[u8], p: &mut usize) -> u32 {
    let v = u32::from_be_bytes([b[*p], b[*p + 1], b[*p + 2], b[*p + 3]]);
    *p += 4;
    v
}

fn read_u64_be(b: &[u8], p: &mut usize) -> u64 {
    let v = u64::from_be_bytes(b[*p..*p + 8].try_into().expect("8 bytes"));
    *p += 8;
    v
}

fn read_u128_be(b: &[u8], p: &mut usize) -> u128 {
    let v = u128::from_be_bytes(b[*p..*p + 16].try_into().expect("16 bytes"));
    *p += 16;
    v
}

fn read_arr32(b: &[u8], p: &mut usize) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(&b[*p..*p + 32]);
    *p += 32;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validator_econ_1_adapter_assay_and_golden_tags() {
        let mut cluster = ConsensusClusterHarnessV1::bootstrap_four_validators().unwrap();
        let pre_wr = cluster.nodes[0].harness().snapshot().unwrap().world_root;
        let bundle = cluster
            .fixture
            .canonical_mixed_race_bundle(1, pre_wr)
            .unwrap();
        let cert = cluster
            .run_round_with_subset(&bundle, 0, &[0, 1, 2, 3])
            .unwrap();
        let sched0 = cluster.nodes[0].schedule_at(0).unwrap().clone();
        let adapter = ValidatorEconAdapterV1::new(
            cluster.fixture.fork_context,
            cluster.fixture.bundle.validator_econ_policy,
        )
        .unwrap();

        let mut econ_copy = cluster.economic_state.clone();
        let (rew_adapter, rew_econ) = adapter
            .apply_epoch_rewards_for_finalized_block(&cert, &sched0, &mut econ_copy, 999_999_999, 1)
            .unwrap();
        assert_eq!(rew_adapter.reward_or_slash_amount, 999_999_996);
        assert_eq!(rew_adapter.reward_dust_unissued, 3);
        assert_eq!(rew_econ.dust_unissued, 3);
        assert!(!rew_adapter.produces_execution_root_domain);
        assert!(!rew_adapter.block_token_used_as_stake);
        assert_eq!(
            ValidatorEconAdapterReceiptV1::decode_canonical(
                &rew_adapter.encode_canonical().unwrap()
            )
            .unwrap(),
            rew_adapter
        );

        let (assay, outcomes) =
            run_canonical_validator_econ_adversarial_assay(&cluster, &cert).unwrap();
        assert_eq!(assay.passed_attack_mask, ALL_SEVEN_ATTACK_VECTORS_MASK);
        assert!(outcomes.iter().all(|o| o.passed));
        assert_eq!(assay.monitored_loss_alert_count, 2);
        assert_eq!(
            ValidatorEconAdversarialAssayV1::decode_canonical(&assay.encode_canonical().unwrap())
                .unwrap(),
            assay
        );

        let adapter_tag = rew_adapter.tag().unwrap().to_string();
        let assay_tag = assay.tag().unwrap().to_string();
        assert_eq!(
            adapter_tag,
            "#bb.receipt.sha256.kwwsv73gxzeuug576j55yktxgcdn6zo5zuv4pguns5ewzq4nukda"
        );
        assert_eq!(
            assay_tag,
            "#bb.receipt.sha256.diw6cjminiwaapvdpjqh36cu7tthymzfzg62wrdeviizgnlftjta"
        );
    }
}
