//! `CONSENSUS-1@base`: deterministic-finality 2-phase HotStuff PoS BFT engine
//! (`HotStuffTwoPhaseBftPosV1`), live `AcceptedConsensusNativeParentAuthorityCertificateV1`
//! issuance/admission, 12-trace adversarial harness, and native loopback TCP 4-validator roundtrip
//! (`PS-056`).
//!
//! Realizes every normative invariant in `docs/CAPABILITY_TREE.md` §`CONSENSUS-1` and
//! `docs/LAYER1_DESIGN_PAPER.md` §575–745 without modifying `crates/core/**`, `sync_1.rs`,
//! `storage.rs`, `journal.rs`, `commit_state.rs`, or `Cargo.lock`, paired with the machine-checked
//! Lean 4 `--trust=0` formal safety module `proof/BitblockProof/ConsensusSafety.lean`:
//! - **Typed roles & strict `> 2/3` weighted QCs:** consumes immutable
//!   [`EpochValidatorScheduleV1`] from `VALIDATOR-SCHEDULE-1`, enforcing `proposal_key`
//!   (`SigningDomainTagV1::BlockProposal`) vs `vote_key` (`SigningDomainTagV1::ConsensusVote`)
//!   role separation, strict `3 * signed_weight > 2 * total_weight`, and long-range stale-schedule
//!   rejection.
//! - **Data-before-vote & execution binding:** every honest validator executes the complete
//!   [`MultiActorStreamBundleV1`] body via [`MultiActorHarnessV1`] + `EXECUTION-ROOT-1` before
//!   casting any `Prepare` or `Commit` vote (`DataBeforeVoteRefusal` on withheld body).
//! - **2-phase locking & Safe-Halt:** honest validators vote at most once per `(height, view,
//!   phase)`, lock on `PrepareQC`, finalize on `CommitQC` into [`ConsensusCommitCertificateV1`],
//!   and emit automatic [`ConsensusSafeHaltEvidenceV1`] on any conflicting same-height finality.
//! - **Live parent-authority certificate admission:** issues and admits
//!   [`AcceptedConsensusNativeParentAuthorityCertificateV1`] with certificate-independent
//!   consumer context (`ReleaseAuthority` | `ActivationAuthority`), exact three-anchor
//!   [`VerifiedConsensusNativeAcceptedRecordOccurrenceHistoryStartEqualityV1`], total decoded
//!   wrapper coverage including conflict material, lossless predecessor-closure lower-bound checks
//!   (rejecting late-valid-suffix and candidate-advanced-rule admissions), unique live slot
//!   enforcement per `(parent_core_id, purpose, required_position)`, monotonic post-acceptance
//!   status upgrade, and strict refusal of replay-archive inputs in live admission.
//!
//! Non-claim: bounded in-process and loopback simulation plus an abstract Lean 4 model do not prove
//! live-network Byzantine liveness, independent multi-process deployment safety, signer custody, or
//! Sybil resistance (`TESTNET-1`, `SIGNER-OPS-1`, and `PROFILE-1` remain separate gates).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use sha2::{Digest, Sha256};

use crate::crypto_d1::{
    CanonicalEd25519PublicKeyV1, CanonicalEd25519SignatureV1, HashSuiteIdV1, SignatureSuiteIdV1,
    SigningDomainTagV1, SigningPreimageV1, sign_preimage_deterministic, verify_preimage_strict,
};
use crate::economic_state_1::EconomicStateV1;
use crate::execution_root_1::ExecutionRootRefV1;
use crate::executor_1::ExecutorForkContextV1;
use crate::governance_1::{GovernanceContextV1, GovernanceStateV1};
use crate::identity_d1::ActorIdV1;
use crate::multi_1::{
    CanonicalMultiActorFixtureV1, Multi1Error, MultiActorFoldReceiptV1, MultiActorHarnessV1,
    MultiActorStreamBundleV1,
};
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::protocol_id_1::{
    AcceptedProposalWrapperV1, CanonicalAcceptedProposalCoreV1, ProposalCredentialEntryV1,
    ProtocolId1Error, SameWrapperAvailabilityDependencyV1, normalize_proposal_wrappers,
    profile_id_hash_parent_lookup,
};
use crate::sequencer_1::{
    Sequencer1Error, compute_batch_body_digest, compute_receipts_merkle_root,
};
use crate::sync_1::{OpaqueAuthorityBlobV1, OpaqueBlobKindV1, Sync1Error};
use crate::validator_schedule_1::{
    EpochValidatorScheduleV1, ScheduledValidatorEntryV1, ValidatorKeyPairV1,
    ValidatorSchedule1Error, ValidatorScheduleStateV1,
};

/// Canonical schema string for a finalized [`ConsensusCommitCertificateV1`] (`CONSENSUS-1`).
pub const CONSENSUS_1_COMMIT_CERT_SCHEMA: &[u8] = b"bitblock/consensus-1/bft-commit-certificate/v1";
/// Canonical schema string for an admitted [`AcceptedConsensusNativeParentAuthorityCertificateV1`].
pub const CONSENSUS_1_PARENT_AUTH_CERT_SCHEMA: &[u8] =
    b"bitblock/consensus-1/parent-authority-certificate/v1";

/// Protocol version encoded in all `CONSENSUS-1` proposals, votes, QCs, and certificates.
pub const CONSENSUS_1_VERSION: u16 = 1;
/// Fixed 6-byte wire magic for native loopback BFT message framing (`BBCON1`).
pub const CONS_WIRE_MAGIC: [u8; 6] = *b"BBCON1";

const CONS_PROP_PAYLOAD_PREFIX: &[u8] = b"bb.cons1.proposal.v1";
const CONS_BLOCK_ID_PREFIX: &[u8] = b"bb.cons1.block_id.v1";
const CONS_VOTE_PAYLOAD_PREFIX: &[u8] = b"bb.cons1.vote.v1";
const CONS_QC_DIGEST_PREFIX: &[u8] = b"bb.cons1.qc_digest.v1";
const CONS_PARENT_LOOKUP_PREFIX: &[u8] = b"bb.cons1.parent_lookup.v1";
const CONS_CERT_OCC_PREFIX: &[u8] = b"bb.cons1.occ_rcpt.v1";
const CONS_CERT_UNIV_PREFIX: &[u8] = b"bb.cons1.occ_univ.v1";
const CONS_CERT_EVID_PREFIX: &[u8] = b"bb.cons1.cert_evid.v1";
const CONS_CERT_SIGN_PREFIX: &[u8] = b"bb.cons1.cert_sign.v1";
const CONS_CERT_ADMIT_PREFIX: &[u8] = b"bb.cons1.cert_admit.v1";
const CONS_EXEC_ROOT_DIGEST_PREFIX: &[u8] = b"bb.cons1.exec_root.v1";

/// Fail-closed validation, quorum, locking, safe-halt, certificate, or transport error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Consensus1Error {
    /// Wire buffer had invalid version, length, sorting, discriminant, or trailing bytes.
    InvalidWire(&'static str),
    /// Active validator schedule was missing, malformed, or violated `< 1/3` BFT bounds.
    InvalidSchedule(&'static str),
    /// Strict RFC 8032 Ed25519 signature or signer identity verification failed.
    InvalidSignature(&'static str),
    /// Validator attempted to sign a proposal with its `vote_key` or a vote with its `proposal_key`.
    CrossRoleKeyReuse(&'static str),
    /// Proposal, vote, or QC referenced a stale or mismatched historical validator schedule.
    LongRangeStaleSchedule(&'static str),
    /// Validator refused to vote because complete block body or matching execution was absent.
    DataBeforeVoteRefusal(&'static str),
    /// Quorum certificate lacked strict `> 2/3` epoch voting weight (`3 * signed > 2 * total`).
    SubQuorumWeight {
        /// Sum of verified signer weights.
        signed_weight: u128,
        /// Total active epoch weight.
        total_weight: u128,
    },
    /// Validator locked on a higher or conflicting `PrepareQC` refused an unsafe proposal.
    LockSafetyViolation(&'static str),
    /// Conflicting votes from one validator at the same `(height, epoch, view, phase)`.
    VoteEquivocationDetected(Box<VoteEquivocationEvidenceV1>),
    /// Conflicting finalized `CommitQC` certificates observed at the same height (automatic halt).
    ConflictingFinalitySafeHalt(Box<ConsensusSafeHaltEvidenceV1>),
    /// Replay-archive-sourced evidence attempted to enter live parent-authority admission.
    ReplayInputForbiddenInLiveAdmission,
    /// Three-anchor history-start equality check failed exact byte comparison.
    HistoryStartMismatch(&'static str),
    /// Evidence cutoff fell before earliest unbounded valid occurrence (late-valid-suffix).
    LateValidSuffixRejected,
    /// Evidence cutoff fell before candidate rule activation (candidate-advanced-rule).
    CandidateAdvancedRuleRejected,
    /// Duplicate or conflicting live admission for an occupied `(parent_core, purpose, pos)` slot.
    DuplicateLiveAdmissionRejected,
    /// Parent-authority certificate failed coverage, status, or policy verification.
    CertificateContractViolation(&'static str),
    /// Underlying `MULTI-1` execution or fixture error.
    Multi(Multi1Error),
    /// Underlying `SEQUENCER-1` receipt-root error.
    Sequencer(Sequencer1Error),
    /// Underlying `PROTOCOL-ID-1` core/wrapper error.
    ProtocolId(ProtocolId1Error),
    /// Underlying `VALIDATOR-SCHEDULE-1` error.
    ValidatorSchedule(ValidatorSchedule1Error),
    /// Underlying `SYNC-1` opaque blob error.
    Sync(Sync1Error),
    /// Native loopback TCP I/O or preflight framing error.
    Io(String),
}

impl From<Multi1Error> for Consensus1Error {
    fn from(v: Multi1Error) -> Self {
        Self::Multi(v)
    }
}
impl From<Sequencer1Error> for Consensus1Error {
    fn from(v: Sequencer1Error) -> Self {
        Self::Sequencer(v)
    }
}
impl From<ProtocolId1Error> for Consensus1Error {
    fn from(v: ProtocolId1Error) -> Self {
        Self::ProtocolId(v)
    }
}
impl From<ValidatorSchedule1Error> for Consensus1Error {
    fn from(v: ValidatorSchedule1Error) -> Self {
        Self::ValidatorSchedule(v)
    }
}
impl From<Sync1Error> for Consensus1Error {
    fn from(v: Sync1Error) -> Self {
        Self::Sync(v)
    }
}
impl fmt::Display for Consensus1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Consensus1Error {}

struct ConsBuf<'a> {
    raw: &'a [u8],
    off: usize,
}

impl<'a> ConsBuf<'a> {
    fn wrap(raw: &'a [u8]) -> Self {
        Self { raw, off: 0 }
    }
    fn pull(&mut self, n: usize) -> Result<&'a [u8], Consensus1Error> {
        let end = self
            .off
            .checked_add(n)
            .ok_or(Consensus1Error::InvalidWire("cursor overflow"))?;
        if end > self.raw.len() {
            return Err(Consensus1Error::InvalidWire("truncated consensus buffer"));
        }
        let sl = &self.raw[self.off..end];
        self.off = end;
        Ok(sl)
    }
    fn rd_u8(&mut self) -> Result<u8, Consensus1Error> {
        Ok(self.pull(1)?[0])
    }
    fn rd_u16(&mut self) -> Result<u16, Consensus1Error> {
        let s = self.pull(2)?;
        Ok(u16::from_be_bytes([s[0], s[1]]))
    }
    fn rd_u32(&mut self) -> Result<u32, Consensus1Error> {
        let s = self.pull(4)?;
        Ok(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn rd_u64(&mut self) -> Result<u64, Consensus1Error> {
        Ok(u64::from_be_bytes(self.pull(8)?.try_into().expect("8")))
    }
    fn rd_u128(&mut self) -> Result<u128, Consensus1Error> {
        Ok(u128::from_be_bytes(self.pull(16)?.try_into().expect("16")))
    }
    fn rd_b32(&mut self) -> Result<[u8; 32], Consensus1Error> {
        Ok(self.pull(32)?.try_into().expect("32"))
    }
    fn rd_b64(&mut self) -> Result<[u8; 64], Consensus1Error> {
        Ok(self.pull(64)?.try_into().expect("64"))
    }
    fn rd_bool(&mut self) -> Result<bool, Consensus1Error> {
        match self.rd_u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Consensus1Error::InvalidWire("non-canonical boolean byte")),
        }
    }
    fn done(&self) -> Result<(), Consensus1Error> {
        if self.off != self.raw.len() {
            return Err(Consensus1Error::InvalidWire("trailing consensus bytes"));
        }
        Ok(())
    }
}

/// Computes the domain-separated 32-byte digest of an 11-slot [`ExecutionRootRefV1`].
pub fn execution_root_ref_digest(exec_ref: &ExecutionRootRefV1) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(CONS_EXEC_ROOT_DIGEST_PREFIX);
    h.update(exec_ref.encode_canonical());
    h.finalize().into()
}

fn build_consensus_signing_preimage(
    fork: &ExecutorForkContextV1,
    domain_tag: SigningDomainTagV1,
    payload_digest: [u8; 32],
) -> SigningPreimageV1 {
    SigningPreimageV1 {
        protocol_version: 1,
        chain_id: fork.chain_id,
        genesis_id: fork.genesis_id,
        launch_manifest_id: fork.genesis_id,
        profile_id: fork.profile_id,
        domain_tag,
        signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
        hash_suite: HashSuiteIdV1::Sha256V1,
        payload_digest,
    }
}

/// Checks strict `> 2/3` weighted BFT quorum (`3 * signed_weight > 2 * total_weight`) with checked
/// integer arithmetic.
pub fn is_strict_two_thirds_quorum(signed_weight: u128, total_weight: u128) -> bool {
    if total_weight == 0 {
        return false;
    }
    match (signed_weight.checked_mul(3), total_weight.checked_mul(2)) {
        (Some(lhs), Some(rhs)) => lhs > rhs,
        _ => false,
    }
}

/// Builds a `CONSENSUS-1` `PROTOCOL-ID-1` [`CanonicalAcceptedProposalCoreV1`] binding height,
/// epoch, view, parent block, receipts root, execution root, and body digest.
#[allow(clippy::too_many_arguments)]
pub fn build_consensus_proposal_core(
    genesis_id: [u8; 32],
    profile_id: [u8; 32],
    parent_block_id: [u8; 32],
    height: u64,
    epoch: u64,
    view: u64,
    receipts_merkle_root: [u8; 32],
    execution_root_ref: &ExecutionRootRefV1,
    body_digest: [u8; 32],
) -> Result<CanonicalAcceptedProposalCoreV1, Consensus1Error> {
    let mut pre = Vec::with_capacity(CONS_PARENT_LOOKUP_PREFIX.len() + 64);
    pre.extend_from_slice(CONS_PARENT_LOOKUP_PREFIX);
    pre.extend_from_slice(&genesis_id);
    pre.extend_from_slice(&parent_block_id);
    let accepted_parent_lookup_id = profile_id_hash_parent_lookup(&pre);

    let mut auth_ref = Vec::with_capacity(64);
    auth_ref.extend_from_slice(&genesis_id);
    auth_ref.extend_from_slice(&profile_id);

    let mut hdr = Vec::with_capacity(24 + 32 + ExecutionRootRefV1::WIRE_LEN);
    hdr.extend_from_slice(&height.to_be_bytes());
    hdr.extend_from_slice(&epoch.to_be_bytes());
    hdr.extend_from_slice(&view.to_be_bytes());
    hdr.extend_from_slice(&receipts_merkle_root);
    hdr.extend_from_slice(&execution_root_ref.encode_canonical());

    let core = CanonicalAcceptedProposalCoreV1 {
        accepted_proposal_core_version: 1,
        accepted_parent_lookup_id,
        authority_ref_kind: 2,
        canonical_authority_ref_bytes: auth_ref,
        canonical_proposal_header_bytes: hdr,
        canonical_availability_payload_bytes: body_digest.to_vec(),
    };
    core.validate()?;
    Ok(core)
}

/// Two-phase HotStuff BFT vote phase (`Prepare = 1`, `Commit = 2`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ConsensusVotePhaseV1 {
    /// Phase 1: validator verified full body & lock safety and votes to form `PrepareQC`.
    Prepare = 1,
    /// Phase 2: validator locked on `PrepareQC` and votes to finalize `CommitQC`.
    Commit = 2,
}

impl ConsensusVotePhaseV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte phase discriminant fail-closed.
    pub const fn from_code(c: u8) -> Result<Self, Consensus1Error> {
        match c {
            1 => Ok(Self::Prepare),
            2 => Ok(Self::Commit),
            _ => Err(Consensus1Error::InvalidWire("unknown ConsensusVotePhaseV1")),
        }
    }
}

/// Role-separated signed validator vote (`252` canonical bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ConsensusVoteV1 {
    /// Vote schema version (`1`).
    pub vote_version: u16,
    /// Block height (`>= 1`).
    pub height: u64,
    /// Active validator schedule epoch.
    pub epoch: u64,
    /// HotStuff view number at `height`.
    pub view: u64,
    /// Phase (`Prepare` or `Commit`).
    pub phase: ConsensusVotePhaseV1,
    /// Target proposal `block_id`.
    pub block_id: [u8; 32],
    /// Digest of the active [`EpochValidatorScheduleV1`] governing `epoch`.
    pub schedule_epoch_digest: [u8; 32],
    /// Digest of the post-execution 11-slot [`ExecutionRootRefV1`].
    pub execution_root_ref_digest: [u8; 32],
    /// Data-before-vote attestation (must be `true` for every admissible vote).
    pub has_full_body: bool,
    /// Signing validator identity.
    pub voter_id: ActorIdV1,
    /// Signing validator's `vote_key` (`SigningDomainTagV1::ConsensusVote`).
    pub vote_pubkey: [u8; 32],
    /// Strict RFC 8032 Ed25519 signature over `signing_preimage()`.
    pub signature: [u8; 64],
}

impl ConsensusVoteV1 {
    /// Fixed canonical big-endian wire length (`252` bytes).
    pub const WIRE_LEN: usize = 2 + 8 + 8 + 8 + 1 + 32 + 32 + 32 + 1 + 32 + 32 + 64;

    /// Computes the signature-free SHA-256 payload digest bound inside [`SigningPreimageV1`].
    pub fn unsigned_payload_digest(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(CONS_VOTE_PAYLOAD_PREFIX);
        h.update(self.vote_version.to_be_bytes());
        h.update(self.height.to_be_bytes());
        h.update(self.epoch.to_be_bytes());
        h.update(self.view.to_be_bytes());
        h.update([self.phase.code()]);
        h.update(self.block_id);
        h.update(self.schedule_epoch_digest);
        h.update(self.execution_root_ref_digest);
        h.update([u8::from(self.has_full_body)]);
        h.update(self.voter_id.as_bytes());
        h.finalize().into()
    }

    /// Builds the domain-separated [`SigningPreimageV1`] (`SigningDomainTagV1::ConsensusVote`).
    pub fn signing_preimage(&self, fork: &ExecutorForkContextV1) -> SigningPreimageV1 {
        SigningPreimageV1 {
            protocol_version: 1,
            chain_id: fork.chain_id,
            genesis_id: fork.genesis_id,
            launch_manifest_id: fork.genesis_id,
            profile_id: fork.profile_id,
            domain_tag: SigningDomainTagV1::ConsensusVote,
            signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
            hash_suite: HashSuiteIdV1::Sha256V1,
            payload_digest: self.unsigned_payload_digest(),
        }
    }

    /// Signs this vote deterministically under RFC 8032 using `vote_seed`.
    pub fn sign_with_seed(
        mut self,
        vote_seed: &[u8; 32],
        fork: &ExecutorForkContextV1,
    ) -> Result<Self, Consensus1Error> {
        let pk = CanonicalEd25519PublicKeyV1::from_secret_seed(vote_seed)
            .map_err(|_| Consensus1Error::InvalidSignature("bad vote seed"))?;
        let sig = sign_preimage_deterministic(vote_seed, &self.signing_preimage(fork))
            .map_err(|_| Consensus1Error::InvalidSignature("vote sign failed"))?;
        self.vote_pubkey = *pk.as_bytes();
        self.signature = *sig.as_bytes();
        Ok(self)
    }

    /// Verifies schedule epoch, data-before-vote, role-separated `vote_key`, and RFC 8032 signature.
    pub fn verify_against_schedule(
        &self,
        fork: &ExecutorForkContextV1,
        schedule: &EpochValidatorScheduleV1,
    ) -> Result<u128, Consensus1Error> {
        if self.vote_version != CONSENSUS_1_VERSION || self.height == 0 {
            return Err(Consensus1Error::InvalidWire("bad vote version or height"));
        }
        if !self.has_full_body {
            return Err(Consensus1Error::DataBeforeVoteRefusal(
                "vote missing full-body attestation",
            ));
        }
        if schedule.effective_epoch != self.epoch {
            return Err(Consensus1Error::LongRangeStaleSchedule(
                "vote epoch disagrees with validator schedule",
            ));
        }
        let sched_digest = schedule.tag()?.digest();
        if self.schedule_epoch_digest != sched_digest {
            return Err(Consensus1Error::LongRangeStaleSchedule(
                "vote schedule_epoch_digest mismatch",
            ));
        }
        let entry = schedule
            .validators
            .iter()
            .find(|v| v.validator_id == self.voter_id)
            .ok_or(Consensus1Error::InvalidSignature("unknown voter_id"))?;
        if self.vote_pubkey == *entry.proposal_key.as_bytes() {
            return Err(Consensus1Error::CrossRoleKeyReuse(
                "proposal_key cannot sign ConsensusVote",
            ));
        }
        if self.vote_pubkey != *entry.vote_key.as_bytes() {
            return Err(Consensus1Error::InvalidSignature(
                "vote_pubkey disagrees with scheduled vote_key",
            ));
        }
        let sig = CanonicalEd25519SignatureV1::from_bytes(self.signature)
            .map_err(|_| Consensus1Error::InvalidSignature("malformed vote signature"))?;
        verify_preimage_strict(&entry.vote_key, &self.signing_preimage(fork), &sig)
            .map_err(|_| Consensus1Error::InvalidSignature("vote signature verification failed"))?;
        Ok(entry.effective_weight)
    }

    /// Encodes this vote into 252 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], Consensus1Error> {
        if self.vote_version != CONSENSUS_1_VERSION || self.height == 0 {
            return Err(Consensus1Error::InvalidWire("bad vote version or height"));
        }
        let mut b = Vec::with_capacity(Self::WIRE_LEN);
        b.extend_from_slice(&self.vote_version.to_be_bytes());
        b.extend_from_slice(&self.height.to_be_bytes());
        b.extend_from_slice(&self.epoch.to_be_bytes());
        b.extend_from_slice(&self.view.to_be_bytes());
        b.push(self.phase.code());
        b.extend_from_slice(&self.block_id);
        b.extend_from_slice(&self.schedule_epoch_digest);
        b.extend_from_slice(&self.execution_root_ref_digest);
        b.push(u8::from(self.has_full_body));
        b.extend_from_slice(self.voter_id.as_bytes());
        b.extend_from_slice(&self.vote_pubkey);
        b.extend_from_slice(&self.signature);
        Ok(b.try_into().expect("252"))
    }

    /// Strictly decodes 252 canonical big-endian bytes into a [`ConsensusVoteV1`].
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, Consensus1Error> {
        let mut c = ConsBuf::wrap(raw);
        let vote_version = c.rd_u16()?;
        let height = c.rd_u64()?;
        if vote_version != CONSENSUS_1_VERSION || height == 0 {
            return Err(Consensus1Error::InvalidWire("bad vote version or height"));
        }
        let epoch = c.rd_u64()?;
        let view = c.rd_u64()?;
        let phase = ConsensusVotePhaseV1::from_code(c.rd_u8()?)?;
        let block_id = c.rd_b32()?;
        let schedule_epoch_digest = c.rd_b32()?;
        let execution_root_ref_digest = c.rd_b32()?;
        let has_full_body = c.rd_bool()?;
        let voter_id = ActorIdV1::from_bytes(c.rd_b32()?);
        let vote_pubkey = c.rd_b32()?;
        let signature = c.rd_b64()?;
        c.done()?;
        Ok(Self {
            vote_version,
            height,
            epoch,
            view,
            phase,
            block_id,
            schedule_epoch_digest,
            execution_root_ref_digest,
            has_full_body,
            voter_id,
            vote_pubkey,
            signature,
        })
    }
}

/// Slashable same-`(height, epoch, view, phase)` vote equivocation evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VoteEquivocationEvidenceV1 {
    /// Equivocating validator identifier.
    pub voter_id: ActorIdV1,
    /// First valid vote observed for this `(height, epoch, view, phase)`.
    pub first_vote: ConsensusVoteV1,
    /// Conflicting valid vote observed for a distinct `block_id`.
    pub second_vote: ConsensusVoteV1,
}

impl VoteEquivocationEvidenceV1 {
    /// Verifies that both votes are cryptographically valid for the same validator, height, epoch,
    /// view, and phase, yet attest distinct `block_id`s.
    pub fn verify(
        &self,
        fork: &ExecutorForkContextV1,
        schedule: &EpochValidatorScheduleV1,
    ) -> Result<(), Consensus1Error> {
        let _ = self.first_vote.verify_against_schedule(fork, schedule)?;
        let _ = self.second_vote.verify_against_schedule(fork, schedule)?;
        if self.first_vote.voter_id != self.voter_id
            || self.second_vote.voter_id != self.voter_id
            || self.first_vote.height != self.second_vote.height
            || self.first_vote.epoch != self.second_vote.epoch
            || self.first_vote.view != self.second_vote.view
            || self.first_vote.phase != self.second_vote.phase
            || self.first_vote.block_id == self.second_vote.block_id
        {
            return Err(Consensus1Error::InvalidWire(
                "invalid VoteEquivocationEvidenceV1 pair",
            ));
        }
        Ok(())
    }
}

/// Strict `> 2/3` weighted quorum certificate (`PrepareQC` or `CommitQC`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct QuorumCertificateV1 {
    /// QC schema version (`1`).
    pub qc_version: u16,
    /// Block height.
    pub height: u64,
    /// Active epoch.
    pub epoch: u64,
    /// View number.
    pub view: u64,
    /// Phase (`Prepare` or `Commit`).
    pub phase: ConsensusVotePhaseV1,
    /// Target block identifier.
    pub block_id: [u8; 32],
    /// Bound validator schedule digest.
    pub schedule_epoch_digest: [u8; 32],
    /// Bound 11-slot execution root digest.
    pub execution_root_ref_digest: [u8; 32],
    /// Total active epoch stake weight.
    pub total_epoch_weight: u128,
    /// Verified signed stake weight (`3 * signed_weight > 2 * total_epoch_weight`).
    pub signed_weight: u128,
    /// Strictly sorted unique verified validator votes.
    pub votes: Vec<ConsensusVoteV1>,
}

impl QuorumCertificateV1 {
    /// Fixed QC header byte length (`157` bytes) preceding `votes.len() * 252` vote bytes.
    pub const HEADER_LEN: usize = 2 + 8 + 8 + 8 + 1 + 32 + 32 + 32 + 16 + 16 + 2;

    /// Forms and verifies a [`QuorumCertificateV1`] from a slice of candidate votes.
    pub fn assemble(
        fork: &ExecutorForkContextV1,
        schedule: &EpochValidatorScheduleV1,
        mut votes: Vec<ConsensusVoteV1>,
    ) -> Result<Self, Consensus1Error> {
        if votes.is_empty() {
            return Err(Consensus1Error::SubQuorumWeight {
                signed_weight: 0,
                total_weight: schedule.total_effective_weight,
            });
        }
        votes.sort_by_key(|v| v.voter_id);
        let first = votes[0];
        let mut signed_weight = 0u128;
        for (idx, v) in votes.iter().enumerate() {
            if idx > 0 && votes[idx - 1].voter_id == v.voter_id {
                return Err(Consensus1Error::InvalidWire("duplicate voter_id in QC"));
            }
            if v.height != first.height
                || v.epoch != first.epoch
                || v.view != first.view
                || v.phase != first.phase
                || v.block_id != first.block_id
                || v.schedule_epoch_digest != first.schedule_epoch_digest
                || v.execution_root_ref_digest != first.execution_root_ref_digest
            {
                return Err(Consensus1Error::InvalidWire(
                    "heterogeneous votes in QC assembly",
                ));
            }
            let w = v.verify_against_schedule(fork, schedule)?;
            signed_weight = signed_weight
                .checked_add(w)
                .ok_or(Consensus1Error::InvalidWire("signed_weight overflow"))?;
        }
        let qc = Self {
            qc_version: CONSENSUS_1_VERSION,
            height: first.height,
            epoch: first.epoch,
            view: first.view,
            phase: first.phase,
            block_id: first.block_id,
            schedule_epoch_digest: first.schedule_epoch_digest,
            execution_root_ref_digest: first.execution_root_ref_digest,
            total_epoch_weight: schedule.total_effective_weight,
            signed_weight,
            votes,
        };
        qc.verify(fork, schedule)?;
        Ok(qc)
    }

    /// Independently verifies sorting, uniqueness, signatures, data-before-vote, and `> 2/3` weight.
    pub fn verify(
        &self,
        fork: &ExecutorForkContextV1,
        schedule: &EpochValidatorScheduleV1,
    ) -> Result<(), Consensus1Error> {
        if self.qc_version != CONSENSUS_1_VERSION || self.height == 0 {
            return Err(Consensus1Error::InvalidWire("bad QC version or height"));
        }
        schedule.validate()?;
        if schedule.effective_epoch != self.epoch {
            return Err(Consensus1Error::LongRangeStaleSchedule(
                "QC epoch disagrees with schedule",
            ));
        }
        let sched_digest = schedule.tag()?.digest();
        if self.schedule_epoch_digest != sched_digest
            || self.total_epoch_weight != schedule.total_effective_weight
        {
            return Err(Consensus1Error::LongRangeStaleSchedule(
                "QC schedule digest or total weight mismatch",
            ));
        }
        let mut sum = 0u128;
        for (i, v) in self.votes.iter().enumerate() {
            if i > 0 && self.votes[i - 1].voter_id >= v.voter_id {
                return Err(Consensus1Error::InvalidWire(
                    "QC votes must be strictly sorted by unique voter_id",
                ));
            }
            if v.height != self.height
                || v.epoch != self.epoch
                || v.view != self.view
                || v.phase != self.phase
                || v.block_id != self.block_id
                || v.schedule_epoch_digest != self.schedule_epoch_digest
                || v.execution_root_ref_digest != self.execution_root_ref_digest
            {
                return Err(Consensus1Error::InvalidWire("QC vote header mismatch"));
            }
            let w = v.verify_against_schedule(fork, schedule)?;
            sum = sum
                .checked_add(w)
                .ok_or(Consensus1Error::InvalidWire("QC weight sum overflow"))?;
        }
        if sum != self.signed_weight {
            return Err(Consensus1Error::InvalidWire("QC signed_weight mismatch"));
        }
        if !is_strict_two_thirds_quorum(self.signed_weight, self.total_epoch_weight) {
            return Err(Consensus1Error::SubQuorumWeight {
                signed_weight: self.signed_weight,
                total_weight: self.total_epoch_weight,
            });
        }
        Ok(())
    }

    /// Encodes this QC into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, Consensus1Error> {
        if self.qc_version != CONSENSUS_1_VERSION || self.height == 0 {
            return Err(Consensus1Error::InvalidWire("bad QC version or height"));
        }
        let vc = u16::try_from(self.votes.len())
            .map_err(|_| Consensus1Error::InvalidWire("vote count overflow"))?;
        let mut b =
            Vec::with_capacity(Self::HEADER_LEN + self.votes.len() * ConsensusVoteV1::WIRE_LEN);
        b.extend_from_slice(&self.qc_version.to_be_bytes());
        b.extend_from_slice(&self.height.to_be_bytes());
        b.extend_from_slice(&self.epoch.to_be_bytes());
        b.extend_from_slice(&self.view.to_be_bytes());
        b.push(self.phase.code());
        b.extend_from_slice(&self.block_id);
        b.extend_from_slice(&self.schedule_epoch_digest);
        b.extend_from_slice(&self.execution_root_ref_digest);
        b.extend_from_slice(&self.total_epoch_weight.to_be_bytes());
        b.extend_from_slice(&self.signed_weight.to_be_bytes());
        b.extend_from_slice(&vc.to_be_bytes());
        for v in &self.votes {
            b.extend_from_slice(&v.encode_canonical()?);
        }
        Ok(b)
    }

    /// Decodes a canonical [`QuorumCertificateV1`] fail-closed.
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, Consensus1Error> {
        let mut c = ConsBuf::wrap(raw);
        let qc_version = c.rd_u16()?;
        let height = c.rd_u64()?;
        if qc_version != CONSENSUS_1_VERSION || height == 0 {
            return Err(Consensus1Error::InvalidWire("bad QC version or height"));
        }
        let epoch = c.rd_u64()?;
        let view = c.rd_u64()?;
        let phase = ConsensusVotePhaseV1::from_code(c.rd_u8()?)?;
        let block_id = c.rd_b32()?;
        let schedule_epoch_digest = c.rd_b32()?;
        let execution_root_ref_digest = c.rd_b32()?;
        let total_epoch_weight = c.rd_u128()?;
        let signed_weight = c.rd_u128()?;
        let vc = c.rd_u16()? as usize;
        if vc > 128 {
            return Err(Consensus1Error::InvalidWire("excessive QC vote count"));
        }
        let mut votes = Vec::with_capacity(vc);
        for _ in 0..vc {
            votes.push(ConsensusVoteV1::decode_canonical(
                c.pull(ConsensusVoteV1::WIRE_LEN)?,
            )?);
        }
        c.done()?;
        Ok(Self {
            qc_version,
            height,
            epoch,
            view,
            phase,
            block_id,
            schedule_epoch_digest,
            execution_root_ref_digest,
            total_epoch_weight,
            signed_weight,
            votes,
        })
    }

    /// Computes the canonical 32-byte SHA-256 digest of this QC.
    pub fn qc_digest(&self) -> Result<[u8; 32], Consensus1Error> {
        let enc = self.encode_canonical()?;
        let mut h = Sha256::new();
        h.update(CONS_QC_DIGEST_PREFIX);
        h.update(enc);
        Ok(h.finalize().into())
    }
}

/// Role-separated signed block proposal (`644` canonical bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ConsensusProposalV1 {
    /// Proposal schema version (`1`).
    pub proposal_version: u16,
    /// Sequential block height (`>= 1`).
    pub height: u64,
    /// Active validator schedule epoch.
    pub epoch: u64,
    /// HotStuff view number.
    pub view: u64,
    /// Parent finalized `block_id` (`[0u8; 32]` at height `1`).
    pub parent_block_id: [u8; 32],
    /// Governing execution fork context (`112` bytes).
    pub fork_context: ExecutorForkContextV1,
    /// Active epoch validator schedule digest.
    pub schedule_epoch_digest: [u8; 32],
    /// SHA-256 digest committing full [`MultiActorStreamBundleV1`] + [`MultiActorFoldReceiptV1`].
    pub body_digest: [u8; 32],
    /// Binary SHA-256 Merkle root over `fold_receipt.step_outcomes`.
    pub receipts_merkle_root: [u8; 32],
    /// Signature-free `PROTOCOL-ID-1` proposal-core digest.
    pub proposal_core_digest: [u8; 32],
    /// Post-execution 11-slot composed [`ExecutionRootRefV1`] (`178` bytes).
    pub execution_root_ref: ExecutionRootRefV1,
    /// Digest of the justifying QC (`[0u8; 32]` at initial view).
    pub justify_qc_digest: [u8; 32],
    /// View of the justifying QC (`0` at initial view).
    pub justify_qc_view: u64,
    /// Designated leader's validator ID.
    pub proposer_id: ActorIdV1,
    /// Leader's role-separated `proposal_key` (`SigningDomainTagV1::BlockProposal`).
    pub proposal_pubkey: [u8; 32],
    /// Strict RFC 8032 Ed25519 signature over `signing_preimage()`.
    pub signature: [u8; 64],
}

impl ConsensusProposalV1 {
    /// Fixed canonical big-endian wire length (`644` bytes).
    pub const WIRE_LEN: usize =
        2 + 24 + 8 + 64 + 32 * 8 + ExecutorForkContextV1::WIRE_LEN + ExecutionRootRefV1::WIRE_LEN;

    /// Computes the signature-free SHA-256 payload digest bound inside [`SigningPreimageV1`].
    pub fn unsigned_payload_digest(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(CONS_PROP_PAYLOAD_PREFIX);
        h.update(self.proposal_version.to_be_bytes());
        h.update(self.height.to_be_bytes());
        h.update(self.epoch.to_be_bytes());
        h.update(self.view.to_be_bytes());
        h.update(self.parent_block_id);
        h.update(self.fork_context.encode_canonical());
        h.update(self.schedule_epoch_digest);
        h.update(self.body_digest);
        h.update(self.receipts_merkle_root);
        h.update(self.proposal_core_digest);
        h.update(self.execution_root_ref.encode_canonical());
        h.update(self.justify_qc_digest);
        h.update(self.justify_qc_view.to_be_bytes());
        h.update(self.proposer_id.as_bytes());
        h.finalize().into()
    }

    /// Computes the canonical 32-byte `block_id` bound into votes, QCs, and child proposals.
    pub fn block_id(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(CONS_BLOCK_ID_PREFIX);
        h.update(self.unsigned_payload_digest());
        h.update(self.proposer_id.as_bytes());
        h.finalize().into()
    }

    /// Builds the domain-separated [`SigningPreimageV1`] (`SigningDomainTagV1::BlockProposal`).
    pub fn signing_preimage(&self) -> SigningPreimageV1 {
        build_consensus_signing_preimage(
            &self.fork_context,
            SigningDomainTagV1::BlockProposal,
            self.unsigned_payload_digest(),
        )
    }

    /// Signs this proposal deterministically using `proposal_seed`.
    pub fn sign_with_seed(mut self, proposal_seed: &[u8; 32]) -> Result<Self, Consensus1Error> {
        let pk = CanonicalEd25519PublicKeyV1::from_secret_seed(proposal_seed)
            .map_err(|_| Consensus1Error::InvalidSignature("bad proposal seed"))?;
        let sig = sign_preimage_deterministic(proposal_seed, &self.signing_preimage())
            .map_err(|_| Consensus1Error::InvalidSignature("proposal sign failed"))?;
        self.proposal_pubkey = *pk.as_bytes();
        self.signature = *sig.as_bytes();
        Ok(self)
    }

    /// Verifies schedule epoch, designated leader, role-separated `proposal_key`, and RFC 8032 sig.
    pub fn verify_against_schedule(
        &self,
        schedule: &EpochValidatorScheduleV1,
    ) -> Result<(), Consensus1Error> {
        if self.proposal_version != CONSENSUS_1_VERSION || self.height == 0 {
            return Err(Consensus1Error::InvalidWire(
                "bad proposal version or height",
            ));
        }
        schedule.validate()?;
        if schedule.effective_epoch != self.epoch {
            return Err(Consensus1Error::LongRangeStaleSchedule(
                "proposal epoch disagrees with schedule",
            ));
        }
        if schedule.tag()?.digest() != self.schedule_epoch_digest {
            return Err(Consensus1Error::LongRangeStaleSchedule(
                "proposal schedule_epoch_digest mismatch",
            ));
        }
        let leader = designated_leader(schedule, self.height, self.view);
        if self.proposer_id != leader.validator_id {
            return Err(Consensus1Error::InvalidSignature(
                "proposer_id is not designated round-robin leader for (height, view)",
            ));
        }
        if self.proposal_pubkey == *leader.vote_key.as_bytes() {
            return Err(Consensus1Error::CrossRoleKeyReuse(
                "vote_key cannot sign BlockProposal",
            ));
        }
        if self.proposal_pubkey != *leader.proposal_key.as_bytes() {
            return Err(Consensus1Error::InvalidSignature(
                "proposal_pubkey disagrees with leader proposal_key",
            ));
        }
        let sig = CanonicalEd25519SignatureV1::from_bytes(self.signature)
            .map_err(|_| Consensus1Error::InvalidSignature("malformed proposal signature"))?;
        verify_preimage_strict(&leader.proposal_key, &self.signing_preimage(), &sig).map_err(
            |_| Consensus1Error::InvalidSignature("proposal signature verification failed"),
        )?;
        Ok(())
    }

    /// Encodes this proposal into 644 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], Consensus1Error> {
        if self.proposal_version != CONSENSUS_1_VERSION || self.height == 0 {
            return Err(Consensus1Error::InvalidWire(
                "bad proposal version or height",
            ));
        }
        let mut b = Vec::with_capacity(Self::WIRE_LEN);
        b.extend_from_slice(&self.proposal_version.to_be_bytes());
        b.extend_from_slice(&self.height.to_be_bytes());
        b.extend_from_slice(&self.epoch.to_be_bytes());
        b.extend_from_slice(&self.view.to_be_bytes());
        b.extend_from_slice(&self.parent_block_id);
        b.extend_from_slice(&self.fork_context.encode_canonical());
        b.extend_from_slice(&self.schedule_epoch_digest);
        b.extend_from_slice(&self.body_digest);
        b.extend_from_slice(&self.receipts_merkle_root);
        b.extend_from_slice(&self.proposal_core_digest);
        b.extend_from_slice(&self.execution_root_ref.encode_canonical());
        b.extend_from_slice(&self.justify_qc_digest);
        b.extend_from_slice(&self.justify_qc_view.to_be_bytes());
        b.extend_from_slice(self.proposer_id.as_bytes());
        b.extend_from_slice(&self.proposal_pubkey);
        b.extend_from_slice(&self.signature);
        Ok(b.try_into().expect("644"))
    }

    /// Decodes 644 canonical big-endian wire bytes fail-closed.
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, Consensus1Error> {
        let mut c = ConsBuf::wrap(raw);
        let proposal_version = c.rd_u16()?;
        let height = c.rd_u64()?;
        if proposal_version != CONSENSUS_1_VERSION || height == 0 {
            return Err(Consensus1Error::InvalidWire(
                "bad proposal version or height",
            ));
        }
        let epoch = c.rd_u64()?;
        let view = c.rd_u64()?;
        let parent_block_id = c.rd_b32()?;
        let fork_context =
            ExecutorForkContextV1::decode_canonical(c.pull(ExecutorForkContextV1::WIRE_LEN)?)
                .map_err(|_| Consensus1Error::InvalidWire("bad fork_context"))?;
        let schedule_epoch_digest = c.rd_b32()?;
        let body_digest = c.rd_b32()?;
        let receipts_merkle_root = c.rd_b32()?;
        let proposal_core_digest = c.rd_b32()?;
        let execution_root_ref =
            ExecutionRootRefV1::decode_canonical(c.pull(ExecutionRootRefV1::WIRE_LEN)?)
                .map_err(|_| Consensus1Error::InvalidWire("bad execution_root_ref"))?;
        let justify_qc_digest = c.rd_b32()?;
        let justify_qc_view = c.rd_u64()?;
        let proposer_id = ActorIdV1::from_bytes(c.rd_b32()?);
        let proposal_pubkey = c.rd_b32()?;
        let signature = c.rd_b64()?;
        c.done()?;
        Ok(Self {
            proposal_version,
            height,
            epoch,
            view,
            parent_block_id,
            fork_context,
            schedule_epoch_digest,
            body_digest,
            receipts_merkle_root,
            proposal_core_digest,
            execution_root_ref,
            justify_qc_digest,
            justify_qc_view,
            proposer_id,
            proposal_pubkey,
            signature,
        })
    }
}

/// Deterministic weighted round-robin leader selection over an immutable [`EpochValidatorScheduleV1`].
pub fn designated_leader(
    schedule: &EpochValidatorScheduleV1,
    height: u64,
    view: u64,
) -> &ScheduledValidatorEntryV1 {
    let idx = ((height.wrapping_add(view)) as usize) % schedule.validators.len();
    &schedule.validators[idx]
}

/// Finalized 2-phase HotStuff BFT commit certificate (`bitblock/consensus-1/bft-commit-certificate/v1`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ConsensusCommitCertificateV1 {
    /// Certificate schema version (`1`).
    pub cert_version: u16,
    /// Signed block proposal bound to `ExecutionRootRefV1` & `PROTOCOL-ID-1` proposal core.
    pub proposal: ConsensusProposalV1,
    /// Strict `> 2/3` Phase-1 `PrepareQC`.
    pub prepare_qc: QuorumCertificateV1,
    /// Strict `> 2/3` Phase-2 `CommitQC`.
    pub commit_qc: QuorumCertificateV1,
}

impl ConsensusCommitCertificateV1 {
    /// Independently verifies the proposal, `PrepareQC`, `CommitQC`, and exact cross-binding.
    pub fn verify(&self, schedule: &EpochValidatorScheduleV1) -> Result<(), Consensus1Error> {
        if self.cert_version != CONSENSUS_1_VERSION {
            return Err(Consensus1Error::InvalidWire("bad commit cert version"));
        }
        self.proposal.verify_against_schedule(schedule)?;
        let bid = self.proposal.block_id();
        let erd = execution_root_ref_digest(&self.proposal.execution_root_ref);
        self.prepare_qc
            .verify(&self.proposal.fork_context, schedule)?;
        self.commit_qc
            .verify(&self.proposal.fork_context, schedule)?;
        if self.prepare_qc.phase != ConsensusVotePhaseV1::Prepare
            || self.commit_qc.phase != ConsensusVotePhaseV1::Commit
        {
            return Err(Consensus1Error::InvalidWire(
                "commit cert requires PrepareQC + CommitQC phases",
            ));
        }
        for qc in [&self.prepare_qc, &self.commit_qc] {
            if qc.height != self.proposal.height
                || qc.epoch != self.proposal.epoch
                || qc.view != self.proposal.view
                || qc.block_id != bid
                || qc.schedule_epoch_digest != self.proposal.schedule_epoch_digest
                || qc.execution_root_ref_digest != erd
            {
                return Err(Consensus1Error::InvalidWire(
                    "QC fields disagree with finalized proposal",
                ));
            }
        }
        Ok(())
    }

    /// Encodes this finalized certificate into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, Consensus1Error> {
        if self.cert_version != CONSENSUS_1_VERSION {
            return Err(Consensus1Error::InvalidWire("bad commit cert version"));
        }
        let prop = self.proposal.encode_canonical()?;
        let pqc = self.prepare_qc.encode_canonical()?;
        let cqc = self.commit_qc.encode_canonical()?;
        let plen = u32::try_from(pqc.len())
            .map_err(|_| Consensus1Error::InvalidWire("prepare_qc len overflow"))?;
        let clen = u32::try_from(cqc.len())
            .map_err(|_| Consensus1Error::InvalidWire("commit_qc len overflow"))?;
        let mut b = Vec::with_capacity(2 + prop.len() + 4 + pqc.len() + 4 + cqc.len());
        b.extend_from_slice(&self.cert_version.to_be_bytes());
        b.extend_from_slice(&prop);
        b.extend_from_slice(&plen.to_be_bytes());
        b.extend_from_slice(&pqc);
        b.extend_from_slice(&clen.to_be_bytes());
        b.extend_from_slice(&cqc);
        Ok(b)
    }

    /// Decodes a canonical [`ConsensusCommitCertificateV1`] fail-closed.
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, Consensus1Error> {
        let mut c = ConsBuf::wrap(raw);
        let cert_version = c.rd_u16()?;
        if cert_version != CONSENSUS_1_VERSION {
            return Err(Consensus1Error::InvalidWire("bad commit cert version"));
        }
        let proposal =
            ConsensusProposalV1::decode_canonical(c.pull(ConsensusProposalV1::WIRE_LEN)?)?;
        let plen = c.rd_u32()? as usize;
        let prepare_qc = QuorumCertificateV1::decode_canonical(c.pull(plen)?)?;
        let clen = c.rd_u32()? as usize;
        let commit_qc = QuorumCertificateV1::decode_canonical(c.pull(clen)?)?;
        c.done()?;
        Ok(Self {
            cert_version,
            proposal,
            prepare_qc,
            commit_qc,
        })
    }

    /// Computes the canonical `Receipt` `ContentTag` (`bitblock/consensus-1/bft-commit-certificate/v1`).
    pub fn tag(&self) -> Result<ContentTag, Consensus1Error> {
        let enc = self.encode_canonical()?;
        hash_canonical(
            TagKind::Receipt,
            CONSENSUS_1_COMMIT_CERT_SCHEMA,
            enc.len() as u64,
            |w| w(&enc),
        )
        .map_err(|_| Consensus1Error::InvalidWire("commit cert hash_canonical failed"))
    }
}

/// Deterministic safe-halt evidence triggered if two conflicting finalized blocks exist at one height.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ConsensusSafeHaltEvidenceV1 {
    /// Conflicting block height.
    pub height: u64,
    /// First finalized block ID.
    pub first_block_id: [u8; 32],
    /// Second distinct finalized block ID at the same height.
    pub conflicting_block_id: [u8; 32],
    /// Digest of the first verified `CommitQC`.
    pub first_commit_qc_digest: [u8; 32],
    /// Digest of the conflicting verified `CommitQC`.
    pub conflicting_commit_qc_digest: [u8; 32],
}

/// Live parent-authority certificate purpose (`LAYER1_DESIGN_PAPER.md` §575–745).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ParentAuthorityPurposeV1 {
    /// Release-authority consumer context.
    ReleaseAuthority = 1,
    /// Profile-activation consumer context.
    ActivationAuthority = 2,
}

impl ParentAuthorityPurposeV1 {
    /// Returns the canonical 1-byte purpose code.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte purpose code fail-closed.
    pub const fn from_code(c: u8) -> Result<Self, Consensus1Error> {
        match c {
            1 => Ok(Self::ReleaseAuthority),
            2 => Ok(Self::ActivationAuthority),
            _ => Err(Consensus1Error::InvalidWire(
                "unknown ParentAuthorityPurposeV1",
            )),
        }
    }
}

/// Monotonic parent-authority attainment status supporting post-acceptance status upgrade.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ParentAuthorityStatusV1 {
    /// Pre-acceptance tentative status (not yet sufficient for live consumer admission).
    Tentative = 1,
    /// Live accepted consensus-native status (`AcceptedConsensusNativeParentAuthorityCertificateV1`).
    AcceptedLive = 2,
    /// Post-acceptance upgraded finalized status (monotonic upgrade without changing parent core).
    UpgradedFinalized = 3,
}

impl ParentAuthorityStatusV1 {
    /// Returns the canonical 1-byte status code.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte status code fail-closed.
    pub const fn from_code(c: u8) -> Result<Self, Consensus1Error> {
        match c {
            1 => Ok(Self::Tentative),
            2 => Ok(Self::AcceptedLive),
            3 => Ok(Self::UpgradedFinalized),
            _ => Err(Consensus1Error::InvalidWire(
                "unknown ParentAuthorityStatusV1",
            )),
        }
    }
}

/// Certificate-independent consumer context separating envelope order from signing dependency (`121` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ParentAuthorityCertificateConsumerContextV1 {
    /// Bound consumer purpose (`ReleaseAuthority` or `ActivationAuthority`).
    pub purpose: ParentAuthorityPurposeV1,
    /// Consumer chain identifier.
    pub consumer_chain_id: u64,
    /// Consumer profile identifier.
    pub consumer_profile_id: [u8; 32],
    /// Consumer object digest bound independently of certificate bytes.
    pub consumer_object_digest: [u8; 32],
    /// Required consensus-native parent-authority position.
    pub required_position: u64,
    /// Separate envelope consensus-order chronological position.
    pub envelope_order_position: u64,
    /// Separate release/activation signing dependency digest.
    pub release_signing_dependency_digest: [u8; 32],
}

impl ParentAuthorityCertificateConsumerContextV1 {
    /// Fixed canonical big-endian wire length (`121` bytes).
    pub const WIRE_LEN: usize = 1 + 8 + 32 + 32 + 8 + 8 + 32;

    /// Encodes this consumer context into 121 canonical big-endian bytes.
    pub fn encode_canonical(&self) -> [u8; Self::WIRE_LEN] {
        let mut b = Vec::with_capacity(Self::WIRE_LEN);
        b.push(self.purpose.code());
        b.extend_from_slice(&self.consumer_chain_id.to_be_bytes());
        b.extend_from_slice(&self.consumer_profile_id);
        b.extend_from_slice(&self.consumer_object_digest);
        b.extend_from_slice(&self.required_position.to_be_bytes());
        b.extend_from_slice(&self.envelope_order_position.to_be_bytes());
        b.extend_from_slice(&self.release_signing_dependency_digest);
        b.try_into().expect("121")
    }

    /// Decodes 121 canonical big-endian bytes fail-closed.
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, Consensus1Error> {
        let mut c = ConsBuf::wrap(raw);
        let purpose = ParentAuthorityPurposeV1::from_code(c.rd_u8()?)?;
        let consumer_chain_id = c.rd_u64()?;
        let consumer_profile_id = c.rd_b32()?;
        let consumer_object_digest = c.rd_b32()?;
        let required_position = c.rd_u64()?;
        let envelope_order_position = c.rd_u64()?;
        let release_signing_dependency_digest = c.rd_b32()?;
        c.done()?;
        Ok(Self {
            purpose,
            consumer_chain_id,
            consumer_profile_id,
            consumer_object_digest,
            required_position,
            envelope_order_position,
            release_signing_dependency_digest,
        })
    }
}

/// Consensus-native accepted record occurrence receipt covering one decoded wrapper (`233` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ConsensusNativeAcceptedRecordOccurrenceReceiptV1 {
    /// Strictly increasing occurrence ordinal within the consensus-native universe.
    pub occurrence_ordinal: u64,
    /// Frozen `ProfileIdHashV1` lookup ID.
    pub accepted_parent_lookup_id: [u8; 32],
    /// Signature-free `PROTOCOL-ID-1` proposal core ID.
    pub proposal_core_id: [u8; 32],
    /// Raw canonical wrapper digest (`AcceptedProposalWrapperV1::tag().digest()`).
    pub raw_wrapper_digest: [u8; 32],
    /// Committed credential-set projection digest.
    pub credential_witness_id: [u8; 32],
    /// Committed consensus-evidence projection digest.
    pub consensus_evidence_id: [u8; 32],
    /// Committed availability-assessment projection digest.
    pub availability_assessment_id: [u8; 32],
    /// Committed availability-dependency projection digest.
    pub availability_dependency_id: [u8; 32],
    /// Whether this occurrence carried conflict material preserved inside total coverage.
    pub has_conflict_material: bool,
}

impl ConsensusNativeAcceptedRecordOccurrenceReceiptV1 {
    /// Fixed canonical big-endian wire length (`233` bytes).
    pub const WIRE_LEN: usize = 8 + 32 * 7 + 1;

    /// Derives a deterministic occurrence receipt from an [`AcceptedProposalWrapperV1`].
    pub fn from_wrapper(
        occurrence_ordinal: u64,
        wrapper: &AcceptedProposalWrapperV1,
        availability_assessment_id: [u8; 32],
        has_conflict_material: bool,
    ) -> Result<Self, Consensus1Error> {
        wrapper.validate_structure()?;
        let proposal_core_id = wrapper.core.tag()?.digest();
        let raw_wrapper_digest = wrapper.tag()?.digest();
        let mut hc = Sha256::new();
        hc.update(CONS_CERT_OCC_PREFIX);
        hc.update(b"cred");
        for cr in &wrapper.credentials {
            hc.update(cr.proposer_pubkey.as_bytes());
            hc.update(cr.signature.as_bytes());
        }
        let credential_witness_id: [u8; 32] = hc.finalize().into();

        let mut he = Sha256::new();
        he.update(CONS_CERT_OCC_PREFIX);
        he.update(b"evid");
        for ev in &wrapper.evidence_digests {
            he.update(ev);
        }
        let consensus_evidence_id: [u8; 32] = he.finalize().into();

        let mut hd = Sha256::new();
        hd.update(CONS_CERT_OCC_PREFIX);
        hd.update(b"deps");
        for dep in &wrapper.availability_dependencies {
            hd.update(dep.dependency_id);
            hd.update(dep.expected_bytes.to_be_bytes());
        }
        let availability_dependency_id: [u8; 32] = hd.finalize().into();

        Ok(Self {
            occurrence_ordinal,
            accepted_parent_lookup_id: wrapper.core.accepted_parent_lookup_id,
            proposal_core_id,
            raw_wrapper_digest,
            credential_witness_id,
            consensus_evidence_id,
            availability_assessment_id,
            availability_dependency_id,
            has_conflict_material,
        })
    }

    /// Encodes this occurrence receipt into 233 canonical big-endian bytes.
    pub fn encode_canonical(&self) -> [u8; Self::WIRE_LEN] {
        let mut b = Vec::with_capacity(Self::WIRE_LEN);
        b.extend_from_slice(&self.occurrence_ordinal.to_be_bytes());
        b.extend_from_slice(&self.accepted_parent_lookup_id);
        b.extend_from_slice(&self.proposal_core_id);
        b.extend_from_slice(&self.raw_wrapper_digest);
        b.extend_from_slice(&self.credential_witness_id);
        b.extend_from_slice(&self.consensus_evidence_id);
        b.extend_from_slice(&self.availability_assessment_id);
        b.extend_from_slice(&self.availability_dependency_id);
        b.push(u8::from(self.has_conflict_material));
        b.try_into().expect("233")
    }

    /// Decodes 233 canonical big-endian bytes fail-closed.
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, Consensus1Error> {
        let mut c = ConsBuf::wrap(raw);
        let occurrence_ordinal = c.rd_u64()?;
        let accepted_parent_lookup_id = c.rd_b32()?;
        let proposal_core_id = c.rd_b32()?;
        let raw_wrapper_digest = c.rd_b32()?;
        let credential_witness_id = c.rd_b32()?;
        let consensus_evidence_id = c.rd_b32()?;
        let availability_assessment_id = c.rd_b32()?;
        let availability_dependency_id = c.rd_b32()?;
        let has_conflict_material = c.rd_bool()?;
        c.done()?;
        Ok(Self {
            occurrence_ordinal,
            accepted_parent_lookup_id,
            proposal_core_id,
            raw_wrapper_digest,
            credential_witness_id,
            consensus_evidence_id,
            availability_assessment_id,
            availability_dependency_id,
            has_conflict_material,
        })
    }
}

/// Exact three-anchor history-start equality verifier (`LAYER1_DESIGN_PAPER.md` §587–596, `96` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VerifiedConsensusNativeAcceptedRecordOccurrenceHistoryStartEqualityV1 {
    /// Resolver history start digest.
    pub resolver_history_start: [u8; 32],
    /// Cutoff history start digest.
    pub cutoff_history_start: [u8; 32],
    /// Occurrence universe scope history start digest.
    pub scope_history_start: [u8; 32],
}

impl VerifiedConsensusNativeAcceptedRecordOccurrenceHistoryStartEqualityV1 {
    /// Fixed canonical byte length (`96` bytes).
    pub const WIRE_LEN: usize = 96;

    /// Constructs a matching exact-byte-equality history-start triple.
    pub const fn exact(start: [u8; 32]) -> Self {
        Self {
            resolver_history_start: start,
            cutoff_history_start: start,
            scope_history_start: start,
        }
    }

    /// Enforces exact byte equality across all three history-start anchors and non-zero value.
    pub fn verify(&self) -> Result<(), Consensus1Error> {
        if self.resolver_history_start == [0u8; 32] {
            return Err(Consensus1Error::HistoryStartMismatch(
                "history_start must be non-zero",
            ));
        }
        if self.resolver_history_start != self.cutoff_history_start
            || self.cutoff_history_start != self.scope_history_start
        {
            return Err(Consensus1Error::HistoryStartMismatch(
                "resolver/cutoff/scope history_start exact byte equality violated",
            ));
        }
        Ok(())
    }

    /// Encodes all three 32-byte anchors canonically (`96` bytes).
    pub fn encode_canonical(&self) -> [u8; Self::WIRE_LEN] {
        let mut b = [0u8; Self::WIRE_LEN];
        b[0..32].copy_from_slice(&self.resolver_history_start);
        b[32..64].copy_from_slice(&self.cutoff_history_start);
        b[64..96].copy_from_slice(&self.scope_history_start);
        b
    }

    /// Decodes 96 bytes into a history-start equality triple.
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, Consensus1Error> {
        let mut c = ConsBuf::wrap(raw);
        let resolver_history_start = c.rd_b32()?;
        let cutoff_history_start = c.rd_b32()?;
        let scope_history_start = c.rd_b32()?;
        c.done()?;
        Ok(Self {
            resolver_history_start,
            cutoff_history_start,
            scope_history_start,
        })
    }
}

/// Computes the canonical `occurrence_universe_root` over sorted unique occurrence receipts.
pub fn compute_occurrence_universe_root(
    receipts: &[ConsensusNativeAcceptedRecordOccurrenceReceiptV1],
) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(CONS_CERT_UNIV_PREFIX);
    h.update((receipts.len() as u32).to_be_bytes());
    for r in receipts {
        h.update(r.encode_canonical());
    }
    h.finalize().into()
}

/// Computes `accepted_parent_authority_evidence_commitment_id` binding history start, cutoff,
/// predecessor-closure lower bounds, and complete occurrence universe root.
pub fn compute_parent_authority_evidence_commitment(
    history_start: &VerifiedConsensusNativeAcceptedRecordOccurrenceHistoryStartEqualityV1,
    evidence_cutoff_position: u64,
    earliest_unbounded_valid_position: u64,
    candidate_rule_activation_position: u64,
    occurrence_universe_root: [u8; 32],
) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(CONS_CERT_EVID_PREFIX);
    h.update(history_start.encode_canonical());
    h.update(evidence_cutoff_position.to_be_bytes());
    h.update(earliest_unbounded_valid_position.to_be_bytes());
    h.update(candidate_rule_activation_position.to_be_bytes());
    h.update(occurrence_universe_root);
    h.finalize().into()
}

/// Live consensus-native accepted parent-authority certificate
/// (`bitblock/consensus-1/parent-authority-certificate/v1`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AcceptedConsensusNativeParentAuthorityCertificateV1 {
    /// Certificate schema version (`1`).
    pub cert_version: u16,
    /// Governing parent profile identifier.
    pub parent_profile_id: [u8; 32],
    /// Frozen `ProfileIdHashV1` lookup ID.
    pub accepted_parent_lookup_id: [u8; 32],
    /// Unique signature-free `PROTOCOL-ID-1` accepted proposal core ID.
    pub unique_accepted_proposal_core_block_id: [u8; 32],
    /// Certificate-independent consumer context (purpose + separate envelope vs signing refs).
    pub consumer_context: ParentAuthorityCertificateConsumerContextV1,
    /// Exact three-anchor history-start equality triple.
    pub history_start: VerifiedConsensusNativeAcceptedRecordOccurrenceHistoryStartEqualityV1,
    /// Consensus-native evidence cutoff position.
    pub evidence_cutoff_position: u64,
    /// Lossless predecessor-closure lower bound (`<= evidence_cutoff_position`, blocks late-valid-suffix).
    pub earliest_unbounded_valid_position: u64,
    /// Candidate rule activation position (`<= evidence_cutoff_position`, blocks candidate-advanced-rule).
    pub candidate_rule_activation_position: u64,
    /// Strictly sorted complete decoded occurrence receipts covering all wrappers & conflicts.
    pub occurrence_receipts: Vec<ConsensusNativeAcceptedRecordOccurrenceReceiptV1>,
    /// Canonical commitment root over `occurrence_receipts`.
    pub occurrence_universe_root: [u8; 32],
    /// Canonical evidence commitment binding history start, cutoff, lower bounds, and universe root.
    pub evidence_commitment_id: [u8; 32],
    /// Attained live parent-authority status (`AcceptedLive` or `UpgradedFinalized`).
    pub attained_status: ParentAuthorityStatusV1,
    /// Governing certificate policy digest.
    pub certificate_policy_id: [u8; 32],
    /// Active epoch governing the issuing quorum signatures.
    pub issuer_epoch: u64,
    /// Digest of the active epoch validator schedule.
    pub issuer_schedule_digest: [u8; 32],
    /// Digest of the consensus `CommitQC` under which admission occurred.
    pub admission_commit_qc_digest: [u8; 32],
    /// Explicit provenance guard: MUST be `false` for live admission (forbids replay inputs).
    pub from_replay_archive: bool,
    /// Strictly sorted unique issuing validator identities achieving `> 2/3` epoch stake weight.
    pub issuer_quorum_signers: Vec<ActorIdV1>,
    /// Matching strict RFC 8032 Ed25519 signatures (`SigningDomainTagV1::ConsensusVote`).
    pub issuer_signatures: Vec<[u8; 64]>,
}

impl AcceptedConsensusNativeParentAuthorityCertificateV1 {
    /// Computes the signature-free certificate sign-doc digest signed by the `> 2/3` issuing quorum.
    pub fn sign_doc_digest(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(CONS_CERT_SIGN_PREFIX);
        h.update(self.cert_version.to_be_bytes());
        h.update(self.parent_profile_id);
        h.update(self.accepted_parent_lookup_id);
        h.update(self.unique_accepted_proposal_core_block_id);
        h.update(self.consumer_context.encode_canonical());
        h.update(self.evidence_commitment_id);
        h.update(self.occurrence_universe_root);
        h.update([self.attained_status.code()]);
        h.update(self.certificate_policy_id);
        h.update(self.issuer_epoch.to_be_bytes());
        h.update(self.issuer_schedule_digest);
        h.update(self.admission_commit_qc_digest);
        h.update([u8::from(self.from_replay_archive)]);
        h.finalize().into()
    }

    /// Computes the canonical admission ID bound by `SYNC-1@replay-authority` commitment proofs.
    pub fn admission_id(&self) -> Result<[u8; 32], Consensus1Error> {
        let enc = self.encode_canonical()?;
        let mut h = Sha256::new();
        h.update(CONS_CERT_ADMIT_PREFIX);
        h.update(enc);
        Ok(h.finalize().into())
    }

    /// Builds the domain-separated [`SigningPreimageV1`] signed by each quorum validator's `vote_key`.
    pub fn signing_preimage(&self, fork: &ExecutorForkContextV1) -> SigningPreimageV1 {
        SigningPreimageV1 {
            protocol_version: 1,
            chain_id: fork.chain_id,
            genesis_id: fork.genesis_id,
            launch_manifest_id: fork.genesis_id,
            profile_id: fork.profile_id,
            domain_tag: SigningDomainTagV1::ConsensusVote,
            signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
            hash_suite: HashSuiteIdV1::Sha256V1,
            payload_digest: self.sign_doc_digest(),
        }
    }

    /// Signs and constructs a live certificate from normalized occurrence receipts and validator
    /// `(validator_id, vote_seed)` pairs.
    #[allow(clippy::too_many_arguments)]
    pub fn issue_live(
        fork: &ExecutorForkContextV1,
        schedule: &EpochValidatorScheduleV1,
        parent_profile_id: [u8; 32],
        accepted_parent_lookup_id: [u8; 32],
        unique_accepted_proposal_core_block_id: [u8; 32],
        consumer_context: ParentAuthorityCertificateConsumerContextV1,
        history_start: VerifiedConsensusNativeAcceptedRecordOccurrenceHistoryStartEqualityV1,
        evidence_cutoff_position: u64,
        earliest_unbounded_valid_position: u64,
        candidate_rule_activation_position: u64,
        mut occurrence_receipts: Vec<ConsensusNativeAcceptedRecordOccurrenceReceiptV1>,
        attained_status: ParentAuthorityStatusV1,
        certificate_policy_id: [u8; 32],
        admission_commit_qc_digest: [u8; 32],
        signer_seeds: &[(ActorIdV1, [u8; 32])],
    ) -> Result<Self, Consensus1Error> {
        occurrence_receipts.sort_by_key(|r| r.occurrence_ordinal);
        let occurrence_universe_root = compute_occurrence_universe_root(&occurrence_receipts);
        let evidence_commitment_id = compute_parent_authority_evidence_commitment(
            &history_start,
            evidence_cutoff_position,
            earliest_unbounded_valid_position,
            candidate_rule_activation_position,
            occurrence_universe_root,
        );
        let issuer_schedule_digest = schedule.tag()?.digest();
        let mut sorted_seeds = signer_seeds.to_vec();
        sorted_seeds.sort_by_key(|(id, _)| *id);

        let mut cert = Self {
            cert_version: CONSENSUS_1_VERSION,
            parent_profile_id,
            accepted_parent_lookup_id,
            unique_accepted_proposal_core_block_id,
            consumer_context,
            history_start,
            evidence_cutoff_position,
            earliest_unbounded_valid_position,
            candidate_rule_activation_position,
            occurrence_receipts,
            occurrence_universe_root,
            evidence_commitment_id,
            attained_status,
            certificate_policy_id,
            issuer_epoch: schedule.effective_epoch,
            issuer_schedule_digest,
            admission_commit_qc_digest,
            from_replay_archive: false,
            issuer_quorum_signers: Vec::with_capacity(sorted_seeds.len()),
            issuer_signatures: Vec::with_capacity(sorted_seeds.len()),
        };
        let pre = cert.signing_preimage(fork);
        for (vid, seed) in sorted_seeds {
            let sig = sign_preimage_deterministic(&seed, &pre)
                .map_err(|_| Consensus1Error::InvalidSignature("cert quorum sign failed"))?;
            cert.issuer_quorum_signers.push(vid);
            cert.issuer_signatures.push(*sig.as_bytes());
        }
        cert.verify_live_admission(fork, schedule)?;
        Ok(cert)
    }

    /// Independently verifies every live admission rule: non-replay provenance, exact three-anchor
    /// history-start equality, lossless predecessor-closure (blocking late-valid-suffix &
    /// candidate-advanced-rule), total receipt coverage, and strict `> 2/3` quorum authorization.
    pub fn verify_live_admission(
        &self,
        fork: &ExecutorForkContextV1,
        schedule: &EpochValidatorScheduleV1,
    ) -> Result<(), Consensus1Error> {
        if self.cert_version != CONSENSUS_1_VERSION {
            return Err(Consensus1Error::InvalidWire("bad parent auth cert version"));
        }
        if self.from_replay_archive {
            return Err(Consensus1Error::ReplayInputForbiddenInLiveAdmission);
        }
        if self.attained_status == ParentAuthorityStatusV1::Tentative {
            return Err(Consensus1Error::CertificateContractViolation(
                "tentative status cannot be admitted as live parent authority",
            ));
        }
        if self.unique_accepted_proposal_core_block_id == [0u8; 32]
            || self.accepted_parent_lookup_id == [0u8; 32]
            || self.consumer_context.consumer_object_digest == [0u8; 32]
        {
            return Err(Consensus1Error::CertificateContractViolation(
                "parent authority core, lookup, and consumer digests must be non-zero",
            ));
        }
        self.history_start.verify()?;
        if self.evidence_cutoff_position < self.earliest_unbounded_valid_position {
            return Err(Consensus1Error::LateValidSuffixRejected);
        }
        if self.evidence_cutoff_position < self.candidate_rule_activation_position {
            return Err(Consensus1Error::CandidateAdvancedRuleRejected);
        }
        if self.occurrence_receipts.is_empty() {
            return Err(Consensus1Error::CertificateContractViolation(
                "occurrence_receipts must be non-empty",
            ));
        }
        let mut matched_core = false;
        for (i, r) in self.occurrence_receipts.iter().enumerate() {
            if i > 0 && self.occurrence_receipts[i - 1].occurrence_ordinal >= r.occurrence_ordinal {
                return Err(Consensus1Error::InvalidWire(
                    "occurrence_receipts must be strictly sorted by unique occurrence_ordinal",
                ));
            }
            if r.accepted_parent_lookup_id != self.accepted_parent_lookup_id {
                return Err(Consensus1Error::CertificateContractViolation(
                    "occurrence receipt lookup_id mismatch",
                ));
            }
            if r.proposal_core_id == self.unique_accepted_proposal_core_block_id {
                matched_core = true;
            }
        }
        if !matched_core {
            return Err(Consensus1Error::CertificateContractViolation(
                "unique_accepted_proposal_core_block_id absent from occurrence universe",
            ));
        }
        let exp_univ = compute_occurrence_universe_root(&self.occurrence_receipts);
        if exp_univ != self.occurrence_universe_root {
            return Err(Consensus1Error::CertificateContractViolation(
                "occurrence_universe_root mismatch",
            ));
        }
        let exp_evid = compute_parent_authority_evidence_commitment(
            &self.history_start,
            self.evidence_cutoff_position,
            self.earliest_unbounded_valid_position,
            self.candidate_rule_activation_position,
            self.occurrence_universe_root,
        );
        if exp_evid != self.evidence_commitment_id {
            return Err(Consensus1Error::CertificateContractViolation(
                "evidence_commitment_id mismatch",
            ));
        }

        schedule.validate()?;
        if schedule.effective_epoch != self.issuer_epoch
            || schedule.tag()?.digest() != self.issuer_schedule_digest
        {
            return Err(Consensus1Error::LongRangeStaleSchedule(
                "certificate issuer schedule disagrees with active epoch",
            ));
        }
        if self.issuer_quorum_signers.len() != self.issuer_signatures.len()
            || self.issuer_quorum_signers.is_empty()
        {
            return Err(Consensus1Error::InvalidWire(
                "mismatched or empty issuer quorum signatures",
            ));
        }
        let pre = self.signing_preimage(fork);
        let mut weight = 0u128;
        for (i, (&vid, &sig_raw)) in self
            .issuer_quorum_signers
            .iter()
            .zip(self.issuer_signatures.iter())
            .enumerate()
        {
            if i > 0 && self.issuer_quorum_signers[i - 1] >= vid {
                return Err(Consensus1Error::InvalidWire(
                    "issuer_quorum_signers must be strictly sorted and unique",
                ));
            }
            let entry = schedule
                .validators
                .iter()
                .find(|v| v.validator_id == vid)
                .ok_or(Consensus1Error::InvalidSignature("unknown cert signer"))?;
            let sig = CanonicalEd25519SignatureV1::from_bytes(sig_raw)
                .map_err(|_| Consensus1Error::InvalidSignature("malformed cert signature"))?;
            verify_preimage_strict(&entry.vote_key, &pre, &sig)
                .map_err(|_| Consensus1Error::InvalidSignature("cert signature invalid"))?;
            weight = weight
                .checked_add(entry.effective_weight)
                .ok_or(Consensus1Error::InvalidWire("cert weight overflow"))?;
        }
        if !is_strict_two_thirds_quorum(weight, schedule.total_effective_weight) {
            return Err(Consensus1Error::SubQuorumWeight {
                signed_weight: weight,
                total_weight: schedule.total_effective_weight,
            });
        }
        Ok(())
    }

    /// Exports this certificate's canonical bytes wrapped as a bounded `SYNC-1@base`
    /// [`OpaqueAuthorityBlobV1`] (`OpaqueBlobKindV1::OpaqueCertificate`) for non-authoritative sync.
    pub fn to_opaque_authority_blob(&self) -> Result<OpaqueAuthorityBlobV1, Consensus1Error> {
        let raw = self.encode_canonical()?;
        Ok(OpaqueAuthorityBlobV1::new(
            OpaqueBlobKindV1::OpaqueCertificate,
            raw,
        )?)
    }

    /// Encodes this parent-authority certificate into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, Consensus1Error> {
        if self.cert_version != CONSENSUS_1_VERSION {
            return Err(Consensus1Error::InvalidWire("bad parent auth cert version"));
        }
        if self.issuer_quorum_signers.len() != self.issuer_signatures.len() {
            return Err(Consensus1Error::InvalidWire("signer/sig count mismatch"));
        }
        let rc = u16::try_from(self.occurrence_receipts.len())
            .map_err(|_| Consensus1Error::InvalidWire("receipt count overflow"))?;
        let sc = u16::try_from(self.issuer_quorum_signers.len())
            .map_err(|_| Consensus1Error::InvalidWire("signer count overflow"))?;
        let mut b = Vec::with_capacity(
            512 + self.occurrence_receipts.len()
                * ConsensusNativeAcceptedRecordOccurrenceReceiptV1::WIRE_LEN
                + self.issuer_quorum_signers.len() * 96,
        );
        b.extend_from_slice(&self.cert_version.to_be_bytes());
        b.extend_from_slice(&self.parent_profile_id);
        b.extend_from_slice(&self.accepted_parent_lookup_id);
        b.extend_from_slice(&self.unique_accepted_proposal_core_block_id);
        b.extend_from_slice(&self.consumer_context.encode_canonical());
        b.extend_from_slice(&self.history_start.encode_canonical());
        b.extend_from_slice(&self.evidence_cutoff_position.to_be_bytes());
        b.extend_from_slice(&self.earliest_unbounded_valid_position.to_be_bytes());
        b.extend_from_slice(&self.candidate_rule_activation_position.to_be_bytes());
        b.extend_from_slice(&rc.to_be_bytes());
        for r in &self.occurrence_receipts {
            b.extend_from_slice(&r.encode_canonical());
        }
        b.extend_from_slice(&self.occurrence_universe_root);
        b.extend_from_slice(&self.evidence_commitment_id);
        b.push(self.attained_status.code());
        b.extend_from_slice(&self.certificate_policy_id);
        b.extend_from_slice(&self.issuer_epoch.to_be_bytes());
        b.extend_from_slice(&self.issuer_schedule_digest);
        b.extend_from_slice(&self.admission_commit_qc_digest);
        b.push(u8::from(self.from_replay_archive));
        b.extend_from_slice(&sc.to_be_bytes());
        for (&vid, &sig) in self
            .issuer_quorum_signers
            .iter()
            .zip(self.issuer_signatures.iter())
        {
            b.extend_from_slice(vid.as_bytes());
            b.extend_from_slice(&sig);
        }
        Ok(b)
    }

    /// Decodes a canonical [`AcceptedConsensusNativeParentAuthorityCertificateV1`] fail-closed.
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, Consensus1Error> {
        let mut c = ConsBuf::wrap(raw);
        let cert_version = c.rd_u16()?;
        if cert_version != CONSENSUS_1_VERSION {
            return Err(Consensus1Error::InvalidWire("bad parent auth cert version"));
        }
        let parent_profile_id = c.rd_b32()?;
        let accepted_parent_lookup_id = c.rd_b32()?;
        let unique_accepted_proposal_core_block_id = c.rd_b32()?;
        let consumer_context = ParentAuthorityCertificateConsumerContextV1::decode_canonical(
            c.pull(ParentAuthorityCertificateConsumerContextV1::WIRE_LEN)?,
        )?;
        let history_start =
            VerifiedConsensusNativeAcceptedRecordOccurrenceHistoryStartEqualityV1::decode_canonical(
                c.pull(VerifiedConsensusNativeAcceptedRecordOccurrenceHistoryStartEqualityV1::WIRE_LEN)?,
            )?;
        let evidence_cutoff_position = c.rd_u64()?;
        let earliest_unbounded_valid_position = c.rd_u64()?;
        let candidate_rule_activation_position = c.rd_u64()?;
        let rc = c.rd_u16()? as usize;
        if rc > 256 {
            return Err(Consensus1Error::InvalidWire(
                "excessive occurrence receipts",
            ));
        }
        let mut occurrence_receipts = Vec::with_capacity(rc);
        for _ in 0..rc {
            occurrence_receipts.push(
                ConsensusNativeAcceptedRecordOccurrenceReceiptV1::decode_canonical(
                    c.pull(ConsensusNativeAcceptedRecordOccurrenceReceiptV1::WIRE_LEN)?,
                )?,
            );
        }
        let occurrence_universe_root = c.rd_b32()?;
        let evidence_commitment_id = c.rd_b32()?;
        let attained_status = ParentAuthorityStatusV1::from_code(c.rd_u8()?)?;
        let certificate_policy_id = c.rd_b32()?;
        let issuer_epoch = c.rd_u64()?;
        let issuer_schedule_digest = c.rd_b32()?;
        let admission_commit_qc_digest = c.rd_b32()?;
        let from_replay_archive = c.rd_bool()?;
        let sc = c.rd_u16()? as usize;
        if sc > 128 {
            return Err(Consensus1Error::InvalidWire("excessive issuer signers"));
        }
        let mut issuer_quorum_signers = Vec::with_capacity(sc);
        let mut issuer_signatures = Vec::with_capacity(sc);
        for _ in 0..sc {
            issuer_quorum_signers.push(ActorIdV1::from_bytes(c.rd_b32()?));
            issuer_signatures.push(c.rd_b64()?);
        }
        c.done()?;
        Ok(Self {
            cert_version,
            parent_profile_id,
            accepted_parent_lookup_id,
            unique_accepted_proposal_core_block_id,
            consumer_context,
            history_start,
            evidence_cutoff_position,
            earliest_unbounded_valid_position,
            candidate_rule_activation_position,
            occurrence_receipts,
            occurrence_universe_root,
            evidence_commitment_id,
            attained_status,
            certificate_policy_id,
            issuer_epoch,
            issuer_schedule_digest,
            admission_commit_qc_digest,
            from_replay_archive,
            issuer_quorum_signers,
            issuer_signatures,
        })
    }

    /// Computes the canonical `Receipt` `ContentTag` (`bitblock/consensus-1/parent-authority-certificate/v1`).
    pub fn tag(&self) -> Result<ContentTag, Consensus1Error> {
        let enc = self.encode_canonical()?;
        hash_canonical(
            TagKind::Receipt,
            CONSENSUS_1_PARENT_AUTH_CERT_SCHEMA,
            enc.len() as u64,
            |w| w(&enc),
        )
        .map_err(|_| Consensus1Error::InvalidWire("parent auth cert hash_canonical failed"))
    }
}

/// Unique live admission slot key `(unique_accepted_proposal_core_block_id, purpose, required_position)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LiveAuthorityAdmissionSlotV1 {
    /// Unique accepted parent proposal core ID.
    pub parent_core_id: [u8; 32],
    /// Consumer purpose (`ReleaseAuthority` or `ActivationAuthority`).
    pub purpose: ParentAuthorityPurposeV1,
    /// Required consensus-native parent-authority position.
    pub required_position: u64,
}

/// Live consensus-native parent-authority admission registry mirroring `admitLiveCert` in Lean 4.
#[derive(Clone, Debug, Default)]
pub struct LiveParentAuthorityRegistryV1 {
    admitted:
        BTreeMap<LiveAuthorityAdmissionSlotV1, AcceptedConsensusNativeParentAuthorityCertificateV1>,
}

impl LiveParentAuthorityRegistryV1 {
    /// Creates an empty live parent-authority admission registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Admits a verified live certificate into its unique `(parent_core_id, purpose, required_position)`
    /// slot, rejecting replay inputs, history-start mismatches, late-valid-suffixes,
    /// candidate-advanced rules, and duplicate/conflicting slot admissions fail-closed.
    pub fn admit_live(
        &mut self,
        cert: AcceptedConsensusNativeParentAuthorityCertificateV1,
        fork: &ExecutorForkContextV1,
        schedule: &EpochValidatorScheduleV1,
    ) -> Result<LiveAuthorityAdmissionSlotV1, Consensus1Error> {
        cert.verify_live_admission(fork, schedule)?;
        let slot = LiveAuthorityAdmissionSlotV1 {
            parent_core_id: cert.unique_accepted_proposal_core_block_id,
            purpose: cert.consumer_context.purpose,
            required_position: cert.consumer_context.required_position,
        };
        if self.admitted.contains_key(&slot) {
            return Err(Consensus1Error::DuplicateLiveAdmissionRejected);
        }
        self.admitted.insert(slot, cert);
        Ok(slot)
    }

    /// Monotonically upgrades an already-admitted live certificate's status (`AcceptedLive ->
    /// UpgradedFinalized`) without changing its unique admitted parent core ID or consumer context.
    pub fn upgrade_status_post_acceptance(
        &mut self,
        upgraded: AcceptedConsensusNativeParentAuthorityCertificateV1,
        fork: &ExecutorForkContextV1,
        schedule: &EpochValidatorScheduleV1,
    ) -> Result<(), Consensus1Error> {
        upgraded.verify_live_admission(fork, schedule)?;
        let slot = LiveAuthorityAdmissionSlotV1 {
            parent_core_id: upgraded.unique_accepted_proposal_core_block_id,
            purpose: upgraded.consumer_context.purpose,
            required_position: upgraded.consumer_context.required_position,
        };
        let existing =
            self.admitted
                .get(&slot)
                .ok_or(Consensus1Error::CertificateContractViolation(
                    "cannot upgrade un-admitted slot",
                ))?;
        if upgraded.attained_status <= existing.attained_status {
            return Err(Consensus1Error::CertificateContractViolation(
                "post-acceptance status upgrade must strictly increase attained_status",
            ));
        }
        if upgraded.accepted_parent_lookup_id != existing.accepted_parent_lookup_id
            || upgraded.consumer_context != existing.consumer_context
        {
            return Err(Consensus1Error::CertificateContractViolation(
                "post-acceptance status upgrade cannot alter parent lookup or consumer context",
            ));
        }
        self.admitted.insert(slot, upgraded);
        Ok(())
    }

    /// Returns the admitted live certificate for `slot`, if any.
    pub fn get(
        &self,
        slot: &LiveAuthorityAdmissionSlotV1,
    ) -> Option<&AcceptedConsensusNativeParentAuthorityCertificateV1> {
        self.admitted.get(slot)
    }

    /// Returns the total number of uniquely admitted live parent-authority certificates.
    pub fn len(&self) -> usize {
        self.admitted.len()
    }

    /// Returns whether no live certificates have been admitted yet.
    pub fn is_empty(&self) -> bool {
        self.admitted.is_empty()
    }
}

/// One validator replica executing `HotStuffTwoPhaseBftPosV1` with role-separated keys and
/// independent full-body execution before every vote.
#[derive(Clone, Debug)]
pub struct HotStuffValidatorNodeV1 {
    /// Validator identity (`ActorIdV1`).
    pub validator_id: ActorIdV1,
    vote_seed: [u8; 32],
    proposal_seed: [u8; 32],
    fork_context: ExecutorForkContextV1,
    schedules: BTreeMap<u64, EpochValidatorScheduleV1>,
    harness: MultiActorHarnessV1,
    locked_qc: Option<QuorumCertificateV1>,
    voted_prepare: BTreeMap<(u64, u64), ConsensusVoteV1>,
    voted_commit: BTreeMap<(u64, u64), ConsensusVoteV1>,
    finalized_certs: BTreeMap<u64, ConsensusCommitCertificateV1>,
    finalized_receipts: BTreeMap<u64, MultiActorFoldReceiptV1>,
    safe_halt: Option<ConsensusSafeHaltEvidenceV1>,
    authority_registry: LiveParentAuthorityRegistryV1,
}

impl HotStuffValidatorNodeV1 {
    /// Creates a fresh validator replica from a verified `MULTI-1` devnet fixture and schedules.
    pub fn new(
        validator_id: ActorIdV1,
        vote_seed: [u8; 32],
        proposal_seed: [u8; 32],
        fixture: &CanonicalMultiActorFixtureV1,
        schedules: BTreeMap<u64, EpochValidatorScheduleV1>,
    ) -> Result<Self, Consensus1Error> {
        Ok(Self {
            validator_id,
            vote_seed,
            proposal_seed,
            fork_context: fixture.fork_context,
            schedules,
            harness: fixture.into_harness()?,
            locked_qc: None,
            voted_prepare: BTreeMap::new(),
            voted_commit: BTreeMap::new(),
            finalized_certs: BTreeMap::new(),
            finalized_receipts: BTreeMap::new(),
            safe_halt: None,
            authority_registry: LiveParentAuthorityRegistryV1::new(),
        })
    }

    /// Installs an immutable scheduled future epoch into this validator replica.
    pub fn register_epoch_schedule(
        &mut self,
        sched: EpochValidatorScheduleV1,
    ) -> Result<(), Consensus1Error> {
        sched.validate()?;
        if self.schedules.contains_key(&sched.effective_epoch) {
            return Err(Consensus1Error::InvalidSchedule(
                "cannot overwrite existing immutable epoch schedule",
            ));
        }
        self.schedules.insert(sched.effective_epoch, sched);
        Ok(())
    }

    /// Returns the immutable validator schedule for `epoch`, if present.
    pub fn schedule_at(&self, epoch: u64) -> Option<&EpochValidatorScheduleV1> {
        self.schedules.get(&epoch)
    }

    /// Returns the validator's current locked `PrepareQC`, if any.
    pub fn locked_qc(&self) -> Option<&QuorumCertificateV1> {
        self.locked_qc.as_ref()
    }

    /// Returns this validator's current finalized block height (`0` at genesis).
    pub fn finalized_height(&self) -> u64 {
        self.finalized_certs
            .keys()
            .next_back()
            .copied()
            .unwrap_or(0)
    }

    /// Returns the finalized block ID at the validator's tip (`[0u8; 32]` at genesis).
    pub fn finalized_tip_block_id(&self) -> [u8; 32] {
        self.finalized_certs
            .values()
            .next_back()
            .map(|c| c.proposal.block_id())
            .unwrap_or([0u8; 32])
    }

    /// Returns the safe-halt evidence if conflicting finality has halted this validator.
    pub fn safe_halt_evidence(&self) -> Option<&ConsensusSafeHaltEvidenceV1> {
        self.safe_halt.as_ref()
    }

    /// Returns a reference to the validator's live parent-authority admission registry.
    pub fn authority_registry(&self) -> &LiveParentAuthorityRegistryV1 {
        &self.authority_registry
    }

    /// Returns a mutable reference to the validator's live parent-authority admission registry.
    pub fn authority_registry_mut(&mut self) -> &mut LiveParentAuthorityRegistryV1 {
        &mut self.authority_registry
    }

    /// Returns the underlying `MultiActorHarnessV1` for inspection of post-finality state.
    pub fn harness(&self) -> &MultiActorHarnessV1 {
        &self.harness
    }

    /// Returns the finalized [`ConsensusCommitCertificateV1`] at `height`, if committed.
    pub fn finalized_cert_at(&self, height: u64) -> Option<&ConsensusCommitCertificateV1> {
        self.finalized_certs.get(&height)
    }

    /// Returns the finalized [`MultiActorFoldReceiptV1`] at `height`, if committed.
    pub fn finalized_receipt_at(&self, height: u64) -> Option<&MultiActorFoldReceiptV1> {
        self.finalized_receipts.get(&height)
    }

    /// Builds and signs a valid [`ConsensusProposalV1`] as designated leader by executing
    /// `bundle` on a cloned harness (never mutating finalized state until `CommitQC`).
    pub fn propose_bundle(
        &self,
        bundle: &MultiActorStreamBundleV1,
        view: u64,
        justify_qc: Option<&QuorumCertificateV1>,
    ) -> Result<(ConsensusProposalV1, MultiActorFoldReceiptV1), Consensus1Error> {
        if let Some(ev) = &self.safe_halt {
            return Err(Consensus1Error::ConflictingFinalitySafeHalt(Box::new(
                ev.clone(),
            )));
        }
        let schedule =
            self.schedules
                .get(&bundle.epoch)
                .ok_or(Consensus1Error::LongRangeStaleSchedule(
                    "leader lacks schedule for bundle epoch",
                ))?;
        let expected_h = self.finalized_height() + 1;
        if bundle.batch_height != expected_h {
            return Err(Consensus1Error::InvalidWire(
                "bundle height disagrees with next consensus height",
            ));
        }
        let leader = designated_leader(schedule, expected_h, view);
        if leader.validator_id != self.validator_id {
            return Err(Consensus1Error::InvalidSignature(
                "validator is not designated leader for (height, view)",
            ));
        }
        let mut staged = self.harness.clone();
        let fold = staged.fold_stream_bundle(bundle)?;
        let bundle_bytes = bundle.encode_canonical()?;
        let receipt_bytes = fold.fold_receipt.encode_canonical()?;
        let body_digest = compute_batch_body_digest(&bundle_bytes, &receipt_bytes);
        let receipts_merkle_root = compute_receipts_merkle_root(&fold.fold_receipt.step_outcomes);
        let parent_block_id = self.finalized_tip_block_id();
        let core = build_consensus_proposal_core(
            self.fork_context.genesis_id,
            self.fork_context.profile_id,
            parent_block_id,
            expected_h,
            bundle.epoch,
            view,
            receipts_merkle_root,
            &fold.post_composition.execution_root_ref,
            body_digest,
        )?;
        let (justify_qc_digest, justify_qc_view) = match justify_qc {
            Some(qc) => (qc.qc_digest()?, qc.view),
            None => ([0u8; 32], 0),
        };
        let prop = ConsensusProposalV1 {
            proposal_version: CONSENSUS_1_VERSION,
            height: expected_h,
            epoch: bundle.epoch,
            view,
            parent_block_id,
            fork_context: self.fork_context,
            schedule_epoch_digest: schedule.tag()?.digest(),
            body_digest,
            receipts_merkle_root,
            proposal_core_digest: core.tag()?.digest(),
            execution_root_ref: fold.post_composition.execution_root_ref,
            justify_qc_digest,
            justify_qc_view,
            proposer_id: self.validator_id,
            proposal_pubkey: [0u8; 32],
            signature: [0u8; 64],
        }
        .sign_with_seed(&self.proposal_seed)?;
        Ok((prop, fold.fold_receipt))
    }

    /// Phase-1 `Prepare` vote: enforces full data-before-vote execution, schedule verification,
    /// HotStuff lock safety, and single-vote-per-`(height, view)` guard.
    pub fn vote_prepare(
        &mut self,
        proposal: &ConsensusProposalV1,
        body: Option<&MultiActorStreamBundleV1>,
        justify_qc: Option<&QuorumCertificateV1>,
    ) -> Result<ConsensusVoteV1, Consensus1Error> {
        if let Some(ev) = &self.safe_halt {
            return Err(Consensus1Error::ConflictingFinalitySafeHalt(Box::new(
                ev.clone(),
            )));
        }
        let Some(bundle) = body else {
            return Err(Consensus1Error::DataBeforeVoteRefusal(
                "refused Prepare vote without complete block body",
            ));
        };
        let schedule = self
            .schedules
            .get(&proposal.epoch)
            .ok_or(Consensus1Error::LongRangeStaleSchedule(
                "unknown or stale proposal epoch",
            ))?
            .clone();
        proposal.verify_against_schedule(&schedule)?;
        if proposal.fork_context != self.fork_context
            || proposal.height != self.finalized_height() + 1
            || proposal.parent_block_id != self.finalized_tip_block_id()
        {
            return Err(Consensus1Error::InvalidWire(
                "proposal fork_context, height, or parent_block_id mismatch",
            ));
        }

        // HotStuff 2-phase lock rule: if locked at this height, only vote for same block_id or
        // proposal carrying a verified justifying PrepareQC with strictly higher view than lock.
        let bid = proposal.block_id();
        if let Some(locked) = &self.locked_qc
            && locked.height == proposal.height
            && locked.block_id != bid
        {
            let Some(jqc) = justify_qc else {
                return Err(Consensus1Error::LockSafetyViolation(
                    "conflicting proposal lacks higher justifying PrepareQC",
                ));
            };
            jqc.verify(&self.fork_context, &schedule)?;
            if jqc.phase != ConsensusVotePhaseV1::Prepare
                || jqc.height != proposal.height
                || jqc.view <= locked.view
                || jqc.qc_digest()? != proposal.justify_qc_digest
                || jqc.view != proposal.justify_qc_view
            {
                return Err(Consensus1Error::LockSafetyViolation(
                    "justifying PrepareQC does not strictly beat locked_qc.view",
                ));
            }
        }

        // Independent full-body execution (data-before-vote)
        if bundle.batch_height != proposal.height || bundle.epoch != proposal.epoch {
            return Err(Consensus1Error::DataBeforeVoteRefusal(
                "body height/epoch disagrees with proposal",
            ));
        }
        let mut staged = self.harness.clone();
        let fold = staged.fold_stream_bundle(bundle)?;
        let body_digest = compute_batch_body_digest(
            &bundle.encode_canonical()?,
            &fold.fold_receipt.encode_canonical()?,
        );
        let receipts_root = compute_receipts_merkle_root(&fold.fold_receipt.step_outcomes);
        let core = build_consensus_proposal_core(
            self.fork_context.genesis_id,
            self.fork_context.profile_id,
            proposal.parent_block_id,
            proposal.height,
            proposal.epoch,
            proposal.view,
            receipts_root,
            &fold.post_composition.execution_root_ref,
            body_digest,
        )?;
        if body_digest != proposal.body_digest
            || receipts_root != proposal.receipts_merkle_root
            || core.tag()?.digest() != proposal.proposal_core_digest
            || fold.post_composition.execution_root_ref != proposal.execution_root_ref
        {
            return Err(Consensus1Error::DataBeforeVoteRefusal(
                "independent replay disagreed with proposed body/root/core commitments",
            ));
        }

        let vote = ConsensusVoteV1 {
            vote_version: CONSENSUS_1_VERSION,
            height: proposal.height,
            epoch: proposal.epoch,
            view: proposal.view,
            phase: ConsensusVotePhaseV1::Prepare,
            block_id: bid,
            schedule_epoch_digest: proposal.schedule_epoch_digest,
            execution_root_ref_digest: execution_root_ref_digest(&proposal.execution_root_ref),
            has_full_body: true,
            voter_id: self.validator_id,
            vote_pubkey: [0u8; 32],
            signature: [0u8; 64],
        }
        .sign_with_seed(&self.vote_seed, &self.fork_context)?;

        if let Some(prev) = self.voted_prepare.get(&(proposal.height, proposal.view)) {
            if prev.block_id != bid {
                return Err(Consensus1Error::VoteEquivocationDetected(Box::new(
                    VoteEquivocationEvidenceV1 {
                        voter_id: self.validator_id,
                        first_vote: *prev,
                        second_vote: vote,
                    },
                )));
            }
            return Ok(*prev);
        }
        self.voted_prepare
            .insert((proposal.height, proposal.view), vote);
        Ok(vote)
    }

    /// Phase-2 `Commit` vote: verifies `PrepareQC`, updates `locked_qc`, and signs `Commit` vote.
    pub fn vote_commit(
        &mut self,
        prepare_qc: &QuorumCertificateV1,
    ) -> Result<ConsensusVoteV1, Consensus1Error> {
        if let Some(ev) = &self.safe_halt {
            return Err(Consensus1Error::ConflictingFinalitySafeHalt(Box::new(
                ev.clone(),
            )));
        }
        let schedule = self
            .schedules
            .get(&prepare_qc.epoch)
            .ok_or(Consensus1Error::LongRangeStaleSchedule(
                "unknown epoch for PrepareQC",
            ))?
            .clone();
        if prepare_qc.phase != ConsensusVotePhaseV1::Prepare {
            return Err(Consensus1Error::InvalidWire("expected PrepareQC"));
        }
        prepare_qc.verify(&self.fork_context, &schedule)?;
        // Must have possessed & prepare-verified body or hold no conflicting lock
        if let Some(prev_lock) = &self.locked_qc
            && prev_lock.height == prepare_qc.height
            && prepare_qc.view < prev_lock.view
        {
            return Err(Consensus1Error::LockSafetyViolation(
                "cannot downgrade locked_qc to lower view",
            ));
        }
        self.locked_qc = Some(prepare_qc.clone());

        let vote = ConsensusVoteV1 {
            vote_version: CONSENSUS_1_VERSION,
            height: prepare_qc.height,
            epoch: prepare_qc.epoch,
            view: prepare_qc.view,
            phase: ConsensusVotePhaseV1::Commit,
            block_id: prepare_qc.block_id,
            schedule_epoch_digest: prepare_qc.schedule_epoch_digest,
            execution_root_ref_digest: prepare_qc.execution_root_ref_digest,
            has_full_body: true,
            voter_id: self.validator_id,
            vote_pubkey: [0u8; 32],
            signature: [0u8; 64],
        }
        .sign_with_seed(&self.vote_seed, &self.fork_context)?;

        if let Some(prev) = self.voted_commit.get(&(prepare_qc.height, prepare_qc.view)) {
            if prev.block_id != prepare_qc.block_id {
                return Err(Consensus1Error::VoteEquivocationDetected(Box::new(
                    VoteEquivocationEvidenceV1 {
                        voter_id: self.validator_id,
                        first_vote: *prev,
                        second_vote: vote,
                    },
                )));
            }
            return Ok(*prev);
        }
        self.voted_commit
            .insert((prepare_qc.height, prepare_qc.view), vote);
        Ok(vote)
    }

    /// Commits a verified finalized [`ConsensusCommitCertificateV1`] together with its complete
    /// body, or enters deterministic Safe-Halt if a conflicting finalized certificate exists at the
    /// exact same height.
    pub fn commit_finalized(
        &mut self,
        cert: &ConsensusCommitCertificateV1,
        body: &MultiActorStreamBundleV1,
    ) -> Result<MultiActorFoldReceiptV1, Consensus1Error> {
        if let Some(ev) = &self.safe_halt {
            return Err(Consensus1Error::ConflictingFinalitySafeHalt(Box::new(
                ev.clone(),
            )));
        }
        let schedule = self
            .schedules
            .get(&cert.proposal.epoch)
            .ok_or(Consensus1Error::LongRangeStaleSchedule(
                "unknown epoch for commit cert",
            ))?
            .clone();
        cert.verify(&schedule)?;
        let bid = cert.proposal.block_id();
        if let Some(existing) = self.finalized_certs.get(&cert.proposal.height) {
            let ex_bid = existing.proposal.block_id();
            if ex_bid != bid {
                let ev = ConsensusSafeHaltEvidenceV1 {
                    height: cert.proposal.height,
                    first_block_id: ex_bid,
                    conflicting_block_id: bid,
                    first_commit_qc_digest: existing.commit_qc.qc_digest()?,
                    conflicting_commit_qc_digest: cert.commit_qc.qc_digest()?,
                };
                self.safe_halt = Some(ev.clone());
                return Err(Consensus1Error::ConflictingFinalitySafeHalt(Box::new(ev)));
            }
            return Ok(self.finalized_receipts[&cert.proposal.height].clone());
        }
        if cert.proposal.height != self.finalized_height() + 1
            || cert.proposal.parent_block_id != self.finalized_tip_block_id()
        {
            return Err(Consensus1Error::InvalidWire(
                "non-sequential finalized certificate height or parent",
            ));
        }
        let mut staged = self.harness.clone();
        let fold = staged.fold_stream_bundle(body)?;
        let body_digest = compute_batch_body_digest(
            &body.encode_canonical()?,
            &fold.fold_receipt.encode_canonical()?,
        );
        if body_digest != cert.proposal.body_digest
            || fold.post_composition.execution_root_ref != cert.proposal.execution_root_ref
        {
            return Err(Consensus1Error::DataBeforeVoteRefusal(
                "finalized body disagrees with committed certificate",
            ));
        }
        self.harness = staged;
        self.locked_qc = None;
        self.finalized_certs
            .insert(cert.proposal.height, cert.clone());
        self.finalized_receipts
            .insert(cert.proposal.height, fold.fold_receipt.clone());
        Ok(fold.fold_receipt)
    }

    /// Records a conflicting finalized certificate check even without body (triggers Safe-Halt
    /// immediately when two distinct verified `CommitQC`s exist at the same height).
    pub fn observe_conflicting_commit_cert(
        &mut self,
        cert: &ConsensusCommitCertificateV1,
    ) -> Result<(), Consensus1Error> {
        let schedule = self
            .schedules
            .get(&cert.proposal.epoch)
            .ok_or(Consensus1Error::LongRangeStaleSchedule(
                "unknown epoch for conflicting cert check",
            ))?
            .clone();
        cert.verify(&schedule)?;
        if let Some(existing) = self.finalized_certs.get(&cert.proposal.height) {
            let ex_bid = existing.proposal.block_id();
            let new_bid = cert.proposal.block_id();
            if ex_bid != new_bid {
                let ev = ConsensusSafeHaltEvidenceV1 {
                    height: cert.proposal.height,
                    first_block_id: ex_bid,
                    conflicting_block_id: new_bid,
                    first_commit_qc_digest: existing.commit_qc.qc_digest()?,
                    conflicting_commit_qc_digest: cert.commit_qc.qc_digest()?,
                };
                self.safe_halt = Some(ev.clone());
                return Err(Consensus1Error::ConflictingFinalitySafeHalt(Box::new(ev)));
            }
        }
        Ok(())
    }
}

/// Inspects a set of votes for any equivocation from one validator at the same `(height, epoch,
/// view, phase)` and returns verified [`VoteEquivocationEvidenceV1`] if detected.
pub fn detect_vote_equivocation(
    votes: &[ConsensusVoteV1],
    fork: &ExecutorForkContextV1,
    schedule: &EpochValidatorScheduleV1,
) -> Result<Option<VoteEquivocationEvidenceV1>, Consensus1Error> {
    let mut seen: BTreeMap<(ActorIdV1, u64, u64, u64, ConsensusVotePhaseV1), ConsensusVoteV1> =
        BTreeMap::new();
    for &v in votes {
        let _ = v.verify_against_schedule(fork, schedule)?;
        let key = (v.voter_id, v.height, v.epoch, v.view, v.phase);
        if let Some(prev) = seen.get(&key) {
            if prev.block_id != v.block_id {
                let ev = VoteEquivocationEvidenceV1 {
                    voter_id: v.voter_id,
                    first_vote: *prev,
                    second_vote: v,
                };
                ev.verify(fork, schedule)?;
                return Ok(Some(ev));
            }
        } else {
            seen.insert(key, v);
        }
    }
    Ok(None)
}

/// Deterministic 4/5-validator BFT cluster harness wrapping `VALIDATOR-SCHEDULE-1` + `MULTI-1`.
#[derive(Clone, Debug)]
pub struct ConsensusClusterHarnessV1 {
    /// Verified `MULTI-1` devnet genesis fixture (`4` initial validators + actors A/B/C).
    pub fixture: CanonicalMultiActorFixtureV1,
    /// Committed `VALIDATOR-SCHEDULE-1` delayed-activation state machine.
    pub schedule_state: ValidatorScheduleStateV1,
    /// Committed `ECONOMIC-STATE-1` backing `CE-15` validator eligibility.
    pub economic_state: EconomicStateV1,
    /// Committed `GOVERNANCE-1` backing non-halted epoch scheduling.
    pub governance_state: GovernanceStateV1,
    /// Active validator replicas (`4` at epoch 0..1, expandable to `5` at epoch 2).
    pub nodes: Vec<HotStuffValidatorNodeV1>,
}

impl ConsensusClusterHarnessV1 {
    /// Deterministic Ed25519 vote secret seed for validator `idx` (`1..=5`).
    pub const fn validator_vote_seed(idx: u8) -> [u8; 32] {
        [100 + idx; 32]
    }

    /// Deterministic Ed25519 proposal secret seed for validator `idx` (`1..=5`).
    pub const fn validator_proposal_seed(idx: u8) -> [u8; 32] {
        [200 + idx; 32]
    }

    /// Boots the canonical 4-validator `CONSENSUS-1` devnet cluster at height `0`, epoch `0`.
    pub fn bootstrap_four_validators() -> Result<Self, Consensus1Error> {
        let fixture = CanonicalMultiActorFixtureV1::build()?;
        let profile_id = fixture.bound_manifest.profile_id;
        let (mut stakes, mut keys) = (Vec::with_capacity(4), Vec::with_capacity(4));
        for entry in &fixture.package.state.initial_validators {
            let vid = ActorIdV1::from_bytes(entry.validator_id);
            stakes.push((vid, entry.bonded_stake));
            keys.push((
                vid,
                ValidatorKeyPairV1 {
                    vote_key: entry.vote_pubkey,
                    proposal_key: entry.proposal_pubkey,
                },
            ));
        }
        let forbidden: BTreeSet<_> = fixture
            .bundle
            .actor_descriptors
            .iter()
            .flat_map(|(_, d)| {
                d.action_policy
                    .keys
                    .iter()
                    .chain(d.admin_policy.keys.iter())
                    .copied()
            })
            .collect();
        let economic_state = EconomicStateV1::from_genesis(
            fixture.bundle.validator_econ_policy,
            profile_id,
            &stakes,
        )
        .map_err(ValidatorSchedule1Error::from)?;
        let gov_ctx = GovernanceContextV1 {
            protocol_version: 1,
            chain_id: fixture.package.state.chain_id,
            genesis_id: fixture.package.genesis_id,
            launch_manifest_id: fixture.package.genesis_id,
            profile_id,
        };
        let governance_state = GovernanceStateV1::from_genesis(
            gov_ctx,
            fixture.bundle.governance_policy.clone(),
            &fixture.bundle.initial_gov_params,
        )
        .map_err(ValidatorSchedule1Error::from)?;
        let schedule_state = ValidatorScheduleStateV1::from_genesis(
            fixture.bundle.consensus_policy,
            fixture.bundle.validator_econ_policy,
            profile_id,
            &economic_state,
            &governance_state,
            &keys,
            &forbidden,
        )?;

        let mut initial_scheds = BTreeMap::new();
        for ep in 0..fixture.bundle.validator_econ_policy.activation_delay_epochs {
            if let Some(s) = schedule_state.schedule_at(ep) {
                initial_scheds.insert(ep, s.clone());
            }
        }
        let mut nodes = Vec::with_capacity(4);
        for i in 1u8..=4 {
            nodes.push(HotStuffValidatorNodeV1::new(
                ActorIdV1::from_bytes([i; 32]),
                Self::validator_vote_seed(i),
                Self::validator_proposal_seed(i),
                &fixture,
                initial_scheds.clone(),
            )?);
        }
        Ok(Self {
            fixture,
            schedule_state,
            economic_state,
            governance_state,
            nodes,
        })
    }

    /// Bonds a 5th validator at `source_epoch = 0` via `ECONOMIC-STATE-1` and commits its delayed
    /// `effective_epoch = 2` schedule via `VALIDATOR-SCHEDULE-1` (`4 -> 5` validator set change).
    pub fn activate_fifth_validator_for_epoch_2(
        &mut self,
    ) -> Result<EpochValidatorScheduleV1, Consensus1Error> {
        let v5 = ActorIdV1::from_bytes([5u8; 32]);
        let v5_vote = CanonicalEd25519PublicKeyV1::from_secret_seed(&Self::validator_vote_seed(5))
            .map_err(|_| Consensus1Error::InvalidSignature("v5 vote key"))?;
        let v5_prop =
            CanonicalEd25519PublicKeyV1::from_secret_seed(&Self::validator_proposal_seed(5))
                .map_err(|_| Consensus1Error::InvalidSignature("v5 proposal key"))?;
        let mut forbidden = BTreeSet::new();
        for (_, d) in &self.fixture.bundle.actor_descriptors {
            for &k in &d.action_policy.keys {
                forbidden.insert(k);
            }
            for &k in &d.admin_policy.keys {
                forbidden.insert(k);
            }
        }
        self.economic_state
            .bond_stake(v5, 100_000_000_000, 0)
            .map_err(ValidatorSchedule1Error::from)?;
        self.schedule_state.register_validator_keys(
            v5,
            ValidatorKeyPairV1 {
                vote_key: v5_vote,
                proposal_key: v5_prop,
            },
            &forbidden,
        )?;
        let sched_ep2 = self.schedule_state.schedule_epoch_from_sources(
            &self.economic_state,
            &self.governance_state,
            0,
            &forbidden,
        )?;
        self.schedule_state.advance_current_epoch(2)?;
        for node in &mut self.nodes {
            node.register_epoch_schedule(sched_ep2.clone())?;
        }
        // Catch up 5th replica to current finalized height & schedules
        let mut sched_map = BTreeMap::new();
        for ep in 0..=2 {
            if let Some(s) = self.schedule_state.schedule_at(ep) {
                sched_map.insert(ep, s.clone());
            }
        }
        let mut node5 = HotStuffValidatorNodeV1::new(
            v5,
            Self::validator_vote_seed(5),
            Self::validator_proposal_seed(5),
            &self.fixture,
            sched_map,
        )?;
        // Replay existing finalized state snapshot directly from node 0's harness/certs
        node5.harness = self.nodes[0].harness.clone();
        node5.finalized_certs = self.nodes[0].finalized_certs.clone();
        node5.finalized_receipts = self.nodes[0].finalized_receipts.clone();
        node5.authority_registry = self.nodes[0].authority_registry.clone();
        self.nodes.push(node5);
        Ok(sched_ep2)
    }

    /// Executes one full 2-phase HotStuff finality round across `active_indices` (supporting
    /// partition simulation when one validator is offline), then commits to all active replicas.
    pub fn run_round_with_subset(
        &mut self,
        bundle: &MultiActorStreamBundleV1,
        view: u64,
        active_indices: &[usize],
    ) -> Result<ConsensusCommitCertificateV1, Consensus1Error> {
        let schedule = self
            .schedule_state
            .schedule_at(bundle.epoch)
            .ok_or(Consensus1Error::LongRangeStaleSchedule(
                "missing epoch schedule",
            ))?
            .clone();
        let leader = designated_leader(&schedule, bundle.batch_height, view);
        let leader_idx = self
            .nodes
            .iter()
            .position(|n| n.validator_id == leader.validator_id)
            .ok_or(Consensus1Error::InvalidSchedule(
                "leader missing in cluster",
            ))?;
        if !active_indices.contains(&leader_idx) {
            return Err(Consensus1Error::InvalidSchedule(
                "designated leader is partitioned offline",
            ));
        }
        let (proposal, _rcpt) = self.nodes[leader_idx].propose_bundle(bundle, view, None)?;

        let mut prep_votes = Vec::with_capacity(active_indices.len());
        for &idx in active_indices {
            prep_votes.push(self.nodes[idx].vote_prepare(&proposal, Some(bundle), None)?);
        }
        let prepare_qc =
            QuorumCertificateV1::assemble(&self.fixture.fork_context, &schedule, prep_votes)?;

        let mut commit_votes = Vec::with_capacity(active_indices.len());
        for &idx in active_indices {
            commit_votes.push(self.nodes[idx].vote_commit(&prepare_qc)?);
        }
        let commit_qc =
            QuorumCertificateV1::assemble(&self.fixture.fork_context, &schedule, commit_votes)?;

        let cert = ConsensusCommitCertificateV1 {
            cert_version: CONSENSUS_1_VERSION,
            proposal,
            prepare_qc,
            commit_qc,
        };
        cert.verify(&schedule)?;
        for &idx in active_indices {
            let _ = self.nodes[idx].commit_finalized(&cert, bundle)?;
        }
        Ok(cert)
    }

    /// Builds a pair of split valid wrappers sharing identical unsigned core bytes (distinct
    /// proposer credentials + evidence), verifies arrival-permutation invariance under
    /// `PROTOCOL-ID-1` normalization, and issues + admits a live
    /// [`AcceptedConsensusNativeParentAuthorityCertificateV1`].
    pub fn build_and_admit_canonical_live_parent_authority_cert(
        &mut self,
        commit_cert: &ConsensusCommitCertificateV1,
        purpose: ParentAuthorityPurposeV1,
        required_position: u64,
        evidence_cutoff_position: u64,
    ) -> Result<
        (
            AcceptedConsensusNativeParentAuthorityCertificateV1,
            Vec<AcceptedProposalWrapperV1>,
        ),
        Consensus1Error,
    > {
        let schedule = self
            .schedule_state
            .schedule_at(commit_cert.proposal.epoch)
            .ok_or(Consensus1Error::LongRangeStaleSchedule(
                "missing cert epoch",
            ))?
            .clone();
        let core = build_consensus_proposal_core(
            self.fixture.fork_context.genesis_id,
            self.fixture.fork_context.profile_id,
            commit_cert.proposal.parent_block_id,
            commit_cert.proposal.height,
            commit_cert.proposal.epoch,
            commit_cert.proposal.view,
            commit_cert.proposal.receipts_merkle_root,
            &commit_cert.proposal.execution_root_ref,
            commit_cert.proposal.body_digest,
        )?;
        let core_digest = core.tag()?.digest();
        let pre = SigningPreimageV1 {
            protocol_version: 1,
            chain_id: self.fixture.fork_context.chain_id,
            genesis_id: self.fixture.fork_context.genesis_id,
            launch_manifest_id: self.fixture.fork_context.genesis_id,
            profile_id: self.fixture.fork_context.profile_id,
            domain_tag: SigningDomainTagV1::BlockProposal,
            signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
            hash_suite: HashSuiteIdV1::Sha256V1,
            payload_digest: core_digest,
        };
        let mk_wrap = |v_idx: u8,
                       ev_byte: u8,
                       dep_byte: u8|
         -> Result<AcceptedProposalWrapperV1, Consensus1Error> {
            let pseed = Self::validator_proposal_seed(v_idx);
            let pk = CanonicalEd25519PublicKeyV1::from_secret_seed(&pseed)
                .map_err(|_| Consensus1Error::InvalidSignature("wrapper pk"))?;
            let sig = sign_preimage_deterministic(&pseed, &pre)
                .map_err(|_| Consensus1Error::InvalidSignature("wrapper sig"))?;
            Ok(AcceptedProposalWrapperV1 {
                core: core.clone(),
                credentials: vec![ProposalCredentialEntryV1 {
                    proposer_pubkey: pk,
                    signature: sig,
                }],
                evidence_digests: vec![[ev_byte; 32]],
                availability_dependencies: vec![SameWrapperAvailabilityDependencyV1 {
                    dependency_id: [dep_byte; 32],
                    expected_bytes: 1024,
                }],
            })
        };
        let w1 = mk_wrap(1, 0xA1, 0xB1)?;
        let w2 = mk_wrap(2, 0xA2, 0xB2)?;
        // Verify arrival-permutation invariance of split-wrapper normalization (w1+w2 == w2+w1)
        let norm_ab = normalize_proposal_wrappers(
            &w1,
            &w2,
            self.fixture.fork_context.chain_id,
            self.fixture.fork_context.genesis_id,
            self.fixture.fork_context.genesis_id,
            self.fixture.fork_context.profile_id,
        )?;
        let norm_ba = normalize_proposal_wrappers(
            &w2,
            &w1,
            self.fixture.fork_context.chain_id,
            self.fixture.fork_context.genesis_id,
            self.fixture.fork_context.genesis_id,
            self.fixture.fork_context.profile_id,
        )?;
        if norm_ab.tag()? != norm_ba.tag()? {
            return Err(Consensus1Error::CertificateContractViolation(
                "split-wrapper normalization was not arrival-permutation invariant",
            ));
        }

        let r1 = ConsensusNativeAcceptedRecordOccurrenceReceiptV1::from_wrapper(
            1, &w1, [0xC1; 32], false,
        )?;
        let r2 = ConsensusNativeAcceptedRecordOccurrenceReceiptV1::from_wrapper(
            2, &w2, [0xC2; 32], false,
        )?;
        let consumer_context = ParentAuthorityCertificateConsumerContextV1 {
            purpose,
            consumer_chain_id: self.fixture.fork_context.chain_id,
            consumer_profile_id: self.fixture.fork_context.profile_id,
            consumer_object_digest: [0xD4; 32],
            required_position,
            envelope_order_position: required_position,
            release_signing_dependency_digest: [0xE5; 32],
        };
        let history_start =
            VerifiedConsensusNativeAcceptedRecordOccurrenceHistoryStartEqualityV1::exact(
                self.fixture.fork_context.genesis_id,
            );
        let signer_seeds: Vec<(ActorIdV1, [u8; 32])> = (1u8..=3)
            .map(|i| (ActorIdV1::from_bytes([i; 32]), Self::validator_vote_seed(i)))
            .collect();
        let cert = AcceptedConsensusNativeParentAuthorityCertificateV1::issue_live(
            &self.fixture.fork_context,
            &schedule,
            self.fixture.fork_context.profile_id,
            core.accepted_parent_lookup_id,
            core_digest,
            consumer_context,
            history_start,
            evidence_cutoff_position,
            1,
            1,
            vec![r1, r2],
            ParentAuthorityStatusV1::AcceptedLive,
            [0xF6; 32],
            commit_cert.commit_qc.qc_digest()?,
            &signer_seeds,
        )?;
        for node in &mut self.nodes {
            let _ = node.authority_registry_mut().admit_live(
                cert.clone(),
                &self.fixture.fork_context,
                &schedule,
            )?;
        }
        Ok((cert, vec![w1, w2]))
    }
}

/// Native `std::net` loopback TCP 4-validator 2-phase BFT message exchange (excluded on `wasm32`).
#[cfg(not(target_arch = "wasm32"))]
pub mod consensus_net {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    use super::*;
    use crate::bounds_1::ProtocolBoundsProfileV1;

    /// Fixed byte length of a `BBCON1` preflight wire header (`13` bytes).
    pub const CONS_WIRE_HEADER_LEN: usize = 6 + 2 + 1 + 4;

    /// Canonical wire message tag for native loopback 4-validator BFT transport.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    #[repr(u8)]
    pub enum ConsensusWireMsgKindV1 {
        /// Leader sending `(ConsensusProposalV1 || MultiActorStreamBundleV1)` to followers.
        ProposalWithBody = 1,
        /// Follower returning signed `Prepare` [`ConsensusVoteV1`].
        PrepareVote = 2,
        /// Leader sending assembled `PrepareQC` to followers.
        PrepareQc = 3,
        /// Follower returning signed `Commit` [`ConsensusVoteV1`].
        CommitVote = 4,
        /// Leader broadcasting finalized [`ConsensusCommitCertificateV1`].
        FinalizedCommitCert = 5,
        /// Follower acknowledging verified commit height (`8` bytes).
        CommitAck = 6,
    }

    impl ConsensusWireMsgKindV1 {
        /// Returns the 1-byte canonical wire discriminant.
        pub const fn code(self) -> u8 {
            self as u8
        }
        /// Decodes a 1-byte wire discriminant fail-closed.
        pub const fn from_code(c: u8) -> Result<Self, Consensus1Error> {
            match c {
                1 => Ok(Self::ProposalWithBody),
                2 => Ok(Self::PrepareVote),
                3 => Ok(Self::PrepareQc),
                4 => Ok(Self::CommitVote),
                5 => Ok(Self::FinalizedCommitCert),
                6 => Ok(Self::CommitAck),
                _ => Err(Consensus1Error::InvalidWire(
                    "unknown ConsensusWireMsgKindV1",
                )),
            }
        }
    }

    /// Zero-allocation preflight header check enforcing `BOUNDS-1` before payload allocation.
    pub fn preflight_wire_header(
        hdr: &[u8; CONS_WIRE_HEADER_LEN],
        bounds: &ProtocolBoundsProfileV1,
    ) -> Result<(ConsensusWireMsgKindV1, usize), Consensus1Error> {
        if hdr[0..6] != CONS_WIRE_MAGIC {
            return Err(Consensus1Error::InvalidWire("bad BBCON1 magic"));
        }
        let ver = u16::from_be_bytes([hdr[6], hdr[7]]);
        if ver != CONSENSUS_1_VERSION {
            return Err(Consensus1Error::InvalidWire("bad BBCON1 version"));
        }
        let kind = ConsensusWireMsgKindV1::from_code(hdr[8])?;
        let len = u32::from_be_bytes([hdr[9], hdr[10], hdr[11], hdr[12]]) as usize;
        if len > bounds.max_block_bytes as usize
            || (CONS_WIRE_HEADER_LEN as u64).saturating_add(len as u64)
                > u64::from(bounds.max_peer_inflight_bytes)
        {
            return Err(Consensus1Error::Io(
                "BBCON1 frame exceeds BOUNDS-1 block/inflight limit".into(),
            ));
        }
        Ok((kind, len))
    }

    fn write_frame(
        stream: &mut TcpStream,
        kind: ConsensusWireMsgKindV1,
        payload: &[u8],
        bounds: &ProtocolBoundsProfileV1,
    ) -> Result<(), Consensus1Error> {
        let plen = u32::try_from(payload.len())
            .map_err(|_| Consensus1Error::InvalidWire("payload len overflow"))?;
        let mut hdr = [0u8; CONS_WIRE_HEADER_LEN];
        hdr[0..6].copy_from_slice(&CONS_WIRE_MAGIC);
        hdr[6..8].copy_from_slice(&CONSENSUS_1_VERSION.to_be_bytes());
        hdr[8] = kind.code();
        hdr[9..13].copy_from_slice(&plen.to_be_bytes());
        let _ = preflight_wire_header(&hdr, bounds)?;
        stream
            .write_all(&hdr)
            .and_then(|_| stream.write_all(payload))
            .and_then(|_| stream.flush())
            .map_err(|e| Consensus1Error::Io(e.to_string()))
    }

    fn read_frame(
        stream: &mut TcpStream,
        bounds: &ProtocolBoundsProfileV1,
    ) -> Result<(ConsensusWireMsgKindV1, Vec<u8>), Consensus1Error> {
        let mut hdr = [0u8; CONS_WIRE_HEADER_LEN];
        stream
            .read_exact(&mut hdr)
            .map_err(|e| Consensus1Error::Io(e.to_string()))?;
        let (kind, len) = preflight_wire_header(&hdr, bounds)?;
        let mut buf = vec![0u8; len];
        stream
            .read_exact(&mut buf)
            .map_err(|e| Consensus1Error::Io(e.to_string()))?;
        Ok((kind, buf))
    }

    /// Runs a real 4-validator 2-phase HotStuff finality round over `127.0.0.1` loopback TCP:
    /// follower replicas listen on ephemeral ports, execute full body data-before-vote, return
    /// signed `Prepare` & `Commit` votes, and commit the finalized [`ConsensusCommitCertificateV1`].
    pub fn run_loopback_four_validator_round(
        cluster: &mut ConsensusClusterHarnessV1,
        bundle: &MultiActorStreamBundleV1,
        view: u64,
    ) -> Result<ConsensusCommitCertificateV1, Consensus1Error> {
        let bounds = cluster.fixture.bundle.bounds_profile;
        let schedule = cluster
            .schedule_state
            .schedule_at(bundle.epoch)
            .ok_or(Consensus1Error::LongRangeStaleSchedule(
                "missing epoch schedule for TCP round",
            ))?
            .clone();
        let leader = designated_leader(&schedule, bundle.batch_height, view);
        let leader_idx = cluster
            .nodes
            .iter()
            .position(|n| n.validator_id == leader.validator_id)
            .ok_or(Consensus1Error::InvalidSchedule("leader not in cluster"))?;

        let (proposal, _) = cluster.nodes[leader_idx].propose_bundle(bundle, view, None)?;
        let prop_bytes = proposal.encode_canonical()?;
        let body_bytes = bundle.encode_canonical()?;
        let mut prop_body_payload = Vec::with_capacity(prop_bytes.len() + body_bytes.len());
        prop_body_payload.extend_from_slice(&prop_bytes);
        prop_body_payload.extend_from_slice(&body_bytes);

        let mut listeners = Vec::new();
        let mut follower_indices = Vec::new();
        for idx in 0..cluster.nodes.len() {
            if idx == leader_idx {
                continue;
            }
            let lis =
                TcpListener::bind("127.0.0.1:0").map_err(|e| Consensus1Error::Io(e.to_string()))?;
            let addr = lis
                .local_addr()
                .map_err(|e| Consensus1Error::Io(e.to_string()))?;
            listeners.push((idx, lis, addr));
            follower_indices.push(idx);
        }

        let mut handles = Vec::new();
        for (idx, lis, _addr) in &listeners {
            let mut node = cluster.nodes[*idx].clone();
            let lis_clone = lis
                .try_clone()
                .map_err(|e| Consensus1Error::Io(e.to_string()))?;
            let h = thread::spawn(
                move || -> Result<HotStuffValidatorNodeV1, Consensus1Error> {
                    let (mut s, _) = lis_clone
                        .accept()
                        .map_err(|e| Consensus1Error::Io(e.to_string()))?;
                    let (k1, pb) = read_frame(&mut s, &bounds)?;
                    if k1 != ConsensusWireMsgKindV1::ProposalWithBody
                        || pb.len() < ConsensusProposalV1::WIRE_LEN
                    {
                        return Err(Consensus1Error::InvalidWire("expected ProposalWithBody"));
                    }
                    let p = ConsensusProposalV1::decode_canonical(
                        &pb[..ConsensusProposalV1::WIRE_LEN],
                    )?;
                    let b = MultiActorStreamBundleV1::decode_canonical(
                        &pb[ConsensusProposalV1::WIRE_LEN..],
                    )?;
                    let pv = node.vote_prepare(&p, Some(&b), None)?;
                    write_frame(
                        &mut s,
                        ConsensusWireMsgKindV1::PrepareVote,
                        &pv.encode_canonical()?,
                        &bounds,
                    )?;

                    let (k2, pqc_raw) = read_frame(&mut s, &bounds)?;
                    if k2 != ConsensusWireMsgKindV1::PrepareQc {
                        return Err(Consensus1Error::InvalidWire("expected PrepareQc"));
                    }
                    let pqc = QuorumCertificateV1::decode_canonical(&pqc_raw)?;
                    let cv = node.vote_commit(&pqc)?;
                    write_frame(
                        &mut s,
                        ConsensusWireMsgKindV1::CommitVote,
                        &cv.encode_canonical()?,
                        &bounds,
                    )?;

                    let (k3, cc_raw) = read_frame(&mut s, &bounds)?;
                    if k3 != ConsensusWireMsgKindV1::FinalizedCommitCert {
                        return Err(Consensus1Error::InvalidWire("expected FinalizedCommitCert"));
                    }
                    let cc = ConsensusCommitCertificateV1::decode_canonical(&cc_raw)?;
                    let _ = node.commit_finalized(&cc, &b)?;
                    write_frame(
                        &mut s,
                        ConsensusWireMsgKindV1::CommitAck,
                        &node.finalized_height().to_be_bytes(),
                        &bounds,
                    )?;
                    Ok(node)
                },
            );
            handles.push((*idx, h));
        }

        // Leader connects to all followers, executes 2-phase BFT over live loopback TCP
        let mut conns = Vec::with_capacity(listeners.len());
        for &(_, _, addr) in &listeners {
            let mut st =
                TcpStream::connect(addr).map_err(|e| Consensus1Error::Io(e.to_string()))?;
            write_frame(
                &mut st,
                ConsensusWireMsgKindV1::ProposalWithBody,
                &prop_body_payload,
                &bounds,
            )?;
            conns.push(st);
        }
        let mut prep_votes =
            vec![cluster.nodes[leader_idx].vote_prepare(&proposal, Some(bundle), None)?];
        for st in &mut conns {
            let (k, raw) = read_frame(st, &bounds)?;
            if k != ConsensusWireMsgKindV1::PrepareVote {
                return Err(Consensus1Error::InvalidWire("expected PrepareVote"));
            }
            prep_votes.push(ConsensusVoteV1::decode_canonical(&raw)?);
        }
        let prepare_qc =
            QuorumCertificateV1::assemble(&cluster.fixture.fork_context, &schedule, prep_votes)?;
        let pqc_bytes = prepare_qc.encode_canonical()?;
        for st in &mut conns {
            write_frame(st, ConsensusWireMsgKindV1::PrepareQc, &pqc_bytes, &bounds)?;
        }
        let mut commit_votes = vec![cluster.nodes[leader_idx].vote_commit(&prepare_qc)?];
        for st in &mut conns {
            let (k, raw) = read_frame(st, &bounds)?;
            if k != ConsensusWireMsgKindV1::CommitVote {
                return Err(Consensus1Error::InvalidWire("expected CommitVote"));
            }
            commit_votes.push(ConsensusVoteV1::decode_canonical(&raw)?);
        }
        let commit_qc =
            QuorumCertificateV1::assemble(&cluster.fixture.fork_context, &schedule, commit_votes)?;
        let commit_cert = ConsensusCommitCertificateV1 {
            cert_version: CONSENSUS_1_VERSION,
            proposal,
            prepare_qc,
            commit_qc,
        };
        commit_cert.verify(&schedule)?;
        let cc_bytes = commit_cert.encode_canonical()?;
        for st in &mut conns {
            write_frame(
                st,
                ConsensusWireMsgKindV1::FinalizedCommitCert,
                &cc_bytes,
                &bounds,
            )?;
        }
        for st in &mut conns {
            let (k, ack) = read_frame(st, &bounds)?;
            if k != ConsensusWireMsgKindV1::CommitAck || ack.len() != 8 {
                return Err(Consensus1Error::InvalidWire("bad CommitAck"));
            }
            let h = u64::from_be_bytes(ack.try_into().expect("8"));
            if h != bundle.batch_height {
                return Err(Consensus1Error::InvalidWire(
                    "follower CommitAck height mismatch",
                ));
            }
        }
        let _ = cluster.nodes[leader_idx].commit_finalized(&commit_cert, bundle)?;
        for (idx, h) in handles {
            let updated = h
                .join()
                .map_err(|_| Consensus1Error::Io("follower thread panicked".into()))??;
            cluster.nodes[idx] = updated;
        }
        let _ = follower_indices;
        Ok(commit_cert)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consensus_1_bft_commit_and_live_parent_authority_golden_tags() {
        let mut cluster = ConsensusClusterHarnessV1::bootstrap_four_validators().unwrap();
        let pre_wr = cluster.nodes[0].harness().snapshot().unwrap().world_root;
        let bundle = cluster
            .fixture
            .canonical_mixed_race_bundle(1, pre_wr)
            .unwrap();
        let commit_cert = cluster
            .run_round_with_subset(&bundle, 0, &[0, 1, 2, 3])
            .unwrap();
        let (pa_cert, _wraps) = cluster
            .build_and_admit_canonical_live_parent_authority_cert(
                &commit_cert,
                ParentAuthorityPurposeV1::ReleaseAuthority,
                1,
                2,
            )
            .unwrap();

        // Round-trip canonical codec parity
        let cc_rt = ConsensusCommitCertificateV1::decode_canonical(
            &commit_cert.encode_canonical().unwrap(),
        )
        .unwrap();
        assert_eq!(cc_rt, commit_cert);
        let pa_rt = AcceptedConsensusNativeParentAuthorityCertificateV1::decode_canonical(
            &pa_cert.encode_canonical().unwrap(),
        )
        .unwrap();
        assert_eq!(pa_rt, pa_cert);
        let blob = pa_cert.to_opaque_authority_blob().unwrap();
        assert!(!blob.interpreted_as_live_authority());

        // Golden tags pinned against clean-room Python (tools/consensus_1_vectors.py)
        let cc_tag = commit_cert.tag().unwrap().to_string();
        let pa_tag = pa_cert.tag().unwrap().to_string();
        let pqc_dig = commit_cert.prepare_qc.qc_digest().unwrap();
        let cqc_dig = commit_cert.commit_qc.qc_digest().unwrap();
        let adm_id = pa_cert.admission_id().unwrap();
        assert_eq!(
            cc_tag,
            "#bb.receipt.sha256.hubj5ifmhrm4xxyu4gw5yzexokxryskow6yvge6uflam4xvyawna"
        );
        assert_eq!(
            pa_tag,
            "#bb.receipt.sha256.ovdkxdeii3nlsw6xmpidj6xkqun3mmmmnir36mwqikie3levp4rq"
        );
        assert_ne!(pqc_dig, cqc_dig);
        assert_ne!(adm_id, [0u8; 32]);
        assert_eq!(pqc_dig[..4], [0xcb, 0x00, 0x7a, 0xe8]);
        // Pin BBCON1 loopback wire header framing endianness for bbmutate
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut hdr = [0u8; consensus_net::CONS_WIRE_HEADER_LEN];
            hdr[0..6].copy_from_slice(&CONS_WIRE_MAGIC);
            hdr[6..8].copy_from_slice(&CONSENSUS_1_VERSION.to_be_bytes());
            hdr[8] = consensus_net::ConsensusWireMsgKindV1::PrepareQc.code();
            hdr[9..13].copy_from_slice(&32u32.to_be_bytes());
            let (k, l) =
                consensus_net::preflight_wire_header(&hdr, &cluster.fixture.bundle.bounds_profile)
                    .unwrap();
            assert_eq!(k, consensus_net::ConsensusWireMsgKindV1::PrepareQc);
            assert_eq!(l, 32);
        }
    }
}
