//! `ECONOMIC-STATE-1`: deterministic authenticated bonded stake, integer-floor pro-rata epoch
//! rewards, 21-epoch unbonding queues, permanent equivocation jailing, and 100% slash-to-burn
//! across both active and unbonding stake over `DomainIdV1::EconomicStake` (`PS-040`).
//!
//! Satisfies `CE-15` / `CE-18` state-producer obligations bound to `VALIDATOR-ECON-D1` without
//! touching `crates/core/**` or `commit_state.rs`:
//! - **Authenticated validator stake map (`DomainIdV1::EconomicStake = 6`):** commits every
//!   validator's bonded stake, unbonding queue entries, accrued rewards, jail bit, and slash
//!   counter into `EconomicStateRootV1` via `AUTH-COMMIT-1`'s shared `DomainPatriciaTrieV1`.
//! - **100% slash-to-burn across active AND unbonding stake:** equivocation (`50%`) and
//!   inactivity (`1%`) slashes burn the exact basis-point fraction of BOTH active bonded stake
//!   and every pending unbonding entry, defeating slash-and-flee withdrawal races. Equivocation
//!   permanently jails the validator.
//! - **Integer-floor pro-rata epoch rewards:** capped at `max_epoch_reward_issuance`, computed via
//!   exact integer floor `floor(budget * bonded_i / total_eligible_bonded)` with zero hidden
//!   issuance (any remainder dust stays strictly unminted).
//! - **Closed accounting conservation:** every state transition and wire decode verifies
//!   `genesis_bonded_pool + cumulative_added_bonds + cumulative_issued_rewards ==
//!   total_active_bonded + total_unbonding + total_accrued_rewards + total_withdrawn +
//!   total_slashed_burned`.
//!
//! Committed economic state does not show that incentives are sufficient, participants are
//! independent, the asset has value, or consensus is safe or live.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use sha2::{Digest, Sha256};

use crate::asset_d1::{AssetEconError, BPS_DENOMINATOR, ValidatorEconPolicyV1};
use crate::auth_commit_1::{
    AuthCommit1Error, DomainIdV1, DomainPatriciaProofV1, DomainPatriciaTrieV1,
};
use crate::identity_d1::ActorIdV1;
use crate::protocol::{ContentTag, TagKind, hash_canonical};

/// Canonical schema string for an [`EconomicStateRootV1`] commitment (`ECONOMIC-STATE-1`).
pub const ECONOMIC_STATE_1_ROOT_SCHEMA: &[u8] = b"bitblock/economic-state-1/economic-root/v1";
/// Canonical schema string for an [`EconomicTransitionReceiptV1`] receipt (`ECONOMIC-STATE-1`).
pub const ECONOMIC_STATE_1_RECEIPT_SCHEMA: &[u8] = b"bitblock/economic-state-1/economic-receipt/v1";

/// Maximum concurrent pending unbonding entries tracked per validator.
pub const MAX_UNBONDING_ENTRIES_PER_VALIDATOR: usize = 64;

/// Fail-closed policy, codec, conservation, or Patricia error for `ECONOMIC-STATE-1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EconomicState1Error {
    /// Underlying `VALIDATOR-ECON-D1` policy validation error.
    Policy(AssetEconError),
    /// Underlying `AUTH-COMMIT-1` Patricia proof or rebuild error.
    AuthCommit(AuthCommit1Error),
    /// Canonical wire buffer had wrong length, version, discriminant, or trailing bytes.
    InvalidWire(&'static str),
    /// Genesis validator pool violated minimum count, uniqueness, or `min_self_stake`.
    InvalidGenesis(&'static str),
    /// Closed economic accounting invariant or checked `u128` arithmetic failed.
    ConservationViolation(&'static str),
}

impl From<AssetEconError> for EconomicState1Error {
    fn from(value: AssetEconError) -> Self {
        Self::Policy(value)
    }
}

impl From<AuthCommit1Error> for EconomicState1Error {
    fn from(value: AuthCommit1Error) -> Self {
        Self::AuthCommit(value)
    }
}

impl fmt::Display for EconomicState1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for EconomicState1Error {}

/// Action kind tag recorded inside an [`EconomicTransitionReceiptV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum EconomicActionKindV1 {
    /// Bond additional native stake to an active or new validator.
    BondStake = 1,
    /// Move bonded stake into a 21-epoch slashable unbonding queue entry.
    RequestUnbond = 2,
    /// Withdraw matured unbonding entries whose `unlock_epoch <= current_epoch`.
    WithdrawMatured = 3,
    /// Distribute bounded integer-floor pro-rata epoch rewards across eligible validators.
    DistributeRewards = 4,
    /// Apply 100% slash-to-burn equivocation penalty across active + unbonding stake and jail.
    EquivocationSlash = 5,
    /// Apply 100% slash-to-burn inactivity penalty across active + unbonding stake.
    InactivitySlash = 6,
}

impl EconomicActionKindV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte action-kind discriminant.
    pub const fn from_code(code: u8) -> Result<Self, EconomicState1Error> {
        match code {
            1 => Ok(Self::BondStake),
            2 => Ok(Self::RequestUnbond),
            3 => Ok(Self::WithdrawMatured),
            4 => Ok(Self::DistributeRewards),
            5 => Ok(Self::EquivocationSlash),
            6 => Ok(Self::InactivitySlash),
            _ => Err(EconomicState1Error::InvalidWire(
                "unknown EconomicActionKindV1",
            )),
        }
    }
}

/// Deterministic disposition recorded inside every [`EconomicTransitionReceiptV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum EconomicDispositionV1 {
    /// Transition verified, committed to Domain 6 Patricia trie, and passed closed conservation.
    Admitted = 0,
    /// Submitted epoch rolled back below `current_epoch`.
    RejectedInvalidEpoch = 1,
    /// Validator identity not found in the economic stake map.
    RejectedUnknownValidator = 2,
    /// Validator is permanently jailed after equivocation and cannot bond further stake.
    RejectedValidatorJailed = 3,
    /// New validator bond fell below `policy.min_self_stake`.
    RejectedBelowMinStake = 4,
    /// Requested bond/unbond amount was zero, exceeded bonded stake, or queue cap was hit.
    RejectedInvalidAmount = 5,
    /// No unbonding entries had reached `unlock_epoch <= epoch` (21-epoch timelock active).
    RejectedUnbondingNotMatured = 6,
    /// Reward budget was zero, exceeded `max_epoch_reward_issuance`, or had no eligible stake.
    RejectedRewardOverCap = 7,
    /// Slash evidence digest was all-zero / malformed.
    RejectedMalformedEvidence = 8,
    /// Reward epoch or slash evidence digest was already processed (replay prevented).
    RejectedDuplicateEpochOrEvidence = 9,
}

impl EconomicDispositionV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte disposition discriminant.
    pub const fn from_code(code: u8) -> Result<Self, EconomicState1Error> {
        match code {
            0 => Ok(Self::Admitted),
            1 => Ok(Self::RejectedInvalidEpoch),
            2 => Ok(Self::RejectedUnknownValidator),
            3 => Ok(Self::RejectedValidatorJailed),
            4 => Ok(Self::RejectedBelowMinStake),
            5 => Ok(Self::RejectedInvalidAmount),
            6 => Ok(Self::RejectedUnbondingNotMatured),
            7 => Ok(Self::RejectedRewardOverCap),
            8 => Ok(Self::RejectedMalformedEvidence),
            9 => Ok(Self::RejectedDuplicateEpochOrEvidence),
            _ => Err(EconomicState1Error::InvalidWire(
                "unknown EconomicDispositionV1",
            )),
        }
    }
}

/// One pending slashable unbonding queue entry bound to `unlock_epoch`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UnbondingEntryV1 {
    /// Remaining native stake locked in this unbonding entry (still slashable until withdrawal).
    pub amount: u128,
    /// Earliest epoch at which this entry matures and may be withdrawn (`request_epoch + delay`).
    pub unlock_epoch: u64,
}

/// Canonical per-validator record committed under `DomainIdV1::EconomicStake`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ValidatorStakeRecordV1 {
    /// Currently active bonded native stake.
    pub bonded_stake: u128,
    /// Cumulative accrued integer-floor epoch rewards.
    pub accrued_rewards: u128,
    /// Cumulative stake slashed and burned from this validator across active and unbonding funds.
    pub total_slashed: u128,
    /// `true` iff permanently jailed for proven equivocation.
    pub jailed: bool,
    /// Number of applied slash events.
    pub slash_count: u32,
    /// Pending slashable unbonding entries awaiting `unbonding_delay_epochs`.
    pub unbonding_entries: Vec<UnbondingEntryV1>,
}

impl ValidatorStakeRecordV1 {
    /// Encodes this validator stake record into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, EconomicState1Error> {
        if self.unbonding_entries.len() > MAX_UNBONDING_ENTRIES_PER_VALIDATOR {
            return Err(EconomicState1Error::InvalidWire(
                "unbonding_entries exceeds cap",
            ));
        }
        let mut out = Vec::with_capacity(55 + self.unbonding_entries.len() * 24);
        out.extend_from_slice(&self.bonded_stake.to_be_bytes());
        out.extend_from_slice(&self.accrued_rewards.to_be_bytes());
        out.extend_from_slice(&self.total_slashed.to_be_bytes());
        out.push(u8::from(self.jailed));
        out.extend_from_slice(&self.slash_count.to_be_bytes());
        out.extend_from_slice(&(self.unbonding_entries.len() as u16).to_be_bytes());
        for entry in &self.unbonding_entries {
            out.extend_from_slice(&entry.amount.to_be_bytes());
            out.extend_from_slice(&entry.unlock_epoch.to_be_bytes());
        }
        Ok(out)
    }

    /// Decodes a canonical [`ValidatorStakeRecordV1`] from wire bytes.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, EconomicState1Error> {
        if bytes.len() < 55 {
            return Err(EconomicState1Error::InvalidWire(
                "truncated ValidatorStakeRecordV1",
            ));
        }
        let jailed = match bytes[48] {
            0 => false,
            1 => true,
            _ => return Err(EconomicState1Error::InvalidWire("non-boolean jailed byte")),
        };
        let slash_count = u32::from_be_bytes(bytes[49..53].try_into().expect("4"));
        let u_len = u16::from_be_bytes([bytes[53], bytes[54]]) as usize;
        if u_len > MAX_UNBONDING_ENTRIES_PER_VALIDATOR || bytes.len() != 55 + u_len * 24 {
            return Err(EconomicState1Error::InvalidWire(
                "invalid ValidatorStakeRecordV1 unbonding length",
            ));
        }
        let mut unbonding_entries = Vec::with_capacity(u_len);
        for i in 0..u_len {
            let off = 55 + i * 24;
            unbonding_entries.push(UnbondingEntryV1 {
                amount: u128::from_be_bytes(bytes[off..off + 16].try_into().expect("16")),
                unlock_epoch: u64::from_be_bytes(bytes[off + 16..off + 24].try_into().expect("8")),
            });
        }
        Ok(Self {
            bonded_stake: u128::from_be_bytes(bytes[0..16].try_into().expect("16")),
            accrued_rewards: u128::from_be_bytes(bytes[16..32].try_into().expect("16")),
            total_slashed: u128::from_be_bytes(bytes[32..48].try_into().expect("16")),
            jailed,
            slash_count,
            unbonding_entries,
        })
    }
}

/// Authenticated commitment to closed economic stake, reward, unbonding, and burn accounting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EconomicStateRootV1 {
    /// Root schema version (`1`).
    pub root_version: u16,
    /// Bound `PROFILE-D1` profile identifier.
    pub profile_id: [u8; 32],
    /// Current committed economic epoch.
    pub current_epoch: u64,
    /// Total committed validator records in Domain 6.
    pub validator_count: u32,
    /// Count of non-jailed validators meeting `>= min_self_stake`.
    pub active_validator_count: u32,
    /// Sum of active bonded stake across all validators.
    pub total_active_bonded: u128,
    /// Sum of pending slashable unbonding stake across all validators.
    pub total_unbonding: u128,
    /// Sum of accrued integer-floor rewards across all validators.
    pub total_accrued_rewards: u128,
    /// Cumulative matured unbonding stake withdrawn.
    pub total_withdrawn: u128,
    /// Cumulative 100% slash-to-burn destroyed stake.
    pub total_slashed_burned: u128,
    /// Initial genesis bonded stake pool.
    pub genesis_bonded_pool: u128,
    /// Cumulative post-genesis bonded stake additions.
    pub cumulative_added_bonds: u128,
    /// Cumulative actually issued integer-floor epoch rewards (excludes unissued dust).
    pub cumulative_issued_rewards: u128,
    /// 32-byte Domain 6 Patricia root (`DomainIdV1::EconomicStake`).
    pub economic_trie_root: [u8; 32],
}

impl EconomicStateRootV1 {
    /// Fixed canonical big-endian wire length (`210` bytes).
    pub const WIRE_LEN: usize = 2 + 32 + 8 + 4 + 4 + 16 * 8 + 32;

    /// Verifies exact closed economic supply conservation across all 8 supply accumulators.
    pub fn check_conservation(&self) -> Result<(), EconomicState1Error> {
        let sources = self
            .genesis_bonded_pool
            .checked_add(self.cumulative_added_bonds)
            .and_then(|v| v.checked_add(self.cumulative_issued_rewards))
            .ok_or(EconomicState1Error::ConservationViolation(
                "economic sources sum overflow",
            ))?;
        let sinks = self
            .total_active_bonded
            .checked_add(self.total_unbonding)
            .and_then(|v| v.checked_add(self.total_accrued_rewards))
            .and_then(|v| v.checked_add(self.total_withdrawn))
            .and_then(|v| v.checked_add(self.total_slashed_burned))
            .ok_or(EconomicState1Error::ConservationViolation(
                "economic sinks sum overflow",
            ))?;
        if sources != sinks {
            return Err(EconomicState1Error::ConservationViolation(
                "economic supply conservation mismatch",
            ));
        }
        Ok(())
    }

    /// Encodes this economic root into 210 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], EconomicState1Error> {
        if self.root_version != 1 {
            return Err(EconomicState1Error::InvalidWire(
                "root_version must equal 1",
            ));
        }
        self.check_conservation()?;
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.root_version.to_be_bytes());
        out[2..34].copy_from_slice(&self.profile_id);
        out[34..42].copy_from_slice(&self.current_epoch.to_be_bytes());
        out[42..46].copy_from_slice(&self.validator_count.to_be_bytes());
        out[46..50].copy_from_slice(&self.active_validator_count.to_be_bytes());
        out[50..66].copy_from_slice(&self.total_active_bonded.to_be_bytes());
        out[66..82].copy_from_slice(&self.total_unbonding.to_be_bytes());
        out[82..98].copy_from_slice(&self.total_accrued_rewards.to_be_bytes());
        out[98..114].copy_from_slice(&self.total_withdrawn.to_be_bytes());
        out[114..130].copy_from_slice(&self.total_slashed_burned.to_be_bytes());
        out[130..146].copy_from_slice(&self.genesis_bonded_pool.to_be_bytes());
        out[146..162].copy_from_slice(&self.cumulative_added_bonds.to_be_bytes());
        out[162..178].copy_from_slice(&self.cumulative_issued_rewards.to_be_bytes());
        out[178..210].copy_from_slice(&self.economic_trie_root);
        Ok(out)
    }

    /// Decodes and conservation-checks a 210-byte canonical [`EconomicStateRootV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, EconomicState1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(EconomicState1Error::InvalidWire(
                "EconomicStateRootV1 must be 210 bytes",
            ));
        }
        let root = Self {
            root_version: u16::from_be_bytes([bytes[0], bytes[1]]),
            profile_id: bytes[2..34].try_into().expect("32"),
            current_epoch: u64::from_be_bytes(bytes[34..42].try_into().expect("8")),
            validator_count: u32::from_be_bytes(bytes[42..46].try_into().expect("4")),
            active_validator_count: u32::from_be_bytes(bytes[46..50].try_into().expect("4")),
            total_active_bonded: u128::from_be_bytes(bytes[50..66].try_into().expect("16")),
            total_unbonding: u128::from_be_bytes(bytes[66..82].try_into().expect("16")),
            total_accrued_rewards: u128::from_be_bytes(bytes[82..98].try_into().expect("16")),
            total_withdrawn: u128::from_be_bytes(bytes[98..114].try_into().expect("16")),
            total_slashed_burned: u128::from_be_bytes(bytes[114..130].try_into().expect("16")),
            genesis_bonded_pool: u128::from_be_bytes(bytes[130..146].try_into().expect("16")),
            cumulative_added_bonds: u128::from_be_bytes(bytes[146..162].try_into().expect("16")),
            cumulative_issued_rewards: u128::from_be_bytes(bytes[162..178].try_into().expect("16")),
            economic_trie_root: bytes[178..210].try_into().expect("32"),
        };
        let _ = root.encode_canonical()?;
        Ok(root)
    }

    /// Computes SHA-256 of canonical encoded economic root bytes.
    pub fn canonical_sha256(&self) -> Result<[u8; 32], EconomicState1Error> {
        Ok(Sha256::digest(self.encode_canonical()?).into())
    }

    /// Computes the canonical `Receipt` `ContentTag` for this economic state root.
    pub fn tag(&self) -> Result<ContentTag, EconomicState1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            ECONOMIC_STATE_1_ROOT_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

/// Deterministic proof-carrying receipt emitted for every `ECONOMIC-STATE-1` transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EconomicTransitionReceiptV1 {
    /// Receipt schema version (`1`).
    pub receipt_version: u16,
    /// Bound `PROFILE-D1` profile identifier.
    pub profile_id: [u8; 32],
    /// Epoch at which the transition was evaluated.
    pub epoch: u64,
    /// Economic action kind evaluated.
    pub action_kind: EconomicActionKindV1,
    /// Deterministic disposition (`Admitted` or specific fail-closed rejection).
    pub disposition: EconomicDispositionV1,
    /// Primary economic amount delta (bonded, unbonded, withdrawn, issued, or slashed-burned).
    pub amount_delta: u128,
    /// Unissued integer-floor remainder dust for reward distributions (`0` for non-reward ops).
    pub dust_unissued: u128,
    /// Validator actor ID or slash evidence digest bound by this receipt.
    pub subject_digest: [u8; 32],
    /// SHA-256 of canonical pre-transition [`EconomicStateRootV1`].
    pub pre_root_sha256: [u8; 32],
    /// SHA-256 of canonical post-transition [`EconomicStateRootV1`].
    pub post_root_sha256: [u8; 32],
}

impl EconomicTransitionReceiptV1 {
    /// Fixed canonical big-endian wire length (`172` bytes).
    pub const WIRE_LEN: usize = 2 + 32 + 8 + 1 + 1 + 16 + 16 + 32 + 32 + 32;

    /// Encodes this economic transition receipt into 172 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], EconomicState1Error> {
        if self.receipt_version != 1 {
            return Err(EconomicState1Error::InvalidWire(
                "receipt_version must equal 1",
            ));
        }
        let mut buf = Vec::with_capacity(Self::WIRE_LEN);
        buf.extend_from_slice(&self.receipt_version.to_be_bytes());
        buf.extend_from_slice(&self.profile_id);
        buf.extend_from_slice(&self.epoch.to_be_bytes());
        buf.push(self.action_kind.code());
        buf.push(self.disposition.code());
        buf.extend_from_slice(&self.amount_delta.to_be_bytes());
        buf.extend_from_slice(&self.dust_unissued.to_be_bytes());
        buf.extend_from_slice(&self.subject_digest);
        buf.extend_from_slice(&self.pre_root_sha256);
        buf.extend_from_slice(&self.post_root_sha256);
        Ok(buf.try_into().expect("172"))
    }

    /// Decodes a 172-byte canonical [`EconomicTransitionReceiptV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, EconomicState1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(EconomicState1Error::InvalidWire(
                "EconomicTransitionReceiptV1 must be 172 bytes",
            ));
        }
        let receipt_version = u16::from_be_bytes([bytes[0], bytes[1]]);
        if receipt_version != 1 {
            return Err(EconomicState1Error::InvalidWire(
                "receipt_version must equal 1",
            ));
        }
        Ok(Self {
            receipt_version,
            profile_id: bytes[2..34].try_into().expect("32"),
            epoch: u64::from_be_bytes(bytes[34..42].try_into().expect("8")),
            action_kind: EconomicActionKindV1::from_code(bytes[42])?,
            disposition: EconomicDispositionV1::from_code(bytes[43])?,
            amount_delta: u128::from_be_bytes(bytes[44..60].try_into().expect("16")),
            dust_unissued: u128::from_be_bytes(bytes[60..76].try_into().expect("16")),
            subject_digest: bytes[76..108].try_into().expect("32"),
            pre_root_sha256: bytes[108..140].try_into().expect("32"),
            post_root_sha256: bytes[140..172].try_into().expect("32"),
        })
    }

    /// Computes the canonical `Receipt` `ContentTag` for this transition receipt.
    pub fn tag(&self) -> Result<ContentTag, EconomicState1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            ECONOMIC_STATE_1_RECEIPT_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

/// Deterministic authenticated staking, reward, unbonding, and slashing state (`Domain 6`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EconomicStateV1 {
    policy: ValidatorEconPolicyV1,
    profile_id: [u8; 32],
    current_epoch: u64,
    records: BTreeMap<ActorIdV1, ValidatorStakeRecordV1>,
    rewarded_epochs: BTreeSet<u64>,
    seen_slash_evidence: BTreeSet<[u8; 32]>,
    genesis_bonded_pool: u128,
    cumulative_added_bonds: u128,
    cumulative_issued_rewards: u128,
    total_withdrawn: u128,
    total_slashed_burned: u128,
    trie: DomainPatriciaTrieV1,
}

impl EconomicStateV1 {
    /// Initializes genesis bonded PoS state under `policy` and commits Domain 6 Patricia root.
    pub fn from_genesis(
        policy: ValidatorEconPolicyV1,
        profile_id: [u8; 32],
        genesis_stakes: &[(ActorIdV1, u128)],
    ) -> Result<Self, EconomicState1Error> {
        policy.validate()?;
        if genesis_stakes.len() < 4 {
            return Err(EconomicState1Error::InvalidGenesis(
                "BFT PoS requires at least 4 distinct genesis validators",
            ));
        }
        let mut records = BTreeMap::new();
        let mut pairs = Vec::with_capacity(genesis_stakes.len());
        let mut genesis_bonded_pool = 0u128;
        for &(vid, stake) in genesis_stakes {
            if stake < policy.min_self_stake {
                return Err(EconomicState1Error::InvalidGenesis(
                    "genesis validator stake below min_self_stake",
                ));
            }
            let rec = ValidatorStakeRecordV1 {
                bonded_stake: stake,
                accrued_rewards: 0,
                total_slashed: 0,
                jailed: false,
                slash_count: 0,
                unbonding_entries: Vec::new(),
            };
            if records.insert(vid, rec.clone()).is_some() {
                return Err(EconomicState1Error::InvalidGenesis(
                    "duplicate validator in genesis_stakes",
                ));
            }
            genesis_bonded_pool = genesis_bonded_pool
                .checked_add(stake)
                .ok_or(EconomicState1Error::ConservationViolation("pool overflow"))?;
            pairs.push((*vid.as_bytes(), rec.encode_canonical()?));
        }
        let trie = DomainPatriciaTrieV1::from_entries(DomainIdV1::EconomicStake, pairs)?;
        let state = Self {
            policy,
            profile_id,
            current_epoch: 0,
            records,
            rewarded_epochs: BTreeSet::new(),
            seen_slash_evidence: BTreeSet::new(),
            genesis_bonded_pool,
            cumulative_added_bonds: 0,
            cumulative_issued_rewards: 0,
            total_withdrawn: 0,
            total_slashed_burned: 0,
            trie,
        };
        state.verify_conservation()?;
        Ok(state)
    }

    /// Returns the bound `VALIDATOR-ECON-D1` policy descriptor.
    pub const fn policy(&self) -> ValidatorEconPolicyV1 {
        self.policy
    }

    /// Returns the current committed epoch.
    pub const fn current_epoch(&self) -> u64 {
        self.current_epoch
    }

    /// Looks up a validator's committed stake/unbonding/jail record.
    pub fn validator_record(&self, validator_id: &ActorIdV1) -> Option<&ValidatorStakeRecordV1> {
        self.records.get(validator_id)
    }

    /// Returns all currently eligible non-jailed validators meeting `>= min_self_stake`
    /// in canonical ascending `ActorIdV1` order (`CE-15` input to `VALIDATOR-SCHEDULE-1`).
    pub fn eligible_validators(&self) -> Vec<(ActorIdV1, u128)> {
        self.records
            .iter()
            .filter(|(_, r)| !r.jailed && r.bonded_stake >= self.policy.min_self_stake)
            .map(|(id, r)| (*id, r.bonded_stake))
            .collect()
    }

    /// Returns the authenticated [`EconomicStateRootV1`] and checks closed conservation.
    pub fn root_commitment(&self) -> Result<EconomicStateRootV1, EconomicState1Error> {
        let (mut active_bonded, mut unbonding, mut accrued) = (0u128, 0u128, 0u128);
        let mut active_validator_count = 0u32;
        for rec in self.records.values() {
            active_bonded = active_bonded.checked_add(rec.bonded_stake).ok_or(
                EconomicState1Error::ConservationViolation("bonded overflow"),
            )?;
            accrued = accrued.checked_add(rec.accrued_rewards).ok_or(
                EconomicState1Error::ConservationViolation("reward overflow"),
            )?;
            for entry in &rec.unbonding_entries {
                unbonding = unbonding.checked_add(entry.amount).ok_or(
                    EconomicState1Error::ConservationViolation("unbonding overflow"),
                )?;
            }
            if !rec.jailed && rec.bonded_stake >= self.policy.min_self_stake {
                active_validator_count += 1;
            }
        }
        let root = EconomicStateRootV1 {
            root_version: 1,
            profile_id: self.profile_id,
            current_epoch: self.current_epoch,
            validator_count: self.records.len() as u32,
            active_validator_count,
            total_active_bonded: active_bonded,
            total_unbonding: unbonding,
            total_accrued_rewards: accrued,
            total_withdrawn: self.total_withdrawn,
            total_slashed_burned: self.total_slashed_burned,
            genesis_bonded_pool: self.genesis_bonded_pool,
            cumulative_added_bonds: self.cumulative_added_bonds,
            cumulative_issued_rewards: self.cumulative_issued_rewards,
            economic_trie_root: self.trie.root(),
        };
        root.check_conservation()?;
        Ok(root)
    }

    /// Generates an inclusion or absence proof for `validator_id` in Domain 6.
    pub fn prove_validator(
        &self,
        validator_id: &ActorIdV1,
    ) -> Result<DomainPatriciaProofV1, EconomicState1Error> {
        Ok(self.trie.prove(validator_id.as_bytes())?)
    }

    /// Verifies cold rebuild parity of the Domain 6 economic stake Patricia trie.
    pub fn verify_rebuild_parity(&self) -> Result<(), EconomicState1Error> {
        Ok(self.trie.verify_rebuild_parity()?)
    }

    /// Verifies exact closed economic supply conservation.
    pub fn verify_conservation(&self) -> Result<(), EconomicState1Error> {
        let _ = self.root_commitment()?;
        Ok(())
    }

    fn emit_receipt(
        &self,
        epoch: u64,
        action_kind: EconomicActionKindV1,
        disposition: EconomicDispositionV1,
        deltas: (u128, u128),
        subject_digest: [u8; 32],
        pre_sha: [u8; 32],
    ) -> Result<EconomicTransitionReceiptV1, EconomicState1Error> {
        let post_sha = self.root_commitment()?.canonical_sha256()?;
        Ok(EconomicTransitionReceiptV1 {
            receipt_version: 1,
            profile_id: self.profile_id,
            epoch,
            action_kind,
            disposition,
            amount_delta: deltas.0,
            dust_unissued: deltas.1,
            subject_digest,
            pre_root_sha256: pre_sha,
            post_root_sha256: post_sha,
        })
    }

    /// Bonds additional stake to an existing or newly joining validator (rejects jailed validators).
    pub fn bond_stake(
        &mut self,
        validator_id: ActorIdV1,
        amount: u128,
        epoch: u64,
    ) -> Result<EconomicTransitionReceiptV1, EconomicState1Error> {
        let pre_sha = self.root_commitment()?.canonical_sha256()?;
        let subj = *validator_id.as_bytes();
        if epoch < self.current_epoch {
            return self.emit_receipt(
                epoch,
                EconomicActionKindV1::BondStake,
                EconomicDispositionV1::RejectedInvalidEpoch,
                (0, 0),
                subj,
                pre_sha,
            );
        }
        if amount == 0 {
            return self.emit_receipt(
                epoch,
                EconomicActionKindV1::BondStake,
                EconomicDispositionV1::RejectedInvalidAmount,
                (0, 0),
                subj,
                pre_sha,
            );
        }
        let mut staged = self.clone();
        let rec = staged
            .records
            .entry(validator_id)
            .or_insert_with(|| ValidatorStakeRecordV1 {
                bonded_stake: 0,
                accrued_rewards: 0,
                total_slashed: 0,
                jailed: false,
                slash_count: 0,
                unbonding_entries: Vec::new(),
            });
        if rec.jailed {
            return self.emit_receipt(
                epoch,
                EconomicActionKindV1::BondStake,
                EconomicDispositionV1::RejectedValidatorJailed,
                (0, 0),
                subj,
                pre_sha,
            );
        }
        let next_bonded = rec
            .bonded_stake
            .checked_add(amount)
            .ok_or(EconomicState1Error::ConservationViolation("bond overflow"))?;
        if next_bonded < staged.policy.min_self_stake {
            return self.emit_receipt(
                epoch,
                EconomicActionKindV1::BondStake,
                EconomicDispositionV1::RejectedBelowMinStake,
                (0, 0),
                subj,
                pre_sha,
            );
        }
        rec.bonded_stake = next_bonded;
        let enc = rec.encode_canonical()?;
        staged.cumulative_added_bonds = staged.cumulative_added_bonds.checked_add(amount).ok_or(
            EconomicState1Error::ConservationViolation("added bonds overflow"),
        )?;
        staged.current_epoch = epoch;
        staged.trie.upsert(subj, enc);
        staged.verify_conservation()?;
        *self = staged;
        self.emit_receipt(
            epoch,
            EconomicActionKindV1::BondStake,
            EconomicDispositionV1::Admitted,
            (amount, 0),
            subj,
            pre_sha,
        )
    }

    /// Requests unbonding of `amount` from `validator_id`, locking it for `unbonding_delay_epochs`.
    pub fn request_unbond(
        &mut self,
        validator_id: ActorIdV1,
        amount: u128,
        epoch: u64,
    ) -> Result<EconomicTransitionReceiptV1, EconomicState1Error> {
        let pre_sha = self.root_commitment()?.canonical_sha256()?;
        let subj = *validator_id.as_bytes();
        if epoch < self.current_epoch {
            return self.emit_receipt(
                epoch,
                EconomicActionKindV1::RequestUnbond,
                EconomicDispositionV1::RejectedInvalidEpoch,
                (0, 0),
                subj,
                pre_sha,
            );
        }
        let mut staged = self.clone();
        let Some(rec) = staged.records.get_mut(&validator_id) else {
            return self.emit_receipt(
                epoch,
                EconomicActionKindV1::RequestUnbond,
                EconomicDispositionV1::RejectedUnknownValidator,
                (0, 0),
                subj,
                pre_sha,
            );
        };
        if amount == 0
            || amount > rec.bonded_stake
            || rec.unbonding_entries.len() >= MAX_UNBONDING_ENTRIES_PER_VALIDATOR
        {
            return self.emit_receipt(
                epoch,
                EconomicActionKindV1::RequestUnbond,
                EconomicDispositionV1::RejectedInvalidAmount,
                (0, 0),
                subj,
                pre_sha,
            );
        }
        rec.bonded_stake -= amount;
        rec.unbonding_entries.push(UnbondingEntryV1 {
            amount,
            unlock_epoch: epoch.saturating_add(staged.policy.unbonding_delay_epochs),
        });
        let enc = rec.encode_canonical()?;
        staged.current_epoch = epoch;
        staged.trie.upsert(subj, enc);
        staged.verify_conservation()?;
        *self = staged;
        self.emit_receipt(
            epoch,
            EconomicActionKindV1::RequestUnbond,
            EconomicDispositionV1::Admitted,
            (amount, 0),
            subj,
            pre_sha,
        )
    }

    /// Withdraws all matured unbonding entries (`unlock_epoch <= epoch`) for `validator_id`.
    pub fn withdraw_matured_unbond(
        &mut self,
        validator_id: ActorIdV1,
        epoch: u64,
    ) -> Result<EconomicTransitionReceiptV1, EconomicState1Error> {
        let pre_sha = self.root_commitment()?.canonical_sha256()?;
        let subj = *validator_id.as_bytes();
        if epoch < self.current_epoch {
            return self.emit_receipt(
                epoch,
                EconomicActionKindV1::WithdrawMatured,
                EconomicDispositionV1::RejectedInvalidEpoch,
                (0, 0),
                subj,
                pre_sha,
            );
        }
        let mut staged = self.clone();
        let Some(rec) = staged.records.get_mut(&validator_id) else {
            return self.emit_receipt(
                epoch,
                EconomicActionKindV1::WithdrawMatured,
                EconomicDispositionV1::RejectedUnknownValidator,
                (0, 0),
                subj,
                pre_sha,
            );
        };
        let mut matured_sum = 0u128;
        let mut remaining = Vec::with_capacity(rec.unbonding_entries.len());
        for entry in &rec.unbonding_entries {
            if entry.unlock_epoch <= epoch {
                matured_sum = matured_sum.checked_add(entry.amount).ok_or(
                    EconomicState1Error::ConservationViolation("withdraw overflow"),
                )?;
            } else {
                remaining.push(*entry);
            }
        }
        if matured_sum == 0 {
            return self.emit_receipt(
                epoch,
                EconomicActionKindV1::WithdrawMatured,
                EconomicDispositionV1::RejectedUnbondingNotMatured,
                (0, 0),
                subj,
                pre_sha,
            );
        }
        rec.unbonding_entries = remaining;
        let enc = rec.encode_canonical()?;
        staged.total_withdrawn = staged.total_withdrawn.checked_add(matured_sum).ok_or(
            EconomicState1Error::ConservationViolation("total_withdrawn overflow"),
        )?;
        staged.current_epoch = epoch;
        staged.trie.upsert(subj, enc);
        staged.verify_conservation()?;
        *self = staged;
        self.emit_receipt(
            epoch,
            EconomicActionKindV1::WithdrawMatured,
            EconomicDispositionV1::Admitted,
            (matured_sum, 0),
            subj,
            pre_sha,
        )
    }

    /// Distributes bounded integer-floor pro-rata epoch rewards across eligible active validators
    /// (`budget <= policy.max_epoch_reward_issuance`; remainder dust stays strictly unminted).
    pub fn distribute_epoch_rewards(
        &mut self,
        budget: u128,
        epoch: u64,
    ) -> Result<EconomicTransitionReceiptV1, EconomicState1Error> {
        let pre_sha = self.root_commitment()?.canonical_sha256()?;
        let subj: [u8; 32] = Sha256::digest(epoch.to_be_bytes()).into();
        if epoch < self.current_epoch {
            return self.emit_receipt(
                epoch,
                EconomicActionKindV1::DistributeRewards,
                EconomicDispositionV1::RejectedInvalidEpoch,
                (0, 0),
                subj,
                pre_sha,
            );
        }
        if self.rewarded_epochs.contains(&epoch) {
            return self.emit_receipt(
                epoch,
                EconomicActionKindV1::DistributeRewards,
                EconomicDispositionV1::RejectedDuplicateEpochOrEvidence,
                (0, 0),
                subj,
                pre_sha,
            );
        }
        if budget == 0 || budget > self.policy.max_epoch_reward_issuance {
            return self.emit_receipt(
                epoch,
                EconomicActionKindV1::DistributeRewards,
                EconomicDispositionV1::RejectedRewardOverCap,
                (0, 0),
                subj,
                pre_sha,
            );
        }
        let eligible = self.eligible_validators();
        let mut total_eligible = 0u128;
        for &(_, stake) in &eligible {
            total_eligible = total_eligible.checked_add(stake).ok_or(
                EconomicState1Error::ConservationViolation("eligible stake overflow"),
            )?;
        }
        if total_eligible == 0 {
            return self.emit_receipt(
                epoch,
                EconomicActionKindV1::DistributeRewards,
                EconomicDispositionV1::RejectedRewardOverCap,
                (0, 0),
                subj,
                pre_sha,
            );
        }
        let mut staged = self.clone();
        let mut issued_sum = 0u128;
        for &(vid, stake) in &eligible {
            let share =
                budget
                    .checked_mul(stake)
                    .ok_or(EconomicState1Error::ConservationViolation(
                        "reward mul overflow",
                    ))?
                    / total_eligible;
            if share > 0 {
                let rec = staged.records.get_mut(&vid).expect("eligible exists");
                rec.accrued_rewards = rec.accrued_rewards.checked_add(share).ok_or(
                    EconomicState1Error::ConservationViolation("accrued reward overflow"),
                )?;
                let enc = rec.encode_canonical()?;
                staged.trie.upsert(*vid.as_bytes(), enc);
                issued_sum = issued_sum.checked_add(share).ok_or(
                    EconomicState1Error::ConservationViolation("issued sum overflow"),
                )?;
            }
        }
        let dust = budget - issued_sum;
        staged.cumulative_issued_rewards = staged
            .cumulative_issued_rewards
            .checked_add(issued_sum)
            .ok_or(EconomicState1Error::ConservationViolation(
                "cumulative rewards overflow",
            ))?;
        staged.rewarded_epochs.insert(epoch);
        staged.current_epoch = epoch;
        staged.verify_conservation()?;
        *self = staged;
        self.emit_receipt(
            epoch,
            EconomicActionKindV1::DistributeRewards,
            EconomicDispositionV1::Admitted,
            (issued_sum, dust),
            subj,
            pre_sha,
        )
    }

    fn apply_slash_internal(
        &mut self,
        validator_id: ActorIdV1,
        evidence_digest: [u8; 32],
        epoch: u64,
        is_equivocation: bool,
    ) -> Result<EconomicTransitionReceiptV1, EconomicState1Error> {
        let action_kind = if is_equivocation {
            EconomicActionKindV1::EquivocationSlash
        } else {
            EconomicActionKindV1::InactivitySlash
        };
        let pre_sha = self.root_commitment()?.canonical_sha256()?;
        if epoch < self.current_epoch {
            return self.emit_receipt(
                epoch,
                action_kind,
                EconomicDispositionV1::RejectedInvalidEpoch,
                (0, 0),
                evidence_digest,
                pre_sha,
            );
        }
        if evidence_digest == [0u8; 32] {
            return self.emit_receipt(
                epoch,
                action_kind,
                EconomicDispositionV1::RejectedMalformedEvidence,
                (0, 0),
                evidence_digest,
                pre_sha,
            );
        }
        if self.seen_slash_evidence.contains(&evidence_digest) {
            return self.emit_receipt(
                epoch,
                action_kind,
                EconomicDispositionV1::RejectedDuplicateEpochOrEvidence,
                (0, 0),
                evidence_digest,
                pre_sha,
            );
        }
        let mut staged = self.clone();
        let Some(rec) = staged.records.get_mut(&validator_id) else {
            return self.emit_receipt(
                epoch,
                action_kind,
                EconomicDispositionV1::RejectedUnknownValidator,
                (0, 0),
                evidence_digest,
                pre_sha,
            );
        };
        let bps = u128::from(if is_equivocation {
            staged.policy.equivocation_slash_bps
        } else {
            staged.policy.inactivity_slash_bps
        });
        let denom = u128::from(BPS_DENOMINATOR);
        let active_slash =
            rec.bonded_stake
                .checked_mul(bps)
                .ok_or(EconomicState1Error::ConservationViolation(
                    "slash mul overflow",
                ))?
                / denom;
        rec.bonded_stake -= active_slash;
        let mut total_slash = active_slash;
        for entry in &mut rec.unbonding_entries {
            let u_slash =
                entry
                    .amount
                    .checked_mul(bps)
                    .ok_or(EconomicState1Error::ConservationViolation(
                        "unbonding slash overflow",
                    ))?
                    / denom;
            entry.amount -= u_slash;
            total_slash = total_slash.checked_add(u_slash).ok_or(
                EconomicState1Error::ConservationViolation("slash sum overflow"),
            )?;
        }
        rec.unbonding_entries.retain(|e| e.amount > 0);
        rec.total_slashed = rec.total_slashed.checked_add(total_slash).ok_or(
            EconomicState1Error::ConservationViolation("record slash overflow"),
        )?;
        rec.slash_count = rec.slash_count.saturating_add(1);
        if is_equivocation {
            rec.jailed = true;
        }
        let enc = rec.encode_canonical()?;
        staged.total_slashed_burned = staged.total_slashed_burned.checked_add(total_slash).ok_or(
            EconomicState1Error::ConservationViolation("total_slashed_burned overflow"),
        )?;
        staged.seen_slash_evidence.insert(evidence_digest);
        staged.current_epoch = epoch;
        staged.trie.upsert(*validator_id.as_bytes(), enc);
        staged.verify_conservation()?;
        *self = staged;
        self.emit_receipt(
            epoch,
            action_kind,
            EconomicDispositionV1::Admitted,
            (total_slash, 0),
            evidence_digest,
            pre_sha,
        )
    }

    /// Applies 100% slash-to-burn equivocation penalty across BOTH active & unbonding stake and
    /// permanently jails `validator_id`.
    pub fn apply_equivocation_slash(
        &mut self,
        validator_id: ActorIdV1,
        evidence_digest: [u8; 32],
        epoch: u64,
    ) -> Result<EconomicTransitionReceiptV1, EconomicState1Error> {
        self.apply_slash_internal(validator_id, evidence_digest, epoch, true)
    }

    /// Applies 100% slash-to-burn inactivity penalty across active & unbonding stake.
    pub fn apply_inactivity_slash(
        &mut self,
        validator_id: ActorIdV1,
        evidence_digest: [u8; 32],
        epoch: u64,
    ) -> Result<EconomicTransitionReceiptV1, EconomicState1Error> {
        self.apply_slash_internal(validator_id, evidence_digest, epoch, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn economic_state_1_codec_conservation_slash_and_golden_tags() {
        let policy = ValidatorEconPolicyV1::v1_pos_default();
        let profile_id = [0x66; 32];
        let v1 = ActorIdV1::from_bytes([1; 32]);
        let v2 = ActorIdV1::from_bytes([2; 32]);
        let v3 = ActorIdV1::from_bytes([3; 32]);
        let v4 = ActorIdV1::from_bytes([4; 32]);
        let stake = 100_000_000_000u128;
        let mut st = EconomicStateV1::from_genesis(
            policy,
            profile_id,
            &[(v1, stake), (v2, stake), (v3, stake), (v4, stake)],
        )
        .unwrap();

        // 1. Rewards at epoch 1 with budget=999_999_999 (dust = 3 units unissued!)
        let r_rew = st.distribute_epoch_rewards(999_999_999, 1).unwrap();
        assert_eq!(r_rew.disposition, EconomicDispositionV1::Admitted);
        assert_eq!(r_rew.amount_delta, 999_999_996);
        assert_eq!(r_rew.dust_unissued, 3);
        assert_eq!(
            EconomicTransitionReceiptV1::decode_canonical(&r_rew.encode_canonical().unwrap())
                .unwrap(),
            r_rew
        );

        // 2. V1 bonds +10B at epoch 1; V3 unbonds 20B at epoch 2 (unlock_epoch=23)
        assert_eq!(
            st.bond_stake(v1, 10_000_000_000, 1).unwrap().disposition,
            EconomicDispositionV1::Admitted
        );
        assert_eq!(
            st.request_unbond(v3, 20_000_000_000, 2)
                .unwrap()
                .disposition,
            EconomicDispositionV1::Admitted
        );

        // 3. V3 equivocates at epoch 5 -> 50% burned from active (80B->40B) + unbonding (20B->10B)
        let r_slash = st.apply_equivocation_slash(v3, [0xE1; 32], 5).unwrap();
        assert_eq!(r_slash.disposition, EconomicDispositionV1::Admitted);
        assert_eq!(r_slash.amount_delta, 50_000_000_000);
        let slash_root = st.root_commitment().unwrap();
        assert_eq!(
            EconomicStateRootV1::decode_canonical(&slash_root.encode_canonical().unwrap()).unwrap(),
            slash_root
        );

        // 4. V3 matured withdraw at epoch 23 -> releases 10B remaining post-slash unbonding
        let r_wd = st.withdraw_matured_unbond(v3, 23).unwrap();
        assert_eq!(r_wd.disposition, EconomicDispositionV1::Admitted);
        assert_eq!(r_wd.amount_delta, 10_000_000_000);
        let final_root = st.root_commitment().unwrap();
        st.verify_rebuild_parity().unwrap();
        st.verify_conservation().unwrap();

        let p_v3 = st.prove_validator(&v3).unwrap();
        let v3_bytes = p_v3
            .verify(
                DomainIdV1::EconomicStake,
                final_root.economic_trie_root,
                v3.as_bytes(),
            )
            .unwrap()
            .expect("v3 present");
        let dec_v3 = ValidatorStakeRecordV1::decode_canonical(&v3_bytes).unwrap();
        assert!(dec_v3.jailed);
        assert_eq!(dec_v3.bonded_stake, 40_000_000_000);
        assert_eq!(dec_v3.total_slashed, 50_000_000_000);

        // Golden tags pinned against independent tools/economic_state_1_vectors.py
        assert_eq!(
            r_rew.tag().unwrap().to_string(),
            "#bb.receipt.sha256.r2jxjtxll4lptlqmd7d5gkbvlzphwe2m2rkb5rlr364sjqq43ndq"
        );
        assert_eq!(
            slash_root.tag().unwrap().to_string(),
            "#bb.receipt.sha256.x4sivbvgyjbvy7od5mxdfoiptaywamcdg42754yayqdqnjpaes7q"
        );
        assert_eq!(
            final_root.tag().unwrap().to_string(),
            "#bb.receipt.sha256.kc7wgdidj5xxkz7v345cskrfi6x6vpfs6fbije4pdhv3m5wysreq"
        );
        assert_eq!(
            r_slash.tag().unwrap().to_string(),
            "#bb.receipt.sha256.kzjsjcb6un22a2u45uyci43gky5bh7o7r3h6xpxzv5aisfx5o7xa"
        );
    }
}
