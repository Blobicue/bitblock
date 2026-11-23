//! `IDENTITY-1`: runtime actor identity authorization, chain/genesis/world/version signing
//! preimage binding, sequential principal nonces, non-transitive depth-1 scoped delegation,
//! two-step timelocked recovery, and strict actor/validator key-domain separation (`PS-028`).
//!
//! Builds strictly over `IDENTITY-D1`, `CRYPTO-D1`, `CRYPTO-1`, and `PROTOCOL-ID-1` behind
//! `--features protocol-research` (`wasm32`-clean, zero new Cargo dependencies):
//! - **Non-circular signing preimage binding (`IdentityContextV1`):** binds `protocol_version`,
//!   `chain_id`, `genesis_id`, `launch_manifest_id`, `profile_id`, active `world_tag`, and suite
//!   IDs before Ed25519 verification so cross-chain, cross-genesis, cross-world, and wrong-suite
//!   replays receive deterministic rejections without mutating state or nonces.
//! - **Non-transitive depth-1 scoped delegation:** a principal actor may delegate `On` and/or
//!   `Off` actions with an explicit expiry height to a direct delegate (`A -> B`); sub-delegation
//!   (`A -> B -> C`) is rejected fail-closed with [`IdentityDispositionV1::RejectedTransitiveDelegation`].
//! - **Two-step timelocked recovery:** recovery guardians initiate a pending recovery with delay
//!   `recovery_delay_heights`; active `ActorAdmin` keys may cancel before `unlock_height`;
//!   finalization at `height >= unlock_height` rotates keys in-place while preserving `ActorIdV1`,
//!   incrementing `descriptor_version`, and clearing all stale delegations.
//! - **Strict actor/validator key-domain separation:** keys registered in validator roles can
//!   never be attached to actor descriptors or sign actor/admin/recovery actions.
//!
//! A public key proves key control, not one unique human, legal ownership, or validator authority.
//! Durable authenticated state/nonce root commitment belongs downstream to `AUTH-COMMIT-1`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use sha2::{Digest, Sha256};

use crate::crypto_d1::{
    CanonicalEd25519PublicKeyV1, CanonicalEd25519SignatureV1, HashSuiteIdV1, SignatureSuiteIdV1,
    SigningDomainTagV1, SigningPreimageV1, verify_preimage_strict,
};
use crate::identity_d1::{
    ActorIdV1, AuthorizationDescriptorV1, CredentialRoleV1, DelegationGrantV1, IdentityD1Error,
    NetworkEditOpV1, ThresholdPolicyV1, TransactionBodyV1, validate_actor_descriptor_set,
};
use crate::protocol::{ContentTag, TagKind, hash_canonical};

/// Canonical schema string for an [`ActorAuthStateV1`] record (`IDENTITY-1`).
pub const IDENTITY_1_AUTH_STATE_SCHEMA: &[u8] = b"bitblock/identity-1/actor-auth-state/v1";
/// Canonical schema string for an [`IdentityAuthReceiptV1`] disposition receipt (`IDENTITY-1`).
pub const IDENTITY_1_AUTH_RECEIPT_SCHEMA: &[u8] = b"bitblock/identity-1/authorization-receipt/v1";

/// Domain-separation prefix binding active `world_tag` into `IDENTITY-1` signing payloads.
const WORLD_BIND_PREFIX: &[u8] = b"bb.id1.world_bind.v1";

/// Errors returned during `IDENTITY-1` state codec decoding or registry setup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Identity1Error {
    /// Underlying `IDENTITY-D1` descriptor, delegation-cycle, or wire error.
    IdentityD1(IdentityD1Error),
    /// Wire buffer had invalid length, version, discriminant, or trailing bytes.
    InvalidWire(&'static str),
    /// Actor registration, validator registration, or recovery configuration violated invariants.
    InvalidConfiguration(&'static str),
    /// A validator key collided with an actor action, admin, or recovery guardian key.
    ValidatorActorKeyCollision,
    /// Actor was already registered in the runtime registry.
    DuplicateActor(ActorIdV1),
}

impl From<IdentityD1Error> for Identity1Error {
    fn from(value: IdentityD1Error) -> Self {
        Self::IdentityD1(value)
    }
}

impl fmt::Display for Identity1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Identity1Error {}

/// Runtime chain, genesis, profile, version, suite, and world context bound by `IDENTITY-1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IdentityContextV1 {
    /// Active protocol version bound in every signing preimage.
    pub protocol_version: u16,
    /// Active chain identifier.
    pub chain_id: u64,
    /// Derived immutable `GENESIS-D1` identifier.
    pub genesis_id: [u8; 32],
    /// Derived launch manifest identifier.
    pub launch_manifest_id: [u8; 32],
    /// Active `PROFILE-D1` identifier.
    pub profile_id: [u8; 32],
    /// Active world `ContentTag` bound into every authorization payload digest.
    pub world_tag: ContentTag,
    /// Active signature suite (`Ed25519StrictRfc8032V1`).
    pub signature_suite: SignatureSuiteIdV1,
    /// Active hash suite (`Sha256V1`).
    pub hash_suite: HashSuiteIdV1,
}

impl IdentityContextV1 {
    /// Derives the world-bound 32-byte payload digest placed inside [`SigningPreimageV1`].
    pub fn world_bound_payload_digest(&self, raw_payload_digest: [u8; 32]) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(WORLD_BIND_PREFIX);
        h.update(self.world_tag.digest());
        h.update(raw_payload_digest);
        h.finalize().into()
    }

    /// Builds the exact non-circular [`SigningPreimageV1`] for `domain_tag` and `raw_payload_digest`.
    pub fn build_preimage(
        &self,
        domain_tag: SigningDomainTagV1,
        raw_payload_digest: [u8; 32],
    ) -> SigningPreimageV1 {
        SigningPreimageV1 {
            protocol_version: self.protocol_version,
            chain_id: self.chain_id,
            genesis_id: self.genesis_id,
            launch_manifest_id: self.launch_manifest_id,
            profile_id: self.profile_id,
            domain_tag,
            signature_suite: self.signature_suite,
            hash_suite: self.hash_suite,
            payload_digest: self.world_bound_payload_digest(raw_payload_digest),
        }
    }

    /// Checks whether a caller-supplied `SigningPreimageV1` matches every bound domain field.
    pub fn matches_preimage(
        &self,
        preimage: &SigningPreimageV1,
        expected_domain: SigningDomainTagV1,
        raw_payload_digest: [u8; 32],
    ) -> bool {
        preimage.protocol_version == self.protocol_version
            && preimage.chain_id == self.chain_id
            && preimage.genesis_id == self.genesis_id
            && preimage.launch_manifest_id == self.launch_manifest_id
            && preimage.profile_id == self.profile_id
            && preimage.domain_tag == expected_domain
            && preimage.signature_suite == self.signature_suite
            && preimage.hash_suite == self.hash_suite
            && preimage.payload_digest == self.world_bound_payload_digest(raw_payload_digest)
    }
}

/// Action kind recorded in an [`IdentityAuthReceiptV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum IdentityActionKindV1 {
    /// Direct transaction signed by the principal actor's `ActorAction` policy.
    DirectTransaction = 0,
    /// Depth-1 delegated transaction signed by an authorized delegate actor's `ActorAction` policy.
    DelegatedTransaction = 1,
    /// In-place descriptor rotation authorized by the actor's `ActorAdmin` policy.
    AdminRotation = 2,
    /// Permanent actor revocation authorized by the actor's `ActorAdmin` policy.
    AdminRevocation = 3,
    /// Two-step recovery initiation signed by `recovery_guardians`.
    RecoveryInitiate = 4,
    /// Pending recovery cancellation signed by the active `ActorAdmin` policy.
    RecoveryCancel = 5,
    /// Timelocked recovery finalization at `block_height >= unlock_height`.
    RecoveryFinalize = 6,
}

impl IdentityActionKindV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte action-kind discriminant.
    pub const fn from_code(code: u8) -> Result<Self, Identity1Error> {
        match code {
            0 => Ok(Self::DirectTransaction),
            1 => Ok(Self::DelegatedTransaction),
            2 => Ok(Self::AdminRotation),
            3 => Ok(Self::AdminRevocation),
            4 => Ok(Self::RecoveryInitiate),
            5 => Ok(Self::RecoveryCancel),
            6 => Ok(Self::RecoveryFinalize),
            _ => Err(Identity1Error::InvalidWire("unknown IdentityActionKindV1")),
        }
    }
}

/// Complete deterministic disposition lane emitted by every `IDENTITY-1` gate check.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum IdentityDispositionV1 {
    /// Request satisfied every cryptographic, domain, nonce, scope, and timelock invariant.
    Admitted = 0,
    /// Principal or signer actor ID does not exist in the registry.
    RejectedUnknownActor = 1,
    /// Principal or delegate descriptor is permanently revoked.
    RejectedActorRevoked = 2,
    /// Chain, genesis, launch-manifest, profile, protocol-version, suite, or world binding differed.
    RejectedDomainBindingMismatch = 3,
    /// A validator-domain key was submitted for an actor/admin/recovery operation.
    RejectedValidatorKeyDomainCrossUse = 4,
    /// Signature, torsion/invalid key, duplicate signer, or threshold check failed.
    RejectedInvalidOrDuplicateSignature = 5,
    /// Submitted nonce did not equal the principal's exact sequential `next_nonce`.
    RejectedStaleOrFutureNonce = 6,
    /// Block height lay outside `[valid_after, valid_until]`.
    RejectedValidityWindow = 7,
    /// Transaction `expected_state_root` disagreed with active `world_tag`.
    RejectedWorldRootPrecondition = 8,
    /// No active depth-1 delegation grant existed or the grant had expired at `block_height`.
    RejectedDelegationMissingOrExpired = 9,
    /// Delegated batch contained an `On`/`Off` action outside the grant's scope flags.
    RejectedDelegationScopeDenied = 10,
    /// Sub-delegate attempted depth >= 2 transitive delegation (`A -> B -> C` forbidden).
    RejectedTransitiveDelegation = 11,
    /// Recovery finalization was attempted strictly before `unlock_height`.
    RejectedRecoveryTimelockActive = 12,
    /// Recovery state precondition failed (already pending, none pending, or invalid target descriptor).
    RejectedRecoveryStateInvalid = 13,
}

impl IdentityDispositionV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte disposition discriminant.
    pub const fn from_code(code: u8) -> Result<Self, Identity1Error> {
        match code {
            0 => Ok(Self::Admitted),
            1 => Ok(Self::RejectedUnknownActor),
            2 => Ok(Self::RejectedActorRevoked),
            3 => Ok(Self::RejectedDomainBindingMismatch),
            4 => Ok(Self::RejectedValidatorKeyDomainCrossUse),
            5 => Ok(Self::RejectedInvalidOrDuplicateSignature),
            6 => Ok(Self::RejectedStaleOrFutureNonce),
            7 => Ok(Self::RejectedValidityWindow),
            8 => Ok(Self::RejectedWorldRootPrecondition),
            9 => Ok(Self::RejectedDelegationMissingOrExpired),
            10 => Ok(Self::RejectedDelegationScopeDenied),
            11 => Ok(Self::RejectedTransitiveDelegation),
            12 => Ok(Self::RejectedRecoveryTimelockActive),
            13 => Ok(Self::RejectedRecoveryStateInvalid),
            _ => Err(Identity1Error::InvalidWire("unknown IdentityDispositionV1")),
        }
    }
}

/// Pending two-step timelocked recovery waiting for `block_height >= unlock_height` or admin cancel.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PendingRecoveryV1 {
    /// Ledger height when `recovery_guardians` initiated recovery.
    pub initiated_height: u64,
    /// Earliest ledger height at which recovery may finalize (`initiated_height + delay`).
    pub unlock_height: u64,
    /// Proposed replacement descriptor (must be non-revoked; stale delegations cleared on finalize).
    pub proposed_descriptor: AuthorizationDescriptorV1,
}

impl PendingRecoveryV1 {
    /// Validates pending recovery timing and target descriptor invariants.
    pub fn validate(&self, expected_delay: u64) -> Result<(), Identity1Error> {
        self.proposed_descriptor.validate()?;
        if self.proposed_descriptor.revoked {
            return Err(Identity1Error::InvalidConfiguration(
                "recovery proposed_descriptor cannot be revoked",
            ));
        }
        let expected_unlock = self.initiated_height.checked_add(expected_delay).ok_or(
            Identity1Error::InvalidConfiguration("unlock_height overflow"),
        )?;
        if self.unlock_height != expected_unlock {
            return Err(Identity1Error::InvalidConfiguration(
                "pending recovery unlock_height disagrees with recovery_delay_heights",
            ));
        }
        Ok(())
    }
}

/// Committed per-actor runtime authorization, nonce, and recovery state (`IDENTITY-1`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ActorAuthStateV1 {
    /// Record schema version (`1`).
    pub state_version: u16,
    /// Stable pre-genesis derived [`ActorIdV1`].
    pub actor_id: ActorIdV1,
    /// Active committed [`AuthorizationDescriptorV1`].
    pub descriptor: AuthorizationDescriptorV1,
    /// Exact next sequential principal transaction nonce (`0` initially).
    pub next_nonce: u64,
    /// Guardian threshold policy (`CredentialRoleV1::ActorAdmin`) authorized to initiate recovery.
    pub recovery_guardians: ThresholdPolicyV1,
    /// Mandatory timelock delay in ledger heights before recovery can finalize (`>= 1`).
    pub recovery_delay_heights: u64,
    /// Optional active pending two-step recovery.
    pub pending_recovery: Option<PendingRecoveryV1>,
}

impl ActorAuthStateV1 {
    /// Validates descriptor, recovery guardian policy, delay positivity, and pending recovery.
    pub fn validate(&self) -> Result<(), Identity1Error> {
        if self.state_version != 1 {
            return Err(Identity1Error::InvalidWire("state_version must equal 1"));
        }
        self.descriptor.validate()?;
        self.recovery_guardians
            .validate(CredentialRoleV1::ActorAdmin)?;
        if self.recovery_delay_heights == 0 {
            return Err(Identity1Error::InvalidConfiguration(
                "recovery_delay_heights must be >= 1",
            ));
        }
        if let Some(pending) = &self.pending_recovery {
            if self.descriptor.revoked {
                return Err(Identity1Error::InvalidConfiguration(
                    "revoked actor cannot hold a pending recovery",
                ));
            }
            pending.validate(self.recovery_delay_heights)?;
        }
        Ok(())
    }

    /// Collects every public key referenced by this actor's action, admin, guardian, and pending policies.
    pub fn all_referenced_keys(&self) -> Vec<CanonicalEd25519PublicKeyV1> {
        let mut keys = Vec::new();
        keys.extend_from_slice(&self.descriptor.action_policy.keys);
        keys.extend_from_slice(&self.descriptor.admin_policy.keys);
        keys.extend_from_slice(&self.recovery_guardians.keys);
        if let Some(pending) = &self.pending_recovery {
            keys.extend_from_slice(&pending.proposed_descriptor.action_policy.keys);
            keys.extend_from_slice(&pending.proposed_descriptor.admin_policy.keys);
        }
        keys
    }

    /// Encodes this actor authorization state into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, Identity1Error> {
        self.validate()?;
        let desc_bytes = self.descriptor.encode_canonical()?;
        let mut guard_bytes = Vec::new();
        self.recovery_guardians.encode_into(&mut guard_bytes)?;
        let mut out = Vec::with_capacity(64 + desc_bytes.len() + guard_bytes.len());
        out.extend_from_slice(&self.state_version.to_be_bytes());
        out.extend_from_slice(self.actor_id.as_bytes());
        out.extend_from_slice(&(desc_bytes.len() as u32).to_be_bytes());
        out.extend_from_slice(&desc_bytes);
        out.extend_from_slice(&self.next_nonce.to_be_bytes());
        out.extend_from_slice(&(guard_bytes.len() as u32).to_be_bytes());
        out.extend_from_slice(&guard_bytes);
        out.extend_from_slice(&self.recovery_delay_heights.to_be_bytes());
        match &self.pending_recovery {
            None => out.push(0),
            Some(pending) => {
                let prop_bytes = pending.proposed_descriptor.encode_canonical()?;
                out.push(1);
                out.extend_from_slice(&pending.initiated_height.to_be_bytes());
                out.extend_from_slice(&pending.unlock_height.to_be_bytes());
                out.extend_from_slice(&(prop_bytes.len() as u32).to_be_bytes());
                out.extend_from_slice(&prop_bytes);
            }
        }
        Ok(out)
    }

    /// Decodes and validates a canonical [`ActorAuthStateV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Identity1Error> {
        if bytes.len() < 38 {
            return Err(Identity1Error::InvalidWire("truncated ActorAuthStateV1"));
        }
        let state_version = u16::from_be_bytes([bytes[0], bytes[1]]);
        let actor_id = ActorIdV1::from_bytes(bytes[2..34].try_into().expect("32"));
        let desc_len = u32::from_be_bytes(bytes[34..38].try_into().expect("4")) as usize;
        let mut cur = 38;
        if bytes.len().saturating_sub(cur) < desc_len + 12 {
            return Err(Identity1Error::InvalidWire("truncated descriptor section"));
        }
        let descriptor = AuthorizationDescriptorV1::decode_canonical(&bytes[cur..cur + desc_len])?;
        cur += desc_len;
        let next_nonce = u64::from_be_bytes(bytes[cur..cur + 8].try_into().expect("8"));
        cur += 8;
        let guard_len = u32::from_be_bytes(bytes[cur..cur + 4].try_into().expect("4")) as usize;
        cur += 4;
        if bytes.len().saturating_sub(cur) < guard_len + 9 {
            return Err(Identity1Error::InvalidWire("truncated guardian section"));
        }
        let mut guard_cur = 0usize;
        let recovery_guardians = ThresholdPolicyV1::decode_from(
            &bytes[cur..cur + guard_len],
            &mut guard_cur,
            CredentialRoleV1::ActorAdmin,
        )?;
        if guard_cur != guard_len {
            return Err(Identity1Error::InvalidWire("trailing guardian bytes"));
        }
        cur += guard_len;
        let recovery_delay_heights = u64::from_be_bytes(bytes[cur..cur + 8].try_into().expect("8"));
        cur += 8;
        let pending_recovery = match bytes[cur] {
            0 => {
                cur += 1;
                None
            }
            1 => {
                cur += 1;
                if bytes.len().saturating_sub(cur) < 20 {
                    return Err(Identity1Error::InvalidWire("truncated pending recovery"));
                }
                let initiated_height =
                    u64::from_be_bytes(bytes[cur..cur + 8].try_into().expect("8"));
                let unlock_height =
                    u64::from_be_bytes(bytes[cur + 8..cur + 16].try_into().expect("8"));
                let prop_len =
                    u32::from_be_bytes(bytes[cur + 16..cur + 20].try_into().expect("4")) as usize;
                cur += 20;
                if bytes.len().saturating_sub(cur) < prop_len {
                    return Err(Identity1Error::InvalidWire("truncated proposed descriptor"));
                }
                let proposed_descriptor =
                    AuthorizationDescriptorV1::decode_canonical(&bytes[cur..cur + prop_len])?;
                cur += prop_len;
                Some(PendingRecoveryV1 {
                    initiated_height,
                    unlock_height,
                    proposed_descriptor,
                })
            }
            _ => return Err(Identity1Error::InvalidWire("invalid pending recovery flag")),
        };
        if cur != bytes.len() {
            return Err(Identity1Error::InvalidWire(
                "trailing bytes in ActorAuthStateV1",
            ));
        }
        let state = Self {
            state_version,
            actor_id,
            descriptor,
            next_nonce,
            recovery_guardians,
            recovery_delay_heights,
            pending_recovery,
        };
        state.validate()?;
        Ok(state)
    }

    /// Computes the canonical `Receipt` `ContentTag` for this actor authorization state.
    pub fn tag(&self) -> Result<ContentTag, Identity1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            IDENTITY_1_AUTH_STATE_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and bounded length"))
    }
}

/// Fixed-size deterministic authorization disposition receipt (`IDENTITY-1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IdentityAuthReceiptV1 {
    /// Receipt schema version (`1`).
    pub receipt_version: u16,
    /// Kind of authorization request evaluated.
    pub action_kind: IdentityActionKindV1,
    /// Deterministic admission or fail-closed rejection disposition.
    pub disposition: IdentityDispositionV1,
    /// Principal actor whose state/nonce was targeted.
    pub principal_actor_id: ActorIdV1,
    /// Acting signer actor ID (equals principal except for delegated requests).
    pub signer_actor_id: ActorIdV1,
    /// Principal sequential nonce before evaluation (`0` if principal unknown).
    pub pre_nonce: u64,
    /// Principal sequential nonce after evaluation (advances by `+1` only on admitted tx).
    pub post_nonce: u64,
    /// Principal descriptor version after evaluation (`0` if principal unknown).
    pub descriptor_version: u64,
    /// Ledger height at which authorization was evaluated.
    pub block_height: u64,
    /// Canonical 32-byte digest of the submitted payload.
    pub payload_digest: [u8; 32],
}

impl IdentityAuthReceiptV1 {
    /// Fixed canonical big-endian wire length (`132` bytes).
    pub const WIRE_LEN: usize = 2 + 1 + 1 + 32 + 32 + 8 + 8 + 8 + 8 + 32;

    /// Encodes the receipt into fixed 132-byte canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], Identity1Error> {
        if self.receipt_version != 1 {
            return Err(Identity1Error::InvalidWire("receipt_version must equal 1"));
        }
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.receipt_version.to_be_bytes());
        out[2] = self.action_kind.code();
        out[3] = self.disposition.code();
        out[4..36].copy_from_slice(self.principal_actor_id.as_bytes());
        out[36..68].copy_from_slice(self.signer_actor_id.as_bytes());
        out[68..76].copy_from_slice(&self.pre_nonce.to_be_bytes());
        out[76..84].copy_from_slice(&self.post_nonce.to_be_bytes());
        out[84..92].copy_from_slice(&self.descriptor_version.to_be_bytes());
        out[92..100].copy_from_slice(&self.block_height.to_be_bytes());
        out[100..132].copy_from_slice(&self.payload_digest);
        Ok(out)
    }

    /// Decodes and validates a 132-byte canonical [`IdentityAuthReceiptV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Identity1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(Identity1Error::InvalidWire(
                "IdentityAuthReceiptV1 must be 132 bytes",
            ));
        }
        let receipt_version = u16::from_be_bytes([bytes[0], bytes[1]]);
        if receipt_version != 1 {
            return Err(Identity1Error::InvalidWire("receipt_version must equal 1"));
        }
        Ok(Self {
            receipt_version,
            action_kind: IdentityActionKindV1::from_code(bytes[2])?,
            disposition: IdentityDispositionV1::from_code(bytes[3])?,
            principal_actor_id: ActorIdV1::from_bytes(bytes[4..36].try_into().expect("32")),
            signer_actor_id: ActorIdV1::from_bytes(bytes[36..68].try_into().expect("32")),
            pre_nonce: u64::from_be_bytes(bytes[68..76].try_into().expect("8")),
            post_nonce: u64::from_be_bytes(bytes[76..84].try_into().expect("8")),
            descriptor_version: u64::from_be_bytes(bytes[84..92].try_into().expect("8")),
            block_height: u64::from_be_bytes(bytes[92..100].try_into().expect("8")),
            payload_digest: bytes[100..132].try_into().expect("32"),
        })
    }

    /// Computes the canonical `Receipt` `ContentTag` for this authorization receipt.
    pub fn tag(&self) -> Result<ContentTag, Identity1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            IDENTITY_1_AUTH_RECEIPT_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

/// Deterministic runtime `IDENTITY-1` authorization, delegation, and timelocked-recovery engine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdentityRegistryV1 {
    context: IdentityContextV1,
    actors: BTreeMap<ActorIdV1, ActorAuthStateV1>,
    registered_validator_keys: BTreeSet<CanonicalEd25519PublicKeyV1>,
}

impl IdentityRegistryV1 {
    /// Creates an empty runtime registry bound to `context`.
    pub fn new(context: IdentityContextV1) -> Self {
        Self {
            context,
            actors: BTreeMap::new(),
            registered_validator_keys: BTreeSet::new(),
        }
    }

    /// Returns the bound [`IdentityContextV1`].
    pub const fn context(&self) -> &IdentityContextV1 {
        &self.context
    }

    /// Updates the active world tag (e.g. after an admitted world transition).
    pub fn set_world_tag(&mut self, world_tag: ContentTag) {
        self.context.world_tag = world_tag;
    }

    /// Borrows an actor's current [`ActorAuthStateV1`].
    pub fn actor_state(&self, actor_id: &ActorIdV1) -> Option<&ActorAuthStateV1> {
        self.actors.get(actor_id)
    }

    /// Registers a validator consensus/proposal key into the disjoint validator key domain.
    pub fn register_validator_key(
        &mut self,
        pk: CanonicalEd25519PublicKeyV1,
    ) -> Result<(), Identity1Error> {
        for state in self.actors.values() {
            if state.all_referenced_keys().contains(&pk) {
                return Err(Identity1Error::ValidatorActorKeyCollision);
            }
        }
        self.registered_validator_keys.insert(pk);
        Ok(())
    }

    /// Registers a validated actor authorization state while enforcing delegation acyclicity
    /// and strict actor/validator key-domain disjointness.
    pub fn register_actor(&mut self, state: ActorAuthStateV1) -> Result<(), Identity1Error> {
        state.validate()?;
        if self.actors.contains_key(&state.actor_id) {
            return Err(Identity1Error::DuplicateActor(state.actor_id));
        }
        for key in state.all_referenced_keys() {
            if self.registered_validator_keys.contains(&key) {
                return Err(Identity1Error::ValidatorActorKeyCollision);
            }
        }
        let mut entries: Vec<(ActorIdV1, AuthorizationDescriptorV1)> = self
            .actors
            .iter()
            .map(|(id, st)| (*id, st.descriptor.clone()))
            .collect();
        entries.push((state.actor_id, state.descriptor.clone()));
        entries.sort_by_key(|(id, _)| *id);
        validate_actor_descriptor_set(&entries)?;
        self.actors.insert(state.actor_id, state);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn mk_receipt(
        action_kind: IdentityActionKindV1,
        disposition: IdentityDispositionV1,
        principal_actor_id: ActorIdV1,
        signer_actor_id: ActorIdV1,
        pre_nonce: u64,
        post_nonce: u64,
        descriptor_version: u64,
        block_height: u64,
        payload_digest: [u8; 32],
    ) -> IdentityAuthReceiptV1 {
        IdentityAuthReceiptV1 {
            receipt_version: 1,
            action_kind,
            disposition,
            principal_actor_id,
            signer_actor_id,
            pre_nonce,
            post_nonce,
            descriptor_version,
            block_height,
            payload_digest,
        }
    }

    fn verify_raw_threshold(
        &self,
        policy: &ThresholdPolicyV1,
        expected_role: CredentialRoleV1,
        preimage: &SigningPreimageV1,
        raw_signatures: &[([u8; 32], [u8; 64])],
    ) -> Result<(), IdentityDispositionV1> {
        if policy.validate(expected_role).is_err() {
            return Err(IdentityDispositionV1::RejectedInvalidOrDuplicateSignature);
        }
        let mut seen = BTreeSet::new();
        for (raw_pk, raw_sig) in raw_signatures {
            let Ok(pk) = CanonicalEd25519PublicKeyV1::from_bytes(*raw_pk) else {
                return Err(IdentityDispositionV1::RejectedInvalidOrDuplicateSignature);
            };
            if self.registered_validator_keys.contains(&pk) {
                return Err(IdentityDispositionV1::RejectedValidatorKeyDomainCrossUse);
            }
            if policy.keys.binary_search(&pk).is_err() || !seen.insert(pk) {
                return Err(IdentityDispositionV1::RejectedInvalidOrDuplicateSignature);
            }
            let Ok(sig) = CanonicalEd25519SignatureV1::from_bytes(*raw_sig) else {
                return Err(IdentityDispositionV1::RejectedInvalidOrDuplicateSignature);
            };
            if verify_preimage_strict(&pk, preimage, &sig).is_err() {
                return Err(IdentityDispositionV1::RejectedInvalidOrDuplicateSignature);
            }
        }
        if seen.len() < usize::from(policy.threshold) {
            return Err(IdentityDispositionV1::RejectedInvalidOrDuplicateSignature);
        }
        Ok(())
    }

    /// Evaluates a direct or depth-1 delegated [`TransactionBodyV1`] deterministically.
    ///
    /// Principal `next_nonce` advances by `+1` **only** when [`IdentityDispositionV1::Admitted`]
    /// is returned; every rejection leaves registry state byte-unchanged.
    pub fn authorize_transaction(
        &mut self,
        tx: &TransactionBodyV1,
        signer_actor_id: ActorIdV1,
        preimage: &SigningPreimageV1,
        raw_signatures: &[([u8; 32], [u8; 64])],
        block_height: u64,
    ) -> IdentityAuthReceiptV1 {
        let principal_id = tx.actor_id;
        let is_delegated = signer_actor_id != principal_id;
        let action_kind = if is_delegated {
            IdentityActionKindV1::DelegatedTransaction
        } else {
            IdentityActionKindV1::DirectTransaction
        };
        let payload_digest = tx.tag().map(|t| t.digest()).unwrap_or([0u8; 32]);

        let Some(principal_snapshot) = self.actors.get(&principal_id).cloned() else {
            return Self::mk_receipt(
                action_kind,
                IdentityDispositionV1::RejectedUnknownActor,
                principal_id,
                signer_actor_id,
                0,
                0,
                0,
                block_height,
                payload_digest,
            );
        };
        let pre_nonce = principal_snapshot.next_nonce;
        let desc_ver = principal_snapshot.descriptor.descriptor_version;
        let reject = |disp| {
            Self::mk_receipt(
                action_kind,
                disp,
                principal_id,
                signer_actor_id,
                pre_nonce,
                pre_nonce,
                desc_ver,
                block_height,
                payload_digest,
            )
        };

        if tx.validate().is_err() {
            return reject(IdentityDispositionV1::RejectedValidityWindow);
        }
        if principal_snapshot.descriptor.revoked {
            return reject(IdentityDispositionV1::RejectedActorRevoked);
        }
        if !self.context.matches_preimage(
            preimage,
            SigningDomainTagV1::ActorTransaction,
            payload_digest,
        ) {
            return reject(IdentityDispositionV1::RejectedDomainBindingMismatch);
        }
        if block_height < tx.valid_after || block_height > tx.valid_until {
            return reject(IdentityDispositionV1::RejectedValidityWindow);
        }
        if let Some(expected_root) = tx.expected_state_root
            && expected_root != self.context.world_tag.digest()
        {
            return reject(IdentityDispositionV1::RejectedWorldRootPrecondition);
        }
        if tx.nonce != pre_nonce {
            return reject(IdentityDispositionV1::RejectedStaleOrFutureNonce);
        }

        let signing_policy = if !is_delegated {
            principal_snapshot.descriptor.action_policy.clone()
        } else {
            let Some(delegate_state) = self.actors.get(&signer_actor_id).cloned() else {
                return reject(IdentityDispositionV1::RejectedUnknownActor);
            };
            if delegate_state.descriptor.revoked {
                return reject(IdentityDispositionV1::RejectedActorRevoked);
            }
            match principal_snapshot
                .descriptor
                .delegations
                .iter()
                .find(|g| g.delegate_actor_id == signer_actor_id)
            {
                Some(grant) => {
                    if block_height > grant.expires_at_height {
                        return reject(IdentityDispositionV1::RejectedDelegationMissingOrExpired);
                    }
                    let scope_ok = tx.operations.iter().all(|op| match op {
                        NetworkEditOpV1::On(_) => grant.allow_on,
                        NetworkEditOpV1::Off(_) => grant.allow_off,
                    });
                    if !scope_ok {
                        return reject(IdentityDispositionV1::RejectedDelegationScopeDenied);
                    }
                    delegate_state.descriptor.action_policy
                }
                None => {
                    let is_transitive = principal_snapshot.descriptor.delegations.iter().any(|g| {
                        self.actors.get(&g.delegate_actor_id).is_some_and(|mid| {
                            mid.descriptor
                                .delegations
                                .iter()
                                .any(|sub| sub.delegate_actor_id == signer_actor_id)
                        })
                    });
                    if is_transitive {
                        return reject(IdentityDispositionV1::RejectedTransitiveDelegation);
                    }
                    return reject(IdentityDispositionV1::RejectedDelegationMissingOrExpired);
                }
            }
        };

        if let Err(disp) = self.verify_raw_threshold(
            &signing_policy,
            CredentialRoleV1::ActorAction,
            preimage,
            raw_signatures,
        ) {
            return reject(disp);
        }

        let Some(post_nonce) = pre_nonce.checked_add(1) else {
            return reject(IdentityDispositionV1::RejectedStaleOrFutureNonce);
        };
        self.actors
            .get_mut(&principal_id)
            .expect("actor exists")
            .next_nonce = post_nonce;

        Self::mk_receipt(
            action_kind,
            IdentityDispositionV1::Admitted,
            principal_id,
            signer_actor_id,
            pre_nonce,
            post_nonce,
            desc_ver,
            block_height,
            payload_digest,
        )
    }

    /// Evaluates an admin-signed in-place key/delegation rotation or permanent revocation.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_admin_rotation(
        &mut self,
        actor_id: ActorIdV1,
        next_action_policy: ThresholdPolicyV1,
        next_admin_policy: ThresholdPolicyV1,
        next_delegations: Vec<DelegationGrantV1>,
        revoke: bool,
        preimage: &SigningPreimageV1,
        raw_admin_signatures: &[([u8; 32], [u8; 64])],
        block_height: u64,
    ) -> IdentityAuthReceiptV1 {
        let action_kind = if revoke {
            IdentityActionKindV1::AdminRevocation
        } else {
            IdentityActionKindV1::AdminRotation
        };
        let Some(state) = self.actors.get(&actor_id).cloned() else {
            return Self::mk_receipt(
                action_kind,
                IdentityDispositionV1::RejectedUnknownActor,
                actor_id,
                actor_id,
                0,
                0,
                0,
                block_height,
                [0u8; 32],
            );
        };
        let next_desc = AuthorizationDescriptorV1 {
            descriptor_version: state.descriptor.descriptor_version.saturating_add(1),
            action_policy: next_action_policy,
            admin_policy: next_admin_policy,
            delegations: if revoke { Vec::new() } else { next_delegations },
            revoked: revoke,
        };
        let payload_digest = next_desc.tag().map(|t| t.digest()).unwrap_or([0u8; 32]);
        let reject = |disp| {
            Self::mk_receipt(
                action_kind,
                disp,
                actor_id,
                actor_id,
                state.next_nonce,
                state.next_nonce,
                state.descriptor.descriptor_version,
                block_height,
                payload_digest,
            )
        };
        if state.descriptor.revoked {
            return reject(IdentityDispositionV1::RejectedActorRevoked);
        }
        if next_desc.validate().is_err() {
            return reject(IdentityDispositionV1::RejectedInvalidOrDuplicateSignature);
        }
        for key in next_desc
            .action_policy
            .keys
            .iter()
            .chain(next_desc.admin_policy.keys.iter())
        {
            if self.registered_validator_keys.contains(key) {
                return reject(IdentityDispositionV1::RejectedValidatorKeyDomainCrossUse);
            }
        }
        if !self.context.matches_preimage(
            preimage,
            SigningDomainTagV1::ActorAdminRotation,
            payload_digest,
        ) {
            return reject(IdentityDispositionV1::RejectedDomainBindingMismatch);
        }
        if let Err(disp) = self.verify_raw_threshold(
            &state.descriptor.admin_policy,
            CredentialRoleV1::ActorAdmin,
            preimage,
            raw_admin_signatures,
        ) {
            return reject(disp);
        }
        let mut entries: Vec<(ActorIdV1, AuthorizationDescriptorV1)> = self
            .actors
            .iter()
            .map(|(id, st)| {
                if *id == actor_id {
                    (*id, next_desc.clone())
                } else {
                    (*id, st.descriptor.clone())
                }
            })
            .collect();
        entries.sort_by_key(|(id, _)| *id);
        if validate_actor_descriptor_set(&entries).is_err() {
            return reject(IdentityDispositionV1::RejectedDelegationScopeDenied);
        }
        let slot = self.actors.get_mut(&actor_id).expect("actor exists");
        slot.descriptor = next_desc;
        if revoke {
            slot.pending_recovery = None;
        }
        Self::mk_receipt(
            action_kind,
            IdentityDispositionV1::Admitted,
            actor_id,
            actor_id,
            slot.next_nonce,
            slot.next_nonce,
            slot.descriptor.descriptor_version,
            block_height,
            payload_digest,
        )
    }

    /// Step 1 of timelocked recovery: `recovery_guardians` initiate pending replacement descriptor.
    pub fn initiate_recovery(
        &mut self,
        actor_id: ActorIdV1,
        proposed_descriptor: AuthorizationDescriptorV1,
        preimage: &SigningPreimageV1,
        raw_guardian_signatures: &[([u8; 32], [u8; 64])],
        block_height: u64,
    ) -> IdentityAuthReceiptV1 {
        let payload_digest = proposed_descriptor
            .tag()
            .map(|t| t.digest())
            .unwrap_or([0u8; 32]);
        let Some(state) = self.actors.get(&actor_id).cloned() else {
            return Self::mk_receipt(
                IdentityActionKindV1::RecoveryInitiate,
                IdentityDispositionV1::RejectedUnknownActor,
                actor_id,
                actor_id,
                0,
                0,
                0,
                block_height,
                payload_digest,
            );
        };
        let reject = |disp| {
            Self::mk_receipt(
                IdentityActionKindV1::RecoveryInitiate,
                disp,
                actor_id,
                actor_id,
                state.next_nonce,
                state.next_nonce,
                state.descriptor.descriptor_version,
                block_height,
                payload_digest,
            )
        };
        if state.descriptor.revoked {
            return reject(IdentityDispositionV1::RejectedActorRevoked);
        }
        if state.pending_recovery.is_some()
            || proposed_descriptor.validate().is_err()
            || proposed_descriptor.revoked
        {
            return reject(IdentityDispositionV1::RejectedRecoveryStateInvalid);
        }
        for key in proposed_descriptor
            .action_policy
            .keys
            .iter()
            .chain(proposed_descriptor.admin_policy.keys.iter())
        {
            if self.registered_validator_keys.contains(key) {
                return reject(IdentityDispositionV1::RejectedValidatorKeyDomainCrossUse);
            }
        }
        if !self.context.matches_preimage(
            preimage,
            SigningDomainTagV1::ActorAdminRotation,
            payload_digest,
        ) {
            return reject(IdentityDispositionV1::RejectedDomainBindingMismatch);
        }
        if let Err(disp) = self.verify_raw_threshold(
            &state.recovery_guardians,
            CredentialRoleV1::ActorAdmin,
            preimage,
            raw_guardian_signatures,
        ) {
            return reject(disp);
        }
        let Some(unlock_height) = block_height.checked_add(state.recovery_delay_heights) else {
            return reject(IdentityDispositionV1::RejectedRecoveryStateInvalid);
        };
        let slot = self.actors.get_mut(&actor_id).expect("actor exists");
        slot.pending_recovery = Some(PendingRecoveryV1 {
            initiated_height: block_height,
            unlock_height,
            proposed_descriptor,
        });
        Self::mk_receipt(
            IdentityActionKindV1::RecoveryInitiate,
            IdentityDispositionV1::Admitted,
            actor_id,
            actor_id,
            slot.next_nonce,
            slot.next_nonce,
            slot.descriptor.descriptor_version,
            block_height,
            payload_digest,
        )
    }

    /// Optional Step 2a: active `ActorAdmin` cancels an open recovery before `unlock_height`.
    pub fn cancel_recovery(
        &mut self,
        actor_id: ActorIdV1,
        preimage: &SigningPreimageV1,
        raw_admin_signatures: &[([u8; 32], [u8; 64])],
        block_height: u64,
    ) -> IdentityAuthReceiptV1 {
        let Some(state) = self.actors.get(&actor_id).cloned() else {
            return Self::mk_receipt(
                IdentityActionKindV1::RecoveryCancel,
                IdentityDispositionV1::RejectedUnknownActor,
                actor_id,
                actor_id,
                0,
                0,
                0,
                block_height,
                [0u8; 32],
            );
        };
        let Some(pending) = &state.pending_recovery else {
            return Self::mk_receipt(
                IdentityActionKindV1::RecoveryCancel,
                IdentityDispositionV1::RejectedRecoveryStateInvalid,
                actor_id,
                actor_id,
                state.next_nonce,
                state.next_nonce,
                state.descriptor.descriptor_version,
                block_height,
                [0u8; 32],
            );
        };
        let payload_digest = pending
            .proposed_descriptor
            .tag()
            .map(|t| t.digest())
            .unwrap_or([0u8; 32]);
        let reject = |disp| {
            Self::mk_receipt(
                IdentityActionKindV1::RecoveryCancel,
                disp,
                actor_id,
                actor_id,
                state.next_nonce,
                state.next_nonce,
                state.descriptor.descriptor_version,
                block_height,
                payload_digest,
            )
        };
        if state.descriptor.revoked {
            return reject(IdentityDispositionV1::RejectedActorRevoked);
        }
        if !self.context.matches_preimage(
            preimage,
            SigningDomainTagV1::ActorAdminRotation,
            payload_digest,
        ) {
            return reject(IdentityDispositionV1::RejectedDomainBindingMismatch);
        }
        if let Err(disp) = self.verify_raw_threshold(
            &state.descriptor.admin_policy,
            CredentialRoleV1::ActorAdmin,
            preimage,
            raw_admin_signatures,
        ) {
            return reject(disp);
        }
        let slot = self.actors.get_mut(&actor_id).expect("actor exists");
        slot.pending_recovery = None;
        Self::mk_receipt(
            IdentityActionKindV1::RecoveryCancel,
            IdentityDispositionV1::Admitted,
            actor_id,
            actor_id,
            slot.next_nonce,
            slot.next_nonce,
            slot.descriptor.descriptor_version,
            block_height,
            payload_digest,
        )
    }

    /// Step 2b: finalizes a timelocked recovery at `block_height >= unlock_height`, preserving
    /// `ActorIdV1`, incrementing `descriptor_version`, and clearing all stale delegations.
    pub fn finalize_recovery(
        &mut self,
        actor_id: ActorIdV1,
        block_height: u64,
    ) -> IdentityAuthReceiptV1 {
        let Some(state) = self.actors.get(&actor_id).cloned() else {
            return Self::mk_receipt(
                IdentityActionKindV1::RecoveryFinalize,
                IdentityDispositionV1::RejectedUnknownActor,
                actor_id,
                actor_id,
                0,
                0,
                0,
                block_height,
                [0u8; 32],
            );
        };
        let Some(pending) = state.pending_recovery else {
            return Self::mk_receipt(
                IdentityActionKindV1::RecoveryFinalize,
                IdentityDispositionV1::RejectedRecoveryStateInvalid,
                actor_id,
                actor_id,
                state.next_nonce,
                state.next_nonce,
                state.descriptor.descriptor_version,
                block_height,
                [0u8; 32],
            );
        };
        let payload_digest = pending
            .proposed_descriptor
            .tag()
            .map(|t| t.digest())
            .unwrap_or([0u8; 32]);
        let reject = |disp| {
            Self::mk_receipt(
                IdentityActionKindV1::RecoveryFinalize,
                disp,
                actor_id,
                actor_id,
                state.next_nonce,
                state.next_nonce,
                state.descriptor.descriptor_version,
                block_height,
                payload_digest,
            )
        };
        if state.descriptor.revoked {
            return reject(IdentityDispositionV1::RejectedActorRevoked);
        }
        if block_height < pending.unlock_height {
            return reject(IdentityDispositionV1::RejectedRecoveryTimelockActive);
        }
        let mut finalized = pending.proposed_descriptor;
        finalized.descriptor_version = state.descriptor.descriptor_version.saturating_add(1);
        finalized.delegations.clear();
        finalized.revoked = false;
        let slot = self.actors.get_mut(&actor_id).expect("actor exists");
        slot.descriptor = finalized;
        slot.pending_recovery = None;
        Self::mk_receipt(
            IdentityActionKindV1::RecoveryFinalize,
            IdentityDispositionV1::Admitted,
            actor_id,
            actor_id,
            slot.next_nonce,
            slot.next_nonce,
            slot.descriptor.descriptor_version,
            block_height,
            payload_digest,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto_d1::sign_preimage_deterministic;
    use crate::history::world_tag;
    use crate::world::{Coord, World};

    fn mk_pk(seed_byte: u8) -> CanonicalEd25519PublicKeyV1 {
        CanonicalEd25519PublicKeyV1::from_secret_seed(&[seed_byte; 32]).unwrap()
    }

    fn mk_policy(role: CredentialRoleV1, seed_bytes: &[u8], threshold: u16) -> ThresholdPolicyV1 {
        let mut keys: Vec<_> = seed_bytes.iter().map(|&b| mk_pk(b)).collect();
        keys.sort();
        ThresholdPolicyV1 {
            role,
            threshold,
            keys,
        }
    }

    fn mk_context() -> IdentityContextV1 {
        IdentityContextV1 {
            protocol_version: 1,
            chain_id: 77,
            genesis_id: [0x11; 32],
            launch_manifest_id: [0x22; 32],
            profile_id: [0x33; 32],
            world_tag: world_tag(&World::new()),
            signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
            hash_suite: HashSuiteIdV1::Sha256V1,
        }
    }

    #[test]
    fn auth_state_and_receipt_roundtrip_and_depth1_recovery_flow() {
        let ctx = mk_context();
        let mut reg = IdentityRegistryV1::new(ctx);
        let desc_b = AuthorizationDescriptorV1 {
            descriptor_version: 0,
            action_policy: mk_policy(CredentialRoleV1::ActorAction, &[21], 1),
            admin_policy: mk_policy(CredentialRoleV1::ActorAdmin, &[22], 1),
            delegations: vec![],
            revoked: false,
        };
        let actor_b = ActorIdV1::derive(ctx.chain_id, "actor-b", &desc_b).unwrap();
        let desc_a = AuthorizationDescriptorV1 {
            descriptor_version: 0,
            action_policy: mk_policy(CredentialRoleV1::ActorAction, &[11], 1),
            admin_policy: mk_policy(CredentialRoleV1::ActorAdmin, &[12], 1),
            delegations: vec![DelegationGrantV1 {
                delegate_actor_id: actor_b,
                allow_on: true,
                allow_off: false,
                expires_at_height: 100,
            }],
            revoked: false,
        };
        let actor_a = ActorIdV1::derive(ctx.chain_id, "actor-a", &desc_a).unwrap();
        let state_a = ActorAuthStateV1 {
            state_version: 1,
            actor_id: actor_a,
            descriptor: desc_a,
            next_nonce: 0,
            recovery_guardians: mk_policy(CredentialRoleV1::ActorAdmin, &[13], 1),
            recovery_delay_heights: 100,
            pending_recovery: None,
        };
        let state_b = ActorAuthStateV1 {
            state_version: 1,
            actor_id: actor_b,
            descriptor: desc_b,
            next_nonce: 0,
            recovery_guardians: mk_policy(CredentialRoleV1::ActorAdmin, &[23], 1),
            recovery_delay_heights: 100,
            pending_recovery: None,
        };
        let enc_a = state_a.encode_canonical().unwrap();
        assert_eq!(ActorAuthStateV1::decode_canonical(&enc_a).unwrap(), state_a);
        reg.register_actor(state_b).unwrap();
        reg.register_actor(state_a).unwrap();

        let tx = TransactionBodyV1 {
            actor_id: actor_a,
            nonce: 0,
            valid_after: 1,
            valid_until: 50,
            expected_state_root: Some(ctx.world_tag.digest()),
            max_fee: 100,
            operations: vec![NetworkEditOpV1::On(Coord::new(1, 0, 0))],
        };
        let pre = ctx.build_preimage(
            SigningDomainTagV1::ActorTransaction,
            tx.tag().unwrap().digest(),
        );
        let sig_a = sign_preimage_deterministic(&[11; 32], &pre).unwrap();
        let rcpt = reg.authorize_transaction(
            &tx,
            actor_a,
            &pre,
            &[(*mk_pk(11).as_bytes(), *sig_a.as_bytes())],
            10,
        );
        assert_eq!(rcpt.disposition, IdentityDispositionV1::Admitted);
        assert_eq!(rcpt.post_nonce, 1);
        let rcpt_enc = rcpt.encode_canonical().unwrap();
        assert_eq!(
            IdentityAuthReceiptV1::decode_canonical(&rcpt_enc).unwrap(),
            rcpt
        );
    }
}
