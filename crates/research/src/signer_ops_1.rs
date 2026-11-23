//! `SIGNER-OPS-1` (`PS-085`): role-separated validator signer boundary, monotonic
//! anti-double-sign watermark, key ceremony, encrypted backup/restore with stale-backup floor
//! protection, compromise/revocation integration (`CRYPTO-1`), crash-safe atomic disk persistence
//! with exclusive OS file locking, and `127.0.0.1` loopback-TCP remote-signer RPC (`BBSIG1`).
//!
//! Strictly behind `--features protocol-research`. The core state machine and canonical codecs
//! are 100% `wasm32`-clean; OS persistence, exclusive locking, and loopback TCP live inside
//! `#[cfg(not(target_arch = "wasm32"))] pub mod signer_io` with zero additional Cargo crates.
//!
//! # Normative Non-Claims (`CAPABILITY_TREE.md`)
//! Secure signer operation does not establish validator honesty, quorum safety, or network
//! availability.

use std::fmt;

use sha2::{Digest, Sha256};

use crate::consensus_1::{ConsensusProposalV1, ConsensusVotePhaseV1, ConsensusVoteV1};
use crate::crypto_1::{
    Crypto1Error, CustodyBackupEnvelopeV1, KeyLifecycleRegistryV1, KeyStatusV1,
    canonical_role_for_domain,
};
use crate::crypto_d1::{
    CanonicalEd25519PublicKeyV1, CanonicalEd25519SignatureV1, CryptoD1Error, HashSuiteIdV1,
    SignatureSuiteIdV1, SigningDomainTagV1, SigningPreimageV1, sign_preimage_deterministic,
};
use crate::executor_1::ExecutorForkContextV1;
use crate::identity_d1::{ActorIdV1, CredentialRoleV1};
use crate::protocol::{ContentTag, TagKind, hash_canonical};

/// Canonical schema version for `SIGNER-OPS-1` artifacts.
pub const SIGNER_OPS_1_VERSION: u16 = 1;

/// Canonical schema string for [`SignerPolicyManifestV1`].
pub const SIGNER_OPS_1_POLICY_SCHEMA: &[u8] = b"bitblock/signer-ops-1/signing-policy/v1";

/// Canonical schema string for [`SignerAuditReceiptV1`].
pub const SIGNER_OPS_1_AUDIT_RECEIPT_SCHEMA: &[u8] = b"bitblock/signer-ops-1/audit-receipt/v1";

const SOPS_CEREMONY_PREFIX: &[u8] = b"bb.sops1.key_ceremony.v1";
const SOPS_BACKUP_PAIR_PREFIX: &[u8] = b"bb.sops1.backup_pair.v1";
const SOPS_WATERMARK_MAC_PREFIX: &[u8] = b"bb.sops1.watermark_mac.v1";

/// Failure modes for `SIGNER-OPS-1` role separation, anti-double-sign, and operational drills.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignerOps1Error {
    /// Malformed or non-canonical wire encoding.
    InvalidWire(&'static str),
    /// Signer policy violated strict role separation or stale-backup invariants.
    InvalidPolicy(&'static str),
    /// Requested signing domain or credential role is unauthorized for this validator boundary.
    RoleOrDomainMismatch(&'static str),
    /// Attempted to sign a distinct `block_id` at an already-signed watermark.
    EquivocationRefused {
        /// Watermark at which a conflicting block was requested.
        watermark: SignerWatermarkV1,
        /// Previously signed block identifier.
        signed_block_id: [u8; 32],
        /// Conflicting candidate block identifier that was refused.
        conflicting_block_id: [u8; 32],
    },
    /// Attempted to sign at a watermark strictly below the committed monotonic floor.
    WatermarkRollbackRefused {
        /// Requested watermark that regressed behind the floor.
        requested: SignerWatermarkV1,
        /// Committed monotonic high-water mark.
        committed: SignerWatermarkV1,
    },
    /// Active key was revoked or reported compromised in `CRYPTO-1`.
    KeyRevokedOrCompromised(&'static str),
    /// Request or standby replica presented a stale `failover_epoch` fence token.
    StaleFailoverFence {
        /// Presented stale failover epoch.
        presented: u64,
        /// Current authoritative failover epoch.
        required: u64,
    },
    /// Backup restore rejected because the restored watermark lags behind the observed floor.
    StaleBackupFloorRefused {
        /// Restored backup watermark.
        backup_watermark: SignerWatermarkV1,
        /// Minimum safe operator-attested watermark floor.
        required_floor: SignerWatermarkV1,
    },
    /// Concurrent replica lock failed (another signer instance owns the exclusive lock).
    ConcurrentReplicaLocked(&'static str),
    /// Persisted watermark file is truncated, corrupt, or failed its integrity checksum.
    CorruptWatermarkState(&'static str),
    /// OS filesystem or loopback TCP transport error.
    IoError(String),
    /// Wrapped upstream `CRYPTO-1` error.
    Crypto1(String),
    /// Wrapped upstream `CRYPTO-D1` error.
    CryptoD1(String),
}

impl From<Crypto1Error> for SignerOps1Error {
    fn from(e: Crypto1Error) -> Self {
        Self::Crypto1(e.to_string())
    }
}

impl From<CryptoD1Error> for SignerOps1Error {
    fn from(e: CryptoD1Error) -> Self {
        Self::CryptoD1(e.to_string())
    }
}

impl fmt::Display for SignerOps1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for SignerOps1Error {}

/// Canonical consensus signing phase inside [`SignerWatermarkV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum SignerPhaseCodeV1 {
    /// Initial uninitialized / pre-height floor sentinel.
    Init = 0,
    /// Block proposal phase (`SigningDomainTagV1::BlockProposal`).
    Proposal = 1,
    /// HotStuff prepare vote phase (`SigningDomainTagV1::ConsensusVote`).
    PrepareVote = 2,
    /// HotStuff commit vote phase (`SigningDomainTagV1::ConsensusVote`).
    CommitVote = 3,
}

impl SignerPhaseCodeV1 {
    /// Returns the canonical 1-byte discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte phase discriminant.
    pub const fn from_code(code: u8) -> Result<Self, SignerOps1Error> {
        match code {
            0 => Ok(Self::Init),
            1 => Ok(Self::Proposal),
            2 => Ok(Self::PrepareVote),
            3 => Ok(Self::CommitVote),
            _ => Err(SignerOps1Error::InvalidWire(
                "unknown SignerPhaseCodeV1 code",
            )),
        }
    }

    /// Maps a `CONSENSUS-1` vote phase to its ordered signer watermark phase.
    pub const fn from_vote_phase(phase: ConsensusVotePhaseV1) -> Self {
        match phase {
            ConsensusVotePhaseV1::Prepare => Self::PrepareVote,
            ConsensusVotePhaseV1::Commit => Self::CommitVote,
        }
    }
}

/// Strictly ordered monotonic anti-double-sign watermark (`21` canonical bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SignerWatermarkV1 {
    /// Active validator schedule epoch.
    pub epoch: u64,
    /// Consensus block height.
    pub height: u64,
    /// View / round number at `height`.
    pub round: u32,
    /// Ordered signing phase (`Init < Proposal < PrepareVote < CommitVote`).
    pub phase: SignerPhaseCodeV1,
}

impl SignerWatermarkV1 {
    /// Fixed canonical wire byte length (`21` bytes).
    pub const WIRE_LEN: usize = 8 + 8 + 4 + 1;

    /// Zero genesis floor watermark `(0, 0, 0, Init)`.
    pub const ZERO: Self = Self {
        epoch: 0,
        height: 0,
        round: 0,
        phase: SignerPhaseCodeV1::Init,
    };

    /// Encodes this watermark into 21 canonical big-endian bytes.
    pub fn encode_canonical(&self) -> [u8; Self::WIRE_LEN] {
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..8].copy_from_slice(&self.epoch.to_be_bytes());
        out[8..16].copy_from_slice(&self.height.to_be_bytes());
        out[16..20].copy_from_slice(&self.round.to_be_bytes());
        out[20] = self.phase.code();
        out
    }

    /// Decodes a 21-byte canonical watermark.
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, SignerOps1Error> {
        if raw.len() != Self::WIRE_LEN {
            return Err(SignerOps1Error::InvalidWire(
                "invalid SignerWatermarkV1 length",
            ));
        }
        Ok(Self {
            epoch: u64::from_be_bytes(raw[0..8].try_into().expect("8 bytes")),
            height: u64::from_be_bytes(raw[8..16].try_into().expect("8 bytes")),
            round: u32::from_be_bytes(raw[16..20].try_into().expect("4 bytes")),
            phase: SignerPhaseCodeV1::from_code(raw[20])?,
        })
    }
}

/// Canonical audit decision recorded in [`SignerAuditReceiptV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum SignerDecisionV1 {
    /// Signature approved after atomic watermark advance and role/key validation.
    SignedApproved = 1,
    /// Refused: distinct `block_id` requested at an already-signed watermark.
    RefusedEquivocation = 2,
    /// Refused: requested watermark regressed behind committed high-water mark.
    RefusedRollback = 3,
    /// Refused: unauthorized role or cross-domain signing request.
    RefusedRoleMismatch = 4,
    /// Refused: signing key is revoked or compromised in `CRYPTO-1`.
    RefusedRevokedOrCompromised = 5,
    /// Refused: presented `failover_epoch` fence token is stale.
    RefusedStaleFailoverFence = 6,
    /// Refused: backup restore lags behind required watermark floor.
    RefusedStaleBackupFloor = 7,
    /// Refused: persisted watermark state was corrupt or truncated.
    RefusedCorruptWatermark = 8,
}

impl SignerDecisionV1 {
    /// Returns the canonical 1-byte discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte audit decision discriminant.
    pub const fn from_code(code: u8) -> Result<Self, SignerOps1Error> {
        match code {
            1 => Ok(Self::SignedApproved),
            2 => Ok(Self::RefusedEquivocation),
            3 => Ok(Self::RefusedRollback),
            4 => Ok(Self::RefusedRoleMismatch),
            5 => Ok(Self::RefusedRevokedOrCompromised),
            6 => Ok(Self::RefusedStaleFailoverFence),
            7 => Ok(Self::RefusedStaleBackupFloor),
            8 => Ok(Self::RefusedCorruptWatermark),
            _ => Err(SignerOps1Error::InvalidWire(
                "unknown SignerDecisionV1 code",
            )),
        }
    }
}

/// Auditable validator signing policy (`bitblock/signer-ops-1/signing-policy/v1`, `257` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SignerPolicyManifestV1 {
    /// Schema version (`1`).
    pub policy_version: u16,
    /// Immutable `GENESIS-D1` identifier.
    pub genesis_id: [u8; 32],
    /// Governing `PROFILE-D1` launch profile identifier.
    pub profile_id: [u8; 32],
    /// Validator actor identifier owning this signer boundary.
    pub validator_id: ActorIdV1,
    /// Dedicated block-proposal public key (`CredentialRoleV1::ValidatorProposal`).
    pub proposal_key: CanonicalEd25519PublicKeyV1,
    /// Dedicated consensus-vote public key (`CredentialRoleV1::ValidatorVote`, `!= proposal_key`).
    pub vote_key: CanonicalEd25519PublicKeyV1,
    /// Digest of the deterministic key-ceremony transcript binding keys & validator identity.
    pub key_ceremony_transcript_digest: [u8; 32],
    /// Digest of the sealed `CRYPTO-1` [`CustodyBackupEnvelopeV1`] pair.
    pub backup_envelope_digest: [u8; 32],
    /// Monotonic active/standby failover fencing epoch (`>= 1`).
    pub failover_epoch: u64,
    /// Minimum safe watermark floor enforced across restarts, failovers, and backup restores.
    pub min_watermark_floor: SignerWatermarkV1,
    /// Must strictly be `true` (forbids cross-role signing or key reuse).
    pub strict_role_separation: bool,
    /// Must strictly be `false` (forbids silently resuming from a stale backup).
    pub allow_stale_backup_resume: bool,
}

impl SignerPolicyManifestV1 {
    /// Fixed canonical big-endian wire length (`257` bytes).
    pub const WIRE_LEN: usize = 2 + 32 * 7 + 8 + SignerWatermarkV1::WIRE_LEN + 1 + 1;

    /// Validates role separation, distinct keys, failover epoch, and non-stale-backup invariant.
    pub fn validate(&self) -> Result<(), SignerOps1Error> {
        if self.policy_version != SIGNER_OPS_1_VERSION {
            return Err(SignerOps1Error::InvalidWire("policy_version must be 1"));
        }
        if self.proposal_key == self.vote_key {
            return Err(SignerOps1Error::InvalidPolicy(
                "proposal_key and vote_key must be distinct",
            ));
        }
        if self.failover_epoch == 0 {
            return Err(SignerOps1Error::InvalidPolicy(
                "failover_epoch must be >= 1",
            ));
        }
        if !self.strict_role_separation {
            return Err(SignerOps1Error::InvalidPolicy(
                "strict_role_separation must be true",
            ));
        }
        if self.allow_stale_backup_resume {
            return Err(SignerOps1Error::InvalidPolicy(
                "allow_stale_backup_resume must be false",
            ));
        }
        Ok(())
    }

    /// Encodes this signing policy into 257 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], SignerOps1Error> {
        self.validate()?;
        let mut out = Vec::with_capacity(Self::WIRE_LEN);
        out.extend_from_slice(&self.policy_version.to_be_bytes());
        out.extend_from_slice(&self.genesis_id);
        out.extend_from_slice(&self.profile_id);
        out.extend_from_slice(self.validator_id.as_bytes());
        out.extend_from_slice(self.proposal_key.as_bytes());
        out.extend_from_slice(self.vote_key.as_bytes());
        out.extend_from_slice(&self.key_ceremony_transcript_digest);
        out.extend_from_slice(&self.backup_envelope_digest);
        out.extend_from_slice(&self.failover_epoch.to_be_bytes());
        out.extend_from_slice(&self.min_watermark_floor.encode_canonical());
        out.push(u8::from(self.strict_role_separation));
        out.push(u8::from(self.allow_stale_backup_resume));
        out.try_into()
            .map_err(|_| SignerOps1Error::InvalidWire("policy wire length mismatch"))
    }

    /// Decodes and validates a 257-byte canonical [`SignerPolicyManifestV1`].
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, SignerOps1Error> {
        if raw.len() != Self::WIRE_LEN {
            return Err(SignerOps1Error::InvalidWire(
                "invalid SignerPolicyManifestV1 length",
            ));
        }
        let mut cur = ByteCursor::new(raw);
        let policy = Self {
            policy_version: cur.u16_be(),
            genesis_id: cur.arr32(),
            profile_id: cur.arr32(),
            validator_id: ActorIdV1::from_bytes(cur.arr32()),
            proposal_key: CanonicalEd25519PublicKeyV1::from_bytes(cur.arr32())?,
            vote_key: CanonicalEd25519PublicKeyV1::from_bytes(cur.arr32())?,
            key_ceremony_transcript_digest: cur.arr32(),
            backup_envelope_digest: cur.arr32(),
            failover_epoch: cur.u64_be(),
            min_watermark_floor: SignerWatermarkV1::decode_canonical(
                cur.slice(SignerWatermarkV1::WIRE_LEN),
            )?,
            strict_role_separation: cur.flag()?,
            allow_stale_backup_resume: cur.flag()?,
        };
        policy.validate()?;
        Ok(policy)
    }

    /// Computes the canonical `ContentTag` over `SIGNER_OPS_1_POLICY_SCHEMA`.
    pub fn tag(&self) -> Result<ContentTag, SignerOps1Error> {
        let enc = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            SIGNER_OPS_1_POLICY_SCHEMA,
            enc.len() as u64,
            |w| w(&enc),
        )
        .expect("static schema and fixed length"))
    }
}

/// Tamper-evident hash-chained signer audit receipt (`bitblock/signer-ops-1/audit-receipt/v1`,
/// `267` bytes) emitted for every approved signature or fail-closed refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SignerAuditReceiptV1 {
    /// Schema version (`1`).
    pub receipt_version: u16,
    /// Immutable `GENESIS-D1` identifier.
    pub genesis_id: [u8; 32],
    /// Governing `PROFILE-D1` launch profile identifier.
    pub profile_id: [u8; 32],
    /// Validator actor identifier.
    pub validator_id: ActorIdV1,
    /// Canonical `ContentTag` digest of the active [`SignerPolicyManifestV1`].
    pub policy_digest: [u8; 32],
    /// Public key targeted by this signing attempt.
    pub signer_public_key: CanonicalEd25519PublicKeyV1,
    /// Canonical [`CredentialRoleV1`] code (`2 = ValidatorVote`, `3 = ValidatorProposal`, etc.).
    pub credential_role_code: u8,
    /// Canonical [`SigningDomainTagV1`] code (`3 = BlockProposal`, `4 = ConsensusVote`, etc.).
    pub signing_domain_code: u8,
    /// Failover fencing epoch active at decision time.
    pub failover_epoch: u64,
    /// Strictly monotonic audit counter (`>= 1`).
    pub state_counter: u64,
    /// Watermark requested by the caller.
    pub watermark: SignerWatermarkV1,
    /// Target consensus `block_id`.
    pub block_id: [u8; 32],
    /// Exact approval or refusal disposition.
    pub decision: SignerDecisionV1,
    /// `SHA-256` of the preceding [`SignerAuditReceiptV1`] wire bytes (`[0; 32]` at genesis).
    pub prev_audit_digest: [u8; 32],
    /// Must strictly be `true` iff `decision == SignerDecisionV1::SignedApproved`.
    pub signature_emitted: bool,
}

impl SignerAuditReceiptV1 {
    /// Fixed canonical big-endian wire length (`267` bytes).
    pub const WIRE_LEN: usize =
        2 + 32 * 5 + 1 + 1 + 8 + 8 + SignerWatermarkV1::WIRE_LEN + 32 + 1 + 32 + 1;

    /// Validates schema version, counter positivity, and exact `decision <-> signature_emitted` lock.
    pub fn validate(&self) -> Result<(), SignerOps1Error> {
        if self.receipt_version != SIGNER_OPS_1_VERSION {
            return Err(SignerOps1Error::InvalidWire("receipt_version must be 1"));
        }
        if self.failover_epoch == 0 || self.state_counter == 0 {
            return Err(SignerOps1Error::InvalidWire(
                "failover_epoch and state_counter must be >= 1",
            ));
        }
        let expected_emit = self.decision == SignerDecisionV1::SignedApproved;
        if self.signature_emitted != expected_emit {
            return Err(SignerOps1Error::InvalidWire(
                "signature_emitted disagrees with decision",
            ));
        }
        Ok(())
    }

    /// Encodes this audit receipt into 267 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], SignerOps1Error> {
        self.validate()?;
        let mut out = Vec::with_capacity(Self::WIRE_LEN);
        out.extend_from_slice(&self.receipt_version.to_be_bytes());
        out.extend_from_slice(&self.genesis_id);
        out.extend_from_slice(&self.profile_id);
        out.extend_from_slice(self.validator_id.as_bytes());
        out.extend_from_slice(&self.policy_digest);
        out.extend_from_slice(self.signer_public_key.as_bytes());
        out.push(self.credential_role_code);
        out.push(self.signing_domain_code);
        out.extend_from_slice(&self.failover_epoch.to_be_bytes());
        out.extend_from_slice(&self.state_counter.to_be_bytes());
        out.extend_from_slice(&self.watermark.encode_canonical());
        out.extend_from_slice(&self.block_id);
        out.push(self.decision.code());
        out.extend_from_slice(&self.prev_audit_digest);
        out.push(u8::from(self.signature_emitted));
        out.try_into()
            .map_err(|_| SignerOps1Error::InvalidWire("audit receipt wire length mismatch"))
    }

    /// Decodes and validates a 267-byte canonical [`SignerAuditReceiptV1`].
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, SignerOps1Error> {
        if raw.len() != Self::WIRE_LEN {
            return Err(SignerOps1Error::InvalidWire(
                "invalid SignerAuditReceiptV1 length",
            ));
        }
        let mut cur = ByteCursor::new(raw);
        let rcpt = Self {
            receipt_version: cur.u16_be(),
            genesis_id: cur.arr32(),
            profile_id: cur.arr32(),
            validator_id: ActorIdV1::from_bytes(cur.arr32()),
            policy_digest: cur.arr32(),
            signer_public_key: CanonicalEd25519PublicKeyV1::from_bytes(cur.arr32())?,
            credential_role_code: cur.byte(),
            signing_domain_code: cur.byte(),
            failover_epoch: cur.u64_be(),
            state_counter: cur.u64_be(),
            watermark: SignerWatermarkV1::decode_canonical(cur.slice(SignerWatermarkV1::WIRE_LEN))?,
            block_id: cur.arr32(),
            decision: SignerDecisionV1::from_code(cur.byte())?,
            prev_audit_digest: cur.arr32(),
            signature_emitted: cur.flag()?,
        };
        rcpt.validate()?;
        Ok(rcpt)
    }

    /// Returns the `SHA-256` digest of this receipt's canonical wire bytes (for audit chaining).
    pub fn wire_sha256(&self) -> Result<[u8; 32], SignerOps1Error> {
        Ok(Sha256::digest(self.encode_canonical()?).into())
    }

    /// Computes the canonical `ContentTag` over `SIGNER_OPS_1_AUDIT_RECEIPT_SCHEMA`.
    pub fn tag(&self) -> Result<ContentTag, SignerOps1Error> {
        let enc = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            SIGNER_OPS_1_AUDIT_RECEIPT_SCHEMA,
            enc.len() as u64,
            |w| w(&enc),
        )
        .expect("static schema and fixed length"))
    }
}

/// Encrypted key-ceremony output binding role-separated validator keys, `CRYPTO-1` lifecycle
/// registration, and sealed [`CustodyBackupEnvelopeV1`] disaster-recovery envelopes.
#[derive(Clone, Debug)]
pub struct ValidatorKeyCeremonyBundleV1 {
    /// Governing [`SignerPolicyManifestV1`] signed into audit receipts.
    pub policy: SignerPolicyManifestV1,
    /// Sealed `CRYPTO-1` backup envelope for the `ValidatorProposal` secret seed.
    pub proposal_backup: CustodyBackupEnvelopeV1,
    /// Sealed `CRYPTO-1` backup envelope for the `ValidatorVote` secret seed.
    pub vote_backup: CustodyBackupEnvelopeV1,
}

impl ValidatorKeyCeremonyBundleV1 {
    /// Executes a deterministic role-separated validator key ceremony, registering both keys in
    /// `registry` and sealing authenticated `CustodyBackupEnvelopeV1` disaster-recovery backups.
    #[allow(clippy::too_many_arguments)]
    pub fn execute(
        fork: &ExecutorForkContextV1,
        validator_id: ActorIdV1,
        proposal_seed: &[u8; 32],
        vote_seed: &[u8; 32],
        passphrase: &[u8],
        proposal_salt: [u8; 16],
        proposal_nonce: [u8; 16],
        vote_salt: [u8; 16],
        vote_nonce: [u8; 16],
        kdf_iterations: u32,
        registry: &mut KeyLifecycleRegistryV1,
    ) -> Result<Self, SignerOps1Error> {
        let proposal_backup = CustodyBackupEnvelopeV1::seal(
            proposal_seed,
            SigningDomainTagV1::BlockProposal,
            passphrase,
            proposal_salt,
            proposal_nonce,
            kdf_iterations,
        )?;
        let vote_backup = CustodyBackupEnvelopeV1::seal(
            vote_seed,
            SigningDomainTagV1::ConsensusVote,
            passphrase,
            vote_salt,
            vote_nonce,
            kdf_iterations,
        )?;
        let proposal_key = proposal_backup.public_key;
        let vote_key = vote_backup.public_key;
        if proposal_key == vote_key {
            return Err(SignerOps1Error::InvalidPolicy(
                "ceremony proposal_key and vote_key must be distinct",
            ));
        }
        registry.register_active_key(proposal_key, SigningDomainTagV1::BlockProposal, 0)?;
        registry.register_active_key(vote_key, SigningDomainTagV1::ConsensusVote, 0)?;

        let mut ch = Sha256::new();
        ch.update(SOPS_CEREMONY_PREFIX);
        ch.update(fork.genesis_id);
        ch.update(fork.profile_id);
        ch.update(validator_id.as_bytes());
        ch.update(proposal_key.as_bytes());
        ch.update(vote_key.as_bytes());
        let key_ceremony_transcript_digest: [u8; 32] = ch.finalize().into();

        let mut bh = Sha256::new();
        bh.update(SOPS_BACKUP_PAIR_PREFIX);
        bh.update(proposal_backup.encode_canonical()?);
        bh.update(vote_backup.encode_canonical()?);
        let backup_envelope_digest: [u8; 32] = bh.finalize().into();

        let policy = SignerPolicyManifestV1 {
            policy_version: SIGNER_OPS_1_VERSION,
            genesis_id: fork.genesis_id,
            profile_id: fork.profile_id,
            validator_id,
            proposal_key,
            vote_key,
            key_ceremony_transcript_digest,
            backup_envelope_digest,
            failover_epoch: 1,
            min_watermark_floor: SignerWatermarkV1::ZERO,
            strict_role_separation: true,
            allow_stale_backup_resume: false,
        };
        policy.validate()?;
        Ok(Self {
            policy,
            proposal_backup,
            vote_backup,
        })
    }
}

/// Checksummed durable watermark state persisted atomically BEFORE releasing any signature bytes
/// (`141` canonical bytes = `109`-byte body + `32`-byte domain-separated integrity digest).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PersistedWatermarkStateV1 {
    /// Format version (`1`).
    pub version: u16,
    /// Active failover fencing epoch (`>= 1`).
    pub failover_epoch: u64,
    /// Strictly monotonic audit/state counter.
    pub state_counter: u64,
    /// Highest committed monotonic watermark across proposals & votes.
    pub last_watermark: SignerWatermarkV1,
    /// `block_id` signed at `last_watermark` (`[0; 32]` at init).
    pub last_block_id: [u8; 32],
    /// `SHA-256` head of the tamper-evident [`SignerAuditReceiptV1`] chain.
    pub audit_chain_head: [u8; 32],
    /// Minimum safe watermark floor enforced after failover or backup restore.
    pub min_watermark_floor: SignerWatermarkV1,
}

impl PersistedWatermarkStateV1 {
    /// Fixed checksummed wire length (`156` bytes = `124` body + `32` integrity hash).
    pub const WIRE_LEN: usize = 2 + 8 + 8 + 21 + 32 + 32 + 21 + 32;

    fn body_bytes(&self) -> [u8; Self::WIRE_LEN - 32] {
        let mut out = [0u8; Self::WIRE_LEN - 32];
        out[0..2].copy_from_slice(&self.version.to_be_bytes());
        out[2..10].copy_from_slice(&self.failover_epoch.to_be_bytes());
        out[10..18].copy_from_slice(&self.state_counter.to_be_bytes());
        out[18..39].copy_from_slice(&self.last_watermark.encode_canonical());
        out[39..71].copy_from_slice(&self.last_block_id);
        out[71..103].copy_from_slice(&self.audit_chain_head);
        out[103..124].copy_from_slice(&self.min_watermark_floor.encode_canonical());
        out
    }

    fn compute_mac(body: &[u8]) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(SOPS_WATERMARK_MAC_PREFIX);
        h.update(body);
        h.finalize().into()
    }

    /// Encodes this durable watermark state with a trailing 32-byte integrity digest.
    pub fn encode_checksummed(&self) -> [u8; Self::WIRE_LEN] {
        let body = self.body_bytes();
        let mac = Self::compute_mac(&body);
        let mut out = [0u8; Self::WIRE_LEN];
        out[..Self::WIRE_LEN - 32].copy_from_slice(&body);
        out[Self::WIRE_LEN - 32..].copy_from_slice(&mac);
        out
    }

    /// Decodes and checksum-verifies persisted watermark bytes fail-closed.
    pub fn decode_checksummed(raw: &[u8]) -> Result<Self, SignerOps1Error> {
        if raw.len() != Self::WIRE_LEN {
            return Err(SignerOps1Error::CorruptWatermarkState(
                "truncated or oversized persisted watermark state",
            ));
        }
        let body = &raw[..Self::WIRE_LEN - 32];
        let expected_mac = Self::compute_mac(body);
        if raw[Self::WIRE_LEN - 32..] != expected_mac {
            return Err(SignerOps1Error::CorruptWatermarkState(
                "persisted watermark state checksum mismatch",
            ));
        }
        let mut cur = ByteCursor::new(body);
        let version = cur.u16_be();
        if version != SIGNER_OPS_1_VERSION {
            return Err(SignerOps1Error::CorruptWatermarkState(
                "unsupported persisted watermark version",
            ));
        }
        let failover_epoch = cur.u64_be();
        if failover_epoch == 0 {
            return Err(SignerOps1Error::CorruptWatermarkState(
                "failover_epoch cannot be 0",
            ));
        }
        Ok(Self {
            version,
            failover_epoch,
            state_counter: cur.u64_be(),
            last_watermark: SignerWatermarkV1::decode_canonical(
                cur.slice(SignerWatermarkV1::WIRE_LEN),
            )?,
            last_block_id: cur.arr32(),
            audit_chain_head: cur.arr32(),
            min_watermark_floor: SignerWatermarkV1::decode_canonical(
                cur.slice(SignerWatermarkV1::WIRE_LEN),
            )?,
        })
    }
}

/// Portable `wasm32`-clean role-separated validator signer enforcing monotonic watermarks,
/// failover fencing, `CRYPTO-1` key lifecycle status, and hash-chained audit receipts.
#[derive(Clone, Debug)]
pub struct ValidatorSignerCoreV1 {
    fork_context: ExecutorForkContextV1,
    policy: SignerPolicyManifestV1,
    proposal_seed: [u8; 32],
    vote_seed: [u8; 32],
    state: PersistedWatermarkStateV1,
    audit_log: Vec<SignerAuditReceiptV1>,
}

impl ValidatorSignerCoreV1 {
    /// Initializes a fresh signer core from a verified [`ValidatorKeyCeremonyBundleV1`] and seeds.
    pub fn from_ceremony(
        fork_context: ExecutorForkContextV1,
        ceremony: &ValidatorKeyCeremonyBundleV1,
        proposal_seed: [u8; 32],
        vote_seed: [u8; 32],
    ) -> Result<Self, SignerOps1Error> {
        ceremony.policy.validate()?;
        let pk_p = CanonicalEd25519PublicKeyV1::from_secret_seed(&proposal_seed)?;
        let pk_v = CanonicalEd25519PublicKeyV1::from_secret_seed(&vote_seed)?;
        if pk_p != ceremony.policy.proposal_key || pk_v != ceremony.policy.vote_key {
            return Err(SignerOps1Error::InvalidPolicy(
                "seeds do not match ceremony public keys",
            ));
        }
        let state = PersistedWatermarkStateV1 {
            version: SIGNER_OPS_1_VERSION,
            failover_epoch: ceremony.policy.failover_epoch,
            state_counter: 0,
            last_watermark: SignerWatermarkV1::ZERO,
            last_block_id: [0u8; 32],
            audit_chain_head: [0u8; 32],
            min_watermark_floor: ceremony.policy.min_watermark_floor,
        };
        Ok(Self {
            fork_context,
            policy: ceremony.policy,
            proposal_seed,
            vote_seed,
            state,
            audit_log: Vec::new(),
        })
    }

    /// Restores a validator signer from `CRYPTO-1` encrypted [`CustodyBackupEnvelopeV1`] backups
    /// ONLY if the operator-attested `new_floor` strictly advances past `observed_last_watermark`
    /// and `new_failover_epoch` strictly exceeds `observed_failover_epoch` (prevents stale backup
    /// double-signing!).
    #[allow(clippy::too_many_arguments)]
    pub fn restore_from_encrypted_backup(
        fork_context: ExecutorForkContextV1,
        ceremony: &ValidatorKeyCeremonyBundleV1,
        passphrase: &[u8],
        backup_watermark: SignerWatermarkV1,
        observed_last_watermark: SignerWatermarkV1,
        observed_failover_epoch: u64,
        new_failover_epoch: u64,
        operator_safe_floor: SignerWatermarkV1,
    ) -> Result<Self, SignerOps1Error> {
        if new_failover_epoch <= observed_failover_epoch {
            return Err(SignerOps1Error::StaleFailoverFence {
                presented: new_failover_epoch,
                required: observed_failover_epoch + 1,
            });
        }
        if operator_safe_floor <= observed_last_watermark {
            return Err(SignerOps1Error::StaleBackupFloorRefused {
                backup_watermark,
                required_floor: observed_last_watermark,
            });
        }
        let proposal_seed = ceremony.proposal_backup.restore_seed(passphrase)?;
        let vote_seed = ceremony.vote_backup.restore_seed(passphrase)?;
        let mut policy = ceremony.policy;
        policy.failover_epoch = new_failover_epoch;
        policy.min_watermark_floor = operator_safe_floor;
        policy.validate()?;
        let state = PersistedWatermarkStateV1 {
            version: SIGNER_OPS_1_VERSION,
            failover_epoch: new_failover_epoch,
            state_counter: 0,
            last_watermark: operator_safe_floor,
            last_block_id: [0u8; 32],
            audit_chain_head: [0u8; 32],
            min_watermark_floor: operator_safe_floor,
        };
        Ok(Self {
            fork_context,
            policy,
            proposal_seed,
            vote_seed,
            state,
            audit_log: Vec::new(),
        })
    }

    /// Rehydrates a signer core from a previously persisted [`PersistedWatermarkStateV1`].
    pub fn from_persisted_state(
        fork_context: ExecutorForkContextV1,
        mut policy: SignerPolicyManifestV1,
        proposal_seed: [u8; 32],
        vote_seed: [u8; 32],
        state: PersistedWatermarkStateV1,
    ) -> Result<Self, SignerOps1Error> {
        policy.failover_epoch = state.failover_epoch;
        policy.min_watermark_floor = state.min_watermark_floor;
        policy.validate()?;
        Ok(Self {
            fork_context,
            policy,
            proposal_seed,
            vote_seed,
            state,
            audit_log: Vec::new(),
        })
    }

    /// Returns the governing [`SignerPolicyManifestV1`].
    pub const fn policy(&self) -> &SignerPolicyManifestV1 {
        &self.policy
    }

    /// Returns the current [`PersistedWatermarkStateV1`].
    pub const fn persisted_state(&self) -> &PersistedWatermarkStateV1 {
        &self.state
    }

    /// Returns the tamper-evident audit receipt log emitted during this session.
    pub fn audit_log(&self) -> &[SignerAuditReceiptV1] {
        &self.audit_log
    }

    /// Rotates a compromised or retired validator key inside this signer core after `CRYPTO-1`
    /// registers the fresh replacement key.
    pub fn install_rotated_key(
        &mut self,
        domain: SigningDomainTagV1,
        new_seed: [u8; 32],
        new_failover_epoch: u64,
        registry: &mut KeyLifecycleRegistryV1,
        effective_height: u64,
    ) -> Result<CanonicalEd25519PublicKeyV1, SignerOps1Error> {
        if new_failover_epoch <= self.state.failover_epoch {
            return Err(SignerOps1Error::StaleFailoverFence {
                presented: new_failover_epoch,
                required: self.state.failover_epoch + 1,
            });
        }
        let new_pk = CanonicalEd25519PublicKeyV1::from_secret_seed(&new_seed)?;
        match domain {
            SigningDomainTagV1::BlockProposal => {
                if new_pk == self.policy.vote_key {
                    return Err(SignerOps1Error::InvalidPolicy(
                        "new proposal key equals vote key",
                    ));
                }
                registry.register_active_key(
                    new_pk,
                    SigningDomainTagV1::BlockProposal,
                    effective_height,
                )?;
                self.proposal_seed = new_seed;
                self.policy.proposal_key = new_pk;
            }
            SigningDomainTagV1::ConsensusVote => {
                if new_pk == self.policy.proposal_key {
                    return Err(SignerOps1Error::InvalidPolicy(
                        "new vote key equals proposal key",
                    ));
                }
                registry.register_active_key(
                    new_pk,
                    SigningDomainTagV1::ConsensusVote,
                    effective_height,
                )?;
                self.vote_seed = new_seed;
                self.policy.vote_key = new_pk;
            }
            _ => {
                return Err(SignerOps1Error::RoleOrDomainMismatch(
                    "validator signer only manages BlockProposal and ConsensusVote keys",
                ));
            }
        }
        self.state.failover_epoch = new_failover_epoch;
        self.policy.failover_epoch = new_failover_epoch;
        Ok(new_pk)
    }

    fn record_audit(
        &mut self,
        signer_pk: CanonicalEd25519PublicKeyV1,
        role: CredentialRoleV1,
        domain: SigningDomainTagV1,
        watermark: SignerWatermarkV1,
        block_id: [u8; 32],
        decision: SignerDecisionV1,
    ) -> Result<SignerAuditReceiptV1, SignerOps1Error> {
        let next_counter = self.state.state_counter + 1;
        let rcpt = SignerAuditReceiptV1 {
            receipt_version: SIGNER_OPS_1_VERSION,
            genesis_id: self.policy.genesis_id,
            profile_id: self.policy.profile_id,
            validator_id: self.policy.validator_id,
            policy_digest: self.policy.tag()?.digest(),
            signer_public_key: signer_pk,
            credential_role_code: role.code(),
            signing_domain_code: domain.code(),
            failover_epoch: self.state.failover_epoch,
            state_counter: next_counter,
            watermark,
            block_id,
            decision,
            prev_audit_digest: self.state.audit_chain_head,
            signature_emitted: decision == SignerDecisionV1::SignedApproved,
        };
        let dig = rcpt.wire_sha256()?;
        self.state.state_counter = next_counter;
        self.state.audit_chain_head = dig;
        self.audit_log.push(rcpt);
        Ok(rcpt)
    }

    /// Evaluates role separation, key lifecycle, failover fencing, and monotonic anti-double-sign
    /// rules, returning the authorized Ed25519 signature and hash-chained [`SignerAuditReceiptV1`].
    #[allow(clippy::too_many_arguments)]
    pub fn authorize_and_sign_digest(
        &mut self,
        requested_role: CredentialRoleV1,
        requested_domain: SigningDomainTagV1,
        presented_failover_epoch: u64,
        watermark: SignerWatermarkV1,
        block_id: [u8; 32],
        unsigned_payload_digest: [u8; 32],
        registry: &KeyLifecycleRegistryV1,
    ) -> Result<(CanonicalEd25519SignatureV1, SignerAuditReceiptV1), SignerOps1Error> {
        let (expected_role, key_pk, seed) = match requested_domain {
            SigningDomainTagV1::BlockProposal => (
                CredentialRoleV1::ValidatorProposal,
                self.policy.proposal_key,
                self.proposal_seed,
            ),
            SigningDomainTagV1::ConsensusVote => (
                CredentialRoleV1::ValidatorVote,
                self.policy.vote_key,
                self.vote_seed,
            ),
            _ => {
                let _ = self.record_audit(
                    self.policy.vote_key,
                    requested_role,
                    requested_domain,
                    watermark,
                    block_id,
                    SignerDecisionV1::RefusedRoleMismatch,
                )?;
                return Err(SignerOps1Error::RoleOrDomainMismatch(
                    "non-validator signing domain refused by signer boundary",
                ));
            }
        };
        if requested_role != expected_role
            || canonical_role_for_domain(requested_domain) != expected_role
        {
            let _ = self.record_audit(
                key_pk,
                requested_role,
                requested_domain,
                watermark,
                block_id,
                SignerDecisionV1::RefusedRoleMismatch,
            )?;
            return Err(SignerOps1Error::RoleOrDomainMismatch(
                "credential role mismatches requested signing domain",
            ));
        }
        if presented_failover_epoch != self.state.failover_epoch {
            let _ = self.record_audit(
                key_pk,
                requested_role,
                requested_domain,
                watermark,
                block_id,
                SignerDecisionV1::RefusedStaleFailoverFence,
            )?;
            return Err(SignerOps1Error::StaleFailoverFence {
                presented: presented_failover_epoch,
                required: self.state.failover_epoch,
            });
        }
        let key_rec = registry.get(&key_pk);
        let key_active = key_rec.is_some_and(|r| {
            r.status == KeyStatusV1::Active
                && r.domain_tag == requested_domain
                && r.role == expected_role
        });
        if !key_active {
            let _ = self.record_audit(
                key_pk,
                requested_role,
                requested_domain,
                watermark,
                block_id,
                SignerDecisionV1::RefusedRevokedOrCompromised,
            )?;
            return Err(SignerOps1Error::KeyRevokedOrCompromised(
                "signing key is not Active in CRYPTO-1 KeyLifecycleRegistryV1",
            ));
        }
        if watermark < self.state.min_watermark_floor {
            let _ = self.record_audit(
                key_pk,
                requested_role,
                requested_domain,
                watermark,
                block_id,
                SignerDecisionV1::RefusedStaleBackupFloor,
            )?;
            return Err(SignerOps1Error::StaleBackupFloorRefused {
                backup_watermark: watermark,
                required_floor: self.state.min_watermark_floor,
            });
        }
        if watermark < self.state.last_watermark {
            let _ = self.record_audit(
                key_pk,
                requested_role,
                requested_domain,
                watermark,
                block_id,
                SignerDecisionV1::RefusedRollback,
            )?;
            return Err(SignerOps1Error::WatermarkRollbackRefused {
                requested: watermark,
                committed: self.state.last_watermark,
            });
        }
        if watermark == self.state.last_watermark && self.state.last_block_id != [0u8; 32] {
            if self.state.last_block_id != block_id {
                let _ = self.record_audit(
                    key_pk,
                    requested_role,
                    requested_domain,
                    watermark,
                    block_id,
                    SignerDecisionV1::RefusedEquivocation,
                )?;
                return Err(SignerOps1Error::EquivocationRefused {
                    watermark,
                    signed_block_id: self.state.last_block_id,
                    conflicting_block_id: block_id,
                });
            }
        } else {
            self.state.last_watermark = watermark;
            self.state.last_block_id = block_id;
        }

        let preimage = SigningPreimageV1 {
            protocol_version: 1,
            chain_id: self.fork_context.chain_id,
            genesis_id: self.fork_context.genesis_id,
            launch_manifest_id: self.fork_context.genesis_id,
            profile_id: self.fork_context.profile_id,
            domain_tag: requested_domain,
            signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
            hash_suite: HashSuiteIdV1::Sha256V1,
            payload_digest: unsigned_payload_digest,
        };
        let sig = sign_preimage_deterministic(&seed, &preimage)?;
        let rcpt = self.record_audit(
            key_pk,
            requested_role,
            requested_domain,
            watermark,
            block_id,
            SignerDecisionV1::SignedApproved,
        )?;
        Ok((sig, rcpt))
    }

    /// Signs a `CONSENSUS-1` [`ConsensusVoteV1`] strictly through the anti-double-sign boundary.
    pub fn sign_consensus_vote(
        &mut self,
        mut vote: ConsensusVoteV1,
        presented_failover_epoch: u64,
        registry: &KeyLifecycleRegistryV1,
    ) -> Result<(ConsensusVoteV1, SignerAuditReceiptV1), SignerOps1Error> {
        let wm = SignerWatermarkV1 {
            epoch: vote.epoch,
            height: vote.height,
            round: vote.view as u32,
            phase: SignerPhaseCodeV1::from_vote_phase(vote.phase),
        };
        let (sig, rcpt) = self.authorize_and_sign_digest(
            CredentialRoleV1::ValidatorVote,
            SigningDomainTagV1::ConsensusVote,
            presented_failover_epoch,
            wm,
            vote.block_id,
            vote.unsigned_payload_digest(),
            registry,
        )?;
        vote.vote_pubkey = *self.policy.vote_key.as_bytes();
        vote.signature = *sig.as_bytes();
        Ok((vote, rcpt))
    }

    /// Signs a `CONSENSUS-1` [`ConsensusProposalV1`] strictly through the anti-double-sign boundary.
    pub fn sign_consensus_proposal(
        &mut self,
        mut proposal: ConsensusProposalV1,
        presented_failover_epoch: u64,
        registry: &KeyLifecycleRegistryV1,
    ) -> Result<(ConsensusProposalV1, SignerAuditReceiptV1), SignerOps1Error> {
        let wm = SignerWatermarkV1 {
            epoch: proposal.epoch,
            height: proposal.height,
            round: proposal.view as u32,
            phase: SignerPhaseCodeV1::Proposal,
        };
        let (sig, rcpt) = self.authorize_and_sign_digest(
            CredentialRoleV1::ValidatorProposal,
            SigningDomainTagV1::BlockProposal,
            presented_failover_epoch,
            wm,
            proposal.block_id(),
            proposal.unsigned_payload_digest(),
            registry,
        )?;
        proposal.proposal_pubkey = *self.policy.proposal_key.as_bytes();
        proposal.signature = *sig.as_bytes();
        Ok((proposal, rcpt))
    }
}

/// Verifies that a slice of [`SignerAuditReceiptV1`] forms an unbroken hash-linked audit chain.
pub fn verify_audit_chain(chain: &[SignerAuditReceiptV1]) -> Result<(), SignerOps1Error> {
    let mut expected_prev = [0u8; 32];
    for (idx, rcpt) in chain.iter().enumerate() {
        rcpt.validate()?;
        if rcpt.state_counter != (idx as u64) + 1 {
            return Err(SignerOps1Error::InvalidWire("audit chain counter gap"));
        }
        if rcpt.prev_audit_digest != expected_prev {
            return Err(SignerOps1Error::InvalidWire(
                "audit chain prev_audit_digest mismatch",
            ));
        }
        expected_prev = rcpt.wire_sha256()?;
    }
    Ok(())
}

/// Native crash-safe disk watermark persistence, OS exclusive file lock, and `127.0.0.1`
/// loopback-TCP remote-signer RPC (`BBSIG1`) with zero external Cargo dependencies.
#[cfg(not(target_arch = "wasm32"))]
pub mod signer_io {
    use std::fs::{self, File, OpenOptions};
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpListener, TcpStream};
    use std::path::{Path, PathBuf};

    use super::*;

    /// 6-byte wire magic prefix for `SIGNER-OPS-1` loopback-TCP remote-signer RPC frames.
    pub const REMOTE_SIGNER_MAGIC: &[u8; 6] = b"BBSIG1";
    /// Maximum bounded payload size (`4 KiB`) enforced before allocating any RPC frame buffer.
    pub const MAX_REMOTE_SIGNER_FRAME_BYTES: usize = 4_096;

    /// Exclusive OS file lock guard preventing concurrent replica access to a signer directory.
    #[derive(Debug)]
    pub struct ExclusiveSignerLockV1 {
        _lock_file: File,
    }

    impl ExclusiveSignerLockV1 {
        /// Acquires a non-blocking exclusive OS lock on `lock_path` (`flock` on Unix,
        /// `share_mode(0)` on Windows). Fails immediately with [`SignerOps1Error::ConcurrentReplicaLocked`]
        /// if another handle holds the lock.
        pub fn acquire(lock_path: &Path) -> Result<Self, SignerOps1Error> {
            #[cfg(unix)]
            {
                use std::os::unix::io::AsRawFd;
                let file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .truncate(false)
                    .open(lock_path)
                    .map_err(|e| SignerOps1Error::IoError(e.to_string()))?;
                // SAFETY: `file` owns an open fd for this non-blocking advisory exclusive lock.
                let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
                if rc != 0 {
                    return Err(SignerOps1Error::ConcurrentReplicaLocked(
                        "exclusive flock already held by active replica",
                    ));
                }
                Ok(Self { _lock_file: file })
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                let file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .truncate(false)
                    .share_mode(0)
                    .open(lock_path)
                    .map_err(|_| {
                        SignerOps1Error::ConcurrentReplicaLocked(
                            "exclusive Windows share_mode(0) lock already held by active replica",
                        )
                    })?;
                Ok(Self { _lock_file: file })
            }
            #[cfg(not(any(unix, windows)))]
            {
                let _ = lock_path;
                Err(SignerOps1Error::ConcurrentReplicaLocked(
                    "unsupported OS for exclusive signer lock",
                ))
            }
        }
    }

    /// Crash-safe persistent validator signer holding an exclusive OS lock and committing every
    /// watermark update via `write(.tmp) -> sync_all() -> atomic rename()` BEFORE emitting any
    /// signature bytes.
    #[derive(Debug)]
    pub struct PersistentValidatorSignerV1 {
        state_dir: PathBuf,
        _lock: ExclusiveSignerLockV1,
        core: ValidatorSignerCoreV1,
    }

    impl PersistentValidatorSignerV1 {
        fn lock_path(dir: &Path) -> PathBuf {
            dir.join("signer.lock")
        }

        fn state_path(dir: &Path) -> PathBuf {
            dir.join("watermark.bin")
        }

        fn tmp_path(dir: &Path) -> PathBuf {
            dir.join("watermark.tmp")
        }

        fn write_state_atomic(
            dir: &Path,
            state: &PersistedWatermarkStateV1,
        ) -> Result<(), SignerOps1Error> {
            let tmp = Self::tmp_path(dir);
            let dst = Self::state_path(dir);
            {
                let mut f = OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&tmp)
                    .map_err(|e| SignerOps1Error::IoError(e.to_string()))?;
                f.write_all(&state.encode_checksummed())
                    .map_err(|e| SignerOps1Error::IoError(e.to_string()))?;
                f.sync_all()
                    .map_err(|e| SignerOps1Error::IoError(e.to_string()))?;
            }
            fs::rename(&tmp, &dst).map_err(|e| SignerOps1Error::IoError(e.to_string()))?;
            Ok(())
        }

        /// Opens or initializes a crash-safe persistent signer in `state_dir`, failing closed on
        /// concurrent replica locks or corrupt persisted watermark files.
        pub fn open_or_create(
            state_dir: PathBuf,
            fork_context: ExecutorForkContextV1,
            ceremony: &ValidatorKeyCeremonyBundleV1,
            proposal_seed: [u8; 32],
            vote_seed: [u8; 32],
        ) -> Result<Self, SignerOps1Error> {
            fs::create_dir_all(&state_dir).map_err(|e| SignerOps1Error::IoError(e.to_string()))?;
            let lock = ExclusiveSignerLockV1::acquire(&Self::lock_path(&state_dir))?;
            let st_path = Self::state_path(&state_dir);
            let core = if st_path.exists() {
                let raw =
                    fs::read(&st_path).map_err(|e| SignerOps1Error::IoError(e.to_string()))?;
                let persisted = PersistedWatermarkStateV1::decode_checksummed(&raw)?;
                ValidatorSignerCoreV1::from_persisted_state(
                    fork_context,
                    ceremony.policy,
                    proposal_seed,
                    vote_seed,
                    persisted,
                )?
            } else {
                let c = ValidatorSignerCoreV1::from_ceremony(
                    fork_context,
                    ceremony,
                    proposal_seed,
                    vote_seed,
                )?;
                Self::write_state_atomic(&state_dir, c.persisted_state())?;
                c
            };
            Ok(Self {
                state_dir,
                _lock: lock,
                core,
            })
        }

        /// Opens a persistent signer initialized from a verified encrypted backup & safe floor.
        pub fn open_from_core(
            state_dir: PathBuf,
            core: ValidatorSignerCoreV1,
        ) -> Result<Self, SignerOps1Error> {
            fs::create_dir_all(&state_dir).map_err(|e| SignerOps1Error::IoError(e.to_string()))?;
            let lock = ExclusiveSignerLockV1::acquire(&Self::lock_path(&state_dir))?;
            Self::write_state_atomic(&state_dir, core.persisted_state())?;
            Ok(Self {
                state_dir,
                _lock: lock,
                core,
            })
        }

        /// Returns a read-only reference to the inner [`ValidatorSignerCoreV1`].
        pub const fn core(&self) -> &ValidatorSignerCoreV1 {
            &self.core
        }

        /// Persists the updated watermark state atomically BEFORE releasing the authorized signature.
        #[allow(clippy::too_many_arguments)]
        pub fn sign_digest_durable(
            &mut self,
            role: CredentialRoleV1,
            domain: SigningDomainTagV1,
            failover_epoch: u64,
            watermark: SignerWatermarkV1,
            block_id: [u8; 32],
            unsigned_payload_digest: [u8; 32],
            registry: &KeyLifecycleRegistryV1,
        ) -> Result<(CanonicalEd25519SignatureV1, SignerAuditReceiptV1), SignerOps1Error> {
            let res = self.core.authorize_and_sign_digest(
                role,
                domain,
                failover_epoch,
                watermark,
                block_id,
                unsigned_payload_digest,
                registry,
            );
            // Persist updated counter/audit_head/watermark atomically on BOTH approval and refusal!
            Self::write_state_atomic(&self.state_dir, self.core.persisted_state())?;
            res
        }

        /// Durable wrapper around [`ValidatorSignerCoreV1::sign_consensus_vote`].
        pub fn sign_vote_durable(
            &mut self,
            vote: ConsensusVoteV1,
            failover_epoch: u64,
            registry: &KeyLifecycleRegistryV1,
        ) -> Result<(ConsensusVoteV1, SignerAuditReceiptV1), SignerOps1Error> {
            let res = self
                .core
                .sign_consensus_vote(vote, failover_epoch, registry);
            Self::write_state_atomic(&self.state_dir, self.core.persisted_state())?;
            res
        }
    }

    /// Fixed-length `BBSIG1` remote-signer signing request (`97` payload bytes).
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct RemoteSignRequestV1 {
        /// Requested credential role code.
        pub role_code: u8,
        /// Requested signing domain code.
        pub domain_code: u8,
        /// Failover fence epoch presented by the validator node.
        pub failover_epoch: u64,
        /// Monotonic consensus watermark.
        pub watermark: SignerWatermarkV1,
        /// Target consensus block identifier.
        pub block_id: [u8; 32],
        /// Signature-free canonical payload digest.
        pub unsigned_payload_digest: [u8; 32],
    }

    impl RemoteSignRequestV1 {
        /// Fixed payload byte length (`95` bytes).
        pub const PAYLOAD_LEN: usize = 1 + 1 + 8 + SignerWatermarkV1::WIRE_LEN + 32 + 32;

        /// Encodes this remote request into a `BBSIG1` bounded length-prefixed frame.
        pub fn encode_frame(&self) -> Vec<u8> {
            let mut out = Vec::with_capacity(6 + 4 + Self::PAYLOAD_LEN);
            out.extend_from_slice(REMOTE_SIGNER_MAGIC);
            out.extend_from_slice(&(Self::PAYLOAD_LEN as u32).to_be_bytes());
            out.push(self.role_code);
            out.push(self.domain_code);
            out.extend_from_slice(&self.failover_epoch.to_be_bytes());
            out.extend_from_slice(&self.watermark.encode_canonical());
            out.extend_from_slice(&self.block_id);
            out.extend_from_slice(&self.unsigned_payload_digest);
            out
        }

        /// Decodes a validated payload buffer.
        pub fn decode_payload(raw: &[u8]) -> Result<Self, SignerOps1Error> {
            if raw.len() != Self::PAYLOAD_LEN {
                return Err(SignerOps1Error::InvalidWire(
                    "invalid RemoteSignRequestV1 payload length",
                ));
            }
            let mut cur = ByteCursor::new(raw);
            Ok(Self {
                role_code: cur.byte(),
                domain_code: cur.byte(),
                failover_epoch: cur.u64_be(),
                watermark: SignerWatermarkV1::decode_canonical(
                    cur.slice(SignerWatermarkV1::WIRE_LEN),
                )?,
                block_id: cur.arr32(),
                unsigned_payload_digest: cur.arr32(),
            })
        }
    }

    fn read_framed(stream: &mut TcpStream) -> Result<Vec<u8>, SignerOps1Error> {
        let mut hdr = [0u8; 10];
        stream
            .read_exact(&mut hdr)
            .map_err(|e| SignerOps1Error::IoError(e.to_string()))?;
        if &hdr[0..6] != REMOTE_SIGNER_MAGIC {
            return Err(SignerOps1Error::InvalidWire("bad BBSIG1 magic"));
        }
        let len = u32::from_be_bytes([hdr[6], hdr[7], hdr[8], hdr[9]]) as usize;
        if len > MAX_REMOTE_SIGNER_FRAME_BYTES {
            return Err(SignerOps1Error::InvalidWire(
                "BBSIG1 frame exceeds 4 KiB cap",
            ));
        }
        let mut body = vec![0u8; len];
        stream
            .read_exact(&mut body)
            .map_err(|e| SignerOps1Error::IoError(e.to_string()))?;
        Ok(body)
    }

    fn write_framed(stream: &mut TcpStream, payload: &[u8]) -> Result<(), SignerOps1Error> {
        let mut hdr = [0u8; 10];
        hdr[0..6].copy_from_slice(REMOTE_SIGNER_MAGIC);
        hdr[6..10].copy_from_slice(&(payload.len() as u32).to_be_bytes());
        stream
            .write_all(&hdr)
            .and_then(|_| stream.write_all(payload))
            .and_then(|_| stream.flush())
            .map_err(|e| SignerOps1Error::IoError(e.to_string()))
    }

    /// Serves a single remote-signer RPC request over `listener` (`127.0.0.1:0`) backed by a
    /// crash-safe [`PersistentValidatorSignerV1`].
    pub fn serve_one_remote_sign_request(
        listener: &TcpListener,
        signer: &mut PersistentValidatorSignerV1,
        registry: &KeyLifecycleRegistryV1,
    ) -> Result<SignerAuditReceiptV1, SignerOps1Error> {
        let (mut stream, _) = listener
            .accept()
            .map_err(|e| SignerOps1Error::IoError(e.to_string()))?;
        let payload = read_framed(&mut stream)?;
        let req = RemoteSignRequestV1::decode_payload(&payload)?;
        let role = CredentialRoleV1::from_code(req.role_code)
            .map_err(|_| SignerOps1Error::InvalidWire("bad role code"))?;
        let domain = SigningDomainTagV1::from_code(req.domain_code)
            .map_err(|_| SignerOps1Error::InvalidWire("bad domain code"))?;
        match signer.sign_digest_durable(
            role,
            domain,
            req.failover_epoch,
            req.watermark,
            req.block_id,
            req.unsigned_payload_digest,
            registry,
        ) {
            Ok((sig, rcpt)) => {
                let mut resp = Vec::with_capacity(1 + 64 + SignerAuditReceiptV1::WIRE_LEN);
                resp.push(1u8); // OK
                resp.extend_from_slice(sig.as_bytes());
                resp.extend_from_slice(&rcpt.encode_canonical()?);
                write_framed(&mut stream, &resp)?;
                Ok(rcpt)
            }
            Err(_) => {
                let rcpt =
                    *signer
                        .core()
                        .audit_log()
                        .last()
                        .ok_or(SignerOps1Error::InvalidWire(
                            "missing refusal audit receipt",
                        ))?;
                let mut resp = Vec::with_capacity(1 + SignerAuditReceiptV1::WIRE_LEN);
                resp.push(0u8); // REFUSED
                resp.extend_from_slice(&rcpt.encode_canonical()?);
                write_framed(&mut stream, &resp)?;
                Ok(rcpt)
            }
        }
    }

    /// Sends one [`RemoteSignRequestV1`] to a `127.0.0.1` remote signer and parses either an
    /// approved `(signature, audit_receipt)` or a fail-closed refusal `(None, audit_receipt)`.
    pub fn call_remote_signer(
        addr: SocketAddr,
        req: &RemoteSignRequestV1,
    ) -> Result<(Option<CanonicalEd25519SignatureV1>, SignerAuditReceiptV1), SignerOps1Error> {
        let mut stream =
            TcpStream::connect(addr).map_err(|e| SignerOps1Error::IoError(e.to_string()))?;
        stream
            .write_all(&req.encode_frame())
            .and_then(|_| stream.flush())
            .map_err(|e| SignerOps1Error::IoError(e.to_string()))?;
        let resp = read_framed(&mut stream)?;
        if resp.is_empty() {
            return Err(SignerOps1Error::InvalidWire("empty remote signer response"));
        }
        if resp[0] == 1 {
            if resp.len() != 1 + 64 + SignerAuditReceiptV1::WIRE_LEN {
                return Err(SignerOps1Error::InvalidWire("malformed approved response"));
            }
            let sig =
                CanonicalEd25519SignatureV1::from_bytes(resp[1..65].try_into().expect("64 bytes"))?;
            let rcpt = SignerAuditReceiptV1::decode_canonical(&resp[65..])?;
            Ok((Some(sig), rcpt))
        } else if resp[0] == 0 {
            let rcpt = SignerAuditReceiptV1::decode_canonical(&resp[1..])?;
            Ok((None, rcpt))
        } else {
            Err(SignerOps1Error::InvalidWire("unknown remote signer status"))
        }
    }
}

struct ByteCursor<'a> {
    raw: &'a [u8],
    pos: usize,
}

impl<'a> ByteCursor<'a> {
    fn new(raw: &'a [u8]) -> Self {
        Self { raw, pos: 0 }
    }

    fn slice(&mut self, n: usize) -> &'a [u8] {
        let s = &self.raw[self.pos..self.pos + n];
        self.pos += n;
        s
    }

    fn byte(&mut self) -> u8 {
        self.slice(1)[0]
    }

    fn flag(&mut self) -> Result<bool, SignerOps1Error> {
        match self.byte() {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(SignerOps1Error::InvalidWire("non-boolean flag")),
        }
    }

    fn u16_be(&mut self) -> u16 {
        u16::from_be_bytes(self.slice(2).try_into().expect("2 bytes"))
    }

    fn u64_be(&mut self) -> u64 {
        u64::from_be_bytes(self.slice(8).try_into().expect("8 bytes"))
    }

    fn arr32(&mut self) -> [u8; 32] {
        self.slice(32).try_into().expect("32 bytes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consensus_1::ConsensusClusterHarnessV1;

    #[test]
    fn signer_ops_1_ceremony_policy_audit_and_golden_tags() {
        let cluster = ConsensusClusterHarnessV1::bootstrap_four_validators().unwrap();
        let vid = ActorIdV1::from_bytes([1u8; 32]);
        let prop_seed = ConsensusClusterHarnessV1::validator_proposal_seed(1);
        let vote_seed = ConsensusClusterHarnessV1::validator_vote_seed(1);
        let mut reg = KeyLifecycleRegistryV1::default();
        let ceremony = ValidatorKeyCeremonyBundleV1::execute(
            &cluster.fixture.fork_context,
            vid,
            &prop_seed,
            &vote_seed,
            b"bitblock-signer-ops-v1-ceremony",
            [0xA1; 16],
            [0xB1; 16],
            [0xA2; 16],
            [0xB2; 16],
            1_000,
            &mut reg,
        )
        .unwrap();

        let mut signer = ValidatorSignerCoreV1::from_ceremony(
            cluster.fixture.fork_context,
            &ceremony,
            prop_seed,
            vote_seed,
        )
        .unwrap();
        let wm = SignerWatermarkV1 {
            epoch: 0,
            height: 1,
            round: 0,
            phase: SignerPhaseCodeV1::PrepareVote,
        };
        let (sig, rcpt) = signer
            .authorize_and_sign_digest(
                CredentialRoleV1::ValidatorVote,
                SigningDomainTagV1::ConsensusVote,
                1,
                wm,
                [0x44; 32],
                [0x55; 32],
                &reg,
            )
            .unwrap();
        assert_eq!(sig.as_bytes().len(), 64);
        assert_eq!(rcpt.decision, SignerDecisionV1::SignedApproved);
        assert!(rcpt.signature_emitted);

        assert_eq!(
            SignerPolicyManifestV1::decode_canonical(&ceremony.policy.encode_canonical().unwrap())
                .unwrap(),
            ceremony.policy
        );
        assert_eq!(
            SignerAuditReceiptV1::decode_canonical(&rcpt.encode_canonical().unwrap()).unwrap(),
            rcpt
        );

        let policy_tag = ceremony.policy.tag().unwrap().to_string();
        let audit_tag = rcpt.tag().unwrap().to_string();
        assert_eq!(
            policy_tag,
            "#bb.policy.sha256.sbnuiur54vs5jeztkb724vb66dknfn2s6kqkgbgmqsgu4g35jqka"
        );
        assert_eq!(
            audit_tag,
            "#bb.receipt.sha256.zsx5ynhdhu23fbzjq6tuhzfe7unacilht67ub6sgmd4rnzr2v6eq"
        );
    }
}
