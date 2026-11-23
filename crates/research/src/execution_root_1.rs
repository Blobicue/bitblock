//! `EXECUTION-ROOT-1`: complete L1 §5 two-layer 11-slot execution-state root composer
//! (`application_root` → `execution_state_root` → external-context [`ExecutionRootRefV1`])
//! over `COMMIT-1`, `AUTH-COMMIT-1`, `ASSET-1`, `GOVERNANCE-1`, `ECONOMIC-STATE-1`,
//! `VALIDATOR-SCHEDULE-1`, and `EXECUTOR-1` (`PS-030`).
//!
//! Leaves frozen `commit_state.rs` (`COMMIT-1`) untouched and composes all 11 execution registry
//! slots strictly from their upstream root outputs without reimplementing any domain producer:
//! - **Application layer (slots `1..=6`):** `WorldOccupancy`, `ActiveToken`, `Episode`,
//!   `NativeBalance`, `EpisodeOwnership`, plus explicit [`InactiveDomainSentinelV1`] at slot `6`
//!   (`ProgramState`) on the initial launch profile (never zero-filled or empty-trie substituted).
//! - **Protocol layer (slots `7..=11`):** `Authorization`, `Nonce`, `GovernanceState`,
//!   `EconomicStake`, and `ValidatorSchedule`.
//! - **Cycle-free identifier rule (L1 §5):** `profile_id` and `chain_launch_manifest_id` are
//!   deliberately excluded from `application_root`, `execution_state_root`, and every map-root
//!   preimage, and bound externally in [`ExecutionRootRefV1`] and [`AuthenticatedRootRefV1`].
//! - **Fail-closed gate controls:** rejects omitted/duplicate/unknown/misordered slots, active
//!   bytes in an inactive slot, sentinel or zero bytes in an active slot, swapped domain/schema
//!   roots, header/chain/genesis/world/version mutations, and mixed-manifest publications.
//!
//! A complete execution root does not prove valid ordering, consensus finality, data availability,
//! or correct rendering.

use std::fmt;

use sha2::{Digest, Sha256};

use crate::executor_1::ExecutorDomainSnapshotV1;
use crate::profile_d1::BoundProfileManifestV1;
use crate::protocol::{ContentTag, TagKind, hash_canonical};

/// Canonical schema string for [`ExecutionDomainRegistryV1`] (`EXECUTION-ROOT-1`).
pub const EXECUTION_ROOT_1_REGISTRY_SCHEMA: &[u8] = b"bitblock/execution-root-1/domain-registry/v1";
/// Canonical schema string for [`ExecutionRootCompositionV1`] (`EXECUTION-ROOT-1`).
pub const EXECUTION_ROOT_1_COMPOSITION_SCHEMA: &[u8] =
    b"bitblock/execution-root-1/root-composition/v1";

/// Canonical version identifier for `EXECUTION-ROOT-1` objects.
pub const EXECUTION_ROOT_1_VERSION: u16 = 1;

const REGISTRY_DIGEST_PREFIX: &[u8] = b"bb.exec_root.domain_registry.v1";
const REQUIRED_SET_PREFIX: &[u8] = b"bb.exec_root.required_set.v1";
const ROOT_ENTRY_SET_PREFIX: &[u8] = b"bb.exec_root.entry_set.v1";
const APP_ROOT_PREFIX: &[u8] = b"bb.exec_root.app.v1";
const EXEC_ROOT_PREFIX: &[u8] = b"bb.exec_root.exec.v1";
const AUTH_ROOT_REF_PREFIX: &[u8] = b"bb.exec_root.auth_ref.v1";
const EXEC_ROOT_REF_PREFIX: &[u8] = b"bb.exec_root.exec_ref.v1";

/// Fail-closed validation, codec, or composition error for `EXECUTION-ROOT-1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecutionRoot1Error {
    /// Wire buffer had invalid version, length, discriminant, or trailing bytes.
    InvalidWire(&'static str),
    /// Registry had omitted, duplicate, unknown, misordered, or wrong-layer entries.
    InvalidRegistry(&'static str),
    /// Supplied slot input omitted a required domain, duplicated a slot, or was out of order.
    SlotCardinalityOrOrder(&'static str),
    /// Active root bytes were supplied for an inactive slot, or a sentinel/zero replaced an active slot.
    ActiveInactiveMismatch(&'static str),
    /// Domain schema digest or layer disagreed with the registry (cross-domain substitution).
    CrossDomainSubstitution(&'static str),
    /// External context (`chain_id`, `genesis_id`, `world_id`, `protocol_version`, `profile_id`) mismatch.
    ContextOrHeaderMismatch(&'static str),
    /// Manifest activation closure disagreed with the execution domain registry (mixed manifest).
    MixedManifestRejected(&'static str),
}

impl fmt::Display for ExecutionRoot1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ExecutionRoot1Error {}

/// Canonical composition layer for each slot in [`ExecutionDomainRegistryV1`] (L1 §5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ExecutionLayerV1 {
    /// Application state layer folded into `application_root` (slots `1..=6`).
    ApplicationLayer = 1,
    /// Protocol authority layer folded alongside `application_root` into `execution_state_root` (`7..=11`).
    ProtocolLayer = 2,
}

impl ExecutionLayerV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte layer discriminant.
    pub const fn from_code(code: u8) -> Result<Self, ExecutionRoot1Error> {
        match code {
            1 => Ok(Self::ApplicationLayer),
            2 => Ok(Self::ProtocolLayer),
            _ => Err(ExecutionRoot1Error::InvalidWire("unknown ExecutionLayerV1")),
        }
    }
}

/// Complete 11-slot canonical execution domain tag table (`1..=11`, strictly ascending).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ExecutionDomainTagV1 {
    /// Slot 1 (App): `COMMIT-1` voxel occupancy root (`WorldRootV1`).
    WorldOccupancy = 1,
    /// Slot 2 (App): `COMMIT-1` active occupancy token root (`ActiveTokenRootV1`).
    ActiveToken = 2,
    /// Slot 3 (App): `COMMIT-1` occupancy episode provenance root (`EpisodeRootV1`).
    Episode = 3,
    /// Slot 4 (App): `ASSET-1` fungible native balance root (`DomainIdV1::NativeBalance`).
    NativeBalance = 4,
    /// Slot 5 (App): `ASSET-1` episode ownership & deposit root (`DomainIdV1::EpisodeOwnership`).
    EpisodeOwnership = 5,
    /// Slot 6 (App): `PROGRAM-STATE-1` code & program state slot (INACTIVE sentinel at launch).
    ProgramState = 6,
    /// Slot 7 (Proto): `AUTH-COMMIT-1` authorization root (`DomainIdV1::Authorization`).
    Authorization = 7,
    /// Slot 8 (Proto): `AUTH-COMMIT-1` sequential nonce root (`DomainIdV1::Nonce`).
    Nonce = 8,
    /// Slot 9 (Proto): `GOVERNANCE-1` parameter & proposal state root (`DomainIdV1::GovernanceState`).
    GovernanceState = 9,
    /// Slot 10 (Proto): `ECONOMIC-STATE-1` bonded stake & slash root (`DomainIdV1::EconomicStake`).
    EconomicStake = 10,
    /// Slot 11 (Proto): `VALIDATOR-SCHEDULE-1` delayed epoch schedule root (`DomainIdV1::ValidatorSchedule`).
    ValidatorSchedule = 11,
}

impl ExecutionDomainTagV1 {
    /// Ordered array of all 11 canonical execution domain tags (`1..=11`).
    pub const ALL: [Self; 11] = [
        Self::WorldOccupancy,
        Self::ActiveToken,
        Self::Episode,
        Self::NativeBalance,
        Self::EpisodeOwnership,
        Self::ProgramState,
        Self::Authorization,
        Self::Nonce,
        Self::GovernanceState,
        Self::EconomicStake,
        Self::ValidatorSchedule,
    ];

    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte execution domain tag (`1..=11`).
    pub const fn from_code(code: u8) -> Result<Self, ExecutionRoot1Error> {
        match code {
            1 => Ok(Self::WorldOccupancy),
            2 => Ok(Self::ActiveToken),
            3 => Ok(Self::Episode),
            4 => Ok(Self::NativeBalance),
            5 => Ok(Self::EpisodeOwnership),
            6 => Ok(Self::ProgramState),
            7 => Ok(Self::Authorization),
            8 => Ok(Self::Nonce),
            9 => Ok(Self::GovernanceState),
            10 => Ok(Self::EconomicStake),
            11 => Ok(Self::ValidatorSchedule),
            _ => Err(ExecutionRoot1Error::InvalidRegistry(
                "unknown ExecutionDomainTagV1",
            )),
        }
    }

    /// Returns the required L1 §5 composition layer for this domain tag.
    pub const fn expected_layer(self) -> ExecutionLayerV1 {
        match self {
            Self::WorldOccupancy
            | Self::ActiveToken
            | Self::Episode
            | Self::NativeBalance
            | Self::EpisodeOwnership
            | Self::ProgramState => ExecutionLayerV1::ApplicationLayer,
            Self::Authorization
            | Self::Nonce
            | Self::GovernanceState
            | Self::EconomicStake
            | Self::ValidatorSchedule => ExecutionLayerV1::ProtocolLayer,
        }
    }

    /// Returns the canonical root-schema identifier string bound to this domain slot.
    pub const fn canonical_root_schema(self) -> &'static [u8] {
        match self {
            Self::WorldOccupancy => b"bb.exec_root.slot.world_v1",
            Self::ActiveToken => b"bb.exec_root.slot.active_token_v1",
            Self::Episode => b"bb.exec_root.slot.episode_v1",
            Self::NativeBalance => b"bb.exec_root.slot.native_balance_v1",
            Self::EpisodeOwnership => b"bb.exec_root.slot.episode_ownership_v1",
            Self::ProgramState => b"bb.exec_root.slot.program_state_v1",
            Self::Authorization => b"bb.exec_root.slot.authorization_v1",
            Self::Nonce => b"bb.exec_root.slot.nonce_v1",
            Self::GovernanceState => b"bb.exec_root.slot.governance_v1",
            Self::EconomicStake => b"bb.exec_root.slot.economic_stake_v1",
            Self::ValidatorSchedule => b"bb.exec_root.slot.validator_schedule_v1",
        }
    }

    /// Returns SHA-256 of [`Self::canonical_root_schema`].
    pub fn canonical_root_schema_digest(self) -> [u8; 32] {
        Sha256::digest(self.canonical_root_schema()).into()
    }
}

/// Typed sentinel occupying an inactive optional domain slot (never zero-filled bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum InactiveDomainSentinelV1 {
    /// Launch profile keeps `ProgramState` explicitly inactive (`program-active = false`).
    UnactivatedLaunchProgramV1 = 1,
}

impl InactiveDomainSentinelV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte inactive sentinel discriminant.
    pub const fn from_code(code: u8) -> Result<Self, ExecutionRoot1Error> {
        match code {
            1 => Ok(Self::UnactivatedLaunchProgramV1),
            _ => Err(ExecutionRoot1Error::ActiveInactiveMismatch(
                "unknown or zero InactiveDomainSentinelV1",
            )),
        }
    }

    /// Returns the canonical sentinel schema string.
    pub const fn schema_bytes(self) -> &'static [u8] {
        match self {
            Self::UnactivatedLaunchProgramV1 => {
                b"bb.exec_root.sentinel.unactivated_launch_program.v1"
            }
        }
    }

    /// Returns SHA-256 of [`Self::schema_bytes`].
    pub fn schema_digest(self) -> [u8; 32] {
        Sha256::digest(self.schema_bytes()).into()
    }
}

/// One slot descriptor in [`ExecutionDomainRegistryV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExecutionDomainEntryV1 {
    /// Active domain slot carrying its required root-schema digest.
    Active {
        /// Canonical domain tag (`1..=11`).
        domain_tag: ExecutionDomainTagV1,
        /// Composition layer (`ApplicationLayer` or `ProtocolLayer`).
        layer: ExecutionLayerV1,
        /// SHA-256 digest of the domain's canonical root schema.
        root_schema_digest: [u8; 32],
    },
    /// Inactive optional domain slot bound to an explicit typed sentinel (no root bytes).
    Inactive {
        /// Canonical domain tag (`1..=11`).
        domain_tag: ExecutionDomainTagV1,
        /// Composition layer (`ApplicationLayer` or `ProtocolLayer`).
        layer: ExecutionLayerV1,
        /// Explicit typed sentinel identifier.
        sentinel: InactiveDomainSentinelV1,
        /// SHA-256 digest of `sentinel.schema_bytes()`.
        sentinel_schema_digest: [u8; 32],
    },
}

impl ExecutionDomainEntryV1 {
    /// Fixed canonical big-endian wire length per registry entry (`36` bytes).
    pub const WIRE_LEN: usize = 36;

    /// Returns this entry's [`ExecutionDomainTagV1`].
    pub const fn domain_tag(&self) -> ExecutionDomainTagV1 {
        match *self {
            Self::Active { domain_tag, .. } | Self::Inactive { domain_tag, .. } => domain_tag,
        }
    }

    /// Returns this entry's [`ExecutionLayerV1`].
    pub const fn layer(&self) -> ExecutionLayerV1 {
        match *self {
            Self::Active { layer, .. } | Self::Inactive { layer, .. } => layer,
        }
    }

    /// Returns `true` iff this slot is active.
    pub const fn is_active(&self) -> bool {
        matches!(*self, Self::Active { .. })
    }

    /// Encodes this registry entry into 36 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> [u8; Self::WIRE_LEN] {
        let mut out = Vec::with_capacity(Self::WIRE_LEN);
        match *self {
            Self::Active {
                domain_tag,
                layer,
                root_schema_digest,
            } => {
                out.push(1);
                out.push(domain_tag.code());
                out.push(layer.code());
                out.push(0);
                out.extend_from_slice(&root_schema_digest);
            }
            Self::Inactive {
                domain_tag,
                layer,
                sentinel,
                sentinel_schema_digest,
            } => {
                out.push(2);
                out.push(domain_tag.code());
                out.push(layer.code());
                out.push(sentinel.code());
                out.extend_from_slice(&sentinel_schema_digest);
            }
        }
        out.try_into().expect("36-byte ExecutionDomainEntryV1")
    }

    /// Strictly decodes 36 canonical big-endian wire bytes into an [`ExecutionDomainEntryV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, ExecutionRoot1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(ExecutionRoot1Error::InvalidWire("wrong entry wire len"));
        }
        let kind = bytes[0];
        let domain_tag = ExecutionDomainTagV1::from_code(bytes[1])?;
        let layer = ExecutionLayerV1::from_code(bytes[2])?;
        let dig = read_arr32(bytes, 4)?;
        match kind {
            1 => {
                if bytes[3] != 0 {
                    return Err(ExecutionRoot1Error::InvalidWire(
                        "active entry sentinel != 0",
                    ));
                }
                Ok(Self::Active {
                    domain_tag,
                    layer,
                    root_schema_digest: dig,
                })
            }
            2 => {
                let sentinel = InactiveDomainSentinelV1::from_code(bytes[3])?;
                Ok(Self::Inactive {
                    domain_tag,
                    layer,
                    sentinel,
                    sentinel_schema_digest: dig,
                })
            }
            _ => Err(ExecutionRoot1Error::InvalidWire("unknown entry kind")),
        }
    }
}

/// Complete 11-slot canonical execution domain registry (`bitblock/execution-root-1/domain-registry/v1`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExecutionDomainRegistryV1 {
    /// Registry schema version (`1`).
    pub registry_version: u16,
    /// Strictly ascending unique 11-slot entries covering `ExecutionDomainTagV1::ALL`.
    pub entries: Vec<ExecutionDomainEntryV1>,
}

impl ExecutionDomainRegistryV1 {
    /// Builds the canonical v1 launch execution domain registry (10 active domains + slot `6`
    /// `ProgramState` explicitly bound to [`InactiveDomainSentinelV1::UnactivatedLaunchProgramV1`]).
    pub fn canonical_launch_v1() -> Self {
        let entries = ExecutionDomainTagV1::ALL
            .into_iter()
            .map(|tag| {
                let layer = tag.expected_layer();
                if tag == ExecutionDomainTagV1::ProgramState {
                    let s = InactiveDomainSentinelV1::UnactivatedLaunchProgramV1;
                    ExecutionDomainEntryV1::Inactive {
                        domain_tag: tag,
                        layer,
                        sentinel: s,
                        sentinel_schema_digest: s.schema_digest(),
                    }
                } else {
                    ExecutionDomainEntryV1::Active {
                        domain_tag: tag,
                        layer,
                        root_schema_digest: tag.canonical_root_schema_digest(),
                    }
                }
            })
            .collect();
        let reg = Self {
            registry_version: EXECUTION_ROOT_1_VERSION,
            entries,
        };
        reg.validate().expect("canonical launch registry is valid");
        reg
    }

    /// Validates completeness (`11` slots), strict ascending order, layer alignment, and schema
    /// & sentinel digests.
    pub fn validate(&self) -> Result<(), ExecutionRoot1Error> {
        if self.registry_version != EXECUTION_ROOT_1_VERSION {
            return Err(ExecutionRoot1Error::InvalidRegistry("bad registry_version"));
        }
        if self.entries.len() != ExecutionDomainTagV1::ALL.len() {
            return Err(ExecutionRoot1Error::InvalidRegistry(
                "registry must declare all 11 canonical domain slots",
            ));
        }
        for (entry, expected_tag) in self.entries.iter().zip(ExecutionDomainTagV1::ALL) {
            if entry.domain_tag() != expected_tag {
                return Err(ExecutionRoot1Error::InvalidRegistry(
                    "registry entries must be strictly ascending 1..=11",
                ));
            }
            if entry.layer() != expected_tag.expected_layer() {
                return Err(ExecutionRoot1Error::InvalidRegistry(
                    "registry entry layer mismatch",
                ));
            }
            match *entry {
                ExecutionDomainEntryV1::Active {
                    root_schema_digest, ..
                } => {
                    if root_schema_digest != expected_tag.canonical_root_schema_digest() {
                        return Err(ExecutionRoot1Error::CrossDomainSubstitution(
                            "active entry root_schema_digest mismatch",
                        ));
                    }
                }
                ExecutionDomainEntryV1::Inactive {
                    domain_tag,
                    sentinel,
                    sentinel_schema_digest,
                    ..
                } => {
                    if domain_tag != ExecutionDomainTagV1::ProgramState {
                        return Err(ExecutionRoot1Error::InvalidRegistry(
                            "mandatory launch domain cannot be marked inactive",
                        ));
                    }
                    if sentinel_schema_digest != sentinel.schema_digest() {
                        return Err(ExecutionRoot1Error::ActiveInactiveMismatch(
                            "inactive sentinel schema digest mismatch",
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Encodes this registry into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, ExecutionRoot1Error> {
        self.validate()?;
        let mut out = Vec::with_capacity(4 + self.entries.len() * ExecutionDomainEntryV1::WIRE_LEN);
        out.extend_from_slice(&self.registry_version.to_be_bytes());
        out.extend_from_slice(&(self.entries.len() as u16).to_be_bytes());
        for entry in &self.entries {
            out.extend_from_slice(&entry.encode_canonical());
        }
        Ok(out)
    }

    /// Strictly decodes canonical big-endian wire bytes into an [`ExecutionDomainRegistryV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, ExecutionRoot1Error> {
        if bytes.len() < 4 {
            return Err(ExecutionRoot1Error::InvalidWire("truncated registry"));
        }
        let registry_version = read_u16_be(bytes, 0)?;
        let count = read_u16_be(bytes, 2)? as usize;
        if bytes.len() != 4 + count * ExecutionDomainEntryV1::WIRE_LEN {
            return Err(ExecutionRoot1Error::InvalidWire("registry length mismatch"));
        }
        let mut entries = Vec::with_capacity(count);
        let mut cur = 4usize;
        for _ in 0..count {
            entries.push(ExecutionDomainEntryV1::decode_canonical(
                &bytes[cur..cur + ExecutionDomainEntryV1::WIRE_LEN],
            )?);
            cur += ExecutionDomainEntryV1::WIRE_LEN;
        }
        let reg = Self {
            registry_version,
            entries,
        };
        reg.validate()?;
        Ok(reg)
    }

    /// Computes the L1 §5 `ProfileIdHashV1` domain-framed `domain_registry_digest`.
    pub fn domain_registry_digest(&self) -> Result<[u8; 32], ExecutionRoot1Error> {
        let encoded = self.encode_canonical()?;
        let mut h = Sha256::new();
        h.update(REGISTRY_DIGEST_PREFIX);
        h.update((encoded.len() as u32).to_be_bytes());
        h.update(&encoded);
        Ok(h.finalize().into())
    }

    /// Computes SHA-256 over the sorted active `required_domain_set` entries.
    pub fn required_domain_set_digest(&self) -> Result<[u8; 32], ExecutionRoot1Error> {
        self.validate()?;
        let active: Vec<_> = self.entries.iter().filter(|e| e.is_active()).collect();
        let mut h = Sha256::new();
        h.update(REQUIRED_SET_PREFIX);
        h.update((active.len() as u16).to_be_bytes());
        for entry in active {
            h.update(entry.encode_canonical());
        }
        Ok(h.finalize().into())
    }

    /// Computes the canonical `Policy` `ContentTag` for this registry.
    pub fn tag(&self) -> Result<ContentTag, ExecutionRoot1Error> {
        let bytes = self.encode_canonical()?;
        hash_canonical(
            TagKind::Policy,
            EXECUTION_ROOT_1_REGISTRY_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .map_err(|_| ExecutionRoot1Error::InvalidWire("hash_canonical failed"))
    }
}

/// Cycle-free preimage context hashed into `application_root` and `execution_state_root` (L1 §5).
///
/// Deliberately excludes `profile_id` and `chain_launch_manifest_id` to prevent self-hash cycles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ExecutionRootPreimageContextV1 {
    /// Protocol version (`1`).
    pub protocol_version: u16,
    /// Chain identifier (`> 0`).
    pub chain_id: u64,
    /// Single `i64³` world identifier (`> 0`).
    pub world_id: u64,
    /// Bound `GENESIS-D1` immutable genesis identifier.
    pub genesis_id: [u8; 32],
    /// Pre-ID `PROFILE-SCOPE-D0` scope digest.
    pub profile_scope_digest: [u8; 32],
}

impl ExecutionRootPreimageContextV1 {
    /// Fixed canonical big-endian wire length (`82` bytes).
    pub const WIRE_LEN: usize = 82;

    /// Validates non-zero protocol/chain/world/genesis/scope fields.
    pub fn validate(&self) -> Result<(), ExecutionRoot1Error> {
        if self.protocol_version != EXECUTION_ROOT_1_VERSION
            || self.chain_id == 0
            || self.world_id == 0
            || self.genesis_id == [0u8; 32]
            || self.profile_scope_digest == [0u8; 32]
        {
            return Err(ExecutionRoot1Error::ContextOrHeaderMismatch(
                "invalid ExecutionRootPreimageContextV1",
            ));
        }
        Ok(())
    }

    /// Encodes this preimage context into 82 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], ExecutionRoot1Error> {
        self.validate()?;
        let mut out = Vec::with_capacity(Self::WIRE_LEN);
        out.extend_from_slice(&self.protocol_version.to_be_bytes());
        out.extend_from_slice(&self.chain_id.to_be_bytes());
        out.extend_from_slice(&self.world_id.to_be_bytes());
        out.extend_from_slice(&self.genesis_id);
        out.extend_from_slice(&self.profile_scope_digest);
        Ok(out.try_into().expect("82-byte preimage context"))
    }

    /// Strictly decodes 82 canonical big-endian wire bytes.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, ExecutionRoot1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(ExecutionRoot1Error::InvalidWire("wrong preimage ctx len"));
        }
        let ctx = Self {
            protocol_version: read_u16_be(bytes, 0)?,
            chain_id: read_u64_be(bytes, 2)?,
            world_id: read_u64_be(bytes, 10)?,
            genesis_id: read_arr32(bytes, 18)?,
            profile_scope_digest: read_arr32(bytes, 50)?,
        };
        ctx.validate()?;
        Ok(ctx)
    }
}

/// One composed slot commitment inside `application_domain_entries` or `protocol_domain_entries`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RootSlotCommitmentV1 {
    /// Active slot binding its domain tag, layer, root schema digest, and 32-byte root.
    ActiveRoot {
        /// Canonical domain tag (`1..=11`).
        domain_tag: ExecutionDomainTagV1,
        /// Composition layer.
        layer: ExecutionLayerV1,
        /// SHA-256 digest of the domain's root schema.
        root_schema_digest: [u8; 32],
        /// 32-byte authenticated Patricia root produced by the owning state producer.
        root_bytes: [u8; 32],
    },
    /// Inactive slot binding its explicit typed sentinel (strictly zero root payload).
    InactiveSentinel {
        /// Canonical domain tag (`6` on launch).
        domain_tag: ExecutionDomainTagV1,
        /// Composition layer (`ApplicationLayer`).
        layer: ExecutionLayerV1,
        /// Explicit typed sentinel discriminant.
        sentinel: InactiveDomainSentinelV1,
        /// SHA-256 digest of `sentinel.schema_bytes()`.
        sentinel_schema_digest: [u8; 32],
    },
}

impl RootSlotCommitmentV1 {
    /// Fixed canonical big-endian wire length (`68` bytes).
    pub const WIRE_LEN: usize = 68;

    /// Returns this slot's [`ExecutionDomainTagV1`].
    pub const fn domain_tag(&self) -> ExecutionDomainTagV1 {
        match *self {
            Self::ActiveRoot { domain_tag, .. } | Self::InactiveSentinel { domain_tag, .. } => {
                domain_tag
            }
        }
    }

    /// Encodes this slot commitment into 68 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> [u8; Self::WIRE_LEN] {
        let mut out = Vec::with_capacity(Self::WIRE_LEN);
        match *self {
            Self::ActiveRoot {
                domain_tag,
                layer,
                root_schema_digest,
                root_bytes,
            } => {
                out.push(1);
                out.push(domain_tag.code());
                out.push(layer.code());
                out.push(0);
                out.extend_from_slice(&root_schema_digest);
                out.extend_from_slice(&root_bytes);
            }
            Self::InactiveSentinel {
                domain_tag,
                layer,
                sentinel,
                sentinel_schema_digest,
            } => {
                out.push(2);
                out.push(domain_tag.code());
                out.push(layer.code());
                out.push(sentinel.code());
                out.extend_from_slice(&sentinel_schema_digest);
                out.extend_from_slice(&[0u8; 32]);
            }
        }
        out.try_into().expect("68-byte RootSlotCommitmentV1")
    }

    /// Strictly decodes 68 canonical big-endian wire bytes into a [`RootSlotCommitmentV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, ExecutionRoot1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(ExecutionRoot1Error::InvalidWire(
                "wrong slot commitment len",
            ));
        }
        let kind = bytes[0];
        let domain_tag = ExecutionDomainTagV1::from_code(bytes[1])?;
        let layer = ExecutionLayerV1::from_code(bytes[2])?;
        let d1 = read_arr32(bytes, 4)?;
        let d2 = read_arr32(bytes, 36)?;
        match kind {
            1 => {
                if bytes[3] != 0 {
                    return Err(ExecutionRoot1Error::InvalidWire(
                        "active slot sentinel != 0",
                    ));
                }
                Ok(Self::ActiveRoot {
                    domain_tag,
                    layer,
                    root_schema_digest: d1,
                    root_bytes: d2,
                })
            }
            2 => {
                let sentinel = InactiveDomainSentinelV1::from_code(bytes[3])?;
                if d2 != [0u8; 32] {
                    return Err(ExecutionRoot1Error::ActiveInactiveMismatch(
                        "inactive sentinel slot must carry zero trailing root bytes",
                    ));
                }
                Ok(Self::InactiveSentinel {
                    domain_tag,
                    layer,
                    sentinel,
                    sentinel_schema_digest: d1,
                })
            }
            _ => Err(ExecutionRoot1Error::InvalidWire(
                "unknown slot commitment kind",
            )),
        }
    }
}

/// External-context binding for a single active domain root (`AuthenticatedRootRefV1`, L1 §5).
///
/// Guarantees typed roots are strictly non-interchangeable across domains, chains, or profiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AuthenticatedRootRefV1 {
    /// Protocol version (`1`).
    pub protocol_version: u16,
    /// Chain identifier.
    pub chain_id: u64,
    /// World identifier.
    pub world_id: u64,
    /// Immutable `GENESIS-D1` identifier.
    pub genesis_id: [u8; 32],
    /// External `PROFILE-D1` profile identifier.
    pub profile_id: [u8; 32],
    /// Single active domain tag bound by this reference.
    pub domain_tag: ExecutionDomainTagV1,
    /// Canonical root schema digest for `domain_tag`.
    pub root_schema_digest: [u8; 32],
    /// 32-byte authenticated domain root.
    pub root_bytes: [u8; 32],
}

impl AuthenticatedRootRefV1 {
    /// Fixed canonical big-endian wire length (`147` bytes).
    pub const WIRE_LEN: usize = 147;

    /// Encodes this typed domain root reference into 147 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> [u8; Self::WIRE_LEN] {
        let mut out = Vec::with_capacity(Self::WIRE_LEN);
        out.extend_from_slice(&self.protocol_version.to_be_bytes());
        out.extend_from_slice(&self.chain_id.to_be_bytes());
        out.extend_from_slice(&self.world_id.to_be_bytes());
        out.extend_from_slice(&self.genesis_id);
        out.extend_from_slice(&self.profile_id);
        out.push(self.domain_tag.code());
        out.extend_from_slice(&self.root_schema_digest);
        out.extend_from_slice(&self.root_bytes);
        out.try_into().expect("147-byte AuthenticatedRootRefV1")
    }

    /// Computes the domain-separated SHA-256 reference identifier for this typed root.
    pub fn ref_digest(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(AUTH_ROOT_REF_PREFIX);
        h.update(self.encode_canonical());
        h.finalize().into()
    }
}

/// External-context binding for `execution_state_root` (`ExecutionRootRefV1`, L1 §4.4 & §5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ExecutionRootRefV1 {
    /// Protocol version (`1`).
    pub protocol_version: u16,
    /// Chain identifier.
    pub chain_id: u64,
    /// World identifier.
    pub world_id: u64,
    /// Immutable `GENESIS-D1` identifier.
    pub genesis_id: [u8; 32],
    /// External `PROFILE-D1` profile identifier (excluded from `execution_state_root` preimage).
    pub profile_id: [u8; 32],
    /// Pre-ID `PROFILE-SCOPE-D0` scope digest.
    pub profile_scope_digest: [u8; 32],
    /// Pre-ID `ExecutionDomainRegistryV1` digest.
    pub domain_registry_digest: [u8; 32],
    /// Composed two-layer `execution_state_root`.
    pub execution_state_root: [u8; 32],
}

impl ExecutionRootRefV1 {
    /// Fixed canonical big-endian wire length (`178` bytes).
    pub const WIRE_LEN: usize = 178;

    /// Encodes this execution root reference into 178 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> [u8; Self::WIRE_LEN] {
        let mut out = Vec::with_capacity(Self::WIRE_LEN);
        out.extend_from_slice(&self.protocol_version.to_be_bytes());
        out.extend_from_slice(&self.chain_id.to_be_bytes());
        out.extend_from_slice(&self.world_id.to_be_bytes());
        out.extend_from_slice(&self.genesis_id);
        out.extend_from_slice(&self.profile_id);
        out.extend_from_slice(&self.profile_scope_digest);
        out.extend_from_slice(&self.domain_registry_digest);
        out.extend_from_slice(&self.execution_state_root);
        out.try_into().expect("178-byte ExecutionRootRefV1")
    }

    /// Strictly decodes 178 canonical big-endian wire bytes into an [`ExecutionRootRefV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, ExecutionRoot1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(ExecutionRoot1Error::InvalidWire(
                "wrong ExecutionRootRefV1 len",
            ));
        }
        Ok(Self {
            protocol_version: read_u16_be(bytes, 0)?,
            chain_id: read_u64_be(bytes, 2)?,
            world_id: read_u64_be(bytes, 10)?,
            genesis_id: read_arr32(bytes, 18)?,
            profile_id: read_arr32(bytes, 50)?,
            profile_scope_digest: read_arr32(bytes, 82)?,
            domain_registry_digest: read_arr32(bytes, 114)?,
            execution_state_root: read_arr32(bytes, 146)?,
        })
    }

    /// Computes the domain-separated SHA-256 digest of this [`ExecutionRootRefV1`].
    pub fn ref_digest(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(EXEC_ROOT_REF_PREFIX);
        h.update(self.encode_canonical());
        h.finalize().into()
    }
}

/// Complete §5 two-layer execution-state root composition (`bitblock/execution-root-1/root-composition/v1`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExecutionRootCompositionV1 {
    /// Composition schema version (`1`).
    pub composition_version: u16,
    /// Block batch height at which this root was composed.
    pub batch_height: u64,
    /// Active protocol epoch.
    pub epoch: u64,
    /// External `PROFILE-D1` profile identifier bound in `execution_root_ref`.
    pub profile_id: [u8; 32],
    /// Cycle-free preimage context hashed into `application_root` and `execution_state_root`.
    pub preimage_context: ExecutionRootPreimageContextV1,
    /// Digest of the governing [`ExecutionDomainRegistryV1`].
    pub domain_registry_digest: [u8; 32],
    /// Digest of the sorted active `required_domain_set`.
    pub required_domain_set_digest: [u8; 32],
    /// Digest of the complete 11-slot composed root entry set.
    pub root_entry_set_digest: [u8; 32],
    /// Composed Layer-1 `application_root` over slots `1..=6`.
    pub application_root: [u8; 32],
    /// Composed Layer-2 `execution_state_root` over `application_root` + slots `7..=11`.
    pub execution_state_root: [u8; 32],
    /// Complete external-context [`ExecutionRootRefV1`].
    pub execution_root_ref: ExecutionRootRefV1,
    /// Exact 11 composed domain slots (`1..=11`).
    pub slots: Vec<RootSlotCommitmentV1>,
}

impl ExecutionRootCompositionV1 {
    /// Encodes this composition into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, ExecutionRoot1Error> {
        if self.composition_version != EXECUTION_ROOT_1_VERSION {
            return Err(ExecutionRoot1Error::InvalidWire("bad composition_version"));
        }
        let mut out = Vec::with_capacity(472 + self.slots.len() * RootSlotCommitmentV1::WIRE_LEN);
        out.extend_from_slice(&self.composition_version.to_be_bytes());
        out.extend_from_slice(&self.batch_height.to_be_bytes());
        out.extend_from_slice(&self.epoch.to_be_bytes());
        out.extend_from_slice(&self.profile_id);
        out.extend_from_slice(&self.preimage_context.encode_canonical()?);
        out.extend_from_slice(&self.domain_registry_digest);
        out.extend_from_slice(&self.required_domain_set_digest);
        out.extend_from_slice(&self.root_entry_set_digest);
        out.extend_from_slice(&self.application_root);
        out.extend_from_slice(&self.execution_state_root);
        out.extend_from_slice(&self.execution_root_ref.encode_canonical());
        out.extend_from_slice(&(self.slots.len() as u16).to_be_bytes());
        for s in &self.slots {
            out.extend_from_slice(&s.encode_canonical());
        }
        Ok(out)
    }

    /// Strictly decodes canonical big-endian wire bytes into an [`ExecutionRootCompositionV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, ExecutionRoot1Error> {
        const HEAD: usize = 2 + 8 + 8 + 32 + 82 + 32 * 5 + 178 + 2;
        if bytes.len() < HEAD {
            return Err(ExecutionRoot1Error::InvalidWire("truncated composition"));
        }
        let composition_version = read_u16_be(bytes, 0)?;
        if composition_version != EXECUTION_ROOT_1_VERSION {
            return Err(ExecutionRoot1Error::InvalidWire("bad composition_version"));
        }
        let batch_height = read_u64_be(bytes, 2)?;
        let epoch = read_u64_be(bytes, 10)?;
        let profile_id = read_arr32(bytes, 18)?;
        let preimage_context = ExecutionRootPreimageContextV1::decode_canonical(&bytes[50..132])?;
        let domain_registry_digest = read_arr32(bytes, 132)?;
        let required_domain_set_digest = read_arr32(bytes, 164)?;
        let root_entry_set_digest = read_arr32(bytes, 196)?;
        let application_root = read_arr32(bytes, 228)?;
        let execution_state_root = read_arr32(bytes, 260)?;
        let execution_root_ref = ExecutionRootRefV1::decode_canonical(&bytes[292..470])?;
        let slot_count = read_u16_be(bytes, 470)? as usize;
        if bytes.len() != HEAD + slot_count * RootSlotCommitmentV1::WIRE_LEN {
            return Err(ExecutionRoot1Error::InvalidWire("composition len mismatch"));
        }
        let mut slots = Vec::with_capacity(slot_count);
        let mut cur = HEAD;
        for _ in 0..slot_count {
            slots.push(RootSlotCommitmentV1::decode_canonical(
                &bytes[cur..cur + RootSlotCommitmentV1::WIRE_LEN],
            )?);
            cur += RootSlotCommitmentV1::WIRE_LEN;
        }
        Ok(Self {
            composition_version,
            batch_height,
            epoch,
            profile_id,
            preimage_context,
            domain_registry_digest,
            required_domain_set_digest,
            root_entry_set_digest,
            application_root,
            execution_state_root,
            execution_root_ref,
            slots,
        })
    }

    /// Projects a typed, non-interchangeable [`AuthenticatedRootRefV1`] for active slot `tag`.
    pub fn authenticated_domain_ref(
        &self,
        tag: ExecutionDomainTagV1,
    ) -> Result<AuthenticatedRootRefV1, ExecutionRoot1Error> {
        let slot = self.slots.iter().find(|s| s.domain_tag() == tag).ok_or(
            ExecutionRoot1Error::SlotCardinalityOrOrder("missing domain slot"),
        )?;
        match *slot {
            RootSlotCommitmentV1::ActiveRoot {
                domain_tag,
                root_schema_digest,
                root_bytes,
                ..
            } => Ok(AuthenticatedRootRefV1 {
                protocol_version: self.preimage_context.protocol_version,
                chain_id: self.preimage_context.chain_id,
                world_id: self.preimage_context.world_id,
                genesis_id: self.preimage_context.genesis_id,
                profile_id: self.profile_id,
                domain_tag,
                root_schema_digest,
                root_bytes,
            }),
            RootSlotCommitmentV1::InactiveSentinel { .. } => {
                Err(ExecutionRoot1Error::ActiveInactiveMismatch(
                    "inactive domain slot has no AuthenticatedRootRefV1",
                ))
            }
        }
    }

    /// Computes the canonical `Receipt` `ContentTag` for this execution root composition.
    pub fn tag(&self) -> Result<ContentTag, ExecutionRoot1Error> {
        let bytes = self.encode_canonical()?;
        hash_canonical(
            TagKind::Receipt,
            EXECUTION_ROOT_1_COMPOSITION_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .map_err(|_| ExecutionRoot1Error::InvalidWire("hash_canonical failed"))
    }
}

/// Composes [`ExecutionRootCompositionV1`] directly from an `EXECUTOR-1` [`ExecutorDomainSnapshotV1`].
pub fn compose_from_executor_snapshot(
    snapshot: &ExecutorDomainSnapshotV1,
    registry: &ExecutionDomainRegistryV1,
    preimage_context: ExecutionRootPreimageContextV1,
    profile_id: [u8; 32],
) -> Result<ExecutionRootCompositionV1, ExecutionRoot1Error> {
    registry.validate()?;
    let prog_entry = &registry.entries[5];
    if snapshot.program_state_active != prog_entry.is_active() {
        return Err(ExecutionRoot1Error::MixedManifestRejected(
            "snapshot program_state_active disagrees with registry slot 6",
        ));
    }
    let roots = snapshot.active_root_array();
    let slots = vec![
        active_slot(ExecutionDomainTagV1::WorldOccupancy, roots[0]),
        active_slot(ExecutionDomainTagV1::ActiveToken, roots[1]),
        active_slot(ExecutionDomainTagV1::Episode, roots[2]),
        active_slot(ExecutionDomainTagV1::NativeBalance, roots[5]),
        active_slot(ExecutionDomainTagV1::EpisodeOwnership, roots[6]),
        RootSlotCommitmentV1::InactiveSentinel {
            domain_tag: ExecutionDomainTagV1::ProgramState,
            layer: ExecutionLayerV1::ApplicationLayer,
            sentinel: InactiveDomainSentinelV1::UnactivatedLaunchProgramV1,
            sentinel_schema_digest: InactiveDomainSentinelV1::UnactivatedLaunchProgramV1
                .schema_digest(),
        },
        active_slot(ExecutionDomainTagV1::Authorization, roots[3]),
        active_slot(ExecutionDomainTagV1::Nonce, roots[4]),
        active_slot(ExecutionDomainTagV1::GovernanceState, roots[7]),
        active_slot(ExecutionDomainTagV1::EconomicStake, roots[8]),
        active_slot(ExecutionDomainTagV1::ValidatorSchedule, roots[9]),
    ];
    compose_from_slot_commitments(
        &slots,
        snapshot.batch_height,
        snapshot.epoch,
        registry,
        preimage_context,
        profile_id,
    )
}

/// Composes [`ExecutionRootCompositionV1`] from explicit ordered [`RootSlotCommitmentV1`] inputs,
/// enforcing every active/inactive, schema, ordering, and non-zero sentinel invariant fail-closed.
pub fn compose_from_slot_commitments(
    slots: &[RootSlotCommitmentV1],
    batch_height: u64,
    epoch: u64,
    registry: &ExecutionDomainRegistryV1,
    preimage_context: ExecutionRootPreimageContextV1,
    profile_id: [u8; 32],
) -> Result<ExecutionRootCompositionV1, ExecutionRoot1Error> {
    registry.validate()?;
    preimage_context.validate()?;
    if profile_id == [0u8; 32] {
        return Err(ExecutionRoot1Error::ContextOrHeaderMismatch(
            "zero profile_id",
        ));
    }
    if slots.len() != registry.entries.len() {
        return Err(ExecutionRoot1Error::SlotCardinalityOrOrder(
            "slot count disagrees with 11-slot registry",
        ));
    }

    let mut app_slots = Vec::with_capacity(6);
    let mut proto_slots = Vec::with_capacity(5);
    for (slot, entry) in slots.iter().zip(&registry.entries) {
        if slot.domain_tag() != entry.domain_tag() {
            return Err(ExecutionRoot1Error::SlotCardinalityOrOrder(
                "slot tag missing, duplicated, or out of canonical ascending order",
            ));
        }
        match (*slot, *entry) {
            (
                RootSlotCommitmentV1::ActiveRoot {
                    layer,
                    root_schema_digest,
                    root_bytes,
                    ..
                },
                ExecutionDomainEntryV1::Active {
                    layer: exp_layer,
                    root_schema_digest: exp_schema,
                    ..
                },
            ) => {
                if layer != exp_layer {
                    return Err(ExecutionRoot1Error::CrossDomainSubstitution(
                        "active slot layer mismatch",
                    ));
                }
                if root_schema_digest != exp_schema {
                    return Err(ExecutionRoot1Error::CrossDomainSubstitution(
                        "active slot root_schema_digest mismatch",
                    ));
                }
                if root_bytes == [0u8; 32] {
                    return Err(ExecutionRoot1Error::ActiveInactiveMismatch(
                        "active domain root cannot be synthetic all-zero bytes",
                    ));
                }
            }
            (
                RootSlotCommitmentV1::InactiveSentinel {
                    layer,
                    sentinel,
                    sentinel_schema_digest,
                    ..
                },
                ExecutionDomainEntryV1::Inactive {
                    layer: exp_layer,
                    sentinel: exp_sentinel,
                    sentinel_schema_digest: exp_sdig,
                    ..
                },
            ) => {
                if layer != exp_layer
                    || sentinel != exp_sentinel
                    || sentinel_schema_digest != exp_sdig
                {
                    return Err(ExecutionRoot1Error::ActiveInactiveMismatch(
                        "inactive sentinel slot disagrees with registry sentinel",
                    ));
                }
            }
            (RootSlotCommitmentV1::ActiveRoot { .. }, ExecutionDomainEntryV1::Inactive { .. }) => {
                return Err(ExecutionRoot1Error::ActiveInactiveMismatch(
                    "active root supplied in inactive domain slot",
                ));
            }
            (
                RootSlotCommitmentV1::InactiveSentinel { .. },
                ExecutionDomainEntryV1::Active { .. },
            ) => {
                return Err(ExecutionRoot1Error::ActiveInactiveMismatch(
                    "inactive sentinel supplied in active domain slot",
                ));
            }
        }
        if entry.layer() == ExecutionLayerV1::ApplicationLayer {
            app_slots.push(*slot);
        } else {
            proto_slots.push(*slot);
        }
    }

    let ctx_bytes = preimage_context.encode_canonical()?;
    let domain_registry_digest = registry.domain_registry_digest()?;
    let required_domain_set_digest = registry.required_domain_set_digest()?;
    let root_entry_set_digest = hash_slot_slice(ROOT_ENTRY_SET_PREFIX, slots);

    let mut app_h = Sha256::new();
    app_h.update(APP_ROOT_PREFIX);
    app_h.update(ctx_bytes);
    app_h.update(domain_registry_digest);
    app_h.update((app_slots.len() as u16).to_be_bytes());
    for s in &app_slots {
        app_h.update(s.encode_canonical());
    }
    let application_root: [u8; 32] = app_h.finalize().into();

    let mut exec_h = Sha256::new();
    exec_h.update(EXEC_ROOT_PREFIX);
    exec_h.update(ctx_bytes);
    exec_h.update(domain_registry_digest);
    exec_h.update(application_root);
    exec_h.update((proto_slots.len() as u16).to_be_bytes());
    for s in &proto_slots {
        exec_h.update(s.encode_canonical());
    }
    let execution_state_root: [u8; 32] = exec_h.finalize().into();

    let execution_root_ref = ExecutionRootRefV1 {
        protocol_version: preimage_context.protocol_version,
        chain_id: preimage_context.chain_id,
        world_id: preimage_context.world_id,
        genesis_id: preimage_context.genesis_id,
        profile_id,
        profile_scope_digest: preimage_context.profile_scope_digest,
        domain_registry_digest,
        execution_state_root,
    };

    Ok(ExecutionRootCompositionV1 {
        composition_version: EXECUTION_ROOT_1_VERSION,
        batch_height,
        epoch,
        profile_id,
        preimage_context,
        domain_registry_digest,
        required_domain_set_digest,
        root_entry_set_digest,
        application_root,
        execution_state_root,
        execution_root_ref,
        slots: slots.to_vec(),
    })
}

/// Verifies complete correspondence between a composed execution root, header [`ExecutionRootRefV1`],
/// governing [`ExecutionDomainRegistryV1`], and [`BoundProfileManifestV1`] (catching mixed-manifest
/// and header-tamper publications fail-closed).
pub fn verify_header_and_manifest_correspondence(
    composition: &ExecutionRootCompositionV1,
    header_root_ref: &ExecutionRootRefV1,
    registry: &ExecutionDomainRegistryV1,
    manifest: &BoundProfileManifestV1,
) -> Result<(), ExecutionRoot1Error> {
    manifest.verify().map_err(|_| {
        ExecutionRoot1Error::MixedManifestRejected("invalid BoundProfileManifestV1")
    })?;
    let recomposed = compose_from_slot_commitments(
        &composition.slots,
        composition.batch_height,
        composition.epoch,
        registry,
        composition.preimage_context,
        composition.profile_id,
    )?;
    if recomposed != *composition {
        return Err(ExecutionRoot1Error::ContextOrHeaderMismatch(
            "recomposed execution root disagrees with supplied composition",
        ));
    }
    if composition.execution_root_ref != *header_root_ref {
        return Err(ExecutionRoot1Error::ContextOrHeaderMismatch(
            "header ExecutionRootRefV1 disagrees with composed execution root",
        ));
    }
    if manifest.profile_id != composition.profile_id
        || manifest.candidate.genesis_id != composition.preimage_context.genesis_id
        || manifest.candidate.genesis_scope_digest
            != composition.preimage_context.profile_scope_digest
    {
        return Err(ExecutionRoot1Error::MixedManifestRejected(
            "manifest profile_id/genesis_id/scope_digest disagrees with composition",
        ));
    }
    let manifest_prog_active = manifest
        .candidate
        .activated_predicates
        .iter()
        .any(|p| p == "program-active");
    if manifest_prog_active != registry.entries[5].is_active() {
        return Err(ExecutionRoot1Error::MixedManifestRejected(
            "manifest program-active predicate disagrees with registry slot 6",
        ));
    }
    Ok(())
}

/// Helper constructing an active [`RootSlotCommitmentV1`] for `tag` with its canonical schema digest.
pub fn active_slot(tag: ExecutionDomainTagV1, root_bytes: [u8; 32]) -> RootSlotCommitmentV1 {
    RootSlotCommitmentV1::ActiveRoot {
        domain_tag: tag,
        layer: tag.expected_layer(),
        root_schema_digest: tag.canonical_root_schema_digest(),
        root_bytes,
    }
}

fn hash_slot_slice(prefix: &[u8], slots: &[RootSlotCommitmentV1]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(prefix);
    h.update((slots.len() as u16).to_be_bytes());
    for s in slots {
        h.update(s.encode_canonical());
    }
    h.finalize().into()
}

fn read_u16_be(bytes: &[u8], off: usize) -> Result<u16, ExecutionRoot1Error> {
    let sl: [u8; 2] = bytes
        .get(off..off + 2)
        .ok_or(ExecutionRoot1Error::InvalidWire("truncated u16"))?
        .try_into()
        .unwrap();
    Ok(u16::from_be_bytes(sl))
}

fn read_u64_be(bytes: &[u8], off: usize) -> Result<u64, ExecutionRoot1Error> {
    let sl: [u8; 8] = bytes
        .get(off..off + 8)
        .ok_or(ExecutionRoot1Error::InvalidWire("truncated u64"))?
        .try_into()
        .unwrap();
    Ok(u64::from_be_bytes(sl))
}

fn read_arr32(bytes: &[u8], off: usize) -> Result<[u8; 32], ExecutionRoot1Error> {
    let sl: [u8; 32] = bytes
        .get(off..off + 32)
        .ok_or(ExecutionRoot1Error::InvalidWire("truncated [u8; 32]"))?
        .try_into()
        .unwrap();
    Ok(sl)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_root_1_registry_two_layer_composition_and_non_interchangeability() {
        let reg = ExecutionDomainRegistryV1::canonical_launch_v1();
        let reg_bytes = reg.encode_canonical().unwrap();
        assert_eq!(
            ExecutionDomainRegistryV1::decode_canonical(&reg_bytes).unwrap(),
            reg
        );

        let snap = ExecutorDomainSnapshotV1 {
            snapshot_version: 1,
            batch_height: 0,
            epoch: 0,
            world_root: [0x11; 32],
            active_token_root: [0x22; 32],
            episode_root: [0x33; 32],
            auth_root: [0x44; 32],
            nonce_root: [0x55; 32],
            native_balance_root: [0x66; 32],
            episode_ownership_root: [0x77; 32],
            governance_root: [0x88; 32],
            economic_stake_root: [0x99; 32],
            validator_schedule_root: [0xAA; 32],
            program_state_active: false,
        };
        let pctx = ExecutionRootPreimageContextV1 {
            protocol_version: 1,
            chain_id: 77,
            world_id: 1,
            genesis_id: [0xB1; 32],
            profile_scope_digest: [0xC2; 32],
        };
        let comp1 = compose_from_executor_snapshot(&snap, &reg, pctx, [0xD3; 32]).unwrap();
        let comp2 = compose_from_executor_snapshot(&snap, &reg, pctx, [0xE4; 32]).unwrap();
        // L1 §5 cycle-free rule: changing external profile_id must NOT change app/exec roots,
        // while ExecutionRootRefV1 MUST differ!
        assert_eq!(comp1.application_root, comp2.application_root);
        assert_eq!(comp1.execution_state_root, comp2.execution_state_root);
        assert_ne!(
            comp1.execution_root_ref.ref_digest(),
            comp2.execution_root_ref.ref_digest()
        );
        let cbytes = comp1.encode_canonical().unwrap();
        assert_eq!(
            ExecutionRootCompositionV1::decode_canonical(&cbytes).unwrap(),
            comp1
        );
        // Non-interchangeability across typed AuthenticatedRootRefV1 domains
        let r_auth = comp1
            .authenticated_domain_ref(ExecutionDomainTagV1::Authorization)
            .unwrap();
        let r_nonce = comp1
            .authenticated_domain_ref(ExecutionDomainTagV1::Nonce)
            .unwrap();
        assert_ne!(r_auth.ref_digest(), r_nonce.ref_digest());
        // Inactive slot 6 has no AuthenticatedRootRefV1
        assert!(matches!(
            comp1.authenticated_domain_ref(ExecutionDomainTagV1::ProgramState),
            Err(ExecutionRoot1Error::ActiveInactiveMismatch(_))
        ));
    }
}
