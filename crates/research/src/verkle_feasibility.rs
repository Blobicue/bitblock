//! `VERKLE-R1` feasibility lane: injective 32-byte coordinate embedding,
//! self-excluding [`VectorCommitmentProfileDescriptorV1`], and deterministic
//! arity-256 stem-compressed structural analysis.
//!
//! This module implements the non-authoritative feasibility surface required by
//! `docs/CAPABILITY_TREE.md` (`PS-017:VERKLE-R1@feasibility`) and
//! `docs/AUTHENTICATED_STATE_PLAN.md` (§ "Separate Verkle feasibility lane — not
//! Candidate C"):
//!
//! - [`VerkleCoordinateKeyV1`]: exact injective 32-byte embedding
//!   `0x0000000000000000 || CoordinateKeyV1(c)` with fail-closed decoding and
//!   bijective `(stem_31b, suffix_u8)` decomposition strictly below the
//!   Bandersnatch prime-order subgroup scalar modulus (`< 2^184 << r`).
//! - [`VerkleLeafSlotEncodingV1`]: canonical 32-byte field-element leaf slot
//!   distinguishing absent slots (`[0u8; 32]`) from present values (including
//!   zero-valued payloads) via bit 128 (`2^128 | value_lo`), strictly below `r`.
//! - [`VectorCommitmentProfileDescriptorV1`]: self-excluding canonical descriptor
//!   and domain-separated profile identifier (`TagKind::Policy`) consumable by a
//!   later `DomainRegistryV1`, independent of candidate, genesis, profile, or
//!   root identifiers.
//! - [`analyze_verkle_256_world`]: deterministic 256-ary stem-compressed tree
//!   topology and honest whole-witness byte/MSM cost analyzer over the exact
//!   same worlds benchmarked in `COMMIT-R1`.
//!
//! Strictly research-only: does not modify `World`, legacy `world_tag`,
//! `COMMIT-1`, or `COMMIT-R1`'s two-candidate hash-tree decision, and does not
//! implement production elliptic-curve cryptography or admit a prototype.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use sha2::{Digest, Sha256};

use crate::commit_r1::{CoordinateKeyV1, KEY_BYTES as COORD_KEY_BYTES};
use crate::commit_state::{CommitmentOperationalLimitsDescriptorV1, ProofPolicyDescriptorV1};
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::world::{Coord, World};

/// Canonical schema string for the injective 32-byte [`VerkleCoordinateKeyV1`] embedding.
pub const VERKLE_COORDINATE_KEY_V1_SCHEMA: &[u8] = b"bitblock/verkle-r1/coord-key-32b/v1";
/// Canonical schema string for the self-excluding [`VectorCommitmentProfileDescriptorV1`].
pub const VERKLE_VC_PROFILE_V1_SCHEMA: &[u8] = b"bitblock/verkle-r1/vector-commitment-profile/v1";

/// Byte length (`32`) of a canonical [`VerkleCoordinateKeyV1`].
pub const VERKLE_KEY_BYTES: usize = 32;
/// Byte length (`8`) of the mandatory leading all-zero prefix in [`VerkleCoordinateKeyV1`].
pub const VERKLE_PREFIX_BYTES: usize = 8;
/// Byte length (`31`) of a canonical Verkle tree stem (`bytes[0..31]`).
pub const VERKLE_STEM_BYTES: usize = 31;
/// Branching arity (`256`) of the evaluated Verkle tree candidates.
pub const VERKLE_ARITY: u16 = 256;
/// Fixed-size IPA multiproof floor (`576` bytes = `16 * 32` L/R points + `32` final scalar + `32` D point).
pub const IPA_256_OPENING_FLOOR_BYTES: u32 = 576;
/// Fixed-size single KZG G1 opening proof floor (`48` bytes on BLS12-381).
pub const KZG_BLS12_381_OPENING_FLOOR_BYTES: u32 = 48;
/// Number of elliptic-curve scalar multiplications (`256 + 16 + 1 = 273`) in one arity-256 IPA verification MSM.
pub const IPA_256_VERIFY_MSM_POINTS: usize = 273;

/// Prime-order subgroup scalar field modulus `r` of Bandersnatch (253 bits, big-endian).
///
/// `r = 13108968793781547619861935127046491459309155893440570251786403306729687672801`
/// (`0x1cfb69d4ca675f520cce760202687600ff8f87007419047174fd06b52876e7e1`).
pub const BANDERSNATCH_SCALAR_MODULUS_BE: [u8; 32] = [
    0x1c, 0xfb, 0x69, 0xd4, 0xca, 0x67, 0x5f, 0x52, 0x0c, 0xce, 0x76, 0x02, 0x02, 0x68, 0x76, 0x00,
    0xff, 0x8f, 0x87, 0x00, 0x74, 0x19, 0x04, 0x71, 0x74, 0xfd, 0x06, 0xb5, 0x28, 0x76, 0xe7, 0xe1,
];

/// Scalar field modulus `r` of BLS12-381 (255 bits, big-endian), pinned for rejected KZG comparison.
///
/// `r = 52435875175126190479447740508185965837690552500527637822603658699938581184513`
/// (`0x73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001`).
pub const BLS12_381_SCALAR_MODULUS_BE: [u8; 32] = [
    0x73, 0xed, 0xa7, 0x53, 0x29, 0x9d, 0x7d, 0x48, 0x33, 0x39, 0xd8, 0x08, 0x09, 0xa1, 0xd8, 0x05,
    0x53, 0xbd, 0xa4, 0x02, 0xff, 0xfe, 0x5b, 0xfe, 0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x01,
];

const GENERATOR_BASIS_DOMAIN: &[u8] = b"bitblock/verkle-r1/generator-basis-spec/v1";
const FIAT_SHAMIR_DOMAIN: &[u8] = b"bitblock/verkle-r1/fiat-shamir-transcript/v1";
const WORLD_ROOT_CANDIDATE_DOMAIN: &[u8] = b"bitblock/verkle-r1/world-root-candidate/v1";

/// Error returned when decoding or validating a `VERKLE-R1` key or profile descriptor fails closed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VerkleFeasibilityError {
    /// Input key slice did not have exact length `32`.
    InvalidKeyLength { expected: usize, actual: usize },
    /// Leading 8-byte prefix of a 32-byte coordinate key or 31-byte stem was non-zero.
    NonZeroCoordinatePrefix,
    /// Serialized descriptor byte length did not match the fixed canonical wire length.
    InvalidDescriptorLength { expected: usize, actual: usize },
    /// Descriptor schema version is unsupported (must be `1`).
    UnsupportedSchemaVersion(u16),
    /// Unknown wire code for a vector-commitment scheme, setup model, or subgroup policy.
    UnknownEnumCode { field: &'static str, code: u8 },
    /// Descriptor failed canonical cross-field validation.
    InvalidProfileField { field: &'static str },
}

impl fmt::Display for VerkleFeasibilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidKeyLength { expected, actual } => {
                write!(
                    f,
                    "invalid Verkle key length: expected {expected}, got {actual}"
                )
            }
            Self::NonZeroCoordinatePrefix => {
                write!(f, "VerkleCoordinateKeyV1 requires leading 8 zero bytes")
            }
            Self::InvalidDescriptorLength { expected, actual } => write!(
                f,
                "invalid VectorCommitmentProfileDescriptorV1 length: expected {expected}, got {actual}"
            ),
            Self::UnsupportedSchemaVersion(v) => {
                write!(
                    f,
                    "unsupported VectorCommitmentProfileDescriptorV1 version {v}"
                )
            }
            Self::UnknownEnumCode { field, code } => {
                write!(f, "unknown {field} wire code {code}")
            }
            Self::InvalidProfileField { field } => {
                write!(f, "invalid canonical profile field: {field}")
            }
        }
    }
}

impl std::error::Error for VerkleFeasibilityError {}

/// Injective 32-byte Verkle coordinate embedding:
/// `VerkleCoordinateKeyV1(c) = 0x0000000000000000 || CoordinateKeyV1(c)`.
///
/// Only keys with exact length 32 and exact 8-byte zero prefix are accepted.
/// Splitting into `(stem = bytes[0..31], suffix = bytes[31])` is bijective and
/// guarantees every stem integer is strictly `< 2^184 << r_Bandersnatch` (69 bits
/// of headroom below the 253-bit prime-order scalar modulus), preventing any
/// modular-reduction alias.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VerkleCoordinateKeyV1([u8; VERKLE_KEY_BYTES]);

impl VerkleCoordinateKeyV1 {
    /// Encodes a signed `Coord` into its canonical 32-byte `VerkleCoordinateKeyV1`.
    pub const fn from_coord(coord: Coord) -> Self {
        let base = CoordinateKeyV1::from_coord(coord);
        let src = base.as_bytes();
        let mut out = [0u8; VERKLE_KEY_BYTES];
        let mut i = 0;
        while i < COORD_KEY_BYTES {
            out[VERKLE_PREFIX_BYTES + i] = src[i];
            i += 1;
        }
        Self(out)
    }

    /// Decodes a canonical 32-byte `VerkleCoordinateKeyV1`, rejecting wrong length or non-zero prefix.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, VerkleFeasibilityError> {
        if bytes.len() != VERKLE_KEY_BYTES {
            return Err(VerkleFeasibilityError::InvalidKeyLength {
                expected: VERKLE_KEY_BYTES,
                actual: bytes.len(),
            });
        }
        if bytes[..VERKLE_PREFIX_BYTES] != [0u8; VERKLE_PREFIX_BYTES] {
            return Err(VerkleFeasibilityError::NonZeroCoordinatePrefix);
        }
        let mut out = [0u8; VERKLE_KEY_BYTES];
        out.copy_from_slice(bytes);
        Ok(Self(out))
    }

    /// Reconstructs a canonical `VerkleCoordinateKeyV1` from its 31-byte stem and 1-byte suffix.
    pub fn from_stem_and_suffix(
        stem: [u8; VERKLE_STEM_BYTES],
        suffix: u8,
    ) -> Result<Self, VerkleFeasibilityError> {
        if stem[..VERKLE_PREFIX_BYTES] != [0u8; VERKLE_PREFIX_BYTES] {
            return Err(VerkleFeasibilityError::NonZeroCoordinatePrefix);
        }
        let mut out = [0u8; VERKLE_KEY_BYTES];
        out[..VERKLE_STEM_BYTES].copy_from_slice(&stem);
        out[VERKLE_STEM_BYTES] = suffix;
        Ok(Self(out))
    }

    /// Returns the canonical 32-byte big-endian key slice.
    pub const fn as_bytes(&self) -> &[u8; VERKLE_KEY_BYTES] {
        &self.0
    }

    /// Decodes the embedded `CoordinateKeyV1` back into its exact signed `Coord`.
    pub fn to_coord(&self) -> Coord {
        let mut raw = [0u8; COORD_KEY_BYTES];
        raw.copy_from_slice(&self.0[VERKLE_PREFIX_BYTES..VERKLE_KEY_BYTES]);
        CoordinateKeyV1::from_bytes(raw).to_coord()
    }

    /// Returns the canonical 31-byte Verkle stem (`bytes[0..31]`).
    pub const fn stem(&self) -> [u8; VERKLE_STEM_BYTES] {
        let mut out = [0u8; VERKLE_STEM_BYTES];
        let mut i = 0;
        while i < VERKLE_STEM_BYTES {
            out[i] = self.0[i];
            i += 1;
        }
        out
    }

    /// Returns the canonical 1-byte leaf-slot suffix (`bytes[31]`, selecting `0..=255`).
    pub const fn suffix(&self) -> u8 {
        self.0[VERKLE_STEM_BYTES]
    }

    /// Encodes the 31-byte stem as a 32-byte unsigned big-endian field scalar (`0x00 || stem`).
    pub const fn stem_as_be_u256(&self) -> [u8; 32] {
        let mut out = [0u8; 32];
        let mut i = 0;
        while i < VERKLE_STEM_BYTES {
            out[1 + i] = self.0[i];
            i += 1;
        }
        out
    }

    /// Returns whether the big-endian stem integer is strictly below `modulus_be` (zero modular reduction).
    pub fn stem_strictly_below_modulus(&self, modulus_be: &[u8; 32]) -> bool {
        self.stem_as_be_u256() < *modulus_be
    }

    /// Computes the canonical `ContentTag` for this 32-byte `VerkleCoordinateKeyV1`.
    pub fn tag(&self) -> ContentTag {
        hash_canonical(
            TagKind::World,
            VERKLE_COORDINATE_KEY_V1_SCHEMA,
            VERKLE_KEY_BYTES as u64,
            |sink| sink(&self.0),
        )
        .expect("VerkleCoordinateKeyV1 tag encoding is canonical")
    }
}

/// Canonical 32-byte field-element leaf-slot encoding separating absence from any present value.
///
/// An absent slot encodes to `[0u8; 32]`. A present slot encodes `2^128 | value_lo`
/// (setting bit 128 in byte 15), so even a present zero-valued payload (`value_lo = 0`)
/// never aliases an absent slot, and the resulting 129-bit integer lies strictly below
/// both `BANDERSNATCH_SCALAR_MODULUS_BE` and `BLS12_381_SCALAR_MODULUS_BE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerkleLeafSlotEncodingV1;

impl VerkleLeafSlotEncodingV1 {
    /// Canonical 32-byte encoding of an absent leaf slot (all zeros).
    pub const ABSENT_SLOT: [u8; 32] = [0u8; 32];

    /// Encodes a present 128-bit payload value with the mandatory bit-128 presence marker.
    pub const fn present_slot(value_lo: u128) -> [u8; 32] {
        let lo = value_lo.to_be_bytes();
        let mut out = [0u8; 32];
        out[15] = 0x01; // bit 128 = 1 (present marker)
        let mut i = 0;
        while i < 16 {
            out[16 + i] = lo[i];
            i += 1;
        }
        out
    }

    /// Canonical present slot for `OnV1` (`value_lo = 1`, presence bit 128 set).
    pub const fn present_on_v1() -> [u8; 32] {
        Self::present_slot(1)
    }

    /// Returns `true` iff `slot` carries the bit-128 presence marker and is strictly canonical.
    pub const fn is_present(slot: &[u8; 32]) -> bool {
        let mut i = 0;
        while i < 15 {
            if slot[i] != 0 {
                return false;
            }
            i += 1;
        }
        slot[15] == 0x01
    }
}

/// Evaluated vector-commitment scheme family identifier in `VERKLE-R1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum VectorCommitmentSchemeIdV1 {
    /// Primary feasibility subject VC-1: Bandersnatch twisted-Edwards Pedersen + arity-256 IPA.
    BandersnatchPedersenIpa256 = 1,
    /// Rejected alternative VC-2: BLS12-381 KZG arity-256 (trusted setup + pairing TCB).
    Bls12381Kzg256 = 2,
    /// Rejected alternative VC-3: Catalano–Fiore hidden-order RSA/class-group arity-256.
    CatalanoFioreRsa256 = 3,
    /// Deferred alternative VC-4: Post-quantum lattice/FRI vector commitment placeholder.
    LatticeFriDeferred = 4,
}

impl VectorCommitmentSchemeIdV1 {
    /// Returns the canonical 1-byte wire code.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte wire code.
    pub const fn from_code(code: u8) -> Result<Self, VerkleFeasibilityError> {
        match code {
            1 => Ok(Self::BandersnatchPedersenIpa256),
            2 => Ok(Self::Bls12381Kzg256),
            3 => Ok(Self::CatalanoFioreRsa256),
            4 => Ok(Self::LatticeFriDeferred),
            _ => Err(VerkleFeasibilityError::UnknownEnumCode {
                field: "vc_scheme",
                code,
            }),
        }
    }
}

/// Setup and generator-basis custody model in `VERKLE-R1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum SetupCustodyModelV1 {
    /// Transparent hash-to-curve generator basis (`trusted_setup_required_u8 = 0`, no toxic waste).
    TransparentHashToCurve = 1,
    /// Updatable universal trusted-setup powers-of-tau ceremony (`trusted_setup_required_u8 = 1`).
    UpdatableTrustedCeremony = 2,
    /// Hidden-order RSA or class-group setup (`trusted_setup_required_u8 = 1`).
    HiddenOrderGroupSetup = 3,
}

impl SetupCustodyModelV1 {
    /// Returns the canonical 1-byte wire code.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte wire code.
    pub const fn from_code(code: u8) -> Result<Self, VerkleFeasibilityError> {
        match code {
            1 => Ok(Self::TransparentHashToCurve),
            2 => Ok(Self::UpdatableTrustedCeremony),
            3 => Ok(Self::HiddenOrderGroupSetup),
            _ => Err(VerkleFeasibilityError::UnknownEnumCode {
                field: "setup_custody_model",
                code,
            }),
        }
    }
}

/// Point/field validation and subgroup rejection policy in `VERKLE-R1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum SubgroupCheckPolicyV1 {
    /// Must reject non-canonical field coordinates, off-curve points, small-subgroup elements,
    /// and identity/infinity points wherever forbidden before any group operation.
    RejectNonCanonicalOffCurveAndSmallSubgroup = 1,
}

impl SubgroupCheckPolicyV1 {
    /// Returns the canonical 1-byte wire code.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte wire code.
    pub const fn from_code(code: u8) -> Result<Self, VerkleFeasibilityError> {
        match code {
            1 => Ok(Self::RejectNonCanonicalOffCurveAndSmallSubgroup),
            _ => Err(VerkleFeasibilityError::UnknownEnumCode {
                field: "subgroup_check_policy",
                code,
            }),
        }
    }
}

/// Self-excluding canonical vector-commitment profile descriptor (`247` wire bytes).
///
/// Freezes the vector-commitment scheme, arity, scalar field modulus, point/opening
/// byte sizes, setup/custody model, subgroup check policy, generator-basis and
/// Fiat–Shamir transcript domain digests, coordinate-key embedding digest, tree
/// domain digest, and bound proof-policy / operational-limits identifiers.
///
/// Strictly self-excluding: contains neither its own derived identifier nor any
/// candidate, genesis, network profile, or produced state-root identifier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VectorCommitmentProfileDescriptorV1 {
    /// Schema version (must equal `1`).
    pub descriptor_schema_version: u16,
    /// Vector-commitment scheme family.
    pub vc_scheme: VectorCommitmentSchemeIdV1,
    /// Branching arity (`256`).
    pub arity_u16: u16,
    /// Prime-order scalar field modulus in big-endian bytes.
    pub scalar_field_modulus_be: [u8; 32],
    /// Canonical compressed commitment byte length (`32` for Bandersnatch, `48` for BLS12-381).
    pub commitment_bytes_u16: u16,
    /// Fixed lower-bound byte length of one opening proof (`576` for IPA-256, `48` for KZG).
    pub opening_proof_fixed_floor_bytes_u32: u32,
    /// Hard maximum multiproof payload byte cap before allocation.
    pub max_multiproof_bytes_u32: u32,
    /// Hard maximum number of distinct opened stems per batch witness.
    pub max_opened_stems_u32: u32,
    /// Setup and generator custody model.
    pub setup_custody_model: SetupCustodyModelV1,
    /// `0` iff transparent setup with no toxic waste, `1` if trusted/hidden-order setup required.
    pub trusted_setup_required_u8: u8,
    /// `0` for classical DLOG/pairing/RSA schemes, `1` for post-quantum schemes.
    pub post_quantum_u8: u8,
    /// Mandatory on-curve, canonical-coordinate, and prime-order subgroup validation rule.
    pub subgroup_check_policy: SubgroupCheckPolicyV1,
    /// SHA-256 commitment to the deterministic generator-basis derivation specification.
    pub generator_basis_digest: [u8; 32],
    /// SHA-256 commitment to the Fiat–Shamir transcript domain separator.
    pub transcript_domain_digest: [u8; 32],
    /// SHA-256 digest of the canonical coordinate-key embedding schema (`VERKLE_COORDINATE_KEY_V1_SCHEMA`).
    pub key_embedding_schema_digest: [u8; 32],
    /// SHA-256 digest of the candidate world-root tree domain (`bitblock/verkle-r1/world-root-candidate/v1`).
    pub tree_domain_digest: [u8; 32],
    /// Bound `ProofPolicyIdV1` from `COMMIT-1`.
    pub proof_policy_id: [u8; 32],
    /// Bound `CommitmentOperationalLimitsIdV1` from `COMMIT-1`.
    pub operational_limits_id: [u8; 32],
}

fn sha256_bytes(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

impl VectorCommitmentProfileDescriptorV1 {
    /// Fixed canonical wire byte length (`247` bytes).
    pub const WIRE_LEN: usize = 2 + 1 + 2 + 32 + 2 + 4 + 4 + 4 + 1 + 1 + 1 + 1 + 32 * 6;

    /// Primary `VERKLE-R1` feasibility subject: Bandersnatch Pedersen + arity-256 IPA
    /// (transparent hash-to-curve basis, no trusted setup, classical DLOG+ROM, deferred for local profiles).
    pub fn bandersnatch_ipa_256_v1() -> Self {
        let scheme = VectorCommitmentSchemeIdV1::BandersnatchPedersenIpa256;
        let arity = VERKLE_ARITY;
        let arity_be = arity.to_be_bytes();
        let scheme_b = [scheme.code()];
        Self {
            descriptor_schema_version: 1,
            vc_scheme: scheme,
            arity_u16: arity,
            scalar_field_modulus_be: BANDERSNATCH_SCALAR_MODULUS_BE,
            commitment_bytes_u16: 32,
            opening_proof_fixed_floor_bytes_u32: IPA_256_OPENING_FLOOR_BYTES,
            max_multiproof_bytes_u32: 131_072,
            max_opened_stems_u32: 1_024,
            setup_custody_model: SetupCustodyModelV1::TransparentHashToCurve,
            trusted_setup_required_u8: 0,
            post_quantum_u8: 0,
            subgroup_check_policy:
                SubgroupCheckPolicyV1::RejectNonCanonicalOffCurveAndSmallSubgroup,
            generator_basis_digest: sha256_bytes(&[
                GENERATOR_BASIS_DOMAIN,
                &scheme_b,
                &arity_be,
                &BANDERSNATCH_SCALAR_MODULUS_BE,
            ]),
            transcript_domain_digest: sha256_bytes(&[FIAT_SHAMIR_DOMAIN, &scheme_b, &arity_be]),
            key_embedding_schema_digest: sha256_bytes(&[VERKLE_COORDINATE_KEY_V1_SCHEMA]),
            tree_domain_digest: sha256_bytes(&[WORLD_ROOT_CANDIDATE_DOMAIN]),
            proof_policy_id: ProofPolicyDescriptorV1::WORLD_DEFAULT.id(),
            operational_limits_id: CommitmentOperationalLimitsDescriptorV1::LOCAL_DEFAULT.id(),
        }
    }

    /// Evaluated and rejected alternative VC-2: BLS12-381 KZG arity-256 (requires updatable
    /// trusted-setup ceremony and pairing TCB).
    pub fn bls12_381_kzg_256_rejected_v1() -> Self {
        let scheme = VectorCommitmentSchemeIdV1::Bls12381Kzg256;
        let arity = VERKLE_ARITY;
        let arity_be = arity.to_be_bytes();
        let scheme_b = [scheme.code()];
        Self {
            descriptor_schema_version: 1,
            vc_scheme: scheme,
            arity_u16: arity,
            scalar_field_modulus_be: BLS12_381_SCALAR_MODULUS_BE,
            commitment_bytes_u16: 48,
            opening_proof_fixed_floor_bytes_u32: KZG_BLS12_381_OPENING_FLOOR_BYTES,
            max_multiproof_bytes_u32: 131_072,
            max_opened_stems_u32: 1_024,
            setup_custody_model: SetupCustodyModelV1::UpdatableTrustedCeremony,
            trusted_setup_required_u8: 1,
            post_quantum_u8: 0,
            subgroup_check_policy:
                SubgroupCheckPolicyV1::RejectNonCanonicalOffCurveAndSmallSubgroup,
            generator_basis_digest: sha256_bytes(&[
                GENERATOR_BASIS_DOMAIN,
                &scheme_b,
                &arity_be,
                &BLS12_381_SCALAR_MODULUS_BE,
            ]),
            transcript_domain_digest: sha256_bytes(&[FIAT_SHAMIR_DOMAIN, &scheme_b, &arity_be]),
            key_embedding_schema_digest: sha256_bytes(&[VERKLE_COORDINATE_KEY_V1_SCHEMA]),
            tree_domain_digest: sha256_bytes(&[WORLD_ROOT_CANDIDATE_DOMAIN]),
            proof_policy_id: ProofPolicyDescriptorV1::WORLD_DEFAULT.id(),
            operational_limits_id: CommitmentOperationalLimitsDescriptorV1::LOCAL_DEFAULT.id(),
        }
    }

    /// Validates canonical field ranges and cross-field invariants fail-closed.
    pub fn validate(&self) -> Result<(), VerkleFeasibilityError> {
        if self.descriptor_schema_version != 1 {
            return Err(VerkleFeasibilityError::UnsupportedSchemaVersion(
                self.descriptor_schema_version,
            ));
        }
        if self.arity_u16 < 2 || !self.arity_u16.is_power_of_two() {
            return Err(VerkleFeasibilityError::InvalidProfileField { field: "arity_u16" });
        }
        if self.scalar_field_modulus_be == [0u8; 32] {
            return Err(VerkleFeasibilityError::InvalidProfileField {
                field: "scalar_field_modulus_be",
            });
        }
        // Maximum possible canonical Verkle stem (Coord::MAX) must fit strictly below modulus!
        let max_key = VerkleCoordinateKeyV1::from_coord(Coord::new(i64::MAX, i64::MAX, i64::MAX));
        if !max_key.stem_strictly_below_modulus(&self.scalar_field_modulus_be) {
            return Err(VerkleFeasibilityError::InvalidProfileField {
                field: "scalar_field_modulus_be",
            });
        }
        if self.commitment_bytes_u16 == 0 {
            return Err(VerkleFeasibilityError::InvalidProfileField {
                field: "commitment_bytes_u16",
            });
        }
        if self.opening_proof_fixed_floor_bytes_u32 == 0
            || self.max_multiproof_bytes_u32 < self.opening_proof_fixed_floor_bytes_u32
        {
            return Err(VerkleFeasibilityError::InvalidProfileField {
                field: "max_multiproof_bytes_u32",
            });
        }
        if self.max_opened_stems_u32 == 0 {
            return Err(VerkleFeasibilityError::InvalidProfileField {
                field: "max_opened_stems_u32",
            });
        }
        let expected_trusted = match self.setup_custody_model {
            SetupCustodyModelV1::TransparentHashToCurve => 0,
            SetupCustodyModelV1::UpdatableTrustedCeremony
            | SetupCustodyModelV1::HiddenOrderGroupSetup => 1,
        };
        if self.trusted_setup_required_u8 != expected_trusted {
            return Err(VerkleFeasibilityError::InvalidProfileField {
                field: "trusted_setup_required_u8",
            });
        }
        let expected_pq = match self.vc_scheme {
            VectorCommitmentSchemeIdV1::LatticeFriDeferred => 1,
            VectorCommitmentSchemeIdV1::BandersnatchPedersenIpa256
            | VectorCommitmentSchemeIdV1::Bls12381Kzg256
            | VectorCommitmentSchemeIdV1::CatalanoFioreRsa256 => 0,
        };
        if self.post_quantum_u8 != expected_pq {
            return Err(VerkleFeasibilityError::InvalidProfileField {
                field: "post_quantum_u8",
            });
        }
        for (field, digest) in [
            ("generator_basis_digest", &self.generator_basis_digest),
            ("transcript_domain_digest", &self.transcript_domain_digest),
            (
                "key_embedding_schema_digest",
                &self.key_embedding_schema_digest,
            ),
            ("tree_domain_digest", &self.tree_domain_digest),
            ("proof_policy_id", &self.proof_policy_id),
            ("operational_limits_id", &self.operational_limits_id),
        ] {
            if *digest == [0u8; 32] {
                return Err(VerkleFeasibilityError::InvalidProfileField { field });
            }
        }
        Ok(())
    }

    /// Encodes this descriptor into its exact `247`-byte canonical big-endian wire format.
    pub fn encode_canonical(&self) -> [u8; Self::WIRE_LEN] {
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.descriptor_schema_version.to_be_bytes());
        out[2] = self.vc_scheme.code();
        out[3..5].copy_from_slice(&self.arity_u16.to_be_bytes());
        out[5..37].copy_from_slice(&self.scalar_field_modulus_be);
        out[37..39].copy_from_slice(&self.commitment_bytes_u16.to_be_bytes());
        out[39..43].copy_from_slice(&self.opening_proof_fixed_floor_bytes_u32.to_be_bytes());
        out[43..47].copy_from_slice(&self.max_multiproof_bytes_u32.to_be_bytes());
        out[47..51].copy_from_slice(&self.max_opened_stems_u32.to_be_bytes());
        out[51] = self.setup_custody_model.code();
        out[52] = self.trusted_setup_required_u8;
        out[53] = self.post_quantum_u8;
        out[54] = self.subgroup_check_policy.code();
        out[55..87].copy_from_slice(&self.generator_basis_digest);
        out[87..119].copy_from_slice(&self.transcript_domain_digest);
        out[119..151].copy_from_slice(&self.key_embedding_schema_digest);
        out[151..183].copy_from_slice(&self.tree_domain_digest);
        out[183..215].copy_from_slice(&self.proof_policy_id);
        out[215..247].copy_from_slice(&self.operational_limits_id);
        out
    }

    /// Decodes and validates a canonical `247`-byte `VectorCommitmentProfileDescriptorV1`.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, VerkleFeasibilityError> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(VerkleFeasibilityError::InvalidDescriptorLength {
                expected: Self::WIRE_LEN,
                actual: bytes.len(),
            });
        }
        let read_32 = |offset: usize| -> [u8; 32] {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes[offset..offset + 32]);
            arr
        };
        let desc = Self {
            descriptor_schema_version: u16::from_be_bytes([bytes[0], bytes[1]]),
            vc_scheme: VectorCommitmentSchemeIdV1::from_code(bytes[2])?,
            arity_u16: u16::from_be_bytes([bytes[3], bytes[4]]),
            scalar_field_modulus_be: read_32(5),
            commitment_bytes_u16: u16::from_be_bytes([bytes[37], bytes[38]]),
            opening_proof_fixed_floor_bytes_u32: u32::from_be_bytes([
                bytes[39], bytes[40], bytes[41], bytes[42],
            ]),
            max_multiproof_bytes_u32: u32::from_be_bytes([
                bytes[43], bytes[44], bytes[45], bytes[46],
            ]),
            max_opened_stems_u32: u32::from_be_bytes([bytes[47], bytes[48], bytes[49], bytes[50]]),
            setup_custody_model: SetupCustodyModelV1::from_code(bytes[51])?,
            trusted_setup_required_u8: bytes[52],
            post_quantum_u8: bytes[53],
            subgroup_check_policy: SubgroupCheckPolicyV1::from_code(bytes[54])?,
            generator_basis_digest: read_32(55),
            transcript_domain_digest: read_32(87),
            key_embedding_schema_digest: read_32(119),
            tree_domain_digest: read_32(151),
            proof_policy_id: read_32(183),
            operational_limits_id: read_32(215),
        };
        desc.validate()?;
        Ok(desc)
    }

    /// Computes the canonical `ContentTag` (`TagKind::Policy`) for this descriptor.
    pub fn tag(&self) -> ContentTag {
        let encoded = self.encode_canonical();
        hash_canonical(
            TagKind::Policy,
            VERKLE_VC_PROFILE_V1_SCHEMA,
            Self::WIRE_LEN as u64,
            |sink| sink(&encoded),
        )
        .expect("VectorCommitmentProfileDescriptorV1 tag encoding is canonical")
    }

    /// Returns the domain-separated 32-byte `VectorCommitmentProfileIdV1` digest consumable by a
    /// future `DomainRegistryV1` (strictly independent of candidate, genesis, or root IDs).
    pub fn profile_id(&self) -> [u8; 32] {
        self.tag().digest()
    }
}

/// Exact deterministic structural and whole-witness cost breakdown for an arity-256
/// stem-compressed Verkle tree over [`VerkleCoordinateKeyV1`] on a concrete [`World`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerkleTreeShapeMetricsV1 {
    /// Total occupied coordinates (`OnV1`) in the world.
    pub occupied_keys: usize,
    /// Number of unique 31-byte Verkle stems (`CoordinateKeyV1[0..23]`, grouping 256 adjacent `z` slots).
    pub unique_stems: usize,
    /// Number of arity-256 internal branch nodes above the stems.
    pub internal_nodes: usize,
    /// Maximum internal branch depth across all stems (`0` when `unique_stems <= 1`).
    pub max_internal_depth: usize,
    /// Internal branch depth along the median-key query path.
    pub single_query_internal_commitments: usize,
    /// Complete honest whole-witness bytes for a single key under Bandersnatch IPA-256
    /// (`576` B IPA opening + path/stem/sub-extension commitments + 32 B key + 1 B value + 16 B framing).
    pub single_query_ipa_whole_witness_bytes: usize,
    /// Complete honest whole-witness bytes for a single key under rejected BLS12-381 KZG-256
    /// (`48` B KZG opening + path/stem/sub-extension 48 B commitments + 32 B key + 1 B value + 16 B framing).
    pub single_query_kzg_whole_witness_bytes: usize,
    /// Complete honest whole-witness bytes for the clustered batch query under Bandersnatch IPA-256.
    pub clustered_batch_ipa_whole_witness_bytes: usize,
    /// Complete honest whole-witness bytes for the dispersed batch query under Bandersnatch IPA-256.
    pub dispersed_batch_ipa_whole_witness_bytes: usize,
    /// Pedersen scalar multiplications needed for one incremental edit (`2 + internal_depth`).
    pub single_edit_pedersen_scalar_muls: usize,
    /// Fixed MSM point count (`273`) required to verify one arity-256 IPA multiproof.
    pub ipa_verify_msm_points: usize,
}

#[derive(Debug)]
enum VerkleSimNode {
    Stem,
    Internal {
        byte_depth: usize,
        children: BTreeMap<u8, VerkleSimNode>,
    },
}

fn build_verkle_sim(stems: &[[u8; VERKLE_STEM_BYTES]], byte_depth: usize) -> VerkleSimNode {
    if stems.len() == 1 || byte_depth >= VERKLE_STEM_BYTES {
        return VerkleSimNode::Stem;
    }
    let mut groups: BTreeMap<u8, Vec<[u8; VERKLE_STEM_BYTES]>> = BTreeMap::new();
    for &stem in stems {
        groups.entry(stem[byte_depth]).or_default().push(stem);
    }
    if groups.len() == 1 {
        // EIP-6800 does not path-compress single-child internal nodes along diverging prefixes?
        // Actually in EIP-6800, two stems sharing a prefix of `k` bytes descend `k` internal nodes
        // until the first differing byte `k` (each byte `0..k` creates an internal node unless
        // path-compressed). Here we model the MORE FAVORABLE path-compressed arity-256 tree so
        // our deferral conclusion holds even against the tightest possible Verkle witness count!
        let only = groups.into_values().next().expect("non-empty group");
        return build_verkle_sim(&only, byte_depth + 1);
    }
    let mut children = BTreeMap::new();
    for (byte_val, sub) in groups {
        children.insert(byte_val, build_verkle_sim(&sub, byte_depth + 1));
    }
    VerkleSimNode::Internal {
        byte_depth,
        children,
    }
}

fn count_sim_nodes(node: &VerkleSimNode) -> (usize, usize) {
    match node {
        VerkleSimNode::Stem => (0, 0),
        VerkleSimNode::Internal { children, .. } => {
            let mut total_internal = 1usize;
            let mut max_depth = 0usize;
            for child in children.values() {
                let (c_int, c_depth) = count_sim_nodes(child);
                total_internal += c_int;
                max_depth = max_depth.max(c_depth);
            }
            (total_internal, 1 + max_depth)
        }
    }
}

/// Collects deduplicated internal node IDs `(byte_depth, prefix_prefix)` touched by `query_stems`.
fn trace_batch_internal_nodes(
    node: &VerkleSimNode,
    query_stems: &BTreeSet<[u8; VERKLE_STEM_BYTES]>,
    depth_id: &mut usize,
    touched_internal: &mut usize,
    touched_stems: &mut usize,
) {
    if query_stems.is_empty() {
        return;
    }
    match node {
        VerkleSimNode::Stem => {
            *touched_stems += 1;
        }
        VerkleSimNode::Internal {
            byte_depth,
            children,
        } => {
            *depth_id += 1;
            *touched_internal += 1;
            for (&b, child) in children {
                let subset: BTreeSet<[u8; VERKLE_STEM_BYTES]> = query_stems
                    .iter()
                    .copied()
                    .filter(|s| s[*byte_depth] == b)
                    .collect();
                if !subset.is_empty() {
                    trace_batch_internal_nodes(
                        child,
                        &subset,
                        depth_id,
                        touched_internal,
                        touched_stems,
                    );
                }
            }
        }
    }
}

/// Computes complete honest whole-witness byte size for opening `queries` in `root`, including:
/// - the shared opening proof floor (`576` B for IPA-256 or `48` B for KZG),
/// - every deduplicated internal commitment (excluding the root commitment already known to verifier),
/// - every touched stem commitment (`stem_comm` + `C1`/`C2` sub-extension commitment = `2 * comm_bytes` per unique touched stem),
/// - every opened `(32-byte key, 1-byte value)` pair (or 31-byte stem + 1-byte suffix + value),
/// - and fixed `16`-byte witness header framing.
pub fn estimate_verkle_batch_whole_witness_bytes(
    world: &World,
    queries: &[Coord],
    commitment_bytes: usize,
    opening_floor_bytes: usize,
) -> usize {
    let mut stem_set: BTreeSet<[u8; VERKLE_STEM_BYTES]> = BTreeSet::new();
    for c in world.iter_on() {
        stem_set.insert(VerkleCoordinateKeyV1::from_coord(c).stem());
    }
    let stems: Vec<[u8; VERKLE_STEM_BYTES]> = stem_set.into_iter().collect();
    if stems.is_empty() || queries.is_empty() {
        return opening_floor_bytes + 16;
    }
    let sim = build_verkle_sim(&stems, 0);
    let q_stems: BTreeSet<[u8; VERKLE_STEM_BYTES]> = queries
        .iter()
        .copied()
        .map(|c| VerkleCoordinateKeyV1::from_coord(c).stem())
        .collect();
    let mut depth_id = 0usize;
    let mut touched_internal = 0usize;
    let mut touched_stems = 0usize;
    trace_batch_internal_nodes(
        &sim,
        &q_stems,
        &mut depth_id,
        &mut touched_internal,
        &mut touched_stems,
    );
    // Root commitment is already supplied by caller; non-root internal commitments + 2 commitments per touched stem (stem + C1/C2).
    let non_root_internal = touched_internal.saturating_sub(1);
    let stem_commitments = q_stems.len().max(touched_stems) * 2;
    let total_commitments = non_root_internal + stem_commitments;
    // Opened payloads: 31 bytes per unique stem + 2 bytes per opened (suffix, OnV1) value + 16 B header
    let opened_payload_bytes = q_stems.len() * VERKLE_STEM_BYTES + queries.len() * 2;
    opening_floor_bytes + total_commitments * commitment_bytes + opened_payload_bytes + 16
}

/// Deterministically analyzes the arity-256 stem-compressed Verkle tree shape and honest
/// whole-witness byte costs for `world` on a median single query, `clustered_batch`, and
/// `dispersed_batch`.
pub fn analyze_verkle_256_world(
    world: &World,
    clustered_batch: &[Coord],
    dispersed_batch: &[Coord],
) -> VerkleTreeShapeMetricsV1 {
    let mut keys: Vec<(VerkleCoordinateKeyV1, Coord)> = world
        .iter_on()
        .map(|c| (VerkleCoordinateKeyV1::from_coord(c), c))
        .collect();
    keys.sort_unstable_by_key(|(k, _)| *k);

    let mut stem_set: BTreeSet<[u8; VERKLE_STEM_BYTES]> = BTreeSet::new();
    for (k, _) in &keys {
        stem_set.insert(k.stem());
    }
    let stems: Vec<[u8; VERKLE_STEM_BYTES]> = stem_set.into_iter().collect();
    let unique_stems = stems.len();
    let (internal_nodes, max_internal_depth, single_query_internal_commitments) =
        if stems.is_empty() {
            (0, 0, 0)
        } else {
            let sim = build_verkle_sim(&stems, 0);
            let (int_n, max_d) = count_sim_nodes(&sim);
            let median_stem = keys[keys.len() / 2].0.stem();
            let mut single_set = BTreeSet::new();
            single_set.insert(median_stem);
            let mut depth_id = 0usize;
            let mut touched_internal = 0usize;
            let mut touched_stems = 0usize;
            trace_batch_internal_nodes(
                &sim,
                &single_set,
                &mut depth_id,
                &mut touched_internal,
                &mut touched_stems,
            );
            (int_n, max_d, touched_internal)
        };

    let median_coord = keys
        .get(keys.len() / 2)
        .map(|(_, c)| *c)
        .unwrap_or(Coord::new(0, 0, 0));
    let single_query_ipa_whole_witness_bytes = estimate_verkle_batch_whole_witness_bytes(
        world,
        &[median_coord],
        32,
        IPA_256_OPENING_FLOOR_BYTES as usize,
    );
    let single_query_kzg_whole_witness_bytes = estimate_verkle_batch_whole_witness_bytes(
        world,
        &[median_coord],
        48,
        KZG_BLS12_381_OPENING_FLOOR_BYTES as usize,
    );
    let clustered_batch_ipa_whole_witness_bytes = estimate_verkle_batch_whole_witness_bytes(
        world,
        clustered_batch,
        32,
        IPA_256_OPENING_FLOOR_BYTES as usize,
    );
    let dispersed_batch_ipa_whole_witness_bytes = estimate_verkle_batch_whole_witness_bytes(
        world,
        dispersed_batch,
        32,
        IPA_256_OPENING_FLOOR_BYTES as usize,
    );

    VerkleTreeShapeMetricsV1 {
        occupied_keys: keys.len(),
        unique_stems,
        internal_nodes,
        max_internal_depth,
        single_query_internal_commitments,
        single_query_ipa_whole_witness_bytes,
        single_query_kzg_whole_witness_bytes,
        clustered_batch_ipa_whole_witness_bytes,
        dispersed_batch_ipa_whole_witness_bytes,
        single_edit_pedersen_scalar_muls: 2 + single_query_internal_commitments,
        ipa_verify_msm_points: IPA_256_VERIFY_MSM_POINTS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verkle_coordinate_key_matches_independent_python_golden_vectors_and_rejects_malformed() {
        let origin = VerkleCoordinateKeyV1::from_coord(Coord::new(0, 0, 0));
        assert_eq!(origin.as_bytes()[..8], [0u8; 8]);
        assert_eq!(origin.to_coord(), Coord::new(0, 0, 0));
        assert_eq!(
            origin.tag().to_string(),
            "#bb.world.sha256.qwdk4qtaegdp2pbulpkpigsmji5bw7tbl4cnizizsdlco7ahpakq"
        );

        let min_k = VerkleCoordinateKeyV1::from_coord(Coord::new(i64::MIN, i64::MIN, i64::MIN));
        assert_eq!(min_k.as_bytes(), &[0u8; 32]);
        assert_eq!(
            min_k.tag().to_string(),
            "#bb.world.sha256.pc6425mj5b53xurcudk4xtko375gdh5d4xc7zo3amcie5j26u36a"
        );

        let max_k = VerkleCoordinateKeyV1::from_coord(Coord::new(i64::MAX, i64::MAX, i64::MAX));
        assert!(max_k.stem_strictly_below_modulus(&BANDERSNATCH_SCALAR_MODULUS_BE));
        assert!(max_k.stem_strictly_below_modulus(&BLS12_381_SCALAR_MODULUS_BE));
        assert_eq!(
            max_k.tag().to_string(),
            "#bb.world.sha256.krl2q4b3l7emg3i6gzeoqdrzw4znfy5xajzxx73qwweihcpxtuma"
        );

        let mixed = VerkleCoordinateKeyV1::from_coord(Coord::new(-1, 0, 1));
        assert_eq!(
            mixed.tag().to_string(),
            "#bb.world.sha256.qklzr6wcwkwgmnk63frylowslg5sbpkoq2lxktpquzotcv2zqypq"
        );
        let round = VerkleCoordinateKeyV1::from_stem_and_suffix(mixed.stem(), mixed.suffix())
            .expect("stem+suffix round-trip");
        assert_eq!(round, mixed);

        // Negative controls: non-zero prefix, 24-byte unpadded key, 33-byte key
        let mut bad_prefix = *origin.as_bytes();
        bad_prefix[7] = 0x01;
        assert_eq!(
            VerkleCoordinateKeyV1::from_bytes(&bad_prefix),
            Err(VerkleFeasibilityError::NonZeroCoordinatePrefix)
        );
        assert_eq!(
            VerkleCoordinateKeyV1::from_bytes(&origin.as_bytes()[8..]),
            Err(VerkleFeasibilityError::InvalidKeyLength {
                expected: 32,
                actual: 24,
            })
        );
        let mut bad_stem = origin.stem();
        bad_stem[0] = 0x80;
        assert_eq!(
            VerkleCoordinateKeyV1::from_stem_and_suffix(bad_stem, 0),
            Err(VerkleFeasibilityError::NonZeroCoordinatePrefix)
        );

        // Leaf slot encoding: present(0) != absent, present(1) == present_on_v1, big-endian pinned
        let present_zero = VerkleLeafSlotEncodingV1::present_slot(0);
        assert_ne!(present_zero, VerkleLeafSlotEncodingV1::ABSENT_SLOT);
        assert!(VerkleLeafSlotEncodingV1::is_present(&present_zero));
        assert!(!VerkleLeafSlotEncodingV1::is_present(
            &VerkleLeafSlotEncodingV1::ABSENT_SLOT
        ));
        let on_slot = VerkleLeafSlotEncodingV1::present_on_v1();
        assert_eq!(on_slot[15], 0x01);
        assert_eq!(on_slot[31], 0x01);
        assert_eq!(on_slot[16..31], [0u8; 15]);
        assert!(on_slot < BANDERSNATCH_SCALAR_MODULUS_BE);
        let custom_slot =
            VerkleLeafSlotEncodingV1::present_slot(0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10);
        assert_eq!(custom_slot[16], 0x01);
        assert_eq!(custom_slot[31], 0x10);
    }

    #[test]
    fn vector_commitment_profile_descriptors_match_python_golden_tags_and_fail_closed() {
        let bander = VectorCommitmentProfileDescriptorV1::bandersnatch_ipa_256_v1();
        bander.validate().unwrap();
        let bander_bytes = bander.encode_canonical();
        assert_eq!(
            bander_bytes.len(),
            VectorCommitmentProfileDescriptorV1::WIRE_LEN
        );
        assert_eq!(
            VectorCommitmentProfileDescriptorV1::decode_canonical(&bander_bytes).unwrap(),
            bander
        );
        assert_eq!(
            bander.tag().to_string(),
            "#bb.policy.sha256.p74jrvkyumadcmam5hrp5cum4smpcciyng2yxfmsmvqbla2lepfq"
        );
        assert_eq!(bander.profile_id(), bander.tag().digest());

        let kzg = VectorCommitmentProfileDescriptorV1::bls12_381_kzg_256_rejected_v1();
        kzg.validate().unwrap();
        let kzg_bytes = kzg.encode_canonical();
        assert_eq!(
            VectorCommitmentProfileDescriptorV1::decode_canonical(&kzg_bytes).unwrap(),
            kzg
        );
        assert_eq!(
            kzg.tag().to_string(),
            "#bb.policy.sha256.tr7fdslagw7eotvfuoerwjfekutud5zh7jhns6v6zwn3uteey2mq"
        );
        assert_ne!(bander.profile_id(), kzg.profile_id());

        // Negative controls: mismatched trusted_setup flag, invalid arity, zero modulus, truncation
        let mut bad_trust = bander.clone();
        bad_trust.trusted_setup_required_u8 = 1;
        assert_eq!(
            bad_trust.validate(),
            Err(VerkleFeasibilityError::InvalidProfileField {
                field: "trusted_setup_required_u8"
            })
        );
        let mut bad_arity = bander.clone();
        bad_arity.arity_u16 = 3;
        assert_eq!(
            bad_arity.validate(),
            Err(VerkleFeasibilityError::InvalidProfileField { field: "arity_u16" })
        );
        let mut bad_mod = bander.clone();
        bad_mod.scalar_field_modulus_be = [0u8; 32];
        assert_eq!(
            bad_mod.validate(),
            Err(VerkleFeasibilityError::InvalidProfileField {
                field: "scalar_field_modulus_be"
            })
        );
        assert!(
            VectorCommitmentProfileDescriptorV1::decode_canonical(&bander_bytes[..100]).is_err()
        );
    }
}
