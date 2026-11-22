//! `IDENTITY-D1`: stable actor identities, role-separated `m-of-n` threshold policies,
//! non-cyclic scoped delegations, sequential nonces, and explicit `On`/`Off` network batches.
//!
//! Freezes the pre-genesis actor and authorization schema selected under `IDENTITY-D1`:
//! - **Stable `ActorIdV1`:** derived pre-genesis from `(chain_id, identity_namespace,
//!   initial_authorization_descriptor)` so genesis descriptors carry zero circular dependency on
//!   `genesis_id`.
//! - **Committed `AuthorizationDescriptorV1`:** carries separate `ActorAction` and `ActorAdmin`
//!   threshold policies, bounded non-cyclic scoped delegations, and in-place versioned rotation
//!   or revocation while preserving `ActorIdV1`.
//! - **Explicit wire actions (`NetworkEditOpV1`):** lowers `toggle` out of the network layer,
//!   admitting only explicit `On(Coord)` and `Off(Coord)` inside bounded atomic multi-op batches
//!   with sequential nonces, height validity windows, and optional expected-state preconditions.
//!   Observer camera pose and reach remain strictly local presentation state.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use sha2::{Digest, Sha256};

use crate::crypto_d1::{
    CanonicalEd25519PublicKeyV1, CanonicalEd25519SignatureV1, CryptoD1Error, SigningDomainTagV1,
    SigningPreimageV1, verify_preimage_strict,
};
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::world::Coord;

/// Canonical schema string for an [`AuthorizationDescriptorV1`] policy object.
pub const IDENTITY_D1_AUTH_DESCRIPTOR_SCHEMA: &[u8] =
    b"bitblock/identity-d1/authorization-descriptor/v1";
/// Canonical schema string for a [`TransactionBodyV1`] network action batch.
pub const IDENTITY_D1_TX_BODY_SCHEMA: &[u8] = b"bitblock/identity-d1/transaction-body/v1";

/// Maximum number of canonical public keys in a single [`ThresholdPolicyV1`].
pub const MAX_THRESHOLD_KEYS: usize = 32;
/// Maximum number of active delegations attached to one [`AuthorizationDescriptorV1`].
pub const MAX_DELEGATIONS_PER_ACTOR: usize = 16;
/// Maximum number of explicit `On`/`Off` operations in one atomic [`TransactionBodyV1`].
pub const MAX_TX_BATCH_OPS: usize = 256;

/// Errors returned by `IDENTITY-D1` descriptor validation, rotation, or batch decoding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdentityD1Error {
    /// Cryptographic validation failed in the underlying `CRYPTO-D1` layer.
    Crypto(CryptoD1Error),
    /// Unknown credential role wire discriminant.
    UnknownCredentialRole(u8),
    /// Threshold policy violated `1 <= threshold <= keys.len() <= MAX_THRESHOLD_KEYS`.
    InvalidThreshold { threshold: u16, key_count: usize },
    /// Public keys in a threshold policy were not strictly sorted and unique.
    UnsortedOrDuplicateKey,
    /// Credential role did not match the authority slot where it appeared.
    RoleMismatch {
        expected: CredentialRoleV1,
        actual: CredentialRoleV1,
    },
    /// Delegation list exceeded cap, was unsorted/duplicate, or granted empty permissions.
    InvalidDelegation(&'static str),
    /// Actor delegated to itself or formed a delegation cycle across the actor set.
    DelegationCycleDetected,
    /// Initial actor set contained duplicate or unsorted actor IDs.
    UnsortedOrDuplicateActor,
    /// Identity namespace was empty or exceeded 64 UTF-8 bytes.
    InvalidIdentityNamespace,
    /// Transaction body had malformed framing, invalid validity window, or out-of-range batch size.
    InvalidTransactionBody(&'static str),
    /// Sequential actor nonce mismatch.
    NonceMismatch { expected: u64, actual: u64 },
    /// Authorization descriptor is revoked and cannot authorize actions or rotations.
    DescriptorRevoked,
    /// Threshold signature check lacked enough valid distinct signers from the required policy.
    InsufficientValidSignatures { required: u16, valid: usize },
    /// Wire buffer ended prematurely or contained trailing unconsumed bytes.
    InvalidWireFraming(&'static str),
}

impl From<CryptoD1Error> for IdentityD1Error {
    fn from(value: CryptoD1Error) -> Self {
        Self::Crypto(value)
    }
}

impl fmt::Display for IdentityD1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for IdentityD1Error {}

/// Strictly separated credential roles preventing authority namespace conflation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum CredentialRoleV1 {
    /// Authorizes ordinary actor world/asset actions.
    ActorAction = 0,
    /// Authorizes actor key rotation, delegation updates, and revocation.
    ActorAdmin = 1,
    /// Authorizes validator consensus votes.
    ValidatorVote = 2,
    /// Authorizes validator block proposals.
    ValidatorProposal = 3,
    /// Authorizes timelocked governance proposals and approvals.
    GovernanceSigner = 4,
    /// Authorizes pause-only emergency security council halts.
    EmergencyPauseSigner = 5,
    /// Authorizes reproducible release build attestations.
    ReleaseSigner = 6,
}

impl CredentialRoleV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte credential role discriminant.
    pub const fn from_code(code: u8) -> Result<Self, IdentityD1Error> {
        match code {
            0 => Ok(Self::ActorAction),
            1 => Ok(Self::ActorAdmin),
            2 => Ok(Self::ValidatorVote),
            3 => Ok(Self::ValidatorProposal),
            4 => Ok(Self::GovernanceSigner),
            5 => Ok(Self::EmergencyPauseSigner),
            6 => Ok(Self::ReleaseSigner),
            other => Err(IdentityD1Error::UnknownCredentialRole(other)),
        }
    }
}

/// Canonical `m-of-n` threshold signature policy bound to one [`CredentialRoleV1`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ThresholdPolicyV1 {
    /// Exclusive credential role enforced for every key in this policy.
    pub role: CredentialRoleV1,
    /// Minimum distinct valid signatures required (`1 <= threshold <= keys.len()`).
    pub threshold: u16,
    /// Strictly sorted, duplicate-free canonical Ed25519 public keys.
    pub keys: Vec<CanonicalEd25519PublicKeyV1>,
}

impl ThresholdPolicyV1 {
    /// Validates threshold bounds, key count cap, and strict ascending uniqueness.
    pub fn validate(&self, expected_role: CredentialRoleV1) -> Result<(), IdentityD1Error> {
        if self.role != expected_role {
            return Err(IdentityD1Error::RoleMismatch {
                expected: expected_role,
                actual: self.role,
            });
        }
        if self.keys.is_empty()
            || self.keys.len() > MAX_THRESHOLD_KEYS
            || self.threshold == 0
            || usize::from(self.threshold) > self.keys.len()
        {
            return Err(IdentityD1Error::InvalidThreshold {
                threshold: self.threshold,
                key_count: self.keys.len(),
            });
        }
        for pair in self.keys.windows(2) {
            if pair[0] >= pair[1] {
                return Err(IdentityD1Error::UnsortedOrDuplicateKey);
            }
        }
        Ok(())
    }

    /// Appends canonical big-endian bytes of this threshold policy into `out`.
    pub fn encode_into(&self, out: &mut Vec<u8>) -> Result<(), IdentityD1Error> {
        self.validate(self.role)?;
        out.push(self.role.code());
        out.extend_from_slice(&self.threshold.to_be_bytes());
        out.extend_from_slice(&(self.keys.len() as u16).to_be_bytes());
        for key in &self.keys {
            out.extend_from_slice(key.as_bytes());
        }
        Ok(())
    }

    /// Decodes a canonical `ThresholdPolicyV1` from `bytes` starting at `*cursor`.
    pub fn decode_from(
        bytes: &[u8],
        cursor: &mut usize,
        expected_role: CredentialRoleV1,
    ) -> Result<Self, IdentityD1Error> {
        if bytes.len().saturating_sub(*cursor) < 5 {
            return Err(IdentityD1Error::InvalidWireFraming(
                "truncated threshold policy header",
            ));
        }
        let role = CredentialRoleV1::from_code(bytes[*cursor])?;
        let threshold = u16::from_be_bytes([bytes[*cursor + 1], bytes[*cursor + 2]]);
        let key_count = u16::from_be_bytes([bytes[*cursor + 3], bytes[*cursor + 4]]) as usize;
        *cursor += 5;
        if key_count == 0 || key_count > MAX_THRESHOLD_KEYS {
            return Err(IdentityD1Error::InvalidThreshold {
                threshold,
                key_count,
            });
        }
        let need = key_count * 32;
        if bytes.len().saturating_sub(*cursor) < need {
            return Err(IdentityD1Error::InvalidWireFraming(
                "truncated threshold policy keys",
            ));
        }
        let mut keys = Vec::with_capacity(key_count);
        for _ in 0..key_count {
            let pk = CanonicalEd25519PublicKeyV1::from_slice(&bytes[*cursor..*cursor + 32])?;
            keys.push(pk);
            *cursor += 32;
        }
        let policy = Self {
            role,
            threshold,
            keys,
        };
        policy.validate(expected_role)?;
        Ok(policy)
    }

    /// Verifies that `signatures` supplies at least `self.threshold` valid distinct signers from
    /// `self.keys` over `preimage`.
    pub fn verify_threshold_signatures(
        &self,
        expected_role: CredentialRoleV1,
        preimage: &SigningPreimageV1,
        signatures: &[(CanonicalEd25519PublicKeyV1, CanonicalEd25519SignatureV1)],
    ) -> Result<(), IdentityD1Error> {
        self.validate(expected_role)?;
        let mut seen = BTreeSet::new();
        for (pk, sig) in signatures {
            if self.keys.binary_search(pk).is_err() {
                return Err(IdentityD1Error::InsufficientValidSignatures {
                    required: self.threshold,
                    valid: seen.len(),
                });
            }
            if !seen.insert(*pk) {
                return Err(IdentityD1Error::UnsortedOrDuplicateKey);
            }
            verify_preimage_strict(pk, preimage, sig)?;
        }
        if seen.len() < usize::from(self.threshold) {
            return Err(IdentityD1Error::InsufficientValidSignatures {
                required: self.threshold,
                valid: seen.len(),
            });
        }
        Ok(())
    }
}

/// Stable 32-byte actor identifier derived pre-genesis without depending on `genesis_id`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ActorIdV1([u8; 32]);

impl ActorIdV1 {
    /// Derives a stable `ActorIdV1` from `chain_id`, `identity_namespace`, and the initial
    /// canonical [`AuthorizationDescriptorV1`].
    pub fn derive(
        chain_id: u64,
        identity_namespace: &str,
        initial_descriptor: &AuthorizationDescriptorV1,
    ) -> Result<Self, IdentityD1Error> {
        let ns = identity_namespace.as_bytes();
        if ns.is_empty() || ns.len() > 64 {
            return Err(IdentityD1Error::InvalidIdentityNamespace);
        }
        let desc_bytes = initial_descriptor.encode_canonical()?;
        let mut h = Sha256::new();
        h.update(b"bitblock/actor-identity/v1");
        h.update(chain_id.to_be_bytes());
        h.update((ns.len() as u32).to_be_bytes());
        h.update(ns);
        h.update((desc_bytes.len() as u32).to_be_bytes());
        h.update(&desc_bytes);
        Ok(Self(h.finalize().into()))
    }

    /// Wraps raw 32 bytes as an `ActorIdV1`.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the raw 32-byte identifier.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Scoped, bounded delegation granting another actor explicit `On` and/or `Off` capability until
/// `expires_at_height`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DelegationGrantV1 {
    /// Target delegate actor identifier.
    pub delegate_actor_id: ActorIdV1,
    /// Grants `On` operation capability when `true`.
    pub allow_on: bool,
    /// Grants `Off` operation capability when `true`.
    pub allow_off: bool,
    /// Inclusive last ledger height at which this delegation is active.
    pub expires_at_height: u64,
}

impl DelegationGrantV1 {
    /// Validates that at least one action permission is granted.
    pub fn validate(&self) -> Result<(), IdentityD1Error> {
        if !self.allow_on && !self.allow_off {
            return Err(IdentityD1Error::InvalidDelegation(
                "delegation must grant at least one action",
            ));
        }
        Ok(())
    }
}

/// Committed authorization descriptor stored in `A_h[actor_id]`, supporting key rotation and
/// revocation without changing `actor_id`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AuthorizationDescriptorV1 {
    /// Monotonic descriptor revision (`0` at genesis/creation).
    pub descriptor_version: u64,
    /// Action signing threshold policy (`CredentialRoleV1::ActorAction`).
    pub action_policy: ThresholdPolicyV1,
    /// Admin rotation/revocation threshold policy (`CredentialRoleV1::ActorAdmin`).
    pub admin_policy: ThresholdPolicyV1,
    /// Strictly sorted unique delegated capabilities (max `MAX_DELEGATIONS_PER_ACTOR`).
    pub delegations: Vec<DelegationGrantV1>,
    /// Terminal revocation flag preventing future actions or rotations.
    pub revoked: bool,
}

impl AuthorizationDescriptorV1 {
    /// Validates policies, role bindings, delegation cap, and canonical sorting.
    pub fn validate(&self) -> Result<(), IdentityD1Error> {
        self.action_policy.validate(CredentialRoleV1::ActorAction)?;
        self.admin_policy.validate(CredentialRoleV1::ActorAdmin)?;
        if self.delegations.len() > MAX_DELEGATIONS_PER_ACTOR {
            return Err(IdentityD1Error::InvalidDelegation(
                "delegation count exceeds MAX_DELEGATIONS_PER_ACTOR",
            ));
        }
        for grant in &self.delegations {
            grant.validate()?;
        }
        for pair in self.delegations.windows(2) {
            if pair[0].delegate_actor_id >= pair[1].delegate_actor_id {
                return Err(IdentityD1Error::InvalidDelegation(
                    "delegations must be strictly sorted by unique delegate_actor_id",
                ));
            }
        }
        Ok(())
    }

    /// Encodes the descriptor into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, IdentityD1Error> {
        self.validate()?;
        let mut out = Vec::new();
        out.extend_from_slice(&self.descriptor_version.to_be_bytes());
        self.action_policy.encode_into(&mut out)?;
        self.admin_policy.encode_into(&mut out)?;
        out.extend_from_slice(&(self.delegations.len() as u16).to_be_bytes());
        for g in &self.delegations {
            out.extend_from_slice(g.delegate_actor_id.as_bytes());
            out.push((u8::from(g.allow_on)) | (u8::from(g.allow_off) << 1));
            out.extend_from_slice(&g.expires_at_height.to_be_bytes());
        }
        out.push(u8::from(self.revoked));
        Ok(out)
    }

    /// Decodes and validates a canonical `AuthorizationDescriptorV1`.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, IdentityD1Error> {
        if bytes.len() < 8 {
            return Err(IdentityD1Error::InvalidWireFraming(
                "truncated descriptor version",
            ));
        }
        let descriptor_version = u64::from_be_bytes(bytes[0..8].try_into().expect("8 bytes"));
        let mut cursor = 8;
        let action_policy =
            ThresholdPolicyV1::decode_from(bytes, &mut cursor, CredentialRoleV1::ActorAction)?;
        let admin_policy =
            ThresholdPolicyV1::decode_from(bytes, &mut cursor, CredentialRoleV1::ActorAdmin)?;
        if bytes.len().saturating_sub(cursor) < 2 {
            return Err(IdentityD1Error::InvalidWireFraming(
                "truncated delegation count",
            ));
        }
        let del_count = u16::from_be_bytes([bytes[cursor], bytes[cursor + 1]]) as usize;
        cursor += 2;
        if del_count > MAX_DELEGATIONS_PER_ACTOR {
            return Err(IdentityD1Error::InvalidDelegation(
                "delegation count exceeds MAX_DELEGATIONS_PER_ACTOR",
            ));
        }
        let mut delegations = Vec::with_capacity(del_count);
        for _ in 0..del_count {
            if bytes.len().saturating_sub(cursor) < 41 {
                return Err(IdentityD1Error::InvalidWireFraming(
                    "truncated delegation grant",
                ));
            }
            let id_bytes: [u8; 32] = bytes[cursor..cursor + 32].try_into().expect("32 bytes");
            let flags = bytes[cursor + 32];
            if flags & !0x03 != 0 {
                return Err(IdentityD1Error::InvalidDelegation(
                    "unknown delegation permission bits set",
                ));
            }
            let expires_at_height =
                u64::from_be_bytes(bytes[cursor + 33..cursor + 41].try_into().expect("8 bytes"));
            cursor += 41;
            delegations.push(DelegationGrantV1 {
                delegate_actor_id: ActorIdV1::from_bytes(id_bytes),
                allow_on: (flags & 0x01) != 0,
                allow_off: (flags & 0x02) != 0,
                expires_at_height,
            });
        }
        if bytes.len().saturating_sub(cursor) != 1 {
            return Err(IdentityD1Error::InvalidWireFraming(
                "missing revoked byte or trailing bytes",
            ));
        }
        let revoked = match bytes[cursor] {
            0 => false,
            1 => true,
            _ => {
                return Err(IdentityD1Error::InvalidWireFraming(
                    "revoked flag must be 0 or 1",
                ));
            }
        };
        let desc = Self {
            descriptor_version,
            action_policy,
            admin_policy,
            delegations,
            revoked,
        };
        desc.validate()?;
        Ok(desc)
    }

    /// Computes the canonical `ContentTag` for this authorization descriptor.
    pub fn tag(&self) -> Result<ContentTag, IdentityD1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            IDENTITY_D1_AUTH_DESCRIPTOR_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and bounded length"))
    }

    /// Rotates action/admin policies or revokes the descriptor after verifying `admin_policy`
    /// signatures over the next descriptor digest, preserving `ActorIdV1`.
    pub fn apply_authorized_rotation(
        &self,
        next_action_policy: ThresholdPolicyV1,
        next_admin_policy: ThresholdPolicyV1,
        next_delegations: Vec<DelegationGrantV1>,
        revoke: bool,
        mut preimage_template: SigningPreimageV1,
        admin_signatures: &[(CanonicalEd25519PublicKeyV1, CanonicalEd25519SignatureV1)],
    ) -> Result<Self, IdentityD1Error> {
        if self.revoked {
            return Err(IdentityD1Error::DescriptorRevoked);
        }
        let next = Self {
            descriptor_version: self.descriptor_version.saturating_add(1),
            action_policy: next_action_policy,
            admin_policy: next_admin_policy,
            delegations: next_delegations,
            revoked: revoke,
        };
        let next_tag = next.tag()?;
        preimage_template.domain_tag = SigningDomainTagV1::ActorAdminRotation;
        preimage_template.payload_digest = next_tag.digest();
        self.admin_policy.verify_threshold_signatures(
            CredentialRoleV1::ActorAdmin,
            &preimage_template,
            admin_signatures,
        )?;
        Ok(next)
    }
}

/// Validates a pre-genesis sorted unique actor descriptor set and rejects any self-delegation
/// or directed delegation cycle.
pub fn validate_actor_descriptor_set(
    entries: &[(ActorIdV1, AuthorizationDescriptorV1)],
) -> Result<(), IdentityD1Error> {
    let mut graph: BTreeMap<ActorIdV1, Vec<ActorIdV1>> = BTreeMap::new();
    for pair in entries.windows(2) {
        if pair[0].0 >= pair[1].0 {
            return Err(IdentityD1Error::UnsortedOrDuplicateActor);
        }
    }
    for (actor_id, desc) in entries {
        desc.validate()?;
        let mut edges = Vec::with_capacity(desc.delegations.len());
        for g in &desc.delegations {
            if g.delegate_actor_id == *actor_id {
                return Err(IdentityD1Error::DelegationCycleDetected);
            }
            edges.push(g.delegate_actor_id);
        }
        graph.insert(*actor_id, edges);
    }
    // 3-color DFS cycle check across the delegation graph.
    let mut state: BTreeMap<ActorIdV1, u8> = BTreeMap::new();
    for &start in graph.keys() {
        if state.get(&start).copied().unwrap_or(0) != 0 {
            continue;
        }
        let mut stack = vec![(start, 0usize)];
        state.insert(start, 1);
        while let Some((node, idx)) = stack.last_mut() {
            let neighbors = graph.get(node).map(Vec::as_slice).unwrap_or(&[]);
            if *idx < neighbors.len() {
                let next = neighbors[*idx];
                *idx += 1;
                match state.get(&next).copied().unwrap_or(0) {
                    1 => return Err(IdentityD1Error::DelegationCycleDetected),
                    0 => {
                        state.insert(next, 1);
                        stack.push((next, 0));
                    }
                    _ => {}
                }
            } else {
                state.insert(*node, 2);
                stack.pop();
            }
        }
    }
    Ok(())
}

/// Sequential per-actor replay protection state stored in `N_h[actor_id]`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ActorNonceV1 {
    /// Next expected exact sequential transaction nonce (`0` initially).
    pub next_nonce: u64,
}

impl ActorNonceV1 {
    /// Verifies `submitted == self.next_nonce` and advances `next_nonce` by `1`.
    pub fn consume(&mut self, submitted: u64) -> Result<(), IdentityD1Error> {
        if submitted != self.next_nonce {
            return Err(IdentityD1Error::NonceMismatch {
                expected: self.next_nonce,
                actual: submitted,
            });
        }
        self.next_nonce = self
            .next_nonce
            .checked_add(1)
            .ok_or(IdentityD1Error::InvalidTransactionBody("nonce overflow"))?;
        Ok(())
    }
}

/// Canonical network wire edit operation: explicit `On` or `Off` only (no `Toggle` on wire).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NetworkEditOpV1 {
    /// Explicitly place/set coordinate occupancy to `1`.
    On(Coord),
    /// Explicitly clear/burn coordinate occupancy to `0`.
    Off(Coord),
}

impl NetworkEditOpV1 {
    /// Fixed canonical wire byte length (`25` bytes: 1-byte op code + 3×`i64` BE).
    pub const WIRE_LEN: usize = 25;

    /// Encodes this operation into canonical big-endian bytes.
    pub fn encode_canonical(self) -> [u8; Self::WIRE_LEN] {
        let (code, coord) = match self {
            Self::On(c) => (0u8, c),
            Self::Off(c) => (1u8, c),
        };
        let mut out = [0u8; Self::WIRE_LEN];
        out[0] = code;
        out[1..9].copy_from_slice(&coord.x.to_be_bytes());
        out[9..17].copy_from_slice(&coord.y.to_be_bytes());
        out[17..25].copy_from_slice(&coord.z.to_be_bytes());
        out
    }

    /// Decodes a canonical `NetworkEditOpV1` (rejecting any discriminant other than `0` or `1`).
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, IdentityD1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(IdentityD1Error::InvalidWireFraming(
                "invalid network edit op length",
            ));
        }
        let coord = Coord::new(
            i64::from_be_bytes(bytes[1..9].try_into().expect("8 bytes")),
            i64::from_be_bytes(bytes[9..17].try_into().expect("8 bytes")),
            i64::from_be_bytes(bytes[17..25].try_into().expect("8 bytes")),
        );
        match bytes[0] {
            0 => Ok(Self::On(coord)),
            1 => Ok(Self::Off(coord)),
            _ => Err(IdentityD1Error::InvalidTransactionBody(
                "wire edit op must be explicit On (0) or Off (1); Toggle is forbidden",
            )),
        }
    }
}

/// Canonical bounded atomic multi-op transaction body (`IDENTITY-D1` wire contract).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TransactionBodyV1 {
    /// Stable actor identifier authorizing this batch.
    pub actor_id: ActorIdV1,
    /// Exact sequential actor nonce.
    pub nonce: u64,
    /// Inclusive earliest ledger height at which this batch is admissible.
    pub valid_after: u64,
    /// Inclusive latest ledger height at which this batch is admissible.
    pub valid_until: u64,
    /// Optional expected parent state root precondition.
    pub expected_state_root: Option<[u8; 32]>,
    /// Maximum native fee offered for atomic batch admission.
    pub max_fee: u128,
    /// Bounded non-empty ordered sequence of explicit `On`/`Off` actions.
    pub operations: Vec<NetworkEditOpV1>,
}

impl TransactionBodyV1 {
    /// Validates batch bounds and validity window invariants.
    pub fn validate(&self) -> Result<(), IdentityD1Error> {
        if self.valid_after > self.valid_until {
            return Err(IdentityD1Error::InvalidTransactionBody(
                "valid_after must be <= valid_until",
            ));
        }
        if self.operations.is_empty() || self.operations.len() > MAX_TX_BATCH_OPS {
            return Err(IdentityD1Error::InvalidTransactionBody(
                "operations count must be within 1..=MAX_TX_BATCH_OPS",
            ));
        }
        Ok(())
    }

    /// Encodes the transaction body into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, IdentityD1Error> {
        self.validate()?;
        let mut out = Vec::with_capacity(
            32 + 8 + 8 + 8 + 1 + 32 + 16 + 2 + self.operations.len() * NetworkEditOpV1::WIRE_LEN,
        );
        out.extend_from_slice(self.actor_id.as_bytes());
        out.extend_from_slice(&self.nonce.to_be_bytes());
        out.extend_from_slice(&self.valid_after.to_be_bytes());
        out.extend_from_slice(&self.valid_until.to_be_bytes());
        match self.expected_state_root {
            None => out.push(0),
            Some(root) => {
                out.push(1);
                out.extend_from_slice(&root);
            }
        }
        out.extend_from_slice(&self.max_fee.to_be_bytes());
        out.extend_from_slice(&(self.operations.len() as u16).to_be_bytes());
        for &op in &self.operations {
            out.extend_from_slice(&op.encode_canonical());
        }
        Ok(out)
    }

    /// Decodes and validates a canonical `TransactionBodyV1`.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, IdentityD1Error> {
        if bytes.len() < 57 {
            return Err(IdentityD1Error::InvalidWireFraming(
                "truncated transaction body header",
            ));
        }
        let actor_id = ActorIdV1::from_bytes(bytes[0..32].try_into().expect("32 bytes"));
        let nonce = u64::from_be_bytes(bytes[32..40].try_into().expect("8 bytes"));
        let valid_after = u64::from_be_bytes(bytes[40..48].try_into().expect("8 bytes"));
        let valid_until = u64::from_be_bytes(bytes[48..56].try_into().expect("8 bytes"));
        let mut cursor = 56;
        let expected_state_root = match bytes[cursor] {
            0 => {
                cursor += 1;
                None
            }
            1 => {
                cursor += 1;
                if bytes.len().saturating_sub(cursor) < 32 {
                    return Err(IdentityD1Error::InvalidWireFraming(
                        "truncated expected_state_root",
                    ));
                }
                let root: [u8; 32] = bytes[cursor..cursor + 32].try_into().expect("32 bytes");
                cursor += 32;
                Some(root)
            }
            _ => {
                return Err(IdentityD1Error::InvalidWireFraming(
                    "invalid option discriminant for expected_state_root",
                ));
            }
        };
        if bytes.len().saturating_sub(cursor) < 18 {
            return Err(IdentityD1Error::InvalidWireFraming(
                "truncated max_fee or op_count",
            ));
        }
        let max_fee = u128::from_be_bytes(bytes[cursor..cursor + 16].try_into().expect("16 bytes"));
        let op_count = u16::from_be_bytes([bytes[cursor + 16], bytes[cursor + 17]]) as usize;
        cursor += 18;
        if op_count == 0 || op_count > MAX_TX_BATCH_OPS {
            return Err(IdentityD1Error::InvalidTransactionBody(
                "operations count must be within 1..=MAX_TX_BATCH_OPS",
            ));
        }
        let expected_remaining = op_count * NetworkEditOpV1::WIRE_LEN;
        if bytes.len().saturating_sub(cursor) != expected_remaining {
            return Err(IdentityD1Error::InvalidWireFraming(
                "operation bytes length mismatch or trailing bytes",
            ));
        }
        let mut operations = Vec::with_capacity(op_count);
        for _ in 0..op_count {
            let op = NetworkEditOpV1::decode_canonical(
                &bytes[cursor..cursor + NetworkEditOpV1::WIRE_LEN],
            )?;
            operations.push(op);
            cursor += NetworkEditOpV1::WIRE_LEN;
        }
        let body = Self {
            actor_id,
            nonce,
            valid_after,
            valid_until,
            expected_state_root,
            max_fee,
            operations,
        };
        body.validate()?;
        Ok(body)
    }

    /// Computes the canonical `ContentTag` for this transaction body.
    pub fn tag(&self) -> Result<ContentTag, IdentityD1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Intent,
            IDENTITY_D1_TX_BODY_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and bounded length"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto_d1::{HashSuiteIdV1, SignatureSuiteIdV1, sign_preimage_deterministic};

    fn test_pk(seed_byte: u8) -> CanonicalEd25519PublicKeyV1 {
        CanonicalEd25519PublicKeyV1::from_secret_seed(&[seed_byte; 32]).unwrap()
    }

    #[test]
    fn stable_actor_id_rotation_and_delegation_cycles() {
        let mut action_keys = vec![test_pk(1), test_pk(2)];
        action_keys.sort();
        let admin_pk = test_pk(3);
        let desc = AuthorizationDescriptorV1 {
            descriptor_version: 0,
            action_policy: ThresholdPolicyV1 {
                role: CredentialRoleV1::ActorAction,
                threshold: 2,
                keys: action_keys,
            },
            admin_policy: ThresholdPolicyV1 {
                role: CredentialRoleV1::ActorAdmin,
                threshold: 1,
                keys: vec![admin_pk],
            },
            delegations: vec![],
            revoked: false,
        };
        let bytes = desc.encode_canonical().unwrap();
        assert_eq!(
            AuthorizationDescriptorV1::decode_canonical(&bytes).unwrap(),
            desc
        );
        let actor_id = ActorIdV1::derive(1, "bitblock-mainnet-v1", &desc).unwrap();

        // Rotate action key while preserving actor_id.
        let new_action = ThresholdPolicyV1 {
            role: CredentialRoleV1::ActorAction,
            threshold: 1,
            keys: vec![test_pk(4)],
        };
        let candidate_next = AuthorizationDescriptorV1 {
            descriptor_version: 1,
            action_policy: new_action.clone(),
            admin_policy: desc.admin_policy.clone(),
            delegations: vec![],
            revoked: false,
        };
        let pre_template = SigningPreimageV1 {
            protocol_version: 1,
            chain_id: 1,
            genesis_id: [0xaa; 32],
            launch_manifest_id: [0xbb; 32],
            profile_id: [0xcc; 32],
            domain_tag: SigningDomainTagV1::ActorAdminRotation,
            signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
            hash_suite: HashSuiteIdV1::Sha256V1,
            payload_digest: candidate_next.tag().unwrap().digest(),
        };
        let admin_sig = sign_preimage_deterministic(&[3u8; 32], &pre_template).unwrap();
        let rotated = desc
            .apply_authorized_rotation(
                new_action,
                desc.admin_policy.clone(),
                vec![],
                false,
                pre_template,
                &[(admin_pk, admin_sig)],
            )
            .unwrap();
        assert_eq!(rotated.descriptor_version, 1);

        // Two-actor delegation cycle must reject.
        let id_a = actor_id;
        let id_b = ActorIdV1::from_bytes([0x99; 32]);
        let mut da = desc.clone();
        da.delegations.push(DelegationGrantV1 {
            delegate_actor_id: id_b,
            allow_on: true,
            allow_off: false,
            expires_at_height: 100,
        });
        let mut db = desc.clone();
        db.delegations.push(DelegationGrantV1 {
            delegate_actor_id: id_a,
            allow_on: false,
            allow_off: true,
            expires_at_height: 100,
        });
        let mut set = vec![(id_a, da), (id_b, db)];
        set.sort_by_key(|(id, _)| *id);
        assert_eq!(
            validate_actor_descriptor_set(&set),
            Err(IdentityD1Error::DelegationCycleDetected)
        );
    }

    #[test]
    fn tx_body_rejects_toggle_and_enforces_nonce() {
        let body = TransactionBodyV1 {
            actor_id: ActorIdV1::from_bytes([0x07; 32]),
            nonce: 0,
            valid_after: 10,
            valid_until: 20,
            expected_state_root: Some([0x55; 32]),
            max_fee: 1_000,
            operations: vec![
                NetworkEditOpV1::On(Coord::new(1, 2, 3)),
                NetworkEditOpV1::Off(Coord::new(-4, 5, -6)),
            ],
        };
        let raw = body.encode_canonical().unwrap();
        assert_eq!(TransactionBodyV1::decode_canonical(&raw).unwrap(), body);

        // Corrupt op discriminant to 2 (Toggle) -> must fail closed.
        let mut toggle_raw = raw.clone();
        let op_offset = raw.len() - NetworkEditOpV1::WIRE_LEN;
        toggle_raw[op_offset] = 2;
        assert!(matches!(
            TransactionBodyV1::decode_canonical(&toggle_raw),
            Err(IdentityD1Error::InvalidTransactionBody(_))
        ));

        let mut nonce = ActorNonceV1::default();
        nonce.consume(0).unwrap();
        assert_eq!(
            nonce.consume(0),
            Err(IdentityD1Error::NonceMismatch {
                expected: 1,
                actual: 0
            })
        );
    }
}
