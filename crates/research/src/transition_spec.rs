//! `TRANSITION-SPEC-D1`: base occupancy/episode transition contract and typed extension interface.
//!
//! Freezes the canonical substrate transition semantics (occupancy + mutable episode provenance +
//! immutable origin bedrock) and the typed extension slot descriptor/registry interface consumed
//! by later `PROFILE-SCOPE-D0` and `TRANSITION-SPEC-1` phases.
//!
//! Deliberately leaves nonce, fee, governance, validator, asset, and visual-program policy slots
//! unfilled: every extension slot must declare disjoint operation/disposition tag ranges, explicit
//! state-domain read/write masks with unique write ownership, a non-zero deterministic work formula,
//! and an explicit conflict policy.

use std::fmt;

use crate::coordinate_episode::OccupancyEpisodeLedger;
use crate::history::{History, NoOpReason, TransitionEffect, world_tag};
use crate::program::{EDIT_CANONICAL_LEN, Edit};
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::world::{BEDROCK_COORD, Coord, World};

/// Canonical schema string for a `BaseTransitionSpecD1` contract descriptor.
pub const TRANSITION_SPEC_D1_CONTRACT_SCHEMA: &[u8] =
    b"bitblock/transition-spec-d1/base-contract/v1";
/// Canonical schema string for an `ExtensionSlotDescriptorV1` interface slot.
pub const TRANSITION_SPEC_D1_EXTENSION_SLOT_SCHEMA: &[u8] =
    b"bitblock/transition-spec-d1/extension-slot/v1";
/// Canonical schema string for a `BaseTransitionReceiptV1` deterministic transition receipt.
pub const TRANSITION_SPEC_D1_BASE_RECEIPT_SCHEMA: &[u8] =
    b"bitblock/transition-spec-d1/base-receipt/v1";

/// Canonical version identifier for the `TRANSITION-SPEC-D1` base transition contract.
pub const BASE_TRANSITION_SPEC_VERSION: u16 = 1;
/// Highest operation tag reserved exclusively for base substrate transitions (`0..=15`).
pub const BASE_RESERVED_OP_TAG_MAX: u8 = 15;
/// Highest disposition tag reserved exclusively for base substrate dispositions (`0..=15`).
pub const BASE_RESERVED_DISPOSITION_MAX: u8 = 15;
/// Maximum number of base occupancy operations admitted in a single atomic batch.
pub const MAX_BASE_BATCH_OPS: usize = 65_536;
/// Fixed canonical wire byte length of an [`ExtensionSlotDescriptorV1`] (`32` bytes).
pub const EXTENSION_SLOT_WIRE_LEN: usize = 32;

/// State-domain ownership bit for binary world occupancy (`World`).
pub const DOMAIN_BIT_WORLD_OCCUPANCY: u32 = 1 << 0;
/// State-domain ownership bit for the live mutable occupancy-episode map.
pub const DOMAIN_BIT_ACTIVE_EPISODE: u32 = 1 << 1;
/// State-domain ownership bit for retained closed/burn episode provenance history.
pub const DOMAIN_BIT_EPISODE_HISTORY: u32 = 1 << 2;
/// Combined bitmask of all three state domains owned exclusively by the base substrate.
pub const BASE_STATE_DOMAINS_MASK: u32 =
    DOMAIN_BIT_WORLD_OCCUPANCY | DOMAIN_BIT_ACTIVE_EPISODE | DOMAIN_BIT_EPISODE_HISTORY;

/// Deterministic conflict resolution policy declared by an extension slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ConflictPolicy {
    /// Reject any batch where two operations contend on the same owned domain key.
    RejectConcurrentWrite = 0,
    /// Resolve same-key writes strictly in canonical batch index order.
    DeterministicOrder = 1,
}

impl ConflictPolicy {
    /// Returns the canonical 1-byte wire discriminant for this policy.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte conflict-policy discriminant.
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::RejectConcurrentWrite),
            1 => Some(Self::DeterministicOrder),
            _ => None,
        }
    }
}

/// Complete ordered base disposition lane for each submitted base occupancy edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum BaseDispositionV1 {
    /// Coordinate transitioned from off to on, minting one fresh occupancy episode.
    Mint = 0,
    /// Mutable coordinate transitioned from on to off, consuming its active episode.
    Burn = 1,
    /// Edit targeted immutable origin bedrock `(0, 0, 0)` and left occupancy unchanged.
    NoOpBedrock = 2,
    /// `On` targeted an already-occupied mutable coordinate and left occupancy unchanged.
    NoOpAlreadyOn = 3,
    /// `Off` targeted an already-off coordinate and left occupancy unchanged.
    NoOpAlreadyOff = 4,
    /// Operation was refused before mutation due to batch or work limits.
    RejectedOverLimit = 5,
    /// Operation was refused before mutation due to malformed canonical encoding.
    RejectedMalformed = 6,
}

impl BaseDispositionV1 {
    /// Returns the canonical 1-byte wire discriminant for this base disposition.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte base disposition discriminant.
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Mint),
            1 => Some(Self::Burn),
            2 => Some(Self::NoOpBedrock),
            3 => Some(Self::NoOpAlreadyOn),
            4 => Some(Self::NoOpAlreadyOff),
            5 => Some(Self::RejectedOverLimit),
            6 => Some(Self::RejectedMalformed),
            _ => None,
        }
    }
}

/// Fail-closed validation, codec, or execution error for `TRANSITION-SPEC-D1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransitionSpecError {
    /// Unsupported or mismatched transition specification version.
    WrongVersion { expected: u16, actual: u16 },
    /// Extension slot identifier `0` is reserved and forbidden.
    ReservedSlotId,
    /// Extension operation tag range overlaps reserved base tags `0..=15` or is inverted.
    InvalidOpTagRange { min: u8, max: u8 },
    /// Extension disposition tag range overlaps reserved base dispositions `0..=15` or is inverted.
    InvalidDispositionRange { min: u8, max: u8 },
    /// Extension owned domain bit lies in reserved base bits `0..=2` or exceeds `31`.
    InvalidOwnedDomainBit { bit: u8 },
    /// Extension attempted to declare write access to a base domain or an unowned domain bit.
    UndeclaredOrForbiddenWriteDomain {
        owned_bit: u8,
        write_domains_mask: u32,
    },
    /// Extension declared zero base work or overflowing work arithmetic.
    InvalidWorkFormula { base: u64, per_item: u64 },
    /// Unknown conflict policy discriminant byte.
    InvalidConflictPolicy { code: u8 },
    /// Extension registry slots are not strictly ascending by `slot_id`.
    NonCanonicalSlotOrder { prev_slot: u16, next_slot: u16 },
    /// Two extension slots declare overlapping operation tag ranges.
    OpTagCollision { first_slot: u16, second_slot: u16 },
    /// Two extension slots declare overlapping disposition tag ranges.
    DispositionTagCollision { first_slot: u16, second_slot: u16 },
    /// Two extension slots claim ownership of the same state domain bit.
    DomainOwnershipCollision {
        first_slot: u16,
        second_slot: u16,
        domain_bit: u8,
    },
    /// Batch operation count exceeded the contract's `max_batch_ops` ceiling before allocation.
    BatchOverLimit { count: usize, limit: usize },
    /// Arithmetic overflow computing deterministic batch work cost.
    WorkCostOverflow,
    /// Payload length is truncated, overlong, or does not match the canonical grammar.
    MalformedPayload { reason: &'static str },
    /// Unknown base disposition discriminant byte in a receipt payload.
    InvalidDispositionCode { code: u8 },
    /// Staged base transition failed ledger bijection or history consistency verification.
    AtomicInvariantFailure { reason: &'static str },
}

impl fmt::Display for TransitionSpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::WrongVersion { expected, actual } => {
                write!(
                    f,
                    "wrong transition spec version {actual} (expected {expected})"
                )
            }
            Self::ReservedSlotId => write!(f, "extension slot_id 0 is reserved"),
            Self::InvalidOpTagRange { min, max } => {
                write!(f, "invalid extension op tag range {min}..={max}")
            }
            Self::InvalidDispositionRange { min, max } => {
                write!(f, "invalid extension disposition tag range {min}..={max}")
            }
            Self::InvalidOwnedDomainBit { bit } => {
                write!(
                    f,
                    "invalid extension owned domain bit {bit} (must be 3..=31)"
                )
            }
            Self::UndeclaredOrForbiddenWriteDomain {
                owned_bit,
                write_domains_mask,
            } => write!(
                f,
                "extension write mask {write_domains_mask:#010x} accesses unowned or base domain (owned bit {owned_bit})"
            ),
            Self::InvalidWorkFormula { base, per_item } => {
                write!(f, "invalid work formula base={base}, per_item={per_item}")
            }
            Self::InvalidConflictPolicy { code } => {
                write!(f, "unknown conflict policy code {code}")
            }
            Self::NonCanonicalSlotOrder {
                prev_slot,
                next_slot,
            } => write!(
                f,
                "extension slots not strictly ascending: {prev_slot} followed by {next_slot}"
            ),
            Self::OpTagCollision {
                first_slot,
                second_slot,
            } => write!(
                f,
                "extension op-tag collision between slots {first_slot} and {second_slot}"
            ),
            Self::DispositionTagCollision {
                first_slot,
                second_slot,
            } => write!(
                f,
                "extension disposition-tag collision between slots {first_slot} and {second_slot}"
            ),
            Self::DomainOwnershipCollision {
                first_slot,
                second_slot,
                domain_bit,
            } => write!(
                f,
                "extension state-domain collision on bit {domain_bit} between slots {first_slot} and {second_slot}"
            ),
            Self::BatchOverLimit { count, limit } => {
                write!(f, "base batch op count {count} exceeds limit {limit}")
            }
            Self::WorkCostOverflow => write!(f, "deterministic work cost overflowed u64"),
            Self::MalformedPayload { reason } => write!(f, "malformed payload: {reason}"),
            Self::InvalidDispositionCode { code } => {
                write!(f, "unknown base disposition code {code}")
            }
            Self::AtomicInvariantFailure { reason } => {
                write!(f, "atomic base transition invariant failed: {reason}")
            }
        }
    }
}

impl std::error::Error for TransitionSpecError {}

/// Typed extension interface slot declared to `TRANSITION-SPEC-D1`.
///
/// Reserves disjoint operation tags, disposition tags, explicit read/write state-domain masks,
/// unique state-domain ownership, a positive deterministic work formula, and a conflict policy
/// without filling any profile-specific execution semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExtensionSlotDescriptorV1 {
    /// Unique non-zero slot identifier (`1..=65535`).
    pub slot_id: u16,
    /// Inclusive lower bound of reserved operation tags (`>= 16`).
    pub op_tag_min: u8,
    /// Inclusive upper bound of reserved operation tags (`>= op_tag_min`).
    pub op_tag_max: u8,
    /// Inclusive lower bound of reserved disposition tags (`>= 16`).
    pub disposition_tag_min: u8,
    /// Inclusive upper bound of reserved disposition tags (`>= disposition_tag_min`).
    pub disposition_tag_max: u8,
    /// Sole state-domain bit (`3..=31`) owned exclusively by this extension slot.
    pub owned_domain_bit: u8,
    /// Bitmask of state domains this extension is permitted to read.
    pub read_domains_mask: u32,
    /// Bitmask of state domains this extension is permitted to mutate (must be subset of owned bit).
    pub write_domains_mask: u32,
    /// Non-zero fixed base work units charged per invocation.
    pub base_work_units: u64,
    /// Additional deterministic work units charged per operand/item.
    pub per_item_work_units: u64,
    /// Explicit conflict resolution policy for contending operations in this slot.
    pub conflict_policy: ConflictPolicy,
}

impl ExtensionSlotDescriptorV1 {
    /// Validates all slot invariants fail-closed before registration or hashing.
    pub fn validate(&self) -> Result<(), TransitionSpecError> {
        if self.slot_id == 0 {
            return Err(TransitionSpecError::ReservedSlotId);
        }
        if self.op_tag_min <= BASE_RESERVED_OP_TAG_MAX || self.op_tag_min > self.op_tag_max {
            return Err(TransitionSpecError::InvalidOpTagRange {
                min: self.op_tag_min,
                max: self.op_tag_max,
            });
        }
        if self.disposition_tag_min <= BASE_RESERVED_DISPOSITION_MAX
            || self.disposition_tag_min > self.disposition_tag_max
        {
            return Err(TransitionSpecError::InvalidDispositionRange {
                min: self.disposition_tag_min,
                max: self.disposition_tag_max,
            });
        }
        if !(3..32).contains(&self.owned_domain_bit) {
            return Err(TransitionSpecError::InvalidOwnedDomainBit {
                bit: self.owned_domain_bit,
            });
        }
        let owned_mask = 1u32 << self.owned_domain_bit;
        if (self.write_domains_mask & BASE_STATE_DOMAINS_MASK) != 0
            || (self.write_domains_mask & !owned_mask) != 0
        {
            return Err(TransitionSpecError::UndeclaredOrForbiddenWriteDomain {
                owned_bit: self.owned_domain_bit,
                write_domains_mask: self.write_domains_mask,
            });
        }
        if self.base_work_units == 0
            || self
                .base_work_units
                .checked_add(self.per_item_work_units)
                .is_none()
        {
            return Err(TransitionSpecError::InvalidWorkFormula {
                base: self.base_work_units,
                per_item: self.per_item_work_units,
            });
        }
        Ok(())
    }

    /// Encodes this validated descriptor into its canonical 32-byte big-endian payload.
    pub fn encode_canonical(&self) -> Result<[u8; EXTENSION_SLOT_WIRE_LEN], TransitionSpecError> {
        self.validate()?;
        let mut out = [0u8; EXTENSION_SLOT_WIRE_LEN];
        out[0..2].copy_from_slice(&self.slot_id.to_be_bytes());
        out[2] = self.op_tag_min;
        out[3] = self.op_tag_max;
        out[4] = self.disposition_tag_min;
        out[5] = self.disposition_tag_max;
        out[6] = self.owned_domain_bit;
        out[7] = self.conflict_policy.code();
        out[8..12].copy_from_slice(&self.read_domains_mask.to_be_bytes());
        out[12..16].copy_from_slice(&self.write_domains_mask.to_be_bytes());
        out[16..24].copy_from_slice(&self.base_work_units.to_be_bytes());
        out[24..32].copy_from_slice(&self.per_item_work_units.to_be_bytes());
        Ok(out)
    }

    /// Decodes and validates a canonical 32-byte [`ExtensionSlotDescriptorV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, TransitionSpecError> {
        if bytes.len() != EXTENSION_SLOT_WIRE_LEN {
            return Err(TransitionSpecError::MalformedPayload {
                reason: "extension slot descriptor must be exactly 32 bytes",
            });
        }
        let conflict_policy = ConflictPolicy::from_code(bytes[7])
            .ok_or(TransitionSpecError::InvalidConflictPolicy { code: bytes[7] })?;
        let slot = Self {
            slot_id: u16::from_be_bytes([bytes[0], bytes[1]]),
            op_tag_min: bytes[2],
            op_tag_max: bytes[3],
            disposition_tag_min: bytes[4],
            disposition_tag_max: bytes[5],
            owned_domain_bit: bytes[6],
            read_domains_mask: u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
            write_domains_mask: u32::from_be_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]),
            base_work_units: u64::from_be_bytes([
                bytes[16], bytes[17], bytes[18], bytes[19], bytes[20], bytes[21], bytes[22],
                bytes[23],
            ]),
            per_item_work_units: u64::from_be_bytes([
                bytes[24], bytes[25], bytes[26], bytes[27], bytes[28], bytes[29], bytes[30],
                bytes[31],
            ]),
            conflict_policy,
        };
        slot.validate()?;
        Ok(slot)
    }

    /// Computes the canonical `Policy` `ContentTag` for this extension slot descriptor.
    pub fn tag(&self) -> Result<ContentTag, TransitionSpecError> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            TRANSITION_SPEC_D1_EXTENSION_SLOT_SCHEMA,
            bytes.len() as u64,
            |sink| sink(&bytes),
        )
        .expect("extension slot canonical tag succeeds"))
    }
}

/// Ordered, collision-free registry of typed extension slots attached to `BaseTransitionSpecD1`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExtensionInterfaceRegistryV1 {
    slots: Vec<ExtensionSlotDescriptorV1>,
}

impl ExtensionInterfaceRegistryV1 {
    /// Returns an empty extension registry with zero profile slots filled.
    pub const fn empty() -> Self {
        Self { slots: Vec::new() }
    }

    /// Validates and constructs a strictly ordered, collision-free extension slot registry.
    pub fn new(slots: Vec<ExtensionSlotDescriptorV1>) -> Result<Self, TransitionSpecError> {
        for slot in &slots {
            slot.validate()?;
        }
        for pair in slots.windows(2) {
            if pair[0].slot_id >= pair[1].slot_id {
                return Err(TransitionSpecError::NonCanonicalSlotOrder {
                    prev_slot: pair[0].slot_id,
                    next_slot: pair[1].slot_id,
                });
            }
        }
        for i in 0..slots.len() {
            for j in (i + 1)..slots.len() {
                let a = &slots[i];
                let b = &slots[j];
                if a.op_tag_min <= b.op_tag_max && b.op_tag_min <= a.op_tag_max {
                    return Err(TransitionSpecError::OpTagCollision {
                        first_slot: a.slot_id,
                        second_slot: b.slot_id,
                    });
                }
                if a.disposition_tag_min <= b.disposition_tag_max
                    && b.disposition_tag_min <= a.disposition_tag_max
                {
                    return Err(TransitionSpecError::DispositionTagCollision {
                        first_slot: a.slot_id,
                        second_slot: b.slot_id,
                    });
                }
                if a.owned_domain_bit == b.owned_domain_bit {
                    return Err(TransitionSpecError::DomainOwnershipCollision {
                        first_slot: a.slot_id,
                        second_slot: b.slot_id,
                        domain_bit: a.owned_domain_bit,
                    });
                }
            }
        }
        Ok(Self { slots })
    }

    /// Borrows the canonically sorted slice of declared extension slots.
    pub fn slots(&self) -> &[ExtensionSlotDescriptorV1] {
        &self.slots
    }
}

/// Versioned base occupancy/episode transition contract (`TRANSITION-SPEC-D1`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseTransitionSpecD1 {
    /// Protocol contract version (`1`).
    pub spec_version: u16,
    /// Maximum base occupancy operations accepted in one atomic batch.
    pub max_batch_ops: u32,
    /// Deterministic base work units charged per base occupancy operation.
    pub base_work_per_edit: u64,
    /// Immutable bedrock coordinate excluded from the mutable active-episode map.
    pub bedrock_coord: Coord,
    /// Bitmask of state domains owned exclusively by the base substrate.
    pub base_domains_mask: u32,
    /// Collision-free typed extension slot registry (empty in the unfilled base contract).
    pub extensions: ExtensionInterfaceRegistryV1,
}

impl BaseTransitionSpecD1 {
    /// Constructs the canonical base-only `TRANSITION-SPEC-D1` contract with no profile slots filled.
    pub const fn canonical_base_only() -> Self {
        Self {
            spec_version: BASE_TRANSITION_SPEC_VERSION,
            max_batch_ops: MAX_BASE_BATCH_OPS as u32,
            base_work_per_edit: 1,
            bedrock_coord: BEDROCK_COORD,
            base_domains_mask: BASE_STATE_DOMAINS_MASK,
            extensions: ExtensionInterfaceRegistryV1::empty(),
        }
    }

    /// Constructs a validated `TRANSITION-SPEC-D1` contract binding `extensions`.
    pub fn with_extensions(extensions: ExtensionInterfaceRegistryV1) -> Self {
        Self {
            extensions,
            ..Self::canonical_base_only()
        }
    }

    /// Validates all contract invariants.
    pub fn validate(&self) -> Result<(), TransitionSpecError> {
        if self.spec_version != BASE_TRANSITION_SPEC_VERSION {
            return Err(TransitionSpecError::WrongVersion {
                expected: BASE_TRANSITION_SPEC_VERSION,
                actual: self.spec_version,
            });
        }
        if self.max_batch_ops == 0 || (self.max_batch_ops as usize) > MAX_BASE_BATCH_OPS {
            return Err(TransitionSpecError::BatchOverLimit {
                count: self.max_batch_ops as usize,
                limit: MAX_BASE_BATCH_OPS,
            });
        }
        if self.base_work_per_edit == 0 {
            return Err(TransitionSpecError::InvalidWorkFormula {
                base: 0,
                per_item: 0,
            });
        }
        if self.bedrock_coord != BEDROCK_COORD || self.base_domains_mask != BASE_STATE_DOMAINS_MASK
        {
            return Err(TransitionSpecError::MalformedPayload {
                reason: "base contract bedrock coordinate or base domain mask mismatch",
            });
        }
        Ok(())
    }

    /// Computes checked deterministic work cost for a base batch of `op_count` edits.
    pub fn work_cost(&self, op_count: usize) -> Result<u64, TransitionSpecError> {
        let count_u64 =
            u64::try_from(op_count).map_err(|_| TransitionSpecError::WorkCostOverflow)?;
        self.base_work_per_edit
            .checked_mul(count_u64)
            .ok_or(TransitionSpecError::WorkCostOverflow)
    }

    /// Encodes this contract into its canonical big-endian wire payload.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, TransitionSpecError> {
        self.validate()?;
        let slot_count = u16::try_from(self.extensions.slots().len()).map_err(|_| {
            TransitionSpecError::MalformedPayload {
                reason: "extension slot count exceeds u16::MAX",
            }
        })?;
        let mut out = Vec::with_capacity(44 + usize::from(slot_count) * EXTENSION_SLOT_WIRE_LEN);
        out.extend_from_slice(&self.spec_version.to_be_bytes());
        out.extend_from_slice(&self.max_batch_ops.to_be_bytes());
        out.extend_from_slice(&self.base_work_per_edit.to_be_bytes());
        out.extend_from_slice(&self.bedrock_coord.x.to_be_bytes());
        out.extend_from_slice(&self.bedrock_coord.y.to_be_bytes());
        out.extend_from_slice(&self.bedrock_coord.z.to_be_bytes());
        out.extend_from_slice(&self.base_domains_mask.to_be_bytes());
        out.extend_from_slice(&slot_count.to_be_bytes());
        for slot in self.extensions.slots() {
            out.extend_from_slice(&slot.encode_canonical()?);
        }
        Ok(out)
    }

    /// Decodes and validates a canonical `BaseTransitionSpecD1` payload fail-closed.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, TransitionSpecError> {
        if bytes.len() < 44 {
            return Err(TransitionSpecError::MalformedPayload {
                reason: "base contract payload truncated before fixed 44-byte header",
            });
        }
        let spec_version = u16::from_be_bytes([bytes[0], bytes[1]]);
        if spec_version != BASE_TRANSITION_SPEC_VERSION {
            return Err(TransitionSpecError::WrongVersion {
                expected: BASE_TRANSITION_SPEC_VERSION,
                actual: spec_version,
            });
        }
        let max_batch_ops = u32::from_be_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]);
        let base_work_per_edit = u64::from_be_bytes([
            bytes[6], bytes[7], bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13],
        ]);
        let bx = i64::from_be_bytes([
            bytes[14], bytes[15], bytes[16], bytes[17], bytes[18], bytes[19], bytes[20], bytes[21],
        ]);
        let by = i64::from_be_bytes([
            bytes[22], bytes[23], bytes[24], bytes[25], bytes[26], bytes[27], bytes[28], bytes[29],
        ]);
        let bz = i64::from_be_bytes([
            bytes[30], bytes[31], bytes[32], bytes[33], bytes[34], bytes[35], bytes[36], bytes[37],
        ]);
        let base_domains_mask = u32::from_be_bytes([bytes[38], bytes[39], bytes[40], bytes[41]]);
        let slot_count = u16::from_be_bytes([bytes[42], bytes[43]]) as usize;
        let expected_len = 44usize
            .checked_add(slot_count.saturating_mul(EXTENSION_SLOT_WIRE_LEN))
            .ok_or(TransitionSpecError::MalformedPayload {
                reason: "extension slot byte length overflow",
            })?;
        if bytes.len() != expected_len {
            return Err(TransitionSpecError::MalformedPayload {
                reason: "base contract length mismatch or trailing bytes",
            });
        }
        let mut slots = Vec::with_capacity(slot_count);
        for i in 0..slot_count {
            let offset = 44 + i * EXTENSION_SLOT_WIRE_LEN;
            slots.push(ExtensionSlotDescriptorV1::decode_canonical(
                &bytes[offset..offset + EXTENSION_SLOT_WIRE_LEN],
            )?);
        }
        let spec = Self {
            spec_version,
            max_batch_ops,
            base_work_per_edit,
            bedrock_coord: Coord::new(bx, by, bz),
            base_domains_mask,
            extensions: ExtensionInterfaceRegistryV1::new(slots)?,
        };
        spec.validate()?;
        Ok(spec)
    }

    /// Computes the canonical `Policy` `ContentTag` for this base transition contract.
    pub fn tag(&self) -> Result<ContentTag, TransitionSpecError> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            TRANSITION_SPEC_D1_CONTRACT_SCHEMA,
            bytes.len() as u64,
            |sink| sink(&bytes),
        )
        .expect("base transition contract canonical tag succeeds"))
    }
}

/// Canonical base-operation batch submitted to [`execute_base_transition_bytes`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseTransitionBatchV1 {
    /// Contract version targeted by this batch (`1`).
    pub spec_version: u16,
    /// Monotonic simulation/ledger tick ordinal for this batch.
    pub tick: u64,
    /// Ordered sequence of base occupancy edits (`0..=max_batch_ops`).
    pub edits: Vec<Edit>,
}

impl BaseTransitionBatchV1 {
    /// Encodes this batch into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, TransitionSpecError> {
        if self.spec_version != BASE_TRANSITION_SPEC_VERSION {
            return Err(TransitionSpecError::WrongVersion {
                expected: BASE_TRANSITION_SPEC_VERSION,
                actual: self.spec_version,
            });
        }
        if self.edits.len() > MAX_BASE_BATCH_OPS {
            return Err(TransitionSpecError::BatchOverLimit {
                count: self.edits.len(),
                limit: MAX_BASE_BATCH_OPS,
            });
        }
        let mut out = Vec::with_capacity(14 + self.edits.len() * EDIT_CANONICAL_LEN);
        out.extend_from_slice(&self.spec_version.to_be_bytes());
        out.extend_from_slice(&self.tick.to_be_bytes());
        out.extend_from_slice(&(self.edits.len() as u32).to_be_bytes());
        for edit in &self.edits {
            out.extend_from_slice(&edit.encode_canonical());
        }
        Ok(out)
    }

    /// Decodes a canonical batch fail-closed, checking `max_ops` before allocating `Vec<Edit>`.
    pub fn decode_canonical(bytes: &[u8], max_ops: usize) -> Result<Self, TransitionSpecError> {
        if bytes.len() < 14 {
            return Err(TransitionSpecError::MalformedPayload {
                reason: "base batch truncated before 14-byte header",
            });
        }
        let spec_version = u16::from_be_bytes([bytes[0], bytes[1]]);
        if spec_version != BASE_TRANSITION_SPEC_VERSION {
            return Err(TransitionSpecError::WrongVersion {
                expected: BASE_TRANSITION_SPEC_VERSION,
                actual: spec_version,
            });
        }
        let tick = u64::from_be_bytes([
            bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7], bytes[8], bytes[9],
        ]);
        let count = u32::from_be_bytes([bytes[10], bytes[11], bytes[12], bytes[13]]) as usize;
        let bound = max_ops.min(MAX_BASE_BATCH_OPS);
        if count > bound {
            return Err(TransitionSpecError::BatchOverLimit {
                count,
                limit: bound,
            });
        }
        let expected_len = 14usize
            .checked_add(count.saturating_mul(EDIT_CANONICAL_LEN))
            .ok_or(TransitionSpecError::MalformedPayload {
                reason: "base batch byte length overflow",
            })?;
        if bytes.len() != expected_len {
            return Err(TransitionSpecError::MalformedPayload {
                reason: "base batch length mismatch or trailing bytes",
            });
        }
        let mut edits = Vec::with_capacity(count);
        for i in 0..count {
            let offset = 14 + i * EDIT_CANONICAL_LEN;
            let edit = Edit::decode_canonical(&bytes[offset..offset + EDIT_CANONICAL_LEN])
                .map_err(|_| TransitionSpecError::MalformedPayload {
                    reason: "invalid canonical edit inside base batch",
                })?;
            edits.push(edit);
        }
        Ok(Self {
            spec_version,
            tick,
            edits,
        })
    }
}

/// Complete deterministic receipt emitted by an atomic `TRANSITION-SPEC-D1` base transition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseTransitionReceiptV1 {
    /// Canonical `Policy` tag of the governing `BaseTransitionSpecD1` contract.
    pub spec_tag: ContentTag,
    /// Tick ordinal committed by this transition.
    pub tick: u64,
    /// World `ContentTag` immediately before this transition.
    pub pre_world: ContentTag,
    /// History head `ContentTag` immediately before this transition.
    pub pre_history: ContentTag,
    /// World `ContentTag` immediately after this transition.
    pub post_world: ContentTag,
    /// History head `ContentTag` immediately after this transition.
    pub post_history: ContentTag,
    /// Deterministic work units charged for this batch.
    pub work_units: u64,
    /// Exact ordered per-operation base dispositions (`len == submitted edit count`).
    pub dispositions: Vec<BaseDispositionV1>,
}

impl BaseTransitionReceiptV1 {
    /// Encodes this transition receipt into its canonical big-endian wire payload.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, TransitionSpecError> {
        if self.dispositions.len() > MAX_BASE_BATCH_OPS {
            return Err(TransitionSpecError::BatchOverLimit {
                count: self.dispositions.len(),
                limit: MAX_BASE_BATCH_OPS,
            });
        }
        let mut out = Vec::with_capacity(180 + self.dispositions.len());
        out.extend_from_slice(&self.spec_tag.digest());
        out.extend_from_slice(&self.tick.to_be_bytes());
        out.extend_from_slice(&self.pre_world.digest());
        out.extend_from_slice(&self.pre_history.digest());
        out.extend_from_slice(&self.post_world.digest());
        out.extend_from_slice(&self.post_history.digest());
        out.extend_from_slice(&self.work_units.to_be_bytes());
        out.extend_from_slice(&(self.dispositions.len() as u32).to_be_bytes());
        for &d in &self.dispositions {
            out.push(d.code());
        }
        Ok(out)
    }

    /// Decodes and validates a canonical `BaseTransitionReceiptV1` payload fail-closed.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, TransitionSpecError> {
        if bytes.len() < 180 {
            return Err(TransitionSpecError::MalformedPayload {
                reason: "base transition receipt truncated before fixed 180-byte header",
            });
        }
        let read_digest = |offset: usize| -> [u8; 32] {
            let mut d = [0u8; 32];
            d.copy_from_slice(&bytes[offset..offset + 32]);
            d
        };
        let spec_tag = ContentTag::from_digest(TagKind::Policy, read_digest(0));
        let tick = u64::from_be_bytes([
            bytes[32], bytes[33], bytes[34], bytes[35], bytes[36], bytes[37], bytes[38], bytes[39],
        ]);
        let pre_world = ContentTag::from_digest(TagKind::World, read_digest(40));
        let pre_history = ContentTag::from_digest(TagKind::Receipt, read_digest(72));
        let post_world = ContentTag::from_digest(TagKind::World, read_digest(104));
        let post_history = ContentTag::from_digest(TagKind::Receipt, read_digest(136));
        let work_units = u64::from_be_bytes([
            bytes[168], bytes[169], bytes[170], bytes[171], bytes[172], bytes[173], bytes[174],
            bytes[175],
        ]);
        let count = u32::from_be_bytes([bytes[176], bytes[177], bytes[178], bytes[179]]) as usize;
        if count > MAX_BASE_BATCH_OPS {
            return Err(TransitionSpecError::BatchOverLimit {
                count,
                limit: MAX_BASE_BATCH_OPS,
            });
        }
        if bytes.len() != 180 + count {
            return Err(TransitionSpecError::MalformedPayload {
                reason: "base transition receipt length mismatch or trailing bytes",
            });
        }
        let mut dispositions = Vec::with_capacity(count);
        for &code in &bytes[180..] {
            let disp = BaseDispositionV1::from_code(code)
                .ok_or(TransitionSpecError::InvalidDispositionCode { code })?;
            dispositions.push(disp);
        }
        Ok(Self {
            spec_tag,
            tick,
            pre_world,
            pre_history,
            post_world,
            post_history,
            work_units,
            dispositions,
        })
    }

    /// Computes the canonical `Receipt` `ContentTag` for this transition receipt.
    pub fn tag(&self) -> Result<ContentTag, TransitionSpecError> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            TRANSITION_SPEC_D1_BASE_RECEIPT_SCHEMA,
            bytes.len() as u64,
            |sink| sink(&bytes),
        )
        .expect("base transition receipt canonical tag succeeds"))
    }
}

/// Executes one canonical `BaseTransitionBatchV1` atomically against `(world, history, episodes)`.
///
/// Decodes and validates all inputs and work bounds before staging mutations. On any error
/// (malformed bytes, wrong version, over-limit batch, or bijection/consistency mismatch),
/// `world`, `history`, and `episodes` are left completely unmodified (zero partial writes).
pub fn execute_base_transition_bytes(
    spec: &BaseTransitionSpecD1,
    initial_world: &World,
    world: &mut World,
    history: &mut History,
    episodes: &mut OccupancyEpisodeLedger,
    batch_bytes: &[u8],
) -> Result<BaseTransitionReceiptV1, TransitionSpecError> {
    spec.validate()?;
    let spec_tag = spec.tag()?;
    let batch = BaseTransitionBatchV1::decode_canonical(batch_bytes, spec.max_batch_ops as usize)?;
    let work_units = spec.work_cost(batch.edits.len())?;

    let pre_world = world_tag(world);
    let pre_history = history.head();

    // Stage all mutations on isolated clones so any failure leaves caller state untouched.
    let mut staged_world = world.clone();
    let mut staged_history = history.clone();
    let mut staged_episodes = episodes.clone();

    let mut dispositions = Vec::with_capacity(batch.edits.len());
    if !batch.edits.is_empty() {
        let mut tx = staged_history.begin_tick(batch.tick);
        for &edit in &batch.edits {
            tx.apply_edit(&staged_history, &mut staged_world, edit);
        }
        let receipt = staged_history
            .commit_tick(tx, &staged_world)
            .ok_or(TransitionSpecError::AtomicInvariantFailure {
                reason: "non-empty base batch produced no history receipt",
            })?
            .clone();
        for effect in receipt.effects() {
            let disp = match effect {
                TransitionEffect::Mint { .. } => BaseDispositionV1::Mint,
                TransitionEffect::Burn { .. } => BaseDispositionV1::Burn,
                TransitionEffect::NoOp { reason, .. } => match reason {
                    NoOpReason::BedrockProtected => BaseDispositionV1::NoOpBedrock,
                    NoOpReason::AlreadyOn => BaseDispositionV1::NoOpAlreadyOn,
                    NoOpReason::AlreadyOff => BaseDispositionV1::NoOpAlreadyOff,
                },
            };
            dispositions.push(disp);
        }
        if dispositions.len() != batch.edits.len() {
            return Err(TransitionSpecError::AtomicInvariantFailure {
                reason: "disposition lane count diverged from submitted edit count",
            });
        }
        staged_episodes.apply_receipt(&receipt).map_err(|_| {
            TransitionSpecError::AtomicInvariantFailure {
                reason: "episode ledger rejected committed base receipt",
            }
        })?;
    }

    if !staged_episodes.verify_world_bijection(initial_world, &staged_world) {
        return Err(TransitionSpecError::AtomicInvariantFailure {
            reason: "post-transition world/episode bijection failed",
        });
    }
    if !staged_episodes.verify_consistency(&staged_history, initial_world) {
        return Err(TransitionSpecError::AtomicInvariantFailure {
            reason: "post-transition history/episode consistency failed",
        });
    }

    let post_world = world_tag(&staged_world);
    let post_history = staged_history.head();

    // Atomic commit into caller state only after every invariant holds.
    *world = staged_world;
    *history = staged_history;
    *episodes = staged_episodes;

    Ok(BaseTransitionReceiptV1 {
        spec_tag,
        tick: batch.tick,
        pre_world,
        pre_history,
        post_world,
        post_history,
        work_units,
        dispositions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::program::Operation;

    #[test]
    fn canonical_base_contract_and_extension_slot_tags_are_pinned_and_roundtrip() {
        let base_spec = BaseTransitionSpecD1::canonical_base_only();
        let bytes = base_spec.encode_canonical().expect("encode base spec");
        assert_eq!(bytes.len(), 44);
        assert_eq!(
            BaseTransitionSpecD1::decode_canonical(&bytes),
            Ok(base_spec.clone())
        );
        let base_tag = base_spec.tag().expect("base spec tag");
        assert_eq!(
            base_tag.to_string(),
            "#bb.policy.sha256.pesnqo27cmnpc4q2jpudlfm5mmnhysue2tmstbyc7f77dkvus5na"
        );

        let slot = ExtensionSlotDescriptorV1 {
            slot_id: 1,
            op_tag_min: 16,
            op_tag_max: 23,
            disposition_tag_min: 16,
            disposition_tag_max: 23,
            owned_domain_bit: 3,
            read_domains_mask: BASE_STATE_DOMAINS_MASK | (1 << 3),
            write_domains_mask: 1 << 3,
            base_work_units: 10,
            per_item_work_units: 2,
            conflict_policy: ConflictPolicy::RejectConcurrentWrite,
        };
        let slot_bytes = slot.encode_canonical().expect("encode slot");
        assert_eq!(slot_bytes.len(), EXTENSION_SLOT_WIRE_LEN);
        assert_eq!(
            ExtensionSlotDescriptorV1::decode_canonical(&slot_bytes),
            Ok(slot)
        );
        let slot_tag = slot.tag().expect("slot tag");
        assert_eq!(
            slot_tag.to_string(),
            "#bb.policy.sha256.kxlgpvzyz3bcvdbpzvlda4yiehaph2rae2qsgrqx7dpibzz4y6ya"
        );

        let registry = ExtensionInterfaceRegistryV1::new(vec![slot]).expect("valid registry");
        let ext_spec = BaseTransitionSpecD1::with_extensions(registry);
        let ext_bytes = ext_spec.encode_canonical().expect("encode ext spec");
        assert_eq!(ext_bytes.len(), 44 + EXTENSION_SLOT_WIRE_LEN);
        assert_eq!(
            BaseTransitionSpecD1::decode_canonical(&ext_bytes),
            Ok(ext_spec)
        );
    }

    #[test]
    fn extension_registry_rejects_collisions_undeclared_writes_and_hijacks() {
        let valid_a = ExtensionSlotDescriptorV1 {
            slot_id: 1,
            op_tag_min: 16,
            op_tag_max: 20,
            disposition_tag_min: 16,
            disposition_tag_max: 20,
            owned_domain_bit: 3,
            read_domains_mask: BASE_STATE_DOMAINS_MASK,
            write_domains_mask: 1 << 3,
            base_work_units: 5,
            per_item_work_units: 1,
            conflict_policy: ConflictPolicy::DeterministicOrder,
        };
        // 1. Base op-tag hijack (<= 15)
        let mut hijack = valid_a;
        hijack.op_tag_min = 15;
        assert!(matches!(
            hijack.validate(),
            Err(TransitionSpecError::InvalidOpTagRange { .. })
        ));
        // 2. Undeclared/forbidden write to base domain
        let mut bad_write = valid_a;
        bad_write.write_domains_mask = DOMAIN_BIT_WORLD_OCCUPANCY | (1 << 3);
        assert!(matches!(
            bad_write.validate(),
            Err(TransitionSpecError::UndeclaredOrForbiddenWriteDomain { .. })
        ));
        // 3. Undeclared write to another extension's domain bit
        let mut unowned_write = valid_a;
        unowned_write.write_domains_mask = (1 << 3) | (1 << 4);
        assert!(matches!(
            unowned_write.validate(),
            Err(TransitionSpecError::UndeclaredOrForbiddenWriteDomain { .. })
        ));
        // 4. Zero base work (uncharged work forbidden)
        let mut free_work = valid_a;
        free_work.base_work_units = 0;
        assert!(matches!(
            free_work.validate(),
            Err(TransitionSpecError::InvalidWorkFormula { .. })
        ));
        // 5. Op-tag collision across two slots
        let overlap_op = ExtensionSlotDescriptorV1 {
            slot_id: 2,
            op_tag_min: 20,
            op_tag_max: 25,
            disposition_tag_min: 24,
            disposition_tag_max: 30,
            owned_domain_bit: 4,
            read_domains_mask: BASE_STATE_DOMAINS_MASK,
            write_domains_mask: 1 << 4,
            base_work_units: 5,
            per_item_work_units: 1,
            conflict_policy: ConflictPolicy::RejectConcurrentWrite,
        };
        assert_eq!(
            ExtensionInterfaceRegistryV1::new(vec![valid_a, overlap_op]),
            Err(TransitionSpecError::OpTagCollision {
                first_slot: 1,
                second_slot: 2
            })
        );
        // 6. Disposition collision across two slots
        let mut overlap_disp = overlap_op;
        overlap_disp.op_tag_min = 21;
        overlap_disp.disposition_tag_min = 20;
        assert_eq!(
            ExtensionInterfaceRegistryV1::new(vec![valid_a, overlap_disp]),
            Err(TransitionSpecError::DispositionTagCollision {
                first_slot: 1,
                second_slot: 2
            })
        );
        // 7. State-domain ownership collision across two slots
        let mut overlap_dom = overlap_op;
        overlap_dom.op_tag_min = 21;
        overlap_dom.owned_domain_bit = 3;
        overlap_dom.write_domains_mask = 1 << 3;
        assert_eq!(
            ExtensionInterfaceRegistryV1::new(vec![valid_a, overlap_dom]),
            Err(TransitionSpecError::DomainOwnershipCollision {
                first_slot: 1,
                second_slot: 2,
                domain_bit: 3
            })
        );
        // 8. Out-of-order / duplicate slot IDs
        let mut valid_b = overlap_op;
        valid_b.op_tag_min = 21;
        assert!(matches!(
            ExtensionInterfaceRegistryV1::new(vec![valid_b, valid_a]),
            Err(TransitionSpecError::NonCanonicalSlotOrder { .. })
        ));
    }

    #[test]
    fn base_transition_executes_atomically_and_rejects_malformed_with_zero_partial_writes() {
        let spec = BaseTransitionSpecD1::canonical_base_only();
        let initial_world = World::new();
        let mut world = initial_world.clone();
        let mut history = History::new(&initial_world);
        let mut episodes = OccupancyEpisodeLedger::new(history.genesis());

        let c = Coord::new(1, 2, 3);
        let batch = BaseTransitionBatchV1 {
            spec_version: BASE_TRANSITION_SPEC_VERSION,
            tick: 1,
            edits: vec![
                Edit::new(Operation::On, c),              // Mint
                Edit::new(Operation::On, c),              // NoOpAlreadyOn
                Edit::new(Operation::Off, BEDROCK_COORD), // NoOpBedrock
                Edit::new(Operation::Off, c),             // Burn
                Edit::new(Operation::Off, c),             // NoOpAlreadyOff
                Edit::new(Operation::Toggle, c),          // Mint (fresh episode ID!)
            ],
        };
        let batch_bytes = batch.encode_canonical().expect("encode batch");
        let receipt = execute_base_transition_bytes(
            &spec,
            &initial_world,
            &mut world,
            &mut history,
            &mut episodes,
            &batch_bytes,
        )
        .expect("atomic base transition succeeds");

        assert_eq!(
            receipt.dispositions,
            vec![
                BaseDispositionV1::Mint,
                BaseDispositionV1::NoOpAlreadyOn,
                BaseDispositionV1::NoOpBedrock,
                BaseDispositionV1::Burn,
                BaseDispositionV1::NoOpAlreadyOff,
                BaseDispositionV1::Mint,
            ]
        );
        assert_eq!(receipt.work_units, 6);
        assert_eq!(episodes.active_count(), 1);
        assert_eq!(episodes.total_mints(), 2);
        assert_eq!(episodes.total_burns(), 1);
        assert!(world.is_on(c));
        assert!(world.is_on(BEDROCK_COORD));

        let receipt_bytes = receipt.encode_canonical().expect("encode receipt");
        assert_eq!(
            BaseTransitionReceiptV1::decode_canonical(&receipt_bytes),
            Ok(receipt.clone())
        );

        // Powered zero-partial-write negative control: malformed 2nd edit rejects without mutating world/history/episodes
        let snap_world = world.clone();
        let snap_history = history.clone();
        let snap_episodes = episodes.clone();
        let mut corrupted = batch_bytes.clone();
        corrupted[14 + EDIT_CANONICAL_LEN] = 99; // invalid opcode on 2nd edit after valid 1st edit!
        assert!(
            execute_base_transition_bytes(
                &spec,
                &initial_world,
                &mut world,
                &mut history,
                &mut episodes,
                &corrupted,
            )
            .is_err()
        );
        assert_eq!(world_tag(&world), world_tag(&snap_world));
        assert_eq!(history.head(), snap_history.head());
        assert_eq!(episodes.total_mints(), snap_episodes.total_mints());
        assert_eq!(episodes.total_burns(), snap_episodes.total_burns());

        // Wrong version rejects before mutation
        let mut wrong_ver = batch_bytes;
        wrong_ver[1] = 2;
        assert_eq!(
            execute_base_transition_bytes(
                &spec,
                &initial_world,
                &mut world,
                &mut history,
                &mut episodes,
                &wrong_ver,
            ),
            Err(TransitionSpecError::WrongVersion {
                expected: 1,
                actual: 2
            })
        );
    }
}
