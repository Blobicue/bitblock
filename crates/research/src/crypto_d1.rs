//! `CRYPTO-D1`: canonical Ed25519 (strict RFC 8032) + SHA-256 suite selection, key/signature
//! validation, role-separated signing preimages, and batch verification with safe fallback.
//!
//! This module freezes the pre-genesis cryptographic primitive contract selected under
//! `CRYPTO-D1`:
//! - **Signature suite (`Ed25519StrictRfc8032V1`):** strict RFC 8032 Ed25519 rejecting all 8
//!   small-order torsion public keys/points, non-canonical compressed coordinates (`y >= p` or
//!   non-canonical sign bit), and malleated scalars (`S >= L`).
//! - **Hash suite (`Sha256V1`):** SHA-256 across protocol identifiers and signing digests,
//!   retaining explicit versioned suite IDs for future PQ or `HASH-R1` migration.
//! - **Role separation:** distinct [`SigningDomainTagV1`] discriminants prevent cross-role and
//!   cross-protocol replay across actor actions, block proposals, consensus votes, transport,
//!   withdrawals, governance, emergency pauses, and releases.
//!
//! Operational key custody, entropy generation, rotation schedules, and HSM boundaries belong
//! to `CRYPTO-1` and `SIGNER-OPS-1`, not this primitive-selection module.

use std::fmt;

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};

use crate::protocol::{ContentTag, TagKind, hash_canonical};

/// Canonical schema string for a [`CryptoSuiteDescriptorV1`] policy object.
pub const CRYPTO_D1_SUITE_DESCRIPTOR_SCHEMA: &[u8] = b"bitblock/crypto-d1/suite-descriptor/v1";
/// Canonical schema string for a [`SigningPreimageV1`] domain-separated signing payload.
pub const CRYPTO_D1_SIGNING_PREIMAGE_SCHEMA: &[u8] = b"bitblock/crypto-d1/signing-preimage/v1";

/// Fixed byte length of a canonical Ed25519 public key (`32` bytes).
pub const ED25519_PUBLIC_KEY_LEN: usize = 32;
/// Fixed byte length of a canonical Ed25519 signature (`64` bytes).
pub const ED25519_SIGNATURE_LEN: usize = 64;

/// Ed25519 subgroup order `L = 2^252 + 27742317777372353535851937790883648493` in little-endian.
const ED25519_ORDER_L_LE: [u8; 32] = [
    0xed, 0xd3, 0xf5, 0x5c, 0x1a, 0x63, 0x12, 0x58, 0xd6, 0x9c, 0xf7, 0xa2, 0xde, 0xf9, 0xde, 0x14,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10,
];

/// Ed25519 field prime `p = 2^255 - 19` in little-endian.
const ED25519_FIELD_P_LE: [u8; 32] = [
    0xed, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f,
];

/// Errors returned by `CRYPTO-D1` canonical validation, decoding, or strict verification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CryptoD1Error {
    /// Wire payload length was invalid or had trailing bytes.
    InvalidLength { expected: usize, actual: usize },
    /// Unknown or unsupported signature suite identifier.
    UnsupportedSignatureSuite(u16),
    /// Unknown or unsupported hash suite identifier.
    UnsupportedHashSuite(u16),
    /// Unknown signing domain tag discriminant.
    UnknownSigningDomain(u8),
    /// Public key or signature `R` bytes were not a canonical curve point encoding.
    NonCanonicalPointEncoding,
    /// Public key or signature `R` point belongs to the small-order torsion subgroup.
    WeakOrSmallOrderPoint,
    /// Signature scalar `S` was not canonically reduced (`S >= L`).
    NonCanonicalSignatureScalar,
    /// Cryptographic signature verification failed against the preimage and public key.
    SignatureVerificationFailed,
    /// Batch verification failed at the reported item index after strict individual fallback.
    BatchVerificationFailed { first_invalid_index: usize },
    /// Descriptor invariant was violated (for example disabling strict canonical checks).
    InvalidSuiteDescriptor(&'static str),
}

impl fmt::Display for CryptoD1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength { expected, actual } => {
                write!(f, "invalid length: expected {expected} bytes, got {actual}")
            }
            Self::UnsupportedSignatureSuite(id) => {
                write!(f, "unsupported signature suite id {id}")
            }
            Self::UnsupportedHashSuite(id) => write!(f, "unsupported hash suite id {id}"),
            Self::UnknownSigningDomain(code) => write!(f, "unknown signing domain code {code}"),
            Self::NonCanonicalPointEncoding => write!(f, "non-canonical Ed25519 point encoding"),
            Self::WeakOrSmallOrderPoint => write!(f, "weak or small-order Ed25519 torsion point"),
            Self::NonCanonicalSignatureScalar => {
                write!(f, "non-canonical Ed25519 signature scalar S >= L")
            }
            Self::SignatureVerificationFailed => write!(f, "strict Ed25519 verification failed"),
            Self::BatchVerificationFailed {
                first_invalid_index,
            } => write!(
                f,
                "strict batch verification failed at index {first_invalid_index}"
            ),
            Self::InvalidSuiteDescriptor(reason) => {
                write!(f, "invalid crypto suite descriptor: {reason}")
            }
        }
    }
}

impl std::error::Error for CryptoD1Error {}

/// Versioned signature suite identifier for `CRYPTO-D1` and future `UPGRADE-1` migrations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum SignatureSuiteIdV1 {
    /// Strict RFC 8032 Ed25519 rejecting weak torsion points and non-canonical `R`/`S`/`A`.
    Ed25519StrictRfc8032V1 = 1,
}

impl SignatureSuiteIdV1 {
    /// Returns the canonical `u16` wire identifier.
    pub const fn code(self) -> u16 {
        self as u16
    }

    /// Decodes a canonical `u16` signature suite identifier.
    pub const fn from_code(code: u16) -> Result<Self, CryptoD1Error> {
        match code {
            1 => Ok(Self::Ed25519StrictRfc8032V1),
            other => Err(CryptoD1Error::UnsupportedSignatureSuite(other)),
        }
    }
}

/// Versioned hash suite identifier for `CRYPTO-D1` and future `HASH-R1` / `UPGRADE-1` agility.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum HashSuiteIdV1 {
    /// Standard SHA-256 matching `PROOF-0` and `COMMIT-1`.
    Sha256V1 = 1,
}

impl HashSuiteIdV1 {
    /// Returns the canonical `u16` wire identifier.
    pub const fn code(self) -> u16 {
        self as u16
    }

    /// Decodes a canonical `u16` hash suite identifier.
    pub const fn from_code(code: u16) -> Result<Self, CryptoD1Error> {
        match code {
            1 => Ok(Self::Sha256V1),
            other => Err(CryptoD1Error::UnsupportedHashSuite(other)),
        }
    }
}

/// Strictly separated cryptographic signing domains preventing cross-role signature replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum SigningDomainTagV1 {
    /// Actor-signed world/asset/identity transaction payload.
    ActorTransaction = 1,
    /// Actor-admin authorization rotation or revocation transition.
    ActorAdminRotation = 2,
    /// Validator-signed block proposal over `AcceptedProposalCoreV1`.
    BlockProposal = 3,
    /// Validator-signed consensus vote over `VoteSignDocV1`.
    ConsensusVote = 4,
    /// Validator peer-to-peer authenticated transport handshake.
    ValidatorTransport = 5,
    /// Validator reward/unbonding withdrawal authorization.
    RewardWithdrawal = 6,
    /// Timelocked governance proposal or approval signature.
    GovernanceAction = 7,
    /// Pause-only emergency security council signature.
    EmergencyPause = 8,
    /// Reproducible release manifest attestation signature.
    ReleaseArtifact = 9,
}

impl SigningDomainTagV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte signing domain discriminant.
    pub const fn from_code(code: u8) -> Result<Self, CryptoD1Error> {
        match code {
            1 => Ok(Self::ActorTransaction),
            2 => Ok(Self::ActorAdminRotation),
            3 => Ok(Self::BlockProposal),
            4 => Ok(Self::ConsensusVote),
            5 => Ok(Self::ValidatorTransport),
            6 => Ok(Self::RewardWithdrawal),
            7 => Ok(Self::GovernanceAction),
            8 => Ok(Self::EmergencyPause),
            9 => Ok(Self::ReleaseArtifact),
            other => Err(CryptoD1Error::UnknownSigningDomain(other)),
        }
    }
}

/// Canonical pre-genesis cryptographic suite descriptor frozen by `CRYPTO-D1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CryptoSuiteDescriptorV1 {
    /// Descriptor schema version (`1`).
    pub schema_version: u16,
    /// Selected actor and validator signature suite.
    pub signature_suite: SignatureSuiteIdV1,
    /// Selected signing and protocol-identifier hash suite.
    pub hash_suite: HashSuiteIdV1,
    /// Canonical public-key byte length (`32`).
    pub public_key_bytes: u16,
    /// Canonical signature byte length (`64`).
    pub signature_bytes: u16,
    /// Requires strict rejection of all 8 small-order torsion points (`true`).
    pub reject_small_order_points: bool,
    /// Requires strict canonical compressed point and reduced scalar encodings (`true`).
    pub reject_noncanonical_encodings: bool,
    /// Enables explicit suite-ID migration under `UPGRADE-1` (`true`).
    pub suite_agility_enabled: bool,
}

impl CryptoSuiteDescriptorV1 {
    /// Fixed canonical wire byte length (`13` bytes).
    pub const WIRE_LEN: usize = 13;

    /// Returns the frozen v1 `CRYPTO-D1` suite descriptor (`Ed25519StrictRfc8032V1` + `Sha256V1`).
    pub const fn v1_ed25519_sha256() -> Self {
        Self {
            schema_version: 1,
            signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
            hash_suite: HashSuiteIdV1::Sha256V1,
            public_key_bytes: ED25519_PUBLIC_KEY_LEN as u16,
            signature_bytes: ED25519_SIGNATURE_LEN as u16,
            reject_small_order_points: true,
            reject_noncanonical_encodings: true,
            suite_agility_enabled: true,
        }
    }

    /// Validates that all strict canonical and key-size invariants hold.
    pub fn validate(&self) -> Result<(), CryptoD1Error> {
        if self.schema_version != 1 {
            return Err(CryptoD1Error::InvalidSuiteDescriptor(
                "schema_version must equal 1",
            ));
        }
        if self.public_key_bytes as usize != ED25519_PUBLIC_KEY_LEN {
            return Err(CryptoD1Error::InvalidSuiteDescriptor(
                "Ed25519 public_key_bytes must equal 32",
            ));
        }
        if self.signature_bytes as usize != ED25519_SIGNATURE_LEN {
            return Err(CryptoD1Error::InvalidSuiteDescriptor(
                "Ed25519 signature_bytes must equal 64",
            ));
        }
        if !self.reject_small_order_points {
            return Err(CryptoD1Error::InvalidSuiteDescriptor(
                "small-order point rejection is mandatory",
            ));
        }
        if !self.reject_noncanonical_encodings {
            return Err(CryptoD1Error::InvalidSuiteDescriptor(
                "non-canonical encoding rejection is mandatory",
            ));
        }
        if !self.suite_agility_enabled {
            return Err(CryptoD1Error::InvalidSuiteDescriptor(
                "suite_agility_enabled must remain true for UPGRADE-1",
            ));
        }
        Ok(())
    }

    /// Encodes the descriptor into exact canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], CryptoD1Error> {
        self.validate()?;
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.schema_version.to_be_bytes());
        out[2..4].copy_from_slice(&self.signature_suite.code().to_be_bytes());
        out[4..6].copy_from_slice(&self.hash_suite.code().to_be_bytes());
        out[6..8].copy_from_slice(&self.public_key_bytes.to_be_bytes());
        out[8..10].copy_from_slice(&self.signature_bytes.to_be_bytes());
        out[10] = u8::from(self.reject_small_order_points);
        out[11] = u8::from(self.reject_noncanonical_encodings);
        out[12] = u8::from(self.suite_agility_enabled);
        Ok(out)
    }

    /// Decodes and validates a canonical `CryptoSuiteDescriptorV1`.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, CryptoD1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(CryptoD1Error::InvalidLength {
                expected: Self::WIRE_LEN,
                actual: bytes.len(),
            });
        }
        let decode_bool = |b: u8| -> Result<bool, CryptoD1Error> {
            match b {
                0 => Ok(false),
                1 => Ok(true),
                _ => Err(CryptoD1Error::InvalidSuiteDescriptor(
                    "boolean byte must be 0 or 1",
                )),
            }
        };
        let desc = Self {
            schema_version: u16::from_be_bytes([bytes[0], bytes[1]]),
            signature_suite: SignatureSuiteIdV1::from_code(u16::from_be_bytes([
                bytes[2], bytes[3],
            ]))?,
            hash_suite: HashSuiteIdV1::from_code(u16::from_be_bytes([bytes[4], bytes[5]]))?,
            public_key_bytes: u16::from_be_bytes([bytes[6], bytes[7]]),
            signature_bytes: u16::from_be_bytes([bytes[8], bytes[9]]),
            reject_small_order_points: decode_bool(bytes[10])?,
            reject_noncanonical_encodings: decode_bool(bytes[11])?,
            suite_agility_enabled: decode_bool(bytes[12])?,
        };
        desc.validate()?;
        Ok(desc)
    }

    /// Computes the canonical `ContentTag` for this suite descriptor.
    pub fn tag(&self) -> Result<ContentTag, CryptoD1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            CRYPTO_D1_SUITE_DESCRIPTOR_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

/// Returns `true` iff 32-byte little-endian unsigned integer `a` is strictly less than `b`.
fn le_u256_lt(a: &[u8; 32], b: &[u8; 32]) -> bool {
    for i in (0..32).rev() {
        if a[i] < b[i] {
            return true;
        }
        if a[i] > b[i] {
            return false;
        }
    }
    false
}

/// Validates that `bytes` is a canonical (`y < p`), non-weak compressed Edwards25519 point.
fn validate_canonical_non_weak_point(bytes: &[u8; 32]) -> Result<VerifyingKey, CryptoD1Error> {
    let mut y_le = *bytes;
    y_le[31] &= 0x7f;
    if !le_u256_lt(&y_le, &ED25519_FIELD_P_LE) {
        return Err(CryptoD1Error::NonCanonicalPointEncoding);
    }
    let vk =
        VerifyingKey::from_bytes(bytes).map_err(|_| CryptoD1Error::NonCanonicalPointEncoding)?;
    if vk.to_bytes() != *bytes {
        return Err(CryptoD1Error::NonCanonicalPointEncoding);
    }
    if vk.is_weak() {
        return Err(CryptoD1Error::WeakOrSmallOrderPoint);
    }
    Ok(vk)
}

/// Validated canonical, non-weak Ed25519 public key (`32` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CanonicalEd25519PublicKeyV1([u8; ED25519_PUBLIC_KEY_LEN]);

impl CanonicalEd25519PublicKeyV1 {
    /// Validates and wraps a 32-byte Ed25519 public key, rejecting non-canonical encodings and
    /// all 8 small-order torsion points.
    pub fn from_bytes(bytes: [u8; ED25519_PUBLIC_KEY_LEN]) -> Result<Self, CryptoD1Error> {
        let _ = validate_canonical_non_weak_point(&bytes)?;
        Ok(Self(bytes))
    }

    /// Parses a byte slice as a canonical Ed25519 public key.
    pub fn from_slice(slice: &[u8]) -> Result<Self, CryptoD1Error> {
        let arr: [u8; ED25519_PUBLIC_KEY_LEN] =
            slice.try_into().map_err(|_| CryptoD1Error::InvalidLength {
                expected: ED25519_PUBLIC_KEY_LEN,
                actual: slice.len(),
            })?;
        Self::from_bytes(arr)
    }

    /// Derives the canonical public key from a 32-byte Ed25519 secret seed.
    pub fn from_secret_seed(seed: &[u8; 32]) -> Result<Self, CryptoD1Error> {
        let sk = SigningKey::from_bytes(seed);
        Self::from_bytes(sk.verifying_key().to_bytes())
    }

    /// Returns the raw canonical 32-byte encoding.
    pub const fn as_bytes(&self) -> &[u8; ED25519_PUBLIC_KEY_LEN] {
        &self.0
    }

    /// Computes a stable 32-byte SHA-256 key identifier for this canonical public key.
    pub fn key_id(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(b"bitblock/crypto-d1/key-id/v1");
        h.update(
            SignatureSuiteIdV1::Ed25519StrictRfc8032V1
                .code()
                .to_be_bytes(),
        );
        h.update(self.0);
        h.finalize().into()
    }

    fn to_verifying_key(self) -> Result<VerifyingKey, CryptoD1Error> {
        validate_canonical_non_weak_point(&self.0)
    }
}

/// Validated canonical, non-malleable Ed25519 signature (`R || S`, `64` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CanonicalEd25519SignatureV1([u8; ED25519_SIGNATURE_LEN]);

impl CanonicalEd25519SignatureV1 {
    /// Validates and wraps a 64-byte Ed25519 signature, rejecting non-canonical `R` points,
    /// small-order `R` points, and non-canonical scalars `S >= L`.
    pub fn from_bytes(bytes: [u8; ED25519_SIGNATURE_LEN]) -> Result<Self, CryptoD1Error> {
        let mut r_bytes = [0u8; 32];
        let mut s_bytes = [0u8; 32];
        r_bytes.copy_from_slice(&bytes[..32]);
        s_bytes.copy_from_slice(&bytes[32..]);
        if !le_u256_lt(&s_bytes, &ED25519_ORDER_L_LE) {
            return Err(CryptoD1Error::NonCanonicalSignatureScalar);
        }
        let _ = validate_canonical_non_weak_point(&r_bytes)?;
        Ok(Self(bytes))
    }

    /// Parses a byte slice as a canonical Ed25519 signature.
    pub fn from_slice(slice: &[u8]) -> Result<Self, CryptoD1Error> {
        let arr: [u8; ED25519_SIGNATURE_LEN] =
            slice.try_into().map_err(|_| CryptoD1Error::InvalidLength {
                expected: ED25519_SIGNATURE_LEN,
                actual: slice.len(),
            })?;
        Self::from_bytes(arr)
    }

    /// Returns the raw canonical 64-byte encoding.
    pub const fn as_bytes(&self) -> &[u8; ED25519_SIGNATURE_LEN] {
        &self.0
    }
}

/// Canonical domain- and context-separated signing preimage binding protocol, chain, genesis,
/// launch manifest, profile, role domain, and suite IDs to a 32-byte payload digest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SigningPreimageV1 {
    /// Protocol version (`1`).
    pub protocol_version: u16,
    /// Chain identifier preventing cross-chain replay.
    pub chain_id: u64,
    /// Derived genesis identifier (`[0; 32]` only for pre-genesis out-of-band launch docs).
    pub genesis_id: [u8; 32],
    /// Admitted launch-manifest identifier.
    pub launch_manifest_id: [u8; 32],
    /// Admitted profile identifier.
    pub profile_id: [u8; 32],
    /// Role-separated signing domain tag.
    pub domain_tag: SigningDomainTagV1,
    /// Bound signature suite identifier.
    pub signature_suite: SignatureSuiteIdV1,
    /// Bound signing hash suite identifier.
    pub hash_suite: HashSuiteIdV1,
    /// Canonical 32-byte payload digest being signed.
    pub payload_digest: [u8; 32],
}

impl SigningPreimageV1 {
    /// Fixed canonical wire byte length (`143` bytes).
    pub const WIRE_LEN: usize = 143;

    /// Encodes the signing preimage into canonical big-endian bytes.
    pub fn encode_canonical(&self) -> [u8; Self::WIRE_LEN] {
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.protocol_version.to_be_bytes());
        out[2..10].copy_from_slice(&self.chain_id.to_be_bytes());
        out[10..42].copy_from_slice(&self.genesis_id);
        out[42..74].copy_from_slice(&self.launch_manifest_id);
        out[74..106].copy_from_slice(&self.profile_id);
        out[106] = self.domain_tag.code();
        out[107..109].copy_from_slice(&self.signature_suite.code().to_be_bytes());
        out[109..111].copy_from_slice(&self.hash_suite.code().to_be_bytes());
        out[111..143].copy_from_slice(&self.payload_digest);
        out
    }

    /// Decodes a canonical `SigningPreimageV1`.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, CryptoD1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(CryptoD1Error::InvalidLength {
                expected: Self::WIRE_LEN,
                actual: bytes.len(),
            });
        }
        let mut genesis_id = [0u8; 32];
        let mut launch_manifest_id = [0u8; 32];
        let mut profile_id = [0u8; 32];
        let mut payload_digest = [0u8; 32];
        genesis_id.copy_from_slice(&bytes[10..42]);
        launch_manifest_id.copy_from_slice(&bytes[42..74]);
        profile_id.copy_from_slice(&bytes[74..106]);
        payload_digest.copy_from_slice(&bytes[111..143]);
        Ok(Self {
            protocol_version: u16::from_be_bytes([bytes[0], bytes[1]]),
            chain_id: u64::from_be_bytes(bytes[2..10].try_into().expect("8 bytes")),
            genesis_id,
            launch_manifest_id,
            profile_id,
            domain_tag: SigningDomainTagV1::from_code(bytes[106])?,
            signature_suite: SignatureSuiteIdV1::from_code(u16::from_be_bytes([
                bytes[107], bytes[108],
            ]))?,
            hash_suite: HashSuiteIdV1::from_code(u16::from_be_bytes([bytes[109], bytes[110]]))?,
            payload_digest,
        })
    }

    /// Computes the canonical `ContentTag` over this signing preimage.
    pub fn tag(&self) -> ContentTag {
        let bytes = self.encode_canonical();
        hash_canonical(
            TagKind::Receipt,
            CRYPTO_D1_SIGNING_PREIMAGE_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length")
    }

    /// Computes the 32-byte domain-separated SHA-256 signing digest fed into Ed25519.
    pub fn signing_digest(&self) -> [u8; 32] {
        self.tag().digest()
    }
}

/// Deterministically signs `preimage` with a 32-byte Ed25519 secret seed under RFC 8032.
pub fn sign_preimage_deterministic(
    secret_seed: &[u8; 32],
    preimage: &SigningPreimageV1,
) -> Result<CanonicalEd25519SignatureV1, CryptoD1Error> {
    let sk = SigningKey::from_bytes(secret_seed);
    let digest = preimage.signing_digest();
    let sig = sk.sign(&digest);
    CanonicalEd25519SignatureV1::from_bytes(sig.to_bytes())
}

/// Strictly verifies a canonical Ed25519 signature over `preimage` under RFC 8032.
pub fn verify_preimage_strict(
    public_key: &CanonicalEd25519PublicKeyV1,
    preimage: &SigningPreimageV1,
    signature: &CanonicalEd25519SignatureV1,
) -> Result<(), CryptoD1Error> {
    let vk = public_key.to_verifying_key()?;
    let sig = Signature::from_bytes(signature.as_bytes());
    let digest = preimage.signing_digest();
    vk.verify_strict(&digest, &sig)
        .map_err(|_| CryptoD1Error::SignatureVerificationFailed)
}

/// Strictly verifies a raw message byte slice (used for RFC 8032 primitive test vectors).
pub fn verify_raw_message_strict(
    public_key: &CanonicalEd25519PublicKeyV1,
    message: &[u8],
    signature: &CanonicalEd25519SignatureV1,
) -> Result<(), CryptoD1Error> {
    let vk = public_key.to_verifying_key()?;
    let sig = Signature::from_bytes(signature.as_bytes());
    vk.verify_strict(message, &sig)
        .map_err(|_| CryptoD1Error::SignatureVerificationFailed)
}

/// Verifies a slice of `(public_key, preimage, signature)` tuples with safe individual-item
/// strict fallback returning the exact first invalid index on any verification failure.
pub fn verify_batch_with_strict_fallback(
    items: &[(
        CanonicalEd25519PublicKeyV1,
        SigningPreimageV1,
        CanonicalEd25519SignatureV1,
    )],
) -> Result<(), CryptoD1Error> {
    for (idx, (pk, preimage, sig)) in items.iter().enumerate() {
        if verify_preimage_strict(pk, preimage, sig).is_err() {
            return Err(CryptoD1Error::BatchVerificationFailed {
                first_invalid_index: idx,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex_decode<const N: usize>(hex: &str) -> [u8; N] {
        assert_eq!(hex.len(), N * 2);
        let mut out = [0u8; N];
        for (i, chunk) in hex.as_bytes().chunks_exact(2).enumerate() {
            let s = std::str::from_utf8(chunk).unwrap();
            out[i] = u8::from_str_radix(s, 16).unwrap();
        }
        out
    }

    #[test]
    fn suite_descriptor_tag_and_roundtrip_pinned() {
        let desc = CryptoSuiteDescriptorV1::v1_ed25519_sha256();
        let bytes = desc.encode_canonical().unwrap();
        assert_eq!(
            CryptoSuiteDescriptorV1::decode_canonical(&bytes).unwrap(),
            desc
        );
        assert_eq!(
            desc.tag().unwrap().to_string(),
            "#bb.policy.sha256.jczjgonfconrys2yy7cgehmsk6mh6757m4r4piyg5ynglq2uanlq"
        );
    }

    #[test]
    fn rfc8032_test_vectors_and_adversarial_rejections() {
        // RFC 8032 §7.1 TEST 1 (empty message)
        let sk1: [u8; 32] =
            hex_decode("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60");
        let pk1_expected: [u8; 32] =
            hex_decode("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a");
        let sig1_expected: [u8; 64] = hex_decode(
            "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06522490155\
             5fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b",
        );
        let pk1 = CanonicalEd25519PublicKeyV1::from_secret_seed(&sk1).unwrap();
        assert_eq!(pk1.as_bytes(), &pk1_expected);
        let sig1 = CanonicalEd25519SignatureV1::from_bytes(sig1_expected).unwrap();
        verify_raw_message_strict(&pk1, b"", &sig1).unwrap();

        // Malleated signature S' = S + L (invalid scalar >= L) must reject.
        let mut malleated = sig1_expected;
        let mut carry = 0u16;
        for i in 0..32 {
            let sum = malleated[32 + i] as u16 + ED25519_ORDER_L_LE[i] as u16 + carry;
            malleated[32 + i] = (sum & 0xff) as u8;
            carry = sum >> 8;
        }
        assert_eq!(
            CanonicalEd25519SignatureV1::from_bytes(malleated),
            Err(CryptoD1Error::NonCanonicalSignatureScalar)
        );

        // All 8 Ed25519 small-order torsion points must be rejected.
        let weak_points: [&str; 8] = [
            "0100000000000000000000000000000000000000000000000000000000000000", // identity (order 1)
            "ecffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f", // order 2
            "0000000000000000000000000000000000000000000000000000000000000000", // order 4
            "0000000000000000000000000000000000000000000000000000000000000080", // order 4
            "c7176a703d4dd84fba3c0b760d10670f2a2053fa2c39ccc64ec7fd7792ac037a", // order 8
            "c7176a703d4dd84fba3c0b760d10670f2a2053fa2c39ccc64ec7fd7792ac03fa", // order 8
            "26e8958fc2b227b045c3f489f2ef98f0d5dfac05d3c63339b13802886d53fc05", // order 8
            "26e8958fc2b227b045c3f489f2ef98f0d5dfac05d3c63339b13802886d53fc85", // order 8
        ];
        for hex in weak_points {
            let pt: [u8; 32] = hex_decode(hex);
            let err = CanonicalEd25519PublicKeyV1::from_bytes(pt).unwrap_err();
            assert!(
                matches!(
                    err,
                    CryptoD1Error::WeakOrSmallOrderPoint | CryptoD1Error::NonCanonicalPointEncoding
                ),
                "weak point {hex} unexpectedly accepted or wrong error: {err:?}"
            );
        }

        // Non-canonical y = p (= 2^255 - 19) must reject.
        let non_canon_y: [u8; 32] =
            hex_decode("edffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f");
        assert_eq!(
            CanonicalEd25519PublicKeyV1::from_bytes(non_canon_y),
            Err(CryptoD1Error::NonCanonicalPointEncoding)
        );
    }

    #[test]
    fn signing_preimage_cross_domain_and_chain_replay_rejected() {
        let seed = [0x42u8; 32];
        let pk = CanonicalEd25519PublicKeyV1::from_secret_seed(&seed).unwrap();
        let pre = SigningPreimageV1 {
            protocol_version: 1,
            chain_id: 77,
            genesis_id: [0x11; 32],
            launch_manifest_id: [0x22; 32],
            profile_id: [0x33; 32],
            domain_tag: SigningDomainTagV1::ActorTransaction,
            signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
            hash_suite: HashSuiteIdV1::Sha256V1,
            payload_digest: [0x44; 32],
        };
        let encoded = pre.encode_canonical();
        assert_eq!(SigningPreimageV1::decode_canonical(&encoded).unwrap(), pre);
        let sig = sign_preimage_deterministic(&seed, &pre).unwrap();
        verify_preimage_strict(&pk, &pre, &sig).unwrap();

        let mut wrong_domain = pre;
        wrong_domain.domain_tag = SigningDomainTagV1::ConsensusVote;
        assert_eq!(
            verify_preimage_strict(&pk, &wrong_domain, &sig),
            Err(CryptoD1Error::SignatureVerificationFailed)
        );

        let mut wrong_chain = pre;
        wrong_chain.chain_id = 78;
        assert_eq!(
            verify_preimage_strict(&pk, &wrong_chain, &sig),
            Err(CryptoD1Error::SignatureVerificationFailed)
        );

        let batch = [(pk, pre, sig), (pk, wrong_domain, sig)];
        assert_eq!(
            verify_batch_with_strict_fallback(&batch),
            Err(CryptoD1Error::BatchVerificationFailed {
                first_invalid_index: 1
            })
        );
    }
}
