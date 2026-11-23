//! `TRANSITION-SPEC-1`: profile-ID-bound total transition specification, disjoint 16-tag
//! operation/disposition bands with exclusive write-domain bitmasks, fail-closed inactive
//! `Program/VM` wall on initial launch, and deterministic byte-to-disposition evaluator (`PS-080`).
//!
//! Resolves `TRANSITION-SPEC-D1`'s unfilled extension interface against a frozen `PROFILE-D1`
//! [`BoundProfileManifestV1`], `BOUNDS-1` [`ProtocolBoundsProfileV1`], and `RESOURCE-1`
//! [`ResourcePolicyV1`] before either authoritative (`EXECUTOR-1`) or independent execution is
//! implemented:
//! - **Disjoint 256-tag allocation:** Base substrate owns `0..=15` (bits `0..=2`); Auth/Nonce owns
//!   `16..=31` (bits `3..=4`); Asset/Resource owns `32..=47` (bits `5..=6`); Validator/Schedule
//!   owns `48..=63` (bits `7..=8`); Gov/Upgrade owns `64..=79` (bit `9`); Program/VM owns `80..=95`
//!   (bit `10` — explicitly **INACTIVE** on Initial Launch and **ACTIVE** on the `CE-43`/`CE-11`
//!   successor profile); unassigned tags `96..=255` reject fail-closed with `UnknownOperationTag`.
//! - **Total byte-to-disposition spec evaluator:** every bounded byte sequence and context yields
//!   exactly one deterministic [`ProfileTransitionEvalReceiptV1`] (either `Admitted` with complete
//!   effects/gas accounting or an atomic rollback rejection with `post_state_root == pre_state_root`).
//! - **Powered semantic mutant verification:** [`verify_receipt_invariants`] and
//!   [`ResolvedProfileTransitionSpecV1::validate`] fail closed on tag collisions, write-domain
//!   overlap, omitted effects, partial-write rollback leaks, wrong version/profile, uncharged work
//!   (`gas == 0`), and inactive-slot reachability.
//!
//! An exact transition specification is not an executor (`EXECUTOR-1`), a commitment/root, profile
//! admission, networking, consensus, or proof of implementation correctness.

use std::collections::BTreeMap;
use std::fmt;

use sha2::{Digest, Sha256};

use crate::bounds_1::ProtocolBoundsProfileV1;
use crate::profile_d1::BoundProfileManifestV1;
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::resource_1::ResourcePolicyV1;
use crate::transition_spec::{
    BASE_RESERVED_DISPOSITION_MAX, BASE_RESERVED_OP_TAG_MAX, BASE_STATE_DOMAINS_MASK,
    BaseTransitionSpecD1, ConflictPolicy, ExtensionSlotDescriptorV1,
};

/// Canonical schema string for a [`ResolvedProfileTransitionSpecV1`] policy object.
pub const TRANSITION_SPEC_1_RESOLVED_SCHEMA: &[u8] =
    b"bitblock/transition-spec-1/resolved-profile-spec/v1";
/// Canonical schema string for a [`ProfileTransitionEvalReceiptV1`] deterministic receipt.
pub const TRANSITION_SPEC_1_EVAL_RECEIPT_SCHEMA: &[u8] =
    b"bitblock/transition-spec-1/spec-eval-receipt/v1";

/// Canonical version identifier for `TRANSITION-SPEC-1`.
pub const TRANSITION_SPEC_1_VERSION: u16 = 1;
/// Exclusive write-domain bitmask for Auth/Nonce band (`bits 3..=4`).
pub const DOMAIN_MASK_AUTH_NONCE: u32 = (1 << 3) | (1 << 4);
/// Exclusive write-domain bitmask for Asset/Resource band (`bits 5..=6`).
pub const DOMAIN_MASK_ASSET_RESOURCE: u32 = (1 << 5) | (1 << 6);
/// Exclusive write-domain bitmask for Validator/Schedule band (`bits 7..=8`).
pub const DOMAIN_MASK_VALIDATOR_SCHEDULE: u32 = (1 << 7) | (1 << 8);
/// Exclusive write-domain bitmask for Governance/Upgrade band (`bit 9`).
pub const DOMAIN_MASK_GOV_UPGRADE: u32 = 1 << 9;
/// Exclusive write-domain bitmask for Program/VM band (`bit 10`, inactive at launch).
pub const DOMAIN_MASK_PROGRAM_VM: u32 = 1 << 10;

const POST_ROOT_PREFIX: &[u8] = b"bb.ts1.post_root.v1";
const EFFECTS_DIGEST_PREFIX: &[u8] = b"bb.ts1.effects.v1";
const OP_EFFECT_PREFIX: &[u8] = b"bb.ts1.op_effect.v1";

/// Fail-closed validation, codec, or semantic-mutant error for `TRANSITION-SPEC-1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransitionSpec1Error {
    /// Spec version did not match [`TRANSITION_SPEC_1_VERSION`].
    WrongVersion { expected: u16, actual: u16 },
    /// Two bands or a band and reserved base range `0..=15` had overlapping operation tags.
    OpTagBandCollision { first_slot: u16, second_slot: u16 },
    /// Two bands or a band and reserved base range `0..=15` had overlapping disposition tags.
    DispositionBandCollision { first_slot: u16, second_slot: u16 },
    /// Band declared empty write mask, base domain write, or overlapping write-domain bits.
    WriteDomainOverlap {
        first_slot: u16,
        second_slot: u16,
        overlap_mask: u32,
    },
    /// Base or extension band declared zero gas cost or overflowing work formula.
    UnchargedWorkFormula { slot_id: u16 },
    /// Band activation state disagreed with the bound `PROFILE-D1` predicate closure.
    ProfileClosureMismatch(&'static str),
    /// Band list was not strictly sorted by unique positive `slot_id`.
    NonCanonicalBandOrder,
    /// Wire buffer was truncated, malformed, had unknown discriminant, or had trailing bytes.
    InvalidWire(&'static str),
    /// Mutant check caught a receipt that omitted an operation effect or mutated its effects digest.
    OmittedEffectDetected,
    /// Mutant check caught a rejected receipt whose `post_state_root` leaked a partial write.
    PartialWriteRollbackViolation,
    /// Mutant check caught a receipt disagreeing with the canonical evaluator output.
    ReceiptEquivalenceMismatch,
}

impl fmt::Display for TransitionSpec1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for TransitionSpec1Error {}

/// Predicate gate code binding an extension band to a `PROFILE-D1` predicate token.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum BandPredicateGateV1 {
    /// Always active in every valid launch and successor profile.
    AlwaysRequired = 0,
    /// Active iff `asset-active` is present in the bound profile manifest.
    AssetActive = 1,
    /// Active iff `program-active` is present in the bound profile manifest (`CE-11`).
    ProgramActive = 2,
}

impl BandPredicateGateV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte predicate-gate discriminant.
    pub const fn from_code(code: u8) -> Result<Self, TransitionSpec1Error> {
        match code {
            0 => Ok(Self::AlwaysRequired),
            1 => Ok(Self::AssetActive),
            2 => Ok(Self::ProgramActive),
            _ => Err(TransitionSpec1Error::InvalidWire(
                "unknown BandPredicateGateV1",
            )),
        }
    }
}

/// One disjoint 16-tag extension band resolved inside [`ResolvedProfileTransitionSpecV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TransitionBandSlotV1 {
    /// Strictly positive unique band slot identifier (`1..=65535`).
    pub slot_id: u16,
    /// Inclusive lower bound of owned operation tags (`>= 16`).
    pub op_tag_min: u8,
    /// Inclusive upper bound of owned operation tags (`>= op_tag_min`).
    pub op_tag_max: u8,
    /// Inclusive lower bound of owned disposition tags (`>= 16`).
    pub disposition_tag_min: u8,
    /// Inclusive upper bound of owned disposition tags (`>= disposition_tag_min`).
    pub disposition_tag_max: u8,
    /// `true` iff this band is reachable in the bound profile; `false` = fail-closed wall.
    pub active: bool,
    /// Deterministic write-contention policy (`RejectConcurrentWrite` or `DeterministicOrder`).
    pub conflict_policy: ConflictPolicy,
    /// Bitmask of state domains permitted for reads.
    pub read_domains_mask: u32,
    /// Exclusive bitmask of state domains permitted for writes (disjoint from base and all peers).
    pub write_domains_mask: u32,
    /// Strictly positive fixed base gas charged per operation in this band.
    pub base_gas: u64,
    /// Strictly positive per-payload-byte gas charged in this band.
    pub per_byte_gas: u64,
    /// Profile predicate gate governing whether `active` may be enabled.
    pub predicate_gate: BandPredicateGateV1,
}

impl TransitionBandSlotV1 {
    /// Fixed canonical big-endian wire length (`33` bytes).
    pub const WIRE_LEN: usize = 33;

    /// Validates range positivity, base disjointness, non-zero work formula, and `TRANSITION-SPEC-D1`
    /// primary owned-bit compatibility.
    pub fn validate(&self) -> Result<(), TransitionSpec1Error> {
        if self.slot_id == 0 {
            return Err(TransitionSpec1Error::NonCanonicalBandOrder);
        }
        if self.op_tag_min <= BASE_RESERVED_OP_TAG_MAX || self.op_tag_min > self.op_tag_max {
            return Err(TransitionSpec1Error::OpTagBandCollision {
                first_slot: 0,
                second_slot: self.slot_id,
            });
        }
        if self.disposition_tag_min <= BASE_RESERVED_DISPOSITION_MAX
            || self.disposition_tag_min > self.disposition_tag_max
        {
            return Err(TransitionSpec1Error::DispositionBandCollision {
                first_slot: 0,
                second_slot: self.slot_id,
            });
        }
        if self.write_domains_mask == 0 || (self.write_domains_mask & BASE_STATE_DOMAINS_MASK) != 0
        {
            return Err(TransitionSpec1Error::WriteDomainOverlap {
                first_slot: 0,
                second_slot: self.slot_id,
                overlap_mask: self.write_domains_mask & BASE_STATE_DOMAINS_MASK,
            });
        }
        if (self.read_domains_mask & self.write_domains_mask) != self.write_domains_mask {
            return Err(TransitionSpec1Error::ProfileClosureMismatch(
                "read_domains_mask must cover write_domains_mask",
            ));
        }
        if self.base_gas == 0
            || self.per_byte_gas == 0
            || self.base_gas.checked_add(self.per_byte_gas).is_none()
        {
            return Err(TransitionSpec1Error::UnchargedWorkFormula {
                slot_id: self.slot_id,
            });
        }
        // Verify projection onto `TRANSITION-SPEC-D1`'s `ExtensionSlotDescriptorV1` for primary bit.
        let primary_bit = self.write_domains_mask.trailing_zeros() as u8;
        let d1_slot = ExtensionSlotDescriptorV1 {
            slot_id: self.slot_id,
            op_tag_min: self.op_tag_min,
            op_tag_max: self.op_tag_max,
            disposition_tag_min: self.disposition_tag_min,
            disposition_tag_max: self.disposition_tag_max,
            owned_domain_bit: primary_bit,
            read_domains_mask: self.read_domains_mask,
            write_domains_mask: 1u32 << primary_bit,
            base_work_units: self.base_gas,
            per_item_work_units: self.per_byte_gas,
            conflict_policy: self.conflict_policy,
        };
        d1_slot.validate().map_err(|_| {
            TransitionSpec1Error::ProfileClosureMismatch("D1 slot invariant failed")
        })?;
        Ok(())
    }

    /// Encodes this band into fixed 33-byte canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], TransitionSpec1Error> {
        self.validate()?;
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.slot_id.to_be_bytes());
        out[2] = self.op_tag_min;
        out[3] = self.op_tag_max;
        out[4] = self.disposition_tag_min;
        out[5] = self.disposition_tag_max;
        out[6] = u8::from(self.active);
        out[7] = self.conflict_policy.code();
        out[8..12].copy_from_slice(&self.read_domains_mask.to_be_bytes());
        out[12..16].copy_from_slice(&self.write_domains_mask.to_be_bytes());
        out[16..24].copy_from_slice(&self.base_gas.to_be_bytes());
        out[24..32].copy_from_slice(&self.per_byte_gas.to_be_bytes());
        out[32] = self.predicate_gate.code();
        Ok(out)
    }

    /// Decodes and validates a canonical 33-byte [`TransitionBandSlotV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, TransitionSpec1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(TransitionSpec1Error::InvalidWire(
                "TransitionBandSlotV1 must be 33 bytes",
            ));
        }
        let active = match bytes[6] {
            0 => false,
            1 => true,
            _ => return Err(TransitionSpec1Error::InvalidWire("invalid active bool")),
        };
        let conflict_policy = ConflictPolicy::from_code(bytes[7])
            .ok_or(TransitionSpec1Error::InvalidWire("unknown ConflictPolicy"))?;
        let slot = Self {
            slot_id: u16::from_be_bytes([bytes[0], bytes[1]]),
            op_tag_min: bytes[2],
            op_tag_max: bytes[3],
            disposition_tag_min: bytes[4],
            disposition_tag_max: bytes[5],
            active,
            conflict_policy,
            read_domains_mask: u32::from_be_bytes(bytes[8..12].try_into().expect("4")),
            write_domains_mask: u32::from_be_bytes(bytes[12..16].try_into().expect("4")),
            base_gas: u64::from_be_bytes(bytes[16..24].try_into().expect("8")),
            per_byte_gas: u64::from_be_bytes(bytes[24..32].try_into().expect("8")),
            predicate_gate: BandPredicateGateV1::from_code(bytes[32])?,
        };
        slot.validate()?;
        Ok(slot)
    }
}

/// Profile-ID-bound total transition specification (`bitblock/transition-spec-1/resolved-profile-spec/v1`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ResolvedProfileTransitionSpecV1 {
    /// Spec version (`1`).
    pub spec_version: u16,
    /// Exact derived `PROFILE-D1` identifier governing this transition specification.
    pub profile_id: [u8; 32],
    /// Digest of `BaseTransitionSpecD1::canonical_base_only().tag()`.
    pub base_contract_digest: [u8; 32],
    /// Digest of the governing `BOUNDS-1` [`ProtocolBoundsProfileV1`].
    pub bounds_profile_digest: [u8; 32],
    /// Digest of the governing `RESOURCE-1` [`ResourcePolicyV1`].
    pub resource_policy_digest: [u8; 32],
    /// Hard maximum operations per atomic batch (`<= ProtocolBoundsProfileV1::max_block_ops`).
    pub max_batch_ops: u32,
    /// Hard maximum gas per atomic batch (`<= ResourcePolicyV1::max_block_gas`).
    pub max_batch_gas: u64,
    /// Strictly positive base gas per base substrate occupancy operation (`0..=15`).
    pub base_op_gas: u64,
    /// Bitmask of state domains owned exclusively by the base substrate (`0b111`).
    pub base_domains_mask: u32,
    /// `true` iff `program-active` is active in the bound profile (`false` on initial launch).
    pub program_slot_active: bool,
    /// Strictly sorted, collision-free extension bands (`16..=95`).
    pub bands: Vec<TransitionBandSlotV1>,
}

impl ResolvedProfileTransitionSpecV1 {
    /// Resolves the canonical `TRANSITION-SPEC-1` specification directly from a verified
    /// `PROFILE-D1` bound manifest, `BOUNDS-1` profile, and `RESOURCE-1` fee policy.
    pub fn resolve_from_profile(
        manifest: &BoundProfileManifestV1,
        bounds: &ProtocolBoundsProfileV1,
        resource: &ResourcePolicyV1,
    ) -> Result<Self, TransitionSpec1Error> {
        manifest.verify().map_err(|_| {
            TransitionSpec1Error::ProfileClosureMismatch("bound profile manifest failed verify")
        })?;
        let bounds_digest = bounds
            .tag()
            .map_err(|_| TransitionSpec1Error::ProfileClosureMismatch("invalid BOUNDS-1 profile"))?
            .digest();
        let resource_digest = resource
            .tag()
            .map_err(|_| {
                TransitionSpec1Error::ProfileClosureMismatch("invalid RESOURCE-1 fee policy")
            })?
            .digest();
        let base_digest = BaseTransitionSpecD1::canonical_base_only()
            .tag()
            .expect("canonical base contract tag")
            .digest();

        let has_pred = |p: &str| {
            manifest
                .candidate
                .activated_predicates
                .iter()
                .any(|x| x == p)
        };
        let asset_active = has_pred("asset-active");
        let program_active = has_pred("program-active");

        let bands = vec![
            TransitionBandSlotV1 {
                slot_id: 1,
                op_tag_min: 16,
                op_tag_max: 31,
                disposition_tag_min: 16,
                disposition_tag_max: 31,
                active: true,
                conflict_policy: ConflictPolicy::DeterministicOrder,
                read_domains_mask: BASE_STATE_DOMAINS_MASK | DOMAIN_MASK_AUTH_NONCE,
                write_domains_mask: DOMAIN_MASK_AUTH_NONCE,
                base_gas: u64::from(resource.gas_per_tx_base),
                per_byte_gas: u64::from(resource.gas_per_tx_byte),
                predicate_gate: BandPredicateGateV1::AlwaysRequired,
            },
            TransitionBandSlotV1 {
                slot_id: 2,
                op_tag_min: 32,
                op_tag_max: 47,
                disposition_tag_min: 32,
                disposition_tag_max: 47,
                active: asset_active,
                conflict_policy: ConflictPolicy::RejectConcurrentWrite,
                read_domains_mask: BASE_STATE_DOMAINS_MASK
                    | DOMAIN_MASK_AUTH_NONCE
                    | DOMAIN_MASK_ASSET_RESOURCE,
                write_domains_mask: DOMAIN_MASK_ASSET_RESOURCE,
                base_gas: 25_000,
                per_byte_gas: u64::from(resource.gas_per_tx_byte),
                predicate_gate: BandPredicateGateV1::AssetActive,
            },
            TransitionBandSlotV1 {
                slot_id: 3,
                op_tag_min: 48,
                op_tag_max: 63,
                disposition_tag_min: 48,
                disposition_tag_max: 63,
                active: true,
                conflict_policy: ConflictPolicy::DeterministicOrder,
                read_domains_mask: BASE_STATE_DOMAINS_MASK
                    | DOMAIN_MASK_AUTH_NONCE
                    | DOMAIN_MASK_ASSET_RESOURCE
                    | DOMAIN_MASK_VALIDATOR_SCHEDULE,
                write_domains_mask: DOMAIN_MASK_VALIDATOR_SCHEDULE,
                base_gas: u64::from(resource.gas_per_ed25519_sig),
                per_byte_gas: u64::from(resource.gas_per_tx_byte),
                predicate_gate: BandPredicateGateV1::AlwaysRequired,
            },
            TransitionBandSlotV1 {
                slot_id: 4,
                op_tag_min: 64,
                op_tag_max: 79,
                disposition_tag_min: 64,
                disposition_tag_max: 79,
                active: true,
                conflict_policy: ConflictPolicy::RejectConcurrentWrite,
                read_domains_mask: BASE_STATE_DOMAINS_MASK
                    | DOMAIN_MASK_AUTH_NONCE
                    | DOMAIN_MASK_ASSET_RESOURCE
                    | DOMAIN_MASK_VALIDATOR_SCHEDULE
                    | DOMAIN_MASK_GOV_UPGRADE,
                write_domains_mask: DOMAIN_MASK_GOV_UPGRADE,
                base_gas: 40_000,
                per_byte_gas: u64::from(resource.gas_per_tx_byte),
                predicate_gate: BandPredicateGateV1::AlwaysRequired,
            },
            TransitionBandSlotV1 {
                slot_id: 5,
                op_tag_min: 80,
                op_tag_max: 95,
                disposition_tag_min: 80,
                disposition_tag_max: 95,
                active: program_active,
                conflict_policy: ConflictPolicy::RejectConcurrentWrite,
                read_domains_mask: BASE_STATE_DOMAINS_MASK
                    | DOMAIN_MASK_AUTH_NONCE
                    | DOMAIN_MASK_ASSET_RESOURCE
                    | DOMAIN_MASK_VALIDATOR_SCHEDULE
                    | DOMAIN_MASK_GOV_UPGRADE
                    | DOMAIN_MASK_PROGRAM_VM,
                write_domains_mask: DOMAIN_MASK_PROGRAM_VM,
                base_gas: 50_000,
                per_byte_gas: u64::from(resource.gas_per_proof_byte),
                predicate_gate: BandPredicateGateV1::ProgramActive,
            },
        ];

        let spec = Self {
            spec_version: TRANSITION_SPEC_1_VERSION,
            profile_id: manifest.profile_id,
            base_contract_digest: base_digest,
            bounds_profile_digest: bounds_digest,
            resource_policy_digest: resource_digest,
            max_batch_ops: bounds.max_block_ops,
            max_batch_gas: resource.max_block_gas,
            base_op_gas: u64::from(resource.gas_per_occupancy_op),
            base_domains_mask: BASE_STATE_DOMAINS_MASK,
            program_slot_active: program_active,
            bands,
        };
        spec.validate()?;
        spec.verify_against_profile(manifest)?;
        Ok(spec)
    }

    /// Verifies that this resolved spec's `profile_id` and predicate-gated bands match `manifest`.
    pub fn verify_against_profile(
        &self,
        manifest: &BoundProfileManifestV1,
    ) -> Result<(), TransitionSpec1Error> {
        manifest.verify().map_err(|_| {
            TransitionSpec1Error::ProfileClosureMismatch("bound manifest verify failed")
        })?;
        if self.profile_id != manifest.profile_id {
            return Err(TransitionSpec1Error::ProfileClosureMismatch(
                "spec profile_id disagrees with BoundProfileManifestV1",
            ));
        }
        let has_pred = |p: &str| {
            manifest
                .candidate
                .activated_predicates
                .iter()
                .any(|x| x == p)
        };
        if self.program_slot_active != has_pred("program-active") {
            return Err(TransitionSpec1Error::ProfileClosureMismatch(
                "program_slot_active disagrees with manifest predicate",
            ));
        }
        for band in &self.bands {
            let expected = match band.predicate_gate {
                BandPredicateGateV1::AlwaysRequired => true,
                BandPredicateGateV1::AssetActive => has_pred("asset-active"),
                BandPredicateGateV1::ProgramActive => has_pred("program-active"),
            };
            if band.active != expected {
                return Err(TransitionSpec1Error::ProfileClosureMismatch(
                    "band active flag disagrees with profile predicate closure",
                ));
            }
        }
        Ok(())
    }

    /// Validates version, non-zero work formulas, strict band order, disjoint op/disposition tags,
    /// and zero write-domain overlap across every declared band.
    pub fn validate(&self) -> Result<(), TransitionSpec1Error> {
        if self.spec_version != TRANSITION_SPEC_1_VERSION {
            return Err(TransitionSpec1Error::WrongVersion {
                expected: TRANSITION_SPEC_1_VERSION,
                actual: self.spec_version,
            });
        }
        if self.profile_id == [0u8; 32] {
            return Err(TransitionSpec1Error::ProfileClosureMismatch(
                "zero profile_id",
            ));
        }
        if self.max_batch_ops == 0 || self.max_batch_gas == 0 || self.base_op_gas == 0 {
            return Err(TransitionSpec1Error::UnchargedWorkFormula { slot_id: 0 });
        }
        if self.base_domains_mask != BASE_STATE_DOMAINS_MASK {
            return Err(TransitionSpec1Error::WriteDomainOverlap {
                first_slot: 0,
                second_slot: 0,
                overlap_mask: self.base_domains_mask ^ BASE_STATE_DOMAINS_MASK,
            });
        }
        if self.bands.is_empty() || self.bands.len() > 32 {
            return Err(TransitionSpec1Error::ProfileClosureMismatch(
                "band count out of range",
            ));
        }
        for band in &self.bands {
            band.validate()?;
        }
        for w in self.bands.windows(2) {
            if w[0].slot_id >= w[1].slot_id {
                return Err(TransitionSpec1Error::NonCanonicalBandOrder);
            }
        }
        for i in 0..self.bands.len() {
            for j in (i + 1)..self.bands.len() {
                let a = &self.bands[i];
                let b = &self.bands[j];
                if a.op_tag_min <= b.op_tag_max && b.op_tag_min <= a.op_tag_max {
                    return Err(TransitionSpec1Error::OpTagBandCollision {
                        first_slot: a.slot_id,
                        second_slot: b.slot_id,
                    });
                }
                if a.disposition_tag_min <= b.disposition_tag_max
                    && b.disposition_tag_min <= a.disposition_tag_max
                {
                    return Err(TransitionSpec1Error::DispositionBandCollision {
                        first_slot: a.slot_id,
                        second_slot: b.slot_id,
                    });
                }
                let overlap = a.write_domains_mask & b.write_domains_mask;
                if overlap != 0 {
                    return Err(TransitionSpec1Error::WriteDomainOverlap {
                        first_slot: a.slot_id,
                        second_slot: b.slot_id,
                        overlap_mask: overlap,
                    });
                }
            }
        }
        if let Some(prog) = self
            .bands
            .iter()
            .find(|b| b.predicate_gate == BandPredicateGateV1::ProgramActive)
            && prog.active != self.program_slot_active
        {
            return Err(TransitionSpec1Error::ProfileClosureMismatch(
                "ProgramActive band disagrees with program_slot_active",
            ));
        }
        Ok(())
    }

    /// Encodes this resolved specification into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, TransitionSpec1Error> {
        self.validate()?;
        let mut out = Vec::with_capacity(157 + self.bands.len() * TransitionBandSlotV1::WIRE_LEN);
        out.extend_from_slice(&self.spec_version.to_be_bytes());
        out.extend_from_slice(&self.profile_id);
        out.extend_from_slice(&self.base_contract_digest);
        out.extend_from_slice(&self.bounds_profile_digest);
        out.extend_from_slice(&self.resource_policy_digest);
        out.extend_from_slice(&self.max_batch_ops.to_be_bytes());
        out.extend_from_slice(&self.max_batch_gas.to_be_bytes());
        out.extend_from_slice(&self.base_op_gas.to_be_bytes());
        out.extend_from_slice(&self.base_domains_mask.to_be_bytes());
        out.push(u8::from(self.program_slot_active));
        out.extend_from_slice(&(self.bands.len() as u16).to_be_bytes());
        for band in &self.bands {
            out.extend_from_slice(&band.encode_canonical()?);
        }
        Ok(out)
    }

    /// Decodes and validates a canonical [`ResolvedProfileTransitionSpecV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, TransitionSpec1Error> {
        const FIXED_HEAD: usize = 2 + 32 * 4 + 4 + 8 + 8 + 4 + 1 + 2;
        if bytes.len() < FIXED_HEAD {
            return Err(TransitionSpec1Error::InvalidWire(
                "truncated ResolvedProfileTransitionSpecV1",
            ));
        }
        let spec_version = u16::from_be_bytes([bytes[0], bytes[1]]);
        if spec_version != TRANSITION_SPEC_1_VERSION {
            return Err(TransitionSpec1Error::WrongVersion {
                expected: TRANSITION_SPEC_1_VERSION,
                actual: spec_version,
            });
        }
        let profile_id: [u8; 32] = bytes[2..34].try_into().expect("32");
        let base_contract_digest: [u8; 32] = bytes[34..66].try_into().expect("32");
        let bounds_profile_digest: [u8; 32] = bytes[66..98].try_into().expect("32");
        let resource_policy_digest: [u8; 32] = bytes[98..130].try_into().expect("32");
        let max_batch_ops = u32::from_be_bytes(bytes[130..134].try_into().expect("4"));
        let max_batch_gas = u64::from_be_bytes(bytes[134..142].try_into().expect("8"));
        let base_op_gas = u64::from_be_bytes(bytes[142..150].try_into().expect("8"));
        let base_domains_mask = u32::from_be_bytes(bytes[150..154].try_into().expect("4"));
        let program_slot_active = match bytes[154] {
            0 => false,
            1 => true,
            _ => {
                return Err(TransitionSpec1Error::InvalidWire(
                    "invalid program_slot_active bool",
                ));
            }
        };
        let band_count = u16::from_be_bytes([bytes[155], bytes[156]]) as usize;
        let expected_len = FIXED_HEAD + band_count * TransitionBandSlotV1::WIRE_LEN;
        if bytes.len() != expected_len {
            return Err(TransitionSpec1Error::InvalidWire(
                "ResolvedProfileTransitionSpecV1 length mismatch",
            ));
        }
        let mut bands = Vec::with_capacity(band_count);
        for i in 0..band_count {
            let off = FIXED_HEAD + i * TransitionBandSlotV1::WIRE_LEN;
            bands.push(TransitionBandSlotV1::decode_canonical(
                &bytes[off..off + TransitionBandSlotV1::WIRE_LEN],
            )?);
        }
        let spec = Self {
            spec_version,
            profile_id,
            base_contract_digest,
            bounds_profile_digest,
            resource_policy_digest,
            max_batch_ops,
            max_batch_gas,
            base_op_gas,
            base_domains_mask,
            program_slot_active,
            bands,
        };
        spec.validate()?;
        Ok(spec)
    }

    /// Computes the canonical `Policy` `ContentTag` for this resolved specification.
    pub fn tag(&self) -> Result<ContentTag, TransitionSpec1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            TRANSITION_SPEC_1_RESOLVED_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and bounded length"))
    }
}

/// One typed framed operation inside a [`ProfileTransitionBatchV1`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProfileTransitionOpV1 {
    /// Operation tag (`0..=15` base, `16..=95` declared bands, `96..=255` rejected unassigned).
    pub op_tag: u8,
    /// Fixed 8-byte domain-scoped target key used for deterministic conflict checks.
    pub target_key: [u8; 8],
    /// Bounded operation operand bytes.
    pub payload: Vec<u8>,
}

/// Canonical batch submitted to [`evaluate_profile_transition_spec`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProfileTransitionBatchV1 {
    /// Spec version (`1`).
    pub spec_version: u16,
    /// Target `profile_id` that this batch is authorized under.
    pub profile_id: [u8; 32],
    /// Ledger height of evaluation.
    pub block_height: u64,
    /// Ordered sequence of operations executed atomically.
    pub ops: Vec<ProfileTransitionOpV1>,
}

impl ProfileTransitionBatchV1 {
    /// Encodes this batch into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(44 + self.ops.len() * 16);
        out.extend_from_slice(&self.spec_version.to_be_bytes());
        out.extend_from_slice(&self.profile_id);
        out.extend_from_slice(&self.block_height.to_be_bytes());
        out.extend_from_slice(&(self.ops.len() as u16).to_be_bytes());
        for op in &self.ops {
            out.push(op.op_tag);
            out.extend_from_slice(&op.target_key);
            out.extend_from_slice(&(op.payload.len() as u16).to_be_bytes());
            out.extend_from_slice(&op.payload);
        }
        out
    }
}

/// Evaluation context passed to [`evaluate_profile_transition_spec`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EvalSpecContextV1 {
    /// Expected active `profile_id` in the execution environment.
    pub expected_profile_id: [u8; 32],
    /// Expected ledger height.
    pub block_height: u64,
    /// Pre-transition 32-byte state root digest.
    pub pre_state_root: [u8; 32],
}

/// Complete deterministic batch-level disposition emitted by [`evaluate_profile_transition_spec`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum TransitionEvalDispositionV1 {
    /// All operations validated, metered, staged, and committed atomically.
    Admitted = 0,
    /// Raw byte sequence had truncated framing, invalid length, or trailing bytes.
    RejectedMalformedFrame = 1,
    /// Batch `spec_version`, `profile_id`, or `block_height` disagreed with spec/context.
    RejectedWrongVersionOrProfile = 2,
    /// Batch operation count exceeded `spec.max_batch_ops` before execution.
    RejectedOverLimit = 3,
    /// Operation tag fell in unassigned `96..=255` range (atomic rollback).
    RejectedUnknownOperationTag = 4,
    /// Operation tag targeted a declared but INACTIVE extension slot (e.g. `80..=95` on launch).
    RejectedInactiveExtensionSlot = 5,
    /// Two operations collided on the same key inside a `RejectConcurrentWrite` band.
    RejectedConflict = 6,
    /// Cumulative deterministic gas exceeded `spec.max_batch_gas` (atomic rollback).
    RejectedGasLimitExceeded = 7,
}

impl TransitionEvalDispositionV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte evaluation disposition discriminant.
    pub const fn from_code(code: u8) -> Result<Self, TransitionSpec1Error> {
        match code {
            0 => Ok(Self::Admitted),
            1 => Ok(Self::RejectedMalformedFrame),
            2 => Ok(Self::RejectedWrongVersionOrProfile),
            3 => Ok(Self::RejectedOverLimit),
            4 => Ok(Self::RejectedUnknownOperationTag),
            5 => Ok(Self::RejectedInactiveExtensionSlot),
            6 => Ok(Self::RejectedConflict),
            7 => Ok(Self::RejectedGasLimitExceeded),
            _ => Err(TransitionSpec1Error::InvalidWire(
                "unknown TransitionEvalDispositionV1",
            )),
        }
    }
}

/// Deterministic evaluation receipt (`bitblock/transition-spec-1/spec-eval-receipt/v1`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProfileTransitionEvalReceiptV1 {
    /// Receipt schema version (`1`).
    pub receipt_version: u16,
    /// Digest of the governing [`ResolvedProfileTransitionSpecV1`] policy tag.
    pub spec_tag_digest: [u8; 32],
    /// Bound `profile_id`.
    pub profile_id: [u8; 32],
    /// Evaluation height.
    pub block_height: u64,
    /// Batch-level deterministic disposition.
    pub disposition: TransitionEvalDispositionV1,
    /// Pre-transition state root digest.
    pub pre_state_root: [u8; 32],
    /// Post-transition state root digest (`== pre_state_root` on every rejection).
    pub post_state_root: [u8; 32],
    /// Total deterministic gas charged (`> 0` for any non-empty admitted batch).
    pub gas_charged: u64,
    /// Number of committed atomic operation effects (`0` on every rejection).
    pub effects_count: u32,
    /// Deterministic SHA-256 digest over ordered effects (`[0u8; 32]` on rejection).
    pub effects_digest: [u8; 32],
    /// Ordered per-operation disposition lane (empty on rejection).
    pub op_dispositions: Vec<u8>,
}

impl ProfileTransitionEvalReceiptV1 {
    /// Encodes this evaluation receipt into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, TransitionSpec1Error> {
        if self.receipt_version != 1 {
            return Err(TransitionSpec1Error::InvalidWire(
                "receipt_version must equal 1",
            ));
        }
        let mut out = Vec::with_capacity(185 + self.op_dispositions.len());
        out.extend_from_slice(&self.receipt_version.to_be_bytes());
        out.extend_from_slice(&self.spec_tag_digest);
        out.extend_from_slice(&self.profile_id);
        out.extend_from_slice(&self.block_height.to_be_bytes());
        out.push(self.disposition.code());
        out.extend_from_slice(&self.pre_state_root);
        out.extend_from_slice(&self.post_state_root);
        out.extend_from_slice(&self.gas_charged.to_be_bytes());
        out.extend_from_slice(&self.effects_count.to_be_bytes());
        out.extend_from_slice(&self.effects_digest);
        out.extend_from_slice(&(self.op_dispositions.len() as u16).to_be_bytes());
        out.extend_from_slice(&self.op_dispositions);
        Ok(out)
    }

    /// Decodes and validates a canonical [`ProfileTransitionEvalReceiptV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, TransitionSpec1Error> {
        const FIXED_HEAD: usize = 2 + 32 + 32 + 8 + 1 + 32 + 32 + 8 + 4 + 32 + 2;
        if bytes.len() < FIXED_HEAD {
            return Err(TransitionSpec1Error::InvalidWire(
                "truncated ProfileTransitionEvalReceiptV1",
            ));
        }
        let receipt_version = u16::from_be_bytes([bytes[0], bytes[1]]);
        if receipt_version != 1 {
            return Err(TransitionSpec1Error::InvalidWire(
                "receipt_version must equal 1",
            ));
        }
        let disp_len = u16::from_be_bytes([bytes[183], bytes[184]]) as usize;
        if bytes.len() != FIXED_HEAD + disp_len {
            return Err(TransitionSpec1Error::InvalidWire(
                "ProfileTransitionEvalReceiptV1 length mismatch",
            ));
        }
        Ok(Self {
            receipt_version,
            spec_tag_digest: bytes[2..34].try_into().expect("32"),
            profile_id: bytes[34..66].try_into().expect("32"),
            block_height: u64::from_be_bytes(bytes[66..74].try_into().expect("8")),
            disposition: TransitionEvalDispositionV1::from_code(bytes[74])?,
            pre_state_root: bytes[75..107].try_into().expect("32"),
            post_state_root: bytes[107..139].try_into().expect("32"),
            gas_charged: u64::from_be_bytes(bytes[139..147].try_into().expect("8")),
            effects_count: u32::from_be_bytes(bytes[147..151].try_into().expect("4")),
            effects_digest: bytes[151..183].try_into().expect("32"),
            op_dispositions: bytes[FIXED_HEAD..].to_vec(),
        })
    }

    /// Computes the canonical `Receipt` `ContentTag` for this evaluation receipt.
    pub fn tag(&self) -> Result<ContentTag, TransitionSpec1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            TRANSITION_SPEC_1_EVAL_RECEIPT_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and bounded length"))
    }
}

/// Total deterministic byte-to-disposition transition spec evaluator.
///
/// Every bounded byte slice `raw_batch_bytes` under `(spec, ctx)` yields exactly one deterministic
/// [`ProfileTransitionEvalReceiptV1`]. Every rejection preserves `post_state_root == pre_state_root`
/// and `effects_count == 0` atomically.
pub fn evaluate_profile_transition_spec(
    spec: &ResolvedProfileTransitionSpecV1,
    ctx: &EvalSpecContextV1,
    raw_batch_bytes: &[u8],
) -> Result<ProfileTransitionEvalReceiptV1, TransitionSpec1Error> {
    spec.validate()?;
    let spec_tag_digest = spec.tag()?.digest();
    let reject = |disposition: TransitionEvalDispositionV1| ProfileTransitionEvalReceiptV1 {
        receipt_version: 1,
        spec_tag_digest,
        profile_id: spec.profile_id,
        block_height: ctx.block_height,
        disposition,
        pre_state_root: ctx.pre_state_root,
        post_state_root: ctx.pre_state_root,
        gas_charged: 0,
        effects_count: 0,
        effects_digest: [0u8; 32],
        op_dispositions: Vec::new(),
    };

    if raw_batch_bytes.len() < 44 {
        return Ok(reject(TransitionEvalDispositionV1::RejectedMalformedFrame));
    }
    let batch_ver = u16::from_be_bytes([raw_batch_bytes[0], raw_batch_bytes[1]]);
    let batch_pid: [u8; 32] = raw_batch_bytes[2..34].try_into().expect("32");
    let batch_height = u64::from_be_bytes(raw_batch_bytes[34..42].try_into().expect("8"));
    let op_count = u16::from_be_bytes([raw_batch_bytes[42], raw_batch_bytes[43]]) as usize;
    if batch_ver != spec.spec_version
        || batch_pid != spec.profile_id
        || spec.profile_id != ctx.expected_profile_id
        || batch_height != ctx.block_height
    {
        return Ok(reject(
            TransitionEvalDispositionV1::RejectedWrongVersionOrProfile,
        ));
    }
    if op_count > spec.max_batch_ops as usize {
        return Ok(reject(TransitionEvalDispositionV1::RejectedOverLimit));
    }

    let mut cur = 44usize;
    let mut parsed_ops = Vec::with_capacity(op_count);
    for _ in 0..op_count {
        if raw_batch_bytes.len().saturating_sub(cur) < 11 {
            return Ok(reject(TransitionEvalDispositionV1::RejectedMalformedFrame));
        }
        let op_tag = raw_batch_bytes[cur];
        let target_key: [u8; 8] = raw_batch_bytes[cur + 1..cur + 9].try_into().expect("8");
        let payload_len =
            u16::from_be_bytes([raw_batch_bytes[cur + 9], raw_batch_bytes[cur + 10]]) as usize;
        cur += 11;
        if raw_batch_bytes.len().saturating_sub(cur) < payload_len {
            return Ok(reject(TransitionEvalDispositionV1::RejectedMalformedFrame));
        }
        parsed_ops.push((op_tag, target_key, &raw_batch_bytes[cur..cur + payload_len]));
        cur += payload_len;
    }
    if cur != raw_batch_bytes.len() {
        return Ok(reject(TransitionEvalDispositionV1::RejectedMalformedFrame));
    }

    let mut staged_writes: BTreeMap<(u32, [u8; 8]), [u8; 32]> = BTreeMap::new();
    let mut op_dispositions = Vec::with_capacity(op_count);
    let mut total_gas: u64 = 0;

    for (op_tag, target_key, payload) in parsed_ops {
        let (write_mask, op_gas, disp_code, reject_concurrent) =
            if op_tag <= BASE_RESERVED_OP_TAG_MAX {
                let Some(byte_gas) = (payload.len() as u64).checked_mul(16) else {
                    return Ok(reject(
                        TransitionEvalDispositionV1::RejectedGasLimitExceeded,
                    ));
                };
                let Some(g) = spec.base_op_gas.checked_add(byte_gas) else {
                    return Ok(reject(
                        TransitionEvalDispositionV1::RejectedGasLimitExceeded,
                    ));
                };
                (BASE_STATE_DOMAINS_MASK, g, 0u8, false)
            } else {
                let Some(band) = spec
                    .bands
                    .iter()
                    .find(|b| op_tag >= b.op_tag_min && op_tag <= b.op_tag_max)
                else {
                    return Ok(reject(
                        TransitionEvalDispositionV1::RejectedUnknownOperationTag,
                    ));
                };
                if !band.active {
                    return Ok(reject(
                        TransitionEvalDispositionV1::RejectedInactiveExtensionSlot,
                    ));
                }
                let Some(byte_gas) = (payload.len() as u64).checked_mul(band.per_byte_gas) else {
                    return Ok(reject(
                        TransitionEvalDispositionV1::RejectedGasLimitExceeded,
                    ));
                };
                let Some(g) = band.base_gas.checked_add(byte_gas) else {
                    return Ok(reject(
                        TransitionEvalDispositionV1::RejectedGasLimitExceeded,
                    ));
                };
                (
                    band.write_domains_mask,
                    g,
                    band.disposition_tag_min,
                    band.conflict_policy == ConflictPolicy::RejectConcurrentWrite,
                )
            };

        let Some(next_gas) = total_gas.checked_add(op_gas) else {
            return Ok(reject(
                TransitionEvalDispositionV1::RejectedGasLimitExceeded,
            ));
        };
        if next_gas > spec.max_batch_gas {
            return Ok(reject(
                TransitionEvalDispositionV1::RejectedGasLimitExceeded,
            ));
        }
        total_gas = next_gas;

        let write_key = (write_mask, target_key);
        if reject_concurrent && staged_writes.contains_key(&write_key) {
            return Ok(reject(TransitionEvalDispositionV1::RejectedConflict));
        }

        let mut eh = Sha256::new();
        eh.update(OP_EFFECT_PREFIX);
        eh.update([op_tag]);
        eh.update(target_key);
        eh.update((payload.len() as u16).to_be_bytes());
        eh.update(payload);
        let eff_hash: [u8; 32] = eh.finalize().into();
        staged_writes.insert(write_key, eff_hash);
        op_dispositions.push(disp_code);
    }

    let mut eff_h = Sha256::new();
    eff_h.update(EFFECTS_DIGEST_PREFIX);
    eff_h.update((op_dispositions.len() as u32).to_be_bytes());
    eff_h.update(&op_dispositions);
    for ((mask, key), val) in &staged_writes {
        eff_h.update(mask.to_be_bytes());
        eff_h.update(key);
        eff_h.update(val);
    }
    let effects_digest: [u8; 32] = eff_h.finalize().into();

    let mut root_h = Sha256::new();
    root_h.update(POST_ROOT_PREFIX);
    root_h.update(ctx.pre_state_root);
    root_h.update(effects_digest);
    let post_state_root: [u8; 32] = root_h.finalize().into();

    Ok(ProfileTransitionEvalReceiptV1 {
        receipt_version: 1,
        spec_tag_digest,
        profile_id: spec.profile_id,
        block_height: ctx.block_height,
        disposition: TransitionEvalDispositionV1::Admitted,
        pre_state_root: ctx.pre_state_root,
        post_state_root,
        gas_charged: total_gas,
        effects_count: op_dispositions.len() as u32,
        effects_digest,
        op_dispositions,
    })
}

/// Verifies a candidate evaluation receipt against the exact specification evaluator, explicitly
/// classifying omitted-effect and partial-write rollback semantic mutants fail-closed.
pub fn verify_receipt_invariants(
    spec: &ResolvedProfileTransitionSpecV1,
    ctx: &EvalSpecContextV1,
    raw_batch_bytes: &[u8],
    candidate_receipt: &ProfileTransitionEvalReceiptV1,
) -> Result<(), TransitionSpec1Error> {
    let expected = evaluate_profile_transition_spec(spec, ctx, raw_batch_bytes)?;
    if candidate_receipt.disposition != TransitionEvalDispositionV1::Admitted
        && (candidate_receipt.post_state_root != candidate_receipt.pre_state_root
            || candidate_receipt.effects_count != 0
            || !candidate_receipt.op_dispositions.is_empty())
    {
        return Err(TransitionSpec1Error::PartialWriteRollbackViolation);
    }
    if expected.disposition == TransitionEvalDispositionV1::Admitted
        && (candidate_receipt.effects_count < expected.effects_count
            || candidate_receipt.op_dispositions.len() < expected.op_dispositions.len()
            || candidate_receipt.effects_digest != expected.effects_digest)
    {
        return Err(TransitionSpec1Error::OmittedEffectDetected);
    }
    if candidate_receipt != &expected {
        return Err(TransitionSpec1Error::ReceiptEquivalenceMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset_d1::AssetPolicyDescriptorV1;
    use crate::bounds_1::{
        DEFAULT_MAX_ALLOC_BYTES_PER_BLOCK, DEFAULT_MAX_BLOCK_BYTES, DEFAULT_MAX_BLOCK_OPS,
        DEFAULT_MAX_BLOCK_SIGNATURES, DEFAULT_MAX_BLOCK_TXS, DEFAULT_MAX_HASHES_PER_BLOCK,
        DEFAULT_MAX_PEER_INFLIGHT_BYTES, DEFAULT_MAX_PEER_REQUESTS_PER_SEC,
        DEFAULT_MAX_PROOF_BYTES, DEFAULT_MAX_PROOF_DEPTH, DEFAULT_MAX_STATE_WRITES_PER_BLOCK,
        DEFAULT_MAX_TX_BYTES, DEFAULT_MAX_VERIFY_BUDGET_MS,
    };
    use crate::profile_d1::CandidateProfileManifestV1;

    fn test_bounds_and_resource() -> (ProtocolBoundsProfileV1, ResourcePolicyV1) {
        let bounds = ProtocolBoundsProfileV1 {
            schema_version: 1,
            threat_model_digest: [0xA1; 32],
            local_bounds_digest: [0xA2; 32],
            max_block_bytes: DEFAULT_MAX_BLOCK_BYTES,
            max_tx_bytes: DEFAULT_MAX_TX_BYTES,
            max_block_txs: DEFAULT_MAX_BLOCK_TXS,
            max_block_signatures: DEFAULT_MAX_BLOCK_SIGNATURES,
            max_block_ops: DEFAULT_MAX_BLOCK_OPS,
            max_state_writes_per_block: DEFAULT_MAX_STATE_WRITES_PER_BLOCK,
            max_proof_bytes: DEFAULT_MAX_PROOF_BYTES,
            max_proof_depth: DEFAULT_MAX_PROOF_DEPTH,
            max_hashes_per_block: DEFAULT_MAX_HASHES_PER_BLOCK,
            max_alloc_bytes_per_block: DEFAULT_MAX_ALLOC_BYTES_PER_BLOCK,
            max_verify_budget_ms: DEFAULT_MAX_VERIFY_BUDGET_MS,
            max_peer_inflight_bytes: DEFAULT_MAX_PEER_INFLIGHT_BYTES,
            max_peer_requests_per_sec: DEFAULT_MAX_PEER_REQUESTS_PER_SEC,
        };
        let resource = ResourcePolicyV1::v1_default(
            &bounds,
            &AssetPolicyDescriptorV1::v1_dual_domain_default(),
        )
        .unwrap();
        (bounds, resource)
    }

    #[test]
    fn resolved_launch_and_successor_specs_roundtrip_and_gate_program_wall() {
        let (bounds, resource) = test_bounds_and_resource();
        let launch_cand = CandidateProfileManifestV1::canonical_launch_v1(
            [0x11; 32],
            [0x22; 32],
            [0x33; 32],
            bounds.tag().unwrap().digest(),
            [0x44; 32],
            resource.tag().unwrap().digest(),
        )
        .unwrap();
        let launch = BoundProfileManifestV1::seal(launch_cand).unwrap();
        let succ_cand =
            CandidateProfileManifestV1::canonical_successor_program_v1(&launch, [0x55; 32])
                .unwrap();
        let succ = BoundProfileManifestV1::seal(succ_cand).unwrap();

        let launch_spec =
            ResolvedProfileTransitionSpecV1::resolve_from_profile(&launch, &bounds, &resource)
                .unwrap();
        let succ_spec =
            ResolvedProfileTransitionSpecV1::resolve_from_profile(&succ, &bounds, &resource)
                .unwrap();
        assert!(!launch_spec.program_slot_active);
        assert!(succ_spec.program_slot_active);
        assert_eq!(
            ResolvedProfileTransitionSpecV1::decode_canonical(
                &launch_spec.encode_canonical().unwrap()
            )
            .unwrap(),
            launch_spec
        );

        let prog_batch = ProfileTransitionBatchV1 {
            spec_version: 1,
            profile_id: launch.profile_id,
            block_height: 10,
            ops: vec![ProfileTransitionOpV1 {
                op_tag: 80,
                target_key: [1; 8],
                payload: vec![0xAA],
            }],
        };
        let ctx_l = EvalSpecContextV1 {
            expected_profile_id: launch.profile_id,
            block_height: 10,
            pre_state_root: [0x01; 32],
        };
        let r_l =
            evaluate_profile_transition_spec(&launch_spec, &ctx_l, &prog_batch.encode_canonical())
                .unwrap();
        assert_eq!(
            r_l.disposition,
            TransitionEvalDispositionV1::RejectedInactiveExtensionSlot
        );
        assert_eq!(r_l.post_state_root, ctx_l.pre_state_root);
        let rcpt_enc = r_l.encode_canonical().unwrap();
        assert_eq!(
            ProfileTransitionEvalReceiptV1::decode_canonical(&rcpt_enc).unwrap(),
            r_l
        );
    }
}
