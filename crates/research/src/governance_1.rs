//! `GOVERNANCE-1`: deterministic authenticated timelocked stake governance, auto-expiring
//! pause-only Emergency Council with anti-DoS cooldown, automatic proven-fault Safe-Halt, and
//! stake-only Safe-Halt recovery over `DomainIdV1::GovernanceState` (`PS-081`).
//!
//! Turns `GOVERNANCE-D1` pre-genesis authority bytes into deterministic authenticated state
//! transitions committed inside [`GovernanceStateRootV1`] (`DomainIdV1::GovernanceState = 5`):
//! - **Mandatory 21-epoch exit timelock:** every `> 2/3` bonded-stake parameter upgrade or
//!   Safe-Halt restart must satisfy `activation_epoch >= approved_epoch + mandatory_exit_timelock`.
//! - **Conflict, replay, expiry, stale-set, and compromised-key guards:** duplicate proposal IDs,
//!   conflicting queued parameter keys, expired proposals, stale validator-set roots, and revoked
//!   compromised keys fail closed with zero state mutation.
//! - **Anti-DoS pause-only Emergency Council:** bounded to `1..=max_emergency_pause_epochs` (`3`),
//!   auto-expiring at `pause_until_epoch`, followed by a mandatory cooldown window
//!   (`pause_cooldown_until_epoch = pause_until_epoch + max_emergency_pause_epochs`) forbidding
//!   back-to-back indefinite pauses. Council holds zero state-mutation, history-rewrite, or
//!   Safe-Halt-restart authority.
//! - **Automatic Safe-Halt & stake-only restart:** proven `BOUNDS-1` (`SafeHaltEvidenceV1`) or
//!   `PROTOCOL-ID-1` parent-collision faults halt normal governance immediately; only `> 2/3`
//!   timelocked stake approval can restore `Normal` operation without rewriting finalized history.
//!
//! Correct governance execution does not make decisions wise, decentralized, fair, or immune to
//! capture.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use sha2::{Digest, Sha256};

use crate::auth_commit_1::{
    AuthCommit1Error, DomainIdV1, DomainPatriciaProofV1, DomainPatriciaTrieV1,
};
use crate::bounds_1::SafeHaltEvidenceV1;
use crate::consensus_gov_d1::{
    ConsensusGovError, GovernancePolicyDescriptorV1, InitialValidatorSetV1,
};
use crate::crypto_d1::{
    CanonicalEd25519PublicKeyV1, CanonicalEd25519SignatureV1, HashSuiteIdV1, SignatureSuiteIdV1,
    SigningDomainTagV1, SigningPreimageV1, verify_preimage_strict,
};
use crate::identity_d1::{ActorIdV1, CredentialRoleV1};
use crate::protocol::{ContentTag, TagKind, hash_canonical};

/// Canonical schema string for a [`GovernanceStateRootV1`] commitment (`GOVERNANCE-1`).
pub const GOVERNANCE_1_STATE_ROOT_SCHEMA: &[u8] = b"bitblock/governance-1/governance-root/v1";
/// Canonical schema string for a [`GovernanceTransitionReceiptV1`] receipt (`GOVERNANCE-1`).
pub const GOVERNANCE_1_RECEIPT_SCHEMA: &[u8] = b"bitblock/governance-1/governance-receipt/v1";

const PROPOSAL_KEY_PREFIX: &[u8] = b"bb.gov1.proposal.v1";
const PARAM_KEY_PREFIX: &[u8] = b"bb.gov1.param.v1";
const APPROVAL_DOC_PREFIX: &[u8] = b"bb.gov1.approval.v1";
const PAUSE_DOC_PREFIX: &[u8] = b"bb.gov1.pause.v1";

/// Fail-closed policy, codec, or Patricia error for `GOVERNANCE-1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Governance1Error {
    /// Underlying `GOVERNANCE-D1` / `CONSENSUS-D1` policy error.
    Policy(ConsensusGovError),
    /// Underlying `AUTH-COMMIT-1` Patricia proof or rebuild error.
    AuthCommit(AuthCommit1Error),
    /// Canonical wire buffer had wrong length, version, discriminant, or trailing bytes.
    InvalidWire(&'static str),
}

impl From<ConsensusGovError> for Governance1Error {
    fn from(value: ConsensusGovError) -> Self {
        Self::Policy(value)
    }
}

impl From<AuthCommit1Error> for Governance1Error {
    fn from(value: AuthCommit1Error) -> Self {
        Self::AuthCommit(value)
    }
}

impl fmt::Display for Governance1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Governance1Error {}

/// Domain binding context for `GOVERNANCE-1` Ed25519 preimages.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GovernanceContextV1 {
    /// Protocol version (`1`).
    pub protocol_version: u16,
    /// Chain identifier.
    pub chain_id: u64,
    /// Bound `GENESIS-D1` genesis identifier.
    pub genesis_id: [u8; 32],
    /// Bound launch manifest identifier.
    pub launch_manifest_id: [u8; 32],
    /// Bound `PROFILE-D1` profile identifier.
    pub profile_id: [u8; 32],
}

impl GovernanceContextV1 {
    /// Builds a domain-separated `SigningPreimageV1` for governance or emergency pause checks.
    pub const fn preimage(
        &self,
        domain_tag: SigningDomainTagV1,
        payload_digest: [u8; 32],
    ) -> SigningPreimageV1 {
        let Self {
            protocol_version,
            chain_id,
            genesis_id,
            launch_manifest_id,
            profile_id,
        } = *self;
        SigningPreimageV1 {
            domain_tag,
            payload_digest,
            signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
            hash_suite: HashSuiteIdV1::Sha256V1,
            protocol_version,
            chain_id,
            genesis_id,
            launch_manifest_id,
            profile_id,
        }
    }
}

/// Active lifecycle mode of the `GOVERNANCE-1` state machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum GovernanceLifecycleCodeV1 {
    /// Normal operation; timelocked parameter proposals and bounded emergency pauses allowed.
    Normal = 0,
    /// Auto-expiring emergency pause active until `pause_until_epoch`.
    EmergencyPaused = 1,
    /// Deterministic Safe-Halt triggered by proven fault; only `> 2/3` stake restart clears it.
    SafeHalted = 2,
}

impl GovernanceLifecycleCodeV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte lifecycle discriminant.
    pub const fn from_code(code: u8) -> Result<Self, Governance1Error> {
        match code {
            0 => Ok(Self::Normal),
            1 => Ok(Self::EmergencyPaused),
            2 => Ok(Self::SafeHalted),
            _ => Err(Governance1Error::InvalidWire(
                "unknown GovernanceLifecycleCodeV1",
            )),
        }
    }
}

/// Action kind tag recorded inside a [`GovernanceTransitionReceiptV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum GovernanceActionKindV1 {
    /// Queue a `> 2/3` stake-approved timelocked parameter or restart proposal.
    QueueProposal = 1,
    /// Activate a previously queued proposal whose timelock has matured.
    ActivateProposal = 2,
    /// Trigger a bounded auto-expiring emergency pause by the Emergency Council.
    EmergencyPause = 3,
    /// Trigger an automatic deterministic Safe-Halt on proven bounds or collision fault.
    AutomaticSafeHalt = 4,
    /// Forbidden state-mutation, emergency-unhalt, or finalized-history-rewrite attempt.
    ForbiddenAttempt = 5,
}

impl GovernanceActionKindV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte action-kind discriminant.
    pub const fn from_code(code: u8) -> Result<Self, Governance1Error> {
        match code {
            1 => Ok(Self::QueueProposal),
            2 => Ok(Self::ActivateProposal),
            3 => Ok(Self::EmergencyPause),
            4 => Ok(Self::AutomaticSafeHalt),
            5 => Ok(Self::ForbiddenAttempt),
            _ => Err(Governance1Error::InvalidWire(
                "unknown GovernanceActionKindV1",
            )),
        }
    }
}

/// Deterministic disposition recorded inside every [`GovernanceTransitionReceiptV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum GovernanceDispositionV1 {
    /// Transition verified and committed cleanly to `GovernanceStateRootV1`.
    Admitted = 0,
    /// Signature invalid, duplicate signer, unknown signer, or insufficient quorum (`<= 2/3`).
    RejectedUnauthorized = 1,
    /// Proposal ID was already queued, activated, or expired.
    RejectedReplayedProposal = 2,
    /// Another queued proposal already targets the same parameter key.
    RejectedConflictingProposal = 3,
    /// Proposal violated the mandatory 21-epoch exit timelock or activation epoch not yet reached.
    RejectedRushedTimelock = 4,
    /// Proposal already expired or had invalid expiry window (`expiry_epoch < activation_epoch`).
    RejectedExpiredProposal = 5,
    /// Submitted validator-set root did not match the active authority-selected validator set.
    RejectedStaleValidatorSet = 6,
    /// Signer key was marked compromised/revoked and refused fail-closed.
    RejectedCompromisedKey = 7,
    /// Emergency pause violated duration cap (`1..=3`) or anti-DoS cooldown (back-to-back pause).
    RejectedPauseCooldownDoS = 8,
    /// Emergency Council attempted forbidden state mutation or Safe-Halt restart.
    RejectedEmergencyOverreach = 9,
    /// Attempted forbidden finalized-history rewrite or epoch rollback.
    RejectedHistoryRewrite = 10,
    /// Operation is forbidden while `SafeHalted` (only `> 2/3` stake restart allowed) or evidence invalid.
    RejectedWhileHaltedOrInvalidFault = 11,
}

impl GovernanceDispositionV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte disposition discriminant.
    pub const fn from_code(code: u8) -> Result<Self, Governance1Error> {
        match code {
            0 => Ok(Self::Admitted),
            1 => Ok(Self::RejectedUnauthorized),
            2 => Ok(Self::RejectedReplayedProposal),
            3 => Ok(Self::RejectedConflictingProposal),
            4 => Ok(Self::RejectedRushedTimelock),
            5 => Ok(Self::RejectedExpiredProposal),
            6 => Ok(Self::RejectedStaleValidatorSet),
            7 => Ok(Self::RejectedCompromisedKey),
            8 => Ok(Self::RejectedPauseCooldownDoS),
            9 => Ok(Self::RejectedEmergencyOverreach),
            10 => Ok(Self::RejectedHistoryRewrite),
            11 => Ok(Self::RejectedWhileHaltedOrInvalidFault),
            _ => Err(Governance1Error::InvalidWire(
                "unknown GovernanceDispositionV1",
            )),
        }
    }
}

/// Computes the 32-byte Patricia key for a governance proposal inside Domain 5.
pub fn proposal_trie_key(proposal_id: &[u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(PROPOSAL_KEY_PREFIX);
    h.update(proposal_id);
    h.finalize().into()
}

/// Computes the 32-byte Patricia key for an active governed parameter slot inside Domain 5.
pub fn param_trie_key(param_key: u32) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(PARAM_KEY_PREFIX);
    h.update(param_key.to_be_bytes());
    h.finalize().into()
}

/// Computes the canonical 32-byte payload digest signed by `> 2/3` validators to approve a proposal.
#[allow(clippy::too_many_arguments)]
pub fn proposal_approval_digest(
    validator_set_root: &[u8; 32],
    proposal_id: &[u8; 32],
    param_key: u32,
    param_value: u64,
    approved_epoch: u64,
    activation_epoch: u64,
    expiry_epoch: u64,
    is_restart: bool,
) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(APPROVAL_DOC_PREFIX);
    h.update(validator_set_root);
    h.update(proposal_id);
    h.update(param_key.to_be_bytes());
    h.update(param_value.to_be_bytes());
    h.update(approved_epoch.to_be_bytes());
    h.update(activation_epoch.to_be_bytes());
    h.update(expiry_epoch.to_be_bytes());
    h.update([u8::from(is_restart)]);
    h.finalize().into()
}

/// Computes the canonical 32-byte payload digest signed by the pause-only Emergency Council.
pub fn emergency_pause_digest(
    epoch: u64,
    duration_epochs: u64,
    evidence_digest: &[u8; 32],
) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(PAUSE_DOC_PREFIX);
    h.update(epoch.to_be_bytes());
    h.update(duration_epochs.to_be_bytes());
    h.update(evidence_digest);
    h.finalize().into()
}

/// Lifecycle status of one committed [`ProposalRecordV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ProposalStatusV1 {
    /// Approved by `> 2/3` stake and awaiting exit timelock maturation.
    Queued = 0,
    /// Activated on-chain after `activation_epoch` was reached.
    Activated = 1,
    /// Expired unactivated past `expiry_epoch`.
    Expired = 2,
}

impl ProposalStatusV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte status discriminant.
    pub const fn from_code(code: u8) -> Result<Self, Governance1Error> {
        match code {
            0 => Ok(Self::Queued),
            1 => Ok(Self::Activated),
            2 => Ok(Self::Expired),
            _ => Err(Governance1Error::InvalidWire("unknown ProposalStatusV1")),
        }
    }
}

/// Canonical 70-byte proposal record committed under `DomainIdV1::GovernanceState`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProposalRecordV1 {
    /// Unique 32-byte proposal identifier (prevents replay).
    pub proposal_id: [u8; 32],
    /// Governed parameter slot (`0` reserved for Safe-Halt stake restart, `>= 1` for parameters).
    pub param_key: u32,
    /// Target numeric parameter value (or restart epoch tag when `is_restart == true`).
    pub param_value: u64,
    /// Epoch when `> 2/3` stake approved and queued this proposal.
    pub approved_epoch: u64,
    /// Earliest epoch at which this proposal may activate (`>= approved_epoch + timelock`).
    pub activation_epoch: u64,
    /// Final epoch at which activation is permitted (`>= activation_epoch`).
    pub expiry_epoch: u64,
    /// Current lifecycle status (`Queued`, `Activated`, or `Expired`).
    pub status: ProposalStatusV1,
    /// Whether this proposal is a stake-only Safe-Halt restart (`true`) or parameter update (`false`).
    pub is_restart: bool,
}

impl ProposalRecordV1 {
    /// Fixed canonical big-endian wire length (`70` bytes).
    pub const WIRE_LEN: usize = 32 + 4 + 8 + 8 + 8 + 8 + 1 + 1;

    /// Encodes this proposal record into 70 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> [u8; Self::WIRE_LEN] {
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..32].copy_from_slice(&self.proposal_id);
        out[32..36].copy_from_slice(&self.param_key.to_be_bytes());
        out[36..44].copy_from_slice(&self.param_value.to_be_bytes());
        out[44..52].copy_from_slice(&self.approved_epoch.to_be_bytes());
        out[52..60].copy_from_slice(&self.activation_epoch.to_be_bytes());
        out[60..68].copy_from_slice(&self.expiry_epoch.to_be_bytes());
        out[68] = self.status.code();
        out[69] = u8::from(self.is_restart);
        out
    }

    /// Decodes a 70-byte canonical [`ProposalRecordV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Governance1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(Governance1Error::InvalidWire(
                "ProposalRecordV1 must be 70 bytes",
            ));
        }
        let is_restart = match bytes[69] {
            0 => false,
            1 => true,
            _ => return Err(Governance1Error::InvalidWire("non-boolean is_restart")),
        };
        Ok(Self {
            proposal_id: bytes[0..32].try_into().expect("32"),
            param_key: u32::from_be_bytes(bytes[32..36].try_into().expect("4")),
            param_value: u64::from_be_bytes(bytes[36..44].try_into().expect("8")),
            approved_epoch: u64::from_be_bytes(bytes[44..52].try_into().expect("8")),
            activation_epoch: u64::from_be_bytes(bytes[52..60].try_into().expect("8")),
            expiry_epoch: u64::from_be_bytes(bytes[60..68].try_into().expect("8")),
            status: ProposalStatusV1::from_code(bytes[68])?,
            is_restart,
        })
    }
}

/// Authenticated root commitment to governance lifecycle, cooldowns, parameters, and proposals.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GovernanceStateRootV1 {
    /// Root schema version (`1`).
    pub root_version: u16,
    /// Bound `PROFILE-D1` profile identifier.
    pub profile_id: [u8; 32],
    /// Current committed governance epoch.
    pub current_epoch: u64,
    /// Current lifecycle mode (`Normal`, `EmergencyPaused`, or `SafeHalted`).
    pub lifecycle: GovernanceLifecycleCodeV1,
    /// Auto-expiry epoch for active emergency pause (`0` when not paused).
    pub pause_until_epoch: u64,
    /// Earliest epoch at which the Emergency Council may invoke another pause (`0` if none).
    pub pause_cooldown_until_epoch: u64,
    /// Epoch when `SafeHalted` was entered (`0` when not halted).
    pub halted_epoch: u64,
    /// Proven fault digest anchoring `SafeHalted` (`[0; 32]` when not halted).
    pub active_fault_digest: [u8; 32],
    /// Number of currently `Queued` proposals.
    pub queued_proposal_count: u32,
    /// Number of committed active protocol parameter entries.
    pub active_param_count: u32,
    /// 32-byte Domain 5 Patricia root (`DomainIdV1::GovernanceState`).
    pub governance_trie_root: [u8; 32],
}

impl GovernanceStateRootV1 {
    /// Fixed canonical big-endian wire length (`139` bytes).
    pub const WIRE_LEN: usize = 2 + 32 + 8 + 1 + 8 + 8 + 8 + 32 + 4 + 4 + 32;

    /// Encodes this governance root commitment into 139 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], Governance1Error> {
        if self.root_version != 1 {
            return Err(Governance1Error::InvalidWire("root_version must equal 1"));
        }
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.root_version.to_be_bytes());
        out[2..34].copy_from_slice(&self.profile_id);
        out[34..42].copy_from_slice(&self.current_epoch.to_be_bytes());
        out[42] = self.lifecycle.code();
        out[43..51].copy_from_slice(&self.pause_until_epoch.to_be_bytes());
        out[51..59].copy_from_slice(&self.pause_cooldown_until_epoch.to_be_bytes());
        out[59..67].copy_from_slice(&self.halted_epoch.to_be_bytes());
        out[67..99].copy_from_slice(&self.active_fault_digest);
        out[99..103].copy_from_slice(&self.queued_proposal_count.to_be_bytes());
        out[103..107].copy_from_slice(&self.active_param_count.to_be_bytes());
        out[107..139].copy_from_slice(&self.governance_trie_root);
        Ok(out)
    }

    /// Decodes a 139-byte canonical [`GovernanceStateRootV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Governance1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(Governance1Error::InvalidWire(
                "GovernanceStateRootV1 must be 139 bytes",
            ));
        }
        let root_version = u16::from_be_bytes([bytes[0], bytes[1]]);
        if root_version != 1 {
            return Err(Governance1Error::InvalidWire("root_version must equal 1"));
        }
        Ok(Self {
            root_version,
            profile_id: bytes[2..34].try_into().expect("32"),
            current_epoch: u64::from_be_bytes(bytes[34..42].try_into().expect("8")),
            lifecycle: GovernanceLifecycleCodeV1::from_code(bytes[42])?,
            pause_until_epoch: u64::from_be_bytes(bytes[43..51].try_into().expect("8")),
            pause_cooldown_until_epoch: u64::from_be_bytes(bytes[51..59].try_into().expect("8")),
            halted_epoch: u64::from_be_bytes(bytes[59..67].try_into().expect("8")),
            active_fault_digest: bytes[67..99].try_into().expect("32"),
            queued_proposal_count: u32::from_be_bytes(bytes[99..103].try_into().expect("4")),
            active_param_count: u32::from_be_bytes(bytes[103..107].try_into().expect("4")),
            governance_trie_root: bytes[107..139].try_into().expect("32"),
        })
    }

    /// Returns the SHA-256 digest of the canonical encoded governance root bytes.
    pub fn canonical_sha256(&self) -> Result<[u8; 32], Governance1Error> {
        Ok(Sha256::digest(self.encode_canonical()?).into())
    }

    /// Computes the canonical `Receipt` `ContentTag` for this governance root.
    pub fn tag(&self) -> Result<ContentTag, Governance1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            GOVERNANCE_1_STATE_ROOT_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

/// Deterministic proof-carrying receipt emitted for every `GOVERNANCE-1` transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GovernanceTransitionReceiptV1 {
    /// Receipt schema version (`1`).
    pub receipt_version: u16,
    /// Bound `PROFILE-D1` profile identifier.
    pub profile_id: [u8; 32],
    /// Epoch at which the transition was evaluated.
    pub epoch: u64,
    /// Action category evaluated.
    pub action_kind: GovernanceActionKindV1,
    /// Deterministic disposition (`Admitted` or specific fail-closed rejection).
    pub disposition: GovernanceDispositionV1,
    /// Proposal ID, pause evidence digest, or proven fault digest bound by this transition.
    pub target_digest: [u8; 32],
    /// SHA-256 of canonical pre-transition [`GovernanceStateRootV1`].
    pub pre_root_sha256: [u8; 32],
    /// SHA-256 of canonical post-transition [`GovernanceStateRootV1`].
    pub post_root_sha256: [u8; 32],
}

impl GovernanceTransitionReceiptV1 {
    /// Fixed canonical big-endian wire length (`140` bytes).
    pub const WIRE_LEN: usize = 2 + 32 + 8 + 1 + 1 + 32 + 32 + 32;

    /// Encodes this governance transition receipt into 140 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], Governance1Error> {
        if self.receipt_version != 1 {
            return Err(Governance1Error::InvalidWire(
                "receipt_version must equal 1",
            ));
        }
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.receipt_version.to_be_bytes());
        out[2..34].copy_from_slice(&self.profile_id);
        out[34..42].copy_from_slice(&self.epoch.to_be_bytes());
        out[42] = self.action_kind.code();
        out[43] = self.disposition.code();
        out[44..76].copy_from_slice(&self.target_digest);
        out[76..108].copy_from_slice(&self.pre_root_sha256);
        out[108..140].copy_from_slice(&self.post_root_sha256);
        Ok(out)
    }

    /// Decodes a 140-byte canonical [`GovernanceTransitionReceiptV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Governance1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(Governance1Error::InvalidWire(
                "GovernanceTransitionReceiptV1 must be 140 bytes",
            ));
        }
        let receipt_version = u16::from_be_bytes([bytes[0], bytes[1]]);
        if receipt_version != 1 {
            return Err(Governance1Error::InvalidWire(
                "receipt_version must equal 1",
            ));
        }
        Ok(Self {
            receipt_version,
            profile_id: bytes[2..34].try_into().expect("32"),
            epoch: u64::from_be_bytes(bytes[34..42].try_into().expect("8")),
            action_kind: GovernanceActionKindV1::from_code(bytes[42])?,
            disposition: GovernanceDispositionV1::from_code(bytes[43])?,
            target_digest: bytes[44..76].try_into().expect("32"),
            pre_root_sha256: bytes[76..108].try_into().expect("32"),
            post_root_sha256: bytes[108..140].try_into().expect("32"),
        })
    }

    /// Computes the canonical `Receipt` `ContentTag` for this transition receipt.
    pub fn tag(&self) -> Result<ContentTag, Governance1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            GOVERNANCE_1_RECEIPT_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

/// Proposal submission signed by `> 2/3` active bonded validator stake.
#[derive(Clone, Debug)]
pub struct GovernanceProposalSubmissionV1 {
    /// Unique 32-byte proposal identifier.
    pub proposal_id: [u8; 32],
    /// Parameter slot (`0` for Safe-Halt stake restart; `>= 1` for protocol parameters).
    pub param_key: u32,
    /// Proposed new numeric parameter value.
    pub param_value: u64,
    /// Target future activation epoch (must satisfy `>= current_epoch + timelock`).
    pub activation_epoch: u64,
    /// Expiration epoch (must satisfy `>= activation_epoch`).
    pub expiry_epoch: u64,
    /// `true` iff this proposal is a stake-only Safe-Halt restart.
    pub is_restart: bool,
    /// Validator-set root bound into the approval signatures (rejects stale sets).
    pub submitted_validator_root: [u8; 32],
    /// Validator `(validator_id, ed25519_sig)` approvals over [`proposal_approval_digest`].
    pub approvals: Vec<(ActorIdV1, CanonicalEd25519SignatureV1)>,
}

/// Deterministic authenticated governance state machine (`DomainIdV1::GovernanceState`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GovernanceStateV1 {
    context: GovernanceContextV1,
    policy: GovernancePolicyDescriptorV1,
    current_epoch: u64,
    lifecycle: GovernanceLifecycleCodeV1,
    pause_until_epoch: u64,
    pause_cooldown_until_epoch: u64,
    halted_epoch: u64,
    active_fault_digest: [u8; 32],
    params: BTreeMap<u32, u64>,
    proposals: BTreeMap<[u8; 32], ProposalRecordV1>,
    revoked_keys: BTreeSet<CanonicalEd25519PublicKeyV1>,
    trie: DomainPatriciaTrieV1,
}

impl GovernanceStateV1 {
    /// Initializes genesis governance state with initial parameter slots and validates `policy`.
    pub fn from_genesis(
        context: GovernanceContextV1,
        policy: GovernancePolicyDescriptorV1,
        initial_params: &[(u32, u64)],
    ) -> Result<Self, Governance1Error> {
        policy.validate(None)?;
        let mut params = BTreeMap::new();
        let mut pairs = Vec::with_capacity(initial_params.len());
        for &(k, v) in initial_params {
            if k == 0 || params.insert(k, v).is_some() {
                return Err(Governance1Error::InvalidWire(
                    "initial param keys must be unique and non-zero",
                ));
            }
            pairs.push((param_trie_key(k), v.to_be_bytes().to_vec()));
        }
        let trie = DomainPatriciaTrieV1::from_entries(DomainIdV1::GovernanceState, pairs)?;
        Ok(Self {
            context,
            policy,
            current_epoch: 0,
            lifecycle: GovernanceLifecycleCodeV1::Normal,
            pause_until_epoch: 0,
            pause_cooldown_until_epoch: 0,
            halted_epoch: 0,
            active_fault_digest: [0u8; 32],
            params,
            proposals: BTreeMap::new(),
            revoked_keys: BTreeSet::new(),
            trie,
        })
    }

    /// Marks a compromised public key as permanently revoked so future signatures fail closed.
    pub fn revoke_compromised_key(&mut self, key: CanonicalEd25519PublicKeyV1) {
        self.revoked_keys.insert(key);
    }

    /// Returns the active value for governed parameter `param_key`, if set.
    pub fn param_value(&self, param_key: u32) -> Option<u64> {
        self.params.get(&param_key).copied()
    }

    /// Returns the current lifecycle mode.
    pub const fn lifecycle(&self) -> GovernanceLifecycleCodeV1 {
        self.lifecycle
    }

    /// Returns the current committed [`GovernanceStateRootV1`].
    pub fn root_commitment(&self) -> GovernanceStateRootV1 {
        let queued_proposal_count = self
            .proposals
            .values()
            .filter(|p| p.status == ProposalStatusV1::Queued)
            .count() as u32;
        GovernanceStateRootV1 {
            root_version: 1,
            profile_id: self.context.profile_id,
            current_epoch: self.current_epoch,
            lifecycle: self.lifecycle,
            pause_until_epoch: self.pause_until_epoch,
            pause_cooldown_until_epoch: self.pause_cooldown_until_epoch,
            halted_epoch: self.halted_epoch,
            active_fault_digest: self.active_fault_digest,
            queued_proposal_count,
            active_param_count: self.params.len() as u32,
            governance_trie_root: self.trie.root(),
        }
    }

    /// Generates an inclusion or non-membership proof for parameter slot `param_key` (Domain 5).
    pub fn prove_param(&self, param_key: u32) -> Result<DomainPatriciaProofV1, Governance1Error> {
        Ok(self.trie.prove(&param_trie_key(param_key))?)
    }

    /// Generates an inclusion or non-membership proof for `proposal_id` (Domain 5).
    pub fn prove_proposal(
        &self,
        proposal_id: &[u8; 32],
    ) -> Result<DomainPatriciaProofV1, Governance1Error> {
        Ok(self.trie.prove(&proposal_trie_key(proposal_id))?)
    }

    /// Verifies cold rebuild parity of the Domain 5 governance Patricia trie.
    pub fn verify_rebuild_parity(&self) -> Result<(), Governance1Error> {
        Ok(self.trie.verify_rebuild_parity()?)
    }

    /// Advances the governance epoch monotonically, auto-expiring elapsed emergency pauses.
    pub fn advance_epoch(&mut self, next_epoch: u64) -> Result<(), Governance1Error> {
        if next_epoch < self.current_epoch {
            return Err(Governance1Error::InvalidWire(
                "cannot roll back finalized governance epoch",
            ));
        }
        self.current_epoch = next_epoch;
        if self.lifecycle == GovernanceLifecycleCodeV1::EmergencyPaused
            && next_epoch >= self.pause_until_epoch
        {
            self.lifecycle = GovernanceLifecycleCodeV1::Normal;
            self.pause_until_epoch = 0;
        }
        Ok(())
    }

    fn emit_receipt(
        &self,
        epoch: u64,
        action_kind: GovernanceActionKindV1,
        disposition: GovernanceDispositionV1,
        target_digest: [u8; 32],
        pre_sha: [u8; 32],
    ) -> Result<GovernanceTransitionReceiptV1, Governance1Error> {
        let post_sha = self.root_commitment().canonical_sha256()?;
        Ok(GovernanceTransitionReceiptV1 {
            receipt_version: 1,
            profile_id: self.context.profile_id,
            epoch,
            action_kind,
            disposition,
            target_digest,
            pre_root_sha256: pre_sha,
            post_root_sha256: post_sha,
        })
    }

    /// Verifies and queues a `> 2/3` stake-approved timelocked parameter or Safe-Halt restart proposal.
    pub fn queue_proposal(
        &mut self,
        validator_set: &InitialValidatorSetV1,
        sub: &GovernanceProposalSubmissionV1,
        epoch: u64,
    ) -> Result<GovernanceTransitionReceiptV1, Governance1Error> {
        let pre_sha = self.root_commitment().canonical_sha256()?;
        if epoch < self.current_epoch {
            return self.emit_receipt(
                epoch,
                GovernanceActionKindV1::QueueProposal,
                GovernanceDispositionV1::RejectedHistoryRewrite,
                sub.proposal_id,
                pre_sha,
            );
        }
        let mut staged = self.clone();
        staged.advance_epoch(epoch)?;

        // While SafeHalted, ONLY stake-only Safe-Halt restart proposals are permitted.
        if staged.lifecycle == GovernanceLifecycleCodeV1::SafeHalted && !sub.is_restart {
            return self.emit_receipt(
                epoch,
                GovernanceActionKindV1::QueueProposal,
                GovernanceDispositionV1::RejectedWhileHaltedOrInvalidFault,
                sub.proposal_id,
                pre_sha,
            );
        }
        if !sub.is_restart && sub.param_key == 0 {
            return self.emit_receipt(
                epoch,
                GovernanceActionKindV1::QueueProposal,
                GovernanceDispositionV1::RejectedUnauthorized,
                sub.proposal_id,
                pre_sha,
            );
        }
        if sub.submitted_validator_root != validator_set.active_validator_root() {
            return self.emit_receipt(
                epoch,
                GovernanceActionKindV1::QueueProposal,
                GovernanceDispositionV1::RejectedStaleValidatorSet,
                sub.proposal_id,
                pre_sha,
            );
        }
        if staged.proposals.contains_key(&sub.proposal_id) {
            return self.emit_receipt(
                epoch,
                GovernanceActionKindV1::QueueProposal,
                GovernanceDispositionV1::RejectedReplayedProposal,
                sub.proposal_id,
                pre_sha,
            );
        }
        if staged.proposals.values().any(|p| {
            p.status == ProposalStatusV1::Queued
                && p.param_key == sub.param_key
                && p.is_restart == sub.is_restart
        }) {
            return self.emit_receipt(
                epoch,
                GovernanceActionKindV1::QueueProposal,
                GovernanceDispositionV1::RejectedConflictingProposal,
                sub.proposal_id,
                pre_sha,
            );
        }
        let earliest = epoch.saturating_add(staged.policy.mandatory_exit_timelock_epochs);
        if sub.activation_epoch < earliest {
            return self.emit_receipt(
                epoch,
                GovernanceActionKindV1::QueueProposal,
                GovernanceDispositionV1::RejectedRushedTimelock,
                sub.proposal_id,
                pre_sha,
            );
        }
        if sub.expiry_epoch < sub.activation_epoch {
            return self.emit_receipt(
                epoch,
                GovernanceActionKindV1::QueueProposal,
                GovernanceDispositionV1::RejectedExpiredProposal,
                sub.proposal_id,
                pre_sha,
            );
        }
        let digest = proposal_approval_digest(
            &sub.submitted_validator_root,
            &sub.proposal_id,
            sub.param_key,
            sub.param_value,
            epoch,
            sub.activation_epoch,
            sub.expiry_epoch,
            sub.is_restart,
        );
        let preimage = staged
            .context
            .preimage(SigningDomainTagV1::GovernanceAction, digest);
        let mut seen = BTreeSet::new();
        let mut signed_weight = 0u128;
        for &(vid, sig) in &sub.approvals {
            if !seen.insert(vid) {
                return self.emit_receipt(
                    epoch,
                    GovernanceActionKindV1::QueueProposal,
                    GovernanceDispositionV1::RejectedUnauthorized,
                    sub.proposal_id,
                    pre_sha,
                );
            }
            let Some(desc) = validator_set.find(&vid) else {
                return self.emit_receipt(
                    epoch,
                    GovernanceActionKindV1::QueueProposal,
                    GovernanceDispositionV1::RejectedUnauthorized,
                    sub.proposal_id,
                    pre_sha,
                );
            };
            if staged.revoked_keys.contains(&desc.vote_key) {
                return self.emit_receipt(
                    epoch,
                    GovernanceActionKindV1::QueueProposal,
                    GovernanceDispositionV1::RejectedCompromisedKey,
                    sub.proposal_id,
                    pre_sha,
                );
            }
            if verify_preimage_strict(&desc.vote_key, &preimage, &sig).is_err() {
                return self.emit_receipt(
                    epoch,
                    GovernanceActionKindV1::QueueProposal,
                    GovernanceDispositionV1::RejectedUnauthorized,
                    sub.proposal_id,
                    pre_sha,
                );
            }
            signed_weight = signed_weight.saturating_add(desc.weight);
        }
        if !validator_set.meets_strict_supermajority(signed_weight)? {
            return self.emit_receipt(
                epoch,
                GovernanceActionKindV1::QueueProposal,
                GovernanceDispositionV1::RejectedUnauthorized,
                sub.proposal_id,
                pre_sha,
            );
        }

        let rec = ProposalRecordV1 {
            proposal_id: sub.proposal_id,
            param_key: sub.param_key,
            param_value: sub.param_value,
            approved_epoch: epoch,
            activation_epoch: sub.activation_epoch,
            expiry_epoch: sub.expiry_epoch,
            status: ProposalStatusV1::Queued,
            is_restart: sub.is_restart,
        };
        staged.proposals.insert(sub.proposal_id, rec);
        staged.trie.upsert(
            proposal_trie_key(&sub.proposal_id),
            rec.encode_canonical().to_vec(),
        );
        *self = staged;
        self.emit_receipt(
            epoch,
            GovernanceActionKindV1::QueueProposal,
            GovernanceDispositionV1::Admitted,
            sub.proposal_id,
            pre_sha,
        )
    }

    /// Activates a queued proposal once `epoch >= activation_epoch` and `epoch <= expiry_epoch`.
    pub fn activate_proposal(
        &mut self,
        proposal_id: [u8; 32],
        epoch: u64,
    ) -> Result<GovernanceTransitionReceiptV1, Governance1Error> {
        let pre_sha = self.root_commitment().canonical_sha256()?;
        if epoch < self.current_epoch {
            return self.emit_receipt(
                epoch,
                GovernanceActionKindV1::ActivateProposal,
                GovernanceDispositionV1::RejectedHistoryRewrite,
                proposal_id,
                pre_sha,
            );
        }
        let mut staged = self.clone();
        staged.advance_epoch(epoch)?;
        let Some(mut rec) = staged.proposals.get(&proposal_id).copied() else {
            return self.emit_receipt(
                epoch,
                GovernanceActionKindV1::ActivateProposal,
                GovernanceDispositionV1::RejectedUnauthorized,
                proposal_id,
                pre_sha,
            );
        };
        if rec.status != ProposalStatusV1::Queued {
            return self.emit_receipt(
                epoch,
                GovernanceActionKindV1::ActivateProposal,
                GovernanceDispositionV1::RejectedReplayedProposal,
                proposal_id,
                pre_sha,
            );
        }
        if epoch < rec.activation_epoch {
            return self.emit_receipt(
                epoch,
                GovernanceActionKindV1::ActivateProposal,
                GovernanceDispositionV1::RejectedRushedTimelock,
                proposal_id,
                pre_sha,
            );
        }
        if epoch > rec.expiry_epoch {
            return self.emit_receipt(
                epoch,
                GovernanceActionKindV1::ActivateProposal,
                GovernanceDispositionV1::RejectedExpiredProposal,
                proposal_id,
                pre_sha,
            );
        }
        if staged.lifecycle == GovernanceLifecycleCodeV1::SafeHalted && !rec.is_restart {
            return self.emit_receipt(
                epoch,
                GovernanceActionKindV1::ActivateProposal,
                GovernanceDispositionV1::RejectedWhileHaltedOrInvalidFault,
                proposal_id,
                pre_sha,
            );
        }
        rec.status = ProposalStatusV1::Activated;
        staged.proposals.insert(proposal_id, rec);
        staged.trie.upsert(
            proposal_trie_key(&proposal_id),
            rec.encode_canonical().to_vec(),
        );
        if rec.is_restart {
            staged.lifecycle = GovernanceLifecycleCodeV1::Normal;
            staged.halted_epoch = 0;
            staged.active_fault_digest = [0u8; 32];
            staged.pause_until_epoch = 0;
        } else {
            staged.params.insert(rec.param_key, rec.param_value);
            staged.trie.upsert(
                param_trie_key(rec.param_key),
                rec.param_value.to_be_bytes().to_vec(),
            );
        }
        *self = staged;
        self.emit_receipt(
            epoch,
            GovernanceActionKindV1::ActivateProposal,
            GovernanceDispositionV1::Admitted,
            proposal_id,
            pre_sha,
        )
    }

    /// Triggers a bounded, auto-expiring emergency pause with mandatory anti-DoS cooldown.
    pub fn trigger_emergency_pause(
        &mut self,
        epoch: u64,
        duration_epochs: u64,
        evidence_digest: [u8; 32],
        council_signatures: &[(CanonicalEd25519PublicKeyV1, CanonicalEd25519SignatureV1)],
    ) -> Result<GovernanceTransitionReceiptV1, Governance1Error> {
        let pre_sha = self.root_commitment().canonical_sha256()?;
        if epoch < self.current_epoch {
            return self.emit_receipt(
                epoch,
                GovernanceActionKindV1::EmergencyPause,
                GovernanceDispositionV1::RejectedHistoryRewrite,
                evidence_digest,
                pre_sha,
            );
        }
        let mut staged = self.clone();
        staged.advance_epoch(epoch)?;
        if staged.lifecycle == GovernanceLifecycleCodeV1::SafeHalted {
            return self.emit_receipt(
                epoch,
                GovernanceActionKindV1::EmergencyPause,
                GovernanceDispositionV1::RejectedEmergencyOverreach,
                evidence_digest,
                pre_sha,
            );
        }
        if duration_epochs == 0
            || duration_epochs > staged.policy.max_emergency_pause_epochs
            || staged.lifecycle == GovernanceLifecycleCodeV1::EmergencyPaused
            || epoch < staged.pause_cooldown_until_epoch
        {
            return self.emit_receipt(
                epoch,
                GovernanceActionKindV1::EmergencyPause,
                GovernanceDispositionV1::RejectedPauseCooldownDoS,
                evidence_digest,
                pre_sha,
            );
        }
        for (k, _) in council_signatures {
            if staged.revoked_keys.contains(k) {
                return self.emit_receipt(
                    epoch,
                    GovernanceActionKindV1::EmergencyPause,
                    GovernanceDispositionV1::RejectedCompromisedKey,
                    evidence_digest,
                    pre_sha,
                );
            }
        }
        let digest = emergency_pause_digest(epoch, duration_epochs, &evidence_digest);
        let preimage = staged
            .context
            .preimage(SigningDomainTagV1::EmergencyPause, digest);
        if staged
            .policy
            .emergency_council
            .verify_threshold_signatures(
                CredentialRoleV1::EmergencyPauseSigner,
                &preimage,
                council_signatures,
            )
            .is_err()
        {
            return self.emit_receipt(
                epoch,
                GovernanceActionKindV1::EmergencyPause,
                GovernanceDispositionV1::RejectedUnauthorized,
                evidence_digest,
                pre_sha,
            );
        }
        let pause_until = epoch.saturating_add(duration_epochs);
        staged.lifecycle = GovernanceLifecycleCodeV1::EmergencyPaused;
        staged.pause_until_epoch = pause_until;
        staged.pause_cooldown_until_epoch =
            pause_until.saturating_add(staged.policy.max_emergency_pause_epochs);
        *self = staged;
        self.emit_receipt(
            epoch,
            GovernanceActionKindV1::EmergencyPause,
            GovernanceDispositionV1::Admitted,
            evidence_digest,
            pre_sha,
        )
    }

    /// Triggers an automatic deterministic Safe-Halt from proven `BOUNDS-1` `SafeHaltEvidenceV1`.
    pub fn trigger_safe_halt_from_bounds_evidence(
        &mut self,
        evidence: &SafeHaltEvidenceV1,
        epoch: u64,
    ) -> Result<GovernanceTransitionReceiptV1, Governance1Error> {
        let pre_sha = self.root_commitment().canonical_sha256()?;
        let fault_digest = evidence.tag().digest();
        if epoch < self.current_epoch {
            return self.emit_receipt(
                epoch,
                GovernanceActionKindV1::AutomaticSafeHalt,
                GovernanceDispositionV1::RejectedHistoryRewrite,
                fault_digest,
                pre_sha,
            );
        }
        if evidence.schema_version != 1 || evidence.observed_value <= evidence.committed_limit {
            return self.emit_receipt(
                epoch,
                GovernanceActionKindV1::AutomaticSafeHalt,
                GovernanceDispositionV1::RejectedWhileHaltedOrInvalidFault,
                fault_digest,
                pre_sha,
            );
        }
        let mut staged = self.clone();
        staged.advance_epoch(epoch)?;
        staged.lifecycle = GovernanceLifecycleCodeV1::SafeHalted;
        staged.halted_epoch = epoch;
        staged.active_fault_digest = fault_digest;
        staged.pause_until_epoch = 0;
        *self = staged;
        self.emit_receipt(
            epoch,
            GovernanceActionKindV1::AutomaticSafeHalt,
            GovernanceDispositionV1::Admitted,
            fault_digest,
            pre_sha,
        )
    }

    /// Evaluates a forbidden Emergency Council state-mutation / unhalt attempt (always fails closed).
    pub fn attempt_emergency_overreach(
        &self,
        epoch: u64,
        attempt_digest: [u8; 32],
    ) -> Result<GovernanceTransitionReceiptV1, Governance1Error> {
        let pre_sha = self.root_commitment().canonical_sha256()?;
        self.emit_receipt(
            epoch,
            GovernanceActionKindV1::ForbiddenAttempt,
            GovernanceDispositionV1::RejectedEmergencyOverreach,
            attempt_digest,
            pre_sha,
        )
    }

    /// Evaluates a forbidden finalized-history rewrite or rollback attempt (always fails closed).
    pub fn attempt_history_rewrite(
        &self,
        epoch: u64,
        attempt_digest: [u8; 32],
    ) -> Result<GovernanceTransitionReceiptV1, Governance1Error> {
        let pre_sha = self.root_commitment().canonical_sha256()?;
        self.emit_receipt(
            epoch,
            GovernanceActionKindV1::ForbiddenAttempt,
            GovernanceDispositionV1::RejectedHistoryRewrite,
            attempt_digest,
            pre_sha,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consensus_gov_d1::InitialValidatorDescriptorV1;
    use crate::crypto_d1::sign_preimage_deterministic;
    use crate::identity_d1::ThresholdPolicyV1;

    fn pk(seed: u8) -> CanonicalEd25519PublicKeyV1 {
        CanonicalEd25519PublicKeyV1::from_secret_seed(&[seed; 32]).unwrap()
    }

    #[test]
    fn governance_1_codec_timelock_pause_safehalt_and_golden_tags() {
        let ctx = GovernanceContextV1 {
            protocol_version: 1,
            chain_id: 77,
            genesis_id: [0x11; 32],
            launch_manifest_id: [0x22; 32],
            profile_id: [0x55; 32],
        };
        let mut council_keys = vec![pk(81), pk(82), pk(83)];
        council_keys.sort();
        let council = ThresholdPolicyV1 {
            keys: council_keys,
            threshold: 2,
            role: CredentialRoleV1::EmergencyPauseSigner,
        };
        let (q_num, q_den, timelock, max_pause) = (2u16, 3u16, 21u64, 3u64);
        let yes = true;
        let policy = GovernancePolicyDescriptorV1 {
            schema_version: 1,
            upgrade_quorum_numerator: q_num,
            upgrade_quorum_denominator: q_den,
            mandatory_exit_timelock_epochs: timelock,
            max_emergency_pause_epochs: max_pause,
            automatic_safe_halt_on_proven_fault: yes,
            emergency_council_pause_only: yes,
            forbid_history_rewrite: yes,
            emergency_council: council,
        };
        let vals: Vec<_> = (1u8..=4)
            .map(|i| InitialValidatorDescriptorV1 {
                weight: 100,
                proposal_key: pk(20 + i),
                vote_key: pk(10 + i),
                validator_id: ActorIdV1::from_bytes([i; 32]),
            })
            .collect();
        let vset = InitialValidatorSetV1::new(vals).unwrap();
        let mut gov = GovernanceStateV1::from_genesis(ctx, policy, &[(1, 50_000_000)]).unwrap();

        // 1. Queue param_key=1 -> 60_000_000 at epoch 1, activation_epoch=22, expiry=30
        let pid = [0xAA; 32];
        let digest = proposal_approval_digest(
            &vset.active_validator_root(),
            &pid,
            1,
            60_000_000,
            1,
            22,
            30,
            false,
        );
        let pre = ctx.preimage(SigningDomainTagV1::GovernanceAction, digest);
        let approvals: Vec<_> = (1u8..=3)
            .map(|i| {
                (
                    ActorIdV1::from_bytes([i; 32]),
                    sign_preimage_deterministic(&[10 + i; 32], &pre).unwrap(),
                )
            })
            .collect();
        let rcpt_q = gov
            .queue_proposal(
                &vset,
                &GovernanceProposalSubmissionV1 {
                    proposal_id: pid,
                    param_key: 1,
                    param_value: 60_000_000,
                    activation_epoch: 22,
                    expiry_epoch: 30,
                    is_restart: false,
                    submitted_validator_root: vset.active_validator_root(),
                    approvals,
                },
                1,
            )
            .unwrap();
        assert_eq!(rcpt_q.disposition, GovernanceDispositionV1::Admitted);
        let queued_root = gov.root_commitment();
        assert_eq!(
            GovernanceStateRootV1::decode_canonical(&queued_root.encode_canonical().unwrap())
                .unwrap(),
            queued_root
        );
        assert_eq!(
            GovernanceTransitionReceiptV1::decode_canonical(&rcpt_q.encode_canonical().unwrap())
                .unwrap(),
            rcpt_q
        );

        // 2. Activate at epoch 22
        let rcpt_act = gov.activate_proposal(pid, 22).unwrap();
        assert_eq!(rcpt_act.disposition, GovernanceDispositionV1::Admitted);
        assert_eq!(gov.param_value(1), Some(60_000_000));
        let act_root = gov.root_commitment();

        // 3. Emergency pause at epoch 25 for 3 epochs (until 28, cooldown until 31)
        let ev = [0xEE; 32];
        let p_dig = emergency_pause_digest(25, 3, &ev);
        let p_pre = ctx.preimage(SigningDomainTagV1::EmergencyPause, p_dig);
        let c_sigs = vec![
            (
                pk(81),
                sign_preimage_deterministic(&[81; 32], &p_pre).unwrap(),
            ),
            (
                pk(82),
                sign_preimage_deterministic(&[82; 32], &p_pre).unwrap(),
            ),
        ];
        let rcpt_p = gov.trigger_emergency_pause(25, 3, ev, &c_sigs).unwrap();
        assert_eq!(rcpt_p.disposition, GovernanceDispositionV1::Admitted);
        let paused_root = gov.root_commitment();

        // 4. Automatic Safe-Halt at epoch 35
        let sh_ev = SafeHaltEvidenceV1 {
            schema_version: 1,
            bounds_profile_digest: [0x77; 32],
            artifact_digest: [0x88; 32],
            violation_code: 3,
            observed_value: 20_000,
            committed_limit: 12_288,
        };
        let rcpt_sh = gov
            .trigger_safe_halt_from_bounds_evidence(&sh_ev, 35)
            .unwrap();
        assert_eq!(rcpt_sh.disposition, GovernanceDispositionV1::Admitted);
        let halted_root = gov.root_commitment();
        gov.verify_rebuild_parity().unwrap();

        // Golden tags pinned against independent tools/governance_1_vectors.py
        assert_eq!(
            queued_root.tag().unwrap().to_string(),
            "#bb.receipt.sha256.ro25qjtz5x6jglbdtk4pzqtlm5tqwfkxfpod53hn2amne7otuhxq"
        );
        assert_eq!(
            act_root.tag().unwrap().to_string(),
            "#bb.receipt.sha256.u5vnfvqypcrivjeu7d7jrsgfeaottqchqap6dxosvlksqdry5f2q"
        );
        assert_eq!(
            paused_root.tag().unwrap().to_string(),
            "#bb.receipt.sha256.6zrhhqdbeao6dhzcbotcfxyfiolbmrbj2wvoc4axkk2owomfpb6q"
        );
        assert_eq!(
            halted_root.tag().unwrap().to_string(),
            "#bb.receipt.sha256.hn4fvvlm3baxpa6ycedxmisfz4zx32j55r2sna5enjzzxfqx4tnq"
        );
        assert_eq!(
            rcpt_q.tag().unwrap().to_string(),
            "#bb.receipt.sha256.lwhmyl2rjlhlpflygohdh2rpiccdeksimz75mjdo23jdqtktp5ha"
        );
    }
}
