//! `CRYPTO-1`: operated key lifecycle, pure SHA-256/HMAC-SHA256 authenticated custody backup,
//! strict 9-domain cross-role separation, bounded batch verification with blame fallback, and
//! anti-downgrade `UPGRADE-1` suite migration policy (`PS-070`).
//!
//! Built strictly on `CRYPTO-D1` (`Ed25519StrictRfc8032V1` + `Sha256V1`) and `IDENTITY-D1` with
//! **zero new Cargo dependencies** (`Cargo.lock` untouched; `wasm32`-clean; byte-identical in
//! stdlib Python):
//! - **Operated key lifecycle (`KeyLifecycleRecordV1`, `KeyLifecycleRegistryV1`):** deterministic
//!   `Active -> Rotated | Revoked | Compromised` state machine with irreversible compromise
//!   quarantine, strict cross-role key-reuse rejection across all 9 `SigningDomainTagV1` domains,
//!   and height-aware verification gating.
//! - **Authenticated custody backup/restore (`CustodyBackupEnvelopeV1`):** standard
//!   `PBKDF2-HMAC-SHA256` -> `HKDF-SHA256` key split -> `SHA-256-CTR` + `HMAC-SHA256`
//!   Encrypt-then-MAC envelope with constant-time MAC verification before decryption and public-key
//!   binding check on restore.
//! - **Anti-downgrade migration (`SuiteMigrationPolicyV1`):** explicit old/new-profile suite
//!   agility rules for `UPGRADE-1`, forbidding suite-ID downgrades and post-sunset legacy fallback.
//!
//! External third-party cryptographic audit and physical HSM custody remain open (`BOUNDED`);
//! a complete key lifecycle does not establish honest actors, human uniqueness, or consensus safety.

use std::collections::BTreeMap;
use std::fmt;

use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};

use crate::crypto_d1::{
    CanonicalEd25519PublicKeyV1, CanonicalEd25519SignatureV1, CryptoD1Error,
    ED25519_PUBLIC_KEY_LEN, HashSuiteIdV1, SignatureSuiteIdV1, SigningDomainTagV1,
    SigningPreimageV1, verify_preimage_strict,
};
use crate::identity_d1::{CredentialRoleV1, IdentityD1Error};
use crate::protocol::{ContentTag, TagKind, hash_canonical};

/// Canonical schema string for a [`KeyLifecycleRecordV1`] record (`CRYPTO-1`).
pub const CRYPTO_1_KEY_LIFECYCLE_SCHEMA: &[u8] = b"bitblock/crypto-1/key-lifecycle-record/v1";
/// Canonical schema string for a [`CustodyBackupEnvelopeV1`] encrypted custody backup (`CRYPTO-1`).
pub const CRYPTO_1_CUSTODY_ENVELOPE_SCHEMA: &[u8] = b"bitblock/crypto-1/custody-backup-envelope/v1";
/// Canonical schema string for a [`SuiteMigrationPolicyV1`] anti-downgrade contract (`CRYPTO-1`).
pub const CRYPTO_1_MIGRATION_POLICY_SCHEMA: &[u8] = b"bitblock/crypto-1/suite-migration-policy/v1";

/// Maximum signatures permitted in one [`KeyLifecycleRegistryV1::verify_batch_authorized`] call
/// (aligned with `BOUNDS-1` `max_block_signatures = 2,048`).
pub const MAX_CRYPTO_1_BATCH_SIGNATURES: usize = 2_048;

/// Default `PBKDF2-HMAC-SHA256` iteration count for deterministic tests and vector suites.
pub const DEFAULT_CUSTODY_KDF_ITERATIONS: u32 = 1_000;
/// Maximum `PBKDF2-HMAC-SHA256` iterations permitted in one custody envelope preflight (`1,000,000`).
pub const MAX_CUSTODY_KDF_ITERATIONS: u32 = 1_000_000;

/// Errors returned by `CRYPTO-1` key lifecycle, custody envelope, batch, and migration checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Crypto1Error {
    /// Underlying `CRYPTO-D1` validation or strict Ed25519 verification error.
    Crypto(CryptoD1Error),
    /// Underlying `IDENTITY-D1` role decoding error.
    Identity(IdentityD1Error),
    /// Wire buffer had wrong length, unknown discriminant, or non-canonical padding.
    InvalidWire(&'static str),
    /// Role did not match the declared `SigningDomainTagV1`.
    RoleDomainMismatch {
        role: CredentialRoleV1,
        domain: SigningDomainTagV1,
    },
    /// Key was already registered in the lifecycle registry (same or different role/domain).
    CrossRoleOrDuplicateKeyReuse,
    /// Key was not found in the lifecycle registry.
    UnknownKey,
    /// Attempted an illegal lifecycle transition (for example un-revoking a compromised key).
    IllegalLifecycleTransition(&'static str),
    /// Verification attempted against a revoked, compromised, or post-rotation-window key.
    KeyNotAuthoritative(KeyStatusV1),
    /// Verification attempted with a preimage domain tag different from the key's bound domain.
    CrossDomainSignatureRejected {
        bound: SigningDomainTagV1,
        attempted: SigningDomainTagV1,
    },
    /// Custody backup MAC check failed in constant time before decryption.
    CustodyMacMismatch,
    /// Decrypted seed did not match the envelope's bound canonical Ed25519 public key.
    CustodyKeyBindingMismatch,
    /// Custody parameters (empty passphrase, out-of-range iterations, or invalid state) failed.
    InvalidCustodyParameters(&'static str),
    /// Suite migration policy attempted a downgrade or violated overlap/sunset invariants.
    SuiteDowngradeOrInvalidMigration(&'static str),
    /// Batch exceeded `BOUNDS-1` `MAX_CRYPTO_1_BATCH_SIGNATURES` or caller ceiling before verifying.
    BatchOverLimit { observed: usize, limit: usize },
    /// Batch verification failed at the reported item index after strict individual blame check.
    BatchItemFailed {
        first_invalid_index: usize,
        reason: &'static str,
    },
}

impl From<CryptoD1Error> for Crypto1Error {
    fn from(value: CryptoD1Error) -> Self {
        Self::Crypto(value)
    }
}

impl From<IdentityD1Error> for Crypto1Error {
    fn from(value: IdentityD1Error) -> Self {
        Self::Identity(value)
    }
}

impl fmt::Display for Crypto1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Crypto1Error {}

/// Returns the canonical [`CredentialRoleV1`] required for each [`SigningDomainTagV1`].
pub const fn canonical_role_for_domain(domain: SigningDomainTagV1) -> CredentialRoleV1 {
    match domain {
        SigningDomainTagV1::ActorTransaction => CredentialRoleV1::ActorAction,
        SigningDomainTagV1::ActorAdminRotation => CredentialRoleV1::ActorAdmin,
        SigningDomainTagV1::BlockProposal | SigningDomainTagV1::ValidatorTransport => {
            CredentialRoleV1::ValidatorProposal
        }
        SigningDomainTagV1::ConsensusVote | SigningDomainTagV1::RewardWithdrawal => {
            CredentialRoleV1::ValidatorVote
        }
        SigningDomainTagV1::GovernanceAction => CredentialRoleV1::GovernanceSigner,
        SigningDomainTagV1::EmergencyPause => CredentialRoleV1::EmergencyPauseSigner,
        SigningDomainTagV1::ReleaseArtifact => CredentialRoleV1::ReleaseSigner,
    }
}

/// Deterministic lifecycle state for an operated Ed25519 key (`CRYPTO-1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum KeyStatusV1 {
    /// Key is live and authorized exclusively for its bound `SigningDomainTagV1`.
    Active = 0,
    /// Key rotated to a fresh `successor_key` effective at `effective_height`.
    Rotated = 1,
    /// Key was voluntarily revoked and cannot sign or reactivate.
    Revoked = 2,
    /// Key was reported compromised; permanently quarantined with mandatory cooldown height.
    Compromised = 3,
}

impl KeyStatusV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte key status discriminant.
    pub const fn from_code(code: u8) -> Result<Self, Crypto1Error> {
        match code {
            0 => Ok(Self::Active),
            1 => Ok(Self::Rotated),
            2 => Ok(Self::Revoked),
            3 => Ok(Self::Compromised),
            _ => Err(Crypto1Error::InvalidWire("unknown KeyStatusV1 code")),
        }
    }
}

/// Canonical operated key lifecycle record (`bitblock/crypto-1/key-lifecycle-record/v1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyLifecycleRecordV1 {
    /// Record schema version (`1`).
    pub record_version: u16,
    /// Exclusive signing domain tag bound to `public_key`.
    pub domain_tag: SigningDomainTagV1,
    /// Credential role matching [`canonical_role_for_domain`].
    pub role: CredentialRoleV1,
    /// Current lifecycle state.
    pub status: KeyStatusV1,
    /// Canonical Ed25519 public key governed by this record.
    pub public_key: CanonicalEd25519PublicKeyV1,
    /// Mandatory successor public key when `status == KeyStatusV1::Rotated` (`None` otherwise).
    pub successor_key: Option<CanonicalEd25519PublicKeyV1>,
    /// Ledger height when the key record was created.
    pub created_height: u64,
    /// Ledger height when the current status took effect (`>= created_height`).
    pub effective_height: u64,
    /// Quarantine cooldown height (`> effective_height` iff `Compromised`, `0` otherwise).
    pub quarantine_until_height: u64,
}

impl KeyLifecycleRecordV1 {
    /// Fixed canonical big-endian wire length (`94` bytes).
    pub const WIRE_LEN: usize = 2 + 1 + 1 + 1 + 32 + 1 + 32 + 8 + 8 + 8;

    /// Validates role-domain binding, status-specific fields, and height monotonicity.
    pub fn validate(&self) -> Result<(), Crypto1Error> {
        if self.record_version != 1 {
            return Err(Crypto1Error::InvalidWire("record_version must equal 1"));
        }
        if canonical_role_for_domain(self.domain_tag) != self.role {
            return Err(Crypto1Error::RoleDomainMismatch {
                role: self.role,
                domain: self.domain_tag,
            });
        }
        if self.effective_height < self.created_height {
            return Err(Crypto1Error::IllegalLifecycleTransition(
                "effective_height < created_height",
            ));
        }
        match self.status {
            KeyStatusV1::Active | KeyStatusV1::Revoked => {
                if self.successor_key.is_some() || self.quarantine_until_height != 0 {
                    return Err(Crypto1Error::IllegalLifecycleTransition(
                        "unexpected successor_key or quarantine height",
                    ));
                }
            }
            KeyStatusV1::Rotated => {
                let Some(succ) = self.successor_key else {
                    return Err(Crypto1Error::IllegalLifecycleTransition(
                        "Rotated record requires successor_key",
                    ));
                };
                if succ == self.public_key || self.quarantine_until_height != 0 {
                    return Err(Crypto1Error::IllegalLifecycleTransition(
                        "invalid rotation successor or quarantine field",
                    ));
                }
            }
            KeyStatusV1::Compromised => {
                if self.successor_key.is_some()
                    || self.quarantine_until_height <= self.effective_height
                {
                    return Err(Crypto1Error::IllegalLifecycleTransition(
                        "Compromised record requires quarantine_until_height > effective_height",
                    ));
                }
            }
        }
        Ok(())
    }

    /// Encodes the record into fixed 94-byte canonical big-endian bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], Crypto1Error> {
        self.validate()?;
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.record_version.to_be_bytes());
        out[2] = self.domain_tag.code();
        out[3] = self.role.code();
        out[4] = self.status.code();
        out[5..37].copy_from_slice(self.public_key.as_bytes());
        if let Some(succ) = self.successor_key {
            out[37] = 1;
            out[38..70].copy_from_slice(succ.as_bytes());
        }
        out[70..78].copy_from_slice(&self.created_height.to_be_bytes());
        out[78..86].copy_from_slice(&self.effective_height.to_be_bytes());
        out[86..94].copy_from_slice(&self.quarantine_until_height.to_be_bytes());
        Ok(out)
    }

    /// Decodes and validates a 94-byte canonical [`KeyLifecycleRecordV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Crypto1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(Crypto1Error::InvalidWire("KeyLifecycleRecordV1 wrong len"));
        }
        let public_key = CanonicalEd25519PublicKeyV1::from_slice(&bytes[5..37])?;
        let successor_key = match bytes[37] {
            0 => {
                if bytes[38..70].iter().any(|&b| b != 0) {
                    return Err(Crypto1Error::InvalidWire("non-zero absent successor_key"));
                }
                None
            }
            1 => Some(CanonicalEd25519PublicKeyV1::from_slice(&bytes[38..70])?),
            _ => return Err(Crypto1Error::InvalidWire("invalid successor flag")),
        };
        let rec = Self {
            record_version: u16::from_be_bytes([bytes[0], bytes[1]]),
            domain_tag: SigningDomainTagV1::from_code(bytes[2])?,
            role: CredentialRoleV1::from_code(bytes[3])?,
            status: KeyStatusV1::from_code(bytes[4])?,
            public_key,
            successor_key,
            created_height: u64::from_be_bytes(bytes[70..78].try_into().expect("8")),
            effective_height: u64::from_be_bytes(bytes[78..86].try_into().expect("8")),
            quarantine_until_height: u64::from_be_bytes(bytes[86..94].try_into().expect("8")),
        };
        rec.validate()?;
        Ok(rec)
    }

    /// Computes the canonical `Receipt` `ContentTag` for this lifecycle record.
    pub fn tag(&self) -> Result<ContentTag, Crypto1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            CRYPTO_1_KEY_LIFECYCLE_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

/// Deterministic operated key lifecycle registry enforcing strict 9-domain key disjointness,
/// rotation/revocation/compromise rules, and bounded batch signature verification.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyLifecycleRegistryV1 {
    records: BTreeMap<CanonicalEd25519PublicKeyV1, KeyLifecycleRecordV1>,
}

impl KeyLifecycleRegistryV1 {
    /// Creates an empty key lifecycle registry.
    pub const fn new() -> Self {
        Self {
            records: BTreeMap::new(),
        }
    }

    /// Returns the lifecycle record for `public_key`, if registered.
    pub fn get(&self, public_key: &CanonicalEd25519PublicKeyV1) -> Option<&KeyLifecycleRecordV1> {
        self.records.get(public_key)
    }

    /// Returns the total number of registered keys across all lifecycle states.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Returns `true` when no keys have been registered.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Registers a fresh `Active` key bound exclusively to `domain_tag`.
    /// Rejects any duplicate key or cross-role/cross-domain reuse fail-closed.
    pub fn register_active_key(
        &mut self,
        public_key: CanonicalEd25519PublicKeyV1,
        domain_tag: SigningDomainTagV1,
        created_height: u64,
    ) -> Result<KeyLifecycleRecordV1, Crypto1Error> {
        if self.records.contains_key(&public_key) {
            return Err(Crypto1Error::CrossRoleOrDuplicateKeyReuse);
        }
        let rec = KeyLifecycleRecordV1 {
            record_version: 1,
            domain_tag,
            role: canonical_role_for_domain(domain_tag),
            status: KeyStatusV1::Active,
            public_key,
            successor_key: None,
            created_height,
            effective_height: created_height,
            quarantine_until_height: 0,
        };
        rec.validate()?;
        self.records.insert(public_key, rec);
        Ok(rec)
    }

    /// Rotates an `Active` key to a fresh `successor_key` in the same domain at `effective_height`.
    pub fn rotate_key(
        &mut self,
        old_key: &CanonicalEd25519PublicKeyV1,
        successor_key: CanonicalEd25519PublicKeyV1,
        effective_height: u64,
    ) -> Result<(KeyLifecycleRecordV1, KeyLifecycleRecordV1), Crypto1Error> {
        if self.records.contains_key(&successor_key) {
            return Err(Crypto1Error::CrossRoleOrDuplicateKeyReuse);
        }
        let existing = *self.records.get(old_key).ok_or(Crypto1Error::UnknownKey)?;
        if existing.status != KeyStatusV1::Active {
            return Err(Crypto1Error::IllegalLifecycleTransition(
                "only Active keys may rotate",
            ));
        }
        if effective_height < existing.created_height {
            return Err(Crypto1Error::IllegalLifecycleTransition(
                "rotation effective_height precedes creation",
            ));
        }
        let rotated = KeyLifecycleRecordV1 {
            status: KeyStatusV1::Rotated,
            successor_key: Some(successor_key),
            effective_height,
            quarantine_until_height: 0,
            ..existing
        };
        rotated.validate()?;
        let new_active = KeyLifecycleRecordV1 {
            record_version: 1,
            domain_tag: existing.domain_tag,
            role: existing.role,
            status: KeyStatusV1::Active,
            public_key: successor_key,
            successor_key: None,
            created_height: effective_height,
            effective_height,
            quarantine_until_height: 0,
        };
        new_active.validate()?;
        self.records.insert(*old_key, rotated);
        self.records.insert(successor_key, new_active);
        Ok((rotated, new_active))
    }

    /// Voluntarily revokes an `Active` or `Rotated` key permanently.
    pub fn revoke_key(
        &mut self,
        key: &CanonicalEd25519PublicKeyV1,
        effective_height: u64,
    ) -> Result<KeyLifecycleRecordV1, Crypto1Error> {
        let existing = *self.records.get(key).ok_or(Crypto1Error::UnknownKey)?;
        if matches!(
            existing.status,
            KeyStatusV1::Revoked | KeyStatusV1::Compromised
        ) {
            return Err(Crypto1Error::IllegalLifecycleTransition(
                "key is already terminally revoked or compromised",
            ));
        }
        let revoked = KeyLifecycleRecordV1 {
            status: KeyStatusV1::Revoked,
            successor_key: None,
            effective_height,
            quarantine_until_height: 0,
            ..existing
        };
        revoked.validate()?;
        self.records.insert(*key, revoked);
        Ok(revoked)
    }

    /// Marks any non-compromised key permanently `Compromised` with a mandatory cooldown window.
    pub fn report_compromise(
        &mut self,
        key: &CanonicalEd25519PublicKeyV1,
        effective_height: u64,
        quarantine_blocks: u64,
    ) -> Result<KeyLifecycleRecordV1, Crypto1Error> {
        if quarantine_blocks == 0 {
            return Err(Crypto1Error::IllegalLifecycleTransition(
                "quarantine_blocks must be positive",
            ));
        }
        let existing = *self.records.get(key).ok_or(Crypto1Error::UnknownKey)?;
        if existing.status == KeyStatusV1::Compromised {
            return Err(Crypto1Error::IllegalLifecycleTransition(
                "key is already quarantined as Compromised",
            ));
        }
        let quarantine_until = effective_height.checked_add(quarantine_blocks).ok_or(
            Crypto1Error::IllegalLifecycleTransition("quarantine height overflow"),
        )?;
        let comp = KeyLifecycleRecordV1 {
            status: KeyStatusV1::Compromised,
            successor_key: None,
            effective_height,
            quarantine_until_height: quarantine_until,
            ..existing
        };
        comp.validate()?;
        self.records.insert(*key, comp);
        Ok(comp)
    }

    /// Verifies that `public_key` is authoritative at `current_height` for `preimage.domain_tag`
    /// and that `signature` passes strict RFC 8032 verification.
    pub fn verify_authorized_signature(
        &self,
        public_key: &CanonicalEd25519PublicKeyV1,
        preimage: &SigningPreimageV1,
        signature: &CanonicalEd25519SignatureV1,
        current_height: u64,
    ) -> Result<(), Crypto1Error> {
        let rec = self
            .records
            .get(public_key)
            .ok_or(Crypto1Error::UnknownKey)?;
        if preimage.domain_tag != rec.domain_tag {
            return Err(Crypto1Error::CrossDomainSignatureRejected {
                bound: rec.domain_tag,
                attempted: preimage.domain_tag,
            });
        }
        match rec.status {
            KeyStatusV1::Active => {
                if current_height < rec.created_height {
                    return Err(Crypto1Error::KeyNotAuthoritative(KeyStatusV1::Active));
                }
            }
            KeyStatusV1::Rotated => {
                if current_height >= rec.effective_height || current_height < rec.created_height {
                    return Err(Crypto1Error::KeyNotAuthoritative(KeyStatusV1::Rotated));
                }
            }
            KeyStatusV1::Revoked | KeyStatusV1::Compromised => {
                return Err(Crypto1Error::KeyNotAuthoritative(rec.status));
            }
        }
        verify_preimage_strict(public_key, preimage, signature)?;
        Ok(())
    }

    /// Verifies a bounded batch of `(public_key, preimage, signature)` tuples against lifecycle,
    /// domain separation, and strict RFC 8032 Ed25519 checks, returning the exact
    /// `first_invalid_index` on any fault.
    pub fn verify_batch_authorized(
        &self,
        items: &[(
            CanonicalEd25519PublicKeyV1,
            SigningPreimageV1,
            CanonicalEd25519SignatureV1,
        )],
        current_height: u64,
        max_batch_sigs: usize,
    ) -> Result<(), Crypto1Error> {
        let cap = max_batch_sigs.min(MAX_CRYPTO_1_BATCH_SIGNATURES);
        if items.len() > cap {
            return Err(Crypto1Error::BatchOverLimit {
                observed: items.len(),
                limit: cap,
            });
        }
        for (idx, (pk, pre, sig)) in items.iter().enumerate() {
            if let Err(err) = self.verify_authorized_signature(pk, pre, sig, current_height) {
                let reason = match err {
                    Crypto1Error::UnknownKey => "unknown_key",
                    Crypto1Error::CrossDomainSignatureRejected { .. } => "cross_domain",
                    Crypto1Error::KeyNotAuthoritative(_) => "key_not_authoritative",
                    Crypto1Error::Crypto(_) => "ed25519_verify_failed",
                    _ => "lifecycle_rejected",
                };
                return Err(Crypto1Error::BatchItemFailed {
                    first_invalid_index: idx,
                    reason,
                });
            }
        }
        Ok(())
    }
}

/// Pure RFC 2104 `HMAC-SHA256` using the workspace's already-pinned `sha2` crate.
pub fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut block = [0u8; 64];
    if key.len() > 64 {
        let digest = Sha256::digest(key);
        block[..32].copy_from_slice(&digest);
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5cu8; 64];
    for i in 0..64 {
        ipad[i] ^= block[i];
        opad[i] ^= block[i];
    }
    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(data);
    let inner_hash = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_hash);
    outer.finalize().into()
}

/// Pure RFC 2898 `PBKDF2-HMAC-SHA256` producing a 32-byte master key (1 block).
pub fn pbkdf2_hmac_sha256_32(passphrase: &[u8], salt: &[u8], iterations: u32) -> [u8; 32] {
    let mut init_msg = Vec::with_capacity(salt.len() + 4);
    init_msg.extend_from_slice(salt);
    init_msg.extend_from_slice(&1u32.to_be_bytes());
    let mut u = hmac_sha256(passphrase, &init_msg);
    let mut out = u;
    for _ in 1..iterations {
        u = hmac_sha256(passphrase, &u);
        for (acc, byte) in out.iter_mut().zip(u.iter()) {
            *acc ^= *byte;
        }
    }
    out
}

/// Constant-time 32-byte equality comparison.
fn ct_eq_32(a: &[u8; 32], b: &[u8; 32]) -> bool {
    let mut diff = 0u8;
    for (&x, &y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Authenticated key-custody backup/restore envelope (`bitblock/crypto-1/custody-backup-envelope/v1`).
///
/// Uses standard `PBKDF2-HMAC-SHA256` -> `HKDF-SHA256` domain-separated split into independent
/// `(enc_key, mac_key)` -> `SHA-256-CTR` XOR over the 32-byte Ed25519 seed -> `HMAC-SHA256`
/// Encrypt-then-MAC over the full header, public key, salt, nonce, and ciphertext.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CustodyBackupEnvelopeV1 {
    /// Envelope schema version (`1`).
    pub envelope_version: u16,
    /// Bound signing domain tag.
    pub domain_tag: SigningDomainTagV1,
    /// Bound credential role matching [`canonical_role_for_domain`].
    pub role: CredentialRoleV1,
    /// `PBKDF2-HMAC-SHA256` iteration count (`1..=MAX_CUSTODY_KDF_ITERATIONS`).
    pub kdf_iterations: u32,
    /// Bound canonical Ed25519 public key corresponding to the sealed 32-byte seed.
    pub public_key: CanonicalEd25519PublicKeyV1,
    /// 16-byte KDF salt.
    pub salt: [u8; 16],
    /// 16-byte encryption nonce.
    pub nonce: [u8; 16],
    /// 32-byte `SHA-256-CTR` encrypted Ed25519 secret seed.
    pub ciphertext: [u8; 32],
    /// 32-byte Encrypt-then-MAC `HMAC-SHA256` authentication tag.
    pub mac_tag: [u8; 32],
}

impl CustodyBackupEnvelopeV1 {
    /// Fixed canonical big-endian wire length (`136` bytes).
    pub const WIRE_LEN: usize = 2 + 1 + 1 + 4 + 32 + 16 + 16 + 32 + 32;

    fn derive_subkeys(
        passphrase: &[u8],
        salt: &[u8; 16],
        kdf_iterations: u32,
        domain_tag: SigningDomainTagV1,
        role: CredentialRoleV1,
    ) -> ([u8; 32], [u8; 32]) {
        let ikm = pbkdf2_hmac_sha256_32(passphrase, salt, kdf_iterations);
        let prk = hmac_sha256(b"bitblock/crypto-1/custody-hkdf-extract/v1", &ikm);
        let mut enc_info = Vec::with_capacity(48);
        enc_info.extend_from_slice(b"bitblock/crypto-1/custody-enc/v1");
        enc_info.push(domain_tag.code());
        enc_info.push(role.code());
        enc_info.push(1);
        let enc_key = hmac_sha256(&prk, &enc_info);

        let mut mac_info = Vec::with_capacity(48);
        mac_info.extend_from_slice(b"bitblock/crypto-1/custody-mac/v1");
        mac_info.push(domain_tag.code());
        mac_info.push(role.code());
        mac_info.push(1);
        let mac_key = hmac_sha256(&prk, &mac_info);
        (enc_key, mac_key)
    }

    fn keystream_32(enc_key: &[u8; 32], nonce: &[u8; 16]) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(b"bitblock/crypto-1/custody-ctr/v1");
        h.update(enc_key);
        h.update(nonce);
        h.update(0u32.to_be_bytes());
        h.finalize().into()
    }

    fn mac_input(&self) -> [u8; 104] {
        let mut buf = [0u8; 104];
        buf[0..2].copy_from_slice(&self.envelope_version.to_be_bytes());
        buf[2] = self.domain_tag.code();
        buf[3] = self.role.code();
        buf[4..8].copy_from_slice(&self.kdf_iterations.to_be_bytes());
        buf[8..40].copy_from_slice(self.public_key.as_bytes());
        buf[40..56].copy_from_slice(&self.salt);
        buf[56..72].copy_from_slice(&self.nonce);
        buf[72..104].copy_from_slice(&self.ciphertext);
        buf
    }

    /// Validates envelope header invariants.
    pub fn validate_header(&self) -> Result<(), Crypto1Error> {
        if self.envelope_version != 1 {
            return Err(Crypto1Error::InvalidWire("envelope_version must equal 1"));
        }
        if canonical_role_for_domain(self.domain_tag) != self.role {
            return Err(Crypto1Error::RoleDomainMismatch {
                role: self.role,
                domain: self.domain_tag,
            });
        }
        if self.kdf_iterations == 0 || self.kdf_iterations > MAX_CUSTODY_KDF_ITERATIONS {
            return Err(Crypto1Error::InvalidCustodyParameters(
                "kdf_iterations out of bounds",
            ));
        }
        Ok(())
    }

    /// Seals a 32-byte Ed25519 secret seed into an authenticated [`CustodyBackupEnvelopeV1`].
    pub fn seal(
        secret_seed: &[u8; 32],
        domain_tag: SigningDomainTagV1,
        passphrase: &[u8],
        salt: [u8; 16],
        nonce: [u8; 16],
        kdf_iterations: u32,
    ) -> Result<Self, Crypto1Error> {
        if passphrase.is_empty() {
            return Err(Crypto1Error::InvalidCustodyParameters(
                "passphrase must be non-empty",
            ));
        }
        let role = canonical_role_for_domain(domain_tag);
        let sk = SigningKey::from_bytes(secret_seed);
        let public_key = CanonicalEd25519PublicKeyV1::from_bytes(sk.verifying_key().to_bytes())?;
        let mut env = Self {
            envelope_version: 1,
            domain_tag,
            role,
            kdf_iterations,
            public_key,
            salt,
            nonce,
            ciphertext: [0u8; 32],
            mac_tag: [0u8; 32],
        };
        env.validate_header()?;
        let (enc_key, mac_key) =
            Self::derive_subkeys(passphrase, &salt, kdf_iterations, domain_tag, role);
        let ks = Self::keystream_32(&enc_key, &nonce);
        for i in 0..32 {
            env.ciphertext[i] = secret_seed[i] ^ ks[i];
        }
        env.mac_tag = hmac_sha256(&mac_key, &env.mac_input());
        Ok(env)
    }

    /// Verifies MAC in constant time BEFORE decrypting, restores the 32-byte Ed25519 seed, and
    /// verifies that the restored seed derives the exact bound `self.public_key`.
    pub fn restore_seed(&self, passphrase: &[u8]) -> Result<[u8; 32], Crypto1Error> {
        if passphrase.is_empty() {
            return Err(Crypto1Error::InvalidCustodyParameters(
                "passphrase must be non-empty",
            ));
        }
        self.validate_header()?;
        let (enc_key, mac_key) = Self::derive_subkeys(
            passphrase,
            &self.salt,
            self.kdf_iterations,
            self.domain_tag,
            self.role,
        );
        let expected_mac = hmac_sha256(&mac_key, &self.mac_input());
        if !ct_eq_32(&expected_mac, &self.mac_tag) {
            return Err(Crypto1Error::CustodyMacMismatch);
        }
        let ks = Self::keystream_32(&enc_key, &self.nonce);
        let mut seed = [0u8; 32];
        for i in 0..32 {
            seed[i] = self.ciphertext[i] ^ ks[i];
        }
        let derived_pk = CanonicalEd25519PublicKeyV1::from_bytes(
            SigningKey::from_bytes(&seed).verifying_key().to_bytes(),
        )?;
        if derived_pk != self.public_key {
            return Err(Crypto1Error::CustodyKeyBindingMismatch);
        }
        Ok(seed)
    }

    /// Encodes the envelope into fixed 136-byte canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], Crypto1Error> {
        self.validate_header()?;
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..104].copy_from_slice(&self.mac_input());
        out[104..136].copy_from_slice(&self.mac_tag);
        Ok(out)
    }

    /// Decodes a 136-byte canonical [`CustodyBackupEnvelopeV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Crypto1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(Crypto1Error::InvalidWire(
                "CustodyBackupEnvelopeV1 wrong len",
            ));
        }
        let env = Self {
            envelope_version: u16::from_be_bytes([bytes[0], bytes[1]]),
            domain_tag: SigningDomainTagV1::from_code(bytes[2])?,
            role: CredentialRoleV1::from_code(bytes[3])?,
            kdf_iterations: u32::from_be_bytes(bytes[4..8].try_into().expect("4")),
            public_key: CanonicalEd25519PublicKeyV1::from_slice(&bytes[8..40])?,
            salt: bytes[40..56].try_into().expect("16"),
            nonce: bytes[56..72].try_into().expect("16"),
            ciphertext: bytes[72..104].try_into().expect("32"),
            mac_tag: bytes[104..136].try_into().expect("32"),
        };
        env.validate_header()?;
        Ok(env)
    }

    /// Computes the canonical `Receipt` `ContentTag` for this custody backup envelope.
    pub fn tag(&self) -> Result<ContentTag, Crypto1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            CRYPTO_1_CUSTODY_ENVELOPE_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

/// Explicit anti-downgrade old/new-profile suite migration policy for `UPGRADE-1` (`CRYPTO-1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SuiteMigrationPolicyV1 {
    /// Policy schema version (`1`).
    pub policy_version: u16,
    /// Old profile signature suite ID (`>= 1`).
    pub old_sig_suite: u16,
    /// Old profile hash suite ID (`>= 1`).
    pub old_hash_suite: u16,
    /// Target profile signature suite ID (MUST be `>= old_sig_suite`).
    pub new_sig_suite: u16,
    /// Target profile hash suite ID (MUST be `>= old_hash_suite`).
    pub new_hash_suite: u16,
    /// Height when the new suite activates.
    pub activation_height: u64,
    /// Height when the old suite sunsets (`>= activation_height + min_overlap_blocks`).
    pub sunset_height: u64,
    /// Requires dual signatures across old and new suites during the overlap window (`true`).
    pub dual_sign_required: bool,
    /// Anti-downgrade invariant: MUST be `false` (no post-sunset legacy fallback).
    pub allow_legacy_fallback_after_sunset: bool,
    /// Minimum mandatory dual-sign overlap duration in blocks (`> 0`).
    pub min_overlap_blocks: u64,
}

impl SuiteMigrationPolicyV1 {
    /// Fixed canonical big-endian wire length (`36` bytes).
    pub const WIRE_LEN: usize = 2 * 5 + 8 * 3 + 2;

    /// Returns the default v1 forward-migration readiness policy (`Ed25519(1)+Sha256(1)` -> future
    /// suite `2` with mandatory dual-sign overlap and zero post-sunset downgrade).
    pub const fn v1_pq_upgrade_ready(activation_height: u64, sunset_height: u64) -> Self {
        Self {
            policy_version: 1,
            old_sig_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1.code(),
            old_hash_suite: HashSuiteIdV1::Sha256V1.code(),
            new_sig_suite: 2,
            new_hash_suite: 1,
            activation_height,
            sunset_height,
            dual_sign_required: true,
            allow_legacy_fallback_after_sunset: false,
            min_overlap_blocks: 1_024,
        }
    }

    /// Validates monotonicity (no suite downgrade), overlap window, and anti-fallback rules.
    pub fn validate(&self) -> Result<(), Crypto1Error> {
        if self.policy_version != 1 {
            return Err(Crypto1Error::InvalidWire("policy_version must equal 1"));
        }
        if self.old_sig_suite == 0 || self.old_hash_suite == 0 {
            return Err(Crypto1Error::SuiteDowngradeOrInvalidMigration(
                "suite IDs must be non-zero",
            ));
        }
        if self.new_sig_suite < self.old_sig_suite || self.new_hash_suite < self.old_hash_suite {
            return Err(Crypto1Error::SuiteDowngradeOrInvalidMigration(
                "suite ID downgrade is strictly forbidden",
            ));
        }
        if self.new_sig_suite == self.old_sig_suite && self.new_hash_suite == self.old_hash_suite {
            return Err(Crypto1Error::SuiteDowngradeOrInvalidMigration(
                "migration must advance at least one suite ID",
            ));
        }
        if self.min_overlap_blocks == 0
            || self.sunset_height.saturating_sub(self.activation_height) < self.min_overlap_blocks
            || self.sunset_height <= self.activation_height
        {
            return Err(Crypto1Error::SuiteDowngradeOrInvalidMigration(
                "sunset_height - activation_height must be >= min_overlap_blocks",
            ));
        }
        if !self.dual_sign_required {
            return Err(Crypto1Error::SuiteDowngradeOrInvalidMigration(
                "dual_sign_required must be true during overlap",
            ));
        }
        if self.allow_legacy_fallback_after_sunset {
            return Err(Crypto1Error::SuiteDowngradeOrInvalidMigration(
                "post-sunset legacy fallback is forbidden (anti-downgrade)",
            ));
        }
        Ok(())
    }

    /// Evaluates whether a signature carrying `(sig_suite, hash_suite)` is permitted at `height`.
    pub fn check_suite_at_height(
        &self,
        sig_suite: u16,
        hash_suite: u16,
        height: u64,
    ) -> Result<(), Crypto1Error> {
        self.validate()?;
        let is_old = sig_suite == self.old_sig_suite && hash_suite == self.old_hash_suite;
        let is_new = sig_suite == self.new_sig_suite && hash_suite == self.new_hash_suite;
        if height < self.activation_height {
            if is_old {
                Ok(())
            } else {
                Err(Crypto1Error::SuiteDowngradeOrInvalidMigration(
                    "new suite not yet active",
                ))
            }
        } else if height < self.sunset_height {
            if is_old || is_new {
                Ok(())
            } else {
                Err(Crypto1Error::SuiteDowngradeOrInvalidMigration(
                    "unknown suite during overlap window",
                ))
            }
        } else if is_new {
            Ok(())
        } else {
            Err(Crypto1Error::SuiteDowngradeOrInvalidMigration(
                "old or downgraded suite rejected after sunset",
            ))
        }
    }

    /// Encodes the migration policy into fixed 36-byte canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], Crypto1Error> {
        self.validate()?;
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.policy_version.to_be_bytes());
        out[2..4].copy_from_slice(&self.old_sig_suite.to_be_bytes());
        out[4..6].copy_from_slice(&self.old_hash_suite.to_be_bytes());
        out[6..8].copy_from_slice(&self.new_sig_suite.to_be_bytes());
        out[8..10].copy_from_slice(&self.new_hash_suite.to_be_bytes());
        out[10..18].copy_from_slice(&self.activation_height.to_be_bytes());
        out[18..26].copy_from_slice(&self.sunset_height.to_be_bytes());
        out[26] = u8::from(self.dual_sign_required);
        out[27] = u8::from(self.allow_legacy_fallback_after_sunset);
        out[28..36].copy_from_slice(&self.min_overlap_blocks.to_be_bytes());
        Ok(out)
    }

    /// Decodes and validates a 36-byte canonical [`SuiteMigrationPolicyV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Crypto1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(Crypto1Error::InvalidWire(
                "SuiteMigrationPolicyV1 wrong len",
            ));
        }
        let dual = match bytes[26] {
            0 => false,
            1 => true,
            _ => return Err(Crypto1Error::InvalidWire("invalid bool byte")),
        };
        let fallback = match bytes[27] {
            0 => false,
            1 => true,
            _ => return Err(Crypto1Error::InvalidWire("invalid bool byte")),
        };
        let policy = Self {
            policy_version: u16::from_be_bytes([bytes[0], bytes[1]]),
            old_sig_suite: u16::from_be_bytes([bytes[2], bytes[3]]),
            old_hash_suite: u16::from_be_bytes([bytes[4], bytes[5]]),
            new_sig_suite: u16::from_be_bytes([bytes[6], bytes[7]]),
            new_hash_suite: u16::from_be_bytes([bytes[8], bytes[9]]),
            activation_height: u64::from_be_bytes(bytes[10..18].try_into().expect("8")),
            sunset_height: u64::from_be_bytes(bytes[18..26].try_into().expect("8")),
            dual_sign_required: dual,
            allow_legacy_fallback_after_sunset: fallback,
            min_overlap_blocks: u64::from_be_bytes(bytes[28..36].try_into().expect("8")),
        };
        policy.validate()?;
        Ok(policy)
    }

    /// Computes the canonical `Policy` `ContentTag` for this migration policy.
    pub fn tag(&self) -> Result<ContentTag, Crypto1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            CRYPTO_1_MIGRATION_POLICY_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

/// Derives the canonical Ed25519 public key from a 32-byte secret seed (`CRYPTO-1` helper).
pub fn public_key_from_seed(
    secret_seed: &[u8; 32],
) -> Result<CanonicalEd25519PublicKeyV1, Crypto1Error> {
    let bytes: [u8; ED25519_PUBLIC_KEY_LEN] = SigningKey::from_bytes(secret_seed)
        .verifying_key()
        .to_bytes();
    Ok(CanonicalEd25519PublicKeyV1::from_bytes(bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto_d1::sign_preimage_deterministic;

    #[test]
    fn crypto_1_lifecycle_custody_and_migration_roundtrip() {
        let seed_a = [0x11u8; 32];
        let seed_b = [0x22u8; 32];
        let pk_a = public_key_from_seed(&seed_a).unwrap();
        let pk_b = public_key_from_seed(&seed_b).unwrap();

        let mut reg = KeyLifecycleRegistryV1::new();
        let rec_a = reg
            .register_active_key(pk_a, SigningDomainTagV1::ActorTransaction, 10)
            .unwrap();
        assert_eq!(
            KeyLifecycleRecordV1::decode_canonical(&rec_a.encode_canonical().unwrap()).unwrap(),
            rec_a
        );
        // Cross-role key reuse must fail closed.
        assert_eq!(
            reg.register_active_key(pk_a, SigningDomainTagV1::ConsensusVote, 10),
            Err(Crypto1Error::CrossRoleOrDuplicateKeyReuse)
        );

        let (rot_a, act_b) = reg.rotate_key(&pk_a, pk_b, 50).unwrap();
        assert_eq!(rot_a.status, KeyStatusV1::Rotated);
        assert_eq!(act_b.status, KeyStatusV1::Active);

        let pre = SigningPreimageV1 {
            protocol_version: 1,
            chain_id: 7,
            genesis_id: [0x33; 32],
            launch_manifest_id: [0x44; 32],
            profile_id: [0x55; 32],
            domain_tag: SigningDomainTagV1::ActorTransaction,
            signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
            hash_suite: HashSuiteIdV1::Sha256V1,
            payload_digest: [0x66; 32],
        };
        let sig_a = sign_preimage_deterministic(&seed_a, &pre).unwrap();
        assert!(
            reg.verify_authorized_signature(&pk_a, &pre, &sig_a, 49)
                .is_ok()
        );
        assert_eq!(
            reg.verify_authorized_signature(&pk_a, &pre, &sig_a, 50),
            Err(Crypto1Error::KeyNotAuthoritative(KeyStatusV1::Rotated))
        );

        let env = CustodyBackupEnvelopeV1::seal(
            &seed_a,
            SigningDomainTagV1::ActorTransaction,
            b"bitblock-devnet-passphrase-v1",
            [0xA1; 16],
            [0xB2; 16],
            DEFAULT_CUSTODY_KDF_ITERATIONS,
        )
        .unwrap();
        let raw_env = env.encode_canonical().unwrap();
        assert_eq!(
            CustodyBackupEnvelopeV1::decode_canonical(&raw_env).unwrap(),
            env
        );
        assert_eq!(
            env.restore_seed(b"bitblock-devnet-passphrase-v1").unwrap(),
            seed_a
        );
        assert_eq!(
            env.restore_seed(b"wrong-passphrase"),
            Err(Crypto1Error::CustodyMacMismatch)
        );

        let mig = SuiteMigrationPolicyV1::v1_pq_upgrade_ready(1_000, 3_048);
        assert_eq!(
            SuiteMigrationPolicyV1::decode_canonical(&mig.encode_canonical().unwrap()).unwrap(),
            mig
        );
        assert!(mig.check_suite_at_height(1, 1, 999).is_ok());
        assert!(mig.check_suite_at_height(1, 1, 3_048).is_err());
        assert!(mig.check_suite_at_height(2, 1, 3_048).is_ok());
    }
}
