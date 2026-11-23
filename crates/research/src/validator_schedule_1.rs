//! `VALIDATOR-SCHEDULE-1`: deterministic 2-epoch delayed validator schedule state, immutable
//! historical epoch proofs, `CE-15` stake-weight eligibility, key-rotation scheduling, and strict
//! `< 1/3` BFT weight concentration enforcement over `DomainIdV1::ValidatorSchedule` (`PS-055`).
//!
//! Completes the Waves 13–15 Launch State-Producer Trunk behind `--features protocol-research`
//! (unlocking `PS-065:EXECUTOR-1@base=READY`) without touching `crates/core/**` or
//! `commit_state.rs`:
//! - **Strict 2-epoch activation delay:** economic stake changes, joins, exits, slashes, and key
//!   rotations observed at `source_epoch = E` take effect strictly at
//!   `effective_epoch = E + activation_delay_epochs` (`E + 2`).
//! - **Immutable historical epoch schedules:** once committed into `DomainIdV1::ValidatorSchedule`
//!   (Domain 7), no past or already-scheduled epoch can be overwritten or retroactively selected
//!   by a proposal or vote (`RejectedRetroactiveScheduleRewrite`).
//! - **`CE-15` economic coupling & `< 1/3` BFT safety guard:** consumes committed
//!   `EconomicStateV1::eligible_validators()` (excluding jailed or `< min_self_stake` validators),
//!   enforces `4 <= N <= max_validators (128)`, applies deterministic policy concentration capping
//!   when `N >= 5`, and hard-rejects any schedule whose maximum single-validator weight exceeds
//!   `3,333 bps` (`< 1/3` BFT threshold).
//! - **Key-domain disjointness:** enforces distinct `(vote_key, proposal_key)` pairs across all
//!   validators and strictly disjoint from ordinary `AUTH-COMMIT-1` actor keys.
//!
//! A committed validator schedule does not prove quorum honesty, decentralization, consensus
//! safety/liveness, economic adequacy, signer custody, or data availability.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use sha2::{Digest, Sha256};

use crate::asset_d1::{BPS_DENOMINATOR, ValidatorEconPolicyV1};
use crate::auth_commit_1::{
    AuthCommit1Error, DomainIdV1, DomainPatriciaProofV1, DomainPatriciaTrieV1,
};
use crate::consensus_gov_d1::{
    ConsensusGovError, ConsensusPolicyDescriptorV1, InitialValidatorDescriptorV1,
    InitialValidatorSetV1, MAX_ACTIVE_VALIDATORS_V1,
};
use crate::crypto_d1::CanonicalEd25519PublicKeyV1;
use crate::economic_state_1::{EconomicState1Error, EconomicStateV1};
use crate::governance_1::{Governance1Error, GovernanceLifecycleCodeV1, GovernanceStateV1};
use crate::identity_d1::ActorIdV1;
use crate::protocol::{ContentTag, TagKind, hash_canonical};

/// Canonical schema string for a [`ValidatorScheduleRootV1`] commitment (`VALIDATOR-SCHEDULE-1`).
pub const VALIDATOR_SCHEDULE_1_ROOT_SCHEMA: &[u8] =
    b"bitblock/validator-schedule-1/schedule-root/v1";
/// Canonical schema string for an [`EpochValidatorScheduleV1`] epoch snapshot (`VALIDATOR-SCHEDULE-1`).
pub const VALIDATOR_SCHEDULE_1_EPOCH_SCHEMA: &[u8] =
    b"bitblock/validator-schedule-1/epoch-schedule/v1";

/// Hard BFT single-validator weight ceiling (`3,333 bps < 1/3`).
pub const BFT_HARD_CONCENTRATION_CAP_BPS: u16 = 3_333;

const EPOCH_KEY_PREFIX: &[u8] = b"bb.vs1.epoch_key.v1";

/// Fail-closed validation, codec, concentration, delay, or Patricia error for `VALIDATOR-SCHEDULE-1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValidatorSchedule1Error {
    /// Underlying `CONSENSUS-D1` / `GOVERNANCE-D1` validation error.
    ConsensusGov(ConsensusGovError),
    /// Underlying `AUTH-COMMIT-1` Patricia proof or rebuild error.
    AuthCommit(AuthCommit1Error),
    /// Underlying `ECONOMIC-STATE-1` commitment error.
    Economic(EconomicState1Error),
    /// Underlying `GOVERNANCE-1` commitment error.
    Governance(Governance1Error),
    /// Canonical wire buffer had wrong length, version, sorting, or trailing bytes.
    InvalidWire(&'static str),
    /// Active validator count fell outside `4..=max_validators`.
    RejectedValidatorCountOutOfBounds,
    /// One validator held `> 3,333 bps` (`>= 1/3` BFT safety breach).
    RejectedConcentrationViolation {
        /// Observed maximum single-validator basis-point share.
        max_bps: u16,
    },
    /// Attempted to overwrite or retroactively select an already-committed epoch schedule.
    RejectedRetroactiveScheduleRewrite {
        /// Target epoch that was already committed.
        effective_epoch: u64,
    },
    /// Attempted to schedule an epoch out of contiguous sequence.
    RejectedNonContiguousEpoch {
        /// Next expected effective epoch.
        expected_effective_epoch: u64,
        /// Supplied effective epoch.
        actual_effective_epoch: u64,
    },
    /// Governance state was `SafeHalted` or economic source epoch disagreed with `source_epoch`.
    RejectedStaleOrHaltedSource(&'static str),
    /// Validator vote/proposal key collided with another validator key or an actor key.
    RejectedKeyDomainCollision(&'static str),
    /// Eligible validator lacked registered consensus vote/proposal keys.
    MissingValidatorKeys(ActorIdV1),
}

impl From<ConsensusGovError> for ValidatorSchedule1Error {
    fn from(value: ConsensusGovError) -> Self {
        Self::ConsensusGov(value)
    }
}

impl From<AuthCommit1Error> for ValidatorSchedule1Error {
    fn from(value: AuthCommit1Error) -> Self {
        Self::AuthCommit(value)
    }
}

impl From<EconomicState1Error> for ValidatorSchedule1Error {
    fn from(value: EconomicState1Error) -> Self {
        Self::Economic(value)
    }
}

impl From<Governance1Error> for ValidatorSchedule1Error {
    fn from(value: Governance1Error) -> Self {
        Self::Governance(value)
    }
}

impl fmt::Display for ValidatorSchedule1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ValidatorSchedule1Error {}

/// Computes the 32-byte Domain 7 Patricia key for `effective_epoch`.
pub fn epoch_trie_key(effective_epoch: u64) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(EPOCH_KEY_PREFIX);
    h.update(effective_epoch.to_be_bytes());
    h.finalize().into()
}

/// Role-separated consensus vote and proposal public keys bound to one validator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ValidatorKeyPairV1 {
    /// Strict RFC 8032 Ed25519 key used exclusively for `ConsensusVote`.
    pub vote_key: CanonicalEd25519PublicKeyV1,
    /// Strict RFC 8032 Ed25519 key used exclusively for `BlockProposal`.
    pub proposal_key: CanonicalEd25519PublicKeyV1,
}

/// One active validator entry inside an immutable [`EpochValidatorScheduleV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ScheduledValidatorEntryV1 {
    /// Validator identity (`ActorIdV1`).
    pub validator_id: ActorIdV1,
    /// Role-separated Ed25519 vote public key.
    pub vote_key: CanonicalEd25519PublicKeyV1,
    /// Role-separated Ed25519 block proposal public key.
    pub proposal_key: CanonicalEd25519PublicKeyV1,
    /// Effective voting weight for this epoch (`> 0`).
    pub effective_weight: u128,
}

/// Immutable per-epoch validator schedule committed inside Domain 7 (`VALIDATOR-SCHEDULE-1`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EpochValidatorScheduleV1 {
    /// Schedule schema version (`1`).
    pub schedule_version: u16,
    /// Epoch in which this schedule governs consensus (`source_epoch + activation_delay`).
    pub effective_epoch: u64,
    /// Source economic/governance epoch from which this schedule was derived.
    pub source_epoch: u64,
    /// Canonical SHA-256 of the `CE-15` `EconomicStateRootV1` snapshot at `source_epoch`.
    pub economic_root_digest: [u8; 32],
    /// Canonical SHA-256 of the `GovernanceStateRootV1` snapshot at `source_epoch`.
    pub governance_root_digest: [u8; 32],
    /// Sum of `effective_weight` across all `validators`.
    pub total_effective_weight: u128,
    /// Maximum single-validator basis-point share (`<= 3,333 bps`).
    pub max_single_validator_bps: u16,
    /// Strictly sorted unique active validator entries (`4..=128`).
    pub validators: Vec<ScheduledValidatorEntryV1>,
}

impl EpochValidatorScheduleV1 {
    /// Fixed header byte length (`102` bytes) preceding `112`-byte validator entries.
    pub const HEADER_LEN: usize = 2 + 8 + 8 + 32 + 32 + 16 + 2 + 2;
    /// Fixed byte length of one encoded [`ScheduledValidatorEntryV1`] (`112` bytes).
    pub const ENTRY_LEN: usize = 32 + 32 + 32 + 16;

    /// Validates sorting, count bounds, distinct keys, weight sum, and `< 1/3` BFT safety cap.
    pub fn validate(&self) -> Result<(), ValidatorSchedule1Error> {
        if self.schedule_version != 1 {
            return Err(ValidatorSchedule1Error::InvalidWire(
                "schedule_version must equal 1",
            ));
        }
        if self.validators.len() < 4
            || self.validators.len() > usize::from(MAX_ACTIVE_VALIDATORS_V1)
        {
            return Err(ValidatorSchedule1Error::RejectedValidatorCountOutOfBounds);
        }
        let mut seen_keys = BTreeSet::new();
        let mut sum = 0u128;
        for (idx, v) in self.validators.iter().enumerate() {
            if idx > 0 && self.validators[idx - 1].validator_id >= v.validator_id {
                return Err(ValidatorSchedule1Error::InvalidWire(
                    "validators must be strictly sorted by unique validator_id",
                ));
            }
            if v.effective_weight == 0 {
                return Err(ValidatorSchedule1Error::InvalidWire(
                    "effective_weight must be > 0",
                ));
            }
            if v.vote_key == v.proposal_key
                || !seen_keys.insert(v.vote_key)
                || !seen_keys.insert(v.proposal_key)
            {
                return Err(ValidatorSchedule1Error::RejectedKeyDomainCollision(
                    "vote/proposal keys must be globally distinct",
                ));
            }
            sum =
                sum.checked_add(v.effective_weight)
                    .ok_or(ValidatorSchedule1Error::InvalidWire(
                        "total_effective_weight overflow",
                    ))?;
        }
        if sum != self.total_effective_weight || sum == 0 {
            return Err(ValidatorSchedule1Error::InvalidWire(
                "total_effective_weight mismatch",
            ));
        }
        let max_bps = compute_max_bps(&self.validators, sum)?;
        if max_bps > BFT_HARD_CONCENTRATION_CAP_BPS {
            return Err(ValidatorSchedule1Error::RejectedConcentrationViolation { max_bps });
        }
        if max_bps != self.max_single_validator_bps {
            return Err(ValidatorSchedule1Error::InvalidWire(
                "max_single_validator_bps mismatch",
            ));
        }
        Ok(())
    }

    /// Encodes this epoch schedule into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, ValidatorSchedule1Error> {
        self.validate()?;
        let mut buf =
            Vec::with_capacity(Self::HEADER_LEN + self.validators.len() * Self::ENTRY_LEN);
        buf.extend_from_slice(&self.schedule_version.to_be_bytes());
        buf.extend_from_slice(&self.effective_epoch.to_be_bytes());
        buf.extend_from_slice(&self.source_epoch.to_be_bytes());
        buf.extend_from_slice(&self.economic_root_digest);
        buf.extend_from_slice(&self.governance_root_digest);
        buf.extend_from_slice(&self.total_effective_weight.to_be_bytes());
        buf.extend_from_slice(&self.max_single_validator_bps.to_be_bytes());
        buf.extend_from_slice(&(self.validators.len() as u16).to_be_bytes());
        for v in &self.validators {
            buf.extend_from_slice(v.validator_id.as_bytes());
            buf.extend_from_slice(v.vote_key.as_bytes());
            buf.extend_from_slice(v.proposal_key.as_bytes());
            buf.extend_from_slice(&v.effective_weight.to_be_bytes());
        }
        Ok(buf)
    }

    /// Decodes and validates a canonical [`EpochValidatorScheduleV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, ValidatorSchedule1Error> {
        if bytes.len() < Self::HEADER_LEN {
            return Err(ValidatorSchedule1Error::InvalidWire(
                "truncated EpochValidatorScheduleV1",
            ));
        }
        let count = u16::from_be_bytes([bytes[100], bytes[101]]) as usize;
        if bytes.len() != Self::HEADER_LEN + count * Self::ENTRY_LEN {
            return Err(ValidatorSchedule1Error::InvalidWire(
                "EpochValidatorScheduleV1 length mismatch",
            ));
        }
        let mut validators = Vec::with_capacity(count);
        for i in 0..count {
            let off = Self::HEADER_LEN + i * Self::ENTRY_LEN;
            let vid: [u8; 32] = bytes[off..off + 32].try_into().expect("32");
            let vk = CanonicalEd25519PublicKeyV1::from_bytes(
                bytes[off + 32..off + 64].try_into().expect("32"),
            )
            .map_err(|_| ValidatorSchedule1Error::InvalidWire("invalid vote_key"))?;
            let pk = CanonicalEd25519PublicKeyV1::from_bytes(
                bytes[off + 64..off + 96].try_into().expect("32"),
            )
            .map_err(|_| ValidatorSchedule1Error::InvalidWire("invalid proposal_key"))?;
            let w = u128::from_be_bytes(bytes[off + 96..off + 112].try_into().expect("16"));
            validators.push(ScheduledValidatorEntryV1 {
                validator_id: ActorIdV1::from_bytes(vid),
                vote_key: vk,
                proposal_key: pk,
                effective_weight: w,
            });
        }
        let sched = Self {
            schedule_version: u16::from_be_bytes([bytes[0], bytes[1]]),
            effective_epoch: u64::from_be_bytes(bytes[2..10].try_into().expect("8")),
            source_epoch: u64::from_be_bytes(bytes[10..18].try_into().expect("8")),
            economic_root_digest: bytes[18..50].try_into().expect("32"),
            governance_root_digest: bytes[50..82].try_into().expect("32"),
            total_effective_weight: u128::from_be_bytes(bytes[82..98].try_into().expect("16")),
            max_single_validator_bps: u16::from_be_bytes([bytes[98], bytes[99]]),
            validators,
        };
        sched.validate()?;
        Ok(sched)
    }

    /// Converts this schedule into a `CONSENSUS-D1` [`InitialValidatorSetV1`] for QC checks.
    pub fn to_validator_set(&self) -> Result<InitialValidatorSetV1, ValidatorSchedule1Error> {
        let items: Vec<_> = self
            .validators
            .iter()
            .map(|v| InitialValidatorDescriptorV1 {
                validator_id: v.validator_id,
                vote_key: v.vote_key,
                proposal_key: v.proposal_key,
                weight: v.effective_weight,
            })
            .collect();
        Ok(InitialValidatorSetV1::new(items)?)
    }

    /// Computes the canonical `Receipt` `ContentTag` for this epoch schedule.
    pub fn tag(&self) -> Result<ContentTag, ValidatorSchedule1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            VALIDATOR_SCHEDULE_1_EPOCH_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and bounded length"))
    }
}

/// Authenticated root commitment over all historical and scheduled validator epochs (Domain 7).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ValidatorScheduleRootV1 {
    /// Root schema version (`1`).
    pub root_version: u16,
    /// Bound `PROFILE-D1` profile identifier.
    pub profile_id: [u8; 32],
    /// Current consensus epoch.
    pub current_epoch: u64,
    /// Highest committed future `effective_epoch` (`>= current_epoch`).
    pub latest_scheduled_epoch: u64,
    /// Bound activation delay in epochs (`>= 2`).
    pub activation_delay_epochs: u64,
    /// Number of immutable committed epoch schedules in Domain 7.
    pub scheduled_epoch_count: u32,
    /// Active validator count at `current_epoch`.
    pub active_validator_count: u16,
    /// Maximum single-validator basis-point share at `current_epoch` (`<= 3,333`).
    pub max_single_validator_bps: u16,
    /// 32-byte Domain 7 Patricia root (`DomainIdV1::ValidatorSchedule`).
    pub schedule_trie_root: [u8; 32],
}

impl ValidatorScheduleRootV1 {
    /// Fixed canonical big-endian wire length (`98` bytes).
    pub const WIRE_LEN: usize = 2 + 32 + 8 + 8 + 8 + 4 + 2 + 2 + 32;

    /// Encodes this schedule root into 98 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], ValidatorSchedule1Error> {
        if self.root_version != 1 {
            return Err(ValidatorSchedule1Error::InvalidWire(
                "root_version must equal 1",
            ));
        }
        if self.activation_delay_epochs < 2
            || self.max_single_validator_bps > BFT_HARD_CONCENTRATION_CAP_BPS
        {
            return Err(ValidatorSchedule1Error::InvalidWire(
                "invalid activation delay or concentration bps",
            ));
        }
        let mut buf = Vec::with_capacity(Self::WIRE_LEN);
        buf.extend_from_slice(&self.root_version.to_be_bytes());
        buf.extend_from_slice(&self.profile_id);
        buf.extend_from_slice(&self.current_epoch.to_be_bytes());
        buf.extend_from_slice(&self.latest_scheduled_epoch.to_be_bytes());
        buf.extend_from_slice(&self.activation_delay_epochs.to_be_bytes());
        buf.extend_from_slice(&self.scheduled_epoch_count.to_be_bytes());
        buf.extend_from_slice(&self.active_validator_count.to_be_bytes());
        buf.extend_from_slice(&self.max_single_validator_bps.to_be_bytes());
        buf.extend_from_slice(&self.schedule_trie_root);
        Ok(buf.try_into().expect("98"))
    }

    /// Decodes a 98-byte canonical [`ValidatorScheduleRootV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, ValidatorSchedule1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(ValidatorSchedule1Error::InvalidWire(
                "ValidatorScheduleRootV1 must be 98 bytes",
            ));
        }
        let root = Self {
            root_version: u16::from_be_bytes([bytes[0], bytes[1]]),
            profile_id: bytes[2..34].try_into().expect("32"),
            current_epoch: u64::from_be_bytes(bytes[34..42].try_into().expect("8")),
            latest_scheduled_epoch: u64::from_be_bytes(bytes[42..50].try_into().expect("8")),
            activation_delay_epochs: u64::from_be_bytes(bytes[50..58].try_into().expect("8")),
            scheduled_epoch_count: u32::from_be_bytes(bytes[58..62].try_into().expect("4")),
            active_validator_count: u16::from_be_bytes([bytes[62], bytes[63]]),
            max_single_validator_bps: u16::from_be_bytes([bytes[64], bytes[65]]),
            schedule_trie_root: bytes[66..98].try_into().expect("32"),
        };
        let _ = root.encode_canonical()?;
        Ok(root)
    }

    /// Computes the canonical `Receipt` `ContentTag` for this validator schedule root.
    pub fn tag(&self) -> Result<ContentTag, ValidatorSchedule1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            VALIDATOR_SCHEDULE_1_ROOT_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

fn compute_max_bps(
    validators: &[ScheduledValidatorEntryV1],
    total: u128,
) -> Result<u16, ValidatorSchedule1Error> {
    let mut max_bps = 0u16;
    for v in validators {
        let num = v
            .effective_weight
            .checked_mul(u128::from(BPS_DENOMINATOR))
            .ok_or(ValidatorSchedule1Error::InvalidWire("bps mul overflow"))?;
        let bps = (num / total) as u16;
        if bps > max_bps {
            max_bps = bps;
        }
    }
    Ok(max_bps)
}

type EffectiveWeightsSummary = (Vec<(ActorIdV1, u128)>, u128, u16);

/// Applies deterministic policy concentration capping when `N >= 5` so no single validator
/// exceeds `policy_cap_bps` (`2,500 bps`), then enforces hard `< 1/3` (`3,333 bps`) BFT safety.
fn compute_effective_weights(
    raw: &[(ActorIdV1, u128)],
    policy_cap_bps: u16,
) -> Result<EffectiveWeightsSummary, ValidatorSchedule1Error> {
    let n = raw.len();
    let mut weights: Vec<(ActorIdV1, u128)> = raw.to_vec();
    let mut total: u128 = 0;
    for &(_, w) in &weights {
        total = total
            .checked_add(w)
            .ok_or(ValidatorSchedule1Error::InvalidWire("stake sum overflow"))?;
    }
    // Policy target cap is feasible strictly when n * policy_cap_bps > 10_000 (e.g. N >= 5 at 2,500 bps).
    if (n as u64) * u64::from(policy_cap_bps) > u64::from(BPS_DENOMINATOR) {
        let mut capped = vec![false; n];
        loop {
            let mut capped_count = 0usize;
            let mut uncapped_sum = 0u128;
            for i in 0..n {
                if capped[i] {
                    capped_count += 1;
                } else {
                    uncapped_sum += weights[i].1;
                }
            }
            let rem_bps = u128::from(BPS_DENOMINATOR)
                .saturating_sub((capped_count as u128) * u128::from(policy_cap_bps));
            if rem_bps == 0 || uncapped_sum == 0 {
                break;
            }
            let cap_val = (uncapped_sum * u128::from(policy_cap_bps)) / rem_bps;
            let mut newly_capped = false;
            for i in 0..n {
                if !capped[i] && weights[i].1 > cap_val {
                    weights[i].1 = cap_val.max(1);
                    capped[i] = true;
                    newly_capped = true;
                }
            }
            if !newly_capped {
                for i in 0..n {
                    if capped[i] {
                        weights[i].1 = cap_val.max(1);
                    }
                }
                break;
            }
        }
        total = weights.iter().map(|(_, w)| *w).sum();
    }
    let mut max_bps = 0u16;
    for &(_, w) in &weights {
        let bps = (w
            .checked_mul(u128::from(BPS_DENOMINATOR))
            .ok_or(ValidatorSchedule1Error::InvalidWire("bps overflow"))?
            / total) as u16;
        if bps > max_bps {
            max_bps = bps;
        }
    }
    if max_bps > BFT_HARD_CONCENTRATION_CAP_BPS {
        return Err(ValidatorSchedule1Error::RejectedConcentrationViolation { max_bps });
    }
    Ok((weights, total, max_bps))
}

/// Deterministic delayed-activation validator schedule engine (`DomainIdV1::ValidatorSchedule`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatorScheduleStateV1 {
    consensus_policy: ConsensusPolicyDescriptorV1,
    econ_policy: ValidatorEconPolicyV1,
    profile_id: [u8; 32],
    current_epoch: u64,
    latest_scheduled_epoch: u64,
    active_keys: BTreeMap<ActorIdV1, ValidatorKeyPairV1>,
    pending_rotations: BTreeMap<(u64, ActorIdV1), ValidatorKeyPairV1>,
    schedules: BTreeMap<u64, EpochValidatorScheduleV1>,
    trie: DomainPatriciaTrieV1,
}

impl ValidatorScheduleStateV1 {
    /// Initializes genesis bootstrap schedules (`0..activation_delay_epochs`) from `econ` and `gov`.
    pub fn from_genesis(
        consensus_policy: ConsensusPolicyDescriptorV1,
        econ_policy: ValidatorEconPolicyV1,
        profile_id: [u8; 32],
        econ: &EconomicStateV1,
        gov: &GovernanceStateV1,
        initial_keys: &[(ActorIdV1, ValidatorKeyPairV1)],
        forbidden_actor_keys: &BTreeSet<CanonicalEd25519PublicKeyV1>,
    ) -> Result<Self, ValidatorSchedule1Error> {
        econ_policy
            .validate()
            .map_err(|_| ValidatorSchedule1Error::InvalidWire("invalid ValidatorEconPolicyV1"))?;
        consensus_policy.validate(Some(&econ_policy))?;
        if econ.current_epoch() != 0 {
            return Err(ValidatorSchedule1Error::RejectedStaleOrHaltedSource(
                "genesis schedule requires economic epoch 0",
            ));
        }
        let mut active_keys = BTreeMap::new();
        for &(vid, kp) in initial_keys {
            validate_keypair_disjoint(&kp, &active_keys, Some(vid), forbidden_actor_keys)?;
            active_keys.insert(vid, kp);
        }
        let econ_sha = econ.root_commitment()?.canonical_sha256()?;
        let gov_sha = gov.root_commitment().canonical_sha256()?;
        let eligible = econ.eligible_validators();
        let delay = econ_policy.activation_delay_epochs;
        let mut schedules = BTreeMap::new();
        let mut pairs = Vec::with_capacity(delay as usize);
        for ep in 0..delay {
            let sched = build_epoch_schedule(
                ep,
                0,
                econ_sha,
                gov_sha,
                &eligible,
                &active_keys,
                &consensus_policy,
                &econ_policy,
                forbidden_actor_keys,
            )?;
            pairs.push((epoch_trie_key(ep), sched.encode_canonical()?));
            schedules.insert(ep, sched);
        }
        let trie = DomainPatriciaTrieV1::from_entries(DomainIdV1::ValidatorSchedule, pairs)?;
        Ok(Self {
            consensus_policy,
            econ_policy,
            profile_id,
            current_epoch: 0,
            latest_scheduled_epoch: delay - 1,
            active_keys,
            pending_rotations: BTreeMap::new(),
            schedules,
            trie,
        })
    }

    /// Registers consensus vote/proposal keys for a newly joining validator (disjoint from actor keys).
    pub fn register_validator_keys(
        &mut self,
        validator_id: ActorIdV1,
        keys: ValidatorKeyPairV1,
        forbidden_actor_keys: &BTreeSet<CanonicalEd25519PublicKeyV1>,
    ) -> Result<(), ValidatorSchedule1Error> {
        validate_keypair_disjoint(
            &keys,
            &self.active_keys,
            Some(validator_id),
            forbidden_actor_keys,
        )?;
        self.active_keys.insert(validator_id, keys);
        Ok(())
    }

    /// Queues a delayed validator key rotation taking effect strictly at `source_epoch + delay`.
    pub fn queue_key_rotation(
        &mut self,
        validator_id: ActorIdV1,
        new_keys: ValidatorKeyPairV1,
        source_epoch: u64,
        forbidden_actor_keys: &BTreeSet<CanonicalEd25519PublicKeyV1>,
    ) -> Result<u64, ValidatorSchedule1Error> {
        if !self.active_keys.contains_key(&validator_id) {
            return Err(ValidatorSchedule1Error::MissingValidatorKeys(validator_id));
        }
        validate_keypair_disjoint(
            &new_keys,
            &self.active_keys,
            Some(validator_id),
            forbidden_actor_keys,
        )?;
        let effective_epoch = source_epoch.saturating_add(self.econ_policy.activation_delay_epochs);
        if self.schedules.contains_key(&effective_epoch) {
            return Err(
                ValidatorSchedule1Error::RejectedRetroactiveScheduleRewrite { effective_epoch },
            );
        }
        self.pending_rotations
            .insert((effective_epoch, validator_id), new_keys);
        Ok(effective_epoch)
    }

    /// Derives and commits the immutable schedule for `effective_epoch = source_epoch + delay`
    /// from committed `ECONOMIC-STATE-1` (`CE-15`) and `GOVERNANCE-1` roots.
    pub fn schedule_epoch_from_sources(
        &mut self,
        econ: &EconomicStateV1,
        gov: &GovernanceStateV1,
        source_epoch: u64,
        forbidden_actor_keys: &BTreeSet<CanonicalEd25519PublicKeyV1>,
    ) -> Result<EpochValidatorScheduleV1, ValidatorSchedule1Error> {
        if gov.lifecycle() == GovernanceLifecycleCodeV1::SafeHalted {
            return Err(ValidatorSchedule1Error::RejectedStaleOrHaltedSource(
                "cannot schedule new epoch while governance is SafeHalted",
            ));
        }
        if econ.current_epoch() > source_epoch {
            return Err(ValidatorSchedule1Error::RejectedStaleOrHaltedSource(
                "economic state epoch ahead of requested source_epoch",
            ));
        }
        let effective_epoch = source_epoch.saturating_add(self.econ_policy.activation_delay_epochs);
        if self.schedules.contains_key(&effective_epoch) {
            return Err(
                ValidatorSchedule1Error::RejectedRetroactiveScheduleRewrite { effective_epoch },
            );
        }
        let expected_next = self.latest_scheduled_epoch.saturating_add(1);
        if effective_epoch != expected_next {
            return Err(ValidatorSchedule1Error::RejectedNonContiguousEpoch {
                expected_effective_epoch: expected_next,
                actual_effective_epoch: effective_epoch,
            });
        }
        let mut staged = self.clone();
        // Apply any queued key rotations activating at `effective_epoch`.
        let rot_keys: Vec<_> = staged
            .pending_rotations
            .range(
                (effective_epoch, ActorIdV1::from_bytes([0; 32]))
                    ..=(effective_epoch, ActorIdV1::from_bytes([0xff; 32])),
            )
            .map(|(k, v)| (*k, *v))
            .collect();
        for ((_, vid), kp) in rot_keys {
            validate_keypair_disjoint(&kp, &staged.active_keys, Some(vid), forbidden_actor_keys)?;
            staged.active_keys.insert(vid, kp);
        }

        let econ_sha = econ.root_commitment()?.canonical_sha256()?;
        let gov_sha = gov.root_commitment().canonical_sha256()?;
        let eligible = econ.eligible_validators();
        let sched = build_epoch_schedule(
            effective_epoch,
            source_epoch,
            econ_sha,
            gov_sha,
            &eligible,
            &staged.active_keys,
            &staged.consensus_policy,
            &staged.econ_policy,
            forbidden_actor_keys,
        )?;
        let encoded = sched.encode_canonical()?;
        staged.trie.upsert(epoch_trie_key(effective_epoch), encoded);
        staged.schedules.insert(effective_epoch, sched.clone());
        staged.latest_scheduled_epoch = effective_epoch;
        if source_epoch > staged.current_epoch {
            staged.current_epoch = source_epoch;
        }
        *self = staged;
        Ok(sched)
    }

    /// Advances `current_epoch` forward to an already-scheduled epoch (forbids rollback or unscheduled epochs).
    pub fn advance_current_epoch(
        &mut self,
        next_epoch: u64,
    ) -> Result<(), ValidatorSchedule1Error> {
        if next_epoch < self.current_epoch {
            return Err(
                ValidatorSchedule1Error::RejectedRetroactiveScheduleRewrite {
                    effective_epoch: next_epoch,
                },
            );
        }
        if !self.schedules.contains_key(&next_epoch) {
            return Err(ValidatorSchedule1Error::RejectedNonContiguousEpoch {
                expected_effective_epoch: self.latest_scheduled_epoch,
                actual_effective_epoch: next_epoch,
            });
        }
        self.current_epoch = next_epoch;
        Ok(())
    }

    /// Looks up the immutable committed schedule for any historical or future scheduled `epoch`.
    pub fn schedule_at(&self, epoch: u64) -> Option<&EpochValidatorScheduleV1> {
        self.schedules.get(&epoch)
    }

    /// Generates an inclusion or absence proof for `epoch` in Domain 7.
    pub fn prove_epoch(
        &self,
        epoch: u64,
    ) -> Result<DomainPatriciaProofV1, ValidatorSchedule1Error> {
        Ok(self.trie.prove(&epoch_trie_key(epoch))?)
    }

    /// Verifies cold rebuild parity of the Domain 7 validator schedule Patricia trie.
    pub fn verify_rebuild_parity(&self) -> Result<(), ValidatorSchedule1Error> {
        Ok(self.trie.verify_rebuild_parity()?)
    }

    /// Returns the authenticated [`ValidatorScheduleRootV1`] snapshot at `current_epoch`.
    pub fn root_commitment(&self) -> Result<ValidatorScheduleRootV1, ValidatorSchedule1Error> {
        let cur =
            self.schedules
                .get(&self.current_epoch)
                .ok_or(ValidatorSchedule1Error::InvalidWire(
                    "missing current_epoch schedule",
                ))?;
        Ok(ValidatorScheduleRootV1 {
            root_version: 1,
            profile_id: self.profile_id,
            current_epoch: self.current_epoch,
            latest_scheduled_epoch: self.latest_scheduled_epoch,
            activation_delay_epochs: self.econ_policy.activation_delay_epochs,
            scheduled_epoch_count: self.schedules.len() as u32,
            active_validator_count: cur.validators.len() as u16,
            max_single_validator_bps: cur.max_single_validator_bps,
            schedule_trie_root: self.trie.root(),
        })
    }
}

fn validate_keypair_disjoint(
    kp: &ValidatorKeyPairV1,
    existing: &BTreeMap<ActorIdV1, ValidatorKeyPairV1>,
    ignore_id: Option<ActorIdV1>,
    forbidden_actor_keys: &BTreeSet<CanonicalEd25519PublicKeyV1>,
) -> Result<(), ValidatorSchedule1Error> {
    if kp.vote_key == kp.proposal_key {
        return Err(ValidatorSchedule1Error::RejectedKeyDomainCollision(
            "vote_key and proposal_key must be distinct",
        ));
    }
    if forbidden_actor_keys.contains(&kp.vote_key)
        || forbidden_actor_keys.contains(&kp.proposal_key)
    {
        return Err(ValidatorSchedule1Error::RejectedKeyDomainCollision(
            "validator key collides with actor action/admin key domain",
        ));
    }
    for (&vid, other) in existing {
        if Some(vid) == ignore_id {
            continue;
        }
        if other.vote_key == kp.vote_key
            || other.proposal_key == kp.vote_key
            || other.vote_key == kp.proposal_key
            || other.proposal_key == kp.proposal_key
        {
            return Err(ValidatorSchedule1Error::RejectedKeyDomainCollision(
                "validator key reused across validators",
            ));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn build_epoch_schedule(
    effective_epoch: u64,
    source_epoch: u64,
    economic_root_digest: [u8; 32],
    governance_root_digest: [u8; 32],
    eligible: &[(ActorIdV1, u128)],
    keys: &BTreeMap<ActorIdV1, ValidatorKeyPairV1>,
    consensus_policy: &ConsensusPolicyDescriptorV1,
    econ_policy: &ValidatorEconPolicyV1,
    forbidden_actor_keys: &BTreeSet<CanonicalEd25519PublicKeyV1>,
) -> Result<EpochValidatorScheduleV1, ValidatorSchedule1Error> {
    if eligible.len() < 4 || eligible.len() > usize::from(consensus_policy.max_validators) {
        return Err(ValidatorSchedule1Error::RejectedValidatorCountOutOfBounds);
    }
    let (eff_weights, total_effective_weight, max_single_validator_bps) =
        compute_effective_weights(eligible, econ_policy.max_single_validator_bps)?;
    let mut validators = Vec::with_capacity(eff_weights.len());
    for (vid, w) in eff_weights {
        let Some(&kp) = keys.get(&vid) else {
            return Err(ValidatorSchedule1Error::MissingValidatorKeys(vid));
        };
        if forbidden_actor_keys.contains(&kp.vote_key)
            || forbidden_actor_keys.contains(&kp.proposal_key)
        {
            return Err(ValidatorSchedule1Error::RejectedKeyDomainCollision(
                "validator key collides with actor key domain",
            ));
        }
        validators.push(ScheduledValidatorEntryV1 {
            validator_id: vid,
            vote_key: kp.vote_key,
            proposal_key: kp.proposal_key,
            effective_weight: w,
        });
    }
    let sched = EpochValidatorScheduleV1 {
        schedule_version: 1,
        effective_epoch,
        source_epoch,
        economic_root_digest,
        governance_root_digest,
        total_effective_weight,
        max_single_validator_bps,
        validators,
    };
    sched.validate()?;
    Ok(sched)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consensus_gov_d1::GovernancePolicyDescriptorV1;
    use crate::governance_1::GovernanceContextV1;
    use crate::identity_d1::{CredentialRoleV1, ThresholdPolicyV1};

    fn pk(seed: u8) -> CanonicalEd25519PublicKeyV1 {
        CanonicalEd25519PublicKeyV1::from_secret_seed(&[seed; 32]).unwrap()
    }

    #[test]
    fn validator_schedule_1_codec_delay_concentration_and_golden_tags() {
        let econ_policy = ValidatorEconPolicyV1::v1_pos_default();
        let cons_policy = ConsensusPolicyDescriptorV1::v1_bft_pos_default();
        let profile_id = [0x77; 32];
        let v: Vec<_> = (1u8..=4).map(|i| ActorIdV1::from_bytes([i; 32])).collect();
        let stake = 100_000_000_000u128;
        let mut econ = EconomicStateV1::from_genesis(
            econ_policy,
            profile_id,
            &[(v[0], stake), (v[1], stake), (v[2], stake), (v[3], stake)],
        )
        .unwrap();

        let mut c_keys = vec![pk(81), pk(82), pk(83)];
        c_keys.sort();
        let council = ThresholdPolicyV1 {
            keys: c_keys,
            threshold: 2,
            role: CredentialRoleV1::EmergencyPauseSigner,
        };
        let (q_n, q_d, tl_ep, mp_ep) = (2u16, 3u16, 21u64, 3u64);
        let flag = true;
        let gov_policy = GovernancePolicyDescriptorV1 {
            emergency_council: council,
            schema_version: 1,
            upgrade_quorum_numerator: q_n,
            upgrade_quorum_denominator: q_d,
            mandatory_exit_timelock_epochs: tl_ep,
            max_emergency_pause_epochs: mp_ep,
            automatic_safe_halt_on_proven_fault: flag,
            emergency_council_pause_only: flag,
            forbid_history_rewrite: flag,
        };
        let gov_ctx = GovernanceContextV1 {
            protocol_version: 1,
            chain_id: 77,
            genesis_id: [0x11; 32],
            launch_manifest_id: [0x22; 32],
            profile_id,
        };
        let gov = GovernanceStateV1::from_genesis(gov_ctx, gov_policy, &[(1, 50_000_000)]).unwrap();
        let init_keys: Vec<_> = (1u8..=4)
            .map(|i| {
                (
                    ActorIdV1::from_bytes([i; 32]),
                    ValidatorKeyPairV1 {
                        vote_key: pk(10 + i),
                        proposal_key: pk(20 + i),
                    },
                )
            })
            .collect();
        let forbidden = BTreeSet::new();

        let mut sched_state = ValidatorScheduleStateV1::from_genesis(
            cons_policy,
            econ_policy,
            profile_id,
            &econ,
            &gov,
            &init_keys,
            &forbidden,
        )
        .unwrap();
        let gen_root = sched_state.root_commitment().unwrap();
        assert_eq!(
            ValidatorScheduleRootV1::decode_canonical(&gen_root.encode_canonical().unwrap())
                .unwrap(),
            gen_root
        );

        // Epoch 0 -> schedules effective_epoch 2 with 5 validators (V5 joins)
        let v5 = ActorIdV1::from_bytes([5; 32]);
        econ.bond_stake(v5, stake, 0).unwrap();
        sched_state
            .register_validator_keys(
                v5,
                ValidatorKeyPairV1 {
                    vote_key: pk(15),
                    proposal_key: pk(25),
                },
                &forbidden,
            )
            .unwrap();
        let ep2 = sched_state
            .schedule_epoch_from_sources(&econ, &gov, 0, &forbidden)
            .unwrap();
        assert_eq!(ep2.effective_epoch, 2);
        assert_eq!(ep2.validators.len(), 5);
        assert_eq!(ep2.max_single_validator_bps, 2_000);
        assert_eq!(
            EpochValidatorScheduleV1::decode_canonical(&ep2.encode_canonical().unwrap()).unwrap(),
            ep2
        );
        let _ = ep2.to_validator_set().unwrap();
        sched_state.advance_current_epoch(2).unwrap();
        let post_root = sched_state.root_commitment().unwrap();
        sched_state.verify_rebuild_parity().unwrap();

        // Golden tags pinned against independent tools/validator_schedule_1_vectors.py
        assert_eq!(
            gen_root.tag().unwrap().to_string(),
            "#bb.receipt.sha256.3odndsylyhbqu3zinnag6r5zc7nveqfui2nmhcl6544elyfv56yq"
        );
        assert_eq!(
            ep2.tag().unwrap().to_string(),
            "#bb.receipt.sha256.jpxuitjyubnexfutl57v2ypfehtlyynd2r27zflm7otqih6hxmca"
        );
        assert_eq!(
            post_root.tag().unwrap().to_string(),
            "#bb.receipt.sha256.3ht2hivyqt4fijsvojhzyadzy3lzfajb3xlyy53jhboq4l7mbkta"
        );
    }
}
