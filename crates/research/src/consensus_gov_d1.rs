//! `CONSENSUS-D1` and `GOVERNANCE-D1`: deterministic-finality PoS BFT quorum certificates,
//! full-possession DA/retention policy, timelocked stake-weighted governance, automatic safe-halt,
//! and pause-only emergency council authority.
//!
//! Freezes the pre-genesis consensus and governance schemas selected under `CONSENSUS-D1` and
//! `GOVERNANCE-D1`:
//! - **Consensus (`HotStuffTwoPhaseBftPosV1`):** partial-synchrony deterministic finality with
//!   strict `> 2/3` Ed25519 quorum certificates (`3 * signed_weight > 2 * total_weight`),
//!   full-possession data-before-vote, rolling retention `>= unbonding_delay_epochs`,
//!   weak-subjectivity checkpoint sync, optional separate archive nodes, single `i64³` world per
//!   chain, transparent public state, and zero v1 bridges/oracles.
//! - **Governance (`GovernancePolicyDescriptorV1`):** `> 2/3` bonded-stake supermajority with
//!   mandatory exit timelock `>= unbonding_delay_epochs`, automatic deterministic safe-halt on
//!   proven safety/root/bounds faults, a strictly pause-only auto-expiring `m-of-n` emergency
//!   council with zero state-mutation power, and a hard ban on rewriting finalized history.

use std::collections::BTreeSet;
use std::fmt;

use sha2::{Digest, Sha256};

use crate::asset_d1::ValidatorEconPolicyV1;
use crate::crypto_d1::{
    CanonicalEd25519PublicKeyV1, CanonicalEd25519SignatureV1, CryptoD1Error, HashSuiteIdV1,
    SignatureSuiteIdV1, SigningDomainTagV1, SigningPreimageV1, verify_preimage_strict,
};
use crate::identity_d1::{ActorIdV1, CredentialRoleV1, IdentityD1Error, ThresholdPolicyV1};
use crate::protocol::{ContentTag, TagKind, hash_canonical};

/// Canonical schema string for a [`ConsensusPolicyDescriptorV1`] policy object.
pub const CONSENSUS_D1_POLICY_SCHEMA: &[u8] = b"bitblock/consensus-d1/policy/v1";
/// Canonical schema string for a [`GovernancePolicyDescriptorV1`] policy object.
pub const GOVERNANCE_D1_POLICY_SCHEMA: &[u8] = b"bitblock/governance-d1/policy/v1";

/// Maximum active validators bounded by the v1 `CONSENSUS-D1` envelope.
pub const MAX_ACTIVE_VALIDATORS_V1: u16 = 128;

/// Errors returned by `CONSENSUS-D1` or `GOVERNANCE-D1` validation, QC checks, or governance rules.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConsensusGovError {
    /// Cryptographic validation failed.
    Crypto(CryptoD1Error),
    /// Identity or threshold policy validation failed.
    Identity(IdentityD1Error),
    /// Consensus or governance policy invariant was violated.
    InvalidPolicy(&'static str),
    /// Wire encoding was truncated, overlong, or had non-canonical fields.
    InvalidWire(&'static str),
    /// Validator set was empty, unsorted, duplicated, had zero weight, or reused keys across roles.
    InvalidValidatorSet(&'static str),
    /// Quorum certificate lacked strict `> 2/3` weight, duplicated a signer, or had an unknown signer.
    InvalidQuorumCertificate(&'static str),
    /// Governance action violated timelock, role separation, pause-only scope, or history immutability.
    GovernanceActionRejected(&'static str),
}

impl From<CryptoD1Error> for ConsensusGovError {
    fn from(value: CryptoD1Error) -> Self {
        Self::Crypto(value)
    }
}

impl From<IdentityD1Error> for ConsensusGovError {
    fn from(value: IdentityD1Error) -> Self {
        Self::Identity(value)
    }
}

impl fmt::Display for ConsensusGovError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ConsensusGovError {}

/// Canonical pre-genesis consensus & network-role policy descriptor (`CONSENSUS-D1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ConsensusPolicyDescriptorV1 {
    /// Schema version (`1`).
    pub schema_version: u16,
    /// Mechanism code (`1` = Deterministic-finality 2-phase BFT + bonded PoS).
    pub mechanism_code: u8,
    /// Quorum numerator (`2` for strict `> 2/3`).
    pub quorum_numerator: u16,
    /// Quorum denominator (`3` for strict `> 2/3`).
    pub quorum_denominator: u16,
    /// Maximum active validators (`4..=128`).
    pub max_validators: u16,
    /// Rolling validator/full-node retention horizon in epochs (`>= unbonding_delay_epochs`).
    pub rolling_retention_epochs: u64,
    /// Requires full block body possession & execution before finality vote (`true`).
    pub data_before_vote_required: bool,
    /// Enables weak-subjectivity checkpoint sync for fresh nodes (`true`).
    pub weak_subjectivity_checkpoint_sync: bool,
    /// Keeps full genesis-to-tip archive nodes optional and separate (`true`).
    pub archive_node_optional_separate: bool,
    /// Scope lock: single `i64³` world per chain (`true`).
    pub single_world_per_chain: bool,
    /// Scope lock: 100% public transparent state (`true`).
    pub public_state_only: bool,
    /// Scope lock: external bridges and oracles excluded in v1 (`false`).
    pub external_bridges_admitted: bool,
}

impl ConsensusPolicyDescriptorV1 {
    /// Fixed canonical wire byte length (`23` bytes).
    pub const WIRE_LEN: usize = 23;

    /// Returns the default v1 deterministic-finality PoS BFT policy selected under `CONSENSUS-D1`.
    pub const fn v1_bft_pos_default() -> Self {
        Self {
            schema_version: 1,
            mechanism_code: 1,
            quorum_numerator: 2,
            quorum_denominator: 3,
            max_validators: MAX_ACTIVE_VALIDATORS_V1,
            rolling_retention_epochs: 28,
            data_before_vote_required: true,
            weak_subjectivity_checkpoint_sync: true,
            archive_node_optional_separate: true,
            single_world_per_chain: true,
            public_state_only: true,
            external_bridges_admitted: false,
        }
    }

    /// Validates all consensus, quorum, DA, retention, and v1 scope invariants.
    pub fn validate(&self, econ: Option<&ValidatorEconPolicyV1>) -> Result<(), ConsensusGovError> {
        if self.schema_version != 1 {
            return Err(ConsensusGovError::InvalidPolicy("schema_version must be 1"));
        }
        if self.mechanism_code != 1 {
            return Err(ConsensusGovError::InvalidPolicy(
                "mechanism_code must be 1 (HotStuffTwoPhaseBftPosV1)",
            ));
        }
        if self.quorum_numerator != 2 || self.quorum_denominator != 3 {
            return Err(ConsensusGovError::InvalidPolicy(
                "BFT quorum rule must be strict > 2/3",
            ));
        }
        if self.max_validators < 4 || self.max_validators > MAX_ACTIVE_VALIDATORS_V1 {
            return Err(ConsensusGovError::InvalidPolicy(
                "max_validators must lie in 4..=128",
            ));
        }
        if self.rolling_retention_epochs == 0 {
            return Err(ConsensusGovError::InvalidPolicy(
                "rolling_retention_epochs must be > 0",
            ));
        }
        if let Some(ep) = econ
            && self.rolling_retention_epochs < ep.unbonding_delay_epochs
        {
            return Err(ConsensusGovError::InvalidPolicy(
                "rolling_retention_epochs must be >= unbonding_delay_epochs",
            ));
        }
        if !self.data_before_vote_required {
            return Err(ConsensusGovError::InvalidPolicy(
                "data_before_vote_required is mandatory",
            ));
        }
        if !self.weak_subjectivity_checkpoint_sync || !self.archive_node_optional_separate {
            return Err(ConsensusGovError::InvalidPolicy(
                "v1 requires weak-subjectivity sync and separate optional archive role",
            ));
        }
        if !self.single_world_per_chain || !self.public_state_only || self.external_bridges_admitted
        {
            return Err(ConsensusGovError::InvalidPolicy(
                "v1 scope locks single world, public state, and zero bridges/oracles",
            ));
        }
        Ok(())
    }

    /// Encodes this consensus descriptor into exact canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], ConsensusGovError> {
        self.validate(None)?;
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.schema_version.to_be_bytes());
        out[2] = self.mechanism_code;
        out[3..5].copy_from_slice(&self.quorum_numerator.to_be_bytes());
        out[5..7].copy_from_slice(&self.quorum_denominator.to_be_bytes());
        out[7..9].copy_from_slice(&self.max_validators.to_be_bytes());
        out[9..17].copy_from_slice(&self.rolling_retention_epochs.to_be_bytes());
        out[17] = u8::from(self.data_before_vote_required);
        out[18] = u8::from(self.weak_subjectivity_checkpoint_sync);
        out[19] = u8::from(self.archive_node_optional_separate);
        out[20] = u8::from(self.single_world_per_chain);
        out[21] = u8::from(self.public_state_only);
        out[22] = u8::from(self.external_bridges_admitted);
        Ok(out)
    }

    /// Decodes and validates a canonical `ConsensusPolicyDescriptorV1`.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, ConsensusGovError> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(ConsensusGovError::InvalidWire(
                "invalid consensus policy length",
            ));
        }
        let flag = |idx: usize| -> Result<bool, ConsensusGovError> {
            match bytes[idx] {
                0 => Ok(false),
                1 => Ok(true),
                _ => Err(ConsensusGovError::InvalidWire("non-boolean flag byte")),
            }
        };
        let desc = Self {
            schema_version: u16::from_be_bytes([bytes[0], bytes[1]]),
            mechanism_code: bytes[2],
            quorum_numerator: u16::from_be_bytes([bytes[3], bytes[4]]),
            quorum_denominator: u16::from_be_bytes([bytes[5], bytes[6]]),
            max_validators: u16::from_be_bytes([bytes[7], bytes[8]]),
            rolling_retention_epochs: u64::from_be_bytes(bytes[9..17].try_into().expect("8 bytes")),
            data_before_vote_required: flag(17)?,
            weak_subjectivity_checkpoint_sync: flag(18)?,
            archive_node_optional_separate: flag(19)?,
            single_world_per_chain: flag(20)?,
            public_state_only: flag(21)?,
            external_bridges_admitted: flag(22)?,
        };
        desc.validate(None)?;
        Ok(desc)
    }

    /// Computes the canonical `ContentTag` for this consensus policy descriptor.
    pub fn tag(&self) -> Result<ContentTag, ConsensusGovError> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            CONSENSUS_D1_POLICY_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

/// One validator's pre-genesis authority descriptor with strictly separated vote and proposal keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct InitialValidatorDescriptorV1 {
    /// Validator identity (`ActorIdV1` namespace distinct from ordinary observer actions).
    pub validator_id: ActorIdV1,
    /// Role-separated Ed25519 key used exclusively for `ConsensusVote`.
    pub vote_key: CanonicalEd25519PublicKeyV1,
    /// Role-separated Ed25519 key used exclusively for `BlockProposal`.
    pub proposal_key: CanonicalEd25519PublicKeyV1,
    /// Positive bonded voting weight (`> 0`).
    pub weight: u128,
}

/// Canonical, non-self-referential validator set (`Q_h`) for quorum-certificate verification.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct InitialValidatorSetV1 {
    validators: Vec<InitialValidatorDescriptorV1>,
    total_weight: u128,
    active_validator_root: [u8; 32],
}

impl InitialValidatorSetV1 {
    /// Constructs and validates a sorted unique validator set with disjoint vote/proposal keys.
    pub fn new(validators: Vec<InitialValidatorDescriptorV1>) -> Result<Self, ConsensusGovError> {
        if validators.len() < 4 || validators.len() > usize::from(MAX_ACTIVE_VALIDATORS_V1) {
            return Err(ConsensusGovError::InvalidValidatorSet(
                "validator count must lie in 4..=128",
            ));
        }
        let mut seen_keys = BTreeSet::new();
        let mut total_weight = 0u128;
        let mut hasher = Sha256::new();
        hasher.update(b"bitblock/consensus-d1/validator-set-root/v1");
        hasher.update((validators.len() as u16).to_be_bytes());
        for (idx, v) in validators.iter().enumerate() {
            if idx > 0 && validators[idx - 1].validator_id >= v.validator_id {
                return Err(ConsensusGovError::InvalidValidatorSet(
                    "validators must be strictly sorted by unique validator_id",
                ));
            }
            if v.weight == 0 {
                return Err(ConsensusGovError::InvalidValidatorSet(
                    "validator weight must be > 0",
                ));
            }
            if v.vote_key == v.proposal_key
                || !seen_keys.insert(v.vote_key)
                || !seen_keys.insert(v.proposal_key)
            {
                return Err(ConsensusGovError::InvalidValidatorSet(
                    "vote and proposal keys must be globally distinct across validators",
                ));
            }
            total_weight = total_weight.checked_add(v.weight).ok_or(
                ConsensusGovError::InvalidValidatorSet("total weight overflow"),
            )?;
            hasher.update(v.validator_id.as_bytes());
            hasher.update(v.vote_key.as_bytes());
            hasher.update(v.proposal_key.as_bytes());
            hasher.update(v.weight.to_be_bytes());
        }
        Ok(Self {
            validators,
            total_weight,
            active_validator_root: hasher.finalize().into(),
        })
    }

    /// Returns the canonical root digest committing to the ordered active validator set.
    pub const fn active_validator_root(&self) -> [u8; 32] {
        self.active_validator_root
    }

    /// Returns the total bonded voting weight of this validator set.
    pub const fn total_weight(&self) -> u128 {
        self.total_weight
    }

    /// Returns `true` iff `signed_weight` satisfies strict `> 2/3` quorum using checked integer math.
    pub fn meets_strict_supermajority(
        &self,
        signed_weight: u128,
    ) -> Result<bool, ConsensusGovError> {
        let lhs =
            signed_weight
                .checked_mul(3)
                .ok_or(ConsensusGovError::InvalidQuorumCertificate(
                    "quorum multiplication overflow",
                ))?;
        let rhs =
            self.total_weight
                .checked_mul(2)
                .ok_or(ConsensusGovError::InvalidQuorumCertificate(
                    "quorum multiplication overflow",
                ))?;
        Ok(lhs > rhs)
    }

    /// Looks up a validator descriptor by its `ActorIdV1`.
    pub fn find(&self, validator_id: &ActorIdV1) -> Option<&InitialValidatorDescriptorV1> {
        self.validators
            .binary_search_by_key(validator_id, |v| v.validator_id)
            .ok()
            .map(|idx| &self.validators[idx])
    }
}

/// Canonical vote signing payload matching `LAYER1_DESIGN_PAPER.md` §6.1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VoteSignDocV1 {
    /// Epoch number of the authority-selected active validator set.
    pub epoch: u64,
    /// Canonical root of the authority-selected active validator set.
    pub active_validator_root: [u8; 32],
    /// Block height being voted on.
    pub height: u64,
    /// BFT round / view number.
    pub round: u32,
    /// BFT vote phase (`0` = prepare, `1` = pre-commit/commit).
    pub vote_phase: u8,
    /// Candidate `AcceptedProposalCoreV1` block identifier.
    pub block_id: [u8; 32],
}

impl VoteSignDocV1 {
    /// Computes the canonical 32-byte digest of this vote document.
    pub fn payload_digest(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(b"bitblock/consensus-d1/vote-sign-doc/v1");
        h.update(self.epoch.to_be_bytes());
        h.update(self.active_validator_root);
        h.update(self.height.to_be_bytes());
        h.update(self.round.to_be_bytes());
        h.update([self.vote_phase]);
        h.update(self.block_id);
        h.finalize().into()
    }
}

/// Verifies a BFT quorum certificate over `vote` against the authority-selected `validator_set`,
/// rejecting wrong validator roots, duplicate signers, unknown signers, invalid signatures, and
/// non-supermajority weight (`3 * signed_weight <= 2 * total_weight`).
pub fn verify_quorum_certificate(
    validator_set: &InitialValidatorSetV1,
    chain_id: u64,
    genesis_id: [u8; 32],
    launch_manifest_id: [u8; 32],
    profile_id: [u8; 32],
    vote: &VoteSignDocV1,
    votes: &[(ActorIdV1, CanonicalEd25519SignatureV1)],
) -> Result<u128, ConsensusGovError> {
    if vote.active_validator_root != validator_set.active_validator_root() {
        return Err(ConsensusGovError::InvalidQuorumCertificate(
            "vote active_validator_root does not match authority-selected validator set",
        ));
    }
    if vote.vote_phase > 1 {
        return Err(ConsensusGovError::InvalidQuorumCertificate(
            "unknown BFT vote_phase",
        ));
    }
    let preimage = SigningPreimageV1 {
        protocol_version: 1,
        chain_id,
        genesis_id,
        launch_manifest_id,
        profile_id,
        domain_tag: SigningDomainTagV1::ConsensusVote,
        signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
        hash_suite: HashSuiteIdV1::Sha256V1,
        payload_digest: vote.payload_digest(),
    };
    let mut seen = BTreeSet::new();
    let mut signed_weight = 0u128;
    for &(vid, sig) in votes {
        if !seen.insert(vid) {
            return Err(ConsensusGovError::InvalidQuorumCertificate(
                "duplicate validator signer in quorum certificate",
            ));
        }
        let Some(desc) = validator_set.find(&vid) else {
            return Err(ConsensusGovError::InvalidQuorumCertificate(
                "unknown validator signer not in active set",
            ));
        };
        verify_preimage_strict(&desc.vote_key, &preimage, &sig)?;
        signed_weight = signed_weight.checked_add(desc.weight).ok_or(
            ConsensusGovError::InvalidQuorumCertificate("signed weight overflow"),
        )?;
    }
    if !validator_set.meets_strict_supermajority(signed_weight)? {
        return Err(ConsensusGovError::InvalidQuorumCertificate(
            "signed weight does not satisfy strict > 2/3 supermajority",
        ));
    }
    Ok(signed_weight)
}

/// Canonical pre-genesis governance, safe-halt, and emergency-pause policy (`GOVERNANCE-D1`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GovernancePolicyDescriptorV1 {
    /// Schema version (`1`).
    pub schema_version: u16,
    /// Bonded-stake supermajority numerator (`2` for strict `> 2/3`).
    pub upgrade_quorum_numerator: u16,
    /// Bonded-stake supermajority denominator (`3` for strict `> 2/3`).
    pub upgrade_quorum_denominator: u16,
    /// Mandatory public exit timelock in epochs (`>= unbonding_delay_epochs`).
    pub mandatory_exit_timelock_epochs: u64,
    /// Auto-expiry ceiling for any emergency pause (`1..=mandatory_exit_timelock_epochs`).
    pub max_emergency_pause_epochs: u64,
    /// Must be `true`: proven equivocation/root/bounds fault triggers automatic deterministic halt.
    pub automatic_safe_halt_on_proven_fault: bool,
    /// Must be `true`: emergency council is strictly pause-only with zero state-mutation power.
    pub emergency_council_pause_only: bool,
    /// Must be `true`: rewriting finalized history is strictly forbidden.
    pub forbid_history_rewrite: bool,
    /// Role-separated `m-of-n` emergency pause council (`CredentialRoleV1::EmergencyPauseSigner`).
    pub emergency_council: ThresholdPolicyV1,
}

impl GovernancePolicyDescriptorV1 {
    /// Validates all governance quorum, exit-timelock, pause-only, and immutability invariants.
    pub fn validate(&self, econ: Option<&ValidatorEconPolicyV1>) -> Result<(), ConsensusGovError> {
        if self.schema_version != 1 {
            return Err(ConsensusGovError::InvalidPolicy("schema_version must be 1"));
        }
        if self.upgrade_quorum_numerator != 2 || self.upgrade_quorum_denominator != 3 {
            return Err(ConsensusGovError::InvalidPolicy(
                "upgrade quorum must be strict > 2/3 bonded stake",
            ));
        }
        if self.mandatory_exit_timelock_epochs == 0 {
            return Err(ConsensusGovError::InvalidPolicy(
                "mandatory_exit_timelock_epochs must be > 0",
            ));
        }
        if let Some(ep) = econ
            && self.mandatory_exit_timelock_epochs < ep.unbonding_delay_epochs
        {
            return Err(ConsensusGovError::InvalidPolicy(
                "mandatory_exit_timelock_epochs must be >= unbonding_delay_epochs",
            ));
        }
        if self.max_emergency_pause_epochs == 0
            || self.max_emergency_pause_epochs > self.mandatory_exit_timelock_epochs
        {
            return Err(ConsensusGovError::InvalidPolicy(
                "max_emergency_pause_epochs must lie in 1..=mandatory_exit_timelock_epochs",
            ));
        }
        if !self.automatic_safe_halt_on_proven_fault {
            return Err(ConsensusGovError::InvalidPolicy(
                "automatic_safe_halt_on_proven_fault is mandatory",
            ));
        }
        if !self.emergency_council_pause_only {
            return Err(ConsensusGovError::InvalidPolicy(
                "emergency council must be strictly pause-only",
            ));
        }
        if !self.forbid_history_rewrite {
            return Err(ConsensusGovError::InvalidPolicy(
                "forbid_history_rewrite is mandatory",
            ));
        }
        self.emergency_council
            .validate(CredentialRoleV1::EmergencyPauseSigner)?;
        Ok(())
    }

    /// Encodes the governance descriptor into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, ConsensusGovError> {
        self.validate(None)?;
        let mut out = Vec::new();
        out.extend_from_slice(&self.schema_version.to_be_bytes());
        out.extend_from_slice(&self.upgrade_quorum_numerator.to_be_bytes());
        out.extend_from_slice(&self.upgrade_quorum_denominator.to_be_bytes());
        out.extend_from_slice(&self.mandatory_exit_timelock_epochs.to_be_bytes());
        out.extend_from_slice(&self.max_emergency_pause_epochs.to_be_bytes());
        out.push(u8::from(self.automatic_safe_halt_on_proven_fault));
        out.push(u8::from(self.emergency_council_pause_only));
        out.push(u8::from(self.forbid_history_rewrite));
        self.emergency_council.encode_into(&mut out)?;
        Ok(out)
    }

    /// Decodes and validates a canonical `GovernancePolicyDescriptorV1`.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, ConsensusGovError> {
        if bytes.len() < 25 {
            return Err(ConsensusGovError::InvalidWire(
                "truncated governance policy header",
            ));
        }
        let flag = |idx: usize| -> Result<bool, ConsensusGovError> {
            match bytes[idx] {
                0 => Ok(false),
                1 => Ok(true),
                _ => Err(ConsensusGovError::InvalidWire("non-boolean flag byte")),
            }
        };
        let mut cursor = 25;
        let emergency_council = ThresholdPolicyV1::decode_from(
            bytes,
            &mut cursor,
            CredentialRoleV1::EmergencyPauseSigner,
        )?;
        if cursor != bytes.len() {
            return Err(ConsensusGovError::InvalidWire(
                "trailing bytes after governance policy",
            ));
        }
        let desc = Self {
            schema_version: u16::from_be_bytes([bytes[0], bytes[1]]),
            upgrade_quorum_numerator: u16::from_be_bytes([bytes[2], bytes[3]]),
            upgrade_quorum_denominator: u16::from_be_bytes([bytes[4], bytes[5]]),
            mandatory_exit_timelock_epochs: u64::from_be_bytes(
                bytes[6..14].try_into().expect("8 bytes"),
            ),
            max_emergency_pause_epochs: u64::from_be_bytes(
                bytes[14..22].try_into().expect("8 bytes"),
            ),
            automatic_safe_halt_on_proven_fault: flag(22)?,
            emergency_council_pause_only: flag(23)?,
            forbid_history_rewrite: flag(24)?,
            emergency_council,
        };
        desc.validate(None)?;
        Ok(desc)
    }

    /// Computes the canonical `ContentTag` for this governance policy descriptor.
    pub fn tag(&self) -> Result<ContentTag, ConsensusGovError> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            GOVERNANCE_D1_POLICY_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and bounded length"))
    }
}

/// Governance action variants evaluated against [`GovernancePolicyDescriptorV1`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GovernanceActionV1 {
    /// Timelocked `> 2/3` stake-weighted upgrade or forward compensating transition.
    TimelockedStakeUpgrade {
        /// Epoch when the proposal closed approval.
        approved_epoch: u64,
        /// Target future activation epoch (must be `>= approved_epoch + timelock`).
        activation_epoch: u64,
        /// Total bonded stake that approved the proposal.
        approved_stake: u128,
    },
    /// Bounded, auto-expiring emergency pause signed by the pause-only council.
    EmergencyPause {
        /// Requested pause duration in epochs (`1..=max_emergency_pause_epochs`).
        duration_epochs: u64,
        /// Digest of public evidence justifying the pause.
        evidence_digest: [u8; 32],
    },
    /// Forbidden attempt by the emergency council to mutate balances, world state, or rules.
    EmergencyStateMutationAttempt,
    /// Forbidden attempt to rewrite finalized history rather than execute a forward recovery.
    HistoryRewriteAttempt,
}

/// Evaluates whether a proposed governance or emergency action is admissible under `policy`.
pub fn evaluate_governance_action(
    policy: &GovernancePolicyDescriptorV1,
    validator_set: &InitialValidatorSetV1,
    action: &GovernanceActionV1,
    mut pause_preimage: Option<SigningPreimageV1>,
    council_signatures: &[(CanonicalEd25519PublicKeyV1, CanonicalEd25519SignatureV1)],
) -> Result<(), ConsensusGovError> {
    policy.validate(None)?;
    match action {
        GovernanceActionV1::TimelockedStakeUpgrade {
            approved_epoch,
            activation_epoch,
            approved_stake,
        } => {
            let earliest = approved_epoch
                .checked_add(policy.mandatory_exit_timelock_epochs)
                .ok_or(ConsensusGovError::GovernanceActionRejected(
                    "timelock epoch overflow",
                ))?;
            if *activation_epoch < earliest {
                return Err(ConsensusGovError::GovernanceActionRejected(
                    "upgrade violates mandatory exit timelock",
                ));
            }
            if !validator_set.meets_strict_supermajority(*approved_stake)? {
                return Err(ConsensusGovError::GovernanceActionRejected(
                    "upgrade lacks > 2/3 bonded stake approval",
                ));
            }
            Ok(())
        }
        GovernanceActionV1::EmergencyPause {
            duration_epochs,
            evidence_digest,
        } => {
            if *duration_epochs == 0 || *duration_epochs > policy.max_emergency_pause_epochs {
                return Err(ConsensusGovError::GovernanceActionRejected(
                    "emergency pause duration outside 1..=max_emergency_pause_epochs",
                ));
            }
            let Some(pre) = pause_preimage.as_mut() else {
                return Err(ConsensusGovError::GovernanceActionRejected(
                    "emergency pause requires signed evidence preimage",
                ));
            };
            pre.domain_tag = SigningDomainTagV1::EmergencyPause;
            pre.payload_digest = *evidence_digest;
            policy.emergency_council.verify_threshold_signatures(
                CredentialRoleV1::EmergencyPauseSigner,
                pre,
                council_signatures,
            )?;
            Ok(())
        }
        GovernanceActionV1::EmergencyStateMutationAttempt => {
            Err(ConsensusGovError::GovernanceActionRejected(
                "emergency council is strictly pause-only and cannot mutate state",
            ))
        }
        GovernanceActionV1::HistoryRewriteAttempt => {
            Err(ConsensusGovError::GovernanceActionRejected(
                "rewriting finalized history is strictly forbidden; use forward compensation or new genesis",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto_d1::sign_preimage_deterministic;

    fn pk(seed: u8) -> CanonicalEd25519PublicKeyV1 {
        CanonicalEd25519PublicKeyV1::from_secret_seed(&[seed; 32]).unwrap()
    }

    #[test]
    fn consensus_qc_and_governance_rules_enforced() {
        let econ = ValidatorEconPolicyV1::v1_pos_default();
        let cons = ConsensusPolicyDescriptorV1::v1_bft_pos_default();
        cons.validate(Some(&econ)).unwrap();
        let raw_c = cons.encode_canonical().unwrap();
        assert_eq!(
            ConsensusPolicyDescriptorV1::decode_canonical(&raw_c).unwrap(),
            cons
        );

        let mut vals = Vec::new();
        for i in 1u8..=4 {
            vals.push(InitialValidatorDescriptorV1 {
                validator_id: ActorIdV1::from_bytes([i; 32]),
                vote_key: pk(10 + i),
                proposal_key: pk(20 + i),
                weight: 100,
            });
        }
        let vset = InitialValidatorSetV1::new(vals.clone()).unwrap();
        let vote = VoteSignDocV1 {
            epoch: 0,
            active_validator_root: vset.active_validator_root(),
            height: 1,
            round: 0,
            vote_phase: 1,
            block_id: [0x77; 32],
        };
        let pre = SigningPreimageV1 {
            protocol_version: 1,
            chain_id: 1,
            genesis_id: [1; 32],
            launch_manifest_id: [2; 32],
            profile_id: [3; 32],
            domain_tag: SigningDomainTagV1::ConsensusVote,
            signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
            hash_suite: HashSuiteIdV1::Sha256V1,
            payload_digest: vote.payload_digest(),
        };
        let s1 = sign_preimage_deterministic(&[11u8; 32], &pre).unwrap();
        let s2 = sign_preimage_deterministic(&[12u8; 32], &pre).unwrap();
        let s3 = sign_preimage_deterministic(&[13u8; 32], &pre).unwrap();

        // 2/4 = 200 <= 2/3 of 400 -> must fail.
        assert!(
            verify_quorum_certificate(
                &vset,
                1,
                [1; 32],
                [2; 32],
                [3; 32],
                &vote,
                &[(vals[0].validator_id, s1), (vals[1].validator_id, s2)],
            )
            .is_err()
        );
        // 3/4 = 300 > 2/3 of 400 -> passes.
        let w = verify_quorum_certificate(
            &vset,
            1,
            [1; 32],
            [2; 32],
            [3; 32],
            &vote,
            &[
                (vals[0].validator_id, s1),
                (vals[1].validator_id, s2),
                (vals[2].validator_id, s3),
            ],
        )
        .unwrap();
        assert_eq!(w, 300);
        // Duplicate signer -> must fail.
        assert!(
            verify_quorum_certificate(
                &vset,
                1,
                [1; 32],
                [2; 32],
                [3; 32],
                &vote,
                &[
                    (vals[0].validator_id, s1),
                    (vals[0].validator_id, s1),
                    (vals[2].validator_id, s3),
                ],
            )
            .is_err()
        );

        let mut council_keys = vec![pk(31), pk(32), pk(33)];
        council_keys.sort();
        let gov = GovernancePolicyDescriptorV1 {
            schema_version: 1,
            upgrade_quorum_numerator: 2,
            upgrade_quorum_denominator: 3,
            mandatory_exit_timelock_epochs: 21,
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
        gov.validate(Some(&econ)).unwrap();
        let raw_g = gov.encode_canonical().unwrap();
        assert_eq!(
            GovernancePolicyDescriptorV1::decode_canonical(&raw_g).unwrap(),
            gov
        );

        // Rushed upgrade fails; timelocked >2/3 upgrade passes; rewrite & emergency mutation fail.
        assert!(
            evaluate_governance_action(
                &gov,
                &vset,
                &GovernanceActionV1::TimelockedStakeUpgrade {
                    approved_epoch: 10,
                    activation_epoch: 30,
                    approved_stake: 300,
                },
                None,
                &[],
            )
            .is_err()
        );
        evaluate_governance_action(
            &gov,
            &vset,
            &GovernanceActionV1::TimelockedStakeUpgrade {
                approved_epoch: 10,
                activation_epoch: 31,
                approved_stake: 300,
            },
            None,
            &[],
        )
        .unwrap();
        assert!(
            evaluate_governance_action(
                &gov,
                &vset,
                &GovernanceActionV1::EmergencyStateMutationAttempt,
                None,
                &[],
            )
            .is_err()
        );
        assert!(
            evaluate_governance_action(
                &gov,
                &vset,
                &GovernanceActionV1::HistoryRewriteAttempt,
                None,
                &[],
            )
            .is_err()
        );
    }
}
