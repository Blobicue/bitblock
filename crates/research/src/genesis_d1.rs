//! `GENESIS-D1`: reproducible non-self-referential launch genesis configuration (`PS-078`).
//!
//! Realizes `ChainGenesisPayloadV1` (`B_0, M_0, E_0, X_0, A_0, N_0, P_0, G_0, Q_0`, validated
//! `BOUNDS-1` limit vector, and `BootstrapAvailabilityPolicyV1`) and `GenesisPackageV1` from
//! `docs/LAYER1_DESIGN_PAPER.md` §2 without altering `profile_bootstrap.rs` or any core byte:
//! - **Locked v1 launch predicates:** `asset-active`, `economic-charging`, `in-place-upgrade`,
//!   `migration-profile`, `stake-consensus`, `validator-economics`, and `validator-rewards`, with
//!   **`program-active` (on-chain VM) and `verkle-selected` explicitly INACTIVE at genesis**.
//! - **Non-self-referential derivation:** `LaunchGenesisStateV1` excludes `genesis_id`,
//!   `profile_id`, and all derived state roots (`GENESIS-ROOT-PARITY-1` explicitly deferred);
//!   `GenesisPackageV1` binds `(LaunchGenesisStateV1, derived genesis_id)` and rejects any
//!   self-reference, omission, duplication, re-ordering, supply mismatch, or `> 1/3` validator
//!   concentration fail-closed.
//!
//! Reproducible genesis bytes do not prove trusted distribution, fair allocation, usable keys,
//! available archives, or consensus safety.

use std::collections::BTreeSet;
use std::fmt;

use crate::asset_d1::{AssetPolicyDescriptorV1, BPS_DENOMINATOR, ValidatorEconPolicyV1};
use crate::bounds_1::ProtocolBoundsProfileV1;
use crate::commit_state::DomainRegistryV1;
use crate::consensus_gov_d1::GovernancePolicyDescriptorV1;
use crate::crypto_d1::{CanonicalEd25519PublicKeyV1, CryptoD1Error};
use crate::history::world_tag;
use crate::identity_d1::ActorIdV1;
use crate::profile_bootstrap::ProfileScopeV1;
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::resource_1::ResourcePolicyV1;
use crate::world::World;

/// Canonical schema string for a [`BootstrapAvailabilityPolicyV1`] policy object (`GENESIS-D1`).
pub const GENESIS_D1_BOOTSTRAP_POLICY_SCHEMA: &[u8] =
    b"bitblock/genesis-d1/bootstrap-availability-policy/v1";
/// Canonical schema string for a pre-ID [`LaunchGenesisStateV1`] object (`GENESIS-D1`).
pub const GENESIS_D1_LAUNCH_STATE_SCHEMA: &[u8] = b"bitblock/genesis-d1/launch-state/v1";
/// Canonical schema string for a bound [`GenesisPackageV1`] object (`GENESIS-D1`).
pub const GENESIS_D1_PACKAGE_SCHEMA: &[u8] = b"bitblock/genesis-d1/genesis-package/v1";

const PRED_ASSET_ACTIVE: &str = "asset-active";
/// Canonical sorted launch-profile predicate tokens approved for immutable v1 launch genesis.
/// Explicitly excludes `program-active`, `post-genesis-program`, `verkle-selected`, and `always`.
pub const CANONICAL_LAUNCH_PREDICATES_V1: [&str; 7] = [
    PRED_ASSET_ACTIVE,
    "economic-charging",
    "in-place-upgrade",
    "migration-profile",
    "stake-consensus",
    "validator-economics",
    "validator-rewards",
];

/// Non-mainnet deterministic test-genesis identity namespace (`GENESIS-D1`).
pub const DEVNET_V1_IDENTITY_NAMESPACE: &str = "bitblock.devnet.v1";

/// Maximum actors admitted in one genesis state vector (`GENESIS-D1`).
pub const MAX_GENESIS_ACTORS: usize = 1_024;
/// Maximum initial validators admitted in `Q_0` (`GENESIS-D1`).
pub const MAX_GENESIS_VALIDATORS: usize = 256;

/// Errors returned by `GENESIS-D1` validation, encoding, decoding, and package verification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GenesisD1Error {
    /// Cryptographic key parsing or validation failed.
    Crypto(CryptoD1Error),
    /// Logical genesis invariant, predicate closure, domain activation, or stake rule failed.
    InvalidGenesis(&'static str),
    /// Wire buffer was truncated, overlong, had unknown flags, or contained trailing bytes.
    InvalidWire(&'static str),
    /// Derived `genesis_id` did not match the canonical hash of `LaunchGenesisStateV1`.
    GenesisIdMismatch,
    /// Detected a self-reference where `genesis_id` leaked into a pre-ID digest field.
    SelfReferenceDetected,
}

impl From<CryptoD1Error> for GenesisD1Error {
    fn from(value: CryptoD1Error) -> Self {
        Self::Crypto(value)
    }
}

impl fmt::Display for GenesisD1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for GenesisD1Error {}

/// Bootstrap peer diversity, archive retention, and chunking policy (`GENESIS-D1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BootstrapAvailabilityPolicyV1 {
    /// Schema version (`1`).
    pub schema_version: u16,
    /// Minimum diverse bootstrap seed peers (`>= 8`, matching `THREAT-1`).
    pub min_seed_peers: u16,
    /// Minimum archive retention window in epochs (`>= 28`, matching `THREAT-1` / `BOUNDS-1`).
    pub archive_retention_epochs: u32,
    /// Maximum genesis snapshot transfer chunk bytes (`1..=2_097_152`).
    pub max_genesis_chunk_bytes: u32,
    /// Deterministic sync checkpoint interval in blocks (`> 0`).
    pub checkpoint_interval_blocks: u32,
    /// Must be `true`: bootstrap mirrors must serve checksummed genesis chunks.
    pub require_checksummed_mirror: bool,
}

impl BootstrapAvailabilityPolicyV1 {
    /// Fixed canonical big-endian wire length (`17` bytes).
    pub const WIRE_LEN: usize = 17;

    /// Returns the default v1 bootstrap and availability policy aligned with `THREAT-1`/`BOUNDS-1`.
    pub const fn v1_default() -> Self {
        Self {
            schema_version: 1,
            min_seed_peers: 8,
            archive_retention_epochs: 28,
            max_genesis_chunk_bytes: 2 * 1024 * 1024,
            checkpoint_interval_blocks: 1_024,
            require_checksummed_mirror: true,
        }
    }

    /// Validates all peer-diversity, retention, chunk-cap, and checksum invariants.
    pub fn validate(&self) -> Result<(), GenesisD1Error> {
        if self.schema_version != 1 {
            return Err(GenesisD1Error::InvalidGenesis("schema_version must be 1"));
        }
        if self.min_seed_peers < 8 {
            return Err(GenesisD1Error::InvalidGenesis(
                "min_seed_peers must be >= 8 per THREAT-1",
            ));
        }
        if self.archive_retention_epochs < 28 {
            return Err(GenesisD1Error::InvalidGenesis(
                "archive_retention_epochs must be >= 28",
            ));
        }
        if self.max_genesis_chunk_bytes == 0 || self.max_genesis_chunk_bytes > 2 * 1024 * 1024 {
            return Err(GenesisD1Error::InvalidGenesis(
                "max_genesis_chunk_bytes must lie in 1..=2 MiB",
            ));
        }
        if self.checkpoint_interval_blocks == 0 {
            return Err(GenesisD1Error::InvalidGenesis(
                "checkpoint_interval_blocks must be > 0",
            ));
        }
        if !self.require_checksummed_mirror {
            return Err(GenesisD1Error::InvalidGenesis(
                "require_checksummed_mirror must be true",
            ));
        }
        Ok(())
    }

    /// Encodes the bootstrap availability policy into 17 canonical big-endian bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], GenesisD1Error> {
        self.validate()?;
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.schema_version.to_be_bytes());
        out[2..4].copy_from_slice(&self.min_seed_peers.to_be_bytes());
        out[4..8].copy_from_slice(&self.archive_retention_epochs.to_be_bytes());
        out[8..12].copy_from_slice(&self.max_genesis_chunk_bytes.to_be_bytes());
        out[12..16].copy_from_slice(&self.checkpoint_interval_blocks.to_be_bytes());
        out[16] = u8::from(self.require_checksummed_mirror);
        Ok(out)
    }

    /// Decodes and validates a 17-byte canonical [`BootstrapAvailabilityPolicyV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, GenesisD1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(GenesisD1Error::InvalidWire(
                "wrong BootstrapAvailabilityPolicyV1 length",
            ));
        }
        let flag = match bytes[16] {
            0 => false,
            1 => true,
            _ => {
                return Err(GenesisD1Error::InvalidWire(
                    "invalid boolean byte in BootstrapAvailabilityPolicyV1",
                ));
            }
        };
        let ru32 = |o: usize| -> u32 { u32::from_be_bytes(bytes[o..o + 4].try_into().expect("4")) };
        let pol = Self {
            schema_version: u16::from_be_bytes([bytes[0], bytes[1]]),
            min_seed_peers: u16::from_be_bytes([bytes[2], bytes[3]]),
            archive_retention_epochs: ru32(4),
            max_genesis_chunk_bytes: ru32(8),
            checkpoint_interval_blocks: ru32(12),
            require_checksummed_mirror: flag,
        };
        pol.validate()?;
        Ok(pol)
    }

    /// Computes the canonical `Policy` `ContentTag` for this bootstrap availability policy.
    pub fn tag(&self) -> Result<ContentTag, GenesisD1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            GENESIS_D1_BOOTSTRAP_POLICY_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

/// Explicit active/inactive logical domain entry carried in [`LaunchGenesisStateV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GenesisDomainSlotV1 {
    /// Canonical logical domain code (`1..=8`): `1=World`, `2=ActiveToken`, `3=Episode`,
    /// `4=Authorization`, `5=Nonce`, `6=AssetState`, `7=ProgramState`, `8=Verkle`.
    pub domain_code: u8,
    /// `true` when active at genesis; `false` when explicitly bound to an inactive sentinel.
    pub active: bool,
}

impl GenesisDomainSlotV1 {
    /// Returns the exact ordered v1 launch domain matrix (`1..=6` active; `7=ProgramState` and
    /// `8=Verkle` explicitly inactive).
    pub const fn canonical_launch_v1() -> [Self; 8] {
        [
            Self {
                domain_code: 1,
                active: true,
            },
            Self {
                domain_code: 2,
                active: true,
            },
            Self {
                domain_code: 3,
                active: true,
            },
            Self {
                domain_code: 4,
                active: true,
            },
            Self {
                domain_code: 5,
                active: true,
            },
            Self {
                domain_code: 6,
                active: true,
            },
            Self {
                domain_code: 7,
                active: false,
            },
            Self {
                domain_code: 8,
                active: false,
            },
        ]
    }
}

/// Initial pre-genesis actor entry (`A_0`, `N_0`, `X_0`) bound strictly before `genesis_id` exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GenesisActorEntryV1 {
    /// Stable pre-genesis `ActorIdV1`.
    pub actor_id: ActorIdV1,
    /// Canonical SHA-256 digest of the actor's initial `AuthorizationDescriptorV1`.
    pub descriptor_digest: [u8; 32],
    /// Initial sequential actor nonce (must be `0` at genesis).
    pub initial_nonce: u64,
    /// Initial liquid native fungible balance allocated to this actor.
    pub initial_liquid_balance: u128,
}

impl GenesisActorEntryV1 {
    /// Fixed canonical wire length per actor entry (`88` bytes).
    pub const WIRE_LEN: usize = 32 + 32 + 8 + 16;
}

/// Initial authority-selected validator entry (`Q_0`) with separated vote & proposal keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GenesisValidatorEntryV1 {
    /// Stable validator identity digest (distinct from actor identities).
    pub validator_id: [u8; 32],
    /// Strict RFC 8032 Ed25519 consensus voting public key (`ValidatorVote`).
    pub vote_pubkey: CanonicalEd25519PublicKeyV1,
    /// Strict RFC 8032 Ed25519 block proposal public key (`ValidatorProposal`, distinct from vote).
    pub proposal_pubkey: CanonicalEd25519PublicKeyV1,
    /// Initial bonded native stake (`>= min_self_stake` and `<= 3_333 bps` of total stake).
    pub bonded_stake: u128,
}

impl GenesisValidatorEntryV1 {
    /// Fixed canonical wire length per validator entry (`112` bytes).
    pub const WIRE_LEN: usize = 32 + 32 + 32 + 16;
}

/// Complete non-self-referential logical genesis state (`GENESIS-D1`, `B_0..Q_0`).
///
/// Strictly excludes derived `genesis_id`, `profile_id`, and all derived commitment roots
/// (`GENESIS-ROOT-PARITY-1` is explicitly deferred).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LaunchGenesisStateV1 {
    /// Schema version (`1`).
    pub genesis_schema_version: u16,
    /// Pre-ID `profile_scope_digest` from `PROFILE-SCOPE-D0`.
    pub initial_profile_scope_digest: [u8; 32],
    /// Pre-ID `domain_registry_digest` from `COMMIT-1` / `PROFILE-SCOPE-D0`.
    pub initial_domain_registry_digest: [u8; 32],
    /// Chain identifier (`> 0`).
    pub chain_id: u64,
    /// Single `i64³` world identifier (`1`).
    pub world_id: u64,
    /// Non-empty ASCII identity namespace (`1..=64` bytes; e.g. `bitblock.devnet.v1`).
    pub identity_namespace: String,
    /// Initial bedrock-only world fingerprint digest (`B_0`).
    pub initial_world_digest: [u8; 32],
    /// Initial active mutable occupancy-token count (`M_0`, must be `0`).
    pub initial_active_token_count: u32,
    /// Initial recorded occupancy-episode count (`E_0`, must be `0`).
    pub initial_episode_count: u32,
    /// Initial locked refundable storage deposits (must be `0`).
    pub initial_locked_deposits: u128,
    /// Digest of `ResourcePolicyV1` (`P_0`, satisfies `CE-08`).
    pub resource_policy_digest: [u8; 32],
    /// Digest of `AssetPolicyDescriptorV1` (`P_0`, satisfies `CE-06`).
    pub asset_policy_digest: [u8; 32],
    /// Digest of `ValidatorEconPolicyV1` (`P_0`, satisfies `CE-09`).
    pub validator_econ_digest: [u8; 32],
    /// Digest of `GovernancePolicyDescriptorV1` (`G_0`).
    pub governance_policy_digest: [u8; 32],
    /// Digest of validated `ProtocolBoundsProfileV1` (`BOUNDS-1`).
    pub validated_limit_vector_digest: [u8; 32],
    /// Digest of `BootstrapAvailabilityPolicyV1`.
    pub bootstrap_availability_digest: [u8; 32],
    /// Exact sorted launch-profile predicate tokens (`program-active` & `verkle-selected` absent).
    pub launch_predicates: Vec<String>,
    /// Explicit active/inactive domain matrix (`1..=6` active, `7..=8` inactive).
    pub domain_entries: Vec<GenesisDomainSlotV1>,
    /// Strictly sorted unique initial actors (`A_0`, `N_0`, `X_0`).
    pub initial_actors: Vec<GenesisActorEntryV1>,
    /// Strictly sorted unique initial validators (`Q_0`).
    pub initial_validators: Vec<GenesisValidatorEntryV1>,
    /// Total initial native supply (`== sum(liquid) + sum(bonded_stake)`).
    pub initial_native_supply: u128,
}

impl LaunchGenesisStateV1 {
    /// Constructs a validated deterministic v1 devnet launch genesis state from upstream policies.
    #[allow(clippy::too_many_arguments)]
    pub fn build_launch_v1(
        scope: &ProfileScopeV1,
        domain_registry: &DomainRegistryV1,
        resource_policy: &ResourcePolicyV1,
        asset_policy: &AssetPolicyDescriptorV1,
        validator_econ: &ValidatorEconPolicyV1,
        governance: &GovernancePolicyDescriptorV1,
        bounds: &ProtocolBoundsProfileV1,
        bootstrap: &BootstrapAvailabilityPolicyV1,
        chain_id: u64,
        identity_namespace: String,
        initial_actors: Vec<GenesisActorEntryV1>,
        initial_validators: Vec<GenesisValidatorEntryV1>,
    ) -> Result<Self, GenesisD1Error> {
        let mut supply = 0u128;
        for a in &initial_actors {
            supply = supply.checked_add(a.initial_liquid_balance).ok_or(
                GenesisD1Error::InvalidGenesis("actor balance supply overflow"),
            )?;
        }
        for v in &initial_validators {
            supply = supply
                .checked_add(v.bonded_stake)
                .ok_or(GenesisD1Error::InvalidGenesis(
                    "validator stake supply overflow",
                ))?;
        }
        let state = Self {
            genesis_schema_version: 1,
            initial_profile_scope_digest: scope
                .tag()
                .map_err(|_| GenesisD1Error::InvalidGenesis("invalid ProfileScopeV1"))?
                .digest(),
            initial_domain_registry_digest: domain_registry.tag().digest(),
            chain_id,
            world_id: 1,
            identity_namespace,
            initial_world_digest: world_tag(&World::new()).digest(),
            initial_active_token_count: 0,
            initial_episode_count: 0,
            initial_locked_deposits: 0,
            resource_policy_digest: resource_policy
                .tag()
                .map_err(|_| GenesisD1Error::InvalidGenesis("invalid ResourcePolicyV1"))?
                .digest(),
            asset_policy_digest: asset_policy
                .tag()
                .map_err(|_| GenesisD1Error::InvalidGenesis("invalid AssetPolicyDescriptorV1"))?
                .digest(),
            validator_econ_digest: validator_econ
                .tag()
                .map_err(|_| GenesisD1Error::InvalidGenesis("invalid ValidatorEconPolicyV1"))?
                .digest(),
            governance_policy_digest: governance
                .tag()
                .map_err(|_| {
                    GenesisD1Error::InvalidGenesis("invalid GovernancePolicyDescriptorV1")
                })?
                .digest(),
            validated_limit_vector_digest: bounds
                .tag()
                .map_err(|_| GenesisD1Error::InvalidGenesis("invalid ProtocolBoundsProfileV1"))?
                .digest(),
            bootstrap_availability_digest: bootstrap.tag()?.digest(),
            launch_predicates: scope.activated_predicates.clone(),
            domain_entries: GenesisDomainSlotV1::canonical_launch_v1().to_vec(),
            initial_actors,
            initial_validators,
            initial_native_supply: supply,
        };
        state.validate(Some(asset_policy), Some(validator_econ))?;
        Ok(state)
    }

    /// Returns all 9 pre-ID 32-byte digest slots for self-reference checking.
    pub fn pre_id_digests(&self) -> [[u8; 32]; 9] {
        [
            self.initial_profile_scope_digest,
            self.initial_domain_registry_digest,
            self.initial_world_digest,
            self.resource_policy_digest,
            self.asset_policy_digest,
            self.validator_econ_digest,
            self.governance_policy_digest,
            self.validated_limit_vector_digest,
            self.bootstrap_availability_digest,
        ]
    }

    /// Validates all logical pre-ID, predicate, domain, actor, validator, BFT `< 1/3`, and supply
    /// conservation invariants fail-closed.
    pub fn validate(
        &self,
        asset: Option<&AssetPolicyDescriptorV1>,
        econ: Option<&ValidatorEconPolicyV1>,
    ) -> Result<(), GenesisD1Error> {
        if self.genesis_schema_version != 1 {
            return Err(GenesisD1Error::InvalidGenesis(
                "genesis_schema_version must be 1",
            ));
        }
        if self.chain_id == 0 || self.world_id != 1 {
            return Err(GenesisD1Error::InvalidGenesis(
                "chain_id must be > 0 and world_id must be 1",
            ));
        }
        let ns = self.identity_namespace.as_bytes();
        if ns.is_empty() || ns.len() > 64 || !ns.is_ascii() {
            return Err(GenesisD1Error::InvalidGenesis(
                "identity_namespace must be 1..=64 ASCII bytes",
            ));
        }
        if self.pre_id_digests().contains(&[0u8; 32]) {
            return Err(GenesisD1Error::InvalidGenesis(
                "all genesis pre-ID digest slots must be non-zero",
            ));
        }
        if self.initial_world_digest != world_tag(&World::new()).digest() {
            return Err(GenesisD1Error::InvalidGenesis(
                "B_0 initial_world_digest must equal canonical bedrock-only World::new()",
            ));
        }
        if self.initial_active_token_count != 0
            || self.initial_episode_count != 0
            || self.initial_locked_deposits != 0
        {
            return Err(GenesisD1Error::InvalidGenesis(
                "M_0, E_0, and initial_locked_deposits must be 0 at genesis",
            ));
        }
        let expected_preds: Vec<&str> = CANONICAL_LAUNCH_PREDICATES_V1.to_vec();
        let actual_preds: Vec<&str> = self.launch_predicates.iter().map(String::as_str).collect();
        if actual_preds != expected_preds {
            return Err(GenesisD1Error::InvalidGenesis(
                "launch_predicates must match canonical v1 launch set (program-active & verkle-selected inactive)",
            ));
        }
        if self.domain_entries != GenesisDomainSlotV1::canonical_launch_v1() {
            return Err(GenesisD1Error::InvalidGenesis(
                "domain_entries must match canonical launch active/inactive domain matrix",
            ));
        }
        if self.initial_actors.is_empty() || self.initial_actors.len() > MAX_GENESIS_ACTORS {
            return Err(GenesisD1Error::InvalidGenesis(
                "initial_actors count must lie in 1..=MAX_GENESIS_ACTORS",
            ));
        }
        let mut liquid_sum = 0u128;
        for (i, a) in self.initial_actors.iter().enumerate() {
            if a.descriptor_digest == [0u8; 32] {
                return Err(GenesisD1Error::InvalidGenesis(
                    "actor descriptor_digest must be non-zero",
                ));
            }
            if a.initial_nonce != 0 {
                return Err(GenesisD1Error::InvalidGenesis(
                    "initial actor nonce N_0 must be 0",
                ));
            }
            if i > 0 && self.initial_actors[i - 1].actor_id >= a.actor_id {
                return Err(GenesisD1Error::InvalidGenesis(
                    "initial_actors must be strictly sorted by unique actor_id",
                ));
            }
            liquid_sum = liquid_sum
                .checked_add(a.initial_liquid_balance)
                .ok_or(GenesisD1Error::InvalidGenesis("liquid supply overflow"))?;
        }
        if self.initial_validators.len() < 4
            || self.initial_validators.len() > MAX_GENESIS_VALIDATORS
        {
            return Err(GenesisD1Error::InvalidGenesis(
                "Q_0 must contain 4..=MAX_GENESIS_VALIDATORS validators for BFT < 1/3 safety",
            ));
        }
        let mut total_stake = 0u128;
        let mut used_keys = BTreeSet::new();
        for (i, v) in self.initial_validators.iter().enumerate() {
            if v.validator_id == [0u8; 32] {
                return Err(GenesisD1Error::InvalidGenesis(
                    "validator_id must be non-zero",
                ));
            }
            if i > 0 && self.initial_validators[i - 1].validator_id >= v.validator_id {
                return Err(GenesisD1Error::InvalidGenesis(
                    "initial_validators must be strictly sorted by unique validator_id",
                ));
            }
            if v.vote_pubkey == v.proposal_pubkey
                || !used_keys.insert(v.vote_pubkey)
                || !used_keys.insert(v.proposal_pubkey)
            {
                return Err(GenesisD1Error::InvalidGenesis(
                    "validator vote and proposal keys must be globally distinct",
                ));
            }
            if v.bonded_stake == 0 {
                return Err(GenesisD1Error::InvalidGenesis(
                    "validator bonded_stake must be > 0",
                ));
            }
            if let Some(ec) = econ
                && v.bonded_stake < ec.min_self_stake
            {
                return Err(GenesisD1Error::InvalidGenesis(
                    "validator bonded_stake below VALIDATOR-ECON-D1 min_self_stake",
                ));
            }
            total_stake = total_stake
                .checked_add(v.bonded_stake)
                .ok_or(GenesisD1Error::InvalidGenesis("total stake overflow"))?;
        }
        let max_bps = u128::from(econ.map_or(3_333, |e| e.max_single_validator_bps));
        for v in &self.initial_validators {
            let bps = v
                .bonded_stake
                .checked_mul(u128::from(BPS_DENOMINATOR))
                .ok_or(GenesisD1Error::InvalidGenesis("bps mul overflow"))?
                / total_stake;
            if bps > max_bps {
                return Err(GenesisD1Error::InvalidGenesis(
                    "validator stake share exceeds BFT < 1/3 concentration ceiling",
                ));
            }
        }
        let computed_supply = liquid_sum
            .checked_add(total_stake)
            .ok_or(GenesisD1Error::InvalidGenesis("total supply overflow"))?;
        if computed_supply == 0 || computed_supply != self.initial_native_supply {
            return Err(GenesisD1Error::InvalidGenesis(
                "initial_native_supply must equal exact sum of liquid balances + bonded stake",
            ));
        }
        if let Some(a) = asset
            && self.initial_native_supply > a.max_native_supply
        {
            return Err(GenesisD1Error::InvalidGenesis(
                "initial_native_supply exceeds ASSET-D1 max_native_supply",
            ));
        }
        Ok(())
    }

    /// Encodes `LaunchGenesisStateV1` into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, GenesisD1Error> {
        self.validate(None, None)?;
        let ns = self.identity_namespace.as_bytes();
        let mut out = Vec::with_capacity(
            512 + self.initial_actors.len() * GenesisActorEntryV1::WIRE_LEN
                + self.initial_validators.len() * GenesisValidatorEntryV1::WIRE_LEN,
        );
        let mut put = |s: &[u8]| out.extend_from_slice(s);
        put(&self.genesis_schema_version.to_be_bytes());
        put(&self.initial_profile_scope_digest);
        put(&self.initial_domain_registry_digest);
        put(&self.chain_id.to_be_bytes());
        put(&self.world_id.to_be_bytes());
        put(&(ns.len() as u16).to_be_bytes());
        put(ns);
        put(&self.initial_world_digest);
        put(&self.initial_active_token_count.to_be_bytes());
        put(&self.initial_episode_count.to_be_bytes());
        put(&self.initial_locked_deposits.to_be_bytes());
        put(&self.resource_policy_digest);
        put(&self.asset_policy_digest);
        put(&self.validator_econ_digest);
        put(&self.governance_policy_digest);
        put(&self.validated_limit_vector_digest);
        put(&self.bootstrap_availability_digest);
        out.extend_from_slice(&(self.launch_predicates.len() as u16).to_be_bytes());
        for p in &self.launch_predicates {
            let pb = p.as_bytes();
            out.extend_from_slice(&(pb.len() as u16).to_be_bytes());
            out.extend_from_slice(pb);
        }
        out.extend_from_slice(&(self.domain_entries.len() as u16).to_be_bytes());
        for d in &self.domain_entries {
            out.push(d.domain_code);
            out.push(u8::from(d.active));
        }
        out.extend_from_slice(&(self.initial_actors.len() as u32).to_be_bytes());
        for a in &self.initial_actors {
            out.extend_from_slice(a.actor_id.as_bytes());
            out.extend_from_slice(&a.descriptor_digest);
            out.extend_from_slice(&a.initial_nonce.to_be_bytes());
            out.extend_from_slice(&a.initial_liquid_balance.to_be_bytes());
        }
        out.extend_from_slice(&(self.initial_validators.len() as u32).to_be_bytes());
        for v in &self.initial_validators {
            out.extend_from_slice(&v.validator_id);
            out.extend_from_slice(v.vote_pubkey.as_bytes());
            out.extend_from_slice(v.proposal_pubkey.as_bytes());
            out.extend_from_slice(&v.bonded_stake.to_be_bytes());
        }
        out.extend_from_slice(&self.initial_native_supply.to_be_bytes());
        Ok(out)
    }

    /// Decodes and validates a canonical `LaunchGenesisStateV1`.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, GenesisD1Error> {
        let mut cur = 0usize;
        let take = |n: usize, c: &mut usize| -> Result<&[u8], GenesisD1Error> {
            if bytes.len().saturating_sub(*c) < n {
                return Err(GenesisD1Error::InvalidWire(
                    "truncated LaunchGenesisStateV1",
                ));
            }
            let sl = &bytes[*c..*c + n];
            *c += n;
            Ok(sl)
        };
        let r16 = |c: &mut usize| -> Result<u16, GenesisD1Error> {
            Ok(u16::from_be_bytes(take(2, c)?.try_into().expect("2")))
        };
        let r32u = |c: &mut usize| -> Result<u32, GenesisD1Error> {
            Ok(u32::from_be_bytes(take(4, c)?.try_into().expect("4")))
        };
        let r64 = |c: &mut usize| -> Result<u64, GenesisD1Error> {
            Ok(u64::from_be_bytes(take(8, c)?.try_into().expect("8")))
        };
        let r128 = |c: &mut usize| -> Result<u128, GenesisD1Error> {
            Ok(u128::from_be_bytes(take(16, c)?.try_into().expect("16")))
        };
        let r32b = |c: &mut usize| -> Result<[u8; 32], GenesisD1Error> {
            Ok(take(32, c)?.try_into().expect("32"))
        };

        let genesis_schema_version = r16(&mut cur)?;
        let initial_profile_scope_digest = r32b(&mut cur)?;
        let initial_domain_registry_digest = r32b(&mut cur)?;
        let chain_id = r64(&mut cur)?;
        let world_id = r64(&mut cur)?;
        let ns_len = r16(&mut cur)? as usize;
        let identity_namespace = std::str::from_utf8(take(ns_len, &mut cur)?)
            .map_err(|_| GenesisD1Error::InvalidWire("non-utf8 identity_namespace"))?
            .to_string();
        let initial_world_digest = r32b(&mut cur)?;
        let initial_active_token_count = r32u(&mut cur)?;
        let initial_episode_count = r32u(&mut cur)?;
        let initial_locked_deposits = r128(&mut cur)?;
        let resource_policy_digest = r32b(&mut cur)?;
        let asset_policy_digest = r32b(&mut cur)?;
        let validator_econ_digest = r32b(&mut cur)?;
        let governance_policy_digest = r32b(&mut cur)?;
        let validated_limit_vector_digest = r32b(&mut cur)?;
        let bootstrap_availability_digest = r32b(&mut cur)?;

        let p_count = r16(&mut cur)? as usize;
        if p_count > 32 {
            return Err(GenesisD1Error::InvalidWire("excessive predicate count"));
        }
        let mut launch_predicates = Vec::with_capacity(p_count);
        for _ in 0..p_count {
            let plen = r16(&mut cur)? as usize;
            let s = std::str::from_utf8(take(plen, &mut cur)?)
                .map_err(|_| GenesisD1Error::InvalidWire("non-utf8 predicate"))?
                .to_string();
            launch_predicates.push(s);
        }

        let d_count = r16(&mut cur)? as usize;
        if d_count > 32 {
            return Err(GenesisD1Error::InvalidWire("excessive domain count"));
        }
        let mut domain_entries = Vec::with_capacity(d_count);
        for _ in 0..d_count {
            let pair = take(2, &mut cur)?;
            let active = match pair[1] {
                0 => false,
                1 => true,
                _ => return Err(GenesisD1Error::InvalidWire("invalid domain active flag")),
            };
            domain_entries.push(GenesisDomainSlotV1 {
                domain_code: pair[0],
                active,
            });
        }

        let a_count = r32u(&mut cur)? as usize;
        if a_count > MAX_GENESIS_ACTORS {
            return Err(GenesisD1Error::InvalidWire("excessive actor count"));
        }
        let mut initial_actors = Vec::with_capacity(a_count);
        for _ in 0..a_count {
            initial_actors.push(GenesisActorEntryV1 {
                actor_id: ActorIdV1::from_bytes(r32b(&mut cur)?),
                descriptor_digest: r32b(&mut cur)?,
                initial_nonce: r64(&mut cur)?,
                initial_liquid_balance: r128(&mut cur)?,
            });
        }

        let v_count = r32u(&mut cur)? as usize;
        if v_count > MAX_GENESIS_VALIDATORS {
            return Err(GenesisD1Error::InvalidWire("excessive validator count"));
        }
        let mut initial_validators = Vec::with_capacity(v_count);
        for _ in 0..v_count {
            let validator_id = r32b(&mut cur)?;
            let vote_pubkey = CanonicalEd25519PublicKeyV1::from_bytes(r32b(&mut cur)?)?;
            let proposal_pubkey = CanonicalEd25519PublicKeyV1::from_bytes(r32b(&mut cur)?)?;
            let bonded_stake = r128(&mut cur)?;
            initial_validators.push(GenesisValidatorEntryV1 {
                validator_id,
                vote_pubkey,
                proposal_pubkey,
                bonded_stake,
            });
        }
        let initial_native_supply = r128(&mut cur)?;
        if cur != bytes.len() {
            return Err(GenesisD1Error::InvalidWire(
                "trailing LaunchGenesisStateV1 bytes",
            ));
        }

        let state = Self {
            genesis_schema_version,
            initial_profile_scope_digest,
            initial_domain_registry_digest,
            chain_id,
            world_id,
            identity_namespace,
            initial_world_digest,
            initial_active_token_count,
            initial_episode_count,
            initial_locked_deposits,
            resource_policy_digest,
            asset_policy_digest,
            validator_econ_digest,
            governance_policy_digest,
            validated_limit_vector_digest,
            bootstrap_availability_digest,
            launch_predicates,
            domain_entries,
            initial_actors,
            initial_validators,
            initial_native_supply,
        };
        state.validate(None, None)?;
        Ok(state)
    }

    /// Computes the canonical `Policy` `ContentTag` whose 32-byte digest is the immutable
    /// `genesis_id` derived strictly after all pre-ID logical fields freeze.
    pub fn tag(&self) -> Result<ContentTag, GenesisD1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            GENESIS_D1_LAUNCH_STATE_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and bounded length"))
    }
}

/// Canonical `GenesisPackageV1 = Canonical(LaunchGenesisStateV1, derived genesis_id)`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GenesisPackageV1 {
    /// Complete immutable pre-ID logical genesis payload.
    pub state: LaunchGenesisStateV1,
    /// Derived immutable `genesis_id` (`state.tag()?.digest()`).
    pub genesis_id: [u8; 32],
}

impl GenesisPackageV1 {
    /// Packages a validated [`LaunchGenesisStateV1`] with its derived `genesis_id`.
    pub fn seal(state: LaunchGenesisStateV1) -> Result<Self, GenesisD1Error> {
        let genesis_id = state.tag()?.digest();
        let pkg = Self { state, genesis_id };
        pkg.verify()?;
        Ok(pkg)
    }

    /// Verifies non-self-reference and exact `genesis_id` reconstruction fail-closed.
    pub fn verify(&self) -> Result<(), GenesisD1Error> {
        self.state.validate(None, None)?;
        if self.state.pre_id_digests().contains(&self.genesis_id) {
            return Err(GenesisD1Error::SelfReferenceDetected);
        }
        let expected = self.state.tag()?.digest();
        if self.genesis_id != expected {
            return Err(GenesisD1Error::GenesisIdMismatch);
        }
        Ok(())
    }

    /// Encodes the sealed genesis package into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, GenesisD1Error> {
        self.verify()?;
        let state_bytes = self.state.encode_canonical()?;
        let mut out = Vec::with_capacity(4 + state_bytes.len() + 32);
        out.extend_from_slice(&(state_bytes.len() as u32).to_be_bytes());
        out.extend_from_slice(&state_bytes);
        out.extend_from_slice(&self.genesis_id);
        Ok(out)
    }

    /// Decodes and verifies a canonical [`GenesisPackageV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, GenesisD1Error> {
        if bytes.len() < 36 {
            return Err(GenesisD1Error::InvalidWire("truncated GenesisPackageV1"));
        }
        let slen = u32::from_be_bytes(bytes[0..4].try_into().expect("4")) as usize;
        if bytes.len() != 4 + slen + 32 {
            return Err(GenesisD1Error::InvalidWire(
                "GenesisPackageV1 length mismatch",
            ));
        }
        let state = LaunchGenesisStateV1::decode_canonical(&bytes[4..4 + slen])?;
        let genesis_id: [u8; 32] = bytes[4 + slen..4 + slen + 32].try_into().expect("32");
        let pkg = Self { state, genesis_id };
        pkg.verify()?;
        Ok(pkg)
    }

    /// Computes the canonical `Receipt` `ContentTag` for this sealed genesis package.
    pub fn tag(&self) -> Result<ContentTag, GenesisD1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            GENESIS_D1_PACKAGE_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and bounded length"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genesis_d1_package_roundtrip_and_guards() {
        let boot = BootstrapAvailabilityPolicyV1::v1_default();
        let raw_b = boot.encode_canonical().unwrap();
        assert_eq!(
            BootstrapAvailabilityPolicyV1::decode_canonical(&raw_b).unwrap(),
            boot
        );

        let validators = (1u8..=4)
            .map(|i| GenesisValidatorEntryV1 {
                validator_id: [i; 32],
                vote_pubkey: CanonicalEd25519PublicKeyV1::from_secret_seed(&[100 + i; 32]).unwrap(),
                proposal_pubkey: CanonicalEd25519PublicKeyV1::from_secret_seed(&[200 + i; 32])
                    .unwrap(),
                bonded_stake: 25_000_000_000,
            })
            .collect::<Vec<_>>();
        let actors = vec![GenesisActorEntryV1 {
            actor_id: ActorIdV1::from_bytes([0x10; 32]),
            descriptor_digest: [0x20; 32],
            initial_nonce: 0,
            initial_liquid_balance: 50_000_000_000,
        }];
        let state = LaunchGenesisStateV1 {
            genesis_schema_version: 1,
            initial_profile_scope_digest: [0x01; 32],
            initial_domain_registry_digest: [0x02; 32],
            chain_id: 1,
            world_id: 1,
            identity_namespace: DEVNET_V1_IDENTITY_NAMESPACE.to_string(),
            initial_world_digest: world_tag(&World::new()).digest(),
            initial_active_token_count: 0,
            initial_episode_count: 0,
            initial_locked_deposits: 0,
            resource_policy_digest: [0x03; 32],
            asset_policy_digest: [0x04; 32],
            validator_econ_digest: [0x05; 32],
            governance_policy_digest: [0x06; 32],
            validated_limit_vector_digest: [0x07; 32],
            bootstrap_availability_digest: boot.tag().unwrap().digest(),
            launch_predicates: CANONICAL_LAUNCH_PREDICATES_V1
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            domain_entries: GenesisDomainSlotV1::canonical_launch_v1().to_vec(),
            initial_actors: actors,
            initial_validators: validators,
            initial_native_supply: 150_000_000_000,
        };
        let pkg = GenesisPackageV1::seal(state.clone()).unwrap();
        let raw_p = pkg.encode_canonical().unwrap();
        assert_eq!(GenesisPackageV1::decode_canonical(&raw_p).unwrap(), pkg);

        // Self-reference guard rejects injecting genesis_id into any pre-ID digest slot.
        let mut bad = pkg.clone();
        bad.state.resource_policy_digest = pkg.genesis_id;
        assert_eq!(bad.verify(), Err(GenesisD1Error::SelfReferenceDetected));
    }
}
