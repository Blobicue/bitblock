//! `PROFILE-SCOPE-D0`, `GENESIS-D1`, and `PROTOCOL-ID-1` non-self-referential pre-ID pipeline.
//!
//! Implements the exact acyclic pre-ID -> genesis -> protocol-identifier pipeline specified in
//! `docs/LAYER1_DESIGN_PAPER.md` §2 and `docs/CAPABILITY_TREE.md`:
//! 1. **`ProfileScopeV1` (`PROFILE-SCOPE-D0`):** binds `BaseTransitionSpecD1`, `DomainRegistryV1`,
//!    `CRYPTO-D1`, `ASSET-D1`, `VALIDATOR-ECON-D1`, `CONSENSUS-D1`, `GOVERNANCE-D1`, proposed
//!    numeric limits, and sorted predicate tokens strictly *before* any derived `genesis_id` or
//!    `profile_id` exists.
//! 2. **`ChainGenesisPayloadV1` (`GENESIS-D1`):** binds `profile_scope_digest`,
//!    `domain_registry_digest`, `chain_id`, `world_id`, `identity_namespace`, initial `world_tag`,
//!    pre-genesis actor set root, initial validator set root, and initial native supply, deriving
//!    one non-circular `genesis_id`.
//! 3. **`AcceptedProposalCoreV1` & `SignedTransactionV1` (`PROTOCOL-ID-1`):** separates one
//!    signature-free unsigned core from bounded typed credentials, enforces
//!    [`AcceptedParentCollisionDispositionV1`] (duplicate credentials normalize without weight
//!    multiplication; distinct unsigned cores safe-halt), and checks protocol ID graph acyclicity.

use std::collections::BTreeSet;
use std::fmt;

use crate::asset_d1::{AssetPolicyDescriptorV1, ValidatorEconPolicyV1};
use crate::commit_state::DomainRegistryV1;
use crate::consensus_gov_d1::{ConsensusPolicyDescriptorV1, GovernancePolicyDescriptorV1};
use crate::crypto_d1::{
    CanonicalEd25519PublicKeyV1, CanonicalEd25519SignatureV1, CryptoD1Error,
    CryptoSuiteDescriptorV1, HashSuiteIdV1, SignatureSuiteIdV1, SigningDomainTagV1,
    SigningPreimageV1, verify_preimage_strict,
};
use crate::identity_d1::{IdentityD1Error, TransactionBodyV1};
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::transition_spec::BaseTransitionSpecD1;

/// Canonical schema string for a pre-ID [`ProfileScopeV1`] object (`PROFILE-SCOPE-D0`).
pub const PROFILE_SCOPE_D0_SCHEMA: &[u8] = b"bitblock/profile-scope-d0/scope/v1";
/// Canonical schema string for a non-self-referential [`ChainGenesisPayloadV1`] (`GENESIS-D1`).
pub const GENESIS_D1_PAYLOAD_SCHEMA: &[u8] = b"bitblock/genesis-d1/chain-genesis-payload/v1";
/// Canonical schema string for a signature-free [`AcceptedProposalCoreV1`] (`PROTOCOL-ID-1`).
pub const PROTOCOL_ID_1_PROPOSAL_CORE_SCHEMA: &[u8] =
    b"bitblock/protocol-id-1/accepted-proposal-core/v1";

/// Maximum number of predicate tokens admitted in one [`ProfileScopeV1`].
pub const MAX_SCOPE_PREDICATES: usize = 32;

/// Registered conditional predicate tokens recognized by `CAPABILITY_TREE.md` (excluding reserved
/// `always`), paired with the `PE-01`..`PE-10` canonical entailment table.
const REGISTERED_PREDICATES: &str = concat!(
    "asset-active economic-charging in-place-upgrade migration-profile ",
    "post-genesis-program program-active stake-consensus validator-economics ",
    "validator-rewards verkle-selected"
);

/// Direct predicate entailments (`PE-01`..`PE-10` from `CAPABILITY_TREE.md`): `(antecedent, entailed)`.
const PREDICATE_ENTAILMENTS: &[(&str, &str)] = &[
    ("economic-charging", "asset-active"),
    ("stake-consensus", "validator-economics"),
    ("stake-consensus", "asset-active"),
    ("stake-consensus", "economic-charging"),
    ("validator-rewards", "validator-economics"),
    ("validator-rewards", "asset-active"),
    ("validator-rewards", "economic-charging"),
    ("post-genesis-program", "program-active"),
    ("post-genesis-program", "migration-profile"),
    ("in-place-upgrade", "migration-profile"),
];

/// Errors returned by `PROFILE-SCOPE-D0`, `GENESIS-D1`, and `PROTOCOL-ID-1` checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BootstrapError {
    /// Cryptographic validation failed.
    Crypto(CryptoD1Error),
    /// Identity or transaction body validation failed.
    Identity(IdentityD1Error),
    /// Scope, genesis, or proposal-core invariant failed.
    InvalidContract(&'static str),
    /// Wire buffer was truncated, overlong, or non-canonical.
    InvalidWire(&'static str),
    /// Predicate token was unregistered, reserved (`always`), unsorted, or duplicated.
    InvalidPredicateToken(String),
    /// Detected a cycle or self-reference in the protocol identifier dependency graph.
    IdentifierCycleDetected,
}

impl From<CryptoD1Error> for BootstrapError {
    fn from(value: CryptoD1Error) -> Self {
        Self::Crypto(value)
    }
}

impl From<IdentityD1Error> for BootstrapError {
    fn from(value: IdentityD1Error) -> Self {
        Self::Identity(value)
    }
}

impl fmt::Display for BootstrapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for BootstrapError {}

/// Proposed numeric admission bounds carried inside [`ProfileScopeV1`] prior to `BOUNDS-1` audit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProposedLimitVectorV1 {
    /// Maximum encoded transaction bytes (`> 0`).
    pub max_tx_bytes: u32,
    /// Maximum transactions per block (`> 0`).
    pub max_block_txs: u32,
    /// Maximum occupancy operations per block (`> 0`).
    pub max_block_ops: u32,
    /// Maximum context-bound proof bytes (`> 0`).
    pub max_proof_bytes: u32,
    /// Maximum new state writes per block (`> 0`).
    pub max_state_writes_per_block: u32,
}

impl ProposedLimitVectorV1 {
    /// Returns the default v1 proposed numeric limit vector aligned with `LOCAL-BOUNDS-1`/`COMMIT-1`.
    pub const fn v1_default() -> Self {
        Self {
            max_tx_bytes: 16_384,
            max_block_txs: 1_024,
            max_block_ops: 65_536,
            max_proof_bytes: 12_288,
            max_state_writes_per_block: 65_536,
        }
    }

    /// Validates that all proposed limits are strictly positive.
    pub fn validate(&self) -> Result<(), BootstrapError> {
        if self.max_tx_bytes == 0
            || self.max_block_txs == 0
            || self.max_block_ops == 0
            || self.max_proof_bytes == 0
            || self.max_state_writes_per_block == 0
        {
            return Err(BootstrapError::InvalidContract(
                "all proposed limits must be > 0",
            ));
        }
        Ok(())
    }
}

/// Canonical pre-ID configuration scope (`PROFILE-SCOPE-D0`), strictly excluding derived
/// `genesis_id` and `profile_id`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProfileScopeV1 {
    /// Schema version (`1`).
    pub schema_version: u16,
    /// Digest of `BaseTransitionSpecD1`.
    pub base_transition_spec_digest: [u8; 32],
    /// Digest of `DomainRegistryV1`.
    pub domain_registry_digest: [u8; 32],
    /// Digest of `CryptoSuiteDescriptorV1`.
    pub crypto_suite_digest: [u8; 32],
    /// Digest of `AssetPolicyDescriptorV1`.
    pub asset_policy_digest: [u8; 32],
    /// Digest of `ValidatorEconPolicyV1`.
    pub validator_econ_digest: [u8; 32],
    /// Digest of `ConsensusPolicyDescriptorV1`.
    pub consensus_policy_digest: [u8; 32],
    /// Digest of `GovernancePolicyDescriptorV1`.
    pub governance_policy_digest: [u8; 32],
    /// Proposed numeric bounds awaiting `BOUNDS-1` adversarial proof.
    pub proposed_limits: ProposedLimitVectorV1,
    /// Strictly sorted unique registered predicate tokens (never containing `always`).
    pub activated_predicates: Vec<String>,
}

impl ProfileScopeV1 {
    /// Builds a validated v1 pre-ID scope from the frozen Wave 1 contract descriptors.
    pub fn from_wave1_contracts(
        domain_registry: &DomainRegistryV1,
        governance: &GovernancePolicyDescriptorV1,
        activated_predicates: Vec<String>,
    ) -> Result<Self, BootstrapError> {
        let base_spec = BaseTransitionSpecD1::canonical_base_only()
            .tag()
            .map_err(|_| BootstrapError::InvalidContract("base transition spec tag"))?
            .digest();
        let domain_reg = domain_registry.tag().digest();
        let crypto = CryptoSuiteDescriptorV1::v1_ed25519_sha256().tag()?.digest();
        let asset = AssetPolicyDescriptorV1::v1_dual_domain_default()
            .tag()
            .map_err(|_| BootstrapError::InvalidContract("asset policy tag"))?
            .digest();
        let econ = ValidatorEconPolicyV1::v1_pos_default()
            .tag()
            .map_err(|_| BootstrapError::InvalidContract("validator econ tag"))?
            .digest();
        let cons = ConsensusPolicyDescriptorV1::v1_bft_pos_default()
            .tag()
            .map_err(|_| BootstrapError::InvalidContract("consensus policy tag"))?
            .digest();
        let gov = governance
            .tag()
            .map_err(|_| BootstrapError::InvalidContract("governance policy tag"))?
            .digest();
        let scope = Self {
            schema_version: 1,
            base_transition_spec_digest: base_spec,
            domain_registry_digest: domain_reg,
            crypto_suite_digest: crypto,
            asset_policy_digest: asset,
            validator_econ_digest: econ,
            consensus_policy_digest: cons,
            governance_policy_digest: gov,
            proposed_limits: ProposedLimitVectorV1::v1_default(),
            activated_predicates,
        };
        scope.validate()?;
        Ok(scope)
    }

    /// Validates all non-zero digest slots, limit bounds, and registered sorted predicates.
    pub fn validate(&self) -> Result<(), BootstrapError> {
        if self.schema_version != 1 {
            return Err(BootstrapError::InvalidContract("schema_version must be 1"));
        }
        let digests = [
            self.base_transition_spec_digest,
            self.domain_registry_digest,
            self.crypto_suite_digest,
            self.asset_policy_digest,
            self.validator_econ_digest,
            self.consensus_policy_digest,
            self.governance_policy_digest,
        ];
        if digests.contains(&[0u8; 32]) {
            return Err(BootstrapError::InvalidContract(
                "all pre-ID component digests must be non-zero",
            ));
        }
        self.proposed_limits.validate()?;
        if self.activated_predicates.len() > MAX_SCOPE_PREDICATES {
            return Err(BootstrapError::InvalidContract(
                "activated_predicates exceeds MAX_SCOPE_PREDICATES",
            ));
        }
        for (idx, pred) in self.activated_predicates.iter().enumerate() {
            if pred == "always" || !REGISTERED_PREDICATES.split(' ').any(|tok| tok == pred) {
                return Err(BootstrapError::InvalidPredicateToken(pred.clone()));
            }
            if idx > 0 && self.activated_predicates[idx - 1] >= *pred {
                return Err(BootstrapError::InvalidPredicateToken(pred.clone()));
            }
        }
        for &(antecedent, entailed) in PREDICATE_ENTAILMENTS {
            if self.activated_predicates.iter().any(|p| p == antecedent)
                && !self.activated_predicates.iter().any(|p| p == entailed)
            {
                return Err(BootstrapError::InvalidPredicateToken(entailed.to_string()));
            }
        }
        Ok(())
    }

    /// Encodes the pre-ID scope into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, BootstrapError> {
        self.validate()?;
        let mut out =
            Vec::with_capacity(2 + 7 * 32 + 20 + 2 + self.activated_predicates.len() * 24);
        out.extend_from_slice(&self.schema_version.to_be_bytes());
        out.extend_from_slice(&self.base_transition_spec_digest);
        out.extend_from_slice(&self.domain_registry_digest);
        out.extend_from_slice(&self.crypto_suite_digest);
        out.extend_from_slice(&self.asset_policy_digest);
        out.extend_from_slice(&self.validator_econ_digest);
        out.extend_from_slice(&self.consensus_policy_digest);
        out.extend_from_slice(&self.governance_policy_digest);
        out.extend_from_slice(&self.proposed_limits.max_tx_bytes.to_be_bytes());
        out.extend_from_slice(&self.proposed_limits.max_block_txs.to_be_bytes());
        out.extend_from_slice(&self.proposed_limits.max_block_ops.to_be_bytes());
        out.extend_from_slice(&self.proposed_limits.max_proof_bytes.to_be_bytes());
        out.extend_from_slice(
            &self
                .proposed_limits
                .max_state_writes_per_block
                .to_be_bytes(),
        );
        out.extend_from_slice(&(self.activated_predicates.len() as u16).to_be_bytes());
        for pred in &self.activated_predicates {
            let b = pred.as_bytes();
            out.extend_from_slice(&(b.len() as u16).to_be_bytes());
            out.extend_from_slice(b);
        }
        Ok(out)
    }

    /// Decodes and validates a canonical `ProfileScopeV1`.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, BootstrapError> {
        if bytes.len() < 248 {
            return Err(BootstrapError::InvalidWire("truncated ProfileScopeV1"));
        }
        let read32 =
            |off: usize| -> [u8; 32] { bytes[off..off + 32].try_into().expect("32 bytes") };
        let read_u32 =
            |off: usize| -> u32 { u32::from_be_bytes(bytes[off..off + 4].try_into().expect("4")) };
        let schema_version = u16::from_be_bytes([bytes[0], bytes[1]]);
        let proposed_limits = ProposedLimitVectorV1 {
            max_tx_bytes: read_u32(226),
            max_block_txs: read_u32(230),
            max_block_ops: read_u32(234),
            max_proof_bytes: read_u32(238),
            max_state_writes_per_block: read_u32(242),
        };
        let pred_count = u16::from_be_bytes([bytes[246], bytes[247]]) as usize;
        if pred_count > MAX_SCOPE_PREDICATES {
            return Err(BootstrapError::InvalidContract("too many predicates"));
        }
        let mut cursor = 248;
        let mut activated_predicates = Vec::with_capacity(pred_count);
        for _ in 0..pred_count {
            if bytes.len().saturating_sub(cursor) < 2 {
                return Err(BootstrapError::InvalidWire("truncated predicate len"));
            }
            let plen = u16::from_be_bytes([bytes[cursor], bytes[cursor + 1]]) as usize;
            cursor += 2;
            if bytes.len().saturating_sub(cursor) < plen {
                return Err(BootstrapError::InvalidWire("truncated predicate text"));
            }
            let s = std::str::from_utf8(&bytes[cursor..cursor + plen])
                .map_err(|_| BootstrapError::InvalidWire("non-utf8 predicate"))?
                .to_string();
            cursor += plen;
            activated_predicates.push(s);
        }
        if cursor != bytes.len() {
            return Err(BootstrapError::InvalidWire("trailing ProfileScopeV1 bytes"));
        }
        let scope = Self {
            schema_version,
            base_transition_spec_digest: read32(2),
            domain_registry_digest: read32(34),
            crypto_suite_digest: read32(66),
            asset_policy_digest: read32(98),
            validator_econ_digest: read32(130),
            consensus_policy_digest: read32(162),
            governance_policy_digest: read32(194),
            proposed_limits,
            activated_predicates,
        };
        scope.validate()?;
        Ok(scope)
    }

    /// Computes the canonical `ContentTag` whose digest is `profile_scope_digest`.
    pub fn tag(&self) -> Result<ContentTag, BootstrapError> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            PROFILE_SCOPE_D0_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and bounded length"))
    }
}

/// Non-self-referential logical genesis payload (`GENESIS-D1`) deriving `genesis_id`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ChainGenesisPayloadV1 {
    /// Schema version (`1`).
    pub genesis_schema_version: u16,
    /// Pre-ID `profile_scope_digest` from `PROFILE-SCOPE-D0`.
    pub initial_profile_scope_digest: [u8; 32],
    /// Pre-ID `domain_registry_digest`.
    pub initial_domain_registry_digest: [u8; 32],
    /// Unique chain identifier (`> 0`).
    pub chain_id: u64,
    /// Single `i64³` world identifier (`1` in v1).
    pub world_id: u64,
    /// Pre-genesis actor identity namespace (`1..=64` ASCII bytes).
    pub identity_namespace: String,
    /// Initial bedrock world fingerprint digest.
    pub initial_world_digest: [u8; 32],
    /// Pre-genesis sorted actor descriptor set root (needs zero `genesis_id`).
    pub initial_actor_set_root: [u8; 32],
    /// Initial authority-selected validator set root (`Q_0`).
    pub initial_validator_set_root: [u8; 32],
    /// Initial genesis native fungible supply (`> 0`).
    pub initial_native_supply: u128,
}

impl ChainGenesisPayloadV1 {
    /// Validates all non-zero pre-ID bindings and namespace bounds.
    pub fn validate(&self) -> Result<(), BootstrapError> {
        if self.genesis_schema_version != 1 {
            return Err(BootstrapError::InvalidContract(
                "genesis_schema_version must be 1",
            ));
        }
        if self.chain_id == 0 || self.world_id == 0 {
            return Err(BootstrapError::InvalidContract(
                "chain_id and world_id must be > 0",
            ));
        }
        let ns = self.identity_namespace.as_bytes();
        if ns.is_empty() || ns.len() > 64 || !ns.is_ascii() {
            return Err(BootstrapError::InvalidContract(
                "identity_namespace must be 1..=64 ASCII bytes",
            ));
        }
        if self.initial_native_supply == 0 {
            return Err(BootstrapError::InvalidContract(
                "initial_native_supply must be > 0",
            ));
        }
        for d in [
            self.initial_profile_scope_digest,
            self.initial_domain_registry_digest,
            self.initial_world_digest,
            self.initial_actor_set_root,
            self.initial_validator_set_root,
        ] {
            if d == [0u8; 32] {
                return Err(BootstrapError::InvalidContract(
                    "genesis preimage digest slots must be non-zero",
                ));
            }
        }
        Ok(())
    }

    /// Encodes the non-self-referential genesis payload into canonical big-endian bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, BootstrapError> {
        self.validate()?;
        let ns = self.identity_namespace.as_bytes();
        let mut out = Vec::with_capacity(2 + 5 * 32 + 16 + 2 + ns.len() + 16);
        out.extend_from_slice(&self.genesis_schema_version.to_be_bytes());
        out.extend_from_slice(&self.initial_profile_scope_digest);
        out.extend_from_slice(&self.initial_domain_registry_digest);
        out.extend_from_slice(&self.chain_id.to_be_bytes());
        out.extend_from_slice(&self.world_id.to_be_bytes());
        out.extend_from_slice(&(ns.len() as u16).to_be_bytes());
        out.extend_from_slice(ns);
        out.extend_from_slice(&self.initial_world_digest);
        out.extend_from_slice(&self.initial_actor_set_root);
        out.extend_from_slice(&self.initial_validator_set_root);
        out.extend_from_slice(&self.initial_native_supply.to_be_bytes());
        Ok(out)
    }

    /// Decodes and validates a canonical `ChainGenesisPayloadV1`.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, BootstrapError> {
        if bytes.len() < 84 {
            return Err(BootstrapError::InvalidWire(
                "truncated ChainGenesisPayloadV1 header",
            ));
        }
        let read32 =
            |off: usize| -> [u8; 32] { bytes[off..off + 32].try_into().expect("32 bytes") };
        let genesis_schema_version = u16::from_be_bytes([bytes[0], bytes[1]]);
        let initial_profile_scope_digest = read32(2);
        let initial_domain_registry_digest = read32(34);
        let chain_id = u64::from_be_bytes(bytes[66..74].try_into().expect("8 bytes"));
        let world_id = u64::from_be_bytes(bytes[74..82].try_into().expect("8 bytes"));
        let ns_len = u16::from_be_bytes([bytes[82], bytes[83]]) as usize;
        if bytes.len() != 84 + ns_len + 96 + 16 {
            return Err(BootstrapError::InvalidWire(
                "ChainGenesisPayloadV1 length mismatch",
            ));
        }
        let identity_namespace = std::str::from_utf8(&bytes[84..84 + ns_len])
            .map_err(|_| BootstrapError::InvalidWire("non-utf8 identity_namespace"))?
            .to_string();
        let off = 84 + ns_len;
        let payload = Self {
            genesis_schema_version,
            initial_profile_scope_digest,
            initial_domain_registry_digest,
            chain_id,
            world_id,
            identity_namespace,
            initial_world_digest: read32(off),
            initial_actor_set_root: read32(off + 32),
            initial_validator_set_root: read32(off + 64),
            initial_native_supply: u128::from_be_bytes(
                bytes[off + 96..off + 112].try_into().expect("16 bytes"),
            ),
        };
        payload.validate()?;
        Ok(payload)
    }

    /// Computes the canonical `ContentTag` whose digest is the derived immutable `genesis_id`.
    pub fn tag(&self) -> Result<ContentTag, BootstrapError> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            GENESIS_D1_PAYLOAD_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and bounded length"))
    }
}

/// Signature-free unsigned proposal core (`PROTOCOL-ID-1`: `AcceptedProposalCoreV1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AcceptedProposalCoreV1 {
    /// Protocol version (`1`).
    pub protocol_version: u16,
    /// Chain identifier.
    pub chain_id: u64,
    /// Derived `genesis_id`.
    pub genesis_id: [u8; 32],
    /// Admitted `profile_id`.
    pub profile_id: [u8; 32],
    /// Ledger block height.
    pub height: u64,
    /// Validator schedule epoch.
    pub epoch: u64,
    /// Parent `AcceptedProposalCoreV1` digest (`genesis_id` at height 1).
    pub parent_core_id: [u8; 32],
    /// Authority-selected active validator root (`Q_h`).
    pub active_validator_root: [u8; 32],
    /// Committed post-execution state root.
    pub execution_state_root: [u8; 32],
    /// Committed history root.
    pub history_root: [u8; 32],
    /// Digest of canonical ordered block body bytes.
    pub body_digest: [u8; 32],
}

impl AcceptedProposalCoreV1 {
    /// Fixed canonical wire byte length (`250` bytes).
    pub const WIRE_LEN: usize = 250;

    /// Encodes the signature-free proposal core into exact canonical big-endian bytes.
    pub fn encode_canonical(&self) -> [u8; Self::WIRE_LEN] {
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.protocol_version.to_be_bytes());
        out[2..10].copy_from_slice(&self.chain_id.to_be_bytes());
        out[10..42].copy_from_slice(&self.genesis_id);
        out[42..74].copy_from_slice(&self.profile_id);
        out[74..82].copy_from_slice(&self.height.to_be_bytes());
        out[82..90].copy_from_slice(&self.epoch.to_be_bytes());
        out[90..122].copy_from_slice(&self.parent_core_id);
        out[122..154].copy_from_slice(&self.active_validator_root);
        out[154..186].copy_from_slice(&self.execution_state_root);
        out[186..218].copy_from_slice(&self.history_root);
        out[218..250].copy_from_slice(&self.body_digest);
        out
    }

    /// Decodes a canonical `AcceptedProposalCoreV1`.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, BootstrapError> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(BootstrapError::InvalidWire(
                "invalid AcceptedProposalCoreV1 length",
            ));
        }
        let r32 = |off: usize| -> [u8; 32] { bytes[off..off + 32].try_into().expect("32 bytes") };
        Ok(Self {
            protocol_version: u16::from_be_bytes([bytes[0], bytes[1]]),
            chain_id: u64::from_be_bytes(bytes[2..10].try_into().expect("8 bytes")),
            genesis_id: r32(10),
            profile_id: r32(42),
            height: u64::from_be_bytes(bytes[74..82].try_into().expect("8 bytes")),
            epoch: u64::from_be_bytes(bytes[82..90].try_into().expect("8 bytes")),
            parent_core_id: r32(90),
            active_validator_root: r32(122),
            execution_state_root: r32(154),
            history_root: r32(186),
            body_digest: r32(218),
        })
    }

    /// Computes the canonical `ContentTag` (`accepted_parent_lookup_id`) of this unsigned core.
    pub fn tag(&self) -> ContentTag {
        let bytes = self.encode_canonical();
        hash_canonical(
            TagKind::Receipt,
            PROTOCOL_ID_1_PROPOSAL_CORE_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length")
    }
}

/// Directional collision disposition separating credential multiplicity from distinct-core collision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AcceptedParentCollisionDispositionV1 {
    /// Two wrappers carry identical unsigned `AcceptedProposalCoreV1` bytes: credentials normalize
    /// into one deduplicated set without adding weight or multiplying parents.
    DuplicateCredentialNormalized,
    /// Two wrappers claim the same parent lookup slot with distinct unsigned cores: triggers
    /// deterministic safe-halt and never selects one core as an ordinary successor.
    DistinctCoreSafeHalt,
}

/// Classifies two candidate proposal cores under `PROTOCOL-ID-1`'s directional collision rule.
pub fn classify_parent_collision(
    existing: &AcceptedProposalCoreV1,
    incoming: &AcceptedProposalCoreV1,
) -> AcceptedParentCollisionDispositionV1 {
    if existing.encode_canonical() == incoming.encode_canonical() {
        AcceptedParentCollisionDispositionV1::DuplicateCredentialNormalized
    } else {
        AcceptedParentCollisionDispositionV1::DistinctCoreSafeHalt
    }
}

/// Verifies a signed transaction envelope binding `TransactionBodyV1` to `(chain_id, genesis_id,
/// launch_manifest_id, profile_id)` via strict RFC 8032 Ed25519 (`PROTOCOL-ID-1`).
pub fn verify_signed_transaction_envelope(
    chain_id: u64,
    genesis_id: [u8; 32],
    launch_manifest_id: [u8; 32],
    profile_id: [u8; 32],
    body: &TransactionBodyV1,
    signer_pk: &CanonicalEd25519PublicKeyV1,
    signature: &CanonicalEd25519SignatureV1,
) -> Result<[u8; 32], BootstrapError> {
    let body_tag = body.tag()?;
    let preimage = SigningPreimageV1 {
        protocol_version: 1,
        chain_id,
        genesis_id,
        launch_manifest_id,
        profile_id,
        domain_tag: SigningDomainTagV1::ActorTransaction,
        signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
        hash_suite: HashSuiteIdV1::Sha256V1,
        payload_digest: body_tag.digest(),
    };
    verify_preimage_strict(signer_pk, &preimage, signature)?;
    Ok(preimage.signing_digest())
}

/// Verifies that a directed protocol identifier derivation graph `(node -> dependencies)` is
/// strictly acyclic (Kahn's algorithm; detects self-loops and multi-hop cycles).
pub fn verify_protocol_id_graph_acyclic(edges: &[(&str, &str)]) -> Result<(), BootstrapError> {
    let mut nodes = BTreeSet::new();
    for &(u, v) in edges {
        if u == v {
            return Err(BootstrapError::IdentifierCycleDetected);
        }
        nodes.insert(u);
        nodes.insert(v);
    }
    let mut visited = BTreeSet::new();
    let mut active = BTreeSet::new();
    for &start in &nodes {
        if visited.contains(start) {
            continue;
        }
        let mut stack = vec![(start, 0usize)];
        active.insert(start);
        while let Some((node, idx)) = stack.last_mut() {
            let nbrs: Vec<&str> = edges
                .iter()
                .filter_map(|&(u, v)| (u == *node).then_some(v))
                .collect();
            if *idx < nbrs.len() {
                let next = nbrs[*idx];
                *idx += 1;
                if active.contains(next) {
                    return Err(BootstrapError::IdentifierCycleDetected);
                }
                if !visited.contains(next) {
                    active.insert(next);
                    stack.push((next, 0));
                }
            } else {
                active.remove(node);
                visited.insert(*node);
                stack.pop();
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto_d1::CanonicalEd25519PublicKeyV1;
    use crate::identity_d1::{CredentialRoleV1, ThresholdPolicyV1};

    #[test]
    fn bootstrap_scope_genesis_proposal_core_and_acyclicity() {
        let mut ckeys = [31u8, 32, 33]
            .into_iter()
            .map(|s| CanonicalEd25519PublicKeyV1::from_secret_seed(&[s; 32]).unwrap())
            .collect::<Vec<_>>();
        ckeys.sort();
        let council = ThresholdPolicyV1 {
            role: CredentialRoleV1::EmergencyPauseSigner,
            threshold: 2,
            keys: ckeys,
        };
        let gov = GovernancePolicyDescriptorV1 {
            emergency_council: council,
            forbid_history_rewrite: true,
            emergency_council_pause_only: true,
            automatic_safe_halt_on_proven_fault: true,
            max_emergency_pause_epochs: 3,
            mandatory_exit_timelock_epochs: 21,
            upgrade_quorum_denominator: 3,
            upgrade_quorum_numerator: 2,
            schema_version: 1,
        };
        let dom = DomainRegistryV1::local_v1();
        let scope = ProfileScopeV1::from_wave1_contracts(
            &dom,
            &gov,
            vec![
                "asset-active".to_string(),
                "economic-charging".to_string(),
                "stake-consensus".to_string(),
                "validator-economics".to_string(),
            ],
        )
        .unwrap();
        let raw_s = scope.encode_canonical().unwrap();
        assert_eq!(ProfileScopeV1::decode_canonical(&raw_s).unwrap(), scope);

        let genesis = ChainGenesisPayloadV1 {
            genesis_schema_version: 1,
            initial_profile_scope_digest: scope.tag().unwrap().digest(),
            initial_domain_registry_digest: dom.tag().digest(),
            chain_id: 1,
            world_id: 1,
            identity_namespace: "bitblock-mainnet-v1".to_string(),
            initial_world_digest: [0x11; 32],
            initial_actor_set_root: [0x22; 32],
            initial_validator_set_root: [0x33; 32],
            initial_native_supply: 1_000_000_000_000_000,
        };
        let raw_g = genesis.encode_canonical().unwrap();
        assert_eq!(
            ChainGenesisPayloadV1::decode_canonical(&raw_g).unwrap(),
            genesis
        );

        let core = AcceptedProposalCoreV1 {
            protocol_version: 1,
            chain_id: 1,
            genesis_id: genesis.tag().unwrap().digest(),
            profile_id: [0x44; 32],
            height: 1,
            epoch: 0,
            parent_core_id: genesis.tag().unwrap().digest(),
            active_validator_root: [0x33; 32],
            execution_state_root: [0x55; 32],
            history_root: [0x66; 32],
            body_digest: [0x77; 32],
        };
        let raw_c = core.encode_canonical();
        assert_eq!(
            AcceptedProposalCoreV1::decode_canonical(&raw_c).unwrap(),
            core
        );
        assert_eq!(
            classify_parent_collision(&core, &core),
            AcceptedParentCollisionDispositionV1::DuplicateCredentialNormalized
        );
        let mut distinct = core;
        distinct.body_digest = [0x88; 32];
        assert_eq!(
            classify_parent_collision(&core, &distinct),
            AcceptedParentCollisionDispositionV1::DistinctCoreSafeHalt
        );

        verify_protocol_id_graph_acyclic(&[
            ("genesis_id", "profile_scope_digest"),
            ("profile_id", "genesis_id"),
            ("tx_id", "profile_id"),
            ("proposal_core_id", "genesis_id"),
        ])
        .unwrap();
        assert_eq!(
            verify_protocol_id_graph_acyclic(&[
                ("genesis_id", "profile_id"),
                ("profile_id", "genesis_id"),
            ]),
            Err(BootstrapError::IdentifierCycleDetected)
        );
    }
}
